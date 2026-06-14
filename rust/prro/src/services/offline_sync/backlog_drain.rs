//! W9b backlog drain orchestration — types, API-level invariants,
//! orchestrator skeleton, and per-doc loop (Commits 2-4).
//!
//! This module is shipped in stages:
//!
//! - **Commit 2** — pure types: [`W12ConfirmOutcome`] typed seam,
//!   [`DrainSummary`] with private counters + invariant-enforcing
//!   API, [`FinalizeEligibility`] decision enum, [`failure_class_for`]
//!   stable-string taxonomy.
//! - **Commit 3** — orchestrator skeleton: pure-function entry that
//!   reads prerequisites (mode, backlog, offline session state),
//!   transitions session Open → Draining via inline `with_immediate`
//!   envelope, emits `OFFLINE_DRAIN_STARTED` audit.
//! - **Commit 4 (this file, [`drain`])** — per-doc loop: iterates the
//!   backlog in strict `lnd ASC` order, invokes `stage_send::run`,
//!   then routes Sent outcome through the W12 chain (see W12
//!   subsection below).  Audits per-doc `OFFLINE_DRAIN_KVT2_ADVANCED`
//!   (Envelope 1a) + `STAGE_FINALIZE_ACK` (Envelope 2) on Acked path;
//!   `OFFLINE_DRAIN_DOC_FAILED` on stage_send failures.  Routes
//!   manual-recon-class failures on pending-drain shifts to
//!   `RequiresManualReconciliation` and halts the drain (per spec
//!   amendment 2026-05-21 and `LEGAL_INVARIANTS.md` §INV-19).
//!   Sibling-continue applies ONLY to per-doc failures on
//!   non-pending-drain shifts.
//! - **Commits 5-7** — widen walker to the unfinished cohort
//!   (`OFFLINE_LOCAL_ACK | SENT | KVT1 | ERROR_RETRYABLE | KVT2`
//!   post W12 Commit 3), add lastChk pre-flight, add the
//!   finalization branch, and add the App entry.
//!
//! ## M3b W12 wiring (2026-05-22)
//!
//! - **W12 Commit 3** widens drain cohort to include `KVT2` (post-
//!   crash advance via `stage_finalize::run` per
//!   `process_via_w12_kvt2_advance`).
//! - **W12 Commit 4b** wires `process_via_stage_send` Sent branch to
//!   `kvt2_confirm::confirm_drain_doc(SentFresh, ...)` —
//!   Envelope 1a (Kvt1Raw + Sent→Kvt1 + Kvt1→Kvt2 + KVT2_ADVANCED
//!   audit) + Envelope 2 (`stage_finalize::run` Kvt2→Ack).
//!   Non-Acked outcomes emit `KVT2_CONFIRM_HOLD` (Warning) or
//!   `KVT2_CONFIRM_STRUCTURAL_DRIFT` (Error) audit-only envelope
//!   before `BootError::Internal` halt per plan §311 +
//!   MED-PR70-R12-01.
//! - **W12 Commit 5** wires Kvt1Reentry source via
//!   `process_via_w12_only` → `kvt2_confirm::confirm_drain_doc(
//!   Kvt1Reentry, ...)` → Envelope 1b (Kvt1Raw + Kvt1→Kvt2 +
//!   KVT2_ADVANCED audit; NO Sent→Kvt1 since doc is already at
//!   Kvt1) + Envelope 2 (`stage_finalize::run` Kvt2→Ack).  Caller
//!   sources `expected_server_fiscal_no` from persisted
//!   `doc.server_fiscal_no` with caller-level
//!   `BootError::Internal` + KVT2_CONFIRM_STRUCTURAL_DRIFT audit
//!   on None (state-machine invariant breach per MED-W12C5-01).
//! - **W12 Commits 5b/6 (pending)** — SentReplay source wiring
//!   with `transport_trace` recovery row, HoldFnDrain projection
//!   (replaces current Hold-path `BootError::Internal` defensive
//!   marker with `DocVerdict::HoldFnDrain { HeldAtSent |
//!   HeldAtKvt1 }` per plan §413).
//!
//! ## C4 known gaps (C5 blocker before "C4 approved")
//!
//! - **SENT rediscovery on restart**: C4 scans only `OFFLINE_LOCAL_
//!   ACK` (see C1 helper `list_offline_local_ack_for_fn_ordered_by_
//!   lnd`).  A crashed-mid-drain SENT or KVT1 doc is NOT re-discovered
//!   by W9b drain on next tick — the M3a `boot_phase` reconciliation
//!   handles SENT → Kvt1 advance pre-W12, but spec §6 I4 mandates
//!   that drain itself owns the rediscovery path.
//! - **TransientRetry-stranded pending-drain shifts (HIGH-C4-8,
//!   2026-05-21)**: a doc that hits `RetryClass::TransientRetry`
//!   during C4 drain moves OFFLINE_LOCAL_ACK → Sending →
//!   ErrorRetryable (via stage_send Pattern B routing).  C4 sibling-
//!   continue is correct (non-manual-recon), but the doc exits the
//!   C4 OFFLINE_LOCAL_ACK-only scan while the shift remains in
//!   `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`.  Next
//!   drain tick will not rediscover the doc, leaving the shift
//!   stranded.
//!
//! **C5 closes both gaps** by widening the walker to
//! `state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE')`
//! (KVT2 deferred to W12 PR per MED-C5-4) and dispatching by
//! `doc.state` (ErrorRetryable → stage_send re-drive via W9a 4-pre
//! source whitelist).  C5 is a blocker before any "C4 approved"
//! verdict at the PR level.
//!
//! ## Pre-W12 invariant pin (operator-flagged 2026-05-20, sign-off pin)
//!
//! W9b alone (before W12 PR plugs in lastChk → Kvt1 → Kvt2 → Ack
//! confirmation) MUST NEVER finalize the drain.  Concretely:
//!
//! 1. [`W12ConfirmOutcome::DeferredKvt1`] CANNOT be routed into
//!    [`DrainSummary::advanced_to_ack`] — routing is centralized in
//!    [`DrainSummary::record_doc_advanced`] which matches the variant
//!    and dispatches.  No public field setter exists.
//! 2. [`DrainSummary::finalize_eligibility`] returns
//!    [`FinalizeEligibility::NotEligible`] with
//!    [`NotEligibleReason::DocsDeferredAtKvt1`] if ANY doc was
//!    `DeferredKvt1` (`advanced_to_kvt1 > 0` blocks finalize).
//! 3. [`DrainSummary::mark_finalized`] errors with
//!    [`FinalizeError::NotEligible`] if eligibility check fails —
//!    cannot accidentally set `finalized = true` outside the typed
//!    contract.
//! 4. Pre-W12 audit chain MUST end with `OFFLINE_DRAIN_PARTIAL`, never
//!    `OFFLINE_DRAIN_COMPLETED` (consumer enforcement in Commit 6).
//!
//! Tests in `tests/backlog_drain_types.rs` (Commit 2) lock all four
//! invariants at the API boundary.

use crate::app::BootError;
use crate::db::models::enums::{DocState, NodeMode, OfflineSessionState, Severity, ShiftState};
use crate::db::models::ids::{DocumentId, OfflineSessionId, ShiftId};
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::{audit_log, fiscal_documents, node_state, offline_sessions, shifts};
use crate::db::tx::with_immediate;
use crate::services::offline_sync::kvt2_confirm;
use crate::services::reconciliation::guard::ReconcileGuard;
use crate::services::reconciliation::runtime::RuntimeView;
use crate::services::shift::transition as shift_transition;
use crate::services::write_path::error_routing::RetryClass;
use crate::services::write_path::stage_send::{self, StageSendError, StageSendOutcome};
use crate::services::write_path::types::hex_encode_lower as hex_lower;
use sqlx::SqlitePool;

// ─── Typed W12 seam ──────────────────────────────────────────────────

/// Per-doc confirmation result between W9b drain orchestrator and the
/// (deferred) W12 `lastChk` evidence path.  Pre-W12: stub
/// (Commit 5) ALWAYS returns `DeferredKvt1`.  W12 PR replaces stub
/// body with real `lastChk(fn_sign)` + Kvt1→Kvt2→Ack via
/// `stage_finalize::run`, returning `Acked { server_fiscal_no }`.
///
/// **C2 guarantees** (LOW-C2-2: `Acked` IS constructible by callers;
/// genuine unforgeability needs opaque proof type — deferred):
/// (1) `DeferredKvt1` always routes to `advanced_to_kvt1` via the
/// match in [`DrainSummary::record_doc_advanced`]; (2)
/// `advanced_to_kvt1 > 0` blocks finalize via
/// [`DrainSummary::finalize_eligibility`].  "No real Ack pre-W12"
/// is enforced by the stub-return constraint, not by the type
/// system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum W12ConfirmOutcome {
    /// W9b pre-W12 default — doc reached `Kvt1` via `stage_send::run`
    /// (wire-sent + DPS Ack received) but KVT2 confirmation has not
    /// been implemented yet.  Drain counts in `advanced_to_kvt1`;
    /// finalize branch refuses to fire.
    DeferredKvt1,
    /// W12 post-PR path — `lastChk` evidence accepted + doc advanced
    /// Kvt1 → Kvt2 → Ack via `stage_finalize::run`.  Drain counts in
    /// `advanced_to_ack`.
    Acked {
        /// Server-assigned fiscal number echoed back by DPS on the
        /// `lastChk` response; for forensic audit row attribution.
        server_fiscal_no: String,
    },
}

impl W12ConfirmOutcome {
    /// Stable string for audit row `to_state` field per spec §4.
    pub fn final_state_str(&self) -> &'static str {
        match self {
            Self::DeferredKvt1 => "KVT1",
            Self::Acked { .. } => "ACK",
        }
    }

    /// Stable string for audit row `w12_status` field per spec §2.3
    /// Step C + §9 OQ-2 resolution.
    pub fn w12_status_str(&self) -> &'static str {
        match self {
            Self::DeferredKvt1 => "DeferredKvt1",
            Self::Acked { .. } => "Acked",
        }
    }
}

// ─── DrainSummary — invariant-enforcing API ──────────────────────────

/// Aggregate per-FN drain outcome for the caller (App entry / boot
/// reconciliation).  **All fields are PRIVATE** — only mutated via
/// invariant-enforcing setter methods; no public field-level write
/// surface exists.  This prevents accidental `advanced_to_ack += 1`
/// outside the typed routing contract.
///
/// MED-C2-1 fix (operator-flagged 2026-05-20): `fiscal_number` +
/// `backlog_size_before` are ALSO private — caller cannot mutate
/// `backlog_size_before` post-construction to game the
/// `AckCountMismatch` finalize guard.  Read via accessors below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainSummary {
    fiscal_number: String,
    backlog_size_before: usize,
    advanced_to_ack: usize,
    advanced_to_kvt1: usize,
    advanced_via_lastchk_replay: usize,
    /// **M3b W12 Commit 2** — counter for `HoldFnDrainProjection::HeldAtKvt1`
    /// (Kvt1 re-entry Hold).  Blocks finalize via `DocsHeldAtKvt1` reason.
    held_at_kvt1: usize,
    /// **M3b W12 Commit 2** — counter for `HoldFnDrainProjection::HeldAtSent`
    /// (SentFresh + SentReplay Hold).  Blocks finalize via `DocsHeldAtSent` reason.
    held_at_sent: usize,
    /// **M3b W12 Commit 2** — counter for `HoldFnDrainProjection::ErRedriveQueued`
    /// (SentNotFoundDowngrade).  Blocks finalize via `DocsErRedriveQueued` reason;
    /// distinct from KVT1 hold because durable state is `ErrorRetryable`,
    /// awaiting next-tick ER class-guard bounded redrive.
    er_redrive_queued: usize,
    per_doc_failures: Vec<(DocumentId, String)>,
    finalized: bool,
}

impl DrainSummary {
    /// New summary for an FN drain attempt.  All counters start at 0;
    /// `finalized` starts false.
    pub fn new(fiscal_number: String, backlog_size_before: usize) -> Self {
        Self {
            fiscal_number,
            backlog_size_before,
            advanced_to_ack: 0,
            advanced_to_kvt1: 0,
            advanced_via_lastchk_replay: 0,
            held_at_kvt1: 0,
            held_at_sent: 0,
            er_redrive_queued: 0,
            per_doc_failures: Vec::new(),
            finalized: false,
        }
    }

    /// Record a successful per-doc advancement.  Routing is governed
    /// by the typed [`W12ConfirmOutcome`] — the ONLY way to increment
    /// `advanced_to_ack` is via [`W12ConfirmOutcome::Acked`].
    ///
    /// `via_lastchk_replay` is the spec §4 boolean flag — caller sets
    /// `true` when the doc short-circuited via lastChk pre-flight, OR
    /// `false` for wire-send completions.  Counted in
    /// `advanced_via_lastchk_replay` regardless of W12 outcome
    /// (replay can land at Kvt1 too if W12 stub is in place).
    pub fn record_doc_advanced(&mut self, outcome: &W12ConfirmOutcome, via_lastchk_replay: bool) {
        match outcome {
            W12ConfirmOutcome::DeferredKvt1 => self.advanced_to_kvt1 += 1,
            W12ConfirmOutcome::Acked { .. } => self.advanced_to_ack += 1,
        }
        if via_lastchk_replay {
            self.advanced_via_lastchk_replay += 1;
        }
    }

    /// Record a per-doc failure.  Sibling docs continue per plan §Task
    /// 9 "try-and-audit shim".  `failure_class` is a stable string per
    /// spec §4 taxonomy (see [`failure_class_for`]).
    pub fn record_doc_failure(&mut self, document_id: DocumentId, failure_class: String) {
        self.per_doc_failures.push((document_id, failure_class));
    }

    /// **M3b W12 Commit 2** — record a `HoldFnDrainProjection::HeldAtKvt1`
    /// outcome.  Fed by `Kvt2ConfirmSource::Kvt1Reentry` Hold ONLY (per
    /// MED-PR70-R7-02 projection matrix).  Doc state stays `Kvt1`.
    /// Blocks `finalize_eligibility` with `DocsHeldAtKvt1` reason.
    ///
    /// **Aggregation rationale (REC-7, Phase 2b 2026-05-24)**:
    /// `_doc_id` and `_hold_class` are intentionally underscore-
    /// prefixed (accepted-but-unused) per operator-pinned design
    /// (memory `feedback_db_vs_log_separation`): detailed per-doc
    /// forensic trace lives в `audit_log` (`KVT2_CONFIRM_HOLD`
    /// events з payload {document_id, source, hold_reason,
    /// hold_reason_detail, dispatch_via, trace_attempt_no}); the
    /// in-memory `DrainSummary` deliberately aggregates only
    /// counters для maximum CPU/RAM efficiency in the per-tick
    /// drain hot path.  Args preserved для:
    /// (a) API parity з `record_doc_failure` (which DOES persist
    ///     per-doc detail в `per_doc_failures: Vec<(DocumentId, String)>`);
    /// (b) future-readiness — Phase 3 might activate per-doc Hold
    ///     tracking for advanced analytics (REC-1 admin CLI Tier 3
    ///     surface, REC-8 W8 race detection telemetry, etc.) without
    ///     a downstream API break.
    pub fn record_doc_held_at_kvt1(&mut self, _doc_id: DocumentId, _hold_class: String) {
        self.held_at_kvt1 += 1;
    }

    /// **M3b W12 Commit 2** — record a `HoldFnDrainProjection::HeldAtSent`
    /// outcome.  Fed by `Kvt2ConfirmSource::SentFresh` Hold (pre-Envelope-1a)
    /// AND `Kvt2ConfirmSource::SentReplay` Hold (post-Envelope-1c-hold).
    /// Doc state stays `Sent`.  Blocks `finalize_eligibility` with
    /// `DocsHeldAtSent` reason.
    ///
    /// **Aggregation rationale (REC-7, Phase 2b 2026-05-24)**: see
    /// [`record_doc_held_at_kvt1`] — same intentional aggregation
    /// design; per-doc detail в `audit_log.KVT2_CONFIRM_HOLD` event
    /// payload.
    pub fn record_doc_held_at_sent(&mut self, _doc_id: DocumentId, _hold_class: String) {
        self.held_at_sent += 1;
    }

    /// **M3b W12 Commit 2** — record a `HoldFnDrainProjection::ErRedriveQueued`
    /// outcome.  Fed by `Kvt2ConfirmOutcome::SentNotFoundDowngrade` ONLY
    /// (per plan §"Source-context routing matrix").  Durable state is
    /// `ErrorRetryable` after Envelope 1c-post; awaiting next-tick W9b ER
    /// class-guard bounded redrive (`MAX_BOOT_ATTEMPTS=5`).  Blocks
    /// `finalize_eligibility` with `DocsErRedriveQueued` reason (NOT
    /// `DocsHeldAtKvt1` — durable state is ER, not Kvt1).
    ///
    /// **Aggregation rationale (REC-7, Phase 2b 2026-05-24)**: see
    /// [`record_doc_held_at_kvt1`] — same intentional aggregation
    /// design; per-doc detail в `audit_log.OFFLINE_DRAIN_DOC_FAILED`
    /// event payload (з `dispatch_via=kvt2_confirm` +
    /// `probe_outcome=NotFound` + `downgrade_to=ERROR_RETRYABLE`).
    pub fn record_doc_er_redrive_queued(&mut self, _doc_id: DocumentId, _downgrade_class: String) {
        self.er_redrive_queued += 1;
    }

