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

Measured on the 2026-08-02/03 runs:

| run | mutants | wall | outcome |
|---|---|---|---|
| a large PR diff (`--in-diff`) | 282 | 2 h 11 min at `-j24` *with trap 1 active* | 271 caught, 0 missed, 0 timeout |
| whole **workspace** (`full`) | 10 548 | 4 h 46 min at `-j64` | see trap 4 — only `prro`'s 3615 were actually tested |
| catch-up of that run's timeouts | 592 | 1 h 57 min at `-j64`, `--timeout-multiplier 12` | 444 caught, **15 missed**, 110 timeout |

The README's "~9200 mutants" and "~8-12 h ≈ €7-9" are stale on both counts.

Per-mutant work is dominated by running the **entire** 2375-test suite, plus a
crate rebuild. Budget accordingly and re-measure rather than trusting a number
from a previous run — throughput swings widely with the mutant mix, and counting
raw completions instead of *tested* ones will mislead you by an order of
magnitude (§6).

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

> 🛑 **This table is RETRACTED.** Every number in it was produced with a shared `CARGO_TARGET_DIR` —
> the configuration where jobs die on `ENOSPC` and "finish" instantly, inflating the counter. The
> `31.4/min` figure in particular is fiction, and every time estimate built on it was wrong.

**Measured on a working configuration** (isolation on, ballast removed, `-j16`, steady state):

| | |
|---|---|
| throughput | **1.3 tested mutants/min** (45 in 34 min) |
| whole `prro` crate (3623 mutants) | **≈ 45 h ≈ €35** |
| load at `-j16` on 48 cores | **460** — each job runs nextest across every core, ~10× oversubscription |
| disk ceiling | 35 GB per job copy ⇒ ~24 copies max on a 902 GB volume |

So the real trade is not "which `--jobs`" — it is that **a full baseline costs two days of rental**
while `prro` is one monolith whose every mutant re-runs all 2375 tests. Until the crate split lands
(a mutant then rebuilds a leaf, not the monolith), a full run is deferred by decision, not by
accident. Pick `--jobs` from `free_disk / 35 GB` and keep a disk watchdog; that is all this knob can
do for you.

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

### Trap 0 — parallel jobs + one `CARGO_TARGET_DIR` = **40 % of the verdicts are wrong**

Read this before `--jobs`. It is the trap that invalidates results rather than merely slowing them.

`cargo-mutants` **honours** an inherited `CARGO_TARGET_DIR` — it does not override it, and the tree
copies it makes contain no target directory of their own (verified: with the variable set, the
copies are bare and everything lands in the one shared path). So with `run.sh` exporting an absolute
path and `-j24…64`, **every job builds and links into the same `debug/`**. A job's test run can
execute a binary another job just linked — or one that was never rebuilt, i.e. **unmutated code**.

`run.sh`'s own header warns about exactly this (bd `PRRO_GATE-9g5`) — for the *dev* target dir. The
export meant to protect it recreates the hazard one level down.

Measured. The 15 survivors from a `-j48` run were re-tested locally with `-j1`, same commit, same
mutants, same multiplier:

| | |
|---|---|
| declared survivors (`-j48`, shared target) | 15 |
| confirmed by the serial run | **9** |
| **false survivors** (serial says *caught*) | **6 — 40 %** |
| real survivors the parallel run **missed** | 2 |

**Wrong in both directions**, so it cannot be dismissed as "merely conservative". And the six false
ones were not harmless noise — they were the alarming ones: `sent_not_found_to_manual` stubbed out,
the fail-closed `"Z_REPORT" | "SHIFT_CLOSE"` arm deleted, inverted conditions in
`dispatch_prepared_via_chain` and `escalate_fn_to_manual_recon`. Every one of them is killed by
existing teeth. A parallel run invents holes exactly where they would frighten you most.

**Do not set `CARGO_TARGET_DIR` at all** — let `cargo-mutants` own the build directory per job. If
you must keep it (to protect a shared dev target), then the run has to be serial, or its verdicts
are not evidence. Cheap way to check the fix before renting anything: mutate one module twice at
`-j4`, with and without the variable, and compare both against a `-j1` baseline.

### Trap 4 — a `full` run silently tests **only `prro`**

`rust/.cargo/mutants.toml` passes `additional_cargo_args = ["--features","test-support"]`, and the
`test-support` feature exists **only on `prro`**. For every sibling crate cargo fails on argument
parsing, so *each of their mutants is recorded as `unviable`* — indistinguishable, in the summary,
from "the mutation didn't compile".

Measured on the 2026-08-03 full run:

| crate | mutants | caught | unviable | timeout |
|---|---|---|---|---|
| `prro` | 3615 | 2996 | 30 | 589 |
| `prro_crypto` | 3253 | 0 | **3253** | 0 |
| `prro_crypto_v2` | 1974 | 0 | **1974** | 0 |
| `maria304_driver` | 720 | 0 | **720** | 0 |
| `prro_sidecar` | 637 | 0 | **637** | 0 |
| the rest | — | 0 | **100 %** | 0 |

