#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Turnkey mutation run on a FRESH Linux VM (Hetzner CCX / AWS c6id / any Ubuntu).
#
# A fresh VM has no forced shared target-dir, so cargo-mutants `-j` parallelism
# works out of the box (the WSL dev box's ~/.cargo target-dir force — which broke
# -j — is absent here). Combined with the committed -j unblock (PR #273: the ADR
# runtime-include fix + rust/.cargo/mutants.toml), this box runs the full crate
# in parallel and seeds the mutation database.
#
# Run as root (or with sudo) on Ubuntu 22.04/24.04:
#   REF=main JOBS=40 SCOPE=full bash bootstrap-vm.sh
#
# Env knobs (all optional):
#   REPO   git url                (default: the PRRO_GATE origin)
#   REF    branch/tag/sha         (default: main)
#   JOBS   cargo-mutants -j       (default: nproc; on 32-48 vCPU use ~nproc)
#   SCOPE  full | file:<path>     (default: full)
#   WORK   scratch dir            (default: largest mount, or /mnt/mutants)
#
# Cost sanity (Hetzner CCX63, 48 vCPU): whole crate ~8-12h ≈ €7-9. Not spot, so
# no interruptions — create, run, `hcloud server delete` when the report is out.
#
# SECURITY: this needs ZERO secrets. The suite runs on the DetCrypto stub +
# in-memory SQLite; no JKS password, no live DPS, no keys ever touch this box.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

REPO="${REPO:-https://github.com/Setter1981/PRRO_GATE.git}"
REF="${REF:-main}"
JOBS="${JOBS:-$(nproc)}"
SCOPE="${SCOPE:-full}"

# ── scratch on the biggest available disk (local NVMe on AWS c6id; the root disk
#    on Hetzner CCX is already large enough) ──────────────────────────────────
if [ -z "${WORK:-}" ]; then
  # prefer an extra local NVMe if present + unmounted (AWS c6id: /dev/nvme1n1)
  if [ -b /dev/nvme1n1 ] && ! findmnt -S /dev/nvme1n1 >/dev/null 2>&1; then
    mkfs.ext4 -F /dev/nvme1n1 && mkdir -p /mnt/nvme && mount /dev/nvme1n1 /mnt/nvme
    WORK=/mnt/nvme/mutants
  else
    WORK=/mnt/mutants
  fi
fi
mkdir -p "$WORK"
export TMPDIR="$WORK/tmp";      mkdir -p "$TMPDIR"
export CARGO_HOME="$WORK/cargo"
export RUSTUP_HOME="$WORK/rustup"
export SCCACHE_DIR="$WORK/sccache"

echo ">>> scratch: $WORK  (TMPDIR=$TMPDIR, CARGO_HOME=$CARGO_HOME)"
df -h "$WORK" | tail -1

# ── OS deps ─────────────────────────────────────────────────────────────────
export DEBIAN_FRONTEND=noninteractive
apt-get update -y
apt-get install -y build-essential git curl clang mold pkg-config libssl-dev jq

# ── rust (rust-toolchain.toml in the repo pins the exact version) ───────────
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
fi
export PATH="$CARGO_HOME/bin:$PATH"

# ── tooling ─────────────────────────────────────────────────────────────────
cargo install cargo-nextest --locked 2>/dev/null || true
cargo install cargo-mutants --locked 2>/dev/null || true
cargo install sccache --locked 2>/dev/null || true
export RUSTC_WRAPPER="$(command -v sccache || true)"
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"

# ── clone + checkout ────────────────────────────────────────────────────────
cd "$WORK"
rm -rf PRRO_GATE
git clone --filter=blob:none "$REPO" PRRO_GATE
cd PRRO_GATE
git checkout "$REF"

# ── run (delegates to the shared runner, which also refreshes the baseline on
#    SCOPE=full and prints the survivor summary) ──────────────────────────────
echo ">>> starting mutation run: SCOPE=$SCOPE JOBS=$JOBS REF=$REF"
time bash scripts/mutation/run.sh "$SCOPE" "$JOBS" || true

echo ""
echo ">>> DONE. Artifacts:"
echo "    survivors : $WORK/PRRO_GATE/rust/mutants.out/missed.txt"
echo "    outcomes  : $WORK/PRRO_GATE/rust/mutants.out/outcomes.json"
echo "    refreshed baseline (SCOPE=full): docs/mutation/baseline/  — scp it back + commit"
echo ">>> Remember to DELETE the VM to stop billing."
