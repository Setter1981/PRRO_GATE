# ADR-004: Rust Fiscal Driver — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python gRPC transport + Node.js signing sidecar with a single Rust `prro_sidecar` binary that handles cp1251 encoding, CMS signing, and gRPC `sendChkV2` via a persistent TLS channel — eliminating the WebCheck-style per-check reconnect overhead.

**Architecture:** `prro_sidecar` (axum HTTP server) extends the existing `prro_crypto` Rust crate as a `[[bin]]` target behind a `sidecar` cargo feature gate. Python introduces a `FiscalSidecarTransport` that posts signed XML + fiscal metadata to `POST /fiscal/sign_and_send`. The write_path requires zero changes — `PassthroughCryptoProvider` is used so `sign_raw()` returns the XML bytes unchanged, and the sidecar does the real signing internally.

**Tech Stack:** Rust (axum 0.7, tonic 0.12, prost 0.13, encoding_rs 0.8), existing prro_crypto CMS layer, Python httpx for transport client.

---

## File Map

| Path | Action | Responsibility |
|------|--------|----------------|
| `rust/prro_crypto/Cargo.toml` | Modify | `sidecar` feature gate; new deps; `[[bin]]` |
| `rust/prro_crypto/build.rs` | Create | `tonic_build::compile_protos` gated on feature |
| `rust/prro_crypto/proto/check.proto` | Create | DPS protobuf schema (reconstructed from WebCheck) |
| `rust/prro_crypto/src/fiscal/mod.rs` | Create | `pub mod grpc_client` (cfg sidecar) |
| `rust/prro_crypto/src/fiscal/grpc_client.rs` | Create | Persistent tonic channel; `send_chk_v2`; `ping` warmup |
| `rust/prro_crypto/src/interop/prro/mod.rs` | Modify | `pub fn load_signing_key()` → `LoadedKey` |
| `rust/prro_crypto/src/bin/prro_sidecar.rs` | Create | axum server: `/fiscal/sign_and_send` + `/health` |
| `rust/prro_crypto/src/lib.rs` | Modify | `#[cfg(feature="sidecar")] pub mod fiscal` |
| `src/prro_gateway/enums.py` | Modify | `DPS_PRRO_FISCAL_SIDECAR` in `TransportKind` |
| `src/prro_gateway/transports/fiscal_sidecar.py` | Create | `FiscalSidecarTransport` |
| `src/prro_gateway/transports/__init__.py` | Modify | Export `FiscalSidecarTransport` |
| `src/prro_gateway/runtime/container.py` | Modify | Wire `DPS_PRRO_FISCAL_SIDECAR` handler |

---

### Task 0: Cargo.toml `sidecar` feature + proto schema + module skeleton

**Goal:** Establish the build scaffolding so `cargo check --features sidecar` compiles cleanly and the proto file is in place.

**Files:**
- Modify: `rust/prro_crypto/Cargo.toml`
- Create: `rust/prro_crypto/build.rs`
- Create: `rust/prro_crypto/proto/check.proto`
- Create: `rust/prro_crypto/src/fiscal/mod.rs`
- Create: `rust/prro_crypto/src/bin/prro_sidecar.rs`
- Modify: `rust/prro_crypto/src/lib.rs`

**Acceptance Criteria:**
- [ ] `cargo check` (no feature) still passes — existing cdylib Python build unaffected
- [ ] `cargo check --features sidecar` passes with new deps resolved
- [ ] `proto/check.proto` present with all 7 fields of `Check` and `ResponseStatus` negative values
- [ ] `src/fiscal/mod.rs` and `src/bin/prro_sidecar.rs` exist as stubs

**Verify:** `cd rust/prro_crypto && cargo check && cargo check --features sidecar` → both exit 0

**Steps:**

- [ ] **Step 1: Add deps and feature gate to Cargo.toml**

  Edit `rust/prro_crypto/Cargo.toml`. Add the following to `[dependencies]`:

  ```toml
  # Serde — promoted from dev-deps; needed for sidecar JSON API
  serde      = { version = "1.0", features = ["derive"] }
  serde_json = "1.0"
  # cp1251 encoding for DPS XML — lightweight pure-Rust, always available
  encoding_rs = "0.8"

  # --- sidecar feature deps (not compiled into the Python cdylib) ---
  axum        = { version = "0.7",  optional = true }
  tokio       = { version = "1",    features = ["full"],               optional = true }
  tonic       = { version = "0.12", features = ["tls", "tls-roots"],   optional = true }
  prost       = { version = "0.13",                                    optional = true }
  ```

  Add to `[build-dependencies]`:

  ```toml
  tonic-build = { version = "0.12", optional = true }
  ```

  Add to `[features]`:

  ```toml
  sidecar = [
      "dep:axum", "dep:tokio", "dep:tonic", "dep:prost",
      "dep:tonic-build",
  ]
  ```

  Add `[[bin]]` section (after the existing `[lib]` section):

  ```toml
  [[bin]]
  name = "prro_sidecar"
  path = "src/bin/prro_sidecar.rs"
  required-features = ["sidecar"]
  ```

  Also remove `serde` and `serde_json` from `[dev-dependencies]` since they are now regular deps.

- [ ] **Step 2: Create build.rs**

  Create `rust/prro_crypto/build.rs`:

  ```rust
  fn main() {
      // Only compile proto when the sidecar feature is requested.
      // CARGO_FEATURE_SIDECAR is set by Cargo whenever --features sidecar
      // (or a dependent feature) is active.
      if std::env::var("CARGO_FEATURE_SIDECAR").is_ok() {
          tonic_build::compile_protos("proto/check.proto")
              .expect("tonic_build::compile_protos failed for proto/check.proto");
      }
  }
  ```

