# CS-3 S7-1 · A3 (boot-first reservation pass) — External Review Brief

**For a model-decorrelated external auditor.** Prior CS-3 reviews proved that same-model review (even
5-lens adversarial) misses what a different-model reviewer, building explicit crash/restore
counterexamples, catches (the per-FN-fence-vs-per-doc-call-once SYSTEMIC finding). Please review in
that spirit: **ground every claim in the live code, and try to CONSTRUCT a sequence that breaks it.**

Branch `cs3-de-slice7-s0`, worktree tip = the A3 commit(s) on top of `1bd040f`. Read the code, do not
trust this prose.

---

## 0. What A3 is (and its risk surface)

A3 lands a **boot-first delivery-reservation pass**: once, globally, BEFORE the per-FN boot
reconciliation loop, `App::reconcile_pending_inner` calls
`services::reconciliation::reservation_boot_pass::run(&guard, pool)`. It resolves reservations a crash
left mid-lifecycle:

1. **normalize** every `CALL_STARTED` → `resume_crashed_reservation` (→ `OUTCOME_OBSERVED` +
   `PENDING_APPLY`, evidence `NoResponse{CrashedBeforeObservation}`, node **STOP_MODE**);
2. **apply** every `OUTCOME_OBSERVED + PENDING_APPLY` → `apply_outcome` (generation-CAS idempotent);
   `HeldNotAutoRelease` = expected hold (log, continue), `NodeStateMissing` = defer;
3. **reconstruct** lost `node_state` for deferred FNs (`boot_phase::reconstruct_lost_node_state` →
   node **BLOCKED** + CRITICAL audit, **UNFENCED**);
4. **retry** the deferred normalize + apply.

**It is INACTIVE.** `authorize_submission` has no production caller yet (verify:
`grep -rn 'authorize_submission' rust/prro/src` = definition only), so no `NewReservation` is minted
in production, so both boot queries return empty → the pass is a **no-op in production today**. It is
landed + tested now so the future Slice-7 cutover only flips the live wire.

**The design-of-record (`docs/CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` §7.1/§7.2/§8/§11) is FROZEN.**
Do NOT re-open the §7.2 design. Attack the **implementation's fidelity to §7.2 and its soundness**.

Two risks dominate:
- **Neutrality now** — does landing this change prod boot behavior when the tables are empty? Is the
  NC-03 extraction byte-behaviour-neutral?
- **Soundness at cutover** — when reservations exist, can the pass double-issue, fork a chain, or
  brick boot?

---

## 1. Ground truth — verify these anchors, don't trust them

- `src/services/reconciliation/reservation_boot_pass.rs` — the pass. `run` (the 4 steps), `apply_one`
  (wraps `apply_outcome` in `with_immediate`, downcasts `ApplyError` to classify), `normalize_one`.
  **The fn signature takes only `(&ReconcileGuard, &SqlitePool)` — NO `DpsChannel`.**
- `src/db/repositories/delivery_reservation.rs`:
  - `apply_outcome` (~795): generation-CAS at ~852 (`authed_gen == cur_gen AND active_ptr == reservation_id`);
    stale → `Ok{applied:false}` no-op (~856); `HeldNotAutoRelease` returns at ~904/912/942, `NodeStateMissing`
    at ~851 — **all BEFORE any write** (so a rollback of that tx is a no-op); seed advance only for online
    `Accepted` under `offline_fiscal_no IS NULL` (~886).
  - `resume_crashed_reservation` (~1067): `UPDATE … WHERE state='CALL_STARTED'` then
    `set_mode_stop_mode_tx`; it **ignores** that setter's bool → does NOT fail if `node_state` is missing.
  - `list_outcome_observed_pending_apply` (NEW): `WHERE state='OUTCOME_OBSERVED' AND apply_state='PENDING_APPLY'`.
- `src/db/repositories/node_state.rs`: `set_mode_stop_mode_tx` (~259) and `set_mode_blocked_tx` (~228)
  are **unconditional** UPDATEs (no mode CAS); `upsert_initial_tx` (~110) is
  `INSERT … ON CONFLICT DO UPDATE SET mode, shift_state` — a lost row is **re-INSERTed with
  `active_delivery_reservation_id = NULL`** and default `delivery_generation`.
- `src/services/reconciliation/boot_phase.rs`: `reconstruct_lost_node_state` (extracted helper, just
  above `run_boot_reconciliation`); branch-(a) now calls it (`if let Some(outcome) = reconstruct…`).
- `src/app.rs`: the pass call sits after `let mut summary = …` and BEFORE `for fn_cfg in &fns`, inside
  the `_recon_guard` critical section (mutex acquired ~534).

---

## 2. Refute each soundness claim (the load-bearing ones)

For each, try to build a concrete counterexample; if you cannot, say why it holds.

- **C-WIRE: the pass can never send.** It has no `DpsChannel` in scope and calls only
  resume/apply/reconstruct (all DB-only). The static pin `boot_pass_module_has_no_wire_access`
  (`tests/s7_boot_reservation_pass.rs`) asserts the module references no `DpsChannel`/`send_chk`.
  *Can you find any transitive path to the wire?*
- **C-FORK: no double-issue / no forked seed.** The seed advances only inside `apply_outcome`'s online
  `Accepted` arm, once, then marks `APPLIED` + clears the pointer atomically; a second apply hits the
  CAS (pointer cleared / `APPLIED`) → no-op. *Can step-1 resume + step-2 apply, or step-2 + step-4
  retry, advance the same seed twice? Can two reservations on one FN both apply?*
