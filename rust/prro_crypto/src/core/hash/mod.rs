//! Hash function implementations for DSTU fiscal signing.
//!
//! Currently implements GOST 34.311-95 (the default for v1 CMS profile).
//! Kupyna (DSTU 7564:2014) is deferred to v1.x.
//!
//! ## Byte-identical parity
//!
//! Output of `gost_34_311_95(data)` must match jkurwa's `gost89.gosthash(data)`
//! for any input. Verified via 21-vector test suite in
//! `tests/vectors/gost3411_jkurwa.json`.

pub mod gost28147;
pub mod gost3411;

pub use gost3411::gost_34_311_95;
