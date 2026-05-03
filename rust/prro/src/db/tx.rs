//! Single source of truth for write transactions.
//!
//! `pool.begin()` opens BEGIN DEFERRED; nesting BEGIN IMMEDIATE inside
//! is a SQLite error.  This helper acquires a raw connection and
//! issues `BEGIN IMMEDIATE` directly, ensuring writers contend on the
//! RESERVED lock from the very first statement (spec decision #39).

use futures::future::BoxFuture;
use sqlx::{SqliteConnection, SqlitePool};

pub async fn with_immediate<R, F>(pool: &SqlitePool, f: F) -> anyhow::Result<R>
where
    F: for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, anyhow::Result<R>> + Send,
    R: Send,
{
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    match f(&mut *conn).await {
        Ok(r) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(r)
        }
        Err(e) => {
            // Best-effort rollback; ignore secondary error.
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(e)
        }
    }
}
