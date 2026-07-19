# Spec #1 — Executable Transition Contract / State Model (rev 3)

**Status: 🔒 DESIGN-LOCKED (rev 3). 2026-07-14.** External audit confirmed LOCK-READY (S1-V1…V4,
S1-I1 all CLOSED). Residuals (§9.4: GAP-CM1/CM2/CM3, GAP-TEETH, shift-matrix code-grounding) are the
**pre-RED-pins-freeze** proof-quality gate — **not** design blockers.
Rev 2 integrated the diff-review findings (S1-V1…V4, S1-I1); **rev 3 adds the two curation
appendices** the auditor left open — **Appendix A** (full normative doc/session/shift/mode matrices,
code-grounded to file:line) and **Appendix B** (the mutation→independent-invariant map) — closing
S1-V2 / S1-I1 up to the honestly-flagged pre-RED-pin-freeze residuals in §9.4. Grounded on
`origin/main 8ec99ca`. This is the **single source** from which runtime edges + the audit contract
derive; the fuzzer keeps **independent** invariants (§4, common-mode).

---

## 1 · Problem
Transition rules live as imperative guards scattered across ~8 FSMs; a transition in one assumes
implicit state of others → B11 broke **at the seams**. This spec replaces the scattered guards with
**normative data-driven transition matrices** + a **pure admission function**, and corrects the
naïve "relist NodeMode as the axes" of rev 1.

## 2 · The state model (aggregates, axes, and one derived projection)

**Machines with their own normative FSM (each a per-FN aggregate):**
- **Document** (`DocState`, 14 states) — per-doc lifecycle.
- **Shift** (9 states, **15 edges**, §3) — per-FN.
- **Offline-session** — a **separate per-FN aggregate**, NOT a document sub-FSM: one session spans an
  ordered **cohort of many documents**, has its own uniqueness + lifecycle
  (`offline_sessions.rs:141/515`). The snapshot carries `active_session: Option<{id, state}>`;
  `Closed`/`Aborted` are the **history of a specific session**, not a global axis (S1-V1).

