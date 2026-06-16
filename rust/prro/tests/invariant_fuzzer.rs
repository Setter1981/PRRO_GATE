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

/// A DPS reject of an online doc lands REJECTED and does NOT advance the seed
/// (online-origin issues only at ACK).
#[test]
fn online_sell_reject_lands_rejected_without_advancing_seed() {
    let mut m = RefModel::new_online_open_shift();
    let out = m.apply(&Op::OnlineSell(DpsScript::send_then_reject()));
    assert_eq!(mutation(&out).doc_state, DocState::Rejected);
    assert_eq!(m.seed, None, "reject must not advance the seed");
    assert_eq!(m.next_lnd, 2, "lnd is still allocated for the rejected doc");
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
    let invalid = [
        Op::RepeatDrain,
        Op::RepeatReboot,
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
    let faults = [Op::Crash(Stage::Send), Op::Reboot];
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
