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

use crate::db::models::enums::{NodeMode, Protocol, ShiftState};
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
    /// M3a hardening pass 1 — ER docs whose durable `retry_class`
    /// (per `transport_trace.retry_class`) indicates the recovery
    /// branch is NOT auto-retryable.  Doc transitioned `ErrorRetryable
    /// → RequiresManualReconciliation` with audit
    /// `BOOT_ER_ESCALATED_TO_MANUAL`.  Applies to retry classes:
    /// `FnConfigError` / `WrapperBug` / `OperatorEscalation` /
    /// `MacRecovery` (Severity::Error) and `TerminalReject`
    /// (Severity::Critical — indicates a structurally inconsistent
    /// durable state, since TerminalReject routes targets `Rejected`
    /// directly and should never land in ErrorRetryable).
    pub error_retryable_escalated_to_manual: usize,
    /// M3a hardening pass 1 — ER docs whose durable `retry_class` is
    /// `ProbeRequired` (status `-2` / `-15` close-shift, status `0`
    /// decode-unknown).  Recovery does NOT auto-retry — submit-time
    /// `last_chk` reconciliation is deferred to M5's generic SENDING
    /// reconciler (per PRRO_GATE-6bj M3a closure annotation).  Doc
    /// stays in ER with audit `BOOT_ER_PROBE_DEFERRED`
    /// (Severity::Warning).
    pub error_retryable_probe_deferred: usize,
    /// M3a hardening pass 1 — ER docs whose durable `retry_class`
    /// row is missing OR carries an unknown wire-string OR is
    /// pre-migration-012 NULL.  Recovery has no durable evidence
    /// to choose a class; per `RetryClass::from_wire_str` contract,
    /// indeterminate state is held without auto-retry.  Doc stays
    /// in ER with audit `BOOT_ER_RETRY_CLASS_INDETERMINATE`
    /// (Severity::Error — durable forensic evidence is missing).
    pub error_retryable_indeterminate_deferred: usize,
    /// M3a hardening pass 1 — PREPARED docs whose
    /// `dispatch_prepared_via_chain` snapshot detected drift
    /// between `fiscal_documents` extras and the matching
    /// `ingress_inbox` row (mismatch on `fiscal_number`,
    /// `payload_sha256_canonical`, or `doc_type` vs
    /// `operation_type`).  Doc stays in PREPARED with audit
    /// `BOOT_PREPARED_REPLAY_DRIFT` (Severity::Critical).  Operator
    /// manual intervention required.
    pub prepared_replay_drift_deferred: usize,
    /// M3a hardening pass 1 — ER docs that would otherwise route
    /// to `stage_send::run` via the TransientRetry arm but have
    /// `attempts_used(doc_id) >= MAX_BOOT_ATTEMPTS` (W9 freeze
    /// §4.0).  Doc transitions `ErrorRetryable →
    /// RequiresManualReconciliation` with audit
    /// `BOOT_ER_BUDGET_EXHAUSTED` (Severity::Error).  Closes
    /// H2 (boot-attempt budget cap declared in
    /// `MAX_BOOT_ATTEMPTS` but never enforced — without this an
    /// infinitely-failing TransientRetry doc would re-dispatch
    /// `send_chk` on every boot tick forever).
    pub error_retryable_budget_exhausted: usize,
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
            + self.error_retryable_escalated_to_manual
            + self.error_retryable_probe_deferred
            + self.error_retryable_indeterminate_deferred
            + self.prepared_replay_drift_deferred
            + self.error_retryable_budget_exhausted
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
        self.error_retryable_escalated_to_manual += other.error_retryable_escalated_to_manual;
        self.error_retryable_probe_deferred += other.error_retryable_probe_deferred;
        self.error_retryable_indeterminate_deferred += other.error_retryable_indeterminate_deferred;
        self.prepared_replay_drift_deferred += other.prepared_replay_drift_deferred;
        self.error_retryable_budget_exhausted += other.error_retryable_budget_exhausted;
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

            // (2) Complete transport_trace row with RetryableServer
            // outcome.  M3a hardening pass 1 — `retry_class =
            // TransientRetry` is the semantically correct durable
            // label: probe `NotFound` means DPS has no record, so
            // tick-2 of the ADR-M3-A9 retry path is the canonical
            // transient retry path.  The new ER dispatcher
            // (`dispatch_error_retryable_by_class`) routes
            // TransientRetry rows through `stage_send::run`.  Writing
            // `None` here would route the doc to the indeterminate-
            // hold branch and break the two-tick contract.
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
                    retry_class: Some(
                        crate::services::write_path::error_routing::RetryClass::TransientRetry
                            .as_str()
                            .to_string(),
                    ),
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
    deps: Option<&super::RuntimeView<'_>>,
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
            "error_retryable_escalated_to_manual": histogram.error_retryable_escalated_to_manual,
            "error_retryable_probe_deferred": histogram.error_retryable_probe_deferred,
            "error_retryable_indeterminate_deferred": histogram.error_retryable_indeterminate_deferred,
            "prepared_replay_drift_deferred": histogram.prepared_replay_drift_deferred,
            "error_retryable_budget_exhausted": histogram.error_retryable_budget_exhausted,
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
    deps: &super::RuntimeView<'_>,
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

