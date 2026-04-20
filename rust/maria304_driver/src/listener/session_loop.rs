//! Per-connection driver loop.
//!
//! Reads wire frames from the TCP stream, feeds them to the
//! dispatcher, serialises each response frame back to the stream.
//! Exits cleanly on disconnect, CRC failure, or idle timeout.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Instant};

use crate::bridge::Bridge;
use crate::protocol::{Command, Response};
use crate::session::dispatcher::{dispatch, Clock, Correlation, Identity};
use crate::session::Session;
use crate::wire::{decode_frame, FrameError};

/// Default idle timeout — if the client is silent for this long we
/// close the socket.  Tunable per plan §config.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Provider for the current clock values (date/time).  Real sessions
/// sample system time; tests pin a fixed value.
pub trait ClockSource: Send + Sync {
    fn now(&self) -> (String, String);
}

/// Trivial zero-state clock — always "20260101" / "000000".  Mostly
/// for tests; M13 replaces with a chrono-backed impl.
#[derive(Debug, Default)]
pub struct FixedClock;

impl ClockSource for FixedClock {
    fn now(&self) -> (String, String) {
        ("20260101".to_string(), "000000".to_string())
    }
}

/// Run the per-connection dispatch loop.  Returns when the socket is
/// closed (EOF / error / idle timeout).
///
/// # Errors
/// I/O errors are propagated.  Protocol-level errors (bad CRC,
/// malformed frame) are handled in-stream — the loop keeps going.
pub async fn run_connection(
    mut stream: TcpStream,
    identity: Arc<Identity>,
    bridge: Arc<dyn Bridge + Send + Sync>,
    clock_src: Arc<dyn ClockSource>,
    session_uuid: String,
    idle_timeout: Duration,
) -> std::io::Result<()> {
    let mut session = Session::new();
    let mut correlation = Correlation {
        session_uuid,
        receipt_seq: 0,
    };
    let mut buf = Vec::with_capacity(512);
    let mut scratch = [0u8; 1024];

    loop {
        // Capture the CRC flag BEFORE dispatch runs.  CSIN1 flips
        // the flag during its dispatch arm, but its own response
        // (DONE + READY) must be sent using the flag value that was
        // active when the incoming CSIN1 was parsed — otherwise the
        // client, which read CSIN1's response with CRC=off, would
        // see extra CRC bytes as junk bytes on the wire.
        let crc_for_write = session.crc_enabled;
        if let Some(responses) =
            try_handle_buffered(&mut buf, &mut session, &identity, &bridge, &*clock_src, &mut correlation)
        {
            for resp in responses {
                write_response(&mut stream, &resp, crc_for_write).await?;
            }
            continue;
        }
        let read_at = Instant::now();
        let n = match timeout(idle_timeout, stream.read(&mut scratch)).await {
            Ok(Ok(0)) => {
                tracing::debug!("client closed after {:?}", read_at.elapsed());
                return Ok(());
            }
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                tracing::warn!("read error: {e}");
                return Err(e);
            }
            Err(_) => {
                tracing::info!("idle timeout {idle_timeout:?}");
                return Ok(());
            }
        };
        buf.extend_from_slice(&scratch[..n]);
    }
}

/// Try to decode + dispatch one frame from the buffer.  Returns
/// `Some(responses)` when a full frame was consumed, `None` when more
/// bytes are needed.  On decode errors we consume enough bytes to
/// resync and continue.
fn try_handle_buffered(
    buf: &mut Vec<u8>,
    session: &mut Session,
    identity: &Identity,
    bridge: &Arc<dyn Bridge + Send + Sync>,
    clock_src: &dyn ClockSource,
    correlation: &mut Correlation,
) -> Option<Vec<Response>> {
    if buf.is_empty() {
        return None;
    }
    match decode_frame(buf, session.crc_enabled) {
        Ok((frame, consumed)) => {
            buf.drain(..consumed);
            let (date, time) = clock_src.now();
            let clock = Clock { date: &date, time: &time };
            let command = Command::parse(&frame);
            let responses = dispatch(session, command, identity, clock, &**bridge, correlation);
            Some(responses)
        }
        Err(FrameError::Empty | FrameError::Incomplete) => None,
        Err(FrameError::MissingStart) => {
            // Drop one byte and retry — the wire is out of sync.
            buf.drain(..1);
            // Return empty so caller re-tries the buffer on the
            // next iteration without reading more bytes.
            Some(Vec::new())
        }
        Err(FrameError::BadCrc) => {
            // Drop one byte and surface the wire error on the
            // outbound side so the client can request a resend.
            buf.drain(..1);
            Some(vec![Response::Error(
                crate::protocol::error_codes::ErrorCode::SoftBadCs,
            )])
        }
        Err(FrameError::NoFrameFound) => {
            // Buffer is definitely corrupt — flush it entirely.
            buf.clear();
            Some(Vec::new())
        }
        Err(FrameError::InvalidCmdLen(_)) => {
            // Cannot happen on decode path (codec enforces 4..=252),
            // but surface as SOFT error just in case.
            buf.drain(..1);
            Some(vec![Response::Error(
                crate::protocol::error_codes::ErrorCode::SoftBlock,
            )])
        }
    }
}

