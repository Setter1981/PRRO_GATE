# Design — Model-Based Invariant Fuzzer (MVP / Phase 0)

**Status:** approved (architect + external audit, 2026-06-15). Externally audited; all audit edits
folded (see §13). Base: `origin/main` `d6c2024`. This is the design spec; the implementation plan
follows separately (writing-plans).

Companion to `docs/TEST_STRATEGY.md` — this is **oracle O3** (state-machine under faults) of the
five-oracle strategy. Phase 0 only.

---

## 1. Problem & bug taxonomy

The system under test is a **local PRRO fiscal gateway** (Rust): a staged write-path
(`PREPARED→SIGNED→SENT→KVT1→KVT2→ACK`), an offline mode (`OFFLINE_LOCAL_ACK` → later drain), boot
reconciliation, and online convergence — coordinated by a per-`fiscal_number` single-writer model over
SQLite (WAL, `synchronous=FULL`). A lost or double-fiscalized receipt is the merchant's tax liability,
so correctness-under-failure is a legal requirement, not polish.

Empirically, **every fiscal-correctness defect found in the M1–M2 audit was a semantic / sequencing /
recovery / cross-fix bug — none on the happy path.** They cluster into **five classes**:

1. **seed** — MAC-chain seed (`node_state.last_known_unsigned_xml_sha256`) advance/read timing.
2. **chain** — `previous_hash` continuity + `ChainSeedMismatch` handling.
3. **drain-reentry** — idempotency of recovery loops (drain / convergence / boot) under re-entry.
4. **shared-fn-caller** — a widened shared predicate breaking a caller that assumed the old behaviour.
5. **projection/mirror drift** — two persisted representations of one fact diverging: `shifts ↔
   node_state.shift_state`, `offline_session ↔ drain_cohort`, `inbox ↔ ledger`. (Audit-added; seen as
   SEAM-D-1, and the SW-3 drain busy-loop was triggered by a non-escalatable mirror.)

The happy path is one deterministic sequence; the risk is the **unhappy combinatorics** — {operations}
× {crash points} × {DPS responses} × {offline transitions} × {timing} — unreachable by hand-written
tests. This component explores that space and asserts the fiscal invariants after every step.

## 2. Approach

**Model-based stateful property testing.** A generator emits random op sequences; an interpreter drives
each through the **real** gateway code; a hand-built **reference model** predicts the expected ledger; a
layered oracle asserts correctness; `proptest` **shrinks** any failure to a minimal repro. Per-PR review
and hand-written tests *sample* the bug space; a generative engine *explores* it.

## 3. Scope (Phase 0)

> **Pre-seeded open-shift state-machine fuzzer for SELL / offline / drain / recovery, with
> invalid/re-entry ops and kill-point bounded expectations.**

**In scope:** an already-open shift (pre-seeded fixture); SELL via `inline::run`; offline transitions +
offline SELL + drain; crash-injection + reboot/recovery; a controllable DPS adversary; valid AND
invalid/re-entry/replay ops; the reference model + the three-layer oracle; mirror-drift checks.

**Explicit non-goals (Phase 0):** SHIFT_OPEN / SHIFT_CLOSE / Z_REPORT as fuzzer ops — `inline::run` is
**SELL/RETURN-only and fail-closes SHIFT_OPEN + Z-class before acquire** (`inline.rs:392`), so they are
not a live seam; the shift is pre-seeded as a fixture, never opened via direct SQL in the interpreter.
national-cashback and the second (EVPZ) egress channel — not yet built in the gateway. **RETURN is
*live*** in the inline path (`inline.rs:1` — "SELL/RETURN only"; green ACK test
`write_path_inline.rs:1029` `online_return_reaches_ack`) but is excluded from Phase 0 for MVP narrowness;
it is high-value to add **early in Phase 2** (RT-3: zero RETURN goldens today). A model that *predicts*
arbitrary recovery (Phase 2). A byte-differential vs. the predecessor product (Phase 1b). CI integration
(Phase 3).

## 4. Architecture (five independently-testable units)

