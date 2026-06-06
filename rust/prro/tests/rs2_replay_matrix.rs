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
    let new = fd::NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::from_bytes(request_id),
        fiscal_number: FN.to_string(),
        shift_id: None,
        offline_session_id: None,
        lnd: 1,
        doc_type,
        backend_profile_id: "b".to_string(),
        transport_profile_id: "t".to_string(),
        fs_mode: "ONLINE",
        business_ts: "2026-06-06T12:00:00Z".to_string(),
        total_sum_kop,
        payload_json: r#"{"items":[],"payments":[]}"#.to_string(),
        payload_sha256_canonical: [0u8; 32],
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
