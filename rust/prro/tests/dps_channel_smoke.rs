//! W3-C4 — DPS gRPC transport integration smoke against a native
//! tonic mock server.
//!
//! C3 wired the real RPC bodies + status mapping but landed no
//! transport-level tests.  C4 (this file) closes that gap with a
//! native Rust tonic mock server bound to an ephemeral-port
//! `TcpListener`, scripts the 5 RPC + tonic::Status outcomes from
//! the test side, and asserts the typed `DpsChannel` surface routes
//! every documented case correctly.
//!
//! Coverage matrix (per W0-1 review):
//!
//! Happy path × 5 RPCs (each asserts both the typed response AND the
//! outbound proto body — proves `From<CheckEnvelope> for gen::Check`
//! and `From<&CheckSignBlob> for gen::CheckRequest` actually wire the
//! field values onto the wire):
//!   - send_chk_happy_returns_check_ack
//!   - last_chk_happy_returns_check_ack
//!   - ping_happy_returns_check_ack
//!   - status_rro_happy_returns_status_snapshot
//!   - info_rro_happy_returns_rro_info
//!
//! DPS non-OK status mapping (decoder split per ADR-M3-A6 prereq):
//!   - send_chk_error_verefy_routes_to_authorization
//!     (-1 → kind = DocumentReject; CheckResponse arm)
//!   - send_chk_error_not_registered_rro_routes_to_authorization_fn_not_registered
//!     (-13 → kind = FiscalNumberNotRegistered; CheckResponse arm)
//!   - send_chk_error_not_registered_signer_routes_to_authorization_fn_not_registered
//!     (-14 → kind = FiscalNumberNotRegistered; CheckResponse arm)
//!   - send_chk_error_unknown_routes_to_transport_retry_class
//!   - send_chk_error_not_open_shift_routes_to_server_kind
//!   - send_chk_status_unknown_routes_to_decode
//!   - status_rro_error_not_registered_rro_routes_to_authorization
//!     (-13 → kind = FiscalNumberNotRegistered; StatusResponse arm —
//!     additional decoder coverage on the status_rro path)
//!
//! tonic::Status mapping:
//!   - tonic_unavailable_routes_to_transport
//!   - tonic_unauthenticated_routes_to_transport
//!     (gRPC `Unauthenticated` is transport-level — no DPS status code,
//!     so it maps to DpsError::Transport rather than Authorization;
//!     see ADR-M3-A6 prereq.)
//!
//! ByServerFiscalNo (PRRO_GATE-5js):
//!   - by_server_fiscal_no_match_returns_ok
//!   - by_server_fiscal_no_mismatch_returns_typed_mismatch
//!   - by_server_fiscal_no_absent_returns_not_found
//!
//! QueryNotSupported + grpc-timeout:
//!   - query_by_local_identity_returns_query_not_supported_without_hitting_server
//!   - grpc_timeout_metadata_set_on_every_rpc (all 5 RPCs in one
//!     test; closes the W3-C4 review finding that the previous
//!     single-RPC test did not prove the "every request" claim)

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use prro::transports::dps::gen::chk_income_service_server::{
    ChkIncomeService, ChkIncomeServiceServer,
};
use prro::transports::dps::gen::{
    check_response, rro_info_response, status_response, Check, CheckRequest, CheckResponse,
    RroInfoResponse, StatusResponse,
};
use prro::transports::dps::{
    AuthorizationKind, CheckEnvelope, CheckSignBlob, DpsChannel, DpsCheckType, DpsError,
    GrpcDpsChannel,
};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Code, Request, Response, Status};

// ─── Mock state + tonic service impl ──────────────────────────────────

/// Inbound request body, captured per-RPC before the mock pops a
/// scripted response.  Tests assert on these to prove the
/// `From<CheckEnvelope> for gen::Check` / `From<&CheckSignBlob> for
/// gen::CheckRequest` conversions in `dto.rs` actually wire field
/// values onto the wire — without this, a buggy conversion (field
/// swap, dropped value) would still pass C4 because mock responses
/// are scripted independently of the inbound request.
#[derive(Debug)]
enum CapturedBody {
    SendChk(Check),
    LastChk(CheckRequest),
    Ping(Check),
    StatusRro(CheckRequest),
    InfoRro(CheckRequest),
}

