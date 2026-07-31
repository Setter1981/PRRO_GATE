# Handoff — MacReseed hardening (LANDED) + deferred fuzzer reconciliation / task #18

**Date:** 2026-07-24 · **Author session:** MacReseed seed-validation hardening.
**Audience:** next session. Everything below is grounded (file:line verified this session).

---

## 0. TL;DR — what is DONE, what is NEXT

- ✅ **DONE (merged to main):** MacReseed seed-validation prod hardening + the CI-health
  bootstrap that had to land first.
- ⬜ **NEXT (this handoff):** on `fuzzer-cs3-oracle` — (B) the deferred MacReseed directed
  regression tooth + model/interp reconciliation, and (C) **task #18** (CS-3 fuzzer
  offline-half). Do both on a FRESH session rebased onto the new main.

Memory: `[[project_macreseed_seed_validation_hardening]]`,
`[[reference_ci_linker_oom_and_supersession_hygiene]]`, `[[project_cs3_fuzzer_oracle_state]]`.

---

## 1. State of `main` (both PRs merged 2026-07-24)

- **PR #338** — MacReseed prod hardening — **MERGED** `bc6f1937` (rebased commit `1e08cbe1`).
- **PR #339** — CI-health bootstrap — **MERGED** `7bc0df74`.
- `main` tip = `bc6f1937` (contains both). Pre-hardening main was `5360ecf4`.

