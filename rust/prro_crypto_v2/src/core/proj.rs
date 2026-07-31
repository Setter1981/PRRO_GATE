//! Lopez-Dahab projective coordinates for binary curves y² + xy = x³ + b.
//!
//! Eliminates per-operation field inversions during scalar multiplication.
//! With Z=1 affine input → projective doubling/addition → single inversion at
//! the end, scalar mul cost drops from O(n × invert) to O(n × mul) — roughly
//! 5-10x speedup.
//!
//! ## Coordinate relation
//! Projective (X, Y, Z) ↔ affine (X/Z, Y/Z²).
//! Point at infinity: Z = 0.
//!
//! ## Formulas (a2=0 case, suitable for DSTU_PB_257)
//! Source: Doche–Lange 2005 ("dbl-2005-dl" + "madd-2005-dl") via
//! Bernstein–Lange Explicit-Formulas Database (hyperelliptic.org/EFD).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::core::comb::{fe_double, fe_madd, FeAffine, FeProj};
use crate::core::curve::Curve;
use crate::core::fe::Fe;
use crate::core::field::FieldEl;
use crate::core::point::Point;

/// Window width for wNAF in Shamir / single-scalar Fe mul.
/// w=6: 16 precomp points per base, ~37 non-zero digits per scalar.
/// vs w=4: 4 points, ~51 digits. Cached tables amortize precompute cost.
const SHAMIR_W: usize = 6;
/// Number of odd precomputed multiples per point: 2^(w-2).
const SHAMIR_TABLE_SIZE: usize = 1 << (SHAMIR_W - 2); // 16

// ── Cached G precompute (lazily built, process-lifetime) ───────────────

/// Precomputed FeAffine odd multiples of G: [G, 3G, 5G, 7G].
/// Avoids ~40µs of FieldEl affine additions per Shamir/verify call.
static G_TABLE: OnceLock<[FeAffine; SHAMIR_TABLE_SIZE]> = OnceLock::new();

fn get_g_table(curve: &Curve) -> &'static [FeAffine; SHAMIR_TABLE_SIZE] {
    G_TABLE.get_or_init(|| {
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let aff = precompute_odd_affine(&g, curve, SHAMIR_TABLE_SIZE);
        let mut table = [FeAffine {
            x: Fe::ZERO,
            y: Fe::ZERO,
        }; SHAMIR_TABLE_SIZE];
        for (i, p) in aff.iter().enumerate() {
            table[i] = FeAffine {
                x: (&p.x).into(),
                y: (&p.y).into(),
            };
        }
        table
    })
}

// ── Cached Q precompute (per compressed pubkey) ────────────────────────

use std::sync::atomic::{AtomicUsize, Ordering};

/// Cap for Q-precompute cache. Synced with pubkey validation cache
/// via [`crate::core::sign::set_verify_cache_capacity`].
static Q_CACHE_CAP: AtomicUsize = AtomicUsize::new(512);

/// Called by [`crate::core::sign::set_verify_cache_capacity`].
pub(crate) fn set_q_cache_capacity(cap: usize) {
    Q_CACHE_CAP.store(cap, Ordering::Relaxed);
}

/// Precomputed FeAffine odd multiples per public key.
/// Key = compressed pubkey 33 bytes.
static Q_CACHE: Mutex<Option<HashMap<[u8; 33], [FeAffine; SHAMIR_TABLE_SIZE]>>> = Mutex::new(None);

