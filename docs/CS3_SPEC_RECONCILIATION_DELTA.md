# CS-3 Spec-Family ↔ Realized-3.2 Reconciliation Delta

This document is the structured delta between the CS-3 Bridge/D-E **spec family** (spec4b DPS
contract, spec2 delivery-reservation FSM, spec1 executable transition contract, and the
double-issue keystone plan) and the **realized CS-3 3.2 code** (`prro-domain` + `prro`,
worktree snapshot). It is the input to the next-stage D/E dossier: it tells us which spec
assertions are already true in code (REALIZED), which have genuinely drifted and must be
written back into the specs before those specs become the D/E oracle (DRIFTED), and which
remain unbuilt (TO_BUILD). Every anchor here is **recon-grade** — each REALIZED/DRIFTED claim
was file:line-grounded by a per-spec reconciler, and **every claimed drift was
independently re-verified by an adversarial verifier** (`CONFIRMED` = real drift; `REJECTED`
= reconciler was wrong, no real divergence). Drifts marked REJECTED are **not** real drifts
and must not be actioned as such.

---

## spec4b — DPS boundary delivery contract

> **Headline:** The TYPE half of #4B (§2/§3/§5 delivery axes; SendResponse/SendOutcome/DpsReject/
> NoResponseCause/classify) is substantially REALIZED in `prro-domain` 3.2 — with two honest drifts
> (opaque not public enum; AuthenticatedPeerGarbage removed); the entire §6 raw-port / submit_raw /
> authorize_submission / reconcile-validator half plus the R4 RetryClass relocation is still TO_BUILD.

**REALIZED**
- Three orthogonal axes (SubmissionCertainty / ResponseProvenance / ActiveRetryClass-7) — `prro-domain/src/delivery/mod.rs:215,241,270`
- `SendOutcome` = Accepted/Rejected/Indeterminate disjoint partition — `mod.rs:495`
- `DpsReject` complete closed 13-code set, no free-form ctor — `mod.rs:648`
- `SendIndeterminate` (UnknownStatus sole free-form; SaveError/CloseAmbiguous/OkButNoFiscalNumber) — `mod.rs:687`
- -2/-15 close-split by doc_type, consumed once at construction — `mod.rs:571` (`from_server_code`)
- Total `classify()` + `SubmissionEvidence` + SE-1 fail-safe + RP4B-2 graph-pin — `mod.rs:892,777`; `prro-domain/tests/rp4b_2_classify_graph_pin.rs`
- 032 CHECK matrix (3 states, 3 axis columns, envelope_hash=32, call_started_at marker) — `prro/migrations/032_delivery_reservation.sql:78-95`
- AM-1 forward-progress (`Parsed(_)` only) + AM-2 incumbent TLS-auth seam (shadow, drives nothing) — `mod.rs:481`; `prro/src/transports/dps/grpc.rs:206`
- §4.4 engine mapper wired READ-ONLY as `_shadow_response` — `prro/src/services/write_path/shadow_map.rs:25`, `stage_send.rs:1573`

**DRIFTED**

