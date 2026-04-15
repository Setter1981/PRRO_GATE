//! Field element wrapper for GF(2^m) used by DSTU 4145 elliptic curves.
//!
//! Backs onto gf2m primitives. Each field element carries a reference to
//! the irreducible polynomial (as both word-form for `finv` and exponent-form
//! for `fmod`).
//!
//! Port-equivalent of jkurwa/lib/field.js but functional rather than OO.

use crate::gf2m::{blength, finv, fmod, fmul, fsqr};

/// GF(2^m) field element.
///
/// Stored as little-endian u32 words; `bytes.len()` always equals
/// `mod_words` from the underlying curve.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldEl {
    pub bytes: Vec<u32>,
}

impl FieldEl {
    /// New zero element of given word size.
    pub fn zero(mod_words: usize) -> Self {
        Self {
            bytes: vec![0u32; mod_words],
        }
    }

    /// New one element of given word size.
    pub fn one(mod_words: usize) -> Self {
        let mut v = vec![0u32; mod_words];
        v[0] = 1;
        Self { bytes: v }
    }

    /// Construct from word array (already in little-endian form).
    pub fn from_words(bytes: Vec<u32>) -> Self {
        Self { bytes }
    }

    /// Construct from big-endian hex string (mirrors jkurwa Field 'hex' format).
    /// `mod_words` is the target word count (zero-padded as needed).
    pub fn from_hex(hex: &str, mod_words: usize) -> Self {
        let mut bytes = vec![0u32; mod_words];
        // jkurwa walks hex right-to-left (low nibble first), 8 chars per word.
        let chars: Vec<char> = hex.chars().filter(|c| !c.is_whitespace()).collect();
        let mut vidx: usize = 0;
        let mut bpos: usize = 0;
        for idx in (0..chars.len()).rev() {
            let chr = chars[idx].to_ascii_uppercase();
            let code = "0123456789ABCDEF"
                .find(chr)
                .expect("invalid hex character");
            let local_bpos = bpos % 8;
            if vidx >= mod_words {
                break; // truncate if too long
            }
            bytes[vidx] |= (code as u32) << (local_bpos * 4);
            if local_bpos == 7 {
                vidx += 1;
            }
            bpos += 1;
        }
        Self { bytes }
    }

    /// Test if element is zero.
    pub fn is_zero(&self) -> bool {
        self.bytes.iter().all(|&w| w == 0)
    }

    /// Test if equal to another element (handles trailing zeros).
    /// Constant-time: XORs all words and ORs results to avoid early-exit
    /// timing leak. Important when comparing secret-derived values like
    /// signature components or private key bits.
    pub fn equals(&self, other: &FieldEl) -> bool {
        let n = self.bytes.len().max(other.bytes.len());
        let mut diff: u32 = 0;
        for i in 0..n {
            let a = self.bytes.get(i).copied().unwrap_or(0);
            let b = other.bytes.get(i).copied().unwrap_or(0);
            diff |= a ^ b;
        }
        diff == 0
    }

    /// Constant-time zero check.
    pub fn is_zero_ct(&self) -> bool {
        let mut acc: u32 = 0;
        for &w in &self.bytes {
            acc |= w;
        }
        acc == 0
    }

    /// Bit length of the element.
    pub fn bit_length(&self) -> u32 {
        blength(&self.bytes)
    }

    /// Test bit n. Returns true if bit n is set, mirroring jkurwa's behavior
    /// where reading past the end returns true (the sentinel value used to
    /// signal "infinity" in some scalar algorithms).
    pub fn test_bit(&self, n: u32) -> bool {
        let test_word = (n / 32) as usize;
        let test_bit = n % 32;
        if test_word >= self.bytes.len() {
            return true; // matches jkurwa quirk
        }
        (self.bytes[test_word] & (1u32 << test_bit)) != 0
    }

    /// Set bit n (returns new field element).
    pub fn set_bit(&self, n: u32) -> FieldEl {
        let test_word = (n / 32) as usize;
        let test_bit = n % 32;
        if test_word >= self.bytes.len() {
            return self.clone();
        }
        let mut ret = self.clone();
        ret.bytes[test_word] |= 1u32 << test_bit;
        ret
    }

