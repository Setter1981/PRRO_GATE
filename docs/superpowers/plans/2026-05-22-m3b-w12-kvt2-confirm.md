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
4. On success: persists KVT1_RAW + advances `Kvt1 → Kvt2 → Ack` via the W1 service-layer `transition_state` helper + existing M3a `stage_finalize::run` `Kvt2 → Ack` arm.
5. On failure: emits typed `Kvt2ConfirmFailure` + `KVT2_CONFIRM_FAILED` audit; doc stays in `Kvt1` for next-tick re-drive (within drain) or for `passive_hold_kvt1` (post-drain boot recovery).

Unblock W9b drain finalization **Eligible arm**: `DrainSummary::finalize_eligibility` currently always returns `NotEligible { reason: DocsDeferredAtKvt1 }` because the stub always reports `DeferredKvt1`. After W12, real `Acked` outcomes route through `DrainSummary::record_doc_advanced` → `advanced_to_ack += 1`, and zero deferred-at-KVT1 docs unlock the `Eligible` arm → `OFFLINE_DRAIN_COMPLETED` audit + node mode `GoingOnline → Online` + session `Draining → Closed`.

---

## Channel scope (operator-pinned 2026-05-16)

W12 is the **WebCheck / gRPC** confirmation path only.  The `lastChk(fn_sign)` evidence shape — `status == OK` + `response.id == doc.server_fiscal_no` + non-empty `data_sign` — is gRPC-channel-specific.  The DFS HTTP / XML channel returns DFS-side tickets through `/fs/pck` / `/fs/doc` parsing rather than `lastChk` snapshots; a future M3+ task must implement DFS-ticket-driven KVT2 confirmation as a **separate helper**.  Do not claim DFS-side confirmation implemented in M3b under W12.

---

## Files (proposed)

- **NEW** `rust/prro/src/services/offline_sync/kvt2_confirm.rs` — typed surface + helper:
  - `pub enum Kvt2ConfirmOutcome { Acked { kvt1_raw_bytes: Vec<u8> }, RetryNextTick(Kvt2ConfirmRetryReason), TerminalFailure(Kvt2ConfirmTerminalReason) }`
  - `pub enum Kvt2ConfirmRetryReason { DpsTransientError(String), DpsServerError(i32) }` — sibling-continue, doc stays in `Kvt1`
  - `pub enum Kvt2ConfirmTerminalReason { ServerFiscalNoMissing, LastChkStatusNotOk{status, message}, LastChkIdMismatch{observed, expected}, LastChkDataSignEmpty, CasMissOnAdvance{from, to, observed} }` — manual-recon class, halts pending-drain shifts via existing W9b ladder
  - `pub async fn confirm_drain_doc(pool, dps, doc_id, fn_sign) -> Result<Kvt2ConfirmOutcome, BootError>` — pure helper, no `with_immediate` envelopes inside (calls outside DB tx per I1)
- **EDIT** `rust/prro/src/services/offline_sync/backlog_drain.rs`:
  - Replace `apply_w12_confirmation` stub body to call `kvt2_confirm::confirm_drain_doc` on the `Sent` outcome path.
  - Route `Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes }` → CAS `Kvt1 → Kvt2 → Ack` via `stage_finalize::run` (existing M3a) wrapped in single `with_immediate` that ALSO persists `kvt1_raw` (HIGH-C5-2 contract preserved) and emits `OFFLINE_DRAIN_DOC_ADVANCED` with `w12_status: "Acked"` + `dispatch_via: "kvt2_confirm"`.
  - Route `Kvt2ConfirmOutcome::RetryNextTick(_)` → `DocVerdict::Failed { class: WireRoutingTransientRetry, manual_recon: false }` (sibling-continue; doc stays in `Kvt1` for next tick).
  - Route `Kvt2ConfirmOutcome::TerminalFailure(_)` → CAS `Kvt1 → RequiresManualReconciliation` via new drain-side helper + `OFFLINE_DRAIN_KVT2_TERMINAL_FAILURE` audit (manual-recon class; participates in pending-drain halt ladder).
  - Update `W12ConfirmOutcome` enum docstring: stub-only invariant retired; `DeferredKvt1` remains as the post-stage_send pre-confirmation marker for documents where `lastChk` retry was deferred to next tick.
