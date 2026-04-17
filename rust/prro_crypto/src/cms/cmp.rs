//! Ukrainian CA cert-lookup-by-SKI client.
//!
//! Reverse-engineered from `dstucrypt/agent` (`jkurwa/lib/services/cmp.js`).
//! Despite the "CMP" folder name on the server side
//! (`/services/cmp/`), this is **not** RFC 4210 CMP — it's an IIT
//! proprietary binary protocol wrapped in a PKCS#7 `id-data`
//! ContentInfo. We keep the local module name `cmp` for continuity
//! with the server path and the osplus.ini nomenclature, but the wire
//! format below is what real Ukrainian CAs (acskidd, czo, uakey via
//! routing) actually answer to.
//!
//! ## Request wire format
//!
//! Inner 120-byte payload (little-endian integers, big-endian raw SKIs):
//!
//! ```text
//! byte 0x00       0x0D                  (message type = cert lookup)
//! byte 0x01..08   0x00                  (reserved)
//! byte 0x08..0C   02 00 00 00           (u32 LE = 2, probably version)
//! byte 0x0C..2C   SKI[0]  (32 bytes)    (primary SKI — the one we want)
//! byte 0x2C..4C   SKI[1]  (32 bytes)    (secondary — copy of SKI[0] in
//!                                        single-key lookups)
//! byte 0x4C..6C   0x00                  (reserved, 32 bytes of zeros)
//! byte 0x6C       0x01                  (flag, sub-1.3-style "one key")
//! byte 0x6D..70   0x00                  (reserved)
//! byte 0x70       0x01                  (flag)
//! byte 0x71..78   0x00                  (reserved)
//! ```
//!
//! That 120-byte blob is then wrapped as:
//!
//! ```text
//! ContentInfo ::= SEQUENCE {
//!     contentType  OID 1.2.840.113549.1.7.1 (id-data),
//!     content      [0] EXPLICIT OCTET STRING (120-byte payload)
//! }
//! ```
//!
//! POSTed to `http://acskidd.gov.ua/services/cmp/` (HTTP port 80, as
//! the ІІТ `osplus.ini` says — the server actually honours HTTP on its
//! `/services/` path and only 301-redirects for `/`).
//!
//! ## Response wire format
//!
//! Also a `ContentInfo` with `id-data`; the OCTET STRING payload holds:
//!
//! ```text
//! byte 0x00..04  header (ignored)
//! byte 0x04..08  u32 LE status (1 = success; anything else = failure)
//! byte 0x08+     nested ContentInfo — PKCS#7 SignedData — carrying
//!                the requested cert(s) under SignedData.certificates
//! ```
//!
//! We walk the nested SignedData structurally to pull out the first
//! `Certificate SEQUENCE` matching the queried SKI and return it as
//! DER bytes.

use thiserror::Error;

