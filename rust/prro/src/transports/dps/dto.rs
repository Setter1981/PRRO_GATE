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

use super::digest_framing::{DigestFramer, MsgType};
use super::error::{AuthorizationKind, DpsError};
use super::gen;
use super::raw_reply::{RawSendReply, WireDiagnostics};
use prro_domain::delivery::{
    BoundedText, DecodedResponseDigest, NonEmptyFiscalNumber, NonOkStatusCode,
};

impl From<DpsCheckType> for gen::check::Type {
    fn from(t: DpsCheckType) -> Self {
        match t {
            DpsCheckType::Chk => gen::check::Type::Chk,
            DpsCheckType::ZReport => gen::check::Type::Zreport,
            DpsCheckType::ServiceChk => gen::check::Type::Servicechk,
        }
    }
}

impl CheckEnvelope {
    /// CS-3 S7-1 (composition impl-design §6-B1): the canonical bound-envelope identity — SHA-256
    /// over the FULL prost-encoded `gen::Check` (EVERY wire field: `rro_fn`, `date_time`,
    /// `check_sign`, `local_number`, `check_type`, `id_offline`, `id_cancel`), NOT just `check_sign`.
    ///
    /// This single value is the reservation/token `envelope_hash`, the `submit_authorized` rebind
    /// check, AND the `transport_trace.request_envelope_sha256` — so a post-authorize tamper of ANY
    /// wire field (not only the CMS blob) is caught before the wire. A `check_sign`-only hash left the
    /// other six fields mutable between authorize and send.
    pub fn wire_hash(&self) -> [u8; 32] {
        use prost::Message;
        use sha2::{Digest, Sha256};
        let proto: gen::Check = self.clone().into();
        Sha256::digest(proto.encode_to_vec()).into()
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

// CS-3 3.2 (PR1): the decoded-content digest is the byte-exact, versioned framing of the KNOWN
// decoded fields (spec §4.1), NOT a prost re-encode. It is a *collision-resistant fingerprint of
// the decoded content* — NOT a raw-wire proof ([[project_digest_decoded_content_decision]]); a
// re-encode would silently drop unknown fields / encoding quirks. The framing lives in
// `digest_framing`; these functions are the SOLE mint of a `DecodedResponseDigest` (source-gated to
// this decoder module — PR1 pin 5), so the engine can only carry a transport-minted digest.

/// Framed digest of a decoded `CheckResponse` (fields in proto field-number order).
fn decoded_digest_check(r: &gen::CheckResponse) -> DecodedResponseDigest {
    let mut f = DigestFramer::new(MsgType::CheckResponse);
    f.field_str(&r.id)
        .field_int(r.status as i64)
        .field_bytes(&r.id_sign)
        .field_bytes(&r.data_sign)
        .field_str(&r.error_message);
    DecodedResponseDigest::from_transport_digest(f.finalize())
}

/// Framed digest of a decoded `StatusResponse`.
fn decoded_digest_status(r: &gen::StatusResponse) -> DecodedResponseDigest {
    let mut f = DigestFramer::new(MsgType::StatusResponse);
    f.field_bool(r.open_shift)
        .field_bool(r.online)
        .field_str(&r.last_signer)
        .field_int(r.status as i64)
        .field_str(&r.error_message);
    DecodedResponseDigest::from_transport_digest(f.finalize())
}

/// Framed digest of a decoded `RroInfoResponse` (16 fields incl. the recursive `operators`).
fn decoded_digest_rro_info(r: &gen::RroInfoResponse) -> DecodedResponseDigest {
    let mut f = DigestFramer::new(MsgType::RroInfoResponse);
    f.field_int(r.status as i64)
        .field_int(r.status_rro as i64)
        .field_bool(r.open_shift)
        .field_bool(r.online)
        .field_str(&r.last_signer)
        .field_str(&r.name)
        .field_str(&r.name_to)
        .field_str(&r.addr)
        .field_bool(r.single_tax)
        .field_bool(r.offline_allowed)
        .field_int(r.add_num as i64)
        .field_str(&r.pn)
        .field_repeated(&r.operators, |b, o| {
            b.field_str(&o.serial)
                .field_int(o.status as i64)
                .field_bool(o.senior)
                .field_str(&o.isname);
        })
        .field_str(&r.tins)
        .field_int(r.lnum as i64)
        .field_str(&r.name_pay);
    DecodedResponseDigest::from_transport_digest(f.finalize())
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
            let digest = decoded_digest_check(&r);
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

// ── CS-3 3.2 PR2 pin3 — the shadow projection (spec §4.2/§4.3) ────────────────────────
//
// `send_chk_observed`'s override projects ONE decoded `gen::CheckResponse` into BOTH the legacy
// `Result<CheckAck, DpsError>` (byte-identical to `try_decode_check_response`) AND the total
// transport-minted `RawSendReply`. The shadow is minted from the RAW reply, NEVER from the
// collapsed `DpsError` — that is the ONLY place the digest survives (spec digest-gap: `Server`/
// `Decode`/`Authorization` carry none). Uniform transport view: any non-Ok/non-Unknown code is a
// `ServerCode` (no domain routing) — it DIVERGES from the legacy classification by design (the
// shadow drives nothing in 3.2; teeth assert neutrality only on the legacy `.0`).

/// Total transport-minted evidence for one decoded `CheckResponse` — dispatches on the RAW status
/// so the digest is framed from live fields. `Status::try_from` fails only for a code outside the
/// declared `-16..=1` enum (proto-unrecognized). §4.3 row-6 (auditor-adjudicated 2026-07-18): an
/// unrecognized NON-ZERO code is still a server verdict code → `ServerCode` (the engine maps it to
/// `UnknownStatus → TransientRetry`); ONLY proto `status == 0` (`Ok(Unknown)`) is `MissingStatus →
/// ProbeRequired`. This preserves the §4.6 "unknown-non-zero" drift-delta (Live `Decode →
/// ProbeRequired` vs Shadow `UnknownStatus → TransientRetry`).
pub(in crate::transports::dps) fn raw_reply_from_check_response(
    r: &gen::CheckResponse,
) -> RawSendReply {
    use gen::check_response::Status;
    match Status::try_from(r.status) {
        // Unrecognized non-zero code (e.g. -17, 2): `NonOkStatusCode` rejects only 0/1, and a
        // `try_from` Err ⇒ the code is outside -16..=1 ⇒ ∉ {0,1}, so the mint always succeeds.
        Err(_) => RawSendReply::server_code(
            NonOkStatusCode::from_transport(r.status)
                .expect("Status::try_from Err ⇒ code ∉ declared -16..=1 enum ⇒ ∉ {0,1}"),
            decoded_digest_check(r),
        ),
        Ok(Status::Ok) => match NonEmptyFiscalNumber::from_transport(r.id.clone()) {
            // Non-empty id IS the transport-proven acceptance evidence (D-4) — no digest.
            Some(id) => RawSendReply::accepted(id),
            // OK but empty id: captures what the legacy `Ok(CheckAck{id:""})` erases.
            None => RawSendReply::ok_no_fiscal_id(decoded_digest_check(r)),
        },
        Ok(Status::Unknown) => RawSendReply::missing_status(decoded_digest_check(r)),
        Ok(other) => RawSendReply::server_code(
            NonOkStatusCode::from_transport(other as i32)
                .expect("Ok(1)/Unknown(0) handled above; every other Status variant is non-0/1"),
            decoded_digest_check(r),
        ),
    }
}

/// Single-decode dual projection (spec §4.2). OWNS the borrow-then-move ordering so no caller can
/// reorder: the shadow (`raw`/`diag`) is built while `r` is still owned (they read `r.id` /
/// `r.error_message` / frame the digest), THEN `try_decode_check_response(r)` consumes `r` — the
/// legacy `.0` is produced by the SAME call on the SAME reply as today's `send_chk`, so it is
/// byte-identical. `WireDiagnostics` is the non-authority forensic sidecar (status_code is
/// forensic-only; it does not imply an error).
pub(in crate::transports::dps) fn observe_check_reply(
    r: gen::CheckResponse,
) -> (Result<CheckAck, DpsError>, RawSendReply, WireDiagnostics) {
    let raw = raw_reply_from_check_response(&r);
    let diag = WireDiagnostics {
        status_code: Some(r.status),
        grpc_code: None,
        message: (!r.error_message.is_empty())
            .then(|| BoundedText::from_truncating(r.error_message.clone())),
    };
    let legacy = try_decode_check_response(r);
    (legacy, raw, diag)
}

/// CS-3 S7-1 test-support: build the SAME faithful `RawSendObservation` the production
/// `GrpcDpsChannel` would mint, from a mock's already-collapsed `Result<CheckAck, DpsError>`.
///
/// The cutover's `submit_authorized` derives the record/apply evidence from
/// `observation.evidence()`, so a mock that only implements `send_chk` (and thus gets the DEGRADED
/// `observe_from_legacy` default — Accepted/NoResponse only) would mis-drive the composed path for
/// server-status rejects. This reverse-constructs the raw `gen::CheckResponse` for the results that
/// map to a real DPS status (`Ok`/`Accepted`, `Authorization{code}`, `Server{code}`, `Decode`→0) and
/// runs the SAME `observe_check_reply` decode the live channel uses. Genuine transport / wrapper
/// errors (`Transport`, `RemoteStatus`, `NotFound`, `Internal`, …) have NO faithful reply, so they
/// fall back to `observe_from_legacy` (correctly `NoResponse`), matching production.
#[cfg(any(test, feature = "test-support"))]
pub fn observe_faithful_from_legacy(
    legacy: &Result<CheckAck, DpsError>,
) -> super::raw_reply::RawSendObservation {
    let reconstructed: Option<gen::CheckResponse> = match legacy {
        Ok(ack) => Some(gen::CheckResponse {
            id: ack.id.clone(),
            status: 1,
            id_sign: ack.id_sign.clone(),
            data_sign: ack.data_sign.clone(),
            error_message: String::new(),
        }),
        Err(DpsError::Authorization { code, message, .. }) => Some(gen::CheckResponse {
            id: String::new(),
            status: *code,
            id_sign: Vec::new(),
            data_sign: Vec::new(),
            error_message: message.clone(),
        }),
        Err(DpsError::Server { code, message }) => Some(gen::CheckResponse {
            id: String::new(),
            status: *code,
            id_sign: Vec::new(),
            data_sign: Vec::new(),
            error_message: message.clone(),
        }),
        Err(DpsError::Decode(msg)) => Some(gen::CheckResponse {
            id: String::new(),
            status: 0,
            id_sign: Vec::new(),
            data_sign: Vec::new(),
            error_message: msg.clone(),
        }),
        _ => None,
    };
    match reconstructed {
        Some(chk) => {
            let (_legacy, raw, diag) = observe_check_reply(chk);
            super::raw_reply::RawSendObservation::new(raw, diag)
        }
        None => super::raw_reply::observe_from_legacy(legacy),
    }
}

/// CS-3 S7-1 — the shared `send_chk_observed` body for scripted test mocks.
///
/// `send_chk_observed` has NO trait default (removed by design so a forgotten mock cannot silently
/// degrade to `NoResponse` and mis-drive the composed apply). A normal scripted mock's override is
/// just `scripted_observation(self.send_chk(env).await)`: it pairs the legacy `Result` with the
/// faithful observation ([`observe_faithful_from_legacy`]) from ONE call. A mock that specifically
/// exercises the ABSENCE of a trusted reply must NOT use this — it returns an explicit `NoResponse`
/// observation instead. Production `GrpcDpsChannel` overrides with the lossless single-decode body.
#[cfg(any(test, feature = "test-support"))]
pub fn scripted_observation(
    legacy: Result<CheckAck, DpsError>,
) -> (
    Result<CheckAck, DpsError>,
    super::raw_reply::RawSendObservation,
) {
    let observation = observe_faithful_from_legacy(&legacy);
    (legacy, observation)
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
            let digest = decoded_digest_status(&r);
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
            let digest = decoded_digest_rro_info(&r);
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

    /// CS-3 3.2: `Indeterminate` carries a `DecodedResponseDigest` — the byte-exact framed
    /// fingerprint of the ACTUAL decoded envelope (§4.1), so the Bridge carries the transport-minted
    /// digest, never a fabricated one. Distinct DECODED CONTENT → distinct digest; identical content
    /// → identical digest (deterministic). Teeth: a constant/fabricated digest fails the `assert_ne!`.
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

    // ── CS-3 3.2 PR2 pin3 — shadow projection teeth (spec §4.3 / §6) ─────────────────────
    // These run crate-internal so they can recompute `decoded_digest_check` and match on the
    // opaque `RawSendReplyKind` — the wire-level double-issue canary lives in dps_channel_smoke.rs.

    use crate::transports::dps::raw_reply::RawSendReplyKind;

    fn chk(status: i32, id: &str, msg: &str) -> gen::CheckResponse {
        gen::CheckResponse {
            id: id.to_string(),
            status,
            id_sign: b"<id-sign>".to_vec(),
            data_sign: b"<data-sign>".to_vec(),
            error_message: msg.to_string(),
        }
    }

    /// Tooth #8 (exhaustive mapping): every declared status + unknown-non-zero maps to the spec
    /// §4.3 `RawSendReply` variant. Break any arm → RED.
    #[test]
    fn pin3_raw_reply_mapping_is_exhaustive() {
        use crate::transports::dps::gen::check_response::Status;
        assert!(matches!(
            raw_reply_from_check_response(&chk(Status::Ok as i32, "DPS-9", "")).kind(),
            RawSendReplyKind::Accepted { .. }
        ));
        assert!(matches!(
            raw_reply_from_check_response(&chk(Status::Ok as i32, "", "")).kind(),
            RawSendReplyKind::OkNoFiscalId { .. }
        ));
        assert!(matches!(
            raw_reply_from_check_response(&chk(Status::Unknown as i32, "", "")).kind(),
            RawSendReplyKind::MissingStatus { .. }
        ));
        // every declared negative verdict -1..-16 → ServerCode{that code}
        for code in [
            -1i32, -2, -3, -4, -5, -6, -7, -8, -9, -10, -11, -12, -13, -14, -15, -16,
        ] {
            match raw_reply_from_check_response(&chk(code, "", "boom")).kind() {
                RawSendReplyKind::ServerCode { code: c, .. } => assert_eq!(c.get(), code),
                other => panic!("status {code} → expected ServerCode, got {other:?}"),
            }
        }
        // §4.3 row-6 (auditor-adjudicated): unknown non-zero (outside the declared enum) →
        // ServerCode carrying that raw code (the engine maps it to UnknownStatus). Only proto
        // status==0 (Status::Unknown, asserted above) is MissingStatus.
        for code in [-17i32, -99, 2, 7, 12_345] {
            match raw_reply_from_check_response(&chk(code, "", "?")).kind() {
                RawSendReplyKind::ServerCode { code: c, .. } => assert_eq!(c.get(), code),
                other => panic!("unknown non-zero {code} must map to ServerCode, got {other:?}"),
            }
        }
    }

    /// Tooth #3 (free -4 cross-mint): `observe_check_reply` mints the digest ONCE per reply; the
    /// legacy `Indeterminate.digest` and the shadow `ServerCode.digest` are byte-equal (both
    /// `decoded_digest_check(&r)`). Cross-validates two independent mint sites on one reply.
    #[test]
    fn pin3_neg4_legacy_and_shadow_share_one_digest_mint() {
        use crate::transports::dps::gen::check_response::Status;
        let (legacy, raw, _diag) = observe_check_reply(chk(Status::ErrorUnknown as i32, "", "srv"));
        let legacy_digest = match legacy {
            Err(DpsError::Indeterminate { digest, .. }) => digest,
            other => panic!("expected Indeterminate, got {other:?}"),
        };
        match raw.kind() {
            RawSendReplyKind::ServerCode { code, digest } => {
                assert_eq!(code.get(), -4);
                assert_eq!(
                    *digest, legacy_digest,
                    "same reply → one digest mint carried by both legacy and shadow"
                );
            }
            other => panic!("expected ServerCode, got {other:?}"),
        }
    }

    /// Tooth #4/#2 (digest-gap + not-fabricated): a `-2` reply → legacy `DpsError::Server` carries
    /// NO digest, yet the shadow `ServerCode` carries the REAL framed digest (equal to an
    /// independent `decoded_digest_check` recompute). Project the shadow from the collapsed
    /// `DpsError` (no digest) or hash a constant → RED.
    #[test]
    fn pin3_digest_gap_shadow_carries_real_digest_where_legacy_server_has_none() {
        use crate::transports::dps::gen::check_response::Status;
        let r = chk(Status::ErrorCheck as i32, "", "check failed");
        let recomputed = decoded_digest_check(&r);
        let (legacy, raw, _diag) = observe_check_reply(r);
        assert!(
            matches!(legacy, Err(DpsError::Server { code: -2, .. })),
            "legacy must be Server{{-2}} (a variant with NO digest field)"
        );
        match raw.kind() {
            RawSendReplyKind::ServerCode { code, digest } => {
                assert_eq!(code.get(), -2);
                assert_eq!(
                    *digest, recomputed,
                    "shadow must carry the real framed digest, not a fabricated/constant one"
                );
            }
            other => panic!("expected ServerCode, got {other:?}"),
        }
    }

    /// Tooth #5 (empty-id split): OK + empty id → shadow `OkNoFiscalId(digest)` even though legacy
    /// is `Ok(CheckAck{id:""})`. Collapsing the split to always-`Accepted` →
    /// `NonEmptyFiscalNumber::from_transport("")` is `None` → cannot build `accepted` → RED.
    #[test]
    fn pin3_ok_empty_id_splits_to_ok_no_fiscal_id() {
        use crate::transports::dps::gen::check_response::Status;
        let (legacy, raw, _diag) = observe_check_reply(chk(Status::Ok as i32, "", ""));
        assert!(
            matches!(&legacy, Ok(ack) if ack.id.is_empty()),
            "legacy Ok with empty id"
        );
        assert!(matches!(raw.kind(), RawSendReplyKind::OkNoFiscalId { .. }));
    }

    /// Tooth #7 (byte-neutrality of the legacy leg): `observe_check_reply(r).0` is byte-identical
    /// to `try_decode_check_response(r)` across the full status matrix — the shadow can diverge, the
    /// legacy leg cannot. Any decoder drift in the split → RED.
    #[test]
    fn pin3_legacy_leg_is_byte_identical_to_try_decode() {
        use crate::transports::dps::gen::check_response::Status;
        let cases = [
            chk(Status::Ok as i32, "DPS-1", ""),
            chk(Status::Ok as i32, "", ""),
            chk(Status::Unknown as i32, "", ""),
            chk(Status::ErrorVerefy as i32, "", "v"),
            chk(Status::ErrorCheck as i32, "", "c"),
            chk(Status::ErrorUnknown as i32, "", "u"),
            chk(Status::ErrorNotRegisteredRro as i32, "", "r"),
            chk(Status::ErrorNotRegisteredSigner as i32, "", "s"),
            chk(-99, "", "unknown-nonzero"),
        ];
        for r in cases {
            let expected = format!("{:?}", try_decode_check_response(r.clone()));
            let actual = format!("{:?}", observe_check_reply(r).0);
            assert_eq!(
                actual, expected,
                "legacy leg must equal try_decode_check_response byte-for-byte"
            );
        }
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
