//! `GrpcDpsChannel` — production `DpsChannel` impl backed by a tonic
//! `ChkIncomeServiceClient`.
//!
//! Holds ONE long-lived `tonic::transport::Channel` (Arc-cloning is
//! cheap; HTTP/2 connections are reused across calls).  Per-call we
//! clone the channel into a fresh `ChkIncomeServiceClient` rather than
//! locking a shared client behind a `Mutex` — tonic clients are
//! designed for this idiom.
//!
//! C3 (this commit) wires the real RPC method bodies + the
//! `tonic::Status` → `DpsError` mapping.  C4 lands the native tonic
//! mock server + integration tests covering the 5 error categories +
//! ByServerFiscalNo match/mismatch/absent triple.

use std::time::Duration;

use async_trait::async_trait;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::Status;

use super::channel::DpsChannel;
use super::dto::{
    try_decode_check_response, try_decode_rro_info_response, try_decode_status_response, CheckAck,
    CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot,
};
use super::error::DpsError;
use super::gen::chk_income_service_client::ChkIncomeServiceClient;

/// Long-lived gRPC client to a single DPS endpoint.
///
/// Construct with [`GrpcDpsChannel::connect`].  The held `Channel` is
/// cheaply `Clone` (it wraps an `Arc` over the underlying HTTP/2
/// connection pool); per-call we clone it into a fresh
/// `ChkIncomeServiceClient` rather than locking a shared client behind
/// a `Mutex` — tonic clients are designed for this idiom.
#[derive(Debug, Clone)]
pub struct GrpcDpsChannel {
    /// Endpoint URI as supplied to `connect`, kept for diagnostics.
    /// Not used on the hot path — the live HTTP/2 state lives in
    /// `channel`.
    #[allow(dead_code)] // diagnostic logging surface — wired in M3 ops layer
    endpoint: String,
    /// Default per-call deadline.  Applied at TWO layers:
    ///   1. `Endpoint::timeout` at construction (covers connect /
    ///      handshake / generic transport-level stalls).
    ///   2. `tonic::Request::set_timeout` on every per-call request
    ///      (writes the `grpc-timeout` HTTP/2 metadata header that
    ///      tonic 0.12 honours as the gRPC deadline; without this
    ///      the server has no deadline visibility and a hung server
    ///      could keep the call open past `Endpoint::timeout`).
    request_timeout: Duration,
    channel: Channel,
}

impl GrpcDpsChannel {
    /// Open a long-lived channel to `endpoint` (e.g.
    /// `https://cabinet.tax.gov.ua:9443`).  Eagerly establishes the
    /// HTTP/2 connection so a misconfigured endpoint surfaces a
    /// typed `DpsError::Transport` at construction time instead of
    /// on the first RPC.  Validates the URI and configures the
    /// default per-call deadline.
    pub async fn connect(endpoint: &str, request_timeout: Duration) -> Result<Self, DpsError> {
        let mut ep = Endpoint::from_shared(endpoint.to_string())
            .map_err(|e| DpsError::Transport(format!("invalid endpoint URI: {e}")))?
            .timeout(request_timeout)
            .connect_timeout(request_timeout);
        // **TLS fix 2026-05-25 (smoke A/B root cause)**: `https://`
        // endpoints require explicit `tls_config(...)` on tonic 0.12
        // — without it the channel silently fails з "transport error"
        // on first RPC.  `with_native_roots()` mirrors Python
        // grpc.ssl_channel_credentials(root_certificates=None) behavior
        // (uses OS native trust store).  Plain `http://` endpoints
        // unaffected (used by in-process mock tests in
        // dps_channel_smoke.rs).
        if endpoint.starts_with("https://") {
            ep = ep
                .tls_config(ClientTlsConfig::new().with_native_roots())
                .map_err(|e| DpsError::Transport(format!("tls config: {e}")))?;
        }
        let channel = ep
            .connect()
            .await
            .map_err(|e| DpsError::Transport(format!("connect: {e}")))?;
        Ok(Self {
            endpoint: endpoint.to_string(),
            request_timeout,
            channel,
        })
    }

    /// Build a fresh per-call client by cloning the long-lived
    /// channel.  Channel clone is an Arc bump; no new TCP / HTTP/2
    /// state is allocated.
    fn client(&self) -> ChkIncomeServiceClient<Channel> {
        ChkIncomeServiceClient::new(self.channel.clone())
    }

    /// Wrap a request payload in a `tonic::Request` with the
    /// `grpc-timeout` metadata header set to `self.request_timeout`.
    /// `Endpoint::timeout` alone is not enough — it covers transport
    /// stalls but does NOT write the gRPC deadline header, so a
    /// server that holds a unary call open indefinitely could blow
    /// past the configured deadline.  `Request::set_timeout` is the
    /// tonic 0.12-documented way to set the gRPC deadline (W0-1
    /// review finding).
    fn request<T>(&self, payload: T) -> tonic::Request<T> {
        let mut req = tonic::Request::new(payload);
        req.set_timeout(self.request_timeout);
        req
    }
}

/// Map a `tonic::Status` (gRPC-level error) to a `DpsError`.
///
/// All gRPC-level statuses (transport, deadline, auth) map to
/// [`DpsError::Transport`].  The application-level
/// [`DpsError::Authorization`] variant is reserved for documented DPS
/// status codes carried inside `CheckResponse` / `StatusResponse` /
/// `RroInfoResponse`, with a typed `AuthorizationKind` (per ADR-M3-A6
/// prereq).  gRPC `Unauthenticated` / `PermissionDenied` have no DPS
/// status code and would force a synthetic `code = 0` if mapped to
/// `Authorization`; routing them as `Transport` keeps the
/// `DpsError::Authorization` shape clean for the W7/W10 routing layer
/// and matches the actual recovery action ("back off + retry the
/// channel" — the wrapper, not the document).
fn map_tonic_status(s: Status) -> DpsError {
    DpsError::Transport(format!("gRPC {:?}: {}", s.code(), s.message()))
}

#[async_trait]
impl DpsChannel for GrpcDpsChannel {
    async fn send_chk(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("send_chk");
        let req = self.request(envelope.into());
        let resp = self
            .client()
            .send_chk_v2(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_check_response(resp.into_inner())
    }

    async fn last_chk(&self, fn_sign: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("last_chk");
        let req = self.request(fn_sign.into());
        let resp = self
            .client()
            .last_chk(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_check_response(resp.into_inner())
    }

    async fn ping(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("ping");
        let req = self.request(envelope.into());
        let resp = self.client().ping(req).await.map_err(map_tonic_status)?;
        try_decode_check_response(resp.into_inner())
    }

    async fn status_rro(&self, fn_sign: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("status_rro");
        let req = self.request(fn_sign.into());
        let resp = self
            .client()
            .status_rro(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_status_response(resp.into_inner())
    }

    async fn info_rro(&self, fn_sign: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        crate::db::tx::assert_not_in_with_immediate("info_rro");
        let req = self.request(fn_sign.into());
        let resp = self
            .client()
            .info_rro(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_rro_info_response(resp.into_inner())
    }
}
