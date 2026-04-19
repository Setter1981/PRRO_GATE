//! Fixed-base scalar multiplication for the DSTU PB-257 base point G.
//!
//! Since Sprint 2.1, the production signing path uses the
//! constant-time Montgomery ladder (`mladder::mul_base_x_ct`).
//! This module's `mul_base` is retained as the **wNAF reference
//! implementation** against which the ladder is differentially
//! tested — see `mladder::assert_ladder_matches_wnaf`. It is NOT
//! compiled into release builds.
#![cfg(test)]
//!
//! Implements `mul_base(k) = k * G` using a precomputed table of odd
//! multiples of G. The table is built once at first use and cached for
//! the lifetime of the process via `OnceLock`. Subsequent signs reuse
//! the table — no per-call precomputation.
//!
//! ## Why this is a win for signing
//!
//! Without this module, every `Priv::sign` call rebuilds the wNAF
//! precomputation table inside `Point::mul`:
//!   - 1 affine doubling: ~30 µs
//!   - 3 affine additions: ~84 µs
//!   - **Total: ~114 µs of pure overhead per sign**
//!
//! With this module, that overhead disappears entirely. The table sits
//! in process memory (a few KB), shared across all sign operations.
//!
//! ## Algorithm
//!
//! Window size `W = 4`. Precomputed table holds: G, 3G, 5G, 7G (4 points).
//! Scalar `k` is recoded into wNAF with width 4 (using existing
//! `wnaf::compute_wnaf`). Main loop processes wNAF digits MSB-to-LSB:
//! one Lopez-Dahab projective doubling per bit, one mixed addition with
//! the (signed) precomputed point per non-zero digit.
//!
//! ## Future improvements
//!
//! - Wider window (W=6 or W=8) for fewer additions per scalar mul
//! - Comb method for further speedup
//! - Constant-time table selection for production hardening
//!
//! For now this is the minimal implementation that delivers the expected
//! per-call speedup without altering semantics.

use std::sync::OnceLock;

use crate::core::curve::Curve;
use crate::core::field::FieldEl;
use crate::core::point::Point;
use crate::core::proj::ProjPoint;

/// Window width for wNAF recoding.
/// W=6: table 16 × ~80 bytes = 1.3 KB; ~43 additions per scalar mul.
/// W=8 also tested — same speed, 4x more memory. W=6 is the sweet spot
/// until comb method or PCLMULQDQ unlock more parallelism.
const WINDOW: usize = 6;
const TABLE_SIZE: usize = 1 << (WINDOW - 2); // 16 entries

/// Lazily-computed table of odd multiples of G.
/// `TABLE.get_or_init` is called the first time `mul_base` runs,
/// then every subsequent call uses the cached table.
static TABLE: OnceLock<[Point; TABLE_SIZE]> = OnceLock::new();

fn build_table(curve: &Curve) -> [Point; TABLE_SIZE] {
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let two_g = g.twice(curve);

    let mut table: Vec<Point> = Vec::with_capacity(TABLE_SIZE);
    table.push(g.clone()); // 1G
    let mut acc = g.clone();
    for _ in 1..TABLE_SIZE {
        acc = acc.add(&two_g, curve);
        table.push(acc.clone());
    }

    // Convert to fixed-size array
    let mut arr: [Point; TABLE_SIZE] = std::array::from_fn(|_| g.clone());
    for (i, p) in table.into_iter().enumerate() {
        arr[i] = p;
    }
    arr
}

/// Compute k * G using the precomputed table.
///
/// `k` must be a non-negative scalar; output is affine.
/// First call initializes the table (~few hundred microseconds);
/// subsequent calls are pure scalar multiplication.
pub fn mul_base(k: &FieldEl, curve: &Curve) -> Point {
    if k.is_zero_ct() {
        return Point::zero(curve.mod_words);
    }

    let table = TABLE.get_or_init(|| build_table(curve));

    // wNAF recoding (window 4 — matches table layout)
    let naf = crate::core::wnaf::compute_wnaf(&k.bytes, WINDOW);
    if naf.is_empty() {
        return Point::zero(curve.mod_words);
    }

    // Main loop: projective accumulator, mixed addition with precomputed affine points.
    let mut r = ProjPoint::zero(curve.mod_words);
    for &digit in naf.iter().rev() {
        if !r.is_zero() {
            r = r.double(curve);
        }
        if digit != 0 {
            let abs = digit.unsigned_abs() as usize;
            let idx = (abs - 1) / 2;
            let p = if digit > 0 {
                table[idx].clone()
            } else {
                table[idx].negate()
            };
            r = r.madd_affine(&p, curve);
        }
    }

    r.to_affine(curve)
}

/// Force-initialize the table. Useful for warm-up benchmarking; not
/// required for correctness (table is initialized lazily on first call).
pub fn warm_up(curve: &Curve) {
    let _ = TABLE.get_or_init(|| build_table(curve));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::field::FieldEl;

    #[test]
    fn test_mul_base_matches_generic_mul_small() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

        for k in 1u32..=20 {
            let mut words = vec![0u32; curve.mod_words];
            words[0] = k;
            let scalar = FieldEl::from_words(words);
            let via_generic = g.mul_naive(&scalar, &curve);
            let via_fixed_base = mul_base(&scalar, &curve);
            assert!(
                via_generic.equals(&via_fixed_base),
                "fixed_base mismatch at k={}",
                k
            );
        }
    }

    #[test]
    fn test_mul_base_matches_generic_full_scalar() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

        let mut words = vec![0u32; curve.mod_words];
        for i in 0..8 {
            words[i] = (0xCAFE_BABEu32).wrapping_mul((i as u32) + 1);
        }
        let scalar = FieldEl::from_words(words);

        let via_generic = g.mul_naive(&scalar, &curve);
        let via_fixed_base = mul_base(&scalar, &curve);
        assert!(via_generic.equals(&via_fixed_base));
    }

    #[test]
    fn test_mul_base_order_is_infinity() {
        let curve = Curve::dstu_pb_257();
        let result = mul_base(&curve.order, &curve);
        assert!(result.is_zero(), "order * G via fixed_base should be infinity");
    }
}
