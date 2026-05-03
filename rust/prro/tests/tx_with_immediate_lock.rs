//! Verifies two concurrent writers correctly contend on
//! `with_immediate`'s RESERVED lock (spec decision #39), and that
//! a returned Err triggers ROLLBACK.

use prro::db::{open_pool, tx::with_immediate};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::time::{Duration, Instant};

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contention.sqlite3");
    // Leak the tempdir so the file persists for the test duration; the OS
    // will reclaim it when the test process exits.
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

/// Pool with `max_connections = 1` so a single dirty connection is
/// guaranteed to be reused on the next acquire — needed to assert
/// `with_immediate` cleans up after a failed COMMIT.
async fn single_conn_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commit_fail.sqlite3");
    std::mem::forget(dir);
    let url = format!("sqlite:{}", path.display());
    let opts = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_writers_serialize() {
    let pool = fresh_pool().await;
    let p1 = pool.clone();
    let p2 = pool.clone();

    let started_at = Instant::now();

    // Writer 1 acquires the RESERVED lock, holds it for 200 ms, then commits.
    let t1 = tokio::spawn(async move {
        with_immediate(&p1, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
                     VALUES ('1111111111', '12345678', 'test')",
                )
                .execute(&mut *conn)
                .await?;
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok::<_, anyhow::Error>(started_at.elapsed())
            })
        })
        .await
    });

    // Writer 2 starts ~50 ms after Writer 1 and must block on RESERVED until
    // Writer 1 commits.  open_pool sets busy_timeout = 5s, which is plenty.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let t2 = tokio::spawn(async move {
        with_immediate(&p2, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
                     VALUES ('2222222222', '12345678', 'test')",
                )
                .execute(&mut *conn)
                .await?;
                Ok::<_, anyhow::Error>(started_at.elapsed())
            })
        })
        .await
    });

    let elapsed1 = t1.await.unwrap().unwrap();
    let elapsed2 = t2.await.unwrap().unwrap();

    assert!(
        elapsed1.as_millis() >= 200,
        "writer 1 elapsed {:?} (expected ≥ 200 ms)",
        elapsed1
    );
    assert!(
        elapsed2 >= elapsed1,
        "writer 2 ({:?}) must finish after writer 1 ({:?})",
        elapsed2,
        elapsed1
    );

    // Both rows must be present after both writers commit.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_number_config WHERE tax_number = '12345678'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2, "both writers must have committed");
}

#[tokio::test]
async fn rollback_on_commit_failure_keeps_pool_clean() {
    // Single-connection pool guarantees the next acquire reuses the
    // possibly-dirty connection from the failed COMMIT.
    let pool = single_conn_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Force COMMIT to fail: defer FK enforcement to commit time, then
    // insert a fiscal_documents row with a non-existent fiscal_number.
    let r1: anyhow::Result<()> = with_immediate(&pool, |conn| {
        Box::pin(async move {
            sqlx::query("PRAGMA defer_foreign_keys = ON")
                .execute(&mut *conn)
                .await?;
            let doc_id = vec![0xAAu8; 16];
            let req_id = vec![0xBBu8; 16];
            let sha = vec![0u8; 32];
            sqlx::query(
                "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, \
                    doc_type, state, backend_profile_id, transport_profile_id, fs_mode, \
                    business_ts, payload_json, payload_sha256_canonical) \
                 VALUES (?, ?, '9999999999', 1, 'SELL', 'PREPARED', 'b', 't', 'ONLINE', \
                    '2026-01-01T00:00:00Z', '{}', ?)",
            )
            .bind(&doc_id)
            .bind(&req_id)
            .bind(&sha)
            .execute(&mut *conn)
            .await?;
            Ok(())
        })
    })
    .await;
    assert!(r1.is_err(), "COMMIT must fail under deferred FK violation");

    // The exact bug being fixed: if the failing COMMIT did not trigger a
    // ROLLBACK, the connection returns to the pool with the transaction
    // still open, and the next BEGIN IMMEDIATE fails with
    // "cannot start a transaction within a transaction".
    let r2: anyhow::Result<()> = with_immediate(&pool, |conn| {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
                 VALUES ('5555555555', '12345678', 'test')",
            )
            .execute(&mut *conn)
            .await?;
            Ok(())
        })
    })
    .await;
    assert!(
        r2.is_ok(),
        "follow-up tx must succeed (commit-failure path must rollback): {:?}",
        r2
    );

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_number_config WHERE fiscal_number = '5555555555'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn rollback_on_error_removes_inserted_row() {
    let pool = fresh_pool().await;
    let result: anyhow::Result<()> = with_immediate(&pool, |conn| {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
                 VALUES ('3333333333', '12345678', 'test')",
            )
            .execute(&mut *conn)
            .await?;
            Err(anyhow::anyhow!("simulated failure"))
        })
    })
    .await;
    assert!(result.is_err(), "Err propagation must reach the caller");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_number_config WHERE fiscal_number = '3333333333'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "rollback must remove the inserted row");
}
