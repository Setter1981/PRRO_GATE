//! Revocation info plumbing for CAdES-LT:
//!
//! - OCSP (RFC 6960) request encoder + response parser
//! - AuthorityInfoAccess / CRLDistributionPoints parsers (extract URLs
//!   from the signer's cert — no hard-coding)
//! - HTTP fetchers for OCSP (POST) and CRL (GET), feature-gated behind
//!   `tsp_http` (reuses the same HTTP dep as TSP)
//! - `revocation-values` unsigned attribute DER encoder (SET of CRLs +
//!   SET of BasicOCSPResponses), shape matches
//!   `id-aa-ets-revocationValues` RFC 5126 §6.3.3 / ETSI TS 101 733.
//!
//! Verification (signature chain, responder cert validity, CRL freshness)
//! is intentionally out of scope: relying parties do their own checks,
//! and embedding *the wrong* revocation bytes is caught downstream.
//! Our job is correct structural encoding and byte-faithful relay.

use thiserror::Error;

use crate::cms::der_writer as dw;
use crate::cms::asn1_util::{self as a1, Asn1Error};
use crate::cms::oids;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RevocationError {
    #[error("DER write: {0}")]
    DerWrite(#[from] crate::cms::der_writer::DerWriterError),
    #[error("ASN.1: {0}")]
    Asn1(#[from] Asn1Error),
    #[error("parse: {0}")]
    Parse(String),
    #[error("OCSP rejected: responseStatus={0}")]
    OcspRejected(u64),
    #[error("HTTP transport: {0}")]
    Http(String),
    #[error("HTTP {code} {status_text}")]
    HttpStatus { code: u16, status_text: String },
}

// ─── OCSP request encode ──────────────────────────────────────────────────

/// Build a minimal RFC 6960 `OCSPRequest` DER blob for a single cert.
///
/// ```text
/// OCSPRequest ::= SEQUENCE {
///     tbsRequest  TBSRequest
///     -- optionalSignature omitted (unsigned requests are the norm)
/// }
/// TBSRequest ::= SEQUENCE {
///     -- version[0] EXPLICIT default v1 → omitted
///     -- requestorName[1] EXPLICIT → omitted
///     requestList       SEQUENCE OF Request,
///     requestExtensions [2] EXPLICIT Extensions OPTIONAL  -- nonce if given
/// }
/// Request ::= SEQUENCE { reqCert CertID }
/// CertID ::= SEQUENCE {
///     hashAlgorithm     AlgorithmIdentifier (GOST 34.311),
///     issuerNameHash    OCTET STRING,
///     issuerKeyHash     OCTET STRING,
///     serialNumber      CertificateSerialNumber (INTEGER)
/// }
/// ```
pub fn encode_ocsp_request(
    issuer_name_hash: &[u8],
    issuer_key_hash: &[u8],
    serial_der: &[u8],
    nonce: Option<&[u8]>,
) -> Result<Vec<u8>, RevocationError> {
    // CertID
    let alg = dw::algorithm_identifier(oids::GOST_34_311_95)?;
    let in_hash = dw::octet_string(issuer_name_hash);
    let ik_hash = dw::octet_string(issuer_key_hash);

    // serial_der is already a full INTEGER TLV (tag+len+value) from
    // `x509-cert`'s SerialNumber::to_der; concatenate directly.
    let mut cert_id_inner = Vec::with_capacity(
        alg.len() + in_hash.len() + ik_hash.len() + serial_der.len(),
    );
    cert_id_inner.extend_from_slice(&alg);
    cert_id_inner.extend_from_slice(&in_hash);
    cert_id_inner.extend_from_slice(&ik_hash);
    cert_id_inner.extend_from_slice(serial_der);
    let cert_id = dw::sequence(&cert_id_inner);

    // Request ::= SEQUENCE { reqCert CertID }
    let request = dw::sequence(&cert_id);
    // requestList SEQUENCE OF Request
    let request_list = dw::sequence(&request);

    let mut tbs_inner = Vec::with_capacity(request_list.len() + 64);
    tbs_inner.extend_from_slice(&request_list);

    if let Some(nonce_bytes) = nonce {
        // Extensions ::= SEQUENCE OF Extension
        // Extension ::= SEQUENCE { extnID OID, extnValue OCTET STRING }
        // nonce OID = 1.3.6.1.5.5.7.48.1.2
        const OCSP_NONCE_OID_DER: &[u8] = &[
            0x06, 0x09, 0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x01, 0x02,
        ];
        let inner_octet = dw::octet_string(nonce_bytes);
        let wrapped_octet = dw::octet_string(&inner_octet);
        let mut ext_inner =
            Vec::with_capacity(OCSP_NONCE_OID_DER.len() + wrapped_octet.len());
        ext_inner.extend_from_slice(OCSP_NONCE_OID_DER);
        ext_inner.extend_from_slice(&wrapped_octet);
        let ext_seq = dw::sequence(&ext_inner);
        let exts_seq = dw::sequence(&ext_seq);
        let exts_wrapped = dw::explicit_context_tag(2, &exts_seq);
        tbs_inner.extend_from_slice(&exts_wrapped);
    }

    let tbs_request = dw::sequence(&tbs_inner);
    Ok(dw::sequence(&tbs_request))
}

// ─── OCSP response parse ──────────────────────────────────────────────────

/// Parse an RFC 6960 `OCSPResponse` and return the DER bytes of the
/// embedded `BasicOCSPResponse` (suitable for dropping into
/// `revocation-values`). Only checks `responseStatus == successful (0)`;
/// signature verification is the relying party's job.
pub fn parse_ocsp_response(resp_der: &[u8]) -> Result<Vec<u8>, RevocationError> {
    let (_, outer_inner) = a1::read_tlv(resp_der, 0)?;
    // responseStatus ENUMERATED
    let status_tag = a1::peek_tag(resp_der, outer_inner)?;
    if status_tag != 0x0a {
        return Err(RevocationError::Parse(format!(
            "responseStatus tag {status_tag:#x} != ENUMERATED 0x0a"
        )));
    }
    let (rs_end, rs_start) = a1::read_tlv(resp_der, outer_inner)?;
    let status = a1::read_integer_be_small(resp_der, rs_start, rs_end)?;
    if status != 0 {
        return Err(RevocationError::OcspRejected(status));
    }

    // responseBytes [0] EXPLICIT ResponseBytes OPTIONAL
    let tag = a1::peek_tag(resp_der, rs_end).map_err(|_| RevocationError::Parse(
        "successful OCSPResponse has no responseBytes".into(),
    ))?;
    if tag != 0xa0 {
        return Err(RevocationError::Parse(format!(
            "expected [0] EXPLICIT responseBytes, got tag {tag:#x}",
        )));
    }
    let (_, rb_ctx_inner) = a1::read_tlv(resp_der, rs_end)?;
    let (_, rb_inner) = a1::read_tlv(resp_der, rb_ctx_inner)?; // ResponseBytes SEQ
    // responseType OID — must be id-pkix-ocsp-basic (1.3.6.1.5.5.7.48.1.1).
    // Accepting an arbitrary OID would let us hand back bytes that aren't
    // a BasicOCSPResponse at all, violating our own contract.
    let rtype_tag = a1::peek_tag(resp_der, rb_inner)?;
    if rtype_tag != 0x06 {
        return Err(RevocationError::Parse(format!(
            "expected OID tag (0x06) for responseType, got {rtype_tag:#x}"
        )));
    }
    let (rtype_end, rtype_inner) = a1::read_tlv(resp_der, rb_inner)?;
    // id-pkix-ocsp-basic = 1.3.6.1.5.5.7.48.1.1
    //   DER: 2B 06 01 05 05 07 30 01 01
    const BASIC_OCSP_OID: &[u8] = &[
        0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x01, 0x01,
    ];
    let rtype_bytes = &resp_der[rtype_inner..rtype_end];
    if rtype_bytes != BASIC_OCSP_OID {
        return Err(RevocationError::Parse(format!(
            "responseType OID {:02x?} is not id-pkix-ocsp-basic",
            rtype_bytes
        )));
    }
    // response must be OCTET STRING (0x04) wrapping a BasicOCSPResponse
    let oct_tag = a1::peek_tag(resp_der, rtype_end)?;
    if oct_tag != 0x04 {
        return Err(RevocationError::Parse(format!(
            "response field must be OCTET STRING (0x04), got tag {oct_tag:#x}"
        )));
    }
    let (oct_end, oct_inner) = a1::read_tlv(resp_der, rtype_end)?;
    let payload = &resp_der[oct_inner..oct_end];
    // BasicOCSPResponse itself must be a SEQUENCE
    if payload.is_empty() || payload[0] != 0x30 {
        return Err(RevocationError::Parse(
            "response payload is not a SEQUENCE (expected BasicOCSPResponse)".into()
        ));
    }
    Ok(payload.to_vec())
}

// ─── AIA (OCSP URL) + CDP (CRL URL) parsers ────────────────────────────────

/// Extract the OCSP responder URL from a cert's AIA extension.
pub fn ocsp_url_from_cert(cert_der: &[u8]) -> Result<String, RevocationError> {
    let ext_bytes = extract_extensions_bytes(cert_der)?;
    find_aia_ocsp_url(&ext_bytes)
}

/// Extract the first HTTP CRL URL from a cert's CRLDistributionPoints.
pub fn crl_url_from_cert(cert_der: &[u8]) -> Result<String, RevocationError> {
    let ext_bytes = extract_extensions_bytes(cert_der)?;
    find_crl_dp_url(&ext_bytes)
}

fn extract_extensions_bytes(cert_der: &[u8]) -> Result<Vec<u8>, RevocationError> {
    let (_, tbs_start) = a1::read_tlv(cert_der, 0)?;
    // Bound the scan to tbsCertificate only — wandering past its end
    // into signatureAlgorithm / signatureValue is a structural bug
    // that could extract a URI from non-extension bytes.
    let (tbs_end, tbs_inner) = a1::read_tlv(cert_der, tbs_start)?;

    let mut pos = tbs_inner;
    if a1::peek_tag(cert_der, pos).ok() == Some(0xa0) {
        let (end, _) = a1::read_tlv(cert_der, pos)?;
        pos = end;
    }
    // skip serial, sig, issuer, validity, subject, spki
    for _ in 0..6 {
        let (end, _) = a1::read_tlv(cert_der, pos)?;
        pos = end;
    }
    while pos < tbs_end {
        let tag = a1::peek_tag(cert_der, pos)?;
        let (end, inner) = a1::read_tlv(cert_der, pos)?;
        if tag == 0xa3 {
            // Slice to the inner SEQUENCE's declared end, not the [3]
            // wrapper's end — trailing bytes between the two would be
            // garbage that our extension scanner could mistakenly parse.
            let (ext_seq_end, ext_seq_inner) = a1::read_tlv(cert_der, inner)?;
            return Ok(cert_der[ext_seq_inner..ext_seq_end].to_vec());
        }
        pos = end;
    }
    Err(RevocationError::Parse(
        "cert has no extensions block".into(),
    ))
}

/// Reject URLs that aren't HTTP(S). Passes through to the caller only
/// if the scheme is `http://` or `https://`. This is the SSRF-boundary
/// gate: without it a malicious cert extension could steer our fetcher
/// to `file:///`, `ftp://`, or an internal `gopher://` endpoint.
fn require_http_scheme(url: &str, context: &str) -> Result<String, RevocationError> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(url.to_string())
    } else {
        Err(RevocationError::Parse(format!(
            "{context}: URL scheme not HTTP(S): {url}"
        )))
    }
}

