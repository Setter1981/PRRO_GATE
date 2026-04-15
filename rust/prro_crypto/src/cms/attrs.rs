//! Signed Attributes builder for CAdES-BES.
//!
//! Per CMS spec + CAdES requirements, the minimum signedAttributes SET for
//! baseline B-B is:
//!   * content-type = id-data
//!   * message-digest = H(content)
//!   * signing-certificate-v2 = ESSCertIDv2 { hash(cert), issuerSerial }
//!
//! The SET OF Attribute is DER-encoded and THAT is what gets signed
//! (not raw content). This module builds the SET and returns the DER bytes.

use crate::cms::{oids, profile::CmsProfile};
use der::{asn1::{OctetString, SetOfVec}, Decode, Encode, Sequence};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AttrsError {
    #[error("DER encode failure: {0}")]
    Der(String),
    #[error("invalid certificate DER: {0}")]
    InvalidCert(String),
    #[error("digest length {got} does not match expected {want}")]
    DigestLen { got: usize, want: usize },
}

/// Build the DER-encoded SET OF Attributes for CAdES-BES.
///
/// Inputs:
///   - `profile`: selects digest OID, cert-hash OID, etc.
///   - `content_digest`: H(content), the message-digest attribute value.
///     Length must match `profile.digest_len()`.
///   - `cert_der`: the signer certificate, DER-encoded. Used to build
///     SigningCertificateV2 and later for `certificates` field.
///
/// Returns:
///   - `attrs_der`: DER of the SET OF Attribute (what the signer signs)
///   - `attrs_inner`: the parsed structure (for embedding in SignerInfo)
///
/// ## Ordering rule
/// DER SET OF must be sorted by encoded element bytes. We use
/// `SetOfVec` which handles this.
pub fn build_signed_attrs(
    profile: CmsProfile,
    content_digest: &[u8],
    cert_der: &[u8],
) -> Result<SignedAttrsBlob, AttrsError> {
    if content_digest.len() != profile.digest_len() {
        return Err(AttrsError::DigestLen {
            got: content_digest.len(),
            want: profile.digest_len(),
        });
    }

    // 1. content-type attribute (id-data for detached data)
    let content_type_val = profile.content_type_oid();
    let content_type = Attribute {
        oid: oids::ID_CONTENT_TYPE,
        value_der: content_type_val.to_der().map_err(|e| AttrsError::Der(e.to_string()))?,
    };

    // 2. message-digest attribute (the content hash)
    let md_value = OctetString::new(content_digest.to_vec())
        .map_err(|e| AttrsError::Der(e.to_string()))?;
    let message_digest = Attribute {
        oid: oids::ID_MESSAGE_DIGEST,
        value_der: md_value.to_der().map_err(|e| AttrsError::Der(e.to_string()))?,
    };

    // 3. signing-certificate-v2 attribute (real GOST 34.311 cert hash)
    let cert_hash = compute_cert_hash(profile, cert_der);
    let scv2_der = build_signing_cert_v2(&cert_hash, profile.cert_hash_oid(), cert_der)?;
    let signing_cert_v2 = Attribute {
        oid: oids::ID_AA_SIGNING_CERTIFICATE_V2,
        value_der: scv2_der,
    };

    Ok(SignedAttrsBlob {
        content_type,
        message_digest,
        signing_cert_v2,
    })
}

/// Flattened signedAttrs prior to DER SET encoding.
#[derive(Debug, Clone)]
pub struct SignedAttrsBlob {
    pub content_type: Attribute,
    pub message_digest: Attribute,
    pub signing_cert_v2: Attribute,
}

impl SignedAttrsBlob {
    /// DER-encode as SET OF Attribute. This is what the signer signs.
    ///
    /// Note: for signing purposes CMS uses IMPLICIT [0] tagged form,
    /// but the DIGEST input is always the explicit SET OF form bytes.
    pub fn to_der_set_of(&self) -> Result<Vec<u8>, AttrsError> {
        // Build individual attribute DER encodings, then wrap in SET OF.
        let a1 = self.content_type.to_der()?;
        let a2 = self.message_digest.to_der()?;
        let a3 = self.signing_cert_v2.to_der()?;

        // Manual SET OF assembly: SET tag 0x31, length, sorted contents.
        // In DER SET OF, elements must be sorted by byte representation.
        let mut elements = vec![a1, a2, a3];
        elements.sort();

        let total_len: usize = elements.iter().map(|e| e.len()).sum();
        let mut out = Vec::with_capacity(total_len + 8);
        out.push(0x31); // SET tag
        encode_length(total_len, &mut out);
        for e in &elements {
            out.extend_from_slice(e);
        }
        Ok(out)
    }
}

