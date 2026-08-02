//! bd `PRRO_GATE-7m8` — the assumption that makes two fail-closed refusals unreachable.
//!
//! Routing the admin CLI through `operator_completion::complete_operator_resolution` (bd
//! `PRRO_GATE-k3y`, #369) gave the operator path two refusals it did not have before:
//!
//!   * `ShiftProjectionFailed` — `apply_shift_transition`'s CAS did not match, i.e. `shifts.state`
//!     is not the `from` the §3.4 cell expects;
//!   * `ShiftMissing` — a shift-family document with a NULL `shift_id`.
//!
//! Both roll the whole `BEGIN IMMEDIATE` back, which is the right posture: issuing while our view
//! of the shift is divergent is precisely what k3y was. But for an operator a rolled-back
//! completion means a HELD reservation they cannot clear, and this codebase has fought that shape
//! before — `b5_resolve_on_blocked_node_completes_and_stays_blocked` exists because a completion
//! that could not be reached was an eternal brick.
//!
//! ## Why no escape hatch was added
//!
//! Because the state is not constructible. While a completable shift-family hold rests, NOTHING in
//! production can move `shifts.state`:
//!
//!   1. `shifts::transition_state` — the whitelisted CAS — is driven by the write path, which the
//!      FN fence blocks while a reservation is unresolved, under the per-FN single-writer lease;
//!   2. `boot_phase` branch (e2) `force_orphan_shift_to_error` is gated on `mode == Online`, and a
//!      held reservation means `STOP_MODE` (or `BLOCKED`) — branch (f) returns before (e2) is even
//!      evaluated;
//!   3. the operator force seams `force_to_error_with_audit` /
//!      `force_to_manual_reconciliation_with_audit` DO permit `Opening -> Error`, which is exactly
//!      the wedge — but **they have no production caller at all.**
//!
//! (3) is the load-bearing one, it is the only one that is a fact about wiring rather than about
//! logic, and it is the one nothing was checking. Hence this test.
//!
//! ## What happens when this goes RED
//!
//! It means someone wired a force seam into a production path — `prro doctor --repair` is in the
//! backlog and is the obvious candidate. At that moment the wedge becomes real: an operator forces
//! a shift while a shift-family completion is pending, and the hold can never be resolved. Do NOT
//! silence this test. Answer 7m8 then, with the escape hatch the new surface requires — the
//! decision is only answerable once the surface exists, which is why it was deferred rather than
//! guessed.

use std::path::{Path, PathBuf};

/// The two operator-driven force seams (spec §4.4: one of the two sanctioned entry surfaces for
/// `Error`, the other being boot's orphan recovery).
const SEAMS: [&str; 2] = [
    "force_to_error_with_audit",
    "force_to_manual_reconciliation_with_audit",
];

/// The seams live here; their own definitions and doc-comments are not callers.
const DEFINITION_FILE: &str = "db/repositories/shifts.rs";

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// A line that MENTIONS a seam without CALLING it: a comment, or a doc-comment. Both are common
/// here on purpose — `boot_phase.rs` and `sent_not_found.rs` each explain at length why they do
/// NOT use these seams, and those explanations must not read as callers.
fn is_prose(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}

#[test]
fn operator_force_seams_have_no_production_caller() {
    let root = src_root();
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    assert!(
        files.len() > 50,
        "sanity: expected to walk the whole prro/src tree, found only {} files — the walker is \
         broken and this test would pass vacuously",
        files.len()
    );

    let mut callers: Vec<String> = Vec::new();
    let mut definitions_seen = [0usize; SEAMS.len()];
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .expect("path under src")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(path).expect("read source file");
        for (i, line) in text.lines().enumerate() {
            if is_prose(line) {
                continue;
            }
            for (si, seam) in SEAMS.iter().enumerate() {
                if !line.contains(seam) {
                    continue;
                }
                if rel == DEFINITION_FILE {
                    definitions_seen[si] += 1;
                    continue;
                }
                callers.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }

    // Guard against the reverse failure: if the seams were renamed or deleted, every `contains`
    // above is false and this test passes while proving nothing.
    // PER SEAM, not a total — and that distinction is not pedantry. The first cut summed the
    // counts, so renaming ONE seam left the other's mentions covering for it and the canary stayed
    // green: the guard against vacuity was itself vacuous.
    for (si, seam) in SEAMS.iter().enumerate() {
        assert!(
            definitions_seen[si] >= 1,
            "bd PRRO_GATE-7m8: no non-prose mention of `{seam}` in {DEFINITION_FILE} — renamed or \
             removed? Every `contains` for it is then false and this test proves NOTHING about it, \
             which is the exact failure mode it exists to avoid."
        );
    }

    assert!(
        callers.is_empty(),
        "bd PRRO_GATE-7m8: an operator force seam now has a PRODUCTION caller:\n  {}\n\n\
         That makes `shifts.state` movable while a shift-family reservation is held, which makes \
         `OperatorCompletionError::ShiftProjectionFailed` REACHABLE — and a reached refusal rolls \
         the completion back, leaving the operator a hold they cannot clear (the eternal-brick \
         shape `b5_resolve_on_blocked_node_completes_and_stays_blocked` was written against).\n\n\
         Do NOT silence this test. It is the trigger to answer bd PRRO_GATE-7m8 — what the escape \
         hatch should be — which was deferred precisely because it is unanswerable until a surface \
         like yours exists.",
        callers.join("\n  ")
    );
}
