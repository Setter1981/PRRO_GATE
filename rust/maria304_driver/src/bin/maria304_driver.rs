//! `maria304_driver` binary — wires config, listeners, bridge,
//! observability, and admin API into a single process.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::signal;

use maria304_driver::admin::{serve_admin, AdminConfig, Registry, RegisteredFn};
use maria304_driver::bridge::{
    Bridge, DeploymentMode, DryRunBridge, HttpBridge, RetryBridge, RetryPolicy,
};
use maria304_driver::listener::session_loop::ClockSource;
use maria304_driver::listener::{FnListener, ListenerConfig};
use maria304_driver::observability::{init_json_subscriber, SessionMetrics};
use maria304_driver::session::dispatcher::Identity;

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields parsed for forward-compatibility even if not wired yet.
struct FileConfig {
    bridge: BridgeCfg,
    #[serde(default)]
    observability: Option<ObservabilityCfg>,
    #[serde(default)]
    admin: Option<AdminCfg>,
    #[serde(default)]
    deployment: DeploymentCfg,
    listeners: Vec<ListenerCfg>,
}

#[derive(Debug, Deserialize)]
struct BridgeCfg {
    gateway_url: String,
    #[serde(default)]
    shared_token: String,
    #[serde(default = "default_connect_timeout_ms")]
    connect_timeout_ms: u64,
    #[serde(default = "default_request_timeout_ms")]
    request_timeout_ms: u64,
    #[serde(default)]
    retry: Option<RetryCfg>,
}

#[derive(Debug, Deserialize)]
struct RetryCfg {
    #[serde(default = "default_retry_attempts")]
    max_attempts: u32,
    #[serde(default = "default_backoff_base_ms")]
    initial_backoff_ms: u64,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct ObservabilityCfg {
    #[serde(default)]
    log_format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdminCfg {
    bind: String,
    #[serde(default)]
    auth_token: String,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct DeploymentCfg {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default = "default_shutdown_s")]
    graceful_shutdown_timeout_s: u64,
}

#[derive(Debug, Deserialize, Clone)]
struct ListenerCfg {
    fiscal_number: String,
    bind: String,
    #[serde(default)]
    identity: Option<IdentityCfg>,
    #[serde(default = "default_idle_timeout_s")]
    idle_timeout_s: u64,
    #[serde(default = "default_cooldown_s")]
    post_disconnect_cooldown_s: u64,
    #[serde(default)]
    cashier_password: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[allow(dead_code)]
struct IdentityCfg {
    #[serde(default)]
    serial_number: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    software_version: Option<String>,
    #[serde(default)]
    build_date: Option<String>,
}

const fn default_connect_timeout_ms() -> u64 {
    5_000
}
const fn default_request_timeout_ms() -> u64 {
    15_000
}
const fn default_retry_attempts() -> u32 {
    3
}
const fn default_backoff_base_ms() -> u64 {
    500
}
const fn default_shutdown_s() -> u64 {
    30
}
const fn default_idle_timeout_s() -> u64 {
    300
}
const fn default_cooldown_s() -> u64 {
    3
}

struct UtcNowClock;

impl ClockSource for UtcNowClock {
    fn now(&self) -> (String, String) {
        // Minimal stdlib-only UTC — format ggggmmdd / hhmmss.  A
        // production build would use chrono for DST + timezone; for
        // the first pilot we run in UTC and display DPS TZ on the
        // Python side where chrono is already in the dep graph.
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (date, time) = format_utc(secs);
        (date, time)
    }
}

// Inline gregorian conversion — Howard Hinnant's algorithm, all u64.
#[allow(clippy::similar_names)]
fn format_utc(secs: u64) -> (String, String) {
    let day = secs / 86_400;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    // Days since 1970-01-01 → (y, m, d).  We stay in u64 throughout;
    // the algorithm only needs positive values for realistic timestamps.
    let z = day + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097; // 0..=146096
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (
        format!("{y:04}{m:02}{d:02}"),
        format!("{hour:02}{minute:02}{second:02}"),
    )
}

fn check_no_duplicates(listeners: &[ListenerCfg]) -> Result<(), String> {
    use std::collections::HashSet;
    let mut seen_fns: HashSet<&str> = HashSet::new();
    let mut seen_binds: HashSet<&str> = HashSet::new();
    for l in listeners {
        if !seen_fns.insert(l.fiscal_number.as_str()) {
            return Err(format!(
                "duplicate fiscal_number '{}' in [listeners] config — each listener must be unique",
                l.fiscal_number
            ));
        }
        if !seen_binds.insert(l.bind.as_str()) {
            return Err(format!(
                "duplicate bind address '{}' in [listeners] config — each listener must bind to a unique port",
                l.bind
            ));
        }
    }
    Ok(())
}

fn build_bridge(cfg: &BridgeCfg, mode: DeploymentMode) -> Result<Arc<dyn Bridge + Send + Sync>, String> {
    if mode == DeploymentMode::DryRun {
        return Ok(Arc::new(DryRunBridge::new()));
    }
    let http = HttpBridge::new(
        &cfg.gateway_url,
        &cfg.shared_token,
        Duration::from_millis(cfg.connect_timeout_ms),
        Duration::from_millis(cfg.request_timeout_ms),
    )
    .map_err(|e| format!("HttpBridge build failed: {e}"))?;

    let bridge: Arc<dyn Bridge + Send + Sync> = if let Some(ref retry) = cfg.retry {
        Arc::new(RetryBridge::new(
            http,
            RetryPolicy {
                max_attempts: retry.max_attempts,
                initial_backoff: Duration::from_millis(retry.initial_backoff_ms),
                ..RetryPolicy::default()
            },
        ))
    } else {
        Arc::new(http)
    };
    Ok(bridge)
}

fn identity_from_cfg(fiscal_number: &str, cfg: &ListenerCfg) -> Identity {
    let id = cfg.identity.clone().unwrap_or_default();
    let password = cfg
        .cashier_password
        .clone()
        .unwrap_or_else(|| "1111111111".to_string());
    Identity {
        factory_serial: id.serial_number.unwrap_or_else(|| format!("MRY304-{fiscal_number}")),
        fiscal_number: fiscal_number.to_string(),
        merchant: "Virtual Maria 304".to_string(),
        fw_version_id: id.software_version.unwrap_or_else(|| "4120".to_string()),
        fw_build_date: id.build_date.unwrap_or_else(|| "20250115".to_string()),
        currency: "Грн".to_string(),
        currency_date: "20250101".to_string(),
        decimals: 2,
        cashier_password: password,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = init_json_subscriber(); // idempotent — ok if caller already set one.

    let config_path = match std::env::var("MARIA304_CONFIG") {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from("/etc/maria304_driver/config.yaml"),
    };
    tracing::info!(path = %config_path.display(), "loading config");

    let raw = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("read config {}: {e}", config_path.display()))?;
    let cfg: FileConfig = serde_yaml::from_str(&raw)
        .map_err(|e| format!("parse config: {e}"))?;

    let mode = match cfg.deployment.mode.as_deref() {
        None => DeploymentMode::default(),
        Some(raw) => DeploymentMode::parse(raw)
            .ok_or_else(|| format!("invalid deployment.mode={raw:?}"))?,
    };
    tracing::info!(?mode, "deployment mode");

    if let Err(e) = check_no_duplicates(&cfg.listeners) {
        eprintln!("configuration error: {e}");
        std::process::exit(1);
    }

    let bridge = build_bridge(&cfg.bridge, mode)?;
    let registry = Arc::new(Registry::new());
    let clock: Arc<dyn ClockSource> = Arc::new(UtcNowClock);

    let shutdown = tokio_util::sync::CancellationToken::new();
    let mut listener_handles = Vec::new();

    // Spawn each listener.
    for listener_cfg in &cfg.listeners {
        let metrics = Arc::new(SessionMetrics::new());
        let identity = identity_from_cfg(&listener_cfg.fiscal_number, listener_cfg);
        let mut lcfg = ListenerConfig::new(
            listener_cfg.fiscal_number.clone(),
            listener_cfg.bind.clone(),
            identity,
        );
        lcfg.idle_timeout = Duration::from_secs(listener_cfg.idle_timeout_s);
        lcfg.cooldown = Duration::from_secs(listener_cfg.post_disconnect_cooldown_s);

        let listener = FnListener::new(lcfg, Arc::clone(&bridge), Arc::clone(&clock), Arc::clone(&metrics));
        let gate = listener.gate();
        registry
            .register_fn(RegisteredFn {
                fiscal_number: listener_cfg.fiscal_number.clone(),
                bind: listener_cfg.bind.clone(),
                gate,
                metrics: Arc::clone(&metrics),
            })
            .await;

        let token = shutdown.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = listener.serve(token).await {
                tracing::error!("listener died: {e}");
            }
        });
        listener_handles.push(handle);
    }

