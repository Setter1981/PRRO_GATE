//! EnvelopedData (PKCS#7 / CMS) decrypt for DSTU 4145 + GOST 28147.
//!
//! Implements the full unwrap pipeline jkurwa uses for encrypted
//! messages from ДПС / ЦСК:
//!
//! 1. Parse EnvelopedData ASN.1 → extract recipient params
//! 2. ECDH cofactor-DH: Z = Q * (d·h), ZZ = Z.x
//! 3. KDF: KEK = GOST34311(ZZ ‖ counter ‖ SharedInfo(ukm))
//! 4. Key-unwrap: CEK = GOST28147_keywrap_unwrap(KEK, wcek)
//! 5. Content decrypt: plain = GOST28147_CFB(CEK, iv, sbox, ct)

use crate::core::curve::Curve;
use crate::core::field::FieldEl;
use crate::core::hash::gost_34_311_95;
use crate::core::point::Point;
use crate::core::scalar::Scalar;
use crate::interop::prro::pbe::gost28147_keywrap_unwrap;

use thiserror::Error;

use crate::cms::asn1_util::{self as a1, Asn1Error};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EnvelopeError {
    #[error("ASN.1 parse: {0}")]
    Asn1(String),
    #[error("ASN.1: {0}")]
    Asn1Util(#[from] Asn1Error),
    #[error("unsupported EnvelopedData version {0}")]
    UnsupportedVersion(u64),
    #[error("no KeyAgreeRecipientInfo found")]
    NoKari,
    #[error("keywrap unwrap: {0}")]
    KeyUnwrap(&'static str),
    #[error("originator public key required but not provided")]
    NoOriginatorKey,
    #[error("private-key scalar width unexpected: {0}")]
    BadScalarWidth(usize),
    #[error("envelope cipher params invalid: {0}")]
    BadCipherParams(String),
}

/// Parameters extracted from an EnvelopedData for a single recipient.
#[derive(Debug)]
#[non_exhaustive]
pub struct EnvelopeParams {
    pub ukm: Vec<u8>,
    pub wcek: Vec<u8>,
    pub iv: Vec<u8>,
    pub sbox: Vec<u8>,
    pub encrypted_content: Vec<u8>,
    pub originator_pub_compressed: Option<Vec<u8>>,
}

// ─── ECDH cofactor-DH ────────────────────────────────────────────────

/// Compute the ECDH shared secret ZZ = (Q · (d·h)).x as a variable-length
/// big-endian byte array (leading zero stripped if present). Port of
/// jkurwa `Priv.prototype.derive` + `sharedKey` ZZ handling.
pub fn ecdh_zz(
    d: &FieldEl,
    pub_q: &Point,
    curve: &Curve,
) -> Result<Vec<u8>, EnvelopeError> {
    // Full public-point validation BEFORE any secret-scalar arithmetic.
    // An invalid-curve / small-subgroup / identity Q must be rejected
    // here, not discovered after the ladder has already leaked scalar
    // bits through timing on a maliciously crafted point.
    crate::core::point::validate_public_point(curve, pub_q)
        .map_err(|e| EnvelopeError::BadCipherParams(format!("originator pubkey: {e}")))?;

    // Compute the secret scalar `s = d·h (mod n)` where `h` is the
    // curve cofactor. From here on `s` is secret and must travel only
    // through constant-time primitives.
    let d_words = d
        .try_as_fe_words()
        .ok_or(EnvelopeError::BadScalarWidth(d.bytes.len()))?;
    let mut d_scalar = Scalar::from_fe_truncated(&d_words);
    let h_scalar = Scalar::from_limbs([curve.kofactor as u64, 0, 0, 0]);
    let mut s_scalar = d_scalar.mul_mod(&h_scalar);

    // Explicit wipe — Scalar is Copy, no auto-drop wipe.
    use zeroize::Zeroize;
    d_scalar.zeroize();

    let s_bytes = s_scalar.to_le_bytes();
    s_scalar.zeroize();
    let mut s_words = vec![0u32; curve.mod_words];
    for i in 0..s_words.len().min(8) {
        s_words[i] = u32::from_le_bytes([
            s_bytes[i * 4],
            s_bytes[i * 4 + 1],
            s_bytes[i * 4 + 2],
            s_bytes[i * 4 + 3],
        ]);
    }
    let s_fe = FieldEl::from_words(s_words);

    // Route through the x-only López-Dahab ladder — the same CT
    // primitive the signing path uses for ephemeral scalar × base.
    // The previous wNAF path (`pub_q.mul(...)`) was variable-time and
    // would leak `s` bits through table lookups / branch timing.
    // `(k·Q).x` is all the caller consumes — we don't need y-recovery.
    let z_x = crate::core::mladder::mul_point_x_ct(&pub_q.x, &s_fe, curve)
        .map_err(|e| EnvelopeError::BadCipherParams(format!("ECDH ladder: {e}")))?
        .ok_or_else(|| EnvelopeError::Asn1("ECDH produced point at infinity".into()))?;

    // x-coordinate → big-endian bytes, trim to ceil(m/8)
    let m_bytes = ((curve.m as usize) + 7) / 8; // 33 for m=257
    let total_bytes = curve.mod_words * 4; // 36 for mod_words=9

    // Build big-endian output (high word first, high byte first within word)
    let mut be = Vec::with_capacity(total_bytes);
    for wi in (0..curve.mod_words).rev() {
        let w = if wi < z_x.bytes.len() { z_x.bytes[wi] } else { 0 };
        be.push((w >> 24) as u8);
        be.push((w >> 16) as u8);
        be.push((w >> 8) as u8);
        be.push(w as u8);
    }
    // Trim to m_bytes from the right (skip leading padding)
    let cut = total_bytes - m_bytes;
    let mut zz: Vec<u8> = be[cut..].to_vec();

    // jkurwa: if(zz[0] === 0) zz = zz.slice(1);
    if !zz.is_empty() && zz[0] == 0 {
        zz = zz[1..].to_vec();
    }
    Ok(zz)
}

// ─── SharedInfo DER (gost_salt) ──────────────────────────────────────

/// OID 1.2.804.2.1.1.1.1.1.1.5 — "Gost28147-cfb-wrap"
const OID_GOST28147_CFB_WRAP: &[u8] = &[
    0x2a, 0x86, 0x24, 0x02, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x05,
];

/// DER-encode the SharedInfo structure used as the KDF salt.
/// Port of jkurwa `gost_salt(ukm)`.
///
/// ```text
/// SharedInfo ::= SEQUENCE {
///     keyInfo    SEQUENCE { algorithm OID, parameters NULL },
///     entityInfo [0] EXPLICIT OCTET STRING (ukm),
///     suppPubInfo [2] EXPLICIT OCTET STRING (0x00000100 = 256 bits)
/// }
/// ```
pub fn encode_shared_info(ukm: &[u8]) -> Vec<u8> {
    // keyInfo = SEQUENCE { OID, NULL }
    let oid_tlv_len = 2 + OID_GOST28147_CFB_WRAP.len(); // 06 0b ...
    let null_len = 2; // 05 00
    let key_info_inner = oid_tlv_len + null_len;
    let key_info_len = 2 + key_info_inner; // 30 [len] ...

    // entityInfo = [0] EXPLICIT { OCTET STRING ukm }
    let ukm_octet_len = der_len_size(ukm.len()) + 1 + ukm.len(); // 04 [len] [data]
    let entity_info_len = der_len_size(ukm_octet_len) + 1 + ukm_octet_len; // a0 [len] ...

    // suppPubInfo = [2] EXPLICIT { OCTET STRING 0x00000100 }
    let supp_inner = 6; // 04 04 00 00 01 00
    let supp_len = 2 + supp_inner; // a2 06 ...

    let seq_inner = key_info_len + entity_info_len + supp_len;

    let mut out = Vec::with_capacity(2 + seq_inner + 4);
    // SEQUENCE
    out.push(0x30);
    push_der_len(&mut out, seq_inner);
    // keyInfo SEQUENCE
    out.push(0x30);
    push_der_len(&mut out, key_info_inner);
    out.push(0x06);
    push_der_len(&mut out, OID_GOST28147_CFB_WRAP.len());
    out.extend_from_slice(OID_GOST28147_CFB_WRAP);
    out.push(0x05);
    out.push(0x00); // NULL
    // entityInfo [0] EXPLICIT
    out.push(0xa0);
    push_der_len(&mut out, ukm_octet_len);
    out.push(0x04);
    push_der_len(&mut out, ukm.len());
    out.extend_from_slice(ukm);
    // suppPubInfo [2] EXPLICIT
    out.push(0xa2);
    push_der_len(&mut out, supp_inner);
    out.push(0x04);
    out.push(0x04);
    out.extend_from_slice(&[0x00, 0x00, 0x01, 0x00]);
    out
}

fn der_len_size(len: usize) -> usize {
    if len < 0x80 { 1 } else if len < 0x100 { 2 } else { 3 }
}

fn push_der_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else if len < 0x100 {
        out.push(0x81);
        out.push(len as u8);
    } else {
        out.push(0x82);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    }
}

// ─── KEK derivation ──────────────────────────────────────────────────

/// Derive the Key Encryption Key (KEK) from ECDH shared secret + UKM.
/// `KEK = GOST34311(ZZ ‖ 0x00000001 ‖ SharedInfo(ukm))`
pub fn derive_kek(zz: &[u8], ukm: &[u8]) -> [u8; 32] {
    let counter: [u8; 4] = [0x00, 0x00, 0x00, 0x01];
    let salt = encode_shared_info(ukm);

    let mut input = Vec::with_capacity(zz.len() + 4 + salt.len());
    input.extend_from_slice(zz);
    input.extend_from_slice(&counter);
    input.extend_from_slice(&salt);
    gost_34_311_95(&input)
}

// ─── Full unwrap ─────────────────────────────────────────────────────

/// Decrypt an EnvelopedData content given the recipient's private key
/// and the originator's public key.
///
/// This is the low-level entry point that takes pre-extracted params.
/// For the full ASN.1 parse + decrypt pipeline, see [`unwrap_envelope`].
pub fn decrypt_with_params(
    d: &FieldEl,
    pub_q: &Point,
    params: &EnvelopeParams,
    curve: &Curve,
) -> Result<Vec<u8>, EnvelopeError> {
    // Fail-fast on wrong cipher-parameter widths. A silent zero-pad of
    // the sbox (as the previous version did) would produce plausible-
    // looking but wrong plaintext — a much worse failure mode than an
    // explicit error.
    if params.wcek.len() != 44 {
        return Err(EnvelopeError::BadCipherParams(format!(
            "wcek length {} != 44", params.wcek.len()
        )));
    }
    // Ukrainian EnvelopedData puts a 32-byte OCTET STRING as the "IV"
    // in contentEncryptionAlgorithm::parameters (legacy convention).
    // Only the first 8 bytes are consumed by GOST 28147 CFB (matching
    // jkurwa's `decrypt_cfb` which does `for i in 0..8 { cur_iv[i] = iv[i] }`).
    // We reject < 8 as structurally malformed; > 8 is accepted because
    // real production envelopes carry 32. An exact `== 8` check would
    // break every real Ukrainian envelope.
    if params.iv.len() < 8 {
        return Err(EnvelopeError::BadCipherParams(format!(
            "iv too short: {} < 8", params.iv.len()
        )));
    }
    if params.sbox.len() != 64 {
        return Err(EnvelopeError::BadCipherParams(format!(
            "sbox length {} != 64 (DSTU packed DKU)", params.sbox.len()
        )));
    }

    use zeroize::Zeroize;

    let mut zz = ecdh_zz(d, pub_q, curve)?;
    let mut kek = derive_kek(&zz, &params.ukm);
    zz.zeroize();
    let mut wcek_arr = [0u8; 44];
    wcek_arr.copy_from_slice(&params.wcek);
    let mut cek = gost28147_keywrap_unwrap(&kek, &wcek_arr)
        .map_err(EnvelopeError::KeyUnwrap)?;
    kek.zeroize();
    wcek_arr.zeroize();

    let mut iv = [0u8; 8];
    iv.copy_from_slice(&params.iv[..8]);

    let mut sbox = [0u8; 64];
    sbox.copy_from_slice(&params.sbox[..64]);

    let ct = &params.encrypted_content;
    if ct.is_empty() {
        cek.zeroize();
        return Ok(Vec::new());
    }
    // Content may not be block-aligned (jkurwa uses ceil-blocks CFB).
    // `gost28147_cfb_decrypt_any_len` already returns exactly `ct.len()`
    // bytes; no further slicing needed.
    let plaintext = crate::interop::prro::pbe::gost28147_cfb_decrypt_any_len(
        &cek, &iv, &sbox, ct,
    );
    cek.zeroize();
    Ok(plaintext)
}

// ─── ASN.1 minimal walker for EnvelopedData ──────────────────────────

/// Parse a PKCS#7 ContentInfo { envelopedData } and decrypt with the
/// given private key + originator pubkey lookup.
///
/// `originator_pub` is the decompressed originator public key. In jkurwa
/// this comes from looking up the originator certificate by
/// IssuerAndSerialNumber. For our API we require the caller to provide it.
pub fn unwrap_envelope(
    envelope_der: &[u8],
    d: &FieldEl,
    originator_pub: &Point,
    curve: &Curve,
) -> Result<Vec<u8>, EnvelopeError> {
    let params = parse_envelope_params(envelope_der)?;
    decrypt_with_params(d, originator_pub, &params, curve)
}

/// Parse EnvelopedData ASN.1 and extract all parameters needed for decrypt.
pub fn parse_envelope_params(data: &[u8]) -> Result<EnvelopeParams, EnvelopeError> {
    // ContentInfo SEQUENCE
    let (_, ci_inner) = a1::read_tlv(data, 0)?;
    // Skip OID (pkcs7-envelopedData)
    let (oid_end, _) = a1::read_tlv(data, ci_inner)?;
    // [0] EXPLICIT → EnvelopedData SEQUENCE
    let (_, ev_ctx) = a1::read_tlv(data, oid_end)?;
    let (_, ev_inner) = a1::read_tlv(data, ev_ctx)?;

    // version INTEGER
    let (ver_end, ver_start) = a1::read_tlv(data, ev_inner)?;
    let version = a1::read_integer_be_small(data, ver_start, ver_end)?;
    if version != 2 {
        return Err(EnvelopeError::UnsupportedVersion(version));
    }

    // recipientInfos SET
    let (ri_set_end, ri_set_inner) = a1::read_tlv(data, ver_end)?;
    // First element: [1] KeyAgreeRecipientInfo. Validate tag class/number
    // BEFORE dereferencing further.
    let ri_tag = a1::peek_tag(data, ri_set_inner)?;
    if (ri_tag & 0x1f) != 1 {
        return Err(EnvelopeError::NoKari);
    }
    let (_kari_end, kari_inner) = a1::read_tlv(data, ri_set_inner)?;

    // KARI: version(3), originator[0], ukm[1], keyEncAlg, recipientEncryptedKeys
    let (kv_end, _) = a1::read_tlv(data, kari_inner)?; // skip version

    // originator [0] — skip entirely
    let (orig_end, _) = a1::read_tlv(data, kv_end)?;

    // ukm [1] EXPLICIT OCTET STRING
    let (ukm_ctx_end, ukm_ctx_inner) = a1::read_tlv(data, orig_end)?;
    let (ukm_end, ukm_inner) = a1::read_tlv(data, ukm_ctx_inner)?;
    let ukm = data[ukm_inner..ukm_end].to_vec();

    // keyEncryptionAlgorithm SEQUENCE — skip
    let (kea_end, _) = a1::read_tlv(data, ukm_ctx_end)?;

    // recipientEncryptedKeys SEQUENCE of SEQUENCE
    let (_, rek_inner) = a1::read_tlv(data, kea_end)?;
    // First entry SEQUENCE { rid, encryptedKey }
    let (_, entry_inner) = a1::read_tlv(data, rek_inner)?;
    // rid (IssuerAndSerialNumber) — skip
    let (rid_end, _) = a1::read_tlv(data, entry_inner)?;
    // encryptedKey OCTET STRING = wcek
    let (wcek_end, wcek_inner) = a1::read_tlv(data, rid_end)?;
    let wcek = data[wcek_inner..wcek_end].to_vec();

    // EncryptedContentInfo SEQUENCE
    let (_, eci_inner) = a1::read_tlv(data, ri_set_end)?;
    // contentType OID — skip
    let (ct_oid_end, _) = a1::read_tlv(data, eci_inner)?;
    // contentEncryptionAlgorithm SEQUENCE
    let (cea_end, cea_inner) = a1::read_tlv(data, ct_oid_end)?;
    // algorithm OID — skip
    let (alg_oid_end, _) = a1::read_tlv(data, cea_inner)?;
    // parameters SEQUENCE { iv OCTET STRING, sbox OCTET STRING }
    let (_, params_inner) = a1::read_tlv(data, alg_oid_end)?;
    // Strict tag check — both children must be OCTET STRING (0x04),
    // not just "whatever TLV sits here". A misidentified iv/sbox
    // (e.g. an INTEGER mistaken for the IV) would produce a valid-
    // looking but wrong decryption key.
    let iv_tag = a1::peek_tag(data, params_inner)?;
    if iv_tag != 0x04 {
        return Err(EnvelopeError::Asn1(format!(
            "expected OCTET STRING (0x04) for iv, got tag {iv_tag:#x}"
        )));
    }
    let (iv_end, iv_inner) = a1::read_tlv(data, params_inner)?;
    let iv = data[iv_inner..iv_end].to_vec();
    let sbox_tag = a1::peek_tag(data, iv_end)?;
    if sbox_tag != 0x04 {
        return Err(EnvelopeError::Asn1(format!(
            "expected OCTET STRING (0x04) for sbox, got tag {sbox_tag:#x}"
        )));
    }
    let (sbox_end, sbox_inner) = a1::read_tlv(data, iv_end)?;
    let sbox = data[sbox_inner..sbox_end].to_vec();

    // encryptedContent [0] IMPLICIT OCTET STRING
    let (ec_end, ec_inner) = a1::read_tlv(data, cea_end)?;
    let encrypted_content = data[ec_inner..ec_end].to_vec();

    Ok(EnvelopeParams {
        ukm,
        wcek,
        iv,
        sbox,
        encrypted_content,
        originator_pub_compressed: None,
    })
}

// Minimal TLV reading lives in `crate::cms::asn1_util`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_info_deterministic() {
        let ukm = b"12345678901234567890123456789012";
        let a = encode_shared_info(ukm);
        let b = encode_shared_info(ukm);
        assert_eq!(a, b);
        // Must start with SEQUENCE tag
        assert_eq!(a[0], 0x30);
    }

    #[test]
    fn shared_info_contains_oid() {
        let ukm = b"test_ukm";
        let info = encode_shared_info(ukm);
        // Must contain the cfb-wrap OID bytes
        assert!(info
            .windows(OID_GOST28147_CFB_WRAP.len())
            .any(|w| w == OID_GOST28147_CFB_WRAP));
    }

    #[test]
    fn shared_info_contains_ukm_and_suppub() {
        let ukm = b"ABCDEFGH";
        let info = encode_shared_info(ukm);
        // Must contain UKM bytes
        assert!(info.windows(ukm.len()).any(|w| w == ukm));
        // Must contain suppPubInfo = 0x00000100
        assert!(info.windows(4).any(|w| w == [0x00, 0x00, 0x01, 0x00]));
    }

    /// End-to-end decrypt of jkurwa test fixture `enc_message.p7`.
    /// Plaintext is "123" (3 bytes).
    #[test]
    fn e2e_decrypt_jkurwa_enc_message() {
        use crate::core::point::expand_compressed_checked;

        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../sidecar/node_modules/jkurwa/test/data/");

        let key40a0_der = std::fs::read(format!("{base}Key40A0.cer"))
            .expect("read Key40A0.cer");
        let d_bytes = crate::interop::prro::der::extract_param_d(&key40a0_der)
            .expect("extract param_d from Key40A0");

        let curve = Curve::dstu_pb_257();
        let d = FieldEl::from_le_bytes(&d_bytes, curve.mod_words);

        let cert6929_der = std::fs::read(format!("{base}SELF_SIGNED_ENC_6929.cer"))
            .expect("read cert6929");
        let pub_compressed = extract_cert_pubkey_bytes(&cert6929_der)
            .expect("extract pubkey from cert6929");
        let pub_q = expand_compressed_checked(&pub_compressed, &curve)
            .expect("cert6929 pubkey must decompress + validate");

        let envelope = std::fs::read(format!("{base}enc_message.p7"))
            .expect("read enc_message.p7");

        let plaintext = unwrap_envelope(&envelope, &d, &pub_q, &curve)
            .expect("unwrap envelope");
        assert_eq!(plaintext, b"123");
    }

    /// Audit-driven regression: a point-off-curve served in the
    /// originator cert path must be rejected by `ecdh_zz` before any
    /// secret-scalar arithmetic happens, not after.
    #[test]
    fn ecdh_zz_rejects_off_curve_originator() {
        let curve = Curve::dstu_pb_257();
        // Any non-trivial d; concrete value doesn't matter — the test
        // ends before the ladder consumes d.
        let d = FieldEl::from_hex("CAFEBABE", curve.mod_words);

        // Valid Q = -d·G to start, then flip a bit in y — off-curve.
        let g = Point::new(curve.base_x.clone(), curve.base_y.clone());
        let mut bad_q = g.mul(&d, &curve).negate();
        bad_q.y.bytes[0] ^= 1;
        assert!(!curve.contains(&bad_q.x, &bad_q.y));

        let err = ecdh_zz(&d, &bad_q, &curve).unwrap_err();
        match err {
            EnvelopeError::BadCipherParams(msg) => assert!(
                msg.contains("originator pubkey"),
                "expected originator-pubkey validation error, got: {msg}"
            ),
            other => panic!("expected BadCipherParams, got {other:?}"),
        }
    }
}

