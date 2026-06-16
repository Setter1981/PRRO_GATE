//! Invariant fuzzer (Phase 0) — Task 1: operation alphabet + reference model.
//!
//! Task 1 is PURE data + PURE logic: the `Op` alphabet (spec §5), the per-op
//! DPS wire `DpsScript`, and a deterministic `RefModel` that predicts the
//! expected ledger (spec §6).  NO SQLite, NO `inline::run`, NO `ScriptedDps`
//! execution — the generator / interpreter / oracle land in Tasks 2-7.
//!
//! The seed-lane and issued-set semantics are load-bearing: the model's
//! "issued" predicate MUST reuse the single-source-of-truth const
//! `fiscal_documents::OFFLINE_ISSUED_STATES` (spec §6), not a second literal.

#![allow(dead_code)]

// The helper modules live under `tests/invariant_fuzzer/`; a crate-root test
// file resolves bare `mod` to its own directory, so point at the subdir
// explicitly (the layout the plan pins).
#[path = "invariant_fuzzer/model.rs"]
mod model;
#[path = "invariant_fuzzer/op.rs"]
mod op;

// Task 2: the interpreter reuses the shared `ScriptedDps` + `det_signing_ctx`
// from `tests/common/` and drives the real seams.
mod common;
#[path = "invariant_fuzzer/interp.rs"]
mod interp;
// Task 3: the shrink-first op-sequence generator.
#[path = "invariant_fuzzer/strategy.rs"]
mod strategy;
// Task 4: the differential oracle (layer 1).
#[path = "invariant_fuzzer/oracle.rs"]
mod oracle;

use prro::db::models::enums::{DocState, NodeMode, ShiftState};
use prro::db::repositories::fiscal_documents::OFFLINE_ISSUED_STATES;

use proptest::prelude::*;

use model::{ExpectedOutcome, RefModel};
use op::{DpsScript, Op, Stage};

/// Every `DocState` variant — the test enumerates the domain to prove the
/// model's issued predicate mirrors the SSOT const for ALL states (the
/// issued-set itself comes only from the const, never this list).
const ALL_DOC_STATES: [DocState; 13] = [
    DocState::Prepared,
    DocState::Signed,
    DocState::Encrypted,
    DocState::Sending,
    DocState::Sent,
    DocState::Kvt1,
    DocState::Kvt2,
    DocState::Ack,
    DocState::OfflineLocalAck,
    DocState::Rejected,
    DocState::Cancelled,
    DocState::ErrorRetryable,
    DocState::RequiresManualReconciliation,
];

fn mutation(outcome: &ExpectedOutcome) -> &model::Mutation {
    match outcome {
        ExpectedOutcome::Mutated(m) => m,
        other => panic!("expected Mutated, got {other:?}"),
    }
}

// ── Required (plan Task 1 acceptance) ───────────────────────────────────────

/// `apply(OnlineSell→ACK)` advances `next_lnd` by one AND advances the seed to
/// that doc's unsigned hash (online-origin issues only at ACK, spec §6).
#[test]
fn online_sell_ackpath_advances_lnd_and_seed() {
    let mut m = RefModel::new_online_open_shift();
    assert_eq!(m.next_lnd, 1);
    assert_eq!(m.seed, None, "genesis seed");

    let out = m.apply(&Op::OnlineSell(DpsScript::ack_path()));

    let mu = mutation(&out);
    assert_eq!(mu.lnd, 1);
    assert_eq!(mu.doc_state, DocState::Ack);
    assert_eq!(mu.previous_hash, None, "first doc chains to genesis tip");
    assert_eq!(m.next_lnd, 2, "next_lnd advanced by one");
    assert_eq!(m.docs.get(&1), Some(&DocState::Ack));
    assert!(m.seed.is_some(), "seed advanced at ACK");
    assert_eq!(m.seed, mu.seed_after, "model seed == reported seed_after");
}

/// `apply(OfflineSell)` advances the seed at `OFFLINE_LOCAL_ACK` (issuance), not
/// later, and consumes exactly one offline code (spec §6 offline lane).
#[test]
fn offline_sell_advances_seed_at_offline_local_ack_and_consumes_code() {
    let mut m = RefModel::new_offline_open_shift(3);
    let seed_before = m.seed;

    let out = m.apply(&Op::OfflineSell);

    let mu = mutation(&out);
    assert_eq!(mu.lnd, 1);
    assert_eq!(mu.doc_state, DocState::OfflineLocalAck);
    assert_eq!(m.docs.get(&1), Some(&DocState::OfflineLocalAck));
    assert_ne!(m.seed, seed_before, "seed advanced at OFFLINE_LOCAL_ACK");
    assert_eq!(m.seed, mu.seed_after);
    assert_eq!(m.codes_consumed, 1, "exactly one code consumed");
    assert_eq!(mu.code_consumed, Some(1));
    // OFFLINE_LOCAL_ACK is in the SSOT issued set — the doc is issued at issuance.
    assert!(RefModel::is_offline_origin_issued(
        DocState::OfflineLocalAck
    ));
}

/// The model's offline-origin issued set IS `fiscal_documents::OFFLINE_ISSUED_STATES`
/// — asserted by REFERENCING the const, never a re-typed literal (spec §6).
#[test]
fn model_offline_issued_set_is_the_ssot_const() {
    // The model returns the const itself (by reference), not a private copy.
    assert_eq!(
        RefModel::offline_issued_states(),
        &OFFLINE_ISSUED_STATES[..],
        "model must expose the SSOT const, not a forked set"
    );
    // …and its membership predicate agrees with the const for EVERY DocState,
    // so a future hand-rolled literal that drifts is caught here.
    for state in ALL_DOC_STATES {
        assert_eq!(
            RefModel::is_offline_origin_issued(state),
            OFFLINE_ISSUED_STATES.contains(&state.as_str()),
            "issued predicate drifted from OFFLINE_ISSUED_STATES for {state:?}"
        );
    }
}

// ── Lane-correctness reinforcements (pure model behaviours) ─────────────────

/// A DPS reject of an online doc → `inline::run` returns Err(DpsRejected) → the
/// interpreter reports Refused, so the model reports NoMutation (no issuance).
/// The lnd is still consumed + a NON-ISSUED rejected row is minted; the seed
/// does not advance.
#[test]
fn online_sell_reject_is_no_mutation_with_non_issued_rejected_row() {
    let mut m = RefModel::new_online_open_shift();
    let out = m.apply(&Op::OnlineSell(DpsScript::send_then_reject()));
    assert_eq!(out, ExpectedOutcome::NoMutation);
    assert_eq!(
        m.docs.get(&1),
        Some(&DocState::Rejected),
        "a non-issued rejected row is still minted"
    );
    assert_eq!(m.seed, None, "reject must not advance the seed");
    assert_eq!(m.next_lnd, 2, "the lnd is still consumed");
}

/// A timed-out online send does NOT reach ACK and does NOT advance the seed
/// (no false issuance) — the precise terminal state is refined by the Task 4
/// differential against the real seam.
#[test]
fn online_sell_timeout_does_not_falsely_issue() {
    let mut m = RefModel::new_online_open_shift();
    let out = m.apply(&Op::OnlineSell(DpsScript::timeout_at_call(1)));
    assert_ne!(mutation(&out).doc_state, DocState::Ack);
    assert_eq!(m.seed, None, "a timed-out send must not advance the seed");
}

/// send→Ack then lastChk→NotFound holds at SENT (probe-pending, kill-matrix K4
/// shape); not yet ACK, so the seed has not advanced.
#[test]
fn online_sell_ack_then_lastchk_not_found_holds_at_sent() {
    let mut m = RefModel::new_online_open_shift();
    let out = m.apply(&Op::OnlineSell(DpsScript::send_ack_then_last_not_found()));
    assert_eq!(mutation(&out).doc_state, DocState::Sent);
    assert_eq!(m.seed, None, "SENT (pre-confirm) has not advanced the seed");
}

