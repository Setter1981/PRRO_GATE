# PRRO Gateway — Full Rust Rewrite Design

**Status:** Draft (brainstorm complete, awaiting user sign-off)
**Date:** 2026-04-22
**Owner:** Setter1981 + pair-AI
**Target:** Single Rust binary on retail (Windows + Linux), full feature parity with current Python gateway, freeze Python after cutover.

---

## 1. Executive summary

Replace the current Python `prro_gateway` (≈ 25K LoC, 1400+ tests) with a single statically-linked Rust binary `prro` that runs all ingress protocols, write-path, transports, admin UI, and rendering in one process. Keep existing Rust crates (`prro_crypto`, `prro_escpos`, `maria304_driver`, `prro_escpos_daemon`) and absorb their logic into the new gateway as library crates plus a permanent universal print daemon.

Rewrite strategy: **big-bang on a side branch** (`rust-gateway`), full feature parity (MVP-C), **clean redesign of SQLite schema** (no production data exists yet), **hybrid testing** (byte-equivalence for hot zones, fresh Rust tests for HTTP/CRUD), **target 7-8 months to cutover**.

---

## 2. Decisions

| # | Topic | Decision |
|---|---|---|
| 1 | Cutover strategy | **C** — big-bang rewrite on side branch, freeze Python at cutover |
| 2 | MVP scope | **MVP-C** — full feature parity, no compromise |
| 3 | Testing strategy | **C** — hybrid: byte-equivalence golden tests for hot zones + fresh Rust tests for HTTP / repos / CRUD |
| 4 | Schema | **B** — clean redesign from migration 001, keep old schema only for one-shot data import (no production data exists yet) |
| 5 | DB driver | **sqlx** (async + compile-time SQL check) over rusqlite |
| 6 | Binary shape | **A** — single monolithic `prro` with subcommands; reuse existing crates as libraries |
| 7 | Single-writer enforcement | **C** — in-process DashMap+mpsc per fiscal_number + DB-level CAS UPDATE + PID file lock |
| 8 | Templates | **Askama** (compile-time, type-safe) |
| 9 | Doctor command | Yes (`prro doctor` for config/perm/network checks) |
| 10 | Old Python sidecar / escpos_daemon | Run in parallel until cutover; `prro_escpos_daemon` stays permanently as universal print service |
| 11 | UI session store | tower-sessions backed by SQLite (persist across restarts) |
| 12 | DejaVu fonts | Embed in binary |
| 13 | CSRF | Custom 50-LoC helper, no third-party crate |
| 14 | PDF engine | **typst** as embedded library (LaTeX-quality output, +30MB binary OK) |
| 15 | Static assets (CSS/JS) | Inline in `_base.html.j2` Askama template |
| 16 | Cred-sealing for JKS passwords | Default-on `xor_soft` with random per-row salt; `plain` only for dev profile |
| 17 | CMP cert fetch | Active capability (Key-6.dat support) |
| 18 | TSP (RFC 3161) | Capability available, per-FN config flag |
| 19 | DPS endpoint | Configurable per-environment in TOML |
| 20 | Mock DPS | Native Rust tonic mock server (port from Python) |
| 21 | "Єдине вікно" channel | Architectural slot via `DpsChannel` trait — future variant |
| 22 | Test mode | `fiscal_mode` per FN + `test_mode` per transport_profile + cross-check at startup |
| 23 | LAN access | Admin UI binds to LAN with CORS allowlist |
| 24 | Per-FN rate limit | 60 req/min default, configurable in TOML |
| 25 | XML-RPC ingress | Required (WebCheck protocol emulation, port logic from web-check Python) |
| 26 | Maria 304 native | Implemented in MVP-C codebase; activation via config disabled by default at first pilot, enabled when retail flow demands |
| 27 | Checkbox-compat ingress | Implemented in MVP-C codebase; activation via config disabled by default at first pilot |
| 28 | Branch strategy | `rust-gateway` branch in current repo |
| 29 | Roadmap | 7-8 months pair-coding effort |
| 30 | Ingress extensibility | Trait-based shell registration so new protocols are 1-file add |

