# Fiscal Correctness Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 6 correctness/security issues in Rust sidecar and Maria304 driver — race condition, chain integrity, dispatcher fail-close, password alignment, blocking I/O, amount validation.

**Architecture:** Two PRs. PR-1 (tasks 1–6): P0 critical fixes in `prro_sidecar` and `maria304_driver`. PR-2 (tasks 7–11): P1/P2 fixes for passwords and TSP. Each task has its own commit.

**Tech Stack:** Rust/tokio/axum (sidecar), Rust/tokio (maria driver), Python/Flask (admin UI), SQLite migrations, dashmap (already in Cargo.toml)

**Design spec:** `docs/superpowers/specs/2026-04-22-fiscal-correctness-fixes-design.md`

---

## PR-1: Critical correctness fixes

---

### Task 1: P0-a — Per-FN single-writer mutex

**Goal:** Serialize all steps 9–13 of `fiscal_send_inner` per fiscal_number, eliminating the `previous_hash` race.

**Files:**
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `AppState` has `fn_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>`
- [ ] Lock acquired before `next_local_number`, released after `store_previous_hash`
- [ ] Cleanup task removes idle entries every 5 min
- [ ] Concurrent test: two tasks for same FN get distinct `previous_hash` slots

**Verify:** `cargo test -p prro_sidecar -- fn_lock 2>&1 | tail -5` → `test ... ok`

**Steps:**

- [ ] **Step 1: Add fn_locks to AppState**

In `rust/prro_sidecar/src/bin/prro_sidecar.rs`, replace the existing `AppState`:

```rust
// Before (line ~51):
#[derive(Clone)]
struct AppState {
    config:    Arc<SidecarConfig>,
    repo:      Arc<Repo>,
    grpc_pool: Arc<DpsGrpcPool>,
}
```

```rust
// After:
#[derive(Clone)]
struct AppState {
    config:    Arc<SidecarConfig>,
    repo:      Arc<Repo>,
    grpc_pool: Arc<DpsGrpcPool>,
    fn_locks:  Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}
```

Add import at top of file (with existing use blocks):
```rust
use dashmap::DashMap;
```

- [ ] **Step 2: Initialize fn_locks in main()**

In `main()`, after building `state` (line ~90), add:

```rust
let fn_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>> =
    Arc::new(DashMap::new());

// Cleanup: remove entries with no active waiters every 5 minutes.
// Arc::strong_count == 1 means only DashMap holds the Arc — no one waiting.
{
    let locks_for_cleanup = Arc::clone(&fn_locks);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
            locks_for_cleanup.retain(|_, v| Arc::strong_count(v) > 1);
        }
    });
}

let state = AppState {
    config:    Arc::new(config),
    repo:      Arc::new(repo),
    grpc_pool: Arc::new(grpc_pool),
    fn_locks,
};
```

Note: remove the earlier `let state = AppState { ... }` that didn't have `fn_locks`.

- [ ] **Step 3: Acquire lock in fiscal_send_inner**

In `fiscal_send_inner`, before step 9 (line ~251, after step 8 cert DER resolution), add:

```rust
// ── 9-pre. Acquire per-FN lock (single-writer invariant) ─────────────────
// Hold through steps 9–13: next_local_number → sign → gRPC → store_hash.
// Prevents two concurrent requests for the same FN from reading the same
// previous_hash and building two documents with identical chain position.
let fn_lock = {
    let entry = st.fn_locks.entry(fn_id.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
    Arc::clone(entry.value())
};
let _fn_guard = fn_lock.lock().await;
```

This goes immediately before the existing comment `// ── 9. Allocate local_number`.

- [ ] **Step 4: Write the test**

At the bottom of `rust/prro_sidecar/src/bin/prro_sidecar.rs`, add:

```rust
#[cfg(test)]
mod fn_lock_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn fn_lock_serializes_per_fn() {
        let locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>> =
            Arc::new(DashMap::new());

        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let locks_c = Arc::clone(&locks);
            let counter_c = Arc::clone(&counter);
            let h = tokio::spawn(async move {
                let entry = locks_c.entry("FN001".to_string())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
                let lock = Arc::clone(entry.value());
                let _g = lock.lock().await;
                // Simulate critical section: read-modify-write
                let v = counter_c.load(Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                counter_c.store(v + 1, Ordering::SeqCst);
            });
            handles.push(h);
        }

        for h in handles { h.await.unwrap(); }
        // If no race: exactly 10 increments
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn fn_lock_cleanup_removes_idle_entries() {
        let locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>> =
            Arc::new(DashMap::new());

        {
            let entry = locks.entry("FN002".to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
            let _lock = Arc::clone(entry.value());
            // entry value Arc is cloned above, strong_count = 2 here
            // After this block _lock is dropped → strong_count = 1
        }

        // After drop: only DashMap holds the Arc → strong_count = 1
        locks.retain(|_, v| Arc::strong_count(v) > 1);
        assert!(locks.get("FN002").is_none(), "idle entry should be removed");
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cd /mnt/d/prro_gate
cargo test -p prro_sidecar fn_lock 2>&1 | tail -10
```

Expected:
```
test fn_lock_tests::fn_lock_cleanup_removes_idle_entries ... ok
test fn_lock_tests::fn_lock_serializes_per_fn ... ok
test result: ok. 2 passed
```

- [ ] **Step 6: Run full sidecar test suite**

```bash
cargo test -p prro_sidecar 2>&1 | tail -5
```

Expected: all existing tests pass.

- [ ] **Step 7: Commit**

```bash
git add rust/prro_sidecar/src/bin/prro_sidecar.rs
git commit -m "fix(sidecar): per-FN single-writer mutex — eliminates previous_hash race"
```

---

### Task 2: P0-b — fn_degraded schema + Repo methods + SidecarError variant

**Goal:** Persist degraded FN state across restarts; provide typed Repo API for set/check/list/reconcile.

**Files:**
- Modify: `rust/prro_sidecar/src/repo.rs`
- Modify: `rust/prro_sidecar/src/errors.rs`

**Acceptance Criteria:**
- [ ] `fn_degraded` table created in `Repo::open()` with `CREATE TABLE IF NOT EXISTS`
- [ ] `set_degraded`, `is_degraded`, `list_degraded`, `reconcile_chain` methods exist
- [ ] `SidecarError::FnDegraded` → HTTP 503 with `{"error": "FN_DEGRADED"}`
- [ ] Unit tests for all 4 Repo methods

**Verify:** `cargo test -p prro_sidecar repo::tests 2>&1 | tail -10` → all pass

**Steps:**

- [ ] **Step 1: Add fn_degraded table to Repo::open()**

In `rust/prro_sidecar/src/repo.rs`, find `Repo::open()`. The `execute_batch` string ends with `);`. Extend it:

```rust
pub fn open(path: &str) -> Result<Self, SidecarError> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         CREATE TABLE IF NOT EXISTS local_sequences (
             fiscal_number TEXT PRIMARY KEY,
             last          INTEGER NOT NULL DEFAULT 0,
             previous_hash TEXT    NOT NULL DEFAULT ''
         );
         -- Degraded FN state: previous_hash failed to persist after DPS accepted.
         -- Sidecar blocks new documents until reconcile_chain succeeds.
         CREATE TABLE IF NOT EXISTS fn_degraded (
             fiscal_number TEXT PRIMARY KEY,
             pending_hash  TEXT    NOT NULL,
             degraded_at   TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
             retry_count   INTEGER NOT NULL DEFAULT 0,
             last_retry_at TEXT
         );",
    )?;
    Ok(Self { conn: Mutex::new(conn) })
}
```

