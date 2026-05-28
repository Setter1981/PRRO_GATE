//! W4-Z2a piece 3 — signing_config_snapshots repository.
//!
//! Append-only main-pool ledger of pinned tax-resolution snapshots,
//! content-addressable via `(fn, driver_id, payload_sha256)` UNIQUE.
//! See migration 020 + memory `project_m4_w4_z2a_locked_design`.
//!
//! ## Race safety
//!
//! Two concurrent stage_acquire calls on the same FN with identical
//! config produce identical `payload_sha256`.  `insert_or_get_id`
//! uses `INSERT OR IGNORE` + `SELECT` — both callers race the
//! INSERT, one wins (UNIQUE), the loser's SELECT returns the
//! winner's id.  No duplicate row, no error surfaced.
//!
//! ## SHA256 verify on read
//!
//! `get_by_id` recomputes `SHA256(payload_json's canonical bytes)`
//! and compares to stored `payload_sha256`.  Mismatch → typed
//! `ChecksumMismatch` error.  Defensive against disk corruption /
//! out-of-band UPDATE / migration bug.  Fiscal compliance demands
//! fail-loud on integrity issues.

use sqlx::SqlitePool;
use thiserror::Error;

use crate::db::tx::WriteTxConn;
use crate::services::write_path::tax_summary::TaxResolutionSnapshot;

/// Errors surfaced by the repository.
#[derive(Debug, Error)]
pub enum SigningConfigSnapshotsRepoError {
    #[error("signing_config_snapshots: row not found for id={id}")]
    NotFound { id: i64 },

    /// Stored `payload_sha256` does not match recomputed hash of
    /// `payload_json`.  Indicates disk corruption, out-of-band
    /// UPDATE, or migration bug.  Caller MUST treat as critical
    /// (audit_log Critical + route doc to RequiresManualReconciliation).
    #[error("signing_config_snapshots: payload_sha256 mismatch for id={id} \
             (stored={stored_hex}, recomputed={recomputed_hex})")]
    ChecksumMismatch {
        id: i64,
        stored_hex: String,
        recomputed_hex: String,
    },

    /// JSON deserialize failed.  Either an unreadable row OR
    /// schema-bump rolled out without backfill.  Same recovery
    /// posture as ChecksumMismatch.
    #[error("signing_config_snapshots: payload deserialize failed for id={id}: {source}")]
    DeserializeFailed {
        id: i64,
        #[source]
        source: serde_json::Error,
    },

    /// External mid-review IMP-3 — `kind` field in payload is not
    /// the current `KIND_V1` constant.  Either a future schema bump
    /// reached this row before the corresponding parser landed, OR
    /// out-of-band INSERT injected non-V1 content via struct-literal
    /// (which is now `pub(crate)`, but defense-in-depth).  Caller
    /// MUST route to RequiresManualReconciliation.
    #[error("signing_config_snapshots: unsupported kind for id={id}: expected={expected}, found={found}")]
    UnsupportedKind {
        id: i64,
        expected: String,
        found: String,
    },

