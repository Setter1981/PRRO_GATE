//! W5 acceptance — offline code-pool (seed + acquire).
//!
//! Covers W5 review axis 3 (atomic code acquisition):
//!
//!   - `seed_code_range` idempotency + count semantics.
//!   - `acquire_code_tx` single-statement CAS via `consumed_at IS
//!     NULL` (no SELECT-then-UPDATE race).
//!   - `CodePoolExhausted` typed error on empty pool.
//!   - `OfflineCodeAlreadyConsumed` typed error mapping for
//!     `ux_offline_codes_consumed_by_doc` partial UNIQUE
//!     (defence-in-depth — engineered via raw-SQL injection of a
//!     consumed row pre-linked to a doc, then acquire_code_tx for
//!     the same doc tries to link another code → UNIQUE fires).
//!   - Concurrent acquire returns DISTINCT codes (two `tokio::task`s
//!     on separate pool connections — per W5 plan §Task 5 line 462).

use prro::db::models::ids::{DocumentId, OfflineSessionId};
use prro::db::repositories::offline_sessions::{self, OfflineSessionError};
use prro::db::tx::with_immediate;
use prro::db::types::{DbDocumentId, DbOfflineSessionId};
use std::sync::Arc;

const FN: &str = "1234567890";

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    seed_fn(&pool, FN).await;
    (dir, pool)
}

async fn seed_fn(pool: &sqlx::SqlitePool, fiscal_number: &str) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fiscal_number)
    .execute(pool)
    .await
    .unwrap();
}

/// INSERT a minimal `fiscal_documents` row that the W4 FK on
/// `offline_codes.consumed_by_document_id` can satisfy.  Returns
/// the generated `DocumentId`.  The row has NULL
/// `offline_session_id` (we're testing the code pool in isolation;
/// session linkage is exercised in the state-machine test file).
async fn insert_fiscal_doc(pool: &sqlx::SqlitePool, fiscal_number: &str, lnd: i64) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = uuid::Uuid::now_v7();
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
            '2026-05-15T00:00:00Z', '{}', ?)",
    )
    .bind(DbDocumentId(doc_id))
    .bind(req_id.as_bytes().to_vec())
    .bind(fiscal_number)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

async fn acquire_via_envelope(
    pool: &sqlx::SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
) -> Result<offline_sessions::AcquiredCode, anyhow::Error> {
    let fn_owned = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            offline_sessions::acquire_code_tx(tx, &fn_owned, doc_id)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

/// `seed_code_range_tx` is tx-bound (PR #54 Round 1 HIGH fix);
/// tests drive it through the same `with_immediate` envelope
/// production does.  The corresponding service-level ergonomic
/// wrapper (`OfflineSessionService::seed_code_range`) is exercised
/// in `seed_via_service_matches_tx_helper` below.
async fn seed_via_envelope(
    pool: &sqlx::SqlitePool,
    fiscal_number: &str,
    first_lnd: i64,
    last_lnd: i64,
) -> u64 {
    let fn_owned = fiscal_number.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            offline_sessions::seed_code_range_tx(tx, &fn_owned, first_lnd, last_lnd)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap()
}

// ─── seed_code_range ────────────────────────────────────────────────

#[tokio::test]
async fn seed_code_range_inserts_full_range_and_returns_count() {
    let (_d, pool) = fresh_pool().await;
    let inserted = seed_via_envelope(&pool, FN, 1, 5).await;
    assert_eq!(inserted, 5, "all 5 codes must be inserted");
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 5);
    // Every code is unconsumed.
    let unused: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes \
         WHERE fiscal_number = ? AND consumed_at IS NULL",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unused, 5);
}

#[tokio::test]
async fn seed_code_range_is_idempotent_on_overlap() {
    let (_d, pool) = fresh_pool().await;
    let first = seed_via_envelope(&pool, FN, 1, 5).await;
    assert_eq!(first, 5);
    // Re-seed overlapping range [3..=7] — codes 3,4,5 already exist;
    // 6,7 are new.  Returned count must be 2.
    let second = seed_via_envelope(&pool, FN, 3, 7).await;
    assert_eq!(second, 2, "INSERT OR IGNORE must skip existing");
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 7);
}

#[tokio::test]
async fn seed_code_range_empty_range_is_noop() {
    let (_d, pool) = fresh_pool().await;
    let inserted = seed_via_envelope(&pool, FN, 10, 5).await;
    assert_eq!(inserted, 0);
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 0);
}

