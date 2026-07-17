//! Typed DTO wrappers for the DPS gRPC transport.
//!
//! Public `DpsChannel` API consumes / returns these typed shapes; raw
//! `tonic`/`prost` generated structs from `super::gen` stay
//! crate-private so the transport contract is reviewable independently
//! of upstream proto-codegen drift (and so a future W4 byte-equivalence
//! test does not have to assert on prost-generated `Default::default`).
//!
//! Field set is the proto field set, mapped to plain owned types — no
//! repr trick, no zero-copy borrow lifetimes.  C3 (this commit) wires
//! the `From<DpsCheckType> for gen::check::Type`, `From<CheckEnvelope>
//! for gen::Check`, and `try_decode_*_response` dispatchers used by
//! `GrpcDpsChannel`'s RPC bodies.

/// Subset of `Check.Type` proto values the wrapper accepts.  `UNKNOWN`
/// is intentionally absent — the proto's UNKNOWN means the field was
/// missing on the wire, which we treat as a decode error rather than a
/// queryable variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpsCheckType {
    /// Standard receipt / refund / arrival document.
    Chk,
    /// Z-report (shift close).
    ZReport,
    /// Service receipt (cash drop / payout, no fiscal effect).
    ServiceChk,
}

/// Caller-side input for `send_chk` and `ping`.  Mirrors `proto Check`
/// one-to-one but enforces a typed `DpsCheckType` enum.
#[derive(Debug, Clone)]
pub struct CheckEnvelope {
    /// Fiscal number string (`rro_fn`).
    pub rro_fn: String,
    /// DPS-specific "Kyiv-local-as-epoch": the wall-clock time in
    /// `Europe/Kiev` interpreted as if it were UTC, then converted to
    /// epoch seconds.  This is NOT a true UTC epoch — DPS expects the
    /// Kyiv-local timestamp on the wire and the existing Python
    /// transport encodes it that way.  Callers in M3+ MUST NOT pass a
    /// raw `Utc::now().timestamp()`; the conversion to Kyiv-local
    /// epoch happens in `services/` before reaching this DTO.
    pub date_time: i64,
    /// CMS-signed receipt blob.  Borrow elided to keep the type
    /// `'static`-safe for `Arc<dyn DpsChannel>` patterns.
    pub check_sign: Vec<u8>,
    /// Per-FN local document number (lnd).
    pub local_number: i32,
    pub check_type: DpsCheckType,
    /// Optional offline-mode document id (hex string).  Empty → online.
    pub id_offline: String,
    /// Optional cancellation reference id.  Empty → not a cancellation.
    pub id_cancel: String,
}

/// Caller-side input for `last_chk`, `status_rro`, `info_rro` — and the
/// transport half of `by_server_fiscal_no`.  The wire field
/// `rro_fn_sign` is a CMS-signed blob containing the FN + caller
/// metadata; we type it as opaque bytes here.
#[derive(Debug, Clone)]
pub struct CheckSignBlob(pub Vec<u8>);

/// Successful `CheckResponse` payload (status == OK).  Server-side
/// non-OK statuses surface as `DpsError::Server { code, message }`,
/// not as a Variant of this struct.
#[derive(Debug, Clone)]
pub struct CheckAck {
    /// Server-assigned fiscal id.  Used by `by_server_fiscal_no` to
    /// match the response against the expected id (PRRO_GATE-5js).
    pub id: String,
    pub id_sign: Vec<u8>,
    pub data_sign: Vec<u8>,
}

/// The minimum plausible length of a real KVT1 `data_sign` quittance. A genuine
/// KVT1 evidence is a DSTU-4145 CMS SignedData blob — hundreds to thousands of
/// bytes (a bare DSTU-4145-256 signature alone is 64 bytes; a live-observed KVT1
/// was ~2500 bytes). Anything shorter is not a signature, so it is NOT a valid
/// KVT1 quittance and must NOT advance a doc to KVT1/ACK — RISK 1 fail-closed
/// harden against a byzantine-but-alive DPS returning non-empty garbage evidence.
/// The bound is deliberately conservative (a bare-signature floor) so it can never
/// false-reject a real quittance. Callers already reject the empty case; this
/// subsumes it (`0 < MIN_KVT1_DATA_SIGN_LEN`).
pub const MIN_KVT1_DATA_SIGN_LEN: usize = 64;

/// Successful `StatusResponse` payload (status == OK).
#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    pub open_shift: bool,
    pub online: bool,
    pub last_signer: String,
}

