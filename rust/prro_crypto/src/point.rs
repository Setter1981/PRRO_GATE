//! Elliptic curve points over GF(2^m) for DSTU 4145.
//!
//! Port of jkurwa/lib/point.js, but without wNAF optimization (uses simple
//! double-and-add scalar multiplication). wNAF can be added later if benchmarks
//! demand it; for fiscal signing throughput, double-and-add is sufficient.

use crate::curve::Curve;
use crate::field::FieldEl;

/// Affine point on a DSTU binary curve. Point at infinity is represented
/// as `(0, 0)` (matches jkurwa's convention; the mathematical "infinity" is
/// not on the curve since (0,0) doesn't satisfy y^2 + xy = x^3 + b for b != 0).
#[derive(Clone, Debug)]
pub struct Point {
    pub x: FieldEl,
    pub y: FieldEl,
}

impl Point {
    /// Construct from x, y coordinates.
    pub fn new(x: FieldEl, y: FieldEl) -> Self {
        Self { x, y }
    }

    /// Point at infinity (zero element of the group).
    pub fn zero(mod_words: usize) -> Self {
        Self {
            x: FieldEl::zero(mod_words),
            y: FieldEl::zero(mod_words),
        }
    }

    /// Test if this is the point at infinity.
    pub fn is_zero(&self) -> bool {
        self.x.is_zero() && self.y.is_zero()
    }

    /// Equality (affine).
    pub fn equals(&self, other: &Point) -> bool {
        self.x.equals(&other.x) && self.y.equals(&other.y)
    }

    /// Negate point: -P = (x, x+y) on this curve form.
    pub fn negate(&self) -> Point {
        Point {
            x: self.x.clone(),
            y: self.x.add(&self.y),
        }
    }

    /// Add two points using affine formulas. Mirrors jkurwa point.js:Point.add.
    pub fn add(&self, other: &Point, curve: &Curve) -> Point {
        if self.is_zero() {
            return other.clone();
        }
        if other.is_zero() {
            return self.clone();
        }

        let a = &curve.a;
        let p_exp = &curve.p_exp;
        let p_words = &curve.p_words;
        let mw = curve.mod_words;

        let x0 = &self.x;
        let y0 = &self.y;
        let x1 = &other.x;
        let y1 = &other.y;

        let lbd: FieldEl;
        let x2: FieldEl;

        if !x0.equals(x1) {
            // Standard addition: lambda = (y0 + y1) / (x0 + x1)
            let tmp = y0.add(y1);
            let tmp2 = x0.add(x1);
            let inv_tmp2 = tmp2.invert(p_exp, p_words, mw);
            lbd = tmp.mod_mul(&inv_tmp2, p_exp, mw);
            // x2 = a + lambda^2 + lambda + x0 + x1
            let lbd_sq = lbd.mod_sqr(p_exp, mw);
            let mut x2_acc = a.add(&lbd_sq);
            x2_acc.add_assign(&lbd);
            x2_acc.add_assign(x0);
            x2_acc.add_assign(x1);
            x2 = x2_acc;
        } else {
            // x0 == x1. Either same point (doubling) or -P (returns infinity).
            if !y1.equals(y0) {
                return Point::zero(mw);
            }
            if x1.is_zero() {
                return Point::zero(mw);
            }

            // Doubling: lambda = x1 + y1/x1
            let inv_x1 = x1.invert(p_exp, p_words, mw);
            let y_over_x = y1.mod_mul(&inv_x1, p_exp, mw);
            lbd = x1.add(&y_over_x);
            // x2 = lambda^2 + lambda + a
            let lbd_sq = lbd.mod_sqr(p_exp, mw);
            let mut x2_acc = lbd_sq.add(a);
            x2_acc.add_assign(&lbd);
            x2 = x2_acc;
        }

        // y2 = lambda * (x1 + x2) + x2 + y1
        let mut y2 = x1.add(&x2);
        y2 = lbd.mod_mul(&y2, p_exp, mw);
        y2.add_assign(&x2);
        y2.add_assign(y1);

        Point { x: x2, y: y2 }
    }

