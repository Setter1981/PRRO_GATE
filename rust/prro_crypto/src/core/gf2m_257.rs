//! Specialized GF(2^257) backend with `x^257 + x^12 + 1` reduction polynomial.
//!
//! All loop bounds, polynomial coefficients, and word counts are compile-time
//! constants. No runtime polynomial walk in `freduce`. No length parameters.
//!
//! Used by `Fe` (fixed-size field type) for hot-path operations.
//! The generic `gf2m` module remains for tests and for any future curve.

use crate::core::fe::{Fe, FeWide, FE_WIDE, FE_WORDS};
use crate::core::gf2m::spread_bits_u32_pub;

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

    // ── Constant-time rewrite (Sprint 2.1c4) ───────────────────────────────
    //
    // The previous implementation had three data-dependent branches that
    // dudect caught as a 31σ leak on the ephemeral-scalar axis:
    //
    //   1. `if zz != 0 { fold }` in the main loop — skipped the XORs when
    //      the high word was already zero.
    //   2. `loop { if zz == 0 break }` on the DN tail — iteration count
    //      tracked the high-bit pattern of the product's top word.
    //   3. `if zz >> D1_T12_FINAL != 0` guarded an overflow XOR.
    //
    // XORing with zero is a no-op; the folds can run unconditionally at
    // the cost of a handful of extra ALU ops per reduction. In exchange
    // every input takes the same path and the same number of cycles.

    // Main reduction: walk from word `FE_WIDE - 1` down to word `DN + 1`,
    // folding each high word back into lower positions — unconditionally.
    let mut j: usize = FE_WIDE - 1;
    while j > DN {
        let zz = r[j];
        r[j] = 0;

        r[j - N_T12] ^= zz >> D0_T12;
        r[j - N_T12 - 1] ^= zz << D1_T12;
        r[j - N_T0] ^= zz >> D0_T0;
        r[j - N_T0 - 1] ^= zz << D1_T0;

        j -= 1;
    }

    // Final fold on word `DN`. After the main loop, r[DN] carries at most
    // the `D1_T0` high bits that still exceed the irreducible's degree;
    // the fold below writes only to r[0] and r[1], never to r[DN] itself,
    // so a single unconditional pass is enough to canonicalise — no
    // secret-dependent loop needed.
    const N_T12_FINAL: usize = 0;
    const D0_T12_FINAL: u32 = 12;
    const D1_T12_FINAL: u32 = 20;

    let zz = r[DN] >> D0_T0; // bits 1+ of word DN
    r[DN] = (r[DN] << D1_T0) >> D1_T0; // keep only bit 0
    r[0] ^= zz; // fold via t^0
    r[N_T12_FINAL] ^= zz << D0_T12_FINAL; // fold via t^12, low half
    r[N_T12_FINAL + 1] ^= zz >> D1_T12_FINAL; // fold via t^12, high half (XOR-0 when zz small)

    out.copy_from_slice(&r[..FE_WORDS]);
}

/// Polynomial multiplication of two Fe values, writing wide result.
///
/// Phase 3: dispatches to backend (PCLMULQDQ on x86_64 if available,
/// portable schoolbook otherwise). All hot-path mod_mul calls flow through
/// this function — the Fe API surface stays unchanged.
#[inline]
pub fn fmul_257(a: &Fe, b: &Fe, out: &mut FeWide) {
    crate::core::backend::fmul_257(a, b, out)
}

/// Squaring via bit-spreading table. Specialized for FE_WORDS input.
///
/// Each 32-bit word is spread so that bit `i` moves to bit `2*i` of a 64-bit
/// result — this IS polynomial squaring in GF(2). The table approach
/// (9 lookups + 18 stores) is faster than PCLMULQDQ for this operation
/// because it avoids SSE register transitions and dispatch overhead.
/// PCLMULQDQ squaring is available in `backend::fsqr_257` for cross-testing.
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
    use crate::core::fe::{Fe, P_EXP};
    use crate::core::gf2m::fmod;

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
