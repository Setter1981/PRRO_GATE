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
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;
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

    #[error("admin(w4-z0): invalid pay_index {0} (must be 1..=99; 0 would yield XML T=-1)")]
    InvalidPayIndex(i64),

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

    #[error(
        "admin(w4-z0): pay_index {1} is a protected D1 slot for RS-2-enabled fn {0:?} — \
         removing/inactivating it would break the frozen-slot policy a running RestHttp \
         listener depends on (CF2)"
    )]
    ProtectedSlotImmutable(String, i64),

    #[error(
        "admin(w4-z0): pay_index {1} on RS-2-enabled fn {0:?} must have iscash={2} (D1 \
         frozen-slot kind) — refusing to set iscash={3} (CF2)"
    )]
    ProtectedSlotKind(String, i64, bool, bool),

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

async fn ensure_fn_in_config(pool_main: &SqlitePool, fn_id: &str) -> Result<(), CfgAdminError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT fiscal_number FROM fiscal_number_config WHERE fiscal_number = ?")
            .bind(fn_id)
            .fetch_optional(pool_main)
            .await
            .map_err(|e| CfgAdminError::Infrastructure(format!("FN existence check: {e}")))?;
    if row.is_none() {
        return Err(CfgAdminError::FiscalNumberNotInConfig(fn_id.to_string()));
    }
    Ok(())
}

/// CF2 — what an admin mutation does to a payment slot, for the
/// protected-slot guard.
enum SlotMutation {
    /// Add the slot, or set its kind — the PROPOSED `iscash`.
    SetKind(bool),
    /// Remove (soft-delete / inactivate) the slot.
    Remove,
}

/// CF2 (RS-2 D1 admin-guard at the MUTATION surface, not startup-only).
///
/// For an FN that has a live RS-2 `RestHttp` listener (`rs2_fns`, derived
/// from `config.listeners` in [`open_pools_from_config`]), a protected D1
/// slot (1..=4) is FROZEN: it may not be removed/inactivated, and its
/// `iscash` kind is fixed (slot 1 = cash, slots 2..=4 = cashless).  A
/// non-RS-2 FN, or a free slot (≥5), is unconstrained — the admin is free.
///
/// The slot→kind map is `preflight::protected_slot_required_iscash`, shared
/// with the startup [`preflight_d1_slots`](crate::runtime::ingress::preflight::preflight_d1_slots)
/// so boot and admin can never disagree.  This closes the "payment_methods
/// is mutable while RS-2 serves" hole: a running listener + convert path
/// depend on the frozen slot semantics, so the admin CLI must not be able to
/// flip slot 1 to non-cash, mislabel a cashless slot, or drop a protected
/// slot out from under live ingress.
fn enforce_protected_slot(
    rs2_fns: &HashSet<String>,
    fn_id: &str,
    pay_index: i64,
    mutation: SlotMutation,
) -> Result<(), CfgAdminError> {
    if !rs2_fns.contains(fn_id) {
        return Ok(());
    }
    let Some(required_cash) =
        crate::runtime::ingress::preflight::protected_slot_required_iscash(pay_index)
    else {
        return Ok(());
    };
    match mutation {
        SlotMutation::Remove => Err(CfgAdminError::ProtectedSlotImmutable(
            fn_id.to_string(),
            pay_index,
        )),
        SlotMutation::SetKind(got) if got != required_cash => Err(
            CfgAdminError::ProtectedSlotKind(fn_id.to_string(), pay_index, required_cash, got),
        ),
        SlotMutation::SetKind(_) => Ok(()),
    }
}

