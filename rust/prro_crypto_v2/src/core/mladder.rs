//! López-Dahab x-only Montgomery ladder for binary curves.
//!
//! Replaces the wNAF scalar-mul path for operations on **secret scalars**
//! (ephemeral `rand_e` in sign). The ladder is constant-time by construction:
//!
//! - loop count is fixed at `NUM_BITS = 257` regardless of scalar value,
//! - every iteration performs exactly one `Mxdbl` and one `Mxadd`,
//! - branches on the scalar bit are replaced by constant-time `cswap` of
//!   the two ladder state points,
//! - table lookups by secret digit (the main wNAF leak) are absent.
//!
//! ## Cost relative to wNAF
//!
//! Per bit: 1 Mxdbl (4 sqr + 2 mul) + 1 Mxadd (4 mul + 1 sqr) = 5 sqr + 6 mul.
//! For 257 bits: ~1300 sqr + ~1500 mul + one final inversion.
//!
//! Measured on this crate's CT primitives the ladder costs ~2× a wNAF
//! scalar mul. The trade is worth it because the wNAF path cannot be
//! made constant-time without either masked table lookups (complex,
//! error-prone) or a full comb-method redesign.
//!
//! ## Scope
//!
//! `mul_base_x_ct` returns the x-coordinate of `k·G` in affine form —
//! DSTU 4145 sign only needs `eG.x`. If a future caller needs the
//! full affine point under CT, extend with a López-Dahab y-recovery
//! step (see Stam 2003 or HMV §3.40 for formulas).
//!
//! ## Curve assumption
//!
//! Formulas below are specialised to the binary Weierstrass form
//! `y² + xy = x³ + b` (i.e. `a = 0`). This covers all DSTU PB curves
//! currently in scope (DSTU_PB_257 is the only one shipped).

use crate::core::curve::Curve;
use crate::core::fe::{Fe, FE_WORDS};
use crate::core::field::FieldEl;

/// Typed error for the x-only Montgomery ladder.
///
/// The ladder is a low-level arithmetic primitive — every precondition
/// it can surface to the caller goes here instead of being swallowed
/// by silent truncation or a `debug_assert!` that only fires in debug
/// builds.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LadderError {
    /// The caller supplied a scalar whose FieldEl backing vector has
    /// MORE words than `curve.mod_words`, and at least one of those
    /// high words is non-zero. A purely zero-padded oversized scalar
    /// is NOT an error — that represents the same integer and we
    /// accept it for API ergonomics.
    #[error("scalar too wide: {provided_words} words exceeds curve max {max_words}")]
    ScalarTooWide {
        provided_words: usize,
        max_words: usize,
    },
}

/// Compute `x(k·G)` in affine form using a constant-time Montgomery ladder.
///
/// Returns `None` only when `k ≡ 0 (mod n)` — in that case `k·G = O` and
/// no affine x exists. The caller (sign path) retries with a fresh scalar.
///
/// **Note:** since Sprint 3.1 the sign path uses the comb method
/// ([`crate::core::comb::mul_base_comb_x_ct`]) instead. This function
/// remains as a CT reference implementation for differential testing and
/// for the ECDH path (`mul_point_x_ct`).
///
/// # CT guarantees
///
/// Subject to the constant-time contracts of `Fe::mod_mul`, `Fe::mod_sqr`,
/// and `Fe::invert`, every control-flow path through this function is
/// independent of `k`. See the module doc for the caveats about initial
/// CPU-feature detection on x86_64 (one-time at process start).
#[cfg(test)]
pub fn mul_base_x_ct(k: &FieldEl, curve: &Curve) -> Option<FieldEl> {
    // The base-point variant keeps the old infallible `Option` shape
    // because the signing hot path doesn't benefit from a richer error:
    // a scalar with non-zero bits past the curve width is already
    // filtered earlier in `sign()` via `try_as_fe_words` / word-8 check.
    // Any ladder-reported error here is treated as "reject this rand_e".
    mul_point_x_ct(&curve.base_x, k, curve).ok().flatten()
}

