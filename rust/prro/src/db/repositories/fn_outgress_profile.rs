//! W4-Z0 piece 6 — repository for `fn_outgress_profile` table.
//!
//! Per spec §1.5.  Per-FN outgress protocol selection.  Pilot every
//! FN defaults to `FSCO_ZZD`; operator switches to `EVPZ_DPS` when
//! W4-Y series ships (post-pilot).
//!
//! Selecting `EVPZ_DPS` for an FN in pilot will surface as
//! `OutgressError::ProfileNotImplemented` at supervisor dispatch
//! time (see `project_m4_outgress_architecture` memory) — the
//! repository accepts the value (the DB enum allows it), but the
//! runtime quartet accessor refuses until W4-Y impl lands.

use sqlx::SqlitePool;
use thiserror::Error;

/// Per-FN outgress protocol selection.  Mirror of the
/// `OutgressProfile` enum defined in the trait abstraction layer
/// (piece 10) — duplicated here so the repository module is
/// stand-alone (no upward dependency on `runtime`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutgressProfile {
    /// Pilot impl: gRPC sendChkV2 to cabinet.tax.gov.ua:9443 (test)
    /// or prro.tax.gov.ua:443 (prod).
    FscoZzd,
    /// Post-pilot: HTTPS REST to ЄВПЗ endpoints.  Schema accepts
    /// the value; runtime supervisor will refuse to dispatch until
    /// W4-Y series lands the impl trio.
    EvpzDps,
}

impl OutgressProfile {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::FscoZzd => "FSCO_ZZD",
            Self::EvpzDps => "EVPZ_DPS",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, FnOutgressProfileRepoError> {
        match s {
            "FSCO_ZZD" => Ok(Self::FscoZzd),
            "EVPZ_DPS" => Ok(Self::EvpzDps),
            other => Err(FnOutgressProfileRepoError::UnknownProfile(
                other.to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnOutgressProfileRow {
    pub fn_id: String,
    pub profile: OutgressProfile,
    pub updated_at: String,
}

#[derive(Debug, Error)]
pub enum FnOutgressProfileRepoError {
    #[error("outgress profile not found: fn={fn_id}")]
    NotFound { fn_id: String },

    /// Defensive — should never trigger because the DB CHECK rejects
    /// non-enum values, but kept typed for symmetry with from_str.
    #[error("unknown outgress profile value: {0}")]
    UnknownProfile(String),

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Upsert the profile for an FN.  Idempotent; bumps `updated_at`.
pub async fn set_profile(
    pool: &SqlitePool,
    fn_id: &str,
    profile: OutgressProfile,
) -> Result<(), FnOutgressProfileRepoError> {
    sqlx::query(
        "INSERT INTO fn_outgress_profile (fn, profile, updated_at) \
         VALUES (?, ?, CURRENT_TIMESTAMP) \
         ON CONFLICT(fn) DO UPDATE SET \
             profile = excluded.profile, \
             updated_at = CURRENT_TIMESTAMP",
    )
    .bind(fn_id)
    .bind(profile.as_db_str())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_profile(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Option<OutgressProfile>, FnOutgressProfileRepoError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT profile FROM fn_outgress_profile WHERE fn = ? LIMIT 1")
            .bind(fn_id)
            .fetch_optional(pool)
            .await?;
    match row {
        None => Ok(None),
        Some((s,)) => OutgressProfile::from_str(&s).map(Some),
    }
}

pub async fn delete_profile(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<(), FnOutgressProfileRepoError> {
    let result = sqlx::query("DELETE FROM fn_outgress_profile WHERE fn = ?")
        .bind(fn_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(FnOutgressProfileRepoError::NotFound {
            fn_id: fn_id.to_string(),
        });
    }
    Ok(())
}

pub async fn list_profiles(
    pool: &SqlitePool,
) -> Result<Vec<FnOutgressProfileRow>, FnOutgressProfileRepoError> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT fn, profile, updated_at FROM fn_outgress_profile ORDER BY fn ASC")
            .fetch_all(pool)
            .await?;

    rows.into_iter()
        .map(|(fn_id, profile_str, updated_at)| {
            OutgressProfile::from_str(&profile_str).map(|profile| FnOutgressProfileRow {
                fn_id,
                profile,
                updated_at,
            })
        })
        .collect()
}
