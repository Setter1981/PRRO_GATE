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
use prro::db::repositories::fiscal_documents::{is_issued, OFFLINE_ISSUED_STATES};

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{FileFailurePersistence, TestRunner};

use model::{ExpectedOutcome, RefModel};
use op::{DpsScript, Op, Stage, WireResponse};

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
/// that doc's unsigned hash (online-origin issues at SEND — A.3 advance-at-SEND;
/// an ACK-path sell is past the SEND crossing, so its seed is advanced).
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
/// later, and consumes offline codes (spec §6 offline lane).
///
/// B10: the FIRST offline sell of a session lazily mints a DocType=9 BEGIN@lnd1
/// (code#1) BEFORE the SELL, so `apply(OfflineSell)` returns the BUSINESS
/// Mutation (SELL@lnd2, code#2) and the model has consumed TWO codes.
#[test]
fn offline_sell_advances_seed_at_offline_local_ack_and_consumes_code() {
    let mut m = RefModel::new_offline_open_shift(3);
    let seed_before = m.seed;

    let out = m.apply(&Op::OfflineSell);

    let mu = mutation(&out);
    assert_eq!(mu.lnd, 2, "the returned Mutation is the SELL (lnd 2, after BEGIN@1)");
    assert_eq!(mu.doc_state, DocState::OfflineLocalAck);
    assert_eq!(m.docs.get(&1), Some(&DocState::OfflineLocalAck), "BEGIN@1 OLA");
    assert_eq!(m.docs.get(&2), Some(&DocState::OfflineLocalAck), "SELL@2 OLA");
    assert_ne!(m.seed, seed_before, "seed advanced at OFFLINE_LOCAL_ACK");
    assert_eq!(m.seed, mu.seed_after);
    assert_eq!(m.codes_consumed, 2, "two codes consumed — BEGIN + SELL");
    assert_eq!(mu.code_consumed, Some(2));
    // OFFLINE_LOCAL_ACK is in the SSOT issued set — the doc is issued at issuance.
    assert!(RefModel::is_offline_origin_issued(
        DocState::OfflineLocalAck
    ));
}

/// U1 D3 (POS tooth) — the model-local fork `MODEL_OFFLINE_ISSUED_STATES` MUST
/// equal the prod SSOT const `fiscal_documents::OFFLINE_ISSUED_STATES` as a set.
/// Pass-on-main / fail-on-drift: perturbing either side turns this RED — the
/// anti-shared-const guarantee (U1 D3), so a prod-side boundary change no longer
/// silently propagates into the differential model but demands a conscious update.
#[test]
fn teeth_d3_forked_set_matches_prod_const() {
    let model: std::collections::BTreeSet<&str> =
        model::MODEL_OFFLINE_ISSUED_STATES.iter().copied().collect();
    let prod: std::collections::BTreeSet<&str> = OFFLINE_ISSUED_STATES.iter().copied().collect();
    assert_eq!(
        model, prod,
        "model fork drifted from prod OFFLINE_ISSUED_STATES — reconcile consciously (U1 D3)"
    );
}

/// U1 D3 (NEG tooth) — the fork must NOT change issued/non-issued classification
/// of any known state: `is_offline_origin_issued` (now from the fork) still equals
/// prod-const membership for EVERY `DocState`.  Proves the fork is a pure
/// re-grounding of the SSOT, not a behaviour change.
#[test]
fn teeth_d3_membership_semantics_unchanged() {
    for state in ALL_DOC_STATES {
        assert_eq!(
            RefModel::is_offline_origin_issued(state),
            OFFLINE_ISSUED_STATES.contains(&state.as_str()),
            "fork changed issued membership for {state:?} (U1 D3 must be behaviour-preserving)"
        );
    }
}

/// U1 D7 (ONLINE-arm tooth — A.3 advance-at-SEND) — the ONLINE counterpart of the
/// D3 offline tooth above.  The model's online-origin seed-advance decision
/// (`RefModel::online_origin_advances_seed`, the SSOT `apply_sell` calls) MUST
/// equal the prod SSOT `fiscal_documents::is_issued` online arm for EVERY
/// `DocState`, under the PHYSICAL sfn-coupling: `server_fiscal_no` is stamped at
/// the `Sending→Sent` CAS (stage_send §6 step 3), so an online-origin doc carries
/// sfn ⟺ its state crossed SEND.  That coupling set is INDEPENDENT ground truth
/// about the write-path — consulted from NEITHER the model rule nor prod
/// `is_issued`, so the three code sites must agree.  Pass-on-main / fail-on-drift:
/// perturb the model's advance set OR prod's online arm and this turns RED (the
/// anti-shared-logic guarantee, mirroring D7 spec-lock), instead of the
/// differential silently blessing a prod/model divergence.
#[test]
fn teeth_d7_online_advance_matches_prod_is_issued() {
    for state in ALL_DOC_STATES {
        // Physical sfn-coupling (independent of both sides): sfn is present ⟺ the
        // online doc crossed the Sending→Sent CAS.
        let crossed_send = matches!(
            state,
            DocState::Sent | DocState::Kvt1 | DocState::Kvt2 | DocState::Ack
        );
        let sfn = if crossed_send { Some("70000001") } else { None };
        assert_eq!(
            RefModel::online_origin_advances_seed(state),
            is_issued(state.as_str(), None, sfn),
            "online-arm drift at {state:?}: model seed-advance != prod is_issued \
             (offline_fiscal_no=None, under the sfn-stamped-at-SEND coupling)"
        );
    }
}

// ── U1 D1 — predict/assert next_lnd (per-FN monotonic allocator) ────────────

/// U1 D1 (POS tooth) — `run_harness` must assert the model's PREDICTED allocator
/// (`next_lnd`) equals the DB SSOT (`node_state.next_lnd`, the `allocate_next_lnd`
/// sequencer — ADR-M3-A1).  A `DuplicateIdemKey` is a NoMutation op (mints no
/// doc), so `check_differential` CANNOT see a desynced allocator — only the D1
/// assert can.  We pre-desync `model.next_lnd = 99`; with the D1 assert in
/// `run_harness` this PANICS.  Revert target: the D1
/// `assert_eq!(model.next_lnd, read_next_lnd())` block — removing it lets the
/// NoMutation op pass with a stale allocator → no panic → this tooth FAILS.
#[test]
fn teeth_d1_next_lnd_predicts_db() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let ctx = interp::FuzzCtx::new_online_open_shift().await;
            let mut model = RefModel::new_online_open_shift();
            // Desync ONLY the allocator; the op mints no doc, so the doc-lnd
            // differential is blind to it — the D1 assert is the sole guard.
            model.next_lnd = 99;
            run_harness(&[Op::DuplicateIdemKey], ctx, model).await;
        });
    }))
    .is_err();
    std::panic::set_hook(prev);
    assert!(
        panicked,
        "U1 D1: run_harness must assert model.next_lnd == node_state.next_lnd for a non-fault \
         op — a NoMutation op with a desynced allocator returned cleanly, so the D1 \
         predict-then-assert is missing and the stale allocator was adopted silently."
    );
}

/// U1 D1 (NEG tooth) — a legitimate gapless lnd consumption is NOT flagged.  An
/// online reject mints a NON-ISSUED `Rejected` row: the lnd IS consumed (the
/// allocator bumps) but the doc is not issued.  The model predicts the bump, so
/// the D1 assert holds — `run_harness` completes without panic.
#[tokio::test]
async fn teeth_d1_gapless_reissue_not_flagged() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    // Must NOT panic — a consumed-but-not-issued lnd is a legal gapless bump.
    let _ = run_harness(&[Op::OnlineSell(DpsScript::send_then_reject())], ctx, model).await;
}

// ── U1 D2 — predict/assert mode + shift_state (before precondition-resync) ───

/// U1 D2 (POS tooth) — `run_harness` must assert the model's PREDICTED
/// `mode`/`shift_state` equals the DB BEFORE `adopt_precondition`
/// (which otherwise silently ADOPTS them).  A `DuplicateIdemKey` is a NoMutation
/// op that changes neither, so a pre-desynced `model.shift_state` survives every
/// other check — only the D2 assert catches it.  Revert target: the D2
/// `assert_eq!(model.shift_state, read_shift_state())` block — without it the
/// resync adopts the DB shift, masking the desync → no panic → this tooth FAILS.
#[test]
fn teeth_d2_predicted_shift_matches_db() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let ctx = interp::FuzzCtx::new_online_open_shift().await;
            let mut model = RefModel::new_online_open_shift();
            // Desync ONLY the predicted shift; the NoMutation op won't move it, so
            // the D2 pre-resync assert is the sole guard.
            model.shift_state = ShiftState::Closing;
            run_harness(&[Op::DuplicateIdemKey], ctx, model).await;
        });
    }))
    .is_err();
    std::panic::set_hook(prev);
    assert!(
        panicked,
        "U1 D2: run_harness must assert model shift_state/mode == DB before \
         resync_preconditions adopts them — a desynced prediction survived cleanly, so the \
         D2 predict-then-assert is missing and the divergence was adopted silently."
    );
}

/// U1 D2 (NEG tooth) — D2 must NOT assert on a FAULT-class op (the crash-window
/// residue → adopt_fault_deferred).  A `Reboot` with a deliberately-divergent
/// model `shift_state` must NOT trip D2 — the fault branch re-syncs via
/// `adopt_fault_deferred`, it does not predict-then-assert.
#[tokio::test]
async fn teeth_d2_mid_transition_deferral_not_flagged() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let mut model = RefModel::new_online_open_shift();
    model.shift_state = ShiftState::Closing; // divergent — but a Reboot is fault-class
                                             // Must NOT panic — D2 skips fault ops; adopt_fault_deferred re-syncs the residue.
    let _ = run_harness(&[Op::Reboot], ctx, model).await;
}