/// Best-effort audit emission.  Audit Round-1 fix (2026-05-27):
/// previous implementation returned `Infrastructure` error when
/// `audit_log::append` failed, which caused the whole command to
/// surface as a CLI failure AFTER the config mutation had already
/// landed on `pool_secure`.  System state diverged from audit trail
/// — operator saw "add-tax-group failed" but the row existed.
///
/// Mirror of W2 PR-B `admin::add_operator` `tracing::error!` fallback
/// pattern for the cross-DB audit-failure branch.  The Critical info
/// here is that the mutation already landed; audit-loss is an
/// observability gap, not a functional failure.  Operator can
/// reconcile via `prro admin list-*` (the row exists) — audit-trail
/// gap is captured in process tracing logs for forensics.
async fn audit_info(
    pool_main: &SqlitePool,
    domain: &str,
    fn_id: &str,
    event_type: &str,
    payload_json: &str,
) -> Result<(), CfgAdminError> {
    if let Err(e) = crate::db::repositories::audit_log::append(
        pool_main,
        domain,
        fn_id,
        event_type,
        crate::db::models::enums::Severity::Info,
        None,
        Some(payload_json),
    )
    .await
    {
        tracing::error!(
            target: "prro::admin_w4_z0",
            event_type = %event_type,
            entity = %fn_id,
            cause = %e,
            "audit append FAILED post-mutation — config row already \
             landed on pool_secure; audit trail missing for this event"
        );
    }
    Ok(())
}

// ─── tax_groups commands ────────────────────────────────────────────

/// Domain parameters for [`add_tax_group`], bundled to keep the
/// function signature within the `clippy::too_many_arguments` bound.
/// Pure data carrier — no behavior change vs the prior flat-param form.
pub struct AddTaxGroupArgs {
    pub fn_id: String,
    pub tx_num: i64,
    pub letter: String,
    pub dtpr: f64,
    pub txpr: f64,
    pub txal: i64,
}

pub async fn add_tax_group(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    args: AddTaxGroupArgs,
) -> Result<(), CfgAdminError> {
    let AddTaxGroupArgs {
        fn_id,
        tx_num,
        letter,
        dtpr,
        txpr,
        txal,
    } = args;
    let fn_id = fn_id.as_str();
    let letter = letter.as_str();
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
            audit_info(
                pool_main,
                "tax_group",
                fn_id,
                "ADMIN_TAX_GROUP_ADDED",
                &payload,
            )
            .await?;
            Ok(())
        }
        Err(TaxGroupsRepoError::DuplicateTxNum { fn_id, tx_num }) => {
            Err(CfgAdminError::DuplicateTaxGroup(fn_id, tx_num))
        }
        Err(TaxGroupsRepoError::DuplicateActiveLetter { fn_id, letter }) => {
            Err(CfgAdminError::DuplicateActiveLetter(fn_id, letter))
        }
        Err(e) => Err(CfgAdminError::Infrastructure(format!(
            "tax_groups::insert: {e}"
        ))),
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

    tg_repo::update_rates(
        pool_secure,
        fn_id,
        tx_num,
        new_dtpr,
        new_txpr,
        new_txal,
        current.txty,
    )
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
    audit_info(
        pool_main,
        "tax_group",
        fn_id,
        "ADMIN_TAX_GROUP_UPDATED",
        &payload,
    )
    .await?;
    Ok(())
}

