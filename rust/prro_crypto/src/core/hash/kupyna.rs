//! Kupyna (DSTU 7564:2014) hash function.
//!
//! Ukrainian national hash standard. Supports 256-bit and 512-bit output.
//! Internally uses an AES-like permutation over an 8×8 or 8×16 byte state
//! with precomputed T-tables (combined SubBytes + ShiftRows + MixColumns).
//!
//! ## Performance
//!
//! T-table approach: 8 lookups + 7 XORs per column per round.
//! Kupyna-256: 10 rounds × 8 columns = 80 T-table lookups per permutation.
//! Compression = 2 permutations (P + Q) → ~160 lookups per 64-byte block.
//!
//! ## Source
//!
//! Ported from UAPKI (BSD-3-Clause), `dstu7564.c`.
//! T-tables from `dstu7624.c` (`subrowcol_default[8][256]`).

#[path = "kupyna_tables.rs"]
mod kupyna_tables;

use kupyna_tables::{Q_CONST_1024, Q_CONST_512, T};

// ── Constants ──────────────────────────────────────────────────────────

const ROWS: usize = 8;
const NB_512: usize = 8; // columns for ≤256-bit hash
const NB_1024: usize = 16; // columns for >256-bit hash
const NR_512: usize = 10; // rounds for 512-bit state
const NR_1024: usize = 14; // rounds for 1024-bit state
const STATE_512: usize = NB_512 * ROWS; // 64 bytes
const STATE_1024: usize = NB_1024 * ROWS; // 128 bytes

// ── T-table round function (combined SubBytes + ShiftRows + MixColumns) ─

/// Apply one T-table round column for 512-bit state (8 columns).
/// ShiftRows for NB=8: row i shifts by i positions.
#[inline(always)]
fn table_g_512(s: &[u64; NB_512], j: usize) -> u64 {
    T[0][(s[j] & 0xFF) as usize]
        ^ T[1][(s[(j + 7) & 7] >> 8 & 0xFF) as usize]
        ^ T[2][(s[(j + 6) & 7] >> 16 & 0xFF) as usize]
        ^ T[3][(s[(j + 5) & 7] >> 24 & 0xFF) as usize]
        ^ T[4][(s[(j + 4) & 7] >> 32 & 0xFF) as usize]
        ^ T[5][(s[(j + 3) & 7] >> 40 & 0xFF) as usize]
        ^ T[6][(s[(j + 2) & 7] >> 48 & 0xFF) as usize]
        ^ T[7][(s[(j + 1) & 7] >> 56 & 0xFF) as usize]
}

/// Apply one T-table round column for 1024-bit state (16 columns).
/// ShiftRows for NB=16: rows shift by [0,1,2,3,4,5,6,11].
#[inline(always)]
fn table_g_1024(s: &[u64; NB_1024], j: usize) -> u64 {
    T[0][(s[j] & 0xFF) as usize]
        ^ T[1][(s[(j + 15) & 15] >> 8 & 0xFF) as usize]
        ^ T[2][(s[(j + 14) & 15] >> 16 & 0xFF) as usize]
        ^ T[3][(s[(j + 13) & 15] >> 24 & 0xFF) as usize]
        ^ T[4][(s[(j + 12) & 15] >> 32 & 0xFF) as usize]
        ^ T[5][(s[(j + 11) & 15] >> 40 & 0xFF) as usize]
        ^ T[6][(s[(j + 10) & 15] >> 48 & 0xFF) as usize]
        ^ T[7][(s[(j + 5) & 15] >> 56 & 0xFF) as usize]
}

// ── P and Q permutations ───────────────────────────────────────────────

/// P-transformation round constant: `(col << 4) | round`.
#[inline(always)]
const fn p_const(round: usize, col: usize) -> u64 {
    ((col as u64) << 4) | (round as u64)
}

/// P-permutation for 512-bit state: 10 rounds with XOR-based constants.
fn p_512(state: &mut [u64; NB_512]) {
    let mut t = [0u64; NB_512];
    for round in 0..NR_512 {
        // AddConstant (XOR)
        for j in 0..NB_512 {
            state[j] ^= p_const(round, j);
        }
        // SubBytes + ShiftRows + MixColumns (T-table)
        for j in 0..NB_512 {
            t[j] = table_g_512(state, j);
        }
        *state = t;
    }
}