fn find_aia_ocsp_url(ext_list: &[u8]) -> Result<String, RevocationError> {
    // AIA OID = 1.3.6.1.5.5.7.1.1 = 2B 06 01 05 05 07 01 01
    const AIA_OID: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x01, 0x01];
    // ocsp accessMethod = 1.3.6.1.5.5.7.48.1 = 2B 06 01 05 05 07 30 01
    const OCSP_OID: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x01];
    let ad_bytes = find_extension_value(ext_list, AIA_OID)?;
    let (_, list_inner) = a1::read_tlv(&ad_bytes, 0)?;
    first_access_url(&ad_bytes[list_inner..], OCSP_OID)
}

fn first_access_url(
    list_bytes: &[u8],
    target_method_oid: &[u8],
) -> Result<String, RevocationError> {
    let mut pos = 0;
    while pos < list_bytes.len() {
        let (end, inner) = a1::read_tlv(list_bytes, pos)?;
        let (am_end, am_inner) = a1::read_tlv(list_bytes, inner)?;
        let am_bytes = &list_bytes[am_inner..am_end];
        if am_bytes == target_method_oid {
            let tag = a1::peek_tag(list_bytes, am_end)?;
            let (al_end, al_inner) = a1::read_tlv(list_bytes, am_end)?;
            if tag == 0x86 {
                let raw = std::str::from_utf8(&list_bytes[al_inner..al_end])
                    .map_err(|e| RevocationError::Parse(format!(
                        "accessLocation URI not UTF-8: {e}"
                    )))?;
                return require_http_scheme(raw, "AIA/SIA accessLocation");
            }
            return Err(RevocationError::Parse(format!(
                "unsupported accessLocation GeneralName tag {tag:#x}"
            )));
        }
        pos = end;
    }
    Err(RevocationError::Parse(
        "accessMethod not found in list".into(),
    ))
}

