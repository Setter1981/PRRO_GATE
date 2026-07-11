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
//! The "issued" predicate uses a model-local FORK of the issued-set,
//! `MODEL_OFFLINE_ISSUED_STATES`, guarded by an equality test against the prod
//! SSOT const `fiscal_documents::OFFLINE_ISSUED_STATES` (U1 D3, anti-shared-
//! const): a prod-side boundary change turns the differential RED, not silent-inherit.

use std::collections::{BTreeMap, BTreeSet};

use sqlx::SqlitePool;

use crate::op::{DpsScript, Op, WireResponse};
use prro::db::models::enums::{DocState, NodeMode, OfflineSessionState, ShiftState};

/// U1 D3 — model-local FORK of the offline-origin "issued" set.  Deliberately a
/// SEPARATE literal from the prod SSOT const
/// `fiscal_documents::OFFLINE_ISSUED_STATES`; equality is enforced by
/// `teeth_d3_forked_set_matches_prod_const`, so a prod-side boundary change turns
/// the differential RED (a conscious model update) instead of silently
/// propagating into the oracle (anti-shared-const).
pub const MODEL_OFFLINE_ISSUED_STATES: [&str; 7] = [
    "OFFLINE_LOCAL_ACK",
    "SENT",
    "KVT1",
    "ERROR_RETRYABLE",
    "KVT2",
    "REJECTED",
    "REQUIRES_MANUAL_RECONCILIATION",
];

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
    /// Expected shift state after this op when the op intentionally drives the
    /// shift machine.  Receipt-only ops leave this as None.
    pub shift_state_after: Option<ShiftState>,
}

/// The predicted outcome of applying one op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedOutcome {
    /// A deterministic fiscal mutation (differential-checked in Task 4).
    Mutated(Mutation),
    /// A TRUE no-op: a typed refusal / idempotent replay that leaves the ledger
    /// entirely unchanged — NO row, NO lnd, NO seed, NO code (spec §5 invalid /
    /// re-entry ops, or a valid op refused BEFORE any row is written).
    NoMutation,
    /// B2 — a refusal that is NOT issuance but DOES mint a legal NON-ISSUED row:
    /// an online DPS-reject (Sending→Rejected, `inline::run` Err) and an
    /// offline-ack refusal that aborted a signed doc (no code / no session →
    /// Aborted).  The lnd IS consumed (the row exists) but NO receipt issued —
    /// the seed does NOT advance and no code is consumed.  Distinct from
    /// `NoMutation` so the harness asserts the RIGHT thing per class (≤1 new
    /// non-issued row here vs zero rows for `NoMutation`).
    NoIssuanceRow,
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
    /// LNDs of shift-lifecycle docs (`SHIFT_OPEN` / `SHIFT_CLOSE` /
    /// `Z_REPORT`).  Z quiescence blocks on in-flight receipts, not on earlier
    /// lifecycle artifacts that can legally rest at `SENT` while the shift has
    /// already advanced at the SEND boundary.
    pub shift_lifecycle_lnds: BTreeSet<i64>,
    /// A.3 PR-C — lnds of OFFLINE-origin docs (`offline_fiscal_no` was
    /// assigned).  Distinguishes an offline-ER (issued, NOT a D5-gate blocker)
    /// from an online-ER (non-issued, a blocker) — the `docs` map stores only
    /// `DocState`, which loses the origin the D5 predicate needs.
    pub offline_origin_lnds: BTreeSet<i64>,
    /// B10 — whether this offline session has already minted its DocType=9
    /// OFFLINE_SESSION_BEGIN.  The model predicts the BEGIN INDEPENDENTLY (from
    /// first principles, NOT by mirroring the impl) as the FIRST offline doc of a
    /// session; this flag makes that once-only (mirrors the impl's request_id
    /// idempotency gate).
    pub session_has_begin: bool,
    /// B10 — whether this session has already minted its DocType=10
    /// OFFLINE_SESSION_END (at drain finalize).  Once-only, like the BEGIN.
    pub session_has_end: bool,

    // ── L0 cash-on-hand accumulator (INV-21) ──────────────────────────────
    //
    // **RAGE W1 coordination note (task #18 — shift-op model additions):**
    // W1 adds shift-lifecycle ops (ShiftOpen/Close/ZReport) to the model.
    // When W1 merges onto this branch it MUST:
    //   - reset `cash_on_hand = 0` in new shift-open prediction (carry across
    //     shifts is L3/L4 scope; the current alphabet uses a SINGLE open shift
    //     per fixture so 0 is correct).
    //   - NOT change sign convention: SELL(cash) → +kop, RETURN(cash) → −kop.
    //   - The INV-21 oracle in oracle.rs reads `model.cash_on_hand` directly.
    //
    // **Model formula (§1.2 restricted, L0 scope):**
    //   cash_on_hand += CASH_AMOUNT_KOP  (per issued SELL — cash leg)
    //   cash_on_hand -= CASH_AMOUNT_KOP  (per issued RETURN — cash leg)
    //   INV-21 refusal: a RETURN is refused fail-closed (NoMutation) when
    //                   cash_on_hand < CASH_AMOUNT_KOP.
    //
    // **Cash identification:** type_code "0" (D1 frozen invariant).
    // Fuzzer fixture (interp.rs:64-65) always uses type_code:"0", sum_kop:15000
    // → 100% cash. Model hard-codes CASH_AMOUNT_KOP (see below) instead of
    // per-op amounts (the op alphabet does not expose payment details).
    //
    // **L3 scope:** service-in/out/EPZ terms stay 0 until wired.
    pub cash_on_hand: i64,
}

