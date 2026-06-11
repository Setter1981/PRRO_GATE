//! RFC 3161 Time-Stamp Protocol (TSP) client + CAdES-T helpers.
//!
//! Scope:
//! - Encode `TimeStampReq` (minimal — version + messageImprint only, no
//!   policy/nonce/certReq flags; matches jkurwa's wire format).
//! - Parse `TimeStampResp` — check PKIStatus, return the embedded
//!   `TimeStampToken` (a re-encoded `ContentInfo`) as DER bytes ready
//!   to plug into a CAdES-T unsigned attribute.
//! - Extract the TSA URL from a signer's certificate `SubjectInfoAccess`
//!   extension (as jkurwa does — no hardcoded TSA).
//!
//! HTTP round-trip lives in [`fetch_timestamp`] (Etap 2); this module
//! keeps ASN.1 and I/O separate so the encoder/parser can be unit-tested
//! without a network.

use thiserror::Error;

use crate::cms::asn1_util::{self as a1, Asn1Error};
use crate::cms::der_writer as dw;
use crate::cms::oids;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TspError {
    #[error("DER write: {0}")]
    DerWrite(#[from] crate::cms::der_writer::DerWriterError),
    #[error("ASN.1: {0}")]
    Asn1(#[from] Asn1Error),
    #[error("TSP response: {0}")]
    Parse(String),
    #[error("TSA rejected request: PKIStatus={status}{detail}",
            detail = detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default())]
    Rejected {
        status: u64,
        detail: Option<String>,
    },
    #[error("HTTP transport: {0}")]
    Http(String),
    #[error("HTTP {code} {status_text}")]
    HttpStatus { code: u16, status_text: String },
}

/// Build a minimal RFC 3161 `TimeStampReq` DER blob.
///
/// ```text
/// TimeStampReq ::= SEQUENCE {
///     version         INTEGER { v1(1) },
///     messageImprint  SEQUENCE {
///         hashAlgorithm AlgorithmIdentifier { Gost34311, NULL },
///         hashedMessage OCTET STRING
///     }
///     -- reqPolicy/nonce/certReq/extensions omitted (jkurwa parity)
/// }
/// ```
pub fn encode_tsp_request(digest: &[u8]) -> Result<Vec<u8>, TspError> {
    let alg = dw::algorithm_identifier(oids::GOST_34_311_95)?;
    let hashed = dw::octet_string(digest);

    let mut mi_inner = Vec::with_capacity(alg.len() + hashed.len());
    mi_inner.extend_from_slice(&alg);
    mi_inner.extend_from_slice(&hashed);
    let message_imprint = dw::sequence(&mi_inner);

    let version = dw::integer_u32(1);

    let mut outer = Vec::with_capacity(version.len() + message_imprint.len());
    outer.extend_from_slice(&version);
    outer.extend_from_slice(&message_imprint);
    Ok(dw::sequence(&outer))
}

