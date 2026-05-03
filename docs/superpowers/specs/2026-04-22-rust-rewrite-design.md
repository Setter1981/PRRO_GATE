# PRRO Gateway — Full Rust Rewrite Design

**Status:** Draft v3 — second-round review fixes applied; awaiting user sign-off
**Date:** 2026-04-22
**Owner:** Setter1981 + pair-AI
**Target:** Single Rust binary on retail (Windows + Linux), full feature parity with current Python gateway, freeze Python after cutover.

**Revision log:**
- v1 (initial): 30 decisions from 7-section brainstorm.
- v2 (1st-review): added decisions #31–#37 closing 3 HIGH + 3 MED findings (idempotency conflict policy, MAC chain alignment, DpsSubmission struct, per-table FK policy, musl Linux target, SQLite bundled pin, M8 packaging).
- v3 (2nd-review): added decisions #38–#41 closing 1 HIGH + 2 MED + 2 LOW findings (DpsStatusQuery + ambiguous-outcome FN block, with_immediate sqlx tx primitive, maintenance/live CLI split, Rust-idiomatic CSRF generator).

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
| 31 | Idempotency conflict policy | **STRICT** — same `(fn, idem_key)` + same `payload_sha256_canonical` ⇒ replay; differing payload ⇒ HTTP 409 Conflict, never silent replay (revised after spec review) |
| 32 | MAC / `previous_hash` semantics | `previous_hash(N) = SHA-256(unsigned_xml(N-1))` — NOT a chain MAC; aligned with Python `write_path.py:477` (revised after spec review) |
| 33 | DpsChannel envelope | Rich `DpsSubmission` struct (FN, LND, doc_type, business_ts, offline ids, cancellation, profile, fiscal_mode) — NOT bare `(cms, idem)` (revised after spec review) |
| 34 | FK deletion policy | Per-table; **RESTRICT** for legally significant data (fiscal_documents, shifts, sidecar_operators), **CASCADE** only for derivative artefacts (document_files, kvt2_envelopes); audit_log carries no FK (revised after spec review) |
| 35 | Linux target | Primary `x86_64-unknown-linux-musl` (truly static), fallback `x86_64-unknown-linux-gnu` if typst sub-deps block musl (revised after spec review) |
| 36 | SQLite bundled | Pinned via `libsqlite3-sys = { version = "0.30", features = ["bundled"] }` in `Cargo.toml` (revised after spec review) |
| 37 | Pre-cutover deliverables | Add to M8: Windows service / systemd unit installers; SBOM + cargo-audit + cargo-deny CI; signed artefacts + reproducible builds (revised after spec review) |
| 38 | DPS reconciliation / status query | `DpsStatusQuery::{ByServerFiscalNo, ByLocalIdentity}` + `DpsStatusOutcome::{Found, NotFound, Ambiguous, QueryNotSupported}`; FN HARD-BLOCKS on Ambiguous/QueryNotSupported until operator resolves at /admin/ui/recovery (revised after spec review v3) |
| 39 | Write-tx primitive | `db::tx::with_immediate(pool, fn)` helper acquires raw connection + manual `BEGIN IMMEDIATE`; never nest under `pool.begin()` (revised after spec review v3) |
| 40 | CLI split | Maintenance CLI (PID lock, daemon stopped) vs Live CLI (HTTP-to-daemon, no lock); see §3.4 (revised after spec review v3) |
| 41 | CSRF token generator | `OsRng + base64url` (Rust idioms), `subtle::ConstantTimeEq` for verify; not Python's `secrets.token_urlsafe` (revised after spec review v3) |

---

## 3. Architecture

### 3.1 Target artifact

One Rust binary `prro`. Statically linked. Cross-compile targets:

- **`x86_64-unknown-linux-musl`** — primary Linux target. Statically linked (no glibc dependency on host). Single-file deploy on any Linux 3.x+ retail box.
- `x86_64-unknown-linux-gnu` — secondary Linux target. Smaller binary (~5-10% less) but requires host glibc ≥ matching dev image. Use only when musl build is blocked by a sub-dependency (e.g. typst sub-deps).
- `x86_64-pc-windows-msvc` (Win10+) — statically linked CRT (`+crt-static` rustflag).
- `aarch64-apple-darwin` (dev only) — not retail-deployed.

Approximate post-strip release size: ~40-45 MB (typst dependency dominates +30 MB; DejaVu fonts +3 MB; rest is gateway code). musl typically adds ~5% size vs gnu.

