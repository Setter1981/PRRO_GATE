# Invariant Fuzzer (Phase 0) — Implementation Plan

> **For the implementer:** this is a **dual-session** plan — the **architect** authored these tasks
> (goal / files / acceptance / TDD structure / interfaces); **you write the code per task with strict
> TDD** (separate RED commit, confirmed-failing for the documented reason → minimal GREEN → refactor),
> exactly as the Batch-C / sweep-remediation PRs. Each task = one PR, DO-NOT-MERGE, architect reviews the
> delta + merges on green CI. The steps below give the interface + the RED→GREEN shape, not finished
> code — that is yours to write.

**Goal:** A model-based stateful invariant fuzzer (`proptest`) that drives random fiscal operation
sequences through the REAL Rust write-path and asserts the fiscal invariants after every step, with
automatic minimal-repro shrinking.

**Architecture:** `proptest` generator → interpreter over real seams (`inline::run`, `drain`,
`run_boot_reconciliation`, `return_online_probe::run_tick_for_fn`, `ScriptedDps`) → hand-built reference
model → three-layer oracle (differential / quiescent-boundary `invariant_scan` / fault bounded-postcond
+ re-sync) + mirror-drift checks. Spec: `docs/superpowers/specs/2026-06-15-invariant-fuzzer-design.md`.

**Tech stack:** Rust, `proptest` (dev-dep, to be added), `sqlx`/SQLite (`synchronous=FULL`), `tokio`,
the existing W2 `ReconcileGuard` test seam + kill-point cancellation-injection.

**Gate (every task, pin 1.95.0, `~/.cargo/bin/cargo`):** `cargo nextest run -p prro --features
test-support` + `cargo fmt -p prro -p prro_crypto -p prro_escpos -- --check` + `cargo clippy -p prro
--all-targets --no-deps --features test-support -- -D warnings`.

**Base:** fresh `origin/main` (`23707ae` at authoring; re-fetch + re-verify anchors per task).

---

### Task 0: Prerequisite — extract `ScriptedDps` + add `proptest` dev-dep

**Goal:** A single reusable DPS stub in test-support (de-duping the two `KpStub` copies) + `proptest`
available; no behaviour change.

**Files:**
- Create: `rust/prro/tests/common/scripted_dps.rs` (declare `mod scripted_dps;` in `tests/common/mod.rs`).
- Modify: `rust/prro/Cargo.toml` (add `proptest` under `[dev-dependencies]`).
- Modify: `rust/prro/tests/kill_point_matrix.rs`, `rust/prro/tests/online_convergence_tick.rs` (replace
  the local `KpStub` with `common::scripted_dps::ScriptedDps`).

**Interface (`ScriptedDps`):** implements `transports::dps::DpsChannel`; a per-method response queue
(`push_send` / `push_last` or a unified `push(call_kind, result)`), a **call log** (ordered record of
calls + envelopes — the "envelope spy"), a **hang-on-call** hook (oneshot, for cancellation-injection),
and a **deterministic unexpected-call error** (empty queue → a typed error, never a `pop().unwrap()`
panic, so an over-send surfaces as a clean assertion not a flaky panic). Preserve the exact behaviour the
two `KpStub`s have today (the K-tests + convergence tests are the regression net).

**Acceptance:**
- [ ] `ScriptedDps` lives in `tests/common/`; both old `KpStub` definitions deleted.
- [ ] `kill_point_matrix.rs` + `online_convergence_tick.rs` use it; all their tests pass unchanged.
- [ ] `proptest` in `[dev-dependencies]`.

**Verify:** `cargo nextest run -p prro --features test-support` → same pass count as pre-change (regression net), fmt+clippy clean.

**TDD note:** this is a refactor — the EXISTING K/convergence tests ARE the RED/GREEN net (they must stay
green across the swap). Commit the `ScriptedDps` extract + both call-site swaps together; if any K-test
changes behaviour, the extract diverged — fix, don't adjust the test.

---

### Task 1: Operation alphabet (`Op`) + reference-model skeleton

**Goal:** The `Op` enum (the §5 alphabet) + a `RefModel` that deterministically predicts the expected
ledger, reusing the gateway's issued-set predicate.

