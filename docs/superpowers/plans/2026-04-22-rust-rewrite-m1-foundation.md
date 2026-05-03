# PRRO Gateway Rust Rewrite — Month 1 (Foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the Foundation layer (workspace, clean schema, repository contracts, transaction primitive, UUIDv7, App skeleton, CI matrix) for the Rust gateway rewrite per spec `docs/superpowers/specs/2026-04-22-rust-rewrite-design.md`.

**Architecture:** New crate `rust/prro/` joins the workspace alongside existing `prro_crypto`, `prro_escpos`, `maria304_driver`, `prro_escpos_daemon`. Clean SQLite schema starts at migration 001 (no carry-over from the Python schema). All write transactions go through one `db::tx::with_immediate` helper that issues `BEGIN IMMEDIATE` on a raw connection (per spec decision #39). UUIDv7 BLOB ids, sqlx compile-time SQL checks, STRICT tables backed by bundled SQLite ≥ 3.46.

**Tech Stack:** Rust 1.83+, sqlx 0.8 (SQLite + macros + chrono + uuid), `libsqlite3-sys` bundled, axum 0.7 (deferred to M5), tokio 1, clap 4.5, tracing, anyhow/thiserror, fs4 (PID lock), rand_core+OsRng, uuid v7, chrono.

**Out of scope this month:** Crypto wrapper, DpsChannel trait + GrpcCabinet impl, mock DPS server, write-path stages, ingress, admin UI. Those land in M2 and beyond — each month gets its own detailed plan when its work begins (see `docs/superpowers/plans/2026-MM-DD-rust-rewrite-mN-*.md`).

**Continuation strategy:**
- M1 (this plan) = Foundation: workspace, schema, repos, tx primitive, app skeleton, CI.
- M2 = Crypto + transports + mock DPS + hot-zone byte-equivalence goldens.
- M3 = Write-path stages + WriteWorker + state machines.
- M4 = Recovery + ingress (REST + XML-RPC).
- M5 = Admin UI + rendering (typst PDF, Askama templates).
- M6 = Maria 304 + checkbox-compat + observability + packaging.
- M7 = Side-by-side hardening + performance.
- M8 = Cutover + Python freeze.

Each month's plan is written when its predecessor is closing — avoids stale planning for work that is 6 months out.

---

## File structure (this month)

```
rust/
  Cargo.toml                              # MODIFY — add `prro` to workspace members
  prro/                                   # NEW crate
    Cargo.toml
    src/
      main.rs                             # binary entrypoint (clap subcommands; minimal)
      lib.rs                              # library re-exports
      app.rs                              # `App` DI container
      config/
        mod.rs                            # `AppConfig` TOML shape
        defaults.rs                       # builtin defaults
      db/
        mod.rs                            # `db::pool::open(path)` -> `SqlitePool`
        tx.rs                             # `with_immediate` helper (spec §6.2 / decision #39)
        models/
          mod.rs                          # re-export domain types
          ids.rs                          # UUIDv7 newtypes (FiscalNumberId etc.)
          enums.rs                        # sqlx::Type wrappers for DocState, ShiftState…
        repositories/
          mod.rs
          fiscal_number_config.rs
          shifts.rs
          node_state.rs
          fiscal_documents.rs             # most complex: state CAS
          ingress_inbox.rs                # idempotency Created/Replay/Conflict
          audit_log.rs
      runtime/
        singleton.rs                      # PID file lock (fs4)
      doctor.rs                           # minimal `prro doctor` (config + db + permissions)
    migrations/
      001_core_identities.sql             # fiscal_number_config, shifts, node_state, audit_log
      002_fiscal_documents.sql            # fiscal_documents, document_files, ingress_inbox
      003_operators_printers.sql          # sidecar_operators, operator_certs, printer_profiles, tax / payment defs
      004_offline_and_routing.sql         # offline_sessions, offline_codes, prro_bindings, backend_profiles, transport_profiles
      005_licenses.sql                    # licenses (kept per spec decision)
    tests/
      db_pool_smoke.rs
      tx_with_immediate_lock.rs
      repo_fiscal_number_config.rs
      repo_shifts.rs
      repo_fiscal_documents_state_cas.rs
      repo_ingress_inbox_idempotency.rs
      repo_audit_log.rs
      doctor_smoke.rs
.github/
  workflows/
    rust-prro.yml                         # NEW — Linux musl + Linux gnu + Windows MSVC matrix
```

---

## Task 0: Workspace bootstrap + branch + crate skeleton

**Goal:** Create branch `rust-gateway`, register new `prro` crate in the workspace, baseline `lib.rs` / `main.rs` / `Cargo.toml`. Repository builds clean.

**Files:**
- Modify: `rust/Cargo.toml` (add `prro` to `[workspace] members`)
- Create: `rust/prro/Cargo.toml`
- Create: `rust/prro/src/lib.rs`
- Create: `rust/prro/src/main.rs`

**Acceptance Criteria:**
- [ ] Branch `rust-gateway` exists locally and on origin
- [ ] `cargo build -p prro --manifest-path rust/Cargo.toml` succeeds with zero warnings
- [ ] `cargo run -p prro --manifest-path rust/Cargo.toml -- --version` prints `prro 0.1.0`

**Verify:** `cd rust && cargo build -p prro && cargo run -p prro -- --version` → output contains `prro 0.1.0`

**Steps:**

- [ ] **Step 1: Create branch from main**

```bash
cd /mnt/d/PRRO_GATE
git fetch origin
git checkout -b rust-gateway origin/main
git push -u origin rust-gateway
```

- [ ] **Step 2: Add `prro` to workspace**

Modify `rust/Cargo.toml` — append `"prro"` to the `members` list:

```toml
[workspace]
resolver = "2"
members = [
    "prro_crypto",
    "prro_crypto_v2",
    "prro_sidecar",
    "maria304_driver",
    "prro_escpos",
    "prro_escpos_daemon",
    "prro",
]
```

- [ ] **Step 3: Write `prro/Cargo.toml`**

Create `rust/prro/Cargo.toml`:

```toml
[package]
name = "prro"
version = "0.1.0"
edition = "2021"
description = "PRRO Gateway — single Rust binary for Ukrainian fiscal endpoint"

[[bin]]
name = "prro"
path = "src/main.rs"

[lib]
name = "prro"
path = "src/lib.rs"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["rt"] }

# DB — bundled SQLite per spec decision #36
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "sqlite", "chrono", "uuid", "macros"] }
libsqlite3-sys = { version = "0.30", features = ["bundled"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# IDs + dates
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

# CLI + logging
clap = { version = "4.5", features = ["derive", "env"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Errors
anyhow = "1"
thiserror = "1"

# OS / fs
fs4 = "0.9"

# RNG (M1 needs it for nothing user-facing, but lock in version for later modules)
rand_core = { version = "0.6", features = ["std"] }

# Crate cross-deps (will be wired in M2+)
prro_crypto = { path = "../prro_crypto" }
prro_escpos = { path = "../prro_escpos" }

[dev-dependencies]
pretty_assertions = "1"
tempfile = "3"
```

- [ ] **Step 4: Minimal `lib.rs` and `main.rs`**

Create `rust/prro/src/lib.rs`:

```rust
//! PRRO Gateway — single-binary Rust implementation.
//!
//! Top-level architecture lives in `app::App`. This crate is composed
//! of:
//! - `config`         — TOML + env + CLI overrides
//! - `db`             — sqlx pool, transaction primitive, repositories
//! - `runtime`        — singleton lock, supervisor, ops loop (M3+)
//! - `crypto`         — wraps `prro_crypto` (M2)
//! - `transports`     — DPS gRPC, Checkbox REST (M2)
//! - `services`       — write_path, reconciliation, ingress (M3+)
//! - `ingress`        — REST/XML-RPC/Maria/Maria304/Checkbox-compat (M4+)
//! - `admin_ui`       — Askama-rendered admin (M5)
//! - `rendering`      — receipt formatter + HTML/PDF/ESC-POS (M5)
//! - `doctor`         — `prro doctor` diagnostics

pub mod app;
pub mod config;
pub mod db;
pub mod doctor;
pub mod runtime;

pub use app::App;
```

Create `rust/prro/src/main.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "prro", version, about = "PRRO Gateway")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Print build info and exit.
    Version,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Version => {
            println!("prro {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
```

Create empty stub modules:

```bash
mkdir -p rust/prro/src/{config,db/{models,repositories},runtime,doctor}
mkdir -p rust/prro/migrations rust/prro/tests
```

Stub files (each one-liner placeholder so the tree compiles):

`rust/prro/src/app.rs`:
```rust
//! `App` is the DI composition root. M1 leaves this empty; M2 wires
//! crypto + transports.
pub struct App {}
```

`rust/prro/src/config/mod.rs`:
```rust
//! Config — fleshed out in Task 14.
```

`rust/prro/src/db/mod.rs`:
```rust
pub mod tx;
pub mod models;
pub mod repositories;
```

`rust/prro/src/db/tx.rs`:
```rust
//! Filled in by Task 6.
```

`rust/prro/src/db/models/mod.rs`:
```rust
//! Filled in by Task 7.
```

`rust/prro/src/db/repositories/mod.rs`:
```rust
//! Filled in by Tasks 8-13.
```

`rust/prro/src/runtime/mod.rs`:
```rust
pub mod singleton;
```

`rust/prro/src/runtime/singleton.rs`:
```rust
//! Filled in by Task 15.
```

`rust/prro/src/doctor.rs`:
```rust
//! Filled in by Task 15.
```

- [ ] **Step 5: Verify build + version**

```bash
cd rust
cargo build -p prro
cargo run -p prro -- --version
```

Expected output:
```
prro 0.1.0
```

No warnings. If warnings appear, fix or document them in this task.

- [ ] **Step 6: Commit**

```bash
cd /mnt/d/PRRO_GATE
git add rust/Cargo.toml rust/prro/
git commit -m "feat(rust): bootstrap prro crate (M1 task 0)

Workspace member registered. Stub lib + main + CLI version subcommand.
Dependency tree pinned per spec decisions #36 (libsqlite3-sys bundled),
#5 (sqlx 0.8), #11/#41 (rand_core + tower-sessions later)."
git push origin rust-gateway
```

---

## Task 1: Migration 001 — core identity tables

**Goal:** Land `fiscal_number_config`, `shifts`, `node_state`, `audit_log` per spec §4. STRICT tables, FKs per spec decision #34, partial indexes, UUIDv7 BLOB ids.

**Files:**
- Create: `rust/prro/migrations/001_core_identities.sql`

**Acceptance Criteria:**
- [ ] `sqlx::migrate!()` against a temp SQLite file applies cleanly via `db::open_pool`
- [ ] DDL contains `STRICT` keyword on every fiscal table (grep)
- [ ] STRICT typing enforced: inserting a TEXT into an INTEGER column returns an error in a fixture test
- [ ] FK policy matches spec table (RESTRICT for shifts→fn_config, audit_log has no FK)

**Verify:**
```bash
# Bundled libsqlite3-sys is the source of truth, NOT the system sqlite3 CLI
# (system sqlite3 is 3.45.x in WSL; runtime is whatever libsqlite3-sys ships).
cargo test -p prro --test migrations_apply
```
Expected output: `test migrations_apply::migration_001_creates_core_tables ... ok` (and STRICT-enforcement test).

**Steps:**

- [ ] **Step 1: Write migration 001**

Create `rust/prro/migrations/001_core_identities.sql`:

```sql
-- 001 — core identity tables.  Per spec §4.
--
-- NB: connection-level pragmas (journal_mode, foreign_keys, busy_timeout,
-- synchronous) live in `db::open_pool` via SqliteConnectOptions — they cannot
-- be set inside a transaction, and sqlx wraps each migration in one.

CREATE TABLE fiscal_number_config (
    fiscal_number          TEXT    PRIMARY KEY  CHECK (length(fiscal_number) = 10),
    tax_number             TEXT    NOT NULL,
    vat_payer_inn          TEXT    CHECK (vat_payer_inn IS NULL OR (length(vat_payer_inn) = 12 AND vat_payer_inn GLOB '[0-9]*')),
    fiscal_mode            TEXT    NOT NULL  CHECK (fiscal_mode IN ('test','prod')),
    org_name               TEXT,
    point_name             TEXT,
    org_address            TEXT,
    tsp_enabled            INTEGER NOT NULL DEFAULT 0  CHECK (tsp_enabled IN (0,1)),
    offline_enabled        INTEGER NOT NULL DEFAULT 1  CHECK (offline_enabled IN (0,1)),
    national_check_enabled INTEGER NOT NULL DEFAULT 0  CHECK (national_check_enabled IN (0,1)),
    min_offline_codes      INTEGER NOT NULL DEFAULT 0,
    max_offline_codes      INTEGER NOT NULL DEFAULT 0,
    created_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE TRIGGER fnc_updated_at
AFTER UPDATE ON fiscal_number_config
BEGIN
    UPDATE fiscal_number_config SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;
END;

CREATE TABLE shifts (
    shift_id               BLOB    PRIMARY KEY  CHECK (length(shift_id) = 16),
    fiscal_number          TEXT    NOT NULL,
    serial                 INTEGER,
    state                  TEXT    NOT NULL  CHECK (state IN ('CREATED','OPENING','OPENED','CLOSING','CLOSED','ERROR')),
    open_mode              TEXT    NOT NULL  CHECK (open_mode IN ('ONLINE','OFFLINE')),
    opened_at              TEXT,
    closed_at              TEXT,
    open_document_id       BLOB,
    close_document_id      BLOB,
    z_report_document_id   BLOB,
    cash_balance_kop       INTEGER NOT NULL DEFAULT 0,
    created_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE INDEX ix_shifts_fn_state ON shifts(fiscal_number, state);

CREATE TRIGGER shifts_updated_at
AFTER UPDATE ON shifts
BEGIN
    UPDATE shifts SET updated_at = CURRENT_TIMESTAMP WHERE shift_id = NEW.shift_id;
END;

CREATE TABLE node_state (
    fiscal_number               TEXT    PRIMARY KEY,
    mode                        TEXT    NOT NULL  CHECK (mode IN ('ONLINE','GOING_OFFLINE','OFFLINE','GOING_ONLINE','BLOCKED','STOP_MODE','CRYPTO_DEGRADED')),
    shift_state                 TEXT    NOT NULL  CHECK (shift_state IN ('CREATED','OPENING','OPENED','CLOSING','CLOSED','ERROR')),
    current_shift_id            BLOB,
    current_offline_session_id  BLOB,
    next_lnd                    INTEGER NOT NULL  CHECK (next_lnd >= 1),
    backend_profile_id          TEXT,
    transport_profile_id        TEXT,
    readiness_state             TEXT    NOT NULL DEFAULT 'STARTING'  CHECK (readiness_state IN ('STARTING','RECOVERING','READY','DEGRADED','STOPPED')),
    recovery_stage              TEXT    NOT NULL DEFAULT 'BOOT'  CHECK (recovery_stage IN ('BOOT','PHASE1','PHASE2','DONE','FAILED')),
    current_month_offline_seconds INTEGER NOT NULL DEFAULT 0,
    last_known_unsigned_xml_sha256 BLOB  CHECK (last_known_unsigned_xml_sha256 IS NULL OR length(last_known_unsigned_xml_sha256) = 32),
    last_fs_ping_at             TEXT,
    updated_at                  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER node_state_updated_at
AFTER UPDATE ON node_state
BEGIN
    UPDATE node_state SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;
END;

-- audit_log carries no FK (append-only, can outlive entities)
CREATE TABLE audit_log (
    audit_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type        TEXT    NOT NULL,
    entity_id          TEXT    NOT NULL,
    event_type         TEXT    NOT NULL,
    severity           TEXT    NOT NULL  CHECK (severity IN ('INFO','WARNING','ERROR','CRITICAL')),
    actor              TEXT,
    event_payload_json TEXT,
    created_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE INDEX ix_audit_entity ON audit_log(entity_type, entity_id, created_at);
```

- [ ] **Step 2: Add `db::open_pool` (early — needed by tests)**

This helper lands here in Task 1 instead of Task 6 because every migration test depends on it. Task 6 keeps the `with_immediate` helper.

Replace `rust/prro/src/db/mod.rs` with:

```rust
pub mod tx;
pub mod models;
pub mod repositories;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

/// Open a connection pool against the given SQLite file.
///
/// Sets WAL journal mode, busy_timeout 5s, foreign_keys ON, NORMAL synchronous.
/// Migrations are applied via `sqlx::migrate!()`.
pub async fn open_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let url = format!("sqlite:{}", path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
```

- [ ] **Step 3: Add unified migration test**

Create `rust/prro/tests/migrations_apply.rs`:

```rust
//! One verification path for all M1 migrations: apply via sqlx::migrate!,
//! assert table/index set, and prove STRICT typing rejects bad inserts.
//! Runs against bundled libsqlite3-sys, NOT the system sqlite3 CLI.

use std::collections::HashSet;

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    (dir, pool)
}

#[tokio::test]
async fn migration_001_creates_core_tables() {
    let (_d, pool) = fresh_pool().await;
    let names: HashSet<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    for t in ["fiscal_number_config", "shifts", "node_state", "audit_log"] {
        assert!(names.contains(t), "missing table {t}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_001_strict_typing_rejects_text_in_int_column() {
    let (_d, pool) = fresh_pool().await;
    // tsp_enabled is INTEGER NOT NULL — STRICT must reject 'abc'
    sqlx::query!(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) VALUES (?, '0', 'test')",
        "1234567890"
    )
    .execute(&pool)
    .await
    .unwrap();
    let err = sqlx::query("UPDATE fiscal_number_config SET tsp_enabled = 'abc' WHERE fiscal_number = '1234567890'")
        .execute(&pool)
        .await
        .expect_err("STRICT must reject TEXT in INTEGER column");
    let msg = err.to_string();
    assert!(
        msg.contains("INTEGER") || msg.contains("STRICT") || msg.contains("type"),
        "expected STRICT/type-mismatch error, got: {msg}"
    );
}
```

- [ ] **Step 4: Run + assert**

```bash
cargo test -p prro --test migrations_apply
```

Both tests must pass.

- [ ] **Step 5: Commit**

```bash
git add rust/prro/migrations/001_core_identities.sql \
        rust/prro/src/db/mod.rs \
        rust/prro/tests/migrations_apply.rs
git commit -m "feat(rust/db): migration 001 + open_pool + migration smoke test

fiscal_number_config + shifts + node_state + audit_log.  STRICT
tables, FKs per spec §4 decision #34.  open_pool + sqlx::migrate!
land here so Tasks 2-5 share one verification path (no sqlx-cli)."
git push origin rust-gateway
```

---

## Task 2: Migration 002 — fiscal_documents + document_files + ingress_inbox

**Goal:** Add document storage and ingress queue tables. Distinct hashes per spec §5.4 (`unsigned_xml_sha256` for chain, `payload_sha256_canonical` for idempotency).

**Files:**
- Create: `rust/prro/migrations/002_fiscal_documents.sql`

**Acceptance Criteria:**
- [ ] Tables apply cleanly on top of 001
- [ ] `ingress_inbox` has UNIQUE(`fiscal_number`,`idempotency_key`) and `payload_sha256_canonical` BLOB(32) NOT NULL
- [ ] `fiscal_documents` has both `unsigned_xml_sha256` and `payload_sha256_canonical` columns

**Verify:**
```bash
cargo test -p prro --test migrations_apply
```
Add a sub-test in `tests/migrations_apply.rs` that asserts both `unsigned_xml_sha256` and `payload_sha256_canonical` are present in `fiscal_documents` (and that `ingress_inbox` has UNIQUE `(fiscal_number, idempotency_key)`).

**Steps:**

- [ ] **Step 1: Write migration 002**

Create `rust/prro/migrations/002_fiscal_documents.sql`:

```sql
-- 002 — fiscal_documents, document_files, ingress_inbox.  Per spec §4.2 + §5.4 + §6.2.

CREATE TABLE fiscal_documents (
    document_id                BLOB    PRIMARY KEY  CHECK (length(document_id) = 16),
    request_id                 BLOB    NOT NULL UNIQUE  CHECK (length(request_id) = 16),
    fiscal_number              TEXT    NOT NULL,
    shift_id                   BLOB,
    offline_session_id         BLOB,
    lnd                        INTEGER NOT NULL  CHECK (lnd >= 1),
    doc_type                   TEXT    NOT NULL  CHECK (doc_type IN (
        'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
        'CASH_WITHDRAWAL','X_REPORT','Z_REPORT'
    )),
    state                      TEXT    NOT NULL  CHECK (state IN (
        'PREPARED','SIGNED','ENCRYPTED','SENT','KVT1','KVT2','ACK',
        'OFFLINE_LOCAL_ACK','REJECTED','CANCELLED','ERROR_RETRYABLE',
        'REQUIRES_MANUAL_RECONCILIATION'
    )),
    backend_profile_id         TEXT    NOT NULL,
    transport_profile_id       TEXT    NOT NULL,
    fs_mode                    TEXT    NOT NULL  CHECK (fs_mode IN ('ONLINE','OFFLINE')),
    business_ts                TEXT    NOT NULL,
    server_fiscal_no           TEXT,
    server_fiscal_date         TEXT,
    offline_fiscal_no          INTEGER,
    offline_fiscal_date        TEXT,
    total_sum_kop              INTEGER,
    payload_json               TEXT    NOT NULL,
    payload_sha256_canonical   BLOB    NOT NULL  CHECK (length(payload_sha256_canonical) = 32),
    unsigned_xml_sha256        BLOB    CHECK (unsigned_xml_sha256 IS NULL OR length(unsigned_xml_sha256) = 32),
    previous_hash              BLOB    CHECK (previous_hash IS NULL OR length(previous_hash) = 32),
    submission_attempted_at    TEXT,                              -- spec §5.6 stage gating
    technical_return           INTEGER  CHECK (technical_return IS NULL OR technical_return IN (0,1)),
    related_receipt_id         BLOB,
    created_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (shift_id)     REFERENCES shifts(shift_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX ix_fd_fn_lnd     ON fiscal_documents(fiscal_number, lnd);
CREATE INDEX ix_fd_state_pending ON fiscal_documents(state, created_at)
    WHERE state IN ('PREPARED','SIGNED','ENCRYPTED','SENT','KVT1','ERROR_RETRYABLE');
CREATE INDEX ix_fd_recon_manual ON fiscal_documents(state)
    WHERE state = 'REQUIRES_MANUAL_RECONCILIATION';

CREATE TRIGGER fd_updated_at
AFTER UPDATE ON fiscal_documents
BEGIN
    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;
END;

-- document_files — derivative of fiscal_documents, CASCADE OK
CREATE TABLE document_files (
    document_id BLOB    NOT NULL,
    kind        TEXT    NOT NULL  CHECK (kind IN ('PAYLOAD_XML','SIGNED_XML','KVT1_RAW','KVT2_RAW','PAYLOAD_JSON_CANONICAL','RECEIPT_PDF')),
    content     BLOB    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (document_id, kind),
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE CASCADE
) STRICT;

-- ingress_inbox — operational queue, no FK to fiscal_documents
CREATE TABLE ingress_inbox (
    request_id               BLOB    PRIMARY KEY  CHECK (length(request_id) = 16),
    fiscal_number            TEXT    NOT NULL,
    protocol                 TEXT    NOT NULL  CHECK (protocol IN ('REST','XMLRPC','MARIA','MARIA304','CHECKBOX_COMPAT','INTERNAL')),
    operation_type           TEXT    NOT NULL,
    idempotency_key          TEXT    NOT NULL,
    status                   TEXT    NOT NULL DEFAULT 'NEW'  CHECK (status IN ('NEW','PROCESSING','DONE','REJECTED','ERROR')),
    payload_json             TEXT    NOT NULL,
    payload_sha256_canonical BLOB    NOT NULL  CHECK (length(payload_sha256_canonical) = 32),
    correlation_id           TEXT,
    received_at              TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    processed_at             TEXT,
    error_text               TEXT
) STRICT;

CREATE UNIQUE INDEX ux_inbox_fn_idem ON ingress_inbox(fiscal_number, idempotency_key);
CREATE INDEX ix_inbox_pending ON ingress_inbox(fiscal_number, received_at)
    WHERE status IN ('NEW','PROCESSING');
```

- [ ] **Step 2: Extend `tests/migrations_apply.rs`**

Add a test that queries `PRAGMA table_info(fiscal_documents)` via sqlx and asserts the presence of `unsigned_xml_sha256`, `payload_sha256_canonical`, `submission_attempted_at`. Add another that queries `sqlite_master` for `ux_inbox_fn_idem`.

```bash
cargo test -p prro --test migrations_apply
```

All migration_002_* tests must pass.

- [ ] **Step 3: Commit**

```bash
git add rust/prro/migrations/002_fiscal_documents.sql
git commit -m "feat(rust/db): migration 002 — fiscal_documents + ingress_inbox

Distinct hashes per spec §5.4 — unsigned_xml_sha256 (chain) +
payload_sha256_canonical (idempotency).  submission_attempted_at
column for stage-gated DPS reconciliation per spec §5.6.
ingress_inbox UNIQUE(fn, idem_key) + sha256 column for
Created/Replay/Conflict outcomes per spec §6.2 / decision #31."
git push origin rust-gateway
```

---

## Task 3: Migration 003 — operators, certs, printers, tax/payment defs

**Goal:** Land cashier/cert/printer/tax-payment dictionaries. `sidecar_operators` always sealed (`xor_soft` per spec decision #16) — store hex + per-row salt.

**Files:**
- Create: `rust/prro/migrations/003_operators_printers.sql`

**Acceptance Criteria:**
- [ ] All 5 tables created
- [ ] `sidecar_operators` has `cred_salt` BLOB and `jks_password_hex` TEXT NOT NULL
- [ ] Partial unique index `(fiscal_number, operator_inn) WHERE active = 1`

**Verify:**
```bash
cargo test -p prro --test migrations_apply
```
Add a sub-test asserting `ux_op_fn_inn_active` is present in `sqlite_master`, AND that `operator_certs` allows two rows with the same `fiscal_number` when only one has `active=1` (decision: PK on `ski_hex`, partial unique idx on `(fn) WHERE active=1` — supports rolling cert refresh).

**Steps:**

- [ ] **Step 1: Write migration 003**

Create `rust/prro/migrations/003_operators_printers.sql`:

```sql
-- 003 — operators, certs, printer profiles, tax/payment defs.

CREATE TABLE sidecar_operators (
    id                BLOB    PRIMARY KEY  CHECK (length(id) = 16),
    fiscal_number     TEXT    NOT NULL,
    operator_name     TEXT,
    operator_inn      TEXT    NOT NULL  CHECK (length(operator_inn) = 10 AND operator_inn GLOB '[0-9]*'),
    jks_path          TEXT    NOT NULL,
    jks_password_hex  TEXT    NOT NULL,                  -- always XOR-soft sealed (spec decision #16)
    cred_salt         BLOB    NOT NULL  CHECK (length(cred_salt) = 16),
    active            INTEGER NOT NULL DEFAULT 1  CHECK (active IN (0,1)),
    created_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE UNIQUE INDEX ux_op_fn_inn_active
    ON sidecar_operators(fiscal_number, operator_inn) WHERE active = 1;

CREATE TRIGGER ops_updated_at
AFTER UPDATE ON sidecar_operators
BEGIN
    UPDATE sidecar_operators SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- operator_certs: cert cache keyed by ski_hex (cert is uniquely identified
-- by its Subject Key Identifier).  At most one row per FN may carry
-- active=1 — enforced by a partial unique index, not by PK.  This supports
-- rolling refresh: stage a new cert (active=0), then flip in one tx.
CREATE TABLE operator_certs (
    ski_hex          TEXT    PRIMARY KEY  CHECK (length(ski_hex) = 64),
    fiscal_number    TEXT    NOT NULL,
    cert_fingerprint TEXT    NOT NULL,
    cert_der         BLOB    NOT NULL,
    subject_dn       TEXT,
    issuer_dn        TEXT,
    valid_from       TEXT,
    valid_to         TEXT,
    fetched_at       TEXT    NOT NULL,
    source           TEXT    NOT NULL  CHECK (source IN ('container','cmp','manual')),
    active           INTEGER NOT NULL DEFAULT 0  CHECK (active IN (0,1)),
    last_refresh_at  TEXT,
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;
CREATE INDEX ix_op_certs_fn ON operator_certs(fiscal_number);
CREATE UNIQUE INDEX ux_op_certs_active_per_fn
    ON operator_certs(fiscal_number) WHERE active = 1;

CREATE TABLE cert_provisioning_config (
    id                  INTEGER PRIMARY KEY  CHECK (id = 1),
    primary_cmp_url     TEXT    NOT NULL DEFAULT 'http://acskidd.gov.ua:80',
    fallback_cmp_url    TEXT,
    timeout_seconds     INTEGER NOT NULL DEFAULT 10,
    cache_ttl_seconds   INTEGER NOT NULL DEFAULT 3600,
    refresh_within_days INTEGER NOT NULL DEFAULT 30,
    updated_at          TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;
INSERT INTO cert_provisioning_config (id) VALUES (1);

CREATE TABLE printer_profiles (
    id                BLOB    PRIMARY KEY  CHECK (length(id) = 16),
    name              TEXT    NOT NULL,
    fiscal_number     TEXT,
    profile_key       TEXT    NOT NULL,
    destination_type  TEXT    NOT NULL  CHECK (destination_type IN ('tcp','serial','usb')),
    host              TEXT,
    port              INTEGER,
    serial_device     TEXT,
    serial_baud       INTEGER,
    usb_vendor_id     INTEGER,
    usb_product_id    INTEGER,
    paper_width_mm    INTEGER NOT NULL DEFAULT 80  CHECK (paper_width_mm IN (58,80,112)),
    timeout_ms        INTEGER NOT NULL DEFAULT 5000,
    active            INTEGER NOT NULL DEFAULT 1  CHECK (active IN (0,1)),
    created_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE SET NULL
) STRICT;

CREATE TABLE tax_group_definitions (
    fiscal_number        TEXT    NOT NULL,
    tax_id               TEXT    NOT NULL,
    name                 TEXT    NOT NULL,
    tax_rate             REAL    NOT NULL DEFAULT 0,
    additional_rate      REAL    NOT NULL DEFAULT 0,
    tax_type             INTEGER NOT NULL DEFAULT 0,
    tax_algorithm        INTEGER NOT NULL DEFAULT 0,
    requires_uktzed      INTEGER NOT NULL DEFAULT 0,
    requires_excise_mark INTEGER NOT NULL DEFAULT 0,
    is_active            INTEGER NOT NULL DEFAULT 1,
    created_at           TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (fiscal_number, tax_id),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE TABLE payment_type_definitions (
    fiscal_number    TEXT    NOT NULL,
    payment_id       TEXT    NOT NULL,
    name             TEXT    NOT NULL,
    payment_kind     TEXT    NOT NULL,
    is_active        INTEGER NOT NULL DEFAULT 1,
    created_at       TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (fiscal_number, payment_id),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;
```

- [ ] **Step 2: Extend `tests/migrations_apply.rs`**

```bash
cargo test -p prro --test migrations_apply
```

Add a sub-test that:
- asserts both `ux_op_fn_inn_active` (operators) and `ux_op_certs_active_per_fn` (certs) exist;
- inserts two `operator_certs` rows with the same `fiscal_number` but different `ski_hex`, only one `active=1` — both inserts succeed;
- attempts to flip the second row to `active=1` while the first is still active — must fail with UNIQUE-constraint error.

- [ ] **Step 3: Commit**

```bash
git add rust/prro/migrations/003_operators_printers.sql
git commit -m "feat(rust/db): migration 003 — operators, certs, printers, tax/pay defs

sidecar_operators always-sealed (jks_password_hex + cred_salt) per
spec decision #16.  Partial unique idx (fn, inn) WHERE active=1
per spec §11."
git push origin rust-gateway
```

---

## Task 4: Migration 004 — offline + routing

**Goal:** Land offline session bookkeeping plus profile / binding tables.

**Files:**
- Create: `rust/prro/migrations/004_offline_and_routing.sql`

**Acceptance Criteria:**
- [ ] `offline_sessions`, `offline_codes`, `prro_bindings`, `backend_profiles`, `transport_profiles` exist
- [ ] `transport_profiles` has `channel_kind` + `test_mode` columns

**Verify:**
```bash
cargo test -p prro --test migrations_apply
```
Add a sub-test asserting `transport_profiles` has both `channel_kind` and `test_mode` columns, and that all 5 routing/offline tables are created.

**Steps:**

- [ ] **Step 1: Write migration 004**

Create `rust/prro/migrations/004_offline_and_routing.sql`:

```sql
-- 004 — offline + routing.

CREATE TABLE offline_sessions (
    offline_session_id BLOB    PRIMARY KEY  CHECK (length(offline_session_id) = 16),
    fiscal_number      TEXT    NOT NULL,
    status             TEXT    NOT NULL  CHECK (status IN ('OPENING','OPEN','CLOSING','CLOSED','ABORTED')),
    opened_at          TEXT    NOT NULL,
    closed_at          TEXT,
    last_known_unsigned_xml_sha256 BLOB,
    docs_count         INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE INDEX ix_offline_active ON offline_sessions(fiscal_number, status)
    WHERE status IN ('OPENING','OPEN','CLOSING');

CREATE TABLE offline_codes (
    fiscal_number TEXT    NOT NULL,
    code_value    INTEGER NOT NULL,
    used_at       TEXT,
    used_by_doc   BLOB,
    PRIMARY KEY (fiscal_number, code_value),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;
CREATE INDEX ix_offline_codes_unused ON offline_codes(fiscal_number) WHERE used_at IS NULL;

CREATE TABLE backend_profiles (
    backend_profile_id TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL  CHECK (kind IN ('DPS_PRRO','CHECKBOX','OTHER')),
    config_json        TEXT,
    created_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE TABLE transport_profiles (
    transport_profile_id TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    channel_kind         TEXT NOT NULL  CHECK (channel_kind IN ('grpc_cabinet','edyne_vikno','soap_dps','checkbox_rest','sidecar_v2')),
    test_mode            INTEGER NOT NULL DEFAULT 0  CHECK (test_mode IN (0,1)),
    config_json          TEXT,
    created_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE TABLE prro_bindings (
    fiscal_number        TEXT PRIMARY KEY,
    backend_profile_id   TEXT NOT NULL,
    transport_profile_id TEXT NOT NULL,
    created_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number)        REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (backend_profile_id)   REFERENCES backend_profiles(backend_profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id) ON DELETE RESTRICT
) STRICT;
```

- [ ] **Step 2: Extend `tests/migrations_apply.rs`**

```bash
cargo test -p prro --test migrations_apply
```

Add a sub-test asserting `transport_profiles` has both `channel_kind` and `test_mode` columns and that all 5 offline/routing tables are present.

- [ ] **Step 3: Commit**

```bash
git add rust/prro/migrations/004_offline_and_routing.sql
git commit -m "feat(rust/db): migration 004 — offline + routing tables

offline_sessions, offline_codes, backend_profiles, transport_profiles
(with channel_kind + test_mode per spec §8.2 + decision #22), and
prro_bindings — strict explicit FN→profile bindings (no soft fallback)."
git push origin rust-gateway
```

---

## Task 5: Migration 005 — licenses

**Goal:** Land `licenses` per spec decision (kept by user for commercial-tier).

**Files:**
- Create: `rust/prro/migrations/005_licenses.sql`

**Acceptance Criteria:**
- [ ] `licenses` table exists with `tier`, `expires_at`, `payload_b64`, `signature_b64` columns

**Verify:**
```bash
cargo test -p prro --test migrations_apply
```
Add a sub-test asserting `licenses` has `tier`, `expires_at`, `payload_b64`, `signature_b64` columns.

**Steps:**

- [ ] **Step 1: Write migration 005**

Create `rust/prro/migrations/005_licenses.sql`:

```sql
-- 005 — licenses (commercial tiering, kept per spec decision).

CREATE TABLE licenses (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    tin              TEXT    NOT NULL,
    fn_numbers_json  TEXT    NOT NULL,
    issued_at        TEXT    NOT NULL,
    expires_at       TEXT    NOT NULL,
    tier             TEXT    NOT NULL  CHECK (tier IN ('demo','basic','pro','enterprise')),
    org_name         TEXT,
    demo_limits_json TEXT,
    payload_b64      TEXT    NOT NULL,
    signature_b64    TEXT    NOT NULL,
    installed_at     TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    active           INTEGER NOT NULL DEFAULT 1  CHECK (active IN (0,1))
) STRICT;

-- At most one active license at a time.
CREATE UNIQUE INDEX ux_lic_active ON licenses(active) WHERE active = 1;
```

- [ ] **Step 2: Extend `tests/migrations_apply.rs`**

```bash
cargo test -p prro --test migrations_apply
```

Add a sub-test asserting `licenses` has `tier`, `expires_at`, `payload_b64`, `signature_b64` columns.

- [ ] **Step 3: Commit**

```bash
git add rust/prro/migrations/005_licenses.sql
git commit -m "feat(rust/db): migration 005 — licenses

Commercial-tier licensing kept per user decision in spec §2 #..."
git push origin rust-gateway
```

---

## Task 6: db::tx::with_immediate helper + single-writer regression test

**Goal:** Implement the spec-decision #39 transaction primitive and prove via test that two racing tasks contend correctly on RESERVED lock.

**Files:**
- Modify: `rust/prro/src/db/mod.rs`
- Modify: `rust/prro/src/db/tx.rs`
- Create: `rust/prro/tests/tx_with_immediate_lock.rs`

**Acceptance Criteria:**
- [ ] `db::tx::with_immediate(pool, |conn| async {...})` compiles and runs
- [ ] `commit` happens on Ok, `rollback` on Err
- [ ] Two concurrent tasks contend — second blocks until first commits

**Verify:** `cd rust/prro && cargo test --test tx_with_immediate_lock` → passes (2 tests)

**Steps:**

- [ ] **Step 1: `db::open_pool` (already landed in Task 1)**

The `open_pool` helper was introduced earlier in Task 1 so the migration tests have a single verification path. Reuse it here — no edits to `db/mod.rs` are needed in this task except to add the `tx` re-export if not yet present.

- [ ] **Step 2: Implement `with_immediate`**

Replace `rust/prro/src/db/tx.rs` with:

```rust
//! Single source of truth for write transactions.
//!
//! `pool.begin()` opens BEGIN DEFERRED; nesting BEGIN IMMEDIATE inside
//! is a SQLite error.  This helper acquires a raw connection and
//! issues `BEGIN IMMEDIATE` directly, ensuring writers contend on the
//! RESERVED lock from the very first statement (spec decision #39).

use futures::future::BoxFuture;
use sqlx::{SqliteConnection, SqlitePool};

pub async fn with_immediate<R, F>(pool: &SqlitePool, f: F) -> anyhow::Result<R>
where
    F: for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, anyhow::Result<R>> + Send,
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

Add `futures = "0.3"` to `[dependencies]` in `rust/prro/Cargo.toml`.

- [ ] **Step 3: Write contention test**

Create `rust/prro/tests/tx_with_immediate_lock.rs`:

```rust
//! Verifies two concurrent writers correctly contend on
//! `with_immediate`'s RESERVED lock.

use prro::db::{open_pool, tx::with_immediate};
use std::time::{Duration, Instant};

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contention.sqlite3");
    // Leak the tempdir so the file persists for the test duration.
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_writers_serialize() {
    let pool = fresh_pool().await;
    let p1 = pool.clone();
    let p2 = pool.clone();

    let started_at = Instant::now();

    // Writer 1 holds the lock for 200 ms, then commits.
    let t1 = tokio::spawn(async move {
        with_immediate(&p1, |conn| Box::pin(async move {
            sqlx::query(
                "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
                 VALUES ('1111111111', '12345678', 'test')")
                .execute(&mut *conn).await?;
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok::<_, anyhow::Error>(started_at.elapsed())
        })).await
    });

    // Writer 2 starts ~50 ms after Writer 1 — should block until t1 commits.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let t2 = tokio::spawn(async move {
        with_immediate(&p2, |conn| Box::pin(async move {
            sqlx::query(
                "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
                 VALUES ('2222222222', '12345678', 'test')")
                .execute(&mut *conn).await?;
            Ok::<_, anyhow::Error>(started_at.elapsed())
        })).await
    });

    let elapsed1 = t1.await.unwrap().unwrap();
    let elapsed2 = t2.await.unwrap().unwrap();

    // Writer 1 finished after at least 200 ms;
    // Writer 2 finished after Writer 1.
    assert!(elapsed1.as_millis() >= 200, "writer 1 elapsed {:?}", elapsed1);
    assert!(elapsed2 >= elapsed1, "writer 2 ({:?}) must finish after writer 1 ({:?})",
            elapsed2, elapsed1);
}

#[tokio::test]
async fn rollback_on_error() {
    let pool = fresh_pool().await;
    let result: anyhow::Result<()> = with_immediate(&pool, |conn| Box::pin(async move {
        sqlx::query(
            "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
             VALUES ('3333333333', '12345678', 'test')")
            .execute(&mut *conn).await?;
        Err(anyhow::anyhow!("simulated failure"))
    })).await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_number_config WHERE fiscal_number = '3333333333'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "rollback must remove the inserted row");
}
```

- [ ] **Step 4: Run + verify**

```bash
cd rust/prro
cargo test --test tx_with_immediate_lock -- --nocapture
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add rust/prro/Cargo.toml rust/prro/src/db/mod.rs rust/prro/src/db/tx.rs rust/prro/tests/tx_with_immediate_lock.rs
git commit -m "feat(rust/db): with_immediate + single-writer contention test

Per spec decision #39 — never nest BEGIN IMMEDIATE inside pool.begin().
Helper acquires raw conn, manages BEGIN/COMMIT/ROLLBACK explicitly.
Test asserts: (1) two writers serialize on RESERVED lock,
(2) rollback on error removes the inserted row."
git push origin rust-gateway
```

---

## Task 7: UUIDv7 helpers + sqlx::Type enum wrappers

**Goal:** Type-safe ids (`DocumentId`, `ShiftId`, …) backed by UUIDv7 BLOB; sqlx::Type wrappers for `DocState`, `ShiftState`, `NodeMode`, `Protocol`, `DocType`, `FiscalMode`, `Severity`.

**Files:**
- Modify: `rust/prro/src/db/models/mod.rs`
- Create: `rust/prro/src/db/models/ids.rs`
- Create: `rust/prro/src/db/models/enums.rs`

**Acceptance Criteria:**
- [ ] `DocumentId::new()` produces UUIDv7 (monotonic, 16 bytes)
- [ ] All enums implement `sqlx::Type<Sqlite>` + `Encode`/`Decode` for TEXT
- [ ] `cargo test --test models_smoke` passes

**Verify:** `cargo test --test models_smoke` → 5+ tests pass

**Steps:**

- [ ] **Step 1: Implement `ids.rs`**

Create `rust/prro/src/db/models/ids.rs`:

```rust
//! Strongly-typed UUIDv7 BLOB ids.

use serde::{Deserialize, Serialize};
use sqlx::sqlite::Sqlite;
use sqlx::{Decode, Encode, Type};
use uuid::Uuid;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self { Self(Uuid::now_v7()) }
            pub fn from_bytes(b: [u8; 16]) -> Self { Self(Uuid::from_bytes(b)) }
            pub fn as_bytes(&self) -> &[u8; 16] { self.0.as_bytes() }
        }

        impl Default for $name {
            fn default() -> Self { Self::new() }
        }

        impl Type<Sqlite> for $name {
            fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
                <Vec<u8> as Type<Sqlite>>::type_info()
            }
        }

        impl<'q> Encode<'q, Sqlite> for $name {
            fn encode_by_ref(&self,
                buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <&[u8] as Encode<Sqlite>>::encode(self.0.as_bytes(), buf)
            }
        }

        impl<'r> Decode<'r, Sqlite> for $name {
            fn decode(value: <Sqlite as sqlx::Database>::ValueRef<'r>)
                -> Result<Self, sqlx::error::BoxDynError>
            {
                let bytes = <Vec<u8> as Decode<Sqlite>>::decode(value)?;
                let array: [u8; 16] = bytes.as_slice().try_into()
                    .map_err(|_| "invalid UUID byte length")?;
                Ok(Self::from_bytes(array))
            }
        }
    };
}

