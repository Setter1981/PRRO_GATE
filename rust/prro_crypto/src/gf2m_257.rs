//! Specialized GF(2^257) backend with `x^257 + x^12 + 1` reduction polynomial.
//!
//! All loop bounds, polynomial coefficients, and word counts are compile-time
//! constants. No runtime polynomial walk in `freduce`. No length parameters.
//!
//! Used by `Fe` (fixed-size field type) for hot-path operations.
//! The generic `gf2m` module remains for tests and for any future curve.

use crate::fe::{Fe, FeWide, FE_WIDE, FE_WORDS};
use crate::gf2m::{mul_2x2, spread_bits_u32_pub};

/// Reduce `wide` modulo `x^257 + x^12 + 1`, write 9-word result to `out`.
///
/// The reduction uses the identity:
///     x^257 = x^12 + 1   (mod p)
///
/// So a bit at position k (k > 256) folds to bits at (k-257) and (k-245).
/// Equivalently, in word form: shift the high words down 257 bits and XOR.
///
/// Compared to generic `fmod`, this:
///   - Has no exponent-list loop (just two folds per word: t^0 and t^12)
///   - Has fixed loop bounds known at compile time
///   - Allows the compiler to unroll
pub fn freduce_257(wide: &[u32; FE_WIDE], out: &mut [u32; FE_WORDS]) {
    // Local working copy
    let mut r = [0u32; FE_WIDE];
    r.copy_from_slice(wide);

    // Constants for x^257 + x^12 + 1:
    //   p[0] = 257, p[1] = 12, p[2] = 0
    //   dN = 257 / 32 = 8 (word containing bit 257)
    //
    // For component t^p[k] = t^12, k=1:
    //   n_exp = 257 - 12 = 245
    //   d0 = 245 % 32 = 21, d1 = 11
    //   n = 245 / 32 = 7
    //
    // For component t^0 (constant term, the implicit +1):
    //   n_exp = 257
    //   d0 = 257 % 32 = 1, d1 = 31
    //   n = 257 / 32 = 8
    const DN: usize = 8;
    const N_T12: usize = 7;
    const D0_T12: u32 = 21;
    const D1_T12: u32 = 11;
    const N_T0: usize = 8;
    const D0_T0: u32 = 1;
    const D1_T0: u32 = 31;

    // Main reduction: walk down from word index `FE_WIDE - 1` to word `DN`,
    // folding each non-zero high word back into lower positions.
    let mut j: usize = FE_WIDE - 1;
    while j > DN {
        let zz = r[j];
        if zz != 0 {
            r[j] = 0;

            // Fold via t^12 component
            r[j - N_T12] ^= zz >> D0_T12;
            r[j - N_T12 - 1] ^= zz << D1_T12;

            // Fold via t^0 (the +1)
            r[j - N_T0] ^= zz >> D0_T0;
            r[j - N_T0 - 1] ^= zz << D1_T0;
            // Stay on j: zz is now zero (we cleared r[j]), and the folds
            // all wrote to positions < j. The next iteration of the outer
            // loop hits `if zz != 0` else-branch and decrements.
        }
        // Decrement (we always processed j, either by folding or by skipping)
        j -= 1;
    }

    // Final round: reduce the high bits of word DN that are above bit (257 % 32) = 1.
    loop {
        let zz = r[DN] >> D0_T0; // bits 1+ of word DN
        if zz == 0 {
            break;
        }

        // Clear top bits of r[DN], keeping only bit 0 (the part below x^257)
        r[DN] = (r[DN] << D1_T0) >> D1_T0;

        // Fold zz back via t^0
        r[0] ^= zz;

        // Fold zz back via t^12: position 12 = word 0 bit 12
        const N_T12_FINAL: usize = 0; // 12 / 32 = 0
        const D0_T12_FINAL: u32 = 12; // 12 % 32 = 12
        const D1_T12_FINAL: u32 = 20;
        r[N_T12_FINAL] ^= zz << D0_T12_FINAL;
        if zz >> D1_T12_FINAL != 0 {
            r[N_T12_FINAL + 1] ^= zz >> D1_T12_FINAL;
        }
    }

    out.copy_from_slice(&r[..FE_WORDS]);
}

