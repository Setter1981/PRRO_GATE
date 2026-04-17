//! PKCS#12 / PFX / ZS2 private-key container reader.
//!
//! Standard RFC 7292 PFX envelope with **Ukrainian DSTU/GOST algorithms**
//! inside (PBES2 with PBKDF2-HMAC-GOST34311 for key derivation, GOST
//! 28147 in CFB mode for the content cipher). АЦСК "Україна" ships its
//! key files as `.ZS2` — the `Z` prefix is the only thing that
//! distinguishes them from a plain `.pfx`; the bytes are RFC-compliant
//! PKCS#12.
//!
//! ## ASN.1 layout (port of jkurwa `lib/spec/{pfx,pbes}.js`)
//!
//! ```text
//! PFX ::= SEQUENCE {
//!     version    INTEGER {v3(3)},
//!     authSafe   ContentInfo,                  -- {id-data, [0] OCTET STRING data}
//!     macData    MacData OPTIONAL              -- ignored here
//! }
//!
//! AuthenticatedSafe ::= SEQUENCE OF ContentInfo
//!                                     -- each: id-data OR id-encryptedData
//!
//! SafeContents ::= SEQUENCE OF SafeBag
//!
//! SafeBag ::= SEQUENCE {
//!     bagId     OBJECT IDENTIFIER,
//!     bagValue  [0] EXPLICIT ANY DEFINED BY bagId,
//!     bagAttributes SET OF Attribute OPTIONAL
//! }
//! -- For our path: bagId = pkcs-12-pkcs8ShroudedKeyBag (1.2.840.113549.1.12.10.1.2)
//! --               bagValue = EncryptedPrivateKeyInfo (PBES2)
//! ```
//!
//! ## Decryption flow
//!
//! 1. Walk PFX → AuthenticatedSafe → first ShroudedKeyBag.
//! 2. The bag's value is a PBES2 `EncryptedPrivateKeyInfo`:
//!    - `algorithm` SEQUENCE encodes `(salt, iters, IV, S-box DKE)`.
//!    - `encryptedContent` OCTET STRING is the ciphertext.
//! 3. Derive a 32-byte session key with [`pbkdf2_hmac_gost34311`]
//!    (`(password, salt, iters)`).
//! 4. Decrypt the content with [`gost28147_cfb_decrypt`]
//!    (`(key, IV, DKE-packed-sbox)`).
//! 5. The plaintext is a DER-encoded `DstuPrivkey` SEQUENCE; pass it to
//!    [`crate::interop::prro::der::extract_param_d`] to pull `param_d`.

use crate::interop::prro::der::{
    self, DerError, Reader, TAG_INTEGER, TAG_OCTET_STRING, TAG_OID, TAG_SEQUENCE,
};
use crate::interop::prro::pbe::{gost28147_cfb_decrypt_any_len, pbkdf2_hmac_gost34311};

#[derive(Debug, thiserror::Error)]
pub enum PfxError {
    #[error("ASN.1: {0}")]
    Der(#[from] DerError),
    #[error("not a PKCS#12 PFX (version {0} != 3)")]
    BadVersion(i64),
    #[error("PFX has no ShroudedKeyBag (no DSTU private key inside)")]
    NoKeyBag,
    #[error("PBES2 decode: {0}")]
    Pbes2(String),
    #[error("salt/IV/sbox sizes wrong: salt={salt}, iv={iv}, sbox={sbox}")]
    BadParamSizes { salt: usize, iv: usize, sbox: usize },
    #[error("decrypted content failed inner DSTU privkey parse — wrong password or corrupt container")]
    BadPassword,
}

/// Result of a successful PFX/ZS2 parse.
///
/// **Secret material.** `param_d` is the DSTU 4145 private scalar —
/// wrapped in `zeroize::Zeroizing` so it's wiped on drop automatically,
/// without blocking callers from moving `certs` out of the struct.
/// Custom `Debug` redacts the secret bytes.
#[derive(Clone)]
pub struct PfxParsed {
    /// DSTU 4145 private scalar bytes.
    pub param_d: zeroize::Zeroizing<Vec<u8>>,
    /// Embedded certificates (any `pkcs-12-certBag` entries that travel
    /// with the key). Empty for containers that ship only the encrypted
    /// key bag.
    pub certs: Vec<Vec<u8>>,
}

impl std::fmt::Debug for PfxParsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PfxParsed")
            .field("param_d", &"<redacted>")
            .field("certs", &format_args!("[{} cert(s)]", self.certs.len()))
            .finish()
    }
}

// ─── OID constants (DER-encoded byte strings) ──────────────────────────────
//
// Comparing OIDs as raw bytes avoids dragging in an OID arc parser; the
// shapes are short and the universe of relevant OIDs is small.

