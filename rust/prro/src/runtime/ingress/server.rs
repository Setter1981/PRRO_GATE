//! RS-2 piece-5b-ii — per-listener axum HTTP ingress server.
//!
//! Wraps the axum-free [`handle_command`](super::handler::handle_command)
//! (piece-5a) in a thin axum server per `RestHttp` listener: a
//! `POST /v1/ingress/:source` ingress route + a read-only
//! `GET /v1/status/:fn` route (piece-6), a [`DefaultBodyLimit`], and graceful
//! shutdown driven by the supervisor's shutdown `watch` (RS-2 piece-5b-i task
//! set).
//!
//! **Router A** (`/v1/ingress/:source`): the front-end POSTs the
//! [`CanonicalCommand`] JSON; `:source` (`webcheck`; `maria304` gated off until
//! its consumer mirror is `Option<fiscal_id>` — see [`source_allowed`]) is an
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
use axum::routing::{get, post};
use axum::{Json, Router};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::db::models::enums::Protocol;
use crate::db::models::ids::{DriverId, RequestId};
use crate::db::repositories::{fiscal_documents, node_state, offline_sessions};

use super::dto::{
    request_id_to_string, CanonicalCommand, CanonicalErrorResponse, OfflineSessionInfo,
    StatusResponse, SCHEMA_VERSION,
};
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
///
/// GATED to `webcheck` only (the pilot front-end).  `maria304` is deliberately
/// NOT accepted yet: its `maria304_driver` response mirror declares
/// `fiscal_id: String` (required), so a (post-RS-3) `OFFLINE_LOCAL_ACK` success
/// with `fiscal_id: null` would FAIL-decode on that consumer.  Rejecting the
/// source now (→ 404 `UNKNOWN_SOURCE`) turns a latent post-RS-3 decode break on
/// the critical path into an explicit boundary error.  RE-ADD `"maria304"` here
/// once the `maria304_driver` mirror is made `Option<fiscal_id>` (RS-3
/// obligation; see the `dto.rs` maria304-parity note).
fn source_allowed(source: &str) -> bool {
    matches!(source, "webcheck")
}