fn get_q_table(pub_q: &Point, curve: &Curve) -> [FeAffine; SHAMIR_TABLE_SIZE] {
    let compressed = crate::core::point::compress_point(pub_q, curve);
    let mut key = [0u8; 33];
    let len = compressed.len().min(33);
    key[..len].copy_from_slice(&compressed[..len]);

    // Check cache
    {
        let guard = Q_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref map) = *guard {
            if let Some(table) = map.get(&key) {
                return *table;
            }
        }
    }

    // Build table
    let aff = precompute_odd_affine(pub_q, curve, SHAMIR_TABLE_SIZE);
    let mut table = [FeAffine {
        x: Fe::ZERO,
        y: Fe::ZERO,
    }; SHAMIR_TABLE_SIZE];
    for (i, p) in aff.iter().enumerate() {
        table[i] = FeAffine {
            x: (&p.x).into(),
            y: (&p.y).into(),
        };
    }

    // Store (with cap to prevent unbounded growth)
    {
        let mut guard = Q_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let map = guard.get_or_insert_with(HashMap::new);
        if map.len() >= Q_CACHE_CAP.load(Ordering::Relaxed) {
            map.clear();
        }
        map.insert(key, table);
    }
    table
}

/// Lopez-Dahab projective point.
#[derive(Clone, Debug)]
pub struct ProjPoint {
    pub x: FieldEl,
    pub y: FieldEl,
    pub z: FieldEl,
}

impl ProjPoint {
    /// Identity (point at infinity): Z = 0.
    pub fn zero(mod_words: usize) -> Self {
        Self {
            x: FieldEl::one(mod_words),
            y: FieldEl::one(mod_words),
            z: FieldEl::zero(mod_words),
        }
    }

    /// Convert affine point to projective with Z = 1.
    pub fn from_affine(p: &Point, mod_words: usize) -> Self {
        if p.is_zero() {
            return Self::zero(mod_words);
        }
        Self {
            x: p.x.clone(),
            y: p.y.clone(),
            z: FieldEl::one(mod_words),
        }
    }

    /// Test if at infinity (Z == 0).
    pub fn is_zero(&self) -> bool {
        self.z.is_zero()
    }

    /// Convert to affine. Costs one field inversion + 2 mul + 1 sqr.
    pub fn to_affine(&self, curve: &Curve) -> Point {
        if self.is_zero() {
            return Point::zero(curve.mod_words);
        }
        let z_inv = self.z.invert(&curve.p_exp, &curve.p_words, curve.mod_words);
        let z_inv_sq = z_inv.mod_sqr(&curve.p_exp, curve.mod_words);
        let x_aff = self.x.mod_mul(&z_inv, &curve.p_exp, curve.mod_words);
        let y_aff = self.y.mod_mul(&z_inv_sq, &curve.p_exp, curve.mod_words);
        Point::new(x_aff, y_aff)
    }

    /// Doubling 2P in Lopez-Dahab projective for curve y²+xy=x³+b (a2=0).
    /// Algorithm: dbl-2005-dl, cost = 3M + 5S + 1·b.
    ///
    /// Steps:
    ///   A  = Z1²
    ///   B  = b·A²
    ///   C  = X1²
    ///   Z3 = A·C
    ///   X3 = C² + B
    ///   Y3 = (Y1² + B)·X3 + Z3·B    (a2 = 0 zeroes the a2·Z3 term)
    pub fn double(&self, curve: &Curve) -> ProjPoint {
        if self.is_zero() {
            return self.clone();
        }
        let mw = curve.mod_words;
        let p_exp = &curve.p_exp;
        let b = &curve.b;

        let a = self.z.mod_sqr(p_exp, mw); // Z1²
        let a_sq = a.mod_sqr(p_exp, mw); // Z1^4
        let b_const = b.mod_mul(&a_sq, p_exp, mw); // b·Z1^4
        let c = self.x.mod_sqr(p_exp, mw); // X1²
        let z3 = a.mod_mul(&c, p_exp, mw); // Z1²·X1²
        let c_sq = c.mod_sqr(p_exp, mw); // X1^4
        let x3 = c_sq.add(&b_const); // X1^4 + b·Z1^4

        // Y3 = (Y1² + B)·X3 + Z3·B
        let y_sq = self.y.mod_sqr(p_exp, mw); // Y1²
        let y_sq_plus_b = y_sq.add(&b_const); // Y1² + b·Z1^4
        let term1 = y_sq_plus_b.mod_mul(&x3, p_exp, mw);
        let term2 = z3.mod_mul(&b_const, p_exp, mw);
        let y3 = term1.add(&term2);

        ProjPoint {
            x: x3,
            y: y3,
            z: z3,
        }
    }

