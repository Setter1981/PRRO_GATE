# CS-3 Slice 7 — whole-fence live cutover plan

**Status:** PLAN ONLY. No live-path activation is authorized by this document.

**Ground truth:** local worktree `/home/setter/prro-gate-wt/cs3-de-slice2`,
branch `cs3-de-slice3`, commit `d5227e4` (Slices 1–6 foundation). This plan is
deliberately grounded on that local state, not on `origin/main`.

**Goal:** make the existing CS-3 D/E machinery authoritative on the live fiscal
path while preserving:

- **P2:** at most one DPS `send_chk` wire call per `document_id` over its lifetime;
- **P3:** the per-FN receipt chain never forks;
- **P4:** a crash cannot silently lose or double-apply an observed outcome;
- **BRICK:** after a verified resolution, a legitimate next document can proceed.

No new table, reservation state, node mode, retry class, domain aggregate, or
resubmit affordance is introduced. Slice 7 reuses `delivery_reservation`,
`Authorization`, `EvidenceDiscriminant`, `ObservedOutcomeV1`, `STOP_MODE`, and
the already-landed apply/operator helpers.

## 0. Readiness verdict on the local foundation

The foundation is suitable for planning the cutover, but it is **not yet ready
for activation**. Five gaps must be closed before the first live call is routed
through it:

1. **The production record boundary is absent.** Tests currently update
   `delivery_reservation` directly and explicitly say that the production
   `record_outcome` path “lands with the wiring”
   (`rust/prro/tests/apply_outcome.rs:106-139`). `apply_outcome` already requires
   `OUTCOME_OBSERVED + PENDING_APPLY`
   (`delivery_reservation.rs:459-617`), so activating authorization without the
   record transaction would convert a wire result into an unrecoverable
   `CALL_STARTED` crash-shaped hold.
2. **Evidence is still optional at `OUTCOME_OBSERVED`.** Migration 036 describes
   itself as validate-if-present and permits all four evidence columns to remain
   NULL (`036_delivery_reservation_evidence_union.sql:35-50,100-125`). Because
   this migration has not been released, tighten 036 in place before merge:
   every transition to `OUTCOME_OBSERVED` must carry exactly one valid evidence
   leaf. Do not add another state or table.
3. **The authorization is not yet a real consumed capability.**
   `Authorization` is `Clone` and has public fields
   (`delivery_reservation.rs:251-264`). Before it guards live I/O, keep the same
   type but make its fields private, remove `Clone`, expose read-only accessors,
   copy the already-stored protocol binding and envelope hash into its private
   fields, and consume it by value at the sole wire function. This prevents a
   second call from a copied or fabricated token and prevents rebinding the
   authority to different bytes. This extends the existing type; it does not
   add another token/entity.
4. **Operator completion is only the regular-online subset.**
   `complete_operator_pending` intentionally returns
   `ShiftFamilyNotSupported` and `OfflineCohortCleanupRequired`
   (`delivery_reservation.rs:760-806,852-876`). It also has no production caller;
   neither do `resume_crashed_reservation` and `list_call_started_without_outcome`.
   Complete the already-designed origin × document-family matrix and wire the
   existing `reset_stop_mode` surface before cutover. Otherwise P2 holds by
   bricking a valid FN, which fails BRICK.
5. **`apply_outcome` is not yet the full live fiscal projection.** It stamps
   SFN/seed and may set BLOCKED, but it does not transition the fiscal document
   out of `Sending`, complete the live trace/audit, or apply shift edges
   (`delivery_reservation.rs:459-617`; the current tests assert only those
   partial effects at `tests/apply_outcome.rs:217-314`). It also auto-releases
   every online `Rejected` row, so the online `-12 MacRecovery` and
   `-6 OperatorEscalation` leaves are not held as required. Activating this
   implementation unchanged could clear the FN pointer while its document
   remains `Sending`, and could release a chain-repair/operator hold. Complete
   the existing apply projection before live ownership moves from legacy 4-b.

These are completion items for the existing design, not new architecture.

## 1. Actual live surface that Slice 7 must cover

