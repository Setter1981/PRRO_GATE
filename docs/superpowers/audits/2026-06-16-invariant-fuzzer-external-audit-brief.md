# External Audit Brief — Invariant Fuzzer (Phase 0 + §15 Hardening)

**Date:** 2026-06-16
**Target:** model-based stateful invariant fuzzer for the Multi-Protocol PRRO Gateway
**Status of target:** Phase-0 complete (T0–T7, 8 PRs merged) + §15 post-T7 hardening complete (Cluster A #193, Cluster B #194). The fuzzer is "CI-grade" by our own bar; this audit tests whether that bar is honest.
**Auditor profile:** an adversarial correctness/security reviewer (strong LLM critic or human expert). You are not here to praise. You are here to find where this fuzzer would let a real fiscal bug reach production.

---

## 0. How to read this brief — the one sentence that matters

> **The single most valuable thing you can produce is a concrete sequence of fiscal operations that violates a legal invariant but which this fuzzer reports as CLEAN** — a false negative, i.e. *missing teeth*.

Second most valuable: a place where the hand-built reference model has drifted from real system semantics such that the differential oracle is comparing the system against fiction (it "passes" because it tests nothing). Everything else — false positives, design smells, coverage gaps — is welcome but ranks below these two.

We have deliberately included a **known-weaknesses** section (§6). Do not spend effort re-discovering those; spend it on holes we have *not* already named. If you think a known gap is more severe than we rate it, say so explicitly.

---

## 1. System context — why correctness is load-bearing

The PRRO gateway is a **local edge fiscal system for Ukraine** with operational and legal risk. It issues fiscal receipts, signs them, sends them to DPS (the tax authority), and must behave correctly across **offline mode** (with hard time- and code-count limits), **crash/recovery**, and **reconciliation**. A silent state-machine violation here is not a cosmetic bug — it is a deregistered fiscal number, a duplicated or lost receipt, an un-drained offline backlog, or a chain-integrity break, any of which has legal/financial consequences.

**The dominant bug class is empirical, not theoretical.** Across this project's audit history, *every* shipped defect (M2-01, M2-N1, AUD-K8-1, AUD-L5-1, EDIT-E, and the fuzzer's own first catch #192) was **sequencing / cross-fix-invalidation / recovery-under-fault**. **Zero were happy-path.** Happy path is cheap and already proven via interop against the real DPS. The risk lives in the *unhappy combinatorics*: {operations × crashes × DPS responses × offline transitions × timing}. That space is millions of branches and is uncoverable by hand. The fuzzer exists to explore it and shrink failures to minimal repros.

So when you audit this tool, the question is not "does it test the system" — it is **"does it actually constrain the unhappy combinatorics, or does it have escape hatches that let the dangerous corners pass as clean?"**

---

## 2. What the fuzzer is — architecture

Model-based stateful invariant fuzzer, TigerBeetle generative-testing pattern applied to a fiscal gateway. Pipeline:

```
generator (shrink-first intent-stream of Ops, len 1..=8)
  → interpreter (drives the REAL write-path seams against a live SQLite DB)
  → 4-layer oracle (after the op-loop and at quiescent boundaries)
  → crash injection (drop-injection at wire points → reboot → boot reconciliation)
  → on failure: proptest shrinks to a minimal op-sequence repro
```

It drives **real production seams**, not a reimplementation: the inline write-path op, the backlog drain, boot reconciliation, and the go-online probe. The DPS counterparty is a scripted stub (`ScriptedDps`) that can be programmed with ack / reject / timeout / hang / superseded / bad-hash-prev style responses.

### The 4 oracle layers (this is the surface you are auditing)

- **L1 — Differential.** Real outcome (observed from the DB) vs the **hand-built reference model**'s prediction. Structural, not byte-exact (it compares lnd advancement, chain linkage, state, seed/code movement — not raw hashes, which are synthetic in test).
- **L2 — Invariant scan (`assert_clean`).** A global SQL-level scan at **quiescent boundaries**: chain integrity, mirror consistency, ledger-only pin, and (post-#192) no stuck non-terminal docs. Runs only in **SETTLED** mode (see §5.A — this gate is a prime audit target).
- **L3 — Fault bounded-postcond + resync.** Under an injected crash, asserts *bounded* properties (e.g. no-resend, ERROR_RETRYABLE recovery) rather than exact state, then **resyncs the model from the DB**. Each resync is a place the model stops predicting and starts trusting (see §5.B).
- **L4 — Mirror-2.** An exact predicate over `offline_session ↔ drain_cohort` consistency, layered above the coarser scan check.

### The crash model

Cancellation/drop-injection at **wire points** (the K3/K4 kill points): the interpreter holds a wire call open, drops the future, and reboots, then runs boot reconciliation. The model mirrors this as a fault that re-syncs after reboot.

---

## 3. What "clean" means — the invariants under test

The authoritative invariant set is `docs/LEGAL_INVARIANTS.md` (INV-01 … INV-20). The fuzzer's `invariant_scan` enforces a *subset* at the SQL level. The ones most relevant to you:

- **Chain integrity** — `prev_hash` linkage across the issued-receipt sequence is unbroken and correctly ordered. **Caveat (honest):** the fuzzer's chain check is *referential* — it asserts `previous_hash == prior doc's stored hash`; it does **not** recompute `sha256(canonical_xml)`, so a corrupt-but-self-consistently-threaded hash would pass. "Chain linkage" here means referential linkage, not cryptographic.
- **Ledger-only pin (M3b)** — `fiscal_documents` is a **ledger of ISSUED receipts only**. Failed DPS rejections and invalid ingress payloads go to `audit_log` only, **never** to `fiscal_documents`. (This is the invariant #192 violated — an orphaned SIGNED row is a non-issued doc sitting in the ledger.)
- **Single-writer** — one `fiscal_number` = one logical write-path; no concurrent mutation.
- **Mirror consistency** — offline session state and the drain cohort agree (no foreign/stale/null session pointers on cohort docs).
- **No stuck non-terminal docs** — at a settled boundary, no doc may sit forever in `{PREPARED, SIGNED, ENCRYPTED}`.

A key structural fact you should test against: **DocState terminals** are `Ack`, `Rejected`, `Cancelled`, `RequiresManualReconciliation`, and (new, from #192) `Aborted`. RMR ("ЧП из ЧП") is a *legitimate durable terminal*, not a liveness violation — the fuzzer treats `settled ⟺ mode∈{Online,Offline} OR shift==RMR`. Challenge this definition if you can.

---

## 4. The teeth — evidence the oracle is not vacuous

A passing fuzzer proves nothing unless it can be shown to *fail on a real bug*. Two artifacts establish teeth:

1. **The AUD-K8-1 teeth test** (durable, in `TEETH_TEST.md`): revert the real guard at `backlog_drain.rs:725` and the fuzzer **finds and shrinks** to a 4-op repro `[OnlineSell([Ack,Ack]), Crash(Send), GoOnline([BadHashPrev]), GoOnline([Ack,Ack])]`, plus a point `#[ignore]` canary fails (`send_calls` 1→2). Restore the guard → 36 green. This proves the differential + the wire-call postcond are load-bearing.
2. **The first real bug, #192** (full ROI loop): the fuzzer found a no-code / post-sign-refusal path that left an **orphaned SIGNED doc** in the ledger (ledger-only-pin violation) which boot reconciliation would then **resurrect** (CRITICAL escalation potential). Triaged → designed → fixed (new `Aborted` terminal + migration + boot-bail + a `StuckNonTerminalDoc` scan check) → guarded with a model-mirror. This is the proof the tool pays for itself.

**Your job in §5 is to find the bug class these two artifacts do NOT cover.**

---

## 5. The audit mandate — attack surfaces, ranked

### A. Oracle soundness / false negatives — HIGHEST VALUE

- **The all-pass hole.** Construct a state where `assert_clean` (L2) **and** `check_differential` (L1) **and** `check_mirrors` (L4) all return OK while a real legal invariant (§3) is violated. Any single such construction is the most valuable finding in this audit.
- **The SETTLED-mode scan-gate.** The scan runs only when `mode∈{Online,Offline} OR shift==RMR`. We *claim* that `GoingOnline` and mid-crash are two classes of legitimate in-flight transient (boot defers GoingOnline-FN docs into a later drain), so scanning them would false-positive. **Attack this claim:** find an invariant violation that lives *only* inside the suppressed window — i.e. a corruption that is real at the GoingOnline / mid-crash instant and is *masked* (resolved, hidden, or overwritten) before the next settled boundary, so the post-settle scan sees clean. If the gate can hide a real, non-transient defect, that is a false negative by construction.
- **Permissive differential arms.** The oracle's `OpClass` has three values: `PredictableMutating` (differential-matched), `ExpectedNoMutation` (assert no fiscal issuance), `FaultOrRecovery` (bounded postcond + resync). `check_differential` for the `ExpectedNoMutation` class is intentionally permissive. Cluster B (#194) refined that class to distinguish a **true zero-row no-op** from a **non-issued-row** case — an online-reject `REJECTED` row, or an offline-no-code `Aborted` row, where the `lnd` is consumed but **no fiscal issuance occurs and the chain seed does not advance**. This distinction lives in the harness assertion arms, not in a separate enum variant. **Is the distinction complete** — can a *wrong* mutation (a row that should not exist, or a `seed`/`code` that moved when it shouldn't) still slip through the permissive `ExpectedNoMutation` shape?

### B. Model fidelity / drift — DEEPEST RISK

The reference model is **hand-built**. We have already had **6 reconciliations** in T7 where the model *adopted* real semantics rather than *predicting* them (e.g. `resync_from_db`, `resync_preconditions_from_db`, the cumulative `code_consumed` observable, drain mid-wire guard, shift drain-eligibility, BadHashPrev MAC-recovery). **Every resync/adopt point is a place the model stops being an independent oracle and starts trusting the system under test.**

- **Enumerate the resync points** and for each, argue whether it could hide a bug: when the model resyncs from the DB the system just wrote, the differential at that point becomes the system agreeing with itself — *vacuous*. Find the resync that matters most.
- **The vacuity test.** Does the model *ever* compute a "prediction" by reading the same DB rows the system produced (directly or transitively)? If so, that comparison tests nothing. Point to it.
- **Semantic invention.** We assert we did not *invent* semantics during reconciliation, only mirror real seam behaviour. Find a model arm where the predicted outcome is a *guess* about correct behaviour rather than a derivation from spec/invariants — because if the guess is wrong, the fuzzer enforces a wrong rule (false positive) or, worse, blesses a wrong behaviour (false negative).

### C. Generator coverage / reachability

- Alphabet is **Phase-0 only**: online sell, offline sell, go-online (probe+drain), drain, crash@{wire points}, reboot, and 6 invalid-input ops. **No RETURN, no Z_REPORT, no online SHIFT_OPEN, no clock/cert-expiry.** Sequences are length **1..=8**, generated **shrink-first** (a plain collection strategy, *not* `prop_filter`), at **N=256** per harness, from **two fixtures** (online and offline).
- **Reachability:** does the generator actually reach the dangerous corners (multi-doc drain re-entry, interleaved crash+drain+go-online), or do the seeds/preconditions keep it in a shallow subspace? Is len≤8 × N=256 enough to hit the *interleaving* bugs, or only the short ones?
- **Fixture blind spot:** is there a state reachable in production that *neither* the online nor the offline fixture can seed into — and that therefore is never explored?

### D. The crash model

- Drop-injection is at **wire points only**. Are there crash points *outside* wire calls that carry real risk and that this model cannot inject — e.g. mid-SQLite-transaction, in the window between a commit and its audit-log write, during a migration, or between local-commit and drain threshold? If such a point can leave a durable inconsistency, the fuzzer is blind to it.
- **Post-reboot completeness:** after reboot the model resyncs and L3 asserts *bounded* postconds. Is the *full* invariant set actually checked post-reboot, or only the bounded ones? A bounded postcond that holds while a global invariant is broken is a false negative.

### E. The #192 fix — correct and durable?

- **Transition completeness.** `Aborted` is reachable from `{Prepared, Signed}`. Can a doc reach `SIGNED` and then take some *other* path that re-creates an orphan (a non-terminal doc that the ledger-only pin forbids)? Enumerate every outgoing edge of `SIGNED` and `PREPARED` and confirm none re-orphans.
- **Resurrection guard breadth.** The boot fix excludes `Aborted` from the pending set in `dispatch_pending_doc`. Does **every** query path that feeds boot reconciliation exclude `Aborted`, or only the one patched? A second pending-selection query that still picks up `Aborted` re-opens the resurrection.
- **Scan unconditionality.** Is the `StuckNonTerminalDoc` check unconditional — can no production code path disable or skip it? (We believe there are zero prod callers of `scan()`/`assert_clean` that could gate it off; verify.)

### F. Determinism / reproducibility escape hatches

- Replay relies on `synchronous=FULL` + test seams. Identify **any** source of nondeterminism that could make a found bug fail to reproduce: wall-clock reads, hash-map iteration order, the per-case temp-DB leak (`std::mem::forget`), or concurrency in the drain. A bug the fuzzer can find but not deterministically *re-find* is nearly worthless as a regression.

---

## 6. Known weaknesses (already on our backlog — do NOT re-report as new)

We disclose these so you focus on *unknown* holes. If you think any of these is **more severe than rated**, say so — that itself is a finding.

1. **temp-DB leak** — the interpreter does `std::mem::forget` on per-case temp databases (a T2 shortcut), so 256-harness runs accumulate temp DBs. Tier-1 fix pending. Current mitigation: `TMPDIR` on disk. Already caused one transient CI link-time "No space left on device" on the gnu target.
2. **No CI seed-corpus persistence yet** — found failures are not yet auto-persisted as permanent proptest regression seeds, so a fixed bug is not yet a permanent regression gate. Tier-1.
3. **Partial multi-doc drain coverage** — some oracle paths historically reasoned over a single drain doc; Cluster B (#194, B1/M1) moved cohort loading to the *real* cohort (SENT/KVT1/ERROR_RETRYABLE/KVT2), but interleaved multi-doc drain-reentry depth is still shallow.
4. **Alphabet is Phase-0 only** — no RETURN/Z_REPORT/online-SHIFT_OPEN/clock. Phase-2 work.
5. **Hand-built model rot** — the reason Phase-1 (a WebCheck-derived reference oracle from the legacy system + production DBs) is planned: it would replace the hand-built model's drift risk with a self-validating ground truth. Until then, §5.B is the live risk.

---

## 7. What to return — deliverable format

For each finding:

- **Class** — one of: `FALSE-NEGATIVE` (fuzzer would miss a real bug) · `FALSE-POSITIVE` (fuzzer false-alarms on correct behaviour) · `MODEL-DRIFT` (differential tests fiction / is vacuous) · `COVERAGE-GAP` (generator cannot reach the state) · `DESIGN-SMELL`.
- **Severity** — the *production* consequence of the missed/mis-handled case (legal/financial), not the code aesthetics. A masked chain-break outranks a redundant assertion.
- **Evidence** — a concrete op-sequence that demonstrates it, OR a specific `file:line` + a tight argument. Speculation with neither a repro nor a code pointer is low-value; mark it as such if that's all you have.
- **Suggested minimal fix** — in the project's style (minimal diff, wire-the-seam, explicit code in hot paths).

Then:

- **Rank** findings by false-negative severity first.
- **State your confidence** per finding and **what you could not verify** without executing the code (you are reading, not running — be explicit about which claims are static-analysis inferences vs. things you'd need a run to confirm).

---

## 8. Curated reading order — file manifest

Paths are relative to the repo root (`/home/setter/prro_gate`). Line counts are as of 2026-06-16. Read in this order.

**The design intent (read first):**
- `docs/superpowers/specs/2026-06-15-invariant-fuzzer-design.md` (315 lines) — the authoritative design. **§9 "Known kill-point bounded postconditions"** at L154; **§15 "Phase-0 hardening (post-T7, CI-grade harness oracle)"** at L241–315 (the contract you are auditing the implementation of).
- `docs/LEGAL_INVARIANTS.md` — INV-01 … INV-20, the legal invariant set the scan enforces a subset of.

**The fuzzer (the harness under audit, `rust/prro/tests/`):**
- `invariant_fuzzer/op.rs` (102 lines) — the alphabet: `Op`, `Stage`, `WireResponse`, `DpsScript` builders.
- `invariant_fuzzer/strategy.rs` (55 lines) — the generator. `op_sequence()` at L53: `prop::collection::vec(op(), 1..=8)` — shrink-first, explicitly **no `prop_filter`** (L4 comment).
- `invariant_fuzzer/interp.rs` (1126 lines) — the interpreter; drives the real seams. Drain calls at L587/L611/L643; the guard it exercises is referenced at L386 (`backlog_drain.rs:741`).
- `invariant_fuzzer/model.rs` (508 lines) — the hand-built `RefModel`. `ExpectedOutcome` at L45; `apply` dispatch at L160; the non-issued-row mint logic at L193–257.
- `invariant_fuzzer/oracle.rs` (325 lines) — `OpClass` at L37; `classify` at L54; `check_differential` at L72; `check_ledger_delta` at L159; the L3 bounded-postconds `assert_crash_send_recovery` L188 / `assert_probe_recovery_no_resend` L209 / `assert_no_resend` L236.
- `invariant_fuzzer.rs` (1430 lines) — the proptest capstone: the two seeded harnesses (`harness_online_seeded`, `harness_offline_seeded`, N=256), the SETTLED-gate logic, the model-mirror, and the directed pins. The non-issued-row pins are at L141 and L1102/L1114.
- `common/scripted_dps.rs` (195 lines) — the DPS adversary stub.
- `invariant_fuzzer/TEETH_TEST.md` — the durable teeth artifact (revert `backlog_drain.rs:725` → find+shrink demo).

**The invariant scan (the L2/L4 oracle, `rust/prro/src/db/`):**
- `invariant_scan.rs` — `Violation` enum at L41; `StuckNonTerminalDoc` variant at L60 + its check at L186; `scan` at L145; `assert_clean` at L399.

**The #192 prod-fix surface (the first real bug → `Aborted` terminal):**
- `rust/prro/src/db/models/enums.rs` — `DocState::Aborted`.
- `rust/prro/migrations/025_fiscal_documents_aborted_state.sql` — adds `'ABORTED'` to the `state` CHECK (table rebuild; self-FK handled via `PRAGMA defer_foreign_keys`).
- `rust/prro/src/services/write_path/inline.rs` — `run` at L444; `terminalise_inbox` + the `PostSignRoute::Refused` arm that atomically aborts a dangling `{PREPARED,SIGNED}` doc.
- `rust/prro/src/services/write_path/stage_acquire.rs` — Step 3a mode-guard (rejects GoingOnline/Blocked **before** the PREPARED commit; this is *why* there is no orphan in that path).
- `rust/prro/src/services/write_path/stage_offline_ack.rs` — `RefusalReason::CodePoolExhausted` (the no-code refusal that the fuzzer first tripped).
- `rust/prro/src/services/reconciliation/boot_phase.rs` — `run_boot_reconciliation` at L1582; the `Aborted` terminal-bail in `dispatch_pending_doc` (closes the resurrection).
- `rust/prro/src/db/repositories/fiscal_documents.rs` — `allowed_transition` with `(Prepared,Aborted)`+`(Signed,Aborted)`; the `OFFLINE_ISSUED_STATES` SSOT const.

## 9. Appendix — the real seams and the alphabet (verbatim)

**Seams the interpreter drives** (definition site; signatures verbatim):
- `inline::run` — `rust/prro/src/services/write_path/inline.rs:444` — `pub async fn run(`
- `backlog_drain::drain` — `rust/prro/src/services/offline_sync/backlog_drain.rs:671` — `pub async fn drain<'a>(` — **note:** under `offline_sync`, **not** `reconciliation`. The AUD-K8-1 teeth guard is the RMR early-return at **L725** (`if ns.shift_state == RequiresManualReconciliation { return Ok(DrainSummary::new(.., 0)) }`).
- `run_boot_reconciliation` — `rust/prro/src/services/reconciliation/boot_phase.rs:1582` — `pub async fn run_boot_reconciliation(`
- `return_online_probe::run_tick_for_fn` — `rust/prro/src/services/offline_sync/return_online_probe.rs:222` — `pub async fn run_tick_for_fn(` — **trap:** a *different* `run_tick_for_fn` also exists at `services/reconciliation/online_convergence.rs:106`; the fuzzer drives the `offline_sync` one. Do not conflate them.
- `invariant_scan::scan` — `rust/prro/src/db/invariant_scan.rs:145` — `pub async fn scan(pool: &SqlitePool) -> sqlx::Result<Vec<Violation>>`
- `invariant_scan::assert_clean` — `rust/prro/src/db/invariant_scan.rs:399` — `pub async fn assert_clean(pool: &SqlitePool)`

**`Op` alphabet** (`op.rs`) — 6 core + 6 exotic:
`OnlineSell(DpsScript)`, `GoOnline(DpsScript)`, `OfflineSell`, `Drain(DpsScript)`, `Crash(Stage)`, `Reboot` · `RepeatDrain`, `RepeatReboot`, `DuplicateIdemKey`, `GoOnlineWithoutBacklog`, `OfflineSellDuringGoingOnline`, `SellWithClosedShift`.

**`Stage`** (crash injection points, *as declared*): `Acquire`, `Sign`, `Send`, `Kvt1`, `Kvt2`, `Finalize`, `OfflineAck`, `Drain`. **Caveat (honest):** the generator currently emits **only `Crash(Send)` and `Crash(Kvt1)`** (`strategy.rs:41`); the other six are `unimplemented!()` in the interpreter (`interp.rs:446-449`). So the crash model is effectively **wire-only**, and the `Finalize` (ACK-commit↔audit-write) and `OfflineAck` (local-commit↔drain-threshold — the #192 birth site) windows are **unreachable**. Treat §5.D as a live attack surface, not a covered one.

**`WireResponse`** (DPS adversary alphabet): `Ack`, `Reject`, `Timeout`, `Superseded`, `BadHashPrev`, `NotFound`.

**`ExpectedOutcome`** (`model.rs:45`): `Mutated(Mutation)`, `NoMutation`, `Fault`.

**`OpClass`** (`oracle.rs:37`): `PredictableMutating`, `ExpectedNoMutation`, `FaultOrRecovery`.

**`Violation`** (`invariant_scan.rs:41`) — the scan's vocabulary: `DuplicateLnd`, `StuckSending`, `StuckNonTerminalDoc`, `AckWithoutServerFiscalNo`, `AckWithoutKvt1Raw`, `ChainBreak`, `ChainSeedMismatch`, `RejectedInboxWithAcceptedDoc`, `OfflineCodeHalfConsumed`, `OfflineFiscalNoUnbacked`, `DuplicateOfflineFiscalNo`, `OfflineOriginWithoutSession`, `ShiftStateMirrorDrift`.

> **Reading note for the auditor:** the `Violation` list above is *exactly* the set of corruptions L2 can name. A productive line of attack (§5.A) is to find a real fiscal corruption that maps to **none** of these 13 variants — the scan cannot report what it cannot enumerate.
