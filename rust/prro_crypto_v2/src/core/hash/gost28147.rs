//! GOST 28147-89 block cipher with DSTU DKU S-box.
//!
//! 64-bit block, 256-bit key, 32 rounds Feistel. Used as a primitive
//! by GOST 34.311-95 hash. This is NOT the same parameter set as Russian
//! GOST — Ukrainian DSTU uses its own S-box "DKU" (default).
//!
//! Port of `jkurwa/node_modules/gost89/lib/gost89.js`. S-box definition
//! from `gost89/lib/dstu.js`.

/// DKU: Ukrainian DSTU default S-box, 128 bytes unpacked (one nibble per byte).
/// Source: `gost89/lib/dstu.js::DKU`.
const DKU: [u8; 128] = [
    0x01, 0x02, 0x03, 0x0E, 0x06, 0x0D, 0x0B, 0x08, 0x0F, 0x0A, 0x0C, 0x05, 0x07, 0x09, 0x00, 0x04,
    0x03, 0x08, 0x0B, 0x05, 0x06, 0x04, 0x0E, 0x0A, 0x02, 0x0C, 0x01, 0x07, 0x09, 0x0F, 0x0D, 0x00,
    0x02, 0x08, 0x09, 0x07, 0x05, 0x0F, 0x00, 0x0B, 0x0C, 0x01, 0x0D, 0x0E, 0x0A, 0x03, 0x06, 0x04,
    0x0F, 0x08, 0x0E, 0x09, 0x07, 0x02, 0x00, 0x0D, 0x0C, 0x06, 0x01, 0x05, 0x0B, 0x04, 0x03, 0x0A,
    0x03, 0x08, 0x0D, 0x09, 0x06, 0x0B, 0x0F, 0x00, 0x02, 0x05, 0x0C, 0x0A, 0x04, 0x0E, 0x01, 0x07,
    0x0F, 0x06, 0x05, 0x08, 0x0E, 0x0B, 0x0A, 0x04, 0x0C, 0x00, 0x03, 0x07, 0x02, 0x09, 0x01, 0x0D,
    0x08, 0x00, 0x0C, 0x04, 0x09, 0x06, 0x07, 0x0B, 0x02, 0x03, 0x01, 0x0F, 0x05, 0x0E, 0x0A, 0x0D,
    0x0A, 0x09, 0x0D, 0x06, 0x0E, 0x0B, 0x04, 0x05, 0x0F, 0x01, 0x03, 0x0C, 0x07, 0x00, 0x08, 0x02,
];

/// Pack the unpacked 128-byte S-box into 64-byte packed form (used for
/// serialization elsewhere; NOT used by `init()`). Mirrors
/// `gost89/lib/dstu.js::packSbox`. Kept for compatibility / future use.
#[allow(dead_code)]
fn pack_sbox(input: &[u8; 128]) -> [u8; 64] {
    let mut out = [0u8; 64];
    let rows = 128 & 0xF0;
    let mut idx = 0;
    while idx < 128 {
        let ret_idx = (rows - 0x10 - (idx & 0xF0)) | (idx & 0x0F);
        out[ret_idx >> 1] = (input[idx] << 4) | input[idx + 1];
        idx += 2;
    }
    out
}

/// GOST 28147 block cipher state with precomputed S-box tables.
pub struct Gost {
    /// Round key: 8 × u32, set via `set_key`.
    pub(crate) k: [u32; 8],
    /// Precomputed S-box tables for fast 32-bit substitution+rotation.
    /// Index by byte value; result is the 4-byte column of a u32 output.
    pub(crate) k87: [u32; 256],
    pub(crate) k65: [u32; 256],
    pub(crate) k43: [u32; 256],
    pub(crate) k21: [u32; 256],
}

impl Gost {
    /// Initialize with DKU (Ukrainian DSTU default) S-box.
    ///
    /// `gost89.init()` passes the UNPACKED 128-byte DKU directly (not
    /// `packSbox(DKU)`). Each byte holds one 4-bit S-box value in its
    /// low nibble. Layout: 8 contiguous rows of 16 entries.
    pub fn with_dku() -> Self {
        Self::with_unpacked_sbox(&DKU)
    }

    /// Initialize with a packed 64-byte S-box, the form Ukrainian PFX /
    /// ZS2 / Key-6 containers carry inside the PBES2 cipher parameters.
    /// Each byte holds two 4-bit S-box values; we unpack inversely to
    /// [`pack_sbox`] before delegating to the unpacked-init path.
    pub fn with_packed_sbox(packed: &[u8; 64]) -> Self {
        let mut unpacked = [0u8; 128];
        let rows = 128 & 0xF0;
        let mut idx = 0;
        while idx < 128 {
            let ret_idx = (rows - 0x10 - (idx & 0xF0)) | (idx & 0x0F);
            let byte = packed[ret_idx >> 1];
            unpacked[idx] = byte >> 4;
            unpacked[idx + 1] = byte & 0x0F;
            idx += 2;
        }
        Self::with_unpacked_sbox(&unpacked)
    }

