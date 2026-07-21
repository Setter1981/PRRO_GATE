# CS-3 S7-1 CUTOVER — grounded build sequence (post seam-map)

**Not a new design.** Design-of-record `CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` is FROZEN (§11
checklist) + round-2 external GO; impl-sequencing `CS3_S7_1_CUTOVER_IMPL_PLAN.md`. This doc records
the **grounded seams** (both halves) against the current tree (branch `cs3-de-slice7-cutover` off
`main`/`1999ff1`) so the atomic build + external review are pure execution. Every anchor verified by
direct read, 2026-07-21.

## Resolved edit-time pins (impl-plan §4.2)
- **legacy 4-b END = `stage_send.rs:1972`** (`.map_err(bridge_anyhow)?;`). Block = `:1710`..`:1972`
  (the whole 2nd `with_immediate`). `:1974-1995` = post-closure `StageSendOutcome` return;
  `:1997+` = recovery-override helpers (R3 territory).
- **`:1420` role = `source_state` re-derivation match** (mirror of the `:1269` top allowlist, feeds
  `transition_state → Sending`). R2 drops `ErrorRetryable` at BOTH `:1269` and `:1420` (keeps the
  `unreachable!` honest + the two allowlists in sync). `:1292` STOP-O3-1 guard's ER-arm becomes dead
  after R2 (leave; not the source allowlist).

## Grounded reservation-lifecycle seams (all `db/repositories/delivery_reservation.rs`; all INACTIVE)
- `authorize_submission(tx, row: NewReservation, call_started_at) -> Result<Authorization,AuthorizeError>` `:496`
  (call-once EXISTS `:513`, RN insert `:527`, gen advance `:532`, RN→CALL_STARTED CAS `:557`; cpv/ecr
  captured `:506-510`→token `:573`). `Authorization` `:297` non-Clone sealed (9 fields).
- `submit_authorized(channel, &port_binding, auth, envelope, doc_type) -> Result<AttemptObservation,SubmitRefused>`
  `submit.rs:45` — sole wire `send_chk_observed` `submit.rs:81`; rebind `SHA256(check_sign)==envelope_hash`
  + AO-2 5-binding echo BEFORE wire; consumes `auth` by value. `AttemptObservation` `:372` non-Clone,
  `from_authorization` `:390`, no wire capability.
- `record_outcome(tx, &AttemptObservation, &ObservedOutcomeV1, &EvidenceDiscriminant) -> Result<(),RecordError>`
  `:634` — 9-col authority CAS CALL_STARTED→OO+PENDING + axes; early STOP/BLOCKED `:698-721`.
  **GAP: does NOT complete `transport_trace` nor append outcome audit** (verified `:634-723`).
- `apply_outcome(tx, reservation_id) -> Result<ApplyResult,ApplyError>` `:795` — evidence_kind match
  `:882`; Accepted→SFN+online seed(UNCONDITIONAL)+doc(Sent); Rejected split (FnConfigError→ER `:920`
  LANDED); gen-CAS idempotent `:843`; APPLIED+pointer-clear `:946-964`.
  **GAP: fires ZERO shift edges / no closing-cash** (design §4 wants edges 3/10 + cash in the apply).
- Boot pass: `reservation_boot_pass::run(&ReconcileGuard, pool)` `reservation_boot_pass.rs:195`
  (normalize/apply/reconstruct/retry), wired `app.rs:576` GLOBAL pre-FN-loop. `apply_one` currently
  calls raw `apply_outcome` → **must call the shared apply orchestration** so boot also fires shift
  edges (§4.1 one projection).
- `sent_not_found_to_manual(tx, doc_id, fscl)` `sent_not_found.rs:67` (Sent→RMR + STOP + audit; trace
  owned by producer). `fn_fence_active_tx` `:204`; `ACTIVE_FENCE_STATE_PREDICATE` `:190`.

## Projection helpers (for record args + teeth)
- `prro_domain::delivery::classify(&SubmissionEvidence) -> ClassifiedOutcome` (`delivery/mod.rs:908`).
- `EvidenceDiscriminant::from_evidence(&SubmissionEvidence)` (`delivery/evidence.rs:207`); 11 leaf
  tags; `roundtrip_all_eleven_leaves` `evidence.rs:623`.
- `ObservedOutcomeV1::record(&ClassifiedOutcome, correlation, AuthorizedGeneration)` (see
  `tests/record_outcome.rs` helper pattern).

## Retirement anchors (R1-R7, verified)
- **R1**: delete `(ErrorRetryable, Sending)` `fiscal_documents.rs:257` (+ comment `:258-261`). Keep
  `:255 (Sending,ErrorRetryable)`, `:241 (ErrorRetryable,RMR)`, `:265 (ErrorRetryable,Rejected)`.
- **R2**: `stage_send.rs:1269` + `:1420` remove `ErrorRetryable`.
- **R3**: kill the `run()` MAC loop (`:1048-1116`): no `run_mac_recovery` `:1081`, no `Resigned=>continue`
  `:1082`, no 2nd attempt. A `-12` routed leaf → record `MacReseedPending` (STOP) → apply HELD.
  (`mac_recovery.rs:516-535` re-sign path retired from the live send loop.)
- **R4**: `boot_phase.rs` `cas_sent_to_error_retryable_from_probe` def `:950` / call `:2870`
  (outcome_kind **RetryableServer**) → `sent_not_found_to_manual`.
