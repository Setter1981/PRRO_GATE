//! W9 boot reconciliation — per-FN dispatch + per-DocState helpers.
//!
//! Per design freeze §4 + §5.4:
//! - W9.2 (this slice) lands 3 per-DocState helpers + `MAX_BOOT_ATTEMPTS`
//!   constant + a stub `run_boot_reconciliation` returning `Ok(())`.
//! - W9.3 wires the 6-branch decision tree on top.
//!
//! All helpers wrap exactly ONE `with_immediate` envelope each (W3
//! single-writer invariant; no foreign IO inside).
//!
//! **M3b W1 — service-layer transition_with_audit helper.**
//! Pre-M3b, the boot-phase helpers issued raw `UPDATE fiscal_documents
//! SET state = '<X>' WHERE … AND state = '<Y>'` SQL directly inside
//! their `with_immediate` envelopes, bypassing
//! [`crate::db::repositories::fiscal_documents::transition_state`]
//! (which is tx-bound and whitelist-gated).  That left the whitelist
//! gate not running on boot-phase CAS, which the M3a-handoff §6.1
//! carry-forward called out as a structural drift hazard.
//!
//! W1 closes that gap by introducing a service-layer composition
//! helper, [`transition_with_audit`], that:
//!   1. delegates the CAS to the existing repository
//!      `fiscal_documents::transition_state` (whitelist still the
//!      single source of truth);
//!   2. writes the boot-phase audit row via [`audit_log::append_tx`]
//!      ONLY when the transition outcome is
//!      [`TransitionOutcome::Applied`] — `Conflict` / `NotFound` /
//!      `Forbidden` paths leave no audit trail.
//!
//! The 7 prior raw-CAS sites are refactored to compose this helper
//! (simple sites) or to use `fiscal_documents::transition_state`
//! directly (sites that need supplementary writes ordered between
//! CAS and audit — see `advance_sent_to_kvt1_from_probe`,
//! `cas_sent_to_manual_reconciliation_from_probe`,
//! `cas_sent_to_error_retryable_from_probe`).  Repository layer is
//! unchanged: no new pub fn on `fiscal_documents`.

use sqlx::SqlitePool;

use crate::db::models::enums::{DocState, NodeMode, Protocol, Severity, ShiftState};
use crate::db::models::ids::{DocumentId, ShiftId};
use crate::db::repositories::fiscal_documents::{self, TransitionOutcome};
use crate::db::repositories::{audit_log, document_files, transport_trace};
use crate::db::tx::{with_immediate, WriteTxConn};
use crate::services::shift::transition as shift_transition;
use crate::services::write_path::signer_guard::SignerCashierMismatch as Scm;
use crate::services::write_path::stage_send;
use crate::services::write_path::types::hex_encode_lower as hex_lower;
use crate::transports::dps::dto::CheckAck;
use sha2::{Digest, Sha256};