    /// Decide whether the drain may finalize (node mode + offline
    /// session transitions).  Returns the typed eligibility — caller
    /// MUST pattern-match.
    ///
    /// Precedence (any single nonzero blocker returns `NotEligible`):
    /// 1. `per_doc_failures` non-empty (W9b);
    /// 2. **M3b W12 Commit 2** — any of three W12 hold counters > 0:
    ///    - `held_at_kvt1` → `DocsHeldAtKvt1`,
    ///    - `held_at_sent` → `DocsHeldAtSent`,
    ///    - `er_redrive_queued` → `DocsErRedriveQueued`;
    /// 3. `advanced_to_kvt1` > 0 → `DocsDeferredAtKvt1` (legacy
    ///    `W12ConfirmOutcome::DeferredKvt1`; inert post-W12 once full
    ///    helper wiring lands in Commits 4 / 5 / 5b);
    /// 4. `advanced_to_ack != backlog_size_before` → `AckCountMismatch`
    ///    (defensive accounting drift guard).
    ///
    /// Forensic note: the chosen reason is the highest-precedence
    /// blocker; `OFFLINE_DRAIN_PARTIAL` audit payload via
    /// [`build_finalize_payload`] reports ALL counter values
    /// regardless of which reason was selected, so operator
    /// dashboards see the full per-counter breakdown when multiple
    /// blockers coexist (multi-reason payload per plan §11).
    pub fn finalize_eligibility(&self) -> FinalizeEligibility {
        if !self.per_doc_failures.is_empty() {
            return FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::PerDocFailuresPresent {
                    count: self.per_doc_failures.len(),
                },
            };
        }
        if self.held_at_kvt1 > 0 {
            return FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsHeldAtKvt1 {
                    count: self.held_at_kvt1,
                },
            };
        }
        if self.held_at_sent > 0 {
            return FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsHeldAtSent {
                    count: self.held_at_sent,
                },
            };
        }
        if self.er_redrive_queued > 0 {
            return FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsErRedriveQueued {
                    count: self.er_redrive_queued,
                },
            };
        }
        if self.advanced_to_kvt1 > 0 {
            return FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsDeferredAtKvt1 {
                    count: self.advanced_to_kvt1,
                },
            };
        }
        if self.advanced_to_ack != self.backlog_size_before {
            return FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::AckCountMismatch {
                    expected: self.backlog_size_before,
                    actual: self.advanced_to_ack,
                },
            };
        }
        FinalizeEligibility::Eligible
    }

    /// Set `finalized = true` ONLY if `finalize_eligibility()` returns
    /// [`FinalizeEligibility::Eligible`].  Returns the eligibility
    /// reason on failure so the caller can emit the right audit
    /// (`OFFLINE_DRAIN_PARTIAL`) without re-computing.
    pub fn mark_finalized(&mut self) -> Result<(), FinalizeError> {
        match self.finalize_eligibility() {
            FinalizeEligibility::Eligible => {
                self.finalized = true;
                Ok(())
            }
            FinalizeEligibility::NotEligible { reason } => Err(FinalizeError::NotEligible(reason)),
        }
    }

    // ─── Read-only accessors (audit emit + tests) ────────────────────

    pub fn fiscal_number(&self) -> &str {
        &self.fiscal_number
    }

    pub fn backlog_size_before(&self) -> usize {
        self.backlog_size_before
    }

    pub fn advanced_to_ack(&self) -> usize {
        self.advanced_to_ack
    }

    pub fn advanced_to_kvt1(&self) -> usize {
        self.advanced_to_kvt1
    }

    pub fn advanced_via_lastchk_replay(&self) -> usize {
        self.advanced_via_lastchk_replay
    }

    pub fn per_doc_failures(&self) -> &[(DocumentId, String)] {
        &self.per_doc_failures
    }

    /// **M3b W12 Commit 2** — count of Kvt1Reentry Hold outcomes
    /// recorded via [`record_doc_held_at_kvt1`].
    pub fn held_at_kvt1(&self) -> usize {
        self.held_at_kvt1
    }

    /// **M3b W12 Commit 2** — count of SentFresh + SentReplay Hold
    /// outcomes recorded via [`record_doc_held_at_sent`].
    pub fn held_at_sent(&self) -> usize {
        self.held_at_sent
    }

    /// **M3b W12 Commit 2** — count of SentNotFoundDowngrade outcomes
    /// recorded via [`record_doc_er_redrive_queued`].
    pub fn er_redrive_queued(&self) -> usize {
        self.er_redrive_queued
    }

    pub fn finalized(&self) -> bool {
        self.finalized
    }
}

/// Result of [`DrainSummary::finalize_eligibility`] — typed so caller
/// cannot bypass the invariant by reading raw counters and deciding
/// themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizeEligibility {
    /// All N docs reached Ack via `W12ConfirmOutcome::Acked`; no
    /// per-doc failures; finalize allowed.
    Eligible,
    /// At least one structural condition blocks finalize.  Caller
    /// emits `OFFLINE_DRAIN_PARTIAL` audit using the `reason` payload.
    NotEligible { reason: NotEligibleReason },
}

/// Why the drain cannot finalize.  Used in `OFFLINE_DRAIN_PARTIAL`
/// audit payload for operator forensic triage.
///
/// `#[non_exhaustive]` — semver-stability for downstream pattern
/// matching as new finalize-blocker categories arrive (W12 Commit 2
/// added 3 new W12-projection variants; future tasks may add more).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotEligibleReason {
    /// At least one doc failed per-doc processing.  Sibling docs
    /// may have succeeded but at least one failure prevents finalize.
    PerDocFailuresPresent { count: usize },
    /// **M3b W12 Commit 2** — at least one doc returned
    /// `DocVerdict::HoldFnDrain { projection: HeldAtKvt1 }` from
    /// `Kvt2ConfirmSource::Kvt1Reentry`.  Durable state stays `Kvt1`;
    /// drain stopped at the held doc this tick.  Next tick re-enters
    /// via Kvt1 cohort dispatch.
    DocsHeldAtKvt1 { count: usize },
    /// **M3b W12 Commit 2** — at least one doc returned
    /// `DocVerdict::HoldFnDrain { projection: HeldAtSent }` from
    /// `Kvt2ConfirmSource::SentFresh` (pre-Envelope-1a) OR
    /// `Kvt2ConfirmSource::SentReplay` (post-Envelope-1c-hold).
    /// Durable state stays `Sent`; drain stopped at the held doc
    /// this tick.  Next tick re-enters via Sent-replay cohort dispatch.
    DocsHeldAtSent { count: usize },
    /// **M3b W12 Commit 2** — at least one doc returned
    /// `DocVerdict::HoldFnDrain { projection: ErRedriveQueued }` from
    /// `Kvt2ConfirmOutcome::SentNotFoundDowngrade`.  Durable state
    /// advanced `Sent → ErrorRetryable` via Envelope 1c-post; awaiting
    /// next-tick W9b ER class-guard bounded Pattern B redrive.
    /// Distinct from `DocsHeldAtKvt1` because durable state is ER, not
    /// Kvt1 (MED-PR70-R6-02 projection-correct reason).
    DocsErRedriveQueued { count: usize },
    /// At least one doc returned `W12ConfirmOutcome::DeferredKvt1` —
    /// legacy W9b pre-W12 stub pin.  Inert post-W12 once full helper
    /// wiring lands (W12ConfirmOutcome::DeferredKvt1 no longer
    /// produced by W12-aware paths); kept for backward-compat with
    /// crash-recovered docs from pre-W12 history.
    DocsDeferredAtKvt1 { count: usize },
    /// `advanced_to_ack != backlog_size_before` despite no recorded
    /// failures.  Defensive: should be unreachable in practice
    /// because failures + deferred + acked should sum to backlog
    /// size; this variant catches accounting drift.
    AckCountMismatch { expected: usize, actual: usize },
}

/// Error type for [`DrainSummary::mark_finalized`] — typed wrapper
/// so caller can pattern-match without re-reading the summary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinalizeError {
    #[error("drain not eligible to finalize: {0:?}")]
    NotEligible(NotEligibleReason),
}

// ─── failure_class taxonomy ──────────────────────────────────────────

/// Stable string per spec §4 — operator dashboards filter on these.
/// Matches W8a `dps_error_class` convention (no Debug-repr instability).
pub fn failure_class_for(class: FailureClass) -> &'static str {
    match class {
        FailureClass::SignerRefused => "signer_refused",
        FailureClass::StateConflict => "state_conflict",
        FailureClass::WireRoutingTerminalReject => "wire_routing_terminal_reject",
        FailureClass::WireRoutingProbeRequired => "wire_routing_probe_required",
        FailureClass::WireRoutingTransientRetry => "wire_routing_transient_retry",
        FailureClass::Transport => "transport",
        FailureClass::Authorization => "authorization",
        FailureClass::Server => "server",
        FailureClass::Decode => "decode",
        FailureClass::Internal => "internal",
        FailureClass::NotFound => "not_found",
        FailureClass::OfflineFiscalNoMissing => "offline_fiscal_no_missing",
        // M3b W9b ER-class-guard: TransientRetry with exhausted
        // boot-attempt budget — operator triage required, doc CAS to
        // RequiresManualReconciliation.  Distinct dashboard signal vs.
        // the per-class manual escalation classes above so operators
        // can spot "infinite retry loop" stuck docs separately.
        FailureClass::BudgetExhausted => "budget_exhausted",
        // M3b W9b ER-class-guard: durable `retry_class` missing OR
        // unknown — drain has no evidence to choose a redrive path.
        // Sibling-continue + hold (no CAS): matches boot semantics from
        // `boot_phase::dispatch_error_retryable_by_class` HoldIndeterminate
        // arm.  Reclassification to manual-class halt is a separate spec
        // decision (operator-confirmed 2026-05-22 scope).
        FailureClass::RetryClassIndeterminate => "retry_class_indeterminate",
        // M2-04 defense-in-depth (2026-06-12): stage_finalize returned
        // ChainSeedMismatch — the doc's previous_hash does not match the
        // FN's current chain seed.  After the M2-01 fix this should be
        // unreachable for offline-origin docs (finalize skips the guard),
        // but any future drift must NOT silent-loop the FN drain tick:
        // escalate the shift to RequiresManualReconciliation (spec §16.7).
        FailureClass::ChainSeedMismatch => "chain_seed_mismatch",
    }
}

/// Closed-enum failure taxonomy for drain per-doc outcomes.  Pre-W12,
/// covers all failure modes the orchestrator emits.  W12 PR may add
/// `LastChkMismatch` variants — `#[non_exhaustive]` allows extension
/// without compile-breaking dashboard consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FailureClass {
    SignerRefused,
    StateConflict,
    WireRoutingTerminalReject,
    WireRoutingProbeRequired,
    WireRoutingTransientRetry,
    Transport,
    Authorization,
    Server,
    Decode,
    Internal,
    NotFound,
    OfflineFiscalNoMissing,
    /// M3b W9b ER-class-guard — TransientRetry boot-attempt budget cap
    /// (`attempts_used >= MAX_BOOT_ATTEMPTS`) exhausted; manual-recon.
    BudgetExhausted,
    /// M3b W9b ER-class-guard — no durable `retry_class` recorded for
    /// the ER doc; sibling-continue hold (non-manual).
    RetryClassIndeterminate,
    /// M2-04 defense (2026-06-12) — `stage_finalize::run` returned
    /// `ChainSeedMismatch` (doc `previous_hash` ≠ FN chain seed).
    /// Manual-recon class: escalates the shift to
    /// `RequiresManualReconciliation` instead of aborting the FN drain
    /// tick every cycle (the M2-04 silent-loop).
    ChainSeedMismatch,
}

// ─── C3 + C4: orchestrator section ───────────────────────────────────

/// Audit entity_type for FN-scoped drain lifecycle events
/// (`OFFLINE_DRAIN_STARTED` / `_SKIPPED_*` / `_COMPLETED` / `_PARTIAL`).
/// Mirrors the `return_online_probe` convention — drain is an
/// FN-scoped operation, indexed by `fiscal_number`.  Per-session
/// transition events (`OFFLINE_SESSION_DRAIN_STARTED`) continue to
/// use `entity_type = "offline_session"` per W5 convention.
const AUDIT_ENTITY_DRAIN_FN: &str = "node_state";

/// Audit entity_type for per-doc drain events
/// (`OFFLINE_DRAIN_DOC_ADVANCED` / `_DOC_FAILED`).  Matches the
/// M3a / W7 convention: per-doc events anchor on `entity_type =
/// "fiscal_document"` + `entity_id = doc_id_hex`.
///
/// **W12 Commit 4a hygiene (LOW-W12C4A-02, 2026-05-22)**:
/// `pub(crate)` so sibling modules in `services::offline_sync` (W12
/// `kvt2_confirm`'s SentReplay / Hold / Drift envelopes) can share the
/// literal instead of duplicating `"fiscal_document"` strings.  Audit
/// dashboards filter on this exact value; co-locating the const
/// eliminates rename drift between drain orchestrator and helper writes.
/// (RS-3 A2.1a moved the Sent/Kvt1-entry Envelope 1a/1b writes to
/// `write_path::kvt2_advance`, which mirrors this literal locally to stay
/// runtime-neutral — no backwards dependency on `offline_sync`.)
pub(crate) const AUDIT_ENTITY_DOC: &str = "fiscal_document";