/// Single operator entry from `RroInfoResponse.Operator`.
#[derive(Debug, Clone)]
pub struct DpsOperator {
    pub serial: String,
    pub status: i32,
    pub senior: bool,
    pub isname: String,
}

/// Successful T=112 ASK_OFFLINE_CODES response: the DPS-assigned offline
/// code IDs parsed from the `data_sign` CMS payload.
///
/// Each element of `codes` is an opaque ASCII string (observed live:
/// `"eYme-jhnkWQ"`).  DPS may return zero codes if the quota is already
/// satisfied, or multiple codes when SIZE > 1 is requested.
#[derive(Debug, Clone)]
pub struct OfflineCodesResponse {
    pub codes: Vec<String>,
}

/// Successful `RroInfoResponse` payload (status == OK).
#[derive(Debug, Clone)]
pub struct RroInfo {
    pub status_rro: i32,
    pub open_shift: bool,
    pub online: bool,
    pub last_signer: String,
    pub name: String,
    pub name_to: String,
    pub addr: String,
    pub single_tax: bool,
    pub offline_allowed: bool,
    pub add_num: i32,
    pub pn: String,
    pub operators: Vec<DpsOperator>,
    pub tins: String,
    pub lnum: i32,
    pub name_pay: String,
}

// ─── Typed-DTO ↔ generated-prost conversions (crate-private) ───────────

use super::error::{AuthorizationKind, DpsError};
use super::gen;

impl From<DpsCheckType> for gen::check::Type {
    fn from(t: DpsCheckType) -> Self {
        match t {
            DpsCheckType::Chk => gen::check::Type::Chk,
            DpsCheckType::ZReport => gen::check::Type::Zreport,
            DpsCheckType::ServiceChk => gen::check::Type::Servicechk,
        }
    }
}

impl From<CheckEnvelope> for gen::Check {
    fn from(e: CheckEnvelope) -> Self {
        gen::Check {
            rro_fn: e.rro_fn,
            date_time: e.date_time,
            check_sign: e.check_sign,
            local_number: e.local_number,
            check_type: gen::check::Type::from(e.check_type) as i32,
            id_offline: e.id_offline,
            id_cancel: e.id_cancel,
        }
    }
}

impl From<&CheckSignBlob> for gen::CheckRequest {
    fn from(b: &CheckSignBlob) -> Self {
        gen::CheckRequest {
            rro_fn_sign: b.0.clone(),
        }
    }
}

/// SHA-256 digest of a decoded DPS response envelope (re-encoded canonically via
/// prost).  Captures the parsed reply as lossless raw-reply evidence for
/// [`DpsError::Indeterminate`] (R3): deterministic, and distinct for distinct
/// replies — so a later Bridge maps `Indeterminate` to
/// `SendIndeterminate::UnknownStatus` carrying the SAME digest, never a fabricated
/// one.  A re-encode (not the wire bytes) is used because tonic decodes the
/// protobuf before this layer; prost's encoding is canonical/deterministic, so the
/// digest is a stable fingerprint of the response content.
fn response_digest<M: prost::Message>(m: &M) -> prro_domain::delivery::RawResponseDigest {
    use prro_domain::delivery::RawResponseDigest;
    use sha2::{Digest, Sha256};
    RawResponseDigest(Sha256::digest(m.encode_to_vec()).into())
}

