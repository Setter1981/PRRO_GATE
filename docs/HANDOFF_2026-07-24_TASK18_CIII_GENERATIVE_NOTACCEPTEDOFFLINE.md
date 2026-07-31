# Handoff — task #18 (C-iii) GENERATIVE `NotAcceptedOffline` (OLA-cohort cancel + chain rewind)

**Date:** 2026-07-24 · **Author session:** C-i + C-ii + C-iii-directed.
**Audience:** next (fresh) session. Everything below is grounded (file:line verified this session).
Memory: `[[project_cs3_fuzzer_oracle_state]]`. Full detail in `docs/CS3_FUZZER_ORACLE_DOSSIER.md`.

---

## 0. TL;DR

- ✅ **DONE (committed, NOT pushed) on `fuzzer-cs3-oracle`, tip `4aafc978`:**
  - **C-i** origin-keyed held witnesses (`bc15bc01`) + **fence-AUTHORITY MAJOR** from external audit
    (`6b408d4e`).
  - **C-ii** drain-produced held witness generative wiring + delegation pinned (`87815a44`).
  - **C-iii DIRECTED slice** — `NotAcceptedOffline` OLA-cohort cancel + rewind + fork-guard (`4aafc978`).
  - Fuzzer **160/160**, fmt + clippy `-D` clean, prod FROZEN, C-i large-N + C-ii large-N GREEN (4096×2 each).
- ⬜ **NEXT (this handoff): GENERATIVE `NotAcceptedOffline`** — remove the generator exclusion, teach the
  model to INDEPENDENTLY predict the cohort-cancel + rewind + fork-guard for arbitrary prehistories,
  wire the relational oracle into `run_harness`. **The genuinely-hard piece** — an adjudication-heavy
  RED-GREEN loop (expect 3-6 model-vs-prod divergences to adjudicate, like Increment 1b). Do it fresh.

---

## 1. Exact state