fn find_crl_dp_url(ext_list: &[u8]) -> Result<String, RevocationError> {
    // CRLDistributionPoints OID = 2.5.29.31 = 55 1D 1F
    const CDP_OID: &[u8] = &[0x55, 0x1D, 0x1F];
    let cdp_bytes = find_extension_value(ext_list, CDP_OID)?;
    // CRLDistributionPoints ::= SEQUENCE OF DistributionPoint
    let (_, list_inner) = a1::read_tlv(&cdp_bytes, 0)?;
    let mut pos = list_inner;
    while pos < cdp_bytes.len() {
        let (dp_end, dp_inner) = a1::read_tlv(&cdp_bytes, pos)?;
        // DistributionPoint ::= SEQUENCE { distributionPoint [0] DistributionPointName OPTIONAL, ... }
        let mut dpi = dp_inner;
        while dpi < dp_end {
            let tag = a1::peek_tag(&cdp_bytes, dpi)?;
            let (f_end, f_inner) = a1::read_tlv(&cdp_bytes, dpi)?;
            if tag == 0xa0 {
                // [0] DistributionPointName CHOICE
                // fullName [0] IMPLICIT GeneralNames
                let cname = a1::peek_tag(&cdp_bytes, f_inner)?;
                if cname == 0xa0 {
                    let (_, fn_inner) = a1::read_tlv(&cdp_bytes, f_inner)?;
                    // GeneralNames ::= SEQUENCE OF GeneralName
                    let mut gn_pos = fn_inner;
                    while gn_pos < f_end {
                        let gtag = a1::peek_tag(&cdp_bytes, gn_pos)?;
                        let (g_end, g_inner) = a1::read_tlv(&cdp_bytes, gn_pos)?;
                        if gtag == 0x86 {
                            let raw = std::str::from_utf8(&cdp_bytes[g_inner..g_end])
                                .map_err(|e| RevocationError::Parse(format!(
                                    "CRL URI not UTF-8: {e}"
                                )))?;
                            return require_http_scheme(raw, "CRLDistributionPoints");
                        }
                        gn_pos = g_end;
                    }
                }
            }
            dpi = f_end;
        }
        pos = dp_end;
    }
    Err(RevocationError::Parse(
        "no HTTP CRL URL in CRLDistributionPoints".into(),
    ))
}

