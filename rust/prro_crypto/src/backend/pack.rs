//! Pack/unpack helpers between the public Fe([u32; 9]) representation
//! and the SIMD-friendly Fe64([u64; 5]) packed form.
//!
//! The 257-bit field needs 5 u64 limbs to hold (256 bits in the low 4 limbs,
//! plus 1 bit in the top limb). Tail bit is masked to 0/1 to keep
//! representations canonical.
//!
//! For the wide product (514 bits of unreduced output), we need 9 u64 limbs
//! to be safe, but practical SIMD multiply produces 8 u64 limbs of cross
//! product plus tail XOR contributions.

use crate::fe::{Fe, FeWide, FE_WIDE, FE_WORDS};

/// Packed field element: 5 little-endian u64 limbs.
/// Limbs 0..3 hold bits 0..255. Limb 4 holds bit 256 (the 257-th bit).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fe64(pub [u64; 5]);

/// Packed wide product: up to 9 u64 limbs (576 bits).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Wide64(pub [u64; 9]);

impl Fe64 {
    pub const ZERO: Fe64 = Fe64([0; 5]);
}

impl Wide64 {
    pub const ZERO: Wide64 = Wide64([0; 9]);
}

/// Pack Fe([u32; 9]) into Fe64([u64; 5]).
///
/// The Fe layout is little-endian u32. Pairs of u32 limbs combine into u64.
/// The 9th u32 (limb 8) holds at most 1 bit (the 257-th); we mask to enforce.
#[inline]
pub fn pack_fe(fe: &Fe) -> Fe64 {
    let w = &fe.0;
    Fe64([
        (w[0] as u64) | ((w[1] as u64) << 32),
        (w[2] as u64) | ((w[3] as u64) << 32),
        (w[4] as u64) | ((w[5] as u64) << 32),
        (w[6] as u64) | ((w[7] as u64) << 32),
        // Top limb: only bit 0 of word 8 matters (the 257-th bit overall).
        // Higher bits should not be set if Fe is canonical (< 2^257), but we
        // mask defensively to ensure no garbage propagates into multiplication.
        (w[8] as u64) & 0x1,
    ])
}

/// Unpack Fe64([u64; 5]) back into Fe([u32; 9]).
#[inline]
pub fn unpack_fe(p: &Fe64) -> Fe {
    let mut w = [0u32; FE_WORDS];
    for i in 0..4 {
        w[2 * i] = p.0[i] as u32;
        w[2 * i + 1] = (p.0[i] >> 32) as u32;
    }
    w[8] = p.0[4] as u32;
    Fe(w)
}

/// Convert a Wide64 product (up to 9 u64 limbs = 576 bits) into the
/// Fe-style FeWide([u32; FE_WIDE = 20]) layout expected by `freduce_257`.
#[inline]
pub fn unpack_wide(p: &Wide64) -> FeWide {
    let mut out = [0u32; FE_WIDE];
    for i in 0..9 {
        let lo = p.0[i] as u32;
        let hi = (p.0[i] >> 32) as u32;
        if 2 * i < FE_WIDE {
            out[2 * i] = lo;
        }
        if 2 * i + 1 < FE_WIDE {
            out[2 * i + 1] = hi;
        }
    }
    FeWide(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fe(seed: u32) -> Fe {
        let mut x = seed;
        let mut w = [0u32; FE_WORDS];
        for i in 0..FE_WORDS {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            w[i] = x;
        }
        w[8] &= 0x1; // canonical: top bit only
        Fe(w)
    }

    #[test]
    fn test_pack_unpack_roundtrip_zero() {
        let z = Fe::ZERO;
        let packed = pack_fe(&z);
        assert_eq!(packed, Fe64::ZERO);
        let unpacked = unpack_fe(&packed);
        assert_eq!(unpacked.0, z.0);
    }

    #[test]
    fn test_pack_unpack_roundtrip_one() {
        let one = Fe::ONE;
        let packed = pack_fe(&one);
        assert_eq!(packed.0[0], 1);
        for i in 1..5 {
            assert_eq!(packed.0[i], 0);
        }
        let unpacked = unpack_fe(&packed);
        assert_eq!(unpacked.0, one.0);
    }

    #[test]
    fn test_pack_unpack_roundtrip_random() {
        for seed in [1u32, 0xCAFE, 0xBABE, 0xDEAD_BEEF, 0xFFFF_FFFF] {
            let original = make_fe(seed);
            let packed = pack_fe(&original);
            let recovered = unpack_fe(&packed);
            assert_eq!(
                recovered.0, original.0,
                "Fe → Fe64 → Fe roundtrip failed for seed {}",
                seed
            );
        }
    }

    #[test]
    fn test_pack_top_bit_masking() {
        // Set high bits in word[8] that should be masked off
        let mut w = [0u32; FE_WORDS];
        w[8] = 0xFFFF_FFFF; // all 32 bits — only bit 0 should survive packing
        let fe = Fe(w);
        let packed = pack_fe(&fe);
        assert_eq!(packed.0[4], 1, "top limb should be masked to 1 bit");
    }

    #[test]
    fn test_unpack_wide_zero() {
        let z = Wide64::ZERO;
        let unpacked = unpack_wide(&z);
        assert_eq!(unpacked.0, [0u32; FE_WIDE]);
    }

    #[test]
    fn test_unpack_wide_known_values() {
        // Known limbs → expected u32 layout
        let mut p = Wide64::ZERO;
        p.0[0] = 0xDEADBEEF_CAFEBABE;
        p.0[1] = 0x12345678_9ABCDEF0;
        let unpacked = unpack_wide(&p);
        assert_eq!(unpacked.0[0], 0xCAFEBABE);
        assert_eq!(unpacked.0[1], 0xDEADBEEF);
        assert_eq!(unpacked.0[2], 0x9ABCDEF0);
        assert_eq!(unpacked.0[3], 0x12345678);
    }
}