/// Compute the Ukrainian Subject Key Identifier (SKI) over a DSTU 4145
/// compressed public key. Empirically reverse-engineered from real
/// АЦСК "Україна" certs: matches `GOST34311(0x04 || 0x21 || Q33)`,
/// i.e. RFC 5280 method 1 applied to the BIT STRING content without
/// the unused-bits byte. The 33 bytes are the LE-encoded compressed
/// x-coordinate of Q (parity in bit 0).
///
/// `pubkey_compressed` is what [`extract_cert_pubkey_bytes`] returns
/// for a fresh cert, or what we build ourselves from a freshly-loaded
/// private key (`Q = -d·G` → compress).
pub fn compute_ski(pubkey_compressed: &[u8]) -> [u8; 32] {
    use crate::core::hash::gost_34_311_95;
    let mut input = Vec::with_capacity(2 + pubkey_compressed.len());
    input.push(0x04);
    input.push(pubkey_compressed.len() as u8);
    input.extend_from_slice(pubkey_compressed);
    gost_34_311_95(&input)
}

/// Extract the raw compressed DSTU 4145 public key bytes from a
/// DER-encoded X.509 cert.
///
/// Validates that:
///   - the cert SPKI algorithm OID is DSTU 4145 (little-endian form)
///   - the BIT STRING unused-bits byte is exactly 0
///   - the inner OCTET STRING wrapper is present (as Ukrainian CAs
///     always emit it) and its length matches the expected compressed
///     x-coordinate width for DSTU PB-257 (33 bytes). A matching CA
///     that one day ships something else (e.g. PB-431) would need to
///     loosen this check against a curve descriptor; keeping it strict
///     now catches garbage-in silently producing garbage-out.
pub fn extract_cert_pubkey_bytes(cert_der: &[u8]) -> Result<Vec<u8>, EnvelopeError> {
    // Walk: SEQUENCE → tbsCertificate SEQUENCE → skip version[0], serial,
    // sigAlg, issuer, validity, subject → subjectPublicKeyInfo SEQUENCE →
    // algorithm SEQUENCE → BIT STRING
    let (_, tbs_start) = a1::read_tlv(cert_der, 0)?;
    let (_, tbs_inner) = a1::read_tlv(cert_der, tbs_start)?;

    let mut pos = tbs_inner;
    // version [0] EXPLICIT (optional but present in v3)
    if a1::peek_tag(cert_der, pos)? == 0xa0 {
        let (end, _) = a1::read_tlv(cert_der, pos)?;
        pos = end;
    }
    // serialNumber / signature / issuer / validity / subject — skip 5 TLVs
    for _ in 0..5 {
        let (end, _) = a1::read_tlv(cert_der, pos)?;
        pos = end;
    }

    // subjectPublicKeyInfo SEQUENCE
    let (_, spki_inner) = a1::read_tlv(cert_der, pos)?;
    // algorithm SEQUENCE — inspect, don't just skip
    let (alg_end, alg_inner) = a1::read_tlv(cert_der, spki_inner)?;
    // First child = algorithm OID (OBJECT IDENTIFIER)
    let (oid_end, oid_inner) = a1::read_tlv(cert_der, alg_inner)?;
    if a1::peek_tag(cert_der, alg_inner)? != 0x06 {
        return Err(EnvelopeError::Asn1(
            "SPKI algorithm: first child must be an OID".into(),
        ));
    }
    // DSTU 4145 LE: 1.2.804.2.1.1.1.1.3.1.1 → DER 2A 86 24 02 01 01 01 01 03 01 01
    const DSTU_4145_LE_OID_DER: &[u8] = &[
        0x2A, 0x86, 0x24, 0x02, 0x01, 0x01, 0x01, 0x01, 0x03, 0x01, 0x01,
    ];
    let oid_bytes = &cert_der[oid_inner..oid_end];
    if oid_bytes != DSTU_4145_LE_OID_DER {
        return Err(EnvelopeError::Asn1(format!(
            "SPKI algorithm OID {:02x?} is not DSTU 4145 LE", oid_bytes
        )));
    }

    // BIT STRING
    let bs_tag = a1::peek_tag(cert_der, alg_end)?;
    if bs_tag != 0x03 {
        return Err(EnvelopeError::Asn1(format!(
            "SPKI: expected BIT STRING (0x03) after algorithm, got {bs_tag:#x}"
        )));
    }
    let (bs_end, bs_inner) = a1::read_tlv(cert_der, alg_end)?;
    let bs_data = &cert_der[bs_inner..bs_end];
    if bs_data.is_empty() {
        return Err(EnvelopeError::Asn1("empty BIT STRING".into()));
    }
    // First byte = unused bits. DSTU 4145-LE pubkey is byte-aligned —
    // anything other than 0 is a malformed cert.
    if bs_data[0] != 0 {
        return Err(EnvelopeError::Asn1(format!(
            "BIT STRING has {} unused bits; DSTU pubkey must be byte-aligned",
            bs_data[0]
        )));
    }
    let key_data = &bs_data[1..];

    // Ukrainian CAs wrap the compressed pubkey in an extra OCTET STRING
    // (`04 21 + 33 bytes` for PB-257). Require it + verify length.
    // Fall back to accepting raw bytes only for forward-compat if a
    // future CA drops the wrapper (none do today).
    const DSTU_PB_257_COMPRESSED_LEN: usize = 33;
    if key_data.len() >= 2 && key_data[0] == 0x04 {
        let (oc_end, oc_inner) = a1::read_tlv(key_data, 0)?;
        let compressed = &key_data[oc_inner..oc_end];
        if compressed.len() != DSTU_PB_257_COMPRESSED_LEN {
            return Err(EnvelopeError::Asn1(format!(
                "compressed pubkey length {} != {} (DSTU PB-257)",
                compressed.len(),
                DSTU_PB_257_COMPRESSED_LEN
            )));
        }
        return Ok(compressed.to_vec());
    }
    // Unwrapped form — still require the expected length.
    if key_data.len() != DSTU_PB_257_COMPRESSED_LEN {
        return Err(EnvelopeError::Asn1(format!(
            "unwrapped pubkey length {} != {} (DSTU PB-257)",
            key_data.len(),
            DSTU_PB_257_COMPRESSED_LEN
        )));
    }
    Ok(key_data.to_vec())
}

