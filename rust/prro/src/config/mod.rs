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

    /// RS-4 (audit pass-2, item 4) — production backup / retention.  Optional +
    /// default-enabled: existing configs with no `[backup]` section get hourly
    /// snapshots of both DB files with sane retention once the supervisor runs.
    #[serde(default)]
    pub backup: BackupCfg,
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

    /// RS-2 D2 (loopback-only pilot) — **host** the ingress listener
    /// binds to (the `port` is the field above; this is host-only).
    /// Defaults to `127.0.0.1`.  For a `RestHttp` listener this is
    /// validated fail-closed at supervisor startup
    /// ([`validate_rs2_loopback_binds`]): it must parse as a
    /// [`std::net::IpAddr`] (hostnames like `localhost` are rejected)
    /// and be a loopback address.  `0.0.0.0` / `::` are
    /// `UNSPECIFIED` (all-interfaces), NOT loopback, and are refused
    /// until the LAN bearer/token-resolver lands (RS-2 §0.4 D2).
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
}

fn default_listen_addr() -> String {
    "127.0.0.1".to_string()
}

/// RS-2 D2 — error from binding-address validation of a `RestHttp`
/// listener under the loopback-only pilot policy.
#[derive(Debug, thiserror::Error)]
pub enum ListenerBindError {
    /// `listen_addr` does not parse as an `IpAddr` — hostnames
    /// (`localhost`) and garbage are rejected fail-closed so the
    /// loopback classification can never be fooled by a name that
    /// resolves off-host.
    #[error(
        "RS-2 RestHttp listener for fn {fiscal_number}: listen_addr {listen_addr:?} is not a \
         valid IP address (hostnames are rejected fail-closed — use 127.0.0.1)"
    )]
    UnparseableAddr {
        fiscal_number: String,
        listen_addr: String,
    },

    /// The bind address parses but is not loopback (`0.0.0.0`, `::`,
    /// or any routable IP).  The loopback-only pilot refuses it until
    /// the LAN token-resolver lands.
    #[error(
        "RS-2 RestHttp listener for fn {fiscal_number}: bind {ip} is non-loopback; the \
         loopback-only pilot refuses it until the LAN token-resolver lands (RS-2 §0.4 D2)"
    )]
    NonLoopbackBind {
        fiscal_number: String,
        ip: std::net::IpAddr,
    },
}

impl ListenerCfg {
    /// Parse [`listen_addr`](Self::listen_addr) as an [`IpAddr`].
    /// Fail-closed: a hostname (`localhost`) or garbage does NOT parse
    /// and surfaces as [`ListenerBindError::UnparseableAddr`] — we never
    /// fall back to a string compare that could mis-classify `::1` /
    /// `localhost` / `0.0.0.0`.
    ///
    /// [`IpAddr`]: std::net::IpAddr
    pub fn bind_ip(&self) -> Result<std::net::IpAddr, ListenerBindError> {
        self.listen_addr.parse::<std::net::IpAddr>().map_err(|_| {
            ListenerBindError::UnparseableAddr {
                fiscal_number: self.fiscal_number.clone(),
                listen_addr: self.listen_addr.clone(),
            }
        })
    }
}