    /// Mixed addition: self (projective) + other (affine, Z=1).
    /// Algorithm: madd-2005-dl, cost = 8M + 5S (a2 = 0 case).
    ///
    /// Returns self + other in projective form.
    /// Caller must handle the special cases self.is_zero() and self == other
    /// (which would require the doubling formula).
    ///
    /// Steps (X1, Y1, Z1) projective + (X2, Y2) affine:
    ///   A  = Y1 + Y2·Z1²
    ///   B  = X1 + X2·Z1
    ///   C  = B·Z1
    ///   Z3 = C²
    ///   D  = X2·Z3
    ///   X3 = A² + C·(A + B²)         (a2 = 0)
    ///   Y3 = (D + X3)·(A·C + Z3) + (Y2 + X2)·Z3²
    pub fn madd_affine(&self, other: &Point, curve: &Curve) -> ProjPoint {
        // Handle identity cases
        if self.is_zero() {
            if other.is_zero() {
                return self.clone();
            }
            return ProjPoint::from_affine(other, curve.mod_words);
        }
        if other.is_zero() {
            return self.clone();
        }

        let mw = curve.mod_words;
        let p_exp = &curve.p_exp;

        // Detect self == ±other on the affine equivalents — must fall back to
        // doubling (or return identity for self == -other) to avoid division
        // by zero when B becomes zero.
        // Affine x of self = X1/Z1. Compare X1 == X2·Z1 (i.e. B == 0).
        let z1_sq = self.z.mod_sqr(p_exp, mw);
        let x2_z1 = other.x.mod_mul(&self.z, p_exp, mw);
        let b = self.x.add(&x2_z1); // B = X1 + X2·Z1
        let y2_z1_sq = other.y.mod_mul(&z1_sq, p_exp, mw);
        let a = self.y.add(&y2_z1_sq); // A = Y1 + Y2·Z1²

        if b.is_zero_ct() {
            // self.x_aff == other.x. Either same point (double) or negation (infinity).
            if a.is_zero_ct() {
                // Same point: use doubling.
                return self.double(curve);
            } else {
                // Negation: result is infinity.
                return ProjPoint::zero(mw);
            }
        }

        let c = b.mod_mul(&self.z, p_exp, mw); // C = B·Z1
        let z3 = c.mod_sqr(p_exp, mw); // Z3 = C²
        let d = other.x.mod_mul(&z3, p_exp, mw); // D = X2·Z3

        // X3 = A² + C·(A + B²)
        let a_sq = a.mod_sqr(p_exp, mw);
        let b_sq = b.mod_sqr(p_exp, mw);
        let inner = a.add(&b_sq); // A + B²  (a2=0 so omit a2·C term)
        let c_times_inner = c.mod_mul(&inner, p_exp, mw);
        let x3 = a_sq.add(&c_times_inner);

        // Y3 = (D + X3)·(A·C + Z3) + (Y2 + X2)·Z3²
        let d_plus_x3 = d.add(&x3);
        let a_c = a.mod_mul(&c, p_exp, mw);
        let a_c_plus_z3 = a_c.add(&z3);
        let term1 = d_plus_x3.mod_mul(&a_c_plus_z3, p_exp, mw);

        let y2_plus_x2 = other.y.add(&other.x);
        let z3_sq = z3.mod_sqr(p_exp, mw);
        let term2 = y2_plus_x2.mod_mul(&z3_sq, p_exp, mw);

        let y3 = term1.add(&term2);

        ProjPoint {
            x: x3,
            y: y3,
            z: z3,
        }
    }
}