id_newtype!(DocumentId);
id_newtype!(RequestId);
id_newtype!(ShiftId);
id_newtype!(OperatorId);
id_newtype!(PrinterId);
id_newtype!(OfflineSessionId);
```

- [ ] **Step 2: Implement `enums.rs`**

Create `rust/prro/src/db/models/enums.rs`:

```rust
//! sqlx::Type wrappers for state-machine and protocol enums.
//!
//! Stored as TEXT in SQLite (matches the CHECK lists in migrations),
//! deserialized into typed Rust enums in repository layer.

use serde::{Deserialize, Serialize};
use sqlx::Type;

macro_rules! str_enum {
    ($name:ident { $( $variant:ident => $sql:literal ),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
        #[sqlx(type_name = "TEXT")]
        pub enum $name {
            $(
                #[sqlx(rename = $sql)]
                #[serde(rename = $sql)]
                $variant,
            )+
        }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $( Self::$variant => $sql, )+ }
            }
        }
    };
}

str_enum!(DocState {
    Prepared                    => "PREPARED",
    Signed                      => "SIGNED",
    Encrypted                   => "ENCRYPTED",
    Sent                        => "SENT",
    Kvt1                        => "KVT1",
    Kvt2                        => "KVT2",
    Ack                         => "ACK",
    OfflineLocalAck             => "OFFLINE_LOCAL_ACK",
    Rejected                    => "REJECTED",
    Cancelled                   => "CANCELLED",
    ErrorRetryable              => "ERROR_RETRYABLE",
    RequiresManualReconciliation => "REQUIRES_MANUAL_RECONCILIATION",
});

