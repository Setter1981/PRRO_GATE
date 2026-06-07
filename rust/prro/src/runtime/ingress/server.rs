//! RS-2 piece-5b-ii — per-listener axum HTTP ingress server.
//!
//! Wraps the axum-free [`handle_command`](super::handler::handle_command)
//! (piece-5a) in a thin axum server: one `POST /v1/ingress/:source` route per
//! `RestHttp` listener, a [`DefaultBodyLimit`], and graceful shutdown driven by
//! the supervisor's shutdown `watch` (RS-2 piece-5b-i task set).
//!
//! **Router A** (`/v1/ingress/:source`): both front-ends POST the identical
//! [`CanonicalCommand`] JSON; `:source` (`webcheck` / `maria304`) is an
//! audit/protocol LABEL only — it does NOT participate in routing or identity.
//! The `(driver_id, fiscal_number)` are stamped EXCLUSIVELY from the listener's
//! [`IngressState`] (via `handle_command`), never the wire — so a `:source`
//! value cannot override which FN a receipt fiscalizes.  An unknown `:source`
//! is `404` and the handler is never invoked.
//!
//! **D2 loopback-only pilot:** there is NO auth/bearer middleware here — the
//! bind address is the trust boundary, enforced fail-closed at supervisor
//! startup by `config::validate_rs2_loopback_binds` (a non-loopback `RestHttp`
//! listener refuses to boot).  The LAN bearer / token-resolver is a separate
//! post-pilot security piece.
//!
//! This module is the axum SHELL only: it deserialises the wire DTO, calls the
//! piece-5a core, and renders [`IngressResponse`] as an HTTP response.  All
//! fiscal logic lives behind the `handle_command` seam.
//!
//! **Pilot-hardening gap (review M-1, TRACKED — not wired here):** there is no
//! per-request / per-connection timeout or connection cap (the `tower-http`
//! `timeout` feature is a declared dep but unused).  Under D2 loopback-only the
//! blast radius is a local buggy/wedged shim holding connections (bounded at
//! shutdown by the supervisor's shared grace), NOT a remote attacker.  Before
//! the bind boundary moves off loopback, wire a
//! `tower_http::timeout::TimeoutLayer` (aligned to the DPS deadline once RS-3
//! adds wire calls) + a hyper header-read timeout + a connection cap.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::db::models::enums::Protocol;
use crate::db::models::ids::{DriverId, RequestId};

use super::dto::{request_id_to_string, CanonicalCommand, CanonicalErrorResponse, SCHEMA_VERSION};
use super::handler::{handle_command, IngressBody, IngressResponse};
use super::seam::WritePathEntry;

/// Max accepted request body.  A fiscal receipt JSON is at most a few KB even
/// with many line items; 1 MiB is far above any real receipt and bounds an
/// abusive/buggy client.  Exceeding it → `413 Payload Too Large` (axum's
/// `DefaultBodyLimit`), the handler is never invoked.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Per-listener axum state — the listener identity + the pools + the
/// inline write-path seam.  `Clone` (cheap: pools are `Arc`-backed, the seam
/// is an `Arc`) so axum can hand each request its own copy.
#[derive(Clone)]
pub struct IngressState {
    pub main_pool: SqlitePool,
    pub secure_pool: SqlitePool,
    /// The listener's configured FN — stamped onto every command (NOT the wire).
    pub listener_fn: String,
    /// The listener's configured driver id — stamped onto every command.
    pub driver_id: DriverId,
    /// The listener's protocol (`RestHttp` → [`Protocol::Rest`]).
    pub protocol: Protocol,
    /// The inline write-path seam — [`UnimplementedWritePath`] pre-RS-3.
    ///
    /// [`UnimplementedWritePath`]: super::seam::UnimplementedWritePath
    pub write_path: Arc<dyn WritePathEntry>,
}

