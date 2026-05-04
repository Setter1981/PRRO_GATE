use prro::runtime::singleton;

#[test]
fn second_acquisition_fails_with_already_running_message() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("x.db");
    let _lock = singleton::acquire(&db_path).expect("first acquire must succeed");
    let err = singleton::acquire(&db_path).expect_err("second acquire must fail");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("already running"),
        "error must mention 'already running': {msg}"
    );
}

#[test]
fn lock_releases_on_drop_and_re_acquires_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("y.db");
    {
        let _lock = singleton::acquire(&db_path).unwrap();
        // First holder drops at end of scope.
    }
    let _lock2 = singleton::acquire(&db_path).expect("after drop, re-acquire must succeed");
}

#[test]
fn lock_path_appends_pid_suffix_and_pid_is_readable_after_release() {
    // While the lock is held, Windows refuses concurrent reads on the file
    // (advisory exclusive lock blocks foreign handles).  Drop the lock first
    // and then verify the file is sane.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("subdir/with/nested/z.sqlite3");
    let expected_lock = db_path.with_file_name("z.sqlite3.pid");
    let actual_lock_path = {
        let lock = singleton::acquire(&db_path).expect("acquire must mkdir -p");
        assert_eq!(
            lock.lock_path, expected_lock,
            "lock path must APPEND .pid (not replace extension)"
        );
        assert!(lock.lock_path.exists());
        lock.lock_path.clone()
    };

    let pid_text = std::fs::read_to_string(&actual_lock_path).unwrap();
    let pid: u32 = pid_text
        .trim()
        .parse()
        .expect("pid file must contain numeric PID");
    assert_eq!(pid, std::process::id());
}

#[test]
fn locks_are_isolated_for_different_db_extensions_with_same_stem() {
    // `prro.sqlite3` and `prro.db` (same stem, different extension) MUST get
    // distinct lock files.  With_extension("pid") would collapse both onto
    // `prro.pid` and over-lock unrelated databases.
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("prro.sqlite3");
    let b = dir.path().join("prro.db");
    let lock_a = singleton::acquire(&a).expect("first lock on .sqlite3");
    let lock_b = singleton::acquire(&b).expect("second lock on .db must not collide");
    assert_ne!(lock_a.lock_path, lock_b.lock_path);
    assert!(lock_a.lock_path.ends_with("prro.sqlite3.pid"));
    assert!(lock_b.lock_path.ends_with("prro.db.pid"));
}
