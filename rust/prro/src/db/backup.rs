//! RS-4 (audit pass-2, item 4) — production backup / retention.
//!
//! Snapshots are taken with **`VACUUM INTO`** (one statement, through the live
//! pool): SQLite walks the live DB + WAL and writes a compact, consistent,
//! defragmented copy WITHOUT stopping writers — so the backup loop never blocks
//! the fiscal path. Each snapshot is then made owner-only (`0o600`, the same
//! posture [`crate::db::set_owner_only_perms`] applies to the live secure DB)
//! and **verified** (`PRAGMA integrity_check` over a fresh read-only open); a
//! snapshot that fails verification is deleted and the call errors.
//!
//! No `with_immediate` and no long write-lock: `VACUUM INTO` is a read-side
//! operation against the source (invariant #1 untouched).
//!
//! Retention ([`prune`]) operates purely by filename convention
//! (`<label>-YYYYMMDD-HHMMSS-<8hex>.db`) — it never touches a file that does
//! not match the pattern (foreign files in the directory are not ours).  A
//! snapshot is RETAINED only while it is BOTH within the `keep_last`
//! most-recent AND younger than `max_age_days` (architect review ruling:
//! `keep_last` is the disk-usage CAP, `max_age_days` an additional expiry —
//! the original AND-delete reading would retain `interval × max_age` full DB
//! copies, exhausting edge-hardware disks).  Deleting by age is safe: the
//! supervisor prunes only AFTER a successful snapshot, so a fresh copy always
//! survives the pass.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// Outcome of a successful [`snapshot`].
#[derive(Debug, Clone)]
pub struct SnapshotReport {
    /// Final path of the verified snapshot.
    pub path: PathBuf,
    /// Size of the snapshot file in bytes.
    pub bytes: u64,
    /// Wall-clock duration of the `VACUUM INTO` statement (ms).
    pub duration_ms: u64,
    /// Always `true` on a returned report — a failed verification errors.
    pub verified: bool,
}

/// Outcome of a [`prune`] pass over one label's snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Snapshots matching the `<label>-…` pattern found this pass.
    pub scanned: usize,
    /// Snapshots deleted (beyond `keep_last` OR older than `max_age_days`).
    pub deleted: usize,
    /// Snapshots retained.
    pub kept: usize,
}

/// Take one verified, owner-only snapshot of the pool's database into
/// `backup_dir`, named `<label>-YYYYMMDD-HHMMSS-<8hex>.db`.
///
/// `live_db_path` is the source DB path (used for the same-device advisory at
/// the loop level and for log context); the `VACUUM INTO` itself snapshots
/// whatever database `pool` is connected to.
///
/// Sequence (spec A1): `VACUUM INTO '<tmp>'` (tmp in the SAME directory, so the
/// subsequent `rename` is intra-device and Windows-safe) → `fs::rename` to the
/// unique final name → owner-only perms → `PRAGMA integrity_check` → on
/// not-`ok`, delete the file and error.
pub async fn snapshot(
    pool: &SqlitePool,
    live_db_path: &Path,
    backup_dir: &Path,
    label: &str,
) -> anyhow::Result<SnapshotReport> {
    let now = chrono::Utc::now();
    let stamp = now.format("%Y%m%d-%H%M%S").to_string();
    // 8 hex from the random tail of a v7 UUID — disambiguates snapshots taken
    // within the same second so we NEVER overwrite an existing file.
    let uuid = uuid::Uuid::now_v7().simple().to_string();
    let rand8 = &uuid[uuid.len() - 8..];
    let final_name = format!("{label}-{stamp}-{rand8}.db");
    let final_path = backup_dir.join(&final_name);
    let tmp_path = backup_dir.join(format!("{final_name}.tmp"));

    // `VACUUM INTO` takes a string-literal filename; embed the tmp path with
    // single-quote escaping (the path is composed from our own label + stamp +
    // hex + operator-configured dir — no user-controlled SQL).
    let tmp_sql = tmp_path.display().to_string().replace('\'', "''");
    let started = Instant::now();
    sqlx::query(&format!("VACUUM INTO '{tmp_sql}'"))
        .execute(pool)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "VACUUM INTO failed for {label} (src {}, dst {}): {e}",
                live_db_path.display(),
                tmp_path.display()
            )
        })?;
    let duration_ms = started.elapsed().as_millis() as u64;

    // Intra-directory rename to the unique final name (atomic on the same
    // device; avoids a half-written file ever carrying the final name).
    std::fs::rename(&tmp_path, &final_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        anyhow::anyhow!("rename {tmp_path:?} -> {final_path:?} failed: {e}")
    })?;

    // Owner-only posture (HIGH-AUDIT-01 parity).
    crate::db::set_owner_only_perms(&final_path)?;

    // Mandatory verification — a snapshot that can't be opened or fails
    // integrity_check is worse than useless; delete it and fail.
    if !verify_snapshot(&final_path).await? {
        let _ = std::fs::remove_file(&final_path);
        anyhow::bail!(
            "snapshot {final_path:?} failed verification (PRAGMA integrity_check != ok); deleted"
        );
    }

    let bytes = std::fs::metadata(&final_path)?.len();
    Ok(SnapshotReport {
        path: final_path,
        bytes,
        duration_ms,
        verified: true,
    })
}