1. **Operation generator** (`proptest`). Emits `Vec<Op>` mixing valid and invalid/re-entry ops (§5).
   Owns shrinking.
2. **Interpreter / driver.** Maps each `Op` to a real seam against a live test SQLite DB. Reuses
   `inline::run`, `backlog_drain::drain`, `run_boot_reconciliation` (W2 `ReconcileGuard` test seam), the
   **return-online probe** (`return_online_probe::run_tick_for_fn`), and the DPS adversary. **Raw
   node-mode setters are fixture / state-construction ONLY** (e.g. pre-seeding `Offline`); a transition OP
   MUST drive its real seam, never a setter (see §5) — a setter bypasses the seam's idempotent-no-op,
   audit, and refusal behaviour, which is where the bugs live.
3. **`ScriptedDps`** (extracted from today's two `KpStub` copies — see §10). A reusable `DpsChannel`
   stub: response queue, call log, envelope spy, hang-on-call (for crash injection), deterministic
   unexpected-call error.
4. **Reference model.** In-memory predictor of the expected ledger (§6).
5. **Oracle.** The three-layer assertion engine (§7) + mirror-drift checks (§8).

Determinism (corrected 2026-06-17): replay comes from the persisted `proptest` seed + the single-threaded current-thread runtime — **not** from `synchronous=FULL`, which is a SQLite durability/fsync PRAGMA unrelated to RNG or generation order. (Persistence is on by default — `SourceParallel("proptest-regressions")` — but today the seed file is uncommitted and `RngSeed::Random` is used, so a find only reproduces via that file; see the Phase-2 durability spec.) Crash injection uses the **two mechanisms the kill-point matrix already
uses** — implementers must NOT invent timing hooks inside a DB transaction: (a) **drop-injection** for a
crash at a **wire await** (the DPS stub hangs on the send / lastChk call and the test drops the in-flight
future — "committed survives, in-flight rolls back"); (b) **stage-composition / manual CAS** for a crash
at a **committed-envelope boundary** (no future to drop mid-`with_immediate`; the test runs the stages up
to the boundary, stops, then reboots). Fixed `proptest` seed ⇒ deterministic replay; shrinking ⇒ minimal
repro.

## 5. Operation alphabet

**Valid:** `online_sell` (`inline::run`, Online node) · `go_online` (**the one real transition op**:
drives `return_online_probe::run_tick_for_fn` for `Offline→GoingOnline`, then `drain` for
`GoingOnline→Online` — exercising the probe's idempotent-no-op / audit / mode-refusal logic) ·
`offline_sell` (consumes an offline code) · `drain` · `crash@{acquire, sign, send, kvt1, kvt2, finalize,
offline_ack, drain}` → `reboot` · per-wire-call DPS response (ack / reject / timeout / superseded-tip /
`ERROR_BAD_HASH_PREV` /
not-found).

**No `go_offline` op (audit).** There is no live, callable `Online→GoingOffline/Offline` transition seam
today (auto-offline lives in comments/specs, not a service entry point), so a `go_offline` op would
violate this design's own rule ("a transition op MUST drive its real seam, never a setter"). **Phase 0
therefore enters the offline lane by fixture / state-construction** (pre-seed `Offline`); `go_online` is
the one real transition op. The organic auto-offline path (`online_sell` × DPS-failure → fallback) is
deferred to Phase 2, contingent on a test-drivable seam.

**Invalid / re-entry / replay (load-bearing for the drain-reentry + shared-fn-caller classes):**
`repeat_drain` · `repeat_reboot` · `duplicate_idempotency_key` (replay) · `go_online_without_backlog` ·
`offline_sell_during_GoingOnline` · `sell_with_closed_shift`. **Oracle for these:** a **typed refusal OR
a no-op, with NO fiscal mutation** (no `lnd` advance, no seed advance, no code consumption, no
state-machine illegal transition). This is what exercises the guard / idempotency / shared-predicate
paths that the M2-N1 / AUD-K8-1 / SW-1..3 bugs lived in.

## 6. Reference model & seed semantics

Per `fiscal_number`: `seed` (MAC tip) · `next_lnd` · `shift_state` · node `mode` · active offline
session + code set (issued / consumed) · per-`lnd` document state. Each non-fault valid op mutates this
deterministically.

**Seed semantics — precise, lane-specific (audit edit 3):**
- **online-origin** doc advances the seed at **ACK** (finalize).
- **offline-origin** doc advances the seed at **`OFFLINE_LOCAL_ACK`** (issuance) and **remains *issued*
  through all drain states, including rejected / manual outcomes.**
- The model's "issued" predicate **MUST reuse `fiscal_documents::OFFLINE_ISSUED_STATES`** — the
  single-source-of-truth const already shared with `invariant_scan` (`invariant_scan.rs:221-226`). The
  model must NOT derive its own issued-set, or model and scan disagree and produce false findings (this
  is itself the shared-fn-caller lesson applied to the test harness).

## 7. Oracle — three layers (with the quiescent-boundary rule)

1. **Differential (non-fault ops).** Assert `real == model`: `lnd` advanced by one; the new doc's
   `previous_hash` equals the prior tip; the seed advanced once at the lane-correct moment (§6); the
   consumed offline code matches; the document reached the expected state.
2. **Invariant scan — at quiescent boundaries only** (audit edit 6). Run `invariant_scan::assert_clean`
   **after a completed op or after reboot/recovery — NOT mid-crash.** `assert_clean` forbids a resting
   `SENDING`, but a `SENDING`-committed-wire-in-flight state is a *legitimate transient* between
   crash-injection and reboot; scanning there would falsely flag it. A crash op therefore bundles
   reboot/recovery before its scan (or, if an intermediate scan is wanted, a filtered scan that tolerates
   the expected transient).
3. **Fault postconditions (crash / reboot ops) — bounded, not bare re-sync** (audit edit 2). For the
   **known kill-points**, assert the bounded expectations the kill-point matrix already pins (§9). For
   **novel** crash points, run the scan + a chain-continuity check, then **re-synchronize the model from
   the real DB** (accept the real recovery as ground truth, without asserting a model-predicted state).

## 8. Projection / mirror-drift checks (the 5th class)

After every quiescent boundary, assert the load-bearing mirrors are consistent:
- `shifts.state` ↔ `node_state.shift_state` (the m3b §5 mirror, owned by `apply_shift_transition`).
- `offline_session` ↔ `drain_cohort` — **exact predicates** (an active/open session with an *empty*
  cohort is LEGAL — do NOT false-positive on it): (a) every drain-cohort doc has a non-null
  `offline_session_id` equal to the selected active/draining session; (b) no *eligible* offline-origin doc
  is invisible to the cohort; (c) an empty active session is allowed. (`list_drain_candidates_for_fn_ordered_by_lnd`;
  `invariant_scan` check-6d `OfflineOriginWithoutSession` partially covers this.)
- `ingress_inbox` status ↔ `fiscal_documents` ledger (the replay-resolver consistency, `replay.rs`).

Where `invariant_scan` already covers a mirror, reuse it; where it does not, the fuzzer adds the
assertion (and a follow-up may promote it into `invariant_scan` so every test gets it for free).

## 9. Known kill-point bounded postconditions

For the crash points the kill-point matrix (`kill_point_matrix.rs`, K1–K9) already specifies, the fuzzer
asserts the exact bounded outcome rather than re-syncing blindly. Examples:
- `crash@send` (SENDING committed, wire in flight) → recovery routes to `ERROR_RETRYABLE` with **no
  second `send_chk`** (no blind resend).
- SENT before confirm (kill-matrix K4) → recovery takes the **probe path, no resend**; exactly one
  `send_chk` total across the restart.
- offline `crash@offline_ack` then drain → the doc drains to `ACK`.
The implementation reuses the kill-matrix's existing assertions; the fuzzer's contribution is exercising
them inside random *sequences*, not just the isolated K-tests.

## 10. Reused vs. new (build scope)

**Reused (~60% scaffolded):** `invariant_scan` (`invariant_scan.rs`); the cancellation-injection crash
model and kill-point assertions (`kill_point_matrix.rs`); the reused seams (`synchronous=FULL` [durability PRAGMA, not a determinism seam],
`inline::run`, `backlog_drain::drain`, `run_boot_reconciliation`, `return_online_probe::run_tick_for_fn`,
**fixture** node-mode setters); the `OFFLINE_ISSUED_STATES` SSOT predicate.

**New:** the operation generator (alphabet + `proptest` strategy + preconditions + invalid/re-entry ops);
the reference model; the differential oracle layer; the bounded kill-point assertion reuse inside
sequences; the mirror-drift checks.

**Prerequisite refactor:** extract `ScriptedDps` into shared `test-support` from the **two** existing
`KpStub` copies (`kill_point_matrix.rs`, `online_convergence_tick.rs`) — the copies are themselves the
shared-fixture-drift this whole effort fights. Add `proptest` to `prro` dev-dependencies (currently
absent).

## 11. Phasing

- **Phase 0 (this spec):** the scope of §3, run locally.
- **Phase 1+ (out of scope):** model-predicts-recovery (closes §7's residual); RETURN / Z / EVPZ-channel
  alphabet expansion (when built); byte-differential vs. the predecessor product as reference oracle; CI
  integration (PR-time small-N + nightly large-N + auto-filed minimal repros).

## 12. Honest limitations (for reviewers)

- **Stub DPS, not interop.** Proves the gateway's *state machine* against an in-process DPS model; does
  **not** prove the wire output is accepted by the real tax-authority server (separate oracle: live test
  server + predecessor differential).
- **Oracle ≤ invariants.** Catches only what a coded invariant or the model predicts; a fiscal rule never
  encoded is invisible. Strong but finite.
- **Re-sync residual (reduced, not eliminated).** For *novel* crash points the model accepts real
  recovery as ground truth, so a recovery that is scan-clean-but-wrong escapes. Bounded kill-point
  assertions (§9) close this for the known crash points; the full closure is Phase 2.
- **Detection + reproduction, not diagnosis.** Finds and minimizes failing sequences automatically;
  root-causing and fixing remain manual.
- **Feature coverage = built features.** Cannot test SHIFT_OPEN / Z / EVPZ until they are live seams.
  (RETURN *is* live — see §3 — and is excluded only for Phase-0 narrowness.)

## 13. Audit trail

Externally audited 2026-06-15. All edits folded: (1) generator includes invalid/re-entry/replay ops with
a no-fiscal-mutation oracle; (2) fault oracle adds bounded kill-point postconditions, not bare re-sync;
(3) precise lane-specific seed semantics reusing `OFFLINE_ISSUED_STATES`; (4) MVP alphabet re-scoped to a
pre-seeded open shift (no SHIFT_OPEN/Z via `inline::run`); (5) `ScriptedDps` extracted to shared
test-support; (6) `invariant_scan` only at quiescent boundaries; (+) fifth class — projection/mirror
drift — added.

**Second audit (spec review, 2026-06-15), all folded:** (1) reproducible teeth-test with an exact
revert-target (AUD-K8-1, `backlog_drain.rs:725`) + command; (2) RETURN corrected — it is *live*
(`inline.rs` SELL/RETURN, green test), excluded only for Phase-0 narrowness; (3) transition ops drive
real seams (`go_online` → `return_online_probe::run_tick_for_fn` + drain), raw setters are fixture-only;
(4) mirror-2 exact predicates (an empty active session is legal — no false positive); (5) crash injection
split into wire-await drop-injection vs. committed-envelope stage-composition (no timing hooks inside a
DB tx).

## 14. Acceptance criteria (MVP done)

- `proptest` strategy generates valid + invalid/re-entry sequences over the §5 alphabet, honoring
  preconditions, with shrinking.
- Interpreter drives every op through the real seams; `ScriptedDps` is the shared stub.
- Reference model + differential pass on a known-good corpus; the three-layer oracle + mirror checks run
  at quiescent boundaries.
- **Teeth test (hard gate, reproducible).** The fuzzer must re-discover a known historical defect via a
  *revert-on-current-main* procedure — the fuzzer is new code, so it cannot run at the historical commit
  where it did not exist; instead revert the specific fix on current `main`, run the fuzzer, confirm the
  finding + shrink, then restore. **Concrete target — AUD-K8-1:** revert the drain-entry RMR re-entry
  guard (`backlog_drain.rs:725` — the `if ns.shift_state == RequiresManualReconciliation { return
  Ok(DrainSummary::new(fn, 0)) }` block; landed in fix `a171f18`, PR #168); run `cargo nextest run -p prro
  --features test-support <fuzzer_test>`; confirm the fuzzer finds the re-tick **re-drive / busy-loop** (a
  manual-recon FN re-driven by the next drain tick — no idempotent halt) and **shrinks** it to a minimal
  `[…, escalate, re-tick]` sequence; restoring the guard returns the fuzzer to green. A second target —
  **M2-N1** (revert the strict-sequential halt → fuzzer finds the orphaned-successor send) — is
  recommended once the alphabet covers it.
- A failing seed replays deterministically and produces a minimal repro.

## 15. Phase-0 hardening (post-T7, CI-grade harness oracle)

T0–T7 landed (PRs #181–#189); the external review confirmed the fuzzer **bites** as a regression signal
(the AUD-K8-1 teeth cycle reproduces) but flagged **false-negative zones** where the harness accepts an
incomplete/erroneous state as admissible — or never scans it. This section is the LOCKED contract for the
hardening pass that closes those zones (one task, `tests/`-only, no `src/` changes, strict test-first;
each finding its own RED→GREEN; **STOP for architect review after Cluster A**).

**Locked — do NOT revisit (hardening ADDS to these, never weakens them):**
- The **SETTLED-mode scan gate** (`assert_clean` / `check_mirrors` run only when `mode ∈ {Online, Offline}`)
  is correct — it is the principled reading of §7.2 (mid-crash and `GoingOnline` mid-transition are the two
  classes of legitimate in-flight transient). Hardening adds a **liveness bound**, it does not lift the gate.
- The **AUD-K8-1 teeth stays MODE-INDEPENDENT** — a bounded-postcondition on wire calls
  (`shift_before == RMR ⇒ send_calls unchanged`), never a scan, so the SETTLED gate cannot blunt it. No new
  check may depend on scanning in `GoingOnline`.

### Cluster A — crash/scan correctness
- **A1 — `pending_crash` from the REAL outcome, not the op name.** A `Crash(Send)` on an Offline node never
  reaches the wire and completes as a real offline-sell (`crash_via_drop`'s `res = &mut fut => Some(res)`
  arm); keying `pending_crash` off `Op::Crash(_)` then wrongly suppresses the settled-boundary scan. Set it
  from `RealOutcome::Crashed{..}`.
- **A2 + A4 — terminal `settle_and_scan` (closes the unpaired-crash gap AND the `GoingOnline` liveness
  bound).** A sequence may end after a real crash (dirty DB never scanned) or in `GoingOnline` (an unbounded
  no-scan zone). At end of the harness run, if not settled, drive a **bounded** settle and then a mandatory
  `assert_clean` + `check_mirrors`. Three architect refinements are part of this contract:
  - **Settle via REAL recovery seams (`Reboot`; a real drain tick if `GoingOnline` + backlog), NEVER
    `force_node_mode`.** A4 asks "does the system settle on its own?" — forcing the mode answers it
    artificially and masks a real liveness bug. `force_node_mode` stays confined to adverse-intent setup.
  - **`RequiresManualReconciliation` is a LEGITIMATE durable terminal, not a liveness violation.** A
    `[offline sells, GoOnline([Reject])]` sequence legitimately ends in `GoingOnline` + RMR (the operator
    surface; the system must NOT auto-settle from it). Define `settled ⟺ mode ∈ {Online, Offline} OR
    shift_state == RequiresManualReconciliation`. Do not force-settle or liveness-panic on RMR; the scan
    must pass on a legitimate RMR state (if it flags there, that is a REAL finding — do not suppress).
  - **`GoingOnline` without an active offline session is an IMPOSSIBLE real-system state — a test-setter
    artifact, NOT a liveness bug (refinement found in implementation, 2026-06-16; verified by 3 facts +
    a spike).** Real `GoingOnline` is only ever entered via `return_online_probe` from Offline, i.e. WITH a
    session; the adverse `OfflineSellDuringGoingOnline` op force-sets it on an Online node with no session +
    an online-origin `SENDING` doc. That state cannot be settled by real seams (drain with no session is a
    no-op — `backlog_drain.rs` `no_active_offline_session`; reboot defers, branch d) and must not be scanned
    (the deferred `SENDING` false-flags as `StuckSending` — the very thing the SETTLED gate exists for). So
    the liveness check is **gated structurally on settle-ability** (NOT an allowlist of violation types),
    using the seam's own `current_open_or_draining_session` predicate:
    - `GoingOnline` **with** an active OPEN/DRAINING session → legitimate, must settle in bounded N → else
      **liveness panic** (a real bug). This is where the liveness check has teeth (the offline harness, via
      a real `GoOnline` probe, reaches it).
    - `GoingOnline` **without** an active session → the artifact → do NOT panic, do NOT scan; assert only the
      universal **bounded no-resend** (the real recovery ops did not re-send — `send_calls` unchanged). A
      durable directed pin documents WHY (the impossible-state reasoning), keeping the skip auditable.
- **A3 — bounded crash-postconditions wired into the property harness.** Faults currently only
  `resync_from_db`; the random search never asserts the no-resend invariant. On the resolving `Reboot`,
  assert the bounded postcondition IN the harness. **Universal invariant under composition = NO-RESEND**
  (`send_calls` unchanged through the reboot for the crashed doc; `last_calls` advances for `Crash(Kvt1)`);
  do NOT assert an exact terminal state in the harness (a SENT doc may legitimately probe → KVT1 / ACK /
  manual) — exact terminals stay in the directed K3/K4 tests.

### Cluster B — oracle completeness (after Cluster-A review)
- **B1 — faithful mid-wire cohort loading (prerequisite) + bounded postconditions for deferred exotic
  drain.** The interpreter loads the drain stub from the `OFFLINE_LOCAL_ACK` count only; the real cohort is
  wider (`SENT`/`KVT1`/`ERROR_RETRYABLE`/`KVT2` — `list_drain_candidates_*`). First load send/last responses
  per the real per-doc state (else the stub under-loads and you test an empty-queue error, not re-entry);
  then, for drain ops the model defers to `Fault`, assert bounded postconditions instead of a silent resync
  (send-delta ≤ |cohort|; no code consumed; `next_lnd` monotonic; seed moves only if the class expects it;
  RMR-halt vs not per the class).
- **B2 — `ExpectedNoMutation` asserts no row / `next_lnd` mutation, via a model-tagged split.** A blanket
  "no new row" is WRONG — an online reject legitimately mints a non-issued `Rejected` row and bumps
  `next_lnd`. Split the model outcome: `TrueNoMutation` (refusal-before-wire → assert doc-count / ledger /
  `next_lnd` / seed / codes all unchanged) vs `NoIssuanceRowAllowed` (online reject → ≤1 new row, it is
  `Rejected`/non-issued, seed unchanged).
- **B3 — Recovered (drain / go-online) ledger-delta checks seed / codes / `next_lnd`, not just states.**
  `check_ledger_delta` compares only `lnd → state`; take a full before/after snapshot for Recovered ops —
  a drain must not consume extra codes, bump `next_lnd`, or move the seed unexpectedly.

**Out of scope (Phase 1+):** `src/` changes; the per-case temp-DB leak (`std::mem::forget`, a separate
follow-up); RETURN/Z/EVPZ/clock alphabet; model-predicts-recovery; WebCheck; nightly large-N; narrowing the
SETTLED suppression to a per-violation filter (A4 uses a final force-settle, not a redesign).
