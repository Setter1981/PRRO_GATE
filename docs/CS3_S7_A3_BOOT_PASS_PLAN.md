# CS-3 S7-1 · A3 — Boot-first reservation pass (§7.1/§7.2) — implementation plan

**Status:** implementation plan for the INACTIVE boot-first reservation pass. Design-of-record is
FROZEN (`docs/CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` §7.1/§7.2/§8, §11 checklist item #4). This
document is the *how*; §7.2 is the *what* and is not re-opened. Grounded at worktree tip `1bd040f`.

---

## 0. Scope & INACTIVE premise (verified)

`authorize_submission` / `submit_authorized` have **zero production callers** at `1bd040f` (grep over
`src/` returns only the definitions + test callers). No `NewReservation` is ever minted in production
until the Slice-7 cutover relocates the wire behind `submit_authorized`. Therefore both boot queries
(`list_call_started_without_outcome`, the new `list_outcome_observed_pending_apply`) return **empty**
on every real boot today → the whole pass is a **provable no-op in production** until cutover. It is
landed now, tested now, so the atomic cutover only has to flip the wire.

Insertion is **global, pre-loop**, per §7.1 (not per-FN): a per-FN insertion would be skipped for
later FNs whenever an earlier FN hits an early return in `run_boot_reconciliation`
(branch-f STOP/Blocked/CryptoDegraded `boot_phase.rs:1901`, manual-recon `:1953`, OfflineRefusal).

---

## 1. New repo fn — `list_outcome_observed_pending_apply` (delivery_reservation.rs)

Pool-bound, mirrors `list_call_started_without_outcome` (`delivery_reservation.rs:1038`) exactly:

```rust
pub async fn list_outcome_observed_pending_apply(
    pool: &sqlx::SqlitePool,
) -> sqlx::Result<Vec<(ReservationId, String)>> {
    // WHERE state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY'
}
```

RED-first: a repo test asserting it returns only rows in that exact state (a CALL_STARTED row and an
APPLIED row are both excluded).

---

## 2. NC-03 reconstruction — extract a callable helper (boot_phase.rs, behavior-neutral)

§7.2 step 3 must invoke the *same* NC-03 seed repair that today lives inline in
`run_boot_reconciliation` branch-(a) (`boot_phase.rs:1752–1836`). Extract the **non-empty-ledger**
reconstruction into:

```rust
pub(crate) async fn reconstruct_lost_node_state(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> anyhow::Result<Option<BranchOutcome>>
// Some(BranchOutcome::BlockedLedgerWithoutNodeState) when the ledger is non-empty (reconstruct +
//   BLOCK + CRITICAL audit, one with_immediate envelope — identical to today);
// None when the ledger is empty (a genuinely fresh FN — the caller keeps the original bootstrap).
```

Branch-(a) becomes: `if let Some(o) = reconstruct_lost_node_state(pool, fn).await? { return Ok(o); }`
followed by the unchanged empty-ledger fresh-FN bootstrap. **Pure extraction** — same reads, same
writes, same audit, same return. Guarded by the existing NC-03 tests
(`tests/app_boot_reconciliation.rs` branch-a, `tests/backup_restore.rs`, `tests/invariant_scan.rs`,
`tests/fn_fence_active.rs`) which must stay green (the neutrality proof).

For a deferred FN in the reservation pass the ledger is always non-empty (a reservation implies a doc
implies a ledger row), so `None` is unreachable there; treat `None` as a benign skip.

---

## 3. `run_reservation_boot_pass` (boot_phase.rs) — the §7.2 five steps

Lives in `boot_phase.rs` (co-located with `with_immediate` `src/db/tx.rs:118`, the reservation repo,
and `reconstruct_lost_node_state`). `app.rs` only adds one call. Runs entirely under the recon mutex
already held by `reconcile_pending_inner`. **No wire I/O, no crypto** — every op is a DB tx (inv #1).

```
async fn run_reservation_boot_pass(_guard, pool) -> Result<ReservationBootSummary, ApplyError-mapped>:
  deferred: BTreeSet<String> = {}

  # Step 1 — normalize crashed CALL_STARTED (each in its own BEGIN IMMEDIATE)
  for (res_id, fn) in list_call_started_without_outcome(pool):
      if node_state::get(pool, fn).is_none():          # NC-03: defer, do NOT convert yet
          deferred.insert(fn); continue
      with_immediate(pool, |tx| resume_crashed_reservation(tx, res_id, fn))   # → OO+PENDING NoResponse{Crashed} + STOP

  # Step 2 — apply OO+PENDING (each in its own BEGIN IMMEDIATE; rolls back on any Err)
  for (res_id, fn) in list_outcome_observed_pending_apply(pool):
      match with_immediate(|tx| apply_outcome(tx, res_id).map_err(anyhow)):
          Ok(_)                              => applied += 1
          Err e if downcast HeldNotAutoRelease => warn!("expected hold"); continue     # -12/-6/SubmittedUnknown — inv #9
          Err e if downcast NodeStateMissing   => deferred.insert(fn); continue        # NC-03: defer, do NOT fail boot
          Err other                            => return Err(other)                    # genuine breach → fail boot

  # Step 3 — reconstruct node_state for deferred FNs (UNFENCED by design — §7.2; a live PENDING
  #          reservation is EXPECTED here; fencing boot_phase:1814 would strand the deferred apply)
  for fn in deferred: reconstruct_lost_node_state(pool, fn)?     # → node BLOCKED + CRITICAL audit

  # Step 4 — retry the deferred normalize + apply (node_state now exists)
  if deferred not empty:
      for (res_id, fn) in list_call_started_without_outcome(pool):
          if fn in deferred: with_immediate(|tx| resume_crashed_reservation(tx, res_id, fn))
      for (res_id, fn) in list_outcome_observed_pending_apply(pool):
          if fn in deferred:
              match apply_outcome: Ok=>applied; HeldNotAutoRelease=>warn+continue;
                                   NodeStateMissing=>Critical log + continue (post-reconstruct: must not brick);
                                   other=>Err
  # Step 5 (the existing per-FN loop) runs only after this returns.
```

`with_immediate` returns `anyhow::Result`; `apply_outcome`/`resume_crashed_reservation` return
`Result<_, ApplyError>`. The closure returns `inner.map_err(anyhow::Error::new)` so **the tx rolls
back on any error** (a partial effect-write is never committed), and the pass `downcast_ref::<ApplyError>()`
to decide continue-vs-fail. A `HeldNotAutoRelease` / `NodeStateMissing` is returned *before* any write
in `apply_outcome` (`:851`, `:904/912/942`), so its rollback is a no-op.

---

## 4. Corner-case resolutions (INACTIVE; both non-trading, no double-issue, no fork)

These arise only in the exotic NC-03 interleave (node_state lost while the ledger **and** an active
reservation survive — a partial-restore anomaly, "ЧП из ЧП"). Resolved for minimal diff + soundness:

- **C1 — BLOCKED→STOP precedence (step 4 normalize).** `set_mode_stop_mode_tx` is an unconditional
  UPDATE (`node_state.rs:259`, no mode CAS). After step 3 sets **BLOCKED**, a step-4 `resume` for a
  deferred CALL_STARTED clobbers it to **STOP_MODE**. Accepted: both modes refuse ingress (non-trading);
  STOP is in fact what a crashed reservation *needs* (it is the mode operator-completion resolves from);
  and the `BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED` CRITICAL audit is permanent. No new mode-preserving
  logic (would enlarge the hot-zone diff for an exotic corner that is already safe).

- **C2 — authority-CAS-miss orphan (step 4 apply).** `upsert_initial_tx` (`node_state.rs:110`) is
  `INSERT … ON CONFLICT DO UPDATE SET mode, shift_state` — a lost row is re-INSERTed with
  `active_delivery_reservation_id = NULL` + default `delivery_generation`. So a step-4 `apply_outcome`
  for a reconstructed FN **fails the generation CAS** (`active_ptr == reservation_id` is false) and
  returns the benign stale-drop `Ok{applied:false}` — it mutates nothing (no wire, no seed advance, no
  fork — that part holds).
  **⚠️ REVIEW CORRECTION (external, 2026-07-21 — was WRONG below):** the original claim that the
  reservation "rests OO+PENDING for the operator" is FALSE. `complete_operator_pending`
  (`delivery_reservation.rs:1286`) runs the **same** authority CAS (`authed_gen == cur_gen AND
  active_ptr == reservation_id`); after reconstruction it ALWAYS returns `StaleAuthority`, so the
  operator path is a **dead end** — the reservation is stuck OO+PENDING + active fence + `Sending`
  **forever**. This is a **cutover prerequisite** fix (§8-A), not an INACTIVE-merge blocker (empty
  pre-cutover), and NOT a design change — it restores the frozen §7.2 "operator-led" intent.

## 8. Cutover prerequisites (external + internal review, 2026-07-21)

Both reviews cleared **A3 as INACTIVE machinery for merge** (0 production blockers — empty queries,
provable no-op; C-WIRE / C-FORK / C-NEUTRAL / C-EXTRACT / C1 all held). Two fixes are required **before
the live Slice-7 cutover** (which is separately GO-gated); they are scoped here with grounded recipes
and must land with the cutover's arch-planner + re-audit rigor (soundness-critical — not blind SQL):

- **A. NC-03 authority restoration (BLOCKER-before-cutover).** During reconstruction of a deferred FN
  that has exactly one active reservation R, restore — in the same tx — `delivery_generation =
  R.authorized_generation`, `active_delivery_reservation_id = R.reservation_id`, mode `STOP_MODE` (not
  BLOCKED), and do **not** auto-apply R (leave it to `complete_operator_pending`). `reconstruct_lost_node_state`
  is shared with branch-(a) (no reservation → keep pointer NULL/BLOCKED), so the restore is
  reservation-conditional and belongs in the pass's step-3, not the shared helper. Tooth: delete
  `node_state` with a live reservation → boot → `complete_operator_pending` succeeds (clears PENDING),
  and a revert of the restore REDs it.
- **B. §7.1 transport_trace completeness (MAJOR-before-cutover).** `normalize_one` currently calls only
  `resume_crashed_reservation`, which does NOT touch `transport_trace` (verified: 0 refs). §7.1
  (design:266) requires the crash-normalize to atomically complete the in-flight trace as crash + append
  a recovery audit. The `< 60 s` orphan scanner (`close_orphan_transport_traces`) deliberately skips
  young traces, so a fast restart leaves the reservation's doc trace open. Fix: in the normalize tx,
  read R's `document_id`, `complete_via_recovery_tx` (`transport_trace.rs:475`) the open trace as crash,
  and `audit_log::append_tx` a recovery row. Tooth: crashed reservation + open young trace → boot →
  trace completed + recovery audit present.
- **C. Folded observability MINORs (internal review, land with A/B, same step-3/4 region):**
  propagate the `resume_crashed_reservation` bool so `summary.normalized` counts only real conversions;
  add the step-4 `HeldNotAutoRelease` warn (parity with step 2). No soundness impact.

These are documented so the review can attack the *reasoning*, not just the code.

---

## 5. S7-P4-BOOT tooth (§8) — RED-first, empirical bite

New non-frozen test file `tests/s7_boot_reservation_pass.rs`. Uses `App::boot` + the real
`reconcile_pending` (the production boot entrypoint) over a temp-file DB, seeding reservations via the
`apply_outcome.rs` pattern (`new_res`/`authorize`/`record`) against the App's pool.

Seed (distinct FNs to isolate node-mode effects):
1. one `CALL_STARTED` reservation **+ its `Sending` doc**;
2. one `OUTCOME_OBSERVED+PENDING_APPLY` **auto-release** (online `Accepted`) — must be *applied once*;
3. one `OUTCOME_OBSERVED+PENDING_APPLY` **MacReseedPending hold** — must NOT surface as a boot error.

Assertions after a correct boot: **wire count == 0**; reservation #1 → OO+PENDING NoResponse{Crashed}
+ node STOP; reservation #2 → APPLIED (SFN stamped, seed advanced once); reservation #3 → still
PENDING_APPLY (held), boot returned `Ok`.

**Order-swap revert-canary (the load-bearing bite).** The distinguishing state between "pass before
loop" (correct) and "pass after loop" (swapped) is **doc #1's state**, pinned **empirically** when the
canary is run:
- correct order: pass normalizes #1 → node STOP → the per-FN loop early-returns (branch-f on STOP) →
  **doc #1 stays `Sending`** (awaits operator completion — the crashed wire is not auto-redriven);
- swapped order: the loop resumes the `Sending` doc → **doc #1 becomes `ErrorRetryable`** *before* the
  pass sets STOP.

The canary asserts doc #1 == `Sending` after a correct boot; moving the pass call to *after* the loop
must flip it to `ErrorRetryable` and RED the test. The exact distinguishing state is confirmed by
running the revert (per teeth-ROI: bite proven empirically, not on assertion of intent). Wire-count is
0 in both orders, so it cannot be the canary — hence the state assertion.

Also extend/lean on `S7-APPLY-GRAPH` for the apply projection (already landed); this tooth adds the
**boot orchestration + ordering** coverage.

---

## 6. Invariant / neutrality check

- **inv #1** (no net/crypto in a write tx): the pass does only DB txs; `resume`/`apply`/`reconstruct`
  never touch the wire. ✓
- **inv #2** (single-writer per FN): runs under the recon mutex; every op is its own BEGIN IMMEDIATE. ✓
- **inv #4** (idempotency): `apply_outcome` is generation-CAS idempotent; `resume` is a
  state-guarded UPDATE (`WHERE state='CALL_STARTED'`). Re-running the whole pass is a no-op. ✓
- **inv #8** (recovery preserves the state machine): CALL_STARTED→OO+PENDING and the apply projection
  are the landed Slice-4 transitions; no new edges. ✓
- **inv #9** (graceful over fast): `HeldNotAutoRelease` and `NodeStateMissing` never abort boot. ✓
- **INACTIVE**: empty lists ⇒ provable no-op pre-cutover. The extraction is behavior-neutral. ✓

## 7. Files touched

- `src/db/repositories/delivery_reservation.rs` — `+ list_outcome_observed_pending_apply`.
- `src/services/reconciliation/boot_phase.rs` — extract `reconstruct_lost_node_state`;
  add `run_reservation_boot_pass`.
- `src/app.rs` — one call in `reconcile_pending_inner`, pre-FN-loop (after `list_all`).
- `tests/…` — repo-fn RED test; `tests/s7_boot_reservation_pass.rs` (S7-P4-BOOT).

Cutover (Slice-7 B) is unchanged and still GO-gated; A3 moves no live wire.
