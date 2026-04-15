//! Public API for producing CAdES-BES detached signatures.
//!
//! This module wraps the scattered CMS concerns (attrs, signer, encoding)
//! behind a friendly, narrow API. v1 scope: single signer, detached,
//! certificate embedded, no timestamps.

use crate::cms::{
    attrs::{build_signed_attrs, AttrsError},
    profile::CmsProfile,
    signer::{RawSigner, SignerError},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CmsError {
    #[error("attrs: {0}")]
    Attrs(#[from] AttrsError),
    #[error("signer: {0}")]
    Signer(#[from] SignerError),
    #[error("DER: {0}")]
    Der(String),
}

/// Output of `sign_detached`: the CMS SignedData DER ready for ДПС / relying party.
#[derive(Debug, Clone)]
pub struct DetachedSignature {
    /// Full CMS SignedData (ContentInfo-wrapped) as DER bytes.
    /// Typically saved as `<document>.p7s` or attached to a request.
    pub cms_der: Vec<u8>,
}

/// High-level signer: takes content, produces detached CMS/CAdES-BES.
pub struct CmsSigner<'a> {
    /// Signer certificate, DER-encoded. Will be embedded in `certificates`
    /// and referenced by SigningCertificateV2 + IssuerAndSerialNumber.
    pub cert_der: &'a [u8],
    /// Anything implementing RawSigner (in-process DSTU, HSM proxy, etc.)
    pub signer: &'a dyn RawSigner,
    /// Profile (digest + algorithm combination).
    pub profile: CmsProfile,
}

impl<'a> CmsSigner<'a> {
    /// Sign content as a detached CAdES-BES .p7s.
    ///
    /// Pipeline:
    ///   1. content_digest = H(content) with profile's digest algorithm
    ///   2. Build SignedAttributes { content-type, message-digest, sc-v2 }
    ///   3. attrs_der = DER(SET OF Attributes)  ← this is what the signer signs
    ///   4. signed_attrs_digest = H(attrs_der)
    ///   5. signature = signer.sign_digest(signed_attrs_digest)
    ///   6. Assemble SignerInfo + SignedData + ContentInfo
    ///   7. Return DER
    pub fn sign_detached(&self, content: &[u8]) -> Result<DetachedSignature, CmsError> {
        // Step 1: real GOST 34.311 content digest
        let content_digest = compute_digest(self.profile, content)?;

        // Step 2 + 3: build signed attributes and DER-encode them
        let attrs = build_signed_attrs(self.profile, &content_digest, self.cert_der)?;
        let attrs_der = attrs.to_der_set_of()?;

        // Step 4: digest of the signedAttrs DER
        let signed_attrs_digest = compute_digest(self.profile, &attrs_der)?;

        // Step 5: raw signature
        let signature_value = self.signer.sign_digest(&signed_attrs_digest)?;

        // Step 6+7: assemble full CMS SignedData
        let cms_der = assemble_signed_data(
            self.profile,
            self.cert_der,
            &attrs_der,
            &signature_value,
        )?;

        Ok(DetachedSignature { cms_der })
    }
}

/// Lower-level API: caller provides pre-computed CONTENT digest only.
///
/// Renamed from the original `sign_detached_prehashed` per expert review
/// 2026-04-15: the old name was misleading because this API still hashes
/// the signedAttrs DER internally before signing. This function ONLY skips
/// the first step (H(content)); it does NOT skip H(signedAttrs).
///
/// Use this when content is huge or already-hashed upstream; the CMS
/// builder still owns the signedAttrs-hashing step.
///
/// Pipeline:
///   content → (caller supplies H(content) as `content_digest`)
///           ↓
///   build signedAttrs { content-type, message-digest = content_digest, scv2 }
///           ↓
///   builder.internal: H(signedAttrs_der)  ← still done here
///           ↓
///   signer.sign_digest(...)
///           ↓
///   CMS SignedData DER
pub fn sign_detached_with_content_digest(
    profile: CmsProfile,
    cert_der: &[u8],
    content_digest: &[u8],
    signer: &dyn RawSigner,
) -> Result<Vec<u8>, CmsError> {
    let attrs = build_signed_attrs(profile, content_digest, cert_der)?;
    let attrs_der = attrs.to_der_set_of()?;
    let signed_attrs_digest = compute_digest(profile, &attrs_der)?;
    let signature_value = signer.sign_digest(&signed_attrs_digest)?;
    assemble_signed_data(profile, cert_der, &attrs_der, &signature_value)
}

/// Deprecated alias. Use `sign_detached_with_content_digest`.
#[deprecated(note = "renamed to sign_detached_with_content_digest — the old name implied full prehashing, which is misleading")]
pub fn sign_detached_prehashed(
    profile: CmsProfile,
    cert_der: &[u8],
    content_digest: &[u8],
    signer: &dyn RawSigner,
) -> Result<Vec<u8>, CmsError> {
    sign_detached_with_content_digest(profile, cert_der, content_digest, signer)
}

// ─── Hash dispatch ──────────────────────────────────────────────────────────

/// Compute the digest matching the profile's hash algorithm.
fn compute_digest(profile: CmsProfile, data: &[u8]) -> Result<Vec<u8>, CmsError> {
    match profile {
        CmsProfile::Dstu4145WithGost34311Pb => {
            Ok(crate::hash::gost_34_311_95(data).to_vec())
        }
    }
}

/// Assemble a real CMS SignedData ContentInfo DER (Sprint 1.2).
///
/// Output structure:
///   ContentInfo ::= SEQUENCE {
///     contentType  OBJECT IDENTIFIER  = id-signedData,
///     content      [0] EXPLICIT SignedData
///   }
///
///   SignedData ::= SEQUENCE {
///     version            INTEGER  = 1 (when sid = IssuerAndSerialNumber),
///     digestAlgorithms   SET OF AlgorithmIdentifier,
///     encapContentInfo   EncapsulatedContentInfo  (detached: no eContent),
///     certificates       [0] IMPLICIT CertificateSet  (signer cert),
///     signerInfos        SET OF SignerInfo  (one entry)
///   }
///
///   SignerInfo ::= SEQUENCE {
///     version            INTEGER  = 1,
///     sid                IssuerAndSerialNumber,
///     digestAlgorithm    AlgorithmIdentifier,
///     signedAttrs        [0] IMPLICIT SET OF Attribute,
///     signatureAlgorithm AlgorithmIdentifier,
///     signature          OCTET STRING  -- raw signature value
///   }
///
/// Cert parsing via `x509-cert` for IssuerAndSerialNumber extraction.
fn assemble_signed_data(
    profile: CmsProfile,
    cert_der: &[u8],
    signed_attrs_set_der: &[u8],
    signature_value: &[u8],
) -> Result<Vec<u8>, CmsError> {
    use crate::cms::der_writer as dw;
    use crate::cms::oids;
    use der::{Decode, Encode};

    // 1. Parse the certificate to extract issuer + serial number.
    let cert = x509_cert::Certificate::from_der(cert_der)
        .map_err(|e| CmsError::Der(format!("cert parse: {}", e)))?;
    let issuer_der = cert
        .tbs_certificate
        .issuer
        .to_der()
        .map_err(|e| CmsError::Der(format!("issuer encode: {}", e)))?;
    let serial_der = cert
        .tbs_certificate
        .serial_number
        .to_der()
        .map_err(|e| CmsError::Der(format!("serial encode: {}", e)))?;

    // 2. IssuerAndSerialNumber ::= SEQUENCE { issuer Name, serialNumber CertificateSerialNumber }
    let mut isn_inner = Vec::with_capacity(issuer_der.len() + serial_der.len());
    isn_inner.extend_from_slice(&issuer_der);
    isn_inner.extend_from_slice(&serial_der);
    let isn_der = dw::sequence(&isn_inner);

    // 3. AlgorithmIdentifiers
    let digest_alg_der =
        dw::algorithm_identifier(profile.digest_oid()).map_err(|e| CmsError::Der(e.to_string()))?;
    let signature_alg_der = dw::algorithm_identifier(profile.signature_oid())
        .map_err(|e| CmsError::Der(e.to_string()))?;

    // 4. signedAttrs as [0] IMPLICIT SET OF Attribute
    // Our `signed_attrs_set_der` already starts with 0x31 (SET tag).
    // For SignerInfo embedding, replace tag with 0xA0 (context [0] IMPLICIT
    // constructed); content bytes (after the tag+length) stay the same.
    let signed_attrs_implicit = retag_set_to_implicit_zero(signed_attrs_set_der)?;

    // 5. signature is raw OCTET STRING
    let signature_octet = dw::octet_string(signature_value);

    // 6. Build SignerInfo
    let mut si_inner = Vec::new();
    si_inner.extend_from_slice(&dw::integer_u32(1)); // version
    si_inner.extend_from_slice(&isn_der); // sid
    si_inner.extend_from_slice(&digest_alg_der); // digestAlgorithm
    si_inner.extend_from_slice(&signed_attrs_implicit); // signedAttrs
    si_inner.extend_from_slice(&signature_alg_der); // signatureAlgorithm
    si_inner.extend_from_slice(&signature_octet); // signature
    let signer_info_der = dw::sequence(&si_inner);

    // 7. signerInfos SET OF SignerInfo (one element)
    let signer_infos_der = dw::set(&signer_info_der);

    // 8. digestAlgorithms SET OF AlgorithmIdentifier (one element)
    let digest_algs_der = dw::set(&digest_alg_der);

    // 9. encapContentInfo (detached: no eContent)
    //   EncapsulatedContentInfo ::= SEQUENCE { eContentType OID [, eContent [0] EXPLICIT OCTET STRING OPTIONAL] }
    let id_data_der = oids::ID_DATA
        .to_der()
        .map_err(|e| CmsError::Der(e.to_string()))?;
    let encap_content_info_der = dw::sequence(&id_data_der);

    // 10. certificates [0] IMPLICIT CertificateSet
    //    For one cert, this is the cert DER wrapped in [0] IMPLICIT.
    //    Per RFC 5652, certificate is a CHOICE — the default branch is
    //    Certificate (the X.509 cert SEQUENCE). We embed the cert_der
    //    directly inside [0] IMPLICIT.
    let certificates_der = dw::implicit_constructed_tag(0, cert_der);

    // 11. SignedData
    let mut sd_inner = Vec::new();
    sd_inner.extend_from_slice(&dw::integer_u32(1)); // version
    sd_inner.extend_from_slice(&digest_algs_der);
    sd_inner.extend_from_slice(&encap_content_info_der);
    sd_inner.extend_from_slice(&certificates_der);
    sd_inner.extend_from_slice(&signer_infos_der);
    let signed_data_der = dw::sequence(&sd_inner);

    // 12. ContentInfo
    let content_explicit = dw::explicit_context_tag(0, &signed_data_der);
    let signed_data_oid_der = oids::ID_SIGNED_DATA
        .to_der()
        .map_err(|e| CmsError::Der(e.to_string()))?;

    let mut ci_inner = Vec::with_capacity(signed_data_oid_der.len() + content_explicit.len());
    ci_inner.extend_from_slice(&signed_data_oid_der);
    ci_inner.extend_from_slice(&content_explicit);
    let content_info_der = dw::sequence(&ci_inner);

    Ok(content_info_der)
}

/// Retag a DER-encoded SET OF (tag 0x31) as `[0] IMPLICIT` (tag 0xA0)
/// with the same length and content. Used for signedAttrs in SignerInfo.
///
/// IMPLICIT context tagging on a constructed type: replace the underlying
/// tag with the context tag, keeping length and value unchanged.
fn retag_set_to_implicit_zero(der: &[u8]) -> Result<Vec<u8>, CmsError> {
    if der.is_empty() || der[0] != 0x31 {
        return Err(CmsError::Der(format!(
            "expected SET tag 0x31, got 0x{:02X}",
            der.first().copied().unwrap_or(0)
        )));
    }
    let mut out = Vec::with_capacity(der.len());
    out.push(0xA0); // [0] IMPLICIT, constructed
    out.extend_from_slice(&der[1..]); // length + content unchanged
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cms::signer::DstuInProcessSigner;
    use crate::field::FieldEl;

    struct StubSigner;
    impl RawSigner for StubSigner {
        fn sign_digest(&self, _d: &[u8]) -> Result<Vec<u8>, SignerError> {
            Ok(vec![0u8; 64]) // 64-byte raw DSTU signature value
        }
    }

    /// Load a real test certificate from the production JKS file.
    /// Skipped if file unavailable.
    fn load_test_cert() -> Option<Vec<u8>> {
        let path = "/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks";
        let data = std::fs::read(path).ok()?;
        let entry = crate::jks::read_jks(&data, "Jrcfyf123").ok()?;
        entry.certs.into_iter().next()
    }

    #[test]
    fn test_cms_signer_with_real_cert() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => {
                eprintln!("SKIP: no test JKS available");
                return;
            }
        };
        let signer = StubSigner;
        let cms_signer = CmsSigner {
            cert_der: &cert,
            signer: &signer,
            profile: CmsProfile::default(),
        };
        let result = cms_signer.sign_detached(b"hello").unwrap();
        // ContentInfo SEQUENCE
        assert_eq!(result.cms_der[0], 0x30);
        assert!(result.cms_der.len() > 100, "CMS too small");
    }

    #[test]
    fn test_sign_detached_with_content_digest() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => return,
        };
        let signer = StubSigner;
        let digest = vec![0u8; 32];
        let r = sign_detached_with_content_digest(
            CmsProfile::default(),
            &cert,
            &digest,
            &signer,
        )
        .unwrap();
        assert!(!r.is_empty());
        assert_eq!(r[0], 0x30); // SEQUENCE
    }

    #[test]
    fn test_cms_with_real_dstu_signer() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => return,
        };
        let d = FieldEl::from_hex("DEADBEEFCAFE12345678", 9);
        // Production constructor — OsRng. Signature bytes differ per run,
        // so we only assert CMS envelope structure, not signature value.
        let signer = DstuInProcessSigner::new(d);
        let cms_signer = CmsSigner {
            cert_der: &cert,
            signer: &signer,
            profile: CmsProfile::default(),
        };
        let result = cms_signer.sign_detached(b"test content").unwrap();
        assert!(!result.cms_der.is_empty());
        assert_eq!(result.cms_der[0], 0x30);
    }
}
