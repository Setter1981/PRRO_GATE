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
use crate::services::write_path::stage_send;
use crate::services::write_path::types::hex_encode_lower as hex_lower;
use crate::transports::dps::dto::CheckAck;

/// Per W9 freeze §4.0: budget cap for `attempts_used(doc_id)` →
/// `RequiresManualReconciliation` escalation in §4.8 ERROR_RETRYABLE
/// pre-check.  Mirrors W0-3 §2 policy "retry up to
/// max_recovery_attempts=5".
pub const MAX_BOOT_ATTEMPTS: i64 = 5;

/// W9.4 fix (L1 + M1) — per-DocState dispatch histogram captured
/// during branch (c)/(e1) per-doc iteration.  Emitted in the
/// `NODE_STATE_BOOT_RECONCILED` audit payload as `"by_outcome": {...}`
/// per freeze §3.3 + aggregated into [`ReconciliationSummary`].
///
/// Counts are mutually exclusive per doc — each pending doc lands in
/// exactly one bucket (success or `dispatch_errors` if M3 try-and-
/// audit shim caught a helper failure).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchHistogram {
    pub sending_resumed: usize,
    pub kvt1_held: usize,
    pub encrypted_rerouted: usize,
    pub kvt2_finalized: usize,
    pub kvt2_failed: usize,
    pub prepared_deferred: usize,
    pub signed_deferred: usize,
    pub sent_deferred: usize,
    pub error_retryable_deferred: usize,
    /// W11 PR-2 — counter for SIGNED docs that the runtime-composed
    /// dispatcher (`reconcile_pending_with`) drove forward through
    /// `stage_send::run`.  Increments on successful dispatch invocation
    /// regardless of the doc's final state (Sent / KVT1 / Rejected /
    /// ErrorRetryable / RequiresManualReconciliation).  Helper failures
    /// route through `BOOT_DISPATCH_ERROR` + `dispatch_errors`.
    pub signed_dispatched: usize,
    /// W11 PR-2 — counter for ERROR_RETRYABLE docs that the runtime-
    /// composed dispatcher drove forward through `stage_send::run`
    /// (which allows `ErrorRetryable → Sending` per ADR-M3-A9).
    pub error_retryable_dispatched: usize,
    /// W11 PR-2b — SENT crash-recovery via `last_chk_probe::probe`,
    /// `ProbeOutcome::Match` arm.  Doc transitioned Sent → Kvt1 via
    /// `advance_sent_to_kvt1_from_probe`.
    pub sent_match_to_kvt1: usize,
    /// W11 PR-2b — SENT crash-recovery, `ProbeOutcome::Mismatch` arm.
    /// Doc transitioned Sent → RequiresManualReconciliation via the
    /// W11 prep-PR whitelist edge (operator handoff per W0-3 §6.4-b).
    pub sent_mismatch_to_manual: usize,
    /// W11 PR-2b — SENT crash-recovery, `ProbeOutcome::NotFound` arm.
    /// Doc transitioned Sent → ErrorRetryable; tick-2 of two-tick
    /// retry path (ADR-M3-A9 step 3) re-drives via Pattern B.
    pub sent_not_found_to_error_retryable: usize,
    /// W11 PR-2b — SENT crash-recovery probe failure (TransportRetry
    /// / DecodeEscalate / Unexpected).  Doc state left at SENT; next
    /// boot tick re-attempts the probe.  Forensic audit
    /// `BOOT_SENT_PROBE_DEFERRED` fires alongside.
    pub sent_probe_failure_deferred: usize,
    /// W11 PR-2b — counter for PREPARED docs that the runtime-composed
    /// dispatcher drove forward through `stage_sign::run` →
    /// `stage_send::run` chain.  Increments on successful dispatch
    /// invocation regardless of the doc's final state.
    pub prepared_dispatched: usize,
    /// W9.4 M3 fix counter — per-doc dispatch failures absorbed by
    /// the try-and-audit shim (helper-level Err that's NOT fatal at
    /// branch level).  Operator dashboard surfaces these via
    /// `BOOT_DISPATCH_ERROR` audit rows AND this counter.
    pub dispatch_errors: usize,
}

impl DispatchHistogram {
    pub fn total_visited(&self) -> usize {
        self.sending_resumed
            + self.kvt1_held
            + self.encrypted_rerouted
            + self.kvt2_finalized
            + self.kvt2_failed
            + self.prepared_deferred
            + self.signed_deferred
            + self.sent_deferred
            + self.error_retryable_deferred
            + self.signed_dispatched
            + self.error_retryable_dispatched
            + self.sent_match_to_kvt1
            + self.sent_mismatch_to_manual
            + self.sent_not_found_to_error_retryable
            + self.sent_probe_failure_deferred
            + self.prepared_dispatched
            + self.dispatch_errors
    }

    fn merge(&mut self, other: &DispatchHistogram) {
        self.sending_resumed += other.sending_resumed;
        self.kvt1_held += other.kvt1_held;
        self.encrypted_rerouted += other.encrypted_rerouted;
        self.kvt2_finalized += other.kvt2_finalized;
        self.kvt2_failed += other.kvt2_failed;
        self.prepared_deferred += other.prepared_deferred;
        self.signed_deferred += other.signed_deferred;
        self.sent_deferred += other.sent_deferred;
        self.error_retryable_deferred += other.error_retryable_deferred;
        self.signed_dispatched += other.signed_dispatched;
        self.error_retryable_dispatched += other.error_retryable_dispatched;
        self.sent_match_to_kvt1 += other.sent_match_to_kvt1;
        self.sent_mismatch_to_manual += other.sent_mismatch_to_manual;
        self.sent_not_found_to_error_retryable += other.sent_not_found_to_error_retryable;
        self.sent_probe_failure_deferred += other.sent_probe_failure_deferred;
        self.prepared_dispatched += other.prepared_dispatched;
        self.dispatch_errors += other.dispatch_errors;
    }
}