/// One captured call: metadata + body, correlated.  `Vec<CapturedCall>`
/// preserves arrival order across all RPCs so the
/// `grpc_timeout_metadata_set_on_every_rpc` test can verify each of
/// the 5 calls carries the deadline header.
#[derive(Debug)]
struct CapturedCall {
    metadata: HashMap<String, String>,
    body: CapturedBody,
}

#[derive(Default)]
struct MockDpsState {
    /// Per-RPC scripted response queues.  Each test pushes the
    /// response(s) it wants; the mock pops them in arrival order.
    /// `None` after exhaustion → the mock returns `Status::internal`
    /// (a "test bug" signal, distinct from any documented routing).
    send_chk_v2: Mutex<VecDeque<Result<CheckResponse, Status>>>,
    last_chk: Mutex<VecDeque<Result<CheckResponse, Status>>>,
    ping: Mutex<VecDeque<Result<CheckResponse, Status>>>,
    status_rro: Mutex<VecDeque<Result<StatusResponse, Status>>>,
    info_rro: Mutex<VecDeque<Result<RroInfoResponse, Status>>>,
    /// Every inbound RPC, in arrival order, with both metadata and
    /// the deserialised body.  Tests that care about either pull
    /// from here after the call returns.
    captured: Mutex<Vec<CapturedCall>>,
}

impl MockDpsState {
    /// Pure metadata snapshot (does not push); the per-RPC handler
    /// pairs the result with the inbound body and pushes one
    /// `CapturedCall`.
    fn snapshot_metadata(&self, md: &tonic::metadata::MetadataMap) -> HashMap<String, String> {
        let mut snapshot: HashMap<String, String> = HashMap::new();
        for kv in md.iter() {
            // Ascii-only keys; binary metadata is irrelevant for the
            // grpc-timeout / user-agent / content-type assertions we
            // make in C4.
            if let tonic::metadata::KeyAndValueRef::Ascii(k, v) = kv {
                snapshot.insert(
                    k.to_string(),
                    v.to_str().unwrap_or("<non-ascii>").to_string(),
                );
            }
        }
        snapshot
    }
}

#[derive(Clone)]
struct MockDpsService {
    state: Arc<MockDpsState>,
}

#[async_trait]
impl ChkIncomeService for MockDpsService {
    async fn send_chk_v2(
        &self,
        request: Request<Check>,
    ) -> Result<Response<CheckResponse>, Status> {
        let metadata = self.state.snapshot_metadata(request.metadata());
        let body = request.into_inner();
        self.state.captured.lock().unwrap().push(CapturedCall {
            metadata,
            body: CapturedBody::SendChk(body),
        });
        match self.state.send_chk_v2.lock().unwrap().pop_front() {
            Some(Ok(r)) => Ok(Response::new(r)),
            Some(Err(s)) => Err(s),
            None => Err(Status::internal("mock: no scripted send_chk_v2 response")),
        }
    }

    async fn last_chk(
        &self,
        request: Request<CheckRequest>,
    ) -> Result<Response<CheckResponse>, Status> {
        let metadata = self.state.snapshot_metadata(request.metadata());
        let body = request.into_inner();
        self.state.captured.lock().unwrap().push(CapturedCall {
            metadata,
            body: CapturedBody::LastChk(body),
        });
        match self.state.last_chk.lock().unwrap().pop_front() {
            Some(Ok(r)) => Ok(Response::new(r)),
            Some(Err(s)) => Err(s),
            None => Err(Status::internal("mock: no scripted last_chk response")),
        }
    }

    async fn ping(&self, request: Request<Check>) -> Result<Response<CheckResponse>, Status> {
        let metadata = self.state.snapshot_metadata(request.metadata());
        let body = request.into_inner();
        self.state.captured.lock().unwrap().push(CapturedCall {
            metadata,
            body: CapturedBody::Ping(body),
        });
        match self.state.ping.lock().unwrap().pop_front() {
            Some(Ok(r)) => Ok(Response::new(r)),
            Some(Err(s)) => Err(s),
            None => Err(Status::internal("mock: no scripted ping response")),
        }
    }

