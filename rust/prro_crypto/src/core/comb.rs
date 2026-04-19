//! Fixed-base comb method for constant-time scalar multiplication k·G.
//!
//! Precomputes a 16-point table of affine combinations of G at column
//! offsets, then evaluates k·G in 65 doubling+addition rounds using
//! López-Dahab projective coordinates on stack-allocated [`Fe`] values.
//!
//! ## Cost
//!
//! | Path            | Doublings | Muls+Sqrs (Fe) | Estimated |
//! |-----------------|-----------|----------------|-----------|
//! | Montgomery      |       257 | 6M+5S / step   |   ~259 µs |
//! | **Comb w=4**    |        65 | 12M+10S / step |   ~165 µs |
//!
//! ## CT guarantees
//!
//! - Loop count is fixed at 65 regardless of scalar value.
//! - Every iteration runs the same sequence of Fe operations.
//! - Table lookup is constant-time (full scan with masked copy).
//! - The accumulator starts at G (blinding), not identity, so no
//!   identity-handling branches appear in the double/madd formulas.
//! - A correction point `-2^65 · G` is subtracted at the end.
//!
//! ## Layout
//!
//! Window `W = 4`, column count `D = ceil(257/4) = 65`.
//! At each column `i` (0..65), a 4-bit value is formed from scalar bits
//! `{i, i+65, i+130, i+195}`. The precomputed table maps each 4-bit
//! value to the affine point `sum_j (bit_j * 2^{j*65} * G)`.

use std::sync::OnceLock;

use subtle::ConditionallySelectable;
use zeroize::Zeroize;

use crate::core::curve::Curve;
use crate::core::fe::{Fe, FE_WORDS};
use crate::core::field::FieldEl;
use crate::core::point::Point;

/// Comb window width.
const W: usize = 4;
/// Number of table entries: 2^W.
const TABLE_SIZE: usize = 1 << W; // 16
/// Number of columns: ceil(257 / W). Bit indices at column i:
/// i, i+D, i+2D, i+3D.
const D: usize = 65;
/// Maximum scalar bit index we ever read.
const MAX_SCALAR_BITS: usize = 256;

// ── Precomputed table ──────────────────────────────────────────────────

/// Affine point in Fe representation (no heap allocation).
/// Shared with Shamir's trick (proj.rs).
#[derive(Clone, Copy)]
pub(crate) struct FeAffine {
    pub(crate) x: Fe,
    pub(crate) y: Fe,
}

/// Precomputed comb table + correction point.
struct CombTable {
    /// T[0..16]: affine points for each 4-bit combination.
    /// T[0] is a dummy (any valid point — never used in the result
    /// because the CT select bypasses it when the column value is 0).
    table: [FeAffine; TABLE_SIZE],
    /// Blinding correction: affine negation of `2^D · G`.
    /// Added at the end to cancel the blinding term.
    correction: FeAffine,
    /// Curve constant b as Fe (needed by the LD double formula).
    b: Fe,
}

/// Lazily-computed table, initialized on first sign call.
static COMB_TABLE: OnceLock<CombTable> = OnceLock::new();

fn build_table(curve: &Curve) -> CombTable {
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

    // Step 1: compute the 4 "row generators" by repeated doubling.
    // row[0] = G, row[1] = 2^D·G, row[2] = 2^(2D)·G, row[3] = 2^(3D)·G.
    let mut rows: [Point; W] = std::array::from_fn(|_| g.clone());
    let mut acc = g.clone();
    for j in 1..W {
        for _ in 0..D {
            acc = acc.twice(curve);
        }
        rows[j] = acc.clone();
    }

    // Step 2: build the 16-entry table.
    // T[b] = sum over j where bit j of b is set: rows[j].
    let zero = Point::zero(curve.mod_words);
    let mut table_points: Vec<Point> = Vec::with_capacity(TABLE_SIZE);
    for b in 0..TABLE_SIZE {
        let mut p = zero.clone();
        for j in 0..W {
            if (b >> j) & 1 == 1 {
                p = p.add(&rows[j], curve);
            }
        }
        table_points.push(p);
    }

    // T[0] is identity — store T[1] (= G) as a safe dummy value for the
    // CT table lookup. The result is discarded by the conditional select.
    let dummy = table_points[1].clone();

    let mut table: [FeAffine; TABLE_SIZE] = [FeAffine {
        x: Fe::ZERO,
        y: Fe::ZERO,
    }; TABLE_SIZE];
    for (i, p) in table_points.iter().enumerate() {
        if p.is_zero() {
            // Identity — use dummy
            table[i] = FeAffine {
                x: (&dummy.x).into(),
                y: (&dummy.y).into(),
            };
        } else {
            table[i] = FeAffine {
                x: (&p.x).into(),
                y: (&p.y).into(),
            };
        }
    }

    // Step 3: correction point = -(2^D · G).
    // The blinding starts the accumulator at G; after D rounds of doubling
    // the blinding term has been doubled D times → 2^D · G. We subtract it.
    let two_d_g = &rows[1]; // rows[1] = 2^D · G
    let neg_two_d_g = two_d_g.negate();
    let correction = FeAffine {
        x: (&neg_two_d_g.x).into(),
        y: (&neg_two_d_g.y).into(),
    };

    let b: Fe = (&curve.b).into();

    CombTable {
        table,
        correction,
        b,
    }
}