/// Parse an RFC 3161 `TimeStampResp` and return the DER bytes of the
/// embedded `TimeStampToken` (itself a `ContentInfo` wrapping SignedData).
///
/// On PKIStatus != granted(0), returns [`TspError::Rejected`].
pub fn parse_tsp_response(resp_der: &[u8]) -> Result<Vec<u8>, TspError> {
    let (outer_end, outer_inner) = a1::read_tlv(resp_der, 0)?;
    // status PKIStatusInfo SEQUENCE
    let (status_end, status_inner) = a1::read_tlv(resp_der, outer_inner)?;
    // status INTEGER
    let (s_int_end, s_int_start) = a1::read_tlv(resp_der, status_inner)?;
    let status = a1::read_integer_be_small(resp_der, s_int_start, s_int_end)?;

    if status != 0 && status != 1 {
        // 0 granted, 1 grantedWithMods — both are success per RFC 3161 §2.4.2
        let detail = if s_int_end < status_end {
            Some(format!(
                "response body starts at +{}; {} bytes of status detail",
                status_inner,
                status_end - s_int_end
            ))
        } else {
            None
        };
        return Err(TspError::Rejected {
            status,
            detail,
        });
    }

    // timeStampToken ContentInfo — reassemble its DER (tag+len+content)
    // by slicing from the original bytes. We must stay inside the outer
    // SEQUENCE's declared boundary: a malformed TSR claiming a token
    // longer than `outer_end - status_end` would otherwise spill.
    let token_start = status_end;
    if token_start >= outer_end {
        return Err(TspError::Parse(
            "granted TSR has no timeStampToken payload".into(),
        ));
    }
    // TimeStampToken is a ContentInfo — must be a SEQUENCE (0x30).
    let token_tag = a1::peek_tag(resp_der, token_start)?;
    if token_tag != 0x30 {
        return Err(TspError::Parse(format!(
            "timeStampToken must be SEQUENCE (0x30), got tag {token_tag:#x}"
        )));
    }
    let (token_end, _) = a1::read_tlv(resp_der, token_start)?;
    if token_end > outer_end {
        return Err(TspError::Parse(format!(
            "timeStampToken overruns outer SEQUENCE: token_end={token_end}, outer_end={outer_end}"
        )));
    }
    Ok(resp_der[token_start..token_end].to_vec())
}

/// Extract the TSA URL from the SubjectInfoAccess (SIA) X.509 extension
/// of a signer's certificate. Matches jkurwa's convention
/// (`cert.extension.subjectInfoAccess.link`).
///
/// Walk: Certificate → tbsCertificate → extensions [3] → each Extension →
/// extnID == id-pe-subjectInfoAccess → extnValue OCTET STRING →
/// SEQUENCE OF AccessDescription → find `id-ad-timeStamping` → URI.
pub fn tsa_url_from_cert(cert_der: &[u8]) -> Result<String, TspError> {
    let (_, tbs_start) = a1::read_tlv(cert_der, 0)?;
    let (tbs_end, tbs_inner) = a1::read_tlv(cert_der, tbs_start)?;

    let mut pos = tbs_inner;
    // version [0] EXPLICIT
    if a1::peek_tag(cert_der, pos).ok() == Some(0xa0) {
        let (end, _) = a1::read_tlv(cert_der, pos)?;
        pos = end;
    }
    // serialNumber, signature, issuer, validity, subject, spki — skip 6
    for _ in 0..6 {
        let (end, _) = a1::read_tlv(cert_der, pos)?;
        pos = end;
    }
    // Bound scan to tbsCertificate — don't wander into sig bytes.
    while pos < tbs_end {
        let tag = a1::peek_tag(cert_der, pos)?;
        let (end, inner) = a1::read_tlv(cert_der, pos)?;
        if tag == 0xa3 {
            // Extensions wrapper [3] EXPLICIT — slice to inner end
            let (ext_seq_end, ext_seq_inner) = a1::read_tlv(cert_der, inner)?;
            return find_sia_timestamping_url(&cert_der[ext_seq_inner..ext_seq_end]);
        }
        pos = end;
    }
    Err(TspError::Parse(
        "certificate has no extensions block".into(),
    ))
}