**Risk noted**: typst's transitive deps may include build scripts that depend on system libraries (e.g. `harfbuzz`, `freetype`). If musl build fails on these, fall back to gnu for Linux and document the glibc minimum version. Validate cross-compile matrix in CI from week 1.

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

# DB — bundled libsqlite3-sys is mandatory (decision in §4):
# - guarantees STRICT tables (SQLite ≥ 3.37) regardless of host
# - guarantees JSON1 + UUID-friendly extensions
# - removes dependency on system libsqlite3 cross-platform
sqlx             = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "sqlite", "chrono", "uuid", "macros"] }
libsqlite3-sys   = { version = "0.30", features = ["bundled"] }   # pinned bundled build

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

CLI is split into **two modes** (post-review fix MED-2). Mixing them
under a single PID lock would prevent the operator from making any
changes while the gateway runs — bad UX for a 24/7 retail service.

#### 3.4.1 Maintenance mode — daemon must be stopped

These touch the DB directly. They **acquire the singleton PID lock**
and refuse to run while `prro serve` is active.

```bash
prro serve [--config /etc/prro/prro.toml]
prro migrate [--db /var/lib/prro/prro.db]
prro doctor                           # config + perm + network + cert validity
prro fn seed-prevhash --fn <FN> --hex <32-byte hex>   # bootstrap chain anchor
prro db backup [--out path]
prro db verify                        # integrity_check + chain replay
```

#### 3.4.2 Live mode — talks to the running daemon over loopback HTTP

These hit `http://127.0.0.1:<admin_port>` (or unix socket on Linux).
They authenticate via a local-only API key auto-generated on
`prro serve` startup and stored in `/var/lib/prro/cli.key` with
mode 0600. No PID lock needed — the daemon itself serializes
operations through its WriteWorker registry.

```bash
prro fn add <FN> --tax-number ... --org "..." [--vat-payer-inn ...]
prro fn list
prro cashier add --fn <FN> --inn <INN> --jks <path> [--name "..."]
prro shift open --fn <FN>
prro shift close --fn <FN>
prro test-print --profile tm-t88ii --host 192.168.1.50:9100
prro health
prro print --doc <UUID> --printer <printer-id>
prro recovery list
prro recovery resolve --doc <UUID> --action {ack|reject|resend}
```

If a live-mode command runs while the daemon is stopped, it errors
clearly: `daemon not running at 127.0.0.1:8443; start with 'prro serve'`.

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

- **STRICT tables** everywhere — bundled SQLite via `libsqlite3-sys = { features = ["bundled"] }` (see §3.3 dependency block) gives 3.46+ unconditionally.
- **UUIDv7 BLOB** for all identifiers (16B vs 36B TEXT, monotonic, sortable).
- **Enum types via TEXT + sqlx::Type** with Rust-side whitelist; minimal `CHECK` constraints.
- **FK deletion policy is per-table, not global** (revised after spec review):

  | Parent → Child | Policy | Rationale |
  |---|---|---|
  | `fiscal_documents → document_files` | `ON DELETE CASCADE` | Files are technical artifacts of a doc; meaningless without doc. |
  | `fiscal_documents → kvt2_envelopes` | `ON DELETE CASCADE` | Same — derivative of doc. |
  | `fiscal_number_config → fiscal_documents` | `ON DELETE RESTRICT` | Legally significant data; cannot delete a registered FN that has issued docs. |
  | `fiscal_number_config → shifts` | `ON DELETE RESTRICT` | Same. |
  | `shifts → fiscal_documents` | `ON DELETE RESTRICT` | Cannot delete a shift that has issued docs. |
  | `fiscal_number_config → sidecar_operators` | `ON DELETE RESTRICT` | Legal record — cashier-FN binding. |
  | `fiscal_number_config → printer_profiles` | `ON DELETE SET NULL` | Operational, not legal — orphan-OK. |
  | `audit_log` → no parent FK | (no FK) | Append-only log, can outlive entities. `entity_type`/`entity_id` are TEXT search keys. |
  | `ingress_inbox → fiscal_documents` (if linked) | (no FK) | Inbox is operational queue; doc creation is an effect, not a parent-child relationship. |
  | `offline_sessions → fiscal_documents` | `ON DELETE RESTRICT` | Session is legally tied to its docs. |

  Default for **legally significant** tables is RESTRICT. Cascade only for technical/derivative artifacts.
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

