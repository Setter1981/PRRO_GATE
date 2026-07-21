# CS-3 S7-1 · A3 — cutover-prerequisite fixes (A: NC-03 authority · B: §7.1 trace) — design

Two cutover-prerequisite fixes confirmed by external review of the landed A3 pass. Grounded at the
worktree tip. Frozen invariants #1/#2/#4/#8/#9 preserved (argued per fix). No design change to §7.1/§7.2
— these implement them faithfully. Minimal diff; RED-first teeth.

---

## Fix A — NC-03 authority restoration (BLOCKER-before-cutover)

**Defect (verified):** when `node_state` is lost while a live reservation R survives,
`reconstruct_lost_node_state` re-creates node_state with `delivery_generation = default` +
`active_delivery_reservation_id = NULL` (`upsert_initial_tx` only sets mode/shift/next_lnd;
`node_state.rs:110`). Then BOTH `apply_outcome` (`delivery_reservation.rs:852`) AND
`complete_operator_pending` (`delivery_reservation.rs:1286`) fail the identical authority CAS
(`authed_gen == cur_gen AND active_ptr == reservation_id`) → the reservation is stuck OO+PENDING +
active fence + `Sending` **forever**; the "operator-led" recovery is a dead end.

**Fix (external recipe, atomic):** reconstruct **and** restore authority in ONE transaction, so a
crash-between rolls back and re-defers on the next boot.

### A.1 `reconstruct_lost_node_state` gains a restore param (`boot_phase.rs`)

Change signature to
`reconstruct_lost_node_state(pool, fiscal_number, restore: Option<ReservationId>) -> anyhow::Result<bool>`
(returns `true` if it reconstructed — ledger non-empty — `false` if empty). Each caller owns its own
outcome:
- **branch-(a)** (`run_boot_reconciliation`, NO reservation): calls `…(pool, fn, None)`; on `true`
  returns `Ok(BranchOutcome::BlockedLedgerWithoutNodeState)` (unchanged behaviour — BLOCKED, pointer
  NULL, `BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED` audit). Existing NC-03 tests must stay green.
- **the pass** (step 3): calls `…(pool, fn, Some(reservation_id))`.

In the single `with_immediate`, after `upsert_initial_tx` + optional seed:
```
match restore {
  None => { set_mode_blocked_tx; audit BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED (existing payload) }
  Some(res_id) => {
     let gen = SELECT authorized_generation FROM delivery_reservation WHERE reservation_id=?  // R's gen
     UPDATE node_state SET delivery_generation=?, active_delivery_reservation_id=? WHERE fiscal_number=?  // mirror authorize_submission :542
     set_mode_stop_mode_tx                                                                     // STOP, not BLOCKED
     audit BOOT_LEDGER_WITHOUT_NODE_STATE_RESERVATION_RESTORED (Critical; payload: reservation_id, restored_generation, next_lnd)
  }
}
```
`gen == R.authorized_generation` is exactly the value `complete_operator_pending`'s CAS compares to, so
after restore the operator path succeeds. Single active reservation per FN (the §3.1 fence) + node_state
was lost (no other active reservation can exist) ⇒ **no fork, no stale-generation collision**. STOP (not
BLOCKED) is required so `complete_operator_pending`'s `STOP_MODE → target` mode-CAS matches.

### A.2 Pass bookkeeping + not-auto-apply (`reservation_boot_pass.rs`)

- `deferred: BTreeSet<String>` → `BTreeMap<String, ReservationId>` — populated at defer time in step 1
  (the CALL_STARTED `reservation_id`) and step 2 (the OO+PENDING `reservation_id`). A FN has ≤1 active
  reservation, so ≤1 entry per FN.
- Step 3: `for (fscl, res_id) in &deferred { if reconstruct_lost_node_state(pool, fscl, Some(*res_id))? { reconstructed += 1 } }`.
- Step 4: **retry normalize only** (a deferred CALL_STARTED → resume → OO+PENDING; resume touches only
  `delivery_reservation` + mode, so it PRESERVES the restored pointer/gen). **Remove the step-4
  auto-apply for deferred FNs** — an NC-03 FN must not auto-stamp/advance a seed on a just-reconstructed
  node; the reservation rests OO+PENDING with restored authority for `complete_operator_pending`.
  (After restore the CAS would PASS, so auto-apply WOULD fire; skipping it is the deliberate,
  conservative choice §7.2 intends.)

**Final state per deferred sub-case (FIRST boot):** CALL_STARTED-deferred → OO+PENDING (evidence
`NoResponse{Crashed}` — a HOLD, never auto-applied), node STOP, gen/pointer restored; OO+PENDING-deferred
→ OO+PENDING (unchanged), node STOP, gen/pointer restored. Both are operator-resolvable via
`complete_operator_pending` on the first boot.

**Second-boot nuance (re-audit, MINOR — documented, no guard).** If the node reboots BEFORE the operator
acts, an OO+PENDING-deferred reservation whose evidence is an **auto-release** outcome (an online
`Accepted` / definitive reject) is no longer deferred (node_state now exists), so step 2's `apply_outcome`
CAS passes (authority was restored on boot 1) and it **auto-applies** — the doc reaches its correct
terminal state (fiscally correct, **no double-issue, no fork**), but the node stays STOP_MODE and the
operator clears it via the admin `reset_stop_mode` path rather than `complete_operator_pending`. The
`CALL_STARTED`-deferred case is immune (its `NoResponse` evidence is always a HOLD). This exotic
double-failure (NC-03 + reboot-before-operator) is left as-is: the outcome is correct and a defer-if-
already-restored guard would enlarge the diff for no soundness gain. Pinned by
`s7_nc03_authority_restore_oo_pending_deferred_path` (first-boot path) + this note.