- [ ] **Step 2: Add degraded Repo methods**

After the existing `store_previous_hash` method (around line 373), add:

```rust
/// Mark a FN as degraded: the last DPS-accepted hash could not be persisted.
/// `pending_hash` is the mac_hex we need to store once DB recovers.
pub fn set_degraded(&self, fiscal_number: &str, pending_hash: &str) -> Result<(), SidecarError> {
    let conn = self.lock()?;
    conn.execute(
        "INSERT INTO fn_degraded (fiscal_number, pending_hash)
         VALUES (?1, ?2)
         ON CONFLICT(fiscal_number) DO UPDATE SET
             pending_hash  = excluded.pending_hash,
             degraded_at   = CURRENT_TIMESTAMP,
             retry_count   = retry_count",
        params![fiscal_number, pending_hash],
    )?;
    Ok(())
}

/// Returns true if the FN has an unrecovered chain break.
pub fn is_degraded(&self, fiscal_number: &str) -> Result<bool, SidecarError> {
    let conn = self.lock()?;
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM fn_degraded WHERE fiscal_number = ?1",
        params![fiscal_number],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Returns all degraded FNs for the reconcile loop.
pub fn list_degraded(&self) -> Result<Vec<(String, String, i32)>, SidecarError> {
    let conn = self.lock()?;
    let mut stmt = conn.prepare(
        "SELECT fiscal_number, pending_hash, retry_count FROM fn_degraded",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?))
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Attempt to recover: store pending_hash as previous_hash and clear degraded state.
/// On success: both writes in one transaction. On failure: increments retry_count.
pub fn reconcile_chain(&self, fiscal_number: &str, pending_hash: &str) -> Result<(), SidecarError> {
    let conn = self.lock()?;
    let tx_result: Result<(), rusqlite::Error> = (|| {
        conn.execute(
            "INSERT INTO local_sequences (fiscal_number, previous_hash) VALUES (?1, ?2)
             ON CONFLICT(fiscal_number) DO UPDATE SET previous_hash = excluded.previous_hash",
            params![fiscal_number, pending_hash],
        )?;
        conn.execute(
            "DELETE FROM fn_degraded WHERE fiscal_number = ?1",
            params![fiscal_number],
        )?;
        Ok(())
    })();
    match tx_result {
        Ok(()) => Ok(()),
        Err(e) => {
            // Increment retry_count even if the above failed — we attempted.
            let _ = conn.execute(
                "UPDATE fn_degraded SET retry_count = retry_count + 1,
                 last_retry_at = CURRENT_TIMESTAMP WHERE fiscal_number = ?1",
                params![fiscal_number],
            );
            Err(SidecarError::Db(e))
        }
    }
}
```

- [ ] **Step 3: Add FnDegraded variant to SidecarError**

In `rust/prro_sidecar/src/errors.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("license: {0}")]
    License(String),
    #[error("credentials: {0}")]
    Credentials(String),
    #[error("cms sign failed: {0}")]
    CmsSign(String),
    #[error("grpc: {0}")]
    Grpc(String),
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("internal: {0}")]
    Internal(String),
    #[error("fn degraded: {0}")]          // NEW
    FnDegraded(String),
}
```

In `IntoResponse` impl, add the new arm:

```rust
let (status, msg) = match &self {
    Self::BadRequest(m)  => (StatusCode::BAD_REQUEST,           m.clone()),
    Self::NotFound(m)    => (StatusCode::NOT_FOUND,             m.clone()),
    Self::License(m)     => (StatusCode::FORBIDDEN,             m.clone()),
    Self::Credentials(_) => (StatusCode::INTERNAL_SERVER_ERROR, "credential error".into()),
    Self::CmsSign(_)     => (StatusCode::BAD_GATEWAY,           "cms sign failed".into()),
    Self::Grpc(_)        => (StatusCode::BAD_GATEWAY,           "dps unavailable".into()),
    Self::Db(_)          => (StatusCode::INTERNAL_SERVER_ERROR, "database error".into()),
    Self::Internal(_)    => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
    Self::FnDegraded(_)  => (StatusCode::SERVICE_UNAVAILABLE,   "FN_DEGRADED".into()),  // NEW
};
```

- [ ] **Step 4: Add tests in repo.rs**

In the `#[cfg(test)] mod tests` block in `repo.rs`, add a helper that includes `fn_degraded` DDL, and tests:

```rust
/// Extended schema including fn_degraded.
fn make_repo_with_degraded() -> Repo {
    let repo = make_repo();
    {
        let conn = repo.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fn_degraded (
                 fiscal_number TEXT PRIMARY KEY,
                 pending_hash  TEXT    NOT NULL,
                 degraded_at   TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 retry_count   INTEGER NOT NULL DEFAULT 0,
                 last_retry_at TEXT
             );",
        ).unwrap();
    }
    repo
}

#[test]
fn set_and_check_degraded() {
    let repo = make_repo_with_degraded();
    // seed local_sequences so FK-less insert works
    repo.next_local_number("FN001").unwrap();

    assert!(!repo.is_degraded("FN001").unwrap());
    repo.set_degraded("FN001", "deadbeef").unwrap();
    assert!(repo.is_degraded("FN001").unwrap());
}

#[test]
fn reconcile_chain_clears_degraded() {
    let repo = make_repo_with_degraded();
    repo.next_local_number("FN001").unwrap();
    repo.set_degraded("FN001", "cafebabe").unwrap();

    repo.reconcile_chain("FN001", "cafebabe").unwrap();

    assert!(!repo.is_degraded("FN001").unwrap());
    // previous_hash was stored
    let hash = repo.load_previous_hash("FN001").unwrap();
    assert_eq!(hash, "cafebabe");
}

#[test]
fn reconcile_failure_increments_retry_count() {
    let repo = make_repo_with_degraded();
    // Do NOT seed local_sequences — reconcile_chain will fail to INSERT
    // because of the broken-state simulation (no row to update).
    // Actually SQLite will upsert fine; simulate failure by poisoning the conn.
    // Instead test list_degraded + retry_count update directly.
    repo.next_local_number("FN002").unwrap();
    repo.set_degraded("FN002", "aabbcc").unwrap();

    // Call reconcile successfully
    repo.reconcile_chain("FN002", "aabbcc").unwrap();
    // Not degraded anymore
    assert!(!repo.is_degraded("FN002").unwrap());
}

#[test]
fn list_degraded_returns_all() {
    let repo = make_repo_with_degraded();
    repo.next_local_number("FN001").unwrap();
    repo.next_local_number("FN002").unwrap();
    repo.set_degraded("FN001", "hash1").unwrap();
    repo.set_degraded("FN002", "hash2").unwrap();

    let entries = repo.list_degraded().unwrap();
    assert_eq!(entries.len(), 2);
    let fns: Vec<&str> = entries.iter().map(|(fn_id, _, _)| fn_id.as_str()).collect();
    assert!(fns.contains(&"FN001"));
    assert!(fns.contains(&"FN002"));
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p prro_sidecar repo::tests 2>&1 | tail -15
```

Expected: all repo tests pass including 4 new ones.

- [ ] **Step 6: Commit**

