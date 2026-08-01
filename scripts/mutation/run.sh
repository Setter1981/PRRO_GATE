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

# ── ISOLATE the build directory (bd PRRO_GATE-9g5, P1) ──────────────────────
#
# cargo-mutants tests each mutant by applying it to a COPY of the tree and
# BUILDING it. With a shared `CARGO_TARGET_DIR` — which this project uses — those
# builds land next to everything else, and the next ordinary `cargo nextest run`
# links against whatever mutant was compiled last.
#
# That was caught red-handed on 2026-08-01: after a gate run, a directed fuzzer
# tooth reported `model granted=false but real outcome was Replenished` — the
# production guard had not fired, because the linked `prro` WAS one of the
# mutants. The suite was faithfully reporting a real divergence against MUTATED
# production code.
#
# This is nastier than it sounds, and it is why the gate owns the isolation
# rather than trusting the caller's environment: the symptom is not obviously
# bogus. It fails exactly the test covering the code you just wrote, with a
# plausible message — and it can equally go GREEN against a mutant and be read as
# verification. A local "all tests pass" taken after a mutation run means nothing
# unless the target dir is known clean.
#
# The secondary cost of sharing was baked paths: `env!("CARGO_MANIFEST_DIR")` is
# resolved at compile time, so binaries built in the copy carry its temp path and
# every repo-reading test later fails `NotFound`.
export CARGO_TARGET_DIR="$ROOT/rust/target-mutants"

MUT_ARGS=(-j "$JOBS")
case "$MODE" in
  full) ;;
  diff)
    git fetch origin main -q 2>/dev/null || true
    # `--relative` is LOAD-BEARING, and its absence made this gate VACUOUS.
    #
    # cargo-mutants runs from the cargo workspace (`rust/`) and matches `--in-diff`
    # paths against ITS view of the tree — `prro/src/...`. But `git diff` emits
    # REPO-relative paths regardless of cwd, i.e. `a/rust/prro/src/...`. Nothing
    # matched, so cargo-mutants logged "Diff changes no Rust source files", tested
    # ZERO mutants, and the gate below happily reported OK — for every PR since this
    # script was written. The comment that used to sit here asserted the paths were
    # already workspace-relative; it was simply wrong, and it read as authoritative.
    #
    # Caught 2026-08-01 while checking why the gate reported "caught this run: 0" on
    # two consecutive PRs. Verification if you ever doubt it: `cargo mutants --list
    # --in-diff <file>` on a diff you KNOW touches mutable code must print mutants,
    # not "no Rust source files". A silent zero is the failure signature.
    git diff --relative origin/main -- prro/src > /tmp/mutation.diff || true
    if [ ! -s /tmp/mutation.diff ]; then
      echo "no prro/src diff vs origin/main — nothing to mutate."
      exit 0
    fi
    # Self-check for the failure above: every `+++ b/` path must be WORKSPACE-relative
    # (`prro/src/...`). If `--relative` is ever dropped again they become `rust/prro/...`,
    # cargo-mutants silently matches nothing, and the gate goes green without testing a
    # single mutant. Fail loudly instead — a gate that cannot bite must not report OK.
    if grep -q '^+++ b/rust/' /tmp/mutation.diff; then
      echo "FATAL: diff paths are REPO-relative (rust/prro/...); cargo-mutants --in-diff" >&2
      echo "       expects workspace-relative (prro/...) and would match NOTHING." >&2
      echo "       The 'git diff' above lost its --relative flag." >&2
      exit 2
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

# ── VACUITY GUARD: this gate used to be fail-OPEN ────────────────────────────
#
# The verdict below is derived purely from `missed.txt` vs the committed
# baseline. An EMPTY `missed.txt` therefore reported OK — no matter WHY it was
# empty. Three distinct failures all produced a confident green:
#   1. `--in-diff` matched nothing (the missing `--relative`, guarded above);
#   2. the UNMUTATED BASELINE failed, so cargo-mutants tested zero mutants and
#      bailed — observed 2026-08-01, `cs1_test_provenance` cannot run inside
#      cargo-mutants' copied tree because the copy carries no `.git`;
#   3. the build broke outright.
# For a project whose posture is fail-closed everywhere else, a gate that
# reports success when it could not run is the wrong default. So: if mutants
# were FOUND but none reached a verdict, refuse to pass.
if [ -f "$OUT/outcomes.json" ]; then
  python3 - "$OUT/outcomes.json" <<'PY' || exit 3
import json, sys
d = json.load(open(sys.argv[1]))
total = d.get("total_mutants", 0)
tested = sum(d.get(k, 0) for k in ("caught", "missed", "timeout", "unviable"))
baseline_failed = any(
    o.get("summary") == "Failure" and o.get("scenario") == "Baseline"
    for o in d.get("outcomes", [])
)
if baseline_failed:
    sys.exit("FATAL: the UNMUTATED baseline failed — zero mutants were tested. "
             "The gate cannot bite; fix the baseline before trusting it.")
if total > 0 and tested == 0:
    sys.exit(f"FATAL: {total} mutants were found but NONE were tested. "
             "The gate cannot bite.")
PY
fi

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
