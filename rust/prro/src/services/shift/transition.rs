//! RS-3 C1 — the single shift-state transition-service.
//!
//! Per the pilot Q1-A′ decision (`shifts` table is WRITE-primary,
//! `node_state.shift_state` / `current_shift_id` is a READ-projection),
//! every shift-state mutation that must keep the node_state projection in
//! lock-step (m3b-shift-state-expansion.md §5 load-bearing mirror
//! invariant) goes through THIS module — and nowhere else.
//!
//! Before C1, two paths hand-wrote the `shifts` transition + the
//! `node_state` projection mirror as adjacent statements: the offline
//! drain (`offline_sync/backlog_drain.rs` escalate-to-manual + the W9b
//! finalization shift ladder) and boot reconciliation
//! (`reconciliation/boot_phase.rs` orphan-shift quarantine). C1 hoists
//! the projection write into named helpers here so the primary↔projection
//! pairing is enforced in one place. A drift-guard test
//! (`tests/shift_transition_service_is_sole_node_state_writer.rs`) asserts
//! no other module raw-writes the shift projection.
//!
//! Invariant #1 (no network/crypto inside a write transaction) holds:
//! every function here is pure SQL on the caller's `WriteTxConn` — the
//! caller owns the `with_immediate` envelope and the atomicity.

use crate::db::models::enums::ShiftState;
use crate::db::models::ids::ShiftId;
use crate::db::repositories::shifts::{self, TransitionOutcome};
use crate::db::tx::WriteTxConn;
use crate::db::types::DbShiftId;
use crate::services::write_path::types::hex_encode_lower as hex_lower;

/// Apply a whitelisted 9-state shift transition AND mirror the
/// `node_state` read-projection, atomically inside the caller's
/// `with_immediate` envelope.
///
/// The shift CAS uses [`shifts::transition_state`] (whitelist + CAS on
/// `(shift_id, from)`); on [`TransitionOutcome::Applied`] the projection
/// is mirrored with a CAS-guarded UPDATE keyed on
/// `(fiscal_number, current_shift_id, from)` so a concurrent writer
/// cannot smuggle a different shift state past the mirror. On any
/// non-`Applied` outcome the projection is left untouched (the shift did
/// not move) and the caller decides how to surface it.
///
/// PRECONDITION: `shift_id` is the FN's `node_state.current_shift_id`
/// (the active shift). Intent edges that OPEN a *new* shift and SET
/// `current_shift_id` for the first time are added when `stage_acquire`
/// is wired (RS-3 A-pieces); C1's callers all transition the already-
/// active shift, so the symmetric mirror suffices.
///
/// POINTER LIFECYCLE: this mirror updates `shift_state` ONLY. On a
/// terminal close (`Closing → Closed`) it therefore leaves
/// `current_shift_id` pointing at the now-`Closed` shift — same as before
/// C1. Orphan resolution clears the pointer ([`clear_active_shift_projection`]),
/// but whether a *normal* close should also clear it is an RS-3 A-piece
/// decision (the next `stage_acquire` open will overwrite it regardless).
/// `terminal_close_pins_current_shift_id_behavior` locks the status quo.
///
/// Returns the shift CAS outcome. The mirror itself failing
/// (`rows_affected != 1` on an `Applied` shift) is structural drift →
/// `Err`, rolling back the caller's envelope.
pub async fn apply_shift_transition(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    shift_id: ShiftId,
    from: ShiftState,
    to: ShiftState,
) -> anyhow::Result<TransitionOutcome> {
    let outcome = shifts::transition_state(tx, shift_id, from, to).await?;
    if let TransitionOutcome::Applied = outcome {
        mirror_projection_tx(tx, fiscal_number, shift_id, from, to).await?;
    }
    Ok(outcome)
}

