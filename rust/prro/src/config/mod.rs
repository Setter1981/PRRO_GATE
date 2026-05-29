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
    /// W4-Z0 piece 9 + audit Round-2 (2026-05-27) — per-listener
    /// ingress config: each `ListenerCfg` carries `(type, port,
    /// driver_id, fn)`.  The listener stamps `(driver_id, fn)`
    /// onto incoming canonical commands via
    /// `to_canonical_fiscal_command_with_context` (see
    /// `runtime/ingress/dto.rs`).  Optional + default = empty Vec
    /// for back-compat with existing M1-M3b configs that have no
    /// HTTP ingress yet; live wiring lands in W4 supervisor.
    #[serde(default)]
    pub listeners: Vec<ListenerCfg>,
}

/// Per-listener ingress config.  W4-Z0 piece 9 architectural pin —
/// each port serves ONE `(driver_id, fn)` pair.  Multiple listeners
/// of the same `driver_id` may bind to different FNs on different
/// ports (e.g. 2 Maria emulators for 2 FNs).
///
/// `type` (renamed `kind` to avoid Rust keyword clash) selects the
/// protocol shell that parses the wire DTO before handing off to
/// `to_canonical_fiscal_command_with_context`.
///
/// Audit Round-3 (2026-05-27): typed enum so YAML typos
/// (`type = "maria304"` missing `_tcp`) fail-fast at config parse
/// time rather than at supervisor startup.
///
/// `driver_id` MUST be non-empty / non-whitespace per
/// `DriverId::new` validation.  `fn` MUST exist in
/// `fiscal_number_config` (validated at supervisor startup, not
/// here at parse time — same boundary as `BindingsRegistry`).
#[derive(Debug, Clone, Deserialize)]
pub struct ListenerCfg {
    #[serde(rename = "type")]
    pub kind: ListenerKind,
    pub port: u16,
    pub driver_id: String,
    #[serde(rename = "fn")]
    pub fiscal_number: String,
}

/// Listener protocol shell selector.  `#[serde(rename_all = "snake_case")]`
/// matches the YAML form (e.g., `type = "maria304_tcp"`).  Unknown
/// values fail-fast at TOML parse time with a clear serde error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenerKind {
    /// Legacy maria304 wire-protocol TCP shell.
    Maria304Tcp,
    /// XML-RPC shell for WebCheck-compatible drivers.
    WebcheckXmlrpc,
    /// M4 HTTP ingress (axum router).
    RestHttp,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseCfg {
    pub db_path: PathBuf,

    /// W2 / HIGH-AUDIT-01 — physical path to the **secure** SQLite
    /// database holding only the `operators` table (cashier EDS-key
    /// registry).  Hard isolation from `db_path`: separate file,
    /// separate pool, separate migrations dir (`migrations_secure/`),
    /// chmod 0o600 enforced at open time.
    ///
    /// **No default** — fail-closed.  Operators must explicitly choose
    /// a path so the secure DB never accidentally lands inside the
    /// main DB's directory or under a world-readable tree.  Existing
    /// configs predating W2 will fail to parse until updated; this is
    /// intentional per the external architectural audit (HIGH-AUDIT-01).
    pub secure_db_path: PathBuf,
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
    /// 60s per M3b plan §Task 8 line 576.
    ///
    /// **Raw value.**  This field stores the operator-supplied value
    /// verbatim; it is NOT clamped at parse time.  Boot callers MUST
    /// route through [`OfflineCfg::clamped_probe_interval_seconds`]
    /// to obtain the safe `[PROBE_INTERVAL_MIN_SECONDS,
    /// PROBE_INTERVAL_MAX_SECONDS]` value and emit a WARN audit if
    /// the supplied value was outside bounds.  Reading this field
    /// directly is an API contract violation for runtime hot paths.
    /// Lower bound guards against accidental DPS overload; upper
    /// bound is one hour (operator pin).
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PR-A iter 1 — parses `[database].secure_db_path` field
    /// (HIGH-AUDIT-01 secure-db hard isolation; W2 plan §3 W2).
    #[test]
    fn parses_secure_db_path_field() {
        let toml = r#"
            app_name = "prro"
            version = "0.1.0"

            [database]
            db_path = "var/prro.db"
            secure_db_path = "var/secure.db"

            [admin_ui]
            enabled = false
            listen = "127.0.0.1:8081"
        "#;
        let cfg = AppConfig::from_toml(toml).expect("parse must succeed");
        assert_eq!(cfg.database.secure_db_path, PathBuf::from("var/secure.db"));
    }

    /// PR-A iter 1 — missing `secure_db_path` is a parse error.
    /// Fail-closed: HIGH-AUDIT-01 requires the operator to explicitly
    /// choose a path so the secure DB never accidentally lands inside
    /// the main DB's directory.  No silent default.
    #[test]
    fn missing_secure_db_path_is_parse_error() {
        let toml = r#"
            app_name = "prro"
            version = "0.1.0"

            [database]
            db_path = "var/prro.db"

            [admin_ui]
            enabled = false
            listen = "127.0.0.1:8081"
        "#;
        let err =
            AppConfig::from_toml(toml).expect_err("must fail without secure_db_path (fail-closed)");
        let msg = err.to_string();
        assert!(
            msg.contains("secure_db_path"),
            "expected error mentioning secure_db_path, got: {msg}"
        );
    }
}
