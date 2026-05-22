# M3b W12 — In-Drain KVT2 Confirmation via `lastChk`

**Status:** OPEN
**Date:** 2026-05-22
**Umbrella plan anchor:** `docs/superpowers/plans/2026-05-14-m3b-implementation.md` §Task 12.
**W0b verdict anchor:** `docs/superpowers/specs/2026-05-14-m3b-w0b-w12-gate-decision.md` — ACCEPTED 2026-05-14, **YES with explicit scope restriction**.
**Predecessor:** W9b (PR #68, `rust-gateway` `09196f1`) + W9b ER-class-guard (PR #69, `rust-gateway` `4a12c2f`).
**Pilot impact:** **Pilot-gating** if pilot acceptance requires real offline backlog closure to final DPS `Ack` (Phase 6 of `docs/PILOT_ACCEPTANCE_TEST_PLAN.md`).

---

## Goal

Replace the W9b pre-W12 `apply_w12_confirmation` stub (`backlog_drain.rs:1470`, always returns `W12ConfirmOutcome::DeferredKvt1`) with a real in-drain KVT2 confirmation helper invoked from THREE drain entry points (per HIGH-PR70-R2-01 + HIGH-PR70-R3-01 fix; W12 is NOT Sent-only AND not Sent-fresh-only):

1. **Fresh `Sent` entry (post-`stage_send`)**: immediately after `stage_send::run` returns `StageSendOutcome::Sent` (doc has `server_fiscal_no` newly stamped this tick); kvt2_confirm runs the lastChk evidence check; on Acked Envelope 1a chain commits (Kvt1Raw + Sent→Kvt1 + Kvt1→Kvt2 + audit) then Envelope 2 finalizes; on Hold the drain stops at the held doc.
2. **`Kvt1` re-entry (cohort)**: drain cohort walker emits `Kvt1` docs (KVT1 already in W9b cohort filter); these are docs Held on a prior tick after a successful Sent→Kvt1 mid-tick advance, OR docs landed in Kvt1 by the legacy stub before W12 merges; kvt2_confirm re-issues lastChk on the **still-latest** doc and either advances or Holds again.
3. **`Sent` re-entry / replay (cohort)** (HIGH-PR70-R3-01 fix): drain cohort walker emits persisted `Sent` docs that crashed mid-tick BEFORE the Envelope 1a chain committed (e.g. inside Envelope 1a rollback OR before Envelope 1a started). The current `process_via_lastchk_replay` arm is rewritten to be **W12-aware**: it invokes `kvt2_confirm::confirm_drain_doc` (same helper as Sent-fresh and Kvt1 entries), routes Acked through Envelope 1a (same atomic Sent→Kvt1→Kvt2 chain), routes Hold through `DocVerdict::HoldFnDrain` (NOT current sibling-continue), routes StructuralDrift through `BootError::Internal`. This closes the W0b latest-doc precondition gap that existed in the prior R2 revision: a Sent-source crash-recovery path that fell back to current W9b replay semantics could allow later same-FN sends after a Held doc.

Plus, the **Sent-replay arm uniquely retains the HIGH-C5-3 safe-redrive case** (HIGH-PR70-R4-01 fix): on `Err(DpsError::NotFound)` from `dps.by_server_fiscal_no(fn_sign, &doc.server_fiscal_no)` (DPS has zero history of the doc's `server_fiscal_no`), the W12 outcome is `Kvt2ConfirmOutcome::SentNotFoundDowngrade` (distinct from Hold) — atomically commit `Sent → ErrorRetryable` + stamp durable `TransientRetry` retry_class label + emit audit + return `DocVerdict::HoldFnDrain`.  Next tick: doc enters ER cohort → W9b ER class guard bounded-redrive (`MAX_BOOT_ATTEMPTS=5` cap) → `stage_send::run` Pattern B redrive → eventually Match (Ack) OR ER budget-exhausted manual.  Preserves boot dispatcher's existing two-tick replay contract (see `boot_phase.rs:733` for reference pattern).  Holding indefinitely on a Sent-replay NotFound would never resend → liveness hole on a known crash-recovery outcome.

In all three cases (Acked / Hold / StructuralDrift outcomes — SentNotFoundDowngrade exclusively from Sent-replay arm):

- Reads the doc's `server_fiscal_no` (from `stage_send::run` 4-b OR from the prior-tick row).
- Issues `lastChk(fn_sign)` and validates evidence per W0b §Verdict: `response.status == OK` + `response.id == doc.server_fiscal_no` + non-empty `response.data_sign`.
- **On success (two-envelope ladder; see §"Transaction envelope shape")**: Envelope 1 (W12-owned) atomically composes the source-state-specific advance (Sent source: `Sent → Kvt1 → Kvt2` + persist `KVT1_RAW`; Kvt1 source: `Kvt1 → Kvt2` + persist `KVT1_RAW`); Envelope 2 (`stage_finalize::run`-owned) runs `Kvt2 → Ack` atomically.
- **On Hold (W0b §97-102 conformance + W0b §latest-doc precondition)**: doc state **unchanged** (stays in `Sent` if Sent-fresh or Sent-replay source AND Envelope 1a never committed; OR stays in `Kvt1` if Kvt1 re-entry source).  Typed `Kvt2ConfirmHoldReason` + `KVT2_CONFIRM_HOLD` Warning audit.  **`DocVerdict::HoldFnDrain` — stops THIS FN's drain at the held doc**, NOT sibling-continue.  Subsequent docs on the same FN are not processed in this tick.  Next drain tick: cohort walker re-visits the still-Sent OR still-Kvt1 doc via the appropriate W12 entry point (Sent-replay arm for Sent-state holds; Kvt1 re-entry arm for Kvt1-state holds); kvt2_confirm re-runs on the **still-latest** doc per W0b precondition.  Pending-drain shifts: Hold does NOT trigger manual escalation (per W9b §3.5 + W0b state-unchanged), BUT it also does NOT continue past the held doc.
- **On StructuralDrift**: `BootError::Internal` propagation halts the entire FN drain.  System-level fail-loud; NOT per-doc Manual CAS.

Unblock W9b drain finalization **Eligible arm**: `DrainSummary::finalize_eligibility` currently always returns `NotEligible { reason: DocsDeferredAtKvt1 }` because the stub always reports `DeferredKvt1`.  After W12: real `Acked` outcomes route through `DrainSummary::record_doc_advanced` → `advanced_to_ack += 1`; real `Hold` outcomes route through the projection-specific recording method (`record_doc_held_at_kvt1` for `Kvt1Reentry` Hold OR `record_doc_held_at_sent` for `SentFresh`/`SentReplay` Hold); real `SentNotFoundDowngrade` outcomes route through `record_doc_er_redrive_queued` (durable state ER, not KVT1).  Zero on ALL THREE W12 counters (held_at_kvt1 + held_at_sent + er_redrive_queued) unlocks the `Eligible` arm → `OFFLINE_DRAIN_COMPLETED` audit + node mode `GoingOnline → Online` + session `Draining → Closed`.  Any nonzero counter returns `NotEligible` with the appropriate reason (`DocsHeldAtKvt1` / `DocsHeldAtSent` / `DocsErRedriveQueued`) and multi-reason payload when several are nonzero.

**Why W12 invocation is NOT Sent-only.**  The Hold semantics require Kvt1 docs to be re-driven through kvt2_confirm on a subsequent drain tick.  This is structurally NOT boot-time arbitrary `Kvt1` polling (which W0b explicitly excludes — see §line 88) — it is drain-time re-confirmation of a doc that was the latest-on-FN at the moment of `stage_send` AND remains the latest-on-FN because Hold stops further same-FN sends.  The W0b latest-doc precondition is preserved by Hold's stop-FN-drain semantics.  Boot-time stale Kvt1 (docs predating M3b drain context) continue to be handled by `passive_hold_kvt1`.

---

## Transaction envelope shape (MED-PR70-01 + MED-PR70-R2-02 resolution, 2026-05-22)

**Chosen form: two-envelope ladder** (NOT tx-local refactor of `stage_finalize`).

**Rationale.**  `stage_finalize::run(pool, doc) -> Result<...>` (at `rust/prro/src/services/write_path/stage_finalize.rs:234`) is a load-bearing M3a contract.  It owns its own `with_immediate` envelope spanning the 5-write atomicity unit (CAS `Kvt2 → Ack` + chain-seed advance + inbox DONE + outbox row + `STAGE_FINALIZE_ACK` audit per W8 review F1 close).  Refactoring this into a tx-local variant would expose chain-seed / inbox / outbox manipulation across module boundaries and require a separate audit of all downstream consumers — **out of W12 scope**.

### Envelope 1 — state-specific (MED-PR70-R2-02 fix)

Envelope 1 owns the source-state-specific advance from the pre-W12 state to `Kvt2`, atomically.  Two source states reach W12 in the drain hot path:

#### (a) Source state = `Sent` (fresh post-`stage_send` invocation)

```
[Envelope 1a: W12-owned, called after StageSendOutcome::Sent]
  with_immediate(pool, |tx| async {
    1. document_files::replace_tx(tx, doc_id, DocumentFileKind::Kvt1Raw, &data_sign_bytes)
       (HIGH-C5-2 contract: byte-for-byte `lastChk.data_sign` persist; W9b uses
        document_files Kvt1Raw artefact kind via INSERT OR REPLACE — matches
        the existing repository contract at
        `rust/prro/src/db/repositories/document_files.rs:124`).
    2. fiscal_documents::transition_state(tx, doc_id, Sent, Kvt1)
       (whitelisted edge per `fiscal_documents.rs:141`; preserves the current
        W9b stub's Sent → Kvt1 CAS — NOT dropped).
    3. fiscal_documents::transition_state(tx, doc_id, Kvt1, Kvt2)
       (whitelisted edge; transitions immediately within the same envelope to
        avoid a Kvt1-intermediate window inside this drain tick).
    4. audit_log::append_tx(tx, "OFFLINE_DRAIN_KVT2_ADVANCED", payload)
       (W12 evidence trail; payload includes from_state="SENT", to_state="KVT2",
        dispatch_via="kvt2_confirm", server_fiscal_no, kvt1_raw_sha256_hex).
  })
```

All four operations commit atomically.  Two whitelisted edges (`Sent → Kvt1`, `Kvt1 → Kvt2`) executed inside one tx — no direct `Sent → Kvt2` edge is invented (none exists in `fiscal_documents::allowed_transition` and none is introduced by W12).

#### (b) Source state = `Kvt1` (drain cohort re-entry after prior-tick Hold)

```
[Envelope 1b: W12-owned, called on cohort dispatch for DocState::Kvt1]
  with_immediate(pool, |tx| async {
    1. document_files::replace_tx(tx, doc_id, DocumentFileKind::Kvt1Raw, &data_sign_bytes)
       (idempotent: prior-tick Hold did not persist Kvt1Raw; replace_tx INSERT OR
        REPLACE handles the no-prior-row case AND the re-write-same-bytes case;
        byte-for-byte invariant means re-write is content-identical).
    2. fiscal_documents::transition_state(tx, doc_id, Kvt1, Kvt2)
       (single whitelisted edge; doc is already at Kvt1 from prior tick).
    3. audit_log::append_tx(tx, "OFFLINE_DRAIN_KVT2_ADVANCED", payload)
       (W12 evidence trail; payload from_state="KVT1", to_state="KVT2",
        dispatch_via="kvt2_confirm_kvt1_reentry", server_fiscal_no,
        kvt1_raw_sha256_hex, prior_hold_count: reads any prior KVT2_CONFIRM_HOLD
        audit count for forensic continuity).
  })
```

#### (c) Source state = `Sent` (Sent-replay), `Err(DpsError::NotFound)` from `dps.by_server_fiscal_no(fn_sign, &doc.server_fiscal_no)` (HIGH-PR70-R4-01 safe-redrive)

Two-envelope-around-DPS-probe lifecycle (MED-PR70-R5-02 fix; mirrors `boot_phase.rs:1521-1542` pre-probe allocation + `boot_phase.rs:743-770` post-NotFound completion).  The DPS call (`lastChk`) MUST sit between the two envelopes (I1).

```
[Envelope 1c-pre: W12-owned, allocates recovery transport_trace row
                  BEFORE lastChk probe — Sent-replay arm only]
  with_immediate(pool, |tx| async {
    let attempt_no = transport_trace::allocate_and_insert_tx(
        tx,
        doc_id,
        transport_trace::NewAttempt {
            backend_profile_id: doc.backend_profile_id,
            transport_profile_id: doc.transport_profile_id,
            request_envelope_sha256: [0u8; 32],  // probe is query, no envelope
        },
    ).await?;
    Ok::<i64, anyhow::Error>(attempt_no)
  })
  → returns attempt_no (threaded into SentNotFoundDowngrade outcome)
```

```
[lastChk DPS call — OUTSIDE envelope per I1]
  let (wire_started, wire_finished, probe_outcome)
      = call_lastchk_recording_wire_times(dps, fn_sign).await;
```

```
[Envelope 1c-post: W12-owned, completes recovery trace row +
                   transitions doc state — fires ONLY on NotFound]
  with_immediate(pool, |tx| async {
    1. transport_trace::complete_tx(tx, doc_id, attempt_no,
         transport_trace::AttemptCompletion {
             wire_call_started_at: wire_started,
             wire_call_finished_at: wire_finished,
             outcome_kind: transport_trace::OutcomeKind::RetryableServer,
             server_fiscal_no: None,
             server_status_code: None,
             error_kind: Some("LAST_CHK_NOTFOUND"),
             error_message: Some("DPS last_chk returned NotFound; \
                                  tick-2 of two-tick retry path will \
                                  re-drive via Pattern B"),
             retry_class: Some(RetryClass::TransientRetry.as_str()),
         });
       (Completes the exact unfinished row allocated in Envelope 1c-pre.
        `transport_trace::complete_tx` per `transport_trace.rs:174`
        requires existing unfinished row; cannot rewrite completed
        attempts.  This is the durable TransientRetry label that the
        next-tick ER class guard's `evaluate_er_redrive` reads via
        `last_attempt_retry_class_for` per `transport_trace.rs:299`.)
    2. fiscal_documents::transition_state(tx, doc_id, Sent, ErrorRetryable)
       (whitelisted edge; HIGH-C5-3 safe Pattern B redrive precondition).
    3. audit_log::append_tx(tx, "OFFLINE_DRAIN_SENT_NOT_FOUND_DOWNGRADE",
       payload).
  })
  → Caller returns DocVerdict::HoldFnDrain — drain stops at this doc.
```

**MED-PR70-R5-02 lifecycle invariants:**
- One `attempt_no` allocated pre-lastChk; same `attempt_no` completed post-NotFound.  No allocation race; `BEGIN IMMEDIATE` in Envelope 1c-pre serialises `MAX(attempt_no)+1`.
- `transport_trace::complete_tx` does NOT rewrite the prior stage_send-completed attempt — it completes the FRESH allocated row.  Allocated row is the latest by `attempt_no` (since allocation was `MAX+1`); ER class guard's `last_attempt_retry_class_for` sorts by `attempt_no DESC LIMIT 1` and reads `TransientRetry` from this completion.
- Non-NotFound outcomes (Match / Hold variants / StructuralDrift) ALL complete the pre-allocated row in dedicated atomic envelopes per R6: Match → Envelope 1a-replay (trace complete OK); Hold → Envelope 1c-hold (trace complete RetryableTransport|RetryableServer, no state change); StructuralDrift → Envelope 1c-drift (trace complete with structural failure evidence before BootError::Internal).  **No row left intentionally unfinished on normal paths** — supersedes prior R5 text suggesting deferred housekeeping.  Only the narrow crash window between Envelope 1c-pre commit and the post-outcome envelope leaves an unfinished row; next-tick recovery allocates MAX+1 (never overwrites); ER guard reads latest via `attempt_no DESC` so the stale row does not affect routing.  Mirrors boot probe trace completion convention at `boot_phase.rs:516, 707, 803`.

Outcome class `Kvt2ConfirmOutcome::SentNotFoundDowngrade { trace_attempt_no }` is exclusive to the Sent-replay arm.  Sent-fresh + Kvt1 re-entry NEVER emit this variant (NotFound from those contexts routes via `StructuralDrift::NotFoundOutsideSentReplay{source}` — see source-context routing matrix in §"Files (proposed)").

#### (c-match) SentReplay Match — Envelope 1a-replay (MED-PR70-R6-01 trace completion)

For Sent-replay Match outcomes, Envelope 1a is extended to include trace completion for the row allocated in Envelope 1c-pre.  Without this, normal Match paths would leave the recovery row unfinished — diverging from boot parity (`boot_phase.rs:516`) and breaking the `transport_trace::last_attempt_retry_class_for` contract (an unfinished latest row carries `retry_class=None` per `transport_trace.rs:299`, which would mis-attribute the completed advance).

```
[Envelope 1a-replay: SentReplay Match only; sent_replay_trace_attempt_no = Some(n)]
  with_immediate(pool, |tx| async {
    1. transport_trace::complete_tx(tx, doc_id, n, AttemptCompletion {
           wire_call_started_at: wire_started,
           wire_call_finished_at: wire_finished,
           outcome_kind: transport_trace::OutcomeKind::Ok,
           server_fiscal_no: Some(doc.server_fiscal_no),
           server_status_code: Some(OK_STATUS_CODE),
           error_kind: None,
           error_message: None,
           retry_class: None,
       });
    2. document_files::replace_tx(tx, doc_id, DocumentFileKind::Kvt1Raw, &data_sign_bytes);
    3. fiscal_documents::transition_state(tx, doc_id, Sent, Kvt1);
    4. fiscal_documents::transition_state(tx, doc_id, Kvt1, Kvt2);
    5. audit_log::append_tx(tx, "OFFLINE_DRAIN_KVT2_ADVANCED", payload with
       dispatch_via="kvt2_confirm_sent_replay", via_lastchk_replay=true);
  })
  // Envelope 2 (stage_finalize::run) follows for Kvt2 → Ack.
```

Sent-fresh Match path runs Envelope 1a (without trace completion step; `sent_replay_trace_attempt_no = None`); Kvt1 re-entry Match runs Envelope 1b (no trace step).

#### (c-hold) SentReplay Hold — Envelope 1c-hold (MED-PR70-R6-01 trace completion, no state change)

For Sent-replay Hold outcomes (DpsTransport / DpsServer / DpsAuthorization / DpsDecode / DataSignEmpty), a dedicated Envelope 1c-hold completes the allocated trace row WITHOUT state mutation.  Mirrors boot pattern at `boot_phase.rs:803` (probe failures with no state change still complete the row for forensic completeness):

```
[Envelope 1c-hold: SentReplay Hold; sent_replay_trace_attempt_no = Some(n)]
  with_immediate(pool, |tx| async {
    1. transport_trace::complete_tx(tx, doc_id, n, AttemptCompletion {
           wire_call_started_at: wire_started,
           wire_call_finished_at: wire_finished,
           outcome_kind: transport_trace::OutcomeKind::RetryableTransport
                         OR RetryableServer (mapped per hold reason),
           server_fiscal_no: None,
           server_status_code: None,
           error_kind: Some(hold_reason.audit_label()),
           error_message: Some(hold_reason.to_string()),
           retry_class: None,
           // retry_class=None for Hold because doc stays Sent; ER class guard
           // does NOT consume the value (only Sent→ER NotFound path needs
           // TransientRetry stamp).  Next-tick SentReplay re-allocates a fresh
           // recovery row.
       });
    2. audit_log::append_tx(tx, "KVT2_CONFIRM_HOLD", payload with
       hold_reason=hold_reason.audit_label(), source="sent_replay");
  })
  // Caller returns DocVerdict::HoldFnDrain { projection: HeldAtSent }.
  // Doc state unchanged (Sent).
```

Sent-fresh Hold and Kvt1 re-entry Hold paths emit `KVT2_CONFIRM_HOLD` audit only (no trace completion — they did not allocate one); `sent_replay_trace_attempt_no = None` ensures the helper does not attempt to complete a nonexistent row.

#### (c-drift) SentReplay StructuralDrift — Envelope 1c-drift (MED-PR70-R6-01 trace completion, fail-loud after audit)

For Sent-replay StructuralDrift outcomes (LastChkIdMismatch only — NotFoundOutsideSentReplay structurally cannot reach SentReplay context per definition), the allocated trace row is completed inside one short envelope BEFORE BootError::Internal propagation surfaces.  This preserves the forensic record even on fail-loud:

```
[Envelope 1c-drift: SentReplay StructuralDrift; sent_replay_trace_attempt_no = Some(n)]
  with_immediate(pool, |tx| async {
    1. transport_trace::complete_tx(tx, doc_id, n, AttemptCompletion {
           wire_call_started_at: wire_started,
           wire_call_finished_at: wire_finished,
           outcome_kind: transport_trace::OutcomeKind::RetryableServer,
           server_fiscal_no: None,
           server_status_code: None,
           error_kind: Some("STRUCTURAL_DRIFT_LASTCHK_ID_MISMATCH"),
           error_message: Some("DPS lastChk id != doc.server_fiscal_no; \
                                state-machine drift past App reconcile mutex"),
           retry_class: None,
       });
    2. audit_log::append_tx(tx, "KVT2_CONFIRM_STRUCTURAL_DRIFT", payload
       with structural_reason="LastChkIdMismatch", source="sent_replay");
  })
  // Caller returns BootError::Internal — drain halt entire FN.
```

**Implementation note on the rare crash window**: a crash strictly between Envelope 1c-pre commit and the subsequent post-outcome envelope (1a-replay / 1c-hold / 1c-post / 1c-drift) leaves the recovery row unfinished.  This is the ONLY normal-path situation where an unfinished W12-allocated row persists.  Next-tick recovery: cohort walker re-emits the doc in its current state (Sent for Hold/Drift-pre-commit; ER for NotFound-pre-commit which never happens because NotFound requires 1c-post commit; etc.).  On re-entry, SentReplay allocates a FRESH recovery row (allocation is `MAX(attempt_no)+1`, never overwrites).  The stale unfinished row remains as forensic evidence of the crash window; ER guard's `last_attempt_retry_class_for` reads the latest row (the new one) per `transport_trace.rs:299` `ORDER BY attempt_no DESC LIMIT 1`, so the stale row does NOT affect routing decisions.

### Envelope 2 — identical for all success paths

```
[Envelope 2: stage_finalize::run-owned, called from drain after envelope 1 commits]
  stage_finalize::run(pool, doc_id).await
    → with_immediate(pool, |tx| async {
        CAS Kvt2 → Ack + seed + inbox + outbox + STAGE_FINALIZE_ACK.
      })
```

### Crash-recovery contract (revised per state-specific Envelope 1)

**Recovery path coverage:**

- **Crash inside Envelope 1**: rolled back atomically.  Doc state stays as the cohort-walker emitted (Sent OR Kvt1).
  - If Sent: cohort walker emits the `Sent` doc; drain dispatch routes to the **W12-aware rewritten `process_via_lastchk_replay` arm** (HIGH-PR70-R3-01 fix), which invokes `kvt2_confirm::confirm_drain_doc` and routes Acked through Envelope 1a chain, Hold through `DocVerdict::HoldFnDrain` (stops FN drain), StructuralDrift through `BootError::Internal`.  This is the same convergent W12 logic as the post-stage_send Sent-fresh path, just reached via cohort dispatch on the next tick.
  - If Kvt1: next drain tick re-enters via cohort `Kvt1` dispatch → `process_via_w12_only` (rewritten in this PR) → kvt2_confirm → Envelope 1b → Envelope 2.
- **Crash between Envelope 1 commit and Envelope 2 invocation**: doc in `Kvt2`.  Recovery path **already exists** at `boot_phase.rs:2468` — `dispatch_pending_doc::DocState::Kvt2` arm calls `stage_finalize::run(pool, doc_id)` directly with idempotent CAS `Kvt2 → Ack` (existing M3a invariant: `Conflict` outcome on already-Ack doc returns `StageFinalizeOutcome::AlreadyAcked`, no side effects).
- **Crash mid-Envelope 2**: rolled back atomically; doc in `Kvt2`; same boot recovery path as above.
- **Crash mid-drain after Envelope 1 commit but before tick exit**: doc in `Kvt2`.  W12 PR widens the W9b cohort walker filter (`fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd`) to include `DocState::Kvt2`, reversing the MED-C5-4 deferral.  Drain dispatch for `Kvt2` cohort entry routes through a new `process_via_w12_kvt2_advance` helper that invokes `stage_finalize::run` (same call as boot recovery) — `DocVerdict::Advanced` on success, idempotent `AlreadyAcked` on replay.  This ensures crash mid-drain is recovered **within the same drain tick** without waiting for boot.

**Why not tx-local refactor.**  See above + W8 review F1 close docstring at `stage_finalize.rs:198-204` — the pool-only signature is a load-bearing safety property ("makes that bug class structurally impossible") that we do NOT want to weaken in W12.

### Idempotency invariants under two-envelope

- Envelope 1a CAS `Sent → Kvt1`: gated by `WHERE state = 'SENT'`; replay finds `Kvt1` (advanced) → `Conflict`; structural drift check via `kvt1_raw` byte-equiv (HIGH-C5-2: if bytes match, no drift; if differ, fail-loud).
- Envelope 1a CAS `Kvt1 → Kvt2`: gated by `WHERE state = 'KVT1'`; replay finds `Kvt2` → `Conflict`; same byte-equiv check via document_files row read.
- Envelope 1b CAS `Kvt1 → Kvt2`: same as Envelope 1a step (3); `document_files::replace_tx` for Kvt1Raw is INSERT OR REPLACE, content-identical replay is no-op.
- Envelope 2 CAS `Kvt2 → Ack`: gated by `WHERE state = 'KVT2'`; replay on Ack → `AlreadyAcked` (no-op).
- Cross-envelope: replay convergence proof in Acceptance §18-20.

---

## Channel scope (operator-pinned 2026-05-16)

W12 is the **WebCheck / gRPC** confirmation path only.  The `lastChk(fn_sign)` evidence shape — `status == OK` + `response.id == doc.server_fiscal_no` + non-empty `data_sign` — is gRPC-channel-specific.  The DFS HTTP / XML channel returns DFS-side tickets through `/fs/pck` / `/fs/doc` parsing rather than `lastChk` snapshots; a future M3+ task must implement DFS-ticket-driven KVT2 confirmation as a **separate helper**.  Do not claim DFS-side confirmation implemented in M3b under W12.

---

## Files (proposed)

- **NEW** `rust/prro/src/services/offline_sync/kvt2_confirm.rs` — typed surface + helper:
  - `pub enum Kvt2ConfirmSource { SentFresh, SentReplay, Kvt1Reentry }` — **MED-PR70-R5-01 fix**: helper signature carries the source context explicitly so identical lastChk evidence outcomes (NotFound, Mismatch) route to context-correct verdicts.
  - `pub enum Kvt2ConfirmOutcome { Acked { kvt1_raw_bytes: Vec<u8>, sent_replay_trace_attempt_no: Option<i64> }, Hold { reason: Kvt2ConfirmHoldReason, sent_replay_trace_attempt_no: Option<i64> }, StructuralDrift { reason: Kvt2ConfirmStructuralReason, sent_replay_trace_attempt_no: Option<i64> }, SentNotFoundDowngrade { trace_attempt_no: i64 } }` — every outcome variant carries the optional `sent_replay_trace_attempt_no` (MED-PR70-R6-01 fix); `Some(_)` only when source == `SentReplay`; threads the Envelope 1c-pre allocated row into the completing envelope so EVERY outcome path completes the recovery trace row (boot parity per `boot_phase.rs:516,707,803`).  Sent-fresh and Kvt1 re-entry contexts always carry `None` because they do not allocate a W12-owned trace row (stage_send's 4-pre trace row is already managed by stage_send itself for Sent-fresh; Kvt1 re-entry has no probe-allocated row).
  - `pub enum Kvt2ConfirmHoldReason { DpsTransport(String), DpsServer(String), DpsAuthorization(String), DpsDecode(String), LastChkDataSignEmpty }` — **all hold; doc state UNCHANGED per W0b §97-102**; drain-control = **STOP FN DRAIN** at this doc (NOT sibling-continue; preserves W0b latest-doc precondition); replayable next tick via the appropriate cohort re-entry seam (Sent OR Kvt1). **MED-PR70-R3-02 fix**: variant set aligned with the actual `DpsChannel::last_chk -> Result<CheckAck, DpsError>` surface at `rust/prro/src/transports/dps/channel.rs:24`. **MED-PR70-R5-01 fix**: `LastChkIdMismatch` moved from Hold to StructuralDrift (consistent across all three contexts — Mismatch is never recoverable through retry; DPS either has the doc with our id OR doesn't, the latter surfaces as NotFound).  Operator-confirmed: any id mismatch on a Sent-stamped doc indicates DPS / local state divergence past the App reconcile mutex; this is system-level, not per-doc operator-actionable.  `LastChkDataSignEmpty` stays in Hold because an Ok(CheckAck) with empty `data_sign` can be a transient DPS-side bug (M3a forensic note: rare, can resolve on retry).
  - `pub enum Kvt2ConfirmStructuralReason { ServerFiscalNoMissing, CasMissOnAdvance{from:DocState, to:DocState, observed:DocState}, LastChkIdMismatch{observed:String, expected:String}, NotFoundOutsideSentReplay{source:Kvt2ConfirmSource} }` — structural-invariant breaches surfacing as `BootError::Internal` for fail-loud forensics.  `NotFoundOutsideSentReplay` is the **MED-PR70-R5-01 fix** — NotFound from Sent-fresh OR Kvt1 re-entry contexts indicates state-machine drift (Sent-fresh just stamped server_fiscal_no from successful sendChk; Kvt1 was already advanced past Sent).  Routing per source context table below.
  - `pub async fn confirm_drain_doc(pool, dps, doc_id, fn_sign, source: Kvt2ConfirmSource) -> Result<Kvt2ConfirmOutcome, BootError>` — source-aware helper (MED-PR70-R5-01 fix); same body for all three contexts but routes evidence outcomes per the **source-context mapping matrix** below.  Helper calls **`dps.by_server_fiscal_no(fn_sign, &doc.server_fiscal_no)`** (HIGH-PR70-R8-01 fix; canonical typed lookup per `channel.rs:53-69`) OUTSIDE any `with_immediate` per I1.  This surface — NOT raw `last_chk` — is essential: `by_server_fiscal_no` performs the canonical empty-id → `DpsError::NotFound` mapping that the safe-redrive contract depends on; raw `last_chk` returns `Ok(CheckAck { id: "" })` on absent-history which would otherwise mis-route to the id-mismatch fail-loud arm.  For `Kvt2ConfirmSource::SentReplay`, the helper additionally allocates a `transport_trace` recovery row pre-DPS-call and threads its `attempt_no` into the `SentNotFoundDowngrade` variant (per MED-PR70-R5-02 lifecycle).

### Source-context routing matrix (MED-PR70-R5-01 fix)

Identical lastChk evidence outcomes route to context-specific verdicts:

Surface: `dps.by_server_fiscal_no(fn_sign, expected_id)` per `channel.rs:53-69` returns canonical typed mapping.  Rows correspond to `Result<CheckAck, DpsError>` variants the canonical helper produces.

| Evidence outcome (by_server_fiscal_no) | `SentFresh` | `SentReplay` | `Kvt1Reentry` |
|---|---|---|---|
| `Ok(CheckAck)` + non-empty data_sign (canonical id-match) | `Acked` | `Acked` | `Acked` |
| `Ok(CheckAck)` + empty data_sign | `Hold(LastChkDataSignEmpty)` → HoldFnDrain | `Hold(LastChkDataSignEmpty)` → HoldFnDrain | `Hold(LastChkDataSignEmpty)` → HoldFnDrain |
| `Err(DpsError::NotFound)` (empty ack.id; DPS zero history) | `StructuralDrift(NotFoundOutsideSentReplay{SentFresh})` → BootError::Internal | **`SentNotFoundDowngrade { trace_attempt_no }`** → Envelope 1c-post safe-redrive | `StructuralDrift(NotFoundOutsideSentReplay{Kvt1Reentry})` → BootError::Internal |
| `Err(DpsError::ServerFiscalIdMismatch{expected, actual})` (non-empty differing ack.id) | `StructuralDrift(LastChkIdMismatch)` → BootError::Internal | `StructuralDrift(LastChkIdMismatch)` → BootError::Internal | `StructuralDrift(LastChkIdMismatch)` → BootError::Internal |
| `Err(DpsError::Transport(_))` | `Hold(DpsTransport)` → HoldFnDrain | `Hold(DpsTransport)` → HoldFnDrain | `Hold(DpsTransport)` → HoldFnDrain |
| `Err(DpsError::Server(_))` | `Hold(DpsServer)` → HoldFnDrain | `Hold(DpsServer)` → HoldFnDrain | `Hold(DpsServer)` → HoldFnDrain |
| `Err(DpsError::Authorization(_))` | `Hold(DpsAuthorization)` → HoldFnDrain | `Hold(DpsAuthorization)` → HoldFnDrain | `Hold(DpsAuthorization)` → HoldFnDrain |
| `Err(DpsError::Decode(_))` | `Hold(DpsDecode)` → HoldFnDrain | `Hold(DpsDecode)` → HoldFnDrain | `Hold(DpsDecode)` → HoldFnDrain |

Rationale: by_server_fiscal_no's canonical empty-id → NotFound conversion (per `channel.rs:60-61`) gives the safe-redrive contract a typed observable; differing non-empty id surfaces as `ServerFiscalIdMismatch` (per `channel.rs:63-67`) — never collapsed with NotFound.  NotFound is context-discriminating (safe-redrive only from crash-recovery Sent-replay context); Mismatch is always structural (any DPS-side id divergence past App mutex is system-level); evidence-failure classes (DataSignEmpty, all wire-level DpsError) are uniformly Hold per W0b §97-102.
- **EDIT** `rust/prro/src/services/offline_sync/backlog_drain.rs`:
  - Add new `DocVerdict::HoldFnDrain { class: FailureClass, projection: HoldFnDrainProjection }` variant (third variant alongside `Advanced` + `Failed`).  **MED-PR70-R6-02 fix**: control behavior is shared across all HoldFnDrain outcomes (stop FN drain at held doc) but the projection field separates summary/finalize accounting per durable doc state:
    - `HoldFnDrainProjection::HeldAtKvt1` — doc state stays Kvt1; emitted **only** by `Kvt1Reentry` Hold (MED-PR70-R7-02 fix: prior R6 wording incorrectly attributed Sent-fresh Hold to HeldAtKvt1; Sent-fresh Hold occurs BEFORE Envelope 1a commit so durable state is still Sent, not Kvt1).
    - `HoldFnDrainProjection::HeldAtSent` — doc state stays Sent; emitted by **both** `SentFresh` Hold (pre-Envelope-1a; durable state still Sent because kvt2_confirm returns Hold before Sent→Kvt1 CAS executes) **and** `SentReplay` Hold.
    - `HoldFnDrainProjection::ErRedriveQueued` — doc state advanced to ErrorRetryable (SentNotFoundDowngrade only); NOT "held at KVT1" or "held at Sent" because durable state is ER awaiting next-tick ER class guard bounded redrive.
  - Drain loop semantics:
    - `Advanced` → sibling-continue (current behavior).
    - `Failed { manual_recon: true }` on pending-drain shift → escalate shift Manual + halt (current W9b behavior).
    - `Failed { _ }` otherwise → sibling-continue (current behavior for ER ProbeRequired / transient).
    - **`HoldFnDrain { .. }` → STOP this FN's drain at the held doc**; subsequent backlog docs in this tick NOT processed; NO shift escalation; NO sibling-continue.  This is the W0b latest-doc precondition enforcement (HIGH-PR70-R2-01 fix).  The `projection` field drives summary accounting and finalize-eligibility reason (see below).
  - Replace `apply_w12_confirmation` stub body with kvt2_confirm-backed implementation.  The function continues to be invoked from `process_via_stage_send` post-`StageSendOutcome::Sent` AND from the rewritten `process_via_w12_only`.
  - Rewrite `process_via_w12_only` (currently a stub that always returns `DeferredKvt1`): on `DocState::Kvt1` cohort dispatch, invoke `kvt2_confirm::confirm_drain_doc` with the doc's existing `server_fiscal_no` (from prior-tick `Sent → Kvt1` advance) → route outcomes per the Acked/Hold/StructuralDrift table.  This is the **Kvt1 re-entry seam** required for HIGH-PR70-R2-01 fix.  Source state Kvt1 means the doc was Held on a prior tick after a successful Sent→Kvt1 mid-tick advance (legacy stub) OR persisted Kvt1 from an in-tick crash AFTER Envelope 1a's Sent→Kvt1 step but before the Kvt1→Kvt2 step.  kvt2_confirm re-runs the same evidence check; the W0b latest-doc precondition holds because Hold stops further same-FN sends.
  - **Rewrite `process_via_lastchk_replay`** to be W12-aware (HIGH-PR70-R3-01 fix) while **preserving HIGH-C5-3 NotFound safe-redrive semantics** (HIGH-PR70-R4-01 reversal of prior R3 over-reach): on `DocState::Sent` cohort dispatch, delegate to `kvt2_confirm::confirm_drain_doc(..., Kvt2ConfirmSource::SentReplay)` which calls **`DpsChannel::by_server_fiscal_no(fn_sign, expected_id=doc.server_fiscal_no)`** (HIGH-PR70-R8-01 fix: NOT raw `last_chk` — `by_server_fiscal_no` is the canonical typed lookup that already maps empty `ack.id` → `Err(DpsError::NotFound)` and differing `ack.id` → `Err(DpsError::ServerFiscalIdMismatch)` per `channel.rs:46-69`; using raw `last_chk` would land empty-id absent-history into the id-mismatch fail-loud arm because raw decoder returns `Ok(CheckAck { id: "" })`).  The legacy `last_chk_probe::probe` helper is retired for all three W12-aware drain entry points — it remains internal to other M3a/W9b code unaffected by W12.  Mapping:
    - `Ok(CheckAck)` (canonical id-match by helper) with non-empty `data_sign` → `Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes, sent_replay_trace_attempt_no: Some(n) }` → Envelope 1a-replay chain (trace complete OK + Kvt1Raw + Sent→Kvt1 + Kvt1→Kvt2 + audit) then Envelope 2 (`stage_finalize::run`).  Audit `dispatch_via="kvt2_confirm_sent_replay"` for forensic distinction from Sent-fresh and Kvt1.  Summary `record_doc_advanced` with `via_lastchk_replay=true` (preserves W9b replay-flag semantic).
    - `Ok(CheckAck)` (id-match) with empty `data_sign` → `Kvt2ConfirmOutcome::Hold { reason: LastChkDataSignEmpty, sent_replay_trace_attempt_no: Some(n) }` → Envelope 1c-hold (trace complete no-state-change + audit) → `DocVerdict::HoldFnDrain { projection: HeldAtSent }`.
    - `Err(DpsError::NotFound)` (`by_server_fiscal_no` produces this on empty `ack.id` per `channel.rs:60-61` = DPS has zero history of `server_fiscal_no`) → **`Kvt2ConfirmOutcome::SentNotFoundDowngrade { trace_attempt_no: n }`** (Sent-replay arm exclusively).  This is the safe Pattern B redrive case per W9b spec HIGH-C5-3 contract: the doc is missing on DPS side, repeated lookups will never create the record — the recovery action is **resend through stage_send**, not poll forever.  Routing:
      - **Envelope 1c-post (Sent-replay NotFound only)**: completes the row allocated in Envelope 1c-pre via `transport_trace::complete_tx` with `retry_class=TransientRetry` + commits `Sent → ErrorRetryable` via `transition_state` + emits `OFFLINE_DRAIN_SENT_NOT_FOUND_DOWNGRADE` audit (all atomic; no DPS call inside envelope per I1).
      - Returns `DocVerdict::HoldFnDrain { class: FailureClass::SentNotFoundDowngrade, projection: HoldFnDrainProjection::ErRedriveQueued }` — **stops current FN drain at this doc** for W0b ordering (HIGH-PR70-R3-01 invariant; doc_i+1 NOT sent in this tick).
      - **Next tick**: doc is in `ErrorRetryable` state; cohort walker emits via ER dispatch arm (W9b ER class guard).  ER guard reads durable `retry_class='TransientRetry'` + `attempts_used` budget via `evaluate_er_redrive` → if under-budget calls `stage_send::run` (Pattern B `ErrorRetryable → Sending → Sent` redrive).  Post-redrive `Sent` outcome runs `kvt2_confirm` on the new server_fiscal_no.  If DPS now has the record → Match → Acked path.  If still NotFound → loop bounded by ER class guard's `MAX_BOOT_ATTEMPTS=5` budget cap → eventually ER budget-exhausted manual escalation (via boot_phase's `cas_error_retryable_budget_exhausted` M3a hardening pass).  No infinite Hold loop; no W0b ordering violation.
    - `Err(DpsError::ServerFiscalIdMismatch { expected, actual })` (`by_server_fiscal_no` produces this when `ack.id` is non-empty AND differs from `expected_id` per `channel.rs:63-67`) → `Kvt2ConfirmOutcome::StructuralDrift { reason: LastChkIdMismatch { observed: actual, expected }, sent_replay_trace_attempt_no: Some(n) }` → Envelope 1c-drift (trace complete with structural failure evidence) → `BootError::Internal`.  Operator-pinned: non-empty differing id on a Sent-stamped doc means DPS / local state divergence past the App reconcile mutex; system-level, not per-doc operator-actionable.
    - `Err(DpsError::Transport(_))` → `Kvt2ConfirmOutcome::Hold { reason: DpsTransport, sent_replay_trace_attempt_no: Some(n) }` → Envelope 1c-hold → `DocVerdict::HoldFnDrain { projection: HeldAtSent }`; doc stays Sent; next-tick Sent-replay re-entry.
    - `Err(DpsError::Server(_))` → `Kvt2ConfirmOutcome::Hold { reason: DpsServer, sent_replay_trace_attempt_no: Some(n) }` → Envelope 1c-hold → `DocVerdict::HoldFnDrain { projection: HeldAtSent }`.
    - `Err(DpsError::Authorization(_))` → `Kvt2ConfirmOutcome::Hold { reason: DpsAuthorization, sent_replay_trace_attempt_no: Some(n) }` → Envelope 1c-hold → `DocVerdict::HoldFnDrain { projection: HeldAtSent }`.
    - `Err(DpsError::Decode(_))` → `Kvt2ConfirmOutcome::Hold { reason: DpsDecode, sent_replay_trace_attempt_no: Some(n) }` → Envelope 1c-hold → `DocVerdict::HoldFnDrain { projection: HeldAtSent }`; transient malformed response.
    - `Err(DpsError::*)` other variants → `Kvt2ConfirmOutcome::Hold { reason: DpsServer(format!("{err:?}")), sent_replay_trace_attempt_no: Some(n) }` as defensive fallback (operator note: catch-all explicit so future DpsError additions land in Hold class with structural-drift escalation deferred until explicit operator decision).
  - **Why NotFound stays in the safe-redrive contract**: a Held Sent-replay NotFound that NEVER downgrades to ER would never resend → DPS never receives the doc → lastChk indefinitely returns NotFound → drain blocks at this doc forever (liveness hole flagged by operator finding HIGH-PR70-R4-01).  The boot-time `boot_phase` Sent-NotFound dispatcher already uses the `Sent → ErrorRetryable + TransientRetry stamp` pattern with successful M3a/M3b two-tick replay convergence; W12 reuses the same shape.  This is NOT a reversion of HIGH-PR70-R3-01: drain still stops at the held doc this tick (`HoldFnDrain` drain control preserved); next-tick redrive goes through the bounded ER class guard path; the W0b latest-doc precondition is preserved because no later same-FN send can occur this tick AND any next-tick redrive runs through the same FN-scoped drain loop under App reconcile mutex.
  - Route `Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes }` through the **two-envelope ladder** (see §"Transaction envelope shape").  Envelope 1 is state-specific:
    - Sent-source: persist `Kvt1Raw` artefact via `document_files::replace_tx` + CAS `Sent → Kvt1` + CAS `Kvt1 → Kvt2` + `OFFLINE_DRAIN_KVT2_ADVANCED` audit (atomic).
    - Kvt1-source: persist `Kvt1Raw` + CAS `Kvt1 → Kvt2` + audit (atomic).
    - Then Envelope 2 invokes `stage_finalize::run(pool, doc_id)` for `Kvt2 → Ack`.  Success: `DocVerdict::Advanced` + summary `record_doc_advanced(W12ConfirmOutcome::Acked, via_lastchk_replay=false)`.
  - Route `Kvt2ConfirmOutcome::Hold { reason, sent_replay_trace_attempt_no }` → `DocVerdict::HoldFnDrain { class: hold-specific FailureClass, projection }` per the projection matrix:
    - `Kvt2ConfirmSource::SentFresh` Hold → projection = `HeldAtSent` (doc state still Sent because Envelope 1a never committed; summary records via `record_doc_held_at_sent`).
    - `Kvt2ConfirmSource::SentReplay` Hold → projection = `HeldAtSent` (same; doc state still Sent; additionally runs Envelope 1c-hold to complete the allocated recovery trace row per MED-PR70-R6-01 fix; summary records via `record_doc_held_at_sent`).
    - `Kvt2ConfirmSource::Kvt1Reentry` Hold → projection = `HeldAtKvt1` (doc state still Kvt1; no trace allocation in this context; summary records via `record_doc_held_at_kvt1`).
    All three emit `KVT2_CONFIRM_HOLD` Warning audit with typed hold reason payload.  **No CAS to Manual.  No sibling-continue.**  Pending-drain shifts: Hold neither halts via Manual nor continues.
  - Route `Kvt2ConfirmOutcome::SentNotFoundDowngrade { trace_attempt_no }` → Envelope 1c-post (atomic complete_tx + Sent→ER + audit per MED-PR70-R5-02) → `DocVerdict::HoldFnDrain { class: FailureClass::SentNotFoundDowngrade, projection: HoldFnDrainProjection::ErRedriveQueued }`; doc state advances to `ErrorRetryable`.  Summary records via `record_doc_er_redrive_queued` — NOT `record_doc_held_at_kvt1` (MED-PR70-R6-02 fix: the durable state is ER, not Kvt1; finalize-eligibility reason is `DocsErRedriveQueued`).
  - Route `Kvt2ConfirmOutcome::StructuralDrift(_)` → `BootError::Internal` propagation (fail-loud; halts entire FN drain via existing `BootError` plumbing).
  - **Widen drain cohort to include `DocState::Kvt2`** (reverses MED-C5-4 W9b deferral): update `fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd` SELECT IN list to `('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')`.  Dispatch `Kvt2` cohort entries via new `process_via_w12_kvt2_advance` that calls `stage_finalize::run` (idempotent under M3a `AlreadyAcked` contract).
  - Add three new `DrainSummary` recording methods (MED-PR70-R6-02 projection split): `record_doc_held_at_kvt1(doc_id, hold_class)`, `record_doc_held_at_sent(doc_id, hold_class)`, `record_doc_er_redrive_queued(doc_id, downgrade_class)` — one per `HoldFnDrainProjection` variant.  Three private counters: `held_at_kvt1: u32`, `held_at_sent: u32`, `er_redrive_queued: u32`.  `finalize_eligibility` returns `NotEligible` with one of three reasons (`DocsHeldAtKvt1`, `DocsHeldAtSent`, `DocsErRedriveQueued`) whichever counter is > 0; precedence reported as multi-reason payload if multiple nonzero.  Distinct from legacy `advanced_to_kvt1 > 0` `DocsDeferredAtKvt1` (inert post-W12 — `W12ConfirmOutcome::DeferredKvt1` no longer produced by W12-aware paths).  Forensic `OFFLINE_DRAIN_PARTIAL` audit payload reports per-counter breakdown so operator dashboards distinguish "drain stopped because Kvt1 doc needs re-confirmation" from "drain stopped because Sent doc had transient hold" from "drain stopped because Sent doc downgraded to ER for next-tick stage_send Pattern B redrive".
  - Retire `W12ConfirmOutcome::DeferredKvt1` routing through `record_doc_advanced` → `advanced_to_kvt1` once W12 lands; keep the enum variant only for backward-compat with crash-recovered docs from pre-W12 history (no migration impact — variant is inert post-W12).
- **EDIT** `rust/prro/src/services/offline_sync/mod.rs` (`pub mod kvt2_confirm`).
- **EDIT** `rust/prro/src/db/repositories/fiscal_documents.rs` — widen drain-cohort SELECT IN list to include `KVT2` (MED-C5-4 W12 reversal).
- **KEEP** `rust/prro/src/services/reconciliation/boot_phase.rs::passive_hold_kvt1` as the primary boot-time handler for stale/pre-existing `Kvt1` docs outside drain context.  W12 does not change boot-time KVT1 dispatch.
- **KEEP** `rust/prro/src/services/reconciliation/boot_phase.rs:2468` `DocState::Kvt2` arm as the boot-time crash-recovery path between W12 envelopes.

---

## Day budget

~6.5 days (revised after six operator review passes; see §"Day-budget breakdown" for current commit allocation totals):
- First pass (MED-PR70-01 + MED-PR70-02): +explicit two-envelope transaction scope, +W0b-conformant Hold/StructuralDrift failure split, +mandatory crash-recovery convergence proofs, +drain cohort widening to include KVT2.
- Second pass (HIGH-PR70-R2-01 + MED-PR70-R2-02 + LOW-PR70-R2-03): +new `DocVerdict::HoldFnDrain` variant with drain-stop semantics, +Kvt1 re-entry seam (`process_via_w12_only` rewritten), +state-specific Envelope 1a/1b (Sent vs Kvt1 source), +`document_files::replace_tx(Kvt1Raw)` instead of `UPDATE fiscal_documents.kvt1_raw`, +4 dedicated W0b latest-doc precondition proofs.
- Third pass (HIGH-PR70-R3-01 + MED-PR70-R3-02 + LOW-PR70-R3-03): +Sent replay/crash-recovery seam W12-awareness (process_via_lastchk_replay rewrite, third entry point), +Hold enum aligned with actual DpsChannel::last_chk -> Result<CheckAck, DpsError> surface (LastChkStatusNotOk dropped; DpsTransport/DpsServer/DpsAuthorization/DpsDecode added), +fixture matrix recounted (23 total, one named fixture per retained Hold class), +retired HIGH-C5-3 NotFound→ER downgrade in favor of W12 Hold semantics.

See §"Day-budget breakdown" for commit-level allocation.

---

## Phasing (commit-level)

- **Commit 1 — helper + types**: `kvt2_confirm.rs` typed surface (`Outcome::{Acked, Hold(reason), StructuralDrift(reason)}`), evidence-check logic, no DB writes.  Unit tests against scripted `DpsChannel` stub.
- **Commit 2 — `DocVerdict::HoldFnDrain` + projection-aware summary + drain loop control**: add new verdict variant with `projection: HoldFnDrainProjection` field (HeldAtKvt1 | HeldAtSent | ErRedriveQueued per MED-PR70-R6-02 + R7-02); teach `drain()` loop to stop at any `HoldFnDrain` outcome (no further docs this tick); add three `DrainSummary` recording methods (`record_doc_held_at_kvt1` / `_held_at_sent` / `_er_redrive_queued`) + three counters; extend `FinalizeEligibility::NotEligible` with three reasons (`DocsHeldAtKvt1` / `DocsHeldAtSent` / `DocsErRedriveQueued`); update `finalize_eligibility()` to block on any nonzero W12 counter.  Unit tests for verdict / projection / summary / eligibility / multi-reason payload.
- **Commit 3 — cohort widening + Kvt2 dispatch arm**: `fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd` SELECT IN list extended to include `KVT2` (reverses MED-C5-4); `backlog_drain.rs::process_one_doc` adds `DocState::Kvt2` dispatch arm routing to new `process_via_w12_kvt2_advance` (calls `stage_finalize::run`).  Integration test: KVT2 cohort entry dispatches to stage_finalize and reaches Ack.
- **Commit 4 — Sent-source W12 wiring (Envelope 1a + Envelope 2)**: replace `apply_w12_confirmation` stub body — on `StageSendOutcome::Sent` outcome, call `kvt2_confirm::confirm_drain_doc(pool, dps, doc_id, fn_sign, Kvt2ConfirmSource::SentFresh)`; on `Acked` route through Envelope 1a (Kvt1Raw via `document_files::replace_tx` + `Sent → Kvt1` CAS + `Kvt1 → Kvt2` CAS + `OFFLINE_DRAIN_KVT2_ADVANCED` audit, all atomic) then Envelope 2 (`stage_finalize::run`).  NotFound/Mismatch from SentFresh context surface as `StructuralDrift` → `BootError::Internal`.
- **Commit 5 — Kvt1-source W12 wiring (Envelope 1b + Envelope 2)**: rewrite `process_via_w12_only` to invoke `kvt2_confirm::confirm_drain_doc(..., Kvt2ConfirmSource::Kvt1Reentry)` on doc's existing `server_fiscal_no`; on `Acked` route through Envelope 1b (Kvt1Raw + `Kvt1 → Kvt2` + audit) then Envelope 2.  This is the **Kvt1 re-entry seam** for prior-tick-Held docs (HIGH-PR70-R2-01 fix).  NotFound/Mismatch from Kvt1Reentry context surface as `StructuralDrift` → `BootError::Internal`.
- **Commit 5b — Sent-replay W12 wiring** (HIGH-PR70-R3-01 + HIGH-PR70-R4-01 + MED-PR70-R5-02 + HIGH-PR70-R8-01 fixes): rewrite `process_via_lastchk_replay` to invoke `kvt2_confirm::confirm_drain_doc(..., Kvt2ConfirmSource::SentReplay)` which internally allocates the recovery `transport_trace` row in **Envelope 1c-pre** (via `transport_trace::allocate_and_insert_tx` per `boot_phase.rs:1521-1542`), then calls **`dps.by_server_fiscal_no(fn_sign, &doc.server_fiscal_no)`** (HIGH-PR70-R8-01 canonical surface — NOT raw `last_chk` — to ensure empty-id → `Err(DpsError::NotFound)` mapping per `channel.rs:60-61` reaches the safe-redrive arm), and routes typed `Result<CheckAck, DpsError>` per the source-context matrix.  Match → Envelope 1a-replay (trace.complete OK + Kvt1Raw + Sent→Kvt1 + Kvt1→Kvt2 + audit) then Envelope 2 (`stage_finalize::run`).  NotFound → Envelope 1c-post (trace.complete TransientRetry + Sent→ER + audit) + `HoldFnDrain { projection: ErRedriveQueued }` (HIGH-C5-3 safe-redrive).  ServerFiscalIdMismatch → Envelope 1c-drift (trace.complete structural + audit) → BootError::Internal.  Transport/Server/Authorization/Decode/empty-data_sign → Envelope 1c-hold (trace.complete RetryableTransport|RetryableServer + audit) → `HoldFnDrain { projection: HeldAtSent }`.
- **Commit 6 — Hold path with projection split (W0b state-unchanged + drain-stop conformance + R6/R7 projection)**: route `Kvt2ConfirmOutcome::Hold { reason, sent_replay_trace_attempt_no }` through `DocVerdict::HoldFnDrain { class: hold-specific FailureClass, projection: HoldFnDrainProjection::{HeldAtKvt1 | HeldAtSent} }` per source-context projection matrix (Kvt1Reentry → HeldAtKvt1; SentFresh / SentReplay → HeldAtSent); emit `KVT2_CONFIRM_HOLD` Warning audit with typed reason payload; no CAS to Manual.  For SentReplay context, additionally runs Envelope 1c-hold to complete the allocated recovery trace row.  Drain loop stops at this doc for this tick (per Commit 2 semantics).  Pending-drain shifts: Hold neither halts via Manual escalation nor continues past held doc.
- **Commit 7 — StructuralDrift path**: route `Kvt2ConfirmOutcome::StructuralDrift(_)` as `BootError::Internal` propagation; halts entire FN drain.  NO per-doc Manual CAS (would mask systemic skew).
- **Commit 8 — fixture acceptance**: 14 helper-typed + 15 drain-integration fixtures (matches Fixture matrix §"33 fixtures total" — 5 retained Hold variants after R5 moved LastChkIdMismatch → StructuralDrift; 1 Acked + 5 Hold + 2 StructuralDrift base + SentNotFoundDowngrade + NotFound × 3 contexts + Mismatch × 3 contexts).
- **Commit 9 — W0b latest-doc precondition proofs (CORE)**:
  - `w12_hold_on_doc_i_blocks_doc_i_plus_1_send_in_same_tick` (no `stage_send::run` call for lnd>held_lnd).
  - `w12_kvt1_reentry_after_hold_advances_to_ack_when_evidence_recovers` (next tick re-enters via cohort, kvt2_confirm returns Acked, doc reaches Ack).
  - `w12_kvt1_reentry_holds_again_blocks_finalize_with_docs_held_at_kvt1` (next tick Hold again → still NotEligible).
  - `w12_pending_drain_hold_does_not_escalate_shift_but_does_not_continue` (shift state Opened**LocalPendingDrain unchanged; lnd>held_lnd docs not processed).
- **Commit 10 — interleave proof (W0b interleave precondition)**: `backlog_drain_no_next_send_before_current_lastchk` fixture (per umbrella plan §Task 12 acceptance) — extends Commit 9 with explicit assertion that DpsChannel `send_chk` count remains at exactly the number of Acked docs processed pre-Hold.
- **Commit 11 — crash-recovery convergence proof (MANDATORY)**: deterministic-replay fixtures covering 4 crash windows:
  - crash inside Envelope 1a → rolled back; doc in Sent → next tick `process_via_lastchk_replay` advances via stub Sent→Kvt1 then next-tick Kvt1 cohort re-entry → W12 → Ack.
  - crash inside Envelope 1b → rolled back; doc in Kvt1 → next tick Kvt1 cohort re-entry → W12 → Ack.
  - crash between Envelope 1 commit and Envelope 2 start → doc in `Kvt2` → boot recovery via `boot_phase::dispatch_pending_doc::DocState::Kvt2` lands `Ack` OR mid-tick cohort widening lands `Ack` in same tick.
  - crash mid-Envelope-2 → doc in `Kvt2` (stage_finalize internal CAS not applied) → same boot/in-tick recovery lands `Ack` idempotent.

---

## Acceptance criteria

### W12 core (from umbrella plan §Task 12 + W0b verdict §97-102 + HIGH-PR70-R2-01 fix)

1. W12 confirmation is invoked **only from drain control flow** — never from boot-time arbitrary `Kvt1` polling.  Three drain entry points (HIGH-PR70-R3-01 fix made `process_via_lastchk_replay` first-class):
   - `process_via_stage_send` post-`StageSendOutcome::Sent` (Sent-fresh path; doc has server_fiscal_no newly stamped this tick).
   - `process_via_w12_only` post-cohort-dispatch for `DocState::Kvt1` (Kvt1 re-entry seam, used for prior-tick Held docs after a successful Sent→Kvt1 mid-tick advance).
   - `process_via_lastchk_replay` post-cohort-dispatch for `DocState::Sent` (Sent replay/crash-recovery seam; W12-aware rewrite of the legacy W9b arm).  All three converge on `kvt2_confirm::confirm_drain_doc`.
2. **W0b interleave precondition**: no same-FN send may occur between `stage_send(doc_i)` and `lastChk(fn_sign)`.  Relies on W2 module-level enforcement + ADR-M3-A10 single-writer discipline; covered by App reconcile mutex (W9b carry-over).  Verified by `backlog_drain_no_next_send_before_current_lastchk` fixture.
3. **W0b latest-doc precondition (HIGH-PR70-R2-01 fix)**: on `Kvt2ConfirmOutcome::Hold`, the drain MUST stop processing subsequent docs on the same FN this tick.  Otherwise `lastChk(fn_sign)` semantics break: a Held `doc_i` is no longer the latest on FN after `doc_{i+1}` sends, and scoped W12 can never re-prove KVT2 for `doc_i`.  Verified by `w12_hold_on_doc_i_blocks_doc_i_plus_1_send_in_same_tick`.
4. Success evidence checks:
   - `lastChk.status == OK`;
   - `response.id == doc.server_fiscal_no`;
   - `response.data_sign` present AND non-empty.
5. On success: **two-envelope ladder** (per §"Transaction envelope shape"):
   - Envelope 1a (Sent source): persist `Kvt1Raw` via `document_files::replace_tx` + CAS `Sent → Kvt1` + CAS `Kvt1 → Kvt2` + `OFFLINE_DRAIN_KVT2_ADVANCED` audit, all atomic.
   - Envelope 1b (Kvt1 source / re-entry): persist `Kvt1Raw` + CAS `Kvt1 → Kvt2` + audit, all atomic.
   - Envelope 2: `stage_finalize::run` for `Kvt2 → Ack` (M3a unchanged).
   - `Kvt1Raw` bytes match `lastChk.data_sign` byte-for-byte (HIGH-C5-2 contract preserved; W9b lastChk-replay-match path's KVT1_RAW persistence pattern reused).
6. **W0b §97-102 state-unchanged conformance for evidence failures (R5-revised)**: on missing/empty `data_sign` OR DPS Transport/Server/Authorization/Decode errors → `Kvt2ConfirmOutcome::Hold(reason)` → `DocVerdict::HoldFnDrain` → doc state UNCHANGED → `KVT2_CONFIRM_HOLD` Warning audit with typed reason payload → **STOP FN DRAIN this tick**.  No CAS to Manual.  No sibling-continue past held doc.  **id mismatch** routes as `StructuralDrift(LastChkIdMismatch)` → `BootError::Internal` (MED-PR70-R5-01 reclassification; not a Hold).  `status != OK` is structurally unobservable through `DpsChannel::last_chk` surface (decoded into `DpsError::*` upstream per MED-PR70-R3-02; not a separate Hold variant).
7. **Structural-drift failures**: `ServerFiscalNoMissing` (stage_send 4-b invariant breach) OR `CasMissOnAdvance` (concurrent writer past App mutex) → `Kvt2ConfirmOutcome::StructuralDrift(reason)` → `BootError::Internal` propagation halts entire FN drain.  No per-doc Manual CAS (would mask systemic skew).
7a. **Sent-replay NotFound safe-redrive (HIGH-PR70-R4-01 reversal of R3 over-reach)**: on Sent-replay arm only, `Err(DpsError::NotFound)` from `dps.by_server_fiscal_no(fn_sign, &doc.server_fiscal_no)` → `Kvt2ConfirmOutcome::SentNotFoundDowngrade` → Envelope 1c (atomic `Sent → ErrorRetryable` + durable `TransientRetry` retry_class stamp + audit) → `DocVerdict::HoldFnDrain` drain control (stops FN drain this tick).  Next tick: ER cohort dispatch → W9b ER class guard `evaluate_er_redrive` reads `TransientRetry` + budget → bounded Pattern B redrive via `stage_send::run`.  Preserves HIGH-C5-3 + boot dispatcher Sent-NotFound shape; closes the indefinite-Hold liveness hole that R3 over-reach would have created.  NotFound from Sent-fresh OR Kvt1 re-entry routes as `StructuralDrift` instead (those contexts have no safe-redrive interpretation — would indicate state-machine drift past App mutex).
8. `passive_hold_kvt1` remains the primary boot-time handler for arbitrary/stale `Kvt1` docs outside drain context.  W12 does NOT change boot-time KVT1 dispatch.  Drain Kvt1 re-entry seam is distinct from boot-time stale-Kvt1 handling.
9. **Kvt2 boot-recovery path preserved**: `boot_phase::dispatch_pending_doc::DocState::Kvt2` arm (line 2468) continues to drive any orphaned `Kvt2` docs through `stage_finalize::run`; W12 does NOT touch this arm.

### Drain control + finalization unblock

10. New `DocVerdict::HoldFnDrain { class }` variant added; drain `drain()` loop stops processing subsequent docs in the current tick when this verdict surfaces.  Verified by `w12_hold_on_doc_i_blocks_doc_i_plus_1_send_in_same_tick` + `w12_pending_drain_hold_does_not_escalate_shift_but_does_not_continue` fixtures.
11. New `DrainSummary` triple of recording methods + counters (MED-PR70-R6-02 + R7-02 projection split):
    - `record_doc_held_at_kvt1(doc_id, hold_class)` + `held_at_kvt1: u32` — fed by `Kvt1Reentry` Hold only.
    - `record_doc_held_at_sent(doc_id, hold_class)` + `held_at_sent: u32` — fed by `SentFresh` Hold + `SentReplay` Hold.
    - `record_doc_er_redrive_queued(doc_id, downgrade_class)` + `er_redrive_queued: u32` — fed by `SentNotFoundDowngrade` only.
    `finalize_eligibility` returns `NotEligible` with one of three reasons (`DocsHeldAtKvt1`, `DocsHeldAtSent`, `DocsErRedriveQueued`) per highest-priority nonzero counter; multi-reason payload on `OFFLINE_DRAIN_PARTIAL` when two or more counters are nonzero.  Distinct from legacy `advanced_to_kvt1 > 0` `DocsDeferredAtKvt1` (inert post-W12 — `W12ConfirmOutcome::DeferredKvt1` no longer produced by W12-aware paths).
12. With at least one `Acked` outcome AND zero on all three W12 counters (`held_at_kvt1 == 0` AND `held_at_sent == 0` AND `er_redrive_queued == 0`) AND zero deferred-at-Kvt1 docs (legacy DeferredKvt1 — inert post-W12), `DrainSummary::finalize_eligibility` returns `Eligible`; `OFFLINE_DRAIN_COMPLETED` audit emits; node mode `GoingOnline → Online`; session `Draining → Closed`.  Verified by `backlog_drain_completes_finalize_after_w12_acked`.
13. With at least one Hold OR SentNotFoundDowngrade outcome, `DrainSummary::finalize_eligibility` returns `NotEligible` with the projection-correct reason (`DocsHeldAtKvt1` for Kvt1Reentry Hold; `DocsHeldAtSent` for SentFresh+SentReplay Hold; `DocsErRedriveQueued` for SentNotFoundDowngrade); `OFFLINE_DRAIN_PARTIAL` audit emits with per-counter breakdown; node + session stay in pre-drain state.

### Cohort widening (KVT2 added to drain cohort)

14. `fiscal_documents::list_drain_candidates_for_fn_ordered_by_lnd` SELECT IN list extended from `('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE')` to `('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')` — reverses MED-C5-4 deferral.
15. New drain dispatch arm `DocState::Kvt2 → process_via_w12_kvt2_advance` calls `stage_finalize::run` (idempotent under M3a `AlreadyAcked` contract); routes `Ok(Acked)` → `DocVerdict::Advanced`, `Ok(AlreadyAcked)` → `DocVerdict::Advanced` (no-op replay), `Err(_)` → typed failure surface.

### Pending-drain halt parity (W9b carry-over, REVISED per W0b conformance + drain-stop semantics)

16. `Kvt2ConfirmOutcome::Hold(_)` on a pending-drain shift (`OpenedLocalPendingDrain` | `ClosingLocalPendingDrain`) does NOT trigger shift Manual escalation (per W9b §3.5 gravity rule + W0b state-unchanged contract) **AND** does NOT sibling-continue (per HIGH-PR70-R2-01 latest-doc precondition).  Drain stops at held doc; shift state unchanged.
17. `Kvt2ConfirmOutcome::StructuralDrift(_)` halts entire FN drain via `BootError::Internal` propagation (different mechanism than W9b ER-guard's Manual CAS — structural drift is system-level, not operator-actionable).  No `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit (that's W9b ER-guard's manual-class halt only).

### Crash-recovery convergence (MANDATORY per MED-PR70-01 fix)

18. Crash inside Envelope 1a or 1b: rolled back atomically; doc state stays as cohort-emitted (Sent or Kvt1).  Next drain tick re-enters via the appropriate drain entry point.
19. Crash between Envelope 1 commit and Envelope 2 invocation: doc in `Kvt2` → boot recovery via `boot_phase::dispatch_pending_doc::DocState::Kvt2` (existing M3a path) OR mid-tick cohort widening dispatches to `process_via_w12_kvt2_advance` and lands `Ack` idempotently in the same tick.
20. Crash mid-Envelope 2 (`stage_finalize::run` internal): doc in `Kvt2` (CAS not applied) → same boot/in-tick recovery lands `Ack`.

### Fixture matrix (33 fixtures total)

W12-confirm helper (typed surface validation, no DB) — **14 fixtures**, one per retained Hold class + happy + StructuralDrift base + SentNotFoundDowngrade + NotFound × 3 contexts + Mismatch × 3 contexts:
- `kvt2_confirm_lastchk_match_returns_acked` (Acked success path)
- `kvt2_confirm_sent_fresh_lastchk_id_mismatch_returns_structural_drift` (MED-PR70-R5-01 context coverage: Mismatch from SentFresh routes as StructuralDrift)
- `kvt2_confirm_sent_replay_lastchk_id_mismatch_returns_structural_drift` (Mismatch from SentReplay routes as StructuralDrift)
- `kvt2_confirm_kvt1_reentry_lastchk_id_mismatch_returns_structural_drift` (Mismatch from Kvt1Reentry routes as StructuralDrift)
- `kvt2_confirm_missing_data_sign_returns_hold_data_sign_empty` (Hold::LastChkDataSignEmpty — W0b §99 conformance)
- `kvt2_confirm_dps_transport_error_returns_hold_dps_transport` (Hold::DpsTransport — DPS transport surface)
- `kvt2_confirm_dps_server_error_returns_hold_dps_server` (Hold::DpsServer — DPS server-side error surface)
- `kvt2_confirm_dps_authorization_error_returns_hold_dps_authorization` (Hold::DpsAuthorization — operator-actionable at lastChk time; collapsed to Hold per W0b §97-102 conformance)
- `kvt2_confirm_dps_decode_error_returns_hold_dps_decode` (Hold::DpsDecode — malformed lastChk response)
- `kvt2_confirm_no_server_fiscal_no_returns_structural_drift` (StructuralDrift::ServerFiscalNoMissing → BootError::Internal upstream)
- `kvt2_confirm_cas_miss_on_kvt1_to_kvt2_returns_structural_drift` (StructuralDrift::CasMissOnAdvance)
- `kvt2_confirm_sent_replay_lastchk_not_found_returns_sent_not_found_downgrade` (SentNotFoundDowngrade — Sent-replay arm only)
- `kvt2_confirm_sent_fresh_lastchk_not_found_returns_structural_drift` (NotFound from Sent-fresh = state-machine drift, distinct from Sent-replay safe-redrive)
- `kvt2_confirm_kvt1_reentry_lastchk_not_found_returns_structural_drift` (same; Kvt1 NotFound = drift)

(Helper subtotal: 14 fixtures — every Kvt2ConfirmOutcome variant has at least one named fixture, plus context-disambiguation matrix coverage for NotFound × 3 contexts AND Mismatch × 3 contexts per MED-PR70-R5-01 routing matrix.  Each fixture asserts (a) outcome enum variant, (b) post-CAS doc state, (c) DocVerdict drain-control projection, (d) audit event type + severity.)

Drain integration — W0b latest-doc precondition proofs (HIGH-PR70-R2-01 + R3-01 + R4-01 CORE) — **15 fixtures** (including 5 SentReplay NotFound safe-redrive proofs added in R4):
- `w12_sent_fresh_acked_runs_envelope_1a_chain_to_ack` (Sent-fresh → Acked → Envelope 1a Sent→Kvt1→Kvt2 + Envelope 2 Kvt2→Ack; Kvt1Raw persisted via document_files::replace_tx)
- `w12_sent_replay_acked_runs_envelope_1a_chain_to_ack` (Sent-replay/cohort → Acked → Envelope 1a chain; dispatch_via="kvt2_confirm_sent_replay"; via_lastchk_replay=true on summary)
- `w12_kvt1_reentry_after_prior_hold_advances_to_ack` (Kvt1 re-entry → Acked → Envelope 1b → Ack)
- `w12_sent_fresh_hold_blocks_doc_i_plus_1_send_in_same_tick` (Sent-fresh Hold: send_chk count proof; held doc stops drain; doc remains Sent)
- `w12_sent_replay_hold_blocks_doc_i_plus_1_send_in_same_tick` (Sent-replay Hold: same; HIGH-PR70-R3-01 evidence — Sent re-entry safe under W0b)
- `w12_kvt1_reentry_hold_blocks_doc_i_plus_1_send_in_same_tick` (Kvt1 re-entry Hold: same; doc remains Kvt1)
- `w12_consecutive_hold_ticks_keep_finalize_blocked_with_docs_held_at_kvt1` (two consecutive Hold ticks; NotEligible {DocsHeldAtKvt1} on both)
- `w12_pending_drain_hold_does_not_escalate_shift_but_does_not_continue` (OpenedLocalPendingDrain: Hold neither manual-escalates NOR sibling-continues; covers all three entry points)
- `backlog_drain_no_next_send_before_current_lastchk` (W0b interleave proof per umbrella plan)
- `backlog_drain_completes_finalize_after_w12_acked` (finalize unblock proof)
- `w12_sent_replay_not_found_atomically_downgrades_to_er_with_transient_retry_label` (Envelope 1c proof — atomic CAS + retry_class stamp + audit; HIGH-PR70-R4-01 core)
- `w12_sent_replay_not_found_blocks_doc_i_plus_1_send_in_same_tick` (HoldFnDrain drain control preserved; W0b ordering maintained)
- `w12_sent_replay_not_found_next_tick_redrives_through_er_class_guard_to_ack` (full safe-redrive cycle: tick-N NotFound → tick-N+1 ER → stage_send Pattern B → Sent → kvt2_confirm Match → Ack)
- `w12_sent_replay_not_found_negative_no_direct_sent_to_sending_resend` (negative proof: no whitelist bypass; Sent→Sending only via stage_send 4-pre after Sent→ER downgrade lands the doc in ER)
- `w12_sent_replay_not_found_indefinite_failure_bounded_by_er_budget` (after MAX_BOOT_ATTEMPTS=5 redrive ticks all returning NotFound, doc lands in RequiresManualReconciliation via ER class guard budget-exhausted path; no infinite Hold loop)

(Drain integration subtotal: 15 fixtures.)

Crash-recovery convergence (MANDATORY per MED-PR70-01) — **4 fixtures**:
- `replay_crash_inside_envelope_1a_rollback_then_next_tick_reaches_ack_via_sent_replay`
- `replay_crash_inside_envelope_1b_rollback_then_next_tick_reaches_ack_via_kvt1_reentry`
- `replay_crash_between_envelope_1_and_envelope_2_lands_ack_via_boot_or_intick_kvt2_dispatch`
- `replay_crash_mid_envelope_2_lands_ack_idempotent`

**Total: 33 fixtures** (14 helper + 15 drain integration + 4 crash-recovery).  Each retained `Kvt2ConfirmOutcome` variant has at least one named helper fixture; MED-PR70-R5-01 routing matrix has NotFound × 3 contexts + Mismatch × 3 contexts coverage; each entry point (Sent-fresh / Sent-replay / Kvt1) has matched Acked + Hold drain proofs; Sent-replay NotFound safe-redrive (HIGH-PR70-R4-01) has 5 dedicated fixtures covering atomic Envelope 1c-pre + 1c-post + drain stop + next-tick redrive + negative whitelist proof + budget-bounded liveness.

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
  - Mandatory fixture proofs (Acceptance §18-20) lock all three crash windows.
- **I8** strengthened: drain replay correctness through full ladder `OfflineLocalAck → Sending → Sent → Kvt1 → Kvt2 → Ack`.  Pre-W12 stub stopped at `Kvt1`; W12 closes the loop AND preserves W0b §97-102 state-unchanged contract for evidence failures (Hold class).  StructuralDrift class is system-level fail-loud, not doc-level escalation.
- **I9** preserved: graceful shutdown between any state pair leaves the doc in a recoverable state per Acceptance §18-20 proofs.  `passive_hold_kvt1` audit chain remains the boot-time forensic record for stale Kvt1 docs outside drain context.

---

## Carry-forwards from M3b (W9b ER-class-guard PR #69 self-review LOWs, deferred as scope-conformant)

W12 introduces no per-doc Manual CAS (failure semantics revised to Hold per W0b §97-102 + HIGH-PR70-R2-01 fix).  The W9b ER-class-guard LOWs below remain documented forensic notes; W12 does NOT inherit a `cas_kvt1_to_manual_via_drain` helper because no W12 outcome routes through Manual CAS.

- **LOW-1**: drain audit taxonomy does not have a distinct `OFFLINE_DRAIN_ER_BUDGET_EXHAUSTED` event_type (operator scope authorizes drain-specific audit projection).  Not applicable to W12; W12 uses `KVT2_CONFIRM_HOLD` + `OFFLINE_DRAIN_KVT2_ADVANCED` audit types.
- **LOW-2**: in W9b ER class guard, `emit_doc_failed` runs in a separate envelope from the CAS+ESCALATED audit.  W12 does NOT introduce an analogous gap — Envelope 1 (Sent or Kvt1 source) atomically emits `OFFLINE_DRAIN_KVT2_ADVANCED` inside the same `with_immediate` as the CAS chain via `audit_log::append_tx`.  W9b retroactive fix is out of W12 scope.
- **LOW-3**: drain CAS-helper returns `Err(BootError::Internal)` on non-Applied (stricter than boot's `Ok(bool)`).  W12 follows the same fail-loud pattern via `Kvt2ConfirmOutcome::StructuralDrift` → `BootError::Internal`.  Consistent with W9b convention; no new variance introduced.

## Carry-forwards from M3b W14a-2a (operator-confirmed)

- **LOW**: direct test for `shifts::TransitionOutcome::Conflict` variant (~10 LoC).  Optional addition during W12 if scope permits; otherwise defer to W13 handoff cleanup.

---

## Day-budget breakdown

| Slice | Day | Detail |
|---|---|---|
| Commit 1 (helper + types) | 0.5 | source-aware typed surface (Kvt2ConfirmSource + Kvt2ConfirmOutcome) + routing matrix + evidence checks; MED-PR70-R5-01 + R5-02 lifecycle |
| Commit 2 (HoldFnDrain verdict + summary + eligibility) | 0.5 | new DocVerdict variant; drain loop control; held_at_kvt1 counter; NotEligible reason |
| Commit 3 (cohort widening + KVT2 dispatch) | 0.25 | SELECT IN widening + new process_via_w12_kvt2_advance arm |
| Commit 4 (Sent-source W12 wiring) | 0.5 | Envelope 1a (Kvt1Raw + Sent→Kvt1 + Kvt1→Kvt2 + audit) + Envelope 2 chain |
| Commit 5 (Kvt1-source W12 wiring) | 0.5 | process_via_w12_only rewrite; Envelope 1b + Envelope 2 chain |
| Commit 5b (Sent-replay W12 wiring, HIGH-PR70-R3-01 + R4-01) | 0.75 | process_via_lastchk_replay rewrite; W12-aware Acked/Hold/StructuralDrift + SentNotFoundDowngrade; Envelope 1c (Sent→ER + retry_class stamp); preserves HIGH-C5-3 safe-redrive |
| Commit 6 (Hold path) | 0.25 | DocVerdict::HoldFnDrain routing + KVT2_CONFIRM_HOLD audit |
| Commit 7 (StructuralDrift path) | 0.25 | BootError::Internal propagation |
| Commit 8 (33-fixture acceptance) | 0.75 | scripted DpsChannel stub fixtures: 14 helper-typed + 15 drain integration + 4 crash-recovery |
| Commit 9 (W0b latest-doc precondition proofs) | 0.5 | 4 fixtures per Acceptance §3 + §10 (CORE HIGH-PR70-R2-01 verification) |
| Commit 10 (interleave proof) | 0.25 | extends Commit 9 with send_chk count assertion |
| Commit 11 (crash-recovery convergence, MANDATORY) | 0.5 | 4 deterministic-replay fixtures (#18-20) |
| Review rounds + polish | 0.5 | per M3b convention (1-3 rounds typical for hot-zone PRs) |

**Total: ~6.5d** (revised after six operator review passes).  R3 added third W12 entry point; R4 added `SentNotFoundDowngrade` + Envelope 1c safe-redrive; R5 added source-aware helper + full transport_trace lifecycle; R6 added per-outcome trace completion (Envelope 1a-replay + 1c-hold + 1c-drift) + HoldFnDrainProjection split (HeldAtKvt1 / HeldAtSent / ErRedriveQueued).  Previous totals: 1.5–2d (umbrella) → 2.5–3d (R1 MEDs) → ~5d (R2 HoldFnDrain) → ~5.5d (R3) → ~6d (R4) → ~6.25d (R5) → ~6.5d (R6).

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
    "W12 invoked only from drain control flow (Sent post-stage_send OR Kvt1 cohort re-entry); never boot-time arbitrary KVT1 polling",
    "lastChk evidence checks: status==OK + id==server_fiscal_no + non-empty data_sign",
    "Sent-source: Envelope 1a atomically persists Kvt1Raw via document_files::replace_tx + Sent→Kvt1 + Kvt1→Kvt2 + audit",
    "Kvt1-source (re-entry): Envelope 1b atomically persists Kvt1Raw + Kvt1→Kvt2 + audit",
    "Envelope 2: stage_finalize::run Kvt2→Ack (M3a unchanged, idempotent AlreadyAcked)",
    "W0b §97-102 state-unchanged conformance for evidence failures (Hold class, no Manual CAS)",
    "HIGH-PR70-R2-01 W0b latest-doc precondition: DocVerdict::HoldFnDrain stops FN drain at held doc (NOT sibling-continue)",
    "next-tick re-entry: Kvt1 cohort dispatch re-runs kvt2_confirm on still-latest doc",
    "StructuralDrift class (ServerFiscalNoMissing / CasMissOnAdvance) → BootError::Internal halt; not per-doc Manual",
    "drain cohort widened to include KVT2 (reverses MED-C5-4); new process_via_w12_kvt2_advance arm",
    "Kvt2 boot-recovery path (boot_phase.rs:2468 DocState::Kvt2 arm) preserved unchanged",
    "no same-FN send interleave before current lastChk (W2 mutex + ADR-M3-A10 + Hold stop-drain semantics)",
    "drain Eligible arm unblocks finalize after Acked outcomes; held_at_kvt1 counter blocks NotEligible{DocsHeldAtKvt1}",
    "Pending-drain Hold does NOT escalate shift AND does NOT continue past held doc",
    "kvt1_raw_bytes persisted byte-for-byte via document_files::replace_tx(Kvt1Raw) (HIGH-C5-2 contract preserved)",
    "crash-recovery convergence proofs MANDATORY: Envelope-1a/1b rollback / between-1-and-2 / mid-Envelope-2",
    "33 fixtures total: 14 helper-typed (1 Acked + 5 Hold variants + 2 StructuralDrift base + SentNotFoundDowngrade + NotFound × 3 contexts + Mismatch × 3 contexts via MED-PR70-R5-01 source-context routing matrix) + 15 drain integration + 4 crash-recovery convergence",
    "MED-PR70-R5-01: source-aware helper signature confirm_drain_doc(..., source: Kvt2ConfirmSource) explicitly distinguishes SentFresh / SentReplay / Kvt1Reentry contexts; LastChkIdMismatch moved Hold → StructuralDrift consistently across all 3 contexts; NotFound routes via context-discriminating matrix (SentReplay → SentNotFoundDowngrade safe-redrive; Sent-fresh + Kvt1 re-entry → StructuralDrift)",
    "MED-PR70-R5-02: Envelope 1c split into 1c-pre (transport_trace::allocate_and_insert_tx allocates fresh recovery row pre-lastChk) + 1c-post (transport_trace::complete_tx completes that exact row with retry_class=TransientRetry; never rewrites prior completed stage_send attempts); mirrors boot_phase.rs:1521-1542 + boot_phase.rs:743-770 reference pattern; ER guard's last_attempt_retry_class_for reads the freshly-completed row via attempt_no DESC LIMIT 1",
    "HIGH-PR70-R4-01: Sent-replay NotFound preserves HIGH-C5-3 safe Pattern B redrive via SentNotFoundDowngrade outcome (atomic Sent→ER + TransientRetry stamp + audit), bounded by ER class guard MAX_BOOT_ATTEMPTS=5 budget; HoldFnDrain drain control preserves W0b ordering"
  ],
  "blockedBy": ["W0b", "W1", "W2", "W3", "W9b", "W9b-er-class-guard"],
  "unblocks": ["W13", "M3b-closure-final", "Phase-6-pilot-acceptance"],
  "operatorFindingsClosed": ["MED-PR70-01", "MED-PR70-02", "HIGH-PR70-R2-01", "MED-PR70-R2-02", "LOW-PR70-R2-03", "HIGH-PR70-R3-01", "MED-PR70-R3-02", "LOW-PR70-R3-03", "HIGH-PR70-R4-01", "MED-PR70-R5-01", "MED-PR70-R5-02", "MED-PR70-R6-01", "MED-PR70-R6-02", "LOW-PR70-R6-03", "MED-PR70-R7-01", "MED-PR70-R7-02", "LOW-PR70-R7-03", "HIGH-PR70-R8-01", "LOW-PR70-R8-02"]
}
```
