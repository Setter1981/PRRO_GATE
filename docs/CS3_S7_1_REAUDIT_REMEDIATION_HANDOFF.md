# CS-3 S7-1 double-issue cutover — re-audit remediation HANDOFF

**Purpose.** Continue the remediation of the S7-1 cutover after a model-decorrelated re-audit
returned **NO-GO**. Five blockers are fixed + checkpointed; this document lets a fresh context
finish the rest without re-grounding. Read this top-to-bottom once, then work §4 in order.

Companion sources (authoritative detail):
- Memory: `project_cs3_s7_cutover_handoff.md` (compact, loads each session) + `MEMORY.md` index.
- Full re-audit findings: `/dev/shm/tmp/.../tasks/w3jfrrj3w.output` (internal 36-agent workflow result — may be GC'd; the consolidated list below is the durable copy).
- External audit brief: `docs/CS3_S7_1_CUTOVER_REAUDIT_EXTERNAL_BRIEF.md`.
- Design of record: `docs/CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` (+ the S7 impl/compose docs).

---

## 0. Where we are (2026-07-22)

- **Branch** `cs3-de-slice7-cutover` in **worktree** `/home/setter/prro-gate-wt/cs3-de-slice2`
  (main repo `/home/setter/prro_gate`). **origin** is at `e344077` (an old WIP) — **everything
  below is UNPUSHED / local**.
- **Commit stack** (base → tip):
  - `38bb8d6` — parent (docs).
  - `761a178` — the atomic cutover (composition core + HELD/RMR contract). **Known-unsound** — this
    is what the re-audit found NO-GO on. Kept as a base; the fixes stack on top.
  - `fd612f2` — CS-1 provenance re-baseline (`LIVE_DRIFT_BASE_SHA=761a178`) + source-inventory re-mint.
  - `eaca65b … 26b5ed0` — **9 verified fix checkpoints** (see §3; B1-resolve `8e753e6` + B5
    `a3dcd94` + F3-targeted `18770d7` + F2-kvt2 `26b5ed0` landed 2026-07-22 after the re-gate).
    Tree is GREEN modulo the expected `cs1_live_drift_base_vs_worktree` leg (§0 CS-1 note; cleared
    at the §5.2 re-baseline).
- **The double-issue core is sound** (both audit loops agreed: exactly-one-wire, per-FN fence +
  generation-CAS, B1-hash unified `wire_hash`, seed/SFN atomicity, HELD invariant-scan witness, R6
  redrive removal all hold). What was broken is the **failure-handling / recovery / resolution
  periphery** — that is what the fix-pass addresses.

---

## 1. Environment / commands / gotchas

```bash
export PATH="$HOME/.cargo/bin:$PATH"          # cargo lives here
cd /home/setter/prro-gate-wt/cs3-de-slice2/rust
# targeted tests (OOM caveat: run per-binary, low threads)
cargo nextest run -p prro --features test-support --test-threads 2 -E 'test(NAME)'
cargo nextest run -p prro --features test-support --test-threads 2 -E 'binary(BIN)'
# clippy — rtk PROXY to get raw file:line (rtk summarizes otherwise); -D promotes warnings
rtk proxy cargo clippy -p prro --features test-support --lib -- -D warnings
cargo fmt -p prro                              # write;  --check for gate
```

- **Every fix: strict TDD.** RED-first tooth → implement (change the transition/contract model, do
  NOT weaken an assertion) → GREEN → **revert-canary** (revert the fix, tooth MUST go RED). If a
  `git stash` canary breaks compilation (removes a referenced symbol), do a **surgical** canary
  instead: `sed` the guard/condition to a no-op, run, restore.
- **CS-1 frozen files.** CORRECTION (verified at re-gate 2026-07-22): the RED-first teeth DID touch
  two tracked test files — `b10_offline_session_handshake.rs` (B2 tooth + `drain_carriers_end_send_fails`
  helper, `7bbc5b3`) and `write_path_deterministic_replay.rs` (`fixture_6` rewrite+rename for F2-boot
  `52f562f`; `fixture_9` `+STOP` assertion + `read_node_mode` helper for B4 `4dd0ba1`). So **both** the
  live-drift leg (`cs1_live_drift_base_vs_worktree`) AND `source_files.sha256` (control-3) go RED
  mid-pass — NOT just control-3 as originally predicted. This is still **expected and benign**: the
  drift is exactly those additive/rewrite test artifacts (grep-verified, no smuggled behavior change),
  and the deferred **re-mint at the very end** (§5.2 re-points `LIVE_DRIFT_BASE_SHA` to the atomic
  commit, which then CONTAINS these test changes → leg goes green) clears it. Do NOT re-mint
  per-checkpoint. `cs1_test_provenance` is RED mid-pass — expected.
- Test-support exposure: `reservation_boot_pass::run` was made `pub` (from `pub(crate)`) for the
  B3 tooth. Fine to leave.
- Admin CLI dispatch lives in `rust/prro/src/main.rs` (`enum AdminCmd` ~:62, dispatch ~:492,
  `prro::admin::run_reset_stop_mode(&config, ...)` ~:496).

---

## 2. Re-audit verdict — consolidated blocker list (both decorrelated loops)

Builder = Opus 4.8. Two loops: an **external** reviewer (uniquely found F3) + an **internal**
36-agent workflow (uniquely found B1, B2 — both genuine bricks). Consolidated:

| Blocker | Sev | One-liner |
|---|---|---|
| **B1** | CRIT | HELD/STOP has NO wired resolution surface; `reset_stop_mode` blind-flip bricks completion + loses issuance. |
| **B2** | CRIT | 4th `stage_send` caller (drain session-END) missed by R6 → infinite spin, no RMR/STOP. |
| **B3 (=ext F1)** | CRIT/MAJ | boot `apply_one` bypassed `apply_orchestration` → shift edges 3/10 lost on crash-resume. |
| **F2** | CRIT | Sent+NotFound producers unwired from `sent_not_found_to_manual` → window with no atomic STOP. |
| **B4** | MAJ | online/boot ER→RMR never set STOP → persistent -13 = unbounded 202 stream, node never halts. |
| **B5** | MAJ | -11 Offline168 reject → node BLOCKED not STOP → completion CAS (`WHERE mode='STOP_MODE'`) unreachable. |
| **F3** | MAJ | `decision` built from legacy (`stage_send.rs:1721`), not TARGET evidence → trace/audit/return/drain diverge from durable state. |

Minor cluster: doc-drift, WrapperBug-held has no deterministic STOP pin, `assert_not_in_with_immediate`
is `debug_assert` only, `whitelist_4pre` guard asserts dead edges, SubmitRefused strands CALL_STARTED.

---

## 3. DONE — 8 verified checkpoints (each: RED-first tooth + revert-canary + regression + clippy/fmt)

| SHA | Blocker | Fix | Tooth |
|---|---|---|---|
| `eaca65b` | **B3** | `reservation_boot_pass::apply_one` → `apply_orchestration::apply_recorded_outcome` (fires edges 3/10 + closing-cash); static pin updated; run() → pub. | `ao04` (boot-pass SHIFT_OPEN OO+PENDING_APPLY → shift OPENED). 219/219. |
| `52f562f` | **F2 (boot)** | `dispatch_sent_via_probe` NotFound arm → new `cas_sent_not_found_to_manual_from_probe` (wires `sent_not_found_to_manual`: Sent→RMR+STOP, retry_class=None); histogram renamed; `boot_phase_raw_cas_edges` count 8→7. | `fixture_6` rewritten (was itself green-but-unsound — name said escalates_manual, body asserted Sent→ER two-tick) → one-tick RMR+STOP; renamed `..._escalates_manual_with_stop`. 144/144. |
| `7bbc5b3` | **B2** | `drain_session_end_doc` `_ => Ok(false)` → `escalate_drain_to_manual` (shift→RMR + Critical audit, FAIL-LOUD). | `b2_drain_session_end_send_fails_escalates_manual_not_spin` (full boot+offline+drain, `drain_carriers_end_send_fails`). 75/75. |
| `4dd0ba1` | **B4** | both ER→RMR helpers (`cas_error_retryable_to_manual_reconciliation` + `cas_error_retryable_budget_exhausted`) call shared `stop_node_for_escalated_doc` (SELECT fscl + `set_mode_stop_mode_tx`) atomically with the doc CAS. | `fixture_9` gained the STOP assertion its own comment claimed but never enforced. 30/30 + 108/108. |
| `98a356f` | **B1 (guard)** | `reset_stop_mode` refuses with new `AdminError::PendingResolutionRequired` (exit 64) when an OUTCOME_OBSERVED+PENDING_APPLY reservation exists. **Brick prevented.** | `b1_reset_stop_mode_refuses_while_pending_apply_unresolved`. Surgical canary. 58/58. |
| `8e753e6` | **B1 (resolve, part 2)** | `admin::resolve_operator_pending` + `run_resolve_operator_pending` + `AdminCmd::ResolveOperatorPending` wire the missing resolution surface → the zero-caller `complete_operator_pending` (one `with_immediate`); pre-tx read-only FN-ownership cross-check (typo guard); `CompletionError`→typed `AdminError` (`ResolutionRefused` 65 / `ReservationFnMismatch` 64 / `Db`→Infrastructure). `main.rs` `parse_hex_fixed`/`build_operator_resolution`. **Brick now CLEARABLE.** | `oc10` (accepted → doc SENT + APPLIED + pointer-clear + node ONLINE; canary: bogus reservation_id → refuse → RED) + `oc11` (wrong `--fiscal-number` refused, nothing mutated; canary: guard disabled → silently resolves the WRONG FN → RED). clippy `--all-features --all-targets` clean; operator_completion 20/20; admin 37/37; CLI `--help` smoke. |
| `a3dcd94` | **B5** | `complete_operator_pending` mode target is now computed PER held-halt mode: STOP_MODE → ONLINE/GOING_ONLINE (unchanged); **BLOCKED (offline `-11`, over ceiling) → STAYS BLOCKED** (completion clears the fence + resolves the doc but does NOT re-enable the over-ceiling node — INV-05). The final CAS reads `mode` with the generation+pointer authority and guards `WHERE mode = <cur_mode>`; a non-halt mode fails closed. New `ModeTarget::Blocked`. **Brick reachable + legal-safe.** | `b5_resolve_on_blocked_node_completes_and_stays_blocked` (offline held + BLOCKED → resolve completes, node stays BLOCKED; canary: BLOCKED→Online → node wrongly ONLINE → RED). `oc22` premise updated (BLOCKED now valid → induce 0-row CAS via non-halt ONLINE; assertions unchanged). nextest `--all-features --no-fail-fast` 2248/2248; both consumers (admin resolve + recon service) compose. |
| `18770d7` | **F3 (targeted)** | Post-wire projection (trace/audit/return/drain) now derives from the SAME evidence classifier as the durable record, not the legacy `route_send_result`. `build_record_args` also returns `ClassifiedOutcome`; new `project_decision_from_evidence(legacy, classified)` rebuilds the sole divergent leaf — RemoteAuthStatus (classify ProbeRequired vs legacy TransientRetry) — as the evidence-correct ProbeRequired routing (reused `StageSendProbeRequired` audit + new `ProbeReason::RemoteStatus`, distinct from OkButNoFiscalNumber). Legacy kept only for Sent SFN + wire-forensics + the drift-pin (untouched). **Full "slice E" deferred** → [[project_backlog_cs3_slice_e_full]]. | `f3_remote_status_projects_probe_required_from_evidence_not_legacy` (real classify(RemoteAuthStatus)=ProbeRequired + legacy=TransientRetry → projection ProbeRequired + trace + return all ProbeRequired; behavioral RED via stub; canary guard-disabled → RED). Tests can't mint a faithful RawSendObservation (`pub(in crate::transports::dps)`), so the tooth exercises the projection seam directly. clippy `--all-features --all-targets` clean; nextest 2249/2249 — behaviour-neutral (override fires only on the prod RemoteAuthStatus path). |
| `26b5ed0` | **F2-kvt2** | SentReplay NotFound arm → new `commit_sent_replay_envelope_1c_manual` (wires `sent_not_found_to_manual`: Sent→RMR + node STOP + Critical `SENT_NOT_FOUND_ESCALATED_MANUAL` audit + trace `retry_class=NULL`/`RetryableServer`) → new `ConfirmDrainOutcome::SentNotFoundEscalated`; SentReplay consumer → `DocVerdict::Failed{NotFound,manual_recon}` (drain escalates FN shift + HALTS); `_1c_post` retired; 3 non-SentReplay consumers fail-loud; design-R7 consumer `backup_restore` AUD-L4-1 (`tip_guard_inflight_sent_notfound_defers_to_drain`) reconciled to R5. | `c5b2` converted → `..._escalates_manual_with_stop` (doc RMR + node STOP + shift RMR + drain halted + SENT_NOT_FOUND audit + er_redrive_queued 0 + trace retry_class NULL). RED-first + surgical revert-canary RED. Fuzzer GREEN (`teeth_d5` `[Ack,NotFound]` = SentFresh→StructuralDrift, NOT cross-tick SentReplay). full `--all-features` 2249/2250 (only cs1-drift RED); clippy `--all-features --all-targets` clean. |

---

## 4. REMAINING — precise, ready to implement (do in this order)

### 4.1 B1-resolve (CRITICAL, part 2 of B1 — the resolution surface) — ✅ DONE (`8e753e6`)
**Shipped** as the checkpoint above. The `resolve-operator-pending` admin command now completes a
held PENDING_APPLY reservation (Accepted issues the doc, releases the node ONLINE/GOING_ONLINE),
with a FN-ownership cross-check. Deferred/not-in-first-cut (backlog): the OPTIONAL pre-tx DPS probe
the internal loop suggested (`complete_operator_pending` takes the operator's resolution directly).
Original scope notes retained below for reference.

The B1 guard now *refuses* `reset_stop_mode` and points the operator at `resolve-operator-pending`,
but **that command does not exist yet**. Without it operators have no path to clear a PENDING_APPLY.
- **Backend exists:** `delivery_reservation::complete_operator_pending(tx, reservation_id,
  resolution: OperatorResolution)` — `delivery_reservation.rs:1281`. `OperatorResolution` enum
  (`:1149`): `Accepted { fiscal_number }` / `NotAccepted` / `NotAcceptedOffline` / `MacReseed { seed }`.
  It has ZERO production callers (grep-confirmed).
- **Do:** add `pub async fn resolve_operator_pending(pool, fscl, reservation_id, resolution)` in
  `admin.rs` (open a `with_immediate` → `complete_operator_pending`; map `CompletionError` → a new
  `AdminError` variant + `exit_code()` arm). Add a `run_resolve_operator_pending(config, ...)`
  mirroring `run_reset_stop_mode`. Wire `AdminCmd::ResolveOperatorPending { fiscal_number,
  reservation_id, resolution }` in `main.rs` (~:62 enum, ~:492 dispatch). The internal suggested an
  OPTIONAL pre-tx DPS probe, but `complete_operator_pending` takes the operator's resolution
  directly — a probe is not required for a first cut.
- **Tooth:** reuse `operator_completion.rs` (`held_pending` + `complete` helpers; oc01-oc09 already
  exercise `complete_operator_pending`). Add a test driving the ADMIN fn end-to-end: held_pending →
  `resolve_operator_pending(Accepted{...})` → doc RMR/SENT per resolution, reservation APPLIED,
  pointer cleared, node released. Canary: without the wiring the pending stays.

### 4.2 B5 (MAJOR, entangled with B1 — fix together + matrix) — ✅ DONE (`a3dcd94`)
**Shipped.** ⚠ NUANCE RESOLVED: a completed **BLOCKED** hold **STAYS BLOCKED** (chosen over
GOING_ONLINE). Grounding: an offline-origin `-11` rests held (`apply_outcome:928` returns
HeldNotAutoRelease for `!online`); `record_outcome:721` sets BLOCKED early. BLOCKED = over the 168h
ceiling; there is NO auto-unblock transition in `node_state.rs` ("until an operator clears the
block", `boot_phase.rs:1754`) — so re-enabling on doc-resolution would (a) conflate doc-resolution
with ceiling-recovery and (b) risk an INV-05 breach (over-ceiling FN issuing offline again).
Approach: NOT the handoff's blanket `mode IN ('STOP_MODE','BLOCKED')` with the old ONLINE target
(that would wrongly re-enable); instead a PER-mode target (STOP_MODE→ONLINE/GOING_ONLINE,
BLOCKED→BLOCKED) with a `WHERE mode=<cur_mode>` CAS. Un-blocking remains a separate concern
(backlog: no operator "clear-block" command exists yet — same gap class B1-resolve just closed).
Original scope notes retained below for reference.

- **Sites:** the -11 BLOCKED branch sets `set_mode_blocked_tx` at `delivery_reservation.rs:721-726`
  (guarded by `offline_reject_hold` excluding `NodeBlocked` at `:708-709`). Completion CAS is
  `UPDATE node_state SET mode=? WHERE fiscal_number=? AND mode='STOP_MODE'` at
  `delivery_reservation.rs:1471` → returns `NodeNotStopMode` (`:1484`) for a BLOCKED node.
  `invariant_scan.rs:237` exemption already admits `BLOCKED`.
- **Do (approach B, cleaner than dual-mode):** extend the completion mode-CAS to
  `mode IN ('STOP_MODE','BLOCKED')`. **⚠ SEMANTIC NUANCE — verify before shipping:** what mode does
  a completed **-11** (over the offline ceiling) land in? Read `complete_operator_pending`'s
  `mode_target` computation (`delivery_reservation.rs:~1345-1470`, `ModeTarget` per resolution). A
  -11 doc is OFFLINE-origin → `OperatorResolution::NotAcceptedOffline`. Completing it must NOT
  wrongly re-enable an over-ceiling node — it should return to the appropriate halt (likely stay
  BLOCKED, or GoingOnline only if the ceiling cleared). Do NOT rush this; it's why B5 was deferred.
- **Matrix tooth:** `{online-STOP, offline-BLOCKED} × {resolve-command exists, reset-stop-mode-first}`
  — a partial fix that only wires the STOP_MODE case re-opens the brick from the BLOCKED branch.

### 4.3 F3 (MAJOR — TARGET is not the sole post-wire authority) — ✅ DONE (targeted, `18770d7`)
**Shipped as a targeted fix** (user scope decision): the projection now derives from the evidence
classifier for the sole divergent leaf (RemoteAuthStatus → ProbeRequired). The FULL "slice E"
(single evidence source-of-truth for ALL post-wire projections, legacy → diagnostics-only, exhaustive
match) is DEFERRED as its own slice on a green base after this remediation — [[project_backlog_cs3_slice_e_full]].
Grounding note: tests cannot mint a faithful `RawSendObservation` (RemoteAuthStatus constructor is
`pub(in crate::transports::dps)` by design), so no stub-driven test reaches the divergent path — the
fix is behaviour-neutral for the whole existing suite; the tooth exercises the projection seam directly
against a real `classify`. Delta-1 (OkButNoFiscalNumber) was already consistent (`target_wire_decision`);
Delta-2 (UnknownStatus `outcome_kind` label) is retry-class-consistent already, left as-is. Original
scope notes retained below for reference.


- **Site:** `let decision = target_wire_decision(route_send_result(legacy, doc_type, true));` at
  `stage_send.rs:1721` — built from **legacy**, while the durable evidence is built from
  `obs.evidence()` at `stage_send.rs:831`. `target_wire_decision` (`error_routing.rs:318`) only
  rewrites the empty-SFN case; everything else passes through. The legacy `decision` then drives
  trace+audit (`stage_send.rs:943`), the public `StageSendOutcome` (`:1763`), and drain
  failure-class (`backlog_drain.rs:1410`).
- **Symptom (drift-test proves 3 deltas):** e.g. TLS RemoteStatus → durable TARGET writes
  `ProbeRequired` but trace/return reports `TransientRetry`. **The durable state is CORRECT** — F3 is
  a consistency/observability gap (hence MAJOR, not CRITICAL).
- **Do:** project trace / audit / returned outcome / drain failure-class from the already-built
  authoritative `EvidenceDiscriminant` / `ClassifiedOutcome` (what feeds `record_outcome`), NOT the
  legacy `decision`. Keep `legacy` ONLY as wire-diagnostics + the drift-pin input.
- **Tooth:** a UnknownStatus/TLS case where durable state == trace/return retry-class (they must
  agree). Watch the existing drift-pin (it deliberately compares legacy vs target — keep it).

### 4.4 F2-kvt2 (offline-drain variant of F2 — deeper) — ✅ DONE (`26b5ed0`)
**Shipped** as the 9th checkpoint (design R5 — "Sent+NotFound producer #2"). The recipe below
was implemented verbatim: new `commit_sent_replay_envelope_1c_manual` (wires `sent_not_found_to_
manual` → Sent→RMR + node STOP + Critical audit + trace `retry_class=NULL`/`RetryableServer`),
arm returns new `ConfirmDrainOutcome::SentNotFoundEscalated`, SentReplay consumer maps →
`DocVerdict::Failed { NotFound, manual_recon }` (drain escalates FN shift + HALTS), `_1c_post`
retired. **Consumer-completeness beyond the recipe:** the shared enum forced arms at 4 exhaustive
match sites — the 3 non-SentReplay ones (`process_via_stage_send` SentFresh, `process_via_w12_only`
+ `online_convergence` Kvt1Reentry) **fail loud** (SentReplay-exclusive); the 5th site
(`drain_session_end_doc`) uses `matches!`+catch-all so the new variant falls into its escalate arm
(no edit). **Design-R7 consumer the recipe MISSED:** `backup_restore.rs`
`tip_guard_inflight_sent_notfound_defers_to_drain` (AUD-L4-1) presumed the retired safe-redrive
Sent→ER edge — reconciled to R5 (Phase-2 now asserts RMR+STOP+audit; Phase-1 tip-guard DEFER
unchanged; the AUD-L4-1 *permanent-silent* wedge is still avoided — recovery is now operator-gated
via `resolve-operator-pending`, double-issue-safe per the P2>liveness pin). **Fuzzer GREEN** (the
`teeth_d5` `[Ack,NotFound]` script is the SentFresh→StructuralDrift held-at-SENT path, NOT the
cross-tick SentReplay drain; alphabet does not construct the latter — model/fuzzer/live_smoke
comments corrected from the mislabel "SentNotFoundDowngrade"). RED-first c5b2 + surgical
revert-canary + full `--all-features` 2249/2250 (only expected cs1-drift RED) + clippy
`--all-features --all-targets` clean. Original recipe retained below for reference.

**Files pathed to `services/offline_sync/` (NOT `reconciliation/`).** Verified sites (2026-07-22):
- **Producer arm:** `kvt2_confirm.rs:984-1048` (`Kvt2ConfirmOutcome::SentNotFoundDowngrade`, classify at
  `:334`) calls `commit_sent_replay_envelope_1c_post` (`:1651`, Sent→ErrorRetryable + trace.complete
  TransientRetry + `OFFLINE_DRAIN_DOC_FAILED` audit + resets ErRedriveQueued) → returns
  `ConfirmDrainOutcome::HoldFnDrain { projection: ErRedriveQueued, class: NotFound }`. R6 retired the
  ER redrive → this now WEDGES the drain (`DocsErRedriveQueued` blocks finalize `backlog_drain.rs:354`)
  instead of the atomic RMR+STOP the double-issue design needs.
- **Boot-mirror (template = `52f562f`):** reuse `reconciliation/sent_not_found.rs:67
  sent_not_found_to_manual` UNCHANGED (Sent→RMR CAS + node `mode=STOP_MODE` + Critical
  `SENT_NOT_FOUND_ESCALATED_MANUAL` audit; the PRODUCER owns the recovery-trace completion — the CHECK
  needs `wire_call_started/finished + outcome_kind` together, `sent_not_found.rs:94-99`).

**Recipe (grounded; the arch-planner's node-column clobber fear is UNFOUNDED — `escalate_drain_to_manual`
writes `node_state.shift_state`, `sent_not_found_to_manual` writes `node_state.mode`, different columns):**
1. **New `commit_sent_replay_envelope_1c_manual`** (kvt2_confirm) — sibling of `1c_post`, ONE
   `with_immediate`: (a) `sent_not_found_to_manual(tx, doc_id, fiscal_number)` (map `DocNotSent(_)`→
   benign, else propagate); (b) `transport_trace::complete_tx(tx, doc_id, trace_attempt_no, AttemptCompletion{..})`
   mirroring `1c_post`'s trace args EXCEPT `retry_class: None` (R6 — no re-drive) + `outcome_kind:
   RetryableServer` + reworded error_message; assert 1 row. NO `OFFLINE_DRAIN_DOC_FAILED` audit (the
   Critical audit is inside sent_not_found_to_manual). **Delete `commit_sent_replay_envelope_1c_post`**
   (sole caller is the arm).
2. **Dispatch arm** (`:984-1048`): keep the non-SentReplay fail-loud guard + the trace_attempt/wire
   extraction; call `..._1c_manual`; return **new `ConfirmDrainOutcome::SentNotFoundEscalated`** (add
   the variant; do NOT reuse `SupersededHeld` — its consumer assumes doc stays SENT + its audit/class
   say "superseded", wrong here).
3. **Consumer** (`backlog_drain.rs:1895` region): `SentNotFoundEscalated => Ok(DocVerdict::Failed {
   class: FailureClass::NotFound, manual_recon: true })` — REUSE `DocVerdict::Failed` (no new DocVerdict).
   The loop arm `DocVerdict::Failed { manual_recon: true }` (`backlog_drain.rs:1027-1042`) already does
   `escalate_drain_to_manual` (shift→RMR) + `return Ok(summary)` = **HALTS the drain** (verified). Result:
   doc RMR + node mode STOP (commit) + shift RMR (escalate) + drain halted. ⚠ Failed's own consumer may
   emit an `OFFLINE_DRAIN_DOC_FAILED` audit — verify at impl; the c5b2 asserts on the audit, adjust.
4. **ErRedriveQueued machinery = KEEP-AS-PERMANENTLY-ZERO** (minimal diff): retire only the PRODUCER
   (the old arm). The field/`record_doc_er_redrive_queued`/`DocsErRedriveQueued`/finalize-block become
   dead (never produced → stays 0); existing `er_redrive_queued()==0` asserts (many test files) stay
   green. Full removal (touches app.rs + JSON schema + ~10 test files) = separate follow-up. The only
   producer is `kvt2_confirm.rs:1044` (grep-confirmed; `inline.rs:107` fail-louds the online path).
5. **RED-first tooth = convert c5b2** (`backlog_drain_state_dispatch.rs:1687`
   `c5b2_sent_replay_lastchk_not_found_holds_fn_drain_er_redrive_queued` → `..._escalates_manual_with_stop`):
   replace `doc==ERROR_RETRYABLE` + `er_redrive_queued()==1` + the OFFLINE_DRAIN_DOC_FAILED-transport
   asserts with: doc→`REQUIRES_MANUAL_RECONCILIATION`, node `mode==STOP_MODE`, shift→RMR (drain halted),
   `SENT_NOT_FOUND_ESCALATED_MANUAL` Critical audit==1, `er_redrive_queued()==0`, trace row retry_class
   NULL, 0 send_chk / 1 last_chk. RED against current Sent→ER code → flip arm+commit → GREEN → canary
   (revert arm to `1c_post`) RED.
6. **Fuzzer model** (`invariant_fuzzer/model.rs:1484` — oracle says SentNotFoundDowngrade → doc held at
   SENT / shift unchanged): now WRONG. IF the full `--all-features` suite RED's the fuzzer differential
   (i.e. the alphabet drives a SentReplay-NotFound drain), update the oracle to doc→RMR + node STOP +
   drain escalates. If green, it's a comment-only follow-up. Also touch `invariant_fuzzer.rs:321`,
   `live_smoke_w12_hardening.rs:222` (comment-only wording).

**Invariants:** #1 (trace complete_tx = pure DB, probe already done pre-tx), #2 (single-writer under
lease), #8 (Sent→RMR whitelisted edge + node STOP atomic) all hold. No CAS race (single-writer + the
`TransitionOutcome::Applied` guard + `DocNotSent` benign path).

### 4.5 minor cluster (land opportunistically; not merge-blockers) — PARTIAL (session 4, 2026-07-22)
- **`assert_not_in_with_immediate` release-mode guard — ✅ DONE (`db/tx.rs`).** Now computes `inside`
  once, keeps the debug `debug_assert!`-panic, AND adds a release-mode CRITICAL `tracing::error!`
  (INVARIANT #1 breach: crypto/transport inside a `BEGIN IMMEDIATE`). Log-not-panic (a panic
  mid-tx could crash the gateway); NO `audit_log` row — the guard has no pool/tx by design.
- **doc-drift — ✅ PARTIAL.** Fixed the 3 stale F2-boot NotFound comments that survived `52f562f`,
  all describing the retired "Sent→ErrorRetryable two-tick redrive": `app.rs` SENT-dispatch summary,
  `boot_phase.rs` `sent_not_found_to_manual` field doc + the `dispatch_sent_via_probe` NotFound-arm
  doc — corrected to RMR+STOP. STILL OPEN: `backlog_drain.rs` ER-redrive comments + the
  `TickSummary::er_redriven` dead-always-zero counter — the `er_redrive_queued`/`ErRedriveQueued`
  machinery is permanently-zero-but-alive (F2-kvt2 keep-as-zero); a full field removal is its own
  follow-up (touches app.rs + JSON schema + ~10 test files).
- **WrapperBug held STOP pin (`rc09`) — ⏸ DEFERRED (needs re-grounding).** Two WrapperBug notions:
  the sealed classifier emits `WrapperBug` ONLY from `NotStarted{PreflightRefusal::SigningFailed}`
  (certainty `NotSubmitted` → **NOT recordable** via `record_outcome`, which is post-`CALL_STARTED`,
  `AuthorizedGeneration::Started`); the fuzzer model separately maps a transport `ServerFiscalIdMismatch
  → WrapperBug` (`model.rs:1468`). A deterministic `record_outcome.rs` tooth needs the recordable
  (Started-generation) WrapperBug producer identified first — else the "pin" is green-but-unsound.
  The `delivery_reservation.rs:734` `NodeEffect::WrapperBug` STOP arm IS runtime-covered (fuzzer
  catches its removal). Re-ground the recordable producer at the §5.3 re-audit.
- **`whitelist_4pre_source_states_regression_guard` message rewrite — ⏸ DEFERRED (premise uncertain).**
  The claim that `ErrorRetryable→Sending` is a "dead edge" is NOT confirmed: `stage_send.rs:1570`
  still documents the 4-pre source-state CAS as accepting `{Signed, ErrorRetryable, OfflineLocalAck}
  → Sending`. Rewriting the message to "dead" without confirming R2's actual application to the live
  4-pre source set would introduce a NEW false comment. Re-ground the current 4-pre source set + R2
  status at §5.3 before touching.
- **SubmitRefused STOP hardening — ⏸ DEFERRED to CS-6.** The arm (`stage_send.rs` `run_one_attempt`
  wire phase) is unreachable-by-construction TODAY (fixed `production_dps_binding()` + the SAME
  envelope moved to the wire), so a STOP+audit added now is untested state-mutating defensive code
  in a write-path hot zone. It becomes reachable + RED-first-testable at CS-6 per-FN binding — do it
  there, test-driven, rather than blind now.

---

## 5. After the fixes: re-gate → atomic-rebuild → narrow re-audit

### 5.1 Full gate (per `feedback_pre_push_ci_gate_checklist`)
1. `cargo fmt -p prro --check` → 0 diffs.
2. `rtk proxy cargo clippy -p prro --all-features --all-targets -- -D warnings` → clean (use
   `--keep-going` to see all at once; rtk summarizes, so grep raw for `file:line`).
3. **Inventory re-mint** (tests changed a lot: `ao04`, `b1_*`, `b2_*`, `fixture_6` rename, plus
   whatever the remaining fixes add): `CARGO=$HOME/.cargo/bin/cargo bash scripts/cs1r/mint_manifests.sh`
   then `CARGO=... bash scripts/cs1r/inventory_gate.sh` (control 1 live==committed + control 3 source
   no-drift). Note control-2 (`--pr`) additions-only WILL flag the adjudicated test removals/renames —
   that's the merge-time architecture-decision, same as the original cutover.
4. `cargo nextest run -p prro --all-features --test-threads 2` → all green (expect ~2243+).

### 5.2 Atomic-rebuild (fold 761a178 + all fixes into ONE sound atomic cutover)
The 5+ WIP checkpoints must be squashed into a single atomic cutover commit, then CS-1 re-baselined.
**Circular dependency:** `LIVE_DRIFT_BASE_SHA` in `cs1_provenance.rs` must point at the atomic
commit, so the re-baseline is a SEPARATE follow-up commit (can't be amended in). Procedure (proven
twice already this cutover):
1. `git reset --soft 38bb8d6` (un-commit `761a178` + `fd612f2` + all fixes; everything staged).
2. `git checkout 38bb8d6 -- rust/prro/tests/support/cs1_provenance.rs
   rust/prro/tests/cs1_test_provenance.rs docs/cs1r/pins/post_cs1_carveout.tsv` (revert CS-1 files —
   the atomic must NOT contain the re-baseline).
3. `cargo fmt` → clean; `clippy -D` → clean (fix any residue IN the working tree BEFORE committing —
   any post-commit edit to a frozen file re-triggers this whole dance).
4. Commit the atomic cutover (message: composition core + HELD/RMR + all re-audit fixes). Capture
   **NEW_SHA**.
5. Re-apply the CS-1 re-baseline: set `LIVE_DRIFT_BASE_SHA = <NEW_SHA>` in `cs1_provenance.rs`
   (keep BASE_SHA=`f2c17b1`, HEAD_SHA=`f2628ba` UNTOUCHED — the immutable neutrality proof); re-point
   the live-drift leg; empty `post_cs1_carveout.tsv`; **re-mint `source_files.sha256`** (it hashes
   `cs1_provenance.rs`/`cs1_test_provenance.rs`, so it belongs in THIS commit, not the atomic). Commit.
6. Verify `cs1_test_provenance`: immutable leg `f2c17b1..f2628ba` still 78/1; live-drift leg
   re-anchored to NEW_SHA, 79/0. Full gate green.

### 5.3 Narrow re-audit (both loops agreed: no full round)
Attack surfaces only: **crash-shift** (B3), the **two NotFound producers** (F2 boot + kvt2), the
**3 drift-delta** (F3), and — critically — the **admin surface** (`admin.rs` was NOT in the original
audited diff yet is load-bearing for B1/B5; a dedicated admin-surface lens is warranted to confirm no
OTHER admin command races the PENDING_APPLY fence). Re-run the external brief + an internal workflow
scoped to these.

### 5.4 Merge (user drives — `feedback_dual_session_role_split`)
Squash-push needs a **force-push** over the origin WIP (`e344077`) — forbidden-by-default, confirm
with the user. Then retarget top-PR → main + squash-merge (stack-landing per
`feedback_cut_review_spiral_land_reversible`: land reversible, no round-3).

---

## 6. Load-bearing lessons (why this happened / how to not repeat)
- **green-but-unsound** (`feedback_green_but_unsound_checkpoint`): the original cutover was gate-green
  but two foundation helpers (`apply_recorded_outcome`, `sent_not_found_to_manual`) were BUILT and
  never WIRED; static pins encoded the WRONG target; several tests' NAMES claimed behavior their
  BODIES didn't enforce (fixture_6, fixture_9's "+STOP"). Grep for "unwired helper" + "comment claims
  X, code does Y" as a class.
- **model-decorrelation is mandatory for a cutover** (`feedback_working_with_stronger_auditor`): the
  internal loop found 2 CRITICALs the external missed; the external found F3 the internal rated low.
  Neither alone was sufficient. Run BOTH; consolidate.
- **grounding-gate** (`feedback_grounding_gate_before_claims`): every finding here was re-verified by
  grep/read against the post-cutover tree before acting — do the same.
