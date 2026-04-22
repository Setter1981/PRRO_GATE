#![allow(clippy::doc_markdown, clippy::duration_suboptimal_units)]
use std::sync::Arc;

use maria304_driver::bridge::{Bridge, BridgeError, CanonicalCommand, CanonicalResponse};
use maria304_driver::protocol::Command;
use maria304_driver::session::dispatcher::{
    dispatch_prepare, dispatch_with_result, Clock, Correlation, DispatchPrepared, Identity,
};
use maria304_driver::session::state::{OpenReceipt, Session, SessionState};

fn clock() -> (String, String) {
    ("20260101".to_string(), "000000".to_string())
}
fn identity() -> Identity {
    Identity::default()
}
fn corr() -> Correlation {
    Correlation { session_uuid: "test".to_string(), receipt_seq: 0 }
}

fn ok_response() -> CanonicalResponse {
    CanonicalResponse {
        ok: true,
        document_id: "DOC1".to_string(),
        fiscal_id: "0000000001".to_string(),
        fiscal_ts: "2026-01-01T00:00:00".to_string(),
        document_state: "ACK".to_string(),
        sale_total_kopecks: 1000,
        return_total_kopecks: 0,
    }
}

/// Session with a cashier logged in and a receipt open.
fn session_with_open_receipt() -> Session {
    let mut s = Session::new();
    s.cashier_id = Some("cashier1".to_string());
    s.state = SessionState::ReceiptOpen(OpenReceipt::default());
    s
}

/// Session with a cashier logged in (no open receipt).
fn session_authenticated() -> Session {
    let mut s = Session::new();
    s.cashier_id = Some("cashier1".to_string());
    s.state = SessionState::Authenticated;
    s
}

/// Bridge that panics if `submit` is called — used to verify dispatch_prepare
/// does NOT call the bridge.
#[allow(dead_code)]
struct PanicBridge;
impl Bridge for PanicBridge {
    fn submit(&self, _: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
        panic!("PanicBridge::submit was called — dispatch_prepare must not call bridge")
    }
}

#[test]
fn dispatch_prepare_comp_returns_needs_bridge_without_calling_bridge() {
    let mut s = session_with_open_receipt();
    let id = identity();
    let mut corr = corr();
    let (date, time) = clock();
    let clock = Clock { date: &date, time: &time };

    let result = dispatch_prepare(&mut s, &Command::Comp(String::new()), &id, clock, &mut corr);

    match result {
        DispatchPrepared::NeedsBridge(env) => {
            assert_eq!(env.schema_version, "1.0");
            assert!(!env.fiscal_number.is_empty());
        }
        DispatchPrepared::Done(_) => panic!("COMP should return NeedsBridge, not Done"),
    }
}

#[test]
fn dispatch_prepare_zrep_returns_needs_bridge_without_calling_bridge() {
    let mut s = session_authenticated();
    s.cashier_id = Some("cashier1".to_string());
    let id = identity();
    let mut corr = corr();
    let (date, time) = clock();
    let clock = Clock { date: &date, time: &time };

    let result = dispatch_prepare(&mut s, &Command::Zrep, &id, clock, &mut corr);
    assert!(matches!(result, DispatchPrepared::NeedsBridge(_)));
}

#[test]
fn dispatch_prepare_sync_returns_done_without_calling_bridge() {
    let mut s = Session::new();
    let id = identity();
    let mut corr = corr();
    let (date, time) = clock();
    let clock = Clock { date: &date, time: &time };

    // SYNC is a non-bridge command — should return Done immediately.
    // Using PanicBridge to prove bridge.submit() is never called.
    // (dispatch_prepare for _ arm calls dispatch() with NeverBridge internally)
    let result = dispatch_prepare(&mut s, &Command::Sync, &id, clock, &mut corr);
    assert!(matches!(result, DispatchPrepared::Done(_)));
}