| Claim | Spec says | Code does (file:line) | Verdict |
|---|---|---|---|
| send-response-enum | `SendResponse` = public 3-arm enum w/ struct ctors | opaque `struct SendResponse(SendResponseInner)` over private inner enum; read via `kind()`, built via source-gated ctors — same 3 arms — `mod.rs:435` | **CONFIRMED** (benign Class-A sealing; spec text is the stale artifact) |
| no-response-cause | exactly 4 arms | 5th arm `CallFailedWithoutTrustedDpsEnvelope` added (§4.3 branch 9); B4 intent preserved — `mod.rs:391,406` | **CONFIRMED** (scope-additive) |
| remote-status-evidence | 2 arms (AuthenticatedPeerGarbage + RemoteAuthStatus) | single arm `RemoteAuthStatus(GrpcStatusDigest)`; AuthenticatedPeerGarbage removed (tonic collapses decode-fail → Internal → NoResponse) — `mod.rs:416` | **CONFIRMED** (opposite of spec's SE-2) |
| retryclass-relocate | RetryClass → prro-domain + compat re-export + `From<ActiveRetryClass>` widening + `set_routing` store API | RetryClass stays in `prro/src/services/write_path/error_routing.rs:69`; no re-export, no `From` impl, no `set_routing` fn | **CONFIRMED** (relocation triple unbuilt) |
| digest-decoded-not-rawwire | type named `RawResponseDigest` (wire bytes) | renamed `DecodedResponseDigest` + distinct `GrpcStatusDigest`; honest decoded-content — `mod.rs:107,141` | **CONFIRMED** (deliberate 3.2 rename; strengthens finding) |
| digest-source-gate | digest mints placement-enforced as a Rust-privacy type-seal | best-effort CI syn-lint (`prro/tests/digest_mint_source_gate.rs`), not a type-seal — `mod.rs:107-132` | **REJECTED** — reconciler conflated the spec's option-(a) type-seal (which the spec applies ONLY to the `AuthorizedSubmission` token, §6/§11/R3) with the digest mints, which the spec never sealed. No spec assertion to diverge from; the code is honest and self-consistent. **Not a real drift.** |

**TO_BUILD**
- `AttemptObservation` (reservation-identity wrapper over evidence) + AO-2 binding echo-check — `code_ref: none`
- `prro-dps-contract` raw-port `DpsSubmissionPort::submit_raw(BoundSignedEnvelope)` (lib.rs still CS-1d empty skeleton) — `code_ref: none`
- Engine-private `AuthorizedSubmission` token + `authorize_submission` (RN→CALL_STARTED CAS) + `submit_authorized` — `code_ref: none`
- Contract-owned `validate_reconcile` / `ReconcileValidation` / `ProvenCorrelation` / `FnLiveness` (attributed outcome *types* exist at `mod.rs:829,802`; the minting validator does not) — `code_ref: none`

---

## spec2 — Delivery-reservation FSM (the D/E fence)

> **Headline:** The spec's TYPE layer (three orthogonal axes, SubmissionEvidence inputs, generation/
> outcome records, total classifier) is largely REALIZED in `prro-domain/delivery` with some naming/
> shape drift, but the actual runtime reservation FSM — the durable CALL_STARTED-before-send marker,
> atomic OutcomeObserved→ledger apply, crash-window resolution, and the SubmittedUnknown chain-
> generation FENCE — remains TO_BUILD: table/columns/apply-state exist only as INACTIVE schema + an
> uncalled repo, and the typed contract is wired READ-ONLY as `_shadow_response` driving nothing.

**REALIZED**
- AuthenticatedPeer/garbage is degraded-only; `proves_dps_forward_progress()` true for `Parsed` only — `prro-domain/src/delivery/mod.rs:481,931`
- Transport-evidence phase rule (NotStarted→NotSubmitted; every non-accept Started→SubmittedUnknown) enforced at the type layer — `mod.rs:776,892,913`

**DRIFTED**

| Claim | Spec says | Code does (file:line) | Verdict |
|---|---|---|---|
| reservation-fsm-states | durable FSM RN→CallStarted→OutcomeObserved drives the call lifecycle | exact 3 states as a strict table CHECK + repo, but repo is INACTIVE with **zero production caller** (tests only) — `032_delivery_reservation.sql:81,86`; `delivery_reservation.rs:9,78` | **CONFIRMED** (live FSM vs persistence shell) |
| crash-window-resolution | boot resolves 3 reboot windows to SubmittedUnknown / boot-idempotent apply | 033 apply_state scaffolding INACTIVE; `CrashedBeforeObservation` classifier target un-mintable by any live path; live boot resolver uses fiscal_documents FSM → ErrorRetryable/RMR (not SubmittedUnknown) — `033...sql:181`; `mod.rs:398` | **CONFIRMED** (mechanism + target states diverge) |
| shadow-readonly-not-driving | 3 fields RECORDED before the target_state collapse (§8 cut-point) | typed `SendResponse` bound `_shadow_response`, drives nothing; live decision still legacy `route_send_result`→WireDecision collapse; fields computed but never recorded — `stage_send.rs:1573,1587`; `shadow_map.rs:15` | **CONFIRMED** (spec is design-locked target; no read-only carve-out) |
| three-orthogonal-fields | routing axis = "the 8 RetryClass" | split into 7-value `ActiveRetryClass` (fresh) + decode-only `HydratedRetryClass::DrainChainSettleRetry` — `mod.rs:214,240,269,308` | **REJECTED** — spec §2(c) explicitly tags the 8th value `(legacy-decode-only)`; the split is the faithful realization of that tag, all 8 remain decodable. **Not a real drift.** |
| minus4-orthogonality | -4 → {SubmittedUnknown, ParsedDpsEnvelope, TransientRetry}, no resend/offline-arm | exact triple via sealed `UnknownStatus` variant + NoNodeEffect — `mod.rs:603,1008,964` | **REJECTED** — certainty/provenance/routing all match; routing via general UnknownStatus mechanism is the spec's own framing (-4 ERROR_UNKNOWN is the canonical member). Representation choice, not divergence. **Not a real drift.** |
| authorized-generation-token | active apply-CAS comparing stored-vs-live generation | `AuthorizedGeneration` type + generation↔certainty invariant in `ObservedOutcomeV1::record` + DB column all present; the compare CAS is unwired (openly deferred to CS-3) — `mod.rs:1071,1144`; `033...sql:344` | **REJECTED** — type + invariant + column all realized faithfully; only runtime activation is deferred (documented "INACTIVE … Activation is CS-3"). Maturity/scope, not code-vs-spec divergence. **Not a real drift.** |
| authenticated-peer-garbage-removed | provenance set w/ a distinct peer-garbage evidence value | `ResponseProvenance` 3 values (incl. AuthenticatedPeer) implemented verbatim; only the intermediate `RemoteStatusEvidence::AuthenticatedPeerGarbage` sub-variant removed; garbage body → SubmittedUnknown (strictly more conservative) — `mod.rs:411,415`; `raw_reply.rs:216` | **REJECTED** — the spec never named `AuthenticatedPeerGarbage` as an evidence *value* ("garbage" is descriptive); the 3-value axis + no-forward-progress behavior are honored. Internal refinement, not drift. **Not a real drift.** |