// ── Fe-based López-Dahab projective operations ─────────────────────────

/// Projective point in Fe representation (López-Dahab).
/// Shared with Shamir's trick (proj.rs).
#[derive(Clone, Copy)]
pub(crate) struct FeProj {
    pub(crate) x: Fe,
    pub(crate) y: Fe,
    pub(crate) z: Fe,
}

/// LD projective doubling for y²+xy = x³+b (a=0).
///
/// Cost: 4M + 5S.
#[inline]
pub(crate) fn fe_double(p: &FeProj, b: &Fe) -> FeProj {
    let a = p.z.mod_sqr();               // Z₁²
    let a_sq = a.mod_sqr();              // Z₁⁴
    let b_const = b.mod_mul(&a_sq);      // b·Z₁⁴
    let c = p.x.mod_sqr();               // X₁²
    let z3 = a.mod_mul(&c);              // Z₃ = Z₁²·X₁²
    let c_sq = c.mod_sqr();              // X₁⁴
    let x3 = c_sq.add(&b_const);         // X₃ = X₁⁴ + b·Z₁⁴

    let y_sq = p.y.mod_sqr();            // Y₁²
    let y_sq_b = y_sq.add(&b_const);     // Y₁² + b·Z₁⁴
    let term1 = y_sq_b.mod_mul(&x3);     // (Y₁² + b·Z₁⁴)·X₃
    let term2 = z3.mod_mul(&b_const);    // Z₃·b·Z₁⁴
    let y3 = term1.add(&term2);          // Y₃

    FeProj { x: x3, y: y3, z: z3 }
}

/// LD mixed addition: projective (X₁,Y₁,Z₁) + affine (x₂,y₂).
///
/// Precondition: Z₁ ≠ 0 and the points are distinct.
/// Cost: 8M + 5S.
#[inline]
pub(crate) fn fe_madd(p: &FeProj, q: &FeAffine) -> FeProj {
    let z1_sq = p.z.mod_sqr();                   // Z₁²
    let x2_z1 = q.x.mod_mul(&p.z);               // x₂·Z₁
    let b = p.x.add(&x2_z1);                     // B = X₁ + x₂·Z₁
    let y2_z1sq = q.y.mod_mul(&z1_sq);            // y₂·Z₁²
    let a = p.y.add(&y2_z1sq);                    // A = Y₁ + y₂·Z₁²
    let c = b.mod_mul(&p.z);                      // C = B·Z₁
    let z3 = c.mod_sqr();                         // Z₃ = C²
    let d = q.x.mod_mul(&z3);                     // D = x₂·Z₃

    let a_sq = a.mod_sqr();                       // A²
    let b_sq = b.mod_sqr();                       // B²
    let inner = a.add(&b_sq);                     // A + B² (a₂=0)
    let c_inner = c.mod_mul(&inner);              // C·(A + B²)
    let x3 = a_sq.add(&c_inner);                  // X₃ = A² + C·(A + B²)

    let d_x3 = d.add(&x3);                        // D + X₃
    let a_c = a.mod_mul(&c);                       // A·C
    let a_c_z3 = a_c.add(&z3);                    // A·C + Z₃
    let term1 = d_x3.mod_mul(&a_c_z3);            // (D + X₃)·(A·C + Z₃)
    let y2_x2 = q.y.add(&q.x);                    // y₂ + x₂
    let z3_sq = z3.mod_sqr();                      // Z₃²
    let term2 = y2_x2.mod_mul(&z3_sq);            // (y₂ + x₂)·Z₃²
    let y3 = term1.add(&term2);                    // Y₃

    FeProj { x: x3, y: y3, z: z3 }
}

