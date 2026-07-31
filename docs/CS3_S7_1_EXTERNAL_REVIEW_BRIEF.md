# CS-3 S7-1 Live-Cutover rev2 — External Spot-Review Brief

**To the reviewer:** you are a **model-decorrelated adversarial reviewer**. Prior internal gates on this project
were run by the *same* model family and have repeatedly missed defects that a decorrelated reviewer caught
(P2/P4 counterexamples, a "lossless" claim that was false, an over-broad fence). Your job is **not** to agree.
Your job is to **break** the safety argument below or prove a corner it does not cover. Be decisive: end with
**GO** or **NOT-YET + numbered corrections**, each grounded in a concrete `file:line` you verified yourself.

Read the companion rev2 design-of-record `CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` for the full composition.
The first external round returned NOT-YET. Rev2 claims to close those findings. This brief asks for a focused
attempt to break the corrections; do not re-litigate settled mechanics unless the correction still fails.

---

## 0. What is being activated (and why it is dangerous)

CS-3 S7-1 is the **live activation of the double-issue fix**. Today the real DPS wire call
(`send_chk_observed`, `stage_send.rs:1568`) is **not** gated by any per-document call-once — so a fiscal receipt
can be sent to the tax authority **twice** (the live bug this fixes). S7-1 relocates that wire behind a sealed
authorization and retires every blind-resend path. **A defect here is a live double-issue of a fiscal receipt**
(legal/financial harm) or a permanent **BRICK** of a legitimate cash register (a legit FN refused forever).

The repository primitives are already landed (S7-0, CI green), but the post-wire type lifecycle, full binding,
boot wiring and full ApplyPlan projection remain **TO_BUILD**. Rev2 retracts the old “no new fiscal semantics”
claim: the cutover is constrained by a full 7D graph with three previously adjudicated deltas and no undeclared
fourth delta.

**The invariant you must try to violate: P2 — at most ONE `send_chk_observed` per document lifetime**, across all
7 callers, all recovery paths, and all crash points.

---

## 1. Ground truth (verify against this, do not trust the prose)

- Branch `cs3-de-slice7-s0` @ `53c5b13`. Repo path `rust/prro/`.
- The wire seam: `send_chk_observed` — **exactly one** production call site today at
  `src/services/write_path/stage_send.rs:1568`. Production body `transports/dps/grpc.rs:250-270` = one RPC, no
  retry/fallback. **Please confirm the "exactly one call site" claim yourself** (grep the tree).
- The 7 callers of `stage_send::run` (all uniform 4-arg `run(pool, dps, doc_id, Some(sign_ctx))`, none constructs
  an authorization): `inline.rs:910`, `backlog_drain.rs:1321`, `backlog_drain.rs:2959`,
  `online_convergence.rs:561`, `boot_phase.rs:3072`, `boot_phase.rs:3685`, `boot_phase.rs:3943`.
- `run` (`stage_send.rs:1031`) is a MAC-recovery loop wrapper over `run_one_attempt` (`stage_send.rs:1238`); the
  4-pre/4a/4b body is in `run_one_attempt`.
- Landed primitives (`src/db/repositories/delivery_reservation.rs`): `authorize_submission` (:357), sealed
  non-`Clone` `Authorization` (:263-307, all fields private), current `record_outcome` (:491) and
  `apply_outcome` (:648). **Do not treat their current signatures/projection as cutover-ready:** rev2 explicitly
  changes the authorization→observation lifecycle and wraps apply in a complete service orchestration.
  Retirement replacement `sent_not_found_to_manual`
  (`src/services/reconciliation/sent_not_found.rs:67`).
- `with_immediate == BEGIN IMMEDIATE` (`src/db/tx.rs:3-16`) — the FN-writer serialization primitive.

---

## 2. The P2 safety argument — TRY TO REFUTE EACH LAYER

The claim: ≤1 wire per lifetime holds **iff** S7-1 relocates the wire behind `authorize_submission` and retires the
bypass routes atomically. The argument rests on three DB-level layers:

- **L1 (load-bearing):** partial unique index
  `ux_delivery_document_ever_started ON delivery_reservation(document_id) WHERE call_started_at IS NOT NULL`
  (`migrations/035:43-45`) + the `no_replace` trigger clause (`035:70-71`). At most one row per `document_id` can
  ever carry a non-NULL `call_started_at`. **Attack:** can any code path set `call_started_at` on a *second* row
  for the same `document_id` without tripping this? Can a document_id be reused/reset? Is the trigger bypassable by
  an `INSERT OR REPLACE` / `UPDATE`?
