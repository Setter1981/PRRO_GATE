//! Direct contract tests for the reusable `common::scripted_dps::ScriptedDps`
//! stub (invariant-fuzzer Phase 0, Task 0).
//!
//! These assert the stub's contract *directly* — the audit (plan Task 0
//! acceptance) requires the stub to be proven on its own, not only implicitly
//! via the kill-point / convergence regression net.  Three properties:
//!   (a) an unexpected / over-call (empty queue) returns a deterministic typed
//!       error (`DpsError::Internal`), NEVER a `pop().unwrap()` panic — so an
//!       over-send surfaces as a clean assertion, not a flaky panic;
//!   (b) the call log (the "envelope spy") records calls in wire order with
//!       the envelope metadata;
//!   (c) the hang-on-call hook is actually reached and is released
//!       controllably (the cancellation-injection seam used by Task 2 works).

mod common;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use tokio::sync::oneshot;

use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, DpsCheckType};
use prro::transports::dps::error::DpsError;

use common::scripted_dps::{DpsCall, ScriptedDps};

fn envelope(lnd: i32) -> CheckEnvelope {
    CheckEnvelope {
        rro_fn: "4000000001".into(),
        date_time: 0,
        check_sign: vec![],
        local_number: lnd,
        check_type: DpsCheckType::Chk,
        id_offline: String::new(),
        id_cancel: String::new(),
    }
}

fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![],
    }
}

fn counters() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
}

/// (a) An over-call against an empty queue returns the deterministic typed
/// error, not a panic — for both `send_chk` and `last_chk`.
#[tokio::test]
async fn over_call_on_empty_queue_returns_typed_error_not_panic() {
    let (send_calls, last_calls) = counters();
    let stub = ScriptedDps::new(send_calls, last_calls);

    // No `push_send` → the queue is empty; the call must NOT panic.
    let res = stub.send_chk(envelope(1)).await;
    assert!(
        matches!(res, Err(DpsError::Internal(_))),
        "over-call on empty send queue must return DpsError::Internal, not panic; got {res:?}"
    );

    let res2 = stub.last_chk(&CheckSignBlob(vec![])).await;
    assert!(
        matches!(res2, Err(DpsError::Internal(_))),
        "over-call on empty last_chk queue must return DpsError::Internal, not panic; got {res2:?}"
    );
}

/// (b) The call log records every call in wire order, carrying the envelope
/// metadata (`send_chk`'s `CheckEnvelope`, `last_chk`'s `CheckSignBlob`).
#[tokio::test]
async fn call_log_records_calls_in_wire_order_with_envelope_metadata() {
    let (send_calls, last_calls) = counters();
    let stub = ScriptedDps::new(send_calls, last_calls);
    stub.push_send(Ok(ack("SFN-1")));
    stub.push_last(Ok(ack("SFN-1")));
    stub.push_send(Ok(ack("SFN-2")));

    stub.send_chk(envelope(7)).await.expect("send 1");
    stub.last_chk(&CheckSignBlob(vec![0xAB]))
        .await
        .expect("last");
    stub.send_chk(envelope(8)).await.expect("send 2");

    let log = stub.calls();
    assert_eq!(log.len(), 3, "spy records exactly one entry per call");
    match &log[0] {
        DpsCall::SendChk(env) => assert_eq!(env.local_number, 7, "1st: send_chk lnd=7"),
        other => panic!("expected SendChk first, got {other:?}"),
    }
    match &log[1] {
        DpsCall::LastChk(blob) => assert_eq!(blob.0, vec![0xAB], "2nd: last_chk fn_sign recorded"),
        other => panic!("expected LastChk second, got {other:?}"),
    }
    match &log[2] {
        DpsCall::SendChk(env) => assert_eq!(env.local_number, 8, "3rd: send_chk lnd=8"),
        other => panic!("expected SendChk third, got {other:?}"),
    }
}

/// (c) The hang-on-call hook is actually reached (the wire await parks) and is
/// then released controllably — the cancellation-injection seam.
#[tokio::test]
async fn hang_on_send_is_reached_and_released_controllably() {
    let (send_calls, last_calls) = counters();
    let stub = Arc::new(ScriptedDps::new(send_calls, last_calls));
    stub.push_send(Ok(ack("SFN-1")));

    let (reached_tx, reached_rx) = oneshot::channel();
    let (block_tx, block_rx) = oneshot::channel();
    stub.hang_send(reached_tx, block_rx);

    let stub2 = Arc::clone(&stub);
    let handle = tokio::spawn(async move { stub2.send_chk(envelope(1)).await });

    // The await must really be reached (hang hook fired) before we release.
    reached_rx
        .await
        .expect("send_chk hang must fire `reached` (the await was entered)");

    // Controlled release: resolving the block oneshot lets the parked await
    // proceed and return the queued response.
    block_tx
        .send(())
        .expect("block receiver still alive (call is parked)");

    let res = handle.await.expect("spawned send_chk task joins");
    assert!(
        res.is_ok(),
        "after a controlled release the parked send_chk returns its queued Ok; got {res:?}"
    );
}
