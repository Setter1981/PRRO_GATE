//! W5 — Stage 1+2 (acquire+validate+guard) targeted fixtures.
//!
//! Drives `services::write_path::stage_acquire::run` directly per
//! the M3a Task 5 plan ("dispatcher deferred"); each fixture seeds a
//! freshly-bootstrapped pool with the FN row + (optionally) shift +
//! inbox NEW row, then asserts the resulting `WorkerProcessResult`
//! variant AND the persisted DB state — with explicit attention to
//! "guard fail does NOT consume an lnd" and "no partial inbox/doc/lnd
//! on collision".

use prro::db::models::enums::{DocType, FiscalMode, NodeMode, Severity, ShiftState};
use prro::db::models::ids::{RequestId, ShiftId};
use prro::db::{open_pool, open_secure_pool};
use prro::db::repositories::{
    fiscal_documents as fd, fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig,
    ingress_inbox as inbox, ingress_inbox::NewInboxEntry, shifts,
};
use prro::services::write_path::{
    stage_acquire,
    types::{CanonicalFiscalCommand, RejectionReason, WorkerProcessResult},
};

const FN: &str = "4000000001";

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w5.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

// W4-Z2a piece 6 — stage_acquire now requires pool_secure (for
// tax_snapshot load) + driver_id.  Tests with minimal payloads (no
// tax_group_1 items) use an empty fresh secure pool — no tax_groups
// configured → snapshot has empty groups → derive_check_tax_summaries
// short-circuits without TaxMappingNotWired (which only fires when
// items reference a tax_group_1).
async fn fresh_secure_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w5-secure.db");
    std::mem::forget(dir);
    open_secure_pool(&path).await.unwrap()
}

const TEST_DRIVER_ID: &str = "test-driver";

async fn seed_fn_config(pool: &sqlx::SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
            tax_number: "12345678".into(),
            vat_payer_inn: None,
            fiscal_mode: FiscalMode::Test,
            org_name: None,
            point_name: None,
            org_address: None,
            tsp_enabled: false,
            offline_enabled: true,
            national_check_enabled: false,
            min_offline_codes: 0,
            max_offline_codes: 0,
        },
    )
    .await
    .unwrap();
}

async fn seed_node_state(
    pool: &sqlx::SqlitePool,
    backend: Option<&str>,
    transport: Option<&str>,
    mode: NodeMode,
    shift_state: ShiftState,
    current_shift_id: Option<ShiftId>,
) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(FN)
    .bind(mode)
    .bind(shift_state)
    .bind(current_shift_id)
    .bind(backend)
    .bind(transport)
    .execute(pool)
    .await
    .unwrap();
}

/// Convenience wrapper for tests with no shift (Closed / lease miss /
/// SHIFT_OPEN happy path / mode-side reject / MissingProfileBinding).
async fn seed_fn_with_profiles(
    pool: &sqlx::SqlitePool,
    backend: Option<&str>,
    transport: Option<&str>,
    mode: NodeMode,
    shift_state: ShiftState,
    current_shift_id: Option<ShiftId>,
) {
    seed_fn_config(pool).await;
    seed_node_state(
        pool,
        backend,
        transport,
        mode,
        shift_state,
        current_shift_id,
    )
    .await;
}

async fn seed_open_shift(pool: &sqlx::SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(shift_id)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

/// LOW-C4-1: seed a shift with explicit caller-chosen state + opener
/// id.  Used by the W14a-2b channel-aware shift-guard tests to model
/// `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain` cells of the
/// spec §3.4 matrix.
async fn seed_shift_with_state(
    pool: &sqlx::SqlitePool,
    shift_id: ShiftId,
    state: ShiftState,
    opener: &str,
) {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, ?, 'ONLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(state)
    .bind(opener)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a NEW inbox row whose `operation_type` is the canonical
/// `DocType::as_str()` of `doc_type` and whose `payload_sha256_canonical`
/// matches [`cmd`].  Stage 1's command-vs-inbox cross-check enforces both.
async fn seed_inbox_new(pool: &sqlx::SqlitePool, doc_type: DocType) -> [u8; 16] {
    seed_inbox_with_overrides(pool, doc_type.as_str(), [0u8; 32]).await
}

/// Same as `seed_inbox_new` but lets the caller provide a different
/// `operation_type` string and / or a different `payload_sha256_canonical`
/// to drive the command-vs-inbox mismatch fixtures.
async fn seed_inbox_with_overrides(
    pool: &sqlx::SqlitePool,
    operation_type: &str,
    payload_sha256_canonical: [u8; 32],
) -> [u8; 16] {
    let req_id = RequestId::new();
    let req_bytes: [u8; 16] = *req_id.as_bytes();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: operation_type.into(),
            idempotency_key: format!("idem-{}", hex::encode(req_bytes)),
            payload_json: r#"{"goods":[]}"#.into(),
            payload_sha256_canonical,
            correlation_id: None,
        },
    )
    .await
    .unwrap();
    req_bytes
}