/// Chain continuity: the second doc's `previous_hash` equals the first doc's
/// advanced seed (the tip) — the property the §7 differential asserts.
#[test]
fn chain_continuity_previous_hash_equals_prior_tip() {
    let mut m = RefModel::new_online_open_shift();
    let first = m.apply(&Op::OnlineSell(DpsScript::ack_path()));
    let tip_after_first = m.seed;
    assert!(tip_after_first.is_some());

    let second = m.apply(&Op::OnlineSell(DpsScript::ack_path()));
    let mu2 = mutation(&second);
    assert_eq!(mu2.lnd, 2);
    assert_eq!(
        mu2.previous_hash, tip_after_first,
        "second doc must chain to the first doc's tip"
    );
    assert_ne!(
        mutation(&first).seed_after,
        mu2.seed_after,
        "each issued doc advances the tip to a distinct value"
    );
}

// ── Invalid / re-entry + fault classification ───────────────────────────────

/// Every invalid / re-entry / replay op is a typed refusal or no-op: NO fiscal
/// mutation (no lnd, no seed, no code consumption) — spec §5.
#[test]
fn invalid_and_reentry_ops_do_not_mutate() {
    // RepeatReboot now defers as Fault (it drives the boot seam); the rest are
    // NoMutation no-ops (some force mode/shift — not a fiscal mutation).
    // OfflineSellDuringGoingOnline stays here: its GoingOnline-mode sell is refused
    // by the post-sign DISPATCHER (NodeGoingOnline), which leaves NO committed doc
    // (the offline-ack CodePoolExhausted refusal — a DIFFERENT path — is the one
    // that mints a non-issued Aborted row).
    let invalid = [
        Op::RepeatDrain,
        Op::DuplicateIdemKey,
        Op::GoOnlineWithoutBacklog,
        Op::OfflineSellDuringGoingOnline,
        Op::SellWithClosedShift,
    ];
    for op in invalid {
        let mut m = RefModel::new_online_open_shift();
        let before = (m.next_lnd, m.seed, m.codes_consumed, m.docs.len());
        let out = m.apply(&op);
        assert_eq!(out, ExpectedOutcome::NoMutation, "{op:?} must be a no-op");
        assert_eq!(
            (m.next_lnd, m.seed, m.codes_consumed, m.docs.len()),
            before,
            "{op:?} must not mutate any fiscal state"
        );
    }
}

/// Fault / not-yet-modelled-deterministic ops (crash, reboot, drain, go_online)
/// defer to the fault/re-sync oracle (Task 5) and do NOT mutate the pure model.
/// Only crash / reboot remain `Fault` (Task 4 moved Drain / GoOnline out of
/// Fault into real predictions — plan constraint #1).
#[test]
fn crash_and_reboot_are_fault_without_mutation() {
    let faults = [Op::Crash(Stage::Send), Op::Reboot, Op::RepeatReboot];
    for op in faults {
        let mut m = RefModel::new_online_open_shift();
        let before = (m.next_lnd, m.seed, m.codes_consumed, m.docs.len());
        let out = m.apply(&op);
        assert_eq!(out, ExpectedOutcome::Fault, "{op:?} must defer as Fault");
        assert_eq!(
            (m.next_lnd, m.seed, m.codes_consumed, m.docs.len()),
            before,
            "{op:?} must not mutate the pure model"
        );
    }
}

/// Drain / GoOnline are no longer `Fault`: out of their precondition (an Online
/// node, no GoingOnline / no offline backlog) they predict a no-op, NOT a fault.
#[test]
fn drain_and_go_online_out_of_precondition_are_no_mutation() {
    for op in [
        Op::Drain(DpsScript::ack_path()),
        Op::GoOnline(DpsScript::ack_path()),
    ] {
        let mut m = RefModel::new_online_open_shift(); // mode Online: not GoingOnline / not Offline
        let out = m.apply(&op);
        assert_eq!(
            out,
            ExpectedOutcome::NoMutation,
            "{op:?} out of precondition must be NoMutation, not Fault"
        );
    }
}

// ── Task 3 Part B — generator smoke + shrink demonstration ──────────────────

/// Drive one generated `Op` sequence through the interpreter on a fresh DB.
/// No op may panic or hit `unimplemented!`; out-of-precondition intents degrade
/// to no-ops (the interpreter classifies admissibility at runtime).
fn drive_sequence(ops: &[Op]) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
        for op in ops {
            let _ = interp::run_op(&mut ctx, op).await;
        }
    });
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Acceptance #2: 64 generated sequences over the WHOLE alphabet run through
    /// the interpreter without a precondition-panic or a reachable
    /// `unimplemented!` (out-of-precondition intents degrade to no-ops).
    #[test]
    fn op_sequences_run_without_panic(ops in strategy::op_sequence()) {
        drive_sequence(&ops);
    }
}

/// Acceptance #3: shrinking demonstrably reduces a forced failure to the
/// minimal triggering sequence (a single `OnlineSell`).
#[test]
fn shrinking_reduces_a_forced_failure_to_minimal() {
    use proptest::test_runner::{Config, TestError, TestRunner};

    let mut runner = TestRunner::new(Config {
        cases: 256,
        ..Config::default()
    });
    let result = runner.run(&strategy::op_sequence(), |ops| {
        // Forced failure: any sequence containing an OnlineSell "fails".
        prop_assert!(
            !ops.iter().any(|op| matches!(op, Op::OnlineSell(_))),
            "forced failure: sequence contains an OnlineSell"
        );
        Ok(())
    });

    match result {
        Err(TestError::Fail(_, minimal)) => {
            assert_eq!(
                minimal.len(),
                1,
                "shrinking reduced the counterexample to one op, got {}: {minimal:?}",
                minimal.len()
            );
            assert!(
                matches!(minimal[0], Op::OnlineSell(_)),
                "the minimal repro is the single trigger op, got {:?}",
                minimal[0]
            );
        }
        other => panic!("expected a shrunk TestError::Fail, got {other:?}"),
    }
}

// ── Task 4 Part A — RefModel Drain / GoOnline prediction (Fault → Mutated) ───

#[test]
fn model_drain_ackpath_advances_backlog_to_ack() {
    let mut m = RefModel::new_offline_open_shift(1);
    let _ = m.apply(&Op::OfflineSell); // backlog: docs[1] = OFFLINE_LOCAL_ACK
    assert_eq!(m.docs.get(&1), Some(&DocState::OfflineLocalAck));
    let seed_before = m.seed;
    m.mode = NodeMode::GoingOnline; // fixture: the probe already flipped (test setup)

    let out = m.apply(&Op::Drain(DpsScript::ack_path()));

    assert!(
        matches!(out, ExpectedOutcome::Mutated(_)),
        "an advancing drain is PredictableMutating, got {out:?}"
    );
    assert_eq!(
        m.docs.get(&1),
        Some(&DocState::Ack),
        "backlog doc drained to ACK"
    );
    assert_eq!(
        m.seed, seed_before,
        "drain does NOT re-advance the seed (offline-origin advanced at issuance)"
    );
    assert_eq!(
        m.mode,
        NodeMode::Online,
        "a full drain flips GoingOnline → Online"
    );
}

