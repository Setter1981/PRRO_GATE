//! B8-1 RED pins: `acquire_code_tx` must skip NULL-dps_code drill rows and
//! prefer real (dps_code IS NOT NULL) rows; an all-drill pool returns
//! `CodePoolExhausted`.
//!
//! Each test starts RED (before the `AND dps_code IS NOT NULL` guard is
//! added to `acquire_code_tx`) and turns GREEN after the implementation.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::offline_sessions::{self, OfflineSessionError};
use prro::db::tx::with_immediate;
use prro::db::types::DbDocumentId;
use uuid::Uuid;

const FN: &str = "9990000001";

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("b8_1.db"))
        .await
        .expect("open_pool");
    // seed fiscal_number_config row (FK required by offline_codes)
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '99000001', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

/// Insert a minimal fiscal_documents row in SIGNED state so the
/// `consumed_by_document_id` FK in offline_codes is satisfied when
/// acquire_code_tx sets it.
async fn insert_signed_doc(pool: &sqlx::SqlitePool) -> DocumentId {
    let doc_id = DocumentId::new();
    let req_id = Uuid::now_v7();
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, 1, 'SELL', 'SIGNED', 'b', 't', 'OFFLINE', \
            '2026-05-15T00:00:00Z', '{}', ?, ?)",
    )
    .bind(DbDocumentId(doc_id))
    .bind(req_id.as_bytes().to_vec())
    .bind(FN)
    .bind(&sha)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

/// B8-1 pin (i): mixed pool — one drill row (dps_code NULL, LOW code_lnd=1)
/// and one real row (dps_code set, higher code_lnd=2).
/// `acquire_code_tx` must return the REAL row (code_lnd=2, dps_code="REAL-1"),
/// NOT the drill row.
///
/// Before fix: returns code_lnd=1 (drill) → RED.
/// After fix: returns code_lnd=2 (real)  → GREEN.
#[tokio::test]
async fn mixed_pool_acquire_returns_real_row_not_drill() {
    let (_d, pool) = fresh_pool().await;

    // Drill row at lnd=1 (dps_code implicitly NULL via seed_code_range_tx)
    with_immediate(&pool, |tx| {
        Box::pin(async move {
            offline_sessions::seed_code_range_tx(tx, FN, 1, 1).await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .unwrap();

    // Real row at lnd=2 via insert_dps_codes_tx
    with_immediate(&pool, |tx| {
        Box::pin(async move {
            offline_sessions::insert_dps_codes_tx(tx, FN, &["REAL-1".to_string()]).await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .unwrap();

    // Verify setup: lnd=1 is drill (NULL), lnd=2 is real
    let rows: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT code_lnd, dps_code FROM offline_codes WHERE fiscal_number = ? ORDER BY code_lnd",
    )
    .bind(FN)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], (1, None), "lnd=1 is drill (NULL)");
    assert_eq!(rows[1], (2, Some("REAL-1".to_string())), "lnd=2 is real");

    // A doc row satisfies the consumed_by_document_id FK in offline_codes.
    let doc_id = insert_signed_doc(&pool).await;
    let acquired = with_immediate(&pool, |tx| {
        let doc = doc_id;
        Box::pin(async move {
            offline_sessions::acquire_code_tx(tx, FN, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap();

    // Must return the REAL row (lnd=2, dps_code="REAL-1"), not the drill row.
    assert_eq!(
        acquired.code_lnd, 2,
        "expected real row (lnd=2), got lnd={} — drill row was returned instead of real",
        acquired.code_lnd
    );
    assert_eq!(
        acquired.dps_code, "REAL-1",
        "expected dps_code=REAL-1, got {:?}",
        acquired.dps_code
    );
}

/// B8-1 pin (ii): pool with ONLY drill (NULL) rows → `acquire_code_tx` must
/// return `CodePoolExhausted` (not return a drill row).
///
/// Before fix: returns a drill row → RED.
/// After fix: returns `CodePoolExhausted` → GREEN.
#[tokio::test]
async fn only_drill_pool_returns_code_pool_exhausted() {
    let (_d, pool) = fresh_pool().await;

    // Seed only drill rows (dps_code NULL)
    with_immediate(&pool, |tx| {
        Box::pin(async move {
            offline_sessions::seed_code_range_tx(tx, FN, 1, 3).await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .unwrap();

    // The UPDATE never fires (no real row found), so the FK is never
    // set, but we still need a doc row for the query binding.
    let doc_id = insert_signed_doc(&pool).await;
    let result = with_immediate(&pool, |tx| {
        let doc = doc_id;
        Box::pin(async move {
            offline_sessions::acquire_code_tx(tx, FN, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await;

    match result {
        Err(e) => {
            let os_err = e.downcast::<OfflineSessionError>().unwrap_or_else(|e2| {
                panic!("expected OfflineSessionError::CodePoolExhausted, got: {e2:?}")
            });
            assert!(
                matches!(os_err, OfflineSessionError::CodePoolExhausted { .. }),
                "expected CodePoolExhausted, got: {os_err:?}"
            );
        }
        Ok(a) => panic!(
            "expected CodePoolExhausted for drill-only pool, got Ok(code_lnd={}, dps_code={:?})",
            a.code_lnd, a.dps_code
        ),
    }
}