pub async fn remove_tax_group(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
    tx_num: i64,
) -> Result<(), CfgAdminError> {
    tg_repo::soft_delete(pool_secure, fn_id, tx_num)
        .await
        .map_err(|e| match e {
            TaxGroupsRepoError::NotFound { fn_id, tx_num } => {
                CfgAdminError::TaxGroupNotFound(fn_id, tx_num)
            }
            other => CfgAdminError::Infrastructure(format!("tax_groups::soft_delete: {other}")),
        })?;

    let payload = serde_json::json!({ "fn": fn_id, "tx_num": tx_num }).to_string();
    audit_info(
        pool_main,
        "tax_group",
        fn_id,
        "ADMIN_TAX_GROUP_REMOVED",
        &payload,
    )
    .await?;
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
    rs2_fns: &HashSet<String>,
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
    // Audit Round-1 (2026-05-27): pay_index=0 → XML T="-1" semantic error.
    // DB CHECK enforces it too; admin-layer rejection surfaces a typed
    // error instead of generic Infrastructure(CHECK violation).
    if !(1..=99).contains(&pay_index) {
        return Err(CfgAdminError::InvalidPayIndex(pay_index));
    }
    ensure_fn_in_config(pool_main, fn_id).await?;
    // CF2 — on an RS-2-enabled FN, adding a protected slot (1..=4) is allowed
    // ONLY if its kind matches the D1 frozen-slot policy.
    enforce_protected_slot(rs2_fns, fn_id, pay_index, SlotMutation::SetKind(iscash))?;

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
            audit_info(
                pool_main,
                "payment_method",
                fn_id,
                "ADMIN_PAYMENT_METHOD_ADDED",
                &payload,
            )
            .await
        }
        Err(PaymentMethodsRepoError::DuplicatePayIndex { fn_id, pay_index }) => {
            Err(CfgAdminError::DuplicatePaymentMethod(fn_id, pay_index))
        }
        Err(PaymentMethodsRepoError::DuplicateActiveName { fn_id, name }) => {
            Err(CfgAdminError::DuplicatePaymentName(fn_id, name))
        }
        Err(e) => Err(CfgAdminError::Infrastructure(format!(
            "payment_methods::insert: {e}"
        ))),
    }
}

pub async fn update_payment_method(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    rs2_fns: &HashSet<String>,
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

    // CF2 — on an RS-2-enabled FN, a protected slot's `iscash` kind is frozen;
    // refuse a mutation whose RESULT would violate the D1 policy.  A name-only
    // update on a correctly-kinded protected slot keeps `new_iscash` == the
    // required kind and passes.
    enforce_protected_slot(rs2_fns, fn_id, pay_index, SlotMutation::SetKind(new_iscash))?;

    pm_repo::update(pool_secure, fn_id, pay_index, &new_name, new_iscash)
        .await
        .map_err(|e| match e {
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
    audit_info(
        pool_main,
        "payment_method",
        fn_id,
        "ADMIN_PAYMENT_METHOD_UPDATED",
        &payload,
    )
    .await
}

pub async fn remove_payment_method(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    rs2_fns: &HashSet<String>,
    fn_id: &str,
    pay_index: i64,
) -> Result<(), CfgAdminError> {
    // CF2 — a protected D1 slot (1..=4) cannot be removed/inactivated on an
    // FN with a live RS-2 RestHttp listener (the listener depends on it).
    enforce_protected_slot(rs2_fns, fn_id, pay_index, SlotMutation::Remove)?;
    pm_repo::soft_delete(pool_secure, fn_id, pay_index)
        .await
        .map_err(|e| match e {
            PaymentMethodsRepoError::NotFound { fn_id, pay_index } => {
                CfgAdminError::PaymentMethodNotFound(fn_id, pay_index)
            }
            other => {
                CfgAdminError::Infrastructure(format!("payment_methods::soft_delete: {other}"))
            }
        })?;
    let payload = serde_json::json!({ "fn": fn_id, "pay_index": pay_index }).to_string();
    audit_info(
        pool_main,
        "payment_method",
        fn_id,
        "ADMIN_PAYMENT_METHOD_REMOVED",
        &payload,
    )
    .await
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

    flags_repo::set_flag(pool_secure, fn_id, name, value)
        .await
        .map_err(|e| match e {
            FnIntegrationFlagsRepoError::Db(e) => {
                CfgAdminError::Infrastructure(format!("flags::set_flag: {e}"))
            }
            other => CfgAdminError::Infrastructure(format!("flags::set_flag: {other}")),
        })?;

    let payload = serde_json::json!({ "fn": fn_id, "name": name, "value": value }).to_string();
    audit_info(
        pool_main,
        "integration_flag",
        fn_id,
        "ADMIN_FLAG_SET",
        &payload,
    )
    .await
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
    // Audit Round-4 (2026-05-27): normalize driver_id via the same
    // `DriverId::new` trim/length validation the listener uses, so
    // an operator typing `--driver-id ' maria304 '` (extra YAML-ish
    // whitespace) DOES persist as the same value the supervisor
    // will look up.  Pre-fix path stored the raw string → W4-Z1
    // driver_tax_mapping lookup would silently miss.
    let normalized = crate::db::models::ids::DriverId::new(driver_id)
        .map_err(|_| CfgAdminError::EmptyArgument("driver-id"))?;
    let driver_id = normalized.as_str();

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
            audit_info(
                pool_main,
                "driver_mapping",
                driver_id,
                "ADMIN_DRIVER_MAPPING_ADDED",
                &payload,
            )
            .await
        }
        Err(DriverTaxMappingRepoError::DuplicatePk {
            driver_id,
            driver_number,
        }) => Err(CfgAdminError::DuplicateDriverMapping(
            driver_id,
            driver_number,
        )),
        Err(e) => Err(CfgAdminError::Infrastructure(format!(
            "driver_tax_mapping::insert: {e}"
        ))),
    }
}