---

## 3. Architecture

### 3.1 Target artifact

One Rust binary `prro`. Statically linked. Cross-compile targets:

- `x86_64-unknown-linux-gnu` (RHEL/Debian/Ubuntu)
- `x86_64-pc-windows-msvc` (Win10+)
- `aarch64-apple-darwin` (dev only)

Approximate post-strip release size: ~40-45 MB (typst dependency is the dominant +30 MB; DejaVu fonts +3 MB; rest is gateway code).

One process. One tokio multi-threaded runtime. All subsystems (ingress, write-path, transports, admin UI, rendering, ops_loop) share `Arc<App>` DI container.

### 3.2 Workspace layout

```
rust/
  Cargo.toml                 — workspace
  prro_crypto/               — DSTU 4145, CMS, X.509  (unchanged library)
  prro_escpos/               — XML→ESC/POS compiler   (unchanged library)
  maria304_driver/           — Maria 304 wire protocol (unchanged library)
  prro_escpos_daemon/        — universal print HTTP service (kept, parallel)
  prro_sidecar/              — DEPRECATED, removed after cutover

  prro/                      — NEW main gateway binary
    src/
      main.rs                — CLI entrypoint (clap subcommands)
      app.rs                 — App DI container
      config/                — TOML + env + CLI overrides
      db/
        models.rs            — domain types (sqlx::Type-derived enums)
        repositories/        — async repos for each table
        migrations/*.sql     — clean schema 001..NNN
      ingress/
        shell.rs             — IngressShell trait
        rest.rs
        xmlrpc.rs            — WebCheck-emulation
        maria.rs
        maria304.rs          — uses maria304_driver lib
        checkbox_compat.rs
      adapters/              — wire → CanonicalFiscalCommand
        rest.rs xmlrpc.rs maria.rs maria304.rs checkbox_compat.rs
        canonical.rs
      services/
        ingress_service.rs   — inbox + worker dispatch
        write_path/
          mod.rs
          stages/             — acquire / validate / guard / sign / send / finalize
          state_machine.rs   — DocState transitions
          worker.rs          — WriteWorker, mpsc loop
          worker_registry.rs — DashMap<FN, WorkerHandle>
        reconciliation.rs
        shifts.rs offline.rs cert_provisioning.rs
      transports/
        dps_channel.rs       — DpsChannel trait
        dps_grpc.rs          — gRPC cabinet impl
        edyne_vikno.rs       — stub for future
        checkbox_rest.rs
        retry.rs             — exponential backoff
      crypto/
        service.rs           — wraps prro_crypto
        cred_seal.rs         — XOR-soft password sealing
        cert_cache.rs
        cmp_fetcher.rs
      rendering/
        context_builder.rs
        formatter.rs
        html.rs              — Askama-based
        pdf.rs               — typst-based
        escpos.rs            — uses prro_escpos
        qr.rs                — qrcodegen wrapper
      admin_ui/
        mod.rs               — register_admin_ui
        routes/              — auth, dashboard, settings, fns, cashiers, printers, recovery, receipt
        templates/*.html.j2  — Askama compile-time
        middleware/          — auth, csrf
        forms.rs
      runtime/
        supervisor.rs        — startup + shutdown sequence
        ops_loop.rs          — periodic tasks (cert refresh, recovery scan)
        health.rs metrics.rs audit.rs
        singleton.rs         — PID file lock (fs4)
      doctor/                — `prro doctor` diagnostics
    assets/
      fonts/
        DejaVuSansMono.ttf
        DejaVuSansMono-Bold.ttf
      printer_profiles/      — bundled XML (also in prro_escpos)
      tls_roots/             — webpki-roots fallback
    migrations/              — sqlx-cli managed
    tests/
      fixtures/              — golden bytes for byte-equivalence
      ingress_e2e/
      write_path_scenarios/
      reconciliation/
```

