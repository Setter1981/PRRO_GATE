//! B1 v1 — online-convergence tick (audit pass-2, item 3).
//!
//! Online documents resting in `SENT` / `KVT1` converge today ONLY on boot
//! (and an online `KVT1` never converges at all — the boot arm is
//! [`super::boot_phase::passive_hold_kvt1`], and drain serves only the offline
//! cohort).  On a 24/7 lane a receipt can hang in `KVT1` for days (OCF-5).
//! This module is the runtime owner that closes that gap: a periodic tick that,
//! for one FN, drives resting `SENT → (probe Match) → KVT1 → (evidence) → ACK`
//! by **reusing the existing recovery arms** — it invents no state transition
//! and opens no new write envelope.
//!
//! Reuse map (spec §2):
//! - `SENT`: the boot Sent-arm [`super::boot_phase::dispatch_sent_via_probe`]
//!   (same function, not a copy) — `last_chk` probe → {Match → `Sent→Kvt1`,
//!   Mismatch → Manual, NotFound → ER, Transport/Decode → hold}.
//! - `KVT1`: the drain `Kvt1Reentry` confirm path
//!   [`crate::services::offline_sync::kvt2_confirm::confirm_drain_doc`] —
//!   `last_chk` → Acked(non-empty) → `kvt2_advance::advance_to_ack(.., Kvt1)`
//!   → ACK + inbox DONE; Hold(empty) → stays `KVT1` until the next tick.
//!
//! A `SENT` doc cascades `SENT → KVT1 → ACK` within ONE tick (SENT-handler
//! then KVT1-handler on the re-read row) — see spec §3c + test 1.
//!
//! Invariants:
//! - **#1** (no wire/crypto inside a write tx): the tick opens NO
//!   `with_immediate`; the wire calls (`last_chk`) live inside the reused arms,
//!   which already keep them strictly between their own committed envelopes.
//! - **#4** (idempotency): every advance is a CAS-guarded transition owned by
//!   the reused arms — a second tick on a converged doc is a no-op (test 6).
//! - **#8** (recovery preserves the state machine): no new transition; only the
//!   whitelisted arm functions move state.
//! - **SELECT-first**: an empty tick (or a non-`Online` FN) issues ZERO wire
//!   calls (tests 3, 4) — so no per-FN backoff is needed.
//!
//! The per-FN A4 write-path gate is acquired by the caller
//! ([`crate::app::App::converge_online_for_fn`]), serialising the tick against
//! the live write-path for that FN (invariant #2).

use sqlx::SqlitePool;

use crate::db::models::enums::{DocState, NodeMode, Severity, ShiftState};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::fiscal_documents::{self, DocumentRow};
use crate::db::repositories::{audit_log, node_state};
use crate::services::offline_sync::backlog_drain::EscalationOutcome;
use crate::services::offline_sync::kvt2_confirm::{self, ConfirmDrainOutcome, Kvt2ConfirmSource};

use super::boot_phase::{self, DispatchHistogram};
use super::RuntimeView;

const LOG_TARGET: &str = "prro::services::reconciliation::online_convergence";

/// Per-FN outcome counters for one tick (operator log surface).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickSummary {
    pub fiscal_number: String,
    /// FN was not `Online` (Offline / GoingOnline are drain / return-probe
    /// jurisdiction) — the tick read `node_state` and issued zero wire calls.
    pub mode_skipped: bool,
    /// Number of resting `SENT` / `KVT1` docs the SELECT returned this tick.
    pub scanned: usize,
    /// `SENT` docs advanced to `KVT1` this tick (probe Match).
    pub advanced_sent_to_kvt1: usize,
    /// `KVT1` docs (incl. ones just cascaded from `SENT`) finalised to `ACK`.
    pub acked_from_kvt1: usize,
    /// `SENT` arm left the doc off the convergence ladder this tick
    /// (Mismatch → Manual, NotFound → ER, or a transport-class hold at `SENT`).
    pub sent_not_converged: usize,
    /// `KVT1` confirm held (empty / transient `last_chk`) — doc stays `KVT1`.
    pub held_kvt1: usize,
    /// `KVT1` confirm held because its DPS `last_chk` tip was SUPERSEDED by a
    /// newer submitted doc (benign supersession per AUD-L5-1 / SEAM-B-3 /
    /// M2-N4) — doc stays `KVT1`, no error.  Distinct dashboard signal from
    /// `held_kvt1` (transient/empty hold) and from `errors` (structural drift).
    pub superseded_held_kvt1: usize,
    /// Per-doc errors that were logged-and-skipped (isolation).
    pub errors: usize,
    /// `KVT1`/`SENT` confirm hit a `ChainSeedMismatch` (chain-integrity breach)
    /// and the FN was escalated to `RequiresManualReconciliation` (AUD-L2-1b) —
    /// a distinct operator signal vs `errors` (per-doc isolation skips).
    pub chain_seed_mismatch_escalated: usize,
    /// A.3 PR-C — resting `ERROR_RETRYABLE` docs re-driven via `stage_send`
    /// this tick (policy verdict `Redrive`): ER→Sending→Sent, un-gating the FN.
    pub er_redriven: usize,
    /// A.3 PR-C — ER docs escalated to `RequiresManualReconciliation` this tick
    /// (policy `BudgetExhausted` / `EscalateManual` / `EscalateInconsistent`).
    pub er_escalated_to_manual: usize,
    /// A.3 PR-C — ER docs HELD this tick (policy `HoldProbeRequired` /
    /// `HoldIndeterminate`): the FN stays gated, audit-only (no state change).
    pub er_held: usize,
}