/// Compute `x(k·P)` in affine form for an arbitrary point P given only
/// its x-coordinate, using a constant-time Montgomery ladder.
///
/// This is the ECDH-friendly variant of [`mul_base_x_ct`]. The x-only
/// ladder doesn't need P's y-coordinate, which is why it's a natural
/// fit for ECDH: we consume just `Q.x`, `k`, and the curve, and produce
/// `(k·Q).x` without ever handling a full affine point under secret
/// inputs.
///
/// Returns `None` when `k·P = O` (point at infinity); caller decides
/// whether that's a fault (signing) or a legitimate corner (ECDH).
///
/// # CT guarantees
///
/// Identical to [`mul_base_x_ct`]: the only thing that changes from
/// the base-point variant is which x-coordinate seeds the ladder state.
/// `P.x` is treated as public (it's a counterparty's pubkey), so
/// using it as an input to `mod_mul` / `mod_sqr` does not widen the
/// secret surface beyond what the scalar `k` already exposes.
pub fn mul_point_x_ct(
    p_x: &FieldEl,
    k: &FieldEl,
    curve: &Curve,
) -> Result<Option<FieldEl>, LadderError> {
    let x_p: Fe = p_x.into();
    let b_fe: Fe = (&curve.b).into();

    // Fail-closed width check. Previous behaviour silently truncated
    // scalars wider than `curve.mod_words` in release builds — which
    // turned a hostile-or-buggy caller passing cryptographic material
    // beyond the ladder's capacity into a "quietly wrong result"
    // outcome. Now: we accept zero-padded oversized vectors (same
    // integer, no data loss) and reject any genuine oversized scalar.
    let mod_words = curve.mod_words;
    if k.bytes.len() > mod_words {
        for &w in &k.bytes[mod_words..] {
            if w != 0 {
                return Err(LadderError::ScalarTooWide {
                    provided_words: k.bytes.len(),
                    max_words: mod_words,
                });
            }
        }
    }
    let mut k_words = vec![0u32; mod_words];
    let copy_len = k.bytes.len().min(mod_words);
    k_words[..copy_len].copy_from_slice(&k.bytes[..copy_len]);
    let num_bits = mod_words * 32;

    // Ladder state. R0 starts at the identity (encoded as (1, 0) so that
    // Mxdbl and Mxadd on it behave correctly — see the unit tests for
    // worked examples). R1 starts at the base point P.
    let mut x0 = Fe::ONE;
    let mut z0 = Fe::ZERO;
    let mut x1 = x_p;
    let mut z1 = Fe::ONE;

    for bit_idx in (0..num_bits).rev() {
        let bit = extract_bit(&k_words, bit_idx);

        cswap_fe(&mut x0, &mut x1, bit);
        cswap_fe(&mut z0, &mut z1, bit);

        // After the cswap, `R0` is the point that will be doubled and
        // `R1` the one that receives the differential add. The branch
        // on the scalar bit is absorbed into the swap — the formulas
        // themselves are branch-free.
        let (new_x1, new_z1) = mxadd(&x0, &z0, &x1, &z1, &x_p);
        let (new_x0, new_z0) = mxdbl(&x0, &z0, &b_fe);

        x0 = new_x0;
        z0 = new_z0;
        x1 = new_x1;
        z1 = new_z1;

        cswap_fe(&mut x0, &mut x1, bit);
        cswap_fe(&mut z0, &mut z1, bit);
    }

    // R0 = k·P in projective (X0, Z0). Convert to affine x = X0/Z0.
    // Z0 == 0 means k·P is the point at infinity (k ≡ 0 mod n).
    if z0.ct_is_zero() {
        return Ok(None);
    }
    let z_inv = z0.invert();
    let x_affine = x0.mod_mul(&z_inv);
    Ok(Some((&x_affine).into()))
}

// ─── López-Dahab (a=0) projective x-only formulas ───────────────────────────