    async fn status_rro(
        &self,
        request: Request<CheckRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let metadata = self.state.snapshot_metadata(request.metadata());
        let body = request.into_inner();
        self.state.captured.lock().unwrap().push(CapturedCall {
            metadata,
            body: CapturedBody::StatusRro(body),
        });
        match self.state.status_rro.lock().unwrap().pop_front() {
            Some(Ok(r)) => Ok(Response::new(r)),
            Some(Err(s)) => Err(s),
            None => Err(Status::internal("mock: no scripted status_rro response")),
        }
    }

    async fn info_rro(
        &self,
        request: Request<CheckRequest>,
    ) -> Result<Response<RroInfoResponse>, Status> {
        let metadata = self.state.snapshot_metadata(request.metadata());
        let body = request.into_inner();
        self.state.captured.lock().unwrap().push(CapturedCall {
            metadata,
            body: CapturedBody::InfoRro(body),
        });
        match self.state.info_rro.lock().unwrap().pop_front() {
            Some(Ok(r)) => Ok(Response::new(r)),
            Some(Err(s)) => Err(s),
            None => Err(Status::internal("mock: no scripted info_rro response")),
        }
    }
}

// ─── Test scaffolding ─────────────────────────────────────────────────

struct Harness {
    state: Arc<MockDpsState>,
    endpoint: String,
    /// tokio task running the tonic server.  Aborted on drop.
    server_handle: JoinHandle<()>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.server_handle.abort();
    }
}

async fn start_mock() -> Harness {
    let listener = tokio::net::TcpListener::bind::<SocketAddr>("127.0.0.1:0".parse().unwrap())
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let endpoint = format!("http://{addr}");

    let state = Arc::new(MockDpsState::default());
    let service = MockDpsService {
        state: state.clone(),
    };
    let server_handle = tokio::spawn(async move {
        // serve_with_incoming returns when the underlying stream is
        // dropped or the task is aborted; we abort on Harness::drop.
        let _ = tonic::transport::Server::builder()
            .add_service(ChkIncomeServiceServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await;
    });
    Harness {
        state,
        endpoint,
        server_handle,
    }
}

async fn channel(endpoint: &str) -> GrpcDpsChannel {
    GrpcDpsChannel::connect(endpoint, Duration::from_secs(5))
        .await
        .expect("connect to mock")
}

fn check_envelope() -> CheckEnvelope {
    CheckEnvelope {
        rro_fn: "1234567890".into(),
        date_time: 1_700_000_000,
        check_sign: b"<signed-cms-bytes>".to_vec(),
        local_number: 42,
        check_type: DpsCheckType::Chk,
        id_offline: String::new(),
        id_cancel: String::new(),
    }
}

fn fn_sign() -> CheckSignBlob {
    CheckSignBlob(b"<rro-fn-sign>".to_vec())
}

fn ok_check_response(id: &str) -> CheckResponse {
    CheckResponse {
        id: id.to_string(),
        status: check_response::Status::Ok as i32,
        id_sign: b"<id-sign>".to_vec(),
        data_sign: b"<data-sign>".to_vec(),
        error_message: String::new(),
    }
}

fn err_check_response(status: check_response::Status, message: &str) -> CheckResponse {
    CheckResponse {
        id: String::new(),
        status: status as i32,
        id_sign: Vec::new(),
        data_sign: Vec::new(),
        error_message: message.to_string(),
    }
}

// ─── Happy paths × 5 RPCs ─────────────────────────────────────────────

/// Assert exactly one call was captured and pull its body, asserting
/// it is the expected `CapturedBody` variant.  Returns the inner
/// proto type for further field-level assertions.
fn expect_one_send_chk_body(state: &MockDpsState) -> Check {
    let captured = state.captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "expected exactly one captured call");
    match &captured[0].body {
        CapturedBody::SendChk(c) => c.clone(),
        other => panic!("expected SendChk body, got {other:?}"),
    }
}

