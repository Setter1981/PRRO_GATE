//! x86_64 PCLMULQDQ-accelerated GF(2) polynomial multiplication.
//!
//! Uses Intel's `_mm_clmulepi64_si128` instruction (Westmere+ / Bulldozer+,
//! ~2010). One CLMUL = one 64×64 → 128-bit GF(2) polynomial multiplication
//! in roughly 5–7 cycles, versus ~50+ cycles for portable schoolbook.
//!
//! Reference: Intel SDM Vol. 2A, entry *PCLMULQDQ — Carry-Less
//! Multiplication Quadword*. The intrinsic's `IMM8` byte selects which
//! 64-bit halves of the two 128-bit operands are multiplied:
//! `0x00 → (a.lo, b.lo)`, `0x01 → (a.hi, b.lo)`, `0x10 → (a.lo, b.hi)`,
//! `0x11 → (a.hi, b.hi)`.
//!
//! ## Safety contract (shared by every `unsafe fn` in this module)
//!
//! Every function here compiles with `#[target_feature(enable =
//! "pclmulqdq,sse2")]`. Calling them on a CPU without that feature is
//! undefined behaviour (illegal instruction at runtime in practice). The
//! single entry point that enforces the contract is
//! [`crate::core::backend::fmul_257`], which probes
//! `is_x86_feature_detected!("pclmulqdq")` before dispatching. No direct
//! caller outside this module may invoke these functions without an
//! equivalent runtime check. Benchmarks and tests that reach in directly
//! must be gated on the same probe.
//!
//! ## Architecture (bottom-up)
//!
//! - [`clmul64`]    — single 64×64 → 128-bit carry-less multiplication.
//!   One `_mm_clmulepi64_si128` with `IMM8 = 0x00`.
//! - [`clmul128`]   — 128×128 → 256-bit product via 2-way Karatsuba
//!   (3 × `clmul64`, saving the 4th schoolbook multiplication).
//! - [`fmul_packed`] — 257×257 → 514-bit multiplication over the packed
//!   [`Fe64`] representation via 2-way Karatsuba over the 128-bit halves
//!   (3 × `clmul128` = 9 × `clmul64`) plus a branch-free fold for each
//!   operand's 257-th bit.
//! - [`fmul_257`]   — public entry point. Packs `Fe → Fe64`, runs
//!   `fmul_packed`, unpacks the 9-limb `Wide64` back into `FeWide`.

#![cfg(target_arch = "x86_64")]

use core::arch::x86_64::*;

use crate::core::backend::pack::{pack_fe, unpack_wide, Fe64, Wide64};
use crate::core::fe::{Fe, FeWide};

/// 64×64 → 128-bit carry-less multiplication over GF(2).
///
/// Returns the 128-bit product as `(low, high)` pair of u64 limbs —
/// `low` carries polynomial degrees 0..63, `high` carries degrees
/// 64..127. `IMM8 = 0x00` picks the low 64-bit half of each input.
///
/// # Safety
/// See module-level safety contract. Caller must ensure the running CPU
/// supports PCLMULQDQ; outside-the-module callers must gate on
/// `is_x86_feature_detected!("pclmulqdq")`.
#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
pub unsafe fn clmul64(a: u64, b: u64) -> (u64, u64) {
    let av = _mm_set_epi64x(0, a as i64);
    let bv = _mm_set_epi64x(0, b as i64);
    let prod = _mm_clmulepi64_si128(av, bv, 0x00);
    let lo = _mm_extract_epi64(prod, 0) as u64;
    let hi = _mm_extract_epi64(prod, 1) as u64;
    (lo, hi)
}

/// 128×128 → 256-bit carry-less multiplication over GF(2).
///
/// Inputs are 128-bit polynomials as `[u64; 2]` (low limb first).
/// Returns the 256-bit product as `[u64; 4]`.
///
/// Uses 2-way Karatsuba — three 64×64 multiplications instead of four:
/// split each operand as `A = A0 + A1·x^64`, compute
/// `z0 = A0·B0`, `z2 = A1·B1`, `zm = (A0+A1)·(B0+B1)`, and reconstruct
/// the middle as `z1 = zm + z0 + z2` (addition in GF(2) is XOR). The
/// recombination `z0 + z1·x^64 + z2·x^128` lands in four 64-bit limbs.
///
/// # Safety
/// See module-level safety contract.
#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
pub unsafe fn clmul128(a: [u64; 2], b: [u64; 2]) -> [u64; 4] {
    let z0 = clmul64(a[0], b[0]);
    let z2 = clmul64(a[1], b[1]);
    let zm = clmul64(a[0] ^ a[1], b[0] ^ b[1]);
    let z1_lo = zm.0 ^ z0.0 ^ z2.0;
    let z1_hi = zm.1 ^ z0.1 ^ z2.1;
    [z0.0, z0.1 ^ z1_lo, z2.0 ^ z1_hi, z2.1]
}