/// W9.4 fix (M1) — sub-branch tag for `BranchOutcome::Reconciled`
/// distinguishing pure pending-set processing (c) from mid-transition
/// shift cascade (e1).  Freeze §3.7 partition: (e1) implies
/// `shift_state ∈ {Opening, Closing}` AND matching pending doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubBranch {
    /// (c) pure pending-set processing; shift_state ∉ {Opening, Closing}.
    C,
    /// (e1) mid-transition shift with matching pending doc; cascades to (c).
    E1,
}

impl SubBranch {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubBranch::C => "c",
            SubBranch::E1 => "e1",
        }
    }
}

/// W9.4 fix (M1) — per-call aggregate returned by
/// [`crate::App::reconcile_pending`].  Each FN's `BranchOutcome`
/// folds into one of the branch-count fields; per-DocState dispatches
/// fold into `docs_advanced`.  Operator / test consumers read the
/// summary directly instead of re-querying `audit_log`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconciliationSummary {
    pub branch_a: usize,
    pub branch_b: usize,
    pub branch_c: usize,
    /// W9.4 cycle-2 LOW-5 fix: symmetric field for branch (d).
    /// Always 0 under M3a's fail-fast policy (caller maps
    /// OfflineRefusal to `BootError::OfflineModeRefusal` before
    /// recording).  Field exists for symmetry + future-proofing
    /// (non-fail-fast variant could populate this without silent
    /// loss).
    pub branch_d_offline_refusal: usize,
    pub branch_e1: usize,
    pub branch_e2: usize,
    pub branch_f_blocked: usize,
    pub branch_f_stop_mode: usize,
    pub branch_f_crypto_degraded: usize,
    /// Aggregated per-DocState dispatch outcomes across all FNs
    /// processed in this `reconcile_pending` call.  Tied to L1
    /// histogram payload — same shape, multi-FN union.
    pub docs_advanced: DispatchHistogram,
    /// (e2) sub-branch: count of orphan shifts forcibly transitioned
    /// to `ERROR` across all FNs.  One FN may contribute multiple
    /// orphans (rare; verified by `branch_e2_handles_multiple_orphan_shifts`).
    pub shift_orphans_to_error: usize,
}

impl ReconciliationSummary {
    /// Fold one FN's [`BranchOutcome`] into this summary.  The histogram
    /// (if any) is merged into `docs_advanced`; the orphan count is
    /// added directly.
    pub fn record(&mut self, outcome: &BranchOutcome) {
        match outcome {
            BranchOutcome::Bootstrapped => self.branch_a += 1,
            BranchOutcome::IdempotentNoop => self.branch_b += 1,
            BranchOutcome::Reconciled {
                histogram,
                sub_branch,
            } => {
                match sub_branch {
                    SubBranch::C => self.branch_c += 1,
                    SubBranch::E1 => self.branch_e1 += 1,
                }
                self.docs_advanced.merge(histogram);
            }
            BranchOutcome::OfflineRefusal { .. } => {
                // LOW-5 symmetric field: populated for non-fail-fast
                // future variant.  Under M3a's current fail-fast
                // policy `App::reconcile_pending` returns Err before
                // calling `record`, so this branch is dead code
                // today — kept for future-proofing + debug_assert
                // catches inadvertent call.
                self.branch_d_offline_refusal += 1;
            }
            BranchOutcome::OrphanShiftResolved { orphans_resolved } => {
                self.branch_e2 += 1;
                self.shift_orphans_to_error += orphans_resolved;
            }
            BranchOutcome::PreservedBlocked => self.branch_f_blocked += 1,
            BranchOutcome::PreservedStopMode => self.branch_f_stop_mode += 1,
            BranchOutcome::PreservedCryptoDegraded => self.branch_f_crypto_degraded += 1,
        }
    }
}

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
/// under M3a's single-writer-per-FN invariant the latter cannot
/// occur within boot — see ADR-M3-A10).
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

