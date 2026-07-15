#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Mutation-testing runner + baseline diff (FW-1).
#
# The "mutant database" is the committed baseline under docs/mutation/baseline/:
#   survivors.txt  — the mutants NO test kills (the actionable teeth-gaps)
#   outcomes.json  — every mutant + its verdict (the full machine-readable record)
#   mutants.json   — the mutant catalog
#
# Two tiers so we never re-run the whole crate "in rounds":
#   full          — whole crate; REFRESHES the committed baseline (rare, on a server)
#   diff          — only mutants in `git diff origin/main` (fast, per-change, default)
#   file:<path>   — one module under prro/src/ (e.g. file:services/write_path/error_routing.rs)
#
# Usage:
#   scripts/mutation/run.sh [full|diff|file:<path>] [JOBS]
#
# Exit non-zero if there are NEW survivors vs the baseline (usable as a gate).
# cargo-mutants inherits rust/.cargo/mutants.toml (--features test-support + nextest).
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT/rust"

MODE="${1:-diff}"
JOBS="${2:-$(nproc)}"
BASE="$ROOT/docs/mutation/baseline"
OUT="mutants.out"

command -v cargo-mutants >/dev/null 2>&1 || {
  echo "cargo-mutants not installed — run: cargo install cargo-mutants --locked" >&2
  exit 127
}

MUT_ARGS=(-j "$JOBS")
case "$MODE" in
  full) ;;
  diff)
    git fetch origin main -q 2>/dev/null || true
    # diff paths are relative to the cargo workspace (rust/), which is what
    # cargo-mutants --in-diff expects.
    git diff origin/main -- prro/src > /tmp/mutation.diff || true
    if [ ! -s /tmp/mutation.diff ]; then
      echo "no prro/src diff vs origin/main — nothing to mutate."
      exit 0
    fi
    MUT_ARGS+=(--in-diff /tmp/mutation.diff)
    ;;
  file:*)
    MUT_ARGS+=(--file "prro/src/${MODE#file:}")
    ;;
  *)
    echo "usage: run.sh [full|diff|file:<path>] [JOBS]" >&2
    exit 2
    ;;
esac

echo ">>> cargo mutants ${MUT_ARGS[*]}  (mode=$MODE, jobs=$JOBS)"
# cargo-mutants exits non-zero when mutants survive; we do our own gating below.
cargo mutants "${MUT_ARGS[@]}" || true

# ── analysis vs the committed baseline ──────────────────────────────────────
CUR_SURV="$OUT/missed.txt"
CUR_CAUGHT="$OUT/caught.txt"
BASE_SURV="$BASE/survivors.txt"
# cargo-mutants writes NO `mutants.out/` directory when a run tests zero mutants
# (e.g. an `--in-diff` diff of pure plumbing / macro-generated code). Without
# the dir, the empty-file guards below fail `No such file or directory` (exit 1)
# and the gate reports a spurious failure. Zero mutants ⇒ zero NEW survivors ⇒
# the gate must PASS, so ensure the dir exists first.
mkdir -p "$OUT"
[ -f "$CUR_SURV" ]  || : > "$CUR_SURV"
[ -f "$CUR_CAUGHT" ] || : > "$CUR_CAUGHT"
[ -f "$BASE_SURV" ] || : > "$BASE_SURV"

NEW="$(comm -23 <(sort -u "$CUR_SURV") <(sort -u "$BASE_SURV") || true)"
CLOSED="$(comm -13 <(sort -u "$CUR_SURV") <(sort -u "$BASE_SURV") || true)"
# Regression: a mutant that the baseline recorded as SURVIVING is now CAUGHT →
# good (CLOSED, above). The dangerous regression — a baseline-caught mutant now
# surviving — is exactly a NEW survivor that also appears in the baseline record,
# which `NEW` already surfaces (it was not in survivors before, now it is).

echo ""
echo "═══════════════════ MUTATION SUMMARY (mode=$MODE) ═══════════════════"
printf "  caught this run   : %s\n" "$(wc -l < "$CUR_CAUGHT" | tr -d ' ')"
printf "  survivors this run: %s\n" "$(wc -l < "$CUR_SURV" | tr -d ' ')"
if [ -n "$NEW" ]; then
  echo "  🔴 NEW survivors (no test kills these — add teeth):"
  echo "$NEW" | sed 's/^/       /'
fi
if [ -n "$CLOSED" ]; then
  echo "  🟢 CLOSED since baseline (teeth added / code changed):"
  echo "$CLOSED" | sed 's/^/       /'
fi
[ -z "$NEW$CLOSED" ] && echo "  no change vs baseline."
echo "═════════════════════════════════════════════════════════════════════"

if [ "$MODE" = full ]; then
  echo ">>> refreshing the committed baseline at docs/mutation/baseline/"
  mkdir -p "$BASE"
  cp "$OUT/missed.txt"   "$BASE/survivors.txt"
  cp "$OUT/outcomes.json" "$BASE/outcomes.json" 2>/dev/null || true
  cp "$OUT/mutants.json"  "$BASE/mutants.json"  2>/dev/null || true
  echo "    baseline refreshed — review + commit docs/mutation/baseline/"
fi

# Gate: a fresh survivor is a real teeth-gap in changed/new code.
if [ -n "$NEW" ]; then
  echo "FAIL: $(echo "$NEW" | grep -c .) new survivor(s)." >&2
  exit 1
fi
echo "OK."