    /// Clear bit n (returns new field element).
    pub fn clear_bit(&self, n: u32) -> FieldEl {
        let test_word = (n / 32) as usize;
        let test_bit = n % 32;
        if test_word >= self.bytes.len() {
            return self.clone();
        }
        let mut ret = self.clone();
        let mask = 1u32 << test_bit;
        ret.bytes[test_word] ^= ret.bytes[test_word] & mask;
        ret
    }

    /// XOR addition (in GF(2^m), addition is XOR).
    pub fn add(&self, other: &FieldEl) -> FieldEl {
        let n = self.bytes.len().max(other.bytes.len());
        let mut out = vec![0u32; n];
        for i in 0..n {
            let a = self.bytes.get(i).copied().unwrap_or(0);
            let b = other.bytes.get(i).copied().unwrap_or(0);
            out[i] = a ^ b;
        }
        FieldEl { bytes: out }
    }

    /// In-place XOR addition.
    pub fn add_assign(&mut self, other: &FieldEl) {
        let n = self.bytes.len().max(other.bytes.len());
        if self.bytes.len() < n {
            self.bytes.resize(n, 0);
        }
        for i in 0..n {
            let b = other.bytes.get(i).copied().unwrap_or(0);
            self.bytes[i] ^= b;
        }
    }

    /// Modular multiplication using curve's modulus (exp form).
    ///
    /// Phase 2 / Commit 1: when operands are exactly DSTU PB-257 sized
    /// (mod_words=9, p_exp=[257,12,0]), delegate to the stack-allocated
    /// Fe::mod_mul. Otherwise fall back to the legacy heap-based path.
    pub fn mod_mul(&self, other: &FieldEl, p_exp: &[u32], mod_words: usize) -> FieldEl {
        if mod_words == crate::fe::FE_WORDS
            && p_exp == crate::fe::P_EXP
            && self.bytes.len() >= crate::fe::FE_WORDS
            && other.bytes.len() >= crate::fe::FE_WORDS
        {
            // Fast path: stack-allocated Fe (no heap)
            let a: crate::fe::Fe = self.into();
            let b: crate::fe::Fe = other.into();
            let result = a.mod_mul(&b);
            return (&result).into();
        }
        // Legacy path for non-DSTU-257 use cases (tests, etc.)
        let s_len = self.bytes.len() + other.bytes.len() + 2;
        let mut s = vec![0u32; s_len];
        fmul(&self.bytes, &other.bytes, &mut s);
        let mut reduced = vec![0u32; s_len];
        fmod(&s, p_exp, &mut reduced);
        FieldEl {
            bytes: reduced[..mod_words].to_vec(),
        }
    }

    /// Modular squaring. In GF(2), squaring is just bit-spreading
    /// (insert a zero between every pair of bits) followed by reduction.
    /// O(n) instead of O(n^2) for naive `mul(self, self)`.
    ///
    /// Phase 2 / Commit 1: delegates to Fe::mod_sqr when operands fit DSTU PB-257.
    pub fn mod_sqr(&self, p_exp: &[u32], mod_words: usize) -> FieldEl {
        if mod_words == crate::fe::FE_WORDS
            && p_exp == crate::fe::P_EXP
            && self.bytes.len() >= crate::fe::FE_WORDS
        {
            let a: crate::fe::Fe = self.into();
            let result = a.mod_sqr();
            return (&result).into();
        }
        let s_len = 2 * self.bytes.len();
        let mut s = vec![0u32; s_len];
        fsqr(&self.bytes, &mut s);
        let mut reduced = vec![0u32; s_len];
        fmod(&s, p_exp, &mut reduced);
        FieldEl {
            bytes: reduced[..mod_words].to_vec(),
        }
    }

