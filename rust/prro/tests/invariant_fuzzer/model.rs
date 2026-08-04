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

use crate::op::{DpsScript, L5Kind, Op, OperatorResolutionKind, ReplenishLeaf, WireResponse};
use prro::db::models::enums::{DocState, DocType, NodeMode, OfflineSessionState, ShiftState};

/// U1 D3 — model-local FORK of the offline-origin "issued" set.  Deliberately a
/// SEPARATE literal from the prod SSOT const
/// `fiscal_documents::OFFLINE_ISSUED_STATES`; equality is enforced by
/// `teeth_d3_forked_set_matches_prod_const`, so a prod-side boundary change turns
/// the differential RED (a conscious model update) instead of silently
/// propagating into the oracle (anti-shared-const).
pub const MODEL_OFFLINE_ISSUED_STATES: [&str; 8] = [
    "OFFLINE_LOCAL_ACK",
    // CS-3 S7-1: a HELD send outcome (ambiguous / transient / -12 / drain-reject) rests the
    // offline-origin doc at SENDING under a PENDING_APPLY reservation — a legitimate issued
    // (non-lost) state, mirroring prod `fiscal_documents::OFFLINE_ISSUED_STATES`.
    "SENDING",
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
    /// CS-3 operator-completion (1b): the predicted outcome of an `OperatorComplete` op.
    /// `Some(ReleasedWitness)` ⟺ the model predicts a LEGAL release (the held doc transitions, the
    /// fence clears, the node un-halts — the model has ALSO mutated `self` accordingly); `None` ⟺ the
    /// model predicts a REFUSED completion (no held reservation rests, or an origin cross-check
    /// contradiction) → the hold is left intact and `self` is unchanged.  The release oracle
    /// (`run_harness`) asserts the anti-BRICK witness + the doc/seed/mode transition, NOT the per-doc
    /// differential (a release is not a fresh issuance).
    Release(Option<ReleasedWitness>),
    /// bd `PRRO_GATE-hpc` — a T=112 replenish.  Its own class because it is the ONLY op that mutates
    /// durable state WITHOUT minting a document: on `granted` the code pool grows and the chain seed
    /// moves to a NON-DOCUMENT value (`sha256(request_xml)`, witnessed by migration 040), while
    /// `next_lnd` does NOT move.  Every other mutating op ties a seed advance to an issuance, so
    /// reusing `Mutated` / `NoMutation` here would either demand a doc that does not exist or assert
    /// a seed that legitimately moved did not.  A dedicated variant keeps the carve-out narrow and
    /// compiler-enforced instead of a blanket exemption.
    Replenish { granted: bool },
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
    /// Peer-tip axis PHASE C (spec §8.1) — the chain link each document was MINTED onto, keyed by
    /// lnd, in the model's symbolic algebra.
    ///
    /// Prod writes `fiscal_documents.previous_hash` once, at mint, and never rewrites it; the
    /// `NotAcceptedOffline` rewind reads exactly that column back (`delivery_reservation.rs`
    /// `offline_cohort_cleanup`).  The model used to mark that rewind with `seed = None` — a "this
    /// value is not mine to know" flag, checked relationally by `run_harness` against the ledger.
    /// That sufficed while the only question asked of the tip was *did it move*.  It stops
    /// sufficing the moment the tip is COMPARED — against the peer's, to derive a `-12` — because
    /// the marker compares equal to genesis, and a model that thinks it is at genesis when the
    /// ledger is three documents along will manufacture divergences that production never had.
    ///
    /// So the model mirrors prod's DERIVATION rather than its result: record the link at mint,
    /// restore that same link at the rewind.
    pub doc_prev: BTreeMap<i64, Option<[u8; 32]>>,
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

    // ── L0 cash-on-hand (INV-21) — DERIVED, never accumulated ─────────────
    //
    // bd PRRO_GATE-6hl.  **Production maintains nothing.** `aggregate_shift_cash`
    // RE-DERIVES the drawer from the ledger on every read, so a document's cash
    // contribution appears and disappears purely as a function of its CURRENT
    // state (`fiscal_documents::counted_in_turnover`).  The model used to keep an
    // incremental SCALAR instead, mutated at the ~9 issuance sites and reversed at
    // the bd x5o cancel sites.  The two agreed only while the counted set was the
    // TERMINAL {ACK, OFFLINE_LOCAL_ACK}: nothing could enter it except at
    // issuance, so "add once at issuance" was EQUIVALENT to deriving.  bd
    // PRRO_GATE-a6n moved entry EARLIER (the issuance boundary), which makes
    // transitions INTO the counted set reachable AFTER the fact — and an
    // incremental accumulator has no hook on those.  The operator-completion
    // release is exactly such a hook: it moves the held doc `Sending → Sent`
    // (Accepted) or `→ RMR` and touched cash in NEITHER direction, leaking a whole
    // cash leg (`[OfflineSell, OperatorComplete(Accepted), OnlineEpz]` — the model
    // refused guard-3c over an empty drawer while prod, counting the doc at SENT,
    // admitted and minted).
    //
    // So the model DERIVES too:
    //   cash_on_hand() = opening_cash_kop
    //                  + Σ { cash_by_lnd[l] | counted_in_turnover(docs[l], origin(l)) }
    // Reversal stops being a thing that can be FORGOTTEN, because it falls out of
    // the document's state.  Every future state edge into or out of the counted set
    // is covered by construction — not by a fourth patch of the same shape.
    //
    // **Model formula (§1.2 restricted, L0 scope):**
    //   SELL / service-in           →  +CASH_AMOUNT_KOP
    //   RETURN / service-out / EPZ  →  −CASH_AMOUNT_KOP
    //   INV-21 refusal: a RETURN is refused fail-closed (NoMutation) when
    //                   `cash_on_hand() < CASH_AMOUNT_KOP`.
    //
    // **Cash identification:** type_code "0" (D1 frozen invariant).
    // Fuzzer fixture (interp.rs:64-65) always uses type_code:"0", sum_kop:15000
    // → 100% cash. Model hard-codes CASH_AMOUNT_KOP (see below) instead of
    // per-op amounts (the op alphabet does not expose payment details).
    /// The CURRENT shift's opening anchor — the model's mirror of the open shift's
    /// `shifts.cash_balance_kop`, which prod's `cash_on_hand_for_fn` adds to the shift
    /// aggregate.  Re-anchored at every shift-open from `carry_cash_kop`, and on a fault
    /// re-sync (`adopt_cash`) to whatever part of reality's drawer the model's own
    /// per-document legs do not explain.
    pub opening_cash_kop: i64,
    /// bd PRRO_GATE-x5o / PRRO_GATE-6hl — the cash leg each CURRENT-shift document carries,
    /// keyed by lnd, recorded AT MINT **regardless of the state the document landed in**.
    /// That mirrors prod exactly: the payment is stored in `payload_json` when the row is
    /// written, and `counted_in_turnover` decides PER READ whether it counts — so a doc that
    /// enters or leaves the counted set later needs no bookkeeping hook at all.  A document
    /// with no cash leg (a DocType=9 offline BEGIN, a SHIFT_OPEN / Z_REPORT) has no entry.
    /// CLEARED at every shift-open: prod's aggregates are scoped to the open `shift_id`, so a
    /// prior shift's legs must not follow the drawer across the boundary — and RE-SCOPED from the
    /// real ledger on a fault re-sync, the one window in which reality can cross that boundary
    /// without the model predicting it.
    pub cash_by_lnd: BTreeMap<i64, i64>,

    // ── Cross-shift cash carry (mirrors prod `shifts.cash_balance_kop`) ────
    // Prod carry semantics (operator decision 2026-07-10, `cash_ledger.rs:13-18`):
    // a NEW shift's `opening_cash = prior shift's cash_balance_kop` (the last
    // CLOSED shift's persisted CLOSING drawer), or 0 for the FN's first shift.
    // A real SHIFT_CLOSE (Z_REPORT) OVERWRITES `cash_balance_kop` with the
    // closing drawer; a force-close (`SellWithClosedShift`) NEVER persists, so
    // the row keeps its OPENING value. This field is the model's mirror of that
    // persisted anchor: it is written to `cash_on_hand` ONLY at a REAL close
    // (online Z happy path + the offline drain finalize), applied as the opening
    // at the NEXT shift-open, and left UNTOUCHED by a force-close (so the next
    // open carries the force-closed shift's opening, exactly like prod).
    pub carry_cash_kop: i64,

    /// CS-3 operator-completion (1b) — the FN's CS-3 `PENDING_APPLY` RESERVATION-hold as
    /// `(held_lnd, online_origin, held_doc_type)`, if any (the fence enforces ≤1).  The document
    /// TYPE joined the tuple under bd `PRRO_GATE-k3y`: prod applies a §3.4 shift-state edge when the
    /// held document is shift-family, so the model cannot predict `shift_state` without it. It rides
    /// the SAME tuple rather than a companion field deliberately — a second field would carry an
    /// "always set together" convention, and this one is compiler-enforced instead.
    /// This is a reality-tracked
    /// PRECONDITION (the "is a completable hold resting, and of which origin" state), RE-SYNCED by the
    /// harness from the real `delivery_reservation` table after every op via
    /// `sync_held_reservation` — NOT independently predicted per-op.  It is NARROWER than "a Sending
    /// doc under STOP_MODE": a drain/crash transport failure commits SENDING + halts WITHOUT a CS-3
    /// reservation and is NOT operator-completable.  The release OUTCOME (`released_witness` axes)
    /// stays INDEPENDENTLY predicted + checked — only this precondition is state-synced (the pattern
    /// of `adopt_fault_deferred`, which syncs `docs`/`mode`/`session` from reality).
    pub held_reservation: Option<(i64, bool, DocType)>,

    /// CS-3 S7-2 — is the FN's **active-reservation fence** raised?  This is a STRICTLY WIDER
    /// predicate than `held_reservation.is_some()` and the two must not be conflated:
    /// `held_reservation` is only the `OUTCOME_OBSERVED` + `PENDING_APPLY` window (an
    /// operator-completable hold), while the fence also covers the IN-FLIGHT states
    /// `RESERVED_NOT_STARTED` / `CALL_STARTED` — an unresolved delivery with no completable hold.
    ///
    /// Found by the fuzzer at `FUZZ_CASES=4096`, shrunk to `[Crash(Send), Replenish(Granted)]`: a
    /// crash at the Send stage leaves `CALL_STARTED` residue, prod's T=112 refuses (correctly — the
    /// chain seed must not move while a delivery outcome is unknown), and a model gated on
    /// `held_reservation` predicted `granted`.  Adjudicated PROD = CORRECT.
    ///
    /// Like `held_reservation` this is a reality-synced PRECONDITION, not an independent prediction
    /// (`sync_fence_active`, after every op).  The honest consequence: the harness verifies *given the
    /// fence state, the fenced op behaves correctly* — it does NOT independently verify that the fence
    /// rises in the right circumstances.  That is the job of `tests/fn_fence_active.rs`.
    pub fence_active: bool,
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
            doc_prev: BTreeMap::new(),
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            opening_cash_kop: 0,
            cash_by_lnd: BTreeMap::new(), // L0: opening = 0 (first shift; carry is L3/L4)
            carry_cash_kop: 0,            // L0: no prior CLOSED shift → carry 0
            held_reservation: None,
            fence_active: false,
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
            doc_prev: BTreeMap::new(),
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            opening_cash_kop: 0,
            cash_by_lnd: BTreeMap::new(), // L0: no open shift → no cash-on-hand
            carry_cash_kop: 0,            // L0: no prior CLOSED shift → carry 0
            held_reservation: None,
            fence_active: false,
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
            doc_prev: BTreeMap::new(),
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            opening_cash_kop: 0,
            cash_by_lnd: BTreeMap::new(), // L0: opening = 0 (first shift)
            carry_cash_kop: 0,            // L0: no prior CLOSED shift → carry 0
            held_reservation: None,
            fence_active: false,
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
            doc_prev: BTreeMap::new(),
            shift_lifecycle_lnds: BTreeSet::new(),
            offline_origin_lnds: BTreeSet::new(),
            session_has_begin: false,
            session_has_end: false,
            opening_cash_kop: 0,
            cash_by_lnd: BTreeMap::new(), // L0: no open shift
            carry_cash_kop: 0,            // L0: no prior CLOSED shift → carry 0
            held_reservation: None,
            fence_active: false,
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

    /// bd `PRRO_GATE-6hl` — the model's mirror of prod `fiscal_documents::counted_in_turnover`:
    /// **is this document part of the shift's turnover right now?**
    ///
    /// Structurally the SAME three-part rule as prod, expressed through the model's OWN lane
    /// SSOTs so it cannot drift from the seed predicates the mint sites already use:
    ///
    /// ```text
    /// counted := state ∈ {ACK, OFFLINE_LOCAL_ACK}                       // the old literal
    ///         || ( model_is_issued(state, origin) && state ∉ {REJECTED, RMR} )
    /// ```
    ///
    /// The `origin` argument replaces prod's physical discriminators: an offline-origin doc is
    /// the one that got an `offline_fiscal_no` (the model tracks it in `offline_origin_lnds`),
    /// and the online arm keys on `server_fiscal_no`, which A.3 stamps at the `Sending → Sent`
    /// CAS — exactly the set `online_origin_advances_seed` names.  The parity is a TOOTH, not a
    /// convention: `teeth_6hl_turnover_matches_prod_counted_in_turnover` asserts this fn equals prod's
    /// for every `DocState` × lane under that physical coupling, so a drift on EITHER side is RED.
    pub fn counted_in_turnover(state: DocState, offline_origin: bool) -> bool {
        let issued = if offline_origin {
            Self::is_offline_origin_issued(state)
        } else {
            Self::online_origin_advances_seed(state)
        };
        matches!(state, DocState::Ack | DocState::OfflineLocalAck)
            || (issued
                && !matches!(
                    state,
                    DocState::Rejected | DocState::RequiresManualReconciliation
                ))
    }

    /// bd `PRRO_GATE-6hl` — is the document at `lnd` counted in turnover in its CURRENT state?
    /// `None` in `docs` (a doc the model never minted) counts as not-counted.
    fn doc_counted_in_turnover(&self, lnd: i64) -> bool {
        self.docs.get(&lnd).is_some_and(|state| {
            Self::counted_in_turnover(*state, self.offline_origin_lnds.contains(&lnd))
        })
    }

    /// bd `PRRO_GATE-6hl` — the DERIVED drawer (INV-21 / the X-report `cash_on_hand_kop`).
    ///
    /// The model's twin of prod `cash_on_hand_for_fn`: the open shift's opening anchor plus the
    /// cash legs of exactly those documents whose CURRENT state is turnover-counted.  There is no
    /// accumulator to keep in step — a state edge in either direction is reflected on the next
    /// read, which is precisely how production behaves.
    pub fn cash_on_hand(&self) -> i64 {
        self.opening_cash_kop + self.counted_cash_legs()
    }

    /// bd `PRRO_GATE-6hl` — the shift-aggregate half of `cash_on_hand()`: the legs of the
    /// currently-counted documents, WITHOUT the opening anchor (which `adopt_cash` solves for).
    fn counted_cash_legs(&self) -> i64 {
        self.cash_by_lnd
            .iter()
            .filter(|(lnd, _)| self.doc_counted_in_turnover(**lnd))
            .map(|(_, delta)| *delta)
            .sum()
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
            // T=112 replenish — no doc, no lnd; the seed and the code pool move (bd hpc).
            Op::Replenish(leaf) => self.apply_replenish(*leaf),
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
            // L3 service-io: online variant is wire-hitting; offline variant is
            // local-ack (same as OfflineSell/OfflineReturn — code consumed at OLA).
            // ServiceOut guard-3b is IN-LEASE ONLINE-ONLY; the offline lane uses
            // the pre-inbox L1 guard only — we do not model in-lease refusal here.
            Op::OnlineServiceIn(script) => self.apply_service_io(script, false),
            Op::OnlineServiceOut(script) => self.apply_service_io(script, true),
            Op::OfflineServiceIn => self.apply_offline_service_io(false),
            Op::OfflineServiceOut => self.apply_offline_service_io(true),
            Op::OnlineShiftOpen(script) => self.apply_online_shift_open(script),
            Op::OfflineShiftOpen => self.apply_offline_shift_open(),
            Op::OnlineZReport(script) => self.apply_online_z_report(script),
            Op::OfflineZReport => self.apply_offline_z_report(),
            // EPZ — видача готівки за ЕПЗ.  Online: wire-hitting cash-OUT (same
            // lnd/seed bookkeeping as ServiceOut, minus epz_out).  Offline:
            // local-ack (mirror OfflineServiceOut).
            Op::OnlineEpz(script) => self.apply_epz(script),
            Op::OfflineEpz => self.apply_offline_epz(),
            // L6 — X-report: a pure read.  Predicts NoMutation UNCONDITIONALLY
            // (nothing changes: no lnd, no seed, no code, no doc, no shift state)
            // — the model DELIBERATELY does not branch on shift/mode: even with
            // no open shift the prod read is a row-less 422, still a NoMutation.
            // The turnover-snapshot equality (cash-on-hand == self.cash_on_hand())
            // is asserted by the harness via `check_x_report_turnover`.
            Op::XReport => self.apply_x_report(),
            // L5 — input-guard probe.  ONLINE-ONLY: the L5 guards are ingress-layer
            // input validation and the interpreter refuses the probe on any
            // non-online node (the amount guards are orthogonal to offline
            // issuance) → NoMutation off-line, for EVERY kind.  On an online node:
            // the FOUR violation kinds are refused fail-closed PRE-INBOX by
            // `convert.rs` (G1..G4) — no inbox row, no fiscal_documents row, no
            // lnd/seed/code → NoMutation.  This prediction is INDEPENDENT of prod
            // (it follows from the guard's fail-closed contract, not from mirroring
            // the impl): reverting a prod guard makes convert admit + issue → prod
            // mints a row the model says must not exist → the harness's
            // ExpectedNoMutation "minted no row" assertion RED (the real teeth).
            // `Valid` (online) converts successfully and issues like an ordinary
            // online SELL (ack_path).
            Op::L5Probe(kind) => {
                if self.mode != NodeMode::Online {
                    ExpectedOutcome::NoMutation
                } else if *kind == L5Kind::Valid {
                    self.apply_sell(&DpsScript::ack_path(), false)
                } else {
                    ExpectedOutcome::NoMutation
                }
            }
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
                // CS-3 S7-1: mirror the interp — this adversarial force must NOT un-halt an
                // already-halted node (a held doc's STOP is only operator-cleared). When halted,
                // leave the halt; the sell is refused on STOP/BLOCKED just the same.
                if !matches!(self.mode, NodeMode::StopMode | NodeMode::Blocked) {
                    self.mode = NodeMode::GoingOnline;
                }
                ExpectedOutcome::NoMutation
            }
            Op::SellWithClosedShift => {
                self.shift_state = ShiftState::Closed;
                ExpectedOutcome::NoMutation
            }
            // A true replay (re-runs an already-DONE row) — no fiscal mutation.
            Op::DuplicateIdemKey => ExpectedOutcome::NoMutation,
            // CS-3 operator-completion (1b) — the generative release prediction. Finds the model's
            // HELD doc (≤1 by the fence), predicts a legal release (mutating self so the NEXT op sees
            // the un-halted node — the second half of the BRICK property) or a Refused completion
            // (origin cross-check / no hold), and returns `Release(Some|None)` for the release oracle.
            Op::OperatorComplete(kind) => self.apply_operator_complete(*kind),
        }
    }

    /// CS-3 operator-completion (1b) — predict + apply a completion against the model's HELD doc.
    ///
    /// The held doc is the single `Sending` doc (the fence enforces ≤1 active).  With no hold, or an
    /// origin cross-check contradiction, the completion is REFUSED (`Release(None)`) and `self` is
    /// UNCHANGED (the hold survives).  A legal completion RELEASES: the held doc transitions
    /// (`Sent` for Accepted / `RequiresManualReconciliation` otherwise), the node un-halts to the
    /// predicted mode (NEVER STOP_MODE — the anti-BRICK guarantee), and the seed advances for the
    /// seed-moving kinds (Accepted stamps the doc's chain tip; MacReseed re-bases).  Mutating `self`
    /// is what lets a SUBSEQUENT op issue again — the "next document passes" half of the property.
    /// bd `PRRO_GATE-6hl` — record `lnd`'s cash leg.  Called AT MINT, unconditionally on the
    /// document's landing state: whether the leg counts is `counted_in_turnover`'s job, re-asked on
    /// every read of `cash_on_hand()`.  There is deliberately NO inverse operation — the x5o
    /// `uncount_cash` it replaced existed only because the old accumulator had to be told, by hand,
    /// about each transition out of the counted set (and bd `PRRO_GATE-6hl` is what that costs when
    /// a new transition INTO the set appears and nobody adds the mirroring hook).
    fn record_cash(&mut self, lnd: i64, delta: i64) {
        self.cash_by_lnd.insert(lnd, delta);
    }

    /// Peer-tip axis PHASE C (spec §8.1) — MINT a document into the model ledger, recording the
    /// chain link it was minted onto.
    ///
    /// `or_insert`, not `insert`: prod's `previous_hash` is written once and is immutable
    /// thereafter, so no later edge may rewrite it.  Plain state TRANSITIONS therefore keep calling
    /// `self.docs.insert` directly — that split is what makes "the link is recorded at mint, and
    /// only at mint" a greppable rule rather than a convention every future arm has to remember.
    fn mint_doc(&mut self, lnd: i64, state: DocState, previous_hash: Option<[u8; 32]>) {
        self.docs.insert(lnd, state);
        self.doc_prev.entry(lnd).or_insert(previous_hash);
    }

    /// bd `PRRO_GATE-6hl` — re-anchor the DERIVED drawer onto a real one (the fault re-sync).
    ///
    /// The model keeps its per-document legs (they stay live for future state edges) and moves the
    /// OPENING anchor to absorb whatever reality holds that those legs do not explain — a
    /// crash-completed doc the model never predicted, most of all.  Post-condition:
    /// `self.cash_on_hand() == real_cash`, exactly as the old scalar assignment guaranteed.
    ///
    /// The caller is responsible for re-scoping `cash_by_lnd` to reality's OPEN shift FIRST (see
    /// `adopt_fault_deferred`): the anchor absorbs the unexplained remainder, so a leg left in the
    /// map from a shift reality has since closed would be absorbed with the WRONG sign and then
    /// move the drawer again on its next state edge.
    pub fn adopt_cash(&mut self, real_cash: i64) {
        self.opening_cash_kop = real_cash - self.counted_cash_legs();
    }

    /// bd `PRRO_GATE-6hl` — a NEW shift's drawer: prod's aggregates are scoped to the open
    /// `shift_id`, so the prior shift's legs stop counting the moment a new shift opens, and the
    /// opening anchor becomes the carry (`shifts.cash_balance_kop` of the last REAL close).
    fn open_shift_cash_anchor(&mut self) {
        self.cash_by_lnd.clear();
        self.opening_cash_kop = self.carry_cash_kop;
    }

    fn apply_operator_complete(&mut self, kind: OperatorResolutionKind) -> ExpectedOutcome {
        // A completion releases ONLY a doc under a CS-3 PENDING_APPLY RESERVATION-hold — tracked
        // explicitly in `held_reservation_lnd` (set when an online wire op holds; re-derived from the
        // real `delivery_reservation` table on a fault re-sync). This is NARROWER than "a Sending doc
        // under STOP_MODE": a drain / crash transport failure can commit a doc SENDING + halt the node
        // WITHOUT a CS-3 reservation (verified — real `operator_complete` returns "no held reservation
        // rests" there), and such a doc is NOT operator-completable. Absent ⇒ inert no-op → Refused.
        let Some((held_lnd, online_origin, held_doc_type)) = self.held_reservation else {
            return ExpectedOutcome::Release(None);
        };
        // Active offline session (prod checks OPENING/OPEN/DRAINING). The current alphabet holds only
        // on an ONLINE node with no offline session, so this is None ⇒ false; `is_some()` is a safe
        // over-approximation for a future offline-hold slice.
        let has_active_offline_session = self.session.is_some();
        let origin_blocked = self.mode == NodeMode::Blocked;
        let Some(witness) = released_witness(
            kind,
            online_origin,
            has_active_offline_session,
            origin_blocked,
        ) else {
            // Origin cross-check contradiction: prod fail-closes BEFORE any mutation — hold intact.
            return ExpectedOutcome::Release(None);
        };
        // bd PRRO_GATE-k3y — the §3.4 operator SHIFT edge, applied by
        // `operator_completion::complete_operator_resolution` in the SAME tx as the core. Before
        // that ticket the admin CLI called the core directly, so prod moved no shift state and this
        // model matched a BYPASS; the model is the reason the regression would have stayed
        // invisible. Computed here (no mutation yet) because prod applies it FIRST — a projection
        // refusal rolls the whole completion back, hold intact.
        let shift_after = match operator_shift_edge(held_doc_type, online_origin, kind) {
            // `MacReseedOnShiftFamily`: §3.4 has no cell for a chain-seed correction on a
            // shift-family document. Prod errors out before touching anything.
            ShiftEdge::Refused => return ExpectedOutcome::Release(None),
            ShiftEdge::Move(from, to) => {
                if self.shift_state != from {
                    // Prod's `apply_shift_transition` CAS returns non-`Applied` →
                    // `ShiftProjectionFailed` → the whole `BEGIN IMMEDIATE` rolls back.
                    return ExpectedOutcome::Release(None);
                }
                Some(to)
            }
            ShiftEdge::NoEdge => None,
        };
        // bd PRRO_GATE-2nk (§5a) — OFFLINE-origin `NotAcceptedOffline` RELEASE: the gap-4b cohort-cancel
        // + chain rewind (delivery_reservation.rs offline_cohort_cleanup). Reached only for an offline
        // hold (an online-origin NotAcceptedOffline already returned Release(None) via the witness gate).
        if matches!(kind, OperatorResolutionKind::NotAcceptedOffline) {
            // FORK GUARD (offline_cohort_cleanup): a later successor that is neither a cancellable
            // OFFLINE_LOCAL_ACK nor already dead (CANCELLED/ABORTED) → prod REFUSES, mutating nothing.
            // One-session approximation ("all later lnds" ≈ same session) is safe for this alphabet
            // (a halted offline hold admits no online-origin successor); a multi-session breach would RED.
            let later_fork = self.docs.range((held_lnd + 1)..).any(|(_, st)| {
                !matches!(
                    st,
                    DocState::OfflineLocalAck | DocState::Cancelled | DocState::Aborted
                )
            });
            if later_fork {
                return ExpectedOutcome::Release(None);
            }
            let cancel: Vec<i64> = self
                .docs
                .range((held_lnd + 1)..)
                .filter(|(_, st)| **st == DocState::OfflineLocalAck)
                .map(|(l, _)| *l)
                .collect();
            for l in cancel {
                // bd PRRO_GATE-x5o: a CANCELLED doc leaves the turnover set, so its cash leg stops
                // counting — under bd PRRO_GATE-6hl that needs no reversal call: `cash_on_hand()`
                // re-asks `counted_in_turnover` about the doc's CURRENT state on the next read.
                self.docs.insert(l, DocState::Cancelled);
            }
            // spec §8.1 — rewind to the held doc's OWN immutable chain link, the same column prod
            // reads back.  This used to be `self.seed = None`, a structural marker whose exact
            // value `run_harness` then checked relationally against the ledger; see `doc_prev` for
            // why a marker stops being good enough once the tip has to be COMPARED.
            //
            // Absent from the map ⇒ the model never saw this document minted (it was adopted across
            // a fault window, and `adopt_fault_deferred` re-derives every link precisely so this
            // cannot happen).  Falling back to genesis rather than panicking is deliberate: the
            // harness's phase-C tip assertion names the drift far more precisely than a panic here
            // would, and it names it at the op that caused it.
            self.seed = self.doc_prev.get(&held_lnd).copied().flatten();

            // The held doc escalates to RMR, which is ALSO outside the turnover set — again
            // nothing to reverse by hand (the leg x5o had to remember, 6hl made structural).
            self.docs
                .insert(held_lnd, DocState::RequiresManualReconciliation);
            self.mode = node_mode_from_str(witness.node_mode); // GOING_ONLINE (active session drains)
                                                               // k3y: an OFFLINE shift-family `NotAcceptedOffline` also rolls the OLPD/CLPD edge back.
            if let Some(to) = shift_after {
                self.shift_state = to;
            }
            return ExpectedOutcome::Release(Some(witness));
        }
        // Release: transition the held doc, un-halt the node, advance the seed for the seed-movers.
        let new_doc_state = match kind {
            OperatorResolutionKind::Accepted => DocState::Sent,
            _ => DocState::RequiresManualReconciliation,
        };
        self.docs.insert(held_lnd, new_doc_state);
        self.mode = node_mode_from_str(witness.node_mode);
        // k3y: the §3.4 shift edge commits with the rest of the completion (ONE tx).
        if let Some(to) = shift_after {
            self.shift_state = to;
        }
        // Seed advance mirrors prod completion: Accepted advances ONLY for an ONLINE-origin doc
        // (delivery_reservation.rs:1374 `if online`) — an OFFLINE-origin doc already advanced the
        // seed at issuance, so its Accepted completion leaves the tip untouched.  MacReseed is
        // ONLINE-only and re-bases the tip.  The differential seed check is STRUCTURAL (advanced iff
        // the model advanced), so the exact value only needs to CHANGE from the pre-completion tip.
        let advances_seed = match kind {
            OperatorResolutionKind::Accepted => online_origin,
            OperatorResolutionKind::MacReseed => true,
            OperatorResolutionKind::NotAccepted | OperatorResolutionKind::NotAcceptedOffline => {
                false
            }
        };
        if advances_seed {
            self.seed = Some(synth_unsigned_hash(held_lnd));
        }
        // The hold is discharged (`held_reservation` is re-synced from reality by the harness after
        // this op — the fence is now clear, so a subsequent op can hold anew).
        ExpectedOutcome::Release(Some(witness))
    }

    /// bd `PRRO_GATE-hpc` — predict a T=112 replenish.
    ///
    /// `Granted`: prod inserts the granted codes (`INSERT OR IGNORE`) and advances the chain seed to
    /// `sha256(request_xml)` — a value the model CANNOT compute (it builds no XML), so the advance is
    /// recorded STRUCTURALLY, exactly as the `NotAcceptedOffline` rewind marker is.  The synthetic
    /// value is keyed off a NEGATIVE ordinal so it can never collide with a document hash
    /// (`synth_unsigned_hash(lnd)` for a real doc always uses a non-negative `lnd`).
    ///
    /// **`next_lnd` deliberately does NOT move** — that is what makes the witness's `lnd_at_write`
    /// ordering frame meaningful, and what the after-SELL tie-break in
    /// `active_chain_tip_unsigned_xml_sha256` depends on.
    ///
    /// `ServerReject`: prod persists NOTHING — no codes, no seed advance, no witness.
    fn apply_replenish(&mut self, leaf: ReplenishLeaf) -> ExpectedOutcome {
        // S7-2 fence (FOUND GENERATIVELY, 2026-07-31): prod refuses a replenish outright while the FN
        // has an ACTIVE delivery reservation — `offline_code_replenish.rs` calls
        // `delivery_reservation::fn_fence_active_tx` INSIDE the envelope and bails with
        // "replenish refused: FN … has an active delivery reservation (S7-2 fence)" before touching
        // the code pool or the seed. The T=112 seed advance would otherwise race a delivery that is
        // mid-flight on the same chain. The model must predict that refusal or the differential
        // diverges the moment the generator lands a Replenish after a held-producing op.
        //
        // The gate is `fence_active`, NOT `held_reservation.is_some()`. That distinction was found the
        // hard way: the first cut gated on the hold, and the 4096 capstone shrank a failure to
        // `[Crash(Send), Replenish(Granted)]` — a crash at Send leaves `CALL_STARTED` residue, which
        // raises prod's fence but produces NO completable hold, so the model predicted `granted` while
        // prod refused. Adjudicated PROD = CORRECT (the seed must not move while a delivery outcome is
        // unknown); the model was widened to prod's own predicate. That seed is pinned in
        // `invariant_fuzzer.regressions` and in the directed tooth
        // `replenish_refused_after_crash_at_send`.
        if self.fence_active {
            return ExpectedOutcome::Replenish { granted: false };
        }
        // bd PRRO_GATE-knk — prod refuses a replenish while an UNDRAINED offline backlog rests.
        //
        // An offline document advances OUR seed at `OFFLINE_LOCAL_ACK`, i.e. at local issuance,
        // before DPS ever sees it. So while such a doc rests, the `<MAC>` the T=112 would carry is a
        // value DPS has never accepted — and the live TEST-cabinet probe of 2026-08-01 settled that
        // DPS chain-checks the request and answers `-12 ERROR_BAD_HASH_PREV` without moving its tip.
        // Prod now refuses locally, BEFORE the wire, because reaching DPS costs a `MacReseedPending`
        // → `STOP_MODE` the operator has no documented way out of.
        //
        // The predicate mirrors prod's FULL disjunction rather than just the obvious state: prod
        // counts `offline_fiscal_no IS NOT NULL AND state IN ('OFFLINE_LOCAL_ACK','ERROR_RETRYABLE')`
        // — `ErrorRetryable` is in because such a doc already crossed `OFFLINE_LOCAL_ACK` and still
        // has no DPS acceptance. Gating BOTH arms on `offline_origin_lnds` mirrors the
        // `offline_fiscal_no IS NOT NULL` clause, so an ONLINE-origin `ErrorRetryable` (which never
        // advanced the seed — A.3 advances at the `Sending → Sent` CAS) does NOT block, exactly as
        // in production.
        //
        // Note the shape this makes VISIBLE rather than hides: the generator's
        // `ReplenishLeaf::Granted` was free-choice, so the harness used to model a granted replenish
        // in a state where production would have earned `-12`. Prod refusing here is what removes
        // that particular piece of vacuous coverage; the general `Granted`-leaf constraint against
        // the PEER tip is still phase B's job.
        let undrained_offline_backlog = self.docs.iter().any(|(lnd, st)| {
            self.offline_origin_lnds.contains(lnd)
                && matches!(st, DocState::OfflineLocalAck | DocState::ErrorRetryable)
        });
        if undrained_offline_backlog {
            return ExpectedOutcome::Replenish { granted: false };
        }
        match leaf {
            ReplenishLeaf::Granted => {
                self.codes_issued += 1;
                self.seed = Some(synth_unsigned_hash(-self.codes_issued - 1));
                ExpectedOutcome::Replenish { granted: true }
            }
            ReplenishLeaf::ServerReject => ExpectedOutcome::Replenish { granted: false },
        }
    }

    /// L6 — X-report (поточний звіт): a pure read.  ALWAYS `NoMutation` — the
    /// snapshot never allocates an lnd, advances the seed, consumes a code, mints
    /// a doc/inbox row, or transitions the shift.  The turnover-snapshot equality
    /// (`self.cash_on_hand()`) is checked by the harness against the real
    /// `XReportPayload.cash_on_hand_kop`; the model itself does not mutate.
    fn apply_x_report(&self) -> ExpectedOutcome {
        ExpectedOutcome::NoMutation
    }

    /// A POST-SIGN refusal: reality reaches `SIGNED` (the lnd IS allocated), then
    /// a dispatcher / offline-ack refusal aborts it → a non-issued `Aborted` row
    /// (mirrors the prod `terminalise_inbox` seam, migration 025).  Like the
    /// online-reject non-issued row, the lnd is consumed (`next_lnd` advances) but
    /// the seed does NOT advance and no code is consumed.  Classified `NoMutation`
    /// (no issuance) — a row exists but no receipt was issued.
    fn mint_aborted_refusal(&mut self) -> ExpectedOutcome {
        let lnd = self.next_lnd;
        self.mint_doc(lnd, DocState::Aborted, self.seed);
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
    /// INV-21 does NOT refuse here.  The derived drawer (`cash_on_hand()`) tracks
    /// what prod actually does (may go negative on empty-drawer returns); the
    /// `check_cash_on_hand` oracle detects divergence independently.
    /// Routes through `apply_sell` with `is_return=true` for the SSOT cash delta.
    fn apply_return(&mut self, script: &DpsScript) -> ExpectedOutcome {
        self.apply_sell(script, true)
    }

    /// L3 — Online service cash-in (`is_out=false`) or cash-out (`is_out=true`).
    ///
    /// **Wire semantics:** online-only; same lnd/seed bookkeeping as the online arm
    /// of `apply_sell` (D5 gate, advance-at-SEND, Rejected→NoIssuanceRow, etc.).
    ///
    /// **Cash accounting:** counted at ACK (mirrors `aggregate_shift_cash` which
    /// filters `state IN ('ACK','OFFLINE_LOCAL_ACK')`):
    ///   ServiceIn  → `cash_on_hand += CASH_AMOUNT_KOP`
    ///   ServiceOut → `cash_on_hand -= CASH_AMOUNT_KOP`
    ///
    /// **Guard-3b (INV-21):** ServiceOut is refused fail-closed PRE-MINT (NoMutation)
    /// when `cash_on_hand < CASH_AMOUNT_KOP`.  Mirrors the in-lease check added in
    /// `stage_acquire` for ServiceOut (online-only; the fuzzer enters at
    /// `inline::run` downstream of `convert.rs`, so only the in-lease arm is
    /// in-lane here).
    fn apply_service_io(&mut self, script: &DpsScript, is_out: bool) -> ExpectedOutcome {
        // Service-io is online-only in production.
        if self.mode != NodeMode::Online {
            return ExpectedOutcome::NoMutation;
        }
        if !self.shift_is_open() {
            return ExpectedOutcome::NoMutation;
        }
        // Guard-3b (in-lease): ServiceOut refused pre-mint when drawer insufficient.
        if is_out && self.cash_on_hand() < CASH_AMOUNT_KOP {
            return ExpectedOutcome::NoMutation;
        }
        // D5 gate: refused pre-mint while a non-issued sibling rests.
        if self.has_write_gate_blocker() {
            return ExpectedOutcome::NoMutation;
        }
        // `-12` is NOT fault-class — see the note on the same leaf in `apply_sell`:
        // S7-1 R3 retired the auto re-sign, so the shared HELD path below predicts
        // it exactly (doc rests `Sending`, node STOP_MODE, seed unmoved).
        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        let doc_state = online_outcome_state(script);
        if doc_state == DocState::Sending {
            // CS-3 S7-1: a HELD online outcome (ambiguous / Superseded / transient after
            // CALL_STARTED) flips the node to STOP_MODE — record_outcome's early node-safety halt.
            self.mode = NodeMode::StopMode;
        }
        self.mint_doc(lnd, doc_state, previous_hash);
        self.next_lnd += 1;
        // bd PRRO_GATE-a6n — cash from the ISSUANCE boundary, not from ACK: prod's
        // `aggregate_shift_service_io` selects `counted_in_turnover`, which for the online
        // lane counts from the `Sending → Sent` CAS (sfn stamped).  bd PRRO_GATE-6hl — the
        // leg is recorded AT MINT whatever the landing state, exactly as prod stores the
        // `amount_kop` payload at mint; `cash_on_hand()` asks the predicate.  That is what
        // lets a later `Sending → Sent` operator completion bring this leg INTO the drawer
        // without a hook here.
        if is_out {
            self.record_cash(lnd, -CASH_AMOUNT_KOP);
        } else {
            self.record_cash(lnd, CASH_AMOUNT_KOP);
        }
        if Self::online_origin_advances_seed(doc_state) {
            self.seed = Some(unsigned_hash);
        }
        if doc_state == DocState::Rejected {
            return ExpectedOutcome::NoIssuanceRow;
        }
        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state,
            seed_after: self.seed,
            previous_hash,
            code_consumed: self.code_consumed_observable(),
            shift_state_after: None,
        })
    }

    /// L3 — Offline service cash-in/out.  Mirrors `OfflineSell`/`OfflineReturn`:
    /// local issuance (OFFLINE_LOCAL_ACK + code consumed + seed advance), no wire.
    ///
    /// **Guard-3b:** the in-lease guard is ONLINE-ONLY (`stage_acquire` scopes
    /// the check to `Channel::Online`).  Offline ServiceOut only has the
    /// pre-inbox L1 guard (convert.rs), which is downstream of `inline::run`.
    /// We do not model the L1 refusal here — the fuzzer fixture ensures
    /// sufficient cash via prior `OfflineServiceIn` (or starts with enough).
    ///
    /// **Cash accounting:** counted at OLA, same as offline SELL/RETURN.
    fn apply_offline_service_io(&mut self, is_out: bool) -> ExpectedOutcome {
        // Offline lane requires Offline mode + open offline session.
        if self.mode != NodeMode::Offline {
            return ExpectedOutcome::NoMutation;
        }
        if self.session != Some(OfflineSessionState::Open) {
            return ExpectedOutcome::NoMutation;
        }
        if !self.shift_is_open() {
            return ExpectedOutcome::NoMutation;
        }

        // NOTE: T2 close-reserve (enforce_offline_close_reserve, inline.rs:1257) is
        // scoped to DocType::Sell | DocType::Return only.  ServiceIn/Out bypass it
        // unconditionally — do NOT apply the reserve gate here.

        // B10 — lazy DocType=9 BEGIN interposition, mirrored from apply_sell's
        // offline arm.  On the FIRST offline doc of a session the impl mints+OLAs
        // a BEGIN before the business doc.  The harness calls check_ledger_delta
        // when real=Recovered{b10_lazy_begin_interposed}, so the model MUST predict
        // BOTH the BEGIN and the business doc in self.docs.
        //   0 codes → NoMutation (pre-mint pool guard, no rows)
        //   1 code  → BEGIN(OLA); pool exhausted → Aborted business row (Fault)
        //   ≥2 codes → BEGIN(OLA) + ServiceIn/Out(OLA)
        if !self.session_has_begin {
            if self.codes_consumed >= self.codes_issued {
                return ExpectedOutcome::NoMutation;
            }
            self.session_has_begin = true;
            let begin_lnd = self.next_lnd;
            let begin_unsigned = synth_unsigned_hash(begin_lnd);
            self.mint_doc(begin_lnd, DocState::OfflineLocalAck, self.seed);
            self.offline_origin_lnds.insert(begin_lnd);
            self.next_lnd += 1;
            self.codes_consumed += 1;
            self.seed = Some(begin_unsigned);
            // fall through to the business doc (chains off BEGIN's seed)
        }

        // Business doc — after any lazy BEGIN above.
        if self.codes_consumed >= self.codes_issued {
            // Pool exhausted for the business doc → offline-ack aborts it.
            return self.mint_aborted_refusal();
        }
        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        self.mint_doc(lnd, DocState::OfflineLocalAck, previous_hash);
        self.offline_origin_lnds.insert(lnd);
        self.next_lnd += 1;
        self.codes_consumed += 1;
        // Offline-origin advances seed at OFFLINE_LOCAL_ACK (spec § offline chain).
        self.seed = Some(unsigned_hash);
        // Cash update at OLA (mirrors aggregate_shift_cash filter
        // `state IN ('ACK','OFFLINE_LOCAL_ACK')`).
        if is_out {
            self.record_cash(lnd, -CASH_AMOUNT_KOP);
        } else {
            self.record_cash(lnd, CASH_AMOUNT_KOP);
        }
        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state: DocState::OfflineLocalAck,
            seed_after: self.seed,
            previous_hash,
            code_consumed: self.code_consumed_observable(),
            shift_state_after: None,
        })
    }

    /// EPZ — видача готівки за ЕПЗ (online, wire-hitting).
    ///
    /// Chain-wise IDENTICAL to the online `apply_service_io(is_out=true)` arm
    /// (same lnd/seed bookkeeping, D5 gate, advance-at-SEND, Rejected→NoIssuanceRow).
    ///
    /// **Cash accounting (`− epz_out`):** counted at ACK (mirrors
    /// `aggregate_shift_epz` which filters `state IN ('ACK','OFFLINE_LOCAL_ACK')`):
    ///   EPZ ACK → `cash_on_hand -= CASH_AMOUNT_KOP`.
    /// EPZ is a CARD payform for turnover (no cash `<M>`); the drawer decrement
    /// is a LEDGER effect only.  The model tracks only the drawer (`cash_on_hand`),
    /// so card-turnover is not separately accumulated here.
    ///
    /// **Guard-3c (INV-21):** refused fail-closed PRE-MINT (NoMutation) when
    /// `cash_on_hand < CASH_AMOUNT_KOP` — mirrors the in-lease check added in
    /// `stage_acquire` for EPZ (online-only; the fuzzer enters at `inline::run`
    /// downstream of `convert.rs`, so only the in-lease arm is in-lane here).
    fn apply_epz(&mut self, script: &DpsScript) -> ExpectedOutcome {
        // EPZ online lane is online-only.
        if self.mode != NodeMode::Online {
            return ExpectedOutcome::NoMutation;
        }
        if !self.shift_is_open() {
            return ExpectedOutcome::NoMutation;
        }
        // Guard-3c (in-lease): EPZ refused pre-mint when the drawer is insufficient.
        if self.cash_on_hand() < CASH_AMOUNT_KOP {
            return ExpectedOutcome::NoMutation;
        }
        // D5 gate: refused pre-mint while a non-issued sibling rests.
        if self.has_write_gate_blocker() {
            return ExpectedOutcome::NoMutation;
        }
        // `-12` is NOT fault-class — see the note on the same leaf in `apply_sell`:
        // S7-1 R3 retired the auto re-sign, so the shared HELD path below predicts
        // it exactly (doc rests `Sending`, node STOP_MODE, seed unmoved).
        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        let doc_state = online_outcome_state(script);
        if doc_state == DocState::Sending {
            // CS-3 S7-1: a HELD online outcome (ambiguous / Superseded / transient after
            // CALL_STARTED) flips the node to STOP_MODE — record_outcome's early node-safety halt.
            self.mode = NodeMode::StopMode;
        }
        self.mint_doc(lnd, doc_state, previous_hash);
        self.next_lnd += 1;
        // bd PRRO_GATE-a6n — cash-OUT from the ISSUANCE boundary, not from ACK (prod's
        // `aggregate_shift_epz` selects `counted_in_turnover`).  bd PRRO_GATE-6hl — recorded
        // AT MINT regardless of landing state; the predicate decides on read.
        self.record_cash(lnd, -CASH_AMOUNT_KOP);
        if Self::online_origin_advances_seed(doc_state) {
            self.seed = Some(unsigned_hash);
        }
        if doc_state == DocState::Rejected {
            return ExpectedOutcome::NoIssuanceRow;
        }
        ExpectedOutcome::Mutated(Mutation {
            lnd,
            doc_state,
            seed_after: self.seed,
            previous_hash,
            code_consumed: self.code_consumed_observable(),
            shift_state_after: None,
        })
    }

    /// EPZ — видача готівки за ЕПЗ (offline).  Mirrors `apply_offline_service_io`
    /// (is_out semantics): local issuance (OFFLINE_LOCAL_ACK + code consumed +
    /// seed advance), no wire; cash-OUT at OLA.  Guard-3c is ONLINE-ONLY in-lease
    /// (like guard-3b), so the offline lane relies on the pre-inbox guard +
    /// the durable local ledger — the fixture ensures sufficient cash.
    fn apply_offline_epz(&mut self) -> ExpectedOutcome {
        // Delegate to the offline service-io lane with is_out=true; it applies
        // the identical local-ack / code-consume / seed-advance bookkeeping and
        // subtracts CASH_AMOUNT_KOP at OLA — the same drawer decrement EPZ needs.
        self.apply_offline_service_io(true)
    }

    /// Online SHIFT_OPEN.  Closed → Opening at acquire, then Opening → Opened
    /// once the doc crosses SEND.  ACK only confirms.
    ///
    /// RAGE W1 — the acquire-time `Closed → Opening` transition PERSISTS under
    /// an `ErrorRetryable` send outcome (e.g. `Superseded` →
    /// `ServerFiscalIdMismatch` → `WrapperBug` → `ErrorRetryable`,
    /// `error_routing.rs:350`, doc-type-agnostic).  Prod stamps `Opening` at
    /// acquire (`stage_acquire.rs:883-910`) and the confirm edge that would
    /// drive `Opening → Opened` runs ONLY inside `if let WireDecision::Sent`
    /// (`stage_send.rs:1758`), so a retryable send does NOT confirm and does
    /// NOT roll back — the shift rests at `Opening`.  The model must mirror
    /// that: the `ErrorRetryable` fall-through leaves the shift at `Opening`
    /// (mid-open), NOT reverted to `Closed` and NOT escalated to RMR (that is
    /// the `Rejected` arm only), with NO seed advance (advance-at-SEND / D2:
    /// `ErrorRetryable` has no sfn and never crosses the SEND boundary).
    fn apply_online_shift_open(&mut self, script: &DpsScript) -> ExpectedOutcome {
        if self.mode != NodeMode::Online || self.shift_state != ShiftState::Closed {
            return ExpectedOutcome::NoMutation;
        }
        if self.has_write_gate_blocker() {
            return ExpectedOutcome::NoMutation;
        }
        // `-12` is NOT fault-class — see the note on the same leaf in `apply_sell`:
        // S7-1 R3 retired the auto re-sign, so the shared HELD path below predicts
        // it exactly (doc rests `Sending`, node STOP_MODE, seed unmoved).

        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        let doc_state = online_outcome_state(script);
        if doc_state == DocState::Sending {
            // CS-3 S7-1: a HELD online outcome (ambiguous / Superseded / transient after
            // CALL_STARTED) flips the node to STOP_MODE — record_outcome's early node-safety halt.
            self.mode = NodeMode::StopMode;
        }
        self.mint_doc(lnd, doc_state, previous_hash);
        self.shift_lifecycle_lnds.insert(lnd);
        self.next_lnd += 1;

        if Self::online_origin_advances_seed(doc_state) {
            self.seed = Some(unsigned_hash);
            self.shift_state = ShiftState::Opened;
            // Cross-shift cash carry (mirrors prod `cash_ledger.rs:14`
            // `opening_cash = prior shift's cash_balance_kop`): a fresh shift opens
            // with `cash_on_hand = <carry>`. The carry is the CLOSING drawer of the
            // last REAL close (set in `carry_cash_kop`); 0 for the FN's first shift
            // and after a non-persisting force-close. This replaces the old blanket
            // reset-to-0, which was only sound under the single-open-shift-per-fixture
            // assumption that the multi-shift alphabet (RAGE W1) broke — the
            // `Sell → Z → ShiftOpen → EPZ` counterexample where prod carries 15000.
            self.open_shift_cash_anchor();
        } else if doc_state == DocState::Rejected {
            self.shift_state = ShiftState::RequiresManualReconciliation;
            return ExpectedOutcome::NoIssuanceRow;
        } else {
            // RAGE W1 — `ErrorRetryable` (Superseded / WrapperBug): the
            // acquire-time `Closed → Opening` transition PERSISTS (prod does
            // not confirm and does not roll back a retryable send).  No seed
            // advance (D2 / advance-at-SEND), no RMR escalation.
            self.shift_state = ShiftState::Opening;
            // Cross-shift carry also applies to a HELD open: prod stamps the new
            // shift's opening `cash_balance_kop = <carry>` at ACQUIRE
            // (`stage_acquire.rs:883-910`), BEFORE the send outcome, so an
            // `Opening`-resting shift is read by `cash_on_hand_for_fn` (which
            // excludes only CLOSED/RMR/ERROR/CREATED) at the CARRIED opening — NOT
            // the stale prior-shift drawer. Counterexample (offline harness, seed
            // 2026-07-23): `[OfflineServiceIn, GoOnline, SellWithClosedShift,
            // OnlineShiftOpen([Superseded]), XReport]` → real 0 vs stale model
            // 15000. The Rejected arm above escalates to RMR (excluded from the
            // cash read), so it deliberately does NOT re-anchor.
            self.open_shift_cash_anchor();
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
            self.mint_doc(begin_lnd, DocState::OfflineLocalAck, self.seed);
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
        self.mint_doc(lnd, DocState::OfflineLocalAck, previous_hash);
        self.shift_lifecycle_lnds.insert(lnd);
        self.offline_origin_lnds.insert(lnd);
        self.next_lnd += 1;
        self.codes_consumed += 1;
        self.seed = Some(unsigned_hash);
        self.shift_state = ShiftState::OpenedLocalPendingDrain;
        // Cross-shift cash carry (offline twin of `apply_online_shift_open`): a fresh
        // shift opens with `cash_on_hand = <carry>` (prod `cash_ledger.rs:14`). This
        // is the RE-AUDIT the old HARD-ZERO comment demanded once a real Z-close that
        // persists `cash_balance_kop` before a reopen entered the alphabet — the carry
        // is set at the offline drain finalize (the real close) / online Z, and is 0
        // for the FN's first shift or after a non-persisting force-close.
        self.open_shift_cash_anchor();

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
    ///
    /// RAGE W1 — the acquire-time `Opened → Closing` transition PERSISTS under
    /// an `ErrorRetryable` send outcome (e.g. `Superseded` →
    /// `ServerFiscalIdMismatch` → `WrapperBug` → `ErrorRetryable`,
    /// `error_routing.rs:350`, doc-type-agnostic).  Prod stamps `Closing` at
    /// acquire (`stage_acquire.rs:917-935`) and the confirm edge that would
    /// drive `Closing → Closed` runs ONLY inside `if let WireDecision::Sent`
    /// (`stage_send.rs:1758`), so a retryable send does NOT confirm and does
    /// NOT roll back — the shift rests at `Closing`.  The model must mirror
    /// that: the `ErrorRetryable` fall-through leaves the shift at `Closing`
    /// (mid-close), NOT reverted to `Opened` and NOT escalated to RMR (that is
    /// the `Rejected` arm only), with NO seed advance (advance-at-SEND / D2:
    /// `ErrorRetryable` has no sfn and never crosses the SEND boundary).
    fn apply_online_z_report(&mut self, script: &DpsScript) -> ExpectedOutcome {
        if self.mode != NodeMode::Online || self.shift_state != ShiftState::Opened {
            return ExpectedOutcome::NoMutation;
        }
        // A live Z first quiesces the shift.  If the model sees a non-terminal
        // online blocker, reality refuses before minting a Z row.
        if self.has_z_quiescence_blocker() {
            return ExpectedOutcome::NoMutation;
        }
        // `-12` is NOT fault-class — see the note on the same leaf in `apply_sell`:
        // S7-1 R3 retired the auto re-sign, so the shared HELD path below predicts
        // it exactly (doc rests `Sending`, node STOP_MODE, seed unmoved).
        let lnd = self.next_lnd;
        let previous_hash = self.seed;
        let unsigned_hash = synth_unsigned_hash(lnd);
        let doc_state = online_outcome_state(script);
        if doc_state == DocState::Sending {
            // CS-3 S7-1: a HELD online outcome (ambiguous / Superseded / transient after
            // CALL_STARTED) flips the node to STOP_MODE — record_outcome's early node-safety halt.
            self.mode = NodeMode::StopMode;
        }
        self.mint_doc(lnd, doc_state, previous_hash);
        self.shift_lifecycle_lnds.insert(lnd);
        self.next_lnd += 1;

        if Self::online_origin_advances_seed(doc_state) {
            self.seed = Some(unsigned_hash);
            self.shift_state = ShiftState::Closed;
            // Cross-shift carry: a real SHIFT_CLOSE persists the CLOSING drawer as
            // `shifts.cash_balance_kop` (prod `cash_ledger.rs:16`), which becomes the
            // NEXT shift's opening carry. Mirror it so a reopen after this Z sees the
            // carried balance (the `Sell → Z → ShiftOpen → EPZ` counterexample).
            self.carry_cash_kop = self.cash_on_hand();
        } else if doc_state == DocState::Rejected {
            // Current production escalates shift-class send rejects to RMR while
            // the document itself rests as non-issued Rejected.
            self.shift_state = ShiftState::RequiresManualReconciliation;
            return ExpectedOutcome::NoIssuanceRow;
        } else {
            // RAGE W1 — `ErrorRetryable` (Superseded / WrapperBug): the
            // acquire-time `Opened → Closing` transition PERSISTS (prod does
            // not confirm and does not roll back a retryable send).  No seed
            // advance (D2 / advance-at-SEND), no RMR escalation.
            self.shift_state = ShiftState::Closing;
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
            self.mint_doc(begin_lnd, DocState::OfflineLocalAck, self.seed);
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
        self.mint_doc(lnd, DocState::OfflineLocalAck, previous_hash);
        self.shift_lifecycle_lnds.insert(lnd);
        self.offline_origin_lnds.insert(lnd);
        self.next_lnd += 1;
        self.codes_consumed += 1;
        self.seed = Some(unsigned_hash);
        self.shift_state = ShiftState::ClosingLocalPendingDrain;
        // NOTE (fuzzer-refuted 2026-07-16): do NOT reset `cash_on_hand` here.
        // An offline Z_REPORT moves the shift to `ClosingLocalPendingDrain`,
        // which is the SAME shift (same `current_shift_id`) merely pending its
        // drain-time close — it is NOT yet CLOSED.  Prod's per-open-shift
        // `cash_on_hand_for_fn` (`cash_ledger.rs:398`) excludes only
        // {CLOSED, REQUIRES_MANUAL_RECONCILIATION, ERROR, CREATED}, so it STILL
        // reports this shift's full turnover until the drain actually closes it.
        // The offline shift-OPEN tail zeroes because THAT mints a brand-new
        // shift (fresh opening anchor 0); an offline Z-report does not.  A hard
        // zero here diverges (`harness_offline_seeded` shrinks to
        // `[Crash(Send), OfflineZReport, XReport]`: real 15000 != model 0).
        // The eventual close/reset is owned by the drain that transitions
        // `ClosingLocalPendingDrain → Closed`, not by the Z-report mint.

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
    ///
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
        if is_return
            && self.shift_is_open()
            && self.mode == NodeMode::Online
            && self.cash_on_hand() < CASH_AMOUNT_KOP
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
                // Server{-12} ERROR_BAD_HASH_PREV used to bail to `Fault` here on the
                // premise that the bounded MAC-recovery orchestrator fired "one auto
                // re-sign + retry" whose second wire the single-shot stub could not
                // serve.  CS-3 S7-1 R3 RETIRED that orchestrator: there is no second
                // wire.  The contract is now deterministic and pinned
                // (`invariant_fuzzer::minus_12_holds_the_node_and_rests_the_doc_sending`
                // + `write_path_stage4_send::minus_12_bad_hash_prev_records_held_stop_no_second_wire`):
                // the doc RESTS `Sending` under the hold, the node halts to STOP_MODE,
                // the seed does NOT advance and the recovery counter stays 0.  Every
                // one of those falls out of the shared HELD path below, so `-12` is an
                // ordinary predictable mutation — no fault-bucket exemption.
                let lnd = self.next_lnd;
                let previous_hash = self.seed;
                let unsigned_hash = synth_unsigned_hash(lnd);
                let doc_state = online_outcome_state(script);
                if doc_state == DocState::Sending {
                    // CS-3 S7-1: a HELD online outcome flips the node to STOP_MODE.
                    self.mode = NodeMode::StopMode;
                }
                self.mint_doc(lnd, doc_state, previous_hash); // the row IS minted (lnd allocated)
                self.next_lnd += 1;
                // L0 cash leg — recorded AT MINT, whatever state the doc landed in (bd
                // PRRO_GATE-6hl).  Prod stores the payment in `payload_json` when the row is
                // written and lets `counted_in_turnover` decide on every read; the model does
                // the same, so a doc that enters the counted set LATER (an operator completion
                // CAS'ing `Sending → Sent`) brings its leg with it, with no hook here.  The
                // leg of a doc that never counts (`Rejected`) is inert by the predicate.
                if is_return {
                    self.record_cash(lnd, -CASH_AMOUNT_KOP);
                } else {
                    self.record_cash(lnd, CASH_AMOUNT_KOP);
                }
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
                // L0 cash counts from the ISSUANCE boundary, not from ACK.
                //
                // bd PRRO_GATE-a6n resolved the question the old NOTE here parked as
                // "a policy question, follow-up, not blocking" — and resolved it
                // AGAINST the old behaviour.  Prod's turnover predicate is
                // `fiscal_documents::counted_in_turnover`, which for the online lane
                // counts from the `Sending → Sent` CAS (where A.3 stamps the sfn and
                // advances the seed), not from ACK.  The authority is the reference
                // PRRO our cash formula was ported from: WebCheck writes the receipt
                // into its journal — and thereby into turnover — the instant the DPS
                // submit returns a fiscal number (`StringXML.cs:1382`), with no
                // quittance wait and no delivery predicate anywhere in the aggregate
                // (`Reports.cs:50-84`).
                //
                // That boundary is applied by `Self::counted_in_turnover` on every read of
                // `cash_on_hand()` (it reuses the model's OWN lane SSOTs, so it cannot drift
                // from the seed predicate the same op used above) — the mint above only
                // RECORDS the leg (bd PRRO_GATE-6hl).
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
                    self.mint_doc(begin_lnd, DocState::OfflineLocalAck, self.seed);
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
                self.mint_doc(lnd, DocState::OfflineLocalAck, previous_hash);
                // A.3 PR-C — mark offline-origin: a later drain-Superseded may
                // park this lnd at ErrorRetryable, which stays ISSUED (offline)
                // and MUST NOT be read as a D5-gate blocker.
                self.offline_origin_lnds.insert(lnd);
                self.next_lnd += 1;
                self.codes_consumed += 1;
                self.seed = Some(unsigned_hash);
                // L0 cash-on-hand update: offline-origin doc is issued at OLA.
                if is_return {
                    self.record_cash(lnd, -CASH_AMOUNT_KOP);
                } else {
                    self.record_cash(lnd, CASH_AMOUNT_KOP);
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
                        // Cross-shift carry (offline twin of the online Z-close): the drain
                        // finalize is the REAL close that persists `cash_balance_kop`, so the
                        // next shift-open carries this drawer (symmetry with online).
                        self.carry_cash_kop = self.cash_on_hand();
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
                    self.mint_doc(end_lnd, DocState::Ack, end_prev);
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
                // CS-3 S7-1: a drain reject of an OFFLINE_LOCAL_ACK backlog doc is a recorded HOLD
                // — record_outcome commits PENDING_APPLY + STOP and the Sending→Rejected doc CAS is
                // deferred to APPLY (never runs), so the HEAD doc rests SENDING, NOT terminal Rejected.
                self.docs.insert(first, DocState::Sending);
                self.shift_state = ShiftState::RequiresManualReconciliation;
                // CS-3 S7-1: the offline-origin reject-hold flips the node to STOP_MODE
                // (record_outcome's `offline_reject_hold` early halt); seed unchanged; no 2nd wire
                // (S7-2 fence). The STOP is cleared only by operator completion.
                self.mode = NodeMode::StopMode;
                ExpectedOutcome::Mutated(Mutation {
                    lnd: first,
                    doc_state: DocState::Sending,
                    seed_after: self.seed,
                    previous_hash,
                    code_consumed: None,
                    shift_state_after: Some(self.shift_state),
                })
            }
            // U1 D5 — Superseded tip: the strict-sequential drain escalates to manual.
            // CS-3 S7-1: a Superseded (ServerFiscalIdMismatch → WrapperBug) after CALL_STARTED is a
            // recorded HOLD — the HEAD backlog doc rests SENDING under PENDING_APPLY (the doc CAS is
            // deferred to APPLY, never runs), NOT ERROR_RETRYABLE; the shift → RMR (EscalateManual,
            // M3b §16.7); mode stays GoingOnline; successors held at OFFLINE_LOCAL_ACK; the seed does
            // NOT re-advance (offline-origin advanced it at issuance).
            [WireResponse::Superseded, ..] => {
                let first = backlog[0];
                self.docs.insert(first, DocState::Sending);
                self.shift_state = ShiftState::RequiresManualReconciliation;
                // CS-3 S7-1: the WrapperBug HOLD flips the node to STOP_MODE (record_outcome halt).
                self.mode = NodeMode::StopMode;
                ExpectedOutcome::Mutated(Mutation {
                    lnd: first,
                    doc_state: DocState::Sending,
                    seed_after: self.seed,
                    previous_hash,
                    code_consumed: None,
                    shift_state_after: Some(self.shift_state),
                })
            }
            // U1 D5 — send Ack'd this tick, then last_chk NotFound: this is the
            // SentFresh confirm path → StructuralDrift, so the freshly-sent HEAD
            // doc is HELD at SENT (no CAS); shift unchanged, mode stays
            // GoingOnline, successors held, seed unchanged.  NB (CS-3 S7-1
            // F2-kvt2): a *cross-tick* SentReplay re-probe returning NotFound now
            // ESCALATES the doc to RMR + node STOP (kvt2_confirm
            // SentNotFoundEscalated); the current alphabet does not drive that
            // cross-tick SentReplay drain, so this single-tick arm is unaffected.
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
        let docs: Vec<(i64, DocState)> = sqlx::query_as::<_, (i64, prro::db::types::DbDocState)>(
            "SELECT lnd, state FROM fiscal_documents ORDER BY lnd",
        )
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|(lnd, s)| (lnd, s.0))
        .collect();
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

        // mode / shift_state / next_lnd ← node_state.
        let (mode, shift_state, next_lnd): (NodeMode, ShiftState, i64) =
            sqlx::query_as::<
                _,
                (
                    prro::db::types::DbNodeMode,
                    prro::db::types::DbShiftState,
                    i64,
                ),
            >("SELECT mode, shift_state, next_lnd FROM node_state LIMIT 1")
            .fetch_one(pool)
            .await
            .map(|(m, s, n)| (m.0, s.0, n))
            .unwrap();
        self.mode = mode;
        self.shift_state = shift_state;
        self.next_lnd = next_lnd;
        // STRUCTURAL seed: Some iff real Some (synthetic placeholder bytes) — read
        // through the tagged `read_seed_fixture` wrapper.  The synthetic per-lnd
        // placeholder must track the ACTUAL chain tip — the doc whose
        // `unsigned_xml_sha256` equals the real seed — NOT `max(lnd)`: a HELD /
        // non-issued `SENDING` doc (a BadHashPrev / UnknownStatus hold) can carry the
        // MAX lnd WITHOUT having advanced the seed, so `max(lnd)` would place the
        // placeholder on a doc the real chain tip has not reached.  When that held doc
        // is LATER issued (operator completion / drain), the real seed advances onto
        // its hash while the model already sat there → the op's real seed-advance
        // would spuriously read as "no model advance" (fuzzer online seed-advance
        // finding, task #18).
        //
        // bd `PRRO_GATE-01g` — the `max(lnd)` fallback RE-ARMED that exact trap.
        // The comment here used to end "fall back to max(lnd) only when the seed
        // matches no doc (e.g. a MacReseed rebase — generator-excluded)". That
        // premise was true when task #18 was written and STOPPED being true when the
        // generative `Replenish` symbol landed (bd hpc): a granted T=112 advances the
        // real seed to `sha256(request_xml)`, a NON-DOCUMENT value that matches no
        // row — so the lookup missed, the fallback aliased the placeholder onto
        // `max(lnd)` = the HELD doc, and the model was already sitting on the tip the
        // later completion would move to. Found generatively as
        // `[Replenish(Granted), SellWithClosedShift, OnlineShiftOpen([Superseded]),
        // Reboot, OperatorComplete(Accepted)]`; adjudicated PROD-RIGHT (prod's
        // completion-time advance is A.3 advance-at-SEND deferred until the
        // ambiguity resolves, and `invariant_scan` is clean at every boundary).
        //
        // A non-document real tip is one the model CANNOT compute (it builds no XML),
        // exactly like the `apply_replenish` advance that produced it — which is why
        // that advance is recorded structurally on a NEGATIVE ordinal. So the correct
        // adoption is to KEEP whatever structural marker the model already holds,
        // never to alias it onto a document ordinal.
        let real_seed = Self::read_seed_fixture(pool).await;
        let tip_lnd: Option<i64> = match &real_seed {
            Some(seed) => sqlx::query_scalar::<_, i64>(
                "SELECT lnd FROM fiscal_documents WHERE unsigned_xml_sha256 = ? \
                 ORDER BY lnd DESC LIMIT 1",
            )
            .bind(&seed[..])
            .fetch_optional(pool)
            .await
            .unwrap(),
            None => None,
        };
        match (&real_seed, tip_lnd) {
            // The real tip IS a document — adopt its ordinal (task #18's rule).
            (Some(_), Some(lnd)) => self.seed = Some(synth_unsigned_hash(lnd)),
            // Real tip present but matching no document: NON-DOCUMENT (a T=112
            // replenish seed today; a MacReseed rebase later). Keep the structural
            // marker rather than inventing a document ordinal for it.
            (Some(_), None) => {}
            // No real seed at all → genesis.
            (None, _) => self.seed = None,
        }

        // spec §8.1 — re-derive every document's chain link, in the model's symbolic algebra.
        //
        // Reality stores real hashes; the model stores ordinals, so each link is projected through
        // the SAME document lookup the seed adoption above uses.  A link that names a row becomes
        // that row's ordinal; a link that names no row is a non-document tip (a T=112 replenish
        // seed, a rebase) and gets the reserved non-document ordinal — the CLASS is what any
        // consumer can act on, and inventing a document ordinal for it would be the exact aliasing
        // trap bd `PRRO_GATE-01g` cost us on the seed itself.  NULL is genesis.
        //
        // Without this, a document minted inside a fault window carries no recorded link, and a
        // later `NotAcceptedOffline` rewind would silently fall back to genesis.
        let doc_links: Vec<(i64, Option<Vec<u8>>)> =
            sqlx::query_as("SELECT lnd, previous_hash FROM fiscal_documents")
                .fetch_all(pool)
                .await
                .unwrap();
        let issued_hashes: Vec<(Vec<u8>, i64)> = sqlx::query_as(
            "SELECT unsigned_xml_sha256, MAX(lnd) FROM fiscal_documents \
             WHERE unsigned_xml_sha256 IS NOT NULL GROUP BY unsigned_xml_sha256",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        let by_hash: BTreeMap<Vec<u8>, i64> = issued_hashes.into_iter().collect();
        self.doc_prev = doc_links
            .into_iter()
            .map(|(lnd, prev)| {
                let sym = prev.map(|p| match by_hash.get(&p) {
                    Some(owner) => synth_unsigned_hash(*owner),
                    None => synth_unsigned_hash(ADOPTED_NON_DOC_ORDINAL),
                });
                (lnd, sym)
            })
            .collect();

        // offline session + codes ← real.
        self.session = sqlx::query_scalar::<_, prro::db::types::DbOfflineSessionState>(
            "SELECT state FROM offline_sessions ORDER BY opened_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .unwrap()
        .map(|w| w.0);
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

        // L6 — the drawer ← the real ledger.  A crash-completed offline sell /
        // return / service-io issues a real cash-moving doc the model does NOT
        // predict across the crash window (it is a Fault op, adopted not
        // predicted); without this the model's drawer drifts from reality, and a
        // subsequent Op::XReport snapshot check would spuriously RED (real cash
        // reflects the crash-completed doc; model still at its pre-fault value).
        // Adopt reality's cash — same discipline as docs / codes / seed above —
        // via the prod SSOT `cash_on_hand_for_fn`.  For a NON-fault X-report the
        // model still DERIVES cash INDEPENDENTLY from its own per-doc legs, so the
        // snapshot check retains its teeth there.
        //
        // bd PRRO_GATE-6hl: adoption moves the OPENING ANCHOR, it does not overwrite an
        // accumulator — the per-doc legs the model already holds stay live, so a
        // post-adoption state edge (a doc leaving or entering the counted set) still moves
        // the drawer.  `docs` is adopted ABOVE, so the legs are re-classified against
        // reality's states before the anchor is solved for.
        let fiscal_number: String =
            sqlx::query_scalar("SELECT fiscal_number FROM node_state LIMIT 1")
                .fetch_one(pool)
                .await
                .unwrap();
        // bd PRRO_GATE-6hl — the legs' SHIFT SCOPE is adopted too, not just their states.
        //
        // The model normally scopes them itself: a shift-open clears `cash_by_lnd`, mirroring
        // prod's per-`shift_id` aggregates.  That self-scoping is exactly what a Fault window
        // can invalidate — reality may have closed one shift and opened another with no model
        // prediction in between, leaving a PRIOR shift's legs in the map to be re-counted, or
        // to move the drawer on a later state edge, in a shift they do not belong to.  Adopting
        // the scope from the ledger completes the funnel: `docs`, `offline_origin_lnds`,
        // `shift_lifecycle_lnds`, codes, seed and the session flags above are ALL re-derived
        // from reality here, and the legs' membership was the one thing that was not.
        //
        // The shift predicate is `cash_on_hand_for_fn`'s verbatim (`cash_ledger.rs`): the shift
        // `node_state.current_shift_id` points at, excluding the four states that mean "not an
        // open drawer".  No open shift ⇒ EMPTY scope ⇒ every leg drops and the anchor absorbs
        // reality's 0 — which is what that read returns.  On every trajectory the current
        // alphabet can produce this is a NO-OP (Crash / Reboot open no shift, and there is no
        // auto-Z symbol), so it hardens a case rather than changing a reachable one.
        let in_scope: Vec<(i64,)> = sqlx::query_as(
            "SELECT fd.lnd FROM fiscal_documents fd \
             JOIN shifts s ON s.shift_id = fd.shift_id \
             JOIN node_state ns ON ns.current_shift_id = s.shift_id \
             WHERE ns.fiscal_number = ? \
               AND s.state NOT IN ('CLOSED','REQUIRES_MANUAL_RECONCILIATION','ERROR','CREATED')",
        )
        .bind(&fiscal_number)
        .fetch_all(pool)
        .await
        .unwrap();
        let in_scope: BTreeSet<i64> = in_scope.into_iter().map(|(lnd,)| lnd).collect();
        self.cash_by_lnd.retain(|lnd, _| in_scope.contains(lnd));
        let real_cash = prro::services::cash_ledger::cash_on_hand_for_fn(pool, &fiscal_number)
            .await
            .unwrap();
        self.adopt_cash(real_cash);
        // held_reservation is synced by the harness after every op (`sync_held_reservation`), so it
        // is NOT re-derived here — adopt covers docs/mode/session/seed/cash only.
    }

    /// CS-3 operator-completion (1b) — RE-SYNC the reality-tracked hold PRECONDITION from the real
    /// `delivery_reservation` table: `Some((held_lnd, online_origin))` for the FN's `PENDING_APPLY`
    /// reservation (the fence enforces ≤1), else `None`.  `online_origin` mirrors prod's completion
    /// origin cross-check (`delivery_reservation.rs:1343` — `offline_fiscal_no IS NULL`).  The harness
    /// calls this after EVERY op so `held_reservation` reflects reality entering the next op (a drain
    /// can create a hold the pure model does not predict); the release OUTCOME stays independent.
    pub async fn sync_held_reservation(&mut self, pool: &SqlitePool) {
        self.held_reservation =
            sqlx::query_as::<_, (i64, Option<i64>, prro::db::types::DbDocType)>(
                "SELECT fd.lnd, fd.offline_fiscal_no, fd.doc_type FROM delivery_reservation dr \
             JOIN fiscal_documents fd \
               ON fd.document_id = dr.document_id AND fd.fiscal_number = dr.fiscal_number \
             WHERE dr.apply_state = 'PENDING_APPLY' LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .unwrap()
            .map(|(lnd, offline_fiscal_no, doc_type)| {
                (lnd, offline_fiscal_no.is_none(), doc_type.0)
            });
    }

    /// CS-3 S7-2 — RE-SYNC `fence_active` from the real `delivery_reservation` table, using
    /// **production's own** `ACTIVE_FENCE_STATE_PREDICATE` const rather than a hand-copied predicate.
    ///
    /// Reusing the const is deliberate: a duplicated predicate here could silently drift from the one
    /// `fn_fence_active_tx` actually enforces, and the model would then mispredict every fenced op
    /// forever. The const itself is not trusted blindly — `fence_predicate_byte_identity`
    /// (`tests/fn_fence_active.rs`) pins it byte-for-byte against the migration 035 index + trigger,
    /// so drift fails CI there, at the right layer.
    ///
    /// Not FN-scoped: the fuzzer fixture is single-FN by construction, and the partial-unique index
    /// guarantees at most one active row per FN anyway.
    pub async fn sync_fence_active(&mut self, pool: &SqlitePool) {
        let pred = prro::db::repositories::delivery_reservation::ACTIVE_FENCE_STATE_PREDICATE;
        let active: i64 = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM delivery_reservation WHERE {pred})"
        ))
        .fetch_one(pool)
        .await
        .unwrap();
        self.fence_active = active == 1;
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
            sqlx::query_as::<_, (prro::db::types::DbNodeMode, prro::db::types::DbShiftState)>(
                "SELECT mode, shift_state FROM node_state LIMIT 1",
            )
            .fetch_one(pool)
            .await
            .map(|(m, s)| (m.0, s.0))
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
        let active_states: Vec<OfflineSessionState> =
            sqlx::query_scalar::<_, prro::db::types::DbOfflineSessionState>(
                "SELECT state FROM offline_sessions WHERE state IN ('OPEN', 'DRAINING') \
                 ORDER BY opened_at DESC, offline_session_id",
            )
            .fetch_all(pool)
            .await
            .unwrap()
            .into_iter()
            .map(|w| w.0)
            .collect();
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

/// Peer-tip axis PHASE C (spec §8.1) — the reserved ordinal a chain link adopted from the ledger
/// gets when it names NO document (a T=112 replenish seed, a rebase).
///
/// Negative, so [`model_tip_class`] reads it as `NonDoc`; `i64::MIN` specifically, so it can never
/// collide with the `-codes_issued - 1` ordinals `apply_replenish` mints for tips the model DID
/// predict.  The model cannot recover WHICH non-document value reality holds — it builds no XML —
/// and must not pretend otherwise; the class is the honest ceiling.
const ADOPTED_NON_DOC_ORDINAL: i64 = i64::MIN;

/// Peer-tip axis PHASE C (spec §8) — the STRUCTURAL class of a model-symbolic MAC tip.
///
/// The model never holds a real hash (it builds no XML); it holds `synth_unsigned_hash(ordinal)`
/// placeholders and, from phase C on, opaque constants copied verbatim from the wire vocabulary.
/// Decoding is therefore TOTAL and injective by construction: a placeholder carries its ordinal
/// big-endian in the leading 8 bytes with the remaining 24 zero, so a NON-NEGATIVE ordinal names a
/// document, a NEGATIVE one names the non-document T=112 seed (`apply_replenish`), and a value that
/// is not a placeholder at all is some other non-document tip.
///
/// This is what makes the model's tip ASSERTABLE rather than merely conventional.  Before phase C
/// the differential only ever asked "did the seed CHANGE"; `run_harness` now also asks "does the
/// model's tip name the same thing reality's names", against
/// [`crate::interp::FuzzCtx::real_tip_class`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTipClass {
    /// The model holds no tip — genesis.
    Genesis,
    /// The model's tip is the document minted at this ordinal.
    Doc(i64),
    /// The model's tip is a NON-document value (the T=112 replenish seed, or an opaque
    /// peer-declared constant).
    NonDoc,
}

/// Decode a model-symbolic tip into its [`ModelTipClass`].  See that type for why this is total.
pub fn model_tip_class(seed: Option<[u8; 32]>) -> ModelTipClass {
    let Some(sym) = seed else {
        return ModelTipClass::Genesis;
    };
    // Not a `synth_unsigned_hash` placeholder at all (the trailing 24 bytes are zero for every
    // ordinal) → an opaque wire value, which by construction names no document.
    if sym[8..].iter().any(|b| *b != 0) {
        return ModelTipClass::NonDoc;
    }
    let ordinal = i64::from_be_bytes(sym[..8].try_into().expect("8 bytes of a 32-byte symbol"));
    if ordinal >= 0 {
        ModelTipClass::Doc(ordinal)
    } else {
        ModelTipClass::NonDoc
    }
}

/// CS-3 operator-completion (1b) — map the `ReleasedWitness` DB-text `node_mode` back to the model's
/// `NodeMode` so a release un-halts the model consistently with the DB.  Never `StopMode`.
fn node_mode_from_str(mode: &str) -> NodeMode {
    match mode {
        "ONLINE" => NodeMode::Online,
        "GOING_ONLINE" => NodeMode::GoingOnline,
        "BLOCKED" => NodeMode::Blocked,
        other => panic!("released_witness produced an unexpected node_mode {other:?}"),
    }
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
        // CS-3 S7-1: every other (ambiguous / transient / Superseded / escalate) online outcome
        // after CALL_STARTED is a recorded HOLD — the doc rests SENDING under a PENDING_APPLY
        // reservation (node → STOP_MODE), NOT auto-retryable ErrorRetryable. The doc-target CAS is
        // deferred to the APPLY orchestration and never runs on a held outcome.
        _ => DocState::Sending,
    }
}

/// The INDEPENDENT model-side mirror of the durable `delivery_reservation` witness that a HELD
/// online outcome persists — the CS-3 Slice E delivery-axis contract, as DB-TEXT literals.
///
/// These strings are hardcoded from the SPEC (the Slice E directed pin
/// `directed_unknown_status_probe.rs` + migration 038), **NOT** read from production `classify()` /
/// `routing_for_indeterminate()` / the production routing table.  That independence is the whole
/// point: if a prod regression flips the persisted `routing_class` back to `TransientRetry`, the
/// oracle sees the model still predicting `ProbeRequired` and REDs — a real finding to adjudicate,
/// never silently tuned to match prod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldWitness {
    pub submission_certainty: &'static str,
    pub response_provenance: &'static str,
    pub routing_class: &'static str,
    pub node_effect: &'static str,
    pub evidence_kind: &'static str,
    pub evidence_code: Option<i64>,
    pub apply_state: &'static str,
    pub node_mode: &'static str,
    pub fence_held: bool,
}

