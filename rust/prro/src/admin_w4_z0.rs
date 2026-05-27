//! W4-Z0 piece 8 — admin CLI library functions for per-FN config tables.
//!
//! Per spec §3.  16 commands grouped into 5 families:
//!
//!   * tax_groups (4)       — add / update_rate / remove / list
//!   * payment_methods (4)  — add / update / remove / list
//!   * integration_flags (3)— set / set_national_receipt alias / list
//!   * driver_tax_mapping (3) — add / remove / list
//!   * fn_outgress_profile (2) — set / show
//!
//! Mirrors W2 PR-B `admin::add_operator` style — typed input, typed
//! errors, audit_log emission on mutations.  Read-only commands
//! (list_*, show_*) do NOT emit audit.
//!
//! Audit event taxonomy (Info severity unless noted):
//!   ADMIN_TAX_GROUP_ADDED / UPDATED / REMOVED
//!   ADMIN_PAYMENT_METHOD_ADDED / UPDATED / REMOVED
//!   ADMIN_FLAG_SET
//!   ADMIN_DRIVER_MAPPING_ADDED / REMOVED
//!   ADMIN_OUTGRESS_PROFILE_SET
//!
//! Three-event protocol (W2 PR-B style ATTEMPTED → REGISTERED|FAILED)
//! is NOT applied here — these are smaller-blast-radius config-table
//! commands, not operator+key registration.  Single audit event on
//! successful mutation; failures surface as typed errors at the CLI
//! boundary.  Per spec §3 "audit-log entries per W2 PR-B pattern" is
//! satisfied by the per-mutation Info row; review criteria item 5
//! reaffirms three-event-where-applicable.

use sqlx::SqlitePool;
use thiserror::Error;

use crate::db::repositories::driver_tax_mapping::{
    self as dtm_repo, DriverTaxMapping, DriverTaxMappingRepoError, NewDriverTaxMapping,
};
use crate::db::repositories::fn_integration_flags::{
    self as flags_repo, FnIntegrationFlag, FnIntegrationFlagsRepoError,
};
use crate::db::repositories::fn_outgress_profile::{
    self as profile_repo, FnOutgressProfileRepoError, OutgressProfile,
};
use crate::db::repositories::payment_methods::{
    self as pm_repo, NewPaymentMethod, PaymentMethod, PaymentMethodsRepoError,
};
use crate::db::repositories::tax_groups::{
    self as tg_repo, NewTaxGroup, TaxGroup, TaxGroupsRepoError,
};

/// Typed errors for the W4-Z0 admin command surface.  Distinct from
/// `crate::admin::AdminError` (W2) so the CLI dispatch can map each
/// to its own sysexits code; the variants are similar in spirit but
/// scoped to per-FN config tables.
#[derive(Debug, Error)]
pub enum CfgAdminError {
    #[error("admin(w4-z0): --{0} MUST be a non-empty value")]
    EmptyArgument(&'static str),

    #[error("admin(w4-z0): fiscal_number {0:?} not found in fiscal_number_config")]
    FiscalNumberNotInConfig(String),

    #[error("admin(w4-z0): tax_group already exists: fn={0} tx_num={1}")]
    DuplicateTaxGroup(String, i64),

    #[error("admin(w4-z0): tax_group letter already in use: fn={0} letter={1}")]
    DuplicateActiveLetter(String, String),

    #[error("admin(w4-z0): tax_group not found: fn={0} tx_num={1}")]
    TaxGroupNotFound(String, i64),

    #[error("admin(w4-z0): payment_method already exists: fn={0} pay_index={1}")]
    DuplicatePaymentMethod(String, i64),

    #[error("admin(w4-z0): payment_method name already in use: fn={0} name={1}")]
    DuplicatePaymentName(String, String),

    #[error("admin(w4-z0): payment_method not found: fn={0} pay_index={1}")]
    PaymentMethodNotFound(String, i64),

    #[error("admin(w4-z0): flag not found: fn={0} name={1}")]
    FlagNotFound(String, String),

    #[error("admin(w4-z0): driver_tax_mapping already exists: driver_id={0} driver_number={1}")]
    DuplicateDriverMapping(String, i64),

    #[error("admin(w4-z0): driver_tax_mapping not found: driver_id={0} driver_number={1}")]
    DriverMappingNotFound(String, i64),

    #[error("admin(w4-z0): unknown outgress profile {0:?} (expected FSCO_ZZD or EVPZ_DPS)")]
    UnknownProfile(String),

    #[error("admin(w4-z0): outgress_profile not found: fn={0}")]
    OutgressProfileNotFound(String),

    #[error("admin(w4-z0): infrastructure: {0}")]
    Infrastructure(String),
}

impl CfgAdminError {
    /// BSD sysexits-aligned process exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            CfgAdminError::Infrastructure(_) => 66, // EX_NOINPUT
            _ => 64,                                // EX_USAGE
        }
    }
}

