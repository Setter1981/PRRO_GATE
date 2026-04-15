//! prro_crypto — Ukrainian DSTU 4145 cryptography for PRRO fiscal gateway.
//!
//! Replaces the Node.js jkurwa sidecar used by Multi-Protocol PRRO Gateway
//! with a native Rust implementation, exposed via PyO3 for Python callers
//! and JNI for Android.
//!
//! ## Phase 2 progress
//! - Step 1: GF(2^m) polynomial arithmetic — DONE
//! - Step 2: EC curve params + point operations — DONE
//! - Step 3: DSTU 4145 sign/verify — pending
//! - Step 4: GOST 28147-89 cipher — pending
//! - Step 5: JKS / Key-6.dat / PFX key containers — pending
//! - Step 6: CMS/PKCS#7 SignedData builder — pending
//! - Step 7: PyO3 bindings for Python API — pending

pub mod gf2m;
pub mod field;
pub mod fe;          // Phase 2 / Commit 1: fixed-size field types
pub mod gf2m_257;    // Phase 2 / Commit 2: specialized DSTU PB-257 backend
pub mod scalar;      // Phase 2 / Commit 4: 256-bit scalar mod n (no BigUint)
pub mod backend;     // Phase 3 / SIMD dispatch (PCLMULQDQ on x86_64)
pub mod cms;         // Phase 4 / Sprint 1: CAdES-BES detached builder (v1 skeleton)
pub mod hash;        // Phase 4 / Sprint 1.1: GOST 34.311 (+ later Kupyna)
pub mod curve;
pub mod point;
pub mod wnaf;
pub mod proj;
pub mod fixed_base;  // Phase 2 / Commit 6: fixed-base scalar mul for G
pub mod sign;
pub mod jks;
pub mod der;

#[cfg(feature = "python")]
mod python;

// Low-level GF(2^m) primitives (public for Rust callers and tests).
pub use gf2m::{blength, fmod, fmul, fsqr, finv, mul_1x1, mul_2x2, shift_right};

// High-level types.
pub use field::FieldEl;
pub use curve::Curve;
pub use point::Point;
pub use sign::{sign, verify, truncate, Signature};