async fn write_response(
    stream: &mut TcpStream,
    resp: &Response,
    with_crc: bool,
) -> std::io::Result<()> {
    let bytes = match resp.to_wire(with_crc) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("unable to encode response {resp:?}: {e}");
            return Ok(()); // skip but don't kill the session
        }
    };
    stream.write_all(&bytes).await?;
    stream.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::MockBridge;

    #[tokio::test]
    async fn fresh_buffer_returns_none() {
        let mut buf = Vec::new();
        let mut s = Session::new();
        let id = Identity::default();
        let bridge: Arc<dyn Bridge + Send + Sync> = Arc::new(MockBridge::new());
        let clock = FixedClock;
        let mut corr = Correlation { session_uuid: "s".to_string(), receipt_seq: 0 };
        let out = try_handle_buffered(&mut buf, &mut s, &id, &bridge, &clock, &mut corr);
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn partial_frame_in_buffer_returns_none() {
        let mut buf = vec![0xFD, b'C', b'S']; // 3 bytes — incomplete
        let mut s = Session::new();
        let id = Identity::default();
        let bridge: Arc<dyn Bridge + Send + Sync> = Arc::new(MockBridge::new());
        let clock = FixedClock;
        let mut corr = Correlation { session_uuid: "s".to_string(), receipt_seq: 0 };
        let out = try_handle_buffered(&mut buf, &mut s, &id, &bridge, &clock, &mut corr);
        assert!(out.is_none());
        assert_eq!(buf.len(), 3, "partial buffer must be preserved");
    }

    #[tokio::test]
    async fn complete_csin1_frame_is_consumed_and_acknowledged() {
        let mut buf = crate::wire::encode_frame("CSIN1", false).unwrap();
        let mut s = Session::new();
        let id = Identity::default();
        let bridge: Arc<dyn Bridge + Send + Sync> = Arc::new(MockBridge::new());
        let clock = FixedClock;
        let mut corr = Correlation { session_uuid: "s".to_string(), receipt_seq: 0 };
        let out = try_handle_buffered(&mut buf, &mut s, &id, &bridge, &clock, &mut corr)
            .expect("a full frame should dispatch");
        assert_eq!(out.len(), 2); // DONE + READY
        assert!(buf.is_empty(), "buffer must be drained after consuming the frame");
        assert!(s.crc_enabled);
    }

    #[tokio::test]
    async fn missing_start_byte_advances_one_byte_and_keeps_session_alive() {
        let mut buf = vec![0x00, 0x01]; // junk prefix
        let mut s = Session::new();
        let id = Identity::default();
        let bridge: Arc<dyn Bridge + Send + Sync> = Arc::new(MockBridge::new());
        let clock = FixedClock;
        let mut corr = Correlation { session_uuid: "s".to_string(), receipt_seq: 0 };
        let out = try_handle_buffered(&mut buf, &mut s, &id, &bridge, &clock, &mut corr);
        assert_eq!(out, Some(Vec::new()));
        assert_eq!(buf.len(), 1, "single junk byte should be dropped");
    }

    #[tokio::test]
    async fn back_to_back_frames_each_consume_one_frame_per_call() {
        // Use two SYNC frames (no CSIN toggle) so session.crc_enabled
        // stays `false` for both decodes.  A real client sends CSIN1
        // then waits for DONE+READY before enabling CRC on its side —
        // pipelining frames across the toggle is protocol violation.
        let a = crate::wire::encode_frame("SYNC", false).unwrap();
        let b = crate::wire::encode_frame("SYNC", false).unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(&a);
        buf.extend_from_slice(&b);
        let id = Identity::default();
        let bridge: Arc<dyn Bridge + Send + Sync> = Arc::new(MockBridge::new());
        let clock = FixedClock;
        let mut corr = Correlation { session_uuid: "s".to_string(), receipt_seq: 0 };
        let mut s = Session::new();

        let r1 = try_handle_buffered(&mut buf, &mut s, &id, &bridge, &clock, &mut corr).unwrap();
        assert_eq!(r1.len(), 2); // DONE + READY
        let r2 = try_handle_buffered(&mut buf, &mut s, &id, &bridge, &clock, &mut corr).unwrap();
        assert_eq!(r2.len(), 2); // DONE + READY
        assert!(buf.is_empty());
    }

}