    // Admin HTTP server (optional).
    if let Some(admin) = cfg.admin {
        let registry = Arc::clone(&registry);
        tokio::spawn(async move {
            let acfg = AdminConfig { bind: admin.bind, auth_token: admin.auth_token };
            if let Err(e) = serve_admin(acfg, registry).await {
                tracing::error!("admin server died: {e}");
            }
        });
    }

    tracing::info!("maria304_driver running; Ctrl-C to stop");
    signal::ctrl_c().await?;
    tracing::info!("shutdown signal received; draining connections");
    shutdown.cancel();

    let timeout_s = cfg.deployment.graceful_shutdown_timeout_s;
    let _ = tokio::time::timeout(
        Duration::from_secs(timeout_s),
        async {
            for h in listener_handles {
                let _ = h.await;
            }
        },
    ).await;
    tracing::info!("shutdown complete");
    Ok(())
}

#[cfg(test)]
mod startup_validation_tests {
    use super::{check_no_duplicates, ListenerCfg};

    fn cfg(fn_: &str, bind: &str) -> ListenerCfg {
        ListenerCfg {
            fiscal_number: fn_.to_string(),
            bind: bind.to_string(),
            identity: None,
            idle_timeout_s: 300,
            post_disconnect_cooldown_s: 3,
            cashier_password: None,
        }
    }

    #[test]
    fn unique_configs_pass() {
        let cfgs = vec![cfg("FN001", "0.0.0.0:9001"), cfg("FN002", "0.0.0.0:9002")];
        assert!(check_no_duplicates(&cfgs).is_ok());
    }

    #[test]
    fn detect_duplicate_fiscal_number() {
        let cfgs = vec![cfg("FN001", "0.0.0.0:9001"), cfg("FN001", "0.0.0.0:9002")];
        let err = check_no_duplicates(&cfgs).unwrap_err();
        assert!(err.contains("FN001"), "expected FN001 in error: {err}");
    }

    #[test]
    fn detect_duplicate_bind() {
        let cfgs = vec![cfg("FN001", "0.0.0.0:9001"), cfg("FN002", "0.0.0.0:9001")];
        let err = check_no_duplicates(&cfgs).unwrap_err();
        assert!(err.contains("9001"), "expected port in error: {err}");
    }
}
