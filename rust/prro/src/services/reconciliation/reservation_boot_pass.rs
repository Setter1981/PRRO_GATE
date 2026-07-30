//! CS-3 S7-1 · A3 — the boot-first delivery-reservation pass (design-of-record
//! `docs/CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` §7.1 / §7.2; plan `docs/CS3_S7_A3_BOOT_PASS_PLAN.md`).
//!
//! Runs ONCE, GLOBALLY, **before** the per-FN `run_boot_reconciliation` loop (wired in
//! `App::reconcile_pending_inner`), under the App recon mutex.  It resolves every delivery reservation
//! that a crash left mid-lifecycle, so exactly-once-across-crash holds:
//!
//! 1. **normalize** every `CALL_STARTED` reservation → `resume_crashed_reservation`
//!    (`NoResponse{CrashedBeforeObservation}` + `PENDING_APPLY` + node STOP), each in its own
//!    `BEGIN IMMEDIATE`.  A reservation whose `node_state` row is missing (the NC-03 anomaly) is
//!    DEFERRED, not converted;
//! 2. **apply** every `OUTCOME_OBSERVED + PENDING_APPLY` reservation via the shared
//!    `apply_orchestration::apply_recorded_outcome` (S7-1 B3 — fires the online shift edges 3/10 +
//!    Z closing-cash in the apply tx, which the bare repo `apply_outcome` skipped).  A
//!    `HeldNotAutoRelease` (a `-12`/`-6`/SubmittedUnknown hold) is EXPECTED — logged, never a boot
//!    error (frozen invariant #9).  `NodeStateMissing` DEFERS the FN;
//! 3. **reconstruct** the lost `node_state` for the deferred FNs (`boot_phase::reconstruct_lost_node_state`),
//!    RESTORING the surviving reservation's authority (generation + active pointer) and setting node
//!    STOP_MODE + CRITICAL audit — without this both `apply_outcome` and `complete_operator_pending`
//!    fail the identical CAS forever (cutover fix A). DELIBERATELY unfenced (§7.2): a live PENDING
//!    reservation is expected during it, and fencing it would strand the reconstruction;
//! 4. **retry** the deferred NORMALIZE (a CALL_STARTED-deferred → OO+PENDING). NC-03 FNs are NOT
//!    auto-applied — an operator resolves them via `complete_operator_pending` (fix A).
//!
//! It performs **no wire I/O and no crypto** — every step is a short DB transaction (invariant #1).
//! No new state-machine edges (`resume`/`apply` are the landed Slice-4 transitions).
//!
//! **INACTIVE until the Slice-7 cutover.** `authorize_submission` has no production caller yet, so no
//! `NewReservation` is minted in production and both boot queries return empty → this is a provable
//! no-op today.  It is landed + tested now so the atomic cutover only has to flip the live wire.
//!
//! This file is the sanctioned production caller of the still-INACTIVE `resume`/`apply`/`list_*`
//! lifecycle helpers; it is therefore excluded from the `inactive_lifecycle_scan` "no production
//! caller" pin (`p03`/`rg08`) and its exact reference set is positively pinned by
//! `migration_032::boot_pass_references_only_the_sanctioned_read_apply_subset`.

use std::collections::BTreeMap;

use sqlx::SqlitePool;

use super::boot_phase;
use crate::db::repositories::delivery_reservation::{self, ApplyError, ApplyResult, ReservationId};
use crate::db::repositories::{audit_log, node_state};
use crate::db::tx::{with_immediate, WriteTxConn};
use crate::services::write_path::apply_orchestration;
use crate::services::write_path::types::hex_encode_lower as hex_lower;

/// Counts emitted for the boot summary / observability (the S7-P4-BOOT tooth asserts DB *state*, not
/// these counters).
#[derive(Debug, Default, Clone, Copy)]
pub struct ReservationBootSummary {
    /// `CALL_STARTED → OUTCOME_OBSERVED + PENDING_APPLY` conversions (crash resume).
    pub normalized: u64,
    /// `OUTCOME_OBSERVED + PENDING_APPLY → APPLIED` auto-releases.
    pub applied: u64,
    /// Recorded outcomes held for the operator (`HeldNotAutoRelease` — expected, not an error).
    pub held: u64,
    /// FNs whose lost `node_state` was reconstructed (the NC-03 defer path).
    pub reconstructed: u64,
    /// Benign stale-drops (generation-CAS miss — mutated nothing).
    pub dropped: u64,
}

impl ReservationBootSummary {
    /// `true` when the pass did any work — used only to gate an info-level log line.
    pub fn is_active(&self) -> bool {
        self.normalized + self.applied + self.held + self.reconstructed + self.dropped > 0
    }
}

