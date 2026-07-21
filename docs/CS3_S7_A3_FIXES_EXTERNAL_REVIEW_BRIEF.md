# CS-3 S7-1 · A3 cutover-fixes — External Re-Review Brief (fixed state)

**For a model-decorrelated external auditor.** This is a SECOND external pass. In the FIRST pass you
returned **FIX_FOLLOWUP** on the landed A3 boot-first reservation pass, with one BLOCKER (NC-03 loses
authority → reservation stuck forever) and one MAJOR (§7.1: normalize doesn't complete the crashed
`transport_trace`). **Both are now fixed** (Fix A / Fix B, + a folded MINOR Fix C). Your job: verify the
fixes are SOUND and did NOT introduce a new defect. Ground every claim in live code; prefer a
constructed crash/restart sequence over an assertion.

Branch `cs3-de-slice7-s0` (uncommitted working tree at review time — read the files directly).

---

## 0. What changed since your first pass

- **Fix A — NC-03 authority restoration.** `reconstruct_lost_node_state` gained
  `restore: Option<ReservationId>` and now returns `bool`. `None` (branch-(a), no reservation) is
  unchanged (BLOCKED, `active_delivery_reservation_id = NULL`, same audit payload). `Some(R)` (the pass,
  a deferred FN with a surviving reservation) **restores authority in the same tx**: sets
  `delivery_generation = R.authorized_generation`, `active_delivery_reservation_id = R`, node
  **STOP_MODE** (not BLOCKED). Without it BOTH `apply_outcome` and `complete_operator_pending` failed the
  identical gen/pointer CAS forever. The pass now keys `deferred` by FN → its `ReservationId`, and step 4
  **no longer auto-applies** NC-03 FNs (only re-normalizes CALL_STARTED) — the operator resolves them.
- **Fix B — §7.1 trace completeness.** `normalize_one` now, in the SAME tx as `resume`, completes the
  reservation's document's OPEN `transport_trace` as `SYSTEM_CRASH` + a recovery audit (TTL-free), so a
  fast restart no longer leaves it open (the <60 s orphan scanner skips fresh traces).
- **Fix C — MINOR.** `normalize_one` returns whether it actually converted, so `summary.normalized`
  counts only real work.

**Design of record:** `docs/CS3_S7_A3_CUTOVER_FIXES_DESIGN.md` (read the "How this could still be wrong"
section + the "Second-boot nuance"). Frozen §7.1/§7.2 is NOT re-opened — attack the fixes' fidelity +
soundness.

---

## 1. Ground truth — verify, don't trust

- `src/services/reconciliation/boot_phase.rs` — `reconstruct_lost_node_state(pool, fn, restore)`: the
  `None`/`Some` branches; the `Some` branch reads `authorized_generation` from the reservation IN-TX,
  UPDATEs node_state gen+pointer (mirrors `authorize_submission` `delivery_reservation.rs:542`), sets
  STOP, audits `BOOT_LEDGER_WITHOUT_NODE_STATE_RESERVATION_RESTORED`. Branch-(a) call site now passes
  `None` and constructs `BranchOutcome::BlockedLedgerWithoutNodeState` itself.
- `src/services/reconciliation/reservation_boot_pass.rs` — `deferred: BTreeMap<String, ReservationId>`;
  step 3 = `reconstruct_lost_node_state(pool, fscl, Some(*res_id))`; step 4 = re-normalize deferred
  CALL_STARTED only (NO auto-apply); `normalize_one` → `complete_crashed_trace` (Fix B).
- `src/db/repositories/delivery_reservation.rs` — the §3.1 fence (≤1 active reservation per FN);
  `apply_outcome` gen/pointer CAS (~852); `complete_operator_pending` gen/pointer CAS (~1286) +
  `STOP_MODE → target` mode-CAS.
- `src/db/repositories/transport_trace.rs` + `boot_phase::close_orphan_transport_traces` (~1607, the
  `SYSTEM_CRASH` close pattern being mirrored); `ix_transport_trace_unfinished` is a NON-unique index.
- Teeth: `tests/s7_boot_reservation_pass.rs` — `s7_nc03_authority_restore_enables_operator_completion`
  (CALL_STARTED path), `s7_nc03_authority_restore_oo_pending_deferred_path` (OO+PENDING path),
  `s7_normalize_completes_crashed_transport_trace` (§7.1) — all empirically revert-canaried.

---

## 2. Refute each fix (build a crash/restart sequence if you can)

- **A-SOUND: restore is fork-free.** Restoring `delivery_generation = R.authorized_generation` + pointer
  = R is safe because the §3.1 fence guarantees ≤1 active reservation per FN AND node_state was lost (no
  other survivor). *Can you construct two active reservations for one FN at boot (fence breach), or a
  restore that mis-authorizes / leaves node_state internally inconsistent? Is reconstruction+restore truly
  atomic (crash-between → rolls back → re-defers next boot)?*
- **A-NO-DOUBLE-ISSUE across restart.** *Build a sequence where the restored authority lets a seed advance
  twice / a doc issue twice: e.g. operator completes R, then a later boot re-applies; or the restored
  pointer lets step-2 apply AND `complete_operator_pending` both fire.* (Known & documented: an OO+PENDING
  **Accepted**-evidence NC-03 reservation auto-applies on the SECOND boot instead of via the operator —
  fiscally correct, node stays STOP, operator uses `reset_stop_mode`. Confirm this is at worst that MINOR
  and never a double-issue/fork.)
- **A-NOT-AUTO-APPLY consistency.** Step 4 skips auto-apply ONLY for NC-03-deferred FNs; the NORMAL
  OO+PENDING path (step 2) still auto-applies per §7.2. *Confirm the normal crash-window matrix is intact.*
- **B-TRACE correctness.** *Can a crashed CALL_STARTED doc have MORE than one open trace, making the
  `LIMIT 1` close leave one open?* (index is non-unique; the argument is the single-writer ADR-M3-A10 +
  the 4-b closing attempt-1 before mac_recovery — verify it holds.) Is `SYSTEM_CRASH` (not
  `complete_via_recovery_tx`'s `Ok`) the right outcome? Are all DDL-required completion columns set? Is it
  idempotent + a benign no-op when there is no open trace?
- **NEUTRALITY.** Is branch-(a) still byte-behaviour-identical (BLOCKED, null pointer, same audit
  payload) — the existing NC-03 tests must still pin it? Does the fix change any NORMAL
  (empty-reservation) boot?

---

## 3. Known residuals (escalate if they are actually worse than MINOR)

Our own decorrelated re-audit returned **6 MINOR, 0 SOUNDNESS/0 BRICK**. The two substantive ones:
1. **Second-boot auto-apply** of an OO+PENDING-deferred **auto-release** outcome (above) — documented, no
   guard. Is it ever a fork/double-issue, or does it strand the doc in a frozen-invariant-violating state?
2. **Trace `LIMIT 1`** relies on single-writer (no DDL backstop). Is there a real code path (not test
   scaffolding) that produces two open traces for one crashed doc?

---

## 4. Verdict required

**GO** (fixes sound, land) / **FIX_FOLLOWUP** (bounded fixes) / **SYSTEMIC** (class defect — name it +
repro). For each finding: severity, `file:line`, a concrete trace, the smallest fix. Note which frozen
§7.1/§7.2 decisions you are NOT contesting. This is INACTIVE machinery (empty pre-cutover) — weight
findings by their post-cutover behaviour.
