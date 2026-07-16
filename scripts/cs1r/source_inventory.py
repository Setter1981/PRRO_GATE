#!/usr/bin/env python3
"""CS-1R R1.2 / R1.4 — source-test-file SHA inventory.

Emits a SORTED `sha256<TAB>repo-relative-path` manifest of every SOURCE test file
in scope (R1.4):
  * all `rust/**/tests/**/*.rs`
  * trybuild fixtures (`rust/**/tests/**/*.rs` under a `*compile_fail*` dir are
    already covered by the glob above; standalone `.rs` fixtures too)
  * all `rust/**/src/**/*.rs` carrying `#[test]`, `#[cfg(test)]`,
    `#[tokio::test]`, `#[rstest]`, `proptest!`, or a `#[path]` test module.

The gate uses this two ways: (1) a new test file must appear here in the same PR
(so a new test can't be added-then-silently-deleted), and (2) it is committed as a
stable diffable record. Run from the repo root.

Usage:
    source_inventory.py            # print manifest to stdout
    source_inventory.py --check  <committed.tsv>   # exit 1 if live != committed
"""
import hashlib
import os
import re
import sys

TEST_MARKERS = re.compile(
    r"#\[test\]|#\[cfg\(test\)\]|#\[tokio::test\]|#\[rstest\]|"
    r"proptest!|#\[path\s*=|#\[cfg\(feature\s*=\s*\"live-dps\"\)\]"
)


def is_src_test_file(path: str) -> bool:
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return bool(TEST_MARKERS.search(f.read()))
    except OSError:
        return False


def collect(root: str) -> list[tuple[str, str]]:
    rust = os.path.join(root, "rust")
    rows = []
    for dirpath, dirnames, filenames in os.walk(rust):
        # skip build output dirs
        dirnames[:] = [d for d in dirnames if d != "target"]
        rel_dir = os.path.relpath(dirpath, root)
        parts = rel_dir.split(os.sep)
        in_tests = "tests" in parts
        in_src = "src" in parts
        for fn in filenames:
            if not fn.endswith(".rs"):
                continue
            abs_path = os.path.join(dirpath, fn)
            rel = os.path.relpath(abs_path, root).replace(os.sep, "/")
            take = False
            if in_tests:
                take = True  # every .rs under a tests/ tree (incl. trybuild fixtures)
            elif in_src and is_src_test_file(abs_path):
                take = True
            if take:
                with open(abs_path, "rb") as f:
                    sha = hashlib.sha256(f.read()).hexdigest()
                rows.append((sha, rel))
    rows.sort(key=lambda r: r[1])
    return rows


def main() -> int:
    root = os.getcwd()
    rows = collect(root)
    lines = [f"{sha}\t{path}" for sha, path in rows]
    if len(sys.argv) >= 2 and sys.argv[1] == "--check":
        committed_path = sys.argv[2]
        with open(committed_path) as f:
            committed = f.read().splitlines()
        live = lines
        # R1.2 control (3): every NEW source test file (live path not in committed
        # paths) must be recorded in the manifest in THIS PR. We enforce that the
        # PATH SET is additions-only-or-equal is handled by the git-diff gate; here
        # we enforce the SHA of each committed path still matches (drift check) and
        # that no committed path VANISHED (a delete must be an explicit manifest
        # edit, caught by the diff gate).
        committed_paths = {ln.split("\t", 1)[1]: ln.split("\t", 1)[0] for ln in committed}
        live_paths = {ln.split("\t", 1)[1]: ln.split("\t", 1)[0] for ln in live}
        failures = []
        for path, sha in committed_paths.items():
            if path not in live_paths:
                failures.append(f"committed source test file VANISHED (not in tree): {path}")
            elif live_paths[path] != sha:
                failures.append(f"source test file CONTENT drifted (sha mismatch): {path}")
        for path in live_paths:
            if path not in committed_paths:
                failures.append(
                    f"NEW source test file not in committed manifest — add it in this PR: {path}"
                )
        if failures:
            sys.stderr.write("SOURCE INVENTORY GATE FAILED:\n" + "\n".join(failures) + "\n")
            return 1
        sys.stderr.write(f"source inventory OK ({len(live)} files)\n")
        return 0
    for ln in lines:
        print(ln)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
