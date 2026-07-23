# CS-3 Slice E — Plan & External-Audit Brief (rev 4)

**Status:** design / spec-first, rev 4. Base = `main @ 224ad46`. Verify all file:line before implementing.

**rev-4 changelog** (rev-3 got NOT-YET from an internal decorrelated spot-check + an external re-check —
converging: "core sound, a short rev4, not a new design". Four blocking plan fixes + one minor, folded):
- **Fix 1 — Track A is not yet a total projection.** `wire_decision_from` cannot rebuild the full tuple
  from `ClassifiedOutcome` alone (4 axes, no message/SFN, `mod.rs:863`): `BadHashPrev` needs
  `mac_recovery_hint.raw_error_message` (`error_routing.rs:571`), not in the discriminant; the new
  `UnknownStatus→ProbeRequired` has **no defined `ProbeReason`**; and `WrapperBug` (Critical diagnostic,
  `error_routing.rs:431`) collapses into `NoResponse` in evidence — the pure classifier cannot rebuild
  it. → §3.1 draws a hard boundary + a full leaf→tuple table.
- **Fix 2 — drift-delta miscount.** Post-Slice-E the count STAYS 3, not 2 (`-17`→equal, `-4`→new delta,
  empty-id + TLS stay). → §4-step-4 / §8 corrected.
- **Fix 3 — fuzzer teeth would be green-but-unsound.** `WireResponse` has no unknown-status
  (`op.rs:23`); `wire_to_result` returns an already-collapsed `Result<CheckAck,DpsError>`
  (`interp.rs:2206`); the faithful adapter cannot rebuild `Indeterminate(-4)` and turns `Decode` into
  status-0/`MissingStatus`, not `-17` (`dto.rs:395`). A `WireResponse::UnknownStatus(i32)` op would NOT
  exercise the leaf. → §6 replaces it with a narrow test-support path through the REAL production decode.
- **Fix 4 — migration order + blast radius must be unambiguous.** 038 cannot ship before the classifier
  (the current classifier would then take SQL rejects); it is ONE atomic change. A real upgrade-test is
  required. The blast radius omitted the normative `rp4b_2_classify_graph_pin.rs:319` (pins
  `-4→TransientRetry/NoNodeEffect`). → §4-step-2/3 merged, §5 + §8 corrected.
- **Minor** — the CloseShiftProbe blast set must include the INTEGRATION fixtures
  (`write_path_dps_error_routing.rs`), not just the `error_routing.rs` unit tests. → §8.

**What held both audits (do NOT re-litigate):** `UnknownStatus→ProbeRequired` opens no resend
(HOLD/STOP keyed on `certainty==SubmittedUnknown`, apply `HeldNotAutoRelease`,
`delivery_reservation.rs:727/967`); P2/P3 + state machine unchanged; `OLD.state` is the right 038
discriminator; simultaneous classifier+matrix cutover is right; CloseShiftProbe merge is safe (code
already sealed away, identical behaviour); `node_effect NoNodeEffect→ProbeRequired` flip is correctly
accounted. Baseline pins verified live: `rp4b_2_classify_graph_pin` 9/9, `cs3_evidence_matrix_
conformance` 1/1, `pin_d_section_4_6_drift_pin` 1/1 — they hold the OLD semantics and must red at Slice E.

---

## 0. Scope & tracks

- **Track A — single routing authority.** Routing/state/node-mode come from the classifier + the sealed
  `EvidenceDiscriminant` ONLY; the existing `WireDiagnostics` (raw obs) is used EXCLUSIVELY for the
  message, DPS status-code, MAC-recovery hint, and the diagnostic audit-overlay. Delete
  `project_decision_from_evidence` (3 prod call sites: `stage_send.rs:1807/1914/2010`); demote
  `route_dps_error` to that diagnostic overlay; add a pin over the NEW authority tuple.
- **Track B — `UnknownStatus` family → ProbeRequired** (`-4`, `-17`, `-99`, …): flips `routing_class`
  AND `node_effect` (→ProbeRequired) + observable `audit_event`/`transport_trace.retry_class`; doc STATE
  unchanged (already HELD). Needs **migration 038** (§5), atomic with the classifier flip.
- **CloseShiftProbe** unification (§3.2); **narrow directed fuzzer teeth** (§6).
- **Deferred → Future:** auto-confirm; `NotFound→RMR` (unreachable for SENDING-held); `SaveError(-3)`;
  `last_chk_probe.rs:19` doc.

---

## 3.1 Track A — the authority/diagnostic boundary + leaf→tuple table (Fix 1)

`ClassifiedOutcome` (`mod.rs:863`) has four axes: `{certainty, provenance, routing:Option<ActiveRetryClass>,
node_effect}`. The `EvidenceDiscriminant` (`evidence.rs:126`) is the sealed leaf, carrying `fiscal_id`
for `Accepted` (`:234`) and a digest per indeterminate leaf, but **NOT** raw messages. So:

