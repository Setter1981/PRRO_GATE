//! `core` — universal DSTU 4145 cryptographic primitives.
//!
//! This layer is deliberately free of PRRO / fiscal / ДПС specifics. It
//! implements:
//!
//! - binary-field arithmetic over GF(2^m) (`gf2m`, `gf2m_257`)
//! - fixed-size field-element types (`field`, `fe`)
//! - scalar arithmetic mod curve order (`scalar`)
//! - elliptic-curve point operations, affine + projective (`curve`, `point`,
//!   `proj`, `wnaf`, `fixed_base`)
//! - DSTU 4145-LE sign/verify (`sign`)
//! - Ukrainian hash functions (`hash`: GOST 34.311-95; Kupyna TBD)
//! - SIMD dispatch for field multiplication (`backend`)
//!
//! Anything application-specific — fiscal receipt framing, JKS loading
//! for ДПС certs, PDF/XML packaging, HTTP transport — lives in sibling
//! modules (`cms`, `interop`, `containers`).

// `gf2m`, `gf2m_257`, `fe` remain `pub` for the duration of the 0.1.x
// line because integration tests (tests/test_backend_differential.rs,
// tests/test_gf2m_vectors.rs) and benches (benches/crypto.rs) reach
// into them by path. Before 1.0 we intend to move the benches inside
// the crate (so they can see `pub(crate)`) and then privatise these.
pub mod gf2m;
pub mod gf2m_257;
pub mod field;
pub mod fe;
pub mod scalar;
pub mod curve;
pub mod point;
// Projective-coord helpers, wNAF tables, fixed-base precompute, and
// the Montgomery ladder are implementation details. Exposing them to
// external code would lock their signatures in for semver — they're
// inherently algorithm-plumbing that may restructure as we add
// curves or backends.
pub mod proj; // pub for benches — TODO: move bench inside crate
pub(crate) mod wnaf;
pub(crate) mod fixed_base;
pub(crate) mod mladder;
pub(crate) mod comb;
pub mod sign;
pub mod hash;
pub mod backend;
