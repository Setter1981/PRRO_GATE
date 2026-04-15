//! 256-bit scalar arithmetic mod the DSTU PB-257 group order.
//!
//! Replaces `num-bigint` in sign/verify hot paths with a fixed-size
//! `Scalar([u64; 4])` and Barrett reduction specialized for the curve order.
//!
//! ## Design (Phase 2 / Commit 4, per expert reviewer's plan)
//!
//! - **Layout:** little-endian `[u64; 4]`. Limb 0 = lowest 64 bits.
//! - **Reduction:** Barrett with precomputed `mu = floor(2^512 / n)`.
//! - **Invariant:** every public `Scalar` value is canonical (`< n`).
//! - **Variable-time** for now (modular reduction). Constant-time hardening
//!   is a separate later commit. All loops are fixed-size; no heap.
//!
//! ## Group order n (DSTU PB-257)
//!
//! n = 0x80000000_00000000_00000000_00000000_6759213A_F182E987_D3E17714_907D470D
//! bit_length(n) = 256

/// Group order n as little-endian limbs.
pub const ORDER_LIMBS: [u64; 4] = [
    0xD3E17714_907D470D,
    0x6759213A_F182E987,
    0x00000000_00000000,
    0x80000000_00000000,
];

/// Barrett constant: mu = floor(2^512 / n). 5 little-endian limbs.
const MU_LIMBS: [u64; 5] = [
    0xB07A23AD_BE0AE3CD,
    0x629B7B14_39F459E0,
    0xFFFFFFFF_FFFFFFFE,
    0xFFFFFFFF_FFFFFFFF,
    0x00000000_00000001,
];

/// Bit length of the order. For DSTU PB-257 this is exactly 256.
pub const ORDER_BITS: usize = 256;

/// 256-bit scalar in [0, n).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Scalar(pub [u64; 4]);

impl Scalar {
    pub const ZERO: Scalar = Scalar([0, 0, 0, 0]);

    pub const ONE: Scalar = Scalar([1, 0, 0, 0]);

    /// The group order n (NOT a valid scalar in [0,n) — exported for inspection only).
    pub const ORDER: Scalar = Scalar(ORDER_LIMBS);

    /// Build from raw limbs WITHOUT canonicalizing. Caller asserts `< n`.
    /// Used internally; public callers should use `from_le_bytes` or
    /// `from_fe_truncated`.
    #[inline]
    pub const fn from_limbs(limbs: [u64; 4]) -> Self {
        Scalar(limbs)
    }

    /// Borrow underlying limbs.
    #[inline]
    pub fn as_limbs(&self) -> &[u64; 4] {
        &self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    /// `self < n`?
    pub fn is_canonical(&self) -> bool {
        cmp_lt_limbs(&self.0, &ORDER_LIMBS)
    }

    /// `self.cmp(other)` over u256 unsigned.
    pub fn cmp(&self, other: &Scalar) -> core::cmp::Ordering {
        for i in (0..4).rev() {
            if self.0[i] < other.0[i] {
                return core::cmp::Ordering::Less;
            }
            if self.0[i] > other.0[i] {
                return core::cmp::Ordering::Greater;
            }
        }
        core::cmp::Ordering::Equal
    }

    /// (self + rhs) mod n.
    pub fn add_mod(&self, rhs: &Scalar) -> Scalar {
        let (sum, carry) = add_with_carry(&self.0, &rhs.0);
        // If we carried out OR the sum >= n, subtract n.
        if carry || !cmp_lt_limbs(&sum, &ORDER_LIMBS) {
            let (diff, _) = sub_with_borrow(&sum, &ORDER_LIMBS);
            Scalar(diff)
        } else {
            Scalar(sum)
        }
    }

    /// (self - rhs) mod n.
    pub fn sub_mod(&self, rhs: &Scalar) -> Scalar {
        let (diff, borrow) = sub_with_borrow(&self.0, &rhs.0);
        if borrow {
            let (sum, _) = add_with_carry(&diff, &ORDER_LIMBS);
            Scalar(sum)
        } else {
            Scalar(diff)
        }
    }

    /// (self * rhs) mod n via Barrett reduction.
    pub fn mul_mod(&self, rhs: &Scalar) -> Scalar {
        let wide = mul_512(&self.0, &rhs.0);
        reduce_barrett(&wide)
    }

    /// Truncate a field element (256-bit lower portion) and conditionally
    /// subtract n. Used for converting `r` from `hash * eG.x` in sign.
    ///
    /// Per expert: mask to bitlen(n) + one conditional subtract.
    /// For DSTU PB-257, bitlen(n) = 256, so we just take the low 256 bits
    /// of the field element and (if needed) subtract n once.
    pub fn from_fe_truncated(fe_words: &[u32]) -> Scalar {
        // Build [u64; 4] from the low 256 bits of fe_words (which is [u32; ≥9]).
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            let lo = fe_words.get(2 * i).copied().unwrap_or(0) as u64;
            let hi = fe_words.get(2 * i + 1).copied().unwrap_or(0) as u64;
            limbs[i] = lo | (hi << 32);
        }
        // Already < 2^256. Order is also 256-bit. One conditional subtract is enough.
        if !cmp_lt_limbs(&limbs, &ORDER_LIMBS) {
            let (diff, _) = sub_with_borrow(&limbs, &ORDER_LIMBS);
            Scalar(diff)
        } else {
            Scalar(limbs)
        }
    }