/// W9b §2.1 entry (b) — pure-function drain entry for the boot
/// reconciliation path and integration tests.  The App-owned entry
/// `App::drain_offline_backlog_with` (C7) wraps this with the App
/// reconcile mutex (OQ-5 operator pin).
///
/// **Commit 3 scope (skeleton):** runs the spec §2.2 prerequisites
/// only — mode-check, backlog read, offline-session transition, and
/// `OFFLINE_DRAIN_STARTED` audit emit.  **No per-doc loop yet** —
/// Commit 4 invokes `stage_send::run` per doc in `lnd ASC` order;
/// Commits 5-6 add lastChk pre-flight + finalization branch.
///
/// `_recon_guard` is the W2 lock-token proving the caller holds the
/// App reconcile mutex (NIT-C7-R1 hardening 2026-05-21).  Production
/// callers construct it via `App::drain_offline_backlog_with`;
/// integration tests use `ReconcileGuard::for_integration_test_only()`
/// (gated behind `test-support` feature).  Without the token, the
/// drain entry physically cannot be called — closes the W2 bypass
/// hole symmetric to `boot_phase::run_boot_reconciliation`.
///
/// `deps` is the per-FN runtime bundle (`dps`, `signing_ctx`,
/// `fn_sign`).  The prerequisites pass touches nothing on it (I1
/// guard); the per-doc loop consumes `deps.dps` + `deps.signing_ctx`
/// when invoking [`stage_send::run`].  `deps.fn_sign` is reserved for
/// the C5 `lastChk` pre-flight + W12 stub.
///
/// ## Return contract
///
/// On prerequisite skip (mode ≠ `GoingOnline` OR empty backlog) the
/// returned [`DrainSummary`] has `backlog_size_before = 0`; audit
/// `OFFLINE_DRAIN_SKIPPED_*` row is emitted to the entity row
/// `(AUDIT_ENTITY_DRAIN_FN, fiscal_number)`.
///
/// Otherwise the summary carries `backlog_size_before = backlog.len()`
/// and per-doc counters reflecting actual processing: post W12 Commit
/// 4b each successful wire send + lastChk Acked converges through
/// `confirm_drain_doc(SentFresh)` → Envelope 1a + Envelope 2 → Ack,
/// incrementing `advanced_to_ack`; each non-Sent outcome appends to
/// `per_doc_failures`.  On a pending-drain shift, the loop halts
/// early on a manual-recon-class failure (see
/// [`is_manual_recon_retry_class`]), transitions shift and the
/// node_state mirror to `RequiresManualReconciliation`, and emits a
/// Critical `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit; in that
/// case the summary reflects state up to the halt position.
///
/// Post W12 Commit 4b (SentFresh production-wired), the SentFresh
/// happy-path drain can reach the Eligible arm and emit
/// `OFFLINE_DRAIN_COMPLETED` + close session/node/shift in the same
/// public entry call.  Other source contexts (Kvt1Reentry /
/// SentReplay) are scope-guarded at `confirm_drain_doc` until
/// Commits 5/5b wire their Envelope 1b / 1c-pre chains; Hold paths
/// return `BootError::Internal` until Commit 6 lands the
/// `HoldFnDrain` projection.  Caller should check
/// `summary.finalized()` to distinguish completed-Eligible from
/// PARTIAL outcomes — `summary.mark_finalized()` only flips on the
/// Eligible-arm envelope commit, so the flag is a reliable signal.
///
/// ## Errors
///
/// - `BootError::Database` — sqlx error on a read/append/CAS.
/// - `BootError::Internal` — structural drift: missing `node_state`
///   row, backlog non-empty without active session, or session CAS
///   Open→Draining produced non-`Applied` outcome (Conflict / NotFound
///   under App mutex implies racing writer — invariant breach).
/// - `BootError::ReconciliationFailed` — `with_immediate` envelope
///   propagated a non-sqlx anyhow chain (e.g. audit insert failure).
pub async fn drain<'a>(
    _recon_guard: &ReconcileGuard<'a>,
    pool: &SqlitePool,
    deps: &RuntimeView<'_>,
    fiscal_number: &str,
) -> Result<DrainSummary, BootError> {
    // ─── Step 1: read node_state mode (must be GoingOnline) ──────────
    let ns = node_state::get(pool, fiscal_number)
        .await
        .map_err(BootError::Database)?
        .ok_or_else(|| {
            BootError::Internal(format!(
                "backlog_drain({fiscal_number}): node_state row missing"
            ))
        })?;

    if ns.mode != NodeMode::GoingOnline {
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "current_mode": ns.mode.as_str(),
        });
        audit_log::append(
            pool,
            AUDIT_ENTITY_DRAIN_FN,
            fiscal_number,
            "OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE",
            Severity::Info,
            None,
            Some(&payload.to_string()),
        )
        .await
        .map_err(BootError::Database)?;
        return Ok(DrainSummary::new(fiscal_number.to_string(), 0));
    }

    // ─── Step 1b: manual-reconciliation re-entry guard (AUD-K8-1) ────
    // escalate_drain_to_manual halts the cohort by CAS-ing the shift to
    // RequiresManualReconciliation (+ node_state.shift_state mirror) but
    // leaves mode == GoingOnline and the session DRAINING. Without this
    // guard the next supervisor/boot drain tick re-enters: the REJECTED
    // predecessor has left the candidate cohort, so the orphaned successor
    // becomes the head and is re-sent — defeating the escalation's
    // "durable operator surface, halts FN drain" contract. Exit requires
    // an operator-driven resolution of the manual-recon shift.
    //
    // No per-tick audit row here: the RMR-FN surface persists until an
    // operator resolves it (hours), backoff is reset so drain re-enters
    // every tick — a row per re-entry would flood the durable ledger.
    // The durable record already exists: the CRITICAL
    // OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL emitted at escalation time.
    //
    // The shift_state mirror is authoritative here — escalate writes the
    // shift CAS + mirror atomically (apply_shift_transition, RS-3 C1); the
    // SEAM-D-1 mirror-desync concern is a boot-reconstruction path, not this.
    if ns.shift_state == ShiftState::RequiresManualReconciliation {
        return Ok(DrainSummary::new(fiscal_number.to_string(), 0));
    }

    // ─── Step 2: read active offline session (OPEN|DRAINING) ─────────
    //
    // Reordered to run BEFORE the backlog scan (HIGH-C5-1 fix
    // 2026-05-21): the cohort walker SQL now filters on
    // `offline_session_id = ?` to avoid capturing online docs of the
    // same FN; we need the session id in hand before issuing the
    // SELECT.  Missing session → empty drain (no active session means
    // no offline docs to drain; differs from prior C3 logic which
    // treated missing-session-with-backlog as Internal).
    let active_session = offline_sessions::current_open_or_draining_session(pool, fiscal_number)
        .await
        .map_err(BootError::Database)?;
    let Some((session_id, session_state)) = active_session else {
        // No active session → no offline cohort can exist (W7 always
        // stamps offline_session_id).  Treat as empty-backlog skip
        // with a distinct audit event for forensic clarity.
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "current_mode": ns.mode.as_str(),
            "reason": "no_active_offline_session",
        });
        audit_log::append(
            pool,
            AUDIT_ENTITY_DRAIN_FN,
            fiscal_number,
            "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG",
            Severity::Info,
            None,
            Some(&payload.to_string()),
        )
        .await
        .map_err(BootError::Database)?;
        return Ok(DrainSummary::new(fiscal_number.to_string(), 0));
    };

    // ─── Step 3: read backlog scoped to the active session ───────────
    let backlog = fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd(
        pool,
        fiscal_number,
        session_id,
    )
    .await
    .map_err(BootError::Database)?;

    if backlog.is_empty() {
        // MED-W12C3-01 (2026-05-22) — drain-finalize crash-recovery
        // branch.  Pre-Commit-3, KVT2 → Ack advance was structurally
        // unreachable from drain context, so empty cohort always meant
        // "nothing to drain" (either fresh session or earlier tick
        // already finalized).  Post-Commit-3, drain itself can advance
        // KVT2 → Ack durably via `stage_finalize::run`, opening a
        // crash window between that envelope commit and the
        // `finalize_drain` 5-write closure: if the process dies in
        // between, the next tick sees empty cohort (doc is now Ack,
        // excluded by the IN list filter) but node/session/shift are
        // still in pre-finalize states.  Reviewer-flagged MED finding
        // 2026-05-22 (closed in this Δ commit).
        //
        // Recovery predicate (conservative, operator-pinned):
        //   - session_state must be `Draining` (Open + all-Ack is
        //     structural drift — drain Open→Draining mid-pass
        //     transition would have committed before any doc reached
        //     Ack, so Open + all-Ack cannot legitimately occur);
        //   - all session docs must be in terminal `ACK` state
        //     (`is_session_drain_completable` predicate;
        //     `REJECTED`/`MANUAL` deliberately NOT included — those
        //     require explicit operator treatment per session-closure
        //     semantics).
        // Both conditions true → finalize via `CrashRecovery` entry
        // (distinct `OFFLINE_DRAIN_RECOVERED_FINALIZE` audit).
        // Otherwise → existing empty-backlog skip path.
        if session_state == OfflineSessionState::Draining
            && fiscal_documents::is_session_drain_completable(pool, session_id)
                .await
                .map_err(BootError::Database)?
        {
            let mut recovery_summary = DrainSummary::new(fiscal_number.to_string(), 0);
            commit_finalize_envelope(
                pool,
                fiscal_number,
                session_id,
                &ns,
                &mut recovery_summary,
                FinalizeEntry::CrashRecovery,
            )
            .await?;
            return Ok(recovery_summary);
        }
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "current_mode": ns.mode.as_str(),
            "session_id": hex_lower(session_id.as_bytes()),
        });
        audit_log::append(
            pool,
            AUDIT_ENTITY_DRAIN_FN,
            fiscal_number,
            "OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG",
            Severity::Info,
            None,
            Some(&payload.to_string()),
        )
        .await
        .map_err(BootError::Database)?;
        return Ok(DrainSummary::new(fiscal_number.to_string(), 0));
    }

    // If session is OPEN, transition to DRAINING + emit W5 audit in
    // one with_immediate envelope (mirrors OfflineSessionService::
    // start_drain shape).  Re-entry (state already DRAINING) is a
    // no-op for the session-state branch.
    if session_state == OfflineSessionState::Open {
        let session_id_for_tx = session_id;
        let outcome = with_immediate(pool, move |tx| {
            Box::pin(async move {
                let outcome = offline_sessions::transition_state(
                    tx,
                    session_id_for_tx,
                    OfflineSessionState::Open,
                    OfflineSessionState::Draining,
                    None,
                )
                .await?;
                if outcome == TransitionOutcome::Applied {
                    let id_hex = hex_lower(session_id_for_tx.as_bytes());
                    let payload = serde_json::json!({
                        "offline_session_id": id_hex,
                        "from": OfflineSessionState::Open.as_str(),
                        "to": OfflineSessionState::Draining.as_str(),
                        "reason_abort": serde_json::Value::Null,
                    });
                    audit_log::append_tx(
                        tx,
                        "offline_session",
                        &id_hex,
                        "OFFLINE_SESSION_DRAIN_STARTED",
                        Severity::Info,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                }
                Ok::<TransitionOutcome, anyhow::Error>(outcome)
            })
        })
        .await
        .map_err(|source| BootError::ReconciliationFailed {
            fiscal_number: fiscal_number.to_string(),
            source,
        })?;

        if outcome != TransitionOutcome::Applied {
            return Err(BootError::Internal(format!(
                "backlog_drain({fiscal_number}): session {sid} CAS Open→Draining produced {outcome:?} (App reconcile mutex should prevent races; investigate concurrent writers)",
                sid = hex_lower(session_id.as_bytes()),
            )));
        }
    }

    // ─── Step 4: emit OFFLINE_DRAIN_STARTED ──────────────────────────
    let payload = serde_json::json!({
        "fiscal_number": fiscal_number,
        "backlog_size": backlog.len(),
        "session_id": hex_lower(session_id.as_bytes()),
        "started_at_iso": chrono::Utc::now().to_rfc3339(),
    });
    audit_log::append(
        pool,
        AUDIT_ENTITY_DRAIN_FN,
        fiscal_number,
        "OFFLINE_DRAIN_STARTED",
        Severity::Info,
        None,
        Some(&payload.to_string()),
    )
    .await
    .map_err(BootError::Database)?;

    // ─── Step 5 (C4): per-doc loop ───────────────────────────────────
    //
    // Strict `lnd ASC` order is preserved by the C1 helper.  Each doc
    // is processed through `stage_send::run`; outcomes are routed via
    // private helpers below into either `record_doc_advanced` (Sent →
    // inline `Sent → Kvt1` stub) or `record_doc_failure` + audit.
    //
    // **Sibling-continue scope (spec amendment 2026-05-21).**  Per-doc
    // failures sibling-continue ONLY on non-pending-drain shifts.  When
    // `shift_state ∈ {OpenedLocalPendingDrain, ClosingLocalPendingDrain}`
    // AND a per-doc failure surfaces, drain ESCALATES: CAS shift →
    // `RequiresManualReconciliation` via edges 6 / 14, emits Critical
    // `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit, and halts FN drain.
    // Subsequent backlog docs are NOT processed in this tick.  Per
    // `LEGAL_INVARIANTS.md` §INV-19 + spec §6.3: drain has crossed the
    // local-commit threshold; rollback semantics don't apply.
    //
    // Only infrastructure failures (audit_log append fails with sqlx
    // error, post-stage_send Sent→Kvt1 CAS produces non-Applied)
    // propagate as `BootError::*` to the caller.
    let mut summary = DrainSummary::new(fiscal_number.to_string(), backlog.len());
    for (position, doc) in backlog.iter().enumerate() {
        let verdict = process_one_doc(pool, deps, fiscal_number, doc, &mut summary).await?;
        // ── M2-N1 / M2-N2a (architect ruling B, 2026-06-13) ── STRICT-
        // SEQUENTIAL offline-origin drain.  The whole drain cohort is
        // `fs_mode='OFFLINE'` (a strict M2-01 predecessor chain: each doc
        // signs `previous_hash = unsigned(prior issued doc)`), so we MUST
        // NOT process/send a doc whose predecessor did not reach ACK
        // (cascade-reject / broken wire-chain).  The chain therefore STOPS
        // at the FIRST non-ACK doc; higher-`lnd` successors are NOT
        // processed this tick.  Online-origin docs are NOT in this cohort —
        // they converge via the separate `online_convergence` tick, behaviour
        // untouched.  Escalation policy (ruling B):
        //   - terminal / non-self-resolving (REJECTED, doc-level manual,
        //     SupersededHeld-without-B1-v2) → halt + escalate the FN to
        //     `RequiresManualReconciliation` REGARDLESS of shift_state (plain
        //     `Opened` via whitelist edge 15; pending-drain via edges 6/14) —
        //     a durable operator surface, never a silent GoingOnline/Draining
        //     wedge (the REJECTED doc leaves the cohort, so without this it
        //     would wedge forever per M2-N2a).
        //   - transient / retryable (HoldFnDrain, ERROR_RETRYABLE,
        //     probe-required `Failed{manual_recon:false}`) → halt the chain
        //     for THIS tick only, NO Manual; the doc stays in the cohort and
        //     retries next tick — preserving the §6.5 retry budget + REC-1
        //     tier degradation (a transient blip must NOT escalate to Manual
        //     on the first hold).
        match verdict {
            DocVerdict::Advanced => {
                // ACK predecessor — the strict chain may advance to lnd+1.
            }
            DocVerdict::HoldFnDrain {
                class,
                projection,
                consecutive_holds,
            } => {
                // Transient hold: halt the chain THIS tick, NO Manual, keep
                // the REC-1 tier budget alive (doc stays in cohort, retries
                // next tick).  Record via the projection-specific method so
                // `finalize_drain` emits `OFFLINE_DRAIN_PARTIAL` with the
                // correct reason.
                let class_str = failure_class_for(class).to_string();
                match projection {
                    HoldFnDrainProjection::HeldAtKvt1 => {
                        summary.record_doc_held_at_kvt1(doc.document_id, class_str);
                    }
                    HoldFnDrainProjection::HeldAtSent => {
                        summary.record_doc_held_at_sent(doc.document_id, class_str);
                    }
                    HoldFnDrainProjection::ErRedriveQueued => {
                        summary.record_doc_er_redrive_queued(doc.document_id, class_str);
                    }
                }
                // **REC-1 Phase 2a.1 (2026-05-24)** — Tier 1+2 degradation
                // triggers for a persistently-held predecessor (the strict
                // chain stays blocked behind it across ticks).  ErRedriveQueued
                // (consecutive_holds=0 by 1c-post reset) bypasses both tiers.
                // Tier 2 checked first (more severe; implies Tier 1 last cycle).
                if consecutive_holds >= 50
                    && matches!(
                        projection,
                        HoldFnDrainProjection::HeldAtSent | HoldFnDrainProjection::HeldAtKvt1
                    )
                {
                    trigger_tier_2_stop_mode(
                        pool,
                        fiscal_number,
                        doc.document_id,
                        consecutive_holds,
                    )
                    .await?;
                } else if consecutive_holds >= 10
                    && matches!(
                        projection,
                        HoldFnDrainProjection::HeldAtSent | HoldFnDrainProjection::HeldAtKvt1
                    )
                {
                    trigger_tier_1_prolonged_hold(
                        pool,
                        doc.document_id,
                        projection,
                        consecutive_holds,
                    )
                    .await?;
                }
                break;
            }
            DocVerdict::Failed {
                class,
                manual_recon: false,
            } => {
                // Transient/retryable (ERROR_RETRYABLE / probe-required):
                // halt the chain THIS tick, NO Manual; the doc stays in the
                // cohort and retries next tick (ER class-guard / re-probe).
                // Already recorded as a per-doc failure by `process_one_doc`.
                let _ = class;
                break;
            }
            DocVerdict::Failed {
                class,
                manual_recon: true,
            } => {
                // Terminal / non-self-resolving (REJECTED / doc-level manual):
                // halt + escalate the FN to RequiresManualReconciliation.
                escalate_drain_to_manual(
                    pool,
                    fiscal_number,
                    &ns,
                    doc.document_id,
                    failure_class_for(class),
                    position,
                )
                .await?;
                return Ok(summary);
            }
            DocVerdict::SupersededHeld => {
                // Non-self-resolving while B1-v2 doc-scoped confirmation does
                // not exist: a superseded predecessor is non-ACK from the
                // successor's chain view AND would wedge if merely held
                // (re-superseded every tick).  So halt + escalate Manual.
                // (Reverses SEAM-B-3's sibling-continue per the M2-N1 contract
                // note + ruling B.)  `confirm_drain_doc` already emitted the
                // TIP_SUPERSEDED audit + completed the recovery trace.
                summary.record_doc_held_at_sent(doc.document_id, "superseded".to_string());
                escalate_drain_to_manual(
                    pool,
                    fiscal_number,
                    &ns,
                    doc.document_id,
                    "superseded",
                    position,
                )
                .await?;
                return Ok(summary);
            }
        }
    }

    // ─── Step 6 (C6): finalization branch ────────────────────────────
    //
    // Spec §2.4 + amendment 2026-05-21: evaluate
    // `summary.finalize_eligibility()` and route to one of:
    //
    //   - Eligible → CAS `node_state.mode: GoingOnline → Online`,
    //     CAS `offline_session: Draining → Closed`, mark summary
    //     finalized, emit `OFFLINE_DRAIN_COMPLETED` audit.  All four
    //     writes commit in ONE `with_immediate` envelope so the
    //     drain cannot leave half-finalized state (mode flipped but
    //     session still Draining, etc).
    //   - NotEligible{reason} → emit `OFFLINE_DRAIN_PARTIAL` audit
    //     with the typed reason payload.  Node + session stay in
    //     their pre-drain states; next drain tick re-evaluates.
    //
    // Operator-pinned pre-W12 invariant: the C5 stub
    // `apply_w12_confirmation` ALWAYS returns `DeferredKvt1`, so
    // `advanced_to_kvt1 > 0` blocks finalize via
    // `NotEligibleReason::DocsDeferredAtKvt1`.  The Eligible branch
    // is structurally unreachable pre-W12 (drain cannot synthesize
    // real Ack proof).  W12 PR plugs `lastChk` evidence → Acked
    // outcomes → eligibility flips to Eligible.
    finalize_drain(pool, fiscal_number, session_id, &ns, &mut summary).await?;
    Ok(summary)
}

// ─── C4: per-doc loop helpers ────────────────────────────────────────

/// Outcome bucket for a single doc — used by [`drain`] to detect the
/// pending-drain shift halt condition (spec amendment 2026-05-21).
///
/// `Failed::manual_recon` distinguishes manual-recon-class outcomes
/// (TerminalReject, FnConfigError, WrapperBug, MacRecovery,
/// OperatorEscalation, StateConflict, DocumentMissing, SignerRefused,
/// all `StageSendError`) from transient / hold-class outcomes
/// (TransientRetry, ProbeRequired).  Only manual-recon-class outcomes
/// trigger the pending-drain halt + shift escalation per spec §3.5
/// gravity principle (Manual = last resort; transient/hold retains
/// retry budget).
///
/// `class: FailureClass` is `Copy` — no per-failure allocation in the
/// loop; the stable-string conversion happens once at the audit emit
/// and once at `record_doc_failure` (which forces ownership via the
/// `String` argument shape inherited from C2's
/// [`DrainSummary::record_doc_failure`]).
enum DocVerdict {
    Advanced,
    Failed {
        class: FailureClass,
        manual_recon: bool,
    },
    /// **M3b W12 Commit 2** — drain-stop verdict introduced for the
    /// W12 KVT2 confirmation flow.  Helper-heavy ownership in
    /// `services::offline_sync::kvt2_confirm::confirm_drain_doc`
    /// commits its envelope(s); caller (drain entry-point) maps the
    /// outcome to this verdict.  Drain loop stops at the held doc
    /// (no further docs this tick); pending-drain shifts do NOT
    /// escalate to Manual on HoldFnDrain (W0b state-unchanged
    /// contract); `projection` distinguishes durable doc state for
    /// summary accounting per [`HoldFnDrainProjection`].
    ///
    /// **5b.2 (2026-05-24)**: production-constructed by
    /// `process_via_lastchk_replay` (SentReplay HoldFnDrain
    /// projection mapping per plan §412); previously dead-coded for
    /// 5b.1 foundation phase.  Tests in `w12_control_surface_tests`
    /// also construct the variant directly to lock the projection +
    /// summary + finalize contracts in isolation.
    ///
    /// **Commit 6.1.2 (2026-05-24) — REC-1 Tier wiring**: `consecutive_
    /// holds` field plumbed from kvt2_confirm's
    /// `ConfirmDrainOutcome::HoldFnDrain.consecutive_holds`.  Used by
    /// drain orchestrator для Tier 1 (>= 10 → `KVT2_CONFIRM_PROLONGED_
    /// HOLD` audit Warning) + Tier 2 (>= 50 → STOP_MODE CAS +
    /// `OFFLINE_DRAIN_FN_STOP_MODE` audit Critical) trigger checks.
    /// 0 для `ErRedriveQueued` projection (counter reset by Envelope
    /// 1c-post atomically з Sent→ER advance).
    HoldFnDrain {
        class: FailureClass,
        projection: HoldFnDrainProjection,
        consecutive_holds: i64,
    },
    /// **SEAM-B-3 (architect-locked contract, 2026-06-13)** — the
    /// SentReplay-exclusive superseded outcome
    /// (`ConfirmDrainOutcome::SupersededHeld`).  A SENT doc that is NOT the
    /// FN's newest submitted doc: its `last_chk` Mismatch is non-fatal (a
    /// newer submitted doc became the tip).  The doc's ACK status is UNKNOWN
    /// from `last_chk` (acked-then-superseded OR never acked), so it is
    /// HELD in SENT (no state change; NOT concluded; `confirm_drain_doc`
    /// already emitted the `TIP_SUPERSEDED` audit + completed the recovery
    /// trace).
    ///
    /// **M2-N1 ruling B (2026-06-13) — HALT + escalate Manual (REVERSES
    /// SEAM-B-3's sibling-continue).**  Under strict-sequential drain a
    /// superseded predecessor is non-ACK from the successor's chain view, and
    /// it is non-self-resolving without B1-v2 (re-superseded every tick →
    /// would wedge if merely held).  So the drain loop records it as
    /// held-at-sent and escalates the FN to `RequiresManualReconciliation`
    /// (operator surface), then returns — it does NOT continue past it (the
    /// successor must not be sent off an unconfirmed predecessor).  No tier
    /// (REC-1) accounting: superseded is NOT a transient retry-hold.
    SupersededHeld,
}