// ─── Helpers ────────────────────────────────────────────────────────

async fn ensure_fn_in_config(
    pool_main: &SqlitePool,
    fn_id: &str,
) -> Result<(), CfgAdminError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT fiscal_number FROM fiscal_number_config WHERE fiscal_number = ?",
    )
    .bind(fn_id)
    .fetch_optional(pool_main)
    .await
    .map_err(|e| CfgAdminError::Infrastructure(format!("FN existence check: {e}")))?;
    if row.is_none() {
        return Err(CfgAdminError::FiscalNumberNotInConfig(fn_id.to_string()));
    }
    Ok(())
}

async fn audit_info(
    pool_main: &SqlitePool,
    domain: &str,
    fn_id: &str,
    event_type: &str,
    payload_json: &str,
) -> Result<(), CfgAdminError> {
    crate::db::repositories::audit_log::append(
        pool_main,
        domain,
        fn_id,
        event_type,
        crate::db::models::enums::Severity::Info,
        None,
        Some(payload_json),
    )
    .await
    .map_err(|e| CfgAdminError::Infrastructure(format!("audit append {event_type}: {e}")))?;
    Ok(())
}

// ─── tax_groups commands ────────────────────────────────────────────

pub async fn add_tax_group(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    tx_num: i64,
    letter: &str,
    dtpr: f64,
    txpr: f64,
    txal: i64,
) -> Result<(), CfgAdminError> {
    if fn_id.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("fn"));
    }
    if letter.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("letter"));
    }
    ensure_fn_in_config(pool_main, fn_id).await?;

    match tg_repo::insert(
        pool_secure,
        &NewTaxGroup {
            fn_id: fn_id.to_string(),
            tx_num,
            letter: letter.to_string(),
            dtpr,
            txpr,
            txal,
            txty: 0,
        },
    )
    .await
    {
        Ok(()) => {
            let payload = serde_json::json!({
                "fn": fn_id,
                "tx_num": tx_num,
                "letter": letter,
                "dtpr": dtpr,
                "txpr": txpr,
                "txal": txal,
            })
            .to_string();
            audit_info(pool_main, "tax_group", fn_id, "ADMIN_TAX_GROUP_ADDED", &payload).await?;
            Ok(())
        }
        Err(TaxGroupsRepoError::DuplicateTxNum { fn_id, tx_num }) => {
            Err(CfgAdminError::DuplicateTaxGroup(fn_id, tx_num))
        }
        Err(TaxGroupsRepoError::DuplicateActiveLetter { fn_id, letter }) => {
            Err(CfgAdminError::DuplicateActiveLetter(fn_id, letter))
        }
        Err(e) => Err(CfgAdminError::Infrastructure(format!("tax_groups::insert: {e}"))),
    }
}

