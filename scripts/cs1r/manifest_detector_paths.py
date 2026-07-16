#!/usr/bin/env python3
"""CS-1R2 A1 — machine consistency gate for the CI path-detector.

The `detect rust changes` step in `.github/workflows/rust-prro.yml` decides
whether the REQUIRED `x86_64-unknown-linux-gnu` (`build`) job runs. A skipped
required check is treated by GitHub as satisfied — so if the detector's grep
pattern does NOT cover a path that the `build` job actually gates, a PR touching
only that path SKIPS the required context and merges green-but-unsound. That is
the empirical #305 bypass (a PR that changed only `rust/prro_crypto_v2/`,
`rust/prro_sidecar/`, `rust/maria304_driver/`, `rust/prro_escpos_daemon/` plus
the `docs/cs1r/inventory/` manifests merged with x86_64 skipping).

This gate asserts:  detector-pattern  ⊇  (every workspace-member dir)
                                        ∪ (every gate-input path the build job reads)

so that a FUTURE crate or gate input added without a matching detector path turns
this check RED instead of silently opening a bypass.

Run from the repo root:
    scripts/cs1r/manifest_detector_paths.py            # verify, exit 1 on gap
Requires: cargo (workspace metadata), python3, git.
"""
import json
import os
import re
import subprocess
import sys

WORKFLOW = ".github/workflows/rust-prro.yml"

# Non-crate paths the `build` job reads as gate inputs (must be covered by the
# detector, else a change to just these SKIPS the required check). These mirror
# the scripts/manifests the `CS-1R R1.2 inventory gate` step and the provenance
# tests consume. Add here whenever the build job grows a new file input.
GATE_INPUT_PATHS = [
    "scripts/cs1r/inventory_gate.sh",
    "scripts/cs1r/mint_manifests.sh",
    "scripts/cs1r/nextest_manifest.py",
    "scripts/cs1r/source_inventory.py",
    "scripts/cs1r/manifest_detector_paths.py",
    "docs/cs1r/inventory/manifest.test-support.tsv",
    "docs/cs1r/inventory/manifest.live-dps.tsv",
    "docs/cs1r/inventory/source_files.sha256",
]


def cargo() -> str:
    return os.environ.get("CARGO", "cargo")


def extract_detector_pattern(workflow_text: str) -> str:
    """Pull the `^( … )` alternation out of the detect-rust-changes grep -qE."""
    # The detector is the FIRST `grep -qE '^( … )'` in the file (the changes job).
    m = re.search(r"grep -qE '\^\((.*?)\)'", workflow_text)
    if not m:
        raise SystemExit(f"could not find detector `grep -qE '^(...)'` in {WORKFLOW}")
    return m.group(1)


def pattern_matches(alternation: str, path: str) -> bool:
    """True if `path` matches the anchored detector alternation `^( … )`."""
    return re.match(f"^({alternation})", path) is not None


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
        manifest = pkg["manifest_path"]
        crate_dir = os.path.dirname(manifest)
        rel = os.path.relpath(crate_dir, root).replace(os.sep, "/")
        # canonical "touched a file under this crate" probe path
        dirs.append(f"{rel}/Cargo.toml")
    return sorted(dirs)


def main() -> int:
    root = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()

    with open(os.path.join(root, WORKFLOW), encoding="utf-8") as f:
        alternation = extract_detector_pattern(f.read())

    probes: list[tuple[str, str]] = []
    for d in workspace_member_dirs(root):
        probes.append(("workspace-member", d))
    # a src file under each member must also match (dir-prefix coverage)
    for d in workspace_member_dirs(root):
        crate = d.rsplit("/", 1)[0]
        probes.append(("workspace-member-src", f"{crate}/src/lib.rs"))
    for p in GATE_INPUT_PATHS:
        probes.append(("gate-input", p))

    uncovered = [(kind, p) for kind, p in probes if not pattern_matches(alternation, p)]

    if uncovered:
        sys.stderr.write(
            "CS-1R2 A1 DETECTOR-COVERAGE GATE FAILED — the `detect rust changes` "
            f"pattern in {WORKFLOW} does NOT match these gated paths, so a PR "
            "touching only them would SKIP the required x86_64 context (bypass):\n"
        )
        for kind, p in uncovered:
            sys.stderr.write(f"  [{kind}] {p}\n")
        sys.stderr.write(
            "\nFix: widen the detector alternation (currently:\n"
            f"  {alternation}\n) to cover the paths above.\n"
        )
        return 1

    sys.stderr.write(
        f"CS-1R2 A1 detector coverage OK — pattern covers "
        f"{len(probes)} gated paths (members + gate inputs)\n"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