### 1.1 Seven callers of `stage_send::run`

| Caller | Live location | Context that must be pinned |
|---|---|---|
| Inline write | `services/write_path/inline.rs:910` | Fresh online issue |
| Backlog document send | `services/offline_sync/backlog_drain.rs:1321` | Offline-origin drain |
| Boundary document send | `services/offline_sync/backlog_drain.rs:2959` | Offline session boundary |
| Online convergence | `services/reconciliation/online_convergence.rs:561` | Recovery path currently described as redrive |
| Boot transient dispatcher | `services/reconciliation/boot_phase.rs:3072` | `ErrorRetryable` recovery |
| Boot online ladder | `services/reconciliation/boot_phase.rs:3685` | Prepared/signed recovery |
| Boot pending dispatcher | `services/reconciliation/boot_phase.rs:3943` | Pending document recovery |

The seven callers must not each implement their own authorization protocol.
The cutover belongs centrally inside `stage_send::run`; caller-specific tests
prove that each route reaches that same implementation. This keeps one
record/wire/apply composition instead of seven variants.

### 1.2 Blind-resend surfaces to retire

- Remove the `(ErrorRetryable, Sending)` transition from
  `db/repositories/fiscal_documents.rs:251-258`.
- Remove the `MacRecoveryOutcome::Resigned => continue` second-wire loop from
  `services/write_path/stage_send.rs:1031-1082`. A `-12` result records
  `BadHashPrev`, holds STOP/PENDING, and requires operator resolution. It never
  replaces bytes and never repeats the same document's wire call.
- Replace both `Sent + NotFound -> ErrorRetryable` producers:
  `boot_phase.rs:938-1036` and
  `offline_sync/kvt2_confirm.rs:1639-1745`. Each producer must complete its own
  trace and invoke the landed `sent_not_found_to_manual` helper in the same
  `BEGIN IMMEDIATE`, yielding document RMR + node STOP + trace + audit, all or
  none.
- Update stale comments and histogram labels that still promise a “redrive”.
  They are not authority, but leaving them would make later maintenance restore
  the deleted path.

### 1.3 Foreign FN-chain writers and offline producers

The exact active-fence predicate is:

```sql
state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
OR (state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY')
```

Every critical check below must execute in the same `BEGIN IMMEDIATE` as the
write it guards, through one tx-bound repository helper using that predicate:

| Surface | Live location | Required behaviour while fenced |
|---|---|---|
| Online accepted seed advance | `stage_send.rs:1809` | Only the matching reservation apply may write it |
| Offline-local-ack seed advance | `stage_offline_ack.rs:495` | Refuse before code consumption/CAS |
| Offline code replenish seed install | `offline_code_replenish.rs:267` | Read preflight refuses before the DPS code request; the write transaction rechecks the same fence before code/seed mutation |
| Boot seed repair | `boot_phase.rs:1814` | Refuse; operator completion owns any repair |
| New document issuance | `stage_acquire.rs:49` | Refuse before creating/pinning a new FN document |
| Offline session open | `services/offline_session.rs:74` | Refuse before session insertion |
| Offline-code consumption at sign | `stage_sign.rs:926-1015` | Refuse before `acquire_code_tx` |
| Offline local acknowledgment | `stage_offline_ack.rs:187-505` | Refuse before state/seed/code mutation |

The read-only `get_active_for_fn` is not enough for mutation safety because a
check followed by a later transaction has a race. Add only a tx-bound predicate
helper beside it; do not introduce a second fence model.

## 2. Target live composition

### 2.1 Central 4-pre: prepare and authorize in one commit

Keep envelope construction before any durable send marker. In the existing
first `with_immediate` in `stage_send`:

1. Read the immutable document inputs and reject unsupported/stale states.
2. Build the exact `CheckEnvelope` and compute its existing envelope SHA-256.
3. Transition only a genuinely fresh source (`Signed` or `OfflineLocalAck`) to
   `Sending`. `ErrorRetryable` is no longer a source state.
4. Allocate the existing `transport_trace` intent.
5. Construct the existing `NewReservation` from the same document, FN,
   envelope hash, and incumbent protocol binding.
