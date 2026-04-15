//! GOST 34.311-95 hash function.
//!
//! 256-bit output. Uses GOST 28147-89 as underlying block cipher
//! (with DSTU DKU S-box for Ukrainian deployment).
//!
//! Port of `jkurwa/node_modules/gost89/lib/hash.js`.
//! Byte-identical output vs jkurwa is enforced by 21-vector test suite.

use crate::hash::gost28147::Gost;

/// Hasher state.
pub struct Gost3411 {
    gost: Gost,
    /// H: chaining variable, 32 bytes.
    h: [u8; 32],
    /// S: checksum accumulator, 32 bytes.
    s: [u8; 32],
    /// Partial last block (up to 31 bytes).
    left: Vec<u8>,
    /// Total byte length hashed.
    len: u64,
}

impl Gost3411 {
    pub fn new() -> Self {
        Gost3411 {
            gost: Gost::with_dku(),
            h: [0; 32],
            s: [0; 32],
            left: Vec::new(),
            len: 0,
        }
    }

    /// Feed data into the hasher.
    pub fn update(&mut self, data: &[u8]) {
        // Combine any leftover partial block with new data.
        let combined: Vec<u8>;
        let input: &[u8] = if self.left.is_empty() {
            data
        } else {
            combined = [self.left.as_slice(), data].concat();
            self.left.clear();
            combined.as_slice()
        };

        // Process 32-byte blocks.
        let mut off = 0;
        while input.len() - off >= 32 {
            let block: [u8; 32] = input[off..off + 32].try_into().unwrap();
            self.step(&block);
            add_blocks(&mut self.s, &block);
            off += 32;
        }
        self.len += off as u64;

        // Save remainder (up to 31 bytes) for next call / finish.
        if off < input.len() {
            self.left = input[off..].to_vec();
        }
    }

    /// Finalize: return 32-byte hash.
    pub fn finish(mut self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        let mut fin_len = self.len;

        if !self.left.is_empty() {
            // Copy leftover into zero-padded buffer.
            for (i, &b) in self.left.iter().enumerate() {
                buf[i] = b;
            }
            self.step(&buf);
            add_blocks(&mut self.s, &buf);
            fin_len += self.left.len() as u64;
            self.left.clear();
            buf.fill(0);
        }

        // Encode length in bits (little-endian) into buf.
        fin_len <<= 3;
        let mut idx = 0;
        while fin_len > 0 {
            buf[idx] = (fin_len & 0xFF) as u8;
            fin_len >>= 8;
            idx += 1;
        }

        // Two final steps: with length block, then with checksum.
        self.step(&buf);
        let s_copy = self.s;
        self.step(&s_copy);

        self.h
    }

    /// Core compression function: H <- Hash(H, M).
    fn step(&mut self, m: &[u8; 32]) {
        let h_in = self.h;
        let mut u = [0u8; 32];
        let mut v = [0u8; 32];
        let mut w = [0u8; 32];
        let mut s_out = [0u8; 32];
        let mut key = [0u8; 32];

        // ─── Round 1: key = swap_bytes(H XOR M); encrypt H[0..8] → S[0..8] ──
        xor_blocks(&mut w, &h_in, m);
        swap_bytes(&w, &mut key);
        self.gost.set_key(&key);
        {
            let h_slice: [u8; 8] = h_in[0..8].try_into().unwrap();
            let mut out = [0u8; 8];
            self.gost.encrypt64(&h_slice, &mut out);
            s_out[0..8].copy_from_slice(&out);
        }

        // ─── Round 2: key = swap(circle_xor8(H) XOR circle_xor8(circle_xor8(M))) ──
        circle_xor8(&h_in, &mut u);
        circle_xor8(m, &mut v);
        {
            let v_copy = v;
            circle_xor8(&v_copy, &mut v);
        }
        xor_blocks(&mut w, &u, &v);
        swap_bytes(&w, &mut key);
        self.gost.set_key(&key);
        {
            let h_slice: [u8; 8] = h_in[8..16].try_into().unwrap();
            let mut out = [0u8; 8];
            self.gost.encrypt64(&h_slice, &mut out);
            s_out[8..16].copy_from_slice(&out);
        }

        // ─── Round 3: apply C3 constant to U, then same formula ──
        {
            let u_copy = u;
            circle_xor8(&u_copy, &mut u);
        }
        // C3 negation pattern (jkurwa lines 123-126):
        let c3_flips: [usize; 16] = [
            31, 29, 28, 24, 23, 20, 18, 17, 14, 12, 10, 8, 7, 5, 3, 1,
        ];
        for &i in &c3_flips {
            u[i] = !u[i];
        }
        {
            let v_copy = v;
            circle_xor8(&v_copy, &mut v);
        }
        {
            let v_copy = v;
            circle_xor8(&v_copy, &mut v);
        }
        xor_blocks(&mut w, &u, &v);
        swap_bytes(&w, &mut key);
        self.gost.set_key(&key);
        {
            let h_slice: [u8; 8] = h_in[16..24].try_into().unwrap();
            let mut out = [0u8; 8];
            self.gost.encrypt64(&h_slice, &mut out);
            s_out[16..24].copy_from_slice(&out);
        }

        // ─── Round 4 ─────────────────────────────────────────────────────
        {
            let u_copy = u;
            circle_xor8(&u_copy, &mut u);
        }
        {
            let v_copy = v;
            circle_xor8(&v_copy, &mut v);
        }
        {
            let v_copy = v;
            circle_xor8(&v_copy, &mut v);
        }
        xor_blocks(&mut w, &u, &v);
        swap_bytes(&w, &mut key);
        self.gost.set_key(&key);
        {
            let h_slice: [u8; 8] = h_in[24..32].try_into().unwrap();
            let mut out = [0u8; 8];
            self.gost.encrypt64(&h_slice, &mut out);
            s_out[24..32].copy_from_slice(&out);
        }

        // ─── Mixing: 12× transform_3, XOR M, 1× transform_3, XOR H, 61× transform_3 ──
        for _ in 0..12 {
            transform_3(&mut s_out);
        }
        {
            let s_copy = s_out;
            xor_blocks(&mut s_out, &s_copy, m);
        }
        transform_3(&mut s_out);
        {
            let s_copy = s_out;
            xor_blocks(&mut s_out, &s_copy, &h_in);
        }
        for _ in 0..61 {
            transform_3(&mut s_out);
        }

        self.h = s_out;
    }
}