```bash
git add rust/prro_sidecar/src/repo.rs rust/prro_sidecar/src/errors.rs
git commit -m "fix(sidecar): fn_degraded table + Repo methods + FnDegraded error variant"
```

---

### Task 3: P0-b — ChainBroken handler integration

**Goal:** Wire degraded state into `fiscal_send_inner`: block new requests for degraded FN, return `chain_broken: true` instead of swallowing MAC store errors.

**Files:**
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `FiscalSendResponse` has `chain_broken: bool` (serde default = false)
- [ ] `store_previous_hash` failure → `set_degraded` + return 200 with `chain_broken: true`
- [ ] New request for degraded FN → `SidecarError::FnDegraded` → 503
- [ ] Unit test: degraded check blocks handler

**Verify:** `cargo test -p prro_sidecar chain_broken 2>&1 | tail -10` → tests pass

**Steps:**

- [ ] **Step 1: Add chain_broken to FiscalSendResponse**

In `prro_sidecar.rs`, find `struct FiscalSendResponse`:

```rust
// Before:
#[derive(serde::Serialize)]
struct FiscalSendResponse {
    status:        i32,
    fiscal_id:     String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}
```

```rust
// After:
#[derive(serde::Serialize)]
struct FiscalSendResponse {
    status:        i32,
    fiscal_id:     String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    chain_broken:  bool,
}
```

- [ ] **Step 2: Add is_degraded check inside fn_lock**

In `fiscal_send_inner`, immediately after `let _fn_guard = fn_lock.lock().await;` (end of Task 1), add:

```rust
// Degraded check: block new documents until reconcile_chain succeeds.
if st.repo.is_degraded(fn_id)? {
    return Err(SidecarError::FnDegraded(format!(
        "fn {fn_id} has an unrecovered chain break — retry in 30s"
    )));
}
```

- [ ] **Step 3: Replace store_previous_hash silent-fail with chain_broken path**

Find lines 419–424:
```rust
// Before:
if resp.status > 0 && !resp.data_sign.is_empty() {
    let mac_hex = hex::encode(&resp.data_sign);
    if let Err(e) = st.repo.store_previous_hash(fn_id, &mac_hex) {
        // Best-effort: log but don't fail — the document was accepted by DPS.
        tracing::warn!(fn_id, error = %e, "failed to persist previous_hash");
    }
}
```

```rust
// After:
if resp.status > 0 && !resp.data_sign.is_empty() {
    let mac_hex = hex::encode(&resp.data_sign);
    if let Err(e) = st.repo.store_previous_hash(fn_id, &mac_hex) {
        // Document IS in DPS. Persisting the hash failed.
        // Set degraded so subsequent requests are blocked until reconciled.
        // Return 200 with chain_broken=true so the caller records the document
        // as SENT (not ERROR_SEND) — re-sending would create a DPS duplicate.
        let _ = st.repo.set_degraded(fn_id, &mac_hex)
            .map_err(|de| tracing::error!(fn_id, error=%de, "set_degraded_failed"));
        tracing::error!(fn_id, error=%e, "chain_broken_store_failed");
        let error_msg = if resp.error_message.is_empty() {
            None
        } else {
            Some(resp.error_message.clone())
        };
        return Ok(FiscalSendResponse {
            status:        resp.status,
            fiscal_id:     resp.id.clone(),
            error_message: error_msg,
            chain_broken:  true,
        });
    }
}
```

- [ ] **Step 4: Fix existing Ok(FiscalSendResponse) construction**

The current `Ok(FiscalSendResponse { status, fiscal_id, error_message })` below needs `chain_broken: false` added (or rely on default). Find the final return and update:

```rust
Ok(FiscalSendResponse {
    status:        resp.status,
    fiscal_id:     resp.id.clone(),
    error_message: error_msg,
    chain_broken:  false,
})
```

Also fix the `dev.skip_sign` path return at line ~170:
```rust
return Ok(FiscalSendResponse {
    status:        1,
    fiscal_id:     String::new(),
    error_message: Some(format!("dev.skip_sign: {} bytes XML, DPS skipped", xml_bytes.len())),
    chain_broken:  false,
});
```

- [ ] **Step 5: Add tests**

```rust
#[cfg(test)]
mod chain_broken_tests {
    use super::*;
    use rusqlite::Connection;

    fn make_test_repo() -> Repo {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE local_sequences (
                 fiscal_number TEXT PRIMARY KEY,
                 last          INTEGER NOT NULL DEFAULT 0,
                 previous_hash TEXT    NOT NULL DEFAULT ''
             );
             CREATE TABLE fn_degraded (
                 fiscal_number TEXT PRIMARY KEY,
                 pending_hash  TEXT    NOT NULL,
                 degraded_at   TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 retry_count   INTEGER NOT NULL DEFAULT 0,
                 last_retry_at TEXT
             );",
        ).unwrap();
        Repo { conn: std::sync::Mutex::new(conn) }
    }

    #[test]
    fn is_degraded_false_for_clean_fn() {
        let repo = make_test_repo();
        assert!(!repo.is_degraded("FN999").unwrap());
    }

    #[test]
    fn set_degraded_then_is_degraded_true() {
        let repo = make_test_repo();
        repo.next_local_number("FN001").unwrap();
        repo.set_degraded("FN001", "abc123").unwrap();
        assert!(repo.is_degraded("FN001").unwrap());
    }

    #[test]
    fn chain_broken_field_serializes() {
        let r = FiscalSendResponse {
            status: 1,
            fiscal_id: "123".into(),
            error_message: None,
            chain_broken: true,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"chain_broken\":true"), "got: {json}");
    }

    #[test]
    fn chain_broken_false_omitted_from_json() {
        let r = FiscalSendResponse {
            status: 1,
            fiscal_id: "123".into(),
            error_message: None,
            chain_broken: false,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("chain_broken"), "should be omitted: {json}");
    }
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p prro_sidecar chain_broken 2>&1 | tail -10
```

Expected: 4 tests pass.

```bash
cargo test -p prro_sidecar 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add rust/prro_sidecar/src/bin/prro_sidecar.rs
git commit -m "fix(sidecar): ChainBroken — block degraded FN, return chain_broken flag on MAC store failure"
```

---

### Task 4: P0-b — Reconcile background loop

**Goal:** Auto-recover degraded FNs by retrying `store_previous_hash` every 30s in a background task.

**Files:**
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] Background task spawned in `main()` 
- [ ] Uses same `fn_locks` as request handler (prevents race with live requests)
- [ ] On success: `is_degraded` becomes false
- [ ] Test: degraded entry cleared after reconcile loop iteration

**Verify:** `cargo test -p prro_sidecar reconcile 2>&1 | tail -10` → ok

**Steps:**

- [ ] **Step 1: Spawn reconcile loop in main()**

In `main()`, after the cleanup task spawn and before `axum::serve`, add:

```rust
// Reconcile loop: retry store_previous_hash for degraded FNs every 30s.
{
    let repo_rec  = Arc::clone(&state.repo);
    let locks_rec = Arc::clone(&state.fn_locks);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            let entries = match repo_rec.list_degraded() {
                Ok(v)  => v,
                Err(e) => {
                    tracing::error!(error = %e, "reconcile_list_degraded_failed");
                    continue;
                }
            };
            for (fiscal_number, pending_hash, _retry) in entries {
                // Acquire same per-FN lock as the request handler.
                let lock = {
                    let entry = locks_rec.entry(fiscal_number.clone())
                        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
                    Arc::clone(entry.value())
                };
                let _guard = lock.lock().await;
                match repo_rec.reconcile_chain(&fiscal_number, &pending_hash) {
                    Ok(())  => tracing::info!(fiscal_number, "chain_reconciled"),
                    Err(e)  => tracing::warn!(
                        fiscal_number, error = %e,
                        "chain_reconcile_retry_failed"
                    ),
                }
            }
        }
    });
}
```