fn expect_one_last_chk_body(state: &MockDpsState) -> CheckRequest {
    let captured = state.captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "expected exactly one captured call");
    match &captured[0].body {
        CapturedBody::LastChk(r) => r.clone(),
        other => panic!("expected LastChk body, got {other:?}"),
    }
}

fn expect_one_ping_body(state: &MockDpsState) -> Check {
    let captured = state.captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "expected exactly one captured call");
    match &captured[0].body {
        CapturedBody::Ping(c) => c.clone(),
        other => panic!("expected Ping body, got {other:?}"),
    }
}

fn expect_one_status_rro_body(state: &MockDpsState) -> CheckRequest {
    let captured = state.captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "expected exactly one captured call");
    match &captured[0].body {
        CapturedBody::StatusRro(r) => r.clone(),
        other => panic!("expected StatusRro body, got {other:?}"),
    }
}

fn expect_one_info_rro_body(state: &MockDpsState) -> CheckRequest {
    let captured = state.captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "expected exactly one captured call");
    match &captured[0].body {
        CapturedBody::InfoRro(r) => r.clone(),
        other => panic!("expected InfoRro body, got {other:?}"),
    }
}

/// Assert that a `gen::Check` body matches the canonical envelope
/// produced by `From<CheckEnvelope> for gen::Check`.  Field-by-field
/// so a future regression in `dto.rs` (field swap, dropped value,
/// truncated bytes) fails loudly here rather than silently when a
/// real DPS rejects the malformed wire payload.
fn assert_check_body_matches_canonical_envelope(c: &Check) {
    use prro::transports::dps::gen::check::Type;
    assert_eq!(c.rro_fn, "1234567890");
    assert_eq!(c.date_time, 1_700_000_000);
    assert_eq!(c.check_sign, b"<signed-cms-bytes>");
    assert_eq!(c.local_number, 42);
    assert_eq!(c.check_type, Type::Chk as i32);
    assert_eq!(c.id_offline, "");
    assert_eq!(c.id_cancel, "");
}

#[tokio::test]
async fn send_chk_happy_returns_check_ack() {
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("FN-OK-1")));
    let ch = channel(&h.endpoint).await;
    let ack = ch.send_chk(check_envelope()).await.expect("happy");
    assert_eq!(ack.id, "FN-OK-1");
    assert_eq!(ack.id_sign, b"<id-sign>");
    assert_eq!(ack.data_sign, b"<data-sign>");

    // Body assertion: the wire payload must reflect the typed
    // envelope passed to send_chk.  Catches a regression in
    // `From<CheckEnvelope> for gen::Check` that a response-only
    // assertion would miss.
    let body = expect_one_send_chk_body(&h.state);
    assert_check_body_matches_canonical_envelope(&body);
}

#[tokio::test]
async fn last_chk_happy_returns_check_ack() {
    let h = start_mock().await;
    h.state
        .last_chk
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("FN-LAST-9")));
    let ch = channel(&h.endpoint).await;
    let ack = ch.last_chk(&fn_sign()).await.expect("happy");
    assert_eq!(ack.id, "FN-LAST-9");

    // Body assertion: rro_fn_sign must equal the bytes wrapped in
    // CheckSignBlob.  Catches a regression in
    // `From<&CheckSignBlob> for gen::CheckRequest`.
    let body = expect_one_last_chk_body(&h.state);
    assert_eq!(body.rro_fn_sign, b"<rro-fn-sign>");
}

#[tokio::test]
async fn ping_happy_returns_check_ack() {
    let h = start_mock().await;
    h.state
        .ping
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("PONG")));
    let ch = channel(&h.endpoint).await;
    let ack = ch.ping(check_envelope()).await.expect("happy");
    assert_eq!(ack.id, "PONG");

    // ping shares the Check shape with send_chk; same canonical
    // envelope must reach the wire.
    let body = expect_one_ping_body(&h.state);
    assert_check_body_matches_canonical_envelope(&body);
}

