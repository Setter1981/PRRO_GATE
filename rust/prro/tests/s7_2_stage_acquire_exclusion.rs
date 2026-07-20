//! S7-2 — `stage_acquire` fence-EXCLUSION equivalence (behavioural teeth).
//!
//! The S7-2 FN active-reservation fence deliberately EXCLUDES `stage_acquire`
//! (`stage_acquire.rs:714`) from the wired writers (see the static-pin doc in
//! `fn_fence_active.rs`): the existing online write-gate
//! `exists_blocking_non_issued_sibling_tx` already refuses minting a fresh doc while a
//! non-issued sibling rests on the FN, and a doc under an active delivery reservation is
//! ALWAYS in `SENDING` (non-issued) for the whole active window — so the
//! reservation-active window ⊂ "non-issued sibling present". Offline mints are
//! unconditionally ungated by design (offline availability is mandatory).
//!
//! These two tests are the EMPIRICAL proof of that exclusion (revert the exclusion
//! justification / write-gate → RED):
//!   * online — a `SENDING` sibling makes the write-gate REJECT the fresh mint;
//!   * offline-origin — the write-gate is online-only, so an offline mint passes it
//!     (its chain-fork guard is the separate `stage_sign` fence, S7-2, wired).
//!
//! They live in their OWN file (NOT `write_path_stage1_acquire.rs`, which is a
//! CS-1-frozen provenance-tracked file — `CS1_MODIFIED_FILES`): adding new tests there
//! trips the `cs1_live_drift_base_vs_worktree` behaviour-neutrality gate. The
//! `stage_acquire` setup helpers below are duplicated verbatim from that file (test-only
//! duplication is cheaper than a CS-1 carve-out that would permanently drop the AST
//! behaviour-neutrality check for the frozen file).

use prro::db::models::enums::{DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{RequestId, ShiftId};
use prro::db::repositories::{
    fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig, ingress_inbox as inbox,
    ingress_inbox::NewInboxEntry,
};
use prro::db::types::DbShiftId;
use prro::db::{open_pool, open_secure_pool};
use prro::services::write_path::{
    stage_acquire,
    types::{CanonicalFiscalCommand, RejectionReason, WorkerProcessResult},
};

const FN: &str = "4000000001";
const TEST_DRIVER_ID: &str = "test-driver";

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s7_2_acquire.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn fresh_secure_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s7_2_acquire-secure.db");
    std::mem::forget(dir);
    open_secure_pool(&path).await.unwrap()
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
    .bind(mode.as_str())
    .bind(shift_state.as_str())
    .bind(current_shift_id.map(DbShiftId))
    .bind(backend)
    .bind(transport)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_shift(pool: &sqlx::SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(DbShiftId(shift_id))
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_inbox_new(pool: &sqlx::SqlitePool, doc_type: DocType) -> [u8; 16] {
    seed_inbox_with_overrides(pool, doc_type.as_str(), [0u8; 32]).await
}

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
            idempotency_key: format!(
                "idem-{}",
                req_bytes
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
            payload_json: r#"{"goods":[]}"#.into(),
            payload_sha256_canonical,
            correlation_id: None,
            signed_by_cashier_id: None,
            driver_id: Some("drv-test".into()),
            business_ts: None,
            total_sum_kop: None,
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
        source_sha256: [0u8; 32],
        signed_by_cashier_id: None,
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

/// Seed a `SENDING` sibling doc on FN — exactly the doc a CALL_STARTED reservation
/// points to.
async fn seed_sending_sibling(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 1, 'SELL', 'SENDING', 'b', 't', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&[0x77u8; 16][..])
    .bind(&[0x88u8; 16][..])
    .bind(FN)
    .bind(&[0u8; 32][..])
    .execute(pool)
    .await
    .expect("seed SENDING sibling");
}

#[tokio::test]
async fn s7_2_online_write_gate_covers_reservation_sending_sibling() {
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
    seed_sending_sibling(&pool).await;
    let before = doc_count(&pool).await;
    let before_lnd = next_lnd(&pool).await;

    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(
        &pool,
        &pool_secure,
        TEST_DRIVER_ID,
        req_id,
        cmd(DocType::Sell),
    )
    .await
    .unwrap();

    match result {
        WorkerProcessResult::Rejected { reason, .. } => assert_eq!(
            reason,
            RejectionReason::WriteGateSiblingPending,
            "an online mint MUST be gated by the SENDING sibling (= the reservation's doc)"
        ),
        other => panic!(
            "expected Rejected{{WriteGateSiblingPending}} with a SENDING sibling, got {other:?}"
        ),
    }
    assert_eq!(
        doc_count(&pool).await,
        before,
        "no fresh doc minted — the write-gate covers the reservation window"
    );
    assert_eq!(next_lnd(&pool).await, before_lnd, "no lnd advance");
}

#[tokio::test]
async fn s7_2_offline_origin_passes_online_write_gate_covered_by_stage_sign_fence() {
    let pool = fresh_pool().await;
    let pool_secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(
        &pool,
        Some("b"),
        Some("t"),
        NodeMode::Offline,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    seed_sending_sibling(&pool).await;
    // Bump next_lnd past the sibling so an offline mint that PROCEEDS does not collide
    // on (fiscal_number, lnd) — keeps the assertion about the write-gate, not a UNIQUE.
    sqlx::query("UPDATE node_state SET next_lnd = 10 WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

    let req_id = seed_inbox_new(&pool, DocType::Sell).await;
    let result = stage_acquire::run(
        &pool,
        &pool_secure,
        TEST_DRIVER_ID,
        req_id,
        cmd(DocType::Sell),
    )
    .await;

    // The sibling write-gate is ONLINE-only → it must NOT reject an offline-origin mint.
    // Offline's chain-fork guard is the separate `stage_sign` fence (S7-2).
    let rejected_by_write_gate = matches!(
        result,
        Ok(WorkerProcessResult::Rejected {
            reason: RejectionReason::WriteGateSiblingPending,
            ..
        })
    );
    assert!(
        !rejected_by_write_gate,
        "offline-origin mint must NOT be gated by the online sibling write-gate (online-only); got {result:?}"
    );
}
