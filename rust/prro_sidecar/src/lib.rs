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

pub mod generated {
    //! tonic-generated gRPC stubs from proto/check.proto.
    #![allow(clippy::all)]
    tonic::include_proto!("com.programika.rro.ws.chk");
}
