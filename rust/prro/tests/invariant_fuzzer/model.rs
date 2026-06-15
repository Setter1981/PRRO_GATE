//! Deterministic reference model (spec §6).
//!
//! In-memory predictor of the expected ledger for an `Op` sequence — pure data
//! and pure logic, no SQLite / `inline::run` / `ScriptedDps`.  Each non-fault
//! valid op mutates this deterministically; the Task 4 differential later
//! asserts `real == model`.
//!
//! Seed lanes (spec §6):
//!   - online-origin doc advances the seed at **ACK** (finalize);
//!   - offline-origin doc advances the seed at **OFFLINE_LOCAL_ACK** (issuance)
//!     and remains *issued* through every later drain state.
//!
//! The "issued" predicate reuses the single-source-of-truth const
//! `fiscal_documents::OFFLINE_ISSUED_STATES` — never a second hand-rolled set
//! (spec §6: the shared-fn-caller lesson applied to the test harness).

use std::collections::BTreeMap;

use prro::db::models::enums::{DocState, NodeMode, OfflineSessionState, ShiftState};
use prro::db::repositories::fiscal_documents::OFFLINE_ISSUED_STATES;

use crate::op::{DpsScript, Op, WireResponse};

/// A predicted fiscal mutation for one op (the differential, Task 4, checks
/// these against the real seam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation {
    /// The local document number the op allocated / advanced.
    pub lnd: i64,
    /// The document state the op left the doc in.
    pub doc_state: DocState,
    /// The MAC seed (tip) AFTER the op (None ⟺ still genesis).
    pub seed_after: Option<[u8; 32]>,
    /// The tip the new doc chains onto (its `previous_hash`) — the seed BEFORE
    /// the op.
    pub previous_hash: Option<[u8; 32]>,
    /// The offline code ordinal consumed (1-based), or None for online ops.
    pub code_consumed: Option<i64>,
}

/// The predicted outcome of applying one op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedOutcome {
    /// A deterministic fiscal mutation (differential-checked in Task 4).
    Mutated(Mutation),
    /// A typed refusal or idempotent no-op: NO fiscal mutation (spec §5 invalid
    /// / re-entry ops, or a valid op whose precondition did not hold).
    NoMutation,
    /// A fault / not-yet-deterministically-modelled op (crash, reboot, and —
    /// for Task 1 — drain / go_online): the fault oracle (Task 5) owns these
    /// (re-sync from the real DB).  Task 4 enriches drain / go_online into
    /// `Mutated`.  The pure model does NOT mutate here.
    Fault,
}

/// Deterministic in-memory ledger predictor for one `fiscal_number`.
#[derive(Debug, Clone)]
pub struct RefModel {
    /// MAC tip (the unsigned-xml hash of the highest issued doc); None = genesis.
    pub seed: Option<[u8; 32]>,
    /// Next local document number to allocate.
    pub next_lnd: i64,
    pub shift_state: ShiftState,
    pub mode: NodeMode,
    /// The active offline session state, if any (offline lane is fixture-seeded).
    pub session: Option<OfflineSessionState>,
    /// Offline codes issued to / consumed by this session.
    pub codes_issued: i64,
    pub codes_consumed: i64,
    /// Per-lnd document state.
    pub docs: BTreeMap<i64, DocState>,
}

impl RefModel {
    /// Fixture: an ONLINE node with an open shift, no offline session, genesis
    /// seed, lnd counting from 1.
    pub fn new_online_open_shift() -> Self {
        Self {
            seed: None,
            next_lnd: 1,
            shift_state: ShiftState::Opened,
            mode: NodeMode::Online,
            session: None,
            codes_issued: 0,
            codes_consumed: 0,
            docs: BTreeMap::new(),
        }
    }

    /// Fixture: an OFFLINE node with an open shift + an OPEN offline session
    /// carrying `codes` issued offline codes (the offline lane is entered by
    /// fixture, spec §5 — there is no `go_offline` op).
    pub fn new_offline_open_shift(codes: i64) -> Self {
        Self {
            seed: None,
            next_lnd: 1,
            shift_state: ShiftState::Opened,
            mode: NodeMode::Offline,
            session: Some(OfflineSessionState::Open),
            codes_issued: codes,
            codes_consumed: 0,
            docs: BTreeMap::new(),
        }
    }

