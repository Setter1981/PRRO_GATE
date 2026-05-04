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
    let lock_path = db_path.with_extension("pid");
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
    // Best-effort PID write so operators can `cat` the file to find the holder.
    // Truncate first so a leftover PID from a prior run does not appear after ours.
    let _ = file.set_len(0);
    let mut writer = &file;
    let _ = writer.write_all(std::process::id().to_string().as_bytes());
    Ok(PidLock {
        _file: file,
        lock_path,
    })
}
