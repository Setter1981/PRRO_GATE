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
//! Happy path × 5 RPCs:
//!   - send_chk_happy_returns_check_ack
//!   - last_chk_happy_returns_check_ack
//!   - ping_happy_returns_check_ack
//!   - status_rro_happy_returns_status_snapshot
//!   - info_rro_happy_returns_rro_info
//!
//! DPS non-OK status mapping:
//!   - send_chk_error_verefy_routes_to_authorization
//!   - send_chk_error_unknown_routes_to_transport_retry_class
//!   - send_chk_error_not_open_shift_routes_to_server_kind
//!   - send_chk_status_unknown_routes_to_decode
//!   - status_rro_error_not_registered_rro_routes_to_authorization
//!
//! tonic::Status mapping:
//!   - tonic_unavailable_routes_to_transport
//!   - tonic_unauthenticated_routes_to_authorization
//!
//! ByServerFiscalNo (PRRO_GATE-5js):
//!   - by_server_fiscal_no_match_returns_ok
//!   - by_server_fiscal_no_mismatch_returns_typed_mismatch
//!   - by_server_fiscal_no_absent_returns_not_found
//!
//! QueryNotSupported + grpc-timeout:
//!   - query_by_local_identity_returns_query_not_supported_without_hitting_server
//!   - grpc_timeout_metadata_set_on_every_request

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
    CheckEnvelope, CheckSignBlob, DpsChannel, DpsCheckType, DpsError, GrpcDpsChannel,
};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Code, Request, Response, Status};

// ─── Mock state + tonic service impl ──────────────────────────────────

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
    /// Captures inbound request metadata (one HashMap per call,
    /// across all RPCs).  Tests that care about metadata pull from
    /// here after the call returns.
    captured_metadata: Mutex<Vec<HashMap<String, String>>>,
}

impl MockDpsState {
    fn capture(&self, md: &tonic::metadata::MetadataMap) {
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
        self.captured_metadata.lock().unwrap().push(snapshot);
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
        self.state.capture(request.metadata());
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
        self.state.capture(request.metadata());
        match self.state.last_chk.lock().unwrap().pop_front() {
            Some(Ok(r)) => Ok(Response::new(r)),
            Some(Err(s)) => Err(s),
            None => Err(Status::internal("mock: no scripted last_chk response")),
        }
    }

    async fn ping(&self, request: Request<Check>) -> Result<Response<CheckResponse>, Status> {
        self.state.capture(request.metadata());
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
        self.state.capture(request.metadata());
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
        self.state.capture(request.metadata());
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
    assert!(
        matches!(err, DpsError::Authorization(ref m) if m.contains("ERROR_VEREFY")),
        "expected Authorization with ERROR_VEREFY, got {err:?}"
    );
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
    assert!(
        matches!(err, DpsError::Transport(ref m) if m.contains("ERROR_UNKNOWN") || m.contains("retry-class")),
        "expected Transport (retry-class), got {err:?}"
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
    assert!(
        matches!(err, DpsError::Authorization(ref m) if m.contains("ERROR_NOT_REGISTERED_RRO")),
        "expected Authorization, got {err:?}"
    );
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
async fn tonic_unauthenticated_routes_to_authorization() {
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
        matches!(err, DpsError::Authorization(ref m) if m.contains("Unauthenticated")),
        "expected Authorization, got {err:?}"
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
        h.state.captured_metadata.lock().unwrap().is_empty(),
        "default-body QueryNotSupported must not hit the server"
    );
}

#[tokio::test]
async fn grpc_timeout_metadata_set_on_every_request() {
    let h = start_mock().await;
    h.state
        .send_chk_v2
        .lock()
        .unwrap()
        .push_back(Ok(ok_check_response("FN-METADATA-PROBE")));
    let ch = channel(&h.endpoint).await;
    let _ = ch.send_chk(check_envelope()).await.expect("happy");

    let captured = h.state.captured_metadata.lock().unwrap();
    assert_eq!(captured.len(), 1, "exactly one call captured");
    let md = &captured[0];
    let timeout = md.get("grpc-timeout").expect(
        "grpc-timeout header MUST be present (set via tonic::Request::set_timeout in W3-C3)",
    );
    // tonic encodes the duration as `<num><unit>` where unit is one
    // of n / u / m / S / M / H.  We seeded the channel with
    // `Duration::from_secs(5)`; tonic 0.12 typically renders this as
    // milliseconds (e.g., "5000m") or larger (5S).  Both are valid;
    // we assert the format shape, not the unit choice.
    let last = timeout.chars().last().expect("non-empty grpc-timeout");
    assert!(
        matches!(last, 'n' | 'u' | 'm' | 'S' | 'M' | 'H'),
        "grpc-timeout must end with a gRPC unit suffix; got {timeout:?}"
    );
    let num: &str = &timeout[..timeout.len() - 1];
    assert!(
        num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty(),
        "grpc-timeout numeric prefix invalid in {timeout:?}"
    );
}
