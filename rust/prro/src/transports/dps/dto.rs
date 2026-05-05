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

use super::error::DpsError;
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

/// Dispatch a `gen::CheckResponse` onto either `Ok(CheckAck)` or a
/// typed `DpsError`.  Status-code mapping (per W0-1 review):
///
/// - `Ok` (1) → `Ok(CheckAck)`
/// - `Unknown` (0) → `Decode` (proto3 default = field was missing
///   on the wire)
/// - `ErrorVerefy` (-1) → `Authorization`
/// - `ErrorNotRegisteredRro` (-13) → `Authorization`
/// - `ErrorNotRegisteredSigner` (-14) → `Authorization`
/// - `ErrorUnknown` (-4) → `Transport` (retry-class per W0-1 D3; the
///   wire error is not stable enough to mark the FN broken — back
///   off + retry)
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
        Status::ErrorVerefy | Status::ErrorNotRegisteredRro | Status::ErrorNotRegisteredSigner => {
            Err(DpsError::Authorization(format!(
                "{}: {}",
                st.as_str_name(),
                r.error_message
            )))
        }
        Status::ErrorUnknown => Err(DpsError::Transport(format!(
            "ERROR_UNKNOWN (-4) — retry-class per W0-1 D3: {}",
            r.error_message
        ))),
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
        Status::ErrorVerefy | Status::ErrorNotRegisteredRro | Status::ErrorNotRegisteredSigner => {
            Err(DpsError::Authorization(format!(
                "{}: {}",
                st.as_str_name(),
                r.error_message
            )))
        }
        Status::ErrorUnknown => Err(DpsError::Transport(format!(
            "ERROR_UNKNOWN (-4) — retry-class per W0-1 D3: {}",
            r.error_message
        ))),
        other => Err(DpsError::Server {
            code: other as i32,
            message: r.error_message,
        }),
    }
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
        Status::ErrorVerefy | Status::ErrorNotRegisteredRro | Status::ErrorNotRegisteredSigner => {
            Err(DpsError::Authorization(st.as_str_name().to_string()))
        }
        Status::ErrorUnknown => Err(DpsError::Transport(
            "ERROR_UNKNOWN (-4) on RroInfoResponse — retry-class per W0-1 D3".into(),
        )),
        other => Err(DpsError::Server {
            code: other as i32,
            message: String::new(),
        }),
    }
}