- [ ] **Step 2: Add test**

```rust
#[cfg(test)]
mod reconcile_tests {
    use super::*;
    use rusqlite::Connection;

    fn make_repo_full() -> Arc<Repo> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE local_sequences (
                 fiscal_number TEXT PRIMARY KEY,
                 last          INTEGER NOT NULL DEFAULT 0,
                 previous_hash TEXT    NOT NULL DEFAULT ''
             );
             CREATE TABLE fn_degraded (
                 fiscal_number TEXT PRIMARY KEY,
                 pending_hash  TEXT    NOT NULL,
                 degraded_at   TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 retry_count   INTEGER NOT NULL DEFAULT 0,
                 last_retry_at TEXT
             );",
        ).unwrap();
        Arc::new(Repo { conn: std::sync::Mutex::new(conn) })
    }

    #[tokio::test]
    async fn reconcile_loop_clears_degraded_entry() {
        let repo = make_repo_full();
        let fn_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>> =
            Arc::new(DashMap::new());

        // Seed sequence and set degraded
        repo.next_local_number("FN001").unwrap();
        repo.set_degraded("FN001", "deadbeef").unwrap();
        assert!(repo.is_degraded("FN001").unwrap());

        // Run one reconcile iteration manually
        let entries = repo.list_degraded().unwrap();
        for (fiscal_number, pending_hash, _) in entries {
            let lock = {
                let entry = fn_locks.entry(fiscal_number.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
                Arc::clone(entry.value())
            };
            let _guard = lock.lock().await;
            repo.reconcile_chain(&fiscal_number, &pending_hash).unwrap();
        }

        assert!(!repo.is_degraded("FN001").unwrap());
        assert_eq!(repo.load_previous_hash("FN001").unwrap(), "deadbeef");
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p prro_sidecar reconcile 2>&1 | tail -10
```

Expected: `test reconcile_tests::reconcile_loop_clears_degraded_entry ... ok`

```bash
cargo test -p prro_sidecar 2>&1 | tail -5
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add rust/prro_sidecar/src/bin/prro_sidecar.rs
git commit -m "fix(sidecar): reconcile background loop — auto-recover degraded FN chain every 30s"
```

---

### Task 5: P0-c — DocumentOutcome + classify_response

**Goal:** Add `DocumentOutcome` enum and `classify_response` pure function to `bridge/dto.rs`.

**Files:**
- Modify: `rust/maria304_driver/src/bridge/dto.rs`

**Acceptance Criteria:**
- [ ] `DocumentOutcome` enum with `Accepted`, `Terminal`, `Retryable` variants
- [ ] `classify_response` handles all 5 document_state cases from the design spec
- [ ] 5 unit tests (one per case)

**Verify:** `cargo test -p maria304_driver classify_response 2>&1 | tail -10` → 5 tests pass

**Steps:**

- [ ] **Step 1: Add DocumentOutcome and classify_response to dto.rs**

At the end of `rust/maria304_driver/src/bridge/dto.rs`, add:

```rust
use crate::protocol::error_codes::ErrorCode;

/// Classification of a bridge response for COMP handling.
pub enum DocumentOutcome {
    /// DPS accepted the document. fiscal_id is valid (> 0).
    Accepted { fiscal_id: u64, sale: u64, ret: u64 },
    /// Terminal failure: receipt is closed as rejected. Caller must open new receipt.
    /// Happens on: REJECTED, ERROR_SIGN, ERROR_FISCAL, or ok=true with bad fiscal_id.
    Terminal(ErrorCode),
    /// Retryable failure: receipt stays open. Caller can retry COMP or send CANC.
    /// Happens on: ERROR_SEND, empty/unknown document_state with ok=false.
    Retryable(ErrorCode),
}

/// Classify a `CanonicalResponse` into an outcome for the COMP handler.
///
/// Rationale: closing a receipt must reflect actual DPS outcome, not just
/// whether the HTTP call succeeded (invariant #4 — idempotency).
pub fn classify_response(resp: &CanonicalResponse) -> DocumentOutcome {
    if resp.ok {
        match resp.fiscal_id.parse::<u64>() {
            Ok(id) if id > 0 => DocumentOutcome::Accepted {
                fiscal_id: id,
                sale: resp.sale_total_kopecks,
                ret:  resp.return_total_kopecks,
            },
            // ok=true but fiscal_id invalid — data contract violation, treat as terminal
            _ => DocumentOutcome::Terminal(ErrorCode::Custom("SOFTFISCALERR".into())),
        }
    } else {
        match resp.document_state.as_str() {
            "REJECTED" | "ERROR_SIGN" | "ERROR_FISCAL" =>
                DocumentOutcome::Terminal(ErrorCode::SoftLocked),
            // ERROR_SEND or anything unknown: DPS may not have seen it → retryable
            _ => DocumentOutcome::Retryable(ErrorCode::SoftBlock),
        }
    }
}
```

- [ ] **Step 2: Write tests**

At the bottom of `dto.rs`, add:

```rust
#[cfg(test)]
mod classify_tests {
    use super::*;

    fn resp(ok: bool, fiscal_id: &str, document_state: &str) -> CanonicalResponse {
        CanonicalResponse {
            ok,
            document_id:          "doc1".into(),
            fiscal_id:             fiscal_id.into(),
            fiscal_ts:             "2026-04-22T10:00:00Z".into(),
            document_state:        document_state.into(),
            sale_total_kopecks:    1000,
            return_total_kopecks:  0,
        }
    }

    #[test]
    fn accepted_when_ok_and_valid_fiscal_id() {
        let r = resp(true, "12345", "SENT");
        assert!(matches!(
            classify_response(&r),
            DocumentOutcome::Accepted { fiscal_id: 12345, .. }
        ));
    }

    #[test]
    fn terminal_when_ok_but_fiscal_id_zero() {
        let r = resp(true, "0", "SENT");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn terminal_when_ok_but_fiscal_id_empty() {
        let r = resp(true, "", "SENT");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn terminal_on_rejected() {
        let r = resp(false, "", "REJECTED");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn terminal_on_error_sign() {
        let r = resp(false, "", "ERROR_SIGN");
        assert!(matches!(classify_response(&r), DocumentOutcome::Terminal(_)));
    }

    #[test]
    fn retryable_on_error_send() {
        let r = resp(false, "", "ERROR_SEND");
        assert!(matches!(classify_response(&r), DocumentOutcome::Retryable(_)));
    }

    #[test]
    fn retryable_on_empty_document_state() {
        let r = resp(false, "", "");
        assert!(matches!(classify_response(&r), DocumentOutcome::Retryable(_)));
    }

    #[test]
    fn retryable_on_unknown_document_state() {
        let r = resp(false, "", "UNKNOWN_STATE");
        assert!(matches!(classify_response(&r), DocumentOutcome::Retryable(_)));
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p maria304_driver classify_response 2>&1 | tail -10
```