#[tokio::test]
async fn status_rro_happy_returns_status_snapshot() {
    let h = start_mock().await;
    h.state
        .status_rro
        .lock()
        .unwrap()
        .push_back(Ok(StatusResponse {
            open_shift: true,
            online: true,
            last_signer: "OP-007".into(),
            status: status_response::Status::Ok as i32,
            error_message: String::new(),
        }));
    let ch = channel(&h.endpoint).await;
    let snap = ch.status_rro(&fn_sign()).await.expect("happy");
    assert!(snap.open_shift);
    assert!(snap.online);
    assert_eq!(snap.last_signer, "OP-007");

    let body = expect_one_status_rro_body(&h.state);
    assert_eq!(body.rro_fn_sign, b"<rro-fn-sign>");
}

#[tokio::test]
async fn info_rro_happy_returns_rro_info() {
    let h = start_mock().await;
    h.state
        .info_rro
        .lock()
        .unwrap()
        .push_back(Ok(RroInfoResponse {
            status: rro_info_response::Status::Ok as i32,
            status_rro: 1,
            open_shift: true,
            online: true,
            last_signer: "OP-007".into(),
            name: "Shop".into(),
            name_to: "Receipt-To".into(),
            addr: "Kyiv".into(),
            single_tax: false,
            offline_allowed: true,
            add_num: 1,
            pn: "PN-1".into(),
            operators: Vec::new(),
            tins: "TINS".into(),
            lnum: 12,
            name_pay: "Pay-Name".into(),
        }));
    let ch = channel(&h.endpoint).await;
    let info = ch.info_rro(&fn_sign()).await.expect("happy");
    assert_eq!(info.name, "Shop");
    assert_eq!(info.addr, "Kyiv");
    assert!(info.offline_allowed);

    let body = expect_one_info_rro_body(&h.state);
    assert_eq!(body.rro_fn_sign, b"<rro-fn-sign>");
}

// ─── DPS non-OK status mapping ────────────────────────────────────────

#[tokio::test]
async fn send_chk_error_verefy_routes_to_authorization() {
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(err_check_response(
            check_response::Status::ErrorVerefy,
            "signature did not verify",
        )));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("ErrorVerefy must error");
    // Typed assertion per ADR-M3-A6 prereq: -1 → kind=DocumentReject.
    match err {
        DpsError::Authorization {
            code,
            kind,
            ref message,
        } => {
            assert_eq!(code, check_response::Status::ErrorVerefy as i32);
            assert_eq!(code, -1, "ADR-M3-A6: ERROR_VEREFY wire code is -1");
            assert_eq!(kind, AuthorizationKind::DocumentReject);
            assert!(
                message.contains("ERROR_VEREFY"),
                "message must carry the typed status name; got {message}"
            );
        }
        other => {
            panic!("expected Authorization {{ code:-1, kind:DocumentReject, .. }}, got {other:?}")
        }
    }
}

#[tokio::test]
async fn send_chk_error_not_registered_rro_routes_to_authorization_fn_not_registered() {
    // ADR-M3-A6 prereq + W0-3 §2.1 row -13 — same kind as -14 but on
    // the CheckResponse decoder path (status_rro coverage of -13 lives
    // in `status_rro_error_not_registered_rro_routes_to_authorization`
    // and is preserved as additional decoder coverage; this fixture
    // pins the CheckResponse arm in `try_decode_check_response`
    // (dto.rs :178 area), which would otherwise be untested for -13
    // even though -1 and -14 are both covered through send_chk).
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(err_check_response(
            check_response::Status::ErrorNotRegisteredRro,
            "FN not registered",
        )));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("ErrorNotRegisteredRro must error");
    match err {
        DpsError::Authorization {
            code,
            kind,
            ref message,
        } => {
            assert_eq!(code, check_response::Status::ErrorNotRegisteredRro as i32);
            assert_eq!(
                code, -13,
                "ADR-M3-A6: ERROR_NOT_REGISTERED_RRO wire code is -13"
            );
            assert_eq!(kind, AuthorizationKind::FiscalNumberNotRegistered);
            assert!(
                message.contains("ERROR_NOT_REGISTERED_RRO"),
                "message must carry the typed status name; got {message}"
            );
        }
        other => panic!(
            "expected Authorization {{ code:-13, kind:FiscalNumberNotRegistered, .. }}, got {other:?}"
        ),
    }
}