str_enum!(ShiftState {
    Created => "CREATED",
    Opening => "OPENING",
    Opened  => "OPENED",
    Closing => "CLOSING",
    Closed  => "CLOSED",
    Error   => "ERROR",
});

str_enum!(NodeMode {
    Online         => "ONLINE",
    GoingOffline   => "GOING_OFFLINE",
    Offline        => "OFFLINE",
    GoingOnline    => "GOING_ONLINE",
    Blocked        => "BLOCKED",
    StopMode       => "STOP_MODE",
    CryptoDegraded => "CRYPTO_DEGRADED",
});

str_enum!(Protocol {
    Rest           => "REST",
    XmlRpc         => "XMLRPC",
    Maria          => "MARIA",
    Maria304       => "MARIA304",
    CheckboxCompat => "CHECKBOX_COMPAT",
    Internal       => "INTERNAL",
});

str_enum!(DocType {
    ShiftOpen      => "SHIFT_OPEN",
    ShiftClose     => "SHIFT_CLOSE",
    Sell           => "SELL",
    Return         => "RETURN",
    ServiceIn      => "SERVICE_IN",
    ServiceOut     => "SERVICE_OUT",
    CashWithdrawal => "CASH_WITHDRAWAL",
    XReport        => "X_REPORT",
    ZReport        => "Z_REPORT",
});