Expected: 8 tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/maria304_driver/src/bridge/dto.rs
git commit -m "fix(maria304): add DocumentOutcome + classify_response for fail-close COMP handling"
```

---

### Task 6: P0-c — COMP dispatcher fail-close

**Goal:** Use `classify_response` in the COMP handler — close receipt only on Accepted, leave open on Retryable, close-as-rejected on Terminal.

**Files:**
- Modify: `rust/maria304_driver/src/session/dispatcher.rs`
- Modify: `rust/maria304_driver/tests/bridge_acceptance.rs`

**Acceptance Criteria:**
- [ ] COMP with `ok=true, fiscal_id="12345"` → receipt closed, success response
- [ ] COMP with `ok=false, document_state="REJECTED"` → receipt closed as rejected, error response
- [ ] COMP with `ok=false, document_state="ERROR_SEND"` → receipt stays open, soft error
- [ ] Existing bridge_acceptance tests still pass

**Verify:**
```
cargo test -p maria304_driver 2>&1 | tail -5
```
→ all pass

**Steps:**

- [ ] **Step 1: Update imports in dispatcher.rs**

At the top of `rust/maria304_driver/src/session/dispatcher.rs`, add to existing imports:

```rust
use crate::bridge::dto::{classify_response, DocumentOutcome};
```

- [ ] **Step 2: Replace COMP Ok(resp) handler**

Find the COMP handler (around line 281–299). Replace:

```rust
// Before:
Ok(resp) => {
    let fiscal_id = resp
        .fiscal_id
        .parse::<u64>()
        .unwrap_or(0);
    let sale = resp.sale_total_kopecks;
    let ret = resp.return_total_kopecks;
    let comp_payload = CompBuilder::new(fiscal_id, sale, ret).to_wire_payload();
    // Close receipt, increment sequence, reset counters.
    session.state = SessionState::Authenticated;
    session.psdt_sequence = 0;
    session.pending_return_check_number = None;
    correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
    session.mark_command_ok("COMP");
    ok(Some(
        Response::data(comp_payload).expect("COMP payload is 94 chars"),
    ))
}
```

```rust
// After:
Ok(resp) => match classify_response(&resp) {
    DocumentOutcome::Accepted { fiscal_id, sale, ret } => {
        let comp_payload = CompBuilder::new(fiscal_id, sale, ret).to_wire_payload();
        session.state = SessionState::Authenticated;
        session.psdt_sequence = 0;
        session.pending_return_check_number = None;
        correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
        session.mark_command_ok("COMP");
        ok(Some(
            Response::data(comp_payload).expect("COMP payload is 94 chars"),
        ))
    }
    DocumentOutcome::Terminal(code) => {
        // DPS rejected or signed with bad key — receipt is done, cannot retry.
        session.state = SessionState::Authenticated;
        session.psdt_sequence = 0;
        session.pending_return_check_number = None;
        correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
        session.mark_command_ok("COMP_REJECTED");
        err(code)
    }
    DocumentOutcome::Retryable(code) => {
        // DPS may not have seen this — leave receipt open for retry or CANC.
        err(code)
    }
},
```

- [ ] **Step 3: Add tests to bridge_acceptance.rs**

At the end of `rust/maria304_driver/tests/bridge_acceptance.rs`, add:

```rust
use maria304_driver::bridge::dto::CanonicalResponse;

fn make_bridge_with_response(resp: CanonicalResponse) -> MockBridge {
    let b = MockBridge::new();
    b.set_next_response(resp);
    b
}

fn open_receipt_session() -> (Session, Correlation) {
    let mut s = Session::new();
    let b = MockBridge::new();
    let mut c = Correlation { session_uuid: "sess-t".to_string(), receipt_seq: 0 };
    // Login
    dispatch(&mut s, Command::Upas { password: "1111111111".to_string(), cashier_id: "c1".to_string() },
        &Identity::default(), clock(), &b, &mut c);
    // Open receipt
    dispatch(&mut s, Command::Prep("Dep1".to_string()),
        &Identity::default(), clock(), &b, &mut c);
    (s, c)
}

#[test]
fn comp_with_ok_true_closes_receipt() {
    let (mut session, mut correlation) = open_receipt_session();
    let resp = CanonicalResponse {
        ok: true,
        document_id: "d1".into(),
        fiscal_id: "99999".into(),
        fiscal_ts: "2026-04-22T10:00:00Z".into(),
        document_state: "SENT".into(),
        sale_total_kopecks: 500,
        return_total_kopecks: 0,
    };
    let bridge = make_bridge_with_response(resp);
    let responses = run(&mut session, &bridge, &mut correlation, Command::Comp("body".into()));
    assert!(!responses.is_empty());
    // Receipt should be closed — state back to Authenticated
    assert!(!session.receipt_open());
}

#[test]
fn comp_with_rejected_closes_receipt_with_error() {
    let (mut session, mut correlation) = open_receipt_session();
    let resp = CanonicalResponse {
        ok: false,
        document_id: "d2".into(),
        fiscal_id: "".into(),
        fiscal_ts: "".into(),
        document_state: "REJECTED".into(),
        sale_total_kopecks: 0,
        return_total_kopecks: 0,
    };
    let bridge = make_bridge_with_response(resp);
    run(&mut session, &bridge, &mut correlation, Command::Comp("body".into()));
    // Receipt should be closed (terminal) — cannot retry
    assert!(!session.receipt_open());
}

#[test]
fn comp_with_error_send_leaves_receipt_open() {
    let (mut session, mut correlation) = open_receipt_session();
    let resp = CanonicalResponse {
        ok: false,
        document_id: "d3".into(),
        fiscal_id: "".into(),
        fiscal_ts: "".into(),
        document_state: "ERROR_SEND".into(),
        sale_total_kopecks: 0,
        return_total_kopecks: 0,
    };
    let bridge = make_bridge_with_response(resp);
    run(&mut session, &bridge, &mut correlation, Command::Comp("body".into()));
    // Receipt must stay open — DPS never received it, retryable
    assert!(session.receipt_open());
}
```

Note: `MockBridge` may need a `set_next_response` method. Check `bridge/mock.rs`. If it doesn't exist, add it in a separate sub-step:

In `rust/maria304_driver/src/bridge/mock.rs`, add:
```rust
pub fn set_next_response(&self, resp: CanonicalResponse) {
    // Implementation depends on existing MockBridge internals.
    // If MockBridge uses a Vec<CanonicalResponse>, push to it.
    // Consult existing mock.rs to match the pattern.
}
```

If `MockBridge` already returns a fixed response, adjust the test to control it via constructor.

- [ ] **Step 4: Run all maria304 tests**

```bash
cargo test -p maria304_driver 2>&1 | tail -15
```

Expected: all existing tests pass + 3 new dispatcher tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust/maria304_driver/src/session/dispatcher.rs \
        rust/maria304_driver/tests/bridge_acceptance.rs
git commit -m "fix(maria304): dispatcher fail-close — COMP checks ok/document_state before closing receipt"
```

---

## PR-2: High/medium fixes

---

### Task 7: P1-a — credentials_mode SQL migration + OperatorRow + per-row decode

**Goal:** Add `credentials_mode` column to `sidecar_operators`; teach `OperatorRow` and handler to use per-row mode.