fn cmd(doc_type: DocType) -> CanonicalFiscalCommand {
    CanonicalFiscalCommand {
        doc_type,
        business_ts: "2026-04-22T12:00:00Z".into(),
        total_sum_kop: Some(15000),
        payload_json: r#"{"goods":[]}"#.into(),
        payload_sha256_canonical: [0u8; 32],
        // W14a-2b Commit 2: stage_acquire test fixtures don't exercise
        // signer enforcement (Commits 3+5); None baseline matches W14a-1
        // semantics.
        signed_by_cashier_id: None,
        // W4-Z0 piece 9: test fixtures don't exercise listener context.
        driver_id: None,
    }
}

async fn doc_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn next_lnd(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT next_lnd FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn inbox_status(pool: &sqlx::SqlitePool, req_id: &[u8; 16]) -> String {
    let req_slice: &[u8] = req_id;
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(req_slice)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_count_for_event(pool: &sqlx::SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

// hex crate not in deps — local helper.
mod hex {
    pub fn encode(bytes: [u8; 16]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

// ─── 1. SELL happy path with Opened shift ─────────────────────────────

#[tokio::test]
async fn stage1_sell_happy_path_with_opened_shift() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(ctx.document.lnd, 1, "first allocation = 1");
    assert_eq!(ctx.document.backend_profile_id, "b");
    assert_eq!(ctx.document.transport_profile_id, "t");
    assert_eq!(ctx.active_shift.unwrap().shift_id, shift_id);
    assert_eq!(doc_count(&pool).await, 1);
    assert_eq!(next_lnd(&pool).await, 2, "next_lnd advanced 1→2");
    assert_eq!(inbox_status(&pool, &req_id).await, "PROCESSING");
    assert_eq!(audit_count_for_event(&pool, "doc_prepared").await, 1);

    // W4-Z2a piece 6b.1 acceptance — `fiscal_documents.signing_config_snapshot_id`
    // MUST be set at INSERT (NOT NULL, points to inserted snapshot row).
    // Closes external-mid-review CRIT-2 two-envelope crash window.
    let snapshot_id_persisted: Option<i64> = sqlx::query_scalar(
        "SELECT signing_config_snapshot_id FROM fiscal_documents WHERE document_id = ?"
    )
    .bind(ctx.document.document_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        snapshot_id_persisted.is_some(),
        "signing_config_snapshot_id MUST be set at INSERT (piece 6b.1 acceptance)"
    );
    let id = snapshot_id_persisted.unwrap();
    assert_eq!(
        id, ctx.tax_resolution_snapshot_id.expect("ctx FK populated"),
        "persisted FK must match WorkerContext id (same insert_or_get_id_tx result)"
    );
    // Confirm snapshot row exists.
    let snapshot_row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM signing_config_snapshots WHERE id = ?"
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(snapshot_row_count, 1, "FK references existing snapshot row");
}

// ─── 2. SHIFT_OPEN happy path with Closed shift_state ─────────────────

#[tokio::test]
async fn stage1_shift_open_happy_path_with_closed_state() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ShiftOpen).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ShiftOpen))
        .await
        .unwrap();

    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert!(ctx.active_shift.is_none(), "no active shift on SHIFT_OPEN");
    assert_eq!(doc_count(&pool).await, 1);
    assert_eq!(audit_count_for_event(&pool, "doc_prepared").await, 1);
}

// ─── 3. Lease miss → Noop, no state mutation, no audit ───────────────