str_enum!(FiscalMode {
    Test => "test",
    Prod => "prod",
});

str_enum!(Severity {
    Info     => "INFO",
    Warning  => "WARNING",
    Error    => "ERROR",
    Critical => "CRITICAL",
});

str_enum!(InboxStatus {
    New        => "NEW",
    Processing => "PROCESSING",
    Done       => "DONE",
    Rejected   => "REJECTED",
    Error      => "ERROR",
});
```

- [ ] **Step 3: Re-export from `models/mod.rs`**

Replace `rust/prro/src/db/models/mod.rs` with:

```rust
pub mod ids;
pub mod enums;

pub use ids::*;
pub use enums::*;
```

- [ ] **Step 4: Smoke test**

Create `rust/prro/tests/models_smoke.rs`:

```rust
use prro::db::models::*;

#[test]
fn document_id_roundtrip() {
    let a = DocumentId::new();
    assert_eq!(a.as_bytes().len(), 16);
    let b = DocumentId::from_bytes(*a.as_bytes());
    assert_eq!(a, b);
}

#[test]
fn document_id_monotonic() {
    let a = DocumentId::new();
    let b = DocumentId::new();
    // UUIDv7 is monotonic by construction (time-ordered).
    assert!(b.as_bytes() >= a.as_bytes(), "two new UUIDv7 should be monotonic");
}

#[test]
fn enum_as_str() {
    assert_eq!(DocState::Prepared.as_str(), "PREPARED");
    assert_eq!(ShiftState::Opened.as_str(), "OPENED");
    assert_eq!(FiscalMode::Test.as_str(), "test");
    assert_eq!(Protocol::Maria304.as_str(), "MARIA304");
}