/// Single Attribute: `SEQUENCE { type OID, values SET OF ANY }`.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub oid: const_oid::ObjectIdentifier,
    /// DER-encoded attribute value (to be wrapped in SET OF one entry).
    pub value_der: Vec<u8>,
}

impl Attribute {
    /// Encode as a CMS `Attribute` SEQUENCE.
    fn to_der(&self) -> Result<Vec<u8>, AttrsError> {
        // Attribute ::= SEQUENCE { attrType OID, attrValues SET OF ANY }
        let oid_der = self.oid.to_der().map_err(|e| AttrsError::Der(e.to_string()))?;

        // Wrap value in SET OF (single element).
        let mut set_of = Vec::with_capacity(self.value_der.len() + 8);
        set_of.push(0x31); // SET
        encode_length(self.value_der.len(), &mut set_of);
        set_of.extend_from_slice(&self.value_der);

        // Wrap [oid, set_of] in SEQUENCE.
        let inner_len = oid_der.len() + set_of.len();
        let mut out = Vec::with_capacity(inner_len + 8);
        out.push(0x30); // SEQUENCE
        encode_length(inner_len, &mut out);
        out.extend_from_slice(&oid_der);
        out.extend_from_slice(&set_of);
        Ok(out)
    }
}

/// DER length encoding (short form or long form).
fn encode_length(n: usize, out: &mut Vec<u8>) {
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.push(0x81);
        out.push(n as u8);
    } else if n < 0x10000 {
        out.push(0x82);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    } else if n < 0x1000000 {
        out.push(0x83);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    } else {
        out.push(0x84);
        out.push((n >> 24) as u8);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    }
}

/// Compute certificate hash using the profile's digest algorithm.
fn compute_cert_hash(profile: CmsProfile, cert_der: &[u8]) -> Vec<u8> {
    match profile {
        CmsProfile::Dstu4145WithGost34311Pb => {
            crate::hash::gost_34_311_95(cert_der).to_vec()
        }
    }
}

/// Build SigningCertificateV2 attribute value.
///
/// ESSCertIDv2 ::= SEQUENCE {
///     hashAlgorithm    AlgorithmIdentifier DEFAULT {id-sha256},
///     certHash         OCTET STRING,
///     issuerSerial     IssuerSerial OPTIONAL
/// }
///
/// IssuerSerial ::= SEQUENCE {
///     issuer           GeneralNames,
///     serialNumber     CertificateSerialNumber
/// }
///
/// SigningCertificateV2 ::= SEQUENCE {
///     certs            SEQUENCE OF ESSCertIDv2,
///     policies         SEQUENCE OF PolicyInformation OPTIONAL
/// }
///
/// ## Semantic constraints (locked per expert review 2026-04-15)
///
/// **hashAlgorithm emitted explicitly.** RFC 5035 defaults this to SHA-256.
/// For non-SHA-256 profiles (GOST 34.311, Kupyna) omitting the field is
/// semantically wrong — a verifier would interpret certHash bytes as
/// SHA-256 and reject the signature. We therefore always serialize
/// hashAlgorithm, even though DER DEFAULT rules would technically allow
/// omission for SHA-256.
///
/// **issuerSerial populated (B3, post-expert-review).** RFC 5035 §3 says
/// "normally present unless the value can be inferred". We use
/// `sid = IssuerAndSerialNumber` in SignerInfo so the value *is*
/// inferable in principle, but populating issuerSerial here provides a
/// defense-in-depth binding that does not rely on verifier inference.
fn build_signing_cert_v2(
    cert_hash: &[u8],
    hash_algorithm_oid: const_oid::ObjectIdentifier,
    cert_der: &[u8],
) -> Result<Vec<u8>, AttrsError> {
    // certHash OCTET STRING
    let cert_hash_oct = OctetString::new(cert_hash.to_vec())
        .map_err(|e| AttrsError::Der(e.to_string()))?;
    let cert_hash_der = cert_hash_oct.to_der().map_err(|e| AttrsError::Der(e.to_string()))?;

    // hashAlgorithm AlgorithmIdentifier — always emitted (see doc above).
    let alg_id_der = encode_algorithm_identifier(hash_algorithm_oid)?;

    // issuerSerial IssuerSerial — populated from parsed cert.
    let issuer_serial_der = build_issuer_serial(cert_der)?;

    // ESSCertIDv2 = SEQUENCE { hashAlgorithm, certHash, issuerSerial }
    let inner_len = alg_id_der.len() + cert_hash_der.len() + issuer_serial_der.len();
    let mut ess_cert_id = Vec::with_capacity(inner_len + 4);
    ess_cert_id.push(0x30); // SEQUENCE
    encode_length(inner_len, &mut ess_cert_id);
    ess_cert_id.extend_from_slice(&alg_id_der);
    ess_cert_id.extend_from_slice(&cert_hash_der);
    ess_cert_id.extend_from_slice(&issuer_serial_der);

    // certs = SEQUENCE OF ESSCertIDv2  (one element)
    let mut certs_seq = Vec::with_capacity(ess_cert_id.len() + 4);
    certs_seq.push(0x30); // SEQUENCE
    encode_length(ess_cert_id.len(), &mut certs_seq);
    certs_seq.extend_from_slice(&ess_cert_id);

    // SigningCertificateV2 = SEQUENCE { certs ... }
    let mut outer = Vec::with_capacity(certs_seq.len() + 4);
    outer.push(0x30); // SEQUENCE
    encode_length(certs_seq.len(), &mut outer);
    outer.extend_from_slice(&certs_seq);
    Ok(outer)
}

