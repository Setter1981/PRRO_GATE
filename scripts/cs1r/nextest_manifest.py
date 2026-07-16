#!/usr/bin/env python3
"""CS-1R R1.2 — normalize `cargo nextest list --message-format json` into the
canonical forward-inventory manifest.

Identity row = TAB-separated `{profile}\t{package}\t{target}\t{test_name}\t{ignored}`.
**profile is part of identity** (spec §4 R1.2): moving a test between profiles is a
delete+add, not a no-op. `target` is the nextest `binary-id` (worktree-independent);
`package` is `package-name`; `ignored` is the per-testcase bool. Output is SORTED
and DETERMINISTIC (no absolute paths, no build hashes), so the committed manifest is
a stable diffable artifact and `live == committed` is an exact string equality.

Usage:
    nextest_manifest.py <profile-label>  < nextest-list.json  > manifest.tsv

The profile label is opaque to this script (it is written verbatim into column 1);
the caller pairs it with the matching `cargo nextest list` feature command.
"""
import json
import sys


def normalize(profile: str, data: dict) -> list[str]:
    rows = []
    for _suite_key, suite in data.get("rust-suites", {}).items():
        package = suite["package-name"]
        target = suite["binary-id"]  # stable, worktree-independent target id
        for test_name, tc in suite.get("testcases", {}).items():
            ignored = bool(tc.get("ignored", False))
            rows.append(f"{profile}\t{package}\t{target}\t{test_name}\t{str(ignored).lower()}")
    # Deterministic order: the whole row string sorts stably.
    rows.sort()
    return rows


def main() -> int:
    if len(sys.argv) != 2:
        sys.stderr.write("usage: nextest_manifest.py <profile-label> < json > tsv\n")
        return 2
    profile = sys.argv[1]
    data = json.load(sys.stdin)
    for row in normalize(profile, data):
        print(row)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