fn find_sia_timestamping_url(ext_list_bytes: &[u8]) -> Result<String, TspError> {
    // Walk SEQUENCE OF Extension. Each Extension is itself a SEQUENCE.
    // We treat ext_list_bytes as the content of the outer SEQUENCE OF.
    let mut pos = 0usize;
    while pos < ext_list_bytes.len() {
        let (end, inner) = a1::read_tlv(ext_list_bytes, pos)?;
        // Extension ::= SEQUENCE { extnID OID, critical BOOLEAN OPT, extnValue OCTET STRING }
        let (oid_end, oid_inner) = a1::read_tlv(ext_list_bytes, inner)?;
        let oid_bytes = &ext_list_bytes[oid_inner..oid_end];

        // Match id-pe-subjectInfoAccess (1.3.6.1.5.5.7.1.11) = 2B 06 01 05 05 07 01 0B
        const SIA_OID: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x01, 0x0B];
        if oid_bytes == SIA_OID {
            // Skip optional critical BOOLEAN; extnValue is the final TLV.
            let mut inner_pos = oid_end;
            // critical BOOLEAN — only consume if the TLV shape matches
            // `01 01 <value>` to avoid colliding with a context tag.
            if inner_pos < end
                && a1::peek_tag(ext_list_bytes, inner_pos).ok() == Some(0x01)
                && ext_list_bytes.get(inner_pos + 1).copied() == Some(0x01)
            {
                let (b_end, _) = a1::read_tlv(ext_list_bytes, inner_pos)?;
                inner_pos = b_end;
            }
            // extnValue OCTET STRING
            let (_ev_end, ev_inner) = a1::read_tlv(ext_list_bytes, inner_pos)?;
            let (sia_end, sia_seq_inner) = a1::read_tlv(ext_list_bytes, ev_inner)?;
            return parse_sia_access_list(&ext_list_bytes[sia_seq_inner..sia_end]);
        }
        pos = end;
    }
    Err(TspError::Parse(
        "no SubjectInfoAccess extension in cert".into(),
    ))
}

fn parse_sia_access_list(sia_bytes: &[u8]) -> Result<String, TspError> {
    // SubjectInfoAccessSyntax ::= SEQUENCE SIZE (1..MAX) OF AccessDescription
    // AccessDescription ::= SEQUENCE { accessMethod OID, accessLocation GeneralName }
    // Our target accessMethod: 1.3.6.1.5.5.7.48.3 = 2B 06 01 05 05 07 30 03
    const TS_OID: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x03];

    let mut pos = 0usize;
    while pos < sia_bytes.len() {
        let (end, inner) = a1::read_tlv(sia_bytes, pos)?;
        let (am_end, am_inner) = a1::read_tlv(sia_bytes, inner)?;
        let am_bytes = &sia_bytes[am_inner..am_end];
        if am_bytes == TS_OID {
            // accessLocation is GeneralName — for HTTP URIs it's
            // context-tagged [6] IMPLICIT IA5String.
            let tag_byte = a1::peek_tag(sia_bytes, am_end)?;
            let (al_end, al_inner) = a1::read_tlv(sia_bytes, am_end)?;
            if tag_byte == 0x86 {
                let uri = &sia_bytes[al_inner..al_end];
                let raw = std::str::from_utf8(uri)
                    .map_err(|e| TspError::Parse(format!("SIA URI not UTF-8: {e}")))?;
                // SSRF gate: only HTTP(S) URIs are safe to auto-fetch.
                if !raw.starts_with("http://") && !raw.starts_with("https://") {
                    return Err(TspError::Parse(format!(
                        "SIA timeStamping URI scheme not HTTP(S): {raw}"
                    )));
                }
                return Ok(raw.to_string());
            }
            return Err(TspError::Parse(format!(
                "SIA timeStamping accessLocation has unsupported GeneralName tag {:#x}",
                tag_byte
            )));
        }
        pos = end;
    }
    Err(TspError::Parse(
        "no id-ad-timeStamping entry in SubjectInfoAccess".into(),
    ))
}

// ─── HTTP TSP client (feature = "tsp_http") ───────────────────────────────