/// Dispatch a `gen::CheckResponse` onto either `Ok(CheckAck)` or a
/// typed `DpsError`.  Status-code mapping (per W0-1 review):
///
/// - `Ok` (1) → `Ok(CheckAck)`
/// - `Unknown` (0) → `Decode` (proto3 default = field was missing
///   on the wire)
/// - `ErrorVerefy` (-1) → `Authorization`
/// - `ErrorNotRegisteredRro` (-13) → `Authorization`
/// - `ErrorNotRegisteredSigner` (-14) → `Authorization`
/// - `ErrorUnknown` (-4) → `Indeterminate { code: -4 }` (parsed DPS
///   envelope whose status does not settle submission certainty; slice
///   A re-types this so later slices can distinguish "possibly
///   submitted" from "definitely not sent" — retry class unchanged)
/// - everything else → `Server { code, message }`
pub(crate) fn try_decode_check_response(r: gen::CheckResponse) -> Result<CheckAck, DpsError> {
    use gen::check_response::Status;
    let st = Status::try_from(r.status).map_err(|_| {
        DpsError::Decode(format!(
            "unknown CheckResponse.status raw value {}",
            r.status
        ))
    })?;
    match st {
        Status::Ok => Ok(CheckAck {
            id: r.id,
            id_sign: r.id_sign,
            data_sign: r.data_sign,
        }),
        Status::Unknown => Err(DpsError::Decode(
            "CheckResponse.status missing on the wire (Unknown=0)".into(),
        )),
        // ADR-M3-A6 prereq: split authorization-class statuses by kind
        // so the routing layer (W7/W10) can distinguish per-doc rejects
        // (-1) from per-FN registration failures (-13/-14).
        Status::ErrorVerefy => Err(DpsError::Authorization {
            code: Status::ErrorVerefy as i32,
            kind: AuthorizationKind::DocumentReject,
            message: format!("{}: {}", st.as_str_name(), r.error_message),
        }),
        Status::ErrorNotRegisteredRro | Status::ErrorNotRegisteredSigner => {
            Err(DpsError::Authorization {
                code: st as i32,
                kind: AuthorizationKind::FiscalNumberNotRegistered,
                message: format!("{}: {}", st.as_str_name(), r.error_message),
            })
        }
        Status::ErrorUnknown => {
            // R3: capture the parsed reply as lossless raw-reply evidence BEFORE
            // moving `error_message` out of `r`.
            let digest = response_digest(&r);
            Err(DpsError::Indeterminate {
                code: -4,
                message: r.error_message,
                digest,
            })
        }
        other => Err(DpsError::Server {
            code: other as i32,
            message: r.error_message,
        }),
    }
}

/// Same dispatch shape for `StatusResponse` — the proto's status enum
/// is a strict subset of `CheckResponse`'s, so the routing rules
/// match one-to-one.
pub(crate) fn try_decode_status_response(
    r: gen::StatusResponse,
) -> Result<StatusSnapshot, DpsError> {
    use gen::status_response::Status;
    let st = Status::try_from(r.status).map_err(|_| {
        DpsError::Decode(format!(
            "unknown StatusResponse.status raw value {}",
            r.status
        ))
    })?;
    match st {
        Status::Ok => Ok(StatusSnapshot {
            open_shift: r.open_shift,
            online: r.online,
            last_signer: r.last_signer,
        }),
        Status::Unknown => Err(DpsError::Decode(
            "StatusResponse.status missing on the wire (Unknown=0)".into(),
        )),
        // ADR-M3-A6 prereq — same split as CheckResponse decoder above.
        Status::ErrorVerefy => Err(DpsError::Authorization {
            code: Status::ErrorVerefy as i32,
            kind: AuthorizationKind::DocumentReject,
            message: format!("{}: {}", st.as_str_name(), r.error_message),
        }),
        Status::ErrorNotRegisteredRro | Status::ErrorNotRegisteredSigner => {
            Err(DpsError::Authorization {
                code: st as i32,
                kind: AuthorizationKind::FiscalNumberNotRegistered,
                message: format!("{}: {}", st.as_str_name(), r.error_message),
            })
        }
        Status::ErrorUnknown => {
            // R3: capture the parsed reply as lossless raw-reply evidence BEFORE
            // moving `error_message` out of `r`.
            let digest = response_digest(&r);
            Err(DpsError::Indeterminate {
                code: -4,
                message: r.error_message,
                digest,
            })
        }
        other => Err(DpsError::Server {
            code: other as i32,
            message: r.error_message,
        }),
    }
}

// ─── T=112 offline-codes decode ─────────────────────────────────────────────

/// Parse `<ID>…</ID>` elements from the inner XML of a T=112 response.
///
/// Called by [`decode_offline_codes`] after the CMS envelope is stripped.
/// The XML is declared `windows-1251` but the ID payloads are ASCII-safe;
/// we parse as UTF-8 (a strict superset for ASCII content).
///
/// Handles both observed shapes:
/// - `<RS V="1"><C T="112"><ID>…</ID></C></RS>` (live capture)
/// - `<C T="112"><ID>…</ID></C>` (no RS wrapper)
///
/// Returns `Ok(vec![])` when no `<ID>` elements are present (empty quota or
/// error path with an empty `<C>` block).  Returns `Err(DpsError::Decode)`
/// on an unclosed `<ID>` or UTF-8 failure.
pub fn parse_offline_codes_xml(xml: &[u8]) -> Result<Vec<String>, DpsError> {
    let s = std::str::from_utf8(xml).map_err(|e| {
        DpsError::Decode(format!(
            "offline codes XML is not valid UTF-8 (expected ASCII-safe windows-1251): {e}"
        ))
    })?;
    const OPEN: &str = "<ID>";
    const CLOSE: &str = "</ID>";
    let mut codes = Vec::new();
    let mut rest = s;
    loop {
        match rest.find(OPEN) {
            None => break,
            Some(start) => {
                let after_open = &rest[start + OPEN.len()..];
                match after_open.find(CLOSE) {
                    None => {
                        return Err(DpsError::Decode(
                            "unclosed <ID> element in offline codes XML".into(),
                        ))
                    }
                    Some(end) => {
                        codes.push(after_open[..end].to_string());
                        rest = &after_open[end + CLOSE.len()..];
                    }
                }
            }
        }
    }
    Ok(codes)
}

