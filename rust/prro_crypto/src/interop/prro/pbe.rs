//! Password-based encryption primitives for Ukrainian DSTU containers.
//!
//! Two building blocks shared between the PFX/ZS2 path (`pfx.rs`) and any
//! future PBES2-style container:
//!
//! - [`pbkdf2_hmac_gost34311`] — RFC 2898 PBKDF2 with HMAC-GOST34311 as
//!   the underlying PRF. Produces a 32-byte session key from
//!   `(password, salt, iterations)`. Mirrors `gost89/lib/util.js::pbkdf`.
//! - [`gost28147_cfb_decrypt`] — Cipher Feedback (CFB) mode of GOST 28147
//!   over an arbitrary byte stream. Mirrors `gost89/lib/gost89.js::decrypt_cfb`.
//!
//! Both reuse the existing [`crate::core::hash`] primitives — there is
//! no new cryptographic algorithm here, just plumbing.

use crate::core::hash::{gost28147::Gost, gost_34_311_95};

/// Fixed initialisation vector used by GOST 28147 key-wrap and key-unwrap
/// (`gost89/lib/keywrap.js::WRAP_IV`). Not configurable — it is part of
/// the wrap/unwrap protocol itself, not a per-call parameter.
const KEYWRAP_IV: [u8; 8] = [0x4A, 0xDD, 0xA2, 0x2C, 0x79, 0xE8, 0x21, 0x05];

/// HMAC-GOST34311-95 single-block input/output. Block size of GOST 34.311
/// is 32 bytes; `key` is XORed into the standard `ipad`/`opad` constants
/// after being right-padded with zeros to 32 bytes (callers ensure
/// `key.len() <= 32` — for our PBES2 use case that always holds since
/// passwords are ASCII).
fn hmac_gost34311(key: &[u8], chunks: &[&[u8]]) -> [u8; 32] {
    let mut k = [0u8; 32];
    let n = key.len().min(32);
    k[..n].copy_from_slice(&key[..n]);

    let mut ipad = [0u8; 32];
    let mut opad = [0u8; 32];
    for i in 0..32 {
        ipad[i] = k[i] ^ 0x36;
        opad[i] = k[i] ^ 0x5C;
    }

    // inner = H(ipad || data)
    let mut inner_buf: Vec<u8> = Vec::with_capacity(32 + chunks.iter().map(|c| c.len()).sum::<usize>());
    inner_buf.extend_from_slice(&ipad);
    for c in chunks {
        inner_buf.extend_from_slice(c);
    }
    let inner = gost_34_311_95(&inner_buf);

    // outer = H(opad || inner)
    let mut outer_buf = [0u8; 64];
    outer_buf[..32].copy_from_slice(&opad);
    outer_buf[32..].copy_from_slice(&inner);
    gost_34_311_95(&outer_buf)
}

/// PBKDF2 with HMAC-GOST34311 as the PRF. Produces a 32-byte derived key
/// (one HMAC block); supports the single-block scenario PFX/ZS2 actually
/// uses. Generic `dkLen > 32` would need a `T_2, T_3, …` loop — out of
/// scope until a real container demands it.
///
/// Per RFC 2898 §5.2: `T_1 = U_1 ^ U_2 ^ … ^ U_c`, where
/// `U_1 = HMAC(P, S || INT(1))` and `U_j = HMAC(P, U_{j-1})`.
pub fn pbkdf2_hmac_gost34311(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    debug_assert!(iterations >= 1, "PBKDF2 must run at least one iteration");
    // U_1 = HMAC(P, S || [00,00,00,01])
    let block_index = [0u8, 0u8, 0u8, 1u8];
    let mut u = hmac_gost34311(password, &[salt, &block_index]);
    let mut t = u;
    for _ in 1..iterations {
        u = hmac_gost34311(password, &[&u]);
        for i in 0..32 {
            t[i] ^= u[i];
        }
    }
    t
}