/// RS-2 D2 (loopback-only pilot) fail-closed guard — every `RestHttp`
/// listener MUST bind a loopback address.  Run at supervisor startup
/// (alongside `BindingsRegistry::build_from_db`, §0.4 CF3) BEFORE any
/// listener binds.  Non-`RestHttp` listeners (the legacy Maria/XML-RPC
/// TCP shells) are out of RS-2 scope and not checked here.
///
/// `IpAddr::is_loopback()` is the classifier: true for `127.0.0.0/8` +
/// `::1`; false for `0.0.0.0` / `::` (`UNSPECIFIED`, all-interfaces) —
/// the footgun is correctly refused.
pub fn validate_rs2_loopback_binds(listeners: &[ListenerCfg]) -> Result<(), ListenerBindError> {
    for l in listeners {
        if l.kind != ListenerKind::RestHttp {
            continue;
        }
        let ip = l.bind_ip()?;
        if !ip.is_loopback() {
            return Err(ListenerBindError::NonLoopbackBind {
                fiscal_number: l.fiscal_number.clone(),
                ip,
            });
        }
    }
    Ok(())
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
#[derive(Debug, Clone, Deserialize)]
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

    /// Offline-backlog drain-ticker cadence (seconds).  **Raw value** —
    /// route boot callers through
    /// [`SupervisorCfg::clamped_drain_interval_seconds`].  The per-FN
    /// exponential-backoff gating lives inside
    /// `drain_offline_backlog_scheduled`, so this is just the wake cadence.
    #[serde(default = "default_drain_interval_seconds")]
    pub drain_interval_seconds: u64,

    /// Graceful-shutdown grace window (seconds) — the max time the
    /// supervisor waits for a tick loop to finish its in-flight pass after
    /// the shutdown signal before proceeding to `drop(App)` (F2).  **Raw
    /// value** — route boot callers through
    /// [`SupervisorCfg::clamped_shutdown_grace_seconds`].  On elapse the
    /// supervisor proceeds anyway: the per-doc W9b drain is crash-safe, so a
    /// cut between docs (or mid-DPS-call via idempotency) is crash-equivalent
    /// and re-drained on next boot.  **Deployment:** set the orchestrator
    /// stop timeout (systemd `TimeoutStopSec`, docker `stop_grace_period`)
    /// GREATER than this value so the grace path runs before SIGKILL.
    #[serde(default = "default_shutdown_grace_seconds")]
    pub shutdown_grace_seconds: u64,

    /// B1 online-convergence tick cadence (seconds).  **Raw value** — route
    /// boot callers through
    /// [`SupervisorCfg::clamped_online_convergence_interval_seconds`].  Like
    /// the drain ticker this is just the wake cadence; the convergence tick is
    /// SELECT-first (an empty tick issues zero wire calls), so it carries no
    /// per-FN backoff gating.
    #[serde(default = "default_online_convergence_interval_seconds")]
    pub online_convergence_interval_seconds: u64,
}

impl Default for SupervisorCfg {
    /// Hand-written (NOT derived) so a missing `[supervisor]` table yields
    /// the same field values as a present table with the fields omitted (a
    /// derived `Default` would give `0` → clamped to the floor).
    fn default() -> Self {
        Self {
            enabled: false,
            dps: DpsCfg::default(),
            drain_interval_seconds: default_drain_interval_seconds(),
            shutdown_grace_seconds: default_shutdown_grace_seconds(),
            online_convergence_interval_seconds: default_online_convergence_interval_seconds(),
        }
    }
}

fn default_drain_interval_seconds() -> u64 {
    60
}

fn default_shutdown_grace_seconds() -> u64 {
    // Aligned with the default DPS request timeout (30s) so the out-of-box
    // config does NOT trip the startup "grace < dps timeout" WARN — an
    // in-flight drain DPS call can finish within grace on the default config.
    30
}

/// Inclusive clamp bounds for the drain-ticker cadence.
pub const DRAIN_INTERVAL_MIN_SECONDS: u64 = 5;
pub const DRAIN_INTERVAL_MAX_SECONDS: u64 = 3600;

/// Inclusive clamp bounds for the shutdown grace window.  Upper bound is
/// kept at the systemd default `TimeoutStopSec` (90s) minus headroom so a
/// default unit file still SIGTERMs cleanly; operators who raise
/// `TimeoutStopSec` can raise the grace to match.
pub const SHUTDOWN_GRACE_MIN_SECONDS: u64 = 1;
pub const SHUTDOWN_GRACE_MAX_SECONDS: u64 = 80;

fn default_online_convergence_interval_seconds() -> u64 {
    60
}

