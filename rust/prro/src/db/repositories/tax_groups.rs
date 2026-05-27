//! W4-Z0 piece 2 — repository for `tax_groups` table (per-FN PDV/excise rates).
//!
//! Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
//! §1.1.  Lives in the **secure** SQLite pool (`var/secure.db`).  See
//! `migrations_secure/021_w4_z0_config_tables.sql` for the DDL.
//!
//! ## Typed-error contract
//!
//! `insert` distinguishes:
//!   * `DuplicateTxNum`         — PRIMARY KEY (fn, tx_num) violation
//!   * `DuplicateActiveLetter`  — partial unique idx on (fn, letter)
//!                                 WHERE is_active=1
//!
//! `update_rates` / `soft_delete` distinguish:
//!   * `NotFound`               — no matching (fn, tx_num) row
//!
//! Mirror W2 PR-B's `repositories/operators.rs` style — typed input
//! struct on INSERT, typed errors on conflict + not-found, plain
//! `sqlx::query()` runtime-bound (the secure pool's migration dir
//! is not in sqlx's offline cache).

use sqlx::SqlitePool;
use thiserror::Error;

/// Snapshot of a `tax_groups` row.
///
/// `fn_id` (not `fn`) because `fn` is a Rust reserved keyword.
#[derive(Debug, Clone, PartialEq)]
pub struct TaxGroup {
    pub fn_id: String,
    pub tx_num: i64,
    pub letter: String,
    pub dtpr: f64,
    pub txpr: f64,
    pub txal: i64,
    pub txty: i64,
    pub is_active: bool,
    pub created_at: String,
}

/// Payload for [`insert`].  `is_active` is intentionally not exposed —
/// every fresh row starts active per migration 021 default.  Soft-
/// delete is the only way to switch `is_active` (no rotation API).
#[derive(Debug, Clone)]
pub struct NewTaxGroup {
    pub fn_id: String,
    pub tx_num: i64,
    pub letter: String,
    pub dtpr: f64,
    pub txpr: f64,
    pub txal: i64,
    pub txty: i64,
}

/// Typed errors surfaced by the repository.
#[derive(Debug, Error)]
pub enum TaxGroupsRepoError {
    /// PRIMARY KEY (fn, tx_num) violation — there is already a row
    /// (active or soft-deleted) at this slot.  Re-using a slot
    /// requires a different `tx_num`, not soft-delete; for re-using
    /// the letter at a different `tx_num`, the partial-unique-index
    /// check on `letter` activates after soft-delete only.
    #[error("duplicate tx_num: fn={fn_id} tx_num={tx_num}")]
    DuplicateTxNum { fn_id: String, tx_num: i64 },

    /// Partial unique index `idx_tax_groups_fn_letter` violation —
    /// another *active* row for this FN already uses this letter.
    /// Soft-delete the existing row first to free the letter slot.
    #[error("duplicate active letter: fn={fn_id} letter={letter}")]
    DuplicateActiveLetter { fn_id: String, letter: String },

    /// No row at `(fn, tx_num)` — `update_rates` / `soft_delete`
    /// require an existing row.
    #[error("tax group not found: fn={fn_id} tx_num={tx_num}")]
    NotFound { fn_id: String, tx_num: i64 },

    /// Any other sqlx error.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Insert a new tax-group row.  `is_active` defaults to 1 per
/// migration 021 DDL.  Returns typed errors for PK and partial-
/// unique-index violations so callers can render an actionable
/// message rather than leaking raw sqlite text.
pub async fn insert(
    pool: &SqlitePool,
    new: &NewTaxGroup,
) -> Result<(), TaxGroupsRepoError> {
    let result = sqlx::query(
        "INSERT INTO tax_groups \
            (fn, tx_num, letter, dtpr, txpr, txal, txty) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&new.fn_id)
    .bind(new.tx_num)
    .bind(&new.letter)
    .bind(new.dtpr)
    .bind(new.txpr)
    .bind(new.txal)
    .bind(new.txty)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(classify_insert_error(e, &new.fn_id, new.tx_num, &new.letter)),
    }
}

/// Distinguish PRIMARY KEY (fn, tx_num) violation from partial unique
/// index (fn, letter) violation on the tax_groups table.  Both surface
/// as `UNIQUE constraint failed` in SQLite STRICT mode — we inspect
/// the message body for column names (`tx_num` vs `letter`) to route
/// to the appropriate typed error.
fn classify_insert_error(
    e: sqlx::Error,
    fn_id: &str,
    tx_num: i64,
    letter: &str,
) -> TaxGroupsRepoError {
    if let sqlx::Error::Database(ref db) = e {
        if db.is_unique_violation() {
            let msg = db.message();
            // sqlite emits column names in the failure message:
            //   PK collision: "UNIQUE constraint failed: tax_groups.fn,
            //                  tax_groups.tx_num"
            //   letter idx hit: "UNIQUE constraint failed: tax_groups.fn,
            //                    tax_groups.letter"
            if msg.contains("tx_num") {
                return TaxGroupsRepoError::DuplicateTxNum {
                    fn_id: fn_id.to_string(),
                    tx_num,
                };
            }
            if msg.contains("letter") {
                return TaxGroupsRepoError::DuplicateActiveLetter {
                    fn_id: fn_id.to_string(),
                    letter: letter.to_string(),
                };
            }
        }
    }
    TaxGroupsRepoError::Db(e)
}

/// Update mutable rate fields on an existing row (identified by
/// `(fn, tx_num)`).  `is_active` is NOT mutable here — use
/// [`soft_delete`] for that.  Returns `NotFound` if no row matches.
pub async fn update_rates(
    pool: &SqlitePool,
    fn_id: &str,
    tx_num: i64,
    dtpr: f64,
    txpr: f64,
    txal: i64,
    txty: i64,
) -> Result<(), TaxGroupsRepoError> {
    let result = sqlx::query(
        "UPDATE tax_groups \
            SET dtpr = ?, txpr = ?, txal = ?, txty = ? \
          WHERE fn = ? AND tx_num = ?",
    )
    .bind(dtpr)
    .bind(txpr)
    .bind(txal)
    .bind(txty)
    .bind(fn_id)
    .bind(tx_num)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(TaxGroupsRepoError::NotFound {
            fn_id: fn_id.to_string(),
            tx_num,
        });
    }
    Ok(())
}