**TO_BUILD**
- CALL_STARTED durable-before-`send_chk` marker (code only computes a throwaway local `wire_call_started_at`; never persisted) — `code_ref: none`
- Atomic OutcomeObserved-evidence + ledger effect in one tx / durable PENDING_APPLY re-applied at boot — `code_ref: none`
- SubmittedUnknown durable per-FN chain-generation FENCE (predicate index exists; no enforcing reader in acquire/write_path) — `code_ref: none`
- Per-protocol `ReconciliationCapability` + default-RMR gate — `code_ref: none`
- Cross-protocol `NoSafeReconciliationIdentity` guard — `code_ref: none`
- RP-8 WAL+synchronous=FULL power-cut durability pin (no subject until the marker exists) — `code_ref: none`

---

## spec1 — Executable transition contract

> **Headline:** The spec's Appendix A/B transition tables — especially the -12/MAC-recovery, node-block,
> and ErrorRetryable rows — are REALIZED and accurately file:line-grounded against 3.2, but the spec's
> own central abstractions (§3 data-driven transition matrices + §4 the ONE pure `admission()` rule-list)
> remain TO_BUILD documentation; the durable delivery-contract types the tables lean on are all present
> in prro-domain 3.2 (read-only shadow).

**REALIZED**
- Document `allowed_transition` whitelist w/ all A.3 PR-B removed edges absent & never re-admitted — `prro/src/db/repositories/fiscal_documents.rs:174-261`
- ErrorRetryable→Rejected -12 MAC-recovery-failure override (wire_status_code=-12, budget spent, no fresh send) — `stage_send.rs:2030-2062`; `fiscal_documents.rs:259`
- Sending→ErrorRetryable (transient) and Sending→Rejected (pre-SENT terminal, D2 seed-not-advanced) — `error_routing.rs:291-293,458-466`
- -11 → Sending→Rejected + node→BLOCKED atomic CAS (NodeStateMissingForBlock breach) — `error_routing.rs:504-515`; `stage_send.rs:1869-1888`
- -12 MacRecovery routing + single-bit bounded orchestrator (at-most-once/run) — `error_routing.rs:516-534`; `mac_recovery.rs:1-56`
- Shift 15-edge matrix + offline-session 6-edge matrix (verbatim) — `shifts.rs:74-94`; `offline_sessions.rs:142-153`
- `TransitionOutcome` = Applied/Forbidden/Conflict/NotFound (Forbidden-before-DB) — `fiscal_documents.rs:166-172`
- Quiescence scan (StuckSending + StuckNonTerminalDoc; resting states excluded) — `invariant_scan.rs:214-241`
- Delivery-contract types present (3 axes + NodeEffect: -11→NodeBlocked, -12→MacReseedPending, -6→OperatorEscalation) — `mod.rs:214-361,983-1004`; wired READ-ONLY as `_shadow_response`
- §4.4-gated `map_send_reply` (6-arm total) wired READ-ONLY — `shadow_map.rs:25-53`; `stage_send.rs:1569-1573`
- Single-writer structural via global-single-writer + BEGIN IMMEDIATE lease (coarser than per-FN, adequate for RP-6) — `stage_acquire.rs:36-37`

**DRIFTED**