    /// Initialize with an unpacked 128-byte S-box (one 4-bit value per byte).
    pub fn with_unpacked_sbox(sbox: &[u8; 128]) -> Self {
        // jkurwa naming (see boxinit code in gost89.js):
        //   k8 = sbox[0..16]   used as TOP nibble of TOP byte (positions 24-31)
        //   k7 = sbox[16..32]  bottom nibble of top byte
        //   k6 = sbox[32..48]
        //   k5 = sbox[48..64]
        //   k4 = sbox[64..80]
        //   k3 = sbox[80..96]
        //   k2 = sbox[96..112]
        //   k1 = sbox[112..128]
        let k8 = &sbox[0..16];
        let k7 = &sbox[16..32];
        let k6 = &sbox[32..48];
        let k5 = &sbox[48..64];
        let k4 = &sbox[64..80];
        let k3 = &sbox[80..96];
        let k2 = &sbox[96..112];
        let k1 = &sbox[112..128];

        let mut k87 = [0u32; 256];
        let mut k65 = [0u32; 256];
        let mut k43 = [0u32; 256];
        let mut k21 = [0u32; 256];

        for i in 0..256usize {
            // Port of jkurwa boxinit:
            //   k87[i] = (k8[i>>>4] << 4 | k7[i & 15]) << 24
            //   k65[i] = (k6[i>>>4] << 4 | k5[i & 15]) << 16
            //   k43[i] = (k4[i>>>4] << 4 | k3[i & 15]) << 8
            //   k21[i] = k2[i>>>4] << 4 | k1[i & 15]
            //
            // Each k_n[ii] is a single 4-bit S-box value (0..15) stored
            // in low nibble of byte. (k8[ii] << 4) | k7[jj] yields a byte
            // (high nibble = S-box 8 output, low = S-box 7). Shifted to
            // appropriate u32 position.
            let ii = i >> 4;
            let jj = i & 15;
            k87[i] = (((k8[ii] as u32) << 4) | (k7[jj] as u32)) << 24;
            k65[i] = (((k6[ii] as u32) << 4) | (k5[jj] as u32)) << 16;
            k43[i] = (((k4[ii] as u32) << 4) | (k3[jj] as u32)) << 8;
            k21[i] = ((k2[ii] as u32) << 4) | (k1[jj] as u32);
        }

        Gost {
            k: [0; 8],
            k87,
            k65,
            k43,
            k21,
        }
    }

    /// Set round key from 32 bytes (8 × u32 little-endian).
    /// Mirrors `gost.key(Key)` in jkurwa.
    pub fn set_key(&mut self, key: &[u8; 32]) {
        for i in 0..8 {
            let off = i * 4;
            self.k[i] = u32::from_le_bytes(key[off..off + 4].try_into().unwrap());
        }
    }

    /// The Feistel round function f(x + k).
    #[inline]
    pub(crate) fn pass(&self, x: u32) -> u32 {
        let y = self.k87[((x >> 24) & 0xFF) as usize]
            | self.k65[((x >> 16) & 0xFF) as usize]
            | self.k43[((x >> 8) & 0xFF) as usize]
            | self.k21[(x & 0xFF) as usize];
        // Rotate left 11 bits.
        y.rotate_left(11)
    }

    /// Encrypt 8 bytes in place. Output goes to `out`.
    /// Mirrors `gost.crypt64(clear, out)`.
    pub fn encrypt64(&self, clear: &[u8; 8], out: &mut [u8; 8]) {
        let mut n0 = u32::from_le_bytes(clear[0..4].try_into().unwrap());
        let mut n1 = u32::from_le_bytes(clear[4..8].try_into().unwrap());

        // 24 forward rounds (k[0..7] three times).
        for _ in 0..3 {
            for k_idx in 0..4 {
                n1 ^= self.pass(n0.wrapping_add(self.k[k_idx * 2]));
                n0 ^= self.pass(n1.wrapping_add(self.k[k_idx * 2 + 1]));
            }
        }

        // 8 reverse rounds (k[7..0]).
        for k_idx in (0..4).rev() {
            n1 ^= self.pass(n0.wrapping_add(self.k[k_idx * 2 + 1]));
            n0 ^= self.pass(n1.wrapping_add(self.k[k_idx * 2]));
        }

        // Output: swap halves (Feistel final swap).
        out[0..4].copy_from_slice(&n1.to_le_bytes());
        out[4..8].copy_from_slice(&n0.to_le_bytes());
    }

