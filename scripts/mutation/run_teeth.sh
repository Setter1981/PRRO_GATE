#!/usr/bin/env bash
# FW-1 — TEETH for the mutation diff-gate (scripts/mutation/run.sh).
#
# WHY THIS EXISTS. The inventory gate has had `inventory_gate_teeth.sh` since
# 2026-07-24; the mutation gate had nothing, and it rotted silently. On
# 2026-08-01 it turned out to be VACUOUS and FAIL-OPEN — three independent
# defects, each of which made it report a confident `OK` while testing zero
# mutants (bd PRRO_GATE-1rw):
#
#   1. `--in-diff` matched NOTHING. cargo-mutants runs from the cargo workspace
#      (`rust/`) and matches diff paths against `prro/src/...`, but `git diff`
#      emits REPO-relative paths, so the script produced `a/rust/prro/src/...`.
#   2. The UNMUTATED BASELINE failed, so cargo-mutants bailed before testing any
#      mutant.
#   3. The verdict came from `missed.txt` with `cargo mutants` under `|| true`,
#      so an EMPTY `missed.txt` read as success no matter WHY it was empty.
#
# A gate nobody proves can fail is a gate that eventually cannot. So each case
# below drives the REAL `run.sh` verbatim and asserts its EXIT CODE. Only the
# INPUTS are synthetic: a throwaway git fixture plus a STUB `cargo mutants` that
# writes controlled `mutants.out/` artifacts. That keeps a case at well under a
# second — the real gate needs ~20 minutes, which is precisely why it was never
# re-verified by hand.
#
# Cases (the script PASSES iff every case matches its expected exit code):
#   T1 clean run, no new survivor                  -> 0  (gate passes)
#   T2 a NEW survivor vs the baseline              -> 1  (the FW-1 ratchet)
#   T3 `--relative` dropped from the git diff      -> 2  (defect 1 above)
#   T4 the unmutated baseline failed               -> 3  (defect 2 above)
#   T5 mutants found but NONE tested               -> 3  (defect 3 above)
#   T6 zero mutants legitimately (plumbing diff)   -> 0  (the guard must NOT over-fire)
#   T7 a baseline-known survivor, still surviving  -> 0  (ratchet ≠ absolute bar)
#
# T6 and T7 are the negative teeth and they matter as much as the rest: a guard
# that fails closed on everything would pass T3-T5 while making the gate useless.
#
# Run: scripts/mutation/run_teeth.sh   (from anywhere in the repo)
set -euo pipefail

REAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE_SRC="$REAL_DIR/run.sh"
PASS=0
FAILED=0

# ── the STUB `cargo mutants` ────────────────────────────────────────────────
# Emulates the artifacts cargo-mutants leaves in `mutants.out/`, driven by
# $TEETH_SCENARIO. Deliberately NOT a mock of cargo-mutants' behaviour — only of
# its OUTPUT, which is the sole thing run.sh reads.
write_stub_cargo() {
  local bin="$1"
  mkdir -p "$bin"
  cat > "$bin/cargo" <<'STUB'
#!/usr/bin/env bash
# Only `cargo mutants ...` is emulated; anything else is a no-op success.
[ "${1:-}" = "mutants" ] || exit 0
# T8 witness: record whether the gate isolated the build directory before
# invoking us (bd PRRO_GATE-9g5 — a shared target dir gets MUTATED artifacts).
printf '%s\n' "${CARGO_TARGET_DIR:-<unset>}" > "${TEETH_TARGET_WITNESS:-/dev/null}"
printf '%s\n' "${TMPDIR:-<unset>}" > "${TEETH_TMPDIR_WITNESS:-/dev/null}"
out="mutants.out"
mkdir -p "$out"
: > "$out/caught.txt"
: > "$out/missed.txt"
: > "$out/unviable.txt"
: > "$out/timeout.txt"
case "${TEETH_SCENARIO:-clean}" in
  clean)
    printf '%s\n' 'prro/src/a.rs:1:1: replace x with y' > "$out/caught.txt"
    cat > "$out/outcomes.json" <<'J'
{"total_mutants":1,"caught":1,"missed":0,"timeout":0,"unviable":0,
 "outcomes":[{"scenario":"Baseline","summary":"Success"}]}
J
    ;;
  new_survivor)
    printf '%s\n' 'prro/src/a.rs:9:9: replace brand_new with 0' > "$out/missed.txt"
    cat > "$out/outcomes.json" <<'J'
{"total_mutants":1,"caught":0,"missed":1,"timeout":0,"unviable":0,
 "outcomes":[{"scenario":"Baseline","summary":"Success"}]}