    /// Construct from 32 little-endian bytes.
    pub fn from_le_bytes(bytes: &[u8; 32]) -> Scalar {
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&bytes[8 * i..8 * (i + 1)]);
            limbs[i] = u64::from_le_bytes(buf);
        }
        if !cmp_lt_limbs(&limbs, &ORDER_LIMBS) {
            let (diff, _) = sub_with_borrow(&limbs, &ORDER_LIMBS);
            Scalar(diff)
        } else {
            Scalar(limbs)
        }
    }

    /// Convert to 32 little-endian bytes.
    pub fn to_le_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..4 {
            out[8 * i..8 * (i + 1)].copy_from_slice(&self.0[i].to_le_bytes());
        }
        out
    }

    /// Test bit i (0 = lowest).
    #[inline]
    pub fn bit(&self, i: usize) -> u64 {
        if i >= 256 {
            return 0;
        }
        (self.0[i / 64] >> (i % 64)) & 1
    }

    pub fn is_odd(&self) -> bool {
        (self.0[0] & 1) == 1
    }

    /// Bit length (index of highest set bit + 1, or 0 for zero).
    pub fn bit_len(&self) -> usize {
        for i in (0..4).rev() {
            if self.0[i] != 0 {
                return 64 - self.0[i].leading_zeros() as usize + 64 * i;
            }
        }
        0
    }
}

// ─── Internal limb-level helpers ───────────────────────────────────────────

/// `a < b` for 4-limb little-endian unsigned values.
fn cmp_lt_limbs(a: &[u64; 4], b: &[u64; 4]) -> bool {
    for i in (0..4).rev() {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
    }
    false
}

/// a + b → (sum, carry_out)
fn add_with_carry(a: &[u64; 4], b: &[u64; 4]) -> ([u64; 4], bool) {
    let mut out = [0u64; 4];
    let mut carry: u64 = 0;
    for i in 0..4 {
        let (s1, c1) = a[i].overflowing_add(b[i]);
        let (s2, c2) = s1.overflowing_add(carry);
        out[i] = s2;
        carry = (c1 as u64) | (c2 as u64);
    }
    (out, carry != 0)
}

/// a - b → (diff, borrow_out)
fn sub_with_borrow(a: &[u64; 4], b: &[u64; 4]) -> ([u64; 4], bool) {
    let mut out = [0u64; 4];
    let mut borrow: u64 = 0;
    for i in 0..4 {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        out[i] = d2;
        borrow = (b1 as u64) | (b2 as u64);
    }
    (out, borrow != 0)
}