6. Call `authorize_submission` inside that transaction. It inserts RN, advances
   generation/pointer, and stamps `CALL_STARTED`.
7. Return the opaque, non-Clone `Authorization` only after `with_immediate`
   commits.

Any failure rolls back the document CAS, trace intent, reservation, generation,
and pointer together. No `Authorization` escapes and no wire I/O occurs.

The reservation's `attempt_no` and `transport_trace.attempt_no` are separate
append-only counters. Do not assume they are equal for legacy documents; carry
each in its own existing field.

### 2.2 Sole wire function

Create one private function in `stage_send`:

```text
submit_authorized(Authorization, CheckEnvelope, &dyn DpsChannel)
    -> (legacy wire result, RawSendObservation)
```

It consumes the authorization and performs exactly one
`send_chk_observed(envelope).await` outside every write transaction. Before the
call it recomputes the envelope hash and verifies it against the private
authorization snapshot. The protocol binding in that snapshot is the binding
selected by the same 4-pre transaction from the FN outgress profile; it must not
be reconstructed or changed after commit. A mismatch returns with zero wire I/O.

The existing source gate is defence-in-depth only. The load-bearing properties
are:

- `Authorization` cannot be externally constructed;
- it cannot be cloned;
- `submit_authorized` consumes it;
- `send_chk_observed` has one production call site;
- the lifetime index/query makes a second authorization impossible.

The current direct call at `stage_send.rs:1564-1573` moves into this private
function. The seven callers remain callers of `stage_send::run`, never of the
wire seam.

### 2.3 Record commit

From the **same** `RawSendObservation` returned by that one RPC:

1. Run the existing `shadow_map::map_send_reply`
   (`services/write_path/shadow_map.rs:25-52`).
2. Build the existing `SubmissionEvidence::Started` using the reservation's
   stored binding and envelope hash.
3. Run the existing total `classify`.
4. Derive the existing `EvidenceDiscriminant::from_evidence`.
5. Mint `ObservedOutcomeV1::record` with
   `AuthorizedGeneration::Started(authorization.authorized_generation)`.
6. In a new repository `record_outcome` function, update only the matching
   `CALL_STARTED` row to `OUTCOME_OBSERVED + PENDING_APPLY`, writing the axes,
   node effect, correlation, and all four evidence columns in one
   `BEGIN IMMEDIATE`.
7. The update predicate includes the full authority tuple:
   reservation id, document id, authorized generation, protocol binding, and
   envelope hash. `rows_affected != 1` is a hard error; it must not fall back to
   the legacy 4-b path.
8. In that same record transaction, unresolved leaves set `STOP_MODE`; `-11`
   sets `BLOCKED`. No fence release occurs here.

`record_outcome` is a repository operation, not a new domain entity. It accepts
the existing sealed record/evidence types and serializes them through the
existing accessors.

### 2.4 Apply commit

After the record commit, call the existing `apply_outcome` in a separate
`BEGIN IMMEDIATE`:

- clean Accepted transitions the document through the existing `Sending ->
  Sent` edge, stamps SFN, reproduces the existing online/offline seed split and
  accepted shift effects, then atomically marks `APPLIED` + clears the matching
  active pointer;
- an online-origin definitive reject transitions through the existing
  classifier/routing target and releases only when it is not one of the
  explicitly held `-12/-6` leaves;
- offline-origin rejects, SubmittedUnknown, `-12`, and `-6` remain PENDING and
  STOP/BLOCKED for operator completion;
- an apply error never rolls back or rewrites the already durable evidence;
- a process crash after record is resumed at boot with zero send RPCs.

The legacy 4-b state/seed logic in `stage_send.rs:1630+` must not execute in
parallel with `apply_outcome`. During cutover, the record transaction owns the
wire trace/audit, while the apply transaction owns document/SFN/seed/node/shift
effects and the APPLIED+pointer-clear release. Delete the duplicate legacy
mutation branch. A shadow call plus the old 4-b is not activation; it is double
ownership.

### 2.5 Crash windows

