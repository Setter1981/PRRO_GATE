//! PR-C2 schema slice — `offline_codes.dps_code` column + `insert_dps_codes_tx`.
//!
//! RED pins (each written before implementation):
//!   (a) `offline_codes_dps_code_column_roundtrips`: insert one code via the new
//!       primitive, read it back — `dps_code` string preserved, `code_lnd` = 1
//!       (COALESCE(MAX,0)+1 on an empty pool).
//!   (b) `dps_code_dedupe_is_idempotent`: same `(fn, dps_code)` inserted twice
//!       → second is no-op; summary reports 1 inserted / 1 deduped; DB has one row.
//!   (c) `drill_seed_path_unaffected`: `seed_code_range_tx` still works (integer
//!       range, `dps_code NULL`); coexists with `dps_code` rows in one pool;
//!       `acquire_code_tx` still consumes the lowest `code_lnd` regardless of kind.
//!   (d) migration-level: column + partial index present post-migration; two
//!       `NULL dps_code` rows for the same FN do NOT violate the partial index.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::offline_sessions::{self, InsertedSummary};
use prro::db::tx::with_immediate;
use prro::db::types::DbDocumentId;

const FN: &str = "4000162280";

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("c2.db"))
        .await
        .expect("open_pool runs migrations including 028");
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

/// Drive `insert_dps_codes_tx` through the same `with_immediate` envelope
/// production will use (W5 review axis 2 discipline).
async fn insert_dps_codes(
    pool: &sqlx::SqlitePool,
    fiscal_number: &str,
    codes: &[&str],
) -> InsertedSummary {
    let fn_owned = fiscal_number.to_string();
    let codes_owned: Vec<String> = codes.iter().map(|s| s.to_string()).collect();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            offline_sessions::insert_dps_codes_tx(tx, &fn_owned, &codes_owned)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap()
}

/// Insert a minimal `fiscal_documents` row so `offline_codes.consumed_by_document_id`
/// FK can be satisfied by `acquire_code_tx`.
async fn insert_fiscal_doc(pool: &sqlx::SqlitePool, fiscal_number: &str, lnd: i64) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = uuid::Uuid::now_v7();
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
            '2026-07-07T00:00:00Z', '{}', ?)",
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

// ─── (a) column roundtrip ───────────────────────────────────────────────────

/// Pin (a): insert one DPS-issued code via the new primitive on an empty pool.
/// `code_lnd` must be assigned 1 (= COALESCE(MAX(0), 0) + 1); `dps_code` must
/// round-trip as the exact string.
#[tokio::test]
async fn offline_codes_dps_code_column_roundtrips() {
    let (_d, pool) = fresh_pool().await;

    let summary = insert_dps_codes(&pool, FN, &["eYme-jhnkWQ"]).await;
    assert_eq!(summary.inserted, 1, "one code must be inserted");
    assert_eq!(summary.deduped, 0, "no dedupes on first insert");

    let row: (i64, Option<String>) = sqlx::query_as(
        "SELECT code_lnd, dps_code \
         FROM offline_codes \
         WHERE fiscal_number = ? AND dps_code = 'eYme-jhnkWQ'",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .expect("the inserted row must be readable back");

    assert_eq!(
        row.0, 1,
        "code_lnd must be COALESCE(MAX(code_lnd),0)+1 = 1 on an empty pool"
    );
    assert_eq!(
        row.1.as_deref(),
        Some("eYme-jhnkWQ"),
        "dps_code must be preserved verbatim (live DPS string)"
    );
}

/// A second distinct DPS code on the same FN gets code_lnd = 2 (MAX is now 1).
#[tokio::test]
async fn offline_codes_dps_code_sequential_lnd_assignment() {
    let (_d, pool) = fresh_pool().await;

    let s1 = insert_dps_codes(&pool, FN, &["code-first"]).await;
    assert_eq!(s1.inserted, 1);

    let s2 = insert_dps_codes(&pool, FN, &["code-second"]).await;
    assert_eq!(s2.inserted, 1);

    let lnds: Vec<i64> = sqlx::query_scalar(
        "SELECT code_lnd FROM offline_codes WHERE fiscal_number = ? ORDER BY code_lnd ASC",
    )
    .bind(FN)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        lnds,
        vec![1, 2],
        "sequential calls must assign code_lnd 1, 2"
    );
}

// ─── (b) dedupe idempotency ─────────────────────────────────────────────────

/// Pin (b): inserting the same `(fn, dps_code)` a second time must be a no-op.
/// Summary must report 1 inserted / 1 deduped; DB must hold exactly one row.
#[tokio::test]
async fn dps_code_dedupe_is_idempotent() {
    let (_d, pool) = fresh_pool().await;

    let s1 = insert_dps_codes(&pool, FN, &["code-alpha"]).await;
    assert_eq!(s1.inserted, 1, "first insert must record 1 inserted");
    assert_eq!(s1.deduped, 0, "no dedupes on first insert");

    let s2 = insert_dps_codes(&pool, FN, &["code-alpha"]).await;
    assert_eq!(
        s2.inserted, 0,
        "second insert of same dps_code must be a no-op"
    );
    assert_eq!(s2.deduped, 1, "deduped count must be 1");

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 1, "only one row must exist after duplicate insert");
}