// ─── Cert basic-fields extractor (M2/W2 prerequisite) ─────────────────

/// Subset of X.509 cert fields the M2 cert-refresher persists into
/// `operator_certs`.  All fields are strings to keep `prro_crypto`
/// chrono-free; the W2 caller parses `valid_from` / `valid_to` via
/// `chrono::DateTime::parse_from_rfc3339` (already in the `prro` crate's
/// dep tree).
///
/// `subject_dn` and `issuer_dn` are minimal RFC 4514–style serialisations
/// (`CN=...,O=...,L=...,C=UA`) — sufficient for the
/// `ca_endpoints.issuer_pattern` case-insensitive substring match and
/// for diagnostic display.  Not intended to round-trip into a strict
/// RFC 4514 parser; if a future caller needs that, build it on top.
#[derive(Debug, Clone)]
pub struct BasicCertFields {
    /// notBefore as RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`).
    pub valid_from: String,
    /// notAfter as RFC 3339.
    pub valid_to: String,
    /// Subject DN, RFC 4514-style (best-effort serialisation).
    pub subject_dn: String,
    /// Issuer DN, RFC 4514-style.
    pub issuer_dn: String,
}

/// Walk a DER-encoded X.509 cert and extract the four "basic" fields the
/// M2 cert-refresher needs (`valid_from`, `valid_to`, `subject_dn`,
/// `issuer_dn`).
///
/// All four fields come out of the `tbsCertificate` SEQUENCE prefix —
/// the same prefix [`extract_cert_pubkey_bytes`] walks.  This is an
/// additive helper (W0-3 §3 amendment 2026-05-05); it does NOT modify
/// any existing parser and shares the bounded `read_tlv` from
/// `crate::cms::asn1_util`.
pub fn parse_cert_basic_fields(cert_der: &[u8]) -> Result<BasicCertFields, EnvelopeError> {
    // Outer SEQUENCE → tbsCertificate SEQUENCE.
    let (_, tbs_start) = a1::read_tlv(cert_der, 0)?;
    let (_, tbs_inner) = a1::read_tlv(cert_der, tbs_start)?;

    let mut pos = tbs_inner;
    // version [0] EXPLICIT (optional)
    if a1::peek_tag(cert_der, pos)? == 0xa0 {
        let (end, _) = a1::read_tlv(cert_der, pos)?;
        pos = end;
    }
    // serialNumber INTEGER — skip
    let (sn_end, _) = a1::read_tlv(cert_der, pos)?;
    pos = sn_end;
    // signature AlgorithmIdentifier SEQUENCE — skip
    let (sig_end, _) = a1::read_tlv(cert_der, pos)?;
    pos = sig_end;

    // issuer Name SEQUENCE → RDNSequence
    let (iss_end, iss_inner) = a1::read_tlv(cert_der, pos)?;
    let issuer_dn = serialize_dn(cert_der, iss_inner, iss_end)?;
    pos = iss_end;

    // validity SEQUENCE { Time, Time }
    let (val_end, val_inner) = a1::read_tlv(cert_der, pos)?;
    let (nb_end, nb_inner) = a1::read_tlv(cert_der, val_inner)?;
    let nb_tag = a1::peek_tag(cert_der, val_inner)?;
    let valid_from = parse_asn1_time(&cert_der[nb_inner..nb_end], nb_tag)?;
    let (na_end, na_inner) = a1::read_tlv(cert_der, nb_end)?;
    let na_tag = a1::peek_tag(cert_der, nb_end)?;
    let valid_to = parse_asn1_time(&cert_der[na_inner..na_end], na_tag)?;
    if na_end > val_end {
        return Err(EnvelopeError::Asn1(
            "validity SEQUENCE: notAfter extends past parent end".into(),
        ));
    }
    pos = val_end;

    // subject Name SEQUENCE → RDNSequence
    let (sub_end, sub_inner) = a1::read_tlv(cert_der, pos)?;
    let subject_dn = serialize_dn(cert_der, sub_inner, sub_end)?;

    Ok(BasicCertFields {
        valid_from,
        valid_to,
        subject_dn,
        issuer_dn,
    })
}