fn find_extension_value(
    ext_list: &[u8],
    target_oid: &[u8],
) -> Result<Vec<u8>, RevocationError> {
    let mut pos = 0;
    while pos < ext_list.len() {
        let (end, inner) = a1::read_tlv(ext_list, pos)?;
        let (oid_end, oid_inner) = a1::read_tlv(ext_list, inner)?;
        let oid_bytes = &ext_list[oid_inner..oid_end];
        if oid_bytes == target_oid {
            let mut inner_pos = oid_end;
            // Optional `critical BOOLEAN`. DER shape `01 01 FF` or `01 01 00`
            // — we only consume the TLV if both the tag IS 0x01 AND the
            // length byte IS 0x01. Avoids mis-consuming an [1] IMPLICIT
            // context tag that shares byte value 0x01 with primitive BOOLEAN.
            if inner_pos < end
                && a1::peek_tag(ext_list, inner_pos).ok() == Some(0x01)
                && ext_list.get(inner_pos + 1).copied() == Some(0x01)
            {
                let (b_end, _) = a1::read_tlv(ext_list, inner_pos)?;
                inner_pos = b_end;
            }
            // extnValue OCTET STRING → slice to its declared end,
            // not the enclosing Extension SEQUENCE end. Trailing bytes
            // between the two would be garbage / injected TLVs.
            let (ev_end, ev_inner) = a1::read_tlv(ext_list, inner_pos)?;
            return Ok(ext_list[ev_inner..ev_end].to_vec());
        }
        pos = end;
    }
    Err(RevocationError::Parse(format!(
        "extension OID {:02x?} not present",
        target_oid
    )))
}

// ─── HTTP fetchers (feature = "tsp_http") ─────────────────────────────────

/// Fetch an OCSP BasicOCSPResponse for the given cert. `timeout` bounds
/// the round-trip. Returns DER bytes ready to drop into
/// [`encode_revocation_values`].
#[cfg(feature = "tsp_http")]
pub fn fetch_ocsp_response(
    ocsp_url: &str,
    issuer_name_hash: &[u8],
    issuer_key_hash: &[u8],
    serial_der: &[u8],
    nonce: Option<&[u8]>,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, RevocationError> {
    let req_bytes = encode_ocsp_request(
        issuer_name_hash, issuer_key_hash, serial_der, nonce,
    )?;
    // `.redirects(0)` is load-bearing: a 301 redirect on a POST loses the
    // body per HTTP spec + ureq semantics. Without this, an OCSP endpoint
    // that answers HTTP→HTTPS from a misconfigured reverse proxy silently
    // breaks revocation checks and degrades CAdES-LT to CAdES-T.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .redirects(0)
        .build();
    let resp = match agent
        .post(ocsp_url)
        .set("Content-Type", "application/ocsp-request")
        .set("Accept", "application/ocsp-response")
        .send_bytes(&req_bytes)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            return Err(RevocationError::HttpStatus {
                code,
                status_text: r.status_text().to_string(),
            });
        }
        Err(e) => return Err(RevocationError::Http(e.to_string())),
    };
    use std::io::Read;
    let mut body = Vec::with_capacity(4096);
    resp.into_reader()
        .take(256 * 1024)
        .read_to_end(&mut body)
        .map_err(|e| RevocationError::Http(format!("read body: {e}")))?;
    parse_ocsp_response(&body)
}

/// Simple HTTP GET for a CRL URL. Returns the DER bytes as-is (the CRL
/// format is a DER SEQUENCE; PEM is not supported here — fiscal CRLs
/// from Ukrainian CAs ship DER).
#[cfg(feature = "tsp_http")]
pub fn fetch_crl(
    crl_url: &str,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, RevocationError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .redirects(0)
        .build();
    let resp = match agent.get(crl_url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            return Err(RevocationError::HttpStatus {
                code,
                status_text: r.status_text().to_string(),
            });
        }
        Err(e) => return Err(RevocationError::Http(e.to_string())),
    };
    use std::io::Read;
    let mut body = Vec::with_capacity(64 * 1024);
    resp.into_reader()
        .take(8 * 1024 * 1024) // 8 MiB cap — Ukrainian fiscal CRLs fit easily
        .read_to_end(&mut body)
        .map_err(|e| RevocationError::Http(format!("read body: {e}")))?;
    if body.starts_with(b"-----BEGIN") {
        return Err(RevocationError::Parse(
            "PEM-encoded CRL not supported — configure CA to serve DER".into(),
        ));
    }
    if body.is_empty() || body[0] != 0x30 {
        return Err(RevocationError::Parse(
            "CRL body is not a DER SEQUENCE".into(),
        ));
    }
    Ok(body)
}

// ─── BasicOCSPResponse status parser (Sprint 13 cert-watch) ───────────────

/// Parsed status of a single-cert OCSP response.
///
/// Shape mirrors the Python dict exposed by the pyfunction wrapper.
/// Signature verification is intentionally NOT performed — TLS to the
/// CA and the operational threat model (legitimate internal revocation,
/// not an attacker on the wire) make embedded signature checks
/// unnecessary for this use case.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OcspStatus {
    /// "good" | "revoked" | "unknown"
    pub status: &'static str,
    /// Unix seconds when the cert was revoked (revoked only).
    pub revoked_at: Option<u64>,
    /// Unix seconds — OCSP producedAt / thisUpdate.
    pub this_update: u64,
    /// Unix seconds — OCSP nextUpdate, if provided.
    pub next_update: Option<u64>,
    // `serial_matches: bool` was removed. The field was always-true
    // (the parser never compared serials). See the doc-comment on
    // `parse_ocsp_status()` for the serial-binding caveat.
}

