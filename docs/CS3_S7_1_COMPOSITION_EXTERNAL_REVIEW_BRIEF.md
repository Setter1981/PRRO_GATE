# CS-3 S7-1 — Composition Impl-Design: EXTERNAL (model-decorrelated) Review Brief

**You are an independent, adversarial reviewer with a DIFFERENT model lineage than the author.** Your
job: find any SOUNDNESS flaw in the implementation design below BEFORE ~1500 lines are written against
it. This is a **release-critical live cutover** of a Ukrainian PRRO fiscal gateway: it kills a *live
double-issue bug* (a fiscal receipt transmitted to the tax service **twice**) on the write hot path.
A wrong move here either **double-issues** (legal/financial exposure) or **bricks a cash register**
(a legit fiscal number permanently refused). Prior review rounds proved that model-decorrelated review
catches what same-model adversarial review misses — please be maximally skeptical and concrete.

You have **no repo access**; everything needed is inline. Ground your reasoning in the code facts given.
If a claim below is unverifiable from what's provided, say so rather than assuming.

---

## 0. What is being decided (and what is FROZEN)

The high-level design is **FROZEN** after ~10 prior review rounds — do **not** re-litigate it:
- The wire (single DPS RPC `send_chk_observed`) relocates behind a sealed `submit_authorized`.
- A per-document **delivery reservation** ledger enforces call-once.
- The legacy post-wire block ("4-b") and the `-12` MAC-recovery loop are deleted.

This brief asks you to review only the **narrow implementation decisions** (Q1–Q5) that the frozen
design delegated to implementation, plus the **composed control flow**. Focus your fire on: **P2**
(≤1 wire per document lifetime, across all callers/recoveries/crashes), **byte-neutrality** for
unchanged rows (a "4th delta" is a design failure), **crash-safety** across the new two-transaction
split, and **BRICK/liveness** (no legit FN permanently stuck).

---

## 1. The double-issue invariants

- **P2:** at most ONE `send_chk_observed` per `document_id` lifetime — across all 7 callers of the send
  entrypoint, all recovery/redrive paths, and all crash/restart interleavings.
- **P3:** an online-origin issuance must extend the FN hash-chain from the last issued doc; the chain
  seed (`node_state.last_known_unsigned_xml_sha256`) must equal the new doc's `previous_hash` — checked
  **before** the wire (a mismatch = refuse, zero wire).
- **Frozen invariant #1:** no network/crypto call inside a SQLite write transaction (`BEGIN IMMEDIATE`).
- **Frozen invariant #9:** graceful shutdown / crash-safety beats "finishing fast".

## 2. Landed foundation primitives (INACTIVE today; the cutover wires them live)

All in `db/repositories/delivery_reservation.rs` unless noted. A reservation row has
`state ∈ {RESERVED_NOT_STARTED, CALL_STARTED, OUTCOME_OBSERVED}` and
`apply_state ∈ {NULL, PENDING_APPLY, APPLIED}`.

- **`authorize_submission(tx, row: NewReservation, call_started_at) -> Result<Authorization, AuthorizeError>`**
  runs entirely in the caller's `BEGIN IMMEDIATE`:
  (1) call-once guard `SELECT EXISTS(... document_id=? AND call_started_at IS NOT NULL)` → `CallOnceAlreadyStarted`;
  (2) INSERT `RESERVED_NOT_STARTED` (`attempt_no = COALESCE(MAX,0)+1`), backstopped by a `no_replace` trigger;
  (3) generation advance (`delivery_generation += 1`, set `active_delivery_reservation_id`);
  (4) CAS `RESERVED_NOT_STARTED → CALL_STARTED`. Returns a sealed token.
- **`Authorization`** — `#[derive(Debug, PartialEq, Eq)]`, **NOT `Clone`**, all fields private, only
  minted by `authorize_submission`. Carries `{reservation_id, document_id, attempt_no,
  authorized_generation, envelope_hash:[u8;32], dps_protocol_id, protocol_contract_version,
  capability_profile_version:Option<i64>, endpoint_config_revision:Option<i64>}`.