#[tokio::test]
async fn stage1_lease_miss_returns_noop_no_state_mutation_no_audit() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ShiftOpen).await;
    // Pre-flip the inbox row to simulate "another worker has it".
    let req_slice: &[u8] = &req_id;
    sqlx::query("UPDATE ingress_inbox SET status = 'PROCESSING' WHERE request_id = ?")
        .bind(req_slice)
        .execute(&pool)
        .await
        .unwrap();
    let audit_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ShiftOpen))
        .await
        .unwrap();

    assert!(
        matches!(result, WorkerProcessResult::Noop),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1, "lnd not advanced");
    let audit_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        audit_after, audit_before,
        "lease miss MUST NOT append audit"
    );
}

// ─── 4. Resume detect → existing pending doc, no fresh lnd ────────────

#[tokio::test]
async fn stage1_resume_detect_existing_prepared_doc_skips_lnd_alloc() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;

    // First call: happy path — INSERTs PREPARED at lnd=1, advances to 2.
    let _ = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    let lnd_after_first = next_lnd(&pool).await;
    assert_eq!(lnd_after_first, 2);

    // Reset inbox row to NEW so a second worker pickup is possible.
    let req_slice: &[u8] = &req_id;
    sqlx::query("UPDATE ingress_inbox SET status = 'NEW' WHERE request_id = ?")
        .bind(req_slice)
        .execute(&pool)
        .await
        .unwrap();

    // Second call: get_by_request_id_tx finds existing PREPARED → Resumed.
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    match result {
        WorkerProcessResult::Resumed(ctx) => {
            assert_eq!(ctx.document.lnd, 1, "resumed reuses original lnd");
        }
        other => panic!("expected Resumed, got {other:?}"),
    }
    assert_eq!(
        next_lnd(&pool).await,
        2,
        "next_lnd MUST NOT advance on resume"
    );
    assert_eq!(doc_count(&pool).await, 1, "no fresh INSERT on resume");
    assert_eq!(audit_count_for_event(&pool, "resume_detected").await, 1);
}

// ─── 5. UNIQUE(fn,lnd) collision → fail-closed (rollback) ─────────────

#[tokio::test]
async fn stage1_unique_fn_lnd_collision_fails_closed() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;

    // First call: lnd=1.
    let req1 = seed_inbox_new(&pool, DocType::Sell).await;
    stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req1, cmd(DocType::Sell))
        .await
        .unwrap();
    assert_eq!(next_lnd(&pool).await, 2);

    // Force collision: rewind next_lnd backwards.
    sqlx::query("UPDATE node_state SET next_lnd = 1 WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

    // Second call: allocate_next_lnd will return 1 again →
    // INSERT fiscal_documents collision on ux_fd_fn_lnd → tx rollback.
    let req2 = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req2, cmd(DocType::Sell)).await;
    assert!(result.is_err(), "UNIQUE collision must surface as Err");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(
        msg.contains("UNIQUE") || msg.contains("constraint"),
        "expected UNIQUE constraint violation; got {msg}"
    );

    // Tx rollback: still exactly 1 doc.
    assert_eq!(
        doc_count(&pool).await,
        1,
        "rollback removed in-flight INSERT"
    );
    // Inbox for req2 should NOT be PROCESSING — the lease CAS was inside the
    // same tx that rolled back.
    assert_eq!(
        inbox_status(&pool, &req2).await,
        "NEW",
        "lease CAS rolled back"
    );
}

// ─── 6. Concurrent writers → strictly monotonic lnd ───────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stage1_concurrent_writers_lnd_monotonic() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let req_a = seed_inbox_new(&pool, DocType::Sell).await;
    let req_b = seed_inbox_new(&pool, DocType::Sell).await;

    let p1 = pool.clone();
    let p2 = pool.clone();
    let ps1 = pool_secure.clone();
    let ps2 = pool_secure.clone();
    let t1 = tokio::spawn(async move {
        stage_acquire::run(&p1, &ps1, TEST_DRIVER_ID, req_a, cmd(DocType::Sell)).await
    });
    let t2 = tokio::spawn(async move {
        stage_acquire::run(&p2, &ps2, TEST_DRIVER_ID, req_b, cmd(DocType::Sell)).await
    });
    let r1 = t1.await.unwrap().unwrap();
    let r2 = t2.await.unwrap().unwrap();

    let lnds: Vec<i64> = [r1, r2]
        .iter()
        .map(|r| match r {
            WorkerProcessResult::Proceed(c) => c.document.lnd,
            other => panic!("expected Proceed, got {other:?}"),
        })
        .collect();
    let mut sorted = lnds.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        vec![1, 2],
        "concurrent writers must allocate {{1,2}}, got {lnds:?}"
    );
    assert_eq!(
        next_lnd(&pool).await,
        3,
        "next_lnd advanced 1→3 across two writers"
    );
}