/// The independent delivery-axis prediction for a HELD online wire outcome, keyed on the wire
/// `script`'s leaf.  Returns `Some(HeldWitness)` for the leaves whose held-reservation contract this
/// model encodes, `None` otherwise (the oracle then skips the axis-check for that leaf — a DOCUMENTED
/// coverage boundary, not a silent pass).
///
/// Increment 1 covers the **`UnknownStatus`** leaf (the Slice E `ProbeRequired` surface — a parsed
/// DPS envelope with an unnamed non-zero status).  Other held leaves (`Superseded` →
/// `ServerFiscalIdMismatch`/`WrapperBug`, `BadHashPrev`) are deliberately `None` here pending a
/// follow-up increment that encodes their (distinct) axis tuple; leaving them `None` is honest —
/// the model does not yet claim their contract, so it must not assert it.
pub fn online_held_witness(script: &DpsScript) -> Option<HeldWitness> {
    match script.0.as_slice() {
        // A parsed envelope carrying an unnamed non-zero status → SubmittedUnknown / ProbeRequired
        // HELD: the doc rests SENDING, the reservation stays PENDING_APPLY (apply did NOT
        // auto-release), the node halts STOP_MODE, and the FN fence pointer is still SET.  The raw
        // status `code` is preserved verbatim in `evidence_code`.
        [WireResponse::UnknownStatus(code), ..] => Some(HeldWitness {
            submission_certainty: "SUBMITTED_UNKNOWN",
            response_provenance: "PARSED_DPS_ENVELOPE",
            routing_class: "ProbeRequired",
            node_effect: "ProbeRequired",
            evidence_kind: "UnknownStatus",
            evidence_code: Some(*code as i64),
            apply_state: "PENDING_APPLY",
            node_mode: "STOP_MODE",
            fence_held: true,
        }),
        // Superseded (server fiscal-id mismatch → `DpsError::ServerFiscalIdMismatch`): the
        // DURABLE persisted class is `TransientRetry` / `NoNodeEffect`.  TWO prod classifiers
        // disagree on this input, and the model deliberately encodes the one that OWNS the
        // persisted reservation: the modern `classify_started` maps the NoResponse cause
        // (`CallFailedWithoutTrustedDpsEnvelope`) to `TransientRetry` (delivery/mod.rs) and is the
        // sole writer of `record_outcome`; the legacy `error_routing`'s `WrapperBug`/`ErrorRetryable`
        // (error_routing.rs:413) is a DIAGNOSTIC OVERLAY that feeds only the return value / audit,
        // NEVER the durable reservation.  Encoding the overlay would be the classic green-but-unsound
        // trap.  It still HOLDS: SENDING under PENDING_APPLY + STOP_MODE + fence.  `node_effect ==
        // NoNodeEffect` while `node_mode == STOP_MODE` is REAL prod — the halt comes from the HELD
        // record, not the classifier's node-effect axis.
        [WireResponse::Superseded, ..] => Some(HeldWitness {
            submission_certainty: "SUBMITTED_UNKNOWN",
            response_provenance: "NO_RESPONSE",
            routing_class: "TransientRetry",
            node_effect: "NoNodeEffect",
            evidence_kind: "NoResponse",
            evidence_code: None,
            apply_state: "PENDING_APPLY",
            node_mode: "STOP_MODE",
            fence_held: true,
        }),
        // BadHashPrev (a bad previous-hash chain link): a PARSED DPS reject routed to the MAC-recovery
        // class — `MacRecovery` / `MacReseedPending` (the operator supplies a corrected seed, `-12`).
        // Held: SENDING under PENDING_APPLY + STOP_MODE + fence. `evidence_code` is a verdict/digest,
        // not a raw integer → None.
        [WireResponse::BadHashPrev, ..] => Some(HeldWitness {
            submission_certainty: "SUBMITTED",
            response_provenance: "PARSED_DPS_ENVELOPE",
            routing_class: "MacRecovery",
            node_effect: "MacReseedPending",
            evidence_kind: "Rejected",
            evidence_code: None,
            apply_state: "PENDING_APPLY",
            node_mode: "STOP_MODE",
            fence_held: true,
        }),
        _ => None,
    }
}