/// L0 — the cash amount used by every SELL/RETURN in the fuzzer fixture.
/// Mirrors `SELL_PAYLOAD` / interp.rs:64: `"sum_kop":15000`, `"type_code":"0"`.
/// INDEPENDENT constant — NOT derived from the prod impl (anti-shared-const,
/// mirrors U1 D3 discipline for the cash oracle).
pub const CASH_AMOUNT_KOP: i64 = 15_000;

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
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            cash_on_hand: 0, // L0: opening = 0 (first shift; carry is L3/L4)
        }
    }

    /// Fixture: ONLINE node with no open shift.
    pub fn new_online_closed_shift() -> Self {
        Self {
            seed: None,
            next_lnd: 1,
            shift_state: ShiftState::Closed,
            mode: NodeMode::Online,
            session: None,
            codes_issued: 0,
            codes_consumed: 0,
            docs: BTreeMap::new(),
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            cash_on_hand: 0, // L0: no open shift → no cash-on-hand
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
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            cash_on_hand: 0, // L0: opening = 0 (first shift)
        }
    }

    /// Fixture: OFFLINE node with no open shift and an OPEN offline session.
    pub fn new_offline_closed_shift(codes: i64) -> Self {
        Self {
            seed: None,
            next_lnd: 1,
            shift_state: ShiftState::Closed,
            mode: NodeMode::Offline,
            session: Some(OfflineSessionState::Open),
            codes_issued: codes,
            codes_consumed: 0,
            docs: BTreeMap::new(),
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            cash_on_hand: 0, // L0: no open shift
        }
    }

    /// The model's offline-origin "issued" set — the model-local FORK
    /// `MODEL_OFFLINE_ISSUED_STATES`, guarded to equal the prod SSOT const (U1 D3).
    pub fn offline_issued_states() -> &'static [&'static str] {
        &MODEL_OFFLINE_ISSUED_STATES[..]
    }

    /// Offline-origin "issued" membership, from the model-local fork (U1 D3).
    pub fn is_offline_origin_issued(state: DocState) -> bool {
        MODEL_OFFLINE_ISSUED_STATES.contains(&state.as_str())
    }

    /// The model's ONLINE-origin seed-advance set (A.3 advance-at-SEND / C6): an
    /// online doc advances the FN chain seed once it crosses the SEND boundary
    /// (`Sent`+), NOT only at `ACK`.  This is the ONLINE counterpart of
    /// `is_offline_origin_issued` (the offline arm, U1 D3).  The single SSOT for
    /// the model's online advance decision — `apply_sell` calls it, and the D7
    /// tooth cross-checks it against prod `fiscal_documents::is_issued` under the
    /// physical sfn-coupling (so a drift on either side turns the tooth RED).
    pub fn online_origin_advances_seed(state: DocState) -> bool {
        matches!(
            state,
            DocState::Sent | DocState::Kvt1 | DocState::Kvt2 | DocState::Ack
        )
    }

    fn shift_is_open(&self) -> bool {
        matches!(
            self.shift_state,
            ShiftState::Opened | ShiftState::OpenedLocalPendingDrain
        )
    }

    /// The OBSERVABLE consumed-code count — the CUMULATIVE `COUNT(*) WHERE
    /// consumed_at IS NOT NULL`, `None` when zero — matching
    /// `FuzzCtx::read_codes_consumed` (the value the real `ObservedDoc` carries).
    /// The real side reports this for EVERY doc, online or offline, so an online
    /// sell that FOLLOWS an offline sell still observes the earlier consumption;
    /// the model must report the same cumulative number, not a per-op `None`.
    fn code_consumed_observable(&self) -> Option<i64> {
        (self.codes_consumed > 0).then_some(self.codes_consumed)
    }

    /// The shift states the REAL drain runs on (`backlog_drain.rs` SW-3 fail-loud
    /// at the escalate seam: "the drain only runs on active Opened/pending-drain
    /// shifts").  On any OTHER state (e.g. a `SellWithClosedShift`-closed shift)
    /// a drain that hits a reject FAILS LOUD (`BootError::Internal` structural
    /// drift) — outside the model's clean predictive scope, so the model defers
    /// to Fault there.  `RequiresManualReconciliation` is handled separately (the
    /// AUD-K8-1 NoMutation teeth guard), so it is intentionally NOT eligible here.
    fn shift_is_drain_eligible(&self) -> bool {
        matches!(
            self.shift_state,
            ShiftState::Opened
                | ShiftState::OpenedLocalPendingDrain
                | ShiftState::ClosingLocalPendingDrain
        )
    }

    /// Apply one op, mutating the model and returning the predicted outcome.
    pub fn apply(&mut self, op: &Op) -> ExpectedOutcome {
        match op {
            // A "sell" outcome is determined by the NODE MODE, not the op name —
            // the interpreter runs `inline::run`, which dispatches by mode
            // (OnlineSell on an Offline node issues offline; OfflineSell on an
            // Online node is a mis-targeted online sell).  Both route through
            // `apply_sell`; OfflineSell carries no wire script.
            Op::OnlineSell(script) => self.apply_sell(script, false),
            Op::OfflineSell => self.apply_sell(&DpsScript(Vec::new()), false),
            // PR-R-fuzz — a RETURN is chain-wise identical to a SELL (§ apply_return):
            // same mode-dispatch, same lnd/seed/code bookkeeping.
            Op::OnlineReturn(script) => self.apply_return(script),
            Op::OfflineReturn => self.apply_return(&DpsScript(Vec::new())),
            Op::OnlineShiftOpen(script) => self.apply_online_shift_open(script),
            Op::OfflineShiftOpen => self.apply_offline_shift_open(),
            Op::OnlineZReport(script) => self.apply_online_z_report(script),
            Op::OfflineZReport => self.apply_offline_z_report(),
            // The advancing transition / drain ops predict a real ledger
            // mutation (PredictableMutating, NOT Fault).
            Op::GoOnline(script) => self.apply_go_online(script),
            Op::Drain(script) => self.apply_drain(script),
            // Faults / recovery: ground-truth re-synced by the Task 5 oracle.
            // RepeatReboot drives the boot seam too → also Fault (re-sync).
            Op::Crash(_) | Op::Reboot | Op::RepeatReboot => ExpectedOutcome::Fault,
            // Re-entry ops that drive a REAL transition seam — mirror the seam:
            Op::RepeatDrain => self.apply_drain(&DpsScript(Vec::new())),
            Op::GoOnlineWithoutBacklog => self.apply_go_online(&DpsScript(Vec::new())),
            // Deliberately-adverse intents whose interpreter arm FORCES a state
            // before a refused sell — mirror the forced state (no fiscal
            // mutation), so the model stays in sync with reality:
            // Force GoingOnline; the sell is refused by the POST-SIGN dispatcher
            // (NodeGoingOnline), which leaves NO committed doc (verified via the
            // interpreter: the ledger is unchanged) — so mirror only the forced
            // mode, no fiscal mutation / no row.
            Op::OfflineSellDuringGoingOnline => {
                self.mode = NodeMode::GoingOnline;
                ExpectedOutcome::NoMutation
            }
            Op::SellWithClosedShift => {
                self.shift_state = ShiftState::Closed;
                ExpectedOutcome::NoMutation
            }
            // A true replay (re-runs an already-DONE row) — no fiscal mutation.
            Op::DuplicateIdemKey => ExpectedOutcome::NoMutation,
        }
    }

    /// A POST-SIGN refusal: reality reaches `SIGNED` (the lnd IS allocated), then
    /// a dispatcher / offline-ack refusal aborts it → a non-issued `Aborted` row
    /// (mirrors the prod `terminalise_inbox` seam, migration 025).  Like the
    /// online-reject non-issued row, the lnd is consumed (`next_lnd` advances) but
    /// the seed does NOT advance and no code is consumed.  Classified `NoMutation`
    /// (no issuance) — a row exists but no receipt was issued.
    fn mint_aborted_refusal(&mut self) -> ExpectedOutcome {
        let lnd = self.next_lnd;
        self.docs.insert(lnd, DocState::Aborted);
        self.next_lnd += 1;
        ExpectedOutcome::NoIssuanceRow
    }

    /// O2 — predict a SELL that COMPLETED under a crash.  An Offline-node
    /// `Crash(Send)` / `Crash(Kvt1)` never reaches the wire (the offline-ack path
    /// makes no wire call), so `inline::run` COMPLETES as a real offline sell
    /// (`RealOutcome::Doc`).  That outcome is DETERMINISTIC — reuse the plain
    /// offline-sell prediction (`OFFLINE_LOCAL_ACK` consuming a code, or
    /// `mint_aborted_refusal` when no code / no session).  Computed purely from
    /// `self.next_lnd` / `self.seed` / codes — **DB-READ-INDEPENDENT**, so routing
    /// it through the `PredictableMutating` differential is NOT vacuous (pre-O2:
    /// `Op::Crash → ExpectedOutcome::Fault` → `check_differential` `Ok(())`
    /// unconditional → the real DB was adopted blindly).  The genuinely-
    /// nondeterministic crash recoveries (wire-reached `Crashed`, MAC-recovery)
    /// stay `Fault`; this is the ONE narrow predictable slice (spec §9 / U2/O2).
    pub fn predict_crash_completed_sell(&mut self) -> ExpectedOutcome {
        self.apply_sell(&DpsScript(Vec::new()), false)
    }

    /// A.3 PR-C — does a NON-ISSUED sibling rest on the FN (the D5-gate blocker
    /// predicate, mirrored)?  Blocks: any doc in a non-terminal in-flight state
    /// {`PREPARED`,`SIGNED`,`ENCRYPTED`,`SENDING`} (always non-issued — ofn/sfn
    /// unset) OR an ONLINE-origin `ERROR_RETRYABLE` (non-issued; an offline-origin
    /// ER is issued and does NOT block).  Mirrors the prod
    /// `exists_blocking_non_issued_sibling_tx` predicate (state IN the 5 in-flight
    /// states AND NOT `is_issued`).  At a fresh mint the new doc is youngest, so
    /// any blocker (all have `lnd < next_lnd`) gates it — no lnd-ordering needed.
    fn has_write_gate_blocker(&self) -> bool {
        self.docs.iter().any(|(lnd, st)| {
            matches!(
                st,
                DocState::Prepared | DocState::Signed | DocState::Encrypted | DocState::Sending
            ) || (*st == DocState::ErrorRetryable && !self.offline_origin_lnds.contains(lnd))
        })
    }

    /// Z quiescence is stricter than the ordinary D5 write gate for RECEIPTS: a
    /// Z must not aggregate/close the shift while any receipt is still in-flight,
    /// including retryable or issued-but-not-final `ERROR_RETRYABLE`/`SENT`/
    /// `KVT1`/`KVT2` docs.  Shift-lifecycle docs are excluded; they advance the
    /// shift at SEND and are not part of receipt aggregation.
    fn has_z_quiescence_blocker(&self) -> bool {
        self.docs.iter().any(|(lnd, st)| {
            !self.shift_lifecycle_lnds.contains(lnd)
                && matches!(
                    st,
                    DocState::Prepared
                        | DocState::Signed
                        | DocState::Encrypted
                        | DocState::Sending
                        | DocState::ErrorRetryable
                        | DocState::Sent
                        | DocState::Kvt1
                        | DocState::Kvt2
                )
        })
    }

    /// A RETURN — chain-wise IDENTICAL to a SELL at the model level for lnd/seed/
    /// codes.  The fuzzer enters at `inline::run`, downstream of `convert.rs`, so
    /// INV-21 does NOT refuse here.  The cash accumulator (`cash_on_hand`) tracks
    /// what prod actually does (may go negative on empty-drawer returns); the
    /// `check_cash_on_hand` oracle detects divergence independently.
    /// Routes through `apply_sell` with `is_return=true` for the SSOT cash delta.
    fn apply_return(&mut self, script: &DpsScript) -> ExpectedOutcome {
        self.apply_sell(script, true)
    }

    /// Online SHIFT_OPEN.  Closed → Opening at acquire, then Opening → Opened
    /// once the doc crosses SEND.  ACK only confirms.
    fn apply_online_shift_open(&mut self, script: &DpsScript) -> ExpectedOutcome {
        if self.mode != NodeMode::Online || self.shift_state != ShiftState::Closed {
            return ExpectedOutcome::NoMutation;
        }
        if self.has_write_gate_blocker() {
            return ExpectedOutcome::NoMutation;
        }
        if matches!(script.0.as_slice(), [WireResponse::BadHashPrev, ..]) {
            return ExpectedOutcome::Fault;
        }

        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        let doc_state = online_outcome_state(script);
        self.docs.insert(lnd, doc_state);
        self.shift_lifecycle_lnds.insert(lnd);
        self.next_lnd += 1;

        if Self::online_origin_advances_seed(doc_state) {
            self.seed = Some(unsigned_hash);
            self.shift_state = ShiftState::Opened;
        } else if doc_state == DocState::Rejected {
            self.shift_state = ShiftState::RequiresManualReconciliation;
            return ExpectedOutcome::NoIssuanceRow;
        }

        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state,
            seed_after: self.seed,
            previous_hash,
            code_consumed: self.code_consumed_observable(),
            shift_state_after: Some(self.shift_state),
        })
    }

    /// Offline SHIFT_OPEN local issuance.  It consumes one offline code, advances
    /// the seed at OFFLINE_LOCAL_ACK, and leaves the shift pending drain.
    fn apply_offline_shift_open(&mut self) -> ExpectedOutcome {
        if self.mode != NodeMode::Offline {
            return ExpectedOutcome::NoMutation;
        }
        if self.session != Some(OfflineSessionState::Open) {
            return ExpectedOutcome::NoMutation;
        }
        // B10 — the `run_staged` hoist mints the lazy DocType=9 BEGIN for ANY
        // offline doc-type (HOLE-A fix), SHIFT_OPEN included, and it runs
        // BEFORE the business doc's acquire guards: a SHIFT_OPEN refused for
        // an already-open shift still leaves the just-minted BEGIN resting
        // (the op's observable is then the BEGIN mutation alone).  Same
        // first-principles prediction as the offline sell lane (`apply_sell`):
        // 0 codes → the pre-mint pool guard refuses the WHOLE op (no rows);
        // else the BEGIN mints FIRST (lowest lnd, code#1, seed advance); a
        // post-BEGIN empty pool aborts the business doc at offline-ack
        // (Aborted row).
        // Business acquire guard (runs AFTER the hoist in the impl): SHIFT_OPEN
        // needs a Closed shift.  A duplicate shift-open still mints the session
        // BEGIN first (+code, +seed) and THEN refuses SHIFT_ALREADY_OPEN — a
        // composite (issued BEGIN row + Refused outcome) the pure model defers
        // to the fault-oracle re-sync (which restores `session_has_begin` from
        // the adopted ledger).  Directed pin:
        // `offline_shift_open_refused_after_lazy_begin_mints_begin_row`.
        if self.shift_state != ShiftState::Closed {
            if !self.session_has_begin {
                if self.codes_consumed >= self.codes_issued {
                    // Pre-mint pool guard fires BEFORE the shift guard →
                    // whole-op 503 refusal, no rows.
                    return ExpectedOutcome::NoMutation;
                }
                return ExpectedOutcome::Fault;
            }
            return ExpectedOutcome::NoMutation;
        }
        let mut just_minted_begin = false;
        if !self.session_has_begin {
            if self.codes_consumed >= self.codes_issued {
                return ExpectedOutcome::NoMutation;
            }
            self.session_has_begin = true;
            just_minted_begin = true;
            let begin_lnd = self.next_lnd;
            let begin_unsigned = synth_unsigned_hash(begin_lnd);
            self.docs.insert(begin_lnd, DocState::OfflineLocalAck);
            self.offline_origin_lnds.insert(begin_lnd);
            self.next_lnd += 1;
            self.codes_consumed += 1;
            self.seed = Some(begin_unsigned);
        }
        if self.codes_consumed >= self.codes_issued {
            // Shift-class offline docs are refused ROW-LESS on pool
            // exhaustion (the lane guard refuses pre-dispatch — unlike the
            // SELL/RETURN lane, whose exhausted business doc aborts WITH a
            // row at offline-ack).  The BEGIN-consumed-the-last-code
            // composite defers to the fault re-sync.
            return if just_minted_begin {
                ExpectedOutcome::Fault
            } else {
                ExpectedOutcome::NoMutation
            };
        }

        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        self.docs.insert(lnd, DocState::OfflineLocalAck);
        self.shift_lifecycle_lnds.insert(lnd);
        self.offline_origin_lnds.insert(lnd);
        self.next_lnd += 1;
        self.codes_consumed += 1;
        self.seed = Some(unsigned_hash);
        self.shift_state = ShiftState::OpenedLocalPendingDrain;

        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state: DocState::OfflineLocalAck,
            seed_after: self.seed,
            previous_hash,
            code_consumed: self.code_consumed_observable(),
            shift_state_after: Some(self.shift_state),
        })
    }

    /// Online Z_REPORT / close-shift.  This is the first Tier-1 shift-machine
    /// slice: Opened → Closing at acquire, then Closing → Closed once the doc
    /// crosses SEND.  ACK is confirmation; issuance is the SEND crossing.
    fn apply_online_z_report(&mut self, script: &DpsScript) -> ExpectedOutcome {
        if self.mode != NodeMode::Online || self.shift_state != ShiftState::Opened {
            return ExpectedOutcome::NoMutation;
        }
        // A live Z first quiesces the shift.  If the model sees a non-terminal
        // online blocker, reality refuses before minting a Z row.
        if self.has_z_quiescence_blocker() {
            return ExpectedOutcome::NoMutation;
        }
        if matches!(script.0.as_slice(), [WireResponse::BadHashPrev, ..]) {
            return ExpectedOutcome::Fault;
        }
        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        let doc_state = online_outcome_state(script);
        self.docs.insert(lnd, doc_state);
        self.shift_lifecycle_lnds.insert(lnd);
        self.next_lnd += 1;

        if Self::online_origin_advances_seed(doc_state) {
            self.seed = Some(unsigned_hash);
            self.shift_state = ShiftState::Closed;
        } else if doc_state == DocState::Rejected {
            // Current production escalates shift-class send rejects to RMR while
            // the document itself rests as non-issued Rejected.
            self.shift_state = ShiftState::RequiresManualReconciliation;
            return ExpectedOutcome::NoIssuanceRow;
        }

        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state,
            seed_after: self.seed,
            previous_hash,
            code_consumed: self.code_consumed_observable(),
            shift_state_after: Some(self.shift_state),
        })
    }

    /// Offline Z_REPORT / close-shift.  Local issuance consumes an offline code,
    /// advances the seed at OFFLINE_LOCAL_ACK, and moves the shift into
    /// ClosingLocalPendingDrain.  Drain/GoOnline later closes or escalates it.
    fn apply_offline_z_report(&mut self) -> ExpectedOutcome {
        if self.mode != NodeMode::Offline {
            return ExpectedOutcome::NoMutation;
        }
        if self.session != Some(OfflineSessionState::Open) {
            return ExpectedOutcome::NoMutation;
        }
        // B10 — the lazy DocType=9 BEGIN precedes ANY first offline doc of the
        // session (the `run_staged` hoist), the offline Z included, and it
        // runs BEFORE the business doc's acquire guards (see
        // `apply_offline_shift_open`).
        // Business acquire guard (runs AFTER the hoist in the impl): the
        // offline Z needs an open(-pending) shift.  The BEGIN-then-refuse
        // composite defers to the fault-oracle re-sync (see
        // `apply_offline_shift_open`).
        if !matches!(
            self.shift_state,
            ShiftState::Opened | ShiftState::OpenedLocalPendingDrain
        ) {
            if !self.session_has_begin {
                if self.codes_consumed >= self.codes_issued {
                    return ExpectedOutcome::NoMutation;
                }
                return ExpectedOutcome::Fault;
            }
            return ExpectedOutcome::NoMutation;
        }
        let mut just_minted_begin = false;
        if !self.session_has_begin {
            if self.codes_consumed >= self.codes_issued {
                return ExpectedOutcome::NoMutation;
            }
            self.session_has_begin = true;
            just_minted_begin = true;
            let begin_lnd = self.next_lnd;
            let begin_unsigned = synth_unsigned_hash(begin_lnd);
            self.docs.insert(begin_lnd, DocState::OfflineLocalAck);
            self.offline_origin_lnds.insert(begin_lnd);
            self.next_lnd += 1;
            self.codes_consumed += 1;
            self.seed = Some(begin_unsigned);
        }
        if self.codes_consumed >= self.codes_issued {
            // Row-less refusal on exhaustion (see `apply_offline_shift_open`).
            return if just_minted_begin {
                ExpectedOutcome::Fault
            } else {
                ExpectedOutcome::NoMutation
            };
        }

        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        self.docs.insert(lnd, DocState::OfflineLocalAck);
        self.shift_lifecycle_lnds.insert(lnd);
        self.offline_origin_lnds.insert(lnd);
        self.next_lnd += 1;
        self.codes_consumed += 1;
        self.seed = Some(unsigned_hash);
        self.shift_state = ShiftState::ClosingLocalPendingDrain;

        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state: DocState::OfflineLocalAck,
            seed_after: self.seed,
            previous_hash,
            code_consumed: self.code_consumed_observable(),
            shift_state_after: Some(self.shift_state),
        })
    }

    /// A sell (or return when `is_return=true`) — the lane is the NODE MODE
    /// (the interpreter's `inline::run` dispatches by mode), not the op name.
    /// Online → per-script outcome; Offline → OFFLINE_LOCAL_ACK (consuming a
    /// code); any other mode → refused.
    ///
    /// **L0 INV-21 (cash-on-hand):**
    /// A RETURN with `is_return=true` is refused fail-closed (`NoMutation`) when
    /// `cash_on_hand < CASH_AMOUNT_KOP` (the cash floor). When it proceeds (or
    /// when `is_return=false` for a SELL), the model updates `cash_on_hand`:
    ///   - SELL:   `cash_on_hand += CASH_AMOUNT_KOP`
    ///   - RETURN: `cash_on_hand -= CASH_AMOUNT_KOP`
    /// Both only when the doc IS actually issued (online: seed advances / not
    /// Rejected; offline: OLA assigned).  Aborted / Rejected rows do NOT change
    /// cash (no issued receipt was produced).
    ///
    /// **Offline RETURN note:** the L1 guard fires pre-inbox (pre-convert) in
    /// prod, so it fires BEFORE the offline code is consumed.  In the model we
    /// apply the same ordering: check cash BEFORE any lazy-BEGIN or code
    /// accounting.
    fn apply_sell(&mut self, script: &DpsScript, is_return: bool) -> ExpectedOutcome {
        if !self.shift_is_open() {
            return ExpectedOutcome::NoMutation;
        }

        // ── INV-21 in-lease cash-floor check (HOLE 2 fix) ─────────────────────────
        // The in-lease guard in `stage_acquire` (Step 6b‴‴) is IN the fuzzer's
        // lane for ONLINE mode.  When `cash_on_hand < CASH_AMOUNT_KOP`, the guard
        // refuses PRE-MINT (no lnd, no row, audit-only) → NoMutation + no cash delta.
        //
        // ONLINE-ONLY: the guard is scoped to `channel == Channel::Online` in prod.
        // Offline mode has B10 lazy-BEGIN interposition complexity: a BEGIN doc may
        // fire before the RETURN lands at the in-lease check.  The offline lane is
        // already protected by the pre-inbox L1 guard (convert.rs); we do not model
        // the in-lease refusal for offline here (it would require "BEGIN fires, RETURN
        // refused" multi-doc prediction).
        //
        // Only applies when is_return=true (SELL is never gated by INV-21).
        if is_return && self.shift_is_open() && self.mode == NodeMode::Online
            && self.cash_on_hand < CASH_AMOUNT_KOP
        {
            // In-lease guard fires (online mode): RETURN refused, no row, no cash delta.
            return ExpectedOutcome::NoMutation;
        }

        match self.mode {
            NodeMode::Online => {
                // A.3 PR-C (D5 gate, acquire layer) — an ONLINE mint is refused
                // PRE-MINT while a non-issued sibling rests on the FN (the
                // ER-parked interleave).  No lnd, no row, no seed change →
                // NoMutation (a refused-no-row sell, same class as the
                // dispatcher-mode refusals below).  This precedes ALL wire-script
                // handling: the acquire refusal fires before sign/send, so the
                // script is never consumed.  Offline mints are NOT gated.
                if self.has_write_gate_blocker() {
                    return ExpectedOutcome::NoMutation;
                }
                // Server{-12} ERROR_BAD_HASH_PREV routes to the bounded MAC-
                // recovery path (error_routing.rs `RetryClass::MacRecovery`): one
                // auto re-sign + retry.  With the fuzzer's single-shot stub the
                // retry hits an empty queue → terminal DpsRejected — a fault-class
                // outcome the pure model does not cleanly predict.  Defer to Fault
                // (the harness re-syncs); the scan / mirror checks still run on the
                // real DB afterwards, so invariant coverage is NOT lost.
                if matches!(script.0.as_slice(), [WireResponse::BadHashPrev, ..]) {
                    return ExpectedOutcome::Fault;
                }
                let lnd = self.next_lnd;
                let previous_hash = self.seed;
                let unsigned_hash = synth_unsigned_hash(lnd);
                let doc_state = online_outcome_state(script);
                self.docs.insert(lnd, doc_state); // the row IS minted (lnd allocated)
                self.next_lnd += 1;
                // A.3 / C6 — online-origin advances the seed at the SEND crossing
                // (`Sent`+, matching prod advance-at-SEND), NOT only at `ACK`.
                // A pre-SENT outcome (`Rejected` / `ErrorRetryable`, no sfn) does
                // NOT advance — mirrors `fiscal_documents::is_issued` (SSOT fn
                // `online_origin_advances_seed`, cross-checked by the D7 tooth).
                if Self::online_origin_advances_seed(doc_state) {
                    self.seed = Some(unsigned_hash);
                }
                // A DPS document-reject CAS's the row Sending→Rejected but
                // `inline::run` returns Err(DpsRejected) → the interpreter reports
                // Refused.  The row is a NON-ISSUED artifact (no seed advance), so
                // the model reports NoMutation (the lnd was still consumed, so
                // next_lnd / docs stay in sync with reality).
                if doc_state == DocState::Rejected {
                    return ExpectedOutcome::NoIssuanceRow;
                }
                // L0 cash-on-hand update: only when the doc reaches ACK state.
                //
                // Prod `aggregate_shift_cash` / `aggregate_shift_cash_tx` filter
                // `state IN ('ACK','OFFLINE_LOCAL_ACK')`.  Docs at SENT/KVT1/KVT2
                // have crossed the issuance boundary (seed advanced) but are NOT yet
                // counted in cash-on-hand — they sit probe-pending until they reach
                // ACK.  The in-lease guard also reads from that same aggregate, so
                // cash availability reflects ONLY ack'd receipts, not in-flight ones.
                // This matches Z aggregation (Z counts ACK docs) — consistent.
                //
                // NOTE (follow-up, not blocking): cash-on-hand counts at ACK, not at
                // SENT.  The deep question of "should SENT docs pre-book cash capacity"
                // is a policy question separate from INV-21 correctness.
                if doc_state == DocState::Ack {
                    if is_return {
                        self.cash_on_hand -= CASH_AMOUNT_KOP;
                    } else {
                        self.cash_on_hand += CASH_AMOUNT_KOP;
                    }
                }
                ExpectedOutcome::Mutated(Mutation {
                    lnd,
                    doc_state,
                    seed_after: self.seed,
                    previous_hash,
                    // Cumulative observable: an online sell consumes NO code, but
                    // the real doc still reports the codes a PRIOR offline sell
                    // consumed (read-back is `COUNT(*) consumed`, not per-op).
                    code_consumed: self.code_consumed_observable(),
                    shift_state_after: None,
                })
            }
            NodeMode::Offline => {
                if self.session != Some(OfflineSessionState::Open) {
                    // No active session: reality reaches SIGNED, then offline-ack
                    // refuses (NoActiveSession, post-sign) → non-issued Aborted row.
                    return self.mint_aborted_refusal();
                }

                // T2 (RULING 3.5) — offline code CLOSE-RESERVE gate, mirrored from
                // prod `enforce_offline_close_reserve` (services/write_path/inline.rs).
                // An ordinary offline SELL/RETURN is refused fail-closed PRE-MINT
                // (row-less 503 `OFFLINE_CODE_RESERVE_HELD` → the interpreter reports
                // Refused; NO ledger mutation) when granting its code would leave
                // fewer free codes than the shift needs to CLOSE offline:
                //   reserve = (BEGIN missing ? 1 : 0) + (offline Z needed ? 1 : 0)
                //   admit  ⟺  free_codes >= 1 + reserve
                // Here the shift is Open (asserted above) → a Z is still owed, so the
                // Z term is 1.  BEGIN "present" == `session_has_begin` (the model's
                // ISSUED-BEGIN flag; a not-yet-minted BEGIN counts as missing, so its
                // code is reserved).  This fires BEFORE the lazy-BEGIN mint below,
                // exactly as prod fires before `ensure_offline_session_begin` — so a
                // refused SELL mints neither the BEGIN nor the business doc.  Invariant:
                // «a shift is NEVER wedged un-closable for lack of a code».
                let free_codes = self.codes_issued - self.codes_consumed;
                // Boundary hand-off (mirrors prod): an EMPTY pool with no BEGIN yet
                // is the lazy-BEGIN pre-mint guard's domain (503
                // `OFFLINE_SESSION_BEGIN_PENDING`) — a "can't OPEN" refusal — so the
                // reserve gate defers.  Both cases predict `NoMutation` (row-less), so
                // the ledger prediction is identical; the branch is kept for exact
                // parity with `enforce_offline_close_reserve`'s ordering.  The reserve
                // gate still owns `free == 0` with a BEGIN already present.
                if free_codes == 0 && !self.session_has_begin {
                    // Falls through to the lazy-BEGIN arm below, which returns
                    // `NoMutation` on the 0-code path (no BEGIN, no business doc).
                } else {
                    let reserve = i64::from(!self.session_has_begin) + 1;
                    if free_codes < 1 + reserve {
                        return ExpectedOutcome::NoMutation;
                    }
                }

                // B10 — LAZY DocType=9 BEGIN, predicted INDEPENDENTLY (first
                // principles; NOT read from the impl).  On the FIRST offline doc of
                // a session (`!session_has_begin`) the impl lazily mints+signs+OLAs
                // the BEGIN BEFORE the business doc: lowest lnd, consumes code#1,
                // chains off the pre-op tip, advances the seed to its own unsigned
                // hash.  Code accounting re-derived vs the impl:
                //   - 0 codes → BEGIN signs bare → offline-ack ABORTS it, and the
                //     lazy-BEGIN gate fail-closes the business doc BEFORE it mints
                //     (503) → NO row at all (the pre-mint pool-empty guard in
                //     `ensure_offline_session_begin` refuses BEFORE minting a BEGIN).
                //   - 1 code → BEGIN(OLA, code#1); business doc finds empty pool →
                //     aborts (Aborted).  Two rows.
                //   - ≥2 codes → BEGIN(OLA, code#1) + business(OLA, code#2).
                if !self.session_has_begin {
                    if self.codes_consumed >= self.codes_issued {
                        // 0 codes → the pre-mint pool guard refuses the whole op
                        // RETRYABLE (503) WITHOUT minting a BEGIN or the business
                        // doc → NO ledger mutation (NOT an Aborted row).  Do NOT set
                        // `session_has_begin` — no BEGIN was minted.
                        return ExpectedOutcome::NoMutation;
                    }
                    self.session_has_begin = true; // the BEGIN row now rests (OLA)
                    let begin_lnd = self.next_lnd;
                    let begin_unsigned = synth_unsigned_hash(begin_lnd);
                    self.docs.insert(begin_lnd, DocState::OfflineLocalAck);
                    self.offline_origin_lnds.insert(begin_lnd);
                    self.next_lnd += 1;
                    self.codes_consumed += 1;
                    self.seed = Some(begin_unsigned);
                    // fall through to the business doc (chains off the BEGIN's seed).
                }

                // The business doc — after any lazy BEGIN above.
                if self.codes_consumed >= self.codes_issued {
                    // Pool exhausted for the business doc → offline-ack aborts it.
                    return self.mint_aborted_refusal();
                }
                let lnd = self.next_lnd;
                let previous_hash = self.seed;
                let unsigned_hash = synth_unsigned_hash(lnd);
                // Offline issuance → OFFLINE_LOCAL_ACK, advances the seed THERE
                // (spec §6 offline lane); stays issued through later drain states.
                self.docs.insert(lnd, DocState::OfflineLocalAck);
                // A.3 PR-C — mark offline-origin: a later drain-Superseded may
                // park this lnd at ErrorRetryable, which stays ISSUED (offline)
                // and MUST NOT be read as a D5-gate blocker.
                self.offline_origin_lnds.insert(lnd);
                self.next_lnd += 1;
                self.codes_consumed += 1;
                self.seed = Some(unsigned_hash);
                // L0 cash-on-hand update: offline-origin doc is issued at OLA.
                if is_return {
                    self.cash_on_hand -= CASH_AMOUNT_KOP;
                } else {
                    self.cash_on_hand += CASH_AMOUNT_KOP;
                }
                ExpectedOutcome::Mutated(Mutation {
                    lnd,
                    doc_state: DocState::OfflineLocalAck,
                    seed_after: self.seed,
                    previous_hash,
                    // Cumulative observable (this op's consume is now counted).
                    code_consumed: self.code_consumed_observable(),
                    shift_state_after: None,
                })
            }
            // GoingOnline / Blocked / etc — a sell is refused by the POST-SIGN
            // DISPATCHER (PostSignRoute::Refused, e.g. NodeGoingOnline).  Unlike
            // the OFFLINE-ack refusals above (which abort a COMMITTED SIGNED doc →
            // Aborted), the dispatcher-mode refusal leaves NO committed doc
            // (verified via the interpreter: ledger unchanged) — so no mutation,
            // no row.
            _ => ExpectedOutcome::NoMutation,
        }
    }

    /// `Drain` runs only in `GoingOnline` (else a no-op refusal); it advances
    /// the OFFLINE_LOCAL_ACK backlog per the wire script.
    fn apply_drain(&mut self, script: &DpsScript) -> ExpectedOutcome {
        if self.mode != NodeMode::GoingOnline {
            return ExpectedOutcome::NoMutation;
        }
        // AUD-K8-1 guard (backlog_drain.rs:725): a drain re-tick on a
        // manual-recon FN is a NO-OP — the model encodes the guard so the
        // differential CATCHES a reverted re-drive (the teeth-test mechanism).
        // MUST precede the eligibility check (RMR is NOT drain-eligible).
        if self.shift_state == ShiftState::RequiresManualReconciliation {
            return ExpectedOutcome::NoMutation;
        }
        // A drain on a NON-eligible shift (e.g. a force-closed shift) FAILS LOUD
        // in the real seam on a reject — outside the clean predictive scope.
        if !self.shift_is_drain_eligible() {
            return ExpectedOutcome::Fault;
        }
        self.drain_backlog(script)
    }

    /// `GoOnline` is the one real transition op: the probe flips
    /// `Offline → GoingOnline` (skipped — no-op — if not Offline), then the
    /// drain advances the backlog (`GoingOnline → Online`).
    fn apply_go_online(&mut self, script: &DpsScript) -> ExpectedOutcome {
        // A GoingOnline start is as real as an Offline one: the REAL seam
        // (probe + drain) proceeds from the very mid-transition mode it
        // completes — e.g. `OfflineSellDuringGoingOnline` forces the node
        // there and does not restore. Restricting the model to an Offline
        // start left its backlog un-drained while the real drain ACKed it
        // (nightly find 2026-06-27; the seed replays first from the committed
        // corpus). Script consumption is identical from either start: the
        // probe feeds off the separate status queue, the script goes wholly
        // to the drain (`interp::go_online`).
        if self.mode != NodeMode::Offline && self.mode != NodeMode::GoingOnline {
            return ExpectedOutcome::NoMutation;
        }
        // AUD-K8-1 mirror (same as `apply_drain`): with a GoingOnline start
        // modeled, RMR+GoingOnline is reachable (a drain reject escalates to
        // RMR and leaves the node GoingOnline) — a GoOnline re-tick on a
        // manual-recon FN must predict NO ledger mutation, because the real
        // drain's RMR re-entry guard (backlog_drain.rs:725) makes it a no-op.
        if self.shift_state == ShiftState::RequiresManualReconciliation {
            return ExpectedOutcome::NoMutation;
        }
        self.mode = NodeMode::GoingOnline;
        // Same eligibility deferral as `apply_drain` — a drain over a
        // non-eligible shift is outside the clean predictive scope.
        if !self.shift_is_drain_eligible() {
            return ExpectedOutcome::Fault;
        }
        self.drain_backlog(script)
    }

    /// Predict the drain of the OFFLINE_LOCAL_ACK backlog (ordered by lnd) per
    /// the script's leading send response:
    ///   - leading `Ack` → every backlog doc OFFLINE_LOCAL_ACK → ACK; a full
    ///     drain CAS's `GoingOnline → Online`.  The seed does NOT re-advance —
    ///     offline-origin docs advanced it at issuance (spec §6).
    ///   - leading `Reject` → strict-sequential halt-on-reject (K8): the first
    ///     backlog doc → REJECTED, the shift → RequiresManualReconciliation, the
    ///     rest are held (stay OFFLINE_LOCAL_ACK).
    ///   - empty backlog → eligible-empty: `GoingOnline → Online`, no mutation.
    ///   - other leading responses (timeout / superseded / bad-hash / not-found)
    ///     → Fault (deferred; the full per-response drain semantics are an agreed
    ///     follow-up — see plan §4 / Task 4 scope note).
    fn drain_backlog(&mut self, script: &DpsScript) -> ExpectedOutcome {
        // The REAL drain re-drives the WHOLE cohort — offline-origin docs in
        // OFFLINE_LOCAL_ACK / SENT / KVT1 / ERROR_RETRYABLE / KVT2 (see
        // `list_drain_candidates_for_fn_ordered_by_lnd`).  The pure model
        // predicts ONLY the CLEAN case where the cohort is entirely
        // OFFLINE_LOCAL_ACK.  If a PRIOR (exotic / partial) drain left a doc
        // mid-wire (SENT / KVT1 / ERROR_RETRYABLE / KVT2), re-driving it is the
        // deferred per-response drain follow-up (plan §4) → defer to Fault (the
        // harness re-syncs and does NOT differential-check it).  This NEVER
        // masks the K8 teeth: the RMR guard in `apply_drain` runs FIRST, and the
        // K8 backlog rests at OFFLINE_LOCAL_ACK / REJECTED (neither is mid-wire).
        let cohort_has_midwire = self.docs.values().any(|st| {
            matches!(
                st,
                DocState::Sent | DocState::Kvt1 | DocState::ErrorRetryable | DocState::Kvt2
            )
        });
        if cohort_has_midwire {
            return ExpectedOutcome::Fault;
        }
        let backlog: Vec<i64> = self
            .docs
            .iter()
            .filter(|(_, st)| **st == DocState::OfflineLocalAck)
            .map(|(lnd, _)| *lnd)
            .collect();
        if backlog.is_empty() {
            // Nothing to drain: the drain CAS's GoingOnline → Online (eligible).
            self.mode = NodeMode::Online;
            return ExpectedOutcome::NoMutation;
        }
        let previous_hash = self.seed; // unchanged by drain (offline already advanced)
        match script.0.as_slice() {
            // Pure AckPath ([Ack, Ack]) only — [Ack, NotFound] is NOT an
            // all-ACK drain (it holds at SENT), so it falls through to Fault.
            [WireResponse::Ack, WireResponse::Ack, ..] => {
                for lnd in &backlog {
                    self.docs.insert(*lnd, DocState::Ack);
                }
                // Tier-1 shift resolution — a full-ACK drain resolves a
                // pending-drain shift BEFORE the END mints (the impl confirms
                // the shift edge on the content backlog, then mints the END at
                // drain finalize), so the END's Mutation carries the
                // post-transition shift state.
                match self.shift_state {
                    ShiftState::OpenedLocalPendingDrain => {
                        self.shift_state = ShiftState::Opened;
                    }
                    ShiftState::ClosingLocalPendingDrain => {
                        self.shift_state = ShiftState::Closed;
                    }
                    _ => {}
                }
                // B10 END-online fix — at drain finalize the DocType=10 END is
                // minted + sent LAST as an ONLINE ISSUANCE (predicted
                // INDEPENDENTLY).  The impl (`ensure_and_drain_session_end`) mints
                // the END at EVERY content-Eligible drain of a bound-shift offline
                // session, gated ONLY on shift presence + `!already-END` — NOT on
                // BEGIN presence (`backlog_drain` skips only on a NULL shift; there
                // is no BEGIN-existence gate).  In normal production a BEGIN always
                // precedes the backlog (the `inline::run` hoist mints it for the
                // first offline doc), so BEGIN presence and END-mint coincide.  The
                // ONE case where they part is a production-UNREACHABLE fuzzer state:
                // `crash_after_sign` stages an offline SELL DIRECTLY (bypassing the
                // `inline::run` BEGIN hoist), so a `Crash(Sign), RepeatReboot,
                // GoOnline` leaves a drainable offline backlog with NO BEGIN — and
                // the real drain still mints the END (real `{1:Ack, 2:Ack}`).  So
                // the model matches the impl: mint the END on any non-empty eligible
                // drain, once-only (`session_has_end`), independent of BEGIN.
                //
                // ONLINE ISSUANCE semantics: the END is `fs_mode='ONLINE'` (bare
                // `<MAC>`), so it consumes NO offline code (`codes_consumed`
                // UNCHANGED — NOT offline-origin) and advances the ONLINE seed at
                // the `Sending → Sent` CAS (advance-at-SEND), NOT at offline-ack.
                // Its issuance is independent of the offline pool — it ALWAYS issues
                // + drains to ACK on the AckPath drain (no pool-exhausted Abort
                // branch: an online issuance never needs a code).
                if !self.session_has_end {
                    self.session_has_end = true;
                    let end_lnd = self.next_lnd;
                    let end_prev = self.seed; // END chains off the last content doc
                    let end_unsigned = synth_unsigned_hash(end_lnd);
                    self.next_lnd += 1;
                    // Online issuance → ACK via drain (Sent→Kvt2→Ack) + finalize.
                    // Advance the ONLINE seed to the END's unsigned hash (mirrors
                    // the impl's advance-at-SEND).  NO code consumed, NOT added to
                    // `offline_origin_lnds` (it is online-origin).
                    self.docs.insert(end_lnd, DocState::Ack);
                    self.seed = Some(end_unsigned);
                    self.mode = NodeMode::Online;
                    return ExpectedOutcome::Mutated(Mutation {
                        lnd: end_lnd,
                        doc_state: DocState::Ack,
                        seed_after: self.seed,
                        previous_hash: end_prev,
                        code_consumed: None,
                        shift_state_after: Some(self.shift_state),
                    });
                }
                self.mode = NodeMode::Online; // full drain → GoingOnline → Online
                let tip = *backlog.last().expect("backlog is non-empty");
                ExpectedOutcome::Mutated(Mutation {
                    lnd: tip,
                    doc_state: DocState::Ack,
                    seed_after: self.seed,
                    previous_hash,
                    code_consumed: None,
                    shift_state_after: Some(self.shift_state),
                })
            }
            [WireResponse::Reject, ..] => {
                let first = backlog[0];
                self.docs.insert(first, DocState::Rejected);
                self.shift_state = ShiftState::RequiresManualReconciliation;
                // mode stays GoingOnline (drain halted); seed unchanged.
                ExpectedOutcome::Mutated(Mutation {
                    lnd: first,
                    doc_state: DocState::Rejected,
                    seed_after: self.seed,
                    previous_hash,
                    code_consumed: None,
                    shift_state_after: Some(self.shift_state),
                })
            }
            // U1 D5 — Superseded tip: the strict-sequential drain escalates to
            // manual.  Empirically probe-derived (the `classify_check_result`
            // Superseded arm, kvt2_confirm.rs:~357): the HEAD backlog doc →
            // ERROR_RETRYABLE, the shift → RMR (EscalateManual, M3b §16.7); mode
            // stays GoingOnline (set by `apply_go_online`); successors held at
            // OFFLINE_LOCAL_ACK; the seed does NOT re-advance (offline-origin
            // advanced it at issuance).
            [WireResponse::Superseded, ..] => {
                let first = backlog[0];
                self.docs.insert(first, DocState::ErrorRetryable);
                self.shift_state = ShiftState::RequiresManualReconciliation;
                ExpectedOutcome::Mutated(Mutation {
                    lnd: first,
                    doc_state: DocState::ErrorRetryable,
                    seed_after: self.seed,
                    previous_hash,
                    code_consumed: None,
                    shift_state_after: Some(self.shift_state),
                })
            }
            // U1 D5 — send Ack'd, last_chk NotFound: the HEAD doc is HELD at SENT
            // (`SentNotFoundDowngrade`, kvt2_confirm.rs:~330); shift unchanged,
            // mode stays GoingOnline, successors held, seed unchanged.
            [WireResponse::Ack, WireResponse::NotFound, ..] => {
                let first = backlog[0];
                self.docs.insert(first, DocState::Sent);
                ExpectedOutcome::Mutated(Mutation {
                    lnd: first,
                    doc_state: DocState::Sent,
                    seed_after: self.seed,
                    previous_hash,
                    code_consumed: None,
                    shift_state_after: Some(self.shift_state),
                })
            }
            // MAC-recovery (BadHashPrev) drain stays genuinely deferred (§7 #1).
            _ => ExpectedOutcome::Fault,
        }
    }

    /// **Re-sync (L3, Task 5).**  After a fault + recovery we do NOT predict the
    /// recovered state — recovery is a Phase-1 wildcard — we ADOPT the real DB
    /// state, so subsequent ops are differential-clean again from there.
    ///
    /// The fuzzer uses ONE fiscal_number per DB, so the reads are unfiltered
    /// (they adopt that FN's real state).  The seed is adopted STRUCTURALLY:
    /// `Some` iff the real `node_state` has a seed, else `None` — a synthetic
    /// per-tip-lnd placeholder, since the exact bytes are never compared (the
    /// differential is structural — advance-iff + chain-continuity vs the real
    /// tip, never `model.seed == real.seed` byte-for-byte).
    ///
    /// **U1 A1 funnel wrapper — `adopt_fault_deferred`** (was `resync_from_db`):
    /// the tagged home for the post-fault recovery adoption residue (the state
    /// the model deliberately does NOT predict across a crash-window).  The
    /// static-scan `model_db_access_is_funneled_through_tagged_wrappers`
    /// (invariant_scan.rs) forbids any raw DB read outside the tagged wrappers.
    pub async fn adopt_fault_deferred(&mut self, pool: &SqlitePool) {
        // docs ← the real ledger (lnd → state).
        let docs: Vec<(i64, DocState)> =
            sqlx::query_as("SELECT lnd, state FROM fiscal_documents ORDER BY lnd")
                .fetch_all(pool)
                .await
                .unwrap();
        self.docs = docs.into_iter().collect();
        // A.3 PR-C — re-derive the offline-origin set (offline_fiscal_no set):
        // needed by the D5-gate blocker predicate to tell an offline-ER (issued)
        // from an online-ER (blocker) after a fault re-sync.
        let offline_lnds: Vec<(i64,)> =
            sqlx::query_as("SELECT lnd FROM fiscal_documents WHERE offline_fiscal_no IS NOT NULL")
                .fetch_all(pool)
                .await
                .unwrap();
        self.offline_origin_lnds = offline_lnds.into_iter().map(|(lnd,)| lnd).collect();
        let shift_lifecycle_lnds: Vec<(i64,)> = sqlx::query_as(
            "SELECT lnd FROM fiscal_documents \
             WHERE doc_type IN ('SHIFT_OPEN','SHIFT_CLOSE','Z_REPORT')",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        self.shift_lifecycle_lnds = shift_lifecycle_lnds.into_iter().map(|(lnd,)| lnd).collect();
        let tip_lnd = self.docs.keys().copied().max().unwrap_or(0);

        // mode / shift_state / next_lnd ← node_state.
        let (mode, shift_state, next_lnd): (NodeMode, ShiftState, i64) =
            sqlx::query_as("SELECT mode, shift_state, next_lnd FROM node_state LIMIT 1")
                .fetch_one(pool)
                .await
                .unwrap();
        self.mode = mode;
        self.shift_state = shift_state;
        self.next_lnd = next_lnd;
        // STRUCTURAL seed: Some iff real Some (synthetic placeholder bytes) —
        // read through the tagged `read_seed_fixture` wrapper.
        let real_seed = Self::read_seed_fixture(pool).await;
        self.seed = real_seed.is_some().then(|| synth_unsigned_hash(tip_lnd));

        // offline session + codes ← real.
        self.session = sqlx::query_scalar::<_, OfflineSessionState>(
            "SELECT state FROM offline_sessions ORDER BY opened_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .unwrap();
        self.codes_issued = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM offline_codes")
            .fetch_one(pool)
            .await
            .unwrap();
        self.codes_consumed = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM offline_codes WHERE consumed_at IS NOT NULL",
        )
        .fetch_one(pool)
        .await
        .unwrap();

        // B10 — re-derive the boundary-doc flags from the real ledger.  Without
        // this a crash+reboot over a session that already minted a DocType=9
        // BEGIN (e.g. `Crash(Sign), RepeatReboot`) left `session_has_begin` at
        // its `false` default, so a subsequent all-ACK `GoOnline` drain SKIPPED
        // the DocType=10 END-mint prediction while the real drain minted it →
        // `{1: Ack}` (model) vs `{1: Ack, 2: Ack}` (real).  The flags are derived
        // OBSERVABLY (presence of a boundary row in an ISSUED / non-terminal-
        // failed state), exactly as `docs` / `codes` are — mirroring the impl's
        // existence probe (`ensure_offline_session_begin`: OLA/SENT/KVT1/KVT2/ACK
        // count as present; ABORTED/REJECTED/CANCELLED/RMR free the slot).  A
        // still-in-flight boundary row (PREPARED/SIGNED/ENCRYPTED/SENDING) is
        // NOT yet "issued" and does not set the flag — boot-resume drives it and
        // the next fault re-sync re-reads it.  Inlined (not a helper) so the read
        // stays inside this tagged funnel wrapper (adoption-lint discipline).
        let issued_boundaries: Vec<(String,)> = sqlx::query_as(
            "SELECT doc_type FROM fiscal_documents \
             WHERE doc_type IN ('OFFLINE_SESSION_BEGIN', 'OFFLINE_SESSION_END') \
               AND state IN ('OFFLINE_LOCAL_ACK', 'SENT', 'KVT1', 'KVT2', 'ACK')",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        self.session_has_begin = issued_boundaries
            .iter()
            .any(|(dt,)| dt == "OFFLINE_SESSION_BEGIN");
        self.session_has_end = issued_boundaries
            .iter()
            .any(|(dt,)| dt == "OFFLINE_SESSION_END");
    }

    /// **U1 A1 funnel wrapper — `read_seed_fixture`.**  The tagged primitive for
    /// reading the persisted MAC-seed presence
    /// (`node_state.last_known_unsigned_xml_sha256`) the model grounds on.  The
    /// value is adopted STRUCTURALLY (`Some`/`None`) — the exact bytes are never
    /// compared (the differential is structural: advance-iff + chain-continuity).
    async fn read_seed_fixture(pool: &SqlitePool) -> Option<Vec<u8>> {
        sqlx::query_scalar::<_, Option<Vec<u8>>>(
            "SELECT last_known_unsigned_xml_sha256 FROM node_state LIMIT 1",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Adopt ONLY the precondition state (mode / shift_state / active session)
    /// from the real DB after a NON-fault op — keeping the PREDICTED ledger
    /// (docs / seed / next_lnd / codes) for the differential.  Transition seams
    /// (go_online / drain / the force-ops) set mode/shift/session in ways the
    /// pure model need not perfectly predict; the LEDGER is what the differential
    /// checks, and the mode/shift mirror integrity is checked by the scan.  This
    /// keeps the NEXT op dispatching from the real precondition state.
    pub async fn adopt_precondition(&mut self, pool: &SqlitePool) {
        let (mode, shift_state): (NodeMode, ShiftState) =
            sqlx::query_as("SELECT mode, shift_state FROM node_state LIMIT 1")
                .fetch_one(pool)
                .await
                .unwrap();
        self.mode = mode;
        self.shift_state = shift_state;
        // The ACTIVE (OPEN / DRAINING) session — the one a sell / drain
        // dispatches on (matches `current_open_or_draining_session`).
        //
        // X2: deterministic `ORDER BY` + single-active-session guard.
        // `ux_offline_active` guarantees ≤1 active session, so this assert never
        // fires on a clean DB — it is a defense-in-depth sentinel (a >1-active
        // breach would otherwise be silently masked by the bare `LIMIT 1` picking
        // an arbitrary row).
        let active_states: Vec<OfflineSessionState> = sqlx::query_scalar::<_, OfflineSessionState>(
            "SELECT state FROM offline_sessions WHERE state IN ('OPEN', 'DRAINING') \
             ORDER BY opened_at DESC, offline_session_id",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert!(
            active_states.len() <= 1,
            "X2: multiple active OPEN/DRAINING offline sessions during precondition \
             resync (single-active-session invariant breach): {active_states:?}"
        );
        self.session = active_states.into_iter().next();
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
/// send-Ack-then-lastChk-NotFound holds at SENT (probe-pending, K4) — which,
/// post-A.3, DOES advance the seed (SENT crosses the SEND boundary; see
/// `online_origin_advances_seed`).  Only a pre-SEND terminal (Rejected /
/// ErrorRetryable, no sfn) does NOT advance (online issues at SEND — advance-at-
/// SEND, A.3).  The precise non-happy terminal states are asserted / refined by
/// the Task 4 differential against the real seam.
fn online_outcome_state(script: &DpsScript) -> DocState {
    match script.0.as_slice() {
        [WireResponse::Ack, WireResponse::Ack, ..] => DocState::Ack,
        [WireResponse::Reject, ..] => DocState::Rejected,
        [WireResponse::Ack, WireResponse::NotFound, ..] => DocState::Sent,
        _ => DocState::ErrorRetryable,
    }
}
