//! W4-Z2a piece 5 — secure-pool tax-snapshot loader.
//!
//! Per locked design (memory `project_m4_w4_z2a_locked_design`):
//! caller-owned snapshot, loaded ONCE per receipt at the
//! orchestrator layer (`stage_acquire`) which holds both
//! `pool_secure` (for live config) and `pool` (for snapshot
//! persistence via `signing_config_snapshots::insert_or_get_id`).
//!
//! This module bridges secure → main: reads live `tax_groups` from
//! secure pool, validates rate round-trip to bps via
//! `TaxResolutionSnapshot::try_from_live`, returns ready snapshot
//! that the caller then persists.
//!
//! **driver_id** is the forensic key disambiguator per locked
//! design audit-lineage rule.  Snapshot payload itself currently
//! captures per-FN tax_groups only; `driver_tax_mapping` is the
//! upstream/adapter concern (item.tax_group_1 is translated to
//! canonical TX BEFORE stage_sign).  Including driver_id in the
//! snapshot table UNIQUE key preserves "this doc signed with
//! maria304's view of tax_groups" forensic claim even when two
//! drivers see identical canonical map.

use sqlx::SqlitePool;
use thiserror::Error;

use crate::db::repositories::driver_tax_mapping::{self, DriverTaxMappingRepoError};
use crate::db::repositories::tax_groups::{self, TaxGroupsRepoError};
use crate::services::write_path::tax_summary::{
    DriverNumberMapping, SnapshotBuildError, TaxResolutionSnapshot,
};

#[derive(Debug, Error)]
pub enum LoadSnapshotError {
    /// `tax_groups::list_active_for_fn` DB failure (corrupted table,
    /// transient sqlx error).  Caller MUST audit_log + route to
    /// RequiresManualReconciliation; deterministic from caller's
    /// perspective (next attempt sees same / new failure).
    #[error("tax_snapshot: failed to load active tax_groups for fn={fn_id}: {source}")]
    TaxGroupsLoad {
        fn_id: String,
        #[source]
        source: TaxGroupsRepoError,
    },
    /// One of the loaded rates fails the W4-Z2a bps round-trip
    /// validation (e.g. admin entered 20.005, NaN, negative).
    /// Caller MUST surface to operator — admin config defect.
    #[error("tax_snapshot: snapshot validation failed for fn={fn_id}: {source}")]
    SnapshotBuild {
        fn_id: String,
        #[source]
        source: SnapshotBuildError,
    },
    /// External mid-review CRIT-1 — `driver_tax_mapping::
    /// list_active_for_driver` DB failure.  Distinct from
    /// `TaxGroupsLoad` so operator audit log distinguishes
    /// "FN config corrupted" vs "driver mapping corrupted".
    #[error("tax_snapshot: failed to load driver_tax_mapping for driver={driver_id}: {source}")]
    DriverMappingLoad {
        driver_id: String,
        #[source]
        source: DriverTaxMappingRepoError,
    },
}

/// Load the active `tax_groups` for `fn_id` from `pool_secure`, run
/// W4-Z2a bps round-trip validation, return a snapshot keyed for
/// `(fn_id, driver_id)`.
///
/// The `driver_id` is currently a forensic disambiguator — included
/// in `signing_config_snapshots` UNIQUE key but NOT in payload
/// content.  Two drivers seeing identical tax_groups produce
/// identical `payload_sha256` but distinct snapshot rows (one per
/// driver_id).
///
/// Empty config (no active tax_groups for this FN) → empty groups
/// vector.  Downstream `derive_check_tax_summaries` then guards
/// via `TaxMappingNotWired` if items reference tax_group_1.
pub async fn load_for_fn_driver(
    pool_secure: &SqlitePool,
    fn_id: &str,
    driver_id: &str,
) -> Result<TaxResolutionSnapshot, LoadSnapshotError> {
    let raw = tax_groups::list_active_for_fn(pool_secure, fn_id)
        .await
        .map_err(|source| LoadSnapshotError::TaxGroupsLoad {
            fn_id: fn_id.to_string(),
            source,
        })?;

    // External mid-review CRIT-1 — load active driver_tax_mapping
    // for `driver_id`.  Each row maps the POS-side driver_number
    // (FiscalLine.tax_group_1) to a canonical tx_num that resolves
    // into our `groups` array.  Snapshot persists BOTH (canonical
    // groups + driver mapping) so `check_payload_from` can
    // deterministically translate driver_number→canonical even on
    // MAC recovery via persisted snapshot_id.
    let raw_mappings = driver_tax_mapping::list_active_for_driver(pool_secure, driver_id)
        .await
        .map_err(|source| LoadSnapshotError::DriverMappingLoad {
            driver_id: driver_id.to_string(),
            source,
        })?;
    let driver_mappings: Vec<DriverNumberMapping> = raw_mappings
        .into_iter()
        .map(|m| DriverNumberMapping {
            driver_number: m.driver_number,
            canonical_tx_num: m.canonical_tx_num,
        })
        .collect();

    let rows: Vec<(String, i64, f64, f64, i64, i64)> = raw
        .into_iter()
        .map(|g| (g.fn_id, g.tx_num, g.txpr, g.dtpr, g.txal, g.txty))
        .collect();

    TaxResolutionSnapshot::try_from_live(rows, driver_mappings).map_err(|source| {
        LoadSnapshotError::SnapshotBuild {
            fn_id: fn_id.to_string(),
            source,
        }
    })
}