// ── U1 D5 — promote deterministic exotic-drain scripts to predicted Mutated ──

/// U1 D5 (POS tooth) — a `[Superseded]` drain is now DIFFERENTIAL-CHECKED, not
/// Fault-adopted.  Empirically probe-derived (the `classify_check_result`
/// Superseded arm applied by the strict-sequential drain): the HEAD backlog doc
/// → `ERROR_RETRYABLE` and the shift escalates to RMR (EscalateManual, M3b
/// §16.7); successors held at OFFLINE_LOCAL_ACK.  With the CORRECT promotion this
/// completes; a WRONG predicted terminal makes run_harness's ledger-delta PANIC
/// (the RED-derivation §7 #1).  Note: the parent §3/D5 "held in SENT" shorthand
/// was inaccurate for Superseded — the real terminal is ERROR_RETRYABLE + RMR.
#[tokio::test]
async fn teeth_d5_superseded_drain_predicts_error_retryable_rmr() {
    let ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
    let model = RefModel::new_offline_open_shift(3);
    let _ = run_harness(
        &[
            Op::OfflineSell,
            Op::OfflineSell,
            Op::GoOnline(DpsScript::superseded_tip()),
        ],
        ctx,
        model,
    )
    .await;
}

/// U1 D5 (POS tooth) — a `[Ack, NotFound]` drain is differential-checked: the
/// head doc → `SENT` (SentNotFoundDowngrade — held pending), shift unchanged,
/// successors held.  (Pre-D5 both exotic scripts routed to Fault → resync.)
#[tokio::test]
async fn teeth_d5_send_ack_notfound_drain_predicts_sent_held() {
    let ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
    let model = RefModel::new_offline_open_shift(3);
    let _ = run_harness(
        &[
            Op::OfflineSell,
            Op::OfflineSell,
            Op::GoOnline(DpsScript::send_ack_then_last_not_found()),
        ],
        ctx,
        model,
    )
    .await;
}

/// U1 D5 (NEG tooth) — a `[BadHashPrev]` (MAC-recovery) drain stays GENUINELY
/// deferred to Fault (§7 #1) — NOT force-promoted, so run_harness adopts via
/// resync and completes without a false differential.
#[tokio::test]
async fn teeth_d5_mac_recovery_drain_still_deferred_not_flagged() {
    let ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
    let model = RefModel::new_offline_open_shift(3);
    let _ = run_harness(
        &[
            Op::OfflineSell,
            Op::OfflineSell,
            Op::GoOnline(DpsScript::bad_hash_prev()),
        ],
        ctx,
        model,
    )
    .await;
}

// ── U1 D4 — BadHashPrev online sell: bounded MAC-recovery (no unbounded resend) ──

/// U1 D4 (NEG tooth) — a BadHashPrev online sell's SINGLE W10.4 MAC-recovery
/// re-entry is legitimate and NOT flagged: the wire send-count (original send +
/// at most one re-send) is within the D4 bound, so run_harness completes.  This
/// is the RED-first test — a too-tight bound makes it PANIC (revealing the real
/// send-delta); the correct bound passes.  The POS "unbounded resend is caught"
/// is proven by that RED evidence + the generative gate in the FaultOrRecovery
/// arm (a reverted src one-shot guard, stage_send.rs:970, would trip it) —
/// analogous to AUD-K8-1, whose over-budget breach also cannot be triggered from
/// tests without a forbidden `src/` change (CP4).
#[tokio::test]
async fn teeth_d4_single_recovery_reentry_not_flagged() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    let _ = run_harness(&[Op::OnlineSell(DpsScript::bad_hash_prev())], ctx, model).await;
}

/// U1 D4 (NEG tooth) — the D4 bound is SCOPED to the BadHashPrev MAC-recovery
/// path: a normal online sell (PredictableMutating, not Fault) is NOT subject to
/// it and completes unflagged.
#[tokio::test]
async fn teeth_d4_normal_online_sell_not_subject_to_bound() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    let _ = run_harness(&[Op::OnlineSell(DpsScript::ack_path())], ctx, model).await;
}

// ── PR-R-fuzz — RETURN in the alphabet (chain-wise identical to SELL) ────────

/// PR-R-fuzz — an ONLINE RETURN issues at the SEND boundary and advances the
/// seed, matching the model.  RED before `apply_return` is implemented (the
/// model arm `todo!()`s → `apply` panics inside `run_harness`); GREEN once
/// `apply_return` delegates to `apply_sell`.
#[tokio::test]
async fn online_return_ack_path_matches_model() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    let _ = run_harness(&[Op::OnlineReturn(DpsScript::ack_path())], ctx, model).await;
}

/// PR-R-fuzz — an OFFLINE RETURN consumes an offline code and issues at
/// OFFLINE_LOCAL_ACK exactly like an offline SELL (symmetry (a): the offline-
/// code CAS `acquire_code_tx` is doc-type-agnostic).  RED under the `todo!()`
/// model arm; GREEN once `apply_return` delegates to `apply_sell`.
#[tokio::test]
async fn offline_return_consumes_code_matches_model() {
    let ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
    let model = RefModel::new_offline_open_shift(3);
    let _ = run_harness(&[Op::OfflineReturn], ctx, model).await;
}

/// PR-R-fuzz — a mixed SELL+RETURN online sequence stays chain-continuous (each
/// doc chains onto the prior tip; the seed advances once per issued doc),
/// matching the model across the interleave.
#[tokio::test]
async fn mixed_sell_return_sequence_matches_model() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    let _ = run_harness(
        &[
            Op::OnlineSell(DpsScript::ack_path()),
            Op::OnlineReturn(DpsScript::ack_path()),
            Op::OnlineSell(DpsScript::ack_path()),
        ],
        ctx,
        model,
    )
    .await;
}

/// PR-R-fuzz — the D5 acquire gate refuses an ONLINE RETURN while a non-issued
/// sibling rests (symmetry (b): `exists_blocking_non_issued_sibling` is
/// doc-type-agnostic).  A `Crash(Send)` leaves a SENDING (non-issued) sibling;
/// the following RETURN is acquire-refused (NoMutation, no new issued row) — the
/// model predicts it via the same `has_write_gate_blocker` path a SELL uses.
#[tokio::test]
async fn online_return_d5_gated_by_non_issued_sibling() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    let _ = run_harness(
        &[
            Op::Crash(Stage::Send),
            Op::OnlineReturn(DpsScript::ack_path()),
        ],
        ctx,
        model,
    )
    .await;
}

/// PR-R-fuzz — the interp drives a GENUINE `RETURN` doc into the ledger.  The
/// chain differential cannot distinguish a SELL from a RETURN (chain-identical),
/// so the wire doc-type is pinned directly here.  Teeth (b): revert
/// `online_return` to seed a SELL row → this RED.
#[tokio::test]
async fn online_return_produces_a_genuine_return_doc() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let outcome = interp::run_op(&mut ctx, &Op::OnlineReturn(DpsScript::ack_path())).await;
    assert!(
        matches!(outcome, interp::RealOutcome::Doc(_)),
        "an ACK-path RETURN must issue a doc, got {outcome:?}"
    );
    assert_eq!(
        ctx.only_doc_type().await,
        "RETURN",
        "the fuzzer must drive a genuine RETURN, not a mislabeled SELL"
    );
}

/// PR-R-fuzz — the OFFLINE lane also drives a GENUINE `RETURN` doc (symmetry
/// with the online genuine-return pin; `build_canonical` maps
/// operation_type→doc_type mode-independently).
#[tokio::test]
async fn offline_return_produces_a_genuine_return_doc() {
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
    let outcome = interp::run_op(&mut ctx, &Op::OfflineReturn).await;
    // B10: the first offline doc lazily interposes a DocType=9 BEGIN, so the op is
    // a two-doc ledger delta → the interp reports `Recovered` (routes to the
    // ledger-delta oracle), and there are now BEGIN + RETURN rows (so
    // `only_doc_type` no longer applies).
    assert!(
        matches!(
            outcome,
            interp::RealOutcome::Recovered { .. } | interp::RealOutcome::Doc(_)
        ),
        "an OFFLINE RETURN must issue a doc (or Recovered w/ interposed BEGIN), got {outcome:?}"
    );
    assert_eq!(
        ctx.count_doc_type("RETURN").await,
        1,
        "the offline fuzzer lane must drive exactly one genuine RETURN, not a mislabeled SELL"
    );
    assert_eq!(
        ctx.count_doc_type("OFFLINE_SESSION_BEGIN").await,
        1,
        "the lazy BEGIN is interposed before the RETURN"
    );
    assert_eq!(
        ctx.count_doc_type("SELL").await,
        0,
        "no SELL row (the RETURN is genuine, and the BEGIN is a service receipt)"
    );
}

/// PR-R-fuzz — a DPS-rejected online RETURN mints a NON-ISSUED `Rejected` row
/// (lnd consumed, seed NOT advanced) → the model's `NoIssuanceRow`, the same
/// `Sending→Rejected` branch a rejected SELL takes.  Exercises the reject
/// differential arm for a RETURN (the ack-path / D5 pins do not).
#[tokio::test]
async fn online_return_reject_matches_model_no_issuance_row() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    let _ = run_harness(
        &[Op::OnlineReturn(DpsScript::send_then_reject())],
        ctx,
        model,
    )
    .await;
}

/// PR-R-fuzz — a BadHashPrev online RETURN routes to the bounded W10.4
/// MAC-recovery path exactly like its SELL twin (doc-type-agnostic, symmetry
/// (c)): the model Fault-defers and run_harness's D4 send-delta bound (now
/// scoped to OnlineSell|OnlineReturn) asserts no unbounded resend.  Exercises
/// the extended D4 gate for a RETURN.
#[tokio::test]
async fn online_return_bad_hash_prev_within_d4_bound() {
    let ctx = interp::FuzzCtx::new_online_open_shift().await;
    let model = RefModel::new_online_open_shift();
    let _ = run_harness(&[Op::OnlineReturn(DpsScript::bad_hash_prev())], ctx, model).await;
}

