//! Targeted verification for the W8.2 helpers:
//!   - `node_state::update_last_known_xml_sha_tx` (tx-bound seed advance).
//!   - `ingress_inbox::mark_done_tx` (tx-bound DONE finaliser).
//!
//! Anchored on the W8 design freeze §4.2.  Both helpers are
//! preconditions for W8.3 stage_finalize::run; their contract is
//! "happy update returns true; missing row returns false; caller
//! treats false as typed stage error".  The fixtures pin that surface
//! end-to-end through `with_immediate`, mirroring how W8.3 will
//! invoke them.
//!
//! Nine fixtures:
//!   1. seed_update happy: existing row → true, value persisted.
//!   2. seed_update overwrite: prior value replaced (chain advance).
//!   3. seed_update same-hash retry: idempotent, still returns true
//!      (W8.2 review F4-bis close).
//!   4. seed_update missing FN: false (not silent ignore).
//!   5. seed_update rolls back with enclosing tx (R8 invariant).
//!   6. mark_done happy: PROCESSING → DONE, processed_at set.
//!   7. mark_done missing: false.
//!   8. mark_done rolls back with enclosing tx.
//!   9. mark_done idempotent on repeat: documents no-status-guard
//!      contract (W8.3 CAS short-circuit owns idempotency upstream).

use prro::db::models::enums::{NodeMode, ShiftState};
use prro::db::repositories::{ingress_inbox, node_state};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

async fn seed_fn_config(pool: &SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state(pool: &SqlitePool) {
    seed_fn_config(pool).await;
    node_state::upsert_initial(pool, "1234567890", NodeMode::Online, ShiftState::Closed, 1)
        .await
        .expect("upsert_initial");
}

async fn seed_inbox_row_processing(pool: &SqlitePool, req_byte: u8) -> [u8; 16] {
    seed_fn_config(pool).await;
    let req_id = [req_byte; 16];
    let req_slice: &[u8] = &req_id;
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, '1234567890', 'REST', 'sell', ?, '{}', ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(format!("idem-{req_byte:02x}"))
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed inbox PROCESSING");
    req_id
}

async fn read_node_seed(pool: &SqlitePool) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = '1234567890'",
    )
    .fetch_one(pool)
    .await
    .expect("read seed")
}

async fn read_inbox_status(pool: &SqlitePool, req_id: &[u8; 16]) -> Option<String> {
    let req_slice: &[u8] = req_id;
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(req_slice)
        .fetch_one(pool)
        .await
        .ok()
}

// ─── update_last_known_xml_sha_tx ────────────────────────────────────

#[tokio::test]
async fn seed_update_advances_existing_row_returns_true() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool).await;
    assert!(
        read_node_seed(&pool).await.is_none(),
        "seed must start NULL"
    );

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = node_state::update_last_known_xml_sha_tx(tx, "1234567890", &[0xAB; 32]).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("update_last_known_xml_sha_tx");
    assert!(updated, "existing FN row must report updated=true");

    let v = read_node_seed(&pool).await.expect("seed must be set");
    assert_eq!(v, vec![0xAB; 32], "seed must equal supplied hash");
}

#[tokio::test]
async fn seed_update_overwrites_prior_value_returns_true() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool).await;
    // Pre-populate seed with a different value (simulates a prior Ack).
    node_state::seed_prevhash(&pool, "1234567890", &[0xCD; 32])
        .await
        .unwrap();
    assert_eq!(read_node_seed(&pool).await.unwrap(), vec![0xCD; 32]);

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = node_state::update_last_known_xml_sha_tx(tx, "1234567890", &[0xEF; 32]).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .unwrap();
    assert!(updated);
    assert_eq!(
        read_node_seed(&pool).await.unwrap(),
        vec![0xEF; 32],
        "seed must overwrite prior value (chain advance per ACK)"
    );
}

#[tokio::test]
async fn seed_update_idempotent_under_same_hash_returns_true() {
    // Same-hash retry: SQLite `rows_affected()` reflects matched rows,
    // not just modified rows.  A re-call with the same value still
    // reports `rows_affected == 1` ⇒ helper returns `true`.  Pins the
    // contract: stage_finalize idempotency under crash-then-retry on
    // the same already-Ack'd doc does NOT degrade the seed advance.
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool).await;
    for _ in 0..2 {
        let updated = with_immediate(&pool, move |tx| {
            Box::pin(async move {
                let b =
                    node_state::update_last_known_xml_sha_tx(tx, "1234567890", &[0xAB; 32]).await?;
                Ok::<bool, anyhow::Error>(b)
            })
        })
        .await
        .unwrap();
        assert!(updated, "same-hash retry must still report updated=true");
    }
    assert_eq!(read_node_seed(&pool).await.unwrap(), vec![0xAB; 32]);
}