**Files:**
- Create: `rust/prro/tests/invariant_fuzzer.rs` (entry; `mod op; mod model;` + later modules).
- Create: `rust/prro/tests/invariant_fuzzer/op.rs`, `rust/prro/tests/invariant_fuzzer/model.rs`.

**Interface:**
- `enum Op` — valid: `OnlineSell`, `GoOnline`, `OfflineSell`, `Drain`, `Crash(Stage)`, `Reboot`; a
  `DpsResp` axis (Ack/Reject/Timeout/Superseded/BadHashPrev/NotFound) carried on the ops that hit the
  wire; invalid/re-entry: `RepeatDrain`, `RepeatReboot`, `DuplicateIdemKey`, `GoOnlineWithoutBacklog`,
  `OfflineSellDuringGoingOnline`, `SellWithClosedShift`. (NO `GoOffline` op — offline is fixture-seeded,
  per spec §5.)
- `struct RefModel { seed: Option<[u8;32]>, next_lnd: i64, shift_state: ShiftState, mode: NodeMode,
  session: Option<SessionState>, codes_issued/consumed, docs: BTreeMap<lnd, DocState> }` with
  `apply(&mut self, op) -> ExpectedOutcome`. The **issued** predicate MUST call/mirror
  `fiscal_documents::OFFLINE_ISSUED_STATES` (spec §6) — do NOT hand-roll a second issued-set.
- Lane-correct seed advance in the model: online-origin advances at ACK; offline-origin at
  `OFFLINE_LOCAL_ACK` and stays issued through drain states.

**Acceptance:**
- [ ] `Op` covers the §5 alphabet (valid + invalid/re-entry).
- [ ] `RefModel::apply(OnlineSell→ACK)` advances `next_lnd` by 1 and sets `seed` to that doc's unsigned hash; `apply(OfflineSell)` advances seed at `OFFLINE_LOCAL_ACK`.
- [ ] The model's issued-set is `OFFLINE_ISSUED_STATES` (assert by referencing the const, not a literal).

**Verify:** `cargo nextest run -p prro --features test-support -E 'test(invariant_fuzzer)'` → the model unit tests pass.

**TDD:** RED — a unit test asserting `RefModel::apply` produces the expected lnd/seed for SELL/offline (fails: model not built) → GREEN — implement `apply`.

---

### Task 2: Interpreter (`Op` → real seam)

**Goal:** Execute an `Op` sequence against a live SQLite test DB through the REAL seams.

**Files:** Create `rust/prro/tests/invariant_fuzzer/interp.rs`.

**Interface:** `async fn run_op(ctx: &mut FuzzCtx, op: &Op) -> RealOutcome`, mapping:
- `OnlineSell` → `inline::run` (Online node) with a `ScriptedDps` programmed by the op's `DpsResp`.
- `GoOnline` → `return_online_probe::run_tick_for_fn` (Offline→GoingOnline) then `drain`
  (GoingOnline→Online) — the **real transition seam, NOT a setter** (spec §5).
- `OfflineSell` → `inline::run` on an Offline node (offline-ack path).
- `Drain` → `backlog_drain::drain`.
- `Reboot` → `run_boot_reconciliation` (W2 `ReconcileGuard::for_integration_test_only`).
- `Crash(stage)` → the **two mechanisms** (spec §4): **drop-injection** (hang `ScriptedDps` on the
  wire await + drop the future) for `send`/`kvt1`-wire stages; **stage-composition / manual CAS** (run
  stages up to the committed-envelope boundary, stop) for non-wire boundaries — NO timing hooks inside a
  `with_immediate`.
- Fixture setup (NOT ops): pre-seed open shift + Offline mode via fixture seeders (`seed_open_shift`,
  `seed_node_state_offline/online`, `seed_open_offline_session`, `seed_offline_code`) — raw node-mode
  setters are fixture-only.
- Invalid ops → drive the same seam expecting refusal/no-op.

**Acceptance:**
- [ ] A hand-written valid sequence (fixture open-shift → `OnlineSell`(ACK) → ...) runs end-to-end and lands the doc at `ACK`.
- [ ] A `Crash(send)` then `Reboot` runs without the interpreter panicking (drop-injection + boot-recon).

**Verify:** `cargo nextest run … -E 'test(invariant_fuzzer)'` → interpreter unit test on a fixed 3-op sequence passes.

