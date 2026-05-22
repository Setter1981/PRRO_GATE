# M3b W12 — In-Drain KVT2 Confirmation via `lastChk`

**Status:** OPEN
**Date:** 2026-05-22
**Umbrella plan anchor:** `docs/superpowers/plans/2026-05-14-m3b-implementation.md` §Task 12.
**W0b verdict anchor:** `docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md` — ACCEPTED 2026-05-14, **YES with explicit scope restriction**.
**Predecessor:** W9b (PR #68, `rust-gateway` `09196f1`) + W9b ER-class-guard (PR #69, `rust-gateway` `4a12c2f`).
**Pilot impact:** **Pilot-gating** if pilot acceptance requires real offline backlog closure to final DPS `Ack` (Phase 6 of `docs/PILOT_ACCEPTANCE_TEST_PLAN.md`).

---

## Goal

Replace the W9b pre-W12 `apply_w12_confirmation` stub (`backlog_drain.rs:1470`, always returns `W12ConfirmOutcome::DeferredKvt1`) with a real in-drain KVT2 confirmation helper that:

1. Reads the just-recorded `doc.server_fiscal_no` (from `stage_send::run` 4-b OR from W9b `lastChk` replay short-circuit).
2. Issues `lastChk(fn_sign)` immediately after `stage_send(doc_i)` and **before** the drain advances to `doc_{i+1}` on the same FN (W0b interleave precondition).
3. Validates evidence per W0b §Verdict: `response.status == OK` + `response.id == doc.server_fiscal_no` + non-empty `response.data_sign`.
4. **On success (two-envelope ladder; see §"Transaction envelope shape")**:
   - **Envelope 1 (W12-owned)**: persist `kvt1_raw_bytes` + CAS `Kvt1 → Kvt2` atomically.
   - **Envelope 2 (existing `stage_finalize::run`-owned)**: CAS `Kvt2 → Ack` + chain seed advance + inbox DONE + outbox + audit, atomically.
5. **On failure (W0b §Failure Semantics conformance)**: doc state **unchanged**, replayable per W0b §95-102.  Typed `Kvt2ConfirmFailure` + `KVT2_CONFIRM_FAILED` audit emitted; doc stays in `Kvt1` for next-tick re-drive (within drain) or for `passive_hold_kvt1` (post-drain boot recovery).  **No manual-recon escalation on evidence-failure classes** — only structural-invariant breaches (e.g. `CasMissOnAdvance` indicating concurrent writer) surface as `BootError::Internal`.

Unblock W9b drain finalization **Eligible arm**: `DrainSummary::finalize_eligibility` currently always returns `NotEligible { reason: DocsDeferredAtKvt1 }` because the stub always reports `DeferredKvt1`. After W12, real `Acked` outcomes route through `DrainSummary::record_doc_advanced` → `advanced_to_ack += 1`, and zero deferred-at-KVT1 docs unlock the `Eligible` arm → `OFFLINE_DRAIN_COMPLETED` audit + node mode `GoingOnline → Online` + session `Draining → Closed`.

---

## Transaction envelope shape (MED-PR70-01 resolution, 2026-05-22)

**Chosen form: two-envelope ladder** (NOT tx-local refactor of `stage_finalize`).

**Rationale.**  `stage_finalize::run(pool, doc) -> Result<...>` (at `rust/prro/src/services/write_path/stage_finalize.rs:234`) is a load-bearing M3a contract.  It owns its own `with_immediate` envelope spanning the 5-write atomicity unit (CAS `Kvt2 → Ack` + chain-seed advance + inbox DONE + outbox row + `STAGE_FINALIZE_ACK` audit per W8 review F1 close).  Refactoring this into a tx-local variant would expose chain-seed / inbox / outbox manipulation across module boundaries and require a separate audit of all downstream consumers — **out of W12 scope**.

**Two-envelope sequence:**

```
[Envelope 1: W12-owned, inside backlog_drain::apply_w12_confirmation]
  with_immediate(pool, |tx| async {
    1. UPDATE fiscal_documents.kvt1_raw = ? WHERE document_id = ?
       (HIGH-C5-2 contract: byte-for-byte data_sign persist).
    2. transition_state(tx, doc_id, Kvt1, Kvt2)
       (W1 service-layer helper; whitelisted edge).
    3. audit_log::append_tx(tx, "OFFLINE_DRAIN_KVT2_ADVANCED", payload)
       (W12 evidence trail).
  })

[Envelope 2: stage_finalize::run-owned, called from drain after envelope 1 commits]
  stage_finalize::run(pool, doc_id).await
    → with_immediate(pool, |tx| async {
        CAS Kvt2 → Ack + seed + inbox + outbox + STAGE_FINALIZE_ACK.
      })
```

**Crash-recovery contract.**  Crash between Envelope 1 commit and Envelope 2 start (OR mid-Envelope-2) leaves the doc in `DocState::Kvt2`.  Recovery path **already exists** at `boot_phase.rs:2468` — `dispatch_pending_doc::DocState::Kvt2` arm calls `stage_finalize::run(pool, doc_id)` directly with idempotent CAS `Kvt2 → Ack` (existing M3a invariant: `Conflict` outcome on already-Ack doc returns `StageFinalizeOutcome::AlreadyAcked`, no side effects).  No new boot dispatcher arm needed for W12 closure.

**Mid-drain crash recovery (same drain tick).**  W12 PR widens the W9b cohort walker filter (`fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd`) to include `DocState::Kvt2`, reversing the MED-C5-4 deferral.  Drain dispatch for `Kvt2` cohort entry routes through a new `process_via_w12_kvt2_advance` helper that invokes `stage_finalize::run` (same call as boot recovery) — sibling-continue with `DocVerdict::Advanced` on success, idempotent `AlreadyAcked` on replay.  This ensures crash mid-drain between Envelope 1 and Envelope 2 is recovered **within the same drain tick** without waiting for boot.

**Why not tx-local refactor.**  See above + W8 review F1 close docstring at `stage_finalize.rs:198-204` — the pool-only signature is a load-bearing safety property ("makes that bug class structurally impossible") that we do NOT want to weaken in W12.

**Idempotency invariants under two-envelope.**
- Envelope 1 CAS `Kvt1 → Kvt2` is gated by `WHERE state = 'KVT1'`; replay finds Kvt2 → `Conflict`; surfaces as structural drift only if `kvt1_raw` already differs from new evidence (HIGH-C5-2 byte-for-byte invariant catches this).
- Envelope 2 CAS `Kvt2 → Ack` is gated by `WHERE state = 'KVT2'`; replay on Ack → `AlreadyAcked` (no-op).
- Both gates together: crash recovery is convergent — every recovery path lands at `Ack`, never duplicates `STAGE_FINALIZE_ACK` audit or chain-seed advance.

---

## Channel scope (operator-pinned 2026-05-16)

W12 is the **WebCheck / gRPC** confirmation path only.  The `lastChk(fn_sign)` evidence shape — `status == OK` + `response.id == doc.server_fiscal_no` + non-empty `data_sign` — is gRPC-channel-specific.  The DFS HTTP / XML channel returns DFS-side tickets through `/fs/pck` / `/fs/doc` parsing rather than `lastChk` snapshots; a future M3+ task must implement DFS-ticket-driven KVT2 confirmation as a **separate helper**.  Do not claim DFS-side confirmation implemented in M3b under W12.

---

## Files (proposed)

- **NEW** `rust/prro/src/services/offline_sync/kvt2_confirm.rs` — typed surface + helper:
  - `pub enum Kvt2ConfirmOutcome { Acked { kvt1_raw_bytes: Vec<u8> }, Hold(Kvt2ConfirmHoldReason), StructuralDrift(Kvt2ConfirmStructuralReason) }`
  - `pub enum Kvt2ConfirmHoldReason { DpsTransientError(String), DpsServerError{status:i32, message:String}, LastChkStatusNotOk{status:i32, message:String}, LastChkIdMismatch{observed:String, expected:String}, LastChkDataSignEmpty }` — **all hold; doc state UNCHANGED per W0b §97-102**; sibling-continue; replayable next tick.
  - `pub enum Kvt2ConfirmStructuralReason { ServerFiscalNoMissing, CasMissOnAdvance{from:DocState, to:DocState, observed:DocState} }` — structural-invariant breach (stage_send 4-b set server_fiscal_no NOT NULL invariant broken / concurrent writer skew); surfaces as `BootError::Internal` for fail-loud forensics.  NOT manual-recon escalation — these indicate higher-level state corruption that operator triage cannot heal at the doc-level.
  - `pub async fn confirm_drain_doc(pool, dps, doc_id, fn_sign) -> Result<Kvt2ConfirmOutcome, BootError>` — pure helper; `lastChk` DPS call OUTSIDE any `with_immediate` per I1.
- **EDIT** `rust/prro/src/services/offline_sync/backlog_drain.rs`:
  - Replace `apply_w12_confirmation` stub body to call `kvt2_confirm::confirm_drain_doc` on the `Sent` outcome path.
  - Route `Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes }` through the **two-envelope ladder** (see §"Transaction envelope shape"): Envelope 1 persists `kvt1_raw` + CAS `Kvt1 → Kvt2` + `OFFLINE_DRAIN_KVT2_ADVANCED` audit (atomic via `with_immediate`); then Envelope 2 invokes `stage_finalize::run(pool, doc_id)` for `Kvt2 → Ack`.  On both envelopes success: `DocVerdict::Advanced` + summary `record_doc_advanced(W12ConfirmOutcome::Acked, via_lastchk_replay=false)`.
  - Route `Kvt2ConfirmOutcome::Hold(_)` → `DocVerdict::Failed { class: hold-specific FailureClass, manual_recon: false }` (sibling-continue; doc state **unchanged**, stays in `Kvt1` per W0b §97-102); emit `KVT2_CONFIRM_HOLD` Warning audit with typed hold reason payload.  **No CAS to Manual.**  Pending-drain shifts: hold class does NOT trigger halt (matches W9b §3.5 gravity rule + W0b state-unchanged contract).
  - Route `Kvt2ConfirmOutcome::StructuralDrift(_)` → `BootError::Internal` propagation (fail-loud; halts entire FN drain via existing `BootError` plumbing).  This is the ONLY non-success path that exits sibling-continue.  Reasoning: structural-drift indicates concurrent writer race past App reconcile mutex OR stage_send invariant breach — both are higher-level system state corruption, not doc-level operator-actionable issues.  Operator triage starts with the BootError audit chain; per-doc Manual CAS would mask the systemic skew.
  - **Widen drain cohort to include `DocState::Kvt2`** (reverses MED-C5-4 W9b deferral): update `fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd` SELECT IN list to `('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')`.  Dispatch `Kvt2` cohort entries via new `process_via_w12_kvt2_advance` that calls `stage_finalize::run` (idempotent under M3a `AlreadyAcked` contract).
  - Update `W12ConfirmOutcome` enum docstring: stub-only invariant retired; `DeferredKvt1` remains as the documented post-stage_send pre-confirmation marker emitted ONLY when `Kvt2ConfirmOutcome::Hold` (state unchanged → cohort walker re-visits next tick).
- **EDIT** `rust/prro/src/services/offline_sync/mod.rs` (`pub mod kvt2_confirm`).
- **EDIT** `rust/prro/src/db/repositories/fiscal_documents.rs` — widen drain-cohort SELECT IN list to include `KVT2` (MED-C5-4 W12 reversal).
- **KEEP** `rust/prro/src/services/reconciliation/boot_phase.rs::passive_hold_kvt1` as the primary boot-time handler for stale/pre-existing `Kvt1` docs outside drain context.  W12 does not change boot-time KVT1 dispatch.
- **KEEP** `rust/prro/src/services/reconciliation/boot_phase.rs:2468` `DocState::Kvt2` arm as the boot-time crash-recovery path between W12 envelopes.

---

## Day budget

2.5–3 days (revised upward from umbrella plan §Task 12 1.5–2d estimate to cover: explicit two-envelope transaction scope per MED-PR70-01, W0b-conformant Hold/StructuralDrift failure split per MED-PR70-02, mandatory crash-recovery convergence proofs, and drain cohort widening to include KVT2).

---

## Phasing (commit-level)

- **Commit 1 — helper + types**: `kvt2_confirm.rs` typed surface (`Outcome::{Acked, Hold(reason), StructuralDrift(reason)}`), evidence-check logic, no DB writes.  Unit tests against scripted `DpsChannel` stub.
- **Commit 2 — cohort widening + drain stub replacement**: `fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd` SELECT IN list extended to include `KVT2` (reverses MED-C5-4); `backlog_drain.rs::process_one_doc` adds `DocState::Kvt2` dispatch arm routing to new `process_via_w12_kvt2_advance` (calls `stage_finalize::run`).  `apply_w12_confirmation` stub replaced — Envelope 1 (W12-owned, persists `kvt1_raw` + Kvt1→Kvt2 + audit) chained with Envelope 2 (`stage_finalize::run` pool-call).
- **Commit 3 — Hold path (W0b state-unchanged conformance)**: route `Kvt2ConfirmOutcome::Hold(_)` through `DocVerdict::Failed { manual_recon: false }`; emit `KVT2_CONFIRM_HOLD` Warning audit with typed reason payload; no CAS.  Pending-drain halt parity NOT triggered (W9b §3.5 gravity rule preserved).
- **Commit 4 — StructuralDrift path**: route `Kvt2ConfirmOutcome::StructuralDrift(_)` as `BootError::Internal` propagation; halts entire FN drain.  NO per-doc Manual CAS (would mask systemic skew).
- **Commit 5 — fixture acceptance**: 6 success/hold-class fixtures (see Acceptance §Fixture matrix).
- **Commit 6 — interleave proof**: `backlog_drain_no_next_send_before_current_lastchk` fixture (per umbrella plan §Task 12 acceptance).
- **Commit 7 — crash-recovery convergence proof (MANDATORY, NOT optional)**: deterministic-replay fixtures covering 3 crash windows:
  - crash between Envelope 1 commit and Envelope 2 start → doc in `Kvt2` → boot recovery via `boot_phase::dispatch_pending_doc::DocState::Kvt2` lands `Ack`;
  - crash mid-Envelope-2 → doc in `Kvt2` (stage_finalize internal CAS not applied) → same boot recovery lands `Ack`;
  - crash mid-drain after Envelope 1 commit → next drain tick re-visits doc via widened cohort (KVT2 included) → `process_via_w12_kvt2_advance` advances to `Ack` (idempotent under M3a `AlreadyAcked`).
  These three proofs are load-bearing for W12 acceptance because the two-envelope ladder introduces a Kvt2 intermediate window that must be provably convergent.

---

## Acceptance criteria

### W12 core (from umbrella plan §Task 12 + W0b verdict §97-102)

1. W12 confirmation is invoked **only** from the W9b drain `Sent` outcome path; no boot-time invocation seam added.
2. **Hard interleave precondition**: no same-FN send may occur between `stage_send(doc_i)` and `lastChk(fn_sign)`.  Relies on W2 module-level enforcement + ADR-M3-A10 single-writer discipline; covered by App reconcile mutex (W9b carry-over).  Verified by `backlog_drain_no_next_send_before_current_lastchk` fixture.
3. Success evidence checks:
   - `lastChk.status == OK`;
   - `response.id == doc.server_fiscal_no`;
   - `response.data_sign` present AND non-empty.
4. On success: **two-envelope ladder** (per §"Transaction envelope shape").  Envelope 1 atomically persists `kvt1_raw_bytes` (byte-for-byte, HIGH-C5-2 contract preserved) + CAS `Kvt1 → Kvt2` via W1 service-layer `transition_state` helper + `OFFLINE_DRAIN_KVT2_ADVANCED` audit.  Envelope 2 calls `stage_finalize::run` for `Kvt2 → Ack` (M3a unchanged).
5. **W0b §97-102 state-unchanged conformance for evidence failures**: on `status != OK` OR id mismatch OR missing/empty `data_sign` OR DPS transport/server errors → `Kvt2ConfirmOutcome::Hold(reason)` → `DocVerdict::Failed { manual_recon: false }` → doc state UNCHANGED (stays in `Kvt1`) → `KVT2_CONFIRM_HOLD` Warning audit with typed reason payload → sibling-continue.  **No CAS to Manual.**  Doc remains replayable per W0b accepted contract.
6. **Structural-drift failures**: `ServerFiscalNoMissing` (stage_send 4-b invariant breach) OR `CasMissOnAdvance` (concurrent writer past App mutex) → `Kvt2ConfirmOutcome::StructuralDrift(reason)` → `BootError::Internal` propagation halts entire FN drain.  No per-doc Manual CAS (would mask systemic skew).
7. `passive_hold_kvt1` remains the primary boot-time handler for arbitrary/stale `Kvt1` docs outside drain context.
8. **Kvt2 boot-recovery path preserved**: `boot_phase::dispatch_pending_doc::DocState::Kvt2` arm (line 2468) continues to drive any orphaned `Kvt2` docs through `stage_finalize::run`; W12 does NOT touch this arm.

### Drain finalization unblock

9. With at least one real `Acked` outcome and zero held-at-Kvt1 docs, `DrainSummary::finalize_eligibility` returns `Eligible`; `OFFLINE_DRAIN_COMPLETED` audit emits; node mode `GoingOnline → Online`; session `Draining → Closed`.  Verified by `backlog_drain_completes_finalize_after_w12_acked` fixture.
10. With at least one `Hold(_)` outcome on Kvt1 docs (deferred-at-Kvt1 > 0), `DrainSummary::finalize_eligibility` returns `NotEligible { DocsDeferredAtKvt1 }`; `OFFLINE_DRAIN_PARTIAL` audit emits; node + session stay in pre-drain state.  Pre-W12 stub behavior preserved as the deferred-hold case.

### Cohort widening (KVT2 added to drain cohort)

11. `fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd` SELECT IN list extended from `('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE')` to `('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')` — reverses MED-C5-4 deferral.
12. New drain dispatch arm `DocState::Kvt2 → process_via_w12_kvt2_advance` calls `stage_finalize::run` (idempotent under M3a `AlreadyAcked` contract); routes `Ok(Acked)` → `DocVerdict::Advanced`, `Ok(AlreadyAcked)` → `DocVerdict::Advanced` (no-op replay), `Err(_)` → typed failure surface.

### Pending-drain halt parity (W9b carry-over, REVISED per W0b conformance)

13. `Kvt2ConfirmOutcome::Hold(_)` on a pending-drain shift (`OpenedLocalPendingDrain` | `ClosingLocalPendingDrain`) does NOT halt — sibling-continue per W9b §3.5 gravity rule + W0b state-unchanged contract.  Operator finding MED-PR70-02 fix: evidence-failure classes never escalate to manual.
14. `Kvt2ConfirmOutcome::StructuralDrift(_)` on a pending-drain shift halts entire FN drain via `BootError::Internal` propagation (different mechanism than W9b ER-guard's Manual CAS — structural drift is system-level, not operator-actionable).  No `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit (that's W9b ER-guard's manual-class halt only).

### Crash-recovery convergence (MANDATORY per MED-PR70-01 fix)

15. Crash between Envelope 1 commit and Envelope 2 invocation: doc in `Kvt2` → boot recovery via `boot_phase::dispatch_pending_doc::DocState::Kvt2` (existing M3a path) advances to `Ack` idempotently.
16. Crash mid-Envelope 2 (`stage_finalize::run` internal): doc in `Kvt2` (CAS not applied) → same boot recovery path lands `Ack`.
17. Crash mid-drain after Envelope 1: next drain tick re-visits the doc via widened cohort (KVT2 included) → `process_via_w12_kvt2_advance` advances to `Ack` idempotently; **same drain tick** recovery, no boot required.

### Fixture matrix (7 + 3 crash recovery)

W12-confirm specific:
- `kvt2_confirm_lastchk_match_advances_to_ack` (success path — Acked)
- `kvt2_confirm_lastchk_status_not_ok_holds_in_kvt1` (Hold class — W0b conformance)
- `kvt2_confirm_lastchk_id_mismatch_holds_in_kvt1` (Hold class — W0b conformance)
- `kvt2_confirm_missing_data_sign_holds_in_kvt1` (Hold class — W0b conformance)
- `kvt2_confirm_dps_transient_error_holds_in_kvt1` (Hold class)
- `kvt2_confirm_no_server_fiscal_no_returns_structural_drift` (StructuralDrift; surfaces as BootError::Internal)
- `kvt2_confirm_cas_miss_on_kvt1_to_kvt2_returns_structural_drift` (StructuralDrift)

Drain integration:
- `backlog_drain_no_next_send_before_current_lastchk` (interleave proof, per umbrella plan)
- `backlog_drain_completes_finalize_after_w12_acked` (finalize unblock proof)
- `backlog_drain_hold_class_does_not_halt_pending_drain` (W9b §3.5 gravity rule verified post-W12)

Crash-recovery convergence (MANDATORY):
- `replay_crash_between_envelope_1_and_envelope_2_lands_ack_via_boot` (#15)
- `replay_crash_mid_envelope_2_lands_ack_via_boot` (#16)
- `replay_crash_mid_drain_after_envelope_1_lands_ack_in_same_tick` (#17)

---

## BlockedBy

- ✅ W0b spec accepted (`2026-05-14-m3b-w0b-w12-gate-decision.md`)
- ✅ W1 helper composition (`transition_state` service-layer; `rust-gateway` `1b7632c`)
- ✅ W2 module-level enforcement (`ReconcileGuard`; `rust-gateway` `1651502`)
- ✅ W3 `first_kvt1_at` column + Kvt1-arm stamp (`rust-gateway` `aaf8d0e`)
- ✅ W9b backlog drain (`rust-gateway` `09196f1` + ER-class-guard `4a12c2f`)
- ✅ W11-Δ deterministic replay seam exists (W11 stays green; replay extension fixtures optional within W12 scope)

---

## Unblocks

- **W13 (M3b handoff doc + memory updates)** — `docs/M3b-handoff.md` per umbrella plan §Task 13.  W13 is docs-only; BlockedBy W11-Δ + W12 only; can land after W12 with tests green.
- **M3b CLOSURE FINAL** — after W12 + W13 lands, M3b is fully closed (currently M3b is CODE CLOSED 2026-05-22 per `4a12c2f` but DRAIN FINALIZATION remains stubbed pending W12).
- **Phase 6 pilot acceptance** — `docs/PILOT_ACCEPTANCE_TEST_PLAN.md` Phase 6 ("Offline With One Fiscal Number") requires real offline backlog closure to `Ack`; pilot demo can run end-to-end only after W12.

---

## Invariant impact

- **I1** preserved: `kvt2_confirm::confirm_drain_doc` makes the `lastChk` DPS call **outside** any `with_immediate` envelope.  Envelope 1 contains only the post-call CAS + audit composition; Envelope 2 (`stage_finalize::run`) owns its own atomic 5-write unit per M3a W8 contract.  Two-envelope ladder does not introduce nested transactions.
- **I2** load-bearing: correctness requires the W2 reconcile mutex to prevent same-FN send interleave between `stage_send(doc_i)` and `lastChk(fn_sign)`.  W2 is already merged; W12 inherits the guard.
- **I4** strengthened with explicit convergence proof: idempotency under all crash windows (between stage_send and W12 confirm / between Envelope 1 and Envelope 2 / mid-Envelope 2 / mid-drain after Envelope 1).  Every recovery path converges to `Ack`:
  - `lastChk` is read-only DPS-side; replay returns identical evidence.
  - Envelope 1 CAS `Kvt1 → Kvt2` guarded by `WHERE state = 'KVT1'`; replay → `Conflict` → state read disambiguates (`Kvt2` → already-advanced; structural drift only if `kvt1_raw` byte-mismatches HIGH-C5-2 expected evidence).
  - Envelope 2 CAS `Kvt2 → Ack` guarded by `WHERE state = 'KVT2'`; replay → `Conflict` → state read returns `Ack` → M3a `StageFinalizeOutcome::AlreadyAcked` (idempotent no-op, no duplicate audit, no duplicate outbox, no duplicate seed advance).
  - Mandatory fixture proofs (Acceptance §15-17) lock all three crash windows.
- **I8** strengthened: drain replay correctness through full ladder `OfflineLocalAck → Sending → Sent → Kvt1 → Kvt2 → Ack`.  Pre-W12 stub stopped at `Kvt1`; W12 closes the loop AND preserves W0b §97-102 state-unchanged contract for evidence failures (Hold class).  StructuralDrift class is system-level fail-loud, not doc-level escalation.
- **I9** preserved: graceful shutdown between any state pair leaves the doc in a recoverable state per Acceptance §15-17 proofs.  `passive_hold_kvt1` audit chain remains the boot-time forensic record for stale Kvt1 docs outside drain context.

---

## Carry-forwards from M3b (W9b ER-class-guard PR #69 self-review LOWs, deferred as scope-conformant)

- **LOW-1**: drain audit taxonomy does not have a distinct `OFFLINE_DRAIN_ER_BUDGET_EXHAUSTED` event_type (operator scope authorizes drain-specific audit projection).  W12 may consolidate: if `OFFLINE_DRAIN_KVT2_TERMINAL_FAILURE` audit taxonomy proves useful, retroactively split W9b ER manual-class events similarly.  Optional, not blocking.
- **LOW-2**: in W9b ER class guard, `emit_doc_failed` runs in a separate envelope from the CAS+ESCALATED audit.  W12 should NOT replicate this pattern — `cas_kvt1_to_manual_via_drain` MUST emit `OFFLINE_DRAIN_KVT2_TERMINAL_FAILURE` + `OFFLINE_DRAIN_DOC_FAILED` inside the SAME `with_immediate` envelope (forensic completeness).  If feasible without taxonomy drift, also retroactively fix W9b ER guard in the same PR or follow-up.
- **LOW-3**: drain CAS-helper returns `Err(BootError::Internal)` on non-Applied (stricter than boot's `Ok(bool)`).  W12 `cas_kvt1_to_manual_via_drain` should follow the same fail-loud pattern for consistency.  Operator-confirmed scope; not changing in this PR.

## Carry-forwards from M3b W14a-2a (operator-confirmed)

- **LOW**: direct test for `shifts::TransitionOutcome::Conflict` variant (~10 LoC).  Optional addition during W12 if scope permits; otherwise defer to W13 handoff cleanup.

---

## Day-budget breakdown

| Slice | Day | Detail |
|---|---|---|
| Commit 1 (helper + types) | 0.25 | typed surface with Hold/StructuralDrift split + evidence checks |
| Commit 2 (cohort widening + drain wiring two-envelope) | 0.5 | KVT2 cohort + Envelope 1 + Envelope 2 chained; new `process_via_w12_kvt2_advance` arm |
| Commit 3 (Hold path) | 0.25 | W0b state-unchanged conformance routing + `KVT2_CONFIRM_HOLD` audit |
| Commit 4 (StructuralDrift path) | 0.25 | BootError::Internal propagation; halt mechanism |
| Commit 5 (7 fixture acceptance) | 0.5 | scripted DpsChannel stub fixtures incl. all Hold sub-classes |
| Commit 6 (interleave proof) | 0.25 | drain-loop integration test |
| Commit 7 (crash-recovery convergence, MANDATORY) | 0.5 | 3 deterministic-replay fixtures (#15-17) |
| Review rounds + polish | 0.5 | per M3b convention (1-3 rounds typical for hot-zone PRs) |

---

## Verification commands

```bash
# Per-task targeted suites
cargo test -p prro --features test-support --test kvt2_confirm
cargo test -p prro --features test-support --test backlog_drain_state_dispatch
cargo test -p prro --features test-support --test backlog_drain_per_doc_loop
cargo test -p prro --features test-support --test backlog_drain_finalize

# Full suite (M3b acceptance baseline 705/0/1)
cargo test -p prro --features test-support

# Lint + format
cargo clippy -p prro --features test-support --tests --lib --no-deps -- -D warnings
cargo fmt -p prro -- --check
```

---

## Pilot gate readiness summary

- **Pre-W12 (current `rust-gateway` HEAD `4a12c2f`)**: offline drain stops at `Kvt1`; drain summary reports `NotEligible { DocsDeferredAtKvt1 }`; `OFFLINE_DRAIN_PARTIAL` audit; node remains `GoingOnline`.  Operator can still issue offline receipts (full Pattern C ingress path), but the backlog never closes to final `Ack` and shift cannot return to normal Online steady-state without manual intervention.
- **Post-W12**: full offline-online round-trip completes deterministically.  Phase 6 pilot acceptance demo executes end-to-end without manual operator step.

---

```json:metadata
{
  "files": [
    "rust/prro/src/services/offline_sync/kvt2_confirm.rs",
    "rust/prro/src/services/offline_sync/backlog_drain.rs",
    "rust/prro/src/services/offline_sync/mod.rs",
    "rust/prro/src/db/repositories/fiscal_documents.rs"
  ],
  "verifyCommand": "cargo test -p prro --features test-support --test kvt2_confirm --test backlog_drain_state_dispatch --test backlog_drain_finalize --test backlog_drain_per_doc_loop",
  "acceptanceCriteria": [
    "W12 invoked only from W9 drain-time flow",
    "lastChk status/id/data_sign evidence checks",
    "two-envelope ladder: Envelope 1 (kvt1_raw + Kvt1→Kvt2) + Envelope 2 (stage_finalize::run Kvt2→Ack)",
    "W0b §97-102 state-unchanged conformance for evidence failures (Hold class, no Manual CAS)",
    "StructuralDrift class (ServerFiscalNoMissing / CasMissOnAdvance) → BootError::Internal halt; not per-doc Manual",
    "drain cohort widened to include KVT2 (reverses MED-C5-4)",
    "new dispatch arm process_via_w12_kvt2_advance uses stage_finalize::run idempotent AlreadyAcked",
    "Kvt2 boot-recovery path (boot_phase.rs:2468 DocState::Kvt2 arm) preserved unchanged",
    "no same-FN send interleave before current lastChk (W2 mutex + ADR-M3-A10)",
    "drain Eligible arm unblocks finalize after Acked outcomes",
    "Hold class does NOT halt pending-drain shifts (W9b §3.5 gravity rule + W0b state-unchanged)",
    "kvt1_raw_bytes persisted byte-for-byte (HIGH-C5-2 contract preserved)",
    "crash-recovery convergence proofs MANDATORY: Envelope-1↔2 gap / mid-Envelope-2 / mid-drain after Envelope 1"
  ],
  "blockedBy": ["W0b", "W1", "W2", "W3", "W9b", "W9b-er-class-guard"],
  "unblocks": ["W13", "M3b-closure-final", "Phase-6-pilot-acceptance"]
}
```
