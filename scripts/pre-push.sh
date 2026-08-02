#!/usr/bin/env bash
# Run the CI gates LOCALLY, before pushing — in the order that fails cheapest first.
#
# WHY THIS EXISTS, stated honestly: in one session on 2026-08-01, six CI rounds were
# spent and THREE were avoidable — a missing exec bit, a clippy error hidden behind a
# cached result, and a source-inventory manifest that was not re-minted after editing
# a tracked test file. Each is caught by a check that already exists; none of them was
# run before pushing. The heavy CI leg costs ~16-26 minutes a round, so a wasted round
# is not a small thing.
#
# The subtle one is clippy. `cargo clippy` REPLAYS a cached result and prints nothing
# when the inputs have not changed by its reckoning — so a local run can be silent
# while CI, building fresh, errors. This script therefore TOUCHES every .rs file that
# differs from the base before linting, which forces a real re-analysis. That single
# behaviour is the reason to use this instead of running the commands by hand.
#
# Usage:
#   scripts/pre-push.sh              # gates vs origin/main
#   scripts/pre-push.sh <base-ref>   # gates vs another base (a stacked PR's parent)
#   FAST=1 scripts/pre-push.sh       # skip the full test suite (fmt/clippy/gates only)
#
# Exit non-zero on the first failing gate. It does NOT push — that stays your decision.
set -uo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
BASE="${1:-origin/main}"
FAILED=0
STEP=0

step() {
  STEP=$((STEP + 1))
  printf '\n\033[1m── %d. %s ──\033[0m\n' "$STEP" "$1"
}

fail() {
  printf '\033[31m❌ %s\033[0m\n' "$1" >&2
  FAILED=1
}

ok() { printf '\033[32m✅ %s\033[0m\n' "$1"; }

# ── 0. What changed, and does it need the heavy legs? ────────────────────────
git fetch origin -q 2>/dev/null || true
if ! git rev-parse --verify -q "$BASE" >/dev/null; then
  echo "base ref '$BASE' not found — pass one explicitly, e.g. scripts/pre-push.sh origin/main" >&2
  exit 2
fi
CHANGED="$(git diff --name-only --no-renames "$BASE"...HEAD)"
if [ -z "$CHANGED" ]; then
  echo "no diff vs $BASE — nothing to gate."
  exit 0
fi
printf 'gating %s changed file(s) vs %s\n' "$(echo "$CHANGED" | grep -c .)" "$BASE"

# ── 1. fmt ───────────────────────────────────────────────────────────────────
step "cargo fmt --check"
if (cd rust && cargo fmt --all -- --check); then ok "fmt"; else fail "fmt — run 'cargo fmt --all'"; fi

# ── 2. clippy, on a FORCED re-analysis ───────────────────────────────────────
#
# The touch is the point. Without it clippy can replay a cached verdict and stay
# silent on code CI will reject — which is exactly how six `needless_borrow`
# errors reached CI on 2026-08-01 after a locally-green run.
step "cargo clippy -D warnings (forced re-analysis of changed files)"
TOUCHED=0
while IFS= read -r f; do
  case "$f" in
    *.rs) [ -f "$f" ] && touch "$f" && TOUCHED=$((TOUCHED + 1)) ;;
  esac
done <<< "$CHANGED"
printf '   touched %d changed .rs file(s) so clippy cannot replay a cached pass\n' "$TOUCHED"
if (cd rust && cargo clippy -p prro --all-targets --no-deps --features test-support --locked -- -D warnings); then
  ok "clippy"
else
  fail "clippy"
fi

# ── 3. the CS-1R inventory gate ──────────────────────────────────────────────
#
# Bites on CONTENT, not intent: editing a tracked test file — even comment-only —
# drifts its SHA and fails the gate until the manifest is re-minted. That is what
# it is for, and what caught a comment-only change on #371.
step "CS-1R inventory gate (three-way, --pr $BASE)"
# `--pr <base>` is NOT optional here, and leaving it off was a real gap: without it
# only controls 1+3 run (live==committed, source-inventory drift) and CONTROL 2 —
# additions-only vs the base — is skipped entirely. Control 2 is the one that fails
# on a removed test identity, and the identity row carries the `ignored` FLAG, so
# merely un-`#[ignore]`ing a test reads as a removal needing a supersession row.
# That is exactly what this script waved through and CI then caught on #375.
if bash scripts/cs1r/inventory_gate.sh --pr "$BASE"; then
  ok "inventory gate"