/// Schoolbook 4x4 → 8 limb multiplication using u128 for partial products.
fn mul_512(a: &[u64; 4], b: &[u64; 4]) -> [u64; 8] {
    let mut out = [0u64; 8];
    for i in 0..4 {
        let mut carry: u128 = 0;
        for j in 0..4 {
            let prod = (a[i] as u128) * (b[j] as u128) + (out[i + j] as u128) + carry;
            out[i + j] = prod as u64;
            carry = prod >> 64;
        }
        out[i + 4] = carry as u64;
    }
    out
}

/// Multiply [u64; 5] by [u64; 4], low 5 limbs only (truncated).
/// Used in Barrett step `q3 * n` mod 2^((k+1)*64) = mod 2^320.
///
/// Subtle: after the inner j loop completes naturally (j reached 4 without
/// hitting the i+j >= 5 break), the residual carry is the high half of the
/// last (a[i] * b[3]) product, which mathematically belongs at position i+4.
/// For i=0 that's position 4 (in range — must propagate). For i >= 1 that's
/// position 5+ (out of range — drop).
fn mul_5x4_low5(a: &[u64; 5], b: &[u64; 4]) -> [u64; 5] {
    let mut out = [0u64; 5];
    for i in 0..5 {
        let mut carry: u128 = 0;
        for j in 0..4 {
            let pos = i + j;
            if pos >= 5 {
                break;
            }
            let prod = (a[i] as u128) * (b[j] as u128) + (out[pos] as u128) + carry;
            out[pos] = prod as u64;
            carry = prod >> 64;
        }
        // Propagate residual carry to out[i+4] if in range. Only i=0 satisfies this.
        let final_pos = i + 4;
        if final_pos < 5 {
            let sum = (out[final_pos] as u128).wrapping_add(carry);
            out[final_pos] = sum as u64;
            // Higher bits of `sum` beyond u64 would go to out[5+] — out of range, drop.
        }
    }
    out
}