/// 257×257 → 514-bit multiplication over the packed [`Fe64`] layout.
///
/// Each Fe64 factors as `A = A_lo + A_hi·x^128 + a_top·x^256`, where
/// `A_lo = limbs[0..2]`, `A_hi = limbs[2..4]`, `a_top = limbs[4] & 1`.
/// The product expands as nine Karatsuba-compatible terms; we execute
/// the three `clmul128` pieces, combine them at the 128-bit offsets,
/// and fold in the single-bit `x^256` contributions from each operand
/// via a masked XOR. Total: 3 × 3 = 9 CLMULs.
///
/// # Safety
/// See module-level safety contract.
#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
pub unsafe fn fmul_packed(a: &Fe64, b: &Fe64) -> Wide64 {
    let a_lo = [a.0[0], a.0[1]];
    let a_hi = [a.0[2], a.0[3]];
    let a_top = a.0[4] & 1;

    let b_lo = [b.0[0], b.0[1]];
    let b_hi = [b.0[2], b.0[3]];
    let b_top = b.0[4] & 1;

    // Three 128-bit Karatsuba products
    let p_ll = clmul128(a_lo, b_lo); // contributes to limbs 0..3 (bits 0..255)
    let p_hh = clmul128(a_hi, b_hi); // contributes to limbs 4..7 (bits 256..511)
                                     // Karatsuba middle on the 128-bit halves:
    let a_mid = [a_lo[0] ^ a_hi[0], a_lo[1] ^ a_hi[1]];
    let b_mid = [b_lo[0] ^ b_hi[0], b_lo[1] ^ b_hi[1]];
    let p_mid = clmul128(a_mid, b_mid);

    // p_cross = p_mid - p_ll - p_hh (in GF(2), subtraction is XOR)
    // p_cross goes to limbs 2..5 (bits 128..383)
    let mut p_cross = [0u64; 4];
    for i in 0..4 {
        p_cross[i] = p_mid[i] ^ p_ll[i] ^ p_hh[i];
    }

    // Now assemble the 8-limb result from p_ll, p_cross, p_hh:
    // out[0..4] += p_ll[0..4]
    // out[2..6] += p_cross[0..4]
    // out[4..8] += p_hh[0..4]
    let mut out = [0u64; 9];
    for i in 0..4 {
        out[i] ^= p_ll[i];
        out[i + 2] ^= p_cross[i];
        out[i + 4] ^= p_hh[i];
    }

    // Top-bit contributions from `a·x^256` and `b·x^256`. The CT rewrite
    // (Sprint 2.1c4, in response to a dudect signal on the `e` axis)
    // replaces the `if a_top == 1` and `if b_top == 1` branches with
    // masked XORs: every sample of `fmul_257` now touches the same memory
    // regardless of whether bit 256 of either operand is set.
    let a_top_mask: u64 = 0u64.wrapping_sub(a_top);
    let b_top_mask: u64 = 0u64.wrapping_sub(b_top);

    //   a_top * x^256 * (b_lo + b_hi·x^128) → contributes to limbs 4..7
    out[4] ^= a_top_mask & b_lo[0];
    out[5] ^= a_top_mask & b_lo[1];
    out[6] ^= a_top_mask & b_hi[0];
    out[7] ^= a_top_mask & b_hi[1];

    //   b_top * x^256 * (a_lo + a_hi·x^128) → also limbs 4..7
    out[4] ^= b_top_mask & a_lo[0];
    out[5] ^= b_top_mask & a_lo[1];
    out[6] ^= b_top_mask & a_hi[0];
    out[7] ^= b_top_mask & a_hi[1];

    //   a_top·b_top·x^512 → limb 8. Both tops are 0 or 1, so a raw AND
    //   gives the correct scalar contribution without any branch.
    out[8] ^= a_top & b_top;

    Wide64(out)
}

