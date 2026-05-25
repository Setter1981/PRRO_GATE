//! TCP listener for a single `fiscal_number`.
//!
//! Owns the port, accepts one connection at a time, enforces the
//! 3-second post-disconnect cooldown, and spawns one `run_connection`
//! task per accepted socket.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use super::cooldown::Cooldown;
use super::exclusion::ConnectionGate;
use super::session_loop::{run_connection, ClockSource, DEFAULT_IDLE_TIMEOUT};

use crate::bridge::Bridge;
use crate::observability::SessionMetrics;
use crate::protocol::{error_codes::ErrorCode, Response};
use crate::session::dispatcher::Identity;

/// Errors raised by the listener pipeline.
#[derive(Debug, thiserror::Error)]
pub enum ListenerError {
    #[error("listener I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Runtime configuration for one FN's listener.
#[derive(Debug, Clone)]
pub struct ListenerConfig {
    pub fiscal_number: String,
    pub bind: String,
    pub identity: Identity,
    pub idle_timeout: Duration,
    pub cooldown: Duration,
}

impl ListenerConfig {
    #[must_use]
    pub fn new(fiscal_number: impl Into<String>, bind: impl Into<String>, identity: Identity) -> Self {
        Self {
            fiscal_number: fiscal_number.into(),
            bind: bind.into(),
            identity,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            cooldown: Duration::from_secs(3),
        }
    }
}

/// A running listener.  Call `serve` to drive it.
pub struct FnListener {
    cfg: ListenerConfig,
    gate: Arc<ConnectionGate>,
    cooldown: Arc<Cooldown>,
    bridge: Arc<dyn Bridge + Send + Sync>,
    clock: Arc<dyn ClockSource>,
    metrics: Arc<SessionMetrics>,
}

impl FnListener {
    pub fn new(
        cfg: ListenerConfig,
        bridge: Arc<dyn Bridge + Send + Sync>,
        clock: Arc<dyn ClockSource>,
        metrics: Arc<SessionMetrics>,
    ) -> Self {
        let cooldown = Arc::new(Cooldown::new(cfg.cooldown));
        Self {
            cfg,
            gate: Arc::new(ConnectionGate::new()),
            cooldown,
            bridge,
            clock,
            metrics,
        }
    }

    /// Expose the gate — admin API (M10) reads this to list active sessions.
    #[must_use]
    pub fn gate(&self) -> Arc<ConnectionGate> {
        Arc::clone(&self.gate)
    }

    /// Spawn the accept loop.  Runs until cancelled or a fatal I/O error.
    ///
    /// # Errors
    /// Returns if binding fails or the accept loop hits an unrecoverable error.
    pub async fn serve(self, shutdown: tokio_util::sync::CancellationToken) -> Result<(), ListenerError> {
        let tcp = TcpListener::bind(&self.cfg.bind).await?;
        tracing::info!(
            fn_id = %self.cfg.fiscal_number,
            addr = %self.cfg.bind,
            "maria304 listener bound",
        );
        let mut connections: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accept_res = tcp.accept() => {
                    let (stream, peer) = accept_res?;
                    let gate = Arc::clone(&self.gate);
                    let cooldown = Arc::clone(&self.cooldown);
                    let bridge = Arc::clone(&self.bridge);
                    let clock = Arc::clone(&self.clock);
                    let identity = Arc::new(self.cfg.identity.clone());
                    let idle = self.cfg.idle_timeout;
                    let conn_shutdown = shutdown.clone();

                    let metrics = Arc::clone(&self.metrics);
                    connections.spawn(async move {
                        handle_incoming(stream, peer, gate, cooldown, bridge, clock, identity, idle, metrics, conn_shutdown).await;
                    });
                }
                () = shutdown.cancelled() => {
                    tracing::info!(fiscal_number = %self.cfg.fiscal_number, "listener shutdown requested");
                    break;
                }
            }
        }
        // Drain in-flight connections.  Each connection's shutdown token is already
        // cancelled, so bridge calls / IO will unwind to their next cancel point.
        while connections.join_next().await.is_some() {}
        Ok(())
    }
}

/// RAII guard — releases the gate and starts the cooldown timer no
/// matter how `handle_incoming` exits: normal return, I/O error, or
/// panic inside the inner async task.  A plain sequential
/// `gate.release()` at the end of `handle_incoming` would leak the
/// gate forever if anything above it panicked, making the FN
/// permanently unreachable until the process restarts.
struct SlotGuard {
    gate: Arc<ConnectionGate>,
    cooldown: Arc<Cooldown>,
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.gate.release();
        self.cooldown.start_now();
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_incoming(
    mut stream: TcpStream,
    peer: std::net::SocketAddr,
    gate: Arc<ConnectionGate>,
    cooldown: Arc<Cooldown>,
    bridge: Arc<dyn Bridge + Send + Sync>,
    clock: Arc<dyn ClockSource>,
    identity: Arc<Identity>,
    idle: Duration,
    metrics: Arc<SessionMetrics>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    if !cooldown.is_clear() {
        tracing::info!(%peer, "reject: cooldown active");
        reject_with_softblock(&mut stream).await;
        return;
    }
    if !gate.try_acquire() {
        tracing::info!(%peer, "reject: gate busy (duplicate connect)");
        reject_with_softblock(&mut stream).await;
        return;
    }
    // Only AFTER we have successfully acquired the slot do we arm
    // the drop guard — otherwise a reject path would mistakenly
    // release something that was never held.
    let _slot = SlotGuard { gate, cooldown };
    let session_uuid = session_uuid();
    tracing::info!(%peer, uuid = %session_uuid, "session accepted");
    let run = run_connection(stream, identity, bridge, clock, session_uuid.clone(), idle, metrics, shutdown).await;
    if let Err(e) = &run {
        tracing::warn!(%peer, uuid = %session_uuid, "session ended: {e}");
    } else {
        tracing::info!(%peer, uuid = %session_uuid, "session ended");
    }
    // _slot drops here — equivalent to the previous explicit
    // release()+start_now(), but now also runs during unwind.
}

/// Emit a `SOFTBLOCK` frame without CRC (CRC is not yet enabled at
/// the TCP accept boundary) then close the stream.
async fn reject_with_softblock(stream: &mut TcpStream) {
    let resp = Response::Error(ErrorCode::SoftBlock);
    if let Ok(bytes) = resp.to_wire(false) {
        let _ = stream.write_all(&bytes).await;
    }
    let _ = stream.shutdown().await;
}

/// Minimal session uuid — monotonically increasing per process.
/// M10 replaces with a real uuid crate.
fn session_uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sess-{n:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_config_uses_safe_defaults() {
        let cfg = ListenerConfig::new("FN", "127.0.0.1:0", Identity::default());
        assert_eq!(cfg.idle_timeout, DEFAULT_IDLE_TIMEOUT);
        assert_eq!(cfg.cooldown, Duration::from_secs(3));
    }

    #[test]
    fn session_uuid_is_monotonic() {
        let a = session_uuid();
        let b = session_uuid();
        let c = session_uuid();
        assert_ne!(a, b);
        assert_ne!(b, c);
    }
}
