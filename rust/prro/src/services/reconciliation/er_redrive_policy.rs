//! Shared ErrorRetryable redrive-vs-escalate policy.
//!
//! Originally lived inline in
//! [`super::boot_phase::dispatch_error_retryable_by_class`] (M3a
//! hardening pass 1, PR #38).  Extracted in M3b W9b ER-class-guard so
//! the W9b backlog drain
//! ([`crate::services::offline_sync::backlog_drain`]) can apply the
//! same policy when re-driving `ErrorRetryable` docs through
//! [`crate::services::write_path::stage_send::run`].  Failing to apply
//! it from drain context would violate the stage_send caller obligation
//! documented at `stage_send.rs:18-61` (R-W10.2-review HIGH 1):
//! re-invoking `stage_send::run` on a non-`TransientRetry` ER doc
//! produces an unbounded crash-loop on every drain tick.
//!
//! The policy is intentionally a **pure decision** — no DB writes, no
//! audit emission.  Each caller projects the decision into its own
//! `with_immediate` envelope and audit taxonomy:
//!   - boot dispatcher emits `BOOT_ER_*` events + boot histogram counters;
//!   - drain dispatcher emits `OFFLINE_DRAIN_*` events + drain summary.
//!
//! Mirrors the operator-confirmed scope for M3b W9b ER class guard
//! (2026-05-22): `TerminalReject` is treated as a structural inconsistency
//! arm (the routing module lands TerminalReject directly into `Rejected`,
//! so observing it as an ER attempt class means routing skew); `None`
//! preserves boot semantics (`HoldIndeterminate`, non-manual class —
//! reclassification to manual is a separate spec decision out of scope
//! here).

use sqlx::SqlitePool;

use crate::db::models::ids::DocumentId;
use crate::db::repositories::transport_trace;
use crate::services::reconciliation::boot_phase::MAX_BOOT_ATTEMPTS;
use crate::services::write_path::error_routing::RetryClass;

/// Closed-enum verdict for an `ErrorRetryable` doc seen at boot or
/// drain time.  Caller selects projection by inspecting the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErRedriveDecision {
    /// Last attempt was `RetryClass::TransientRetry` AND
    /// `attempts_used < MAX_BOOT_ATTEMPTS`.  Caller is authorized to
    /// re-invoke `stage_send::run` (Pattern B retry path).
    Redrive,

    /// Last attempt was `RetryClass::TransientRetry` AND
    /// `attempts_used >= MAX_BOOT_ATTEMPTS`.  Caller MUST escalate to
    /// `DocState::RequiresManualReconciliation` with `Severity::Error`;
    /// audit payload carries `attempts_used` for forensics.  Without
    /// this gate a doc that keeps failing every tick would re-burn DPS
    /// quota indefinitely (W9 freeze §4.0).
    BudgetExhausted { attempts_used: i64 },

    /// Last attempt was an operator-actionable durable class
    /// (`FnConfigError` / `WrapperBug` / `OperatorEscalation` /
    /// `MacRecovery`).  Caller MUST escalate to
    /// `RequiresManualReconciliation` with `Severity::Error`.
    EscalateManual { class: RetryClass },

    /// Last attempt was `RetryClass::TerminalReject`.  Structurally
    /// inconsistent — `error_routing::route_dps_error` lands
    /// TerminalReject directly in `Rejected`, never in `ErrorRetryable`.
    /// Observing this combination is durable evidence of a routing
    /// skew.  Caller MUST escalate with `Severity::Critical`.
    EscalateInconsistent { class: RetryClass },

    /// Last attempt was `RetryClass::ProbeRequired` (Decode /
    /// -2/-15 close-shift).  Caller holds (no DB mutation); emits a
    /// Warning audit row noting the deferral.  Submit-time `last_chk`
    /// reconciliation is deferred to M5 generic SENDING reconciler.
    HoldProbeRequired,

    /// No durable retry class is recorded — transport_trace row is
    /// missing OR `retry_class` is NULL OR the persisted string is
    /// unrecognized by [`RetryClass::from_wire_str`].  Caller holds
    /// (no DB mutation); emits an Error-severity audit (durable evidence
    /// missing, operator triage required).
    HoldIndeterminate,
}

/// Read the doc's last-attempt `retry_class` from `transport_trace`
/// and (only when relevant) its `attempts_used`, then return the
/// policy verdict.  Pure decision — no DB writes, no audit.
///
/// `attempts_used` is read ONLY for `Some(TransientRetry)` to keep the
/// pool reads minimal in the hot path of the dispatcher.
pub async fn evaluate_er_redrive(
    pool: &SqlitePool,
    doc_id: DocumentId,
) -> sqlx::Result<ErRedriveDecision> {
    let class = transport_trace::last_attempt_retry_class_for(pool, doc_id).await?;
    match class {
        Some(RetryClass::TransientRetry) => {
            let attempts = transport_trace::attempts_used(pool, doc_id).await?;
            if attempts >= MAX_BOOT_ATTEMPTS {
                Ok(ErRedriveDecision::BudgetExhausted {
                    attempts_used: attempts,
                })
            } else {
                Ok(ErRedriveDecision::Redrive)
            }
        }
        // Legacy-only B10 tag: it was written by a withdrawn `-8` retry
        // experiment.  Preserve decoding so historical rows cannot become an
        // indeterminate hold, but never re-send their persisted bytes.
        Some(
            rc @ (RetryClass::FnConfigError
            | RetryClass::WrapperBug
            | RetryClass::OperatorEscalation
            | RetryClass::MacRecovery
            | RetryClass::DrainChainSettleRetry),
        ) => Ok(ErRedriveDecision::EscalateManual { class: rc }),
        Some(RetryClass::TerminalReject) => Ok(ErRedriveDecision::EscalateInconsistent {
            class: RetryClass::TerminalReject,
        }),
        Some(RetryClass::ProbeRequired) => Ok(ErRedriveDecision::HoldProbeRequired),
        None => Ok(ErRedriveDecision::HoldIndeterminate),
    }
}