/// Convert an ASN.1 Time TLV (UTCTime tag 0x17, GeneralizedTime tag 0x18)
/// to RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// UTCTime is `YYMMDDHHMMSSZ` (13 chars).  RFC 5280 §4.1.2.5.1 defines
/// the YY pivot: 00..49 → 2000..2049, 50..99 → 1950..1999.
/// GeneralizedTime is `YYYYMMDDHHMMSSZ` (15 chars).
///
/// STRICT per the RFC 5280 cert-validity profile: the body before `Z` must be
/// EXACTLY 12 (UTCTime) or 14 (GeneralizedTime) ASCII digits — trailing junk
/// (`491231235959junkZ`) and fractional seconds (forbidden in cert validity)
/// are rejected, NOT silently truncated.  The calendar fields are then
/// range- and day-of-month-validated (rejects month 13, Feb 31, etc.) so an
/// impossible time can never be turned into an impossible RFC-3339 string.
pub fn parse_asn1_time(content: &[u8], tag: u8) -> Result<String, EnvelopeError> {
    let s = std::str::from_utf8(content)
        .map_err(|_| EnvelopeError::Asn1("ASN.1 Time content is not ASCII".into()))?;
    if !s.ends_with('Z') {
        return Err(EnvelopeError::Asn1(format!(
            "ASN.1 Time {s:?} not in UTC (must end with Z)"
        )));
    }
    let body = &s[..s.len() - 1];
    let (year, mmddhhmmss): (i32, &str) = match tag {
        0x17 => {
            // UTCTime: EXACTLY YYMMDDHHMMSS (no trailing data).
            if body.len() != 12 {
                return Err(EnvelopeError::Asn1(format!(
                    "UTCTime must be exactly YYMMDDHHMMSSZ: {s:?}"
                )));
            }
            let yy: i32 = body[0..2]
                .parse()
                .map_err(|_| EnvelopeError::Asn1(format!("UTCTime: bad year in {s:?}")))?;
            let year = if yy < 50 { 2000 + yy } else { 1900 + yy };
            (year, &body[2..12])
        }
        0x18 => {
            // GeneralizedTime: EXACTLY YYYYMMDDHHMMSS — RFC 5280 cert validity
            // forbids fractional seconds, so any extra chars are rejected.
            if body.len() != 14 {
                return Err(EnvelopeError::Asn1(format!(
                    "GeneralizedTime must be exactly YYYYMMDDHHMMSSZ (no fractional seconds): {s:?}"
                )));
            }
            let yyyy: i32 = body[0..4]
                .parse()
                .map_err(|_| EnvelopeError::Asn1(format!("GeneralizedTime: bad year in {s:?}")))?;
            (yyyy, &body[4..14])
        }
        _ => {
            return Err(EnvelopeError::Asn1(format!(
                "validity entry: unsupported tag {tag:#x} \
                 (expect UTCTime 0x17 or GeneralizedTime 0x18)"
            )));
        }
    };
    if !mmddhhmmss.chars().all(|c| c.is_ascii_digit()) {
        return Err(EnvelopeError::Asn1(format!(
            "ASN.1 Time MMDDHHMMSS is not all digits in {s:?}"
        )));
    }
    // Calendar validation — previously ABSENT, so month=13 / day=99 / Feb-31
    // produced an impossible RFC-3339-shaped string that downstream callers
    // stored without a parse-check.  The 2-char ASCII-digit slices always parse.
    let f = |a: usize| mmddhhmmss[a..a + 2].parse::<u32>().unwrap();
    let (mo, da, ho, mi, se) = (f(0), f(2), f(4), f(6), f(8));
    if !(1..=12).contains(&mo)
        || da < 1
        || da > crate::cms::calendar::days_in_month(year as i64, mo)
        || ho > 23
        || mi > 59
        || se > 60
    {
        return Err(EnvelopeError::Asn1(format!(
            "ASN.1 Time has impossible calendar fields: {s:?}"
        )));
    }
    Ok(format!(
        "{:04}-{}-{}T{}:{}:{}Z",
        year,
        &mmddhhmmss[0..2],
        &mmddhhmmss[2..4],
        &mmddhhmmss[4..6],
        &mmddhhmmss[6..8],
        &mmddhhmmss[8..10],
    ))
}