/// `id-data` (1.2.840.113549.1.7.1) — wraps a SafeContents blob.
const OID_PKCS7_DATA: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01,
];
/// `pkcs-12-pkcs8ShroudedKeyBag` (1.2.840.113549.1.12.10.1.2) — the
/// SafeBag kind that carries an EncryptedPrivateKeyInfo.
const OID_SHROUDED_KEY_BAG: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x0C, 0x0A, 0x01, 0x02,
];
/// `pkcs-12-certBag` (1.2.840.113549.1.12.10.1.3) — a bag carrying an
/// X.509 cert. We collect these for downstream callers that want the
/// cert chain embedded in the container.
const OID_CERT_BAG: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x0C, 0x0A, 0x01, 0x03,
];
/// `pkcs5-pbes2` (1.2.840.113549.1.5.13).
const OID_PBES2: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x05, 0x0D,
];
/// `pkcs5-pbkdf2` (1.2.840.113549.1.5.12).
const OID_PBKDF2: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x05, 0x0C,
];

// ─── Public entry point ────────────────────────────────────────────────────

/// Parse a PKCS#12 / PFX / ZS2 container with the given password.
pub fn parse(data: &[u8], password: &[u8]) -> Result<PfxParsed, PfxError> {
    // PFX SEQUENCE { version INTEGER, authSafe ContentInfo, macData OPTIONAL }
    let mut outer = Reader::new(data);
    let pfx_body = outer.expect(TAG_SEQUENCE)?;
    let mut pfx = Reader::new(pfx_body);

    let version_bytes = pfx.expect(TAG_INTEGER)?;
    let version = read_small_int(version_bytes);
    if version != 3 {
        return Err(PfxError::BadVersion(version));
    }

    // authSafe ContentInfo { contentType=id-data, content=[0] EXPLICIT OCTET STRING }
    let auth_safe_body = pfx.expect(TAG_SEQUENCE)?;
    let auth_safe_data = expect_id_data_explicit_octet(auth_safe_body)?;

    // The OCTET STRING content IS the AuthenticatedSafe = SEQUENCE OF ContentInfo.
    let mut auth_safe = Reader::new(auth_safe_data);
    let inner_seq_body = auth_safe.expect(TAG_SEQUENCE)?;
    let mut content_infos = Reader::new(inner_seq_body);

    let mut param_d: Option<Vec<u8>> = None;
    let mut certs: Vec<Vec<u8>> = Vec::new();

    while !content_infos.at_end() {
        let ci_body = content_infos.expect(TAG_SEQUENCE)?;
        let safe_contents = expect_id_data_explicit_octet(ci_body)?;
        // Encrypted-data ContentInfo (id-encryptedData) is *not* used by
        // any test fixture we have today — the AuthenticatedSafe entries
        // we see all use plain id-data with the encryption pushed down
        // into the per-bag PBES2. If that changes we add a branch here.

        // SafeContents := SEQUENCE OF SafeBag
        let mut sc = Reader::new(safe_contents);
        let bags_body = sc.expect(TAG_SEQUENCE)?;
        let mut bags = Reader::new(bags_body);
        while !bags.at_end() {
            let bag_body = bags.expect(TAG_SEQUENCE)?;
            let mut bag = Reader::new(bag_body);
            let bag_id = bag.expect(TAG_OID)?;
            // bagValue is wrapped in [0] EXPLICIT.
            let bag_value = bag.expect(0xA0)?;
            // bagAttributes (SET) is optional — skip whatever is left.

            if bag_id == OID_SHROUDED_KEY_BAG {
                if param_d.is_some() {
                    // Multiple key bags — first one wins. Production
                    // PRRO containers carry a single signing key.
                    continue;
                }
                let plain = decrypt_pbes2(bag_value, password)?;
                let d = der::extract_param_d(&plain)
                    .map_err(|_| PfxError::BadPassword)?;
                param_d = Some(d);
            } else if bag_id == OID_CERT_BAG {
                if let Some(cert_der) = extract_cert_from_certbag(bag_value) {
                    certs.push(cert_der);
                }
            }
            // Other bag types (secretBag, etc.) — ignore.
        }
    }

    let param_d = param_d.ok_or(PfxError::NoKeyBag)?;
    Ok(PfxParsed {
        param_d: zeroize::Zeroizing::new(param_d),
        certs,
    })
}

// ─── PBES2 decryption ──────────────────────────────────────────────────────