/// Update an existing driver-tax-mapping row's canonical_tx_num.
/// Audit Round-1 fix (2026-05-27): admin previously exposed only
/// add/remove/list, but since soft-delete leaves the (driver_id,
/// driver_number) PK occupied, operator could not correct a bad
/// mapping via add-after-remove.  This is the missing UPDATE path.
pub async fn update_driver_mapping(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    driver_id: &str,
    driver_number: i64,
    canonical_tx_num: i64,
) -> Result<(), CfgAdminError> {
    // Audit Round-4: normalize via DriverId::new (see add_driver_mapping).
    let normalized = crate::db::models::ids::DriverId::new(driver_id)
        .map_err(|_| CfgAdminError::EmptyArgument("driver-id"))?;
    let driver_id = normalized.as_str();
    if !(1..=99).contains(&canonical_tx_num) {
        return Err(CfgAdminError::Infrastructure(format!(
            "canonical_tx_num {canonical_tx_num} out of range 1..=99"
        )));
    }

    dtm_repo::update_canonical(pool_secure, driver_id, driver_number, canonical_tx_num)
        .await
        .map_err(|e| match e {
            DriverTaxMappingRepoError::NotFound {
                driver_id,
                driver_number,
            } => CfgAdminError::DriverMappingNotFound(driver_id, driver_number),
            other => CfgAdminError::Infrastructure(format!(
                "driver_tax_mapping::update_canonical: {other}"
            )),
        })?;

    let payload = serde_json::json!({
        "driver_id": driver_id,
        "driver_number": driver_number,
        "canonical_tx_num": canonical_tx_num,
    })
    .to_string();
    audit_info(
        pool_main,
        "driver_mapping",
        driver_id,
        "ADMIN_DRIVER_MAPPING_UPDATED",
        &payload,
    )
    .await
}

pub async fn remove_driver_mapping(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    driver_id: &str,
    driver_number: i64,
) -> Result<(), CfgAdminError> {
    // Audit Round-4: normalize via DriverId::new.
    let normalized = crate::db::models::ids::DriverId::new(driver_id)
        .map_err(|_| CfgAdminError::EmptyArgument("driver-id"))?;
    let driver_id = normalized.as_str();
    dtm_repo::soft_delete(pool_secure, driver_id, driver_number)
        .await
        .map_err(|e| match e {
            DriverTaxMappingRepoError::NotFound {
                driver_id,
                driver_number,
            } => CfgAdminError::DriverMappingNotFound(driver_id, driver_number),
            other => {
                CfgAdminError::Infrastructure(format!("driver_tax_mapping::soft_delete: {other}"))
            }
        })?;
    let payload = serde_json::json!({
        "driver_id": driver_id, "driver_number": driver_number,
    })
    .to_string();
    audit_info(
        pool_main,
        "driver_mapping",
        driver_id,
        "ADMIN_DRIVER_MAPPING_REMOVED",
        &payload,
    )
    .await
}