/// W11 PR-2b — SENT crash-recovery, `ProbeOutcome::Mismatch` arm.
///
/// CAS `Sent → RequiresManualReconciliation` via the W11 prep-PR
/// whitelist edge + complete the in-flight `transport_trace` row
/// with `OutcomeKind::Rejected` (DPS protocol-state divergence is
/// recorded as a doc-level rejection from the local PoV) + audit
/// `BOOT_SENT_LAST_CHK_MISMATCH_RM`.
///
/// All three writes commit inside one `with_immediate` envelope.
/// `actual_id` carries the DPS-returned id for forensic audit
/// (operator inspects to understand why the divergence happened).
pub async fn cas_sent_to_manual_reconciliation_from_probe(
    pool: &SqlitePool,
    doc_id: DocumentId,
    attempt_no: i32,
    actual_id: &str,
    wire_call_started_at: &str,
    wire_call_finished_at: &str,
) -> anyhow::Result<bool> {
    let actual_id_owned = actual_id.to_string();
    let wire_started = wire_call_started_at.to_string();
    let wire_finished = wire_call_finished_at.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (1) CAS Sent → RequiresManualReconciliation
            // (whitelist edge added in prep PR per W0-3 §6.4-b).
            let cas = sqlx::query(
                "UPDATE fiscal_documents SET state = 'REQUIRES_MANUAL_RECONCILIATION' \
                 WHERE document_id = ? AND state = 'SENT'",
            )
            .bind(doc_id)
            .execute(&mut **tx)
            .await?;
            if cas.rows_affected() != 1 {
                // CAS conflict — bail early, no partial trail.
                return Ok::<bool, anyhow::Error>(false);
            }

            // (2) Complete transport_trace row with Rejected outcome
            // and the DPS-returned id captured for forensics.
            let n = transport_trace::complete_tx(
                tx,
                doc_id,
                attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind: transport_trace::OutcomeKind::Rejected,
                    server_fiscal_no: Some(actual_id_owned.clone()),
                    server_status_code: None,
                    error_kind: Some("LAST_CHK_MISMATCH".to_string()),
                    error_message: Some(format!(
                        "DPS last_chk returned id={actual_id_owned} not matching transport_request_id; operator handoff"
                    )),
                    retry_class: None,
                },
            )
            .await?;
            if n != 1 {
                anyhow::bail!(
                    "transport_trace mismatch completion: rows_affected = {n} (expected 1; doc {doc_id:?}, attempt_no = {attempt_no})"
                );
            }

            // (3) Audit BOOT_SENT_LAST_CHK_MISMATCH_RM.
            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-sent-mismatch",
                "attempt_no": attempt_no,
                "actual_id_from_dps": actual_id_owned,
                "rationale":
                    "last_chk returned different id than recorded transport_request_id — protocol-state divergence; operator handoff via RequiresManualReconciliation",
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_SENT_LAST_CHK_MISMATCH_RM",
                crate::db::models::enums::Severity::Error,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<bool, anyhow::Error>(true)
        })
    })
    .await
}

/// W11 PR-2b — SENT crash-recovery, `ProbeOutcome::NotFound` arm.
///
/// CAS `Sent → ErrorRetryable` + complete the in-flight
/// `transport_trace` row with `OutcomeKind::RetryableServer` (DPS
/// has no record — server-side condition, retryable) + audit
/// `BOOT_SENT_LAST_CHK_NOTFOUND`.
///
/// This is the first tick of the two-tick recovery path
/// (operator-decided 2026-05-12 per W11 design doc §9 Q1).  Second
/// tick: ERROR_RETRYABLE dispatch via `stage_send::run` re-drives
/// via Pattern B `ErrorRetryable → Sending → wire send`.  ADR-M3-A9
/// step 3 forbids the direct `Sent → Sending` edge.
pub async fn cas_sent_to_error_retryable_from_probe(
    pool: &SqlitePool,
    doc_id: DocumentId,
    attempt_no: i32,
    wire_call_started_at: &str,
    wire_call_finished_at: &str,
) -> anyhow::Result<bool> {
    let wire_started = wire_call_started_at.to_string();
    let wire_finished = wire_call_finished_at.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (1) CAS Sent → ErrorRetryable (whitelisted at base).
            let cas = sqlx::query(
                "UPDATE fiscal_documents SET state = 'ERROR_RETRYABLE' \
                 WHERE document_id = ? AND state = 'SENT'",
            )
            .bind(doc_id)
            .execute(&mut **tx)
            .await?;
            if cas.rows_affected() != 1 {
                return Ok::<bool, anyhow::Error>(false);
            }

            // (2) Complete transport_trace row with RetryableServer outcome.
            let n = transport_trace::complete_tx(
                tx,
                doc_id,
                attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind: transport_trace::OutcomeKind::RetryableServer,
                    server_fiscal_no: None,
                    server_status_code: None,
                    error_kind: Some("LAST_CHK_NOTFOUND".to_string()),
                    error_message: Some(
                        "DPS last_chk returned NotFound; tick-2 of two-tick retry path will re-drive via Pattern B".to_string()
                    ),
                    retry_class: None,
                },
            )
            .await?;
            if n != 1 {
                anyhow::bail!(
                    "transport_trace notfound completion: rows_affected = {n} (expected 1; doc {doc_id:?}, attempt_no = {attempt_no})"
                );
            }

            // (3) Audit BOOT_SENT_LAST_CHK_NOTFOUND.
            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-sent-notfound",
                "attempt_no": attempt_no,
                "rationale":
                    "last_chk returned NotFound — DPS has no record of doc with this transport_request_id; tick-1 transition Sent → ErrorRetryable (ADR-M3-A9 step 3 two-tick retry path)",
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_SENT_LAST_CHK_NOTFOUND",
                crate::db::models::enums::Severity::Warning,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<bool, anyhow::Error>(true)
        })
    })
    .await
}

