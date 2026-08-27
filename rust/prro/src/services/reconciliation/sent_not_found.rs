//! CS-3 Slice 6 — atomic `Sent + NotFound` escalation (design §3.5).
//!
//! A document that is durably `SENT` (issued-but-unconfirmed) but that a recovery / drain probe
//! reports the DPS does **not** have (`last_chk` / `status_rro` → NotFound) must NOT be blindly
//! redriven — the old blind-resend is the double-issue hazard this whole campaign removes. Instead
//! it escalates to manual reconciliation. The design §3.5 sequence, all in the caller's single
//! `with_immediate` (tx-bound repository functions ONLY — a pool-bound escalation after the document
//! CAS would open a crash window):
//!
//! ```text
//! document Sent -> RequiresManualReconciliation   (this helper)
//! node_state.mode -> STOP_MODE                    (this helper)
//! transport_trace complete                        (the PRODUCER — needs the probe's own data)
//! audit_log append (Critical)                     (this helper)
//! ```
//!
//! This helper owns the three fiscal-critical, producer-agnostic writes. The `transport_trace`
//! completion needs the recovery probe's own timing / outcome (`wire_call_started/finished`,
//! `outcome_kind` — the table's CHECK requires them together), which only the producer holds, so it
//! completes that trace via `transport_trace::complete_via_recovery_tx` / `complete_tx` in the SAME
//! transaction at the cutover (Slice 7).
//!
//! The `STOP_MODE` write is essential and is what §3.5 adds over the pre-existing bare Sent→RMR
//! escalation: the SENT document leaves the per-FN cohort, so without STOP a *successor* could
//! wrongly become the chain head — a fork. It deliberately does NOT use
//! `shifts::force_to_manual_reconciliation_with_audit` (that does not update the node mirror and
//! shift-RMR has no exit).
//!
//! **LIVE since the S7-1 cutover (#336):** called by the boot/convergence shared path
//! (`boot_phase.rs:967`) and the offline drain path (`kvt2_confirm.rs:1688`).

use crate::db::models::enums::{DocState, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::{audit_log, fiscal_documents, node_state};
use crate::db::tx::WriteTxConn;

/// Why [`sent_not_found_to_manual`] could not escalate (nothing is mutated on any of these).
#[derive(Debug, thiserror::Error)]
pub enum SentNotFoundError {
    /// The document is not in `SENT` (the escalation edge `Sent -> RMR` did not apply).
    #[error("document is not in SENT (cannot escalate to manual): {0:?}")]
    DocNotSent(TransitionOutcome),
    /// No `node_state` row for the fiscal number.
    #[error("node_state row missing")]
    NodeStateMissing,
    /// A lower-level SQLite error.
    #[error(transparent)]
    Db(sqlx::Error),
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Atomically escalate a SENT-but-NotFound document to manual reconciliation (design §3.5).
///
/// In the caller's single `BEGIN IMMEDIATE`, in order: `Sent -> RequiresManualReconciliation`;
/// node `STOP_MODE`; complete any still-open transport-trace row for the document; append a Critical
/// audit. Any failure (document not SENT, missing node row, SQLite error) returns an error so the
/// whole transaction rolls back — all four writes commit together or none do.
pub async fn sent_not_found_to_manual(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    fiscal_number: &str,
) -> Result<(), SentNotFoundError> {
    // 1. document Sent -> RequiresManualReconciliation.
    let outcome = fiscal_documents::transition_state(
        tx,
        doc_id,
        DocState::Sent,
        DocState::RequiresManualReconciliation,
    )
    .await
    .map_err(SentNotFoundError::Db)?;
    if !matches!(outcome, TransitionOutcome::Applied) {
        return Err(SentNotFoundError::DocNotSent(outcome));
    }

    // 2. node_state.mode -> STOP_MODE (protects a successor from becoming a fork head).
    let ok = node_state::set_mode_stop_mode_tx(tx, fiscal_number)
        .await
        .map_err(SentNotFoundError::Db)?;
    if !ok {
        return Err(SentNotFoundError::NodeStateMissing);
    }

    // The `transport_trace complete` of §3.5 is the PRODUCER's write, not this helper's: the
    // recovery-trace completion needs the probe's own timing / outcome (wire_call_started/finished,
    // outcome_kind — required together by the `transport_trace` CHECK), which only the producer
    // (`boot_phase` / `kvt2_confirm`, at the cutover) holds. It completes that recovery trace via
    // `transport_trace::complete_via_recovery_tx` / `complete_tx` in the SAME `with_immediate`,
    // atomically with the three writes here. Fabricating a bare `completed_at` would violate that
    // CHECK. This helper owns the three fiscal-critical, producer-agnostic writes.

    // 3. audit_log append (Critical) — the operator-handoff record.
    let entity_id = hex_lower(doc_id.as_bytes());
    audit_log::append_tx(
        tx,
        "fiscal_document",
        &entity_id,
        "SENT_NOT_FOUND_ESCALATED_MANUAL",
        Severity::Critical,
        None,
        Some("{\"reason\":\"sent_not_found\",\"slice\":\"cs3-6\"}"),
    )
    .await
    .map_err(SentNotFoundError::Db)?;

    Ok(())
}
