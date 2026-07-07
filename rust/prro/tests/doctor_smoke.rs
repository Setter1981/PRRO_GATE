//! Verifies `prro doctor` honours the maintenance-lock contract: the
//! singleton lock is acquired BEFORE the DB pool is opened, so a running
//! `prro serve` (holding the lock) makes doctor fail fast without
//! touching the database.

use prro::runtime::singleton;
use std::fs;
use std::path::{Path, PathBuf};

fn cfg_toml(db_path: &str) -> String {
    format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#
    )
}

fn write_cfg(dir: &Path, db_path: &Path) -> PathBuf {
    let cfg_path = dir.join("prro.toml");
    let text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    fs::write(&cfg_path, text).unwrap();
    cfg_path
}

#[tokio::test]
async fn doctor_refuses_when_lock_is_already_held_and_does_not_touch_db() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.sqlite3");
    let cfg_path = write_cfg(dir.path(), &db_path);

    // Spec §3.4.1: maintenance CLI must refuse before any direct DB-touch.
    // Acquire the singleton lock as if a `prro serve` were already running.
    let _holder = singleton::acquire(&db_path).expect("primary lock");

    let err = prro::doctor::run(&cfg_path, None)
        .await
        .expect_err("doctor must fail while serve holds the lock");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("already running"),
        "doctor must surface the singleton conflict, got: {msg}"
    );

    // Critical assertion: the DB file was NOT created.  If the lock check
    // had come AFTER `db::open_pool`, the file (with migrations) would now
    // exist.  Lock-first ordering means the DB is untouched.
    assert!(
        !db_path.exists(),
        "doctor must NOT touch the DB before the lock check"
    );
}

#[tokio::test]
async fn doctor_succeeds_when_lock_is_free() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.sqlite3");
    let cfg_path = write_cfg(dir.path(), &db_path);

    prro::doctor::run(&cfg_path, None)
        .await
        .expect("doctor must pass on a clean dir");
    assert!(
        db_path.exists(),
        "doctor's pool open must have created the DB"
    );
}
