//! W4-Z0 piece 5 — repository for `fn_integration_flags` table.
//!
//! Per spec §1.4.  Per-FN key-value flag store.  Upsert semantics:
//! `set_flag` overwrites the value + bumps `updated_at`; missing rows
//! get inserted.  No partial unique index; PK (fn, flag_name) is the
//! only constraint.
//!
//! Initial flag: `useecheckmegovua` (Національний чек, per
//! `feedback_operator_ua_fiscal_authority` + `StringXML.cs:1152`
//! WebCheck decompile).  Future flags use the same KV store.

use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct FnIntegrationFlag {
    pub fn_id: String,
    pub flag_name: String,
    pub flag_value: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Error)]
pub enum FnIntegrationFlagsRepoError {
    #[error("flag not found: fn={fn_id} flag={flag_name}")]
    NotFound { fn_id: String, flag_name: String },

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Upsert a flag value.  Inserts if (fn, flag_name) is new; updates
/// `flag_value` + bumps `updated_at` if exists.  Idempotent — calling
/// twice with the same value is a no-op on `flag_value` but does
/// refresh `updated_at`.
pub async fn set_flag(
    pool: &SqlitePool,
    fn_id: &str,
    flag_name: &str,
    flag_value: &str,
) -> Result<(), FnIntegrationFlagsRepoError> {
    sqlx::query(
        "INSERT INTO fn_integration_flags (fn, flag_name, flag_value, updated_at) \
         VALUES (?, ?, ?, CURRENT_TIMESTAMP) \
         ON CONFLICT(fn, flag_name) DO UPDATE SET \
             flag_value = excluded.flag_value, \
             updated_at = CURRENT_TIMESTAMP",
    )
    .bind(fn_id)
    .bind(flag_name)
    .bind(flag_value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_flag(
    pool: &SqlitePool,
    fn_id: &str,
    flag_name: &str,
) -> Result<Option<String>, FnIntegrationFlagsRepoError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT flag_value FROM fn_integration_flags \
         WHERE fn = ? AND flag_name = ? LIMIT 1",
    )
    .bind(fn_id)
    .bind(flag_name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(v,)| v))
}

pub async fn delete_flag(
    pool: &SqlitePool,
    fn_id: &str,
    flag_name: &str,
) -> Result<(), FnIntegrationFlagsRepoError> {
    let result = sqlx::query(
        "DELETE FROM fn_integration_flags WHERE fn = ? AND flag_name = ?",
    )
    .bind(fn_id)
    .bind(flag_name)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(FnIntegrationFlagsRepoError::NotFound {
            fn_id: fn_id.to_string(),
            flag_name: flag_name.to_string(),
        });
    }
    Ok(())
}

pub async fn list_flags_for_fn(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<FnIntegrationFlag>, FnIntegrationFlagsRepoError> {
    let rows = sqlx::query_as::<_, FnIntegrationFlagRowRaw>(
        "SELECT fn, flag_name, flag_value, created_at, updated_at \
         FROM fn_integration_flags \
         WHERE fn = ? \
         ORDER BY flag_name ASC",
    )
    .bind(fn_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[derive(sqlx::FromRow)]
struct FnIntegrationFlagRowRaw {
    #[sqlx(rename = "fn")]
    fn_id: String,
    flag_name: String,
    flag_value: String,
    created_at: String,
    updated_at: String,
}

impl From<FnIntegrationFlagRowRaw> for FnIntegrationFlag {
    fn from(r: FnIntegrationFlagRowRaw) -> Self {
        Self {
            fn_id: r.fn_id,
            flag_name: r.flag_name,
            flag_value: r.flag_value,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