/// How a single `apply_outcome` attempt classifies for the pass's control flow.
enum ApplyClass {
    /// Auto-released (or a benign stale-drop) — carries the `ApplyResult`.
    Resolved(ApplyResult),
    /// A `-12`/`-6`/SubmittedUnknown hold — EXPECTED, must not fail boot.
    Held,
    /// The FN's `node_state` row is missing — defer to the NC-03 reconstruction.
    NodeMissing,
    /// A genuine error (DB / structural) — fail boot.
    Fatal(anyhow::Error),
}

/// Apply one reservation in its own `BEGIN IMMEDIATE`, classifying the outcome.  The closure returns
/// the `apply_outcome` error as an `anyhow::Error` so the transaction ROLLS BACK on any error (a
/// partial effect-write is never committed); a `Held`/`NodeMissing` returns before any write in
/// `apply_outcome`, so its rollback is a no-op.
/// Apply one recorded outcome THROUGH the shared apply orchestration (S7-1 B3 fix, re-audit
/// external-F1 / internal-B3). `apply_recorded_outcome` opens its own `BEGIN IMMEDIATE` and fires the
/// online shift edges 3/10 (`Opening → Opened` / `Closing → Closed`) + Z closing-cash in the SAME tx
/// as the doc-state CAS, returning the identical `HeldNotAutoRelease`/`NodeStateMissing`
/// classification via the anyhow contract (FIX-C). The bare repo `apply_outcome` (the prior boot
/// path) advanced the doc/SFN/seed but SKIPPED the shift edge, stranding an online SHIFT_OPEN/Z
/// crash-resume in `Opening`/`Closing` (later destructively orphaned by boot branch e2).
async fn apply_one(
    pool: &SqlitePool,
    reservation_id: delivery_reservation::ReservationId,
) -> ApplyClass {
    let attempt = apply_orchestration::apply_recorded_outcome(pool, reservation_id).await;
    match attempt {
        Ok(res) => ApplyClass::Resolved(res),
        Err(e) => match e.downcast_ref::<ApplyError>() {
            Some(ApplyError::HeldNotAutoRelease) => ApplyClass::Held,
            Some(ApplyError::NodeStateMissing) => ApplyClass::NodeMissing,
            _ => ApplyClass::Fatal(e),
        },
    }
}

/// Normalize one crashed `CALL_STARTED` reservation in its own `BEGIN IMMEDIATE`: resume it to the
/// durable `NoResponse{Crashed}` hold AND (§7.1) atomically complete its in-flight `transport_trace`
/// as a crash + append a recovery audit. The `< 60 s` orphan scanner
/// (`boot_phase::close_orphan_transport_traces`) deliberately skips fresh traces, so a fast restart
/// would otherwise leave the crashed wire's trace open. Returns whether the reservation was actually
/// converted (a re-run after conversion is a no-op → `false`), so the caller counts only real work.
async fn normalize_one(
    pool: &SqlitePool,
    reservation_id: ReservationId,
    fiscal_number: &str,
) -> anyhow::Result<bool> {
    let fscl = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let converted =
                delivery_reservation::resume_crashed_reservation(tx, reservation_id, &fscl)
                    .await
                    .map_err(anyhow::Error::from)?;
            if converted {
                complete_crashed_trace(tx, reservation_id).await?;
            }
            Ok::<bool, anyhow::Error>(converted)
        })
    })
    .await
}