/// **M3b W12 Commit 2** — projection for [`DocVerdict::HoldFnDrain`]
/// that separates drain-stop CONTROL (shared) from durable-state
/// ACCOUNTING (per-context).
///
/// Caller projects per `Kvt2ConfirmSource` (see plan
/// §"Source-context routing matrix"):
/// - `Kvt2ConfirmSource::SentFresh` Hold → `HeldAtSent` (pre-Envelope-1a
///   commit; doc state still `Sent`).
/// - `Kvt2ConfirmSource::SentReplay` Hold → `HeldAtSent` (doc state
///   stays `Sent` after 1c-hold trace.complete-no-state-change).
/// - `Kvt2ConfirmSource::Kvt1Reentry` Hold → `HeldAtKvt1` (doc state
///   stays `Kvt1`).
/// - `Kvt2ConfirmOutcome::SentNotFoundDowngrade` → `ErRedriveQueued`
///   (durable state advanced to `ErrorRetryable` via Envelope 1c-post;
///   awaiting next-tick W9b ER class-guard bounded redrive).
///
/// Routes to projection-specific [`DrainSummary`] recording method
/// (`record_doc_held_at_kvt1` / `record_doc_held_at_sent` /
/// `record_doc_er_redrive_queued`) which feeds the matching
/// [`NotEligibleReason`] when `finalize_eligibility` is computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldFnDrainProjection {
    /// Doc state stays `Kvt1` (Kvt1 re-entry Hold).
    HeldAtKvt1,
    /// Doc state stays `Sent` (SentFresh Hold pre-Envelope-1a OR
    /// SentReplay Hold post-1c-hold).
    HeldAtSent,
    /// Doc state advanced `Sent → ErrorRetryable` via Envelope 1c-post;
    /// next-tick ER cohort dispatch + bounded Pattern B redrive.
    /// Exclusive to `SentNotFoundDowngrade` outcome.
    ErRedriveQueued,
}

/// Map `RetryClass` → "is this a manual-recon-class outcome for
/// pending-drain halt purposes?" per spec §6.2 + §6.3 + §3.5.
///
/// True for TerminalReject / FnConfigError / WrapperBug / MacRecovery
/// / OperatorEscalation; false for TransientRetry / ProbeRequired
/// (operator-confirmed: transient transport / probe-required cases
/// retain retry budget — escalating to Manual would contradict the
/// "Manual is last resort" gravity rule).
fn is_manual_recon_retry_class(retry: RetryClass) -> bool {
    match retry {
        RetryClass::TerminalReject
        | RetryClass::FnConfigError
        | RetryClass::WrapperBug
        | RetryClass::MacRecovery
        | RetryClass::OperatorEscalation => true,
        RetryClass::TransientRetry | RetryClass::ProbeRequired => false,
    }
}

/// Dispatch one doc by its persisted `state` (spec amendment
/// 2026-05-21 cohort dispatch contract; post MED-C5-4 KVT2 deferral;
/// M3b W9b ER-class-guard 2026-05-22 ER split):
/// - `OFFLINE_LOCAL_ACK` → wire send via [`process_via_stage_send`]
///   (W9a 4-pre source whitelist).  No retry_class history is expected
///   on this branch: the doc has been offline-acked but never sent to
///   DPS, so no transport_trace row exists.
/// - `ERROR_RETRYABLE` → [`process_via_er_class_guard`] reads the
///   durable last-attempt `retry_class` and applies the
///   redrive-vs-escalate policy shared with
///   [`crate::services::reconciliation::boot_phase::dispatch_error_retryable_by_class`]
///   (M3b W9b HIGH-M3B-01 fix).  Non-`TransientRetry` classes do NOT
///   reach [`stage_send::run`] — re-driving would violate the
///   `stage_send.rs:18` caller obligation table.
/// - `SENT` → lastChk pre-flight via `process_via_lastchk_replay`
///   (closes I4 restart safety per spec §6).  No wire fall-through:
///   Mismatch / Decode / Unexpected route to manual-recon failure;
///   NotFound downgrades to `ErrorRetryable` for next-tick Pattern B
///   re-drive (HIGH-C5-3); TransportRetry keeps the doc SENT for
///   the next tick to re-probe.
/// - `KVT1` → `process_via_w12_only` (no wire; reads persisted
///   `doc.server_fiscal_no` + invokes
///   `kvt2_confirm::confirm_drain_doc(Kvt1Reentry, ...)` → Envelope
///   1b (2-CAS Kvt1Raw + Kvt1→Kvt2 + audit) + Envelope 2 on Acked
///   per M3b W12 Commit 5).
/// - `KVT2` → **M3b W12 Commit 3** `process_via_w12_kvt2_advance`
///   (calls `stage_finalize::run` for idempotent Kvt2→Ack advance;
///   reverses MED-C5-4 deferral).  Surfaces mid-tick crash recovery
///   between Envelope 1 (W12 Kvt1→Kvt2 advance) and Envelope 2
///   (`stage_finalize::run` Kvt2→Ack).
/// - Other states → `BootError::Internal` (cohort walker SELECT
///   filter breach).
///
/// Each branch appends to [`DrainSummary`] + emits exactly one audit
/// row (`OFFLINE_DRAIN_DOC_ADVANCED` / `_DOC_FAILED`).  Only
/// infrastructure failures propagate (audit append sqlx error,
/// post-stage_send Sent→Kvt1 CAS non-Applied, node_state shift_state
/// mirror UPDATE drift during pending-drain escalation).
async fn process_one_doc(
    pool: &SqlitePool,
    deps: &RuntimeView<'_>,
    fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
) -> Result<DocVerdict, BootError> {
    match doc.state {
        DocState::OfflineLocalAck => {
            process_via_stage_send(pool, deps, fiscal_number, doc, summary).await
        }
        DocState::ErrorRetryable => {
            process_via_er_class_guard(pool, deps, fiscal_number, doc, summary).await
        }
        DocState::Sent => process_via_lastchk_replay(pool, deps, fiscal_number, doc, summary).await,
        DocState::Kvt1 => process_via_w12_only(pool, fiscal_number, doc, summary, deps).await,
        DocState::Kvt2 => process_via_w12_kvt2_advance(pool, fiscal_number, doc, summary).await,
        other => Err(BootError::Internal(format!(
            "backlog_drain({fiscal_number}): cohort walker returned unexpected \
             doc.state {state} for doc {hex} (SELECT must filter to drain \
             candidates: OFFLINE_LOCAL_ACK | SENT | KVT1 | KVT2 | ERROR_RETRYABLE \
             post M3b W12 Commit 3 KVT2 cohort widening)",
            state = other.as_str(),
            hex = hex_lower(doc.document_id.as_bytes()),
        ))),
    }
}

/// Process a doc in `OFFLINE_LOCAL_ACK` or `ERROR_RETRYABLE` state
/// through `stage_send::run` (W9a 4-pre source whitelist accepts
/// both).  On `Sent`, inline-advance Sent → Kvt1 via the C5 W12
/// stub helper.  Per-doc failures surface via
/// [`DocVerdict::Failed`] (no `?` propagation).
async fn process_via_stage_send(
    pool: &SqlitePool,
    deps: &RuntimeView<'_>,
    // M3b W12 Commit 4b.3 (2026-05-22): `fiscal_number` no longer
    // used directly — Sent branch now delegates to
    // `kvt2_confirm::confirm_drain_doc` which sources FN internally
    // from `doc.fiscal_number`.  Param kept on signature for
    // symmetry with sibling dispatch helpers
    // (`process_via_lastchk_replay`, `process_via_w12_only`) and
    // for future Commit 5/5b consumers.
    _fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
) -> Result<DocVerdict, BootError> {
    let outcome = stage_send::run(pool, deps.dps, doc.document_id, Some(deps.signing_ctx)).await;
    let id_hex = hex_lower(doc.document_id.as_bytes());

    match outcome {
        Ok(StageSendOutcome::Sent {
            server_fiscal_no,
            attempt_no,
        }) => {
            // **M3b W12 Commit 4b.3 (2026-05-22)** — Sent-source W12
            // wiring per plan §410.  stage_send committed doc to Sent
            // inside its own 4-b envelope; we now ask DPS via canonical
            // `last_chk` / `by_server_fiscal_no` whether the receipt
            // is recognized, then on Acked converge Sent→Kvt1→Kvt2→Ack
            // via Envelope 1a (atomic) + Envelope 2 (`stage_finalize::
            // run` Kvt2→Ack).
            //
            // **MED-PR70-R11-01 handoff**: `expected_server_fiscal_no`
            // sourced from `StageSendOutcome::Sent { server_fiscal_no
            // }` (just stamped this tick by stage_send 4-b), NOT from
            // the pre-stage_send cohort snapshot `doc.server_fiscal_no`
            // (which is `None` at SELECT time per stage_send 4-b
            // invariant).
            //
            // **SentFresh source-context routing** (helper-internal):
            //   - Acked → Envelope 1a + Envelope 2 → Advanced.
            //   - StructuralDrift (NotFound/Mismatch) → Envelope
            //     1c-drift-light audit + BootError::Internal per plan
            //     §410.
            //   - Hold (Transport/Server/Auth/Decode/empty-data_sign)
            //     → Envelope 1c-hold-light audit + BootError::Internal
            //     ("Commit 6 will project to HoldFnDrain").
            let confirm_outcome = kvt2_confirm::confirm_drain_doc(
                pool,
                deps.dps,
                doc,
                &server_fiscal_no,
                deps.fn_sign,
                kvt2_confirm::Kvt2ConfirmSource::SentFresh,
                Some(attempt_no.into()),
            )
            .await?;
            match confirm_outcome {
                kvt2_confirm::ConfirmDrainOutcome::Advanced => {
                    summary.record_doc_advanced(
                        &W12ConfirmOutcome::Acked {
                            server_fiscal_no: server_fiscal_no.clone(),
                        },
                        /* via_lastchk_replay */ false,
                    );
                    Ok(DocVerdict::Advanced)
                }
                kvt2_confirm::ConfirmDrainOutcome::HoldFnDrain {
                    projection,
                    consecutive_holds,
                    class,
                } => Ok(DocVerdict::HoldFnDrain {
                    class,
                    projection,
                    consecutive_holds,
                }),
                kvt2_confirm::ConfirmDrainOutcome::SupersededHeld => {
                    // **SEAM-B-3 (2026-06-13)**: structurally unreachable.
                    // This is the SentFresh confirm path (doc just sent
                    // this tick); the superseded exception is SentReplay-
                    // exclusive (superseded=false here), so confirm_drain_doc
                    // cannot return SupersededHeld.  Fail-loud on regression.
                    Err(BootError::Internal(format!(
                        "process_via_stage_send({id_hex}): SupersededHeld is \
                         SentReplay-exclusive; SentFresh cannot produce it \
                         (kvt2_confirm routing regression)"
                    )))
                }
            }
        }
        Ok(StageSendOutcome::Routed {
            decision,
            attempt_no,
            wire_status_code,
            wire_error_message,
        }) => {
            let class = failure_class_for_retry(decision.retry_class);
            let manual_recon = is_manual_recon_retry_class(decision.retry_class);
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "retry_class": decision.retry_class.as_str(),
                "target_state": decision.target_state.as_str(),
                "attempt_no": attempt_no,
                "wire_status_code": wire_status_code,
                "wire_error_message": wire_error_message,
                "manual_recon_class": manual_recon,
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon,
            })
        }
        Ok(StageSendOutcome::StateConflict { observed }) => {
            let class = FailureClass::StateConflict;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "observed_state": observed.as_str(),
                "manual_recon_class": true,
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        Ok(StageSendOutcome::DocumentMissing) => {
            let class = FailureClass::NotFound;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "manual_recon_class": true,
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        Ok(StageSendOutcome::SignerRefused(mismatch)) => {
            let class = FailureClass::SignerRefused;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "mismatch_detail": mismatch.to_string(),
                "manual_recon_class": true,
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        Err(send_err) => {
            let class = failure_class_for_send_err(&send_err);
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "send_error_detail": send_err.to_string(),
                "manual_recon_class": true,
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
    }
}

/// M3b W9b ER-class-guard (HIGH-M3B-01 fix, 2026-05-22).
///
/// Process a doc in `ERROR_RETRYABLE` state through the shared
/// redrive-vs-escalate policy
/// ([`crate::services::reconciliation::er_redrive_policy::evaluate_er_redrive`]).
/// The policy gates wire re-drive on the durable last-attempt
/// `retry_class` + `MAX_BOOT_ATTEMPTS` budget; only `TransientRetry`
/// under budget is allowed to re-enter [`stage_send::run`] (matching
/// the `stage_send.rs:18` caller obligation).  Other classes either:
///   - escalate to `RequiresManualReconciliation` (manual-recon — halts
///     pending-drain shifts via the outer loop's escalation ladder);
///   - hold in `ERROR_RETRYABLE` (ProbeRequired / Indeterminate —
///     sibling-continue, retains retry budget for a future tick where
///     evidence may resolve).
///
/// Each branch:
///   - escalations CAS `ErrorRetryable → RequiresManualReconciliation` and
///     emit `OFFLINE_DRAIN_ER_ESCALATED_TO_MANUAL` audit inside ONE
///     `with_immediate` envelope (atomic), then emit the standard
///     per-doc `OFFLINE_DRAIN_DOC_FAILED` audit afterwards;
///   - holds emit only the `OFFLINE_DRAIN_DOC_FAILED` audit (no CAS).
///
/// Both paths return `DocVerdict::Failed`; the `manual_recon` flag
/// drives the outer pending-drain halt decision.
async fn process_via_er_class_guard(
    pool: &SqlitePool,
    deps: &RuntimeView<'_>,
    fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
) -> Result<DocVerdict, BootError> {
    use crate::services::reconciliation::boot_phase::MAX_BOOT_ATTEMPTS;
    use crate::services::reconciliation::er_redrive_policy::{
        evaluate_er_redrive, ErRedriveDecision,
    };

    let id_hex = hex_lower(doc.document_id.as_bytes());
    let decision = evaluate_er_redrive(pool, doc.document_id)
        .await
        .map_err(BootError::Database)?;

    match decision {
        ErRedriveDecision::Redrive => {
            // TransientRetry + attempts < MAX_BOOT_ATTEMPTS — Pattern B
            // retry path.  Reuse the OfflineLocalAck wire-send branch
            // verbatim: stage_send::run handles the 4-pre CAS
            // `ErrorRetryable → Sending` per W7 / W9a freeze §4.2.
            process_via_stage_send(pool, deps, fiscal_number, doc, summary).await
        }
        ErRedriveDecision::BudgetExhausted { attempts_used } => {
            cas_er_to_manual_via_drain(
                pool,
                fiscal_number,
                doc.document_id,
                RetryClass::TransientRetry.as_str(),
                Severity::Error,
                "budget_exhausted",
            )
            .await?;
            let class = FailureClass::BudgetExhausted;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "retry_class": RetryClass::TransientRetry.as_str(),
                "attempts_used": attempts_used,
                "max_boot_attempts": MAX_BOOT_ATTEMPTS,
                "manual_recon_class": true,
                "dispatch_via": "er_class_guard",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        ErRedriveDecision::EscalateManual { class: rc } => {
            cas_er_to_manual_via_drain(
                pool,
                fiscal_number,
                doc.document_id,
                rc.as_str(),
                Severity::Error,
                "non_retryable_class",
            )
            .await?;
            let class = failure_class_for_retry(rc);
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "retry_class": rc.as_str(),
                "manual_recon_class": true,
                "dispatch_via": "er_class_guard",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        ErRedriveDecision::EscalateInconsistent { class: rc } => {
            // TerminalReject + ER = structural inconsistency: routing
            // module lands TerminalReject directly in `Rejected`, never
            // in `ErrorRetryable`.  CRITICAL severity audit.
            cas_er_to_manual_via_drain(
                pool,
                fiscal_number,
                doc.document_id,
                rc.as_str(),
                Severity::Critical,
                "terminal_reject_inconsistent",
            )
            .await?;
            let class = failure_class_for_retry(rc);
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "retry_class": rc.as_str(),
                "manual_recon_class": true,
                "structural_inconsistency": true,
                "dispatch_via": "er_class_guard",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        ErRedriveDecision::HoldProbeRequired => {
            // No CAS — doc stays in ER; sibling-continue.
            // `manual_recon: false` preserves the per-spec §3.5 gravity
            // rule (probe-required is not "last resort manual").
            let class = FailureClass::WireRoutingProbeRequired;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "retry_class": RetryClass::ProbeRequired.as_str(),
                "manual_recon_class": false,
                "hold_reason": "probe_required",
                "dispatch_via": "er_class_guard",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: false,
            })
        }
        ErRedriveDecision::HoldIndeterminate => {
            // No CAS — durable retry_class evidence missing; mirror boot
            // semantics (Severity::Error audit at the boot helper).  Drain
            // surfaces the same forensic state via `OFFLINE_DRAIN_DOC_FAILED`
            // + non-manual-recon flag (operator-pinned 2026-05-22 scope).
            let class = FailureClass::RetryClassIndeterminate;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "retry_class": serde_json::Value::Null,
                "manual_recon_class": false,
                "hold_reason": "retry_class_indeterminate",
                "dispatch_via": "er_class_guard",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: false,
            })
        }
    }
}

/// M3b W9b ER-class-guard helper — CAS `ErrorRetryable →
/// RequiresManualReconciliation` + audit `OFFLINE_DRAIN_ER_ESCALATED_TO_MANUAL`
/// inside ONE `with_immediate` envelope (atomic per I8: state machine
/// CAS and audit row never split across tx boundaries).
///
/// The whitelisted edge `(ErrorRetryable, RequiresManualReconciliation)`
/// is shared with the boot dispatcher (declared at
/// `fiscal_documents::allowed_transition` line 194).  This helper is
/// drain-flavored — uses `OFFLINE_DRAIN_*` audit event types instead of
/// boot's `BOOT_ER_*`.
///
/// Idempotent under tick replay: the CAS `WHERE state = 'ERROR_RETRYABLE'`
/// guard makes a second invocation produce `TransitionOutcome::Conflict`,
/// which is treated as a structural drift (re-entering an already-escalated
/// doc indicates a missed walker filter — surfaces as `BootError::Internal`).
async fn cas_er_to_manual_via_drain(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    retry_class_label: &str,
    severity: Severity,
    rationale: &'static str,
) -> Result<(), BootError> {
    let retry_class_owned = retry_class_label.to_string();
    let fiscal_number_owned = fiscal_number.to_string();
    let outcome = with_immediate(pool, move |tx| {
        Box::pin(async move {
            let outcome = fiscal_documents::transition_state(
                tx,
                doc_id,
                DocState::ErrorRetryable,
                DocState::RequiresManualReconciliation,
            )
            .await?;
            if matches!(outcome, TransitionOutcome::Applied) {
                let payload = serde_json::json!({
                    "fiscal_number": fiscal_number_owned,
                    "document_id": hex_lower(doc_id.as_bytes()),
                    "retry_class": retry_class_owned,
                    "rationale": rationale,
                    "dispatch_via": "er_class_guard",
                });
                audit_log::append_tx(
                    tx,
                    AUDIT_ENTITY_DOC,
                    &hex_lower(doc_id.as_bytes()),
                    "OFFLINE_DRAIN_ER_ESCALATED_TO_MANUAL",
                    severity,
                    None,
                    Some(&payload.to_string()),
                )
                .await?;
            }
            Ok::<TransitionOutcome, anyhow::Error>(outcome)
        })
    })
    .await
    .map_err(|source| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source,
    })?;

    if !matches!(outcome, TransitionOutcome::Applied) {
        return Err(BootError::Internal(format!(
            "backlog_drain({fiscal_number}): doc {doc_hex} CAS ErrorRetryable→RequiresManualReconciliation \
             produced {outcome} (App reconcile mutex should prevent races; non-Applied here \
             indicates structural drift — walker emitted a doc that is no longer in ER)",
            doc_hex = hex_lower(doc_id.as_bytes()),
            outcome = outcome_as_str(outcome),
        )));
    }
    Ok(())
}

