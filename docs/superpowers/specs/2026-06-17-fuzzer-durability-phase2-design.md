# Invariant Fuzzer — Durability (Phase 2) design spec

**Date:** 2026-06-17
**Status:** locked (architect). Implementer executes unit-by-unit, RED-first where applicable, with the checkpoints in §9.
**Scope:** TESTS / CI only — **no production `src/` changes**. If any unit appears to need a `src/` change, STOP and checkpoint (§9).
**Supersedes:** the "Tier-1" bullet in `[[project_invariant_fuzzer_plan]]` (temp-leak + CI seed-persist + nightly large-N) — this is its formal design.
**Companion:** the long-run roadmap `docs/superpowers/plans/2026-06-17-optimal-roadmap.md` (Phase 2).

---

## §0 Context & intent

Phase 0 delivered a model-based stateful invariant fuzzer (T0–T7, §15 hardening) that is *functionally* CI-grade and has already found + closed two real production bugs (#192 nocode-offline-sell, P1 boot-resume CodePoolExhausted). But three durability gaps stand between "green when run" and "a trusted, compounding CI gate":

1. The harness **leaks** per-case temp DBs (`std::mem::forget`) → large-N runs exhaust disk, so we cannot run the depth that finds tail bugs.
2. Found bugs leave **no committed regression seed** → every fix is guarded only by a hand-written teeth test; the *compounding* property (each find becomes a permanent regression for free) is not realized.
3. There is **no CI cadence** separating a fast PR-time gate from a deep nightly run, and the case count is hard-coded so depth cannot be dialed up.

Closing these makes the fuzzer the mechanism the project was built around: every new feature/fix re-validates the core for free, and every find is pinned forever. This is the highest-ROI fuzzer work after the bugs themselves.

## §1 Goals / Non-goals

**Goals**
- G1 — per-case temp DBs are cleaned; depth is bounded only by time, not disk.
- G2 — every fuzzer find leaves a committed seed that replays first on the next run (permanent regression), and CI refuses to silently drop a find.
- G3 — PR-time runs a fast gate; a nightly job runs deep (large-N); nightly finds persist + surface.

**Non-goals**
- N1 — generator-fidelity work (e.g. generative `Crash(Sign)` "no-new-op-until-reboot" realism). That is **Phase 3** (it sits with the Cluster-C oracle/realism family, not durability). See §10.
- N2 — new oracle checks / coverage expansion (Cluster C O/D/C items) — Phase 3.
- N3 — any change to fiscal `src/` behavior.

## §2 Verified baseline (ground truth, 2026-06-17)

Architect-verified by direct read (the implementer's ШАГ-0 re-confirms before acting — the dry-run was imprecise on persistence):

- **Leak:** `std::mem::forget(dir)` at `rust/prro/tests/invariant_fuzzer/interp.rs:957` and `:964` (two sites). The `dir` is a `tempfile::TempDir`; it is forgotten so it is not dropped (and thus does not delete the SQLite file) at the end of the fixture-setup fn while the `SqlitePool` still references it.
- **Case counts are hard-coded:** `ProptestConfig { cases: 64, .. }` (`invariant_fuzzer.rs:287`), `cases: 256` (`:305`, `:1195`). An explicit `cases:` literal **overrides** the `PROPTEST_CASES` env var, so today depth cannot be raised from the environment.
- **Persistence is already ON (dry-run X3 was imprecise):** none of those `ProptestConfig` literals set `failure_persistence`, so `ProptestConfig::default()` applies → `Some(FileFailurePersistence::SourceParallel("proptest-regressions"))` → proptest *does* write a seed file on a find. ⚠ **Path correction (review F1):** the default dir is **`proptest-regressions` (NO leading dot)**, written *source-parallel* — for an integration test at `rust/prro/tests/invariant_fuzzer.rs` the actual file is something like `rust/prro/tests/proptest-regressions/invariant_fuzzer.txt`, **not** `.proptest-regressions`. The earlier `.proptest-regressions` claim (and the `git check-ignore` on it) was a wrong-path false-positive — DO NOT rely on it. The exact write path for THIS integration-test target must be confirmed empirically (§4 ШАГ-0) or pinned by explicit config. The real gap is **discipline** (seeds not committed, CI does not enforce), **not** "persistence is off".
- **Historical finds already have directed teeth:** AUD-K8-1, the #192 model-mirror, and P1 (`teeth_p1_boot_resume_codepool_aborts` + 2 kept pins) — do **not** duplicate; reference them.

## §3 Unit 1 — temp-DB-leak fix (foundation)

**Goal (G1).** Remove both `std::mem::forget(dir)`; per-case temp DBs are cleaned when the owning `FuzzCtx` drops.

**ШАГ-0 inventory (read-only).** Establish *why* the forget exists: the `TempDir` guard drops at the end of the setup fn and would delete the DB file while the `SqlitePool` is still open. Map `FuzzCtx`'s lifecycle — how many `TempDir`s (online vs offline fixture? main vs secure DB?), who owns the pool(s), and the required Drop order.

**Approach.** Move ownership of the `TempDir` guard(s) **into `FuzzCtx`** so they live exactly as long as the pool(s); remove both `mem::forget`. Ensure on `FuzzCtx` drop the pool is closed/dropped **before or together with** the tempdir so cleanup does not race a live connection (no "database is locked"). RAII, not forget. Determinism of replay must be unaffected (the DB path can stay per-case-unique; only its *cleanup* changes).

**Acceptance (A1).**
- Zero `mem::forget` in `interp.rs`.
- A high-depth run (`PROPTEST_CASES` large, or a bounded loop) does **not** grow the temp-dir count monotonically — measure `ls "$TMPDIR" | wc -l` (or the configured dir) stable across the run.
- Full harness green (`cargo nextest run -p prro --features test-support`), replay still deterministic.

## §4 Unit 2 — seed-corpus persistence (X3, the compound mechanism)

**Goal (G2).** Every find leaves a committed seed → permanent regression; CI refuses to drop a find silently.

**ШАГ-0 (verify the baseline AND pin the path — review F1).** Temporarily break an invariant so a `proptest!` block fails; observe **where proptest actually writes the seed** (expect `…/proptest-regressions/<file>.txt`, NO leading dot, source-parallel — confirm the exact path for this integration-test target), confirm a re-run replays that seed **first**, then restore. If persistence is disabled anywhere (explicit `failure_persistence: None`) or `SourceParallel` does not write from an integration-test target, report (§9).
**Path decision (F1, locked):** prefer **explicit, deterministic config** over the quirky default — set `failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions")))` (or `Direct(<committed path>)`) so the seed lands at a single known, committed location regardless of proptest's integration-test path resolution. Track + guard **that** path. Do NOT track/guard `.proptest-regressions` (wrong path).

**Approach (the real gap = commit + enforce, not "turn on").**
1. Pin the regression dir via explicit `failure_persistence` (above), put **that exact dir** under git (`.gitkeep` if empty). Confirm it is not gitignored.
2. Document the workflow — "a find → commit its seed file = a permanent regression" — in the fuzzer's `TEETH_TEST.md` (or a fuzzer CONTRIBUTING section).
3. CI guard (F2 — must catch UNTRACKED, not just modified): a PR/CI run fails if a fuzzer run left an **uncommitted OR untracked** seed under the regression dir. Use `git status --porcelain -- <regressions-path>` (non-empty ⇒ fail) or `git ls-files --modified --others --exclude-standard -- <regressions-path>` (non-empty ⇒ fail). **Do NOT use `git diff --exit-code`** — it misses newly-created (untracked) seed files, which is exactly the silent-drop case this guard must close.
4. Do **not** re-pin historical finds — AUD-K8-1 / #192 / P1 already have directed teeth; reference them.

**Acceptance (A2).**
- A planted-bug seed is written to the **pinned** path + replays-first on re-run.
- The pinned regression dir is tracked.
- The CI guard fails on an **untracked** seed (verify with a freshly-created file), fails on a modified one, and passes on a clean tree.

## §5 Unit 3 — CI integration (PR-time small N + nightly large-N)

**Goal (G3).** Fast PR gate; deep nightly; finds persist + surface. **Depends on U1 (no leak at depth) + U2 (persistence mechanism).**

**ШАГ-0 (scope the target — review F3).** There are **multiple** hard-coded `cases` sites (`invariant_fuzzer.rs:287` =64, `:305` =256, `:1195` =256), and they are NOT all equal: some are smoke/demo-style runners, (at least) one is the real capstone harness. **Classify each** `proptest!` block as *capstone* (the genuine generative fuzzer — the only one worth running deep) vs *helper/smoke* (fixed, cheap; inflating it nightly is pure waste). Env-N must target **only the capstone harness(es)**; helper runners keep their small fixed counts. An explicit `cases:` literal overrides `PROPTEST_CASES`, so the capstone block must read the env (or drop its literal); helper blocks must KEEP their literal so the env does not touch them.

**Approach.**
1. Make **only the capstone** case count env-overridable — read `PROPTEST_CASES` (or a dedicated knob, e.g. `FUZZ_CASES`, to avoid coupling to proptest's global env) for the capstone; PR-time default stays small; nightly raises it. Helper/smoke blocks stay fixed. State in code-comment which block is which and why.
2. Add a **nightly** workflow (`.github/workflows`, `schedule:` cron) that runs the **capstone** fuzzer at large-N, `--features test-support`, with **`TMPDIR` on disk** (relies on U1).
3. A nightly find must **persist** its seed (U2) **and surface** — upload the pinned regressions dir as a build artifact and fail loudly (optionally open an issue). PR-time behavior unchanged.

**Acceptance (A3).**
- The env knob drives N for the **capstone only** (verify: setting it high does NOT inflate helper/smoke runners — confirm their case count is unchanged).
- A nightly workflow exists and runs the capstone fuzzer at large-N.
- A planted find in the nightly path produces a persisted + surfaced seed.

**Constraint.** The nightly job must **not** become a required PR status check (branch protection) — it would sit "pending/expected" on every PR and block merges. Keep it off the required-checks list (see the `fmt-clippy.yml` vs `rust-prro.yml` precedent: only `fmt + clippy (gnu)` is required).

## §6 Cross-cutting invariants & discipline

- **Tests/CI only.** No `src/` behavior change. U1 ⊂ `tests/`; U2/U3 ⊂ proptest config + `.github/` + `.proptest-regressions/`.
- **Determinism of replay preserved** in every unit (U1 must not perturb seed→outcome).
- **Isolated worktree.** Work in a dedicated worktree (`git worktree add ../prro_gate_p2 -b feat/fuzzer-durability origin/main`), NOT the shared main tree. Rationale: in the P1 tranche, a checkout in the shared tree thrashed the parallel session's HEAD and mislanded commits. See [[feedback_review_agents_git_checkout_hazard]].
- **Local gate before each handoff** (P1 lesson — clippy alone is insufficient; the required CI job runs *both* fmt and clippy):
  - `cargo fmt -p prro -p prro_crypto -p prro_escpos -- --check`
  - `cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings`
  - `cargo nextest run -p prro --features test-support`
- **Vertical slices.** Each unit is its own PR (base `main`), reviewed + merged independently.

## §7 Sequencing & delivery

Order: **U1 → U2 → U3** (U3 depends on U1 + U2; U1 + U2 are otherwise independent). Each unit ships a Delivery report (7 items: Intent · Files · Tests/checks with output · Result · Known risks/not done · Invariant check [determinism preserved; tests/CI-only] · Next).

## §8 Acceptance (roll-up)

Phase 2 is done when: A1 (no leak; depth-stable) ∧ A2 (seeds committed + replay-first + CI-enforced) ∧ A3 (env-N + nightly large-N + surfaced finds), all three units merged, the full gate green, and replay determinism intact.

## §9 Risks & checkpoints (stop-and-ask)

- **R1 / CP1 (U1):** removing `forget` may surface a Drop-order / pool-still-open constraint. Checkpoint with the `FuzzCtx` lifecycle finding before forcing it; do **not** silently restore `forget`.
- **R2 / CP2 (U2):** if persistence is actually off somewhere, or `SourceParallel` does not write from the integration-test target, the approach changes — checkpoint with the empirical finding.
- **R3 / CP3 (U3):** the env-N approach (read `PROPTEST_CASES` vs drop the literal), the cron schedule, and the not-required status all need confirmation before wiring.
- **R4 / CP4:** any unit that appears to need a production `src/` change — STOP, this spec is tests/CI-only.

## §10 Locked decisions / deferred

- **(b) generative `Crash(Sign)` harness-realism → Phase 3.** Today `Crash(Sign)` is directed-only (the P1 teeth canary); generative emission produces a "buried-SIGNED" artifact (`[Crash(Sign), OnlineSell, …]` hides the SIGNED doc under a later-issued one — unreachable in prod: single-writer + boot-recon-before-serve). Modeling "no new op until reboot while a crash is pending" is generator *fidelity*, which belongs with the Cluster-C realism family in Phase 3, not durability. Decision (a) from the P1 tranche: the directed canary is a sufficient regression gate now.
- **Adjacent findings (NOT P1, Phase-3 probe targets):** `NodeBlocked`-permanent / `ShiftNotOpened` / `NoActiveSession` can leave a durable `SIGNED` (different semantics from `CodePoolExhausted` — a pause/precondition, not a permanent refusal). The fuzzer will probe these once (b) lands; on confirmed reachability each gets its own fix with a proven repro. See [[project_fuzzer_finding_p1_boot_resume_refusal]].

## References

- `[[project_invariant_fuzzer_plan]]` — Phase-0 status + Tier/long-run roadmap.
- `docs/superpowers/specs/2026-06-15-invariant-fuzzer-design.md` — Phase-0 design.
- `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` — X3 + the Cluster-C backlog.
- `rust/prro/tests/invariant_fuzzer/TEETH_TEST.md` — teeth discipline + #192/P1 demos.
