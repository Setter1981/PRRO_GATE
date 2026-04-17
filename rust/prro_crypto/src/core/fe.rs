//! Fixed-size field elements for DSTU PB-257.
//!
//! Replaces `FieldEl(Vec<u32>)` in hot paths with stack-allocated
//! `Fe([u32; 9])` and `FeWide([u32; 18])`. This is Phase 2 / Commit 1
//! of the representation refactor: introduce the new types, keep the
//! old FieldEl as a compatibility wrapper, prove parity via tests.
//!
//! Sizes:
//! - Fe: 9 u32 words = 288 bits, sufficient for GF(2^257) elements.
//! - FeWide: 18 u32 words = 576 bits, holds an unreduced product
//!   of two Fe values plus the slack required by the mul algorithm
//!   (a.len() + b.len() + 2 in the existing `fmul`).

use crate::core::gf2m::{blength, fmod};

/// Number of u32 words to hold one DSTU PB-257 field element (288 bits).
pub const FE_WORDS: usize = 9;

/// Number of u32 words to hold an unreduced product.
/// Algorithmically: `a.len() + b.len() + 2` slack for the 2x2 fmul on
/// odd-sized operands. Mathematically a 257*257 product is 513 bits (17 words),
/// but the schoolbook algorithm rounds up to an even pair of pairs.
/// Commit 2's specialized `fmul_257` will collapse this to exactly 17 words.
pub const FE_WIDE: usize = 20;

/// Reduction polynomial for DSTU PB-257 in exponent form.
pub const P_EXP: [u32; 3] = [257, 12, 0];

/// Reduction polynomial in word form (for `finv`).
/// x^257 + x^12 + 1 → bit 257 in word 8, bits 12 and 0 in word 0.
pub const P_WORDS: [u32; FE_WORDS] = [0x1001, 0, 0, 0, 0, 0, 0, 0, 0x2];

/// Field element in GF(2^257). Stored as little-endian u32 words on the stack.
///
/// Always normalized (high bits above 257 are zero after reduction).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fe(pub [u32; FE_WORDS]);

/// Unreduced product of two Fe values (up to 514 bits + algorithmic slack).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeWide(pub [u32; FE_WIDE]);

impl Fe {
    /// All-zero element.
    pub const ZERO: Fe = Fe([0; FE_WORDS]);

    /// Multiplicative identity (the polynomial 1).
    pub const ONE: Fe = {
        let mut w = [0u32; FE_WORDS];
        w[0] = 1;
        Fe(w)
    };

    /// Constructor from raw words.
    #[inline]
    pub const fn from_words(w: [u32; FE_WORDS]) -> Self {
        Fe(w)
    }

    /// Borrow the underlying word array.
    #[inline]
    pub fn as_words(&self) -> &[u32; FE_WORDS] {
        &self.0
    }

    /// Constant-time zero check (XOR-OR all words, compare to 0).
    #[inline]
    pub fn ct_is_zero(&self) -> bool {
        let mut acc: u32 = 0;
        for &w in &self.0 {
            acc |= w;
        }
        acc == 0
    }

    /// Constant-time equality.
    #[inline]
    pub fn ct_eq(&self, other: &Fe) -> bool {
        let mut diff: u32 = 0;
        for i in 0..FE_WORDS {
            diff |= self.0[i] ^ other.0[i];
        }
        diff == 0
    }

    /// XOR addition (in characteristic 2, addition is XOR).
    #[inline]
    pub fn add(&self, other: &Fe) -> Fe {
        let mut out = [0u32; FE_WORDS];
        for i in 0..FE_WORDS {
            out[i] = self.0[i] ^ other.0[i];
        }
        Fe(out)
    }

    /// In-place XOR addition.
    #[inline]
    pub fn add_assign(&mut self, other: &Fe) {
        for i in 0..FE_WORDS {
            self.0[i] ^= other.0[i];
        }
    }

    /// Bit length (index of highest set bit + 1, or 0 if all zero).
    #[inline]
    pub fn bit_length(&self) -> u32 {
        blength(&self.0)
    }

