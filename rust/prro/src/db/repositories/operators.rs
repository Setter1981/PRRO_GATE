//! W2 PR-B — repository for `operators` table (cashier EDS-key registry).
//!
//! Lives in the **secure** SQLite pool (`var/secure.db` per
//! `DatabaseCfg.secure_db_path`), distinct from the main ledger.  See
//! `migrations_secure/020_operators.sql` for the DDL.
//!
//! Repo policy mirrors the M1 reference shape (see
//! `fiscal_number_config.rs`): typed input struct on INSERT, typed
//! errors on conflict, plain `sqlx::query()` runtime-bound for ergonomic
//! BLOB binding (compile-time `sqlx::query!` is not applied here because
//! the secure pool is opened from a separate migration directory which
//! sqlx's `prepare`-based offline cache does not currently span).
//!
//! ## Typed-error contract
//!
//! `insert` distinguishes the structural "duplicate active cashier per
//! fiscal_number" violation (caught by the partial unique index
//! `operators_active_fn_uidx`) from generic database errors so callers
//! (admin CLI, future bindings registry) can render an operator-friendly
//! message rather than leaking raw sqlite text.

use sqlx::SqlitePool;
use thiserror::Error;

/// Snapshot of an `operators` row.
#[derive(Debug, Clone, PartialEq)]
pub struct OperatorRow {
    pub id: i64,
    pub operator_id: String,
    pub fiscal_number: String,
    pub name: String,
    pub key_path: String,
    pub key_pass_enc: Vec<u8>,
    pub is_active: bool,
    pub created_at: String,
}

/// Payload for [`insert`].  `is_active` is intentionally not exposed —
/// every fresh row starts active; rotation is a future-PR concern.
#[derive(Debug, Clone)]
pub struct NewOperator {
    pub operator_id: String,
    pub fiscal_number: String,
    pub name: String,
    pub key_path: String,
    pub key_pass_enc: Vec<u8>,
}

/// Typed errors surfaced by the repository.
#[derive(Debug, Error)]
pub enum OperatorsRepoError {
    /// Caught by the partial unique index `operators_active_fn_uidx
    /// WHERE is_active = 1`: an active cashier already exists for this
    /// fiscal_number.  Carry the FN in the variant so the CLI can
    /// render a deterministic "FN <X> already has an active cashier"
    /// message.
    #[error("duplicate active cashier for fiscal_number {0}")]
    DuplicateActive(String),

    /// Any other sqlx error (IO, type, etc.).  Boxed via `#[from]`.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Insert a new operator row.  `is_active` defaults to 1 (active) per
/// migration 020 DDL.  Returns [`OperatorsRepoError::DuplicateActive`]
/// when the partial unique index rejects a second active row for the
/// same fiscal_number.
pub async fn insert(pool: &SqlitePool, new: &NewOperator) -> Result<(), OperatorsRepoError> {
    let result = sqlx::query(
        "INSERT INTO operators \
            (operator_id, fiscal_number, name, key_path, key_pass_enc) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&new.operator_id)
    .bind(&new.fiscal_number)
    .bind(&new.name)
    .bind(&new.key_path)
    .bind(&new.key_pass_enc)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) if is_unique_violation(&e) => Err(OperatorsRepoError::DuplicateActive(
            new.fiscal_number.clone(),
        )),
        Err(e) => Err(OperatorsRepoError::Db(e)),
    }
}

/// Find the currently-active operator row for a given fiscal_number.
/// Returns `None` if no row matches (either no cashier ever registered
/// or all historical rows are `is_active = 0`).
pub async fn find_by_fiscal_number(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Option<OperatorRow>, OperatorsRepoError> {
    let row = sqlx::query_as::<_, OperatorRowRaw>(
        "SELECT id, operator_id, fiscal_number, name, key_path, \
                key_pass_enc, is_active, created_at \
         FROM operators \
         WHERE fiscal_number = ? AND is_active = 1 \
         LIMIT 1",
    )
    .bind(fn_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// List every operator row regardless of `is_active`.  Ordering is
/// unspecified at the SQL layer; callers must sort if order matters.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<OperatorRow>, OperatorsRepoError> {
    let rows = sqlx::query_as::<_, OperatorRowRaw>(
        "SELECT id, operator_id, fiscal_number, name, key_path, \
                key_pass_enc, is_active, created_at \
         FROM operators",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Raw sqlx row → `OperatorRow` conversion intermediate.  `is_active`
/// arrives as `i64`; convert to `bool` at the boundary.
#[derive(sqlx::FromRow)]
struct OperatorRowRaw {
    id: i64,
    operator_id: String,
    fiscal_number: String,
    name: String,
    key_path: String,
    key_pass_enc: Vec<u8>,
    is_active: i64,
    created_at: String,
}

impl From<OperatorRowRaw> for OperatorRow {
    fn from(r: OperatorRowRaw) -> Self {
        Self {
            id: r.id,
            operator_id: r.operator_id,
            fiscal_number: r.fiscal_number,
            name: r.name,
            key_path: r.key_path,
            key_pass_enc: r.key_pass_enc,
            is_active: r.is_active != 0,
            created_at: r.created_at,
        }
    }
}

/// Distinguish a sqlite UNIQUE constraint violation from other database
/// errors.  Used to map the partial-unique-index hit on
/// `(fiscal_number, is_active=1)` to [`OperatorsRepoError::DuplicateActive`].
///
/// Delegates to sqlx 0.8's typed [`sqlx::error::DatabaseError::is_unique_violation`]
/// rather than parsing extended-code strings — the typed API maps both
/// `SQLITE_CONSTRAINT_UNIQUE` (2067) and `SQLITE_CONSTRAINT_PRIMARYKEY`
/// (1555) to `ErrorKind::UniqueViolation`.  This module previously only
/// expected the UNIQUE flavor; the PRIMARYKEY case is also benign in
/// our schema (the surrogate `id INTEGER PRIMARY KEY` auto-allocates,
/// so a PK collision would itself be a "duplicate cashier" semantically).
fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_unique_violation())
}