#[test]
fn model_drain_reject_halts_and_escalates_manual() {
    let mut m = RefModel::new_offline_open_shift(2);
    let _ = m.apply(&Op::OfflineSell); // docs[1]
    let _ = m.apply(&Op::OfflineSell); // docs[2]
    m.mode = NodeMode::GoingOnline;

    let out = m.apply(&Op::Drain(DpsScript::send_then_reject()));

    assert!(matches!(out, ExpectedOutcome::Mutated(_)));
    assert_eq!(
        m.docs.get(&1),
        Some(&DocState::Rejected),
        "first backlog doc → REJECTED"
    );
    assert_eq!(
        m.docs.get(&2),
        Some(&DocState::OfflineLocalAck),
        "the rest are held (strict-sequential halt-on-reject)"
    );
    assert_eq!(
        m.shift_state,
        ShiftState::RequiresManualReconciliation,
        "K8: a drain reject of an OFFLINE_LOCAL_ACK backlog doc escalates manual"
    );
}

#[test]
fn model_go_online_transitions_and_drains() {
    let mut m = RefModel::new_offline_open_shift(1);
    let _ = m.apply(&Op::OfflineSell);

    let out = m.apply(&Op::GoOnline(DpsScript::ack_path()));

    assert!(matches!(out, ExpectedOutcome::Mutated(_)));
    assert_eq!(
        m.mode,
        NodeMode::Online,
        "go_online: Offline → (GoingOnline) → Online"
    );
    assert_eq!(
        m.docs.get(&1),
        Some(&DocState::Ack),
        "the backlog drained to ACK"
    );
}

// ── Task 4 Part B — differential oracle (model vs interpreter) ──────────────

/// Acceptance [1]: a clean valid sequence differential-matches the model at
/// every step (Doc case + structural seed).
#[tokio::test]
async fn differential_clean_online_sell_sequence_matches_model() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let mut model = RefModel::new_online_open_shift();

    for i in 1..=3 {
        let prior_tip = ctx.read_seed().await; // real tip BEFORE the op
        let op = Op::OnlineSell(DpsScript::ack_path());
        let expected = model.apply(&op);
        let real = interp::run_op(&mut ctx, &op).await;
        oracle::check_differential(&real, &expected, prior_tip.as_deref())
            .unwrap_or_else(|d| panic!("op {i}: clean sell must match the model: {d:?}"));
    }
}

/// Acceptance [2]: an injected model/real divergence (real lnd != model lnd) is
/// caught — `check_differential` returns `Err(Divergence)`, not a pass.
#[test]
fn differential_catches_lnd_divergence() {
    let expected = ExpectedOutcome::Mutated(model::Mutation {
        lnd: 2,
        doc_state: DocState::Ack,
        seed_after: Some([2u8; 32]),
        previous_hash: Some([1u8; 32]),
        code_consumed: None,
    });
    let real = interp::RealOutcome::Doc(interp::ObservedDoc {
        lnd: 3, // ← divergence: model expected lnd 2
        doc_state: DocState::Ack,
        previous_hash: Some(vec![1u8; 32]),
        seed_after: Some(vec![9u8; 32]),
        code_consumed: None,
    });
    let res = oracle::check_differential(&real, &expected, Some(&[1u8; 32]));
    assert!(
        res.is_err(),
        "real lnd 3 != model lnd 2 must be flagged; got {res:?}"
    );
}

/// Acceptance [3]: an invalid op (`SellWithClosedShift`) is `ExpectedNoMutation`
/// — the differential accepts the real refusal and asserts no fiscal issuance
/// (the seed does not advance).  It NEVER applies an lnd+1 expectation here.
#[tokio::test]
async fn differential_invalid_sell_with_closed_shift_is_no_mutation() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let mut model = RefModel::new_online_open_shift();

    let seed_before = ctx.read_seed().await;
    let op = Op::SellWithClosedShift;
    let expected = model.apply(&op);
    assert_eq!(
        oracle::classify(&expected),
        oracle::OpClass::ExpectedNoMutation,
        "an invalid op classifies as ExpectedNoMutation"
    );

    let real = interp::run_op(&mut ctx, &op).await;
    oracle::check_differential(&real, &expected, seed_before.as_deref())
        .expect("ExpectedNoMutation must accept the real refusal");

    // No fiscal issuance: the MAC seed must not advance (a refused sell issues
    // no receipt).  (A non-issued PREPARED shell may exist; the seed is the
    // load-bearing no-issuance signal.)
    assert_eq!(
        ctx.read_seed().await,
        seed_before,
        "refused sell must not advance the seed"
    );
}

/// Drain / GoOnline differential: after `GoOnline` (probe + drain) the real
/// ledger matches the model's predicted ledger (the Recovered ledger-delta).
#[tokio::test]
async fn differential_go_online_ledger_matches_model() {
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(1).await;
    let mut model = RefModel::new_offline_open_shift(1);

    let _ = model.apply(&Op::OfflineSell);
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await;

    let _ = model.apply(&Op::GoOnline(DpsScript::ack_path()));
    let _ = interp::run_op(&mut ctx, &Op::GoOnline(DpsScript::ack_path())).await;

    let real_ledger = ctx.read_ledger().await;
    oracle::check_ledger_delta(&model.docs, &real_ledger)
        .expect("after go_online the real ledger must match the model's predicted ledger");
    assert_eq!(
        real_ledger.get(&1),
        Some(&DocState::Ack),
        "backlog doc reached ACK"
    );
}

// ── Task 5 — fault oracle: quiescent scan (L2) + bounded postcond + resync (L3)

/// Acceptance: Crash(Send) + Reboot → recovery routes the committed-SENDING doc
/// to ERROR_RETRYABLE with NO second send_chk (no blind resend, kill-matrix K3),
/// and the post-recovery quiescent boundary scans clean.
#[tokio::test]
async fn fault_crash_send_reboot_bounded_postcond_and_clean() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;

    let crashed = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    assert!(
        matches!(crashed, interp::RealOutcome::Crashed { .. }),
        "Crash(Send) committed SENDING and dropped the wire future"
    );
    let sends_before = ctx.send_calls();

    let _ = interp::run_op(&mut ctx, &Op::Reboot).await;

    oracle::assert_crash_send_recovery(ctx.only_doc_state().await, sends_before, ctx.send_calls())
        .expect("Crash(Send)+Reboot bounded postcondition");
    // Quiescent boundary: AFTER recovery the scan is clean.
    oracle::assert_clean(&ctx.pool).await;
}

/// Acceptance: after Crash(Send)+Reboot, re-syncing the model from the real DB
/// makes the NEXT op differential-clean again (we adopt recovery, not predict it).
#[tokio::test]
async fn fault_resync_then_next_op_is_differential_clean() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let mut model = RefModel::new_online_open_shift();

    // Fault + recovery (model leaves these as Fault — no prediction).
    let _ = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    let _ = model.apply(&Op::Crash(Stage::Send));
    let _ = interp::run_op(&mut ctx, &Op::Reboot).await;
    let _ = model.apply(&Op::Reboot);

    // Adopt the real recovered state.
    model.resync_from_db(&ctx.pool).await;
    assert_eq!(
        model.docs,
        ctx.read_ledger().await,
        "resync adopts the real ledger"
    );

    // The next op is differential-clean from the re-synced state.
    let prior_tip = ctx.read_seed().await;
    let op = Op::OnlineSell(DpsScript::ack_path());
    let expected = model.apply(&op);
    let real = interp::run_op(&mut ctx, &op).await;
    oracle::check_differential(&real, &expected, prior_tip.as_deref())
        .expect("after resync the next op must be differential-clean");
}