/// Inclusive clamp bounds for the online-convergence tick cadence.  Floor is
/// higher than the drain floor (5s) — convergence is a slow OCF-5 backstop for
/// resting `SENT`/`KVT1`, not a latency-sensitive path, so 15s avoids needless
/// wire chatter while still bounding the worst-case KVT1-hang dwell.
pub const ONLINE_CONVERGENCE_INTERVAL_MIN_SECONDS: u64 = 15;
pub const ONLINE_CONVERGENCE_INTERVAL_MAX_SECONDS: u64 = 3600;

impl SupervisorCfg {
    /// Validate + clamp `drain_interval_seconds` to
    /// `[DRAIN_INTERVAL_MIN_SECONDS, DRAIN_INTERVAL_MAX_SECONDS]`.  Returns
    /// `(clamped_value, was_clamped)` for the WARN-audit pattern.
    pub fn clamped_drain_interval_seconds(&self) -> (u64, bool) {
        let raw = self.drain_interval_seconds;
        let clamped = raw.clamp(DRAIN_INTERVAL_MIN_SECONDS, DRAIN_INTERVAL_MAX_SECONDS);
        (clamped, clamped != raw)
    }

    /// Validate + clamp `shutdown_grace_seconds` to
    /// `[SHUTDOWN_GRACE_MIN_SECONDS, SHUTDOWN_GRACE_MAX_SECONDS]`.  Returns
    /// `(clamped_value, was_clamped)` for the WARN-audit pattern.
    pub fn clamped_shutdown_grace_seconds(&self) -> (u64, bool) {
        let raw = self.shutdown_grace_seconds;
        let clamped = raw.clamp(SHUTDOWN_GRACE_MIN_SECONDS, SHUTDOWN_GRACE_MAX_SECONDS);
        (clamped, clamped != raw)
    }

    /// Validate + clamp `online_convergence_interval_seconds` to
    /// `[ONLINE_CONVERGENCE_INTERVAL_MIN_SECONDS,
    /// ONLINE_CONVERGENCE_INTERVAL_MAX_SECONDS]`.  Returns
    /// `(clamped_value, was_clamped)` for the WARN-audit pattern — mirror of
    /// [`SupervisorCfg::clamped_drain_interval_seconds`].
    pub fn clamped_online_convergence_interval_seconds(&self) -> (u64, bool) {
        let raw = self.online_convergence_interval_seconds;
        let clamped = raw.clamp(
            ONLINE_CONVERGENCE_INTERVAL_MIN_SECONDS,
            ONLINE_CONVERGENCE_INTERVAL_MAX_SECONDS,
        );
        (clamped, clamped != raw)
    }
}

/// RS-4 (audit pass-2, item 4) — production backup / retention config.  Mirror
/// of the `[supervisor]` clamp/default pattern.  The `[backup]` section is
/// optional and default-ENABLED (a config with no section still gets hourly
/// snapshots of both DB files with sane retention).
#[derive(Debug, Clone, Deserialize)]
pub struct BackupCfg {
    /// Master switch.  `false` → the supervisor never spawns the backup loop.
    #[serde(default = "default_backup_enabled")]
    pub enabled: bool,
    /// Directory snapshots are written to (created if missing).  A path on a
    /// SECOND physical device is strongly recommended — see
    /// `docs/operations/RETENTION_POLICY.md`.
    #[serde(default = "default_backup_dir")]
    pub dir: PathBuf,
    /// Snapshot cadence (seconds).  **Raw value** — route callers through
    /// [`BackupCfg::clamped_interval_seconds`].
    #[serde(default = "default_backup_interval_seconds")]
    pub interval_seconds: u64,
    /// Retain at least this many most-recent snapshots per label, regardless of
    /// age.
    #[serde(default = "default_backup_keep_last")]
    pub keep_last: usize,
    /// Prune snapshots older than this many days — but only those ALSO beyond
    /// `keep_last` (the two conditions are ANDed: a snapshot survives if it is
    /// within the keep_last window OR younger than `max_age_days`).
    #[serde(default = "default_backup_max_age_days")]
    pub max_age_days: u64,
}

