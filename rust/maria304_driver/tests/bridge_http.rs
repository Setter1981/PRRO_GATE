//! Sprint M7 acceptance — `HttpBridge` end-to-end over real HTTP.
//!
//! Spins up a tiny hyper-based HTTP server, points an `HttpBridge`
//! at it, submits canonical envelopes, asserts the HTTP contract.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use maria304_driver::bridge::dto::{CanonicalCommand, CommandType, ReceiptPayload};
use maria304_driver::bridge::{Bridge, BridgeError, HttpBridge};

/// Captured request bodies + last auth header, shared between the
/// test and the handler task.
#[derive(Default)]
struct CapturedServer {
    bodies: Vec<String>,
    last_auth: Option<String>,
}

/// Minimal HTTP responder — parses just enough of the request to
/// capture the body + Authorization header, then returns a canned
/// response chosen by the test.
async fn serve_one_request(
    listener: &TcpListener,
    response_line: &str,
    response_body: &str,
    captured: Arc<Mutex<CapturedServer>>,
) {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut buf = Vec::with_capacity(1024);
    let mut scratch = [0u8; 512];

    // Read until we find header terminator \r\n\r\n, then body.
    loop {
        let n = socket.read(&mut scratch).await.unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&scratch[..n]);
        if let Some(hdr_end) = find_header_end(&buf) {
            // Extract Content-Length.
            let header_text = std::str::from_utf8(&buf[..hdr_end]).unwrap_or("").to_string();
            let content_length = header_text
                .lines()
                .find_map(|l| {
                    let lower = l.to_ascii_lowercase();
                    lower
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            let auth = header_text.lines().find_map(|l| {
                let lower = l.to_ascii_lowercase();
                lower
                    .strip_prefix("authorization:")
                    .map(|v| v.trim().to_string())
            });
            let body_start = hdr_end + 4;
            while buf.len() < body_start + content_length {
                let n = socket.read(&mut scratch).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&scratch[..n]);
            }
            let body =
                String::from_utf8_lossy(&buf[body_start..body_start + content_length]).to_string();

            {
                let mut c = captured.lock().await;
                c.bodies.push(body);
                c.last_auth = auth;
            }

            let response = format!(
                "{response_line}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{response_body}",
                response_body.len(),
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.flush().await.unwrap();
            break;
        }
    }
    let _ = socket.shutdown().await;
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn spawn_server(response_line: &'static str, response_body: &'static str) -> (String, Arc<Mutex<CapturedServer>>) {
    let captured = Arc::new(Mutex::new(CapturedServer::default()));
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr: SocketAddr = probe.local_addr().unwrap();
    drop(probe);

    let listener = TcpListener::bind(addr).await.unwrap();
    let cap = Arc::clone(&captured);
    tokio::spawn(async move {
        serve_one_request(&listener, response_line, response_body, cap).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (format!("http://{addr}/v1/ingress/maria304"), captured)
}

fn sample_cmd() -> CanonicalCommand {
    CanonicalCommand {
        schema_version: "1.0".to_string(),
        fiscal_number: "FN-TEST".to_string(),
        command_type: CommandType::Sell,
        idempotency_key: "maria304:FN-TEST:sess:1".to_string(),
        cashier_id: Some("csh1".to_string()),
        department: Some("Bar".to_string()),
        return_check_number: None,
        payload: ReceiptPayload::default(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn successful_ack_response_is_parsed_into_canonical_response() {
    let response_body = r#"{
        "ok": true,
        "document_id": "doc-xyz",
        "fiscal_id": "0000042",
        "fiscal_ts": "2026-04-20T10:00:00+00:00",
        "document_state": "ACK",
        "sale_total_kopecks": 1500,
        "return_total_kopecks": 0
    }"#;
    let (url, captured) = spawn_server("HTTP/1.1 200 OK", response_body).await;

    // Run the blocking reqwest call on a dedicated thread so we
    // don't block the test runtime.
    let bridge = tokio::task::spawn_blocking(move || {
        HttpBridge::new(url, "secret-token", Duration::from_millis(500), Duration::from_secs(2))
            .unwrap()
    })
    .await
    .unwrap();

    let result = tokio::task::spawn_blocking(move || bridge.submit(&sample_cmd()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(result.fiscal_id, "0000042");
    assert_eq!(result.document_state, "ACK");
    assert_eq!(result.sale_total_kopecks, 1500);

    let cap = captured.lock().await;
    assert_eq!(cap.bodies.len(), 1);
    // Case-folded to lowercase by the minimal-header-parser used in
    // this test — what matters is the bearer token substring.
    assert_eq!(cap.last_auth.as_deref(), Some("bearer secret-token"));
    // Body is valid JSON and matches our envelope shape.
    let parsed: CanonicalCommand = serde_json::from_str(&cap.bodies[0]).unwrap();
    assert_eq!(parsed.idempotency_key, "maria304:FN-TEST:sess:1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_rejection_with_error_code_surfaces_as_rejected() {
    let body = r#"{
        "ok": false,
        "error_code": "SOFTBADART",
        "error_message": "Unknown article 999"
    }"#;
    let (url, _captured) = spawn_server("HTTP/1.1 400 Bad Request", body).await;

    let bridge = tokio::task::spawn_blocking(move || {
        HttpBridge::new(url, "", Duration::from_millis(500), Duration::from_secs(2)).unwrap()
    })
    .await
    .unwrap();
    let err = tokio::task::spawn_blocking(move || bridge.submit(&sample_cmd()))
        .await
        .unwrap()
        .unwrap_err();

    match err {
        BridgeError::Rejected { code, message } => {
            assert_eq!(code, "SOFTBADART");
            assert!(message.contains("Unknown article"));
        }
        BridgeError::Transport(_) => panic!("expected Rejected, got Transport"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_500_without_error_body_falls_back_to_http_code_as_rejected() {
    let (url, _) = spawn_server("HTTP/1.1 500 Internal Server Error", "not json at all").await;
    let bridge = tokio::task::spawn_blocking(move || {
        HttpBridge::new(url, "", Duration::from_millis(500), Duration::from_secs(2)).unwrap()
    })
    .await
    .unwrap();
    let err = tokio::task::spawn_blocking(move || bridge.submit(&sample_cmd()))
        .await
        .unwrap()
        .unwrap_err();
    match err {
        BridgeError::Rejected { code, .. } => assert_eq!(code, "HTTP_500"),
        BridgeError::Transport(msg) => panic!("unexpected Transport error: {msg}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_token_skips_authorization_header() {
    let body = r#"{
        "ok": true, "document_id": "d", "fiscal_id": "1", "fiscal_ts": "t",
        "document_state": "ACK"
    }"#;
    let (url, captured) = spawn_server("HTTP/1.1 200 OK", body).await;
    let bridge = tokio::task::spawn_blocking(move || {
        HttpBridge::new(url, "", Duration::from_millis(500), Duration::from_secs(2)).unwrap()
    })
    .await
    .unwrap();
    tokio::task::spawn_blocking(move || bridge.submit(&sample_cmd()))
        .await
        .unwrap()
        .unwrap();
    let cap = captured.lock().await;
    assert!(cap.last_auth.is_none(), "empty token must not send Authorization header");
}