- **DB call-once index (crash/2-connection-proof):** migration 035 partial-unique
  `ux_delivery_document_ever_started ON delivery_reservation(document_id) WHERE call_started_at IS NOT NULL`
  — at most ONE row per `document_id` may ever carry a non-NULL `call_started_at`.
- **`submit_authorized(channel, port_binding: &DpsProtocolBinding, auth: Authorization, envelope,
  doc_type) -> Result<AttemptObservation, SubmitRefused>`** (`services/write_path/submit.rs`) — the
  SOLE production wire fn: rebind guard `SHA256(envelope.check_sign) == auth.envelope_hash()`
  → `EnvelopeRebind`; AO-2 5-binding echo vs `port_binding` → `BindingMismatch`; then **exactly one**
  `channel.send_chk_observed(envelope)`; returns `AttemptObservation::from_authorization(auth, evidence)`
  which **consumes `auth` by value**. `AttemptObservation` is non-`Clone`, carries no channel, cannot wire.
- **`record_outcome(tx, obs: &AttemptObservation, outcome: &ObservedOutcomeV1, evidence:
  &EvidenceDiscriminant) -> Result<(), RecordError>`** — full-authority CAS (9-column WHERE incl.
  `authorized_generation`, the 5-binding, `state='CALL_STARTED'`) → `CALL_STARTED → OUTCOME_OBSERVED +
  PENDING_APPLY` + persists axes {submission_certainty, response_provenance, routing_class, node_effect,
  remote_correlation_id, evidence_kind/text/code/digest}; `rows_affected != 1 → AuthorityMismatch`
  (hard error, no fallback). Then **early safety halt**: `NodeBlocked → set_mode_blocked_tx`;
  `SubmittedUnknown OR node_effect ∈ {MacReseedPending, OperatorEscalation, ProbeRequired, WrapperBug}
  → set_mode_stop_mode_tx`. It does **NOT** complete the transport_trace nor write the outcome audit
  (that is the gap the cutover fills — see Q2).
- **`apply_outcome(tx, reservation_id) -> Result<ApplyResult, ApplyError>`** — re-reads the persisted
  `evidence_kind` and projects: `Accepted` → stamp `server_fiscal_no`; if online-origin
  (`offline_fiscal_no IS NULL`) advance the seed **unconditionally** (`node_advance_seed`) + set
  `seed_advanced`; doc CAS `Sending → Sent`. `Rejected` split: offline → `HeldNotAutoRelease`; online
  `MacReseedPending|OperatorEscalation → HeldNotAutoRelease`; `NodeBlocked → node_set_blocked` + doc
  `Rejected`; `FnConfigError → doc ErrorRetryable`; other → doc `Rejected`. `SubmittedUnknown` leaves →
  `HeldNotAutoRelease`. A **generation-CAS** (`authorized_generation == node_state.delivery_generation
  AND active_ptr == reservation_id`) gates everything: stale/superseded → idempotent no-op
  (`applied:false`, nothing mutated). On success: mark `APPLIED`, clear `active_delivery_reservation_id`.
  `ApplyResult { applied: bool, seed_advanced: bool, server_fiscal_no: Option<String> }`. It fires
  **ZERO shift edges** and writes **no** closing cash (the cutover adds these — Q3).
- **Boot-first reservation pass** (`services/reconciliation/reservation_boot_pass.rs`, wired global
  pre-FN-loop in `app.rs`): normalize every `CALL_STARTED` via `resume_crashed_reservation` (→
  `NoResponse{Crashed}` + PENDING + STOP, never wires); apply every `OUTCOME_OBSERVED + PENDING_APPLY`
  via `apply_outcome`; NC-03 (lost node_state) reconstruct+retry. `HeldNotAutoRelease` is an EXPECTED
  hold (log+continue, must not abort boot).

## 3. The legacy code being replaced