- **C-BRICK: boot never aborts on a tolerable outcome (invariant #9).** Only `HeldNotAutoRelease`
  (continue) and `NodeStateMissing` (defer) are non-fatal; every other `ApplyError`
  (`ReservationNotFound` / `NotPendingApply` / `DocumentMissing` / `MissingSeedHash` /
  `MissingFiscalNumber` / `DocTransitionFailed` / `Db`) fails boot. **Attack this hardest:** is there a
  LEGITIMATE crash/restore state where a benign row surfaces as one of the "fatal" variants and
  wrongly bricks the whole boot for every FN? (e.g. `NotPendingApply` from a concurrently/previously
  `APPLIED` row that reappears in the list; a `DocTransitionFailed` because the doc already left
  `Sending`.) Should any of those be defer/continue instead of fatal?
- **C-NEUTRAL: empty tables ⇒ provable no-op.** With no reservations, both list queries return `[]` and
  the pass returns a zero summary before the loop. *Any observable effect, ordering change, or error
  path on a normal boot?*
- **C-EXTRACT: `reconstruct_lost_node_state` is byte-behaviour-neutral vs the old branch-(a) inline.**
  Diff the helper against `git show HEAD~N:…boot_phase.rs`: same reads, same writes, same audit event +
  **same payload JSON string**, same `BranchOutcome::BlockedLedgerWithoutNodeState`, same empty-ledger
  fresh-FN fall-through. *Any divergence?*

---

## 3. The NC-03 interleave (§7.2) — the highest-risk corner

Node_state lost while the ledger **and** a live reservation survive ("ЧП из ЧП"). The pass defers (step
1/2), reconstructs `node_state` BLOCKED (step 3), retries (step 4). The plan
(`docs/CS3_S7_A3_BOOT_PASS_PLAN.md` §4) resolves two corners — **challenge the reasoning, not just the
code**:

- **C1 (BLOCKED→STOP):** step-4 `resume` flips the just-BLOCKED node to STOP (unconditional setter). The
  plan accepts this (both non-trading; STOP is the mode operator-completion resolves from; the CRITICAL
  audit is permanent). *Is losing the BLOCKED mode a real hazard — e.g. does any boot path or ingress
  gate treat STOP more permissively than BLOCKED in a way that lets an NC-03 FN trade?*
- **C2 (authority-CAS-miss orphan):** after reconstruction, `active_delivery_reservation_id = NULL`, so
  step-4 `apply_outcome` fails the CAS → benign stale-drop; the reservation rests `PENDING` forever
  under a non-trading node for the operator. *Is "rests PENDING, operator-led, stuck-doc-guard flags it"
  actually reachable/recoverable, or is the doc stranded in `Sending` in a way that violates a frozen
  invariant?*
- **Ordering hazards:** can a deferred FN be left UNREACHABLE, infinitely deferred, or reconstructed
  twice? Is there a TOCTOU in step-1's `node_state::get` (pool read) followed by a separate `resume` tx
  (both under the recon mutex — is single-writer really guaranteed at boot)? Is leaving
  `reconstruct_lost_node_state` **UNFENCED** correct (§7.2 says a live PENDING reservation is expected
  there), and does the S7-2 fence-exclusion pin still hold?

---

## 4. Gate & teeth — is the pin a HOLE, and does the tooth BITE?

- The pass is the first production caller of the still-INACTIVE `resume`/`apply`/`list_*` lifecycle, so
  `reservation_boot_pass.rs` is **excluded file-wide** from the `inactive_lifecycle_scan` "no production
  caller" pin (p03/rg08). The hole is meant to be closed by the positive allowlist pin
  `boot_pass_references_only_the_sanctioned_read_apply_subset` (`tests/migration_032_…`), which asserts
  the pass references EXACTLY {resume/apply/two lists} and NEVER a mint/authority symbol
  (`authorize_submission` / `record_outcome` / `delivery_reservation::insert` / `get_active_for_fn`).
  **Attack:** could someone add a forbidden lifecycle call to the pass WITHOUT REDding any test? Is the
  positive pin's allowlist complete and its matching exact (backtick-anchored)?
- **S7-P4-BOOT tooth** (`tests/s7_boot_reservation_pass.rs`): drives `App::reconcile_pending` over a
  crashed `CALL_STARTED`+`Sending`-doc, an online `Accepted`, and a `-12` hold. The **order-swap canary**
  asserts the crashed doc rests `SENDING` (pass-before-loop; the loop early-returns on STOP and does not
  auto-redrive it) — empirically REDs to `ERROR_RETRYABLE` when the pass is moved after the loop.
  **Attack:** is that the RIGHT distinguishing state? Are step-4 retry, `HeldNotAutoRelease`-in-retry, and
  the `reconstruct → None` branch untested? Is the wire=0 claim actually pinned (structural + ctx-free)?

---

## 5. Counterexample invitations (please try to build at least one)

1. A crash/restart SEQUENCE where a reservation is issued (or its seed advanced) and then the pass, on a
   later boot, advances it or a successor AGAIN (double-issue / fork).
2. A legitimate boot state where the pass returns `Err` and aborts reconciliation for ALL FNs (brick),
   where the frozen design intends graceful continuation.
3. A normal (empty-reservation) boot where landing A3 changes any observable outcome vs `1bd040f`.
4. An NC-03 FN that ends up TRADING (ingress accepted) despite the reconstruction, or a doc stranded in a
   frozen-invariant-violating rest state.

---

## 6. Verdict required

Return one of: **GO** (sound + neutral, land as-is) / **FIX_FOLLOWUP** (specific, bounded fixes) /
**SYSTEMIC** (a class defect — name it + a reproduction). For every finding: severity, `file:line`, a
concrete trace/repro (not "could"), and the smallest fix. Note which frozen §7.2 decisions you are
explicitly NOT contesting. Ground everything; prefer a constructed sequence over an assertion.