- **L2:** `authorize_submission` does call-once `NOT EXISTS` (`:372-382`), fresh `RESERVED_NOT_STARTED` insert,
  generation advance (`:400-413`), `RN→CALL_STARTED` CAS (`:416-430`) **all in one `BEGIN IMMEDIATE`**; any
  `rows_affected!=1 → Err →` rollback, `Authorization` minted only on full success (`:432`). **Attack:** is the
  whole 4-pre truly in ONE immediate tx? Can two callers interleave between the `NOT EXISTS` and the insert on
  separate connections under WAL and both proceed? Does the generation advance have a lost-update?
- **L3 (rev2):** `Authorization` is sealed, non-`Clone`, consumed **by value** by `submit_authorized`, which checks
  the full protocol/capability/endpoint/hash binding. The function returns the #4B post-wire
  `AttemptObservation`, never the reusable authorization; `record_outcome` accepts only that post-wire value.
  **Attack:** can any wire-capable value survive the call, can `AttemptObservation` reach the wire, or can any
  binding component be omitted/rebound? Verify wrong binding/hash means zero wire calls.

**Race closure claim:** two concurrent `run()` for the same doc serialize under `BEGIN IMMEDIATE`; the loser hits
`CallOnceAlreadyStarted` or the unique index and rolls back with no token. **Attack this directly** — construct an
interleaving (two callers; or a caller + a boot recovery) that reaches two wire calls.

**P3 pre-wire correction:** for an online-origin document, the same authorization transaction checks
`node_state.seed == document.previous_hash` before `CALL_STARTED`. Attack with a foreign writer committing H1
after the document was signed against H0 but before authorization. The expected result is zero wire I/O.

---

## 3. Bypass routes that MUST be retired atomically — is the list COMPLETE?

The same-model gate already found that plan §1.2 was **incomplete**: it named the transition edge + the `-12` loop
+ the two Sent+NotFound producers, but **missed three `Redrive` callers**. We disclose this so you look for what
ELSE is missing — **do not assume this list is now complete.**

| # | Surface | Anchor | Retire action |
|---|---|---|---|
| R1 | `(ErrorRetryable,Sending)` edge | `fiscal_documents.rs:257` | delete the arm |
| R2 | `ErrorRetryable` in 4-pre source allowlist | `stage_send.rs:1269` | remove `ErrorRetryable` as a source |
| R3 | `-12` MAC-recovery path | `stage_send.rs:1048-1082`, `mac_recovery.rs:516-535` | short-circuit before `run_mac_recovery`; PENDING/STOP, no re-sign/XML/CMS replacement, no `continue` |
| R4 | Sent+NotFound producer #1 | `boot_phase.rs:969` (`cas_sent_to_error_retryable_from_probe`) | → `sent_not_found_to_manual` |
| R5 | Sent+NotFound producer #2 | `kvt2_confirm.rs:1713` (`commit_sent_replay_envelope_1c_post`) | → `sent_not_found_to_manual` |
| **R6** | **3 Redrive callers** | `online_convergence.rs:560`, `backlog_drain.rs:1542`, `boot_phase.rs:3069` (`ErRedriveDecision::Redrive => stage_send::run`) | fixed decision: manual/RMR + STOP; never call `stage_send::run` |
| R7 | ER-redrive consumers presuming the edge survives | `dispatch_error_retryable_by_class`, `kvt2_confirm.rs:1645-1649`, `boot_phase.rs:947-948` | retarget/remove |

**Attack list for §3:**
- Enumerate **every** producer of `DocState::ErrorRetryable` and **every** producer of any `→ Sending` transition.
  Is there a fourth path (offline drain? a boot dispatcher? an admin/operator path?) that re-drives an
  already-`CALL_STARTED` doc to a wire that R1-R7 do not cover?
- Is there any transition that puts an *issued* doc (SFN stamped, `Sent`/`Kvt1`/…) back to `Signed` or
  `OfflineLocalAck` (the only two remaining wire sources after R2)? If yes, it is a re-wire surface.
- The gate's claim: after R1+R2 the three Redrive branches hit a `StateConflict` **no-op** → the doc spins in
  `ErrorRetryable` forever, nothing escalates = **BRICK**. Confirm or refute this liveness failure. Does anything
  eventually escalate a doc stuck in `ErrorRetryable`?