/// Walk an X.509 RDNSequence (`SEQUENCE OF SET OF AttributeTypeAndValue`)
/// from `start..end` and produce a flat RFC 4514–style DN string.
///
/// `start` is the content offset (first byte of the first SET tag);
/// `end` is the half-open content end.  Each RDN's SET produces one
/// `OID=value` pair (multi-value RDNs are joined with `+` per RFC 4514);
/// pairs are joined with `,` in REVERSE order — RFC 4514 specifies that
/// the most-significant component (CN) comes first, while X.509 stores
/// them root-first.
fn serialize_dn(data: &[u8], start: usize, end: usize) -> Result<String, EnvelopeError> {
    let mut rdn_strings: Vec<String> = Vec::new();
    let mut pos = start;
    while pos < end {
        let (set_end, set_inner) = a1::read_tlv(data, pos)?;
        if a1::peek_tag(data, pos)? != 0x31 {
            return Err(EnvelopeError::Asn1(format!(
                "DN: expected SET (0x31) tag at offset {pos}, got {:#x}",
                a1::peek_tag(data, pos)?
            )));
        }
        // Each SET contains 1+ AttributeTypeAndValue.
        let mut atv_strings: Vec<String> = Vec::new();
        let mut atv_pos = set_inner;
        while atv_pos < set_end {
            let (atv_end, atv_inner) = a1::read_tlv(data, atv_pos)?;
            // AttributeTypeAndValue ::= SEQUENCE { type OID, value ANY }
            let (oid_end, oid_inner) = a1::read_tlv(data, atv_inner)?;
            if a1::peek_tag(data, atv_inner)? != 0x06 {
                return Err(EnvelopeError::Asn1(
                    "DN: AttributeTypeAndValue: first child must be OID".into(),
                ));
            }
            let oid_bytes = &data[oid_inner..oid_end];
            let attr = oid_short_name(oid_bytes);
            // value: usually PrintableString / UTF8String / IA5String /
            // BMPString / UniversalString.  We accept the printable +
            // UTF-8 cases byte-for-byte; everything else falls back to
            // hex-prefixed octet form (`#<hex>`).
            let (val_end, val_inner) = a1::read_tlv(data, oid_end)?;
            let val_tag = a1::peek_tag(data, oid_end)?;
            let val_bytes = &data[val_inner..val_end];
            let val_str = match val_tag {
                0x0c | 0x13 | 0x14 | 0x16 => {
                    // UTF8String, PrintableString, T61String, IA5String
                    String::from_utf8(val_bytes.to_vec()).unwrap_or_else(|_| {
                        format!(
                            "#{}",
                            val_bytes
                                .iter()
                                .map(|b| format!("{b:02x}"))
                                .collect::<String>()
                        )
                    })
                }
                _ => format!(
                    "#{}",
                    val_bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                ),
            };
            atv_strings.push(format!("{attr}={}", escape_dn_value(&val_str)));
            atv_pos = atv_end;
        }
        rdn_strings.push(atv_strings.join("+"));
        pos = set_end;
    }
    // RFC 4514 emits most-significant-RDN first; X.509 stored root-first.
    rdn_strings.reverse();
    Ok(rdn_strings.join(","))
}

