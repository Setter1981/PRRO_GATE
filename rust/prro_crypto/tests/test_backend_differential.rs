//! Differential test suite: portable backend vs PCLMULQDQ backend.
//!
//! 10,000 random Fe pairs multiplied through both backends. Any divergence
//! is a CRITICAL bug — it would silently corrupt one in N signatures in
//! production. This is the gate before enabling SIMD dispatch.

use prro_crypto::backend;
use prro_crypto::fe::{Fe, FeWide, FE_WORDS};

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
fn test_fmul_portable_vs_pclmul_10k_random() {
    if !std::is_x86_feature_detected!("pclmulqdq") {
        eprintln!("SKIP: CPU does not support PCLMULQDQ");
        return;
    }

    let mut state: u64 = 0xCAFE_BABE_DEAD_BEEF;
    let mut diverged = 0u32;
    let mut first_div: Option<(Fe, Fe, FeWide, FeWide)> = None;

    for iter in 0..10_000 {
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
        panic!("{} of 10000 fmul cases diverged between portable and PCLMULQDQ", diverged);
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
        (
            Fe([0xFFFF_FFFF; 9]),
            one,
            "all_ones * 1",
        ),
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
    use prro_crypto::gf2m_257;

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
