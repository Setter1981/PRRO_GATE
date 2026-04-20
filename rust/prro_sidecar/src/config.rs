//! TOML configuration parser for sidecar.toml.

use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Top-level ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SidecarConfig {
    pub sidecar:  SidecarSection,
    pub db:       DbSection,
    pub dps:      DpsProfiles,
    #[serde(default)]
    pub certs:    CertsSection,
    #[serde(default)]
    pub security: SecuritySection,
    pub tsp:      Option<TspSection>,
    #[serde(default)]
    pub dev:      DevSection,
    pub license:  Option<LicenseSection>,
}

// ─── Sections ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SidecarSection {
    /// TCP bind address (e.g. "127.0.0.1:8765")
    pub bind:      String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Device name embedded in DPS XML <NDv> tag (default: "ПРО_каса")
    pub device_name:    Option<String>,
    /// Device version embedded in DPS XML <PrV> tag (default: "1.1")
    pub device_version: Option<String>,
}

fn default_log_level() -> String { "info".into() }

// ─── Certs section ───────────────────────────────────────────────────────────

/// Paths to certificate files used by the sidecar.
///
/// Both files are updated independently (e.g. via a weekly cron/systemd timer):
///   tls_chain  — `openssl s_client -connect prro.tax.gov.ua:443 -showcerts`
///   ca_bundle  — downloaded from https://acskidd.gov.ua/CACertificates.p7b
///
/// Per-endpoint overrides in [dps.prod] / [dps.test] take precedence over
/// these globals (DpsEndpoint.ca_bundle).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CertsSection {
    /// PEM certificate chain for the DPS gRPC TLS connection.
    /// Mirrors `C:\ProgramData\WebCheck\prro-tax-gov-ua-chain.pem`.
    #[serde(default = "default_tls_chain")]
    pub tls_chain: String,
    /// ACSK CA certificates bundle (p7b or PEM) for EDS chain validation.
    /// Primary source: acskidd.gov.ua (DPS's own CA).
    #[serde(default = "default_ca_bundle")]
    pub ca_bundle: String,
}

impl Default for CertsSection {
    fn default() -> Self {
        Self {
            tls_chain: default_tls_chain(),
            ca_bundle: default_ca_bundle(),
        }
    }
}

fn default_tls_chain() -> String {
    "/var/lib/prro_sidecar/certs/prro-tax-gov-ua-chain.pem".into()
}