#[tokio::test]
async fn enum_sqlx_roundtrip() {
    let pool = prro::db::open_pool(&tempfile::tempdir().unwrap().path().join("a.db"))
        .await.unwrap();
    sqlx::query(
        "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, ?, ?)")
        .bind("4444444444")
        .bind("12345678")
        .bind(FiscalMode::Test)
        .execute(&pool).await.unwrap();
    let mode: FiscalMode = sqlx::query_scalar(
        "SELECT fiscal_mode FROM fiscal_number_config WHERE fiscal_number = '4444444444'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(mode, FiscalMode::Test);
}

#[tokio::test]
async fn document_id_sqlx_roundtrip() {
    let pool = prro::db::open_pool(&tempfile::tempdir().unwrap().path().join("b.db"))
        .await.unwrap();
    // Insert minimal fn_config so shifts FK passes.
    sqlx::query(
        "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
         VALUES ('5555555555', '12345678', 'test')")
        .execute(&pool).await.unwrap();
    let id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, next_lnd, \
            opened_via_backend_profile_id, opened_via_transport_profile_id) \
         VALUES (?, '5555555555', 'CREATED', 'ONLINE', 1, 'b', 't')")
        .bind(id)
        .execute(&pool).await.unwrap();
    let got: ShiftId = sqlx::query_scalar(
        "SELECT shift_id FROM shifts WHERE fiscal_number = '5555555555'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(id, got);
}
```

> **Note:** `shifts` schema in migration 001 doesn't include `opened_via_backend_profile_id` columns — the test SQL above is illustrative. If migration 001 didn't carry those columns (they're not in the spec sketch), drop them from the INSERT — the test only proves UUIDv7 round-trips.

Adjust the INSERT to match actual migration 001:

```rust
sqlx::query(
    "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at, \
        cash_balance_kop) VALUES (?, '5555555555', 'CREATED', 'ONLINE', '2026-04-22T00:00:00Z', 0)")
    .bind(id)
    .execute(&pool).await.unwrap();
```

- [ ] **Step 5: Run + verify**

```bash
cd rust/prro
cargo test --test models_smoke -- --nocapture
```

Expected: 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add rust/prro/src/db/models/ rust/prro/tests/models_smoke.rs
git commit -m "feat(rust/db): UUIDv7 ids + sqlx::Type enum wrappers

Newtypes for DocumentId/RequestId/ShiftId/OperatorId/PrinterId/
OfflineSessionId backed by UUIDv7 BLOB.  String enums for
DocState/ShiftState/NodeMode/Protocol/DocType/FiscalMode/Severity/
InboxStatus.  All implement sqlx::Type for SQLite TEXT/BLOB."
git push origin rust-gateway
```

---

## Task 8: FiscalNumberConfigRepo + tests

**Goal:** Async repository for `fiscal_number_config` covering insert / load / update / list. Compile-time SQL via `sqlx::query!` macros.

**Files:**
- Create: `rust/prro/src/db/repositories/fiscal_number_config.rs`
- Modify: `rust/prro/src/db/repositories/mod.rs` (add `pub mod fiscal_number_config;`)
- Create: `rust/prro/tests/repo_fiscal_number_config.rs`

**Acceptance Criteria:**
- [ ] `FnConfig` struct mirrors row, with typed `FiscalMode` enum
- [ ] `insert`, `get_by_fn`, `update`, `list_all` functions
- [ ] Test covers each public method

**Verify:** `cargo test --test repo_fiscal_number_config` → 4+ tests pass

**Steps:**

- [ ] **Step 1: Generate offline SQL data for sqlx**

Add to `rust/prro/Cargo.toml` `[package.metadata]`:

```toml
[package.metadata.sqlx]
# Use offline mode in CI; locally we run with DATABASE_URL.
```

Set up `.env.example` and add `.env` to `.gitignore`:

```bash
echo 'DATABASE_URL=sqlite:./var/prro.dev.db' > rust/prro/.env.example
echo '.env' >> rust/prro/.gitignore
```

For local dev (no `sqlx-cli` required — bootstrap the dev DB via our own
`open_pool`, which already runs `sqlx::migrate!()`):

Add `rust/prro/examples/bootstrap_dev_db.rs`:
```rust
//! One-off helper: create `var/prro.dev.db` and apply migrations so that
//! `sqlx::query!` macros can compile against the dev schema.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    std::fs::create_dir_all("var")?;
    let _pool = prro::db::open_pool(std::path::Path::new("var/prro.dev.db")).await?;
    println!("dev DB ready: var/prro.dev.db");
    Ok(())
}
```

Run from `rust/prro/`:
```bash
cargo run --example bootstrap_dev_db
echo "DATABASE_URL=sqlite:./var/prro.dev.db" > .env
```

For `cargo sqlx prepare` (committing the `.sqlx/` offline cache used by Task
16 CI), `sqlx-cli` is needed *once*: `cargo install sqlx-cli --no-default-features --features sqlite`. Day-to-day dev uses `cargo build/test` only.

- [ ] **Step 2: Write `FnConfig` struct + repo functions**

Create `rust/prro/src/db/repositories/fiscal_number_config.rs`:

```rust
//! Repository for `fiscal_number_config`.

use crate::db::models::enums::FiscalMode;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct FnConfig {
    pub fiscal_number: String,
    pub tax_number: String,
    pub vat_payer_inn: Option<String>,
    pub fiscal_mode: FiscalMode,
    pub org_name: Option<String>,
    pub point_name: Option<String>,
    pub org_address: Option<String>,
    pub tsp_enabled: bool,
    pub offline_enabled: bool,
    pub min_offline_codes: i64,
    pub max_offline_codes: i64,
}

#[derive(Debug, Clone)]
pub struct NewFnConfig {
    pub fiscal_number: String,
    pub tax_number: String,
    pub vat_payer_inn: Option<String>,
    pub fiscal_mode: FiscalMode,
    pub org_name: Option<String>,
    pub point_name: Option<String>,
    pub org_address: Option<String>,
    pub tsp_enabled: bool,
    pub offline_enabled: bool,
    pub min_offline_codes: i64,
    pub max_offline_codes: i64,
}

pub async fn insert(pool: &SqlitePool, n: &NewFnConfig) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO fiscal_number_config (
             fiscal_number, tax_number, vat_payer_inn, fiscal_mode,
             org_name, point_name, org_address,
             tsp_enabled, offline_enabled, min_offline_codes, max_offline_codes
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
        .bind(&n.fiscal_number)
        .bind(&n.tax_number)
        .bind(n.vat_payer_inn.as_deref())
        .bind(n.fiscal_mode)
        .bind(n.org_name.as_deref())
        .bind(n.point_name.as_deref())
        .bind(n.org_address.as_deref())
        .bind(n.tsp_enabled as i64)
        .bind(n.offline_enabled as i64)
        .bind(n.min_offline_codes)
        .bind(n.max_offline_codes)
        .execute(pool).await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Option<FnConfig>> {
    let row = sqlx::query!(
        r#"SELECT fiscal_number,
                  tax_number,
                  vat_payer_inn,
                  fiscal_mode    as "fiscal_mode: FiscalMode",
                  org_name, point_name, org_address,
                  tsp_enabled    as "tsp_enabled: i64",
                  offline_enabled as "offline_enabled: i64",
                  min_offline_codes  as "min_offline_codes: i64",
                  max_offline_codes  as "max_offline_codes: i64"
           FROM fiscal_number_config WHERE fiscal_number = ?"#,
        fn_id
    ).fetch_optional(pool).await?;
    Ok(row.map(|r| FnConfig {
        fiscal_number: r.fiscal_number,
        tax_number: r.tax_number,
        vat_payer_inn: r.vat_payer_inn,
        fiscal_mode: r.fiscal_mode,
        org_name: r.org_name,
        point_name: r.point_name,
        org_address: r.org_address,
        tsp_enabled: r.tsp_enabled != 0,
        offline_enabled: r.offline_enabled != 0,
        min_offline_codes: r.min_offline_codes,
        max_offline_codes: r.max_offline_codes,
    }))
}

pub async fn list_all(pool: &SqlitePool) -> sqlx::Result<Vec<FnConfig>> {
    let rows = sqlx::query!(
        r#"SELECT fiscal_number,
                  tax_number,
                  vat_payer_inn,
                  fiscal_mode    as "fiscal_mode: FiscalMode",
                  org_name, point_name, org_address,
                  tsp_enabled    as "tsp_enabled: i64",
                  offline_enabled as "offline_enabled: i64",
                  min_offline_codes  as "min_offline_codes: i64",
                  max_offline_codes  as "max_offline_codes: i64"
           FROM fiscal_number_config ORDER BY fiscal_number"#
    ).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| FnConfig {
        fiscal_number: r.fiscal_number,
        tax_number: r.tax_number,
        vat_payer_inn: r.vat_payer_inn,
        fiscal_mode: r.fiscal_mode,
        org_name: r.org_name,
        point_name: r.point_name,
        org_address: r.org_address,
        tsp_enabled: r.tsp_enabled != 0,
        offline_enabled: r.offline_enabled != 0,
        min_offline_codes: r.min_offline_codes,
        max_offline_codes: r.max_offline_codes,
    }).collect())
}

pub async fn update_org_metadata(
    pool: &SqlitePool, fn_id: &str,
    org_name: Option<&str>, point_name: Option<&str>, org_address: Option<&str>,
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE fiscal_number_config
         SET org_name = ?, point_name = ?, org_address = ?
         WHERE fiscal_number = ?"
    )
        .bind(org_name).bind(point_name).bind(org_address)
        .bind(fn_id)
        .execute(pool).await?;
    Ok(res.rows_affected())
}
```

Add to `rust/prro/src/db/repositories/mod.rs`:

```rust
pub mod fiscal_number_config;
```

- [ ] **Step 3: Generate `sqlx-data.json` for offline build**

```bash
cd rust/prro
DATABASE_URL=sqlite:./var/prro.dev.db cargo sqlx prepare
git add .sqlx
```

(creates `.sqlx/` cache directory; required for `SQLX_OFFLINE=true` CI builds.)

- [ ] **Step 4: Write tests**

Create `rust/prro/tests/repo_fiscal_number_config.rs`:

```rust
use prro::db::{open_pool, models::enums::FiscalMode,
               repositories::fiscal_number_config as repo};

async fn fresh() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

fn sample(fn_id: &str) -> repo::NewFnConfig {
    repo::NewFnConfig {
        fiscal_number: fn_id.to_string(),
        tax_number: "12345678".to_string(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: Some("ТОВ Демо".to_string()),
        point_name: Some("Точка #1".to_string()),
        org_address: Some("м. Київ".to_string()),
        tsp_enabled: false,
        offline_enabled: true,
        min_offline_codes: 0,
        max_offline_codes: 0,
    }
}

#[tokio::test]
async fn insert_and_get_roundtrip() {
    let pool = fresh().await;
    repo::insert(&pool, &sample("4000000001")).await.unwrap();
    let got = repo::get(&pool, "4000000001").await.unwrap().unwrap();
    assert_eq!(got.tax_number, "12345678");
    assert_eq!(got.fiscal_mode, FiscalMode::Test);
    assert_eq!(got.org_name.as_deref(), Some("ТОВ Демо"));
    assert!(got.offline_enabled);
}

#[tokio::test]
async fn get_missing_returns_none() {
    let pool = fresh().await;
    let got = repo::get(&pool, "9999999999").await.unwrap();
    assert!(got.is_none());
}

#[tokio::test]
async fn list_returns_sorted() {
    let pool = fresh().await;
    repo::insert(&pool, &sample("4000000003")).await.unwrap();
    repo::insert(&pool, &sample("4000000001")).await.unwrap();
    repo::insert(&pool, &sample("4000000002")).await.unwrap();
    let all = repo::list_all(&pool).await.unwrap();
    let nums: Vec<&str> = all.iter().map(|r| r.fiscal_number.as_str()).collect();
    assert_eq!(nums, vec!["4000000001", "4000000002", "4000000003"]);
}

#[tokio::test]
async fn update_org_metadata_works() {
    let pool = fresh().await;
    repo::insert(&pool, &sample("4000000010")).await.unwrap();
    let n = repo::update_org_metadata(
        &pool, "4000000010", Some("New name"), Some("New point"), Some("New addr")
    ).await.unwrap();
    assert_eq!(n, 1);
    let got = repo::get(&pool, "4000000010").await.unwrap().unwrap();
    assert_eq!(got.org_name.as_deref(), Some("New name"));
    assert_eq!(got.point_name.as_deref(), Some("New point"));
    assert_eq!(got.org_address.as_deref(), Some("New addr"));
}
```

- [ ] **Step 5: Run + verify**

```bash
cd rust/prro
cargo test --test repo_fiscal_number_config -- --nocapture
```

Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add rust/prro/Cargo.toml rust/prro/.env.example rust/prro/.gitignore \
        rust/prro/src/db/repositories/ rust/prro/.sqlx/ \
        rust/prro/tests/repo_fiscal_number_config.rs
git commit -m "feat(rust/db): FiscalNumberConfigRepo + tests

Async repo for fn_config: insert / get / list_all / update_org_metadata.
Compile-time SQL via sqlx::query!.  4 unit tests cover each method."
git push origin rust-gateway
```

---

## Task 9: ShiftsRepo + state guard tests

**Goal:** Async repository for `shifts` with allowed-transition guard. Returns typed `ShiftRow`. State transitions go through CAS UPDATE (`UPDATE ... WHERE state = expected`).

**Files:**
- Create: `rust/prro/src/db/repositories/shifts.rs`
- Modify: `rust/prro/src/db/repositories/mod.rs`
- Create: `rust/prro/tests/repo_shifts.rs`

**Acceptance Criteria:**
- [ ] `insert_created` / `transition` / `get` functions
- [ ] `transition` performs CAS — unauthorized transition returns 0 rows
- [ ] At least one test asserts forbidden transitions are blocked

**Verify:** `cargo test --test repo_shifts` → 5+ tests pass

**Steps:**

- [ ] **Step 1: Add `shift_state_machine` module**

Create `rust/prro/src/db/repositories/shifts.rs`:

```rust
use crate::db::models::{enums::ShiftState, ids::ShiftId};
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct ShiftRow {
    pub shift_id: ShiftId,
    pub fiscal_number: String,
    pub serial: Option<i64>,
    pub state: ShiftState,
    pub cash_balance_kop: i64,
}

pub fn allowed_transition(from: ShiftState, to: ShiftState) -> bool {
    use ShiftState::*;
    matches!((from, to),
        (Created, Opening) | (Opening, Opened) | (Opening, Error) |
        (Opened, Closing) | (Closing, Closed) | (Closing, Error) |
        (Error, Closed)        // operator-driven recovery close
    )
}

pub async fn insert_created(
    pool: &SqlitePool, id: ShiftId, fiscal_number: &str, open_mode: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop) \
         VALUES (?, ?, 'CREATED', ?, 0)"
    )
        .bind(id).bind(fiscal_number).bind(open_mode)
        .execute(pool).await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, id: ShiftId) -> sqlx::Result<Option<ShiftRow>> {
    let row = sqlx::query!(
        r#"SELECT shift_id      as "shift_id: ShiftId",
                  fiscal_number,
                  serial,
                  state          as "state: ShiftState",
                  cash_balance_kop
           FROM shifts WHERE shift_id = ?"#,
        id
    ).fetch_optional(pool).await?;
    Ok(row.map(|r| ShiftRow {
        shift_id: r.shift_id,
        fiscal_number: r.fiscal_number,
        serial: r.serial,
        state: r.state,
        cash_balance_kop: r.cash_balance_kop,
    }))
}

/// Atomic CAS state transition.  Returns true if exactly one row
/// changed (transition succeeded), false otherwise.  Caller decides
/// what to do on `false` (typically: load current state and decide
/// whether to retry or give up).
///
/// The `allowed_transition` whitelist is enforced in code (cheap)
/// before hitting the DB.
pub async fn transition(
    pool: &SqlitePool, id: ShiftId, from: ShiftState, to: ShiftState,
) -> sqlx::Result<bool> {
    if !allowed_transition(from, to) {
        return Ok(false);
    }
    let res = sqlx::query("UPDATE shifts SET state = ? WHERE shift_id = ? AND state = ?")
        .bind(to).bind(id).bind(from)
        .execute(pool).await?;
    Ok(res.rows_affected() == 1)
}
```

Add to `rust/prro/src/db/repositories/mod.rs`:

```rust
pub mod shifts;
```

- [ ] **Step 2: Re-prepare sqlx data**

```bash
cd rust/prro
DATABASE_URL=sqlite:./var/prro.dev.db cargo sqlx prepare
```

- [ ] **Step 3: Write tests**

Create `rust/prro/tests/repo_shifts.rs`:

```rust
use prro::db::{open_pool,
               models::{enums::ShiftState, ids::ShiftId},
               repositories::{fiscal_number_config as fn_repo,
                              fiscal_number_config::NewFnConfig,
                              shifts}};
use prro::db::models::enums::FiscalMode;

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(&pool, &NewFnConfig {
        fiscal_number: "4000000001".into(),
        tax_number: "12345678".into(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: None, point_name: None, org_address: None,
        tsp_enabled: false, offline_enabled: true,
        min_offline_codes: 0, max_offline_codes: 0,
    }).await.unwrap();
    (pool, "4000000001".to_string())
}

#[tokio::test]
async fn insert_created_then_get() {
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE").await.unwrap();
    let row = shifts::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(row.state, ShiftState::Created);
}

#[tokio::test]
async fn allowed_transitions_succeed() {
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE").await.unwrap();
    assert!(shifts::transition(&pool, id, ShiftState::Created, ShiftState::Opening).await.unwrap());
    assert!(shifts::transition(&pool, id, ShiftState::Opening, ShiftState::Opened).await.unwrap());
    assert_eq!(shifts::get(&pool, id).await.unwrap().unwrap().state, ShiftState::Opened);
}

#[tokio::test]
async fn forbidden_transitions_blocked_in_code() {
    // Code-level whitelist short-circuits BEFORE hitting the DB.
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE").await.unwrap();
    let did_it = shifts::transition(&pool, id, ShiftState::Created, ShiftState::Closed).await.unwrap();
    assert!(!did_it);
    assert_eq!(shifts::get(&pool, id).await.unwrap().unwrap().state, ShiftState::Created);
}

#[tokio::test]
async fn cas_blocks_when_state_diverged() {
    // Allowed transition Opening→Opened, but row is in Created — CAS fails.
    let (pool, fn_id) = fresh_with_fn().await;
    let id = ShiftId::new();
    shifts::insert_created(&pool, id, &fn_id, "ONLINE").await.unwrap();
    let did_it = shifts::transition(&pool, id, ShiftState::Opening, ShiftState::Opened).await.unwrap();
    assert!(!did_it, "CAS must reject when actual state ≠ expected from");
}

#[tokio::test]
async fn allowed_transition_table_matrix() {
    use ShiftState::*;
    // Spot-check the whitelist.
    assert!(shifts::allowed_transition(Created, Opening));
    assert!(shifts::allowed_transition(Opened, Closing));
    assert!(shifts::allowed_transition(Closing, Closed));
    assert!(!shifts::allowed_transition(Closed, Opening));
    assert!(!shifts::allowed_transition(Created, Opened));   // skipped Opening
    assert!(!shifts::allowed_transition(Opened, Created));   // backwards
}
```

- [ ] **Step 4: Run + verify**

```bash
cd rust/prro
cargo test --test repo_shifts -- --nocapture
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust/prro/src/db/repositories/shifts.rs rust/prro/.sqlx/ \
        rust/prro/tests/repo_shifts.rs
git commit -m "feat(rust/db): ShiftsRepo + state-machine guard tests