/// PR-R-fuzz (anti-silent-zero) — the generator ACTUALLY emits both Return ops
/// over a large deterministic draw, so Return / mixed sequences are really
/// exercised by the property harness.  A dropped or zero weight makes this RED
/// loudly instead of silently never testing Returns.
#[test]
fn generator_emits_online_and_offline_returns() {
    let mut runner = TestRunner::deterministic();
    let strat = strategy::op_sequence();
    let mut online = 0usize;
    let mut offline = 0usize;
    for _ in 0..2000 {
        let seq = strat.new_tree(&mut runner).unwrap().current();
        for op in &seq {
            match op {
                Op::OnlineReturn(_) => online += 1,
                Op::OfflineReturn => offline += 1,
                _ => {}
            }
        }
    }
    // Density floor, not just `> 0`: at the Sell-equal weight (1/14) over a
    // 2000-seq × ~4.5-avg-len draw the expected count is ≈ 643 per variant, so
    // `>= 100` is astronomically non-flaky yet ALSO catches a future explicit
    // under-weighting (not only an arm outright dropped to zero).
    assert!(
        online >= 100,
        "OnlineReturn under-emitted ({online} over 2000 seqs) — the Return weight \
         is dropped or severely skewed (anti-silent-zero)"
    );
    assert!(
        offline >= 100,
        "OfflineReturn under-emitted ({offline} over 2000 seqs) — the Return weight \
         is dropped or severely skewed (anti-silent-zero)"
    );
}

// ── Lane-correctness reinforcements (pure model behaviours) ─────────────────