/// Quiescent-boundary timing (spec §7.2): a committed-SENDING is a LEGAL
/// in-flight transient — scanning mid-crash false-positives on it.  The scan
/// belongs AFTER recovery, where it is clean.
#[tokio::test]
async fn scan_skips_mid_crash_sending_transient() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;

    let _ = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sending,
        "committed SENDING is a legal in-flight transient"
    );

    // A scan HERE would FALSE-POSITIVE (StuckSending) — so the harness does NOT
    // scan mid-crash.
    let mid_crash = prro::db::invariant_scan::scan(&ctx.pool)
        .await
        .expect("scan query");
    assert!(
        !mid_crash.is_empty(),
        "mid-crash SENDING would be flagged (StuckSending) — proving the scan must NOT run here"
    );

    // Resolve with Reboot, THEN scan: clean at the quiescent boundary.
    let _ = interp::run_op(&mut ctx, &Op::Reboot).await;
    let post = prro::db::invariant_scan::scan(&ctx.pool)
        .await
        .expect("scan query");
    assert!(
        post.is_empty(),
        "post-recovery quiescent scan is clean, got {post:?}"
    );
    oracle::assert_clean(&ctx.pool).await;
}

/// SETTLED-mode scan gate (architect decision, 2026-06-16) — DURABLE pin.
///
/// `GoingOnline` is the SECOND class of legitimate in-flight transient (the
/// generalisation of spec §7.2's mid-crash rule): a `Crash(Send)` →
/// force-`GoingOnline` → `Reboot` sequence leaves an online-origin `SENDING` doc
/// that boot reconciliation DEFERS to the W9 drain loop (branch d,
/// `boot_phase.rs:1739` — "FN in GOING_ONLINE mode, W9 backlog drain owns this
/// FN's reconciliation").  The doc is NOT resolved by the reboot, and being
/// online-origin it is NOT in the drain cohort either — it rests `SENDING`
/// LEGITIMATELY until the node settles back to `Online` and a reboot resolves it.
///
/// So the harness must scan ONLY in a SETTLED mode `{Online, Offline}` — NOT
/// mid-transition.  This test makes the suppression AUDITABLE: it proves the
/// scan WOULD flag the resting `SENDING` (the suppression is load-bearing, not
/// vacuous), that the gate's predicate skips it in `GoingOnline`, and that a
/// genuinely-stuck doc is still caught post-settle (Online reboot resolves
/// `SENDING → ERROR_RETRYABLE`, then the scan runs clean).
#[tokio::test]
async fn scan_gate_suppresses_going_online_transient_then_clean_on_settle() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;

    // 1) Crash(Send) → doc1 committed SENDING (legal mid-crash transient).
    let crashed = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    assert!(matches!(crashed, interp::RealOutcome::Crashed { .. }));

    // 2) Force GoingOnline (the adverse OfflineSellDuringGoingOnline seam).
    let _ = interp::run_op(&mut ctx, &Op::OfflineSellDuringGoingOnline).await;
    assert_eq!(ctx.read_node_mode().await, NodeMode::GoingOnline);

    // 3) Reboot → branch-d defer (boot_phase.rs:1739): the SENDING doc STAYS put.
    let _ = interp::run_op(&mut ctx, &Op::Reboot).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sending,
        "boot reconciliation DEFERS a GoingOnline FN to the W9 drain — the SENDING \
         doc is NOT resolved (and online-origin, it is not in the drain cohort)"
    );
    assert_eq!(ctx.read_node_mode().await, NodeMode::GoingOnline);

    // The scan WOULD flag this resting SENDING — the suppression is load-bearing.
    let in_transition = prro::db::invariant_scan::scan(&ctx.pool)
        .await
        .expect("scan query");
    assert!(
        in_transition
            .iter()
            .any(|v| matches!(v, prro::db::invariant_scan::Violation::StuckSending { .. })),
        "the scan DOES flag resting SENDING — the SETTLED-mode gate is exactly what \
         suppresses this false positive in the GoingOnline transition; got {in_transition:?}"
    );
    // The gate's predicate: GoingOnline is NOT settled → the harness skips the scan.
    assert!(
        !matches!(
            ctx.read_node_mode().await,
            NodeMode::Online | NodeMode::Offline
        ),
        "GoingOnline is mid-transition (not SETTLED) — the harness scan gate skips here"
    );

    // 4) Settle: return to Online, Reboot → per-doc dispatch resolves SENDING →
    // ERROR_RETRYABLE (no resend).  Now SETTLED → the scan runs and is clean.
    ctx.force_node_mode(NodeMode::Online).await;
    let _ = interp::run_op(&mut ctx, &Op::Reboot).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::ErrorRetryable,
        "post-settle Online reboot resolves SENDING → ERROR_RETRYABLE — a genuinely \
         stuck doc IS caught at the settled boundary"
    );
    assert!(matches!(
        ctx.read_node_mode().await,
        NodeMode::Online | NodeMode::Offline
    ));
    oracle::assert_clean(&ctx.pool).await; // SETTLED → scan runs → clean
}

/// Bounded postcond for Crash(Kvt1) + Reboot (kill-matrix K4): SENT-before-confirm
/// → recovery takes the PROBE path (a lastChk), NOT a resend.
#[tokio::test]
async fn fault_crash_kvt1_reboot_probe_no_resend() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;

    let crashed = interp::run_op(&mut ctx, &Op::Crash(Stage::Kvt1)).await;
    assert!(matches!(crashed, interp::RealOutcome::Crashed { .. }));
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sent,
        "Crash(Kvt1) committed SENT"
    );
    let (sends_before, lasts_before) = (ctx.send_calls(), ctx.last_calls());

    let _ = interp::run_op(&mut ctx, &Op::Reboot).await;

    oracle::assert_probe_recovery_no_resend(
        sends_before,
        ctx.send_calls(),
        lasts_before,
        ctx.last_calls(),
    )
    .expect("Crash(Kvt1)+Reboot: probe path, no resend");
}

// ── Task 6 — Mirror-2 drift (offline_session ↔ drain_cohort), the 5th class ──

/// An empty active offline session is LEGAL — the Mirror-2 predicate is over
/// DOCS (each cohort doc points at the active session), NOT "every session has
/// docs".  A naive "session must have docs" predicate would false-positive here.
#[tokio::test]
async fn mirrors_legal_empty_active_session_passes() {
    // OPEN offline session, offline codes, but ZERO cohort docs.
    let ctx = interp::FuzzCtx::new_offline_open_shift(1).await;
    oracle::check_mirrors(&ctx.pool)
        .await
        .expect("an empty active offline session is legal (no false-positive)");
}

/// A seeded Mirror-2 desync — a drain-cohort doc repointed at a FOREIGN
/// (non-active) session — is caught.  The foreign session is non-null, so
/// invariant_scan's check-6d (NULL-only) misses it; Mirror-2 is the predicate
/// that catches the mismatch.
#[tokio::test]
async fn mirrors_catch_seeded_mirror2_desync() {
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(1).await;

    // A real offline sell stamps the cohort doc with the ACTIVE session.
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await;
    oracle::check_mirrors(&ctx.pool)
        .await
        .expect("a correctly-stamped cohort is Mirror-2-clean");

    // Corrupt Mirror-2: repoint the cohort doc at a foreign CLOSED session.
    ctx.corrupt_cohort_session_to_foreign().await;

    let res = oracle::check_mirrors(&ctx.pool).await;
    assert!(
        res.is_err(),
        "a cohort doc pointing at a non-active session must be caught; got {res:?}"
    );
    assert!(
        format!("{res:?}").contains("Mirror-2"),
        "the mismatch is a Mirror-2 violation (not check-6d's NULL case), got {res:?}"
    );
}

// ── Task 7 — the end-to-end harness (generator → interpreter → all oracles) ──

