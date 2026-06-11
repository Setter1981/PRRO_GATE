//! Differential test suite: portable backend vs PCLMULQDQ backend.
//!
//! Multiplies the same `Fe` pair through both backends and compares the
//! full 514-bit product. Any divergence is a critical bug — in production
//! it would silently corrupt one in N signatures. This is the gate that
//! must stay green before the SIMD backend dispatch is allowed to ship.
//!
//! Sprint 2.3 (x86 polish, Expert-2 hand-over): the random-pair count
//! is bumped to 50 000 and an edge-case grid (zero, one, all-ones,
//! top-bit-only, Fe-squared) is run separately, because the bug class
//! that dudect previously surfaced in `fmul_packed`'s top-bit fold
//! lived at exactly the inputs a uniform random sampler almost never
//! exercises (both operands having the 256-th bit set simultaneously
//! happens in ~1/4 of pairs only after many thousands of draws).

// x86-only by definition: the differential needs BOTH backends, and
// `backend::x86_pclmul` exists only on x86_64. On other arches (aarch64
// CI arm) this file compiles to an empty test binary. (CRY-3 follow-up:
// the first per-target CI run of this suite caught the unconditional
// x86_pclmul references here.)
#![cfg(target_arch = "x86_64")]
// Explicit index loops mirror the core's CT style (see core/mod.rs).
#![allow(clippy::needless_range_loop)]

use prro_crypto::core::backend;
use prro_crypto::core::fe::{Fe, FeWide, FE_WORDS};

fn make_fe(seed: u64) -> Fe {
    let mut x = seed;
    let mut w = [0u32; FE_WORDS];
    for i in 0..FE_WORDS {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        w[i] = x as u32;
    }
    w[8] &= 0x1; // canonical: top bit only
    Fe(w)
}

#[test]
fn test_fmul_portable_vs_pclmul_50k_random() {
    if !std::is_x86_feature_detected!("pclmulqdq") {
        eprintln!("SKIP: CPU does not support PCLMULQDQ");
        return;
    }

    let mut state: u64 = 0xCAFE_BABE_DEAD_BEEF;
    let mut diverged = 0u32;
    let mut first_div: Option<(Fe, Fe, FeWide, FeWide)> = None;

    for iter in 0..50_000 {
        // Generate two random Fe values
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let a = make_fe(state);
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let b = make_fe(state);

        // Compute via portable backend
        let mut out_portable = FeWide::ZERO;
        backend::portable::fmul_257(&a, &b, &mut out_portable);

        // Compute via PCLMULQDQ backend
        let mut out_simd = FeWide::ZERO;
        unsafe {
            backend::x86_pclmul::fmul_257(&a, &b, &mut out_simd);
        }

        // The full FeWide arrays should be identical.
        // Note: portable produces 18-word output (FE_WIDE = 20 with slack);
        // SIMD produces 9 u64 = 18 u32 words in lower part.
        // We compare the meaningful product range (limbs 0..17).
        for i in 0..18 {
            if out_portable.0[i] != out_simd.0[i] {
                if first_div.is_none() {
                    first_div = Some((a, b, out_portable, out_simd));
                }
                diverged += 1;
                break;
            }
        }

        if diverged > 0 && iter < 5 {
            eprintln!("Iter {}: portable={:08x?}", iter, out_portable.0);
            eprintln!("Iter {}: simd    ={:08x?}", iter, out_simd.0);
        }
    }

    if diverged > 0 {
        if let Some((a, b, p, s)) = first_div {
            eprintln!(
                "First divergence:\n  a    = {:08x?}\n  b    = {:08x?}\n  port = {:08x?}\n  simd = {:08x?}",
                a.0, b.0, p.0, s.0
            );
        }
        panic!(
            "{} of 50000 fmul cases diverged between portable and PCLMULQDQ",
            diverged
        );
    }
}