/// M3a hardening pass 1 — CAS `ErrorRetryable → RequiresManualReconciliation`
/// for ER docs whose durable `retry_class` indicates the recovery
/// branch is NOT auto-retryable (FnConfigError, WrapperBug,
/// OperatorEscalation, MacRecovery, TerminalReject).
///
/// Single `with_immediate` envelope: CAS `ErrorRetryable →
/// RequiresManualReconciliation` (whitelisted at base — see
/// `fiscal_documents::allowed_transition` line 160) + audit
/// `BOOT_ER_ESCALATED_TO_MANUAL`.  No DPS call; no signing.
///
/// **Severity selection.**  Caller passes `severity` per class:
///   - `Severity::Error` for FnConfigError / WrapperBug /
///     OperatorEscalation / MacRecovery (durable evidence indicates
///     operator action needed; not retryable but expected).
///   - `Severity::Critical` for TerminalReject (a TerminalReject row
///     should have landed the doc directly in `Rejected`, never in
///     `ErrorRetryable` — observing it here is a structural breach).
///
/// CAS guard `WHERE state = 'ERROR_RETRYABLE'` makes a second
/// invocation a no-op (idempotent under boot replay).
pub async fn cas_error_retryable_to_manual_reconciliation(
    pool: &SqlitePool,
    doc_id: DocumentId,
    retry_class_str: &str,
    severity: crate::db::models::enums::Severity,
) -> anyhow::Result<bool> {
    let retry_class_owned = retry_class_str.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let cas = sqlx::query(
                "UPDATE fiscal_documents SET state = 'REQUIRES_MANUAL_RECONCILIATION' \
                 WHERE document_id = ? AND state = 'ERROR_RETRYABLE'",
            )
            .bind(doc_id)
            .execute(&mut **tx)
            .await?;
            if cas.rows_affected() != 1 {
                return Ok::<bool, anyhow::Error>(false);
            }

            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-error-retryable-escalated",
                "retry_class": retry_class_owned,
                "rationale":
                    "ErrorRetryable + non-retryable durable retry_class — operator triage required; no auto-retry per stage_send §4.2",
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_ER_ESCALATED_TO_MANUAL",
                severity,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<bool, anyhow::Error>(true)
        })
    })
    .await
}

