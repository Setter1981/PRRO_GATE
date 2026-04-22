//! Sprint M4 integration acceptance — canonical envelope posted
//! to the mock bridge on COMP.
//!
//! Plan doc §M4 requires: "Full SELL flow lands a canonical envelope
//! on the mock bridge with every field populated from wire commands".

use maria304_driver::bridge::{BridgeError, CanonicalCommand, CommandType, MockBridge};
use maria304_driver::bridge::dto::CanonicalResponse;
use maria304_driver::protocol::error_codes::ErrorCode;
use maria304_driver::protocol::{Command, Response};
use maria304_driver::session::dispatcher::Correlation;
use maria304_driver::session::{dispatch, Clock, Identity, Session};

fn clock() -> Clock<'static> {
    Clock { date: "20260420", time: "101530" }
}

fn run(session: &mut Session, bridge: &MockBridge, correlation: &mut Correlation, cmd: Command) -> Vec<Response> {
    dispatch(session, cmd, &Identity::default(), clock(), bridge, correlation)
}

fn logged_in_session() -> (Session, MockBridge, Correlation) {
    let mut s = Session::new();
    let b = MockBridge::new();
    let mut c = Correlation { session_uuid: "sess-test".to_string(), receipt_seq: 0 };
    run(
        &mut s,
        &b,
        &mut c,
        Command::Upas {
            password: "1111111111".to_string(),
            cashier_id: "csh1".to_string(),
        },
    );
    (s, b, c)
}

#[test]
fn sell_with_items_payments_slip_lands_canonical_envelope_on_bridge() {
    let (mut session, bridge, mut correlation) = logged_in_session();

    // Open receipt in "Bar1" department.
    run(&mut session, &bridge, &mut correlation, Command::Prep("Bar1".to_string()));

    // Three fiscal lines — bodies are opaque to Rust, the Python
    // adapter does the rich parse.
    for body in [
        "goods_line_1",
        "goods_line_2",
        "goods_line_3",
    ] {
        run(&mut session, &bridge, &mut correlation, Command::Fisc(body.to_string()));
    }

    // Dual-tax mode — alcohol (А + Б).
    run(&mut session, &bridge, &mut correlation, Command::Nlpr { tax1_char: 'А', tax2_char: 'Б' });

    // One excise-stamp batch.
    run(&mut session, &bridge, &mut correlation, Command::Acld("03STAMP_001".to_string()));

    // One acquirer slip.
    run(&mut session, &bridge, &mut correlation, Command::Psdt("102merchantTerm".to_string()));

    // Close receipt.
    let out = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    // Exactly one bridge submission — duplicates = bug.
    assert_eq!(bridge.call_count(), 1);

    let env: CanonicalCommand = bridge.last().unwrap();
    assert_eq!(env.schema_version, "1.0");
    assert_eq!(env.command_type, CommandType::Sell);
    assert_eq!(env.fiscal_number, Identity::default().fiscal_number);
    assert_eq!(env.cashier_id.as_deref(), Some("csh1"));
    assert_eq!(env.department.as_deref(), Some("Bar1"));
    assert_eq!(env.return_check_number, None);
    // F3: key is now content-stable — prefix is "maria304:{FN}:" + 64-hex SHA-256.
    let fn_id = Identity::default().fiscal_number;
    let expected_prefix = format!("maria304:{fn_id}:");
    assert!(
        env.idempotency_key.starts_with(&expected_prefix),
        "idempotency_key must start with {expected_prefix:?}, got {:?}",
        env.idempotency_key
    );
    let hash_part = &env.idempotency_key[expected_prefix.len()..];
    assert_eq!(hash_part.len(), 64, "hash part must be 64 hex chars");
    assert!(
        hash_part.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        "hash part must be lowercase hex"
    );

    // Dual-tax mode — Cyrillic А=1, Б=2.
    let dual = env.payload.dual_tax_mode.expect("NLPR should populate dual-tax");
    assert_eq!(dual.tax_group_1, 1);
    assert_eq!(dual.tax_group_2, 2);

    // Raw frames must match the commands we sent, in order.
    let opcodes: Vec<&str> = env.payload.raw_frames.iter().map(|f| f.opcode.as_str()).collect();
    assert_eq!(
        opcodes,
        vec!["FISC", "FISC", "FISC", "NLPR", "ACLD", "PSDt", "COMP"],
    );

    // Exactly one PSDt → session counter reflects it.
    assert_eq!(
        env.payload.raw_frames.iter().filter(|f| f.opcode == "PSDt").count(),
        1,
    );

    // COMP response — last wire frame must carry the fiscal_id the
    // mock bridge minted (0000000001).
    assert_eq!(out.len(), 3);
    match &out[0] {
        Response::Data(p) => {
            assert!(p.starts_with("COMP0000000001"), "got {p}");
        }
        other => panic!("{other:?}"),
    }

    // Receipt is closed after successful COMP.
    assert!(!session.receipt_open());

    // Idempotency sequence advances — second receipt would have seq=1.
    assert_eq!(correlation.receipt_seq, 1);
}