    /// Test bit n. Returns false for out-of-range indices.
    #[inline]
    pub fn test_bit(&self, n: u32) -> bool {
        let widx = (n / 32) as usize;
        if widx >= FE_WORDS {
            return false;
        }
        (self.0[widx] & (1u32 << (n % 32))) != 0
    }

    /// Construct from big-endian hex string.
    pub fn from_hex(hex: &str) -> Self {
        let mut bytes = [0u32; FE_WORDS];
        let chars: Vec<char> = hex.chars().filter(|c| !c.is_whitespace()).collect();
        let mut vidx: usize = 0;
        let mut bpos: usize = 0;
        for idx in (0..chars.len()).rev() {
            let chr = chars[idx].to_ascii_uppercase();
            let code = "0123456789ABCDEF"
                .find(chr)
                .expect("invalid hex character");
            let local_bpos = bpos % 8;
            if vidx >= FE_WORDS {
                break;
            }
            bytes[vidx] |= (code as u32) << (local_bpos * 4);
            if local_bpos == 7 {
                vidx += 1;
            }
            bpos += 1;
        }
        Fe(bytes)
    }

    /// Modular multiplication: self * other mod (x^257 + x^12 + 1).
    ///
    /// Phase 2 / Commit 2: uses specialized `gf2m_257` backend with
    /// hardcoded reduction. No exponent-list loop, no length parameters.
    #[inline]
    pub fn mod_mul(&self, other: &Fe) -> Fe {
        crate::core::gf2m_257::mul_257(self, other)
    }

    /// Modular squaring via specialized fsqr_257 + freduce_257.
    #[inline]
    pub fn mod_sqr(&self) -> Fe {
        crate::core::gf2m_257::sqr_257(self)
    }

    /// Modular inverse in GF(2^257).
    ///
    /// **Constant-time** — implemented via Itoh-Tsujii (addition chain
    /// over squarings + 8 multiplications). No branches or loops depend
    /// on the input. Used on hot signing paths; see `invert_binary_euclid`
    /// below for the variable-time reference implementation kept only
    /// for regression testing.
    ///
    /// ## Algorithm
    ///
    /// For GF(2^m) the multiplicative group has order `2^m - 1`, so
    /// `a^(-1) = a^(2^m - 2) = (a^(2^(m-1) - 1))^2`. With `m = 257` we
    /// need `β_256 = a^(2^256 - 1)`, then square once.
    ///
    /// `β_{2k} = β_k · (β_k)^(2^k)` lets us double the exponent with
    /// `k` squarings and one multiplication. Walking the chain
    /// `1 → 2 → 4 → 8 → … → 256` costs `1+2+4+…+128 = 255` squarings,
    /// 8 multiplications, plus one final squaring. All primitives
    /// (`mod_sqr`, `mod_mul`) are constant-time.
    ///
    /// ## Zero behaviour
    ///
    /// `0^k = 0` for any positive `k`, so inverting zero returns zero.
    /// This matches the abstract field's undefined behaviour (zero has
    /// no inverse) — callers must not rely on it. The signing path never
    /// inverts zero on a valid curve point.
    #[inline]
    pub fn invert(&self) -> Fe {
        let beta_1 = *self;

        // β_2 = a · a^{2}
        let beta_2 = {
            let t = beta_1.mod_sqr();
            beta_1.mod_mul(&t)
        };

        // β_4 = β_2 · β_2^{2^2}
        let beta_4 = {
            let mut t = beta_2.mod_sqr();
            t = t.mod_sqr();
            beta_2.mod_mul(&t)
        };

        // β_8 = β_4 · β_4^{2^4}
        let beta_8 = {
            let mut t = beta_4;
            for _ in 0..4 {
                t = t.mod_sqr();
            }
            beta_4.mod_mul(&t)
        };

        // β_16 = β_8 · β_8^{2^8}
        let beta_16 = {
            let mut t = beta_8;
            for _ in 0..8 {
                t = t.mod_sqr();
            }
            beta_8.mod_mul(&t)
        };

        // β_32 = β_16 · β_16^{2^16}
        let beta_32 = {
            let mut t = beta_16;
            for _ in 0..16 {
                t = t.mod_sqr();
            }
            beta_16.mod_mul(&t)
        };

        // β_64 = β_32 · β_32^{2^32}
        let beta_64 = {
            let mut t = beta_32;
            for _ in 0..32 {
                t = t.mod_sqr();
            }
            beta_32.mod_mul(&t)
        };

        // β_128 = β_64 · β_64^{2^64}
        let beta_128 = {
            let mut t = beta_64;
            for _ in 0..64 {
                t = t.mod_sqr();
            }
            beta_64.mod_mul(&t)
        };

        // β_256 = β_128 · β_128^{2^128}
        let beta_256 = {
            let mut t = beta_128;
            for _ in 0..128 {
                t = t.mod_sqr();
            }
            beta_128.mod_mul(&t)
        };

        // a^{-1} = β_256^{2}
        beta_256.mod_sqr()
    }