/// SETTLED predicate (A2/A4): a node is settled-for-scan when its mode is a
/// resting `{Online, Offline}` OR the shift is `RequiresManualReconciliation` —
/// a LEGITIMATE durable operator terminal (AUD-K8-1), scanned IN PLACE, never
/// forced out.  Reject-halt legitimately rests at `GoingOnline + RMR`; the
/// system must NOT auto-settle from there, so RMR is settled-for-scan.  Used by
/// BOTH the per-op scan gate and the terminal settle.
fn is_settled(mode: NodeMode, shift: ShiftState) -> bool {
    matches!(mode, NodeMode::Online | NodeMode::Offline)
        || shift == ShiftState::RequiresManualReconciliation
}

/// The shift states a real drain can make progress on (`backlog_drain.rs`
/// finalize transitions + SW-3 fail-loud).  A drain over a non-eligible shift
/// (e.g. a force-closed shift) cannot finalize `GoingOnline → Online`, so a
/// GoingOnline left over a non-eligible shift is a forced-mode artifact, not a
/// liveness failure.  `RequiresManualReconciliation` is excluded here (it is
/// already SETTLED via `is_settled`).
fn shift_drain_eligible(shift: ShiftState) -> bool {
    matches!(
        shift,
        ShiftState::Opened
            | ShiftState::OpenedLocalPendingDrain
            | ShiftState::ClosingLocalPendingDrain
    )
}

/// The terminal disposition for a node that has run its bounded real recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalVerdict {
    /// Settled (`{Online, Offline}` or RMR) → run the quiescent scan + mirrors.
    Scan,
    /// Still un-settled with a REAL settle path that a drain should have taken
    /// (GoingOnline + active session + non-empty cohort + drain-eligible shift)
    /// → a genuine liveness failure.
    Liveness,
    /// A forced-mode GoingOnline artifact a real drain cannot progress (no
    /// session — an impossible real GoingOnline; OR empty cohort — nothing to
    /// drain; OR ineligible shift — force-closed).  Do NOT scan (a deferred
    /// online-origin SENDING would false-flag) and do NOT liveness-panic
    /// (no real settle path exists) — assert bounded no-resend instead.
    ArtifactNoResend,
}

/// Decide the terminal disposition (PURE — unit-tested on the decision table).
/// Inputs are read from the REAL DB / real drain predicates by the caller
/// (`is_settled`, `active_offline_session`, `drain_cohort_len`,
/// `shift_drain_eligible`), so the structural settle-capability test matches
/// exactly what a real drain would see.
fn terminal_verdict(
    mode: NodeMode,
    shift: ShiftState,
    has_active_session: bool,
    cohort_nonempty: bool,
    shift_eligible: bool,
) -> TerminalVerdict {
    if is_settled(mode, shift) {
        return TerminalVerdict::Scan;
    }
    if mode == NodeMode::GoingOnline {
        if has_active_session && cohort_nonempty && shift_eligible {
            TerminalVerdict::Liveness
        } else {
            TerminalVerdict::ArtifactNoResend
        }
    } else {
        // Any other non-settled mode is unexpected in the fuzzer alphabet
        // (the generator only reaches Online / Offline / GoingOnline) — treat a
        // surprise un-settled mode as a liveness failure (fail loud).
        TerminalVerdict::Liveness
    }
}

/// Terminal liveness + scan (A2/A4): after the op-loop the node must SETTLE on
/// its own.  Drives BOUNDED REAL recovery ops — a drain-tick WITH Ack responses
/// (simulating DPS coming back) at GoingOnline, and a Reboot to resolve a
/// committed crash transient — up to `SETTLE_BUDGET` rounds, NEVER `force_node_mode`.
/// Then dispatches on [`terminal_verdict`]:
///   - `Scan` → `assert_clean` + `check_mirrors` (RMR scanned in place);
///   - `Liveness` → panic (a real drain should have settled this but didn't);
///   - `ArtifactNoResend` → a forced-mode GoingOnline artifact: assert the
///     bounded no-resend invariant (the recovery ops re-drove nothing).
async fn settle_and_scan(ctx: &mut interp::FuzzCtx, pending_crash: bool) {
    // A legitimate settle takes 1–2 real recovery ops (a drain-tick that
    // finalizes GoingOnline → Online, OR a reboot that resolves a crash
    // transient, OR a reject that lands RMR).  `3` is a small bound above that;
    // exceeding it with a real settle path open is the liveness signal.
    const SETTLE_BUDGET: usize = 3;

    let sends_before = ctx.send_calls();
    let mut crash_pending = pending_crash;
    for _ in 0..SETTLE_BUDGET {
        let mode = ctx.read_node_mode().await;
        let shift = ctx.read_shift_state().await;
        if is_settled(mode, shift) && !crash_pending {
            break;
        }
        if mode == NodeMode::GoingOnline {
            // Drive a real cohort-sized Ack drain ONLY when it can finalize
            // (settle-CAPABLE: active session + non-empty drain cohort +
            // drain-eligible shift).  Otherwise a drain would either no-op (no
            // session / empty cohort) or send pointlessly then fail at finalize
            // (ineligible / force-closed shift) — so we skip it, leaving those
            // forced-mode artifacts for the no-resend branch below.
            if let Some(sid) = ctx.active_offline_session().await {
                if shift_drain_eligible(shift) && ctx.drain_cohort_len(sid).await > 0 {
                    let _ = interp::settle_drain_tick(ctx).await;
                }
            }
        }
        // Reboot resolves a committed crash transient (SENDING → ERROR_RETRYABLE,
        // SENT → probe) at a settled mode; at GoingOnline it DEFERS (no wire call).
        let _ = interp::run_op(ctx, &Op::Reboot).await;
        crash_pending = false;
    }

    let mode = ctx.read_node_mode().await;
    let shift = ctx.read_shift_state().await;
    let session = ctx.active_offline_session().await;
    let cohort_nonempty = match session {
        Some(sid) => ctx.drain_cohort_len(sid).await > 0,
        None => false,
    };
    match terminal_verdict(
        mode,
        shift,
        session.is_some(),
        cohort_nonempty,
        shift_drain_eligible(shift),
    ) {
        TerminalVerdict::Scan => {
            oracle::assert_clean(&ctx.pool).await;
            if let Err(d) = oracle::check_mirrors(&ctx.pool).await {
                panic!("terminal mirror drift (mode={mode:?} shift={shift:?}): {d:?}");
            }
        }
        TerminalVerdict::Liveness => {
            panic!(
                "LIVENESS: node did not settle to {{Online, Offline, RMR}} within \
                 {SETTLE_BUDGET} real recovery ops despite a real settle path (active \
                 session + non-empty drain cohort + drain-eligible shift) — \
                 mode={mode:?} shift={shift:?}"
            );
        }
        TerminalVerdict::ArtifactNoResend => {
            // A forced-mode GoingOnline a real drain cannot progress (no session —
            // an impossible real GoingOnline, which is only entered via the
            // return-online probe FROM Offline, i.e. with a session; OR empty
            // cohort; OR ineligible/force-closed shift).  Do NOT scan (a deferred
            // online-origin SENDING would false-flag StuckSending) and do NOT
            // liveness-panic (no real settle path).  Assert the bounded invariant
            // that the recovery ops re-drove nothing.
            assert_eq!(
                ctx.send_calls(),
                sends_before,
                "terminal GoingOnline artifact (mode={mode:?} shift={shift:?}): the bounded \
                 real recovery ops made a NEW wire send — deferred docs must not be re-driven"
            );
        }
    }
}