J
    ;;
  known_survivor)
    printf '%s\n' 'prro/src/a.rs:5:5: replace known_gap with 0' > "$out/missed.txt"
    cat > "$out/outcomes.json" <<'J'
{"total_mutants":1,"caught":0,"missed":1,"timeout":0,"unviable":0,
 "outcomes":[{"scenario":"Baseline","summary":"Success"}]}
J
    ;;
  baseline_failed)
    # cargo-mutants bails: the unmutated tree does not pass its own tests.
    cat > "$out/outcomes.json" <<'J'
{"total_mutants":13,"caught":0,"missed":0,"timeout":0,"unviable":0,
 "outcomes":[{"scenario":"Baseline","summary":"Failure"}]}
J
    ;;
  found_none_tested)
    # Mutants enumerated, none reached a verdict (the --in-diff mismatch shape).
    cat > "$out/outcomes.json" <<'J'
{"total_mutants":13,"caught":0,"missed":0,"timeout":0,"unviable":0,
 "outcomes":[{"scenario":"Baseline","summary":"Success"}]}
J
    ;;
  died_no_outcomes)
    # cargo-mutants aborted before writing outcomes.json (a copy failure, a broken
    # build). Nothing to reason about — the gate must refuse, not fall through to an
    # empty missed.txt. This is the #376 shape.
    echo "Error: Failed to copy ... File name too long (os error 36)" >&2
    exit 1
    ;;
  zero_mutants)
    # A legitimate no-op: the diff touched nothing mutable.
    cat > "$out/outcomes.json" <<'J'
{"total_mutants":0,"caught":0,"missed":0,"timeout":0,"unviable":0,
 "outcomes":[{"scenario":"Baseline","summary":"Success"}]}
J
    ;;
esac
exit 0
STUB
  chmod +x "$bin/cargo"
  # run.sh refuses to start unless cargo-mutants looks installed.
  printf '#!/usr/bin/env bash\nexit 0\n' > "$bin/cargo-mutants"
  chmod +x "$bin/cargo-mutants"
}

# run_case NAME EXPECTED_EXIT SCENARIO [BASELINE_SURVIVORS] [MUTATE_GATE_SED]
run_case() {
  local name="$1" expect="$2" scenario="$3" baseline="${4:-}" mutate="${5:-}"
  local R; R="$(mktemp -d)"
  mkdir -p "$R/rust/prro/src" "$R/docs/mutation/baseline" "$R/scripts/mutation" "$R/bin"
  cp "$GATE_SRC" "$R/scripts/mutation/run.sh"
  # T3 mutates the gate itself — the canary for the defect that started all this.
  [ -n "$mutate" ] && sed -i "$mutate" "$R/scripts/mutation/run.sh"
  printf '%s' "$baseline" > "$R/docs/mutation/baseline/survivors.txt"
  write_stub_cargo "$R/bin"

  (
    cd "$R"
    git init -q
    git config user.email t@t; git config user.name t
    printf 'fn a() {}\n' > rust/prro/src/a.rs
    git add -A; git commit -qm base
    # No real remote: point `origin/main` at the base commit directly.
    git update-ref refs/remotes/origin/main HEAD
    # A real, non-empty prro/src diff so the gate reaches cargo-mutants.
    printf 'fn a() { let _ = 1 > 0; }\n' > rust/prro/src/a.rs
  )

  local rc=0
  # Inherit a DELIBERATELY shared target dir, so T8 proves the gate overrides it
  # rather than merely happening to run in a clean environment.
  ( cd "$R" && PATH="$R/bin:$PATH" TEETH_SCENARIO="$scenario" \
      CARGO_TARGET_DIR="$R/shared-target" TEETH_TARGET_WITNESS="$R/target_witness" \
      TMPDIR="$R/ramdisk-pretend" TEETH_TMPDIR_WITNESS="$R/tmpdir_witness" \
      bash scripts/mutation/run.sh diff 1 ) > "$R/out.log" 2>&1 || rc=$?
  LAST_TARGET_WITNESS="$(cat "$R/target_witness" 2>/dev/null || echo '<never-invoked>')"
  LAST_TMPDIR_WITNESS="$(cat "$R/tmpdir_witness" 2>/dev/null || echo '<never-invoked>')"
  LAST_FIXTURE_ROOT="$R"

  if [ "$rc" -eq "$expect" ]; then
    echo "✅ $name (exit $rc == expected $expect)"
    PASS=$((PASS + 1))
  else
    echo "❌ $name (exit $rc, expected $expect)"
    sed 's/^/    | /' "$R/out.log"
    FAILED=$((FAILED + 1))
  fi
  rm -rf "$R"
}