    /// Variable-time inverse via binary extended Euclid. NOT constant-time;
    /// kept solely as a regression oracle for `invert` and for tests that
    /// do not run on secret data.
    ///
    /// Must not be called from any code path that handles secret scalars
    /// or private keys.
    #[doc(hidden)]
    pub fn invert_binary_euclid(&self) -> Fe {
        let mut a = [0u32; FE_WORDS];
        fmod(&self.0, &P_EXP, &mut a);
        let mut out = [0u32; FE_WORDS];
        crate::core::gf2m::finv(&a, &P_WORDS, &mut out);
        Fe(out)
    }
}

impl FeWide {
    pub const ZERO: FeWide = FeWide([0; FE_WIDE]);
}

// ─── Compatibility adapters ────────────────────────────────────────────────
// Convert between the new fixed-size Fe and the legacy Vec-backed FieldEl.
// These let us migrate one module at a time. After Commit 3, the legacy
// FieldEl is removed entirely and these go away.

impl From<&crate::core::field::FieldEl> for Fe {
    fn from(legacy: &crate::core::field::FieldEl) -> Self {
        let mut w = [0u32; FE_WORDS];
        let n = legacy.bytes.len().min(FE_WORDS);
        w[..n].copy_from_slice(&legacy.bytes[..n]);
        Fe(w)
    }
}