### What #338 changed (the hardening)
`complete_operator_pending` (`rust/prro/src/db/repositories/delivery_reservation.rs`) now runs
**two fail-closed guards BEFORE any mutation** (whole tx rolls back → nothing changes):
- **Guard A (hold-type):** MacReseed valid only when the reservation's `node_effect ==
  "MacReseedPending"` → else `CompletionError::MacReseedHoldMismatch`. (SELECT now threads
  `node_effect`.)
- **Guard B (expected-tip):** the operator seed must equal
  `fiscal_documents::last_issued_unsigned_xml_sha256(&mut **tx, fn)` — the SHARED `is_issued`
  projection `invariant_scan` walks to (that helper is now **executor-generic**; its 1 caller
  `boot_phase.rs:1729` is unaffected) → else `CompletionError::MacReseedSeedMismatch`.
- Teeth: `tests/operator_completion.rs` — `oc23` (guard A = the fuzzer repro, a NoResponse
  crash hold), `oc24` (guard B), `oc04` rebuilt on a faithful `-12` hold + issued predecessor.
- **Design pin:** guard B validates against the LOCAL last-issued tip (= what the scan checks).
  A seed ≠ tip IS a `ChainSeedMismatch` by definition, so guard B cannot over-reject a
  legitimate reseed. Reseeding to an external DPS value that diverges from local docs is NOT
  supported by the system as-built (the scan would flag it anyway) — a separate design decision,
  not this patch.

### What #339 changed (CI-health) — SEE `[[reference_ci_linker_oom_and_supersession_hygiene]]`
- `.github/workflows/rust-prro.yml`: job-level `CARGO_BUILD_JOBS: "1"` on the x86 build job
  (serial link → no `lld` OOM bus-error). **x86 CI is now ~28-31 min** (was ~15).
- `docs/cs1r/inventory/superseded_removals.tsv`: pruned to empty (the 3 stale #337 rows).
- `scripts/cs1r/inventory_gate.sh`: empty-registry `|| true` guard.
- `scripts/cs1r/inventory_gate_teeth.sh`: NEW — 5 teeth on the gate (T1-T5), revert-canary proven.

---

## 2. (B) DEFERRED — MacReseed directed tooth + fuzzer reconciliation

Branch: **`fuzzer-cs3-oracle`** (tip `d772fecb`, 141/141, pushed, external GO). It carries the
generative `OperatorComplete` op but **MacReseed is EXCLUDED from the generator** and the interp
uses an **arbitrary** seed.

### Precise anchors (on `fuzzer-cs3-oracle`, verified this session)
- `tests/invariant_fuzzer/interp.rs:1114` — `OperatorResolutionKind::MacReseed =>
  OperatorResolution::MacReseed { seed: [0x5a; 32] }`  ← **arbitrary seed** (never the real tip).
- `tests/invariant_fuzzer/model.rs:444` — `fn apply_operator_complete(...)` predicts the outcome.
- `tests/invariant_fuzzer/model.rs:2013` — `fn released_witness(...)`: `:2024` `MacReseed =>
  !online_origin` (refused only if OFFLINE), `:2041` `MacReseed => RMR`. **The model currently
  predicts an ONLINE MacReseed is RELEASED (RMR, seed re-based) — after #338 that is WRONG for
  the arbitrary seed (guard B refuses).**
- `tests/invariant_fuzzer/model.rs:1974` — `node_effect: "MacReseedPending"` (model knows the -12
  hold type).
- `tests/invariant_fuzzer.rs:1335` / `:1343` — directed asserts on `model::released_witness(...)`
  — these are **MODEL-ONLY** (do not run prod), so they will NOT auto-break on rebase.
- `tests/invariant_fuzzer/strategy.rs:163` — MacReseed excluded from the generator.

### Expectation after rebasing `fuzzer-cs3-oracle` onto new `main`
Because MacReseed is generator-excluded AND the directed asserts are model-only, **the fuzzer
gate most likely stays GREEN on rebase** (nothing exercises MacReseed against the hardened prod).
**FIRST STEP: rebase onto `origin/main` and RUN THE GATE to confirm green** (do not assume).

### The tooth to add (the "later directed valid-seed test" the finding promised)
Add a DIRECTED test (not generative) that drives MacReseed through prod and proves the guards:
1. **Valid path:** `[OnlineSell(-12 BadHashPrev hold) → OperatorComplete(MacReseed{seed = the
   model's expected tip})]` → completion SUCCEEDS, seed installed = tip, doc → RMR, scan clean.
   - The interp's hardcoded `[0x5a;32]` MUST become **tip-aware** for this (parametrize the op /
     let the directed test supply the model's `last_issued` tip), else guard B refuses.
2. **Refusal — guard B:** MacReseed with `seed ≠ tip` → `MacReseedSeedMismatch`, doc stays
   SENDING, node NOT released, seed unchanged, `invariant_scan` clean (no ChainSeedMismatch).
3. **Refusal — guard A:** MacReseed on a NON-`MacReseedPending` hold (e.g. a NoResponse hold) →
   `MacReseedHoldMismatch`, nothing mutated.
4. Update `model.rs` (`apply_operator_complete` / `released_witness`) so the model predicts the
   refusals (seed≠tip OR hold≠MacReseedPending → NOT released). Keep the model INDEPENDENT (do
   not call prod to decide) — compute the expected tip in the model.
- This mirrors the prod teeth `oc23`/`oc24` but through the generative harness. Cross-ref the
  finding: `docs/superpowers/audits/2026-07-24-macreseed-seed-validation-finding.md`.

---

## 3. (C) task #18 — CS-3 fuzzer OFFLINE-half

The bigger next increment on `fuzzer-cs3-oracle` (per `[[project_cs3_fuzzer_oracle_state]]`): the
online-half is done (8 increments, 141/141, external GO, BRICK-property generative). Task #18 =
the offline half. Do it fresh-session. Fold the (B) MacReseed tooth into this branch's work.

---

## 4. KNOWN TRAPS (bit us this session — read before touching CI)

1. **Linker OOM will RECUR on heavy PRs.** `ld: signal 7 Bus error` when `lld` links ~200 test
   binaries on ubuntu-latest (~7 GB). Now mitigated by `CARGO_BUILD_JOBS=1` (serial, ~28-31 min).
   **Retries do NOT converge (~1/5).** If a heavy PR still OOMs single-link → `mold` (adds a CI
   dep) or a bigger runner. Do NOT revert to `-j2` without proof it survives.
2. **Supersession-registry hygiene:** when a retire-and-replace PR MERGES, its rows in
   `docs/cs1r/inventory/superseded_removals.tsv` become STALE (no removal-vs-base) and **RED every
   future PR off main**. Prune them (record stays in the merged commit + git history). Push-CI
   does NOT catch this (control-2 only runs `--pr`).
3. **Adding tests → re-mint** `scripts/cs1r/mint_manifests.sh` (regenerates the test-support +
   live-dps manifests) BEFORE pushing, else control-1 (live==committed) REDs. Re-mint ONCE at the
   true tip.
4. **`git push --force` and `git reset --hard` are BLOCKED by a project guardrail.** For a rebase
   force-push, ask the operator to run it via the `!` prefix, OR open a fresh PR (non-destructive).
5. **`git fetch` intermittently flakes** ("could not read from remote … repository exists") while
   `git ls-remote` works — retry fetch; use `ls-remote refs/heads/main` for the authoritative tip.
6. **`gh pr edit` hits a GraphQL projectCards deprecation error** — use
   `gh api --method PATCH repos/OWNER/REPO/pulls/N -f title=… -F body=@file` instead.
   `gh pr merge` may return a transient **502** but still merge — verify with `gh pr view --json state`.
7. **Toolchain:** `cargo`/`nextest` live at `~/.cargo/bin` — `export PATH="$HOME/.cargo/bin:$PATH"`.
   Serial CI runs are slow; poll in the background, don't foreground-`sleep`.
8. **Leftover local worktrees/branches** to clean up when convenient: worktrees
   `/home/setter/prro_gate-macreseed`, `/home/setter/prro_gate-ci`; local branches
   `macreseed-rebased`, `prod-macreseed-seed-validation`, `ci-serial-link-build`.

---

## 5. Verification / gate checklist (for the fuzzer work)

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd rust
cargo fmt -p prro -p prro_crypto -p prro_escpos -- --check
cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings
cargo nextest run -p prro --features test-support --locked -E 'binary(invariant_fuzzer)'
cargo nextest run -p prro --features test-support --locked          # full
bash prro/tests/check_seed_committed.sh                              # fuzzer regression seed
# large-N before merge:
FUZZ_CASES=4096 cargo nextest run -p prro --features test-support --locked -E 'test(/^harness_(online|offline)_seeded$/)'
```
Pre-push CI gate: `[[feedback_pre_push_ci_gate_checklist]]` (fmt + crate-scoped clippy -D +
inventory re-mint if tests changed + `--all-features` nextest). Teeth must bite (empirical
revert-canary, `[[project_real_teeth_roi_pr257]]`).
