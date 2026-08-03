# Mutation testing (FW-1) — the "who tests the tests" gate

Mutation testing perturbs the production code (flip a `<` to `<=`, drop a `?`,
replace a return value…) and checks whether **the test suite catches it**. A
mutant the tests kill = teeth that bite. A **survivor** = a place the code could
be wrong and no test would notice = an actionable teeth-gap. It is the empirical
answer to "are our green tests real, or vacuous?".

Tool: [`cargo-mutants`](https://mutants.rs). Config lives in
`rust/.cargo/mutants.toml` (builds with `--features test-support`, runs via
nextest — matching the CI gate).

## The mutant database (why we don't re-run in rounds)

The whole crate is ~9200 mutants — too many to re-run every change. So the
database under **`docs/mutation/baseline/`** is committed and consulted:

| file | what |
|---|---|
| `survivors.txt` | mutants **no test kills** — the teeth-gaps to close |
| `outcomes.json` | every mutant + its verdict (full machine-readable record) |
| `mutants.json`  | the mutant catalog |

Two tiers, driven by `scripts/mutation/run.sh`:

- **`full`** — the whole crate. Slow; run on a rented server (see below). It
  **refreshes** the committed baseline. Rare (once, then periodically).
- **`diff`** *(default)* — only mutants in `git diff origin/main` via
  `cargo mutants --in-diff`. Fast; each change tests only its own new/changed
  code. **Unchanged code is never re-mutated** — its mutants were already killed
  and the code didn't change. This is how we avoid "running them in rounds".
- **`file:<path>`** — one module, e.g. `file:services/write_path/error_routing.rs`.

`run.sh` diffs the run against the baseline and reports:
- 🔴 **NEW survivors** — uncaught mutants in changed code → add teeth (gate: exit 1).
- 🟢 **CLOSED** — a former survivor now killed (teeth added) or code removed.
  ⚠️ **Only meaningful in `full` mode.** In `diff` mode this is
  `comm -13 <this run's survivors> <baseline>`, so it lists every baseline entry
  *outside the diff* — mutants that were never tested — as "closed". The verdict
  itself is unaffected and the baseline is only rewritten by `full`, but do not
  prune `survivors.txt` from that list (bd `PRRO_GATE-a0d`).

## How to run

```bash
# incremental (default) — fast, only your diff vs main:
scripts/mutation/run.sh diff

# one module:
scripts/mutation/run.sh file:services/cash_ledger.rs

# whole crate (needs a beefy box — refreshes the baseline):
scripts/mutation/run.sh full 40
```

### Full run on a rented server

📖 **Read [`RUNBOOK-rented-box.md`](RUNBOOK-rented-box.md) before the first run.**
It carries the measured settings and three traps that each cost real box-hours.

Recommended: **Hetzner Cloud CCX63** (48 vCPU / 192 GB / 960 GB, hourly, **not
spot → no interruptions**, ~€0.7/h).

```bash
# on the fresh Ubuntu VM (as root):
REF=main JOBS=64 SCOPE=full bash scripts/mutation/bootstrap-vm.sh
# → installs rust + cargo-mutants + nextest + mold + sccache, clones, runs,
#   refreshes docs/mutation/baseline/. scp the baseline back + commit. Delete VM.
```

`bootstrap-vm.sh` needs **zero secrets** — the suite uses the DetCrypto stub and
in-memory SQLite; no JKS password / live DPS / keys ever reach the box.

Two corrections to what this section used to claim, both measured 2026-08-02:

- **A fresh VM does *not* free you from the shared `target-dir`.** `run.sh` itself
  exports `CARGO_TARGET_DIR="$ROOT/rust/target-mutants"` unconditionally, on every
  machine — and because that path is *inside* the tree and is not named `target`,
  `cargo-mutants` copies the whole build directory into every job (~40 GB each; at
  `JOBS=24` that fills a 902 GB disk). Move it outside the tree first — runbook §5.
- **`JOBS=40` is not the sweet spot and `8-12 h ≈ €7-9` is optimistic.** Measured
  throughput at `-j64` was 6× that of `-j24`; the whole crate is **10 548** mutants
  (not ~9200), and per-mutant cost is dominated by running the entire 2375-test
  suite. Re-measure per run — runbook §4 and §6.

## History / seed

- **error_routing.rs** (2026-07-13, scoped pilot on WSL): 27 mutants, 23 caught,
  4 unviable, **0 survived** (100 % kill-rate — the S3 reject corpus + byzantine +
  routing tests catch every viable mutation). First clean baseline entry.

The full-crate baseline is seeded by the first server run.