- **Branch:** `fuzzer-cs3-oracle`, worktree `/home/setter/prro-gate-wt/fuzzer-cs3`, tip **`4aafc978`**
  **NOT PUSHED** (`origin/fuzzer-cs3-oracle` still `d772fecb`; push needs a force-push — blocked by the
  project guardrail → ask the operator's `!`-prefix OR a fresh PR). Do NOT `git push --force` yourself.
- **Prod is FROZEN** — the committed diff must not touch `rust/prro/src` / `rust/prro-domain/src`
  (verified 0-diff every commit). A hot-zone canary is OK only if restored pristine before committing.
- **`TMPDIR=/home/setter/tmp/fuzz`** (disk, NOT `/dev/shm` — per-case temp-DBs leak RAM-tmpfs).
- **Toolchain:** `export PATH="$HOME/.cargo/bin:$PATH"`.

---

## 2. The empirical GROUND TRUTH (already verified — do NOT re-derive)

A `NotAcceptedOffline` completion on an OFFLINE-origin held doc (cross-check: online-origin → refused
`OriginMismatch`, already directed-covered) RELEASES with the full gap-4b effect. Probe scenario
(`2×OfflineSell → GoOnline([Reject])` holds `OFFLINE_SESSION_BEGIN` lnd1; lnd2/lnd3 = later
`OFFLINE_LOCAL_ACK` SELL successors):

- **Outcome:** `Released(apply_state=APPLIED / node_mode=GOING_ONLINE / fence_held=false / doc=RMR)`.
- **Durable:** held doc → `REQUIRES_MANUAL_RECONCILIATION`; lnd2 & lnd3 → **`CANCELLED`**; node seed
  **rewound `Some([17,…]) → None`** (held doc lnd1, its `previous_hash` is genesis).

**Prod machinery (already read, `delivery_reservation.rs`):**
- release branch **1451-1481**: `offline_cohort_cleanup` + rewind `node_state` seed to the held doc's
  own `previous_hash` (`Some(prev)` → predecessor, `None` → genesis) + `doc_to_rmr`.
- **`offline_cohort_cleanup` 1579-1643**: scan same-session docs with `lnd > this_lnd`;
  `OFFLINE_LOCAL_ACK` → cancellable (CAS → `CANCELLED`); `CANCELLED`/`ABORTED` → ignore;
  `SENDING` → `LaterSuccessorInFlight` (refuse); `PREPARED`/`SIGNED`/`ENCRYPTED` → `LaterSuccessorInvalidState`
  (refuse); **anything else (ISSUED: SENT/KVT1/KVT2/ERROR_RETRYABLE/REJECTED/RMR/ACK)** →
  `LaterSuccessorIssued` (refuse — fork guard). Any refuse rolls the WHOLE tx back (nothing mutated).

The C-iii DIRECTED teeth (`4aafc978`) already pin this CONTRACT against the real ledger — they are the
verified TARGET the generative model must match.

---

## 3. The design worked out this session (implement this — don't re-derive)

### 3a. Model state gap
`RefModel` (`tests/invariant_fuzzer/model.rs:99`) tracks `docs: BTreeMap<i64, DocState>` (state by lnd),
`held_reservation: Option<(i64 lnd, bool online_origin)>` (re-synced from reality by
`sync_held_reservation`), and `seed: Option<...>`. It does **NOT** track per-lnd `previous_hash` nor
`offline_session_id` per doc.

### 3b. Extend `apply_operator_complete` (`model.rs:444`) for `NotAcceptedOffline`
After the existing `released_witness` Some/None gate (which already handles the origin cross-check),
special-case `NotAcceptedOffline` BEFORE the generic doc/mode/seed handling (~line 468):

```rust
if matches!(kind, OperatorResolutionKind::NotAcceptedOffline) {
    // FORK GUARD: a later successor that is not cancellable-OLA / dead cannot be rewound away →
    // prod REFUSES, nothing mutated. (Same-session ≈ all later lnds — see the 3d approximation.)
    let later_fork = self.docs.range((held_lnd + 1)..).any(|(_, st)| !matches!(
        st, DocState::OfflineLocalAck | DocState::Cancelled | DocState::Aborted));
    if later_fork { return ExpectedOutcome::Release(None); }   // Refused (hold intact)
    // COHORT-CANCEL: later OFFLINE_LOCAL_ACK successors → CANCELLED.
    let cancel: Vec<i64> = self.docs.range((held_lnd + 1)..)
        .filter(|(_, st)| **st == DocState::OfflineLocalAck).map(|(l, _)| *l).collect();
    for l in cancel { self.docs.insert(l, DocState::Cancelled); }
    // REWIND: seed → held doc's previous_hash. STRUCTURAL only (real_advanced==model_advanced); the
    // EXACT value is asserted relationally by run_harness (3c). `None` always registers a change (the
    // offline cohort advanced the seed to Some before the hold), so model_advanced==true==real.
    self.seed = None;
    self.docs.insert(held_lnd, DocState::RequiresManualReconciliation);
    self.mode = node_mode_from_str(witness.node_mode);   // GOING_ONLINE (active session)
    return ExpectedOutcome::Release(Some(witness));
}
```
**Why `self.seed = None` (not per-lnd previous_hash tracking):** the seed is synthetic + compared
STRUCTURALLY only (`real_advanced == model_advanced`, `run_harness` Release arm ~3390 / the recovered
arm ~3340). Rewinding always CHANGES the advanced tip, so a `None` marker registers the change; the
next issuance self-corrects to `synth_unsigned_hash(lnd)`. **Verify no absolute seed-value compare
exists** (`oracle::check_differential` — the comment says "never value-equal"; confirm). If one bites,
fall back to tracking `doc_previous_hash: BTreeMap<i64, Option<[u8;32]>>` populated at the ~13
`self.seed = Some(...)` issuance sites (grep `self.seed = ` in model.rs).

**⚠️ The current `advances_seed=false` for `NotAcceptedOffline` (model.rs:483) is the latent
imprecision** — the above rewind REPLACES it (delete NotAcceptedOffline from the `advances_seed` match
or guard it, since the special-case returns early).

### 3c. Wire the relational oracle into `run_harness` (invariant_fuzzer.rs, the Release arm ~3350)
After a generative `NotAcceptedOffline` that RELEASED (real `RealOutcome::Released`), assert the durable
gap-4b effect relationally (mirror the directed tooth
`directed_not_accepted_offline_cancels_cohort_and_rewinds`): capture the held doc's `request_id` +
`previous_hash` BEFORE the op (the op is `OperatorComplete(NotAcceptedOffline)`; the held doc is
`active_held_reservation()` — capture it in the pre-op block near `held_res_before` at ~2996), then
after: every later `OFFLINE_LOCAL_ACK` successor → `CANCELLED`, held → RMR, `read_seed() ==` the held
doc's `read_previous_hash(&held_rid)`. New accessors already exist: `read_doc_states_by_lnd`,
`read_previous_hash`, `force_doc_state_by_lnd` (interp.rs, added this session).

### 3d. Generator (`tests/invariant_fuzzer/strategy.rs:160-168`)
Remove `NotAcceptedOffline` from the exclusion comment and ADD it to the `OperatorComplete` arm
(currently `Accepted + NotAccepted` only, appended LAST — keep corpus-index order). **Keep `MacReseed`
excluded** (needs the operator's corrected seed — separate).

### 3e. The one-session approximation (DEFENSIBLE for the current alphabet — document it)
Prod scans `offline_session_id = session AND lnd > this_lnd`; the model approximates "same session" as
"all later lnds". SAFE because: on a halted offline hold (STOP_MODE) no online-origin successor can be
issued (a halted node refuses sells), so every `lnd > held_lnd` doc IS an offline-cohort doc. If the
generator ever reaches a multi-session / online-successor state the fuzzer will RED — adjudicate then
(likely track `offline_session_id` per doc).

---

## 4. Method + expected divergences

Mirror Increment 1b: add the model + generator + oracle, run the capstones at a moderate
`FUZZ_CASES` (1500), and TRIAGE each RED as a model-vs-prod adjudication (prod-correct → tighten the
model; a genuine prod defect → a SEPARATE `bd`-finding with `discovered-from`, never worked around).
Candidate divergences to expect: (a) the seed-value structural edge (3b); (b) the one-session
approximation (3e); (c) the fork-guard classification for an exotic later state; (d) `held_reservation`
targeting when a drain holds a non-first doc.

Then: a generative-path directed tooth (`run_harness` on
`[OfflineSell, OfflineSell, GoOnline([Reject]), OperatorComplete(NotAcceptedOffline)]`) whose CANARY
(flip the model's cohort-cancel or rewind) REDs. Keep the 2 existing directed teeth.

---

## 5. KNOWN TRAPS (verified this session)

1. **Capstones use `RngSeed::Random`** — each run explores DIFFERENT cases; a fresh RED is a finding to
   TRIAGE (systematic-debugging), not a regression. A new find pins to `tests/invariant_fuzzer.regressions`.
2. **`TMPDIR` on DISK** (`/home/setter/tmp/fuzz`), not `/dev/shm`.
3. **fmt reformats multi-line `assert!`** — `cargo fmt -p prro` before committing (CI gate is `--check`).
4. **Match refusal Display strings, not variant names** (`RealOutcome::Refused(String)`).
5. **Force-push blocked; `git fetch` flakes** (retry; `git ls-remote refs/heads/main` for the tip).
6. **Prod FROZEN** — 0 `src` diff. Hot-zone canary only if restored pristine before commit.
7. **Two heavy nextest sessions on one target contend on CPU** — a 1s directed test alongside a running
   large-N is fine (compile lock is only held during compile, not test-execution); don't launch TWO
   large-N at once.
8. **Adding tests → re-mint manifests** (`scripts/cs1r/mint_manifests.sh`) — do it ONCE at the true tip
   (step-6 landing), NOT per increment. `[[reference_ci_linker_oom_and_supersession_hygiene]]`.

---

## 6. Verification / gate

```bash
export PATH="$HOME/.cargo/bin:$PATH"; export TMPDIR=/home/setter/tmp/fuzz
cd /home/setter/prro-gate-wt/fuzzer-cs3/rust
cargo fmt -p prro -- --check
cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings
cargo nextest run -p prro --features test-support --locked -E 'binary(invariant_fuzzer)'   # 160/160 now
FUZZ_CASES=1500 cargo nextest run -p prro --features test-support --locked -E 'test(/^harness_(online|offline)_seeded$/)'  # dev loop
FUZZ_CASES=4096 cargo nextest run -p prro --features test-support --locked -E 'test(/^harness_(online|offline)_seeded$/)'  # before commit
```
Each RED-tooth: personally introduce → RED → revert (canary), naming the test. Prod pristine after.

---

## 7. After generative NotAcceptedOffline — the remaining offline/fuzzer runway

- step-4 fence-identity RESIDUAL (standalone fence-pointer invariant on every op; held-witness surface
  DONE in `6b408d4e`).
- step-5 RETURN idempotent replay (P2).
- step-6 **CS-1 re-baseline + re-mint manifests + PR** — BATCHED at the true tip, do NOT run early.