// ─── 7. SELL with Closed shift → reject; no doc, no lnd ───────────────

#[tokio::test]
async fn stage1_sell_with_closed_shift_rejects_inbox_no_doc() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    match result {
        WorkerProcessResult::Rejected {
            reason:
                RejectionReason::ShiftNotOpen {
                    current: ShiftState::Closed,
                },
        } => {}
        other => panic!("expected ShiftNotOpen{{Closed}}, got {other:?}"),
    }
    assert_eq!(doc_count(&pool).await, 0, "no doc on guard reject");
    assert_eq!(next_lnd(&pool).await, 1, "lnd not advanced on guard reject");
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
    assert_eq!(audit_count_for_event(&pool, "guard_rejected").await, 1);
}

// ─── 8. SHIFT_CLOSE with Opened shift → proceed ───────────────────────

#[tokio::test]
async fn stage1_shift_close_with_opened_shift_proceeds() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ShiftClose).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ShiftClose))
        .await
        .unwrap();

    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(ctx.document.doc_type, DocType::ShiftClose);
    assert_eq!(doc_count(&pool).await, 1);
}

// ─── 9. Mode-guard reject (W14a-2b Commit 4: NodeOffline → 4 explicit) ──

/// W14a-2b Commit 4: `NodeOffline` semantic bucket retired.  The four
/// explicit mode-side refusals (`NodeGoingOnlineDrainInFlight` /
/// `NodeBlocked` / `NodeStopMode` / `NodeCryptoDegraded`) are the new
/// surface — `NodeMode::Offline` itself now maps to `Channel::Offline`
/// and proceeds to the shift guard.  This test exercises the
/// `NodeMode::Blocked → NodeBlocked` path; the other three classes
/// share the same dispatch / audit pattern (covered by the per-mode
/// matrix test below).
#[tokio::test]
async fn stage1_node_blocked_rejects_with_audit() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Blocked,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::NodeBlocked
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1);
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
    assert_eq!(
        audit_count_for_event(&pool, "STAGE_ACQUIRE_BLOCKED_REFUSED").await,
        1
    );
}

// MED-C4-1 — 3 additional mode-guard tests for parity with the
// stage1_node_blocked_rejects_with_audit above.  Together with that
// test, covers all 4 mode-side refusal classes from spec §3.3
// (operator-required "плюс отдельные mode guard tests").

#[tokio::test]
async fn stage1_node_going_online_rejects_with_audit() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::GoingOnline,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::NodeGoingOnlineDrainInFlight
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(
        audit_count_for_event(&pool, "STAGE_ACQUIRE_GOING_ONLINE_REFUSED").await,
        1
    );
}

#[tokio::test]
async fn stage1_node_stop_mode_rejects_with_audit() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::StopMode,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::NodeStopMode
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(
        audit_count_for_event(&pool, "STAGE_ACQUIRE_STOP_MODE_REFUSED").await,
        1
    );
}

#[tokio::test]
async fn stage1_node_crypto_degraded_rejects_with_audit() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::CryptoDegraded,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::NodeCryptoDegraded
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(
        audit_count_for_event(&pool, "STAGE_ACQUIRE_CRYPTO_DEGRADED_REFUSED").await,
        1
    );
}

// LOW-C4-1 — minimal channel-aware shift-guard coverage for the 5 new
// refusal variants.  Full 162-cell matrix coverage lands in Commit 7
// per spec §10; these 5 tests pin variant reachability + audit-event
// names per spec §3.6.

