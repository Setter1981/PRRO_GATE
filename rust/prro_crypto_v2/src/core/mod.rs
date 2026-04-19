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
//! - Ukrainian hash functions (`hash`: GOST 34.311-95)
//! - SIMD dispatch for field multiplication (`backend`)
//! - τ-NAF scalar multiplication for Koblitz curves (`tau_naf`)
//!
//! Anything application-specific — fiscal receipt framing, JKS loading
//! for ДПС certs, PDF/XML packaging, HTTP transport — lives in sibling
//! modules if/when added.

pub mod gf2m;
pub mod gf2m_257;
pub mod field;
pub mod fe;
pub mod scalar;
pub mod curve;
pub mod point;
pub mod proj;
pub(crate) mod wnaf;
pub(crate) mod fixed_base;
pub(crate) mod mladder;
pub(crate) mod comb;
pub mod sign;
pub(crate) mod batch;
pub mod hash;
pub mod backend;
pub(crate) mod tau_naf;

// ─── τ-NAF policy types ──────────────────────────────────────────────────────

/// Controls whether τ-NAF is used on a given operation.
///
/// `Off` → use the constant-time comb/wNAF path (safe for secret scalars).
/// `On`  → use τ-NAF (variable-time, faster for public scalars only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TauNafMode {
    /// Constant-time path. Safe for secret scalars.
    Off,
    /// Variable-time τ-NAF. 2–4× faster but leaks scalar timing.
    /// Only safe for PUBLIC scalars (e.g., verify signature components).
    On,
}

/// Policy controlling τ-NAF use in sign and verify operations.
///
/// Default: signing uses CT comb (Off), verify uses τ-NAF (On).
/// This is the safe default: nonces are secret (Off), r/s are public (On).
#[derive(Clone, Copy, Debug)]
pub struct TauNafPolicy {
    /// τ-NAF mode for the ephemeral nonce multiplication in sign().
    /// MUST be Off (or understand the timing risk) for production signing.
    pub sign: TauNafMode,
    /// τ-NAF mode for verify(). Safe to set On because r, s are public.
    pub verify: TauNafMode,
}

impl Default for TauNafPolicy {
    fn default() -> Self {
        TauNafPolicy {
            sign: TauNafMode::Off,
            verify: TauNafMode::On,
        }
    }
}