pub async fn list_driver_mappings(
    pool_secure: &SqlitePool,
    driver_id: &str,
) -> Result<Vec<DriverTaxMapping>, CfgAdminError> {
    // Audit Round-4: normalize via DriverId::new so list lookup
    // matches the supervisor-stamped value.
    let normalized = crate::db::models::ids::DriverId::new(driver_id)
        .map_err(|_| CfgAdminError::EmptyArgument("driver-id"))?;
    dtm_repo::list_active_for_driver(pool_secure, normalized.as_str())
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
    let profile = OutgressProfile::from_str(profile_str)
        .map_err(|_| CfgAdminError::UnknownProfile(profile_str.to_string()))?;
    ensure_fn_in_config(pool_main, fn_id).await?;

    profile_repo::set_profile(pool_secure, fn_id, profile)
        .await
        .map_err(|e| match e {
            FnOutgressProfileRepoError::Db(e) => {
                CfgAdminError::Infrastructure(format!("profile::set_profile: {e}"))
            }
            other => CfgAdminError::Infrastructure(format!("profile::set_profile: {other}")),
        })?;

    let payload = serde_json::json!({ "fn": fn_id, "profile": profile.as_db_str() }).to_string();
    audit_info(
        pool_main,
        "outgress_profile",
        fn_id,
        "ADMIN_OUTGRESS_PROFILE_SET",
        &payload,
    )
    .await
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

// ─── CLI entry-points (piece 8c) ───────────────────────────────────
//
// `run_*` wrappers read config, acquire singleton lock, open both
// pools, invoke the library function, then close pools.  Mirror of
// W2 PR-B `admin::run_add_operator` pattern.

/// Audit Round-2 (2026-05-27): every W4-Z0 mutating CLI command now
/// acquires the singleton lock from `runtime::singleton::acquire`,
/// matching `admin::run_add_operator` + `admin::run_reset_stop_mode`.
/// Without the lock, a `prro admin add-tax-group` could race
/// `prro serve`'s live secure.db reads (post-W4-Z1) or another
/// admin command, causing config drift between read and use.
///
/// The lock is acquired BEFORE pool open; caller MUST keep the
/// returned `SingletonGuard` alive for the duration of the command.
async fn open_pools_from_config(
    config_path: &Path,
) -> Result<
    (
        crate::runtime::singleton::PidLock,
        SqlitePool,
        SqlitePool,
        HashSet<String>,
    ),
    CfgAdminError,
> {
    let cfg_text = std::fs::read_to_string(config_path)
        .map_err(|e| CfgAdminError::Infrastructure(format!("read config: {e}")))?;
    let cfg = crate::config::AppConfig::from_toml(&cfg_text)
        .map_err(|e| CfgAdminError::Infrastructure(format!("parse config: {e}")))?;
    // CF2 — derive the RS-2-enabled FN set (FNs with a RestHttp listener) so
    // the payment-method mutation guard is config-aware.  The full AppConfig
    // (incl. `listeners`) was already parsed here and previously dropped.
    let rs2_fns: HashSet<String> = cfg
        .listeners
        .iter()
        .filter(|l| l.kind == crate::config::ListenerKind::RestHttp)
        .map(|l| l.fiscal_number.clone())
        .collect();
    let guard = crate::runtime::singleton::acquire(&cfg.database.db_path)
        .map_err(|e| CfgAdminError::Infrastructure(format!("singleton lock: {e}")))?;
    let pool_main = crate::db::open_pool(&cfg.database.db_path)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("open main pool: {e}")))?;
    let pool_secure = crate::db::open_secure_pool(&cfg.database.secure_db_path)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("open secure pool: {e}")))?;
    Ok((guard, pool_main, pool_secure, rs2_fns))
}

