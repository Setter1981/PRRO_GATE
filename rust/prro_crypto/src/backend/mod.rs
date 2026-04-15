//! Backend dispatch for hot-path field arithmetic.
//!
//! The DSTU 4145 cryptographic core sits on top of GF(2^257) polynomial
//! multiplication. This module routes those multiplications to the best
//! implementation available on the current CPU.
//!
//! ## Backends
//! - `portable`: plain Rust schoolbook 2x2 — works everywhere
//! - `x86_pclmul`: x86_64 with PCLMULQDQ — ~10-40x faster than portable
//! - `aarch64_pmull`: ARMv8 with NEON+AES — same speedup class (TODO)
//!
//! ## Safety
//! All `unsafe` is contained inside backend submodules. Public functions
//! here perform CPU-feature detection and dispatch; callers see only safe
//! Rust signatures.

pub mod portable;

#[cfg(target_arch = "x86_64")]
pub mod x86_pclmul;

pub mod pack;

use crate::fe::{Fe, FeWide};

/// Multiply two 257-bit field elements, writing the unreduced 514-bit product
/// to `out`. Dispatches to the fastest available backend.
///
/// This is the single hot-path entrypoint that all `Fe::mod_mul` calls go
/// through. CPU feature detection is cached by `is_x86_feature_detected!`
/// so runtime overhead is negligible after the first call.
#[inline]
pub fn fmul_257(a: &Fe, b: &Fe, out: &mut FeWide) {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("pclmulqdq") {
            // SAFETY: feature detected at runtime.
            unsafe {
                x86_pclmul::fmul_257(a, b, out);
            }
            return;
        }
    }
    portable::fmul_257(a, b, out);
}