/// M3b W1 — service-layer transition + audit composition helper.
///
/// Drives a fiscal-document state transition via the repository
/// [`fiscal_documents::transition_state`] (which gates on the
/// `allowed_transition` whitelist — the single source of truth for
/// valid `(from, to)` pairs) and, **only when the transition is
/// `Applied`**, writes one `audit_log` row with the supplied
/// `event_type` + `severity` + payload built by the closure.
///
/// Operator review criteria (M3b W1, 2026-05-14):
///   - Helper must call existing `fiscal_documents::transition_state`,
///     so the whitelist stays a single source of truth.  ✓
///   - Audit row written ONLY on `TransitionOutcome::Applied`.  ✓
///   - `Conflict` / `NotFound` / `Forbidden` outcomes leave NO audit
///     row (caller observes outcome + decides what to do).  ✓
///   - Stays inside the caller's `with_immediate` envelope — no
///     network, no crypto, no extra DB transaction.  ✓
///
/// **Closure shape.**  `FnOnce() -> serde_json::Value` (sync,
/// monomorphised, no async-lifetime complexity per plan OQ1 closure
/// — closed in-plan 2026-05-14).  Caller pre-computes any DB-read
/// data needed for the payload OUTSIDE the closure, capturing the
/// values, so the closure body itself does only JSON construction.
/// The closure is invoked ONLY on `Applied` — its side-effects (if
/// any beyond JSON building) only happen on the success path.
///
/// **Idempotency / replay.**  Identical to the prior raw-CAS shape:
/// a second invocation against the same `(doc_id, from, to)` after
/// the first applied returns `TransitionOutcome::Conflict` (doc
/// already in `to`), no audit row.  Replay-safe.
///
/// **Forbidden handling.**  If the `(from, to)` pair is not in the
/// `allowed_transition` whitelist (which should never happen for the
/// 7 boot_phase sites — all their pairs are explicitly whitelisted
/// at base, see fiscal_documents.rs:131-180), the helper returns
/// `Ok(TransitionOutcome::Forbidden)` without touching the DB.  No
/// CAS attempted, no audit row written.  Callers can treat this as
/// a programmer error (matches an `unreachable!` in a more strict
/// design, but plumbing the outcome through preserves the existing
/// caller contract that just inspects "rows_affected == 1").
async fn transition_with_audit<F>(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    from: DocState,
    to: DocState,
    event_type: &str,
    severity: Severity,
    payload_fn: F,
) -> anyhow::Result<TransitionOutcome>
where
    F: FnOnce() -> serde_json::Value,
{
    let outcome = fiscal_documents::transition_state(tx, doc_id, from, to).await?;
    if matches!(outcome, TransitionOutcome::Applied) {
        let payload = payload_fn();
        audit_log::append_tx(
            tx,
            "fiscal_document",
            &hex_lower(doc_id.as_bytes()),
            event_type,
            severity,
            None,
            Some(&payload.to_string()),
        )
        .await?;
    }
    Ok(outcome)
}

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
    /// M3b W7b — counter for docs that the post-sign dispatcher
    /// routed to `stage_offline_ack::run` and that resulted in an
    /// `Applied` outcome (offline_local_ack emitted, code consumed).
    /// Increments for both PREPARED→dispatched (via
    /// `dispatch_prepared_via_chain`) and SIGNED→dispatched (boot
    /// recovery of a doc resumed in SIGNED with current mode
    /// Offline/GoingOffline) outcomes.
    pub offline_local_ack_emitted: usize,
    /// M3b W7b — counter for docs that the post-sign dispatcher
    /// REFUSED (mode in {Blocked, StopMode, CryptoDegraded,
    /// GoingOnline}) OR that stage_offline_ack itself refused at
    /// its internal validation layer (Refused outcome).  Either way:
    /// doc state unchanged; audit row emitted.  `dispatch_post_sign`
    /// emits `WRITE_PATH_DISPATCH_REFUSED`; stage_offline_ack emits
    /// `OFFLINE_ACK_REFUSED`.
    pub write_path_dispatch_refused: usize,
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
    /// W14a-2b Commit 5 — counter for operator-actionable
    /// `SignerCashierMismatch::Mismatch` refusals at stage_send 4-pre
    /// (spec §16.8 1-cashier-per-shift sign-time enforcement).  This
    /// bucket counts "operator typed wrong cashier id / signer pipeline
    /// misconfigured" — the operator can fix by reissuing with the
    /// correct cashier.  Distinct from the structural-bug bucket
    /// below so dashboards distinguish actionable from engineering
    /// concern (NIT-C5-5 split, 2026-05-20).
    pub signer_refused_mismatch: usize,
    /// W14a-2b Commit 5 — counter for structural caller-bug
    /// `SignerCashierMismatch` variants (`SignerIdMissing` /
    /// `ShiftMissingForFiscalDoc` / `ShiftIdMismatch` /
    /// `CrossFnMismatch`) at stage_send 4-pre.  These indicate
    /// ingress adapter / dispatcher / caller bugs — operator cannot
    /// remediate via reissue.  Engineering-tier signal; spikes here
    /// require ingress / dispatcher investigation.
    pub signer_refused_structural_bug: usize,
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
            + self.offline_local_ack_emitted
            + self.write_path_dispatch_refused
            + self.error_retryable_escalated_to_manual
            + self.error_retryable_probe_deferred
            + self.error_retryable_indeterminate_deferred
            + self.prepared_replay_drift_deferred
            + self.error_retryable_budget_exhausted
            + self.dispatch_errors
            + self.signer_refused_mismatch
            + self.signer_refused_structural_bug
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
        self.offline_local_ack_emitted += other.offline_local_ack_emitted;
        self.write_path_dispatch_refused += other.write_path_dispatch_refused;
        self.error_retryable_escalated_to_manual += other.error_retryable_escalated_to_manual;
        self.error_retryable_probe_deferred += other.error_retryable_probe_deferred;
        self.error_retryable_indeterminate_deferred += other.error_retryable_indeterminate_deferred;
        self.prepared_replay_drift_deferred += other.prepared_replay_drift_deferred;
        self.error_retryable_budget_exhausted += other.error_retryable_budget_exhausted;
        self.dispatch_errors += other.dispatch_errors;
        self.signer_refused_mismatch += other.signer_refused_mismatch;
        self.signer_refused_structural_bug += other.signer_refused_structural_bug;
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
    /// 0 under the ctx-free boot-gate (`App::boot` / `reconcile_pending`,
    /// `deps == None`), which still maps the FIRST OfflineRefusal to
    /// `BootError::OfflineModeRefusal` and fails-fast.  **RS-1 F7
    /// (2026-05-30):** under the runtime path (`reconcile_pending_with`,
    /// `deps == Some`) a `GoingOnline` FN is no longer fatal — it is
    /// deferred to the W9 drain loop and counted HERE, so this field is
    /// the live count of FNs handed off to the drain loop at boot.
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
                // LOW-5 symmetric field.  The ctx-free boot-gate
                // (`deps == None`) returns `BootError::OfflineModeRefusal`
                // before reaching `record`, so it never lands here.  RS-1
                // F7 (2026-05-30) DOES reach here: the runtime reconcile
                // path defers a `GoingOnline` FN to the W9 drain loop and
                // records it instead of aborting — this counts those
                // hand-offs.
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
            // M3b W1 — service-layer helper: gates CAS through
            // `fiscal_documents::transition_state` (whitelist single
            // source of truth) and writes audit ONLY when Applied.
            let outcome = transition_with_audit(
                tx,
                doc_id,
                DocState::Sending,
                DocState::ErrorRetryable,
                "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE",
                Severity::Error,
                || {
                    serde_json::json!({
                        "document_id": hex_lower(doc_id.as_bytes()),
                        "branch": "c-sending",
                        "rationale":
                            "DPS does not deduplicate; re-sending would be duplicate-document hazard",
                    })
                },
            )
            .await?;
            Ok::<bool, anyhow::Error>(matches!(outcome, TransitionOutcome::Applied))
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
            // (1) CAS Sent → Kvt1 — M3b W1: routed through the
            // repository `transition_state` so the whitelist gate
            // runs.  Supplementary writes (KVT1_RAW + transport_trace
            // completion) and the audit row land BELOW because they
            // need to be ordered after the CAS but before the audit;
            // they cannot fit into the generic `transition_with_audit`
            // closure shape.  All four writes still commit (or roll
            // back) atomically inside this one `with_immediate`.
            let cas =
                fiscal_documents::transition_state(tx, doc_id, DocState::Sent, DocState::Kvt1)
                    .await?;
            if !matches!(cas, TransitionOutcome::Applied) {
                // Conflict / NotFound — bail out early, no partial trail.
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
            // (whitelist edge added in prep PR per W0-3 §6.4-b;
            // M3b W1 routes through repository `transition_state`
            // so the whitelist gate runs).  Supplementary writes
            // (transport_trace completion + audit) land below because
            // they need to be ordered after the CAS — same envelope.
            let cas = fiscal_documents::transition_state(
                tx,
                doc_id,
                DocState::Sent,
                DocState::RequiresManualReconciliation,
            )
            .await?;
            if !matches!(cas, TransitionOutcome::Applied) {
                // Conflict / NotFound — bail early, no partial trail.
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
            // (1) CAS Sent → ErrorRetryable (whitelisted at base;
            // M3b W1 routes through repository `transition_state`).
            // Supplementary writes (transport_trace completion +
            // audit) land below — same envelope.
            let cas = fiscal_documents::transition_state(
                tx,
                doc_id,
                DocState::Sent,
                DocState::ErrorRetryable,
            )
            .await?;
            if !matches!(cas, TransitionOutcome::Applied) {
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
/// write.  Single audit-row INSERT documenting that this Kvt1
/// doc is held under passive observation only.
///
/// **Why no state change.**  Per W0-1 §2.1, `Kvt1 → Kvt2` requires
/// authoritative DPS evidence (the second receipt).  The DPS proto
/// surface as inspected during the W0b gate decision
/// (`docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md`)
/// does not produce per-doc KVT2 evidence for arbitrary stale Kvt1
/// rows: `status_rro` / `infoRro` are RRO-wide aggregates, and
/// `lastChk(fn_sign)` returns evidence ONLY for the latest doc on
/// the FN (insufficient for a stale doc that's no longer the
/// latest).  W0b verdict is scoped YES: M3b's W12 confirmation
/// step (in-drain `lastChk` after `stage_send(doc_i)`) advances
/// the **currently-being-drained** doc through `Kvt1 → Kvt2 →
/// Ack`; this `passive_hold_kvt1` helper continues to handle
/// stale / pre-existing / random-boot-time Kvt1 docs that fall
/// outside the W9 drain window.  Resolving those is M3c/M4 scope
/// or operator-driven manual reconciliation.
///
/// **M3a follow-up — age-based severity escalation.**  Senior-review
/// finding noted that an unbounded passive hold can mask a genuinely
/// stuck doc: if DPS never delivers the second receipt, the doc
/// stays in Kvt1 forever and the only operator-visible signal is a
/// stream of identical `INFO`-severity audit rows on every reboot.
/// We compute the time the doc has been in Kvt1 (from
/// `fiscal_documents.first_kvt1_at`, the dedicated column added in
/// M3b W3 migration 014 — pre-W3 the code read `updated_at` as a
/// proxy via the `fd_updated_at` trigger) and escalate audit
/// severity:
///   - age `< 1h`              → `Severity::Info`     (normal hold window)
///   - age `1h ..< 24h`        → `Severity::Warning`  (operator should investigate)
///   - age `>= 24h`            → `Severity::Error`    (likely stuck; manual reconciliation indicated)
///
/// The age in seconds and the `first_kvt1_at` timestamp are also
/// emitted in the payload so log-aggregation queries don't need
/// to join back to `fiscal_documents`.
pub async fn passive_hold_kvt1(pool: &SqlitePool, doc_id: DocumentId) -> anyhow::Result<()> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // LOW 4 fix: defensive state validation.  The helper
            // emits a forensic audit claiming the doc is in Kvt1;
            // firing on a non-Kvt1 doc would produce a misleading
            // audit trail.  W9.3 dispatch will guard this externally
            // too, but in-helper check hardens against ad-hoc
            // admin / test callers.
            //
            // **M3b W3** — read `first_kvt1_at` (dedicated column
            // added in migration 014) instead of `updated_at`.
            // PR #45's `updated_at` proxy worked under M3a's "no
            // other UPDATE during Kvt1 hold" property, but the
            // dedicated column makes the Kvt1-entry timestamp
            // unambiguous and decouples it from the
            // `fd_updated_at` trigger.  Stamped automatically by
            // `fiscal_documents::transition_state` on every
            // `to == Kvt1` CAS (W3 extension, COALESCE semantics
            // preserve the timestamp on idempotent re-entry).
            //
            // The column is nullable for non-Kvt1 rows; on Kvt1 rows
            // post-migration 014 it is ALWAYS populated (either by
            // the W3 transition_state stamp or by the migration
            // backfill from `updated_at`).  We read as
            // `Option<String>` and treat `None` as a database-shape
            // regression: degrade to age=0 + Info severity rather
            // than crashing boot reconcile (consistent with PR #45
            // unparseable-timestamp fallback policy).
            let (state, first_kvt1_at_text): (String, Option<String>) = sqlx::query_as(
                "SELECT state, first_kvt1_at FROM fiscal_documents WHERE document_id = ?",
            )
            .bind(doc_id)
            .fetch_one(&mut **tx)
            .await?;
            anyhow::ensure!(
                state == "KVT1",
                "passive_hold_kvt1: doc not in Kvt1 (got {state})"
            );

            // SQLite `CURRENT_TIMESTAMP` format: `YYYY-MM-DD HH:MM:SS`
            // (UTC, no T separator, no Z suffix).  If the column is
            // NULL (database-shape regression — should be impossible
            // post-W3 migration on a doc in Kvt1) or parsing fails,
            // we fall back to age=0 + Info severity rather than
            // crashing the boot reconcile; the audit row still emits
            // so the operator gets the signal.
            let timestamp_for_audit = first_kvt1_at_text.clone().unwrap_or_default();
            let (kvt1_age_seconds, severity) = age_and_severity_for_kvt1(
                first_kvt1_at_text.as_deref().unwrap_or(""),
                chrono::Utc::now(),
            );

            let payload = serde_json::json!({
                "document_id": hex_lower(doc_id.as_bytes()),
                "branch": "c-kvt1",
                // M3b W0b verdict: KVT2 evidence is achievable only
                // for the doc CURRENTLY being drained (W12 in-drain
                // lastChk after stage_send(doc_i)).  This helper
                // covers stale / pre-existing / random-boot-time
                // Kvt1 docs that are NOT in the active drain window
                // — their KVT2 resolution is M3c/M4 scope or
                // operator manual reconciliation.
                "deferred_to": "M3c/M4 or operator manual reconciliation (out of W9 drain window; W12 covers only the current drain doc)",
                "first_kvt1_at": timestamp_for_audit,
                "kvt1_age_seconds": kvt1_age_seconds,
                "severity_threshold": severity.as_str(),
            });
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &hex_lower(doc_id.as_bytes()),
                "BOOT_KVT1_HOLD_DEFERRED",
                severity,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
}

/// Bucket the Kvt1 hold age into a severity level.  Pulled out as a
/// free function so the boundary logic is unit-testable without an
/// in-memory pool.  Returns `(age_seconds, severity)`; on parse
/// failure of `first_kvt1_at_text` returns `(0, Severity::Info)` —
/// preferring to emit the audit row with a degraded signal over
/// crashing boot reconcile on a malformed timestamp.
///
/// Thresholds:
///   - `age < 3600s`     → `Severity::Info`
///   - `3600 ≤ age < 86400` → `Severity::Warning`
///   - `age ≥ 86400`     → `Severity::Error`
///
/// Negative ages (clock skew, e.g. the row's `first_kvt1_at` is in
/// the future relative to `now`) are clamped to 0.
///
/// **W3 (M3b)**: parameter renamed from `updated_at_text` to
/// `first_kvt1_at_text` to reflect the W3 column switch (was a
/// `fiscal_documents.updated_at` proxy pre-W3; now reads the
/// dedicated `first_kvt1_at` column populated by migration 014 +
/// `fiscal_documents::transition_state` Kvt1 arm stamp).
fn age_and_severity_for_kvt1(
    first_kvt1_at_text: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> (i64, crate::db::models::enums::Severity) {
    use crate::db::models::enums::Severity;
    // SQLite default datetime format used by `CURRENT_TIMESTAMP`.
    let parsed = chrono::NaiveDateTime::parse_from_str(first_kvt1_at_text, "%Y-%m-%d %H:%M:%S")
        .map(|naive| naive.and_utc())
        // Also accept RFC3339 / ISO-8601 (Z-suffixed or `+00:00`),
        // so test fixtures or future migrations that store the
        // timestamp in that form keep working without
        // round-tripping through the SQLite default format.
        .or_else(|_| {
            chrono::DateTime::parse_from_rfc3339(first_kvt1_at_text)
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });
    let Ok(kvt1_at) = parsed else {
        return (0, Severity::Info);
    };
    let age_seconds = (now - kvt1_at).num_seconds().max(0);
    let severity = if age_seconds >= 86_400 {
        Severity::Error
    } else if age_seconds >= 3_600 {
        Severity::Warning
    } else {
        Severity::Info
    };
    (age_seconds, severity)
}

/// W9.3 — per-FN decision-tree dispatch.  Closed-enum outcome lets
/// the caller (`App::reconcile_pending`) accumulate per-FN
/// histograms without re-reading the DB.
///
/// Size delta tolerated (W14a-2b NIT-C5-5 added 2 more `usize`
/// counters to `DispatchHistogram`): `Reconciled` carries the
/// histogram inline, dwarfing the small unit-like variants
/// (`Bootstrapped` / `OfflineRefusal`).  Boxing the histogram would
/// trade allocation cost on every call for memory savings; current
/// call-path is hot enough that staying inline is the right tradeoff.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
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
    /// (d) M3b W7b update (2026-05-16): mode == `GoingOnline` at boot.
    /// The CALLER decides fatality (**RS-1 F7, 2026-05-30**): the ctx-free
    /// boot-gate (`reconcile_pending`, `deps == None`) fails-closed and
    /// surfaces `BootError::OfflineModeRefusal`; the runtime path
    /// (`reconcile_pending_with`, `deps == Some`) DEFERS this FN to the W9
    /// drain loop — records it in `branch_d_offline_refusal` and continues
    /// the other FNs.  Pre-W7b this also covered `Offline` / `GoingOffline`,
    /// but those modes now fall through to per-doc dispatch via W7b's
    /// `dispatch_post_sign` (see `dispatch_pending_doc::DocState::Signed`
    /// and `dispatch_prepared_via_chain`).  `GoingOnline` stays this
    /// branch's outcome because that's W9 backlog-drain territory.
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
/// **Branch (d) — GoingOnline refusal** (M3b W7b update,
/// 2026-05-16).  Returns `BranchOutcome::OfflineRefusal {
/// observed_mode }` rather than erroring directly; the caller
/// decides fatality.  The ctx-free boot-gate (`App::reconcile_pending`,
/// `deps == None`) maps this to `BootError::OfflineModeRefusal` and
/// fails-fast on the first GoingOnline FN.  **RS-1 F7 (2026-05-30):**
/// the runtime path (`reconcile_pending_with`, `deps == Some`) instead
/// DEFERS the GoingOnline FN to the W9 drain loop (records it, continues
/// the other FNs) so one transitioning FN cannot deny the runtime to all
/// the rest.  Pre-W7b this also covered Offline
/// / GoingOffline (per freeze §13.3); post-W7b those modes fall
/// through to the per-doc dispatch loop where the W7b post-sign
/// dispatcher (`services::write_path::dispatch::dispatch_post_sign`)
/// routes SIGNED / PREPARED docs to `stage_offline_ack::run` for
/// local fiscalisation.  Only `GoingOnline` (W9 backlog-drain
/// territory) still aborts boot at this branch — boot reconciliation
/// defers to W9's drain loop for those FNs.
///
/// **Branch (c)/(e1) — per-doc dispatch scope.**  W9.3 ships the
/// dispatch shell that loops `list_pending_for_fn` in
/// `(lnd, created_at, document_id)` order.  Per-DocState routing:
///
///   | DocState        | action (ctx-free / ctx-wired, post-W7b)|
///   | --------------- | -------------------------------------- |
///   | `Sending`       | `resume_sending_to_error_retryable`    |
///   | `Kvt1`          | `passive_hold_kvt1`                    |
///   | `Encrypted`     | transition → ErrorRetryable + audit    |
///   | `Kvt2`          | `stage_finalize::run` (W8; no ctx)     |
///   | `Prepared`      | deps=None: DEFERRED audit; deps=Some:  |
///   |                 | `dispatch_prepared_via_chain` →        |
///   |                 | stage_sign → **W7b post-sign           |
///   |                 | dispatcher** → stage_send OR           |
///   |                 | stage_offline_ack (per node mode)      |
///   | `Signed`        | deps=None: DEFERRED audit; deps=Some:  |
///   |                 | **W7b post-sign dispatcher** routes by |
///   |                 | node mode (Online → stage_send;        |
///   |                 | Offline/GoingOffline → stage_offline_ack;|
///   |                 | non-routable modes → typed refusal)    |
///   | `Sent`          | DEFERRED audit / `dispatch_sent_via_probe` |
///   | `ErrorRetryable`| DEFERRED audit / `dispatch_error_retryable_by_class` |
///
/// Ctx-needy DocStates (Prepared/Signed/Sent/ErrorRetryable) emit
/// `BOOT_DISPATCH_DEFERRED` WARN per occurrence so operators see the
/// docs that the boot tick observed but couldn't drive forward.
/// W11+ runtime composition will wire the missing dispatches; until
/// then those docs stay in their source state.
///
/// **Module-level enforcement of ADR-M3-A10 (since M3b W2).**  The
/// first parameter is a `&ReconcileGuard<'_>` lock-token proving the
/// caller holds (or is exempt from, via the test seam) the App
/// reconcile mutex.  Production callers obtain the token from
/// `App::reconcile_pending_inner` after `reconcile_mutex.lock().await`;
/// the token's lifetime ties to the `MutexGuard`, so mutex release
/// follows token drop.  Integration tests use
/// `ReconcileGuard::for_integration_test_only()` (explicit test seam;
/// see `guard.rs` for the contract).  Token does not change
/// behaviour — its sole purpose is to enforce at compile time that
/// no caller can invoke `run_boot_reconciliation` while bypassing the
/// App mutex.  Closes the bypass left over from M3a HP2.
/// **M3b W12 Post-Closure Hardening Phase 3 / REC-3 (2026-05-24)** —
/// scan `transport_trace` for orphan rows (allocated by Envelope 1c-pre
/// but never completed) that pre-date `ttl_secs` before now.  Close
/// each з `outcome_kind=SystemCrash` + emit `TRANSPORT_TRACE_ORPHAN_
/// CLOSED` Info audit.
///
/// **Scenario**: SentReplay path в `services/offline_sync/kvt2_confirm.
/// rs::commit_sent_replay_envelope_1c_pre` allocates `transport_trace`
/// row BEFORE DPS lastChk wire call.  If process crashes mid-call
/// (SIGKILL / OOM / power loss), row stays in `Allocated` state
/// (`outcome_kind = NULL`).  Next-tick SentReplay re-allocates a fresh
/// row; orphan persists forever без this scanner.
///
/// **TTL buffer (default 60s)**: in-flight DPS calls на graceful-
/// shutdown boundary may have started seconds before App restart;
/// `ttl_secs` guards against false-positive close of legitimately
/// in-flight rows that started right before App init started its own
/// timer.  60s default = upper bound на typical DPS call duration +
/// generous buffer.
///
/// **Atomicity (I4)**: per-row `with_immediate` envelope — each orphan
/// closed з 1 UPDATE + 1 audit row.  Crash mid-scan leaves
/// partially-closed traces; next boot scanner pass re-processes the
/// remainder.  Idempotent (already-closed rows excluded by `WHERE
/// outcome_kind IS NULL` predicate).
///
/// **Returns**: number of orphan rows closed (for boot summary +
/// operator analytics).
pub async fn close_orphan_transport_traces(
    pool: &SqlitePool,
    ttl_secs: i64,
) -> anyhow::Result<usize> {
    use crate::db::repositories::transport_trace;
    use crate::db::tx::with_immediate;

    // Snapshot orphan candidate rows pre-scan.  Per-row close envelopes
    // avoid single-large-tx amplification (rare-event scanner) + give
    // operator per-row audit visibility.
    let orphans: Vec<(Vec<u8>, i32, String)> = sqlx::query_as(
        "SELECT document_id, attempt_no, started_at \
         FROM transport_trace \
         WHERE outcome_kind IS NULL \
           AND started_at < datetime('now', ?) \
         ORDER BY started_at",
    )
    .bind(format!("-{ttl_secs} seconds"))
    .fetch_all(pool)
    .await?;

    let mut closed_count = 0usize;
    let now_iso = iso8601_now();
    for (doc_id_bytes, attempt_no, started_at) in &orphans {
        // Doc ID from BLOB → DocumentId.
        let doc_id_arr: [u8; 16] = doc_id_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!(
                "boot orphan scanner: transport_trace.document_id length != 16 для started_at={started_at}"
            ))?;
        let doc_id = crate::db::models::ids::DocumentId::from_bytes(doc_id_arr);
        let id_hex = hex_lower(doc_id_bytes);
        let payload = serde_json::json!({
            "document_id": id_hex,
            "attempt_no": attempt_no,
            "started_at": started_at,
            "closed_at": now_iso,
            "outcome_kind": transport_trace::OutcomeKind::SystemCrash.as_str(),
            "reason": "Process exited mid-wire-call; no outcome captured (boot scanner)",
            "ttl_secs": ttl_secs,
        });
        let payload_owned = payload.to_string();
        let id_hex_owned = id_hex.clone();
        let started_at_owned = started_at.clone();
        let now_iso_owned = now_iso.clone();
        let attempt_no_owned = *attempt_no;
        with_immediate(pool, move |tx| {
            Box::pin(async move {
                // (a) Close orphan з SystemCrash outcome.  Use the
                // original started_at as wire_call_started_at (best-
                // effort recovery — actual call may have started slightly
                // later, but DDL CHECK requires all 4 completion fields
                // NOT NULL).
                let rows = sqlx::query(
                    "UPDATE transport_trace SET \
                        completed_at = ?, \
                        wire_call_started_at = ?, \
                        wire_call_finished_at = ?, \
                        outcome_kind = 'SYSTEM_CRASH', \
                        error_kind = 'ORPHANED_AT_BOOT_RECOVERY', \
                        error_message = 'Process exited mid-wire-call; no outcome captured' \
                     WHERE document_id = ? AND attempt_no = ? AND outcome_kind IS NULL",
                )
                .bind(&now_iso_owned)
                .bind(&started_at_owned)
                .bind(&now_iso_owned)
                .bind(doc_id)
                .bind(attempt_no_owned)
                .execute(&mut **tx)
                .await?
                .rows_affected();
                // rows_affected==0 means another writer closed it
                // between SELECT and UPDATE — race-safe skip (idempotent).
                if rows == 0 {
                    return Ok::<bool, anyhow::Error>(false);
                }
                // (b) Audit row.
                audit_log::append_tx(
                    tx,
                    "transport_trace",
                    &id_hex_owned,
                    "TRANSPORT_TRACE_ORPHAN_CLOSED",
                    crate::db::models::enums::Severity::Info,
                    None,
                    Some(&payload_owned),
                )
                .await?;
                Ok(true)
            })
        })
        .await?;
        closed_count += 1;
    }
    Ok(closed_count)
}

pub async fn run_boot_reconciliation(
    _guard: &super::ReconcileGuard<'_>,
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

    // ── Branch (d) — GoingOnline refused at boot ─────────────────
    //
    // M3b W7b update (operator PR #57 Round 1 HIGH, 2026-05-16):
    // pre-W7 this branch refused ALL three offline-class modes
    // (Offline / GoingOffline / GoingOnline) because boot
    // reconciliation had no W7-aware path.  Post-W7, boot CAN
    // process docs in Offline / GoingOffline modes — per-doc
    // dispatch routes them through `dispatch_post_sign` →
    // `stage_offline_ack::run` (see `dispatch_pending_doc` SIGNED
    // arm and `dispatch_prepared_via_chain` PREPARED arm).
    //
    // `GoingOnline` stays refused at THIS branch — it's W9
    // backlog-drain mode and boot reconciliation defers to W9's
    // drain loop (out of W7b scope per memory
    // `m3b-w7-review-criteria` criterion 6 + `m3b-w7b-review-criteria`
    // criterion 4).  Per-FN single refusal audit here (vs
    // per-doc spam if we let dispatch_post_sign handle it for
    // every pending doc).
    if matches!(row.mode, NodeMode::GoingOnline) {
        let payload = serde_json::json!({
            "fiscal_number": fiscal_number,
            "observed_mode": row.mode.as_str(),
            "message": "FN in GOING_ONLINE mode — W9 backlog drain owns this FN's reconciliation",
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

    // From here: row.mode is one of {Online, Offline, GoingOffline}.
    // (M3b W7b: branches d + f already returned for refused modes;
    // Offline / GoingOffline fall through to per-doc dispatch via
    // W7b's `dispatch_post_sign`.)
    // Decision: pending docs?  shift_state ∈ {Opening, Closing}?
    let pending = fiscal_documents::list_pending_for_fn(pool, fiscal_number).await?;

    // ── Branch (e2) — mid-transition shift orphan (no pending) ───
    //
    // M3b W7b operator PR #57 Round 2 HIGH fix (2026-05-16):
    // gated on `row.mode == NodeMode::Online`.  Branch (e2)
    // performs invasive shift recovery (orphan shifts → ERROR
    // + node_state.shift_state → CLOSED) which is an
    // online-mode invariant.  For Offline / GoingOffline modes
    // shift lifecycle is managed by the W4-W6 offline session
    // machinery, NOT by boot reconciliation; (e2) must NOT
    // touch shift state for those modes.  Offline / GoingOffline
    // + no pending falls through to branch (b) below
    // (idempotent no-op).
    if row.mode == NodeMode::Online
        && matches!(row.shift_state, ShiftState::Opening | ShiftState::Closing)
        && pending.is_empty()
    {
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
                    // Branch e2 system-context recovery — raw UPDATE to
                    // 'ERROR' bypasses `shifts::transition_state` (which
                    // would `Forbidden` per §4.1 whitelist) AND bypasses
                    // `force_to_error_with_audit` (which requires
                    // operator evidence_json — system-context has no
                    // operator identity at boot).
                    //
                    // PR #66 R6 HIGH clarification: codified in spec §4.4
                    // as ONE OF TWO entry surfaces for `Error` (the other
                    // being operator-driven force seam).  Audit shape
                    // `SHIFT_BOOT_ORPHAN_ERROR` (spec §8) distinct from
                    // `SHIFT_FORCE_TO_ERROR` so forensic queries separate
                    // boot recovery from operator escalation.  I8
                    // preserved (no silent transition; audit emitted with
                    // observed pre-state + branch context).
                    //
                    // RS-3 C1: the raw shift→ERROR write lives in the
                    // transition-service (the sole home for shift+projection
                    // writes); the projection reset follows once after the
                    // loop via `clear_active_shift_projection`.
                    shift_transition::force_orphan_shift_to_error(tx, shift_id).await?;
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
                // Reset node_state.shift_state to Closed (HIGH 10 fix) +
                // clear current_shift_id (RS-3 C2) so a resolved orphan
                // leaves no dangling pointer; per Q1-A′ node_state is a
                // read-projection.  Runs once per FN — including when zero
                // orphan rows were found — to repair a dangling
                // OPENING/CLOSING projection with no backing shift.
                // RS-3 C1: routed through the transition-service so no raw
                // node_state shift write survives in boot_phase.
                shift_transition::clear_active_shift_projection(tx, &fn_owned).await?;
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
            // M3b W7b — post-sign dispatcher outcomes (operator PR
            // #57 Round 1 MED-3 fix, 2026-05-16): bucket sums in
            // `by_outcome` must reconcile to `pending_visited`.
            "offline_local_ack_emitted": histogram.offline_local_ack_emitted,
            "write_path_dispatch_refused": histogram.write_path_dispatch_refused,
            "error_retryable_escalated_to_manual": histogram.error_retryable_escalated_to_manual,
            "error_retryable_probe_deferred": histogram.error_retryable_probe_deferred,
            "error_retryable_indeterminate_deferred": histogram.error_retryable_indeterminate_deferred,
            "prepared_replay_drift_deferred": histogram.prepared_replay_drift_deferred,
            "error_retryable_budget_exhausted": histogram.error_retryable_budget_exhausted,
            "dispatch_errors": histogram.dispatch_errors,
            // W14a-2b Commit 5 NIT-C5-2 + NIT-C5-5: signer-refused
            // split into operator-actionable Mismatch vs structural
            // caller-bug bucket so dashboards can route alerts to the
            // right responder (operator vs engineering).
            "signer_refused_mismatch": histogram.signer_refused_mismatch,
            "signer_refused_structural_bug": histogram.signer_refused_structural_bug,
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
            // M3b W1 — service-layer helper: whitelist gate + audit
            // only on Applied.
            let outcome = transition_with_audit(
                tx,
                doc_id,
                DocState::ErrorRetryable,
                DocState::RequiresManualReconciliation,
                "BOOT_ER_ESCALATED_TO_MANUAL",
                severity,
                || {
                    serde_json::json!({
                        "document_id": hex_lower(doc_id.as_bytes()),
                        "branch": "c-error-retryable-escalated",
                        "retry_class": retry_class_owned,
                        "rationale":
                            "ErrorRetryable + non-retryable durable retry_class — operator triage required; no auto-retry per stage_send §4.2",
                    })
                },
            )
            .await?;
            Ok::<bool, anyhow::Error>(matches!(outcome, TransitionOutcome::Applied))
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
            // M3b W1 — service-layer helper composition.
            let outcome = transition_with_audit(
                tx,
                doc_id,
                DocState::ErrorRetryable,
                DocState::RequiresManualReconciliation,
                "BOOT_ER_BUDGET_EXHAUSTED",
                Severity::Error,
                || {
                    serde_json::json!({
                        "document_id": hex_lower(doc_id.as_bytes()),
                        "branch": "c-error-retryable-budget-exhausted",
                        "retry_class": retry_class_owned,
                        "attempts_used": attempts_used,
                        "max_boot_attempts": MAX_BOOT_ATTEMPTS,
                        "rationale":
                            "TransientRetry boot-attempt budget exhausted (attempts_used >= MAX_BOOT_ATTEMPTS); doc would re-burn DPS quota every boot tick without escalation; operator triage required per W9 freeze §4.0",
                    })
                },
            )
            .await?;
            Ok::<bool, anyhow::Error>(matches!(outcome, TransitionOutcome::Applied))
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
    use crate::services::reconciliation::er_redrive_policy::{
        evaluate_er_redrive, ErRedriveDecision,
    };
    use crate::services::write_path::error_routing::RetryClass;

    let doc_id = doc.document_id;

    // M3b W9b ER-class-guard: the redrive-vs-escalate decision is
    // shared with the W9b backlog drain.  Boot owns the BOOT_ER_* audit
    // taxonomy + DispatchHistogram counter projection below; drain owns
    // its OFFLINE_DRAIN_* projection separately
    // (`backlog_drain::process_via_er_class_guard`).
    let decision = evaluate_er_redrive(pool, doc_id).await?;

    match decision {
        ErRedriveDecision::Redrive => {
            // TransientRetry + attempts < MAX_BOOT_ATTEMPTS — Pattern B
            // retry path (existing W11 PR-2a wiring).
            match stage_send::run(pool, deps.dps, doc_id, Some(deps.signing_ctx)).await {
                // W14a-2b Commit 5 NIT-C5-2 + NIT-C5-5: split
                // SignerRefused off the generic dispatched bucket
                // AND split Mismatch (operator-actionable) from the
                // structural caller-bug variants (engineering signal).
                Ok(stage_send::StageSendOutcome::SignerRefused(Scm::Mismatch { .. })) => {
                    histogram.signer_refused_mismatch += 1
                }
                Ok(stage_send::StageSendOutcome::SignerRefused(_)) => {
                    histogram.signer_refused_structural_bug += 1
                }
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
        ErRedriveDecision::BudgetExhausted { attempts_used } => {
            // M3a hardening pass 1 — H2 closure: enforce the
            // boot-attempt budget cap.  `MAX_BOOT_ATTEMPTS = 5` is
            // declared in W9 freeze §4.0; without this gate an
            // infinitely-failing TransientRetry doc would re-burn DPS
            // quota on every boot tick.  `attempts_used` counts ALL
            // transport_trace rows for the doc (both completed and
            // in-flight), so the cap covers crash-mid-send attempts.
            match cas_error_retryable_budget_exhausted(
                pool,
                doc_id,
                attempts_used,
                RetryClass::TransientRetry.as_str(),
            )
            .await
            {
                Ok(_) => histogram.error_retryable_budget_exhausted += 1,
                Err(e) => {
                    emit_dispatch_error(pool, doc_id, "c-error-retryable-budget", &e, histogram)
                        .await?
                }
            }
        }
        ErRedriveDecision::EscalateManual { class: rc } => {
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
                    emit_dispatch_error(pool, doc_id, "c-error-retryable-escalate", &e, histogram)
                        .await?
                }
            }
        }
        ErRedriveDecision::EscalateInconsistent { class: rc } => {
            // Structurally inconsistent: TerminalReject targets `Rejected`
            // directly per `error_routing::route_dps_error`; an ER doc
            // tagged TerminalReject is durable evidence of a routing /
            // CAS skew.  Escalate with CRITICAL severity.
            match cas_error_retryable_to_manual_reconciliation(
                pool,
                doc_id,
                rc.as_str(),
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
        ErRedriveDecision::HoldProbeRequired => {
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
                    emit_dispatch_error(pool, doc_id, "c-error-retryable-probe", &e, histogram)
                        .await?
                }
            }
        }
        ErRedriveDecision::HoldIndeterminate => {
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
    use crate::db::repositories::{
        ingress_inbox::InboxRow, node_state, shifts, signing_config_snapshots,
    };
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
                        payload_sha256_canonical, source_sha256 \
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
            // RS-3 A1Z (migration 024): the SOURCE/wire hash, NULL on pre-024
            // legacy docs. `Some` → drift-check against the inbox wire hash;
            // `None` → fall back to the legacy payload-hash compare below.
            let fd_source_sha: Option<[u8; 32]> = {
                let v: Option<Vec<u8>> = fd_row.try_get("source_sha256")?;
                match v {
                    Some(b) => Some(b.as_slice().try_into().map_err(|_| {
                        anyhow::anyhow!(
                            "fiscal_documents.source_sha256 length != 32 for doc {doc_id:?}"
                        )
                    })?),
                    None => None,
                }
            };

            // (1b) Read the matching inbox row.  No `ingress_inbox`
            // pool/tx reader exists today; PR-2b is the sole caller,
            // so the read lives inline rather than adding a one-off
            // repo helper.
            let req_slice: &[u8] = &request_id;
            let inbox_row_db = sqlx::query(
                "SELECT request_id, fiscal_number, protocol, operation_type, \
                        idempotency_key, status, payload_json, \
                        payload_sha256_canonical, correlation_id, received_at, \
                        signed_by_cashier_id, driver_id, business_ts, total_sum_kop \
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
                signed_by_cashier_id: inbox_row_db.try_get("signed_by_cashier_id")?,
                driver_id: inbox_row_db.try_get("driver_id")?,
                business_ts: inbox_row_db.try_get("business_ts")?,
                total_sum_kop: inbox_row_db.try_get("total_sum_kop")?,
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
            // RS-3 A1Z (D5): the source-aware RELAXATION (the doc payload may
            // differ from the inbox WIRE payload) is correct ONLY for Z-CLASS
            // docs, whose canonical body is an AGGREGATE of the shift ledger
            // and legitimately differs from the wire intent. For ANY non-Z doc
            // the canonical payload MUST equal the inbox payload (no
            // aggregation), so the full inbox-equality compare is retained —
            // else a consistently-corrupted non-Z payload (payload_json AND its
            // hash both rewritten) would pass and be signed (review HIGH).
            let is_z_class = matches!(doc_type_copy.as_str(), "Z_REPORT" | "SHIFT_CLOSE");
            let (drift, payload_json_mismatch) = match fd_source_sha {
                // Z-class (024+): source must match the inbox wire hash AND BOTH
                // payloads' self-integrity must hold — the doc's (canonical hash
                // == sha256(its payload_json), catches a tampered aggregated
                // body) AND the inbox WIRE payload's (sha256(inbox.payload_json)
                // == inbox hash; the inbox bytes are the crash-recovery anchor
                // `source` is tied to, and the dropped payload_json-vs-inbox
                // compare no longer guards them — wide-audit MEDIUM). The
                // payload_json-vs-inbox EQUALITY is dropped (Z divergence is
                // legitimate); the two self-integrity checks replace it.
                Some(src) if is_z_class => {
                    // Integrity is meaningful because each hash was computed over
                    // the EXACT bytes now persisted (build_z_canonical / ingest
                    // store the literal hashed bytes, not a re-serialized struct).
                    let recomputed: [u8; 32] = Sha256::digest(payload_json.as_bytes()).into();
                    let inbox_recomputed: [u8; 32] =
                        Sha256::digest(inbox_row.payload_json.as_bytes()).into();
                    let d = fd_fiscal_number != inbox_row.fiscal_number
                        || src != inbox_row.payload_sha256_canonical
                        || doc_type_copy.as_str() != inbox_row.operation_type
                        || recomputed != payload_sha
                        || inbox_recomputed != inbox_row.payload_sha256_canonical;
                    (d, false)
                }
                // Non-Z (any) OR legacy/pre-024: the ORIGINAL compare — the
                // doc's canonical hash + payload_json MUST equal the inbox. For
                // a post-024 non-Z doc source == canonical == inbox, so the
                // inbox-hash equality already catches drift; no separate source
                // compare needed.
                _ => {
                    let pjm = payload_json != inbox_row.payload_json;
                    let d = fd_fiscal_number != inbox_row.fiscal_number
                        || payload_sha != inbox_row.payload_sha256_canonical
                        || doc_type_copy.as_str() != inbox_row.operation_type
                        || pjm;
                    (d, pjm)
                }
            };
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
            // an open shift.  Recovery deliberately uses a NARROWER
            // filter than stage_acquire step 5 (which post-W14a-2b
            // §3.6a widens to `Opened | OpenedLocalPendingDrain`):
            // boot recovery just passes None through and lets stage_sign
            // drive on the persisted doc — it does NOT escalate
            // ShiftInvariantViolation (the live worker does that on
            // first ingress).  Pre-W14a-2b mirror parity dropped on
            // purpose; offline-context boot uses the same `None`
            // fallback for both Opened and OpenedLocalPendingDrain.
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
                // RS-3 A1Z (D5): reconstruct the SOURCE hash from the doc's
                // persisted source_sha256 (the wire hash — differs from the
                // canonical hash for a Z doc). Pre-024 legacy docs have none;
                // there source == canonical (they are non-aggregated).
                source_sha256: fd_source_sha.unwrap_or(payload_sha),
                // W14a-2b Commit 2 (updated post-021): signer attribution
                // IS now persisted on the ingress_inbox row (migration 021),
                // but this is the PREPARED-doc REPLAY path — a
                // `fiscal_documents` row already exists and is the
                // AUTHORITATIVE signer surface: it carries the persisted
                // `signed_by_cashier_id` and `signer_guard` reads it from
                // `SendInputs` (Commits 3 + 5).  This boot snapshot command
                // is for resume-detect / canonical-hash verification only, so
                // it intentionally stays `None` here.  (The RS-3
                // stale-PROCESSING reaper — a row WITHOUT a doc yet — is the
                // path that rebuilds identity FROM the inbox; see seam.rs.)
                signed_by_cashier_id: None,
                // W4-Z0 piece 9: `driver_id` is likewise persisted on the
                // inbox now (021) but unused on this PREPARED-replay path
                // (system context; the doc is authoritative).
                driver_id: None,
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

    // W4-Z2a piece 13 (self-review IMP-1 close) — reload persisted
    // snapshot via doc FK BEFORE re-entry.  Locked rule #9: boot
    // recovery uses the snapshot the doc was originally pinned with,
    // NEVER current config.  Required to drive 3-NO-TX `<TX>` emit
    // for taxable items; without it, derive_check_tax_summaries fails
    // with TaxMappingNotWired and the doc loops PREPARED forever.
    //
    // None branch covers pre-W4-Z2a docs (FK NULL — migration window
    // back-compat); these docs by construction don't carry tax_group_1
    // items, so empty map = unchanged behaviour.
    let (tax_resolution_snapshot, tax_resolution_snapshot_id) = match doc.signing_config_snapshot_id
    {
        Some(id) => match signing_config_snapshots::get_by_id(pool, id).await {
            Ok(s) => (Some(s), Some(id)),
            Err(e) => {
                // Snapshot ledger drift / corruption / orphan FK —
                // catastrophic.  Surface via emit_dispatch_error
                // (same forensic class as the pre-existing recovery
                // errors above) and bail; doc stays in PREPARED for
                // operator inspection.  Audit log carries the typed
                // SigningConfigSnapshotsRepoError chain.
                emit_dispatch_error(
                    pool,
                    doc_id,
                    "c-prepared-snapshot-reload",
                    &anyhow::Error::new(e),
                    histogram,
                )
                .await?;
                return Ok(());
            }
        },
        None => (None, None),
    };

    let worker_ctx = WorkerContext {
        inbox,
        command,
        node_state,
        active_shift,
        document: doc.clone(),
        tax_resolution_snapshot,
        tax_resolution_snapshot_id,
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

    // (3) M3b W7b post-sign dispatcher: route based on current
    // node mode.  Online → stage_send::run (unchanged M3a happy
    // path).  Offline / GoingOffline → stage_offline_ack::run;
    // pipeline TERMINATES here for this doc — no stage_send call.
    // 4 non-routable modes (Blocked / StopMode / CryptoDegraded /
    // GoingOnline) → typed dispatcher refusal + audit; doc stays
    // in SIGNED.  See `services::write_path::dispatch`.
    use crate::services::write_path::dispatch::{self, PostSignRoute};
    use crate::services::write_path::stage_offline_ack::OfflineAckOutcome;
    match dispatch::dispatch_post_sign(pool, doc_id, &doc.fiscal_number).await {
        Ok(PostSignRoute::Online { .. }) => {
            // Existing M3a online ladder.  stage_send::run drives
            // SIGNED → SENT (or routed target) via Pattern B.
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
        }
        Ok(PostSignRoute::Offline { outcome, .. }) => {
            // Pipeline terminates here — no stage_send / stage_finalize.
            match outcome {
                OfflineAckOutcome::Applied { .. } => {
                    histogram.offline_local_ack_emitted += 1;
                }
                OfflineAckOutcome::Refused(_) => {
                    // stage_offline_ack already emitted
                    // OFFLINE_ACK_REFUSED audit; histogram bumps
                    // the same bucket as dispatcher-level refusal
                    // (both represent "doc not advanced").
                    histogram.write_path_dispatch_refused += 1;
                }
            }
        }
        Ok(PostSignRoute::Refused(_)) => {
            // Dispatcher-level refusal (Blocked / StopMode /
            // CryptoDegraded / GoingOnline).  WRITE_PATH_DISPATCH_REFUSED
            // audit already emitted by dispatch_post_sign.
            histogram.write_path_dispatch_refused += 1;
        }
        Err(e) => {
            emit_dispatch_error(pool, doc_id, "c-prepared-dispatch", &e, histogram).await?;
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
            // M3b W1 — service-layer helper: whitelist gate via
            // repository `transition_state` + audit only on Applied.
            // Pre-W1 the audit fired unconditionally even on CAS
            // no-op (e.g. parallel writer raced); W1 tightens this
            // per operator review criterion ("RowsAffected = 0 path
            // должен не создавать audit row").  Under ADR-M3-A10
            // single-writer-per-FN the race cannot occur in
            // practice, so this is a strict invariant tightening, not
            // a behavioural change in production.
            let result = with_immediate(pool, move |tx| {
                Box::pin(async move {
                    transition_with_audit(
                        tx,
                        doc_id,
                        DocState::Encrypted,
                        DocState::ErrorRetryable,
                        "BOOT_ENCRYPTED_REROUTED",
                        Severity::Warning,
                        || {
                            serde_json::json!({
                                "document_id": hex_lower(doc_id.as_bytes()),
                                "branch": "c-encrypted",
                                "rationale":
                                    "M3a is Pattern B + ONLINE; ENCRYPTED is Checkbox-only contour",
                            })
                        },
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
            // M3b W7b: a doc resumed in SIGNED at boot may have
            // been crashed-recovered with a node mode that has
            // since flipped to Offline / GoingOffline.  Route
            // through the post-sign dispatcher so the offline
            // branch is reachable here too (criterion 1: wiring
            // happens after any path that produces a SIGNED doc).
            Some(d) => {
                use crate::services::write_path::dispatch::{self, PostSignRoute};
                use crate::services::write_path::stage_offline_ack::OfflineAckOutcome;
                match dispatch::dispatch_post_sign(pool, doc_id, &doc.fiscal_number).await {
                    Ok(PostSignRoute::Online { .. }) => {
                        match stage_send::run(pool, d.dps, doc_id, Some(d.signing_ctx)).await {
                            // W14a-2b Commit 5 NIT-C5-2 + NIT-C5-5:
                            // split SignerRefused off the generic
                            // signed_dispatched counter; Mismatch =
                            // operator-actionable bucket; structural
                            // bug bucket otherwise.
                            Ok(stage_send::StageSendOutcome::SignerRefused(Scm::Mismatch {
                                ..
                            })) => histogram.signer_refused_mismatch += 1,
                            Ok(stage_send::StageSendOutcome::SignerRefused(_)) => {
                                histogram.signer_refused_structural_bug += 1
                            }
                            Ok(_) => histogram.signed_dispatched += 1,
                            Err(e) => {
                                emit_dispatch_error(
                                    pool,
                                    doc_id,
                                    "c-signed",
                                    &anyhow::Error::new(e),
                                    histogram,
                                )
                                .await?
                            }
                        }
                    }
                    Ok(PostSignRoute::Offline { outcome, .. }) => match outcome {
                        OfflineAckOutcome::Applied { .. } => {
                            histogram.offline_local_ack_emitted += 1;
                        }
                        OfflineAckOutcome::Refused(_) => {
                            histogram.write_path_dispatch_refused += 1;
                        }
                    },
                    Ok(PostSignRoute::Refused(_)) => {
                        histogram.write_path_dispatch_refused += 1;
                    }
                    Err(e) => {
                        emit_dispatch_error(pool, doc_id, "c-signed-dispatch", &e, histogram)
                            .await?;
                    }
                }
            }
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
