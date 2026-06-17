# Teeth test — proving the invariant fuzzer bites (AUD-K8-1)

A fuzzer that never fails is indistinguishable from one that does nothing. This
file is the durable, repeatable proof that the Phase-0 invariant fuzzer actually
**catches a real, planted regression** — and the record of the run that did.

The planted defect is the **AUD-K8-1 re-entry guard**: a drain re-tick on a
`RequiresManualReconciliation` (RMR) fiscal number must be a **no-op**. The guard
halts it; without the guard the drain re-enters, the REJECTED predecessor has
left the candidate cohort, and the orphaned successor becomes the head and is
**re-sent** — defeating the escalation's "durable operator surface, halts FN
drain" contract (double-fiscalisation risk).

## Revert target

`prro/src/services/offline_sync/backlog_drain.rs` — Step 1b, the re-entry guard
(currently at **lines 725-727**):

```rust
if ns.shift_state == ShiftState::RequiresManualReconciliation {
    return Ok(DrainSummary::new(fiscal_number.to_string(), 0));
}
```

To demonstrate the teeth, **delete (or comment out) those three lines** — and
restore them afterwards. The repository must ship with the guard PRESENT; this
is a manual demonstration, never a committed state.

> Line numbers drift. Confirm with:
> `grep -n "manual-reconciliation re-entry guard (AUD-K8-1)" prro/src/services/offline_sync/backlog_drain.rs`

## How the teeth bite — and why it is MODE-INDEPENDENT

Detection counts **wire calls**, not a scan:

> A drain re-tick on an RMR FN must make **no new `send_chk`**. With the guard it
> is a no-op (zero wire calls); without it the drain re-drives the orphaned
> backlog → a fresh `send_chk`.

This matters because of the harness's **SETTLED-mode scan gate** (architect
decision, 2026-06-16): the ledger `assert_clean` / mirror scan runs ONLY in a
SETTLED mode `{Online, Offline}`, never mid-transition. A reverted re-drive rests
in `GoingOnline`, where the scan is *suppressed* — so a scan-based teeth would be
blunted by the gate. The wire-call (bounded-postcond) teeth is **independent of
mode**, so it bites regardless. (See
`scan_gate_suppresses_going_online_transient_then_clean_on_settle` for the
companion proof that the gate suppression is load-bearing, not vacuous, and that
a genuinely-stuck doc is still caught post-settle.)

The teeth live in **two** places, both mode-independent:

1. **Deterministic canary** — `teeth_aud_k8_1_rmr_redrive_makes_no_new_wire_call`
   (now a CI gate — un-`#[ignore]`d 2026-06-17; PASSES on main, FAILS on revert).
2. **Property harness** — `harness_offline_seeded` carries the same
   wire-call invariant (`shift_before == RMR ⇒ send_calls unchanged`), so the
   random search finds the same class of defect and **shrinks** it.

## Run it

```bash
# 1) Baseline (guard PRESENT): both must be GREEN.  (Canary is now a normal CI test — no --run-ignored.)
cargo nextest run -p prro --features test-support \
  -E 'binary(invariant_fuzzer) and test(teeth_aud_k8_1)'
cargo nextest run -p prro --features test-support \
  -E 'binary(invariant_fuzzer) and test(harness_offline_seeded)'

# 2) Revert the three guard lines above, then re-run BOTH — they must FAIL.
# 3) Restore the guard, re-run — GREEN again.
```

> Note: the harness fixtures leak per-case temp DBs (`std::mem::forget` in
> `interp.rs`). On a RAM-backed `/tmp`, point `TMPDIR` at a disk path for the
> 256-case harness runs, e.g. `TMPDIR=$PWD/.fuzz_tmp cargo nextest ...`, and
> remove it afterwards. (Tracked as a follow-up; see the Task 7 report.)

## Expected finding

**Deterministic canary** — with the guard reverted:

