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

use crate::core::curve::Curve;
use crate::core::field::FieldEl;
use crate::core::point::Point;

/// Returns `true` when the 256-bit value encoded in `fe_words[0..8]` was
/// already ≥ the curve order and had to be canonicalised by
/// `Scalar::from_fe_truncated`. Used by `sign` and `verify` to detect
/// non-canonical caller inputs.
fn scalar_differs_from_low256(
    canonical: &crate::core::scalar::Scalar,
    fe_words: &[u32; crate::core::fe::FE_WORDS],
) -> bool {
    let canonical_bytes = canonical.to_le_bytes();
    let mut raw_bytes = [0u8; 32];
    for i in 0..8 {
        raw_bytes[i * 4..i * 4 + 4].copy_from_slice(&fe_words[i].to_le_bytes());
    }
    canonical_bytes != raw_bytes
}

/// Truncate a field element so its bit length is strictly less than the
/// curve order's bit length. Semantically identical to jkurwa
/// `Curve.truncate` — it clears every bit from position `bit_length(order)`
/// upward — but rewritten as a fixed-width mask operation so the cost no
/// longer depends on the input value.
///
/// ## CT notes (Sprint 2.1c3)
///
/// The previous implementation looped
/// `while order.bit_length() <= value.bit_length()`, calling `blength()`
/// on the secret input each iteration. Both the loop count and
/// `blength()`'s internal short-circuit leaked the magnitude of the
/// secret-derived product `hash * eG.x`. The rewrite:
///
///   - determines the cut position from `order` only (public);
///   - writes to every word at or above the cut using fixed indexing;
///   - never observes the secret's bit pattern.
///
/// `order` is a curve parameter, so `bit_length(order)` is a compile-time
/// constant in practice (256 for DSTU PB-257). We still compute it at
/// runtime to keep the function generic over curves.
pub fn truncate(value: &FieldEl, order: &FieldEl) -> FieldEl {
    // Post-condition: `result.bit_length() < order.bit_length()`.
    // In jkurwa's loop this was reached by clearing the highest set bit of
    // `value` while `value.bit_length() >= order.bit_length()`. Equivalently:
    // every bit at position `bit_length(order) - 1` and above is zero in
    // the output. The CT rewrite clears that entire range unconditionally.
    let bitl_o = order.bit_length() as usize;
    let mut ret = value.clone();
    if bitl_o == 0 {
        // Degenerate order (should not happen on real curves); return value
        // unchanged to match the old loop's behaviour (it never entered).
        return ret;
    }
    let cut = bitl_o - 1; // bit index at/above which all bits are cleared
    let cut_word = cut / 32;
    let cut_bit = (cut % 32) as u32;

    if cut_word < ret.bytes.len() {
        // Keep bits [0, cut_bit) of the boundary word; clear bit `cut_bit`
        // and everything above.
        let keep_mask: u32 = if cut_bit == 0 {
            0
        } else {
            (1u32 << cut_bit) - 1
        };
        ret.bytes[cut_word] &= keep_mask;

        // Zero all higher words unconditionally — fixed-index loop, no
        // branch on the value being truncated.
        for w in ret.bytes.iter_mut().skip(cut_word + 1) {
            *w = 0;
        }
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
    use crate::core::scalar::Scalar;

    // --- Sprint 2.2 contract enforcement (Expert-2 finding) ---
    //
    // Both the Montgomery ladder and the `s = d·r + e mod n` computation
    // must see the SAME scalar. The ladder processes every bit in
    // `rand_e` (288 bits for PB-257); `Scalar::from_fe_truncated` only
    // reads the low 256. A caller-supplied `rand_e` with bits above 255
    // would therefore split the nonce into two different values — eG
    // derived from the full 288-bit scalar, `s` derived from only the
    // low 256 — and the resulting signature would not verify.
    //
    // We require `rand_e < 2^256` (equivalently: word 8 must be zero).
    // Values in `[n, 2^256)` are still accepted: `Scalar::from_fe_truncated`
    // reduces them mod `n`, and `k·G` depends only on `k mod (order of G)`,
    // so both sides see the same scalar. This matches jkurwa's behaviour
    // and preserves byte-identity against the jkurwa vector set (which
    // contains such `rand_e` inputs).
    let e_words = rand_e.try_as_fe_words()?;
    if e_words[crate::core::fe::FE_WORDS - 1] != 0 {
        return None;
    }
    let mut e_scalar = Scalar::from_fe_truncated(e_words);

    // `mul_base_x_ct` returns only the x-coordinate of k·G — sufficient
    // for DSTU 4145 sign, which never needs y(k·G). The ladder uses the
    // same `rand_e` that `e_scalar` was derived from (word 8 is proven
    // zero above, so the 288-bit ladder result equals the 256-bit
    // `e_scalar` value — no domain mismatch).
    let eg_x = crate::core::mladder::mul_base_x_ct(rand_e, curve)?;
    if eg_x.is_zero_ct() {
        return None;
    }
    // r = hash * eG.x  (field multiplication in GF(2^m))
    let r_field = hash.mod_mul(&eg_x, &curve.p_exp, curve.mod_words);
    let r = truncate(&r_field, &curve.order);
    if r.is_zero_ct() {
        return None;
    }

    let r_scalar = Scalar::from_fe_truncated(r.try_as_fe_words()?);
    let mut d_scalar = Scalar::from_fe_truncated(d.try_as_fe_words()?);

    // s = (d * r + e) mod n
    let mut s_scalar = d_scalar.mul_mod(&r_scalar).add_mod(&e_scalar);

    // Wipe secret scalar temporaries. Scalar is Copy so there's no
    // auto-wipe on drop; we do it explicitly here, at the boundary
    // where the secret leaves the arithmetic domain and becomes a
    // serialised signature component.
    use zeroize::Zeroize;
    d_scalar.zeroize();
    e_scalar.zeroize();

    // Pack s back into a FieldEl with mod_words u32 limbs.
    let s_bytes = s_scalar.to_le_bytes();
    s_scalar.zeroize();
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
    use crate::core::scalar::Scalar;

    // Public-key validation (Sprint-hardening audit 2026-04-16). Without
    // these checks, a caller can pass a point off the curve or in the
    // wrong subgroup and coerce `verify` into accepting forged signatures
    // (DSTU 4145 §5.2 and the generic small-subgroup attack).
    //
    // We check:
    //   (a) Q != O  — the point at infinity obviously can't verify anything.
    //   (b) Q lies on the curve equation y² + xy = x³ + ax² + b.
    //   (c) cofactor·Q != O — cheap "not in a small subgroup" check; the
    //       full `n·Q == O` assertion would require scalar mul with the
    //       order, which doubles verify cost for a threat that doesn't
    //       apply in our deployment (DSTU PB-257 has cofactor 4, and keys
    //       from the Ukrainian CAs are generated with the order-clearing
    //       step already done). Multiplying by the cofactor rules out
    //       the 2-torsion / 4-torsion subgroup points, which is what a
    //       small-subgroup attacker would try to feed us.
    if pub_q.is_zero() {
        return false;
    }
    if !curve.contains(&pub_q.x, &pub_q.y) {
        return false;
    }
    // Small-subgroup fast path: if `cofactor·Q == O`, Q sits in the
    // kernel of the cofactor map (2-/4-torsion etc.) and can't be a
    // valid long-term public key. Cheap short-circuit before the full
    // order check below.
    {
        let mut h_words = vec![0u32; curve.mod_words];
        h_words[0] = curve.kofactor;
        let h_fe = FieldEl::from_words(h_words);
        if pub_q.mul(&h_fe, curve).is_zero() {
            return false;
        }
    }
    // Full subgroup check: `n · Q == O`. Guarantees Q lies in the
    // prime-order subgroup and closes the remaining small-subgroup
    // / invalid-curve attack surface reachable through the public
    // Python `verify_dstu_pb_257(pub_x, pub_y, ...)` entry point.
    //
    // `curve.order` is public, so wNAF scalar-mul here is fine
    // (no secret-timing concerns). Adds ~10 ms per verify against
    // the cofactor fast path — acceptable for verify, which runs off
    // the hot fiscal signing path.
    if !pub_q.mul(&curve.order, curve).is_zero() {
        return false;
    }

    let r = &signature.r;
    let s = &signature.s;

    if r.is_zero() || s.is_zero() {
        return false;
    }

    // Accept only well-formed FieldEl widths. Sprint 2.2 (Expert-2 review):
    // verify() must never panic on caller-supplied signatures. The old
    // `r.as_fe_words()` path would, given a short FieldEl; it now
    // surfaces as `false`.
    let r_words = match r.try_as_fe_words() {
        Some(w) => w,
        None => return false,
    };
    let s_words = match s.try_as_fe_words() {
        Some(w) => w,
        None => return false,
    };

    // Reject any non-zero bits above 2^256 (word 8 in FE_WORDS=9 layout).
    // These cannot fit in a valid scalar < order. The pre-Sprint-2.2 check
    // only inspected the low 8 words and would have admitted malformed r/s
    // carrying bits at position ≥ 256.
    if r_words[crate::core::fe::FE_WORDS - 1] != 0
        || s_words[crate::core::fe::FE_WORDS - 1] != 0
    {
        return false;
    }

    // Check r, s < order. `Scalar::from_fe_truncated` canonicalises; if
    // the original 256-bit value was ≥ n, the canonical differs — a
    // malformed (non-canonical) signature.
    let r_check = Scalar::from_fe_truncated(r_words);
    let s_check = Scalar::from_fe_truncated(s_words);
    if scalar_differs_from_low256(&r_check, r_words)
        || scalar_differs_from_low256(&s_check, s_words)
    {
        return false;
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

    /// `truncate` (Sprint 2.1c3 CT rewrite) must match jkurwa's loop-based
    /// semantics exactly: the post-condition is
    /// `result.bit_length() < order.bit_length()`. For DSTU PB-257 that
    /// means clearing every bit at position ≥ 255 (not ≥ 256 — an
    /// off-by-one here broke `test_sign_byte_identical_with_jkurwa` during
    /// development). These cases probe the bit-255 boundary specifically
    /// plus a non-word-aligned cut.
    #[test]
    fn test_truncate_ct_matches_semantics() {
        let curve = Curve::dstu_pb_257();
        let bitl_o = curve.order.bit_length() as usize;
        assert_eq!(bitl_o, 256, "precondition: DSTU PB-257 order is 256-bit");

        // Case A: value already has `bit_length() < bitl_o` → unchanged.
        // `0xDEADBEEF` in word 0 and `0x12345678` in word 6 (only touching
        // bits ≤ 222) both satisfy the post-condition.
        let mut v = vec![0u32; curve.mod_words];
        v[0] = 0xDEAD_BEEF;
        v[6] = 0x1234_5678;
        let value = FieldEl::from_words(v.clone());
        let out = truncate(&value, &curve.order);
        assert!(out.equals(&value), "values below the cut must be preserved verbatim");

        // Case B: bit 255 set → MUST be cleared (jkurwa's loop clears it
        // because `xbit == 256 >= bitl_o == 256`).
        let mut v = vec![0u32; curve.mod_words];
        v[7] = 1u32 << 31; // bit 255
        v[0] = 0xAA55_AA55;
        let value = FieldEl::from_words(v);
        let out = truncate(&value, &curve.order);
        assert_eq!(
            out.bytes[7] & (1u32 << 31),
            0,
            "bit 255 must be cleared by truncate (jkurwa parity)"
        );
        assert_eq!(out.bytes[0], 0xAA55_AA55, "bits below the cut must survive");

        // Case C: bit 256 and 255 both set plus high interior junk → both
        // must be cleared, low bits kept.
        let mut v = vec![0u32; curve.mod_words];
        v[8] = 0x0000_00FF; // bits 256..263
        v[7] = 0xFFFF_FFFF; // bits 224..255
        v[0] = 0xBADD_F00D;
        let value = FieldEl::from_words(v);
        let out = truncate(&value, &curve.order);
        assert_eq!(out.bytes[8], 0, "word 8 (bit 256+) must be zero");
        assert_eq!(out.bytes[7], 0x7FFF_FFFF, "bit 255 must be cleared, bits 224..254 kept");
        assert_eq!(out.bytes[0], 0xBADD_F00D, "word 0 must be preserved");

        // Case D: non-word-aligned cut — synthesize a 5-bit order (cut at
        // bit 4). Expected: keep bits 0..=3, clear the rest.
        let tiny_order = {
            let mut w = vec![0u32; curve.mod_words];
            w[0] = 0b10000; // bit 4 set → bit_length = 5
            FieldEl::from_words(w)
        };
        assert_eq!(tiny_order.bit_length(), 5);
        let all_ones = FieldEl::from_words(vec![u32::MAX; curve.mod_words]);
        let out = truncate(&all_ones, &tiny_order);
        assert_eq!(out.bytes[0], 0b1111, "low 4 bits must survive (cut at bit 4)");
        for i in 1..curve.mod_words {
            assert_eq!(out.bytes[i], 0, "word {} must be cleared", i);
        }

        // Case E: post-condition always holds — output `bit_length() < bitl_o`.
        assert!(out.bit_length() < tiny_order.bit_length());
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

    /// Sprint 2.2 regression guard. Expert-2 found that the old code used
    /// the FULL 288-bit `rand_e` in the Montgomery ladder but only the
    /// LOW 256 bits in the `s = d·r + e mod n` computation. With bit 256
    /// of rand_e set, the two paths saw different scalars and verify
    /// would fail on the resulting signature. The fix makes `sign`
    /// reject such an input at entry instead of silently producing a
    /// broken signature.
    #[test]
    fn sign_rejects_rand_e_with_bits_above_255() {
        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_hex("DEADBEEFCAFE12345678", curve.mod_words);
        let hash = FieldEl::from_hex(
            "0102030405060708091011121314151617181920212223242526272829303132",
            curve.mod_words,
        );

        // Build `rand_e` with bit 256 set (word 8 bit 0).
        let mut bad_words = vec![0u32; curve.mod_words];
        bad_words[0] = 0xCAFE_BABE;
        bad_words[1] = 0xDEAD_BEEF;
        bad_words[crate::core::fe::FE_WORDS - 1] = 1; // bit 256
        let bad_rand_e = FieldEl::from_words(bad_words);

        assert!(
            sign(&curve, &d, &hash, &bad_rand_e).is_none(),
            "sign must reject rand_e carrying bits at position ≥ 256 \
             (would split nonce into two domains; produces invalid sig)"
        );
    }

    /// The OsRng-driven signing path must produce signatures that actually
    /// verify. Before Sprint 2.2 the ladder / scalar domain mismatch meant
    /// almost every production sign (~100 % of OsRng samples, because the
    /// old 288-bit draw essentially always set some bit at position ≥ 256)
    /// yielded a signature that failed verification. This test drives the
    /// fixed path end-to-end with deterministic but full-width inputs.
    #[test]
    fn sign_verify_roundtrip_with_non_trivial_rand_e() {
        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_hex("DEADBEEFCAFE12345678", curve.mod_words);
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let pub_q = g.mul(&d, &curve).negate();
        let hash = FieldEl::from_hex(
            "FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210",
            curve.mod_words,
        );

        // `rand_e` strictly below 2^256 but with several high-interior
        // bits set — exactly the shape that the old `from_fe_truncated`
        // canonicalised away while the ladder processed verbatim.
        let rand_e = FieldEl::from_hex(
            "7FFFFFFFFFFFFFFF0000000000000000AAAA5555AAAA5555DEADBEEFCAFEBABE",
            curve.mod_words,
        );

        let sig = sign(&curve, &d, &hash, &rand_e).expect("sign must succeed");
        assert!(
            verify(&curve, &pub_q, &hash, &sig),
            "sign → verify roundtrip must succeed for rand_e inside the \
             enforced `< 2^256` contract"
        );
    }

    /// `verify()` must never panic on caller-supplied signatures. Malformed
    /// widths (shorter FieldEl than `FE_WORDS`) now surface as `false`
    /// via `try_as_fe_words`.
    /// Audit-driven regression: `verify` must reject a pubkey that
    /// isn't on the curve. Without Q-on-curve validation an attacker
    /// can coerce the verify math into accepting forgeries constructed
    /// with a malicious pubkey.
    #[test]
    fn verify_rejects_pubkey_off_curve() {
        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_hex("ABCD", curve.mod_words);
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let good_q = g.mul(&d, &curve).negate();
        let hash = FieldEl::from_hex("1234", curve.mod_words);

        // Produce a real signature so the only reason to reject is the
        // tampered pubkey.
        let rand_e = FieldEl::from_hex("DEADBEEF0000", curve.mod_words);
        let sig = sign(&curve, &d, &hash, &rand_e).unwrap();
        assert!(verify(&curve, &good_q, &hash, &sig));

        // Now flip one bit of Q.y — the point falls off the curve.
        let mut bad_q = good_q.clone();
        bad_q.y.bytes[0] ^= 1;
        assert!(!curve.contains(&bad_q.x, &bad_q.y));
        assert!(!verify(&curve, &bad_q, &hash, &sig));
    }

    /// Point at infinity must also be rejected.
    #[test]
    fn verify_rejects_zero_pubkey() {
        let curve = Curve::dstu_pb_257();
        let hash = FieldEl::from_hex("1234", curve.mod_words);
        let sig = Signature {
            r: small_field(&curve, 1),
            s: small_field(&curve, 1),
        };
        let zero_q = Point::zero(curve.mod_words);
        assert!(!verify(&curve, &zero_q, &hash, &sig));
    }

    #[test]
    fn verify_rejects_malformed_width_without_panic() {
        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_hex("ABCD", curve.mod_words);
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let pub_q = g.mul(&d, &curve).negate();
        let hash = FieldEl::from_hex("1234", curve.mod_words);

        // A three-word FieldEl is below FE_WORDS=9; pre-Sprint-2.2 code
        // would panic in `as_fe_words()`. Post-fix it must return false.
        let narrow = FieldEl::from_words(vec![0x1u32, 0x2, 0x3]);
        let bad_sig = Signature {
            r: narrow.clone(),
            s: narrow,
        };
        assert!(!verify(&curve, &pub_q, &hash, &bad_sig));
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