- Verify reservation-less legacy `SENDING`/`ERROR_RETRYABLE` rows are either rejected by a pre-deploy
  empty-in-flight gate or moved fail-closed to RMR/STOP; no `transport_trace` inference may certify them safe.

---

## 4. Legacy 4-b double-ownership — is the deletion boundary right?

`apply_outcome` and the legacy 4-b overlap on doc CAS, SFN stamp, seed advance and node block, but current
`apply_outcome` is **not** a complete replacement. Rev2 deletes the whole second `with_immediate` at
`stage_send.rs:1710-1972` only after two replacement boundaries exist: record owns evidence + trace + audit +
early STOP/BLOCKED; the shared apply service owns doc/SFN/seed + shift/closing-cash + APPLIED/pointer clear.

**The sharp edge:** the legacy 4-b seed advance **skips** its `previous_hash==node-seed` equality gate when
`mac_recovery_attempts>=1` (`stage_send.rs:1800`). On a `-12` recovery the re-signed sha differs from
`previous_hash`; if both 4-b and `apply_outcome` ran, the **last writer wins** and the seed lands on the **wrong
tip → FN chain fork** (P3 violation).

**Attack list for §4:**
- Is `1710-1972` the exact and complete deletion boundary? Does anything *outside* it depend on a side effect
  *inside* it (e.g. the shift-confirm edges `:1829-1859`, the `-11` block `:1877`, the trace completion `:1895`)
  that `record_outcome`/`apply_outcome` do **not** reproduce?
- Verify the pre-lock `closing_cash_kop_for_4b` derivation (`:1629-1700`) is performed by the shared live/boot
  apply service from durable state on every invocation; crash between record and apply must not lose an
  ephemeral cash value. The service calls the sole shift writer; reject any repository→service inversion.
- Prove or refute: after cutover, **exactly one** path advances the seed per document. Is there any interleaving
  (same-tx vs separate-tx placement of `record`/`apply`) where the doc CAS collision is *silent* rather than
  fail-closed?
- Attack the full ApplyPlan pin: every legal evidence leaf must match
  `{target_state,retry_class,SFN/seed/shift,node_effect,audit,probe,fence}`. In particular `-13/-14`
  (`FnConfigError`) must not fall through to blanket `Rejected`, and the three declared 3.2 deltas must be the
  complete delta set.

---

## 5. Closed foreign-writer inventory (fence, S7-2) — is it complete?

Several operations advance the FN chain tip / mint docs / consume codes with **no** active-reservation fence.
`get_active_for_fn` (`delivery_reservation.rs:183`) is **pool-bound read-only** → a read-then-write across separate
connections races the fence (TOCTOU). The design adds a tx-bound
`fn_fence_active_tx(&mut WriteTxConn, fiscal_number)` with a predicate **byte-identical** to the inline query at
`:195-196`, wired into each writer's own `BEGIN IMMEDIATE`, refusing when active — **excluding `apply_outcome`**
(it *is* the active reservation; self-fence = deadlock).

Known surfaces include `stage_send.rs:1809`, `stage_offline_ack.rs:495`, `stage_offline_ack.rs:187-505`,
`offline_code_replenish.rs:267` (**highest risk — no equality gate at all**), `boot_phase.rs:1814` (quiescent,
pre-ingress), `stage_acquire.rs:49` (new-doc issuance), `offline_session.rs:79`, `stage_sign.rs:992`, plus
`offline_sync/backlog_drain.rs:2701-2779::mint_session_end_prepared`, which bypasses `stage_acquire`, allocates
LND and inserts a prepared END directly. The old brittle count “8” is retired.

**Attack list for §5:**
- Is a pool-bound read genuinely insufficient here, or does an existing lock (the FN write-lease
  `fn_gate.rs`, `reconcile_mutex` `app.rs:759`) already serialize these against `authorize_submission`? Prove the
  concurrency window is real (the design claims drain-side and inline-side hold **distinct** locks).
- Is excluding `apply_outcome` from the fence safe, or does that leave a window where `apply_outcome`'s own seed
  advance races a foreign writer that already passed the fence in a separate tx?
- Is `offline_code_replenish.rs:267` really the highest fork risk (advances seed to `sha256(request_xml)` with no
  equality gate)? Any writer with a worse profile?
