//! Sprint M5 acceptance — fiscal reports via bridge.
//!
//! Plan §M5:
//!   * Z-report POSTs to bridge and receives a `fiscal_id`.
//!   * X-report does NOT close the fiscal day (state check).

use maria304_driver::bridge::{CommandType, MockBridge};
use maria304_driver::protocol::error_codes::ErrorCode;
use maria304_driver::protocol::{Command, Response};
use maria304_driver::session::dispatcher::Correlation;
use maria304_driver::session::{dispatch, Clock, Identity, Session, SessionState};

fn clock() -> Clock<'static> {
    Clock { date: "20260420", time: "101530" }
}

fn run(s: &mut Session, b: &MockBridge, c: &mut Correlation, cmd: Command) -> Vec<Response> {
    dispatch(s, cmd, &Identity::default(), clock(), b, c)
}

fn logged_in() -> (Session, MockBridge, Correlation) {
    let mut s = Session::new();
    let b = MockBridge::new();
    let mut c = Correlation { session_uuid: "sess-r".to_string(), receipt_seq: 0 };
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
fn x_report_submits_with_xreport_command_type() {
    let (mut s, b, mut c) = logged_in();
    let out = run(&mut s, &b, &mut c, Command::Zrep);
    assert_eq!(out, vec![Response::Done, Response::Ready]);
    assert_eq!(b.call_count(), 1);
    let env = b.last().unwrap();
    assert_eq!(env.command_type, CommandType::XReport);
    // Raw frame preserves the wire opcode, not the semantic
    // command_type — Python can cross-check.
    assert_eq!(env.payload.raw_frames[0].opcode, "ZREP");
    assert_eq!(env.payload.raw_frames[0].body, "");
}

#[test]
fn x_report_is_allowed_while_nothing_is_open() {
    // X-report (our wire "ZREP") does not require any specific state
    // beyond being logged in.  It must NOT flip any "Z-report-done"
    // session flag — that is the whole difference between X and Z.
    let (mut s, b, mut c) = logged_in();
    // Before: session was just logged in, no receipt, no last CMD.
    let before = s.last_command_id.clone();
    assert_ne!(before, "ZREP");

    run(&mut s, &b, &mut c, Command::Zrep);

    // last_command_id updated
    assert_eq!(s.last_command_id, "ZREP");
    // But session state is still Authenticated — X-report doesn't
    // transition anything.
    assert_eq!(s.state, SessionState::Authenticated);
}

#[test]
fn z_report_submits_with_zreport_command_type() {
    let (mut s, b, mut c) = logged_in();
    let out = run(&mut s, &b, &mut c, Command::Nrep);
    assert_eq!(out, vec![Response::Done, Response::Ready]);
    assert_eq!(b.call_count(), 1);
    let env = b.last().unwrap();
    assert_eq!(env.command_type, CommandType::ZReport);
    assert_eq!(env.payload.raw_frames[0].opcode, "NREP");
}

#[test]
fn z_report_is_rejected_while_a_receipt_is_open() {
    let (mut s, b, mut c) = logged_in();
    run(&mut s, &b, &mut c, Command::Prep("X".to_string()));
    assert!(s.receipt_open());

    let out = run(&mut s, &b, &mut c, Command::Nrep);
    assert_eq!(out, vec![Response::Error(ErrorCode::SoftCheck)]);
    assert_eq!(b.call_count(), 0, "Z-report must not submit while receipt open");
    // Receipt untouched.
    assert!(s.receipt_open());
}

#[test]
fn nrep_lowercase_maps_to_shift_open_command_type() {
    let (mut s, b, mut c) = logged_in();
    run(&mut s, &b, &mut c, Command::NrepLowercase);
    assert_eq!(b.call_count(), 1);
    let env = b.last().unwrap();
    assert_eq!(env.command_type, CommandType::ShiftOpen);
    assert_eq!(env.payload.raw_frames[0].opcode, "nrep");
}

#[test]
fn firn_periodic_report_carries_znumber_range_in_body() {
    let (mut s, b, mut c) = logged_in();
    let cmd = Command::Firn { first: 42, last: 1234 };
    let out = run(&mut s, &b, &mut c, cmd);
    assert_eq!(out, vec![Response::Done, Response::Ready]);
    let env = b.last().unwrap();
    assert_eq!(env.command_type, CommandType::PeriodicReport);
    assert_eq!(env.payload.raw_frames[0].opcode, "FIRN");
    assert_eq!(env.payload.raw_frames[0].body, "00421234");
}

#[test]
fn firp_periodic_report_carries_date_range_in_body() {
    let (mut s, b, mut c) = logged_in();
    let cmd = Command::Firp {
        from: "20260101".to_string(),
        to: "20261231".to_string(),
    };
    run(&mut s, &b, &mut c, cmd);
    let env = b.last().unwrap();
    assert_eq!(env.payload.raw_frames[0].opcode, "FIRP");
    assert_eq!(env.payload.raw_frames[0].body, "2026010120261231");
}

#[test]
fn iren_and_irep_use_periodic_report_command_type() {
    let (mut s, b, mut c) = logged_in();
    run(&mut s, &b, &mut c, Command::Iren { first: 1, last: 2 });
    assert_eq!(b.last().unwrap().command_type, CommandType::PeriodicReport);
    run(
        &mut s,
        &b,
        &mut c,
        Command::Irep {
            from: "20260101".to_string(),
            to: "20260201".to_string(),
        },
    );
    assert_eq!(b.last().unwrap().command_type, CommandType::PeriodicReport);
    assert_eq!(b.call_count(), 2);
}

#[test]
fn local_reports_do_not_hit_the_bridge() {
    let (mut s, b, mut c) = logged_in();
    for cmd in [Command::Artz, Command::Dizv, Command::Null] {
        run(&mut s, &b, &mut c, cmd);
    }
    assert_eq!(b.call_count(), 0, "ARTZ/DIZV/NULL must stay local");
}

#[test]
fn report_idempotency_key_includes_opcode_for_replay_safety() {
    // Two X-reports back-to-back must produce distinct idempotency
    // keys so the gateway cannot accidentally dedupe them.
    let (mut s, b, mut c) = logged_in();
    run(&mut s, &b, &mut c, Command::Zrep);
    run(&mut s, &b, &mut c, Command::Zrep);
    let keys: Vec<String> = b
        .submitted()
        .iter()
        .map(|e| e.idempotency_key.clone())
        .collect();
    assert_ne!(keys[0], keys[1]);
    // Both carry ZREP as the per-report tag so audit can group them.
    assert!(keys[0].ends_with(":ZREP"));
    assert!(keys[1].ends_with(":ZREP"));
}

#[test]
fn report_bridge_failure_surfaces_as_softblock_not_done() {
    use maria304_driver::bridge::BridgeError;
    let (mut s, b, mut c) = logged_in();
    b.force_error(BridgeError::Transport("down".to_string()));
    let out = run(&mut s, &b, &mut c, Command::Nrep);
    assert_eq!(out, vec![Response::Error(ErrorCode::SoftBlock)]);
    assert_eq!(b.call_count(), 0);
    // last_command_id did NOT update on failure.
    assert_ne!(s.last_command_id, "NREP");
}

#[test]
fn report_correlation_receipt_seq_bumps_on_every_successful_report() {
    let (mut s, b, mut c) = logged_in();
    let start = c.receipt_seq;
    run(&mut s, &b, &mut c, Command::Zrep);
    run(&mut s, &b, &mut c, Command::Nrep);
    assert_eq!(c.receipt_seq, start + 2);
}

#[test]
fn report_firn_key_is_content_stable_across_tcp_sessions() {
    // F2-fix: FIRN/IREN carry a date-range body — the key must survive a TCP
    // reconnect so a DPS-already-accepted report isn't re-submitted.
    let fn_id = Identity::default().fiscal_number;
    let prefix = format!("maria304:{fn_id}:");

    let (mut s1, b1, mut c1) = logged_in();
    run(&mut s1, &b1, &mut c1, Command::Firn { first: 1, last: 10 });
    let key1 = b1.submitted().last().unwrap().idempotency_key.clone();

    // New TCP session (different session_uuid / receipt_seq) — same date range.
    let (mut s2, b2, mut c2) = logged_in();
    run(&mut s2, &b2, &mut c2, Command::Firn { first: 1, last: 10 });
    let key2 = b2.submitted().last().unwrap().idempotency_key.clone();

    assert_eq!(key1, key2, "FIRN key must be stable across TCP sessions for same date range");
    assert!(key1.starts_with(&prefix));
    assert_eq!(key1[prefix.len()..].len(), 64, "hash part must be 64 hex chars");

    // Different date range → different key.
    let (mut s3, b3, mut c3) = logged_in();
    run(&mut s3, &b3, &mut c3, Command::Firn { first: 11, last: 20 });
    let key3 = b3.submitted().last().unwrap().idempotency_key.clone();

    assert_ne!(key1, key3, "FIRN key must differ for different date ranges");
}
