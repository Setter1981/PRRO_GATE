#!/usr/bin/env bash
# CS-1R R1.2 — forward additions-only inventory gate (three-way control).
#
# Enforces ALL THREE (spec §4 R1.2), profile in identity:
#   (1) live `nextest list` == committed manifest (no drift), per profile;
#   (2) the PR's manifest diff vs base may ONLY ADD identity rows;
#   (3) every new source test file appears in the committed source inventory in
#       THIS PR (a new test can't be added-then-silently-deleted).
#
# The TWO literal profile commands are frozen below (verbatim from spec §4 R1.2).
# `cargo-nextest` is pinned to 0.9.137 in CI (rust-prro.yml). `cargo` is pinned to
# 1.95.0 via rust-toolchain.toml.
#
# Usage:
#   scripts/cs1r/inventory_gate.sh            # controls (1) + (3) — always
#   scripts/cs1r/inventory_gate.sh --pr <base-ref>   # + control (2) additions-only
#
# Run from the repo root. Requires: cargo (+nextest), python3, git.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

CARGO="${CARGO:-cargo}"
TARGET="x86_64-unknown-linux-gnu"
INV_DIR="docs/cs1r/inventory"
SCRIPTS="scripts/cs1r"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() { echo "❌ INVENTORY GATE: $*" >&2; exit 1; }
ok() { echo "✅ $*"; }

# ── the TWO literal profile commands (verbatim, spec §4 R1.2) ────────────────
list_test_support() {
  ( cd rust && "$CARGO" nextest list --workspace --features prro/test-support \
      --message-format json --locked --target "$TARGET" )
}
list_live_dps() {
  ( cd rust && "$CARGO" nextest list --workspace --features prro/test-support,prro/live-dps \
      --message-format json --locked --target "$TARGET" )
}

# ── control (1): live == committed, per profile ──────────────────────────────
check_profile() {
  local profile="$1"
  local lister="$2"
  local committed="$INV_DIR/manifest.$profile.tsv"
  [ -f "$committed" ] || fail "committed manifest missing: $committed"
  "$lister" > "$TMP/$profile.json"
  python3 "$SCRIPTS/nextest_manifest.py" "$profile" < "$TMP/$profile.json" > "$TMP/$profile.tsv"
  if ! diff -u "$committed" "$TMP/$profile.tsv" > "$TMP/$profile.diff"; then
    echo "----- live vs committed drift ($profile) -----" >&2
    cat "$TMP/$profile.diff" >&2
    fail "live \`nextest list\` != committed manifest ($profile). \
Re-mint: scripts/cs1r/mint_manifests.sh"
  fi
  ok "profile '$profile': live == committed ($(wc -l < "$committed") tests)"
}

check_profile "test-support" list_test_support
check_profile "live-dps" list_live_dps

# ── control (3): source inventory drift + new-file-present ────────────────────
python3 "$SCRIPTS/source_inventory.py" --check "$INV_DIR/source_files.sha256" \
  || fail "source inventory drift (control 3)"
ok "source inventory: no drift, every source test file recorded"

# ── control (2): additions-only vs base (PR mode) ────────────────────────────
if [ "${1:-}" = "--pr" ]; then
  BASE_REF="${2:?--pr requires a base ref}"
  for profile in test-support live-dps; do
    committed="$INV_DIR/manifest.$profile.tsv"
    git show "$BASE_REF:$committed" > "$TMP/base.$profile.tsv" 2>/dev/null \
      || { echo "base has no $committed (new file) — treated as empty"; : > "$TMP/base.$profile.tsv"; }
    # rows present in base but ABSENT in the PR manifest = a REMOVAL (forbidden).
    removed="$(comm -23 <(sort "$TMP/base.$profile.tsv") <(sort "$committed") || true)"
    if [ -n "$removed" ]; then
      echo "----- forbidden REMOVALS from $profile manifest -----" >&2
      echo "$removed" >&2
      fail "manifest diff vs base REMOVED identity rows ($profile) — additions-only \
(delete/rename/#[ignore] of an existing test is forbidden without an explicit \
architecture decision)"
    fi
    added="$(comm -13 <(sort "$TMP/base.$profile.tsv") <(sort "$committed") | wc -l)"
    ok "profile '$profile': additions-only vs $BASE_REF ($added added, 0 removed)"
  done

  # control (3) additions-only for the SOURCE inventory: a committed source test
  # file path may not VANISH from the manifest vs base (a delete is an explicit
  # manifest edit that this catches).
  git show "$BASE_REF:$INV_DIR/source_files.sha256" > "$TMP/base.src.tsv" 2>/dev/null \
    || : > "$TMP/base.src.tsv"
  removed_src="$(comm -23 <(cut -f2 "$TMP/base.src.tsv" | sort) \
                          <(cut -f2 "$INV_DIR/source_files.sha256" | sort) || true)"
  if [ -n "$removed_src" ]; then
    echo "----- source test files REMOVED from inventory -----" >&2
    echo "$removed_src" >&2
    fail "source inventory REMOVED file paths vs base — additions-only"
  fi
  ok "source inventory: additions-only vs $BASE_REF"
fi

ok "CS-1R R1.2 inventory gate PASSED"