/// Update mutable rate fields.  Optional params: only fields that
/// caller passed `Some(_)` get changed; others retain current value.
pub async fn update_tax_rate(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    tx_num: i64,
    dtpr: Option<f64>,
    txpr: Option<f64>,
    txal: Option<i64>,
) -> Result<(), CfgAdminError> {
    let current = tg_repo::find(pool_secure, fn_id, tx_num)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("tax_groups::find: {e}")))?
        .ok_or_else(|| CfgAdminError::TaxGroupNotFound(fn_id.to_string(), tx_num))?;

    let new_dtpr = dtpr.unwrap_or(current.dtpr);
    let new_txpr = txpr.unwrap_or(current.txpr);
    let new_txal = txal.unwrap_or(current.txal);

    tg_repo::update_rates(pool_secure, fn_id, tx_num, new_dtpr, new_txpr, new_txal, current.txty)
        .await
        .map_err(|e| match e {
            TaxGroupsRepoError::NotFound { fn_id, tx_num } => {
                CfgAdminError::TaxGroupNotFound(fn_id, tx_num)
            }
            other => CfgAdminError::Infrastructure(format!("tax_groups::update_rates: {other}")),
        })?;

    let payload = serde_json::json!({
        "fn": fn_id, "tx_num": tx_num,
        "dtpr": new_dtpr, "txpr": new_txpr, "txal": new_txal,
        "previous": { "dtpr": current.dtpr, "txpr": current.txpr, "txal": current.txal },
    })
    .to_string();
    audit_info(pool_main, "tax_group", fn_id, "ADMIN_TAX_GROUP_UPDATED", &payload).await?;
    Ok(())
}

pub async fn remove_tax_group(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    tx_num: i64,
) -> Result<(), CfgAdminError> {
    tg_repo::soft_delete(pool_secure, fn_id, tx_num).await.map_err(|e| match e {
        TaxGroupsRepoError::NotFound { fn_id, tx_num } => {
            CfgAdminError::TaxGroupNotFound(fn_id, tx_num)
        }
        other => CfgAdminError::Infrastructure(format!("tax_groups::soft_delete: {other}")),
    })?;

    let payload = serde_json::json!({ "fn": fn_id, "tx_num": tx_num }).to_string();
    audit_info(pool_main, "tax_group", fn_id, "ADMIN_TAX_GROUP_REMOVED", &payload).await?;
    Ok(())
}

pub async fn list_tax_groups(
    pool_secure: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<TaxGroup>, CfgAdminError> {
    tg_repo::list_active_for_fn(pool_secure, fn_id)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("tax_groups::list: {e}")))
}

// ─── payment_methods commands ──────────────────────────────────────

pub async fn add_payment_method(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    pay_index: i64,
    name: &str,
    iscash: bool,
) -> Result<(), CfgAdminError> {
    if fn_id.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("fn"));
    }
    if name.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("name"));
    }
    ensure_fn_in_config(pool_main, fn_id).await?;

    match pm_repo::insert(
        pool_secure,
        &NewPaymentMethod {
            fn_id: fn_id.to_string(),
            pay_index,
            name: name.to_string(),
            iscash,
        },
    )
    .await
    {
        Ok(()) => {
            let payload = serde_json::json!({
                "fn": fn_id, "pay_index": pay_index, "name": name, "iscash": iscash,
            })
            .to_string();
            audit_info(pool_main, "payment_method", fn_id, "ADMIN_PAYMENT_METHOD_ADDED", &payload).await
        }
        Err(PaymentMethodsRepoError::DuplicatePayIndex { fn_id, pay_index }) => {
            Err(CfgAdminError::DuplicatePaymentMethod(fn_id, pay_index))
        }
        Err(PaymentMethodsRepoError::DuplicateActiveName { fn_id, name }) => {
            Err(CfgAdminError::DuplicatePaymentName(fn_id, name))
        }
        Err(e) => Err(CfgAdminError::Infrastructure(format!("payment_methods::insert: {e}"))),
    }
}

