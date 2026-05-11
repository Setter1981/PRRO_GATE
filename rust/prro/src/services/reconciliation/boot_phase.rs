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

use sqlx::SqlitePool;

use crate::db::models::enums::{NodeMode, ShiftState};
use crate::db::models::ids::{DocumentId, ShiftId};
use crate::db::repositories::{audit_log, document_files, transport_trace};
use crate::db::tx::with_immediate;
use crate::services::write_path::types::hex_encode_lower as hex_lower;
use crate::transports::dps::dto::CheckAck;

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
/// **Whitelist alignment (LOW 1 fix).**  The raw `UPDATE ... WHERE
/// state = 'SENDING'` CAS hardcodes the
/// `Sending → ErrorRetryable` edge and bypasses
/// [`crate::db::repositories::fiscal_documents::transition_state`]
/// (the pool-bound helper that consults the
/// `fiscal_documents::allowed_transition` whitelist).  This edge IS
/// in the whitelist (W7 added it for Pattern B crash-resume).
/// Future maintainers MUST keep this CAS aligned with the whitelist
/// — if `Sending → ErrorRetryable` is ever removed from the
/// whitelist (extremely unlikely; the edge is structural Pattern B
/// safety), this helper would silently violate I8.  Consider
/// promoting to a `transition_state_tx` variant when a tx-bound
/// transition helper lands.
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
///
/// **Whitelist alignment (LOW 1 fix).**  Raw `UPDATE ... WHERE state
/// = 'SENT'` CAS hardcodes the `Sent → Kvt1` edge and bypasses
/// [`crate::db::repositories::fiscal_documents::transition_state`].
/// This edge IS in the
/// `fiscal_documents::allowed_transition` whitelist (W1 base; W0-1
/// §2.1 row 5).  Future maintainers MUST keep this CAS aligned —
/// if `Sent → Kvt1` is ever removed from the whitelist (the doc
/// transition is structurally fundamental, so removal would be a
/// major schema redesign), this helper would silently violate I8.
/// Consider promoting to a `transition_state_tx` variant when a
/// tx-bound transition helper lands.
pub async fn advance_sent_to_kvt1_from_probe(
    pool: &SqlitePool,
    doc_id: DocumentId,
    attempt_no: i32,
    ack: &CheckAck,
    wire_call_started_at: &str,
    wire_call_finished_at: &str,
) -> anyhow::Result<bool> {
    // NIT 1 fix: single clone layer.  `move |tx|` captures the
    // outer-scope owned strings; the `async move` block then takes
    // them into the future.  No inner re-clone needed under FnOnce.
    let ack_id = ack.id.clone();
    let ack_data_sign = ack.data_sign.clone();
    let wire_started = wire_call_started_at.to_string();
    let wire_finished = wire_call_finished_at.to_string();
    with_immediate(pool, move |tx| {
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
            // LOW 4 fix: defensive state validation.  The helper
            // emits a forensic audit claiming the doc is in Kvt1;
            // firing on a non-Kvt1 doc would produce a misleading
            // audit trail.  W9.3 dispatch will guard this externally
            // too, but in-helper check hardens against ad-hoc
            // admin / test callers.
            let state: String =
                sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
                    .bind(doc_id)
                    .fetch_one(&mut **tx)
                    .await?;
            anyhow::ensure!(
                state == "KVT1",
                "passive_hold_kvt1: doc not in Kvt1 (got {state})"
            );

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

/// W9.3 — per-FN decision-tree dispatch.  Closed-enum outcome lets
/// the caller (`App::reconcile_pending`) accumulate per-FN
/// histograms without re-reading the DB.
#[derive(Debug, PartialEq, Eq)]
pub enum BranchOutcome {
    /// (a) FN row absent → upsert_initial executed.
    Bootstrapped,
    /// (b) FN row + mode=Online + no pending docs → idempotent no-op.
    IdempotentNoop,
    /// (c) FN row + pending docs → per-doc dispatch executed.
    /// Carries the count of pending docs visited (NOT advanced —
    /// some may have been deferred per ctx-needy DocStates).
    Reconciled {
        pending_visited: usize,
    },
    /// (d) Mode ∈ {Offline, GoingOffline, GoingOnline} → refuse
    /// boot; caller surfaces `BootError::OfflineModeRefusal`.
    OfflineRefusal {
        observed_mode: NodeMode,
    },
    /// (e2) Mid-transition shift orphan with no matching pending
    /// doc → shift→Error + node_state.shift_state→Closed.
    /// (e1) collapses into Reconciled (c).
    OrphanShiftResolved,
    /// (f) Mode ∈ {Blocked, StopMode, CryptoDegraded} → preserved.
    PreservedBlocked,
    PreservedStopMode,
    PreservedCryptoDegraded,
}

/// W9 freeze §3 + §4 — per-FN decision tree.
///
/// Reads `node_state::get(pool, fn_id)` + pending-doc list, dispatches
/// to one of branches (a)–(f) per the partition matrix in §3.7.
/// Returns the [`BranchOutcome`] so the caller can aggregate without
/// re-reading the DB.
///
/// **Branch (d) — OFFLINE refusal.**  Returns
/// `BranchOutcome::OfflineRefusal { observed_mode }` rather than
/// erroring directly; the caller (`App::reconcile_pending`) maps this
/// outcome to `BootError::OfflineModeRefusal` and fails-fast on the
/// FIRST OFFLINE FN encountered (per freeze §13.3).
///
/// **Branch (c)/(e1) — per-doc dispatch scope.**  W9.3 ships the
/// dispatch shell that loops `list_pending_for_fn` in
/// `(lnd, created_at, document_id)` order.  Per-DocState routing:
///
///   | DocState        | W9.3 action                            |
///   | --------------- | -------------------------------------- |
///   | `Sending`       | `resume_sending_to_error_retryable`    |
///   | `Kvt1`          | `passive_hold_kvt1`                    |
///   | `Encrypted`     | transition → ErrorRetryable + audit    |
///   | `Kvt2`          | `stage_finalize::run` (W8; no ctx)     |
///   | `Prepared`      | DEFERRED audit (W11 wires SigningCtx)  |
///   | `Signed`        | DEFERRED audit (W11 wires DpsChannel)  |
///   | `Sent`          | DEFERRED audit (W11 wires DpsChannel)  |
///   | `ErrorRetryable`| DEFERRED audit (W11 wires DpsChannel)  |
///
/// Ctx-needy DocStates (Prepared/Signed/Sent/ErrorRetryable) emit
/// `BOOT_DISPATCH_DEFERRED` WARN per occurrence so operators see the
/// docs that the boot tick observed but couldn't drive forward.
/// W11+ runtime composition will wire the missing dispatches; until
/// then those docs stay in their source state.
pub async fn run_boot_reconciliation(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> anyhow::Result<BranchOutcome> {
    use crate::db::repositories::{fiscal_documents, node_state};

    let row = node_state::get(pool, fiscal_number).await?;

    // ── Branch (a) — FN row absent ───────────────────────────────
    let Some(row) = row else {
        node_state::upsert_initial(pool, fiscal_number, NodeMode::Online, ShiftState::Closed, 1)
            .await?;
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "branch": "a",
        });
        audit_log::append(
            pool,
            "node_state",
            fiscal_number,
            "NODE_STATE_INITIALISED",
            crate::db::models::enums::Severity::Info,
            None,
            Some(&payload.to_string()),
        )
        .await?;
        return Ok(BranchOutcome::Bootstrapped);
    };

    // ── Branch (d) — OFFLINE-class modes ─────────────────────────
    match row.mode {
        NodeMode::Offline | NodeMode::GoingOffline | NodeMode::GoingOnline => {
            let payload = serde_json::json!({
                "fiscal_number": fiscal_number,
                "observed_mode": row.mode.as_str(),
                "message": "FN in OFFLINE mode — start with --recover-offline M3b CLI",
            });
            audit_log::append(
                pool,
                "node_state",
                fiscal_number,
                "NODE_STATE_BOOT_OFFLINE_REFUSAL",
                crate::db::models::enums::Severity::Error,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            return Ok(BranchOutcome::OfflineRefusal {
                observed_mode: row.mode,
            });
        }
        _ => {}
    }

    // ── Branch (f) — Blocked / StopMode / CryptoDegraded ─────────
    let (preserved_event, preserved_severity, preserved_outcome) = match row.mode {
        NodeMode::Blocked => (
            "NODE_STATE_BOOT_BLOCKED_PRESERVED",
            crate::db::models::enums::Severity::Info,
            Some(BranchOutcome::PreservedBlocked),
        ),
        NodeMode::StopMode => (
            "NODE_STATE_BOOT_STOP_MODE_PRESERVED",
            crate::db::models::enums::Severity::Warning,
            Some(BranchOutcome::PreservedStopMode),
        ),
        NodeMode::CryptoDegraded => (
            "NODE_STATE_BOOT_CRYPTO_DEGRADED_PRESERVED",
            crate::db::models::enums::Severity::Warning,
            Some(BranchOutcome::PreservedCryptoDegraded),
        ),
        _ => ("", crate::db::models::enums::Severity::Info, None),
    };
    if let Some(outcome) = preserved_outcome {
        let branch_tag = match &outcome {
            BranchOutcome::PreservedBlocked => "f1",
            BranchOutcome::PreservedStopMode => "f2",
            BranchOutcome::PreservedCryptoDegraded => "f3",
            _ => unreachable!(),
        };
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "branch": branch_tag,
            "observed_mode": row.mode.as_str(),
        });
        audit_log::append(
            pool,
            "node_state",
            fiscal_number,
            preserved_event,
            preserved_severity,
            None,
            Some(&payload.to_string()),
        )
        .await?;
        return Ok(outcome);
    }

    // From here: row.mode == NodeMode::Online.
    // Decision: pending docs?  shift_state ∈ {Opening, Closing}?
    let pending = fiscal_documents::list_pending_for_fn(pool, fiscal_number).await?;

    // ── Branch (e2) — mid-transition shift orphan (no pending) ───
    if matches!(row.shift_state, ShiftState::Opening | ShiftState::Closing) && pending.is_empty() {
        // Find the orphan shift in transition; transition to ERROR.
        let orphans: Vec<(ShiftId, ShiftState)> = sqlx::query_as(
            "SELECT shift_id, state FROM shifts \
             WHERE fiscal_number = ? AND state IN ('OPENING', 'CLOSING')",
        )
        .bind(fiscal_number)
        .fetch_all(pool)
        .await?;
        // Single envelope: shift→Error + node_state.shift_state→Closed.
        let fn_owned = fiscal_number.to_string();
        let orphans_owned = orphans.clone();
        with_immediate(pool, move |tx| {
            let fn_owned = fn_owned.clone();
            let orphans_owned = orphans_owned.clone();
            Box::pin(async move {
                for (shift_id, current) in orphans_owned {
                    sqlx::query("UPDATE shifts SET state = 'ERROR' WHERE shift_id = ?")
                        .bind(shift_id)
                        .execute(&mut **tx)
                        .await?;
                    let payload = serde_json::json!({
                        "fiscal_number": fn_owned,
                        "shift_id": hex_lower(shift_id.as_bytes()),
                        "observed_shift_state_pre": current.as_str(),
                        "node_shift_state_post": "Closed",
                        "branch": "e2",
                    });
                    audit_log::append_tx(
                        tx,
                        "shift",
                        &hex_lower(shift_id.as_bytes()),
                        "SHIFT_BOOT_ORPHAN_ERROR",
                        crate::db::models::enums::Severity::Critical,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                }
                // Reset node_state.shift_state to Closed (HIGH 10 fix).
                sqlx::query(
                    "UPDATE node_state SET shift_state = 'CLOSED' \
                     WHERE fiscal_number = ? AND shift_state IN ('OPENING', 'CLOSING')",
                )
                .bind(&fn_owned)
                .execute(&mut **tx)
                .await?;
                Ok::<(), anyhow::Error>(())
            })
        })
        .await?;
        return Ok(BranchOutcome::OrphanShiftResolved);
    }

    // ── Branch (b) — Online + no pending ─────────────────────────
    if pending.is_empty() {
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "branch": "b",
            "observed_mode": row.mode.as_str(),
            "observed_shift_state": row.shift_state.as_str(),
        });
        audit_log::append(
            pool,
            "node_state",
            fiscal_number,
            "NODE_STATE_BOOT_IDEMPOTENT",
            crate::db::models::enums::Severity::Info,
            None,
            Some(&payload.to_string()),
        )
        .await?;
        return Ok(BranchOutcome::IdempotentNoop);
    }

    // ── Branch (c) / (e1) — pending docs ─────────────────────────
    let pending_count = pending.len();
    for doc in &pending {
        dispatch_pending_doc(pool, doc).await?;
    }
    let payload = serde_json::json!({
        "fiscal_number": fiscal_number,
        "branch": if matches!(row.shift_state, ShiftState::Opening | ShiftState::Closing) {
            "e1"
        } else {
            "c"
        },
        "pending_visited": pending_count,
    });
    audit_log::append(
        pool,
        "node_state",
        fiscal_number,
        "NODE_STATE_BOOT_RECONCILED",
        crate::db::models::enums::Severity::Info,
        None,
        Some(&payload.to_string()),
    )
    .await?;
    Ok(BranchOutcome::Reconciled {
        pending_visited: pending_count,
    })
}