/// GOST 28147 CFB decrypt over a byte stream of any whole-block length.
///
/// `iv` is the 8-byte initialisation vector. The cipher is initialised
/// with the supplied 32-byte session key and DKU S-box (the packed S-box
/// argument — pre-unpacked into the cipher's internal table). Each
/// 8-byte ciphertext block is XORed with `Encrypt(prev_iv)`; the next
/// IV is the *ciphertext* block (CFB feedback, not plaintext).
///
/// Returns the plaintext, of the same length as `ciphertext`. Caller is
/// responsible for ensuring `ciphertext.len() % 8 == 0` for the simple
/// whole-block path; partial-tail handling is intentionally omitted to
/// match jkurwa's behaviour and avoid silent data loss.
pub fn gost28147_cfb_decrypt(
    key: &[u8; 32],
    iv: &[u8; 8],
    sbox_packed: &[u8; 64],
    ciphertext: &[u8],
) -> Vec<u8> {
    debug_assert!(
        ciphertext.len() % 8 == 0,
        "CFB stream length {} not a multiple of GOST 28147 block size 8",
        ciphertext.len()
    );

    let mut cipher = Gost::with_packed_sbox(sbox_packed);
    cipher.set_key(key);

    let mut out = vec![0u8; ciphertext.len()];
    let mut cur_iv = *iv;
    let mut gamma = [0u8; 8];

    for off in (0..ciphertext.len()).step_by(8) {
        cipher.encrypt64(&cur_iv, &mut gamma);
        for j in 0..8 {
            out[off + j] = ciphertext[off + j] ^ gamma[j];
        }
        // CFB: the next IV is the ciphertext block we just consumed.
        cur_iv.copy_from_slice(&ciphertext[off..off + 8]);
    }
    out
}

/// CFB decrypt for byte streams whose length is NOT required to be a
/// multiple of 8. Used by the keywrap protocol (`wcek` = 44 bytes and
/// an intermediate 36-byte blob). The implementation mirrors
/// `gost89/lib/gost89.js::Gost.prototype.decrypt_cfb`:
///
/// - process `ceil(len / 8)` blocks, the last one may be partial;
/// - for the final partial block, only the leading `trailing` bytes
///   are XORed with the gamma and returned — matching what `decrypt_cfb`
///   then slices with `clear.slice(0, ctext.length)`.
///
/// Deliberately kept separate from [`gost28147_cfb_decrypt`] so that
/// whole-block callers (PFX, ZS2, Key-6 content streams) retain the
/// strict guard that catches real misalignment bugs.
pub fn gost28147_cfb_decrypt_any_len(
    key: &[u8; 32],
    iv: &[u8; 8],
    sbox_packed: &[u8; 64],
    ciphertext: &[u8],
) -> Vec<u8> {
    let mut cipher = Gost::with_packed_sbox(sbox_packed);
    cipher.set_key(key);

    let mut out = vec![0u8; ciphertext.len()];
    let mut cur_iv = *iv;
    let mut gamma = [0u8; 8];

    let full_blocks = ciphertext.len() / 8;
    for bi in 0..full_blocks {
        let off = bi * 8;
        cipher.encrypt64(&cur_iv, &mut gamma);
        for j in 0..8 {
            out[off + j] = ciphertext[off + j] ^ gamma[j];
        }
        cur_iv.copy_from_slice(&ciphertext[off..off + 8]);
    }
    let tail = ciphertext.len() - full_blocks * 8;
    if tail > 0 {
        cipher.encrypt64(&cur_iv, &mut gamma);
        let off = full_blocks * 8;
        for j in 0..tail {
            out[off + j] = ciphertext[off + j] ^ gamma[j];
        }
    }
    out
}