### 3.3 Top-level dependencies

```toml
[dependencies]
prro_crypto      = { path = "../prro_crypto" }
prro_escpos      = { path = "../prro_escpos" }
maria304_driver  = { path = "../maria304_driver" }

# Runtime + HTTP
axum             = "0.7"
tokio            = { version = "1", features = ["full"] }
tokio-util       = { version = "0.7", features = ["rt"] }
tower            = "0.5"
tower-http       = { version = "0.5", features = ["trace", "cors", "compression-br", "limit"] }
tower-sessions   = "0.13"
tower-sessions-sqlx-store = "0.13"

# Templates + assets
askama           = "0.12"
askama_axum      = "0.4"
rust-embed       = "8"

# DB
sqlx             = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "sqlite", "chrono", "uuid", "macros"] }

# Serialization
serde            = { version = "1", features = ["derive"] }
serde_json       = "1"
toml             = "0.8"
quick-xml        = "0.36"

# Crypto / hashing
sha2             = "0.10"
hex              = "0.4"
base64           = "0.22"
encoding_rs      = "0.8"
hmac             = "0.12"

# UUID + dates
uuid             = { version = "1", features = ["v7", "serde"] }
chrono           = { version = "0.4", features = ["serde"] }

# CLI + logging
clap             = { version = "4.5", features = ["derive", "env"] }
tracing          = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Concurrency
dashmap          = "6"
arc-swap         = "1"

# gRPC / HTTP transports
tonic            = { version = "0.12", features = ["tls", "tls-roots"] }
prost            = "0.13"
reqwest          = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

# Rendering
typst            = "0.13"
typst-pdf        = "0.13"
qrcodegen        = "1.8"
image            = "0.25"

# Misc
fs4              = "0.9"          # PID file lock
secrecy          = "0.10"         # SecretString
thiserror        = "1"
anyhow           = "1"

[features]
default = ["rest", "xmlrpc", "maria304", "admin_ui", "dps_grpc", "checkbox_rest"]
maria_legacy = []
checkbox_compat = []
serial_print = []   # opt-in: pulls serialport crate
usb_print = []      # opt-in: pulls rusb crate
```

### 3.4 CLI shape

```bash
prro serve [--config /etc/prro/prro.toml]
prro migrate [--db /var/lib/prro/prro.db]
prro doctor                           # config + perm + network + cert validity
prro fn add <FN> --tax-number ... --org "..." [--vat-payer-inn ...]
prro fn list
prro fn seed-mac --fn <FN> --hex <32-byte hex>
prro cashier add --fn <FN> --inn <INN> --jks <path> [--name "..."]
prro shift open --fn <FN>
prro shift close --fn <FN>
prro test-print --profile tm-t88ii --host 192.168.1.50:9100
prro health
prro print --doc <UUID> --printer <printer-id>
```

CLI commands operate against the same SQLite DB. They acquire the singleton PID lock just like `serve`, so they cannot run concurrently with a running daemon.

### 3.5 Ingress extensibility (trait-based shell registration)

Adding a new ingress protocol is a single-file change:

```rust
// ingress/shell.rs
#[async_trait]
pub trait IngressShell: Send + Sync + 'static {
    async fn run(self: Box<Self>, app: Arc<App>, cancel: CancellationToken) -> Result<()>;
    fn name(&self) -> &'static str;
    fn protocol(&self) -> Protocol;
}

// ingress/registry.rs
pub fn build_shells(config: &IngressConfig) -> Vec<Box<dyn IngressShell>> {
    let mut shells: Vec<Box<dyn IngressShell>> = Vec::new();
    if config.rest.enabled { shells.push(Box::new(RestIngress::new(&config.rest))); }
    if config.xmlrpc.enabled { shells.push(Box::new(XmlRpcIngress::new(&config.xmlrpc))); }
    if config.maria.enabled { shells.push(Box::new(MariaIngress::new(&config.maria))); }
    if config.maria304.enabled { shells.push(Box::new(Maria304Ingress::new(&config.maria304))); }
    if config.checkbox_compat.enabled { shells.push(Box::new(CheckboxCompatIngress::new(...))); }
    shells
}

// supervisor spawns each
for shell in shells {
    let app = app.clone();
    let cancel = cancel_root.child_token();
    tokio::spawn(async move {
        let name = shell.name();
        if let Err(e) = shell.run(app, cancel).await {
            error!(target: "ingress", shell = name, err = %e, "shell exited");
        }
    });
}
```

