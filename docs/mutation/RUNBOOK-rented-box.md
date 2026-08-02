# Runbook — mutation run on a rented box

Operational companion to [`README.md`](README.md). Everything here was **measured**
on 2026-08-02 (Hetzner CCX63, 48 vCPU / 184 GB usable / 902 GB disk, Ubuntu 26.04)
while unblocking PR #377; where something is inferred rather than measured, it
says so.

Read this **before** the first run. Three of its traps each cost real box-hours.

---

## 1. When you need this at all

The per-PR `mutation diff-gate` runs `cargo mutants --in-diff` against
`origin/main` on a hosted runner with `timeout-minutes: 45`. That budget is sized
for small diffs. A branch that touches fiscal-logic `src/` broadly blows it:

| PR #377 (7 commits, ~270 changed src lines) | |
|---|---|
| mutants in the diff | **282** |
| CI job outcome | `cancelled` at 45m19s — **timeout, not a survivor** |
| `missed.txt` in CI | never written (the run never reached a verdict) |

A cancelled gate is **not** a verdict. Do not read it as "clean" and do not read
it as "failed" — nothing was decided. Rerun it somewhere without the cap, then
attach the artifacts to the PR.

`mutation diff-gate` is a **required** check on `main`
(`fmt + clippy (gnu)`, `x86_64-unknown-linux-gnu`, `mutation diff-gate`), so this
blocks merge until it is resolved.

---

## 2. Cost and duration — corrected

The README's older estimate (`~8-12 h ≈ €7-9` for the whole crate) is optimistic.
Measured reality:

| scope | mutants | notes |
|---|---|---|
| a large PR diff | 282 | **2 h 11 min** at `JOBS=24` *with the target-dir trap below* |
| whole crate | **10 548** | README says "~9200" — stale |

Per-mutant work is dominated by running the **entire** 2375-test suite, plus a
crate rebuild. Budget accordingly and re-measure rather than trusting a number
from a previous run — throughput swings widely with the mutant mix.

---

## 3. The recipe

`bootstrap-vm.sh` installs the toolchain (rust, cargo-mutants, nextest, mold,
sccache), clones, and runs. It needs **zero secrets** — the suite uses the
DetCrypto stub and SQLite; no JKS password, no live DPS, no keys reach the box.
Use a **dedicated** SSH key, not your personal one; the box is disposable.

For anything past the first run, drive `run.sh` directly — re-cloning and
re-installing the toolchain each time is pure waste:

```bash
# environment for every subsequent run on the box
export CARGO_HOME=/mnt/mutants/cargo RUSTUP_HOME=/mnt/mutants/rustup \
       TMPDIR=/mnt/mutants/tmp SCCACHE_DIR=/mnt/mutants/sccache
export PATH="$CARGO_HOME/bin:$PATH"
export RUSTC_WRAPPER="$(command -v sccache || true)"
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"

# MANDATORY — see trap 1
sed -i 's|^export CARGO_TARGET_DIR=.*|export CARGO_TARGET_DIR="/mnt/mutants/target-mutants"|' \
    scripts/mutation/run.sh

# detach from ssh: the run outlives the session
setsid nohup bash -c 'time bash scripts/mutation/run.sh full 64' \
    > /root/full.out 2>&1 < /dev/null &
```

Bring back `mutants.out/missed.txt` and `mutants.out/outcomes.json`; for
`SCOPE=full` also the refreshed `docs/mutation/baseline/`. **Then delete the VM** —
it bills by the hour.

---

## 4. `JOBS` — measured, and against the tool's own advice

`cargo-mutants` warns that `--jobs` above 8 "may overload your machine". For
**this** workload on a 48-vCPU box that advice is actively harmful.

| config | tested mutants/min | CPU |
|---|---|---|
| `-j8` + `CARGO_BUILD_JOBS=6` + `NEXTEST_TEST_THREADS=6` | 1.62 | `id=96` — **idle** |
| `-j24` | 5.1 | `id=90` — **idle** |
| **`-j64`** | **31.4** | `us=73 sy=27 id=0` |

Why the intuition fails: the bottleneck is **slots, not cores**. A mutant that
hangs the suite sleeps until the test timeout (auto-set to ~680 s from the
baseline) — it holds a job slot but burns no CPU. More slots put the idle cores
back to work. Capping each job's *internal* parallelism is the opposite of what
helps: `--test-threads=6` stretches the dominant cost (the whole suite) by nearly
an order of magnitude, and 8 jobs cannot fill 48 cores.

Memory is not the constraint: 15 GB of 184 GB at `-j64`.

> Throughput **swings** (11–31/min observed within one run). Measure over a long
> window, never a short one.

---

## 5. Three traps

### Trap 1 — `CARGO_TARGET_DIR` lives inside the repo

`run.sh` exports `CARGO_TARGET_DIR="$ROOT/rust/target-mutants"` — **inside the
tree**, unconditionally, on every machine. `cargo-mutants` copies the source tree
per job and excludes a directory named `target`; `target-mutants` does **not**
match that exclusion, so the entire build directory is copied into every job.

