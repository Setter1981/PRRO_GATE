//! DSTU 4145 elliptic curve parameters and helpers.
//!
//! Currently supports DSTU_PB_257 (curve 6 — the only one used by the Ukrainian
//! tax authority for fiscal signing). DSTU_PB_431 and others can be added by
//! defining additional `Standard*` constants.

use crate::field::FieldEl;

/// DSTU curve definition (binary-field elliptic curve y^2 + xy = x^3 + ax^2 + b).
#[derive(Clone, Debug)]
pub struct Curve {
    /// Field degree m (e.g. 257 for DSTU_PB_257).
    pub m: u32,
    /// Reduction polynomial exponents in descending order, terminated by 0.
    /// e.g. for x^257 + x^12 + 1: [257, 12, 0].
    pub p_exp: Vec<u32>,
    /// Reduction polynomial in word form (for `finv`).
    pub p_words: Vec<u32>,
    /// Number of u32 words needed for one field element (= ceil(m / 32)).
    pub mod_words: usize,
    /// Curve coefficient a (one of {0, 1} for DSTU curves).
    pub a: FieldEl,
    /// Curve coefficient b.
    pub b: FieldEl,
    /// Group order n.
    pub order: FieldEl,
    /// Cofactor (small integer, e.g. 4 for DSTU_PB_257).
    pub kofactor: u32,
    /// Base point G of the cyclic subgroup of order n.
    pub base_x: FieldEl,
    pub base_y: FieldEl,
}

impl Curve {
    /// Construct DSTU_PB_257 (curve 6) — the standard Ukrainian fiscal curve.
    pub fn dstu_pb_257() -> Self {
        let m: u32 = 257;
        let mod_words = ((m as usize) + 31) / 32;
        // Polynomial: x^257 + x^12 + 1
        let p_exp = vec![257u32, 12, 0];
        // Word form: word 0 has bit 0 (x^0) and bit 12 (x^12); word 8 has bit 1 (x^257)
        let mut p_words = vec![0u32; mod_words];
        p_words[0] = (1u32 << 0) | (1u32 << 12); // 0x1001
        p_words[8] = 1u32 << 1; // 0x2

        // Curve params from jkurwa standard.js DSTU_PB_257:
        // a = "0"
        // b = "01 CEF49472 0115657E 18F938D7 A7942394 FF9425C1 458C5786 1F9EEA6A DBE3BE10"
        // base.x = "2A29EF20 7D0E9B6C 55CD260B 306C7E00 7AC491CA 1B10C623 34A9E8DC D8D20FB7"
        // base.y = "01 0686D41F F744D444 9FCCF6D8 EEA03102 E6812C93 A9D60B97 8B702CF1 56D814EF"
        // order = "80000000 00000000 00000000 00000000 6759213A F182E987 D3E17714 907D470D"
        // kofactor = 4
        let a = FieldEl::from_hex("0", mod_words);
        let b = FieldEl::from_hex(
            "01CEF494720115657E18F938D7A7942394FF9425C1458C57861F9EEA6ADBE3BE10",
            mod_words,
        );
        let base_x = FieldEl::from_hex(
            "2A29EF207D0E9B6C55CD260B306C7E007AC491CA1B10C62334A9E8DCD8D20FB7",
            mod_words,
        );
        let base_y = FieldEl::from_hex(
            "010686D41FF744D4449FCCF6D8EEA03102E6812C93A9D60B978B702CF156D814EF",
            mod_words,
        );
        let order = FieldEl::from_hex(
            "800000000000000000000000000000006759213AF182E987D3E17714907D470D",
            mod_words,
        );

        Self {
            m,
            p_exp,
            p_words,
            mod_words,
            a,
            b,
            order,
            kofactor: 4,
            base_x,
            base_y,
        }
    }

    /// Verify that point (x, y) lies on the curve:
    /// y^2 + xy + a*x^2 = x^3 + b   (over GF(2^m))
    /// Equivalently: (x^3 + b) ^ (y^2 + xy + a*x^2) == 0.
    pub fn contains(&self, x: &FieldEl, y: &FieldEl) -> bool {
        // lh = (x + a) * x + y  — adapted from jkurwa Point.contains:
        //   lh = (x + a) * x + y
        //   lh = lh * x + b
        //   lh += y^2
        //   return lh == 0
        let mut lh = x.add(&self.a);
        lh = lh.mod_mul(x, &self.p_exp, self.mod_words);
        lh.add_assign(y);
        lh = lh.mod_mul(x, &self.p_exp, self.mod_words);
        lh.add_assign(&self.b);

        let y2 = y.mod_sqr(&self.p_exp, self.mod_words);
        lh.add_assign(&y2);

        lh.is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dstu_pb_257_construction() {
        let c = Curve::dstu_pb_257();
        assert_eq!(c.m, 257);
        assert_eq!(c.mod_words, 9);
        assert_eq!(c.kofactor, 4);
        assert!(c.a.is_zero());
        assert!(!c.b.is_zero());
    }

    #[test]
    fn test_dstu_pb_257_base_on_curve() {
        let c = Curve::dstu_pb_257();
        assert!(
            c.contains(&c.base_x, &c.base_y),
            "DSTU_PB_257 base point should lie on curve"
        );
    }
}
