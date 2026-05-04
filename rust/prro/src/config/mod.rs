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

impl AppConfig {
    pub fn from_toml(s: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(s)?)
    }
}