- **R5**: `offline_sync/kvt2_confirm.rs` `commit_sent_replay_envelope_1c_post` def `:1651` / call `:1021`
  (outcome_kind **RetryableTransport**) → `sent_not_found_to_manual` (preserve distinct kind, R-Q3).
- **R6**: `er_redrive_policy.rs:38` collapse `Redrive`(`:43`/return `:99`)→`EscalateManual{TransientRetry}`,
  DELETE the `Redrive` variant. 3 callers route to their already-wired `cas_*_to_manual` siblings:
  `online_convergence.rs` Redrive arm `:560` (sibling `:592`); `boot_phase.rs` `:3135` (siblings
  `:3175/:3191/:3211`); `backlog_drain.rs` `:1543` `process_via_stage_send` (sibling `cas_er_to_manual_via_drain`
  `:1696`).
- **R7 consumers**: `dispatch_error_retryable_by_class`, `kvt2_confirm.rs:1645-1649` (W9b ER-guard),
  `boot_phase.rs:947-948` (two-tick retry) — retarget/remove same commit.
- **#1**: `admin.rs:300 reset_stop_mode` — before the mode CAS `:341` add
  `SELECT COUNT(*) delivery_reservation WHERE fscl=? AND state='OUTCOME_OBSERVED' AND apply_state='PENDING_APPLY'`;
  `>0 → AdminError::PendingResolutionRequired` (new variant, enum `admin.rs:48`).

## New `run_one_attempt` shape (MAC loop collapses to straight-line)
1. **AUTHORIZE tx** (replaces 4-pre `:1244-1551`): allowlist {Signed,OLA} (R2), STOP-O3-1, signer,
   envelope build, **P3 online-origin equality `node_state.last_known_unsigned_xml_sha256 ==
   fiscal_document.previous_hash`** (§2.1), CAS source→Sending, mark, alloc trace, `authorize_submission`
   → `Authorization`. Any refusal/loser → 0 wire, tx rolls back → no token.
2. **WIRE** (outside tx): `submit_authorized` → `AttemptObservation` (1 `send_chk_observed`).
3. **RECORD tx**: `classify` + `EvidenceDiscriminant::from_evidence` → `record_outcome`; **+ GAP-fill:
   complete `transport_trace` + outcome audit**.
4. **APPLY orchestration** (service, shared live+boot): derive closing-cash outside tx (preserve
   `:1629-1700`); one tx: `apply_outcome` + **GAP-fill: `confirm_shift_edge` (online edges 3/10 +
   cash)**; seed advance stays unconditional (P3 moved it pre-wire). HELD = expected.

## Static-pin FLIP (part of the atomic commit)
Wiring the composition makes `stage_send::run` reference `authorize_submission`/`record_outcome`/
`apply_outcome` → the INACTIVE denylist pins **`migration_032::p03` + `migration_033::rg08`**
(`tests/support/inactive_lifecycle_scan.rs`) go RED. Retarget them to the **positive sole-seam**
(S7-P2-2): `send_chk_observed` EXACTLY 1 call-site (inside `submit_authorized`), `submit_authorized`
EXACTLY 1 caller (`stage_send::run`), the lifecycle fns called only by the sanctioned cutover sites +
the boot pass. Remove the now-active symbols from `UNIQUE_INACTIVE_LIFECYCLE` / re-scope the allowlist.

## Teeth status (corrected — foundation vs cutover)
- **LANDED (foundation, test INACTIVE primitives):** `ap01-10`/`rc01-08`/`az01-07`, `apply_plan_pin`
  (S7-APPLY-GRAPH), `s7_boot_reservation_pass` (S7-P4-BOOT + NC-03), `s7_2_stage_acquire_exclusion`
  (S7-FENCE), `cs3_evidence_matrix_conformance`.
- **TO-WRITE (cutover, drive LIVE composed `run()`):** S7-P2-1 sole-wire (2 concurrent run→1 wire +
  resume variant; revert-canary index/direct-wire), **S7-P2-2** static sole-seam (retarget p03/rg08 +
  compile-fail Authorization non-Clone), S7-P3-1 single seed-writer (extend `apply_outcome.rs` via
  cutover run), S7-P3-2 mac-divergence (`-12`→HELD, no 2nd wire), S7-P2-3 BRICK 3-caller matrix (R6),
  S7-P3-3 Sent+NotFound→RMR+STOP (R4/R5), S7-P3-4 pre-wire predecessor (§2.1).

## Test harness
Counting `StubDpsChannel` `tests/common/mod.rs:88` (`call_count()`). Drive via
`seed_signed_doc_with_xml` (`tests/write_path_stage4_send.rs`) → `stage_send::run(pool, &chan, doc, sctx)`.
Reservation seeding: `seed_doc`/`authorize`/`record` (`tests/apply_outcome.rs` / `record_outcome.rs`).

## Build order (one atomic commit; all §8 teeth RED-first + revert-canary before flip)
Phase-T teeth → Phase-C (composition core → 3 gap-fills → delete 4-b `:1710-1972` → R1-R7 → #1 →
static-pin flip) → Phase-G (empty-in-flight gate: reservation-less legacy SENDING/ER → RMR/STOP, NOT
inferred from `transport_trace`). Full gate (fmt · clippy --all-features -D · nextest --all-features ·
inventory re-mint) → decorrelated internal re-audit + external model-decorrelated review on the atomic
diff → merge on the standing GO.

**Nothing implemented yet. Live send-path code untouched.**