Current `run_one_attempt` (`services/write_path/stage_send.rs`) is a 3-segment "Pattern B":
- **4-pre `with_immediate` (:1244–1551):** source-state allowlist `{Signed, ErrorRetryable,
  OfflineLocalAck}` (:1269), signer guard, build `CheckEnvelope`, CAS `source → Sending`, stamp
  `submission_attempted`, allocate a `transport_trace` intent row, audit `STAGE_SEND_INTENT_MARKED`.
- **wire (:1568):** `dps_channel.send_chk_observed(envelope)` — the current, ungated double-issue site.
- **4-b `with_immediate` (:1710–1972):** post-wire CAS `Sending → target`, stamp `server_fiscal_no`,
  online seed advance (with an in-tx equality gate at :1800 that is **skipped when
  mac_recovery_attempts ≥ 1** — the sharp non-idempotent edge), online shift edges 3/10 + closing cash,
  `-11` node BLOCKED, complete `transport_trace`, `STAGE_SEND_RESULT`/routed audit.
- **`run` (:1031–1117):** a MAC-recovery loop wrapper; on a `-12` it calls `run_mac_recovery` (re-signs,
  rewrites XML/CMS, mutates `previous_hash`) and `continue`s → a **second wire**.

The double-issue bug is live-by-construction: the wire at :1568 is not gated by authorization, and
`ErrorRetryable` re-drives + the `-12` `continue` produce second wires.

## 4. The composed `run_one_attempt` (4 phases) — REVIEW THIS

```
run(pool, dps_channel, doc, _sign_ctx)  ->  run_one_attempt(pool, dps_channel, doc)   // MAC loop deleted

run_one_attempt:
  // PHASE 1 — AUTHORIZE tx (one BEGIN IMMEDIATE; replaces the 4-pre):
  //   * 4-pre body unchanged EXCEPT the source-state allowlist drops ErrorRetryable
  //     (so only a fresh Signed / OfflineLocalAck can seed a Sending) — call this R2.
  //   * BEFORE minting the reservation, for online-origin (offline_fiscal_no IS NULL):
  //         ensure!( node_state.last_known_unsigned_xml_sha256 == fiscal_document.previous_hash )   // P3
  //     (check + reservation insert + CALL_STARTED marker all in THIS one BEGIN IMMEDIATE)
  //   * CAS source -> Sending; stamp submission_attempted; allocate transport_trace intent row;
  //     audit STAGE_SEND_INTENT_MARKED  (all unchanged from the 4-pre)
  //   * auth = authorize_submission(tx, NewReservation{ ...fixed binding..., envelope_hash=SHA256(check_sign) }, now)
  //   -> returns Authorization + { envelope, attempt_no, doc_type } ;  refusals return early, ZERO wire
  //
  // PHASE 2 — WIRE (outside any tx): submit_authorized is the SOLE send_chk_observed
  let port_binding = production_dps_binding();                    // fixed const {FscoZzd, v1, None, None}
  let wire_started = now();
  let (obs, legacy) = submit_authorized(dps_channel, &port_binding, auth, envelope, doc_type)?;  // consumes auth
  let wire_finished = now();
  //
  // PHASE 3 — RECORD tx (one BEGIN IMMEDIATE):
  let decision  = route_send_result(legacy, doc_type, true);     // VERBATIM legacy routing (WireDecision)
  let forensics = match &legacy { Ok(_) => None, Err(e) => Some(extract_wire_forensics(e)) };
  let (classified, disc, outcome) = build_record_args(&obs);     // classify + EvidenceDiscriminant + ObservedOutcomeV1
  record_transaction(pool, &obs, &outcome, &disc, &decision, forensics, wire_started, wire_finished, doc, attempt_no):
       with_immediate:
         record_outcome(tx, obs, outcome, disc)?;                // axes + PENDING_APPLY + early STOP/BLOCKED (repo)
         let completion = build_attempt_completion(decision, forensics, started, finished);  // RELOCATED verbatim from 4-b
         transport_trace::complete_tx(tx, doc, attempt_no, completion)?;                      // (rows==0 -> error)
         append_stage_send_result_audit(tx, doc, decision, attempt_no, ...)?;                 // RELOCATED verbatim from 4-b
  //
  // PHASE 4 — APPLY orchestration (shared with the boot pass):
  apply_orchestration::apply_recorded_outcome(pool, obs.reservation_id())?:
       let ctx = load_apply_context(pool, res_id);               // read doc_type, shift_id, offline_fiscal_no (pool read)
       let closing_cash = derive_closing_cash(pool, ctx);        // OUTSIDE tx (invariant #1); relocated from 4-b :1629-1700
       with_immediate:
         let res = apply_outcome(tx, res_id)?;                   // doc CAS + SFN + seed + APPLIED + pointer-clear (repo)
         if res.applied && ctx.online && res.server_fiscal_no.is_some() {
            match ctx.doc_type {
               ShiftOpen               => confirm_shift_edge(tx, ..., Opening->Opened, "edge3_open", None),
               ZReport | ShiftClose    => confirm_shift_edge(tx, ..., Closing->Closed, "edge10_close", closing_cash),
               _ => {}
            }
         }
  //
  Ok(stage_send_outcome_from(decision, attempt_no, forensics))   // Sent{sfn} / Routed{...} — verbatim :1974-1994
```

