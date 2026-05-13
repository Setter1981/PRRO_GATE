pub mod models;
pub mod repositories;
pub mod tx;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

/// Open a connection pool against the given SQLite file.
///
/// Sets WAL journal mode, busy_timeout 5s, foreign_keys ON, NORMAL synchronous.
/// Migrations are applied via `sqlx::migrate!()`.
pub async fn open_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let url = format!("sqlite:{}", path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// M3a hardening pass 3 — probe-only open of an existing SQLite
/// file WITHOUT applying migrations.
///
/// Used by `App::boot` to run `PRAGMA quick_check(1)` against an
/// existing DB BEFORE migrations can re-apply and silently overwrite
/// corrupted pages (sqlx::migrate! recreates tables when
/// `_sqlx_migrations` is unreadable, masking integrity failures —
/// see `tests/app_boot_quick_check_failure.rs` module docstring for
/// the empirical evidence behind this design).
///
/// **Existing-DB-only path.**  `create_if_missing(false)` makes this
/// pool refuse to bootstrap a fresh file; caller has already
/// verified `path` exists before invoking this helper.  Returns
/// `sqlx::Error::Database` if the file is not a valid SQLite db
/// (caller may treat that as an integrity-class failure or as a
/// distinct config error — `App::boot` chooses the integrity path
/// per W9 freeze §10.2 fail-closed semantics).
///
/// **Single connection cap.**  `max_connections(1)` because this is
/// a one-shot probe; we don't want to leave a long-lived pool that
/// can hold WAL locks against the second-phase migrate open.  The
/// caller MUST `pool.close().await` before invoking [`open_pool`]
/// for the second-phase open.
///
/// Same WAL / foreign_keys / synchronous / busy_timeout PRAGMAs as
/// [`open_pool`] for behavioural parity — the probe sees the file
/// the way a normal open would, minus the migration replay.
pub async fn open_pool_no_migrate(path: &Path) -> anyhow::Result<SqlitePool> {
    let url = format!("sqlite:{}", path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(false)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;
    Ok(pool)
}
