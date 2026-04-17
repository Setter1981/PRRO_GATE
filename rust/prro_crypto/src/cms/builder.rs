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
#[non_exhaustive]
pub enum CmsError {
    #[error("attrs: {0}")]
    Attrs(#[from] AttrsError),
    #[error("signer: {0}")]
    Signer(#[from] SignerError),
    #[error("DER: {0}")]
    Der(String),
    #[error("TSP: {0}")]
    Tsp(#[from] crate::cms::tsp::TspError),
    #[error("revocation: {0}")]
    Revocation(#[from] crate::cms::revocation::RevocationError),
}

/// Output of `sign_detached`: the CMS SignedData DER ready for ДПС / relying party.
#[derive(Debug, Clone)]
pub struct DetachedSignature {
    /// Full CMS SignedData (ContentInfo-wrapped) as DER bytes.
    /// Typically saved as `<document>.p7s` or attached to a request.
    pub cms_der: Vec<u8>,
}

/// Build-time options for the CMS signature: encapsulation mode and
/// optional signed attributes. Defaults match the most common ДПС
/// fiscal-receipt configuration: detached, current-time `signingTime`.
#[derive(Debug, Clone, Copy)]
pub struct CmsBuildOptions {
    /// Encapsulation mode: `false` (default) emits `EncapsulatedContentInfo`
    /// without an `eContent`, requiring the verifier to bring the content
    /// out-of-band (a `.p7s` "detached" file). `true` embeds the content
    /// inside the SignedData itself — the shape ЦЗО publishes for its
    /// `test.txt.p7s` reference.
    pub attached: bool,
    /// Whether to emit a `signingTime` SignedAttribute (UTCTIME). When
    /// `Some(t)` the supplied instant is encoded; when `None` the
    /// attribute is omitted. The high-level [`CmsSigner::sign_detached`]
    /// constructs `Some(SystemTime::now())` by default — matching every
    /// official ЦЗО CAdES-BES sample we examined.
    pub signing_time: Option<std::time::SystemTime>,
}

impl Default for CmsBuildOptions {
    fn default() -> Self {
        // `signing_time: None` means "ask the signer to stamp at signing
        // time" — `sign_detached` materialises `SystemTime::now()` right
        // before the digest is taken. Baking `now()` into the default
        // would freeze the timestamp to whenever `::default()` was first
        // evaluated, producing stale `signingTime` on batched signing.
        Self {
            attached: false,
            signing_time: None,
        }
    }
}

/// High-level signer: takes content, produces a CMS/CAdES-BES envelope.
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
    /// Sign content as a CAdES-BES `.p7s`. Defaults: detached
    /// encapsulation, `signingTime = now()`. Use [`Self::sign_with`] to
    /// override either default explicitly.
    ///
    /// The `DetachedSignature` type name is kept for backwards
    /// compatibility — the wrapped DER can be either detached or
    /// attached depending on the options used.
    pub fn sign_detached(&self, content: &[u8]) -> Result<DetachedSignature, CmsError> {
        // Stamp the `signingTime` attribute with the actual moment of
        // signing — not whenever `Default::default()` happened to be
        // materialised. Matches every official ЦЗО CAdES-BES sample.
        self.sign_with(
            content,
            CmsBuildOptions {
                attached: false,
                signing_time: Some(std::time::SystemTime::now()),
            },
        )
    }

    /// Sign content with explicit [`CmsBuildOptions`].
    ///
    /// Pipeline:
    ///   1. `content_digest = H(content)`
    ///   2. Build SignedAttributes (content-type, message-digest, sc-v2,
    ///      and optionally signingTime)
    ///   3. `attrs_der = DER(SET OF Attributes)` — this is what the
    ///      signer signs
    ///   4. `signed_attrs_digest = H(attrs_der)`
    ///   5. `signature = signer.sign_digest(signed_attrs_digest)`
    ///   6. Assemble SignerInfo + SignedData + ContentInfo with the
    ///      requested encapsulation mode.
    pub fn sign_with(
        &self,
        content: &[u8],
        opts: CmsBuildOptions,
    ) -> Result<DetachedSignature, CmsError> {
        let content_digest = compute_digest(self.profile, content)?;

        let attrs = crate::cms::attrs::build_signed_attrs_with_time(
            self.profile,
            &content_digest,
            self.cert_der,
            opts.signing_time,
        )?;
        let attrs_der = attrs.to_der_set_of()?;
        let signed_attrs_digest = compute_digest(self.profile, &attrs_der)?;
        let signature_value = self.signer.sign_digest(&signed_attrs_digest)?;

        let cms_der = assemble_signed_data_with_opts(
            self.profile,
            self.cert_der,
            &attrs_der,
            &signature_value,
            if opts.attached { Some(content) } else { None },
        )?;

        Ok(DetachedSignature { cms_der })
    }

    /// CAdES-T uplift: sign, then embed a caller-supplied TST. Use this
    /// when the TimeStampToken is already in hand (e.g. fetched earlier
    /// or pulled from cache). The TST is added as an unsigned attribute
    /// in the single SignerInfo.
    ///
    /// The TST must be a full RFC 3161 `TimeStampToken` DER — i.e. a
    /// `ContentInfo` wrapping `SignedData { eContentType = id-ct-tstInfo }`.
    /// This is exactly what [`crate::cms::tsp::parse_tsp_response`] returns.
    pub fn sign_with_tst(
        &self,
        content: &[u8],
        opts: CmsBuildOptions,
        tst_der: &[u8],
    ) -> Result<DetachedSignature, CmsError> {
        let content_digest = compute_digest(self.profile, content)?;
        let attrs = crate::cms::attrs::build_signed_attrs_with_time(
            self.profile,
            &content_digest,
            self.cert_der,
            opts.signing_time,
        )?;
        let attrs_der = attrs.to_der_set_of()?;
        let signed_attrs_digest = compute_digest(self.profile, &attrs_der)?;
        let signature_value = self.signer.sign_digest(&signed_attrs_digest)?;

        let cms_der = assemble_signed_data_inner(
            self.profile,
            self.cert_der,
            &attrs_der,
            &signature_value,
            if opts.attached { Some(content) } else { None },
            Some(tst_der),
            None,
        )?;
        Ok(DetachedSignature { cms_der })
    }

    /// CAdES-LT uplift. Embeds BOTH a signature-timestamp-token (if given)
    /// AND `id-aa-ets-revocationValues` carrying CRLs and/or OCSP
    /// responses. All inputs are pre-fetched DER bytes — no network.
    ///
    /// The caller is responsible for selecting *which* CRLs and OCSP
    /// responses are relevant to the signer chain and the TSA chain
    /// (CAdES-LT mandates revocation info for every cert in the signed
    /// path). This method trusts the list and emits it byte-faithfully.
    pub fn sign_with_lt(
        &self,
        content: &[u8],
        opts: CmsBuildOptions,
        tst_der: Option<&[u8]>,
        crls_der: &[Vec<u8>],
        ocsp_responses_der: &[Vec<u8>],
    ) -> Result<DetachedSignature, CmsError> {
        let content_digest = compute_digest(self.profile, content)?;
        let attrs = crate::cms::attrs::build_signed_attrs_with_time(
            self.profile,
            &content_digest,
            self.cert_der,
            opts.signing_time,
        )?;
        let attrs_der = attrs.to_der_set_of()?;
        let signed_attrs_digest = compute_digest(self.profile, &attrs_der)?;
        let signature_value = self.signer.sign_digest(&signed_attrs_digest)?;

        let rv_der = if !crls_der.is_empty() || !ocsp_responses_der.is_empty() {
            Some(crate::cms::revocation::encode_revocation_values(
                crls_der,
                ocsp_responses_der,
            ))
        } else {
            None
        };

        let cms_der = assemble_signed_data_inner(
            self.profile,
            self.cert_der,
            &attrs_der,
            &signature_value,
            if opts.attached { Some(content) } else { None },
            tst_der,
            rv_der.as_deref(),
        )?;
        Ok(DetachedSignature { cms_der })
    }

    /// CAdES-T uplift with live TSA round-trip. Signs content to BES,
    /// hashes the resulting raw signature with the profile's digest
    /// algorithm, ships the hash to `tsa_url`, embeds the returned TST.
    ///
    /// Fails if either the signer or the TSA fails. No retry — the
    /// caller owns operational error handling (PRRO write-path already
    /// has structured retry/backoff at its own layer).
    #[cfg(feature = "tsp_http")]
    pub fn sign_with_tsp(
        &self,
        content: &[u8],
        opts: CmsBuildOptions,
        tsa_url: &str,
        tsa_timeout: std::time::Duration,
    ) -> Result<DetachedSignature, CmsError> {
        // Stage 1: compute signature bytes (we need them for the TSP
        // messageImprint even though we don't assemble CMS yet).
        let content_digest = compute_digest(self.profile, content)?;
        let attrs = crate::cms::attrs::build_signed_attrs_with_time(
            self.profile,
            &content_digest,
            self.cert_der,
            opts.signing_time,
        )?;
        let attrs_der = attrs.to_der_set_of()?;
        let signed_attrs_digest = compute_digest(self.profile, &attrs_der)?;
        let signature_value = self.signer.sign_digest(&signed_attrs_digest)?;

        // Stage 2: hash the signature bytes and round-trip to the TSA.
        let signature_imprint = compute_digest(self.profile, &signature_value)?;
        let tst_der = crate::cms::tsp::fetch_timestamp(
            tsa_url,
            &signature_imprint,
            tsa_timeout,
        )
        // Preserve the typed TSP error — callers downstream want to
        // distinguish `TspError::Rejected { status, … }` from
        // `TspError::Http` for retry-vs-surface-to-operator decisions.
        // Wrapping as `CmsError::Der("TSP: …")` (old behaviour) flattens
        // both to a stringly-typed DER error.
        .map_err(CmsError::Tsp)?;

        // Stage 3: reassemble with the TST bolted on as an unsigned attr.
        let cms_der = assemble_signed_data_inner(
            self.profile,
            self.cert_der,
            &attrs_der,
            &signature_value,
            if opts.attached { Some(content) } else { None },
            Some(&tst_der),
            None,
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

// ─── Hash dispatch ──────────────────────────────────────────────────────────

/// Compute the digest matching the profile's hash algorithm.
fn compute_digest(profile: CmsProfile, data: &[u8]) -> Result<Vec<u8>, CmsError> {
    match profile {
        CmsProfile::Dstu4145WithGost34311Pb => {
            Ok(crate::core::hash::gost_34_311_95(data).to_vec())
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
    assemble_signed_data_with_opts(profile, cert_der, signed_attrs_set_der, signature_value, None)
}

/// Variant of [`assemble_signed_data`] that optionally embeds the
/// original content as `eContent` (attached encapsulation). Pass
/// `None` for the legacy detached-only behaviour, `Some(bytes)` for
/// the attached form ЦЗО publishes in its CAdES-BES samples.
fn assemble_signed_data_with_opts(
    profile: CmsProfile,
    cert_der: &[u8],
    signed_attrs_set_der: &[u8],
    signature_value: &[u8],
    content_for_encap: Option<&[u8]>,
) -> Result<Vec<u8>, CmsError> {
    assemble_signed_data_inner(
        profile, cert_der, signed_attrs_set_der, signature_value,
        content_for_encap, None, None,
    )
}

fn assemble_signed_data_inner(
    profile: CmsProfile,
    cert_der: &[u8],
    signed_attrs_set_der: &[u8],
    signature_value: &[u8],
    content_for_encap: Option<&[u8]>,
    timestamp_token_der: Option<&[u8]>,
    revocation_values_der: Option<&[u8]>,
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

    // 6. Build SignerInfo — plus optional unsignedAttrs carrying a
    //    signature-time-stamp-token (CAdES-T uplift).
    let mut si_inner = Vec::new();
    si_inner.extend_from_slice(&dw::integer_u32(1)); // version
    si_inner.extend_from_slice(&isn_der); // sid
    si_inner.extend_from_slice(&digest_alg_der); // digestAlgorithm
    si_inner.extend_from_slice(&signed_attrs_implicit); // signedAttrs
    si_inner.extend_from_slice(&signature_alg_der); // signatureAlgorithm
    si_inner.extend_from_slice(&signature_octet); // signature

    // unsignedAttrs [1] IMPLICIT SET OF Attribute — may carry TST
    // (CAdES-T) and/or revocation-values (CAdES-LT). If both are None,
    // we omit the whole field.
    let mut unsigned_attrs_concat: Vec<u8> = Vec::new();
    if let Some(tst) = timestamp_token_der {
        let tsa_oid_der = oids::ID_AA_SIGNATURE_TIME_STAMP_TOKEN
            .to_der()
            .map_err(|e| CmsError::Der(e.to_string()))?;
        let values_set = dw::set(tst);
        let mut attr_inner = Vec::with_capacity(tsa_oid_der.len() + values_set.len());
        attr_inner.extend_from_slice(&tsa_oid_der);
        attr_inner.extend_from_slice(&values_set);
        unsigned_attrs_concat.extend_from_slice(&dw::sequence(&attr_inner));
    }
    if let Some(rv) = revocation_values_der {
        let rv_oid_der = oids::ID_AA_ETS_REVOCATION_VALUES
            .to_der()
            .map_err(|e| CmsError::Der(e.to_string()))?;
        let values_set = dw::set(rv);
        let mut attr_inner = Vec::with_capacity(rv_oid_der.len() + values_set.len());
        attr_inner.extend_from_slice(&rv_oid_der);
        attr_inner.extend_from_slice(&values_set);
        unsigned_attrs_concat.extend_from_slice(&dw::sequence(&attr_inner));
    }
    if !unsigned_attrs_concat.is_empty() {
        si_inner.extend_from_slice(&dw::implicit_constructed_tag(1, &unsigned_attrs_concat));
    }
    let signer_info_der = dw::sequence(&si_inner);

    // 7. signerInfos SET OF SignerInfo (one element)
    let signer_infos_der = dw::set(&signer_info_der);

    // 8. digestAlgorithms SET OF AlgorithmIdentifier (one element)
    let digest_algs_der = dw::set(&digest_alg_der);

    // 9. encapContentInfo. Detached form omits `eContent`; attached
    //    form embeds the original content inside an OCTET STRING wrapped
    //    in `[0] EXPLICIT`. The content_for_encap argument decides:
    //    `None` → detached, `Some(bytes)` → attached.
    //
    //    EncapsulatedContentInfo ::= SEQUENCE {
    //        eContentType  OID,
    //        eContent     [0] EXPLICIT OCTET STRING OPTIONAL
    //    }
    let id_data_der = oids::ID_DATA
        .to_der()
        .map_err(|e| CmsError::Der(e.to_string()))?;
    let encap_content_info_der = match content_for_encap {
        None => dw::sequence(&id_data_der),
        Some(bytes) => {
            let octet = dw::octet_string(bytes);
            let explicit = dw::explicit_context_tag(0, &octet);
            let mut inner = Vec::with_capacity(id_data_der.len() + explicit.len());
            inner.extend_from_slice(&id_data_der);
            inner.extend_from_slice(&explicit);
            dw::sequence(&inner)
        }
    };

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
    use crate::core::field::FieldEl;

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
        let entry = crate::interop::prro::jks::read_jks(&data, "Jrcfyf123").ok()?;
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
    fn test_sign_with_tst_embeds_unsigned_attribute() {
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

        let opts = CmsBuildOptions {
            attached: false,
            signing_time: None,
        };

        let bes = cms_signer
            .sign_with(b"hello CAdES-T", opts)
            .unwrap()
            .cms_der;

        // Synthetic TST: just a recognisable byte pattern wrapped in a
        // SEQUENCE so it looks like a valid ContentInfo shell to the
        // assembler. Real TSTs are actual SignedData; here we only care
        // that the bytes are reproduced verbatim inside the unsigned attr.
        let fake_tst = {
            let mut v = vec![0x30, 0x08]; // SEQUENCE (8)
            v.extend_from_slice(b"FAKE_TST");
            v
        };

        let t = cms_signer
            .sign_with_tst(b"hello CAdES-T", opts, &fake_tst)
            .unwrap()
            .cms_der;

        // CAdES-T must strictly grow past BES (unsigned attr adds bytes).
        assert!(t.len() > bes.len(), "TST-bearing CMS must be larger");

        // OID id-aa-signatureTimeStampToken = 1.2.840.113549.1.9.16.2.14
        //   DER: 06 0B 2A 86 48 86 F7 0D 01 09 10 02 0E
        const ID_AA_SIG_TST_OID: &[u8] = &[
            0x06, 0x0B, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x0E,
        ];
        assert!(
            t.windows(ID_AA_SIG_TST_OID.len()).any(|w| w == ID_AA_SIG_TST_OID),
            "CAdES-T CMS must contain id-aa-signatureTimeStampToken OID"
        );
        assert!(
            t.windows(fake_tst.len()).any(|w| w == fake_tst),
            "CAdES-T CMS must contain the caller-supplied TST bytes verbatim"
        );
        // BES must NOT contain those bytes.
        assert!(!bes
            .windows(ID_AA_SIG_TST_OID.len())
            .any(|w| w == ID_AA_SIG_TST_OID));
    }

    #[test]
    fn test_sign_with_lt_embeds_revocation_values_and_tst() {
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
        let opts = CmsBuildOptions { attached: false, signing_time: None };

        // Synthetic TST + CRL + OCSP. Real ones have SignedData inside;
        // the embedder passes them through verbatim so byte-pattern
        // matching is a valid structural check here.
        let fake_tst = {
            let mut v = vec![0x30, 0x08];
            v.extend_from_slice(b"FAKE_TST");
            v
        };
        let fake_crl = vec![0x30, 0x06, b'C', b'R', b'L', b'B', b'Y', b'T'];
        let fake_ocsp = vec![0x30, 0x08, b'O', b'C', b'S', b'P', b'B', b'O', b'D', b'Y'];

        let lt = cms_signer
            .sign_with_lt(
                b"LT payload", opts, Some(&fake_tst),
                std::slice::from_ref(&fake_crl),
                std::slice::from_ref(&fake_ocsp),
            )
            .unwrap()
            .cms_der;

        // id-aa-ets-revocationValues OID = 1.2.840.113549.1.9.16.2.24
        //   DER: 06 0B 2A 86 48 86 F7 0D 01 09 10 02 18
        const ID_AA_ETS_RV_OID: &[u8] = &[
            0x06, 0x0B, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x18,
        ];
        assert!(
            lt.windows(ID_AA_ETS_RV_OID.len()).any(|w| w == ID_AA_ETS_RV_OID),
            "CAdES-LT must carry id-aa-ets-revocationValues"
        );
        assert!(lt.windows(fake_tst.len()).any(|w| w == fake_tst));
        assert!(lt.windows(fake_crl.len()).any(|w| w == fake_crl));
        assert!(lt.windows(fake_ocsp.len()).any(|w| w == fake_ocsp));
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

    /// `sign_with` in attached mode must embed the content as `eContent`
    /// inside `encapContentInfo`. We probe the DER for the OCTET STRING
    /// holding the original content bytes.
    #[test]
    fn sign_with_attached_embeds_econtent() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => return,
        };
        let signer = StubSigner;
        let content = b"PRRO receipt body MUST appear in eContent";
        let cms_signer = CmsSigner {
            cert_der: &cert,
            signer: &signer,
            profile: CmsProfile::default(),
        };
        let opts = CmsBuildOptions {
            attached: true,
            signing_time: None,
        };
        let cms = cms_signer.sign_with(content, opts).unwrap().cms_der;
        // Content must appear verbatim somewhere inside the CMS DER.
        assert!(
            cms.windows(content.len()).any(|w| w == content),
            "attached mode should embed content as eContent"
        );

        // Detached form does NOT embed the content.
        let opts_det = CmsBuildOptions {
            attached: false,
            signing_time: None,
        };
        let cms_det = cms_signer.sign_with(content, opts_det).unwrap().cms_der;
        assert!(
            !cms_det.windows(content.len()).any(|w| w == content),
            "detached mode must not embed content"
        );
        assert!(
            cms_det.len() < cms.len(),
            "detached output should be smaller than attached"
        );
    }

    /// `signing_time` option encodes a UTCTIME inside the signedAttrs SET.
    /// We probe for the UTCTIME tag (0x17, length 13) and the attribute
    /// OID `1.2.840.113549.1.9.5` (`signingTime`).
    #[test]
    fn sign_with_signing_time_emits_utctime_attr() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => return,
        };
        let signer = StubSigner;
        let cms_signer = CmsSigner {
            cert_der: &cert,
            signer: &signer,
            profile: CmsProfile::default(),
        };
        // Fixed instant: 2026-04-19T10:00:00Z (UNIX 1_776_592_800)
        let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_776_592_800);
        let opts = CmsBuildOptions {
            attached: false,
            signing_time: Some(when),
        };
        let cms = cms_signer.sign_with(b"x", opts).unwrap().cms_der;

        // signingTime OID DER: 06 09 2A 86 48 86 F7 0D 01 09 05
        let oid = [0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05];
        assert!(
            cms.windows(oid.len()).any(|w| w == oid),
            "signingTime OID must be present in signedAttrs"
        );

        // UTCTIME for 2026-04-19T10:00:00Z = "260419100000Z"
        let utctime = b"260419100000Z";
        let utctime_tlv = {
            let mut v = vec![0x17, utctime.len() as u8];
            v.extend_from_slice(utctime);
            v
        };
        assert!(
            cms.windows(utctime_tlv.len()).any(|w| w == utctime_tlv),
            "UTCTIME bytes for the supplied instant must appear in CMS"
        );

        // Without signing_time, the OID and UTCTIME must NOT appear.
        let opts_none = CmsBuildOptions {
            attached: false,
            signing_time: None,
        };
        let cms_no_st = cms_signer.sign_with(b"x", opts_none).unwrap().cms_der;
        assert!(
            !cms_no_st.windows(oid.len()).any(|w| w == oid),
            "signingTime OID must be absent when signing_time = None"
        );
    }

    /// Default `CmsBuildOptions` is detached + `signing_time = None`
    /// (the explicit "no baked-in time" state). `sign_detached()` then
    /// materialises `SystemTime::now()` at the moment of signing — that
    /// contract is exercised by [`sign_with_signing_time_emits_utctime_attr`]
    /// and the `default_then_sign_detached_stamps_live_time` test below.
    #[test]
    fn default_build_options_are_unstamped() {
        let opts = CmsBuildOptions::default();
        assert_eq!(opts.attached, false);
        assert!(opts.signing_time.is_none());
    }

    /// `sign_detached()` stamps the `signingTime` attribute at call
    /// time even when `Default::default()` carries no time. Proves the
    /// stale-default issue cannot recur silently.
    #[test]
    fn sign_detached_stamps_live_time_despite_empty_default() {
        let cert = match load_test_cert() {
            Some(c) => c,
            None => return,
        };
        let signer = StubSigner;
        let cms_signer = CmsSigner {
            cert_der: &cert,
            signer: &signer,
            profile: CmsProfile::default(),
        };
        let result = cms_signer.sign_detached(b"hello").unwrap();
        // signingTime OID = 1.2.840.113549.1.9.5 = 06 09 2A 86 48 86 F7 0D 01 09 05
        const SIGNING_TIME_OID: &[u8] = &[
            0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05,
        ];
        assert!(
            result.cms_der.windows(SIGNING_TIME_OID.len()).any(|w| w == SIGNING_TIME_OID),
            "sign_detached must embed a signingTime attribute"
        );
    }
}