/// Decrypt a `pkcs-12-pkcs8ShroudedKeyBag` value: PBES2 envelope with
/// PBKDF2-HMAC-GOST34311 + GOST 28147 CFB.
fn decrypt_pbes2(bag_value: &[u8], password: &[u8]) -> Result<Vec<u8>, PfxError> {
    // EncryptedPrivateKeyInfo ::= SEQUENCE {
    //     encryptionAlgorithm AlgorithmIdentifier { OID PBES2, params PBES2-params },
    //     encryptedData       OCTET STRING
    // }
    let mut r = Reader::new(bag_value);
    let epki_body = r.expect(TAG_SEQUENCE)?;
    let mut epki = Reader::new(epki_body);

    let alg_body = epki.expect(TAG_SEQUENCE)?;
    let mut alg = Reader::new(alg_body);
    let alg_oid = alg.expect(TAG_OID)?;
    if alg_oid != OID_PBES2 {
        return Err(PfxError::Pbes2(format!(
            "encryption algorithm OID is not PBES2 (got {} bytes)",
            alg_oid.len()
        )));
    }

    // PBES2-params SEQUENCE { keyDerivationFunc, encryptionScheme }
    let pbes2_params_body = alg.expect(TAG_SEQUENCE)?;
    let mut pbes2_params = Reader::new(pbes2_params_body);

    // keyDerivationFunc SEQUENCE { algorithm OID = PBKDF2, parameters PBKDF2-params }
    let kdf_body = pbes2_params.expect(TAG_SEQUENCE)?;
    let mut kdf = Reader::new(kdf_body);
    let kdf_oid = kdf.expect(TAG_OID)?;
    if kdf_oid != OID_PBKDF2 {
        return Err(PfxError::Pbes2("KDF is not PBKDF2".into()));
    }
    let kdf_params_body = kdf.expect(TAG_SEQUENCE)?;
    let mut kdf_params = Reader::new(kdf_params_body);
    let salt = kdf_params.expect(TAG_OCTET_STRING)?;
    let iters_bytes = kdf_params.expect(TAG_INTEGER)?;
    let iters = read_small_int(iters_bytes);
    // DoS cap. A hostile PFX can set `iters = 1_000_000_000` and the
    // decrypter would run a billion GOST-34.311 hashes on the caller's
    // thread. Real Ukrainian tooling uses 2048-10000 — we cap well
    // above that to stay compatible with rare high-iteration variants
    // while rejecting clearly pathological values.
    const MAX_PBKDF2_ITERS: i64 = 200_000;
    if iters <= 0 || iters > MAX_PBKDF2_ITERS {
        return Err(PfxError::Pbes2(format!(
            "PBKDF2 iter count {iters} outside accepted range (1..{MAX_PBKDF2_ITERS})"
        )));
    }
    // Optional `keyLength` INTEGER + optional `prf` AlgorithmIdentifier.
    // We don't need them for our single-block 32-byte derived key path.

    // encryptionScheme SEQUENCE { algorithm OID, parameters SEQUENCE { iv, dke } }
    let enc_body = pbes2_params.expect(TAG_SEQUENCE)?;
    let mut enc = Reader::new(enc_body);
    let enc_oid = enc.expect(TAG_OID)?;
    // Validate encryption-scheme OID: must be GOST 28147 CFB
    // (1.2.804.2.1.1.1.1.1.1.3). Accepting an unknown OID would
    // silently attempt GOST-CFB decrypt on non-GOST ciphertext.
    const GOST28147_CFB_OID: &[u8] = &[
        0x2A, 0x86, 0x24, 0x02, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x03,
    ];
    if enc_oid != GOST28147_CFB_OID {
        return Err(PfxError::Pbes2(format!(
            "unsupported encryption-scheme OID: {:02x?} (expected GOST 28147 CFB)",
            enc_oid
        )));
    }
    let enc_params_body = enc.expect(TAG_SEQUENCE)?;
    let mut enc_params = Reader::new(enc_params_body);
    let iv = enc_params.expect(TAG_OCTET_STRING)?;
    let dke = enc_params.expect(TAG_OCTET_STRING)?;

    if salt.len() != 32 || iv.len() != 8 || dke.len() != 64 {
        return Err(PfxError::BadParamSizes {
            salt: salt.len(),
            iv: iv.len(),
            sbox: dke.len(),
        });
    }

    let encrypted = epki.expect(TAG_OCTET_STRING)?;
    // Derive session key + decrypt via the any-length CFB path — the
    // encrypted payload is not guaranteed block-aligned. The previous
    // code decrypted only `(len / 8) * 8` bytes and zero-filled the
    // tail, which both (a) produced wrong plaintext for non-aligned
    // payloads and (b) silently normalised tampered tail bytes to
    // zeros instead of surfacing an error. `gost28147_cfb_decrypt_any_len`
    // handles partial trailing blocks correctly (matching jkurwa's
    // `decrypt_cfb` with `Math.ceil(len/8)` blocks).
    let key = pbkdf2_hmac_gost34311(password, salt, iters as u32);
    let iv_arr: [u8; 8] = iv.try_into().expect("iv length checked above");
    let dke_arr: [u8; 64] = dke.try_into().expect("dke length checked above");

    let plain = gost28147_cfb_decrypt_any_len(&key, &iv_arr, &dke_arr, encrypted);
    Ok(plain)
}