/// Process a doc in `SENT` state via `lastChk` pre-flight (spec
/// §2.3 Step A — replay rediscovery).  Match → advance via the W12
/// stub (Sent→Kvt1) with audit `replay_short_circuit=true`.  Other
/// probe outcomes record per-doc failure (manual or non-manual per
/// outcome class).
///
/// **No wire fall-through**: a `SENT` doc has crossed the wire-send
/// threshold — re-driving via `stage_send::run` would double-fiscalize
/// (4-pre source whitelist would reject Sent → Sending and the
/// outcome would be StateConflict).  This helper records per-doc
/// failure on lastChk non-Match and leaves the doc in `SENT` for
/// the next drain tick.
///
/// **Server_fiscal_no NULL on SENT** is a structural invariant breach
/// (stage_send 4-b stamps both atomically with the CAS Sending→Sent);
/// surfaces as a per-doc Internal-class failure for forensic audit.
/// **M3b W12 Commit 5b.2 (plan §412 production wiring, 2026-05-24)**
/// — replaces the pre-W12 `last_chk_probe::probe` → `ProbeOutcome` →
/// `apply_w12_confirmation`/`downgrade_sent_to_error_retryable_for_retry`
/// dispatch with the canonical `confirm_drain_doc(SentReplay)` W12
/// chain.  All wire-call semantics now flow through the W12 routing
/// matrix (`classify_check_result` + `evaluate_lastchk`).
///
/// **Behavioral pivot vs pre-W12** (operator runbook entry — significant
/// audit semantics shift; see plan §412 MED-1):
/// - **Match + non-empty data_sign**: pre-W12 stopped at `Kvt1` (stub
///   `DeferredKvt1`); now advances all the way through
///   `Sent → Kvt1 → Kvt2 → Ack` atomically via Envelope 1a-replay
///   (5-write bundled) + Envelope 2 (`stage_finalize::run`).
/// - **Match + empty data_sign**: pre-W12 marked
///   `failure_class=Internal, manual_recon=true` (manual recon required);
///   now classified `Hold(LastChkDataSignEmpty)` → `HoldFnDrain` with
///   transient retry (next tick re-probes).  Operator dashboards
///   transition from "manual recon needed" to "KVT2_CONFIRM_HOLD" audit.
/// - **NotFound**: pre-W12 downgraded `Sent → ErrorRetryable` + per-doc
///   `OFFLINE_DRAIN_DOC_FAILED` (Transport, manual_recon=false); W12
///   bundles the downgrade with `transport_trace.complete_tx
///   TransientRetry` + `OFFLINE_DRAIN_DOC_FAILED` in ONE envelope
///   (1c-post atomic), returns `HoldFnDrain { ErRedriveQueued }` which
///   halts current FN drain pending next-tick W9b ER-class-guard
///   Pattern B redrive via `stage_send::run`.
/// - **Mismatch**: pre-W12 per-doc `failure_class=Internal,
///   manual_recon=true` (drain continued); W12 emits
///   `KVT2_CONFIRM_STRUCTURAL_DRIFT` (Severity::Error) via bundled
///   Envelope 1c-drift, then `BootError::Internal` halts the FN drain.
///   Mismatch is now treated as state-machine drift (drain-halting)
///   instead of per-doc failure.
/// - **TransportRetry / DecodeEscalate / Unexpected**: pre-W12 per-doc
///   failure (drain continued); W12 maps to `Hold(DpsTransport |
///   DpsDecode | DpsServer | DpsAuthorization)` → bundled Envelope
///   1c-hold + `HoldFnDrain { HeldAtSent }` (doc stays in Sent;
///   drain halts; next tick re-probes via SentReplay).
///
/// **Invariants**:
/// - I1: DPS call (`evaluate_lastchk`) sits OUTSIDE any `with_immediate`
///   (1c-pre commits + releases tx BEFORE wire; outcome envelopes open
///   NEW tx after wire returns).
/// - I4: Acked path is 5-write atomic; crash between `transport_trace.
///   complete_tx` and `Sent→Kvt1` CAS is single-tx undone; post-tx crash
///   before stage_finalize recovers via `stage_finalize::run AlreadyAcked`.
/// - I8: bundled trace.complete + outcome audit row per plan §412 (every
///   outcome path emits both forensic markers atomically).
async fn process_via_lastchk_replay(
    pool: &SqlitePool,
    deps: &RuntimeView<'_>,
    fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
) -> Result<DocVerdict, BootError> {
    let id_hex = hex_lower(doc.document_id.as_bytes());
    // Source canonical expected_server_fiscal_no per MED-PR70-R11-01 —
    // SENT cohort row was stage_send-stamped (4-b invariant); None
    // here is state-machine breach.  Emit drift envelope + halt
    // (mirrors `process_via_w12_only` Kvt1Reentry pattern from
    // Commit 5 Δ MED-W12C5-01 fix).
    let expected_server_fiscal_no = match doc.server_fiscal_no.as_deref() {
        Some(s) => s,
        None => {
            kvt2_confirm::commit_drift_envelope_1c_drift_light(
                pool,
                fiscal_number,
                &id_hex,
                kvt2_confirm::Kvt2ConfirmSource::SentReplay,
                &kvt2_confirm::Kvt2ConfirmStructuralReason::ServerFiscalNoMissing,
            )
            .await?;
            return Err(BootError::Internal(format!(
                "process_via_lastchk_replay({fn_id}): doc {id_hex} at state Sent has \
                 NULL server_fiscal_no — stage_send 4-b stamp invariant breach \
                 (Sent cohort row ALWAYS implies stage_send stamped it).  \
                 KVT2_CONFIRM_STRUCTURAL_DRIFT audit emitted prior to halt.",
                fn_id = fiscal_number,
            )));
        }
    };
    let confirm_outcome = kvt2_confirm::confirm_drain_doc(
        pool,
        deps.dps,
        doc,
        expected_server_fiscal_no,
        deps.fn_sign,
        kvt2_confirm::Kvt2ConfirmSource::SentReplay,
        // attempt_no: None — SentReplay's trace attempt_no lives in
        // `transport_trace` (allocated by Envelope 1c-pre INSIDE
        // confirm_drain_doc), NOT on confirm_drain_doc's audit-
        // payload surface.
        None,
    )
    .await?;
    match confirm_outcome {
        kvt2_confirm::ConfirmDrainOutcome::Advanced => {
            summary.record_doc_advanced(
                &W12ConfirmOutcome::Acked {
                    server_fiscal_no: expected_server_fiscal_no.to_string(),
                },
                /* via_lastchk_replay */ true,
            );
            Ok(DocVerdict::Advanced)
        }
        kvt2_confirm::ConfirmDrainOutcome::HoldFnDrain {
            projection,
            consecutive_holds,
            class,
        } => {
            // SentReplay HoldFnDrain projection maps directly to
            // DocVerdict::HoldFnDrain — drain orchestrator halts on
            // this doc this tick; next tick re-evaluates via SentReplay
            // (HeldAtSent: doc stays Sent, next tick re-probes) OR
            // via W9b ER cohort (ErRedriveQueued: doc advanced to ER
            // by 1c-post, next tick re-sends via stage_send).
            // **6.1.2**: consecutive_holds plumbed для Tier 1/2 triggers.
            // **6.2 (REC-6)**: class plumbed from kvt2_confirm — granular
            // per-Hold-reason mapping (Transport/Server/Authorization/
            // Decode/Internal/NotFound) replaces hardcoded Transport.
            Ok(DocVerdict::HoldFnDrain {
                class,
                projection,
                consecutive_holds,
            })
        }
        kvt2_confirm::ConfirmDrainOutcome::SupersededHeld => {
            // **M2-N1 ruling B (2026-06-13)**: the probed SENT doc is superseded
            // by a newer submitted doc (now the tip); its ACK status is UNKNOWN
            // from lastChk.  `confirm_drain_doc` already completed the recovery
            // trace + emitted the TIP_SUPERSEDED audit and left the doc in SENT.
            // Map to the SupersededHeld verdict — per strict-sequential the
            // drain loop HALTS the chain and escalates the FN to Manual
            // (reverses SEAM-B-3's sibling-continue): a superseded predecessor
            // is non-ACK + non-self-resolving, so continuing would send a
            // successor off an unconfirmed predecessor / wedge.
            Ok(DocVerdict::SupersededHeld)
        }
    }
}

/// Process a doc in `KVT1` state via the W12 Kvt1Reentry chain
/// (`confirm_drain_doc(Kvt1Reentry, ...)`).  The doc was previously
/// advanced to `Kvt1` by a prior tick (boot recovery OR prior-tick
/// W12 Hold) and is now eligible for the `lastChk` evidence path
/// via the persisted `server_fiscal_no`.
///
/// **M3b W12 Commit 5 (plan §411, 2026-05-22)**: replaced pre-W12
/// `apply_w12_confirmation` stub call with full W12 chain:
/// `confirm_drain_doc(Kvt1Reentry, ...)` → Envelope 1b (Kvt1Raw +
/// Kvt1→Kvt2 + audit) → Envelope 2 (`stage_finalize::run` Kvt2→Ack)
/// on Acked.  NotFound/Mismatch surface as `StructuralDrift` →
/// `BootError::Internal` per plan §410.  Hold path emits
/// `KVT2_CONFIRM_HOLD` audit + returns BootError (HoldFnDrain
/// projection deferred to Commit 6 per plan §413).
///
/// **MED-PR70-R11-01 handoff**: `expected_server_fiscal_no` sourced
/// from persisted `doc.server_fiscal_no` (Kvt1 state guarantees
/// stamp present per stage_send 4-b invariant) with explicit
/// `BootError::Internal` fail-loud on None — Kvt1 without
/// `server_fiscal_no` is a state-machine invariant breach (cohort
/// walker should never have surfaced it).
async fn process_via_w12_only(
    pool: &SqlitePool,
    _fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
    deps: &RuntimeView<'_>,
) -> Result<DocVerdict, BootError> {
    let id_hex = hex_lower(doc.document_id.as_bytes());
    // Source the canonical expected_server_fiscal_no from persisted
    // doc row — Kvt1 state guarantees stage_send 4-b stamped it.
    // **MED-W12C5-01 fix (5 Δ, 2026-05-22)**: emit durable
    // KVT2_CONFIRM_STRUCTURAL_DRIFT audit (Severity::Error) via
    // Envelope 1c-drift-light BEFORE the BootError::Internal halt
    // so forensic operators see the structural breach in audit_log
    // (not just the returned error string).  Doc state untouched.
    let expected_server_fiscal_no = match doc.server_fiscal_no.as_deref() {
        Some(s) => s,
        None => {
            kvt2_confirm::commit_drift_envelope_1c_drift_light(
                pool,
                &doc.fiscal_number,
                &id_hex,
                kvt2_confirm::Kvt2ConfirmSource::Kvt1Reentry,
                &kvt2_confirm::Kvt2ConfirmStructuralReason::ServerFiscalNoMissing,
            )
            .await?;
            return Err(BootError::Internal(format!(
                "process_via_w12_only({fn_id}): doc {id_hex} at state Kvt1 has \
                 NULL server_fiscal_no — stage_send 4-b stamp invariant breach \
                 (Kvt1 ALWAYS implies server_fiscal_no stamped by prior tick).  \
                 KVT2_CONFIRM_STRUCTURAL_DRIFT audit emitted prior to halt.",
                fn_id = doc.fiscal_number,
            )));
        }
    };
    let confirm_outcome = kvt2_confirm::confirm_drain_doc(
        pool,
        deps.dps,
        doc,
        expected_server_fiscal_no,
        deps.fn_sign,
        kvt2_confirm::Kvt2ConfirmSource::Kvt1Reentry,
        // attempt_no: None — Kvt1Reentry has no fresh wire attempt
        // this tick (no stage_send invocation).
        None,
    )
    .await?;
    match confirm_outcome {
        kvt2_confirm::ConfirmDrainOutcome::Advanced => {
            summary.record_doc_advanced(
                &W12ConfirmOutcome::Acked {
                    server_fiscal_no: expected_server_fiscal_no.to_string(),
                },
                /* via_lastchk_replay */ false,
            );
            Ok(DocVerdict::Advanced)
        }
        kvt2_confirm::ConfirmDrainOutcome::HoldFnDrain {
            projection,
            consecutive_holds,
            class,
        } => Ok(DocVerdict::HoldFnDrain {
            class,
            projection,
            consecutive_holds,
        }),
        kvt2_confirm::ConfirmDrainOutcome::SupersededHeld => {
            // **AUD-L5-1 (2026-06-14)**: Kvt1Reentry is now superseded-capable
            // (kvt2_confirm fetch-gate widened) — a resting KVT1 head whose DPS
            // last_chk tip was superseded by a newer submitted doc.
            // confirm_drain_doc already emitted the light TIP_SUPERSEDED
            // (Warning) audit + left the doc at KVT1 (no CAS).  In the OFFLINE
            // drain a superseded head is non-self-resolving (a strict M2-01
            // chain successor chained off it, so a mere hold would re-supersede
            // every tick), so map it to the SupersededHeld verdict and let the
            // strict-sequential loop HALT + escalate the FN to Manual (ruling B,
            // mirroring the SentReplay consumer at the dispatch_sent_via_probe
            // site).  Distinct from the online-convergence tick, which has no
            // chain-head and HOLDS the same outcome (AUD-L5-1 EDIT-D).
            Ok(DocVerdict::SupersededHeld)
        }
    }
}

