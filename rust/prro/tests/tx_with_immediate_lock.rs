//! Verifies two concurrent writers correctly contend on
//! `with_immediate`'s RESERVED lock (spec decision #39), and that
//! a returned Err triggers ROLLBACK.

use prro::db::{open_pool, tx::with_immediate};
use std::time::{Duration, Instant};

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contention.sqlite3");
    // Leak the tempdir so the file persists for the test duration; the OS
    // will reclaim it when the test process exits.
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
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