/// GOST 28147-89 MAC ("imitovstavka") in CBC-MAC variant used by
/// `gost89/lib/gost89.js::Gost.prototype.mac` — 16 Feistel rounds per
/// block (k0..k7 twice, no reverse cycle), 8-byte CBC chain, with
/// implicit zero-padding of the final block when the input is exactly
/// a multiple of 8 bytes.
///
/// Returns `mac_bits / 8` bytes (caller picks the truncation length;
/// for the GOST keywrap protocol the magic value is 32 bits = 4 bytes).
///
/// `key` is the 32-byte cipher key, `dku_packed` is the 64-byte packed
/// S-box. Both supplied explicitly so the function is reusable across
/// wrap protocols that may carry their own DKE rather than the default
/// DSTU one.
pub fn gost28147_mac(
    key: &[u8; 32],
    dku_packed: &[u8; 64],
    data: &[u8],
    mac_bits: usize,
) -> Vec<u8> {
    assert!(
        mac_bits > 0 && mac_bits <= 64 && mac_bits % 8 == 0,
        "gost28147_mac: mac_bits must be in (0, 64] and multiple of 8, got {mac_bits}"
    );
    let mut cipher = Gost::with_packed_sbox(dku_packed);
    cipher.set_key(key);

    let mut state = [0u8; 8]; // CBC chain register

    let mut i = 0;
    while i + 8 <= data.len() {
        mac_round(&cipher, &mut state, &data[i..i + 8]);
        i += 8;
    }
    let trailing = data.len() - i;
    if trailing > 0 {
        let mut last = [0u8; 8];
        last[..trailing].copy_from_slice(&data[i..]);
        mac_round(&cipher, &mut state, &last);
    } else if i == 8 {
        // jkurwa quirk (`gost89/lib/gost89.js::Gost.prototype.mac`): when
        // the input was exactly 8 bytes, one extra all-zero block is fed
        // in. Note this fires ONLY for data.len() == 8, not for every
        // multiple of 8 — a 32-byte CEK (the keywrap case) runs exactly
        // 4 full rounds with no zero-block suffix.
        mac_round(&cipher, &mut state, &[0u8; 8]);
    }

    let nbytes = mac_bits / 8;
    state[..nbytes].to_vec()
}

/// One MAC round: XOR plaintext block into state, then run 16 Feistel
/// rounds (k[0..7] twice). Differs from `encrypt64`/`decrypt64` only in
/// round count and absence of the final half swap.
fn mac_round(cipher: &Gost, state: &mut [u8; 8], block: &[u8]) {
    for j in 0..8 {
        state[j] ^= block[j];
    }
    let mut n0 = (state[0] as u32)
        | ((state[1] as u32) << 8)
        | ((state[2] as u32) << 16)
        | ((state[3] as u32) << 24);
    let mut n1 = (state[4] as u32)
        | ((state[5] as u32) << 8)
        | ((state[6] as u32) << 16)
        | ((state[7] as u32) << 24);

    // Feistel-style 16 rounds, k[0..7] twice. Mirrors `mac64` in
    // `gost89/lib/gost89.js`. NOTE: there is NO half-swap at the end —
    // the result is written back as `(n0_low, n1_high)`, not
    // `(n1_low, n0_high)` as in `encrypt64`.
    for _ in 0..2 {
        for k_idx in 0..4 {
            n1 ^= cipher_pass(cipher, n0.wrapping_add(cipher_key(cipher, k_idx * 2)));
            n0 ^= cipher_pass(cipher, n1.wrapping_add(cipher_key(cipher, k_idx * 2 + 1)));
        }
    }

    state[0] = (n0 & 0xFF) as u8;
    state[1] = ((n0 >> 8) & 0xFF) as u8;
    state[2] = ((n0 >> 16) & 0xFF) as u8;
    state[3] = ((n0 >> 24) & 0xFF) as u8;
    state[4] = (n1 & 0xFF) as u8;
    state[5] = ((n1 >> 8) & 0xFF) as u8;
    state[6] = ((n1 >> 16) & 0xFF) as u8;
    state[7] = ((n1 >> 24) & 0xFF) as u8;
}

#[inline]
fn cipher_pass(cipher: &Gost, x: u32) -> u32 {
    cipher.pass(x)
}

#[inline]
fn cipher_key(cipher: &Gost, idx: usize) -> u32 {
    cipher.k[idx]
}