#[tokio::test]
async fn send_chk_error_not_registered_signer_routes_to_authorization_fn_not_registered() {
    // ADR-M3-A6 prereq + W0-3 §2.1 row -14:
    // ERROR_NOT_REGISTERED_SIGNER → Authorization { kind: FiscalNumberNotRegistered }.
    // Same kind as -13 (FN-config failure, operator must reconcile creds);
    // the dto.rs decoder collapses both -13 and -14 onto the same
    // `FiscalNumberNotRegistered` arm but preserves the distinct wire
    // code so the audit trail keeps -13 vs -14 distinguishable.
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(err_check_response(
            check_response::Status::ErrorNotRegisteredSigner,
            "signer cert not registered",
        )));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("ErrorNotRegisteredSigner must error");
    match err {
        DpsError::Authorization {
            code,
            kind,
            ref message,
        } => {
            assert_eq!(
                code,
                check_response::Status::ErrorNotRegisteredSigner as i32
            );
            assert_eq!(code, -14, "ADR-M3-A6: ERROR_NOT_REGISTERED_SIGNER wire code is -14");
            assert_eq!(kind, AuthorizationKind::FiscalNumberNotRegistered);
            assert!(
                message.contains("ERROR_NOT_REGISTERED_SIGNER"),
                "message must carry the typed status name; got {message}"
            );
        }
        other => panic!(
            "expected Authorization {{ code:-14, kind:FiscalNumberNotRegistered, .. }}, got {other:?}"
        ),
    }
}

#[tokio::test]
async fn send_chk_error_unknown_routes_to_transport_retry_class() {
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(err_check_response(
            check_response::Status::ErrorUnknown,
            "transient",
        )));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("ErrorUnknown must error");
    // CS-3 Slice A: `-4` (ERROR_UNKNOWN) no longer collapses into `Transport` (where it
    // was indistinguishable from a network timeout — the double-issue root); it now
    // surfaces as the parsed-but-outcome-unsettling `Indeterminate`. The retry class is
    // UNCHANGED (`route_dps_error` routes `Indeterminate` to `TransientRetry`, exactly as
    // `Transport` did) — this slice only re-types the signal so the CS-3 classifier can
    // tell `-4` from a real timeout. (Legacy test name retained to keep the inventory
    // additions-only gate green; the assertion is the updated contract.)
    assert!(
        matches!(err, DpsError::Indeterminate { code: -4, .. }),
        "expected Indeterminate {{ code: -4 }} (CS-3 Slice A re-typed -4), got {err:?}"
    );
}

#[tokio::test]
async fn send_chk_error_not_open_shift_routes_to_server_kind() {
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(err_check_response(
            check_response::Status::ErrorNotOpenShift,
            "no open shift",
        )));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("ErrorNotOpenShift must error");
    match err {
        DpsError::Server { code, ref message } => {
            assert_eq!(code, check_response::Status::ErrorNotOpenShift as i32);
            assert_eq!(message, "no open shift");
        }
        other => panic!("expected Server {{ code, message }}, got {other:?}"),
    }
}

#[tokio::test]
async fn send_chk_status_unknown_routes_to_decode() {
    let h = start_mock().await;
    // status = Unknown (0) — proto3 default; means the field was
    // missing on the wire; W0-1 review rule: route to Decode.
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(CheckResponse {
            id: String::new(),
            status: check_response::Status::Unknown as i32,
            id_sign: Vec::new(),
            data_sign: Vec::new(),
            error_message: String::new(),
        }));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("Unknown must error");
    assert!(
        matches!(err, DpsError::Decode(ref m) if m.contains("Unknown=0") || m.contains("Unknown")),
        "expected Decode for Unknown, got {err:?}"
    );
}

