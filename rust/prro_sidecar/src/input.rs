//! Minimal serde parsing of the canonical-command JSON posted by Python.
//! Only the subset required for XML build + license check is deserialized;
//! unknown fields pass through — Python-side additions must not break us.
//!
//! Phase 1.1 implements full structs; this is a Phase 0 stub.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct CanonicalCommand {
    pub schema_version: String,
    pub operation_type: String,
    pub fiscal_number:  String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