use crate::cms::asn1_util::{self as a1, Asn1Error};
use crate::cms::der_writer as dw;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CmpError {
    #[error("DER write: {0}")]
    DerWrite(#[from] crate::cms::der_writer::DerWriterError),
    #[error("ASN.1: {0}")]
    Asn1(#[from] Asn1Error),
    #[error("parse: {0}")]
    Parse(String),
    #[error("server rejected lookup: status={0}")]
    ServerError(u32),
    /// Network/transport failure (DNS, TLS, TCP, timeout, read error).
    /// Downstream retry logic typically matches on this vs `HttpStatus`.
    #[error("HTTP transport: {0}")]
    Http(String),
    /// The HTTP call completed but the server returned a non-200 code.
    /// Surfaced separately so the caller can short-circuit retry loops
    /// on 4xx (client/config error) vs 5xx/5xx (retry-friendly).
    #[error("HTTP {code} {status_text}")]
    HttpStatus { code: u16, status_text: String },
    #[error("cert not found by SKI")]
    NotFound,
    #[error("SKI must be exactly 32 bytes, got {0}")]
    BadSkiLen(usize),
    #[error("response structurally too deep (possible DoS)")]
    ResponseTooDeep,
}

// id-data = 1.2.840.113549.1.7.1
const ID_DATA_OID_DER: &[u8] = &[
    0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01,
];

const IIT_PAYLOAD_LEN: usize = 120;
const IIT_MSG_TYPE_CERT_LOOKUP: u8 = 0x0D;
const IIT_STATUS_SUCCESS: u32 = 1;

// ─── Request encoder ─────────────────────────────────────────────────────

/// Build the full request blob ready to POST. `ski` must be exactly 32
/// bytes — the GOST34311-based Subject Key Identifier of the cert we want.
pub fn encode_iit_cert_lookup(ski: &[u8]) -> Result<Vec<u8>, CmpError> {
    if ski.len() != 32 {
        return Err(CmpError::BadSkiLen(ski.len()));
    }
    // Inner 120-byte "black magic" payload.
    let mut payload = vec![0u8; IIT_PAYLOAD_LEN];
    payload[0x00] = IIT_MSG_TYPE_CERT_LOOKUP;
    // u32 LE = 2 at offset 8
    payload[0x08] = 0x02;
    payload[0x0C..0x2C].copy_from_slice(ski); // primary SKI
    payload[0x2C..0x4C].copy_from_slice(ski); // secondary — same SKI
    payload[0x6C] = 0x01;
    payload[0x70] = 0x01;

    // Wrap as ContentInfo { id-data, [0] EXPLICIT OCTET STRING payload }
    let octet = dw::octet_string(&payload);
    let content_explicit = dw::explicit_context_tag(0, &octet);
    let mut ci_inner = Vec::with_capacity(ID_DATA_OID_DER.len() + content_explicit.len());
    ci_inner.extend_from_slice(ID_DATA_OID_DER);
    ci_inner.extend_from_slice(&content_explicit);
    Ok(dw::sequence(&ci_inner))
}

// ─── Response parser ─────────────────────────────────────────────────────

/// Parse a response blob and return the DER bytes of the first
/// Certificate SEQUENCE whose SKI matches `expected_ski`. Returns
/// `NotFound` if no matching cert is in the reply, `ServerError` if
/// the IIT status word is not 1, `Parse` on structural issues.
pub fn parse_iit_cert_response(
    body: &[u8],
    expected_ski: &[u8],
) -> Result<Vec<u8>, CmpError> {
    // Outer: ContentInfo SEQUENCE { OID id-data, [0] EXPLICIT OCTET STRING }
    let (_, ci_inner) = a1::read_tlv(body, 0)?;
    let (oid_end, _) = a1::read_tlv(body, ci_inner)?;

    let ctx_tag = a1::peek_tag(body, oid_end)?;
    if ctx_tag != 0xa0 {
        return Err(CmpError::Parse(format!(
            "expected [0] EXPLICIT content, got tag {ctx_tag:#x}"
        )));
    }
    let (ctx_end, ctx_inner) = a1::read_tlv(body, oid_end)?;

    let oct_tag = a1::peek_tag(body, ctx_inner)?;
    if oct_tag != 0x04 {
        return Err(CmpError::Parse(format!(
            "expected OCTET STRING inside [0], got tag {oct_tag:#x}"
        )));
    }
    let (oct_end, oct_inner) = a1::read_tlv(body, ctx_inner)?;
    if oct_end > ctx_end {
        return Err(CmpError::Parse(
            "inner OCTET STRING overruns [0] wrapper".into(),
        ));
    }
    let iit = &body[oct_inner..oct_end];
    if iit.len() < 8 {
        return Err(CmpError::Parse(format!(
            "IIT payload too short: {} bytes",
            iit.len()
        )));
    }
    let status = a1::read_u32_le(iit, 4)?;
    if status != IIT_STATUS_SUCCESS {
        return Err(CmpError::ServerError(status));
    }
    let inner = &iit[8..];
    find_cert_with_ski_iterative(inner, expected_ski)?.ok_or(CmpError::NotFound)
}

/// Iterative walk over a DER blob for the first X.509 Certificate
/// SEQUENCE whose computed SKI matches `expected_ski`.
///
/// Explicitly non-recursive to prevent a stack overflow on a hostile
/// CA response with deeply-nested SEQUENCEs (a one-line craft: `30 04
/// 30 02 30 00 …` scales unboundedly inside the 1 MiB body cap we
/// grant the HTTP fetcher). Each frame pushed onto the work stack
/// counts against a hard depth cap (`MAX_DER_DEPTH`) and a TLV-count
/// cap (`MAX_TLV_VISITS`) — both are many orders of magnitude above
/// any realistic CA reply while still bounding memory + time.
///
/// Returns `Ok(Some(cert_der))` on hit, `Ok(None)` on a well-formed
/// blob with no matching cert, `Err(ResponseTooDeep)` on hostile nesting.
fn find_cert_with_ski_iterative(
    data: &[u8],
    expected_ski: &[u8],
) -> Result<Option<Vec<u8>>, CmpError> {
    const MAX_DER_DEPTH: u32 = 32;
    const MAX_TLV_VISITS: u32 = 8192;

    // Each stack frame is a half-open byte range inside `data` plus the
    // current nesting depth. We seed with the top-level range.
    struct Frame {
        start: usize,
        end: usize,
        depth: u32,
    }
    let mut stack = Vec::<Frame>::with_capacity(16);
    stack.push(Frame { start: 0, end: data.len(), depth: 0 });

    let mut visits = 0u32;
    // Track whether any frame encountered a malformed TLV. If the
    // walker finishes without finding a cert AND at least one frame
    // hit a parse issue, the error is `Parse` (corrupt/truncated CA
    // response) rather than `NotFound` (well-formed response that
    // simply didn't contain the requested cert).
    let mut had_parse_error = false;
    while let Some(Frame { start, end, depth }) = stack.pop() {
        if depth > MAX_DER_DEPTH {
            return Err(CmpError::ResponseTooDeep);
        }
        let mut pos = start;
        while pos < end {
            visits += 1;
            if visits > MAX_TLV_VISITS {
                return Err(CmpError::ResponseTooDeep);
            }
            let tag = match a1::peek_tag(data, pos) {
                Ok(t) => t,
                Err(_) => { had_parse_error = true; break; }
            };
            let (tlv_end, content_start) = match a1::read_tlv(data, pos) {
                Ok(v) => v,
                Err(_) => { had_parse_error = true; break; }
            };

            if tag == 0x30 {
                let cert_slice = &data[pos..tlv_end];
                if looks_like_certificate(cert_slice) {
                    if let Ok(pubkey_bytes) =
                        crate::cms::envelope::extract_cert_pubkey_bytes(cert_slice)
                    {
                        let ski = crate::cms::envelope::compute_ski(&pubkey_bytes);
                        if ski.as_slice() == expected_ski {
                            return Ok(Some(cert_slice.to_vec()));
                        }
                    }
                }
                // Not a matching cert — schedule the inner range for
                // traversal in case certs are nested one level deeper.
                stack.push(Frame {
                    start: content_start,
                    end: tlv_end,
                    depth: depth + 1,
                });
            } else if (0xA0..=0xBF).contains(&tag) {
                stack.push(Frame {
                    start: content_start,
                    end: tlv_end,
                    depth: depth + 1,
                });
            }
            // Any other tag (primitive OID/OCTET STRING/INTEGER/…) —
            // skip without descending.
            pos = tlv_end;
        }
    }
    if had_parse_error {
        Err(CmpError::Parse(
            "response contains malformed TLV — cannot determine whether \
             the requested cert is present; treating as corrupt, not as \
             NotFound".into(),
        ))
    } else {
        Ok(None)
    }
}

/// Quick-and-cheap heuristic: is this SEQUENCE shaped like an X.509
/// Certificate? Required structure: SEQUENCE { tbsCertificate SEQ,
/// signatureAlgorithm SEQ, signature BIT STRING }. We don't validate
/// deeply — just that a tbsCertificate SEQUENCE exists at the start
/// and a BIT STRING sits at the tail; this rejects most non-cert
/// SEQUENCEs (RDNs, extension lists, etc.) without expensive parsing.
fn looks_like_certificate(seq_der: &[u8]) -> bool {
    let Ok((_, inner_start)) = a1::read_tlv(seq_der, 0) else {
        return false;
    };
    // First child must be a SEQUENCE (tbsCertificate).
    if a1::peek_tag(seq_der, inner_start).ok() != Some(0x30) {
        return false;
    }
    let Ok((tbs_end, _)) = a1::read_tlv(seq_der, inner_start) else {
        return false;
    };
    // Next child is signatureAlgorithm SEQUENCE.
    if a1::peek_tag(seq_der, tbs_end).ok() != Some(0x30) {
        return false;
    }
    let Ok((sig_alg_end, _)) = a1::read_tlv(seq_der, tbs_end) else {
        return false;
    };
    // Final child is BIT STRING (0x03) signature.
    a1::peek_tag(seq_der, sig_alg_end).ok() == Some(0x03)
}

// ─── HTTP client (feature = "tsp_http") ─────────────────────────────────

/// Blocking HTTP round-trip: encode, POST to `cmp_url`, parse, verify
/// that the returned cert actually has the SKI we asked for.
///
/// `cmp_url` should be the full URL e.g.
/// `http://acskidd.gov.ua/services/cmp/`. We keep port 80 (HTTP) per
/// the osplus.ini default — `ca.tax.gov.ua` happily answers our POST
/// on plain HTTP even though browser traffic is 301'd to HTTPS.
#[cfg(feature = "tsp_http")]
pub fn fetch_cert_by_ski(
    cmp_url: &str,
    ski: &[u8],
    timeout: std::time::Duration,
) -> Result<Vec<u8>, CmpError> {
    let req_bytes = encode_iit_cert_lookup(ski)?;

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .redirects(0) // don't 301 POST away — it would lose the body
        .build();

    let resp = match agent
        .post(cmp_url)
        .set("Content-Type", "application/octet-stream")
        .set("Content-Length", &req_bytes.len().to_string())
        .send_bytes(&req_bytes)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            return Err(CmpError::HttpStatus {
                code,
                status_text: r.status_text().to_string(),
            });
        }
        Err(e) => return Err(CmpError::Http(e.to_string())),
    };

    let mut body = Vec::with_capacity(8192);
    use std::io::Read;
    resp.into_reader()
        .take(1024 * 1024) // 1 MiB ceiling — a single cert + chain fits easily
        .read_to_end(&mut body)
        .map_err(|e| CmpError::Http(format!("read body: {e}")))?;

    parse_iit_cert_response(&body, ski)
}