// ── CT primitives ──────────────────────────────────────────────────────

/// Constant-time table lookup: scan all 16 entries, mask-copy the matching
/// one via `subtle::ConditionallySelectable`. The `subtle` crate wraps
/// conditional operations in an opacity barrier, preventing the compiler
/// from optimising masked copies into data-dependent branches.
#[inline]
fn ct_table_lookup(table: &[FeAffine; TABLE_SIZE], index: u32) -> FeAffine {
    let mut result = table[0];
    for i in 1..TABLE_SIZE {
        let choice = subtle::ConstantTimeEq::ct_eq(&(i as u32), &index);
        ct_cmov_fe(&mut result.x, &table[i].x, choice);
        ct_cmov_fe(&mut result.y, &table[i].y, choice);
    }
    result
}

/// Constant-time conditional move via `subtle::ConditionallySelectable`.
/// If `choice` is true, copy `src` into `dst`; otherwise leave unchanged.
#[inline]
fn ct_cmov_fe(dst: &mut Fe, src: &Fe, choice: subtle::Choice) {
    for i in 0..FE_WORDS {
        dst.0[i].conditional_assign(&src.0[i], choice);
    }
}

/// Constant-time conditional select for projective points.
/// Returns `a` when `choice` is true, `b` when false.
#[inline]
fn ct_select_proj(a: &FeProj, b: &FeProj, choice: subtle::Choice) -> FeProj {
    let mut out = *b;
    ct_cmov_fe(&mut out.x, &a.x, choice);
    ct_cmov_fe(&mut out.y, &a.y, choice);
    ct_cmov_fe(&mut out.z, &a.z, choice);
    out
}

// ── Scalar decomposition ───────────────────────────────────────────────

/// Extract the 4-bit comb column value at position `col` from a scalar.
///
/// Bit indices sampled: `col`, `col + D`, `col + 2*D`, `col + 3*D`.
/// Bits beyond `MAX_SCALAR_BITS` (inclusive) are treated as 0.
///
/// ## CT note
///
/// All branches here depend only on `col` and `j` (public loop counters),
/// never on the secret scalar value. The scalar is accessed only via
/// fixed-index word reads and bit shifts.
#[inline]
fn extract_column(scalar: &[u32], col: usize) -> u32 {
    let mut val = 0u32;
    for j in 0..W {
        let bit_idx = col + j * D;
        let bit = if bit_idx <= MAX_SCALAR_BITS {
            let wi = bit_idx / 32;
            let bi = bit_idx % 32;
            if wi < scalar.len() {
                (scalar[wi] >> bi) & 1
            } else {
                0
            }
        } else {
            0
        };
        val |= bit << j;
    }
    val
}

// ── Public API ─────────────────────────────────────────────────────────

/// Compute `x(k·G)` using the precomputed comb table. Constant-time.
///
/// Returns `None` when `k·G = O` (the point at infinity).
///
/// First call initializes the comb table (~3 ms one-time cost).
/// Subsequent calls use the cached table and cost ~165 µs.
pub fn mul_base_comb_x_ct(k: &FieldEl, curve: &Curve) -> Option<FieldEl> {
    let ct = COMB_TABLE.get_or_init(|| build_table(curve));

    // Normalize scalar to FE_WORDS u32 limbs
    let mut k_words = [0u32; FE_WORDS];
    let copy_len = k.bytes.len().min(FE_WORDS);
    k_words[..copy_len].copy_from_slice(&k.bytes[..copy_len]);

    // Initialize accumulator at the blinding point G (Z=1, never identity).
    let g_aff = &ct.table[1]; // T[1] = G
    let mut acc = FeProj {
        x: g_aff.x,
        y: g_aff.y,
        z: Fe::ONE,
    };

    // Main comb loop: d-1 downto 0 (65 iterations).
    // Each iteration: double, extract column, CT lookup, conditional add.
    for i in (0..D).rev() {
        // Always double (first iteration doubles G → 2G, fine).
        acc = fe_double(&acc, &ct.b);

        // Extract the 4-bit column value
        let col_val = extract_column(&k_words, i);

        // CT table lookup
        let table_point = ct_table_lookup(&ct.table, col_val);

        // Always compute the addition
        let acc_added = fe_madd(&acc, &table_point);

        // CT select: use acc_added when col_val != 0, keep acc when col_val == 0.
        // `subtle::ConstantTimeEq` with compiler opacity barrier prevents the
        // optimizer from converting this into a data-dependent branch.
        let col_nonzero = !subtle::ConstantTimeEq::ct_eq(&col_val, &0u32);
        acc = ct_select_proj(&acc_added, &acc, col_nonzero);
    }

    // Subtract the blinding term: acc currently holds (2^D + k)·G.
    // Add the correction point -(2^D · G) to get k·G.
    acc = fe_madd(&acc, &ct.correction);

    // Convert to affine x = X/Z
    if acc.z.ct_is_zero() {
        return None;
    }
    let z_inv = acc.z.invert();
    let x_affine = acc.x.mod_mul(&z_inv);

    // Wipe secret-derived intermediates from the stack.
    k_words.zeroize();

    Some((&x_affine).into())
}

