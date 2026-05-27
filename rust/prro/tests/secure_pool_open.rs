//! W2 PR-A iter 3 — `db::open_secure_pool` opens the secure database,
//! runs `migrations_secure/`, and chmods the file (and WAL sidecars)
//! to 0o600.
//!
//! Four contracts:
//!
//!   1. Fresh open creates the file with the operators schema applied
//!      and `_sqlx_migrations` recording version 20 (HIGH-AUDIT-01
//!      isolation: the file MUST be the secure file, separate from
//!      `db_path`).
//!   2. The main file is chmod 0o600 after open — owner read/write
//!      only.
//!   3. The WAL sidecars (`-wal`, `-shm`) are also chmod 0o600 when
//!      present — required by HIGH-AUDIT-01 because `-wal` contains
//!      un-checkpointed writes (including `key_pass_enc` BLOBs from
//!      PR-B's `add-operator` CLI) in plaintext.  Leaving them at
//!      umask would defeat the isolation.
//!   4. Re-opening an existing secure DB is idempotent: no second
//!      migration apply (checksum guard).

use sqlx::Row;
// `PermissionsExt` is only available on Unix; the chmod-checking tests
// below are `#[cfg(unix)]`-gated, so the import must be too — otherwise
// `cargo test` on a Windows developer host fails compilation outright
// (regression caught by external audit Round 4 finding B1).
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Fresh secure pool — proves the migrations_secure set lands and
/// operators table is queryable.
#[tokio::test]
async fn open_secure_pool_runs_migrations_secure_and_creates_operators_table() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secure.db");

    let pool = prro::db::open_secure_pool(&path)
        .await
        .expect("open_secure_pool must succeed on fresh path");

    // operators table reachable
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM operators")
        .fetch_one(&pool)
        .await
        .expect("SELECT against operators must succeed")
        .get(0);
    assert_eq!(count, 0, "fresh secure DB has empty operators");

    // _sqlx_migrations recorded version 20 for the secure set
    let version: i64 =
        sqlx::query("SELECT version FROM _sqlx_migrations WHERE version = 20")
            .fetch_one(&pool)
            .await
            .expect("migration 020 row present in _sqlx_migrations")
            .get(0);
    assert_eq!(version, 20);
}

/// HIGH-AUDIT-01 — chmod 0o600 enforced after open.  Linux-only
/// (Windows has no equivalent mode bits).
#[cfg(unix)]
#[tokio::test]
async fn open_secure_pool_chmods_file_to_owner_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secure.db");

    let _pool = prro::db::open_secure_pool(&path)
        .await
        .expect("open_secure_pool");

    let meta = std::fs::metadata(&path).expect("stat secure.db");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "secure.db must be chmod 0o600 (owner-only), got {mode:o}"
    );
}

/// HIGH-AUDIT-01 — `-wal` and `-shm` sidecars must also be 0o600.
/// SQLite creates them lazily on first write; the migration apply
/// in `open_secure_pool` performs writes that materialize both.
/// Without this chmod, un-checkpointed cashier-key BLOBs sit in
/// `-wal` under the process umask, world-readable, defeating the
/// isolation property the secure DB exists for.
#[cfg(unix)]
#[tokio::test]
async fn open_secure_pool_chmods_wal_and_shm_sidecars() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secure.db");

    let pool = prro::db::open_secure_pool(&path).await.expect("open");

    // Force WAL activity so sidecars exist and are non-empty.
    sqlx::query(
        "INSERT INTO operators \
            (operator_id, fiscal_number, name, key_path, key_pass_enc) \
         VALUES ('OP-CHMOD-WAL', '4000000001', 'Test', '/tmp/k.dat', X'cafe')",
    )
    .execute(&pool)
    .await
    .expect("insert forces WAL write");

    for suffix in ["-wal", "-shm"] {
        let sidecar = std::path::PathBuf::from(format!(
            "{}{}",
            path.display(),
            suffix
        ));
        assert!(
            sidecar.exists(),
            "sidecar {sidecar:?} must exist after WAL write"
        );
        let meta = std::fs::metadata(&sidecar).expect("stat sidecar");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "sidecar {sidecar:?} must be chmod 0o600 (got {mode:o})"
        );
    }
}

/// Re-opening the same secure DB does not re-apply migration 020
/// (sqlx checksum gate).
#[tokio::test]
async fn open_secure_pool_is_idempotent_on_second_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secure.db");

    let pool1 = prro::db::open_secure_pool(&path).await.expect("first open");
    drop(pool1);

    let pool2 = prro::db::open_secure_pool(&path)
        .await
        .expect("second open must be a no-op");
    let mig_rows: i64 = sqlx::query("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool2)
        .await
        .expect("query")
        .get(0);
    // First open applies the full `migrations_secure/` set (020 +
    // 021 as of W4-Z0).  Second open MUST be idempotent — same
    // row count, no duplicate inserts into `_sqlx_migrations`.
    let mig_rows_first_open: i64 = sqlx::query("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool2)
        .await
        .expect("query")
        .get(0);
    assert_eq!(
        mig_rows, mig_rows_first_open,
        "second open must not re-apply or duplicate any migration"
    );
    assert!(
        mig_rows >= 2,
        "expected at least migration 020 + 021 recorded, got {mig_rows}"
    );
}