| Crash point | Durable state on restart | Required recovery |
|---|---|---|
| Before 4-pre commit | No CALL_STARTED | Fresh invocation may authorize |
| After 4-pre commit, before/during wire | CALL_STARTED, no outcome | Boot converts once to `NoResponse(CrashedBeforeObservation)` + PENDING + STOP; zero wire |
| After returned wire, before record commit | CALL_STARTED, no durable outcome | Same conservative boot conversion; zero wire |
| After record commit, before apply | OUTCOME_OBSERVED + PENDING | Hydrate and call `apply_outcome`; zero wire |
| Mid-apply transaction | Transaction rolls back; still PENDING | Repeat `apply_outcome`; zero wire |
| After APPLIED commit | APPLIED, pointer cleared atomically | No-op on replay; next document may authorize |

The “returned wire but crashed before record” case intentionally loses response
detail but not safety: it becomes SubmittedUnknown and requires reconciliation.

## 3. Activation of the inactive recovery helpers

Slice 7 must wire the already-landed helpers; merely compiling them is not an
activation:

1. At boot, call `list_call_started_without_outcome`, then
   `resume_crashed_reservation` once per row in its own DB transaction. Never
   call `stage_send` for these documents.
2. Hydrate every `OUTCOME_OBSERVED + PENDING_APPLY` row and call
   `apply_outcome`. `HeldNotAutoRelease` is a normal STOP/operator result, not a
   resend trigger.
3. Strengthen the existing `reset_stop_mode` production path so a plain reset
   refuses while a PENDING reservation exists. The existing read-only
   `status_rro` probe runs outside the transaction; verified resolution then
   calls the completed `complete_operator_pending` transaction.
4. Retarget both live Sent+NotFound producers to
   `sent_not_found_to_manual` as described in §1.2.

No recovery path may construct a fresh `NewReservation` for a document whose
history contains `call_started_at IS NOT NULL`.

## 4. Legacy cutover and deployment

The preferred activation is a short, single-binary maintenance cutover:

1. Stop the writer process; do not run old and new binaries concurrently.
2. Run migrations 035/036 only after their empty-table guards pass.
3. Before starting the cutover binary, require zero reservation-less documents
   in `Sending` or `ErrorRetryable`. If the pre-deploy query is non-empty, abort
   activation and classify those rows manually.
4. Keep a runtime backstop: a reservation-less `Sending` or
   `ErrorRetryable` row is moved to RMR/STOP without any send call. Do not infer
   safety from `transport_trace`.
5. Start only the cutover binary. Once it creates a real `CALL_STARTED` row,
   rolling back to an old binary that ignores the fence is forbidden. Operational
   rollback is stop/fail-closed and deploy forward, not restart the blind-resend
   build.

This project is single-operator/single-process, so a rolling-version protocol is
unnecessary. The no-mixed-binary rule is sufficient and materially simpler.

## 5. RED-first teeth

The following tests are merge gates. For each, revert the named guard and prove
the test turns RED; a permanently green test is discarded.