### 5.4 MAC / `previous_hash` semantics — ALIGNED WITH PYTHON

**Critical clarification after spec review.** Earlier draft said
`MAC(N) = SHA-256(prev_mac || payload(N))` — that is **WRONG** for our DPS protocol.

Python source of truth: `src/prro_gateway/services/write_path.py:477` —
the `previous_hash` field on document N is **SHA-256 of the previous
document's UNSIGNED DPS XML payload**. This is NOT a Merkle-style
running MAC chain; it is "doc N references doc N-1's unsigned XML
hash". DPS validates by recomputing.

Rust must compute byte-identical:

```rust
// crypto/mac.rs
pub fn previous_hash_for(unsigned_xml_n_minus_1: &[u8]) -> [u8; 32] {
    Sha256::digest(unsigned_xml_n_minus_1).into()
}
```

Storage:

- `fiscal_documents.unsigned_xml_sha256` BLOB(32) — sha256 of THIS document's
  unsigned DPS XML (cp1251 bytes, before CMS wrap). Computed at sign-stage,
  persisted on commit. Used by next document's `previous_hash`.
- `fiscal_documents.previous_hash` BLOB(32) NULL — pulled from previous doc's
  `unsigned_xml_sha256` (or from `node_state.last_known_unsigned_xml_sha256` for
  the first doc after fresh DB).
- `node_state.last_known_unsigned_xml_sha256` BLOB(32) NULL — bootstrap anchor:
  set via `prro fn seed-prevhash --fn ... --hex ...` when the FN already has
  DPS history but a fresh DB is being initialised; updated on every successful
  ACK.

Distinction from `payload_sha256`:

- `fiscal_documents.payload_sha256_canonical` BLOB(32) — sha256 of the
  canonical JSON envelope (what we have now). Used for inbox-side
  idempotency conflict detection (see §6.2).
- `fiscal_documents.unsigned_xml_sha256` BLOB(32) — sha256 of the cp1251 DPS
  XML. Used for `previous_hash` chain.

Two distinct hashes; do not confuse. Byte-equivalence golden tests
(see §9) capture both per-document.

**Bootstrap policy** (project memory `project_dps_mac_bootstrap`): for a
fresh DB against an FN that already has DPS history, operator MUST seed
`node_state.last_known_unsigned_xml_sha256` from the last DPS-acknowledged
document via the CLI. Without seeding, DPS rejects the next submission
with code -2 (chain mismatch).

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

### 6.2 IngressService contract — strict idempotency

`receive_sync(cmd, timeout) → DocResponse` and `receive_async(cmd) → RequestId`.

**Revised after spec review.** Earlier draft said "ON CONFLICT IGNORE"
— that is **WRONG** because it would silently accept a different
`payload_sha256_canonical` under the same `(fiscal_number,
idempotency_key)` and replay the previous response. Two clients each
submitting their own SELL with the same idempotency_key (collision)
would see one of them lose data without any error.

Correct policy: **idempotency key must map deterministically to
exactly one payload hash; differing payload under the same key is
a 409 Conflict**.

Repository implementation:

**Note on transaction primitive (post-review fix MED-1).** sqlx's
`pool.begin()` already opens a `BEGIN DEFERRED`; issuing another
`BEGIN IMMEDIATE` inside that span is a SQLite error. We need a
helper that acquires a connection AND issues `BEGIN IMMEDIATE`
manually (so writers contend on the RESERVED lock from the start):

```rust
// db/tx.rs — single source of truth for write transactions
pub async fn with_immediate<R, F>(pool: &SqlitePool, f: F) -> Result<R>
where
    F: for<'c> FnOnce(&'c mut SqliteConnection)
        -> futures::future::BoxFuture<'c, Result<R>> + Send,
    R: Send,
{
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    match f(&mut *conn).await {
        Ok(r) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(r)
        }
        Err(e) => {
            // Best-effort rollback; ignore secondary error.
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(e)
        }
    }
}
```

The inbox insert and every write-path stage transition use this helper
exclusively. Test: `tests/db/single_writer_lock.rs` — spawn two tokio
tasks both racing for `with_immediate` on the same FN; assert one
succeeds, the other blocks until first commits, neither corrupts state.