/// Force an orphan shift row to `ERROR` (raw, bypassing the §4.1
/// whitelist and the operator force seam).
///
/// Per spec §4.4 this is ONE OF TWO sanctioned entry surfaces for
/// `Error` (the other being the operator-driven `force_to_*_with_audit`
/// seam). It exists for the boot orphan-recovery path: a shift left in
/// `OPENING` / `CLOSING` across a crash with no pending work and no
/// operator identity in scope. The caller emits the branch-specific
/// `SHIFT_BOOT_ORPHAN_ERROR` forensic audit (kept at the callsite so it
/// carries the boot-branch context); this helper only owns the raw shift
/// write so it lives alongside the projection reset rather than as a
/// stray SQL statement in `boot_phase.rs`.
///
/// Asymmetric by design: the dead shift row is quarantined in `ERROR`
/// while the node returns to "no active shift" — see
/// [`clear_active_shift_projection`], which the caller invokes once per
/// FN after quarantining its orphan(s).
pub async fn force_orphan_shift_to_error(
    tx: &mut WriteTxConn<'_>,
    shift_id: ShiftId,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE shifts SET state = 'ERROR' WHERE shift_id = ?")
        .bind(DbShiftId(shift_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Reset the FN's shift projection to "no active shift": set
/// `node_state.shift_state = 'CLOSED'` and clear `current_shift_id`,
/// scoped to the orphan source states `('OPENING', 'CLOSING')`.
///
/// Runs once per FN in the boot orphan-recovery branch — including when
/// zero orphan shift rows were found, to repair a dangling
/// `OPENING`/`CLOSING` projection that has no backing shift row.
/// Clearing `current_shift_id` (RS-3 C2) prevents a resolved orphan from
/// leaving a pointer at the now-`ERROR` shift.
///
/// The `shift_state IN ('OPENING','CLOSING')` guard means a projection
/// already in any other state is left untouched (idempotent / no-op).
pub async fn clear_active_shift_projection(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE node_state SET shift_state = 'CLOSED', current_shift_id = NULL \
         WHERE fiscal_number = ? AND shift_state IN ('OPENING', 'CLOSING')",
    )
    .bind(fiscal_number)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// `node_state.shift_state` mirror — m3b-shift-state-expansion.md §5
/// load-bearing invariant: `node_state.shift_state` MUST equal the active
/// shift row's state. CAS-guarded on `(fiscal_number, current_shift_id,
/// from)` so a concurrent writer cannot smuggle a different shift state
/// past the mirror. A non-1 `rows_affected` surfaces as an `anyhow`
/// chain so the caller's `with_immediate` ROLLS BACK the whole envelope.
async fn mirror_projection_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    shift_id: ShiftId,
    from: ShiftState,
    to: ShiftState,
) -> anyhow::Result<()> {
    let rows_affected = sqlx::query(
        "UPDATE node_state SET shift_state = ? \
         WHERE fiscal_number = ? AND shift_state = ? AND current_shift_id = ?",
    )
    .bind(to.as_str())
    .bind(fiscal_number)
    .bind(from.as_str())
    .bind(DbShiftId(shift_id))
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows_affected != 1 {
        return Err(anyhow::anyhow!(
            "shift transition-service({fiscal_number}): node_state.shift_state mirror \
             UPDATE produced rows_affected={rows_affected} (expected 1; structural drift \
             between shifts and node_state for shift {shift_hex})",
            shift_hex = hex_lower(shift_id.as_bytes()),
        ));
    }
    Ok(())
}

/// Create a brand-new shift for `fiscal_number` and bind it as the FN's
/// active shift — A′.1 pieces 0b+1.  The intent-open edge that this
/// service's `apply_shift_transition` docstring reserves ("edges that OPEN
/// a *new* shift and SET `current_shift_id` for the first time").
///
/// Runs inside the caller's `with_immediate` envelope (`stage_acquire`
/// fresh-Proceed / `stage_offline_ack`, wired in Phase C).  It:
///   (i)  INSERTs the `shifts` row `state='CREATED'` via
///        [`shifts::insert_created_tx`] (`shift_id` deterministic from the
///        SHIFT_OPEN document → an idempotent re-create collides on the PK;
///        the partial-unique index `ux_shifts_one_open_per_fn` (migration
///        026) is the schema-level backstop against a second open shift);
///   (ii) CASes the `node_state` projection `CLOSED → CREATED` and SETs
///        `current_shift_id`, keyed on `shift_state='CLOSED'`.  The pointer
///        is OVERWRITTEN, NOT required-NULL: a normal close leaves
///        `current_shift_id` dangling at the closed shift
///        (`terminal_close_pins_current_shift_id_behavior`), so NULL only
///        occurs on fresh bootstrap / after orphan-clear.
///
/// `rows_affected != 1` on the CAS ⇒ the FN was not `CLOSED` (a shift is
/// already live, or the FN row is missing) → fail-closed `Err`, rolling
/// back the whole envelope (including the shifts INSERT).  This bare
/// `node_state` projection write lives HERE (the sole-writer allowlist) by
/// design — no other module may raw-write the projection.
/// `opening_cash_kop` — carry-over opening balance for the new shift
/// (prior shift's `cash_balance_kop`; 0 for the FN's first shift).
/// Stored atomically with the shift-row INSERT in this envelope (invariant #2).
pub async fn create_shift_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    shift_id: ShiftId,
    open_mode: &str,
    opened_by_cashier_id: &str,
    opening_cash_kop: i64,
) -> anyhow::Result<()> {
    // (i) Mint the shifts row as CREATED.  An idempotent re-create of the
    //     SAME SHIFT_OPEN collides on the PK (deterministic shift_id); the
    //     partial-unique index `ux_shifts_one_open_per_fn` backstops a
    //     second open shift for the FN at the schema level.
    shifts::insert_created_tx(
        tx,
        shift_id,
        fiscal_number,
        open_mode,
        opened_by_cashier_id,
        opening_cash_kop,
    )
    .await?;

    // (ii) CAS the node_state projection CLOSED → CREATED and SET the
    //      pointer.  Keyed on `shift_state='CLOSED'` with pointer OVERWRITE
    //      (NOT `current_shift_id IS NULL`): the close-pin leaves the pointer
    //      dangling at the prior closed shift, so NULL only occurs on fresh
    //      bootstrap / after orphan-clear.  `rows_affected != 1` ⇒ the FN is
    //      not CLOSED (a shift is already live) or the row is missing ⇒
    //      fail-closed Err, rolling back the shifts INSERT in the same
    //      envelope.  This bare projection write lives HERE (the sole-writer
    //      allowlist) by design.
    let rows_affected = sqlx::query(
        "UPDATE node_state SET current_shift_id = ?, shift_state = 'CREATED' \
         WHERE fiscal_number = ? AND shift_state = 'CLOSED'",
    )
    .bind(DbShiftId(shift_id))
    .bind(fiscal_number)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows_affected != 1 {
        return Err(anyhow::anyhow!(
            "create_shift_tx({fiscal_number}): node_state CLOSED→CREATED CAS produced \
             rows_affected={rows_affected} (expected 1; a shift is already live for this FN, \
             or the node_state row is missing) for shift {shift_hex}",
            shift_hex = hex_lower(shift_id.as_bytes()),
        ));
    }
    Ok(())
}