Adding a 6th protocol = new file in `ingress/` + new file in `adapters/` + 2 lines in `registry.rs` + config block. No restructuring.

---

## 4. Persistence schema

Clean redesign from migration 001. Drop legacy fields (`current_channel_lock`, `current_integration_owner`, `crypto_state_json`, `serial`, `control_number`). Reduce 22 migrations to a clean 5-10 logical migrations representing the real domain.

### 4.1 Schema principles

- **STRICT tables** everywhere (sqlx bundles SQLite 3.46+ via libsqlite3-sys when `bundled` feature is on; safe to require).
- **UUIDv7 BLOB** for all identifiers (16B vs 36B TEXT, monotonic, sortable).
- **Enum types via TEXT + sqlx::Type** with Rust-side whitelist; minimal `CHECK` constraints.
- **All FKs ON DELETE CASCADE** for child tables.
- **`updated_at` via TRIGGER** — single source of truth.
- **Partial indexes** on hot query patterns (`WHERE active=1`, `WHERE status NOT IN (final-states)`).

### 4.2 Core tables

(Full DDL in `migrations/001_core.sql`. Sketch — not exhaustive.)

- `applied_migrations`
- `fiscal_number_config` — adds `point_name`, drops legacy
- `licenses` — per-TIN commercial license rows (kept per user requirement)
- `prro_bindings` — explicit `(fiscal_number, backend_profile_id, transport_profile_id)`, no soft fallback
- `backend_profiles` / `transport_profiles` — including `channel_kind`, `test_mode`
- `shifts` — UUIDv7 PK
- `node_state` — drops `current_channel_lock`, `current_integration_owner`
- `fiscal_documents` — UUIDv7 PK, BLOB hashes (32B)
- `document_files` — `kind` enum
- `ingress_inbox` — UUIDv7 PK, UNIQUE `(fiscal_number, idempotency_key)`
- `offline_sessions` / `offline_codes`
- `sidecar_operators` — adds `cred_salt` BLOB, `jks_password_hex` always sealed
- `operator_certs` — public cert info, populated on first sign or via CMP
- `cert_provisioning_config` — single row, CMP endpoints + timeouts
- `tax_group_definitions`
- `payment_type_definitions`
- `printer_profiles` — `destination_type` enum (tcp/serial/usb), `paper_width_mm` 58/80/112
- `audit_log` — adds `actor` column

Drops (vs current Python schema):

- `cert_status_cache`, `cert_watch_config` — folded into single `cert_provisioning` module
- `offline_local_ack_state` — absorbed into `fiscal_documents.state` enum
- `fiscal_documents.serial` — covered by `lnd` + `shifts.serial`
- `fiscal_documents.control_number` — unused
- Various legacy bridging tables from migration churn

### 4.3 Repository layer pattern

All repos follow:

```rust
pub struct FiscalDocumentRepo<'a> { pool: &'a SqlitePool }

impl<'a> FiscalDocumentRepo<'a> {
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<FiscalDocument>>;
    pub async fn save_prepared(&self, doc: &NewFiscalDocument) -> Result<()>;
    pub async fn transition_state(&self, id: Uuid, from: DocState, to: DocState) -> Result<()>;
    // ...
}
```

`sqlx::query_as!` macro → compile-time SQL/schema check. Schema drift = build error, not runtime.

