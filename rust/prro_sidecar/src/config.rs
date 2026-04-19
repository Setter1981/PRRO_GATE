//! TOML configuration parser for sidecar.toml.
//! Phase 4.3 implements full SidecarConfig struct; this is a Phase 0 stub.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialsMode {
    #[default]
    XorSoft,
    Plain,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SidecarConfig {
    pub db:       DbSection,
    pub security: SecuritySection,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DbSection {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SecuritySection {
    #[serde(default)]
    pub credentials_mode: CredentialsMode,
}
