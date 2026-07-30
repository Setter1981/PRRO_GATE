//! Ordering-totality gate — every `ORDER BY … LIMIT 1` must have a DETERMINISTIC winner.
//!
//! **Why this gate exists.** Two defects of the same class landed in one slice
//! (bd `PRRO_GATE-hpc`), both passed design review, and both were caught only by a
//! test that constructed the tie deliberately:
//!
//! 1. `(lnd_at_write, created_at)` is not a total order — a T=112 replenish allocates
//!    no `lnd` and `datetime('now')` is second-granular, so two replenishes inside one
//!    second tie on BOTH keys. The projection returned the EARLIER witness while
//!    `node_state` held the later, so an NC-03 boot would have recovered the WRONG seed.
//! 2. A consumer carrying its own running state did not inherit the shared fix.
//!
//! `ORDER BY … LIMIT 1` over a partial order is not a selection — it is whatever the
//! query plan happens to return. This gate makes the author state, at the call site,
//! WHY the last ordering key breaks all ties within that query's `WHERE` scope.
//!
//! **The contract.** For every Rust string literal under `src/` that contains both
//! `ORDER BY` and `LIMIT 1`, one of:
//!   - the final ordering key is `rowid` (SQLite's implicit rowid is unique and, on an
//!     append-only table, monotonic — provably total, no prose needed); or
//!   - a `// ordering-justified: …` comment appears within
//!     [`JUSTIFY_WINDOW_LINES`] lines above the literal, stating why the key is unique
//!     inside the query's scope (e.g. "ux_fd_fn_lnd(fiscal_number, lnd) + WHERE
//!     fiscal_number = ?").
//!
//! The marker is a RUST comment on purpose: our SQL literals are joined into ONE
//! logical line by `\` continuations, so a `--` SQL comment inside the string would
//! swallow the rest of the query.
//!
//! Non-vacuity: the scan asserts it still finds at least [`MIN_EXPECTED_HITS`]
//! `ORDER BY … LIMIT 1` literals, so a formatting refactor that defeats the parser
//! fails LOUDLY instead of silently gating nothing.

use std::fs;
use std::path::{Path, PathBuf};

/// How far above the literal a `// ordering-justified:` marker may sit.
const JUSTIFY_WINDOW_LINES: usize = 30;

/// Non-vacuity floor — the number of `ORDER BY … LIMIT 1` literals present when this
/// gate landed. A drop below this means the parser stopped seeing queries.
const MIN_EXPECTED_HITS: usize = 15;

/// The marker an author writes to justify a non-`rowid` final ordering key.
const MARKER: &str = "ordering-justified:";

fn rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Byte offset → 0-based line index.
fn line_of(src: &str, byte: usize) -> usize {
    src[..byte].bytes().filter(|b| *b == b'\n').count()
}

/// Every `LIMIT 1` in a literal, paired with the final key of the `ORDER BY` that
/// PRECEDES it.  Pairing matters: a nested query has several `ORDER BY`s, and the one
/// that governs a given `LIMIT 1` is the nearest one above it — not the last one in
/// the string.  A `LIMIT 1` with no `ORDER BY` before it is deterministic by
/// definition only if the WHERE is unique, which is out of scope for this gate; it is
/// not reported (there is no ordering to justify).
fn ordered_limit_keys(literal: &str) -> Vec<String> {
    let lower = literal.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find("limit 1") {
        let limit_at = from + rel;
        from = limit_at + "limit 1".len();
        let Some(ob) = lower[..limit_at].rfind("order by") else {
            continue; // no ordering governs this LIMIT 1
        };
        let clause = &literal[ob + "order by".len()..limit_at];
        let Some(last) = clause.split(',').next_back() else {
            continue;
        };
        let cleaned = last
            .replace(['\\', '\n'], " ")
            .to_ascii_lowercase()
            .replace(" desc", " ")
            .replace(" asc", " ")
            .trim()
            .to_string();
        out.push(
            cleaned
                .rsplit('.')
                .next()
                .unwrap_or(&cleaned)
                .trim()
                .to_string(),
        );
    }
    out
}

/// Extract Rust string literals (handles `\"` and `\`-newline continuations) with
/// their byte offsets.
fn string_literals(src: &str) -> Vec<(usize, String)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'"' {
            let start = i;
            i += 1;
            let content_start = i;
            while i < b.len() {
                match b[i] {
                    b'\\' => i += 2,
                    b'"' => break,
                    _ => i += 1,
                }
            }
            if i <= b.len() && i > content_start {
                if let Some(text) = src.get(content_start..i.min(src.len())) {
                    out.push((start, text.to_string()));
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn every_order_by_limit_one_has_a_deterministic_winner() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&src_root, &mut files);
    files.sort();

    let mut hits = 0usize;
    let mut violations: Vec<String> = Vec::new();

    for path in &files {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        for (offset, lit) in string_literals(&src) {
            let keys = ordered_limit_keys(&lit);
            if keys.is_empty() {
                continue;
            }
            hits += keys.len();
            let line = line_of(&src, offset);
            for key in keys {
                if key == "rowid" {
                    continue; // provably unique — no prose required
                }
                let from = line.saturating_sub(JUSTIFY_WINDOW_LINES);
                let justified = lines[from..=line.min(lines.len() - 1)]
                    .iter()
                    .any(|l| l.contains(MARKER));
                if !justified {
                    let rel = path
                        .strip_prefix(&src_root)
                        .unwrap_or(path)
                        .display()
                        .to_string();
                    violations.push(format!(
                        "{rel}:{} — `ORDER BY … {key} … LIMIT 1` has no `// {MARKER}` within \
                         {JUSTIFY_WINDOW_LINES} lines above. State why `{key}` breaks every tie \
                         inside this query's WHERE scope (name the UNIQUE index / allocator), or \
                         add a provably-unique final key such as `rowid`.",
                        line + 1
                    ));
                }
            }
        }
    }

    assert!(
        hits >= MIN_EXPECTED_HITS,
        "ordering gate scanned only {hits} `ORDER BY … LIMIT 1` literals (expected >= \
         {MIN_EXPECTED_HITS}) — the parser has stopped seeing queries, so this gate is \
         vacuous. Fix the scanner before lowering the floor."
    );
    assert!(
        violations.is_empty(),
        "ordering-totality gate — {} unjustified `ORDER BY … LIMIT 1` \
         selection(s).\n\n{}\n\nSee .claude/skills/safe-write-path-change/SKILL.md \
         (Ordering / totality checklist).",
        violations.len(),
        violations.join("\n")
    );
}