| Claim | Spec says | Code does (file:line) | Verdict |
|---|---|---|---|
| payloaded-events-in-code | each matrix row names its event+guard as first-class row data (RP-1 0-or-1-row check) | `allowed_transition` is `(from,to)`-only bool; events/guards live in scattered invokers / spec Appendix A "evidence" column — `fiscal_documents.rs:174-261` | **REJECTED** — category error: the spec **explicitly** acknowledges the `(from,to)`-only code reality (§3, Appendix A line 149, A.3 line 226, §9.4 residual) and authors events/guards as a spec-side mapping. The code matches what the spec asserts about the code. **Not a real drift.** |
| denial-not-conflict-rp2 | HOLD/legal denial → Denied/Deferred (no recompute); only VersionConflict recomputes | store returns `Conflict` (not `VersionConflict`); no Denied/Deferred decision layer at all — `fiscal_documents.rs:166-172` | **REJECTED** — spec §4 itself defines the abstract `VersionConflict` as mapping to the real seam name `Conflict` (conformance, not drift); the missing Denied/Deferred half is an unrealized-by-design RED-pin on a DESIGN-LOCKED spec, not a contradiction of existing behavior. **Not a real drift.** |

**TO_BUILD**
- The ONE pure `admission(OperationKind, axes) → Allow(plan)|Denied|Deferred|NoTransition` ordered rule-list (only narrow `time_budget::admission_refusal` + scattered guards exist) — `code_ref: none`
- Data-driven executable `(from,event,guard)→(to,EffectPlan)` matrix + `TransitionPlan` applier (code is `matches!` whitelists + imperative dispatch) — `code_ref: none`
- Appendix A.3 shift-row events/guards/effects code-grounded to A.1/A.2 rigor (spec self-flags as TODO-curation; only 15 `(from,to)` pairs grounded) — `code_ref: none`

---

## keystone — CS-3 double-issue kill plan

> **Headline:** The spec's C-pure/A/A′/B foundation slices (delivery contract types, -4 Indeterminate
> seam, RemoteStatus auth seam, migration 033/034 schema incl. authorized_generation column) are
> REALIZED in 3.2, but the double-issue KILL itself — NS-1 fence gating, NS-3 -12-loop removal, and
> the durable authorized_generation replay-CAS (D/E slices) — is entirely TO_BUILD; and the spec's
> "current hazard" -4 grounding has DRIFTED (the type split shipped, but -4 still routes as blind-resend).

**REALIZED** (hazard baselines realized-as-described, i.e. still un-fenced, + built foundation slices)
- Seven un-gated `stage_send::run` call-sites (NS-1 must bound all) — `inline.rs:910`; `online_convergence.rs:561`; `boot_phase.rs:3072,3685,3943`; `backlog_drain.rs:1321,2959`
- `(ErrorRetryable, Sending)` re-send edge live; no SubmittedUnknown discriminant on the doc (grep-empty) — `fiscal_documents.rs:251,233`
- Live -12 double-wire (`Resigned => continue` re-loops with re-signed bytes) — `stage_send.rs:1082`
- Four un-fenced chain-seed writers — `stage_offline_ack.rs:495`; `stage_send.rs:1809`; `boot_phase.rs:1814`; `offline_code_replenish.rs:267`
- Slice A: -4 typed as `DpsError::Indeterminate` distinct from timeout — `dto.rs:277-286`; `error.rs:67-75`
- Slice A′: TLS-proven `DpsError::RemoteStatus` auth seam — `error.rs:57-65`; `grpc.rs:35-37,152-162`
- Slice B migration 033/034: delivery_generation + authorized_generation + apply-state + integrity triggers (INACTIVE) — `033...sql:8-48,153-303`
- C-pure: total classifier + NodeEffect + `ObservedOutcomeV1` w/ node_effect + authorized_generation — `mod.rs:892-1034,1094-1193`
- reservation repo INACTIVE (no src/ caller) as spec's baseline predicts — `delivery_reservation.rs:9-15`
- Phased DAG-pin present & green (C-pure landed in prro-domain, no prro-dps-contract dep added) — `prro-domain/tests/rp_cs1_4_contract_dag.rs:142`

**DRIFTED**

