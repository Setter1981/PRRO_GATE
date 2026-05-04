//! Repository for `node_state`.
//!
//! `node_state` is single-row-per-FN — a long-lived snapshot of where the
//! gateway thinks the FN is in its lifecycle (mode, shift_state, next_lnd,
//! last_known_unsigned_xml_sha256, readiness/recovery markers, …).
//!
//! Repo policy:
//! - SELECT decode through `sqlx::query!` (compile-time schema verification).
//! - INSERT / UPDATE runtime-bound (`Encode<Sqlite>` covers param types).
//! - `upsert_initial` is intentionally idempotent for the bootstrap path:
//!   it inserts on first call and refreshes only the cheap `mode` and
//!   `shift_state` fields on conflict — `next_lnd`, recovery markers, and
//!   `last_known_unsigned_xml_sha256` are NOT clobbered.
//! - `seed_prevhash` is what the `prro fn seed-prevhash <hex>` CLI calls
//!   when an operator imports a chain pre-history (spec §5.4).  Returns
//!   `false` (not error) if the FN row is missing — the caller can decide
//!   whether to upsert_initial first.

use crate::db::models::enums::{NodeMode, ShiftState};
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct NodeStateRow {
    pub fiscal_number: String,
    pub mode: NodeMode,
    pub shift_state: ShiftState,
    pub next_lnd: i64,
    pub last_known_unsigned_xml_sha256: Option<[u8; 32]>,
}

pub async fn upsert_initial(
    pool: &SqlitePool,
    fn_id: &str,
    mode: NodeMode,
    shift_state: ShiftState,
    next_lnd: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(fiscal_number) DO UPDATE SET \
            mode = excluded.mode, shift_state = excluded.shift_state",
    )
    .bind(fn_id)
    .bind(mode)
    .bind(shift_state)
    .bind(next_lnd)
    .execute(pool)
    .await?;
    Ok(())
}

/// Set `last_known_unsigned_xml_sha256` on an existing FN row.  Returns
/// `true` if the FN row existed and was updated, `false` if no such row.
/// Caller decides whether to bootstrap via `upsert_initial` first.
pub async fn seed_prevhash(pool: &SqlitePool, fn_id: &str, hash: &[u8; 32]) -> sqlx::Result<bool> {
    let res = sqlx::query(
        "UPDATE node_state SET last_known_unsigned_xml_sha256 = ? WHERE fiscal_number = ?",
    )
    .bind(&hash[..])
    .bind(fn_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn get(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Option<NodeStateRow>> {
    let row = sqlx::query!(
        r#"SELECT fiscal_number,
                  mode               as "mode: NodeMode",
                  shift_state        as "shift_state: ShiftState",
                  next_lnd,
                  last_known_unsigned_xml_sha256 as "last_known_unsigned_xml_sha256: Vec<u8>"
           FROM node_state WHERE fiscal_number = ?"#,
        fn_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| NodeStateRow {
        fiscal_number: r.fiscal_number,
        mode: r.mode,
        shift_state: r.shift_state,
        next_lnd: r.next_lnd,
        last_known_unsigned_xml_sha256: r
            .last_known_unsigned_xml_sha256
            .and_then(|v| v.as_slice().try_into().ok()),
    }))
}