    /// Modular inverse using word-form polynomial p_words.
    /// Mirrors field.js: first reduce, then call finv.
    ///
    /// Phase 2 / Commit 1: delegates to Fe::invert when operands fit DSTU PB-257.
    pub fn invert(&self, p_exp: &[u32], p_words: &[u32], mod_words: usize) -> FieldEl {
        if mod_words == crate::fe::FE_WORDS
            && p_exp == crate::fe::P_EXP
            && p_words == crate::fe::P_WORDS
            && self.bytes.len() >= crate::fe::FE_WORDS
        {
            let a: crate::fe::Fe = self.into();
            let result = a.invert();
            return (&result).into();
        }
        let mut reduced = vec![0u32; self.bytes.len()];
        fmod(&self.bytes, p_exp, &mut reduced);
        let mut a = vec![0u32; mod_words];
        let n_copy = mod_words.min(reduced.len());
        a[..n_copy].copy_from_slice(&reduced[..n_copy]);
        let mut ret = vec![0u32; mod_words];
        finv(&a, p_words, &mut ret);
        FieldEl { bytes: ret }
    }

    /// Compute trace of field element. For DSTU curves with odd m, trace is
    /// the sum (XOR) of m squarings: trace(x) = x + x^2 + x^4 + ... + x^(2^(m-1))
    /// Returns 0 or 1 (the low bit of the accumulator).
    pub fn trace(&self, m: u32, p_exp: &[u32], mod_words: usize) -> u8 {
        let mut rv = self.clone();
        for _ in 1..=(m - 1) {
            rv = rv.mod_sqr(p_exp, mod_words);
            rv.add_assign(self);
        }
        (rv.bytes[0] & 1) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dstu6_p_exp() -> [u32; 3] {
        [257, 12, 0]
    }

    fn dstu6_mod_words() -> usize {
        9
    }

    #[test]
    fn test_field_zero_one() {
        let z = FieldEl::zero(9);
        assert!(z.is_zero());
        let o = FieldEl::one(9);
        assert!(!o.is_zero());
        assert_eq!(o.bytes[0], 1);
    }

    #[test]
    fn test_field_add_xor() {
        let a = FieldEl::from_words(vec![0x12345678, 0, 0, 0, 0, 0, 0, 0, 0]);
        let b = FieldEl::from_words(vec![0x87654321, 0, 0, 0, 0, 0, 0, 0, 0]);
        let c = a.add(&b);
        assert_eq!(c.bytes[0], 0x12345678 ^ 0x87654321);
    }

    #[test]
    fn test_field_mul_then_inv_roundtrip() {
        let mod_words = dstu6_mod_words();
        let p_exp = dstu6_p_exp();
        // word form: bit 257 = word 8 bit 1; bit 12 = word 0 bit 12; bit 0 = word 0 bit 0
        let mut p_words = vec![0u32; 9];
        p_words[0] = 0x1001;
        p_words[8] = 0x2;

        let a = FieldEl::from_hex("3", 9); // x + 1
        let inv = a.invert(&p_exp, &p_words, mod_words);

        // a * inv should equal 1 mod p
        let prod = a.mod_mul(&inv, &p_exp, mod_words);
        let one = FieldEl::one(9);
        assert!(prod.equals(&one), "a * inv(a) != 1, got {:x?}", prod.bytes);
    }

    #[test]
    fn test_field_from_hex() {
        let f = FieldEl::from_hex("01CEF49472", 9);
        // bytes (little-endian words):
        // hex (BE): 01 CE F4 94 72
        // bits 0..7 = 0x72, bits 8..15 = 0x94, bits 16..23 = 0xF4, bits 24..31 = 0xCE
        // word 0 = 0xCEF49472, word 1 = 0x01, rest 0
        assert_eq!(f.bytes[0], 0xCEF4_9472);
        assert_eq!(f.bytes[1], 0x01);
        assert_eq!(f.bytes[2], 0);
    }

    #[test]
    fn test_test_bit_set_clear() {
        let f = FieldEl::from_words(vec![0xFF, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(f.test_bit(0));
        assert!(f.test_bit(7));
        assert!(!f.test_bit(8));

        let g = f.set_bit(8);
        assert!(g.test_bit(8));
        assert_eq!(g.bytes[0], 0x1FF);

        let h = g.clear_bit(0);
        assert!(!h.test_bit(0));
        assert_eq!(h.bytes[0], 0x1FE);
    }
}