/// Polynomial multiplication of two Fe values, writing wide result.
///
/// Phase 3: dispatches to backend (PCLMULQDQ on x86_64 if available,
/// portable schoolbook otherwise). All hot-path mod_mul calls flow through
/// this function — the Fe API surface stays unchanged.
#[inline]
pub fn fmul_257(a: &Fe, b: &Fe, out: &mut FeWide) {
    crate::backend::fmul_257(a, b, out)
}

/// Squaring via bit-spreading. Specialized for FE_WORDS input.
pub fn fsqr_257(a: &Fe, out: &mut FeWide) {
    for v in out.0.iter_mut() {
        *v = 0;
    }
    for i in 0..FE_WORDS {
        let spread = spread_bits_u32_pub(a.0[i]);
        out.0[2 * i] = spread as u32;
        out.0[2 * i + 1] = (spread >> 32) as u32;
    }
}

/// Combined multiply + reduce: a * b mod p, returning Fe.
#[inline]
pub fn mul_257(a: &Fe, b: &Fe) -> Fe {
    let mut wide = FeWide::ZERO;
    fmul_257(a, b, &mut wide);
    let mut out = [0u32; FE_WORDS];
    freduce_257(&wide.0, &mut out);
    Fe(out)
}

/// Combined squaring + reduce.
#[inline]
pub fn sqr_257(a: &Fe) -> Fe {
    let mut wide = FeWide::ZERO;
    fsqr_257(a, &mut wide);
    let mut out = [0u32; FE_WORDS];
    freduce_257(&wide.0, &mut out);
    Fe(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fe::{Fe, P_EXP};
    use crate::gf2m::fmod;

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

    #[test]
    fn test_freduce_257_matches_generic_fmod() {
        // Build a 18-word "wide" with random data, padded to FE_WIDE (20)
        let mut wide = [0u32; FE_WIDE];
        let mut x: u32 = 0xDEAD_BEEF;
        for i in 0..FE_WIDE {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            wide[i] = x;
        }

        let mut out_specialized = [0u32; FE_WORDS];
        freduce_257(&wide, &mut out_specialized);

        // Compare with generic fmod
        let mut out_generic = [0u32; FE_WIDE];
        fmod(&wide, &P_EXP, &mut out_generic);

        for i in 0..FE_WORDS {
            assert_eq!(
                out_specialized[i], out_generic[i],
                "freduce_257 differs from fmod at word {}: specialized={:08x}, generic={:08x}",
                i, out_specialized[i], out_generic[i]
            );
        }
    }

    #[test]
    fn test_mul_257_matches_fe_mod_mul() {
        let a = make_fe(0xCAFE);
        let b = make_fe(0xBABE);

        let result_specialized = mul_257(&a, &b);
        let result_generic = a.mod_mul(&b);

        assert_eq!(
            result_specialized.0, result_generic.0,
            "mul_257 differs from Fe::mod_mul"
        );
    }

    #[test]
    fn test_sqr_257_matches_fe_mod_sqr() {
        let a = make_fe(0xDEAD);
        let result_specialized = sqr_257(&a);
        let result_generic = a.mod_sqr();
        assert_eq!(result_specialized.0, result_generic.0);
    }

    #[test]
    fn test_known_value_x257_reduction() {
        // x^257 mod p = x^12 + 1
        let mut wide = [0u32; FE_WIDE];
        wide[8] = 1 << 1; // bit 257 = word 8 bit 1

        let mut out = [0u32; FE_WORDS];
        freduce_257(&wide, &mut out);

        // Expected: x^12 + 1 = 0x1001 in word 0
        assert_eq!(out[0], 0x1001);
        for i in 1..FE_WORDS {
            assert_eq!(out[i], 0);
        }
    }
}
