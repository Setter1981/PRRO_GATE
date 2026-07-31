//! End-to-end HTTP + TCP tests for prro_escpos_daemon.
//!
//! Each test spins up the Axum app on an ephemeral port, hits it
//! with reqwest, and asserts the response.  The `/print` test also
//! spins up a tokio TCP listener acting as a fake printer so we can
//! verify the bytes that actually reach the wire.

use prro_escpos_daemon::{
    api::{build_router, AppState},
    ProfileRegistry,
};
use std::sync::Arc;

async fn start_daemon() -> String {
    let state = AppState {
        registry: Arc::new(ProfileRegistry::new()),
        default_timeout_ms: 2_000,
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn health_returns_live_json() {
    let base = start_daemon().await;
    let resp: serde_json::Value = reqwest::get(format!("{base}/health"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp["live"], true);
    assert!(resp["profiles_loaded"].as_u64().unwrap() >= 3);
}

#[tokio::test]
async fn profiles_lists_bundled() {
    let base = start_daemon().await;
    let resp: serde_json::Value = reqwest::get(format!("{base}/profiles"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let profiles = resp["profiles"].as_array().unwrap();
    let keys: Vec<String> = profiles
        .iter()
        .filter_map(|p| p["key"].as_str().map(str::to_owned))
        .collect();
    assert!(keys.contains(&"tm-t88ii".to_string()));
    assert!(keys.contains(&"pp8000l".to_string()));
    assert!(keys.contains(&"cts310ii".to_string()));
}

#[tokio::test]
async fn compile_simple_receipt_returns_expected_hex() {
    let base = start_daemon().await;
    let body = serde_json::json!({
        "profile": "tm-t88ii",
        "instructions": [
            {"op": "init"},
            {"op": "codepage", "value": "cp866"},
            {"op": "align", "value": "center"},
            {"op": "text", "value": "HELLO"},
            {"op": "newline"},
            {"op": "feed", "value": 2},
            {"op": "cut"},
        ]
    });
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/compile"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let hex_str = resp["bytes_hex"].as_str().unwrap();
    // Known ESC/POS sequence.
    assert_eq!(
        hex_str,
        "1b401b74111b6101" // init, cp866, center
            .to_string() + "48454c4c4f"         // "HELLO"
            + "0a"                              // LF
            + "1b6402"                          // feed 2
            + "1d5601", // cut DEFAULT
    );
    assert_eq!(resp["length"].as_u64().unwrap(), 20);
}

#[tokio::test]
async fn compile_unknown_profile_returns_404() {
    let base = start_daemon().await;
    let body = serde_json::json!({"profile": "nope", "instructions": []});
    let resp = reqwest::Client::new()
        .post(format!("{base}/compile"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn compile_bad_hex_in_raw_returns_400() {
    let base = start_daemon().await;
    let body = serde_json::json!({
        "profile": "tm-t88ii",
        "instructions": [{"op": "raw", "hex": "zz"}]
    });
    let resp = reqwest::Client::new()
        .post(format!("{base}/compile"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn print_sends_bytes_over_tcp_to_listener() {
    // Fake printer: accept one connection, read everything, stash.
    let printer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let printer_addr = printer.local_addr().unwrap();
    let received = Arc::new(tokio::sync::Mutex::new(Vec::<u8>::new()));
    let received_clone = received.clone();
    tokio::spawn(async move {
        let (mut sock, _) = printer.accept().await.unwrap();
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        sock.read_to_end(&mut buf).await.unwrap();
        *received_clone.lock().await = buf;
    });

    let base = start_daemon().await;
    let body = serde_json::json!({
        "profile": "tm-t88ii",
        "instructions": [
            {"op": "init"},
            {"op": "raw", "hex": "414243"}, // "ABC"
            {"op": "newline"},
            {"op": "cut"},
        ],
        "destination": {
            "type": "tcp",
            "host": printer_addr.ip().to_string(),
            "port": printer_addr.port(),
        }
    });
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/print"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp["ok"], true);

    // Give the spawned receiver a tick to finish draining.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let got = received.lock().await.clone();
    // Expected: ESC @ + "ABC" + LF + GS V 01
    let expected: Vec<u8> = [&[0x1B, 0x40][..], b"ABC", b"\n", &[0x1D, 0x56, 0x01]].concat();
    assert_eq!(got, expected);
}

#[tokio::test]
async fn print_tcp_unreachable_returns_502() {
    let base = start_daemon().await;
    let body = serde_json::json!({
        "profile": "tm-t88ii",
        "instructions": [{"op": "init"}, {"op": "cut"}],
        "destination": {
            "type": "tcp",
            "host": "127.0.0.1",
            // Unused port — RST immediately on connect attempt.
            "port": 1u16,
        },
        "timeout_ms": 500u64,
    });
    let resp = reqwest::Client::new()
        .post(format!("{base}/print"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
}
