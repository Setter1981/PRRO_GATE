#!/usr/bin/env bash
#
# Phase-2 U2 (spec §4 / F2) — refuse to silently drop an invariant-fuzzer find.
#
# The fuzzer capstone pins its proptest regression corpus to ONE committed file
# (see the capstone `proptest_config` in `rust/prro/tests/invariant_fuzzer.rs`).
# On a find, proptest writes the minimal failing seed there. On an ephemeral CI
# runner that seed is LOST unless committed — so a real bug the fuzzer caught
# would silently vanish. This guard fails whenever a fuzzer run leaves that seed
# UNCOMMITTED or UNTRACKED, forcing it to be committed (= a permanent regression
# that replays first on every later run).
#
# We use `git status --porcelain` (NOT `git diff --exit-code`) deliberately:
# `git diff` does not report NEWLY-CREATED (untracked) files — exactly the
# first-find silent-drop case this guard must close. `--porcelain` additionally
# reports a shrink-rewrite of an already-tracked seed (status `M`).
set -euo pipefail

SEED="rust/prro/tests/invariant_fuzzer.regressions"

# Resolve relative to the repo root so the check is cwd-independent.
cd "$(git rev-parse --show-toplevel)"

status="$(git status --porcelain -- "$SEED")"
if [ -n "$status" ]; then
  echo "FAIL: invariant-fuzzer regression seed is uncommitted or untracked:" >&2
  echo "$status" >&2
  echo >&2
  echo "A fuzzer run wrote (or rewrote) a seed at:" >&2
  echo "    $SEED" >&2
  echo "Commit it to pin the case as a permanent regression:" >&2
  echo "    git add $SEED && git commit -m 'test(fuzzer): pin regression seed'" >&2
  exit 1
fi

echo "OK: no uncommitted/untracked invariant-fuzzer regression seed."