    #[error("signing_config_snapshots: database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Insert a new snapshot row OR return the id of an existing row
/// matching `(fn, driver_id, snapshot.sha256())`.  Race-safe via
/// UNIQUE constraint + INSERT OR IGNORE.
///
/// `payload_json` is `snapshot.canonical_bytes()`; `payload_sha256`
/// is `snapshot.sha256()` — both computed in-process so caller
/// trusts the same canonicalization that `get_by_id` will verify.
pub async fn insert_or_get_id(
    pool: &SqlitePool,
    fn_id: &str,
    driver_id: &str,
    snapshot: &TaxResolutionSnapshot,
) -> Result<i64, SigningConfigSnapshotsRepoError> {
    let canonical = snapshot.canonical_bytes();
    let payload_json = std::str::from_utf8(&canonical)
        .expect("canonical_bytes is ASCII-safe JSON");
    let sha256 = snapshot.sha256();

    // Step 1: INSERT OR IGNORE — wins the race or no-op on conflict.
    sqlx::query(
        "INSERT OR IGNORE INTO signing_config_snapshots \
            (fn, driver_id, kind, payload_json, payload_sha256) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(fn_id)
    .bind(driver_id)
    .bind(&snapshot.kind)
    .bind(payload_json)
    .bind(sha256.as_slice())
    .execute(pool)
    .await?;

    // Step 2: SELECT — winner OR loser both get the canonical id.
    let id: i64 = sqlx::query_scalar(
        "SELECT id FROM signing_config_snapshots \
         WHERE fn = ? AND driver_id = ? AND payload_sha256 = ?",
    )
    .bind(fn_id)
    .bind(driver_id)
    .bind(sha256.as_slice())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// W4-Z2a — tx-bound variant of [`insert_or_get_id`].  Same content-
/// addressable semantics, but participates in the caller's
/// `with_immediate` envelope so the resulting `id` is atomic with
/// other writes (lease + fiscal_documents INSERT in stage_acquire).
/// Race safety still via UNIQUE constraint.
pub async fn insert_or_get_id_tx(
    tx: &mut WriteTxConn<'_>,
    fn_id: &str,
    driver_id: &str,
    snapshot: &TaxResolutionSnapshot,
) -> Result<i64, SigningConfigSnapshotsRepoError> {
    let canonical = snapshot.canonical_bytes();
    let payload_json = std::str::from_utf8(&canonical)
        .expect("canonical_bytes is ASCII-safe JSON");
    let sha256 = snapshot.sha256();

    sqlx::query(
        "INSERT OR IGNORE INTO signing_config_snapshots \
            (fn, driver_id, kind, payload_json, payload_sha256) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(fn_id)
    .bind(driver_id)
    .bind(&snapshot.kind)
    .bind(payload_json)
    .bind(sha256.as_slice())
    .execute(&mut **tx)
    .await?;

    let id: i64 = sqlx::query_scalar(
        "SELECT id FROM signing_config_snapshots \
         WHERE fn = ? AND driver_id = ? AND payload_sha256 = ?",
    )
    .bind(fn_id)
    .bind(driver_id)
    .bind(sha256.as_slice())
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// Fetch snapshot by id, deserialize payload, and verify
/// SHA256(canonical_bytes(deserialized)) == stored payload_sha256.
///
/// Mismatch → `ChecksumMismatch` typed error.  Caller (stage_sign
/// or MAC recovery) MUST fail-closed.
pub async fn get_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<TaxResolutionSnapshot, SigningConfigSnapshotsRepoError> {
    let row = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT payload_json, payload_sha256 \
         FROM signing_config_snapshots WHERE id = ? LIMIT 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    verify_and_decode(id, row)
}

/// W4-Z2a piece 14 — tx-bound variant for callers already inside a
/// `with_immediate` envelope (e.g., `stage_acquire` Resume branch).
/// Re-uses the writer-tx connection instead of acquiring a separate
/// pool connection (avoids pool-contention deadlock potential under
/// single-writer model + pool-size-of-N constraints).  Same sha256
/// verify + V1 kind validation as [`get_by_id`].
pub async fn get_by_id_tx(
    tx: &mut WriteTxConn<'_>,
    id: i64,
) -> Result<TaxResolutionSnapshot, SigningConfigSnapshotsRepoError> {
    let row = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT payload_json, payload_sha256 \
         FROM signing_config_snapshots WHERE id = ? LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    verify_and_decode(id, row)
}

/// Shared deserialise + V1 kind + sha256 verify logic for both
/// pool-bound and tx-bound read variants.
fn verify_and_decode(
    id: i64,
    row: Option<(String, Vec<u8>)>,
) -> Result<TaxResolutionSnapshot, SigningConfigSnapshotsRepoError> {
    let (payload_json, stored_sha256) = match row {
        Some(r) => r,
        None => return Err(SigningConfigSnapshotsRepoError::NotFound { id }),
    };

    let snapshot: TaxResolutionSnapshot = serde_json::from_str(&payload_json)
        .map_err(|source| SigningConfigSnapshotsRepoError::DeserializeFailed { id, source })?;

    // External mid-review IMP-3 — validate kind == KIND_V1 BEFORE
    // accepting the snapshot.  Defense-in-depth: future schema bumps
    // would land alongside their own parser; until then any non-V1
    // row is fail-loud.
    if !snapshot.is_v1() {
        return Err(SigningConfigSnapshotsRepoError::UnsupportedKind {
            id,
            expected: TaxResolutionSnapshot::KIND_V1.to_string(),
            found: snapshot.kind().to_string(),
        });
    }

    let recomputed = snapshot.sha256();
    if recomputed.as_slice() != stored_sha256.as_slice() {
        return Err(SigningConfigSnapshotsRepoError::ChecksumMismatch {
            id,
            stored_hex: stored_sha256.iter().map(|b| format!("{b:02x}")).collect(),
            recomputed_hex: recomputed.iter().map(|b| format!("{b:02x}")).collect(),
        });
    }
    Ok(snapshot)
}