```
thread 'teeth_aud_k8_1_rmr_redrive_makes_no_new_wire_call' panicked:
assertion `left == right` failed: AUD-K8-1: a drain re-tick on an RMR FN must
make NO new wire call. ... the backlog_drain.rs:725 re-entry guard is missing.
  left: 2
 right: 1
```

**Property harness** — with the guard reverted, the run below found the defect
and shrank it to a minimal 4-op repro:

```
thread 'harness_offline_seeded' panicked:
assertion `left == right` failed: AUD-K8-1: op GoOnline(DpsScript([Ack, Ack]))
on an RMR FN made a NEW wire send — the drain re-entry guard
(backlog_drain.rs:725) must halt a re-tick on a manual-reconciliation FN
  left: 2
 right: 1

minimal failing input: ops = [
    OnlineSell(DpsScript([Ack, Ack])),   // offline-origin (node is Offline) → OFFLINE_LOCAL_ACK backlog doc 1
    Crash(Send),                         // offline node: no wire reached → completes as offline sell → backlog doc 2
    GoOnline(DpsScript([BadHashPrev])),  // probe → GoingOnline; drain head hits -12 → escalates shift → RMR (drain halts)
    GoOnline(DpsScript([Ack, Ack])),     // shift_before == RMR: WITH guard a no-op; WITHOUT it the orphaned successor is re-sent
]
```

The shrink is faithful to the AUD-K8-1 mechanism: build an offline backlog →
escalate the FN to RMR via a rejecting/MAC-failing drain → re-tick the drain. The
guard's job is to make that final re-tick inert; the fuzzer proves it does.

---

# Teeth test #2 — P1 boot-resume `CodePoolExhausted` abort