- **EDIT** `rust/prro/src/services/offline_sync/mod.rs` (`pub mod kvt2_confirm`).
- **KEEP** `rust/prro/src/services/reconciliation/boot_phase.rs::passive_hold_kvt1` as the primary boot-time handler for stale/pre-existing `Kvt1` docs outside drain context.  W12 does not change boot-time KVT1 dispatch.

---

## Day budget

1.5–2 days (matches umbrella plan §Task 12 estimate).

---

## Phasing (commit-level)

- **Commit 1 — helper + types**: `kvt2_confirm.rs` typed surface, evidence-check logic, no DB writes.  Unit tests against scripted `DpsChannel` stub.
- **Commit 2 — drain stub replacement**: `backlog_drain.rs` `apply_w12_confirmation` rewritten to delegate to `kvt2_confirm::confirm_drain_doc`; `Acked` branch persists `kvt1_raw` + advances Kvt1→Kvt2 via `transition_state` + Kvt2→Ack via `stage_finalize::run` in one `with_immediate` envelope.
- **Commit 3 — terminal failure path**: new drain-side CAS helper `cas_kvt1_to_manual_via_drain` (mirrors W9b ER-guard `cas_er_to_manual_via_drain` pattern); typed terminal outcomes routed through it; new audit event `OFFLINE_DRAIN_KVT2_TERMINAL_FAILURE`.
- **Commit 4 — integration tests**: 5 acceptance fixtures (see Acceptance).
- **Commit 5 — interleave proof**: `backlog_drain_no_next_send_before_current_lastchk` fixture (per umbrella plan §Task 12 acceptance).
- **Commit 6 — replay-extension fixtures (W11-Δ optional)**: deterministic-replay fixtures covering crash points between stage_send and W12 confirm, between W12 confirm and Kvt2→Ack advance.  Can be split into W11-Δ if scope ballooning.

---

## Acceptance criteria

### W12 core (from umbrella plan §Task 12 + W0b verdict)

1. W12 confirmation is invoked **only** from the W9b drain `Sent` outcome path; no boot-time invocation seam added.
2. **Hard interleave precondition**: no same-FN send may occur between `stage_send(doc_i)` and `lastChk(fn_sign)`.  Relies on W2 module-level enforcement + ADR-M3-A10 single-writer discipline; covered by App reconcile mutex (W9b carry-over).  Verified by `backlog_drain_no_next_send_before_current_lastchk` fixture.
3. Success evidence checks:
   - `lastChk.status == OK`;
   - `response.id == doc.server_fiscal_no`;
   - `response.data_sign` present AND non-empty.
4. On success: `Kvt1 → Kvt2` via `transition_state` (W1 service-layer helper) + `Kvt2 → Ack` via `stage_finalize::run` (M3a unchanged).  `kvt1_raw_bytes` persisted byte-for-byte (HIGH-C5-2 contract preserved).
5. On `status != OK`, id mismatch, missing/empty `data_sign`, or lost CAS: typed `Kvt2ConfirmTerminalReason` + `KVT2_CONFIRM_FAILED` Warning audit + `OFFLINE_DRAIN_KVT2_TERMINAL_FAILURE` audit on CAS to Manual; doc does NOT reach `Ack`.
6. On DPS transport/server retry-class errors: typed `Kvt2ConfirmRetryReason` + sibling-continue; doc stays in `Kvt1` for next drain tick.
7. `passive_hold_kvt1` remains the primary boot-time handler for arbitrary/stale `Kvt1` docs outside drain context.

### Drain finalization unblock

8. With at least one real `Acked` outcome and zero deferred-at-Kvt1 docs, `DrainSummary::finalize_eligibility` returns `Eligible`; `OFFLINE_DRAIN_COMPLETED` audit emits; node mode `GoingOnline → Online`; session `Draining → Closed`.  Verified by `backlog_drain_completes_finalize_after_w12_acked` fixture.
9. With at least one `RetryNextTick` outcome on Kvt1 docs (deferred-at-Kvt1 > 0), `DrainSummary::finalize_eligibility` returns `NotEligible { DocsDeferredAtKvt1 }`; `OFFLINE_DRAIN_PARTIAL` audit emits; node + session stay in pre-drain state.  Pre-W12 stub behavior preserved as the deferred-retry case.

