//! Sprint M6 acceptance — real TCP end-to-end.
//!
//! Binds an actual TCP port, spawns the listener, connects a client,
//! drives the handshake, asserts that:
//!
//!   * A valid command sequence receives well-formed wire responses.
//!   * A second connect while the first is active receives SOFTBLOCK
//!     and is closed.
//!   * Post-disconnect cooldown rejects an immediate reconnect.
//!   * Multi-FN scenario: two listeners on two ports work independently.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use maria304_driver::bridge::{Bridge, MockBridge};
use maria304_driver::listener::session_loop::{ClockSource, FixedClock};
use maria304_driver::listener::{FnListener, ListenerConfig};
use maria304_driver::session::dispatcher::Identity;
use maria304_driver::wire::{decode_frame, encode_frame};

/// Bind an ephemeral port and return `(bound_addr, listener_handle)`.
async fn spawn_listener(fn_id: &str, cooldown: Duration) -> (String, Arc<MockBridge>) {
    let bridge = Arc::new(MockBridge::new());
    let bridge_trait: Arc<dyn Bridge + Send + Sync> = bridge.clone();
    let clock: Arc<dyn ClockSource> = Arc::new(FixedClock);

    // Ask the OS for an available port by binding to :0, then reuse
    // the same port string for the listener — FnListener will rebind
    // internally.  The tiny race between `drop(probe)` and the
    // listener's `bind` is mitigated by a longer settle sleep below.
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let mut cfg = ListenerConfig::new(fn_id.to_string(), addr.to_string(), Identity::default());
    cfg.cooldown = cooldown;
    cfg.idle_timeout = Duration::from_secs(2);

    let listener = FnListener::new(cfg, bridge_trait, clock);
    tokio::spawn(async move {
        if let Err(e) = listener.serve().await {
            eprintln!("listener died: {e}");
        }
    });

    // Give the listener time to bind its socket.  Probing via connect
    // would consume the single-connection slot, so we just sleep.
    tokio::time::sleep(Duration::from_millis(200)).await;
    (addr.to_string(), bridge)
}

/// Tiny helper around a persistent receive buffer so multiple
/// `read_frame` calls on the same stream don't lose bytes between
/// frames delivered in a single TCP read.
struct Reader {
    pub buf: Vec<u8>,
    scratch: [u8; 128],
}

impl Reader {
    fn new() -> Self {
        Self { buf: Vec::with_capacity(64), scratch: [0u8; 128] }
    }