/// Blocking HTTP round-trip against an RFC 3161 TSA.
///
/// Sends the encoded `TimeStampReq` as `application/tsp-request` and
/// returns the DER bytes of the `TimeStampToken` on success. `timeout`
/// bounds both connect and read phases; pick something small (≤ 10 s)
/// — fiscal signing is on the hot path.
///
/// No retry, no backoff: callers that need it should wrap this function.
/// Ukrainian TSAs are internal infrastructure; a blip is better surfaced
/// than hidden by a silent retry that blows past the PRRO SLA.
#[cfg(feature = "tsp_http")]
pub fn fetch_timestamp(
    tsa_url: &str,
    digest: &[u8],
    timeout: std::time::Duration,
) -> Result<Vec<u8>, TspError> {
    let req_bytes = encode_tsp_request(digest)?;

    // `.redirects(0)` is load-bearing: a 301 on a POST can lose the
    // request body. TSA hosts are usually HTTPS already, but the
    // property should hold regardless of operator config.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .redirects(0)
        .build();

    // ureq 2.x returns `Ok(Response)` ONLY for 2xx. Non-2xx comes
    // back as `Err(Error::Status(code, resp))`, transport failures as
    // `Err(Error::Transport(...))`. We match explicitly so the typed
    // `HttpStatus` / `Http` distinction actually reaches callers.
    let resp = match agent
        .post(tsa_url)
        .set("Content-Type", "application/tsp-request")
        .set("Accept", "application/tsp-response")
        .send_bytes(&req_bytes)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            return Err(TspError::HttpStatus {
                code,
                status_text: r.status_text().to_string(),
            });
        }
        Err(e) => return Err(TspError::Http(e.to_string())),
    };

    let mut body = Vec::with_capacity(4096);
    use std::io::Read;
    resp.into_reader()
        .take(512 * 1024) // 512 KiB ceiling — tokens with full chain fit well
        .read_to_end(&mut body)
        .map_err(|e| TspError::Http(format!("read body: {e}")))?;

    parse_tsp_response(&body)
}

// DER helpers now live in `crate::cms::asn1_util`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsp_request_deterministic_and_well_formed() {
        let digest = vec![0xAB; 32];
        let a = encode_tsp_request(&digest).unwrap();
        let b = encode_tsp_request(&digest).unwrap();
        assert_eq!(a, b, "encoding must be deterministic");
        assert_eq!(a[0], 0x30, "starts with SEQUENCE");
        // Must contain the GOST 34.311 OID bytes.
        const GOST_OID_BYTES: &[u8] =
            &[0x2A, 0x86, 0x24, 0x02, 0x01, 0x01, 0x01, 0x01, 0x02, 0x01];
        assert!(a.windows(GOST_OID_BYTES.len()).any(|w| w == GOST_OID_BYTES));
        assert!(a.windows(digest.len()).any(|w| w == digest));
    }

    #[test]
    fn tsp_request_rejects_distinct_digests() {
        let a = encode_tsp_request(&[0x00u8; 32]).unwrap();
        let b = encode_tsp_request(&[0xFFu8; 32]).unwrap();
        assert_ne!(a, b);
    }

    /// Synthetic TimeStampResp with PKIStatus=granted and a minimal token.
    /// The token here is a placeholder — the parser only extracts it as
    /// DER bytes; verification of the TST's SignedData is out of scope.
    #[test]
    fn tsp_response_extracts_token_on_granted() {
        // PKIStatusInfo = SEQUENCE { INTEGER 0 } → 30 03 02 01 00
        // timeStampToken = SEQUENCE { OCTET STRING "tst" } → 30 05 04 03 74 73 74
        let resp = hex::decode("300C3003020100300504037473 74".replace(' ', ""))
            .unwrap();
        let token = parse_tsp_response(&resp).expect("granted");
        // Token must be the SEQUENCE at position 5..12 of input.
        assert_eq!(token, vec![0x30, 0x05, 0x04, 0x03, 0x74, 0x73, 0x74]);
    }

    #[test]
    fn tsp_response_rejects_non_granted_status() {
        // PKIStatusInfo = SEQUENCE { INTEGER 2 (rejection) } and no token.
        let resp = hex::decode("30053003020102").unwrap();
        let err = parse_tsp_response(&resp).unwrap_err();
        match err {
            TspError::Rejected { status, .. } => assert_eq!(status, 2),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn sia_extraction_on_real_cert_gracefully_fails_when_absent() {
        // Vendored from jkurwa's test data (CRY-3 follow-up: the old path
        // reached into sidecar/node_modules, which does not exist in CI).
        let cert_der = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/SELF_SIGNED1.cer"
        ))
        .expect("read test cert");
        // Self-signed test cert has no SIA. The parser must return a
        // typed parse error rather than panic.
        match tsa_url_from_cert(&cert_der) {
            Err(TspError::Parse(_)) => {}
            Ok(url) => panic!("unexpected URL from test cert: {url}"),
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}
