//! x86_64 PCLMULQDQ-accelerated GF(2) polynomial multiplication.
//!
//! Uses Intel's `_mm_clmulepi64_si128` instruction (Westmere+ / Bulldozer+,
//! ~2010). One CLMUL = one 64x64 → 128-bit GF(2) polynomial multiplication
//! in roughly 5-7 cycles, vs ~50+ cycles for portable schoolbook.
//!
//! ## Layout
//! All functions in this module assume the CPU has `pclmulqdq` enabled.
//! Caller (`backend::fmul_257`) verifies via `is_x86_feature_detected!`
//! before calling. SAFETY: do NOT call these functions without that check.
//!
//! ## Architecture
//! - `clmul64`: 64x64 → 128 GF(2) mul (one CLMUL instruction)
//! - `clmul128`: 128x128 → 256 via Karatsuba (3 CLMULs)
//! - `fmul_257`: 257x257 → 514 via field decomposition + Karatsuba

#![cfg(target_arch = "x86_64")]

use core::arch::x86_64::*;

use crate::backend::pack::{pack_fe, unpack_wide, Fe64, Wide64};
use crate::fe::{Fe, FeWide};

/// Carry-less multiply of two u64 values over GF(2). Returns the 128-bit
/// product as `(low, high)` u64 pair.
///
/// SAFETY: requires CPU feature `pclmulqdq`.
#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
unsafe fn clmul64(a: u64, b: u64) -> (u64, u64) {
    let av = _mm_set_epi64x(0, a as i64);
    let bv = _mm_set_epi64x(0, b as i64);
    // IMM8 = 0x00 selects (a low, b low) → product of the two low 64-bit halves.
    let prod = _mm_clmulepi64_si128(av, bv, 0x00);
    let lo = _mm_extract_epi64(prod, 0) as u64;
    let hi = _mm_extract_epi64(prod, 1) as u64;
    (lo, hi)
}

/// Multiply two 128-bit polynomials (each as `[u64; 2]`) over GF(2).
/// Returns 256-bit product as `[u64; 4]`. Uses Karatsuba (3 CLMULs).
///
/// SAFETY: requires CPU feature `pclmulqdq`.
#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
unsafe fn clmul128(a: [u64; 2], b: [u64; 2]) -> [u64; 4] {
    // z0 = a0 * b0
    let z0 = clmul64(a[0], b[0]);
    // z2 = a1 * b1
    let z2 = clmul64(a[1], b[1]);
    // zm = (a0 ^ a1) * (b0 ^ b1)
    let zm = clmul64(a[0] ^ a[1], b[0] ^ b[1]);
    // Karatsuba middle: z1 = zm ^ z0 ^ z2
    let z1_lo = zm.0 ^ z0.0 ^ z2.0;
    let z1_hi = zm.1 ^ z0.1 ^ z2.1;

    // Combine into 4-limb result:
    //   result = z0 + (z1 << 64) + (z2 << 128)
    // Limb 0: z0.0
    // Limb 1: z0.1 ^ z1_lo
    // Limb 2: z2.0 ^ z1_hi
    // Limb 3: z2.1
    [z0.0, z0.1 ^ z1_lo, z2.0 ^ z1_hi, z2.1]
}

/// Multiply two 257-bit field elements via PCLMULQDQ.
///
/// Decomposition: each Fe64 is `(low_128, high_128, top_bit)` where
/// `low_128 = limbs[0..2]`, `high_128 = limbs[2..4]`, `top_bit = limbs[4] & 1`.
///
/// Product = (A_low + A_high·x^128 + a_top·x^256) · (B_low + B_high·x^128 + b_top·x^256)
///
/// The 128x128 cross products are done via `clmul128` (Karatsuba). The
/// top-bit contributions are sparse (single-bit) so we fold them in with
/// shifts and XORs.
///
/// SAFETY: requires CPU feature `pclmulqdq`.
#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
unsafe fn fmul_packed(a: &Fe64, b: &Fe64) -> Wide64 {
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

    // Now handle the top-bit contributions:
    //   a_top * x^256 * (b_lo + b_hi·x^128 + b_top·x^256)
    //   = a_top·b_lo·x^256 + a_top·b_hi·x^384 + a_top·b_top·x^512
    //   b_top * x^256 * (a_lo + a_hi·x^128)
    //   = b_top·a_lo·x^256 + b_top·a_hi·x^384
    // (the a_top·b_top·x^512 term goes to limb 8.)
    if a_top == 1 {
        out[4] ^= b_lo[0];
        out[5] ^= b_lo[1];
        out[6] ^= b_hi[0];
        out[7] ^= b_hi[1];
        if b_top == 1 {
            out[8] ^= 1;
        }
    }
    if b_top == 1 {
        out[4] ^= a_lo[0];
        out[5] ^= a_lo[1];
        out[6] ^= a_hi[0];
        out[7] ^= a_hi[1];
    }

    Wide64(out)
}

/// Public entrypoint: multiply two Fe values into FeWide using PCLMULQDQ.
///
/// SAFETY: requires CPU feature `pclmulqdq`.
#[target_feature(enable = "pclmulqdq,sse2")]
pub unsafe fn fmul_257(a: &Fe, b: &Fe, out: &mut FeWide) {
    let a_packed = pack_fe(a);
    let b_packed = pack_fe(b);
    let wide_packed = fmul_packed(&a_packed, &b_packed);
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
}