/// **M3b W12 Commit 3** — KVT2 cohort dispatch helper (reverses
/// MED-C5-4 deferral per plan §"Crash-recovery convergence" §19 +
/// §"Cohort widening" §14-15).
///
/// Invokes `stage_finalize::run(pool, doc_id)` for idempotent
/// `Kvt2 → Ack` advance.  Surfaces mid-tick crash recovery between
/// Envelope 1 (W12 Kvt1→Kvt2 advance, lands in Commits 4/5/5b) and
/// Envelope 2 (`stage_finalize::run` Kvt2→Ack).  M3a `AlreadyAcked`
/// contract means a doc already in `Ack` returns
/// `StageFinalizeOutcome::AlreadyAcked` (no-op success-shape;
/// concurrent finish-doc race or replay arrived after boot recovery
/// already finalized).
///
/// **Outcome routing** per plan §15:
/// - `Acked { fiscal_number, lnd }` → [`DocVerdict::Advanced`] +
///   summary `record_doc_advanced(W12ConfirmOutcome::Acked {
///   server_fiscal_no }, via_lastchk_replay=false)` +
///   `OFFLINE_DRAIN_DOC_ADVANCED` forensic audit
///   (`dispatch_via="w12_kvt2_recovery"`).
/// - `AlreadyAcked` → same Advanced+record as Acked (idempotent
///   replay; doc IS at Ack now).
/// - `StateConflict { observed }` → [`DocVerdict::Failed`] with
///   `FailureClass::StateConflict` + `manual_recon: true` (concurrent
///   writer past App reconcile mutex; system-level signal).
/// - `DocumentMissing` → [`DocVerdict::Failed`] with
///   `FailureClass::NotFound` + `manual_recon: true` (cohort race
///   with delete; should not happen in production but defensive).
/// - `Err(StageFinalizeError)` → propagate as
///   [`BootError::ReconciliationFailed`] (infrastructure failure).
///
/// **I1 preserved**: `stage_finalize::run` is pool-only and owns its
/// own `with_immediate` envelope per M3a W8 contract; this helper
/// adds no envelope around the call.  Forensic audits
/// (`OFFLINE_DRAIN_DOC_ADVANCED` / `_DOC_FAILED`) are pool-bound and
/// emitted AFTER stage_finalize commits.
async fn process_via_w12_kvt2_advance(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
) -> Result<DocVerdict, BootError> {
    use crate::services::write_path::stage_finalize::{
        self, StageFinalizeError, StageFinalizeOutcome,
    };

    let id_hex = hex_lower(doc.document_id.as_bytes());
    let outcome = match stage_finalize::run(pool, doc.document_id).await {
        Ok(o) => o,
        // M2-04 defense (2026-06-12): a ChainSeedMismatch must NOT abort
        // the whole FN drain tick and recur every cycle (the silent
        // fail-loop — doc stays KVT2, re-selected by the KVT2 cohort
        // filter). Route it to a manual-recon-class per-doc Failed so the
        // pending-drain loop escalates the shift to
        // RequiresManualReconciliation (spec §16.7) with an operator
        // surface. After the M2-01 fix this arm is unreachable for
        // offline-origin docs (finalize skips the guard); it is
        // defense-in-depth for any future online-chain drift.
        Err(StageFinalizeError::ChainSeedMismatch {
            expected, actual, ..
        }) => {
            let class = FailureClass::ChainSeedMismatch;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "dispatch_via": "w12_kvt2_recovery",
                "manual_recon_class": true,
                "expected_seed_hex": expected.map(|h| hex_lower(&h)),
                "actual_seed_hex": actual.map(|h| hex_lower(&h)),
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            return Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            });
        }
        Err(source) => {
            return Err(BootError::ReconciliationFailed {
                fiscal_number: fiscal_number.to_string(),
                source: anyhow::Error::new(source),
            });
        }
    };

    // Acked + AlreadyAcked share the "doc reached Ack" forensic
    // shape; pre-compute the success payload once, then dispatch on
    // outcome.  doc.server_fiscal_no is `Some(..)` per stage_send 4-b
    // invariant (doc in KVT2 state implies original Sent advance
    // stamped it); empty-string fallback avoids an extra error path
    // for the structurally-impossible None case.
    match outcome {
        StageFinalizeOutcome::Acked {
            fiscal_number: ack_fn,
            lnd,
        } => {
            let server_fiscal_no = doc.server_fiscal_no.clone().unwrap_or_default();
            let payload = serde_json::json!({
                "document_id": id_hex,
                "from_state": doc.state.as_str(),
                "to_state": DocState::Ack.as_str(),
                "replay_short_circuit": false,
                "w12_status": W12ConfirmOutcome::Acked {
                    server_fiscal_no: server_fiscal_no.clone(),
                }
                .w12_status_str(),
                "dispatch_via": "w12_kvt2_recovery",
                "stage_finalize_outcome": "Acked",
                "stage_finalize_lnd": lnd,
                "stage_finalize_fiscal_number": ack_fn,
            });
            audit_log::append(
                pool,
                AUDIT_ENTITY_DOC,
                &id_hex,
                "OFFLINE_DRAIN_DOC_ADVANCED",
                Severity::Info,
                None,
                Some(&payload.to_string()),
            )
            .await
            .map_err(BootError::Database)?;
            summary.record_doc_advanced(
                &W12ConfirmOutcome::Acked { server_fiscal_no },
                /* via_lastchk_replay */ false,
            );
            Ok(DocVerdict::Advanced)
        }
        StageFinalizeOutcome::AlreadyAcked => {
            let server_fiscal_no = doc.server_fiscal_no.clone().unwrap_or_default();
            let payload = serde_json::json!({
                "document_id": id_hex,
                "from_state": doc.state.as_str(),
                "to_state": DocState::Ack.as_str(),
                "replay_short_circuit": false,
                "w12_status": W12ConfirmOutcome::Acked {
                    server_fiscal_no: server_fiscal_no.clone(),
                }
                .w12_status_str(),
                "dispatch_via": "w12_kvt2_recovery",
                "stage_finalize_outcome": "AlreadyAcked",
            });
            audit_log::append(
                pool,
                AUDIT_ENTITY_DOC,
                &id_hex,
                "OFFLINE_DRAIN_DOC_ADVANCED",
                Severity::Info,
                None,
                Some(&payload.to_string()),
            )
            .await
            .map_err(BootError::Database)?;
            summary.record_doc_advanced(
                &W12ConfirmOutcome::Acked { server_fiscal_no },
                /* via_lastchk_replay */ false,
            );
            Ok(DocVerdict::Advanced)
        }
        StageFinalizeOutcome::StateConflict { observed } => {
            let class = FailureClass::StateConflict;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "observed_state": observed.as_str(),
                "dispatch_via": "w12_kvt2_recovery",
                "manual_recon_class": true,
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        StageFinalizeOutcome::DocumentMissing => {
            let class = FailureClass::NotFound;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "dispatch_via": "w12_kvt2_recovery",
                "manual_recon_class": true,
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
    }
}

/// Wire-form string for the four `TransitionOutcome` variants.
/// `as_str()` doesn't exist on `TransitionOutcome` (it's distinct
/// from the str_enum-derived `DocState`); local helper used inside
/// the closure to avoid Debug-format leak in audit-relevant errors.
fn outcome_as_str(outcome: TransitionOutcome) -> &'static str {
    match outcome {
        TransitionOutcome::Applied => "Applied",
        TransitionOutcome::Forbidden => "Forbidden",
        TransitionOutcome::Conflict => "Conflict",
        TransitionOutcome::NotFound => "NotFound",
    }
}

/// **REC-1 Phase 2a.1 Commit 6.1.2 (2026-05-24)** — Tier 1 prolonged-
/// hold audit emit (Severity::Warning).  No state mutation; pure
/// forensic signal that an FN doc reached >= 10 consecutive holds.
///
/// Operator dashboards can query rate
/// `KVT2_CONFIRM_PROLONGED_HOLD` events / hour / FN to detect
/// degrading-but-not-yet-stopped FN trends BEFORE Tier 2 fires.
/// Counter accumulated по Hold/Advance lifecycle persisted в
/// `fiscal_documents.consecutive_holds` (DDL 018, atomic increment
/// inside Envelope 1c-hold).
///
/// Pool-only single audit row (no envelope; no state).  Idempotent
/// per-tick — drain orchestrator only invokes on HoldFnDrain break
/// AND counter >= 10; next-tick re-evaluation re-fires if counter
/// still >= 10 (intended — operator sees prolonged-hold signal each
/// tick that doc stays held).
async fn trigger_tier_1_prolonged_hold(
    pool: &SqlitePool,
    doc_id: DocumentId,
    projection: HoldFnDrainProjection,
    consecutive_holds: i64,
) -> Result<(), BootError> {
    let id_hex = hex_lower(doc_id.as_bytes());
    let projection_str = match projection {
        HoldFnDrainProjection::HeldAtSent => "HeldAtSent",
        HoldFnDrainProjection::HeldAtKvt1 => "HeldAtKvt1",
        HoldFnDrainProjection::ErRedriveQueued => "ErRedriveQueued",
    };
    let payload = serde_json::json!({
        "document_id": id_hex,
        "projection": projection_str,
        "consecutive_holds": consecutive_holds,
        "tier": 1,
        "tier_threshold": 10,
    });
    audit_log::append(
        pool,
        AUDIT_ENTITY_DOC,
        &id_hex,
        "KVT2_CONFIRM_PROLONGED_HOLD",
        Severity::Warning,
        None,
        Some(&payload.to_string()),
    )
    .await
    .map_err(BootError::Database)?;
    Ok(())
}

/// **REC-1 Phase 2a.1 Commit 6.1.2 (2026-05-24)** — Tier 2 STOP_MODE
/// escalation.  When an FN doc accumulates >= 50 consecutive holds,
/// flip `node_state.mode` → `STOP_MODE` + emit Critical audit in ONE
/// `with_immediate` envelope.
///
/// **Effect**: new чек ingress на цю FN rejected at adapter layer
/// (existing STOP_MODE contract).  Existing held docs remain в Sent/Kvt1
/// з накопиченим counter, але вони **НЕ** auto-drained post-recovery:
/// `return_online_probe` explicitly SKIPS a STOP_MODE node
/// (`SkipReason::NodeNotOfflineOrTransition` — the probe only auto-flips
/// an `Offline` node to `GoingOnline`, never STOP_MODE/Blocked/etc.).
/// So nothing re-arms the W9 drain on its own.  Per operator memory
/// `feedback_manual_recon_catastrophe`: STOP_MODE is an intermediate tier
/// (NOT эскалація в Manual) that REQUIRES operator intervention — the
/// operator must drive the node back to return-online within the 36h
/// offline-cap window (до cert.NotAfter-2160min); only then does the W8
/// probe / W9 drain machinery resume and the held docs drain.
///
/// **Atomicity (I4)**: bundled з audit row.  If CAS fails (missing FN
/// row — structural breach), tx rolls back → no half-state.
///
/// Idempotent re-entry safe — repeated invocations re-CAS to STOP_MODE
/// (no-op if already там) + emit additional Critical audit (signals
/// continued degradation; operator dashboards aggregate per-hour).
async fn trigger_tier_2_stop_mode(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    consecutive_holds: i64,
) -> Result<(), BootError> {
    let id_hex = hex_lower(doc_id.as_bytes());
    let payload = serde_json::json!({
        "document_id": id_hex,
        "fiscal_number": fiscal_number,
        "consecutive_holds": consecutive_holds,
        "tier": 2,
        "tier_threshold": 50,
        "node_mode_target": "STOP_MODE",
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.clone();
    let fn_owned = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (a) CAS node_state.mode → STOP_MODE (idempotent UPDATE).
            let updated = node_state::set_mode_stop_mode_tx(tx, &fn_owned).await?;
            if !updated {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_owned}): Tier-2 STOP_MODE CAS produced \
                     rows_affected=0 for doc {doc_hex} — missing node_state \
                     row (structural breach)",
                    fn_owned = fn_owned,
                    doc_hex = id_hex_owned,
                ));
            }
            // (b) Critical audit row.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "OFFLINE_DRAIN_FN_STOP_MODE",
                Severity::Critical,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|err| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source: err,
    })?;
    Ok(())
}

/// W9b C4 manual-escalation seam (spec amendment 2026-05-21 +
/// `LEGAL_INVARIANTS.md` §INV-19 + spec §6.3).
///
/// In ONE `with_immediate` envelope:
///   1. CAS `shifts.state: {OpenedLocalPendingDrain |
///      ClosingLocalPendingDrain} → RequiresManualReconciliation`
///      (whitelisted edges 6 / 14).
///   2. UPDATE `node_state.shift_state` for the same FN to mirror the
///      new shifts row (m3b-shift-state-expansion.md §5 load-bearing
///      invariant: node_state.shift_state MUST equal the active shift
///      row's state).
///   3. Emit `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` Critical audit
///      with full forensic payload.
///
/// Caller (drain loop) returns the current summary IMMEDIATELY after —
/// subsequent backlog docs are NOT processed.
///
/// `current_shift_id` is read from the prereqs `NodeStateRow`; if
/// `None` (structural drift — pending-drain shift_state without a
/// current_shift_id), surfaces as `BootError::Internal`.
async fn escalate_drain_to_manual(
    pool: &SqlitePool,
    fiscal_number: &str,
    ns: &crate::db::repositories::node_state::NodeStateRow,
    failed_doc_id: DocumentId,
    failure_class: &str,
    halt_position: usize,
) -> Result<(), BootError> {
    let shift_id: ShiftId = ns.current_shift_id.ok_or_else(|| {
        BootError::Internal(format!(
            "backlog_drain({fiscal_number}): drain-reject escalation on shift_state={state} \
             but node_state.current_shift_id is NULL — structural drift",
            state = ns.shift_state.as_str(),
        ))
    })?;
    let from_state = ns.shift_state;
    let to_state = ShiftState::RequiresManualReconciliation;

    let fiscal_number_owned = fiscal_number.to_string();
    let failure_class_owned = failure_class.to_string();
    let outcome = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // RS-3 C1: shift CAS + node_state projection mirror via the
            // single transition-service (was: transition_state + the local
            // mirror_node_state_shift_state_tx pair).
            let outcome = shift_transition::apply_shift_transition(
                tx,
                &fiscal_number_owned,
                shift_id,
                from_state,
                to_state,
            )
            .await?;
            if let shifts::TransitionOutcome::Applied = outcome {
                let payload = serde_json::json!({
                    "fiscal_number": fiscal_number_owned,
                    "shift_id": hex_lower(shift_id.as_bytes()),
                    "document_id": hex_lower(failed_doc_id.as_bytes()),
                    "failure_class": failure_class_owned,
                    "current_shift_state": from_state.as_str(),
                    "halt_position": halt_position,
                });
                audit_log::append_tx(
                    tx,
                    "shift",
                    &hex_lower(shift_id.as_bytes()),
                    "OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL",
                    Severity::Critical,
                    None,
                    Some(&payload.to_string()),
                )
                .await?;
            }
            Ok::<shifts::TransitionOutcome, anyhow::Error>(outcome)
        })
    })
    .await
    .map_err(|source| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source,
    })?;

    if !matches!(outcome, shifts::TransitionOutcome::Applied) {
        return Err(BootError::Internal(format!(
            "backlog_drain({fiscal_number}): shift {shift_hex} CAS {from:?}→RequiresManualReconciliation \
             produced {outcome:?} (App reconcile mutex should prevent races)",
            shift_hex = hex_lower(shift_id.as_bytes()),
            from = from_state,
        )));
    }
    Ok(())
}

// RS-3 C1: the `node_state.shift_state` mirror (m3b §5 load-bearing
// invariant) moved into `services::shift::transition::apply_shift_transition`
// — the single transition-service now owns the shift CAS + projection
// mirror pairing for both this drain path and boot reconciliation.

/// W9b C6 finalization branch (spec §2.4 + amendment 2026-05-21).
///
/// Evaluates [`DrainSummary::finalize_eligibility`] and routes:
///
/// - `Eligible` → CAS `node_state.mode: GoingOnline → Online` +
///   CAS `offline_session: Draining → Closed` +
///   `OFFLINE_SESSION_CLOSED` audit + `OFFLINE_DRAIN_COMPLETED`
///   audit, all in ONE `with_immediate` envelope.  Both CAS guards
///   use the from-state value so a concurrent writer cannot smuggle
///   a different mode/state past finalize.  Non-`Applied` outcome
///   on either CAS rolls back the entire envelope and propagates
///   as `BootError::ReconciliationFailed` (single-writer invariant
///   breach per ADR-M3-A10).  `summary.mark_finalized()` runs
///   **after** the envelope commits (defensive double-check; the
///   in-memory mutation cannot rollback with the DB, so the
///   COMPLETED audit payload encodes `finalized=true` directly).
/// - `NotEligible{reason}` → emit `OFFLINE_DRAIN_PARTIAL` audit
///   (Warning) with the typed `reason` payload + summary
///   read-only accessors.  Node + session stay in their pre-drain
///   states (`GoingOnline` + `Draining`); next drain tick
///   re-evaluates.
///
/// Pre-W12 operator-pinned invariant: drain can finalize ONLY when
/// every doc returned `Acked { server_fiscal_no }` — pre-W12 stub
/// always returns `DeferredKvt1` so `advanced_to_kvt1 > 0` blocks
/// eligibility via `NotEligibleReason::DocsDeferredAtKvt1`.  The
/// Eligible arm is structurally unreachable until W12 PR plugs in
/// real lastChk evidence.
async fn finalize_drain(
    pool: &SqlitePool,
    fiscal_number: &str,
    session_id: OfflineSessionId,
    ns: &crate::db::repositories::node_state::NodeStateRow,
    summary: &mut DrainSummary,
) -> Result<(), BootError> {
    match summary.finalize_eligibility() {
        FinalizeEligibility::Eligible => {
            commit_finalize_envelope(
                pool,
                fiscal_number,
                session_id,
                ns,
                summary,
                FinalizeEntry::NormalEligible,
            )
            .await
        }
        FinalizeEligibility::NotEligible { reason } => {
            emit_partial(pool, fiscal_number, session_id, summary, &reason).await
        }
    }
}

/// **M3b W12 Commit 3 Δ** (MED-W12C3-01 fix, 2026-05-22) — entry-point
/// taxonomy for [`commit_finalize_envelope`].  Distinguishes the
/// per-tick "drain processed docs to Ack" path from the post-crash
/// "drain found session already completable" recovery path so audit
/// monitoring can tell them apart cleanly.
///
/// **Why distinct entries**: the recovery path runs with a fresh
/// [`DrainSummary`] (0 in-flight docs this tick) but commits the same
/// 5-write finalize envelope.  Operators reading the
/// `OFFLINE_DRAIN_COMPLETED` audit row stream would otherwise see
/// `backlog_size_before=0, advanced_to_ack=0` and have no signal that
/// THIS finalize was driven by post-crash recovery rather than a
/// genuinely empty drain pass.  The distinct event name
/// (`OFFLINE_DRAIN_RECOVERED_FINALIZE`) + `entry_reason` payload
/// field make the recovery case grep-able for forensic dashboards.
#[derive(Debug, Clone, Copy)]
enum FinalizeEntry {
    /// Per-tick drain processed docs to Ack and is finalizing as
    /// usual via `FinalizeEligibility::Eligible`.
    NormalEligible,
    /// MED-W12C3-01 crash-recovery entry: empty cohort + session in
    /// `Draining` + `is_session_drain_completable` proved all session
    /// docs already in `Ack`.  Prior drain tick committed
    /// `stage_finalize::run` Kvt2 → Ack durably but crashed before
    /// reaching `finalize_drain`.  This tick closes the session via
    /// the same 5-write envelope with distinct audit shape.
    CrashRecovery,
}

impl FinalizeEntry {
    fn audit_event_name(self) -> &'static str {
        match self {
            Self::NormalEligible => "OFFLINE_DRAIN_COMPLETED",
            Self::CrashRecovery => "OFFLINE_DRAIN_RECOVERED_FINALIZE",
        }
    }

    fn entry_reason_str(self) -> &'static str {
        match self {
            Self::NormalEligible => "normal_eligible",
            Self::CrashRecovery => "crash_recovery",
        }
    }
}