**HARD boundary — `wire_decision_from(disc, &classified, &diag) -> WireDecision`:**
- **From `classified` + `disc` (the routing authority):** `target_state`, `retry_class`, `node_mode_flip`,
  `probe_hint.reason`, and the happy `Sent{server_fiscal_no}` (SFN from `disc::Accepted`). A
  `WireDecision` sum (`Sent{sfn} | Routed(RoutingDecision)`) is necessary + sufficient for the SFN.
- **From `diag: &WireDiagnostics` (raw obs) — diagnostic overlay ONLY, never routing:** `error_message`,
  DPS `server_status_code=Some(code)`, `mac_recovery_hint.raw_error_message` (the `-12`/`BadHashPrev`
  message, `error_routing.rs:571` — absent from `disc`), the `audit_severity` **overlay** for the
  `WrapperBug` Critical diagnostic (`error_routing.rs:431`; evidence collapses wrapper errors into
  `NoResponse`, so Critical-ness cannot be rebuilt from the classifier and must ride the overlay).

**New `ProbeReason`:** add a variant for the `UnknownStatus`→ProbeRequired leaf (no existing reason fits;
`RemoteStatus`/`Code*CloseShift`/`DecodeUnknown`/`OkButNoFiscalNumber` are all other leaves). Call it
`ProbeReason::SubmittedUnknown` (or `ServerStatusUnknown`).

**Deliverable:** an explicit **leaf → (target_state, retry_class, node_mode_flip, audit_event,
audit_severity, probe_reason, source=classifier|diag)** table in Pin 0 covering every
`EvidenceDiscriminant` variant, so a new leaf forces an exhaustive-match update.

### 3.2 CloseShiftProbe unification (safe merge)
`-2|-15 → CloseAmbiguous{digest}`, code discarded (`mod.rs:617`, `evidence.rs:247`); both already
ProbeRequired/HELD/no-second-wire (`error_routing.rs:500/590`); no downstream distinguishes them.
Replace `Code2CloseShift`/`Code15CloseShift` with one `ProbeReason::CloseShiftProbe`; keep digest +
transport diagnostics; declare the audit-label merge an accepted observable change. **This is a
PREREQUISITE of the total projection** (the projection cannot assign a per-code reason for the merged
leaf) — land it before/with Track-A Pin 1 (§4).

---

## 4. rev-4 pin sequence

1. **CloseShiftProbe unification** (§3.2) — prerequisite for the total projection.
2. **Track A total projection** (§3.1): add `wire_decision_from(disc,&classified,&diag)` + the leaf→tuple
   table + the new `ProbeReason`; switch all THREE call sites (`stage_send.rs:1807/1914/2010`); delete
   `project_decision_from_evidence`; demote `route_dps_error` to the diagnostic overlay; **add a pin
   locking the full `wire_decision_from` tuple** (closes the unguarded-central-change hole).
3. **ONE atomic change** — classifier flip + migration 038 + all matrix/graph pins together:
   `routing_for_indeterminate(UnknownStatus)→ProbeRequired`; migration 038 (§5); update
   `cs3_evidence_matrix_conformance.rs`, `cs3_c_db_classifier_storage_roundtrip.rs`, and the normative
   `rp4b_2_classify_graph_pin.rs:319` (pins `-4→TransientRetry/NoNodeEffect`). **038 must NOT ship
   separately** (the un-flipped classifier would then take SQL rejects).
4. **Drift-pin update** (`grpc.rs`, Fix 2): `-17`→equal_rows (both ProbeRequired); `-4`→a NEW delta
   (Live `TransientRetry` / Shadow `ProbeRequired`); empty-id + TLS RemoteStatus stay deltas. **The count
   stays THREE** — keep "3 declared deltas" (`apply_plan_pin.rs:8`, `grpc.rs:759/760/771/789`); Delta 2's
   content changes `-17`→`-4` (direction flips). RED-first canary.
5. **Narrow directed fuzzer teeth** (§6).
6. **Full gate + short external re-check.**

Forensics `error_kind` distinctness + `server_status_code` preservation ride Pin 2's diagnostic overlay;
no `error_kind` migration (plain TEXT).

---

## 5. Migration 038 (corrected — atomic, `OLD.state`, real upgrade-test)

Shipped `036` immutable; add `038_*.sql` replacing `delivery_reservation_evidence_matrix_update`, for the
`UnknownStatus` arm:
- `OLD.state='CALL_STARTED'` (fresh CS→OO record) ⇒ require `routing_class='ProbeRequired' AND
  node_effect='ProbeRequired'`.
- `OLD.state='OUTCOME_OBSERVED'` (apply-time re-validation) ⇒ ALSO accept the legacy
  `(TransientRetry, NoNodeEffect)` combo unchanged.
- No fresh transition may write the legacy combo. (INSERT never reaches OO — `insert_state`→
  `RESERVED_NOT_STARTED`, `032:138` — so the discriminator is `OLD.state`, not INSERT-vs-UPDATE.)
- The immutable freeze trigger (`032:184`, fires only on `OLD.state=OO`) does NOT block the fresh
  ProbeRequired write.

**Atomicity:** 038 ships in the SAME change as the classifier flip (§4 step 3) — never before.