/// M3a hardening pass 1 — H2 closure: CAS `ErrorRetryable →
/// RequiresManualReconciliation` for ER/TransientRetry docs whose
/// boot-attempt budget (`attempts_used(doc_id) >=
/// MAX_BOOT_ATTEMPTS`) is exhausted.  Same single `with_immediate`
/// envelope as the per-class escalation helper, but with a
/// distinct audit type so operator dashboards can alert on the
/// "infinite TransientRetry stuck" signal separately from
/// per-class non-retryable escalations.
///
/// Payload carries `attempts_used`, `max_boot_attempts`, and
/// `retry_class` for forensics.  Severity::Error (operator triage
/// required, but not a structural breach).
async fn cas_error_retryable_budget_exhausted(
    pool: &SqlitePool,
    doc_id: DocumentId,
    attempts_used: i64,
    retry_class_str: &str,
) -> anyhow::Result<bool> {
    let retry_class_owned = retry_class_str.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let cas = sqlx::query(
                "UPDATE fiscal_documents SET state = 'REQUIRES_MANUAL_RECONCILIATION' \
                 WHERE document_id = ? AND state = 'ERROR_RETRYABLE'",
            )
            .bind(doc_id)
            .execute(&mut **tx)
            .await?;
            if cas.rows_affected() != 1 {
                return Ok::<bool, anyhow::Error>(false);
            }

            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-error-retryable-budget-exhausted",
                "retry_class": retry_class_owned,
                "attempts_used": attempts_used,
                "max_boot_attempts": MAX_BOOT_ATTEMPTS,
                "rationale":
                    "TransientRetry boot-attempt budget exhausted (attempts_used >= MAX_BOOT_ATTEMPTS); doc would re-burn DPS quota every boot tick without escalation; operator triage required per W9 freeze §4.0",
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_ER_BUDGET_EXHAUSTED",
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

/// M3a hardening pass 1 — emit a forensic audit for an ER doc held
/// without state change (ProbeRequired / indeterminate `retry_class`).
/// No CAS, no DPS — the doc stays in `ErrorRetryable` until M5's
/// generic SENDING reconciler (ProbeRequired) handles it OR an
/// operator manually resolves the indeterminate state.
async fn emit_error_retryable_hold_audit(
    pool: &SqlitePool,
    doc_id: DocumentId,
    event_type: &'static str,
    severity: crate::db::models::enums::Severity,
    retry_class_label: &str,
    rationale: &'static str,
) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "document_id": hex_lower(doc_id.as_bytes()),
        "branch": "c-error-retryable-hold",
        "retry_class": retry_class_label,
        "rationale": rationale,
    });
    audit_log::append(
        pool,
        "fiscal_document",
        &hex_lower(doc_id.as_bytes()),
        event_type,
        severity,
        None,
        Some(&payload.to_string()),
    )
    .await?;
    Ok(())
}