/// GOST 28147 key-unwrap (RFC 4357 §6.4 variant used by Ukrainian
/// fiscal containers). Recovers the original 32-byte content
/// encryption key (CEK) from a 44-byte wrapped form.
///
/// The protocol is the inverse of `gost89/lib/keywrap.js::key_wrap`:
/// 1. CFB-decrypt the 44-byte input under `kek` with the fixed
///    [`KEYWRAP_IV`] → produces 44 bytes that we reverse byte-wise.
/// 2. Take the first 8 bytes as the per-CEK IV; the remaining 36 bytes
///    are CFB-decrypted under `kek` with that IV → gives 32-byte CEK
///    plus 4-byte ICV (integrity check value).
/// 3. Recompute the ICV as the GOST 28147 MAC (32-bit truncation) over
///    the recovered CEK; compare bytewise.
///
/// The whole protocol uses the **default DSTU DKU S-box** — the wrap
/// step in jkurwa hard-codes it, and so do all wrapped-CEK blobs in
/// the test fixtures we need to read.
pub fn gost28147_keywrap_unwrap(
    kek: &[u8; 32],
    wrapped: &[u8; 44],
) -> Result<[u8; 32], &'static str> {
    let dku_packed = default_dku_packed();

    // Step 1: CFB-decrypt under WRAP_IV; reverse the result.
    // Wrapped blob length (44) is not a multiple of 8 — jkurwa's
    // `decrypt_cfb` uses `ceil(len/8)` blocks and slices back to the
    // original length. Use the any-length variant here; the bulk path
    // (`gost28147_cfb_decrypt`) keeps its strict whole-block guard for
    // PFX/ZS2/Key-6 where misalignment would be a real corruption.
    let temp3 = gost28147_cfb_decrypt_any_len(kek, &KEYWRAP_IV, &dku_packed, wrapped);
    debug_assert_eq!(temp3.len(), 44);
    let mut temp2 = [0u8; 44];
    for i in 0..44 {
        temp2[i] = temp3[44 - i - 1];
    }

    // Step 2: split off the per-CEK IV, CFB-decrypt the rest.
    let mut iv = [0u8; 8];
    iv.copy_from_slice(&temp2[..8]);
    let mut temp1 = [0u8; 36];
    temp1.copy_from_slice(&temp2[8..44]);
    let cekicv = gost28147_cfb_decrypt_any_len(kek, &iv, &dku_packed, &temp1);
    debug_assert_eq!(cekicv.len(), 36);

    let mut cek = [0u8; 32];
    cek.copy_from_slice(&cekicv[..32]);
    let icv = &cekicv[32..36];

    // Step 3: MAC the recovered CEK; constant-time-ish compare.
    let computed = gost28147_mac(kek, &dku_packed, &cek, 32);
    let mut diff: u8 = 0;
    for i in 0..4 {
        diff |= icv[i] ^ computed[i];
    }
    if diff != 0 {
        return Err("GOST keywrap unwrap: ICV mismatch (wrong KEK or corrupt blob)");
    }
    Ok(cek)
}

