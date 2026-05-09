//! Stage 1+2 — acquire+validate+guard.
//!
//! Per W0-1 §3.1–§3.2 + ADR-M3-A1 (lnd sequencer) + ADR-M3-A5
//! (Pattern A — pure DB stage) + ADR-M3-A7 (App::boot interaction).
//!
//! One `with_immediate` envelope per request.  All operations inside
//! the lock are pure DB — no `CryptoProvider`, no `DpsChannel`, no
//! `spawn_blocking` (W3 static scan + runtime guard enforce this).

use anyhow::Context;

use crate::db::models::enums::{DocType, NodeMode, Severity, ShiftState};
use crate::db::models::ids::{DocumentId, RequestId};
use crate::db::repositories::{
    audit_log,
    fiscal_documents::{self as fd, NewDocument},
    ingress_inbox, node_state, shifts,
};
use crate::db::tx::with_immediate;
use sqlx::SqlitePool;

use super::types::{CanonicalFiscalCommand, RejectionReason, WorkerContext, WorkerProcessResult};

/// Public stage-1 entry.  Opens one `with_immediate` envelope and
/// runs the lease + guard + lnd-allocate + INSERT PREPARED + audit
/// sequence atomically.  See module-level docs for ADR anchors.
///
/// Invariants:
/// - On `Proceed`: lease=PROCESSING, lnd advanced, fiscal_documents
///   row PREPARED, audit `doc_prepared` appended.  All committed
///   together.
/// - On `Resumed`: lease=PROCESSING, NO lnd advance, NO new fiscal_
///   documents row (existing pending row reused), audit
///   `resume_detected` appended.
/// - On `Noop`: NO state mutation; tx commits an empty diff.
/// - On `Rejected`: ingress_inbox.status=REJECTED + audit row
///   appended.  NO fiscal_documents row, NO lnd advance.
pub async fn run(
    pool: &SqlitePool,
    request_id: [u8; 16],
    command: CanonicalFiscalCommand,
) -> anyhow::Result<WorkerProcessResult> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // [Step 1] Acquire lease — CAS NEW → PROCESSING.
            let inbox = match ingress_inbox::acquire_lease(tx, &request_id).await? {
                Some(row) => row,
                None => return Ok(WorkerProcessResult::Noop),
            };
            let fn_id = inbox.fiscal_number.clone();

            // [Step 2] Snapshot node_state inside the same tx.
            let node_state = node_state::get_tx(tx, &fn_id)
                .await?
                .with_context(|| format!("node_state row missing for fn={fn_id}"))?;

            // [Step 3a] Fast-path: NodeMode != Online → reject.
            if node_state.mode != NodeMode::Online {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::NodeOffline,
                    "node_offline_reject",
                    Severity::Warning,
                )
                .await;
            }

            // [Step 3b] Profile-binding guard — schema permits NULL,
            // submissions cannot proceed without resolved bindings.
            if node_state.backend_profile_id.is_none()
                || node_state.transport_profile_id.is_none()
            {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::MissingProfileBinding,
                    "profile_binding_missing",
                    Severity::Warning,
                )
                .await;
            }

            // [Step 4] Shift-state guard — keyed on doc_type.
            if let Some(reason) = check_shift_guard(command.doc_type, node_state.shift_state) {
                return reject(
                    tx,
                    &request_id,
                    reason,
                    "guard_rejected",
                    Severity::Warning,
                )
                .await;
            }

            // [Step 5] Resolve active_shift via node_state.current_shift_id.
            //          Single source of truth; no scan-for-latest-open.
            let active_shift = match (node_state.shift_state, &node_state.current_shift_id) {
                (ShiftState::Opened, Some(shift_id)) => {
                    let row = shifts::get_tx(tx, *shift_id).await?;
                    match row {
                        Some(s) if s.state == ShiftState::Opened => Some(s),
                        _ => {
                            // shift_state=Opened but resolved row missing or not Opened.
                            return reject(
                                tx,
                                &request_id,
                                RejectionReason::ShiftInvariantViolation,
                                "shift_invariant_violation",
                                Severity::Critical,
                            )
                            .await;
                        }
                    }
                }
                (ShiftState::Opened, None) => {
                    // shift_state=Opened but current_shift_id IS NULL.
                    return reject(
                        tx,
                        &request_id,
                        RejectionReason::ShiftInvariantViolation,
                        "shift_invariant_violation",
                        Severity::Critical,
                    )
                    .await;
                }
                _ => None,
            };

            // [Step 6] Resume-detect — reuse existing pending doc.
            let request_id_typed = RequestId::from_bytes(request_id);
            if let Some(existing) = fd::get_by_request_id_tx(tx, &request_id_typed).await? {
                audit_log::append_tx(
                    tx,
                    "fiscal_document",
                    &format!("{:?}", existing.document_id),
                    "resume_detected",
                    Severity::Info,
                    None,
                    Some(&format!(
                        r#"{{"request_id":"{request_id_hex}","existing_state":"{state}","lnd":{lnd}}}"#,
                        request_id_hex = hex_encode(&request_id),
                        state = existing.state.as_str(),
                        lnd = existing.lnd
                    )),
                )
                .await?;
                return Ok(WorkerProcessResult::Resumed(WorkerContext {
                    inbox,
                    command,
                    node_state,
                    active_shift,
                    document: existing,
                }));
            }

            // [Step 7] Allocate lnd atomically (UPDATE ... RETURNING).
            let lnd = node_state::allocate_next_lnd(tx, &fn_id).await?;

            // [Step 8] INSERT fiscal_documents (state=PREPARED).
            //          Profile bindings unwrap-safe — guarded at Step 3b.
            let backend_profile_id = node_state
                .backend_profile_id
                .clone()
                .expect("profile-binding guard ensures Some");
            let transport_profile_id = node_state
                .transport_profile_id
                .clone()
                .expect("profile-binding guard ensures Some");
            let document_id = DocumentId::new();
            let new_doc = NewDocument {
                document_id,
                request_id: request_id_typed,
                fiscal_number: fn_id.clone(),
                shift_id: active_shift.as_ref().map(|s| s.shift_id),
                offline_session_id: None,
                lnd,
                doc_type: command.doc_type,
                backend_profile_id,
                transport_profile_id,
                fs_mode: "ONLINE",
                business_ts: command.business_ts.clone(),
                total_sum_kop: command.total_sum_kop,
                payload_json: command.payload_json.clone(),
                payload_sha256_canonical: command.payload_sha256_canonical,
                unsigned_xml_sha256: None,
                previous_hash: None,
            };
            fd::insert_prepared_tx(tx, &new_doc).await?;

            // [Step 9] Audit success.
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{document_id:?}"),
                "doc_prepared",
                Severity::Info,
                None,
                Some(&format!(
                    r#"{{"request_id":"{}","lnd":{lnd},"doc_type":"{doc_type}"}}"#,
                    hex_encode(&request_id),
                    doc_type = command.doc_type.as_str()
                )),
            )
            .await?;

            // [Step 10] Build the freshly-inserted DocumentRow snapshot
            //          to hand to the next stage.  We could re-SELECT,
            //          but the values we just INSERTed are deterministic.
            let document = fd::DocumentRow {
                document_id,
                fiscal_number: fn_id.clone(),
                lnd,
                state: crate::db::models::enums::DocState::Prepared,
                doc_type: command.doc_type,
                server_fiscal_no: None,
                submission_attempted_at: None,
                backend_profile_id: new_doc.backend_profile_id.clone(),
                transport_profile_id: new_doc.transport_profile_id.clone(),
            };

            Ok(WorkerProcessResult::Proceed(WorkerContext {
                inbox,
                command,
                node_state,
                active_shift,
                document,
            }))
        })
    })
    .await
}

