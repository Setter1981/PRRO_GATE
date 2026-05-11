//! W9 boot reconciliation — per-FN dispatch + per-DocState helpers.
//!
//! Per design freeze §4 + §5.4:
//! - W9.2 (this slice) lands 3 per-DocState helpers + `MAX_BOOT_ATTEMPTS`
//!   constant + a stub `run_boot_reconciliation` returning `Ok(())`.
//! - W9.3 wires the 6-branch decision tree on top.
//!
//! All helpers wrap exactly ONE `with_immediate` envelope each (W3
//! single-writer invariant; no foreign IO inside).  CAS state
//! transitions use direct UPDATE SQL inside the envelope because the
//! existing `fiscal_documents::transition_state` is pool-bound — for
//! W9.2 boot helpers we need tx-bound CAS to land alongside audit +
//! artifact writes atomically.

use std::fmt::Write as _;

use sqlx::SqlitePool;

use crate::db::models::ids::DocumentId;
use crate::db::repositories::{audit_log, document_files, transport_trace};
use crate::db::tx::with_immediate;
use crate::transports::dps::dto::CheckAck;

/// Local hex helper — duplicates `services::write_path::types::hex_encode_lower`
/// (pub(super)-scoped within write_path/).  Tiny enough not to warrant
/// cross-module re-exposure for a single audit-payload use case.
fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Per W9 freeze §4.0: budget cap for `attempts_used(doc_id)` →
/// `RequiresManualReconciliation` escalation in §4.8 ERROR_RETRYABLE
/// pre-check.  Mirrors W0-3 §2 policy "retry up to
/// max_recovery_attempts=5".
pub const MAX_BOOT_ATTEMPTS: i64 = 5;

/// W9 freeze §4.4 — `Sending` crash-resume helper.
///
/// Single `with_immediate` envelope containing a CAS
/// `Sending → ErrorRetryable` and an audit
/// `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE` ERROR.  No DPS call (DPS
/// doesn't deduplicate; re-sending would create a duplicate-doc
/// hazard per ADR-M3-A5 / W0-2 §5.2).
///
/// **Idempotency.**  CAS guard `WHERE state = 'SENDING'` makes a
/// second invocation a no-op (the first call moved state to
/// `ErrorRetryable`; second sees no rows).
///
/// **Outcome shape.**  `Ok(true)` → CAS applied, audit appended.
/// `Ok(false)` → CAS didn't apply (doc not in Sending — already
/// resumed by prior boot tick OR state changed by parallel writer;
/// under single-writer-per-FN the latter cannot occur within boot).
pub async fn resume_sending_to_error_retryable(
    pool: &SqlitePool,
    doc_id: DocumentId,
) -> anyhow::Result<bool> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let res = sqlx::query(
                "UPDATE fiscal_documents SET state = 'ERROR_RETRYABLE' \
                 WHERE document_id = ? AND state = 'SENDING'",
            )
            .bind(doc_id)
            .execute(&mut **tx)
            .await?;
            let applied = res.rows_affected() == 1;
            if applied {
                let payload = serde_json::json!({
                    "document_id": hex_lower(doc_id.as_bytes()),
                    "branch": "c-sending",
                    "rationale":
                        "DPS does not deduplicate; re-sending would be duplicate-document hazard",
                });
                audit_log::append_tx(
                    tx,
                    "fiscal_document",
                    &hex_lower(doc_id.as_bytes()),
                    "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE",
                    crate::db::models::enums::Severity::Error,
                    None,
                    Some(&payload.to_string()),
                )
                .await?;
            }
            Ok::<bool, anyhow::Error>(applied)
        })
    })
    .await
}