/// Precompute affine odd multiples of P: P, 3P, 5P, ..., (2^(w-1)-1)P.
///
/// Shared by `mul_proj_wnaf` (single scalar) and `mul_shamir_wnaf` (dual scalar).
fn precompute_odd_affine(p: &Point, curve: &Curve, count: usize) -> Vec<Point> {
    let mut out = Vec::with_capacity(count);
    out.push(p.clone());
    if count > 1 {
        let two_p = p.twice(curve);
        let mut acc = p.clone();
        for _ in 1..count {
            acc = acc.add(&two_p, curve);
            out.push(acc.clone());
        }
    }
    out
}

/// Scalar multiplication k · P using projective coordinates internally.
/// Input/output are affine. wNAF window 4 is used for digit reduction.
pub fn mul_proj_wnaf(point: &Point, scalar: &FieldEl, curve: &Curve) -> Point {
    if scalar.is_zero_ct() || point.is_zero() {
        return Point::zero(curve.mod_words);
    }

    let width = SHAMIR_W;
    let naf = crate::core::wnaf::compute_wnaf(&scalar.bytes, width);
    if naf.is_empty() {
        return Point::zero(curve.mod_words);
    }

    let count = 1usize << (width - 2);
    let precomp_affine = precompute_odd_affine(point, curve, count);

    // Process digits MSB-to-LSB in projective.
    let mut r = ProjPoint::zero(curve.mod_words);
    for &digit in naf.iter().rev() {
        if !r.is_zero() {
            r = r.double(curve);
        }
        if digit != 0 {
            let abs = digit.unsigned_abs() as usize;
            let idx = (abs - 1) / 2;
            let p_aff = if digit > 0 {
                precomp_affine[idx].clone()
            } else {
                precomp_affine[idx].negate()
            };
            r = r.madd_affine(&p_aff, curve);
        }
    }

    r.to_affine(curve)
}

/// Fe-based wNAF scalar multiplication k·P for arbitrary point P.
///
/// Same algorithm as `mul_proj_wnaf` but uses stack-allocated `Fe` for
/// the main loop (~2× faster than the FieldEl-based version). Precompute
/// still uses FieldEl affine (small one-time cost), then converts to Fe.
///
/// Used by pubkey validation (`n·Q == O` check) where both k and P are
/// public, so variable-time is fine.
pub fn mul_proj_wnaf_fe(point: &Point, scalar: &FieldEl, curve: &Curve) -> Point {
    if scalar.is_zero_ct() || point.is_zero() {
        return Point::zero(curve.mod_words);
    }

    let width = SHAMIR_W;
    let naf = crate::core::wnaf::compute_wnaf(&scalar.bytes, width);
    if naf.is_empty() {
        return Point::zero(curve.mod_words);
    }

    // Precompute odd multiples as FieldEl, convert to Fe
    let count = 1usize << (width - 2);
    let precomp_aff = precompute_odd_affine(point, curve, count);
    let fe_table: Vec<FeAffine> = precomp_aff
        .iter()
        .map(|p| FeAffine {
            x: (&p.x).into(),
            y: (&p.y).into(),
        })
        .collect();

    let b_fe: Fe = (&curve.b).into();
    let mut started = false;
    let mut acc = FeProj {
        x: Fe::ONE,
        y: Fe::ONE,
        z: Fe::ZERO,
    };

    for &digit in naf.iter().rev() {
        if started {
            acc = fe_double(&acc, &b_fe);
        }
        if digit != 0 {
            let abs = digit.unsigned_abs() as usize;
            let idx = (abs - 1) / 2;
            let p = if digit > 0 {
                fe_table[idx]
            } else {
                FeAffine {
                    x: fe_table[idx].x,
                    y: fe_table[idx].x.add(&fe_table[idx].y),
                }
            };
            if !started {
                acc = FeProj {
                    x: p.x,
                    y: p.y,
                    z: Fe::ONE,
                };
                started = true;
            } else {
                // madd_affine with doubling/negation handling (VT, public scalars)
                let x2_z1 = p.x.mod_mul(&acc.z);
                if acc.x.ct_eq(&x2_z1) {
                    let z1_sq = acc.z.mod_sqr();
                    let y2_z1sq = p.y.mod_mul(&z1_sq);
                    if acc.y.ct_eq(&y2_z1sq) {
                        acc = fe_double(&acc, &b_fe);
                    } else {
                        acc = FeProj {
                            x: Fe::ONE,
                            y: Fe::ONE,
                            z: Fe::ZERO,
                        };
                        started = false;
                    }
                } else {
                    acc = fe_madd(&acc, &p);
                }
            }
        }
    }

    if acc.z.ct_is_zero() {
        return Point::zero(curve.mod_words);
    }
    let z_inv = acc.z.invert();
    let z_inv_sq = z_inv.mod_sqr();
    let x_aff = acc.x.mod_mul(&z_inv);
    let y_aff = acc.y.mod_mul(&z_inv_sq);
    Point::new((&x_aff).into(), (&y_aff).into())
}

