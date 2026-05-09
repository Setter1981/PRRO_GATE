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
//!
//!   CALLER CONTRACT: do NOT blindly call `upsert_initial(fn, Online, Closed,
//!   1)` for every configured FN at App boot.  If the FN already has a row
//!   from a previous run, that row may carry `shift_state = Opened` from an
//!   in-flight shift that crashed mid-operation; overwriting it with
//!   `Closed` would mask the recovery requirement.  T14 / App::boot MUST
//!   either (a) call `get(fn)` first and only `upsert_initial` for missing
//!   rows, or (b) pass the `mode` / `shift_state` values it has already
//!   reconciled with `shifts` and `fiscal_documents` repos.  See
//!   bd-issue PRRO_GATE-ah8 tracking the App-boot constraint.
//! - `seed_prevhash` is what the `prro fn seed-prevhash <hex>` CLI calls
//!   when an operator imports a chain pre-history (spec §5.4).  Returns
//!   `false` (not error) if the FN row is missing — the caller can decide
//!   whether to upsert_initial first.

use crate::db::models::enums::{NodeMode, ShiftState};
use crate::db::models::ids::ShiftId;
use crate::db::tx::WriteTxConn;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct NodeStateRow {
    pub fiscal_number: String,
    pub mode: NodeMode,
    pub shift_state: ShiftState,
    pub next_lnd: i64,
    pub last_known_unsigned_xml_sha256: Option<[u8; 32]>,
    /// W5 — read existing schema columns (`migrations/001`).  All three
    /// can be `NULL` on a freshly bootstrapped FN row that has not yet
    /// been bound to profiles or had a shift opened.  Stage 2 guards
    /// reject when the request needs them but they are absent.
    pub current_shift_id: Option<ShiftId>,
    pub backend_profile_id: Option<String>,
    pub transport_profile_id: Option<String>,
}

/// Decode `last_known_unsigned_xml_sha256` BLOB into a fixed-32-byte array.
///
/// Fail-closed for chain-state safety: if the BLOB is present but its length
/// is not 32, return a decode error rather than silently treating the chain
/// as un-seeded.  Schema CHECK length=32 should make malformed rows
/// impossible in production, but a defensive check here means a corrupted
/// or hand-edited DB will surface as an explicit error at FN load time
/// (operator-visible) instead of silently breaking chain hashing.
fn decode_chain_hash(raw: Option<Vec<u8>>) -> Result<Option<[u8; 32]>, sqlx::Error> {
    match raw {
        None => Ok(None),
        Some(v) => {
            let arr: [u8; 32] = v.as_slice().try_into().map_err(|_| {
                sqlx::Error::Decode(
                    format!(
                        "node_state.last_known_unsigned_xml_sha256: expected 32 bytes, got {}",
                        v.len()
                    )
                    .into(),
                )
            })?;
            Ok(Some(arr))
        }
    }
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
                  last_known_unsigned_xml_sha256 as "last_known_unsigned_xml_sha256: Vec<u8>",
                  current_shift_id   as "current_shift_id: ShiftId",
                  backend_profile_id,
                  transport_profile_id
           FROM node_state WHERE fiscal_number = ?"#,
        fn_id
    )
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else {
        return Ok(None);
    };
    Ok(Some(NodeStateRow {
        fiscal_number: r.fiscal_number,
        mode: r.mode,
        shift_state: r.shift_state,
        next_lnd: r.next_lnd,
        last_known_unsigned_xml_sha256: decode_chain_hash(r.last_known_unsigned_xml_sha256)?,
        current_shift_id: r.current_shift_id,
        backend_profile_id: r.backend_profile_id,
        transport_profile_id: r.transport_profile_id,
    }))
}

/// W5 — same as [`get`] but takes `&mut WriteTxConn<'_>` so stage 1
/// reads `node_state` inside its `with_immediate` envelope (atomic
/// snapshot vs. allocate_next_lnd UPDATE that follows).
pub async fn get_tx(tx: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<Option<NodeStateRow>> {
    let row = sqlx::query!(
        r#"SELECT fiscal_number,
                  mode               as "mode: NodeMode",
                  shift_state        as "shift_state: ShiftState",
                  next_lnd,
                  last_known_unsigned_xml_sha256 as "last_known_unsigned_xml_sha256: Vec<u8>",
                  current_shift_id   as "current_shift_id: ShiftId",
                  backend_profile_id,
                  transport_profile_id
           FROM node_state WHERE fiscal_number = ?"#,
        fn_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else {
        return Ok(None);
    };
    Ok(Some(NodeStateRow {
        fiscal_number: r.fiscal_number,
        mode: r.mode,
        shift_state: r.shift_state,
        next_lnd: r.next_lnd,
        last_known_unsigned_xml_sha256: decode_chain_hash(r.last_known_unsigned_xml_sha256)?,
        current_shift_id: r.current_shift_id,
        backend_profile_id: r.backend_profile_id,
        transport_profile_id: r.transport_profile_id,
    }))
}

/// W5 / ADR-M3-A1 — atomically allocate and advance the FN's `next_lnd`.
///
/// Single statement `UPDATE ... RETURNING`: no CAS race window inside
/// the same `with_immediate` envelope; concurrent writers serialise on
/// the SQLite RESERVED lock and emerge with strictly monotonic lnd
/// values.  `ux_fd_fn_lnd` (W1 migration 007) is the fail-closed
/// downstream guard against any drift.
///
/// Returns the allocated lnd (the value that should be persisted on
/// the new fiscal_documents row); the FN's stored `next_lnd` is
/// advanced by 1 atomically.  Returns `RowNotFound` if the FN row
/// does not exist (caller must `upsert_initial` first or treat as
/// invariant breach — App::boot is responsible for ensuring the row
/// exists before stage 1 runs).
pub async fn allocate_next_lnd(tx: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<i64> {
    let allocated: i64 = sqlx::query_scalar(
        "UPDATE node_state SET next_lnd = next_lnd + 1 \
         WHERE fiscal_number = ? \
         RETURNING next_lnd - 1",
    )
    .bind(fn_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(allocated)
}

#[cfg(test)]
mod tests {
    use super::decode_chain_hash;

    #[test]
    fn decode_none_is_ok_none() {
        assert_eq!(decode_chain_hash(None).unwrap(), None);
    }

    #[test]
    fn decode_exact_32_is_ok_some() {
        let blob = vec![0xABu8; 32];
        assert_eq!(decode_chain_hash(Some(blob)).unwrap(), Some([0xABu8; 32]));
    }

    #[test]
    fn decode_too_short_fails_closed() {
        let err = decode_chain_hash(Some(vec![0u8; 31])).expect_err("31-byte blob must error");
        let msg = err.to_string();
        assert!(
            msg.contains("32 bytes"),
            "error must mention expected length: {msg}"
        );
        assert!(
            msg.contains("31"),
            "error must mention actual length: {msg}"
        );
    }

    #[test]
    fn decode_too_long_fails_closed() {
        let err = decode_chain_hash(Some(vec![0u8; 33])).expect_err("33-byte blob must error");
        assert!(err.to_string().contains("32 bytes"));
    }
}
