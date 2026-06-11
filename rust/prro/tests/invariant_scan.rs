//! Invariant-scan tool acceptance (audit pass-2, item 1).
//!
//! Each fixture seeds ONE deliberate ledger breach and asserts the scan
//! reports exactly that violation class; the clean fixtures pin zero
//! false positives. The scan itself is the reusable post-condition gate
//! for the kill-point matrix / soak / chaos harnesses that follow.

mod common;

use prro::db::invariant_scan::{scan, Violation};
use prro::db::models::enums::{FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, RequestId, ShiftId};
use prro::db::open_pool;
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use sqlx::SqlitePool;

const FN: &str = "4000000001";

async fn fresh_pool() -> SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scan.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn seed_fn(pool: &SqlitePool) -> ShiftId {
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
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'cashier')",
    )
    .bind(shift_id)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_node_state(pool: &SqlitePool, shift_id: ShiftId, seed: Option<[u8; 32]>) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id, last_known_unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 10, 'b', 't', ?)",
    )
    .bind(FN)
    .bind(NodeMode::Online)
    .bind(ShiftState::Opened)
    .bind(shift_id)
    .bind(seed.map(|s| s.to_vec()))
    .execute(pool)
    .await
    .unwrap();
}

/// One configurable fiscal-document row (+ optional KVT1_RAW evidence).
#[allow(clippy::too_many_arguments)]
async fn seed_doc(
    pool: &SqlitePool,
    shift_id: ShiftId,
    lnd: i64,
    state: &str,
    server_fiscal_no: Option<&str>,
    previous_hash: Option<[u8; 32]>,
    unsigned_sha: Option<[u8; 32]>,
    offline_fiscal_no: Option<i64>,
    with_kvt1_raw: bool,
) -> (DocumentId, [u8; 16]) {
    let doc_id = DocumentId::new();
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, shift_id, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, server_fiscal_no, \
            previous_hash, unsigned_xml_sha256, offline_fiscal_no \
         ) VALUES (?, ?, ?, ?, ?, 'SELL', ?, 'b', 't', 'ONLINE', \
            '2026-06-11T00:00:00Z', '{}', ?, ?, ?, ?, ?)",
    )
    .bind(doc_id)
    .bind(&request_id[..])
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(state)
    .bind(vec![0u8; 32])
    .bind(server_fiscal_no)
    .bind(previous_hash.map(|h| h.to_vec()))
    .bind(unsigned_sha.map(|h| h.to_vec()))
    .bind(offline_fiscal_no)
    .execute(pool)
    .await
    .unwrap();
    if with_kvt1_raw {
        sqlx::query(
            "INSERT INTO document_files(document_id, kind, content) VALUES (?, 'KVT1_RAW', ?)",
        )
        .bind(doc_id)
        .bind(vec![0xAAu8; 8])
        .execute(pool)
        .await
        .unwrap();
    }
    (doc_id, request_id)
}

fn h(b: u8) -> [u8; 32] {
    [b; 32]
}

// ─── Clean fixtures: zero false positives ───────────────────────────────

#[tokio::test]
async fn clean_empty_db_scans_clean() {
    let pool = fresh_pool().await;
    assert_eq!(scan(&pool).await.unwrap(), vec![]);
}

/// A consistent two-receipt ledger: genesis ACK (prev=NULL) → second ACK
/// chaining off it; node seed = last ACK's sha; both with DPS id + KVT1_RAW.
#[tokio::test]
async fn clean_consistent_ledger_scans_clean() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(2))).await;
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        2,
        "ACK",
        Some("D-2"),
        Some(h(1)),
        Some(h(2)),
        None,
        true,
    )
    .await;
    assert_eq!(scan(&pool).await.unwrap(), vec![]);
}

// ─── Detection fixtures: one breach each ────────────────────────────────

/// Drift-guard: if a future migration LOSES the `ux_fd_fn_lnd` unique
/// index, the scan still catches double-issued lnd.
#[tokio::test]
async fn detects_duplicate_lnd() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    sqlx::query("DROP INDEX ux_fd_fn_lnd")
        .execute(&pool)
        .await
        .unwrap();
    seed_doc(
        &pool,
        shift,
        7,
        "SENT",
        Some("D-A"),
        None,
        Some(h(1)),
        None,
        false,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        7,
        "SENT",
        Some("D-B"),
        None,
        Some(h(2)),
        None,
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter().any(|x| matches!(
            x,
            Violation::DuplicateLnd { fiscal_number, lnd: 7, count: 2 } if fiscal_number == FN
        )),
        "got: {v:#?}"
    );
}