/// Q-permutation for 512-bit state: 10 rounds with ADD-based constants.
fn q_512(state: &mut [u64; NB_512]) {
    let mut t = [0u64; NB_512];
    for round in 0..NR_512 {
        // AddConstant (modular addition)
        for j in 0..NB_512 {
            state[j] = state[j].wrapping_add(Q_CONST_512[round][j]);
        }
        // SubBytes + ShiftRows + MixColumns
        for j in 0..NB_512 {
            t[j] = table_g_512(state, j);
        }
        *state = t;
    }
}

/// P-permutation for 1024-bit state: 14 rounds.
fn p_1024(state: &mut [u64; NB_1024]) {
    let mut t = [0u64; NB_1024];
    for round in 0..NR_1024 {
        for j in 0..NB_1024 {
            state[j] ^= p_const(round, j);
        }
        for j in 0..NB_1024 {
            t[j] = table_g_1024(state, j);
        }
        *state = t;
    }
}

/// Q-permutation for 1024-bit state: 14 rounds.
fn q_1024(state: &mut [u64; NB_1024]) {
    let mut t = [0u64; NB_1024];
    for round in 0..NR_1024 {
        for j in 0..NB_1024 {
            state[j] = state[j].wrapping_add(Q_CONST_1024[round][j]);
        }
        for j in 0..NB_1024 {
            t[j] = table_g_1024(state, j);
        }
        *state = t;
    }
}

// ── State helpers ──────────────────────────────────────────────────────

/// Load a byte slice into u64 array (little-endian).
#[inline]
fn bytes_to_u64s<const N: usize>(bytes: &[u8], out: &mut [u64; N]) {
    for i in 0..N {
        out[i] = u64::from_le_bytes([
            bytes[i * 8],
            bytes[i * 8 + 1],
            bytes[i * 8 + 2],
            bytes[i * 8 + 3],
            bytes[i * 8 + 4],
            bytes[i * 8 + 5],
            bytes[i * 8 + 6],
            bytes[i * 8 + 7],
        ]);
    }
}

/// Store u64 array to byte slice (little-endian).
#[inline]
fn u64s_to_bytes<const N: usize>(words: &[u64; N], out: &mut [u8]) {
    for i in 0..N {
        let b = words[i].to_le_bytes();
        out[i * 8..i * 8 + 8].copy_from_slice(&b);
    }
}

// ── Kupyna hasher ──────────────────────────────────────────────────────

/// Kupyna (DSTU 7564) incremental hasher.
///
/// Supports hash output sizes from 8 to 512 bits (1 to 64 bytes).
/// Standard sizes: 256-bit (32 bytes) and 512-bit (64 bytes).
pub struct Kupyna {
    /// Internal state as u64 words.
    /// For ≤256-bit: 8 words (512-bit state).
    /// For >256-bit: 16 words (1024-bit state).
    state: [u64; NB_1024],
    /// Number of active columns (8 or 16).
    nb: usize,
    /// Block size in bytes (64 or 128).
    block_size: usize,
    /// Desired output hash length in bytes.
    hash_len: usize,
    /// Partial block buffer. Sized for worst-case padding: two blocks
    /// (block_size * 2) when the partial block + 0x80 + 12-byte length
    /// overflows one block boundary.
    buf: [u8; STATE_1024 * 2],
    /// Bytes accumulated in `buf`.
    buf_len: usize,
    /// Total message length in bytes (128-bit counter for padding).
    msg_len: [u64; 2],
}