/// LOW-C4-1 helper: full seed for shift+node_state in a specific
/// ShiftState (preserves FK order — fn_config → shift → node_state).
async fn seed_for_shift_state_test(
    pool: &sqlx::SqlitePool,
    mode: NodeMode,
    shift_state: ShiftState,
    opener: &str,
) -> ShiftId {
    let shift_id = ShiftId::new();
    seed_fn_config(pool).await;
    seed_shift_with_state(pool, shift_id, shift_state, opener).await;
    seed_node_state(
        pool,
        Some("b"),
        Some("t"),
        mode,
        shift_state,
        Some(shift_id),
    )
    .await;
    shift_id
}

#[tokio::test]
async fn stage1_offline_op_on_online_channel_with_pending_drain_refused() {
    // (Sell, OpenedLocalPendingDrain, Online) → ShiftOpenPendingDrainOpRefused.
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_for_shift_state_test(
        &pool,
        NodeMode::Online,
        ShiftState::OpenedLocalPendingDrain,
        "cashier-vasya",
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftOpenPendingDrainOpRefused
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(
        audit_count_for_event(&pool, "SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED").await,
        1
    );
}

#[tokio::test]
async fn stage1_sale_on_closing_local_pending_drain_refused() {
    // (Sell, ClosingLocalPendingDrain, _) → PostLocalCloseSaleRefused.
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_for_shift_state_test(
        &pool,
        NodeMode::Online,
        ShiftState::ClosingLocalPendingDrain,
        "cashier-vasya",
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::PostLocalCloseSaleRefused
            }
        ),
        "got {result:?}"
    );
    assert_eq!(
        audit_count_for_event(&pool, "POST_LOCAL_CLOSE_SALE_REFUSED").await,
        1
    );
}

#[tokio::test]
async fn stage1_shift_close_on_opened_local_pending_drain_refused() {
    // (ShiftClose, OpenedLocalPendingDrain, _) → OfflineShiftCloseNotSupported.
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_for_shift_state_test(
        &pool,
        NodeMode::Online,
        ShiftState::OpenedLocalPendingDrain,
        "cashier-vasya",
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ShiftClose).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ShiftClose))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::OfflineShiftCloseNotSupported
            }
        ),
        "got {result:?}"
    );
    assert_eq!(
        audit_count_for_event(&pool, "OFFLINE_SHIFT_CLOSE_REFUSED").await,
        1
    );
}

#[tokio::test]
async fn stage1_shift_open_on_closing_local_pending_drain_refused() {
    // NIT-C4-1 fix: (ShiftOpen, ClosingLocalPendingDrain, _) → ShiftClosingInFlight
    // (NOT ShiftAlreadyOpen — pre-fix code mis-labelled this cell).
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_for_shift_state_test(
        &pool,
        NodeMode::Online,
        ShiftState::ClosingLocalPendingDrain,
        "cashier-vasya",
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ShiftOpen).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ShiftOpen))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftClosingInFlight
            }
        ),
        "got {result:?} (must be ShiftClosingInFlight per spec §3.4 matrix, NOT ShiftAlreadyOpen)"
    );
    assert_eq!(
        audit_count_for_event(&pool, "SHIFT_CLOSING_IN_FLIGHT_OP_REFUSED").await,
        1
    );
}