/// The ORIGIN-KEYED held-witness prediction for a HELD wire outcome observed on the **OFFLINE
/// drain** lane (`backlog_drain::drain` re-driving an offline-issued backlog doc), keyed on the wire
/// `script`'s leaf.  The offline lane runs the SAME production delivery classifier as an online send
/// (same `submission_certainty` / `response_provenance` / `routing_class` / `node_effect` /
/// `evidence_*` axes), so for every leaf EXCEPT `[Reject]` the durable held tuple is identical to
/// [`online_held_witness`] and this delegates to it.  The **`[Reject]` leaf is the one genuine
/// origin divergence** — hence a separate function rather than a shared one.
///
/// **`[Reject]` origin divergence (empirically grounded, probe 2026-07-24):**
///   - ONLINE `[Reject]`: the reject is APPLIED at SEND → the doc rests `REJECTED` (a non-issued
///     row, D2 pin — the seed is NOT advanced), the reservation is `APPLIED`, the fence is CLEAR,
///     the node stays `ONLINE`.  Nothing is held → [`online_held_witness`] correctly returns `None`.
///   - OFFLINE-drain `[Reject]`: the backlog doc has already crossed the local-commit threshold
///     (it rests `OFFLINE_LOCAL_ACK`), so a reject can NOT roll it back to `REJECTED`.  The drain
///     instead HOLDS it: the doc rests `SENDING` under a `PENDING_APPLY` reservation, the node halts
///     `STOP_MODE`, the FN fence pointer is SET, and the shift escalates to
///     `RequiresManualReconciliation` (the confirmed primary W9b manual-recon surface — CLAUDE.md
///     §16.7 trigger family 1, edges 6/14).  The classifier axes are byte-identical to the online
///     reject (`SUBMITTED` / `PARSED_DPS_ENVELOPE` / `TerminalReject` / `Rejected`); the divergence
///     is purely in the APPLY decision — `apply_state` (`PENDING_APPLY` vs `APPLIED`), `node_mode`
///     (`STOP_MODE` vs `ONLINE`), and `fence_held` (`true` vs `false`).
///
/// These strings are hardcoded from the empirical probe + the CLAUDE.md persistence contract, NOT
/// read from production `classify()` / the routing table — the same independence discipline as
/// [`online_held_witness`].
///
/// `[Ack, NotFound]` is NOT listed: the probe confirmed it RELEASES at SEND on BOTH origins
/// (`APPLIED` reservation, doc `SENT`, fence clear — the send Ack is the issuance moment, the
/// `NotFound` lastChk merely defers the KVT quittance), so it correctly delegates to
/// [`online_held_witness`]'s `None` — a VERIFIED non-divergence, not a coverage gap.
pub fn offline_held_witness(script: &DpsScript) -> Option<HeldWitness> {
    match script.0.as_slice() {
        [WireResponse::Reject, ..] => Some(HeldWitness {
            submission_certainty: "SUBMITTED",
            response_provenance: "PARSED_DPS_ENVELOPE",
            routing_class: "TerminalReject",
            node_effect: "NoNodeEffect",
            evidence_kind: "Rejected",
            evidence_code: None,
            apply_state: "PENDING_APPLY",
            node_mode: "STOP_MODE",
            fence_held: true,
        }),
        // Every other leaf holds (or releases) identically on the offline lane.
        _ => online_held_witness(script),
    }
}