**Deletions (all in the same atomic commit):** the whole legacy 4-b `stage_send.rs:1710-1972`; the MAC
loop `:1048-1116` (a `-12` is now a recorded HOLD — `record_outcome` sets STOP, `apply_outcome` returns
`HeldNotAutoRelease` — never a re-sign or second wire); the `EmptyServerFiscalNo` early-return guard
`:1594-1598` (an empty server id becomes the typed `OkButNoFiscalNumber` HOLD leaf, one of 3
pre-adjudicated deltas). Also landing in the same commit (out of scope for THIS brief unless they break
the above): R1 delete the `(ErrorRetryable, Sending)` transition edge; R6 retire the 3 `ErrorRetryable`
re-drive callers to RMR/STOP; retarget 2 `Sent+NotFound` producers.

## 5. The 5 delegated implementation decisions

- **Q1 (binding):** `port_binding` and the token's binding come from a fixed const
  `production_dps_binding() = {FscoZzd, ProtocolContractVersion(1), None, None}` (no `DpsChannel::binding()`
  method added). Rationale: every binding literal in-tree is that tuple; cpv/ecr are provably always
  `None`; there is no config column; the token binding and the port binding derive from the SAME const,
  so AO-2 always echoes on the happy path (AO-2 stays a *rebind/tamper guard on the authorize→wire seam*,
  the per-doc `envelope_hash = SHA256(check_sign)` being the load-bearing part). **Q for you:** is a fixed
  binding sound, or does any live path depend on a per-FN/per-protocol binding? Does making AO-2's binding
  half a tautology weaken P2/P3 anywhere?
- **Q2 (trace+audit):** `submit_authorized`'s return type changes to surface the legacy
  `Result<CheckAck, DpsError>` alongside the `AttemptObservation`; the caller runs
  `route_send_result(legacy, doc_type, true)` (byte-identical to the pre-cutover routing per the
  CS-3 3.2 "single-RPC" pin) and RELOCATES `build_attempt_completion` + the audit **verbatim** from 4-b
  into the RECORD tx — so unchanged rows stay byte-equal (the ApplyPlan graph pin requires this).
  **Q for you:** does surfacing the legacy result leak any P2 Layer-3 capability (the `AttemptObservation`
  is still the only authority-bearing value)? Does moving trace-complete + audit from the *combined* 4-b
  tx (which also did the doc CAS) into a *separate* RECORD tx that runs BEFORE the APPLY tx change any
  persisted row, `outcome_kind`, or audit payload for any evidence leaf (a 4th delta)?
