//! Repository for `chain_seed_transitions` (migration 040).
//!
//! Durable witness for NON-DOCUMENT MAC-chain seed advances — currently only the
//! standalone T=112 offline-code replenish, which advances the seed to
//! `Hs = sha256(request_xml)` with NO producing `fiscal_documents` row.  The shared
//! ledger-walk projection (`fiscal_documents::active_chain_tip_unsigned_xml_sha256`)
//! folds the latest witness in so all three seed consumers (NC-03 boot, MacReseed
//! guard-B, invariant_scan) recover `Hs` — mirroring exactly how bd PRRO_GATE-2nk
//! folded the `chain_superseded_at` rewind marker into that same projection.  bd
//! PRRO_GATE-hpc.
//!
//! Repo policy: runtime-bound queries (`sqlx::query` / `query_as`) — the param and
//! row types are covered by `Encode<Sqlite>` / `FromRow`, so no `.sqlx` offline
//! metadata is required (matches the INSERT/UPDATE runtime-bound convention of the
//! sibling repos).
//!
//! Append-only: [`insert_seed_transition_tx`] is the only writer; there is no UPDATE
//! or DELETE.  A later doc/advance simply appends a higher `lnd_at_write`.

use crate::db::tx::WriteTxConn;

/// Provenance discriminator for the sole writer in this slice (standalone T=112
/// replenish).  Future non-doc advances (e.g. operator MacReseed) would append their
/// own source string.
pub const SOURCE_T112: &str = "T112";

/// Append one durable seed-transition witness inside the caller's `with_immediate`
/// envelope (tx-bound — the only way to obtain a `WriteTxConn`).
///
/// **Atomicity contract.**  The caller (offline_code_replenish) MUST invoke this in
/// the SAME `with_immediate` as `node_state::update_last_known_xml_sha_tx` +
/// `insert_dps_codes_tx`, so there is no window where the seed advanced but the
/// witness did not (or vice versa) — the whole point of the witness.
///
/// **Ordering-frame contract (Frozen invariant #2 — load-bearing).**  `lnd_at_write`
/// MUST be the FN's current `node_state.next_lnd` read INSIDE this same tx.  Because
/// the replenish holds `acquire_fn_gate` (single-writer per FN), no document can
/// interleave the read-of-next_lnd and this insert, so the witness sits in the same
/// strictly-monotonic per-FN frame as `fiscal_documents.lnd`.  If a doc could
/// interleave, the ordinal frame — and thus the §4.2 tie-break — would break.
///
/// `new_seed` is the 32-byte sha256 the transition installed.  `source` is the
/// provenance discriminator (see [`SOURCE_T112`]).
pub async fn insert_seed_transition_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    lnd_at_write: i64,
    new_seed: &[u8; 32],
    source: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO chain_seed_transitions (fiscal_number, lnd_at_write, new_seed, source) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(fiscal_number)
    .bind(lnd_at_write)
    .bind(&new_seed[..])
    .bind(source)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Read the newest seed-transition witness for `fiscal_number`, or `None` if the FN
/// has never had a non-doc seed advance.
///
/// Returns `(new_seed, lnd_at_write)`.  Newest-first ordering is
/// `lnd_at_write DESC, created_at DESC` (the index `ix_chain_seed_transitions_fn_lnd`
/// covers the leading key): a later replenish always records a `lnd_at_write` >= an
/// earlier one under the single-writer frame, and `created_at DESC` breaks any
/// same-ordinal tie deterministically toward the most-recently-written row.
///
/// The `new_seed` BLOB length is not re-validated here (schema stores exactly the
/// 32 bytes [`insert_seed_transition_tx`] wrote); the projection's downstream
/// `node_state` decode path is the fail-closed length guard for the recovered seed.
pub async fn latest_seed_transition<'e, E>(
    executor: E,
    fiscal_number: &str,
) -> sqlx::Result<Option<(Vec<u8>, i64)>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row: Option<(Vec<u8>, i64)> = sqlx::query_as(
        "SELECT new_seed, lnd_at_write FROM chain_seed_transitions \
         WHERE fiscal_number = ? \
         ORDER BY lnd_at_write DESC, created_at DESC, rowid DESC \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}
