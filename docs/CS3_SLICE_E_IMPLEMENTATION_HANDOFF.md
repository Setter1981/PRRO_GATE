# CS-3 Slice E — Implementation Handoff

Pick-up point for continuing the Slice E implementation (Pins 2–6). Pin 1 is landed.

## Where
- **Worktree:** `/home/setter/prro-gate-wt/cs3-de-slice2`
- **Branch:** `cs3-slice-e` @ `a76563f` (off `main` `224ad46`, the merged S7-1 cutover). UNPUSHED.
- **Stack:** `a76563f` (Pin 1) → `5a5a061` (plan/design docs) → `224ad46` (main).
- **cargo on PATH:** `export PATH="$HOME/.cargo/bin:$PATH"` (non-interactive shell lacks it).

## The load-bearing spec (READ FIRST)
- `docs/CS3_SLICE_E_PLAN_AND_AUDIT_BRIEF.md` (**rev 4**) — the implementation oracle. Every pin, the
  leaf→tuple boundary (§3.1), the CloseShiftProbe merge (§3.2, DONE), migration-038 (§5), the directed
  teeth (§6), blast radius (§8). All file:line anchors are against `224ad46` — **re-verify before editing.**
- `docs/CS3_SLICE_E_EXTERNAL_RECHECK_BRIEF.md` — the external handoff (attack axes + What-Held).

## Audit status
rev4 passed convergent review: an **internal decorrelated audit** (5 lenses → verify → synth) and an
**external cross-model re-check** both returned "core sound, a short rev4, not a new design." All 8 majors
+ minors are folded into rev4. **Do NOT re-litigate the What-Held** (rev4 changelog + §1): no
resend/double-issue; P2/P3 + state machine unchanged; `OLD.state` is the right 038 discriminator; atomic
classifier+matrix cutover is right; CloseShiftProbe safe; `node_effect` flip accounted.

## Done — Pin 1 (CloseShiftProbe unification) `a76563f`
`Code2CloseShift`/`Code15CloseShift` → one `ProbeReason::CloseShiftProbe` (10 sites, 0 residual).
RED-first `slice_e_close_shift_probe_unifies_minus_2_and_minus_15` (rename-agnostic) + revert-canary
verified. This is the PREREQUISITE for Pin 2's total projection (the merged reason is derivable from the
single `CloseAmbiguous` disc leaf; `-2`/`-15` were not).

## Remaining pins (rev4 §4 order)
2. **Track A total projection** (§3.1) — the design-heavy one. Add
   `wire_decision_from(disc, &classified, &diag) -> WireDecision` as a TOTAL function over
   `EvidenceDiscriminant`; write the explicit **leaf → (target_state, retry_class, node_mode_flip,
   audit_event, audit_severity, probe_reason, source=classifier|diag)** table. Boundary: routing/state/
   node-mode from `classified`+`disc` ONLY; message / DPS status-code / `mac_recovery_hint.raw_error_
   message` (the `-12` message, absent from disc — `error_routing.rs:571`) / the `WrapperBug` Critical
   audit-severity overlay (`error_routing.rs:431`, evidence collapses wrapper→NoResponse) from
   `WireDiagnostics` (raw). Add a new `ProbeReason::SubmittedUnknown` for the UnknownStatus leaf. Switch
   **all THREE** call sites `stage_send.rs:1807/1914/2010`; delete `project_decision_from_evidence`;
   demote `route_dps_error` to the diagnostic overlay; **add a pin locking the full `wire_decision_from`
   tuple** (none exists today — the central change is otherwise unguarded).
   - `disc` DOES carry `fiscal_id` for `Accepted` (`evidence.rs:234`) → the `WireDecision` sum
     (`Sent{sfn}|Routed`) threads the SFN. `disc` variant tags → target_state/audit_event/severity are
     derivable by a mapping. `ActiveRetryClass` is a SUBSET of legacy `RetryClass` (no TerminalReject/
     MacRecovery) — the map is `(disc-variant, ActiveRetryClass) → RoutingDecision`.
