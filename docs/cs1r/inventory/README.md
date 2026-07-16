# CS-1R R1.2 — forward additions-only inventory gate (artifact)

**Spec:** `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §4 **R1.2**.
**Machinery:** `scripts/cs1r/{inventory_gate.sh, mint_manifests.sh, nextest_manifest.py, source_inventory.py}`.

## Committed manifests (minted at CS-1R head)

| file | rows | what |
|---|---|---|
| `manifest.test-support.tsv` | 3106 | `nextest list` for profile `test-support` |
| `manifest.live-dps.tsv` | 3120 | `nextest list` for profile `test-support,live-dps` |
| `source_files.sha256` | 352 | SHA of every in-scope source test file (R1.4) |

Identity row = `{profile}\t{package}\t{target}\t{test_name}\t{ignored}` — **profile is part of
identity** (moving a test between profiles is a delete+add, not a no-op). `target` is the nextest
`binary-id` (worktree-independent); output is sorted and deterministic.

The **two literal profile commands** (verbatim, spec §4 R1.2):

```
# profile = "test-support"
cargo nextest list --workspace --features prro/test-support \
  --message-format json --locked --target x86_64-unknown-linux-gnu
# profile = "live-dps"
cargo nextest list --workspace --features prro/test-support,prro/live-dps \
  --message-format json --locked --target x86_64-unknown-linux-gnu
```

`cargo-nextest` is pinned to **0.9.137** in CI; `cargo` is `1.95.0` via `rust-toolchain.toml`.

## The three-way control the gate enforces

1. **live == committed** (per profile) — no drift.
2. **additions-only vs base** (`--pr <base-ref>`) — a removed identity row (delete / rename /
   `#[ignore]` of an existing test) is forbidden.
3. **every new source test file present in `source_files.sha256` in the same PR** — a new test
   cannot be added-then-silently-deleted.

## Re-minting

Human-run only (like `cargo xtask update-purity-closure`); **CI never auto-mints**:

```
scripts/cs1r/mint_manifests.sh
```

A drift is meant to turn the gate RED so a human reviews and re-mints. Re-minting is legitimate only
when ADDING tests — the gate's additions-only control still bites on any removal at PR time.

## RP-R1-1 (teeth — empirically verified, 2026-07-16)

Each mutation makes the gate RED (each reverted after):

| tooth | mutation | RED how |
|---|---|---|
| `#[ignore]` an existing test | `#[ignore]` on `models_smoke::document_id_roundtrip_bytes` | control 1: live row `ignored` flips `false`→`true`, `live != committed` |
| add a test absent from the manifest | append `cs1r_rp_r1_1_new_test_absent_from_manifest` to `models_smoke.rs` | control 1: an extra live row not in the committed manifest |

(A delete/rename is the same class: control 1 sees the missing/renamed row live, and control 2
`--pr` rejects the removal vs base.)