KNOWN='prro/src/a.rs:5:5: replace known_gap with 0'

run_case "T1 clean run, no new survivor -> gate PASSES" 0 clean ""
run_case "T2 NEW survivor vs baseline -> RED" 1 new_survivor ""
# T3: strip `--relative`, exactly as the gate shipped for three weeks. The diff
# then carries repo-relative paths and the self-check must refuse to proceed —
# WITHOUT it this case exits 0, which is the bug in one line.
run_case "T3 --relative dropped -> refuses (repo-relative paths)" 2 clean "" \
  's/git diff --relative origin\/main/git diff origin\/main/'
run_case "T4 unmutated baseline failed -> refuses (vacuity guard)" 3 baseline_failed ""
run_case "T5 mutants found but none tested -> refuses (vacuity guard)" 3 found_none_tested ""
run_case "T6 zero mutants legitimately -> gate PASSES (guard must not over-fire)" 0 zero_mutants ""
run_case "T7 baseline-KNOWN survivor still surviving -> gate PASSES (ratchet)" 0 known_survivor "$KNOWN
"
# T10 — cargo-mutants DIED before writing outcomes.json. The vacuity guard reads that
# file, so without an explicit arm the whole check is skipped and the verdict falls
# through to an empty missed.txt: fail-OPEN. That is exactly how #376 produced a green
# 24-second "OK" from a run that tested nothing (a recursive tree copy, my own bug).
run_case "T10 cargo-mutants died, no outcomes.json -> refuses" 4 died_no_outcomes ""

# T8 — the gate must ISOLATE its build directory (bd PRRO_GATE-9g5, P1).
#
# `run_case` deliberately exports a shared CARGO_TARGET_DIR into the gate. If the
# gate passes that through, cargo-mutants writes MUTATED production artifacts
# into it, and the next ordinary test run links against a mutant — observed on
# 2026-08-01 as a directed fuzzer tooth failing with a completely plausible
# message. The witness is recorded by the stub at the moment it is invoked, so
# this asserts what the gate ACTUALLY passed down, not what the script says.
if [ "$LAST_TARGET_WITNESS" = "<never-invoked>" ]; then
  echo "❌ T8 gate isolates CARGO_TARGET_DIR (cargo-mutants was never invoked)"
  FAILED=$((FAILED + 1))
elif [ "$LAST_TARGET_WITNESS" = "<unset>" ] || case "$LAST_TARGET_WITNESS" in *shared-target) true ;; *) false ;; esac; then
  echo "❌ T8 gate isolates CARGO_TARGET_DIR (leaked the caller's: $LAST_TARGET_WITNESS)"
  FAILED=$((FAILED + 1))
else
  echo "✅ T8 gate isolates CARGO_TARGET_DIR ($LAST_TARGET_WITNESS)"
  PASS=$((PASS + 1))
fi

# T9 — the gate must own its SCRATCH too (bd PRRO_GATE-9g5, second half).
#
# `run_case` hands the gate a TMPDIR of the caller's choosing, standing in for the
# tmpfs these dev boxes use. If the gate passes it through, cargo-mutants copies
# the tree and runs thousands of temp-DB tests on a RAM disk — which on 2026-08-01
# filled `/dev/shm` to 100% with ~3900 orphaned dirs from SIGKILLed runs and made
# UNRELATED tests fail for want of space.
if [ "$LAST_TMPDIR_WITNESS" = "<never-invoked>" ]; then
  echo "❌ T9 gate owns its scratch dir (cargo-mutants was never invoked)"
  FAILED=$((FAILED + 1))
elif [ "$LAST_TMPDIR_WITNESS" = "<unset>" ] \
  || case "$LAST_TMPDIR_WITNESS" in *ramdisk-pretend) true ;; *) false ;; esac \
  || case "$LAST_TMPDIR_WITNESS" in "$LAST_FIXTURE_ROOT"/*) true ;; *) false ;; esac; then
  echo "❌ T9 gate owns its scratch dir, OUTSIDE the copied tree (got: $LAST_TMPDIR_WITNESS)"
  FAILED=$((FAILED + 1))
else
  echo "✅ T9 gate owns its scratch dir ($LAST_TMPDIR_WITNESS)"
  PASS=$((PASS + 1))
fi

echo "── mutation-gate teeth: $PASS passed, $FAILED failed ──"
[ "$FAILED" -eq 0 ]
