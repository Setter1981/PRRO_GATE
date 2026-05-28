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

use crate::db::repositories::tax_groups::{self, TaxGroupsRepoError};
use crate::services::write_path::tax_summary::{
    SnapshotBuildError, TaxResolutionSnapshot,
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
    _driver_id: &str,
) -> Result<TaxResolutionSnapshot, LoadSnapshotError> {
    let raw = tax_groups::list_active_for_fn(pool_secure, fn_id)
        .await
        .map_err(|source| LoadSnapshotError::TaxGroupsLoad {
            fn_id: fn_id.to_string(),
            source,
        })?;

    let rows: Vec<(String, i64, f64, f64, i64, i64)> = raw
        .into_iter()
        .map(|g| (g.fn_id, g.tx_num, g.txpr, g.dtpr, g.txal, g.txty))
        .collect();

    TaxResolutionSnapshot::try_from_live(rows).map_err(|source| {
        LoadSnapshotError::SnapshotBuild {
            fn_id: fn_id.to_string(),
            source,
        }
    })
}