impl Default for BackupCfg {
    /// Hand-written (NOT derived) so a missing `[backup]` table yields the same
    /// field values as a present table with the fields omitted.
    fn default() -> Self {
        Self {
            enabled: default_backup_enabled(),
            dir: default_backup_dir(),
            interval_seconds: default_backup_interval_seconds(),
            keep_last: default_backup_keep_last(),
            max_age_days: default_backup_max_age_days(),
        }
    }
}

fn default_backup_enabled() -> bool {
    true
}
fn default_backup_dir() -> PathBuf {
    PathBuf::from("var/backups")
}
fn default_backup_interval_seconds() -> u64 {
    3600
}
fn default_backup_keep_last() -> usize {
    30
}
fn default_backup_max_age_days() -> u64 {
    14
}

/// Inclusive clamp bounds for the backup snapshot cadence (5 min … 1 day).
pub const BACKUP_INTERVAL_MIN_SECONDS: u64 = 300;
pub const BACKUP_INTERVAL_MAX_SECONDS: u64 = 86_400;

impl BackupCfg {
    /// Validate + clamp `interval_seconds` to
    /// `[BACKUP_INTERVAL_MIN_SECONDS, BACKUP_INTERVAL_MAX_SECONDS]`.  Returns
    /// `(clamped_value, was_clamped)` for the WARN pattern — mirror of
    /// [`SupervisorCfg::clamped_drain_interval_seconds`].
    pub fn clamped_interval_seconds(&self) -> (u64, bool) {
        let raw = self.interval_seconds;
        let clamped = raw.clamp(BACKUP_INTERVAL_MIN_SECONDS, BACKUP_INTERVAL_MAX_SECONDS);
        (clamped, clamped != raw)
    }
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

    // ─── RS-2 D2 — loopback-only bind guard (piece-1) ─────────────

    fn rest_listener(fn_id: &str, addr: &str) -> ListenerCfg {
        ListenerCfg {
            kind: ListenerKind::RestHttp,
            port: 8080,
            driver_id: "drv".to_string(),
            fiscal_number: fn_id.to_string(),
            listen_addr: addr.to_string(),
        }
    }

    /// `listen_addr` is optional in TOML and defaults to loopback.
    #[test]
    fn listen_addr_defaults_to_loopback() {
        let toml = r#"
            app_name = "prro"
            version = "0.1.0"
            [database]
            db_path = "var/prro.db"
            secure_db_path = "var/secure.db"
            [admin_ui]
            enabled = false
            listen = "127.0.0.1:8081"
            [[listeners]]
            type = "rest_http"
            port = 8080
            driver_id = "drv"
            fn = "4000000001"
        "#;
        let cfg = AppConfig::from_toml(toml).expect("parse must succeed");
        assert_eq!(cfg.listeners[0].listen_addr, "127.0.0.1");
    }

    /// Loopback v4 (`127.0.0.0/8`) and v6 (`::1`) pass the D2 guard.
    #[test]
    fn rs2_loopback_guard_accepts_ipv4_and_ipv6_loopback() {
        for addr in ["127.0.0.1", "127.0.0.5", "::1"] {
            let ls = [rest_listener("4000000001", addr)];
            validate_rs2_loopback_binds(&ls)
                .unwrap_or_else(|e| panic!("loopback {addr} must pass: {e}"));
        }
    }

    /// `0.0.0.0` / `::` (UNSPECIFIED, all-interfaces) and routable IPs
    /// are refused — the all-interfaces footgun is correctly closed.
    #[test]
    fn rs2_loopback_guard_refuses_unspecified_and_routable() {
        for addr in ["0.0.0.0", "::", "192.168.1.5", "10.0.0.1"] {
            let ls = [rest_listener("4000000001", addr)];
            let err = validate_rs2_loopback_binds(&ls).expect_err("non-loopback must be refused");
            assert!(
                matches!(err, ListenerBindError::NonLoopbackBind { .. }),
                "addr {addr}: expected NonLoopbackBind, got {err:?}"
            );
        }
    }

