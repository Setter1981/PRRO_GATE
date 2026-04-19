//! Per-mode persistent tonic channel pool — prod / test DPS endpoints.
//!
//! Wraps `ChkIncomeServiceClient` from the generated proto, providing:
//! - two pre-connected channels (prod + test) with HTTP/2 keep-alive
//! - 5 RPC wrappers with typed error classification
//! - lightweight `GrpcError` that distinguishes transient vs permanent DPS codes

use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use crate::config::DpsEndpoint;

// Generated gRPC stubs
use crate::generated::{
    chk_income_service_client::ChkIncomeServiceClient,
    Check, CheckRequest, CheckResponse, StatusResponse, RroInfoResponse,
};
use crate::generated::check_response::Status as ChkStatus;

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    #[error("transport: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("DPS RPC status {code}: {message}")]
    Status { code: i32, message: String },
    #[error("deadline exceeded")]
    Deadline,
    #[error("TLS config: {0}")]
    Tls(String),
}

/// Classify a `CheckResponse.status` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpsErrorCategory {
    /// Retry is likely to help (server-side transient error)
    Transient,
    /// Retry will NOT help — document or registration is wrong
    Permanent,
}

pub fn classify_dps_status(status: i32) -> Option<DpsErrorCategory> {
    match status {
        1 => None, // OK
        -3 | -4 | -12 => Some(DpsErrorCategory::Transient), // ERROR_SAVE, ERROR_UNKNOWN, ERROR_BAD_HASH_PREV
        _ => Some(DpsErrorCategory::Permanent),
    }
}

// ─── Pool ─────────────────────────────────────────────────────────────────────

/// Holds one pre-connected client per fiscal mode (prod + test).
/// Clone is cheap — tonic Channel is Arc-backed.
#[derive(Clone)]
pub struct DpsGrpcPool {
    prod: ChkIncomeServiceClient<Channel>,
    test: ChkIncomeServiceClient<Channel>,
}

impl DpsGrpcPool {
    /// Construct and connect both channels eagerly.
    /// Warmup ping failure is logged but does not prevent construction.
    pub async fn new(
        prod_cfg: &DpsEndpoint,
        test_cfg: &DpsEndpoint,
    ) -> Result<Self, GrpcError> {
        let prod_ch = build_channel(prod_cfg).await?;
        let test_ch = build_channel(test_cfg).await?;
        Ok(Self {
            prod: ChkIncomeServiceClient::new(prod_ch),
            test: ChkIncomeServiceClient::new(test_ch),
        })
    }

    /// Return a client for the given fiscal mode.
    pub fn for_mode(&self, mode: &crate::repo::FiscalMode) -> ChkIncomeServiceClient<Channel> {
        match mode {
            crate::repo::FiscalMode::Prod => self.prod.clone(),
            crate::repo::FiscalMode::Test => self.test.clone(),
        }
    }
}

async fn build_channel(cfg: &DpsEndpoint) -> Result<Channel, GrpcError> {
    use std::time::Duration;

    let mut ep = Endpoint::from_shared(cfg.endpoint.clone())?
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_timeout(Duration::from_secs(10))
        .keep_alive_while_idle(true);

    // TLS: apply custom CA bundle if provided, otherwise system roots
    let tls = if let Some(bundle_path) = &cfg.ca_bundle {
        let pem = std::fs::read(bundle_path)
            .map_err(|e| GrpcError::Tls(format!("read CA bundle {bundle_path}: {e}")))?;
        let cert = tonic::transport::Certificate::from_pem(pem);
        ClientTlsConfig::new().ca_certificate(cert)
    } else {
        ClientTlsConfig::new().with_enabled_roots()
    };

    ep = ep.tls_config(tls).map_err(|e: tonic::transport::Error| GrpcError::Tls(e.to_string()))?;
    Ok(ep.connect().await?)
}

// ─── RPC wrappers ─────────────────────────────────────────────────────────────

impl DpsGrpcPool {
    pub async fn send_chk_v2(
        &self,
        mode: &crate::repo::FiscalMode,
        req: Check,
    ) -> Result<CheckResponse, GrpcError> {
        let mut client = self.for_mode(mode);
        let resp = client.send_chk_v2(req).await.map_err(map_status)?;
        Ok(resp.into_inner())
    }

    pub async fn last_chk(
        &self,
        mode: &crate::repo::FiscalMode,
        req: CheckRequest,
    ) -> Result<CheckResponse, GrpcError> {
        let mut client = self.for_mode(mode);
        let resp = client.last_chk(req).await.map_err(map_status)?;
        Ok(resp.into_inner())
    }

    pub async fn ping(
        &self,
        mode: &crate::repo::FiscalMode,
        req: Check,
    ) -> Result<CheckResponse, GrpcError> {
        let mut client = self.for_mode(mode);
        let resp = client.ping(req).await.map_err(map_status)?;
        Ok(resp.into_inner())
    }

    pub async fn status_rro(
        &self,
        mode: &crate::repo::FiscalMode,
        req: CheckRequest,
    ) -> Result<StatusResponse, GrpcError> {
        let mut client = self.for_mode(mode);
        let resp = client.status_rro(req).await.map_err(map_status)?;
        Ok(resp.into_inner())
    }

    pub async fn info_rro(
        &self,
        mode: &crate::repo::FiscalMode,
        req: CheckRequest,
    ) -> Result<RroInfoResponse, GrpcError> {
        let mut client = self.for_mode(mode);
        let resp = client.info_rro(req).await.map_err(map_status)?;
        Ok(resp.into_inner())
    }
}

fn map_status(status: tonic::Status) -> GrpcError {
    if status.code() == tonic::Code::DeadlineExceeded {
        GrpcError::Deadline
    } else {
        GrpcError::Status {
            code:    status.code() as i32,
            message: status.message().to_string(),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_ok_returns_none() {
        assert_eq!(classify_dps_status(1), None);
    }

    #[test]
    fn classify_transient_codes() {
        assert_eq!(classify_dps_status(-3), Some(DpsErrorCategory::Transient)); // ERROR_SAVE
        assert_eq!(classify_dps_status(-4), Some(DpsErrorCategory::Transient)); // ERROR_UNKNOWN
        assert_eq!(classify_dps_status(-12), Some(DpsErrorCategory::Transient)); // ERROR_BAD_HASH_PREV
    }

    #[test]
    fn classify_permanent_codes() {
        for code in [-1i32, -2, -5, -6, -7, -8, -9, -10, -11, -13, -14, -15, -16] {
            assert_eq!(
                classify_dps_status(code),
                Some(DpsErrorCategory::Permanent),
                "code {code} should be Permanent"
            );
        }
    }

    #[test]
    fn classify_unknown_status_is_permanent() {
        // 0 = UNKNOWN — no retry value, treat as permanent
        assert_eq!(classify_dps_status(0), Some(DpsErrorCategory::Permanent));
    }

    #[test]
    fn deadline_status_maps_correctly() {
        let status = tonic::Status::deadline_exceeded("test");
        let err = map_status(status);
        assert!(matches!(err, GrpcError::Deadline));
    }

    #[test]
    fn non_deadline_status_maps_to_status_variant() {
        let status = tonic::Status::unavailable("server down");
        let err = map_status(status);
        assert!(matches!(err, GrpcError::Status { .. }));
    }
}