/// M3a hardening pass 1 — ER recovery orchestrator.  Reads the
/// doc's last-attempt `retry_class` from `transport_trace` and
/// dispatches by class:
///
///   - **`TransientRetry`** → `stage_send::run` (Pattern B
///     `ErrorRetryable → Sending → wire`).  Existing W11 PR-2a
///     wiring; histogram counter `error_retryable_dispatched`.
///   - **`FnConfigError` / `WrapperBug` / `OperatorEscalation` /
///     `MacRecovery`** → CAS `ErrorRetryable →
///     RequiresManualReconciliation` (Severity::Error).
///     `BOOT_ER_ESCALATED_TO_MANUAL`; counter
///     `error_retryable_escalated_to_manual`.
///   - **`TerminalReject`** → same CAS but `Severity::Critical`
///     (structurally inconsistent: TerminalReject should target
///     `Rejected` directly, never ER).
///   - **`ProbeRequired`** → hold; audit `BOOT_ER_PROBE_DEFERRED`
///     (Severity::Warning); counter `error_retryable_probe_deferred`.
///     Submit-time `last_chk` reconciliation is deferred to M5 per
///     PRRO_GATE-6bj M3a closure annotation.
///   - **`None` (missing / unknown / pre-migration-012 NULL)** →
///     hold; audit `BOOT_ER_RETRY_CLASS_INDETERMINATE`
///     (Severity::Error — durable evidence missing); counter
///     `error_retryable_indeterminate_deferred`.
///
/// **Why the filter is mandatory.**  `stage_send::run`'s module
/// docs (lines 33-40) explicitly call out that calling `run`
/// repeatedly on a non-`TransientRetry` ER doc produces an
/// unbounded crash-loop: same envelope, same server reply, same
/// `ErrorRetryable` landing.  This dispatcher implements the
/// "tests + ops scripts MUST manually filter" guidance the W7
/// design freeze flagged.
async fn dispatch_error_retryable_by_class(
    pool: &SqlitePool,
    deps: &super::RuntimeView<'_>,
    doc: &crate::db::repositories::fiscal_documents::DocumentRow,
    histogram: &mut DispatchHistogram,
) -> anyhow::Result<()> {
    use crate::db::repositories::transport_trace as tt;
    use crate::services::write_path::error_routing::RetryClass;

    let doc_id = doc.document_id;

    let retry_class = tt::last_attempt_retry_class_for(pool, doc_id).await?;

    match retry_class {
        Some(RetryClass::TransientRetry) => {
            // M3a hardening pass 1 — H2 closure: enforce the
            // boot-attempt budget cap BEFORE re-dispatching.
            // `MAX_BOOT_ATTEMPTS = 5` is declared in W9 freeze §4.0;
            // without this gate an infinitely-failing TransientRetry
            // doc would re-burn DPS quota on every boot tick.
            // `attempts_used` counts ALL transport_trace rows for
            // the doc (both completed and in-flight), so the cap
            // covers crash-mid-send attempts too per W9 docstring.
            let attempts = tt::attempts_used(pool, doc_id).await?;
            if attempts >= MAX_BOOT_ATTEMPTS {
                match cas_error_retryable_budget_exhausted(
                    pool,
                    doc_id,
                    attempts,
                    RetryClass::TransientRetry.as_str(),
                )
                .await
                {
                    Ok(_) => histogram.error_retryable_budget_exhausted += 1,
                    Err(e) => {
                        emit_dispatch_error(
                            pool,
                            doc_id,
                            "c-error-retryable-budget",
                            &e,
                            histogram,
                        )
                        .await?
                    }
                }
                return Ok(());
            }
            match stage_send::run(pool, deps.dps, doc_id, Some(deps.signing_ctx)).await {
                Ok(_) => histogram.error_retryable_dispatched += 1,
                Err(e) => {
                    emit_dispatch_error(
                        pool,
                        doc_id,
                        "c-error-retryable-transient",
                        &anyhow::Error::new(e),
                        histogram,
                    )
                    .await?
                }
            }
        }
        Some(rc @ (RetryClass::FnConfigError
        | RetryClass::WrapperBug
        | RetryClass::OperatorEscalation
        | RetryClass::MacRecovery)) => {
            match cas_error_retryable_to_manual_reconciliation(
                pool,
                doc_id,
                rc.as_str(),
                crate::db::models::enums::Severity::Error,
            )
            .await
            {
                Ok(_) => histogram.error_retryable_escalated_to_manual += 1,
                Err(e) => {
                    emit_dispatch_error(
                        pool,
                        doc_id,
                        "c-error-retryable-escalate",
                        &e,
                        histogram,
                    )
                    .await?
                }
            }
        }
        Some(RetryClass::TerminalReject) => {
            // Structurally inconsistent: TerminalReject targets `Rejected`
            // directly per `error_routing::route_dps_error`; an ER doc
            // tagged TerminalReject is durable evidence of a routing /
            // CAS skew.  Escalate with CRITICAL severity.
            match cas_error_retryable_to_manual_reconciliation(
                pool,
                doc_id,
                RetryClass::TerminalReject.as_str(),
                crate::db::models::enums::Severity::Critical,
            )
            .await
            {
                Ok(_) => histogram.error_retryable_escalated_to_manual += 1,
                Err(e) => {
                    emit_dispatch_error(
                        pool,
                        doc_id,
                        "c-error-retryable-terminal-inconsistent",
                        &e,
                        histogram,
                    )
                    .await?
                }
            }
        }
        Some(RetryClass::ProbeRequired) => {
            match emit_error_retryable_hold_audit(
                pool,
                doc_id,
                "BOOT_ER_PROBE_DEFERRED",
                crate::db::models::enums::Severity::Warning,
                RetryClass::ProbeRequired.as_str(),
                "ProbeRequired retry_class — submit-time last_chk reconciliation deferred to M5 generic SENDING reconciler (per PRRO_GATE-6bj M3a annotation); doc stays in ERROR_RETRYABLE",
            )
            .await
            {
                Ok(_) => histogram.error_retryable_probe_deferred += 1,
                Err(e) => {
                    emit_dispatch_error(
                        pool,
                        doc_id,
                        "c-error-retryable-probe",
                        &e,
                        histogram,
                    )
                    .await?
                }
            }
        }
        None => {
            // Durable retry_class is missing / unknown / pre-migration-012
            // NULL — recovery has no evidence to choose a class.  Per
            // `RetryClass::from_wire_str` contract (`error_routing.rs:125-140`),
            // None is treated as "indeterminate from durable evidence",
            // forwarded to manual triage rather than auto-retried.
            match emit_error_retryable_hold_audit(
                pool,
                doc_id,
                "BOOT_ER_RETRY_CLASS_INDETERMINATE",
                crate::db::models::enums::Severity::Error,
                "<none>",
                "ER doc has no durable retry_class (transport_trace row missing OR retry_class NULL/unknown); operator triage required; doc stays in ERROR_RETRYABLE",
            )
            .await
            {
                Ok(_) => histogram.error_retryable_indeterminate_deferred += 1,
                Err(e) => {
                    emit_dispatch_error(
                        pool,
                        doc_id,
                        "c-error-retryable-indeterminate",
                        &e,
                        histogram,
                    )
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

/// W11 PR-2b — PREPARED crash-recovery orchestrator.
///
/// W0-3 §6.1 mandates: a PREPARED doc crashed before sign / send.
/// Recovery drives forward through the canonical W6 + W7 chain:
/// `stage_sign::run` (Prepared → Signed) → `stage_send::run`
/// (Signed → Sending → Sent / routed target).  Both stages manage
/// their own `with_immediate` envelopes; `dispatch_prepared_via_chain`
/// itself does NO crypto / network IO inside any envelope (W3
/// invariant preserved structurally).
///
/// **WorkerContext reconstruction.**  `DocumentRow` does not carry
/// the payload columns stage_sign needs (`business_ts`,
/// `payload_json`, `total_sum_kop`, `payload_sha256_canonical`); the
/// inbox row, node_state, and active_shift also live outside the
/// dispatcher's signature.  A single short `with_immediate` envelope
/// reads all five in one atomic snapshot:
///   1. raw SELECT of fiscal_documents payload extras (no DocumentRow
///      extension to keep PR-2b minimal-diff);
///   2. raw SELECT of the matching inbox row (no `ingress_inbox`
///      pool/tx reader exists — this is the sole caller, so the read
///      lives inline);
///   3. `node_state::get_tx` (single source of truth — the live worker
///      reads node_state inside stage 1 the same way);
///   4. `shifts::get_tx` when `node_state.shift_state == Opened` AND
///      `current_shift_id IS Some`.
///
/// The envelope contains ONLY reads + struct decoding; foreign IO
/// (crypto, DPS) is invoked AFTER the envelope returns.
///
/// **histogram contract.**  `prepared_dispatched += 1` on the
/// stage_send happy path.  Both stage_sign and stage_send errors
/// route through `emit_dispatch_error` (M3 try-and-audit shim);
/// doc state stays in PREPARED (stage_sign error before CAS) or
/// SIGNED (stage_sign succeeded, stage_send error) depending on
/// where the chain failed — next boot tick re-dispatches via the
/// appropriate arm.
async fn dispatch_prepared_via_chain(
    pool: &SqlitePool,
    deps: &super::RuntimeView<'_>,
    doc: &crate::db::repositories::fiscal_documents::DocumentRow,
    histogram: &mut DispatchHistogram,
) -> anyhow::Result<()> {
    use crate::db::repositories::{ingress_inbox::InboxRow, node_state, shifts};
    use crate::services::write_path::stage_sign;
    use crate::services::write_path::types::{CanonicalFiscalCommand, WorkerContext};
    use sqlx::Row as _;

    let doc_id = doc.document_id;
    let fn_id_for_read = doc.fiscal_number.clone();
    // Capture doc.doc_type as owned `Copy` value BEFORE the
    // `with_immediate` closure — the closure must not borrow the
    // `&DocumentRow` parameter (its lifetime is non-`'static`, but
    // `Box::pin(async move {...})` futures returned by `with_immediate`
    // need to be `'static`).
    let doc_type_copy = doc.doc_type;

    // M3a hardening pass 1 — Patch 3 (PREPARED replay drift detection).
    // The snapshot envelope cross-checks `fiscal_documents` row against
    // its matching `ingress_inbox` row (per stage_acquire step 1b at
    // `stage_acquire.rs:58-89`).  On any mismatch, return the `Drift`
    // outcome WITHOUT proceeding to stage_sign; the caller emits a
    // CRITICAL audit + holds the doc in PREPARED.  Live `stage_acquire`
    // already fail-closes on drift at first ingress; recovery
    // re-asserts the same invariant on every boot tick.
    type PreparedInputs = (
        CanonicalFiscalCommand,
        InboxRow,
        node_state::NodeStateRow,
        Option<shifts::ShiftRow>,
    );

    enum SnapshotOutcome {
        // PreparedInputs is large (≈ 250 bytes — InboxRow + NodeStateRow +
        // optional ShiftRow); box the success variant to keep the enum
        // discriminant cheap (clippy::large_enum_variant).
        Ok(Box<PreparedInputs>),
        Drift {
            fd_fiscal_number: String,
            inbox_fiscal_number: String,
            fd_payload_sha_hex: String,
            inbox_payload_sha_hex: String,
            fd_doc_type: String,
            inbox_operation_type: String,
            /// M3a hardening pass 2 — Finding 4 closure: byte-equality
            /// check between `fd.payload_json` and `inbox.payload_json`
            /// catches drift the hash-only check would miss (DB
            /// corruption / hand-edit where one column was updated
            /// without the other; payload-level mutation between
            /// stage_acquire write and recovery read).  `true` when
            /// the two `payload_json` strings differ verbatim.
            payload_json_mismatch: bool,
        },
    }

    // (1) Atomic snapshot — fiscal_documents extras + inbox + node_state
    // + active_shift, all inside one short `with_immediate` envelope.
    // No foreign IO; W3 invariant preserved.
    let inputs_result: anyhow::Result<SnapshotOutcome> = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // (1a) Read payload columns + request_id + fiscal_number
            // from fiscal_documents (fiscal_number additionally needed
            // for the drift cross-check against inbox row).
            let fd_row = sqlx::query(
                "SELECT request_id, fiscal_number, business_ts, total_sum_kop, payload_json, \
                        payload_sha256_canonical \
                 FROM fiscal_documents WHERE document_id = ?",
            )
            .bind(doc_id)
            .fetch_one(&mut **tx)
            .await?;
            let request_id_v: Vec<u8> = fd_row.try_get("request_id")?;
            let request_id: [u8; 16] = request_id_v.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!("fiscal_documents.request_id length != 16 for doc {doc_id:?}")
            })?;
            let fd_fiscal_number: String = fd_row.try_get("fiscal_number")?;
            let business_ts: String = fd_row.try_get("business_ts")?;
            let total_sum_kop: Option<i64> = fd_row.try_get("total_sum_kop")?;
            let payload_json: String = fd_row.try_get("payload_json")?;
            let sha_v: Vec<u8> = fd_row.try_get("payload_sha256_canonical")?;
            let payload_sha: [u8; 32] = sha_v.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!(
                    "fiscal_documents.payload_sha256_canonical length != 32 for doc {doc_id:?}"
                )
            })?;

            // (1b) Read the matching inbox row.  No `ingress_inbox`
            // pool/tx reader exists today; PR-2b is the sole caller,
            // so the read lives inline rather than adding a one-off
            // repo helper.
            let req_slice: &[u8] = &request_id;
            let inbox_row_db = sqlx::query(
                "SELECT request_id, fiscal_number, protocol, operation_type, \
                        idempotency_key, status, payload_json, \
                        payload_sha256_canonical, correlation_id, received_at \
                 FROM ingress_inbox WHERE request_id = ?",
            )
            .bind(req_slice)
            .fetch_one(&mut **tx)
            .await?;
            let inbox_request_id_v: Vec<u8> = inbox_row_db.try_get("request_id")?;
            let inbox_request_id: [u8; 16] =
                inbox_request_id_v.as_slice().try_into().map_err(|_| {
                    anyhow::anyhow!("ingress_inbox.request_id length != 16 for doc {doc_id:?}")
                })?;
            let inbox_sha_v: Vec<u8> = inbox_row_db.try_get("payload_sha256_canonical")?;
            let inbox_sha: [u8; 32] = inbox_sha_v.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!(
                    "ingress_inbox.payload_sha256_canonical length != 32 for doc {doc_id:?}"
                )
            })?;
            let inbox_row = InboxRow {
                request_id: inbox_request_id,
                fiscal_number: inbox_row_db.try_get("fiscal_number")?,
                protocol: inbox_row_db.try_get::<Protocol, _>("protocol")?,
                operation_type: inbox_row_db.try_get("operation_type")?,
                idempotency_key: inbox_row_db.try_get("idempotency_key")?,
                status: inbox_row_db.try_get("status")?,
                payload_json: inbox_row_db.try_get("payload_json")?,
                payload_sha256_canonical: inbox_sha,
                correlation_id: inbox_row_db.try_get("correlation_id")?,
                received_at: inbox_row_db.try_get("received_at")?,
            };

            // M3a hardening pass 1 — Patch 3 (drift cross-check).
            // stage_acquire step 1b establishes the invariant at first
            // ingress: `(fd.fiscal_number, fd.payload_sha256_canonical,
            // fd.doc_type)` MUST equal `(inbox.fiscal_number,
            // inbox.payload_sha256_canonical, inbox.operation_type)`.
            //
            // M3a hardening pass 2 — Finding 4: extended with explicit
            // `payload_json` byte-equality between fd and inbox.  The
            // hash-only check from pass 1 left a defence-in-depth gap:
            // DB corruption / hand-edit could mutate one row's
            // `payload_json` without re-hashing the other's
            // `payload_sha256_canonical`, and the snapshot would have
            // built `WorkerContext.command` from `fd.payload_json` even
            // though that bytes no longer matched what inbox / hash
            // attested to.  Adding the byte-equality closes that gap
            // before stage_sign runs canonical-XML build on
            // `command.payload_json`.  Recompute of canonical hash is
            // intentionally deferred to follow-up (no Rust-side
            // canonicalization helper exists in M3a; ingress adapter
            // chain lands in M4).
            //
            // Recovery re-asserts the invariant on every boot tick.
            // Drift = DB corruption / hand-edit / migration artifact —
            // never a business-level reject.  Fail-closed hold:
            // return `Drift` (no state mutation, no sign/send); caller
            // emits CRITICAL audit.
            let payload_json_mismatch = payload_json != inbox_row.payload_json;
            let drift = fd_fiscal_number != inbox_row.fiscal_number
                || payload_sha != inbox_row.payload_sha256_canonical
                || doc_type_copy.as_str() != inbox_row.operation_type
                || payload_json_mismatch;
            if drift {
                return Ok::<SnapshotOutcome, anyhow::Error>(SnapshotOutcome::Drift {
                    fd_fiscal_number,
                    inbox_fiscal_number: inbox_row.fiscal_number.clone(),
                    fd_payload_sha_hex: hex_lower(&payload_sha),
                    inbox_payload_sha_hex: hex_lower(&inbox_row.payload_sha256_canonical),
                    fd_doc_type: doc_type_copy.as_str().to_string(),
                    inbox_operation_type: inbox_row.operation_type.clone(),
                    payload_json_mismatch,
                });
            }

            // (1c) Read node_state via the tx-bound repo helper.
            let ns = node_state::get_tx(tx, &fn_id_for_read)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "node_state missing for fn {fn_id_for_read} during PREPARED recovery"
                    )
                })?;

            // (1d) Resolve active_shift only when node_state advertises
            // an open shift — mirrors stage_acquire step 5.  Recovery
            // does NOT escalate ShiftInvariantViolation (the live worker
            // does that on first ingress; boot recovery just passes
            // None through and lets stage_sign drive on the persisted
            // doc).
            let active_shift = match (ns.shift_state, &ns.current_shift_id) {
                (ShiftState::Opened, Some(sid)) => shifts::get_tx(tx, *sid).await?,
                _ => None,
            };

            let command = CanonicalFiscalCommand {
                doc_type: doc_type_copy,
                business_ts,
                total_sum_kop,
                payload_json,
                payload_sha256_canonical: payload_sha,
            };

            Ok::<SnapshotOutcome, anyhow::Error>(SnapshotOutcome::Ok(Box::new((
                command,
                inbox_row,
                ns,
                active_shift,
            ))))
        })
    })
    .await;

    let (command, inbox, node_state, active_shift) = match inputs_result {
        Ok(SnapshotOutcome::Ok(boxed)) => *boxed,
        Ok(SnapshotOutcome::Drift {
            fd_fiscal_number,
            inbox_fiscal_number,
            fd_payload_sha_hex,
            inbox_payload_sha_hex,
            fd_doc_type,
            inbox_operation_type,
            payload_json_mismatch,
        }) => {
            // M3a hardening pass 1 — Patch 3 + pass 2 Finding 4.
            // Drift between fiscal_documents and ingress_inbox
            // detected; emit CRITICAL audit + counter += 1, doc
            // stays in PREPARED, no sign/send invoked.  Operator
            // manual intervention required (drift is corruption /
            // migration artifact; recovery cannot decide direction
            // safely).
            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-prepared-replay-drift",
                "fd_fiscal_number": fd_fiscal_number,
                "inbox_fiscal_number": inbox_fiscal_number,
                "fd_payload_sha256_canonical_hex": fd_payload_sha_hex,
                "inbox_payload_sha256_canonical_hex": inbox_payload_sha_hex,
                "fd_doc_type": fd_doc_type,
                "inbox_operation_type": inbox_operation_type,
                "payload_json_mismatch": payload_json_mismatch,
                "rationale":
                    "fiscal_documents ↔ ingress_inbox drift (mismatch on fiscal_number / payload_sha256_canonical / doc_type vs operation_type / payload_json byte-equality); recovery holds doc in PREPARED — operator triage required",
            });
            audit_log::append(
                pool,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_PREPARED_REPLAY_DRIFT",
                crate::db::models::enums::Severity::Critical,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            histogram.prepared_replay_drift_deferred += 1;
            return Ok(());
        }
        Err(e) => {
            // Snapshot read failed — emit dispatch_error and bail
            // without further state mutation; doc stays in PREPARED.
            emit_dispatch_error(pool, doc_id, "c-prepared-inputs", &e, histogram).await?;
            return Ok(());
        }
    };

    let worker_ctx = WorkerContext {
        inbox,
        command,
        node_state,
        active_shift,
        document: doc.clone(),
    };

    // (2) stage_sign::run drives PREPARED → SIGNED via its own
    // envelopes (pin-then-persist).  Crypto invoked between the two
    // envelopes per W6 Pattern A; W3 scanner accepts.  On Err the
    // doc stays in PREPARED — next boot tick re-dispatches here.
    if let Err(sign_err) = stage_sign::run(pool, deps.signing_ctx, worker_ctx).await {
        emit_dispatch_error(
            pool,
            doc_id,
            "c-prepared-sign",
            &anyhow::Error::new(sign_err),
            histogram,
        )
        .await?;
        return Ok(());
    }

    // (3) stage_send::run drives SIGNED → SENT (or routed target) via
    // Pattern B.  Doc-id only — stage_send re-reads from the pool;
    // the runtime composition supplies `Some(signing_ctx)` for the
    // optional MAC-recovery loop body.  On Err the doc stays in
    // SIGNED — next boot tick re-dispatches via the PR-2a SIGNED arm.
    match stage_send::run(pool, deps.dps, doc_id, Some(deps.signing_ctx)).await {
        Ok(_) => histogram.prepared_dispatched += 1,
        Err(send_err) => {
            emit_dispatch_error(
                pool,
                doc_id,
                "c-prepared-send",
                &anyhow::Error::new(send_err),
                histogram,
            )
            .await?;
        }
    }
    Ok(())
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
    deps: Option<&super::RuntimeView<'_>>,
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
            Some(d) => dispatch_error_retryable_by_class(pool, d, doc, histogram).await?,
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
        DocState::Prepared => match deps {
            Some(d) => dispatch_prepared_via_chain(pool, d, doc, histogram).await?,
            None => {
                emit_ctx_needy_deferred(pool, doc_id, doc.state, deps_available).await?;
                histogram.prepared_deferred += 1;
            }
        },
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