/// Polynomial squaring via PCLMULQDQ.
///
/// `clmul(x, x)` spreads bits: bit `i` → bit `2*i`, which is exactly
/// polynomial squaring in GF(2). For our 257-bit element (4 × u64 + 1 bit):
/// 4 PCLMULQDQ calls, no Karatsuba cross-terms needed (a² has no mixed
/// products in characteristic 2).
///
/// Cost: 4 CLMULs vs. 9 CLMULs for full multiplication — a 2.25× speedup
/// on the raw polynomial expansion, before reduction.
///
/// # Safety
/// See module-level safety contract.
#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
pub unsafe fn fsqr_packed(a: &Fe64) -> Wide64 {
    let mut out = [0u64; 9];

    // Each 64-bit limb squares independently — no cross products in GF(2).
    // clmul64(x, x) produces a 128-bit result where bit i of x maps to
    // bit 2*i of the output.
    let (lo0, hi0) = clmul64(a.0[0], a.0[0]);
    let (lo1, hi1) = clmul64(a.0[1], a.0[1]);
    let (lo2, hi2) = clmul64(a.0[2], a.0[2]);
    let (lo3, hi3) = clmul64(a.0[3], a.0[3]);

    out[0] = lo0;
    out[1] = hi0;
    out[2] = lo1;
    out[3] = hi1;
    out[4] = lo2;
    out[5] = hi2;
    out[6] = lo3;
    out[7] = hi3;

    // Top bit: (0 or 1)² = (0 or 1) in GF(2).
    out[8] = a.0[4] & 1;

    Wide64(out)
}

/// Public entry point — square an [`Fe`] value into [`FeWide`] via PCLMULQDQ.
///
/// Packs the input into [`Fe64`], runs [`fsqr_packed`], and unpacks the
/// result back into `FeWide` for `freduce_257`.
///
/// # Safety
/// See module-level safety contract.
#[target_feature(enable = "pclmulqdq,sse2")]
pub unsafe fn fsqr_257(a: &Fe, out: &mut FeWide) {
    let a_packed = pack_fe(a);
    let wide_packed = fsqr_packed(&a_packed);
    *out = unpack_wide(&wide_packed);
}

/// Public entry point — multiply two [`Fe`] values into [`FeWide`] via
/// PCLMULQDQ.
///
/// Pack both inputs into the [`Fe64`] representation (5 u64 limbs each),
/// run [`fmul_packed`], and unpack the 9-limb [`Wide64`] back into the
/// u32-limbed [`FeWide`] layout that [`crate::core::gf2m_257::freduce_257`]
/// expects.
///
/// # Safety
/// See module-level safety contract. The [`crate::core::backend::fmul_257`]
/// dispatcher is the only call site in this crate that does the feature
/// probe; any other caller (benchmarks, tests) must probe first.
#[target_feature(enable = "pclmulqdq,sse2")]
pub unsafe fn fmul_257(a: &Fe, b: &Fe, out: &mut FeWide) {
    let a_packed = pack_fe(a);
    let b_packed = pack_fe(b);
    let wide_packed = fmul_packed(&a_packed, &b_packed);
    *out = unpack_wide(&wide_packed);
}

// ── VPCLMULQDQ (AVX2) tier — 256-bit wide carry-less multiply ──────────
//
// On CPUs with VPCLMULQDQ+AVX2 (Zen 3+, Ice Lake+), each instruction
// performs TWO independent 64×64→128 carry-less multiplies (one per
// 128-bit lane). The 3-level Karatsuba still needs 9 products, but we
// batch them into 5 VPCLMULQDQ instructions (4 batches of 2 + 1 leftover
// via regular PCLMULQDQ).
//
// Expected gain: ~1.5× on Intel (1c throughput), ~neutral on Zen 3 (2c).
// The dispatch in `backend::fmul_257` probes at runtime.

