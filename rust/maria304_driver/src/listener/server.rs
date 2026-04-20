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
}

impl FnListener {
    pub fn new(
        cfg: ListenerConfig,
        bridge: Arc<dyn Bridge + Send + Sync>,
        clock: Arc<dyn ClockSource>,
    ) -> Self {
        let cooldown = Arc::new(Cooldown::new(cfg.cooldown));
        Self {
            cfg,
            gate: Arc::new(ConnectionGate::new()),
            cooldown,
            bridge,
            clock,
        }
    }

    /// Expose the gate — admin API (M10) reads this to list active sessions.
    #[must_use]
    pub fn gate(&self) -> Arc<ConnectionGate> {
        Arc::clone(&self.gate)
    }

    /// Spawn the accept loop.  Runs until the listener is dropped.
    ///
    /// # Errors
    /// Returns if binding fails or the accept loop hits an unrecoverable error.
    pub async fn serve(self) -> Result<(), ListenerError> {
        let tcp = TcpListener::bind(&self.cfg.bind).await?;
        tracing::info!(
            fn_id = %self.cfg.fiscal_number,
            addr = %self.cfg.bind,
            "maria304 listener bound",
        );
        loop {
            let (stream, peer) = tcp.accept().await?;
            let gate = Arc::clone(&self.gate);
            let cooldown = Arc::clone(&self.cooldown);
            let bridge = Arc::clone(&self.bridge);
            let clock = Arc::clone(&self.clock);
            let identity = Arc::new(self.cfg.identity.clone());
            let idle = self.cfg.idle_timeout;

            tokio::spawn(async move {
                handle_incoming(stream, peer, gate, cooldown, bridge, clock, identity, idle).await;
            });
        }
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
    let session_uuid = session_uuid();
    tracing::info!(%peer, uuid = %session_uuid, "session accepted");
    let run = run_connection(stream, identity, bridge, clock, session_uuid.clone(), idle).await;
    if let Err(e) = &run {
        tracing::warn!(%peer, uuid = %session_uuid, "session ended: {e}");
    } else {
        tracing::info!(%peer, uuid = %session_uuid, "session ended");
    }
    gate.release();
    cooldown.start_now();
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