The second planted-regression proof, for the **P1 boot-resume abort** (the boot
twin of fix #192). On boot, a post-sign offline-ack refusal with the TERMINAL
`CodePoolExhausted` cause must abort a dangling `SIGNED` doc (`SIGNED → Aborted`);
without it the doc rests non-terminal and a later online resurrection would
wrongly ISSUE a check refused at offline-ack time (ledger-only pin + Frozen
Invariant #8).

## Revert target

`prro/src/services/reconciliation/boot_phase.rs` — the two `OfflineAckOutcome::
Refused` arms (PREPARED-resume arc ~3514 + SIGNED-resume arc ~3745):

```rust
if matches!(reason, RefusalReason::CodePoolExhausted) {
    if let Err(e) = abort_signed_on_offline_code_exhaustion(pool, doc_id).await { … }
}
```

To demonstrate the teeth, **disable both arms** (e.g. `if false && matches!(…)`)
— and restore afterwards. The repository must ship with the abort PRESENT.

> Confirm with:
> `grep -n "RefusalReason::CodePoolExhausted" prro/src/services/reconciliation/boot_phase.rs`

## How the teeth bite

A `Crash(Sign)` commits a `SIGNED` doc and stops before dispatch (the
crash-after-sign window); the next `Reboot` drives boot reconciliation on an
Offline node with an EXHAUSTED code pool → `CodePoolExhausted` → the abort.
Detection is the **settled-mode `assert_clean` scan** AFTER the reboot resolves
the crash transient (the node rests `Offline` → SETTLED → scanned): with the
abort the doc is `Aborted` (clean); without it the doc rests `SIGNED` →
`invariant_scan` flags `StuckNonTerminalDoc`.

The teeth here live in **one** place — the deterministic canary
`teeth_p1_boot_resume_codepool_aborts` (now a CI gate — un-`#[ignore]`d
2026-06-17; PASSES on main, FAILS on revert). Unlike AUD-K8-1, this class is NOT wired into the random property
harness: `Crash(Sign)` is implemented (`interp::crash_after_sign`) but is
**directed-only**, not generatively emitted. A context-free generator produces
crash-after-sign sequences FOLLOWED by further issuance before a reboot (e.g.
`[Crash(Sign), OnlineSell, …]`), which buries the SIGNED doc under a later-issued
doc — an UNREACHABLE production state (single-writer + boot-recon-before-serve:
a crashed process serves no new request before recovery). Surfacing that artifact
in the generative net is a separate harness-realism follow-up (model "no new op
until reboot while a crash is pending). See `strategy.rs::op` for the rationale.

## Run it

```bash
# 1) Baseline (abort PRESENT): canary must be GREEN.  (Now a normal CI test — no --ignored.)
cargo test -p prro --features test-support --test invariant_fuzzer \
  -- teeth_p1_boot_resume_codepool_aborts

# 2) Disable both abort arms (above), re-run — it must FAIL.
# 3) Restore, re-run — GREEN again.
```

## Expected finding (abort reverted)

```
thread 'teeth_p1_boot_resume_codepool_aborts' panicked:
assertion `left == right` failed: P1: boot recovery MUST abort the
post-sign-refused SIGNED doc (CodePoolExhausted). ... the fuzzer's teeth bite.
  left: Some(Signed)
 right: Some(Aborted)
```

This was demonstrated on 2026-06-17: revert → FAIL (above) → restore → GREEN.

---

## Scope (Phase 0)

- `N = 256` per harness (PR-time; documented small). Larger `N` / nightly is a
  Phase-1 follow-up.
- Out of scope (Phase 1+): RETURN/Z/EVPZ/clock op alphabet, model-predicts-
  recovery, WebCheck, the temp-DB-leak fix.

---

## Seed-corpus persistence (Phase-2 U2) — every find becomes a permanent regression

The historical teeth above (AUD-K8-1, P1) are **hand-written** directed pins.
Going forward, a fuzzer find pins **itself**, for free.

**How it works.** The capstone `proptest!` block (`harness_online_seeded` +
`harness_offline_seeded` in `invariant_fuzzer.rs`) pins its regression corpus to
ONE committed file via an explicit absolute path:

```rust
failure_persistence: Some(Box::new(FileFailurePersistence::Direct(concat!(
    env!("CARGO_MANIFEST_DIR"), "/tests/invariant_fuzzer.regressions"))))
```

so the seed always lands at the committed file
`rust/prro/tests/invariant_fuzzer.regressions` — NOT proptest's fragile default
(which, for an integration-test target, falls back to a `WithSource`-renamed
file because there is no `lib.rs`/`main.rs` in the walk-up from `tests/`). On a
find, proptest writes the **minimal** failing seed there and **replays it first**
on every later run (before any novel case).

**The workflow when the fuzzer finds a bug:**

1. The capstone fails; proptest writes the shrunk seed to
   `rust/prro/tests/invariant_fuzzer.regressions`.
2. **Commit that file** — `git add rust/prro/tests/invariant_fuzzer.regressions`.
   That single commit makes the case a **permanent regression**: it replays
   first on every subsequent run, on every machine, forever. (Then fix the bug;
   the committed seed now guards the fix the way the hand-written teeth above do
   — usually no separate teeth test is needed.)

There is **no** `proptest-regressions/` directory and **no** `.gitkeep`; the file
exists only once a find has occurred.

**The CI guard (refuse to silently drop a find).** A find on an ephemeral CI
runner writes the seed into the runner's checkout — and it would vanish when the
runner is torn down. `rust/prro/tests/check_seed_committed.sh` (wired into
`.github/workflows/rust-prro.yml`) fails the job whenever a fuzzer run leaves an
**uncommitted or untracked** seed at the pinned path. It uses
`git status --porcelain` (NOT `git diff --exit-code`, which is blind to a
newly-created untracked file — exactly the first-find silent-drop case). Run it
locally any time:

```bash
bash rust/prro/tests/check_seed_committed.sh   # exit 0 = clean, exit 1 = commit the seed
```

Do **not** commit a planted / experimental seed (e.g. one produced while
deliberately breaking an invariant to test this mechanism): delete it before
committing, or the capstone will replay a fake regression forever.