#[tokio::test]
async fn stage1_offline_sell_on_opened_local_pending_drain_carries_shift_id() {
    // HIGH-C4-1 acceptance: (Sell, OpenedLocalPendingDrain, Offline) is
    // allowed by §3.4 matrix, AND active_shift resolver widening (§3.6a)
    // ensures the persisted fiscal_documents.shift_id IS NOT NULL.
    // Without this, the doc would orphan its shift binding and break
    // future W9b drain-time signer enforcement.
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    let shift_id = seed_for_shift_state_test(
        &pool,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
        "cashier-vasya",
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    let active_shift = ctx
        .active_shift
        .expect("HIGH-C4-1 invariant: active_shift must be Some for OpenedLocalPendingDrain offline path");
    assert_eq!(active_shift.shift_id, shift_id);
    assert_eq!(active_shift.state, ShiftState::OpenedLocalPendingDrain);
    assert_eq!(ctx.document.doc_type, DocType::Sell);
    // Verify the persisted ledger row carries shift_id (NOT NULL).
    let persisted_shift_id: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT shift_id FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(ctx.document.document_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        persisted_shift_id.expect("shift_id must be NOT NULL"),
        shift_id.as_bytes().to_vec(),
        "fiscal_documents.shift_id must match active shift",
    );
    // MED-C4-3: persisted fs_mode must reflect the offline channel —
    // NOT the pre-fix hardcoded "ONLINE".
    let persisted_fs_mode: String = sqlx::query_scalar(
        "SELECT fs_mode FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(ctx.document.document_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        persisted_fs_mode, "OFFLINE",
        "fs_mode must be derived from channel; offline path → 'OFFLINE'",
    );
}

#[tokio::test]
async fn stage1_offline_op_on_opened_local_pending_drain_missing_shift_id_is_invariant_breach() {
    // HIGH-C4-1 invariant guard: OpenedLocalPendingDrain + current_shift_id
    // = None must surface as ShiftInvariantViolation, NOT silently
    // proceed with shift_id = None.
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
        None, // invariant breach
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftInvariantViolation
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
}

#[tokio::test]
async fn stage1_z_report_on_opened_local_pending_drain_offline_blocked() {
    // (ZReport, OpenedLocalPendingDrain, Offline) → ZReportBlockedBacklogDrainPending.
    // Pre-W10 guardrail per operator correction #3 (2026-05-19).
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_for_shift_state_test(
        &pool,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
        "cashier-vasya",
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ZReport).await;
    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ZReport))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ZReportBlockedBacklogDrainPending
            }
        ),
        "got {result:?}"
    );
    assert_eq!(
        audit_count_for_event(
            &pool,
            "OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED"
        )
        .await,
        1
    );
}

// ─── 10. Shift invariant violation: Opened but current_shift_id=None ──

#[tokio::test]
async fn stage1_shift_invariant_violation_caught() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        None, // ← invariant breach: shift_state=Opened but no current_shift_id
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftInvariantViolation
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
    assert_eq!(
        audit_count_for_event(&pool, "shift_invariant_violation").await,
        1
    );
}

// ─── 11. Missing profile binding → reject; no doc, no lnd ─────────────

#[tokio::test]
async fn stage1_missing_profile_binding_rejects_inbox_no_doc_no_lnd() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    // backend_profile_id explicitly NULL.
    seed_fn_with_profiles(
        &pool,
        None,
        Some("t"),
        NodeMode::Online,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ShiftOpen).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ShiftOpen))
        .await
        .unwrap();

    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::MissingProfileBinding
            }
        ),
        "got {result:?}"
    );
    assert_eq!(
        doc_count(&pool).await,
        0,
        "no doc on missing-profile reject"
    );
    assert_eq!(
        next_lnd(&pool).await,
        1,
        "lnd not advanced on missing-profile reject"
    );
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
    assert_eq!(
        audit_count_for_event(&pool, "profile_binding_missing").await,
        1
    );
    // Sanity: doc_prepared MUST NOT have been written.
    assert_eq!(audit_count_for_event(&pool, "doc_prepared").await, 0);
}

// ─── 12. command/inbox payload-hash mismatch → reject InvalidPayload ──

#[tokio::test]
async fn stage1_command_inbox_hash_mismatch_rejects_no_doc_no_lnd() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    // inbox carries a non-zero hash; cmd() carries [0u8; 32].
    let req_id = seed_inbox_with_overrides(&pool, DocType::Sell.as_str(), [0xAAu8; 32]).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    match result {
        WorkerProcessResult::Rejected {
            reason: RejectionReason::InvalidPayload { detail },
        } => assert_eq!(detail, "command_payload_hash_mismatch"),
        other => panic!("expected InvalidPayload(hash_mismatch), got {other:?}"),
    }
    assert_eq!(doc_count(&pool).await, 0, "no doc on hash mismatch");
    assert_eq!(next_lnd(&pool).await, 1, "lnd not advanced on mismatch");
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
    assert_eq!(
        audit_count_for_event(&pool, "command_inbox_mismatch").await,
        1
    );
}

// ─── 13. command/inbox doc_type mismatch → reject InvalidPayload ──────