/// Parse a BasicOCSPResponse DER blob and return the status of the
/// **first** `SingleResponse`. The parser is intentionally tolerant —
/// any deviation from the expected shape yields `RevocationError::Parse`
/// and the caller treats it as "unreachable / unknown".
///
/// # Serial-binding caveat
///
/// This function does **NOT** compare the `CertID.serialNumber` inside
/// the OCSP response against any expected serial. It takes the first
/// `SingleResponse` in the sequence unconditionally. In a multi-response
/// or reordered-response scenario (uncommon for single-cert probes but
/// possible with aggregated OCSP responders), the returned status may
/// belong to a **different certificate** than the one the caller asked
/// about.
///
/// For production cert-watch, the caller MUST cross-check that the
/// returned `this_update` / `next_update` window and the OCSP
/// responder's cert-ID field match the expected cert. A future API
/// revision (`parse_ocsp_status_for_serial(der, expected_serial)`)
/// will close this gap at the Rust layer.
pub fn parse_ocsp_status(basic_ocsp_der: &[u8]) -> Result<OcspStatus, RevocationError> {
    // BasicOCSPResponse ::= SEQUENCE { tbsResponseData, sigAlg, signature, [0]? certs }
    let (_, basic_inner) = a1::read_tlv(basic_ocsp_der, 0)?;
    // tbsResponseData ResponseData ::= SEQUENCE
    let (_, tbs_inner) = a1::read_tlv(basic_ocsp_der, basic_inner)?;

    let mut pos = tbs_inner;
    // version [0] EXPLICIT OPTIONAL — skip if present
    if pos < basic_ocsp_der.len() && basic_ocsp_der[pos] == 0xa0 {
        let (end, _) = a1::read_tlv(basic_ocsp_der, pos)?;
        pos = end;
    }
    // responderID CHOICE — [1] EXPLICIT Name | [2] EXPLICIT KeyHash
    if pos >= basic_ocsp_der.len() {
        return Err(RevocationError::Parse("missing responderID".into()));
    }
    let (rid_end, _) = a1::read_tlv(basic_ocsp_der, pos)?;
    pos = rid_end;
    // producedAt GeneralizedTime
    let (prod_end, _) = a1::read_tlv(basic_ocsp_der, pos)?;
    pos = prod_end;
    // responses SEQUENCE OF SingleResponse
    if pos >= basic_ocsp_der.len() || basic_ocsp_der[pos] != 0x30 {
        return Err(RevocationError::Parse(format!(
            "expected responses SEQUENCE OF, got tag {:#x}",
            if pos < basic_ocsp_der.len() { basic_ocsp_der[pos] } else { 0 }
        )));
    }
    let (resp_seq_end, resp_seq_inner) = a1::read_tlv(basic_ocsp_der, pos)?;
    if resp_seq_inner >= resp_seq_end {
        return Err(RevocationError::Parse("empty responses SEQUENCE".into()));
    }

    // Take the first SingleResponse.
    let (sr_end, sr_inner) = a1::read_tlv(basic_ocsp_der, resp_seq_inner)?;
    // SingleResponse ::= SEQUENCE { certID, certStatus, thisUpdate, [0]? nextUpdate, [1]? singleExts }
    // certID ::= SEQUENCE { hashAlg, issuerNameHash, issuerKeyHash, serialNumber }
    let (cid_end, _cid_inner) = a1::read_tlv(basic_ocsp_der, sr_inner)?;
    let mut sr_pos = cid_end;
    // certStatus CHOICE — IMPLICIT primitive tags:
    //   [0] good   = NULL   → tag 0x80 len 0
    //   [1] revoked= SEQUENCE(RevokedInfo) → tag 0xa1 (constructed, since it wraps SEQUENCE)
    //   [2] unknown= NULL   → tag 0x82 len 0
    if sr_pos >= sr_end {
        return Err(RevocationError::Parse("SingleResponse missing certStatus".into()));
    }
    let status_tag = basic_ocsp_der[sr_pos];
    let (status_end, status_inner) = a1::read_tlv(basic_ocsp_der, sr_pos)?;
    let (status_name, revoked_at) = match status_tag {
        0x80 => ("good", None),
        0xa1 | 0x81 => {
            // Revoked: RevokedInfo SEQUENCE { revocationTime GeneralizedTime, ... }
            // Constructed form is tagged 0xa1 (context-specific, constructed);
            // primitive form is 0x81 (some non-standard encodings). We tolerate both.
            if status_inner >= status_end {
                ("revoked", None)
            } else {
                let (rt_end, rt_inner) = a1::read_tlv(basic_ocsp_der, status_inner)?;
                let rt = parse_generalized_time(&basic_ocsp_der[rt_inner..rt_end]).ok();
                ("revoked", rt)
            }
        }
        0x82 => ("unknown", None),
        other => return Err(RevocationError::Parse(format!(
            "unexpected certStatus tag {other:#x}"
        ))),
    };
    sr_pos = status_end;
    // thisUpdate GeneralizedTime
    let (tu_end, tu_inner) = a1::read_tlv(basic_ocsp_der, sr_pos)?;
    let this_update = parse_generalized_time(&basic_ocsp_der[tu_inner..tu_end])
        .map_err(|e| RevocationError::Parse(format!("thisUpdate: {e}")))?;
    sr_pos = tu_end;
    // [0] EXPLICIT nextUpdate OPTIONAL
    let mut next_update: Option<u64> = None;
    if sr_pos < sr_end && basic_ocsp_der[sr_pos] == 0xa0 {
        let (nu_end, nu_ctx_inner) = a1::read_tlv(basic_ocsp_der, sr_pos)?;
        let (gt_end, gt_inner) = a1::read_tlv(basic_ocsp_der, nu_ctx_inner)?;
        if let Ok(v) = parse_generalized_time(&basic_ocsp_der[gt_inner..gt_end]) {
            next_update = Some(v);
        }
        sr_pos = nu_end;
    }
    let _ = sr_pos; // remaining extensions are ignored

    Ok(OcspStatus {
        status: status_name,
        revoked_at,
        this_update,
        next_update,
    })
}