**Real upgrade-test (backward-compat):** apply migrations up to 037; write a legacy
`OO/UnknownStatus/TransientRetry/NoNodeEffect` row; apply 038; drive the row to a terminal state via the
operator path (`complete_operator_pending`, `operator_completion.rs`). NB: for a `SubmittedUnknown` leaf,
`complete_operator_pending` currently short-circuits at `HeldNotAutoRelease` (`delivery_reservation.rs:967`)
BEFORE the `apply_state='APPLIED'` UPDATE (`:970`) — so the test must drive the operator's TERMINAL
resolution (the UPDATE that actually re-fires the matrix on the OO row), or, if no such path exists, be a
direct trigger-level SQL test (seed legacy OO row → OO-preserving UPDATE → assert 038 does not abort) and
the lenient arm is then documented as defensive. Ground the exact operator UPDATE before writing the test.

---

## 6. Narrow directed teeth (Fix 3 — real decode, not a fuzzer op)

Adding `WireResponse::UnknownStatus(i32)` does NOT work: `wire_to_result` (`interp.rs:2206`) hands the
model an already-collapsed `Result<CheckAck,DpsError>`, and the faithful adapter cannot rebuild
`Indeterminate(-4)` (and turns `Decode`→status-0/`MissingStatus`, not `-17`, `dto.rs:395`). Instead, a
**narrow test-support path** (do NOT expand `Mutation`):
- construct a REAL `gen::CheckResponse{ status: -4 }` and `{ status: -17 }`;
- run each through the PRODUCTION `observe_check_reply` / classify path (no mock);
- assert: reservation axes (`certainty=SubmittedUnknown`, `routing_class=ProbeRequired`,
  `node_effect=ProbeRequired`), STOP/fence held, **exactly one wire**, no auto-resend;
- revert-canary: restore `TransientRetry` → the test reds.

Directed unit tests are the precise teeth; the invariant fuzzer stays the compositional harness (crash /
boot / next-doc / repeat-tick) — the `UnknownStatus` leaf is validated by the directed path above, not a
new fuzzer op-alphabet.

---

## 7. Invariants
1. **P2** — one wire, held; no probe/resend. 2. **P3** — untouched. 3. **INV-1** — no net in write-tx.
4. **Durable STATE unchanged** (HELD on `certainty`); durable COLUMNS (`routing_class`, `node_effect`) +
   observable labels (`audit_event`, `transport_trace.retry_class`, merged `CloseShiftProbe`) change.
5. **No double-issue.** 6. **CS-1** — re-anchor + re-mint; supersession-register renames; immutable leg
   untouched.

---

## 8. Blast radius (corrected)

**Atomic with the classifier flip (§4 step 3):** migration 038; `cs3_evidence_matrix_conformance.rs`
(`:168/186`); `cs3_c_db_classifier_storage_roundtrip.rs` (`:666/899`); **`rp4b_2_classify_graph_pin.rs:319`**
(normative `-4→TransientRetry/NoNodeEffect` graph-pin — was missing).

**Track A / drift (§4 step 2 & 4):** `grpc.rs` `pin_d_section_4_6_drift_pin` (`:598`) + `drift_check`
(`:700–789`, `-4`→delta / `-17`→equal, **still 3 deltas**); `apply_plan_pin.rs:8` comment; the F3 pins
`stage_send.rs:1867/1963` (removed with the bridge or re-pinned on `wire_decision_from`);
`extract_wire_forensics_..._to_transport` (`:2520`); a NEW directed `-17` persisted-trace test.

**CloseShiftProbe (10 sites, Fix 5 minor):** `error_routing.rs:508/602` (constructors), `:1204/1208`
(inline), `:853/934` (fixtures); `apply_plan_pin.rs:227/232`; **integration fixtures
`write_path_dps_error_routing.rs:460/470/638/647`** (2 struct + 2 JSON-string asserts).

**Stays GREEN (do NOT re-mint):** `apply_plan_pin.rs` `routed(-4)=transient_retry()` (`:208`, legacy
forensics); `backlog_drain_types.rs` (stable-string taxonomy).

---

## 9. External re-check axes (rev 4)
1. Leaf→tuple table (§3.1) is exhaustive; every field sourced classifier-vs-diag correctly; new
   `ProbeReason` present; `WrapperBug` Critical rides the overlay.
2. 038 + classifier + matrix + graph pins land as ONE atomic change; the upgrade-test drives a real
   matrix re-fire (or the lenient arm is proven defensive).
3. Drift-pin: `-4` delta / `-17` equal, count STILL 3.
4. Fuzzer teeth go through the real `observe_check_reply` on a `-4`/`-17` `CheckResponse`; revert-canary reds.
5. CloseShiftProbe: all 10 sites; zero second wire; no downstream `-2`/`-15` distinction lost.
6. The new `wire_decision_from` tuple is pinned; 3 call sites switched.

## 10. Deferred → Future (not Slice E)
Auto-confirm via `last_chk` (no correlation key); `NotFound→RMR` (unreachable for SENDING-held);
`SaveError(-3)` (2-row migration); `last_chk_probe.rs:19` doc (disjoint SENT-recovery path).