/// Force-initialize the comb table. Call at process start to avoid
/// the ~3 ms startup cost on the first sign call.
///
/// Called by [`crate::core::backend::warm_up`] and the Python `warm_up()`.
pub fn warm_up(curve: &Curve) {
    let _ = COMB_TABLE.get_or_init(|| build_table(curve));
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_column_smoke() {
        // Scalar = 0b1010 in word 0 → bit 0=0, bit 1=1, bit 2=0, bit 3=1
        let scalar = [0b1010u32, 0, 0, 0, 0, 0, 0, 0, 0];
        // Column 0: bits 0, 65, 130, 195 → only bit 0 = 0
        assert_eq!(extract_column(&scalar, 0), 0);
        // Column 1: bits 1, 66, 131, 196 → bit 1 = 1
        assert_eq!(extract_column(&scalar, 1), 1);
        // Column 3: bits 3, 68, 133, 198 → bit 3 = 1
        assert_eq!(extract_column(&scalar, 3), 1);
    }

    /// Comb matches Montgomery ladder for small scalars.
    #[test]
    fn comb_matches_ladder_small() {
        let curve = Curve::dstu_pb_257();
        for k_val in 1u32..=50 {
            let mut w = vec![0u32; curve.mod_words];
            w[0] = k_val;
            let k = FieldEl::from_words(w);

            let via_ladder = crate::core::mladder::mul_base_x_ct(&k, &curve);
            let via_comb = mul_base_comb_x_ct(&k, &curve);

            match (via_ladder, via_comb) {
                (None, None) => {}
                (Some(ref lx), Some(ref cx)) => {
                    assert!(
                        lx.equals(cx),
                        "comb vs ladder mismatch at k={}: ladder={:?}, comb={:?}",
                        k_val, lx.bytes, cx.bytes,
                    );
                }
                _ => panic!(
                    "comb vs ladder None/Some mismatch at k={}",
                    k_val,
                ),
            }
        }
    }

    /// Comb matches ladder for full 256-bit scalars.
    #[test]
    fn comb_matches_ladder_full_scalars() {
        let curve = Curve::dstu_pb_257();
        let mut x: u32 = 0xC0FE_BABE;
        for trial in 0..100u32 {
            let mut words = vec![0u32; curve.mod_words];
            for w in words.iter_mut().take(8) {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *w = x;
            }
            let k = FieldEl::from_words(words);

            let via_ladder = crate::core::mladder::mul_base_x_ct(&k, &curve);
            let via_comb = mul_base_comb_x_ct(&k, &curve);

            match (via_ladder, via_comb) {
                (None, None) => {}
                (Some(ref lx), Some(ref cx)) => {
                    assert!(
                        lx.equals(cx),
                        "trial {}: comb vs ladder mismatch",
                        trial,
                    );
                }
                _ => panic!("trial {}: None/Some mismatch", trial),
            }
        }
    }

    /// Order * G = identity via comb.
    #[test]
    fn comb_order_is_identity() {
        let curve = Curve::dstu_pb_257();
        let result = mul_base_comb_x_ct(&curve.order, &curve);
        assert!(result.is_none(), "order·G via comb must be identity");
    }

    /// k=1 returns G.x.
    #[test]
    fn comb_one_returns_base() {
        let curve = Curve::dstu_pb_257();
        let one = FieldEl::one(curve.mod_words);
        let x = mul_base_comb_x_ct(&one, &curve).expect("1·G is not identity");
        assert!(x.equals(&curve.base_x), "1·G must equal G");
    }

    /// k=0 returns None (identity / point at infinity).
    #[test]
    fn comb_zero_returns_none() {
        let curve = Curve::dstu_pb_257();
        let zero = FieldEl::zero(curve.mod_words);
        let result = mul_base_comb_x_ct(&zero, &curve);
        assert!(result.is_none(), "0·G via comb must be identity (None)");
    }
}