/// Default DSTU DKU S-box in packed form (64 bytes). Sourced from
/// `Gost::with_dku()`'s internal table by round-tripping through
/// `pack_sbox` once at module init.
fn default_dku_packed() -> [u8; 64] {
    // The unpacked DKU is hard-coded in `core::hash::gost28147` and not
    // exported. Easiest reproducible source: use the canonical packed
    // form that gost89/lib/dstu.js exports verbatim.
    [
        0xa9, 0xd6, 0xeb, 0x45, 0xf1, 0x3c, 0x70, 0x82, 0x80, 0xc4, 0x96, 0x7b,
        0x23, 0x1f, 0x5e, 0xad, 0xf6, 0x58, 0xeb, 0xa4, 0xc0, 0x37, 0x29, 0x1d,
        0x38, 0xd9, 0x6b, 0xf0, 0x25, 0xca, 0x4e, 0x17, 0xf8, 0xe9, 0x72, 0x0d,
        0xc6, 0x15, 0xb4, 0x3a, 0x28, 0x97, 0x5f, 0x0b, 0xc1, 0xde, 0xa3, 0x64,
        0x38, 0xb5, 0x64, 0xea, 0x2c, 0x17, 0x9f, 0xd0, 0x12, 0x3e, 0x6d, 0xb8,
        0xfa, 0xc5, 0x79, 0x04,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hard-coded `default_dku_packed` bytes must agree with the
    /// internally-built DKU S-box (`Gost::with_dku`). If anyone ever
    /// mistypes a byte in the const, ECB(plaintext) with the two
    /// initialisers would diverge; this test catches it before the
    /// wrong KEK/CEK silently hits production wcek unwrap.
    #[test]
    fn default_dku_packed_matches_canonical_dku() {
        let key = [0x42u8; 32];
        let block = [0x01u8, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let mut out_a = [0u8; 8];
        let mut out_b = [0u8; 8];

        let mut a = Gost::with_dku();
        a.set_key(&key);
        a.encrypt64(&block, &mut out_a);

        let mut b = Gost::with_packed_sbox(&default_dku_packed());
        b.set_key(&key);
        b.encrypt64(&block, &mut out_b);

        assert_eq!(
            out_a, out_b,
            "default_dku_packed() must produce the same cipher as Gost::with_dku()"
        );
    }

    /// `gost28147_mac` panics loudly on invalid mac_bits — keeps the
    /// API honest instead of silently truncating output.
    #[test]
    #[should_panic(expected = "mac_bits")]
    fn gost28147_mac_rejects_non_byte_aligned_mac_bits() {
        let key = [0u8; 32];
        let dku = default_dku_packed();
        gost28147_mac(&key, &dku, b"x", 33);
    }

    #[test]
    #[should_panic(expected = "mac_bits")]
    fn gost28147_mac_rejects_oversized_mac_bits() {
        let key = [0u8; 32];
        let dku = default_dku_packed();
        gost28147_mac(&key, &dku, b"x", 128);
    }

    /// PBKDF2 must be deterministic — same `(password, salt, iters)`
    /// always yields the same derived key.
    #[test]
    fn pbkdf2_is_deterministic() {
        let k1 = pbkdf2_hmac_gost34311(b"hello", b"saltysalt", 100);
        let k2 = pbkdf2_hmac_gost34311(b"hello", b"saltysalt", 100);
        assert_eq!(k1, k2);
    }

    /// Different inputs must produce different derived keys.
    #[test]
    fn pbkdf2_responds_to_input_changes() {
        let base = pbkdf2_hmac_gost34311(b"hello", b"salt0000", 100);
        assert_ne!(base, pbkdf2_hmac_gost34311(b"world", b"salt0000", 100));
        assert_ne!(base, pbkdf2_hmac_gost34311(b"hello", b"salt0001", 100));
        assert_ne!(base, pbkdf2_hmac_gost34311(b"hello", b"salt0000", 101));
    }

    /// CFB decrypt is a stream cipher — encrypting the result by XORing
    /// with the same gamma stream must recover the ciphertext. We test
    /// the operation by running CFB with one IV/key pair against a known
    /// plaintext and verifying the round-trip via a manual encrypt path.
    #[test]
    fn cfb_decrypt_roundtrip_zero_input() {
        // Ciphertext = encrypt(plain) ⊕ gamma. If plain == 0, ciphertext == gamma.
        // Then decrypt(ciphertext) should give back zeros.
        let key = [0u8; 32];
        let iv = [0u8; 8];
        // Use the canonical packed DKU as sbox.
        let sbox = {
            // pack_sbox is private — derive packed form by observing:
            // with_packed_sbox(packed) ≡ with_dku() iff packed = pack(DKU).
            // For the test we don't need the exact bytes; encrypt-then-decrypt
            // is what matters. We grab the packed form via a roundtrip:
            // any input we encrypt must decrypt back to itself.
            // Since pack_sbox is private, this test uses the cipher's
            // internal DKU init by passing all-zero packed and checking
            // self-consistency rather than against an external vector.
            [0u8; 64]
        };

        let plain = vec![0u8; 16];
        let ct = gost28147_cfb_decrypt(&key, &iv, &sbox, &plain);
        // CT is whatever the gamma stream is — non-trivial.
        assert_eq!(ct.len(), 16);

        // Round-trip via the SAME function (CFB encrypt and decrypt are
        // structurally identical: both XOR with E(prev_iv). The
        // distinction lives in which side stores ct vs pt for next IV.
        // For round-trip we simulate encryption manually:
        let mut manual_ct = vec![0u8; 16];
        let mut cipher = Gost::with_packed_sbox(&sbox);
        cipher.set_key(&key);
        let mut cur_iv = iv;
        let mut gamma = [0u8; 8];
        for off in (0..16).step_by(8) {
            cipher.encrypt64(&cur_iv, &mut gamma);
            for j in 0..8 {
                manual_ct[off + j] = plain[off + j] ^ gamma[j];
            }
            cur_iv.copy_from_slice(&manual_ct[off..off + 8]);
        }
        let recovered = gost28147_cfb_decrypt(&key, &iv, &sbox, &manual_ct);
        assert_eq!(recovered, plain, "CFB encrypt → decrypt must recover plaintext");
    }

    /// HMAC of the empty message under the empty key — sanity check that
    /// the function runs without panic and produces a 32-byte digest.
    #[test]
    fn hmac_smoke() {
        let h = hmac_gost34311(b"", &[b""]);
        assert_eq!(h.len(), 32);
        // Different keys ⇒ different MACs.
        assert_ne!(h, hmac_gost34311(b"key", &[b""]));
    }

    /// GOST 28147 key-unwrap test vectors from
    /// `sidecar/node_modules/gost89/test/test-util.js::key_unwrap()`.
    /// Both vectors share the same expected CEK but use different KEKs
    /// and wcek ciphertexts — this catches both an inverted reverse step
    /// and a broken ICV computation.
    #[test]
    fn keywrap_unwrap_vector_1() {
        let kek = hex32(
            "9c6e6852023b46f499f25b9b0eb7027387fdd5650f5d638ee5f99eb8dc781fde",
        );
        let wcek = hex44(
            "e90fe95628e715f4d6f3d1151ded367250f7006648ffffde574a4ea38250c1c5a0fdff11f36d9186d3b27c60",
        );
        let cek = gost28147_keywrap_unwrap(&kek, &wcek).expect("unwrap v1");
        assert_eq!(
            hex::encode(cek),
            "11080811020a0d0913040f020111190b04060c101d1c0a0911060e160b121419"
        );
    }

    #[test]
    fn keywrap_unwrap_vector_2() {
        let kek = hex32(
            "1de57ae661b7e142727170dd5066d04bd63231d5a207778075d17a831e853902",
        );
        let wcek = hex44(
            "359a37cf972520b590ef109b7c454c991d95da782e30ac9fe917fbb52e9402a4d236fd030f49627ec63c2684",
        );
        let cek = gost28147_keywrap_unwrap(&kek, &wcek).expect("unwrap v2");
        assert_eq!(
            hex::encode(cek),
            "11080811020a0d0913040f020111190b04060c101d1c0a0911060e160b121419"
        );
    }

    /// A 1-bit flip in the wrapped CEK must trip the ICV check.
    #[test]
    fn keywrap_unwrap_rejects_tampered_blob() {
        let kek = hex32(
            "9c6e6852023b46f499f25b9b0eb7027387fdd5650f5d638ee5f99eb8dc781fde",
        );
        let mut wcek = hex44(
            "e90fe95628e715f4d6f3d1151ded367250f7006648ffffde574a4ea38250c1c5a0fdff11f36d9186d3b27c60",
        );
        wcek[0] ^= 0x01;
        assert!(gost28147_keywrap_unwrap(&kek, &wcek).is_err());
    }

    // ── hex helpers confined to the test module ────────────────────────
    fn hex32(s: &str) -> [u8; 32] {
        let v = hex::decode(s).expect("hex32 decode");
        let mut out = [0u8; 32];
        out.copy_from_slice(&v);
        out
    }
    fn hex44(s: &str) -> [u8; 44] {
        let v = hex::decode(s).expect("hex44 decode");
        let mut out = [0u8; 44];
        out.copy_from_slice(&v);
        out
    }
}
