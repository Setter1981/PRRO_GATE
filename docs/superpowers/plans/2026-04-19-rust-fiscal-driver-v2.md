# ADR-004 v2: Rust Fiscal Driver — Full Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python gRPC transport (`transports/dps_fiscal_server.py`) and the Node.js `jkurwa` signing sidecar with a **single Rust binary** `prro_sidecar` that owns: canonical JSON → cp1251 XML build → CMS sign (optionally RFC 3161 TSP) → gRPC `sendChkV2` to DPS. The sidecar also enforces a **detached-signature license** gated on (TIN, FN set, expires_at), publishes a minimal admin HTTP surface, and ships CLI onboarding tools.

**Why now:**
- The Python `dps_xml.py` + gRPC split costs 2 network round-trips per receipt and is the heaviest code path in the gateway (~570 lines of XML).
- WebCheck (`docs/webcheck_reverse/TaxGrpc/TaxGrpc/Client.cs`) calls `channel.ShutdownAsync()` after every receipt → ~100 ms TLS handshake per check. We already ship a persistent-channel Python client (`0a49b22`), but keeping XML build and signing in two processes blocks commercialization.
- The signing sidecar currently runs Node.js (`jkurwa`) — license enforcement and audit have no natural home there. Moving to Rust gives us `include_bytes!`-embedded license master pubkey, crash-only single binary ops, and shared codebase with `prro_crypto`.

**Architecture (finalized 2026-04-19, user-approved):**

```
Python gateway (canonical model is source of truth)
  └─ write_path.py
       └─ HTTP POST /fiscal/send  →  Rust prro_sidecar
                                       │
                                       ├─ 1. serde: parse canonical JSON (minimal fields)
                                       ├─ 2. SQLite read: fn_config + active operator + license row
                                       ├─ 3. License check (tin match, FN in set, expires_at ± 14d grace, demo caps)
                                       ├─ 4. XML build (cp1251 bytes, byte-identical to dps_xml.py)
                                       ├─ 5. CMS sign — DSTU 4145-2002 + GOST 34.311 (optional RFC 3161 TSP)
                                       ├─ 6. tonic sendChkV2 (prod or test channel per fn.fiscal_mode)
                                       └─ 7. JSON response {status, fiscal_id, payload_sha256, signatures, warnings}
```

**Tech stack (Rust side):** axum 0.7, tonic 0.12 + tls-roots, prost 0.13, tokio (full), serde + serde_json, encoding_rs, reqwest (TSP POST), rusqlite (bundled), aes / sha2 / hmac / hex / base64. Existing `prro_crypto` core is reused — container readers (`interop::prro`), CMS signer (`cms::signer::DstuInProcessSigner`, `cms::CmsSigner`), TSP client.

**Frozen invariants touched:**
- **(1) No network / crypto calls inside long SQLite write transactions.** Rust repo module MUST keep every query short (single SELECT / INSERT) and never open a transaction that spans gRPC or TSP calls.
- **(2) One `fiscal_number` = one logical single-writer write-path.** Sidecar is stateless across FNs; parallelism is enforced upstream by Python `write_path`. Rust does not add its own per-FN locking.
- **(6) Adapters must build full canonical payloads, not summary-only payloads.** Python still emits full canonical; Rust only reads subset fields needed for XML build and license check. We do not drop fields — we ignore unknown ones.
- **(7) All canonical envelopes must carry `schema_version`.** Input schema check: Rust rejects payloads missing `schema_version`.
- **(10) Local signing may be bypassed only by explicit profile/config behavior.** Passthrough crypto stays a Python concern; the Rust sidecar always signs. A config flag `dev.skip_sign = true` (dev only, guarded by `DEBUG_INSECURE_MODE=1` env) is explicit.

**Non-goals of this plan:**
- Unified Window (cp1251 XML HTTP) — reserved for Phase 8, separate plan.
- Offline UUID management, shift state machines, reconciliation — stay in Python.
- Printer drivers, X-report rendering, PDF export — out of scope.
- HSM / remote signer — out of scope; `DstuInProcessSigner` only.

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `rust/prro_crypto/Cargo.toml` | Modify | Sidecar feature gate, new deps, 6 `[[bin]]` targets |
| `rust/prro_crypto/build.rs` | Modify | `tonic_build` gated on `CARGO_FEATURE_SIDECAR` |
| `rust/prro_crypto/proto/check.proto` | Create | DPS protobuf (7 RPC methods, all 17 status codes) |
| `rust/prro_crypto/src/fiscal/mod.rs` | Create | Module root (cfg sidecar) |
| `rust/prro_crypto/src/fiscal/input.rs` | Create | serde structs matching Python canonical JSON subset |
| `rust/prro_crypto/src/fiscal/xml_builder.rs` | Create | Byte-identical port of `dps_xml.py` |
| `rust/prro_crypto/src/fiscal/cp1251.rs` | Create | Helpers for cp1251 encoding / XML escape |
| `rust/prro_crypto/src/fiscal/license.rs` | Create | License payload, JCS canonicalization, verify, tier policy |
| `rust/prro_crypto/src/fiscal/grpc_client.rs` | Create | Per-mode persistent tonic channel pool + 7 RPC wrappers |
| `rust/prro_crypto/src/fiscal/cms_adapter.rs` | Create | Thin adapter over `cms::CmsSigner::{sign_with, sign_with_tst}` |
| `rust/prro_crypto/src/fiscal/config.rs` | Create | TOML parser for sidecar.toml |
| `rust/prro_crypto/src/fiscal/repo.rs` | Create | rusqlite queries (fn_config, sidecar_operators, licenses, operator_certs) |
| `rust/prro_crypto/src/fiscal/errors.rs` | Create | `SidecarError` enum + DPS status classifier |
| `rust/prro_crypto/src/fiscal/license_pubkey_current.der` | Create (placeholder) | Current master verification pubkey DER, embedded via `include_bytes!` |
| `rust/prro_crypto/src/fiscal/license_pubkey_next.der` | Create (placeholder) | Next rotation slot pubkey DER; initially identical to current |
| `rust/prro_crypto/src/bin/prro_sidecar.rs` | Create | axum HTTP server binary |
| `rust/prro_crypto/src/bin/prro_admin.rs` | Create | Admin CLI (register_fn, add_operator, list_operators, (de)activate) |
| `rust/prro_crypto/src/bin/prro_license_keygen.rs` | Create | Generate master DSTU keypair |
| `rust/prro_crypto/src/bin/prro_license_sign.rs` | Create | Sign license (single / CSV batch) |
| `rust/prro_crypto/src/bin/prro_license_verify.rs` | Create | Dry-run verify license file |
| `rust/prro_crypto/src/bin/prro_sidecar_preflight.rs` | Create | Onboarding CLI: load JKS, call `infoRro`, print metadata |
| `rust/prro_crypto/ops/sidecar.example.toml` | Create | Documented example TOML config |
| `rust/prro_crypto/ops/prro-tax-gov-ua-chain.pem.placeholder` | Create | TLS chain location marker (real PEM shipped separately) |
| `rust/prro_crypto/tests/xml_golden.rs` | Create | Byte-equality tests over ~30 captured scenarios |
| `rust/prro_crypto/tests/license_roundtrip.rs` | Create | Sign / verify / mutate unit tests |
| `rust/prro_crypto/tests/grpc_contract.rs` | Create | tonic mock server — 7 RPC method contracts |
| `rust/prro_crypto/tests/sidecar_e2e.rs` | Create | Spawn binary, full SHIFT_OPEN → SELL → Z_REPORT on mock |
| `sql/017_sidecar_ops_and_fn_business.sql` | Create | fiscal_number_config extensions + sidecar_operators + licenses |
| `tests/test_gate1j_migration_idempotency.py` | Modify | Extend to cover migration 017 (if not auto-detected) |
| `scripts/dps_golden_dump.py` | Create | Dev tool: run `dps_xml.py` across fixtures → JSON file for Rust golden tests |
| `scripts/run_sidecar_dev.py` | Create | Dev runner that starts mock gRPC + the sidecar |
| `src/prro_gateway/enums.py` | Modify | `DPS_PRRO_FISCAL_SIDECAR_V2` transport kind |
| `src/prro_gateway/transports/fiscal_sidecar.py` | Create | httpx client posting canonical JSON to Rust sidecar |
| `src/prro_gateway/transports/__init__.py` | Modify | Export new transport |
| `src/prro_gateway/runtime/container.py` | Modify | Wire new transport handler |
| `src/prro_gateway/transports/dps_fiscal_server.py` | **Delete** (Phase 6.5 only) | After pilot validation |
| `src/prro_gateway/transports/proto/fiscal_server.proto` | **Delete** (Phase 6.5 only) | After pilot validation |
| `tests/test_fiscal_sidecar_v2_transport.py` | Create | httpx-mocked contract tests |
| `docs/ADR-004-rust-fiscal-driver.md` | Modify | Mark v2 superseded-with-link to this plan |
| `CHANGELOG.md` | Modify | Phase-by-phase changelog entries |
| `README.md` | Modify | Add sidecar build/run section |
| `rust/prro_crypto/README.md` | Modify | Document `sidecar` feature and 6 binaries |

---

## Phase timeline

| Phase | Days | Commits | Owner task count |
|-------|------|---------|-----|
| 0 — Scaffolding | 0.5 | 1 | 7 |
| 1 — JSON input + SQLite repo | 1.0 | 1 | 2 |
| 2 — XML builder + golden tests | 2.0 | 3 | 6 |
| 3 — License module + tools | 1.0 | 2 | 4 |
| 4 — gRPC client + CMS adapter + config | 1.0 | 1 | 3 |
| 5 — `prro_sidecar` + `prro_admin` + preflight | 2.0 | 3 | 5 |
| 6 — Python integration | 0.5 | 1 | 5 |
| 7 — Documentation + cleanup | 0.5 | 1 | 3 |
| **Total** | **8.5** | **13** | **35** |

---

## Dependency graph (task → task)

```
Phase 0 (scaffolding)  ──▶  Phase 1.1 (input.rs)  ──▶  Phase 2.1-2.5 (xml_builder)
                       └─▶  Phase 1.2 (repo.rs)   ──▶  Phase 3.1 (license.rs)  ──▶  Phase 3.2-3.4 (bins)
                       └─▶  Phase 4.1 (grpc_client)
                       └─▶  Phase 4.2 (cms_adapter)
                       └─▶  Phase 4.3 (config.rs)
Phase 2.6 (golden tests)  ──▶  Phase 5.x (sidecar wiring)
Phase 3, 4 outputs         ──▶  Phase 5.1-5.4
Phase 5.5 (E2E)            ──▶  Phase 6 (Python integration)
Phase 6 stable             ──▶  Phase 6.5 (remove old transport)  ──▶  Phase 7 (docs)
```

No task requires an artifact from a later task.

---

## Task template (applied to every task below)

Each task block uses:

- **Goal** — one sentence
- **Files** — created / modified (absolute paths or `rust/…`-relative)
- **Acceptance Criteria** — `- [ ]` checkboxes, testable without a live DPS server
- **Verify** — exact shell command (prefix `rtk proxy` for pytest)
- **Steps** — numbered `- [ ] **Step N: …**` with code / schema blocks where load-bearing
- **Commit** — proposed commit message ending with the Claude co-authoring trailer

---

# Phase 0 — Scaffolding

## Task 0.1: Cargo.toml — sidecar feature gate + deps + six `[[bin]]` targets

**Goal:** Turn `prro_crypto` into a dual-crate: library (existing) + binaries gated behind the `sidecar` feature.

**Files:**
- Modify: `rust/prro_crypto/Cargo.toml`

**Acceptance Criteria:**
- [ ] `cargo check` (no features) still passes — cdylib Python build unaffected
- [ ] `cargo check --features sidecar` passes — all 6 binaries resolve
- [ ] Each `[[bin]]` entry has `required-features = ["sidecar"]`
- [ ] `[features] sidecar = [ … ]` lists every optional dep the binaries import
- [ ] `serde`, `serde_json`, `hex` promoted from dev-deps to deps (xml_builder needs them at runtime, not only tests)

**Verify:** `cd rust/prro_crypto && cargo check && cargo check --features sidecar`

**Steps:**

- [ ] **Step 1: Add new runtime deps**

  Append to `[dependencies]`:

  ```toml
  # Promoted from dev-dependencies (needed by xml_builder / license / config)
  serde      = { version = "1.0", features = ["derive"] }
  serde_json = "1.0"
  hex        = "0.4"

  # Lightweight helpers used by sidecar modules (always compiled — tiny)
  encoding_rs = "0.8"

  # --- sidecar-only deps (not in the Python cdylib) ---
  axum        = { version = "0.7",  optional = true, features = ["json", "tokio"] }
  tokio       = { version = "1",    optional = true, features = ["full"] }
  tonic       = { version = "0.12", optional = true, features = ["tls", "tls-roots"] }
  prost       = { version = "0.13", optional = true }
  reqwest     = { version = "0.12", optional = true, default-features = false, features = ["rustls-tls", "blocking", "json"] }
  rusqlite    = { version = "0.32", optional = true, features = ["bundled", "chrono"] }
  toml        = { version = "0.8",  optional = true }
  clap        = { version = "4.5",  optional = true, features = ["derive"] }
  base64      = { version = "0.22", optional = true }
  tracing     = { version = "0.1",  optional = true }
  tracing-subscriber = { version = "0.3", optional = true, features = ["env-filter", "json"] }
  time        = { version = "0.3",  optional = true, features = ["macros", "formatting", "parsing"] }
  # Credentials storage — XOR-soft obfuscation by default (see Task 4.4).
  # Key = SHA-256(valid_to_str + operator_name[1]). No machine binding,
  # no OS services. `plain` mode available as opt-out for migration from
  # WebCheck or for debug. sha2 added for key derivation.
  sha2 = "0.10"
  ```

- [ ] **Step 2: Add build-deps**

  ```toml
  [build-dependencies]
  tonic-build = { version = "0.12", optional = true }
  ```

- [ ] **Step 3: Define feature**

  ```toml
  [features]
  default  = ["tsp_http"]
  python   = ["pyo3"]
  tsp_http = ["dep:ureq"]
  sidecar = [
      "dep:axum", "dep:tokio", "dep:tonic", "dep:prost", "dep:reqwest",
      "dep:rusqlite", "dep:toml", "dep:clap", "dep:base64",
      "dep:tracing", "dep:tracing-subscriber", "dep:time",
      "dep:tonic-build",
  ]
  # security.credentials_mode = "xor_soft" → default, XOR + SHA-256 obfuscation, no OS deps
  # security.credentials_mode = "plain"    → opt-out for migration from WebCheck / debug
  dangerous_deterministic_k_for_tests = []
  legacy_jkurwa_interop = []
  ```