**Two hours of a 4 h 46 m run went into crates that could never build.** Pass `--package prro`
explicitly: the gate's scope is already `prro` (CI's `detect mutation-relevant changes` triggers on
`rust/prro/src/`), so this *narrows nothing* — it aligns the run with the gate and cuts the time
roughly threefold. If sibling crates should genuinely be covered, the feature argument has to become
per-package first; today they are silently skipped.

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

- 🔴 **A tight timeout hides survivors *systematically*, and the gate reports `0` while doing it.**
  This is the single most important thing on this page. A mutant nobody kills runs the suite **to
  completion** — no test fails, so there is no early exit — which makes survivors the **slowest**
  mutants and the first to hit the cap. The cap does not lose a random sample; it eats precisely
  what the gate exists to find.

  Measured, same code, same box, same 589 mutants:

  | run | timeout | result |
  |---|---|---|
  | `full`, auto-timeout 582 s | 589 | **`missed = 0`** → baseline rewritten to an **empty file** |
  | catch-up, `--timeout-multiplier 12` | 110 | 444 caught, **15 real survivors** |

  ⚠️ **Those "15" were re-tested serially and only 9 held** — the catch-up itself ran with a shared
  build directory (trap 0), so 6 of them were false. The corrected finding is below; the *mechanism*
  above stands, because the 59 s / 1010 s split is measured within that run and does not depend on
  which mutants were mis-verdicted.

  **What actually survives, after serial re-verification** (`-j1`, same commit, same multiplier):
  9 of the 15, plus 2 the parallel run had missed. Their character is nothing like the first
  reading. Nine are **counters and summary-struct fields** — `ReservationBootSummary::is_active`
  (three variants), the `dispatch_pending_doc` and `run_tick_for_fn` histograms, the `scanned` and
  `fiscal_number` fields, `+= → *=` in `reservation_boot_pass::run`. Diagnostics nobody asserts, the
  same class the July baseline already accepts (`DispatchHistogram::total_visited`). One is
  `&& → ||` in the supervisor's loop condition. Two are `hex_lower` in the operator-handoff audit
  record — and that pair is the only one arguing for a real tooth: `sn01` asserts the
  `SENT_NOT_FOUND_ESCALATED_MANUAL` row **exists** (`COUNT(*)`) but never that it names the right
  document.

  Everything frightening in the first reading — `sent_not_found_to_manual` stubbed to `Ok(())`, the
  deleted `"Z_REPORT" | "SHIFT_CLOSE"` arm, the inverted conditions in `dispatch_prepared_via_chain`
  and `escalate_fn_to_manual_recon`, `normalize_one`, `attempts_used` — **is killed by existing
  teeth**. Verified by hand and again serially.

  > The lesson is sharper than either finding. A tight timeout hides survivors; a shared build
  > directory *invents* them, and it invents them in the scariest places. Neither failure announces
  > itself — both produce a report that reads like a result.

  **So: never read `survivors: 0` as coverage while `timeout.txt` is non-empty.** Re-run the
  timeouts with a far larger budget before believing any verdict, and treat what still times out as
  *not caught*. The 110 that survived `×12` are spread across `boot_phase` (34),
  `reservation_boot_pass` (22), `transport_trace` writes (13), `supervisor` (9),
  `canonical_builder` (9), `backlog_drain` (6). Raising the budget further will not resolve them —
  the same mutants timed out at 582 s and at ~1450 s.

  > The shared shape *appears* to be "a stubbed return means something a test awaits never happens"
  > — a server that never binds, a boot pass that never converges, a request that never completes.
  > **Inferred from the distribution, not verified**: confirming it means reading each call site.

- **`TIMEOUT` outcomes are invisible to the ratchet — and a `full` run will silently
  drop them from the baseline.** They land in `timeout.txt`, never in `missed.txt`,
  and `MODE=full` refreshes the baseline with `cp missed.txt survivors.txt`. So a
  known survivor that *hangs* the suite in the new run instead of passing it
  **disappears from the ratchet's record**, though nothing killed it — and will
  later reappear as a "NEW survivor" once it stops hanging.

  Measured mid-run on 2026-08-02 (`main@74dc4df`, 15 % through): 157 baseline
  survivors, 64 timeouts, **4 of them the same mutant** — `outgress.rs` `wrap` /
  `submit`, `supervisor.rs:315` `replace && with ||`. That count grows with the run.

  > **Before committing a refreshed baseline:** diff the new `timeout.txt` against
  > the *old* `survivors.txt` and rule on every intersection by hand. A mutant that
  > hangs the suite is not caught; it is at least as dangerous as one that passes,
  > and it belongs in the record (or gets teeth).

  Timeouts cluster where a stub breaks a loop's exit condition — `runtime/outgress`,
  `runtime/coding`, `runtime/supervisor`. They are not cheap: each burns the full
  timeout (~582-680 s) of a job slot.
- **`🟢 CLOSED since baseline` lies in `diff` mode.** `run.sh` computes it as
  `comm -13 <this run's survivors> <baseline>`, so in a diff run it lists every
  baseline entry **outside the diff** — mutants that were never tested — as
  "closed". The gate verdict itself is unaffected (`NEW = comm -23` is correct, and
  the baseline is only rewritten by `MODE=full`), but acting on that list would
  prune real survivors from the ratchet's record without re-verifying them.
