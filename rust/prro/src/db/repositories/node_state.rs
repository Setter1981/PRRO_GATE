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
    /// W6 — Z-report counter allocator state (`migrations/009`).  Bumped
    /// by [`allocate_z_report_number`] strictly inside `with_immediate`
    /// envelopes, mirroring `next_lnd`'s discipline.  CHECK (>=1) at
    /// schema level makes wraparound observable as a constraint
    /// violation rather than a silent corruption.
    pub next_z_report_number: i64,
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

/// Tx-bound variant of [`upsert_initial`] — identical INSERT/ON-CONFLICT,
/// composable inside a `with_immediate` envelope.  Added for NC-03 (2026-06-11):
/// boot branch (a) must create the `node_state` row with a RECONSTRUCTED
/// `next_lnd`, project the MAC seed, and flip to BLOCKED (via the W10.3 setter)
/// ATOMICALLY in one envelope when the ledger survived but the row was lost.
pub async fn upsert_initial_tx(
    tx: &mut WriteTxConn<'_>,
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
    .execute(&mut **tx)
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

/// W8 stage 5 finalize — advance the cross-doc MAC chain seed
/// **inside** the same `with_immediate` envelope as the CAS
/// `Kvt2 → Ack` on `fiscal_documents`.  Tx-bound counterpart of
/// [`seed_prevhash`].
///
/// **Atomicity contract.**  Caller (`stage_finalize::run`) MUST
/// invoke this AFTER the CAS Applied; relative ordering against the
/// other post-CAS writes (inbox-DONE / outbox-INSERT / audit) does
/// not matter — all four commit atomically together as a single
/// `with_immediate` envelope.  If any one fails, the entire tx rolls
/// back and the seed advance is undone — the next-doc MAC chain
/// pointer cannot leak past a failed Ack.
///
/// Returns `true` if the FN row existed and was updated; `false`
/// indicates a missing `node_state` row, which is a state-invariant
/// breach (W5 acquire stage upserts the row before stage 1 runs).
/// Caller MUST treat `false` as a typed stage error
/// (`StageFinalizeError::SeedUpdateMissing`), not silent ignore.
pub async fn update_last_known_xml_sha_tx(
    tx: &mut WriteTxConn<'_>,
    fn_id: &str,
    hash: &[u8; 32],
) -> sqlx::Result<bool> {
    let res = sqlx::query(
        "UPDATE node_state SET last_known_unsigned_xml_sha256 = ? WHERE fiscal_number = ?",
    )
    .bind(&hash[..])
    .bind(fn_id)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected() == 1)
}

/// W10.3 — flip `node_state.mode` to `BLOCKED` for the FN.  Mirror of
/// [`update_last_known_xml_sha_tx`]: tx-bound, single UPDATE,
/// returns `bool` (`true` = row existed and was updated; `false` =
/// missing FN row, structural breach).
///
/// Invoked from `stage_send::run` 4-b closure when
/// `decision.node_mode_flip == Some(NodeMode::Blocked)` (Server `-11`
/// ERROR_OFFLINE_168 — 168-hour cumulative-offline limit exceeded
/// per W0-1 §2.4).  Atomic with the post-wire CAS `Sending → Rejected`,
/// `transport_trace::complete_tx`, and the audit
/// `STAGE_SEND_NODE_BLOCKED` row, all in the same `with_immediate`
/// envelope.
///
/// **Idempotent.**  CAS-style guard `WHERE mode != 'BLOCKED'` would
/// add cost (one row read) for no real benefit: if the doc just
/// reached us with `-11`, the prior worker either flipped already
/// (`rows_affected == 0` is fine — we don't surface as error) OR not
/// (we flip now — also fine).  The bare UPDATE is thus the simplest
/// shape; callers MUST treat `false` as "missing row" specifically,
/// not "already-blocked".  W5 acquire upserts the row before stage 1
/// runs, so missing-row at 4-b time is a structural breach.
///
/// Carries no `node_state_updated_at` bump — that column does not
/// exist (verified `migrations/001_core_identities.sql`); the change
/// is fully captured by the audit-log entry the caller writes
/// alongside.
pub async fn set_mode_blocked_tx(tx: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<bool> {
    let res = sqlx::query("UPDATE node_state SET mode = 'BLOCKED' WHERE fiscal_number = ?")
        .bind(fn_id)
        .execute(&mut **tx)
        .await?;
    Ok(res.rows_affected() == 1)
}

/// **M3b W12 Post-Closure Hardening Phase 2a.1 Commit 6.1.2 / REC-1
/// (2026-05-24)** — flip `node_state.mode` to `STOP_MODE` for the FN.
/// Idempotent CAS-free shape mirroring [`set_mode_blocked_tx`] (single
/// UPDATE; `rows_affected == 0` indicates missing FN row — structural
/// breach).
///
/// Invoked from `backlog_drain` Tier-2 degradation path when an FN
/// accumulates >= 50 consecutive HoldFnDrain outcomes на одному doc.
/// Effect: new чек ingress на цю FN rejected at adapter layer
/// (existing STOP_MODE contract per app.rs:373 + return_online_probe.rs:177);
/// existing held docs лишаються в Sent/Kvt1 з накопиченим counter для
/// auto-drain post-recovery.
///
/// **Operator runbook** (operator memory `feedback_manual_recon_
/// catastrophe`): STOP_MODE — це проміжний degradation tier, НЕ
/// эскалація в Manual.  Operator має 36h offline-cap window
/// (до cert.NotAfter-2160min) для intervention; за цей час можна
/// reset через admin CLI (Tier 3, Phase 2a.2 scope) АБО зачекати
/// auto-recovery через W8 return_online_probe.
///
/// Carries no `node_state_updated_at` bump (column does not exist);
/// change captured by paired `OFFLINE_DRAIN_FN_STOP_MODE` Critical
/// audit row written by caller inside the same `with_immediate`.
pub async fn set_mode_stop_mode_tx(tx: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<bool> {
    let res = sqlx::query("UPDATE node_state SET mode = 'STOP_MODE' WHERE fiscal_number = ?")
        .bind(fn_id)
        .execute(&mut **tx)
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
                  transport_profile_id,
                  next_z_report_number
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
        next_z_report_number: r.next_z_report_number,
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
                  transport_profile_id,
                  next_z_report_number
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
        next_z_report_number: r.next_z_report_number,
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

/// W6 / ADR-M3-A2 — atomically allocate and advance the FN's
/// `next_z_report_number`.
///
/// Mirrors [`allocate_next_lnd`] discipline (single-statement
/// `UPDATE … RETURNING`; bare CAS — the existing `node_state_updated_at`
/// trigger from migration 001 covers `updated_at`).  Caller (W6 stage
/// 3-PRE) MUST persist the returned value onto
/// `fiscal_documents.z_report_number` in the SAME `with_immediate`
/// envelope; on retry, stage 3-PRE reads the persisted value back and
/// does NOT call this allocator again.  Partial UNIQUE index
/// `ux_fd_fn_zrn` (migration 009) is the fail-closed downstream guard
/// against any drift.
///
/// Returns the allocated z_report_number; returns `RowNotFound` if the
/// FN row is missing (caller responsibility per ADR-M3-A7 / App::boot
/// invariants).
pub async fn allocate_z_report_number(tx: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<i64> {
    let allocated: i64 = sqlx::query_scalar(
        "UPDATE node_state SET next_z_report_number = next_z_report_number + 1 \
         WHERE fiscal_number = ? \
         RETURNING next_z_report_number - 1",
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
