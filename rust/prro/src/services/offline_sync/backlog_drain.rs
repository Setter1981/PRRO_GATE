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
//!   inlines the W12-stub `Sent → Kvt1` advance so `advanced_to_kvt1`,
//!   the audit `to_state="KVT1"`, and the persisted DB state all stay
//!   consistent.  Audits per-doc `_DOC_ADVANCED` / `_DOC_FAILED`.
//!   Routes manual-recon-class failures on pending-drain shifts to
//!   `RequiresManualReconciliation` and halts the drain (per spec
//!   amendment 2026-05-21 and `LEGAL_INVARIANTS.md` §INV-19).
//!   Sibling-continue applies ONLY to per-doc failures on
//!   non-pending-drain shifts.  **No lastChk pre-flight yet** (C5);
//!   **no finalize branch yet** (C6).
//! - **Commits 5-7** — widen walker to the unfinished cohort
//!   (`OFFLINE_LOCAL_ACK | SENT | KVT1 | ERROR_RETRYABLE`; KVT2
//!   deferred to W12 PR per MED-C5-4), add lastChk pre-flight,
//!   extract the inline `Sent → Kvt1` into the
//!   `apply_w12_confirmation` helper, add the finalization branch,
//!   and add the App entry.
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
use crate::db::repositories::{
    audit_log, document_files, fiscal_documents, node_state, offline_sessions, shifts,
};
use crate::db::tx::{with_immediate, WriteTxConn};
use crate::services::reconciliation::guard::ReconcileGuard;
use crate::services::reconciliation::last_chk_probe::{self, ProbeOutcome};
use crate::services::reconciliation::runtime::RuntimeView;
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

    /// Decide whether the drain may finalize (node mode + offline
    /// session transitions).  Returns the typed eligibility — caller
    /// MUST pattern-match.  Pre-W12 invariant pin: any `DeferredKvt1`
    /// outcome (i.e. `advanced_to_kvt1 > 0`) blocks finalize.
    pub fn finalize_eligibility(&self) -> FinalizeEligibility {
        if !self.per_doc_failures.is_empty() {
            return FinalizeEligibility::NotEligible {
                reason: NotEligibleReason::PerDocFailuresPresent {
                    count: self.per_doc_failures.len(),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotEligibleReason {
    /// At least one doc failed per-doc processing.  Sibling docs
    /// may have succeeded but at least one failure prevents finalize.
    PerDocFailuresPresent { count: usize },
    /// At least one doc returned `W12ConfirmOutcome::DeferredKvt1` —
    /// pre-W12 invariant pin.  This is the steady-state result for
    /// W9b pre-W12 PR.
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
const AUDIT_ENTITY_DOC: &str = "fiscal_document";

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
/// and per-doc counters reflecting C4's actual processing: each
/// successful wire send increments `advanced_to_kvt1` (via the inline
/// W12 stub Sent→Kvt1 advance); each non-Sent outcome appends to
/// `per_doc_failures`.  On a pending-drain shift, the loop halts
/// early on a manual-recon-class failure (see
/// [`is_manual_recon_retry_class`]), transitions shift and the
/// node_state mirror to `RequiresManualReconciliation`, and emits a
/// Critical `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit; in that
/// case the summary reflects state up to the halt position.
///
/// Caller observing this intermediate result MUST NOT treat the
/// chain as finalized — `OFFLINE_DRAIN_COMPLETED|PARTIAL` has not
/// been emitted, and `finalize_eligibility` returns
/// `NotEligible{DocsDeferredAtKvt1 | AckCountMismatch}` by
/// construction (pre-W12 cannot produce real Ack proof).
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
    let shift_in_pending_drain = matches!(
        ns.shift_state,
        ShiftState::OpenedLocalPendingDrain | ShiftState::ClosingLocalPendingDrain
    );

    let mut summary = DrainSummary::new(fiscal_number.to_string(), backlog.len());
    for (position, doc) in backlog.iter().enumerate() {
        let verdict = process_one_doc(pool, deps, fiscal_number, doc, &mut summary).await?;
        // Halt ONLY on manual-recon-class failures on pending-drain
        // shifts.  TransientRetry / ProbeRequired stay in
        // sibling-continue per spec §3.5 (Manual is last resort;
        // transient outcomes retain retry budget).
        if shift_in_pending_drain {
            if let DocVerdict::Failed {
                class,
                manual_recon: true,
            } = verdict
            {
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
/// 2026-05-21 cohort dispatch contract; post MED-C5-4 KVT2 deferral):
/// - `OFFLINE_LOCAL_ACK` / `ERROR_RETRYABLE` → wire send via
///   `process_via_stage_send` (W9a 4-pre source whitelist).
/// - `SENT` → lastChk pre-flight via `process_via_lastchk_replay`
///   (closes I4 restart safety per spec §6).  No wire fall-through:
///   Mismatch / Decode / Unexpected route to manual-recon failure;
///   NotFound downgrades to `ErrorRetryable` for next-tick Pattern B
///   re-drive (HIGH-C5-3); TransportRetry keeps the doc SENT for
///   the next tick to re-probe.
/// - `KVT1` → `process_via_w12_only` (no wire, no pre-flight;
///   pre-W12 stub records DeferredKvt1).
/// - Other states → `BootError::Internal` (cohort walker SELECT
///   filter breach).  `KVT2` is deferred to W12 PR per MED-C5-4 —
///   pre-W12 drain has no clean discharge path.
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
        DocState::OfflineLocalAck | DocState::ErrorRetryable => {
            process_via_stage_send(pool, deps, fiscal_number, doc, summary).await
        }
        DocState::Sent => process_via_lastchk_replay(pool, deps, fiscal_number, doc, summary).await,
        DocState::Kvt1 => process_via_w12_only(pool, fiscal_number, doc, summary).await,
        other => Err(BootError::Internal(format!(
            "backlog_drain({fiscal_number}): cohort walker returned unexpected \
             doc.state {state} for doc {hex} (SELECT must filter to drain \
             candidates: OFFLINE_LOCAL_ACK | SENT | KVT1 | ERROR_RETRYABLE post \
             MED-C5-4 KVT2 deferral)",
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
    fiscal_number: &str,
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
            // C5 + LOW-C5-R1 (2026-05-21): pre-build the audit payload
            // and pass it through to apply_w12_confirmation; the helper
            // commits CAS Sent→Kvt1 + audit row inside ONE
            // `with_immediate` envelope so audit append failure rolls
            // back the CAS.  Pre-W12 stub return is always
            // DeferredKvt1.
            //
            // `doc.state` here is OfflineLocalAck or ErrorRetryable
            // (per outer state dispatch); stage_send Sent outcome
            // implies the doc has already transitioned through
            // Sending → Sent inside stage_send's 4-b envelope.  The
            // audit `from_state` reflects the drain-loop ENTRY state
            // (pre-stage_send), not the inner Sending intermediate.
            let audit_payload = serde_json::json!({
                "document_id": id_hex,
                "from_state": doc.state.as_str(),
                "to_state": DocState::Kvt1.as_str(),
                "replay_short_circuit": false,
                "attempt_no": attempt_no,
                "server_fiscal_no": server_fiscal_no,
                "w12_status": W12ConfirmOutcome::DeferredKvt1.w12_status_str(),
                "dispatch_via": "stage_send",
            });
            // Pass `DocState::Sent` literal so the helper routes
            // through the Sent → Kvt1 CAS arm — at this point
            // `stage_send::run` has already committed the doc to
            // Sent inside its own envelope, even though `doc.state`
            // (the cohort-walker snapshot) shows the pre-stage_send
            // OFFLINE_LOCAL_ACK / ERROR_RETRYABLE.
            //
            // `kvt1_raw_bytes: None` — stage_send::Sent outcome does
            // not surface `ack.data_sign` in its public outcome
            // (only `server_fiscal_no` + `attempt_no`).  Pre-W12 gap;
            // W12 PR closes by routing all Sent advances through the
            // lastChk evidence path.
            let w12 = apply_w12_confirmation(
                pool,
                fiscal_number,
                doc.document_id,
                DocState::Sent,
                &audit_payload,
                None,
            )
            .await?;
            summary.record_doc_advanced(&w12, false);
            Ok(DocVerdict::Advanced)
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
async fn process_via_lastchk_replay(
    pool: &SqlitePool,
    deps: &RuntimeView<'_>,
    fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
) -> Result<DocVerdict, BootError> {
    let id_hex = hex_lower(doc.document_id.as_bytes());

    let Some(expected_id) = doc.server_fiscal_no.as_deref() else {
        // Structural drift — record as Internal failure, mark
        // manual-recon-class.
        let class = FailureClass::Internal;
        let class_str = failure_class_for(class);
        summary.record_doc_failure(doc.document_id, class_str.to_string());
        let payload = serde_json::json!({
            "document_id": id_hex,
            "failure_class": class_str,
            "drift_reason": "SENT doc has server_fiscal_no = NULL (stage_send 4-b invariant breach)",
            "manual_recon_class": true,
            "dispatch_via": "lastchk_replay",
        });
        emit_doc_failed(pool, &id_hex, &payload).await?;
        return Ok(DocVerdict::Failed {
            class,
            manual_recon: true,
        });
    };

    let probe_outcome = last_chk_probe::probe(deps.dps, deps.fn_sign, expected_id).await;
    match probe_outcome {
        ProbeOutcome::Match { ack } => {
            // Spec §2.3 Step A predicate also requires non-empty
            // data_sign (KVT2 evidence).  Empty data_sign = "DPS
            // matched the id but didn't return KVT2 bytes" — treat
            // as a structural Match-but-no-evidence anomaly.
            if ack.data_sign.is_empty() {
                let class = FailureClass::Internal;
                let class_str = failure_class_for(class);
                summary.record_doc_failure(doc.document_id, class_str.to_string());
                let payload = serde_json::json!({
                    "document_id": id_hex,
                    "failure_class": class_str,
                    "probe_outcome": "MatchButEmptyDataSign",
                    "manual_recon_class": true,
                    "dispatch_via": "lastchk_replay",
                });
                emit_doc_failed(pool, &id_hex, &payload).await?;
                return Ok(DocVerdict::Failed {
                    class,
                    manual_recon: true,
                });
            }
            // REPLAY HIT.  Advance via W12 stub — CAS Sent→Kvt1 +
            // audit row in ONE `with_immediate` envelope (LOW-C5-R1
            // atomicity).
            let audit_payload = serde_json::json!({
                "document_id": id_hex,
                "from_state": DocState::Sent.as_str(),
                "to_state": DocState::Kvt1.as_str(),
                "replay_short_circuit": true,
                "server_fiscal_no": expected_id,
                "w12_status": W12ConfirmOutcome::DeferredKvt1.w12_status_str(),
                "dispatch_via": "lastchk_replay",
            });
            // Cohort walker filtered to SENT — pass the literal so
            // the caller-side contract is grep-able alongside the
            // dispatcher arm.  KVT1_RAW persisted in-envelope from
            // `ack.data_sign` (HIGH-C5-2 fix: forensic evidence
            // contract matches M3a `boot_phase::advance_sent_to_
            // kvt1_from_probe`).
            let w12 = apply_w12_confirmation(
                pool,
                fiscal_number,
                doc.document_id,
                DocState::Sent,
                &audit_payload,
                Some(&ack.data_sign),
            )
            .await?;
            summary.record_doc_advanced(&w12, true);
            Ok(DocVerdict::Advanced)
        }
        ProbeOutcome::Mismatch { actual_id } => {
            let class = FailureClass::Internal;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "probe_outcome": "Mismatch",
                "expected_server_fiscal_no": expected_id,
                "actual_server_fiscal_no": actual_id,
                "manual_recon_class": true,
                "dispatch_via": "lastchk_replay",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        ProbeOutcome::NotFound => {
            // HIGH-C5-3 fix (2026-05-21): DPS NotFound is the safe
            // Pattern B re-drive case — DPS has zero record of any
            // check for this FN_sign, so re-sending via `stage_send`
            // (next tick, through ERROR_RETRYABLE cohort) does NOT
            // double-fiscalize.  Matches the existing M3a
            // `boot_phase` last_chk NotFound contract (W9 freeze
            // §4.5).  CAS Sent → ErrorRetryable + audit row in ONE
            // `with_immediate` envelope so the downgrade is atomic
            // with its forensic trail.  Non-manual class: pending-
            // drain shifts retain retry budget (spec §3.5 "Manual is
            // last resort").
            downgrade_sent_to_error_retryable_for_retry(
                pool,
                fiscal_number,
                doc.document_id,
                &id_hex,
                expected_id,
            )
            .await?;
            let class = FailureClass::Transport;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            Ok(DocVerdict::Failed {
                class,
                manual_recon: false,
            })
        }
        ProbeOutcome::TransportRetry { reason } => {
            // Non-manual: retain retry budget for next tick.  Doc
            // stays in SENT; next drain tick re-probes.
            let class = FailureClass::Transport;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "probe_outcome": "TransportRetry",
                "probe_reason": reason,
                "manual_recon_class": false,
                "dispatch_via": "lastchk_replay",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: false,
            })
        }
        ProbeOutcome::DecodeEscalate { reason } => {
            let class = FailureClass::Decode;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "probe_outcome": "DecodeEscalate",
                "probe_reason": reason,
                "manual_recon_class": true,
                "dispatch_via": "lastchk_replay",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
        ProbeOutcome::Unexpected { dps_error } => {
            let class = FailureClass::Internal;
            let class_str = failure_class_for(class);
            summary.record_doc_failure(doc.document_id, class_str.to_string());
            let payload = serde_json::json!({
                "document_id": id_hex,
                "failure_class": class_str,
                "probe_outcome": "Unexpected",
                "probe_error": dps_error,
                "manual_recon_class": true,
                "dispatch_via": "lastchk_replay",
            });
            emit_doc_failed(pool, &id_hex, &payload).await?;
            Ok(DocVerdict::Failed {
                class,
                manual_recon: true,
            })
        }
    }
}

/// Process a doc in `KVT1` or `KVT2` state via the W12 stub only
/// (no wire, no pre-flight).  Pre-W12 the stub returns
/// `DeferredKvt1` without DB mutation — the doc remains in its
/// current state until W12 PR lands the `lastChk` evidence path.
/// Each drain tick re-visits these docs and re-records DeferredKvt1
/// (operator-pinned correct but inefficient pre-W12 steady-state).
async fn process_via_w12_only(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc: &fiscal_documents::DocumentRow,
    summary: &mut DrainSummary,
) -> Result<DocVerdict, BootError> {
    let id_hex = hex_lower(doc.document_id.as_bytes());
    let audit_payload = serde_json::json!({
        "document_id": id_hex,
        "from_state": doc.state.as_str(),
        // Pre-W12 stub: no DB transition for Kvt1/Kvt2; to_state
        // mirrors from_state.  W12 PR will set to_state="ACK" on
        // the Kvt2 → Ack path via stage_finalize::run.
        "to_state": doc.state.as_str(),
        "replay_short_circuit": false,
        "w12_status": W12ConfirmOutcome::DeferredKvt1.w12_status_str(),
        "dispatch_via": "w12_only",
    });
    // Cohort walker filtered to KVT1 post MED-C5-4 (KVT2 deferred to
    // W12 PR); pass `doc.state` through.
    let w12 = apply_w12_confirmation(
        pool,
        fiscal_number,
        doc.document_id,
        doc.state,
        &audit_payload,
        None,
    )
    .await?;
    summary.record_doc_advanced(&w12, false);
    Ok(DocVerdict::Advanced)
}

/// W9b C5 W12 stub seam (spec §2.3 Step C + §9 OQ-2).  Pre-W12
/// stub body ALWAYS returns [`W12ConfirmOutcome::DeferredKvt1`] —
/// no real Ack proof is constructible by drain alone (operator-pinned
/// invariant).  W12 PR replaces this body with the `lastChk` evidence
/// path (`Kvt1 → Kvt2 → Ack` via `stage_finalize::run`).
///
/// **Per-state semantics**:
/// - `Sent` → CAS `Sent → Kvt1` + `OFFLINE_DRAIN_DOC_ADVANCED` audit
///   row atomically in one `with_immediate` envelope (LOW-C5-R1
///   atomicity fix 2026-05-21: audit-append-after-commit would leave
///   the doc advanced without forensic trail if audit append failed).
/// - `Kvt1` / `Kvt2` → no DB CAS; emit pool-bound audit (atomicity
///   not at risk — nothing else to be atomic with).  Pre-W12 drain
///   re-encounters these states across ticks (the walker scans
///   them); the stub records "still awaiting W12" without mutation.
///   Post-W12, this arm gains the `Kvt2 → Ack` advance via
///   `stage_finalize::run`.
/// - Other states → `BootError::Internal` (caller bug — the cohort
///   walker SELECT must filter to these states).
///
/// Single-writer invariant (App reconcile mutex + per-FN serialised
/// drain) makes non-`Applied` CAS unreachable in production — a
/// non-`Applied` outcome surfaces as `BootError::Internal` for
/// operator triage rather than silent miscounting.
///
/// `audit_payload` is the pre-built `OFFLINE_DRAIN_DOC_ADVANCED`
/// payload from the caller (carries dispatch-specific fields like
/// `dispatch_via`, `replay_short_circuit`, `attempt_no`, etc.).  The
/// helper emits the audit row inside the envelope to bind CAS +
/// audit atomicity.
///
/// `current_state` is the state the caller asserts the doc is in at
/// the moment of invocation (cohort-walker state for KVT1
/// w12-only path; `DocState::Sent` literal after a successful
/// `stage_send::run` Sent outcome OR after a lastChk replay HIT).
/// Helper routes the per-state CAS arm by this value — the caller
/// binds the contract at the call site.
///
/// `kvt1_raw_bytes` (HIGH-C5-2 fix 2026-05-21): when the caller
/// has the `lastChk` ack's `data_sign` evidence in hand (replay HIT
/// path), pass `Some(&ack.data_sign)` and the helper persists it
/// into `document_files::Kvt1Raw` INSIDE the same envelope as the
/// Sent→Kvt1 CAS + audit.  Matches the M3a `boot_phase::advance_
/// sent_to_kvt1_from_probe` evidence contract (forensic KVT1_RAW
/// per legal-trail requirements).  Pass `None` from the
/// stage_send::Sent path (no `data_sign` in hand pre-W12; W12 PR
/// closes this gap by routing all Sent advances through the
/// lastChk evidence path).  Ignored on the `Kvt1` arm.
async fn apply_w12_confirmation(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    current_state: DocState,
    audit_payload: &serde_json::Value,
    kvt1_raw_bytes: Option<&[u8]>,
) -> Result<W12ConfirmOutcome, BootError> {
    let id_hex = hex_lower(doc_id.as_bytes());
    match current_state {
        DocState::Sent => {
            let payload_owned = audit_payload.to_string();
            let id_hex_owned = id_hex.clone();
            let fn_for_internal = fiscal_number.to_string();
            let kvt1_raw_owned: Option<Vec<u8>> = kvt1_raw_bytes.map(|b| b.to_vec());
            with_immediate(pool, move |tx| {
                Box::pin(async move {
                    let outcome = fiscal_documents::transition_state(
                        tx,
                        doc_id,
                        DocState::Sent,
                        DocState::Kvt1,
                    )
                    .await?;
                    if outcome != TransitionOutcome::Applied {
                        return Err(anyhow::anyhow!(
                            "backlog_drain({fn_id}): apply_w12_confirmation CAS \
                             Sent→Kvt1 produced {outcome} for doc {doc_hex} \
                             (single-writer invariant breach)",
                            fn_id = fn_for_internal,
                            outcome = outcome_as_str(outcome),
                            doc_hex = id_hex_owned,
                        ));
                    }
                    if let Some(raw) = kvt1_raw_owned.as_deref() {
                        document_files::replace_tx(
                            tx,
                            doc_id,
                            document_files::DocumentFileKind::Kvt1Raw,
                            raw,
                        )
                        .await?;
                    }
                    audit_log::append_tx(
                        tx,
                        AUDIT_ENTITY_DOC,
                        &id_hex_owned,
                        "OFFLINE_DRAIN_DOC_ADVANCED",
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
            Ok(W12ConfirmOutcome::DeferredKvt1)
        }
        DocState::Kvt1 => {
            audit_log::append(
                pool,
                AUDIT_ENTITY_DOC,
                &id_hex,
                "OFFLINE_DRAIN_DOC_ADVANCED",
                Severity::Info,
                None,
                Some(&audit_payload.to_string()),
            )
            .await
            .map_err(BootError::Database)?;
            Ok(W12ConfirmOutcome::DeferredKvt1)
        }
        other => Err(BootError::Internal(format!(
            "backlog_drain({fiscal_number}): apply_w12_confirmation invoked on \
             unsupported state {state} for doc {doc_hex} (cohort walker should \
             only surface Sent/Kvt1 to this helper post MED-C5-4 KVT2 deferral)",
            state = other.as_str(),
            doc_hex = id_hex,
        ))),
    }
}

/// HIGH-C5-3 helper (2026-05-21): on a `lastChk` NotFound for a
/// SENT cohort doc, downgrade the doc to `ErrorRetryable` so the
/// next drain tick re-drives it through the W9a 4-pre source
/// whitelist (`stage_send::run`).  CAS + audit emit committed in
/// ONE `with_immediate` envelope.
///
/// The audit event reuses `OFFLINE_DRAIN_DOC_FAILED` (Warning) with
/// `failure_class="transport"` + `manual_recon_class=false` —
/// matches the spec amendment 2026-05-21 contract that TransientRetry-
/// class outcomes retain retry budget and do NOT halt pending-drain
/// shifts.  Distinguishable from genuine Transport-on-stage_send via
/// the `dispatch_via="lastchk_replay"` + `probe_outcome="NotFound"`
/// payload fields.
async fn downgrade_sent_to_error_retryable_for_retry(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    id_hex: &str,
    expected_server_fiscal_no: &str,
) -> Result<(), BootError> {
    let class_str = failure_class_for(FailureClass::Transport);
    let payload = serde_json::json!({
        "document_id": id_hex,
        "failure_class": class_str,
        "probe_outcome": "NotFound",
        "expected_server_fiscal_no": expected_server_fiscal_no,
        "downgrade_target_state": DocState::ErrorRetryable.as_str(),
        "manual_recon_class": false,
        "dispatch_via": "lastchk_replay",
    });
    let payload_owned = payload.to_string();
    let id_hex_owned = id_hex.to_string();
    let fn_for_internal = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let outcome = fiscal_documents::transition_state(
                tx,
                doc_id,
                DocState::Sent,
                DocState::ErrorRetryable,
            )
            .await?;
            if outcome != TransitionOutcome::Applied {
                return Err(anyhow::anyhow!(
                    "backlog_drain({fn_id}): downgrade Sent→ErrorRetryable CAS \
                     produced {outcome} for doc {doc_hex} (single-writer invariant \
                     breach)",
                    fn_id = fn_for_internal,
                    outcome = outcome_as_str(outcome),
                    doc_hex = id_hex_owned,
                ));
            }
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DOC,
                &id_hex_owned,
                "OFFLINE_DRAIN_DOC_FAILED",
                Severity::Warning,
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
    Ok(())
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
            "backlog_drain({fiscal_number}): shift_state={state} indicates pending-drain \
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
            let outcome = shifts::transition_state(tx, shift_id, from_state, to_state).await?;
            if let shifts::TransitionOutcome::Applied = outcome {
                mirror_node_state_shift_state_tx(
                    tx,
                    &fiscal_number_owned,
                    shift_id,
                    from_state,
                    to_state,
                )
                .await?;
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

/// `m3b-shift-state-expansion.md` §5 load-bearing mirror invariant:
/// `node_state.shift_state` MUST equal the active shifts row's state.
/// CAS-guarded UPDATE on `(fiscal_number, current_shift_id,
/// from_shift_state)` so a concurrent writer cannot smuggle a
/// different shift state past the mirror update.  Non-`Applied`
/// rows_affected surfaces as an anyhow chain inside the closure —
/// the surrounding `with_immediate` ROLLS BACK the entire escalation
/// tx (shift CAS, mirror UPDATE, audit emit) atomically.
async fn mirror_node_state_shift_state_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    shift_id: ShiftId,
    from_state: ShiftState,
    to_state: ShiftState,
) -> Result<(), anyhow::Error> {
    let rows_affected = sqlx::query(
        "UPDATE node_state SET shift_state = ? \
         WHERE fiscal_number = ? AND shift_state = ? AND current_shift_id = ?",
    )
    .bind(to_state)
    .bind(fiscal_number)
    .bind(from_state)
    .bind(shift_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows_affected != 1 {
        return Err(anyhow::anyhow!(
            "backlog_drain({fiscal_number}): node_state.shift_state mirror UPDATE \
             produced rows_affected={rows_affected} (expected 1; structural drift \
             between shifts and node_state for shift {shift_hex})",
            shift_hex = hex_lower(shift_id.as_bytes()),
        ));
    }
    Ok(())
}

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
            commit_finalize_envelope(pool, fiscal_number, session_id, ns, summary).await
        }
        FinalizeEligibility::NotEligible { reason } => {
            emit_partial(pool, fiscal_number, session_id, summary, &reason).await
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
    let payload_owned = payload.to_string();
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
                let shift_outcome =
                    shifts::transition_state(tx, shift_id, shift_state_from, target).await?;
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
                // Mirror node_state.shift_state per
                // m3b-shift-state-expansion.md §5 load-bearing
                // invariant.  Reuses C4's mirror helper.
                mirror_node_state_shift_state_tx(
                    tx,
                    &fiscal_number_owned,
                    shift_id,
                    shift_state_from,
                    target,
                )
                .await?;
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
            // (4) Audit OFFLINE_DRAIN_COMPLETED.
            audit_log::append_tx(
                tx,
                AUDIT_ENTITY_DRAIN_FN,
                &fiscal_number_owned,
                "OFFLINE_DRAIN_COMPLETED",
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
        StageSendError::OfflineFiscalNoMissing { .. } => FailureClass::OfflineFiscalNoMissing,
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

        commit_finalize_envelope(&pool, FN, session_id, &ns, &mut summary)
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

        commit_finalize_envelope(&pool, FN, session_id, &ns, &mut summary)
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

        commit_finalize_envelope(&pool, FN, session_id, &ns, &mut summary)
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