---

## 5. Write-path + state machines

### 5.1 6-stage pipeline

`acquire → validate → guard → sign → send_or_offline → finalize`. Identical staging to current Python — proven design.

Each stage = async function. Failure at any stage transitions the doc to `ERROR_RETRYABLE` / `REJECTED` / `REQUIRES_MANUAL_RECONCILIATION` per error classification.

### 5.2 Single-writer per fiscal_number (invariant #2)

**Approach C** (decision #7):

- One tokio task per FN, owns `mpsc::Receiver<WorkerJob>` (compiler enforces single consumer).
- `WorkerRegistry: DashMap<FiscalNumber, WorkerHandle>` for routing (`tx.clone()` is fine for senders).
- DB-level CAS via `UPDATE WHERE state = ?` — catches cross-process races.
- PID file lock (`fs4` crate) on startup — prevents two `prro` instances on same DB.

### 5.3 State machines

`DocState`, `ShiftState`, `OfflineSessionState`, `NodeMode` — Rust enums with `#[derive(sqlx::Type)]`. Allowed transitions whitelisted in `state_machine.rs`. Tests assert each forbidden transition is rejected with `StateTransitionConflict`.

### 5.4 MAC chain

`fn next_mac(prev: Option<&[u8;32]>, payload_xml: &[u8]) -> [u8;32]` — SHA-256(prev || payload). Ported 1:1 from Python.

`node_state.last_known_mac` BLOB(32) — bootstrap anchor. CLI command `prro fn seed-mac --fn ... --hex ...` for fresh DB against existing DPS history.

### 5.5 DPS error classification

Whitelist table in `transports/dps_grpc.rs`. Code → `ErrorClass` enum (`Rejected`, `RetryableWithCooldown`, `RetryableWithBackoff`, `NeedsReconciliation`, `Manual`). Full table per Section 6 of brainstorm — ported from Python.

### 5.6 Recovery (invariant #8)

Boot phases: `BOOT → RECOVERY → READY`. Recovery scans `fiscal_documents` in non-final states, attempts reconciliation against DPS (`fetch_status(fiscal_no)`). Dangling shifts surfaced in `/admin/ui/recovery`.

### 5.7 Graceful shutdown (invariant #9)

`CancellationToken` cascades from supervisor: ingress shells stop accepting → workers drain inbox → ops_loop / cert_watch / backup stop → DB pool closes. Configurable `graceful_shutdown_timeout_seconds`. After timeout — `tokio::select!` aborts.

---

## 6. Ingress + adapters

### 6.1 Per-protocol matrix

| Protocol | Sync | Idempotency source | Auth | Default port |
|---|---|---|---|---|
| REST | yes | `Idempotency-Key` header | optional bearer | 8080 |
| XML-RPC | yes | request_id param | none (legacy) | 8090 |
| Maria | async | frame number | none | 8100 |
| Maria 304 native | async | RequestId field | shared token | 8200 |
| Checkbox-compat | yes | webhook `id` | API key | 8443 |

### 6.2 IngressService contract

`receive_sync(cmd, timeout) → DocResponse` and `receive_async(cmd) → RequestId`. Inbox INSERT with UNIQUE `(fiscal_number, idempotency_key)` ON CONFLICT IGNORE. ResponseResolver: `DashMap<RequestId, oneshot::Sender>` for sync wait.

### 6.3 Per-FN rate limiting

Sliding window per fiscal_number. Default 60 req/min, configurable. Internal ingress (CLI) bypass.

### 6.4 Security per ingress

Body size limit 5 MB default. TCP `set_nodelay` + read timeout 30s. Maria 304 token constant-time compare. CORS allowlist for admin UI.

---

## 7. Admin UI + rendering

### 7.1 Stack

axum + tower-sessions (SQLite store) + Askama templates + custom CSRF helper + LAN bind with CORS allowlist.

### 7.2 Routes (port from Python admin UI)

- `/admin/ui/login`, `/logout`
- `/admin/ui/`, `/documents`, `/documents/:id`
- `/admin/ui/settings/fns/{new,edit,delete}`
- `/admin/ui/settings/operators` (list) + `/admin/ui/settings/fns/{fn}/operators/new` + `/operators/{id}/{edit,delete}` + key parsing dialog
- `/admin/ui/settings/printers` (list) + `/admin/ui/settings/printers/{new,/{id}/edit,/{id}/delete}` (port from Phase 13 Step 4a)
- `/admin/ui/settings/node`, `/dps`
- `/admin/ui/recovery` — dangling shifts, manual-recon docs, stuck inbox
- `/admin/ui/documents/:id/{receipt.html,receipt.pdf,print}`

### 7.3 Receipt rendering

- HTML: Askama template + inline QR SVG (qrcodegen)
- PDF: typst as embedded library, source compiled per-receipt with embedded DejaVu Sans Mono
- ESC/POS: prro_escpos compiler + bundled XML profiles, sent to printer via in-process transport (TCP/Serial/USB)

### 7.4 CSRF helper

50 LoC: per-session token via `secrets::token_urlsafe(32)` stored in tower-session, hidden form input, double-submit cookie, `hmac::compare_digest` validation.

---

## 8. Transports + crypto

### 8.1 DPS channel abstraction

```rust
#[async_trait]
pub trait DpsChannel: Send + Sync {
    async fn send_signed_cms(&self, cms: &[u8], idem: &str) -> Result<DpsAckOrError>;
    async fn fetch_status(&self, fiscal_no: &str) -> Result<DocStatus>;
    fn channel_id(&self) -> &str;
    fn supports_offline_codes(&self) -> bool;
    fn supports_async_callback(&self) -> bool;
}
```

Variants:

- `GrpcCabinet` — `cabinet.tax.gov.ua:9443` (test) / prod endpoint, via tonic
- `EdyneVikno` — stub for future `diia.gov.ua` integration, returns `NotImplemented` until launched
- `SoapDps` — legacy fallback if any FN still requires it

### 8.2 Test/prod isolation

Per-FN `fiscal_mode IN ('test','prod')`. Per-transport_profile `test_mode: bool`. Cross-check at startup (`prro doctor` warns) and at signing-stage (refuse to sign if mismatch).

### 8.3 Crypto service

Wraps `prro_crypto`. In-process (no HTTP hop to sidecar). Cred-sealing default-on (xor_soft + per-row salt). Cert provisioning module ports `cert_provisioning.py` 1:1 with active CMP fetch capability.

### 8.4 Print transport

Used internally by admin UI `print` action; uses `prro_escpos` compiler + bundled XML profiles + TCP/Serial/USB transport. `prro_escpos_daemon` remains as a separate universal HTTP service for non-PRRO consumers.

---

## 9. Testing strategy

### 9.1 Hybrid (decision #3)

- **Byte-equivalence golden tests** for hot zones: DPS XML serializer, MAC chain, KVT1/KVT2 parser, ESC/POS bytes, Maria 304 wire, offline code allocation, audit log shape. Capture goldens from current Python gateway, assert byte-for-byte equality.
- **Fresh Rust tests** for HTTP handlers (axum::test), repositories (in-memory sqlx), Askama templates, state machine transitions, recovery scenarios.

### 9.2 Estimated test effort

~16-22 weeks of pure test work, parallel with implementation. Budget breakdown in roadmap.

### 9.3 Mock DPS server

Native Rust tonic server mirroring `mock_dps_server.py`. ~300 LoC. Used in CI integration tests.

---

## 10. Cutover plan

### 10.1 Pre-cutover criteria

| Gate | Metric |
|---|---|
| Hot-zone byte-equivalence | 80/80 pass |
| Ingress→write_path→DPS happy path | 30/30 E2E pass |
| Crash recovery scenarios | 5/5 pass |
| DPS error reactions | 30/30 codes match Python |
| Side-by-side run on test FN | 7 days, 0 byte diffs |
| Pilot retail box | 30 days, no fiscal regressions |
| Performance baseline | ≥ Python req/sec, p99 latency |

### 10.2 Cutover sequence

1. M7-W1: Side-by-side run on test FN, capture diffs nightly.
2. M7-W2-3: Fix any diff. Re-run for 7 clean days.
3. M7-W4: Performance tune.
4. M8-W1: Documentation pack (deploy guide, ops runbook, troubleshooting).
5. M8-W2: One pilot retail box swap. Old Python kept running on backup VM.
6. M8-W3-4: Monitor 30 days. Freeze Python branch. Remove old `prro_sidecar` directory.

### 10.3 What stays

- `prro_escpos_daemon` — kept permanently as universal print service for other projects (per user requirement).
- `prro_crypto`, `prro_escpos`, `maria304_driver` — library crates, statically linked into `prro`.

### 10.4 What goes

- Python `src/prro_gateway/` — frozen, then deleted.
- `prro_sidecar` directory — deleted (logic absorbed into `prro::transports::dps_grpc`).

---

## 11. Roadmap

| Month | Focus | Deliverable |
|---|---|---|
| M1 | Schema + repos | `cargo test -p prro` 100+ DB tests green |
| M2 | Crypto + transports | Mock DPS round-trip, hot-zone byte tests green |
| M3 | Write-path | SHIFT_OPEN + SELL + SHIFT_CLOSE e2e against mock DPS |
| M4 | Recovery + ingress | Full ingress→write_path→DPS for test FN |
| M5 | Admin UI | Feature parity with Python admin UI |
| M6 | Maria + polish | All ingress, observability, packaging |
| M7 | Side-by-side | 7 clean days vs Python on test FN, performance tune |
| M8 | Cutover | Pilot deploy, 30-day monitor, Python freeze |

**Total: 7-8 months pair-coding effort.** Scope reduction (drop XML-RPC / Maria 304 / Checkbox-compat) could shave 1-2 months.

---

## 12. Risks + mitigations

| Risk | Probability | Mitigation |
|---|---|---|
| typst dependency cross-compile breaks | Low | CI matrix Linux+Windows from week 1 |
| sqlx compile-time SQL slow at edit | Med | `SQLX_OFFLINE=true` + `sqlx prepare` workflow |
| Maria 304 wire edge cases missed | High | Capture 100+ wire frames from Python prod, byte-replay |
| DPS XML cp1251 edge cases | Med | All Python sprint test cases ported as goldens |
| Cert chain handling (Key-6.dat) | Med | CMP fetch active + manual upload UI |
| Performance regression on retail | Low | criterion benchmarks from M3 |
| Pilot regression on real DPS | Med-High | Side-by-side M7 — zero diff before cutover |
| typst breaking changes in 0.x | Med | Pin minor version; review on bumps |

---

## 13. Out of scope

- Multi-tenant isolation beyond per-FN routing
- HTTPS termination (reverse proxy responsibility)
- Real-time replication / clustering
- WASM admin UI (server-rendered Askama is the choice)
- Migration of existing Python production data (no production exists)

---

## 14. Open follow-ups (post-cutover)

- Windows service / systemd unit installer scripts
- Auto-update mechanism (background fetch + rolling restart)
- Multi-printer per FN (currently one default printer per FN, multi-printer routing rules)
- "Єдине вікно" backend channel implementation when API published
- Full SBOM + supply-chain audit (cargo-audit + cargo-deny in CI)

---

## 15. Sign-off

This design synthesizes 7 brainstorm sections. Awaiting user approval. After approval:

1. Self-review for placeholders / contradictions / ambiguity (inline fixes).
2. User review of this committed spec file.
3. Invoke `writing-plans` skill to produce week-by-week implementation plan with task breakdown.
4. Implementation in `rust-gateway` branch.