- [ ] **Step 4: Add six `[[bin]]` targets**

  ```toml
  [[bin]]
  name = "prro_sidecar"
  path = "src/bin/prro_sidecar.rs"
  required-features = ["sidecar"]

  [[bin]]
  name = "prro_admin"
  path = "src/bin/prro_admin.rs"
  required-features = ["sidecar"]

  [[bin]]
  name = "prro_license_keygen"
  path = "src/bin/prro_license_keygen.rs"
  required-features = ["sidecar"]

  [[bin]]
  name = "prro_license_sign"
  path = "src/bin/prro_license_sign.rs"
  required-features = ["sidecar"]

  [[bin]]
  name = "prro_license_verify"
  path = "src/bin/prro_license_verify.rs"
  required-features = ["sidecar"]

  [[bin]]
  name = "prro_sidecar_preflight"
  path = "src/bin/prro_sidecar_preflight.rs"
  required-features = ["sidecar"]
  ```

- [ ] **Step 5: Remove duplicates from `[dev-dependencies]`**

  Delete the `serde`, `serde_json`, `hex` lines from `[dev-dependencies]` — they are now first-class deps.

- [ ] **Step 6: Run both `cargo check`s**

  ```
  cd rust/prro_crypto
  cargo check
  cargo check --features sidecar
  ```

  Expected: both exit 0. Warnings about unused stubs are acceptable until Task 0.5.

**Commit:**