- [ ] **Step 3: Create proto/check.proto**

  Create `rust/prro_crypto/proto/check.proto`:

  ```protobuf
  syntax = "proto3";
  package com.programika.rro.ws.chk;

  // Reconstructed from WebCheck PRRO32 TaxGrpc.dll decompile (2026-04-17).
  // Field numbers from WriteTo/MergeFrom in
  // docs/webcheck_reverse/TaxGrpc/Com.Programika.Rro.Ws.Chk/

  enum CheckType {
    UNKNOWN    = 0;
    CHK        = 1;   // receipt: SELL, RETURN, SERVICE_IN/OUT, CASH_WITHDRAWAL
    ZREPORT    = 2;   // Z-report (close-of-day)
    SERVICECHK = 3;   // service check: SHIFT_OPEN, mode transitions
  }

  message Check {
    string    rro_fn       = 1;  // fiscal number of the RRO
    int64     date_time    = 2;  // epoch seconds, Kyiv local interpreted as UTC (see DpsFiscalServerTransport._kyiv_local_epoch)
    bytes     check_sign   = 3;  // CMS DER bytes; signed content = cp1251-encoded XML
    int32     local_number = 4;  // local document number (lnd); 0 for SHIFT_OPEN
    CheckType check_type   = 5;
    string    id_offline   = 6;  // pre-allocated offline fiscal number (or "")
    string    id_cancel    = 7;  // fiscal ID of receipt being cancelled (or "")
  }

  message CheckRequest {
    bytes rro_fn_sign = 1;  // signed FN bytes (for lastChk / statusRro)
  }

  message CheckRequestId {
    string id = 1;
  }

  enum ResponseStatus {
    UNKNOWN_STATUS              =   0;
    OK                          =   1;
    ERROR_VEREFY                =  -1;
    ERROR_CHECK                 =  -2;
    ERROR_SAVE                  =  -3;
    ERROR_UNKNOWN               =  -4;
    ERROR_TYPE                  =  -5;
    ERROR_NOT_PREV_ZREPORT      =  -6;
    ERROR_XML                   =  -7;
    ERROR_XML_DATE              =  -8;
    ERROR_XML_CHK               =  -9;
    ERROR_XML_ZREPORT           = -10;
    ERROR_OFFLINE_168           = -11;
    ERROR_BAD_HASH_PREV         = -12;
    ERROR_NOT_REGISTERED_RRO    = -13;
    ERROR_NOT_REGISTERED_SIGNER = -14;
    ERROR_NOT_OPEN_SHIFT        = -15;
    ERROR_OFFLINE_ID            = -16;
  }

  message CheckResponse {
    string         id            = 1;  // server-assigned fiscal ID
    ResponseStatus status        = 2;
    bytes          id_sign       = 3;  // server signature of ID
    bytes          data_sign     = 4;  // server signature of data
    string         error_message = 5;
  }

  message StatusResponse {
    ResponseStatus status        = 1;
    string         error_message = 2;
  }

  message RroInfoResponse {
    ResponseStatus status        = 1;
    string         error_message = 2;
  }

  service ChkIncomeService {
    rpc sendChk        (Check)          returns (CheckResponse);
    rpc sendChkV2      (Check)          returns (CheckResponse);
    rpc ping           (Check)          returns (CheckResponse);
    rpc lastChk        (CheckRequest)   returns (CheckResponse);
    rpc delLastChk     (CheckRequest)   returns (CheckResponse);
    rpc delLastChkId   (CheckRequestId) returns (CheckResponse);
    rpc statusRro      (CheckRequest)   returns (StatusResponse);
    rpc infoRro        (CheckRequest)   returns (RroInfoResponse);
  }
  ```

- [ ] **Step 4: Create src/fiscal/mod.rs stub**

  Create `rust/prro_crypto/src/fiscal/mod.rs`:

  ```rust
  //! Fiscal protocol driver — cp1251 XML encoding, gRPC transport.
  //!
  //! All items in this module require the `sidecar` cargo feature.
  
  #[cfg(feature = "sidecar")]
  pub mod grpc_client;
  ```

- [ ] **Step 5: Create src/bin/prro_sidecar.rs stub**

  Create `rust/prro_crypto/src/bin/prro_sidecar.rs`:

  ```rust
  //! prro_sidecar — Rust fiscal protocol driver.
  //!
  //! HTTP sidecar that handles: cp1251 encoding → CMS/GOST signing → gRPC sendChkV2.
  //! Replaces the Node.js signing sidecar + Python gRPC transport with a single
  //! persistent-channel Rust process.
  
  fn main() {
      println!("prro_sidecar: stub — implementation in Task 2");
  }
  ```

- [ ] **Step 6: Update src/lib.rs**

  Add after the existing `pub mod core;` / `pub mod cms;` block in `rust/prro_crypto/src/lib.rs`:

  ```rust
  #[cfg(feature = "sidecar")]
  pub mod fiscal;
  ```

- [ ] **Step 7: Verify compilation**

  Run:
  ```bash
  cd rust/prro_crypto
  cargo check
  cargo check --features sidecar
  ```

  Expected: both exit 0, no errors. Warnings about unused imports in the stub are acceptable.

- [ ] **Step 8: Commit**

  ```bash
  cd /mnt/d/prro_gate
  git add rust/prro_crypto/Cargo.toml rust/prro_crypto/Cargo.lock \
          rust/prro_crypto/build.rs \
          rust/prro_crypto/proto/check.proto \
          rust/prro_crypto/src/fiscal/mod.rs \
          rust/prro_crypto/src/bin/prro_sidecar.rs \
          rust/prro_crypto/src/lib.rs
  git commit -m "feat(sidecar): cargo feature gate, proto/check.proto, fiscal module skeleton"
  ```