/// Per-op dispatch gluing T1-T6: model.apply + run_op, then assert per the op's
/// classification, with the T5 scan-timing rule (never mid-crash) and re-sync
/// after a fault.
async fn run_harness(ops: &[Op], mut ctx: interp::FuzzCtx, mut model: RefModel) {
    // A crash transient opened but not yet resolved by a reboot (A1 scan-gate
    // suppresses the scan while set; A3 asserts the no-resend postcond on the
    // resolving reboot).
    let mut pending_crash = false;
    for op in ops {
        let prior_tip = ctx.read_seed().await; // real MAC tip BEFORE the op
        let codes_before = ctx.consumed_codes_count().await;
        let sends_before = ctx.send_calls(); // wire send count BEFORE the op (A3 no-resend)
        let shift_before = ctx.read_shift_state().await; // real shift_state BEFORE the op

        let expected = model.apply(op);
        let real = interp::run_op(&mut ctx, op).await;

        let class = oracle::classify(&expected);
        match class {
            // Fault / recovery — we do NOT predict recovery; adopt the real DB.
            oracle::OpClass::FaultOrRecovery => {
                model.resync_from_db(&ctx.pool).await;
            }
            // Predictable mutation — differential-match the model.
            oracle::OpClass::PredictableMutating => {
                if let Err(d) = oracle::check_differential(&real, &expected, prior_tip.as_deref()) {
                    panic!("differential divergence on {op:?}: {d:?}");
                }
                // drain / go-online carry no per-doc detail → ledger-delta.
                if matches!(real, interp::RealOutcome::Recovered { .. }) {
                    let real_ledger = ctx.read_ledger().await;
                    if let Err(d) = oracle::check_ledger_delta(&model.docs, &real_ledger) {
                        panic!("ledger-delta divergence on {op:?}: {d:?}");
                    }
                }
            }
            // No mutation — the differential is permissive here, so the harness
            // independently asserts NO ISSUANCE (else an erroneously-mutating
            // invalid op slips through).
            oracle::OpClass::ExpectedNoMutation => {
                if let Err(d) = oracle::check_differential(&real, &expected, prior_tip.as_deref()) {
                    panic!("no-mutation differential on {op:?}: {d:?}");
                }
                assert_eq!(
                    ctx.read_seed().await,
                    prior_tip,
                    "ExpectedNoMutation {op:?} advanced the seed (issuance leaked)"
                );
                assert_eq!(
                    ctx.consumed_codes_count().await,
                    codes_before,
                    "ExpectedNoMutation {op:?} consumed an offline code (issuance leaked)"
                );
            }
        }

        // Mode-INDEPENDENT AUD-K8-1 teeth (bounded-postcond on wire calls).
        // A drain re-tick on a `RequiresManualReconciliation` FN must make NO new
        // wire send — the re-entry guard (`backlog_drain.rs:725`) halts the drain;
        // WITHOUT it the drain re-drives the orphaned backlog → a fresh `send_chk`.
        // Conditioned on the RMR state BEFORE the op (so the op that ITSELF
        // escalates to RMR — which legitimately sent the rejecting wire call — is
        // excluded), and counts wire calls rather than running a scan, so it bites
        // regardless of mode — including the `GoingOnline` window where the
        // SETTLED-mode scan gate suppresses `assert_clean`.  This is the
        // mode-independent home of the teeth the SETTLED gate must NOT blunt.
        if shift_before == ShiftState::RequiresManualReconciliation {
            assert_eq!(
                ctx.send_calls(),
                sends_before,
                "AUD-K8-1: op {op:?} on an RMR FN made a NEW wire send — the drain \
                 re-entry guard (backlog_drain.rs:725) must halt a re-tick on a \
                 manual-reconciliation FN"
            );
        }

        // Scan-timing (T5, generalised): scan ONLY in a SETTLED state — NOT
        // mid-crash AND NOT mid-transition.  This is the principled reading of
        // spec §7.2 ("a committed-in-flight transient is legal — do not scan
        // there"): mid-crash and mid-transition are the TWO classes of legitimate
        // in-flight transient.
        //   - mid-crash: a Crash opens a committed `SENDING` transient; tracked
        //     by `pending_crash`.  A Reboot does NOT always resolve it — if the
        //     node is in `GoingOnline`, boot reconciliation DEFERS the doc to the
        //     W9 drain loop (branch d, `boot_phase.rs:1739`), so the transient
        //     persists until a later drain/settle.
        //   - mid-transition: `GoingOnline` is a transitional mode whose pending
        //     docs belong to the W9 drain / subsequent online-settle, NOT to a
        //     quiescent ledger.  SETTLED ⟺ `mode ∈ {Online, Offline}`.
        // A genuinely-stuck doc is still caught at the post-settle Online boundary
        // (once the node settles, drain/convergence resolves it and the scan is
        // either clean or flags a REAL violation).
        // A1 (HIGH): track the crash-transient from the REAL outcome, not the op
        // NAME.  A `Crash(stage)` only opens a committed in-flight transient when
        // it actually reached the wire and was dropped (`RealOutcome::Crashed`);
        // on an Offline node it never reaches the wire and COMPLETES as a real
        // offline sell (`RealOutcome::Doc`), leaving the node SETTLED with nothing
        // to defer — so the settled scan must NOT be suppressed there.
        // A1: a Crash opens a committed in-flight transient (the SETTLED scan is
        // suppressed until a reboot resolves it).
        if matches!(real, interp::RealOutcome::Crashed { .. }) {
            pending_crash = true;
        }
        // A3: a reboot that RESOLVES a pending crash must NOT re-send (DPS does
        // not dedup → a blind resend double-fiscalises) — assert the UNIVERSAL
        // no-resend bounded postcond IN the property harness (was: faults only
        // resync, silently adopting a resend).  Only no-resend is asserted here:
        // the exact terminal AND the Kvt1 PROBE vary under composition (a SENT
        // doc at GoingOnline is DEFERRED to the W9 drain → no probe THIS reboot),
        // so those stay pinned in the directed K3/K4 tests; no-resend is the
        // invariant that holds in ALL compositions.
        if matches!(op, Op::Reboot | Op::RepeatReboot) && pending_crash {
            if let Err(d) = oracle::assert_no_resend(sends_before, ctx.send_calls()) {
                panic!("crash-recovery resend on {op:?}: {d:?}");
            }
            pending_crash = false;
        }
        //   - RMR: `is_settled` also admits `RequiresManualReconciliation` (a
        //     legitimate durable operator terminal, AUD-K8-1) even at mode
        //     GoingOnline — a reject-halt rests there and MUST be scanned in
        //     place (a violation there is a REAL finding, not suppressed).
        let mode_now = ctx.read_node_mode().await;
        let shift_now = ctx.read_shift_state().await;
        if !pending_crash && is_settled(mode_now, shift_now) {
            oracle::assert_clean(&ctx.pool).await;
            if let Err(d) = oracle::check_mirrors(&ctx.pool).await {
                panic!("mirror drift on {op:?}: {d:?}");
            }
        }

        // After a NON-fault op, adopt ONLY the precondition state (mode /
        // shift_state / active session) from the real DB so the NEXT op
        // dispatches from reality.  The transition seams (go_online / drain /
        // the force-ops) move mode/shift/session in ways the pure model need
        // not perfectly mirror; the LEDGER (which the differential just checked)
        // and the mirrors (which the scan just checked) are what we hold the
        // model to.  Fault ops already did a FULL resync above.
        if !matches!(class, oracle::OpClass::FaultOrRecovery) {
            model.resync_preconditions_from_db(&ctx.pool).await;
        }
    }

    // A2/A4: terminal liveness + scan.  The per-op scan is suppressed mid-crash
    // and mid-transition; without a terminal pass an unpaired crash or a
    // GoingOnline terminal would NEVER be recovered/scanned.  This drives bounded
    // REAL recovery until the node settles, then scans (or fails liveness on a
    // genuine non-settling settle-path; a forced-mode artifact asserts no-resend).
    settle_and_scan(&mut ctx, pending_crash).await;
}