**Orthogonal ADMISSION AXES** (independent inputs to the admission function — the old `NodeMode`
was NOT orthogonal, it mixed several; `enums.rs:81`):
| Axis | Meaning |
|---|---|
| `connectivity_evidence` | forward-progress evidence (Spec #2) — *not a mode* |
| `legal_budget` / `code_pool` | 168h/36h/24h remaining; offline codes / close-reserve |
| `crypto_key_health` | signer/cert/key state (was buried in `CryptoDegraded`) |
| `clock_health` | monotonic vs wall, NTP-jump / rollback detection |
| `protocol_binding` | the shift's bound `IngressProfileId` + `DpsProtocolId` (Plan §4) |
| `holds` | `{local?, fleet?}` — independent |
| `recovery_health` | ok / `RequiresManualReconciliation` / degraded |
| `shift_phase` | projection of the shift aggregate |

**`node_mode` is a DERIVED legacy projection** of these axes (kept for wire/compat), **not** an
input axis (S1-V1).

## 3 · Normative transition matrices (data-driven, grounded)

A machine's matrix is rows `(from_state, event, guard) → (to_state, EffectPlan)`. Rules:
- **Events are PAYLOADED:** `Fiscalize{doc_type}`, `Drain{phase, outcome}`, `BootRecover{observed}`,
  `Probe{...}`, `FleetCommand{...}`, `AdminCommand{...}`, `Timer{...}`, plus the send outcomes
  (Spec #2's three fields).
- **Exactly 0 or 1 matching row** per `(machine, from, event)`; **overlapping guards = contract
  error** (S1-V2).
- **Dormant edges are marked explicitly** (present in the graph, no auto-invoker — e.g.
  `OfflineLocalAck→Cancelled` is operator/force-seam only; the sfn-less A.3-PR-B dormant edges).
- The **complete matrices are curated from the cited seams** and machine-checked; the executable
  table is the artifact, this spec is its contract.

**Shift matrix (15 edges, verbatim from `shifts.rs:73`):** `Created→Opening`(1),
`Created→OpenedLocalPendingDrain`(2), `Opening→Opened`(3), `Opening→RMR`(4),
`OpenedLocalPendingDrain→Opened`(5), `→RMR`(6), `→ClosingLocalPendingDrain`(7), `Opened→Closing`(8),
`Opened→ClosingLocalPendingDrain`(9), `Closing→Closed`(10), **`Closing→Opened`(11)**, `Closing→RMR`(12),
`ClosingLocalPendingDrain→Closed`(13), `→RMR`(14), `Opened→RMR`(15, M2-N2a).

**Document matrix (from `fiscal_documents.rs:171` `allowed_transition`):** includes `Signed→Sending`,
`Sending→Sent`, `Sent→{Kvt1, ErrorRetryable, RMR}`, `ErrorRetryable→{Sending, Rejected, RMR}`,
`OfflineLocalAck→{Sending, Cancelled(dormant)}`, `Prepared/Signed→Aborted` (#192 terminalise). Each
row must name its **event** and **guard**; a `BootRecover{observed:Sending}` or `Drain{outcome}` edge
that exists in code but not in the matrix is a **gap**, distinguishable from an intentionally-absent
edge by the dormant-marking rule (fixes old RP-1 ambiguity, S1-V2).

## 4 · Admission = ONE pure function, a TOTAL priority rule-list (S1-V3, open-q3)
`admission(OperationKind, axes) → Allow(plan) | Denied(reason) | Deferred(reason) | NoTransition`.
Implemented as a **total, ordered rule-list** (not a flat predicate soup), minimal precedence:
1. invariant / `STOP` / `RMR` (hard stop)
2. crypto / legal impossibility (key dead, cap exhausted)
3. shift / protocol-binding / caps
4. **holds** — only for *new business*; a **mandatory drain / reconciliation is NEVER blocked by a
   HOLD**
5. session phase
6. connectivity routing (online vs offline lane)

**Policy denial ≠ stale-CAS (S1-V3).** The oracle's `Denied/Deferred` is a *decision*; the store's
result is separately `Applied | VersionConflict | NotFound | InvariantBreach` — mapping to the real
persistence seam `Applied | Forbidden | Conflict | NotFound` (`fiscal_documents.rs:154`,
`shifts.rs:95`; `Forbidden` = caller/policy decided *before* the DB, `Conflict` = row diverged). A
persistent HOLD/legal denial surfaced as `Conflict` would make the actor **recompute forever** — so
it MUST surface as `Denied/Deferred`, and the actor stops (or defers), never busy-loops.

## 5 · Derivation (common-mode-safe — S1-I1)
Runtime edges = the matrices; the audit contract = the matrices. The **fuzzer oracle is NOT derived
from the matrices** and **must not import table-derived predicates.** Common-mode is defended by a
**mutation matrix** — for every: add-edge / remove-edge / weaken-guard / change-effect /
change-audit / change-protocol-binding — a **map "safety-semantic → an independent invariant with a
tooth"** must show the mutation is caught by an oracle that does not read the table.

## 6 · `NoEphemeralDocStateAtQuiescence` (renamed + precise — S1-V4)
Only **ephemeral** `Prepared / Signed / Encrypted / Sending` are forbidden **at rest**;
`ErrorRetryable / Sent / Kvt1 / Kvt2 / OfflineLocalAck` **legitimately rest**
(`invariant_scan.rs:26/48`). A `Sending` with an **active reservation / crash-marker** is legal
mid-flight — it is not "at rest." **Three distinct quiescence pins:**
- **ingress-idle:** mailbox empty, no active call/reservation, actor fenced.
- **graceful shutdown:** intake closed **and** actors successfully joined — a *grace-timeout* with
  detached workers (`supervisor.rs:805`) is **NOT** a quiescent boundary.
- **boot:** only **post-recovery, post-fencing**, with runtime deps ready (`app.rs:687` runs the
  detector, not a repairer — a SIGKILL-after-`CallStarted` crash marker at raw boot is **correct**,
  not a violation).

## 7 · Invariants across all transitions
single-writer per FN (structural); no network/crypto in a write tx; `NoEphemeralDocStateAtQuiescence`;
delivery certainty never dropped (Spec #2); every transition audited (actor/reason); guarded-CAS
idempotency (`VersionConflict` → recompute); advance-at-SEND seed.

## 8 · RED-pins (rev 2)
- **RP-1 (0-or-1 row + gap-vs-dormant):** every `(machine, from, event)` matches ≤1 row; a code edge
  absent from the matrix fails as a **gap** unless explicitly dormant-marked; overlapping guards
  fail a static contract check.
- **RP-2 (denial ≠ conflict):** a HOLD/legal denial returns `Denied/Deferred` and the actor does NOT
  recompute; only a `VersionConflict` triggers recompute.
- **RP-3 (quiescence ×3):** the three boundary pins each pass on a correct state and fail on a real
  ephemeral-at-rest; a crash-marked `Sending` at raw boot is NOT flagged.
- **RP-4 (audit contract):** every applied edge emits its declared audit event.
- **RP-5 (common-mode mutation matrix):** each listed mutation is caught by a table-independent
  invariant tooth.
- **RP-6 (single-writer):** two concurrent commands for one FN never both apply a state-mutating plan.

## 9 · Open questions (answered per audit)
1. Offline-session = separate per-FN aggregate (not a doc sub-FSM); snapshot `active_session`.
2. Machines kept as **separate matrices on a common schema**, applied by one atomic `TransitionPlan`
   (a single cross-product table would combinatorially explode).
3. Admission = one pure function realised as the total priority rule-list above, taking
   `OperationKind`; HOLD inapplicable to mandatory drain/reconciliation.
4. **ADDRESSED (rev 3):** the exhaustive doc/session/mode matrices + the mutation→independent-invariant
   map are now **Appendix A / Appendix B**, code-grounded to file:line on `8ec99ca`. Honestly-flagged
   residuals to close **before RED-pins freeze** (not design blockers): (a) the **shift** matrix's
   events/guards are reconstructed from spec §16 and must be code-grounded to A.1/A.2 rigor; and four
   **common-mode GAPs** — GAP-CM1 (`is_issued` is a shared writer/scanner surface, mitigated by the
   forked-set teeth), GAP-CM2 (manual-lockstep literal sets → mutation-test them), GAP-CM3 (canonical
   XML truth is only *fully* independent via out-of-process **WebCheck replay** — not closeable
   in-repo), GAP-TEETH (add proactive revert-canaries for `invariant_scan` checks 2/2b, 5, 15 so every
   mutation row has a proactive, not merely reactive-at-quiescence, tooth).


---

## Appendix A — Normative transition matrices

Source of truth: each machine's `allowed_transition` whitelist is the legality gate; every listed edge short-circuits to `Forbidden` before any DB call if not whitelisted. "Dormant" = whitelisted (or enum-declared) but **zero production auto-invoker** (operator/force-seam or deferred-wiring only). Multiple distinct events driving the same `(from,to)` edge are listed as separate rows.

### A.1 — `document` machine

| from | event | guard | to | effect | dormant | evidence |
|---|---|---|---|---|---|---|
| Prepared | Fiscalize{doc_type} (sign persist) | stage-3 sign ok; CAS from=PREPARED | Signed | PREPARED→SIGNED; persist PayloadXml+SignedXml; update unsigned_xml_sha256; audit; no seed/sfn | no | whitelist fiscal_documents.rs:175; stage_sign.rs:522 |
| Prepared | (none) — **TODO-curation** | whitelisted, zero producer (pre-SENT reject fires at Sending→Rejected; pre-acquire refusals are audit-only) | Rejected | n/a — no invoker | yes | whitelist fiscal_documents.rs:176; grep: only whitelist line |
| Prepared | BootRecover / inline-refuse (terminalise_inbox) | request refused post-mint pre-sign; dangling row∈{PREPARED,SIGNED}; CAS from=PREPARED | Aborted | PREPARED→ABORTED (non-issued terminal); audit INLINE_REFUSED_DOC_ABORTED; no seed/sfn | no | whitelist fiscal_documents.rs:181; inline.rs:426 |
| Signed | inline-refuse (terminalise_inbox) | post-sign refusal (dispatch-internal/refused/offline-refused); dangling SIGNED; CAS from=SIGNED | Aborted | SIGNED→ABORTED (#192 fix); audit INLINE_REFUSED_DOC_ABORTED; may flag aborted_shift_class for RMR | no | whitelist fiscal_documents.rs:182; inline.rs:426 |
| Signed | Fiscalize{offline} ack — unstamped bare-MAC | node Offline/GoingOffline; read_offline_stamp_tx=None (bare `<MAC>`, can never drain, DPS -9) | Aborted | SIGNED→ABORTED in-envelope; code pool untouched; audit OFFLINE_ACK_UNSTAMPED_ABORTED; Refused(UnstampedBareMacAbort) | no | whitelist fiscal_documents.rs:182; stage_offline_ack.rs:413 |
| Signed | BootRecover{offline code exhaustion} | boot post-sign offline refusal, CodePoolExhausted; CAS from=SIGNED | Aborted | SIGNED→ABORTED (P1 boot-resume twin of #192); audit BOOT_DOC_ABORTED; idempotent on replay | no | whitelist fiscal_documents.rs:182; boot_phase.rs:151 |
| Signed | (none) — **TODO-curation** | whitelisted, zero producer; ENCRYPTED is Checkbox-only contour | Encrypted | n/a — no invoker | yes | whitelist fiscal_documents.rs:183; grep: only whitelist line |
| Signed | (none) — **TODO-curation** | whitelisted, zero direct producer; ER reachable only via Signed→Sending→routed-ER | ErrorRetryable | n/a — no invoker | yes | whitelist fiscal_documents.rs:184; grep |
| Signed | Fiscalize{offline} ack — stamped | node Offline/GoingOffline; shift Opened; active OPEN session; offline stamp present (code_lnd+consumed_at+dps_code); doc∈FN; via transition_signed_to_offline_local_ack_tx | OfflineLocalAck | SIGNED→OFFLINE_LOCAL_ACK; stamps offline_fiscal_no/date/session_id/dps_code; advances MAC seed (M2-01 offline issuance); audit | no | whitelist fiscal_documents.rs:185; stage_offline_ack.rs:445 (gate :645) |
| Encrypted | BootRecover{stranded Encrypted} | boot finds ENCRYPTED (Checkbox-only); 1-tick deferral; CAS from=ENCRYPTED | ErrorRetryable | ENCRYPTED→ERROR_RETRYABLE (reroute to online Pattern B); audit BOOT_ENCRYPTED_REROUTED | no | whitelist fiscal_documents.rs:188; boot_phase.rs:3820 |
| Sent | Drain{confirm,KVT1} (online finalize) | KVT1 ack (data_sign valid); reached SENT this tick; CAS from=SENT | Kvt1 | SENT→KVT1; stamp first_kvt1_at (COALESCE); persist Kvt1Raw; reset consecutive_holds; audit; advances→Kvt2 same envelope | no | whitelist fiscal_documents.rs:189; kvt2_advance.rs:307 |
| Sent | Drain{confirm,lastChk} (offline drain) | offline-drain lastChk confirm yields KVT1; CAS from=SENT | Kvt1 | SENT→KVT1; stamp first_kvt1_at; persist Kvt1Raw; complete transport_trace; audit; advances→Kvt2 | no | whitelist fiscal_documents.rs:189; offline_sync/kvt2_confirm.rs:1576 |
| Sent | BootRecover{probe=CheckAck id-match} | boot SENT probe returns KVT1 (id==transport_request_id); CAS from=SENT | Kvt1 | SENT→KVT1; stamp first_kvt1_at; persist Kvt1Raw + trace completion; audit; single with_immediate | no | whitelist fiscal_documents.rs:189; boot_phase.rs:784 |
| Sent | BootRecover{probe=NotFound} | boot SENT probe: DPS no record (ProbeOutcome::NotFound); CAS from=SENT | **RequiresManualReconciliation** *(CS-3-CORRECTED, rev3 §3.5 — was `ErrorRetryable`+tick-2 redrive; a `Sent` doc crossed the issuance CAS ⇒ possibly issued-but-unconfirmed, blind re-wire double-issues, and the doc leaves the drain cohort so a successor could become head)* | SENT→RMR **+ `node_state.mode→STOP_MODE` + trace-complete + audit `BOOT_SENT_LAST_CHK_NOTFOUND_RECONCILE`, ALL in ONE `with_immediate`** (commit all four or none); **no send-redrive** | no | new edge `(Sending/Sent → RMR)` design §3.4/§3.5; boot_phase.rs:964 |
| Sent | Drain{confirm,lastChk NotFound} (offline drain) | offline drain lastChk: DPS no record; CAS from=SENT | **RequiresManualReconciliation** *(CS-3-CORRECTED, rev3 §3.5 — was `ErrorRetryable`+re-drive)* | SENT→RMR + `node_state.mode→STOP_MODE` + trace/audit atomically; **no re-drive**; NOT via `shifts::force_to_manual_reconciliation_with_audit` (it skips the node mirror, and shift-RMR has no exit) | no | offline_sync/kvt2_confirm.rs:1696 |
| Sent | BootRecover{probe=CheckAck id-mismatch} | boot SENT probe (W11): lastChk id≠transport_request_id (protocol divergence, not retryable); CAS from=SENT | RequiresManualReconciliation | SENT→RMR (operator handoff); complete trace Rejected(LAST_CHK_MISMATCH); audit BOOT_SENT_LAST_CHK_MISMATCH_RM; seed NOT rolled back | no | whitelist fiscal_documents.rs:203; boot_phase.rs:874 |
| Kvt1 | Drain{confirm,KVT2} | KVT2 advance proof persisted; at KVT1; CAS from=KVT1 | Kvt2 | KVT1→KVT2; persist Kvt1Raw; reset consecutive_holds; audit OFFLINE_DRAIN_KVT2_ADVANCED | no | whitelist fiscal_documents.rs:204; kvt2_advance.rs:320/397 + offline_sync/kvt2_confirm.rs:1589 |
| Kvt1 | (none) — **TODO-curation** | whitelisted, zero producer; KVT1 docs passively held (passive_hold_kvt1) or advance to KVT2, never routed to ER | ErrorRetryable | n/a — no invoker | yes | whitelist fiscal_documents.rs:205; passive_hold_kvt1 boot_phase.rs:1223 does NOT CAS |
| Kvt2 | Drain{confirm,ACK} (finalize) | KVT2 ack; CAS from=KVT2; Conflict on already-Ack ⇒ idempotent AlreadyAcked | Ack | KVT2→ACK (issued+confirmed terminal); write outbox (PK document_id, seq=lnd); rich audit; chain-seed guard | no | whitelist fiscal_documents.rs:206; stage_finalize.rs:253 |
| OfflineLocalAck | Drain{send} (return-online) | node back online; W9b drain; doc∈OFFLINE_LOCAL_ACK; 4-pre source-state CAS; CAS from=OFFLINE_LOCAL_ACK | Sending | OFFLINE_LOCAL_ACK→SENDING (Pattern B intent-marker); mark_submission_attempted_at; allocate trace attempt; audit STAGE_SEND_INTENT_MARKED | no | whitelist fiscal_documents.rs:227; stage_send.rs:1414 |
| OfflineLocalAck | AdminCommand/force-seam (operator abandon) | NO auto-invoker; manual escape if drain abandoned mid-flight; wiring deferred (AUD-L1-2) | Cancelled | n/a — operator/force-seam only | yes | whitelist fiscal_documents.rs:228 + comment 218-219; neg-pinned tests/fiscal_documents_offline_local_ack_edges_locked.rs:25,71 |
| ErrorRetryable | BootRecover{ER non-retryable class} | boot: ER doc with non-retryable durable retry_class; no auto-retry (stage_send §4.2); CAS from=ERROR_RETRYABLE | RequiresManualReconciliation | ER→RMR (operator triage); audit BOOT_ER_ESCALATED_TO_MANUAL | no | whitelist fiscal_documents.rs:232; boot_phase.rs:2908 |
| ErrorRetryable | BootRecover{ER budget exhausted} | boot: TransientRetry attempts_used≥MAX_BOOT_ATTEMPTS; CAS from=ERROR_RETRYABLE | RequiresManualReconciliation | ER→RMR; audit BOOT_ER_BUDGET_EXHAUSTED (distinct signal) | no | whitelist fiscal_documents.rs:232; boot_phase.rs:2954 |
| ErrorRetryable | Drain{reject/ER-class} (offline drain escalation) | offline drain ER-class guard; §16.7 family-1 escalation; CAS from=ERROR_RETRYABLE; Conflict=structural drift | RequiresManualReconciliation | ER→RMR; audit (er_class_guard, dispatch_via drain); operator handoff | no | whitelist fiscal_documents.rs:232; offline_sync/backlog_drain.rs:1705 |
| Signed | Fiscalize{online} send / retry | online stage 4-pre; node online; doc SIGNED (offline_fiscal_no NULL); envelope built; CAS from=SIGNED | Sending | SIGNED→SENDING (Pattern B intent-marker); mark_submission_attempted_at; allocate trace (is_probe=false); audit STAGE_SEND_INTENT_MARKED | no | whitelist fiscal_documents.rs:238; stage_send.rs:1414 |
| Encrypted | (none) — **TODO-curation** | whitelisted, zero producer; Encrypted rerouted to ER at boot, never sent directly | Sending | n/a — no invoker | yes | whitelist fiscal_documents.rs:239; grep: only whitelist line |
| Sending | SendOutcome{Sent} (wire ACK) | post-wire CAS; WireDecision::Sent{sfn non-empty}; CAS from=SENDING; non-Applied=structural breach | Sent | SENDING→SENT; set server_fiscal_no; **ADVANCE-AT-SEND** seed advance (online-origin, A.3 D3 sfn⇔seed lockstep); complete trace | no | whitelist fiscal_documents.rs:240; stage_send.rs:1704 |
| Sending | SendOutcome{Routed:transient/retryable} | post-wire CAS; WireDecision::Routed target=ErrorRetryable (route_dps_error transient/transport); CAS from=SENDING | ErrorRetryable | SENDING→ERROR_RETRYABLE; complete trace (RetryableTransport/Server); audit; NO sfn, NO seed advance; re-drive via ER→Sending | no | whitelist fiscal_documents.rs:246; stage_send.rs:1704 (error_routing.rs:292) |
| Sending | BootRecover{in-flight Sending} | boot finds stranded SENDING (crash mid-send); DPS no-dedup ⇒ re-send is double-issue hazard ⇒ route ER for probe-first; CAS from=SENDING | ErrorRetryable | SENDING→ERROR_RETRYABLE; audit BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE | no | whitelist fiscal_documents.rs:246; boot_phase.rs:675 |
| Sending | SendOutcome{Routed:TerminalReject} (pre-SENT reject) | post-wire CAS; WireDecision::Routed target=Rejected (terminal-reject codes); before sfn/seed advance; CAS from=SENDING | Rejected | SENDING→REJECTED (non-issued terminal legitimately rests; lnd consumed, seed NOT advanced — **D2 pin**); complete trace Rejected; audit | no | whitelist fiscal_documents.rs:247; stage_send.rs:1704 (error_routing.rs:304) |
| ErrorRetryable | Fiscalize/Drain retry (re-send) | ER dispatcher re-drives (online tick or W9b drain); 4-pre source-state CAS; CAS from=ERROR_RETRYABLE | Sending | ERROR_RETRYABLE→SENDING; mark_submission_attempted_at; allocate fresh trace attempt; audit STAGE_SEND_INTENT_MARKED | no | whitelist fiscal_documents.rs:248; stage_send.rs:1414 |
| ErrorRetryable | SendOutcome{-12 MAC-recovery failure override} | W10.4 step2d: HashNotExtractable/CounterExhausted/second -12 short-circuit; budget spent, no fresh send; CAS from=ERROR_RETRYABLE | Rejected | ERROR_RETRYABLE→REJECTED (terminal); (2061) audit MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH; (2014) no extra audit; wire_status_code -12 | no | whitelist fiscal_documents.rs:256; stage_send.rs:2014 / 2061 |

**Removed (A.3 PR-B) — no longer in whitelist, must NOT be re-admitted:** `(Encrypted,Sent)`, `(Sent,Rejected)`, `(OfflineLocalAck,Sent)`, `(OfflineLocalAck,Kvt2)`, `(Sending,Kvt1)`, `(ErrorRetryable,Sent)`, `(ErrorRetryable,Kvt1)`. Policy D3: post-SENT reject ⇒ RMR, never Rejected (seed already advanced at SEND). Seed pins: online issuance = Sending→Sent CAS; offline issuance = Signed→OfflineLocalAck; Rejected/Aborted never advance the seed; RMR escalations never roll it back.

**⟶ CS-3 CORRECTION (rev3 §3.5 — `CS3_REMEDIATION_DESIGN.md`):** the **issued-doc redrive** `Sent →
ErrorRetryable{NotFound} → … → Sending → wire` is **retired** — both `Sent + last_chk NotFound` producers
(boot + offline-drain) now route atomically to **RMR + `STOP_MODE`** (rows above), because a `Sent` doc is
possibly issued-but-unconfirmed and a blind re-wire double-issues (P2). The still-legal `(Sending,ErrorRetryable)`
/ `(ErrorRetryable,Sending)` edges carry ONLY **never-issued transient** re-sends and are additionally bounded
by the per-document lifetime call-once guard (design §2), so they can no longer double-wire an issued doc.

### A.2 — `offline_session` machine

Whitelist (offline_sessions.rs:141-152) permits exactly 6 edges. CLOSED/ABORTED terminal. Column-stamp contract: Draining⇒drained_at, Closed⇒closed_at, Aborted⇒reason_abort (never closed_at).

| from | event | guard | to | effect | dormant | evidence |
|---|---|---|---|---|---|---|
| (none / INSERT) | AdminCommand{GoOffline} / service open_session | mode CAS ONLINE→OFFLINE flipped==true; partial UNIQUE ux_offline_active: no OPENING/OPEN/DRAINING for FN (else AnotherSessionActive) | OPENING | INSERT row state=OPENING, stamp opened_at; same envelope: mode ONLINE→OFFLINE + audit ADMIN_GO_OFFLINE(Critical) | no | offline_sessions.rs:160-177; admin.rs:463-483; ux index :156-159 |
| OPENING | AdminCommand{GoOffline} — same envelope as INSERT | allowed(Opening,Open); CAS WHERE state='OPENING' must be Applied (else programming-bug) | OPEN | UPDATE state=OPEN (state-only, no timestamp); audit OFFLINE_SESSION_OPENED(Info) | no | offline_sessions.rs:145,278-285; admin.rs:484-507; service twin offline_session.rs:90-111 |
| OPEN | Probe (return_online_probe tick)→backlog_drain first-entry | allowed(Open,Draining); state==Open (skip if already DRAINING); CAS WHERE state='OPEN' Applied (reconcile_mutex serialises) | DRAINING | UPDATE state=DRAINING, stamp drained_at=COALESCE(...); audit OFFLINE_SESSION_DRAIN_STARTED(Info); begins per-doc OLA→SENDING→…→ACK replay | no | offline_sessions.rs:147,231-242; backlog_drain.rs:840-886; probe return_online_probe.rs:469-517 |
| DRAINING | Probe/drain finalize (commit_finalize_envelope) — all cohort ACK | allowed(Draining,Closed); mode CAS GOING_ONLINE→ONLINE Applied; session CAS WHERE state='DRAINING' Applied; drain-completable (all ACK; REJECTED/MANUAL excluded) | CLOSED | UPDATE state=CLOSED, stamp closed_at=COALESCE(...); mode→ONLINE; optional pending-drain shift ladder close; audit OFFLINE_SESSION_CLOSED | no | offline_sessions.rs:149,243-254; backlog_drain.rs:3180-3223; completable :800-816 |
| OPENING | Operator force-seam (service abort_session) — **NO prod auto-invoker** | allowed(Opening,Aborted); reason_abort non-empty (else MissingReasonAbort) | ABORTED | UPDATE state=ABORTED, stamp reason_abort (closed_at NULL = abnormal-exit signal); audit OFFLINE_SESSION_ABORTED(Warning) | yes | offline_sessions.rs:146,255-271; abort_session offline_session.rs:160-175 (only test caller) |
| OPEN | Operator force-seam (service abort_session) — **NO prod auto-invoker** | allowed(Open,Aborted); reason_abort non-empty | ABORTED | UPDATE state=ABORTED, stamp reason_abort (closed_at NULL); audit OFFLINE_SESSION_ABORTED(Warning) | yes | offline_sessions.rs:148,255-271; offline_session.rs:160-175 |
| DRAINING | Operator force-seam (service abort_session) — **NO prod auto-invoker** | allowed(Draining,Aborted); reason_abort non-empty | ABORTED | UPDATE state=ABORTED, stamp reason_abort (closed_at NULL); audit OFFLINE_SESSION_ABORTED(Warning) | yes | offline_sessions.rs:150,255-271 |

**Note:** partial-drain failures (REJECTED/MANUAL cohort docs) do **not** abort the session — they escalate the FN to RMR and leave the session DRAINING (backlog_drain.rs:3311/3364 PARTIAL finalize). No prod path drives session ABORTED; all three ABORTED edges are operator/force-seam only.

### A.3 — `shift` machine (M3b 9-state, 15 edges — already in spec §11; reproduced for completeness)

| from | event | guard | to | effect | dormant | evidence |
|---|---|---|---|---|---|---|
| Created | ShiftOpen{online} | online open path | Opening | begin online SHIFT_OPEN | no | §16 spec / shift ladder |
| Created | ShiftOpen{offline} | offline open (BEGIN deferred) | OpenedLocalPendingDrain | local-pending open, drain deferred | no | §16 spec |
| Opening | wire ACK | online SHIFT_OPEN accepted | Opened | shift Opened | no | §16 spec |
| Opening | wire timeout (edge 4) | ambiguous wire timeout, cannot determine DPS accept | RMR | §16.7 family-2 escalation | no | §16.7 |
| OpenedLocalPendingDrain | Drain{finalize BEGIN} (edge 5) | drain confirms deferred BEGIN | Opened | pending-drain resolved | no | §16 spec |
| OpenedLocalPendingDrain | Drain{reject} (edge 6) | W9b drain reject of OFFLINE_LOCAL_ACK backlog | RMR | §16.7 family-1 universal EscalateManual | no | §16.7 |
| OpenedLocalPendingDrain | ShiftClose (still pending) | close requested before drain resolved | ClosingLocalPendingDrain | close ladder w/ pending drain | no | §16 spec |
| Opened | ShiftClose{online} | online Z path | Closing | begin online Z_REPORT | no | §16 spec |
| Opened | ShiftClose (offline/pending) | close w/ undrained backlog | ClosingLocalPendingDrain | close ladder w/ pending drain | no | §16 spec |
| Opened | fault escalation (edge 15) | generic §6.3 universal EscalateManual | RMR | operator handoff | no | §16.7 §6.3 |
| Closing | wire ACK | online Z accepted | Closed | shift Closed (terminal) | no | §16 spec |
| Closing | Z retry / re-open | Z re-drive keeps shift open | Opened | back to Opened | no | §16 spec |
| Closing | wire timeout (edge 12) | ambiguous Z wire timeout | RMR | §16.7 family-2 escalation | no | §16.7 |
| ClosingLocalPendingDrain | Drain{finalize} (edge 13) | drain completes, Z closes | Closed | shift Closed (terminal) | no | §16 spec |
| ClosingLocalPendingDrain | Drain{reject} (edge 14) | W9b drain reject of backlog | RMR | §16.7 family-1 EscalateManual | no | §16.7 |

**TODO-curation (shift):** the shift edge list supplied to this appendix carries `(from,to)` only; the events/guards/effects above are reconstructed from spec §16 / §16.7 semantics and should be reconciled against the shift-machine code whitelist and invoker inventory to reach the same file:line evidence rigor as A.1/A.2 before RED-pins freeze. The §16.7 SW-5a operator-force / senior-close seam is a **dormant** surface (not yet enumerated as a distinct edge above) and must be curated explicitly.

### A.4 — `node_mode` (projected) machine

No whitelist table exists; each of the 4 setters' SQL `WHERE` clause **is** the legality gate. The flat `mode` column conflates 5 axes (connectivity / legal-session / holds-budget / recovery-admin-block / crypto); the projection below splits BLOCKED by cause and flags the dormant axes.

| from | event | guard | to | effect | dormant | evidence |
|---|---|---|---|---|---|---|
| ONLINE | AdminCommand{go_offline} | reason non-empty; read_mode==ONLINE; SQL CAS WHERE mode='ONLINE' (rows==1 else fail-loud); MODE-ONLY (shift untouched, inv#3) | OFFLINE | set_mode_offline_tx + insert_opening + Opening→Open + OFFLINE_SESSION_OPENED + ADMIN_GO_OFFLINE(Critical), one envelope | no | node_state.rs:256-264; admin.rs:447-530 |
| OFFLINE | AdminCommand{go_online} | ensure_full_offline_surface_ready; reason non-empty; read_mode∈(OFFLINE,GOING_OFFLINE); SQL CAS same set (rows==1); MODE-ONLY | GOING_ONLINE | set_mode_going_online_tx + ADMIN_GO_ONLINE(Critical); finalize→ONLINE NOT emitted here (drain-driven) | no | node_state.rs:275-284; admin.rs:540-599 |
| GOING_OFFLINE | AdminCommand{go_online} | same go_online_inner; read_mode∈(OFFLINE,GOING_OFFLINE) accepts GOING_OFFLINE; CAS matches | GOING_ONLINE | set_mode_going_online_tx + ADMIN_GO_ONLINE(Critical) (identical envelope) | no | node_state.rs:277-278; admin.rs:563-569 |
| * | Send outcome DPS -11 (ERROR_OFFLINE_168) at Sending→Rejected CAS | WireDecision::Routed; node_mode_flip==Blocked; bare UPDATE (idempotent); rows==0 = MISSING-ROW structural breach (NodeStateMissingForBlock) | BLOCKED *(legal-cap)* | set_mode_blocked_tx atomic w/ Sending→Rejected + trace complete + STAGE_SEND_NODE_BLOCKED (W10.3, 168h cap) | no | node_state.rs:204-210; stage_send.rs:1844-1863 |
| * | BootRecover{ledger-survived, node_state-row-lost} (NC-03) | boot: ledger survived, node_state lost → reconstruct next_lnd + project seed, upsert_initial_tx(Online,Closed), flip; bare UPDATE, rows==0 → ensure! rollback | BLOCKED *(recovery-integrity)* | upsert_initial_tx + update_last_known_xml_sha_tx + set_mode_blocked_tx + BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED(Critical); no auto-trade until operator clears | no | node_state.rs:204-210; boot_phase.rs:1802-1834 |
| * | BootRecover{stale-ledger-tip} (PR-B tip-guard) | wire probe (outside tx) found lastChk divergence → block_on_stale_tip; bare UPDATE, rows==0 → ensure! rollback (no ghost-FN block) | BLOCKED *(recovery-integrity)* | set_mode_blocked_tx + TIP_GUARD_STALE_LEDGER(Critical); no doc-state transition | no | node_state.rs:204-210; boot_phase.rs:2530-2556 |
| * | Drain{W9,HoldFnDrain} accumulation (Tier-2) | FN accrues ≥50 consecutive HoldFnDrain on one doc (tier_threshold=50); bare idempotent UPDATE, rows==0=missing row structural breach | STOP_MODE | set_mode_stop_mode_tx + OFFLINE_DRAIN_FN_STOP_MODE(Critical); ingress rejected at adapter; probe SKIPS STOP_MODE; operator within 36h to resume | no | node_state.rs:235-241; backlog_drain.rs:2316-2346 |
| GOING_ONLINE | Drain converges (return-online probe + fully drained) | **NO setter** — finalize deliberately not emitted; "inert until A′.3 O2" | ONLINE | (intended drain-path convergence) — no guarded-CAS setter present today | yes | node_state.rs:270-272; admin.rs:552-553 |
| ONLINE | (none — no auto-invoker) | GOING_OFFLINE enum variant exists but NO setter writes it; only READ as go_online source | GOING_OFFLINE | (no setter mints it) | yes | enums.rs:83; grep: no writer targets it |
| * | (none — no auto-invoker) | CryptoDegraded enum variant declared; NO setter, NO invoker reads/writes in mode path | CRYPTO_DEGRADED | (no setter, no CAS) — crypto-axis placeholder | yes | enums.rs:88; grep: none of 4 setters |

**Idempotency contract split:** `set_mode_offline_tx`/`set_mode_going_online_tx` = GUARDED CAS ⇒ rows==0 is a real race (surfaced loud); `set_mode_blocked_tx`/`set_mode_stop_mode_tx` = BARE idempotent UPDATE ⇒ rows==0 redefined to "missing FN row / structural breach". **BLOCKED** deliberately projected as two distinct causes (legal-cap vs recovery-integrity) — LOCK-note: they share one enum value but must be treated as separate reasons by any consumer.

---

## Appendix B — Mutation → independent-invariant map

Scope pin (load-bearing): **none** of the catching invariants read the Spec #1 transition **whitelist** — a transition-table mutation cannot corrupt an oracle that never consults the table (`table_independent=true` for every row). Many checks read the durable **data** tables (fiscal_documents, node_state, ingress_inbox, offline_codes, shifts); reading the ledger is orthogonal to a transition-*table* mutation and is therefore not common-mode-unsafe for this family.

| mutation | example | caught-by (independent invariant) | tooth |
|---|---|---|---|
| add-edge | Re-admit removed `(Sent)→Rejected`: roll an issued (sfn-stamped, seed-advanced) doc into non-issued Rejected instead of RMR | MAC-seed monotonicity + online-issued sfn: invariant_scan check 4 ChainSeedMismatch/ChainBreak (:288-361) walks signed docs, requires terminal seed==node_state.last_known_unsigned_xml_sha256; check 3a' OnlineIssuedStateWithoutServerFiscalNo (:254-273); sfn⇔issued coupling. Compares durable seed projection vs ledger hashes, never the whitelist | invariant_fuzzer settled check_mirrors→invariant_scan (:2606-2610) fires ChainSeedMismatch; teeth_d7_online_advance_matches_prod_is_issued (:176-192) reddens on issued/seed drift |
| add-edge | Whitelist a second write-path for same FN in-flight (e.g. `(Opened)→Fiscalize` skipping inbox lease CAS) ⇒ concurrent 2nd issuer allocates same lnd | Per-FN lnd uniqueness: check 1 DuplicateLnd (:199-212) + DDL UNIQUE ux_fd_fn_lnd (migrations/025:181). DB-engine-enforced independent of app transition; re-detected at rest | invariant_scan DuplicateLnd via check_mirrors; U1 D1 allocator-prediction assert (:2497-2503, teeth_d1:204); rs3_fn_write_gate.rs proves per-FN serialisation |
| remove-edge | Delete `(Sending)→ErrorRetryable` crash-recovery downgrade ⇒ doc rests in SENDING at quiescence | "No doc rests in SENDING/pre-send at quiescence": check 2 StuckSending (:214-222) + check 2b StuckNonTerminalDoc {PREPARED,SIGNED,ENCRYPTED} (:224-241). At-rest ledger predicate from durability pin, fires regardless of table | settled assert_clean/check_mirrors (:2606-2610) raises StuckSending/StuckNonTerminalDoc; crash+reboot scan-gate pending_crash (:2560-2599) forces scan once settled |
| remove-edge | Remove pending-drain exit 5/13 ⇒ pending-drain shift w/ dead anchor has no path out | Orphaned-pending-drain: check 15 OrphanedPendingDrainShift (:473-504) — shift in *_LOCAL_PENDING_DRAIN whose anchor exists only terminal-non-issued can never fire exit. Structural shifts×fiscal_documents cross-join | check_mirrors→invariant_scan (:2608) surfaces OrphanedPendingDrainShift; exotic-drain FaultOrRecovery block (:2222-2232) bounds shift outcome to {unchanged, RMR, legit resolution} |
| weaken-guard | Offline SELL fires row-less w/ exhausted code pool / reserve floor, or double-consumes one code | Offline-code backing + no-double-consume: 6a OfflineCodeHalfConsumed (:384-396), 6b OfflineFiscalNoUnbacked (:398-412), 6c DuplicateOfflineFiscalNo (:414-429). Physical accounting over offline_codes ledger | ExpectedNoIssuanceRow branches assert consumed==model.codes_consumed + seed-freeze on refusal (:2388-2392,2454-2460); check_mirrors→OfflineFiscalNoUnbacked/DuplicateOfflineFiscalNo. Directed: t2_offline_close_reserve.rs, offline_session_code_pool.rs |
| weaken-guard | Replay guard weakened: terminally REJECTED/ERROR inbox row re-processed → ACCEPTED doc (AUD-1 lie) | Replay-consistency: check 5 RejectedInboxWithAcceptedDoc (:363-382) joins ingress_inbox×fiscal_documents, forbids REJECTED/ERROR inbox + ACK/OFFLINE_LOCAL_ACK doc for same request_id (status set mirrors replay.rs). Cross-ledger, never whitelist | check_mirrors→RejectedInboxWithAcceptedDoc (:2608); DuplicateIdemKey first-class Op (:722) drives replay seam; teeth_d1/d2 prove NoMutation replays adopt no state. Directed: repo_ingress_inbox_idempotency.rs |
| change-effect | ACK finalize reaches ACK without persisting server_fiscal_no or KVT1_RAW evidence | ACK-completeness: 3a AckWithoutServerFiscalNo (:243-252), 3b AckWithoutKvt1Raw (:275-286), 3a' (:254-273). Content-completeness of durable row, independent of producing transition | check_mirrors→AckWithoutServerFiscalNo/AckWithoutKvt1Raw (:2608); PredictableMutating check_differential (:2265) matches doc vs model mutation. Directed: repo_fiscal_documents_state_cas.rs, stage_finalize_idempotency.rs |
| change-effect | Offline doc written to drain-cohort state w/o offline_session_id, or node_state.shift_state not synced with shifts row | (a) check 6d OfflineOriginWithoutSession (:431-447) — cohort doc w/ NULL session invisible to drain (silent leak); (b) ShiftStateMirrorDrift (:449-471) requires node_state.shift_state==active shifts.state. Cross-row ledger predicates | check_mirrors→OfflineOriginWithoutSession/ShiftStateMirrorDrift (:2608); Mirror-2 cohort↔session join (oracle.rs:899-921); U1 D2 shift-state prediction (:2517-2522) |
| change-effect | Drain/crash-recovery re-SENDS an already-issued backlog doc (blind resend), or go-online END mint consumes code / advances seed twice | Bounded-wire + no-resend + code/lnd monotonicity postconditions (count physical wire calls + ledger deltas, NOT table-derived): resolving reboot must not increase send_chk (assert_no_resend, oracle.rs:835); RMR-FN drain re-tick = 0 new sends (AUD-K8-1, :2478-2486); exotic drain consumes 0 codes, ≤1 lnd, send-delta≤2·cohort+1 (:2175-2212) | FaultOrRecovery safety block (:2154-2261) + AUD-K8-1 wire-count (:2478-2486) + assert_no_resend (:2587-2595). Canaries: assert_crash_send_recovery / assert_probe_recovery_no_resend (oracle.rs:787-825), kill_point_matrix.rs |
| change-audit | Pre-acquire/invalid-ingress refusal mints a fiscal_documents row (or advances lnd/seed/consumes code) instead of audit-only + inbox terminalise | TrueNoMutation ledger-freeze: at audit-only refusal the ENTIRE ledger unchanged — no doc row, no lnd, no seed, no code. Asserted against durable counters, independent of any refusal table entry | ExpectedNoMutation branch (:2369-2392): observed_doc_count==before, next_lnd frozen, seed==prior_tip, consumed==before. Guards pre-acquire atomic lease+REJECT (inline.rs:576-583) |
| change-audit | DPS terminal reject leaves inbox terminal-failed (REJECTED/ERROR) while doc actually reached ACCEPTED (audit contradicts fiscal outcome) | Replay/audit-vs-ledger: check 5 RejectedInboxWithAcceptedDoc (:363-382), inbox-status set in lockstep w/ replay.rs. Cross-table equality, no whitelist consult | check_mirrors→RejectedInboxWithAcceptedDoc (:2608). Complementary: repo_audit_log.rs, mark_rejected_if_new.rs assert audit CAS atomic w/ state CAS |
| change-protocol-binding | Stored MAC hash (unsigned_xml_sha256) ≠ sha256(persisted PAYLOAD_XML), or offline doc renders bare `<MAC>` vs `<MAC ID=...>` mismatching hashed/chained bytes | Payload-hash referential integrity (O3): oracle.rs check_payload_hash_integrity (:724-750) recomputes sha256(PAYLOAD_XML)==stored hash. Crypto self-consistency, independent of table AND chain oracle | settled check_payload_hash_integrity (:2613-2615) panics on divergence. Chain-continuity: check_differential real prev_hash==prior tip (oracle.rs:172-177); check 4 ChainBreak. Directed: goldens_byte_equiv.rs, b9_stamp_at_sign.rs, b10_offline_session_handshake.rs |
| change-protocol-binding | Online-origin issued doc stamped via offline_fiscal_no path (or vice-versa) — mis-bind issuance lane so seed-advance predicate + sfn/offline-number coupling disagree with physical wire mode | Physical issued-lane↔identity coupling (independent ground truth, not model rule nor prod is_issued): sfn-at-SEND ⇔ crossed Sending→Sent; offline_fiscal_no ⇔ OFFLINE_LOCAL_ACK. check 3a' (:254-273) + 6b/6c enforce lane identity backing | teeth_d7 (:176-192) + teeth_d3_* (:138-161) redden on lane drift; check_mirrors→OnlineIssuedStateWithoutServerFiscalNo/OfflineFiscalNoUnbacked. Directed: b8_render_id_offline.rs, stage_send_offline_doc_routed_online.rs |

### Common-mode hazards & GAPs to close before RED-pins freeze

- **GAP-CM1 (shared-logic common-mode):** check 4's seed walk calls `fiscal_documents::is_issued` (invariant_scan.rs:345) — the *same* fn prod uses to advance node_state.seed. A change-effect mutation flipping `is_issued` corrupts BOTH writer and scanner expectation ⇒ ChainSeedMismatch tooth goes silent. **Mitigation already in-tree:** forked `MODEL_OFFLINE_ISSUED_STATES` + teeth_d3/d7 assert fork==prod const against independent sfn-coupling ground truth (invariant_fuzzer.rs:35,138-192). Keep the forked-set teeth as the independent anchor; treat `is_issued` as a monitored shared surface.
- **GAP-CM2 (manual-lockstep literals):** check 5 and check 6d hard-code state/status string sets "kept in lockstep" with prod (replay.rs short-circuit set; drain-cohort IN-set) (invariant_scan.rs:98-101,431-447). A state-rename mutation can drift prod without drifting the scanner literal (or vice-versa), blinding the check. **Action:** target these literal sets with a mutation-testing pass before freeze.
- **GAP-CM3 (canonical-truth partial-independence):** check_payload_hash_integrity (O3) proves stored_hash==sha256(stored PAYLOAD_XML) but CANNOT prove the stored PAYLOAD_XML is the correct canonical XML (canonicaliser seam is private). A change-protocol-binding mutation that consistently mis-renders AND mis-hashes the *same* wrong bytes passes O3. True independence deferred to out-of-process **WebCheck replay** (oracle.rs:721-723). This is the one in-repo place where "independent" is only partial — flag as required live-campaign coverage, not closeable in-repo.
- **GAP-TEETH (missing revert-canaries):** rows caught reactively at the settled boundary via invariant_scan rather than a dedicated pass-on-main/fail-on-revert canary — specifically **add-edge (check 4/ChainSeed reactive path), remove-edge check 2/2b, remove-edge check 15, change-audit check 5**. change-effect / protocol-binding / single-writer rows ARE empirically teeth-proven (teeth_d1/d2/d3/d7/o2 + reverted-guard canaries). **Action before RED-pins freeze:** add revert canaries for check 2/2b, check 5, and check 15 to complete the teeth ladder so every mutation row has a proactive (not merely reactive-at-quiescence) independent tooth.
- **Dormant-edge risk note:** dormant edges (whitelist/enum entry, no auto-invoker — `(Prepared,Rejected)`, `(Signed,Encrypted)`, `(Signed,ErrorRetryable)`, `(Kvt1,ErrorRetryable)`, `(Encrypted,Sending)`, `(OfflineLocalAck,Cancelled)`, all three session→ABORTED, node `GOING_ONLINE→ONLINE` / `→GOING_OFFLINE` / `→CRYPTO_DEGRADED`, shift §16.7 SW-5a operator-force) are the highest-risk add-edge/weaken-guard surface — no runtime auto-invoker exercises them. The independent net when an operator-force seam is later wired is `ShiftStateMirrorDrift` (forward-compat note invariant_scan.rs:129-135); directed guard `shifts_force_seam_source_guard.rs`. Recommend an explicit negatively-pinned test per dormant edge before it is ever activated.