//! W4-Z2a piece 17 (review round 4 R2 H close) — regression
//! coverage for `ingress_inbox::mark_rejected_if_new_tx`, the
//! source-state-guarded reject variant added in piece 16 to close
//! the round-3 R1 High race.
//!
//! Without these tests, a future refactor removing the
//! `AND status = 'NEW'` guard from the SQL would silently regress
//! the race-safety property; the existing piece-16 stage_acquire
//! tests would still pass under the regression because they don't
//! exercise the concurrent-worker scenario.
//!
//! The full thundering-herd concurrent-worker fixture stays
//! deferred (heavy; needs multi-tokio-task coordination).  These
//! state-transition repository tests prove the SQL contract.

use prro::db::open_pool;
use prro::db::repositories::ingress_inbox::{
    self, mark_rejected_if_new_tx, InboxInsertOutcome, NewInboxEntry,
};
use prro::db::tx::with_immediate;
use prro::db::models::enums::Protocol;

const FN: &str = "4000000010";

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mark-rej.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn seed_fn(pool: &sqlx::SqlitePool) {
    use prro::db::repositories::fiscal_number_config::{insert, NewFnConfig};
    insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
            tax_number: "12345678".into(),
            vat_payer_inn: None,
            fiscal_mode: prro::db::models::enums::FiscalMode::Test,
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

async fn seed_inbox_new(pool: &sqlx::SqlitePool, request_id: [u8; 16]) {
    seed_fn(pool).await;
    let entry = NewInboxEntry {
        request_id,
        fiscal_number: FN.to_string(),
        protocol: Protocol::Rest,
        operation_type: "SELL".into(),
        idempotency_key: format!("idem-{}", request_id[0]),
        payload_json: r#"{"items":[]}"#.into(),
        payload_sha256_canonical: [0u8; 32],
        correlation_id: None,
    };
    match ingress_inbox::insert(pool, &entry).await.unwrap() {
        InboxInsertOutcome::Created(_) => {}
        other => panic!("seed must be a fresh Created insert, got {other:?}"),
    }
}

async fn fetch_status(pool: &sqlx::SqlitePool, request_id: &[u8; 16]) -> String {
    let req_slice: &[u8] = request_id;
    sqlx::query_scalar::<_, String>("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(req_slice)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn run_mark(pool: &sqlx::SqlitePool, request_id: [u8; 16]) -> bool {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let was_new = mark_rejected_if_new_tx(tx, &request_id).await?;
            Ok::<_, anyhow::Error>(was_new)
        })
    })
    .await
    .unwrap()
}

// ─── Source-state truth-table ───────────────────────────────────────

/// NEW → REJECTED, returns true.  Happy path: pre-lease reject
/// worker successfully claimed the right to reject.
#[tokio::test]
async fn mark_new_returns_true_and_flips_to_rejected() {
    let pool = fresh_pool().await;
    let request_id = [1u8; 16];
    seed_inbox_new(&pool, request_id).await;
    assert_eq!(fetch_status(&pool, &request_id).await, "NEW");

    let was_new = run_mark(&pool, request_id).await;
    assert!(was_new, "NEW status MUST yield was_new=true");
    assert_eq!(fetch_status(&pool, &request_id).await, "REJECTED");
}

/// PROCESSING → unchanged, returns false.  Race-lost: another
/// worker has the lease.  Critical regression guard — without the
/// `AND status='NEW'` SQL guard, this would silently corrupt the
/// other worker's PROCESSING state.
#[tokio::test]
async fn mark_processing_returns_false_state_untouched() {
    let pool = fresh_pool().await;
    let request_id = [2u8; 16];
    seed_inbox_new(&pool, request_id).await;
    // Simulate another worker taking the lease.
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            ingress_inbox::acquire_lease(tx, &request_id).await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .unwrap();
    assert_eq!(fetch_status(&pool, &request_id).await, "PROCESSING");

    let was_new = run_mark(&pool, request_id).await;
    assert!(
        !was_new,
        "PROCESSING status MUST yield was_new=false (race lost)"
    );
    assert_eq!(
        fetch_status(&pool, &request_id).await,
        "PROCESSING",
        "PROCESSING status MUST be untouched — guard prevents the silent overwrite \
         that motivated piece 16-H1 fix"
    );
}

/// DONE → unchanged, returns false.  Race-completed: another
/// worker finished before this reject path got the small tx.
#[tokio::test]
async fn mark_done_returns_false_state_untouched() {
    let pool = fresh_pool().await;
    let request_id = [3u8; 16];
    seed_inbox_new(&pool, request_id).await;
    // Simulate full completion: PROCESSING then mark_done.
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            ingress_inbox::acquire_lease(tx, &request_id).await?;
            ingress_inbox::mark_done_tx(tx, &request_id).await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .unwrap();
    assert_eq!(fetch_status(&pool, &request_id).await, "DONE");

    let was_new = run_mark(&pool, request_id).await;
    assert!(!was_new, "DONE status MUST yield was_new=false");
    assert_eq!(
        fetch_status(&pool, &request_id).await,
        "DONE",
        "DONE status MUST be untouched — terminal-state guard"
    );
}

/// Missing row (defensive — shouldn't happen but covered) →
/// returns false, no state mutation.
#[tokio::test]
async fn mark_missing_row_returns_false() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let request_id = [4u8; 16];

    let was_new = run_mark(&pool, request_id).await;
    assert!(!was_new, "missing row MUST yield was_new=false");
}

/// Idempotent on re-application: NEW → REJECTED (was_new=true),
/// then second call → REJECTED (was_new=false) since status is no
/// longer NEW.  Proves the guard provides natural idempotency for
/// retry safety.
#[tokio::test]
async fn mark_twice_idempotent() {
    let pool = fresh_pool().await;
    let request_id = [5u8; 16];
    seed_inbox_new(&pool, request_id).await;

    let first = run_mark(&pool, request_id).await;
    assert!(first, "first call on NEW MUST return true");
    assert_eq!(fetch_status(&pool, &request_id).await, "REJECTED");

    let second = run_mark(&pool, request_id).await;
    assert!(
        !second,
        "second call on REJECTED MUST return false — natural idempotency from guard"
    );
    assert_eq!(fetch_status(&pool, &request_id).await, "REJECTED");
}
