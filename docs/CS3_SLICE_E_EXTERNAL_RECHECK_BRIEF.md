# CS-3 Slice E — External Re-check Brief (rev 3)

**This is a SHORT re-check, not a full audit.** The plan has already been through two audit passes; your
job is to falsify the soundness of the **rev-3 fixes and the two design decisions**, not to re-derive the
whole design. Prefer a few grounded, falsifiable findings over breadth.

- **Plan under review:** `docs/CS3_SLICE_E_PLAN_AND_AUDIT_BRIEF.md` (rev 3). Read it first.
- **Base:** `main @ 224ad46` (post S7-1 double-issue cutover). Code under `rust/`. Ground every claim
  against real code (open the files) — do not trust the plan's or this brief's file:line without checking.
- **Rules:** read-only; no `git checkout`/mutation of the shared worktree.

## History (so you don't redo it)
- rev 1 → external NOT-YET (4 class-B + 5 class-A) → rev 2.
- rev 2 → internal decorrelated audit (5 lenses, 27 findings → 20 confirmed / 8 majors / **0 blockers /
  0 durable-soundness holes**) → rev 3. Verdict was characterised as "a good NOT-YET: oracle / migration
  / projection desync, not a P2/P3/HELD hole."
- rev 3 folds all 8 majors + the minors + the two architect decisions (unified `CloseShiftProbe`; narrow
  fuzzer alphabet).

## What already HELD — do NOT re-litigate (invert only if you have new grounding)
1. **The core routing change is sound.** `UnknownStatus → ProbeRequired` creates no double-issue/resend:
   HOLD/STOP is keyed on `certainty==SubmittedUnknown` (`delivery_reservation.rs:727`) + the
   `_ => HeldNotAutoRelease` auto-release arm (`:967`) — **neither keyed on `routing_class`/`node_effect`**
   → the reservation state machine does not move. ("Not a state-machine change" survives at the STATE
   level; the plan now also enumerates the durable-COLUMN + audit-label changes.)
2. **Legacy demote, don't rewrite** is coherent: legacy `route_dps_error` keeps `-4 → TransientRetry`, and
   `apply_plan_pin` `routed(-4)=transient_retry()` (`:208`) stays correctly GREEN as a legacy/forensics
   pin. The prior audit tried to invert this and failed — do not invert it without new evidence.
3. **Backward-compat posture** ("accept the existing `OO/TransientRetry` combo unchanged") is the only
   viable one: the immutable trigger freezes `routing_class` once `OLD.state='OUTCOME_OBSERVED'`
   (`032:184`) — a legacy row can never be rewritten, and the plan requires accepting it unchanged.

## The 7 focused axes (falsify these)
Each is a concrete target with the plan's grounding — verify or break it.

1. **Track-A totality (§4 step 1, §1.1).** `ClassifiedOutcome` carries only `{certainty, provenance,
   routing, node_effect}` (`mod.rs:863`). Does `wire_decision_from(disc, &classified)` reconstruct EVERY
   observable field — including the happy-path `Sent{server_fiscal_no}` (the plan makes the return a
   `WireDecision` sum, F8), `audit_event`, `probe_hint`, `audit_severity`? Find any field that still
   REQUIRES legacy `route_dps_error` and cannot come from `classified` (that breaks "legacy = forensics
   only"). Confirm a pin now locks the FULL `wire_decision_from` tuple (rev-2 had none — the central
   Track-A change was unguarded).
2. **Migration-038 predicate (§5).** The trigger is BEFORE UPDATE on `NEW.state='OUTCOME_OBSERVED'`
   (`036:117`); INSERTs never reach OO (`insert_state` → `RESERVED_NOT_STARTED`, `032:138`). Is the
   plan's `OLD.state` branch (`CALL_STARTED → require ProbeRequired+ProbeRequired`; `OUTCOME_OBSERVED →
   accept legacy TransientRetry+NoNodeEffect`) actually expressible and SOUND as a SQLite trigger, and
   does the **apply-time re-validation** (`delivery_reservation.rs:971`, reached via
   `reservation_boot_pass.rs:225`) pass for a legacy row? Is the backward-compat tooth driven through
   `complete_operator_pending` (the UPDATE that re-fires the matrix), NOT boot-resume (which
   short-circuits to Held)?
3. **`node_effect` coupling (§2/§5/§7).** `node_effect_for_active(ProbeRequired)=ProbeRequired`
   (`mod.rs:1046`), persisted (`delivery_reservation.rs:642/661`). Does 038 pin BOTH `routing_class` AND
   `node_effect` on the fresh arm? Any other durable consumer of `node_effect` for `UnknownStatus` that
   the flip disturbs?
4. **Classifier↔matrix simultaneity (§4 step 3).** `routing_for_indeterminate(UnknownStatus)→ProbeRequired`
   and migration-038 MUST land together or `record_outcome` hits the old matrix and rolls back. Are the
   two conformance tests (`cs3_evidence_matrix_conformance.rs:168/186`,
   `cs3_c_db_classifier_storage_roundtrip.rs:666/899`) updated in the same change and bound to the 038
   GREEN gate?
5. **Drift-pin inversion (§4 step 4, §8).** `grpc.rs` `pin_d_section_4_6_drift_pin` (`:598`) / `drift_check`
   (`:700–788`): after Pin 2, is `-4` a delta (Live `TransientRetry` / Shadow `ProbeRequired`) and `-17`
   an equal_row (both `ProbeRequired`), with "3 deltas"→2 everywhere (`grpc.rs:580/759`,
   `apply_plan_pin.rs:8`, `error_routing.rs:319`)? Any residual `-4`/`-17` assertion the plan misses?
6. **`CloseShiftProbe` merge (Decision 1, §3.2).** `-2|-15` collapse to `CloseAmbiguous{digest}` with the
   code discarded (`mod.rs:617`, `evidence.rs:352`). Is merging `Code2/Code15CloseShift → CloseShiftProbe`
   the ONLY observable change (digest + transport diagnostics intact), with **zero second wire** for both,
   and no safety property lost? Are `apply_plan_pin.rs:227/232` + the `error_routing.rs:853/934` fixtures
   the complete set that must change?
7. **Fuzzer narrowness (Decision 2, §6).** `WireResponse::UnknownStatus(i32)` with `-4` AND `-17`, mapped
   through the REAL production classifier; properties (one wire / fence held / no auto-resend /
   `→ProbeRequired`) + a revert-canary (restore `TransientRetry` → RED). Is this genuinely narrow (no
   sprawling model change), and does the model observe `certainty/routing_class/node_effect` (today it
   collapses HELD outcomes to `doc_state=SENDING` only)?

## Explicitly deferred (out of Slice E — do not require)
Auto-confirm via `last_chk` (no correlation key: `-4 → ServerCode{code,digest}`, no `fiscal_id`);
`NotFound→RMR` (unreachable for the SENDING-held family); `SaveError(-3)` (needs a 2-row migration);
the `last_chk_probe.rs:19` doc-fix (disjoint SENT-recovery path).

## Verdict requested
`GO` / `GO-WITH-FIXES` / `NOT-YET`, judged ONLY on the soundness of the rev-3 fixes + the two decisions
(not scope preference). Ground each finding with a real file:line and a falsifiable argument. If you
confirm the 7 axes hold, that is a GO — the implementer proceeds in the §4 order (Track-A tuple → 038 +
compat → classifier+conformance together → drift-pin → CloseShiftProbe → fuzzer → full gate).