/// Dedicated coverage for the top-bit fold region of `fmul_packed`. The
/// branch-based `if a_top == 1` / `if b_top == 1` pre-Sprint-2.1c4 code
/// broke exactly on these inputs (we caught it with dudect, not with
/// random sampling alone). The grid runs every combination of top-bit
/// patterns crossed with a few representative low-half values.
#[test]
fn test_fmul_top_bit_grid_portable_vs_pclmul() {
    if !std::is_x86_feature_detected!("pclmulqdq") {
        return;
    }

    // Build a helper for Fe with explicit (low_u64[4], top_bit) shape.
    fn fe_from(low: [u64; 4], top: u32) -> Fe {
        let mut w = [0u32; FE_WORDS];
        for i in 0..4 {
            w[2 * i] = low[i] as u32;
            w[2 * i + 1] = (low[i] >> 32) as u32;
        }
        w[8] = top & 1;
        Fe(w)
    }

    let interesting_lows: [[u64; 4]; 4] = [
        [0, 0, 0, 0],
        [1, 0, 0, 0],
        [u64::MAX, u64::MAX, u64::MAX, u64::MAX],
        [
            0xCAFE_BABE_DEAD_BEEF,
            0x0123_4567_89AB_CDEF,
            0,
            0xFFFF_FFFF_0000_0000,
        ],
    ];

    for &a_low in &interesting_lows {
        for a_top in 0..=1u32 {
            for &b_low in &interesting_lows {
                for b_top in 0..=1u32 {
                    let a = fe_from(a_low, a_top);
                    let b = fe_from(b_low, b_top);

                    let mut out_p = FeWide::ZERO;
                    backend::portable::fmul_257(&a, &b, &mut out_p);
                    let mut out_s = FeWide::ZERO;
                    unsafe {
                        backend::x86_pclmul::fmul_257(&a, &b, &mut out_s);
                    }

                    for i in 0..18 {
                        assert_eq!(
                            out_p.0[i], out_s.0[i],
                            "top-bit grid divergence at (a_top={}, b_top={}, a_low={:016x?}, \
                             b_low={:016x?}) limb {}: port={:08x} simd={:08x}",
                            a_top, b_top, a_low, b_low, i, out_p.0[i], out_s.0[i]
                        );
                    }
                }
            }
        }
    }
}

/// `fmul(a, a)` — same operand on both sides. Squaring in GF(2) is a
/// classical source of corner cases for Karatsuba-based multipliers
/// because `a + a = 0` makes one of the Karatsuba "middle" products
/// collapse. Must still match the portable schoolbook result bit-for-bit.
#[test]
fn test_fmul_self_square_portable_vs_pclmul() {
    if !std::is_x86_feature_detected!("pclmulqdq") {
        return;
    }
    let mut state: u64 = 0xDEAD_BEEF_0123_4567;
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let a = make_fe(state);

        let mut out_p = FeWide::ZERO;
        backend::portable::fmul_257(&a, &a, &mut out_p);
        let mut out_s = FeWide::ZERO;
        unsafe {
            backend::x86_pclmul::fmul_257(&a, &a, &mut out_s);
        }

        for i in 0..18 {
            assert_eq!(
                out_p.0[i], out_s.0[i],
                "self-square diverged: a = {:08x?}, limb {}: port={:08x} simd={:08x}",
                a.0, i, out_p.0[i], out_s.0[i]
            );
        }
    }
}

#[test]
fn test_fmul_known_inputs() {
    if !std::is_x86_feature_detected!("pclmulqdq") {
        return;
    }

    // Edge cases
    let zero = Fe::ZERO;
    let one = Fe::ONE;

    let cases = [
        (zero, zero, "0 * 0"),
        (zero, one, "0 * 1"),
        (one, one, "1 * 1"),
        (one, make_fe(0xCAFE), "1 * x"),
        (make_fe(0xCAFE), one, "x * 1"),
        (Fe([0xFFFF_FFFF; 9]), one, "all_ones * 1"),
    ];

    for (a, b, name) in cases.iter() {
        let mut a_canon = *a;
        a_canon.0[8] &= 1;
        let mut b_canon = *b;
        b_canon.0[8] &= 1;

        let mut out_portable = FeWide::ZERO;
        backend::portable::fmul_257(&a_canon, &b_canon, &mut out_portable);

        let mut out_simd = FeWide::ZERO;
        unsafe {
            backend::x86_pclmul::fmul_257(&a_canon, &b_canon, &mut out_simd);
        }

        for i in 0..18 {
            assert_eq!(
                out_portable.0[i], out_simd.0[i],
                "case '{}' diverges at limb {}: port={:08x} simd={:08x}",
                name, i, out_portable.0[i], out_simd.0[i]
            );
        }
    }
}

/// Verify the full Fe::mod_mul path via PCLMULQDQ produces same result as portable.
/// Tests reduction integration too (since PCLMULQDQ output goes through freduce_257).
#[test]
fn test_mod_mul_via_dispatch_matches_direct_portable() {
    use prro_crypto::core::gf2m_257;

    if !std::is_x86_feature_detected!("pclmulqdq") {
        return;
    }

    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    for _ in 0..1000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let a = make_fe(state);
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let b = make_fe(state);

        // Direct portable: portable fmul + portable freduce
        let mut wide_p = FeWide::ZERO;
        backend::portable::fmul_257(&a, &b, &mut wide_p);
        let mut reduced_p = [0u32; FE_WORDS];
        gf2m_257::freduce_257(&wide_p.0, &mut reduced_p);

        // Through dispatch: SIMD fmul + portable freduce
        let mut wide_d = FeWide::ZERO;
        backend::fmul_257(&a, &b, &mut wide_d);
        let mut reduced_d = [0u32; FE_WORDS];
        gf2m_257::freduce_257(&wide_d.0, &mut reduced_d);

        assert_eq!(
            reduced_p, reduced_d,
            "mod_mul mismatch: a={:08x?} b={:08x?}",
            a.0, b.0
        );
    }
}