```
chore(sidecar): Cargo feature gate and six bin targets

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 0.2: build.rs — tonic_build gated on sidecar feature

**Goal:** Generate protobuf stubs only when the sidecar is built; keep the library build deterministic.

**Files:**
- Modify: `rust/prro_crypto/build.rs` (may not exist yet — create)

**Acceptance Criteria:**
- [ ] `cargo check` (no feature) does NOT run `tonic_build`
- [ ] `cargo check --features sidecar` generates `com.programika.rro.ws.chk.rs` in `OUT_DIR`
- [ ] `println!("cargo:rerun-if-changed=proto/check.proto")` emitted
- [ ] Missing `proto/check.proto` panics with a clear message (caught in Task 0.3 if forgotten)

**Verify:** `cd rust/prro_crypto && cargo build --features sidecar && ls target/debug/build/prro_crypto-*/out/ | grep chk`

**Steps:**

- [ ] **Step 1: Write build.rs**

  ```rust
  fn main() {
      if std::env::var_os("CARGO_FEATURE_SIDECAR").is_some() {
          println!("cargo:rerun-if-changed=proto/check.proto");
          tonic_build::configure()
              .build_server(true)   // tests spawn in-process mock server
              .build_client(true)
              .compile(&["proto/check.proto"], &["proto/"])
              .expect("tonic_build::compile failed");
      }
  }
  ```

- [ ] **Step 2: Verify the guard**

  `cargo check` (no feature) must not require `tonic-build` — confirmed by the `optional = true` in `[build-dependencies]`.

**Commit:** rolled into Task 0.1 (same commit).

---

## Task 0.3: proto/check.proto — 7 RPC methods, 17 status codes

**Goal:** Canonical, review-ready protobuf schema reconstructed from WebCheck + `cabinet.tax.gov.ua/help/api.html`.

**Files:**
- Create: `rust/prro_crypto/proto/check.proto`

**Acceptance Criteria:**
- [ ] All 7 RPC methods present: `sendChk`, `sendChkV2`, `ping`, `lastChk`, `delLastChk`, `delLastChkId`, `statusRro`, `infoRro` (8 including `sendChk` legacy)
- [ ] `Check` message has 7 fields with the right field numbers (per `WriteTo`/`MergeFrom` in decompile)
- [ ] `CheckResponse.Status` enum has OK=1 + 16 negative values matching `docs/webcheck_reverse/TaxGrpc/Com.Programika.Rro.Ws.Chk/CheckResponse.Types.cs`
- [ ] `RroInfoResponse` carries `name`, `addr`, `tins`, `operators[]` so `prro_sidecar_preflight` can display metadata

**Verify:** `cd rust/prro_crypto && cargo build --features sidecar && grep -q "ChkIncomeServiceClient" target/debug/build/prro_crypto-*/out/com.programika.rro.ws.chk.rs`

**Steps:**

- [ ] **Step 1: Write proto/check.proto**

  ```protobuf
  syntax = "proto3";
  package com.programika.rro.ws.chk;

  // Reconstructed from WebCheck PRRO32 TaxGrpc.dll (2026-04-17) cross-
  // checked against cabinet.tax.gov.ua/help/api.html + the existing
  // Python proto at src/prro_gateway/transports/proto/fiscal_server.proto.

  enum CheckType {
    UNKNOWN    = 0;
    CHK        = 1;  // SELL, RETURN, SERVICE_IN/OUT, CASH_WITHDRAWAL
    ZREPORT    = 2;  // Z-report (close-of-day)
    SERVICECHK = 3;  // SHIFT_OPEN, mode transitions
  }

  message Check {
    string    rro_fn       = 1;
    int64     date_time    = 2;  // Kyiv local wall-clock as "fake UTC" epoch
    bytes     check_sign   = 3;  // CMS DER; signed content = cp1251 XML
    int32     local_number = 4;
    CheckType check_type   = 5;
    string    id_offline   = 6;
    string    id_cancel    = 7;
  }

  message CheckRequest   { bytes  rro_fn_sign = 3; }
  message CheckRequestId { string id          = 1; }

  message CheckResponse {
    string id            = 1;
    enum Status {
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
    Status status        = 2;
    bytes  id_sign       = 3;
    bytes  data_sign     = 4;
    string error_message = 5;
  }

  message StatusResponse {
    bool   open_shift    = 1;
    bool   online        = 2;
    string last_signer   = 3;
    CheckResponse.Status status = 4;
    string error_message = 5;
  }

  message RroInfoResponse {
    CheckResponse.Status status       = 1;
    int32                status_rro   = 2;
    bool                 open_shift   = 3;
    bool                 online       = 4;
    string               last_signer  = 5;
    string               name         = 6;
    string               name_to      = 7;
    string               addr         = 8;
    bool                 single_tax   = 9;
    bool                 offline_allowed = 10;
    int32                add_num      = 11;
    string               pn           = 12;
    message Operator {
      string serial = 1;
      int32  status = 2;
      bool   senior = 3;
      string isname = 4;
    }
    repeated Operator operators = 13;
    string            tins      = 14;
    int32             lnum      = 15;
    string            name_pay  = 16;
  }

  service ChkIncomeService {
    rpc sendChk      (Check)          returns (CheckResponse);
    rpc sendChkV2    (Check)          returns (CheckResponse);
    rpc ping         (Check)          returns (CheckResponse);
    rpc lastChk      (CheckRequest)   returns (CheckResponse);
    rpc delLastChk   (CheckRequest)   returns (CheckResponse);
    rpc delLastChkId (CheckRequestId) returns (CheckResponse);
    rpc statusRro    (CheckRequest)   returns (StatusResponse);
    rpc infoRro      (CheckRequest)   returns (RroInfoResponse);
  }
  ```

**Commit:** rolled into Task 0.1.

---

## Task 0.4: src/fiscal/mod.rs + 7 submodule stubs

**Goal:** Module skeleton so every later task touches exactly one file (no duplicate edits).

**Files:**
- Create: `rust/prro_crypto/src/fiscal/mod.rs`
- Create: `rust/prro_crypto/src/fiscal/input.rs` (empty `pub fn` placeholders)
- Create: `rust/prro_crypto/src/fiscal/xml_builder.rs`
- Create: `rust/prro_crypto/src/fiscal/cp1251.rs`
- Create: `rust/prro_crypto/src/fiscal/license.rs`
- Create: `rust/prro_crypto/src/fiscal/grpc_client.rs`
- Create: `rust/prro_crypto/src/fiscal/cms_adapter.rs`
- Create: `rust/prro_crypto/src/fiscal/config.rs`
- Create: `rust/prro_crypto/src/fiscal/repo.rs`
- Create: `rust/prro_crypto/src/fiscal/errors.rs`
- Create: `rust/prro_crypto/src/fiscal/credentials.rs` (XorSoft + Plain backends)
- Modify: `rust/prro_crypto/src/lib.rs`

**Acceptance Criteria:**
- [ ] `cargo check --features sidecar` compiles with all 9 submodules as stubs
- [ ] `src/lib.rs` exposes `#[cfg(feature = "sidecar")] pub mod fiscal;`
- [ ] Each stub has at least one doc comment describing its scope and invariant surface

**Verify:** `cd rust/prro_crypto && cargo check --features sidecar`

**Steps:**

- [ ] **Step 1: Write `src/fiscal/mod.rs`**

  ```rust
  //! Fiscal protocol driver — canonical JSON → cp1251 XML → CMS sign → DPS gRPC.
  //!
  //! All items require the `sidecar` cargo feature.
  //!
  //! Invariant audit surface (mirrors root CLAUDE.md frozen invariants):
  //!   - (1) No network / crypto call inside a SQLite transaction: enforced in `repo`.
  //!   - (6) Full canonical payload reads: `input::CanonicalCommand` uses `#[serde(flatten)]`
  //!         for unknown fields so Python-side additions don't break us.
  //!   - (7) `schema_version` is required by `CanonicalCommand`.
  //!   - (10) Signing is not bypassable except by explicit `dev.skip_sign` + env.

  pub mod cp1251;
  pub mod errors;
  pub mod input;
  pub mod license;
  pub mod xml_builder;

  #[cfg(feature = "sidecar")]
  pub mod cms_adapter;
  #[cfg(feature = "sidecar")]
  pub mod config;
  #[cfg(feature = "sidecar")]
  pub mod credentials;
  #[cfg(feature = "sidecar")]
  pub mod grpc_client;
  #[cfg(feature = "sidecar")]
  pub mod repo;
  ```

- [ ] **Step 2: Minimal stub bodies**

  Each file: one module-level doc comment + a `#[allow(dead_code)]` line to silence warnings until the next phase wires it up. Example (`input.rs`):

  ```rust
  //! Minimal serde parsing of the canonical-command JSON posted by Python.
  //! Only the subset required for XML build + license check is deserialized;
  //! unknown fields pass through via #[serde(flatten)].
  //!
  //! Owner task: Phase 1.1.

  #![allow(dead_code)]
  ```

- [ ] **Step 3: Expose module in lib.rs**

  Append to `rust/prro_crypto/src/lib.rs` (below existing `pub mod` lines):

  ```rust
  // Fiscal driver — all items require the `sidecar` cargo feature.
  #[cfg(feature = "sidecar")]
  pub mod fiscal;
  ```

**Commit:** rolled into Task 0.1 or a dedicated second commit — choose one commit per phase; scaffolding is cheap enough for one commit.

---

## Task 0.5: Binary stubs

**Goal:** Six stub binaries so `cargo build --features sidecar --bins` exits 0; real logic lands in Phases 3–5.

**Files:**
- Create: `rust/prro_crypto/src/bin/prro_sidecar.rs`
- Create: `rust/prro_crypto/src/bin/prro_admin.rs`
- Create: `rust/prro_crypto/src/bin/prro_license_keygen.rs`
- Create: `rust/prro_crypto/src/bin/prro_license_sign.rs`
- Create: `rust/prro_crypto/src/bin/prro_license_verify.rs`
- Create: `rust/prro_crypto/src/bin/prro_sidecar_preflight.rs`

**Acceptance Criteria:**
- [ ] Each binary has a `--help` pattern via clap (just the top-level parser — subcommands come in Phase 5)
- [ ] `cargo build --release --features sidecar --bins` produces 6 executables in `target/release`
- [ ] Each stub prints "{binary}: Phase N stub — see plan doc" and exits 0

**Verify:** `cd rust/prro_crypto && cargo build --release --features sidecar --bins && ls target/release/prro_*`

**Steps:**

- [ ] **Step 1: Template**

  ```rust
  // rust/prro_crypto/src/bin/prro_sidecar.rs
  //! prro_sidecar — HTTP fiscal driver (Phase 5 stub).
  fn main() {
      eprintln!("prro_sidecar: Phase 5 stub — see docs/superpowers/plans/2026-04-19-rust-fiscal-driver-v2.md");
  }
  ```

  Apply the same pattern to the other 5 binaries — each with its own one-liner message pointing to the phase where it's implemented.

**Commit:** single commit for the full scaffolding set.

---

## Task 0.6: license_pubkey.der placeholder + sidecar.example.toml

**Goal:** Placeholders that force the build to fail fast if a real artifact is forgotten at ship time.

**Files:**
- Create: `rust/prro_crypto/src/fiscal/license_pubkey_current.der` (placeholder, current rotation slot)
- Create: `rust/prro_crypto/src/fiscal/license_pubkey_next.der` (placeholder, next rotation slot — can be identical at first deploy)
- Create: `rust/prro_crypto/ops/sidecar.example.toml`
- Create: `rust/prro_crypto/ops/prro-tax-gov-ua-chain.pem.placeholder` (text note)
- Create: `rust/prro_crypto/ops/README.md` (points at the plan doc)

**Acceptance Criteria:**
- [ ] `license_pubkey_current.der` and `license_pubkey_next.der` exist (even as placeholder bytes)
- [ ] Both are `include_bytes!`-referenced in `license.rs` — build fails if either is missing
- [ ] `sidecar.example.toml` has every documented key with inline comments
- [ ] Placeholder clearly labelled so ops cannot accidentally ship it

**Verify:** `ls rust/prro_crypto/src/fiscal/license_pubkey_{current,next}.der && cat rust/prro_crypto/ops/sidecar.example.toml | head -50`

**Steps:**

- [ ] **Step 1: sidecar.example.toml**

  ```toml
  # prro_sidecar configuration — copy to sidecar.toml and edit.
  # License: ops/README.md explains the ship-time master-pubkey swap.

  [sidecar]
  bind      = "127.0.0.1:8090"    # HTTP listener
  log_level = "info"              # trace / debug / info / warn / error

  [db]
  path = "var/prro.db"            # SQLite (same file as Python gateway)

  [license]
  # Optional. If unset sidecar starts in DEMO mode (500 UAH/check, no offline).
  path = "ops/license.json"

  [dps.prod]
  endpoint  = "https://prro.tax.gov.ua:443"
  ca_bundle = "ops/prro-tax-gov-ua-chain.pem"
  connect_timeout_ms = 10000
  request_timeout_ms = 90000

  [dps.test]
  endpoint  = "https://cabinet.tax.gov.ua:9443"
  ca_bundle = "ops/prro-tax-gov-ua-chain.pem"
  connect_timeout_ms = 10000
  request_timeout_ms = 90000

  [tsp]
  # RFC 3161 timestamp server — used when a FN has tsp_enabled = 1.
  url     = "http://acsk.ca.gov.ua/services/tsp/"
  timeout_ms = 5000

  [dev]
  # ONLY for local testing. Requires DEBUG_INSECURE_MODE=1 in env to activate.
  skip_sign  = false
  skip_dps   = false
  ```

- [ ] **Step 2: placeholder `license_pubkey.der`**

  Ship a 32-byte file with a recognizable pattern (`DE AD BE EF` repeated). The build treats it as opaque bytes; real pubkey is dropped in at release.

- [ ] **Step 3: ops/README.md**

  Short note: ship steps (1. run `prro_license_keygen` once, 2. commit the DER into `src/fiscal/license_pubkey.der`, 3. rebuild, 4. never rotate without a migration plan).

**Commit:** rolled into the scaffolding commit.

---

## Task 0.7: SQL migration 017

**Goal:** Extend `fiscal_number_config`, introduce `sidecar_operators`, introduce `licenses` — all under one idempotent migration.

**Files:**
- Create: `sql/017_sidecar_ops_and_fn_business.sql`

**Acceptance Criteria:**
- [ ] Re-running migration on a DB that already has it is a no-op (checksum runner detects)
- [ ] All new columns added with `ALTER TABLE … ADD COLUMN` (SQLite has no `IF NOT EXISTS` for columns → use a runner convention: pre-check via `PRAGMA table_info` or rely on the checksum runner's one-shot guarantee; we rely on one-shot here)
- [ ] `tests/test_gate1j_migration_idempotency.py` passes with 017 applied
- [ ] No existing data is rewritten; new columns all have defaults so old rows remain valid

**Verify:** `rtk proxy pytest tests/test_gate1j_migration_idempotency.py -v`

**Steps:**

- [ ] **Step 1: Write migration**

  ```sql
  -- sql/017_sidecar_ops_and_fn_business.sql
  -- Sprint: Rust Fiscal Driver v2 (ADR-004 v2).
  --
  -- Extends fiscal_number_config with business identity + per-FN driver
  -- behavior flags. Introduces sidecar_operators (JKS credentials per
  -- cashier) and licenses (per-TIN commercial licensing row).
  --
  -- See docs/PER_FN_CONFIG.md for field semantics.

  -- ─── 1. fiscal_number_config additions ──────────────────────────────
  ALTER TABLE fiscal_number_config ADD COLUMN tax_number             TEXT    NOT NULL DEFAULT '';
  ALTER TABLE fiscal_number_config ADD COLUMN fiscal_mode            TEXT    NOT NULL DEFAULT 'test'
      CHECK (fiscal_mode IN ('prod','test'));
  ALTER TABLE fiscal_number_config ADD COLUMN national_check_enabled INTEGER NOT NULL DEFAULT 0
      CHECK (national_check_enabled IN (0,1));
  ALTER TABLE fiscal_number_config ADD COLUMN offline_enabled        INTEGER NOT NULL DEFAULT 1
      CHECK (offline_enabled IN (0,1));
  ALTER TABLE fiscal_number_config ADD COLUMN tsp_enabled            INTEGER NOT NULL DEFAULT 0
      CHECK (tsp_enabled IN (0,1));
  ALTER TABLE fiscal_number_config ADD COLUMN org_name               TEXT;
  ALTER TABLE fiscal_number_config ADD COLUMN org_address            TEXT;

  -- ─── 2. sidecar_operators (1 FN → N cashiers) ──────────────────────
  CREATE TABLE sidecar_operators (
      id             INTEGER PRIMARY KEY AUTOINCREMENT,
      fiscal_number  TEXT    NOT NULL,
      operator_name  TEXT,
      operator_inn   TEXT    NOT NULL,                  -- 10-digit INN (cashier)
      jks_path       TEXT    NOT NULL,                  -- absolute path to container
      jks_password   TEXT    NOT NULL,                  -- XOR-soft obfuscated hex OR plain text (see credentials_mode in sidecar.toml)
      active         INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
      created_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
      updated_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
      FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number)
  );

  CREATE INDEX ix_sidecar_operators_fn
      ON sidecar_operators (fiscal_number, active);
  CREATE INDEX ix_sidecar_operators_active
      ON sidecar_operators (active) WHERE active = 1;

  -- ─── 3. licenses (single active row per install) ───────────────────
  CREATE TABLE licenses (
      id              INTEGER PRIMARY KEY AUTOINCREMENT,
      tin             TEXT    NOT NULL,                 -- EDRPOU / TIN of licensee
      fn_numbers_json TEXT    NOT NULL,                 -- JSON array of allowed FNs
      issued_at       TEXT    NOT NULL,                 -- ISO-8601
      expires_at      TEXT    NOT NULL,                 -- ISO-8601
      tier            TEXT    NOT NULL
          CHECK (tier IN ('demo','basic','pro','enterprise')),
      org_name        TEXT,
      demo_limits_json TEXT,                            -- NULL for paid tiers
      payload_b64     TEXT    NOT NULL,                 -- base64 of license JSON (JCS-canonical)
      signature_b64   TEXT    NOT NULL,                 -- base64 of detached DSTU signature
      installed_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
      active          INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1))
  );

  CREATE UNIQUE INDEX ix_licenses_active_single
      ON licenses(active) WHERE active = 1;
  ```

- [ ] **Step 2: Verify idempotency test**

  If the migration runner already discovers `sql/*.sql` by glob and checksums, no test changes are needed. Otherwise extend `tests/test_gate1j_migration_idempotency.py` to include file 017 explicitly.

**Commit:**

```
feat(sql): migration 017 — sidecar operators, FN business identity, licenses

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Phase 0 deliverable summary:** 1 commit, `cargo check --features sidecar` green, migration 017 applied.

---

# Phase 1 — JSON input + SQLite repo

## Task 1.1: `src/fiscal/input.rs` — canonical-command serde structs

**Goal:** Parse the minimum canonical-command JSON subset Rust needs for XML build + license check. Unknown keys must not break us.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/input.rs`

**Acceptance Criteria:**
- [ ] `CanonicalCommand`, `Receipt`, `Goods`, `Payment`, `Discount`, `ReceiptTotals`, `ZReportData` structs resolve
- [ ] `#[serde(deny_unknown_fields)]` is **not** used — forward compat with Python additions
- [ ] Fields required by XML build are present; optional fields use `Option<T>` with `#[serde(default)]`
- [ ] Round-trip: serialize → deserialize → serialize produces identical JSON on all 10 Phase-2 golden fixtures (unit test)
- [ ] `schema_version` is required (serde missing-field error on absence) — invariant (7)
- [ ] `operation_type` is `enum OperationType` with the same 8 supported variants as `_OP_TO_CHECK_TYPE` in `dps_fiscal_server.py`

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar input`

**Steps:**

- [ ] **Step 1: Struct definitions**

  Signatures (abbreviated — full field list mirrors `src/prro_gateway/models/canonical.py`):

  ```rust
  use serde::{Deserialize, Serialize};

  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
  pub enum OperationType {
      ShiftOpen,
      ShiftClose,
      Sell,
      Return,
      ServiceIn,
      ServiceOut,
      CashWithdrawal,
      ZReport,
      // XReport, GoOffline, GoOnline — intentionally absent: sidecar refuses them early.
  }

  #[derive(Debug, Clone, Deserialize, Serialize)]
  pub struct CanonicalCommand {
      pub schema_version: String,
      pub request_id:     String,
      pub idempotency_key: String,
      pub operation_type: OperationType,
      pub fiscal_number:  String,
      pub business_ts:    String,              // ISO-8601 UTC — parsed by xml_builder
      pub payload:        serde_json::Value,   // keeps full payload for audit
      pub payload_sha256: String,
      // Convenience: a typed view of payload for XML/license purposes.
      #[serde(default)]
      pub receipt:        Option<Receipt>,
      #[serde(default)]
      pub z_report_data:  Option<ZReportData>,
      #[serde(default)]
      pub service_sum:    Option<i64>,
      #[serde(default)]
      pub cash_withdrawal_sum: Option<i64>,
      #[serde(flatten)]
      pub other:          serde_json::Map<String, serde_json::Value>,
  }
  ```

  Full `Receipt` / `Goods` / `Payment` / `Discount` / `ReceiptTotals` / `ZReportData` structs mirror canonical.py exactly — one i64 for every monetary field (kopecks, always non-negative), `Option<String>` for every nullable string.

- [ ] **Step 2: Convenience helper**

  ```rust
  impl CanonicalCommand {
      /// Whether a payment has an RRN (triggers <L>ERECEIPT…</L> tags
      /// when fn_config.national_check_enabled = 1).
      pub fn has_card_rrn(&self) -> bool {
          self.receipt.as_ref().map_or(false, |r| {
              r.payments.iter().any(|p| p.rrn.as_deref().unwrap_or("").len() > 0)
          })
      }
  }
  ```

- [ ] **Step 3: Unit tests (10 round-trip fixtures)**

  `rust/prro_crypto/tests/input_roundtrip.rs` loads `tests/fixtures/canonical_*.json` (produced by `scripts/dps_golden_dump.py` in Task 2.6) and asserts deserialize → serialize → deserialize is stable.

  Expect the following 10 fixture names (selected from existing Python tests):
  1. `shift_open_minimal.json`
  2. `sell_cash_single_item.json`
  3. `sell_card_with_rrn.json`
  4. `sell_multi_payment_with_change.json`
  5. `sell_with_per_item_discount.json`
  6. `sell_with_check_level_discount.json`
  7. `return_with_cancel_id.json`
  8. `service_in_cash.json`
  9. `cash_withdrawal_card.json`
  10. `z_report_full.json`

**Commit:** combined with Task 1.2.

---

## Task 1.2: `src/fiscal/repo.rs` — rusqlite queries

**Goal:** One module, short transactions only, every query documented.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/repo.rs`

**Acceptance Criteria:**
- [ ] `Repo::open(path)` returns a `Repo` with `rusqlite::Connection` inside a `Mutex`
- [ ] Methods: `load_fn_config(fn) -> Option<FnConfig>`, `load_active_operator(fn) -> Option<Operator>`, `load_active_license() -> Option<StoredLicense>`, `load_operator_cert_metadata(fn) -> Option<CertInfo>`, `audit_log_insert(entry)`
- [ ] Every method opens / commits within one SQL statement (no transactions spanning network calls) — invariant (1)
- [ ] `load_active_operator` uses the exact JOIN from `docs/PER_FN_CONFIG.md`
- [ ] Unit test against an in-memory SQLite DB preloaded with migrations 011 + 015 + 017

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar repo`

**Steps:**

- [ ] **Step 1: Types**

  ```rust
  pub struct Repo { conn: std::sync::Mutex<rusqlite::Connection> }

  #[derive(Debug, Clone)]
  pub struct FnConfig {
      pub fiscal_number: String,
      pub tax_number:    String,
      pub fiscal_mode:   FiscalMode,    // Prod | Test
      pub national_check_enabled: bool,
      pub offline_enabled: bool,
      pub tsp_enabled:     bool,
      pub org_name:        Option<String>,
      pub org_address:     Option<String>,
      pub enforce_blocked_mode: bool,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum FiscalMode { Prod, Test }

  #[derive(Debug, Clone)]
  pub struct Operator {
      pub id: i64,
      pub fiscal_number: String,
      pub operator_inn:  String,
      pub operator_name: Option<String>,
      pub jks_path:      String,
      pub jks_password:  String,   // stored value; decode via CredentialStore::fetch()
      pub cert_valid_to: Option<String>,  // from operator_certs — used as XOR key material
  }

  #[derive(Debug, Clone)]
  pub struct StoredLicense {
      pub id: i64,
      pub payload_b64:   String,
      pub signature_b64: String,
      pub installed_at:  String,
  }

  #[derive(Debug, Clone)]
  pub struct CertInfo {
      pub valid_to: time::OffsetDateTime,
      pub subject_dn: String,
  }
  ```

- [ ] **Step 2: SQL queries**

  Hard-coded query strings (documented inline). Example:

  ```rust
  impl Repo {
      pub fn load_active_operator(&self, fn_: &str) -> rusqlite::Result<Option<Operator>> {
          let conn = self.conn.lock().unwrap();
          let mut st = conn.prepare_cached(
              "SELECT so.id, so.fiscal_number, so.operator_inn, so.operator_name,
                      so.jks_path, so.jks_password, oc.valid_to
                 FROM sidecar_operators so
                 LEFT JOIN operator_certs oc USING (fiscal_number)
                WHERE so.fiscal_number = ? AND so.active = 1
                ORDER BY so.created_at ASC
                LIMIT 1"
          )?;
          // … map_row
      }
  }
  ```

- [ ] **Step 3: Unit test against migrations**

  `rust/prro_crypto/tests/repo_roundtrip.rs`:

  - Opens `rusqlite::Connection::open_in_memory()`.
  - Runs `include_str!("../../sql/011_per_fn_config.sql")` + 015 + 017.
  - Seeds a FN + operator + license row.
  - Asserts all loaders return the right values.

**Commit:**

```
feat(sidecar): canonical input parser and SQLite repository

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Phase 1 deliverable summary:** 1 commit, ~600 LoC Rust + 10 JSON fixtures + unit tests green.

---

# Phase 2 — XML builder + cp1251 + golden tests

## Task 2.1: helpers — `tag()`, `xml_escape()`, `calc_tax()`, `kyiv_epoch()`, cp1251 encoder

**Goal:** Port the small utility surface from `dps_xml.py` that every later xml_builder sub-task depends on.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/cp1251.rs`
- Modify: `rust/prro_crypto/src/fiscal/xml_builder.rs` (helpers at top of file)

**Acceptance Criteria:**
- [ ] `tag(name, attrs, content)` — attrs are `Vec<(&str, String)>`, sorted alphabetically (matches Python `sorted(attrs.items())`)
- [ ] `xml_escape` replaces `& " < >` in exactly the same order as the Python
- [ ] `calc_tax(group_sum, tax_rate, additional_rate, tax_algorithm)` returns `(txsm: i64, dtsm: i64)` with the four TXAL branches
- [ ] `kyiv_local_ts_string(iso_utc: &str) -> String` produces `%Y%m%d%H%M%S` in Europe/Kyiv — must match `_kyiv_local_epoch` semantics
- [ ] `encode_cp1251(xml_utf8: &str) -> Vec<u8>` via `encoding_rs::WINDOWS_1251.encode()`; non-representable characters replaced with `?` (same as existing Python behavior)
- [ ] Unit tests for each helper with at least 5 inputs

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar xml_helpers`

**Steps:**

- [ ] **Step 1: Port `_tag`**

  ```rust
  pub fn tag(name: &str, attrs: &[(&str, String)], content: &str) -> String {
      let mut a = attrs.to_vec();
      a.sort_by(|x, y| x.0.cmp(y.0));
      let mut open = format!("<{name}");
      for (k, v) in a.iter() {
          let v = xml_escape(v);
          open.push(' ');
          open.push_str(k);
          open.push_str("=\"");
          open.push_str(&v);
          open.push('"');
      }
      open.push('>');
      format!("{open}{content}</{name}>")
  }
  ```

  Note: Python skips `None` values. Rust callers pass only `Some` — we filter at the call site.

- [ ] **Step 2: calc_tax port**

  Exact 1:1 translation of the Python formulas. Rust uses integer `i64` for sums (kopecks) but f64 for `tax_rate` / `additional_rate` per the existing model. `round()` = banker's rounding in Python; we use `(x + 0.5).floor()` with the same sign handling — add one unit test per branch matching golden Python output.

- [ ] **Step 3: kyiv_local_ts_string**

  Use `time` crate with `tz` feature. Fallback to UTC when parsing fails (same as Python).

**Commit:** combined with Task 2.2–2.4.

---

## Task 2.2: `build_shift_open`, `build_sell`, `build_return`

**Goal:** Cover the three most frequent operations first.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/xml_builder.rs`

**Acceptance Criteria:**
- [ ] `build_shift_open(&CanonicalCommand, &FnConfig, &BuildCtx) -> String` produces `<C T="108">` with `DI="0"`
- [ ] `build_sell` produces `<C T="0">`
- [ ] `build_return` produces `<C T="1">` and embeds `id_cancel` as `<E>` attribute when present (per dps_xml.py — cancel reference is a sidecar concern, not XML — verify!)
- [ ] `<DAT>` attrs order: `DI`, `FN`, `TN`, `V`, `ZN` (alphabetical, matches python)
- [ ] `<E>` emits `FN`, `N`, `NO`, `SM`, `TS` — always (matches commit `d2680a0`)
- [ ] Per-item and check-level discounts via `<D>`/`<S>` tags follow TY/PR/SM rules from `dps_xml.py`

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar builder_shift_open builder_sell builder_return`

**Steps:**

- [ ] **Step 1: BuildCtx**

  ```rust
  pub struct BuildCtx<'a> {
      pub fn_config:     &'a FnConfig,
      pub previous_hash: &'a str,      // MAC chain
      pub z_number:      i64,
      pub device_name:   &'a str,      // default "ПРО_каса"
      pub device_version: &'a str,     // default "1.1"
      pub tax_groups:    Option<&'a TaxGroupMap>,
      pub local_number:  i64,
  }
  ```

- [ ] **Step 2: build_shift_open**

  Direct port of `_build_shift_open`. Opening sum extraction from `payload.receipt.totals.total_sum`.

- [ ] **Step 3: build_sell / build_return**

  Port `_build_check` including:
  - header lines → `<L>` before `<P>`
  - per-item discounts → `<D>`/`<S>` with `NI = p_item_no`
  - check-level discounts → `<D TR="1">` with `<NI NI="…"/>` children
  - payments → `<M>` with EPZ attributes for non-cash
  - `RM` (change) assigned to the first CASH payment only
  - `SMP` (rounding) assigned to the first CASH payment only
  - footer → `<L>` after payments, before `<E>`

**Commit:** phase commit 2a.

---

## Task 2.3: `build_service_in`, `build_service_out`, `build_cash_withdrawal`

**Goal:** Three less-frequent but still production operations.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/xml_builder.rs`

**Acceptance Criteria:**
- [ ] `build_service_in/out` emit `<C T="2">` with `<I>` or `<O>` tag
- [ ] `build_cash_withdrawal` emits `<C T="8">` with mandatory `<P>` + `<M>` + `<E>`
- [ ] Defaults: PC = "ВИДАЧА КОШТІВ" when not provided, commission → `PF`

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar builder_service builder_cash_withdrawal`

**Steps:**

- [ ] **Step 1: Port `_build_service`** (identical structure to Python).
- [ ] **Step 2: Port `_build_cash_withdrawal`**.

**Commit:** phase commit 2b.

---

## Task 2.4: `build_z_report`

**Goal:** Multi-section Z-report aggregation — the most complex XML body.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/xml_builder.rs`

**Acceptance Criteria:**
- [ ] `<Z NO="…">` contains `<TXS>` per tax group in `sorted(keys)` order
- [ ] `<M>` rows for each payment type — same rule (CASH → T='0', others → T='2')
- [ ] `<IO>` rows for each service_sums key
- [ ] `<NC NI NO>` single entry
- [ ] `<EPZ EPC EPCS EPSM>` single entry iff non-empty
- [ ] Golden equality with Python for `test_sprint9_z_report.py` cases

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar builder_z_report`

**Steps:**

- [ ] **Step 1: Port `_build_z_report`** keeping sort order identical.

**Commit:** phase commit 2c.

---

## Task 2.5: Національний чек ("National Check") tags

**Goal:** When `fn.national_check_enabled = 1` AND receipt payments include an RRN, append the 5 `<L>` tags (ERECEIPT, BID, RID, BTX, TIN) inside the `<C>` content.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/xml_builder.rs`

**Acceptance Criteria:**
- [ ] Activation condition: `ctx.fn_config.national_check_enabled && cmd.has_card_rrn()`
- [ ] Tags emitted in fixed order before `<E>`: `<L>ERECEIPT</L>`, `<L>BID=…</L>`, `<L>RID=…</L>`, `<L>BTX=…</L>`, `<L>TIN=…</L>`
- [ ] Values: BID = receipt_id (or request_id), RID = payment.rrn, BTX = derived from payment.payment_system, TIN = fn_config.tax_number
- [ ] Inactive when flag off → no tags, XML byte-identical to the non-national version

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar builder_national_check`

**Steps:**

- [ ] **Step 1: Helper `national_check_tags(cmd, ctx) -> String`** — returns empty string when inactive.
- [ ] **Step 2: Insert at the right position** inside `build_sell` / `build_return` (right before `<E>`).
- [ ] **Step 3: Document** the tag emission rules in an in-file comment pointing to the WebCheck source file `StringXML.cs` for cross-check.

**Commit:** phase commit 2d (combined with 2.6).

---

## Task 2.6: Golden-test generator + golden test suite

**Goal:** Python writes a JSON dump of (canonical-command, expected-cp1251-bytes-hex) for ~30 scenarios; Rust compares byte-identical.

**Files:**
- Create: `scripts/dps_golden_dump.py`
- Create: `rust/prro_crypto/tests/fixtures/golden_*.json` (generated)
- Create: `rust/prro_crypto/tests/xml_golden.rs`

**Acceptance Criteria:**
- [ ] `scripts/dps_golden_dump.py` reads the same fixtures used by the existing Python tests (sprint 7 / 9 / 10) and writes one JSON per scenario with `command`, `fn_config`, `expected_xml_cp1251_hex`, `expected_mac_hash_prefix`
- [ ] At least 30 scenarios covering: 3× shift_open, 8× sell (cash / card / multi-payment / per-item discount / check-level discount / TXAL 0 / 1 / 2), 3× return, 3× service_in, 3× service_out, 3× cash_withdrawal, 5× z_report, 2× national_check
- [ ] Rust test `xml_golden.rs` loads every `tests/fixtures/golden_*.json` and asserts Rust output bytes == expected hex bytes
- [ ] On mismatch: diff with byte offsets is printed (so maintainers can see which tag differs)
- [ ] Runs under 10 s

**Verify:**
```
python scripts/dps_golden_dump.py --out rust/prro_crypto/tests/fixtures/
cd rust/prro_crypto && cargo test --features sidecar --test xml_golden
```

**Steps:**

- [ ] **Step 1: Scenario registry (scripts/dps_golden_dump.py)**

  ```python
  SCENARIOS = [
      ("shift_open_minimal",            build_shift_open_fixture_minimal),
      ("shift_open_with_opening_sum",   build_shift_open_fixture_opening_sum),
      ("shift_open_with_mac_chain",     build_shift_open_fixture_mac_chain),
      ("sell_cash_single_item",         build_sell_fixture_cash_single),
      ("sell_card_with_rrn",            build_sell_fixture_card_rrn),
      ("sell_multi_payment_with_change", build_sell_fixture_multi_payment),
      ("sell_with_per_item_discount",   build_sell_fixture_discount_item),
      ("sell_with_check_level_discount", build_sell_fixture_discount_check),
      ("sell_with_txal_0",              build_sell_fixture_txal0),
      ("sell_with_txal_1",              build_sell_fixture_txal1),
      ("sell_with_txal_2",              build_sell_fixture_txal2),
      ("sell_with_excise_ca",           build_sell_fixture_excise),
      ("return_with_cancel_id",         build_return_fixture_cancel),
      ("return_without_cancel",         build_return_fixture_no_cancel),
      ("return_technical",              build_return_fixture_technical),
      ("service_in_cash",               build_service_in_fixture),
      ("service_in_with_header",        build_service_in_fixture_header),
      ("service_out_cash",              build_service_out_fixture),
      ("service_out_with_header",       build_service_out_fixture_header),
      ("service_out_zero_balance",      build_service_out_fixture_zero),
      ("cash_withdrawal_card",          build_cash_withdrawal_fixture_card),
      ("cash_withdrawal_default_label", build_cash_withdrawal_fixture_default),
      ("cash_withdrawal_with_commission", build_cash_withdrawal_fixture_commission),
      ("z_report_empty",                build_z_report_fixture_empty),
      ("z_report_with_taxes",           build_z_report_fixture_taxes),
      ("z_report_with_payments",        build_z_report_fixture_payments),
      ("z_report_with_service",         build_z_report_fixture_service),
      ("z_report_full",                 build_z_report_fixture_full),
      ("national_check_active",         build_national_check_active),
      ("national_check_inactive",       build_national_check_inactive),
  ]
  ```

- [ ] **Step 2: JSON schema per fixture**

  ```json
  {
    "name": "sell_cash_single_item",
    "command": { /* full canonical command */ },
    "fn_config": {
      "fiscal_number": "3001234567",
      "tax_number": "1234567890",
      "fiscal_mode": "test",
      "national_check_enabled": false,
      "offline_enabled": true,
      "tsp_enabled": false,
      "org_name": null,
      "org_address": null,
      "enforce_blocked_mode": false
    },
    "tax_groups": { "0": { "tax_rate": 20.0, "additional_rate": 0.0, "tax_algorithm": 0, "tax_type": 0 } },
    "z_number": 17,
    "local_number": 42,
    "previous_hash": "abc123",
    "expected_xml_cp1251_hex": "3c52512056..."
  }
  ```

- [ ] **Step 3: Rust test loader**

  ```rust
  #[test]
  fn golden_scenarios() {
      let dir = std::path::Path::new("tests/fixtures");
      let mut failed = Vec::new();
      for entry in std::fs::read_dir(dir).unwrap() {
          let path = entry.unwrap().path();
          if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
          let scenario: Scenario = serde_json::from_reader(std::fs::File::open(&path).unwrap()).unwrap();
          let actual = prro_crypto::fiscal::xml_builder::build(&scenario.command, &scenario.ctx());
          let actual_bytes = prro_crypto::fiscal::cp1251::encode_cp1251(&actual);
          let expected = hex::decode(&scenario.expected_xml_cp1251_hex).unwrap();
          if actual_bytes != expected {
              failed.push(diff_report(&scenario.name, &actual_bytes, &expected));
          }
      }
      if !failed.is_empty() { panic!("{} golden scenarios failed:\n{}", failed.len(), failed.join("\n")); }
  }
  ```

**Commit:**

```
feat(sidecar): XML builder — full port of dps_xml.py + 30 golden scenarios

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Phase 2 deliverable summary:** 3 commits, all 7 operation types built in Rust, 30 byte-identical golden scenarios green.

---

# Phase 3 — License module + tools

## Task 3.1: `src/fiscal/license.rs` — payload + JCS + verify

**Goal:** Detached DSTU-signed license with deterministic canonicalization (RFC 8785 JCS). Master pubkey baked in via `include_bytes!`.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/license.rs`

**Acceptance Criteria:**
- [ ] `LicensePayload` struct has exactly 9 fields (see below) — schema frozen for v1
- [ ] `LicensePayload::to_canonical_bytes()` returns RFC 8785 JCS bytes (sorted keys, no whitespace, UTF-8, deterministic number formatting)
- [ ] `verify(payload_json, signature, master_pubkey_der) -> Result<LicenseState, LicenseError>` covers 6 states: `Valid`, `Grace { days_left }`, `Expired`, `TinMismatch`, `FnNotLicensed`, `SignatureInvalid`
- [ ] Demo mode (`tier == "demo"`): enforces `max_payment_sum_kopecks` and `no_offline`; returns `LicenseState::Demo { …limits }`
- [ ] 14-day grace: `expires_at - 14d <= now < expires_at` → warning state; `now >= expires_at` → hard stop
- [ ] Unit tests: 10 scenarios (see below)

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar license`

**Steps:**

- [ ] **Step 1: LicensePayload schema (v1, frozen)**

  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct LicensePayload {
      pub schema_version: String,              // "license.v1"
      pub tin:            String,              // TIN / EDRPOU of licensee
      pub fn_numbers:     Vec<String>,         // FNs allowed — sorted ASC
      pub issued_at:      String,              // ISO-8601 UTC
      pub expires_at:     String,              // ISO-8601 UTC
      pub tier:           LicenseTier,         // Demo | Basic | Pro | Enterprise
      pub org_name:       Option<String>,
      pub demo_limits:    Option<DemoLimits>,  // Some iff tier == Demo
      pub issuer:         String,              // "PRRO_GATE_ADMIN" or similar
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct DemoLimits {
      pub max_payment_sum_kopecks: i64,       // 50000 = 500 UAH; checked PER-CHECK, not cumulative
      pub no_offline:              bool,       // true for demo
      pub require_dps_online:      bool,       // true for demo
  }

  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(rename_all = "lowercase")]
  pub enum LicenseTier { Demo, Basic, Pro, Enterprise }

  #[derive(Debug)]
  pub enum LicenseState {
      Valid,
      Grace { days_left: i32 },
      Expired,
      TinMismatch { expected: String, actual: String },
      FnNotLicensed { fn_: String },
      Demo { limits: DemoLimits },
      SignatureInvalid,
  }
  ```

- [ ] **Step 2: JCS canonicalization**

  Hand-rolled or via a minimal RFC 8785 implementation: sort object keys lex-ASC, recurse into arrays, normalize numbers as shortest-round-trip JSON representation.

- [ ] **Step 3: Two-function API**

  Two public functions share the same decode logic but differ in what they check:

  ```rust
  /// Verify signature + expiry only. Used at startup and by install_license.
  /// Does NOT check TIN or FN membership (those are checked per-request).
  pub fn verify_signature_only(
      payload_b64:  &str,
      signature_b64: &str,
      now: time::OffsetDateTime,
  ) -> Result<LicenseState, LicenseError>  // returns Valid/Grace/Expired/SignatureInvalid

  /// Full per-request check: sig + expiry + TIN + FN membership + tier limits.
  pub fn verify(
      payload_b64:  &str,
      signature_b64: &str,
      fn_: &str,
      tin: &str,
      now: time::OffsetDateTime,
  ) -> Result<LicenseState, LicenseError> {
      let payload_bytes = base64::decode(payload_b64)?;
      let signature = base64::decode(signature_b64)?;
      let payload: LicensePayload = serde_json::from_slice(&payload_bytes)?;

      // 1. signature check — try current pubkey first, then next (rotation support, B2)
      let jcs = payload.to_canonical_bytes();
      const PUBKEY_CURRENT: &[u8] = include_bytes!("license_pubkey_current.der");
      const PUBKEY_NEXT: &[u8]    = include_bytes!("license_pubkey_next.der");
      let sig_ok = prro_crypto::core::verify_detached(PUBKEY_CURRENT, &jcs, &signature)
          .unwrap_or(false)
          || prro_crypto::core::verify_detached(PUBKEY_NEXT, &jcs, &signature)
              .unwrap_or(false);
      if !sig_ok {
          return Ok(LicenseState::SignatureInvalid);
      }

      // 2. tin / fn membership
      if payload.tin != tin {
          return Ok(LicenseState::TinMismatch { expected: payload.tin, actual: tin.into() });
      }
      if !payload.fn_numbers.iter().any(|f| f == fn_) {
          return Ok(LicenseState::FnNotLicensed { fn_: fn_.into() });
      }

      // 3. tier-specific
      if payload.tier == LicenseTier::Demo {
          let limits = payload.demo_limits.ok_or(LicenseError::MissingDemoLimits)?;
          return Ok(LicenseState::Demo { limits });
      }

      // 4. expiry / grace
      let expires = parse_iso8601(&payload.expires_at)?;
      let grace_start = expires - time::Duration::days(14);
      if now >= expires {
          return Ok(LicenseState::Expired);
      }
      if now >= grace_start {
          let days_left = (expires - now).whole_days() as i32;
          return Ok(LicenseState::Grace { days_left });
      }
      Ok(LicenseState::Valid)
  }
  ```

- [ ] **Step 4: Ten unit tests**

  1. `valid_pro_within_expiry`
  2. `grace_window_13d`, `grace_window_1d`
  3. `expired_1d_ago`
  4. `tin_mismatch`
  5. `fn_not_in_list`
  6. `signature_tampered`
  7. `demo_valid`
  8. `demo_without_limits_errors`
  9. `jcs_key_order_stable_across_serializations`
  10. `jcs_unicode_chars_preserved`

**Commit:** combined with Task 3.2.

---

## Task 3.2: `prro_license_keygen`

**Goal:** CLI to generate a DSTU master keypair for offline signing of licenses.

**Files:**
- Modify: `rust/prro_crypto/src/bin/prro_license_keygen.rs`

**Acceptance Criteria:**
- [ ] `prro_license_keygen --out-priv master.key --out-pub master.pub.der` produces two files
- [ ] Private key is DSTU 4145 PB-257 scalar, serialized as little-endian 32 bytes (matches existing `DstuInProcessSigner` format)
- [ ] Public key is DER-encoded `SubjectPublicKeyInfo` with correct DSTU OID
- [ ] Exits non-zero on file write failure or existing output path (no overwrites without `--force`)

**Verify:**
```
cd rust/prro_crypto && cargo run --features sidecar --bin prro_license_keygen -- --out-priv /tmp/m.key --out-pub /tmp/m.der
```

**Steps:**

- [ ] **Step 1: clap parser**

  ```rust
  #[derive(clap::Parser)]
  struct Cli {
      #[clap(long)] out_priv: std::path::PathBuf,
      #[clap(long)] out_pub:  std::path::PathBuf,
      #[clap(long)] force:    bool,
  }
  ```

- [ ] **Step 2: Key generation**

  Use `prro_crypto::core::scalar::Scalar::random_nonzero(curve)` and `prro_crypto::core::point::Point::generator_mul(scalar)`, serialize via existing helpers.

- [ ] **Step 3: DER encoding**

  Reuse `prro_crypto::cms::builder` helpers for `SubjectPublicKeyInfo`.

**Commit:** combined with Task 3.3 / 3.4 (single "license tools" commit).

---

## Task 3.3: `prro_license_sign` (single + CSV batch)

**Goal:** Offline signer CLI operated by the product owner / support desk.

**Files:**
- Modify: `rust/prro_crypto/src/bin/prro_license_sign.rs`

**Acceptance Criteria:**
- [ ] Single mode: `prro_license_sign --master master.key --tin 1234567890 --fn 3001234567 --fn 3001234568 --expires-at 2027-04-19 --tier pro --out license.json`
- [ ] Batch mode: `prro_license_sign --master master.key --from-csv customers.csv --out-dir licenses/`
- [ ] `license.json` format: `{ "payload_b64": "...", "signature_b64": "...", "payload": {...} }` — `payload` duplicated for human readability
- [ ] Exits non-zero on CSV parse failure, on invalid tier, on past-dated `expires_at`
- [ ] Batch output: one file per row, named `{tin}.json`

**Verify:**
```
cd rust/prro_crypto && cargo run --features sidecar --bin prro_license_sign -- --help
```

**Steps:**

- [ ] **Step 1: CSV schema**

  ```
  tin,fn_numbers,expires_at,tier,org_name
  1234567890,"3001234567;3001234568",2027-04-19,pro,"ТОВ Мій Магазин"
  ```

- [ ] **Step 2: Sign path**

  Reuse `prro_crypto::core::sign::sign()` over JCS bytes (see Task 3.1).

**Commit:** combined with 3.2 / 3.4.

---

## Task 3.4: `prro_license_verify`

**Goal:** Dry-run verifier — takes a license file + env-provided TIN/FN/now, prints the state. Useful for support debugging.

**Files:**
- Modify: `rust/prro_crypto/src/bin/prro_license_verify.rs`

**Acceptance Criteria:**
- [ ] `prro_license_verify --license license.json --tin 1234567890 --fn 3001234567` prints state + exit code 0 on Valid/Grace/Demo, non-zero on Expired/TinMismatch/FnNotLicensed/SignatureInvalid
- [ ] Uses the embedded `license_pubkey.der` — no external pubkey arg (prevents operators from fake-verifying with a test pubkey)
- [ ] `--now 2027-04-19T12:00:00Z` override for time-travel testing

**Verify:**
```
cd rust/prro_crypto && cargo run --features sidecar --bin prro_license_verify -- --help
```

**Steps:**

- [ ] **Step 1: Structure**

  ```rust
  // Uses same dual-pubkey embed as license.rs (current + next rotation slot):
  const PUBKEY_CURRENT: &[u8] = include_bytes!("../../src/fiscal/license_pubkey_current.der");
  const PUBKEY_NEXT:    &[u8] = include_bytes!("../../src/fiscal/license_pubkey_next.der");
  ```

- [ ] **Step 2: Emit JSON output** (so ops can pipe into jq):

  ```json
  { "state": "Grace", "days_left": 7, "tier": "pro", "tin": "1234567890" }
  ```

**Commit:**

```
feat(sidecar): license module + keygen/sign/verify CLIs

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Phase 3 deliverable summary:** 1 commit (tools) + 1 commit (verify is standalone), ~900 LoC, 10 license unit tests green.

---

# Phase 4 — gRPC client + CMS adapter + config

## Task 4.1: `src/fiscal/grpc_client.rs` — mode-aware persistent channels

**Goal:** Two long-lived tonic channels (prod + test) with keep-alive, TLS-roots + optional PEM bundle, and wrappers for all 7 RPC methods.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/grpc_client.rs`

**Acceptance Criteria:**
- [ ] `DpsGrpcPool::new(config: &DpsConfig) -> Result<Self>` creates both prod and test channels eagerly (and warms them with `ping`)
- [ ] `DpsGrpcPool::for_mode(FiscalMode) -> DpsClient` returns a cheaply-cloneable client (tonic channel is Arc<> internally)
- [ ] All 7 wrappers: `send_chk_v2`, `send_chk`, `ping`, `last_chk`, `del_last_chk`, `del_last_chk_id`, `status_rro`, `info_rro` (8 including sendChk legacy)
- [ ] Every wrapper returns `Result<T, GrpcError>` where `GrpcError` distinguishes `Transport`, `Status`, `Tls`, `Deadline`
- [ ] Keep-alive settings match the Python client (30s interval, 10s timeout, permit_without_calls=true)
- [ ] Warmup failure is logged but does not fail `new()`

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar grpc_pool`

**Steps:**

- [ ] **Step 1: proto module include**

  ```rust
  pub mod proto {
      tonic::include_proto!("com.programika.rro.ws.chk");
  }
  ```

- [ ] **Step 2: pool + client**

  Tonic's `Channel::clone()` is cheap; we store `ChkIncomeServiceClient<Channel>` per mode in an `Arc<DpsGrpcPool>`.

- [ ] **Step 3: Error classifier**

  `fn classify_dps_status(s: proto::check_response::Status) -> DpsErrorCategory { … }` — `Transient` (ERROR_SAVE, ERROR_UNKNOWN, ERROR_BAD_HASH_PREV) vs `Permanent` (ERROR_XML, ERROR_NOT_REGISTERED_RRO, …). Documented table in Task 4.3.

- [ ] **Step 4: tonic mock server test**

  Use `tokio::test` + `tonic::transport::Server` spawning an `InMemoryChkIncomeService` that returns canned responses. Assert all 7 wrappers hit the right RPC path.

**Commit:**

```
feat(sidecar): DPS gRPC pool + CMS adapter + TOML config

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 4.2: `src/fiscal/cms_adapter.rs`

**Goal:** Thin, testable adapter over existing `prro_crypto::cms::CmsSigner` that chooses `sign_with` vs `sign_with_tst` per `fn.tsp_enabled`.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/cms_adapter.rs`

**Acceptance Criteria:**
- [ ] `CmsAdapter::sign(content_bytes, signer, cert_der, ctx) -> Result<CmsSignedBytes, CmsError>`
- [ ] When `ctx.tsp_enabled` → calls `sign_with_tst(tsp_url, tsp_timeout)`; otherwise `sign_with(now)`
- [ ] **TSP URL resolution via `ca_endpoints` automapping** (migration 016 — already in production): parse `cert_der.issuer_dn`, run `SELECT tsp_url FROM ca_endpoints WHERE ? LIKE '%' || issuer_pattern || '%' AND enabled = 1 ORDER BY priority ASC LIMIT 1`. If no match → error `NoTspMapping{issuer_dn}`. No new columns needed in migration 017.
- [ ] Returns DER bytes suitable for `Check.check_sign`
- [ ] TSP failure with `ctx.tsp_enabled = required` propagates; with `ctx.tsp_enabled = optional` logs + falls back (flag reserved, default required)
- [ ] Unit test with a fixed signer + cert (reuse `tests/` fixtures from existing CMS tests)
- [ ] Unit test for `ca_endpoints` mapping: 3 cases — `acskidd`/`податкова` → `acskidd.gov.ua/services/tsp/`; `приватбанк` → `acsk.privatbank.ua/services/tsp/`; unknown issuer → `NoTspMapping`

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar cms_adapter`

**Steps:**

- [ ] **Step 1: signature**

  ```rust
  pub struct CmsSignCtx<'a> {
      pub tsp_enabled: bool,
      pub tsp_timeout_ms: u64,
      pub now: time::OffsetDateTime,
      // TSP URL is NOT passed from caller — resolved inside sign_content()
      // via `resolve_tsp_url(&conn, issuer_dn)` (ca_endpoints lookup).
      pub conn: Option<&'a rusqlite::Connection>,
  }

  pub fn sign_content(
      content: &[u8],
      signer: &prro_crypto::cms::signer::DstuInProcessSigner,
      cert_der: &[u8],
      ctx: &CmsSignCtx<'_>,
  ) -> Result<Vec<u8>, CmsAdapterError> { … }

  // Helper — consulted only when tsp_enabled = true.
  fn resolve_tsp_url(
      conn: &rusqlite::Connection,
      issuer_dn: &str,
  ) -> Result<String, CmsAdapterError> {
      // SELECT tsp_url FROM ca_endpoints
      //  WHERE ? LIKE '%' || lower(issuer_pattern) || '%'
      //    AND enabled = 1 AND tsp_url IS NOT NULL
      //  ORDER BY priority ASC LIMIT 1
      // ...
  }
  ```

**Commit:** combined with 4.1.

---

## Task 4.3: `src/fiscal/config.rs` — TOML parser

**Goal:** Typed `SidecarConfig` produced from `ops/sidecar.toml`.

**Files:**
- Modify: `rust/prro_crypto/src/fiscal/config.rs`

**Acceptance Criteria:**
- [ ] `SidecarConfig::from_toml_file(path) -> Result<Self>` round-trips `ops/sidecar.example.toml`
- [ ] Missing required keys (db.path, sidecar.bind, dps.prod.endpoint, dps.test.endpoint) produce a clear error
- [ ] Optional keys default sensibly (`log_level = "info"`, CA bundle = None, tsp.timeout_ms = 5000)
- [ ] Roundtrip test: parse → `to_toml_string` → parse → equal

**Verify:** `cd rust/prro_crypto && cargo test --features sidecar config`

**Steps:**

- [ ] **Step 1: Struct tree**

  ```rust
  #[derive(Debug, Clone, Deserialize, Serialize)]
  pub struct SidecarConfig {
      pub sidecar: SidecarSection,
      pub db:      DbSection,
      pub license: Option<LicenseSection>,
      pub dps:     DpsProfiles,
      pub tsp:     Option<TspSection>,
      #[serde(default)]
      pub dev:     DevSection,
  }
  ```

- [ ] **Step 2: Validation**

  Post-parse: require bind address parseable by `std::net::SocketAddr::from_str`, require TOML `dps.prod` present even if only `test` is used (explicit declaration prevents accidental prod-in-test-fixtures).

- [ ] **Step 3: `security.credentials_mode` field**

  ```rust
  #[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
  #[serde(rename_all = "snake_case")]
  pub enum CredentialsMode {
      #[default]
      XorSoft,   // XOR with SHA-256(valid_to + operator_name[1]) — default, cross-platform
      Plain,     // raw password in DB — opt-out for WebCheck migration / debug
  }

  #[derive(Debug, Clone, Deserialize, Serialize, Default)]
  pub struct SecuritySection {
      #[serde(default)]  // default = XorSoft
      pub credentials_mode: CredentialsMode,
  }
  ```

**Commit:** combined with 4.1.

**Phase 4 deliverable summary:** 1 commit, gRPC pool + CMS adapter + TOML config + XOR-soft credentials, all 4 sub-modules covered by unit/contract tests, all 16 DPS error codes classified.

---

## Task 4.4: `src/fiscal/credentials.rs` — XOR-soft + plain backends

**Goal:** Obfuscate JKS passwords in SQLite so they are not visible as plain text. No OS-specific services or machine binding. Cross-platform, zero new crate deps (sha2 added in Task 0.1).

**Files:**
- Create: `rust/prro_crypto/src/fiscal/credentials.rs`

**Algorithm (XorSoft mode):**
```
salt   = format!("{}{}", operator.cert_valid_to, operator.operator_name.chars().nth(1).unwrap_or('?'))
key    = sha2::Sha256::digest(salt.as_bytes())   // [u8; 32]
stored = hex::encode( password_bytes XOR key[i % 32] )
```
Decode = same XOR with same key. Deterministic, no stored salt needed.

**Plain mode:** raw password string, no transformation. Used when migrating from WebCheck or for local debug (`credentials_mode = "plain"` in sidecar.toml).

**Acceptance Criteria:**
- [ ] `encode_password(password, valid_to, name) -> String` produces hex string ≠ password
- [ ] `decode_password(hex, valid_to, name) -> Result<String>` round-trips correctly
- [ ] `PlainStore::store` writes plaintext; `PlainStore::fetch` reads plaintext unchanged
- [ ] `XorSoftStore::store` writes hex-encoded XOR; `XorSoftStore::fetch` decodes back to plaintext
- [ ] `credential_store_for(mode) -> Box<dyn CredentialStore>` dispatches correctly
- [ ] Unit test: `roundtrip_xor_soft` — encode then decode → original password for ASCII and Unicode passwords
- [ ] Unit test: `xor_different_names_different_ciphertext` — same password, different name[1] → different stored value

**Verify:**
```
cd rust/prro_crypto && cargo test --features sidecar credentials
```

**Steps:**

- [ ] **Step 1: Core functions**

  ```rust
  use sha2::{Sha256, Digest};

  fn derive_key(valid_to: &str, operator_name: &str) -> [u8; 32] {
      let c = operator_name.chars().nth(1).unwrap_or('?');
      let mut salt = String::with_capacity(valid_to.len() + 4);
      salt.push_str(valid_to);
      salt.push(c);
      Sha256::digest(salt.as_bytes()).into()
  }

  pub fn encode_password(password: &str, valid_to: &str, operator_name: &str) -> String {
      let key = derive_key(valid_to, operator_name);
      let encoded: Vec<u8> = password.as_bytes()
          .iter().enumerate()
          .map(|(i, b)| b ^ key[i % 32])
          .collect();
      hex::encode(encoded)
  }

  pub fn decode_password(hex_str: &str, valid_to: &str, operator_name: &str) -> Result<String, CredError> {
      let bytes = hex::decode(hex_str).map_err(|_| CredError::Corrupted)?;
      let key = derive_key(valid_to, operator_name);
      let decoded: Vec<u8> = bytes.iter().enumerate()
          .map(|(i, b)| b ^ key[i % 32])
          .collect();
      String::from_utf8(decoded).map_err(|_| CredError::Corrupted)
  }
  ```

- [ ] **Step 2: Trait + backends**

  ```rust
  pub trait CredentialStore: Send + Sync {
      fn store(&self, operator: &Operator, plaintext: &str) -> Result<(), CredError>;
      fn fetch(&self, operator: &Operator) -> Result<String, CredError>;
  }

  pub struct PlainStore;
  impl CredentialStore for PlainStore {
      fn store(&self, op: &Operator, pw: &str) -> Result<(), CredError> { /* write pw as-is */ }
      fn fetch(&self, op: &Operator) -> Result<String, CredError> { Ok(op.jks_password.clone()) }
  }

  pub struct XorSoftStore;
  impl CredentialStore for XorSoftStore {
      fn store(&self, op: &Operator, pw: &str) -> Result<(), CredError> {
          let vt = op.cert_valid_to.as_deref().unwrap_or("1970-01-01");
          let name = op.operator_name.as_deref().unwrap_or("?");
          let encoded = encode_password(pw, vt, name);
          /* UPDATE sidecar_operators SET jks_password = encoded WHERE id = op.id */
      }
      fn fetch(&self, op: &Operator) -> Result<String, CredError> {
          let vt = op.cert_valid_to.as_deref().unwrap_or("1970-01-01");
          let name = op.operator_name.as_deref().unwrap_or("?");
          decode_password(&op.jks_password, vt, name)
      }
  }
  ```

- [ ] **Step 3: Factory**

  ```rust
  pub fn credential_store_for(mode: CredentialsMode) -> Box<dyn CredentialStore> {
      match mode {
          CredentialsMode::XorSoft => Box::new(XorSoftStore),
          CredentialsMode::Plain   => Box::new(PlainStore),
      }
  }
  ```

- [ ] **Step 4: Wire into `prro_admin add_operator` (Task 5.3) and `prro_sidecar` startup (Task 5.1)**

  Both read `sidecar.security.credentials_mode` from TOML → call `credential_store_for(mode)` → use `store()` / `fetch()` instead of touching `jks_password` directly.

**Commit:** combined with 4.1 — `feat(sidecar): XOR-soft credential obfuscation`.

---

# Phase 5 — `prro_sidecar` + `prro_admin` + preflight

## Task 5.1: `prro_sidecar` startup

**Goal:** Boot sequence: load TOML → open SQLite → load active operator JKS for each licensed FN → verify license → warm up both gRPC channels → bind HTTP port.

**Files:**
- Modify: `rust/prro_crypto/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `prro_sidecar --config ops/sidecar.toml` starts and logs structured JSON
- [ ] Startup order is deterministic and documented
- [ ] License missing → DEMO mode engaged with `max_payment_sum_kopecks=50000` and `offline_enabled=0` forced
- [ ] License `Expired` → exit code 101 (loud startup failure)
- [ ] License `Grace` → start + add `license_grace_days_left` warning to every response
- [ ] JKS load failure for an active operator → log `credentials_missing` + refuse requests for that FN (other FNs still work)
- [ ] Graceful shutdown on SIGTERM: finish in-flight requests within `max(request_timeout_ms) + 10 000 ms` (≥ 100 s for default config), then drop gRPC channels — invariant (9)

**Verify:**
```
cd rust/prro_crypto && cargo build --release --features sidecar --bin prro_sidecar
ls target/release/prro_sidecar
```

Dev smoke test (uses `scripts/run_sidecar_dev.py` from Task 5.5):
```
rtk proxy python scripts/run_sidecar_dev.py
```

**Steps:**

- [ ] **Step 1: main() sequence**

  ```
  1. init tracing-subscriber (JSON to stderr)
  2. parse --config path (clap)
  3. SidecarConfig::from_toml_file(path)
  4. Repo::open(config.db.path)
  5. StoredLicense::load_or_demo(&repo)
  6. `license::verify_signature_only(payload_b64, sig_b64, now)` → panics on `Expired`; logs `Grace` as WARN
  7. DpsGrpcPool::new(&config.dps).await → logs warmup failures but continues
  8. AppState { repo, license_state, grpc_pool, cms_config, tsp_config }
  9. axum::Router::new()
       .route("/fiscal/send",            post(send_handler))
       .route("/fiscal/cancel_last",     post(cancel_last_handler))
       .route("/fiscal/cancel_by_id",    post(cancel_by_id_handler))
       .route("/fiscal/status_rro",      post(status_rro_handler))
       .route("/fiscal/info_rro",        post(info_rro_handler))
       .route("/health",                 get(health_handler))
       .route("/readyz",                 get(readyz_handler))
       .route("/license/status",         get(license_status_handler))
       .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))  // S5: 256 KB max body
       .with_state(Arc::new(state));
  10. let shutdown_timeout = Duration::from_millis(config.dps.max_request_timeout_ms() + 10_000);
      axum::serve(listener, app)
          .with_graceful_shutdown(shutdown_signal(shutdown_timeout))
          .await;
  ```

- [ ] **Step 2: AppState**

  ```rust
  struct AppState {
      repo: Repo,
      grpc_pool: Arc<DpsGrpcPool>,
      license_state: RwLock<LicenseState>,
      operator_key_cache: DashMap<String, Arc<LoadedOperatorKey>>,  // keyed by fiscal_number
      cms_cfg: CmsSignCtx<'static>,
      demo_mode: bool,
  }
  ```

  `operator_key_cache` is populated on first request per FN. On SIGHUP: cache is cleared atomically (`DashMap::clear()`), triggering fresh JKS reload on next request — supports key rotation without restart (S6).

**Commit:** combined with Task 5.2.

---

## Task 5.2: HTTP endpoints

**Goal:** 8 endpoints — 5 fiscal + 3 meta — all returning structured JSON, all preserving invariants.

**Files:**
- Modify: `rust/prro_crypto/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `POST /fiscal/send` happy-path passes SHIFT_OPEN → SELL → Z_REPORT against the mock gRPC server
- [ ] `POST /fiscal/cancel_last` calls `delLastChk` and returns the cancelled document metadata
- [ ] `POST /fiscal/cancel_by_id` calls `delLastChkId`
- [ ] `POST /fiscal/status_rro` calls `statusRro` and normalizes the response
- [ ] `POST /fiscal/info_rro` calls `infoRro` and returns the preflight metadata
- [ ] `GET /health` returns 200 OK unconditionally (k8s liveness)
- [ ] `GET /readyz` returns 200 OK iff gRPC pool is warm AND license state is non-`Expired` AND at least one active operator is loaded
- [ ] `GET /license/status` returns `{state, tier, tin, fn_numbers, days_left}`
- [ ] Each fiscal endpoint emits an `audit_log_insert` row via `repo`
- [ ] Error responses use a structured error envelope (documented below)

**Verify:**
```
cd rust/prro_crypto && cargo test --features sidecar --test sidecar_e2e
```

**Steps:**

- [ ] **Step 1: Request / response schemas**

  `POST /fiscal/send` request body:

  ```json
  {
    "canonical": { /* full CanonicalCommand model_dump */ },
    "metadata": {
      "lnd":                42,
      "related_receipt_id": null,
      "offline_fiscal_no":  null,
      "z_number":           7
    }
  }
  ```

  Success response:

  ```json
  {
    "ok": true,
    "fiscal_id":      "200000000000123",
    "dps_status":     1,
    "payload_sha256": "<hex>",
    "signed_payload_b64": "<CMS DER>",
    "id_sign_b64":   "<server-sig>",
    "data_sign_b64": "<server-sig>",
    "warnings": ["license_grace_days_left=13"]
  }
  ```

  Error response (uniform across all endpoints):

  ```json
  {
    "ok": false,
    "error": "license_expired",
    "detail": "License expired at 2026-04-10T00:00:00Z",
    "dps_status": null
  }
  ```

  Error codes emitted by the sidecar itself (in addition to DPS `-1`…`-16`):

  | code                 | meaning                                                          | HTTP |
  |----------------------|------------------------------------------------------------------|------|
  | `license_expired`    | Hard stop (past `expires_at`)                                    | 403  |
  | `license_invalid_signature` | Master sig verify failed                                   | 403  |
  | `tin_mismatch`       | fn.tax_number ≠ license.tin                                      | 403  |
  | `fn_not_licensed`    | fn ∉ license.fn_numbers                                          | 403  |
  | `demo_limit_exceeded`| Payment > 500 UAH in demo                                        | 403  |
  | `demo_offline_denied`| GO_OFFLINE attempted in demo                                     | 403  |
  | `cert_expires_soon`  | Operator cert `valid_to < now + 14d` — warning, not error        | 200 + warnings |
  | `credentials_missing`| Active operator JKS absent or unreadable                         | 503  |
  | `fn_config_missing`  | No row in `fiscal_number_config`                                 | 404  |
  | `bad_request`        | Missing `schema_version` or malformed canonical                  | 400  |
  | `cms_sign_failed`    | CMS signing error (detail contains cause)                        | 500  |
  | `tsp_failed`         | TSP required but unreachable                                     | 502  |
  | `grpc_unavailable`   | Channel not ready; retryable                                     | 503  |
  | `dps_rejected`       | DPS status < 0 — see `dps_status` field                           | 422  |

- [ ] **Step 2: Send handler skeleton**

  ```
  1. Parse CanonicalCommand (→ 400 on schema_version missing)
  2. repo.load_fn_config(cmd.fiscal_number)? (→ 404 on None)
  3. license::verify(payload_b64, sig_b64, fn, tin, now) → LicenseState (→ 403 on disallowed)
     • Demo: check cmd.receipt.totals.total_sum_kopecks ≤ limits.max_payment_sum_kopecks PER THIS CHECK (not cumulative)
  4. repo.load_active_operator(fn) (→ 503 on None)
  5. operator_key_cache.entry(fn).or_insert_with(|| load_jks(op))
  6. xml_builder::build(&cmd, &ctx) → xml_utf8
  7. cp1251::encode_cp1251(&xml_utf8) → bytes
  8. cms_adapter::sign_content(&bytes, &key.signer, &key.cert_der, &ctx)
  9. grpc_pool.for_mode(fn.fiscal_mode).send_chk_v2(Check{…})
  10. Map response → SendResponseBody + warnings + audit_log
  ```

- [ ] **Step 3: Cancel endpoints**

  `cancel_last` + `cancel_by_id`: reuse `grpc_client.del_last_chk` / `del_last_chk_id`. No XML build, no CMS — these endpoints sign the FN (or the id), send, return.

- [ ] **Step 4: status_rro / info_rro**

  Sign FN bytes via `DstuInProcessSigner::sign`, wrap in `CheckRequest.rro_fn_sign`, call RPC, return parsed response.

**Commit:** combined with Task 5.1.

---

## Task 5.3: `prro_admin` CLI

**Goal:** Single binary for SQLite-side operator / FN management. Invoked manually by the ops team during onboarding.

**Files:**
- Modify: `rust/prro_crypto/src/bin/prro_admin.rs`

**Acceptance Criteria:**
- [ ] Subcommand `register_fn` inserts / upserts a `fiscal_number_config` row with all Phase-0 columns
- [ ] Subcommand `add_operator` opens the JKS with the supplied password, extracts the cert, upserts into `operator_certs(source='container')` AND inserts into `sidecar_operators`
- [ ] Subcommand `list_operators --fn FN` prints a table
- [ ] Subcommand `deactivate_operator --id N` flips `active=0`; `reactivate_operator --id N` flips back
- [ ] Subcommand `install_license --license-file license.json` parses + verifies the file + INSERTs into `licenses` with `active=1`; marks all prior active rows `active=0`
- [ ] Each subcommand prints a short `OK: …` line on success and non-zero exit on failure

**Verify:**
```
cd rust/prro_crypto && cargo run --features sidecar --bin prro_admin -- --help
cargo run --features sidecar --bin prro_admin -- register_fn --help
```

**Steps:**

- [ ] **Step 1: clap top-level**

  ```rust
  #[derive(clap::Parser)]
  #[command(name = "prro_admin", version)]
  struct Cli {
      #[clap(long, default_value = "var/prro.db")]
      db: std::path::PathBuf,

      #[clap(subcommand)]
      cmd: Cmd,
  }

  #[derive(clap::Subcommand)]
  enum Cmd {
      RegisterFn {
          #[clap(long)] fiscal_number: String,
          #[clap(long)] tax_number:    String,
          #[clap(long, default_value = "test")] fiscal_mode: String,
          #[clap(long, default_value_t = false)] national_check: bool,
          #[clap(long, default_value_t = true)]  offline: bool,
          #[clap(long, default_value_t = false)] tsp: bool,
          #[clap(long)] org_name:    Option<String>,
          #[clap(long)] org_address: Option<String>,
      },
      AddOperator {
          #[clap(long)] fn_: String,
          #[clap(long)] jks_path: std::path::PathBuf,
          /// Read password from a file path or "-" for stdin (never passed as raw arg — avoids ps aux exposure).
          #[clap(long)] jks_password_file: Option<std::path::PathBuf>,
          #[clap(long)] operator_inn:  String,
          #[clap(long)] operator_name: Option<String>,
      },
      ListOperators     { #[clap(long)] fn_: String },
      DeactivateOperator{ #[clap(long)] id: i64 },
      ReactivateOperator{ #[clap(long)] id: i64 },
      InstallLicense    { #[clap(long)] license_file: std::path::PathBuf },
  }
  ```

- [ ] **Step 2: add_operator flow**

  1. Resolve password: if `--jks-password-file -` → read from stdin (trim newline); else read from file path. Error if neither provided.
  2. Read `jks_path` bytes.
  3. `prro_crypto::interop::prro::load_signing_key(bytes, password)?` → `LoadedKey`.
  4. Compute cert fingerprint (SHA-256 over DER) + SKI (from cert). Extract `valid_to`.
  5. Encode password via `credential_store_for(mode).store(...)`.
  6. Open repo, `BEGIN`; upsert `operator_certs`; insert `sidecar_operators`; `COMMIT`.
  7. Print `OK: operator id=42 for FN=3001234567`.

- [ ] **Step 3: install_license flow**

  1. Read file, parse `{payload_b64, signature_b64}`.
  2. Call `license::verify_signature_only(payload_b64, sig_b64, now)` — signature + expiry only; TIN/FN binding happens per-request in the sidecar.
  3. `UPDATE licenses SET active = 0;` then `INSERT`.

**Commit:**

```
feat(sidecar): prro_admin CLI + prro_sidecar binary + preflight tool

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 5.4: `prro_sidecar_preflight`

**Goal:** Standalone diagnostic binary for onboarding and support. No HTTP, no DB — just "load JKS, call `infoRro`, print metadata".

**Files:**
- Modify: `rust/prro_crypto/src/bin/prro_sidecar_preflight.rs`

**Acceptance Criteria:**
- [ ] `prro_sidecar_preflight --jks-path key.jks --jks-password ... --fn 3001234567 --mode test` prints the `RroInfoResponse` fields as JSON
- [ ] Works without the sidecar TOML — takes the test/prod endpoint from `--mode` + optional `--endpoint` override
- [ ] Exit 0 on OK, 1 on signed-but-status<0, 2 on transport error
- [ ] Documents the 16 DPS error codes in `--help`

**Verify:**
```
cd rust/prro_crypto && cargo run --features sidecar --bin prro_sidecar_preflight -- --help
```

**Steps:**

- [ ] **Step 1: Wiring**

  Reuse `grpc_client::DpsGrpcPool` with only one mode active, and `load_signing_key` for JKS; sign FN bytes, call `info_rro`, print.

- [ ] **Step 2: Output format**

  ```json
  {
    "status":           1,
    "status_rro":       1,
    "open_shift":       true,
    "online":           true,
    "last_signer":      "АВТОР.ПОЛТАВА ТОВ...",
    "name":             "ТОВ МІЙ МАГАЗИН",
    "addr":             "Україна, м.Полтава, вул.Леніна, 1",
    "tins":             "1234567890",
    "operators":        [{"isname":"...","senior":true,"serial":"..."}],
    "offline_allowed":  true,
    "pn":               "ПРРО 1",
    "lnum":             1423
  }
  ```

**Commit:** combined with 5.3.

---

## Task 5.5: E2E integration test — mock gRPC server

**Goal:** One test binary that spawns an in-process mock DPS, starts the sidecar, issues SHIFT_OPEN → SELL → Z_REPORT via HTTP, and asserts the full round-trip.

**Files:**
- Create: `rust/prro_crypto/tests/sidecar_e2e.rs`
- Create: `rust/prro_crypto/tests/grpc_contract.rs`
- Create: `scripts/run_sidecar_dev.py` (developer convenience)

**Acceptance Criteria:**
- [ ] `grpc_contract.rs` passes: one test per RPC method × (OK, negative-status) → 16 tests
- [ ] `sidecar_e2e.rs` passes: spawns sidecar + mock gRPC, posts 3 canonical commands sequentially, asserts responses
- [ ] Test DB is `rusqlite::Connection::open_in_memory()`-backed via a `?mode=memory` URI OR a fresh file under `/tmp`
- [ ] Total runtime < 20 s

**Verify:**
```
cd rust/prro_crypto && cargo test --features sidecar --test sidecar_e2e --test grpc_contract
```

**Steps:**

- [ ] **Step 1: Mock gRPC server**

  `tonic::transport::Server::builder().add_service(InMemoryChkIncomeService::new()).serve(addr)`. `InMemoryChkIncomeService` stores inbound requests, returns pre-programmed responses.

- [ ] **Step 2: Spawn the sidecar**

  Use `std::process::Command` to launch `target/debug/prro_sidecar` with a test TOML that points `dps.test.endpoint` at the mock server address.

- [ ] **Step 3: HTTP assertions**

  Use `reqwest::blocking::Client` from the test.

**Commit:**

```
test(sidecar): E2E and gRPC contract tests (mock DPS server)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Phase 5 deliverable summary:** 3 commits, binary fully wired, 8 HTTP endpoints, 4 CLIs, 16 contract tests + E2E test green.

---

# Phase 6 — Python integration

## Task 6.1: `TransportKind.DPS_PRRO_FISCAL_SIDECAR_V2`

**Goal:** New transport kind. Leave existing `DPS_PRRO_GRPC_ECABINET` alone for now.

**Files:**
- Modify: `src/prro_gateway/enums.py` (line ~137)

**Acceptance Criteria:**
- [ ] `TransportKind.DPS_PRRO_FISCAL_SIDECAR_V2 = "DPS_PRRO_FISCAL_SIDECAR_V2"` exists
- [ ] Existing tests referencing `TransportKind` still pass

**Verify:** `rtk proxy pytest tests/ -k enums`

**Steps:**

- [ ] **Step 1: Add enum value**

  ```python
  class TransportKind(StrEnum):
      CHECKBOX_REST_TRANSPORT = "CHECKBOX_REST_TRANSPORT"
      DPS_PRRO_GRPC_ECABINET = "DPS_PRRO_GRPC_ECABINET"
      DPS_PRRO_XML_UNIFIED_WINDOW = "DPS_PRRO_XML_UNIFIED_WINDOW"
      CUSTOM_TRANSPORT = "CUSTOM_TRANSPORT"
      DPS_PRRO_FISCAL_SIDECAR_V2 = "DPS_PRRO_FISCAL_SIDECAR_V2"
  ```

**Commit:** combined with 6.2–6.4.

---

## Task 6.2: `transports/fiscal_sidecar.py`

**Goal:** Thin httpx client that posts the canonical JSON + metadata to `/fiscal/send`, maps the response to `SendResult`, and classifies sidecar errors.

**Files:**
- Create: `src/prro_gateway/transports/fiscal_sidecar.py`

**Acceptance Criteria:**
- [ ] `FiscalSidecarTransport(sidecar_url, http_client=None)` constructor
- [ ] `send(document_id, signed_payload, fiscal_number, ...)` — since the sidecar now builds the XML itself, we pass `canonical_payload` + `business_ts` + `lnd` + `related_receipt_id` + `offline_fiscal_no` + `z_number`. `signed_payload` from write_path is the canonical JSON (`model_dump_json`).
- [ ] OK (`status == 1`) → `SendResult(state=ACK, transport_request_id=fiscal_id, submission_status="DPS_ACK", server_response=data)`
- [ ] `dps_status < 0` → `TransportRejectedError` with the mapped error message
- [ ] `license_*`, `tin_mismatch`, `fn_not_licensed`, `demo_*` → `TransportRejectedError` (not retryable)
- [ ] `grpc_unavailable`, `tsp_failed`, `credentials_missing` → `TransportRetryableError`
- [ ] Warnings from response (`license_grace_days_left`, `cert_expires_soon`) logged at WARN and attached to `SendResult.warnings`
- [ ] `poll_status` calls `/fiscal/cancel_last` style recovery? No — stays `NotImplementedError` unless we extend write_path to consume `lastChk`

**Verify:** `rtk proxy pytest tests/test_fiscal_sidecar_v2_transport.py -v`

**Steps:**

- [ ] **Step 1: skeleton**

  ```python
  class FiscalSidecarTransport:
      def __init__(self, *, sidecar_url: str, http_client=None):
          self._base = sidecar_url.rstrip("/")
          self._client = http_client or httpx.Client(timeout=120.0)

      def send(self, *, document_id, signed_payload, fiscal_number, operation_type, **kw):
          # signed_payload here = canonical JSON bytes (write_path uses passthrough)
          if isinstance(signed_payload, (bytes, bytearray)):
              canonical_json = json.loads(signed_payload)
          else:
              canonical_json = json.loads(signed_payload) if isinstance(signed_payload, str) else signed_payload

          body = {
              "canonical": canonical_json,
              "metadata": {
                  "lnd":                kw.get("lnd", 0),
                  "related_receipt_id": kw.get("related_receipt_id") or "",
                  "offline_fiscal_no":  kw.get("offline_fiscal_no") or "",
                  "z_number":           kw.get("z_number", 0),
              },
          }
          resp = self._client.post(f"{self._base}/fiscal/send", json=body)
          self._raise_for_error(resp)
          data = resp.json()
          return self._map_success(data)
  ```

- [ ] **Step 2: Error mapping table**

  ```python
  _RETRYABLE_CODES = {"grpc_unavailable", "tsp_failed", "credentials_missing"}
  _NON_RETRYABLE_CODES = {
      "license_expired", "license_invalid_signature", "tin_mismatch",
      "fn_not_licensed", "demo_limit_exceeded", "demo_offline_denied",
      "fn_config_missing", "bad_request", "cms_sign_failed", "dps_rejected",
  }
  ```

**Commit:** combined with 6.3–6.4.

---

## Task 6.3: `runtime/container.py` wiring

**Goal:** Register `FiscalSidecarTransport` behind the new `TransportKind`.

**Files:**
- Modify: `src/prro_gateway/runtime/container.py`
- Modify: `src/prro_gateway/transports/__init__.py`

**Acceptance Criteria:**
- [ ] Config YAML with `transport_kind: DPS_PRRO_FISCAL_SIDECAR_V2` + `endpoint: http://127.0.0.1:8090` wires to `FiscalSidecarTransport`
- [ ] Crypto provider must be `passthrough` (write_path emits canonical JSON as signed_payload — actual sign happens in the Rust sidecar). Container raises if combined with `sidecar` provider (double signing).

**Verify:** `rtk proxy pytest tests/test_container_wiring.py` (if exists) and `rtk proxy pytest tests/test_fiscal_sidecar_v2_transport.py`

**Steps:**

- [ ] **Step 1: Export**

  Append to `src/prro_gateway/transports/__init__.py`:
  ```python
  from .fiscal_sidecar import FiscalSidecarTransport
  ```
  Add to `__all__`.

- [ ] **Step 2: Handler table**

  In `container.py` wherever `TransportKind.CHECKBOX_REST_TRANSPORT` is mapped:

  ```python
  TransportKind.DPS_PRRO_FISCAL_SIDECAR_V2: FiscalSidecarTransport(
      sidecar_url=getattr(config.crypto, 'sidecar_url', 'http://127.0.0.1:8090'),
      http_client=self.transport_http_client,
  ),
  ```

- [ ] **Step 3: Guard**

  When the YAML profile selects the sidecar transport, assert `crypto.provider == "passthrough"`. Raise a config error with a pointer to the plan doc otherwise.

**Commit:** combined with 6.2 / 6.4.

---

## Task 6.4: `tests/test_fiscal_sidecar_v2_transport.py`

**Goal:** Mock-based contract tests.

**Files:**
- Create: `tests/test_fiscal_sidecar_v2_transport.py`

**Acceptance Criteria:**
- [ ] 6 tests: `send_ok`, `send_dps_rejected`, `send_license_expired`, `send_grpc_unavailable_retries`, `send_demo_limit_exceeded`, `request_body_shape_matches_spec`
- [ ] All tests use `unittest.mock` + `httpx.Client` seam — no real network
- [ ] `rtk proxy pytest tests/test_fiscal_sidecar_v2_transport.py -v` green

**Verify:** `rtk proxy pytest tests/test_fiscal_sidecar_v2_transport.py -v`

**Steps:**

- [ ] **Step 1: Template** — pattern-match the existing `tests/test_fiscal_sidecar_transport.py` from the earlier plan (Task 3 in `2026-04-17-rust-fiscal-driver.md`).

**Commit:**

```
feat(transport): FiscalSidecarTransport + DPS_PRRO_FISCAL_SIDECAR_V2 kind

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 6.5: Remove `DpsFiscalServerTransport` + proto stubs (post-validation)

**Goal:** Drop the old transport once the new path is proven against a real FN under pilot conditions. Held until user approves, informed by WebCheck DB diff.

**Files:**
- Delete: `src/prro_gateway/transports/dps_fiscal_server.py`
- Delete: `src/prro_gateway/transports/proto/fiscal_server.proto`
- Delete: `src/prro_gateway/transports/proto/fiscal_server_pb2.py`
- Delete: `src/prro_gateway/transports/proto/fiscal_server_pb2_grpc.py`
- Modify: `src/prro_gateway/transports/__init__.py`
- Modify: `src/prro_gateway/runtime/container.py` (remove handler entry)
- Modify: any test that still imports `DpsFiscalServerTransport`

**Acceptance Criteria:**
- [ ] `rtk proxy pytest tests/` green
- [ ] `rg DpsFiscalServerTransport src/ tests/` returns nothing
- [ ] `rg fiscal_server_pb2 src/ tests/` returns nothing
- [ ] Node.js signing sidecar package references removed from `package.json` / Dockerfiles

**Verify:** `rtk proxy pytest tests/`

**Steps:**

- [ ] **Step 1:** Confirm with user that pilot validation has passed (cross-check against WebCheck DB). **Gate:** only proceed after user says "go".
- [ ] **Step 2:** Delete files, update imports, run tests.
- [ ] **Step 3:** Remove the Node.js jkurwa sidecar from Docker Compose and package manifests.

**Commit:**

```
chore(transport): remove legacy DpsFiscalServerTransport after v2 pilot validation

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Phase 6 deliverable summary:** 1 commit (6.1–6.4), plus 1 deferred commit (6.5) post-pilot.

---

# Phase 7 — Documentation + cleanup

## Task 7.1: Update ADR-004 + README + CHANGELOG

**Goal:** Mark ADR-004 v1 as superseded; record v2 status; surface build/run commands.

**Files:**
- Modify: `docs/ADR-004-rust-fiscal-driver.md`
- Modify: `README.md`
- Modify: `rust/prro_crypto/README.md`
- Modify: `CHANGELOG.md`

**Acceptance Criteria:**
- [ ] ADR-004 has a header block: "Superseded-by: docs/superpowers/plans/2026-04-19-rust-fiscal-driver-v2.md; status: implemented <date>"
- [ ] README.md has a "Rust sidecar (prro_sidecar)" section with build + run + smoke test
- [ ] CHANGELOG.md gets one entry per committed phase
- [ ] All doc links resolve (`rg -n '\]\(.*\.md\)' docs/ | xargs -I{} test -f {}`)

**Verify:** Manual review + link check.

**Steps:**

- [ ] **Step 1:** Edit ADR-004 top metadata block.
- [ ] **Step 2:** Add README.md quickstart:

  ```bash
  cd rust/prro_crypto && cargo build --release --features sidecar
  cp target/release/prro_sidecar /opt/prro_sidecar
  ./target/release/prro_admin --db var/prro.db register_fn --fiscal-number 3001234567 --tax-number 1234567890 --fiscal-mode test
  ./target/release/prro_admin --db var/prro.db add_operator --fn 3001234567 --jks-path keys/op1.jks --jks-password-file keys/op1.pass --operator-inn 1111111111
  ./target/release/prro_sidecar_preflight --jks-path keys/op1.jks --jks-password-file keys/op1.pass --fn 3001234567 --mode test
  ./target/release/prro_admin --db var/prro.db install_license --license-file ops/license.json
  ./target/release/prro_sidecar --config ops/sidecar.toml
  ```

**Commit:** single doc commit.

---

## Task 7.2: Remove Node.js jkurwa sidecar (if not done in 6.5)

**Goal:** Eliminate the old signing sidecar entirely.

**Files:**
- Delete: `rust/prro_crypto/package.json` (if it exists and is jkurwa bootstrap)
- Delete: `rust/prro_crypto/package-lock.json`
- Delete: `rust/prro_crypto/node_modules/` (already in `.gitignore`)
- Modify: `docker-compose.yml` (remove node service)
- Modify: `Dockerfile` (remove node install steps)

**Acceptance Criteria:**
- [ ] `docker compose up` passes smoke without the node service
- [ ] No references to "jkurwa" or "node_modules" in CI manifests

**Verify:** `docker compose up --build` local smoke.

**Steps:**

- [ ] **Step 1:** Audit `docker-compose.yml`.
- [ ] **Step 2:** Audit Dockerfile.
- [ ] **Step 3:** Confirm no runtime code still imports the jkurwa sidecar.

**Commit:**

```
chore(ops): remove Node.js jkurwa signing sidecar — replaced by prro_sidecar

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Task 7.3: Document sidecar.toml options

**Goal:** `ops/sidecar.example.toml` stays in sync with the actual `SidecarConfig` struct.

**Files:**
- Modify: `rust/prro_crypto/ops/sidecar.example.toml`
- Create: `docs/sidecar_configuration.md`

**Acceptance Criteria:**
- [ ] Every field in `SidecarConfig` has a matching inline comment in `sidecar.example.toml`
- [ ] `docs/sidecar_configuration.md` cross-links to PER_FN_CONFIG, ADR-004 v2, and the plan doc

**Verify:** Manual review.

**Steps:**

- [ ] **Step 1:** Enumerate Rust struct fields via `grep -R "pub .*: " src/fiscal/config.rs` and ensure each is documented in TOML.

**Commit:** combined with 7.1.

**Phase 7 deliverable summary:** 1 commit (docs + cleanup).

---

# Cross-phase specifications

## A. License payload JSON schema (v1, frozen)

```json
{
  "schema_version": "license.v1",
  "tin":        "1234567890",
  "fn_numbers": ["3001234567", "3001234568"],
  "issued_at":  "2026-04-19T00:00:00Z",
  "expires_at": "2027-04-19T00:00:00Z",
  "tier":       "pro",
  "org_name":   "ТОВ Мій Магазин",
  "demo_limits": null,
  "issuer":     "PRRO_GATE_ADMIN"
}
```

Demo variant:

```json
{
  "schema_version": "license.v1",
  "tin":        "0000000000",
  "fn_numbers": [],
  "issued_at":  "2026-04-19T00:00:00Z",
  "expires_at": "9999-12-31T23:59:59Z",
  "tier":       "demo",
  "org_name":   null,
  "demo_limits": {
    "max_payment_sum_kopecks": 50000,
    "no_offline":               true,
    "require_dps_online":       true
  },
  "issuer": "PRRO_GATE_ADMIN"
}
```

## B. sidecar.toml example (canonical form)

See Task 0.6 Step 1. All fields documented in `docs/sidecar_configuration.md` (Task 7.3).

## C. HTTP endpoint response schemas

### `POST /fiscal/send`

- Success 200: `{ ok: true, fiscal_id, dps_status, payload_sha256, signed_payload_b64, id_sign_b64, data_sign_b64, warnings[] }`
- Error 400 / 403 / 404 / 422 / 500 / 502 / 503: `{ ok: false, error, detail, dps_status? }`

### `POST /fiscal/cancel_last`

- Request: `{ fiscal_number, mode: "prod"|"test" }`
- Response: `{ ok, cancelled_fiscal_id, dps_status, error?, detail? }`

### `POST /fiscal/cancel_by_id`

- Request: `{ id: "...", mode: "prod"|"test" }`
- Response: same shape as cancel_last.

### `POST /fiscal/status_rro`

- Request: `{ fiscal_number, mode }`
- Response: `{ ok, open_shift, online, last_signer, status, error_message }`

### `POST /fiscal/info_rro`

- Request: `{ fiscal_number, mode }`
- Response: full `RroInfoResponse` fields as JSON.

### `GET /health`

- 200 OK `{ ok: true, version }` unconditionally.

### `GET /readyz`

- 200 OK `{ ok: true, checks: { license, grpc_prod, grpc_test, operator_cache } }` iff all checks pass.
- 503 otherwise with the failing sub-checks annotated.

### `GET /license/status`

- Always 200. Body: `{ state: "Valid"|"Grace"|"Expired"|"Demo"|"SignatureInvalid", tier, tin, fn_numbers, issued_at, expires_at, days_left? }`.

## D. Error code consolidation

| error (JSON code)       | HTTP | Retryable | Notes                                      |
|-------------------------|------|-----------|--------------------------------------------|
| bad_request             | 400  | no        | JSON parse / schema_version missing        |
| license_expired         | 403  | no        | past expires_at                            |
| license_invalid_signature | 403 | no      | master sig verify failed                   |
| tin_mismatch            | 403  | no        | fn.tax_number ≠ license.tin                |
| fn_not_licensed         | 403  | no        | fn ∉ fn_numbers                            |
| demo_limit_exceeded     | 403  | no        | sum > 500 UAH in demo tier                 |
| demo_offline_denied     | 403  | no        | offline attempted in demo tier             |
| fn_config_missing       | 404  | no        | no row in fiscal_number_config             |
| dps_rejected            | 422  | no        | DPS status -1..-16, dps_status field set   |
| cms_sign_failed         | 500  | no        | DSTU sign failure                          |
| tsp_failed              | 502  | partially | required TSP unreachable                   |
| grpc_unavailable        | 503  | yes       | channel warm-up / reconnect cycle          |
| credentials_missing     | 503  | yes       | JKS path unreadable, race with admin CLI   |
| cert_expires_soon       | 200  | —         | warning only, attached to response         |

## E. Golden test scenarios (30)

Listed in Task 2.6 Step 1. Each has a deterministic input, a canonical expected cp1251 hex string, and an expected behavior under a specified FN config.

## F. Invariant check matrix

| Invariant                                        | Touched by                                | Preservation mechanism                                                        |
|--------------------------------------------------|-------------------------------------------|-------------------------------------------------------------------------------|
| (1) No network/crypto in SQLite transactions     | Phase 1.2 (repo), Phase 5 (send handler)  | Each repo method is a single short-lived statement; audit_log commits after response, not inside |
| (2) One fiscal_number = one writer               | Phase 5 send handler                      | Sidecar does not add new locking — defers to Python write_path single-writer lease |
| (4) Idempotency                                  | Phase 6.2 transport                       | `idempotency_key` is part of canonical; sidecar ignores duplicates at DPS level (DPS dedupes by lnd) |
| (6) Full canonical payloads in adapters          | Phase 1.1 input                           | `#[serde(flatten)]` on CanonicalCommand captures unknown fields               |
| (7) schema_version always present                | Phase 1.1 input                           | Required field; serde missing-field error → 400                               |
| (8) Recovery does not break state transitions    | Phase 6.2 transport                       | poll_status remains NotImplementedError; recovery stays Python-side           |
| (9) Graceful shutdown                            | Phase 5.1 startup                         | axum `with_graceful_shutdown` on SIGTERM; drains within 30 s                  |
| (10) No accidental unsigned path                 | Phase 5.1 startup + AppState              | `dev.skip_sign` only honored when `DEBUG_INSECURE_MODE=1` env is set          |

---

# Rollback plan

Every phase is a single commit (or a small group of commits on one branch). Rollback is `git revert` per phase:

| Phase failing | Rollback step                                                        | Data impact           |
|---------------|----------------------------------------------------------------------|------------------------|
| 0             | `git revert` → regenerate Cargo.lock                                 | None                   |
| 1             | `git revert`                                                          | None                   |
| 2             | `git revert` golden commits                                          | None                   |
| 3             | `git revert`                                                          | None                   |
| 4             | `git revert`                                                          | None                   |
| 5             | `git revert` + redeploy old Python + Node.js sidecars                | None (state in SQLite) |
| 6             | Switch YAML profile back to `DPS_PRRO_GRPC_ECABINET` + `git revert`  | None                   |
| 6.5           | `git revert` — old transport reinstated                              | None                   |
| 7             | `git revert` — docs only                                             | None                   |

`sql/017_sidecar_ops_and_fn_business.sql` is **not automatically rolled back**. The added columns are non-destructive (defaults safe for old code). A separate `sql/018_rollback_017.sql` should only be written if forced — per project policy, we avoid down-migrations.

---

# Open questions (non-blocking for plan approval)

1. ~~**TSP URL selection**~~ — **RESOLVED 2026-04-19:** use existing `ca_endpoints.tsp_url` + `issuer_pattern` automapping (migration 016 is already in production with 9 Ukrainian CAs seeded). No new column. See Task 4.2 acceptance criteria for the SQL query. If per-FN override ever needed → future migration adds `fiscal_number_config.tsp_url_override TEXT NULL`.

2. **Demo-tier FN allocation** — in DEMO mode there is no license-side `fn_numbers` list. Do we scope demo to exactly one FN at install time, or allow any FN that has a `fiscal_number_config` row? Proposal: **any FN that has been `register_fn`-ed** — keeps demo frictionless.

3. **`prro_admin` multi-operator atomicity** — `add_operator` writes to two tables (`operator_certs` + `sidecar_operators`). If we keep these in one transaction, we respect invariant (1) because it's a single short transaction and no network call is inside. Confirmed OK.

4. **Reload on license install** — `prro_admin install_license` updates `licenses`, but a running `prro_sidecar` already cached the old state. Mitigation: (a) Admin CLI notifies via SIGHUP, (b) sidecar re-reads `/license/status` on every request. Proposal: **(a) SIGHUP handler** — cheap, explicit.

5. **`prro_crypto` version bump** — the crate goes from v0.1.0-alpha to at least 0.2.0 (new public module). Follow-up: confirm whether `cargo publish` is in scope (probably not for v1).

6. ~~**Credentials storage encryption**~~ — **RESOLVED 2026-04-19:** `jks_password` stored plaintext by default (as WebCheck `OPERATORS.KEYPASS`); defense via file permissions. Optional `security.credentials_mode = "dpapi" | "keyring"` feature flag for enterprise users — implemented via per-target-os feature gates in Cargo (see Task 0.1). Trait `CredentialStore` in `src/fiscal/credentials.rs` with three impls: `PlainStore`, `DpapiStore` (Windows), `KeyringStore` (Linux). Admin CLI reads mode from `sidecar.toml` and uses appropriate backend.

---

# Summary

- **Total effort:** ~8.5 working days, 13 commits.
- **Hot zones touched:** `transports/`, `rust/prro_crypto/`, migrations (new 017), runtime container, crypto path (CMS adapter), adapters/XML builder.
- **New surface:** 6 Rust binaries, 1 SQLite migration, 1 Python transport, 8 HTTP endpoints, 1 JSON license schema.
- **Deletions (deferred post-pilot):** `dps_fiscal_server.py` + proto stubs + Node.js jkurwa sidecar.
- **Invariants audit:** all 10 preserved (see matrix in Cross-phase spec F).
- **Verification gate:** all four test layers green — unit, golden, contract, E2E — before Phase 6.5 removal is attempted.
