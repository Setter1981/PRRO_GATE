# Fiscal Correctness Fixes — Design Spec

**Date:** 2026-04-22  
**Scope:** Rust sidecar + Maria304 driver  
**Priority:** correctness over speed  
**Delivery:** two PRs (P0 critical → P1+P2 high/medium)

---

## Context

Code review identified 7 correctness and security issues in the Rust fiscal subsystem.
This document covers the agreed design for all fixes.

---

## Delivery structure

### PR-1 (P0 — critical, blocks fiscal testing)

| Fix | File(s) |
|-----|---------|
| P0-a: per-FN single-writer mutex | `prro_sidecar/src/bin/prro_sidecar.rs`, `AppState` |
| P0-b: ChainBroken + auto-reconcile | `prro_sidecar/src/bin/prro_sidecar.rs`, `src/repo.rs`, `src/errors.rs`, new migration |
| P0-c: dispatcher fail-close | `maria304_driver/src/session/dispatcher.rs`, `src/bridge/dto.rs` |

### PR-2 (P1+P2 — high/medium, before production)

| Fix | File(s) |
|-----|---------|
| P1-a: JKS password alignment + migration | `prro_sidecar/src/bin/prro_admin.rs`, `admin_ui/routes.py`, new migration |
| P1-b: spawn_blocking for TSP | `prro_sidecar/src/bin/prro_sidecar.rs` |
| P2-b: amount validation error | `maria304_driver/src/protocol/commands.rs` |

**P2-a (env substitution in config):** deferred post-pilot. Document as known limitation.

---

## P0-a: Per-FN single-writer mutex

### Problem

`next_local_number` and `load_previous_hash` are two separate DB calls with no barrier.
Concurrent requests for the same `fn_id` receive the same `previous_hash` → DPS rejects the chain.

**Violated invariant:** #2 (one fiscal_number = one logical single-writer write-path).

### Design

Add to `AppState`:
```rust
fn_locks: Arc<DashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
```

Key: `fn_id` (i64). Value: `Arc<Mutex<()>>` created lazily on first access.

**Critical section in `handle_fiscal_send`:**
```
[steps 1–8]  ← unchanged (validation, key loading)
ACQUIRE fn_lock(fn_id)
  [step 9]   next_local_number + load_previous_hash
  [step 10]  build XML
  [step 11]  CMS sign (CPU, in-process)
  [step 12]  gRPC send (network)
  [step 13]  store_previous_hash
RELEASE fn_lock
```

The lock is held through steps 11–12 because:
- CMS signs `previous_hash` as part of XML — another request must not update it mid-flight
- gRPC sends a document with a specific `local_number` — order must be strictly monotonic

**Timeout:** existing `REQUEST_TIMEOUT` (30s) at axum level handles stalled holders.

**DashMap cleanup:** background task in `main()`, runs every 5 minutes:
```rust
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        fn_locks.retain(|_, v| Arc::strong_count(v) > 1);
    }
});
```
`retain` is atomic per DashMap shard. Entry removed only when `strong_count == 1`
(only DashMap holds the `Arc` — no one waiting or holding the mutex).

**Dependency:** add `dashmap = "6"` to `prro_sidecar/Cargo.toml`.

### Tests
- Concurrent test: 2 tokio tasks per `fn_id` — verify `local_number` is monotonic and `previous_hash` not duplicated
- Cleanup test: after lock release, `Arc::strong_count == 1`, `retain` removes entry

---

## P0-b: ChainBroken + auto-reconcile

### Problem

`store_previous_hash` failure is only logged (warn). Next document for this `fn_id` uses
the old hash → DPS rejects the entire chain.

**Violated invariant:** #8 (recovery must not silently violate state transitions).

### DDL (new migration)

```sql
CREATE TABLE fn_degraded (
    fn_id         INTEGER PRIMARY KEY REFERENCES fn_configs(id),
    pending_hash  TEXT    NOT NULL,
    degraded_at   TEXT    NOT NULL,
    retry_count   INTEGER NOT NULL DEFAULT 0,
    last_retry_at TEXT
);
```

### New Repo methods

| Method | Action |
|--------|--------|
| `set_degraded(fn_id, hash)` | INSERT OR REPLACE into fn_degraded |
| `is_degraded(fn_id) → bool` | SELECT EXISTS |
| `list_degraded() → Vec<(fn_id, hash, retries)>` | for reconcile loop |
| `reconcile_chain(fn_id, hash)` | on success: transaction(store_previous_hash + DELETE fn_degraded); on failure: UPDATE retry_count + last_retry_at |