/// A DPS reject of an online doc → `inline::run` returns Err(DpsRejected) → the
/// interpreter reports Refused.  B2: the model reports `NoIssuanceRow` (NOT
/// `NoMutation`) — a NON-ISSUED Rejected row IS minted + the lnd consumed, but
/// no receipt is issued (the seed does not advance).
#[test]
fn online_sell_reject_is_no_issuance_row_with_non_issued_rejected_row() {
    let mut m = RefModel::new_online_open_shift();
    let out = m.apply(&Op::OnlineSell(DpsScript::send_then_reject()));
    assert_eq!(out, ExpectedOutcome::NoIssuanceRow);
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
/// shape).  A.3: reaching SENT crosses the online issuance threshold, so the
/// seed ADVANCES here (advance-at-SEND) — no longer deferred to ACK.
#[test]
fn online_sell_ack_then_lastchk_not_found_holds_at_sent() {
    let mut m = RefModel::new_online_open_shift();
    let out = m.apply(&Op::OnlineSell(DpsScript::send_ack_then_last_not_found()));
    assert_eq!(mutation(&out).doc_state, DocState::Sent);
    assert!(
        m.seed.is_some(),
        "A.3: SENT crosses the issuance threshold → the seed advances at SEND"
    );
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
    // B10: the first offline sell mints BEGIN(lnd1) + SELL(lnd2) → seed 3 codes so
    // both issue AND the drain-time END(lnd3) can too.
    let mut m = RefModel::new_offline_open_shift(3);
    let _ = m.apply(&Op::OfflineSell); // BEGIN@1 + SELL@2, both OFFLINE_LOCAL_ACK
    assert_eq!(m.docs.get(&1), Some(&DocState::OfflineLocalAck), "BEGIN@1 OLA");
    assert_eq!(m.docs.get(&2), Some(&DocState::OfflineLocalAck), "SELL@2 OLA");
    let seed_before = m.seed;
    m.mode = NodeMode::GoingOnline; // fixture: the probe already flipped (test setup)

    let out = m.apply(&Op::Drain(DpsScript::ack_path()));

    assert!(
        matches!(out, ExpectedOutcome::Mutated(_)),
        "an advancing drain is PredictableMutating, got {out:?}"
    );
    assert_eq!(m.docs.get(&1), Some(&DocState::Ack), "BEGIN drained to ACK");
    assert_eq!(m.docs.get(&2), Some(&DocState::Ack), "SELL drained to ACK");
    // B10: the drain also minted + drained the DocType=10 END (lnd3 → ACK).
    assert_eq!(m.docs.get(&3), Some(&DocState::Ack), "END minted + drained to ACK");
    // The drained backlog (BEGIN + SELL) does NOT re-advance the seed (offline
    // advanced at issuance); the END, however, is a fresh offline doc issued AT
    // drain, so it DOES advance the seed to its own unsigned hash (M2-01).
    assert_ne!(
        m.seed, seed_before,
        "the drain-time END advances the seed to its own unsigned hash"
    );
    assert_eq!(
        m.mode,
        NodeMode::Online,
        "a full drain (with a spare code for the END) flips GoingOnline → Online"
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
    // B10: BEGIN@1 + SELL@2 at issuance + END@3 at drain → seed 3 codes.
    let mut m = RefModel::new_offline_open_shift(3);
    let _ = m.apply(&Op::OfflineSell);

    let out = m.apply(&Op::GoOnline(DpsScript::ack_path()));

    assert!(matches!(out, ExpectedOutcome::Mutated(_)));
    assert_eq!(
        m.mode,
        NodeMode::Online,
        "go_online: Offline → (GoingOnline) → Online (spare code for the END)"
    );
    assert_eq!(m.docs.get(&1), Some(&DocState::Ack), "BEGIN drained to ACK");
    assert_eq!(m.docs.get(&2), Some(&DocState::Ack), "SELL drained to ACK");
    assert_eq!(m.docs.get(&3), Some(&DocState::Ack), "END drained to ACK");
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
    model.adopt_fault_deferred(&ctx.pool).await;
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
            // O1 (CP2 re-scope): the online-convergence drive+assert is DIRECTED-ONLY
            // (`teeth_o1_*`), NOT wired here.  Driving it in the random net converges
            // GENERATIVELY-STACKED SENT docs — multiple sells that each rest at SENT
            // were signed against the SAME (un-advanced) seed, so acking them all
            // chain-breaks (the lnd-2+ docs never chained onto lnd-1's post-ACK tip).
            // That stacked-then-all-converge state is an artifact the generator
            // reaches but real single-writer convergence escalates on (ChainSeedMismatch)
            // — exactly the P1 "directed-only, generatively-unreachable" precedent.
            // See TEETH_TEST.md + the architect note.
            oracle::assert_clean(&ctx.pool).await;
            if let Err(d) = oracle::check_mirrors(&ctx.pool).await {
                panic!("terminal mirror drift (mode={mode:?} shift={shift:?}): {d:?}");
            }
            // O3: catch a stored-hash/payload divergence the referential oracle misses.
            if let Err(d) = oracle::check_payload_hash_integrity(&ctx.pool).await {
                panic!(
                    "terminal payload-hash integrity (O3) (mode={mode:?} shift={shift:?}): {d:?}"
                );
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
            // O5: previously this branch SKIPPED the scan entirely (a deferred
            // online-origin SENDING would false-flag StuckSending).  Run the scan
            // but EXCUSE only that StuckSending variant — every OTHER violation
            // (chain break / leaked pre-send doc / duplicate lnd / …) stays fatal,
            // closing the blind spot where this terminal was never scanned.
            let violations = prro::db::invariant_scan::scan(&ctx.pool)
                .await
                .expect("invariant_scan query");
            if let Err(d) = oracle::filter_artifact_violations(violations) {
                panic!("terminal ArtifactNoResend scan (mode={mode:?} shift={shift:?}): {d:?}");
            }
        }
    }
}

/// Per-op dispatch gluing T1-T6: model.apply + run_op, then assert per the op's
/// classification, with the T5 scan-timing rule (never mid-crash) and re-sync
/// after a fault.
async fn run_harness(ops: &[Op], mut ctx: interp::FuzzCtx, mut model: RefModel) -> interp::FuzzCtx {
    // A crash transient opened but not yet resolved by a reboot (A1 scan-gate
    // suppresses the scan while set; A3 asserts the no-resend postcond on the
    // resolving reboot).
    let mut pending_crash = false;
    // U3 realism: a STAGE-COMPOSITION crash (Sign/Finalize/OfflineAck) models
    // PROCESS death — single-writer + boot-recon-before-serve means a crashed
    // gateway serves no new request before recovery, so every op until the
    // resolving Reboot is SKIPPED (pre-U3 a `[Crash(Sign), OnlineSell, …]`
    // buried the crashed SIGNED doc under later issuance — an unreachable
    // production state that confuses recovery into a FALSE StuckNonTerminalDoc,
    // see `dead_until_reboot_skips_ops_after_composition_crash`).  Wire crashes
    // (Send/Kvt1) are TRANSPORT collapse — the process may live on, later ops
    // legitimately run (the nightly-find 0627 class) — so they do NOT set this.
    let mut dead_until_reboot = false;
    // U3: a WIRE crash (Send/Kvt1 — the doc already hit the wire) is pending;
    // gates the A3 no-resend assert (composition-crash resumes legitimately
    // perform a FIRST send — see the A3 comment below).
    let mut pending_wire_crash = false;
    for op in ops {
        if dead_until_reboot && !matches!(op, Op::Reboot | Op::RepeatReboot) {
            continue; // process is dead — no op reaches the gateway before reboot
        }
        let prior_tip = ctx.read_seed().await; // real MAC tip BEFORE the op
        let seed_at_op_start = model.seed; // model MAC tip BEFORE the op (B10 B3 structural seed check)
        let codes_before = ctx.consumed_codes_count().await;
        let sends_before = ctx.send_calls(); // wire send count BEFORE the op (A3 no-resend)
        let shift_before = ctx.read_shift_state().await; // real shift_state BEFORE the op
        let cohort_before = ctx.full_drain_cohort_count().await; // MH: drain re-drives ≤ cohort
        let next_lnd_before = ctx.read_next_lnd().await; // MH/B2: a drain/no-op allocates no lnd
        let doc_count_before = ctx.observed_doc_count().await; // B2: TrueNoMutation mints no row

        let real = interp::run_op(&mut ctx, op).await;
        // O2: an Offline-node Crash that never reached the wire COMPLETES as a real
        // offline sell (RealOutcome::Doc) — a DETERMINISTIC mutation, not a fault.
        // Predict it (DB-read-independent) so the crash differential is NOT vacuous
        // (pre-O2: Op::Crash → Fault → check_differential Ok(()) → the FaultOrRecovery
        // arm resync'd, silently adopting the real DB).  A wire-reached Crash
        // (RealOutcome::Crashed) and every Reboot stay Fault.
        let expected = match (op, &real) {
            // O2: an offline-node crash completes as a real offline sell.  With
            // B10 it may interpose a BEGIN (→ `Recovered`, two-doc) or not (→
            // `Doc`, single); either way predict it deterministically via the
            // two-doc-aware `predict_crash_completed_sell` (→ `apply_sell`) so the
            // crash differential is NOT vacuous (see the `b10_lazy_begin_interposed`
            // ledger-delta branch below for the Recovered case).
            (Op::Crash(_), interp::RealOutcome::Doc(_))
            | (Op::Crash(_), interp::RealOutcome::Recovered { .. }) => {
                model.predict_crash_completed_sell()
            }
            _ => model.apply(op),
        };

        let class = oracle::classify(&expected);
        match class {
            // Fault / recovery — we do NOT predict recovery; adopt the real DB.
            oracle::OpClass::FaultOrRecovery => {
                // MH (B1): a Fault-DEFERRED DRAIN (exotic wire script / mid-wire
                // cohort the model cannot cleanly predict) was previously BLINDLY
                // resync'd, leaving the exotic-drain path UNVERIFIED.  Assert the
                // bounded SAFETY postconds FIRST, so an erroneous exotic drain is
                // CAUGHT, not adopted.  (Crash / Reboot Faults are NOT drains —
                // they are covered by A3's no-resend below.)
                if matches!(
                    op,
                    Op::Drain(_) | Op::RepeatDrain | Op::GoOnline(_) | Op::GoOnlineWithoutBacklog
                ) {
                    // 1. No offline code consumed — codes are consumed at ISSUANCE,
                    //    never by a drain (which re-drives already-issued docs).
                    assert_eq!(
                        ctx.consumed_codes_count().await,
                        codes_before,
                        "MH: exotic drain {op:?} consumed an offline code"
                    );
                    // 2. next_lnd unchanged — a drain allocates NO new lnd.
                    assert_eq!(
                        ctx.read_next_lnd().await,
                        next_lnd_before,
                        "MH: exotic drain {op:?} allocated a new lnd"
                    );
                    // 3. Seed unchanged — offline-origin advanced the MAC seed at
                    //    issuance (OFFLINE_LOCAL_ACK); a drain re-drives, never
                    //    re-advances it.
                    assert_eq!(
                        ctx.read_seed().await,
                        prior_tip,
                        "MH: exotic drain {op:?} advanced the MAC seed"
                    );
                    // 4. Send-delta bounded by the cohort — the drain re-drives
                    //    each cohort doc a BOUNDED number of times (no unbounded
                    //    resend loop); 2×+1 allows one MAC-recovery retry per doc.
                    let send_delta = ctx.send_calls() - sends_before;
                    assert!(
                        send_delta <= 2 * cohort_before + 1,
                        "MH: exotic drain {op:?} send-delta {send_delta} exceeds \
                         2×cohort({cohort_before})+1 — unbounded re-drive"
                    );
                    // 5. Shift unchanged OR escalated to RMR — a drain either makes
                    //    progress (shift unchanged) or halts-manual (RMR); never
                    //    some other shift transition.
                    let shift_after = ctx.read_shift_state().await;
                    assert!(
                        shift_after == shift_before
                            || shift_after == ShiftState::RequiresManualReconciliation,
                        "MH: exotic drain {op:?} moved shift {shift_before:?} -> {shift_after:?} \
                         (neither unchanged nor RMR)"
                    );
                }
                // U1 D4 — a BadHashPrev online sell routes to the bounded W10.4
                // MAC-recovery path (DDL `mac_recovery_attempts CHECK IN (0,1)` +
                // the `mac_recovery_invoked` one-shot flag, stage_send.rs:951/970):
                // AT MOST ONE re-sign + re-send per `run()`.  The pure model
                // defers the terminal (Fault), but the WIRE send-count must stay
                // bounded — no unbounded resend.  A regression that removed the
                // one-shot guard would resend without limit; this generative gate
                // catches it (like AUD-K8-1's wire-call bound).  PR-R-fuzz — a
                // BadHashPrev online RETURN takes the SAME doc-type-agnostic
                // MAC-recovery path (symmetry (c)), so it is bounded identically.
                if matches!(
                    op,
                    Op::OnlineSell(s) | Op::OnlineReturn(s)
                        if matches!(s.0.as_slice(), [WireResponse::BadHashPrev, ..])
                ) {
                    let send_delta = ctx.send_calls() - sends_before;
                    // Probe-derived: the real send-delta is exactly 1 (the single
                    // original send; the W10.4 re-sign's re-send hits the stub's
                    // empty queue → terminal, no second wire call).  Bound at the
                    // exact threshold, so ANY extra resend (a reverted one-shot
                    // guard → send-delta ≥ 2) is caught.
                    assert!(
                        send_delta <= 1,
                        "U1 D4: BadHashPrev online sell wire send-delta {send_delta} exceeds the \
                         bounded MAC-recovery budget (original send + at most one W10.4 re-send)"
                    );
                }
                model.adopt_fault_deferred(&ctx.pool).await;
            }
            // Predictable mutation — differential-match the model.
            oracle::OpClass::PredictableMutating => {
                if let Err(d) = oracle::check_differential(&real, &expected, prior_tip.as_deref()) {
                    panic!("differential divergence on {op:?}: {d:?}");
                }
                // drain / go-online carry no per-doc detail → ledger-delta.
                if let interp::RealOutcome::Recovered { branch } = &real {
                    let real_ledger = ctx.read_ledger().await;
                    if let Err(d) = oracle::check_ledger_delta(&model.docs, &real_ledger) {
                        panic!("ledger-delta divergence on {op:?}: {d:?}");
                    }
                    if branch == "b10_lazy_begin_interposed" {
                        // B10 — the FIRST offline doc lazily interposed a DocType=9
                        // BEGIN.  This op DID consume 2 codes + allocate 2 lnds +
                        // advance the seed (the B3 no-mutation invariants below do
                        // NOT apply — they are for drain/go-online RE-DRIVE).  TEETH:
                        // verify the two-doc chain linkage + code accounting so a
                        // reverted BEGIN-chain REDs (proven by the canary
                        // `teeth_b10_reverted_begin_chain_reddens_ledger_delta`).
                        assert_eq!(
                            ctx.consumed_codes_count().await,
                            codes_before + 2,
                            "B10: first-offline op must consume EXACTLY 2 codes \
                             (BEGIN + business): {op:?}"
                        );
                        assert_eq!(
                            ctx.read_next_lnd().await,
                            next_lnd_before + 2,
                            "B10: first-offline op must allocate EXACTLY 2 lnds \
                             (BEGIN + business): {op:?}"
                        );
                        if let Err(d) = ctx
                            .assert_b10_boundary_chain_linked(prior_tip.as_deref())
                            .await
                        {
                            panic!("B10 boundary-chain divergence on {op:?}: {d}");
                        }
                    } else {
                        // B3 — FULL snapshot beyond lnd→state.  A recovered drain /
                        // go-online RE-DRIVES already-issued docs (no code / no lnd /
                        // no seed change) EXCEPT for the B10 DocType=10 END, which a
                        // finalizing drain mints LAST (consuming one code + one lnd).
                        // The MODEL predicts the END independently (`drain_backlog`
                        // AckPath), so assert the real post-op consumed-codes /
                        // next-lnd match the MODEL's post-op values — this keeps the
                        // teeth (a spurious extra code/lnd, or a MISSING END, REDs)
                        // while admitting the one legit END mint.
                        //
                        // `codes_before` / `next_lnd_before` are used as a lower
                        // bound sanity: the model's values are >= the pre-op values.
                        let end_minted = model.session_has_end
                            && model.docs.values().any(|s| {
                                matches!(s, DocState::Ack | DocState::Signed)
                            });
                        let _ = end_minted; // documentation of the delta source
                        assert_eq!(
                            ctx.consumed_codes_count().await as i64,
                            model.codes_consumed,
                            "B3: recovered drain/go-online {op:?} — real consumed-codes must \
                             equal the model's (which predicts the END's code)"
                        );
                        assert!(
                            model.codes_consumed >= codes_before as i64,
                            "B3 sanity: model codes_consumed never decreases"
                        );
                        assert_eq!(
                            ctx.read_next_lnd().await,
                            model.next_lnd,
                            "B3: recovered drain/go-online {op:?} — real next_lnd must equal \
                             the model's (which predicts the END's lnd)"
                        );
                        assert!(
                            model.next_lnd >= next_lnd_before,
                            "B3 sanity: model next_lnd never decreases"
                        );
                        // Seed is compared STRUCTURALLY (model uses synthetic
                        // hashes, reality real crypto hashes — never value-equal):
                        // the real seed must CHANGE iff the model's seed changed.
                        // A pure re-drive leaves both unchanged; a finalizing drain
                        // that issues the B10 END advances BOTH (the END's M2-01
                        // OLA).  A spurious real re-advance without a model advance
                        // (or vice-versa) REDs.
                        let real_seed_after = ctx.read_seed().await;
                        let real_advanced = real_seed_after.as_deref() != prior_tip.as_deref();
                        let model_advanced = model.seed != seed_at_op_start;
                        assert_eq!(
                            real_advanced, model_advanced,
                            "B3: recovered drain/go-online {op:?} — real seed-advance \
                             ({real_advanced}) must match the model's ({model_advanced})"
                        );
                    }
                }
            }
            // No mutation — the differential is permissive here, so the harness
            // independently asserts NO ISSUANCE (else an erroneously-mutating
            // invalid op slips through).
            // B2 — TrueNoMutation: a refusal / replay refused BEFORE any row is
            // written.  STRICT: the ledger is ENTIRELY unchanged (no row, no lnd,
            // no seed, no code) — a leaked row is caught HERE, not at a later
            // ledger-delta.
            oracle::OpClass::ExpectedNoMutation => {
                if let Err(d) = oracle::check_differential(&real, &expected, prior_tip.as_deref()) {
                    panic!("no-mutation differential on {op:?}: {d:?}");
                }
                assert_eq!(
                    ctx.observed_doc_count().await,
                    doc_count_before,
                    "ExpectedNoMutation {op:?} minted a fiscal_documents row (a true no-op must not)"
                );
                assert_eq!(
                    ctx.read_next_lnd().await,
                    next_lnd_before,
                    "ExpectedNoMutation {op:?} allocated an lnd (a true no-op must not)"
                );
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
            // B2 — NoIssuanceRowAllowed: a refusal that mints a LEGAL non-issued
            // row (online-reject Rejected / offline-ack Aborted).  The row IS
            // allowed (the lnd is consumed → next_lnd may bump), but it is NOT
            // issued: the row matches the model's predicted non-issued state
            // (ledger-delta) AND the seed + codes do NOT move.
            oracle::OpClass::ExpectedNoIssuanceRow => {
                if let Err(d) = oracle::check_differential(&real, &expected, prior_tip.as_deref()) {
                    panic!("no-issuance-row differential on {op:?}: {d:?}");
                }
                let real_ledger = ctx.read_ledger().await;
                if let Err(d) = oracle::check_ledger_delta(&model.docs, &real_ledger) {
                    panic!("no-issuance-row ledger-delta on {op:?}: {d:?}");
                }
                assert_eq!(
                    ctx.read_seed().await,
                    prior_tip,
                    "ExpectedNoIssuanceRow {op:?} advanced the seed (a refused row is NOT issued)"
                );
                assert_eq!(
                    ctx.consumed_codes_count().await,
                    codes_before,
                    "ExpectedNoIssuanceRow {op:?} consumed a code (a refused row is NOT issued)"
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

        // U1 D1 — next_lnd is PREDICTED, not adopted, for non-fault ops.  The
        // model advances its allocator per issuing op (`apply`); assert that
        // prediction equals the DB SSOT `node_state.next_lnd` (the
        // `allocate_next_lnd` sequencer, ADR-M3-A1) — per-FN monotonic, no-gap
        // (`ux_fd_fn_lnd`).  This catches an allocator drift the DOC-lnd
        // differential CANNOT: a NoMutation op has no doc, and a missed increment
        // leaves the doc correct but the allocator stale.  Fault ops cannot
        // predict the crash-window allocation → they adopt via `adopt_fault_deferred`
        // (the classified deferral, §4 funnel), so D1 skips them.
        if !matches!(class, oracle::OpClass::FaultOrRecovery) {
            assert_eq!(
                model.next_lnd,
                ctx.read_next_lnd().await,
                "U1 D1: model next_lnd prediction diverged from node_state.next_lnd on {op:?}"
            );
        }

        // U1 D2 — mode / shift_state are PREDICTED, not adopted, for non-fault
        // ops.  `apply` sets both from the M3b 9-state shift machine + the
        // node-mode machine (enums.rs); assert the prediction equals the DB.
        // Fault ops adopt via `adopt_fault_deferred` (adopt_fault_deferred), so D2 skips
        // them.  §7 #3 RESIDUE SPLIT: the drain / go-online MODE outcome (the
        // `GoingOnline → Online` CAS in `drain_backlog`) is a MID-TRANSITION
        // residue the pure model cannot pin — a FORCED `GoingOnline`
        // (`OfflineSellDuringGoingOnline`) does not complete to `Online` the way a
        // real go-online does, so `drain_backlog`'s empty-backlog CAS over-predicts
        // `Online` where reality stays `GoingOnline`.  Those ops therefore DEFER
        // mode to `adopt_precondition` (adopt_precondition); their SHIFT
        // (RMR / unchanged) IS predicted and asserted.
        if !matches!(class, oracle::OpClass::FaultOrRecovery) {
            assert_eq!(
                model.shift_state,
                ctx.read_shift_state().await,
                "U1 D2: model shift_state prediction diverged from node_state.shift_state on {op:?}"
            );
            let mode_is_transition_residue = matches!(
                op,
                Op::GoOnline(_) | Op::GoOnlineWithoutBacklog | Op::Drain(_) | Op::RepeatDrain
            );
            if !mode_is_transition_residue {
                assert_eq!(
                    model.mode,
                    ctx.read_node_mode().await,
                    "U1 D2: model mode prediction diverged from node_state.mode on {op:?}"
                );
            }
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
            // U3: a stage-composition crash is a PROCESS death — hold every op
            // until the resolving Reboot (see `dead_until_reboot` above).
            if matches!(
                real,
                interp::RealOutcome::Crashed {
                    stage: Stage::Sign | Stage::Finalize | Stage::OfflineAck,
                    ..
                }
            ) {
                dead_until_reboot = true;
            } else {
                pending_wire_crash = true;
            }
        }
        // A3: a reboot that RESOLVES a pending WIRE crash must NOT re-send (DPS
        // does not dedup → a blind resend double-fiscalises) — assert the
        // no-resend bounded postcond IN the property harness (was: faults only
        // resync, silently adopting a resend).  Only no-resend is asserted here:
        // the exact terminal AND the Kvt1 PROBE vary under composition (a SENT
        // doc at GoingOnline is DEFERRED to the W9 drain → no probe THIS reboot),
        // so those stay pinned in the directed K3/K4 tests.
        // U3 scope: WIRE crashes only, and only when NO composition crash is
        // also pending — a composition-crash doc (SIGNED/OFFLINE_LOCAL_ACK)
        // was never sent, so its resume legitimately performs a FIRST send
        // (not a re-send); asserting no-resend there would be a false alarm.
        if matches!(op, Op::Reboot | Op::RepeatReboot) && pending_crash {
            if pending_wire_crash && !dead_until_reboot {
                if let Err(d) = oracle::assert_no_resend(sends_before, ctx.send_calls()) {
                    panic!("crash-recovery resend on {op:?}: {d:?}");
                }
            }
            pending_crash = false;
            pending_wire_crash = false;
        }
        // U3: the Reboot ran boot reconciliation — the process is alive again.
        if matches!(op, Op::Reboot | Op::RepeatReboot) {
            dead_until_reboot = false;
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
            // O3: the referential chain oracle trusts the stored hash; recompute
            // sha256(PAYLOAD_XML) and catch a stored-hash/payload divergence.
            if let Err(d) = oracle::check_payload_hash_integrity(&ctx.pool).await {
                panic!("payload-hash integrity (O3) on {op:?}: {d:?}");
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
            model.adopt_precondition(&ctx.pool).await;
        }
    }

    // A2/A4: terminal liveness + scan.  The per-op scan is suppressed mid-crash
    // and mid-transition; without a terminal pass an unpaired crash or a
    // GoingOnline terminal would NEVER be recovered/scanned.  This drives bounded
    // REAL recovery until the node settles, then scans (or fails liveness on a
    // genuine non-settling settle-path; a forced-mode artifact asserts no-resend).
    settle_and_scan(&mut ctx, pending_crash).await;
    // U3: hand the ctx back so directed pins can inspect the settled ledger
    // (e.g. the dead-until-reboot doc-count pin) — the capstones ignore it.
    ctx
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

/// U3 — `drive` variant returning the SETTLED ledger doc count, for directed
/// pins that assert HOW MANY docs a sequence minted (the full harness — model,
/// differential, scans — still runs; this only adds the final count read).
fn drive_counting(ops: &[Op], offline: bool) -> i64 {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let ctx = if offline {
            run_harness(
                ops,
                interp::FuzzCtx::new_offline_open_shift(3).await,
                RefModel::new_offline_open_shift(3),
            )
            .await
        } else {
            run_harness(
                ops,
                interp::FuzzCtx::new_online_open_shift().await,
                RefModel::new_online_open_shift(),
            )
            .await
        };
        ctx.observed_doc_count().await
    })
}

// ─── U3 directed pins ───────────────────────────────────────────────────────

/// Nightly find 2026-06-27 — LITERAL pin (strategy-independent).  The committed
/// corpus seed (`cc e7d4ce…`) regenerates its sequence through the CURRENT
/// strategy, so U3's widened crash pool re-maps that seed to a different
/// sequence — the seed alone no longer covers the `apply_go_online`
/// GoingOnline-start fix.  This pins the exact shrunk sequence literally,
/// immune to strategy/proptest changes.
#[test]
fn nightly_find_0627_go_online_from_going_online_literal_pin() {
    let ops = vec![
        Op::Crash(Stage::Send),
        Op::OfflineSellDuringGoingOnline,
        Op::GoOnline(DpsScript(vec![WireResponse::Ack, WireResponse::Ack])),
        Op::OnlineSell(DpsScript(vec![WireResponse::Reject])),
    ];
    // The offline-seeded lane is where the find fired (model kept the
    // crash-completed offline sell at OfflineLocalAck while the real GoOnline
    // drained it to Ack).  `drive` runs the full differential — a regression
    // re-fails here deterministically.
    drive(&ops, true);
}

/// U3 realism pin — a stage-composition crash (`Sign`/`OfflineAck`) is a
/// PROCESS death: no op may reach the gateway until the resolving `Reboot`
/// (single-writer + boot-recon-before-serve).  Pre-U3 the harness ran the ops
/// anyway, so `[Crash(Sign), OnlineSell, …]` minted a second doc BURYING the
/// crashed SIGNED one — an unreachable production state (the reason
/// `Crash(Sign)` was directed-only).  With dead-until-reboot the `OnlineSell`
/// is SKIPPED: exactly ONE doc exists after settle (the crashed one, resumed
/// by boot).  RED pre-realism: the buried sequence mints 2 docs.
#[test]
fn dead_until_reboot_skips_ops_after_composition_crash() {
    let ops = vec![
        Op::Crash(Stage::Sign),
        Op::OnlineSell(DpsScript::ack_path()),
        Op::Reboot,
    ];
    let docs = drive_counting(&ops, true);
    assert_eq!(
        docs, 1,
        "dead-until-reboot: an op after a stage-composition crash must be \
         SKIPPED (process dead) — got {docs} docs (2 = the buried-SIGNED \
         artifact the realism removes)"
    );
}

/// U3 / O4 — `Crash(OfflineAck)` (the #192 birth-site window) is reachable and
/// recoverable: the offline-ack envelope committed OFFLINE_LOCAL_ACK (code
/// consumed, seed advanced at issuance), the process died before the inbox
/// finalize, and the resolving Reboot converges WITHOUT double-issuance —
/// exactly one issued doc, full settled scan clean (run by the harness).
#[test]
fn crash_after_offline_ack_reboot_converges_single_issue() {
    let ops = vec![Op::Crash(Stage::OfflineAck), Op::Reboot];
    let docs = drive_counting(&ops, true);
    assert_eq!(
        docs, 1,
        "Crash(OfflineAck)+Reboot must converge to exactly ONE issued doc \
         (no loss, no double-issuance) — got {docs}"
    );
}

/// Phase-2 U3 (spec §5): the capstone case count, driven by a DEDICATED
/// `FUZZ_CASES` env knob (NOT the global `PROPTEST_CASES`, which would also
/// inflate the `:288` smoke). Defaults to 256 (the PR-time count) when unset or
/// unparseable. CI sets `FUZZ_CASES`, NEVER `PROPTEST_CASES`.
fn fuzz_cases() -> u32 {
    std::env::var("FUZZ_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

/// Phase-2 U3 (spec §5 / A3): `FUZZ_CASES` selects the capstone N; unset or
/// garbage falls back to 256. `nextest` runs each test in its own process, so
/// mutating the process-global env here does not race other tests.
#[test]
fn fuzz_cases_reads_env_with_256_default() {
    let prior = std::env::var_os("FUZZ_CASES");

    std::env::set_var("FUZZ_CASES", "4096");
    assert_eq!(fuzz_cases(), 4096, "an explicit FUZZ_CASES is honored");

    std::env::set_var("FUZZ_CASES", "not_a_number");
    assert_eq!(
        fuzz_cases(),
        256,
        "unparseable FUZZ_CASES falls back to 256"
    );

    std::env::remove_var("FUZZ_CASES");
    assert_eq!(fuzz_cases(), 256, "unset FUZZ_CASES defaults to 256");

    match prior {
        Some(v) => std::env::set_var("FUZZ_CASES", v),
        None => std::env::remove_var("FUZZ_CASES"),
    }
}

proptest! {
    // CAPSTONE block (the durability surface): pin the regression corpus to ONE
    // exact, committed FILE via an absolute `Direct(...)` path built from
    // `CARGO_MANIFEST_DIR` (= rust/prro). We do NOT rely on proptest's default
    // resolution: for an integration-test target it falls back to a
    // `WithSource`-renamed FILE (no `lib.rs`/`main.rs` in the walk-up from
    // `tests/`), which is fragile and cwd-independent only by accident. On a
    // find, proptest writes the minimal seed to this file; committing it pins
    // the case as a PERMANENT regression that replays first (spec §4 / G2).
    // Scope: capstone only — the `:288` smoke and the `:305` manual demo are not
    // durability surfaces and stay on the proptest default.
    #![proptest_config(ProptestConfig {
        // Capstone N: PR-time default 256, scaled UP for the nightly large-N run
        // via the dedicated `FUZZ_CASES` knob (see `fuzz_cases()`). CI sets
        // `FUZZ_CASES`, NEVER `PROPTEST_CASES` — the latter is global and would
        // also inflate the `:288` smoke. U3 / spec §5.
        cases: fuzz_cases(),
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/invariant_fuzzer.regressions"
        )))),
        ..ProptestConfig::default()
    })]

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

/// B1/MH — a Fault-deferred EXOTIC drain (the model cannot cleanly predict it) is
/// now VERIFIED by the bounded safety postconds in run_harness, not blindly
/// resync'd.  Driving [OfflineSell x2, GoOnline([Superseded])] exercises the MH
/// postconds (no code consumed / no new lnd / seed unmoved / send bounded by the
/// cohort / shift unchanged-or-RMR); they HOLD, so the harness does not panic —
/// proving the exotic-drain path is now ASSERTED, not silently adopted.  (A
/// genuine bound violation would panic here.  Like A3, the bounds are
/// defense-in-depth: the real drain does not violate them, so the value is the
/// VERIFICATION coverage this closes — the exotic-drain false-negative zone.)
#[test]
fn harness_exotic_drain_is_bounded_postcond_verified() {
    drive(
        &[
            Op::OfflineSell,
            Op::OfflineSell,
            Op::GoOnline(DpsScript::superseded_tip()),
        ],
        true,
    );
}

/// B2 — the ExpectedNoMutation split is exercised through run_harness for BOTH
/// classes: a NoIssuanceRow op (online DPS-reject → a legal non-issued Rejected
/// row, verified by the ledger-delta) and a TrueNoMutation op (closed-shift sell
/// → refused before any row, asserted strictly zero new row / lnd).  Both
/// resolve cleanly, so the harness does not panic — proving each arm is
/// exercised AND does not false-fire (a leaked row in the TrueNoMutation op
/// would now panic at the op, not slip to a later ledger-delta).
#[test]
fn harness_no_mutation_split_both_classes() {
    drive(&[Op::OnlineSell(DpsScript::send_then_reject())], false); // NoIssuanceRow
    drive(&[Op::SellWithClosedShift], false); // TrueNoMutation
}

/// B3 — a recovered drain / go-online is FULLY snapshot-verified (ledger + seed +
/// next_lnd + consumed-codes), not just lnd→state.  [OfflineSell, GoOnline(Ack)]
/// drains the backlog to ACK; the full snapshot holds (the drain consumes no
/// code, allocates no lnd, and does NOT re-advance the seed the offline sell
/// advanced at issuance), so the harness does not panic — proving the B3
/// snapshot postcond is exercised.  (A drain that consumed a code / bumped
/// next_lnd / moved the seed would now panic here.)
#[test]
fn harness_recovered_go_online_full_snapshot_verified() {
    drive(
        &[Op::OfflineSell, Op::GoOnline(DpsScript::ack_path())],
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
/// A deterministic CI regression gate (un-`#[ignore]`d 2026-06-17): it PASSES on
/// main and FAILS only when the `backlog_drain.rs:725` guard is reverted — that
/// pass-on-main / fail-on-revert shape IS a regression gate, so it runs in CI
/// (not a manual-only canary).  Detection is MODE-INDEPENDENT (counts wire calls,
/// not a scan), so it bites even though the reverted re-drive rests in
/// `GoingOnline`, where the harness's SETTLED-mode scan gate suppresses
/// `assert_clean`.  (The capstone `drive()` loop also carries this as a
/// generative bounded-postcond; this directed test makes the coverage
/// deterministic rather than probabilistic.)
#[tokio::test]
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

/// P1 TEETH CANARY (deterministic; see `tests/invariant_fuzzer/TEETH_TEST.md`).
///
/// Boot-resume twin of fix #192.  `Crash(Sign)` commits a `SIGNED` doc and stops
/// before dispatch (the crash-after-sign window); on `Reboot`, boot
/// reconciliation drives that SIGNED doc on an Offline node with an EXHAUSTED
/// code pool → `OfflineAckOutcome::Refused(CodePoolExhausted)` at
/// `boot_phase.rs` arc 3745 → the doc MUST be aborted (`SIGNED → Aborted`).
/// WITHOUT the boot abort the doc rests non-terminal in `SIGNED`, a ledger-only
/// pin breach that `invariant_scan` flags as `StuckNonTerminalDoc`.
///
/// A deterministic CI regression gate (un-`#[ignore]`d 2026-06-17): it PASSES on
/// main and FAILS only when the boot abort is reverted (the fuzzer's teeth bite) —
/// that pass-on-main / fail-on-revert shape IS a regression gate, so it runs in CI
/// (not a manual-only canary).  The detection here is the settled-mode
/// `assert_clean` scan AFTER the reboot resolves the crash transient (mode rests
/// Offline → SETTLED → scanned).
#[tokio::test]
async fn teeth_p1_boot_resume_codepool_aborts() {
    // Offline node, OPEN shift + session, EMPTY code pool (0 codes seeded).
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(0).await;

    // Crash AFTER sign → a committed SIGNED doc (lnd 1); no code consumed (the
    // offline-ack never ran), no dispatch.
    let crashed = interp::run_op(&mut ctx, &Op::Crash(Stage::Sign)).await;
    assert!(
        matches!(
            crashed,
            interp::RealOutcome::Crashed {
                stage: Stage::Sign,
                committed_state: Some(DocState::Signed),
            }
        ),
        "Crash(Sign) must commit a SIGNED doc, got {crashed:?}"
    );

    // Reboot → boot reconciliation drives the SIGNED doc → offline-ack →
    // CodePoolExhausted (empty pool) → the P1 abort.
    let _ = interp::run_op(&mut ctx, &Op::Reboot).await;

    let ledger = ctx.read_ledger().await;
    assert_eq!(
        ledger.get(&1),
        Some(&DocState::Aborted),
        "P1: boot recovery MUST abort the post-sign-refused SIGNED doc \
         (CodePoolExhausted). If this is SIGNED, the boot abort \
         (boot_phase.rs arc 3745) is missing/reverted — the fuzzer's teeth bite."
    );
    // FULL invariant_scan clean — the same gate the property harness's
    // post-reboot settled scan runs (no StuckNonTerminalDoc, no chain break).
    oracle::assert_clean(&ctx.pool).await;
}

// ── Phase 3 — oracle-honesty teeth (U2) ──────────────────────────────────────
// Each O/X tooth comes in a PAIR: a POSITIVE tooth (the closed blind-spot now
// FAILS when the oracle fix is reverted) and a NEGATIVE tooth (a legitimate
// scenario still PASSES — proving the fix is not over-strict).  A false-positive
// is a merge-blocker on the enforced gate, so the negative tooth is mandatory.
// Revert targets are recorded in `tests/invariant_fuzzer/TEETH_TEST.md`.

/// X2 NEGATIVE tooth: a SINGLE active offline session with a consistent cohort
/// doc must NOT be flagged by `check_mirrors` (the X2 guard is not over-strict).
#[tokio::test]
async fn teeth_x2_single_active_session_not_flagged() {
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(1).await;
    // One offline sell → an OFFLINE_LOCAL_ACK cohort doc pointing at THE one
    // active OPEN session (so the Mirror-2 loop actually runs and must pass).
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await;
    assert!(
        oracle::check_mirrors(&ctx.pool).await.is_ok(),
        "X2: a single active OPEN/DRAINING session with a consistent cohort must \
         NOT be flagged — the guard must not false-positive on the legal one-session case"
    );
}

/// X2 POSITIVE tooth: TWO active OPEN sessions (the single-active-session
/// invariant breach) MUST be flagged by `check_mirrors`.
///
/// NUANCE (verified): the >1-active state is normally SCHEMA-PREVENTED by the
/// partial unique index `ux_offline_active ON offline_sessions(fiscal_number)
/// WHERE state IN ('OPENING','OPEN','DRAINING')` — the `check_mirrors` /
/// `adopt_precondition` `OPEN/DRAINING` filter is a subset, so a clean
/// DB never returns >1.  X2 is therefore DEFENSE-IN-DEPTH + determinism-hardening
/// (a regression sentinel if that index is ever weakened), not closure of a
/// currently-reachable false-negative.  This tooth drops the index to construct
/// the breach the guard is meant to catch.
///
/// WITH the X2 fix (`ORDER BY` + `> 1` count guard) `check_mirrors` returns the
/// "multiple active … sessions" `Divergence`.  Revert target: the `> 1` guard in
/// `oracle::check_mirrors` — restoring the bare `LIMIT 1` silently picks ONE
/// session and (with an empty cohort) returns `Ok`, masking the breach → this
/// tooth then FAILS.  Detection is independent of `invariant_scan` (no
/// session-count check there).
#[tokio::test]
async fn teeth_x2_multiple_active_sessions_flagged() {
    // Offline fixture seeds ONE OPEN session; no sells → empty cohort (so the
    // bare LIMIT 1 path would return Ok regardless of which session it picks).
    let ctx = interp::FuzzCtx::new_offline_open_shift(1).await;
    // Drop ux_offline_active + plant a 2nd OPEN session (the schema-prevented breach).
    ctx.plant_second_active_session_dropping_guard_index().await;

    let result = oracle::check_mirrors(&ctx.pool).await;
    let err = result.expect_err(
        "X2: two active OPEN offline sessions violate the single-active-session \
         invariant and MUST be flagged. If this is Ok, the bare LIMIT 1 lookup \
         silently picked one (the masked false-negative) — the X2 guard is missing.",
    );
    assert!(
        err.0.contains("multiple active"),
        "X2: the flag must name the multiple-active-session breach, got: {err:?}"
    );
}

/// O5 NEGATIVE tooth: the lone deferred online-origin `SENDING` (a `StuckSending`)
/// in the `ArtifactNoResend` terminal must be EXCUSED — `settle_and_scan` runs the
/// scan there now but must NOT panic on it (the exact false-positive the SETTLED
/// gate originally skipped the scan to avoid).
#[tokio::test]
async fn teeth_o5_artifact_excuses_deferred_sending() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let _ = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await; // online-origin SENDING (deferred)
    let _ = interp::run_op(&mut ctx, &Op::OfflineSellDuringGoingOnline).await; // force GoingOnline, no session
    assert_eq!(ctx.read_node_mode().await, NodeMode::GoingOnline);
    assert!(
        ctx.active_offline_session().await.is_none(),
        "online fixture has NO session — this is the no-session ArtifactNoResend terminal"
    );
    // The deferred SENDING is exactly the StuckSending the O5 filter must EXCUSE.
    let v = prro::db::invariant_scan::scan(&ctx.pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, prro::db::invariant_scan::Violation::StuckSending { .. })),
        "setup: a deferred SENDING (StuckSending) must be present to exercise the O5 excuse, got {v:?}"
    );

    // Runs the ArtifactNoResend scan but EXCUSES the lone StuckSending → no panic.
    settle_and_scan(&mut ctx, true).await;

    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sending,
        "the deferred SENDING is left in place (excused, not re-driven)"
    );
}

