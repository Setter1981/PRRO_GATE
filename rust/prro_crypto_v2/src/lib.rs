//! `prro_crypto_v2` — DSTU 4145 cryptography core with τ-NAF policy dispatch.
//!
//! This crate is a clean-room evolution of `prro_crypto`. It targets
//! DSTU_PB_257 only (the curve used by the Ukrainian fiscal authority) and
//! adds τ-NAF scalar multiplication for Koblitz curve acceleration, gated
//! behind an explicit [`TauNafPolicy`].
//!
//! ## Crate structure
//!
//! - [`core`] — field / curve / scalar arithmetic, DSTU 4145-LE sign + verify
//!   with full public-point validation, GOST 34.311-95 hash, τ-NAF module,
//!   CPU-feature-dispatched backends. No fiscal / PRRO / ДПС specifics.
//!
//! ## τ-NAF policy
//!
//! By default ([`TauNafPolicy::default()`]):
//! - Signing uses the constant-time comb (safe for the secret nonce).
//! - Verification uses τ-NAF (variable-time, 2–4× faster; safe because
//!   signature components r, s are public).
//!
//! Use [`sign_with_policy`] / [`verify_with_policy`] to override.
//!
//! ## Security note
//!
//! Enabling `TauNafMode::On` for signing creates a timing side-channel on
//! the ephemeral nonce. See `core::sign` docs for the full threat model.

pub mod core;

// ─── Top-level re-exports ────────────────────────────────────────────────────

pub use core::curve::Curve;
pub use core::field::FieldEl;
pub use core::point::Point;
pub use core::sign::{
    set_verify_cache_capacity, sign, truncate, verify,
    sign_with_policy, verify_with_policy,
    Signature,
};
pub use core::{TauNafMode, TauNafPolicy};
pub use crate::core::batch::{batch_verify, batch_verify_fast, BatchItem, BatchResult};

/// Prime the backend dispatch so the first `sign()` call in a process does
/// not pay the one-time `CPUID` probing cost.
pub use core::backend::warm_up;
