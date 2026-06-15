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

/// One `Op` intent.  `Crash` is drawn ONLY from the implemented wire stages
/// {Send, Kvt1}; the non-wire crash stages are NOT emitted (stage-composition
/// follow-up, plan §4), so no generated op can hit `unimplemented!`.
fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        // ── valid (wire ops carry a DpsScript) ──
        dps_script().prop_map(Op::OnlineSell),
        Just(Op::OfflineSell),
        dps_script().prop_map(Op::GoOnline),
        dps_script().prop_map(Op::Drain),
        Just(Op::Reboot),
        // ── crash — wire stages only (drop-injection implemented) ──
        prop_oneof![Just(Stage::Send), Just(Stage::Kvt1)].prop_map(Op::Crash),
        // ── invalid / re-entry / replay (first-class intents) ──
        Just(Op::RepeatDrain),
        Just(Op::RepeatReboot),
        Just(Op::DuplicateIdemKey),
        Just(Op::GoOnlineWithoutBacklog),
        Just(Op::OfflineSellDuringGoingOnline),
        Just(Op::SellWithClosedShift),
    ]
}

/// A flat intent-stream of 1..=8 ops — shrink-first, no precondition gating.
pub fn op_sequence() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op(), 1..=8)
}