/// O5 POSITIVE tooth: a NON-`StuckSending` violation in the `ArtifactNoResend`
/// terminal MUST be flagged — the scan is variant-specific (only the deferred
/// `StuckSending` is excused), so a planted `AckWithoutServerFiscalNo` is fatal
/// even though a deferred `StuckSending` is also present and excused.
///
/// WITH the O5 fix the `ArtifactNoResend` arm runs `scan()` + `filter_artifact_
/// violations` and panics on the fatal breach.  Revert target: that scan+filter
/// in `settle_and_scan`'s `ArtifactNoResend` arm — restoring the scan-skip lets
/// the breach pass silently (the closed blind spot) → this tooth then FAILS.
#[test]
fn teeth_o5_artifact_flags_non_stuck_sending() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
            // An ACK doc, then drop its server_fiscal_no → AckWithoutServerFiscalNo
            // (a NON-StuckSending scan violation; terminal ACK so reboot won't touch it).
            let _ = interp::run_op(&mut ctx, &Op::OnlineSell(DpsScript::ack_path())).await;
            ctx.corrupt_ack_drop_server_fiscal_no().await;
            // A deferred online-origin SENDING (the StuckSending that IS excused) +
            // force GoingOnline → the no-session ArtifactNoResend terminal.
            let _ = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
            let _ = interp::run_op(&mut ctx, &Op::OfflineSellDuringGoingOnline).await;
            assert_eq!(ctx.read_node_mode().await, NodeMode::GoingOnline);
            // The ArtifactNoResend arm must scan and FLAG the non-StuckSending breach.
            settle_and_scan(&mut ctx, true).await;
        });
    }))
    .is_err();
    std::panic::set_hook(prev);

    assert!(
        panicked,
        "O5: the ArtifactNoResend terminal must scan and FLAG a non-StuckSending violation \
         (AckWithoutServerFiscalNo) even though a deferred StuckSending is also present and \
         excused. It returned cleanly → the branch skipped the scan (the blind spot); revert \
         target: the scan+filter in settle_and_scan's ArtifactNoResend arm."
    );
}