/// Eligible-arm helper: atomic node-mode CAS + session CAS +
/// shift CAS (pending-drain ladder closure) + node_state.shift_state
/// mirror UPDATE + `OFFLINE_SESSION_CLOSED` audit +
/// `OFFLINE_DRAIN_COMPLETED` audit in ONE `with_immediate` envelope.
///
/// **Shift transition (MED-W9B-1 fix, 2026-05-21)**: when the drain
/// finalizes successfully, the shift state must close the
/// pending-drain ladder per `m3b-shift-state-expansion.md` §§4.1 / 6.x:
///   - `OpenedLocalPendingDrain` → `Opened` (edge 5) when the
///     SHIFT_OPEN backlog Ack'd → shift is now operationally Opened.
///   - `ClosingLocalPendingDrain` → `Closed` (edge 13) when the
///     close-drain predicate is satisfied (drain Ack'd the Z_REPORT
///     AND all earlier shift docs).  Pre-W12 the predicate is
///     enforced via the C2 `finalize_eligibility` gate (Eligible
///     only when every drain doc returned Acked); W12 PR adds
///     explicit predicate verification post-lastChk.
///   - `Opened` (online finalize case) → no shift transition.
///   - Other states → BootError::Internal (structural drift; finalize
///     should not run on shift states outside `{Opened,
///     OpenedLocalPendingDrain, ClosingLocalPendingDrain}`).
///
/// `summary.mark_finalized()` runs **after** the envelope commits
/// (defensive double-check); the COMPLETED audit payload encodes
/// `finalized=true` directly to avoid the in-memory/audit-row drift
/// that would otherwise result from the post-envelope mutation
/// ordering.
async fn commit_finalize_envelope(
    pool: &SqlitePool,
    fiscal_number: &str,
    session_id: OfflineSessionId,
    ns: &crate::db::repositories::node_state::NodeStateRow,
    summary: &mut DrainSummary,
    entry: FinalizeEntry,
) -> Result<(), BootError> {
    let fiscal_number_owned = fiscal_number.to_string();
    let node_mode_from = ns.mode;
    let shift_state_from = ns.shift_state;
    let shift_id_opt = ns.current_shift_id;
    // MED-W9B-1 fix (2026-05-21): pre-compute pending-drain shift
    // transition.  Drain finalize closes the pending-drain ladder
    // per `m3b-shift-state-expansion.md` §4.1 + §16.4:
    //   - OpenedLocalPendingDrain → Opened (edge 5)
    //   - ClosingLocalPendingDrain → Closed (edge 13)
    //   - Opened → no-op (online finalize)
    //   - Other → Internal (structural drift; finalize shouldn't
    //     run on other shift states pre-W12).
    let shift_finalize_target: Option<ShiftState> = match shift_state_from {
        ShiftState::OpenedLocalPendingDrain => Some(ShiftState::Opened),
        ShiftState::ClosingLocalPendingDrain => Some(ShiftState::Closed),
        ShiftState::Opened => None,
        other => {
            return Err(BootError::Internal(format!(
                "backlog_drain({fiscal_number}): finalize Eligible on unexpected \
                 shift_state {state} — drain orchestrator expected \
                 {{Opened, OpenedLocalPendingDrain, ClosingLocalPendingDrain}}",
                state = other.as_str(),
            )));
        }
    };
    if shift_finalize_target.is_some() && shift_id_opt.is_none() {
        return Err(BootError::Internal(format!(
            "backlog_drain({fiscal_number}): pending-drain shift_state {state} but \
             node_state.current_shift_id is NULL — structural drift",
            state = shift_state_from.as_str(),
        )));
    }
    // MED-C6-1 fix (2026-05-21): override `finalized=true` in the
    // COMPLETED payload.  `summary.mark_finalized()` runs AFTER the
    // envelope commits, so without this override the audit row would
    // carry `finalized=false` (the summary's in-memory state at
    // build time).  Eligible arm intent is "we are finalizing"; if
    // the envelope rolls back, the audit row never lands either, so
    // the optimistic `true` is structurally honest.
    let mut payload = build_finalize_payload(summary, session_id, "COMPLETED", None);
    payload["finalized"] = serde_json::Value::Bool(true);
    // MED-W12C3-01 (2026-05-22): tag entry path so operators can grep
    // the post-crash recovery finalizes apart from per-tick finalizes
    // (both share the 5-write envelope; only audit shape differs).
    payload["entry_reason"] = serde_json::Value::String(entry.entry_reason_str().to_string());
    let payload_owned = payload.to_string();
    let audit_event_name = entry.audit_event_name();
    // MED-C6-2 fix (2026-05-21): W5 session-lifecycle audit shape for
    // the Draining → Closed transition (matches `OfflineSessionService::
    // close_session` convention).
    let session_id_hex = hex_lower(session_id.as_bytes());
    let session_closed_payload = serde_json::json!({
        "offline_session_id": session_id_hex,
        "from": OfflineSessionState::Draining.as_str(),
        "to": OfflineSessionState::Closed.as_str(),
        "reason_abort": serde_json::Value::Null,
    });
    let session_closed_payload_owned = session_closed_payload.to_string();
    let session_id_hex_owned = session_id_hex.clone();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (1) CAS node_state.mode GoingOnline → Online (W8 inverse).
            // Symmetric bind for both to-state and from-state (LOW-C6-2
            // 2026-05-21): str_enum drift would otherwise silently
            // skip the CAS if NodeMode::Online wire form ever changed.
            let mode_rows = sqlx::query(
                "UPDATE node_state SET mode = ? \
                 WHERE fiscal_number = ? AND mode = ?",
            )
            .bind(NodeMode::Online)
            .bind(&fiscal_number_owned)
            .bind(node_mode_from)
            .execute(&mut **tx)
            .await?
            .rows_affected();
            if mode_rows != 1 {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): finalize CAS node_state.mode \
                     {from} → ONLINE produced rows_affected={rows} (App reconcile \
                     mutex should prevent races)",
                    fn_id = fiscal_number_owned,
                    from = node_mode_from.as_str(),
                    rows = mode_rows,
                ));
            }
            // (2) CAS offline_session Draining → Closed via W5 helper.
            let session_outcome = offline_sessions::transition_state(
                tx,
                session_id,
                OfflineSessionState::Draining,
                OfflineSessionState::Closed,
                None,
            )
            .await?;
            if session_outcome != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): finalize CAS session \
                     Draining → Closed produced {outcome} for session {sid}",
                    fn_id = fiscal_number_owned,
                    outcome = outcome_as_str(session_outcome),
                    sid = session_id_hex_owned,
                ));
            }
            // (3) MED-W9B-1 (2026-05-21): close the pending-drain
            // shift ladder atomically with the mode + session
            // transition.  Only fires if the prereq pass observed
            // a pending-drain shift; Opened (online finalize) has
            // no shift transition.
            if let Some(target) = shift_finalize_target {
                let shift_id = shift_id_opt.expect("checked before envelope");
                // RS-3 C1: shift CAS + node_state projection mirror (m3b §5
                // load-bearing invariant) via the single transition-service
                // (was: transition_state + the local mirror pair).
                let shift_outcome = shift_transition::apply_shift_transition(
                    tx,
                    &fiscal_number_owned,
                    shift_id,
                    shift_state_from,
                    target,
                )
                .await?;
                if !matches!(shift_outcome, shifts::TransitionOutcome::Applied) {
                    return Err(anyhow::anyhow!(
                        "backlog_drain({fn_id}): finalize CAS shift {from} → \
                         {to} produced {outcome:?} for shift {sid} (App reconcile \
                         mutex should prevent races)",
                        fn_id = fiscal_number_owned,
                        from = shift_state_from.as_str(),
                        to = target.as_str(),
                        outcome = shift_outcome,
                        sid = hex_lower(shift_id.as_bytes()),
                    ));
                }
            }
            // (4) Audit OFFLINE_SESSION_CLOSED (W5 session-lifecycle
            // contract — MED-C6-2 2026-05-21).
            audit_log::append_tx(
                tx,
                "offline_session",
                &session_id_hex_owned,
                "OFFLINE_SESSION_CLOSED",
                Severity::Info,
                None,
                Some(&session_closed_payload_owned),
            )
            .await?;
            // (4) Audit OFFLINE_DRAIN_COMPLETED — or
            // OFFLINE_DRAIN_RECOVERED_FINALIZE per [`FinalizeEntry`]
            // (MED-W12C3-01 2026-05-22).
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DRAIN_FN,
                &fiscal_number_owned,
                audit_event_name,
                Severity::Info,
                None,
                Some(&payload_owned),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|source| BootError::ReconciliationFailed {
        fiscal_number: fiscal_number.to_string(),
        source,
    })?;
    // (4) Mark summary finalized AFTER envelope commits.  The typed
    // `mark_finalized()` guard re-runs `finalize_eligibility()` and
    // would Err if anything changed mid-envelope — but we already
    // checked eligibility before opening the tx, so this is a
    // defensive double-check.
    summary.mark_finalized().map_err(|err| {
        BootError::Internal(format!(
            "backlog_drain({fiscal_number}): mark_finalized failed after envelope \
             commit: {err}"
        ))
    })?;
    Ok(())
}

/// NotEligible-arm helper: emit `OFFLINE_DRAIN_PARTIAL` audit with the
/// typed reason payload.
async fn emit_partial(
    pool: &SqlitePool,
    fiscal_number: &str,
    session_id: OfflineSessionId,
    summary: &DrainSummary,
    reason: &NotEligibleReason,
) -> Result<(), BootError> {
    let payload = build_finalize_payload(summary, session_id, "PARTIAL", Some(reason));
    audit_log::append(
        pool,
        AUDIT_ENTITY_DRAIN_FN,
        fiscal_number,
        "OFFLINE_DRAIN_PARTIAL",
        Severity::Warning,
        None,
        Some(&payload.to_string()),
    )
    .await
    .map_err(BootError::Database)?;
    Ok(())
}

/// Build `OFFLINE_DRAIN_COMPLETED` / `OFFLINE_DRAIN_PARTIAL` audit
/// payload from the summary state.  Shared shape so operator
/// dashboards parsing on `outcome` field can switch on the literal
/// value without dual schemas.
fn build_finalize_payload(
    summary: &DrainSummary,
    session_id: OfflineSessionId,
    outcome: &'static str,
    reason: Option<&NotEligibleReason>,
) -> serde_json::Value {
    let per_doc_failures: Vec<serde_json::Value> = summary
        .per_doc_failures()
        .iter()
        .map(|(doc_id, class)| {
            serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "failure_class": class,
            })
        })
        .collect();
    let mut payload = serde_json::json!({
        "fiscal_number": summary.fiscal_number(),
        "session_id": hex_lower(session_id.as_bytes()),
        "outcome": outcome,
        "backlog_size_before": summary.backlog_size_before(),
        "advanced_to_ack": summary.advanced_to_ack(),
        "advanced_to_kvt1": summary.advanced_to_kvt1(),
        "advanced_via_lastchk_replay": summary.advanced_via_lastchk_replay(),
        // M3b W12 Commit 2 — per-counter breakdown for multi-reason
        // forensic payload.  Operator dashboards filter on these even
        // when `not_eligible_reason.kind` selects the
        // highest-precedence single blocker.
        "held_at_kvt1": summary.held_at_kvt1(),
        "held_at_sent": summary.held_at_sent(),
        "er_redrive_queued": summary.er_redrive_queued(),
        "per_doc_failures": per_doc_failures,
        "finalized": summary.finalized(),
    });
    if let Some(reason) = reason {
        payload["not_eligible_reason"] = not_eligible_reason_as_json(reason);
    }
    payload
}

/// Stable JSON encoding of [`NotEligibleReason`] for audit consumers.
fn not_eligible_reason_as_json(reason: &NotEligibleReason) -> serde_json::Value {
    match reason {
        NotEligibleReason::PerDocFailuresPresent { count } => serde_json::json!({
            "kind": "PerDocFailuresPresent",
            "count": count,
        }),
        NotEligibleReason::DocsHeldAtKvt1 { count } => serde_json::json!({
            "kind": "DocsHeldAtKvt1",
            "count": count,
        }),
        NotEligibleReason::DocsHeldAtSent { count } => serde_json::json!({
            "kind": "DocsHeldAtSent",
            "count": count,
        }),
        NotEligibleReason::DocsErRedriveQueued { count } => serde_json::json!({
            "kind": "DocsErRedriveQueued",
            "count": count,
        }),
        NotEligibleReason::DocsDeferredAtKvt1 { count } => serde_json::json!({
            "kind": "DocsDeferredAtKvt1",
            "count": count,
        }),
        NotEligibleReason::AckCountMismatch { expected, actual } => serde_json::json!({
            "kind": "AckCountMismatch",
            "expected": expected,
            "actual": actual,
        }),
    }
}

/// Emit `OFFLINE_DRAIN_DOC_FAILED` audit row.  Severity is always
/// `Warning` per spec §4 (operator dashboards filter by severity).
async fn emit_doc_failed(
    pool: &SqlitePool,
    id_hex: &str,
    payload: &serde_json::Value,
) -> Result<(), BootError> {
    audit_log::append(
        pool,
        AUDIT_ENTITY_DOC,
        id_hex,
        "OFFLINE_DRAIN_DOC_FAILED",
        Severity::Warning,
        None,
        Some(&payload.to_string()),
    )
    .await
    .map_err(BootError::Database)?;
    Ok(())
}

/// Map `RetryClass` (error_routing taxonomy) → `FailureClass` (C2
/// taxonomy).  The three wire-routing branches map 1:1; the remaining
/// four `RetryClass` variants (FnConfigError / WrapperBug / MacRecovery
/// / OperatorEscalation) project onto the closest C2 class.
fn failure_class_for_retry(retry: RetryClass) -> FailureClass {
    match retry {
        RetryClass::TerminalReject => FailureClass::WireRoutingTerminalReject,
        RetryClass::TransientRetry => FailureClass::WireRoutingTransientRetry,
        RetryClass::ProbeRequired => FailureClass::WireRoutingProbeRequired,
        // `-13` / `-14` ERROR_NOT_REGISTERED_RRO|SIGNER — semantic
        // authorization class.
        RetryClass::FnConfigError => FailureClass::Authorization,
        // Wrapper bug / Internal / NotFound on live / etc. — collapsed
        // into Internal for the drain audit taxonomy (operator can
        // grep the underlying `send_error_detail` for precision).
        RetryClass::WrapperBug => FailureClass::Internal,
        // MAC recovery surfacing during drain is rare (would require
        // a -12 on a drain doc); treat as Internal — operator triage
        // via the audit detail.
        RetryClass::MacRecovery => FailureClass::Internal,
        // -6 operator-recoverable.
        RetryClass::OperatorEscalation => FailureClass::Server,
    }
}

/// Map `StageSendError` (per-doc Err result) → `FailureClass`.  All
/// `StageSendError` variants are treated as per-doc failures —
/// sibling continues per spec §2.5 try-and-audit shim.
fn failure_class_for_send_err(err: &StageSendError) -> FailureClass {
    match err {
        // Both producer-side offline_fiscal_no defects (NULL vs <= 0) are the
        // same DRAIN disposition; the forensic split lives at the
        // StageSendError + audit layer (m3b W9a Round-2 LOW #1).
        StageSendError::OfflineFiscalNoMissing { .. }
        | StageSendError::OfflineFiscalNoNonPositive { .. } => FailureClass::OfflineFiscalNoMissing,
        StageSendError::DocumentMissingForRecovery { .. } => FailureClass::NotFound,
        StageSendError::UnsupportedDocType { .. }
        | StageSendError::LndOutOfRangeI32 { .. }
        | StageSendError::TimestampConversion { .. }
        | StageSendError::SignedArtifactMissing { .. }
        | StageSendError::EmptyServerFiscalNo { .. }
        | StageSendError::PostWireCasFailed { .. }
        | StageSendError::MarkSubmissionAttemptedMissing { .. }
        | StageSendError::NodeStateMissingForBlock { .. }
        | StageSendError::MacRecoveryContextMissing { .. }
        | StageSendError::MacRecoveryArtifactMissing { .. }
        | StageSendError::FnConfigMissingForRecovery { .. }
        | StageSendError::MacRecoverySignFailed(_)
        | StageSendError::MacRecoverySnapshotReloadFailed(_)
        | StageSendError::SetServerFiscalNoMissing { .. }
        | StageSendError::TraceMissingAtComplete { .. }
        | StageSendError::Db(_)
        | StageSendError::Internal(_) => FailureClass::Internal,
    }
}

// ─── C6 amend2 (MED-C6-3): Eligible-arm integration tests ────────────
//
// Pre-W12 the C5 `apply_w12_confirmation` stub always returns
// `DeferredKvt1`, so the public `drain()` entry cannot naturally
// reach the Eligible arm.  These inline `#[cfg(test)]` tests
// construct a `DrainSummary` with `Acked` outcomes manually + call
// `commit_finalize_envelope` directly (crate-internal access) to
// prove the Eligible-arm CAS chain + audit emission contract pre-W12.
// W12 PR replaces these with full-flow integration tests once
// real Ack proof is constructible from drain.

#[cfg(test)]
mod eligible_arm_tests {
    use super::*;
    use crate::db::models::ids::OfflineSessionId;
    use crate::db::repositories::node_state::NodeStateRow;

    const FN: &str = "1234567890";

