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
use prro::db::open_pool;
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
/// SHIFT_OPEN happy path / NodeOffline / MissingProfileBinding).
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
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, cash_balance_kop) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0)",
    )
    .bind(shift_id)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_inbox_new(pool: &sqlx::SqlitePool) -> [u8; 16] {
    let req_id = RequestId::new();
    let req_bytes: [u8; 16] = *req_id.as_bytes();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: "sell".into(),
            idempotency_key: format!("idem-{}", hex::encode(req_bytes)),
            payload_json: r#"{"goods":[]}"#.into(),
            payload_sha256_canonical: [0u8; 32],
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
    let req_id = seed_inbox_new(&pool).await;

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::Sell))
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
}

// ─── 2. SHIFT_OPEN happy path with Closed shift_state ─────────────────

#[tokio::test]
async fn stage1_shift_open_happy_path_with_closed_state() {
    let pool = fresh_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool).await;

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::ShiftOpen))
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
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool).await;
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

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::ShiftOpen))
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
    let req_id = seed_inbox_new(&pool).await;

    // First call: happy path — INSERTs PREPARED at lnd=1, advances to 2.
    let _ = stage_acquire::run(&pool, req_id, cmd(DocType::Sell))
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
    let result = stage_acquire::run(&pool, req_id, cmd(DocType::Sell))
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
    let req1 = seed_inbox_new(&pool).await;
    stage_acquire::run(&pool, req1, cmd(DocType::Sell))
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
    let req2 = seed_inbox_new(&pool).await;
    let result = stage_acquire::run(&pool, req2, cmd(DocType::Sell)).await;
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
    let req_a = seed_inbox_new(&pool).await;
    let req_b = seed_inbox_new(&pool).await;

    let p1 = pool.clone();
    let p2 = pool.clone();
    let t1 = tokio::spawn(async move { stage_acquire::run(&p1, req_a, cmd(DocType::Sell)).await });
    let t2 = tokio::spawn(async move { stage_acquire::run(&p2, req_b, cmd(DocType::Sell)).await });
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
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool).await;

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::Sell))
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
    let req_id = seed_inbox_new(&pool).await;

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::ShiftClose))
        .await
        .unwrap();

    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(ctx.document.doc_type, DocType::ShiftClose);
    assert_eq!(doc_count(&pool).await, 1);
}

// ─── 9. NodeMode != Online → reject with audit ───────────────────────

#[tokio::test]
async fn stage1_node_offline_rejects_with_audit() {
    let pool = fresh_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Offline,
        ShiftState::Closed,
        None,
    )
    .await;
    let req_id = seed_inbox_new(&pool).await;

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::Sell))
        .await
        .unwrap();

    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::NodeOffline
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1);
    assert_eq!(inbox_status(&pool, &req_id).await, "REJECTED");
    assert_eq!(audit_count_for_event(&pool, "node_offline_reject").await, 1);
}

// ─── 10. Shift invariant violation: Opened but current_shift_id=None ──

#[tokio::test]
async fn stage1_shift_invariant_violation_caught() {
    let pool = fresh_pool().await;
    seed_fn_with_profiles(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Online,
        ShiftState::Opened,
        None, // ← invariant breach: shift_state=Opened but no current_shift_id
    )
    .await;
    let req_id = seed_inbox_new(&pool).await;

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::Sell))
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
    let req_id = seed_inbox_new(&pool).await;

    let result = stage_acquire::run(&pool, req_id, cmd(DocType::ShiftOpen))
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
    };
    let _ = Severity::Info;
    let _ = shifts::ShiftRow {
        shift_id: ShiftId::new(),
        fiscal_number: String::new(),
        serial: None,
        state: ShiftState::Closed,
        cash_balance_kop: 0,
    };
}