// ─── B8: helper to seed DPS-issued codes (dps_code NOT NULL) ────────
// acquire_code_tx (B8-1) only picks codes where dps_code IS NOT NULL,
// so acquire tests must seed via raw INSERT with dps_code set.

async fn seed_dps_codes_raw(pool: &sqlx::SqlitePool, fiscal_number: &str, codes: &[(&str, i64)]) {
    for (dps_code, code_lnd) in codes {
        sqlx::query(
            "INSERT INTO offline_codes(fiscal_number, code_lnd, dps_code) VALUES (?, ?, ?)",
        )
        .bind(fiscal_number)
        .bind(*code_lnd)
        .bind(*dps_code)
        .execute(pool)
        .await
        .unwrap();
    }
}

// ─── acquire_code_tx ────────────────────────────────────────────────

#[tokio::test]
async fn acquire_code_picks_lowest_available_and_stamps_consumed() {
    let (_d, pool) = fresh_pool().await;
    // B8-1: seed with dps_code so acquire_code_tx can pick them.
    seed_dps_codes_raw(
        &pool,
        FN,
        &[("DPS-100", 100), ("DPS-101", 101), ("DPS-102", 102)],
    )
    .await;
    let doc = insert_fiscal_doc(&pool, FN, 1).await;
    let acquired = acquire_via_envelope(&pool, FN, doc).await.unwrap();
    assert_eq!(acquired.code_lnd, 100, "must pick lowest available code");
    assert_eq!(acquired.dps_code, "DPS-100", "dps_code must be returned");
    assert!(
        !acquired.consumed_at.is_empty(),
        "consumed_at must be stamped"
    );
    // Row in DB reflects consumption.
    let consumed_by: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT consumed_by_document_id FROM offline_codes \
         WHERE fiscal_number = ? AND code_lnd = 100",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(consumed_by.as_deref(), Some(&doc.as_bytes()[..]));
}

#[tokio::test]
async fn acquire_code_returns_code_pool_exhausted_on_empty_pool() {
    let (_d, pool) = fresh_pool().await;
    // No seed — pool is empty.
    let doc = insert_fiscal_doc(&pool, FN, 1).await;
    let err = acquire_via_envelope(&pool, FN, doc).await.unwrap_err();
    let typed = err.downcast_ref::<OfflineSessionError>();
    match typed {
        Some(OfflineSessionError::CodePoolExhausted { fiscal_number }) => {
            assert_eq!(fiscal_number, FN);
        }
        other => panic!(
            "expected typed CodePoolExhausted on empty pool; got: {other:?} \
             (full err: {err:?})"
        ),
    }
}

#[tokio::test]
async fn acquire_code_returns_code_pool_exhausted_after_full_drain() {
    let (_d, pool) = fresh_pool().await;
    // B8-1: seed with dps_code.
    seed_dps_codes_raw(&pool, FN, &[("DPS-1", 1), ("DPS-2", 2)]).await;
    let doc1 = insert_fiscal_doc(&pool, FN, 1).await;
    let doc2 = insert_fiscal_doc(&pool, FN, 2).await;
    let _ = acquire_via_envelope(&pool, FN, doc1).await.unwrap();
    let _ = acquire_via_envelope(&pool, FN, doc2).await.unwrap();
    // Pool now drained.
    let doc3 = insert_fiscal_doc(&pool, FN, 3).await;
    let err = acquire_via_envelope(&pool, FN, doc3).await.unwrap_err();
    let typed = err.downcast_ref::<OfflineSessionError>();
    assert!(
        matches!(typed, Some(OfflineSessionError::CodePoolExhausted { .. })),
        "expected CodePoolExhausted after full drain; got: {err:?}"
    );
}

// ─── W4-compat mapping: ux_offline_codes_consumed_by_doc → OfflineCodeAlreadyConsumed ─