/// Barrett reduction: x (8 limbs, < 2^512) mod n → 4-limb result < n.
///
/// Algorithm (HAC 14.42, base b=2^64, k=4):
/// 1. q1 = floor(x / b^(k-1)) — high 5 limbs of x
/// 2. q2 = q1 * mu              — 5 × 5 = up to 9 limbs
/// 3. q3 = floor(q2 / b^(k+1))  — high 4 limbs of q2
/// 4. r1 = x mod b^(k+1)        — low 5 limbs of x
/// 5. r2 = (q3 * n) mod b^(k+1) — low 5 limbs of (q3 * n)
/// 6. r = r1 - r2 (mod 2^((k+1)*64))
/// 7. while r >= n: r -= n  (at most 2 iterations)
fn reduce_barrett(x: &[u64; 8]) -> Scalar {
    // Step 1: q1 = x[3..8] (5 limbs, the high portion)
    let mut q1 = [0u64; 5];
    q1.copy_from_slice(&x[3..8]);

    // Step 2: q2 = q1 * mu  — 5x5 product, fits in 10 limbs
    let q2 = mul_5x5(&q1, &MU_LIMBS);

    // Step 3: q3 = q2[5..10] — high 5 limbs of q2 (we use 4 + extra for safety)
    let mut q3 = [0u64; 5];
    q3.copy_from_slice(&q2[5..10]);

    // Step 4: r1 = x[0..5]
    let mut r1 = [0u64; 5];
    r1.copy_from_slice(&x[0..5]);

    // Step 5: r2 = (q3 * n) mod 2^320
    let r2 = mul_5x4_low5(&q3, &ORDER_LIMBS);

    // Step 6: r = r1 - r2 mod 2^320
    let mut r = [0u64; 5];
    let mut borrow: u64 = 0;
    for i in 0..5 {
        let (d1, b1) = r1[i].overflowing_sub(r2[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        r[i] = d2;
        borrow = (b1 as u64) | (b2 as u64);
    }

    // Step 7: r should now fit in 5 limbs but may still be ≥ n.
    // At most 2 conditional subtractions of n.
    for _ in 0..3 {
        if cmp_lt_5_limbs(&r, &order_5_limbs()) {
            break;
        }
        r = sub_5_limbs(&r, &order_5_limbs());
    }

    // Take low 4 limbs (top limb should be zero by now)
    let mut out = [0u64; 4];
    out.copy_from_slice(&r[0..4]);
    Scalar(out)
}

/// Multiply two 5-limb numbers → 10 limbs.
fn mul_5x5(a: &[u64; 5], b: &[u64; 5]) -> [u64; 10] {
    let mut out = [0u64; 10];
    for i in 0..5 {
        let mut carry: u128 = 0;
        for j in 0..5 {
            let prod = (a[i] as u128) * (b[j] as u128) + (out[i + j] as u128) + carry;
            out[i + j] = prod as u64;
            carry = prod >> 64;
        }
        out[i + 5] = carry as u64;
    }
    out
}

/// `a < b` for 5-limb little-endian.
fn cmp_lt_5_limbs(a: &[u64; 5], b: &[u64; 5]) -> bool {
    for i in (0..5).rev() {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
    }
    false
}

/// `a - b` for 5-limb. Caller ensures a >= b (ignores final borrow).
fn sub_5_limbs(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let mut out = [0u64; 5];
    let mut borrow: u64 = 0;
    for i in 0..5 {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        out[i] = d2;
        borrow = (b1 as u64) | (b2 as u64);
    }
    out
}

#[inline]
fn order_5_limbs() -> [u64; 5] {
    [
        ORDER_LIMBS[0],
        ORDER_LIMBS[1],
        ORDER_LIMBS[2],
        ORDER_LIMBS[3],
        0,
    ]
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;

    fn order_bn() -> BigUint {
        BigUint::from_bytes_le(&Scalar::ORDER.to_le_bytes())
    }

    fn scalar_to_bn(s: &Scalar) -> BigUint {
        BigUint::from_bytes_le(&s.to_le_bytes())
    }

    fn bn_to_scalar(n: &BigUint) -> Scalar {
        let mut bytes = n.to_bytes_le();
        bytes.resize(32, 0);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Scalar::from_le_bytes(&arr)
    }

    #[test]
    fn test_zero_one() {
        assert!(Scalar::ZERO.is_zero());
        assert!(!Scalar::ONE.is_zero());
        assert!(Scalar::ZERO.is_canonical());
        assert!(Scalar::ONE.is_canonical());
    }

    #[test]
    fn test_order_constant() {
        // ORDER itself is NOT canonical (n is not in [0, n))
        assert!(!Scalar::ORDER.is_canonical());
    }

    #[test]
    fn test_le_bytes_roundtrip() {
        let bytes = [
            0x0D, 0x47, 0x7D, 0x90, 0x14, 0x77, 0xE1, 0xD3, 0x87, 0xE9, 0x82, 0xF1, 0x3A, 0x21,
            0x59, 0x67, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x80,
        ];
        // This is exactly ORDER. from_le_bytes should canonicalize → ZERO.
        let s = Scalar::from_le_bytes(&bytes);
        assert!(s.is_zero(), "ORDER bytes should canonicalize to zero");
    }

    #[test]
    fn test_add_mod_simple() {
        let a = Scalar::ONE;
        let b = Scalar::ONE;
        let c = a.add_mod(&b);
        assert_eq!(c.0[0], 2);
    }

    #[test]
    fn test_add_mod_wraparound() {
        // (n - 1) + 1 = n ≡ 0
        let n_minus_1 = bn_to_scalar(&(order_bn() - 1u32));
        let result = n_minus_1.add_mod(&Scalar::ONE);
        assert!(result.is_zero(), "(n-1) + 1 should be 0 mod n");
    }

    #[test]
    fn test_sub_mod_underflow() {
        // 0 - 1 = -1 ≡ n - 1
        let result = Scalar::ZERO.sub_mod(&Scalar::ONE);
        let expected = bn_to_scalar(&(order_bn() - 1u32));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_mul_mod_small() {
        // 2 * 3 = 6
        let two = Scalar([2, 0, 0, 0]);
        let three = Scalar([3, 0, 0, 0]);
        let six = two.mul_mod(&three);
        assert_eq!(six.0, [6, 0, 0, 0]);
    }

    #[test]
    fn test_mul_mod_large() {
        // Random-ish 256-bit values; cross-check with BigUint.
        let a = Scalar([
            0xDEAD_BEEF_CAFE_BABE,
            0x1234_5678_9ABC_DEF0,
            0xFEDC_BA98_7654_3210,
            0x0F0F_F0F0_AAAA_5555,
        ]);
        let b = Scalar([
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
            0x9999_AAAA_BBBB_CCCC,
            0x0011_2233_4455_6677,
        ]);
        // Make canonical
        let a_canon = if a.is_canonical() { a } else { Scalar::from_le_bytes(&a.to_le_bytes()) };
        let b_canon = if b.is_canonical() { b } else { Scalar::from_le_bytes(&b.to_le_bytes()) };

        let result = a_canon.mul_mod(&b_canon);

        let expected_bn = (&scalar_to_bn(&a_canon) * &scalar_to_bn(&b_canon)) % order_bn();
        let expected = bn_to_scalar(&expected_bn);

        assert_eq!(
            result, expected,
            "mul_mod differs from BigUint reference\n  got:      {:016X?}\n  expected: {:016X?}",
            result.0, expected.0
        );
    }

    #[test]
    fn test_mul_mod_n_minus_1_squared() {
        // (n-1)^2 mod n = 1
        let n_minus_1 = bn_to_scalar(&(order_bn() - 1u32));
        let result = n_minus_1.mul_mod(&n_minus_1);
        assert_eq!(result, Scalar::ONE, "(n-1)^2 should be 1 mod n");
    }

    #[test]
    fn test_from_fe_truncated_small() {
        // Small FE that's already canonical — should pass through unchanged.
        let fe_words = [0x1234_5678u32, 0x9ABC_DEF0, 0, 0, 0, 0, 0, 0, 0];
        let s = Scalar::from_fe_truncated(&fe_words);
        assert_eq!(s.0[0], 0x9ABC_DEF0_1234_5678);
        assert_eq!(s.0[1], 0);
    }

    #[test]
    fn test_from_fe_truncated_above_order() {
        // FE with top bit set = larger than n (since n starts with 0x80...)
        // so we need a value where bit 255 is set AND low part > order's low part.
        // Order = 0x80...00006759213A...907D470D
        // Let's try FE where bit 255 is set but everything else is zero.
        let mut fe = [0u32; 9];
        fe[7] = 0x8000_0000; // bit 255
        let s = Scalar::from_fe_truncated(&fe);
        // 2^255 < n (n = 2^255 + low_part), so this should be 2^255 unchanged.
        assert_eq!(s.0[3], 0x8000_0000_0000_0000);
        assert_eq!(s.0[2], 0);
        assert_eq!(s.0[1], 0);
        assert_eq!(s.0[0], 0);
        assert!(s.is_canonical());
    }

    #[test]
    fn test_bit_and_oddness() {
        let s = Scalar([0b10101011, 0, 0, 0]);
        assert!(s.is_odd());
        assert_eq!(s.bit(0), 1);
        assert_eq!(s.bit(1), 1);
        assert_eq!(s.bit(2), 0);
        assert_eq!(s.bit(3), 1);
        assert_eq!(s.bit(8), 0);
        assert_eq!(s.bit(255), 0);
        assert_eq!(s.bit(999), 0);
    }

    #[test]
    fn test_bit_len() {
        assert_eq!(Scalar::ZERO.bit_len(), 0);
        assert_eq!(Scalar::ONE.bit_len(), 1);
        assert_eq!(Scalar([0xFF, 0, 0, 0]).bit_len(), 8);
        assert_eq!(Scalar([0, 1, 0, 0]).bit_len(), 65);
        assert_eq!(Scalar([0, 0, 0, 1u64 << 63]).bit_len(), 256);
    }
}
