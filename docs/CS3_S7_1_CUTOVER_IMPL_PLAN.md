# CS-3 S7-1 cutover — implementation-sequencing plan (grounded)

**Not a new design.** The design-of-record `CS3_S7_1_DOUBLE_ISSUE_SAFETY_DESIGN.md` (§2-§5 / §8 / §9 /
§11) is FROZEN + round-2 external GO. This doc re-grounds its anchors against the **current** tree
(branch `cs3-de-slice7-cutover` off `main`/`1999ff1`, after the S7 foundation landed) and orders the
atomic build. The cutover is the **release-critical, live double-issue kill** — one atomic commit,
teeth RED-first, its own explicit GO already given.

## 0. Re-grounded anchor map (design@53c5b13 → current)

| Item | Current anchor | Note |
|---|---|---|
| WIRE `send_chk_observed` | `stage_send.rs:1568` | relocate behind `submit_authorized` |
| `submit_authorized` (built, sole-wire) | `services/write_path/submit.rs:45` | relocation target |
| 4-pre `with_immediate` #1 | `stage_send.rs:1244` | central authorize + P3 guard here |
| **legacy 4-b `with_immediate` #2** | `stage_send.rs:1710` … END (confirm at edit; design :1972) | DELETE whole block |
| 4-b seed-advance mac skip | `stage_send.rs:1800` (`if mac_recovery_attempts < 1`) | the sharp non-idempotent edge |
| R1 `(ErrorRetryable,Sending)` edge | `fiscal_documents.rs:257` | delete the arm |
| R2 ER in 4-pre source allowlist | `stage_send.rs:1292` (+ second site `:1420`) | remove `ErrorRetryable` (both sites — confirm :1420 role) |
| R3 `run_mac_recovery` (`-12`) | `stage_send.rs:1081` (call), budget `:1039` | short-circuit BEFORE the call |
| R4 Sent+NotFound #1 | `boot_phase.rs:950` `cas_sent_to_error_retryable_from_probe` (call `:2870`) | retarget → `sent_not_found_to_manual` |
| R5 Sent+NotFound #2 | `kvt2_confirm.rs:1651` `commit_sent_replay_envelope_1c_post` (call `:1021`) | same (keep its distinct `outcome_kind`) |
| R6 Redrive callers ×3 | `online_convergence.rs:560`, `boot_phase.rs:3135`, `backlog_drain.rs:1544` | route to RMR/STOP |
| `ErRedriveDecision::Redrive` | `er_redrive_policy.rs:43` (variant), `:99` (return) | collapse → `EscalateManual{TransientRetry}`, delete variant |
| `sent_not_found_to_manual` (built) | `sent_not_found.rs:67` | R4/R5 target |
| #1 `reset_stop_mode` | `admin.rs:300` | add PENDING_APPLY guard |

**Verdict:** anchors stable — wire `:1568`, R1 `:257`, 4-b start `:1710`, seed-skip `:1800`,
`reset_stop_mode:300` are byte-exact vs the design; only R2/R6/R4/R5 line-drifted (symbols intact). No
structural surprise; the frozen design implements faithfully. **Two items to pin at edit time:** the
exact legacy-4b END line, and the role of the second ER-allowlist site `:1420`.

## 1. Build order (§9) — one atomic cutover, teeth RED-first

The teeth (§8) must be RED-first PROVEN before the atomic flip. Order within the single cutover commit
(all landed together — a partial state either re-wires = double-issue, or stucks a doc = BRICK):

**Phase T (teeth first, still on the pre-cutover code where they should currently behave a certain way):**
1. S7-P2-2 static sole-seam (send_chk_observed 1 call-site, submit_authorized 1 caller, compile-fail pins)
2. S7-P2-1 sole-wire (2 concurrent `run` → 1 wire; revert-canaries on index + direct-wire)
3. S7-P3-1 single seed-writer · S7-P3-2 mac-divergence
4. S7-P2-3 3-caller BRICK matrix (each Redrive → wire=1 + RMR, never spin)
5. S7-P3-3 Sent+NotFound → RMR+STOP · S7-P3-4 pre-wire predecessor guard

**Phase C (the atomic composition):**
6. Relocate wire `:1568` → into `submit_authorized`; central 4-pre + P3 predecessor guard (§2.1).
7. Delete legacy 4-b `:1710-END` (incl. the `:1800` mac-skip seed advance).
8. Activate record/apply (record_outcome → apply orchestration; boot uses the same apply — landed).
9. R1-R7 together: R1 (`fd:257` delete) · R2 (`ss:1292`/`:1420` remove ER) · R3 (short-circuit before
   `ss:1081`) · R4 (`boot_phase:950`→sent_not_found_to_manual) · R5 (`kvt2_confirm:1651` same) · R6
   (3 callers → RMR/STOP, `er_redrive_policy:99` collapse + delete variant) · R7 (consumers).
10. #1 `reset_stop_mode:300` PENDING_APPLY guard (`AdminError::PendingResolutionRequired`).

**Phase G (pre-deploy):** empty-in-flight gate OR fail-closed→RMR/STOP for reservation-less legacy
`SENDING`/`ERROR_RETRYABLE` rows (do NOT infer safe from `transport_trace`).

## 2. Verification

Every §8 tooth RED-first (revert-canary proven). Full gate (fmt · clippy --all-features -D · nextest
--all-features · inventory re-mint). Then a **decorrelated re-audit** (internal) + **external
model-decorrelated review** on the atomic diff (living double-issue risk, P3-sensitive) before merge —
the cutover carries its own explicit GO but the P2/P3 teeth must be demonstrated.

## 3. Invariant posture

The whole point is P2 (≤1 wire/lifetime) + P3 (pre-wire predecessor) + no BRICK. Frozen invariants
#1 (wire outside tx — the relocation keeps `send_chk` between the two tx, not inside), #2, #4, #8, #9
preserved. This is the one place the live send-path moves — hence teeth-first + double review.

## 4. Next actions

1. (done) branch `cs3-de-slice7-cutover` off main + anchor re-grounding.
2. Pin the legacy-4b END boundary + the `:1420` ER-allowlist role (Read the full 4-b block).
3. Implement Phase-T teeth RED-first.
4. Implement Phase-C atomic composition + Phase-G gate.
5. Gate + decorrelated re-audit + external review → merge on GO.