/// Per-DocState dispatch inside branch (c)/(e1) iteration.  Ctx-free
/// states get fully driven; ctx-needy states emit a
/// `BOOT_DISPATCH_DEFERRED` WARN audit (W11+ wires the rest).
///
/// **Shifts.state side-effects** of W6/W7/W8 transitions are NOT
/// re-applied here — they fire inside the stage workers' own
/// envelopes when those workers run (live or in a future ctx-wired
/// boot dispatch).  W9.3's ctx-free dispatches (SENDING / KVT1 /
/// ENCRYPTED / KVT2 via stage_finalize) preserve invariant I8.
async fn dispatch_pending_doc(
    pool: &SqlitePool,
    doc: &crate::db::repositories::fiscal_documents::DocumentRow,
) -> anyhow::Result<()> {
    use crate::db::models::enums::DocState;
    match doc.state {
        DocState::Sending => {
            resume_sending_to_error_retryable(pool, doc.document_id).await?;
        }
        DocState::Kvt1 => {
            passive_hold_kvt1(pool, doc.document_id).await?;
        }
        DocState::Encrypted => {
            // 1-tick deferral per freeze §4.3 MED 6 fix: transition
            // Encrypted → ErrorRetryable; subsequent boot tick handles
            // via §4.8.
            let doc_id = doc.document_id;
            with_immediate(pool, move |tx| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE fiscal_documents SET state = 'ERROR_RETRYABLE' \
                         WHERE document_id = ? AND state = 'ENCRYPTED'",
                    )
                    .bind(doc_id)
                    .execute(&mut **tx)
                    .await?;
                    let payload = serde_json::json!({
                        "document_id": hex_lower(doc_id.as_bytes()),
                        "branch": "c-encrypted",
                        "rationale":
                            "M3a is Pattern B + ONLINE; ENCRYPTED is Checkbox-only contour",
                    });
                    audit_log::append_tx(
                        tx,
                        "fiscal_document",
                        &hex_lower(doc_id.as_bytes()),
                        "BOOT_ENCRYPTED_REROUTED",
                        crate::db::models::enums::Severity::Warning,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                    Ok::<(), anyhow::Error>(())
                })
            })
            .await?;
        }
        DocState::Kvt2 => {
            // W8 stage_finalize::run — pool + doc_id only, no ctx.
            // The helper internally CASs Kvt2→Ack + advances chain
            // seed + outbox INSERT + audit.
            let _ = crate::services::write_path::stage_finalize::run(pool, doc.document_id).await;
            // Outcome is the W8 enum; we don't surface it here — the
            // worker emits its own audit (STAGE_FINALIZE_ACK).  W9
            // boot-level audit is the branch-level NODE_STATE_BOOT_RECONCILED
            // row already emitted at end of branch (c).
        }
        // Ctx-needy states — emit DEFERRED audit, do not transition.
        DocState::Prepared | DocState::Signed | DocState::Sent | DocState::ErrorRetryable => {
            let doc_id = doc.document_id;
            let state_str = doc.state.as_str().to_string();
            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "observed_state": state_str,
                "rationale":
                    "ctx-needy dispatch deferred to runtime composition (W11+); doc stays in source state",
            });
            audit_log::append(
                pool,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_DISPATCH_DEFERRED",
                crate::db::models::enums::Severity::Warning,
                None,
                Some(&payload.to_string()),
            )
            .await?;
        }
        // Terminal states should NEVER appear in `list_pending_for_fn`
        // per its WHERE clause (excludes ACK/REJECTED/CANCELLED/
        // OFFLINE_LOCAL_ACK/REQUIRES_MANUAL_RECONCILIATION).  If we
        // observe one here, the SELECT contract is broken — surface
        // as anyhow error rather than silently dispatching.
        DocState::Ack
        | DocState::Rejected
        | DocState::Cancelled
        | DocState::OfflineLocalAck
        | DocState::RequiresManualReconciliation => {
            anyhow::bail!(
                "dispatch_pending_doc: terminal DocState {:?} returned by list_pending_for_fn — \
                 contract violation",
                doc.state
            );
        }
    }
    Ok(())
}
