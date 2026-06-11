//! Elliptic curve points over GF(2^m) for DSTU 4145.
//!
//! Port of jkurwa/lib/point.js, but without wNAF optimization (uses simple
//! double-and-add scalar multiplication). wNAF can be added later if benchmarks
//! demand it; for fiscal signing throughput, double-and-add is sufficient.

use crate::core::curve::Curve;
use crate::core::field::FieldEl;

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
        crate::core::proj::mul_proj_wnaf(self, k, curve)
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

/// Compress a DSTU 4145 affine point to its LE byte representation
/// with the y-parity smuggled into bit 0 of x. Inverse of
/// [`expand_compressed`]. Port of jkurwa `Point.prototype.compress`.
///
/// For the standard DSTU PB-257 curve the result is 33 bytes
/// (`ceil(m/8)`). The encoding is what gets fed to `compute_ski` and
/// what lives inside the BIT STRING of `subjectPublicKey` in a real
/// X.509 cert.
pub fn compress_point(p: &Point, curve: &Curve) -> Vec<u8> {
    let mw = curve.mod_words;
    let p_exp = &curve.p_exp;

    // For x = 0 the parity bit is forced to 0 (no division by x).
    let parity_bit: u8 = if p.x.is_zero() {
        0
    } else {
        let x_inv = p.x.invert(p_exp, &curve.p_words, mw);
        let z = p.y.mod_mul(&x_inv, p_exp, mw);
        z.trace(curve.m, p_exp, mw)
    };

    // Serialise x as LE bytes (ceil(m/8) bytes wide).
    let m_bytes = (curve.m as usize).div_ceil(8);
    let mut out = vec![0u8; m_bytes];
    for i in 0..m_bytes {
        let word_idx = i / 4;
        let byte_idx = i % 4;
        if word_idx < p.x.bytes.len() {
            out[i] = (p.x.bytes[word_idx] >> (byte_idx * 8)) as u8;
        }
    }
    // Mask off any bits beyond `m` in the high byte — a non-reduced
    // FieldEl (bit 257+ set by accident) would otherwise serialise as
    // garbage that decompression happily re-accepts as a different x.
    // `m = 257` → `m_bytes = 33` → byte 32 holds bits 256..263; bit 256
    // is the only legitimate one there, so mask to 0b0000_0001.
    let high_byte_idx = m_bytes - 1;
    let bits_in_high = (curve.m as usize) - (high_byte_idx * 8);
    if bits_in_high < 8 {
        let mask = (1u8 << bits_in_high) - 1;
        out[high_byte_idx] &= mask;
    }

    // Bit 0 of byte 0 = parity. Clear first then set if needed (x.bit0
    // carries no information about the curve x-coordinate — DSTU
    // reserves it for the y-parity flag).
    out[0] = (out[0] & 0xFE) | (parity_bit & 1);
    out
}

/// Typed error for checked public-key decompression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PointDecodeError {
    #[error("wrong compressed length: expected {expected}, got {actual}")]
    WrongLength { expected: usize, actual: usize },
    #[error("decoded point is the identity / infinity")]
    Infinity,
    #[error("decoded point does not lie on the curve equation")]
    NotOnCurve,
    #[error("decoded point is not in the prime-order subgroup")]
    WrongSubgroup,
}

/// Checked variant of [`expand_compressed`] — the only API the decrypt
/// and verify paths should use for untrusted-origin compressed pubkeys.
///
/// Validates in order:
///   - byte length matches `ceil(curve.m / 8)`
///   - y-recovery produces a point on the curve equation
///   - the point is not the identity
///   - the point sits in the prime-order subgroup (`n·Q == O`)
///
/// Returns `Ok(Point)` only when all four hold. The old infallible
/// [`expand_compressed`] is kept for internal / trusted-input call
/// sites (our own tests, hardcoded constants) but is marked
/// `#[deprecated]`-note-worthy for any caller-facing decrypt flow.
pub fn expand_compressed_checked(
    compressed: &[u8],
    curve: &Curve,
) -> Result<Point, PointDecodeError> {
    let expected = (curve.m as usize).div_ceil(8);
    if compressed.len() != expected {
        return Err(PointDecodeError::WrongLength {
            expected,
            actual: compressed.len(),
        });
    }
    let q = expand_compressed(compressed, curve);
    validate_public_point(curve, &q)?;
    Ok(q)
}

