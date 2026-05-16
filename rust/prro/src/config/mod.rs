//! AppConfig — TOML, env, CLI overrides.
//!
//! M1 carries only the fields needed to open the DB pool and stub admin_ui;
//! M2+ adds crypto, transports, listen addresses for ingress shells, etc.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub app_name: String,
    pub version: String,
    pub database: DatabaseCfg,
    pub admin_ui: AdminUiCfg,
    /// M3b W8 — return-online detection probe + future offline-sync
    /// settings.  Optional in TOML for back-compat with existing
    /// config files that predate W8.
    #[serde(default)]
    pub offline: OfflineCfg,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseCfg {
    pub db_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminUiCfg {
    pub enabled: bool,
    pub listen: String,
    #[serde(default)]
    pub keys_dir: Option<PathBuf>,
}

/// M3b W8 — offline-sync configuration.  Currently carries the
/// return-online detection probe tick interval; future offline-sync
/// settings (backlog drain cadence, STOP_MODE thresholds, etc) live
/// here too.
#[derive(Debug, Clone, Deserialize)]
pub struct OfflineCfg {
    /// Tick interval for the return-online probe (seconds).  Default
    /// 60s per M3b plan §Task 8 line 576.  Clamped at parse time to
    /// `[5, 3600]` — lower bound guards against accidental
    /// DPS-DoS; upper bound is one hour (operator pin).
    #[serde(default = "default_return_online_probe_interval_seconds")]
    pub return_online_probe_interval_seconds: u64,
}

impl Default for OfflineCfg {
    fn default() -> Self {
        Self {
            return_online_probe_interval_seconds: default_return_online_probe_interval_seconds(),
        }
    }
}

fn default_return_online_probe_interval_seconds() -> u64 {
    60
}

/// Inclusive clamp bounds for `return_online_probe_interval_seconds`.
/// Public so callers (App::boot, doctor, tests) read the same source
/// of truth.
pub const PROBE_INTERVAL_MIN_SECONDS: u64 = 5;
pub const PROBE_INTERVAL_MAX_SECONDS: u64 = 3600;

impl OfflineCfg {
    /// Validate + clamp `return_online_probe_interval_seconds` to
    /// `[PROBE_INTERVAL_MIN_SECONDS, PROBE_INTERVAL_MAX_SECONDS]`.
    /// Returns `(clamped_value, was_clamped)` so callers can emit a
    /// WARN audit if the operator-supplied value was outside bounds.
    pub fn clamped_probe_interval_seconds(&self) -> (u64, bool) {
        let raw = self.return_online_probe_interval_seconds;
        let clamped = raw.clamp(PROBE_INTERVAL_MIN_SECONDS, PROBE_INTERVAL_MAX_SECONDS);
        (clamped, clamped != raw)
    }
}

impl AppConfig {
    pub fn from_toml(s: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(s)?)
    }
}