### Modified error path (line 421)

```rust
if let Err(e) = st.repo.store_previous_hash(fn_id, &mac_hex) {
    let _ = st.repo.set_degraded(fn_id, &mac_hex)
        .map_err(|de| tracing::error!(fn_id, error=%de, "set_degraded_failed"));
    tracing::error!(fn_id, error=%e, "chain_broken_store_failed");
    // Return 200 with flag — document IS in DPS, not a send error
    return Ok(FiscalSendResponse { chain_broken: true, ..resp_data });
}
```

**Why 200, not 503:** document was accepted by DPS. Returning 503 causes Python gateway
to write the document as `ERROR_SEND` and reconciliation will re-send it → DPS duplicate.
Instead: 200 + `chain_broken: true` → Python records document as SENT, sets FN to
`CRYPTO_DEGRADED`, stops new documents for this FN.

### Incoming request check (first thing inside fn_lock, before step 9)

```rust
// Inside critical section, after ACQUIRE fn_lock(fn_id):
if st.repo.is_degraded(fn_id)? {
    return Err(SidecarError::FnDegraded(fn_id));  // → HTTP 503 FN_DEGRADED
}
// Only then proceed to next_local_number / load_previous_hash
```

### Reconcile background loop

```rust
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        match repo.list_degraded() {
            Ok(entries) => {
                for (fn_id, pending_hash, _retry_count) in entries {
                    let lock = fn_locks.entry(fn_id)
                        .or_insert_with(|| Arc::new(Mutex::new(()))).clone();
                    let _guard = lock.lock().await;
                    match repo.reconcile_chain(fn_id, &pending_hash) {
                        Ok(()) => tracing::info!(fn_id, "chain_reconciled"),
                        Err(e) => tracing::warn!(fn_id, error=%e, "chain_reconcile_retry_failed"),
                    }
                }
            }
            Err(e) => tracing::error!(error=%e, "degraded_list_failed"),
        }
    }
});
```

Reconcile loop acquires `fn_lock(fn_id)` — same lock as `handle_fiscal_send` — eliminating race.

### FiscalSendResponse changes

Add field:
```rust
pub chain_broken: bool,  // default false
```

HTTP status remains 200. Callers check this field.

### Tests
- `set_degraded` → `is_degraded` returns true
- `reconcile_chain` success → `is_degraded` returns false, `previous_hash` updated
- `reconcile_chain` failure → `retry_count` incremented, degraded remains
- HTTP: request on degraded `fn_id` → 503
- HTTP: document accepted + store failure → 200 with `chain_broken: true`

---

## P0-c: Dispatcher fail-close

### Problem

`dispatcher.rs:282–298`: receipt closed on any `Ok(resp)` without checking `resp.ok`
or `resp.document_state`. `fiscal_id` parse failure silently becomes `0`.

**Violated invariant:** #4 (idempotency — closing a receipt must reflect actual DPS outcome).

### New function in `dto.rs`

```rust
pub enum DocumentOutcome {
    Accepted { fiscal_id: u64, sale: u64, ret: u64 },
    Terminal(ErrorCode),   // receipt closed as rejected — caller must open new receipt
    Retryable(ErrorCode),  // receipt stays open — caller can retry or CANC
}

pub fn classify_response(resp: &CanonicalResponse) -> DocumentOutcome {
    if resp.ok {
        match resp.fiscal_id.parse::<u64>() {
            Ok(id) if id > 0 => DocumentOutcome::Accepted {
                fiscal_id: id,
                sale: resp.sale_total_kopecks,
                ret:  resp.return_total_kopecks,
            },
            _ => DocumentOutcome::Terminal(ErrorCode::SoftFiscalError),
        }
    } else {
        match resp.document_state.as_str() {
            "REJECTED" | "ERROR_SIGN" | "ERROR_FISCAL" =>
                DocumentOutcome::Terminal(ErrorCode::SoftFiscalError),
            _ =>
                DocumentOutcome::Retryable(ErrorCode::SoftHwError),
        }
    }
}
```

### document_state mapping

