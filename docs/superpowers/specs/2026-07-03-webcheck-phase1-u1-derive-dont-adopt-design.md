# Invariant Fuzzer — Phase 1 U1: Derive-don't-adopt, grounded in OUR pins (design spec)

**Date:** 2026-07-03
**Status:** **LOCKED (architect, 2026-07-03).** Drafted by the implementer; load-bearing pins (D1/D4/D5, scan-host) re-verified by the architect by hand; all five §7 decisions RULED (see §7). Execution: implementer, tests-only, RED-first per closure in the §6 order (D3→D1→D2→D5→D4→funnel), RED evidence per closure in Delivery.
**Scope:** tests-only (CP4 gates any `src/` need). Convert the fuzzer's five DB-adoption sites (D1–D5) into **derived predictions justified by a cited normative pin of ours**, or a **documented bounded deferral**; make adoption **mechanically exhaustive** via an adoption-lint funnel + static-scan. No corpus, no replay (those are U2/U3).
**Predecessor:** **U0 MERGED** (`WEBCHECK_GROUND_TRUTH.md`, PR #212 squash `0306ac1`). U1 is fully unblocked in-repo.
**Authoritative parent:** `docs/superpowers/specs/2026-07-02-webcheck-ground-truth-phase1-design.md` §3/U1 + §5/A1 (LOCKED v2.1).

---

## §0 Intent & why now

The fuzzer's `RefModel` **adopts** the real DB wherever it cannot predict — precisely at D1–D5, where recovery bugs live — making the differential **vacuous** exactly there. U1 removes the vacuity: every adopted dimension becomes either a **derived prediction asserted equal to reality** (grounded in one of OUR pins), or an **explicitly classified, bounded deferral**. The anti-vacuity guarantee is mechanical: after U1, a raw DB read in `model.rs` that is not a classified deferral is a **static-scan failure**, not a silent adoption.

**WebCheck's role in U1 is validation substrate, NOT rule source** (parent §2 reframe). None of D1–D5 transfers from WebCheck; each derives from an **OUR** pin. Field validation of these derived rules lands with U2/U3 (parent §1/G2-G3). Where a rule is ours, it is **labeled ours**.

---

## §1 Phase rules & inputs (binding constraints on this unit)

- **PR-1 (U0 citation gate):** any WebCheck behavior U1 relies on must be cited **only** from `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md` (parent A0). U1 barely touches WebCheck — but where it does (fixture shapes, code-consumption), the citation is a `GROUND_TRUTH §n` reference, never a fresh C# dig.
- **PR-2 (A07-ruling, from U0):** `fns`/offline-number **consumption is modeled only via the insert-trigger path** (WebCheck `fnsupdate10`, INSERT `offline=2` — `GROUND_TRUTH §1.3/§5`). The `offline=3` UPDATE-trigger (`fnsupdateerror5`) is **absent in all 15 field DBs** → **U1/U3 must NOT model a trigger-driven `fns` consumption on `offline=3`.** (Note: our code-consumption is `offline_codes`, not `fns`; this ruling bounds any WebCheck-derived expectation about *when* a code is consumed to the insert-site only.)
- **PR-3 (tests-only / CP4):** if any closure appears to need a `src/` change, **STOP** and open a separate contract. U1 changes live under `rust/prro/tests/**` only.
- **PR-4 (RED-first + paired teeth):** each closure lands RED-first (a pin that fails before the derivation, passes after) **plus a paired negative tooth** (a legitimate scenario that must NOT be flagged). A false-positive is a **merge-blocker on the enforced gnu gate**.

---

## §2 Current adoption inventory (baseline to replace) — verified

`rust/prro/tests/invariant_fuzzer/model.rs` (563 lines) contains **exactly 7** `.fetch_*` sites, all inside two methods; **no inline query exists elsewhere** (this is the entire surface the funnel must fence):

| Method | model.rs | Adopts | Called from | When |
|---|---|---|---|---|
| `resync_from_db` | 456–498 | `docs` (459–462), `mode`+`shift_state`+`next_lnd`+seed-presence (467–474), `session` (482–487), `codes_issued` (488–490), `codes_consumed` (492–497) | `invariant_fuzzer.rs:1037` | on `FaultOrRecovery` ops (Crash/Reboot/exotic-drain) |
| `resync_preconditions_from_db` | 507–536 | `mode`+`shift_state` (508–513), `session` active OPEN/DRAINING (523–528) | `invariant_fuzzer.rs:1239` | after **every** non-fault op |

**7 fetch lines:** 460, 472, 485, 489, 495 (`resync_from_db`); 510, 527 (`resync_preconditions_from_db`).

**Current D1–D5 disposition (what U1 changes):**

| Closure | Today | U1 target |
|---|---|---|
| D1 next_lnd | predicted `+=1` (model.rs:210/254/291) **between** faults; **adopted** on `resync_from_db` (472) | predict-then-**assert equal** to DB; adoption becomes a checked deferral only across genuine faults |
| D2 mode/shift | predicted in `apply()` then **overwritten** by `resync_preconditions_from_db` (510) after every op | predict-then-**assert** for ops the model understands, **before** precondition-resync |
| D3 issued-set | uses **shared** prod const directly (model.rs:22 import; :120/:125) | **fork** into a model-local literal + equality-assert vs prod const (anti-shared-const) |
| D4 BadHashPrev | → `ExpectedOutcome::Fault` (model.rs:245–247) → full resync | apply **no-resend bound** (min) or predict single-shot-stub terminal |
| D5 exotic-drain | → `ExpectedOutcome::Fault` (model.rs:441–443 `_ =>`) + MH bounded-postcond | promote **deterministic** scripts to predicted `Mutated`; MAC-recovery stays deferred |

---

## §3 The five closures (each: OUR-pin → derivation → RED → GREEN → paired teeth)

Line anchors below were machine-verified 2026-07-03 (both migration `001_baseline` and the `025` rebuild are cited where a table was rebuilt — lock against `025` as authoritative-current, `001` as origin).

### D1 — `next_lnd := max(adopted lnds)+1`, asserted equal to DB. **Grounding: ours.**

- **OUR pins:** `ux_fd_fn_lnd UNIQUE(fiscal_number, lnd)` — `migrations/025_fiscal_documents_aborted_state.sql:181` (rebuild) / `001_baseline.sql:352` (origin); `next_lnd INTEGER NOT NULL CHECK (next_lnd >= 1)` — `001_baseline.sql:537`; SSOT allocator `node_state::allocate_next_lnd` — `src/db/repositories/node_state.rs:324`, body `326–328` (`UPDATE node_state SET next_lnd = next_lnd + 1 … RETURNING next_lnd - 1`); call-site `stage_acquire.rs:600`; ADR-M3-A1 (`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md`).
- **Derivation:** the model already increments `next_lnd` per issuing op. U1 makes the increment **load-bearing**: after each op the model asserts its predicted `next_lnd == node_state.next_lnd` and predicted `max(issued lnd)+1 == next_lnd` — i.e. per-FN monotonic, no-gap, matching the allocator SSOT. This is **not WebCheck** (parent HIGH#1: WebCheck `localchecknumber` is per-shift; our lnd is per-FN — `GROUND_TRUTH §4`).
- **RED-pin:** disable the increment (or seed it wrong) → the equality assert fails on the first issuing op. **GREEN:** with the derivation, the predicted value equals the DB value across the seeded harness.
- **Teeth:** `teeth_d1_next_lnd_predicts_db` (POS — a planted off-by-one in the model's next_lnd is caught) + `teeth_d1_gapless_reissue_not_flagged` (NEG — a legitimate abort→reissue that consumes an lnd without gap is NOT flagged).

### D2 — predict-then-assert mode/shift before precondition-resync. **Grounding: ours.**

- **OUR pins:** 9-state `ShiftState` enum — `src/db/models/enums.rs:69–79` (`Created/Opening/OpenedLocalPendingDrain/Opened/ClosingLocalPendingDrain/Closing/Closed/RequiresManualReconciliation/Error`); `NodeMode` enum — `enums.rs:81–89` (`Online/GoingOffline/Offline/GoingOnline/Blocked/StopMode/CryptoDegraded`); M3b spec `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`.
- **Derivation:** for the ops the model **understands** (sell, shift open/close, go-offline/online, session open/drain), predict the resulting `mode`/`shift_state` from the M3b/node-mode transition rules and **assert equal to DB before** `resync_preconditions_from_db` overwrites it. The resync then remains only to carry forward state the model deliberately does not predict (e.g. mid-transition it cannot resolve) — and that residue must be a **classified deferral** (§4), not a silent adoption. WebCheck grounds only trivial open/closed existence — **ours** carries the 9-state richness.
- **RED-pin:** a mis-predicted transition (e.g. predict `Opened` where the machine yields `OpenedLocalPendingDrain`) fails the pre-resync assert. **GREEN:** correct transitions match.
- **Teeth:** `teeth_d2_predicted_shift_matches_db` (POS) + `teeth_d2_mid_transition_deferral_not_flagged` (NEG — a genuinely mid-transition mode the model defers is NOT asserted, so it does not false-fail).

### D3 — fork `OFFLINE_ISSUED_STATES` into a model literal + equality-assert. **Grounding: ours (anti-shared-const).**

- **OUR pins:** prod const `OFFLINE_ISSUED_STATES: [&str; 7]` — `src/db/repositories/fiscal_documents.rs:897–905` (`OFFLINE_LOCAL_ACK, SENT, KVT1, ERROR_RETRYABLE, KVT2, REJECTED, REQUIRES_MANUAL_RECONCILIATION`); `DocState` enum boundary — `enums.rs:29–55`.
- **Derivation:** today the model **imports the prod const** (model.rs:22) — so a prod-side change to the issued-set silently propagates and the "differential" cannot detect a boundary drift. U1 **forks** a model-local literal `MODEL_OFFLINE_ISSUED_STATES` and adds a `debug_assert_eq!`/test asserting it equals `fiscal_documents::OFFLINE_ISSUED_STATES`. Now a boundary change is a **RED test** demanding conscious model update, not a silent inherit. (dry-run D3 in parent §3.)
- **RED-pin:** perturb the model literal (drop/add a state) → equality test fails. **GREEN:** the two sets match.
- **Teeth:** `teeth_d3_forked_set_matches_prod_const` (POS — a forked-literal drift is caught) + `teeth_d3_membership_semantics_unchanged` (NEG — issued/non-issued classification of a known doc is unchanged by the fork).

### D4 — BadHashPrev: no-resend bound (minimum) or predict the single-shot-stub terminal. **Grounding: ours.**

- **OUR pins:** DDL `mac_recovery_attempts INTEGER NOT NULL DEFAULT 0 CHECK (mac_recovery_attempts IN (0,1))` — `025_…:103–104` / `001_baseline.sql:332–333`; W10.4 single-re-entry bound — `stage_send.rs:942–951` (comment), flag `:951` (`let mut mac_recovery_invoked = false`), guard `:970` (`if mac_recovery_invoked { … }`).
- **Derivation:** today `BadHashPrev → Fault → full resync` (model.rs:245–247) — vacuous. U1's **minimum** derivation: assert the **no-resend / bounded-dispatch** postcond — the DDL `IN (0,1)` budget + the W10.4 one-shot flag mean at most ONE MAC-recovery re-entry per `run()`, so the wire send-count is bounded (no unbounded resend). A **stronger** (optional, §7) derivation predicts the exact single-shot-stub terminal (`ERROR_RETRYABLE`/`DpsRejected` per the stub's empty-queue behavior) instead of deferring to Fault.
- **RED-pin:** revert/loosen the send-count bound assert → a resend regression is caught. **GREEN:** the bound holds on the seeded harness.
- **Teeth:** `teeth_d4_badhashprev_no_second_send` (POS — a second wire send on MAC-recovery is caught) + `teeth_d4_single_recovery_reentry_not_flagged` (NEG — exactly one legitimate re-entry is NOT flagged as a violation).

### D5 — promote deterministic exotic-drain scripts to predicted `Mutated`; MAC-recovery genuinely deferred. **Grounding: ours.**

- **OUR pins:** strict-sequential per-doc drain loop — `backlog_drain.rs:928` (`for (position, doc) in backlog.iter().enumerate()`), strict-sequential comment `930–946` (STOP at first non-ACK); RMR halt guard — `backlog_drain.rs:725–726` (`if ns.shift_state == RequiresManualReconciliation { return Ok(DrainSummary::new(fn, 0)); }`), comment `706–714` (AUD-K8-1); drain classifier `kvt2_confirm::classify_check_result` — `kvt2_confirm.rs:301`; M3b §16.7 drain-reject→EscalateManual — `2026-05-17-m3b-shift-state-expansion.md:1117`.
- **Derivation:** promote the **deterministic** exotic scripts the fuzzer emits (`op.rs`: `superseded_tip()` → `[Superseded]`; `send_ack_then_last_not_found()` → `[Ack, NotFound]`) from `Fault` (`_ =>` at model.rs:441–443) to **predicted `Mutated`**, with the predicted terminals **derived from the actual classifier arms**:
  - **Superseded → `SupersededHold`** (kvt2_confirm.rs ~357–365): the doc is not the DPS tip → **held in SENT**, siblings continue.
  - **NotFound (SentReplay) → `SentNotFoundDowngrade`** (~330–337); **NotFound (non-SentReplay) → `StructuralDrift`** (~338–344).
  - MAC-recovery drain scripts remain **genuinely deferred** (labeled, classified deferral — §4).
  - ⚠ **CP2 / §7 open:** parent §3/U1 D5's shorthand *"Superseded→all ERROR_RETRYABLE; NotFound-hold→SENT"* is **looser than the current classifier** (which yields `SupersededHold`(SENT)/`SentNotFoundDowngrade`). U1 must pin the predicted terminal to the **real** `classify_check_result` arms; the architect confirms the exact cohort-level `Mutated` terminal per script at lock. Also honor the RMR halt (`:725`) and strict-sequential STOP (`:928`) — a rejected `OFFLINE_LOCAL_ACK` drain routes to Manual per §16.7, which is a **durable terminal**, not a liveness violation (existing SETTLED-gate ruling).
- **RED-pin:** replace a promoted script's predicted terminal with the wrong state → `check_differential` fails. **GREEN:** predicted terminal matches observed.
- **Teeth:** `teeth_d5_superseded_predicts_held_sent` (POS) + `teeth_d5_mac_recovery_drain_still_deferred_not_flagged` (NEG — the deliberately-deferred MAC-recovery drain is NOT force-predicted).

---

## §4 A1 — adoption-lint funnel + static-scan (mechanical exhaustiveness)

Per parent §5/A1. Goal: **FORBIDDEN is empty** — every DB read in `model.rs` is a classified, intentional deferral, so no future adoption can silently re-open a vacuity.

- **Wrapper funnel:** all DB access in `model.rs` MUST go through **three tagged wrapper fns**, each taking the `&SqlitePool` and returning the read:
  - `read_seed_fixture(...)` — initial/seed state reads (fixture grounding);
  - `adopt_fault_deferred(...)` — post-fault recovery adoption that is **genuinely** not predictable (the residue of `resync_from_db` after D1 removes next_lnd);
  - `adopt_precondition(...)` — mode/shift/session precondition-resync that D2 does not predict (the residue of `resync_preconditions_from_db`).
  Raw `sqlx::` / `query*` / `.fetch_*` / `.execute` calls **outside these wrapper definitions are forbidden**.
- **Static-scan test** (house pattern — lives in the existing `rust/prro/tests/invariant_scan.rs` static-scan file, same text-grep approach as the compile-fail/guard tests): read `model.rs` as text and **FAIL** on any `sqlx::` / `query` / `fetch_` / `.execute` token that is **not** inside one of the three wrapper fn bodies. This is the same source-scan discipline used by existing guard tests (per A's map, `invariant_scan.rs` already hosts grep-based scans).
- **Registry:** each wrapper **call-site** is classified `{ seed-fixture | fault-deferred | precondition-only }`. Any unclassified/raw read = scan failure = **FORBIDDEN non-empty** = A1 red.
- **A1 passes iff:** static-scan green **AND** every D1–D5 teeth pair green.

**Wire-in (from A's map, run_harness `invariant_fuzzer.rs:939–1252`):** D1/D2 predict-then-assert land at **step 2–3** (assert predicted vs observed **before** the resync at step 7 overwrites) and/or replace the adopted fields in the wrappers; D4/D5 land in the `apply`/`classify` path (steps 3–4); the static-scan is a standalone test. The 7 fetch sites (§2) are relocated verbatim into the wrapper bodies.

---

## §5 Acceptance (A1, from parent §5)

- Adoption-lint **registry exhaustive** and **FORBIDDEN empty** (static-scan green).
- **D1, D3 derived** (ours, hard); **D2, D4, D5 derived-or-bounded**, each with an **our-pin citation** (§3).
- **Paired teeth per closure** (pos + neg), all **un-`#[ignore]`d** (house style since #199 — normal CI tests).
- Full `nextest` suite green + elevated-N probe (`FUZZ_CASES` bump) green; both seeded capstones (`harness_online_seeded`, `harness_offline_seeded`) green — replay determinism intact.
- **Delivery citation-inventory (parent A0/new-MED#1):** U1's Delivery lists every WebCheck-behavior claim it relies on, each cross-checked line-by-line against `WEBCHECK_GROUND_TRUTH.md`; a claim absent from U0's doc blocks the unit (extend U0 first).

---

## §6 Sequencing, risks, checkpoints

- **Order (RED-first, one closure at a time):** D3 (pure fork, cheapest) → D1 (predict/assert next_lnd) → D2 (mode/shift) → D5 (promote exotic drains) → D4 (MAC bound) → **then** land the adoption-lint funnel + static-scan (it fences whatever adoption residue remains). Rationale: the funnel is only meaningful once D1–D5 have removed the predictable adoptions; landing it last makes FORBIDDEN-empty a true statement.
- **R2 (mis-derived rule → over-strict oracle blocking merges):** paired negative teeth + reviewable pin citations + full-capstone re-runs. A derived rule that contradicts observed gateway behavior → **CP2 triage** (our bug vs mis-derivation vs intentional difference) — **do not force** (parent §7/CP2).
- **R5 (pre-corpus circularity, honest):** U1's derived rules are validated against our own fuzzer + pins, not yet against field shapes — **U1's Delivery must state this honestly** ("field validation lands with U2/U3"). U2/U3 close it.
- **CP4:** any `src/` need → STOP, separate contract.

---

## §7 Decisions — RULED at lock (architect, 2026-07-03; load-bearing pins re-verified by hand: D1 `025:181`+ADR-comment exact, D4 `001:332-333` CHECK + `mac_recovery_invoked` `stage_send.rs:951/:970` exact, D5 `classify_check_result` at `kvt2_confirm.rs:301` with arms `SentReplay`+NotFound→`SentNotFoundDowngrade` / `SentFresh|Kvt1Reentry`+NotFound→`StructuralDrift` / superseded→`SupersededHold`, `tests/invariant_scan.rs` exists as grep-scan host)

1. **D5 predicted terminal — RULED: pin to the REAL `classify_check_result` arms.** The parent §3/dry-run shorthand ("Superseded→all ERROR_RETRYABLE; NotFound-hold→SENT") is **superseded** — it inherited the dry-run's stale wording; the verified arms are `SupersededHold` (doc held in SENT) / `SentNotFoundDowngrade` / `StructuralDrift`. Cohort semantics honor the strict-sequential STOP (`backlog_drain.rs:928-946`) and the RMR halt (`:725`). The precise model-level `Mutated` shape per script is derived during RED from these arms + the drain loop (and reviewed migration-grade) — the RULING fixes the source of truth (real arms), not a guessed table.
2. **D4 depth — RULED: minimum now** (bounded-dispatch / no-resend postcond per the `IN (0,1)` budget + W10.4 one-shot flag). Terminal-prediction: the implementer MAY attempt it once during GREEN if it falls out deterministic; if not, stay bounded — no CP needed, note the outcome in Delivery.
3. **D2 op-coverage — RULED: predict-then-assert for every op whose `apply()` already models mode/shift deterministically** — `GoOnline` (full-drain→Online / halted→GoingOnline / reject→RMR), `Drain`/`RepeatDrain` (incl. reject→RMR), `GoOnlineWithoutBacklog`, `OfflineSellDuringGoingOnline` (→GoingOnline), `SellWithClosedShift` (→Closed), plain sells (assert **no-change**). Fault-class ops (`Crash*`/`Reboot`/`RepeatReboot`) → `adopt_fault_deferred` (classified). Mid-transition residue the model deliberately does not resolve → `adopt_precondition` (classified). Nothing else adopts.
4. **Funnel — RULED:** the three names confirmed as drafted (`read_seed_fixture` / `adopt_fault_deferred` / `adopt_precondition`); registry = call-site classification comments + the scan's allowlist; the static-scan lands in the **existing `rust/prro/tests/invariant_scan.rs`** (verified grep-scan host) — no new file.
5. **RED-pin authorship — RULED (per the established dual-session pattern, P1→Phase-3):** the LOCKED §3 pin-targets ARE the normative RED content — they state exactly what each pin asserts; the implementer authors the test code **test-first**, and each closure's Delivery must include the RED evidence (the failing output before the derivation). The architect reviews migration-grade. This satisfies "контракты ведут с RED-пина" without serializing on architect-authored test code.

---

## References

- Parent: `docs/superpowers/specs/2026-07-02-webcheck-ground-truth-phase1-design.md` (§3/U1, §5/A1, LOCKED v2.1).
- U0 ground truth: `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md` (sole WebCheck citation source; A07-ruling).
- Our pins: `migrations/001_baseline.sql`, `migrations/025_fiscal_documents_aborted_state.sql`, `src/db/models/enums.rs`, `src/db/repositories/node_state.rs`, `src/db/repositories/fiscal_documents.rs`, `src/services/write_path/stage_send.rs`, `src/services/offline_sync/backlog_drain.rs`, `src/services/offline_sync/kvt2_confirm.rs`, ADR-M3-A1 (`2026-05-04-m2-pre-plan-adr.md`), M3b (`2026-05-17-m3b-shift-state-expansion.md`).
- Harness: `rust/prro/tests/invariant_fuzzer.rs` (+ `invariant_fuzzer/{model,oracle,interp,op,strategy}.rs`, `common/scripted_dps.rs`, `TEETH_TEST.md`), `tests/invariant_scan.rs`.