/// W11 PR-2b — SENT crash-recovery probe failure (TransportRetry /
/// DecodeEscalate / Unexpected ProbeOutcome variants).
///
/// Complete the in-flight `transport_trace` row with the supplied
/// outcome kind + emit `BOOT_SENT_PROBE_DEFERRED` audit.  **No state
/// transition** — doc stays in SENT; next boot tick re-attempts the
/// probe.  Single `with_immediate` envelope.
#[allow(clippy::too_many_arguments)]
pub async fn complete_probe_trace_no_state_change(
    pool: &SqlitePool,
    doc_id: DocumentId,
    attempt_no: i32,
    wire_call_started_at: &str,
    wire_call_finished_at: &str,
    outcome_kind: transport_trace::OutcomeKind,
    failure_label: &'static str,
    failure_reason: &str,
) -> anyhow::Result<()> {
    let wire_started = wire_call_started_at.to_string();
    let wire_finished = wire_call_finished_at.to_string();
    let reason_owned = failure_reason.to_string();
    let failure_label_owned = failure_label.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::complete_tx(
                tx,
                doc_id,
                attempt_no,
                transport_trace::AttemptCompletion {
                    wire_call_started_at: wire_started,
                    wire_call_finished_at: wire_finished,
                    outcome_kind,
                    server_fiscal_no: None,
                    server_status_code: None,
                    error_kind: Some(failure_label_owned.clone()),
                    error_message: Some(reason_owned.clone()),
                    retry_class: None,
                },
            )
            .await?;
            if n != 1 {
                anyhow::bail!(
                    "transport_trace probe-failure completion: rows_affected = {n} (expected 1; doc {doc_id:?}, attempt_no = {attempt_no})"
                );
            }

            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-sent-probe-deferred",
                "attempt_no": attempt_no,
                "failure_label": failure_label_owned,
                "failure_reason": reason_owned,
                "rationale":
                    "last_chk probe failed mid-recovery; doc stays in SENT; next boot tick re-attempts",
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_SENT_PROBE_DEFERRED",
                crate::db::models::enums::Severity::Warning,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchOutcome {
    /// (a) FN row absent → upsert_initial executed.
    Bootstrapped,
    /// (b) FN row + mode=Online + no pending docs → idempotent no-op.
    IdempotentNoop,
    /// (c) FN row + pending docs → per-doc dispatch executed.  W9.4
    /// (M1 + L1 fix): carries per-DocState histogram + sub-branch
    /// tag (c vs e1 cascade).
    Reconciled {
        histogram: DispatchHistogram,
        sub_branch: SubBranch,
    },
    /// (d) Mode ∈ {Offline, GoingOffline, GoingOnline} → refuse
    /// boot; caller surfaces `BootError::OfflineModeRefusal`.
    OfflineRefusal {
        observed_mode: NodeMode,
    },
    /// (e2) Mid-transition shift orphan with no matching pending
    /// doc → shift→Error + node_state.shift_state→Closed.  Carries
    /// the count of orphans resolved (one FN may have multiple).
    OrphanShiftResolved {
        orphans_resolved: usize,
    },
    /// (f) Mode ∈ {Blocked, StopMode, CryptoDegraded} → preserved.
    PreservedBlocked,
    PreservedStopMode,
    PreservedCryptoDegraded,
}

impl BranchOutcome {
    /// W9.4 fix (N2) — stable string tag for this branch, used in
    /// audit payloads and operator-facing tooling.  Single source of
    /// truth — adding a new variant requires extending this map AND
    /// the corresponding audit-emission paths in
    /// `run_boot_reconciliation`.
    pub fn branch_tag(&self) -> &'static str {
        match self {
            BranchOutcome::Bootstrapped => "a",
            BranchOutcome::IdempotentNoop => "b",
            BranchOutcome::Reconciled { sub_branch, .. } => sub_branch.as_str(),
            BranchOutcome::OfflineRefusal { .. } => "d",
            BranchOutcome::OrphanShiftResolved { .. } => "e2",
            BranchOutcome::PreservedBlocked => "f1",
            BranchOutcome::PreservedStopMode => "f2",
            BranchOutcome::PreservedCryptoDegraded => "f3",
        }
    }
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
    deps: Option<&super::ReconciliationRuntime<'_>>,
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
    // N1 fix: Option<tuple> shape instead of sentinel-string tuple
    // for the unreachable Online arm.  Online path drops through to
    // the (e2)/(b)/(c)/(e1) cascade below.
    let preserved: Option<(
        &'static str,
        crate::db::models::enums::Severity,
        BranchOutcome,
    )> = match row.mode {
        NodeMode::Blocked => Some((
            "NODE_STATE_BOOT_BLOCKED_PRESERVED",
            crate::db::models::enums::Severity::Info,
            BranchOutcome::PreservedBlocked,
        )),
        NodeMode::StopMode => Some((
            "NODE_STATE_BOOT_STOP_MODE_PRESERVED",
            crate::db::models::enums::Severity::Warning,
            BranchOutcome::PreservedStopMode,
        )),
        NodeMode::CryptoDegraded => Some((
            "NODE_STATE_BOOT_CRYPTO_DEGRADED_PRESERVED",
            crate::db::models::enums::Severity::Warning,
            BranchOutcome::PreservedCryptoDegraded,
        )),
        _ => None,
    };
    if let Some((preserved_event, preserved_severity, outcome)) = preserved {
        // N2 fix: use BranchOutcome::branch_tag() instead of inline
        // match-on-variant — single source of truth.
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "branch": outcome.branch_tag(),
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
        // LOW 5 fix: SELECT shifts + per-shift UPDATE + node_state
        // reset ALL inside a single `with_immediate` envelope.  The
        // BEGIN IMMEDIATE serialisation removes the read-then-update
        // gap that the earlier pool-bound SELECT had — no need to
        // rely on the SWFN invariant as the sole guard.
        // N3 fix: single move into the closure body — no inner re-clone.
        let fn_owned = fiscal_number.to_string();
        let orphans_resolved = with_immediate(pool, move |tx| {
            Box::pin(async move {
                // Read orphan shifts inside the envelope so any
                // parallel writer (theoretical under non-SWFN) cannot
                // race between SELECT and UPDATE.
                let orphans: Vec<(ShiftId, ShiftState)> = sqlx::query_as(
                    "SELECT shift_id, state FROM shifts \
                     WHERE fiscal_number = ? AND state IN ('OPENING', 'CLOSING')",
                )
                .bind(&fn_owned)
                .fetch_all(&mut **tx)
                .await?;
                let orphans_resolved = orphans.len();
                for (shift_id, current) in orphans {
                    // LOW 4 fix: whitelist alignment — `any → ERROR`
                    // is allowed per W0-1 §2.2 (operator-forced
                    // terminal state).  Raw UPDATE bypasses
                    // `shifts::transition_state` but preserves I8.
                    // Future maintainers MUST keep aligned; switch to
                    // `shifts::transition_state_tx` when a tx-bound
                    // variant is introduced.
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
                Ok::<usize, anyhow::Error>(orphans_resolved)
            })
        })
        .await?;
        return Ok(BranchOutcome::OrphanShiftResolved { orphans_resolved });
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
    let sub_branch = if matches!(row.shift_state, ShiftState::Opening | ShiftState::Closing) {
        SubBranch::E1
    } else {
        SubBranch::C
    };
    let mut histogram = DispatchHistogram::default();
    for doc in &pending {
        // M3 fix: per-doc dispatch is failure-isolated.  Helper-level
        // errors absorbed into BOOT_DISPATCH_ERROR audit + histogram
        // counter; only infrastructure-level errors (audit insert
        // failure) propagate.
        dispatch_pending_doc(pool, doc, deps, &mut histogram).await?;
    }
    // L1 fix: emit histogram in the FN-level audit payload.
    let payload = serde_json::json!({
        "fiscal_number": fiscal_number,
        "branch": sub_branch.as_str(),
        "by_outcome": {
            "sending_resumed": histogram.sending_resumed,
            "kvt1_held": histogram.kvt1_held,
            "encrypted_rerouted": histogram.encrypted_rerouted,
            "kvt2_finalized": histogram.kvt2_finalized,
            "kvt2_failed": histogram.kvt2_failed,
            "prepared_deferred": histogram.prepared_deferred,
            "signed_deferred": histogram.signed_deferred,
            "sent_deferred": histogram.sent_deferred,
            "error_retryable_deferred": histogram.error_retryable_deferred,
            "signed_dispatched": histogram.signed_dispatched,
            "error_retryable_dispatched": histogram.error_retryable_dispatched,
            "sent_match_to_kvt1": histogram.sent_match_to_kvt1,
            "sent_mismatch_to_manual": histogram.sent_mismatch_to_manual,
            "sent_not_found_to_error_retryable": histogram.sent_not_found_to_error_retryable,
            "sent_probe_failure_deferred": histogram.sent_probe_failure_deferred,
            "prepared_dispatched": histogram.prepared_dispatched,
            "dispatch_errors": histogram.dispatch_errors,
        },
        "pending_visited": histogram.total_visited(),
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
        histogram,
        sub_branch,
    })
}

/// W11 PR-2b — SENT crash-recovery orchestrator.  Allocates a fresh
/// `transport_trace` row, runs `last_chk_probe::probe` (network,
/// OUTSIDE any `with_immediate`), then dispatches on `ProbeOutcome`:
///
/// - **Match**: `advance_sent_to_kvt1_from_probe` → state Sent → Kvt1
///   + KVT1_RAW persisted from ack.data_sign + trace completion.
/// - **Mismatch**: `cas_sent_to_manual_reconciliation_from_probe` →
///   state Sent → RequiresManualReconciliation (operator handoff per
///   W0-3 §6.4-b; whitelist edge from prep PR #35) + trace completion
///   with `Rejected` outcome.
/// - **NotFound**: `cas_sent_to_error_retryable_from_probe` → state
///   Sent → ErrorRetryable + trace completion with `RetryableServer`
///   outcome.  Tick-2 of the two-tick retry path
///   (operator-decided per W11 design doc §9 Q1).
/// - **TransportRetry / DecodeEscalate / Unexpected**:
///   `complete_probe_trace_no_state_change` → trace completion +
///   `BOOT_SENT_PROBE_DEFERRED` audit; doc stays in SENT for next
///   boot tick.
///
/// Doc with `server_fiscal_no = None` is a structural breach (SENT
/// requires the transport_request_id to have been recorded); we emit
/// `BOOT_DISPATCH_ERROR` + return without further dispatch.
async fn dispatch_sent_via_probe(
    pool: &SqlitePool,
    deps: &super::ReconciliationRuntime<'_>,
    doc: &crate::db::repositories::fiscal_documents::DocumentRow,
    histogram: &mut DispatchHistogram,
) -> anyhow::Result<()> {
    use super::last_chk_probe::{self, ProbeOutcome};
    let doc_id = doc.document_id;

    // Read expected wire id from the persisted SENT marker.
    let expected_id = match doc.server_fiscal_no.clone() {
        Some(s) => s,
        None => {
            emit_dispatch_error(
                pool,
                doc_id,
                "c-sent-no-server-fiscal-no",
                &anyhow::anyhow!(
                    "doc {doc_id:?} in SENT lacks server_fiscal_no — cannot probe last_chk"
                ),
                histogram,
            )
            .await?;
            return Ok(());
        }
    };

    // Allocate transport_trace recovery row inside a dedicated tx —
    // BEGIN IMMEDIATE serialises the `MAX(attempt_no)+1` read.
    // `request_envelope_sha256` is zero-bytes because a probe is a
    // query, not a wire submit; there is no envelope payload.
    let backend_id = doc.backend_profile_id.clone();
    let transport_id = doc.transport_profile_id.clone();
    let attempt_no = with_immediate(pool, move |tx| {
        Box::pin(async move {
            transport_trace::allocate_and_insert_tx(
                tx,
                doc_id,
                transport_trace::NewAttempt {
                    backend_profile_id: backend_id,
                    transport_profile_id: transport_id,
                    request_envelope_sha256: [0u8; 32],
                },
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await?;

    // Capture wire times around the probe (network call, no tx).
    let wire_started = iso8601_now();
    let outcome = last_chk_probe::probe(deps.dps, deps.fn_sign, &expected_id).await;
    let wire_finished = iso8601_now();

    match outcome {
        ProbeOutcome::Match { ack } => {
            match advance_sent_to_kvt1_from_probe(
                pool,
                doc_id,
                attempt_no,
                &ack,
                &wire_started,
                &wire_finished,
            )
            .await
            {
                Ok(_) => histogram.sent_match_to_kvt1 += 1,
                Err(e) => emit_dispatch_error(pool, doc_id, "c-sent-match", &e, histogram).await?,
            }
        }
        ProbeOutcome::Mismatch { actual_id } => match cas_sent_to_manual_reconciliation_from_probe(
            pool,
            doc_id,
            attempt_no,
            &actual_id,
            &wire_started,
            &wire_finished,
        )
        .await
        {
            Ok(_) => histogram.sent_mismatch_to_manual += 1,
            Err(e) => emit_dispatch_error(pool, doc_id, "c-sent-mismatch", &e, histogram).await?,
        },
        ProbeOutcome::NotFound => {
            match cas_sent_to_error_retryable_from_probe(
                pool,
                doc_id,
                attempt_no,
                &wire_started,
                &wire_finished,
            )
            .await
            {
                Ok(_) => histogram.sent_not_found_to_error_retryable += 1,
                Err(e) => {
                    emit_dispatch_error(pool, doc_id, "c-sent-notfound", &e, histogram).await?
                }
            }
        }
        ProbeOutcome::TransportRetry { reason } => {
            match complete_probe_trace_no_state_change(
                pool,
                doc_id,
                attempt_no,
                &wire_started,
                &wire_finished,
                transport_trace::OutcomeKind::RetryableTransport,
                "LAST_CHK_TRANSPORT_RETRY",
                &reason,
            )
            .await
            {
                Ok(_) => histogram.sent_probe_failure_deferred += 1,
                Err(e) => {
                    emit_dispatch_error(pool, doc_id, "c-sent-probe-transport", &e, histogram)
                        .await?
                }
            }
        }
        ProbeOutcome::DecodeEscalate { reason } => {
            match complete_probe_trace_no_state_change(
                pool,
                doc_id,
                attempt_no,
                &wire_started,
                &wire_finished,
                transport_trace::OutcomeKind::RetryableServer,
                "LAST_CHK_DECODE_ESCALATE",
                &reason,
            )
            .await
            {
                Ok(_) => histogram.sent_probe_failure_deferred += 1,
                Err(e) => {
                    emit_dispatch_error(pool, doc_id, "c-sent-probe-decode", &e, histogram).await?
                }
            }
        }
        ProbeOutcome::Unexpected { dps_error } => {
            match complete_probe_trace_no_state_change(
                pool,
                doc_id,
                attempt_no,
                &wire_started,
                &wire_finished,
                transport_trace::OutcomeKind::RetryableServer,
                "LAST_CHK_UNEXPECTED",
                &dps_error,
            )
            .await
            {
                Ok(_) => histogram.sent_probe_failure_deferred += 1,
                Err(e) => {
                    emit_dispatch_error(pool, doc_id, "c-sent-probe-unexpected", &e, histogram)
                        .await?
                }
            }
        }
    }
    Ok(())
}

/// Tiny ISO-8601 helper for `last_chk` wire-time capture.  Uses
/// `chrono::Utc::now()` — same source the W7 stage_send wire-time
/// path uses.  Lives here (not in a shared module) because boot
/// recovery is the only ctx-needy reader.
fn iso8601_now() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
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
///
/// **W9.4 M3 fix — per-doc failure containment.**  Each helper call
/// is wrapped in `match`; helper-level Err is caught, audited as
/// `BOOT_DISPATCH_ERROR` WARN, and counted in
/// `histogram.dispatch_errors`.  Only infrastructure-level errors
/// (audit insert itself failing) propagate — those are unrecoverable
/// per-FN failures.  Net: one stuck doc cannot abort sibling
/// reconciliation in branch (c)/(e1).
async fn dispatch_pending_doc(
    pool: &SqlitePool,
    doc: &crate::db::repositories::fiscal_documents::DocumentRow,
    deps: Option<&super::ReconciliationRuntime<'_>>,
    histogram: &mut DispatchHistogram,
) -> anyhow::Result<()> {
    use crate::db::models::enums::DocState;
    // PR-1a plumbing — `deps` is threaded through the dispatch tree so
    // PR-2 can wire ctx-needy branches (PREPARED / SIGNED / SENT /
    // ERROR_RETRYABLE) without re-touching the function signature.
    // PR-1a's SENDING / KVT1 / ENCRYPTED / KVT2 branches do NOT
    // consult `deps`; per ADR-M3-A10 + W0-3 §6.3 / §6.5 / §6.6, those
    // recovery paths are structurally ctx-free.  The DEFERRED branch
    // surfaces `deps_available` in the audit payload so operators can
    // see at which boot tick the runtime composition arrived.
    let deps_available = deps.is_some();
    let doc_id = doc.document_id;
    match doc.state {
        DocState::Sending => match resume_sending_to_error_retryable(pool, doc_id).await {
            Ok(_) => histogram.sending_resumed += 1,
            Err(e) => emit_dispatch_error(pool, doc_id, "c-sending", &e, histogram).await?,
        },
        DocState::Kvt1 => match passive_hold_kvt1(pool, doc_id).await {
            Ok(_) => histogram.kvt1_held += 1,
            Err(e) => emit_dispatch_error(pool, doc_id, "c-kvt1", &e, histogram).await?,
        },
        DocState::Encrypted => {
            // 1-tick deferral per freeze §4.3 MED 6 fix.
            let result = with_immediate(pool, move |tx| {
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
            .await;
            match result {
                Ok(_) => histogram.encrypted_rerouted += 1,
                Err(e) => emit_dispatch_error(pool, doc_id, "c-encrypted", &e, histogram).await?,
            }
        }
        DocState::Kvt2 => {
            // W8 stage_finalize::run — pool + doc_id only, no ctx.
            // Per HIGH 1 fix: distinguish kvt2_finalized (success)
            // from kvt2_failed (stuck-Kvt2 surfaced via
            // BOOT_KVT2_DISPATCH_FAILED audit).
            match crate::services::write_path::stage_finalize::run(pool, doc_id).await {
                Ok(_) => histogram.kvt2_finalized += 1,
                Err(e) => {
                    let payload = serde_json::json!({
                        "document_id": hex_lower(doc_id.as_bytes()),
                        "branch": "c-kvt2",
                        "stage_finalize_error": format!("{e}"),
                        "rationale":
                            "Kvt2 → Ack advance failed; doc remains in Kvt2 \
                             pending operator inspection / next-boot retry",
                    });
                    audit_log::append(
                        pool,
                        "fiscal_document",
                        &hex_lower(doc_id.as_bytes()),
                        "BOOT_KVT2_DISPATCH_FAILED",
                        crate::db::models::enums::Severity::Warning,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                    histogram.kvt2_failed += 1;
                }
            }
        }
        // Ctx-needy states: per-state dispatch.  W11 PR-2 wires
        // SIGNED + ERROR_RETRYABLE through `stage_send::run`; SENT
        // + PREPARED are wired in the same PR's subsequent commits.
        //
        // None-path (`reconcile_pending` legacy entry) preserves the
        // W9 BOOT_DISPATCH_DEFERRED behaviour for ALL four states —
        // doc stays in source state, deps_available=false in audit.
        DocState::Signed => match deps {
            Some(d) => match stage_send::run(pool, d.dps, doc_id, Some(d.signing_ctx)).await {
                Ok(_) => histogram.signed_dispatched += 1,
                Err(e) => {
                    emit_dispatch_error(pool, doc_id, "c-signed", &anyhow::Error::new(e), histogram)
                        .await?
                }
            },
            None => {
                emit_ctx_needy_deferred(pool, doc_id, doc.state, deps_available).await?;
                histogram.signed_deferred += 1;
            }
        },
        DocState::ErrorRetryable => match deps {
            Some(d) => match stage_send::run(pool, d.dps, doc_id, Some(d.signing_ctx)).await {
                Ok(_) => histogram.error_retryable_dispatched += 1,
                Err(e) => {
                    emit_dispatch_error(
                        pool,
                        doc_id,
                        "c-error-retryable",
                        &anyhow::Error::new(e),
                        histogram,
                    )
                    .await?
                }
            },
            None => {
                emit_ctx_needy_deferred(pool, doc_id, doc.state, deps_available).await?;
                histogram.error_retryable_deferred += 1;
            }
        },
        DocState::Sent => match deps {
            Some(d) => dispatch_sent_via_probe(pool, d, doc, histogram).await?,
            None => {
                emit_ctx_needy_deferred(pool, doc_id, doc.state, deps_available).await?;
                histogram.sent_deferred += 1;
            }
        },
        // PREPARED — still DEFERRED in BOTH Some/None paths until
        // C3 (PR-2b PREPARED wiring) lands.
        DocState::Prepared => {
            emit_ctx_needy_deferred(pool, doc_id, doc.state, deps_available).await?;
            histogram.prepared_deferred += 1;
        }
        // Terminal states should NEVER appear in `list_pending_for_fn`
        // per its WHERE clause (excludes ACK/REJECTED/CANCELLED/
        // OFFLINE_LOCAL_ACK/REQUIRES_MANUAL_RECONCILIATION).  If we
        // observe one here, the SELECT contract is broken — surface
        // as anyhow error rather than silently dispatching.
        //
        // **W9.4 cycle-2 LOW-4 clarification:** this bail() is
        // INTENTIONALLY NOT wrapped in the M3 try-and-audit shim.
        // "Helper-level errors" that the shim absorbs are
        // recoverable: a single doc's state-validation failure,
        // CAS conflict, or transient SQL error.  Terminal-state
        // SELECT contract violation is INFRASTRUCTURE-level (schema
        // bug OR raw-SQL bypass) — wrapping it in audit + continuing
        // iteration would mask a real corruption signal.  Fail-fast
        // is correct here; the surrounding `BootError::ReconciliationFailed`
        // preserves `fiscal_number` attribution per MED-B fix.
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

/// W9.4 M3 fix — try-and-audit shim: emit `BOOT_DISPATCH_ERROR` WARN
/// and increment the dispatch_errors histogram bucket.  Used inside
/// `dispatch_pending_doc` when a helper returns Err that's NOT a
/// fatal infrastructure failure (audit insert failure propagates
/// via `?`).
///
/// Operator-facing impact: one stuck doc surfaces as a single audit
/// row + histogram count; sibling pending docs continue dispatch.
async fn emit_dispatch_error(
    pool: &SqlitePool,
    doc_id: DocumentId,
    branch_tag: &str,
    error: &anyhow::Error,
    histogram: &mut DispatchHistogram,
) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "document_id": hex_lower(doc_id.as_bytes()),
        "branch": branch_tag,
        "dispatch_error": format!("{error}"),
        "rationale":
            "per-doc helper failure absorbed by try-and-audit shim; doc stays in source state",
    });
    audit_log::append(
        pool,
        "fiscal_document",
        &hex_lower(doc_id.as_bytes()),
        "BOOT_DISPATCH_ERROR",
        crate::db::models::enums::Severity::Warning,
        None,
        Some(&payload.to_string()),
    )
    .await?;
    histogram.dispatch_errors += 1;
    Ok(())
}

/// Emit a `BOOT_DISPATCH_DEFERRED` audit for a ctx-needy state whose
/// dispatch arm is not (yet) wired.  Centralises the W9 deferral
/// audit shape that PR-1a + PR-2 share across multiple match arms.
///
/// `deps_available = true` indicates the caller invoked
/// `App::reconcile_pending_with` (runtime composition is in place),
/// but the dispatch arm itself is still pending wire-up.
/// `deps_available = false` mirrors the pre-W11 legacy
/// `App::reconcile_pending` ctx-free entry.
async fn emit_ctx_needy_deferred(
    pool: &SqlitePool,
    doc_id: DocumentId,
    state: crate::db::models::enums::DocState,
    deps_available: bool,
) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "document_id": hex_lower(doc_id.as_bytes()),
        "observed_state": state.as_str(),
        "deps_available": deps_available,
        "rationale": if deps_available {
            "ctx-needy dispatch arm pending wire-up (W11 PR-2 subsequent commits); doc stays in source state"
        } else {
            "ctx-needy dispatch deferred to runtime composition (W11+); doc stays in source state"
        },
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
    Ok(())
}

#[cfg(test)]
mod tests {
    //! W9.4 cycle-2 MED-A fix: prove `emit_dispatch_error`'s contract
    //! in isolation.  The M3 try-and-audit shim is the most
    //! consequential piece of W9.4 — it changes failure mode from
    //! "one stuck doc aborts the whole FN" to "stuck doc surfaces as
    //! audit + counter".  Without this test the contract is
    //! regression-prone.
    //!
    //! **Why lib-unit not integration:** the M3 shim fires only on a
    //! helper returning Err.  Under SWFN + single-thread tokio, no
    //! parallel mutation can inject helper failure between
    //! `list_pending_for_fn` read and `dispatch_pending_doc` call.
    //! Injecting failure via deliberate seed corruption requires
    //! bypassing the schema CHECK / FK constraints we rely on.  The
    //! cleanest test seam is `emit_dispatch_error` directly — the
    //! function IS the M3 contract.

    use super::*;
    use crate::db::models::ids::DocumentId;
    use sqlx::SqlitePool;

    async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = crate::db::open_pool(&dir.path().join("m.db"))
            .await
            .expect("open_pool");
        (dir, pool)
    }

    async fn seed_min_doc(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
        sqlx::query(
            "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
             VALUES ('1234567890', '12345678', 'test')",
        )
        .execute(pool)
        .await
        .unwrap();
        let bytes = vec![doc_byte; 16];
        let req = vec![doc_byte ^ 0xFF; 16];
        let sha = vec![0u8; 32];
        sqlx::query(
            "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
                state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
                payload_json, payload_sha256_canonical) \
             VALUES (?, ?, '1234567890', ?, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
                '2026-01-01T00:00:00Z', '{}', ?)",
        )
        .bind(&bytes)
        .bind(&req)
        .bind(doc_byte as i64)
        .bind(&sha)
        .execute(pool)
        .await
        .unwrap();
        DocumentId::from_bytes(<[u8; 16]>::try_from(bytes.as_slice()).unwrap())
    }

    #[tokio::test]
    async fn emit_dispatch_error_writes_audit_and_increments_counter() {
        let (_dir, pool) = fresh_pool().await;
        let doc = seed_min_doc(&pool, 0xE1).await;
        let mut histogram = DispatchHistogram::default();
        let err = anyhow::anyhow!("synthetic helper failure for test");

        emit_dispatch_error(&pool, doc, "c-test", &err, &mut histogram)
            .await
            .expect("emit_dispatch_error must not fail under healthy DB");

        assert_eq!(histogram.dispatch_errors, 1, "counter incremented");
        assert_eq!(histogram.total_visited(), 1, "total reflects single error");

        // Verify audit row exists with correct shape.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE event_type = 'BOOT_DISPATCH_ERROR'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "single BOOT_DISPATCH_ERROR audit row");

        let payload: String = sqlx::query_scalar(
            "SELECT event_payload_json FROM audit_log \
             WHERE event_type = 'BOOT_DISPATCH_ERROR'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            payload.contains("\"branch\":\"c-test\""),
            "payload carries branch_tag: {payload}"
        );
        assert!(
            payload.contains("synthetic helper failure for test"),
            "payload carries error message: {payload}"
        );
    }

    #[tokio::test]
    async fn emit_dispatch_error_idempotent_under_repeated_calls() {
        // Each call emits its own audit row + bumps counter.  This
        // mirrors the real M3 shim behaviour: N failed dispatches in
        // one branch (c) iteration produce N audit rows + counter=N.
        let (_dir, pool) = fresh_pool().await;
        let doc = seed_min_doc(&pool, 0xE2).await;
        let mut histogram = DispatchHistogram::default();
        let err1 = anyhow::anyhow!("first failure");
        let err2 = anyhow::anyhow!("second failure");

        emit_dispatch_error(&pool, doc, "c-test1", &err1, &mut histogram)
            .await
            .unwrap();
        emit_dispatch_error(&pool, doc, "c-test2", &err2, &mut histogram)
            .await
            .unwrap();

        assert_eq!(histogram.dispatch_errors, 2);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE event_type = 'BOOT_DISPATCH_ERROR'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 2, "two distinct audit rows");
    }
}
