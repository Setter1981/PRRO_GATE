//! Java KeyStore (JKS) reader for DSTU 4145 keys.
//!
//! Supports the legacy JKS format (Sun's old keystore format) used by Ukrainian
//! certification authorities and ДПС test keys. JKS uses a custom SHA1-based
//! keystream cipher to encrypt the wrapped private key blob.
//!
//! Port of the JKS-reading logic from `sidecar/server.js::decryptJksKey`.
//!
//! ## Format
//! ```text
//! 4 bytes   magic = 0xFEEDFEED
//! 4 bytes   version = 2
//! 4 bytes   entry count
//! For each entry:
//!   4 bytes  tag (1 = private key, 2 = trusted cert)
//!   2 bytes  alias length
//!   N bytes  alias (UTF-8)
//!   8 bytes  creation time
//!   For private key entries:
//!     4 bytes  key length
//!     N bytes  encrypted key blob (DER-wrapped EncryptedPrivateKeyInfo)
//!     4 bytes  certificate count
//!     For each cert:
//!       2 bytes  cert type len
//!       N bytes  cert type (e.g. "X.509")
//!       4 bytes  cert DER length
//!       N bytes  cert DER
//! ```
//!
//! The encrypted key blob (after stripping the DER header to find the OCTET STRING)
//! contains: `salt(20) || encrypted(N) || hash_check(20)`. SHA1 keystream:
//! `ks_0 = SHA1(password_utf16be || salt)`, `ks_i = SHA1(password_utf16be || ks_{i-1})`,
//! XOR'd into `encrypted`. Final integrity: `SHA1(password_utf16be || decrypted) == hash_check`.

use sha1::{Digest, Sha1};

#[derive(Debug, thiserror::Error)]
pub enum JksError {
    #[error("not a JKS file (bad magic)")]
    BadMagic,
    #[error("entry is not a private key (tag={0})")]
    NotPrivateKey(u32),
    #[error("integrity check failed — wrong password")]
    BadPassword,
    #[error("truncated input at offset {0}")]
    Truncated(usize),
}

/// Result of reading a JKS keystore: the decrypted private key DER blob plus
/// the chain of certificates stored in the entry.
#[derive(Debug, Clone)]
pub struct JksEntry {
    pub alias: String,
    /// DER-encoded EncryptedPrivateKeyInfo (or wrapped DSTU privkey).
    pub key_der: Vec<u8>,
    /// Certificates in DER form, in stored order.
    pub certs: Vec<Vec<u8>>,
}

/// Read a single private-key entry from a JKS file.
///
/// Mirrors `decryptJksKey` from sidecar/server.js: assumes one entry of tag=1
/// (private key). Multi-entry keystores would need a richer API; for the PRRO
/// pilot we always have one signing key per JKS.
pub fn read_jks(data: &[u8], password: &str) -> Result<JksEntry, JksError> {
    if data.len() < 12 {
        return Err(JksError::Truncated(0));
    }
    let magic = read_u32_be(data, 0);
    if magic != 0xFEED_FEED {
        return Err(JksError::BadMagic);
    }
    // Skip version (4) + entry count (4) — we read first entry only.
    let mut offset = 12usize;

    let tag = read_u32_be(data, offset);
    offset += 4;
    if tag != 1 {
        return Err(JksError::NotPrivateKey(tag));
    }

    let alias_len = read_u16_be(data, offset) as usize;
    offset += 2;
    let alias = String::from_utf8_lossy(&data[offset..offset + alias_len]).into_owned();
    offset += alias_len;
    // Skip 8 bytes creation time
    offset += 8;

    let pk_len = read_u32_be(data, offset) as usize;
    offset += 4;
    let pk_blob = &data[offset..offset + pk_len];
    offset += pk_len;

    // pk_blob is an EncryptedPrivateKeyInfo SEQUENCE. The encrypted OCTET STRING
    // sits at byte 22 (after the algorithm identifier wrapping). The sidecar
    // hardcodes this offset because all JKS entries share the same algorithm.
    if pk_blob.len() < 24 {
        return Err(JksError::Truncated(offset));
    }
    let enc_octet_len = read_u16_be(pk_blob, 22) as usize;
    if pk_blob.len() < 24 + enc_octet_len {
        return Err(JksError::Truncated(offset));
    }
    let encrypted = &pk_blob[24..24 + enc_octet_len];
    if encrypted.len() < 40 {
        return Err(JksError::Truncated(offset));
    }
    let salt = &encrypted[..20];
    let enc_data = &encrypted[20..encrypted.len() - 20];
    let check_hash = &encrypted[encrypted.len() - 20..];

    // Build UTF-16BE password buffer (matching JKS spec).
    let mut pwd_buf: Vec<u8> = Vec::with_capacity(password.encode_utf16().count() * 2);
    for c in password.encode_utf16() {
        pwd_buf.push((c >> 8) as u8);
        pwd_buf.push((c & 0xFF) as u8);
    }

    // SHA1 keystream XOR.
    let mut result = vec![0u8; enc_data.len()];
    let mut counter: Vec<u8> = salt.to_vec();
    let mut xo = 0usize;
    while xo < enc_data.len() {
        let mut h = Sha1::new();
        h.update(&pwd_buf);
        h.update(&counter);
        let ks = h.finalize();
        for i in 0..ks.len() {
            if xo >= enc_data.len() {
                break;
            }
            result[xo] = enc_data[xo] ^ ks[i];
            xo += 1;
        }
        counter = ks.to_vec();
    }

    // Integrity check: SHA1(pwd || decrypted) == check_hash
    let mut int_check = Sha1::new();
    int_check.update(&pwd_buf);
    int_check.update(&result);
    let computed = int_check.finalize();
    if computed.as_slice() != check_hash {
        return Err(JksError::BadPassword);
    }

    // Read certificate chain.
    let cert_count = read_u32_be(data, offset) as usize;
    offset += 4;
    let mut certs: Vec<Vec<u8>> = Vec::with_capacity(cert_count);
    for _ in 0..cert_count {
        let ct_len = read_u16_be(data, offset) as usize;
        offset += 2;
        offset += ct_len; // skip cert type string
        let c_len = read_u32_be(data, offset) as usize;
        offset += 4;
        certs.push(data[offset..offset + c_len].to_vec());
        offset += c_len;
    }

    Ok(JksEntry {
        alias,
        key_der: result,
        certs,
    })
}

#[inline]
fn read_u16_be(buf: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}

#[inline]
fn read_u32_be(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live test against the test JKS shipped with PRRO_GATE.
    /// Skipped if the file isn't where we expect it.
    #[test]
    fn test_read_real_jks() {
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("skipping: test JKS not at {}", path);
                return;
            }
        };
        let entry = read_jks(&data, "Jrcfyf123").expect("real JKS must read");
        assert!(!entry.alias.is_empty(), "alias should be populated");
        assert!(!entry.key_der.is_empty(), "decrypted key must have content");
        assert!(
            !entry.certs.is_empty(),
            "JKS must have at least one cert in chain"
        );
        eprintln!(
            "JKS read: alias={}, key_der={} bytes, {} certs",
            entry.alias,
            entry.key_der.len(),
            entry.certs.len()
        );
    }

    #[test]
    fn test_bad_magic() {
        let data = vec![0u8; 100];
        let result = read_jks(&data, "");
        assert!(matches!(result, Err(JksError::BadMagic)));
    }

    #[test]
    fn test_bad_password() {
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return, // skip
        };
        let result = read_jks(&data, "wrongpassword");
        assert!(matches!(result, Err(JksError::BadPassword)));
    }
}
