#!/usr/bin/env python3
"""CS-1R3 A1 — the SINGLE source of truth for "is this changed file rust-relevant?".

## Why this file exists (the round-3 bypass it closes)

The `detect rust changes` step in `.github/workflows/rust-prro.yml` decides whether
the REQUIRED `x86_64-unknown-linux-gnu` (`build`) job runs. GitHub treats a SKIPPED
required check as SATISFIED — so if the detector fails to cover a path the `build`
job actually gates, a PR touching only that path SKIPS the required context and
merges green-but-unsound (the empirical #305 bypass).

CS-1R2 tried to guard this with a consistency gate that TEXT-SEARCHED the YAML for
the detector's `grep -qE '^( … )'` regex. That gate was ITSELF bypassable: it took
the FIRST `grep -qE` match INCLUDING COMMENTS, so a WIDE reference regex in a YAML
comment above a NARROWED real detector line fooled it green (round-3 A1). Text-
parsing a gate's own definition is unsound.

## The fix (structural, no YAML text-parsing)

This module OWNS the canonical prefix list (`CANONICAL_PREFIXES`) as executable
Python data. There is exactly ONE detector:

  * CI (`.github/workflows/rust-prro.yml` `changes` job) pipes the PR's changed-file
    list to `python3 scripts/cs1r/rust_change_paths.py detect` — which prints
    `rust=true` / `rust=false` using THIS list. No inline `grep -qE`, so there is no
    comment line for an attacker to inject a wide regex into.
  * The consistency gate (`manifest_detector_paths.py`) IMPORTS `CANONICAL_PREFIXES`
    from this module (not parsed from YAML) and asserts it ⊇ every workspace-member
    dir + every gate-input path.
  * The push-paths gate (`manifest_detector_paths.py`) STRUCTURALLY parses
    `on.push.paths` (PyYAML) and asserts every canonical prefix is covered.

Pure stdlib (`re`, `sys`) — the CI `changes` job has only bash + `actions/checkout`,
no pip. Keep it dependency-free.

## Usage
    # detector (CI): read NUL- or newline-separated changed files on stdin
    git diff --name-only BASE HEAD | python3 scripts/cs1r/rust_change_paths.py detect
    #   prints `rust=true` or `rust=false`; exit 0

    python3 scripts/cs1r/rust_change_paths.py print   # dump the canonical prefixes
"""
from __future__ import annotations

import re
import sys

# ─────────────────────────────────────────────────────────────────────────────
# THE canonical rust-change prefix list. A changed file is rust-relevant iff its
# repo-relative path starts with one of these prefixes (anchored `^`).
#
# INVARIANT (enforced by manifest_detector_paths.py): this list must cover
#   (a) every workspace-member directory (so a crate-local change builds/tests),
#   (b) every non-crate gate-input file the `build` job reads (scripts + manifests),
#   (c) the workflow file itself.
# Adding a workspace member or a new gate input WITHOUT extending this list turns
# the consistency gate RED — the drift cannot merge silently.
#
# We use a small set of BROAD prefixes (`rust/`, `scripts/cs1r/`,
# `docs/cs1r/inventory/`) rather than one-prefix-per-crate: `rust/` already covers
# every current AND future workspace member, so a new crate cannot open a gap.
# ─────────────────────────────────────────────────────────────────────────────
CANONICAL_PREFIXES: list[str] = [
    # All 13 (and any future) workspace members live under rust/ and are built +
    # tested on the required x86 leg — one broad prefix, no per-crate gap.
    "rust/",
    # The CS-1R inventory gate's executable inputs (this file + its siblings).
    "scripts/cs1r/",
    # The committed inventory manifests the gate diffs against.
    "docs/cs1r/inventory/",
    # This workflow itself (a change to the gate wiring must re-run the gate).
    ".github/workflows/rust-prro.yml",
]


def is_rust_change(path: str) -> bool:
    """True iff `path` (repo-relative, forward slashes) is rust-relevant."""
    return any(path == p or path.startswith(p) for p in CANONICAL_PREFIXES)


def prefix_covers(prefix: str, probe: str) -> bool:
    """True iff a file at `probe` would be matched by canonical `prefix`.

    Used by the consistency gate: a workspace-member dir `rust/prro` is "covered"
    when a file under it (`rust/prro/Cargo.toml`) is a rust-change. Exact-file
    prefixes (the workflow path) match only themselves.
    """
    return probe == prefix or probe.startswith(prefix)


def _read_changed_files(stream) -> list[str]:
    """Read changed files from a stream: newline- or NUL-separated, tolerant of
    blank lines and CR. Returns forward-slash repo-relative paths."""
    data = stream.read()
    parts = re.split(r"[\0\n]", data)
    return [p.strip().replace("\\", "/") for p in parts if p.strip()]


def _cmd_detect() -> int:
    files = _read_changed_files(sys.stdin)
    rust = any(is_rust_change(f) for f in files)
    # Emit the exact `rust=<bool>` line the workflow appends to $GITHUB_OUTPUT.
    print(f"rust={'true' if rust else 'false'}")
    # Diagnostics to stderr so the CI log shows the decision without polluting the
    # single stdout line the workflow captures.
    matched = [f for f in files if is_rust_change(f)]
    sys.stderr.write(
        f"[rust_change_paths] {len(files)} changed file(s); "
        f"{len(matched)} rust-relevant → rust={'true' if rust else 'false'}\n"
    )
    return 0


def _cmd_print() -> int:
    for p in CANONICAL_PREFIXES:
        print(p)
    return 0


def main(argv: list[str]) -> int:
    cmd = argv[1] if len(argv) > 1 else "detect"
    if cmd == "detect":
        return _cmd_detect()
    if cmd == "print":
        return _cmd_print()
    sys.stderr.write(f"usage: {argv[0]} [detect|print]\n")
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