#[tokio::test]
async fn acquire_code_classifies_partial_unique_violation_as_offline_code_already_consumed() {
    let (_d, pool) = fresh_pool().await;
    // B8-1: seed with dps_code so acquire can pick them.
    seed_dps_codes_raw(&pool, FN, &[("DPS-1", 1), ("DPS-2", 2)]).await;
    let doc_x = insert_fiscal_doc(&pool, FN, 1).await;

    // Engineer the W4-compat violation path: manually link code 1
    // to doc_x via raw SQL.  This bypasses the `acquire_code_tx`
    // helper.  The raw UPDATE does NOT fire the W4
    // `offline_codes_consumed_immutable` trigger because the trigger
    // guards `WHEN OLD.consumed_at IS NOT NULL` — and OLD.consumed_at
    // is NULL pre-UPDATE.  Result: code 1 is consumed and linked to
    // doc_x.
    sqlx::query(
        "UPDATE offline_codes \
         SET consumed_at = CURRENT_TIMESTAMP, consumed_by_document_id = ? \
         WHERE fiscal_number = ? AND code_lnd = 1",
    )
    .bind(DbDocumentId(doc_x))
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();

    // Now call acquire_code_tx with the SAME doc_x — it will pick
    // code 2 (the only remaining unconsumed) and try to link it to
    // doc_x.  The partial UNIQUE ux_offline_codes_consumed_by_doc
    // fires (one doc already has code 1 linked).
    let err = acquire_via_envelope(&pool, FN, doc_x).await.unwrap_err();
    let typed = err.downcast_ref::<OfflineSessionError>();
    assert!(
        matches!(typed, Some(OfflineSessionError::OfflineCodeAlreadyConsumed)),
        "expected typed OfflineCodeAlreadyConsumed from ux_offline_codes_consumed_by_doc UNIQUE; \
         got: {err:?}"
    );
}

// ─── Concurrent acquire returns distinct codes ───────────────────────

#[tokio::test]
async fn concurrent_acquire_on_same_fn_returns_distinct_codes() {
    // Plan §W5 line 462 test shape: two `tokio::task`s on separate
    // pool connections, each calling acquire_code_tx; assert the
    // two returned codes are DISTINCT and the W4 partial UNIQUE
    // `ux_offline_codes_consumed_by_doc` invariant holds (no row
    // duplicates `consumed_by_document_id`).  Mechanism: SQLite
    // RESERVED-lock serialises BEGIN IMMEDIATE writers; whichever
    // commits first wins the lowest available code, the other
    // sees an updated `consumed_at IS NULL` set and picks the
    // next.
    let (_d, pool) = fresh_pool().await;
    let pool = Arc::new(pool);
    // B8-1: seed with dps_code so acquire_code_tx can pick them.
    seed_dps_codes_raw(
        &pool,
        FN,
        &[
            ("DPS-1", 1),
            ("DPS-2", 2),
            ("DPS-3", 3),
            ("DPS-4", 4),
            ("DPS-5", 5),
        ],
    )
    .await;
    let doc_a = insert_fiscal_doc(&pool, FN, 1).await;
    let doc_b = insert_fiscal_doc(&pool, FN, 2).await;

    let p1 = Arc::clone(&pool);
    let p2 = Arc::clone(&pool);
    let t1 = tokio::spawn(async move { acquire_via_envelope(&p1, FN, doc_a).await });
    let t2 = tokio::spawn(async move { acquire_via_envelope(&p2, FN, doc_b).await });
    let r1 = t1.await.unwrap().unwrap();
    let r2 = t2.await.unwrap().unwrap();
    assert_ne!(
        r1.code_lnd, r2.code_lnd,
        "concurrent acquire must return distinct codes; got {} and {}",
        r1.code_lnd, r2.code_lnd
    );
    // Each acquired code is one of the first two (whichever
    // serialisation order wins).
    let codes: Vec<i64> = {
        let mut v = vec![r1.code_lnd, r2.code_lnd];
        v.sort();
        v
    };
    assert_eq!(codes, vec![1, 2], "the two lowest codes must be acquired");

    // W4 invariant: ux_offline_codes_consumed_by_doc holds — every
    // consumed_by_document_id is unique.
    let dup_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) - COUNT(DISTINCT consumed_by_document_id) FROM offline_codes \
         WHERE consumed_by_document_id IS NOT NULL",
    )
    .fetch_one(&*pool)
    .await
    .unwrap();
    assert_eq!(
        dup_count, 0,
        "no consumed code may share a consumed_by_document_id with another"
    );
}

// ─── list_pending_for_session basic shape ────────────────────────────

#[tokio::test]
async fn list_pending_for_session_returns_non_terminal_docs_in_lnd_order() {
    let (_d, pool) = fresh_pool().await;
    // Insert 3 docs with offline_session_id set.
    let sid = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions (offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-05-15T00:00:00Z')",
    )
    .bind(DbOfflineSessionId(sid))
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    let mut doc_ids: Vec<DocumentId> = Vec::new();
    for lnd in 1..=3 {
        let did = DocumentId::new();
        let rid = uuid::Uuid::now_v7();
        let sha = vec![0u8; 32];
        sqlx::query(
            "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
                state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
                payload_sha256_canonical, offline_session_id) \
             VALUES (?, ?, ?, ?, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
                '2026-05-15T00:00:00Z', '{}', ?, ?)",
        )
        .bind(DbDocumentId(did))
        .bind(rid.as_bytes().to_vec())
        .bind(FN)
        .bind(lnd)
        .bind(&sha)
        .bind(DbOfflineSessionId(sid))
        .execute(&pool)
        .await
        .unwrap();
        doc_ids.push(did);
    }
    let pending = offline_sessions::list_pending_for_session(&pool, sid)
        .await
        .unwrap();
    assert_eq!(pending.len(), 3, "all 3 PREPARED docs are non-terminal");
    // Order matches lnd ASC (insertion order in this fixture).
    assert_eq!(pending, doc_ids);
}