#[tokio::test]
async fn seed_update_returns_false_for_missing_fn_row() {
    let (_d, pool) = fresh_pool().await;
    // No seed_node_state — node_state row absent.
    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = node_state::update_last_known_xml_sha_tx(tx, "9999999999", &[0xAA; 32]).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("update_last_known_xml_sha_tx");
    assert!(
        !updated,
        "missing FN row must report updated=false (caller treats as stage error)"
    );
}

#[tokio::test]
async fn seed_update_rolls_back_with_enclosing_tx() {
    let (_d, pool) = fresh_pool().await;
    seed_node_state(&pool).await;
    node_state::seed_prevhash(&pool, "1234567890", &[0xBB; 32])
        .await
        .unwrap();

    // Advance seed inside `with_immediate`, then return Err to force
    // rollback.  The seed must revert to its pre-tx value.
    let res: Result<(), anyhow::Error> = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            node_state::update_last_known_xml_sha_tx(tx, "1234567890", &[0xFF; 32]).await?;
            // Simulate a downstream failure (e.g. inbox.mark_done returning false).
            Err(anyhow::anyhow!("simulated downstream failure"))
        })
    })
    .await;
    assert!(res.is_err(), "tx must surface the simulated error");

    assert_eq!(
        read_node_seed(&pool).await.unwrap(),
        vec![0xBB; 32],
        "seed must rollback with the enclosing tx — chain pointer cannot leak past failed Ack"
    );
}

// ─── mark_done_tx ────────────────────────────────────────────────────

#[tokio::test]
async fn mark_done_updates_processing_row_returns_true() {
    let (_d, pool) = fresh_pool().await;
    let req_id = seed_inbox_row_processing(&pool, 0x11).await;
    assert_eq!(
        read_inbox_status(&pool, &req_id).await.as_deref(),
        Some("PROCESSING")
    );

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = ingress_inbox::mark_done_tx(tx, &req_id).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("mark_done_tx");
    assert!(updated, "existing inbox row must report updated=true");
    assert_eq!(
        read_inbox_status(&pool, &req_id).await.as_deref(),
        Some("DONE")
    );

    // processed_at must be set (DDL clock).
    let req_slice: &[u8] = &req_id;
    let processed_at: Option<String> =
        sqlx::query_scalar("SELECT processed_at FROM ingress_inbox WHERE request_id = ?")
            .bind(req_slice)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        processed_at.is_some(),
        "processed_at must be set by CURRENT_TIMESTAMP"
    );
}

#[tokio::test]
async fn mark_done_returns_false_for_missing_row() {
    let (_d, pool) = fresh_pool().await;
    let bogus = [0xCCu8; 16];

    let updated = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let b = ingress_inbox::mark_done_tx(tx, &bogus).await?;
            Ok::<bool, anyhow::Error>(b)
        })
    })
    .await
    .expect("mark_done_tx");
    assert!(
        !updated,
        "missing inbox row must report updated=false (caller treats as stage error)"
    );
}

#[tokio::test]
async fn mark_done_rolls_back_with_enclosing_tx() {
    let (_d, pool) = fresh_pool().await;
    let req_id = seed_inbox_row_processing(&pool, 0x22).await;

    let res: Result<(), anyhow::Error> = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            ingress_inbox::mark_done_tx(tx, &req_id).await?;
            // Simulate downstream failure (e.g. outbox PK violation).
            Err(anyhow::anyhow!("simulated downstream failure"))
        })
    })
    .await;
    assert!(res.is_err());

    // Inbox status must NOT have been advanced.
    assert_eq!(
        read_inbox_status(&pool, &req_id).await.as_deref(),
        Some("PROCESSING"),
        "inbox status must rollback with the enclosing tx"
    );
}

#[tokio::test]
async fn mark_done_idempotent_via_repeat_returns_true_each_time() {
    // Note: unlike outbox INSERT, mark_done_tx is a plain UPDATE
    // without a state-source guard.  A second call after DONE will
    // still UPDATE 1 row (re-setting processed_at).  This is
    // documented behaviour — stage_finalize is responsible for
    // idempotency at the CAS level (Kvt2→Ack short-circuits on rerun
    // BEFORE this helper runs, so in practice mark_done_tx is called
    // exactly once per doc lifecycle).  The test pins this contract.
    let (_d, pool) = fresh_pool().await;
    let req_id = seed_inbox_row_processing(&pool, 0x33).await;

    for _ in 0..2 {
        let updated = with_immediate(&pool, move |tx| {
            Box::pin(async move {
                let b = ingress_inbox::mark_done_tx(tx, &req_id).await?;
                Ok::<bool, anyhow::Error>(b)
            })
        })
        .await
        .unwrap();
        assert!(
            updated,
            "each call against an existing row reports updated=true"
        );
    }
    assert_eq!(
        read_inbox_status(&pool, &req_id).await.as_deref(),
        Some("DONE")
    );
}