```rust
pub enum InboxInsertOutcome {
    /// First time we see this (fn, idem_key) — row inserted.
    Created(InboxRow),
    /// Duplicate of an earlier identical request — return prior result.
    /// Same `payload_sha256_canonical` ⇒ true replay.
    Replay(InboxRow),
    /// Same (fn, idem_key) but DIFFERENT payload hash ⇒ caller bug or
    /// idempotency-key collision. Reject with 409, never replay.
    Conflict { existing_payload_hash: [u8; 32], submitted_payload_hash: [u8; 32] },
}

pub async fn insert_inbox(
    pool: &SqlitePool, cmd: &CanonicalFiscalCommand,
) -> Result<InboxInsertOutcome> {
    db::tx::with_immediate(pool, |conn| Box::pin(async move {
        if let Some(existing) = sqlx::query_as!(InboxRow,
            r#"SELECT ... FROM ingress_inbox
               WHERE fiscal_number = ? AND idempotency_key = ?"#,
            cmd.fiscal_number, cmd.idempotency_key
        ).fetch_optional(&mut *conn).await? {
            return Ok(if existing.payload_sha256_canonical[..] == cmd.payload_sha256[..] {
                InboxInsertOutcome::Replay(existing)
            } else {
                InboxInsertOutcome::Conflict {
                    existing_payload_hash: existing.payload_sha256_canonical,
                    submitted_payload_hash: cmd.payload_sha256,
                }
            });
        }
        let row = sqlx::query_as!(InboxRow,
            r#"INSERT INTO ingress_inbox (... , payload_sha256_canonical)
               VALUES (... , ?)
               RETURNING ..."#,
            cmd.payload_sha256
        ).fetch_one(&mut *conn).await?;
        Ok(InboxInsertOutcome::Created(row))
    })).await
}
```

`ingress_inbox.payload_sha256_canonical` BLOB(32) NOT NULL — new column
in §4 schema. Plus the existing `UNIQUE (fiscal_number, idempotency_key)`.

Caller-visible behaviour:

| Outcome | Sync response | Async response |
|---|---|---|
| `Created` | wait for worker → DocResponse | 202 + `request_id` |
| `Replay` | return previously stored `DocResponse` | 200 + previously stored `request_id` |
| `Conflict` | **HTTP 409 Conflict** with body `{ existing_hash, submitted_hash }` | same — fail-fast |

ResponseResolver: `DashMap<RequestId, oneshot::Sender<DocResponse>>`
for sync wait. Replay path looks up stored response from
`ingress_inbox` (or its linked `fiscal_documents` row) — never
re-runs the worker.

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
- Cashiers (`sidecar_operators`):
  - `GET  /admin/ui/settings/operators`
  - `GET  /admin/ui/settings/fns/{fn}/operators/new`
  - `POST /admin/ui/settings/fns/{fn}/operators/new`
  - `GET  /admin/ui/settings/operators/{id}/edit`
  - `POST /admin/ui/settings/operators/{id}/edit`
  - `POST /admin/ui/settings/operators/{id}/delete`
  - `POST /admin/ui/settings/fns/parse-key` (key parsing dialog)
- Printers (`printer_profiles`):
  - `GET  /admin/ui/settings/printers`
  - `GET  /admin/ui/settings/printers/new`
  - `POST /admin/ui/settings/printers/new`
  - `GET  /admin/ui/settings/printers/{id}/edit`
  - `POST /admin/ui/settings/printers/{id}/edit`
  - `POST /admin/ui/settings/printers/{id}/delete`
- `/admin/ui/settings/node`, `/dps`
- `/admin/ui/recovery` — dangling shifts, manual-recon docs, stuck inbox
- `/admin/ui/documents/:id/{receipt.html,receipt.pdf,print}`

### 7.3 Receipt rendering

- HTML: Askama template + inline QR SVG (qrcodegen)
- PDF: typst as embedded library, source compiled per-receipt with embedded DejaVu Sans Mono
- ESC/POS: prro_escpos compiler + bundled XML profiles, sent to printer via in-process transport (TCP/Serial/USB)

### 7.4 CSRF helper

~50 LoC, custom (no third-party crate). Per-session token stored in
tower-session, hidden form input, double-submit cookie, constant-time
comparison.

Token generation in pure Rust (post-review fix LOW-2 — earlier draft
referenced a Python `secrets.token_urlsafe` API that has no Rust
equivalent):

```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand_core::{OsRng, RngCore};

pub fn generate_csrf_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn verify_csrf(stored: &str, submitted: &str) -> bool {
    use subtle::ConstantTimeEq;
    stored.as_bytes().ct_eq(submitted.as_bytes()).into()
}
```