/// Mxdbl: 2·(X₁, Z₁) for the curve y² + xy = x³ + b.
///
/// `x(2P) = x² + b/x²`; in projective form
/// `X₃ = X₁⁴ + b·Z₁⁴`, `Z₃ = X₁²·Z₁²`.
#[inline]
fn mxdbl(x1: &Fe, z1: &Fe, b: &Fe) -> (Fe, Fe) {
    let x1_sqr = x1.mod_sqr();
    let z1_sqr = z1.mod_sqr();
    let z3 = x1_sqr.mod_mul(&z1_sqr); // X₁²·Z₁²
    let x1_quad = x1_sqr.mod_sqr(); // X₁⁴
    let z1_quad = z1_sqr.mod_sqr(); // Z₁⁴
    let b_z4 = b.mod_mul(&z1_quad);
    let x3 = x1_quad.add(&b_z4);
    (x3, z3)
}

/// Mxadd: differential addition `R_a + R_b` given that `R_a - R_b = P`
/// (whose x-coordinate `xP` is supplied). For the curve y² + xy = x³ + b.
///
/// Let `A = X₁·Z₂`, `B = X₂·Z₁`, `C = A + B`, `D = A·B`. Then
/// `Z₃ = C²`, `X₃ = xP·Z₃ + D`.
#[inline]
fn mxadd(x1: &Fe, z1: &Fe, x2: &Fe, z2: &Fe, x_p: &Fe) -> (Fe, Fe) {
    let a = x1.mod_mul(z2);
    let b = x2.mod_mul(z1);
    let c = a.add(&b);
    let d = a.mod_mul(&b);
    let z3 = c.mod_sqr();
    let xp_z3 = x_p.mod_mul(&z3);
    let x3 = xp_z3.add(&d);
    (x3, z3)
}

// ─── CT primitives ──────────────────────────────────────────────────────────

/// Extract bit `bit_idx` (little-endian across words) from a `u32` slice.
/// Returns 0 when `bit_idx` is past the end — preserves the CT property
/// that the output depends only on the bit value at a fixed position.
#[inline]
fn extract_bit(words: &[u32], bit_idx: usize) -> u32 {
    let wi = bit_idx / 32;
    let bi = bit_idx % 32;
    if wi >= words.len() {
        return 0;
    }
    (words[wi] >> bi) & 1
}

