# CS-3 Bridge + D/E — terrain map (next stage after 3.2)

**Status:** RECON terrain map (not a spec). Seeds the next stage: taking the CS-3 3.2 **read-only
shadow** LOAD-BEARING. Anchors are **recon-grade — re-verify each file:line at spec time** (grounding
gate). 3.2 delivered `map_send_reply` wired read-only at `stage_send.rs:1573`
(`let _shadow_response = …`, drives nothing); Bridge + D/E make it authoritative.

---

## The 5 axes

### 1. Shadow go-live — what the LIVE path drives today (must be replicated)
Branch point ~`stage_send.rs:1525`. On `WireDecision::Sent` the live path drives, atomic in the 4-b
envelope: CAS `Sending→Sent`; `set_server_fiscal_no_tx`; **seed advance**
(`node_state.last_known_unsigned_xml_sha256`) — the A.3 sfn-lockstep pin (sfn set ⟺ seed advanced);
shift confirm edges 3/10; `transport_trace::complete_tx`; audit row. On error: `node_mode_flip`
(→BLOCKED) + target-state CAS. **Minimal seam:** shadow → durable `ObservedOutcomeV1` returned from
4-a → **record-then-apply** (4-b-i record, 4-b-ii apply) with the apply CAS checking
`stored_authorized_generation == node_state.delivery_generation` (replay guard). **Correction (external
audit):** the `delivery_reservation` table (migrations 032/033/034, INACTIVE) and `ObservedOutcomeV1`
(`mod.rs:1114`) **already exist** — they are NOT new. **TO-BUILD:** the ACTIVATION (record-then-apply
wiring), **migration 035** (fence index through `PENDING_APPLY`), and the total `ObservedOutcomeV1 →
ApplyPlan` (missing: `target_state`/audit/trace/probe). Slices C-pure(ApplyPlan)/C-DB/D. See the dossier
§3-D for the grounded detail.

### 2. DpsChannel → DpsSubmissionPort
`transports/dps/channel.rs` — `DpsChannel` trait exists (`send_chk`/`last_chk`/`ping`/`status_rro`/
`info_rro`/`ask_offline_codes`/…). **`DpsSubmissionPort` / `submit_raw` DOES NOT EXIST — TO-BUILD
(Bridge).** Adapter layer, not a cut: `GrpcDpsChannel` also impls the port; both coexist. Enforce a
**static sole-caller gate** — the only path to `submit_raw` is `submit_authorized` (D). Update the
contract-DAG pin (`prro-domain/tests/rp_cs1_4_contract_dag.rs`) if a contract crate gains a real dep.

### 3. Authoritative record
**Correction (external audit — supersedes the stale text below):** `ObservedOutcomeV1` (`mod.rs:1114`),
the reservation/outcome columns (migrations 032/033/034, INACTIVE), and the `authorized_generation`
snapshot **already EXIST** — they are NOT "ALL TO-BUILD". What is TO-BUILD: **production activation**
(the `record_outcome_tx` + apply repository wiring, parallel to `transport_trace.rs`/`audit_log.rs`), the
**total `ObservedOutcomeV1 → ApplyPlan`** (missing `target_state`/audit/trace/probe), and **migration
035** (fence index through `PENDING_APPLY`). The effect-discriminant `node_effect` — its **column and
immutability trigger are in 033** (033:185 / 033:280) — is already present; **034** adds the
OUTCOME_OBSERVED-completeness + clean-accept constraints (034:42). See the dossier §3-D for the grounded
detail.

### 4. Blind-resend kill
Current loop `stage_send.rs:~1013` `loop { run_one_attempt() }`; MAC-recovery dispatch re-enters via
`continue` on `Resigned` (`mac_recovery.rs`, single-bit budget `mac_recovery_attempts 0→1`). **Kill:**
remove the `continue` arm (no attempt #2), fence the doc under a `CALL_STARTED` reservation, leave it
`ErrorRetryable` for W9 reconciliation to probe; bytes immutable, orchestrator deferred (safe re-add
is future work). D+E replace the loop with fence-based gating.

### 5. D/E entry-conditions (already flagged)
- **(a) authority co-location vs lint — RESOLVED:** lint + second-cofounder review accepted for the
  trusted-contributor model → **no code change** (recorded in the 3.2 spec §4.4 D/E note).
- **(b) `NoResponseCause::CrashedBeforeObservation` consumer — TO-BUILD:** a boot scanner (E slice,
  `services/reconciliation/boot_phase.rs`-ish) that on reboot finds `delivery_reservation.state=
  CALL_STARTED AND outcome IS NULL` → route to reconcile-only (NEVER re-send); fence held. Pattern to
  copy: `close_orphan_transport_traces` (the boot-phase fn `boot_phase.rs:1553`, run at boot from
  `app.rs:548`). The M2 `TransportAbsence` seal already keeps
  the transport from minting this cause; only the recovery path may.
- **(c) drift-pin only `{retry_class, node-Blocked}` — becomes real at D/E:** the finer `node_effect`
  (MacReseedPending/OperatorEscalation/…) and immutable `target_state` are enforced from the RECORDED
  outcome, not re-routed live (RP4B-9). C-pure adds `classify()` → `(certainty, provenance, routing)`;
  `SubmittedUnknown` docs stay fenced, never blind-resent.

---

## Sequencing (locked, spec §2)
`C-pure ‖ B → C-DB → A → A′ → Bridge → D → E`. **HARD RULE: D and E ship in the SAME production
release** (D creates reservations without enforcement; E enforces the fence — D-without-E = risk with
no gain; E-without-D = nothing to enforce). Bridge is the last pre-D/E transport-seam adapter.

## Top 3 risks / constraints
1. **INV-1 (no net/crypto in write-tx) — CRITICAL.** The new 4-pre (`RN→CALL_STARTED`) and the
   record-then-apply split must NOT re-enter the wire call. Each tx boundary = DB-only; the wire (4-a)
   stays strictly between committed tx boundaries. RED-pin: `submit_authorized`/4-a not inside any
   `with_immediate`.
2. **Sole-issuance CAS must not move (A4-3/D2) — HIGH.** The seed/sfn/shift-edge CAS is THE issuance
   moment; D wraps it (updates `delivery_reservation → APPLIED` in the SAME atomic batch), never
   reorders it. A crash between issuance and reservation-apply must be boot-detectable → reconcile, not
   resend.
3. **Legacy cutover — HIGH.** At E go-live, in-flight `Sending`/`ErrorRetryable` docs WITHOUT a
   reservation must fail-closed (fenced / manual recon), never auto-redrive (`transport_trace` alone
   can't certify safety — A4-1). Pre-cutover: drain to a clean state, or a migration-time legacy gate.

---

## Related
`docs/superpowers/specs/2026-07-18-cs3-32-transport-engine-seam.md` (3.2 spec, §D/E note),
`project_spec4b_dps_contract_go` (raw-port/engine-mint split), `project_cs3_32_pr_series`.