// DER/int helpers now live in `crate::cms::asn1_util`.

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKI: [u8; 32] = [
        0xFD, 0xC5, 0x99, 0x01, 0xEB, 0xA8, 0xCC, 0x05, 0x11, 0x73, 0xD2, 0x98, 0x96, 0xA0,
        0xEC, 0xEE, 0x90, 0x45, 0x4F, 0x12, 0x90, 0x8F, 0x27, 0x55, 0x80, 0xE4, 0x55, 0x1C,
        0x67, 0x9A, 0xD1, 0x3B,
    ];

    #[test]
    fn encode_produces_well_formed_iit_request() {
        let msg = encode_iit_cert_lookup(&SAMPLE_SKI).unwrap();
        assert_eq!(msg[0], 0x30, "outer SEQUENCE");
        // OID id-data must appear.
        assert!(msg.windows(ID_DATA_OID_DER.len()).any(|w| w == ID_DATA_OID_DER));
        // SKI must appear at least once.
        assert!(msg.windows(SAMPLE_SKI.len()).any(|w| w == SAMPLE_SKI));
        // Final byte (or near final) of the octet string must carry the
        // IIT msg-type byte 0x0D preceded by the message-type marker.
        // We assert the 120-byte inner payload shape by round-trip:
        let (_, ci_inner) = a1::read_tlv(&msg, 0).unwrap();
        let (oid_end, _) = a1::read_tlv(&msg, ci_inner).unwrap();
        let (_, ctx_inner) = a1::read_tlv(&msg, oid_end).unwrap();
        let (oct_end, oct_inner) = a1::read_tlv(&msg, ctx_inner).unwrap();
        let inner = &msg[oct_inner..oct_end];
        assert_eq!(inner.len(), IIT_PAYLOAD_LEN);
        assert_eq!(inner[0], IIT_MSG_TYPE_CERT_LOOKUP);
        assert_eq!(inner[0x08], 0x02);
        assert_eq!(&inner[0x0C..0x2C], &SAMPLE_SKI);
        assert_eq!(&inner[0x2C..0x4C], &SAMPLE_SKI);
        assert_eq!(inner[0x6C], 0x01);
        assert_eq!(inner[0x70], 0x01);
    }

    #[test]
    fn encode_rejects_wrong_size_ski() {
        let err = encode_iit_cert_lookup(&[0u8; 20]).unwrap_err();
        match err {
            CmpError::BadSkiLen(20) => {}
            other => panic!("expected BadSkiLen(20), got {other:?}"),
        }
    }

    /// A fabricated response with status=0 must surface `ServerError`.
    #[test]
    fn parse_rejects_non_success_status() {
        // Inner IIT payload: 4 bytes header + 4 bytes status(5) + nothing else.
        let mut iit = vec![0u8; 8];
        iit[4] = 0x05; // LE u32 = 5
        let octet = dw::octet_string(&iit);
        let ctx = dw::explicit_context_tag(0, &octet);
        let mut ci = Vec::new();
        ci.extend_from_slice(ID_DATA_OID_DER);
        ci.extend_from_slice(&ctx);
        let body = dw::sequence(&ci);
        let err = parse_iit_cert_response(&body, &SAMPLE_SKI).unwrap_err();
        match err {
            CmpError::ServerError(5) => {}
            other => panic!("expected ServerError(5), got {other:?}"),
        }
    }

    /// Status=1 but no cert inside → NotFound. Uses a minimal 20-byte
    /// inner blob so the SignedData-walker has nothing to latch onto.
    #[test]
    fn parse_success_without_cert_returns_not_found() {
        let mut iit = vec![0u8; 20];
        iit[4] = 0x01; // status = 1
        // bytes 8..20: junk SEQUENCE that's not a Certificate
        iit[8] = 0x30;
        iit[9] = 0x0a;
        // 10 bytes of garbage — not enough to look like a cert
        let octet = dw::octet_string(&iit);
        let ctx = dw::explicit_context_tag(0, &octet);
        let mut ci = Vec::new();
        ci.extend_from_slice(ID_DATA_OID_DER);
        ci.extend_from_slice(&ctx);
        let body = dw::sequence(&ci);
        let err = parse_iit_cert_response(&body, &SAMPLE_SKI).unwrap_err();
        match err {
            CmpError::NotFound => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    /// Positive test: a synthetic successful response wrapping a real
    /// DSTU cert (the director cert from WebCheck). The parser must
    /// walk the SignedData-ish nested structure, compute the SKI of
    /// the embedded cert, match it against `expected_ski`, and return
    /// the cert DER verbatim.
    #[test]
    fn parse_returns_matching_cert_from_real_dstu_response() {
        let cert_path = "/mnt/c/ProgramData/WebCheck/Keys/CA-0882240800000000000000000000000000000001.cer";
        let cert = match std::fs::read(cert_path) { Ok(d) => d, Err(_) => return };

        // Build the IIT payload: 8-byte header + a fake SignedData-shell
        // (a SEQUENCE that contains the cert). find_cert_with_ski's
        // recursive walker doesn't care about the SignedData tag chain
        // as long as it eventually meets a Certificate SEQUENCE.
        let mut iit = vec![0u8; 8];
        iit[4] = 0x01;
        iit.extend_from_slice(&dw::sequence(&cert));

        let octet = dw::octet_string(&iit);
        let ctx = dw::explicit_context_tag(0, &octet);
        let mut ci = Vec::new();
        ci.extend_from_slice(ID_DATA_OID_DER);
        ci.extend_from_slice(&ctx);
        let body = dw::sequence(&ci);

        let out = parse_iit_cert_response(&body, &SAMPLE_SKI).unwrap();
        assert_eq!(out, cert, "returned cert must equal the embedded one");
    }

    /// Regression: hostile response with pathologically deep SEQUENCE
    /// nesting must return `ResponseTooDeep`, not blow the stack.
    #[test]
    fn parse_rejects_pathologically_deep_response() {
        // Construct 200 nested SEQUENCEs by wrapping from the inside out.
        let mut nested = vec![0x30, 0x00]; // innermost empty SEQUENCE
        for _ in 0..200 {
            let len = nested.len();
            // Short-form length works up to 127; longer uses long form.
            if len < 0x80 {
                let mut next = Vec::with_capacity(len + 2);
                next.push(0x30);
                next.push(len as u8);
                next.extend_from_slice(&nested);
                nested = next;
            } else {
                let mut next = Vec::with_capacity(len + 4);
                next.push(0x30);
                next.push(0x82);
                next.push((len >> 8) as u8);
                next.push(len as u8);
                next.extend_from_slice(&nested);
                nested = next;
            }
        }

        // Wrap as a successful IIT response so we reach the walker.
        let mut iit = vec![0u8; 8];
        iit[4] = 0x01; // status = success
        iit.extend_from_slice(&nested);
        let octet = dw::octet_string(&iit);
        let ctx = dw::explicit_context_tag(0, &octet);
        let mut ci = Vec::new();
        ci.extend_from_slice(ID_DATA_OID_DER);
        ci.extend_from_slice(&ctx);
        let body = dw::sequence(&ci);

        match parse_iit_cert_response(&body, &SAMPLE_SKI) {
            Err(CmpError::ResponseTooDeep) => {}
            other => panic!("expected ResponseTooDeep, got {other:?}"),
        }
    }

    /// Live test against the real Ukrainian CA frontend. Requires
    /// network; `#[ignore]` so CI doesn't accidentally run it.
    /// Run with: `cargo test --lib cmp:: -- --ignored --nocapture`
    #[test]
    #[ignore]
    #[cfg(feature = "tsp_http")]
    fn live_acskidd_lookup_roundtrips_ski() {
        // SKI of ДПС's own signing cert (EK_S_NEW.cer in WebCheck).
        // acskidd knows its own keys, so this is the minimal "does the
        // wire protocol work at all?" test that doesn't depend on
        // cross-CA routing.
        const DPS_OWN_SKI: [u8; 32] = [
            0x14, 0xED, 0xC2, 0x06, 0x97, 0xBD, 0x37, 0x23, 0x93, 0xCA, 0x35, 0xA0,
            0x1E, 0x12, 0x4E, 0x9E, 0xC0, 0xA2, 0xCA, 0x01, 0x39, 0xFB, 0x7F, 0xB2,
            0xBC, 0x5C, 0x81, 0xC9, 0x2E, 0x13, 0xB0, 0x37,
        ];
        let cert = fetch_cert_by_ski(
            "http://acskidd.gov.ua/services/cmp/",
            &DPS_OWN_SKI,
            std::time::Duration::from_secs(15),
        )
        .expect("live IIT-CMP lookup");
        assert!(!cert.is_empty());
        assert_eq!(cert[0], 0x30, "returned blob must be DER SEQUENCE");
        let pub_bytes = crate::cms::envelope::extract_cert_pubkey_bytes(&cert)
            .expect("extract pubkey from returned cert");
        let roundtrip = crate::cms::envelope::compute_ski(&pub_bytes);
        assert_eq!(
            roundtrip.as_slice(),
            DPS_OWN_SKI.as_slice(),
            "returned cert's SKI must match the query"
        );
    }

    /// Third CA. JKS director key's SKI (computed earlier in
    /// `e2e_phase1_jks_ski_round_trip`) should resolve on PrivatBank's
    /// CA. Same IIT wire protocol, only the host differs.
    #[test]
    #[ignore]
    #[cfg(feature = "tsp_http")]
    fn live_privatbank_lookup_roundtrips_jks_ski() {
        const JKS_DIRECTOR_SKI: [u8; 32] = [
            0x22, 0x6B, 0xC6, 0x89, 0x9C, 0x05, 0x58, 0x33, 0x12, 0xD1, 0x5B, 0x8F,
            0x2C, 0x64, 0xF1, 0xE5, 0x84, 0x2F, 0x21, 0x98, 0x62, 0xC0, 0xF2, 0xF1,
            0x5E, 0x96, 0x9B, 0x93, 0x49, 0xA7, 0x74, 0x9E,
        ];
        let cert = fetch_cert_by_ski(
            "http://acsk.privatbank.ua/services/cmp/",
            &JKS_DIRECTOR_SKI,
            std::time::Duration::from_secs(15),
        )
        .expect("live PrivatBank lookup");
        assert!(!cert.is_empty());
        assert_eq!(cert[0], 0x30);
        let pub_bytes = crate::cms::envelope::extract_cert_pubkey_bytes(&cert)
            .expect("extract pubkey");
        let roundtrip = crate::cms::envelope::compute_ski(&pub_bytes);
        assert_eq!(roundtrip.as_slice(), JKS_DIRECTOR_SKI.as_slice());
    }

    /// Cross-CA live test: the director cert's SKI (АЦСК "Україна"
    /// issuer) should resolve against `uakey.com.ua` — the correct CA
    /// for that key, since acskidd.gov.ua only serves ДПС-issued certs.
    #[test]
    #[ignore]
    #[cfg(feature = "tsp_http")]
    fn live_uakey_lookup_roundtrips_director_ski() {
        let cert = fetch_cert_by_ski(
            "http://uakey.com.ua/services/cmp/",
            &SAMPLE_SKI, // director SKI FDC59901...D13B
            std::time::Duration::from_secs(15),
        )
        .expect("live uakey lookup");
        assert!(!cert.is_empty());
        assert_eq!(cert[0], 0x30);
        let pub_bytes = crate::cms::envelope::extract_cert_pubkey_bytes(&cert)
            .expect("extract pubkey");
        let roundtrip = crate::cms::envelope::compute_ski(&pub_bytes);
        assert_eq!(roundtrip.as_slice(), SAMPLE_SKI.as_slice());
    }
}
