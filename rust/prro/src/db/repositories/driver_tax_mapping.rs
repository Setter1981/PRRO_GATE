//! W4-Z0 piece 4 — repository for `driver_tax_mapping` table.
//!
//! Per spec §1.3.  Translates each driver vendor's internal coding
//! scheme (`driver_number`) to the canonical TX number used in DPS
//! `<P TX="..." />`.  Keyed on `(driver_id, driver_number)` — NOT
//! per-FN scoped (vendor-level lookup, not operator-level).
//!
//! ## Usage flow
//!
//! 1. Driver registration installs a default 1:1 mapping for the
//!    vendor's known number range (Maria's 1-12 = canonical 1-12).
//! 2. If a vendor's scheme differs from canonical, operator adjusts
//!    individual rows via admin CLI `set-driver-mapping`.
//! 3. At conversion time (W4-Z1), the FSCO builder looks up
//!    `(listener.driver_id, cmd.tax_group_1)` → canonical_tx_num and
//!    emits `<P TX="canonical_tx_num" />`.
//!
//! `driver_letter` is an optional audit-only field — useful for admin
//! CLI display ("driver 4 = ГА") but not required for runtime.

use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct DriverTaxMapping {
    pub driver_id: String,
    pub driver_number: i64,
    pub driver_letter: Option<String>,
    pub canonical_tx_num: i64,
    pub is_active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewDriverTaxMapping {
    pub driver_id: String,
    pub driver_number: i64,
    pub driver_letter: Option<String>,
    pub canonical_tx_num: i64,
}

#[derive(Debug, Error)]
pub enum DriverTaxMappingRepoError {
    /// PRIMARY KEY (driver_id, driver_number) violation.  Note PK
    /// uniqueness is not scoped by `is_active`, so soft-deleted rows
    /// still occupy the slot.  Operator must use `update_canonical`
    /// for re-purpose, not re-insert.
    #[error("duplicate (driver_id, driver_number): driver_id={driver_id} driver_number={driver_number}")]
    DuplicatePk { driver_id: String, driver_number: i64 },

    #[error("driver_tax_mapping not found: driver_id={driver_id} driver_number={driver_number}")]
    NotFound { driver_id: String, driver_number: i64 },

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

pub async fn insert(
    pool: &SqlitePool,
    new: &NewDriverTaxMapping,
) -> Result<(), DriverTaxMappingRepoError> {
    let result = sqlx::query(
        "INSERT INTO driver_tax_mapping \
            (driver_id, driver_number, driver_letter, canonical_tx_num) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(&new.driver_id)
    .bind(new.driver_number)
    .bind(new.driver_letter.as_deref())
    .bind(new.canonical_tx_num)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) if is_unique_violation(&e) => Err(DriverTaxMappingRepoError::DuplicatePk {
            driver_id: new.driver_id.clone(),
            driver_number: new.driver_number,
        }),
        Err(e) => Err(DriverTaxMappingRepoError::Db(e)),
    }
}

pub async fn update_canonical(
    pool: &SqlitePool,
    driver_id: &str,
    driver_number: i64,
    canonical_tx_num: i64,
) -> Result<(), DriverTaxMappingRepoError> {
    let result = sqlx::query(
        "UPDATE driver_tax_mapping SET canonical_tx_num = ? \
          WHERE driver_id = ? AND driver_number = ?",
    )
    .bind(canonical_tx_num)
    .bind(driver_id)
    .bind(driver_number)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DriverTaxMappingRepoError::NotFound {
            driver_id: driver_id.to_string(),
            driver_number,
        });
    }
    Ok(())
}

pub async fn soft_delete(
    pool: &SqlitePool,
    driver_id: &str,
    driver_number: i64,
) -> Result<(), DriverTaxMappingRepoError> {
    let result = sqlx::query(
        "UPDATE driver_tax_mapping SET is_active = 0 \
          WHERE driver_id = ? AND driver_number = ?",
    )
    .bind(driver_id)
    .bind(driver_number)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DriverTaxMappingRepoError::NotFound {
            driver_id: driver_id.to_string(),
            driver_number,
        });
    }
    Ok(())
}

pub async fn find(
    pool: &SqlitePool,
    driver_id: &str,
    driver_number: i64,
) -> Result<Option<DriverTaxMapping>, DriverTaxMappingRepoError> {
    let row = sqlx::query_as::<_, DriverTaxMappingRowRaw>(
        "SELECT driver_id, driver_number, driver_letter, \
                canonical_tx_num, is_active, created_at \
         FROM driver_tax_mapping \
         WHERE driver_id = ? AND driver_number = ? LIMIT 1",
    )
    .bind(driver_id)
    .bind(driver_number)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn list_active_for_driver(
    pool: &SqlitePool,
    driver_id: &str,
) -> Result<Vec<DriverTaxMapping>, DriverTaxMappingRepoError> {
    let rows = sqlx::query_as::<_, DriverTaxMappingRowRaw>(
        "SELECT driver_id, driver_number, driver_letter, \
                canonical_tx_num, is_active, created_at \
         FROM driver_tax_mapping \
         WHERE driver_id = ? AND is_active = 1 \
         ORDER BY driver_number ASC",
    )
    .bind(driver_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[derive(sqlx::FromRow)]
struct DriverTaxMappingRowRaw {
    driver_id: String,
    driver_number: i64,
    driver_letter: Option<String>,
    canonical_tx_num: i64,
    is_active: i64,
    created_at: String,
}

impl From<DriverTaxMappingRowRaw> for DriverTaxMapping {
    fn from(r: DriverTaxMappingRowRaw) -> Self {
        Self {
            driver_id: r.driver_id,
            driver_number: r.driver_number,
            driver_letter: r.driver_letter,
            canonical_tx_num: r.canonical_tx_num,
            is_active: r.is_active != 0,
            created_at: r.created_at,
        }
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_unique_violation())
}