/// Batch with a mix of new and already-present codes: correct split in summary.
#[tokio::test]
async fn dps_code_batch_with_partial_dedupe() {
    let (_d, pool) = fresh_pool().await;

    // Pre-insert "A".
    let s1 = insert_dps_codes(&pool, FN, &["A"]).await;
    assert_eq!(s1.inserted, 1);

    // Batch: "A" (dup), "B" (new), "A" (dup again in same call), "C" (new).
    // Expected: 2 inserted (B, C) + 2 deduped (A, A).
    let s2 = insert_dps_codes(&pool, FN, &["A", "B", "A", "C"]).await;
    assert_eq!(s2.inserted, 2, "B and C are new");
    assert_eq!(s2.deduped, 2, "A appears twice in this call, both ignored");

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 3, "A, B, C — three distinct codes total");
}

/// Empty slice is a fast-path no-op returning (0, 0).
#[tokio::test]
async fn dps_code_empty_slice_is_noop() {
    let (_d, pool) = fresh_pool().await;
    let s = insert_dps_codes(&pool, FN, &[]).await;
    assert_eq!(
        s,
        InsertedSummary {
            inserted: 0,
            deduped: 0
        }
    );
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 0);
}

// ─── (c) drill seed path — acquire skips NULL-dps_code drill rows ──────────

/// Pin (c): `seed_code_range_tx` works exactly as before (integer range,
/// `dps_code NULL`); `insert_dps_codes_tx` coexists in the same pool;
/// `acquire_code_tx` skips drill codes (dps_code IS NULL) and picks only
/// DPS-issued codes — this is the B8-1 real-first guard.
#[tokio::test]
async fn drill_seed_path_unaffected() {
    let (_d, pool) = fresh_pool().await;

    // Seed drill codes 1-3 (dps_code NULL, old behaviour).
    let fn_owned = FN.to_string();
    let drill_inserted = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            offline_sessions::seed_code_range_tx(tx, &fn_owned, 1, 3)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();
    assert_eq!(
        drill_inserted, 3,
        "seed_code_range_tx must insert 3 drill codes"
    );

    // Verify drill codes have dps_code NULL.
    let null_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NULL",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(null_count, 3, "drill codes must carry dps_code NULL");

    // Add one DPS-issued code on top (code_lnd must be 4 = MAX(3)+1).
    let s = insert_dps_codes(&pool, FN, &["dps-X"]).await;
    assert_eq!(s.inserted, 1);

    let dps_lnd: i64 = sqlx::query_scalar(
        "SELECT code_lnd FROM offline_codes WHERE fiscal_number = ? AND dps_code = 'dps-X'",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        dps_lnd, 4,
        "DPS code gets code_lnd = MAX(drill codes)+1 = 4"
    );

    // B8-1: acquire_code_tx skips drill codes (dps_code IS NULL) and picks the
    // DPS-issued code (code_lnd=4) — the lowest code with dps_code IS NOT NULL.
    let doc_id = insert_fiscal_doc(&pool, FN, 1).await;
    let fn_owned2 = FN.to_string();
    let acquired = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            offline_sessions::acquire_code_tx(tx, &fn_owned2, doc_id)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();
    assert_eq!(
        acquired.code_lnd, 4,
        "acquire_code_tx must skip drill codes (dps_code NULL) and pick the DPS-issued code"
    );
    assert_eq!(
        acquired.dps_code, "dps-X",
        "acquired dps_code must be the DPS-issued string"
    );
}

/// `seed_code_range_tx` is idempotent on overlap — unaffected by the new column.
#[tokio::test]
async fn seed_code_range_still_idempotent_with_dps_column_present() {
    let (_d, pool) = fresh_pool().await;

    let fn_owned = FN.to_string();
    let first = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            offline_sessions::seed_code_range_tx(tx, &fn_owned, 1, 5)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();
    assert_eq!(first, 5);

    let fn_owned2 = FN.to_string();
    let second = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            offline_sessions::seed_code_range_tx(tx, &fn_owned2, 3, 7)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();
    assert_eq!(
        second, 2,
        "INSERT OR IGNORE skips codes 3-5 already present; inserts 6,7"
    );
}

// ─── (d) migration schema pins ─────────────────────────────────────────────

/// Pin (d-i): `dps_code` column exists in `offline_codes` after migration 028.
#[tokio::test]
async fn migration_028_dps_code_column_exists() {
    let (_d, pool) = fresh_pool().await;

    let cols: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('offline_codes')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        cols.iter().any(|c| c == "dps_code"),
        "offline_codes must have dps_code column after migration 028; cols: {cols:?}"
    );
}

/// Pin (d-ii): partial unique index `ux_offline_codes_fn_dps_code` exists.
#[tokio::test]
async fn migration_028_partial_unique_index_exists() {
    let (_d, pool) = fresh_pool().await;

    let idx: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master \
         WHERE type = 'index' AND name = 'ux_offline_codes_fn_dps_code'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        idx.as_deref(),
        Some("ux_offline_codes_fn_dps_code"),
        "partial unique index ux_offline_codes_fn_dps_code must exist"
    );
}

/// Pin (d-iii): two rows with `dps_code NULL` for the same FN do NOT violate
/// the partial index (SQLite partial indexes exclude rows where the predicate is false).
#[tokio::test]
async fn migration_028_two_null_dps_codes_same_fn_not_unique_violation() {
    let (_d, pool) = fresh_pool().await;

    // Insert two drill-style rows (dps_code implicitly NULL via seed_code_range_tx).
    let fn_owned = FN.to_string();
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            offline_sessions::seed_code_range_tx(tx, &fn_owned, 1, 2)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();

    // Both rows present with dps_code NULL — no constraint violation.
    let null_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NULL",
    )
    .bind(FN)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        null_count, 2,
        "two NULL dps_code rows for the same FN must coexist without violating the partial index"
    );
}