/// O3 NEGATIVE tooth: a clean signed doc's stored `unsigned_xml_sha256` matches
/// `sha256(PAYLOAD_XML)` → `check_payload_hash_integrity` is `Ok` (the integrity
/// oracle must not false-positive on a correctly-persisted doc).
#[tokio::test]
async fn teeth_o3_clean_payload_hash_matches() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    // A real online sell → ACK runs stage_sign, persisting PAYLOAD_XML + the
    // sha256(PAYLOAD_XML) into unsigned_xml_sha256.
    let _ = interp::run_op(&mut ctx, &Op::OnlineSell(DpsScript::ack_path())).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "setup: doc must reach ACK"
    );

    assert!(
        oracle::check_payload_hash_integrity(&ctx.pool)
            .await
            .is_ok(),
        "O3: a correctly-persisted doc (stored hash == sha256(PAYLOAD_XML)) must NOT be flagged"
    );
}

/// O3 POSITIVE tooth: a stored `unsigned_xml_sha256` that no longer matches its
/// persisted `PAYLOAD_XML` MUST be flagged by `check_payload_hash_integrity`.
///
/// WITH the O3 fix the integrity check recomputes `sha256(PAYLOAD_XML)` and
/// flags the divergence.  Revert target: the real body of
/// `oracle::check_payload_hash_integrity` — restoring the `Ok(())` stub (the
/// pre-O3 referential-only blind spot, which trusts the stored hash) lets the
/// corrupted hash pass → this tooth then FAILS.
#[tokio::test]
async fn teeth_o3_corrupted_stored_hash_flagged() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let _ = interp::run_op(&mut ctx, &Op::OnlineSell(DpsScript::ack_path())).await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "setup: doc must reach ACK"
    );

    // Corrupt the stored hash so it no longer matches the persisted PAYLOAD_XML.
    ctx.corrupt_stored_unsigned_hash().await;

    let err = oracle::check_payload_hash_integrity(&ctx.pool)
        .await
        .expect_err(
            "O3: a stored unsigned_xml_sha256 that does not match sha256(PAYLOAD_XML) MUST be \
             flagged. If this is Ok, the integrity check is the no-op stub — the referential \
             chain oracle trusts the stored hash and is blind to this corruption.",
        );
    assert!(
        err.0.contains("unsigned_xml_sha256") || err.0.contains("PAYLOAD_XML"),
        "O3: the flag must name the payload/hash divergence, got: {err:?}"
    );
}