#[tokio::test]
async fn list_pending_for_session_excludes_terminal_states() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions (offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-05-15T00:00:00Z')",
    )
    .bind(DbOfflineSessionId(sid))
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    // ACK and REJECTED are terminal — should NOT appear in pending.
    for (lnd, state) in [(1, "ACK"), (2, "REJECTED"), (3, "PREPARED")] {
        let did = DocumentId::new();
        let rid = uuid::Uuid::now_v7();
        let sha = vec![0u8; 32];
        sqlx::query(
            "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
                state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
                payload_sha256_canonical, offline_session_id) \
             VALUES (?, ?, ?, ?, 'SELL', ?, 'b', 't', 'OFFLINE', \
                '2026-05-15T00:00:00Z', '{}', ?, ?)",
        )
        .bind(DbDocumentId(did))
        .bind(rid.as_bytes().to_vec())
        .bind(FN)
        .bind(lnd)
        .bind(state)
        .bind(&sha)
        .bind(DbOfflineSessionId(sid))
        .execute(&pool)
        .await
        .unwrap();
    }
    let pending = offline_sessions::list_pending_for_session(&pool, sid)
        .await
        .unwrap();
    assert_eq!(
        pending.len(),
        1,
        "only PREPARED (lnd=3) is pending; ACK/REJECTED are terminal"
    );
}

/// PR #54 Round 1 MED-1 (operator review 2026-05-15): pin the
/// inclusion of `OFFLINE_LOCAL_ACK` in `list_pending_for_session`.
///
/// The prior docstring claimed OFFLINE_LOCAL_ACK was excluded
/// ("already locally durable") but the SQL included it.  Operator
/// review resolved the contradiction: the SQL is right (W9 backlog-
/// drain needs to see OFFLINE_LOCAL_ACK rows so it can flip them
/// through the online ladder `OFFLINE_LOCAL_ACK → SENDING → SENT
/// → KVT1 …` after return-online).  This test pins that semantic
/// so a future refactor can't silently flip it.
#[tokio::test]
async fn list_pending_for_session_includes_offline_local_ack() {
    let (_d, pool) = fresh_pool().await;
    let sid = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions (offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-05-15T00:00:00Z')",
    )
    .bind(DbOfflineSessionId(sid))
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    // Single OFFLINE_LOCAL_ACK doc; must appear in pending.
    let did = DocumentId::new();
    let rid = uuid::Uuid::now_v7();
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, offline_session_id) \
         VALUES (?, ?, ?, 1, 'SELL', 'OFFLINE_LOCAL_ACK', 'b', 't', 'OFFLINE', \
            '2026-05-15T00:00:00Z', '{}', ?, ?)",
    )
    .bind(DbDocumentId(did))
    .bind(rid.as_bytes().to_vec())
    .bind(FN)
    .bind(&sha)
    .bind(DbOfflineSessionId(sid))
    .execute(&pool)
    .await
    .unwrap();
    let pending = offline_sessions::list_pending_for_session(&pool, sid)
        .await
        .unwrap();
    assert_eq!(
        pending,
        vec![did],
        "OFFLINE_LOCAL_ACK doc MUST be returned by list_pending_for_session (W9 backlog drain depends on this)"
    );
}

/// PR #54 Round 1 HIGH (operator review 2026-05-15): pool-level
/// ergonomic wrapper on the service layer must produce the same
/// row-state as the tx-bound primitive driven inline.
#[tokio::test]
async fn seed_via_service_matches_tx_helper() {
    let (_d, pool) = fresh_pool().await;
    let pool_arc = std::sync::Arc::new(pool.clone());
    let svc = prro::services::offline_session::OfflineSessionService::new(pool_arc);
    let inserted = svc.seed_code_range(FN, 1, 4).await.unwrap();
    assert_eq!(inserted, 4);
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 4);
    // Idempotent: a second call over the same range inserts 0.
    let second = svc.seed_code_range(FN, 1, 4).await.unwrap();
    assert_eq!(second, 0);
}