/// Shamir's trick: compute `s*G + r*Q` in a single wNAF pass.
///
/// Instead of two independent scalar multiplications (2 × 257 doublings),
/// we interleave the wNAF digits of both scalars and scan them in a single
/// MSB→LSB sweep. This halves the doublings to 257 and merges the additions,
/// yielding ~40% speedup on verify.
///
/// Both `s` and `r` are public signature components, so variable-time
/// operations (wNAF, data-dependent branches) are safe here.
///
/// Algorithm (Straus–Shamir joint wNAF):
///   1. Compute wNAF(s, w) and wNAF(r, w)
///   2. Precompute odd multiples of G: {1,3,5,7}G and Q: {1,3,5,7}Q
///   3. Scan from max(len(naf_s), len(naf_r)) down to 0:
///      - Double accumulator
///      - If s-digit nonzero: add corresponding i*G
///      - If r-digit nonzero: add corresponding j*Q
///   4. Convert to affine
pub fn mul_shamir_wnaf(
    s: &FieldEl,
    _base_g: &Point, // G is taken from cached precompute table
    r: &FieldEl,
    pub_q: &Point,
    curve: &Curve,
) -> Point {
    if s.is_zero_ct() && r.is_zero_ct() {
        return Point::zero(curve.mod_words);
    }

    let width = SHAMIR_W;
    let naf_s = crate::core::wnaf::compute_wnaf(&s.bytes, width);
    let naf_r = crate::core::wnaf::compute_wnaf(&r.bytes, width);

    let max_len = naf_s.len().max(naf_r.len());
    if max_len == 0 {
        return Point::zero(curve.mod_words);
    }

    // Cached precomputed odd multiples — G is process-lifetime,
    // Q is per-pubkey. Eliminates ~80µs of FieldEl affine additions.
    let fe_g = get_g_table(curve);
    let fe_q = get_q_table(pub_q, curve);

    let negate_fe = |p: &FeAffine| -> FeAffine {
        FeAffine {
            x: p.x,
            y: p.x.add(&p.y),
        }
    };

    let b_fe: Fe = (&curve.b).into();
    let mut started = false;
    let mut acc = FeProj {
        x: Fe::ONE,
        y: Fe::ONE,
        z: Fe::ZERO,
    };

    // Safe mixed-addition: handles doubling (same x) and negation cases
    // that fe_madd alone does not. Both scalars are public, so VT is OK.
    let fe_madd_safe = |acc: &FeProj, q: &FeAffine, started: &mut bool| -> FeProj {
        if !*started {
            *started = true;
            return FeProj {
                x: q.x,
                y: q.y,
                z: Fe::ONE,
            };
        }
        // Check B = X₁ + x₂·Z₁ — zero means same affine x (double or negate)
        let x2_z1 = q.x.mod_mul(&acc.z);
        if acc.x.ct_eq(&x2_z1) {
            let z1_sq = acc.z.mod_sqr();
            let y2_z1sq = q.y.mod_mul(&z1_sq);
            if acc.y.ct_eq(&y2_z1sq) {
                // Same point → double
                return fe_double(acc, &b_fe);
            }
            // Negation → identity
            *started = false;
            return FeProj {
                x: Fe::ONE,
                y: Fe::ONE,
                z: Fe::ZERO,
            };
        }
        fe_madd(acc, q)
    };

    for i in (0..max_len).rev() {
        if started {
            acc = fe_double(&acc, &b_fe);
        }

        let ds = if i < naf_s.len() { naf_s[i] } else { 0 };
        let dr = if i < naf_r.len() { naf_r[i] } else { 0 };

        if ds != 0 {
            let abs = ds.unsigned_abs() as usize;
            let idx = (abs - 1) / 2;
            let p = if ds > 0 {
                fe_g[idx]
            } else {
                negate_fe(&fe_g[idx])
            };
            acc = fe_madd_safe(&acc, &p, &mut started);
        }

        if dr != 0 {
            let abs = dr.unsigned_abs() as usize;
            let idx = (abs - 1) / 2;
            let p = if dr > 0 {
                fe_q[idx]
            } else {
                negate_fe(&fe_q[idx])
            };
            acc = fe_madd_safe(&acc, &p, &mut started);
        }
    }

    // Convert Fe projective → affine → Point
    if acc.z.ct_is_zero() {
        return Point::zero(curve.mod_words);
    }
    let z_inv = acc.z.invert();
    let z_inv_sq = z_inv.mod_sqr();
    let x_aff = acc.x.mod_mul(&z_inv);
    let y_aff = acc.y.mod_mul(&z_inv_sq);
    Point::new((&x_aff).into(), (&y_aff).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proj_zero() {
        let curve = Curve::dstu_pb_257();
        let z = ProjPoint::zero(curve.mod_words);
        assert!(z.is_zero());
        let aff = z.to_affine(&curve);
        assert!(aff.is_zero());
    }

    #[test]
    fn test_proj_roundtrip_base() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let g_proj = ProjPoint::from_affine(&g, curve.mod_words);
        let back = g_proj.to_affine(&curve);
        assert!(g.equals(&back));
    }

    #[test]
    fn test_double_matches_affine() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

        let aff_double = g.twice(&curve);
        let proj_double = ProjPoint::from_affine(&g, curve.mod_words)
            .double(&curve)
            .to_affine(&curve);

        assert!(
            aff_double.equals(&proj_double),
            "projective doubling != affine doubling\n  aff:  x={:x?}\n  proj: x={:x?}",
            aff_double.x.bytes,
            proj_double.x.bytes
        );
    }

    #[test]
    fn test_double_chain_matches_affine() {
        // 2G, 4G, 8G via projective vs affine
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

        let mut aff = g.twice(&curve);
        let mut proj = ProjPoint::from_affine(&g, curve.mod_words).double(&curve);

        for i in 1..=4 {
            aff = aff.twice(&curve);
            proj = proj.double(&curve);
            let proj_aff = proj.to_affine(&curve);
            assert!(
                aff.equals(&proj_aff),
                "chain doubling mismatch at 2^{}G",
                i + 1
            );
        }
    }

    #[test]
    fn test_madd_matches_affine() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let g2_aff = g.twice(&curve); // 2G affine
        let g3_aff = g.add(&g2_aff, &curve); // 3G affine

        // 2G (projective) + G (affine) = 3G
        let g2_proj = ProjPoint::from_affine(&g, curve.mod_words).double(&curve);
        let g3_proj = g2_proj.madd_affine(&g, &curve);
        let g3_back = g3_proj.to_affine(&curve);

        assert!(
            g3_aff.equals(&g3_back),
            "2G + G via projective != 3G affine"
        );
    }

    #[test]
    fn test_mul_proj_matches_naive_small() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        for k in 1u32..=20 {
            let mut words = vec![0u32; curve.mod_words];
            words[0] = k;
            let scalar = FieldEl::from_words(words);
            let naive = g.mul_naive(&scalar, &curve);
            let via_proj = mul_proj_wnaf(&g, &scalar, &curve);
            assert!(naive.equals(&via_proj), "mul_proj mismatch at k={}", k);
        }
    }

    #[test]
    fn test_mul_proj_matches_naive_full_scalar() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let mut words = vec![0u32; curve.mod_words];
        for i in 0..8 {
            words[i] = (0xCAFE_BABEu32).wrapping_mul((i as u32) + 1);
        }
        let scalar = FieldEl::from_words(words);

        let naive = g.mul_naive(&scalar, &curve);
        let via_proj = mul_proj_wnaf(&g, &scalar, &curve);
        assert!(
            naive.equals(&via_proj),
            "mul_proj != naive on 256-bit scalar"
        );
    }

    #[test]
    fn test_mul_proj_order_g_is_infinity() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let result = mul_proj_wnaf(&g, &curve.order, &curve);
        assert!(result.is_zero(), "order * G should be infinity");
    }

    /// Shamir's trick must produce the same result as two separate scalar muls.
    #[test]
    fn test_shamir_matches_separate_muls_small() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        // Q = some known point (3G for simplicity)
        let q = {
            let mut w = vec![0u32; curve.mod_words];
            w[0] = 3;
            g.mul(&FieldEl::from_words(w), &curve)
        };

        for s_val in 1u32..=10 {
            for r_val in 1u32..=10 {
                let mut s_w = vec![0u32; curve.mod_words];
                s_w[0] = s_val;
                let s = FieldEl::from_words(s_w);

                let mut r_w = vec![0u32; curve.mod_words];
                r_w[0] = r_val;
                let r = FieldEl::from_words(r_w);

                // Reference: separate muls + add
                let sg = g.mul(&s, &curve);
                let rq = q.mul(&r, &curve);
                let expected = sg.add(&rq, &curve);

                // Shamir's trick
                let got = mul_shamir_wnaf(&s, &g, &r, &q, &curve);

                assert!(
                    expected.equals(&got),
                    "Shamir mismatch at s={} r={}",
                    s_val,
                    r_val,
                );
            }
        }
    }

    /// Shamir with full 256-bit scalars.
    #[test]
    fn test_shamir_full_scalars() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());

        // Q = d*G for some d
        let d = FieldEl::from_hex("DEADBEEFCAFE12345678", curve.mod_words);
        let q = g.mul(&d, &curve);

        let s = FieldEl::from_hex(
            "7FFFFFFFFFFFFFFF0000000000000000AAAA5555AAAA5555DEADBEEFCAFEBABE",
            curve.mod_words,
        );
        let r = FieldEl::from_hex(
            "123456789ABCDEF0FEDCBA98765432100011223344556677AABBCCDDEEFF0011",
            curve.mod_words,
        );

        let sg = g.mul(&s, &curve);
        let rq = q.mul(&r, &curve);
        let expected = sg.add(&rq, &curve);

        let got = mul_shamir_wnaf(&s, &g, &r, &q, &curve);

        assert!(
            expected.equals(&got),
            "Shamir mismatch on full 256-bit scalars",
        );
    }

    /// Edge: Shamir with one zero scalar.
    #[test]
    fn test_shamir_one_zero() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let q = g.twice(&curve);

        let zero = FieldEl::zero(curve.mod_words);
        let mut s_w = vec![0u32; curve.mod_words];
        s_w[0] = 5;
        let s = FieldEl::from_words(s_w);

        // 0*G + 5*Q = 5*Q
        let shamir = mul_shamir_wnaf(&zero, &g, &s, &q, &curve);
        let expected = q.mul(&s, &curve);
        assert!(expected.equals(&shamir), "Shamir(0,s) != s*Q");

        // 5*G + 0*Q = 5*G
        let shamir2 = mul_shamir_wnaf(&s, &g, &zero, &q, &curve);
        let expected2 = g.mul(&s, &curve);
        assert!(expected2.equals(&shamir2), "Shamir(s,0) != s*G");
    }
}
