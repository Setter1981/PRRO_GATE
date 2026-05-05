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
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Status};

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
    /// Default per-call deadline applied to every RPC.  Configured on
    /// the `Endpoint` at construction time so every clone of the
    /// channel inherits the same deadline; not re-applied per call.
    #[allow(dead_code)] // documented contract; tonic Endpoint enforces it
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
        let ep = Endpoint::from_shared(endpoint.to_string())
            .map_err(|e| DpsError::Transport(format!("invalid endpoint URI: {e}")))?
            .timeout(request_timeout)
            .connect_timeout(request_timeout);
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
}

/// Map a `tonic::Status` (gRPC-level error) to a `DpsError` per the
/// W0-1 review rules:
///
/// - `Unavailable` / `DeadlineExceeded` → `Transport` (retry-class)
/// - `Unauthenticated` / `PermissionDenied` → `Authorization`
/// - everything else → `Transport` (conservative; M3 ops layer will
///   tighten this if a specific gRPC code needs different routing)
fn map_tonic_status(s: Status) -> DpsError {
    match s.code() {
        Code::Unavailable | Code::DeadlineExceeded => {
            DpsError::Transport(format!("gRPC {:?}: {}", s.code(), s.message()))
        }
        Code::Unauthenticated | Code::PermissionDenied => {
            DpsError::Authorization(format!("gRPC {:?}: {}", s.code(), s.message()))
        }
        _ => DpsError::Transport(format!("gRPC {:?}: {}", s.code(), s.message())),
    }
}

#[async_trait]
impl DpsChannel for GrpcDpsChannel {
    async fn send_chk(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        let req = tonic::Request::new(envelope.into());
        let resp = self
            .client()
            .send_chk_v2(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_check_response(resp.into_inner())
    }

    async fn last_chk(&self, fn_sign: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        let req = tonic::Request::new(fn_sign.into());
        let resp = self
            .client()
            .last_chk(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_check_response(resp.into_inner())
    }

    async fn ping(&self, envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        let req = tonic::Request::new(envelope.into());
        let resp = self.client().ping(req).await.map_err(map_tonic_status)?;
        try_decode_check_response(resp.into_inner())
    }

    async fn status_rro(&self, fn_sign: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        let req = tonic::Request::new(fn_sign.into());
        let resp = self
            .client()
            .status_rro(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_status_response(resp.into_inner())
    }

    async fn info_rro(&self, fn_sign: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        let req = tonic::Request::new(fn_sign.into());
        let resp = self
            .client()
            .info_rro(req)
            .await
            .map_err(map_tonic_status)?;
        try_decode_rro_info_response(resp.into_inner())
    }
}