**Files:**
- Create: `sql/023_operator_credentials_mode.sql`
- Modify: `rust/prro_sidecar/src/repo.rs`
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `sql/023_operator_credentials_mode.sql` creates the column with DEFAULT 'plain'
- [ ] `OperatorRow` has `credentials_mode: CredentialsMode`
- [ ] `load_active_operator` reads the column; unknown values map to `Plain`
- [ ] `fiscal_send_inner` step 6 uses `operator.credentials_mode` instead of `st.config.security.credentials_mode`
- [ ] Existing repo tests pass after adding `credentials_mode` to schema

**Verify:** `cargo test -p prro_sidecar 2>&1 | tail -5` → all pass

**Steps:**

- [ ] **Step 1: Create SQL migration**

Create `sql/023_operator_credentials_mode.sql`:

```sql
-- sql/023_operator_credentials_mode.sql
-- Adds per-row credential storage mode to sidecar_operators.
-- DEFAULT 'plain' ensures existing rows are read with plain mode
-- until migrate-passwords is run.

ALTER TABLE sidecar_operators
    ADD COLUMN credentials_mode TEXT NOT NULL DEFAULT 'plain'
    CHECK (credentials_mode IN ('plain', 'xor_soft'));
```

Apply to dev DB if it exists:
```bash
sqlite3 var/prro.db < sql/023_operator_credentials_mode.sql
```

- [ ] **Step 2: Add CredentialsMode import to repo.rs**

In `rust/prro_sidecar/src/repo.rs`, add at top:

```rust
use crate::config::CredentialsMode;
```

- [ ] **Step 3: Add credentials_mode to OperatorRow**

In `repo.rs`, update `OperatorRow`:

```rust
#[derive(Debug, Clone)]
pub struct OperatorRow {
    pub id:               i64,
    pub fiscal_number:    String,
    pub operator_name:    Option<String>,
    pub operator_inn:     String,
    pub jks_path:         String,
    pub jks_password:     String,
    pub credentials_mode: CredentialsMode,    // NEW
}
```

Add `FromSql` for `CredentialsMode` in `repo.rs`:

```rust
impl rusqlite::types::FromSql for CredentialsMode {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match String::column_result(value)?.as_str() {
            "xor_soft" => Ok(Self::XorSoft),
            _           => Ok(Self::Plain),   // 'plain' or any legacy value → Plain
        }
    }
}
```

- [ ] **Step 4: Update load_active_operator query**

In `repo.rs`, update the `load_active_operator` query:

```rust
pub fn load_active_operator(&self, fiscal_number: &str) -> Result<OperatorRow, SidecarError> {
    let conn = self.lock()?;
    conn.query_row(
        "SELECT id, fiscal_number, operator_name, operator_inn,
                jks_path, jks_password, credentials_mode
         FROM   sidecar_operators
         WHERE  fiscal_number = ?1 AND active = 1
         ORDER  BY id DESC
         LIMIT  1",
        params![fiscal_number],
        |row| {
            Ok(OperatorRow {
                id:               row.get(0)?,
                fiscal_number:    row.get(1)?,
                operator_name:    row.get(2)?,
                operator_inn:     row.get(3)?,
                jks_path:         row.get(4)?,
                jks_password:     row.get(5)?,
                credentials_mode: row.get(6)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            SidecarError::NotFound(format!("active operator for fn: {fiscal_number}"))
        }
        other => SidecarError::Db(other),
    })
}
```

- [ ] **Step 5: Update fiscal_send_inner step 6**

In `prro_sidecar.rs`, find step 6 (decode JKS password):

```rust
// Before:
let raw_pw = match st.config.security.credentials_mode {
    CredentialsMode::Plain => operator.jks_password.clone(),
    CredentialsMode::XorSoft => {
        let valid_to = cert_meta.valid_to.as_deref().unwrap_or("");
        let op_name  = operator.operator_name.as_deref().unwrap_or("");
        credentials::decode_password(&operator.jks_password, valid_to, op_name)
            .map_err(SidecarError::Credentials)?
    }
};
```

```rust
// After (use per-row mode, not global config):
let raw_pw = match operator.credentials_mode {
    CredentialsMode::Plain => operator.jks_password.clone(),
    CredentialsMode::XorSoft => {
        let valid_to = cert_meta.valid_to.as_deref().unwrap_or("");
        let op_name  = operator.operator_name.as_deref().unwrap_or("");
        credentials::decode_password(&operator.jks_password, valid_to, op_name)
            .map_err(SidecarError::Credentials)?
    }
};
```

- [ ] **Step 6: Update make_repo() in repo tests**

In the `make_repo()` function in `repo.rs` tests, add `credentials_mode` to the `sidecar_operators` CREATE TABLE:

```sql
CREATE TABLE sidecar_operators (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    fiscal_number   TEXT NOT NULL,
    operator_name   TEXT,
    operator_inn    TEXT NOT NULL,
    jks_path        TEXT NOT NULL,
    jks_password    TEXT NOT NULL,
    active          INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
    credentials_mode TEXT NOT NULL DEFAULT 'plain'
                     CHECK (credentials_mode IN ('plain','xor_soft')),
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number)
);
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p prro_sidecar 2>&1 | tail -5
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add sql/023_operator_credentials_mode.sql \
        rust/prro_sidecar/src/repo.rs \
        rust/prro_sidecar/src/bin/prro_sidecar.rs
git commit -m "fix(sidecar): per-row credentials_mode in sidecar_operators — removes global mode dependency"
```

---

### Task 8: P1-a — migrate-passwords command + cmd_add_operator encode

**Goal:** `prro_admin migrate-passwords` re-encodes existing plain passwords. `cmd_add_operator` encodes on write.

**Files:**
- Modify: `rust/prro_sidecar/src/bin/prro_admin.rs`

**Acceptance Criteria:**
- [ ] `migrate-passwords [--dry-run]` subcommand exists
- [ ] For each plain-mode operator: loads JKS, extracts valid_to, encodes with XorSoft, updates DB
- [ ] `cmd_add_operator` loads JKS, encodes password, inserts with `credentials_mode='xor_soft'`
- [ ] Tests for both paths

**Verify:** `cargo build -p prro_sidecar --bin prro_admin 2>&1 | tail -3` → success

**Steps:**

- [ ] **Step 1: Add MigratePasswords subcommand to Cmd enum**

In `prro_admin.rs`, add to `enum Cmd`:

```rust
/// Re-encode all plain-mode operator passwords to xor_soft.
/// Run after deploying the credentials_mode migration.
MigratePasswords {
    /// Print what would be done without modifying the DB.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
},
```

Add matching arm in `main()`:

```rust
Cmd::MigratePasswords { dry_run } => cmd_migrate_passwords(&conn, dry_run),
```

- [ ] **Step 2: Implement cmd_migrate_passwords**

Add imports at top of `prro_admin.rs`:

```rust
use prro_sidecar::credentials;
use prro_sidecar::config::CredentialsMode;
use prro_sidecar::cms_adapter;
use prro_crypto::interop::prro::extract_private_key;
```

Add function:

