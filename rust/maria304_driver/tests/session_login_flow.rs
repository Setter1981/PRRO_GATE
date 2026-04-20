//! Sprint M3 integration acceptance — full OLE Manager login flow.
//!
//! Replays the exact command sequence the Resonance OLE DLL emits
//! when `Init(port, cashier, password)` runs against a real Maria 304:
//! `CSIN1` → `SYNC` → `UPAS<pwd><cashier>` → `CONf`.  Every step must
//! produce well-formed wire bytes that our own decoder can round-trip,
//! and the resulting `CONf` payload must reflect the session state.

use maria304_driver::protocol::Command;
use maria304_driver::session::{dispatch, Clock, Identity, Session, SessionState};
use maria304_driver::wire::{decode_frame, encode_frame, Frame};

fn handshake_flow() -> (Session, Identity, Clock<'static>) {
    (
        Session::new(),
        Identity::default(),
        Clock { date: "20260420", time: "101530" },
    )
}

/// Helper — run a single dispatch round-trip through the wire layer
/// and return the decoded response text(s).  Asserts every response
/// frame is self-consistent (self-verifying CRC when `with_crc`).
fn exchange(
    session: &mut Session,
    identity: &Identity,
    clock: Clock<'_>,
    wire_cmd: &str,
    with_crc: bool,
) -> Vec<String> {
    // Encode the client command, decode it (proves the framer round-
    // trips), parse it, dispatch, then encode every response and
    // decode each one to prove the full pipe is clean.
    let bytes = encode_frame(wire_cmd, with_crc).expect("valid wire command");
    let (Frame { text, .. }, consumed) = decode_frame(&bytes, with_crc).unwrap();
    assert_eq!(consumed, bytes.len(), "framer must consume entire request");
    assert_eq!(text, wire_cmd);

    let cmd = Command::parse_text(&text);
    let responses = dispatch(session, cmd, identity, clock);

    let mut decoded = Vec::with_capacity(responses.len());
    for r in responses {
        let bytes = r.to_wire(with_crc).unwrap();
        let (frame, n) = decode_frame(&bytes, with_crc).unwrap();
        assert_eq!(n, bytes.len());
        decoded.push(frame.text);
    }
    decoded
}

#[test]
fn ole_manager_init_command_sequence_byte_for_byte() {
    let (mut session, identity, clock) = handshake_flow();

    // Step 1 — CSIN1 (enable CRC).  Before this, CRC is disabled.
    let out = exchange(&mut session, &identity, clock, "CSIN1", false);
    assert_eq!(out, vec!["DONE".to_string(), "READY".to_string()]);
    assert!(session.crc_enabled);

    // Step 2 — SYNC with CRC.
    let out = exchange(&mut session, &identity, clock, "SYNC", true);
    assert_eq!(out, vec!["DONE".to_string(), "READY".to_string()]);

    // Step 3 — UPAS<pwd><cashier>.  Default password from Identity.
    let out = exchange(
        &mut session,
        &identity,
        clock,
        "UPAS1111111111Кассир",
        true,
    );
    assert_eq!(out, vec!["DONE".to_string(), "READY".to_string()]);
    assert_eq!(session.state, SessionState::Authenticated);
    assert_eq!(session.cashier_id.as_deref(), Some("Кассир"));

    // Step 4 — CONf (device state).  The decoded payload must start
    // with "CONf" and be exactly 4 + 148 = 152 chars total.
    let out = exchange(&mut session, &identity, clock, "CONf", true);
    assert_eq!(out.len(), 3);
    assert!(out[0].starts_with("CONf"), "got {}", out[0]);
    assert_eq!(out[0].chars().count(), 4 + 148);
    assert_eq!(out[1], "DONE");
    assert_eq!(out[2], "READY");
}

#[test]
fn unknown_opcode_responds_with_done_not_error() {
    // Defensive default per plan doc §M3 acceptance: unknown 4-byte
    // opcodes produce DONE so 1C's polling does not abort.  This
    // specifically protects against "driver firmware version drift"
    // where a newer OLE Manager uses opcodes we don't model yet.
    let (mut session, identity, clock) = handshake_flow();

    // Get past login first.
    exchange(&mut session, &identity, clock, "CSIN1", false);
    exchange(
        &mut session,
        &identity,
        clock,
        "UPAS1111111111Casher",
        true,
    );

    let out = exchange(&mut session, &identity, clock, "XYZA", true);
    assert_eq!(out, vec!["DONE".to_string(), "READY".to_string()]);
}

#[test]
fn conf_payload_crc_self_verifies_on_the_wire() {
    use maria304_driver::wire::crc16;

    let (mut session, identity, clock) = handshake_flow();

    // Login with CRC on.
    exchange(&mut session, &identity, clock, "CSIN1", false);
    exchange(
        &mut session,
        &identity,
        clock,
        "UPAS1111111111Cshr",
        true,
    );

    let cmd_bytes = encode_frame("CONf", true).unwrap();
    assert_eq!(
        crc16(&cmd_bytes),
        0,
        "request frame must self-verify",
    );
    let (Frame { text, .. }, _) = decode_frame(&cmd_bytes, true).unwrap();
    let responses = dispatch(&mut session, Command::parse_text(&text), &identity, clock);

    // Every response frame must self-verify end-to-end.
    for resp in responses {
        let bytes = resp.to_wire(true).unwrap();
        assert_eq!(crc16(&bytes), 0, "response {resp:?} must self-verify");
    }
}

#[test]
fn receipt_full_lifecycle_through_wire() {
    let (mut session, identity, clock) = handshake_flow();

    exchange(&mut session, &identity, clock, "CSIN1", false);
    exchange(
        &mut session,
        &identity,
        clock,
        "UPAS1111111111Cshr",
        true,
    );

    // Open receipt.
    let out = exchange(&mut session, &identity, clock, "PREPBar1", true);
    assert_eq!(out, vec!["DONE".to_string(), "READY".to_string()]);
    assert!(session.receipt_open());

    // Receipt-building commands — each replies DONE/READY in M3.
    // M4 replaces this with real line accumulation.
    for cmd in ["FISCgoods1-1000-100", "PSDt1022MERCHANTTERM", "ACLD03ABC"] {
        let out = exchange(&mut session, &identity, clock, cmd, true);
        assert_eq!(
            out,
            vec!["DONE".to_string(), "READY".to_string()],
            "receipt command {cmd} must succeed",
        );
    }
    assert_eq!(session.psdt_sequence, 1);

    // Close receipt — COMP replies Data + DONE + READY.
    let out = exchange(&mut session, &identity, clock, "COMPsum", true);
    assert_eq!(out.len(), 3);
    assert!(out[0].starts_with("COMP"));
    assert_eq!(out[1], "DONE");
    assert_eq!(out[2], "READY");
    assert!(!session.receipt_open());
}