impl TickSummary {
    fn new(fiscal_number: &str) -> Self {
        Self {
            fiscal_number: fiscal_number.to_string(),
            ..Default::default()
        }
    }

    fn skipped_mode(fiscal_number: &str) -> Self {
        Self {
            fiscal_number: fiscal_number.to_string(),
            mode_skipped: true,
            ..Default::default()
        }
    }
}

/// Run one online-convergence tick for a single FN.
///
/// SELECT-first: reads `node_state` (mode-guard) and the read-only pending
/// list, and only touches the wire for docs actually resting in `SENT`/`KVT1`.
/// Per-doc failures are logged and skipped so one stuck doc cannot abort the
/// rest of the FN's cohort (spec §3d).
pub async fn run_tick_for_fn(
    pool: &SqlitePool,
    view: &RuntimeView<'_>,
    fiscal_number: &str,
) -> anyhow::Result<TickSummary> {
    // (b) mode-guard — Online only.  A missing node_state row means the FN was
    // never booted; there is nothing to converge (boot bootstraps it).
    //
    // M1 review item 5 (M1-05/M1-H2 ruling, 2026-06-11) — this `Online`-only
    // guard is LOAD-BEARING for concurrency: the convergence tick holds the
    // `fn_write_gate`, the offline drain holds the SEPARATE `reconcile_mutex`
    // (see `App::drain_offline_backlog_with`).  Both reuse the same SENT/KVT1
    // arms under non-overlapping locks, so mutual exclusion relies on this
    // mode-partition (drain ⇒ `GoingOnline`; convergence ⇒ `Online`) + per-row
    // CAS — NOT a shared lock.  Unification under `fn_gate` is deferred to A2.4.
    let Some(ns) = node_state::get(pool, fiscal_number).await? else {
        return Ok(TickSummary::new(fiscal_number));
    };
    if ns.mode != NodeMode::Online {
        return Ok(TickSummary::skipped_mode(fiscal_number));
    }

    // sweep SW-2 — RMR re-entry guard (AUD-K8-1 parity).  A FN escalated to
    // shift_state==RMR while mode stayed Online (Batch-C convergence / boot-KVT2
    // escalation leaves mode) is HALTED until operator resolution — it must NOT
    // re-probe its resting SENT/KVT1 siblings (wire-traffic on a halted FN).
    // SELECT-first: return an empty skip BEFORE the pending SELECT (zero wire).
    if ns.shift_state == ShiftState::RequiresManualReconciliation {
        return Ok(TickSummary::new(fiscal_number));
    }

    // (a) SELECT-first — reuse the read-only pending list and filter to the two
    // resting online states.  ZERO wire calls if nothing is resting.
    // A.3 PR-C — the ER (pre-SENT) cohort joins the resting set: an online
    // ErrorRetryable doc is a NON-ISSUED D5-gate blocker (it stalls every
    // successor on the FN), so the tick is its runtime re-driver (via
    // er_redrive_policy) — otherwise the gate would be an FN-wide stall until
    // reboot (only boot re-drives ER today).
    let resting: Vec<DocumentRow> = fiscal_documents::list_pending_for_fn(pool, fiscal_number)
        .await?
        .into_iter()
        .filter(|d| {
            matches!(
                d.state,
                DocState::Sent | DocState::Kvt1 | DocState::ErrorRetryable
            )
        })
        .collect();

    let mut summary = TickSummary::new(fiscal_number);
    summary.scanned = resting.len();
    if resting.is_empty() {
        return Ok(summary);
    }

    // (c)/(d) per-doc cascade with per-doc error isolation.
    for doc in resting {
        let doc_id = doc.document_id;
        if let Err(e) = converge_one_doc(pool, view, doc, &mut summary).await {
            tracing::warn!(
                target: LOG_TARGET,
                fiscal_number,
                document_id = ?doc_id,
                error = ?e,
                "online-convergence: doc skipped this tick (per-doc isolation)"
            );
            summary.errors += 1;
        }
    }
    Ok(summary)
}