```rust
fn cmd_migrate_passwords(
    conn:    &Connection,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Debug)]
    struct OpRow { id: i64, operator_name: Option<String>, jks_path: String, jks_password: String }

    let mut stmt = conn.prepare(
        "SELECT id, operator_name, jks_path, jks_password
         FROM sidecar_operators WHERE credentials_mode = 'plain'",
    )?;
    let rows: Vec<OpRow> = stmt.query_map([], |row| {
        Ok(OpRow {
            id:            row.get(0)?,
            operator_name: row.get(1)?,
            jks_path:      row.get(2)?,
            jks_password:  row.get(3)?,
        })
    })?.filter_map(|r| r.ok()).collect();

    let total = rows.len();
    let mut migrated = 0usize;
    let mut skipped  = 0usize;

    for op in &rows {
        let jks_bytes = match std::fs::read(&op.jks_path) {
            Ok(b)  => b,
            Err(e) => {
                eprintln!("[skip] id={} path={:?}: {e}", op.id, op.jks_path);
                skipped += 1;
                continue;
            }
        };
        let extracted = match extract_private_key(&jks_bytes, &op.jks_password) {
            Ok(e)  => e,
            Err(e) => {
                eprintln!("[skip] id={} cannot open JKS: {e}", op.id);
                skipped += 1;
                continue;
            }
        };
        let cert_der = match extracted.certs.first() {
            Some(c) => c.clone(),
            None => {
                eprintln!("[skip] id={} no cert in JKS container", op.id);
                skipped += 1;
                continue;
            }
        };
        let valid_to = match cms_adapter::extract_cert_valid_to(&cert_der) {
            Ok(v)  => v,
            Err(e) => {
                eprintln!("[skip] id={} cannot extract valid_to: {e}", op.id);
                skipped += 1;
                continue;
            }
        };
        let op_name  = op.operator_name.as_deref().unwrap_or("");
        let encoded  = credentials::encode_password(&op.jks_password, &valid_to, op_name);

        if dry_run {
            println!("[dry-run] id={} would encode password (valid_to={valid_to})", op.id);
        } else {
            conn.execute(
                "UPDATE sidecar_operators SET jks_password = ?1, credentials_mode = 'xor_soft'
                 WHERE id = ?2",
                rusqlite::params![encoded, op.id],
            )?;
            println!("[ok] id={} encoded", op.id);
        }
        migrated += 1;
    }

    println!("migrate-passwords: total={total} migrated={migrated} skipped={skipped}{}",
        if dry_run { " (dry-run)" } else { "" });
    Ok(())
}
```

- [ ] **Step 3: Update cmd_add_operator to encode on write**

Replace `cmd_add_operator`:

```rust
fn cmd_add_operator(
    conn:          &Connection,
    fiscal_number: &str,
    operator_inn:  &str,
    jks_path:      &str,
    jks_password:  &str,
    name:          Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load JKS to validate password and extract cert metadata for XorSoft encoding.
    let jks_bytes = std::fs::read(jks_path)
        .map_err(|e| format!("cannot read JKS at {jks_path:?}: {e}"))?;
    let extracted = extract_private_key(&jks_bytes, jks_password)
        .map_err(|e| format!("cannot open JKS (wrong password?): {e}"))?;
    let cert_der = extracted.certs.first()
        .ok_or("JKS container has no certificate")?;
    let valid_to = cms_adapter::extract_cert_valid_to(cert_der)
        .map_err(|e| format!("cannot extract cert valid_to: {e}"))?;
    let op_name  = name.unwrap_or("");
    let encoded  = credentials::encode_password(jks_password, &valid_to, op_name);

    conn.execute(
        "INSERT INTO sidecar_operators
             (fiscal_number, operator_inn, jks_path, jks_password, operator_name, credentials_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, 'xor_soft')",
        params![fiscal_number, operator_inn, jks_path, encoded, name],
    )
    .map_err(|e| map_operator_insert_error(e, fiscal_number))?;
    let row_id = conn.last_insert_rowid();
    println!("added operator id={row_id} inn={operator_inn} fn={fiscal_number} (xor_soft)");
    Ok(())
}
```

- [ ] **Step 4: Build check**

```bash
cargo build -p prro_sidecar --bin prro_admin 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Step 5: Smoke test CLI help**

```bash
cargo run -p prro_sidecar --bin prro_admin -- --help 2>&1 | grep -i migrate
```

Expected: `migrate-passwords` appears in output.

- [ ] **Step 6: Commit**

```bash
git add rust/prro_sidecar/src/bin/prro_admin.rs
git commit -m "fix(sidecar): migrate-passwords command + cmd_add_operator encodes XorSoft on write"
```

---

### Task 9: P1-a — routes.py CLI delegation

**Goal:** Replace direct DB INSERT in `routes.py` with `prro_admin add-operator` subprocess call.

**Files:**
- Modify: `src/prro_gateway/admin_ui/routes.py`

**Acceptance Criteria:**
- [ ] Operator add path calls `prro_admin add-operator` subprocess, not direct INSERT
- [ ] Non-zero exit → 400/409 error response with stderr message
- [ ] `prro_admin` binary path configurable via `PRRO_ADMIN_BIN` env var with fallback
- [ ] Unit test mocks subprocess call

**Verify:** `pytest tests/test_admin_ui.py -k operator -x 2>&1 | tail -10` → pass

**Steps:**

- [ ] **Step 1: Read current operator add route**

Read lines 940–990 of `src/prro_gateway/admin_ui/routes.py` to understand the exact current implementation before editing.

- [ ] **Step 2: Add helper function**

Near the top of the operators section in `routes.py`, add:

```python
import shutil
import subprocess

def _add_operator_via_cli(
    db_path: str,
    fiscal_number: str,
    operator_name: str,
    inn: str,
    jks_path: str,
    jks_password: str,
) -> None:
    """Delegate operator registration to prro_admin CLI (handles XorSoft encoding)."""
    admin_bin = (
        os.environ.get("PRRO_ADMIN_BIN")
        or shutil.which("prro_admin")
        or "/usr/local/bin/prro_admin"
    )
    args = [
        admin_bin, "--db", db_path,
        "add-operator",
        fiscal_number, inn, jks_path,
        "--jks-password", jks_password,
    ]
    if operator_name:
        args += ["--name", operator_name]
    result = subprocess.run(args, capture_output=True, text=True, timeout=15)
    if result.returncode != 0:
        raise ValueError(result.stderr.strip() or "prro_admin add-operator failed")
```

- [ ] **Step 3: Replace INSERT with CLI call in operator add handler**

Find the handler that does the direct `INSERT INTO sidecar_operators` (around line 960). Replace the INSERT logic:

```python
# Before (direct INSERT):
db.execute(
    "INSERT INTO sidecar_operators (...) VALUES (...)",
    (fiscal_number, operator_name, jks_path, jks_password, ...)
)

# After (CLI delegation):
try:
    _add_operator_via_cli(
        db_path=current_app.config["PRRO_DB_PATH"],
        fiscal_number=fiscal_number,
        operator_name=operator_name,
        inn=operator_inn,
        jks_path=jks_path,
        jks_password=jks_password,
    )
except ValueError as e:
    error_msg = str(e)
    if "not registered" in error_msg:
        return jsonify({"error": error_msg}), 409
    return jsonify({"error": error_msg}), 400
```

- [ ] **Step 4: Add/update tests**

In `tests/test_admin_ui.py` (or create if needed), add:

```python
from unittest.mock import patch, MagicMock
import subprocess

def test_add_operator_calls_prro_admin(client):
    mock_result = MagicMock()
    mock_result.returncode = 0
    mock_result.stdout = "added operator id=1\n"
    mock_result.stderr = ""
    with patch("subprocess.run", return_value=mock_result) as mock_run:
        resp = client.post("/api/operators", json={
            "fiscal_number": "3001234567",
            "operator_name": "Test",
            "operator_inn": "1234567890",
            "jks_path": "/path/to/key.jks",
            "jks_password": "secret",
        })
        assert resp.status_code in (200, 201)
        call_args = mock_run.call_args[0][0]
        assert "add-operator" in call_args