/// O1 — convergence-assert decision table (PURE): a deterministic (no-hold) tick
/// that LEFT a doc resting is FLAGGED; a converged tick / a legitimate hold / an
/// empty tick is NOT.  Proves BOTH directions of `assert_online_convergence`
/// without a seam (mirrors `a4_terminal_verdict_decision_table`).
#[test]
fn o1_convergence_assert_decision() {
    use prro::services::reconciliation::online_convergence::TickSummary;
    let summary = |scanned: usize, held: usize| TickSummary {
        scanned,
        held_kvt1: held,
        ..Default::default()
    };
    // Deterministic tick (no holds) that LEFT a doc resting → FLAG.
    assert!(oracle::assert_online_convergence(&summary(1, 0), 1).is_err());
    // Same but fully converged (resting_after == 0) → OK.
    assert!(oracle::assert_online_convergence(&summary(1, 0), 0).is_ok());
    // Scanned WITH a legitimate hold + still resting → OK (excused).
    assert!(oracle::assert_online_convergence(&summary(1, 1), 1).is_ok());
    // Empty tick (nothing scanned / resting) → OK.
    assert!(oracle::assert_online_convergence(&summary(0, 0), 0).is_ok());
}

/// O1 NEGATIVE tooth: a legitimate SENT transport-hold (the tick REPORTS the
/// non-convergence) must NOT be flagged — the convergence assert is not
/// over-strict (CP2: the negative tooth must pass first).
#[tokio::test]
async fn teeth_o1_legit_sent_hold_not_flagged() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    let _ = interp::run_op(
        &mut ctx,
        &Op::OnlineSell(DpsScript::send_ack_then_last_not_found()),
    )
    .await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sent,
        "setup: doc rests at SENT"
    );

    // A convergence tick whose probe returns the K4 Hold form → the doc
    // legitimately stays SENT (no Match evidence yet).
    let summary = interp::convergence_tick_holds(&ctx)
        .await
        .expect("convergence tick ok");
    let resting_after = ctx.resting_online_doc_count().await;

    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sent,
        "the SENT doc legitimately holds (no Match evidence)"
    );
    assert!(
        oracle::assert_online_convergence(&summary, resting_after).is_ok(),
        "O1: a legitimate SENT transport-hold (summary reports the non-convergence) must NOT \
         be flagged — the convergence assert must not be over-strict (got {summary:?})"
    );
}

