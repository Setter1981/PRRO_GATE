//! Interpreter (Task 2): execute an `Op` sequence against a LIVE SQLite DB
//! through the REAL write-path seams.
//!
//! This is the first real consumer of `ScriptedDps` + `DpsScript`.  No
//! `proptest` generator (Task 3) and no model differential (Task 4) here — just
//! drive each `Op` through its real seam and read the observed ledger back.
//!
//! Per the plan, Task 2 GREEN is scoped to the acceptance: `OnlineSell`,
//! `Crash(Send)` (drop-injection), and `Reboot`.  The remaining alphabet
//! (`OfflineSell` / `GoOnline` / `Drain` / invalid-re-entry / the non-wire
//! `Crash` stages) is wired when the Task 3 generator starts emitting it.

// ── GREEN implementation (FuzzCtx, RealOutcome, ObservedDoc, run_op) lands
//    above this line. ──

#[tokio::test]
async fn valid_three_op_online_sell_sequence_all_reach_ack() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;

    for i in 1..=3 {
        let out = run_op(&mut ctx, &Op::OnlineSell(DpsScript::ack_path())).await;
        match out {
            RealOutcome::Doc(doc) => {
                assert_eq!(doc.lnd, i, "lnd advances 1,2,3 across the sequence");
                assert_eq!(
                    doc.doc_state,
                    DocState::Ack,
                    "an online SELL on the AckPath lands ACK end-to-end"
                );
            }
            other => panic!("op {i}: expected Doc(ACK), got {other:?}"),
        }
    }
    assert_eq!(ctx.observed_doc_count().await, 3, "three issued docs");
}

#[tokio::test]
async fn crash_send_then_reboot_recovers_without_panic_or_resend() {
    let mut ctx = FuzzCtx::new_online_open_shift().await;

    let crashed = run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    match &crashed {
        RealOutcome::Crashed {
            stage,
            committed_state,
        } => {
            assert_eq!(*stage, Stage::Send);
            assert_eq!(
                *committed_state,
                Some(DocState::Sending),
                "crash@send leaves SENDING durably committed (Pattern B intent marker)"
            );
        }
        other => panic!("expected Crashed{{Send}}, got {other:?}"),
    }
    assert_eq!(ctx.send_calls(), 1, "exactly one send_chk before the crash");

    // Reboot recovery must not panic the interpreter (drop-injection + boot-recon).
    let _ = run_op(&mut ctx, &Op::Reboot).await;

    assert_eq!(
        ctx.only_doc_state().await,
        DocState::ErrorRetryable,
        "the Sending arm downgrades to ERROR_RETRYABLE (HoldIndeterminate, no resend)"
    );
    assert_eq!(
        ctx.send_calls(),
        1,
        "send_chk total stays 1 across crash + reboot — auto-resend is forbidden"
    );
}

use crate::op::{DpsScript, Op, Stage};
use prro::db::models::enums::DocState;
