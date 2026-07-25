//! Operation-sequence generator (Task 3 Part B) — a shrink-first intent stream.
//!
//! `op_sequence()` produces a FLAT `Vec<Op>` of intents (each op + its
//! `DpsScript`) with NO `prop_filter` / precondition-gating: admissibility is
//! classified by the interpreter at RUNTIME (an out-of-precondition intent
//! degrades to a no-op, it is NOT filtered out of the sample space).  This keeps
//! shrinking clean — proptest can drop any element to minimize a failing case.
//! A filter-heavy or deeply-stateful `prop_flat_map` generator is forbidden as
//! the primary path (poor shrink paths).  Invalid / re-entry ops are first-class
//! intents in the stream.

use proptest::prelude::*;

use crate::op::{DpsScript, L5Kind, Op, OperatorResolutionKind, Stage};

/// The result-able wire-response shapes for a wire op.  `timeout_at_call` is
/// intentionally EXCLUDED — the timeout SCENARIO is realized via `Crash`
/// drop-injection, not a queued response.
fn dps_script() -> impl Strategy<Value = DpsScript> {
    prop_oneof![
        Just(DpsScript::ack_path()),
        Just(DpsScript::send_ack_then_last_not_found()),
        Just(DpsScript::send_then_reject()),
        Just(DpsScript::superseded_tip()),
        Just(DpsScript::bad_hash_prev()),
        // CS-3 Slice E — APPENDED LAST (corpus seed indices preserved): a parsed unnamed non-zero
        // status → UnknownStatus → ProbeRequired HELD. `-4` is the effect-class representative; the
        // directed corpus (`directed_unknown_status_probe.rs` + the fuzzer's directed pins) covers
        // `-17` / outside-enum. Feeds the REAL decode via `interp::load_script` obs-override.
        Just(DpsScript::unknown_status(-4)),
    ]
}

/// Tier-1 shift/Z generator slice.  ACK is the happy path; `Ack, NotFound`
/// covers the D5 held-at-SENT path; Reject exercises the shift RMR lane.
///
/// RAGE W1 (PRRO_GATE-eid resolved): `Superseded` is now exercised on live
/// SHIFT_OPEN / Z_REPORT sends.  Contract adjudicated (prod=correct, model=gap):
/// prod routes `ServerFiscalIdMismatch` doc-type-agnostically to
/// `ErrorRetryable` (`error_routing.rs:350`, `WrapperBug`, CRITICAL — W9
/// escalates to RMR downstream, NOT at send).  The doc-state prediction was
/// already right — `online_outcome_state`'s `_` arm returns `ErrorRetryable`
/// (`model.rs:1464`), no seed advance, no `Rejected` RMR escalation.  The gap
/// was `shift_state`: prod stamps the mid-lifecycle state at ACQUIRE
/// (SHIFT_OPEN → `Opening`, `stage_acquire.rs:883-910`; Z_REPORT → `Closing`,
/// `stage_acquire.rs:917-935`) and the confirm edge runs ONLY on
/// `WireDecision::Sent` (`stage_send.rs:1758`), so a retryable send leaves the
/// shift resting at `Opening` / `Closing` (no rollback).  The model's
/// `apply_online_shift_open` / `apply_online_z_report` previously left the
/// shift at its PRE-op state (`Closed` / `Opened`) on the `ErrorRetryable`
/// fall-through — fixed to persist the acquire-time mid-state (TEST-ONLY model
/// change; the Ack / [Ack,NotFound] / Reject arms are unchanged).
fn shift_dps_script() -> impl Strategy<Value = DpsScript> {
    prop_oneof![
        Just(DpsScript::ack_path()),
        Just(DpsScript::send_ack_then_last_not_found()),
        Just(DpsScript::send_then_reject()),
        Just(DpsScript::superseded_tip()),
        // CS-3 Slice E — APPENDED LAST: SHIFT_OPEN / Z_REPORT also meet the UnknownStatus leaf
        // (doc-type-agnostic — `-4` is not the `-2/-15` close-shift split). ProbeRequired HELD.
        Just(DpsScript::unknown_status(-4)),
    ]
}

