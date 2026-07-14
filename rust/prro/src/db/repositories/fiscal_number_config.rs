//! Repository for `fiscal_number_config`.
//!
//! Repo policy (this file is the M1 reference shape):
//! - `SELECT` decode goes through `sqlx::query!` so column types are
//!   verified against the schema at compile time (catches drift between
//!   migrations and Rust struct fields).
//! - `INSERT` / `UPDATE` mutations use runtime-bound `sqlx::query()`.
//!   Parameter types are already enforced by the `Encode<Sqlite>` impls
//!   on `FiscalMode` / id newtypes, so the compile-time payoff is small,
//!   and runtime SQL keeps multi-line statements ergonomic.

use crate::db::models::enums::FiscalMode;
use crate::db::types::DbFiscalMode;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct FnConfig {
    pub fiscal_number: String,
    pub tax_number: String,
    pub vat_payer_inn: Option<String>,
    pub fiscal_mode: FiscalMode,
    pub org_name: Option<String>,
    pub point_name: Option<String>,
    pub org_address: Option<String>,
    pub tsp_enabled: bool,
    pub offline_enabled: bool,
    /// Drives <L> tag injection in National Check submissions per old sidecar.
    pub national_check_enabled: bool,
    pub min_offline_codes: i64,
    pub max_offline_codes: i64,
}

#[derive(Debug, Clone)]
pub struct NewFnConfig {
    pub fiscal_number: String,
    pub tax_number: String,
    pub vat_payer_inn: Option<String>,
    pub fiscal_mode: FiscalMode,
    pub org_name: Option<String>,
    pub point_name: Option<String>,
    pub org_address: Option<String>,
    pub tsp_enabled: bool,
    pub offline_enabled: bool,
    pub national_check_enabled: bool,
    pub min_offline_codes: i64,
    pub max_offline_codes: i64,
}

pub async fn insert(pool: &SqlitePool, n: &NewFnConfig) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO fiscal_number_config (
             fiscal_number, tax_number, vat_payer_inn, fiscal_mode,
             org_name, point_name, org_address,
             tsp_enabled, offline_enabled, national_check_enabled,
             min_offline_codes, max_offline_codes
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&n.fiscal_number)
    .bind(&n.tax_number)
    .bind(n.vat_payer_inn.as_deref())
    .bind(n.fiscal_mode.as_str())
    .bind(n.org_name.as_deref())
    .bind(n.point_name.as_deref())
    .bind(n.org_address.as_deref())
    .bind(n.tsp_enabled as i64)
    .bind(n.offline_enabled as i64)
    .bind(n.national_check_enabled as i64)
    .bind(n.min_offline_codes)
    .bind(n.max_offline_codes)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Option<FnConfig>> {
    let row = sqlx::query!(
        r#"SELECT fiscal_number,
                  tax_number,
                  vat_payer_inn,
                  fiscal_mode    as "fiscal_mode: DbFiscalMode",
                  org_name, point_name, org_address,
                  tsp_enabled            as "tsp_enabled: i64",
                  offline_enabled        as "offline_enabled: i64",
                  national_check_enabled as "national_check_enabled: i64",
                  min_offline_codes      as "min_offline_codes: i64",
                  max_offline_codes      as "max_offline_codes: i64"
           FROM fiscal_number_config WHERE fiscal_number = ?"#,
        fn_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| FnConfig {
        fiscal_number: r.fiscal_number,
        tax_number: r.tax_number,
        vat_payer_inn: r.vat_payer_inn,
        fiscal_mode: r.fiscal_mode.0,
        org_name: r.org_name,
        point_name: r.point_name,
        org_address: r.org_address,
        tsp_enabled: r.tsp_enabled != 0,
        offline_enabled: r.offline_enabled != 0,
        national_check_enabled: r.national_check_enabled != 0,
        min_offline_codes: r.min_offline_codes,
        max_offline_codes: r.max_offline_codes,
    }))
}

pub async fn list_all(pool: &SqlitePool) -> sqlx::Result<Vec<FnConfig>> {
    let rows = sqlx::query!(
        r#"SELECT fiscal_number,
                  tax_number,
                  vat_payer_inn,
                  fiscal_mode    as "fiscal_mode: DbFiscalMode",
                  org_name, point_name, org_address,
                  tsp_enabled            as "tsp_enabled: i64",
                  offline_enabled        as "offline_enabled: i64",
                  national_check_enabled as "national_check_enabled: i64",
                  min_offline_codes      as "min_offline_codes: i64",
                  max_offline_codes      as "max_offline_codes: i64"
           FROM fiscal_number_config ORDER BY fiscal_number"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| FnConfig {
            fiscal_number: r.fiscal_number,
            tax_number: r.tax_number,
            vat_payer_inn: r.vat_payer_inn,
            fiscal_mode: r.fiscal_mode.0,
            org_name: r.org_name,
            point_name: r.point_name,
            org_address: r.org_address,
            tsp_enabled: r.tsp_enabled != 0,
            offline_enabled: r.offline_enabled != 0,
            national_check_enabled: r.national_check_enabled != 0,
            min_offline_codes: r.min_offline_codes,
            max_offline_codes: r.max_offline_codes,
        })
        .collect())
}

pub async fn update_org_metadata(
    pool: &SqlitePool,
    fn_id: &str,
    org_name: Option<&str>,
    point_name: Option<&str>,
    org_address: Option<&str>,
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE fiscal_number_config
         SET org_name = ?, point_name = ?, org_address = ?
         WHERE fiscal_number = ?",
    )
    .bind(org_name)
    .bind(point_name)
    .bind(org_address)
    .bind(fn_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}