impl Default for Gost3411 {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot hash.
pub fn gost_34_311_95(data: &[u8]) -> [u8; 32] {
    let mut h = Gost3411::new();
    h.update(data);
    h.finish()
}

// ─── Helpers (port of jkurwa's byte manipulations) ─────────────────────────

/// XOR two 32-byte blocks.
#[inline]
fn xor_blocks(out: &mut [u8; 32], a: &[u8; 32], b: &[u8; 32]) {
    for i in 0..32 {
        out[i] = a[i] ^ b[i];
    }
}

/// swap_bytes: rearrange 32 bytes per jkurwa Hash.swap_bytes.
/// `k[i+4*j] = w[8*i+j]` for i in 0..4, j in 0..8.
#[inline]
fn swap_bytes(w: &[u8; 32], k: &mut [u8; 32]) {
    for i in 0..4 {
        for j in 0..8 {
            k[i + 4 * j] = w[8 * i + j];
        }
    }
}

/// circle_xor8: k[0..24] = w[8..32]; k[24..32] = w[0..8] XOR k[0..8].
#[inline]
fn circle_xor8(w: &[u8; 32], k: &mut [u8; 32]) {
    // First copy w[8..32] to k[0..24].
    k[0..24].copy_from_slice(&w[8..32]);
    // Then k[24..32] = w[0..8] XOR k[0..8] (which came from w[8..16]).
    for i in 0..8 {
        k[i + 24] = w[i] ^ k[i];
    }
}

/// transform_3: polynomial-style byte shuffle (jkurwa Hash.transform_3).
///
/// Compute t16 (16-bit value), shift data[2..32] down by 2, put t16 at top.
#[inline]
fn transform_3(data: &mut [u8; 32]) {
    let t16_lo = data[0] ^ data[2] ^ data[4] ^ data[6] ^ data[24] ^ data[30];
    let t16_hi = data[1] ^ data[3] ^ data[5] ^ data[7] ^ data[25] ^ data[31];

    for i in 0..30 {
        data[i] = data[i + 2];
    }
    data[30] = t16_lo;
    data[31] = t16_hi;
}

/// add_blocks: add two 32-byte little-endian big-integers byte-wise with carry.
/// Mirrors jkurwa's Hash.add_blocks (modular add mod 2^256).
#[inline]
fn add_blocks(left: &mut [u8; 32], right: &[u8; 32]) {
    let mut carry: u32 = 0;
    for i in 0..32 {
        let sum = (left[i] as u32) + (right[i] as u32) + carry;
        left[i] = (sum & 0xFF) as u8;
        carry = sum >> 8;
    }
    // Final carry is discarded (mod 2^256).
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input_reference() {
        // Byte-identical expected from jkurwa:
        //   hash('') = da37bdf41145e39e34111775b40646e8059c2e969c1460bb98abccb26f0f76a5
        let h = gost_34_311_95(&[]);
        let expected =
            hex_literal("da37bdf41145e39e34111775b40646e8059c2e969c1460bb98abccb26f0f76a5");
        assert_eq!(h, expected);
    }

    #[test]
    fn test_one_byte_zero() {
        //   hash([0]) = 267a02fc973147e189849143b52c606cdce3392f3d614f3eb93d0e6b820e09aa
        let h = gost_34_311_95(&[0]);
        let expected =
            hex_literal("267a02fc973147e189849143b52c606cdce3392f3d614f3eb93d0e6b820e09aa");
        assert_eq!(h, expected);
    }

    fn hex_literal(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..32 {
            let byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
            out[i] = byte;
        }
        out
    }
}