---

### Task 1: `LoadedKey` public API + gRPC client

**Goal:** Expose a public `load_signing_key()` helper from `interop::prro` and implement a persistent-channel tonic gRPC client in `fiscal::grpc_client`.

**Files:**
- Modify: `rust/prro_crypto/src/interop/prro/mod.rs`
- Create: `rust/prro_crypto/src/fiscal/grpc_client.rs`

**Acceptance Criteria:**
- [ ] `load_signing_key(data, password)` is publicly callable, returns `LoadedKey { signer, cert_der }`
- [ ] `DpsGrpcClient::new(endpoint, tls_root_certs)` creates a tonic channel with keep-alive configured
- [ ] `DpsGrpcClient::send_chk_v2(check)` returns `CheckResponse`
- [ ] `DpsGrpcClient::ping()` sends a ping `Check` with `check_type = UNKNOWN`
- [ ] `cargo build --features sidecar` exits 0

**Verify:** `cd rust/prro_crypto && cargo build --features sidecar` → exit 0

**Steps:**

- [ ] **Step 1: Add LoadedKey to interop/prro/mod.rs**

  Read `rust/prro_crypto/src/interop/prro/mod.rs` first. Then add after the existing imports:

  ```rust
  use crate::cms::signer::DstuInProcessSigner;
  use crate::core::curve::Curve;
  use crate::core::field::FieldEl;
  
  /// Key material ready for CMS signing: an in-process signer and the
  /// signing certificate DER bytes to embed in the CMS SignedData.
  pub struct LoadedKey {
      pub signer:   DstuInProcessSigner,
      /// Signing certificate, DER-encoded. First cert from the container.
      pub cert_der: Vec<u8>,
  }
  
  /// Load a key container (JKS / PFX / ZS2 / Key-6.dat) and return a
  /// ready-to-use signer. The first certificate in the container is used
  /// as the signing cert embedded in the CMS SignedData.
  ///
  /// # Errors
  /// `ContainerError::UnknownFormat` if the bytes don't match any known
  /// container magic. Other variants for parse / password failures.
  pub fn load_signing_key(data: &[u8], password: &str) -> Result<LoadedKey, ContainerError> {
      let extracted = extract_private_key(data, password)?;
      let curve = Curve::dstu_pb_257();
      // param_d is 32 LE bytes — convert to FieldEl via hex round-trip
      // (matches the same path used in python.rs extract_private_key binding).
      let hex: String = extracted.param_d.iter().rev()
          .map(|b| format!("{:02x}", b))
          .collect();
      let d = FieldEl::try_from_hex(&hex, curve.mod_words)
          .map_err(|e| ContainerError::Der(format!("param_d hex: {}", e)))?;
      let signer = DstuInProcessSigner::new(d);
      let cert_der = extracted.certs.into_iter().next()
          .ok_or_else(|| ContainerError::Der("container has no certificates".into()))?;
      Ok(LoadedKey { signer, cert_der })
  }
  ```

  Also add `pub use containers::ContainerError;` to the re-exports at the top of the file if not already present, and make sure `extract_private_key` is accessible from this module (it's already in `containers.rs` which is pub within the module).

- [ ] **Step 2: Create src/fiscal/grpc_client.rs**

  Create `rust/prro_crypto/src/fiscal/grpc_client.rs`:

  ```rust
  //! Persistent gRPC channel to the DPS fiscal server (prro.tax.gov.ua:443).
  //!
  //! Key behaviour: one TCP connection + TLS session is shared across ALL
  //! checks sent during the sidecar process lifetime. This eliminates the
  //! ~100ms TLS handshake that WebCheck pays on every receipt (due to its
  //! channel.ShutdownAsync() call in FillResult).
  
  use std::time::Duration;
  use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
  
  // Include proto-generated code. OUT_DIR is set by tonic_build in build.rs.
  pub mod proto {
      tonic::include_proto!("com.programika.rro.ws.chk");
  }
  
  use proto::{
      chk_income_service_client::ChkIncomeServiceClient,
      Check, CheckResponse,
  };
  
  #[derive(Debug, thiserror::Error)]
  pub enum GrpcError {
      #[error("tonic transport: {0}")]
      Transport(#[from] tonic::transport::Error),
      #[error("tonic status: {0}")]
      Status(#[from] tonic::Status),
  }
  
  /// Single shared gRPC client. Clone cheaply — the underlying channel is
  /// Arc-wrapped by tonic and does HTTP/2 stream multiplexing internally.
  #[derive(Clone)]
  pub struct DpsGrpcClient {
      inner: ChkIncomeServiceClient<Channel>,
  }
  
  impl DpsGrpcClient {
      /// Build a persistent, TLS-enabled channel and return a ready client.
      ///
      /// `endpoint`        — `"https://prro.tax.gov.ua:443"` or test env.
      /// `tls_root_certs`  — Optional PEM bytes for a custom CA bundle. When
      ///                     `None`, the system root store is used (via
      ///                     `tls-roots` feature of tonic).
      pub async fn new(
          endpoint: &str,
          tls_root_certs: Option<&[u8]>,
      ) -> Result<Self, GrpcError> {
          let mut tls = ClientTlsConfig::new();
          if let Some(pem) = tls_root_certs {
              let cert = tonic::transport::Certificate::from_pem(pem);
              tls = tls.ca_certificate(cert);
          }
  
          let channel = Endpoint::from_shared(endpoint.to_owned())
              .map_err(tonic::transport::Error::from)?
              .tls_config(tls)?
              .tcp_nodelay(true)                           // disable Nagle
              .keep_alive_interval(Duration::from_secs(30))
              .keep_alive_timeout(Duration::from_secs(10))
              .keep_alive_while_idle(true)
              .connect_timeout(Duration::from_secs(10))
              .timeout(Duration::from_secs(90))
              .connect()
              .await?;
  
          Ok(Self {
              inner: ChkIncomeServiceClient::new(channel),
          })
      }
  
      /// Send a fiscal check via sendChkV2.
      pub async fn send_chk_v2(&mut self, check: Check) -> Result<CheckResponse, GrpcError> {
          let resp = self.inner.send_chk_v2(check).await?;
          Ok(resp.into_inner())
      }
  
      /// Warm up the TLS session so the first real check doesn't pay
      /// the cold-start cost. Uses the `ping` RPC with a zero-valued Check.
      /// Errors are logged but not propagated — a ping failure must not
      /// block startup.
      pub async fn ping_warmup(&mut self) {
          let _ = self.inner.ping(Check::default()).await;
      }
  }
  ```

- [ ] **Step 3: Verify**

  ```bash
  cd rust/prro_crypto && cargo build --features sidecar
  ```

  Expected: exit 0. Proto-generated code appears under `target/*/build/prro_crypto-*/out/`.

- [ ] **Step 4: Commit**

  ```bash
  cd /mnt/d/prro_gate
  git add rust/prro_crypto/src/interop/prro/mod.rs \
          rust/prro_crypto/src/fiscal/grpc_client.rs \
          rust/prro_crypto/Cargo.lock
  git commit -m "feat(sidecar): LoadedKey API + persistent DpsGrpcClient"
  ```

---

### Task 2: Sidecar binary — axum server + sign_and_send handler

**Goal:** Implement the full `prro_sidecar` binary: load key at startup, warm up gRPC, serve `POST /fiscal/sign_and_send` (cp1251 encode → CMS sign → gRPC) and `GET /health`.

**Files:**
- Replace: `rust/prro_crypto/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `cargo build --release --features sidecar` produces `target/release/prro_sidecar`
- [ ] `GET /health` returns `200 OK`
- [ ] `POST /fiscal/sign_and_send` with valid XML UTF-8 → calls gRPC (verified in integration test)
- [ ] cp1251 encoding applied: `encoding_rs::WINDOWS_1251.encode()` used on the XML string
- [ ] CMS sign applied: `CmsSigner::sign_detached()` called on cp1251 bytes
- [ ] `check_sign` in gRPC Check = CMS DER bytes (not raw XML)
- [ ] Startup: key loaded from `SIDECAR_KEY_PATH` + `SIDECAR_KEY_PASSWORD`, ping warmup run

**Verify:** `cd rust/prro_crypto && cargo build --release --features sidecar && ls target/release/prro_sidecar`

**Steps:**

- [ ] **Step 1: Define request/response types**

  At the top of `src/bin/prro_sidecar.rs`, add the JSON model:

  ```rust
  use serde::{Deserialize, Serialize};
  
  /// POST /fiscal/sign_and_send — request body
  #[derive(Debug, Deserialize)]
  struct SignAndSendRequest {
      /// XML document as UTF-8 bytes, base64-encoded.
      /// Python side: base64.b64encode(xml_string.encode('utf-8'))
      xml_b64: String,
      /// Fiscal number of the RRO.
      fiscal_number: String,
      /// date_time: epoch seconds (Kyiv local interpreted as UTC).
      /// Computed by Python's _kyiv_local_epoch(business_ts).
      date_time: i64,
      /// Local document number. 0 for SHIFT_OPEN.
      local_number: i32,
      /// Proto enum value: 1=CHK, 2=ZREPORT, 3=SERVICECHK
      check_type: i32,
      /// Pre-allocated offline fiscal number ("" for online checks).
      #[serde(default)]
      offline_id: String,
      /// Fiscal ID of the receipt being cancelled ("" normally).
      #[serde(default)]
      cancel_id: String,
  }
  
  /// POST /fiscal/sign_and_send — response body
  #[derive(Debug, Serialize)]
  struct SignAndSendResponse {
      fiscal_id:     String,
      status:        i32,
      id_sign_b64:   String,
      data_sign_b64: String,
      error_message: String,
  }
  ```

- [ ] **Step 2: Implement the handler**

  ```rust
  use std::sync::Arc;
  use axum::{extract::State, http::StatusCode, response::Json, routing::{get, post}, Router};
  use base64::{engine::general_purpose::STANDARD as B64, Engine};
  use encoding_rs::WINDOWS_1251;
  use prro_crypto::{
      cms::{CmsSigner, CmsProfile},
      interop::prro::load_signing_key,
  };
  use tokio::sync::Mutex;
  
  use crate::{
      fiscal::grpc_client::{DpsGrpcClient, proto::Check},
      SignAndSendRequest, SignAndSendResponse,
  };
  
  struct AppState {
      grpc:     Mutex<DpsGrpcClient>,
      cert_der: Vec<u8>,
      // DstuInProcessSigner is Send+Sync
      signer:   prro_crypto::cms::signer::DstuInProcessSigner,
  }
  
  async fn health_handler() -> StatusCode {
      StatusCode::OK
  }
  
  async fn sign_and_send_handler(
      State(state): State<Arc<AppState>>,
      Json(req): Json<SignAndSendRequest>,
  ) -> Result<Json<SignAndSendResponse>, (StatusCode, String)> {
      // 1. Decode XML bytes from base64
      let xml_utf8_bytes = B64.decode(&req.xml_b64)
          .map_err(|e| (StatusCode::BAD_REQUEST, format!("base64 decode: {}", e)))?;
      let xml_string = String::from_utf8(xml_utf8_bytes)
          .map_err(|e| (StatusCode::BAD_REQUEST, format!("UTF-8 decode: {}", e)))?;
  
      // 2. Encode to cp1251 (Windows-1251)
      // Characters not representable in cp1251 are replaced with '?'.
      // Ukrainian fiscal XML is always cp1251-safe.
      let (cp1251_bytes, _, _) = WINDOWS_1251.encode(&xml_string);
      let cp1251_bytes: Vec<u8> = cp1251_bytes.into_owned();
  
      // 3. CMS sign the cp1251 bytes (GOST 34.311 + DSTU 4145)
      let cms_signer = CmsSigner {
          cert_der: &state.cert_der,
          signer:   &state.signer,
          profile:  CmsProfile::default(),
      };
      let sig = cms_signer.sign_detached(&cp1251_bytes)
          .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("CMS sign: {}", e)))?;
  
      // 4. Build gRPC Check message
      let check = Check {
          rro_fn:       req.fiscal_number,
          date_time:    req.date_time,
          check_sign:   sig.cms_der,   // CMS DER bytes, not raw XML
          local_number: req.local_number,
          check_type:   req.check_type,
          id_offline:   req.offline_id,
          id_cancel:    req.cancel_id,
      };
  
      // 5. gRPC sendChkV2 via persistent channel
      let response = state.grpc.lock().await
          .send_chk_v2(check).await
          .map_err(|e| (StatusCode::BAD_GATEWAY, format!("gRPC: {}", e)))?;
  
      Ok(Json(SignAndSendResponse {
          fiscal_id:     response.id,
          status:        response.status,
          id_sign_b64:   B64.encode(response.id_sign),
          data_sign_b64: B64.encode(response.data_sign),
          error_message: response.error_message,
      }))
  }
  ```

- [ ] **Step 3: Implement main()**

  ```rust
  #[tokio::main]
  async fn main() {
      // Key loading
      let key_path = std::env::var("SIDECAR_KEY_PATH")
          .expect("SIDECAR_KEY_PATH required");
      let key_password = std::env::var("SIDECAR_KEY_PASSWORD")
          .unwrap_or_default();
      let key_bytes = std::fs::read(&key_path)
          .unwrap_or_else(|e| panic!("cannot read key {}: {}", key_path, e));
      let loaded = load_signing_key(&key_bytes, &key_password)
          .unwrap_or_else(|e| panic!("key load failed: {}", e));
  
      // gRPC channel
      let dps_endpoint = std::env::var("DPS_ENDPOINT")
          .unwrap_or_else(|_| "https://prro.tax.gov.ua:443".into());
      let tls_ca_path = std::env::var("DPS_TLS_CA_CERT").ok();
      let tls_root_certs: Option<Vec<u8>> = tls_ca_path.as_deref()
          .map(|p| std::fs::read(p).unwrap_or_else(|e| panic!("TLS CA {}: {}", p, e)));
  
      let mut grpc = DpsGrpcClient::new(&dps_endpoint, tls_root_certs.as_deref())
          .await
          .unwrap_or_else(|e| panic!("gRPC channel init: {}", e));
  
      // Warmup: establishes TLS session so first real check is fast
      grpc.ping_warmup().await;
  
      let state = Arc::new(AppState {
          grpc:     Mutex::new(grpc),
          cert_der: loaded.cert_der,
          signer:   loaded.signer,
      });
  
      let bind_addr = std::env::var("SIDECAR_BIND")
          .unwrap_or_else(|_| "127.0.0.1:8090".into());
      let listener = tokio::net::TcpListener::bind(&bind_addr).await
          .unwrap_or_else(|e| panic!("bind {}: {}", bind_addr, e));
  
      let app = Router::new()
          .route("/fiscal/sign_and_send", post(sign_and_send_handler))
          .route("/health", get(health_handler))
          .with_state(state);
  
      println!("prro_sidecar listening on {}", bind_addr);
      axum::serve(listener, app).await.expect("axum serve failed");
  }
  ```

  Add all necessary use statements and `use prro_crypto::fiscal::grpc_client` at the top of the file. The full file should compile with no `use` errors.

- [ ] **Step 4: Add `base64` dependency**

  Add to `[dependencies]` in Cargo.toml (under `sidecar` optional group):
  ```toml
  base64 = { version = "0.22", optional = true }
  ```

  Add `"dep:base64"` to the `sidecar` feature list.

- [ ] **Step 5: Verify build**

  ```bash
  cd rust/prro_crypto && cargo build --release --features sidecar
  ls -lh target/release/prro_sidecar
  ```

  Expected: binary ~5-15MB, exit 0.

- [ ] **Step 6: Commit**

  ```bash
  cd /mnt/d/prro_gate
  git add rust/prro_crypto/src/bin/prro_sidecar.rs rust/prro_crypto/Cargo.toml rust/prro_crypto/Cargo.lock
  git commit -m "feat(sidecar): prro_sidecar binary — sign_and_send handler, persistent gRPC"
  ```

---

### Task 3: Python `FiscalSidecarTransport` + runtime wiring

**Goal:** Add `FiscalSidecarTransport` to the Python transport layer and wire it into the container so YAML config can select `transport_kind: DPS_PRRO_FISCAL_SIDECAR`.

**Files:**
- Modify: `src/prro_gateway/enums.py` (line ~141)
- Create: `src/prro_gateway/transports/fiscal_sidecar.py`
- Modify: `src/prro_gateway/transports/__init__.py`
- Modify: `src/prro_gateway/runtime/container.py`

**Acceptance Criteria:**
- [ ] `TransportKind.DPS_PRRO_FISCAL_SIDECAR` exists in enums
- [ ] `FiscalSidecarTransport.send()` posts `{xml_b64, fiscal_number, date_time, local_number, check_type, offline_id, cancel_id}` to `{sidecar_url}/fiscal/sign_and_send`
- [ ] On `status == 1` (OK): returns `SendResult(state=ACK, transport_request_id=fiscal_id)`
- [ ] On `status < 0`: raises `TransportRejectedError` with `error_message`
- [ ] `poll_status()` raises `NotImplementedError` (sidecar transport is immediate/synchronous)
- [ ] `pytest tests/` passes (no regressions)
- [ ] `pytest tests/test_transports.py -k fiscal_sidecar` passes if test file exists

**Verify:** `pytest tests/ -x -q` → all pass

**Steps:**

- [ ] **Step 1: Add DPS_PRRO_FISCAL_SIDECAR to TransportKind**

  In `src/prro_gateway/enums.py`, find the `TransportKind` class (line ~137) and add:

  ```python
  class TransportKind(StrEnum):
      CHECKBOX_REST_TRANSPORT    = "CHECKBOX_REST_TRANSPORT"
      DPS_PRRO_GRPC_ECABINET     = "DPS_PRRO_GRPC_ECABINET"
      DPS_PRRO_XML_UNIFIED_WINDOW = "DPS_PRRO_XML_UNIFIED_WINDOW"
      CUSTOM_TRANSPORT           = "CUSTOM_TRANSPORT"
      DPS_PRRO_FISCAL_SIDECAR    = "DPS_PRRO_FISCAL_SIDECAR"  # Rust sidecar
  ```

- [ ] **Step 2: Write a failing test first**

  Create `tests/test_fiscal_sidecar_transport.py`:

  ```python
  """Tests for FiscalSidecarTransport."""
  from __future__ import annotations

  import base64
  import json
  from unittest.mock import MagicMock, patch

  import pytest
  import httpx

  from prro_gateway.transports.fiscal_sidecar import FiscalSidecarTransport
  from prro_gateway.ports import SendResult
  from prro_gateway.enums import DocumentState


  def _make_transport(sidecar_response: dict) -> FiscalSidecarTransport:
      """Build transport with mocked httpx."""
      mock_resp = MagicMock()
      mock_resp.status_code = 200
      mock_resp.json.return_value = sidecar_response
      mock_client = MagicMock(spec=httpx.Client)
      mock_client.post.return_value = mock_resp
      return FiscalSidecarTransport(sidecar_url="http://localhost:8090", http_client=mock_client)


  def test_send_ok_returns_ack():
      t = _make_transport({
          "fiscal_id": "FIS123",
          "status": 1,
          "id_sign_b64": "",
          "data_sign_b64": "",
          "error_message": "",
      })
      xml_bytes = b"<RQ V='1'></RQ>"
      result = t.send(
          document_id="doc-1",
          signed_payload=xml_bytes,
          fiscal_number="3001234567",
          backend_profile_id="bp1",
          transport_profile_id="tp1",
          operation_type="SELL",
          lnd=42,
          business_ts=None,
          related_receipt_id=None,
          offline_fiscal_no=None,
      )
      assert result.state == DocumentState.ACK.value
      assert result.transport_request_id == "FIS123"


  def test_send_dps_error_raises_rejected():
      from prro_gateway.ports import TransportRejectedError
      t = _make_transport({
          "fiscal_id": "",
          "status": -7,
          "id_sign_b64": "",
          "data_sign_b64": "",
          "error_message": "XML_INVALID",
      })
      with pytest.raises(TransportRejectedError, match="XML_INVALID"):
          t.send(
              document_id="doc-2",
              signed_payload=b"<bad/>",
              fiscal_number="3001234567",
              backend_profile_id="bp1",
              transport_profile_id="tp1",
              operation_type="SELL",
              lnd=1,
              business_ts=None,
              related_receipt_id=None,
              offline_fiscal_no=None,
          )


  def test_request_body_has_correct_fields():
      """Verify the JSON posted to the Rust sidecar has the expected shape."""
      mock_resp = MagicMock()
      mock_resp.status_code = 200
      mock_resp.json.return_value = {
          "fiscal_id": "X", "status": 1,
          "id_sign_b64": "", "data_sign_b64": "", "error_message": "",
      }
      mock_client = MagicMock(spec=httpx.Client)
      mock_client.post.return_value = mock_resp

      t = FiscalSidecarTransport(sidecar_url="http://localhost:8090", http_client=mock_client)
      xml_bytes = "данные чека".encode("utf-8")
      t.send(
          document_id="doc-3",
          signed_payload=xml_bytes,
          fiscal_number="3001234567",
          backend_profile_id="bp1",
          transport_profile_id="tp1",
          operation_type="SELL",
          lnd=7,
          business_ts=None,
          related_receipt_id=None,
          offline_fiscal_no=None,
      )
      call_kwargs = mock_client.post.call_args
      body = call_kwargs[1]["json"]
      assert body["xml_b64"] == base64.b64encode(xml_bytes).decode()
      assert body["fiscal_number"] == "3001234567"
      assert body["local_number"] == 7
      assert body["check_type"] == 1  # SELL → CHK = 1
  ```

  Run: `pytest tests/test_fiscal_sidecar_transport.py -v`
  Expected: FAIL (ImportError — module not yet created)

- [ ] **Step 3: Implement FiscalSidecarTransport**

  Create `src/prro_gateway/transports/fiscal_sidecar.py`:

  ```python
  """
  FiscalSidecarTransport — delegates to the Rust prro_sidecar binary.

  The Rust sidecar handles: cp1251 encoding → CMS/GOST signing → gRPC sendChkV2.
  Python sends: UTF-8 XML bytes (base64) + fiscal metadata.
  Python receives: fiscal_id, status, server signatures.

  Use with PassthroughCryptoProvider so sign_raw() leaves XML bytes unchanged.
  The sidecar does the real signing internally.
  """
  from __future__ import annotations

  import base64
  import logging
  from datetime import UTC, datetime

  import httpx

  from ..enums import DocumentState, OperationType
  from ..ports import SendResult, TransportRejectedError, TransportRetryableError

  logger = logging.getLogger("prro_gateway.transports.fiscal_sidecar")

  # Proto check_type enum values (mirrored from proto/check.proto)
  _CHECK_TYPE_CHK        = 1
  _CHECK_TYPE_ZREPORT    = 2
  _CHECK_TYPE_SERVICECHK = 3

  _OP_TO_CHECK_TYPE: dict[str, int] = {
      OperationType.SHIFT_OPEN:       _CHECK_TYPE_SERVICECHK,
      OperationType.SELL:             _CHECK_TYPE_CHK,
      OperationType.RETURN:           _CHECK_TYPE_CHK,
      OperationType.Z_REPORT:         _CHECK_TYPE_ZREPORT,
      OperationType.SERVICE_IN:       _CHECK_TYPE_CHK,
      OperationType.SERVICE_OUT:      _CHECK_TYPE_CHK,
      OperationType.CASH_WITHDRAWAL:  _CHECK_TYPE_CHK,
  }

  _SUPPORTED_OPS = set(_OP_TO_CHECK_TYPE.keys())


  def _kyiv_local_epoch(utc_dt) -> int:
      """Kyiv-local wall-clock time expressed as a fake UTC epoch.

      Matches DpsFiscalServerTransport._kyiv_local_epoch — DPS requires
      date_time to match the XML <TS> timestamp (Kyiv local time).
      """
      try:
          from zoneinfo import ZoneInfo
          from datetime import timezone
          kyiv = ZoneInfo("Europe/Kyiv")
          local = utc_dt.astimezone(kyiv)
          fake = datetime(local.year, local.month, local.day,
                          local.hour, local.minute, local.second,
                          tzinfo=timezone.utc)
          return int(fake.timestamp())
      except Exception:
          return int(utc_dt.timestamp())


  class FiscalSidecarTransport:
      """Transport that delegates to the Rust prro_sidecar via HTTP.

      Args:
          sidecar_url: Base URL of the Rust sidecar, e.g. "http://127.0.0.1:8090".
          http_client: Optional pre-built httpx.Client (for testing / connection
                       pooling). If None, a default client is created on first use.
      """

      def __init__(
          self,
          *,
          sidecar_url: str = "http://127.0.0.1:8090",
          http_client: httpx.Client | None = None,
      ) -> None:
          self._sidecar_url = sidecar_url.rstrip("/")
          self._http_client = http_client
          self._owns_client = http_client is None

      def _client(self) -> httpx.Client:
          if self._http_client is None:
              self._http_client = httpx.Client(timeout=120.0)
          return self._http_client

      def send(
          self,
          *,
          document_id: str,
          signed_payload,
          fiscal_number: str,
          backend_profile_id: str,
          transport_profile_id: str,
          operation_type: str | None = None,
          request_payload: dict | None = None,
          request_payload_json: str | None = None,
          external_request_id: str | None = None,
          transport_profile=None,
          **kwargs,
      ) -> SendResult:
          op = OperationType(operation_type) if operation_type else None
          if op not in _SUPPORTED_OPS:
              raise TransportRejectedError(
                  f"FiscalSidecarTransport: unsupported operation {operation_type}"
              )

          # signed_payload = XML UTF-8 bytes (passthrough crypto provider)
          if isinstance(signed_payload, bytes):
              xml_bytes = signed_payload
          else:
              xml_bytes = signed_payload.encode("utf-8")

          check_type = _OP_TO_CHECK_TYPE.get(op, _CHECK_TYPE_CHK)
          local_number = 0 if op == OperationType.SHIFT_OPEN else int(kwargs.get("lnd", 0) or 0)
          id_cancel  = str(kwargs.get("related_receipt_id", "") or "")
          id_offline = str(kwargs.get("offline_fiscal_no", "") or "")

          now = datetime.now(UTC)
          business_ts = kwargs.get("business_ts", now)
          if business_ts is None:
              business_ts = now
          date_time = _kyiv_local_epoch(business_ts)

          body = {
              "xml_b64":       base64.b64encode(xml_bytes).decode(),
              "fiscal_number": fiscal_number,
              "date_time":     date_time,
              "local_number":  local_number,
              "check_type":    check_type,
              "offline_id":    id_offline,
              "cancel_id":     id_cancel,
          }

          try:
              resp = self._client().post(
                  f"{self._sidecar_url}/fiscal/sign_and_send",
                  json=body,
              )
              resp.raise_for_status()
          except httpx.HTTPStatusError as exc:
              raise TransportRetryableError(
                  f"sidecar HTTP {exc.response.status_code}: {exc.response.text}"
              ) from exc
          except httpx.RequestError as exc:
              raise TransportRetryableError(f"sidecar unreachable: {exc}") from exc

          data = resp.json()
          status = data.get("status", 0)
          server_id = data.get("fiscal_id", "")
          error_msg = data.get("error_message", "")

          if status == 1:  # OK
              return SendResult(
                  state=DocumentState.ACK.value,
                  transport_request_id=server_id,
                  submission_status="DPS_ACK",
                  server_response=data,
              )

          if status < 0:  # DPS rejection
              raise TransportRejectedError(
                  f"DPS rejected (status={status}): {error_msg}"
              )

          # status=0 or unexpected positive value — treat as retryable
          raise TransportRetryableError(
              f"DPS unexpected status {status}: {error_msg}"
          )

      def poll_status(self, **kwargs):
          raise NotImplementedError(
              "FiscalSidecarTransport is synchronous — no poll_status needed"
          )

      def close(self) -> None:
          if self._owns_client and self._http_client is not None:
              self._http_client.close()

      def __del__(self):
          self.close()
  ```

- [ ] **Step 4: Run the test (should pass now)**

  ```bash
  cd /mnt/d/prro_gate
  pytest tests/test_fiscal_sidecar_transport.py -v
  ```

  Expected: 3 tests PASS.

- [ ] **Step 5: Export from transports/__init__.py**

  Read `src/prro_gateway/transports/__init__.py`. Add to the exports:

  ```python
  from .fiscal_sidecar import FiscalSidecarTransport
  ```

  Also add `FiscalSidecarTransport` to `__all__` if it exists.

- [ ] **Step 6: Wire in container.py**

  Read `src/prro_gateway/runtime/container.py`. Find where `DpsFiscalServerTransport` is registered in the `handlers` dict (search for `TransportKind.DPS_PRRO_GRPC_ECABINET` or similar).

  Import `FiscalSidecarTransport` at the top:
  ```python
  from ..transports.fiscal_sidecar import FiscalSidecarTransport
  ```

  In the method that builds the `handlers` dict (likely `_build_transport_handlers()` or similar), add:
  ```python
  TransportKind.DPS_PRRO_FISCAL_SIDECAR: FiscalSidecarTransport(
      sidecar_url=getattr(config.crypto, 'sidecar_url', 'http://127.0.0.1:8090'),
      http_client=self.transport_http_client,
  ),
  ```

  Read the exact method signature and wiring pattern in `container.py` before editing.

- [ ] **Step 7: Run full test suite**

  ```bash
  cd /mnt/d/prro_gate && pytest tests/ -x -q
  ```

  Expected: all pass, no regressions. Note any new failures and fix them before committing.

- [ ] **Step 8: Commit**

  ```bash
  git add src/prro_gateway/enums.py \
          src/prro_gateway/transports/fiscal_sidecar.py \
          src/prro_gateway/transports/__init__.py \
          src/prro_gateway/runtime/container.py \
          tests/test_fiscal_sidecar_transport.py
  git commit -m "feat(transport): FiscalSidecarTransport + DPS_PRRO_FISCAL_SIDECAR kind"
  ```

---

### Task 4 (Phase 2 — separate sprint): XML builder in Rust

**Goal:** Move `dps_xml.py` logic into `src/fiscal/xml_builder.rs` so the sidecar can accept canonical JSON at `/fiscal/canonical_send` and build + sign + send without Python XML generation.

**Status:** DEFERRED. This task is not part of Phase 1 delivery. Phase 1 (`/fiscal/sign_and_send`) must be stable in production before Phase 2 begins.

**Prerequisite:** Phase 1 (`/fiscal/sign_and_send`) passes end-to-end tests with at least one real fiscal number.

**Scope when activated:**
- `src/fiscal/types.rs` — Rust structs mirroring canonical JSON (all 7 operation types)
- `src/fiscal/xml_builder.rs` — `build_dps_xml_utf8(params)` → cp1251 bytes, replicating all logic from `serializers/dps_xml.py` including `_calc_tax()`, `_build_e_element()`, Z-report structure
- Golden-file tests: run Python dps_xml.py on test vectors, save as hex, verify Rust produces identical cp1251 output
- New sidecar endpoint: `POST /fiscal/canonical_send` replaces `/fiscal/sign_and_send`
- Python side: FiscalSidecarTransport posts canonical JSON instead of XML bytes

---

## Deployment

```bash
# Build
cargo build --release --features sidecar -p prro_crypto
cp target/release/prro_sidecar /opt/prro_sidecar

# Run
SIDECAR_KEY_PATH=/keys/signing.jks \
SIDECAR_KEY_PASSWORD=secret \
DPS_ENDPOINT=https://prro.tax.gov.ua:443 \
SIDECAR_BIND=127.0.0.1:8090 \
  /opt/prro_sidecar

# Config (config.yaml):
# transports:
#   - transport_profile_id: dps_sidecar
#     kind: DPS_PRRO_FISCAL_SIDECAR
#     endpoint: http://127.0.0.1:8090
# crypto:
#   provider: passthrough     ← required with FiscalSidecarTransport
#   sidecar_url: http://127.0.0.1:8090
```

## Performance improvement summary

| Metric | WebCheck | Before (Python+Node.js) | After (Rust sidecar) |
|--------|---------|------------------------|----------------------|
| TLS handshakes / check | 1 (bug) | 0 (persistent per restart) | 0 (persistent + warmup) |
| Network round-trips / check | 2 (sign + send) | 2 (sign + send) | 1 (sidecar does both) |
| cp1251 encoding | Correct (WebCheck) | Bug (UTF-8 sent as cp1251) | Correct (encoding_rs) |
| Cold start latency | ~100ms/check | ~100ms/check | ~50ms/check (2nd+) |
