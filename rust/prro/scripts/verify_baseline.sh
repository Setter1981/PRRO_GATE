#!/usr/bin/env bash
# ============================================================================
# verify_baseline.sh — equivalence gate for the 001_baseline.sql squash.
#
# Proves: (old 001-024 migration chain) ≡ (001_baseline.sql) at the
# sqlite_master level.  The diff MUST be EMPTY.  A non-empty diff means STOP —
# do NOT "normalise" the baseline; the generation method (squash spec §2:
# baseline = filtered .schema of the fully-migrated chain) guarantees an empty
# diff by construction, so any difference is a real defect to escalate.
#
# Method: build DB-A by applying the old chain (materialised from git at the
# pre-squash ref) and DB-B by applying the new baseline, then diff a sorted
# sqlite_master dump of each (curated `--` comments live outside CREATE
# statements, so they never enter sqlite_master and cannot affect this gate).
#
# Usage:
#   rust/prro/scripts/verify_baseline.sh [PRE_SQUASH_REF]
#     PRE_SQUASH_REF  git ref that still carries the 24-file chain.
#                     Default: HEAD~1 (the parent of the squash commit).
#                     When running BEFORE committing the squash, pass HEAD.
#
# Requires: bash, git, sqlite3.
# ============================================================================
set -euo pipefail

REF="${1:-HEAD~1}"
ROOT="$(git rev-parse --show-toplevel)"
BASELINE="$ROOT/rust/prro/migrations/001_baseline.sql"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

DUMP="SELECT type, name, tbl_name, sql FROM sqlite_master
 WHERE name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations'
 ORDER BY type, name;"

# ── DB-A: the OLD chain, materialised from git at $REF, applied in version order.
mkdir -p "$TMP/old"
mapfile -t OLD < <(git ls-tree --name-only "$REF" -- rust/prro/migrations/ \
                     | grep -E '/[0-9]+_.*\.sql$' \
                     | grep -v '/001_baseline\.sql$' \
                     | sort -V)
if [ "${#OLD[@]}" -eq 0 ]; then
  echo "✗ no old migration chain found at ref '$REF' — wrong PRE_SQUASH_REF?" >&2
  exit 2
fi
for f in "${OLD[@]}"; do
  git show "$REF:$f" > "$TMP/old/$(basename "$f")"
done
for f in $(ls "$TMP/old"/*.sql | sort -V); do
  sqlite3 "$TMP/a.db" < "$f"
done

# ── DB-B: the new baseline.
sqlite3 "$TMP/b.db" < "$BASELINE"

sqlite3 "$TMP/a.db" "$DUMP" > "$TMP/a.schema.txt"
sqlite3 "$TMP/b.db" "$DUMP" > "$TMP/b.schema.txt"

# ── DATA gate (architect amendment 2026-06-11): a filtered `.dump` of each DB
# carries BOTH the schema and every INSERT row, so it catches seed-data drift
# (the .schema-only baseline method silently dropped the 003/006 seeds).
# `.dump` emits objects in creation (rowid) order — deterministic per DB, and
# identical between A and B because the baseline was generated in the chain's
# rowid order.  `_sqlx_migrations` is filtered out; both DBs here are built by
# sqlite3-apply so it is absent anyway (defensive).  Volatile
# `DEFAULT (CURRENT_TIMESTAMP)` / `…'now'` values are normalised to <TS>: the
# two DBs are built seconds apart, so seed-row auto-timestamps differ by build
# time, NOT by content — this gate compares the meaningful seed columns.
TS_NORM='s/[0-9]{4}-[0-9]{2}-[0-9]{2}[T ][0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z?/<TS>/g'
sqlite3 "$TMP/a.db" ".dump" | grep -v '_sqlx_migrations' | sed -E "$TS_NORM" > "$TMP/a.dump.txt"
sqlite3 "$TMP/b.db" ".dump" | grep -v '_sqlx_migrations' | sed -E "$TS_NORM" > "$TMP/b.dump.txt"

echo "old chain: ${#OLD[@]} files @ $REF   |   schema rows: $(wc -l < "$TMP/b.schema.txt")   |   dump lines: $(wc -l < "$TMP/b.dump.txt")"

rc=0
echo "── gate 1/2: sqlite_master (schema) diff ──"
if diff -u "$TMP/a.schema.txt" "$TMP/b.schema.txt"; then
  echo "✓ schema diff EMPTY."
else
  echo "✗ schema diff NON-EMPTY — STOP. Do NOT normalise; escalate (spec §3.4)." >&2
  rc=1
fi

echo "── gate 2/2: filtered .dump (schema + data) diff ──"
if diff -u "$TMP/a.dump.txt" "$TMP/b.dump.txt"; then
  echo "✓ data diff EMPTY (seeds + every row reproduced)."
else
  echo "✗ data diff NON-EMPTY — STOP. A seed/INSERT row diverged; escalate (spec §3.4 + 2026-06-11 amendment)." >&2
  rc=1
fi

if [ "$rc" -eq 0 ]; then
  echo "✓✓ BOTH DIFFS EMPTY — 001_baseline.sql reproduces the old chain ($REF) schema AND data."
fi
exit "$rc"