/// Parse a GeneralizedTime content octets string into Unix seconds.
/// Accepts the common OCSP form "YYYYMMDDHHMMSSZ". Fractional seconds
/// and non-Z zones are not emitted by conforming OCSP responders so we
/// reject them to keep the parser small and fail-loud.
fn parse_generalized_time(content: &[u8]) -> Result<u64, String> {
    // Minimum shape: YYYYMMDDHHMMSS + 'Z' = 15 bytes. Optional
    // fractional seconds (".fff") may sit between the seconds and the
    // trailing 'Z' — RFC 5280 §4.1.2.5.2 allows but frowns on them;
    // IIS-based responders emit them anyway. We accept and discard.
    if content.len() < 15 {
        return Err(format!(
            "GeneralizedTime too short: {} bytes",
            content.len()
        ));
    }
    let last = content[content.len() - 1];
    if last != b'Z' {
        return Err(format!(
            "GeneralizedTime must end in 'Z', got {:#x}",
            last
        ));
    }
    // Validate that any bytes between offset 14 and the final 'Z' look
    // like a fractional-second string (`.<digits>`).
    if content.len() > 15 {
        if content[14] != b'.' {
            return Err(format!(
                "unexpected GeneralizedTime byte at 14: {:#x}",
                content[14]
            ));
        }
        for &b in &content[15..content.len() - 1] {
            if !b.is_ascii_digit() {
                return Err(format!(
                    "non-digit inside fractional-second: {:#x}",
                    b
                ));
            }
        }
    }

    let s = std::str::from_utf8(&content[..14])
        .map_err(|e| format!("GeneralizedTime utf8: {e}"))?;
    let parse = |start: usize, len: usize| -> Result<u32, String> {
        s[start..start + len]
            .parse::<u32>()
            .map_err(|e| { let end = start + len; format!("parse {start}..{end}: {e}") })
    };
    let year = parse(0, 4)?;
    let month = parse(4, 2)?;
    let day = parse(6, 2)?;
    let hour = parse(8, 2)?;
    let minute = parse(10, 2)?;
    let second = parse(12, 2)?;
    ymd_hms_to_unix(year, month, day, hour, minute, second)
        .ok_or_else(|| format!("invalid calendar values: {year}-{month}-{day} {hour}:{minute}:{second}"))
}

