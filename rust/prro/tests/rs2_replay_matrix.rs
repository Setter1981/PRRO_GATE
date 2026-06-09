//! RS-2 piece-4b — integration test for the replay resolver's JOINT
//! `(inbox_status, fiscal_doc_state)` matrix (H3).  Seeds a fiscal
//! document for a `request_id`, drives it to a terminal/in-flight state,
//! and asserts `resolve_replay` produces the right Completed / InProgress
//! / Failed resolution.  Read-only on the resolver side.

use prro::db::models::enums::{DocState, DocType, FiscalMode, Protocol};
use prro::db::models::ids::{DocumentId, RequestId};
use prro::db::open_pool;
use prro::db::repositories::fiscal_documents as fd;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::ingress_inbox::InboxRow;
use prro::runtime::ingress::replay::{resolve_replay, ReplayResolution};
use sqlx::SqlitePool;

const FN: &str = "4000000001";

async fn fresh_main_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open_pool");
    fn_repo::insert(
        &pool,
        &NewFnConfig {
            fiscal_number: FN.to_string(),
            tax_number: "12345678".to_string(),
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
    .expect("seed fn_config");
    (dir, pool)
}

/// Seed a fiscal document for `request_id`, then drive it to `state`
/// (and an optional `server_fiscal_no` for ACK) via a direct UPDATE.
async fn seed_doc(
    pool: &SqlitePool,
    request_id: [u8; 16],
    doc_type: DocType,
    state: DocState,
    server_fiscal_no: Option<&str>,
    total_sum_kop: Option<i64>,
) {
    // lnd=1 by default; multi-doc tests use `seed_doc_lnd` (ux_fd_fn_lnd is
    // UNIQUE on (fiscal_number, lnd)).
    seed_doc_lnd(
        pool,
        request_id,
        1,
        doc_type,
        state,
        server_fiscal_no,
        total_sum_kop,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn seed_doc_lnd(
    pool: &SqlitePool,
    request_id: [u8; 16],
    lnd: i64,
    doc_type: DocType,
    state: DocState,
    server_fiscal_no: Option<&str>,
    total_sum_kop: Option<i64>,
) {
    let new = fd::NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::from_bytes(request_id),
        fiscal_number: FN.to_string(),
        shift_id: None,
        offline_session_id: None,
        lnd,
        doc_type,
        backend_profile_id: "b".to_string(),
        transport_profile_id: "t".to_string(),
        fs_mode: "ONLINE",
        business_ts: "2026-06-06T12:00:00Z".to_string(),
        total_sum_kop,
        payload_json: r#"{"items":[],"payments":[]}"#.to_string(),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    };
    fd::insert_prepared(pool, &new)
        .await
        .expect("insert_prepared");
    sqlx::query("UPDATE fiscal_documents SET state = ?, server_fiscal_no = ? WHERE request_id = ?")
        .bind(state)
        .bind(server_fiscal_no)
        .bind(&request_id[..])
        .execute(pool)
        .await
        .expect("drive state");
}

fn inbox(request_id: [u8; 16], status: &str) -> InboxRow {
    InboxRow {
        request_id,
        fiscal_number: FN.to_string(),
        protocol: Protocol::Rest,
        operation_type: "SELL".to_string(),
        idempotency_key: "k".to_string(),
        status: status.to_string(),
        payload_json: "{}".to_string(),
        payload_sha256_canonical: [0u8; 32],
        correlation_id: None,
        received_at: "2026-06-06T12:00:00Z".to_string(),
        signed_by_cashier_id: None,
        driver_id: Some("drv-test".to_string()),
        business_ts: None,
        total_sum_kop: None,
    }
}

/// Acceptance #3 — PROCESSING + OFFLINE_LOCAL_ACK is COMPLETED (a
/// client-terminal accepted state), NOT in-progress, with `fiscal_id`
/// null (no DPS id yet).
#[tokio::test]
async fn processing_plus_offline_local_ack_is_completed() {
    let (_d, pool) = fresh_main_pool().await;
    let rid = [1u8; 16];
    seed_doc(
        &pool,
        rid,
        DocType::Sell,
        DocState::OfflineLocalAck,
        None,
        Some(15000),
    )
    .await;

    let res = resolve_replay(&inbox(rid, "PROCESSING"), &pool)
        .await
        .unwrap();
    match res {
        ReplayResolution::Completed(r) => {
            assert!(r.ok);
            assert_eq!(r.document_state, "OFFLINE_LOCAL_ACK");
            assert_eq!(r.fiscal_id, None, "offline-local-ack has no DPS fiscal id");
            assert_eq!(r.sale_total_kopecks, 15000);
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

/// DONE backed by a terminal-accepted ACK → Completed with fiscal_id.
#[tokio::test]
async fn done_plus_ack_is_completed_with_fiscal_id() {
    let (_d, pool) = fresh_main_pool().await;
    let rid = [2u8; 16];
    seed_doc(
        &pool,
        rid,
        DocType::Sell,
        DocState::Ack,
        Some("777001"),
        Some(15000),
    )
    .await;

    let res = resolve_replay(&inbox(rid, "DONE"), &pool).await.unwrap();
    match res {
        ReplayResolution::Completed(r) => {
            assert_eq!(r.fiscal_id.as_deref(), Some("777001"));
            assert_eq!(r.document_state, "ACK");
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

/// Acceptance #4 — DONE without a terminal-accepted fiscal doc is a typed
/// drift error (do not trust the inbox alone).
#[tokio::test]
async fn done_without_fiscal_doc_is_drift_error() {
    let (_d, pool) = fresh_main_pool().await;
    // No fiscal doc seeded for this request_id.
    let res = resolve_replay(&inbox([3u8; 16], "DONE"), &pool)
        .await
        .unwrap();
    match res {
        ReplayResolution::Failed(e) => assert_eq!(e.error_code, "INBOX_LEDGER_DRIFT"),
        other => panic!("expected Failed drift, got {other:?}"),
    }
}

/// DONE but the fiscal doc is non-accepted (in-flight) → drift error.
#[tokio::test]
async fn done_with_non_accepted_doc_is_drift_error() {
    let (_d, pool) = fresh_main_pool().await;
    let rid = [4u8; 16];
    seed_doc(
        &pool,
        rid,
        DocType::Sell,
        DocState::Sending,
        None,
        Some(15000),
    )
    .await;
    let res = resolve_replay(&inbox(rid, "DONE"), &pool).await.unwrap();
    match res {
        ReplayResolution::Failed(e) => assert_eq!(e.error_code, "INBOX_LEDGER_DRIFT"),
        other => panic!("expected Failed drift, got {other:?}"),
    }
}

/// Acceptance #5 — NEW/PROCESSING with no fiscal doc → deterministic
/// InProgress (NOT a fake success).
#[tokio::test]
async fn processing_without_fiscal_doc_is_in_progress() {
    let (_d, pool) = fresh_main_pool().await;
    let res = resolve_replay(&inbox([5u8; 16], "PROCESSING"), &pool)
        .await
        .unwrap();
    match res {
        ReplayResolution::InProgress(e) => assert_eq!(e.error_code, "IN_PROGRESS"),
        other => panic!("expected InProgress, got {other:?}"),
    }
}

/// PROCESSING + in-flight fiscal doc (Sending) → InProgress.
#[tokio::test]
async fn processing_with_in_flight_doc_is_in_progress() {
    let (_d, pool) = fresh_main_pool().await;
    let rid = [6u8; 16];
    seed_doc(
        &pool,
        rid,
        DocType::Sell,
        DocState::Sending,
        None,
        Some(15000),
    )
    .await;
    let res = resolve_replay(&inbox(rid, "PROCESSING"), &pool)
        .await
        .unwrap();
    assert!(
        matches!(res, ReplayResolution::InProgress(_)),
        "got {res:?}"
    );
}

/// PROCESSING + terminally-failed fiscal doc (Rejected) → Failed.
#[tokio::test]
async fn processing_with_rejected_doc_is_failed() {
    let (_d, pool) = fresh_main_pool().await;
    let rid = [7u8; 16];
    seed_doc(
        &pool,
        rid,
        DocType::Sell,
        DocState::Rejected,
        None,
        Some(15000),
    )
    .await;
    let res = resolve_replay(&inbox(rid, "PROCESSING"), &pool)
        .await
        .unwrap();
    match res {
        ReplayResolution::Failed(e) => assert_eq!(e.error_code, "FISCAL_REJECTED"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// NEW status (vs PROCESSING) + accepted fiscal doc → Completed (locks
/// that NEW and PROCESSING share the lenient branch).
#[tokio::test]
async fn new_plus_ack_is_completed() {
    let (_d, pool) = fresh_main_pool().await;
    let rid = [8u8; 16];
    seed_doc(
        &pool,
        rid,
        DocType::Sell,
        DocState::Ack,
        Some("888002"),
        Some(15000),
    )
    .await;
    let res = resolve_replay(&inbox(rid, "NEW"), &pool).await.unwrap();
    match res {
        ReplayResolution::Completed(r) => assert_eq!(r.fiscal_id.as_deref(), Some("888002")),
        other => panic!("expected Completed, got {other:?}"),
    }
}

/// PROCESSING + Cancelled fiscal doc (another terminally-failed state) →
/// Failed.
#[tokio::test]
async fn processing_with_cancelled_doc_is_failed() {
    let (_d, pool) = fresh_main_pool().await;
    let rid = [9u8; 16];
    seed_doc(
        &pool,
        rid,
        DocType::Sell,
        DocState::Cancelled,
        None,
        Some(15000),
    )
    .await;
    let res = resolve_replay(&inbox(rid, "PROCESSING"), &pool)
        .await
        .unwrap();
    match res {
        ReplayResolution::Failed(e) => assert_eq!(e.error_code, "FISCAL_REJECTED"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// piece-6 — `last_server_fiscal_no` returns ONLY a real online `ACK` with a
/// non-null `server_fiscal_no`.  An `OFFLINE_LOCAL_ACK` (no DPS number) is NOT
/// a candidate even at a higher lnd; the most recent qualifying ACK wins.
#[tokio::test]
async fn last_server_fiscal_no_only_real_acks() {
    let (_d, pool) = fresh_main_pool().await;
    // No acked docs yet → None.
    assert_eq!(fd::last_server_fiscal_no(&pool, FN).await.unwrap(), None);

    // An online ACK with a DPS fiscal number.
    seed_doc_lnd(
        &pool,
        [21u8; 16],
        1,
        DocType::Sell,
        DocState::Ack,
        Some("777001"),
        Some(15000),
    )
    .await;
    // An OFFLINE_LOCAL_ACK (no DPS number) at a HIGHER lnd — NOT a candidate.
    seed_doc_lnd(
        &pool,
        [22u8; 16],
        2,
        DocType::Sell,
        DocState::OfflineLocalAck,
        None,
        Some(9000),
    )
    .await;
    assert_eq!(
        fd::last_server_fiscal_no(&pool, FN)
            .await
            .unwrap()
            .as_deref(),
        Some("777001"),
        "offline-local-ack must not shadow a real ACK"
    );

    // A newer online ACK wins (most recent).
    seed_doc_lnd(
        &pool,
        [23u8; 16],
        3,
        DocType::Sell,
        DocState::Ack,
        Some("888002"),
        Some(20000),
    )
    .await;
    assert_eq!(
        fd::last_server_fiscal_no(&pool, FN)
            .await
            .unwrap()
            .as_deref(),
        Some("888002")
    );
}

/// review-r1 HIGH regression: ranking is by `lnd`, NOT `first_kvt1_at`.  A
/// NEWER ACK with `first_kvt1_at = NULL` (a pre-014 legacy terminal row that
/// migration 014 never backfilled) must STILL win over an OLDER ACK with a
/// non-null stamp — `first_kvt1_at DESC` would have surfaced the stale OLDER
/// number (SQLite sorts NULL last under DESC).
#[tokio::test]
async fn last_server_fiscal_no_ranks_by_lnd_not_first_kvt1_at() {
    let (_d, pool) = fresh_main_pool().await;
    // Older ACK (lnd=10) WITH a non-null first_kvt1_at.
    seed_doc_lnd(
        &pool,
        [31u8; 16],
        10,
        DocType::Sell,
        DocState::Ack,
        Some("OLD"),
        Some(1000),
    )
    .await;
    sqlx::query("UPDATE fiscal_documents SET first_kvt1_at = ? WHERE request_id = ?")
        .bind("2026-01-01T00:00:00Z")
        .bind(&[31u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    // Newer ACK (lnd=20) with NULL first_kvt1_at (legacy pre-014 terminal row).
    seed_doc_lnd(
        &pool,
        [32u8; 16],
        20,
        DocType::Sell,
        DocState::Ack,
        Some("NEW"),
        Some(2000),
    )
    .await;
    assert_eq!(
        fd::last_server_fiscal_no(&pool, FN)
            .await
            .unwrap()
            .as_deref(),
        Some("NEW"),
        "lnd ranks recency; a newer NULL-first_kvt1_at ACK must not be shadowed by an older stamped one"
    );
}

/// review-r2 HIGH-b: an ACK with an EMPTY `server_fiscal_no` is corrupt (NOT a
/// real DPS number — `replay::build_accepted` treats it as drift), so it is
/// excluded even at a higher lnd; the last NON-empty ACK wins.
#[tokio::test]
async fn last_server_fiscal_no_excludes_empty_string() {
    let (_d, pool) = fresh_main_pool().await;
    // A real ACK at lnd=1.
    seed_doc_lnd(
        &pool,
        [41u8; 16],
        1,
        DocType::Sell,
        DocState::Ack,
        Some("REAL"),
        Some(1000),
    )
    .await;
    // A NEWER ACK (lnd=2) with an EMPTY server_fiscal_no — must be excluded.
    seed_doc_lnd(
        &pool,
        [42u8; 16],
        2,
        DocType::Sell,
        DocState::Ack,
        Some(""),
        Some(2000),
    )
    .await;
    assert_eq!(
        fd::last_server_fiscal_no(&pool, FN)
            .await
            .unwrap()
            .as_deref(),
        Some("REAL"),
        "an empty-string ACK server_fiscal_no must not be surfaced as a real number"
    );
}