#[tokio::test]
async fn detects_stuck_sending() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    seed_doc(
        &pool,
        shift,
        1,
        "SENDING",
        None,
        None,
        Some(h(1)),
        None,
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::StuckSending { .. })),
        "got: {v:#?}"
    );
}

#[tokio::test]
async fn detects_ack_without_server_fiscal_no() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await;
    seed_doc(&pool, shift, 1, "ACK", None, None, Some(h(1)), None, true).await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::AckWithoutServerFiscalNo { .. })),
        "got: {v:#?}"
    );
}

#[tokio::test]
async fn detects_ack_without_kvt1_raw() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await;
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::AckWithoutKvt1Raw { .. })),
        "got: {v:#?}"
    );
}

/// Second ACK's previous_hash points at garbage instead of the first
/// ACK's unsigned sha.
#[tokio::test]
async fn detects_chain_break() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(2))).await;
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        2,
        "ACK",
        Some("D-2"),
        Some(h(9)),
        Some(h(2)),
        None,
        true,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::ChainBreak { lnd: 2, .. })),
        "got: {v:#?}"
    );
}

/// Docs chain fine, but node_state's seed was left behind (or ahead).
#[tokio::test]
async fn detects_chain_seed_mismatch() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(9))).await; // wrong seed
    seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::ChainSeedMismatch { .. })),
        "got: {v:#?}"
    );
}

/// REJECTED inbox + accepted doc for the same request — the replay-lie
/// hazard (AUD-1) must be visible to the scan.
#[tokio::test]
async fn detects_rejected_inbox_with_accepted_doc() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, Some(h(1))).await;
    let (_, request_id) = seed_doc(
        &pool,
        shift,
        1,
        "ACK",
        Some("D-1"),
        None,
        Some(h(1)),
        None,
        true,
    )
    .await;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', 'k-1', '{}', ?, 'REJECTED')",
    )
    .bind(&request_id[..])
    .bind(FN)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::RejectedInboxWithAcceptedDoc { .. })),
        "got: {v:#?}"
    );
}

#[tokio::test]
async fn detects_half_consumed_offline_code() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
         VALUES (?, 5, '2026-06-11T00:00:01Z', NULL)",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::OfflineCodeHalfConsumed { code_lnd: 5, .. })),
        "got: {v:#?}"
    );
}

/// A doc claims offline_fiscal_no=5 but no code row was consumed by it.
#[tokio::test]
async fn detects_unbacked_offline_fiscal_no() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    let session = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-06-11T00:00:00Z')",
    )
    .bind(session)
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    seed_doc(
        &pool,
        shift,
        1,
        "OFFLINE_LOCAL_ACK",
        None,
        None,
        Some(h(1)),
        Some(5),
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter()
            .any(|x| matches!(x, Violation::OfflineFiscalNoUnbacked { .. })),
        "got: {v:#?}"
    );
}

/// Two docs share one (fn, offline_fiscal_no) — an offline fiscal number
/// was issued twice (the post-power-cut double-consume scenario DUR-1
/// exists to prevent; the scan is its detector of last resort).
#[tokio::test]
async fn detects_duplicate_offline_fiscal_no() {
    let pool = fresh_pool().await;
    let shift = seed_fn(&pool).await;
    seed_node_state(&pool, shift, None).await;
    seed_doc(
        &pool,
        shift,
        1,
        "OFFLINE_LOCAL_ACK",
        None,
        None,
        Some(h(1)),
        Some(5),
        false,
    )
    .await;
    seed_doc(
        &pool,
        shift,
        2,
        "OFFLINE_LOCAL_ACK",
        None,
        None,
        Some(h(2)),
        Some(5),
        false,
    )
    .await;
    let v = scan(&pool).await.unwrap();
    assert!(
        v.iter().any(|x| matches!(
            x,
            Violation::DuplicateOfflineFiscalNo {
                offline_fiscal_no: 5,
                count: 2,
                ..
            }
        )),
        "got: {v:#?}"
    );
}