/// §7.1 — in the caller's tx, complete the crashed reservation's document's OPEN `transport_trace` as
/// `SYSTEM_CRASH` (the wire outcome is unknown) + a recovery audit. Mirrors the boot orphan scanner's
/// crash-close (`boot_phase::close_orphan_transport_traces`) but is **TTL-free** — a `CALL_STARTED`
/// reservation is a definitive crash — and targets exactly the reservation's own document. A doc with
/// no open trace (a pre-wire crash) is a benign no-op.
async fn complete_crashed_trace(
    tx: &mut WriteTxConn<'_>,
    reservation_id: ReservationId,
) -> anyhow::Result<()> {
    let doc_id: Vec<u8> =
        sqlx::query_scalar("SELECT document_id FROM delivery_reservation WHERE reservation_id = ?")
            .bind(&reservation_id[..])
            .fetch_one(&mut **tx)
            .await?;
    // The single open (uncompleted) trace for that document, if any.
    let open: Option<(i64, String)> = sqlx::query_as(
        // ordering-justified: `attempt_no` is allocated `COALESCE(MAX(attempt_no),0)+1` per
        // `document_id` inside the BEGIN IMMEDIATE envelope (transport_trace.rs), so it is
        // unique per document by the ALLOCATOR. NOTE: the index
        // `transport_trace(document_id, attempt_no DESC, retry_class)` is NOT unique — the
        // guarantee rests on the allocator + write lease, not on the schema.
        "SELECT attempt_no, started_at FROM transport_trace \
         WHERE document_id = ? AND outcome_kind IS NULL ORDER BY attempt_no DESC LIMIT 1",
    )
    .bind(&doc_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((attempt_no, started_at)) = open else {
        return Ok(()); // pre-wire crash — no trace to complete
    };
    let rows = sqlx::query(
        "UPDATE transport_trace SET \
            completed_at = CURRENT_TIMESTAMP, wire_call_started_at = ?, \
            wire_call_finished_at = CURRENT_TIMESTAMP, outcome_kind = 'SYSTEM_CRASH', \
            error_kind = 'CRASHED_RESERVATION_AT_BOOT', \
            error_message = 'Reservation resumed at boot; wire outcome unknown' \
         WHERE document_id = ? AND attempt_no = ? AND outcome_kind IS NULL",
    )
    .bind(&started_at)
    .bind(&doc_id)
    .bind(attempt_no)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows == 1 {
        let doc_hex = hex_lower(&doc_id);
        let payload = serde_json::json!({
            "document_id": doc_hex,
            "attempt_no": attempt_no,
            "outcome_kind": "SYSTEM_CRASH",
            "reason": "boot reservation pass resumed a crashed CALL_STARTED reservation; the in-flight wire outcome is unknown",
        });
        audit_log::append_tx(
            tx,
            "transport_trace",
            &doc_hex,
            "TRANSPORT_TRACE_CRASHED_RESERVATION_CLOSED",
            crate::db::models::enums::Severity::Info,
            None,
            Some(&payload.to_string()),
        )
        .await?;
    }
    Ok(())
}

/// The boot-first reservation pass (§7.2). Runs under the App recon mutex (`_guard` proves it).
pub async fn run(
    _guard: &super::ReconcileGuard<'_>,
    pool: &SqlitePool,
) -> anyhow::Result<ReservationBootSummary> {
    let mut summary = ReservationBootSummary::default();
    // FNs deferred because their `node_state` row was lost (NC-03), mapped to their single active
    // reservation (the §3.1 fence ⇒ ≤1 per FN) — reconstructed + authority-restored in step 3.
    let mut deferred: BTreeMap<String, ReservationId> = BTreeMap::new();

    // ── Step 1 — normalize crashed CALL_STARTED reservations ───────────────────────────────────
    for (reservation_id, fscl) in
        delivery_reservation::list_call_started_without_outcome(pool).await?
    {
        // Defer NC-03 FNs: `resume` sets STOP unconditionally but does not fail on a missing
        // `node_state` row (it would convert the reservation without a mode flip).  Under the recon
        // mutex there is no concurrent writer, so this pool read is stable through the resume below.
        if node_state::get(pool, &fscl).await?.is_none() {
            deferred.insert(fscl, reservation_id);
            continue;
        }
        if normalize_one(pool, reservation_id, &fscl).await? {
            summary.normalized += 1;
        }
    }

    // ── Step 2 — apply recorded OUTCOME_OBSERVED + PENDING_APPLY reservations ───────────────────
    for (reservation_id, fscl) in
        delivery_reservation::list_outcome_observed_pending_apply(pool).await?
    {
        match apply_one(pool, reservation_id).await {
            ApplyClass::Resolved(res) => {
                if res.applied {
                    summary.applied += 1;
                } else {
                    summary.dropped += 1;
                }
            }
            ApplyClass::Held => {
                summary.held += 1;
                tracing::warn!(
                    fiscal_number = %fscl,
                    "boot reservation pass: recorded outcome held for operator (expected — not a boot error)"
                );
            }
            ApplyClass::NodeMissing => {
                deferred.insert(fscl, reservation_id);
            }
            ApplyClass::Fatal(e) => return Err(e),
        }
    }

    // ── Step 3 — reconstruct lost node_state for the deferred FNs (UNFENCED by design), RESTORING
    //    the surviving reservation's authority so operator completion can resolve it (cutover fix A).
    for (fscl, res_id) in &deferred {
        if boot_phase::reconstruct_lost_node_state(pool, fscl, Some(*res_id)).await? {
            summary.reconstructed += 1;
            tracing::error!(
                fiscal_number = %fscl,
                "boot reservation pass: node_state lost with a live reservation (NC-03) — reconstructed + authority restored + STOP"
            );
        }
    }

    // ── Step 4 — retry the deferred NORMALIZE now that node_state exists (a CALL_STARTED-deferred
    //    reservation → OO+PENDING; resume preserves the restored pointer/generation). NC-03 FNs are
    //    NOT auto-applied — an operator resolves them via `complete_operator_pending` (cutover fix A):
    //    auto-stamping/advancing a seed on a just-reconstructed node is unsafe.
    if !deferred.is_empty() {
        for (reservation_id, fscl) in
            delivery_reservation::list_call_started_without_outcome(pool).await?
        {
            if !deferred.contains_key(&fscl) {
                continue;
            }
            if normalize_one(pool, reservation_id, &fscl).await? {
                summary.normalized += 1;
            }
        }
    }

    Ok(summary)
}
