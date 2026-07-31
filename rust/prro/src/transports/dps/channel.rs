//! `DpsChannel` async trait — every DPS gRPC operation in M2 goes
//! through here.
//!
//! Per ADR-M2-6, no method takes a `SqlitePool` / `SqliteConnection` /
//! `Pool<Sqlite>` / `Transaction<…>`.  W5 will static-assert this at
//! build time.  Per ADR-M2-2, the only production impl is
//! `super::grpc::GrpcDpsChannel` (tonic-backed); a native Rust tonic
//! mock for tests lands in C4.

use async_trait::async_trait;

use super::dto::{
    CheckAck, CheckEnvelope, CheckSignBlob, OfflineCodesResponse, RroInfo, StatusSnapshot,
};
use super::error::DpsError;
use super::raw_reply::RawSendObservation;

/// `Send + Sync` so impls can be wrapped in `Arc<dyn DpsChannel>` and
/// shared across worker tasks.  All five wire RPCs are unary (W0-1
/// finding); the trait does not expose any streaming surface.
#[async_trait]
pub trait DpsChannel: Send + Sync {
    /// `sendChkV2` — submit a CMS-signed receipt.  Maps the `Check`
    /// proto + `CheckResponse` proto onto the typed envelope/ack pair.
    async fn send_chk(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError>;

    /// CS-3 3.2 PR2 pin3 — the single-RPC fan-out (spec §4.2): submit a receipt with EXACTLY ONE
    /// wire call and return BOTH the legacy `Result` AND the total transport-minted
    /// `RawSendObservation` (the typed delivery evidence the Bridge/engine maps at apply).
    ///
    /// **REQUIRED — NO trait default (CS-3 S7-1).** The composed write-path apply derives the
    /// terminal document state from `observation.evidence()`, so a degraded observation silently
    /// collapses every outcome to `SubmittedUnknown`/HELD. A synthetic reverse-projection default
    /// (from the already-collapsed `Result`) is REFUSED here: it would let a future production
    /// adapter that forgets to override emit a FABRICATED digest/provenance as if transport-minted.
    /// By carrying no default, every impl MUST choose its fidelity explicitly, compiler-enforced —
    /// production `GrpcDpsChannel` overrides with the lossless single-decode body; scripted test
    /// mocks use `dto::scripted_observation(self.send_chk(env).await)`; a mock exercising the
    /// ABSENCE of a trusted reply returns an explicit `NoResponse` observation.
    ///
    /// **RETURN-TYPE PIN:** keep the tuple `(Result<…>, RawSendObservation)`. It MUST NOT be
    /// "simplified" to `Result<(CheckAck, RawSendObservation), DpsError>` — that would DROP the
    /// observation on the `Err` arm, which is the most reconciliation-valuable shadow
    /// (`NoResponse`/`RemoteAuthStatus`). `?` on the tuple is a compile error, by design.
    async fn send_chk_observed(
        &self,
        envelope: CheckEnvelope,
    ) -> (Result<CheckAck, DpsError>, RawSendObservation);

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

    /// T=112 ASK_OFFLINE_CODES — request offline fiscal-document IDs from DPS.
    ///
    /// Sends `envelope` (a CMS-signed T=112 ServiceChk XML) via the same
    /// `sendChkV2` RPC used by `send_chk` (live-proven: T=112 rides
    /// `typCheck=3` / `DpsCheckType::ServiceChk`), then CMS-strips the
    /// `data_sign` field of the successful `CheckAck` and parses the
    /// `<ID>…</ID>` offline-code elements from the inner XML.
    ///
    /// The caller is responsible for building the T=112 envelope (slice C3).
    /// This method only covers the transport + decode half.
    ///
    /// Error mapping is identical to `send_chk` — server-level DPS status
    /// codes surface as `DpsError::Server { code, message }` with the same
    /// integer codes; a CMS-strip or XML-parse failure surfaces as
    /// `DpsError::Decode`.
    async fn ask_offline_codes(
        &self,
        envelope: CheckEnvelope,
    ) -> Result<OfflineCodesResponse, DpsError>;

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
        crate::db::tx::assert_not_in_with_immediate("by_server_fiscal_no");
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
        crate::db::tx::assert_not_in_with_immediate("query_by_local_identity");
        Err(DpsError::QueryNotSupported("query_by_local_identity"))
    }
}
