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
/// Note: "lease" in this module means the inbox-row lease taken by
/// `ingress_inbox::acquire_lease`, which is keyed on `request_id`.
/// It is **not** an FN-scope lock — see ADR-M3-A10 for the M3a
/// single-writer-per-FN invariant and its current enforcement
/// mechanism (global-single-writer + `BEGIN IMMEDIATE`).
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

            // [Step 1b] Command-vs-inbox cross-check.  The leased
            //           inbox row carries `payload_json`,
            //           `payload_sha256_canonical`, and
            //           `operation_type` persisted by ingress; the
            //           in-process `command` argument MUST agree on
            //           hash and doc_type, otherwise the worker is
            //           about to PREPARE a doc against payload it did
            //           not receive.  Reject without lnd advance and
            //           without INSERT.
            if command.payload_sha256_canonical != inbox.payload_sha256_canonical {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::InvalidPayload {
                        detail: "command_payload_hash_mismatch".to_string(),
                    },
                    "command_inbox_mismatch",
                    Severity::Critical,
                )
                .await;
            }
            if command.doc_type.as_str() != inbox.operation_type {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::InvalidPayload {
                        detail: "command_doc_type_mismatch".to_string(),
                    },
                    "command_inbox_mismatch",
                    Severity::Critical,
                )
                .await;
            }

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

            // [Step 6] Resume-detect — pending lookup only.  Terminal
            //          rows (ACK / REJECTED / CANCELLED /
            //          OFFLINE_LOCAL_ACK / REQUIRES_MANUAL_RECONCILIATION)
            //          MUST NOT drive Resumed; their flow is concluded.
            let request_id_typed = RequestId::from_bytes(request_id);
            if let Some(existing) =
                fd::get_pending_by_request_id_tx(tx, &request_id_typed).await?
            {
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

            // [Step 6b] Terminal-doc + NEW-inbox-for-same-request_id =
            //           invariant breach.  Refuse to INSERT a duplicate
            //           PREPARED; surface as InvalidPayload (Critical
            //           audit) so the operator can investigate the
            //           lifecycle clash.
            if fd::exists_terminal_by_request_id_tx(tx, &request_id_typed).await? {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::InvalidPayload {
                        detail: "terminal_document_for_request_id".to_string(),
                    },
                    "terminal_document_for_request_id",
                    Severity::Critical,
                )
                .await;
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
                // W14a-2b Commit 1: plumbing-only — None until Commit 2
                // threads CanonicalFiscalCommand.signed_by_cashier_id.
                signed_by_cashier_id: None,
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
                // W6 — fresh PREPARED row: chain hash, Z, unsigned-hash,
                // and pin-marker are all genuinely None.  Stage 3-PRE
                // (W6) is the canonical pin site.
                previous_hash: None,
                z_report_number: None,
                unsigned_xml_sha256: None,
                signing_inputs_pinned_at: None,
                // W14a-2b Commit 1: carries the value from new_doc (None
                // until Commit 2 plumbs CanonicalFiscalCommand).
                signed_by_cashier_id: new_doc.signed_by_cashier_id.clone(),
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
///
/// **Order matters**: `ShiftState::Error` and
/// `ShiftState::RequiresManualReconciliation` are both terminal /
/// operator-action states that apply uniformly to every doc_type, so
/// their catch-all arms run FIRST — before any
/// `(SHIFT_OPEN, _)` / `(SHIFT_CLOSE, _)` / `(Z_REPORT, _)` catch-all
/// that would otherwise mislabel them as `ShiftAlreadyOpen` /
/// `ShiftNotOpen`.  Per spec §5.6 matrix:
///   - `(_, Error)` → `ShiftInError` (structural-breach surface).
///   - `(_, RequiresManualReconciliation)` → `ShiftRequiresOperatorAttention`
///     (operator-action / accounting-compensation surface; per M3b §16.7
///     drain rejected an OFFLINE_LOCAL_ACK backlog doc, ambiguous wire
///     timeout, or force seam).
fn check_shift_guard(doc_type: DocType, shift_state: ShiftState) -> Option<RejectionReason> {
    match (doc_type, shift_state) {
        // ERROR is terminal — reject everything.  Must precede every
        // doc_type-specific catch-all below.
        (_, ShiftState::Error) => Some(RejectionReason::ShiftInError),
        // REQUIRES_MANUAL_RECONCILIATION is operator-action territory — reject
        // everything with the operator-attention surface (PR #65 R1 M1 per
        // spec §5.6).  Distinct from `ShiftInError` so audit/metrics/UI
        // can label appropriately (Manual = accounting compensation,
        // Error = structural breach).
        (_, ShiftState::RequiresManualReconciliation) => {
            Some(RejectionReason::ShiftRequiresOperatorAttention)
        }
        // Shift-management ops require specific shift states.
        (DocType::ShiftOpen, ShiftState::Closed) => None,
        (DocType::ShiftOpen, _) => Some(RejectionReason::ShiftAlreadyOpen),
        (DocType::ShiftClose, ShiftState::Opened) => None,
        (DocType::ZReport, ShiftState::Opened) => None,
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
        // W14a-1 minimal compile coverage for the 2 new in-flight M3b
        // shift states.  Fiscal ops against `OpenedLocalPendingDrain` /
        // `ClosingLocalPendingDrain` defensively refused via the existing
        // `ShiftNotOpen` reason.  Full semantics land in W14a-2
        // (channel-aware offline ops on OpenedLocalPendingDrain per spec
        // §3.3; explicit POST_LOCAL_CLOSE_SALE_REFUSED for
        // ClosingLocalPendingDrain per PR #62 §W10).
        // (`RequiresManualReconciliation` is caught earlier by the
        // `(_, ShiftState::RequiresManualReconciliation)` arm above —
        // PR #65 R1 M1 fix per spec §5.6.)
        // ShiftOpen + these in-flight states is already caught upstream
        // by the `(ShiftOpen, _)` arm above (→ ShiftAlreadyOpen).
        // ShiftClose / ZReport + these is caught by the catch-all below.
        (
            DocType::Sell
            | DocType::Return
            | DocType::ServiceIn
            | DocType::ServiceOut
            | DocType::CashWithdrawal
            | DocType::XReport,
            ShiftState::OpenedLocalPendingDrain
            | ShiftState::ClosingLocalPendingDrain,
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