/// Cascade one doc `SENT → KVT1 → ACK` using the reused arms.  A `SENT` doc
/// runs the SENT-handler, is re-read, and (if it advanced to `KVT1`) falls
/// through to the KVT1-handler in the same tick; a `KVT1` doc runs only the
/// KVT1-handler.
async fn converge_one_doc(
    pool: &SqlitePool,
    view: &RuntimeView<'_>,
    doc: DocumentRow,
    summary: &mut TickSummary,
) -> anyhow::Result<()> {
    let doc_id = doc.document_id;
    let fiscal_number = doc.fiscal_number.clone();
    let mut doc = doc;

    // A.3 PR-C — ER resolver branch.  An `ErrorRetryable` doc is a non-issued
    // D5-gate blocker, NOT part of the SENT→KVT1→ACK convergence cascade; route
    // it through `er_redrive_policy` and return.  (A `Redrive` that advances the
    // doc to `Sent` un-gates the FN; the next tick converges Sent→ACK.)
    if doc.state == DocState::ErrorRetryable {
        return converge_error_retryable_doc(pool, view, &doc, summary).await;
    }

    // SENT-handler — the boot Sent-arm, invoked outside boot (same function).
    if doc.state == DocState::Sent {
        let mut hist = DispatchHistogram::default();
        boot_phase::dispatch_sent_via_probe(pool, view, &doc, &mut hist).await?;
        // Re-read to observe the committed advance (Sent → Kvt1 on Match) via
        // the read-only pending list.  NB (architect review fix): Mismatch CASes
        // the doc to `REQUIRES_MANUAL_RECONCILIATION`, which is NOT in the
        // pending-list state filter — so a vanished doc here is the EXPECTED
        // Mismatch outcome (the arm already audited the escalation), not just a
        // defensive race branch.  Count it as not-converged for the tick log.
        let Some(reread) = fiscal_documents::list_pending_for_fn(pool, &fiscal_number)
            .await?
            .into_iter()
            .find(|d| d.document_id == doc_id)
        else {
            // Mismatch → REQUIRES_MANUAL_RECONCILIATION left the pending cohort
            // (or, defensively, a concurrent delete).  The arm owns the audit
            // trail; the tick only records the non-convergence.
            summary.sent_not_converged += 1;
            return Ok(());
        };
        doc = reread;
        if doc.state == DocState::Kvt1 {
            summary.advanced_sent_to_kvt1 += 1;
            // fall through to the KVT1-handler — cascade within this tick.
        } else {
            // Mismatch → Manual, NotFound → ER, or transport-class hold at SENT.
            summary.sent_not_converged += 1;
            return Ok(());
        }
    }

    // KVT1-handler — the drain Kvt1Reentry confirm path (same function).
    if doc.state == DocState::Kvt1 {
        let Some(sfn) = doc.server_fiscal_no.clone() else {
            // KVT1 ALWAYS implies server_fiscal_no was stamped at SENT (stage_send
            // 4-b invariant).  A NULL here is a structural breach — surface as a
            // per-doc error so the isolation path logs it and moves on.
            anyhow::bail!(
                "online-convergence: KVT1 doc {doc_id:?} has NULL server_fiscal_no \
                 (stage_send 4-b stamp invariant breach)"
            );
        };
        match kvt2_confirm::confirm_drain_doc(
            pool,
            view.dps,
            &doc,
            &sfn,
            view.fn_sign,
            Kvt2ConfirmSource::Kvt1Reentry,
            // No fresh wire attempt this tick (Kvt1Reentry issues only last_chk).
            None,
        )
        .await?
        {
            ConfirmDrainOutcome::Advanced => summary.acked_from_kvt1 += 1,
            ConfirmDrainOutcome::HoldFnDrain { .. } => summary.held_kvt1 += 1,
            ConfirmDrainOutcome::SupersededHeld => {
                // **AUD-L5-1 (2026-06-14)**: a resting KVT1 whose DPS last_chk
                // tip was superseded by a newer submitted doc is a BENIGN hold,
                // NOT an error.  Kvt1Reentry is now superseded-capable
                // (kvt2_confirm fetch-gate widened); confirm_drain_doc already
                // emitted the TIP_SUPERSEDED (Warning) audit + left the doc at
                // KVT1 (no CAS).  The online tick has NO chain-head (unlike the
                // offline drain, which escalates Manual per ruling B over the
                // same outcome), so we simply HOLD and count a distinct
                // dashboard signal.  No doc-state change here.
                summary.superseded_held_kvt1 += 1;
            }
            ConfirmDrainOutcome::ChainSeedMismatch { document_id } => {
                // **AUD-L2-1b (2026-06-14)**: the Kvt2→Ack step hit a chain-seed
                // breach (node_state seed != doc.previous_hash, online-origin).
                // confirm_drain_doc committed Envelope 1 (Kvt1→Kvt2) and rolled
                // back the finalize, so the doc rests at KVT2.  Escalate the FN to
                // Manual (durable operator surface) instead of a silent per-doc
                // isolation skip.  Re-read node_state for the shift context (the
                // tick's `ns` is not threaded down here); escalate is idempotent
                // so a re-ticking FN already at RMR is a clean no-op.
                let ns = node_state::get(pool, &fiscal_number)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "online-convergence: node_state row vanished for \
                         {fiscal_number} during ChainSeedMismatch escalation"
                        )
                    })?;
                match crate::services::offline_sync::backlog_drain::escalate_fn_to_manual_recon(
                    pool,
                    &fiscal_number,
                    &ns,
                    document_id,
                    "chain_seed_mismatch",
                    0,
                    "CONVERGE_CHAIN_SEED_MISMATCH_ESCALATE_MANUAL",
                    "CONVERGE_CHAIN_SEED_MISMATCH_NO_SHIFT",
                )
                .await?
                {
                    EscalationOutcome::Escalated => summary.chain_seed_mismatch_escalated += 1,
                    // sweep SW-3: residual SEAM-D-1 mirror-desync (the tick's shift
                    // is normally Opened/escalatable).  The no-shift audit is the
                    // operator surface; the FN was NOT CAS'd to RMR → NOT counted as
                    // an escalation.  Proceed (per-doc isolation).
                    EscalationOutcome::NoEscalatableShift => {}
                }
            }
        }
    }
    Ok(())
}