#[tokio::test]
async fn status_rro_error_not_registered_rro_routes_to_authorization() {
    let h = start_mock().await;
    h.state
        .status_rro
        .lock()
        .unwrap()
        .push_back(Ok(StatusResponse {
            open_shift: false,
            online: false,
            last_signer: String::new(),
            status: status_response::Status::ErrorNotRegisteredRro as i32,
            error_message: "FN not registered".into(),
        }));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .status_rro(&fn_sign())
        .await
        .expect_err("ErrorNotRegisteredRro must error");
    // Typed assertion per ADR-M3-A6 prereq:
    // -13 → kind=FiscalNumberNotRegistered.
    match err {
        DpsError::Authorization {
            code,
            kind,
            ref message,
        } => {
            assert_eq!(code, status_response::Status::ErrorNotRegisteredRro as i32);
            assert_eq!(code, -13, "ADR-M3-A6: ERROR_NOT_REGISTERED_RRO wire code is -13");
            assert_eq!(kind, AuthorizationKind::FiscalNumberNotRegistered);
            assert!(
                message.contains("ERROR_NOT_REGISTERED_RRO"),
                "message must carry the typed status name; got {message}"
            );
        }
        other => panic!(
            "expected Authorization {{ code:-13, kind:FiscalNumberNotRegistered, .. }}, got {other:?}"
        ),
    }
}

// ─── tonic::Status mapping ────────────────────────────────────────────

#[tokio::test]
async fn tonic_unavailable_routes_to_transport() {
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Err(Status::new(Code::Unavailable, "service down")));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("Unavailable must error");
    assert!(
        matches!(err, DpsError::Transport(ref m) if m.contains("Unavailable")),
        "expected Transport, got {err:?}"
    );
}

#[tokio::test]
async fn tonic_unauthenticated_routes_to_transport() {
    // CS-3 Slice A′ + R1 (TLS trust boundary). The law:
    //   RemoteStatus ⇔ code ∈ {Unauthenticated, PermissionDenied} ∧ TLS peer-auth proven.
    // gRPC `Unauthenticated` / `PermissionDenied` promote to `DpsError::RemoteStatus`
    // (AuthenticatedPeer provenance) ONLY over a TLS-proven channel.  This smoke test
    // connects over PLAINTEXT `http://` (the in-process mock), so the peer is UNPROVEN —
    // a plaintext MITM could forge those statuses — and R1 routes them to `Transport`,
    // NEVER `RemoteStatus`.  This test pins the trust boundary itself; the TLS-proven
    // mapping (Unproven → Transport vs TlsProven → RemoteStatus, both codes) is unit-tested
    // in grpc.rs.  Recovery is UNCHANGED (route_dps_error routes both to `TransientRetry`).
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Err(Status::new(Code::Unauthenticated, "creds rejected")));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .send_chk(check_envelope())
        .await
        .expect_err("Unauthenticated must error");
    assert!(
        matches!(err, DpsError::Transport(_)),
        "plaintext (Unproven) channel must route gRPC Unauthenticated to Transport, not \
         RemoteStatus (R1 provenance-forgery fix); got {err:?}"
    );
}

// ─── ByServerFiscalNo (PRRO_GATE-5js) ─────────────────────────────────

#[tokio::test]
async fn by_server_fiscal_no_match_returns_ok() {
    let h = start_mock().await;
    h.state
        .last_chk
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("FN-MATCH")));
    let ch = channel(&h.endpoint).await;
    let ack = ch
        .by_server_fiscal_no(&fn_sign(), "FN-MATCH")
        .await
        .expect("match must return Ok");
    assert_eq!(ack.id, "FN-MATCH");
}

#[tokio::test]
async fn by_server_fiscal_no_mismatch_returns_typed_mismatch() {
    let h = start_mock().await;
    h.state
        .last_chk
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("FN-WHAT-SERVER-HAS")));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .by_server_fiscal_no(&fn_sign(), "FN-WHAT-CALLER-EXPECTS")
        .await
        .expect_err("mismatch must error");
    match err {
        DpsError::ServerFiscalIdMismatch {
            expected_id,
            actual_id,
        } => {
            assert_eq!(expected_id, "FN-WHAT-CALLER-EXPECTS");
            assert_eq!(actual_id, "FN-WHAT-SERVER-HAS");
        }
        other => panic!("expected ServerFiscalIdMismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn by_server_fiscal_no_absent_returns_not_found() {
    let h = start_mock().await;
    // Empty id signals "no record for this FN" per W0-1 + 5js.
    h.state
        .last_chk
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("")));
    let ch = channel(&h.endpoint).await;
    let err = ch
        .by_server_fiscal_no(&fn_sign(), "FN-ANYTHING")
        .await
        .expect_err("absent must error");
    assert!(
        matches!(err, DpsError::NotFound),
        "expected NotFound, got {err:?}"
    );
}

