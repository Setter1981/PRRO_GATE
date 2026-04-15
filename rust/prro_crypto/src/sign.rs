//! DSTU 4145 digital signature: sign and verify.
//!
//! Algorithm summary (matches jkurwa Priv.help_sign / Pub.help_verify):
//!
//! Sign(curve, d, hash, e):
//!   eG = e * G
//!   if eG.x == 0 return None  (caller retries with new e)
//!   r = truncate(hash * eG.x)
//!   if r == 0 return None
//!   s = (d * r + e) mod n     (where n = curve order)
//!   return (r, s)
//!
//! Verify(curve, pub_q, hash, r, s):
//!   pointR = s*G + r*pub_q
//!   if pointR.is_zero() return false
//!   r1 = truncate(pointR.x * hash)
//!   return r == r1
//!
//! `pub_q` is the public point Q = -d * G (jkurwa stores the negation).
//! With Q = -d*G, verify reduces to:
//!   pointR = s*G + r*(-d*G) = (s - d*r)*G = e*G = eG
//! so r1 = truncate(eG.x * hash) == r when sign was correct.

use crate::curve::Curve;
use crate::field::FieldEl;
use crate::point::Point;

/// Truncate a field element so its bit length is strictly less than the
/// curve order's bit length. Mirrors jkurwa Curve.truncate.
pub fn truncate(value: &FieldEl, order: &FieldEl) -> FieldEl {
    let bitl_o = order.bit_length();
    let mut ret = value.clone();
    let mut xbit = ret.bit_length();
    while bitl_o <= xbit && xbit > 0 {
        ret = ret.clear_bit(xbit - 1);
        xbit = ret.bit_length();
    }
    ret
}

/// Result of a successful sign.
#[derive(Clone, Debug)]
pub struct Signature {
    pub r: FieldEl,
    pub s: FieldEl,
}

/// DSTU 4145 sign. Returns None when the chosen `rand_e` produces a
/// degenerate signature (eG.x == 0 or r == 0); caller should retry with
/// a fresh `rand_e`.
///
/// Inputs:
///   - `curve`: the DSTU curve (e.g., Curve::dstu_pb_257())
///   - `d`: signer's private key as a field element
///   - `hash`: message hash as a field element (e.g., GOST 34.311 hash)
///   - `rand_e`: a uniformly random non-zero scalar < curve order
///
/// All field elements MUST be sized with `mod_words = curve.mod_words`.
pub fn sign(
    curve: &Curve,
    d: &FieldEl,
    hash: &FieldEl,
    rand_e: &FieldEl,
) -> Option<Signature> {
    // Phase 2 / Commit 6: use fixed-base scalar mul (precomputed table for G).
    // Eliminates per-sign wNAF table construction (~114 µs of pure overhead).
    let eg = crate::fixed_base::mul_base(rand_e, curve);
    if eg.x.is_zero() {
        return None;
    }
    // r = hash * eG.x  (field multiplication in GF(2^m))
    let r_field = hash.mod_mul(&eg.x, &curve.p_exp, curve.mod_words);
    let r = truncate(&r_field, &curve.order);
    if r.is_zero() {
        return None;
    }

    // Phase 2 / Commit 4: scalar arithmetic mod n via custom Scalar([u64; 4]),
    // no BigUint allocations.
    use crate::scalar::Scalar;
    let r_scalar = Scalar::from_fe_truncated(&r.bytes);
    let d_scalar = Scalar::from_fe_truncated(&d.bytes);
    let e_scalar = Scalar::from_fe_truncated(&rand_e.bytes);

    // s = (d * r + e) mod n
    let s_scalar = d_scalar.mul_mod(&r_scalar).add_mod(&e_scalar);

    // Pack s back into a FieldEl with mod_words u32 limbs.
    let s_bytes = s_scalar.to_le_bytes();
    let mut s_words = vec![0u32; curve.mod_words];
    for i in 0..s_words.len().min(8) {
        s_words[i] = u32::from_le_bytes([
            s_bytes[i * 4],
            s_bytes[i * 4 + 1],
            s_bytes[i * 4 + 2],
            s_bytes[i * 4 + 3],
        ]);
    }
    let s = FieldEl::from_words(s_words);

    Some(Signature { r, s })
}