/// One `Op` intent.  `Crash` is drawn from the wire stages {Send, Kvt1}
/// (drop-injection: the in-flight wire future drops — TRANSPORT collapse; the
/// process may live on, so later ops legitimately run) and, since U3, the
/// stage-composition stages {Sign, OfflineAck} (the pipeline runs to a
/// committed-envelope boundary and STOPS — PROCESS death, so the harness
/// enforces "no new op until the resolving Reboot": dead-until-reboot in
/// `run_harness`).  That realism is what makes generative emission safe —
/// pre-U3 a `[Crash(Sign), OnlineSell, …]` buried the SIGNED doc under later
/// issuance, an unreachable production state (single-writer +
/// boot-recon-before-serve), so Crash(Sign) was directed-only.
/// `Crash(OfflineAck)` reaches the #192 birth-site window generatively.
/// {Finalize} is DEFERRED (its true window sits inside `inline::run`'s private
/// ladder — see the interp dispatch comment); {Acquire, Kvt2, Drain} stay
/// unimplemented; none of those is generated, so no generated op can hit
/// `unimplemented!`.
fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        // ── valid (wire ops carry a DpsScript) ──
        dps_script().prop_map(Op::OnlineSell),
        Just(Op::OfflineSell),
        // PR-R-fuzz — Return ops carry the same weight as their Sell twins, so
        // Return / mixed SELL+RETURN sequences really appear in runs (asserted
        // by `generator_emits_online_and_offline_returns`).
        dps_script().prop_map(Op::OnlineReturn),
        Just(Op::OfflineReturn),
        // Tier-1 shift/Z surface.  These stay flat intents too: if the current
        // node mode/shift precondition is wrong, the interpreter/model classify
        // the refusal or no-op at runtime instead of filtering the sample space.
        shift_dps_script().prop_map(Op::OnlineShiftOpen),
        Just(Op::OfflineShiftOpen),
        shift_dps_script().prop_map(Op::OnlineZReport),
        Just(Op::OfflineZReport),
        dps_script().prop_map(Op::GoOnline),
        dps_script().prop_map(Op::Drain),
        Just(Op::Reboot),
        // ── crash — wire drop-injection {Send, Kvt1} + stage-composition
        //    process-death {Sign, OfflineAck} (dead-until-reboot realism) ──
        prop_oneof![
            Just(Stage::Send),
            Just(Stage::Kvt1),
            Just(Stage::Sign),
            Just(Stage::OfflineAck)
        ]
        .prop_map(Op::Crash),
        // ── invalid / re-entry / replay (first-class intents) ──
        Just(Op::RepeatDrain),
        Just(Op::RepeatReboot),
        Just(Op::DuplicateIdemKey),
        Just(Op::GoOnlineWithoutBacklog),
        Just(Op::OfflineSellDuringGoingOnline),
        Just(Op::SellWithClosedShift),
        // L3 service cash-in/out — APPENDED AFTER existing ops to preserve
        // regression-seed indices (proptest encodes by prop_oneof! position;
        // inserting before existing arms would re-decode saved seeds to wrong ops).
        // guard-3b cash-floor fires on online ServiceOut with empty drawer (model
        // predicts NoMutation; differential detects mismatch).
        dps_script().prop_map(Op::OnlineServiceIn),
        dps_script().prop_map(Op::OnlineServiceOut),
        Just(Op::OfflineServiceIn),
        Just(Op::OfflineServiceOut),
        // EPZ — видача готівки за ЕПЗ.  APPENDED AFTER service-io (same
        // seed-index-preservation rule).  guard-3c cash-floor fires on online
        // EPZ with an insufficient drawer (model predicts NoMutation; the
        // differential detects any mismatch); a non-terminal EPZ blocks Z-close.
        dps_script().prop_map(Op::OnlineEpz),
        Just(Op::OfflineEpz),
        // L5 — input-guard probe.  APPENDED LAST (same seed-index-preservation
        // rule).  Each violation kind drives a SELL through convert_to_signer_payload
        // so the pre-inbox guard actually fires (model → NoMutation; the harness's
        // ExpectedNoMutation "minted no row" assertion is the durable teeth); the
        // Valid kind converts + issues (model → Mutated, differential-checked).
        prop_oneof![
            Just(L5Kind::OverCap),
            Just(L5Kind::ZeroPrice),
            Just(L5Kind::ZeroPayment),
            Just(L5Kind::Underpaid),
            Just(L5Kind::Valid),
        ]
        .prop_map(Op::L5Probe),
        // L6 — X-report (поточний звіт).  APPENDED LAST (same seed-index-
        // preservation rule).  A pure read carrying no DpsScript, insertable at
        // ANY point: the model predicts NoMutation and the harness asserts the
        // side-effect-free set + turnover snapshot == model totals.  Reverting
        // the side-effect-free property (making X consume an lnd / write a row)
        // makes a seeded harness with Op::XReport go RED — the durable teeth.
        Just(Op::XReport),
        // CS-3 operator-completion (1b) — APPENDED LAST (same seed-index-preservation rule).  A
        // no-op refusal when no held reservation rests (interp guards → no prop_filter), so it is a
        // first-class intent insertable ANYWHERE: the generator combines it with arbitrary
        // HELD/crash/reboot/stale prehistory.  When a hold DOES rest, it drives the REAL
        // `resolve_operator_pending` and the release oracle asserts the anti-BRICK witness — the
        // "next document passes after completion" half of the property.
        // 1b generates `Accepted` + `NotAccepted` + `NotAcceptedOffline` — the kinds that resolve a
        // hold WITHOUT an operator-supplied seed.  `NotAcceptedOffline` on an OFFLINE-origin hold
        // RELEASES with an OLA-cohort cancel + a chain rewind to the held doc's own `previous_hash`
        // (delivery_reservation.rs); on an ONLINE-origin hold it REFUSES (origin cross-check).  Its
        // generative release is re-enabled now that `invariant_scan` is cohort-cancel-aware (bd
        // PRRO_GATE-2nk: `active_chain_tip` + the marker re-anchor no longer false-positive on the
        // rewound cohort).  One kind stays EXCLUDED:
        //   - `MacReseed`: needs the operator's CORRECTED chain seed.  A generatively-arbitrary seed
        //     produces a `ChainSeedMismatch` (the fuzzer surfaced this — prod does NOT validate the
        //     operator's seed against the expected chain tip, so a wrong seed corrupts the chain; a
        //     potential hardening finding, docs/CS3_FUZZER_ORACLE_DOSSIER.md).  A valid-seed MacReseed
        //     is directed-only.
        prop_oneof![
            Just(OperatorResolutionKind::Accepted),
            Just(OperatorResolutionKind::NotAccepted),
            Just(OperatorResolutionKind::NotAcceptedOffline),
        ]
        .prop_map(Op::OperatorComplete),
    ]
}

/// A flat intent-stream of 1..=8 ops — shrink-first, no precondition gating.
pub fn op_sequence() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op(), 1..=8)
}