    async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = crate::db::open_pool(&dir.path().join("c6_eligible.db"))
            .await
            .expect("open_pool runs migrations");
        sqlx::query(
            "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
             VALUES (?, '12345678', 'test')",
        )
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
        (dir, pool)
    }

    async fn seed_node_state_going_online(pool: &sqlx::SqlitePool) -> NodeStateRow {
        sqlx::query(
            "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
             VALUES (?, 'GOING_ONLINE', 'OPENED', 100)",
        )
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
        node_state::get(pool, FN).await.unwrap().unwrap()
    }

    async fn seed_draining_session(pool: &sqlx::SqlitePool) -> OfflineSessionId {
        let session_id = OfflineSessionId::new();
        sqlx::query(
            "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at, drained_at) \
             VALUES (?, ?, 'DRAINING', '2026-05-21T00:00:00Z', '2026-05-21T00:00:01Z')",
        )
        .bind(session_id)
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
        session_id
    }

    async fn audit_count(pool: &sqlx::SqlitePool, event_type: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
            .bind(event_type)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn audit_latest_payload(pool: &sqlx::SqlitePool, event_type: &str) -> serde_json::Value {
        let raw: String = sqlx::query_scalar(
            "SELECT event_payload_json FROM audit_log \
             WHERE event_type = ? ORDER BY audit_id DESC LIMIT 1",
        )
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    async fn read_node_mode(pool: &sqlx::SqlitePool) -> String {
        sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn read_session_state(pool: &sqlx::SqlitePool, session_id: OfflineSessionId) -> String {
        sqlx::query_scalar("SELECT state FROM offline_sessions WHERE offline_session_id = ?")
            .bind(session_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// MED-C6-3 (2026-05-21): Eligible-arm full contract.  Construct
    /// a DrainSummary with 2 Acked outcomes manually, call
    /// `commit_finalize_envelope` directly, assert:
    ///   - `node_state.mode` GoingOnline → Online
    ///   - `offline_session.state` Draining → Closed
    ///   - `OFFLINE_SESSION_CLOSED` audit emitted with W5 payload shape
    ///   - `OFFLINE_DRAIN_COMPLETED` audit emitted with `finalized=true`
    ///   - `summary.finalized() == true` post-helper
    #[tokio::test]
    async fn c6_eligible_arm_commits_mode_session_audits_and_finalizes_summary() {
        let (_d, pool) = fresh_pool().await;
        let ns = seed_node_state_going_online(&pool).await;
        let session_id = seed_draining_session(&pool).await;

        // Build a DrainSummary with 2 Acked outcomes → finalize_eligibility
        // returns Eligible.
        let mut summary = DrainSummary::new(FN.to_string(), 2);
        summary.record_doc_advanced(
            &W12ConfirmOutcome::Acked {
                server_fiscal_no: "DPS-A".into(),
            },
            false,
        );
        summary.record_doc_advanced(
            &W12ConfirmOutcome::Acked {
                server_fiscal_no: "DPS-B".into(),
            },
            false,
        );
        assert!(matches!(
            summary.finalize_eligibility(),
            FinalizeEligibility::Eligible
        ));

        commit_finalize_envelope(
            &pool,
            FN,
            session_id,
            &ns,
            &mut summary,
            FinalizeEntry::NormalEligible,
        )
        .await
        .expect("Eligible commit must succeed on properly-seeded fixture");

        // DB state.
        assert_eq!(read_node_mode(&pool).await, "ONLINE");
        assert_eq!(read_session_state(&pool, session_id).await, "CLOSED");

        // Audit emission.
        assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 1);
        assert_eq!(audit_count(&pool, "OFFLINE_SESSION_CLOSED").await, 1);

        // OFFLINE_DRAIN_COMPLETED payload contract (MED-C6-1 fix).
        let completed = audit_latest_payload(&pool, "OFFLINE_DRAIN_COMPLETED").await;
        assert_eq!(completed["outcome"], "COMPLETED");
        assert_eq!(
            completed["finalized"], true,
            "MED-C6-1: COMPLETED audit MUST carry finalized=true"
        );
        assert_eq!(completed["advanced_to_ack"], 2);
        assert_eq!(completed["advanced_to_kvt1"], 0);
        assert_eq!(completed["backlog_size_before"], 2);
        assert_eq!(completed["per_doc_failures"].as_array().unwrap().len(), 0);

        // OFFLINE_SESSION_CLOSED payload contract (MED-C6-2 fix; W5 shape).
        let closed = audit_latest_payload(&pool, "OFFLINE_SESSION_CLOSED").await;
        assert_eq!(closed["from"], "DRAINING");
        assert_eq!(closed["to"], "CLOSED");
        assert!(
            closed["reason_abort"].is_null(),
            "W5 close payload: reason_abort=null on normal close"
        );
        let session_hex: String = session_id
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(closed["offline_session_id"], session_hex);

        // Summary state.
        assert!(
            summary.finalized(),
            "summary.mark_finalized() MUST set finalized=true after Eligible envelope commits"
        );
    }

    // ─── MED-W9B-1 (2026-05-21) — pending-drain shift closure ────────

    use crate::db::models::ids::ShiftId;

    async fn seed_node_state_with_shift(
        pool: &sqlx::SqlitePool,
        mode: &str,
        shift_state: &str,
        shift_id: ShiftId,
    ) -> NodeStateRow {
        sqlx::query(
            "INSERT INTO node_state(fiscal_number, mode, shift_state, current_shift_id, next_lnd) \
             VALUES (?, ?, ?, ?, 100)",
        )
        .bind(FN)
        .bind(mode)
        .bind(shift_state)
        .bind(shift_id)
        .execute(pool)
        .await
        .unwrap();
        node_state::get(pool, FN).await.unwrap().unwrap()
    }

    async fn seed_shift_in_state(pool: &sqlx::SqlitePool, state: &str) -> ShiftId {
        let shift_id = ShiftId::new();
        sqlx::query(
            "INSERT INTO shifts(shift_id, fiscal_number, serial, state, \
                open_mode, cash_balance_kop, opened_by_cashier_id) \
             VALUES (?, ?, 1, ?, 'OFFLINE', 0, 'test-cashier')",
        )
        .bind(shift_id)
        .bind(FN)
        .bind(state)
        .execute(pool)
        .await
        .unwrap();
        shift_id
    }

    async fn read_shift_state(pool: &sqlx::SqlitePool, shift_id: ShiftId) -> String {
        sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn read_node_shift_state(pool: &sqlx::SqlitePool) -> String {
        sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn ack_summary_2() -> DrainSummary {
        let mut summary = DrainSummary::new(FN.to_string(), 2);
        summary.record_doc_advanced(
            &W12ConfirmOutcome::Acked {
                server_fiscal_no: "DPS-A".into(),
            },
            false,
        );
        summary.record_doc_advanced(
            &W12ConfirmOutcome::Acked {
                server_fiscal_no: "DPS-B".into(),
            },
            false,
        );
        summary
    }

    /// MED-W9B-1: shift in `OpenedLocalPendingDrain` at finalize time
    /// → drain transitions `OpenedLocalPendingDrain → Opened` (edge 5)
    /// + mirrors `node_state.shift_state` in the same envelope.
    #[tokio::test]
    async fn c6_finalize_closes_opened_local_pending_drain_ladder_to_opened() {
        let (_d, pool) = fresh_pool().await;
        let shift_id = seed_shift_in_state(&pool, "OPENED_LOCAL_PENDING_DRAIN").await;
        let ns = seed_node_state_with_shift(
            &pool,
            "GOING_ONLINE",
            "OPENED_LOCAL_PENDING_DRAIN",
            shift_id,
        )
        .await;
        let session_id = seed_draining_session(&pool).await;
        let mut summary = ack_summary_2();

        commit_finalize_envelope(
            &pool,
            FN,
            session_id,
            &ns,
            &mut summary,
            FinalizeEntry::NormalEligible,
        )
        .await
        .expect("Eligible commit on pending-drain shift must succeed");

        // Mode + session + shift + node_state mirror all finalized.
        assert_eq!(read_node_mode(&pool).await, "ONLINE");
        assert_eq!(read_session_state(&pool, session_id).await, "CLOSED");
        assert_eq!(read_shift_state(&pool, shift_id).await, "OPENED");
        assert_eq!(
            read_node_shift_state(&pool).await,
            "OPENED",
            "node_state.shift_state MUST mirror shifts.state per §5 invariant"
        );

        // Audit chain.
        assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 1);
        assert_eq!(audit_count(&pool, "OFFLINE_SESSION_CLOSED").await, 1);

        assert!(summary.finalized());
    }

    /// MED-W9B-1: shift in `ClosingLocalPendingDrain` at finalize time
    /// → drain transitions `ClosingLocalPendingDrain → Closed`
    /// (edge 13) + mirrors `node_state.shift_state`.  Pre-W12 the
    /// close-drain predicate is enforced via the C2
    /// `finalize_eligibility` gate (Eligible only when every drain
    /// doc returned Acked — Z_REPORT included).
    #[tokio::test]
    async fn c6_finalize_closes_closing_local_pending_drain_ladder_to_closed() {
        let (_d, pool) = fresh_pool().await;
        let shift_id = seed_shift_in_state(&pool, "CLOSING_LOCAL_PENDING_DRAIN").await;
        let ns = seed_node_state_with_shift(
            &pool,
            "GOING_ONLINE",
            "CLOSING_LOCAL_PENDING_DRAIN",
            shift_id,
        )
        .await;
        let session_id = seed_draining_session(&pool).await;
        let mut summary = ack_summary_2();

        commit_finalize_envelope(
            &pool,
            FN,
            session_id,
            &ns,
            &mut summary,
            FinalizeEntry::NormalEligible,
        )
        .await
        .expect("Eligible commit on closing-pending-drain shift must succeed");

        assert_eq!(read_node_mode(&pool).await, "ONLINE");
        assert_eq!(read_session_state(&pool, session_id).await, "CLOSED");
        assert_eq!(read_shift_state(&pool, shift_id).await, "CLOSED");
        assert_eq!(
            read_node_shift_state(&pool).await,
            "CLOSED",
            "node_state.shift_state MUST mirror shifts.state"
        );

        assert_eq!(audit_count(&pool, "OFFLINE_DRAIN_COMPLETED").await, 1);
        assert_eq!(audit_count(&pool, "OFFLINE_SESSION_CLOSED").await, 1);

        assert!(summary.finalized());
    }
}

// ─── M3b W12 Commit 2: HoldFnDrain control surface tests ─────────────
//
// Synthetic tests for the projection-aware DrainSummary +
// FinalizeEligibility surface added in Commit 2.  No drain-loop
// integration here — full helper wiring + drain dispatcher rewires
// land in Commits 4 / 5 / 5b with their own integration tests.

#[cfg(test)]
mod w12_control_surface_tests {
    use super::*;
    use crate::db::models::ids::{DocumentId, OfflineSessionId};

    const FN: &str = "1234567890";

    fn doc() -> DocumentId {
        DocumentId::new()
    }

    fn summary_with_size(backlog_size: usize) -> DrainSummary {
        DrainSummary::new(FN.to_string(), backlog_size)
    }

    // ─── Counter increment per recording method ──────────────────────

    #[test]
    fn record_doc_held_at_kvt1_increments_counter() {
        let mut s = summary_with_size(3);
        assert_eq!(s.held_at_kvt1(), 0);
        s.record_doc_held_at_kvt1(doc(), "wire_routing_probe_required".into());
        assert_eq!(s.held_at_kvt1(), 1);
        assert_eq!(s.held_at_sent(), 0);
        assert_eq!(s.er_redrive_queued(), 0);
    }

    #[test]
    fn record_doc_held_at_sent_increments_counter() {
        let mut s = summary_with_size(3);
        s.record_doc_held_at_sent(doc(), "transport".into());
        assert_eq!(s.held_at_sent(), 1);
        assert_eq!(s.held_at_kvt1(), 0);
        assert_eq!(s.er_redrive_queued(), 0);
    }

    #[test]
    fn record_doc_er_redrive_queued_increments_counter() {
        let mut s = summary_with_size(3);
        s.record_doc_er_redrive_queued(doc(), "sent_not_found_downgrade".into());
        assert_eq!(s.er_redrive_queued(), 1);
        assert_eq!(s.held_at_kvt1(), 0);
        assert_eq!(s.held_at_sent(), 0);
    }

    // ─── finalize_eligibility per single W12 counter ─────────────────

    #[test]
    fn held_at_kvt1_blocks_finalize_with_docs_held_at_kvt1() {
        let mut s = summary_with_size(1);
        s.record_doc_held_at_kvt1(doc(), "transport".into());
        match s.finalize_eligibility() {
            FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsHeldAtKvt1 { count },
            } => assert_eq!(count, 1),
            other => panic!("expected DocsHeldAtKvt1, got {other:?}"),
        }
    }

    #[test]
    fn held_at_sent_blocks_finalize_with_docs_held_at_sent() {
        let mut s = summary_with_size(1);
        s.record_doc_held_at_sent(doc(), "transport".into());
        assert!(matches!(
            s.finalize_eligibility(),
            FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsHeldAtSent { count: 1 },
            }
        ));
    }

    #[test]
    fn er_redrive_queued_blocks_finalize_with_docs_er_redrive_queued() {
        let mut s = summary_with_size(1);
        s.record_doc_er_redrive_queued(doc(), "sent_not_found_downgrade".into());
        assert!(matches!(
            s.finalize_eligibility(),
            FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsErRedriveQueued { count: 1 },
            }
        ));
    }

    // ─── Precedence ──────────────────────────────────────────────────

    #[test]
    fn per_doc_failure_takes_precedence_over_w12_counters() {
        let mut s = summary_with_size(3);
        s.record_doc_failure(doc(), "transport".into());
        s.record_doc_held_at_kvt1(doc(), "transport".into());
        s.record_doc_held_at_sent(doc(), "transport".into());
        s.record_doc_er_redrive_queued(doc(), "sent_not_found_downgrade".into());
        assert!(matches!(
            s.finalize_eligibility(),
            FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::PerDocFailuresPresent { count: 1 },
            }
        ));
    }

    #[test]
    fn held_at_kvt1_takes_precedence_over_held_at_sent_and_er_redrive() {
        let mut s = summary_with_size(3);
        s.record_doc_held_at_kvt1(doc(), "transport".into());
        s.record_doc_held_at_sent(doc(), "transport".into());
        s.record_doc_er_redrive_queued(doc(), "sent_not_found_downgrade".into());
        assert!(matches!(
            s.finalize_eligibility(),
            FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsHeldAtKvt1 { count: 1 },
            }
        ));
    }

    #[test]
    fn held_at_sent_takes_precedence_over_er_redrive_queued() {
        let mut s = summary_with_size(2);
        s.record_doc_held_at_sent(doc(), "transport".into());
        s.record_doc_er_redrive_queued(doc(), "sent_not_found_downgrade".into());
        assert!(matches!(
            s.finalize_eligibility(),
            FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsHeldAtSent { count: 1 },
            }
        ));
    }

    #[test]
    fn w12_counters_take_precedence_over_legacy_deferred_at_kvt1() {
        // Synthetic: simulate one legacy DeferredKvt1 (via
        // record_doc_advanced) plus one W12 ErRedriveQueued.  Precedence
        // says W12 counters block before the legacy stub counter.
        let mut s = summary_with_size(2);
        s.record_doc_advanced(
            &W12ConfirmOutcome::DeferredKvt1,
            /* via_lastchk_replay */ false,
        );
        s.record_doc_er_redrive_queued(doc(), "sent_not_found_downgrade".into());
        assert!(matches!(
            s.finalize_eligibility(),
            FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::DocsErRedriveQueued { count: 1 },
            }
        ));
    }

    // ─── Eligible only with ALL counters zero + Acked == backlog ─────

    #[test]
    fn eligible_only_when_all_w12_counters_zero_and_acked_complete() {
        let mut s = summary_with_size(1);
        s.record_doc_advanced(
            &W12ConfirmOutcome::Acked {
                server_fiscal_no: "FN-001".into(),
            },
            false,
        );
        assert_eq!(s.held_at_kvt1(), 0);
        assert_eq!(s.held_at_sent(), 0);
        assert_eq!(s.er_redrive_queued(), 0);
        assert_eq!(s.advanced_to_ack(), 1);
        assert_eq!(s.advanced_to_kvt1(), 0);
        assert!(matches!(
            s.finalize_eligibility(),
            FinalizeEligibility::Eligible
        ));
    }

    // ─── Multi-reason payload in OFFLINE_DRAIN_PARTIAL ───────────────

    #[test]
    fn build_finalize_payload_includes_all_three_w12_counters() {
        let mut s = summary_with_size(3);
        s.record_doc_held_at_kvt1(doc(), "transport".into());
        s.record_doc_held_at_sent(doc(), "transport".into());
        s.record_doc_er_redrive_queued(doc(), "sent_not_found_downgrade".into());

        let reason = NotEligibleReason::DocsHeldAtKvt1 { count: 1 };
        let session_id = OfflineSessionId::new();
        let payload = build_finalize_payload(&s, session_id, "PARTIAL", Some(&reason));

        // Multi-reason breakdown: all three W12 counters are present
        // as separate JSON keys regardless of which one was selected
        // for `not_eligible_reason.kind` (precedence picks one;
        // payload carries all for forensic dashboards).
        assert_eq!(payload["held_at_kvt1"], 1);
        assert_eq!(payload["held_at_sent"], 1);
        assert_eq!(payload["er_redrive_queued"], 1);
        // not_eligible_reason carries the highest-precedence single
        // blocker.
        assert_eq!(payload["not_eligible_reason"]["kind"], "DocsHeldAtKvt1");
        assert_eq!(payload["not_eligible_reason"]["count"], 1);
    }

    // ─── not_eligible_reason_as_json: all three new variants ─────────

    #[test]
    fn not_eligible_reason_as_json_docs_held_at_kvt1() {
        let j = not_eligible_reason_as_json(&NotEligibleReason::DocsHeldAtKvt1 { count: 7 });
        assert_eq!(j["kind"], "DocsHeldAtKvt1");
        assert_eq!(j["count"], 7);
    }

    #[test]
    fn not_eligible_reason_as_json_docs_held_at_sent() {
        let j = not_eligible_reason_as_json(&NotEligibleReason::DocsHeldAtSent { count: 3 });
        assert_eq!(j["kind"], "DocsHeldAtSent");
        assert_eq!(j["count"], 3);
    }

    #[test]
    fn not_eligible_reason_as_json_docs_er_redrive_queued() {
        let j = not_eligible_reason_as_json(&NotEligibleReason::DocsErRedriveQueued { count: 2 });
        assert_eq!(j["kind"], "DocsErRedriveQueued");
        assert_eq!(j["count"], 2);
    }

    // ─── HoldFnDrain variant + HoldFnDrainProjection compile-time ────

    #[test]
    fn hold_fn_drain_variant_constructible_per_projection() {
        let v_kvt1 = DocVerdict::HoldFnDrain {
            class: FailureClass::WireRoutingProbeRequired,
            projection: HoldFnDrainProjection::HeldAtKvt1,
            consecutive_holds: 0,
        };
        let v_sent = DocVerdict::HoldFnDrain {
            class: FailureClass::Transport,
            projection: HoldFnDrainProjection::HeldAtSent,
            consecutive_holds: 0,
        };
        let v_er = DocVerdict::HoldFnDrain {
            class: FailureClass::BudgetExhausted,
            projection: HoldFnDrainProjection::ErRedriveQueued,
            consecutive_holds: 0,
        };
        // Construction does not panic; pattern-match arms are
        // structurally distinguishable.
        for v in [v_kvt1, v_sent, v_er] {
            match v {
                DocVerdict::HoldFnDrain { projection, .. } => {
                    let _ = projection; // exhaustive arm reachable
                }
                DocVerdict::Advanced | DocVerdict::Failed { .. } | DocVerdict::SupersededHeld => {
                    panic!("unexpected non-HoldFnDrain variant")
                }
            }
        }
    }
}