- **Q3 (apply orchestration):** a new service fn `apply_recorded_outcome(pool, res_id)` (shared by live
  run + boot pass) reads doc/shift context, derives closing-cash outside the tx, then one `BEGIN
  IMMEDIATE`: `apply_outcome(tx)` + online shift edges 3/10 (gated `res.applied && ctx.online &&
  server_fiscal_no.is_some()` — claimed byte-equal to the 4-b gate `offline_fiscal_no.is_none()` inside
  the `Sent` arm). `apply_outcome`'s online seed advance is **unconditional** (the old in-tx equality
  gate is dropped because P3 moved it pre-wire). **Q for you:** does the fence + generation-CAS + pre-wire
  P3 equality FULLY subsume the deleted in-tx seed-drift `ensure!`, or can a chain-fork advance silently?
  Does the shift edge fire EXACTLY once across `record → apply → crash → boot re-apply` (given
  `apply_outcome`'s generation-CAS idempotency), or can it double-advance / never-advance?
- **Q4 (run shape / sign_ctx):** `run` collapses to a straight-line call (no MAC loop). `sign_ctx`
  becomes `_sign_ctx` (unused; the envelope is pre-signed; R3 removes MAC re-sign). **Q for you:** with
  the MAC loop gone, is every previously-looping outcome now terminal-or-held correctly? Is there a
  routed leaf that the old loop handled that the straight-line path now drops or mis-terminalizes?
- **Q5 (two hashes):** the token/reservation `envelope_hash = SHA256(check_sign)` (CMS blob, rebind-
  checked); the trace `request_envelope_sha256 = SHA256(prost(gen::Check))` (the full wire proto,
  unchanged). Both computed in the authorize tx. `reservation_id` = a fresh random 16-byte id (backstopped
  by `UNIQUE(document_id, attempt_no)` + the `no_replace` trigger). **Q for you:** any hash confusion, or
  a `reservation_id` collision/rollback hazard under two concurrent authorizes for the same doc?

## 6. The crash-window matrix you must attack (Phase 3/4 split)

The old design did doc CAS + SFN + seed + shift + trace + audit in ONE 4-b tx. The new design splits into
RECORD tx (evidence + PENDING_APPLY + trace + audit + safety-halt) then APPLY tx (doc CAS + SFN + seed +
shift + APPLIED + pointer-clear). Attack EVERY window:

| crash after | reservation state at boot | claimed recovery | your job |
|---|---|---|---|
| wire, before RECORD | `CALL_STARTED` | boot `resume_crashed_reservation` → NoResponse{Crashed}+PENDING+STOP; operator completes | can it re-wire? is the wire outcome lost silently? |
| RECORD commit, before APPLY | `OUTCOME_OBSERVED + PENDING_APPLY` | boot applies via the SAME `apply_recorded_outcome` (gen-CAS idempotent) | does boot fire the shift edge / closing-cash correctly, exactly once? is closing-cash re-derived from durable state correct after crash? |
| mid-APPLY tx | PENDING_APPLY (uncommitted) or APPLIED | re-apply / no-op | any partial write? |
| after APPLIED | APPLIED | terminal no-op | any double shift / double seed? |

Also: `SubmitRefused` (EnvelopeRebind / BindingMismatch) fires AFTER `CALL_STARTED` is committed but
returns before any RECORD — the doc is left in `Sending` with a `CALL_STARTED` reservation and no
outcome. Is that a stuck non-terminal doc, or does the boot pass resolve it? (These refusals are
"can't happen" on the happy path — token binding == port binding by construction — but attack the
crash/regression case.)

## 7. What to return

For each finding: **severity** (DOUBLE_ISSUE / BRICK / CHAIN_FORK / NEUTRALITY_BREAK / CRASH_LOSS /
MINOR), the **concrete triggering interleaving or input**, the **specific decision/phase it breaks**, and
your **confidence**. An empty finding set ("the design is sound on the questions asked") is a valid,
valuable answer — say so explicitly per question rather than inventing hypotheticals. Prioritize
DOUBLE_ISSUE / CHAIN_FORK / BRICK / CRASH_LOSS. If you need a code fact not provided here to settle a
question, name exactly which fact.
