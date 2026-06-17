# Invariant Fuzzer — Phase 3: Oracle Honesty + Prod Guard (design spec)

**Date:** 2026-06-17
**Status:** locked (architect). Implementer executes unit-by-unit, RED-first; checkpoints in §9.
**Scope:** oracle false-negative closures (tests) + ONE production runtime guard (`src`, hot-zone). Model-fidelity (D-cluster) is explicitly DEFERRED to Phase 1 WebCheck — see §10.
**Predecessors:** Phase 2 durability MERGED (U1 RAII `#200` / U2 seed-persist `#201` / U3 CI gate `#202`). The fuzzer is now an **ENFORCED merge gate** — `x86_64-unknown-linux-gnu` (full nextest suite) is a required status check (+ `rust-prro-skip.yml` companion).
**Source backlog:** `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` (Cluster C: O1–O5, D1–D5, C1–C3, X1–X2).

---

## §0 Intent & why now

Phase 2 made the fuzzer a *reliable, enforced* gate (no leak, committed seeds, gnu required). But the dry-run found the oracles carry **false-negative zones** — the gate can pass while a real bug exists.

With the gate now **ENFORCED**, a blind oracle is no longer merely "green when run" — it is **enforced false confidence on fiscal correctness**: the required check officially certifies "clean" while a bug slips through. For an edge fiscal system (a lost or double-fiscalized receipt = the merchant's tax liability), that is the most expensive failure mode, and enforcement *amplifies* the cost of every oracle blind spot.

Phase 3 protects the capital already invested in the gate by making its green **honest** (close the highest-leverage oracle false-negatives), and adds the one missing **production** runtime guard for the #192/P1 ledger-pin.

## §1 Goals / Non-goals

**Goals**
- **G1 (oracle honesty)** — close the highest-leverage oracle false-negatives (O1/O2/O3/O5) so the fuzzer catches the bug classes it currently blesses as clean.
- **G2 (prod guard)** — the `StuckNonTerminalDoc` ledger-pin (the #192/P1 invariant) is enforced at RUNTIME in production, not only in tests (X1).
- **G3 (crash reach)** — the generator reaches the crash windows where the #192-class orphan is born (O4 `Crash(Finalize)`/`Crash(OfflineAck)`; generative `Crash(Sign)`; adjacent durable-`SIGNED` probe).

**Non-goals**
- **N1 — model-fidelity / vacuity (D1–D5).** Do NOT hand-build independent model prediction for `next_lnd` / mode / shift / Fault-deferral. **Phase 1 WebCheck** solves this at the root (ground-truth replaces the hand model); hand-polishing a model WebCheck will replace = paying twice. A trivially-cheap D-close may ride along (e.g. D3 fork-the-const + `debug_assert`), but the deep fidelity work is OUT. See §10.
- **N2 — coverage expansion (C1/C2/C3).** A separate tranche. C1 (offline time/code-limit, Frozen Invariant #5) is legal-critical and HIGH value but is *coverage* (new variant + fixture), not oracle-honesty — recommended follow-on (§10).
- **N3 — WebCheck itself (Phase 1)** — the flagship, separate next investment.

## §2 Verified baseline (architect-read on `d60e665`)

- **X1:** `invariant_scan` is `#![cfg(any(test, feature = "test-support"))]` (`invariant_scan.rs:33`) → **ZERO `src/` callers** (grep-confirmed). `StuckNonTerminalDoc` flags `{PREPARED, SIGNED, ENCRYPTED}` (`invariant_scan.rs:60-63`). `run_boot_reconciliation` is `boot_phase.rs:1638` (the prod-guard slot is its tail). ⚠ `invariant_scan` is **not prod-compilable** (test-gated) → U1 needs a small *dedicated prod* query, NOT enabling `test-support` in prod.
- **O1:** `online_convergence::run_tick_for_fn` (`online_convergence.rs:106`) is **never called by the interpreter** (grep-confirmed: no `online_convergence` reference under `tests/invariant_fuzzer/`). Offline cohort gets a real drain-tick at settle; online docs get nothing → `SENT`/`KVT1`/`ERROR_RETRYABLE` blessed clean (the §15 exclusion written for the offline cohort, applied to online too).
- **O2:** MH bounded postconds run only for `Drain|RepeatDrain|GoOnline|GoOnlineWithoutBacklog`; a `Crash(_)`/`Reboot` falls to `model.resync_from_db` with zero independent check, and its differential is `Ok(())` unconditional (`oracle.rs`). A `Crash(Send)` on an **Offline** node completes a *real* offline sell (mints `OFFLINE_LOCAL_ACK`, consumes a code) that is never differential- or bounded-checked.
- **O3:** chain oracle is **referential, not cryptographic** — model assigns `synth_unsigned_hash(lnd)`; the differential explicitly never compares bytes (`oracle.rs:145-155`); the L2 chain walk compares `previous_hash` against the *stored* `unsigned_xml_sha256` of the prior doc and **never recomputes** `sha256(canonical_xml)` (`invariant_scan.rs:223-282`). A wrong-but-consistently-threaded hash passes L1 + ChainBreak + ChainSeedMismatch.
- **O4:** generator emits **only** `Crash(Send)`/`Crash(Kvt1)` (`strategy.rs:52`); `Crash(Sign)` is **directed-only** (`interp::crash_after_sign`); `Crash(Finalize)` + `Crash(OfflineAck)` are `unimplemented!()`. The `OfflineAck` window is the **#192 birth site** — the fuzzer can never crash *inside* the envelope where the orphan is born.
- **O5:** the terminal `ArtifactNoResend` branch of `settle_and_scan` asserts only `send_calls == sends_before` and runs neither `scan()` nor `check_mirrors` — suppression is *total*, so any co-resident `ChainBreak`/`DuplicateLnd`/Mirror-2 desync is skipped with no later boundary to re-catch it.
- **Oracle check fns (current):** `oracle.rs` — `classify:60`, `check_differential:79`, `check_ledger_delta:169`, `assert_crash_send_recovery:198`.

## §3 Units

### U1 — X1 prod runtime guard (⚠ SRC, hot-zone) — Goal G2

Wire a **cheap, prod-compilable** `StuckNonTerminalDoc` (+ `StuckSending`) check into `run_boot_reconciliation`'s tail (`boot_phase.rs:1638`+) so the #192/P1 ledger-pin is enforced at **runtime**, not only in tests.

- **NOT** by enabling `test-support` in prod and **NOT** by calling the test-only `invariant_scan`. Add a small dedicated prod query: docs resting in `{PREPARED, SIGNED, ENCRYPTED}` at the quiescent boot boundary that are not legitimately in-flight.
- **MUST be mode-gated** — do not flag the *transient* `BLOCKED` / `GoingOnline`-resting docs (the same SETTLED scan-gate the O-series uses). A runtime scan over a transient mode would false-flag.
- **Decision (locked):** **WARN-audit + health-degraded**, NOT hard-fail-boot. A stuck doc must surface loudly (audit `CRITICAL` + `/health/ready` degraded) but should not necessarily block boot/serving — graceful behavior over "finishing fast" (Frozen #9). Reconsider only if a checkpoint shows a stuck doc is unsafe to serve alongside.
- **Hot-zone discipline (CLAUDE.md):** plan-first; minimal diff; **frozen-invariants check** (esp. #8 recovery-not-silently-violate, #9 graceful-shutdown); targeted tests; summarize state-machine impact.
- **ШАГ-0:** confirm the prod-compilable shape (a dedicated SELECT, not the cfg-gated scan); confirm the mode-gate predicate; confirm where in the tail (after the per-doc loop, before the final `Ok`).

### U2 — Oracle honesty teeth (O1/O2/O3/O5) — Goal G1 — tests-only

The core of Phase 3 — convert the false-negative zones into real assertions.

- **O1** — drive `online_convergence::run_tick_for_fn` (Ack-loaded) in `settle_and_scan` **symmetric with** the offline drain-tick, so a doc that *should* converge but doesn't becomes a liveness failure (instead of being blessed as "resting").
- **O2** — when a "crash" returns a completed `Doc`, route it through the **same differential** the `PredictableMutating` arm uses (the model can predict an offline sell deterministically); and **extend the bounded postconds** to `Crash`/`Reboot`.
- **O3** — recompute the content hash for the docs the fuzzer mints (**wire the seam's canonicaliser — do NOT reimplement**), at least behind the directed pins, so a wrong-but-self-consistent hash is caught.
- **O5** — in the `ArtifactNoResend` branch, run `scan()` but **filter out only the known transient doc by `document_id`**, asserting the remaining violation set is empty (the per-violation filter the §15 contract deferred).
- **X2 (cheap, fold in)** — add `ORDER BY` + a single-active-session guard assert to the `LIMIT 1` active-session lookup (`model.rs:484-489`, `oracle.rs:286-292`).
- **RISK / RED-first:** each O-tooth tightens an oracle → it MUST be validated it does not **false-positive** on legit behavior. RED-first per tooth: the tooth PASSES on main, and FAILS when the corresponding blind-spot bug is reintroduced (teeth). Run the full capstone after each to confirm no new false-positive.

### U3 — Generative crash reach (O4 + generative `Crash(Sign)` + adjacent-`SIGNED`) — Goal G3 — tests-only

- **O4** — implement `Crash(Finalize)` + `Crash(OfflineAck)` in the interpreter (currently `unimplemented!()`) and emit them generatively. The `OfflineAck` window is the #192 birth site — this lets the fuzzer crash *inside* the orphan-minting envelope.
- **Generative `Crash(Sign)`** — emit it generatively with **"no new op until reboot while a crash is pending"** harness-realism (else a `[Crash(Sign), OnlineSell, …]` buries the SIGNED doc under a later-issued one — an unreachable prod state = false artifact).
- **Adjacent durable-`SIGNED` probe** — let the generator reach `NodeBlocked`-permanent / `ShiftNotOpened` / `NoActiveSession` resuming-`SIGNED` (the classes P1 deferred). **On confirmed reachability → a NEW finding with a proven repro and its own fix** — do NOT bundle a prod fix into this fuzzer tranche (triage separately, per the role split).

## §4 Sequencing & risk

**Order:** **U2 (oracle teeth) first** — highest-leverage false-negative closure, the thesis; then **U1 (X1 prod guard)** as a careful independent SRC tranche (can run in parallel — different files); **U3 (generative reach) last** — it depends on the oracles being honest, else a "new find" can't be trusted as real.

**Risks:**
- **R1 (U2)** — over-strict oracle → **false-positive** flagging legit behavior. Mitigation: RED-first per tooth + full-capstone re-run.
- **R2 (U1)** — SRC hot-zone (`run_boot_reconciliation`). Mitigation: plan-first, mode-gating, frozen-invariant check, careful review; WARN-not-hard-fail.
- **R3 (U3)** — may surface NEW real bugs (adjacent-`SIGNED`). Mitigation: triage with a proven repro; separate fix PR; never bundle into the fuzzer tranche.

## §5 Acceptance

- **A1 (U1):** a stuck `{PREPARED,SIGNED,ENCRYPTED}` doc at a quiescent boot boundary is detected **at runtime in a prod build (no `test-support`)** and surfaced (audit `CRITICAL` + health degraded), mode-gated (no false-flag on transient `BLOCKED`). Teeth: revert the guard → the directed test misses it.
- **A2 (U2):** each of O1/O2/O3/O5 has a directed test that PASSES on main and FAILS when its blind-spot is reintroduced (teeth); the full harness stays green (no false-positive on the existing capstone).
- **A3 (U3):** `Crash(Finalize)` + `Crash(OfflineAck)` are reachable generatively; generative `Crash(Sign)` does not false-flag buried-`SIGNED`; the adjacent-`SIGNED` probe runs (yielding either a clean result or a triaged finding).

## §6 Cross-cutting invariants & discipline

- **Tests-only for U2/U3; U1 is the ONLY `src/` change** (hot-zone — §3 discipline). Determinism of replay preserved in every unit.
- **Each unit its own PR** (base `main`), now gated by the required `x86_64-unknown-linux-gnu` (full suite). Local gate before each handoff: `cargo fmt … --check` + `cargo clippy … -D warnings` + `cargo nextest run -p prro --features test-support`.
- **Isolated worktree** per worker (dual-session shared-tree hazard).
- **Delivery:** 7-item report per unit (Intent · Files · Tests/checks with output · Result · Known risks · Invariant check [determinism; tests-only or hot-zone-justified] · Next).

## §7 Sequencing summary

U2 (oracle teeth O1/O2/O3/O5 + X2) → U1 (X1 prod guard, parallel SRC) → U3 (generative O4 + Crash(Sign) + adjacent-SIGNED). Phase 3 done when A1 ∧ A2 ∧ A3, full gate green, determinism intact.

## §8 Risks & checkpoints (stop-and-ask)

- **CP1 (U1):** if a prod-compilable subset can't be cheaply expressed, or the mode-gate predicate is ambiguous, or a stuck doc turns out unsafe to serve alongside (hard-fail needed) → checkpoint.
- **CP2 (U2):** if an O-tooth false-positives on legit capstone runs → checkpoint (the tooth is over-strict; re-scope).
- **CP3 (U3):** adjacent-`SIGNED` probe yields a reachable orphan → STOP the fuzzer tranche, file the finding + proven repro, hand the prod fix as a separate contract.
- **CP4:** any U2/U3 unit that appears to need a `src/` change (other than U1) → STOP, this is tests-only.

## §9 Locked decisions / deferred

- **D1–D5 model-vacuity → Phase 1 WebCheck.** WebCheck replaces the hand model with ground-truth (old system + live DBs), fixing adoption-vacuity at the root. Hand-polishing the model now = building it twice. A trivially-cheap D-close (D3: fork the `OFFLINE_ISSUED_STATES` literal in the model + `debug_assert` equals the const) MAY ride along with U2; the deep fidelity (D1 derive `next_lnd`, D2 predict mode/shift, D4/D5 promote Fault-deferrals) is WebCheck's.
- **C1 offline-cap (Frozen Invariant #5, legal) → recommended FOLLOW-ON.** High value — a hard legal limit, presently unmodeled, and the fuzzer calls `backlog_drain::drain` directly, bypassing the caller-side time-window gate. But it is *coverage* (an `OfflineCapExceeded` variant + a real `max_offline_codes` fixture), not oracle-honesty → its own tranche after/parallel to Phase 3. C2 (`(OfflineLocalAck,Cancelled)`) lives the day Cancel is wired (Phase-2 RETURN); C3 lower.
- **WebCheck (Phase 1)** — the flagship next investment after Phase 3.

## References
- `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` — Cluster C source (O/D/C/X).
- `docs/superpowers/specs/2026-06-17-fuzzer-durability-phase2-design.md` — Phase 2 (predecessor).
- `docs/superpowers/plans/2026-06-17-optimal-roadmap.md` — long-run roadmap (Phase 3 = oracle false-negatives; Phase 4 = WebCheck/RETURN).
- `rust/prro/tests/invariant_fuzzer/TEETH_TEST.md` — teeth discipline.
