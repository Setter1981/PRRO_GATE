#!/usr/bin/env bash
# CS-1R R1.2 — forward additions-only inventory gate (three-way control).
#
# Enforces ALL THREE (spec §4 R1.2), profile in identity:
#   (1) live `nextest list` == committed manifest (no drift), per profile;
#   (2) the PR's manifest diff vs base may ONLY ADD identity rows — the sole
#       exception is an adjudicated retire-and-replace recorded EXACTLY in the
#       supersession registry (docs/cs1r/inventory/superseded_removals.tsv);
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

# CS-1R4 — the CS-1R3 A1 "CI rust-change-detector consistency" block was DELETED
# here. It structurally checked the `changes` (detect rust changes) job + its
# canonical-prefix list, but CS-1R4 REMOVED that whole dispatch: the required
# `x86_64-unknown-linux-gnu` context now runs UNCONDITIONALLY (rust-prro.yml has
# no `changes` job / detector), so there is no detector left to keep consistent —
# the attackable surface (coverage gap / push-paths unsync / comment-injection)
# evaporated with the detector. `scripts/cs1r/manifest_detector_paths.py` and
# `scripts/cs1r/rust_change_paths.py` were deleted in the same change.

# ── control (2): additions-only vs base, with a recorded SUPERSESSION registry ──
# A removal (a test identity present in base but ABSENT in the PR) stays FORBIDDEN
# by default. The ONLY escape is an adjudicated retire-and-replace recorded EXACTLY
# in the supersession registry (docs/cs1r/inventory/superseded_removals.tsv):
#   * every removal MUST be registered (module,test) — no glob, no wildcard;
#   * the registry MUST NOT be stale — every registered removal must ACTUALLY be
#     absent vs base;
#   * every registered REPLACEMENT test MUST exist in the current manifest (retired
#     coverage moved, it did not vanish).
# So the set of real removals == the set of registered supersessions, exactly. Any
# UNregistered future removal (or a stale/false registry row, or a missing
# replacement) is RED. Controls (1) live==committed and (3) source-inventory are
# UNCHANGED.
if [ "${1:-}" = "--pr" ]; then
  BASE_REF="${2:?--pr requires a base ref}"
  REGISTRY="$INV_DIR/superseded_removals.tsv"

  # Approved (removed_module, removed_test) identities from the registry.
  # Columns (TAB): removed_module  removed_test  repl_module  repl_test  reason
  if [ -f "$REGISTRY" ]; then
    grep -vE '^[[:space:]]*(#|$)' "$REGISTRY" | cut -f1,2 | sort -u > "$TMP/approved_ident.tsv"
  else
    : > "$TMP/approved_ident.tsv"
  fi

  : > "$TMP/removed_ident_all.tsv"
  for profile in test-support live-dps; do
    committed="$INV_DIR/manifest.$profile.tsv"
    git show "$BASE_REF:$committed" > "$TMP/base.$profile.tsv" 2>/dev/null \
      || { echo "base has no $committed (new file) — treated as empty"; : > "$TMP/base.$profile.tsv"; }
    comm -23 <(sort "$TMP/base.$profile.tsv") <(sort "$committed") > "$TMP/removed.$profile.tsv" || true
    # (module f3, test f4) of each removed identity row.
    cut -f3,4 "$TMP/removed.$profile.tsv" | sort -u > "$TMP/removed_ident.$profile.tsv"
    cat "$TMP/removed_ident.$profile.tsv" >> "$TMP/removed_ident_all.tsv"
    # (a) every removal must be a REGISTERED supersession — else forbidden.
    unapproved="$(comm -23 "$TMP/removed_ident.$profile.tsv" "$TMP/approved_ident.tsv" || true)"
    if [ -n "$unapproved" ]; then
      echo "----- REMOVED identities NOT in supersession registry ($profile) -----" >&2
      echo "$unapproved" >&2
      fail "manifest REMOVED test identities not recorded in $REGISTRY ($profile) — additions-only \
unless an adjudicated retire-and-replace is registered (delete/rename/#[ignore] is otherwise forbidden)"
    fi
    added="$(comm -13 <(sort "$TMP/base.$profile.tsv") <(sort "$committed") | wc -l | tr -d ' ')"
    superseded="$(wc -l < "$TMP/removed_ident.$profile.tsv" | tr -d ' ')"
    ok "profile '$profile': additions-only + registered-supersession vs $BASE_REF ($added added, $superseded superseded)"
  done

  # (b) NO stale registry rows: every approved supersession must ACTUALLY be absent
  # vs base in some profile (a registry row with no real removal is RED).
  sort -u "$TMP/removed_ident_all.tsv" > "$TMP/removed_ident_u.tsv"
  stale="$(comm -23 "$TMP/approved_ident.tsv" "$TMP/removed_ident_u.tsv" || true)"
  if [ -n "$stale" ]; then
    echo "----- STALE supersession registry rows (no matching removal vs base) -----" >&2
    echo "$stale" >&2
    fail "supersession registry has rows with no actual removal vs base — prune $REGISTRY (exact, no stale)"
  fi

  # (c) every registered REPLACEMENT must EXIST in the current committed manifest
  # (test-support OR live-dps). Read via process-substitution (NOT a pipe) so a
  # `fail` inside the loop exits the script instead of a subshell.
  if [ -f "$REGISTRY" ]; then
    while IFS=$'\t' read -r rmod rtest repl_mod repl_test reason; do
      case "$rmod" in ''|\#*) continue ;; esac
      hit="$(awk -F'\t' -v m="$repl_mod" -v t="$repl_test" '$3==m && $4==t {print; exit}' \
             "$INV_DIR/manifest.test-support.tsv" "$INV_DIR/manifest.live-dps.tsv")"
      if [ -z "$hit" ]; then
        fail "supersession replacement '$repl_mod::$repl_test' (retiring '$rmod::$rtest') \
not found in the committed manifests — the replacement test must exist"
      fi
    done < <(grep -vE '^[[:space:]]*(#|$)' "$REGISTRY")
    ok "supersession registry: all replacements exist in the current manifest"
  fi

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