/// Strip the CMS `SignedData` envelope from a T=112 `data_sign` blob, then
/// parse the `<ID>` offline-code elements from the inner XML.
///
/// Used by [`super::grpc::GrpcDpsChannel::ask_offline_codes`] to convert the
/// raw gRPC `CheckAck.data_sign` field into a typed [`OfflineCodesResponse`].
///
/// Two-stage decode:
/// 1. `prro_crypto::cms::signed_data::extract_econtent` — strips the outer
///    CMS `SignedData` DER, returns the raw `eContent` octets.
/// 2. [`parse_offline_codes_xml`] — scans for `<ID>…</ID>` elements.
pub(crate) fn decode_offline_codes(data_sign: &[u8]) -> Result<OfflineCodesResponse, DpsError> {
    let inner = prro_crypto::cms::signed_data::extract_econtent(data_sign).map_err(|e| {
        DpsError::Decode(format!("T=112 data_sign: CMS eContent strip failed: {e}"))
    })?;
    let codes = parse_offline_codes_xml(&inner)?;
    Ok(OfflineCodesResponse { codes })
}

/// Same dispatch shape for `RroInfoResponse`.  `RroInfoResponse` does
/// not carry an `error_message` field, so server-side errors here
/// surface with an empty message; callers that need to render it
/// fall back to the typed status code.
pub(crate) fn try_decode_rro_info_response(r: gen::RroInfoResponse) -> Result<RroInfo, DpsError> {
    use gen::rro_info_response::Status;
    let st = Status::try_from(r.status).map_err(|_| {
        DpsError::Decode(format!(
            "unknown RroInfoResponse.status raw value {}",
            r.status
        ))
    })?;
    match st {
        Status::Ok => Ok(RroInfo {
            status_rro: r.status_rro,
            open_shift: r.open_shift,
            online: r.online,
            last_signer: r.last_signer,
            name: r.name,
            name_to: r.name_to,
            addr: r.addr,
            single_tax: r.single_tax,
            offline_allowed: r.offline_allowed,
            add_num: r.add_num,
            pn: r.pn,
            operators: r
                .operators
                .into_iter()
                .map(|o| DpsOperator {
                    serial: o.serial,
                    status: o.status,
                    senior: o.senior,
                    isname: o.isname,
                })
                .collect(),
            tins: r.tins,
            lnum: r.lnum,
            name_pay: r.name_pay,
        }),
        Status::Unknown => Err(DpsError::Decode(
            "RroInfoResponse.status missing on the wire (Unknown=0)".into(),
        )),
        // ADR-M3-A6 prereq — same split as the two decoders above.
        // RroInfoResponse has no error_message field, so the message
        // carries the typed status name only.
        Status::ErrorVerefy => Err(DpsError::Authorization {
            code: Status::ErrorVerefy as i32,
            kind: AuthorizationKind::DocumentReject,
            message: st.as_str_name().to_string(),
        }),
        Status::ErrorNotRegisteredRro | Status::ErrorNotRegisteredSigner => {
            Err(DpsError::Authorization {
                code: st as i32,
                kind: AuthorizationKind::FiscalNumberNotRegistered,
                message: st.as_str_name().to_string(),
            })
        }
        Status::ErrorUnknown => {
            // R3: lossless raw-reply digest of the RroInfoResponse envelope.
            let digest = response_digest(&r);
            Err(DpsError::Indeterminate {
                code: -4,
                message: "ERROR_UNKNOWN (-4) on RroInfoResponse".into(),
                digest,
            })
        }
        other => Err(DpsError::Server {
            code: other as i32,
            message: String::new(),
        }),
    }
}

