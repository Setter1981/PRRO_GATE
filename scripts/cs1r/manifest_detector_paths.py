#!/usr/bin/env python3
"""CS-1R3 A1 — machine-consistency gate for the CI rust-change detector.

The `detect rust changes` step in `.github/workflows/rust-prro.yml` decides whether
the REQUIRED `x86_64-unknown-linux-gnu` (`build`) job runs. GitHub treats a SKIPPED
required check as SATISFIED — so a detector that misses a gated path lets a PR
touching only that path SKIP the required context and merge green-but-unsound
(the empirical #305 bypass).

## What changed in round 3 (why this file no longer text-parses YAML)

The CS-1R2 version of this gate extracted the detector regex by TEXT-SEARCHING the
YAML (`re.search(r"grep -qE '\\^\\((.*?)\\)'", …)`, first match INCLUDING comments).
That was itself bypassable: a WIDE reference regex in a YAML comment above a
NARROWED real detector line fooled it green (round-3 finding A1). A gate that parses
its own subject's text is unsound.

Now the canonical prefix list is EXECUTABLE Python data in
`scripts/cs1r/rust_change_paths.py` (imported here, NOT parsed from YAML), and the
CI `changes` job CALLS that module instead of an inline `grep -qE` — so there is no
comment line to inject a wide regex into.

## This gate asserts (all three RED on drift)
  1. **detector coverage** — `CANONICAL_PREFIXES` ⊇ every workspace-member dir
     ∪ every gate-input path the `build` job reads. A future crate or gate input
     added without matching coverage → RED (the #305 class).
  2. **push-paths sync (structural)** — `on.push.paths` in the workflow, parsed with
     PyYAML (NOT text-search), ⊇ every canonical prefix, so a push to `rust-gateway`
     that touches a gated path can never skip the trigger. RED when push.paths drops
     a crate/input the PR-detector covers (the round-3 push.paths-unsynced finding).
  3. **CI wires the module, not an inline grep** — the workflow's `changes` job must
     invoke `rust_change_paths.py detect` (proving the sound detector is the one CI
     actually runs), and must contain NO inline `grep -qE '^(' detector (proving the
     bypassable text form is gone).

Run from the repo root:
    scripts/cs1r/manifest_detector_paths.py            # verify, exit 1 on gap
Requires: cargo (workspace metadata), python3, git; PyYAML for the push-paths leg
(auto-installed if missing).
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys

# Import the canonical list from the executable detector module — the SINGLE
# source of truth. (Sibling import: add this dir to sys.path.)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rust_change_paths import CANONICAL_PREFIXES, prefix_covers  # noqa: E402

WORKFLOW = ".github/workflows/rust-prro.yml"
DETECTOR_MODULE_INVOCATION = "rust_change_paths.py detect"

# Non-crate paths the `build` job reads as gate inputs (must be covered by the
# canonical prefixes, else a change to just these SKIPS the required check). These
# mirror the scripts/manifests the inventory gate step and the provenance tests
# consume. Add here whenever the build job grows a new file input.
GATE_INPUT_PATHS = [
    "scripts/cs1r/inventory_gate.sh",
    "scripts/cs1r/mint_manifests.sh",
    "scripts/cs1r/nextest_manifest.py",
    "scripts/cs1r/source_inventory.py",
    "scripts/cs1r/manifest_detector_paths.py",
    "scripts/cs1r/rust_change_paths.py",
    "docs/cs1r/inventory/manifest.test-support.tsv",
    "docs/cs1r/inventory/manifest.live-dps.tsv",
    "docs/cs1r/inventory/source_files.sha256",
]


def cargo() -> str:
    return os.environ.get("CARGO", "cargo")


def repo_root() -> str:
    return subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()


def workspace_member_dirs(root: str) -> list[str]:
    out = subprocess.run(
        [cargo(), "metadata", "--no-deps", "--format-version", "1"],
        cwd=os.path.join(root, "rust"),
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    meta = json.loads(out)
    dirs = []
    for pkg in meta["packages"]:
        crate_dir = os.path.dirname(pkg["manifest_path"])
        rel = os.path.relpath(crate_dir, root).replace(os.sep, "/")
        dirs.append(f"{rel}/Cargo.toml")
    return sorted(dirs)


def covered(probe: str) -> bool:
    """True iff any canonical prefix would match a file at `probe`."""
    return any(prefix_covers(p, probe) for p in CANONICAL_PREFIXES)


# ── leg 1: detector coverage ────────────────────────────────────────────────
def check_detector_coverage(root: str) -> list[str]:
    probes: list[tuple[str, str]] = []
    for d in workspace_member_dirs(root):
        probes.append(("workspace-member", d))
        crate = d.rsplit("/", 1)[0]
        probes.append(("workspace-member-src", f"{crate}/src/lib.rs"))
    for p in GATE_INPUT_PATHS:
        probes.append(("gate-input", p))

    fails = []
    for kind, p in probes:
        if not covered(p):
            fails.append(
                f"[detector-coverage] {kind} `{p}` is NOT covered by any canonical "
                f"prefix → a PR touching only it SKIPS the required x86 context."
            )
    if not fails:
        sys.stderr.write(
            f"OK detector coverage — CANONICAL_PREFIXES covers {len(probes)} gated "
            f"paths (members + gate inputs)\n"
        )
    return fails


# ── leg 2: push.paths sync (structural YAML parse) ──────────────────────────
def _load_yaml(text: str):
    try:
        import yaml  # type: ignore
    except ImportError:
        subprocess.run(
            [sys.executable, "-m", "pip", "install", "--quiet", "pyyaml"],
            check=True,
        )
        import yaml  # type: ignore
    return yaml.safe_load(text)


def check_push_paths_sync(root: str) -> list[str]:
    with open(os.path.join(root, WORKFLOW), encoding="utf-8") as f:
        doc = _load_yaml(f.read())
    # NOTE: PyYAML parses the bare `on:` key as the boolean True (YAML 1.1). Handle
    # both spellings so the gate is robust to the key's rendered form.
    on = doc.get("on", doc.get(True))
    if not isinstance(on, dict):
        return [f"[push-paths] `on:` block missing or not a mapping in {WORKFLOW}"]
    push = on.get("push")
    if not isinstance(push, dict):
        return [f"[push-paths] `on.push` missing or not a mapping in {WORKFLOW}"]
    paths = push.get("paths")
    if not isinstance(paths, list):
        return [f"[push-paths] `on.push.paths` missing or not a list in {WORKFLOW}"]

    fails = []
    for prefix in CANONICAL_PREFIXES:
        if not _push_paths_cover_prefix(paths, prefix):
            fails.append(
                f"[push-paths] canonical prefix `{prefix}` is NOT covered by any "
                f"`on.push.paths` glob — a push touching it would not trigger the "
                f"workflow. Add a glob covering `{prefix}` to on.push.paths."
            )
    if not fails:
        sys.stderr.write(
            f"OK push-paths sync — on.push.paths ({len(paths)} globs) covers all "
            f"{len(CANONICAL_PREFIXES)} canonical prefixes\n"
        )
    return fails


def _push_paths_cover_prefix(paths: list[str], prefix: str) -> bool:
    """True iff some `on.push.paths` glob covers the canonical `prefix`.

    A dir-prefix `rust/` is covered by a `rust/**` (or `rust/*`) glob; an exact-file
    prefix (`.github/workflows/rust-prro.yml`) is covered by a literal or a `**`
    glob whose non-`**` head is a prefix of it. Conservative: only accept a glob
    that is a prefix of (or equals) the canonical prefix, so a NARROWER glob (e.g.
    `rust/prro/**` for canonical `rust/`) does NOT count as covering it.
    """
    for glob in paths:
        head = glob.split("*", 1)[0]  # literal head before the first wildcard
        if prefix.endswith("/"):
            # dir prefix: a glob covers it iff the glob's literal head is a prefix
            # of (or equals) `prefix` AND the glob wildcards the remainder.
            if "*" in glob and prefix.startswith(head) and len(head) <= len(prefix):
                return True
        else:
            # exact-file prefix: literal equality, or a `**` glob whose head is a
            # prefix of the file path.
            if glob == prefix:
                return True
            if "*" in glob and prefix.startswith(head):
                return True
    return False


# ── leg 3: CI wires the sound module, not the bypassable inline grep ─────────
def check_ci_wires_module(root: str) -> list[str]:
    with open(os.path.join(root, WORKFLOW), encoding="utf-8") as f:
        text = f.read()
    fails = []
    if DETECTOR_MODULE_INVOCATION not in text:
        fails.append(
            f"[ci-wiring] the workflow's `changes` job must invoke "
            f"`{DETECTOR_MODULE_INVOCATION}` (the sound detector). Not found in "
            f"{WORKFLOW} — the CI is not running the canonical detector."
        )
    # The old bypassable inline detector (`grep -qE '^(' … )` deciding rust=true)
    # must be GONE — its presence means a comment-injectable text detector is live.
    # Scan only NON-COMMENT lines (a `#`-prefixed YAML comment mentioning the old
    # form — as this very rationale does — is not an active detector). This is the
    # same structural discipline the fix itself enforces: never treat comment text
    # as executable.
    active = "\n".join(
        ln for ln in text.splitlines() if not ln.lstrip().startswith("#")
    )
    if re.search(r"grep\s+-qE\s+'\^\(", active):
        fails.append(
            f"[ci-wiring] an ACTIVE `grep -qE '^( … )'` inline detector is present in "
            f"{WORKFLOW} — this is the comment-injectable form the round-3 A1 bypass "
            f"exploited. The detector must be `{DETECTOR_MODULE_INVOCATION}`."
        )
    if not fails:
        sys.stderr.write(
            "OK ci-wiring — the `changes` job invokes the canonical detector module "
            "and the bypassable inline grep detector is absent\n"
        )
    return fails


def main() -> int:
    root = repo_root()
    fails: list[str] = []
    fails += check_detector_coverage(root)
    fails += check_push_paths_sync(root)
    fails += check_ci_wires_module(root)

    if fails:
        sys.stderr.write(
            "\nCS-1R3 A1 DETECTOR-CONSISTENCY GATE FAILED — one or more of "
            "{detector-coverage, push-paths-sync, ci-wiring} drifted, which can open "
            "a skip-the-required-check bypass:\n\n"
        )
        for f in fails:
            sys.stderr.write(f"  {f}\n")
        sys.stderr.write(
            "\nCanonical prefixes (scripts/cs1r/rust_change_paths.py "
            "CANONICAL_PREFIXES):\n"
        )
        for p in CANONICAL_PREFIXES:
            sys.stderr.write(f"    {p}\n")
        return 1

    sys.stderr.write(
        "\nCS-1R3 A1 detector-consistency gate OK — coverage + push-paths + "
        "ci-wiring all consistent with the canonical prefix list.\n"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