CAS UPDATE for transitions; code-level allowed_transition() whitelist
short-circuits forbidden transitions before hitting DB."
git push origin rust-gateway
```

---

## Task 10: FiscalDocumentRepo + state CAS test

**Goal:** Most-complex repo. Insert prepared doc, transition state via CAS, list pending docs by FN.

**Files:**
- Create: `rust/prro/src/db/repositories/fiscal_documents.rs`
- Modify: `rust/prro/src/db/repositories/mod.rs`
- Create: `rust/prro/tests/repo_fiscal_documents_state_cas.rs`

**Acceptance Criteria:**
- [ ] `insert_prepared` accepts `NewDocument` with both `payload_sha256_canonical` (NOT NULL) and optional `unsigned_xml_sha256`
- [ ] `transition_state(id, from, to)` CAS-only — refuses non-whitelisted transitions
- [ ] `list_pending_for_fn(fn)` returns rows in non-final states ordered by `created_at`

**Verify:** `cargo test --test repo_fiscal_documents_state_cas` → 4+ tests pass

**Steps:**

- [ ] **Step 1: Implement repo + transition whitelist**

Create `rust/prro/src/db/repositories/fiscal_documents.rs`:

```rust
use crate::db::models::{
    enums::{DocState, DocType},
    ids::{DocumentId, RequestId, ShiftId, OfflineSessionId},
};
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct NewDocument {
    pub document_id: DocumentId,
    pub request_id: RequestId,
    pub fiscal_number: String,
    pub shift_id: Option<ShiftId>,
    pub offline_session_id: Option<OfflineSessionId>,
    pub lnd: i64,
    pub doc_type: DocType,
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    pub fs_mode: &'static str,                 // "ONLINE" | "OFFLINE"
    pub business_ts: String,                   // ISO-8601
    pub total_sum_kop: Option<i64>,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    pub unsigned_xml_sha256: Option<[u8; 32]>,
    pub previous_hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub struct DocumentRow {
    pub document_id: DocumentId,
    pub fiscal_number: String,
    pub state: DocState,
    pub doc_type: DocType,
    pub server_fiscal_no: Option<String>,
    pub submission_attempted_at: Option<String>,
}

pub fn allowed_transition(from: DocState, to: DocState) -> bool {
    use DocState::*;
    matches!((from, to),
        (Prepared, Signed) | (Prepared, Rejected) |
        (Signed, Encrypted) | (Signed, ErrorRetryable) | (Signed, OfflineLocalAck) |
        (Encrypted, Sent) | (Encrypted, ErrorRetryable) |
        (Sent, Kvt1) | (Sent, ErrorRetryable) | (Sent, Rejected) |
        (Kvt1, Kvt2) | (Kvt1, ErrorRetryable) |
        (Kvt2, Ack) |
        (OfflineLocalAck, Sent) |
        (ErrorRetryable, Sent) | (ErrorRetryable, Kvt1) | (ErrorRetryable, RequiresManualReconciliation)
    )
}

pub async fn insert_prepared(pool: &SqlitePool, n: &NewDocument) -> sqlx::Result<()> {
    sqlx::query(
        r#"INSERT INTO fiscal_documents (
             document_id, request_id, fiscal_number, shift_id, offline_session_id,
             lnd, doc_type, state, backend_profile_id, transport_profile_id,
             fs_mode, business_ts, total_sum_kop, payload_json,
             payload_sha256_canonical, unsigned_xml_sha256, previous_hash
         ) VALUES (?, ?, ?, ?, ?, ?, ?, 'PREPARED', ?, ?, ?, ?, ?, ?, ?, ?, ?)"#)
        .bind(n.document_id)
        .bind(n.request_id)
        .bind(&n.fiscal_number)
        .bind(n.shift_id)
        .bind(n.offline_session_id)
        .bind(n.lnd)
        .bind(n.doc_type)
        .bind(&n.backend_profile_id)
        .bind(&n.transport_profile_id)
        .bind(n.fs_mode)
        .bind(&n.business_ts)
        .bind(n.total_sum_kop)
        .bind(&n.payload_json)
        .bind(&n.payload_sha256_canonical[..])
        .bind(n.unsigned_xml_sha256.as_ref().map(|b| &b[..]))
        .bind(n.previous_hash.as_ref().map(|b| &b[..]))
        .execute(pool).await?;
    Ok(())
}

pub async fn transition_state(
    pool: &SqlitePool, id: DocumentId, from: DocState, to: DocState,
) -> sqlx::Result<bool> {
    if !allowed_transition(from, to) {
        return Ok(false);
    }
    let res = sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ? AND state = ?")
        .bind(to).bind(id).bind(from)
        .execute(pool).await?;
    Ok(res.rows_affected() == 1)
}

pub async fn list_pending_for_fn(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Vec<DocumentRow>> {
    let rows = sqlx::query!(
        r#"SELECT document_id    as "document_id: DocumentId",
                  fiscal_number,
                  state           as "state: DocState",
                  doc_type        as "doc_type: DocType",
                  server_fiscal_no,
                  submission_attempted_at
           FROM fiscal_documents
           WHERE fiscal_number = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENT','KVT1','ERROR_RETRYABLE')
           ORDER BY created_at"#,
        fn_id
    ).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| DocumentRow {
        document_id: r.document_id,
        fiscal_number: r.fiscal_number,
        state: r.state,
        doc_type: r.doc_type,
        server_fiscal_no: r.server_fiscal_no,
        submission_attempted_at: r.submission_attempted_at,
    }).collect())
}
```

Add to repositories `mod.rs`:

```rust
pub mod fiscal_documents;
```

Re-prepare sqlx data:

```bash
cd rust/prro
DATABASE_URL=sqlite:./var/prro.dev.db cargo sqlx prepare
```

- [ ] **Step 2: Write tests**

Create `rust/prro/tests/repo_fiscal_documents_state_cas.rs`:

```rust
use prro::db::{open_pool,
               models::{enums::{DocState, DocType, FiscalMode},
                        ids::{DocumentId, RequestId}},
               repositories::{fiscal_number_config as fn_repo,
                              fiscal_number_config::NewFnConfig,
                              fiscal_documents as fd}};

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(&pool, &NewFnConfig {
        fiscal_number: "4000000001".into(),
        tax_number: "12345678".into(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: None, point_name: None, org_address: None,
        tsp_enabled: false, offline_enabled: true,
        min_offline_codes: 0, max_offline_codes: 0,
    }).await.unwrap();
    (pool, "4000000001".to_string())
}

fn sample_doc(fn_id: &str) -> fd::NewDocument {
    fd::NewDocument {
        document_id: DocumentId::new(),
        request_id:  RequestId::new(),
        fiscal_number: fn_id.to_string(),
        shift_id: None,
        offline_session_id: None,
        lnd: 1,
        doc_type: DocType::Sell,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        fs_mode: "ONLINE",
        business_ts: "2026-04-22T12:00:00Z".into(),
        total_sum_kop: Some(15000),
        payload_json: r#"{"goods":[]}"#.into(),
        payload_sha256_canonical: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
    }
}

#[tokio::test]
async fn insert_then_transition_signed() {
    let (pool, fn_id) = fresh_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();
    assert!(fd::transition_state(&pool, id, DocState::Prepared, DocState::Signed).await.unwrap());
}

#[tokio::test]
async fn forbidden_transition_blocked_in_code() {
    let (pool, fn_id) = fresh_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();
    // PREPARED → ACK is not allowed.
    assert!(!fd::transition_state(&pool, id, DocState::Prepared, DocState::Ack).await.unwrap());
}

#[tokio::test]
async fn cas_blocks_when_actual_state_diverged() {
    let (pool, fn_id) = fresh_with_fn().await;
    let new = sample_doc(&fn_id);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();
    // Allowed transition Sent→Kvt1, but row is still PREPARED.
    assert!(!fd::transition_state(&pool, id, DocState::Sent, DocState::Kvt1).await.unwrap());
}

#[tokio::test]
async fn list_pending_excludes_final_states() {
    let (pool, fn_id) = fresh_with_fn().await;
    let a = sample_doc(&fn_id);
    let id_a = a.document_id;
    fd::insert_prepared(&pool, &a).await.unwrap();
    fd::transition_state(&pool, id_a, DocState::Prepared, DocState::Signed).await.unwrap();

    let mut b = sample_doc(&fn_id);
    b.lnd = 2;
    let id_b = b.document_id;
    fd::insert_prepared(&pool, &b).await.unwrap();
    fd::transition_state(&pool, id_b, DocState::Prepared, DocState::Rejected).await.unwrap();

    let pending = fd::list_pending_for_fn(&pool, &fn_id).await.unwrap();
    let ids: Vec<_> = pending.iter().map(|r| r.document_id).collect();
    assert_eq!(ids, vec![id_a]);
}
```

- [ ] **Step 3: Run + verify**

```bash
cd rust/prro
cargo test --test repo_fiscal_documents_state_cas -- --nocapture
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/prro/src/db/repositories/fiscal_documents.rs rust/prro/.sqlx/ \
        rust/prro/tests/repo_fiscal_documents_state_cas.rs
git commit -m "feat(rust/db): FiscalDocumentRepo + state-CAS tests

NewDocument carries both payload_sha256_canonical (idempotency,
NOT NULL) and unsigned_xml_sha256 (chain, optional).
transition_state CAS + code-level allowed_transition whitelist.
list_pending_for_fn excludes final states, ordered by created_at."
git push origin rust-gateway
```

---

## Task 11: IngressInboxRepo + Created/Replay/Conflict outcomes

**Goal:** Implement the three-outcome insert per spec §6.2 / decision #31. This is the spec's most subtle correctness gate.

**Files:**
- Create: `rust/prro/src/db/repositories/ingress_inbox.rs`
- Modify: `rust/prro/src/db/repositories/mod.rs`
- Create: `rust/prro/tests/repo_ingress_inbox_idempotency.rs`

**Acceptance Criteria:**
- [ ] First insert returns `Created`
- [ ] Same key + same payload hash returns `Replay`
- [ ] Same key + different payload hash returns `Conflict { existing, submitted }`
- [ ] All branches inside one `with_immediate` transaction

**Verify:** `cargo test --test repo_ingress_inbox_idempotency` → 3 tests pass

**Steps:**

- [ ] **Step 1: Write repo + outcomes enum**

Create `rust/prro/src/db/repositories/ingress_inbox.rs`:

```rust
use crate::db::models::enums::Protocol;
use crate::db::tx::with_immediate;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct InboxRow {
    pub request_id: [u8; 16],
    pub fiscal_number: String,
    pub protocol: Protocol,
    pub operation_type: String,
    pub idempotency_key: String,
    pub status: String,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    pub correlation_id: Option<String>,
    pub received_at: String,
}

#[derive(Debug, Clone)]
pub struct NewInboxEntry {
    pub request_id: [u8; 16],
    pub fiscal_number: String,
    pub protocol: Protocol,
    pub operation_type: String,
    pub idempotency_key: String,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    pub correlation_id: Option<String>,
}

#[derive(Debug)]
pub enum InboxInsertOutcome {
    Created(InboxRow),
    Replay(InboxRow),
    Conflict {
        existing_payload_hash: [u8; 32],
        submitted_payload_hash: [u8; 32],
    },
}

pub async fn insert(pool: &SqlitePool, n: &NewInboxEntry) -> anyhow::Result<InboxInsertOutcome> {
    let n = n.clone();
    with_immediate(pool, move |conn| Box::pin(async move {
        // Step 1: probe existing row by (fn, idem_key).
        let existing = sqlx::query!(
            r#"SELECT request_id      as "request_id: Vec<u8>",
                      fiscal_number,
                      protocol         as "protocol: Protocol",
                      operation_type,
                      idempotency_key,
                      status,
                      payload_json,
                      payload_sha256_canonical as "payload_sha256_canonical: Vec<u8>",
                      correlation_id,
                      received_at
               FROM ingress_inbox
               WHERE fiscal_number = ? AND idempotency_key = ?"#,
            n.fiscal_number, n.idempotency_key
        ).fetch_optional(&mut *conn).await?;

        if let Some(r) = existing {
            let existing_hash: [u8; 32] = r.payload_sha256_canonical.as_slice().try_into()
                .map_err(|_| anyhow::anyhow!("bad sha256 length in inbox row"))?;
            let request_id: [u8; 16] = r.request_id.as_slice().try_into()
                .map_err(|_| anyhow::anyhow!("bad request_id length in inbox row"))?;
            if existing_hash == n.payload_sha256_canonical {
                return Ok(InboxInsertOutcome::Replay(InboxRow {
                    request_id,
                    fiscal_number: r.fiscal_number,
                    protocol: r.protocol,
                    operation_type: r.operation_type,
                    idempotency_key: r.idempotency_key,
                    status: r.status,
                    payload_json: r.payload_json,
                    payload_sha256_canonical: existing_hash,
                    correlation_id: r.correlation_id,
                    received_at: r.received_at,
                }));
            } else {
                return Ok(InboxInsertOutcome::Conflict {
                    existing_payload_hash: existing_hash,
                    submitted_payload_hash: n.payload_sha256_canonical,
                });
            }
        }

        sqlx::query(
            "INSERT INTO ingress_inbox (
                 request_id, fiscal_number, protocol, operation_type,
                 idempotency_key, payload_json, payload_sha256_canonical, correlation_id
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
            .bind(&n.request_id[..])
            .bind(&n.fiscal_number)
            .bind(n.protocol)
            .bind(&n.operation_type)
            .bind(&n.idempotency_key)
            .bind(&n.payload_json)
            .bind(&n.payload_sha256_canonical[..])
            .bind(n.correlation_id.as_deref())
            .execute(&mut *conn).await?;

        Ok(InboxInsertOutcome::Created(InboxRow {
            request_id: n.request_id,
            fiscal_number: n.fiscal_number.clone(),
            protocol: n.protocol,
            operation_type: n.operation_type.clone(),
            idempotency_key: n.idempotency_key.clone(),
            status: "NEW".to_string(),
            payload_json: n.payload_json.clone(),
            payload_sha256_canonical: n.payload_sha256_canonical,
            correlation_id: n.correlation_id.clone(),
            received_at: chrono::Utc::now().to_rfc3339(),
        }))
    })).await
}
```

Add to repositories `mod.rs`:

```rust
pub mod ingress_inbox;
```

Re-prepare sqlx data.

- [ ] **Step 2: Write idempotency tests**

Create `rust/prro/tests/repo_ingress_inbox_idempotency.rs`:

```rust
use prro::db::{open_pool,
               models::enums::{FiscalMode, Protocol},
               repositories::{fiscal_number_config as fn_repo,
                              fiscal_number_config::NewFnConfig,
                              ingress_inbox::{insert, InboxInsertOutcome, NewInboxEntry}}};

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(&pool, &NewFnConfig {
        fiscal_number: "4000000001".into(),
        tax_number: "12345678".into(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: None, point_name: None, org_address: None,
        tsp_enabled: false, offline_enabled: true,
        min_offline_codes: 0, max_offline_codes: 0,
    }).await.unwrap();
    (pool, "4000000001".to_string())
}

fn entry(fn_id: &str, idem: &str, hash: [u8; 32]) -> NewInboxEntry {
    NewInboxEntry {
        request_id: uuid::Uuid::now_v7().into_bytes(),
        fiscal_number: fn_id.to_string(),
        protocol: Protocol::Rest,
        operation_type: "SELL".into(),
        idempotency_key: idem.into(),
        payload_json: r#"{"x":1}"#.into(),
        payload_sha256_canonical: hash,
        correlation_id: None,
    }
}

#[tokio::test]
async fn first_insert_creates() {
    let (pool, fn_id) = fresh_with_fn().await;
    let outcome = insert(&pool, &entry(&fn_id, "k1", [1u8; 32])).await.unwrap();
    assert!(matches!(outcome, InboxInsertOutcome::Created(_)));
}

#[tokio::test]
async fn second_with_same_hash_replays() {
    let (pool, fn_id) = fresh_with_fn().await;
    let _ = insert(&pool, &entry(&fn_id, "k1", [1u8; 32])).await.unwrap();
    let outcome = insert(&pool, &entry(&fn_id, "k1", [1u8; 32])).await.unwrap();
    assert!(matches!(outcome, InboxInsertOutcome::Replay(_)));
}

#[tokio::test]
async fn second_with_different_hash_conflicts() {
    let (pool, fn_id) = fresh_with_fn().await;
    let _ = insert(&pool, &entry(&fn_id, "k1", [1u8; 32])).await.unwrap();
    let outcome = insert(&pool, &entry(&fn_id, "k1", [2u8; 32])).await.unwrap();
    match outcome {
        InboxInsertOutcome::Conflict { existing_payload_hash, submitted_payload_hash } => {
            assert_eq!(existing_payload_hash[0], 1);
            assert_eq!(submitted_payload_hash[0], 2);
        }
        other => panic!("expected Conflict, got {:?}", other),
    }
}
```

- [ ] **Step 3: Run + verify**

```bash
cd rust/prro
cargo test --test repo_ingress_inbox_idempotency -- --nocapture
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/prro/src/db/repositories/ingress_inbox.rs rust/prro/.sqlx/ \
        rust/prro/tests/repo_ingress_inbox_idempotency.rs
git commit -m "feat(rust/db): IngressInboxRepo with Created/Replay/Conflict outcomes

Implements spec §6.2 / decision #31 — same (fn, idem_key) +
matching payload_sha256_canonical = Replay; differing hash =
Conflict, never silent replay.  Uses with_immediate for
serialised probe-then-insert."
git push origin rust-gateway
```

---

## Task 12: AuditLogRepo

**Goal:** Append-only audit log with severity + actor.

**Files:**
- Create: `rust/prro/src/db/repositories/audit_log.rs`
- Modify: `rust/prro/src/db/repositories/mod.rs`
- Create: `rust/prro/tests/repo_audit_log.rs`

**Acceptance Criteria:**
- [ ] `append(entity_type, entity_id, event_type, severity, actor, payload_json)` returns inserted `audit_id`
- [ ] `list_for_entity(entity_type, entity_id, limit)` returns rows ordered DESC

**Verify:** `cargo test --test repo_audit_log` → 2 tests pass

**Steps:**

- [ ] **Step 1: Implement repo**

Create `rust/prro/src/db/repositories/audit_log.rs`:

```rust
use crate::db::models::enums::Severity;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub audit_id: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub event_type: String,
    pub severity: Severity,
    pub actor: Option<String>,
    pub event_payload_json: Option<String>,
    pub created_at: String,
}

