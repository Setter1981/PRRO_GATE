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

    /// RS-1 (2026-05-30) — runtime supervisor (composition root + boot
    /// reconcile + drain/probe loops).  Optional + default-disabled:
    /// existing M1-M3b configs with no `[supervisor]` section parse
    /// unchanged and the binary stays M1-idle.  Turning it on is an
    /// explicit config flip; rollback is the reverse flip, never a code
    /// revert.
    #[serde(default)]
    pub supervisor: SupervisorCfg,
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

/// RS-1 — runtime supervisor configuration.  The supervisor (Serve path)
/// builds the composition root (per-FN `DpsChannel` + `SigningContext` via
/// the operator key loader), runs boot reconciliation under the global
/// reconcile mutex, then spawns the drain + return-online loops.
///
/// Gated by `enabled` (default **false**) so the binary ships M1-idle
/// until the pilot DB + live DPS channel are validated.  This is the
/// rollback seam: turning the supervisor on/off is a config flip, never a
/// code revert.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SupervisorCfg {
    /// Master on/off for the runtime spine.  Default **false** = the
    /// binary boots and idles (M1 behaviour) regardless of the other
    /// fields.  Only when `true` does Serve construct the channel +
    /// bindings + supervisor and spawn the loops.
    #[serde(default)]
    pub enabled: bool,

    /// DPS fiscal-service wire config.  Validated at supervisor startup
    /// (NOT at parse time) so a default-off binary with no `endpoint`
    /// still boots unchanged.
    #[serde(default)]
    pub dps: DpsCfg,
}

/// DPS fiscal-service connection config.  RS-1 is **wire-only**:
/// server-trust TLS (no client certificate) because DPS authentication is
/// application-layer CMS, not mTLS.
#[derive(Debug, Clone, Deserialize)]
pub struct DpsCfg {
    /// Endpoint URL, e.g. `https://cabinet.tax.gov.ua:9443`.
    ///
    /// **`Option`, not a required field.**  Absence is a parse-time
    /// no-op so an `enabled = false` binary boots without it (operator
    /// refinement 2026-05-30).  It is **fail-closed at supervisor
    /// startup** via [`SupervisorCfg::require_dps_endpoint`], which errors
    /// when `enabled = true` and the endpoint is missing/blank — unlike
    /// `secure_db_path`, which fails at parse time for ALL configs.
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Per-request gRPC deadline (seconds).  **Raw value** — route boot
    /// callers through [`DpsCfg::clamped_request_timeout_seconds`].
    #[serde(default = "default_dps_request_timeout_seconds")]
    pub request_timeout_seconds: u64,
}

fn default_dps_request_timeout_seconds() -> u64 {
    30
}

impl Default for DpsCfg {
    /// Hand-written (NOT derived) so that a missing `[supervisor.dps]`
    /// table yields the SAME `request_timeout_seconds` as a present table
    /// with the field omitted.  A derived `Default` would give `0`
    /// (→ clamped to the 1s floor), making the effective timeout depend
    /// on whether the operator wrote the `[supervisor.dps]` header — a
    /// silent inconsistency.  Keep this in sync with the
    /// `#[serde(default = ...)]` on `request_timeout_seconds`.
    fn default() -> Self {
        Self {
            endpoint: None,
            request_timeout_seconds: default_dps_request_timeout_seconds(),
        }
    }
}

/// Inclusive clamp bounds for the DPS per-request timeout.  Public so
/// boot callers read the same source of truth.
pub const DPS_REQUEST_TIMEOUT_MIN_SECONDS: u64 = 1;
pub const DPS_REQUEST_TIMEOUT_MAX_SECONDS: u64 = 120;

impl DpsCfg {
    /// Validate + clamp `request_timeout_seconds` to
    /// `[DPS_REQUEST_TIMEOUT_MIN_SECONDS, DPS_REQUEST_TIMEOUT_MAX_SECONDS]`.
    /// Returns `(clamped_value, was_clamped)` so callers can emit a WARN
    /// audit if the operator-supplied value was outside bounds.
    pub fn clamped_request_timeout_seconds(&self) -> (u64, bool) {
        let raw = self.request_timeout_seconds;
        let clamped = raw.clamp(
            DPS_REQUEST_TIMEOUT_MIN_SECONDS,
            DPS_REQUEST_TIMEOUT_MAX_SECONDS,
        );
        (clamped, clamped != raw)
    }
}