/// Allowed `:source` path segments (router A).  NOT identity/routing — purely
/// an audit/protocol label; the FN + driver_id come ONLY from [`IngressState`].
fn source_allowed(source: &str) -> bool {
    matches!(source, "webcheck" | "maria304")
}

/// Build the per-listener router: `POST /v1/ingress/:source` + body limit +
/// state.  Exposed (not just `serve`) so it can be unit-tested via
/// `tower::ServiceExt::oneshot` without a real socket.
pub fn router(state: IngressState) -> Router {
    Router::new()
        .route("/v1/ingress/:source", post(ingress_post))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// Serve the ingress router on an ALREADY-BOUND listener until the shutdown
/// watch flips.  The bind is done by the caller (supervisor, piece-5b-ii) so a
/// bind failure is a BOOT failure, not a runtime loop-death.  Returns the
/// server result so the F1 seam audits a genuine `serve()` error (the handle is
/// a `GracefulOkAfterShutdown` task — see [`SupervisedTask::graceful`]).
///
/// [`SupervisedTask::graceful`]: crate::runtime::supervisor::SupervisedTask::graceful
pub async fn serve(
    listener: TcpListener,
    state: IngressState,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async move {
            // Resolve when the watch is flipped true (or the sender drops).
            while !*shutdown_rx.borrow() {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("ingress server serve error: {e}"))
}

/// `POST /v1/ingress/:source` — deserialise the wire DTO, run the piece-5a
/// core, render the response.  Adapter-level rejections (unknown source,
/// malformed JSON) mint a fresh `request_id` for correlation and render the
/// same [`CanonicalErrorResponse`] envelope the core uses — never a bare axum
/// error.
async fn ingress_post(
    State(state): State<IngressState>,
    Path(source): Path<String>,
    body: Bytes,
) -> Response {
    if !source_allowed(&source) {
        // review L-2: shell-level rejections are pre-fiscal (no ledger
        // effect) so they are NOT audited, but a burst (a misconfigured
        // front-end / a probe) must be operator-visible — trace it.
        tracing::warn!(
            target: "prro::runtime::ingress",
            fiscal_number = %state.listener_fn,
            source = %source,
            "ingress: rejected unknown source"
        );
        return adapter_error(
            StatusCode::NOT_FOUND,
            "UNKNOWN_SOURCE",
            format!("unknown ingress source {source:?} (expected webcheck or maria304)"),
        );
    }

    let cmd: CanonicalCommand = match serde_json::from_slice(&body) {
        Ok(c) => c,
        Err(e) => {
            // review L-1/L-2: log the parse detail SERVER-SIDE; return a
            // GENERIC message to the wire (do NOT forward a third-party
            // error's Display to the client).
            tracing::warn!(
                target: "prro::runtime::ingress",
                fiscal_number = %state.listener_fn,
                source = %source,
                error = %e,
                "ingress: rejected malformed CanonicalCommand JSON"
            );
            return adapter_error(
                StatusCode::BAD_REQUEST,
                "MALFORMED_JSON",
                "request body is not a valid CanonicalCommand".to_string(),
            );
        }
    };

    let resp = handle_command(
        &cmd,
        &state.listener_fn,
        state.driver_id.clone(),
        state.protocol,
        &state.main_pool,
        &state.secure_pool,
        state.write_path.as_ref(),
    )
    .await;

    into_axum_response(resp)
}

/// Render the core's [`IngressResponse`] as an axum response (status + JSON
/// body).  An out-of-range `http_status` falls back to `500` — never a panic.
fn into_axum_response(resp: IngressResponse) -> Response {
    let status =
        StatusCode::from_u16(resp.http_status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match resp.body {
        IngressBody::Success(r) => (status, Json(r)).into_response(),
        IngressBody::Error(e) => (status, Json(e)).into_response(),
    }
}

/// Build an adapter-level error envelope (mints a fresh `request_id`).
fn adapter_error(status: StatusCode, error_code: &str, error_message: String) -> Response {
    let request_id = request_id_to_string(RequestId::new().as_bytes());
    let body = CanonicalErrorResponse {
        ok: false,
        request_id,
        schema_version: SCHEMA_VERSION.to_string(),
        error_code: error_code.to_string(),
        error_message,
        config_drift: false,
    };
    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::enums::FiscalMode;
    use crate::db::repositories::fiscal_number_config::{insert as fn_insert, NewFnConfig};
    use crate::db::{open_pool, open_secure_pool};
    use crate::runtime::ingress::seam::UnimplementedWritePath;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // oneshot

    const FN: &str = "4000000001";

    async fn fresh_state() -> (tempfile::TempDir, IngressState) {
        let dir = tempfile::tempdir().unwrap();
        let main = open_pool(&dir.path().join("main.db")).await.unwrap();
        let secure = open_secure_pool(&dir.path().join("secure.db"))
            .await
            .unwrap();
        fn_insert(
            &main,
            &NewFnConfig {
                fiscal_number: FN.to_string(),
                tax_number: "12345678".to_string(),
                vat_payer_inn: None,
                fiscal_mode: FiscalMode::Test,
                org_name: None,
                point_name: None,
                org_address: None,
                tsp_enabled: false,
                offline_enabled: true,
                national_check_enabled: false,
                min_offline_codes: 0,
                max_offline_codes: 0,
            },
        )
        .await
        .unwrap();
        let state = IngressState {
            main_pool: main,
            secure_pool: secure,
            listener_fn: FN.to_string(),
            driver_id: DriverId::new("drv-1").unwrap(),
            protocol: Protocol::Rest,
            write_path: Arc::new(UnimplementedWritePath),
        };
        (dir, state)
    }

    fn shift_open_json() -> String {
        format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SHIFT_OPEN",
                "idempotency_key":"k-1","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
        )
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn post(uri: &str, json: String) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(json))
            .unwrap()
    }

    /// A well-formed receipt on an allowed source flows router → handler →
    /// seam → response: pre-RS-3 the seam is NotImplemented → 501.
    #[tokio::test]
    async fn allowed_source_valid_command_reaches_handler() {
        let (_d, state) = fresh_state().await;
        let resp = router(state)
            .oneshot(post("/v1/ingress/webcheck", shift_open_json()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        let body = body_json(resp).await;
        assert_eq!(body["ok"], false);
        assert_eq!(body["error_code"], "NOT_IMPLEMENTED");
    }

    /// `maria304` is also an allowed source (whitelist has both).
    #[tokio::test]
    async fn maria304_source_is_allowed() {
        let (_d, state) = fresh_state().await;
        let resp = router(state)
            .oneshot(post("/v1/ingress/maria304", shift_open_json()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    /// An unknown `:source` is 404 with the typed envelope; the handler is
    /// never invoked.
    #[tokio::test]
    async fn unknown_source_is_404() {
        let (_d, state) = fresh_state().await;
        let resp = router(state)
            .oneshot(post("/v1/ingress/evil", shift_open_json()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = body_json(resp).await;
        assert_eq!(body["error_code"], "UNKNOWN_SOURCE");
    }

    /// Malformed JSON is 400 with the typed envelope (no bare axum error).
    #[tokio::test]
    async fn malformed_json_is_400() {
        let (_d, state) = fresh_state().await;
        let resp = router(state)
            .oneshot(post("/v1/ingress/webcheck", "{not json".to_string()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["error_code"], "MALFORMED_JSON");
    }

    /// A body over the limit is rejected (413) by the body-limit layer before
    /// the handler runs.
    #[tokio::test]
    async fn oversized_body_is_413() {
        let (_d, state) = fresh_state().await;
        let big = "x".repeat(MAX_BODY_BYTES + 1);
        let resp = router(state)
            .oneshot(post("/v1/ingress/webcheck", big))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