    /// Decrypt 8 bytes (single ECB block). Inverse of [`encrypt64`].
    ///
    /// GOST 28147 Feistel decryption uses the same round function with
    /// the round-key schedule reversed: 8 forward rounds (k[0..7]) then
    /// three reverse cycles (k[7..0] × 3). Mirrors `gost.decrypt64` in
    /// jkurwa's `gost89.js`.
    pub fn decrypt64(&self, crypt: &[u8; 8], out: &mut [u8; 8]) {
        let mut n0 = u32::from_le_bytes(crypt[0..4].try_into().unwrap());
        let mut n1 = u32::from_le_bytes(crypt[4..8].try_into().unwrap());

        // 8 forward rounds (k[0..7] once).
        for k_idx in 0..4 {
            n1 ^= self.pass(n0.wrapping_add(self.k[k_idx * 2]));
            n0 ^= self.pass(n1.wrapping_add(self.k[k_idx * 2 + 1]));
        }

        // 24 reverse rounds (k[7..0] three times).
        for _ in 0..3 {
            for k_idx in (0..4).rev() {
                n1 ^= self.pass(n0.wrapping_add(self.k[k_idx * 2 + 1]));
                n0 ^= self.pass(n1.wrapping_add(self.k[k_idx * 2]));
            }
        }

        out[0..4].copy_from_slice(&n1.to_le_bytes());
        out[4..8].copy_from_slice(&n0.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_sbox_shape() {
        let packed = pack_sbox(&DKU);
        // Last row of DKU (idx 112..128) should end up in first 8 bytes of packed.
        // DKU[112..116] = [0x0A, 0x09, 0x0D, 0x06]
        // packed[0] should be (DKU[112]<<4 | DKU[113]) = (0x0A<<4 | 0x09) = 0xA9
        assert_eq!(packed[0], 0xA9);
        assert_eq!(packed[1], 0xD6);
    }

    #[test]
    fn test_boxinit_known_values() {
        // Cross-check with empirical jkurwa output:
        //   k87[0] = 0x13000000  (k8[0]=DKU[0]=1, k7[0]=DKU[16]=3)
        //   k87[16] = 0x23000000 (k8[1]=DKU[1]=2, k7[0]=DKU[16]=3)
        //   k87[255] = 0x40000000 (k8[15]=DKU[15]=4, k7[15]=DKU[31]=0)
        let g = Gost::with_dku();
        assert_eq!(g.k87[0], 0x13000000);
        assert_eq!(g.k87[16], 0x23000000);
        assert_eq!(g.k87[255], 0x40000000);
    }

    #[test]
    fn test_gost_construction_doesnt_panic() {
        let _g = Gost::with_dku();
    }

    #[test]
    fn test_encrypt_with_zero_key_round_trip() {
        let mut g = Gost::with_dku();
        g.set_key(&[0u8; 32]);
        let clear = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut out = [0u8; 8];
        g.encrypt64(&clear, &mut out);
        // Just smoke: output is not the same as input.
        assert_ne!(clear, out);
    }

    /// Encrypt → decrypt must round-trip back to the original plaintext.
    /// Covers the new `decrypt64` schedule (8 forward + 24 reverse rounds).
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mut g = Gost::with_dku();
        let key = {
            let mut k = [0u8; 32];
            for i in 0..32 {
                k[i] = (i as u8).wrapping_mul(0x9D).wrapping_add(0x37);
            }
            k
        };
        g.set_key(&key);
        for plain_seed in [0u8, 0x42, 0xAA, 0xFF] {
            let plain = [
                plain_seed,
                plain_seed ^ 0x55,
                plain_seed.wrapping_add(1),
                plain_seed.wrapping_mul(3),
                0x12,
                0x34,
                0x56,
                0x78,
            ];
            let mut ct = [0u8; 8];
            let mut back = [0u8; 8];
            g.encrypt64(&plain, &mut ct);
            g.decrypt64(&ct, &mut back);
            assert_eq!(plain, back, "encrypt/decrypt roundtrip failed for {:?}", plain);
        }
    }

    /// `with_packed_sbox` must produce exactly the same Gost state as the
    /// default `with_dku`, since the canonical packed form of `DKU` is
    /// what `pack_sbox` produces.
    #[test]
    fn test_packed_sbox_matches_default() {
        let packed = pack_sbox(&DKU);
        let g_packed = Gost::with_packed_sbox(&packed);
        let g_default = Gost::with_dku();
        assert_eq!(g_packed.k87, g_default.k87, "k87 tables must match");
        assert_eq!(g_packed.k65, g_default.k65, "k65 tables must match");
        assert_eq!(g_packed.k43, g_default.k43, "k43 tables must match");
        assert_eq!(g_packed.k21, g_default.k21, "k21 tables must match");
    }
}