3. **ATOMIC classifier + migration 038 + all pins** (§5, §8). `routing_for_indeterminate(UnknownStatus)
   → ProbeRequired` (fixes `-4` AND `-17/-99` — one arm, `mod.rs:1024`). Flips persisted `routing_class`
   AND `node_effect` (→ProbeRequired). Migration **038** (never ships before the classifier — the
   un-flipped classifier would then take SQL rejects): replace trigger
   `delivery_reservation_evidence_matrix_update`; `OLD.state='CALL_STARTED'⇒require (ProbeRequired,
   ProbeRequired)`; `OLD.state='OUTCOME_OBSERVED'⇒also accept legacy (TransientRetry, NoNodeEffect)`.
   Update in the SAME change: `cs3_evidence_matrix_conformance.rs`, `cs3_c_db_classifier_storage_
   roundtrip.rs`, and the normative `rp4b_2_classify_graph_pin.rs:319`. **Real upgrade-test:** migrate to
   037, write a legacy OO/UnknownStatus/TransientRetry/NoNodeEffect row, apply 038, drive it terminal via
   the operator path — NB `complete_operator_pending` short-circuits at `HeldNotAutoRelease`
   (`delivery_reservation.rs:967`) BEFORE the APPLIED UPDATE (`:970`), so drive the operator's TERMINAL
   resolution (the UPDATE that re-fires the matrix on the OO row) or make it a direct trigger-SQL test and
   document the lenient arm as defensive. Ground the exact operator UPDATE first.
4. **Drift-pin** (`grpc.rs`, §4-step-4). After Pin 3: `-17`→equal_rows, `-4`→a NEW delta (Live
   `TransientRetry`/Shadow `ProbeRequired`), empty-id + TLS stay. **Count STAYS 3** — keep "3 declared
   deltas" (`apply_plan_pin.rs:8`, `grpc.rs:759/760/771/789`); Delta 2 content `-17`→`-4`. RED-first canary.
5. **Directed teeth** (§6). NOT a `WireResponse::UnknownStatus` fuzzer op (`wire_to_result` collapses,
   `interp.rs:2206`; faithful adapter can't rebuild `-4`, `dto.rs:395`). A narrow test-support path: build
   a REAL `gen::CheckResponse{status:-4/-17}`, run it through production `observe_check_reply`/classify,
   assert `certainty=SubmittedUnknown`/`routing_class=ProbeRequired`/`node_effect=ProbeRequired` + STOP/
   fence + exactly-one-wire; revert-canary (restore TransientRetry → RED).
6. **Full gate + CS-1 re-anchor + short external re-check.**

## CS-1 provenance (batched at slice tip — do at Pin 6, not per-pin)
Pins 1/3/5 touch FROZEN test files (`write_path_dps_error_routing.rs`, `cs3_*`, `rp4b_2_*`,
`invariant_fuzzer/*`) → the live-drift leg (`cs1_test_provenance`) reds. At the slice tip:
1. `Edit` (NOT sed — `&str` in a sed replacement means "whole match", it WILL corrupt the line)
   `LIVE_DRIFT_BASE_SHA` in `rust/prro/tests/support/cs1_provenance.rs` → the slice-tip commit SHA.
2. `bash scripts/cs1r/mint_manifests.sh` — **AFTER `cargo fmt`** (re-mint before fmt = stale sha, a real
   CI-red we hit this session).
3. Verify: `bash scripts/cs1r/inventory_gate.sh --pr origin/main` PASSED; `cs1_test_provenance` 6/6
   (`cs1_live_drift_base_vs_worktree` green, `ast_ok` count updated for any added frozen-file tests).
4. New tests added to frozen files are fine (the re-anchor covers them); the immutable
   `f2c17b1..f2628ba` leg must stay UNTOUCHED.

## Per-pin discipline (non-negotiable)
- **RED-first + revert-canary that BITES** — write the failing test, watch it RED, implement, watch GREEN,
  then revert the fix and watch it RED again (teeth must be proven empirically, not asserted).
- **Ground every anchor** before editing (`git grep`/Read the real definition; the plan's line numbers may
  drift). Zero "by memory".
- Minimal diff; run targeted tests per pin; summarize state-machine impact.

## Pre-push CI gate (GREEN nextest ≠ green CI)
`cargo fmt -p prro … -- --check` + `cargo clippy -p prro --all-targets --no-deps --features test-support
--locked -- -D warnings` + inventory re-mint (AFTER fmt) + `cargo nextest run -p prro --features
test-support --locked` + the live-dps compile-only (`cargo test -p prro --features live-dps --test
live_dps_extended_smoke --no-run`). The x86_64 CI job also runs the additions-only inventory gate +
`cs1_test_provenance` + the fuzzer.

## Session hazards learned (avoid re-hitting)
- `LIVE_DRIFT_BASE_SHA` update via `Edit`, never `sed` (`&` bug).
- Re-mint the inventory AFTER `cargo fmt`.
- Force-push is guardrail-blocked here → fast-forward pushes only (or the operator pushes).
- Test files under `--features test-support` compile with `cfg(test)`; the live-dps smoke harness compiles
  with only `--features live-dps` — a lib helper it uses must be gated to include `live-dps`.