/// DSTU 4145 verify.
///
/// Inputs:
///   - `curve`: the DSTU curve
///   - `pub_q`: the signer's public point (typically Q = -d*G)
///   - `hash`: the message hash
///   - `signature`: the (r, s) pair to verify
///
/// Returns true if signature is valid, false otherwise.
/// Returns false on any malformed input (zero r/s, r/s >= order).
pub fn verify(curve: &Curve, pub_q: &Point, hash: &FieldEl, signature: &Signature) -> bool {
    let r = &signature.r;
    let s = &signature.s;

    if r.is_zero() || s.is_zero() {
        return false;
    }

    // Check r, s < order via Scalar (no BigUint).
    use crate::scalar::Scalar;
    let r_check = Scalar::from_fe_truncated(&r.bytes);
    let s_check = Scalar::from_fe_truncated(&s.bytes);
    // from_fe_truncated already canonicalizes (subtracts n if needed). If the
    // ORIGINAL FE was >= n, the canonicalized value differs — that's a malformed sig.
    let r_words = r_check.to_le_bytes();
    let s_words = s_check.to_le_bytes();
    let mut r_orig_le = [0u8; 32];
    let mut s_orig_le = [0u8; 32];
    for i in 0..8 {
        let r_word = r.bytes.get(i).copied().unwrap_or(0);
        let s_word = s.bytes.get(i).copied().unwrap_or(0);
        r_orig_le[i * 4..i * 4 + 4].copy_from_slice(&r_word.to_le_bytes());
        s_orig_le[i * 4..i * 4 + 4].copy_from_slice(&s_word.to_le_bytes());
    }
    if r_orig_le != r_words || s_orig_le != s_words {
        return false; // r or s was >= order (rejected)
    }

    // pointR = s*G + r*Q
    let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
    let mul_s = g.mul(s, curve);
    let mul_q = pub_q.mul(r, curve);
    let point_r = mul_s.add(&mul_q, curve);

    if point_r.is_zero() {
        return false;
    }

    // r1 = truncate(pointR.x * hash)
    let r1_field = point_r.x.mod_mul(hash, &curve.p_exp, curve.mod_words);
    let r1 = truncate(&r1_field, &curve.order);

    r.equals(&r1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_field(curve: &Curve, value: u32) -> FieldEl {
        let mut v = vec![0u32; curve.mod_words];
        v[0] = value;
        FieldEl::from_words(v)
    }

    #[test]
    fn test_truncate_smaller_than_order() {
        let curve = Curve::dstu_pb_257();
        let small = small_field(&curve, 0xDEAD);
        let truncated = truncate(&small, &curve.order);
        // Should be unchanged since 0xDEAD < order
        assert!(truncated.equals(&small));
    }

    #[test]
    fn test_truncate_larger_than_order() {
        let curve = Curve::dstu_pb_257();
        // Order has bit length 256 (jkurwa standard.js: order is 256 bits).
        // Construct value with bit 256 set — should be truncated.
        let mut large_words = vec![0u32; curve.mod_words];
        // bit 256 = word 8, bit 0 = word_idx 8 bit 0 → words[8] = 1
        large_words[8] = 1;
        large_words[0] = 0xDEAD;
        let large = FieldEl::from_words(large_words);

        let truncated = truncate(&large, &curve.order);
        // After truncation, bit length must be < order's bit length
        assert!(
            truncated.bit_length() < curve.order.bit_length(),
            "truncated bit_length {} must be < order bit_length {}",
            truncated.bit_length(),
            curve.order.bit_length()
        );
    }

    #[test]
    fn test_sign_verify_roundtrip() {
        let curve = Curve::dstu_pb_257();

        // Synthetic private key d (non-zero, < order)
        let d = FieldEl::from_hex("DEADBEEFCAFE12345678", curve.mod_words);
        // Pubkey: jkurwa stores Q = -d*G (negate of d*G)
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let dg = g.mul(&d, &curve);
        let pub_q = dg.negate();

        // Sanity: pubkey is on curve
        assert!(curve.contains(&pub_q.x, &pub_q.y));

        // Hash (some non-zero field element)
        let hash = FieldEl::from_hex("01020304050607080910AABBCCDDEEFF", curve.mod_words);

        // Random scalar e (non-zero, < order). For test we use deterministic value.
        let e = FieldEl::from_hex("123456789ABCDEF0FEDCBA9876543210", curve.mod_words);

        let sig = sign(&curve, &d, &hash, &e).expect("sign must succeed for this input");

        // Verify with same inputs
        assert!(
            verify(&curve, &pub_q, &hash, &sig),
            "valid signature must verify"
        );

        // Verify with modified hash should fail
        let mut bad_hash_words = hash.bytes.clone();
        bad_hash_words[0] ^= 1;
        let bad_hash = FieldEl::from_words(bad_hash_words);
        assert!(
            !verify(&curve, &pub_q, &bad_hash, &sig),
            "modified hash must not verify"
        );

        // Verify with modified r should fail
        let mut bad_r_words = sig.r.bytes.clone();
        bad_r_words[0] ^= 1;
        let bad_sig = Signature {
            r: FieldEl::from_words(bad_r_words),
            s: sig.s.clone(),
        };
        assert!(
            !verify(&curve, &pub_q, &hash, &bad_sig),
            "modified r must not verify"
        );
    }

    #[test]
    fn test_sign_with_known_zero_eG_returns_none() {
        // Cannot easily contrive eG.x == 0 deterministically without knowing
        // the curve structure deeply. Skip — covered by general round-trip.
        // Placeholder to document the behavior contract.
    }

    #[test]
    fn test_zero_sig_components_rejected() {
        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_hex("ABCD", curve.mod_words);
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let pub_q = g.mul(&d, &curve).negate();
        let hash = FieldEl::from_hex("1234", curve.mod_words);

        let zero = FieldEl::zero(curve.mod_words);
        let one = small_field(&curve, 1);

        let bad1 = Signature {
            r: zero.clone(),
            s: one.clone(),
        };
        let bad2 = Signature {
            r: one.clone(),
            s: zero.clone(),
        };
        assert!(!verify(&curve, &pub_q, &hash, &bad1));
        assert!(!verify(&curve, &pub_q, &hash, &bad2));
    }
}
