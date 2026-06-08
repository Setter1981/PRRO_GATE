//! RS-3 C1 drift-guard — the shift transition-service is the SOLE writer
//! of the `node_state` shift projection (`shift_state` + `current_shift_id`),
//! and (together with the `shifts` repository) the only home for a
//! `shifts.state` transition write.
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
//! Robustness (external-review hardening): matching is **case-insensitive**
//! and **whitespace-normalized** (a `\`-continued multi-line SQL literal is
//! collapsed to one space before scanning), and uses an **anchored window**
//! so a projection column written in any position after `SET` — incl.
//! `current_shift_id`-first and `ON CONFLICT ... DO UPDATE SET` upserts — is
//! still caught, not just the exact happy-path spelling.
//!
//! Scope notes:
//! - It targets `UPDATE` / `DO UPDATE SET` statements. A plain `INSERT INTO
//!   node_state (...)` that seeds an initial `shift_state` on row creation
//!   is creation, not a transition, and is allowed.
//! - `UPDATE node_state SET mode = ...` (NodeMode transitions, no projection
//!   column in the window) is allowed.

use std::fs;
use std::path::{Path, PathBuf};

/// Files permitted to write the node_state shift projection
/// (`shift_state` / `current_shift_id`) via a bare `UPDATE`.
const PROJECTION_WRITER_ALLOWLIST: &[&str] = &["services/shift/transition.rs"];

/// Files permitted to write `UPDATE shifts SET state` (the primary
/// transition). The repository owns the whitelist-CAS + force seams; the
/// transition-service owns the boot orphan force-write.
const SHIFT_STATE_WRITER_ALLOWLIST: &[&str] =
    &["db/repositories/shifts.rs", "services/shift/transition.rs"];

/// Files permitted to write a node_state projection column via an
/// `INSERT ... ON CONFLICT ... DO UPDATE SET` upsert. The node_state
/// repository owns row bootstrap/first-touch; the service owns transitions.
const NODE_STATE_UPSERT_ALLOWLIST: &[&str] = &[
    "db/repositories/node_state.rs",
    "services/shift/transition.rs",
];

/// node_state projection columns (the read-projection of shift lifecycle).
const PROJECTION_COLUMNS: &[&str] = &["shift_state", "current_shift_id"];

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

/// Lowercase + collapse every run of ASCII whitespace (incl. the newline +
/// indentation of a `\`-continued SQL string literal) to a single space, so
/// neither case changes nor a write spread across source lines can evade the
/// scan.
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// True if, starting at any occurrence of `anchor`, the next `win` characters
/// contain any of `needles`. Char-based windowing (not byte slicing) so
/// non-ASCII comment text cannot cause a panic.
fn anchored_window_has(hay: &str, anchor: &str, win: usize, needles: &[&str]) -> bool {
    for (idx, _) in hay.match_indices(anchor) {
        let window: String = hay[idx..].chars().take(win).collect();
        if needles.iter().any(|n| window.contains(n)) {
            return true;
        }
    }
    false
}

/// Window after a `SET`-introducing anchor, large enough to span a multi-
/// column SET list so a projection column in any position is seen.
const SET_WINDOW: usize = 200;

#[test]
fn node_state_shift_projection_is_written_only_by_the_transition_service() {
    let root = src_root();
    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);
    assert!(!files.is_empty(), "no .rs files found under src/");

    let mut violations: Vec<String> = Vec::new();
    for path in &files {
        let rel = rel_src(path, &root);
        let body = normalize(&fs::read_to_string(path).expect("read src file"));

        // (a) bare UPDATE of the node_state projection (shift_state or
        // current_shift_id in any column position after SET).
        if anchored_window_has(
            &body,
            "update node_state set",
            SET_WINDOW,
            PROJECTION_COLUMNS,
        ) && !PROJECTION_WRITER_ALLOWLIST.contains(&rel.as_str())
        {
            violations.push(format!(
                "{rel}: raw `UPDATE node_state SET ...` touches a shift-projection column \
                 outside the transition-service (route it through services::shift::transition)"
            ));
        }

        // (b) ON CONFLICT ... DO UPDATE SET upsert touching the projection,
        // outside the node_state repo / service.
        if anchored_window_has(&body, "do update set", SET_WINDOW, PROJECTION_COLUMNS)
            && !NODE_STATE_UPSERT_ALLOWLIST.contains(&rel.as_str())
        {
            violations.push(format!(
                "{rel}: `ON CONFLICT ... DO UPDATE SET ...` touches a shift-projection column \
                 outside the node_state repository / transition-service"
            ));
        }

        // (c) raw shifts.state transition write.
        if anchored_window_has(&body, "update shifts set", SET_WINDOW, &["state"])
            && !SHIFT_STATE_WRITER_ALLOWLIST.contains(&rel.as_str())
        {
            violations.push(format!(
                "{rel}: raw `UPDATE shifts SET state ...` outside the repository primitive / \
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

    let service = normalize(
        &fs::read_to_string(root.join("services/shift/transition.rs"))
            .expect("transition-service module must exist"),
    );
    assert!(
        anchored_window_has(
            &service,
            "update node_state set",
            SET_WINDOW,
            PROJECTION_COLUMNS
        ),
        "the transition-service must own the projection write"
    );
    assert!(
        anchored_window_has(&service, "update shifts set", SET_WINDOW, &["state"]),
        "the transition-service must own the boot orphan force-write"
    );

    let repo = normalize(
        &fs::read_to_string(root.join("db/repositories/shifts.rs"))
            .expect("shifts repository must exist"),
    );
    assert!(
        anchored_window_has(&repo, "update shifts set", SET_WINDOW, &["state"]),
        "the shifts repository must own the whitelist-CAS transition primitive"
    );

    // The ON CONFLICT carve-out is load-bearing: node_state.rs really does
    // upsert a projection column, so a rename can't make check (b) vacuous.
    let ns_repo = normalize(
        &fs::read_to_string(root.join("db/repositories/node_state.rs"))
            .expect("node_state repository must exist"),
    );
    assert!(
        anchored_window_has(&ns_repo, "do update set", SET_WINDOW, PROJECTION_COLUMNS),
        "the node_state repository must own the bootstrap upsert of the projection"
    );
}