/// 2×(64×64→128) via a single `vpclmulqdq ymm`. Returns ([lo0,hi0], [lo1,hi1]).
///
/// # Safety
/// Caller must ensure VPCLMULQDQ+AVX2 are available.
#[target_feature(enable = "vpclmulqdq,avx2,pclmulqdq,sse2")]
#[inline]
unsafe fn vclmul2(a0: u64, b0: u64, a1: u64, b1: u64) -> ((u64, u64), (u64, u64)) {
    let av = _mm256_set_epi64x(0, a1 as i64, 0, a0 as i64);
    let bv = _mm256_set_epi64x(0, b1 as i64, 0, b0 as i64);
    let prod = _mm256_clmulepi64_epi128(av, bv, 0x00);
    let lo0 = _mm256_extract_epi64(prod, 0) as u64;
    let hi0 = _mm256_extract_epi64(prod, 1) as u64;
    let lo1 = _mm256_extract_epi64(prod, 2) as u64;
    let hi1 = _mm256_extract_epi64(prod, 3) as u64;
    ((lo0, hi0), (lo1, hi1))
}

/// 128×128 Karatsuba using VPCLMULQDQ — 2 vclmul2 calls (4 products)
/// instead of 3 clmul64 calls.
#[target_feature(enable = "vpclmulqdq,avx2,pclmulqdq,sse2")]
#[inline]
unsafe fn vclmul128(a: [u64; 2], b: [u64; 2]) -> [u64; 4] {
    // Batch: z0 = a[0]*b[0] and z2 = a[1]*b[1] in one instruction
    let (z0, z2) = vclmul2(a[0], b[0], a[1], b[1]);
    // Third product: zm = (a[0]^a[1]) * (b[0]^b[1]) via regular clmul
    let zm = clmul64(a[0] ^ a[1], b[0] ^ b[1]);
    let z1_lo = zm.0 ^ z0.0 ^ z2.0;
    let z1_hi = zm.1 ^ z0.1 ^ z2.1;
    [z0.0, z0.1 ^ z1_lo, z2.0 ^ z1_hi, z2.1]
}

/// VPCLMULQDQ-accelerated 257×257 → 514 multiplication.
///
/// Same Karatsuba structure as `fmul_packed` but uses `vclmul128` for
/// the three 128×128 sub-products. Total: 3×2=6 VPCLMULQDQ + 3 PCLMULQDQ
/// = 9 carry-less multiplies in 6 instructions (vs 9 in the PCLMULQDQ path).
///
/// # Safety
///
/// The caller MUST have verified at runtime that the CPU supports
/// `vpclmulqdq`, `avx2`, `pclmulqdq` and `sse2` (e.g. via
/// `is_x86_feature_detected!`) before calling — the `#[target_feature]`
/// attribute makes calling this on a CPU without those features
/// undefined behavior (illegal instruction). The dispatcher in
/// `backend::fmul_257` is the only sanctioned call path.
#[target_feature(enable = "vpclmulqdq,avx2,pclmulqdq,sse2")]
#[inline]
pub unsafe fn fmul_packed_vpclmul(a: &Fe64, b: &Fe64) -> Wide64 {
    let a_lo = [a.0[0], a.0[1]];
    let a_hi = [a.0[2], a.0[3]];
    let a_top = a.0[4] & 1;
    let b_lo = [b.0[0], b.0[1]];
    let b_hi = [b.0[2], b.0[3]];
    let b_top = b.0[4] & 1;

    let p_ll = vclmul128(a_lo, b_lo);
    let p_hh = vclmul128(a_hi, b_hi);
    let a_mid = [a_lo[0] ^ a_hi[0], a_lo[1] ^ a_hi[1]];
    let b_mid = [b_lo[0] ^ b_hi[0], b_lo[1] ^ b_hi[1]];
    let p_mid = vclmul128(a_mid, b_mid);

    let mut p_cross = [0u64; 4];
    for i in 0..4 {
        p_cross[i] = p_mid[i] ^ p_ll[i] ^ p_hh[i];
    }

    let mut out = [0u64; 9];
    for i in 0..4 {
        out[i] ^= p_ll[i];
        out[i + 2] ^= p_cross[i];
        out[i + 4] ^= p_hh[i];
    }

    // Top-bit contributions (CT masked XOR, same as fmul_packed)
    let a_top_mask: u64 = 0u64.wrapping_sub(a_top);
    let b_top_mask: u64 = 0u64.wrapping_sub(b_top);
    out[4] ^= a_top_mask & b_lo[0];
    out[5] ^= a_top_mask & b_lo[1];
    out[6] ^= a_top_mask & b_hi[0];
    out[7] ^= a_top_mask & b_hi[1];
    out[4] ^= b_top_mask & a_lo[0];
    out[5] ^= b_top_mask & a_lo[1];
    out[6] ^= b_top_mask & a_hi[0];
    out[7] ^= b_top_mask & a_hi[1];
    out[8] ^= a_top & b_top;

    Wide64(out)
}

