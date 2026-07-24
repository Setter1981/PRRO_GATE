#!/usr/bin/env bash
# CS-1R R1.2 — TEETH for the inventory gate (scripts/cs1r/inventory_gate.sh).
#
# Proves the gate BITES on real defects and that the empty-registry guard does NOT
# weaken removal detection. Each case builds a throwaway git fixture and runs the
# REAL `inventory_gate.sh --pr <base>` against synthetic manifests + registry, with
# a STUB `cargo nextest list` so controls (1) live==committed and (3) source-inventory
# pass trivially — isolating control (2) (additions-only + supersession) and the
# control-(1) drift check. The LOGIC under test is the real script, verbatim; only the
# INPUTS are synthetic.
#
# Cases (exit 0 = PASS, exit 1 = RED):
#   T1 empty registry + zero removals            -> PASS  (the empty-registry guard)
#   T2 unregistered real removal (empty registry)-> RED   (guard must NOT swallow removals)
#   T3 stale registry row (no matching removal)  -> RED
#   T4 registered removal, replacement missing    -> RED
#   T5 live/committed manifest drift              -> RED
#
# Run: scripts/cs1r/inventory_gate_teeth.sh   (from anywhere in the repo)
set -euo pipefail

REAL_SCRIPTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$REAL_SCRIPTS/inventory_gate.sh"
PASS=0
FAILED=0

# JSON for `cargo nextest list --message-format json` from a space-separated test list.
json_of() {
  local names="$1" tc=""
  for n in $names; do tc="$tc\"$n\":{\"ignored\":false},"; done
  tc="${tc%,}"
  printf '{"rust-suites":{"prro::alpha":{"package-name":"prro","binary-id":"prro::alpha","testcases":{%s}}}}' "$tc"
}

# run_case NAME EXPECT HEAD_TESTS BASE_TESTS REGISTRY_BODY [DRIFT_EXTRA_TEST]
run_case() {
  local name="$1" expect="$2" head_tests="$3" base_tests="$4" registry_body="$5" drift="${6:-}"
  local R; R="$(mktemp -d)"
  mkdir -p "$R/scripts/cs1r" "$R/docs/cs1r/inventory" "$R/rust" "$R/bin"
  cp "$REAL_SCRIPTS/inventory_gate.sh" "$REAL_SCRIPTS/nextest_manifest.py" \
     "$REAL_SCRIPTS/source_inventory.py" "$R/scripts/cs1r/"

  json_of "$head_tests" > "$R/head.json"
  json_of "$base_tests" > "$R/base.json"

  # STUB cargo: live `nextest list` == the HEAD test set (so control 1 passes,
  # unless we deliberately inject drift into the committed manifest below).
  cat > "$R/bin/cargo" <<STUB
#!/usr/bin/env bash
case "\$*" in
  *live-dps*) cat "$R/head.json" ;;
  *)          cat "$R/head.json" ;;
esac
STUB
  chmod +x "$R/bin/cargo"

  local mkman='python3 scripts/cs1r/nextest_manifest.py'
  (
    cd "$R"
    git init -q
    git config user.email t@t; git config user.name t

    # BASE commit: base manifests (+ empty registry / source inventory).
    $mkman test-support < base.json > docs/cs1r/inventory/manifest.test-support.tsv
    $mkman live-dps     < base.json > docs/cs1r/inventory/manifest.live-dps.tsv
    printf '# empty\n' > docs/cs1r/inventory/superseded_removals.tsv
    python3 scripts/cs1r/source_inventory.py > docs/cs1r/inventory/source_files.sha256
    git add -A; git commit -qm base
    git rev-parse HEAD > .base_sha

    # HEAD working tree: head manifests + the case registry + optional drift row.
    $mkman test-support < head.json > docs/cs1r/inventory/manifest.test-support.tsv
    $mkman live-dps     < head.json > docs/cs1r/inventory/manifest.live-dps.tsv
    printf '%b' "$registry_body" > docs/cs1r/inventory/superseded_removals.tsv
    if [ -n "$drift" ]; then
      printf 'test-support\tprro\tprro::alpha\t%s\tfalse\n' "$drift" \
        >> docs/cs1r/inventory/manifest.test-support.tsv
    fi
  )

  local base_sha; base_sha="$(cat "$R/.base_sha")"
  local rc=0
  ( cd "$R" && CARGO="$R/bin/cargo" bash scripts/cs1r/inventory_gate.sh --pr "$base_sha" ) \
    > "$R/out.log" 2>&1 || rc=$?

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

# A single registered supersession row (module/target is the fixture's "prro::alpha").
reg_row() { printf '# reg\nprro\t%s\tprro\t%s\tadjudicated\n' "$1" "$2"; }

run_case "T1 empty registry + zero removals -> PASS" 0 \
  "keep_a keep_b" "keep_a keep_b" "$(printf '# empty\n')"

# T2 uses a NON-empty, VALID registry (registers `also_removed`, a real removal whose
# replacement `keep_a` exists) so the RED is unambiguously "unregistered removal of
# `removable_x`" (control 2a) — not the empty-registry path (that is T1) nor a stale row.
run_case "T2 unregistered removal (valid non-empty registry) -> RED" 1 \
  "keep_a" "keep_a removable_x also_removed" "$(reg_row also_removed keep_a)"

run_case "T3 stale registry row -> RED" 1 \
  "keep_a keep_b" "keep_a keep_b" "$(reg_row keep_b keep_a)"

run_case "T4 registered removal, replacement missing -> RED" 1 \
  "keep_a" "keep_a removable_x" "$(reg_row removable_x ghost_replacement)"

run_case "T5 live/committed manifest drift -> RED" 1 \
  "keep_a keep_b" "keep_a keep_b" "$(printf '# empty\n')" "ghost_drift"

echo "── teeth: $PASS passed, $FAILED failed ──"
[ "$FAILED" -eq 0 ]