/// Constant-time conditional swap of two `Fe` values.
///
/// When `bit == 1` the two inputs are swapped; when `bit == 0` they are
/// left unchanged. The swap is delegated to
/// `subtle::ConditionallySelectable::conditional_swap` per `u32` limb
/// (Sprint 2.1c5.2). `subtle` wraps the `Choice` in an opacity barrier
/// so a future compiler cannot "prove" the bit statically and fold the
/// swap into a branch.
#[inline]
fn cswap_fe(a: &mut Fe, b: &mut Fe, bit: u32) {
    debug_assert!(bit == 0 || bit == 1);
    let choice = subtle::Choice::from(bit as u8);
    for i in 0..FE_WORDS {
        <u32 as subtle::ConditionallySelectable>::conditional_swap(
            &mut a.0[i],
            &mut b.0[i],
            choice,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fixed_base;

    /// Helper — build a FieldEl from a single-word value.
    fn k_from_u32(v: u32, mod_words: usize) -> FieldEl {
        let mut w = vec![0u32; mod_words];
        w[0] = v;
        FieldEl::from_words(w)
    }

    /// Helper — ladder result equals wNAF result x-coord (or both are infinity).
    fn assert_ladder_matches_wnaf(k: &FieldEl, curve: &Curve, label: &str) {
        let via_wnaf = fixed_base::mul_base(k, curve);
        let via_ladder = mul_base_x_ct(k, curve);

        match (via_wnaf.is_zero(), via_ladder.is_none()) {
            (true, true) => return,
            (false, true) => panic!(
                "{}: wnaf returned non-infinity x = {:?} but ladder returned None",
                label, via_wnaf.x.bytes
            ),
            (true, false) => panic!(
                "{}: wnaf returned infinity but ladder returned x = {:?}",
                label,
                via_ladder.as_ref().unwrap().bytes
            ),
            (false, false) => {
                let ladder = via_ladder.unwrap();
                assert!(
                    via_wnaf.x.equals(&ladder),
                    "{}: wnaf.x = {:?}, ladder = {:?}",
                    label,
                    via_wnaf.x.bytes,
                    ladder.bytes
                );
            }
        }
    }

    /// Tiny scalars — smoke test before the heavier diffs.
    #[test]
    fn ladder_matches_wnaf_small_scalars() {
        let curve = Curve::dstu_pb_257();
        for k_small in 1u32..=32 {
            let k = k_from_u32(k_small, curve.mod_words);
            assert_ladder_matches_wnaf(&k, &curve, &format!("k={}", k_small));
        }
    }

    /// Edge cases around 0, 1, 2, `n-1`, `n`, `n+1`, `2^256`, all-ones.
    /// These probe every boundary where a naive ladder might misbehave.
    #[test]
    fn ladder_edge_cases() {
        let curve = Curve::dstu_pb_257();
        let mw = curve.mod_words;

        // k = 0 → both paths return infinity.
        assert_ladder_matches_wnaf(&FieldEl::zero(mw), &curve, "k=0");

        // k = 1 → both paths return G.
        assert_ladder_matches_wnaf(&k_from_u32(1, mw), &curve, "k=1");

        // k = 2 → both paths return 2·G.
        assert_ladder_matches_wnaf(&k_from_u32(2, mw), &curve, "k=2");

        // k = n − 1 → equals −G, and on binary curves x(−G) = x(G).
        // We also use this below as an independent algebraic check.
        let n_minus_1 = {
            let mut b = curve.order.bytes.clone();
            // Subtract 1 (order is odd → just decrement low word).
            b[0] = b[0].wrapping_sub(1);
            FieldEl::from_words(b)
        };
        assert_ladder_matches_wnaf(&n_minus_1, &curve, "k=n-1");
        let ladder_x_nm1 = mul_base_x_ct(&n_minus_1, &curve)
            .expect("(n-1)·G is not the identity");
        assert!(
            ladder_x_nm1.equals(&curve.base_x),
            "x((n-1)·G) must equal x(G) on binary curves — got {:?}",
            ladder_x_nm1.bytes
        );

        // k = n → both paths return infinity.
        assert_ladder_matches_wnaf(&curve.order, &curve, "k=n");

        // k = n + 1 → equals 1·G = G.
        let n_plus_1 = {
            let mut b = curve.order.bytes.clone();
            // Order low word is odd-ish; +1 may carry. Be conservative:
            let (s0, c) = b[0].overflowing_add(1);
            b[0] = s0;
            let mut carry = c as u32;
            let mut i = 1;
            while carry != 0 && i < b.len() {
                let (s, c2) = b[i].overflowing_add(carry);
                b[i] = s;
                carry = c2 as u32;
                i += 1;
            }
            FieldEl::from_words(b)
        };
        assert_ladder_matches_wnaf(&n_plus_1, &curve, "k=n+1");
        let ladder_x_np1 = mul_base_x_ct(&n_plus_1, &curve)
            .expect("(n+1)·G = G is not the identity");
        assert!(
            ladder_x_np1.equals(&curve.base_x),
            "x((n+1)·G) must equal x(G) — got {:?}",
            ladder_x_np1.bytes
        );

        // k = 2^256 — a scalar with exactly one very high bit.
        let pow_256 = {
            let mut w = vec![0u32; mw];
            w[8] = 1; // bit 256 = word 8 bit 0
            FieldEl::from_words(w)
        };
        assert_ladder_matches_wnaf(&pow_256, &curve, "k=2^256");

        // All-ones across the full FieldEl width — stresses every bit slot.
        let all_ones = FieldEl::from_words(vec![u32::MAX; mw]);
        assert_ladder_matches_wnaf(&all_ones, &curve, "k=all-ones");

        // A scalar with just the topmost bit of the FieldEl set — probes the
        // dynamic NUM_BITS logic against wNAF which processes the same range.
        let top_bit_only = {
            let mut w = vec![0u32; mw];
            w[mw - 1] = 1u32 << 31;
            FieldEl::from_words(w)
        };
        assert_ladder_matches_wnaf(&top_bit_only, &curve, "k=2^(mod_words*32-1)");
    }

    /// Large random batch. A bug anywhere in the ladder formulas or the
    /// cswap would surface as a mismatch somewhere in the batch. 4096 is
    /// enough to drown out per-scalar-flakiness and close to the effective
    /// upper limit of what's practical to run in-tree.
    #[test]
    fn ladder_matches_wnaf_random_scalars_4k() {
        let curve = Curve::dstu_pb_257();
        let mw = curve.mod_words;
        let mut x: u32 = 0xC0FE_BABE;
        for trial in 0..4096u32 {
            let mut words = vec![0u32; mw];
            for w in words.iter_mut().take(8) {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *w = x;
            }
            let k = FieldEl::from_words(words);
            if k.is_zero() {
                continue;
            }
            assert_ladder_matches_wnaf(&k, &curve, &format!("trial {}", trial));
        }
    }

    /// Scalars spanning the full 288-bit FieldEl width (not only the
    /// 256-bit sub-range). Before the dynamic `num_bits` fix these would
    /// silently compute `(k mod 2^257)·G` — this test guards against that
    /// regression.
    #[test]
    fn ladder_matches_wnaf_full_width_scalars() {
        let curve = Curve::dstu_pb_257();
        let mw = curve.mod_words;
        let mut x: u32 = 0xDEAD_BEEF;
        for trial in 0..256u32 {
            let mut words = vec![0u32; mw];
            for w in words.iter_mut() {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *w = x;
            }
            let k = FieldEl::from_words(words);
            if k.is_zero() {
                continue;
            }
            assert_ladder_matches_wnaf(
                &k,
                &curve,
                &format!("full-width trial {}", trial),
            );
        }
    }

    /// `k = order` must produce the point at infinity.
    #[test]
    fn ladder_order_times_g_is_infinity() {
        let curve = Curve::dstu_pb_257();
        let result = mul_base_x_ct(&curve.order, &curve);
        assert!(result.is_none(), "order·G must land on infinity (None)");
    }

    /// `cswap_fe` smoke test — the CT primitive underpinning the ladder's
    /// secret-independence claim. A compiler that removed the masked path
    /// on `bit == 0` would still pass this, but the shape of the output
    /// confirms at least the semantics are right.
    #[test]
    fn cswap_semantics() {
        let a0 = Fe::from_hex("CAFE");
        let b0 = Fe::from_hex("BABE");

        // No swap.
        let mut a = a0;
        let mut b = b0;
        cswap_fe(&mut a, &mut b, 0);
        assert!(a.ct_eq(&a0));
        assert!(b.ct_eq(&b0));

        // Swap.
        let mut a = a0;
        let mut b = b0;
        cswap_fe(&mut a, &mut b, 1);
        assert!(a.ct_eq(&b0));
        assert!(b.ct_eq(&a0));
    }

    /// Within the supported scalar-width range (≤ `curve.mod_words` words),
    /// ladder output must be independent of how many zero-padding words
    /// the caller happened to attach. Before the fixed-width normalization
    /// fix, shorter inputs produced a shorter loop and diverged from the
    /// canonical 9-word path.
    #[test]
    fn ladder_output_length_invariant_within_contract() {
        let curve = Curve::dstu_pb_257();
        let narrow = FieldEl::from_words(vec![0xDEAD_BEEFu32]);
        let canonical = {
            let mut w = vec![0u32; curve.mod_words];
            w[0] = 0xDEAD_BEEF;
            FieldEl::from_words(w)
        };

        let via_narrow = mul_base_x_ct(&narrow, &curve).expect("non-identity");
        let via_canonical =
            mul_base_x_ct(&canonical, &curve).expect("non-identity");

        assert!(
            via_narrow.equals(&via_canonical),
            "narrow vs canonical diverged: {:?} != {:?}",
            via_narrow.bytes,
            via_canonical.bytes
        );
    }

    /// Replaces the old `ladder_rejects_oversized_scalar_in_debug`
    /// `should_panic` test: the ladder is now fail-closed in all build
    /// modes. An oversized scalar with non-zero tail returns
    /// `LadderError::ScalarTooWide` instead of truncating.
    ///
    /// `mul_base_x_ct` wraps the low-level result into `Option` — on
    /// error it yields `None`, so signing callers reject the bad
    /// `rand_e` as "draw a fresh one".
    #[test]
    fn ladder_base_rejects_oversized_scalar_release_safe() {
        let curve = Curve::dstu_pb_257();
        let oversized = {
            let mut w = vec![0u32; curve.mod_words + 3];
            w[0] = 0xDEAD_BEEF;
            w[curve.mod_words] = 0x0000_0001; // non-zero tail
            FieldEl::from_words(w)
        };
        assert!(mul_base_x_ct(&oversized, &curve).is_none());
    }

    /// Differential test for `mul_point_x_ct` against the wNAF
    /// reference `Point::mul`. For a batch of random *valid* Q (each
    /// generated as `r·G` for random r, so on-curve and in the prime
    /// subgroup by construction) and random scalars k, the ladder's
    /// x(k·Q) must equal the x-coordinate of wNAF's `Q.mul(k)`.
    ///
    /// This closes the audit's "the CT fix is there but not
    /// differential-tested for arbitrary Q" gap.
    #[test]
    fn ladder_point_matches_wnaf_random_valid_q() {
        use crate::core::point::Point;
        let curve = Curve::dstu_pb_257();
        let mw = curve.mod_words;

        // Deterministic "random" sequence — we want a diff harness,
        // not a probabilistic pass/fail. Use an xorshift64 seeded from
        // a fixed value so a regression shows up in the same slot.
        let mut state: u64 = 0x4DC51_E1C0_DE_BA5Eu64;
        let mut next_u32 = || -> u32 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xFFFF_FFFF) as u32
        };

        const N: usize = 64;
        for trial in 0..N {
            // Build a random scalar r (256-bit width, high word = 0)
            // and use it to generate a valid Q via the base-point wNAF.
            let mut r_words = vec![0u32; mw];
            for w in &mut r_words[..mw.min(8)] {
                *w = next_u32();
            }
            let r = FieldEl::from_words(r_words);
            let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
            let q = g.mul(&r, &curve);
            if q.is_zero() {
                continue; // `r ≡ 0 mod n` — skip and draw again
            }

            // Random scalar k with the same width discipline.
            let mut k_words = vec![0u32; mw];
            for w in &mut k_words[..mw.min(8)] {
                *w = next_u32();
            }
            let k = FieldEl::from_words(k_words);

            let via_wnaf = q.mul(&k, &curve);
            let via_ladder = mul_point_x_ct(&q.x, &k, &curve)
                .expect("random-width scalar should not trip ScalarTooWide");

            match (via_wnaf.is_zero(), via_ladder.is_none()) {
                (true, true) => continue,
                (false, false) => {
                    let ladder_x = via_ladder.unwrap();
                    assert!(
                        via_wnaf.x.equals(&ladder_x),
                        "trial {}: wnaf x = {:?}, ladder x = {:?}",
                        trial,
                        via_wnaf.x.bytes,
                        ladder_x.bytes,
                    );
                }
                (wnaf_inf, ladder_inf) => panic!(
                    "trial {}: infinity mismatch — wnaf_inf={}, ladder_inf={}",
                    trial, wnaf_inf, ladder_inf
                ),
            }
        }
    }

    /// Edge case: ladder on the base point with cofactor-inflated
    /// scalars — this is the exact shape the ECDH path feeds through
    /// (`s = d · h`).
    #[test]
    fn ladder_point_matches_wnaf_cofactor_scaled_scalars() {
        use crate::core::point::Point;
        let curve = Curve::dstu_pb_257();
        let mw = curve.mod_words;
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

        for d_u32 in [1u32, 2, 3, 7, 0xABCD_1234, 0xFFFF_FFFF] {
            let d = k_from_u32(d_u32, mw);
            let q = g.mul(&d, &curve).negate(); // Q = -d·G (DSTU convention)

            // Feed Q through the ladder with a handful of scalars,
            // including the cofactor value (h=4 for pb257) which is
            // literally what ecdh_zz exercises.
            for k_u32 in [1u32, 2, 4, 13, 0xDEAD_BEEF] {
                let k = k_from_u32(k_u32, mw);
                let via_wnaf = q.mul(&k, &curve);
                let via_ladder = mul_point_x_ct(&q.x, &k, &curve)
                    .expect("narrow scalar never trips ScalarTooWide");
                if via_wnaf.is_zero() {
                    assert!(via_ladder.is_none());
                } else {
                    let ladder = via_ladder.unwrap();
                    assert!(
                        via_wnaf.x.equals(&ladder),
                        "d={d_u32} k={k_u32}: wnaf x vs ladder x differ"
                    );
                }
            }
        }
    }

    /// Fail-closed contract: a scalar whose backing vector has more
    /// words than `curve.mod_words` AND carries non-zero data in the
    /// high tail must be rejected with `ScalarTooWide`. Previous
    /// behaviour silently truncated in release builds.
    #[test]
    fn mul_point_x_ct_rejects_nonzero_oversized_scalar() {
        let curve = Curve::dstu_pb_257();
        let mut oversized = vec![1u32; curve.mod_words];
        oversized.push(0xDEADBEEF); // non-zero tail → must be rejected
        let k = FieldEl::from_words(oversized);
        let err = mul_point_x_ct(&curve.base_x, &k, &curve).unwrap_err();
        match err {
            LadderError::ScalarTooWide { provided_words, max_words } => {
                assert_eq!(provided_words, curve.mod_words + 1);
                assert_eq!(max_words, curve.mod_words);
            }
        }
    }

    /// Harmless zero-padding in the high words must NOT trip
    /// `ScalarTooWide` — otherwise every caller that over-allocates
    /// for ergonomic reasons would break.
    #[test]
    fn mul_point_x_ct_accepts_zero_padded_oversized_scalar() {
        let curve = Curve::dstu_pb_257();
        // Canonical narrow scalar.
        let narrow = k_from_u32(7, curve.mod_words);
        // Same integer, but with 3 zero words appended.
        let mut padded_words = vec![0u32; curve.mod_words + 3];
        padded_words[0] = 7;
        let padded = FieldEl::from_words(padded_words);

        let via_narrow = mul_point_x_ct(&curve.base_x, &narrow, &curve)
            .expect("narrow scalar must not error")
            .expect("k=7 is not identity");
        let via_padded = mul_point_x_ct(&curve.base_x, &padded, &curve)
            .expect("zero-padded oversized scalar must be accepted")
            .expect("k=7 is not identity");
        assert!(
            via_narrow.equals(&via_padded),
            "zero-padded scalar must yield the same x as its narrow form"
        );
    }

    /// `extract_bit` must return 0/1 without panicking past the end of the
    /// backing slice. The ladder relies on this to keep the loop count
    /// decoupled from scalar bit-length.
    #[test]
    fn extract_bit_out_of_range_is_zero() {
        let words = vec![0xFFFF_FFFFu32; 2]; // 64 bits of ones
        assert_eq!(extract_bit(&words, 0), 1);
        assert_eq!(extract_bit(&words, 31), 1);
        assert_eq!(extract_bit(&words, 63), 1);
        assert_eq!(extract_bit(&words, 64), 0, "past end must be 0");
        assert_eq!(extract_bit(&words, 999), 0, "far past end must be 0");
    }
}