#[test]
fn cancel_receipt_does_not_hit_the_bridge() {
    let (mut session, bridge, mut correlation) = logged_in_session();

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Fisc("line".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Cnac);

    assert_eq!(bridge.call_count(), 0, "CANC must not touch the bridge");
    assert!(!session.receipt_open());
    // Sequence untouched because no COMP happened.
    assert_eq!(correlation.receipt_seq, 0);
}

#[test]
fn bridge_error_surfaces_as_softblock_and_leaves_receipt_open() {
    let (mut session, bridge, mut correlation) = logged_in_session();
    bridge.force_error(BridgeError::Transport("mock down".to_string()));

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Fisc("line".to_string()));
    let out = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    // Bridge error → SOFTBLOCK, single frame, no Ready.
    assert_eq!(out, vec![Response::Error(ErrorCode::SoftBlock)]);

    // Receipt stays open so caller can retry or CANC.
    assert!(session.receipt_open());
    assert_eq!(correlation.receipt_seq, 0);
}

#[test]
fn bridge_typed_rejection_translates_to_specific_soft_code() {
    let (mut session, bridge, mut correlation) = logged_in_session();
    bridge.force_error(BridgeError::Rejected {
        code: "SOFTBADART".to_string(),
        message: "bad article".to_string(),
    });

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    let out = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    assert_eq!(out, vec![Response::Error(ErrorCode::SoftBadArt)]);
}