    /// A hostname does not parse as `IpAddr` → fail-closed (never a
    /// string compare that could mis-classify `localhost`).
    #[test]
    fn rs2_loopback_guard_rejects_hostname_fail_closed() {
        let ls = [rest_listener("4000000001", "localhost")];
        let err = validate_rs2_loopback_binds(&ls).expect_err("hostname must fail-closed");
        assert!(
            matches!(err, ListenerBindError::UnparseableAddr { .. }),
            "expected UnparseableAddr, got {err:?}"
        );
    }

    /// Non-`RestHttp` listeners (legacy Maria/XML-RPC TCP shells) are
    /// out of RS-2 scope and not subject to the D2 loopback guard.
    #[test]
    fn rs2_loopback_guard_ignores_non_rest_http_listeners() {
        let ls = [ListenerCfg {
            kind: ListenerKind::Maria304Tcp,
            port: 9000,
            driver_id: "drv".to_string(),
            fiscal_number: "4000000001".to_string(),
            listen_addr: "0.0.0.0".to_string(),
        }];
        validate_rs2_loopback_binds(&ls).expect("non-RestHttp must be ignored by the D2 guard");
    }

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
            cfg.supervisor
                .require_dps_endpoint()
                .expect("disabled = Ok"),
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

    /// RS-1 Piece 1 (self-review LOW) — a DISABLED supervisor ignores a
    /// set endpoint: `require_dps_endpoint` short-circuits to `Ok(None)`
    /// so the binary stays M1-idle regardless of `[supervisor.dps]`.
    #[test]
    fn supervisor_disabled_ignores_set_endpoint() {
        let toml = format!(
            "{BASE_CFG}\n[supervisor]\nenabled = false\n[supervisor.dps]\nendpoint = \"https://cabinet.tax.gov.ua:9443\"\n"
        );
        let cfg = AppConfig::from_toml(&toml).expect("parse must succeed");
        assert!(!cfg.supervisor.enabled);
        assert_eq!(
            cfg.supervisor
                .require_dps_endpoint()
                .expect("disabled = Ok(None)"),
            None,
            "disabled supervisor ignores a set endpoint",
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

    /// RS-1 Piece 5a — drain-ticker cadence clamps to bounds; the
    /// hand-written Default keeps it in-range whether or not the table is
    /// present.
    #[test]
    fn drain_interval_clamps_and_defaults_in_range() {
        let sup = SupervisorCfg {
            enabled: true,
            dps: DpsCfg::default(),
            drain_interval_seconds: 0,
            shutdown_grace_seconds: default_shutdown_grace_seconds(),
            online_convergence_interval_seconds: default_online_convergence_interval_seconds(),
        };
        let (clamped, was_clamped) = sup.clamped_drain_interval_seconds();
        assert_eq!(clamped, DRAIN_INTERVAL_MIN_SECONDS);
        assert!(was_clamped);

        let (def, def_clamped) = SupervisorCfg::default().clamped_drain_interval_seconds();
        assert_eq!(def, default_drain_interval_seconds());
        assert!(!def_clamped);
    }

    /// RS-1 F2 — shutdown grace window clamps to bounds; the hand-written
    /// Default keeps it in-range whether or not the table is present.
    #[test]
    fn shutdown_grace_clamps_and_defaults_in_range() {
        let sup = SupervisorCfg {
            enabled: true,
            dps: DpsCfg::default(),
            drain_interval_seconds: default_drain_interval_seconds(),
            shutdown_grace_seconds: 0,
            online_convergence_interval_seconds: default_online_convergence_interval_seconds(),
        };
        let (clamped, was_clamped) = sup.clamped_shutdown_grace_seconds();
        assert_eq!(clamped, SHUTDOWN_GRACE_MIN_SECONDS);
        assert!(was_clamped);

        let too_big = SupervisorCfg {
            enabled: true,
            dps: DpsCfg::default(),
            drain_interval_seconds: default_drain_interval_seconds(),
            shutdown_grace_seconds: 99_999,
            online_convergence_interval_seconds: default_online_convergence_interval_seconds(),
        };
        let (clamped, was_clamped) = too_big.clamped_shutdown_grace_seconds();
        assert_eq!(clamped, SHUTDOWN_GRACE_MAX_SECONDS);
        assert!(was_clamped);

        let (def, def_clamped) = SupervisorCfg::default().clamped_shutdown_grace_seconds();
        assert_eq!(def, default_shutdown_grace_seconds());
        assert!(!def_clamped);
    }

    /// B1 — online-convergence tick cadence clamps to bounds; the hand-written
    /// Default keeps it in-range whether or not the table is present.  Mirror
    /// of [`drain_interval_clamps_and_defaults_in_range`] (spec §5 test 9).
    #[test]
    fn online_convergence_interval_clamps_and_defaults_in_range() {
        // Below floor → clamped up to MIN.
        let too_small = SupervisorCfg {
            enabled: true,
            dps: DpsCfg::default(),
            drain_interval_seconds: default_drain_interval_seconds(),
            shutdown_grace_seconds: default_shutdown_grace_seconds(),
            online_convergence_interval_seconds: 1,
        };
        let (clamped, was_clamped) = too_small.clamped_online_convergence_interval_seconds();
        assert_eq!(clamped, ONLINE_CONVERGENCE_INTERVAL_MIN_SECONDS);
        assert_eq!(clamped, 15);
        assert!(was_clamped);

        // Above ceiling → clamped down to MAX.
        let too_big = SupervisorCfg {
            enabled: true,
            dps: DpsCfg::default(),
            drain_interval_seconds: default_drain_interval_seconds(),
            shutdown_grace_seconds: default_shutdown_grace_seconds(),
            online_convergence_interval_seconds: 9999,
        };
        let (clamped, was_clamped) = too_big.clamped_online_convergence_interval_seconds();
        assert_eq!(clamped, ONLINE_CONVERGENCE_INTERVAL_MAX_SECONDS);
        assert_eq!(clamped, 3600);
        assert!(was_clamped);

        // Default (missing or omitted table) → 60, in range, not clamped.
        let (def, def_clamped) =
            SupervisorCfg::default().clamped_online_convergence_interval_seconds();
        assert_eq!(def, default_online_convergence_interval_seconds());
        assert_eq!(def, 60);
        assert!(!def_clamped);
    }

    /// RS-4 — backup snapshot cadence clamps to bounds; the hand-written
    /// Default keeps it in range whether or not the `[backup]` table is present
    /// (spec test 7).
    #[test]
    fn backup_interval_clamps_and_defaults_in_range() {
        let too_small = BackupCfg {
            interval_seconds: 1,
            ..BackupCfg::default()
        };
        let (clamped, was_clamped) = too_small.clamped_interval_seconds();
        assert_eq!(clamped, BACKUP_INTERVAL_MIN_SECONDS);
        assert_eq!(clamped, 300);
        assert!(was_clamped);

        let too_big = BackupCfg {
            interval_seconds: 999_999,
            ..BackupCfg::default()
        };
        let (clamped, was_clamped) = too_big.clamped_interval_seconds();
        assert_eq!(clamped, BACKUP_INTERVAL_MAX_SECONDS);
        assert_eq!(clamped, 86_400);
        assert!(was_clamped);

        let def = BackupCfg::default();
        let (clamped, def_clamped) = def.clamped_interval_seconds();
        assert_eq!(clamped, default_backup_interval_seconds());
        assert_eq!(clamped, 3600);
        assert!(!def_clamped);

        // Section-absent defaults: enabled + sane retention.
        assert!(def.enabled, "backup is default-enabled");
        assert_eq!(def.dir, std::path::PathBuf::from("var/backups"));
        assert_eq!(def.keep_last, 30);
        assert_eq!(def.max_age_days, 14);
    }
}
