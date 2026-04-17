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

use crate::core::curve::Curve;
use crate::core::field::FieldEl;
use crate::core::point::Point;

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

/// Scalar multiplication k · P using projective coordinates internally.
/// Input/output are affine. wNAF window 4 is used for digit reduction.
pub fn mul_proj_wnaf(point: &Point, scalar: &FieldEl, curve: &Curve) -> Point {
    if scalar.is_zero_ct() || point.is_zero() {
        return Point::zero(curve.mod_words);
    }

    let width = crate::core::wnaf::DEFAULT_WINDOW;
    let naf = crate::core::wnaf::compute_wnaf(&scalar.bytes, width);
    if naf.is_empty() {
        return Point::zero(curve.mod_words);
    }

    // Precompute affine odd multiples of P: P, 3P, 5P, ..., (2^(w-1)-1)P.
    // We build them from affine P + (2P) using affine ops once, then keep affine.
    let count = 1usize << (width - 2);
    let mut precomp_affine: Vec<Point> = Vec::with_capacity(count);
    precomp_affine.push(point.clone());
    if count > 1 {
        let two_p = point.twice(curve);
        let mut acc = point.clone();
        for _ in 1..count {
            acc = acc.add(&two_p, curve);
            precomp_affine.push(acc.clone());
        }
    }

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

        assert!(g3_aff.equals(&g3_back), "2G + G via projective != 3G affine");
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
            assert!(
                naive.equals(&via_proj),
                "mul_proj mismatch at k={}",
                k
            );
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
        assert!(naive.equals(&via_proj), "mul_proj != naive on 256-bit scalar");
    }

    #[test]
    fn test_mul_proj_order_g_is_infinity() {
        let curve = Curve::dstu_pb_257();
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let result = mul_proj_wnaf(&g, &curve.order, &curve);
        assert!(result.is_zero(), "order * G should be infinity");
    }
}