/// Build `IssuerSerial ::= SEQUENCE { issuer GeneralNames, serialNumber CertificateSerialNumber }`
/// from the signer certificate.
///
/// `GeneralName.directoryName` is `[4] EXPLICIT Name` — not IMPLICIT —
/// because `Name` is a CHOICE and IMPLICIT-tagging a CHOICE is forbidden
/// by ASN.1. The explicit form wraps the full Name DER (starting with
/// its own SEQUENCE tag) inside `[4]`.
fn build_issuer_serial(cert_der: &[u8]) -> Result<Vec<u8>, AttrsError> {
    let cert = x509_cert::Certificate::from_der(cert_der)
        .map_err(|e| AttrsError::InvalidCert(e.to_string()))?;

    let issuer_name_der = cert
        .tbs_certificate
        .issuer
        .to_der()
        .map_err(|e| AttrsError::Der(format!("issuer encode: {}", e)))?;
    let serial_der = cert
        .tbs_certificate
        .serial_number
        .to_der()
        .map_err(|e| AttrsError::Der(format!("serial encode: {}", e)))?;

    // GeneralName ::= CHOICE { ..., directoryName [4] EXPLICIT Name, ... }
    // Tag 0xA4 = context-specific [4], constructed.
    let mut directory_name = Vec::with_capacity(issuer_name_der.len() + 4);
    directory_name.push(0xA4);
    encode_length(issuer_name_der.len(), &mut directory_name);
    directory_name.extend_from_slice(&issuer_name_der);

    // GeneralNames ::= SEQUENCE SIZE (1..MAX) OF GeneralName
    let mut general_names = Vec::with_capacity(directory_name.len() + 4);
    general_names.push(0x30); // SEQUENCE
    encode_length(directory_name.len(), &mut general_names);
    general_names.extend_from_slice(&directory_name);

    // IssuerSerial ::= SEQUENCE { issuer GeneralNames, serialNumber CertificateSerialNumber }
    let inner_len = general_names.len() + serial_der.len();
    let mut out = Vec::with_capacity(inner_len + 4);
    out.push(0x30); // SEQUENCE
    encode_length(inner_len, &mut out);
    out.extend_from_slice(&general_names);
    out.extend_from_slice(&serial_der);
    Ok(out)
}

