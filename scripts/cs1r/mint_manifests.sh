#!/usr/bin/env bash
# CS-1R R1.2 — (re)mint the committed forward-inventory manifests at the CURRENT
# head. Human-run ONLY (like `cargo xtask update-purity-closure`); CI never
# auto-mints — a drift is meant to turn the gate RED so a human reviews and
# re-mints here. Regenerating is legitimate only when ADDING tests (the gate's
# additions-only control still bites on any removal at PR time).
#
# Usage: scripts/cs1r/mint_manifests.sh
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
CARGO="${CARGO:-cargo}"
TARGET="x86_64-unknown-linux-gnu"
INV="docs/cs1r/inventory"

echo "minting test-support manifest…"
( cd rust && "$CARGO" nextest list --workspace --features prro/test-support \
    --message-format json --locked --target "$TARGET" ) \
  | python3 scripts/cs1r/nextest_manifest.py test-support > "$INV/manifest.test-support.tsv"

echo "minting live-dps manifest…"
( cd rust && "$CARGO" nextest list --workspace --features prro/test-support,prro/live-dps \
    --message-format json --locked --target "$TARGET" ) \
  | python3 scripts/cs1r/nextest_manifest.py live-dps > "$INV/manifest.live-dps.tsv"

echo "minting source inventory…"
python3 scripts/cs1r/source_inventory.py > "$INV/source_files.sha256"

echo "minted:"
wc -l "$INV/manifest.test-support.tsv" "$INV/manifest.live-dps.tsv" "$INV/source_files.sha256"
