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
fn lock_path_uses_pid_extension_next_to_db_and_pid_is_readable_after_release() {
    // While the lock is held, Windows refuses concurrent reads on the file
    // (advisory exclusive lock blocks foreign handles).  Drop the lock first
    // and then verify the file is sane.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("subdir/with/nested/z.db");
    let expected_lock_path = {
        let lock = singleton::acquire(&db_path).expect("acquire must mkdir -p");
        assert_eq!(lock.lock_path, db_path.with_extension("pid"));
        assert!(lock.lock_path.exists());
        lock.lock_path.clone()
        // lock drops here -> file is unlocked but path persists
    };

    let pid_text = std::fs::read_to_string(&expected_lock_path).unwrap();
    let pid: u32 = pid_text
        .trim()
        .parse()
        .expect("pid file must contain numeric PID");
    assert_eq!(pid, std::process::id());
}