/// Map a few well-known DN attribute OIDs to their short names.  Anything
/// not in the table renders as a dotted-decimal OID (RFC 4514 fallback).
fn oid_short_name(oid_der: &[u8]) -> String {
    match oid_der {
        [0x55, 0x04, 0x03] => "CN".into(),
        [0x55, 0x04, 0x04] => "SN".into(),
        [0x55, 0x04, 0x05] => "serialNumber".into(),
        [0x55, 0x04, 0x06] => "C".into(),
        [0x55, 0x04, 0x07] => "L".into(),
        [0x55, 0x04, 0x08] => "ST".into(),
        [0x55, 0x04, 0x09] => "STREET".into(),
        [0x55, 0x04, 0x0a] => "O".into(),
        [0x55, 0x04, 0x0b] => "OU".into(),
        [0x55, 0x04, 0x0c] => "title".into(),
        [0x55, 0x04, 0x2a] => "GN".into(),
        [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x01] => "emailAddress".into(),
        _ => format_oid_dotted(oid_der),
    }
}

/// Render a DER-encoded OID as a dotted-decimal string (RFC 4514's
/// fallback form for unknown attribute types).
fn format_oid_dotted(oid_der: &[u8]) -> String {
    if oid_der.is_empty() {
        return String::from("2.5"); // unreachable in well-formed DER
    }
    let mut out = String::new();
    let first = oid_der[0];
    let arc1 = (first / 40) as u32;
    let arc2 = (first % 40) as u32;
    out.push_str(&arc1.to_string());
    out.push('.');
    out.push_str(&arc2.to_string());
    let mut acc: u32 = 0;
    for &b in &oid_der[1..] {
        acc = (acc << 7) | (b & 0x7f) as u32;
        if b & 0x80 == 0 {
            out.push('.');
            out.push_str(&acc.to_string());
            acc = 0;
        }
    }
    out
}