/// Build the per-listener router: `POST /v1/ingress/:source` + body limit +
/// state.  Exposed (not just `serve`) so it can be unit-tested via
/// `tower::ServiceExt::oneshot` without a real socket.
pub fn router(state: IngressState) -> Router {
    Router::new()
        .route("/v1/ingress/:source", post(ingress_post))
        .route("/v1/status/:fn", get(status_get))
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
            format!("unknown ingress source {source:?} (expected webcheck)"),
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

/// `GET /v1/status/:fn` (RS-2 piece-6) — read-only status for the WebCheck
/// shim's Initialization / GetCurrentStatus.  A pure projection of
/// `node_state` + the last server `ACK` + the active offline session: NO
/// write-path side effects, all reads pool-bound outside any tx.
///
/// **CF1 FN-enforce:** this listener serves ONE `(driver_id, fn)`; reading a
/// DIFFERENT `:fn` through it is cross-FN data access (acute under D2
/// loopback / no-token) → `403`.  A configured FN with no `node_state` row
/// yet → `404` (no runtime state).
async fn status_get(
    State(state): State<IngressState>,
    Path(requested_fn): Path<String>,
) -> Response {
    // CF1 — only the listener's own FN is readable here.
    //
    // post-pilot LAN (review-r1 A-Low, deferred under D2 loopback-only): the
    // 403 (cross-FN) vs 404 (own-FN-not-initialized) split + echoing the
    // configured FN here is an FN-existence oracle, and `offline_session.id` /
    // `current_shift_id` expose internal uuids.  Harmless under loopback (the
    // caller is the local trusted shim + the FN is in the local config), but
    // when the LAN bearer/token-resolver lands, collapse 403/404 to one opaque
    // code, stop echoing the FN, and re-confirm the shim needs the ids.
    if requested_fn != state.listener_fn {
        return adapter_error(
            StatusCode::FORBIDDEN,
            "FN_FORBIDDEN",
            format!(
                "this listener serves fn {:?}, not {requested_fn:?}",
                state.listener_fn
            ),
        );
    }

    let ns = match node_state::get(&state.main_pool, &state.listener_fn).await {
        Ok(Some(ns)) => ns,
        Ok(None) => {
            return adapter_error(
                StatusCode::NOT_FOUND,
                "NO_NODE_STATE",
                format!("fn {:?} has no runtime state yet", state.listener_fn),
            )
        }
        Err(e) => return status_read_error(&state.listener_fn, "node_state", &e),
    };

    let last_server_fiscal_no =
        match fiscal_documents::last_server_fiscal_no(&state.main_pool, &state.listener_fn).await {
            Ok(v) => v,
            Err(e) => return status_read_error(&state.listener_fn, "last_server_fiscal_no", &e),
        };

    let offline_session = match offline_sessions::current_open_or_draining_session(
        &state.main_pool,
        &state.listener_fn,
    )
    .await
    {
        Ok(Some((id, st))) => Some(OfflineSessionInfo {
            id: request_id_to_string(id.as_bytes()),
            state: st.as_str().to_string(),
        }),
        Ok(None) => None,
        Err(e) => return status_read_error(&state.listener_fn, "offline_session", &e),
    };

    let body = StatusResponse {
        schema_version: SCHEMA_VERSION.to_string(),
        fiscal_number: state.listener_fn.clone(),
        node_mode: ns.mode.as_str().to_string(),
        shift_state: ns.shift_state.as_str().to_string(),
        current_shift_id: ns
            .current_shift_id
            .map(|s| request_id_to_string(s.as_bytes())),
        next_local_number: ns.next_lnd,
        next_z_report_number: ns.next_z_report_number,
        last_server_fiscal_no,
        offline_session,
    };
    (StatusCode::OK, Json(body)).into_response()
}

/// A status-read DB error → traced (server-side) + a generic `500` (no
/// internal detail to the wire).
fn status_read_error(fiscal_number: &str, what: &str, err: &sqlx::Error) -> Response {
    tracing::error!(
        target: "prro::runtime::ingress",
        fiscal_number = %fiscal_number,
        read = what,
        error = %err,
        "status: read failed"
    );
    adapter_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL",
        "status read failed".to_string(),
    )
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

    /// `maria304` is GATED OFF until its consumer mirror is `Option<fiscal_id>`
    /// (else a post-RS-3 `OFFLINE_LOCAL_ACK` null fiscal_id would fail-decode on
    /// that driver) — so the source is rejected 404 `UNKNOWN_SOURCE`, never
    /// reaching the handler.  Re-enable the whitelist entry + flip this test
    /// when the maria304 mirror is fixed.
    #[tokio::test]
    async fn maria304_source_is_gated_off() {
        let (_d, state) = fresh_state().await;
        let resp = router(state)
            .oneshot(post("/v1/ingress/maria304", shift_open_json()))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = body_json(resp).await;
        assert_eq!(body["error_code"], "UNKNOWN_SOURCE");
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

    // ─── piece-6 — GET /v1/status/:fn ────────────────────────────────────

    use crate::db::models::enums::{NodeMode, ShiftState};

    fn get(uri: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    /// Status projects `node_state` (mode / shift / next_lnd) + carries
    /// `schema_version`; with no acked doc + no offline session both are null.
    #[tokio::test]
    async fn status_reflects_node_state() {
        let (_d, state) = fresh_state().await;
        node_state::upsert_initial(
            &state.main_pool,
            FN,
            NodeMode::Online,
            ShiftState::Closed,
            5,
        )
        .await
        .unwrap();
        let resp = router(state)
            .oneshot(get(&format!("/v1/status/{FN}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let b = body_json(resp).await;
        assert_eq!(b["schema_version"], "1.0");
        assert_eq!(b["fiscal_number"], FN);
        assert_eq!(b["node_mode"], "ONLINE");
        assert_eq!(b["shift_state"], "CLOSED");
        assert_eq!(b["next_local_number"], 5);
        assert!(b["last_server_fiscal_no"].is_null());
        assert!(b["offline_session"].is_null());
    }

    /// CF1 — reading a DIFFERENT FN through this listener is forbidden (403).
    #[tokio::test]
    async fn status_cross_fn_is_403() {
        let (_d, state) = fresh_state().await;
        node_state::upsert_initial(
            &state.main_pool,
            FN,
            NodeMode::Online,
            ShiftState::Closed,
            1,
        )
        .await
        .unwrap();
        let resp = router(state)
            .oneshot(get("/v1/status/4000000999"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let b = body_json(resp).await;
        assert_eq!(b["error_code"], "FN_FORBIDDEN");
    }

    /// A configured FN with no `node_state` row yet → 404 (no runtime state).
    #[tokio::test]
    async fn status_no_node_state_is_404() {
        let (_d, state) = fresh_state().await;
        let resp = router(state)
            .oneshot(get(&format!("/v1/status/{FN}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let b = body_json(resp).await;
        assert_eq!(b["error_code"], "NO_NODE_STATE");
    }

    /// An active offline session surfaces as `offline_session: {id, state}`.
    #[tokio::test]
    async fn status_surfaces_active_offline_session() {
        let (_d, state) = fresh_state().await;
        node_state::upsert_initial(
            &state.main_pool,
            FN,
            NodeMode::Offline,
            ShiftState::Opened,
            1,
        )
        .await
        .unwrap();
        let sid = [0x55u8; 16];
        sqlx::query(
            "INSERT INTO offline_sessions (offline_session_id, fiscal_number, state, opened_at) \
             VALUES (?, ?, 'OPEN', ?)",
        )
        .bind(sid.as_slice())
        .bind(FN)
        .bind("2026-06-07T00:00:00Z")
        .execute(&state.main_pool)
        .await
        .unwrap();
        let resp = router(state)
            .oneshot(get(&format!("/v1/status/{FN}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let b = body_json(resp).await;
        assert_eq!(b["offline_session"]["state"], "OPEN");
        assert_eq!(
            b["offline_session"]["id"], "55555555555555555555555555555555",
            "offline session id is lowercase-32-hex"
        );
        // review-r1 (C-Medium-1): pin the wire enum strings — a NodeMode /
        // ShiftState rename must break THIS surface's contract, not slip
        // through (status_reflects_node_state covers ONLINE/CLOSED).
        assert_eq!(b["node_mode"], "OFFLINE");
        assert_eq!(b["shift_state"], "OPENED");
    }

    /// Seed a terminal `ACK` doc with a given `server_fiscal_no` at `lnd`.
    async fn seed_ack(pool: &SqlitePool, lnd: i64, server_fiscal_no: &str) {
        use crate::db::models::enums::DocType;
        use crate::db::models::ids::{DocumentId, RequestId};
        let new = fiscal_documents::NewDocument {
            document_id: DocumentId::new(),
            request_id: RequestId::new(),
            fiscal_number: FN.to_string(),
            shift_id: None,
            offline_session_id: None,
            lnd,
            doc_type: DocType::Sell,
            backend_profile_id: "b".to_string(),
            transport_profile_id: "t".to_string(),
            fs_mode: "ONLINE",
            business_ts: "2026-06-07T00:00:00Z".to_string(),
            total_sum_kop: Some(15000),
            payload_json: "{}".to_string(),
            payload_sha256_canonical: [0u8; 32],
            source_sha256: [0u8; 32],
            unsigned_xml_sha256: None,
            previous_hash: None,
            signed_by_cashier_id: None,
            signing_config_snapshot_id: None,
        };
        fiscal_documents::insert_prepared(pool, &new).await.unwrap();
        sqlx::query(
            "UPDATE fiscal_documents SET state = 'ACK', server_fiscal_no = ? \
             WHERE fiscal_number = ? AND lnd = ?",
        )
        .bind(server_fiscal_no)
        .bind(FN)
        .bind(lnd)
        .execute(pool)
        .await
        .unwrap();
    }

    /// review-r1 (Low): the status endpoint surfaces a REAL `last_server_fiscal_no`
    /// (not just the null case) — the HTTP projection of an acked receipt.
    #[tokio::test]
    async fn status_surfaces_last_server_fiscal_no() {
        let (_d, state) = fresh_state().await;
        node_state::upsert_initial(
            &state.main_pool,
            FN,
            NodeMode::Online,
            ShiftState::Closed,
            9,
        )
        .await
        .unwrap();
        seed_ack(&state.main_pool, 8, "777042").await;
        let resp = router(state)
            .oneshot(get(&format!("/v1/status/{FN}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let b = body_json(resp).await;
        assert_eq!(b["last_server_fiscal_no"], "777042");
    }
}