#[tokio::test]
async fn stage1_command_inbox_doc_type_mismatch_rejects_no_doc_no_lnd() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    // inbox.operation_type = "RETURN", cmd.doc_type = SELL — mismatch.
    let req_id = seed_inbox_with_overrides(&pool, "RETURN", [0u8; 32]).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    match result {
        WorkerProcessResult::Rejected {
            reason: RejectionReason::InvalidPayload { detail },
        } => assert_eq!(detail, "command_doc_type_mismatch"),
        other => panic!("expected InvalidPayload(doc_type_mismatch), got {other:?}"),
    }
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1);
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
}

// ─── 14. terminal-doc + NEW inbox for same request_id → reject ────────

#[tokio::test]
async fn stage1_terminal_existing_doc_rejects_not_resumed() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::Sell).await;

    // Plant a TERMINAL fiscal_documents row for the same request_id
    // (e.g. ACK from a previous flow whose inbox row got recycled).
    let req_slice: &[u8] = &req_id;
    let doc_id = prro::db::models::ids::DocumentId::new();
    sqlx::query(
        "INSERT INTO fiscal_documents (
             document_id, request_id, fiscal_number, lnd, doc_type, state,
             backend_profile_id, transport_profile_id, fs_mode, business_ts,
             total_sum_kop, payload_json, payload_sha256_canonical
         ) VALUES (?, ?, ?, 999, 'SELL', 'ACK', 'b', 't', 'ONLINE', ?, ?, ?, ?)",
    )
    .bind(doc_id)
    .bind(req_slice)
    .bind(FN)
    .bind("2026-04-22T11:00:00Z")
    .bind(15000_i64)
    .bind(r#"{"goods":[]}"#)
    .bind(&[0u8; 32][..])
    .execute(&pool)
    .await
    .unwrap();

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    match result {
        WorkerProcessResult::Rejected {
            reason: RejectionReason::InvalidPayload { detail },
        } => assert_eq!(detail, "terminal_document_for_request_id"),
        WorkerProcessResult::Resumed(_) => {
            panic!("terminal doc MUST NOT drive Resumed")
        }
        other => panic!("expected InvalidPayload(terminal_document), got {other:?}"),
    }
    // Pre-existing terminal doc remains; NO fresh PREPARED inserted.
    assert_eq!(
        doc_count(&pool).await,
        1,
        "stage 1 must not INSERT alongside terminal row"
    );
    assert_eq!(
        next_lnd(&pool).await,
        1,
        "lnd not advanced on terminal-coexistence reject"
    );
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
    assert_eq!(
        audit_count_for_event(&pool, "terminal_document_for_request_id").await,
        1
    );
}

// ─── 15. SHIFT_OPEN against ShiftState::Error → ShiftInError ──────────

#[tokio::test]
async fn stage1_shift_open_against_error_state_rejects_as_shift_in_error() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Error,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool, DocType::ShiftOpen).await;

    let result = stage_acquire::run(&pool, &pool_secure, TEST_DRIVER_ID, req_id, cmd(DocType::ShiftOpen))
        .await
        .unwrap();

    // Critical ordering: Error must take precedence over the
    // ShiftOpen catch-all (which would otherwise label this as
    // ShiftAlreadyOpen).
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftInError
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1);
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
}

// fd / shifts unused-import suppression:
#[allow(dead_code)]
fn _unused_imports_suppression() {
    let _ = fd::DocumentRow {
        document_id: prro::db::models::ids::DocumentId::new(),
        fiscal_number: String::new(),
        lnd: 0,
        state: prro::db::models::enums::DocState::Prepared,
        doc_type: DocType::Sell,
        server_fiscal_no: None,
        submission_attempted_at: None,
        backend_profile_id: String::new(),
        transport_profile_id: String::new(),
        // W6 additive fields.
        previous_hash: None,
        z_report_number: None,
        unsigned_xml_sha256: None,
        signing_inputs_pinned_at: None,
        // W14a-2b additive.
        signed_by_cashier_id: None,
    };
    let _ = Severity::Info;
    let _ = shifts::ShiftRow {
        shift_id: ShiftId::new(),
        fiscal_number: String::new(),
        serial: None,
        state: ShiftState::Closed,
        cash_balance_kop: 0,
        // W14a-2b additive — placeholder; sentinel back-fill semantics
        // mirror the W14a-1 production rows.
        opened_by_cashier_id: prro::db::models::ids::CashierId::new(
            "__test_unused_imports__",
        )
        .expect("valid cashier id"),
    };
}
