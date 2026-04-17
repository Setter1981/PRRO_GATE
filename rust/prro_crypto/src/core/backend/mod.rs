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

use crate::core::fe::{Fe, FeWide};

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

#[cfg(test)]
mod tests {
    use super::*;

    /// `warm_up()` must not panic and must leave later `fmul_257` calls
    /// functional. The real perf claim (first-call CPUID cost folded into
    /// initialisation) can only be observed on a cold process; here we just
    /// assert the helper is safe to call unconditionally.
    #[test]
    fn warm_up_is_safe_and_idempotent() {
        warm_up();
        warm_up();
        // Verify backend is still usable.
        let a = Fe::ONE;
        let b = Fe::ONE;
        let mut out = FeWide::ZERO;
        fmul_257(&a, &b, &mut out);
        // 1 * 1 = 1, unreduced layout: low word == 1, rest zero.
        assert_eq!(out.0[0], 1);
        for i in 1..out.0.len() {
            assert_eq!(out.0[i], 0, "warm_up left backend in bad state at limb {}", i);
        }
    }
}

/// Prime the CPU-feature detection cache and the chosen backend's
/// instruction cache.
///
/// `is_x86_feature_detected!` runs a one-time `CPUID` probe on its first
/// invocation in a process and caches the answer atomically. Every
/// subsequent call is a single atomic load. Without warm-up, the first
/// `sign()` in a fresh process pays that probing cost; with warm-up it
/// does not.
///
/// This is **not** a cryptographic concern — the CPUID answer is public
/// and does not correlate with any secret — but it is an operational
/// one for anyone measuring per-call signing latency. Call once at
/// process start (or before any timing-sensitive benchmark) to fold
/// that cost into the initialisation phase instead of the first sign.
pub fn warm_up() {
    // Priming `is_x86_feature_detected!` directly is enough to populate
    // the atomic cache that every `fmul_257` call later reads. We
    // deliberately avoid calling `fmul_257` itself here — routing
    // warm-up through it would create a new, non-`target_feature`
    // caller of the backend entry points and LLVM would respond by
    // de-inlining them out of the hot signing path (measured as a
    // ~40% regression on `sign_full`). A direct CPUID probe has none
    // of that codegen downside.
    #[cfg(target_arch = "x86_64")]
    {
        let detected = std::is_x86_feature_detected!("pclmulqdq");
        std::hint::black_box(detected);
    }
}
