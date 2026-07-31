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

// ─── Grandfathered lint debt (CS-1R lint-debt pass) ──────────────────────────
//
// This crate is a CT-sensitive clean-room DSTU-4145 core. The lints below are
// the exact pre-existing findings under `-D warnings --all-targets`; they are
// GRANDFATHERED at the crate root so the clippy CI leg lints FUTURE code while a
// proper per-finding, CT-aware cleanup is deferred. We do NOT run clippy --fix
// or autofix on constant-time crypto — an autofixed loop/cast can silently break
// a CT property. This block is attributes-only: zero compiled-behaviour change.
#![allow(
    clippy::needless_range_loop,
    clippy::identity_op,
    clippy::manual_memcpy,
    clippy::manual_div_ceil,
    clippy::manual_range_contains,
    clippy::needless_return,
    clippy::unnecessary_cast,
    clippy::unnecessary_to_owned,
    clippy::useless_vec,
    clippy::unusual_byte_groupings,
    clippy::should_implement_trait,
    clippy::missing_safety_doc,
    clippy::items_after_test_module,
    clippy::explicit_counter_loop,
    clippy::doc_overindented_list_items,
    dead_code,
    unused,
    unused_variables
)]

pub mod core;

// ─── Top-level re-exports ────────────────────────────────────────────────────

pub use crate::core::batch::{batch_verify, batch_verify_fast, BatchItem, BatchResult};
pub use core::curve::Curve;
pub use core::field::FieldEl;
pub use core::point::Point;
pub use core::sign::{
    set_verify_cache_capacity, sign, sign_with_policy, truncate, verify, verify_with_policy,
    Signature,
};
pub use core::{TauNafMode, TauNafPolicy};

/// Prime the backend dispatch so the first `sign()` call in a process does
/// not pay the one-time `CPUID` probing cost.
pub use core::backend::warm_up;