impl Kupyna {
    /// Create a new Kupyna hasher for the given output size in bytes.
    ///
    /// - `hash_len = 32` → Kupyna-256 (512-bit internal state, 10 rounds)
    /// - `hash_len = 48` → Kupyna-384 (1024-bit internal state, 14 rounds)
    /// - `hash_len = 64` → Kupyna-512 (1024-bit internal state, 14 rounds)
    pub fn new(hash_len: usize) -> Self {
        assert!(
            (1..=64).contains(&hash_len),
            "Kupyna hash_len must be 1..=64"
        );
        let (nb, block_size) = if hash_len <= 32 {
            (NB_512, STATE_512)
        } else {
            (NB_1024, STATE_1024)
        };
        let mut state = [0u64; NB_1024];
        // IV: first byte = block_size
        state[0] = block_size as u64;
        Kupyna {
            state,
            nb,
            block_size,
            hash_len,
            buf: [0u8; STATE_1024 * 2],
            buf_len: 0,
            msg_len: [0, 0],
        }
    }

    /// Kupyna-256 convenience constructor.
    pub fn new_256() -> Self {
        Self::new(32)
    }

    /// Kupyna-512 convenience constructor.
    pub fn new_512() -> Self {
        Self::new(64)
    }

    /// Feed data into the hasher.
    pub fn update(&mut self, data: &[u8]) {
        // Update message length counter (in bytes, 128-bit)
        let (new_lo, carry) = self.msg_len[0].overflowing_add(data.len() as u64);
        self.msg_len[0] = new_lo;
        if carry {
            self.msg_len[1] = self.msg_len[1].wrapping_add(1);
        }

        let mut offset = 0;

        // If we have buffered data, try to complete a block
        if self.buf_len > 0 {
            let need = self.block_size - self.buf_len;
            if data.len() < need {
                self.buf[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
                self.buf_len += data.len();
                return;
            }
            self.buf[self.buf_len..self.buf_len + need].copy_from_slice(&data[..need]);
            self.compress(&self.buf.clone());
            self.buf_len = 0;
            offset = need;
        }

        // Process full blocks
        while offset + self.block_size <= data.len() {
            self.compress(&data[offset..offset + self.block_size]);
            offset += self.block_size;
        }

        // Buffer remainder
        let remaining = data.len() - offset;
        if remaining > 0 {
            self.buf[..remaining].copy_from_slice(&data[offset..]);
            self.buf_len = remaining;
        }
    }

    /// Finalize and return the hash.
    pub fn finalize(mut self) -> Vec<u8> {
        // Padding
        self.pad();

        // Output transformation: state = state ⊕ P(state)
        if self.nb == NB_512 {
            let mut temp = [0u64; NB_512];
            temp.copy_from_slice(&self.state[..NB_512]);
            p_512(&mut temp);
            for j in 0..NB_512 {
                self.state[j] ^= temp[j];
            }
        } else {
            let mut temp = [0u64; NB_1024];
            temp.copy_from_slice(&self.state[..NB_1024]);
            p_1024(&mut temp);
            for j in 0..NB_1024 {
                self.state[j] ^= temp[j];
            }
        }

        // Extract hash from the tail of the state
        let mut state_bytes = vec![0u8; self.nb * 8];
        if self.nb == NB_512 {
            let s: &[u64; NB_512] = self.state[..NB_512].try_into().unwrap();
            u64s_to_bytes(s, &mut state_bytes);
        } else {
            u64s_to_bytes(&self.state, &mut state_bytes);
        }
        // Hash is the last `hash_len` bytes of the state
        let start = state_bytes.len() - self.hash_len;
        state_bytes[start..].to_vec()
    }

    /// Compression function: state = state ⊕ P(state ⊕ msg) ⊕ Q(msg).
    fn compress(&mut self, block: &[u8]) {
        if self.nb == NB_512 {
            self.compress_512(block);
        } else {
            self.compress_1024(block);
        }
    }

    fn compress_512(&mut self, block: &[u8]) {
        let mut m = [0u64; NB_512];
        bytes_to_u64s(block, &mut m);

        // temp1 = state ⊕ message
        let mut t1 = [0u64; NB_512];
        for j in 0..NB_512 {
            t1[j] = self.state[j] ^ m[j];
        }
        // temp2 = message
        let mut t2 = m;

        // P(temp1), Q(temp2)
        p_512(&mut t1);
        q_512(&mut t2);

        // state ^= temp1 ^ temp2
        for j in 0..NB_512 {
            self.state[j] ^= t1[j] ^ t2[j];
        }
    }

    fn compress_1024(&mut self, block: &[u8]) {
        let mut m = [0u64; NB_1024];
        bytes_to_u64s(block, &mut m);

        let mut t1 = [0u64; NB_1024];
        for j in 0..NB_1024 {
            t1[j] = self.state[j] ^ m[j];
        }
        let mut t2 = m;

        p_1024(&mut t1);
        q_1024(&mut t2);

        for j in 0..NB_1024 {
            self.state[j] ^= t1[j] ^ t2[j];
        }
    }

    /// Pad the remaining data and compress.
    fn pad(&mut self) {
        // Message length in bits (128-bit LE)
        let msg_bits_lo = self.msg_len[0] << 3;
        let msg_bits_hi = (self.msg_len[1] << 3) | (self.msg_len[0] >> 61);

        // Append 0x80
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;

        // Calculate zero padding needed.
        // We need: (buf_len + zero_pad + 12) mod block_size == 0
        // where 12 = 8 bytes (lo) + 4 bytes (hi) of the length field.
        let target = if self.buf_len + 12 > self.block_size {
            self.block_size * 2
        } else {
            self.block_size
        };
        let zero_pad = target - self.buf_len - 12;

        // Zero fill
        for i in 0..zero_pad {
            self.buf[self.buf_len + i] = 0;
        }
        self.buf_len += zero_pad;

        // Append 96-bit length (12 bytes LE): 8 bytes lo + 4 bytes hi
        let lo_bytes = msg_bits_lo.to_le_bytes();
        self.buf[self.buf_len..self.buf_len + 8].copy_from_slice(&lo_bytes);
        self.buf_len += 8;
        let hi_bytes = (msg_bits_hi as u32).to_le_bytes();
        self.buf[self.buf_len..self.buf_len + 4].copy_from_slice(&hi_bytes);
        self.buf_len += 4;

        // Compress all padded blocks.  The stack copy exists to end the
        // `self.buf` borrow before the `&mut self` compress call (the old
        // `.to_vec()` did the same with a heap allocation per block).
        let mut offset = 0;
        while offset < self.buf_len {
            let end = offset + self.block_size;
            let mut block = [0u8; STATE_1024];
            block[..self.block_size].copy_from_slice(&self.buf[offset..end]);
            self.compress(&block[..self.block_size]);
            offset = end;
        }
        self.buf_len = 0;
    }
}

// ── Convenience functions ──────────────────────────────────────────────

/// Compute Kupyna-256 hash of a byte slice.
pub fn kupyna_256(data: &[u8]) -> [u8; 32] {
    let mut h = Kupyna::new_256();
    h.update(data);
    let result = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Compute Kupyna-512 hash of a byte slice.
pub fn kupyna_512(data: &[u8]) -> [u8; 64] {
    let mut h = Kupyna::new_512();
    h.update(data);
    let result = h.finalize();
    let mut out = [0u8; 64];
    out.copy_from_slice(&result);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // M2 = 0x00..0xFF (256 bytes), used with various lengths
    fn m2() -> Vec<u8> {
        (0x00u8..=0xFF).collect()
    }

    fn h(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    // ── Kupyna-256: 6 official DSTU 7564 vectors from UAPKI ──

    #[test]
    fn k256_empty() {
        assert_eq!(
            &kupyna_256(b"")[..],
            &h("CD5101D1CCDF0D1D1F4ADA56E888CD724CA1A0838A3521E7131D4FB78D0F5EB6")[..]
        );
    }

    #[test]
    fn k256_8bit() {
        assert_eq!(
            &kupyna_256(&[0xFF])[..],
            &h("EA7677CA4526555680441C117982EA14059EA6D0D7124D6ECDB3DEEC49E890F4")[..]
        );
    }

    #[test]
    fn k256_512bit() {
        assert_eq!(
            &kupyna_256(&m2()[..64])[..],
            &h("08F4EE6F1BE6903B324C4E27990CB24EF69DD58DBE84813EE0A52F6631239875")[..]
        );
    }

    #[test]
    fn k256_760bit() {
        assert_eq!(
            &kupyna_256(&m2()[..95])[..],
            &h("1075C8B0CB910F116BDA5FA1F19C29CF8ECC75CAFF7208BA2994B68FC56E8D16")[..]
        );
    }

    #[test]
    fn k256_1024bit() {
        assert_eq!(
            &kupyna_256(&m2()[..128])[..],
            &h("0A9474E645A7D25E255E9E89FFF42EC7EB31349007059284F0B182E452BDA882")[..]
        );
    }

    #[test]
    fn k256_2048bit() {
        assert_eq!(
            &kupyna_256(&m2()[..256])[..],
            &h("D305A32B963D149DC765F68594505D4077024F836C1BF03806E1624CE176C08F")[..]
        );
    }

    // ── Kupyna-512: 6 official DSTU 7564 vectors from UAPKI ──

    #[test]
    fn k512_empty() {
        assert_eq!(&kupyna_512(b"")[..], &h("656B2F4CD71462388B64A37043EA55DBE445D452AECD46C3298343314EF04019BCFA3F04265A9857F91BE91FCE197096187CEDA78C9C1C021C294A0689198538")[..]);
    }

    #[test]
    fn k512_8bit() {
        assert_eq!(&kupyna_512(&[0xFF])[..], &h("871B18CF754B72740307A97B449ABEB32B64444CC0D5A4D65830AE5456837A72D8458F12C8F06C98C616ABE11897F86263B5CB77C420FB375374BEC52B6D0292")[..]);
    }

    #[test]
    fn k512_512bit() {
        assert_eq!(&kupyna_512(&m2()[..64])[..], &h("3813E2109118CDFB5A6D5E72F7208DCCC80A2DFB3AFDFB02F46992B5EDBE536B3560DD1D7E29C6F53978AF58B444E37BA685C0DD910533BA5D78EFFFC13DE62A")[..]);
    }

    #[test]
    fn k512_1024bit() {
        assert_eq!(&kupyna_512(&m2()[..128])[..], &h("76ED1AC28B1D0143013FFA87213B4090B356441263C13E03FA060A8CADA32B979635657F256B15D5FCA4A174DE029F0B1B4387C878FCC1C00E8705D783FD7FFE")[..]);
    }

    #[test]
    fn k512_1536bit() {
        assert_eq!(&kupyna_512(&m2()[..192])[..], &h("B189BFE987F682F5F167F0D7FA565330E126B6E592B1C55D44299064EF95B1A57F3C2D0ECF17869D1D199EBBD02E8857FB8ADD67A8C31F56CD82C016CF743121")[..]);
    }

    #[test]
    fn k512_2048bit() {
        assert_eq!(&kupyna_512(&m2()[..256])[..], &h("0DD03D7350C409CB3C29C25893A0724F6B133FA8B9EB90A64D1A8FA93B56556611EB187D715A956B107E3BFC76482298133A9CE8CBC0BD5E1436A5B197284F7E")[..]);
    }

    // ── Functional tests ──

    #[test]
    fn incremental_matches_single_shot() {
        let input: Vec<u8> = (0x00u8..=0x3F).collect();
        let single = kupyna_256(&input);
        let mut h = Kupyna::new_256();
        h.update(&input[..10]);
        h.update(&input[10..30]);
        h.update(&input[30..]);
        assert_eq!(&single[..], &h.finalize()[..]);
    }

    /// Kupyna-512 near block boundary — exercises two-block padding path.
    /// This would panic before the buf size fix (H2).
    #[test]
    fn k512_near_block_boundary() {
        // 128 + 120 = 248 bytes: last partial block = 120 bytes (> 128-12=116)
        let input: Vec<u8> = (0..248).map(|i| (i & 0xFF) as u8).collect();
        let h1 = kupyna_512(&input);
        let h2 = kupyna_512(&input);
        assert_eq!(&h1[..], &h2[..], "must be deterministic");
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn byte_at_a_time() {
        let input = m2();
        let expected = kupyna_256(&input[..128]);
        let mut h = Kupyna::new_256();
        for &b in &input[..128] {
            h.update(&[b]);
        }
        assert_eq!(&expected[..], &h.finalize()[..]);
    }
}