/// O1 POSITIVE tooth (DIRECTED canary; CP2 re-scope — not random-net-wired):
/// an Ack/Match-loaded convergence tick must drive a resting SENT doc to ACK, and
/// the convergence postcondition must AGREE (fully converged, nothing left
/// resting).  Online docs converge only on boot / this tick, and the referential
/// scan never flags SENT/KVT1 — so a convergence-seam regression that leaves a
/// Match-able doc stuck was a fuzzer false-negative; this canary catches it.
///
/// GREEN on main (production convergence advances the doc).  Revert target: the
/// production `SENT → KVT1` / `KVT1 → ACK` advancement in
/// `online_convergence.rs` (+ its reused boot Sent-arm / drain Kvt1Reentry
/// confirm arms) — break it and the doc stays SENT → `assert_online_convergence`
/// flags it → this tooth FAILS (the fuzzer's teeth bite).
#[tokio::test]
async fn teeth_o1_online_convergence_drives_sent_to_ack() {
    let mut ctx = interp::FuzzCtx::new_online_open_shift().await;
    // A single online sell that holds at SENT ([Ack, NotFound]) — a doc the
    // referential scan blesses clean (SENT is neither StuckSending nor
    // StuckNonTerminalDoc), so a never-converged SENT was a false-negative.
    let _ = interp::run_op(
        &mut ctx,
        &Op::OnlineSell(DpsScript::send_ack_then_last_not_found()),
    )
    .await;
    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Sent,
        "setup: doc rests at SENT"
    );

    // Drive the REAL online-convergence seam (Ack/Match-loaded).
    let summary = interp::settle_convergence_tick(&ctx)
        .await
        .expect("convergence tick ok");
    let resting_after = ctx.resting_online_doc_count().await;

    assert_eq!(
        ctx.only_doc_state().await,
        DocState::Ack,
        "O1: an Ack/Match convergence tick must drive a resting SENT doc SENT→KVT1→ACK. If \
         this is SENT, the production convergence advancement (online_convergence.rs + reused \
         arms) is reverted/broken — the fuzzer's teeth bite."
    );
    assert!(
        oracle::assert_online_convergence(&summary, resting_after).is_ok(),
        "O1: a fully-converged tick (no doc left resting) must NOT be flagged (got {summary:?})"
    );
}

/// O2 NEGATIVE tooth: a crash-completed offline sell AGREES with the
/// deterministic, DB-read-independent prediction (the slice is non-vacuous AND
/// not over-strict).  An Offline-node `Crash(Send)` never reaches the wire → it
/// completes as a real `OFFLINE_LOCAL_ACK` sell.
#[tokio::test]
async fn teeth_o2_crash_completed_sell_matches_prediction() {
    // B10: seed 3 codes and issue ONE ordinary offline sell FIRST (which mints the
    // lazy BEGIN + the sell) so the CRASH-completed sell under test is a SUBSEQUENT
    // offline doc (no new BEGIN interposed) → it completes as a single `Doc`, the
    // clean O2 slice.  (A first-offline crash-completed sell would interpose a
    // BEGIN → a two-doc `Recovered`, covered by the harness ledger-delta branch.)
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
    let mut model = RefModel::new_offline_open_shift(3);
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await; // BEGIN + sell#1
    let _ = model.apply(&Op::OfflineSell);
    let prior = ctx.read_seed().await;

    let real = interp::run_op(&mut ctx, &Op::Crash(Stage::Send)).await;
    assert!(
        matches!(&real, interp::RealOutcome::Doc(d) if d.doc_state == DocState::OfflineLocalAck),
        "setup: a subsequent offline Crash(Send) completes as an OFFLINE_LOCAL_ACK sell \
         (no new BEGIN — one already exists), got {real:?}"
    );

    let expected = model.predict_crash_completed_sell();
    assert!(
        oracle::check_differential(&real, &expected, prior.as_deref()).is_ok(),
        "O2: the crash-completed offline sell must AGREE with the deterministic prediction \
         (got real {real:?}, expected {expected:?})"
    );
}

/// O2 POSITIVE tooth: `run_harness` must DIFFERENTIAL-CHECK a crash-completed
/// offline sell — closing the vacuum where `Op::Crash → ExpectedOutcome::Fault`
/// routed to `check_differential` `Ok(())` and the real DB was adopted blindly.
///
/// We prove the routing BITES by passing a model whose `next_lnd` is pre-desynced
/// so the deterministic prediction (lnd 99) cannot match the real sell (lnd 1) →
/// `run_harness`'s differential PANICS.  Revert target: the
/// `(Op::Crash(_), RealOutcome::Doc(_)) => predict_crash_completed_sell` routing
/// in `run_harness` — restoring `model.apply(op)` (Fault → resync) adopts the
/// divergence silently → no panic → this tooth FAILS.
#[test]
fn teeth_o2_run_harness_catches_crash_completed_sell_divergence() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
            let mut model = RefModel::new_offline_open_shift(3);
            // Desync the prediction: the crash-completed FIRST offline sell mints
            // BEGIN@lnd1 + SELL@lnd2 (real), but the model now predicts lnd 99/100
            // → the differential (ledger-delta / next_lnd) must catch the divergence.
            model.next_lnd = 99;
            run_harness(&[Op::Crash(Stage::Send)], ctx, model).await;
        });
    }))
    .is_err();
    std::panic::set_hook(prev);

    assert!(
        panicked,
        "O2: run_harness must differential-check a crash-completed offline sell (pre-O2: \
         Crash → Fault → check_differential Ok(()) — vacuous, adopting the real DB). With a \
         desynced prediction it must PANIC; it returned cleanly → the (Crash, Doc) routing is \
         missing and the divergence was silently adopted."
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
    // B10: seed 3 codes — the offline sell mints the lazy BEGIN (code#1) + the
    // SELL (code#2), and the terminal settle-drain mints the DocType=10 END
    // (code#3) so the drain can FINALIZE (GoingOnline → Online).  With too few
    // codes the END would abort and the drain would not finalize (the legit-settle
    // intent of this test would be lost).
    let mut ctx = interp::FuzzCtx::new_offline_open_shift(3).await;
    let _ = interp::run_op(&mut ctx, &Op::OfflineSell).await; // BEGIN + SELL → OFFLINE_LOCAL_ACK
    ctx.force_node_mode(NodeMode::GoingOnline).await; // legit GoingOnline (active session + cohort)
    assert!(
        ctx.active_offline_session().await.is_some(),
        "the offline fixture has an active session — this is the settle-able GoingOnline"
    );

    settle_and_scan(&mut ctx, false).await;

    assert_eq!(
        ctx.read_node_mode().await,
        NodeMode::Online,
        "A4: the terminal settle must drain a legit GoingOnline cohort (BEGIN + SELL + END) to Online"
    );
    // B10: the whole session — BEGIN + SELL + END — is ACK after the settle.
    let ledger = ctx.read_ledger().await;
    assert!(
        ledger.values().all(|s| *s == DocState::Ack),
        "the whole session (BEGIN + SELL + END) drained to ACK, got {ledger:?}"
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