**TDD:** RED — a test driving `[OnlineSell(ACK)]` asserting `RealOutcome` reaches ACK (fails: interp not built) → GREEN.

---

### Task 3: Generator (`proptest` strategy + preconditions + invalid/re-entry mix)

**Goal:** A `proptest` strategy producing `Vec<Op>` that respects preconditions (and deliberately emits
the invalid/re-entry ops), with shrinking.

**Files:** Create `rust/prro/tests/invariant_fuzzer/strategy.rs`.

**Interface:** `fn op_sequence() -> impl Strategy<Value = Vec<Op>>` — a stateful/`prop_flat_map`
generator that tracks a lightweight precondition view (shift open? mode? session? backlog?) so valid ops
are reachable, and injects invalid/re-entry ops at random positions (they are *meant* to be illegal). DPS
responses chosen per wire op.

**Acceptance:**
- [ ] Generated sequences are precondition-coherent for valid ops; invalid ops appear with non-trivial frequency.
- [ ] A `proptest!` smoke test runs N (e.g. 64) generated sequences through the interpreter without a precondition-panic (illegal ops are *handled*, not crashing the harness).
- [ ] Shrinking demonstrably reduces a forced failure to a shorter sequence.

**Verify:** `cargo nextest run … -E 'test(invariant_fuzzer)'` → the strategy smoke proptest passes.

**TDD:** RED — a `proptest!` asserting "every generated valid op has its precondition satisfied" (fails before the precondition-aware generator) → GREEN.

---

### Task 4: Oracle layer 1 — differential (non-fault ops) + invalid-op oracle

**Goal:** After each non-fault op, assert `real == model`; after each invalid op, assert typed-refusal /
no-op with NO fiscal mutation.

**Files:** Create `rust/prro/tests/invariant_fuzzer/oracle.rs` (`fn check_differential(real, model, op)`).

**Interface:** compare lnd advance, the new doc's `previous_hash` vs the prior tip, the seed advance at
the lane-correct moment (§6), code consumption, and the document state. For invalid ops: assert no
`lnd`/seed/code mutation and an illegal-transition did NOT occur (typed refusal or no-op).

**Acceptance:**
- [ ] A clean valid sequence passes the differential.
- [ ] An injected model/real divergence (e.g. a deliberately wrong expected lnd in a test fixture) is caught.
- [ ] An invalid op (`SellWithClosedShift`) asserts no fiscal mutation.

**Verify:** `cargo nextest run … -E 'test(invariant_fuzzer)'` → differential unit tests pass.

**TDD:** RED — a test where the model expects lnd+1 but a stubbed real-outcome returns lnd+2 → `check_differential` must flag (fails before the check exists) → GREEN.

---

### Task 5: Oracle layer 2 (quiescent-boundary scan) + layer 3 (fault bounded-postcond + re-sync)

**Goal:** `invariant_scan::assert_clean` at every quiescent boundary (NOT mid-crash); for crash ops,
assert the bounded kill-point postconditions then re-sync the model from the real DB.

**Files:** Modify `rust/prro/tests/invariant_fuzzer/oracle.rs`.

**Interface:**
- `assert_clean(&pool)` after a *completed* op or after `Reboot`/recovery — never on the mid-crash
  transient (a committed-`SENDING`-wire-in-flight is legal; do not scan there — spec §7.2).
- For known kill-points (spec §9): assert the bounded postcondition (e.g. `Crash(send)` → recovery routes
  to `ERROR_RETRYABLE`, **no second `send_chk`**; SENT-before-confirm → probe path, no resend; offline
  `Crash(offline_ack)` → drains to ACK). Reuse the kill-matrix's existing assertion helpers.
- For novel crash points: `assert_clean` + chain-continuity, then `model.resync_from_db(&pool)`.

**Acceptance:**
- [ ] Scan runs at boundaries only; a `Crash(send)` sequence does NOT scan the in-flight transient.
- [ ] `Crash(send)` → bounded postcondition (`ERROR_RETRYABLE`, no resend) asserted; model re-synced; subsequent ops differential-clean.

**Verify:** `cargo nextest run … -E 'test(invariant_fuzzer)'` → fault-oracle unit tests pass.

