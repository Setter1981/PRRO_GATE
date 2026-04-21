//! Daemon configuration — TOML-based, CLI override via clap.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Address to bind the HTTP listener on.  Loopback by default —
    /// operators must explicitly expose the port for LAN access.
    #[serde(default = "default_listen")]
    pub listen: String,

    /// Default TCP write/connect timeout for `/print` jobs (ms).
    #[serde(default = "default_timeout")]
    pub default_timeout_ms: u64,

    /// Optional directory with extra `*.xml` printer profiles to load
    /// alongside the bundled ones.
    #[serde(default)]
    pub profiles_dir: Option<PathBuf>,
}

fn default_listen() -> String {
    "127.0.0.1:9105".to_string()
}
fn default_timeout() -> u64 {
    5000
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            default_timeout_ms: default_timeout(),
            profiles_dir: None,
        }
    }
}

impl Config {
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(s)?)
    }
}