- Confirm both ordinary issuance and direct END mint refuse inside their own write transaction when the fence is
  active. Then run the writer-before-authorize H0→H1 attack: only the independent pre-wire equality may stop it.

---

## 6. Crash-window matrix — is exactly-once preserved across every crash?

The rev2 claim is now about explicit production wiring, not helper availability. Before ordinary document
dispatch, boot must (1) resume all `CALL_STARTED`, then (2) apply all
`OUTCOME_OBSERVED+PENDING_APPLY`, then (3) run ordinary document dispatch. Neither reservation pass may call the
wire or construct a `NewReservation`. The CALL_STARTED resume transaction must also complete the in-flight
trace as crash/unknown and append its recovery audit; calling the repository UPDATE alone is incomplete.

| Crash window | Reservation state | Boot action | Re-wire? |
|---|---|---|---|
| before 4-pre commit | none (rolled back) | fresh authorize later | no |
| after 4-pre, before/during wire | `CALL_STARTED` | resume → `NoResponse{Crashed}` + PENDING + STOP | no |
| after wire, before `record` | `CALL_STARTED` | resume → operator completion resolves unknown | no |
| after `record`, before `apply` | `OUTCOME_OBSERVED`+PENDING_APPLY | re-apply via `apply_outcome` (gen-CAS idempotent) | no |
| mid-`apply` (one tx) | PENDING_APPLY / APPLIED | re-apply / no-op | no |
| after APPLIED | APPLIED | terminal no-op | no |

**Attack list for §6:**
- Find a crash window where boot **re-sends** a `CALL_STARTED` doc (any of the 7 routes; a boot dispatcher that
  calls `stage_send::run` for a doc with `call_started_at IS NOT NULL`).
- The "after wire, before record" window loses the wire outcome (unknown if DPS accepted). The design routes this
  to SubmittedUnknown → STOP → operator, **never** a blind re-send. Is that consistent, or is there an
  auto-redrive that fires first? (This ties to R6 — a surviving `Redrive` would violate it.)
- Remove each boot reservation pass, and separately swap the ordering with ordinary dispatch. The production
  entrypoint test must RED in all cases; testing the repository helpers alone is insufficient.

---

## 7. Resolved architecture decisions to challenge

- **R1:** Redrive collapses to manual/RMR; no automatic wire retry in S7-1.
- **R2:** additive S7-2 fence lands before S7-1 cutover.
- **R3:** the two Sent+NotFound producers keep **different** `transport_trace outcome_kind` (boot `RetryableServer`
  vs kvt2 `RetryableTransport`). Confirm each keeps its own trace semantics when bundling
  `sent_not_found_to_manual`.
- **R4:** inactive foundation may be built after design GO; the live S7-1 flip still requires a separate explicit
  implementation GO after all RED teeth bite.

### 7.1 Teeth that must prove the correction, not merely stay green

- The concurrent P2 canary must RED when the DB call-once layer is disabled and when a direct/second wire call
  bypasses `submit_authorized`. Removing R1/R2 is **not** a valid P2 bite because the DB layer still holds.
- R1/R2 are tested as liveness/retirement teeth: an `ErrorRetryable` document must reach RMR/STOP rather than
  spin on `StateConflict`.
- A `-12` tooth snapshots envelope hash, XML/CMS and `previous_hash`; all remain unchanged after one wire.
  Re-enabling either `run_mac_recovery` or `continue` must RED.
- The boot tooth exercises the production boot entrypoint, not repository helpers in isolation.
- The ApplyPlan tooth round-trips every legal classifier leaf through record storage and the same boot/live apply
  orchestration, comparing the complete 7D graph.
- The fence tooth covers both `stage_acquire` and direct END mint, plus the writer-before-authorize H0→H1 race.

---

## 8. Verdict required

End with one of:
- **GO** — the P2 argument holds, the retirement list (§3) is complete, the deletion boundary (§4) is exact, the
  fence (§5) is sound, the crash matrix (§6) is exactly-once. State which claims you verified empirically (grep /
  DB / code read) vs reasoned.
- **NOT-YET** — numbered, each a concrete `file:line` defect or an uncovered corner, ranked by severity
  (double-issue > chain-fork > BRICK > operational), with the minimal correction. Prefer **simplify** over
  **escalate** where a guard is provably dead.

Nothing has been implemented. Live send-path code is untouched at `53c5b13`.