// ─── Unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transports::dps::error::DpsError;

    /// RP4B-3 pin: `try_decode_check_response` on a `-4` (ERROR_UNKNOWN)
    /// CheckResponse must yield `DpsError::Indeterminate { code: -4, .. }`
    /// — NOT `DpsError::Transport`.  The two types are distinct signals:
    /// `Transport` = no response (network/TLS/timeout); `Indeterminate` =
    /// parsed DPS envelope that does not settle submission certainty.
    ///
    /// This pin is RED until `DpsError::Indeterminate` is added to
    /// `error.rs` and the `-4` arm in each decoder is updated.
    #[test]
    fn rp4b_3_neg4_check_response_yields_indeterminate_not_transport() {
        use crate::transports::dps::gen::check_response::Status;
        let resp = gen::CheckResponse {
            status: Status::ErrorUnknown as i32,
            error_message: "transient server error".to_string(),
            ..Default::default()
        };
        let err = try_decode_check_response(resp).expect_err("ErrorUnknown must error");
        assert!(
            matches!(err, DpsError::Indeterminate { code: -4, .. }),
            "expected Indeterminate{{code:-4}}, got {err:?}"
        );
    }

    /// RP4B-3 companion: `try_decode_check_response` on a `-4` must NOT
    /// produce `DpsError::Transport`.  Transport is reserved for
    /// genuine network/TLS/deadline failures that have no DPS envelope.
    #[test]
    fn rp4b_3_neg4_check_response_is_not_transport() {
        use crate::transports::dps::gen::check_response::Status;
        let resp = gen::CheckResponse {
            status: Status::ErrorUnknown as i32,
            error_message: "transient".to_string(),
            ..Default::default()
        };
        let err = try_decode_check_response(resp).expect_err("ErrorUnknown must error");
        assert!(
            !matches!(err, DpsError::Transport(_)),
            "ERROR_UNKNOWN must NOT produce Transport; got {err:?}"
        );
    }

    /// R3 (lossless raw-reply seam): `Indeterminate` carries a `RawResponseDigest`
    /// derived from the ACTUAL parsed envelope, so the Bridge maps it losslessly
    /// (no fabricated digest). Different replies → different digests; identical
    /// replies → identical digest (deterministic). Teeth: a constant/fabricated
    /// digest would fail the `assert_ne!`.
    #[test]
    fn r3_indeterminate_carries_reply_digest_not_fabricated() {
        use crate::transports::dps::gen::check_response::Status;
        let mk = |msg: &str| {
            let resp = gen::CheckResponse {
                status: Status::ErrorUnknown as i32,
                error_message: msg.to_string(),
                ..Default::default()
            };
            match try_decode_check_response(resp) {
                Err(DpsError::Indeterminate { digest, .. }) => digest,
                other => panic!("expected Indeterminate, got {other:?}"),
            }
        };
        let da = mk("reply A");
        let db = mk("reply B");
        let da2 = mk("reply A");
        assert_ne!(da, db, "different replies must yield different digests");
        assert_eq!(
            da, da2,
            "identical replies must yield identical digest (deterministic)"
        );
    }

    /// RP4B-3 companion: `try_decode_status_response` on a `-4` must also
    /// yield `Indeterminate`, not `Transport`.
    #[test]
    fn rp4b_3_neg4_status_response_yields_indeterminate() {
        use crate::transports::dps::gen::status_response::Status;
        let resp = gen::StatusResponse {
            status: Status::ErrorUnknown as i32,
            error_message: "transient".to_string(),
            ..Default::default()
        };
        let err = try_decode_status_response(resp).expect_err("ErrorUnknown must error");
        assert!(
            matches!(err, DpsError::Indeterminate { code: -4, .. }),
            "expected Indeterminate{{code:-4}}, got {err:?}"
        );
    }

    /// RP4B-3 companion: `try_decode_rro_info_response` on a `-4` must also
    /// yield `Indeterminate`, not `Transport`.
    #[test]
    fn rp4b_3_neg4_rro_info_response_yields_indeterminate() {
        use crate::transports::dps::gen::rro_info_response::Status;
        let resp = gen::RroInfoResponse {
            status: Status::ErrorUnknown as i32,
            ..Default::default()
        };
        let err = try_decode_rro_info_response(resp).expect_err("ErrorUnknown must error");
        assert!(
            matches!(err, DpsError::Indeterminate { code: -4, .. }),
            "expected Indeterminate{{code:-4}}, got {err:?}"
        );
    }
}
