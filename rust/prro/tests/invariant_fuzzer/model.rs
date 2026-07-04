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

use std::collections::BTreeMap;

use sqlx::SqlitePool;

use prro::db::models::enums::{DocState, NodeMode, OfflineSessionState, ShiftState};
use crate::op::{DpsScript, Op, WireResponse};

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

    /// The model's offline-origin "issued" set — the model-local FORK
    /// `MODEL_OFFLINE_ISSUED_STATES`, guarded to equal the prod SSOT const (U1 D3).
    pub fn offline_issued_states() -> &'static [&'static str] {
        &MODEL_OFFLINE_ISSUED_STATES[..]
    }

    /// Offline-origin "issued" membership, from the model-local fork (U1 D3).
    pub fn is_offline_origin_issued(state: DocState) -> bool {
        MODEL_OFFLINE_ISSUED_STATES.contains(&state.as_str())
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
            Op::OnlineSell(script) => self.apply_sell(script),
            Op::OfflineSell => self.apply_sell(&DpsScript(Vec::new())),
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
        self.apply_sell(&DpsScript(Vec::new()))
    }

    /// A sell — the lane is the NODE MODE (the interpreter's `inline::run`
    /// dispatches by mode), not the op name.  Online → per-script outcome;
    /// Offline → OFFLINE_LOCAL_ACK (consuming a code); any other mode → refused.
    fn apply_sell(&mut self, script: &DpsScript) -> ExpectedOutcome {
        if !self.shift_is_open() {
            return ExpectedOutcome::NoMutation;
        }
        match self.mode {
            NodeMode::Online => {
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
                if doc_state == DocState::Ack {
                    self.seed = Some(unsigned_hash); // online-origin issues at ACK
                }
                // A DPS document-reject CAS's the row Sending→Rejected but
                // `inline::run` returns Err(DpsRejected) → the interpreter reports
                // Refused.  The row is a NON-ISSUED artifact (no seed advance), so
                // the model reports NoMutation (the lnd was still consumed, so
                // next_lnd / docs stay in sync with reality).
                if doc_state == DocState::Rejected {
                    return ExpectedOutcome::NoIssuanceRow;
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
                })
            }
            NodeMode::Offline => {
                let code_available = self.codes_consumed < self.codes_issued;
                if self.session != Some(OfflineSessionState::Open) || !code_available {
                    // No active session / no code: reality reaches SIGNED, then the
                    // offline-ack refuses (NoActiveSession / CodePoolExhausted,
                    // post-sign) → the seam aborts it → a non-issued Aborted row.
                    return self.mint_aborted_refusal();
                }
                let lnd = self.next_lnd;
                let previous_hash = self.seed;
                let unsigned_hash = synth_unsigned_hash(lnd);
                // Offline issuance → OFFLINE_LOCAL_ACK, advances the seed THERE
                // (spec §6 offline lane); stays issued through later drain states.
                self.docs.insert(lnd, DocState::OfflineLocalAck);
                self.next_lnd += 1;
                self.codes_consumed += 1;
                self.seed = Some(unsigned_hash);
                ExpectedOutcome::Mutated(Mutation {
                    lnd,
                    doc_state: DocState::OfflineLocalAck,
                    seed_after: self.seed,
                    previous_hash,
                    // Cumulative observable (this op's consume is now counted).
                    code_consumed: self.code_consumed_observable(),
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
                self.mode = NodeMode::Online; // full drain → GoingOnline → Online
                let tip = *backlog.last().expect("backlog is non-empty");
                ExpectedOutcome::Mutated(Mutation {
                    lnd: tip,
                    doc_state: DocState::Ack,
                    seed_after: self.seed,
                    previous_hash,
                    code_consumed: None,
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
    pub async fn resync_from_db(&mut self, pool: &SqlitePool) {
        // docs ← the real ledger (lnd → state).
        let docs: Vec<(i64, DocState)> =
            sqlx::query_as("SELECT lnd, state FROM fiscal_documents ORDER BY lnd")
                .fetch_all(pool)
                .await
                .unwrap();
        self.docs = docs.into_iter().collect();
        let tip_lnd = self.docs.keys().copied().max().unwrap_or(0);

        // mode / shift_state / next_lnd / seed-presence ← node_state.
        let (mode, shift_state, next_lnd, real_seed): (NodeMode, ShiftState, i64, Option<Vec<u8>>) =
            sqlx::query_as(
                "SELECT mode, shift_state, next_lnd, last_known_unsigned_xml_sha256 \
                 FROM node_state LIMIT 1",
            )
            .fetch_one(pool)
            .await
            .unwrap();
        self.mode = mode;
        self.shift_state = shift_state;
        self.next_lnd = next_lnd;
        // STRUCTURAL seed: Some iff real Some (synthetic placeholder bytes).
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
    }

    /// Adopt ONLY the precondition state (mode / shift_state / active session)
    /// from the real DB after a NON-fault op — keeping the PREDICTED ledger
    /// (docs / seed / next_lnd / codes) for the differential.  Transition seams
    /// (go_online / drain / the force-ops) set mode/shift/session in ways the
    /// pure model need not perfectly predict; the LEDGER is what the differential
    /// checks, and the mode/shift mirror integrity is checked by the scan.  This
    /// keeps the NEXT op dispatching from the real precondition state.
    pub async fn resync_preconditions_from_db(&mut self, pool: &SqlitePool) {
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
