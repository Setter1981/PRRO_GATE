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
