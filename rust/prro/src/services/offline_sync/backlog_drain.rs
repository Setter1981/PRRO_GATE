//! W9b backlog drain orchestration — types, API-level invariants, and
//! orchestrator skeleton (Commits 2-3).
//!
//! This module is shipped in two phases:
//!
//! - **Commit 2** — pure types: [`W12ConfirmOutcome`] typed seam,
//!   [`DrainSummary`] with private counters + invariant-enforcing
//!   API, [`FinalizeEligibility`] decision enum, [`failure_class_for`]
//!   stable-string taxonomy.
//! - **Commit 3 (this file, [`drain`])** — orchestrator skeleton: pure-
//!   function entry that reads prerequisites (mode, backlog, offline
//!   session state), transitions session Open → Draining via inline
//!   `with_immediate` envelope, emits `OFFLINE_DRAIN_STARTED` audit.
//!   **No per-doc loop yet** — Commit 4 wires that.
//! - **Commits 4-7** — per-doc loop, lastChk pre-flight, finalization
//!   branch, App entry.
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
use crate::db::models::enums::{NodeMode, OfflineSessionState, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::{audit_log, fiscal_documents, node_state, offline_sessions};
use crate::db::tx::with_immediate;
use crate::services::reconciliation::runtime::RuntimeView;
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

// ─── C3: orchestrator skeleton (prerequisites + audit emit) ──────────

/// Audit entity_type for FN-scoped drain lifecycle events
/// (`OFFLINE_DRAIN_*`).  Mirrors the `return_online_probe` convention
/// — drain is an FN-scoped operation, indexed by `fiscal_number`.
/// Per-session events (`OFFLINE_SESSION_DRAIN_STARTED`) continue to
/// use `entity_type = "offline_session"` per W5 convention.
const AUDIT_ENTITY_DRAIN_FN: &str = "node_state";

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
/// `_deps` is the per-FN runtime bundle (`dps`, `signing_ctx`,
/// `fn_sign`).  Unused in the prerequisites pass — drain skeleton
/// must never reach the wire (I1) — but pinned in the signature
/// so the per-doc loop (which DOES consume it) can be added without
/// churning the entry shape (operator pin: signature-stable entry).
///
/// ## Return contract
///
/// On prerequisite skip (mode ≠ `GoingOnline` OR empty backlog) the
/// returned [`DrainSummary`] has `backlog_size_before = 0`; audit
/// `OFFLINE_DRAIN_SKIPPED_*` row is emitted to the entity row
/// `(AUDIT_ENTITY_DRAIN_FN, fiscal_number)`.  Otherwise the summary
/// carries `backlog_size_before = backlog.len()` but ZERO advance /
/// failure counters (per-doc loop deferred).  Caller observing this
/// intermediate result MUST NOT treat the chain as finalized — the
/// `OFFLINE_DRAIN_STARTED` audit has fired but `OFFLINE_DRAIN_
/// COMPLETED|PARTIAL` has not, and `finalize_eligibility` returns
/// `NotEligible{AckCountMismatch}` by construction (acked == 0,
/// backlog_size_before > 0).
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
    _deps: &RuntimeView<'_>,
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

    // Per-doc loop / lastChk pre-flight / finalization deferred to
    // C4-C6.  Return a summary carrying the backlog size; counters
    // are zero — by construction `finalize_eligibility` returns
    // `NotEligible{AckCountMismatch}` so any caller that misuses
    // this C3-intermediate result as a final summary cannot finalize.
    Ok(DrainSummary::new(fiscal_number.to_string(), backlog.len()))
}