pub async fn append(
    pool: &SqlitePool,
    entity_type: &str, entity_id: &str, event_type: &str,
    severity: Severity, actor: Option<&str>, payload_json: Option<&str>,
) -> sqlx::Result<i64> {
    let res = sqlx::query(
        "INSERT INTO audit_log (entity_type, entity_id, event_type, severity, actor, event_payload_json) \
         VALUES (?, ?, ?, ?, ?, ?)"
    )
        .bind(entity_type).bind(entity_id).bind(event_type)
        .bind(severity).bind(actor).bind(payload_json)
        .execute(pool).await?;
    Ok(res.last_insert_rowid())
}

pub async fn list_for_entity(
    pool: &SqlitePool, entity_type: &str, entity_id: &str, limit: i64,
) -> sqlx::Result<Vec<AuditEntry>> {
    let rows = sqlx::query!(
        r#"SELECT audit_id, entity_type, entity_id, event_type,
                  severity as "severity: Severity",
                  actor, event_payload_json, created_at
           FROM audit_log
           WHERE entity_type = ? AND entity_id = ?
           ORDER BY audit_id DESC
           LIMIT ?"#,
        entity_type, entity_id, limit
    ).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| AuditEntry {
        audit_id: r.audit_id.unwrap_or_default(),
        entity_type: r.entity_type,
        entity_id: r.entity_id,
        event_type: r.event_type,
        severity: r.severity,
        actor: r.actor,
        event_payload_json: r.event_payload_json,
        created_at: r.created_at,
    }).collect())
}
```

Add to `mod.rs`:

```rust
pub mod audit_log;
```

Re-prepare sqlx data.

- [ ] **Step 2: Write tests**

Create `rust/prro/tests/repo_audit_log.rs`:

```rust
use prro::db::{open_pool, models::enums::Severity, repositories::audit_log as al};

async fn fresh() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

#[tokio::test]
async fn append_and_list() {
    let pool = fresh().await;
    let id1 = al::append(&pool, "fn", "4000000001", "fn_registered",
        Severity::Info, Some("admin_ui"), Some(r#"{"mode":"test"}"#)).await.unwrap();
    let id2 = al::append(&pool, "fn", "4000000001", "fn_updated",
        Severity::Info, Some("admin_ui"), None).await.unwrap();
    assert!(id2 > id1);

    let entries = al::list_for_entity(&pool, "fn", "4000000001", 10).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].audit_id, id2);            // DESC order
    assert_eq!(entries[0].event_type, "fn_updated");
    assert_eq!(entries[1].event_type, "fn_registered");
}

#[tokio::test]
async fn list_unrelated_entity_empty() {
    let pool = fresh().await;
    al::append(&pool, "fn", "4000000001", "x",
        Severity::Info, None, None).await.unwrap();
    let entries = al::list_for_entity(&pool, "shift", "4000000001", 10).await.unwrap();
    assert!(entries.is_empty());
}
```

- [ ] **Step 3: Run + verify**

```bash
cd rust/prro
cargo test --test repo_audit_log -- --nocapture
```

Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/prro/src/db/repositories/audit_log.rs rust/prro/.sqlx/ \
        rust/prro/tests/repo_audit_log.rs
git commit -m "feat(rust/db): AuditLogRepo (append + list_for_entity)

Append-only log per spec §4 — no FK to entities.  Severity enum +
optional actor + JSON payload.  list_for_entity returns DESC by audit_id."
git push origin rust-gateway
```

---

## Task 13: NodeStateRepo (mode + last_known_unsigned_xml_sha256 helpers)

**Goal:** Single-row-per-FN state. Includes `seed_prevhash` for the `prro fn seed-prevhash` CLI command (spec §5.4).

**Files:**
- Create: `rust/prro/src/db/repositories/node_state.rs`
- Modify: `rust/prro/src/db/repositories/mod.rs`
- Create: `rust/prro/tests/repo_node_state.rs`

**Acceptance Criteria:**
- [ ] `upsert_initial(fn, mode, shift_state)` creates if missing
- [ ] `seed_prevhash(fn, hash)` updates `last_known_unsigned_xml_sha256`
- [ ] `get(fn)` returns typed row

**Verify:** `cargo test --test repo_node_state` → 3 tests pass

**Steps:**

- [ ] **Step 1: Implement repo**

Create `rust/prro/src/db/repositories/node_state.rs`:

```rust
use crate::db::models::enums::{NodeMode, ShiftState};
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct NodeStateRow {
    pub fiscal_number: String,
    pub mode: NodeMode,
    pub shift_state: ShiftState,
    pub next_lnd: i64,
    pub last_known_unsigned_xml_sha256: Option<[u8; 32]>,
}

pub async fn upsert_initial(
    pool: &SqlitePool, fn_id: &str, mode: NodeMode, shift_state: ShiftState, next_lnd: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(fiscal_number) DO UPDATE SET mode = excluded.mode, shift_state = excluded.shift_state"
    )
        .bind(fn_id).bind(mode).bind(shift_state).bind(next_lnd)
        .execute(pool).await?;
    Ok(())
}

pub async fn seed_prevhash(
    pool: &SqlitePool, fn_id: &str, hash: &[u8; 32],
) -> sqlx::Result<bool> {
    let res = sqlx::query(
        "UPDATE node_state SET last_known_unsigned_xml_sha256 = ? WHERE fiscal_number = ?"
    )
        .bind(&hash[..]).bind(fn_id)
        .execute(pool).await?;
    Ok(res.rows_affected() == 1)
}

pub async fn get(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Option<NodeStateRow>> {
    let row = sqlx::query!(
        r#"SELECT fiscal_number,
                  mode               as "mode: NodeMode",
                  shift_state        as "shift_state: ShiftState",
                  next_lnd,
                  last_known_unsigned_xml_sha256 as "last_known_unsigned_xml_sha256: Vec<u8>"
           FROM node_state WHERE fiscal_number = ?"#,
        fn_id
    ).fetch_optional(pool).await?;
    Ok(row.map(|r| NodeStateRow {
        fiscal_number: r.fiscal_number,
        mode: r.mode,
        shift_state: r.shift_state,
        next_lnd: r.next_lnd,
        last_known_unsigned_xml_sha256: r.last_known_unsigned_xml_sha256
            .and_then(|v| v.as_slice().try_into().ok()),
    }))
}
```

Add to `mod.rs`:

```rust
pub mod node_state;
```

Re-prepare sqlx data.

- [ ] **Step 2: Write tests**

Create `rust/prro/tests/repo_node_state.rs`:

```rust
use prro::db::{open_pool,
               models::enums::{FiscalMode, NodeMode, ShiftState},
               repositories::{fiscal_number_config as fn_repo,
                              fiscal_number_config::NewFnConfig,
                              node_state as ns}};

async fn fresh_with_fn() -> (sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    std::mem::forget(dir);
    let pool = open_pool(&path).await.unwrap();
    fn_repo::insert(&pool, &NewFnConfig {
        fiscal_number: "4000000001".into(),
        tax_number: "12345678".into(), vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: None, point_name: None, org_address: None,
        tsp_enabled: false, offline_enabled: true,
        min_offline_codes: 0, max_offline_codes: 0,
    }).await.unwrap();
    (pool, "4000000001".to_string())
}

#[tokio::test]
async fn upsert_then_get() {
    let (pool, fn_id) = fresh_with_fn().await;
    ns::upsert_initial(&pool, &fn_id, NodeMode::Online, ShiftState::Closed, 1).await.unwrap();
    let row = ns::get(&pool, &fn_id).await.unwrap().unwrap();
    assert_eq!(row.mode, NodeMode::Online);
    assert_eq!(row.shift_state, ShiftState::Closed);
    assert_eq!(row.next_lnd, 1);
    assert!(row.last_known_unsigned_xml_sha256.is_none());
}

#[tokio::test]
async fn seed_prevhash_persists() {
    let (pool, fn_id) = fresh_with_fn().await;
    ns::upsert_initial(&pool, &fn_id, NodeMode::Online, ShiftState::Closed, 1).await.unwrap();
    let h = [0xABu8; 32];
    assert!(ns::seed_prevhash(&pool, &fn_id, &h).await.unwrap());
    let row = ns::get(&pool, &fn_id).await.unwrap().unwrap();
    assert_eq!(row.last_known_unsigned_xml_sha256, Some(h));
}

#[tokio::test]
async fn seed_prevhash_unknown_fn_returns_false() {
    let (pool, _) = fresh_with_fn().await;
    let h = [0xCDu8; 32];
    assert!(!ns::seed_prevhash(&pool, "9999999999", &h).await.unwrap());
}
```

- [ ] **Step 3: Run + verify**

```bash
cd rust/prro
cargo test --test repo_node_state -- --nocapture
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/prro/src/db/repositories/node_state.rs rust/prro/.sqlx/ \
        rust/prro/tests/repo_node_state.rs
git commit -m "feat(rust/db): NodeStateRepo + seed_prevhash helper

upsert_initial / get / seed_prevhash for the chain bootstrap CLI
(prro fn seed-prevhash)."
git push origin rust-gateway
```

---

## Task 14: AppConfig + App composition root

**Goal:** Wire `App { config, db: SqlitePool }`. TOML loading via `serde`. CLI `prro serve --config <path>` boots the App and idles (waits for SIGINT). M3+ wires services into App.

**Files:**
- Create: `rust/prro/src/config/mod.rs`
- Modify: `rust/prro/src/app.rs`
- Modify: `rust/prro/src/main.rs`
- Create: `rust/prro/tests/app_boot.rs`

**Acceptance Criteria:**
- [ ] `AppConfig::from_toml(text)` parses sample TOML
- [ ] `App::boot(cfg)` opens DB pool + applies migrations
- [ ] `prro serve --config <example>` boots, prints `prro listening` log line, exits cleanly on SIGINT

**Verify:** `cargo test --test app_boot` → passes

**Steps:**

- [ ] **Step 1: Write `config/mod.rs`**

```rust
//! AppConfig — TOML, env, CLI overrides.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub app_name: String,
    pub version: String,
    pub database: DatabaseCfg,
    pub admin_ui: AdminUiCfg,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseCfg {
    pub db_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminUiCfg {
    pub enabled: bool,
    pub listen: String,
    #[serde(default)]
    pub keys_dir: Option<PathBuf>,
}

impl AppConfig {
    pub fn from_toml(s: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(s)?)
    }
}
```

- [ ] **Step 2: Write `app.rs`**

```rust
//! Composition root.  M1 wires DB pool + config; M2+ adds crypto,
//! transports, services.

use crate::config::AppConfig;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct App {
    inner: Arc<Inner>,
}

struct Inner {
    pub config: AppConfig,
    pub db: SqlitePool,
}

impl App {
    pub async fn boot(config: AppConfig) -> anyhow::Result<Self> {
        if let Some(parent) = config.database.db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = crate::db::open_pool(&config.database.db_path).await?;
        Ok(Self { inner: Arc::new(Inner { config, db }) })
    }

    pub fn config(&self) -> &AppConfig { &self.inner.config }
    pub fn db(&self) -> &SqlitePool { &self.inner.db }
}
```