    /// Double the point.
    pub fn twice(&self, curve: &Curve) -> Point {
        self.add(self, curve)
    }

    /// Scalar multiplication k * P. Uses Lopez-Dahab projective coordinates
    /// with windowed NAF (window 4) for ~10x speedup over naive double-and-add.
    ///
    /// `k` is a field-style element representing a non-negative scalar.
    /// Note: callers should ensure `k < curve_order` for cryptographic uses.
    pub fn mul(&self, k: &FieldEl, curve: &Curve) -> Point {
        if k.is_zero_ct() {
            return Point::zero(curve.mod_words);
        }
        if self.is_zero() {
            return Point::zero(curve.mod_words);
        }
        crate::proj::mul_proj_wnaf(self, k, curve)
    }

    /// Naive double-and-add — kept for testing/benchmarking the wNAF path.
    pub fn mul_naive(&self, k: &FieldEl, curve: &Curve) -> Point {
        if k.is_zero() {
            return Point::zero(curve.mod_words);
        }
        if self.is_zero() {
            return Point::zero(curve.mod_words);
        }
        let bits = k.bit_length();
        if bits == 0 {
            return Point::zero(curve.mod_words);
        }
        let mut result = Point::zero(curve.mod_words);
        for i in (0..bits).rev() {
            result = result.twice(curve);
            if bit_set(&k.bytes, i) {
                result = result.add(self, curve);
            }
        }
        result
    }
}

#[inline]
fn bit_set(words: &[u32], n: u32) -> bool {
    let widx = (n / 32) as usize;
    if widx >= words.len() {
        return false;
    }
    (words[widx] & (1u32 << (n % 32))) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_point_doubling() {
        let c = Curve::dstu_pb_257();
        let g = Point::new(c.base_x.clone(), c.base_y.clone());
        let g2 = g.twice(&c);

        // Result should be on curve
        assert!(c.contains(&g2.x, &g2.y), "2*G must lie on curve");
        // And not equal to G itself
        assert!(!g2.equals(&g));
        // And not zero
        assert!(!g2.is_zero());
    }

    #[test]
    fn test_add_p_neg_p_eq_zero() {
        let c = Curve::dstu_pb_257();
        let g = Point::new(c.base_x.clone(), c.base_y.clone());
        let neg_g = g.negate();
        let zero = g.add(&neg_g, &c);
        assert!(zero.is_zero(), "P + (-P) should be infinity");
    }

    #[test]
    fn test_add_zero_returns_self() {
        let c = Curve::dstu_pb_257();
        let g = Point::new(c.base_x.clone(), c.base_y.clone());
        let zero = Point::zero(c.mod_words);
        let r = g.add(&zero, &c);
        assert!(r.equals(&g));
        let r2 = zero.add(&g, &c);
        assert!(r2.equals(&g));
    }

    #[test]
    fn test_mul_one_returns_self() {
        let c = Curve::dstu_pb_257();
        let g = Point::new(c.base_x.clone(), c.base_y.clone());
        let one = FieldEl::one(c.mod_words);
        let r = g.mul(&one, &c);
        assert!(r.equals(&g));
    }

    #[test]
    fn test_mul_two_eq_twice() {
        let c = Curve::dstu_pb_257();
        let g = Point::new(c.base_x.clone(), c.base_y.clone());
        let two = FieldEl::from_words({
            let mut v = vec![0u32; c.mod_words];
            v[0] = 2;
            v
        });
        let r = g.mul(&two, &c);
        let g2 = g.twice(&c);
        assert!(r.equals(&g2));
        assert!(c.contains(&r.x, &r.y));
    }

    #[test]
    fn test_mul_three_eq_g_plus_2g() {
        let c = Curve::dstu_pb_257();
        let g = Point::new(c.base_x.clone(), c.base_y.clone());
        let three = FieldEl::from_words({
            let mut v = vec![0u32; c.mod_words];
            v[0] = 3;
            v
        });
        let r = g.mul(&three, &c);
        let g2 = g.twice(&c);
        let g3 = g.add(&g2, &c);
        assert!(r.equals(&g3));
        assert!(c.contains(&r.x, &r.y));
    }
}
