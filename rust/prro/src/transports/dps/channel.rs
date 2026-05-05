//! `DpsChannel` async trait — every DPS gRPC operation in M2 goes
//! through here.
//!
//! Per ADR-M2-6, no method takes a `SqlitePool` / `SqliteConnection` /
//! `Pool<Sqlite>` / `Transaction<…>`.  W5 will static-assert this at
//! build time.  Per ADR-M2-2, the only production impl is
//! `super::grpc::GrpcDpsChannel` (tonic-backed); a native Rust tonic
//! mock for tests lands in C4.

use async_trait::async_trait;

use super::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use super::error::DpsError;

/// `Send + Sync` so impls can be wrapped in `Arc<dyn DpsChannel>` and
/// shared across worker tasks.  All five wire RPCs are unary (W0-1
/// finding); the trait does not expose any streaming surface.
#[async_trait]
pub trait DpsChannel: Send + Sync {
    /// `sendChkV2` — submit a CMS-signed receipt.  Maps the `Check`
    /// proto + `CheckResponse` proto onto the typed envelope/ack pair.
    async fn send_chk(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError>;

    /// `lastChk` — fetch the last receipt the server has on file for
    /// the FN encoded in `fn_sign`.  Used by recovery and by the
    /// `by_server_fiscal_no` lookup (PRRO_GATE-5js).
    async fn last_chk(&self, fn_sign: &CheckSignBlob) -> Result<CheckAck, DpsError>;

    /// `ping` — connectivity probe; sends a `Check` envelope and
    /// expects an OK reply.
    async fn ping(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError>;

    /// `statusRro` — fetch shift / online state for the FN.
    async fn status_rro(&self, fn_sign: &CheckSignBlob) -> Result<StatusSnapshot, DpsError>;

    /// `infoRro` — fetch full RRO descriptor (name, address, operators,
    /// tax mode).
    async fn info_rro(&self, fn_sign: &CheckSignBlob) -> Result<RroInfo, DpsError>;

    /// "By server fiscal no" lookup — `lastChk(fn_sign) + response.id
    /// match` (PRRO_GATE-5js).  The wire protocol does not expose a
    /// direct lookup-by-id RPC; a caller asking "do you know fiscal
    /// id X for this FN?" sends the FN signature, gets back the most
    /// recent server-known id, and asserts it matches X.
    ///
    /// Default body in C2 is an `Internal` stub — no panic path, no
    /// silent success.  C3 lands the real `last_chk` + match logic;
    /// C4 covers the match / mismatch / absent triple with a tonic
    /// mock.  We expose the default here so impls cannot accidentally
    /// diverge: there is one canonical implementation living next to
    /// the trait definition.
    async fn by_server_fiscal_no(
        &self,
        _fn_sign: &CheckSignBlob,
        _expected_id: &str,
    ) -> Result<CheckAck, DpsError> {
        Err(DpsError::Internal(
            "W3-C3-not-yet-wired: by_server_fiscal_no semantic lands in C3".into(),
        ))
    }
}