fn default_ca_bundle() -> String {
    "/var/lib/prro_sidecar/certs/CACertificates.p7b".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DbSection {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DpsProfiles {
    pub prod: DpsEndpoint,
    pub test: DpsEndpoint,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DpsEndpoint {
    /// gRPC endpoint URI (e.g. "https://cabinet.tax.gov.ua:9443")
    pub endpoint:  String,
    /// Optional path to PEM CA bundle (for private CA or custom root)
    pub ca_bundle: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SecuritySection {
    #[serde(default)]
    pub credentials_mode: CredentialsMode,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CredentialsMode {
    /// XOR with SHA-256(valid_to + operator_name[1]) — default, cross-platform
    #[default]
    XorSoft,
    /// Raw password in DB — opt-out for WebCheck migration / debug
    Plain,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TspSection {
    #[serde(default = "default_tsp_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_tsp_timeout_ms() -> u64 { 5_000 }

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DevSection {
    /// Skip CMS signing (return raw cp1251 XML). Requires DEV_MODE env var.
    #[serde(default)]
    pub skip_sign:  bool,
    /// Pretty-print tracing logs instead of JSON
    #[serde(default)]
    pub log_pretty: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LicenseSection {
    pub payload_b64:   String,
    pub signature_b64: String,
}

// ─── Load / validate ─────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("read {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("TOML parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("validation: {0}")]
    Validation(String),
}

impl SidecarConfig {
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::from_toml_str(&content)
    }

    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let cfg: SidecarConfig = toml::from_str(s)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn to_toml_string(&self) -> String {
        toml::to_string(self).expect("SidecarConfig always serializes")
    }

    fn validate(&self) -> Result<(), ConfigError> {
        // bind must be a valid socket address
        self.sidecar.bind.parse::<std::net::SocketAddr>().map_err(|e| {
            ConfigError::Validation(format!("sidecar.bind {:?}: {e}", self.sidecar.bind))
        })?;

        // db.path must be non-empty
        if self.db.path.trim().is_empty() {
            return Err(ConfigError::Validation("db.path must not be empty".into()));
        }

        // dps.prod.endpoint must be non-empty (required even in test-only deploys)
        if self.dps.prod.endpoint.trim().is_empty() {
            return Err(ConfigError::Validation("dps.prod.endpoint must not be empty".into()));
        }
        if self.dps.test.endpoint.trim().is_empty() {
            return Err(ConfigError::Validation("dps.test.endpoint must not be empty".into()));
        }

        // dev.skip_sign bypasses CMS signing — only allowed when DEV_MODE env var is set
        if self.dev.skip_sign && std::env::var("DEV_MODE").is_err() {
            return Err(ConfigError::Validation(
                "dev.skip_sign = true requires DEV_MODE env var to be set".into(),
            ));
        }

        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-wide env vars (DEV_MODE).
    /// Rust test threads share the same process; concurrent env mutation causes
    /// non-deterministic failures when the remove_var and set_var calls interleave.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    const EXAMPLE_TOML: &str = r#"
[sidecar]
bind = "127.0.0.1:8765"
log_level = "info"

[db]
path = "/var/lib/prro_sidecar/prro.db"

[dps.prod]
endpoint = "https://cabinet.tax.gov.ua:9443"

[dps.test]
endpoint = "https://dev-cabinet.tax.gov.ua:9443"

[certs]
tls_chain = "/var/lib/prro_sidecar/certs/prro-tax-gov-ua-chain.pem"
ca_bundle = "/var/lib/prro_sidecar/certs/CACertificates.p7b"

[security]
credentials_mode = "xor_soft"

[tsp]
timeout_ms = 5000

[dev]
skip_sign = false
log_pretty = false
"#;

    #[test]
    fn parse_example_config() {
        let cfg = SidecarConfig::from_toml_str(EXAMPLE_TOML).expect("parse must succeed");
        assert_eq!(cfg.sidecar.bind, "127.0.0.1:8765");
        assert_eq!(cfg.db.path, "/var/lib/prro_sidecar/prro.db");
        assert_eq!(cfg.dps.prod.endpoint, "https://cabinet.tax.gov.ua:9443");
        assert_eq!(cfg.security.credentials_mode, CredentialsMode::XorSoft);
        assert_eq!(cfg.tsp.as_ref().unwrap().timeout_ms, 5000);
        assert!(!cfg.dev.skip_sign);
    }

    #[test]
    fn roundtrip_toml() {
        let cfg1 = SidecarConfig::from_toml_str(EXAMPLE_TOML).expect("parse");
        let toml_str = cfg1.to_toml_string();
        let cfg2 = SidecarConfig::from_toml_str(&toml_str).expect("re-parse");
        assert_eq!(cfg1.sidecar.bind, cfg2.sidecar.bind);
        assert_eq!(cfg1.db.path, cfg2.db.path);
        assert_eq!(cfg1.dps.prod.endpoint, cfg2.dps.prod.endpoint);
        assert_eq!(cfg1.dps.test.endpoint, cfg2.dps.test.endpoint);
        assert_eq!(cfg1.security.credentials_mode, cfg2.security.credentials_mode);
    }

    #[test]
    fn defaults_apply() {
        let minimal = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let cfg = SidecarConfig::from_toml_str(minimal).expect("parse minimal");
        assert_eq!(cfg.sidecar.log_level, "info");
        assert_eq!(cfg.security.credentials_mode, CredentialsMode::XorSoft);
        assert!(cfg.tsp.is_none());
        assert!(!cfg.dev.skip_sign);
        // cert defaults
        assert_eq!(cfg.certs.tls_chain, "/var/lib/prro_sidecar/certs/prro-tax-gov-ua-chain.pem");
        assert_eq!(cfg.certs.ca_bundle, "/var/lib/prro_sidecar/certs/CACertificates.p7b");
    }

    #[test]
    fn certs_section_overridable() {
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
[certs]
tls_chain = "/opt/certs/dps-chain.pem"
ca_bundle = "/opt/certs/acsk-ca.p7b"
"#;
        let cfg = SidecarConfig::from_toml_str(toml).expect("parse");
        assert_eq!(cfg.certs.tls_chain, "/opt/certs/dps-chain.pem");
        assert_eq!(cfg.certs.ca_bundle, "/opt/certs/acsk-ca.p7b");
    }

    #[test]
    fn invalid_bind_address_rejected() {
        let bad = r#"
[sidecar]
bind = "not-a-socket-addr"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let err = SidecarConfig::from_toml_str(bad).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)), "expected Validation error, got {err}");
    }

    #[test]
    fn missing_dps_prod_rejected() {
        let no_prod = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        // No [dps.prod] → toml parse should fail (prod is required field)
        assert!(SidecarConfig::from_toml_str(no_prod).is_err());
    }

    #[test]
    fn plain_credentials_mode_parses() {
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
[security]
credentials_mode = "plain"
"#;
        let cfg = SidecarConfig::from_toml_str(toml).expect("parse");
        assert_eq!(cfg.security.credentials_mode, CredentialsMode::Plain);
    }

    // ── dev.skip_sign guard ───────────────────────────────────────────────────

    #[test]
    fn dev_skip_sign_without_dev_mode_env_rejected() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::remove_var("DEV_MODE");
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
[dev]
skip_sign = true
"#;
        let err = SidecarConfig::from_toml_str(toml).unwrap_err();
        assert!(
            matches!(err, ConfigError::Validation(_)),
            "skip_sign=true without DEV_MODE must produce Validation error, got {err}"
        );
        assert!(
            err.to_string().contains("DEV_MODE"),
            "error must mention DEV_MODE, got: {err}"
        );
    }

    #[test]
    fn dev_skip_sign_with_dev_mode_env_accepted() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("DEV_MODE", "1");
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
[dev]
skip_sign = true
"#;
        let result = SidecarConfig::from_toml_str(toml);
        std::env::remove_var("DEV_MODE"); // cleanup
        assert!(result.is_ok(), "skip_sign=true WITH DEV_MODE must succeed: {result:?}");
        assert!(result.unwrap().dev.skip_sign);
    }

    // ── db.path validation ────────────────────────────────────────────────────

    #[test]
    fn empty_db_path_rejected() {
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = ""
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let err = SidecarConfig::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)), "empty db.path must be rejected: {err}");
    }

    #[test]
    fn whitespace_only_db_path_rejected() {
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "   "
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let err = SidecarConfig::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)), "whitespace db.path must be rejected: {err}");
    }

    // ── dps endpoint validation ───────────────────────────────────────────────

    #[test]
    fn empty_dps_prod_endpoint_rejected() {
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = ""
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let err = SidecarConfig::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)), "empty prod endpoint must be rejected: {err}");
    }

    #[test]
    fn empty_dps_test_endpoint_rejected() {
        let toml = r#"
[sidecar]
bind = "127.0.0.1:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = ""
"#;
        let err = SidecarConfig::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)), "empty test endpoint must be rejected: {err}");
    }

    // ── bind address parsing ──────────────────────────────────────────────────

    #[test]
    fn bind_address_without_port_rejected() {
        let toml = r#"
[sidecar]
bind = "127.0.0.1"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let err = SidecarConfig::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)), "bind without port must be rejected: {err}");
    }

    #[test]
    fn bind_address_port_out_of_range_rejected() {
        // Port 99999 > 65535 — SocketAddr parse must fail.
        let toml = r#"
[sidecar]
bind = "127.0.0.1:99999"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let err = SidecarConfig::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)), "port >65535 must be rejected: {err}");
    }

    #[test]
    fn bind_ipv6_loopback_accepted() {
        let toml = r#"
[sidecar]
bind = "[::1]:8765"
[db]
path = "/tmp/prro.db"
[dps.prod]
endpoint = "https://prod.example.com:9443"
[dps.test]
endpoint = "https://test.example.com:9443"
"#;
        let cfg = SidecarConfig::from_toml_str(toml).expect("IPv6 loopback must be accepted");
        assert_eq!(cfg.sidecar.bind, "[::1]:8765");
    }
}
