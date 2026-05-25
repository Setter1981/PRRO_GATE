pub mod models;
pub mod repositories;
pub mod tx;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

/// W2 / HIGH-AUDIT-01 — open the **secure** SQLite pool.
///
/// Distinct from [`open_pool`] in three ways:
///
///   1. Migration set is `./migrations_secure/` (currently a single
///      migration 020 creating the `operators` table).  See
///      `rust/prro/migrations_secure/README.md` for why this lives in
///      a separate directory.
///   2. After open, the underlying file is `chmod 0o600` (owner read/
///      write only) on Unix.  Defense-in-depth: prevents accidental
///      world-readable misconfiguration of the cashier-key store.
///      Windows has no equivalent mode bit; the chmod is a no-op via
///      `cfg(unix)` and the platform's ACL story applies separately.
///   3. The pool has the same PRAGMA tuning as [`open_pool`] (WAL,
///      foreign_keys ON, NORMAL synchronous, busy_timeout 5s) so the
///      secure file behaves identically under concurrent access.
///
/// Failure modes:
///
///   - Path parent missing → sqlx returns the underlying IO error.
///   - Migration checksum mismatch → sqlx refuses to apply.
///   - `chmod` failure → returned as `anyhow::Error` so boot fails
///     fast rather than silently leaving the file world-readable.
pub async fn open_secure_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let url = format!("sqlite:{}", path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations_secure").run(&pool).await?;

    // HIGH-AUDIT-01: enforce owner-only mode on the secure file AND
    // its WAL sidecars.  SQLite in WAL journal mode produces three
    // physical files: `<path>` (the main DB), `<path>-wal` (the
    // un-checkpointed write-ahead log), and `<path>-shm` (the shared
    // memory mapping).  Newly written rows — including the cashier
    // `key_pass_enc` BLOBs that motivate this whole isolation —
    // land in `-wal` before the next checkpoint flushes them to the
    // main file.  If only the main file is chmod'd to 0o600 then
    // `-wal` retains the process umask (typically 0o644 / 0o666),
    // leaving the un-checkpointed write log world-readable on disk
    // and defeating the HIGH-AUDIT-01 isolation guarantee.
    //
    // We chmod each sidecar to 0o600 if it exists.  Existence is
    // checked because `-wal` and `-shm` are created lazily by SQLite
    // on first write; the migration apply above performed writes so
    // they are normally present, but we tolerate their absence (e.g.,
    // pristine open then close without writes never creates them).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let main_path = path.as_os_str().to_owned();
        for suffix in ["", "-wal", "-shm"] {
            let mut sidecar = main_path.clone();
            sidecar.push(suffix);
            let sidecar = std::path::PathBuf::from(sidecar);
            if !sidecar.exists() {
                continue;
            }
            let mut perms = std::fs::metadata(&sidecar)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&sidecar, perms)?;
        }
    }

    Ok(pool)
}

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