/// The INDEPENDENT model-side mirror of the durable witness AFTER a legal operator completion
/// releases a HELD reservation — the CS-3 eternal-BRICK contract, as DB-TEXT literals.  Encoded
/// from the SPEC (the completion effect matrix in `delivery_reservation.rs` completion +
/// `resolve_operator_pending`), NOT read from production.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasedWitness {
    pub apply_state: &'static str,
    pub node_mode: &'static str,
    pub fence_held: bool,
    pub doc_state: &'static str,
}

/// The independent prediction of a legal operator completion — `Some(ReleasedWitness)` if the
/// resolution RELEASES the hold, `None` if prod REFUSES it (an origin cross-check contradiction —
/// the model predicts a `Refused` outcome shape, NOT a release).  Derived PURELY from the resolution,
/// the doc's ORIGIN, and the model's OWN tracked state (active offline session, origin-blocked hold),
/// NOT from any production call.  The load-bearing anti-BRICK invariant is baked into the release
/// branch: EVERY legal completion yields `apply_state == "APPLIED"`, `fence_held == false`, and a
/// `node_mode` that is NEVER `"STOP_MODE"`.
///
/// Contract (SPEC, `delivery_reservation.rs` completion):
///   - Origin cross-check (1351-1359) — REFUSED (⇒ `None`) BEFORE any mutation: `NotAccepted` on an
///     OFFLINE-origin doc, `NotAcceptedOffline` on an ONLINE-origin doc, `MacReseed` on an
///     OFFLINE-origin doc (a seed rewind on the wrong origin would fork a live chain).
///   - Release branch (1367-1512): `apply_state` = "APPLIED"; `fence_held` = false (the sole
///     pointer-clear branch); `node_mode` = "BLOCKED" iff origin-blocked (offline `-11`, INV-05) /
///     else "GOING_ONLINE" iff an active offline session must drain / else "ONLINE"; `doc_state` =
///     "SENT" for `Accepted` (issued) / "REQUIRES_MANUAL_RECONCILIATION" otherwise.
pub fn released_witness(
    resolution: OperatorResolutionKind,
    online_origin: bool,
    has_active_offline_session: bool,
    origin_blocked: bool,
) -> Option<ReleasedWitness> {
    // Origin cross-check (delivery_reservation.rs:1351-1359): a resolution whose origin contradicts
    // the doc's origin is fail-closed BEFORE any mutation — the model predicts Refused (None).
    let refused = match resolution {
        OperatorResolutionKind::NotAccepted => !online_origin, // ONLINE origin only
        OperatorResolutionKind::NotAcceptedOffline => online_origin, // OFFLINE origin only
        OperatorResolutionKind::MacReseed => !online_origin,   // ONLINE origin only
        OperatorResolutionKind::Accepted => false,             // valid on both origins
    };
    if refused {
        return None;
    }
    let node_mode = if origin_blocked {
        "BLOCKED"
    } else if has_active_offline_session {
        "GOING_ONLINE"
    } else {
        "ONLINE"
    };
    let doc_state = match resolution {
        OperatorResolutionKind::Accepted => "SENT",
        OperatorResolutionKind::NotAccepted
        | OperatorResolutionKind::NotAcceptedOffline
        | OperatorResolutionKind::MacReseed => "REQUIRES_MANUAL_RECONCILIATION",
    };
    Some(ReleasedWitness {
        apply_state: "APPLIED",
        node_mode,
        fence_held: false,
        doc_state,
    })
}