/// Public VPCLMULQDQ entry point: Fe × Fe → FeWide.
///
/// # Safety
///
/// Same contract as [`fmul_packed_vpclmul`]: the caller MUST have
/// runtime-verified `vpclmulqdq`/`avx2`/`pclmulqdq`/`sse2` support
/// (`is_x86_feature_detected!`) — calling without them is UB. Use the
/// `backend::fmul_257` dispatcher instead of calling this directly.
#[target_feature(enable = "vpclmulqdq,avx2,pclmulqdq,sse2")]
pub unsafe fn fmul_257_vpclmul(a: &Fe, b: &Fe, out: &mut FeWide) {
    let a_packed = pack_fe(a);
    let b_packed = pack_fe(b);
    let wide_packed = fmul_packed_vpclmul(&a_packed, &b_packed);
    *out = unpack_wide(&wide_packed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_supports_pclmul() -> bool {
        std::is_x86_feature_detected!("pclmulqdq")
    }

    #[test]
    fn test_clmul64_zero() {
        if !cpu_supports_pclmul() {
            eprintln!("SKIP: no pclmulqdq");
            return;
        }
        unsafe {
            assert_eq!(clmul64(0, 0xDEAD_BEEF), (0, 0));
            assert_eq!(clmul64(0xDEAD_BEEF, 0), (0, 0));
        }
    }

    #[test]
    fn test_clmul64_one() {
        if !cpu_supports_pclmul() {
            return;
        }
        unsafe {
            // 1 * x = x (in GF(2) polynomial)
            assert_eq!(clmul64(1, 1), (1, 0));
            // x * 1 = x
            assert_eq!(clmul64(0xDEAD_BEEF, 1), (0xDEAD_BEEF, 0));
            // 0xFF * 0xFF in GF(2)
            // Let's compute by hand: it's the same as squaring the polynomial 0xFF.
            // Actually, let's just compare to a portable reference.
        }
    }

    /// Portable carry-less multiply for cross-checking.
    fn clmul64_portable(a: u64, b: u64) -> (u64, u64) {
        let mut lo: u64 = 0;
        let mut hi: u64 = 0;
        for i in 0..64 {
            if (b >> i) & 1 == 1 {
                lo ^= a << i;
                if i > 0 {
                    hi ^= a >> (64 - i);
                }
            }
        }
        (lo, hi)
    }

    #[test]
    fn test_clmul64_vs_portable_random() {
        if !cpu_supports_pclmul() {
            return;
        }
        let mut x: u64 = 0xCAFE_BABE_DEAD_BEEF;
        let mut y: u64 = 0x1234_5678_9ABC_DEF0;
        for _ in 0..1000 {
            // xorshift64
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            y ^= y << 21;
            y ^= y >> 35;
            y ^= y << 4;

            let want = clmul64_portable(x, y);
            let got = unsafe { clmul64(x, y) };
            assert_eq!(got, want, "mismatch on x={:#x} y={:#x}", x, y);
        }
    }

    #[test]
    fn test_clmul64_high_bit() {
        if !cpu_supports_pclmul() {
            return;
        }
        unsafe {
            // x^63 * x^63 = x^126
            let r = clmul64(1u64 << 63, 1u64 << 63);
            // bit 126 in 128-bit result: high half bit 62
            assert_eq!(r, (0, 1u64 << 62));
        }
    }

    #[test]
    fn test_clmul128_vs_portable_random() {
        if !cpu_supports_pclmul() {
            return;
        }
        let mut state = [0xDEAD_BEEFu64, 0xCAFE_BABE, 0x1234_5678, 0x9ABC_DEF0];
        for _ in 0..200 {
            for s in &mut state {
                *s ^= *s << 13;
                *s ^= *s >> 7;
                *s ^= *s << 17;
            }
            let a = [state[0], state[1]];
            let b = [state[2], state[3]];

            let got = unsafe { clmul128(a, b) };
            let want = clmul128_portable(a, b);
            assert_eq!(got, want, "clmul128 mismatch on a={:?} b={:?}", a, b);
        }
    }

    /// Portable 128x128 polynomial mul for cross-checking.
    fn clmul128_portable(a: [u64; 2], b: [u64; 2]) -> [u64; 4] {
        let z0 = clmul64_portable(a[0], b[0]);
        let z2 = clmul64_portable(a[1], b[1]);
        let zm = clmul64_portable(a[0] ^ a[1], b[0] ^ b[1]);
        let z1_lo = zm.0 ^ z0.0 ^ z2.0;
        let z1_hi = zm.1 ^ z0.1 ^ z2.1;
        [z0.0, z0.1 ^ z1_lo, z2.0 ^ z1_hi, z2.1]
    }

    /// Differential test: fsqr_packed (PCLMULQDQ) vs portable spread_bits.
    #[test]
    fn test_fsqr_packed_vs_portable() {
        if !cpu_supports_pclmul() {
            eprintln!("SKIP: no pclmulqdq");
            return;
        }
        use crate::core::fe::FE_WORDS;

        fn make_fe(seed: u32) -> Fe {
            let mut x = seed;
            let mut w = [0u32; FE_WORDS];
            for i in 0..FE_WORDS {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                w[i] = x;
            }
            w[8] &= 0x1;
            Fe(w)
        }

        for seed in 0..500u32 {
            let a = make_fe(seed);
            let mut pclmul_out = FeWide::ZERO;
            let mut portable_out = FeWide::ZERO;

            unsafe {
                fsqr_257(&a, &mut pclmul_out);
            }
            crate::core::backend::portable::fsqr_257(&a, &mut portable_out);

            assert_eq!(
                pclmul_out.0, portable_out.0,
                "fsqr_257 PCLMULQDQ vs portable mismatch at seed {}",
                seed,
            );
        }
    }

    /// Squaring identity: a² = a * a in GF(2^m).
    #[test]
    fn test_fsqr_equals_fmul_self() {
        if !cpu_supports_pclmul() {
            return;
        }
        use crate::core::fe::FE_WORDS;

        fn make_fe(seed: u32) -> Fe {
            let mut x = seed;
            let mut w = [0u32; FE_WORDS];
            for i in 0..FE_WORDS {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                w[i] = x;
            }
            w[8] &= 0x1;
            Fe(w)
        }

        for seed in 0..200u32 {
            let a = make_fe(seed);
            let mut sqr_out = FeWide::ZERO;
            let mut mul_out = FeWide::ZERO;

            unsafe {
                fsqr_257(&a, &mut sqr_out);
                fmul_257(&a, &a, &mut mul_out);
            }

            assert_eq!(
                sqr_out.0, mul_out.0,
                "fsqr_257 != fmul_257(a,a) at seed {}",
                seed,
            );
        }
    }

    /// Edge cases: zero, one, max (all bits set).
    #[test]
    fn test_fsqr_edge_cases() {
        if !cpu_supports_pclmul() {
            return;
        }
        // Zero squares to zero
        let mut out = FeWide::ZERO;
        unsafe {
            fsqr_257(&Fe::ZERO, &mut out);
        }
        assert_eq!(out, FeWide::ZERO);

        // One squares to one (unreduced: word 0 = 1, rest zero)
        unsafe {
            fsqr_257(&Fe::ONE, &mut out);
        }
        assert_eq!(out.0[0], 1);
        for i in 1..out.0.len() {
            assert_eq!(out.0[i], 0, "ONE² nonzero at limb {}", i);
        }

        // Element with only bit 256 set: (x^256)² = x^512
        let mut top_only = Fe::ZERO;
        top_only.0[8] = 1;
        unsafe {
            fsqr_257(&top_only, &mut out);
        }
        // x^512 lives at bit 512 = wide word 16, bit 0
        // In our FeWide(u32×20), word 16 = out.0[16]
        assert_eq!(out.0[16], 1);
        // Everything else zero
        for i in 0..out.0.len() {
            if i != 16 {
                assert_eq!(out.0[i], 0, "top_only² nonzero at limb {}", i);
            }
        }
    }
}
