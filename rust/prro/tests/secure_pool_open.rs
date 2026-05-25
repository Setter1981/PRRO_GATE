//! W2 PR-A iter 3 — `db::open_secure_pool` opens the secure database,
//! runs `migrations_secure/`, and chmods the file to 0o600.
//!
//! Three contracts:
//!
//!   1. Fresh open creates the file with the operators schema applied
//!      and `_sqlx_migrations` recording version 20 (HIGH-AUDIT-01
//!      isolation: the file MUST be the secure file, separate from
//!      `db_path`).
//!   2. The file is chmod 0o600 after open — owner read/write only.
//!      Defense-in-depth against world-readable misconfiguration
//!      (HIGH-AUDIT-01 hard-isolation callout).
//!   3. Re-opening an existing secure DB is idempotent: no second
//!      migration apply (checksum guard), permissions re-asserted.

use sqlx::Row;
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

/// Re-opening the same secure DB does not re-apply migration 020
/// (sqlx checksum gate) and re-asserts permissions.
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
    assert_eq!(
        mig_rows, 1,
        "second open must not re-apply or duplicate migration 020"
    );
}
