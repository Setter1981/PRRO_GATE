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

use crate::op::{DpsScript, Op, Stage};

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
    ]
}

/// Tier-1 shift/Z generator slice.  ACK is the happy path; `Ack, NotFound`
/// covers the D5 held-at-SENT path; Reject exercises the shift RMR lane.
/// `Superseded` stays held out as the PRRO_GATE-eid known-red until the
/// production/model contract is resolved.
fn shift_dps_script() -> impl Strategy<Value = DpsScript> {
    prop_oneof![
        Just(DpsScript::ack_path()),
        Just(DpsScript::send_ack_then_last_not_found()),
        Just(DpsScript::send_then_reject()),
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
    ]
}

/// A flat intent-stream of 1..=8 ops — shrink-first, no precondition gating.
pub fn op_sequence() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op(), 1..=8)
}
