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

> **Honesty note (audit HIGH#1):** Phase 2 delivers the durability *mechanism* (depth, committed seeds, nightly cadence). It does **not** by itself make the fuzzer a merge-**blocking** gate: the fuzzer + seed-guard run in the **non-required** `rust-prro.yml` + the nightly, and branch protection currently requires only `fmt + clippy (gnu)`. Turning the mechanism into an enforced gate is a separate **branch-protection** step (add a fast fuzzer/seed-guard status to required checks) — out of scope here. Read "CI gate" below as "the mechanism for one", not "an enforced gate today".

## §1 Goals / Non-goals

**Goals**
- G1 — per-case temp DBs are cleaned; depth is bounded only by time, not disk.
- G2 — every fuzzer find leaves a committed seed that replays first on the next run (permanent regression), and CI refuses to silently drop a find.
- G3 — PR-time runs a fast (non-required) check; a nightly job runs deep (large-N); nightly finds persist + surface. (Making either a *required* merge gate is a branch-protection step — see the §0 honesty note.)

**Non-goals**
- N1 — generator-fidelity work (e.g. generative `Crash(Sign)` "no-new-op-until-reboot" realism). That is **Phase 3** (it sits with the Cluster-C oracle/realism family, not durability). See §10.
- N2 — new oracle checks / coverage expansion (Cluster C O/D/C items) — Phase 3.
- N3 — any change to fiscal `src/` behavior.

## §2 Verified baseline (ground truth, 2026-06-17)

Architect-verified by direct read (the implementer's ШАГ-0 re-confirms before acting — the dry-run was imprecise on persistence):

- **Leak:** `std::mem::forget(dir)` at `rust/prro/tests/invariant_fuzzer/interp.rs:957` and `:964` (two sites). The `dir` is a `tempfile::TempDir`; it is forgotten so it is not dropped (and thus does not delete the SQLite file) at the end of the fixture-setup fn while the `SqlitePool` still references it.
- **Case counts are hard-coded — and the override rule is NOT uniform (review F3, corrected, verified against proptest 1.11.0):** `ProptestConfig { cases: 64, .. }` (`invariant_fuzzer.rs:287`, a `proptest!` **smoke** block), `cases: 256` (`:1195`, the `proptest!` **capstone** block — the two `harness_*_seeded` tests), and `TestRunner::new(Config { cases: 256, ..Config::default() })` (`:304`, a **manual** shrinking-demo). The earlier blanket claim "a `cases:` literal overrides `PROPTEST_CASES`" is **WRONG for the macro blocks**. Reality:
  - **Manual `TestRunner::new` (`:304`) IS immune** — `TestRunner::new` does not re-read env (`runner.rs:316`), and the explicit `cases: 256` field wins over `..Config::default()`.
  - **Both `proptest!` macro blocks (`:287`, `:1195`) are OVERRIDDEN by `PROPTEST_CASES`** — the macro passes the literal config to `contextualize_config($config.clone())` at runtime (`sugar.rs:160`), which overwrites `result.cases` from the env var (`config.rs:86`).
  - Consequence: `PROPTEST_CASES` would inflate **both** the smoke (`:287`) **and** the capstone (`:1195`) while leaving the manual demo (`:304`) fixed — i.e. it is the **wrong knob** for "capstone-only" scaling. §5 uses a dedicated `FUZZ_CASES` instead.
- **Persistence is already ON (dry-run X3 was imprecise):** none of those configs set `failure_persistence`, so `ProptestConfig::default()` applies → `Some(FileFailurePersistence::SourceParallel("proptest-regressions"))` → proptest *does* write a seed file on a find. ⚠ **Path corrected AGAIN (review F1, verified against proptest 1.11.0 source):** the default is `SourceParallel("proptest-regressions")` (NO leading dot), but for **this integration-test target** it does **not** resolve to `tests/proptest-regressions/invariant_fuzzer.txt`. `SourceParallel` walks the source path's ancestors for a dir containing `lib.rs`/`main.rs` (`file.rs:336-347`); from `rust/prro/tests/invariant_fuzzer.rs` the ancestors are `tests/`, `prro/`, `rust/`, … — **none** holds `lib.rs`/`main.rs` (the crate's `lib.rs` is under `src/`, a *sibling* of `tests/`, not an ancestor). So `found == false` → it warns and **falls back to `WithSource`** (`file.rs:349-354`), which only **renames the extension** → the real default path is the single FILE **`rust/prro/tests/invariant_fuzzer.proptest-regressions`**, NOT a `proptest-regressions/` directory. Both prior claims (`.proptest-regressions` AND `tests/proptest-regressions/…txt`) were wrong. We will NOT rely on this fragile default — §4 pins an explicit absolute path. The real gap is **discipline** (seeds not committed, CI does not enforce), **not** "persistence is off".
- **Historical finds already have directed teeth:** AUD-K8-1, the #192 model-mirror, and P1 (`teeth_p1_boot_resume_codepool_aborts` + 2 kept pins) — do **not** duplicate; reference them.

## §3 Unit 1 — temp-DB-leak fix (foundation)

**Goal (G1).** Remove both `std::mem::forget(dir)`; per-case temp DBs are cleaned when the owning `FuzzCtx` drops.

**ШАГ-0 inventory (read-only).** Establish *why* the forget exists and map the full lifecycle. `FuzzCtx` today owns **two** pools — `pool` + `pool_secure` (`interp.rs:101-102`) — and **no** `TempDir`. The two leaks are independent: `fresh_pool()` (`:957`) and `fresh_secure_pool()` (`:964`), each `tempfile::tempdir()` → join db path → `mem::forget(dir)` → open pool. The forget exists because the `TempDir` guard would drop at the end of the factory fn and delete the DB file while the returned `SqlitePool` is still open. So **two** guards must be threaded out and held for the ctx's lifetime.

**Approach.** Have `fresh_pool()` / `fresh_secure_pool()` **return** their `TempDir` alongside the pool, and store **both** guards as fields on `FuzzCtx` so they live exactly as long as the pools; remove both `mem::forget`. **Drop discipline (review MED):** Rust drops struct fields in **declaration order** — declare the pool fields **before** the tempdir fields so pools close first, then the tempdirs delete the files (or add an explicit close-then-drop `Drop` impl). Cleanup must not race a live connection (no "database is locked"). RAII, not forget. Determinism of replay must be unaffected (the DB path can stay per-case-unique; only its *cleanup* changes).

**Acceptance (A1).**
- Zero `mem::forget` in `interp.rs`.
- A high-depth run (`FUZZ_CASES` large — §5, or a bounded loop) does **not** grow the temp-dir count monotonically — measure with an **isolated `TMPDIR`** (`TMPDIR=$(mktemp -d)`) so the count reflects only this harness, not global `/tmp` noise; assert `ls "$TMPDIR" | wc -l` is stable across the run.
- Full harness green (`cargo nextest run -p prro --features test-support`), replay still deterministic.

## §4 Unit 2 — seed-corpus persistence (X3, the compound mechanism)

**Goal (G2).** Every find leaves a committed seed → permanent regression; CI refuses to drop a find silently.

**ШАГ-0 (verify the baseline AND confirm the pin — review F1).** Temporarily break an invariant so the capstone `proptest!` block fails; observe **where proptest actually writes the seed**. Per §2 the *unpinned* default for this target is the single file `rust/prro/tests/invariant_fuzzer.proptest-regressions` (WithSource fallback) — confirm. Confirm a re-run replays that seed **first**, then restore. **Also confirm replay is deterministic** despite the time-based UUIDv7 IDs minted in the write path (`rust/prro/src/db/models/ids.rs:16`): a committed seed must reproduce the *same* outcome on a later day/run. If a UUIDv7 timestamp leaks into model state or an assertion, committed seeds are non-portable across time and the pin must capture more than the RNG seed — report (§9). If persistence is disabled anywhere (explicit `failure_persistence: None`), report.
**Path decision (F1, corrected + locked).** Do NOT rely on the default resolution, and do NOT use `WithSource("regressions")` expecting a *directory* — `WithSource` only renames the source file's **extension** (a FILE, `file.rs:380-384`), and relative `Direct(...)` is **cwd-sensitive** (`file.rs:396`, no absolutization). Pin **one exact, absolute, committed FILE**:
`failure_persistence: Some(Box::new(FileFailurePersistence::Direct(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/invariant_fuzzer.regressions"))))`.
`CARGO_MANIFEST_DIR` is `rust/prro/`, so the seed file is the committed `rust/prro/tests/invariant_fuzzer.regressions` regardless of cwd or proptest's path heuristics. Set this on the **capstone** config (`:1195`); the smoke (`:287`) and manual demo (`:304`) are not durability surfaces — leave them on default. Track + guard **that one file**. There is NO directory and NO `.gitkeep`.

**Approach (the real gap = commit + enforce, not "turn on").**
1. Pin the capstone's regression FILE via explicit `Direct(...)` (above). The file is created by proptest on the first find; commit it then (there is no empty dir to pre-create). Confirm the path is not gitignored.
2. Document the workflow — "a find → commit its seed file = a permanent regression" — in the fuzzer's `TEETH_TEST.md` (or a fuzzer CONTRIBUTING section).
3. CI guard (F2 — must catch UNTRACKED, not just modified): a PR/CI run fails if a fuzzer run left an **uncommitted OR untracked** seed at the pinned file. Use `git status --porcelain -- rust/prro/tests/invariant_fuzzer.regressions` (non-empty ⇒ fail) or `git ls-files --modified --others --exclude-standard -- rust/prro/tests/invariant_fuzzer.regressions` (non-empty ⇒ fail). **Do NOT use `git diff --exit-code`** — it misses newly-created (untracked) seed files, which is exactly the silent-drop case this guard must close. (Note: this also catches a *shrink-rewrite* of an existing tracked seed — `--porcelain` reports `M`.)
4. Do **not** re-pin historical finds — AUD-K8-1 / #192 / P1 already have directed teeth; reference them.

**Acceptance (A2).**
- A planted-bug seed is written to the **pinned file** (`rust/prro/tests/invariant_fuzzer.regressions`) + replays-first on re-run.
- The pinned regression file is tracked.
- The CI guard fails on an **untracked** seed (verify with a freshly-created file), fails on a modified one, and passes on a clean tree.

## §5 Unit 3 — CI integration (PR-time small N + nightly large-N)

**Goal (G3).** Fast PR gate; deep nightly; finds persist + surface. **Depends on U1 (no leak at depth) + U2 (persistence mechanism).**

**ШАГ-0 (scope the target — review F3, corrected).** Three hard-coded `cases` sites, classified: **capstone** = the `proptest!` block at `:1194` holding **both** `harness_online_seeded` (`:1199`) AND `harness_offline_seeded` (`:1206`) — the offline test is REQUIRED (the AUD-K8-1 / drain / manual-recon lane only exists offline; the teeth live there), so "the capstone" = **both** tests, not one; **smoke** = the `proptest!` block at `:287` (=64); **manual demo** = the `TestRunner::new` at `:304` (=256, the shrinking-acceptance demo). Only the capstone is worth running deep. Per §2 the override semantics are NOT uniform: `PROPTEST_CASES` inflates BOTH macro blocks (`:287` smoke + `:1194` capstone) and leaves the manual demo fixed — so `PROPTEST_CASES` is the **wrong** knob for capstone-only scaling.

**Approach.**
1. Scale **only the capstone** via a **dedicated `FUZZ_CASES`** knob (NOT `PROPTEST_CASES`, which is global and would also inflate the `:287` smoke). Read it in a small helper used only by the capstone config, e.g. `fn fuzz_cases() -> u32 { std::env::var("FUZZ_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(256) }` with `#![proptest_config(ProptestConfig { cases: fuzz_cases(), ..ProptestConfig::default() })]` at `:1195`. PR-time default stays 256; the smoke (`:287`) keeps its literal 64 and does NOT read `FUZZ_CASES`. ⚠ Caveat (from §2): the `proptest!` macro still runs `contextualize_config` afterwards, so a developer who *also* set `PROPTEST_CASES` would override `FUZZ_CASES` — therefore **CI must set `FUZZ_CASES`, never `PROPTEST_CASES`**. State in a code-comment which block is capstone vs smoke and why.
2. Add a **nightly** workflow (`.github/workflows`, `schedule:` cron, e.g. `0 2 * * *`) that runs the capstone at large-N with `FUZZ_CASES` set high (target **≥4096**, tuned to a bounded budget), `--features test-support`, and **`TMPDIR` on disk** (relies on U1 — without it large-N exhausts disk). Run both capstone tests, e.g. `cargo nextest run -p prro --features test-support -E 'test(/^harness_(online|offline)_seeded$/)'` (or the full suite — only the capstone reads `FUZZ_CASES`). Set a job `timeout-minutes` (e.g. 60) so a hang fails loudly rather than burning CI.
3. A nightly find must **persist** its seed (U2) **and surface** — upload the pinned regression file as a build artifact (`actions/upload-artifact`, `retention-days: 30`) and fail the job loudly (optionally open an issue). PR-time behavior unchanged.

**Acceptance (A3).**
- `FUZZ_CASES` drives N for the **capstone only** (verify: setting it high does NOT inflate the `:287` smoke or the `:304` manual demo — confirm their case counts are unchanged); `PROPTEST_CASES` is NOT used by CI.
- A nightly workflow exists and runs **both** capstone tests (`harness_online_seeded` + `harness_offline_seeded`) at large-N with `TMPDIR` on disk and a job timeout.
- A planted find in the nightly path produces a persisted (committed on the pinned file) + surfaced (artifact + loud failure) seed.

**Constraint.** The nightly job must **not** become a required PR status check (branch protection) — it would sit "pending/expected" on every PR and block merges. Keep it off the required-checks list (see the `fmt-clippy.yml` vs `rust-prro.yml` precedent: only `fmt + clippy (gnu)` is required).

## §6 Cross-cutting invariants & discipline

- **Tests/CI only.** No `src/` behavior change. U1 ⊂ `tests/`; U2/U3 ⊂ proptest config + `.github/` + the single committed seed file `rust/prro/tests/invariant_fuzzer.regressions`.
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
- **R2 / CP2 (U2):** if the explicit `Direct(...)` pin does not write from the integration-test target, or replay is non-deterministic (UUIDv7 time-leak, §4 ШАГ-0), or persistence is off somewhere, the approach changes — checkpoint with the empirical finding.
- **R3 / CP3 (U3):** the env-N knob is **`FUZZ_CASES`** (capstone-only; `PROPTEST_CASES` is the wrong, global knob — §5), plus the cron schedule, the large-N target, the job timeout, and the not-required status — all need confirmation before wiring.
- **R4 / CP4:** any unit that appears to need a production `src/` change — STOP, this spec is tests/CI-only.

## §10 Locked decisions / deferred

- **(b) generative `Crash(Sign)` harness-realism → Phase 3.** Today `Crash(Sign)` is directed-only (the P1 teeth canary); generative emission produces a "buried-SIGNED" artifact (`[Crash(Sign), OnlineSell, …]` hides the SIGNED doc under a later-issued one — unreachable in prod: single-writer + boot-recon-before-serve). Modeling "no new op until reboot while a crash is pending" is generator *fidelity*, which belongs with the Cluster-C realism family in Phase 3, not durability. Decision (a) from the P1 tranche: the directed canary is a sufficient regression gate now.
- **Adjacent findings (NOT P1, Phase-3 probe targets):** `NodeBlocked`-permanent / `ShiftNotOpened` / `NoActiveSession` can leave a durable `SIGNED` (different semantics from `CodePoolExhausted` — a pause/precondition, not a permanent refusal). The fuzzer will probe these once (b) lands; on confirmed reachability each gets its own fix with a proven repro. See [[project_fuzzer_finding_p1_boot_resume_refusal]].

## References

- `[[project_invariant_fuzzer_plan]]` — Phase-0 status + Tier/long-run roadmap.
- `docs/superpowers/specs/2026-06-15-invariant-fuzzer-design.md` — Phase-0 design.
- `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` — X3 + the Cluster-C backlog.
- `rust/prro/tests/invariant_fuzzer/TEETH_TEST.md` — teeth discipline + #192/P1 demos.
