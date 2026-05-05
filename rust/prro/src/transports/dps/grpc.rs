//! `GrpcDpsChannel` — production `DpsChannel` impl backed by a tonic
//! `ChkIncomeServiceClient`.
//!
//! C2 (this commit) lands the substrate: a stateless wrapper that
//! holds ONE long-lived `tonic::transport::Channel` (Arc-cloning is
//! cheap; HTTP/2 connections are reused across calls) and a default
//! per-call deadline.  All five `DpsChannel` methods stub out to
//! `Err(DpsError::Internal("W3-C3-not-yet-wired: …"))` — no panic
//! path; loud failure if a caller hits this in production.
//!
//! C3 wires the real method bodies (typed-DTO ↔ generated-prost
//! conversions, status-enum dispatch, `tonic::Status` → `DpsError`
//! mapping).  C4 lands the native tonic mock server + integration
//! tests covering the 5 error categories + ByServerFiscalNo
//! match/mismatch/absent triple.

use std::time::Duration;

use async_trait::async_trait;
use tonic::transport::{Channel, Endpoint};

use super::channel::DpsChannel;
use super::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use super::error::DpsError;

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
    #[allow(dead_code)] // consumed in C3 for diagnostic logging
    endpoint: String,
    /// Default per-call deadline applied to every RPC.
    #[allow(dead_code)] // consumed in C3 when each RPC sets the deadline
    request_timeout: Duration,
    #[allow(dead_code)] // consumed in C3 when each RPC issues a tonic call
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
}

#[async_trait]
impl DpsChannel for GrpcDpsChannel {
    async fn send_chk(&self, _envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        Err(DpsError::Internal(
            "W3-C3-not-yet-wired: GrpcDpsChannel::send_chk".into(),
        ))
    }

    async fn last_chk(&self, _fn_sign: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        Err(DpsError::Internal(
            "W3-C3-not-yet-wired: GrpcDpsChannel::last_chk".into(),
        ))
    }

    async fn ping(&self, _envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        Err(DpsError::Internal(
            "W3-C3-not-yet-wired: GrpcDpsChannel::ping".into(),
        ))
    }

    async fn status_rro(&self, _fn_sign: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        Err(DpsError::Internal(
            "W3-C3-not-yet-wired: GrpcDpsChannel::status_rro".into(),
        ))
    }

    async fn info_rro(&self, _fn_sign: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        Err(DpsError::Internal(
            "W3-C3-not-yet-wired: GrpcDpsChannel::info_rro".into(),
        ))
    }
}
