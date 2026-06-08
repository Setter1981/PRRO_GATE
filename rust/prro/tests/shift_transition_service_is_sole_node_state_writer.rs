//! RS-3 C1 drift-guard — the shift transition-service is the SOLE writer
//! of the `node_state` shift projection, and (together with the `shifts`
//! repository) the only home for a `shifts.state` transition write.
//!
//! Per the pilot Q1-A′ decision (`shifts` WRITE-primary,
//! `node_state.shift_state` / `current_shift_id` READ-projection) the
//! projection MUST move in lock-step with the active shift row (m3b §5).
//! C1 hoisted the two ad-hoc dual-write paths (`backlog_drain`,
//! `boot_phase`) into `services::shift::transition`. This scanner fails
//! if a new raw shift-projection write (or a raw `shifts.state`
//! transition outside the repo primitive) creeps into any other module —
//! the bypass that C1 exists to prevent.
//!
//! Scope notes:
//! - It targets `UPDATE` statements only. `INSERT INTO node_state (...)`
//!   that seeds an initial `shift_state` on row creation (bootstrap /
//!   first-touch) is row creation, not a transition, and is allowed.
//! - `UPDATE node_state SET mode = ...` (NodeMode transitions) does not
//!   touch the shift projection and is allowed.

use std::fs;
use std::path::{Path, PathBuf};

/// Files permitted to write `node_state SET shift_state` (the projection
/// mirror / reset).
const PROJECTION_WRITER_ALLOWLIST: &[&str] = &["services/shift/transition.rs"];

/// Files permitted to write `UPDATE shifts SET state` (the primary
/// transition). The repository owns the whitelist-CAS + force seams; the
/// transition-service owns the boot orphan force-write.
const SHIFT_STATE_WRITER_ALLOWLIST: &[&str] =
    &["db/repositories/shifts.rs", "services/shift/transition.rs"];

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir src") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Returns the path rendered relative to `src/` with `/` separators, for
/// stable allowlist matching across platforms.
fn rel_src(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .expect("path under src")
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn node_state_shift_projection_is_written_only_by_the_transition_service() {
    let root = src_root();
    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);
    assert!(!files.is_empty(), "no .rs files found under src/");

    let mut violations: Vec<String> = Vec::new();
    for path in &files {
        let rel = rel_src(path, &root);
        let body = fs::read_to_string(path).expect("read src file");

        // (a) raw node_state shift-projection write.
        if body.contains("node_state SET shift_state")
            && !PROJECTION_WRITER_ALLOWLIST.contains(&rel.as_str())
        {
            violations.push(format!(
                "{rel}: writes `node_state SET shift_state` outside the transition-service \
                 (route it through services::shift::transition)"
            ));
        }

        // (b) raw shifts.state transition write.
        if body.contains("UPDATE shifts SET state")
            && !SHIFT_STATE_WRITER_ALLOWLIST.contains(&rel.as_str())
        {
            violations.push(format!(
                "{rel}: writes `UPDATE shifts SET state` outside the repository primitive / \
                 transition-service (use shifts::transition_state or the C1 service)"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "RS-3 C1 drift-guard found raw shift/projection writes outside the sanctioned homes:\n{}",
        violations.join("\n")
    );
}

/// Sanity: the allowlisted files actually DO contain the patterns, so a
/// future rename of the service module can't silently make the guard pass
/// by scanning nothing.
#[test]
fn allowlisted_writers_still_exist() {
    let root = src_root();

    let service = fs::read_to_string(root.join("services/shift/transition.rs"))
        .expect("transition-service module must exist");
    assert!(
        service.contains("node_state SET shift_state"),
        "the transition-service must own the projection write"
    );
    assert!(
        service.contains("UPDATE shifts SET state"),
        "the transition-service must own the boot orphan force-write"
    );

    let repo = fs::read_to_string(root.join("db/repositories/shifts.rs"))
        .expect("shifts repository must exist");
    assert!(
        repo.contains("UPDATE shifts SET state"),
        "the shifts repository must own the whitelist-CAS transition primitive"
    );
}
