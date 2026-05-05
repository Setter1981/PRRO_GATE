//! Typed error surface for the DPS gRPC transport.
//!
//! Six variants cover the failure modes a `DpsChannel` caller has to
//! make routing decisions on (retry now, fall back, mark FN broken,
//! escalate to operator).  The categories are protocol-shape, not
//! error-source: a `tonic::Status::with_code(GRPC_UNAVAILABLE)` and a
//! TCP connect-refused both surface as `Transport`, because the
//! caller's response is the same — back off + retry.

use thiserror::Error;

/// Errors returned by every `DpsChannel` method.
#[derive(Debug, Error)]
pub enum DpsError {
    /// Transport-level failure (TCP / TLS / DNS / per-call deadline /
    /// gRPC `Unavailable` / `DeadlineExceeded`).  Caller should back
    /// off and retry; the FN is not necessarily broken.
    #[error("DPS transport: {0}")]
    Transport(String),

    /// Authorization-class server response (cert revoked, signature
    /// rejected, FN not registered for this signer, etc.).  Retrying
    /// won't help; an operator must rotate creds or reconcile FN
    /// registration with DPS.
    #[error("DPS authorization: {0}")]
    Authorization(String),

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

    /// `by_server_fiscal_no`-specific outcome: server returned a
    /// well-formed response but no record matched the requested
    /// fiscal id (the response's `id` field is empty / disagrees in
    /// the documented absent-shape).
    #[error("DPS lookup not found for the requested fiscal id")]
    NotFound,

    /// Wrapper-side bug or un-wired path.  Production callers should
    /// never see this; if they do, the channel is mis-configured.
    /// Used during W3 substrate phases for stub paths so they fail
    /// loudly without panicking.
    #[error("DPS wrapper internal: {0}")]
    Internal(String),
}