// ─── A.3 PR-C — ER (pre-SENT) resolver ───────────────────────────────────────

/// Consecutive `HoldIndeterminate` ticks before the audit escalates
/// `Warning → Critical`.  `HoldIndeterminate` means the durable `retry_class`
/// is missing (no `transport_trace` row / NULL / unrecognized) — re-driving is
/// impossible, but the indeterminacy MAY resolve if a late-arriving trace
/// lands.  `N = 3` grants two grace ticks (Warning) before surfacing a CRITICAL
/// operator ticket — ambiguous-wire is a manual-recon family (CLAUDE.md), so a
/// prompt-but-not-spammy escalation is correct (this is a HOLD, never a spin:
/// the doc is not re-driven and the FN stays gated regardless of severity).
const HOLD_INDETERMINATE_CRITICAL_TICKS: i64 = 3;

const CONVERGE_ER_HOLD_INDETERMINATE: &str = "CONVERGE_ER_HOLD_INDETERMINATE";
const CONVERGE_ER_PROBE_DEFERRED: &str = "CONVERGE_ER_PROBE_DEFERRED";
const CONVERGE_ER_REDRIVE_ERROR: &str = "CONVERGE_ER_REDRIVE_ERROR";

fn doc_hex(doc_id: DocumentId) -> String {
    use std::fmt::Write;
    doc_id.as_bytes().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Resolve one resting `ErrorRetryable` doc via the EXISTING `er_redrive_policy`
/// (no policy-semantics change — the tick only ROUTES around its verdicts):
///   - `Redrive` → `stage_send::run` (the boot re-drive mechanism, now in the
///     tick): ErrorRetryable→Sending→Sent; the sfn stamp at the Sending→Sent
///     CAS makes the doc ISSUED ⇒ the D5 gate opens.
///   - `BudgetExhausted` / `EscalateManual` / `EscalateInconsistent` → escalate
///     the doc to `RequiresManualReconciliation` (reuse the boot CAS).
///   - `HoldProbeRequired` → HOLD + Warning audit (submit-time `last_chk`
///     reconciliation is deferred to M5, same as boot).
///   - `HoldIndeterminate` → fail-closed HOLD: the FN stays gated (doc
///     unchanged) + an audit that escalates Warning→Critical after N ticks.
///
/// Per-doc isolation: a `Redrive` `stage_send` error audits + counts, never
/// aborts the FN's cohort (the caller's per-doc guard also catches a bubbled
/// `Err`).  No wire/crypto inside a write tx (INV #1): `stage_send` and the
/// escalate CAS own their own envelopes; audit appends are pool-bound.
async fn converge_error_retryable_doc(
    pool: &SqlitePool,
    view: &RuntimeView<'_>,
    doc: &DocumentRow,
    summary: &mut TickSummary,
) -> anyhow::Result<()> {
    use crate::services::reconciliation::er_redrive_policy::{
        evaluate_er_redrive, ErRedriveDecision,
    };
    use crate::services::write_path::error_routing::RetryClass;

    let doc_id = doc.document_id;
    let entity = doc_hex(doc_id);

    match evaluate_er_redrive(pool, doc_id).await? {
        ErRedriveDecision::Redrive => {
            match crate::services::write_path::stage_send::run(
                pool,
                view.dps,
                doc_id,
                Some(view.signing_ctx),
            )
            .await
            {
                Ok(_) => summary.er_redriven += 1,
                Err(e) => {
                    // Per-doc isolation: record + count, do NOT abort the cohort.
                    let payload = serde_json::json!({
                        "document_id": entity,
                        "branch": "converge-er-redrive",
                        "error": e.to_string(),
                    });
                    audit_log::append(
                        pool,
                        "fiscal_document",
                        &entity,
                        CONVERGE_ER_REDRIVE_ERROR,
                        Severity::Warning,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                    summary.errors += 1;
                }
            }
        }
        ErRedriveDecision::BudgetExhausted { .. } => {
            boot_phase::cas_error_retryable_to_manual_reconciliation(
                pool,
                doc_id,
                RetryClass::TransientRetry.as_str(),
                Severity::Error,
            )
            .await?;
            summary.er_escalated_to_manual += 1;
        }
        ErRedriveDecision::EscalateManual { class } => {
            boot_phase::cas_error_retryable_to_manual_reconciliation(
                pool,
                doc_id,
                class.as_str(),
                Severity::Error,
            )
            .await?;
            summary.er_escalated_to_manual += 1;
        }
        ErRedriveDecision::EscalateInconsistent { class } => {
            boot_phase::cas_error_retryable_to_manual_reconciliation(
                pool,
                doc_id,
                class.as_str(),
                Severity::Critical,
            )
            .await?;
            summary.er_escalated_to_manual += 1;
        }
        ErRedriveDecision::HoldProbeRequired => {
            let payload = serde_json::json!({
                "document_id": entity,
                "branch": "converge-er-hold",
                "retry_class": "ProbeRequired",
            });
            audit_log::append(
                pool,
                "fiscal_document",
                &entity,
                CONVERGE_ER_PROBE_DEFERRED,
                Severity::Warning,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            summary.er_held += 1;
        }
        ErRedriveDecision::HoldIndeterminate => {
            // Fail-closed HOLD: the FN stays gated (doc unchanged).  Escalate the
            // audit Warning→Critical once N consecutive HoldIndeterminate ticks
            // have accrued (durable evidence missing → operator triage surface).
            let prior = count_converge_indeterminate_audits(pool, &entity).await?;
            let severity = if prior + 1 >= HOLD_INDETERMINATE_CRITICAL_TICKS {
                Severity::Critical
            } else {
                Severity::Warning
            };
            let payload = serde_json::json!({
                "document_id": entity,
                "branch": "converge-er-hold",
                "retry_class": "indeterminate",
                "tick_no": prior + 1,
            });
            audit_log::append(
                pool,
                "fiscal_document",
                &entity,
                CONVERGE_ER_HOLD_INDETERMINATE,
                severity,
                None,
                Some(&payload.to_string()),
            )
            .await?;
            summary.er_held += 1;
        }
    }
    Ok(())
}

/// Count prior `CONVERGE_ER_HOLD_INDETERMINATE` audit rows for a doc — the
/// durable per-doc tick counter backing the Warning→Critical escalation (no
/// schema churn: the audit trail IS the counter).
async fn count_converge_indeterminate_audits(pool: &SqlitePool, entity: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? AND event_type = ?",
    )
    .bind(entity)
    .bind(CONVERGE_ER_HOLD_INDETERMINATE)
    .fetch_one(pool)
    .await
}