Deps: `rand_core = "0.6"`, `subtle = "2.5"`. Both small, no transitive
crypto.

---

## 8. Transports + crypto

### 8.1 DPS channel abstraction

**Revised after spec review.** `send_signed_cms(cms, idem)` is too thin
— DPS channels need full submission context (FN, LND, doc type, business
ts, offline ids, cancellation linkage, profile context) for routing,
metadata building, and audit trail. Introduce `DpsSubmission` struct
that carries everything the channel needs to make a decision:

```rust
#[derive(Debug)]
pub struct DpsSubmission {
    pub document_id: Uuid,
    pub fiscal_number: FiscalNumber,
    pub local_number: u64,                       // LND
    pub doc_type: DocType,                       // SHIFT_OPEN/SELL/RETURN/Z_REPORT/...
    pub business_ts: DateTime<Utc>,
    pub idempotency_key: String,
    pub signed_cms: Vec<u8>,
    pub unsigned_xml_sha256: [u8; 32],           // for chain validation (see §5.4)
    pub previous_hash: Option<[u8; 32]>,
    pub offline_session_id: Option<Uuid>,
    pub offline_fiscal_no: Option<u64>,
    pub offline_fiscal_date: Option<DateTime<Utc>>,
    pub cancellation_of: Option<Uuid>,           // tech-return / cancel pointer
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    pub fiscal_mode: FiscalMode,                 // test|prod (see §8.2)
    pub correlation_id: Option<String>,
}

#[derive(Debug)]
pub struct DpsAck {
    pub server_fiscal_no: String,
    pub server_fiscal_date: DateTime<Utc>,
    pub raw_response: Vec<u8>,                   // KVT2 bytes for archive
}

#[derive(Debug)]
pub enum DpsResponse {
    Ack(DpsAck),
    Reject { code: i32, message: String },
    NeedsReconciliation { code: i32, hint: String },
    Retryable { class: RetryClass, after: Option<Duration> },
}

#[async_trait]
pub trait DpsChannel: Send + Sync {
    async fn submit(&self, sub: &DpsSubmission) -> Result<DpsResponse, DpsTransportError>;
    async fn query_status(&self, q: &DpsStatusQuery)
        -> Result<DpsStatusOutcome, DpsTransportError>;
    fn channel_id(&self) -> &str;
    fn capabilities(&self) -> ChannelCapabilities;
}

// See §8.5 below for DpsStatusQuery / DpsStatusOutcome.

#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    pub supports_offline_codes: bool,
    pub supports_async_callback: bool,
    pub supports_cancellation: bool,
    pub max_payload_bytes: usize,
    pub supports_status_query: bool,
}
```

Variants implement the trait independently:

- `GrpcCabinetChannel` — packs `DpsSubmission` into gRPC `SendChkV2Request`
- `EdyneViknoChannel` — future, signature stays the same when API ships
- `SoapDpsChannel` — legacy fallback if needed

The submission struct is the contract. Channels translate to wire format
in their own module.

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

### 8.5 Reconciliation / status query — ambiguous-outcome policy

**Problem.** When a document is in `state IN ('SENDING','SENT','KVT1','ERROR_RETRYABLE')`
and the gateway crashed or its DPS request timed out, we may not know
whether DPS accepted the submission. We may **not even have a
`server_fiscal_no`**: the doc was acked by the channel transport
but the response packet was lost, or the connection died mid-write.
A `fetch_status(fn, server_fiscal_no)` API cannot represent this case.