| Tooth | Scenario | Must prove |
|---|---|---|
| S7-P2-1 lifetime call once | Race two `stage_send::run` invocations for one document | Exactly one CALL_STARTED and at most one real RPC |
| S7-P2-2 seven-route composition | Exercise all seven callers with a counting channel | Each reaches the same sole wire function; lifetime RPC count ≤1 |
| S7-P2-3 crash after marker | Commit authorization, drop future, cold reopen | Boot records crash evidence; zero RPC; same doc cannot reauthorize |
| S7-P2-4 kill `-12` loop | Return `BadHashPrev`, then a reply that would succeed on call two | RPC count remains one; bytes/hash unchanged; PENDING+STOP |
| S7-P2-5 no ER redrive | Seed `ErrorRetryable` with started history | No transition to Sending and zero RPC |
| S7-P3-1 foreign writers | Hold one active reservation, invoke each of four seed writers | Every foreign seed remains byte-identical; zero auxiliary code request where applicable |
| S7-P3-2 offline surfaces | Hold a fence, attempt acquire/session-open/code-consume/offline-ack | All refuse before fiscal/code/session mutation |
| S7-P3-3 Sent+NotFound | Boot and drain producers, with failure injected after each write | All-old or RMR+STOP+trace+audit; never ErrorRetryable |
| S7-P4-1 record totality | All 11 evidence leaves through their real construction path (wire mapper for Started, preflight constructors for NotStarted) → classify→record→cold hydrate | Exact evidence/axes/effect round-trip; no all-NULL OO |
| S7-P4-2 record/apply crash matrix | Crash before record and after every apply write | Conservative PENDING or exactly one applied effect; zero recovery RPC |
| S7-P4-3 same-response derivation | Unique marker in a digest-bearing DPS reply | Legacy trace and durable evidence derive from the same single response |
| S7-BRICK-1 release | Complete each automatic and operator-held outcome | Matching pointer clears only with APPLIED; next document authorizes |
| S7-BRICK-2 operator matrix | online/offline × accepted/not-accepted × regular/shift family | Every supported resolution either completes safely or stays explicitly held; no permanent undocumented refusal |
| S7-LEGACY | Reservation-less Sending/ErrorRetryable at startup/runtime | RMR/STOP and zero RPC |
| S7-SOLE | Scan production call graph plus runtime counter | One `send_chk_observed` production call site; moving/adding a second site turns RED |

The property tests should schedule boot, inline, convergence, and drain in
different orders against the same document history. The invariant is lifetime
wire count, not “one call per function invocation”.

## 6. Implementation order

All steps may be separate review commits, but none is independently deployable.
They ship in one production release.

1. **S7-0 — finish inactive foundation:** mandatory evidence in migration 036;
   production `record_outcome`; sealed/non-Clone existing `Authorization`;
   total document/SFN/seed/node/shift apply projection with explicit `-12/-6`
   holds; full operator matrix and real `reset_stop_mode` wiring. Add RED teeth
   without changing the live send path.
2. **S7-1 — sole authorized composition:** central 4-pre authorization,
   private consuming wire function, record transaction, repeatable apply. The
   seven callers still enter through `stage_send::run`.
3. **S7-2 — whole FN fence:** tx-bound active-fence check at the four seed
   writers and issuance/session/code/offline surfaces.
4. **S7-3 — retire redrive:** remove `ErrorRetryable -> Sending`, remove the
   `-12` loop, retarget both Sent+NotFound producers, delete stale redrive
   branches/comments.
5. **S7-4 — boot/operator activation:** wire CALL_STARTED reaper, PENDING
   replay, and verified operator completion; add the BRICK teeth.
6. **S7-5 — cutover gate:** legacy empty-in-flight preflight, runtime
   fail-closed backstop, sole-caller scan, all route/crash tests, full workspace
   verification.

Review can stop after any commit. Deployment cannot.

## 7. Merge and release gate

Slice 7 is GO only when all of the following are true:

- the five §0 gaps are closed;
- every tooth in §5 has been observed RED on guard removal and GREEN restored;
- full `--all-features` workspace tests, fmt, and clippy are green;
- migration 035/036 replay is green on a clean DB and refuses a non-empty
  activation DB without partial objects;
- production grep/source-gate finds only the private authorized wire site;
- no live producer transitions issued history to `ErrorRetryable` for resend;
- the pre-deploy query reports zero reservation-less in-flight documents;
- the release procedure forbids an old binary after the first live
  `CALL_STARTED`.

At that point the defending clauses are:

| Property | Load-bearing clause |
|---|---|
| P2 | committed CALL_STARTED marker + lifetime unique/NOT-EXISTS + non-Clone consumed authorization + one wire site |
| P3 | one active FN reservation + tx-bound guards on every seed/offline writer + atomic origin-aware apply/STOP |
| P4 | mandatory durable evidence + record/apply commit split + generation/pointer CAS + boot replay with zero wire |
| BRICK | automatic APPLIED release where safe + complete verified operator resolution + pointer clear atomically with completion |

Until then, Slices 1–6 remain inactive machinery and the current live fiscal
path remains unchanged.
