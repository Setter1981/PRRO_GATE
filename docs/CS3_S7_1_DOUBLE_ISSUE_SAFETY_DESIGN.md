# CS-3 Slice-7 (S7-1) — Live-Cutover Double-Issue-Safety Design-of-Record

**Status:** rev2 + round-2 resolution — design **FROZEN** (build checklist §11). **NOT an implementation authorization; the live cutover still needs its own explicit GO.**
**Baseline:** worktree `cs3-de-slice2`, branch `cs3-de-slice7-s0` @ `53c5b13` (S7-0 5/5 landed, CI #335 green).
**Predecessor process:** mirrors `CS3_GAP4B_FORK_SAFETY_DESIGN.md` (design-gate → external round → GO → implement).
**Gate provenance:** adversarial design-gate `wf_8b8c2be9-095` followed by an independent external review.
Rev2 incorporates the external findings on the authorization lifecycle, pre-wire chain check, boot wiring,
`-12` short-circuit, ApplyPlan completeness, the direct END-mint writer and RED-teeth soundness. Every claim
below is grounded in `file:line` at `53c5b13`.

---

## 0. What S7-1 is

S7-1 is the **live activation of the double-issue fix**: it composes the **already-landed** S7-0 primitives
into the real send path and retires the legacy blind-resend. It relocates ownership of the wire and the
post-wire projection from the legacy `stage_send` 4-b block onto the reservation ledger. This is **not**
claimed byte- or behaviour-neutral: before cutover the full ApplyPlan graph is pinned (§4.1), including the
three already-adjudicated 3.2 deltas. No additional fiscal semantics may appear outside that closed graph.

Landed primitives plus the explicitly TO-BUILD cutover adapters (`delivery_reservation.rs`):
- `authorize_submission` (:357) → sealed non-`Clone` `Authorization` (:263) — the central 4-pre.
- `submit_authorized` — consumes `Authorization` and returns the already-specified #4B
  `AttemptObservation`; it never returns a reusable authorization.
- `record_outcome` (:491) — is changed to consume/borrow only the post-wire `AttemptObservation`
  (or its record-only projection), never `Authorization`; full-authority CAS,
  `rows_affected!=1` hard error, no legacy fallback.
- `apply_outcome` (:648) — origin-split fiscal projection (SFN, seed, `-11`/`-12`/`-6` holds, `APPLIED`+pointer-clear).
- retirement replacement `sent_not_found_to_manual` (`sent_not_found.rs:67`) — landed Slice 6.

**The invariant S7-1 must hold: P2 — at most ONE real DPS wire (`send_chk_observed`) per document lifetime,
across all 7 callers, all recovery paths, and all crashes.**

---

## 1. Grounding verdict (all surfaces confirmed at 53c5b13)

| Surface | Verdict | Note |
|---|---|---|
| 7 callers of `stage_send::run` | **READY** | Uniform 4-arg sig; construct no authorization → **zero caller edits**. `run` (`:1031`) is a MAC-loop wrapper over `run_one_attempt` (`:1238`); the 4-pre/4a/4b body is in `run_one_attempt`. |
| Sole wire seam | **READY** | `send_chk_observed` has **exactly one** production call site: `stage_send.rs:1568`. The observed-variant already exists; its `_shadow_response` (`:1573`) is currently READ-ONLY (CS-3 3.2). Relocation, not addition. |
| Landed S7-0 primitives | **READY as inactive foundation** | `Authorization` is sealed/non-`Clone`; current `record_outcome`/`apply_outcome` are inactive primitives. Their signature, full binding and projection are completed by §§2/4 before cutover; no partial live wiring exists. |
| Blind-resend surfaces (§1.2) | **READY to retire** | `(ErrorRetryable,Sending)` at `fiscal_documents.rs:257`; `Resigned=>continue` at `stage_send.rs:1082`; **exactly two** `Sent→ErrorRetryable` producers (`boot_phase.rs:969`, `kvt2_confirm.rs:1713`) — no hidden third. |
| Foreign FN-chain writers (§6) | **NOT READY w/o helper** | The closed mutation inventory includes the direct drain END mint that bypasses `stage_acquire`; none carries the complete active-reservation fence. `get_active_for_fn` (:183) is pool-bound read-only — insufficient (TOCTOU). Needs an additive tx-bound helper (S7-2). |

Line-drift vs the plan is cosmetic and all forward (`53c5b13` ahead of the plan's `d5227e4`): wire `1568` (not
`1564-1573`); legacy 4-b block opens at `1710` (not `1630+`; `1629-1700` is the pre-lock cash derive that MUST
stay outside the lock, invariant #1); ER edge `257` (not `251-258`). No structural drift.

---

## 2. The P2 double-issue safety argument (why ≤1 wire holds)

The guarantee rests on **three composing DB-level layers** and holds **IFF** S7-1 relocates the wire behind
`authorize_submission`. Today the wire at `stage_send.rs:1568` is **not** gated by authorization at all → the
double-issue bug is **live by construction** (expected pre-cutover; this is what S7-1 fixes).

- **Layer 1 (load-bearing, DB-enforced, survives crashes/retries/two connections):** partial unique index
  `ux_delivery_document_ever_started ON delivery_reservation(document_id) WHERE call_started_at IS NOT NULL`
  (`migrations/035:43-45`). At most **one** row per `document_id` can ever carry a non-NULL `call_started_at`.
  A second `CALL_STARTED` for the same doc fails `SQLITE_CONSTRAINT_UNIQUE`. This is what makes P2 **cross-route**,
  not just cross-caller.
- **Layer 2:** `authorize_submission` runs its explicit call-once `NOT EXISTS` (`:372-382 → CallOnceAlreadyStarted`),
  the fresh `RESERVED_NOT_STARTED` insert (no-replace trigger backstop `035:70-71`), the generation advance
  (`:400-413`), and the `RN→CALL_STARTED` CAS (`:416-430`) **all in the caller's single `BEGIN IMMEDIATE`**
  (`with_immediate == BEGIN IMMEDIATE`, `db/tx.rs:3-16`). Any `rows_affected!=1 → Err →` whole tx rolls back →
  **no `Authorization` minted** (`:432` reached only on full success).
- **Layer 3:** `Authorization` is sealed, non-`Clone`, and consumed by value. `submit_authorized` verifies the
  complete binding tuple and `envelope_hash`, performs the sole wire call, and returns a post-wire-only
  `AttemptObservation` containing the immutable reservation/generation/binding/hash plus the observed evidence.
  `AttemptObservation` cannot authorize a wire. `record_outcome` accepts this post-wire value, not
  `Authorization`. **No `Authorization` ⇒ zero wire I/O; after one call no wire-capable value survives.**
  A second authorization is the only way to a second wire, and Layers 1+2 make it impossible.

  The token and reservation snapshot include the **full binding**:
  `{dps_protocol_id, protocol_contract_version, capability_profile_version,
  endpoint_config_revision, envelope_hash}`. The submission adapter exposes the corresponding bound-port
  snapshot (the #4B `binding()` contract, or an exactly equivalent immutable adapter value). A mismatch returns
  before any wire I/O. Checking only protocol id/version is insufficient.

**Race closure:** all 7 callers funnel through the same `stage_send::run` and construct no authorization; under
`BEGIN IMMEDIATE` two `authorize_submission` tx serialize → the loser sees the winner's `CALL_STARTED`
(`CallOnceAlreadyStarted`) or trips the unique index → rolls back with no token. **SAFE by construction once
composed.**

The guarantee is **conditional** on the complete R1-R7 retirement (§5) landing atomically with the composition —
otherwise a surviving route can either reach the wire outside Layer 2 or permanently spin a fenced document.

### 2.1 Pre-wire chain-predecessor guard (P3)

The S7-2 active-reservation fence closes mutations **after** an active reservation exists, but it cannot stop a
foreign writer that commits immediately **before** authorization. Therefore the online-origin authorization
transaction also checks, before `RN→CALL_STARTED`:

```text
node_state.last_known_unsigned_xml_sha256 == fiscal_document.previous_hash
```

The check, reservation insert and `CALL_STARTED` marker are in the same `BEGIN IMMEDIATE`. A mismatch refuses
authorization with zero wire I/O. This moves the incumbent post-wire check (`stage_send.rs:1771-1808`) to the
only safe side of the wire. It is **online-origin only**; an offline-origin document follows the already-defined
offline chain/cohort rules and must not be forced through this equality.

---

## 3. KEY FINDING — the plan §1.2 retirement list is INCOMPLETE (BRICK-class)

> The gate's highest-value result. Plan §1.2 names the transition edge, the `-12` loop, and the two Sent+NotFound
> producers, but **omits three live callers that redrive `ErrorRetryable` through `stage_send::run`**. Retiring the
> edge/allowlist alone leaves these three spinning.

**The three un-named Redrive callers** (all funnel through the 4-pre source allowlist `stage_send.rs:1269`):
1. `online_convergence.rs:561` — `converge_error_retryable_doc`, `Redrive => stage_send::run`.
2. `boot_phase.rs:3072` — `dispatch_error_retryable_by_class`, `Redrive => stage_send::run`.
3. `backlog_drain.rs:1547` — `process_via_er_class_guard`, `Redrive => process_via_stage_send`.

**Why this is a real defect, not cosmetics:** every `ErrorRetryable` doc has provably already consumed **exactly
one** wire (all ER producers are *post-wire* `route_dps_error` routes, `error_routing.rs:291..398`; pre-wire
refusals resolve to `StateConflict`/`SignerRefused` and never reach `ErrorRetryable`). So any ER-redrive is
inherently a **second wire for an already-`CALL_STARTED` doc**.

**Post-cutover failure mode:** with `ErrorRetryable` dropped from the source allowlist (`:1269`) the three
`Redrive` branches hit the `!matches!` gate → `StateConflict` **no-op** → the doc **never leaves `ErrorRetryable`,
the FN never converges, nothing escalates**, and the conflict is only counted into a histogram. This is a **live
BRICK-class regression + silent spin** (violates the "legit FN not permanently refused" property). Same hazard
for the `Kvt1→ErrorRetryable` edge (`fiscal_documents.rs:214`) on an *already-issued* (SFN-stamped) doc.

**P2-the-wire is still SAFE** here (the double-lock — dropping ER from both the edge `257` and the allowlist
`1269` — means the redrive can't reach the wire). The defect is **BRICK/liveness**, and it must be fixed in the
**same commit** as the edge removal.

**Decision:** collapse `Redrive → EscalateManual{TransientRetry}` in `er_redrive_policy.rs:92-100` and delete
the `Redrive` variant. All three callers already have `cas_*_to_manual_reconciliation` helpers wired
(`online_convergence.rs:601`, `boot_phase.rs:3124`, `backlog_drain.rs:1549`) → the document reaches RMR and
the node STOPs.

The rejected alternative was to hydrate through `resume_crashed_reservation`/`apply_outcome`. It is not a
valid general replacement: legacy `ErrorRetryable` rows predate reservation activation and may have no
reservation at all, while `resume_crashed_reservation` accepts only `CALL_STARTED`. Automatic recovery can be
designed later as reconciliation of a known outcome; S7-1 never converts a retry classification into a new wire.

At cutover, reservation-less legacy `SENDING`/`ERROR_RETRYABLE` rows must either be proven absent by a
pre-deploy empty-in-flight gate or moved fail-closed to RMR/STOP. They must not be inferred safe from
`transport_trace`.

---

## 4. Legacy 4-b deletion boundary (exact)

`apply_outcome` and the legacy 4-b write the **same three mutations via the same repo fns** — doc CAS
`Sending→target` (`apply_outcome:751/772` == 4-b `:1729`), SFN stamp (`apply_outcome:738` == 4-b `:1751`), online
seed advance (`apply_outcome:746` == 4-b `:1809`, verified identical fn body → `node_state.rs:164`), plus `-11`
node-BLOCKED. **Double-owned.** Today only 4-b runs (`apply_outcome` has **zero** production callers) → no live
double-write; it becomes one the instant a *shadow* `apply_outcome` is added without deleting 4-b.

**The sharp non-idempotent edge:** the legacy 4-b seed advance **skips** its `previous_hash==node-seed` equality
gate whenever `mac_recovery_attempts>=1` (`stage_send.rs:1800`). On a `-12` recovery the doc's re-signed sha
differs from `previous_hash` by construction. If both 4-b and `apply_outcome` run, the **last writer wins** and the
seed can land on the **wrong tip → FN chain fork**. So "shadow call + old 4-b" is unsafe even though the *value*
looks idempotent on the happy path.

**Deletion boundary (delete the WHOLE second `with_immediate`):** `stage_send.rs:1710-1972` — `transition_state`
`:1729`, `set_server_fiscal_no_tx` `:1751`, seed advance `:1809` **incl. its `:1800-1808` gate**, shift edges
`:1829-1859`, `-11` block `:1877`, `complete_tx` `:1895`, audit `:1957`.

This block is replaced by two explicit commit boundaries:

1. **record transaction:** `record_outcome(AttemptObservation)` persists evidence/axes/effect and
   `PENDING_APPLY`, completes the existing `transport_trace`, appends the outcome audit, and applies the early
   STOP/BLOCKED safety mode. Failure rolls the whole record transaction back; no legacy fallback runs.
2. **apply transaction/service:** the repeatable apply orchestration performs the document target CAS,
   SFN/seed projection and online shift-confirm/closing-cash edge, then marks `APPLIED` and clears the pointer.
   The service owns `apply_shift_transition`; the repository must not call upward into `services::*`.

**Preserve:** the pre-lock `closing_cash_kop_for_4b` derivation `:1629-1700` (outside the write transaction,
invariant #1), but move it behind one shared live/boot apply service. The service derives it from durable
document/shift/ledger state immediately before each apply transaction; it does **not** rely on an ephemeral
value surviving the record→apply crash window. The active FN fence keeps the derivation inputs stable. The
`EmptyServerFiscalNo` condition becomes the typed `OkButNoFiscalNumber` ApplyPlan row; it is not allowed to abort
outside record and leave an unrecorded `CALL_STARTED`. The wire `:1568` relocates into `submit_authorized`.

### 4.1 Full ApplyPlan graph is a cutover prerequisite

The coarse 3.2 drift pin is insufficient for live state mutation. Before deleting 4-b, one normative pin
enumerates every legal evidence leaf and compares the full tuple:

```text
(target_state, retry_class, SFN/seed/shift effects, node_effect,
 audit, probe, fence disposition)
```

Unchanged incumbent rows must be exactly equal. The three already-adjudicated deltas
(`OkButNoFiscalNumber`, unknown non-zero status, TLS-proven RemoteStatus) must match their exact locked target
tuples; any fourth delta is a design failure until adjudicated.

In particular, `FnConfigError` (`-13/-14`) retains the incumbent `ErrorRetryable` target; the R6 convergence
policy then moves it to RMR/STOP without a wire. It must not fall through to the current repository's blanket
online `Rejected` projection. Boot replay calls this same apply orchestration; it does not maintain a second
projection table.

---

## 5. Complete blind-resend retirement list (amended)

| # | Surface | Location | Action |
|---|---|---|---|
| R1 | `(ErrorRetryable,Sending)` edge | `fiscal_documents.rs:257` | **delete the arm** — kills the ER-redrive edge AND makes ER a non-source (§2.1 pt3). |
| R2 | `ErrorRetryable` in 4-pre source allowlist | `stage_send.rs:1269` | **remove `ErrorRetryable`** — only a fresh `Signed`/`OfflineLocalAck` can seed a `Sending`. |
| R3 | `-12` MAC-recovery path | `stage_send.rs:1048-1082`, `mac_recovery.rs:516-535` | short-circuit **before `run_mac_recovery`**; record `BadHashPrev` + PENDING/STOP. Do not re-sign, replace XML/CMS, change `previous_hash`, or `continue` to a second wire. |
| R4 | Sent+NotFound producer #1 | `boot_phase.rs` `cas_sent_to_error_retryable_from_probe` (`:969`) | retarget to landed `sent_not_found_to_manual` in its existing `with_immediate`, keeping its own `transport_trace` completion. |
| R5 | Sent+NotFound producer #2 | `kvt2_confirm.rs` `commit_sent_replay_envelope_1c_post` (`:1713`) | same as R4 (preserve its distinct `outcome_kind`). |
| **R6 (NEW, §3)** | **3 Redrive callers** | `online_convergence.rs:561`, `boot_phase.rs:3072`, `backlog_drain.rs:1547` | route to RMR/STOP per the §3 decision; never call `stage_send::run`. |
| R7 | ER-redrive **consumers** presuming the edge survives | `dispatch_error_retryable_by_class`, `kvt2_confirm.rs:1645-1649` (W9b ER-guard), `boot_phase.rs:947-948` (two-tick retry) | retarget/remove in the same commit or they fail-closed on a now-forbidden transition. |

All of R1-R7 must land **together** (one atomic cutover), or a partial state either re-wires (double-issue) or
stucks a doc (BRICK).

---

## 6. tx-bound fence helper contract (S7-2 — closed mutation inventory)

The §1.3 writers advance the FN chain tip, allocate chain positions or mint documents with **no knowledge of
`active_delivery_reservation_id`**. `get_active_for_fn` (`:183`) is pool-bound read-only → a read-then-write across
separate connections **races** the fence (TOCTOU confirmed; the FN write-lease is split — drain holds
`reconcile_mutex`, inline/convergence hold `fn_write_gate`, distinct locks, `app.rs:759`).

**Highest fork risk:** `offline_code_replenish.rs:267` — advances the seed to `sha256(request_xml)` with **no
equality gate at all** and no reservation check → silent wrong-tip overwrite → hard fork with no fail-closed
backstop. **Second:** `stage_acquire.rs:49` — mints doc N+1 before doc N's wire outcome is applied. The seed-advance
surfaces at `stage_send.rs:1809`/`stage_offline_ack.rs:495` carry a partial in-tx equality gate (catches a shifted
seed but as a crash, not a clean fence; and stage_send skips it on `-12`). `boot_phase.rs:1814` is quiescent
(pre-ingress). `offline_session.rs:79` has its own `ux_offline_active` index.

The inventory includes both ordinary `stage_acquire` issuance and the direct drain END mint
`offline_sync/backlog_drain.rs:2701-2779`, which deliberately bypasses `stage_acquire`, allocates an LND and
inserts a `PREPARED` row in its own transaction. It must run the same tx-bound fence before allocation/insert.
The earlier label “8 writers” is retired: the design owns a closed table of mutation operations, not a fragile
count or duplicated file ranges.

**Helper contract:**
```rust
// delivery_reservation.rs — predicate BYTE-IDENTICAL to the inline query at :195-196
pub async fn fn_fence_active_tx(tx: &mut WriteTxConn<'_>, fiscal_number: &str) -> Result<bool, sqlx::Error>
// EXISTS(state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
//     OR (state='OUTCOME_OBSERVED' AND apply_state='PENDING_APPLY'))
```
- (a) **MUST** take `&mut WriteTxConn` and run inside the writer's own `BEGIN IMMEDIATE` — a pool-bound variant
  re-introduces the TOCTOU and is **forbidden**.
- (b) the predicate string is the **single source of truth** shared with `get_active_for_fn`, the
  `authorize_submission` query, the `ux_reservation_active` index and the `no_replace` trigger — enforce with a
  shared const **or** a byte-identity conformance test so drift fails CI.
- (c) returns `bool`; each foreign writer fails-closed (refuse) when `true`, **before** the mutation.
- **EXCLUDE `apply_outcome`** — it *is* the active reservation; self-fencing would deadlock the design. It keeps
  its generation-CAS (`:705-714`) as its atomic guard.
- A foreign writer that starts before authorization may commit before the reservation exists; this is why the
  online pre-wire predecessor equality in §2.1 is independently load-bearing. The fence and equality guard
  defend different interleavings.

---

## 7. Crash-window matrix (synthesized from landed `resume_crashed_reservation`, Slice 4)

Exactly-once-across-crash rests on an explicit **boot-first reservation pass**, which is part of S7-1 rather
than an assumed property of the landed helpers:

1. before ordinary document dispatch, list every `CALL_STARTED` reservation and run
   the boot-resume service once in its own `BEGIN IMMEDIATE`; that service calls
   `resume_crashed_reservation`, completes the in-flight trace as crash/unknown and appends the recovery audit
   atomically;
2. list every `OUTCOME_OBSERVED + PENDING_APPLY` reservation and run the shared repeatable apply
   orchestration;
3. only after those passes may the existing document-state boot dispatch run.

`resume_crashed_reservation` produces `NoResponse{Crashed}` + PENDING + STOP, never enters the wire and never
constructs a fresh `NewReservation`. Merely landing the helper is insufficient: S7-1 owns the production boot
wiring and its ordering.

### 7.1 Boot-pass insertion point (round-2 #4)

The reservation pass is **global** (`list_call_started_without_outcome`, `delivery_reservation.rs:878`, has no FN
filter) and must be wired **once, pre-loop, in `reconcile_pending_inner` (`app.rs`), BEFORE the `for fn_cfg in
&fns` loop** — NOT inside per-FN `run_boot_reconciliation`. Inserting it per-FN is unsafe: the early returns in
`run_boot_reconciliation` (branch-f STOP_MODE/Blocked/CryptoDegraded `boot_phase.rs:1901-1942`, manual-recon
`:1953`, `OfflineModeRefusal` `:1882`, which under the ctx-free boot path returns an error before later FNs run)
would skip a later FN's `CALL_STARTED` rows for the whole boot. Step 2 (apply every `OUTCOME_OBSERVED+PENDING_APPLY`,
via a companion global query `list_outcome_observed_pending_apply`) **must treat `ApplyError::HeldNotAutoRelease`
as an EXPECTED hold** (log Warning + continue) — a `-12`/`-6` PENDING hold is valid and must not abort boot
(propagating it as a `BootError` violates frozen invariant #9). Only after both passes may the per-FN loop run.

### 7.2 NC-03 (lost `node_state`) interleave — why the boot fence is EXCLUDED, not merely harmless

The §7.1 apply pass can hit `NodeStateMissing` for an FN whose `node_state` row was lost while the
`fiscal_documents` ledger + `delivery_reservation` rows survived (the NC-03 condition). A naive global pass would
propagate `NodeStateMissing` and abort boot **before** the NC-03 reconstruction ever runs, so an NC-03 FN that
also carries an active reservation would be **unreachable** despite the S7-2 exclusion. The pass therefore
**defers-then-retries**, and the NC-03 seed repair (`boot_phase.rs:1814`) is DELIBERATELY left **unfenced**
because a live PENDING reservation is EXPECTED during it:

1. **normalize** every `CALL_STARTED` reservation (`resume_crashed_reservation → NoResponse{Crashed}` + PENDING +
   STOP), deferring any whose `node_state` is missing;
2. **apply** every `OUTCOME_OBSERVED + PENDING_APPLY` via the shared orchestration; on `NodeStateMissing` **DEFER**
   that FN (record it, do NOT fail boot); on `HeldNotAutoRelease` log + continue (§7.1);
3. run **NC-03 reconstruction** for the deferred FNs — rebuild `node_state` (LND + seed from the surviving
   ledger) and set **BLOCKED** (`boot_phase.rs:1814`). This step MUST run with a live PENDING reservation present,
   which is exactly why it carries **no** `fn_fence_active_tx`;
4. **retry** the deferred normalize/apply (now `node_state` exists);
5. only then run the ordinary per-FN dispatch loop.

The boot fence exclusion is thus **load-bearing, not harmless**: fencing `boot_phase.rs:1814` would refuse step 3
whenever a reservation is PENDING, stranding the deferred apply with no `node_state` — an unrecoverable boot. The
S7-2 `fence_wired_into_writers_static_pin` records this exclusion + rationale.

| Crash window | Reservation state at boot | Boot action | Re-wire? |
|---|---|---|---|
| before 4-pre commit | none (rolled back) | fresh doc; a later authorize is clean | no |
| after 4-pre commit, before/during wire | `CALL_STARTED` | `resume_crashed_reservation → NoResponse{Crashed}` + PENDING + STOP | **no** — Layer-1 blocks a fresh authorize; resume hydrates, does not call `stage_send::run` |
| after wire, before `record_outcome` | `CALL_STARTED` | same → operator completion resolves the unknown outcome | **no** (never blind re-send an unknown-outcome wire) |
| after `record_outcome`, before apply | `OUTCOME_OBSERVED` + PENDING_APPLY | boot invokes the shared apply orchestration (generation-CAS idempotent) | no |
| mid-`apply_outcome` (one `BEGIN IMMEDIATE`) | PENDING_APPLY (uncommitted) or APPLIED | re-apply / no-op | no |
| after APPLIED | APPLIED | terminal no-op | no |

**Dependency on §3:** this holds only if the three `Redrive` callers are retired (R6). A surviving `Redrive`
constructs a fresh `NewReservation` for a doc with `call_started_at` history — the exact rule §3/plan-§3 forbids.
So R6 is **also** the crash-window guard.

---

## 8. RED-first teeth (must bite empirically — revert-canary each)

- **S7-P2-1 (sole-wire, load-bearing):** fire two concurrent `stage_send::run` for the **same** `document_id`
  (and a `run` + `resume_crashed_reservation` variant) against a **counting** fake `DpsChannel`; assert **exactly
  one** `send_chk_observed` reaches it and the loser returns `CallOnceAlreadyStarted`. **Revert-canaries:**
  independently remove the DB call-once predicate/index, and independently introduce a second/direct wire call
  inside/outside `submit_authorized`; each must RED. R1/R2 are not this test's load-bearing guard.
- **S7-P2-2 (static sole-seam):** static scan — `send_chk_observed` has **exactly one** call site (inside
  `submit_authorized`) and `submit_authorized` has **exactly one** caller (`stage_send::run`). Compile-fail pins
  prove `Authorization` cannot be cloned/constructed, `AttemptObservation` cannot call the wire, and a
  wrong full binding/hash produces zero wire calls.
- **S7-P3-1 (single seed writer):** drive one online SELL through the cutover `run()` to `Sent`; assert
  `last_known_unsigned_xml_sha256 == doc.unsigned_xml_sha256` and the seed advanced **exactly once** (extend
  `tests/apply_outcome.rs:179/235`). **Revert-canary:** re-add the 4-b seed advance `:1809` → the "exactly once"
  assertion FAILS.
- **S7-P3-2 (mac-recovery divergence):** an online `-12` (`mac_recovery_attempts>=1`) lands the seed on
  PENDING/STOP after one wire with envelope hash/XML/CMS/`previous_hash` unchanged. Re-enable
  `run_mac_recovery` or `continue` and the test REDs.
- **S7-P2-3 (BRICK / 3-caller matrix, §3):** seed a doc in `ErrorRetryable` **with `call_started_at` set** and
  drive it through **each** of the three Redrive callers with a counting channel; assert (a) lifetime wire count
  stays 1, (b) the doc does **not** rest spinning in `ErrorRetryable` — it reaches RMR (or held PENDING) and STOPs,
  never a `StateConflict` no-op loop.
- **S7-P3-3 (Sent+NotFound):** both retargeted producers escalate `Sent → RMR + STOP + audit` atomically, no
  re-wire.
- **S7-P3-4 (pre-wire predecessor):** an online signed doc snapshots H0; a foreign writer commits H1 before
  authorize; authorize refuses before the counting channel. Removing the §2.1 equality check makes the wire
  count become one and REDs the test.
- **S7-P4-BOOT (tightened, round-2 #5):** seed one `CALL_STARTED` reservation **+ its `Sending` doc** and one
  `OUTCOME_OBSERVED+PENDING_APPLY`, run the production boot entrypoint, assert wire count stays zero, the first
  becomes crash evidence/STOP and the second is applied once. The **order-swap** revert-canary must assert the
  **intermediate reservation STATE** — after boot the `CALL_STARTED` row is `OUTCOME_OBSERVED+PENDING_APPLY` (the
  reservation pass converted it) **and** its doc reached `ErrorRetryable`, NOT still `CALL_STARTED` — because in the
  INACTIVE state wire-count is 0 either way, so wire-count alone cannot RED the order-swap. Also assert a
  `HeldNotAutoRelease` (a `MacReseedPending` PENDING_APPLY row) in step 2 does **not** surface as a boot error.
- **S7-APPLY-GRAPH:** all legal evidence rows traverse `record → persisted row → boot/shared apply`; assert the
  exact full 7D ApplyPlan graph, including `FnConfigError`, shift/closing-cash, trace, audit and fence.
- **S7-FENCE (S7-2):** `fence_race_replenish` (active `CALL_STARTED` + `offline_code_replenish` seed install → MUST
  refuse; revert guard → GREEN-advance proves it's load-bearing); ordinary issuance and direct END-mint refusal;
  `toctou_separated_tx`; `predicate_byte_identity` conformance.

---

## 9. Safe multi-slice order (too big for one commit)

Each sub-slice independently green + reversible. **S7-1 proper = the atomic composition + retirement (cannot be
split further without a live-unsafe intermediate).**

- **S7-2 first (fence, additive, no behavior flip):** add `fn_fence_active_tx` + wire it into every operation in
  the closed foreign-writer/mint inventory (including direct END mint) + S7-FENCE teeth. Independent of the
  composition; lands the chain-fork guard *before* activation. Reversible.
- **S7-1 foundation (still INACTIVE):** complete full binding, `Authorization → AttemptObservation`,
  record/apply orchestration, full ApplyPlan graph, boot-first reservation pass and their RED teeth. None of
  these changes may move the live wire yet.
- **S7-1 cutover (the atomic live flip — one commit/release):** relocate wire `:1568` behind
  `submit_authorized`; central 4-pre with the online predecessor guard; delete legacy 4-b `:1710-1972`;
  activate record/apply; land **R1-R7 together** (edge + allowlist + pre-re-sign `-12` stop + 2 producers +
  3 Redrive callers + consumers). All §8 teeth must already be RED-first proven.
  This is the release-critical, double-issue-risk step.
- **S7-3 (cleanup):** remove now-dead code (loop wrapper, `mac_recovery_invoked`, redrive histogram buckets).

> Rationale for fence-before-cutover: S7-2 is pure additive fail-closed and removes the highest-severity silent
> fork vector (`offline_code_replenish`) independently of the P2 wire work, shrinking the blast radius of the
> atomic S7-1 commit.

---

## 10. Overall verdict & resolved architecture decisions

**Verdict: REV2 READY FOR SPOT-REVIEW, NOT YET AUTHORIZED FOR LIVE CUTOVER.** The DB call-once core is real.
Rev2 additionally makes the type lifecycle implementable, closes the writer-before-authorize fork, assigns boot
replay, stops `-12` before byte mutation, makes the ApplyPlan complete and closes the mutation inventory.
Implementation may begin only with S7-2 and the INACTIVE S7-1 foundation; the atomic live flip requires its own
explicit GO after the teeth are demonstrated.

**Resolved decisions:**
- **R-Q1:** Redrive collapses to manual/RMR; no outcome hydration is allowed to become a wire retry.
- **R-Q2:** fence-before-cutover: separate S7-2 first.
- **R-Q3:** the two Sent+NotFound producers keep their **different** `transport_trace` `outcome_kind`
  (boot `RetryableServer` vs kvt2 `RetryableTransport`). Confirm each keeps its own semantics when bundling the
  `sent_not_found_to_manual` escalation (helper is agnostic; implementer must preserve).
- **R-Q4:** S7-1 is release-critical and requires its own explicit GO; no unilateral activation.

---

## 11. Round-2 resolution — FROZEN build checklist

External model-decorrelated round-2 (Sonnet ×5 + Fable critic, `wf_4350aaff-63e`) verdict: **NOT-YET, but no
double-issue and no chain-fork defect survives** — the P2 core (L1 call-once index `035:43`, sealed `Authorization`
lifecycle, R6 Redrive retirement) is proven sound (corrections 1/5/7/8/10 HOLD). The critic's two claims
(`NoResponse`=4th delta; stranded `CALL_STARTED`=BRICK) were **refuted, grounded**. Per the hard-cut rule
(external NOT-YET = last design round; one targeted fix for a surviving BRICK → freeze; **no round-3**), the 5
surviving defects are folded here as the frozen build checklist. **This design is FROZEN.** Further findings become
backlog, not a new review round.

| # | Sev | Defect (anchor) | Fix | Lands in |
|---|---|---|---|---|
| 1 | **BRICK** | `reset_stop_mode` (`admin.rs:300-396`) has no active-`PENDING_APPLY` guard. An operator calling it instead of `complete_operator_pending` on a held doc clears STOP → next boot (`GOING_ONLINE`, branch-f skipped) `resume_sending_to_error_retryable` (`boot_phase.rs:3804`)→ER→R6→RMR → then `complete_operator_pending`'s `doc_to_rmr` (`delivery_reservation.rs:1214`) expects `Sending`, finds `RMR` → `DocTransitionFailed` → operator resolution permanently bricked. Zero exposure pre-cutover. | In the `reset_stop_mode` `BEGIN IMMEDIATE` (before the mode CAS `admin.rs:341`): `SELECT COUNT(*) FROM delivery_reservation WHERE fiscal_number=? AND state='OUTCOME_OBSERVED' AND apply_state='PENDING_APPLY'`; if >0 → new `AdminError::PendingResolutionRequired` (direct to `complete_operator_pending`). Race-safe (in-tx, not pre-read). | **cutover** (= the operator-recovery activation, no longer deferrable) |
| 2 | oper | `Authorization` (`delivery_reservation.rs:264-276`) carries 3 of 5 binding fields — missing `capability_profile_version`/`endpoint_config_revision` (present in `NewReservation:58-60`, table, `ReservationRow:129-130`); `record_outcome` CAS WHERE (`:509-511`) omits them too. Latent (both always None at 53c5b13, `grpc.rs:683-684`); `submit_authorized` cannot enforce AO-2 echo-check from the token alone. | Add the 2 `Option<i64>` private fields + accessors; capture at `:365-369`; add to `Authorization::Ok` ctor `:432-440`; extend CAS WHERE with `AND capability_profile_version IS ? AND endpoint_config_revision IS ?` (NULL-safe `IS`). Zero blast radius (INACTIVE struct, 0 callers). | **foundation** |
| 3 | oper | `FnConfigError` (−13/−14) is a 4th delta: `routing_for_reject` (`prro-domain/.../mod.rs:1013`) → `(FnConfigError, NodeEffect::FnConfigError)`, discriminant `Rejected`; in `apply_outcome` online-Rejected arm (`delivery_reservation.rs:763-769`) `FnConfigError` hits the `_ => {}` catchall `:768` → falls to `doc_from_sending(…, Rejected)` `:772`. Post-cutover a −13/−14 permanently Rejects (R6 only escalates `ErrorRetryable`, never `Rejected`). No test covers it. | Add explicit `Some("FnConfigError") => doc_from_sending(tx, doc_id, DocState::ErrorRetryable)` before the catchall (edge `fiscal_documents.rs:255` legal); R6 then → RMR+STOP without a wire. Extend S7-APPLY-GRAPH tooth + add `ap10` test. | **foundation** |
| 4 | oper | Boot-pass insertion point unspecified → per-FN insertion skips later FNs on early return. | Resolved in **§7.1**: global pre-loop in `reconcile_pending_inner` before the FN loop; `HeldNotAutoRelease` = expected hold. | **§7.1 (done)** |
| 5 | oper | `S7-P4-BOOT` canary asserts wire-count, which is 0 either way in INACTIVE → order-swap can't RED. | Resolved in **§8**: assert intermediate reservation STATE (`OUTCOME_OBSERVED` + doc `ErrorRetryable`), not wire-count. | **§8 (done)** |

**Slice order unchanged (§9):** S7-2 fence (additive) → S7-1 INACTIVE foundation (now incl. #2, #3) → atomic
cutover (now incl. #1 guard) on explicit GO → S7-3 cleanup. #4/#5 are already folded above.

---

**Nothing in this document has been implemented.** Live send-path code is untouched.