| `document_state` | Outcome | Rationale |
|---|---|---|
| `SENT` + `ok=true` | Accepted | DPS accepted |
| `REJECTED` | Terminal | DPS rejected — sequence slot consumed |
| `ERROR_SIGN` | Terminal | Invalid signature — retry creates new document |
| `ERROR_FISCAL` | Terminal | DPS fiscal error |
| `ERROR_SEND` | Retryable | Transport — DPS never received |
| `""` / unknown | Retryable | Unclear — safer to leave open |

### Modified COMP handler in `dispatcher.rs`

```rust
Ok(resp) => match classify_response(&resp) {
    DocumentOutcome::Accepted { fiscal_id, sale, ret } => {
        let payload = CompBuilder::new(fiscal_id, sale, ret).to_wire_payload();
        session.state = SessionState::Authenticated;
        session.psdt_sequence = 0;
        session.pending_return_check_number = None;
        correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
        session.mark_command_ok("COMP");
        ok(Some(Response::data(payload).expect("COMP payload is 94 chars")))
    }
    DocumentOutcome::Terminal(code) => {
        session.state = SessionState::Authenticated;
        session.mark_command_ok("COMP_REJECTED");
        err(code)
    }
    DocumentOutcome::Retryable(code) => {
        // receipt stays open — caller can retry or CANC
        err(code)
    }
},
```

### Tests
- `classify_response`: 5 unit tests (one per table row above)
- Integration: `COMP` with `ok=false, document_state="REJECTED"` → receipt closed, wire error
- Integration: `COMP` with `ok=false, document_state="ERROR_SEND"` → receipt open, soft error

---

## P1-a: JKS password alignment + migration

### Problem

`prro_admin.rs:196` and `routes.py:960` write password as-is (plain). Sidecar reads
with XorSoft decode → key fails to open.

**Principle:** XorSoft encoding lives only in Rust (`credentials.rs`). Python does not
reimplement XOR — it delegates to Rust CLI.

### DDL (new migration)

```sql
ALTER TABLE sidecar_operators
    ADD COLUMN credentials_mode TEXT NOT NULL DEFAULT 'plain';
```

Existing rows get `credentials_mode = 'plain'` via DEFAULT. Updated to `'xor_soft'`
after migration command runs.

### New command: `prro_admin migrate-passwords`

```
prro_admin --db sidecar.db migrate-passwords [--dry-run]
```

Algorithm:
1. `SELECT id, operator_name, jks_path, jks_password FROM sidecar_operators WHERE credentials_mode = 'plain'`
2. For each row:
   - Load JKS at `jks_path` with current plain `jks_password` (verify it works + get cert metadata)
   - Extract `valid_to` from cert (`cms_adapter::extract_cert_valid_to`)
   - `encoded = credentials::encode_password(XorSoft, &jks_password, &valid_to, &operator_name)`
   - `UPDATE sidecar_operators SET jks_password = encoded, credentials_mode = 'xor_soft' WHERE id = ?`
3. Print report: `migrated N, skipped M (see errors above)`

`--dry-run` prints plan without writing.

### `cmd_add_operator` changes

```rust
// Load JKS to get cert metadata (also validates password early)
let jks_bytes = std::fs::read(&jks_path)?;
let extracted = extract_private_key(&jks_bytes, &raw_password)?;
let cert_der = extracted.certs.first().ok_or(AdminError::NoCert)?;
let valid_to = cms_adapter::extract_cert_valid_to(cert_der)?;

// Encode before INSERT
let encoded = credentials::encode_password(
    CredentialsMode::XorSoft, &raw_password, &valid_to, &operator_name,
);
repo.add_operator(&fiscal_number, &operator_name, &inn, &jks_path, &encoded)?;
// INSERT also sets credentials_mode = 'xor_soft'
```

Side effect: JKS is validated at registration time, not at first receipt.

### `routes.py` changes

Replace direct INSERT with CLI delegation:
```python
import subprocess, shutil

def _add_operator_via_cli(fiscal_number, operator_name, inn, jks_path, jks_password):
    admin_bin = shutil.which("prro_admin") or "/usr/local/bin/prro_admin"
    result = subprocess.run(
        [admin_bin, "--db", current_app.config["SIDECAR_DB"],
         "add-operator",
         "--fiscal-number", fiscal_number,
         "--name", operator_name,
         "--inn", inn,
         "--jks-path", jks_path,
         "--password", jks_password],
        capture_output=True, text=True, timeout=10
    )
    if result.returncode != 0:
        raise OperatorAddError(result.stderr.strip())
```