fn drive(ops: &[Op], offline: bool) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        if offline {
            run_harness(
                ops,
                interp::FuzzCtx::new_offline_open_shift(3).await,
                RefModel::new_offline_open_shift(3),
            )
            .await;
        } else {
            run_harness(
                ops,
                interp::FuzzCtx::new_online_open_shift().await,
                RefModel::new_online_open_shift(),
            )
            .await;
        }
    });
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// The fuzzer from an ONLINE-seeded fixture.
    #[test]
    fn harness_online_seeded(ops in strategy::op_sequence()) {
        drive(&ops, false);
    }

    /// The fuzzer from an OFFLINE-seeded fixture — REQUIRED: the AUD-K8-1 /
    /// drain / manual-recon lane only exists offline (the teeth live here).
    #[test]
    fn harness_offline_seeded(ops in strategy::op_sequence()) {
        drive(&ops, true);
    }
}

/// Post-sign refusal MIRROR (deterministic pin for the seed-rare divergence the
/// `Aborted` prod fix introduced).  After the fix a no-code offline sell mints a
/// non-issued `Aborted` row (the lnd is consumed, reaching SIGNED, then the
/// offline-ack refuses → the terminalise_inbox seam aborts it).  The model MUST
/// mirror it: else a later `GoOnline` ledger-delta diverges by the extra
/// `Aborted` doc the model omitted.  Sequence: 3 codes → 3 OFFLINE_LOCAL_ACK,
/// the 4th sell is no-code → Aborted, then GoOnline drains the backlog.
#[test]
fn harness_offline_no_code_sell_mirrors_aborted_row() {
    drive(
        &[
            Op::OfflineSell,
            Op::OfflineSell,
            Op::OfflineSell,
            Op::OfflineSell, // no code left → reality mints a non-issued Aborted row
            Op::GoOnline(DpsScript::ack_path()),
        ],
        true,
    );
}

/// AUD-K8-1 TEETH CANARY (deterministic; see `tests/invariant_fuzzer/TEETH_TEST.md`).
///
/// Constructs the exact AUD-K8-1 scenario the fuzzer hunts: an offline backlog
/// is drained with a leading reject, which REJECTS the head doc and escalates the
/// shift to `RequiresManualReconciliation` (halting the drain, leaving the
/// successor held).  A drain RE-TICK must then be a no-op — the re-entry guard at
/// `backlog_drain.rs:725` halts a drain on an RMR FN.  WITHOUT that guard the
/// drain re-enters and re-sends the orphaned successor (a fresh `send_chk`),
/// defeating the escalation's "durable operator surface, halts FN drain" contract.
///
/// `#[ignore]` because it asserts a property of the PRESENT guard: it PASSES on
/// main and FAILS only when the guard is reverted — a manual canary, not a CI
/// gate.  Detection is MODE-INDEPENDENT (counts wire calls, not a scan), so it
/// bites even though the reverted re-drive rests in `GoingOnline`, where the
/// harness's SETTLED-mode scan gate suppresses `assert_clean`.
#[tokio::test]
#[ignore = "AUD-K8-1 teeth canary: PASSES with the backlog_drain.rs:725 guard, \
            FAILS when it is reverted. See tests/invariant_fuzzer/TEETH_TEST.md."]
async fn teeth_aud_k8_1_rmr_redrive_makes_no_new_wire_call() {
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(2).await;

    // Backlog: two OFFLINE_LOCAL_ACK docs.
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await;
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await;

    // Drain (via GoOnline) with a leading reject → head doc REJECTED, shift →
    // RequiresManualReconciliation, drain halts (the successor stays held).
    let _ = interp::run_op(&mut ctx, &Op::GoOnline(DpsScript::send_then_reject())).await;
    let ledger = ctx.read_ledger().await;
    assert_eq!(
        ledger.get(&1),
        Some(&DocState::Rejected),
        "head backlog doc is REJECTED by the leading reject"
    );
    assert_eq!(
        ledger.get(&2),
        Some(&DocState::OfflineLocalAck),
        "the successor is HELD (strict-sequential halt-on-reject)"
    );

    let sends_after_escalate = ctx.send_calls();

    // Re-tick the drain.  WITH the AUD-K8-1 guard this is a no-op (the RMR shift
    // halts the drain — no wire call).  WITHOUT it the drain re-enters and
    // re-sends the orphaned successor → a fresh send_chk.
    let _ = interp::run_op(&mut ctx, &Op::RepeatDrain).await;
    assert_eq!(
        ctx.send_calls(),
        sends_after_escalate,
        "AUD-K8-1: a drain re-tick on an RMR FN must make NO new wire call. If this \
         fails, the backlog_drain.rs:725 re-entry guard is missing — the fuzzer's \
         teeth bite (this counts wire calls, not a scan, so it is mode-independent)."
    );
}

// ── Hardening (CI conditions) — Cluster A: crash/scan correctness ────────────

/// A1 (HIGH): `pending_crash` must reflect the REAL outcome, not the op name.
///
/// On an OFFLINE node a `Crash(Send)` never reaches the wire — `inline::run`
/// takes the offline-ack branch and COMPLETES, so `run_op` returns
/// `RealOutcome::Doc` (a real offline sell), NOT `RealOutcome::Crashed`
/// (interp.rs `crash_via_drop`: the `res = &mut fut` select arm wins).  There is
/// therefore NO in-flight transient and the node rests in a SETTLED `Offline`
/// state, where the harness MUST run its quiescent scan / mirror check.
///
/// The current harness sets `pending_crash = true` from the op NAME
/// (`Op::Crash(_)`), wrongly suppressing the settled scan — a false-negative
/// window.  This is made observable with a planted Mirror-2 violation: a settled
/// scan would catch it (panic); a suppressed scan returns cleanly.
#[test]
fn a1_offline_crash_send_completes_as_doc_so_settled_scan_runs() {
    // Build a ctx with a planted Mirror-2 corruption, drive [Crash(Send)] through
    // the harness, and assert the harness panicked (caught the planted violation).
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
            // Plant a cohort doc, then repoint it at a FOREIGN session → Mirror-2 desync.
            let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await;
            ctx.corrupt_cohort_session_to_foreign().await;
            // sanity: the corruption really is present before the harness runs.
            assert!(
                oracle::check_mirrors(&ctx.pool).await.is_err(),
                "setup: a Mirror-2 violation must be planted"
            );
            // [Crash(Send)] on OFFLINE completes as a real offline sell (Doc),
            // leaving the node SETTLED (Offline) — the harness must scan here.
            run_harness(
                &[Op::Crash(Stage::Send)],
                ctx,
                RefModel::new_offline_open_shift(3),
            )
            .await;
        });
    }))
    .is_err();
    std::panic::set_hook(prev);

    assert!(
        panicked,
        "the harness must scan at the SETTLED Offline boundary after a Crash(Send) that \
         completed as a real offline sell (RealOutcome::Doc, not Crashed) and catch the \
         planted Mirror-2 violation. It returned cleanly → the settled scan was wrongly \
         suppressed because pending_crash was set from the op name instead of the real outcome."
    );
}