    async fn next_frame(&mut self, stream: &mut TcpStream) -> String {
        loop {
            if let Ok((frame, consumed)) = decode_frame(&self.buf, false) {
                self.buf.drain(..consumed);
                return frame.text;
            }
            match timeout(Duration::from_millis(2000), stream.read(&mut self.scratch)).await {
                Ok(Ok(0)) => panic!("server closed unexpectedly, buf={:02X?}", self.buf),
                Ok(Ok(n)) => self.buf.extend_from_slice(&self.scratch[..n]),
                Ok(Err(e)) => panic!("read error: {e}"),
                Err(tokio::time::error::Elapsed { .. }) => {
                    panic!("timed out waiting for frame, buf so far: {:02X?}", self.buf)
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn valid_sync_receives_done_ready_over_real_tcp() {
    // Uses SYNC (not CSIN1) to keep CRC disabled — the toggle
    // semantic is tested separately in the crc_toggle test.
    let (addr, _bridge) = spawn_listener("FN-A", Duration::from_millis(10)).await;
    let mut client = TcpStream::connect(&addr).await.unwrap();
    let mut reader = Reader::new();

    let req = encode_frame("SYNC", false).unwrap();
    client.write_all(&req).await.unwrap();
    let r1 = reader.next_frame(&mut client).await;
    assert_eq!(r1, "DONE");
    let r2 = reader.next_frame(&mut client).await;
    assert_eq!(r2, "READY");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn second_concurrent_connect_receives_softblock_and_is_closed() {
    let (addr, _bridge) = spawn_listener("FN-B", Duration::from_secs(10)).await;

    // First client — keeps the FN slot busy.
    let mut first = TcpStream::connect(&addr).await.unwrap();
    let mut first_reader = Reader::new();
    // Push one handshake byte so the listener definitely moves past accept.
    let probe = encode_frame("SYNC", false).unwrap();
    first.write_all(&probe).await.unwrap();
    let _ = first_reader.next_frame(&mut first).await; // DONE
    let _ = first_reader.next_frame(&mut first).await; // READY

    // Second concurrent client — should be rejected with SOFTBLOCK.
    let mut second = TcpStream::connect(&addr).await.unwrap();
    let mut second_reader = Reader::new();
    let rejected = second_reader.next_frame(&mut second).await;
    assert_eq!(rejected, "SOFTBLOCK", "duplicate connect must see SOFTBLOCK");

    // Server closes the second socket after rejecting.
    let mut tail = [0u8; 16];
    let n = timeout(Duration::from_millis(500), second.read(&mut tail))
        .await
        .expect("server should close second socket quickly")
        .unwrap_or(0);
    assert_eq!(n, 0, "server must close the rejected socket");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_disconnect_cooldown_rejects_immediate_reconnect() {
    // Large cooldown so the test is deterministic.
    let (addr, _bridge) = spawn_listener("FN-C", Duration::from_secs(2)).await;

    // First session opens + closes immediately.
    let first = TcpStream::connect(&addr).await.unwrap();
    drop(first);

    // Give the listener a tick to observe the disconnect.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Immediate reconnect during cooldown — SOFTBLOCK expected.
    let mut second = TcpStream::connect(&addr).await.unwrap();
    let mut second_reader = Reader::new();
    let rejected = second_reader.next_frame(&mut second).await;
    assert_eq!(rejected, "SOFTBLOCK");

    // After cooldown elapses, a fresh connect succeeds.
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let mut third = TcpStream::connect(&addr).await.unwrap();
    let mut third_reader = Reader::new();
    let req = encode_frame("SYNC", false).unwrap();
    third.write_all(&req).await.unwrap();
    let r = third_reader.next_frame(&mut third).await;
    assert_eq!(r, "DONE");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn csin1_response_is_sent_without_crc_matching_request_mode() {
    // Protocol semantics: a CSIN1 request is sent with CRC=off, and
    // its DONE/READY response MUST also be without CRC — the toggle
    // takes effect starting from the NEXT command.  The original
    // M6 draft flipped the flag before writing CSIN1's own response,
    // which made the client read extra CRC bytes as junk.
    let (addr, _bridge) = spawn_listener("FN-CRC", Duration::from_millis(10)).await;
    let mut client = TcpStream::connect(&addr).await.unwrap();
    let mut reader = Reader::new();

    client.write_all(&encode_frame("CSIN1", false).unwrap()).await.unwrap();
    assert_eq!(reader.next_frame(&mut client).await, "DONE");
    assert_eq!(reader.next_frame(&mut client).await, "READY");
    // reader.buf must be empty — if the server had sent CRC bytes
    // the reader would have 2 leftover bytes that decode_frame
    // cannot interpret.
    assert!(
        reader.buf.is_empty(),
        "CSIN1 response must not carry trailing CRC bytes, got {:02X?}",
        reader.buf,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_fns_on_two_ports_operate_independently() {
    let (addr_a, _) = spawn_listener("FN-X", Duration::from_millis(10)).await;
    let (addr_b, _) = spawn_listener("FN-Y", Duration::from_millis(10)).await;
    assert_ne!(addr_a, addr_b, "each FN must bind to a unique port");

    // Connect both simultaneously — no cross-interference.
    let mut a = TcpStream::connect(&addr_a).await.unwrap();
    let mut b = TcpStream::connect(&addr_b).await.unwrap();
    let mut ra = Reader::new();
    let mut rb = Reader::new();

    a.write_all(&encode_frame("SYNC", false).unwrap()).await.unwrap();
    b.write_all(&encode_frame("SYNC", false).unwrap()).await.unwrap();

    assert_eq!(ra.next_frame(&mut a).await, "DONE");
    assert_eq!(rb.next_frame(&mut b).await, "DONE");
    assert_eq!(ra.next_frame(&mut a).await, "READY");
    assert_eq!(rb.next_frame(&mut b).await, "READY");
}
