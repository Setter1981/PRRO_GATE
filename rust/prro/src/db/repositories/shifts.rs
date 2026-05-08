//! Repository for `shifts`.
//!
//! Repo policy:
//! - SELECT decode goes through `sqlx::query!` (compile-time schema verification).
//! - INSERT and UPDATE use runtime-bound `sqlx::query()`; param types are already
//!   typed via `Encode<Sqlite>` on `ShiftId` / `ShiftState`.
//! - State transitions go through CAS UPDATE + a code-level `allowed_transition`
//!   whitelist.  CAS ensures concurrent transitions cannot race; the whitelist
//!   short-circuits forbidden moves before touching the DB.

use crate::db::models::{enums::ShiftState, ids::ShiftId};
use crate::db::tx::WriteTxConn;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct ShiftRow {
    pub shift_id: ShiftId,
    pub fiscal_number: String,
    pub serial: Option<i64>,
    pub state: ShiftState,
    pub cash_balance_kop: i64,
}

pub fn allowed_transition(from: ShiftState, to: ShiftState) -> bool {
    use ShiftState::*;
    matches!(
        (from, to),
        (Created, Opening)
            | (Opening, Opened)
            | (Opening, Error)
            | (Opened, Closing)
            | (Closing, Closed)
            | (Closing, Error)
            | (Error, Closed) // operator-driven recovery close
    )
}

pub async fn insert_created(
    pool: &SqlitePool,
    id: ShiftId,
    fiscal_number: &str,
    open_mode: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop) \
         VALUES (?, ?, 'CREATED', ?, 0)",
    )
    .bind(id)
    .bind(fiscal_number)
    .bind(open_mode)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, id: ShiftId) -> sqlx::Result<Option<ShiftRow>> {
    let row = sqlx::query!(
        r#"SELECT shift_id      as "shift_id: ShiftId",
                  fiscal_number,
                  serial,
                  state          as "state: ShiftState",
                  cash_balance_kop
           FROM shifts WHERE shift_id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ShiftRow {
        shift_id: r.shift_id,
        fiscal_number: r.fiscal_number,
        serial: r.serial,
        state: r.state,
        cash_balance_kop: r.cash_balance_kop,
    }))
}

/// Atomic CAS state transition.  Returns true if exactly one row
/// changed (transition succeeded), false otherwise.  Caller decides
/// what to do on `false` (typically: load current state and decide
/// whether to retry or give up).
///
/// The `allowed_transition` whitelist is enforced in code (cheap)
/// before hitting the DB.
///
/// Per ADR-M3-A4 / W0-2 §4.4 (M3a W2), takes `&mut WriteTxConn<'_>` —
/// callers obtain it from a `with_immediate` closure, mirroring the
/// `fiscal_documents::transition_state` discipline.
pub async fn transition(
    tx: &mut WriteTxConn<'_>,
    id: ShiftId,
    from: ShiftState,
    to: ShiftState,
) -> sqlx::Result<bool> {
    if !allowed_transition(from, to) {
        return Ok(false);
    }
    let res = sqlx::query("UPDATE shifts SET state = ? WHERE shift_id = ? AND state = ?")
        .bind(to)
        .bind(id)
        .bind(from)
        .execute(&mut **tx)
        .await?;
    Ok(res.rows_affected() == 1)
}