pub async fn update_payment_method(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    pay_index: i64,
    name: Option<&str>,
    iscash: Option<bool>,
) -> Result<(), CfgAdminError> {
    let current = pm_repo::find(pool_secure, fn_id, pay_index)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("payment_methods::find: {e}")))?
        .ok_or_else(|| CfgAdminError::PaymentMethodNotFound(fn_id.to_string(), pay_index))?;

    let new_name = name.unwrap_or(&current.name).to_string();
    let new_iscash = iscash.unwrap_or(current.iscash);

    pm_repo::update(pool_secure, fn_id, pay_index, &new_name, new_iscash).await.map_err(|e| match e {
        PaymentMethodsRepoError::NotFound { fn_id, pay_index } => {
            CfgAdminError::PaymentMethodNotFound(fn_id, pay_index)
        }
        PaymentMethodsRepoError::DuplicateActiveName { fn_id, name } => {
            CfgAdminError::DuplicatePaymentName(fn_id, name)
        }
        other => CfgAdminError::Infrastructure(format!("payment_methods::update: {other}")),
    })?;

    let payload = serde_json::json!({
        "fn": fn_id, "pay_index": pay_index,
        "name": new_name, "iscash": new_iscash,
        "previous": { "name": current.name, "iscash": current.iscash },
    })
    .to_string();
    audit_info(pool_main, "payment_method", fn_id, "ADMIN_PAYMENT_METHOD_UPDATED", &payload).await
}

pub async fn remove_payment_method(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    pay_index: i64,
) -> Result<(), CfgAdminError> {
    pm_repo::soft_delete(pool_secure, fn_id, pay_index).await.map_err(|e| match e {
        PaymentMethodsRepoError::NotFound { fn_id, pay_index } => {
            CfgAdminError::PaymentMethodNotFound(fn_id, pay_index)
        }
        other => CfgAdminError::Infrastructure(format!("payment_methods::soft_delete: {other}")),
    })?;
    let payload = serde_json::json!({ "fn": fn_id, "pay_index": pay_index }).to_string();
    audit_info(pool_main, "payment_method", fn_id, "ADMIN_PAYMENT_METHOD_REMOVED", &payload).await
}

pub async fn list_payment_methods(
    pool_secure: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<PaymentMethod>, CfgAdminError> {
    pm_repo::list_active_for_fn(pool_secure, fn_id)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("payment_methods::list: {e}")))
}

// ─── integration_flags commands ────────────────────────────────────

pub async fn set_flag(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    name: &str,
    value: &str,
) -> Result<(), CfgAdminError> {
    if fn_id.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("fn"));
    }
    if name.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("name"));
    }
    ensure_fn_in_config(pool_main, fn_id).await?;

    flags_repo::set_flag(pool_secure, fn_id, name, value).await.map_err(|e| match e {
        FnIntegrationFlagsRepoError::Db(e) => {
            CfgAdminError::Infrastructure(format!("flags::set_flag: {e}"))
        }
        other => CfgAdminError::Infrastructure(format!("flags::set_flag: {other}")),
    })?;

    let payload = serde_json::json!({ "fn": fn_id, "name": name, "value": value }).to_string();
    audit_info(pool_main, "integration_flag", fn_id, "ADMIN_FLAG_SET", &payload).await
}

/// Convenience alias for the Національний чек integration toggle.
pub async fn set_national_receipt(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    enabled: bool,
) -> Result<(), CfgAdminError> {
    set_flag(
        pool_main,
        pool_secure,
        fn_id,
        "useecheckmegovua",
        if enabled { "1" } else { "0" },
    )
    .await
}

pub async fn list_flags(
    pool_secure: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<FnIntegrationFlag>, CfgAdminError> {
    flags_repo::list_flags_for_fn(pool_secure, fn_id)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("flags::list: {e}")))
}

// ─── driver_tax_mapping commands ───────────────────────────────────