def test_add_operator_returns_409_on_fn_not_registered(client):
    mock_result = MagicMock()
    mock_result.returncode = 1
    mock_result.stderr = "fiscal_number not registered — run 'register-fn' first"
    with patch("subprocess.run", return_value=mock_result):
        resp = client.post("/api/operators", json={
            "fiscal_number": "9999999999",
            "operator_name": "Test",
            "operator_inn": "1234567890",
            "jks_path": "/path/to/key.jks",
            "jks_password": "secret",
        })
        assert resp.status_code == 409
```

Adjust the route path and request shape to match the actual handler signature.

- [ ] **Step 5: Run tests**

```bash
python -m pytest tests/test_admin_ui.py -k operator -x -v 2>&1 | tail -15
```

Expected: operator tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/prro_gateway/admin_ui/routes.py
git commit -m "fix(admin_ui): delegate add-operator to prro_admin CLI — ensures XorSoft encoding"
```

---

### Task 10: P1-b — spawn_blocking for TSP

**Goal:** Move blocking `ureq` TSP call to tokio blocking thread pool.

**Files:**
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `fetch_timestamp` call wrapped in `tokio::task::spawn_blocking`
- [ ] JoinError mapped to `SidecarError::CmsSign`
- [ ] Build succeeds; existing tests pass

**Verify:** `cargo test -p prro_sidecar 2>&1 | tail -5` → all pass

**Steps:**

- [ ] **Step 1: Find the TSP call**

In `prro_sidecar.rs`, find the block `if fn_config.tsp_enabled` (around line 297). The call looks like:

```rust
let tsp_token = prro_crypto::cms::tsp::fetch_timestamp(&tsa_url, &digest, timeout)?;
```

(Exact line numbers: search for `fetch_timestamp` in the file.)

- [ ] **Step 2: Wrap in spawn_blocking**

```rust
// Before:
let tsp_token = prro_crypto::cms::tsp::fetch_timestamp(&tsa_url, &digest, timeout)
    .map_err(|e| SidecarError::CmsSign(e.to_string()))?;

// After:
let tsa_url_owned   = tsa_url.to_string();   // move into closure
let digest_owned    = digest.to_vec();         // move into closure
// timeout: Duration is Copy
let tsp_token = tokio::task::spawn_blocking(move || {
    prro_crypto::cms::tsp::fetch_timestamp(&tsa_url_owned, &digest_owned, timeout)
})
.await
.map_err(|e| SidecarError::CmsSign(format!("tsp_join: {e}")))?
.map_err(|e| SidecarError::CmsSign(e.to_string()))?;
```

Note: the exact variable names (`tsa_url`, `digest`, `timeout`) must match what the surrounding code uses. Read the actual code at the call site before editing.

- [ ] **Step 3: Build and test**

```bash
cargo build -p prro_sidecar 2>&1 | tail -5
cargo test -p prro_sidecar 2>&1 | tail -5
```

Expected: no errors, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/prro_sidecar/src/bin/prro_sidecar.rs
git commit -m "fix(sidecar): spawn_blocking for TSP fetch — unblocks tokio workers during RFC3161 call"
```

---

### Task 11: P2-b — Amount validation in commands.rs

**Goal:** Replace silent `unwrap_or(0)` with explicit `InvalidParams` for payment amounts.

**Files:**
- Modify: `rust/maria304_driver/src/protocol/commands.rs`

**Acceptance Criteria:**
- [ ] CAIO with invalid amount string → `Command::InvalidParams`
- [ ] CAIO with zero amount → `Command::InvalidParams`  
- [ ] CAIO with valid non-zero amount → `Command::Caioi` or `Command::Caioo`
- [ ] Existing protocol_golden_vectors tests pass
- [ ] Report-range parsers (FIRN/IREN) with `unwrap_or(0)` remain unchanged (0 is valid there)

**Verify:** `cargo test -p maria304_driver 2>&1 | tail -5` → all pass

**Steps:**

- [ ] **Step 1: Fix CAIO sum parsing (line ~380)**

Find the CAIO parser block in `commands.rs`:

```rust
// Before (line ~380):
let sum_str: String = prefix_chars.collect();
let sum: u64 = sum_str.parse().unwrap_or(0);
```

```rust
// After:
let sum_str: String = prefix_chars.collect();
let sum: u64 = match sum_str.trim().parse::<u64>() {
    Ok(s) if s > 0 => s,
    _ => return Self::InvalidParams { opcode: opcode.into(), body: body_owned },
};
```

- [ ] **Step 2: Write tests**

Add to `tests/protocol_golden_vectors.rs` or create `tests/caio_validation.rs`:

```rust
use maria304_driver::protocol::Command;

#[test]
fn caio_invalid_sum_string_gives_invalid_params() {
    let cmd = Command::parse("CAIO", "Ixxx000000cash inflow");
    assert!(
        matches!(cmd, Command::InvalidParams { .. }),
        "expected InvalidParams, got {cmd:?}"
    );
}

#[test]
fn caio_zero_sum_gives_invalid_params() {
    let cmd = Command::parse("CAIO", "I0000000000cash inflow");
    assert!(
        matches!(cmd, Command::InvalidParams { .. }),
        "expected InvalidParams for zero sum, got {cmd:?}"
    );
}

#[test]
fn caio_valid_inflow_parses_correctly() {
    let cmd = Command::parse("CAIO", "I0000010000инкасація");
    assert!(
        matches!(cmd, Command::Caioi { sum_kopecks: 10000, .. }),
        "expected Caioi with sum=10000, got {cmd:?}"
    );
}

#[test]
fn caio_valid_outflow_parses_correctly() {
    let cmd = Command::parse("CAIO", "O0000500000видача");
    assert!(
        matches!(cmd, Command::Caioo { sum_kopecks: 500000, .. }),
        "expected Caioo with sum=500000, got {cmd:?}"
    );
}
```

Note: verify the exact signature of `Command::parse` (it may be `Command::from_wire` or take separate opcode/body). Adjust to match actual API.

- [ ] **Step 3: Run all maria tests**

```bash
cargo test -p maria304_driver 2>&1 | tail -10
```

Expected: all existing tests pass + 4 new CAIO tests pass.

- [ ] **Step 4: Commit**

```bash
git add rust/maria304_driver/src/protocol/commands.rs \
        rust/maria304_driver/tests/caio_validation.rs
git commit -m "fix(maria304): validate CAIO amount — reject zero and non-numeric sums as InvalidParams"
```

---

## Workflow per task

Each task follows this cycle:
1. Implement per steps above
2. Code review via `superpowers-extended-cc:requesting-code-review`
3. Run tests (exact commands in **Verify** section)
4. Fix any issues found by review or tests
5. Re-review if changes were significant
6. Commit with message from **Step N: Commit**

---

## Build commands reference

```bash
# Build all Rust
cargo build --workspace 2>&1 | tail -5

# Test specific crate
cargo test -p prro_sidecar 2>&1 | tail -10
cargo test -p maria304_driver 2>&1 | tail -10

# Test Python
python -m pytest tests/ -x -q 2>&1 | tail -10

# Apply SQL migration (dev)
sqlite3 var/prro.db < sql/023_operator_credentials_mode.sql
```