Measured: ~40 GB per copy (`Copied source tree total_bytes=16845405100` locally,
the same shape on the box). At `JOBS=24` that is ~900 GB against a 902 GB disk —
**the disk fills completely**, and jobs then stall on `ENOSPC` while looking, from
the outside, merely "idle".

Move it outside the tree (step 3 above). Semantics are unchanged — the path was
already absolute and shared across jobs; only the copy size changes.

> ⚠️ **Hypothesis, not yet proven:** this may also be the dominant cost of the CI
> gate, in which case the fix is a one-line path change rather than narrowing the
> per-mutant test set. Not confirmed — it needs a clean A/B (`--in-diff` twice, path
> inside vs outside). Tracked in bd `PRRO_GATE-a0d`. **Do not narrow the test set
> before running that A/B**: a too-narrow filter skips the test that *would* have
> killed a mutant and reports a **false survivor**, and false survivors get answered
> with exclusions — which is how a gate gets hollowed out.

### Trap 2 — `kill -9` leaves 40 GB behind, every time

`cargo-mutants` removes its tree copies on a clean exit only — `tempfile` cleans on
`Drop`, never on `SIGKILL`. Four kills during one session left **21 orphaned copies
= 855 GB** in `/root/.cache/prro-mutants-scratch`.

```bash
find /root/.cache/prro-mutants-scratch -mindepth 1 -depth -delete
```

Also: `pkill -x cargo-mutants` does **not** reliably kill it. Use `-9` and then
*verify* the process is gone instead of trusting the exit code.

### Trap 3 — `pkill -f` matches its own command line

`pkill -f calib.sh` issued from a command whose own command line contains
`calib.sh` kills the shell running it. Everything after it silently does not run.
The tell is an **empty** output where you expected a confirmation.

Use `pgrep -f "cal[i]b.sh"` (the bracket keeps the literal out of your own
command line) or kill by PID.

---

## 6. Measuring throughput without being wrong by an order of magnitude

Count **only tested** mutants — `caught + missed + timeout`. **Exclude
`unviable`.**

Unviable mutants (~730 of 10 548) do not compile, are rejected without running a
single test, and cluster at the start of a run. Counting them produced an apparent
"80 mutants/min → ETA 2 h" when the real tested rate was 5/min and the honest ETA
was an order of magnitude larger.

Two more diagnostic rules learned the hard way:

- **Do not diagnose from `load average`.** It counts uninterruptible-sleep (I/O)
  processes too. A load of 215 on 48 cores looked like the box was wedged; `id` was
  0.5 % and it was simply busy. Read `us/sy/id/wa` from `vmstat`/`top`.
- **Check the disk before trusting a measurement.** A `-j` comparison taken while
  the disk was full showed "85 % idle" that was `ENOSPC`, not parallelism.

---

## 7. What to do with the results

`missed.txt` is the actionable set: mutants **no test kills**. Per the FW-1
ratchet, a PR may introduce **no new survivor**. Each one is resolved one of two
ways, and both are recorded:

1. **Killed with a teeth test** — preferred. Prove it empirically: apply the
   mutation by hand, watch the test go **RED**, revert, watch it go **GREEN**. A
   test that has not been seen to fail under its mutation is not teeth; a toothless
   test passes on correct code just as happily.
2. **Adjudicated and accepted** — with a one-line rationale in
   `docs/mutation/baseline/survivors.txt`:
   - *EQUIVALENT* — the mutation changes the text but not the behaviour (an
     unread counter, a discarded return). **No test can ever kill it**; demanding
     teeth is meaningless.
   - *LOW* — behaviour changes only on a dead or diagnostic path.

   `rust/.cargo/mutants.toml` `exclude_re` is for genuinely dead / unreachable /
   test-only code — **never** to silence a real gap.

Before writing a test, confirm the survivor is actually reachable: a plausible
story about how it bites is often wrong (a guard upstream masks it, the value is
discarded). An adjudication needs a note, not a test.

---

## 8. Known gaps in the tooling (bd `PRRO_GATE-a0d`)

- **`TIMEOUT` outcomes are invisible to the ratchet.** They land in `timeout.txt`,
  not `missed.txt`. A mutant that *hangs the suite* is therefore neither caught nor
  reported — and at `JOBS=24` timeouts were 16 % of tested mutants, each burning
  ~680 s of a slot.
- **`🟢 CLOSED since baseline` lies in `diff` mode.** `run.sh` computes it as
  `comm -13 <this run's survivors> <baseline>`, so in a diff run it lists every
  baseline entry **outside the diff** — mutants that were never tested — as
  "closed". The gate verdict itself is unaffected (`NEW = comm -23` is correct, and
  the baseline is only rewritten by `MODE=full`), but acting on that list would
  prune real survivors from the ratchet's record without re-verifying them.
