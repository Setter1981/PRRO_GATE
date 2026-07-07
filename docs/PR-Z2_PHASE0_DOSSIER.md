# PR-Z2 (live-Z dispatch) — Phase-0 Design Dossier

Base: `feat/aprime-z2` @ `e1a7817` (post RETURN+ main). Read-only recon (4 lenses).
**No code written — this is the design-before-code deliverable for architect adjudication.**

---

## Verdict (headline)

**PR-Z2 is an ORCHESTRATOR-WIRING increment, not a stage build.** Every write-path
STAGE already handles Z / SHIFT_OPEN / SHIFT_CLOSE. The *only* gap is that the
orchestrator `inline::run` FAIL-CLOSES those doc-types instead of routing them
through the stages that already exist. So PR-Z2 = "route Z/shift through the
existing stages + flip the gate + wire the residual escalations", NOT "build Z
signing/sending".

This reframes the effort down from the memory's "целый write-path инкремент" — but
it surfaces **4 scope/policy STOPs** the architect must adjudicate before code.

---

## What is already built (verified, recon B)

Every stage handles Z/shift; nothing to build here:

| Capability | Where | Status |
|---|---|---|
| Shift guard matrix (Z@Opened=OK, backlog-drain-pending/closing/closed refused) | `stage_acquire.rs:1014-1097` | READY |
| Edge 1/8 (Created→Opening online-open; Opened→Closing online-Z/close) | `stage_acquire.rs:706-760` | READY (A'.1 piece 3) |
| Z-number allocation (3-PRE, atomic w/ pin) | `stage_sign.rs:322-327` | READY |
| Z payload parse (`ZReportJson` incl. TXS `tax_summaries`) | `stage_sign.rs:963-999` | READY (PR-Z1) |
| Z canonical build + ts_prefix from header | `stage_sign.rs:1079-1129` | READY |
| CheckType map (ZReport→ZREPORT, ShiftOpen→ServiceChk) | `stage_send.rs:355-361` | READY |
| Envelope build (local_number=0 for SHIFT_OPEN) | `stage_send.rs:394-444` | READY |
| Edge 3/10 confirm (Opening→Opened; Closing→Closed on Ack) | `stage_send.rs:1591-1629` | READY (A'.1 piece 2) |
| Signer-cashier guard BYPASS for Z/shift (§16.9 senior-cashier seam) | `signer_guard.rs:194-199` | READY |
| Quiescence C10 gate (`quiesce_shift_before_z`: finalize leading KVT2 run, else Pending) | `z_builder.rs:102-145` | READY |
| Z aggregation + D5 dual-hash (`aggregate_z_payload_for_shift`, `build_z_canonical`) | `convert.rs:857+`, `z_builder.rs:159-225` | READY (PR-Z1) |
| Advance-at-SEND for Z (Sending→Sent CAS stamps sfn + advances seed — no Z exemption) | `stage_send.rs:1603-1629` | READY |
| D5 sibling gate applies to Z (online-origin + non-issued sibling → D5GateBlocked/503) | `stage_sign.rs:266-285` | READY |

## The gap (verified, recon A)

`inline::run` (`inline.rs:495`) is the single production orchestrator (`InlineWritePath`
binding at `supervisor.rs:227`). It fail-closes:

- **Z_REPORT / SHIFT_CLOSE** (`inline.rs:511-540`): `ensure_full_z_surface_ready()` →
  `Err` (flag false) → 501 `Z_SURFACE_NOT_READY` + a `debug_assert!(surface.is_err())`
  tripwire + `terminalise_inbox_pre_acquire` (REJECTED, no ledger row).
- **SHIFT_OPEN** (`inline.rs:541-556`): unconditional 422 `SHIFT_OPEN_NOT_SUPPORTED`.
- **SELL/RETURN** (`inline.rs:560-922`): the LIVE happy-path template
  (build_canonical → acquire → sign → dispatch → send → confirm → advance → finalize).
  **This template is directly reusable for Z/shift** — the stages are doc-type-agnostic.

**Flag:** `FULL_Z_SURFACE_READY = false` (`z_builder.rs:48`, no config knob — D6). Sole
consumers: `inline.rs:519` + tripwire test `z_live_dispatch_is_gated_until_full_z_surface`.
A naive flip (flag→true, no routing) fires the debug_assert (debug) / falls through to
`build_canonical`→`build_z_canonical` with no aggregated payload → BuildReject (release).
So the flip MUST be lock-step with the routing.

**A'.1 wired the online shift EDGES in the STAGES, but the ORCHESTRATOR never routes
SHIFT_OPEN/Z to them** — A'.1's "first e2e open→sell" drove the stages directly, not the
live `inline::run` binding. So the live open/close path is genuinely unwired.

---

## The 15-edge landscape (recon C) — A'.1 wired vs PR-Z2 residual

`shifts.rs:73-93` whitelists 15 edges. **A'.1 wired ONLY the online edges 1/3/8/10.**

| Edge | Transition | Trigger | Status | Natural home |
|---|---|---|---|---|
| 1 | Created→Opening | online SHIFT_OPEN | WIRED | — |
| 3 | Opening→Opened | online-open Ack | WIRED | — |
| 8 | Opened→Closing | online Z/SHIFT_CLOSE | WIRED | — |
| 10 | Closing→Closed | online-close Ack | WIRED | — |
| **4** | Opening→**RMR** | online-open hard-reject / ambiguous timeout | UNWIRED | **PR-Z2** (online correctness) |
| **12** | Closing→**RMR** | online-Z hard-reject / ambiguous timeout | UNWIRED | **PR-Z2** (online correctness) |
| 11 | Closing→Opened | DocumentReject only (operator reissue) | UNWIRED | PR-Z2? (online-close reject rollback) |
| 2 | Created→OpenedLocalPendingDrain | offline SHIFT_OPEN | UNWIRED | A'.3 offline-enable |
| 5/6 | OLPD→Opened / →RMR | drain open success / reject | UNWIRED | A'.3 offline-enable |
| 7/9 | →ClosingLocalPendingDrain | offline Z ingress | UNWIRED | A'.3 offline-enable |
| 13/14 | CLPD→Closed / →RMR | drain close success / reject | UNWIRED | A'.3 offline-enable |
| 15 | Opened→RMR | strict-seq drain reject (M2-N2a) | UNWIRED | A'.3 offline-enable |

The manual-recon escalation seam `escalate_fn_to_manual_recon` EXISTS
(`backlog_drain.rs:2399-2506`: CAS→RMR + node_state mirror + §8.1 forensic audit) but is
only called from drain / convergence / boot — **NOT from an online stage_send hard-reject
for edges 4/12.**

---

## Proposed scope — and the STOPs for adjudication

### STOP-S1 — PR-Z2 = ONLINE-only live Z/shift dispatch (offline → A'.3)?

**Recommendation: YES.** Scope PR-Z2 to the ONLINE lane only:
- Route SHIFT_OPEN + online Z_REPORT/SHIFT_CLOSE through `inline::run` (edges 1/3/8/10 —
  stages ready; orchestrator wiring + flag flip).
- Wire edges 4/12 (online hard-reject / ambiguous-timeout → RMR) in the stage_send
  reject branch.
- Wire edge 11 (Closing→Opened on a pure DocumentReject → operator reissue) IF the online
  reject taxonomy needs it (see STOP-S3).
- **Defer the 8 OFFLINE edges (2/5/6/7/9/13/14/15) to A'.3 offline-enable** — they need the
  offline lane + backlog-drain finalization, a separate large increment already on the roadmap.

This gives a shippable **PILOT-GATE online e2e: boot → online SHIFT_OPEN → SELL → online
Z-close**, without dragging the offline drain machine into this PR. **Adjudicate: is
online-only the right cut, or must PR-Z2 also carry offline?**

### STOP-S2 — Does flipping `FULL_Z_SURFACE_READY` require IO/EPZ, or is TXS + legitimately-absent IO/EPZ the "full surface"?

The flag's rationale (`z_builder.rs:33-48`) says false "until W4-Z2 completes the TXS / IO /
EPZ surface." **PR-Z1 (#232) already landed TXS aggregation.** Per the PR-Z1 finding, **IO
(`service_sums`) and EPZ are legitimately ALWAYS-EMPTY** for this gateway (SERVICE_IN/OUT are
classified `Unsupported` at ingress → never minted; acquirer-slip fail-closes before the
envelope). So the "full surface" for this contour = **TXS (done) + intentionally-absent
IO/EPZ**. `stage_sign` still hardcodes `service_sums`/`epz` empty (`stage_sign.rs:1122-1127`),
which is CORRECT if they're legitimately absent.

**Adjudicate:** is the surface "complete" (flip the flag now, IO/EPZ absent-by-design), or
does the pilot require IO/EPZ before a live Z is honest? This is the load-bearing gate — the
whole PR is blocked on it. (My read: TXS + absent-IO/EPZ is complete for the pilot scope, but
this is your fiscal-correctness call, not mine.)

### STOP-S3 — Edges 4/12 recovery taxonomy + operator alert scope

Edges 4/12 (online SHIFT_OPEN/Z hard-reject) must evaluate `ShiftOpenRecoveryClass` /
`ShiftCloseRecoveryClass` (spec §6.2/6.4) BEFORE escalating: Transport-timeout-exhausted /
Server -1/-5/-7/-8/-9/-10/-11 / FiscalNumberNotRegistered → `EscalateManual` (edge 4/12);
a pure `Authorization::DocumentReject` → reissue (SHIFT_OPEN stays `Opening`; SHIFT_CLOSE
rolls back `Closing→Opened`, edge 11). Retry budgets per §6.5 (Transport 20/≥30min; hard
3/≥5min; hard-fail-immediately for FiscalNumberNotRegistered / -11 / Decode / Internal).

Every Manual landing must emit the §8.1 forensic-snapshot audit (already in
`escalate_fn_to_manual_recon`) AND — per §8.2 — trigger an out-of-band operator alert ≤60s via
a pluggable channel (Telegram/HTTP/SMTP). **Adjudicate: is the §8.2 operator-alert channel in
PR-Z2 scope, or deferred (audit-only for now, alert as a follow-up)?** The alert is a whole
pluggable-transport subsystem; I'd recommend deferring it (audit + RMR land in PR-Z2; the
alert channel is its own piece) — but that leaves a Manual FN without a live push until then.

### STOP-S4 — SHIFT_OPEN live dispatch: PR-Z2 or a separate A2.2 piece?

Recon A notes the SHIFT_OPEN fail-closed arm was scoped to "A2.2 owns it". But the PILOT-GATE
e2e needs a LIVE online OPEN (edges 1/3 — stages ready). **Recommendation: fold online
SHIFT_OPEN live-dispatch into PR-Z2** (it's the "OPEN" half of open→sell→Z-close and the
stages already handle it; splitting it out would leave the pilot e2e unbuildable). **Adjudicate:
SHIFT_OPEN in PR-Z2, or keep it separate (and PR-Z2 pilots only Z-close on a shift opened by a
test seam)?**

---

## Proposed design (pending the STOPs)

Assuming STOP-S1=online-only, STOP-S2=flip-now, STOP-S4=SHIFT_OPEN-in:

1. **`inline::run` routing** — replace the two fail-closed arms with a doc-type dispatch:
   - `SHIFT_OPEN` → the happy-path template (acquire[edge1] → sign → send[edge3 confirm] →
     advance/finalize). No aggregation/quiescence (SHIFT_OPEN has no body to aggregate).
   - `Z_REPORT | SHIFT_CLOSE` → **quiesce first**: `quiesce_shift_before_z` → on `Pending`,
     fail-closed RETRYABLE (503, drain/B1 finishes the in-flight, client retries); on `Clear`,
     `aggregate_z_payload_for_shift` → `build_z_canonical` (D5 dual-hash) → happy-path template
     (acquire[edge8] → sign[Z-number, D5 gate] → send[edge10 confirm, advance-at-SEND] →
     finalize).
   - Both reuse `stage_acquire`/`stage_sign`/`stage_send`/`advance_to_ack` verbatim.
2. **Flag flip** — `FULL_Z_SURFACE_READY = true` IN THE SAME PR; update the tripwire test to
   assert the new (routed) behavior; remove the `debug_assert!(surface.is_err())` at
   `inline.rs:520` (it becomes false by design). Keep `ensure_full_z_surface_ready()` as the
   single gate the router consults (now Ok).
3. **Edges 4/12** — in the stage_send / dispatch reject path for a Z/shift doc, evaluate the
   recovery class; `EscalateManual` → `escalate_fn_to_manual_recon` (edge 4/12); DocumentReject
   → reissue/rollback (edge 11). Per STOP-S3, operator-alert (§8.2) likely deferred.
4. **Idempotency note (recon A):** the pre-flip ZSurfaceNotReady REJECTED-inbox consequence
   (same idem-key won't auto-fiscalize) is moot post-flip for new requests; no migration.

## Invariants to hold
- **INV-1** quiescence runs SHORT `with_immediate` TXs, no network/crypto (already so); the
  router calls it OUTSIDE the fn-gate-held long section per its contract.
- **INV-8** Z closes only via the C10 quiescence gate — the router MUST call
  `quiesce_shift_before_z` and refuse (503) on `Pending`; no bypass.
- **#2** single-writer — the router runs under the per-FN gate (`InlineWritePath` holds it).
- **D5** dual-hash + sibling gate apply to Z unchanged; advance-at-SEND for Z unchanged.
- **Zero migrations** (no schema change — all state/columns exist). **D6** no config knob (flag
  is a `const`, flipped in-code).

## Proposed test plan (RED-first anchors)
- **PILOT-GATE online e2e** (mirror `tests/write_path_inline.rs` + `rs1_supervisor_boot.rs`):
  boot → live online SHIFT_OPEN → SELL → online Z-close, all through the `InlineWritePath`
  binding; assert the shift walks Created→Opening→Opened→Closing→Closed and the Z doc reaches
  Ack with a stamped Z-number + aggregated TXS.
- **Quiescence-refusal pin:** a Z with an in-flight non-KVT2 doc → 503 (Pending), no Z minted.
- **Edge 4/12 pins:** online SHIFT_OPEN/Z with a transport-timeout-exhausted / hard-reject
  script → shift lands RMR + forensic audit; a DocumentReject → reissue/rollback (edge 11).
- **Flag-flip tripwire:** update `z_live_dispatch_is_gated_until_full_z_surface` to the routed
  contract (teeth: revert the routing → the e2e REDs).
- **Regression:** full nextest green (the online SELL/RETURN path + shift-edge tests unbroken).

## Recommendation
GO to a locked contract once STOP-S1..S4 are adjudicated. My defaults: **S1 online-only**,
**S2 flip-now (TXS complete, IO/EPZ absent-by-design)**, **S3 RMR+audit in PR-Z2, §8.2 alert
deferred**, **S4 SHIFT_OPEN in**. If S2 is "not yet" (IO/EPZ required), PR-Z2 STOPS — the whole
live-Z path is blocked on the surface, and that becomes a separate IO/EPZ contract first.

---

## LOCKED RULINGS (architect adjudication, 2026-07-07)

All four STOPs adjudicated; contract locked. Both ingress guards confirmed present on main
(SERVICE → Unsupported 422 pre-inbox; `acquirer_slip` → fail-closed `convert.rs:514`).

- **S1 — ONLINE-ONLY (confirmed).** PR-Z2 = edges 1/3/8/10 (staged) + 4/12 (→RMR) + 11
  (rollback). The 8 offline edges (2/5/6/7/9/13/14/15) → **A'.3 offline-enable** (needs the
  offline lane + drain finalization — a separate increment). **Honesty pin:** the full PILOT
  GATE formula includes the offline-drill; **PR-Z2 closes only the ONLINE half** of the pilot
  e2e (boot→OPEN→SELLs→Z-close). State this in the PR: *"PILOT GATE (online-half) closed;
  offline-drill half → A'.3."*

- **S2 — FLIP-NOW, with a MANDATORY coupling-tripwire (blocking flip condition).** Fiscal
  rationale: for THIS contour, absent IO/EPZ = *accuracy*, not under-reporting — the gateway
  structurally cannot accept the ops those sections report (SERVICE → 422 pre-inbox;
  slip-carrying payment → 422 fail-closed; Python-prod 4yr emitted both sections only on
  non-empty data = ground truth). BUT surface-completeness is now CONDITIONAL on those two
  ingress guards. **Flip condition:** next to `FULL_Z_SURFACE_READY = true`, land a
  **coupling-pin** (test + a doc-comment on the const) asserting SERVICE_IN/OUT stay
  ingress-rejected AND `acquirer_slip` stays fail-closed, worded: *"enabling either WITHOUT
  building its Z-half re-opens the under-reporting hazard — flip back or extend the surface IN
  THE SAME CHANGE."* Anyone who later enables SERVICE docs without the IO-half must break this
  pin LOUDLY. The `z_live_dispatch_is_gated_until_full_z_surface` tripwire flips deliberately
  per its own contract.

- **S3 — RMR + §8.1 audit IN PR-Z2; §8.2 operator-alert DEFERRED with an address.** Edges
  4/12 → `escalate_fn_to_manual_recon` (the idempotent seam already used by
  convergence/boot/SW-4) + §8.1 forensic audit — in scope. §8.2 operator-alert (≤60s pluggable
  Telegram/HTTP/SMTP) → **named residual addressed to the monitoring unit**
  (`project_backlog_monitoring` — itself a must-have-pre-pilot, off-by-default), NOT an open-ended
  "someday". **§6.5 retry budgets:** assess in the contract/impl phase — if the existing
  `error_routing` already yields them (transport-class → `ErrorRetryable` + convergence redrive),
  **REUSE it, do not build a parallel budget machine.**

- **S4 — SHIFT_OPEN IN (confirmed).** Without a live OPEN the e2e is unbuildable (a "clean
  production config" excludes a seeded shift). Online-lane SHIFT_OPEN dispatch closes HERE; the
  recon note "A2.2 owns SHIFT_OPEN" is revised — **enumerate the residual A2.2/A2.5 owns AFTER
  this PR so the roadmap entry isn't dead:** (a) **offline SHIFT_OPEN** (edge 2, → A'.3, not
  A2.2); (b) **resume × shift-guard interaction** (the A2.5-class item — a boot-resume of a
  half-open shift crossing the guard matrix); (c) any A2.2 shift-open pieces beyond the online
  happy-path + edges 1/3/4 (to be confirmed against the A2.2 tracker in the contract phase). PR-Z2
  claims ONLY the online SHIFT_OPEN happy-path + its edges (1/3 confirm, 4 → RMR).

### Contract frame (locked)
- Branch `feat/aprime2-z2-live-dispatch` off current main (`e1a7817`). Strict RED-first. Hot-zone
  (`inline.rs`) — the happy-path template `:560-922` is reused VERBATIM; orchestrator increment,
  NOT a stage rework.
- **Delivery order:** (1) SHIFT_OPEN dispatch + pins → (2) Z/SHIFT_CLOSE dispatch (quiescence C10
  via the existing gate; D5 dual-hash as-is) → (3) edges 4/12 → RMR + 11 rollback → (4) flag flip
  + coupling-pin + tripwire-flip in ONE commit → (5) PILOT e2e (online-half): boot → SHIFT_OPEN →
  SELLs → Z-close on a clean prod config via the live binding, `invariant_scan::assert_clean` at
  the end. Rewrite the Z-arm `debug_assert` per the routing reality.
- **Delivery gate:** adversarial lenses (hot-zone mandatory: invariants #1/#2/#8/#9, C10-bypass,
  D5, diff boundaries) + full nextest + fmt/clippy + 7-point report.
- **STOP-protocol:** the stages are "ready" per recon — on ANY divergence found while wiring them,
  STOP and triage to the architect; do NOT fix a stage silently.

---

## CONTRACT-PHASE FINDING (S3 assessment) — edges 4/12 are NOT cheap reuse → NEW STOP-S5

Assessing the S3 retry-budget ruling ("reuse `error_routing`+convergence if it already gives the
§6.5 budgets") surfaced a material gap. **The escalation FOUNDATION is reusable, but three pieces
are missing for edges 4/12 — and one of them touches a "ready" stage-adjacent file (STOP-protocol
trigger).**

**Reusable (verified):** transport failure → `RetryClass::TransientRetry` (`error_routing.rs:284`)
→ `ErrorRetryable`; the online convergence tick (`online_convergence.rs:375`) re-drives ER docs via
`stage_send::run` and, on `evaluate_er_redrive` (`er_redrive_policy.rs:86`) →
`BudgetExhausted`/`EscalateManual`, CASes to `RequiresManualReconciliation`. This is the same
escalate-to-RMR spine PR-Z2 wants.

**Gaps (must be resolved for edges 4/12):**
1. **SHIFT_OPEN hard-rejects mis-classified.** `is_close_shift()` (`error_routing.rs:547`) matches
   only `{SHIFT_CLOSE, Z_REPORT}`. A SHIFT_OPEN Server `-1/-11/-15` → `TerminalReject` → `Rejected`,
   never reaching the manual-escalation layer → **edge 4 never fires.** Fixing this edits
   `error_routing` — a stage-adjacent file the recon called "ready". **STOP-protocol: surfaced, not
   silently patched.**
2. **No per-class §6.5 budgets.** Only a flat `MAX_BOOT_ATTEMPTS = 5` for all `TransientRetry`
   (`er_redrive_policy.rs` + `transport_trace::attempts_used`). No per-code differentiation
   (Transport 20/≥30min vs Server-5 3/≥5min vs Server-11 0/immediate), no wall-clock `first_failure_at`,
   no backoff. Full §6.5 compliance = a budget-engine refactor of `transport_trace` +
   `er_redrive_policy` (a new column + timer state) — a sizable increment on its own.
3. **No shift-recovery classifiers.** `ShiftOpenRecoveryClass` / `ShiftCloseRecoveryClass` (spec
   §6.2/6.4: RetryClass + shift_id + shift.state → shift-edge verdict) do not exist. `error_routing`
   is a pure doc-state classifier (no shift context by design) — the shift-edge decision is a new layer.

### STOP-S5 — how much of edges 4/12 is in PR-Z2?

The PILOT online-half e2e (boot→OPEN→SELLs→Z-close on a clean config) is the **happy path — all
Acks — so edges 4/12 (the hard-reject / timeout FAILURE paths) are NOT exercised by it.** So PR-Z2's
live-dispatch unblock does not strictly depend on 4/12. Three cuts:

- **(A) Minimal-viable 4/12:** fix the SHIFT_OPEN classification (gap 1) + reuse the existing
  flat-5 convergence→RMR escalation (gap 2/3 deferred). Unblocks edges 4/12 to RMR+§8.1 audit, but
  the §6.5 timing (20/≥30min etc.) lands as a **named residual → recovery-hardening increment**.
- **(B) Full §6.5:** build the per-class budget engine + shift-recovery classifiers now. Large;
  pulls a recovery-machine refactor into a dispatch PR (couples two concerns).
- **(C) Defer 4/12 entirely:** PR-Z2 ships the live ONLINE happy-path dispatch (steps 1/2/4/5) +
  edge 11 (Closing→Opened on a pure DocumentReject — cheap, no budget machinery). Edges 4/12
  (hard-reject/timeout → RMR) + the §6.5 budget engine + shift-recovery classifiers become a
  SEPARATE recovery increment (they share the offline-drain escalation machinery A'.3 also needs).

**Recommendation: (C), or (A) if you want the RMR safety-net landed with the dispatch.** (B) violates
the "orchestrator increment, not a recovery build" framing and couples PR-Z2 to a budget refactor.
(C) keeps PR-Z2 = the live-dispatch happy path (the actual pilot-online-half unblocker) and quarantines
the failure-path recovery machinery into its own increment (naturally co-scoped with A'.3's offline
escalations). Either way, the §6.5 budget engine is its OWN piece, not smuggled into PR-Z2.

**This STOP re-opens the step-3 line of the delivery order and must be adjudicated before code.**

### STOP-S5 RESOLUTION (2026-07-07) — option (A) minimal-viable

Architect re-locked the contract with edges 4/12 IN step 3 + the standing S3 ruling "§6.5
budgets — reuse, do NOT build a parallel [budget engine]". Binding interpretation:

**Step 3 = option (A) minimal-viable.** Edges 4/12 escalate to RMR via the existing idempotent
`escalate_fn_to_manual_recon` seam + the §8.1 forensic audit (IN scope). The **§6.5 per-class
budget engine (wall-clock / backoff / `first_failure_at` column / ShiftRecoveryClass) is NOT
built** — that is the "parallel" machine the S3 ruling forbids; it becomes a **named residual →
the recovery increment (co-scoped with A'.3 offline escalations + `project_backlog_monitoring`
§8.2 alert)**. Edge 11 (DocumentReject → Closing→Opened rollback) IS in scope (cheap, no budget
machinery). **STOP-protocol valve:** if wiring 4/12 minimally proves to require the full
ShiftRecoveryClass layer (a DocumentReject-vs-hard-reject decision that cannot be made at the
orchestrator level from the existing SendDisposition), STOP and re-triage rather than growing a
recovery build inside this dispatch PR.

**Execution: NOW, in-session (architect switched to a 1M-context model — the "fresh session"
recommendation is withdrawn; the budget was granted).**

---

## STEP-1 DONE (2026-07-07, commit `8dbed91`)

SHIFT_OPEN live dispatch: removed the `inline::run` SHIFT_OPEN fail-closed arm → SHIFT_OPEN falls
through to the doc-type-agnostic happy-path stages (acquire edge1 Created→Opening, sign ShiftOpen,
send edge3 Opening→Opened, advance→Ack). Confirmed recon B: the stages were built; the gap was pure
orchestrator routing. Pins: `online_shift_open_reaches_ack` (RED-first: 422 → ACK + shift OPENED) +
`online_shift_open_while_open_is_shift_already_open` (guard governs live: SHIFT_ALREADY_OPEN 422, no
mint). Dead `SHIFT_OPEN_NOT_SUPPORTED` code removed; obsolete fail-closed test deleted. Build clean.

## STEP-2 DESIGN (Z/SHIFT_CLOSE) + STOP-S6

**Structure (locked):** extract the happy-path TAIL (`inline.rs` stage_acquire→…→finalize, :585-909)
into `run_staged(pool, …, row, command)` — a behavior-preserving refactor guarded by the 18 existing
`write_path_inline` tests. The SELL/RETURN/SHIFT_OPEN path becomes `build_canonical → run_staged`; a
new `run_z_dispatch(pool, …, row)` does the Z prefix (resolve current_shift_id → `quiesce_shift_before_z`
→ `aggregate_z_payload_for_shift` → `build_z_canonical` → `run_staged`). Testing `run_z_dispatch`
DIRECTLY is flag-independent, so step 2 is RED-first-testable BEFORE the step-4 flag flip (the
`FULL_Z_SURFACE_READY` const can't be flipped at runtime). inline::run's Z arm: `if
ensure_full_z_surface_ready().is_ok() { run_z_dispatch } else { <501 fail-closed> }` — production stays
501 until step 4 flips the const. All Z helpers are `pub` + importable (verified). Z advance-at-SEND +
D5 sibling gate apply unchanged. The Clear path is unambiguous + PILOT-critical.

### ⚠️ STOP-S6 (seam-contract conflict on `QuiescenceOutcome::Pending`) — NEEDS A RULING

`quiesce_shift_before_z` returns `Pending{blocking}` when in-flight receipts remain; its doc-comment
says the caller "returns IN_PROGRESS / Z_QUIESCENCE_PENDING WITHOUT creating a Z doc; the Z retries once
the runtime drives them issued." **But this is entirely unbuilt AND it collides with the seam contract**
(`seam.rs:213-225`): returning ANY non-`NotImplemented` `FiscalError` MUST leave the inbox non-`NEW` +
terminal (the lease moved NEW→PROCESSING; a terminal refusal drives it terminal). So a true
retryable-leave-NEW is a **seam-contract exception the handler doesn't support** — and building it means
touching `handler.rs` / `seam.rs` (the ingress boundary the contract declared off-limits).

Options for the Pending arm:
- **(A) leave-NEW + handler recognises a pre-lease retryable** — true same-key retry (202 on replay).
  **Touches ingress/handler → OUT of the PR-Z2 boundary.**
- **(B) terminalise (respect the seam) + `OfflineRefused{Z_QUIESCENCE_PENDING}` 503** — first POST 503;
  a same-key replay → 500 (REJECTED-inbox resolve); the operator retries with a NEW request. Fiscally
  SAFE (no doc minted, no double-Z), boundary-clean (no ingress touch). Cost: the documented "same-key
  retry" needs a new key. Named residual: true same-key-retry → the recovery/handler increment.
- **(C) new `FiscalError::QuiescencePending` variant + handler branch** — explicit, but touches seam.rs
  + handler.rs (ingress boundary).

**Recommendation: (B)** for PR-Z2 — it is the only boundary-clean option, fiscally safe, and the PILOT
online-half e2e never hits Pending (SELLs Ack before Z → Clear), so it does not gate the deliverable.
The true same-key-retry (A/C) becomes a named residual co-scoped with the recovery increment (which
already owns the handler-aware escalation work). **Architect: ratify (B), or ring-fence an ingress
change for (A)/(C)?** The Clear path proceeds regardless; only the Pending arm awaits this ruling.