/// One backup pass for a single `(pool, label)`: take a snapshot, then prune
/// retention.  **Errors are logged (WARN) and swallowed** — a backup failure
/// must NEVER fail the supervisor loop or block the fiscal path (F1).  Returns
/// the snapshot report on success, or `None` if the snapshot itself failed
/// (already logged); a prune failure after a good snapshot is logged but still
/// returns the snapshot.  This is the unit the supervisor's backup tick calls
/// once per DB file (main + secure).
pub async fn snapshot_and_prune(
    pool: &SqlitePool,
    live_db_path: &Path,
    backup_dir: &Path,
    label: &str,
    keep_last: usize,
    max_age_days: u64,
) -> Option<SnapshotReport> {
    match snapshot(pool, live_db_path, backup_dir, label).await {
        Ok(report) => {
            if let Err(e) = prune(backup_dir, label, keep_last, max_age_days) {
                tracing::warn!(
                    target: "prro::db::backup",
                    label,
                    error = ?e,
                    "backup prune failed (snapshot kept)"
                );
            }
            tracing::debug!(
                target: "prro::db::backup",
                label,
                path = %report.path.display(),
                bytes = report.bytes,
                duration_ms = report.duration_ms,
                "backup snapshot ok"
            );
            Some(report)
        }
        Err(e) => {
            tracing::warn!(
                target: "prro::db::backup",
                label,
                dir = %backup_dir.display(),
                error = ?e,
                "backup snapshot failed; skipped this tick"
            );
            None
        }
    }
}

/// Verify a snapshot file: open it READ-ONLY (`mode=ro`, so the open never
/// mutates it — e.g. by converting journal mode to WAL) and run
/// `PRAGMA integrity_check`.  Returns `Ok(true)` iff it opens and reports
/// `"ok"`; a file that cannot be opened as SQLite, or whose check is not
/// `"ok"`, returns `Ok(false)`.  Only a genuine query/IO error propagates.
pub async fn verify_snapshot(path: &Path) -> anyhow::Result<bool> {
    let url = format!("sqlite:{}?mode=ro", path.display());
    let opts = match SqliteConnectOptions::from_str(&url) {
        Ok(o) => o,
        Err(_) => return Ok(false),
    };
    let pool = match SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
    {
        Ok(p) => p,
        // Not a valid SQLite database / unreadable → verification fails.
        Err(_) => return Ok(false),
    };
    let result: Result<String, sqlx::Error> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await;
    pool.close().await;
    match result {
        Ok(s) => Ok(s == "ok"),
        Err(_) => Ok(false),
    }
}

/// Parse a snapshot filename of the form `<label>-YYYYMMDD-HHMMSS-<8hex>.db`,
/// returning its embedded UTC timestamp.  Returns `None` for any name that does
/// not match the pattern — those are foreign files [`prune`] must never touch.
fn parse_snapshot_stamp(name: &str, label: &str) -> Option<chrono::NaiveDateTime> {
    let rest = name.strip_prefix(&format!("{label}-"))?;
    let rest = rest.strip_suffix(".db")?;
    let parts: Vec<&str> = rest.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let (date, time, hex) = (parts[0], parts[1], parts[2]);
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if time.len() != 6 || !time.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if hex.len() != 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    chrono::NaiveDateTime::parse_from_str(&format!("{date}{time}"), "%Y%m%d%H%M%S").ok()
}

/// Prune `<label>` snapshots in `backup_dir`: a snapshot is kept only while it
/// is BOTH within the `keep_last` most-recent AND younger than `max_age_days`
/// (i.e. deleted when beyond the cap OR expired — see the module doc for the
/// architect ruling).  Never touches a file that does not match the snapshot
/// name pattern.  A missing directory is not an error (nothing to prune).
pub fn prune(
    backup_dir: &Path,
    label: &str,
    keep_last: usize,
    max_age_days: u64,
) -> anyhow::Result<PruneReport> {
    let entries = match std::fs::read_dir(backup_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(PruneReport::default()),
        Err(e) => return Err(e.into()),
    };

    // Collect OUR snapshots only (pattern match), newest first.
    let mut ours: Vec<(PathBuf, chrono::NaiveDateTime)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(stamp) = parse_snapshot_stamp(name, label) {
            ours.push((entry.path(), stamp));
        }
    }
    ours.sort_by_key(|b| std::cmp::Reverse(b.1));

    let now = chrono::Utc::now().naive_utc();
    let mut deleted = 0usize;
    for (idx, (path, stamp)) in ours.iter().enumerate() {
        let beyond_keep = idx >= keep_last;
        let age_days = (now - *stamp).num_days();
        let too_old = age_days > max_age_days as i64;
        if beyond_keep || too_old {
            std::fs::remove_file(path)
                .map_err(|e| anyhow::anyhow!("prune: remove {path:?} failed: {e}"))?;
            deleted += 1;
        }
    }
    let scanned = ours.len();
    Ok(PruneReport {
        scanned,
        deleted,
        kept: scanned - deleted,
    })
}