pub async fn add_driver_mapping(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    driver_id: &str,
    driver_number: i64,
    canonical_tx_num: i64,
    driver_letter: Option<&str>,
) -> Result<(), CfgAdminError> {
    if driver_id.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("driver-id"));
    }

    match dtm_repo::insert(
        pool_secure,
        &NewDriverTaxMapping {
            driver_id: driver_id.to_string(),
            driver_number,
            driver_letter: driver_letter.map(String::from),
            canonical_tx_num,
        },
    )
    .await
    {
        Ok(()) => {
            let payload = serde_json::json!({
                "driver_id": driver_id,
                "driver_number": driver_number,
                "driver_letter": driver_letter,
                "canonical_tx_num": canonical_tx_num,
            })
            .to_string();
            // Use driver_id as audit "fiscal_number" surrogate (driver_tax_mapping
            // is not per-FN; we record the driver scope in the payload).
            audit_info(pool_main, "driver_mapping", driver_id, "ADMIN_DRIVER_MAPPING_ADDED", &payload).await
        }
        Err(DriverTaxMappingRepoError::DuplicatePk { driver_id, driver_number }) => {
            Err(CfgAdminError::DuplicateDriverMapping(driver_id, driver_number))
        }
        Err(e) => Err(CfgAdminError::Infrastructure(format!("driver_tax_mapping::insert: {e}"))),
    }
}

pub async fn remove_driver_mapping(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    driver_id: &str,
    driver_number: i64,
) -> Result<(), CfgAdminError> {
    dtm_repo::soft_delete(pool_secure, driver_id, driver_number).await.map_err(|e| match e {
        DriverTaxMappingRepoError::NotFound { driver_id, driver_number } => {
            CfgAdminError::DriverMappingNotFound(driver_id, driver_number)
        }
        other => CfgAdminError::Infrastructure(format!("driver_tax_mapping::soft_delete: {other}")),
    })?;
    let payload = serde_json::json!({
        "driver_id": driver_id, "driver_number": driver_number,
    })
    .to_string();
    audit_info(pool_main, "driver_mapping", driver_id, "ADMIN_DRIVER_MAPPING_REMOVED", &payload).await
}

pub async fn list_driver_mappings(
    pool_secure: &SqlitePool,
    driver_id: &str,
) -> Result<Vec<DriverTaxMapping>, CfgAdminError> {
    dtm_repo::list_active_for_driver(pool_secure, driver_id)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("driver_tax_mapping::list: {e}")))
}

// ─── fn_outgress_profile commands ──────────────────────────────────

pub async fn set_outgress_profile(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    profile_str: &str,
) -> Result<(), CfgAdminError> {
    if fn_id.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("fn"));
    }
    let profile = OutgressProfile::from_str(profile_str).map_err(|_| {
        CfgAdminError::UnknownProfile(profile_str.to_string())
    })?;
    ensure_fn_in_config(pool_main, fn_id).await?;

    profile_repo::set_profile(pool_secure, fn_id, profile).await.map_err(|e| match e {
        FnOutgressProfileRepoError::Db(e) => {
            CfgAdminError::Infrastructure(format!("profile::set_profile: {e}"))
        }
        other => CfgAdminError::Infrastructure(format!("profile::set_profile: {other}")),
    })?;

    let payload = serde_json::json!({ "fn": fn_id, "profile": profile.as_db_str() }).to_string();
    audit_info(pool_main, "outgress_profile", fn_id, "ADMIN_OUTGRESS_PROFILE_SET", &payload).await
}

pub async fn show_outgress_profile(
    pool_secure: &SqlitePool,
    fn_id: &str,
) -> Result<OutgressProfile, CfgAdminError> {
    profile_repo::get_profile(pool_secure, fn_id)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("profile::get: {e}")))?
        .ok_or_else(|| CfgAdminError::OutgressProfileNotFound(fn_id.to_string()))
}