    /// The model's offline-origin "issued" set — the SSOT const itself, by
    /// reference, NOT a forked literal (spec §6).
    pub fn offline_issued_states() -> &'static [&'static str] {
        &OFFLINE_ISSUED_STATES[..]
    }

    /// Offline-origin "issued" membership, derived solely from the SSOT const.
    pub fn is_offline_origin_issued(state: DocState) -> bool {
        OFFLINE_ISSUED_STATES.contains(&state.as_str())
    }

    fn shift_is_open(&self) -> bool {
        matches!(
            self.shift_state,
            ShiftState::Opened | ShiftState::OpenedLocalPendingDrain
        )
    }

    /// Apply one op, mutating the model and returning the predicted outcome.
    pub fn apply(&mut self, op: &Op) -> ExpectedOutcome {
        match op {
            Op::OnlineSell(script) => self.apply_online_sell(script),
            Op::OfflineSell => self.apply_offline_sell(),
            // Valid transition / drain ops: deterministic prediction is enriched
            // in Task 4 (classified PredictableMutating there); Task 1 defers
            // them to the fault / re-sync oracle.
            Op::GoOnline(_) | Op::Drain(_) => ExpectedOutcome::Fault,
            // Faults: the pure model does not mutate; recovery is ground-truth
            // re-synced by the Task 5 fault oracle.
            Op::Crash(_) | Op::Reboot => ExpectedOutcome::Fault,
            // Invalid / re-entry / replay: a typed refusal or idempotent no-op —
            // NO fiscal mutation (spec §5).
            Op::RepeatDrain
            | Op::RepeatReboot
            | Op::DuplicateIdemKey
            | Op::GoOnlineWithoutBacklog
            | Op::OfflineSellDuringGoingOnline
            | Op::SellWithClosedShift => ExpectedOutcome::NoMutation,
        }
    }

    fn apply_online_sell(&mut self, script: &DpsScript) -> ExpectedOutcome {
        if !self.shift_is_open() || self.mode != NodeMode::Online {
            return ExpectedOutcome::NoMutation;
        }
        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        let doc_state = online_outcome_state(script);
        self.docs.insert(lnd, doc_state);
        self.next_lnd += 1;
        // Online-origin advances the seed ONLY at ACK (spec §6).
        if doc_state == DocState::Ack {
            self.seed = Some(unsigned_hash);
        }
        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state,
            seed_after: self.seed,
            previous_hash,
            code_consumed: None,
        })
    }

    fn apply_offline_sell(&mut self) -> ExpectedOutcome {
        let code_available = self.codes_consumed < self.codes_issued;
        if !self.shift_is_open()
            || self.mode != NodeMode::Offline
            || self.session != Some(OfflineSessionState::Open)
            || !code_available
        {
            return ExpectedOutcome::NoMutation;
        }
        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        // Offline issuance lands at OFFLINE_LOCAL_ACK and advances the seed
        // THERE (spec §6 offline lane); it stays issued through later drain
        // states.
        self.docs.insert(lnd, DocState::OfflineLocalAck);
        self.next_lnd += 1;
        self.codes_consumed += 1;
        self.seed = Some(unsigned_hash);
        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state: DocState::OfflineLocalAck,
            seed_after: self.seed,
            previous_hash,
            code_consumed: Some(self.codes_consumed),
        })
    }
}

/// Deterministic synthetic `unsigned_xml_sha256` for the doc at `lnd`.  The pure
/// model cannot compute the real sha256 (no XML / crypto), so it assigns a
/// per-lnd-distinct value; the Task 2 / 4 interpreter reconciles model ↔ real.
/// What the chain-continuity oracle needs is distinctness + determinism — both
/// hold.
fn synth_unsigned_hash(lnd: i64) -> [u8; 32] {
    let mut h = [0u8; 32];
    h[..8].copy_from_slice(&lnd.to_be_bytes());
    h
}

/// Online-lane outcome state from the wire script.  The happy `AckPath`
/// (send -> Ack, last -> Ack) finalizes to ACK; a leading reject -> REJECTED;
/// send-Ack-then-lastChk-NotFound holds at SENT (probe-pending, K4); any other
/// path does NOT reach ACK and therefore does NOT advance the seed (online
/// issues only at ACK, spec §6).  The precise non-happy terminal states are
/// asserted / refined by the Task 4 differential against the real seam.
fn online_outcome_state(script: &DpsScript) -> DocState {
    match script.0.as_slice() {
        [WireResponse::Ack, WireResponse::Ack, ..] => DocState::Ack,
        [WireResponse::Reject, ..] => DocState::Rejected,
        [WireResponse::Ack, WireResponse::NotFound, ..] => DocState::Sent,
        _ => DocState::ErrorRetryable,
    }
}