impl SupervisorCfg {
    /// Fail-closed DPS endpoint resolution — invoked **at supervisor
    /// startup**, not at config parse time.  Returns:
    /// - `Ok(None)` when the supervisor is disabled (binary stays M1-idle);
    /// - `Ok(Some(endpoint))` when enabled with a non-blank endpoint;
    /// - `Err` when `enabled = true` but the endpoint is missing/blank.
    ///
    /// The parse-vs-startup split is deliberate (operator refinement
    /// 2026-05-30): a default-off binary must boot without a
    /// `[supervisor.dps] endpoint`, while an *enabled* supervisor must
    /// fail-closed rather than silently dial nothing.
    pub fn require_dps_endpoint(&self) -> anyhow::Result<Option<String>> {
        if !self.enabled {
            return Ok(None);
        }
        match self.dps.endpoint.as_deref().map(str::trim) {
            Some(ep) if !ep.is_empty() => Ok(Some(ep.to_string())),
            _ => anyhow::bail!(
                "supervisor.enabled = true but supervisor.dps.endpoint is missing or blank \
                 (fail-closed: an enabled supervisor must have an explicit DPS endpoint)"
            ),
        }
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

    const BASE_CFG: &str = r#"
        app_name = "prro"
        version = "0.1.0"

        [database]
        db_path = "var/prro.db"
        secure_db_path = "var/secure.db"

        [admin_ui]
        enabled = false
        listen = "127.0.0.1:8081"
    "#;

    /// RS-1 Piece 1 — a config with no `[supervisor]` section parses and
    /// the supervisor is disabled by default; `require_dps_endpoint`
    /// returns `Ok(None)` so the M1-idle binary boots without a DPS
    /// endpoint (operator refinement: fail-closed ONLY when enabled).
    #[test]
    fn supervisor_absent_defaults_disabled_and_boots() {
        let cfg = AppConfig::from_toml(BASE_CFG).expect("parse must succeed");
        assert!(!cfg.supervisor.enabled, "supervisor disabled by default");
        assert!(cfg.supervisor.dps.endpoint.is_none());
        assert_eq!(
            cfg.supervisor.require_dps_endpoint().expect("disabled = Ok"),
            None,
            "disabled supervisor needs no endpoint",
        );
    }

    /// RS-1 Piece 1 — an ENABLED supervisor with no DPS endpoint PARSES
    /// (no parse-time failure, unlike secure_db_path) but fails closed at
    /// supervisor startup.  This is the operator's parse-vs-startup split.
    #[test]
    fn supervisor_enabled_without_endpoint_parses_but_startup_fails() {
        let toml = format!("{BASE_CFG}\n[supervisor]\nenabled = true\n");
        let cfg = AppConfig::from_toml(&toml).expect("parse must succeed (no parse-time fail)");
        assert!(cfg.supervisor.enabled);
        let err = cfg
            .supervisor
            .require_dps_endpoint()
            .expect_err("enabled + no endpoint must fail closed at startup");
        assert!(
            err.to_string().contains("endpoint"),
            "expected endpoint error, got: {err}"
        );
    }

    /// RS-1 Piece 1 — an enabled supervisor with a (whitespace-padded)
    /// endpoint resolves to the trimmed value.
    #[test]
    fn supervisor_enabled_with_endpoint_resolves() {
        let toml = format!(
            "{BASE_CFG}\n[supervisor]\nenabled = true\n[supervisor.dps]\nendpoint = \"  https://cabinet.tax.gov.ua:9443  \"\n"
        );
        let cfg = AppConfig::from_toml(&toml).expect("parse must succeed");
        assert_eq!(
            cfg.supervisor.require_dps_endpoint().expect("Ok"),
            Some("https://cabinet.tax.gov.ua:9443".to_string()),
        );
    }

    /// RS-1 Piece 1 — DPS request timeout clamps to bounds + reports it.
    #[test]
    fn dps_request_timeout_clamps_out_of_range() {
        let dps = DpsCfg {
            endpoint: None,
            request_timeout_seconds: 99_999,
        };
        let (clamped, was_clamped) = dps.clamped_request_timeout_seconds();
        assert_eq!(clamped, DPS_REQUEST_TIMEOUT_MAX_SECONDS);
        assert!(was_clamped);
        // Default is in-range and not clamped.
        let (def, def_clamped) = DpsCfg::default().clamped_request_timeout_seconds();
        assert_eq!(def, default_dps_request_timeout_seconds());
        assert!(!def_clamped);
    }
}