/// W9 freeze §4.5 (HIGH 1+5+8+9 fixes consolidated) — advance a
/// `Sent` doc to `Kvt1` after the `last_chk_probe::probe` matched.
///
/// Single `with_immediate` envelope, four atomic writes:
///   1. CAS `Sent → Kvt1` on `fiscal_documents`.
///   2. `document_files::replace_tx(doc_id, Kvt1Raw, ack.data_sign)`
///      — forensic record of the receipt bytes (W9 captures these
///      even though live W7 `Sent → Kvt1` path doesn't yet; freeze
///      HIGH 5 asymmetry note).
///   3. `transport_trace::complete_via_recovery_tx` — completes the
///      in-flight trace row with `outcome_kind = 'OK'` (HIGH 8) and
///      the probe's wire times (HIGH 9 — schema all-or-none CHECK
///      forces wire times non-NULL on completion).
///   4. Audit `BOOT_LAST_CHK_MATCH_KVT1` INFO.
///
/// **`attempt_no` parameter.**  Caller MUST pass the in-flight
/// trace row's `attempt_no` (looked up before calling, since W9.2
/// helpers don't read transport_trace inside `with_immediate`).
/// W9.3 dispatch fetches this via `transport_trace::list_for`
/// filtering on `completed_at IS NULL`.
///
/// **CAS conflict shape.**  Returns `Ok(false)` if the `Sent → Kvt1`
/// CAS didn't apply (doc no longer in Sent — already advanced by
/// parallel writer OR by prior boot tick).  In that case the
/// envelope rolls back (none of the 4 writes commit).  Caller sees
/// `Ok(false)` and decides to skip / escalate.
pub async fn advance_sent_to_kvt1_from_probe(
    pool: &SqlitePool,
    doc_id: DocumentId,
    attempt_no: i32,
    ack: &CheckAck,
    wire_call_started_at: &str,
    wire_call_finished_at: &str,
) -> anyhow::Result<bool> {
    let ack_id = ack.id.clone();
    let ack_data_sign = ack.data_sign.clone();
    let wire_started = wire_call_started_at.to_string();
    let wire_finished = wire_call_finished_at.to_string();
    with_immediate(pool, move |tx| {
        let ack_id = ack_id.clone();
        let ack_data_sign = ack_data_sign.clone();
        let wire_started = wire_started.clone();
        let wire_finished = wire_finished.clone();
        Box::pin(async move {
            // (1) CAS Sent → Kvt1.
            let cas = sqlx::query(
                "UPDATE fiscal_documents SET state = 'KVT1' \
                 WHERE document_id = ? AND state = 'SENT'",
            )
            .bind(doc_id)
            .execute(&mut **tx)
            .await?;
            if cas.rows_affected() != 1 {
                // CAS conflict — leave envelope unchanged (RAII rollback
                // via `?` on the next steps would also work, but bail
                // out early to avoid emitting a partial trail).
                return Ok::<bool, anyhow::Error>(false);
            }

            // (2) Persist KVT1_RAW from ack.data_sign.
            document_files::replace_tx(
                tx,
                doc_id,
                document_files::DocumentFileKind::Kvt1Raw,
                &ack_data_sign,
            )
            .await?;

            // (3) Complete transport_trace row via recovery helper.
            let n_completed = transport_trace::complete_via_recovery_tx(
                tx,
                doc_id,
                attempt_no,
                &ack_id,
                &wire_started,
                &wire_finished,
            )
            .await?;
            if n_completed != 1 {
                // In-flight row missing or already completed.  Surface
                // as anyhow error → envelope rolls back; caller observes
                // failure and can escalate (W9.3 handling).
                anyhow::bail!(
                    "transport_trace recovery completion: rows_affected = {n_completed} \
                     (expected 1; doc {doc_id:?}, attempt_no = {attempt_no})"
                );
            }

            // (4) Audit BOOT_LAST_CHK_MATCH_KVT1 INFO.
            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-sent",
                "ack_id": ack_id,
                "attempt_no": attempt_no,
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_LAST_CHK_MATCH_KVT1",
                crate::db::models::enums::Severity::Info,
                None,
                Some(&payload.to_string()),
            )
            .await?;

            Ok::<bool, anyhow::Error>(true)
        })
    })
    .await
}

/// W9 freeze §4.6 (HIGH 6 fix option A) — passive hold for `Kvt1`
/// docs.  No DPS call; no state mutation; no `transport_trace`
/// write.  Single audit-row INSERT documenting that active KVT2
/// polling is deferred to M3b.
///
/// **Why no state change.**  Per W0-1 §2.1, `Kvt1 → Kvt2` requires
/// authoritative DPS evidence (the second receipt).  M3a's
/// `DpsChannel` trait doesn't yet expose a 2nd-receipt API
/// (`status_rro` returns RRO-wide state, not per-doc KVT2).  Until
/// M3b lands active polling, recovery for `Kvt1` is observation-
/// only.  Operator-driven manual reconciliation is the escape
/// hatch.
pub async fn passive_hold_kvt1(pool: &SqlitePool, doc_id: DocumentId) -> anyhow::Result<()> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-kvt1",
                "deferred_to": "M3b active KVT2 polling",
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_KVT1_HOLD_DEFERRED",
                crate::db::models::enums::Severity::Info,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
}

/// W9.3 stub — per-FN decision tree dispatch (per freeze §3 +
/// §4.3).  W9.2 ships this as `Ok(())` no-op so the module surface
/// is callable from `App::reconcile_pending`.  W9.3 fills in the
/// 6-branch dispatch.
pub async fn run_boot_reconciliation(
    _pool: &SqlitePool,
    _fiscal_number: &str,
) -> anyhow::Result<()> {
    // TODO(W9.3): 6-branch dispatch per freeze §3 + §4.
    Ok(())
}