/// Public-point validation shared by verify/decrypt paths.
///
/// Enforces the full DSTU 4145 §5.2 checklist for a point that came
/// from untrusted bytes:
///   - not the identity
///   - on the curve equation
///   - in the prime-order subgroup (`n·Q == O`)
///
/// The order-mul costs a full scalar mul but it runs at onboarding
/// or per-envelope-decrypt — not per-chek.
pub fn validate_public_point(curve: &Curve, q: &Point) -> Result<(), PointDecodeError> {
    if q.is_zero() {
        return Err(PointDecodeError::Infinity);
    }
    if !curve.contains(&q.x, &q.y) {
        return Err(PointDecodeError::NotOnCurve);
    }
    if !q.mul(&curve.order, curve).is_zero() {
        return Err(PointDecodeError::WrongSubgroup);
    }
    Ok(())
}

/// Decompress a DSTU 4145 compressed public key (LE byte array) into
/// a full (x, y) affine point. Port of jkurwa `Curve.prototype.expand`.
///
/// The compressed format is `ceil(m/8)` bytes encoding the x-coordinate
/// in little-endian, with bit 0 carrying the parity flag for y-recovery.
///
/// **TRUSTED INPUT ONLY.** This infallible variant never returns an
/// error; a malformed compressed blob will silently produce a
/// structurally-valid but wrong `Point`. For any public-facing /
/// network / cert-sourced input, use
/// [`expand_compressed_checked`], which additionally validates that
/// Q lies on the curve and in the prime-order subgroup before handing
/// it back.
pub fn expand_compressed(compressed: &[u8], curve: &Curve) -> Point {
    let mw = curve.mod_words;
    let p_exp = &curve.p_exp;

    let mut x = FieldEl::from_le_bytes(compressed, mw);

    if x.is_zero() {
        let y = curve.b.mod_sqr(p_exp, mw);
        return Point::new(x, y);
    }

    let k = (x.bytes[0] & 1) != 0;
    x.bytes[0] &= !1u32; // clear bit 0

    let tr = x.trace(curve.m, p_exp, mw);
    let a_is_zero = curve.a.is_zero();
    if (tr != 0 && a_is_zero) || (tr == 0 && !a_is_zero) {
        x.bytes[0] |= 1; // set bit 0
    }

    let x2 = x.mod_sqr(p_exp, mw);
    let mut rhs = x2.mod_mul(&x, p_exp, mw); // x^3
    if !a_is_zero {
        rhs.add_assign(&x2); // + a*x^2 (a=1 for non-zero case)
    }
    rhs.add_assign(&curve.b); // + b

    let inv_x2 = x2.invert(p_exp, &curve.p_words, mw);
    let mut w = rhs.mod_mul(&inv_x2, p_exp, mw); // (x^3+ax^2+b)/x^2

    w = w.half_trace(curve.m, p_exp, mw);

    let tr_w = w.trace(curve.m, p_exp, mw);
    if (k && tr_w == 0) || (!k && tr_w != 0) {
        w.bytes[0] ^= 1;
    }

    let y = w.mod_mul(&x, p_exp, mw);
    Point::new(x, y)
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

    /// Checked decompression rejects a blob of the wrong byte length.
    #[test]
    fn expand_compressed_rejects_wrong_length() {
        let c = Curve::dstu_pb_257();
        let too_short = vec![0u8; 10];
        assert!(matches!(
            expand_compressed_checked(&too_short, &c),
            Err(PointDecodeError::WrongLength { .. })
        ));
        let too_long = vec![0u8; 64];
        assert!(matches!(
            expand_compressed_checked(&too_long, &c),
            Err(PointDecodeError::WrongLength { .. })
        ));
    }

    /// Round-trip: compress the canonical base point, decompress via
    /// the checked path, assert we recovered G.
    #[test]
    fn expand_compressed_accepts_canonical_base_point() {
        let c = Curve::dstu_pb_257();
        let g = Point::new(c.base_x.clone(), c.base_y.clone());
        let compressed = compress_point(&g, &c);
        let recovered = expand_compressed_checked(&compressed, &c)
            .expect("G is valid and in the prime-order subgroup");
        assert!(recovered.equals(&g));
    }

    /// `validate_public_point` rejects the identity.
    #[test]
    fn validate_public_point_rejects_infinity() {
        let c = Curve::dstu_pb_257();
        let zero = Point::zero(c.mod_words);
        assert!(matches!(
            validate_public_point(&c, &zero),
            Err(PointDecodeError::Infinity)
        ));
    }

    /// `validate_public_point` rejects a point off the curve.
    #[test]
    fn validate_public_point_rejects_off_curve() {
        let c = Curve::dstu_pb_257();
        let mut q = Point::new(c.base_x.clone(), c.base_y.clone());
        q.y.bytes[0] ^= 1; // off-curve
        assert!(matches!(
            validate_public_point(&c, &q),
            Err(PointDecodeError::NotOnCurve)
        ));
    }
}