// F3: idempotency_key is now content-stable.
// Two receipts with different content produce different keys.
// The same receipt content produces the same key (supports dedup on TCP reconnect).
#[test]
fn idempotency_key_is_content_stable() {
    let fn_id = Identity::default().fiscal_number;
    let prefix = format!("maria304:{fn_id}:");

    // Receipt A: one item "X".
    let (mut session, bridge, mut correlation) = logged_in_session();
    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Fisc("100item".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    let key_a1 = bridge.last().unwrap().idempotency_key.clone();

    // Receipt B: different item "Y".
    run(&mut session, &bridge, &mut correlation, Command::Prep("Y".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Fisc("200item".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    let key_b = bridge.last().unwrap().idempotency_key.clone();

    // Different content → different keys.
    assert_ne!(key_a1, key_b, "different receipt content must produce different idempotency keys");

    // Both keys have the right format.
    for key in [&key_a1, &key_b] {
        assert!(key.starts_with(&prefix), "key must start with {prefix:?}");
        assert_eq!(&key[prefix.len()..].len(), &64, "hash part must be 64 chars");
    }

    // Simulate retry: same receipt "X" (new session, same content) → same key.
    let (mut session2, bridge2, mut corr2) = logged_in_session();
    run(&mut session2, &bridge2, &mut corr2, Command::Prep("X".to_string()));
    run(&mut session2, &bridge2, &mut corr2, Command::Fisc("100item".to_string()));
    run(&mut session2, &bridge2, &mut corr2, Command::Comp(String::new()));
    let key_a2 = bridge2.last().unwrap().idempotency_key.clone();

    assert_eq!(key_a1, key_a2,
        "same receipt content must produce the same key across sessions (content-stable)");
}

#[test]
fn comp_retry_after_bridge_failure_submits_same_canonical_envelope() {
    // Invariant: a COMP that failed because the bridge was down should
    // be retryable as-is.  Receipt state must survive the first COMP,
    // so the second COMP produces an identical envelope except for
    // the outcome.
    let (mut session, bridge, mut correlation) = logged_in_session();
    bridge.force_error(BridgeError::Transport("network blip".to_string()));

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Fisc("line".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Psdt("102ACQ".to_string()));

    // First COMP — fails.
    let out = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    assert_eq!(out, vec![Response::Error(ErrorCode::SoftBlock)]);
    assert_eq!(bridge.call_count(), 0, "failed submit must not be logged");
    assert!(session.receipt_open(), "receipt must stay open after failure");

    // Second COMP — bridge recovered, same envelope submits cleanly.
    let out = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    assert_eq!(out.len(), 3); // Data + Done + Ready
    assert_eq!(bridge.call_count(), 1, "recovered submit is logged");
    assert!(!session.receipt_open());

    // F1-fix: build_canonical deduplicates COMP frames — only one COMP in envelope
    // even though raw_frames in session state has two (from the failed first attempt).
    let env = bridge.last().unwrap();
    let opcodes: Vec<&str> = env.payload.raw_frames.iter().map(|f| f.opcode.as_str()).collect();
    assert_eq!(opcodes, vec!["FISC", "PSDt", "COMP"]);

    // F1-fix: the retry must use the SAME idempotency key as a clean first attempt.
    // COMP frames are excluded from the fingerprint so accumulated COMPs in raw_frames
    // don't shift the hash and accidentally bypass the sidecar's idempotency journal.
    let (mut clean, clean_b, mut clean_c) = logged_in_session();
    run(&mut clean, &clean_b, &mut clean_c, Command::Prep("X".to_string()));
    run(&mut clean, &clean_b, &mut clean_c, Command::Fisc("line".to_string()));
    run(&mut clean, &clean_b, &mut clean_c, Command::Psdt("102ACQ".to_string()));
    run(&mut clean, &clean_b, &mut clean_c, Command::Comp(String::new()));
    let clean_key = clean_b.last().unwrap().idempotency_key;

    assert_eq!(env.idempotency_key, clean_key,
        "retry COMP must produce the same idempotency key as a clean attempt with identical content");
}

#[test]
fn session_psdt_sequence_stays_in_sync_with_receipt_psdt_sequence() {
    // Regression guard for the Session/OpenReceipt duplication.
    let (mut session, bridge, mut correlation) = logged_in_session();

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    for _ in 0..3 {
        run(&mut session, &bridge, &mut correlation, Command::Psdt("slip".to_string()));
    }
    assert_eq!(session.psdt_sequence, 3);
    match &session.state {
        maria304_driver::session::SessionState::ReceiptOpen(r) => {
            assert_eq!(r.psdt_sequence, 3, "OpenReceipt counter must mirror Session");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn bridge_rejection_with_unknown_code_falls_back_to_softblock() {
    // Defensive: if the Python gateway returns a code we don't know
    // (or that's malformed), fall back to SOFTBLOCK so the caller
    // gets a retryable signal instead of crashing.
    let (mut session, bridge, mut correlation) = logged_in_session();
    bridge.force_error(BridgeError::Rejected {
        code: "NONSENSE".to_string(),
        message: "x".to_string(),
    });

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    let out = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    assert_eq!(out, vec![Response::Error(ErrorCode::SoftBlock)]);
}

#[test]
fn cancelled_then_reopened_receipt_starts_with_fresh_raw_frames() {
    // After CANC, the next PREP must start with an empty accumulator —
    // otherwise leftover frames would contaminate the next receipt.
    let (mut session, bridge, mut correlation) = logged_in_session();

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Fisc("leftover".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Cnac);

    run(&mut session, &bridge, &mut correlation, Command::Prep("Y".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Fisc("fresh".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    let env = bridge.last().unwrap();
    let bodies: Vec<&str> = env.payload.raw_frames.iter().map(|f| f.body.as_str()).collect();
    assert_eq!(bodies, vec!["fresh", ""]); // FISC body + COMP body
    assert!(
        !env.payload.raw_frames.iter().any(|f| f.body == "leftover"),
        "cancelled receipt frames must not leak into the next",
    );
}

#[test]
fn return_receipt_command_type_is_return_not_sell() {
    let (mut session, bridge, mut correlation) = logged_in_session();

    // BCHN sets the return-check number.
    run(
        &mut session,
        &bridge,
        &mut correlation,
        Command::Bchn("123".to_string()),
    );
    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    let env = bridge.last().unwrap();
    assert_eq!(env.command_type, CommandType::Return);
    assert_eq!(env.return_check_number.as_deref(), Some("123"));
}

#[test]
fn return_line_opcode_marks_receipt_as_return() {
    let (mut session, bridge, mut correlation) = logged_in_session();

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Bfis("return-line".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    let env = bridge.last().unwrap();
    assert_eq!(env.command_type, CommandType::Return);
    assert_eq!(env.return_check_number, None);
}

// ── Task 5: submit_report ok=false returns error ──────────────────────────────

#[test]
fn submit_report_bridge_ok_false_returns_error() {
    // Regression guard: submit_report() must NOT return ok(None) when the
    // bridge sets ok=false.  Previously the Ok(_) arm ignored resp.ok.
    let (mut session, bridge, mut correlation) = logged_in_session();

    // Inject ok=false for the ZREP submit.
    bridge.set_next_response(canonical_response(false, "", "REJECTED"));

    let responses = run(&mut session, &bridge, &mut correlation, Command::Zrep);

    // Must return an error frame, not DONE+READY.
    let has_error = responses.iter().any(|r| matches!(r, Response::Error(_)));
    assert!(
        has_error,
        "expected Error when bridge ok=false, got: {responses:?}",
    );

    // Correlation sequence must NOT advance on failure.
    assert_eq!(
        correlation.receipt_seq, 0,
        "sequence must not advance when submit_report fails",
    );
}

#[test]
fn submit_report_bridge_ok_true_returns_done_ready() {
    // Positive case: ok=true must still produce DONE+READY (no regression).
    let (mut session, bridge, mut correlation) = logged_in_session();

    bridge.set_next_response(canonical_response(true, "0000000001", "ACK"));

    let responses = run(&mut session, &bridge, &mut correlation, Command::Zrep);

    let has_error = responses.iter().any(|r| matches!(r, Response::Error(_)));
    assert!(
        !has_error,
        "expected no error when bridge ok=true, got: {responses:?}",
    );
    assert_eq!(
        correlation.receipt_seq, 1,
        "sequence must advance on success",
    );
}

// ── Task 6: fail-close COMP dispatcher ───────────────────────────────────────

fn canonical_response(ok: bool, fiscal_id: &str, document_state: &str) -> CanonicalResponse {
    CanonicalResponse {
        ok,
        document_id:         "doc-test".into(),
        fiscal_id:           fiscal_id.into(),
        fiscal_ts:           "2026-04-22T10:00:00Z".into(),
        document_state:      document_state.into(),
        sale_total_kopecks:  500,
        return_total_kopecks: 0,
    }
}

#[test]
fn comp_accepted_closes_receipt() {
    let (mut session, bridge, mut correlation) = logged_in_session();
    bridge.set_next_response(canonical_response(true, "99999", "SENT"));

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    // Receipt must be closed — a new COMP without PREP must fail with SoftNoDoc.
    let responses = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    assert!(
        responses.iter().any(|r| matches!(r, Response::Error(ErrorCode::SoftNoDoc))),
        "receipt must be closed after Accepted: {responses:?}",
    );
}

#[test]
fn comp_terminal_closes_receipt_as_rejected() {
    let (mut session, bridge, mut correlation) = logged_in_session();
    bridge.set_next_response(canonical_response(false, "", "REJECTED"));

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    let responses = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    // Terminal: an error response is returned.
    assert!(
        responses.iter().any(|r| matches!(r, Response::Error(_))),
        "terminal COMP must return error: {responses:?}",
    );
    // Receipt must be closed — next COMP without PREP must fail with SoftNoDoc.
    let next = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    assert!(
        next.iter().any(|r| matches!(r, Response::Error(ErrorCode::SoftNoDoc))),
        "receipt must be closed after Terminal: {next:?}",
    );
}

#[test]
fn comp_retryable_leaves_receipt_open() {
    let (mut session, bridge, mut correlation) = logged_in_session();
    bridge.set_next_response(canonical_response(false, "", "ERROR_SEND"));

    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    let responses = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    // Retryable: an error response is returned.
    assert!(
        responses.iter().any(|r| matches!(r, Response::Error(_))),
        "retryable COMP must return error: {responses:?}",
    );
    // Receipt must stay open — can retry COMP immediately.
    bridge.set_next_response(canonical_response(true, "88888", "SENT"));
    let retry = run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    assert!(
        retry.iter().any(|r| matches!(r, Response::Data(_))),
        "retry COMP after Retryable must succeed: {retry:?}",
    );
}