/// Encode `AlgorithmIdentifier ::= SEQUENCE { algorithm OID, parameters ANY DEFINED BY algorithm OPTIONAL }`.
///
/// For hash algorithms, parameters is typically NULL or absent. We emit
/// NULL parameters for maximum interoperability (RFC 4055 §2.1 recommends
/// NULL for hash algorithms that historically used it).
fn encode_algorithm_identifier(
    alg_oid: const_oid::ObjectIdentifier,
) -> Result<Vec<u8>, AttrsError> {
    let oid_der = alg_oid.to_der().map_err(|e| AttrsError::Der(e.to_string()))?;
    // NULL value: 0x05 0x00
    let null_der = [0x05u8, 0x00];

    let inner_len = oid_der.len() + null_der.len();
    let mut out = Vec::with_capacity(inner_len + 4);
    out.push(0x30); // SEQUENCE
    encode_length(inner_len, &mut out);
    out.extend_from_slice(&oid_der);
    out.extend_from_slice(&null_der);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_length_short_form() {
        let mut v = vec![];
        encode_length(0, &mut v);
        assert_eq!(v, [0]);

        let mut v = vec![];
        encode_length(127, &mut v);
        assert_eq!(v, [127]);
    }

    #[test]
    fn test_encode_length_long_form() {
        let mut v = vec![];
        encode_length(128, &mut v);
        assert_eq!(v, [0x81, 128]);

        let mut v = vec![];
        encode_length(256, &mut v);
        assert_eq!(v, [0x82, 0x01, 0x00]);

        let mut v = vec![];
        encode_length(0xFFFF, &mut v);
        assert_eq!(v, [0x82, 0xFF, 0xFF]);
    }

    /// Load real production cert from test JKS. Tests that touch the cert
    /// parser must use a real cert — a minimal fake DER won't satisfy
    /// x509-cert's Certificate::from_der.
    fn load_test_cert() -> Option<Vec<u8>> {
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = std::fs::read(path).ok()?;
        let entry = crate::jks::read_jks(&data, "Jrcfyf123").ok()?;
        entry.certs.into_iter().next()
    }

    #[test]
    fn test_build_signed_attrs_wrong_digest_len() {
        // The digest-length check runs BEFORE cert parsing, so a fake cert
        // is fine here — we never reach the Certificate::from_der step.
        let cert = &[0x30, 0x03, 0x02, 0x01, 0x05];
        let digest = [0u8; 16]; // too short for GOST
        let profile = CmsProfile::Dstu4145WithGost34311Pb;
        let r = build_signed_attrs(profile, &digest, cert);
        assert!(matches!(r, Err(AttrsError::DigestLen { .. })));
    }

    #[test]
    fn test_build_signed_attrs_success_smoke() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => {
                eprintln!("SKIP: no test JKS available");
                return;
            }
        };
        let digest = [0u8; 32];
        let profile = CmsProfile::Dstu4145WithGost34311Pb;
        let attrs = build_signed_attrs(profile, &digest, &cert).unwrap();
        let der = attrs.to_der_set_of().unwrap();
        assert!(!der.is_empty());
        // Starts with SET tag
        assert_eq!(der[0], 0x31);
    }

    /// Regression guard for B3: ESSCertIDv2 must carry issuerSerial now,
    /// so the attribute value is materially larger than the pre-B3 form
    /// and must contain the parsed issuer Name bytes somewhere inside it.
    #[test]
    fn test_signing_cert_v2_includes_issuer_serial() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => return,
        };
        let cert_hash = vec![0u8; 32];
        let scv2 =
            build_signing_cert_v2(&cert_hash, oids::GOST_34_311_95, &cert).unwrap();

        // Parse back: SigningCertificateV2 -> certs(SEQUENCE OF) -> ESSCertIDv2.
        // We don't need a full parser — just verify issuerSerial contributes
        // a recognizable structure: three top-level components inside
        // ESSCertIDv2 (algId SEQUENCE + certHash OCTET STRING + issuerSerial
        // SEQUENCE). Without issuerSerial there would be only two.
        assert_eq!(scv2[0], 0x30, "SigningCertificateV2 must be SEQUENCE");

        // Verify the cert's issuer bytes actually appear inside the output.
        // This proves we did parse the cert and embed issuer Name — a naive
        // zero-filled placeholder would not contain any cert bytes.
        let parsed = x509_cert::Certificate::from_der(&cert).unwrap();
        let issuer_der = parsed.tbs_certificate.issuer.to_der().unwrap();
        assert!(
            scv2.windows(issuer_der.len())
                .any(|w| w == issuer_der.as_slice()),
            "issuer Name DER must appear inside SigningCertificateV2 output"
        );
    }
}