/// A2 (HIGH): an UNPAIRED crash must be recovered + scanned at the terminal.
///
/// A `Crash(Send)` with no following `Reboot` leaves a committed `SENDING`
/// transient and `pending_crash` set to sequence end — the per-op scan is
/// suppressed for the rest of the run, and today there is NO final recovery /
/// scan, so the transient is never resolved or checked.  The terminal
/// `settle_and_scan` must drive a real `Reboot` to resolve the unpaired crash
/// (`SENDING → ERROR_RETRYABLE`, no resend) and then scan the settled boundary.
#[tokio::test]
async fn a2_terminal_settle_resolves_unpaired_crash_and_scans() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;

    let crashed = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    assert!(matches!(crashed, interp::RealOutcome::Crashed { .. }));
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sending,
        "an unpaired crash leaves a committed SENDING transient"
    );
    let sends_before = ctx.send_calls();

    // Terminal procedure: must reboot the unpaired crash → ERROR_RETRYABLE (no
    // resend) AND scan the now-settled boundary.
    settle_and_scan(&mut ctx, true).await;

    assert_eq!(
        ctx.only_doc_state().await,
        DocState::ErrorRetryable,
        "A2: the terminal settle must reboot an unpaired crash transient to ERROR_RETRYABLE"
    );
    assert_eq!(
        ctx.send_calls(),
        sends_before,
        "A2: the terminal recovery must NOT resend across the reboot"
    );
    assert!(matches!(ctx.read_node_mode().await, NodeMode::Online));
}

/// A4 (HIGH): a LEGITIMATE GoingOnline terminal (active session + drainable
/// cohort) must be settled + scanned — closing the indefinite no-scan zone.
///
/// The terminal `settle_and_scan` drives a real drain-tick WITH Ack responses
/// (simulating DPS coming back) → the OFFLINE_LOCAL_ACK backlog drains to ACK →
/// finalize CAS's `GoingOnline → Online` → the settled boundary scans clean.
#[tokio::test]
async fn a4_terminal_settle_drains_legit_going_online_to_online() {
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(1).await;
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await; // OFFLINE_LOCAL_ACK backlog
    ctx.force_node_mode(NodeMode::GoingOnline).await; // legit GoingOnline (active session + cohort)
    assert!(
        ctx.active_offline_session().await.is_some(),
        "the offline fixture has an active session — this is the settle-able GoingOnline"
    );

    settle_and_scan(&mut ctx, false).await;

    assert_eq!(
        ctx.read_node_mode().await,
        NodeMode::Online,
        "A4: the terminal settle must drain a legit GoingOnline cohort to Online"
    );
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "the backlog drained to ACK at the terminal settle"
    );
}

/// A4 (HIGH) — DURABLE PIN: the no-session GoingOnline ARTIFACT must NOT
/// liveness-panic and must NOT be scanned.
///
/// WHY this is an artifact, not a real liveness failure: a real PRRO node only
/// enters `GoingOnline` via the return-online probe FROM `Offline`, which always
/// has an active offline session.  The adverse `OfflineSellDuringGoingOnline`
/// seam reaches `GoingOnline` via a `force_node_mode` setter on an ONLINE node
/// with NO session — an impossible real state.  Real recovery seams cannot
/// settle it (drain skips with `no_active_offline_session`, `backlog_drain.rs:741`;
/// reboot DEFERS a GoingOnline FN to the W9 drain, branch d `boot_phase.rs:1739`),
/// and it cannot be scanned (the deferred online-origin `SENDING` would
/// false-flag `StuckSending` — the exact false positive the SETTLED gate exists
/// to suppress).  So the terminal asserts the bounded no-resend invariant
/// (recovery re-drove nothing) instead of liveness-panicking or scanning.
/// This test makes that decision AUDITABLE (an external reviewer asking "why is
/// this GoingOnline neither settled nor scanned" is answered here).
#[tokio::test]
async fn a4_terminal_no_session_going_online_artifact_does_not_liveness_panic() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let _ = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await; // online-origin SENDING
    let _ = interp::run_op(&mut ctx, &Op::OfflineSellDuringGoingOnline).await; // force GoingOnline
    assert_eq!(ctx.read_node_mode().await, NodeMode::GoingOnline);
    assert!(
        ctx.active_offline_session().await.is_none(),
        "online fixture has NO offline session — GoingOnline here is the impossible-state artifact"
    );
    let sends_before = ctx.send_calls();

    // Must return WITHOUT a liveness panic (the no-session artifact branch) and
    // assert bounded no-resend internally.
    settle_and_scan(&mut ctx, true).await;

    assert_eq!(
        ctx.send_calls(),
        sends_before,
        "artifact recovery must not re-drive the deferred doc (no new wire send)"
    );
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sending,
        "the deferred online-origin SENDING is left in place (not re-driven, not scanned)"
    );
}

/// A4 (HIGH): the terminal decision table (PURE) — proves liveness BITES on a
/// genuine settle-path GoingOnline, RMR is settled-in-place, and every forced-
/// mode artifact (no session / empty cohort / ineligible shift) routes to the
/// bounded no-resend branch (never a liveness false positive).
#[test]
fn a4_terminal_verdict_decision_table() {
    use TerminalVerdict::{ArtifactNoResend, Liveness, Scan};

    // Settled → Scan.
    assert_eq!(
        terminal_verdict(NodeMode::Online, ShiftState::Opened, false, false, true),
        Scan
    );
    assert_eq!(
        terminal_verdict(NodeMode::Offline, ShiftState::Opened, true, true, true),
        Scan
    );
    // RMR is SETTLED even at mode GoingOnline (reject-halt durable terminal).
    assert_eq!(
        terminal_verdict(
            NodeMode::GoingOnline,
            ShiftState::RequiresManualReconciliation,
            true,
            true,
            false
        ),
        Scan
    );
    // GoingOnline with a REAL settle path (active session + non-empty cohort +
    // drain-eligible shift) that did not settle → LIVENESS (a genuine failure).
    assert_eq!(
        terminal_verdict(NodeMode::GoingOnline, ShiftState::Opened, true, true, true),
        Liveness
    );
    // Artifacts → ArtifactNoResend (no liveness false positive):
    assert_eq!(
        terminal_verdict(
            NodeMode::GoingOnline,
            ShiftState::Opened,
            false,
            false,
            true
        ),
        ArtifactNoResend,
        "no active session — impossible real GoingOnline"
    );
    assert_eq!(
        terminal_verdict(NodeMode::GoingOnline, ShiftState::Opened, true, false, true),
        ArtifactNoResend,
        "empty cohort — nothing to drain"
    );
    assert_eq!(
        terminal_verdict(NodeMode::GoingOnline, ShiftState::Closed, true, true, false),
        ArtifactNoResend,
        "ineligible (force-closed) shift — a drain cannot finalize"
    );
}

/// A3 — the no-resend predicate (the bounded crash-recovery postcond wired into
/// run_harness on a resolving reboot) catches a resend.
#[test]
fn a3_assert_no_resend_catches_a_resend() {
    assert!(
        oracle::assert_no_resend(1, 2).is_err(),
        "a resend (send_chk 1 -> 2) across a crash-recovery reboot must be flagged"
    );
    assert!(
        oracle::assert_no_resend(1, 1).is_ok(),
        "no resend (send_chk unchanged) is clean"
    );
}

/// A3 — the property harness ENFORCES the bounded no-resend postcond on the
/// resolving reboot (was: faults only resync).  Driving [Crash(Send), Reboot]
/// and [Crash(Kvt1), Reboot] through `run_harness` exercises the wired no-resend
/// check for BOTH crash stages; both resolve cleanly with NO resend, so the
/// harness does not panic — proving the check is exercised AND does not
/// false-fire.  (A genuine resend would now panic here, not be silently adopted.
/// The exact terminal + the Kvt1 PROBE stay in the directed K3/K4 tests — they
/// vary under composition, unlike no-resend.)
#[test]
fn harness_crash_reboot_enforces_no_resend_postcond() {
    drive(&[Op::Crash(Stage::Send), Op::Reboot], false);
    drive(&[Op::Crash(Stage::Kvt1), Op::Reboot], false);
}