// ─── QueryNotSupported + grpc-timeout ─────────────────────────────────

#[tokio::test]
async fn query_by_local_identity_returns_query_not_supported_without_hitting_server() {
    let h = start_mock().await;
    let ch = channel(&h.endpoint).await;
    // Default trait body returns QueryNotSupported synchronously,
    // never touches the wire.  Captured-metadata vector should
    // remain empty after the call.
    let err = ch
        .query_by_local_identity("1234567890", 42)
        .await
        .expect_err("must surface QueryNotSupported");
    assert!(
        matches!(err, DpsError::QueryNotSupported(name) if name == "query_by_local_identity"),
        "expected QueryNotSupported(\"query_by_local_identity\"), got {err:?}"
    );
    assert!(
        h.state.captured.lock().unwrap().is_empty(),
        "default-body QueryNotSupported must not hit the server"
    );
}

/// Validate a single `grpc-timeout` header value: `<num><unit>` where
/// unit is one of n / u / m / S / M / H per the gRPC HTTP/2 spec.
/// tonic 0.12 picks the unit; we don't lock to a specific choice
/// because it can vary with the configured Duration value.
fn assert_grpc_timeout_shape(value: &str) {
    let last = value.chars().last().expect("non-empty grpc-timeout");
    assert!(
        matches!(last, 'n' | 'u' | 'm' | 'S' | 'M' | 'H'),
        "grpc-timeout must end with a gRPC unit suffix; got {value:?}"
    );
    let num: &str = &value[..value.len() - 1];
    assert!(
        num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty(),
        "grpc-timeout numeric prefix invalid in {value:?}"
    );
}

#[tokio::test]
async fn grpc_timeout_metadata_set_on_every_rpc() {
    let h = start_mock().await;
    // Script one OK response per RPC so all five calls land happy
    // and we accumulate five captured entries.
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("a")));
    h.state
        .last_chk
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("b")));
    h.state
        .ping
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("c")));
    h.state
        .status_rro
        .lock()
        .unwrap()
        .push_back(Ok(StatusResponse {
            open_shift: false,
            online: false,
            last_signer: String::new(),
            status: status_response::Status::Ok as i32,
            error_message: String::new(),
        }));
    h.state
        .info_rro
        .lock()
        .unwrap()
        .push_back(Ok(RroInfoResponse {
            status: rro_info_response::Status::Ok as i32,
            status_rro: 0,
            open_shift: false,
            online: false,
            last_signer: String::new(),
            name: String::new(),
            name_to: String::new(),
            addr: String::new(),
            single_tax: false,
            offline_allowed: false,
            add_num: 0,
            pn: String::new(),
            operators: Vec::new(),
            tins: String::new(),
            lnum: 0,
            name_pay: String::new(),
        }));

    let ch = channel(&h.endpoint).await;
    let _ = ch.send_chk(check_envelope()).await.expect("send_chk");
    let _ = ch.last_chk(&fn_sign()).await.expect("last_chk");
    let _ = ch.ping(check_envelope()).await.expect("ping");
    let _ = ch.status_rro(&fn_sign()).await.expect("status_rro");
    let _ = ch.info_rro(&fn_sign()).await.expect("info_rro");

    let captured = h.state.captured.lock().unwrap();
    assert_eq!(
        captured.len(),
        5,
        "all five RPCs should be captured; got {captured:?}"
    );
    // Per-call: every entry must carry the grpc-timeout header
    // (set by GrpcDpsChannel::request via tonic::Request::set_timeout
    // in W3-C3, commit 580ed20).  Closes the C4 review finding that
    // the previous test only proved the header on send_chk.
    for (i, call) in captured.iter().enumerate() {
        let timeout = call.metadata.get("grpc-timeout").unwrap_or_else(|| {
            panic!(
                "grpc-timeout MUST be present on RPC #{i} ({:?})",
                std::mem::discriminant(&call.body)
            )
        });
        assert_grpc_timeout_shape(timeout);
    }
}