### Sidecar reads with credentials_mode awareness

```rust
let mode = match op.credentials_mode.as_str() {
    "xor_soft" => CredentialsMode::XorSoft,
    _          => CredentialsMode::Plain,
};
let raw_pw = credentials::decode_password(mode, &op.jks_password, &valid_to, &op.name)?;
```

Allows mixed state during migration without downtime.

### Deploy order

```
1. Deploy new binaries (prro_admin + prro_sidecar)
2. Stop sidecar
3. prro_admin migrate-passwords --dry-run   ← verify
4. prro_admin migrate-passwords             ← apply
5. Start sidecar
```

Steps 3–5 are intentionally manual — operation is irreversible.

### Tests
- `migrate-passwords --dry-run` does not change DB
- After `migrate-passwords`: all rows have `credentials_mode = 'xor_soft'`
- Sidecar reads xor_soft row → decode → matches original password
- Sidecar reads plain row (mixed mode during migration) → decode plain → works

---

## P1-b: spawn_blocking for TSP

### Problem

`fetch_timestamp` in `tsp.rs:254` uses blocking `ureq`. Called from async axum handler —
blocks a tokio worker thread for up to 5s (TSP timeout). Under load ≥ N workers the
entire event loop stalls.

### Design

Wrap call in `spawn_blocking` at the call site in `prro_sidecar.rs` (step 11, ~line 297):

```rust
let tsp_token = tokio::task::spawn_blocking(move || {
    fetch_timestamp(&tsa_url, &digest, timeout)
})
.await
.map_err(|e| SidecarError::CmsSign(format!("tsp_join: {e}")))??;
```

`spawn_blocking` routes the blocking call to tokio's blocking thread pool (default 512 threads),
freeing async workers.

`tsa_url` (String) and `digest` (Vec<u8>) move into closure. `timeout` (Duration) is Copy.
No Arc, no heavy clones.

**Why not rewrite `fetch_timestamp` as async:** would require replacing `ureq` with `reqwest`,
adding `tokio` feature to `prro_crypto`, and reworking `tsp.rs` — outside minimal diff scope.
`spawn_blocking` is the correct pattern for legacy blocking I/O in async runtime.

### Tests
- Concurrency test: 10 parallel requests with mock TSP server — verify no latency spike
  suggesting event loop blocking

---

## P2-b: Amount validation error

### Problem

`commands.rs:379` and other locations use `unwrap_or(0)` when parsing amounts.
Invalid amount silently becomes `0` — fiscal document with zero amount reaches DPS.

### Design

Replace `unwrap_or(0)` with explicit `InvalidParams`:

```rust
// Was:
let sum_kopecks = sum_str.parse::<u64>().unwrap_or(0);

// Now:
let sum_kopecks = sum_str.parse::<u64>()
    .map_err(|_| Command::InvalidParams)?;

if sum_kopecks == 0 {
    return Command::InvalidParams;
}
```

**Scope:** only locations where `0` is semantically invalid (payment amounts: CAIO, COMP
sub-fields). Locations where `0` is a valid value (counters, indexes) — unchanged.

**Cash register behavior on `InvalidParams`:** wire error `SOFT_INVALID_PARAMS` → register
receives explicit rejection, does not forward document to bridge.

### Tests
- CAIO with invalid amount string → `InvalidParams`
- CAIO with amount `0` → `InvalidParams`
- CAIO with valid amount → `Accepted`

---

## Migrations summary

| Migration file | Content | PR |
|---|---|---|
| `NNN_fn_degraded.sql` | CREATE TABLE fn_degraded | PR-1 |
| `NNN_operator_credentials_mode.sql` | ALTER TABLE sidecar_operators ADD credentials_mode | PR-2 |

Both are additive (no DROP, no NOT NULL without DEFAULT). Safe for zero-downtime apply.

---

## Known limitations / deferred

- **P2-a (env substitution):** `${MARIA304_BRIDGE_TOKEN}` in YAML config is not expanded — token must be written directly into config file. Deferred post-pilot. Document in INSTALL.md as known limitation.
- **Multi-instance sidecar:** per-FN mutex is in-process only. If multiple sidecar instances are ever needed for one set of FNs, DB-level advisory locking or a serializing worker process will be required.
- **DashMap growth:** entries accumulate (one per FN). Cleanup task runs every 5 minutes. For large FN counts, monitor DashMap size.
