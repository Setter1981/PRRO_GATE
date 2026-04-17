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
///
/// **Secret material.** `key_der` is the decrypted DstuPrivkey DER
/// carrying the raw private scalar — wrapped in `Zeroizing<Vec<u8>>`
/// so it's wiped on drop. `Debug` redacts the bytes.
#[derive(Clone)]
pub struct JksEntry {
    pub alias: String,
    /// DER-encoded EncryptedPrivateKeyInfo (or wrapped DSTU privkey).
    pub key_der: zeroize::Zeroizing<Vec<u8>>,
    /// Certificates in DER form, in stored order.
    pub certs: Vec<Vec<u8>>,
}

impl std::fmt::Debug for JksEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JksEntry")
            .field("alias", &self.alias)
            .field("key_der", &"<redacted>")
            .field("certs", &format_args!("[{} cert(s)]", self.certs.len()))
            .finish()
    }
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

    // Every multi-byte read from here on must surface truncation as a
    // typed error. A silently-indexed panic on a short JKS file turns
    // into an operator-hostile bug report ("the service crashed when I
    // uploaded the keystore") instead of "bad file: truncated at N".
    let tag = read_u32_be_checked(data, offset)?;
    offset += 4;
    if tag != 1 {
        return Err(JksError::NotPrivateKey(tag));
    }

    let alias_len = read_u16_be_checked(data, offset)? as usize;
    offset += 2;
    if offset + alias_len > data.len() {
        return Err(JksError::Truncated(offset));
    }
    let alias = String::from_utf8_lossy(&data[offset..offset + alias_len]).into_owned();
    offset += alias_len;
    // Skip 8 bytes creation time
    if offset + 8 > data.len() {
        return Err(JksError::Truncated(offset));
    }
    offset += 8;

    let pk_len = read_u32_be_checked(data, offset)? as usize;
    offset += 4;
    if offset + pk_len > data.len() {
        return Err(JksError::Truncated(offset));
    }
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
    let cert_count = read_u32_be_checked(data, offset)? as usize;
    offset += 4;
    // Cap allocation by `data.len()` — prevents `with_capacity(usize::MAX)`
    // on a hostile file that claims a billion certs.
    let mut certs: Vec<Vec<u8>> = Vec::with_capacity(cert_count.min(data.len()));
    for _ in 0..cert_count {
        let ct_len = read_u16_be_checked(data, offset)? as usize;
        offset += 2;
        if offset + ct_len > data.len() {
            return Err(JksError::Truncated(offset));
        }
        offset += ct_len; // skip cert type string
        let c_len = read_u32_be_checked(data, offset)? as usize;
        offset += 4;
        if offset + c_len > data.len() {
            return Err(JksError::Truncated(offset));
        }
        certs.push(data[offset..offset + c_len].to_vec());
        offset += c_len;
    }

    Ok(JksEntry {
        alias,
        key_der: zeroize::Zeroizing::new(result),
        certs,
    })
}

#[inline]
fn read_u16_be(buf: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}

#[inline]
fn read_u16_be_checked(buf: &[u8], offset: usize) -> Result<u16, JksError> {
    if offset + 2 > buf.len() {
        return Err(JksError::Truncated(offset));
    }
    Ok(u16::from_be_bytes([buf[offset], buf[offset + 1]]))
}

#[inline]
fn read_u32_be_checked(buf: &[u8], offset: usize) -> Result<u32, JksError> {
    if offset + 4 > buf.len() {
        return Err(JksError::Truncated(offset));
    }
    Ok(u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ]))
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

    /// Regression: a truncated JKS file must surface `Truncated`, not
    /// panic. Several lengths around the boundaries where the parser
    /// reads u16/u32 integers are exercised.
    #[test]
    fn truncated_jks_returns_typed_error_not_panic() {
        let magic: [u8; 4] = [0xFE, 0xED, 0xFE, 0xED];
        // Cases:
        //  - 0 / 5 / 11 bytes: short-circuited by the initial `< 12` guard
        //  - 12 bytes: magic + truncated version/count; parser reads tag next
        //  - 16 bytes: magic+hdr + truncated alias_len u16
        //  - 18 bytes: magic+hdr+tag=1 + alias_len but no alias bytes
        for n in [12usize, 16, 18, 22, 24, 32] {
            let mut v = vec![0u8; n];
            v[..4].copy_from_slice(&magic);
            // Set tag = 1 if we're past the tag bytes
            if n >= 16 {
                v[12..16].copy_from_slice(&[0, 0, 0, 1]);
            }
            let err = read_jks(&v, "").unwrap_err();
            match err {
                JksError::Truncated(_) | JksError::NotPrivateKey(_) => {}
                other => panic!("len={n}: expected Truncated / NotPrivateKey, got {other:?}"),
            }
        }
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