/// bd `PRRO_GATE-k3y` — the operator shift-state edge a completion applies to a SHIFT-FAMILY held
/// document, per design §3.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftEdge {
    /// No shift edge: a non-shift-family document, an OFFLINE **Accepted** shift-family document
    /// (the drain finalize owns the forward edge, §4.1 edges 5/13), or an origin-mismatched
    /// resolution (the core's origin cross-check is the authority and fails it closed anyway).
    NoEdge,
    /// Apply `from -> to`, CAS-guarded on `from`. A mismatch is a projection failure and the WHOLE
    /// completion rolls back.
    Move(ShiftState, ShiftState),
    /// `MacReseed` on a shift-family document: §3.4 has no such cell — a `-12` chain-seed correction
    /// is not defined for SHIFT_OPEN / SHIFT_CLOSE / Z_REPORT. Fail-closed before any mutation.
    Refused,
}

/// bd `PRRO_GATE-k3y` — INDEPENDENT re-derivation of the §3.4 shift-projection matrix that
/// `operator_completion::complete_operator_resolution` applies.
///
/// Deliberately NOT importing production's `shift_projection`: this value is compared against
/// reality by the U1 D2 `shift_state` differential, so importing the production matrix would make
/// that assertion tautological and the oracle would lose the ability to catch a WRONG prod matrix.
/// It is written from the spec, exactly like [`released_witness`] and
/// [`macreseed_completion_releases`], whose directed teeth prove the real seam agrees.
///
/// Every cell is listed, including ones the current alphabet may not reach. That is the bd
/// `PRRO_GATE-01g` lesson: an "unreachable, generator-excluded" note is written about the alphabet
/// of its day and silently rots when a symbol is added.
pub fn operator_shift_edge(
    doc_type: DocType,
    online: bool,
    resolution: OperatorResolutionKind,
) -> ShiftEdge {
    use DocType::{ShiftClose, ShiftOpen, ZReport};
    use OperatorResolutionKind as R;
    use ShiftState::{
        Closed, Closing, ClosingLocalPendingDrain, Opened, OpenedLocalPendingDrain, Opening,
    };
    match (doc_type, online, resolution) {
        // ── online shift-family ──
        (ShiftOpen, true, R::Accepted) => ShiftEdge::Move(Opening, Opened),
        (ShiftOpen, true, R::NotAccepted) => ShiftEdge::Move(Opening, Closed),
        (ShiftClose | ZReport, true, R::Accepted) => ShiftEdge::Move(Closing, Closed),
        (ShiftClose | ZReport, true, R::NotAccepted) => ShiftEdge::Move(Closing, Opened),
        // ── offline shift-family NOT-accepted rollback ──
        (ShiftOpen, false, R::NotAcceptedOffline) => {
            ShiftEdge::Move(OpenedLocalPendingDrain, Closed)
        }
        (ShiftClose | ZReport, false, R::NotAcceptedOffline) => {
            ShiftEdge::Move(ClosingLocalPendingDrain, OpenedLocalPendingDrain)
        }
        // Offline shift-family Accepted: the drain finalize owns the forward edge, not the operator.
        (ShiftOpen | ShiftClose | ZReport, false, R::Accepted) => ShiftEdge::NoEdge,
        // MacReseed on any shift-family document has no §3.4 cell.
        (ShiftOpen | ShiftClose | ZReport, _, R::MacReseed) => ShiftEdge::Refused,
        // Non-shift-family, or an origin-mismatched resolution.
        _ => ShiftEdge::NoEdge,
    }
}

/// CS-3 MacReseed (task #18 (B)) — the INDEPENDENT model prediction of whether a `-12` MacReseed
/// completion RELEASES the hold, composing the THREE prod fail-closed gates in
/// `complete_operator_pending` (`delivery_reservation.rs`): the ORIGIN cross-check (MacReseed is
/// ONLINE-origin only, :1378 `MacReseedNotOfflineDefined`), guard A (the hold's `node_effect` must be
/// `MacReseedPending`, :1393 `MacReseedHoldMismatch`), and guard B (the operator seed must equal the
/// last-issued chain tip, :1408 `MacReseedSeedMismatch`).  Encoded from the SPEC, NOT read from prod —
/// the `macreseed_*` directed teeth prove the REAL seam agrees. All three must hold to release; any one
/// failing is a fail-closed refusal BEFORE any mutation (hold intact, seed unchanged).
pub fn macreseed_completion_releases(
    online_origin: bool,
    hold_is_macreseed_pending: bool,
    seed_matches_tip: bool,
) -> bool {
    online_origin && hold_is_macreseed_pending && seed_matches_tip
}