**TDD:** RED — a `Crash(send)`+`Reboot` test asserting no second `send_chk` (fails before the bounded-postcond layer) → GREEN.

---

### Task 6: Mirror-drift checks (the 5th class)

**Goal:** After each quiescent boundary, assert the three load-bearing mirrors with the exact predicates.

**Files:** Modify `rust/prro/tests/invariant_fuzzer/oracle.rs` (`fn check_mirrors(&pool)`).

**Interface:** reuse `invariant_scan` where it covers a mirror — Mirror-1 `shifts.state ↔
node_state.shift_state` (the new `ShiftStateMirrorDrift`, #177), Mirror-3 `inbox ↔ ledger` (check-5).
Mirror-2 `offline_session ↔ drain_cohort` — the **exact predicates** (spec §8, audit): every drain-cohort
doc has a non-null `offline_session_id` == the active/draining session; no eligible offline-origin doc is
invisible to the cohort; **an empty active session is LEGAL** (no false-positive). Since `assert_clean`
now includes Mirror-1 and Mirror-3, `check_mirrors` largely = `assert_clean` + the Mirror-2 predicate.

**Acceptance:**
- [ ] A seeded Mirror-2 violation (a cohort-state offline doc with a mismatched/NULL session) is caught.
- [ ] A legal **empty active session** passes (no false-positive).

**Verify:** `cargo nextest run … -E 'test(invariant_fuzzer)'` → mirror-check unit tests pass.

**TDD:** RED — seed an empty active session → `check_mirrors` must NOT fire (fails if the predicate is naive) AND seed a real Mirror-2 desync → must fire → GREEN.

---

### Task 7: Full `proptest!` harness + the teeth-test

**Goal:** The end-to-end fuzzer — generator → interpreter → all oracle layers + mirror checks over N
cases — plus the reproducible teeth-test proving the oracle has teeth.

**Files:** Modify `rust/prro/tests/invariant_fuzzer.rs` (the `proptest!` harness wiring Tasks 1-6).

**Interface:** `proptest!` test: for each generated `Vec<Op>`, fresh DB + fixture (open shift), run each
op via the interpreter, after each step run the oracle (differential for non-fault / bounded-postcond +
re-sync for fault) + `assert_clean` + `check_mirrors` at quiescent boundaries. Config N (PR-time small,
e.g. 256; nightly large is Phase 3).

**Teeth-test (spec §14, hard gate, reproducible):** a documented procedure (a `#[ignore]`-d test or a
README step in `tests/invariant_fuzzer/`): on current `main`, revert the AUD-K8-1 drain-entry RMR guard
(`backlog_drain.rs:725` — the `if ns.shift_state == RequiresManualReconciliation { return Ok(...) }`
block, fix `a171f18`/#168), run the fuzzer, confirm it finds the re-tick re-drive / busy-loop and shrinks
to a minimal `[…, escalate, re-tick]` sequence; restore the guard → fuzzer green. Document the exact
revert + run command + expected finding.

**Acceptance:**
- [ ] The `proptest!` harness runs N cases green on current `main`.
- [ ] The teeth-test procedure (revert AUD-K8-1 guard → fuzzer finds + shrinks the busy-loop) is documented and demonstrated once.
- [ ] A failing seed replays deterministically and yields a minimal repro.

**Verify:** `cargo nextest run -p prro --features test-support -E 'test(invariant_fuzzer)'` → green; the teeth-test (manual revert) demonstrated in the PR description.

**TDD:** the harness composes Tasks 1-6 (already TDD'd); the teeth-test IS the RED proof that the whole pipeline has teeth (it must FIND the reverted-guard bug).

---

## Out of scope (Phase 1+, per spec §11)
RETURN / Z / EVPZ alphabet expansion; model-predicts-recovery (closes §7 residual); WebCheck
byte-differential reference oracle; CI nightly large-N + auto-filed repros. SW-5a operator-force is a
separate plan item.

## Sequencing & dependencies
Task 0 → 1 → 2 → 3; Tasks 4/5/6 depend on 2+3 (can interleave); Task 7 depends on 1-6. One PR per task,
DO-NOT-MERGE, architect reviews + merges on green CI (full matrix). Strict-TDD RED-first per task.
