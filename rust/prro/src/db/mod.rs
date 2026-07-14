pub mod backup;
pub mod invariant_scan;
pub mod models;
pub mod repositories;
pub mod tx;
// CS-1b (contract §4): store-side sqlx wrappers (`DbDocState`, …) for the pure
// `prro-domain` TEXT enums. Public so repository code + the `prro`-scope
// conformance/round-trip tests (RP-CS1-5) can name them. Relocates into
// `prro-store-sqlite` at CS-7.
pub mod types;

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
///      foreign_keys ON, FULL synchronous, busy_timeout 5s) so the
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
        .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations_secure").run(&pool).await?;

    // HIGH-AUDIT-01: enforce owner-only mode on the secure file AND its WAL
    // sidecars.  Extracted to [`set_owner_only_perms`] (RS-4) so the backup
    // module applies the IDENTICAL posture to snapshot artifacts (not a copy).
    set_owner_only_perms(path)?;

    Ok(pool)
}

/// HIGH-AUDIT-01 / RS-4 — enforce owner-only (`0o600`) mode on a SQLite file
/// AND its WAL sidecars (`-wal`, `-shm`), tolerating sidecar absence.
///
/// SQLite in WAL journal mode produces up to three physical files: `<path>`
/// (the main DB), `<path>-wal` (the un-checkpointed write-ahead log), and
/// `<path>-shm` (the shared memory mapping).  Newly written rows — including the
/// cashier `key_pass_enc` BLOBs that motivate the secure-DB isolation — land in
/// `-wal` before the next checkpoint flushes them to the main file.  If only the
/// main file were chmod'd, `-wal` would retain the process umask (typically
/// `0o644`/`0o666`), leaving the un-checkpointed write log world-readable and
/// defeating the isolation guarantee.
///
/// Existence is checked per-sidecar (no TOCTOU: fetch metadata directly and
/// tolerate `NotFound`, since `-wal`/`-shm` are created lazily and SQLite may
/// delete `-wal` during a concurrent checkpoint); other IO errors propagate
/// fail-closed.  Unix-only — Windows has no equivalent mode bit (no-op; the
/// platform ACL story applies separately).
pub(crate) fn set_owner_only_perms(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let main_path = path.as_os_str().to_owned();
        for suffix in ["", "-wal", "-shm"] {
            let mut sidecar = main_path.clone();
            sidecar.push(suffix);
            let sidecar = std::path::PathBuf::from(sidecar);
            let meta = match std::fs::metadata(&sidecar) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&sidecar, perms)?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path; // no mode bit on non-unix; best-effort no-op
    }
    Ok(())
}

/// Open a connection pool against the given SQLite file.
///
/// Sets WAL journal mode, busy_timeout 5s, foreign_keys ON, FULL synchronous.
/// Migrations are applied via `sqlx::migrate!()`.
///
/// **Why FULL, not NORMAL (durability audit 2026-06-10, operator-signed).**
/// Under WAL+NORMAL a commit reaches the OS page cache but is fsync'd only
/// at checkpoint time: an app crash is safe, but a POWER CUT (or kernel
/// panic) can drop the tail of COMMITTED transactions.  For this ledger
/// that is not a routine data loss: an `OFFLINE_LOCAL_ACK` receipt already
/// handed to the customer would vanish together with its consumed offline
/// code (→ the code could be CONSUMED TWICE), and `node_state.next_lnd` /
/// the MAC chain seed would roll back (→ lnd reuse against DPS, chain
/// break).  Pilot hardware is heterogeneous and mostly WITHOUT a UPS, and
/// grid outages make power cuts the EXPECTED failure mode — so every
/// commit pays one WAL fsync (~1–30ms depending on storage).  At receipt
/// rates (a few per minute per FN) this is negligible; do NOT relax back
/// to NORMAL for performance without re-running the durability analysis.
pub async fn open_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let url = format!("sqlite:{}", path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
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
        .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;
    Ok(pool)
}