/// Convert a Gregorian UTC date/time to Unix seconds without external
/// deps. Handles the 1970-2099 range conservatively — good enough for
/// OCSP validity windows, which never span that long. Returns `None`
/// for out-of-range calendar values (month 0, day 32, hour 25…).
fn ymd_hms_to_unix(year: u32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Option<u64> {
    // Range validation — OCSP responders shouldn't emit nonsense, but
    // a malformed GeneralizedTime on the wire (corrupt by bit-flip or
    // bad encoder) must surface loudly rather than quietly produce a
    // "revoked 1970-01-01" record that confuses the cert-watch loop.
    if !(1970..=2199).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60  // DER allows leap seconds (60)
    {
        return None;
    }
    // Days-from-epoch via civil calendar algorithm (Howard Hinnant).
    let y = if month <= 2 { year as i64 - 1 } else { year as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = month as u64;
    let d = day as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_from_epoch = era * 146097 + doe as i64 - 719468;
    let unix = days_from_epoch * 86400
        + (hour as i64) * 3600
        + (minute as i64) * 60
        + second as i64;
    Some(unix.max(0) as u64)
}

// ─── revocation-values unsigned attribute encoder ─────────────────────────

/// Encode the *value* of the `id-aa-ets-revocationValues` unsigned
/// attribute: a `RevocationValues` SEQUENCE containing two optional
/// SEQUENCE OFs (CRLs, OCSP responses). Takes raw DERs — no parsing.
///
/// ```text
/// RevocationValues ::= SEQUENCE {
///     crlVals      [0] EXPLICIT SEQUENCE OF CertificateList       OPTIONAL,
///     ocspVals     [1] EXPLICIT SEQUENCE OF BasicOCSPResponse     OPTIONAL,
///     otherRevVals [2] EXPLICIT OtherRevVals                      OPTIONAL
/// }
/// ```
///
/// Returns the content bytes ready to wrap in `dw::sequence(...)` to
/// form the attribute value, OR to pass to the builder's unsigned-attr
/// embedding helper.
pub fn encode_revocation_values(
    crls_der: &[Vec<u8>],
    ocsp_responses_der: &[Vec<u8>],
) -> Vec<u8> {
    let mut inner = Vec::new();
    if !crls_der.is_empty() {
        let mut seq = Vec::new();
        for c in crls_der {
            seq.extend_from_slice(c);
        }
        let seq_of = dw::sequence(&seq);
        inner.extend_from_slice(&dw::explicit_context_tag(0, &seq_of));
    }
    if !ocsp_responses_der.is_empty() {
        let mut seq = Vec::new();
        for o in ocsp_responses_der {
            seq.extend_from_slice(o);
        }
        let seq_of = dw::sequence(&seq);
        inner.extend_from_slice(&dw::explicit_context_tag(1, &seq_of));
    }
    dw::sequence(&inner)
}

// DER helpers now live in `crate::cms::asn1_util`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocsp_request_encoding_is_deterministic_and_structural() {
        let n_hash = [0x11u8; 32];
        let k_hash = [0x22u8; 32];
        let serial = hex::decode("020412345678").unwrap(); // INTEGER 0x12345678
        let a = encode_ocsp_request(&n_hash, &k_hash, &serial, None).unwrap();
        let b = encode_ocsp_request(&n_hash, &k_hash, &serial, None).unwrap();
        assert_eq!(a, b);
        assert_eq!(a[0], 0x30);
        assert!(a.windows(32).any(|w| w == n_hash));
        assert!(a.windows(32).any(|w| w == k_hash));
    }

    #[test]
    fn ocsp_request_with_nonce_differs_from_none() {
        let n_hash = [0u8; 32];
        let k_hash = [0u8; 32];
        let serial = hex::decode("02020001").unwrap();
        let a = encode_ocsp_request(&n_hash, &k_hash, &serial, None).unwrap();
        let b = encode_ocsp_request(&n_hash, &k_hash, &serial, Some(&[0xAA; 16])).unwrap();
        assert_ne!(a, b);
        assert!(b.len() > a.len());
    }

    #[test]
    fn ocsp_response_extracts_basic_on_success() {
        // Synthetic response wrapping a minimal SEQUENCE inside the
        // OCTET STRING (our parser now requires the inner payload to
        // start with 0x30 = BasicOCSPResponse SEQUENCE).
        //
        // Structure:
        //   30 1F  OCSPResponse SEQUENCE (31)
        //     0A 01 00        responseStatus = 0 (successful)
        //     A0 1A           [0] EXPLICIT (26)
        //       30 18         ResponseBytes SEQUENCE (24)
        //         06 09 2B... responseType = id-pkix-ocsp-basic
        //         04 0B       response OCTET STRING (11)
        //           30 09     inner SEQUENCE ("BasicOCSPResponse")
        //             42 41 53 49 43 4F 43 53 50  "BASICOCSP"
        let resp = hex::decode(
            "301F0A0100A01A3018 06092B0601050507300101 040B300942415349434F435350"
                .replace(' ', ""),
        )
        .unwrap();
        let inner = parse_ocsp_response(&resp).expect("granted");
        // The returned bytes are the OCTET STRING content = the SEQUENCE.
        assert_eq!(&inner[0..2], &[0x30, 0x09]);
        assert_eq!(&inner[2..], b"BASICOCSP");
    }

    #[test]
    fn ocsp_response_rejects_wrong_response_type() {
        // Same structure but responseType = OID 1.2.3.4 (not basic).
        let resp = hex::decode(
            "30170A0100A01230100603 2A0304 04094241534943 4F435350"
                .replace(' ', ""),
        )
        .unwrap();
        match parse_ocsp_response(&resp) {
            Err(RevocationError::Parse(msg)) => {
                assert!(msg.contains("not id-pkix-ocsp-basic"), "got: {msg}");
            }
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn ocsp_response_rejects_non_successful() {
        // responseStatus = 1 (malformedRequest), no responseBytes
        let resp = hex::decode("30030A0101").unwrap();
        match parse_ocsp_response(&resp).unwrap_err() {
            RevocationError::OcspRejected(s) => assert_eq!(s, 1),
            other => panic!("expected OcspRejected, got {other:?}"),
        }
    }

    #[test]
    fn revocation_values_shape_empty_is_empty_sequence() {
        let v = encode_revocation_values(&[], &[]);
        // Empty SEQUENCE: 30 00
        assert_eq!(v, vec![0x30, 0x00]);
    }

    // ─── parse_ocsp_status: synthesised BasicOCSPResponse vectors ─────

    /// Build a minimal BasicOCSPResponse with one SingleResponse.
    /// `cert_status_der` is the CertStatus CHOICE alternative TLV:
    ///   good:    [0x80, 0x00]
    ///   revoked: 0xA1 len RevokedInfo(SEQUENCE{GeneralizedTime})
    ///   unknown: [0x82, 0x00]
    fn build_basic_ocsp(cert_status_der: &[u8]) -> Vec<u8> {
        // certID SEQUENCE { hashAlg AlgId, issuerNameHash OCT, issuerKeyHash OCT, serial INT }
        let alg = dw::algorithm_identifier(oids::GOST_34_311_95).unwrap();
        let inh = dw::octet_string(&[0x11u8; 4]);
        let ikh = dw::octet_string(&[0x22u8; 4]);
        let serial = vec![0x02, 0x01, 0x01]; // INTEGER 1
        let mut cid_inner = Vec::new();
        cid_inner.extend_from_slice(&alg);
        cid_inner.extend_from_slice(&inh);
        cid_inner.extend_from_slice(&ikh);
        cid_inner.extend_from_slice(&serial);
        let cert_id = dw::sequence(&cid_inner);
        // thisUpdate = GeneralizedTime "20260101000000Z" (20 bytes TLV)
        let this_update_content = b"20260101000000Z";
        let mut this_update = Vec::with_capacity(2 + this_update_content.len());
        this_update.push(0x18); // GeneralizedTime
        this_update.push(this_update_content.len() as u8);
        this_update.extend_from_slice(this_update_content);
        // SingleResponse SEQUENCE { certID, certStatus, thisUpdate }
        let mut sr_inner = Vec::new();
        sr_inner.extend_from_slice(&cert_id);
        sr_inner.extend_from_slice(cert_status_der);
        sr_inner.extend_from_slice(&this_update);
        let single_response = dw::sequence(&sr_inner);
        let responses = dw::sequence(&single_response);

        // responderID — [1] EXPLICIT Name (SEQUENCE empty for simplicity).
        let rid_name = dw::sequence(&[]);
        let rid = dw::explicit_context_tag(1, &rid_name);
        // producedAt GeneralizedTime
        let produced_at_content = b"20260101000000Z";
        let mut produced_at = Vec::with_capacity(2 + produced_at_content.len());
        produced_at.push(0x18);
        produced_at.push(produced_at_content.len() as u8);
        produced_at.extend_from_slice(produced_at_content);

        // ResponseData SEQUENCE { responderID, producedAt, responses }
        let mut rd_inner = Vec::new();
        rd_inner.extend_from_slice(&rid);
        rd_inner.extend_from_slice(&produced_at);
        rd_inner.extend_from_slice(&responses);
        let tbs = dw::sequence(&rd_inner);

        // signatureAlgorithm AlgorithmIdentifier (GOST OID, NULL params)
        let sig_alg = dw::algorithm_identifier(oids::GOST_34_311_95).unwrap();
        // signature BIT STRING — empty body with leading "unused bits" = 0
        let signature = vec![0x03, 0x01, 0x00];

        // BasicOCSPResponse SEQUENCE { tbs, sigAlg, signature }
        let mut b_inner = Vec::new();
        b_inner.extend_from_slice(&tbs);
        b_inner.extend_from_slice(&sig_alg);
        b_inner.extend_from_slice(&signature);
        dw::sequence(&b_inner)
    }

    #[test]
    fn parse_ocsp_status_good() {
        // certStatus = [0] IMPLICIT NULL → 0x80 0x00
        let basic = build_basic_ocsp(&[0x80, 0x00]);
        let parsed = parse_ocsp_status(&basic).expect("parse good");
        assert_eq!(parsed.status, "good");
        assert!(parsed.revoked_at.is_none());
        assert!(parsed.this_update > 0);
    }

    #[test]
    fn parse_ocsp_status_revoked() {
        // RevokedInfo SEQUENCE { revocationTime GeneralizedTime "20260101000000Z" }
        // Content: 18 0F 32 30 32 36 30 31 30 31 30 30 30 30 30 30 5A
        let mut revoked_info_inner = Vec::new();
        let rt_content = b"20260101000000Z";
        revoked_info_inner.push(0x18);
        revoked_info_inner.push(rt_content.len() as u8);
        revoked_info_inner.extend_from_slice(rt_content);
        // Wrap in [1] IMPLICIT SEQUENCE == tag 0xa1.
        let mut wrapped = Vec::with_capacity(2 + revoked_info_inner.len());
        wrapped.push(0xa1);
        wrapped.push(revoked_info_inner.len() as u8);
        wrapped.extend_from_slice(&revoked_info_inner);

        let basic = build_basic_ocsp(&wrapped);
        let parsed = parse_ocsp_status(&basic).expect("parse revoked");
        assert_eq!(parsed.status, "revoked");
        assert!(parsed.revoked_at.is_some());
    }

    #[test]
    fn parse_ocsp_status_unknown() {
        // certStatus = [2] IMPLICIT UnknownInfo (NULL) → 0x82 0x00
        let basic = build_basic_ocsp(&[0x82, 0x00]);
        let parsed = parse_ocsp_status(&basic).expect("parse unknown");
        assert_eq!(parsed.status, "unknown");
        assert!(parsed.revoked_at.is_none());
    }

    #[test]
    fn revocation_values_carries_both_crls_and_ocsp() {
        let crl = vec![0x30, 0x04, 0xCA, 0xFE, 0xBA, 0xBE];
        let ocsp = vec![0x30, 0x04, 0xDE, 0xAD, 0xBE, 0xEF];
        let v = encode_revocation_values(&[crl.clone()], &[ocsp.clone()]);
        // Outer SEQUENCE tag
        assert_eq!(v[0], 0x30);
        assert!(v.windows(crl.len()).any(|w| w == crl));
        assert!(v.windows(ocsp.len()).any(|w| w == ocsp));
        // Must contain [0] EXPLICIT (A0) and [1] EXPLICIT (A1) wrappers
        assert!(v.iter().any(|&b| b == 0xA0));
        assert!(v.iter().any(|&b| b == 0xA1));
    }
}