**Fix (decision #38).** Introduce a richer query type and an explicit
ambiguous-outcome path that BLOCKS the FN until the operator resolves.

```rust
pub enum DpsStatusQuery {
    /// We do hold a server_fiscal_no — fast direct lookup.
    ByServerFiscalNo {
        fiscal_number: FiscalNumber,
        server_fiscal_no: String,
    },
    /// We do NOT have a server_fiscal_no.  DPS is asked to look up by
    /// our local identity + canonical content hash.  Channels that
    /// can't do this return `QueryNotSupported`.
    ByLocalIdentity {
        fiscal_number: FiscalNumber,
        local_number: u64,                    // LND
        business_ts: DateTime<Utc>,
        unsigned_xml_sha256: [u8; 32],         // ties query to specific content
        idempotency_key: String,
    },
}

pub enum DpsStatusOutcome {
    /// DPS has it acked. Recovery transitions doc → ACK.
    Found(DpsAck),

    /// DPS does not have any record matching the query. Channel is
    /// confident the submission did not land. Recovery may safely
    /// retry the original submission with the same idempotency_key.
    NotFound,

    /// DPS responded but the local hash / identity does not match
    /// any record DPS has. This is chain divergence — manual review
    /// required. Gateway BLOCKS subsequent docs on this FN.
    Ambiguous { reason: String },

    /// Channel cannot answer this query (e.g. no by-content lookup
    /// support). Recovery escalates to RequiresManualReconciliation.
    QueryNotSupported,
}
```

Recovery flow on boot or post-timeout:

1. For each `fiscal_documents` row in non-final state:
   - If `server_fiscal_no IS NOT NULL` → `ByServerFiscalNo` query.
   - Else → `ByLocalIdentity` query.
2. Map the outcome:
   - `Found(ack)` → transition `state → ACK`, persist `server_fiscal_no`,
     update `node_state.last_known_unsigned_xml_sha256`.
   - `NotFound` → safe to re-submit. Worker re-enters `send` stage
     with same `idempotency_key`.
   - `Ambiguous` → set `state = REQUIRES_MANUAL_RECONCILIATION`,
     emit audit row severity=CRITICAL, **block FN**.
   - `QueryNotSupported` → same as Ambiguous; manual.

**Hard block on RequiresManualReconciliation** (per fiscal_number):

```rust
async fn ensure_fn_unblocked(pool: &SqlitePool, fn_id: &FiscalNumber) -> Result<()> {
    let blocked: Option<i64> = sqlx::query_scalar!(
        r#"SELECT 1 FROM fiscal_documents
           WHERE fiscal_number = ?
             AND state = 'REQUIRES_MANUAL_RECONCILIATION'
           LIMIT 1"#,
        fn_id
    ).fetch_optional(pool).await?;
    if blocked.is_some() {
        return Err(WriteError::FnBlockedManualRecon { fn_id: fn_id.clone() });
    }
    Ok(())
}
```

WriteWorker calls this BEFORE Stage 1 (acquire) for every job. Until
the operator resolves the doc via `/admin/ui/recovery` (mark-ACK,
reject, or re-send), the FN does not accept new ingress. Ingress
returns HTTP 409 + `{ error: "fn_blocked_manual_reconciliation",
hint: "Resolve at /admin/ui/recovery" }`.

This closes the recovery gate (§5.6) for the SENDING-after-crash
scenario and aligns with frozen invariant #8 (recovery may not
silently violate state transitions).

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
| M8 | Packaging + cutover | systemd unit + Windows-service installer + signed `.deb`/`.rpm`/`.msi` + reproducible build CI + SBOM/audit; pilot deploy + 30-day monitor + Python freeze |

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

## 14. Open follow-ups

### 14.1 Required PRE-cutover (added M8)

Per spec review — these affect operational deployability and must be
solved before any pilot retail box runs the Rust binary.

- **Windows service / systemd unit installer scripts** — without these,
  retail can't auto-start `prro` after reboot. Block cutover.
- **SBOM + cargo-audit + cargo-deny in CI** — supply-chain hygiene for
  a binary running on a fiscal endpoint. Generate CycloneDX SBOM at
  build, fail CI on known-vulnerable transitive deps.
- **Packaging + signing + release reproducibility** — signed `.deb`,
  `.rpm`, `.msi` artefacts. Reproducible builds (`cargo --frozen
  --locked --offline` + pinned toolchain) so two builders produce
  byte-identical binaries — required for hash-based update verification
  and audit trail.

### 14.2 OK to defer post-cutover

- **Auto-update mechanism** (background fetch + rolling restart) — pilot
  retail boxes can be hand-updated for the first 60 days.
- **Multi-printer routing per FN** (current model: one default printer
  per FN). Operators with multiple printers per point can assign manually
  in the meantime.
- **"Єдине вікно" backend channel implementation** — only deferable if
  the DPS-published API isn't ready by pilot. The architectural slot
  (DpsChannel trait variant) is in M2.

---

## 15. Sign-off

This design synthesizes 7 brainstorm sections. Awaiting user approval. After approval:

1. Self-review for placeholders / contradictions / ambiguity (inline fixes).
2. User review of this committed spec file.
3. Invoke `writing-plans` skill to produce week-by-week implementation plan with task breakdown.
4. Implementation in `rust-gateway` branch.
