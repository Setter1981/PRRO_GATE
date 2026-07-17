//! Typed error surface for the DPS gRPC transport.
//!
//! Eight variants cover the failure modes a `DpsChannel` caller has
//! to make routing decisions on (retry now, fall back, mark FN
//! broken, escalate to operator, classify by-id-lookup outcome,
//! reject unsupported queries).  The categories are protocol-shape,
//! not error-source: a `tonic::Status::with_code(GRPC_UNAVAILABLE)`
//! and a TCP connect-refused both surface as `Transport`, because
//! the caller's response is the same — back off + retry.

use thiserror::Error;

/// Errors returned by every `DpsChannel` method.
#[derive(Debug, Error)]
pub enum DpsError {
    /// Transport-level failure (TCP / TLS / DNS / per-call deadline /
    /// gRPC `Unavailable` / `DeadlineExceeded`).  Caller should back
    /// off and retry; the FN is not necessarily broken.
    ///
    /// Distinct from [`DpsError::Indeterminate`]: `Transport` means
    /// **no DPS envelope was received at all** — the channel itself
    /// failed before any application-level status could be decoded.
    #[error("DPS transport: {0}")]
    Transport(String),

    /// A DPS response envelope was successfully decoded but the status
    /// code does not settle submission certainty — forward progress
    /// was observed on the wire yet the outcome (accepted / rejected)
    /// is unknown.  The canonical case is `ERROR_UNKNOWN (-4)` from
    /// `CheckResponse` / `StatusResponse` / `RroInfoResponse`.
    ///
    /// Distinct from [`DpsError::Transport`] (no envelope received)
    /// and [`DpsError::Server`] (a definitive non-OK server status).
    /// Slice A re-types `-4` here so later slices can distinguish
    /// "possibly submitted" from "definitely not sent" without parsing
    /// error message strings.
    ///
    /// `code` is the raw DPS status integer (`-4` for `ERROR_UNKNOWN`).
    /// `message` carries the server's textual explanation when present.
    #[error("DPS indeterminate (code={code}): {message}")]
    Indeterminate { code: i32, message: String },

    /// Authorization-class server response, split by the application-
    /// level DPS status code so callers can route per-doc rejects
    /// (`-1`, `kind = DocumentReject`) separately from per-FN
    /// configuration failures (`-13` / `-14`,
    /// `kind = FiscalNumberNotRegistered`).  Retrying never helps; the
    /// caller's response depends on `kind` (reject the doc vs operator-
    /// escalate the FN).  See ADR-M3-A6 prereq + W0-3 §2.1 for the
    /// routing table that consumes `kind`.
    ///
    /// `kind` covers exactly the application-level CheckResponse /
    /// StatusResponse / RroInfoResponse status codes for which an
    /// authorization-class destination is documented.  Transport-level
    /// authentication failures (gRPC `Unauthenticated` /
    /// `PermissionDenied`) are mapped to [`DpsError::Transport`] in
    /// `grpc.rs::map_tonic_status` — they have no DPS status code and
    /// would force a synthetic `code = 0` here.
    #[error("DPS authorization {kind:?} (code={code}): {message}")]
    Authorization {
        code: i32,
        kind: AuthorizationKind,
        message: String,
    },

    /// Server-side response decode failed (malformed protobuf, missing
    /// required field, status enum out of range).  Possibly an
    /// upstream contract drift — log loudly + treat as broken FN
    /// until investigated.
    #[error("DPS response decode: {0}")]
    Decode(String),

    /// DPS replied with a non-OK status code from one of the
    /// `CheckResponse::Status` / `StatusResponse::Status` /
    /// `RroInfoResponse::Status` enums (e.g. `ERROR_NOT_OPEN_SHIFT`,
    /// `ERROR_BAD_HASH_PREV`).  `code` is the raw enum value as
    /// emitted on the wire; `message` carries the server's textual
    /// explanation when present.
    #[error("DPS server status {code}: {message}")]
    Server { code: i32, message: String },

    /// `by_server_fiscal_no` absent path: server returned a
    /// well-formed response but no record exists for the FN
    /// (`response.id` empty / documented absent-shape).
    #[error("DPS lookup not found for the requested fiscal id")]
    NotFound,

    /// `by_server_fiscal_no` mismatch path (PRRO_GATE-5js): server's
    /// `lastChk` returned a fiscal id that does NOT match what the
    /// caller expected.  Distinct variant so callers can route this
    /// to a reconciliation flow without pattern-matching on
    /// `Server { code, .. }` substrings.
    #[error("DPS fiscal id mismatch: expected {expected_id}, server returned {actual_id}")]
    ServerFiscalIdMismatch {
        expected_id: String,
        actual_id: String,
    },

    /// Caller asked for a query method DPS does not implement (W0-1
    /// finding: `ByLocalIdentity` has no wire-level RPC; the only
    /// server-side lookup is `lastChk` + id-match).  Distinct from
    /// `NotFound` (which is a runtime outcome) and from `Internal`
    /// (which is a wrapper bug) — `QueryNotSupported` is a typed
    /// "this protocol does not offer that operation" signal.
    #[error("DPS does not support the requested query: {0}")]
    QueryNotSupported(&'static str),

    /// Wrapper-side bug or un-wired path.  Production callers should
    /// never see this; if they do, the channel is mis-configured.
    /// Used during W3 substrate phases for stub paths so they fail
    /// loudly without panicking.
    #[error("DPS wrapper internal: {0}")]
    Internal(String),
}

/// Sub-classification of [`DpsError::Authorization`] for routing.
///
/// Per ADR-M3-A6 prereq + W0-3 §2.1, M3a write-path routing splits
/// authorization-class server responses by intent:
/// - `DocumentReject` — the server rejected this specific document
///   (cert/signature did not verify the canonical envelope).  Live
///   stage-4-4b: `Sending → Rejected`.
/// - `FiscalNumberNotRegistered` — the FN itself is in a bad
///   registration state with DPS.  Live stage-4-4b:
///   `Sending → ErrorRetryable → RequiresManualReconciliation`.
///
/// `kind` is intentionally a closed set of application-level
/// authorization classes; transport-level gRPC `Unauthenticated` /
/// `PermissionDenied` does not belong here (no DPS status code, and
/// the corrective action is "back off + retry" not "reject doc" or
/// "rotate creds").  See [`DpsError::Authorization`] for the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationKind {
    /// `-1` ERROR_VEREFY: per-document authorization failure.  The
    /// document's signature or content was rejected by DPS for this
    /// specific receipt; FN registration is intact.
    DocumentReject,
    /// `-13` ERROR_NOT_REGISTERED_RRO / `-14` ERROR_NOT_REGISTERED_SIGNER:
    /// per-FN configuration failure.  The FN row or its signer cert is
    /// not registered with DPS; operator must rotate / reconcile creds.
    FiscalNumberNotRegistered,
}
