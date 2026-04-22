//! `prro_sidecar` — Rust fiscal driver sidecar.
//!
//! Owns the pipeline: canonical JSON → cp1251 XML → CMS sign (DSTU 4145) →
//! gRPC `sendChkV2` to ДПС. Uses `prro_crypto` for all cryptographic
//! primitives without modifying it.
//!
//! Frozen invariants (mirrors CLAUDE.md):
//!   - (1) No network / crypto calls inside a SQLite transaction.
//!   - (2) Per-FN single-writer enforced upstream by Python write_path.
//!   - (7) `schema_version` required on every incoming canonical envelope.
//!   - (10) Signing not bypassable except via explicit `dev.skip_sign` + env.

pub mod cp1251;
pub mod credentials;
pub mod errors;
pub mod input;
pub mod license;
pub mod time_utils;
pub mod xml_builder;

pub mod cms_adapter;
pub mod config;
pub mod grpc_client;
pub mod repo;

// ── Envelope integrity ────────────────────────────────────────────────────────

use sha2::{Digest, Sha256};
use crate::input::CanonicalCommand;

const ALLOWED_SCHEMA_VERSIONS: &[&str] = &["1.0"];

/// Validate envelope fields checkable before any DB/crypto work.
///
/// Checks (invariant 7):
///   1. `schema_version` is in the allowlist.
///   2. `payload_sha256` is mandatory — must be exactly 64 lowercase hex chars.
///   3. Recompute SHA-256 of the serialized `payload` and verify it matches.
///      F5: previously the check was skipped when `payload_sha256` was empty,
///      allowing unauthenticated payloads to bypass integrity validation.
pub fn validate_envelope(cmd: &CanonicalCommand) -> Result<(), String> {
    if !ALLOWED_SCHEMA_VERSIONS.contains(&cmd.schema_version.as_str()) {
        return Err(format!(
            "schema_version '{}' not in allowlist {:?}",
            cmd.schema_version, ALLOWED_SCHEMA_VERSIONS
        ));
    }
    // F5: payload_sha256 is now mandatory — reject empty or non-hex values.
    if cmd.payload_sha256.len() != 64
        || !cmd.payload_sha256.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!(
            "payload_sha256 must be a 64-char lowercase hex SHA-256 digest; \
             got {:?} ({} chars)",
            cmd.payload_sha256,
            cmd.payload_sha256.len()
        ));
    }
    let payload_bytes = serde_json::to_vec(&cmd.payload)
        .map_err(|e| format!("payload serialize: {e}"))?;
    let computed = hex::encode(Sha256::digest(&payload_bytes));
    if computed != cmd.payload_sha256 {
        return Err(format!(
            "payload_sha256 mismatch: expected '{}', computed '{computed}'",
            cmd.payload_sha256
        ));
    }
    Ok(())
}

pub mod generated {
    //! tonic-generated gRPC stubs from proto/check.proto.
    #![allow(clippy::all)]
    tonic::include_proto!("com.programika.rro.ws.chk");
}
