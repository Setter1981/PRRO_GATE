//! Cross-platform exclusive process lock backed by an OS file lock.
//!
//! Used by maintenance CLI (`serve`, `migrate`, `doctor`, future
//! `db-backup`) to ensure at most one prro process operates on a given
//! DB file at a time.  Live admin CLI (`fn add`, `shift open`, …) does
//! NOT call this — it talks to the running daemon over loopback HTTP.
//!
//! The lock is released automatically when the returned `PidLock` is
//! dropped (file descriptor closed; OS releases the advisory lock).

use anyhow::{anyhow, Context};
use fs4::fs_std::FileExt;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct PidLock {
    /// Hold the file handle for the lock's lifetime.  Drop releases.
    _file: File,
    pub lock_path: PathBuf,
}

pub fn acquire(db_path: &Path) -> anyhow::Result<PidLock> {
    // Append `.pid` rather than replacing the extension via
    // `with_extension("pid")` — otherwise `prro.sqlite3` and any sibling
    // `prro.<other-ext>` would collapse onto the same `prro.pid` lock and
    // over-lock distinct databases that happen to share a file stem.
    let mut s: OsString = db_path.as_os_str().to_owned();
    s.push(".pid");
    let lock_path: PathBuf = s.into();
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating lock dir {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening lock file {}", lock_path.display()))?;
    file.try_lock_exclusive().map_err(|_| {
        anyhow!(
            "another prro process is already running (lock at {})",
            lock_path.display()
        )
    })?;
    // Truncate stale PID from a prior run, then write ours.  Failure here
    // means the lock IS held but the PID file is unhelpful for diagnostics —
    // surface the error rather than silently leaving an empty/stale file.
    file.set_len(0)
        .with_context(|| format!("truncating lock file {}", lock_path.display()))?;
    let mut writer = &file;
    writer
        .write_all(std::process::id().to_string().as_bytes())
        .with_context(|| format!("writing PID to lock file {}", lock_path.display()))?;
    Ok(PidLock {
        _file: file,
        lock_path,
    })
}