/// Open pools + call body + close pools.  Inline form (closures + async
/// move had lifetime issues with `&String` borrows captured into the
/// returned future).
///
/// Audit Round-2 (2026-05-27): the singleton `_guard` returned from
/// `open_pools_from_config` is bound here so it stays alive for the
/// duration of the command body, then drops AFTER pool closure.
/// Drop order: result async block (mutation + audit) → pools close
/// → guard drop releases the lock file.
macro_rules! with_pools {
    // 3-arg — callers that do not need the RS-2-enabled FN set (the set is
    // bound and dropped).
    ($cfg:expr, $main:ident, $secure:ident, $body:block) => {{
        let (_guard, $main, $secure, _rs2_fns) = open_pools_from_config($cfg).await?;
        let result = async $body.await;
        $secure.close().await;
        $main.close().await;
        drop(_guard);
        result
    }};
    // 4-arg (CF2) — exposes the RS-2-enabled FN set (`$rs2`) to the body for
    // the payment-method mutation guard.
    ($cfg:expr, $main:ident, $secure:ident, $rs2:ident, $body:block) => {{
        let (_guard, $main, $secure, $rs2) = open_pools_from_config($cfg).await?;
        let result = async $body.await;
        $secure.close().await;
        $main.close().await;
        drop(_guard);
        result
    }};
}

pub async fn run_add_tax_group(
    cfg: &Path,
    fn_id: String,
    tx_num: i64,
    letter: String,
    dtpr: f64,
    txpr: f64,
    txal: i64,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        add_tax_group(
            &pm,
            &ps,
            AddTaxGroupArgs {
                fn_id,
                tx_num,
                letter,
                dtpr,
                txpr,
                txal,
            },
        )
        .await
    })
}

pub async fn run_update_tax_rate(
    cfg: &Path,
    fn_id: String,
    tx_num: i64,
    dtpr: Option<f64>,
    txpr: Option<f64>,
    txal: Option<i64>,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        update_tax_rate(&pm, &ps, &fn_id, tx_num, dtpr, txpr, txal).await
    })
}

pub async fn run_remove_tax_group(
    cfg: &Path,
    fn_id: String,
    tx_num: i64,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        remove_tax_group(&pm, &ps, &fn_id, tx_num).await
    })
}

pub async fn run_list_tax_groups(
    cfg: &Path,
    fn_id: String,
) -> Result<Vec<TaxGroup>, CfgAdminError> {
    with_pools!(cfg, pm, ps, { list_tax_groups(&ps, &fn_id).await })
}

pub async fn run_add_payment_method(
    cfg: &Path,
    fn_id: String,
    pay_index: i64,
    name: String,
    iscash: bool,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, rs2, {
        add_payment_method(&pm, &ps, &rs2, &fn_id, pay_index, &name, iscash).await
    })
}

pub async fn run_update_payment_method(
    cfg: &Path,
    fn_id: String,
    pay_index: i64,
    name: Option<String>,
    iscash: Option<bool>,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, rs2, {
        update_payment_method(&pm, &ps, &rs2, &fn_id, pay_index, name.as_deref(), iscash).await
    })
}

pub async fn run_remove_payment_method(
    cfg: &Path,
    fn_id: String,
    pay_index: i64,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, rs2, {
        remove_payment_method(&pm, &ps, &rs2, &fn_id, pay_index).await
    })
}

pub async fn run_list_payment_methods(
    cfg: &Path,
    fn_id: String,
) -> Result<Vec<PaymentMethod>, CfgAdminError> {
    with_pools!(cfg, pm, ps, { list_payment_methods(&ps, &fn_id).await })
}

pub async fn run_set_flag(
    cfg: &Path,
    fn_id: String,
    name: String,
    value: String,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        set_flag(&pm, &ps, &fn_id, &name, &value).await
    })
}