impl From<&Fe> for crate::core::field::FieldEl {
    fn from(fe: &Fe) -> Self {
        crate::core::field::FieldEl::from_words(fe.0.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::field::FieldEl;

    /// Helper: build a synthetic non-trivial Fe value.
    fn make_fe(seed: u32) -> Fe {
        let mut x = seed;
        let mut w = [0u32; FE_WORDS];
        for i in 0..FE_WORDS {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            w[i] = x;
        }
        w[8] &= 0x1; // mask to <2^257
        Fe(w)
    }

    #[test]
    fn test_zero_one_constants() {
        assert!(Fe::ZERO.ct_is_zero());
        assert!(!Fe::ONE.ct_is_zero());
        assert_eq!(Fe::ONE.0[0], 1);
        for i in 1..FE_WORDS {
            assert_eq!(Fe::ONE.0[i], 0);
        }
    }

    #[test]
    fn test_add_xor() {
        let a = Fe([0x12345678, 0, 0, 0, 0, 0, 0, 0, 0]);
        let b = Fe([0x87654321, 0, 0, 0, 0, 0, 0, 0, 0]);
        let c = a.add(&b);
        assert_eq!(c.0[0], 0x12345678 ^ 0x87654321);
    }

    #[test]
    fn test_ct_eq() {
        let a = make_fe(1);
        let b = make_fe(1);
        let c = make_fe(2);
        assert!(a.ct_eq(&b));
        assert!(!a.ct_eq(&c));
    }

    #[test]
    fn test_test_bit_in_range() {
        let f = Fe([0xFF, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(f.test_bit(0));
        assert!(f.test_bit(7));
        assert!(!f.test_bit(8));
        // Out of range returns false (NOT the FieldEl quirk that returned true)
        assert!(!f.test_bit(9999));
    }

    #[test]
    fn test_from_hex_matches_legacy() {
        let hex = "01CEF49472";
        let fe = Fe::from_hex(hex);
        let legacy = FieldEl::from_hex(hex, FE_WORDS);
        let from_legacy: Fe = (&legacy).into();
        assert!(fe.ct_eq(&from_legacy), "Fe::from_hex differs from FieldEl::from_hex");
    }

    #[test]
    fn test_mod_mul_matches_legacy() {
        let a = make_fe(0xCAFE);
        let b = make_fe(0xBABE);
        let result_new = a.mod_mul(&b);

        let a_legacy: FieldEl = (&a).into();
        let b_legacy: FieldEl = (&b).into();
        let result_legacy = a_legacy.mod_mul(&b_legacy, &P_EXP, FE_WORDS);
        let result_legacy_fe: Fe = (&result_legacy).into();

        assert!(
            result_new.ct_eq(&result_legacy_fe),
            "Fe::mod_mul differs from FieldEl::mod_mul\n  new:    {:08x?}\n  legacy: {:08x?}",
            result_new.0,
            result_legacy_fe.0,
        );
    }

    #[test]
    fn test_mod_sqr_matches_legacy() {
        let a = make_fe(42);
        let result_new = a.mod_sqr();

        let a_legacy: FieldEl = (&a).into();
        let result_legacy = a_legacy.mod_sqr(&P_EXP, FE_WORDS);
        let result_legacy_fe: Fe = (&result_legacy).into();

        assert!(result_new.ct_eq(&result_legacy_fe));
    }

    #[test]
    fn test_invert_roundtrip() {
        let a = make_fe(0xDEAD);
        let inv = a.invert();
        let prod = a.mod_mul(&inv);
        assert!(prod.ct_eq(&Fe::ONE), "a * a^(-1) should be 1");
    }

    #[test]
    fn test_invert_matches_legacy() {
        let a = make_fe(0xBEEF);
        let inv_new = a.invert();

        let a_legacy: FieldEl = (&a).into();
        let inv_legacy = a_legacy.invert(&P_EXP, &P_WORDS, FE_WORDS);
        let inv_legacy_fe: Fe = (&inv_legacy).into();

        assert!(inv_new.ct_eq(&inv_legacy_fe));
    }

    /// Differential: Itoh-Tsujii `invert` vs the binary-Euclid oracle over
    /// a stream of pseudo-random inputs. If these diverge on any element
    /// the CT path has a bug that the single-input test above cannot catch.
    #[test]
    fn test_invert_ct_matches_binary_euclid_batch() {
        for seed in 0..512u32 {
            let a = make_fe(0xC0FE ^ seed.wrapping_mul(0x9E37_79B1));
            // Skip zero — neither algorithm is defined there.
            if a.0.iter().all(|w| *w == 0) {
                continue;
            }
            let ct = a.invert();
            let gcd = a.invert_binary_euclid();
            assert!(
                ct.ct_eq(&gcd),
                "Itoh-Tsujii vs binary-Euclid mismatch at seed {}: ct={:?} gcd={:?}",
                seed,
                ct.0,
                gcd.0
            );
        }
    }

    /// For every non-zero input, `a * invert(a)` must be ONE.
    #[test]
    fn test_invert_ct_roundtrip_batch() {
        for seed in 0..256u32 {
            let a = make_fe(0xBADC0DE ^ seed.wrapping_mul(2654435761));
            if a.0.iter().all(|w| *w == 0) {
                continue;
            }
            let inv = a.invert();
            let prod = a.mod_mul(&inv);
            assert!(
                prod.ct_eq(&Fe::ONE),
                "roundtrip failed at seed {}: product = {:?}",
                seed,
                prod.0
            );
        }
    }

    #[test]
    fn test_adapter_roundtrip() {
        let a = make_fe(0x1234);
        let legacy: FieldEl = (&a).into();
        let back: Fe = (&legacy).into();
        assert!(a.ct_eq(&back), "Fe → FieldEl → Fe roundtrip lost data");
    }
}
