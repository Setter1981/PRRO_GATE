//! Typed DTO wrappers for the DPS gRPC transport.
//!
//! Public `DpsChannel` API consumes / returns these typed shapes; raw
//! `tonic`/`prost` generated structs from `super::gen` stay
//! crate-private so the transport contract is reviewable independently
//! of upstream proto-codegen drift (and so a future W4 byte-equivalence
//! test does not have to assert on prost-generated `Default::default`).
//!
//! Field set is the proto field set, mapped to plain owned types — no
//! repr trick, no zero-copy borrow lifetimes.  C3 fills in the
//! `From<gen::*>` / `Into<gen::*>` conversions; C2 only lands the
//! shapes.

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
    /// Unix-epoch seconds.
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
