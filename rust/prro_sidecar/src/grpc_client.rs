//! Per-mode persistent tonic channel pool — prod / test DPS endpoints.
//!
//! Wraps `ChkIncomeServiceClient` from the generated proto, providing:
//! - two pre-connected channels (prod + test) with HTTP/2 keep-alive
//! - 5 RPC wrappers with typed error classification
//! - lightweight `GrpcError` that distinguishes transient vs permanent DPS codes

use crate::config::DpsEndpoint;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

// Generated gRPC stubs
use crate::generated::{
    chk_income_service_client::ChkIncomeServiceClient, Check, CheckRequest, CheckResponse,
    RroInfoResponse, StatusResponse,
};

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
        1 => None,                                // OK
        -3 => Some(DpsErrorCategory::Transient),  // ERROR_SAVE — DB write race; retry
        -4 => Some(DpsErrorCategory::Transient),  // ERROR_UNKNOWN — server-side transient; retry
        -12 => Some(DpsErrorCategory::Transient), // ERROR_BAD_HASH_PREV — chain gap; retry after resync
        -1 => Some(DpsErrorCategory::Permanent),  // ERROR_VEREFY — signature verification failed
        -2 => Some(DpsErrorCategory::Permanent),  // ERROR_CHECK — cert/auth check failed
        -5 => Some(DpsErrorCategory::Permanent),  // ERROR_TYPE — unknown doc type
        -6 => Some(DpsErrorCategory::Permanent),  // ERROR_NOT_PREV_ZREPORT — prev Z-report missing
        -7 => Some(DpsErrorCategory::Permanent),  // ERROR_XML — XML malformed
        -8 => Some(DpsErrorCategory::Permanent),  // ERROR_XML_DATE — date field invalid
        -9 => Some(DpsErrorCategory::Permanent),  // ERROR_XML_CHK — check XML invalid
        -10 => Some(DpsErrorCategory::Permanent), // ERROR_XML_ZREPORT — Z-report XML invalid
        -11 => Some(DpsErrorCategory::Permanent), // ERROR_OFFLINE_168 — offline >168h limit
        -13 => Some(DpsErrorCategory::Permanent), // ERROR_NOT_REGISTERED_RRO — RRO not registered
        -14 => Some(DpsErrorCategory::Permanent), // ERROR_NOT_REGISTERED_SIGNER — signer not registered
        -15 => Some(DpsErrorCategory::Permanent), // ERROR_NOT_OPEN_SHIFT — shift not open
        -16 => Some(DpsErrorCategory::Permanent), // ERROR_OFFLINE_ID — offline doc ID conflict
        _ => Some(DpsErrorCategory::Permanent),   // 0 = UNKNOWN or any future code
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
    /// Build pool with lazy-connected channels — actual TCP handshake deferred to first RPC.
    /// Construction succeeds even if DPS endpoints are temporarily unreachable.
    pub fn new(prod_cfg: &DpsEndpoint, test_cfg: &DpsEndpoint) -> Result<Self, GrpcError> {
        let prod_ch = build_channel(prod_cfg)?;
        let test_ch = build_channel(test_cfg)?;
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

fn build_channel(cfg: &DpsEndpoint) -> Result<Channel, GrpcError> {
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

    ep = ep
        .tls_config(tls)
        .map_err(|e: tonic::transport::Error| GrpcError::Tls(e.to_string()))?;
    // connect_lazy defers TCP handshake to first RPC — construction never fails due to network
    Ok(ep.connect_lazy())
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
            code: status.code() as i32,
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
        assert_eq!(classify_dps_status(-12), Some(DpsErrorCategory::Transient));
        // ERROR_BAD_HASH_PREV
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

    // ── A. Invalid endpoint URI ────────────────────────────────────────────────

    #[test]
    fn build_channel_invalid_uri_returns_transport_error() {
        // Endpoint::from_shared rejects non-URI strings immediately.
        // DpsGrpcPool::new is sync (uses connect_lazy), so no async runtime needed.
        use crate::config::DpsEndpoint;
        let bad_cfg = DpsEndpoint {
            endpoint: "not a valid uri at all !!!".to_string(),
            ca_bundle: None,
        };
        let valid_cfg = DpsEndpoint {
            endpoint: "https://example.com:9443".to_string(),
            ca_bundle: None,
        };
        let result = DpsGrpcPool::new(&bad_cfg, &valid_cfg);
        assert!(
            result.is_err(),
            "invalid URI must produce Err from DpsGrpcPool::new, got Ok"
        );
        // Endpoint::from_shared produces tonic::transport::Error which maps via From impl
        // to GrpcError::Transport.
        // Use .err().unwrap() to avoid the T: Debug bound that unwrap_err() requires.
        let err = result.err().unwrap();
        assert!(
            matches!(err, GrpcError::Transport(_)),
            "invalid URI error must be GrpcError::Transport, got: {err}"
        );
    }

    // ── B. connect_lazy succeeds with unreachable endpoints ───────────────────

    #[tokio::test]
    async fn build_pool_succeeds_even_if_endpoints_unreachable() {
        // connect_lazy() defers TCP handshake but internally registers with the
        // Tokio reactor — a runtime context is required even for construction.
        // Construction must succeed for valid URI format even when no server is listening.
        use crate::config::DpsEndpoint;
        let cfg_prod = DpsEndpoint {
            endpoint: "https://127.0.0.1:19999".to_string(), // nothing listening here
            ca_bundle: None,
        };
        let cfg_test = DpsEndpoint {
            endpoint: "https://127.0.0.1:19998".to_string(), // nothing listening here
            ca_bundle: None,
        };
        let result = DpsGrpcPool::new(&cfg_prod, &cfg_test);
        assert!(
            result.is_ok(),
            "connect_lazy must allow construction without a live server"
        );
    }

    // ── C. classify_dps_status edge cases — all named codes individually ──────

    #[test]
    fn classify_all_named_dps_codes_individually() {
        // Verify each named code produces the correct category.
        // Catches accidental regressions if match arms are reordered.
        let transient_codes = [-3i32, -4, -12];
        let permanent_codes = [-1i32, -2, -5, -6, -7, -8, -9, -10, -11, -13, -14, -15, -16];

        for &code in &transient_codes {
            assert_eq!(
                classify_dps_status(code),
                Some(DpsErrorCategory::Transient),
                "code {code} must be Transient"
            );
        }
        for &code in &permanent_codes {
            assert_eq!(
                classify_dps_status(code),
                Some(DpsErrorCategory::Permanent),
                "code {code} must be Permanent"
            );
        }
    }

    #[test]
    fn classify_extreme_negative_code_is_permanent() {
        // Future/unknown codes fall to the wildcard arm — must be Permanent (no retry).
        assert_eq!(
            classify_dps_status(-999),
            Some(DpsErrorCategory::Permanent),
            "unknown code -999 must be Permanent via wildcard"
        );
        assert_eq!(
            classify_dps_status(i32::MIN),
            Some(DpsErrorCategory::Permanent),
            "i32::MIN must be Permanent via wildcard"
        );
        assert_eq!(
            classify_dps_status(999),
            Some(DpsErrorCategory::Permanent),
            "positive unknown code 999 must be Permanent"
        );
    }

    #[test]
    fn classify_status_2_is_permanent() {
        // Status 2 is not in the named list — falls to wildcard.
        assert_eq!(classify_dps_status(2), Some(DpsErrorCategory::Permanent));
    }

    // ── D. map_status for specific gRPC codes ─────────────────────────────────

    #[test]
    fn cancelled_status_maps_to_status_variant_not_deadline() {
        // Only DeadlineExceeded maps to GrpcError::Deadline — Cancelled must NOT.
        let status = tonic::Status::cancelled("operation cancelled");
        let err = map_status(status);
        assert!(
            matches!(err, GrpcError::Status { .. }),
            "Cancelled must map to Status, not Deadline"
        );
    }

    #[test]
    fn internal_status_maps_to_status_variant() {
        let status = tonic::Status::internal("internal server error");
        let err = map_status(status);
        assert!(matches!(err, GrpcError::Status { .. }));
    }

    #[test]
    fn status_variant_preserves_code_and_message() {
        let status = tonic::Status::not_found("fiscal number not registered");
        let err = map_status(status);
        match err {
            GrpcError::Status { code, message } => {
                assert!(code != 0, "code must be non-zero for non-OK status");
                assert!(
                    message.contains("fiscal number not registered"),
                    "message must be preserved, got: {message}"
                );
            }
            other => panic!("expected Status variant, got {other:?}"),
        }
    }
}