### Pending-drain halt parity (W9b carry-over)

10. `Kvt2ConfirmOutcome::TerminalFailure` on a pending-drain shift (`OpenedLocalPendingDrain` | `ClosingLocalPendingDrain`) triggers the existing W9b halt ladder: shift CAS via edge 6/14 → `RequiresManualReconciliation` + Critical `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit + drain returns immediately without visiting subsequent docs.  `RetryNextTick` on pending-drain shift does NOT halt (sibling-continue per W9b §3.5 gravity rule).

### Fixture matrix (5 + 2)

- `kvt2_confirm_lastchk_match_advances_to_ack` (success path)
- `kvt2_confirm_lastchk_id_mismatch_no_ack` (terminal failure)
- `kvt2_confirm_missing_data_sign_no_ack` (terminal failure)
- `kvt2_confirm_dps_transient_error_retries_next_tick` (retry class)
- `kvt2_confirm_no_server_fiscal_no_no_ack` (terminal failure — structural)
- `backlog_drain_no_next_send_before_current_lastchk` (interleave proof, per umbrella plan)
- `backlog_drain_completes_finalize_after_w12_acked` (finalize unblock proof)

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

- **I1** preserved: `kvt2_confirm::confirm_drain_doc` makes the `lastChk` DPS call **outside** any `with_immediate` envelope; only the post-call CAS + audit composition lands inside a single short `with_immediate`.
- **I2** load-bearing: correctness requires the W2 reconcile mutex to prevent same-FN send interleave between `stage_send(doc_i)` and `lastChk(fn_sign)`.  W2 is already merged; W12 inherits the guard.
- **I4** strengthened: idempotency under crash between stage_send and confirm — `lastChk` is read-only on DPS side, drain CAS uses whitelisted edge gates.  Next-tick re-drive finds the same `server_fiscal_no` recorded; `lastChk` returns the same evidence; advance is idempotent (CAS guard `WHERE state = 'KVT1'` makes 2nd attempt a structural-drift Internal error).
- **I8** strengthened: drain replay correctness through full ladder `OfflineLocalAck → Sending → Sent → Kvt1 → Kvt2 → Ack`.  Pre-W12 stub stopped at `Kvt1`; W12 closes the loop.
- **I9** preserved: graceful shutdown between any two states leaves the doc in a recoverable state — either next-tick W12 re-confirms (if Kvt1) or `passive_hold_kvt1` records forensic audit (if drain context lost).

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
| Commits 1+2 (helper + drain wiring) | 0.5 | typed surface + evidence checks + stub replacement |
| Commit 3 (terminal failure path) | 0.25 | new drain-side CAS helper + audit emit |
| Commit 4 (5 fixture acceptance) | 0.5 | scripted DpsChannel stub fixtures |
| Commit 5 (interleave proof) | 0.25 | drain-loop integration test with empty `doc_{i+1}` queue assertion |
| Commit 6 (W11-Δ replay extensions, optional) | 0.25 | crash-point fixtures; split to W11-Δ if balloon |
| Review rounds + polish | 0.25 | per M3b convention (1-2 rounds typical) |

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
    "rust/prro/src/services/offline_sync/mod.rs"
  ],
  "verifyCommand": "cargo test -p prro --features test-support --test kvt2_confirm --test backlog_drain_state_dispatch --test backlog_drain_finalize",
  "acceptanceCriteria": [
    "W12 invoked only from W9 drain-time flow",
    "lastChk status/id/data_sign evidence checks",
    "Kvt1→Kvt2→Ack on success",
    "typed terminal failure with no Ack on mismatch/missing evidence",
    "typed retry-class on DPS transport/server errors keeps doc in Kvt1",
    "passive_hold_kvt1 remains primary for stale boot-time Kvt1",
    "no same-FN send interleave before current lastChk",
    "drain Eligible arm unblocks finalize after Acked outcomes",
    "pending-drain halt parity for TerminalFailure",
    "kvt1_raw_bytes persisted byte-for-byte (HIGH-C5-2 contract preserved)"
  ],
  "blockedBy": ["W0b", "W1", "W2", "W3", "W9b", "W9b-er-class-guard"],
  "unblocks": ["W13", "M3b-closure-final", "Phase-6-pilot-acceptance"]
}
```
