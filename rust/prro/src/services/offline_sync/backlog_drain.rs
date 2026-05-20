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
//! - **Commits 5-7** — widen walker to the full unfinished cohort
//!   (`OFFLINE_LOCAL_ACK | SENT | KVT1 | KVT2 | ERROR_RETRYABLE`),
//!   add lastChk pre-flight, extract the inline `Sent → Kvt1` into
//!   the `apply_w12_confirmation` helper, add the finalization
//!   branch, and add the App entry.
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
//! `state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','KVT2','ERROR_
//! RETRYABLE')` and dispatching by `doc.state` (ErrorRetryable →
//! stage_send re-drive via W9a 4-pre source whitelist).  C5 is
//! a blocker before any "C4 approved" verdict at the PR level.
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
use crate::db::models::ids::{DocumentId, ShiftId};
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::{audit_log, fiscal_documents, node_state, offline_sessions, shifts};
use crate::db::tx::{with_immediate, WriteTxConn};
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
    pub fn record_doc_advanced(
        &mut self,
        outcome: &W12ConfirmOutcome,
        via_lastchk_replay: bool,
    ) {
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
            FinalizeEligibility::NotEligible { reason } => {
                Err(FinalizeError::NotEligible(reason))
            }
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
pub async fn drain(
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

    // ─── Step 2: read backlog (strict lnd ASC) ───────────────────────
    let backlog =
        fiscal_documents::list_offline_local_ack_for_fn_ordered_by_lnd(pool, fiscal_number)
            .await
            .map_err(BootError::Database)?;

    if backlog.is_empty() {
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "current_mode": ns.mode.as_str(),
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

    // ─── Step 3: read active offline session (OPEN|DRAINING) ─────────
    //
    // Backlog non-empty ⇒ an active session MUST exist (W7's
    // stage_offline_ack stamps `offline_session_id` on every
    // OFFLINE_LOCAL_ACK doc + W4's partial UNIQUE keeps at most one
    // active session per FN).  Missing session is a structural drift
    // — bail with Internal so the operator sees the signal.
    let (session_id, session_state) =
        offline_sessions::current_open_or_draining_session(pool, fiscal_number)
            .await
            .map_err(BootError::Database)?
            .ok_or_else(|| {
                BootError::Internal(format!(
                    "backlog_drain({fiscal_number}): backlog of {n} OFFLINE_LOCAL_ACK docs but no active session (OPEN|DRAINING).  Structural drift — investigate offline_sessions consistency.",
                    n = backlog.len(),
                ))
            })?;

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

/// Process exactly one doc through `stage_send::run` + per-outcome
/// routing.  Mutations are: append into [`DrainSummary`] (advance or
/// failure) + one audit row (`OFFLINE_DRAIN_DOC_ADVANCED` /
/// `_DOC_FAILED`).
///
/// **Sent path (MED-C4-3 fix, 2026-05-21)**: after `stage_send::run`
/// returns `Sent`, this helper inlines a `Sent → Kvt1` CAS via the
/// typed W12 stub so the persisted DB state, `advanced_to_kvt1`
/// counter, and audit `to_state="KVT1"` all stay consistent within
/// a single C4-only flow.  C5 will extract the inline transition
/// into the `apply_w12_confirmation` helper and add the `lastChk`
/// pre-flight branch — same `DeferredKvt1` outcome, same counter
/// semantics, same audit shape.
///
/// **Failure paths**: every non-Sent outcome surfaces as
/// `DocVerdict::Failed { class_str }` so [`drain`] can detect the
/// pending-drain shift halt condition.  Sibling-continue on
/// non-pending-drain shifts; halt + Manual escalation otherwise
/// (handled at the [`drain`] level, NOT here).
///
/// Only infrastructure failures propagate (audit append sqlx error,
/// post-stage_send Sent→Kvt1 CAS non-Applied, node_state shift_state
/// mirror UPDATE drift during pending-drain escalation).
async fn process_one_doc(
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
            // C4 inline W12 stub: advance Sent → Kvt1 atomically so
            // DB state matches the `advanced_to_kvt1` counter and the
            // audit `to_state="KVT1"`.  Pre-W12 stub return is always
            // `DeferredKvt1`; C5 extracts this into the
            // `apply_w12_confirmation` helper without changing the
            // counter semantics or audit shape.
            let w12 = apply_w12_stub_sent_to_kvt1(pool, fiscal_number, doc.document_id).await?;
            summary.record_doc_advanced(&w12, false);
            let payload = serde_json::json!({
                "document_id": id_hex,
                "from_state": DocState::OfflineLocalAck.as_str(),
                "to_state": DocState::Kvt1.as_str(),
                "replay_short_circuit": false,
                "attempt_no": attempt_no,
                "server_fiscal_no": server_fiscal_no,
                "w12_status": w12.w12_status_str(),
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

/// W9b C4 inline W12 stub — advance `Sent → Kvt1` atomically in its
/// own `with_immediate` envelope and return `DeferredKvt1`.  Mirrors
/// what the C5 `apply_w12_confirmation` stub will encapsulate; the
/// inline form lives in C4 so per-doc DB state matches the audit
/// + counter semantics within a single C4 flow.
///
/// Single-writer invariant (App reconcile mutex + per-FN serialised
/// drain) makes non-`Applied` CAS unreachable in production — a
/// non-`Applied` outcome surfaces as `BootError::Internal` for
/// operator triage rather than silent miscounting.
async fn apply_w12_stub_sent_to_kvt1(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
) -> Result<W12ConfirmOutcome, BootError> {
    let outcome = with_immediate(pool, move |tx| {
        Box::pin(async move {
            let outcome = fiscal_documents::transition_state(
                tx,
                doc_id,
                DocState::Sent,
                DocState::Kvt1,
            )
            .await?;
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
            "backlog_drain({fiscal_number}): post-stage_send W12 stub CAS Sent→Kvt1 \
             produced {outcome:?} for doc {doc_hex} (single-writer invariant breach)",
            doc_hex = hex_lower(doc_id.as_bytes()),
        )));
    }
    Ok(W12ConfirmOutcome::DeferredKvt1)
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
            let outcome =
                shifts::transition_state(tx, shift_id, from_state, to_state).await?;
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
