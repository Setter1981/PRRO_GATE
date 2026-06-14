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

use crate::db::models::enums::{DocState, NodeMode};
use crate::db::repositories::fiscal_documents::{self, DocumentRow};
use crate::db::repositories::node_state;
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

    // (a) SELECT-first — reuse the read-only pending list and filter to the two
    // resting online states.  ZERO wire calls if nothing is resting.
    let resting: Vec<DocumentRow> = fiscal_documents::list_pending_for_fn(pool, fiscal_number)
        .await?
        .into_iter()
        .filter(|d| matches!(d.state, DocState::Sent | DocState::Kvt1))
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
        }
    }
    Ok(())
}
