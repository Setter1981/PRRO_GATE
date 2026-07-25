# Handoff — task #18 fuzzer-cs3-oracle: rebase + 2 findings + (B) MacReseed DONE → (C) offline-half NEXT

**Date:** 2026-07-24 · **Author session:** task #18 session 2 (rebase + latent findings + MacReseed teeth).
**Audience:** next session. Everything below is grounded (file:line verified this session).
Supersedes the "(B) DEFERRED" part of `HANDOFF_2026-07-24_MACRESEED_AND_FUZZER_NEXT.md` (that (B) is now DONE).
Memory: `[[project_cs3_fuzzer_oracle_state]]`, `[[project_macreseed_seed_validation_hardening]]`,
`[[reference_ci_linker_oom_and_supersession_hygiene]]`.

---

## 0. TL;DR — what is DONE, what is NEXT

- ✅ **DONE this session** (2 commits on `fuzzer-cs3-oracle`, local tip `e6548a5b`, NOT pushed):
  - Rebase onto `main` `bc6f1937` (#338 MacReseed hardening + #339 CI).
  - Resolved **2 latent capstone findings** the rebase surfaced (both `OperatorComplete(Accepted)` on a
    held doc; #338 is INERT for non-MacReseed → RngSeed::Random latent cases, not regressions).
  - **(B) MacReseed directed teeth** — 4 teeth mirroring prod `oc23`/`oc24` through the real seam.
  - Fuzzer **147/147**, fmt + clippy `-D` clean, prod FROZEN (0 src diff), all new teeth canary-proven.
- ⬜ **NEXT:** **(C) offline-half** — the MOST IMPORTANT remaining CS-3 gap (dossier follow-up #2).
  Do it fresh-session from local tip `e6548a5b`.

---

## 1. Exact state

- **Branch:** `fuzzer-cs3-oracle`. **Worktree:** `/home/setter/prro-gate-wt/fuzzer-cs3`.
- **Local tip `e6548a5b`** (rebased onto `bc6f1937`). **NOT PUSHED.**
  `origin/fuzzer-cs3-oracle` is still `d772fecb` (pre-rebase backup — safe to fall back to).
- Push requires a **force-push** (rebase rewrote history) which the project guardrail BLOCKS →
  ask the operator to run it via the `!` prefix, OR open a fresh PR. Do NOT `git reset --hard` /
  `git push --force` yourself (blocked).
- This session's 2 commits (on top of the 9 rebased increments + the dossier commit `8a71c777`):
  - `d8f6f5ad` — resolve 2 latent capstone findings.
  - `e6548a5b` — MacReseed directed teeth (task #18 B).
- Dossier `docs/CS3_FUZZER_ORACLE_DOSSIER.md` (committed on branch) has full detail:
  section "Rebase-onto-#338 latent findings" + "(B) MacReseed directed teeth".

### What the 2 findings were (both NOT prod bugs — the fuzzer doing its job)
1. **Offline liveness** `[Crash(Send), GoOnline([UnknownStatus(-4)]), OperatorComplete(Accepted)]` —
   fixture-FN-fidelity: the interp Accepted stub FN (`"5000000001"`) ≠ the DPS stub FN
   (`SERVER_FISCAL_NO`); an operator-Accepted go-online-drain held doc re-enters the cohort as `SENT`,
   the settle-drain re-probes via `last_chk` → `LastChkIdMismatch` → stuck `GoingOnline`. Fix: interp
   Accepted FN → `SERVER_FISCAL_NO` (`interp.rs` ~1107). Tooth
   `harness_offline_operator_accepted_held_drain_doc_resettles_online`.
2. **Online seed-advance** `[OnlineServiceIn([Ack,Ack]), OnlineSell([BadHashPrev]), OperatorComplete(Accepted)]`
   — `model::adopt_fault_deferred` placed the structural seed placeholder on `max(lnd)` (a held
   non-issued SENDING doc) instead of the real chain tip. Prod correct → model tightened: `tip_lnd`
   now = the doc whose `unsigned_xml_sha256 == real seed` (model.rs ~1704). Tooth
   `harness_online_operator_accepted_after_badhashprev_hold_seed_advance`.

### What (B) added (`e6548a5b`)
- `interp::operator_complete_macreseed(ctx, seed: [u8;32])` — directed MacReseed driver (explicit seed,
  NOT the `[0x5a;32]` placeholder; enum NOT changed). `FuzzCtx::last_issued_tip()` helper.
- `model::macreseed_completion_releases(online_origin, hold_is_macreseed_pending, seed_matches_tip)`.
- 4 teeth: model contract + valid (seed=tip→RMR) + guard A (`MacReseedHoldMismatch`) + guard B
  (`MacReseedSeedMismatch`). **Match the Display strings** of the refusal, NOT the variant names:
  `"only valid for a MacReseedPending"` / `"does not match the expected chain tip"` (the error is
  `ResolutionRefused { reason: <Display> }`).
- **Latent model imprecision (documented, not wired):** guard B only accepts `seed == local tip`, so a
  valid MacReseed is a value-no-op reseed, yet `model::apply_operator_complete` marks MacReseed
  `advances_seed=true` (model.rs ~482). Unexercised (MacReseed generator-excluded + directed asserts
  model-only). If a future increment makes MacReseed generative, fix this first.

---

## 2. (C) offline-half — the NEXT increment (dossier follow-up #2, "do NOT defer behind 3/4")

Sub-parts, in the intended order (do NOT reorder):

### (C-i) origin-keying `[Reject]` / `[Ack,NotFound]` held witnesses
The held-witness matrix currently returns `None` for these two leaves (documented gap):
- `[Reject]` — ONLINE releases to `Rejected` (non-issued row); **OFFLINE-drain HOLDS** → needs an
  origin-keyed held witness.
- `[Ack, NotFound]` — `Sent` (D5 held-at-SENT), online release-at-SEND; offline-lane keying needed.
- **Method (mirror Increment 3 exactly):** write a directed instrumented repro on the OFFLINE fixture
  (`FuzzCtx::new_offline_open_shift(3)`), drive the leaf through the real drain, PROBE the real
  `delivery_reservation` row (dump `submission_certainty / response_provenance / routing_class /
  node_effect / evidence_kind / evidence_code / apply_state` + node mode + fence), THEN encode the
  tuple INDEPENDENTLY in the model — do NOT import prod `classify()`. Add teeth + a canary
  (flip a routing_class → RED), like `superseded_durable_class_is_transient_retry_not_wrapperbug`.
- Anchors: `model::online_held_witness` (model.rs ~1888-1982; the `Superseded`/`BadHashPrev`/`_→None`
  match) — you likely need an ORIGIN-keyed variant (`offline_held_witness` or a param). Oracle
  `oracle::check_held_witness` (7 text axes + evidence_code + fence). Read via
  `interp::FuzzCtx::read_held_witness` → `ObservedHeld` (interp.rs ~114/670).

### (C-ii) drain-produced HELD witness (map MINOR-2)
The held-witness oracle must fire after a DRAIN (not just a direct send). A drain-produced held
reservation carries the witness; assert it in `run_harness` on the offline lane. The existing
held-witness check in `run_harness` is gated on `doc_state == Sending` from a DIRECT send; a drain
holds via a different path. Grounding: `settle_drain_tick` (interp.rs ~2226) drives the REAL
`backlog_drain::drain`; the drain candidate predicate is
`fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd` (states
`OFFLINE_LOCAL_ACK/SENT/KVT1/ERROR_RETRYABLE/KVT2`).

### (C-iii) `NotAcceptedOffline` cohort-cancel + chain rewind — LAST, "the hard one"
An operator `NotAcceptedOffline` on an OFFLINE-origin held doc RELEASES with an OLA-cohort cancel + a
chain rewind. The model must INDEPENDENTLY predict the cohort cancellation + seed/chain rewind; the
oracle asserts the durable cohort state. Adversarial teeth per axis.
- Prod machinery: `delivery_reservation.rs` — origin cross-check `NotAcceptedOffline if online →
  OriginMismatch` (~1375); the cohort-cancel + rewind vars `cancelled_cohort` /
  `rewound_predecessor_seed` (~1415-1429). `NotAcceptedOffline` is currently EXCLUDED from the
  generator + directed-covered only for its online-origin REFUSAL
  (`operator_complete_offline_kind_on_online_hold_refused_intact`); the RELEASE path is the new work.
- The model's `released_witness` already encodes `NotAcceptedOffline` at the ORIGIN axis (releases iff
  offline-origin) — extend for the cohort-cancel + rewind effects.

### Then (documented, later):
- step-4 fence-IDENTITY strengthening (P3): the fence NAMES this doc's reservation at the CURRENT
  `delivery_generation`, not merely non-NULL (mig 034).
- step-5 RETURN idempotent replay (P2).
- step-6 **CS-1 re-baseline + re-mint manifests + PR** — BATCHED at the true tip, do NOT run early.

---

## 3. KNOWN TRAPS (verified this session)

1. **Capstone uses `RngSeed::Random`** → each `harness_online/offline_seeded` run explores DIFFERENT
   cases. "141/141 GREEN" was one lucky run; a re-run can surface a NEW latent case (that is exactly
   what happened this session). A new find persists to `tests/invariant_fuzzer.regressions` (2 seeds
   pinned this session). Treat a fresh RED as a finding to TRIAGE (systematic-debugging), not a
   rebase regression — first prove whether the prod delta is even reachable by the failing ops.
2. **Run capstones with `TMPDIR` on DISK** (e.g. `/home/setter/tmp/fuzz`), NOT `/dev/shm` — the
   per-case temp-DBs leak and can exhaust RAM-tmpfs. `export TMPDIR=/home/setter/tmp/fuzz`.
3. **Match refusal Display strings, not variant names** — `RealOutcome::Refused(String)` wraps
   `ResolutionRefused { reason: <Display of CompletionError> }`.
4. **fmt reformats multi-line `assert!`/`assert_ne!`** — run `cargo fmt -p prro` (not just --check)
   before committing; the CI gate is `fmt --check`.
5. **Force-push blocked** (§1). `git fetch` intermittently flakes — retry; `git ls-remote
   refs/heads/main` for the authoritative tip.
6. **Adding tests → re-mint manifests** (`scripts/cs1r/mint_manifests.sh`) BEFORE pushing (control-1
   live==committed). Do it ONCE at the true tip (step-6), NOT per increment.
7. **Linker OOM on heavy PRs** — see `[[reference_ci_linker_oom_and_supersession_hygiene]]`
   (`CARGO_BUILD_JOBS=1`, x86 CI ~28-31 min). Supersession registry: prune stale rows after merge.
8. **Prod is FROZEN** — the committed diff must not touch `rust/prro/src` / `rust/prro-domain/src`.
   A hot-zone canary (temporarily reverting a prod guard to prove a tooth bites) is OK ONLY if
   restored + verified pristine (`git diff -- <file>` empty) BEFORE committing.

---

## 4. Verification / gate checklist

```bash
export PATH="$HOME/.cargo/bin:$PATH"
export TMPDIR=/home/setter/tmp/fuzz        # disk, not /dev/shm
cd /home/setter/prro-gate-wt/fuzzer-cs3/rust
cargo fmt -p prro -- --check
cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings
cargo nextest run -p prro --features test-support --locked -E 'binary(invariant_fuzzer)'   # 147/147 now
cargo nextest run -p prro --features test-support --locked                                  # full
# large-N before PR:
FUZZ_CASES=4096 cargo nextest run -p prro --features test-support --locked -E 'test(/^harness_(online|offline)_seeded$/)'
```
Each RED-tooth: personally introduce → RED → revert (canary), naming the test. Prod pristine after.
Pre-push CI gate: `[[feedback_pre_push_ci_gate_checklist]]`.
