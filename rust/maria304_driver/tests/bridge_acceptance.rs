//! Sprint M4 integration acceptance — canonical envelope posted
//! to the mock bridge on COMP.
//!
//! Plan doc §M4 requires: "Full SELL flow lands a canonical envelope
//! on the mock bridge with every field populated from wire commands".

use maria304_driver::bridge::{BridgeError, CanonicalCommand, CommandType, MockBridge};
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
    assert_eq!(
        env.idempotency_key,
        format!("maria304:{}:sess-test:0", Identity::default().fiscal_number),
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

#[test]
fn idempotency_key_changes_per_receipt_sequence() {
    let (mut session, bridge, mut correlation) = logged_in_session();

    // Three receipts in a row.
    for _ in 0..3 {
        run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
        run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));
    }
    assert_eq!(bridge.call_count(), 3);

    let keys: Vec<String> = bridge
        .submitted()
        .iter()
        .map(|e| e.idempotency_key.clone())
        .collect();
    assert_eq!(
        keys,
        vec![
            format!("maria304:{}:sess-test:0", Identity::default().fiscal_number),
            format!("maria304:{}:sess-test:1", Identity::default().fiscal_number),
            format!("maria304:{}:sess-test:2", Identity::default().fiscal_number),
        ],
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
    // Manually flip direction — no wire opcode in M4 yet; direction
    // is set by the session state init which defaults to Sale.
    // For the envelope to be a RETURN, we flip the OpenReceipt.
    run(&mut session, &bridge, &mut correlation, Command::Prep("X".to_string()));
    if let maria304_driver::session::SessionState::ReceiptOpen(ref mut r) = session.state {
        r.direction = maria304_driver::bridge::ReceiptDirection::Return;
    }
    run(&mut session, &bridge, &mut correlation, Command::Comp(String::new()));

    let env = bridge.last().unwrap();
    assert_eq!(env.command_type, CommandType::Return);
    assert_eq!(env.return_check_number.as_deref(), Some("123"));
}