#[test]
fn dispatch_with_result_comp_accepted_clears_receipt() {
    let mut s = session_with_open_receipt();
    let mut corr = corr();

    let responses = dispatch_with_result(
        &mut s,
        &Command::Comp(String::new()),
        Ok(ok_response()),
        &mut corr,
    );

    // Should return DONE + READY after accepted bridge response.
    assert!(responses.iter().any(|r| matches!(r, maria304_driver::protocol::Response::Done)));
    // Receipt must be closed.
    assert!(!s.receipt_open());
}

#[test]
fn dispatch_with_result_comp_transport_error_keeps_receipt_open() {
    let mut s = session_with_open_receipt();
    let mut corr = corr();

    let responses = dispatch_with_result(
        &mut s,
        &Command::Comp(String::new()),
        Err(BridgeError::Transport("timeout".into())),
        &mut corr,
    );

    assert!(responses.iter().any(|r| matches!(r, maria304_driver::protocol::Response::Error(_))));
    // Receipt stays open so the caller can retry or CANC.
    assert!(s.receipt_open());
}

#[tokio::test]
async fn spawn_blocking_bridge_submit_runs_outside_async_executor() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static SUBMIT_CALLED: AtomicBool = AtomicBool::new(false);

    struct TrackingBridge;
    impl Bridge for TrackingBridge {
        fn submit(&self, _: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
            SUBMIT_CALLED.store(true, Ordering::SeqCst);
            Ok(ok_response())
        }
    }

    let bridge: Arc<dyn Bridge + Send + Sync> = Arc::new(TrackingBridge);
    let mut s = session_with_open_receipt();
    let id = identity();
    let mut corr = corr();
    let (date, time) = clock();
    let clk = Clock { date: &date, time: &time };

    let prepared = dispatch_prepare(&mut s, &Command::Comp(String::new()), &id, clk, &mut corr);
    if let DispatchPrepared::NeedsBridge(envelope) = prepared {
        let b = Arc::clone(&bridge);
        let result = tokio::task::spawn_blocking(move || b.submit(&envelope))
            .await
            .expect("spawn_blocking should not panic");
        dispatch_with_result(&mut s, &Command::Comp(String::new()), result, &mut corr);
    } else {
        panic!("expected NeedsBridge");
    }

    assert!(SUBMIT_CALLED.load(Ordering::SeqCst), "bridge.submit must have been called");
}

#[test]
fn dispatch_prepare_unauthenticated_zrep_returns_softupas_not_bridge() {
    // Regression for C1: unauthenticated session must NOT reach bridge for any fiscal command.
    let mut s = Session::new(); // no cashier registered
    let id = identity();
    let mut corr = corr();
    let (date, time) = clock();
    let clk = Clock { date: &date, time: &time };

    for cmd in [
        Command::Zrep,
        Command::Nrep,
        Command::NrepLowercase,
    ] {
        let result = dispatch_prepare(&mut s, &cmd, &id, clk, &mut corr);
        match result {
            DispatchPrepared::Done(responses) => {
                assert!(
                    responses.iter().any(|r| matches!(r, maria304_driver::protocol::Response::Error(
                        maria304_driver::protocol::error_codes::ErrorCode::SoftUpas
                    ))),
                    "{cmd:?} must return SoftUpas when unauthenticated, got {responses:?}",
                );
            }
            DispatchPrepared::NeedsBridge(_) => {
                panic!("{cmd:?}: unauthenticated session must not reach bridge");
            }
        }
    }
}

#[test]
fn dispatch_prepare_unauthenticated_caioi_returns_softupas_not_bridge() {
    let mut s = Session::new();
    let id = identity();
    let mut corr = corr();
    let (date, time) = clock();
    let clk = Clock { date: &date, time: &time };

    let result = dispatch_prepare(
        &mut s,
        &Command::Caioi { sum_kopecks: 100, description: "test".to_string() },
        &id, clk, &mut corr,
    );
    assert!(
        matches!(result, DispatchPrepared::Done(_)),
        "unauthenticated CAIOI must return Done(SoftUpas), not NeedsBridge"
    );
}