pub async fn run_set_national_receipt(
    cfg: &Path,
    fn_id: String,
    enabled: bool,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        set_national_receipt(&pm, &ps, &fn_id, enabled).await
    })
}

pub async fn run_list_flags(
    cfg: &Path,
    fn_id: String,
) -> Result<Vec<FnIntegrationFlag>, CfgAdminError> {
    with_pools!(cfg, pm, ps, { list_flags(&ps, &fn_id).await })
}

pub async fn run_add_driver_mapping(
    cfg: &Path,
    driver_id: String,
    driver_number: i64,
    canonical_tx_num: i64,
    driver_letter: Option<String>,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        add_driver_mapping(
            &pm,
            &ps,
            &driver_id,
            driver_number,
            canonical_tx_num,
            driver_letter.as_deref(),
        )
        .await
    })
}

pub async fn run_update_driver_mapping(
    cfg: &Path,
    driver_id: String,
    driver_number: i64,
    canonical_tx_num: i64,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        update_driver_mapping(&pm, &ps, &driver_id, driver_number, canonical_tx_num).await
    })
}

pub async fn run_remove_driver_mapping(
    cfg: &Path,
    driver_id: String,
    driver_number: i64,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        remove_driver_mapping(&pm, &ps, &driver_id, driver_number).await
    })
}

pub async fn run_list_driver_mappings(
    cfg: &Path,
    driver_id: String,
) -> Result<Vec<DriverTaxMapping>, CfgAdminError> {
    with_pools!(cfg, pm, ps, { list_driver_mappings(&ps, &driver_id).await })
}

pub async fn run_set_outgress_profile(
    cfg: &Path,
    fn_id: String,
    profile: String,
) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, {
        set_outgress_profile(&pm, &ps, &fn_id, &profile).await
    })
}

pub async fn run_show_outgress_profile(
    cfg: &Path,
    fn_id: String,
) -> Result<OutgressProfile, CfgAdminError> {
    with_pools!(cfg, pm, ps, { show_outgress_profile(&ps, &fn_id).await })
}

pub async fn run_bootstrap_defaults(cfg: &Path, fn_id: String) -> Result<(), CfgAdminError> {
    with_pools!(cfg, pm, ps, { bootstrap_defaults(&pm, &ps, &fn_id).await })
}

// ─── recovery: standalone bootstrap-defaults command ──────────────

/// Audit Round-1 fix (2026-05-27): explicit operator-driven
/// re-bootstrap of per-FN config defaults.  Use case: bootstrap
/// failed during `add-operator` (e.g. transient secure.db lock) and
/// the FN was registered without the 11 tax_groups + 4 payment_
/// methods + outgress_profile + useecheckmegovua flag.  Operator
/// invokes this to seed the missing rows.
///
/// Idempotency semantics inherit from `bootstrap_fn_defaults`:
/// existing active rows are preserved; soft-deleted defaults are
/// reactivated; no row data is overwritten.
pub async fn bootstrap_defaults(
    pool_main: &SqlitePool,
    pool_secure: &SqlitePool,
    fn_id: &str,
) -> Result<(), CfgAdminError> {
    if fn_id.trim().is_empty() {
        return Err(CfgAdminError::EmptyArgument("fn"));
    }
    ensure_fn_in_config(pool_main, fn_id).await?;

    crate::runtime::bootstrap::bootstrap_fn_defaults(pool_secure, fn_id)
        .await
        .map_err(|e| CfgAdminError::Infrastructure(format!("bootstrap_fn_defaults: {e}")))?;

    let payload = serde_json::json!({ "fn": fn_id }).to_string();
    audit_info(
        pool_main,
        "operator",
        fn_id,
        "ADMIN_FN_DEFAULTS_BOOTSTRAPPED",
        &payload,
    )
    .await
}