### A.3 Tooth (RED-first, empirical revert)

`tests/s7_boot_reservation_pass.rs`: seed an online reservation R (CALL_STARTED or OO+PENDING) + its
`Sending` doc, then **DELETE the node_state row** (the NC-03 condition), run `App::reconcile_pending`.
Assert: node_state reconstructed (mode STOP), reservation OO+PENDING, and
**`complete_operator_pending(R, Accepted{F})` SUCCEEDS** (stamps F, clears PENDING). Revert-canary: drop
the gen/pointer restore → `complete_operator_pending` returns `StaleAuthority` → RED.

---

## Fix B — §7.1 transport_trace completeness (MAJOR-before-cutover)

**Defect (verified):** `normalize_one` calls only `resume_crashed_reservation`, which does NOT touch
`transport_trace` (0 refs). §7.1 (design:266) requires crash-normalize to atomically complete the
in-flight trace as crash + append a recovery audit. The `< 60 s` orphan scanner
(`close_orphan_transport_traces`) deliberately skips fresh traces, so a fast restart leaves the crashed
reservation's doc trace open.

**Fix:** in `normalize_one`'s tx, after resume, close the reservation's doc's OPEN trace with the same
**SYSTEM_CRASH** semantics the orphan scanner uses (`boot_phase.rs:1607` — `outcome_kind='SYSTEM_CRASH'`),
but marked as the reservation-resume path, regardless of the 60 s TTL (the CALL_STARTED reservation is a
definitive crash):
```
// inside normalize_one's with_immediate, after resume:
let doc_id = SELECT document_id FROM delivery_reservation WHERE reservation_id=?
if let Some(attempt_no) = SELECT attempt_no FROM transport_trace
                          WHERE document_id=? AND outcome_kind IS NULL ORDER BY attempt_no DESC LIMIT 1 {
   UPDATE transport_trace SET completed_at=now, wire_call_started_at=started_at, wire_call_finished_at=now,
          outcome_kind='SYSTEM_CRASH', error_kind='CRASHED_RESERVATION_AT_BOOT',
          error_message='Reservation resumed at boot; wire outcome unknown'
     WHERE document_id=? AND attempt_no=? AND outcome_kind IS NULL
   audit_log::append_tx("transport_trace", doc_hex, "TRANSPORT_TRACE_CRASHED_RESERVATION_CLOSED", Info, …)
}
```
`resume` + trace-close + audit all in the ONE `with_immediate` (atomic, §7.1). `NoResponse{Crashed}` is
NOT `complete_via_recovery_tx` (that stamps `OutcomeKind::Ok` — a recovery that FOUND acceptance — which
is the wrong semantics for an unknown crash outcome).

### B.1 Tooth

Seed a CALL_STARTED reservation + its doc + an OPEN (`outcome_kind IS NULL`) `transport_trace` row
started `< 60 s` ago (so the orphan scanner skips it) → boot → assert the trace is completed
(`outcome_kind='SYSTEM_CRASH'`) and the `TRANSPORT_TRACE_CRASHED_RESERVATION_CLOSED` audit exists.
Revert-canary: remove the trace-close from `normalize_one` → the trace stays open → RED.

---

## Fix C — folded observability MINORs (internal review, same region)

- `normalize_one` propagates the `resume_crashed_reservation` bool; `summary.normalized` increments only
  on a real conversion (`true`).
- Step-4 `HeldNotAutoRelease` arm emits the same `tracing::warn!` as step 2 (parity).

---

## Invariant check

- **#1** (no net/crypto in a write tx): all additions are DB-only (trace UPDATE, node_state UPDATE, audit). ✓
- **#2** (single-writer): runs under the recon mutex; each op its own BEGIN IMMEDIATE. ✓
- **#4** (idempotency): reconstruct is INSERT-then-set (a second boot finds node_state present, no
  re-defer); trace-close `WHERE outcome_kind IS NULL` is idempotent; the restore UPDATE is deterministic. ✓
- **#8** (recovery preserves the state machine): no new doc edges; the restore only rewrites node_state
  authority columns to R's own captured values; resume/apply transitions unchanged. ✓
- **#9** (graceful over fast): the fixes add no new fatal path; a missing trace (None) is a no-op. ✓

## How this could still be wrong (self-adversarial — for the re-audit)

1. **Restore generation choice.** Is `R.authorized_generation` always the right `cur_gen` to restore?
   If two reservations somehow existed (fence breach), restoring one's gen could mis-authorize. Mitigated
   by the fence (≤1 active) + node_state loss (no survivor) — but the re-audit must confirm the fence
   truly guarantees ≤1 CALL_STARTED-or-OO+PENDING per FN at boot.
2. **Not-auto-apply vs. the frozen crash-window matrix.** §7.2's matrix says boot invokes the shared
   apply for OO+PENDING. Skipping auto-apply ONLY for NC-03-deferred FNs (not the normal path) must be
   shown consistent with §7.2 (normal OO+PENDING still auto-applies in step 2).
3. **Trace attempt_no selection.** Picking the newest open trace assumes ≤1 open trace per crashed doc.
   Confirm a crashed CALL_STARTED reservation's doc can have at most one `outcome_kind IS NULL` trace.
4. **STOP vs BLOCKED downgrade for NC-03.** Restoring to STOP (not BLOCKED) is required for operator
   completion, but does any consumer treat an NC-03 node in STOP more permissively than BLOCKED? (C1
   earlier held for ingress; re-confirm for any boot/drain path.)

## GO / NO-GO

**GO to implement TDD**, then decorrelated re-audit of the fix (esp. items 1–4 above) before push.