/// Apply a guard rejection: mark inbox REJECTED + append audit row.
/// No `fiscal_documents` row, no lnd allocation — the inbox carries
/// the rejection.
async fn reject(
    tx: &mut crate::db::tx::WriteTxConn<'_>,
    request_id: &[u8; 16],
    reason: RejectionReason,
    event_type: &'static str,
    severity: Severity,
) -> anyhow::Result<WorkerProcessResult> {
    ingress_inbox::mark_rejected_tx(tx, request_id).await?;
    audit_log::append_tx(
        tx,
        "ingress_inbox",
        &hex_encode(request_id),
        event_type,
        severity,
        None,
        Some(&format!(r#"{{"reason":"{reason:?}"}}"#)),
    )
    .await?;
    Ok(WorkerProcessResult::Rejected { reason })
}

/// Stage 2 shift-state guard.  Returns `Some(reason)` if the
/// (doc_type, shift_state) pair is forbidden, `None` if allowed.
fn check_shift_guard(doc_type: DocType, shift_state: ShiftState) -> Option<RejectionReason> {
    match (doc_type, shift_state) {
        // Shift-management ops require specific shift states.
        (DocType::ShiftOpen, ShiftState::Closed) => None,
        (DocType::ShiftOpen, _) => Some(RejectionReason::ShiftAlreadyOpen),
        (DocType::ShiftClose, ShiftState::Opened) => None,
        (DocType::ZReport, ShiftState::Opened) => None,
        // ERROR is terminal — reject everything.
        (_, ShiftState::Error) => Some(RejectionReason::ShiftInError),
        // Mid-transition (Opening / Closing / Created) — block everything.
        (_, ShiftState::Created | ShiftState::Opening | ShiftState::Closing) => {
            Some(RejectionReason::ShiftNotOpen {
                current: shift_state,
            })
        }
        // Regular fiscal ops require Opened.
        (
            DocType::Sell
            | DocType::Return
            | DocType::ServiceIn
            | DocType::ServiceOut
            | DocType::CashWithdrawal
            | DocType::XReport,
            ShiftState::Opened,
        ) => None,
        (
            DocType::Sell
            | DocType::Return
            | DocType::ServiceIn
            | DocType::ServiceOut
            | DocType::CashWithdrawal
            | DocType::XReport,
            ShiftState::Closed,
        ) => Some(RejectionReason::ShiftNotOpen {
            current: shift_state,
        }),
        // Catch-all: SHIFT_CLOSE / Z_REPORT against non-Opened.
        (DocType::ShiftClose, _) | (DocType::ZReport, _) => Some(RejectionReason::ShiftNotOpen {
            current: shift_state,
        }),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            use std::fmt::Write;
            let _ = write!(acc, "{b:02x}");
            acc
        })
}