| Claim | Spec says | Code does (file:line) | Verdict |
|---|---|---|---|
| hazard-4-collapse-drifted | "current hazard": `ErrorUnknown => DpsError::Transport` (-4 indistinguishable from timeout) | Slice A already shipped: `ErrorUnknown → DpsError::Indeterminate{code:-4,…}` (type IS distinct, positively pinned) — `dto.rs:277-286`. BUT `error_routing.rs:331` still projects Indeterminate → ErrorRetryable/TransientRetry ("compatibility projection"), so blind-resend persists until E | **CONFIRMED** — type-baseline drifted (spec's hazard snapshot is stale); behavioral double-issue hazard remains. Reconciler cleanly separated the two halves. |

**TO_BUILD**
- NS-3: kill the -12 loop (remove `Resigned=>continue` second wire; -12 → one wire, fence/RMR held) — `code_ref: none`
- Durable authorized_generation replay-CAS (stored-vs-live compare; stale observation drops) — `code_ref: none`
- Slice D: `authorize_submission` (RN→CALL_STARTED two-UPDATE CAS + generation bump + token-after-commit) + two-commit record-then-apply + crash-window resolution — `code_ref: none`
- Slice E: whole-fence enforcement — gate all 7 callers by reservation certainty, remove/guard `(ErrorRetryable,Sending)` under fence, forbid foreign seed advance / new issuance / offline-session under fence, legacy-cutover fail-closed RMR/HOLD — `code_ref: none`

---

## Top reconciliation actions for the dossier

Before the spec family becomes the D/E oracle, these **CONFIRMED** drifts must be written back
into the specs (the specs are stale relative to the improved 3.2 code — fix the spec text, not
the code):

1. **spec4b — RemoteStatusEvidence: remove `AuthenticatedPeerGarbage`.** Spec §3 still declares a 2-arm evidence enum with an SE-2 rule routing decode-failures to AuthenticatedPeerGarbage; shipped tonic collapses that path to `NoResponse` (single arm `RemoteAuthStatus` survives). Update §3 + SE-2. (`mod.rs:416`)
2. **spec4b — NoResponseCause 5th arm.** Add `CallFailedWithoutTrustedDpsEnvelope` (§4.3 branch 9) to the spec's frozen 4-arm set. (`mod.rs:391,406`)
3. **spec4b — SendResponse is opaque, not a public enum.** Re-state §3 as an opaque struct over a private inner enum with source-gated ctors + `kind()` view (Class-A sealing). (`mod.rs:435`) Same treatment for `SendOutcome`/`SendIndeterminate` sealing.
4. **spec4b — digest renamed / split.** `RawResponseDigest` → `DecodedResponseDigest` (honest decoded-content) + distinct `GrpcStatusDigest` for transport-status replies. Fix every algebra/evidence field name in §3/§5/§6. (`mod.rs:107,141`)
5. **spec4b — R4 RetryClass relocation is unbuilt (not just drifted).** RetryClass stays in `error_routing.rs:69`; no re-export / `From<ActiveRetryClass>` widening / `set_routing` API. Either descope R4 or carry it as an explicit D/E work item.
6. **spec2 §8 — classify is a read-only shadow.** The three fields are computed but **not recorded before the target_state collapse**; the live decision is still the legacy `route_send_result` collapse. Add the "3.2 read-only shadow, D/E wires it live" posture to §8 so the spec's §8 cut-point isn't read as already-realized. (`stage_send.rs:1573,1587`)
7. **keystone §1 — refresh the -4 "current hazard" snapshot.** Slice A shipped `DpsError::Indeterminate{-4}`; the residual hazard is the `error_routing.rs:331` compatibility projection (Indeterminate → TransientRetry → blind-resend), not the old `DpsError::Transport` collapse. Re-anchor the hazard to the routing layer, deferred to slice E. (`dto.rs:277-286`, `error_routing.rs:331`)

**Do NOT action (verifier REJECTED — reconciler was wrong, no real drift):** spec4b digest-source-gate;
spec2 three-orthogonal-fields, minus4-orthogonality, authorized-generation-token, authenticated-peer-garbage-removed;
spec1 payloaded-events-in-code, denial-not-conflict-rp2.

**Biggest TO_BUILD blocks** (the actual D/E build, all `code_ref: none`): the **raw-port / `submit_raw`**
(`prro-dps-contract` is still an empty CS-1d skeleton) → **`authorize_submission`** (RN→CALL_STARTED CAS +
generation bump + record-then-apply, keystone Slice D) → the live **delivery_reservation FSM + SubmittedUnknown
fence + crash-window resolution** (spec2; table/columns exist INACTIVE, no production caller) → **whole-fence
enforcement + NS-3 -12-loop kill** across the 7 callers and 4 seed-writers (keystone Slice E) → the pure
**`admission()` rule-list + data-driven transition matrix** (spec1 §3/§4).
