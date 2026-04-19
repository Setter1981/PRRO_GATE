//! Hash function implementations for DSTU fiscal signing.
//!
//! Implements GOST 34.311-95 (default CMS profile) and
//! Kupyna-256/512 (DSTU 7564:2014, auto-detected from certificate).
//!
//! ## Byte-identical parity
//!
//! Output of `gost_34_311_95(data)` must match jkurwa's `gost89.gosthash(data)`
//! for any input. Verified via 21-vector test suite in
//! `tests/vectors/gost3411_jkurwa.json`.

pub mod gost28147;
pub mod gost3411;
pub mod kupyna;

pub use gost3411::gost_34_311_95;
pub use kupyna::{kupyna_256, kupyna_512};
