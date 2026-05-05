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
    /// Server-side fiscal-id lookup (PRRO_GATE-5js).
    ///
    /// DPS does not expose a direct "lookup by fiscal id" RPC.  The
    /// canonical semantic per W0-1 is: send `lastChk(fn_sign)`, then
    /// match `response.id` against the caller's `expected_id`:
    ///
    /// - `response.id` empty           → `DpsError::NotFound`
    /// - `response.id != expected_id`  → `DpsError::ServerFiscalIdMismatch`
    /// - `response.id == expected_id`  → `Ok(ack)`
    ///
    /// Default body lives in the trait so impls cannot diverge —
    /// there is one canonical implementation.  C4 mock asserts the
    /// match / mismatch / absent triple end-to-end.
    async fn by_server_fiscal_no(
        &self,
        fn_sign: &CheckSignBlob,
        expected_id: &str,
    ) -> Result<CheckAck, DpsError> {
        let ack = self.last_chk(fn_sign).await?;
        if ack.id.is_empty() {
            return Err(DpsError::NotFound);
        }
        if ack.id != expected_id {
            return Err(DpsError::ServerFiscalIdMismatch {
                expected_id: expected_id.to_string(),
                actual_id: ack.id,
            });
        }
        Ok(ack)
    }

    /// Lookup by local-identity (per-FN local document number / lnd).
    ///
    /// W0-1 finding: DPS does NOT expose this query at the wire
    /// level.  The trait carries the method as a typed `Err` for
    /// caller ergonomics — services calling `dyn DpsChannel` can
    /// match on `DpsError::QueryNotSupported` without having to know
    /// about the wire shape.  Default body returns
    /// `QueryNotSupported`; impls SHOULD NOT override it.  C4 mock
    /// asserts the typed-Err contract so a future caller cannot
    /// silently get `Internal(...)` instead.
    async fn query_by_local_identity(
        &self,
        _fn_id: &str,
        _local_number: i32,
    ) -> Result<CheckAck, DpsError> {
        Err(DpsError::QueryNotSupported("query_by_local_identity"))
    }
}
