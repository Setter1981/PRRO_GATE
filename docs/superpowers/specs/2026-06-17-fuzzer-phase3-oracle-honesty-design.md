# Invariant Fuzzer — Phase 3: Oracle Honesty + Prod Guard (design spec)

**Date:** 2026-06-17
**Status:** **REVISED v2 (post external-audit).** locked (architect). Implementer executes unit-by-unit, RED-first; checkpoints in §8.
**Scope:** oracle false-negative closures (tests) + ONE production runtime guard (`src`, hot-zone) + a minimal legal-cap coverage tooth. The **broad** model-fidelity work (D1–D5) is DEFERRED to Phase 1 WebCheck — see §9. One **narrow, explicit** model-prediction slice is carved IN for O2 (see U2/O2).
**Predecessors:** Phase 2 durability MERGED (U1 RAII `#200` / U2 seed-persist `#201` / U3 CI gate `#202`). The fuzzer is now an **ENFORCED merge gate** — `x86_64-unknown-linux-gnu` (full nextest suite) is a required status check (+ `rust-prro-skip.yml` companion).
**Source backlog:** `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` (Cluster C: O1–O5, D1–D5, C1–C3, X1–X2).

> **v2 changelog (external audit, all verified against `d60e665`):** O2 now carves an explicit narrow model-prediction slice (the differential was vacuous — crash→`Fault`→`Ok(())`). O1 split: convergence required only for scripted Match/Ack, `ERROR_RETRYABLE` explicitly out, + negative teeth. U1 insertion corrected (`run_boot_reconciliation` has **multiple early-returns**, no single tail) + health-surface & WARN-vs-block made checkpoints. O3 retargeted to DB-integrity (no callable seam canonicaliser). O5 filter made variant-specific (chain/LND violations have no `document_id`). C1 **promoted to a unit** (legal invariant, enforced gate). Honesty claim narrowed accordingly.

---

## §0 Intent & why now

Phase 2 made the fuzzer a *reliable, enforced* gate. But the dry-run found the oracles carry **false-negative zones** — the gate can pass while a real bug exists.