// ─── ASN.1 helpers ─────────────────────────────────────────────────────────

/// Walk `SEQUENCE { OID id-data, [0] EXPLICIT OCTET STRING data }` and
/// return the OCTET STRING value bytes. Used at every level where PKCS#12
/// wraps a SafeContents inside a ContentInfo with `id-data`.
fn expect_id_data_explicit_octet(body: &[u8]) -> Result<&[u8], PfxError> {
    let mut r = Reader::new(body);
    let oid = r.expect(TAG_OID)?;
    if oid != OID_PKCS7_DATA {
        return Err(PfxError::Pbes2(
            "expected ContentInfo with id-data OID".into(),
        ));
    }
    let explicit_body = r.expect(0xA0)?; // [0] EXPLICIT
    let mut e = Reader::new(explicit_body);
    let octets = e.expect(TAG_OCTET_STRING)?;
    Ok(octets)
}

/// Extract the X.509 cert DER from a `certBag` value: SEQUENCE of
/// `SEQUENCE { id OID, certValue [0] EXPLICIT OCTET STRING }`.
fn extract_cert_from_certbag(bag_value: &[u8]) -> Option<Vec<u8>> {
    // CertBag ::= SEQUENCE { certId OID, certValue [0] EXPLICIT ... }
    // We expect certId = 1.2.840.113549.1.9.22.1 (x509Certificate).
    // Silently accepting any OID would import a non-X.509 blob as if
    // it were a cert — harmless structurally but misleading for the
    // chain consumers downstream.
    const X509_CERT_OID: &[u8] = &[
        0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x16, 0x01,
    ];
    let mut r = Reader::new(bag_value);
    let body = r.expect(TAG_SEQUENCE).ok()?;
    let mut s = Reader::new(body);
    let cert_type_oid = s.expect(TAG_OID).ok()?;
    if cert_type_oid != X509_CERT_OID {
        return None; // not an X.509 certBag — skip silently
    }
    let exp_body = s.expect(0xA0).ok()?; // [0] EXPLICIT
    let mut e = Reader::new(exp_body);
    let cert = e.expect(TAG_OCTET_STRING).ok()?;
    Some(cert.to_vec())
}

/// Read a small INTEGER (≤ i64 worth) from its DER content bytes.
/// Two's-complement big-endian. Used for PFX `version` and PBKDF2 `iters`.
fn read_small_int(bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return 0;
    }
    let mut v: i64 = (bytes[0] as i8) as i64;
    for &b in &bytes[1..] {
        v = (v << 8) | (b as i64);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end parse of the legal-entity ZS2. Cross-validation lives
    /// in `tests/test_pfx_key6_cross.rs` (sanity that PFX-extracted
    /// `param_d` equals the Key-6-extracted one for the same private key).
    #[test]
    fn parse_real_legal_entity_zs2() {
        let path = "/mnt/d/PRRO_GATE/39197544_U250703163535.ZS2";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("SKIP: {} not available", path);
                return;
            }
        };
        let parsed = parse(&data, b"061082").expect("must parse");
        assert!(
            parsed.param_d.len() >= 32 && parsed.param_d.len() <= 34,
            "param_d length {} unexpected for DSTU_PB_257",
            parsed.param_d.len()
        );
        eprintln!(
            "ZS2 legal-entity param_d ({} bytes): {:02x?}",
            parsed.param_d.len(),
            parsed.param_d
        );
    }

    #[test]
    fn parse_real_director_zs2() {
        let path = "/mnt/d/PRRO_GATE/39197544_2790008754_DU250703163535.ZS2";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let parsed = parse(&data, b"061082").expect("director ZS2 must parse");
        eprintln!(
            "ZS2 director param_d ({} bytes): {:02x?}",
            parsed.param_d.len(),
            parsed.param_d
        );
    }

    #[test]
    fn parse_wrong_password_returns_typed_error() {
        let path = "/mnt/d/PRRO_GATE/39197544_U250703163535.ZS2";
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let r = parse(&data, b"wrongpassword");
        assert!(
            matches!(r, Err(PfxError::BadPassword)),
            "expected BadPassword, got {:?}",
            r
        );
    }
}
