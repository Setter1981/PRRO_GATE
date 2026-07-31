//! Minimal admin HTTP server built on axum.
//!
//! Four endpoints, bearer-token gated:
//!
//!   * `GET /admin/health`   → 200 "OK"
//!   * `GET /admin/fns`      → JSON array of `FnSnapshot`
//!   * `GET /admin/sessions` → JSON array of `SessionSnapshot`
//!   * `GET /admin/metrics`  → JSON `MetricsSummary`

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tokio::net::TcpListener;

use super::registry::{FnSnapshot, MetricsSummary, Registry, SessionSnapshot};

/// Admin server runtime config.
#[derive(Debug, Clone)]
pub struct AdminConfig {
    pub bind: String,
    pub auth_token: String,
}

#[derive(Clone)]
struct AppState {
    registry: Arc<Registry>,
    token: String,
}

/// Spawn the admin HTTP server on `cfg.bind`.  Runs until the
/// returned future is dropped.
///
/// # Errors
/// Propagates TCP bind / axum serve errors.
pub async fn serve_admin(cfg: AdminConfig, registry: Arc<Registry>) -> Result<(), std::io::Error> {
    let state = AppState {
        registry,
        token: cfg.auth_token,
    };
    let app = Router::new()
        .route("/admin/health", get(health))
        .route("/admin/fns", get(list_fns))
        .route("/admin/sessions", get(list_sessions))
        .route("/admin/metrics", get(metrics))
        .with_state(state);

    let listener = TcpListener::bind(&cfg.bind).await?;
    tracing::info!(addr = %cfg.bind, "admin http server bound");
    axum::serve(listener, app).await
}

async fn health() -> &'static str {
    "OK"
}

async fn list_fns(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<FnSnapshot>>, StatusCode> {
    require_auth(&state.token, &headers)?;
    Ok(Json(state.registry.snapshot_fns().await))
}

async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SessionSnapshot>>, StatusCode> {
    require_auth(&state.token, &headers)?;
    Ok(Json(state.registry.snapshot_sessions().await))
}

async fn metrics(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MetricsSummary>, StatusCode> {
    require_auth(&state.token, &headers)?;
    Ok(Json(state.registry.aggregate_metrics().await))
}

/// Constant-time bearer-token check.  Empty configured token disables
/// auth entirely (suitable for dev / loopback).
fn require_auth(expected: &str, headers: &HeaderMap) -> Result<(), StatusCode> {
    if expected.is_empty() {
        return Ok(());
    }
    let Some(got) = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if constant_time_eq(got.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Reason kept public so admin-runner applications can introspect.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum AuthOutcome {
    Ok,
    MissingHeader,
    BadToken,
}

impl IntoResponse for AuthOutcome {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Ok => StatusCode::OK.into_response(),
            Self::MissingHeader => StatusCode::UNAUTHORIZED.into_response(),
            Self::BadToken => StatusCode::FORBIDDEN.into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_slices() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn constant_time_eq_rejects_different_length() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }

    #[test]
    fn constant_time_eq_rejects_different_bytes() {
        assert!(!constant_time_eq(b"abc", b"abd"));
    }

    #[test]
    fn require_auth_with_empty_token_always_succeeds() {
        let headers = HeaderMap::new();
        assert!(require_auth("", &headers).is_ok());
    }

    #[test]
    fn require_auth_with_missing_header_yields_unauthorized() {
        let headers = HeaderMap::new();
        let e = require_auth("real-token", &headers).unwrap_err();
        assert_eq!(e, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn require_auth_with_wrong_token_yields_forbidden() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer wrong".parse().unwrap());
        let e = require_auth("real-token", &headers).unwrap_err();
        assert_eq!(e, StatusCode::FORBIDDEN);
    }

    #[test]
    fn require_auth_with_matching_token_succeeds() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer real-token".parse().unwrap());
        require_auth("real-token", &headers).unwrap();
    }

    #[test]
    fn non_bearer_scheme_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Basic abc".parse().unwrap());
        let e = require_auth("real-token", &headers).unwrap_err();
        assert_eq!(e, StatusCode::UNAUTHORIZED);
    }
}