With the gate now **ENFORCED**, a blind oracle is **enforced false confidence on fiscal correctness**: the required check officially certifies "clean" while a bug slips. For an edge fiscal system (a lost/double-fiscalized receipt = the merchant's tax liability), that is the most expensive failure mode, and enforcement *amplifies* the cost of every blind spot.

Phase 3 makes the gate's green **honest where it claims to be** — closing the highest-leverage oracle false-negatives, adding the missing **production** runtime guard for the #192/P1 ledger-pin, and closing the one **legal** blind spot (offline cap) that an enforced gate must not silently certify. **Honesty scope is explicit:** after Phase 3 the gate is honest on the **O-cluster + ledger-pin + offline-cap**; the **broad model-fidelity** dimension (D1–D5) remains a *declared* blind spot until WebCheck (§9) — not silently, but on the record.

## §1 Goals / Non-goals

**Goals**
- **G1 (oracle honesty)** — close O1/O2/O3/O5 so the fuzzer catches the bug classes it currently blesses.
- **G2 (prod guard)** — the `StuckNonTerminalDoc` ledger-pin (#192/P1) enforced at RUNTIME in production, not only in tests (X1).
- **G3 (crash reach)** — the generator reaches the crash windows where the #192-class orphan is born (O4; generative `Crash(Sign)`; adjacent durable-`SIGNED` probe).
- **G4 (legal cap)** — the offline time/code-limit (Frozen Invariant #5) is modeled, so an enforced gate does not silently certify a legal-limit breach (minimal C1).

**Non-goals**
- **N1 — BROAD model-fidelity (D1–D5).** Do NOT hand-build full independent prediction for `next_lnd` / mode / shift / Fault-deferral. WebCheck solves this at the root; hand-polishing a model WebCheck will replace = paying twice. **Exception (carved in):** O2 requires a *narrow, deterministic* prediction slice (the crash-completed offline sell) — see U2/O2. That single slice is IN; the rest of D stays OUT (§9). A trivially-cheap D3 (fork-the-const + `debug_assert`) may ride along.
- **N2 — WebCheck itself (Phase 1)** — the flagship, separate next investment.
- **N3 — broader coverage C2/C3** — C2 (`(OfflineLocalAck,Cancelled)`) lives when Cancel is wired (Phase-2 RETURN); C3 lower. Out of Phase 3.

## §2 Verified baseline (architect-read on `d60e665`; external-audit-confirmed)

- **X1:** `invariant_scan` is `#![cfg(any(test, feature = "test-support"))]` (`invariant_scan.rs:33`) → **ZERO `src/` callers** (grep-confirmed). `StuckNonTerminalDoc` flags `{PREPARED, SIGNED, ENCRYPTED}` (`invariant_scan.rs:60-63`/`:173-190`). ⚠ `run_boot_reconciliation` (`boot_phase.rs:1638`) has **multiple early-returns** (`:1754/1774/1811/1860/1888/1987/2008/2076`) — there is **no single tail**; a guard "in the tail" would miss branches (audit #3). The prod guard belongs at the **App-level caller after `run_boot_reconciliation` returns** (see U1). `invariant_scan` is **not prod-compilable** (test-gated) → U1 needs a small *dedicated prod* query.
- **O1:** `online_convergence::run_tick_for_fn` (`online_convergence.rs:106`) is **never called by the interpreter** (grep-confirmed). It selects **only `SENT`/`KVT1`** cohorts (`online_convergence.rs:137-143`) — **NOT `ERROR_RETRYABLE`** (audit #2). It has **legitimate non-converging outcomes**: KVT1 hold/superseded (`:237-250`), SENT not-found / transport-hold (`:192-210`). Offline cohort gets a real drain-tick at settle (budget 3, `invariant_fuzzer.rs:830-862`); online docs get nothing → they are blessed clean.
- **O2:** crash differential is **vacuous** (audit #1, verified): `Op::Crash(_) | Reboot | RepeatReboot => ExpectedOutcome::Fault` (`model.rs:177`); `check_differential` for `FaultOrRecovery => Ok(())` unconditional (`oracle.rs:85`); after a fault the harness **adopts** from DB (`invariant_fuzzer.rs:985`, `model.rs:413-415`). The model *can* predict a plain offline sell (`model.rs:261-286`) but the **crash path never uses it**. MH bounded postconds run only for `Drain|RepeatDrain|GoOnline|GoOnlineWithoutBacklog` (`invariant_fuzzer.rs:940-943`).
- **O3:** chain oracle is **referential, not cryptographic** — model assigns `synth_unsigned_hash(lnd)`; differential never compares bytes (`oracle.rs:145-155`); L2 walk compares the **stored** `unsigned_xml_sha256`, never recomputes `sha256(canonical_xml)` (`invariant_scan.rs:233-282`). ⚠ No publicly-callable seam canonicaliser: `stage_sign`'s build/sign helper is private (`stage_sign.rs:657-714`); `build_canonical_xml` needs an already-built `CanonicalDoc` (`xml/mod.rs:800`); the persisted payload+hash land in `document_files`/`unsigned_xml_sha256` (`stage_sign.rs:431-441`) (audit #4).
- **O4:** generator emits **only** `Crash(Send)`/`Crash(Kvt1)` (`strategy.rs:29-52`); `Crash(Sign)` directed-only (`interp::crash_after_sign`, `interp.rs:562-631`); `Crash(Finalize)`/`Crash(OfflineAck)` `unimplemented!()`. `OfflineAck` = #192 birth site.
- **O5:** the `ArtifactNoResend` branch (`invariant_fuzzer.rs:892-905`) asserts only `send_calls == sends_before` and runs neither `scan()` nor `check_mirrors`. ⚠ Not all `Violation`s are document-scoped: `DuplicateLnd`/`ChainBreak`/`ChainSeedMismatch` have **no `document_id_hex`** (`invariant_scan.rs:42-82`) (audit #5).
- **C1:** the fuzzer calls `backlog_drain::drain` **directly** (`interp.rs:681-729`), which checks mode/session/backlog but **not the offline legal cap** (`backlog_drain.rs:687-773`); the fixture seeds `max_offline_codes: 0` (`interp.rs:995-996`). Offline time/code-limit (Frozen Invariant #5) is **unmodeled** (audit #6).

## §3 Units

### U1 — X1 prod runtime guard (⚠ SRC, hot-zone) — Goal G2

Enforce the #192/P1 ledger-pin at **runtime in production**.

- **Insertion (corrected, audit #3; caller VERIFIED):** NOT "the tail of `run_boot_reconciliation`" — it has multiple early-returns. The caller is **`App::reconcile_pending_inner`**, which invokes `boot_phase::run_boot_reconciliation` **per-FN in a loop** (`app.rs:581`). Run the guard at that caller, **after each FN's `run_boot_reconciliation` returns** (the per-FN post-reconciliation boundary), so every `BranchOutcome` branch is covered by one check. ШАГ-0: decide per-FN-after-each vs once-after-the-loop-over-all-FNs, and confirm the guard sees the settled state.
- **Query:** a small dedicated **prod-compilable** SELECT for docs resting in `{PREPARED, SIGNED, ENCRYPTED}` (+ optionally `StuckSending`) at the quiescent boundary — NOT the cfg-gated `invariant_scan`, NOT `test-support` in prod.
- **MUST be mode-gated** — do not flag transient `BLOCKED`/`GoingOnline`-resting docs (the SETTLED gate the O-series uses).
- **Effect = CHECKPOINT (audit #3 + self-verified), not yet locked.** ⚠ **VERIFIED: the prro CORE has NO readiness surface** — `/health/*` routes exist only in the separate `prro_sidecar` (license check) and `prro_escpos_daemon` binaries, NOT in the gateway core; CLAUDE.md's `/health/ready (post-recovery)` describes the **dead Python contour** (its paths are Python), not the Rust core. So there is **no existing toggle to degrade**. CP1 must choose: **(a)** audit `CRITICAL`-only — weak (a warning nobody acts on, per the audit); or **(b)** build a NEW FN-level readiness/degraded marker (e.g. a `node_state` flag + a real readiness gate) with a verifiable effect — REQUIRED if a stuck non-terminal doc is unsafe to serve that FN. Decide WARN-vs-block (and whether to build (b)) *with the state-machine owner*, not by default.
- **Hot-zone discipline (CLAUDE.md):** plan-first; minimal diff; **frozen-invariants check** (esp. #8 recovery-not-silently-violate, #9 graceful-shutdown); targeted tests; summarize state-machine impact.

### U2 — Oracle honesty teeth (O1/O2/O3/O5 + X2) — Goal G1 — tests-only

- **O1 (split, audit #2)** — require convergence **only** in deterministic scripted cases where the DPS script guarantees it (`Match`/`Ack`, no RMR/no hold): drive `online_convergence::run_tick_for_fn` (Ack-loaded) at settle and assert the doc reaches `Ack`. **`ERROR_RETRYABLE` is explicitly OUT of O1** (the seam doesn't select it) — handle its redrive separately (note as follow-on) or leave declared-out. **Add a paired NEGATIVE tooth:** a legitimate KVT1 hold / SENT transport-hold must **NOT** be flagged (proves O1 isn't over-strict).
- **O2 (narrow D-slice carved in, audit #1)** — the differential is currently vacuous (crash→`Fault`→`Ok(())`). Fix: when a `Crash(*)` returns a completed `RealOutcome::Doc`, **build the deterministic expected mutation in the model BEFORE `resync_from_db`** (reuse the existing plain offline-sell prediction `model.rs:261-286`) and route it through the `PredictableMutating` differential; extend the bounded postconds to `Crash`/`Reboot`. This is the **one** narrow prediction slice (offline-sell-completed-under-crash) — NOT broad D-fidelity (§9). The genuinely-nondeterministic MAC-recovery crash stays `Fault`-deferred.
- **O3 (retargeted to DB-integrity, audit #4)** — there is no callable seam canonicaliser. Scope O3 to **DB-integrity**: hash the **persisted `PAYLOAD_XML`** (`document_files`) and assert it equals the stored `unsigned_xml_sha256` — catches a stored-hash that doesn't match its stored payload. **Canonical-truth** (recompute from canonical bytes) needs a deliberate `src` seam or a golden/WebCheck oracle → **deferred** (note, don't fake it).
- **O5 (variant-specific filter, audit #5)** — in the `ArtifactNoResend` branch, run `scan()` and allow **only** the expected `StuckSending { document_id_hex }` (filter by that id); **all other violations stay fatal** — `ChainBreak`/`ChainSeedMismatch`/`DuplicateLnd`/session-desync have no `document_id` and must NOT be filtered.
- **X2 (cheap)** — add `ORDER BY` + single-active-session guard assert to the `LIMIT 1` active-session lookup (`model.rs:484-489`, `oracle.rs:286-292`).
- **RED-first + PAIRED negative teeth (audit #8):** each tooth gets BOTH a positive teeth (reintroduce the blind-spot → FAILS) AND a **negative** teeth (a legitimate non-converging/excused-transient scenario → PASSES). The negative tooth is mandatory because a false-positive is now a **merge-blocker** on the enforced gate. Full capstone re-run after each.

### U3 — Generative crash reach (O4 + generative `Crash(Sign)` + adjacent-`SIGNED`) — Goal G3 — tests-only

- **O4** — implement `Crash(Finalize)` + `Crash(OfflineAck)` in the interpreter and emit them generatively (the `OfflineAck` window = #192 birth site).
- **Generative `Crash(Sign)`** — emit generatively with **"no new op until reboot while a crash is pending"** harness-realism (else buried-`SIGNED` false artifact).
- **Adjacent durable-`SIGNED` probe** — let the generator reach `NodeBlocked`-permanent / `ShiftNotOpened` / `NoActiveSession` resuming-`SIGNED`. **On confirmed reachability → a NEW finding with a proven repro + its own fix PR** — never bundle a prod fix into this fuzzer tranche (CP3).

### U4 — Minimal C1: offline legal-cap modeled (audit #6) — Goal G4 — tests-only

The offline time/code-limit (Frozen Invariant #5) is a **hard legal limit**, presently unmodeled, while the gate is enforced. Minimal closure:
- Add an `OfflineCapExceeded` scan variant (a sell that consumes beyond `max_offline_codes` or past the time window).
- Add a fixture seeding a **real** `max_offline_codes` (current fixture seeds `0`).
- Note: the fuzzer calls `drain` directly, bypassing the caller-side time-window gate — model the cap at the *issuance* point the cap actually binds.
- Scoped minimal (the variant + fixture + one directed pin); full offline-cap coverage can extend later.

## §4 Sequencing & risk

**Order:** **U2 (oracle teeth) first** — highest-leverage; then **U1 (X1 prod guard)** as a careful independent SRC tranche (parallel — different files); **U4 (C1)** alongside U2 (tests-only, independent); **U3 (generative reach) last** (depends on honest oracles).

**Risks:**
- **R1 (U2)** — over-strict oracle → **false-positive = merge-blocker** on the enforced gate. Mitigation: **paired negative teeth** per tooth (not just positive) + full-capstone re-run. O1 and O5 are the highest false-positive risk.
- **R2 (U1)** — SRC hot-zone; insertion point + readiness surface + WARN-vs-block unresolved → CP1.
- **R3 (U2/O2)** — the narrow D-slice could itself be vacuous if it adopts; the predicted mutation must be **independent** of the post-crash DB read.
- **R4 (U3)** — may surface NEW real bugs (adjacent-`SIGNED`) → triage with proven repro, separate fix PR (CP3).

## §5 Acceptance

- **A1 (U1):** a stuck `{PREPARED,SIGNED,ENCRYPTED}` doc at the post-reconciliation boundary is detected **at runtime in a prod build (no `test-support`)** and surfaced with a **verifiable effect** (audit `CRITICAL` + the readiness effect resolved in CP1), mode-gated (no false-flag on transient `BLOCKED`). Teeth: revert guard → directed test misses it.
- **A2 (U2):** each of O1/O2/O3/O5 has BOTH a positive teeth (reintroduced blind-spot FAILS) AND a **negative teeth** (legitimate scenario PASSES); O2's narrow prediction is independent of the post-crash DB read; full harness green (no capstone false-positive).
- **A3 (U3):** `Crash(Finalize)`+`Crash(OfflineAck)` reachable generatively; generative `Crash(Sign)` no buried-`SIGNED` false-flag; adjacent-`SIGNED` probe runs (clean or triaged finding).
- **A4 (U4):** an offline sell beyond `max_offline_codes` / past the window is flagged by the `OfflineCapExceeded` variant against a real-cap fixture; teeth: a within-cap sell PASSES.

## §6 Cross-cutting invariants & discipline

- **Tests-only for U2/U3/U4; U1 is the ONLY `src/` change** (hot-zone). Determinism of replay preserved in every unit.
- **Each unit its own PR** (base `main`), now gated by the required `x86_64-unknown-linux-gnu`. Local gate: `cargo fmt … --check` + `cargo clippy … -D warnings` + `cargo nextest run -p prro --features test-support`.
- **Isolated worktree** per worker. **Delivery:** 7-item report per unit.

## §7 Sequencing summary

U2 (O1/O2/O3/O5 + X2, paired teeth) ∥ U4 (C1) → U1 (X1 prod guard, SRC) → U3 (generative). Done when A1∧A2∧A3∧A4, full gate green, determinism intact.

## §8 Risks & checkpoints (stop-and-ask)

- **CP1 (U1) — three open questions to resolve in ШАГ-0:** (a) exact App-level insertion point (post-`run_boot_reconciliation`), (b) the real readiness surface (don't assume a `/health/ready` toggle), (c) WARN-vs-FN-block — if a stuck non-terminal doc is unsafe to serve, a warning is insufficient → FN-level degraded readiness with verifiable effect. Decide with the state-machine owner.
- **CP2 (U2/O1, O5):** if a tooth false-positives on a legitimate scenario (KVT1 hold; a non-document-scoped violation in the ArtifactNoResend branch) → re-scope; the negative teeth must pass first.
- **CP3 (U3):** adjacent-`SIGNED` probe yields a reachable orphan → STOP, file finding + proven repro, separate fix contract.
- **CP4 (O2):** if the narrow crash-prediction can only be expressed by adopting the post-crash DB (i.e. it's vacuous) → it belongs to WebCheck, not Phase 3 — checkpoint and move O2 out.
- **CP5:** any U2/U3/U4 unit that appears to need a `src/` change (other than U1) → STOP, this is tests-only.

## §9 Locked decisions / deferred

- **BROAD model-fidelity D1–D5 → Phase 1 WebCheck** (root-cause; don't build the hand model twice). **Carved exception:** O2's *narrow* deterministic crash-completed-offline-sell prediction is IN (U2/O2) — it is the minimum to make the crash differential non-vacuous; the rest (D1 derive `next_lnd`, D2 predict mode/shift, D4/D5 promote Fault-deferrals) stays WebCheck's. D3 (fork-const + `debug_assert`) may ride along.
- **O3 canonical-truth recompute → WebCheck / golden oracle** (no callable seam canonicaliser). Phase-3 O3 is the achievable DB-integrity subset (persisted-payload-vs-stored-hash).
- **C2/C3** — C2 lives when Cancel is wired (Phase-2 RETURN); C3 lower.
- **WebCheck (Phase 1)** — the flagship next investment after Phase 3; it subsumes the deferred D-fidelity and O3-canonical-truth.

## References
- `docs/superpowers/audits/2026-06-16-invariant-fuzzer-dryrun-findings.md` — Cluster C source.
- `docs/superpowers/specs/2026-06-17-fuzzer-durability-phase2-design.md` — Phase 2 (predecessor).
- `docs/superpowers/plans/2026-06-17-optimal-roadmap.md` — long-run roadmap.
- `rust/prro/tests/invariant_fuzzer/TEETH_TEST.md` — teeth discipline.