/// Minimal RFC 4514 DN-value escape for ASCII commas / `+` / `\` /
/// leading-`#` / leading-space / trailing-space.  Good enough for the
/// substring-match callers in M2; not a full RFC 4514 implementation.
fn escape_dn_value(v: &str) -> String {
    if v.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = v.chars().collect();
    let mut out = String::with_capacity(v.len());
    for (i, &c) in chars.iter().enumerate() {
        let needs_escape = matches!(c, ',' | '+' | '\\' | '"' | '<' | '>' | ';')
            || (i == 0 && (c == '#' || c == ' '))
            || (i + 1 == chars.len() && c == ' ');
        if needs_escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod basic_fields_tests {
    use super::*;

    /// Vendored test cert — DSTU 4145 self-signed, validity Jul 2017
    /// → Nov 2023, subject/issuer == `O=Very Much CA,serialNumber=
    /// UA-99999991,L=Wakanda` (printed by `openssl x509 -text`).
    /// File comes from the jkurwa upstream test corpus.
    const CERT_DER: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/node_modules/jkurwa/test/data/SELF_SIGNED_ENC_6929.cer"
    ));

    #[test]
    fn parse_cert_basic_fields_self_signed_6929() {
        let parsed = parse_cert_basic_fields(CERT_DER)
            .expect("parse_cert_basic_fields must succeed on a real DSTU cert");

        // openssl x509 -inform DER -text -noout for this cert prints:
        //   Not Before: Jul 14 02:40:00 2017 GMT
        //   Not After : Nov 14 22:13:20 2023 GMT
        assert_eq!(parsed.valid_from, "2017-07-14T02:40:00Z");
        assert_eq!(parsed.valid_to, "2023-11-14T22:13:20Z");

        // openssl prints the DN root-first ("O=...,serialNumber=...,L=...");
        // RFC 4514 specifies most-significant first, which for this cert
        // (an O-rooted DN with no CN) means L first, then serialNumber,
        // then O.  We assert by membership rather than exact position so
        // the test survives any later RDN ordering tweak — the values
        // themselves are what matter.
        for needle in ["O=Very Much CA", "serialNumber=UA-99999991", "L=Wakanda"] {
            assert!(
                parsed.subject_dn.contains(needle),
                "subject_dn missing {needle:?}: {sub}",
                sub = parsed.subject_dn
            );
            assert!(
                parsed.issuer_dn.contains(needle),
                "issuer_dn missing {needle:?}: {iss}",
                iss = parsed.issuer_dn
            );
        }
        // Self-signed: subject == issuer.
        assert_eq!(parsed.subject_dn, parsed.issuer_dn);
    }

    #[test]
    fn parse_cert_basic_fields_rejects_truncated_input() {
        let truncated = &CERT_DER[..50];
        let err = parse_cert_basic_fields(truncated).expect_err("must reject truncated input");
        assert!(matches!(
            err,
            EnvelopeError::Asn1(_) | EnvelopeError::Asn1Util(_)
        ));
    }

    #[test]
    fn format_oid_dotted_well_known() {
        // 2.5.4.3 commonName
        assert_eq!(format_oid_dotted(&[0x55, 0x04, 0x03]), "2.5.4.3");
        // 1.2.840.113549.1.9.1 emailAddress
        assert_eq!(
            format_oid_dotted(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x01]),
            "1.2.840.113549.1.9.1"
        );
    }

    #[test]
    fn parse_asn1_time_utctime_y2k_pivot() {
        // UTCTime: 49 → 2049, 50 → 1950 per RFC 5280.
        assert_eq!(
            parse_asn1_time(b"491231235959Z", 0x17).unwrap(),
            "2049-12-31T23:59:59Z"
        );
        assert_eq!(
            parse_asn1_time(b"500101000000Z", 0x17).unwrap(),
            "1950-01-01T00:00:00Z"
        );
    }

    #[test]
    fn parse_asn1_time_generalizedtime() {
        assert_eq!(
            parse_asn1_time(b"20991231235959Z", 0x18).unwrap(),
            "2099-12-31T23:59:59Z"
        );
    }

    #[test]
    fn parse_asn1_time_rejects_non_utc() {
        assert!(parse_asn1_time(b"170714024000+0300", 0x17).is_err());
    }
}