- [ ] **Step 3: Update `main.rs` with `serve` + `migrate`**

```rust
use clap::{Parser, Subcommand};
use prro::{App, config::AppConfig};
use std::path::PathBuf;
use tokio::signal;

#[derive(Parser, Debug)]
#[command(name = "prro", version, about = "PRRO Gateway")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Print build info and exit.
    Version,
    /// Apply DB migrations and exit.
    Migrate {
        #[arg(long)]
        config: PathBuf,
    },
    /// Boot the gateway and serve until SIGINT/SIGTERM.
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Version => {
            println!("prro {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Cmd::Migrate { config } => {
            let text = std::fs::read_to_string(&config)?;
            let cfg = AppConfig::from_toml(&text)?;
            let _app = App::boot(cfg).await?;            // boot triggers migrate
            tracing::info!("migrations applied");
            Ok(())
        }
        Cmd::Serve { config } => {
            let text = std::fs::read_to_string(&config)?;
            let cfg = AppConfig::from_toml(&text)?;
            let app = App::boot(cfg).await?;
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "prro listening (M1 — idle)");
            // M3+ adds the supervisor + ingress shells.  M1 just idles.
            signal::ctrl_c().await?;
            tracing::info!("shutting down");
            drop(app);
            Ok(())
        }
    }
}
```

- [ ] **Step 4: Boot smoke test**

Create `rust/prro/tests/app_boot.rs`:

```rust
use prro::{App, config::AppConfig};

const SAMPLE_TOML: &str = r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "var/prro_t14.sqlite3"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#;

#[tokio::test]
async fn boot_applies_migrations_and_returns_pool() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{}"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display()
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_number_config")
        .fetch_one(app.db()).await.unwrap();
    assert_eq!(count, 0);
    let _ = SAMPLE_TOML;     // suppress unused if/when we want to keep it for reference
}
```

- [ ] **Step 5: Run + verify**

```bash
cd rust/prro
cargo test --test app_boot -- --nocapture
```

Expected: 1 test passes.

- [ ] **Step 6: Commit**

```bash
git add rust/prro/src/config/ rust/prro/src/app.rs rust/prro/src/main.rs \
        rust/prro/tests/app_boot.rs
git commit -m "feat(rust/app): App composition root + serve/migrate CLI

AppConfig parses TOML; App::boot opens pool + runs migrations.
serve subcommand idles waiting for SIGINT (M3+ wires supervisor)."
git push origin rust-gateway
```

---

## Task 15: PID file lock (singleton mode) + minimal `prro doctor`

**Goal:** `prro serve` and maintenance subcommands (`migrate`, `doctor`) acquire an exclusive PID lock so two processes can't run on the same DB. `prro doctor` prints a minimal config/DB/permissions report.

**Files:**
- Create: `rust/prro/src/runtime/singleton.rs` (was stub)
- Modify: `rust/prro/src/doctor.rs` (was stub)
- Modify: `rust/prro/src/main.rs` (add `Doctor` subcommand, wire singleton lock)
- Create: `rust/prro/tests/singleton_lock.rs`

**Acceptance Criteria:**
- [ ] `singleton::acquire_lock(&path)` returns Ok with held file on success
- [ ] Second call against the same path errors with `AlreadyRunning`
- [ ] `prro doctor --config <path>` prints `OK` lines for config + DB + lock dir

**Verify:** `cargo test --test singleton_lock` → 2 tests pass; `prro doctor --config <example>` exits 0

**Steps:**

- [ ] **Step 1: Implement singleton**

Replace `rust/prro/src/runtime/singleton.rs`:

```rust
//! Cross-platform exclusive process lock backed by an OS file lock.
//!
//! Used by maintenance CLI (serve/migrate/doctor/db-backup) to ensure
//! at most one prro process operates on a given DB file at a time.
//! Live CLI (fn add, shift open, …) does NOT call this — it talks to
//! the running daemon over loopback HTTP.

use anyhow::{anyhow, Context};
use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;

pub struct PidLock {
    /// Hold the file handle for the lock's lifetime.  Drop releases.
    _file: File,
}

pub fn acquire(db_path: &Path) -> anyhow::Result<PidLock> {
    let lock_path = db_path.with_extension("pid");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = OpenOptions::new()
        .create(true).read(true).write(true).truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening lock file {}", lock_path.display()))?;
    file.try_lock_exclusive()
        .map_err(|_| anyhow!(
            "another prro process is already running (lock at {})",
            lock_path.display()
        ))?;
    use std::io::Write;
    let mut f = &file;
    f.write_all(std::process::id().to_string().as_bytes()).ok();
    Ok(PidLock { _file: file })
}
```

- [ ] **Step 2: Implement minimal `doctor`**

Replace `rust/prro/src/doctor.rs`:

```rust
//! `prro doctor` — preflight checks for config, DB, lock, permissions.

use crate::config::AppConfig;
use std::path::Path;

pub async fn run(config_path: &Path) -> anyhow::Result<()> {
    println!("== prro doctor ==");

    // 1. Config file readable + parses.
    let text = std::fs::read_to_string(config_path)?;
    let cfg = AppConfig::from_toml(&text)?;
    println!("[OK]  config:  {}", config_path.display());

    // 2. DB parent directory exists or can be created.
    if let Some(parent) = cfg.database.db_path.parent() {
        std::fs::create_dir_all(parent)?;
        println!("[OK]  db dir:  {}", parent.display());
    }

    // 3. DB pool opens (this also runs migrations idempotently).
    let pool = crate::db::open_pool(&cfg.database.db_path).await?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool).await?;
    println!("[OK]  migrations applied: {n}");

    // 4. Singleton lock available (acquire then release).
    let lock = crate::runtime::singleton::acquire(&cfg.database.db_path)?;
    drop(lock);
    println!("[OK]  pid lock acquirable");

    // 5. Admin UI listen address parses.
    let _: std::net::SocketAddr = cfg.admin_ui.listen.parse()?;
    println!("[OK]  admin_ui.listen: {}", cfg.admin_ui.listen);

    println!("== ALL CHECKS PASSED ==");
    Ok(())
}
```

- [ ] **Step 3: Wire `Doctor` into CLI + lock around serve/migrate**

Replace the body of `main.rs` cmd match:

```rust
match cli.cmd {
    Cmd::Version => {
        println!("prro {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    }
    Cmd::Migrate { config } => {
        let text = std::fs::read_to_string(&config)?;
        let cfg = AppConfig::from_toml(&text)?;
        let _lock = prro::runtime::singleton::acquire(&cfg.database.db_path)?;
        let _app = App::boot(cfg).await?;
        tracing::info!("migrations applied");
        Ok(())
    }
    Cmd::Doctor { config } => {
        prro::doctor::run(&config).await
    }
    Cmd::Serve { config } => {
        let text = std::fs::read_to_string(&config)?;
        let cfg = AppConfig::from_toml(&text)?;
        let _lock = prro::runtime::singleton::acquire(&cfg.database.db_path)?;
        let app = App::boot(cfg).await?;
        tracing::info!(version = env!("CARGO_PKG_VERSION"), "prro listening (M1 — idle)");
        signal::ctrl_c().await?;
        tracing::info!("shutting down");
        drop(app);
        Ok(())
    }
}
```

Add `Doctor` to the enum:

```rust
#[derive(Subcommand, Debug)]
enum Cmd {
    Version,
    Migrate { #[arg(long)] config: PathBuf },
    Doctor { #[arg(long)] config: PathBuf },
    Serve { #[arg(long)] config: PathBuf },
}
```

- [ ] **Step 4: Singleton lock test**

Create `rust/prro/tests/singleton_lock.rs`:

```rust
use prro::runtime::singleton;

#[test]
fn second_acquisition_fails() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("x.db");
    let _lock = singleton::acquire(&db_path).expect("first must succeed");
    let result = singleton::acquire(&db_path);
    assert!(result.is_err(), "second must fail");
}

#[test]
fn lock_releases_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("y.db");
    {
        let _lock = singleton::acquire(&db_path).unwrap();
        // first holder drops at end of scope
    }
    let _lock2 = singleton::acquire(&db_path).expect("after drop, re-acquire must succeed");
}
```

- [ ] **Step 5: Sample config + smoke**

Create `rust/prro/etc/prro.example.toml`:

```toml
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "var/prro.sqlite3"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
```

Run:

```bash
cd rust/prro
cargo run -p prro -- doctor --config etc/prro.example.toml
```

Expected: every line starts with `[OK]`, final `== ALL CHECKS PASSED ==`.

```bash
cargo test --test singleton_lock -- --nocapture
```

Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add rust/prro/src/runtime/singleton.rs rust/prro/src/doctor.rs \
        rust/prro/src/main.rs rust/prro/etc/prro.example.toml \
        rust/prro/tests/singleton_lock.rs
git commit -m "feat(rust/runtime): PID singleton + prro doctor

Maintenance CLI (serve/migrate/doctor) acquires fs4 exclusive lock
on <db>.pid.  Two processes on same DB → second fails fast.
prro doctor: config + DB dir + migrations + lock + listen-addr."
git push origin rust-gateway
```

---

## Task 16: CI matrix — Linux musl + Linux gnu + Windows MSVC

**Goal:** GitHub Actions builds and tests `prro` on all three production targets per spec decision #35.

**Files:**
- Create: `.github/workflows/rust-prro.yml`

**Acceptance Criteria:**
- [ ] Workflow triggers on PR + push to `rust-gateway`
- [ ] Three jobs (linux-musl, linux-gnu, windows-msvc) each run `cargo build` + `cargo test -p prro`
- [ ] sqlx prep is cached / `SQLX_OFFLINE=true` honoured

**Verify:** Open PR from `rust-gateway` to itself or to `main` (draft); CI matrix turns green.

**Steps:**

- [ ] **Step 1: Write workflow**

Create `.github/workflows/rust-prro.yml`:

```yaml
name: rust-prro

on:
  push:
    branches: [rust-gateway]
    paths:
      - 'rust/prro/**'
      - 'rust/Cargo.*'
      - '.github/workflows/rust-prro.yml'
  pull_request:
    paths:
      - 'rust/prro/**'
      - 'rust/Cargo.*'
      - '.github/workflows/rust-prro.yml'

env:
  RUST_BACKTRACE: 1
  CARGO_TERM_COLOR: always
  SQLX_OFFLINE: "true"

jobs:
  build:
    name: ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os:     ubuntu-latest
            apt:    "musl-tools"
          - target: x86_64-unknown-linux-gnu
            os:     ubuntu-latest
            apt:    ""
          - target: x86_64-pc-windows-msvc
            os:     windows-latest
            apt:    ""

    steps:
      - uses: actions/checkout@v4

      - name: Install apt deps (Linux)
        if: matrix.apt != ''
        run: sudo apt-get update && sudo apt-get install -y ${{ matrix.apt }}

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: rust

      - name: Build
        working-directory: rust
        run: cargo build -p prro --target ${{ matrix.target }} --locked

      - name: Test
        if: matrix.target != 'x86_64-unknown-linux-musl'
        working-directory: rust
        run: cargo test -p prro --target ${{ matrix.target }} --locked

      - name: Test (musl runs as native)
        if: matrix.target == 'x86_64-unknown-linux-musl'
        working-directory: rust
        # Tests don't need cross-compile execution if the host is gnu;
        # we run on the gnu host with the musl-built binary cached.
        run: cargo test -p prro --locked
```

- [ ] **Step 2: Push and verify**

```bash
git add .github/workflows/rust-prro.yml
git commit -m "ci: rust-prro matrix (linux musl + gnu + windows msvc)

Per spec decision #35.  SQLX_OFFLINE=true honoured (prepared queries
cached in .sqlx/).  Build on all three; test on linux-gnu + windows."
git push origin rust-gateway
```

Open a draft PR `rust-gateway → main` on GitHub. CI should run matrix and pass within ~10 min.

If a target fails on `typst` or another transitive dep, document in the PR description and either:
- Switch its row to `x86_64-unknown-linux-gnu` only and note the deferral
- Add `--features` flags to disable the offending crate for that target

---

## Self-review checklist (run before declaring M1 done)

- [ ] All 16 tasks committed individually with conventional `feat(rust/…):` messages
- [ ] `cargo build -p prro` clean (zero warnings) on Linux + Windows
- [ ] `cargo test -p prro` — every test in this plan passes (~25 tests total)
- [ ] CI matrix green on `rust-gateway`
- [ ] `prro doctor --config etc/prro.example.toml` exits 0 with all `[OK]` lines
- [ ] No `TODO`/`FIXME`/`unimplemented!()` in any committed source
- [ ] sqlx `.sqlx/` directory committed for offline CI

---

## What's NOT in M1 (next plans)

| Topic | Plan |
|---|---|
| Crypto wrapper + CertCache + cred_seal | M2 |
| `DpsChannel` trait + `DpsSubmission`/`DpsStatusQuery` types | M2 |
| `GrpcCabinetChannel` + tonic stubs | M2 |
| Mock DPS server (Rust tonic) | M2 |
| Hot-zone byte-equivalence goldens (XML/MAC/KVT) | M2 |
| `WriteWorker` + 6-stage pipeline | M3 |
| State machine guard for `DocState` (full whitelist + tests) | M3 |
| Ingress shells (REST first) | M4 |
| Recovery boot phase + `/admin/ui/recovery` | M4 |
| Admin UI (Askama + axum + sessions + CSRF) | M5 |
| Rendering (typst PDF + ESC-POS) | M5 |
| Maria 304 + Checkbox-compat ingress | M6 |
| Observability (`/metrics`, structured logs to file) | M6 |
| Packaging (deb/rpm/msi, systemd, Windows service) | M8 |

Each gets its own `docs/superpowers/plans/2026-MM-DD-rust-rewrite-mN-….md` written when its month starts.