/// Soft-delete a row by setting `is_active = 0`.  Frees the partial-
/// unique-index slot on `(fn, letter)` so a fresh row may re-use
/// the letter at a different `tx_num`.  Returns `NotFound` if no row
/// matches.
pub async fn soft_delete(
    pool: &SqlitePool,
    fn_id: &str,
    tx_num: i64,
) -> Result<(), TaxGroupsRepoError> {
    let result = sqlx::query(
        "UPDATE tax_groups SET is_active = 0 \
          WHERE fn = ? AND tx_num = ?",
    )
    .bind(fn_id)
    .bind(tx_num)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(TaxGroupsRepoError::NotFound {
            fn_id: fn_id.to_string(),
            tx_num,
        });
    }
    Ok(())
}

/// Lookup by `(fn, tx_num)` — returns row regardless of `is_active`
/// (soft-deleted rows still findable for audit / admin display).
/// Returns `None` if no row matches.
pub async fn find(
    pool: &SqlitePool,
    fn_id: &str,
    tx_num: i64,
) -> Result<Option<TaxGroup>, TaxGroupsRepoError> {
    let row = sqlx::query_as::<_, TaxGroupRowRaw>(
        "SELECT fn, tx_num, letter, dtpr, txpr, txal, txty, \
                is_active, created_at \
         FROM tax_groups \
         WHERE fn = ? AND tx_num = ? \
         LIMIT 1",
    )
    .bind(fn_id)
    .bind(tx_num)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Lookup by `(fn, letter)` — returns the **active** row only
/// (per the partial unique index there is at most one).  Returns
/// `None` if no active row matches.
pub async fn find_by_letter(
    pool: &SqlitePool,
    fn_id: &str,
    letter: &str,
) -> Result<Option<TaxGroup>, TaxGroupsRepoError> {
    let row = sqlx::query_as::<_, TaxGroupRowRaw>(
        "SELECT fn, tx_num, letter, dtpr, txpr, txal, txty, \
                is_active, created_at \
         FROM tax_groups \
         WHERE fn = ? AND letter = ? AND is_active = 1 \
         LIMIT 1",
    )
    .bind(fn_id)
    .bind(letter)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// List every active tax-group row for the given FN.  Ordering is
/// by `tx_num` ascending (deterministic for admin display).
pub async fn list_active_for_fn(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<TaxGroup>, TaxGroupsRepoError> {
    let rows = sqlx::query_as::<_, TaxGroupRowRaw>(
        "SELECT fn, tx_num, letter, dtpr, txpr, txal, txty, \
                is_active, created_at \
         FROM tax_groups \
         WHERE fn = ? AND is_active = 1 \
         ORDER BY tx_num ASC",
    )
    .bind(fn_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Raw sqlx row used as a structural mirror of the table; converted to
/// `TaxGroup` at the boundary so consumers see a typed `is_active: bool`.
#[derive(sqlx::FromRow)]
struct TaxGroupRowRaw {
    // SQL column is `fn` (reserved Rust keyword); Rust field renamed
    // via the sqlx attribute so the struct is ergonomic to use.
    #[sqlx(rename = "fn")]
    fn_id: String,
    tx_num: i64,
    letter: String,
    dtpr: f64,
    txpr: f64,
    txal: i64,
    txty: i64,
    is_active: i64,
    created_at: String,
}

impl From<TaxGroupRowRaw> for TaxGroup {
    fn from(r: TaxGroupRowRaw) -> Self {
        Self {
            fn_id: r.fn_id,
            tx_num: r.tx_num,
            letter: r.letter,
            dtpr: r.dtpr,
            txpr: r.txpr,
            txal: r.txal,
            txty: r.txty,
            is_active: r.is_active != 0,
            created_at: r.created_at,
        }
    }
}