else
  fail "inventory gate — re-mint with 'bash scripts/cs1r/mint_manifests.sh'; a REMOVED identity \
(incl. an un-\`#[ignore]\`d test) needs a row in docs/cs1r/inventory/superseded_removals.tsv"
fi

# ── 4. the mutation gate's own teeth ─────────────────────────────────────────
# Seconds, no cargo. Proves the gate can still FAIL (bd PRRO_GATE-1rw).
step "mutation gate teeth"
# Report the count the teeth script ITSELF prints. The first version hardcoded
# "8/8" and went on claiming it after T9 landed — a status line that cannot be
# wrong is a status line that tells you nothing.
TEETH_OUT="$(bash scripts/mutation/run_teeth.sh 2>&1)"
TEETH_RC=$?
TEETH_TALLY="$(printf '%s' "$TEETH_OUT" | grep -oE '[0-9]+ passed, [0-9]+ failed' | tail -1)"
if [ "$TEETH_RC" -eq 0 ]; then
  ok "gate teeth — ${TEETH_TALLY:-passed}"
else
  printf '%s\n' "$TEETH_OUT" | grep -E '^(✅|❌)' | sed 's/^/     /'
  fail "mutation gate teeth — ${TEETH_TALLY:-failed}"
fi

# ── 5. executable bits on scripts CI invokes ─────────────────────────────────
# A gate script committed 100644 is a gate CI cannot run — #370 shipped exactly
# that, and `inventory_gate_teeth.sh` still sits at 100644 today.
step "exec bit on scripts a workflow invokes DIRECTLY"
# Only a DIRECT invocation needs the bit. Most gates are called as
# `bash scripts/…`, which works at any mode — and five committed scripts are in
# fact 100644 today for exactly that reason. Failing on those would make this a
# nag, and a gate people learn to ignore is worse than no gate at all. So the
# check is narrow: a workflow `run:` line that executes a script WITHOUT a
# bash/sh prefix, where the file is not executable in git. That is precisely the
# shape that turned #370's gate-teeth job red on its first run.
DIRECT="$(grep -rhoE '^[[:space:]]*run:[[:space:]]+(\./)?scripts/[A-Za-z0-9_/.-]+\.sh' .github/workflows/ 2>/dev/null \
  | sed -E 's/^[[:space:]]*run:[[:space:]]+(\.\/)?//' | sort -u)"
BAD=""
for s in $DIRECT; do
  mode="$(git ls-files -s -- "$s" | awk '{print $1}')"
  [ "$mode" = "100644" ] && BAD="$BAD$s"$'\n'
done
if [ -z "$BAD" ]; then
  ok "$(echo "$DIRECT" | grep -c .) directly-invoked script(s), all executable"
else
  printf '   invoked directly by a workflow but NOT executable in git:\n%s' \
    "$(echo "$BAD" | sed 's/^/     /')"
  printf '   fix: git update-index --chmod=+x <path>\n'
  fail "a directly-invoked script would not run in CI"
fi

# ── 6. the suite ─────────────────────────────────────────────────────────────
if [ "${FAST:-0}" = "1" ]; then
  printf '\n(FAST=1 — skipping the test suite)\n'
else
  step "cargo nextest run -p prro (the merge gate)"
  if (cd rust && cargo nextest run -p prro --features test-support --locked --no-fail-fast); then
    ok "suite"
  else
    fail "suite"
  fi
fi

printf '\n'
if [ "$FAILED" -eq 0 ]; then
  printf '\033[32m── all local gates passed — safe to push ──\033[0m\n'
else
  printf '\033[31m── a gate failed; fix it here rather than spending a ~20min CI round ──\033[0m\n'
fi
exit "$FAILED"
