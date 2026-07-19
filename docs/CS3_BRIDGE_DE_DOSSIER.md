# CS-3 Bridge + D/E — next-stage dossier (shadow → load-bearing)

**Status:** PLANNING dossier, **rev 9** — external audit r1 + spot-rechecks #1–#7. Latest (#7): B3 Bl2
final — the pin is **7D** (`retry_class` an explicit dim, resolving the 6D/7D inconsistency); the
ProbeRequired audit LOCKED to `AuditEvent::StageSendProbeRequired/Warning` (`error_routing.rs:449`), not a
`probe-audit` placeholder; empty-id probe reason `ProbeReason::OkButNoFiscalNumber` named. Bl1 + all else CLOSED.
Builds on **CS-3 3.2 MERGED** (PR #329, squash `39e950ca`) — the read-only shadow foundation. NOT
greenlit for implementation. **This dossier DEFERS to the keystone-plan §2 slice table + §74/§75 for the
full token/fence contract; it does not re-state it** (an earlier rev drifted and over-simplified).
**Inputs (the oracle + evidence):**
- `docs/superpowers/specs/2026-07-17-cs3-double-issue-keystone-plan.md` — PLAN rev4, kickoff-GO — **the spine**.
- `2026-07-16-spec4b-dps-contract.md` (GO), `2026-07-14-spec2-delivery-reservation-fsm.md` (LOCKED),
  `2026-07-14-spec1-executable-transition-contract.md` (LOCKED) — the sub-contracts.
- `docs/CS3_BRIDGE_DE_TERRAIN_MAP.md` — grounded seam map (recon-grade file:line).
- `docs/CS3_SPEC_RECONCILIATION_DELTA.md` — the spec↔realized-3.2 delta (adversarially verified).

> **⟶ SUPERSEDED for the D/E fence + evidence + operator-recovery by `docs/CS3_REMEDIATION_DESIGN.md`
> (rev3, `DESIGN_SOUND / IMPLEMENTATION-NOT-YET-GATED`).** A model-decorrelated external re-audit rated this
> dossier's D/E design **SYSTEMIC** — it admitted **P2** (a doc can be wired twice) and **P4** (the durable
> record loses the accepted fiscal number on a crash) — and its `§3-D` permanent routing-fence **unsound**
> (a first transport blip bricks the FN with no operator exit). Rev3 corrects it with **NO new
> table/state/token**; the `§3-D`/`§3-E` fence-predicate + token detail below is **superseded**:
> - **P2** — a per-document lifetime **call-once** guard: `ux_delivery_document_ever_started ON
>   delivery_reservation(document_id) WHERE call_started_at IS NOT NULL` + a `NOT EXISTS` clause in
>   `authorize_submission`; a started-then-ambiguous attempt is **never re-wired**.
> - **Fence** — reduced to `state IN ('RESERVED_NOT_STARTED','CALL_STARTED') OR (state='OUTCOME_OBSERVED'
>   AND apply_state='PENDING_APPLY')` (no routing/certainty disjunct, no `seed_advanced` — proven dead).
>   **Unresolved** outcomes (SubmittedUnknown / `-12` / `-6`) stay `PENDING_APPLY` + flip the existing
>   **`STOP_MODE`**; **release** = the strengthened existing **`reset_stop_mode`** (operator completes
>   PENDING → APPLIED + `STOP_MODE → GOING_ONLINE`). Definitive seed-unchanged rejects RELEASE at APPLIED.
> - **P4** — `EvidenceDiscriminant` is **payload-carrying + durable** in four union columns on
>   `delivery_reservation` (`evidence_kind/text/code/digest`) with fail-closed matrix triggers + boot hydration.
> - **`Sent + NotFound`** → atomic doc-RMR + node-`STOP_MODE` + trace/audit (not a redrive).
>
> Full storage matrix, migration 035 DDL, the operator resolution matrix, and RED-pins are in
> `CS3_REMEDIATION_DESIGN.md` §2–§7. The `§3-D`/`§3-E` detail below is retained for history, NOT current design.

---

## §0 Where we are

3.2 landed the delivery **type contract** + the **read-only shadow**: `map_send_reply`
(`stage_send.rs:1573`) derives `RawSendReply → SendResponse` and binds it as `_shadow_response`, driving
**nothing**. The shadow **ENDS at `SendResponse`** — `classify` is NOT called on the production seam (only
in the drift-pin, `grpc.rs`); the live path separately runs `route_send_result` (`stage_send.rs:1587`).
Making the shadow **load-bearing** — it becomes the authoritative record that drives routing, and
blind-resend is prevented — is exactly **Bridge + D + E**. Everything below is scoped to that.

---

## §1 First action — commit the oracle + write back the confirmed drifts

The four specs are **UNTRACKED** (not on `main`). The keystone-plan itself says it is "TO BE
committed to origin/main as the oracle". **Step 1 of this stage: commit the spec family**, but FIRST
write back the **9 adversarially-CONFIRMED drifts** (per the reconciliation delta) so the oracle
matches reality:

- **spec4b (5):** `SendResponse` is **opaque struct, not a public enum**; `NoResponseCause` has a
  **5th arm** `CallFailedWithoutTrustedDpsEnvelope` (§4.3 branch 9); `RemoteStatusEvidence` has **only**
  `RemoteAuthStatus` (`AuthenticatedPeerGarbage` REMOVED); **`RetryClass` was NOT relocated** to
  `prro-domain` (it stays in `error_routing.rs:69` — no re-export / `From<ActiveRetryClass>` /
  `set_routing`); the digest is **honest DECODED-content** (`RawResponseDigest` → `DecodedResponseDigest`
  + `GrpcStatusDigest` split), not raw-wire. **NOT drifts (already REALIZED — do NOT "write back"):**
  `classify` taking no `doc_type`, and the `Rejected{digest}` / `MissingStatus` arms.
- **spec2 (3) + keystone (1):** reservation-FSM is shell-only; the crash-window mechanism; the read-only
  shadow; the `-4` hazard snapshot — reconcile the prose to the shipped code.

**Do NOT touch the 7 REJECTED drifts** (the verifier overturned them — faithful/design-locked places:
spec2's 3-orthogonal-fields, authorized-generation-token, `-4`; spec1's payloaded-events, denial-not-
conflict). They are correct as written.

---

## §2 Reconciliation → slice status (honest)

Reconciliation totals: **32 REALIZED · 9 DRIFTED-confirmed · 7 DRIFTED-rejected · 17 TO_BUILD**.
The 17 TO_BUILD are the whole `§6` raw-port / authorization / reservation-FSM / reconcile half — i.e.
Bridge + D + E. The type half (`SendResponse`/`SendOutcome`/`DpsReject`/`classify`/`SubmissionEvidence`)
is shipped.

**Slice order (keystone-plan §2, LOCKED):** `C-pure ‖ B → C-DB → A → A′ → Bridge → D → E`. Slice status
on **current `main`** (re-grounded — the keystone's `2dbae3c` baseline is STALE for A/A′):
- **`A` = the `-4` seam = REALIZED.** `Status::ErrorUnknown` surfaces `-4` as a typed
  `DpsError::Indeterminate` (`dto.rs:277`), NOT a `Transport` collapse (the keystone's `dto.rs:215`
  collapse is pre-`-4`-fix).
- **`A′` = the RemoteStatus / AuthenticatedPeer split = REALIZED for TLS-proven auth.** A TLS-proven
  `Unauthenticated`/`PermissionDenied` live-converts to `DpsError::RemoteStatus` (`grpc.rs:169`); the
  shadow yields `RemoteAuthStatus` (`grpc.rs:206`). An **arbitrary WAF/garbage body is NOT proven** and
  is out of scope (needs a custom codec — the 3.2 §7 non-goal).
- **`B` = migrations 032/033/034 (shipped INACTIVE); `C-pure` = classifier + `ObservedOutcomeV1`** — the
  TYPES shipped in 3.2, but `classify` is NOT wired on the prod seam and the total `ObservedOutcomeV1 →
  ApplyPlan` is missing (§3-D).

(Two earlier-rev errors, both corrected: A/A′ were first mislabelled "seed-fork / shift-wiring merged"
— unrelated older PR labels — then wrongly called "residual"; on current `main` they are REALIZED.)
**⚠️ Before Bridge, run a SLICE-STATUS AUDIT** confirming the two GENUINE residuals: `C-pure`
(classify prod-wiring + `ApplyPlan`) and `C-DB` (033 roundtrip). A/A′/B are realized.

**Decision required (not an audit item): `RetryClass` relocation (spec4b R4) is UNBUILT** — `RetryClass`
stays in `error_routing.rs:69`, with no re-export / `From<ActiveRetryClass>` / `set_routing`. Bridge/D
need a routing-store home, so this must be **decided**: either descope for CS-3, or carry it as a named
D/E work item feeding the routing store. (`admission()` + the spec1 §3/§4 data-driven matrix are the
oracle but **OUT OF SCOPE** this stage — noted so they are not silently assumed done.)

---

## §3 The build — Bridge → D → E

### Bridge — the transport port (adapter, not a cut)
- **Build:** `DpsSubmissionPort::submit_raw(BoundSignedEnvelope)` in `prro-dps-contract` (today an
  **empty skeleton**, `lib.rs:14`); `GrpcDpsChannel` also impls it; `DpsChannel` and the port **coexist**.
  **`submit_raw` returns a contract-owned RAW observation `{evidence, diagnostics}`, NOT a `SendResponse`
  (audit blocker 2):** a `SendResponse` for a server-code needs the **store-owned `doc_type`**
  (`shadow_map.rs:23`, `mod.rs:571`) which `RawSendReply` deliberately does not carry, and building it in
  the port would also **drop `WireDiagnostics`** (`raw_reply.rs`) needed for trace/audit. The live seam
  already yields `(legacy_result, RawSendObservation)` from ONE RPC/decode (`channel.rs:27`). The ENGINE —
  holding the token + the immutable `doc_type` — then runs `map_send_reply → classify → identity-attach`.
  **Type-home (LOCKED — not left to the implementer):** the **contract owns a PURE DTO `{evidence,
  diagnostics}`** with **NO `tonic`/`prost`/transport types**; the adapter converts its wire/decode result
  into that DTO (the transport-owned `RawSendObservation`, `raw_reply.rs:175`, **stays adapter-side** — a
  contract trait must not return an adapter type, that would create a back-dependency). If the contract
  DTO uses domain types, add an **explicit `prro-dps-contract → prro-domain` dep AND update the phased
  DAG-pin in the SAME slice** (`rp_cs1_4_contract_dag.rs:134`; contract deps are currently ∅,
  `Cargo.toml:15`) — never silently.
- **RED-pins:** **sole-caller gate** — the only production path to `send_chk`/`submit_raw` is via
  `submit_authorized` (source-level static pin, keystone RP4B-5); wrong-port ⇒ zero wire; contract-DAG pin
  update if `prro-dps-contract` gains a dep.
- **Invariant:** the wire call stays OUTSIDE any write-tx (INV-1) — Bridge only relocates the call site.

### D — the reservation + authoritative record (mint) — full contract in **keystone §74** (defer to it)
- **Schema ships INACTIVE, but D DOES author new DDL — migration 035 (audit blocker 1, reproduced).**
  032/033/034 (Slice B) create the two ORTHOGONAL columns `state ∈ {RESERVED_NOT_STARTED, CALL_STARTED,
  OUTCOME_OBSERVED}` (033:153) and `apply_state ∈ {NULL, PENDING_APPLY, APPLIED}` (033:181, set only at
  `OUTCOME_OBSERVED`, 033:224), and the 034 integrity triggers (incl. the `authorized_generation ↔
  call_started_at` pairing at **034:27**, NOT 033). **BUT the fence index `ux_reservation_active` (033:239)
  does NOT account for `apply_state`** — a **clean accept** (`OUTCOME_OBSERVED`, `routing_class NULL`,
  `apply_state=PENDING_APPLY`) falls OUT of the index, so the FN-fence drops BEFORE the ledger apply (the
  auditor reproduced a second same-FN reservation inserting). 033 itself says "CS-3 rebuilds this with
  PENDING_APPLY/APPLIED once activation wires the callers." → **D authors migration 035** rebuilding the
  **two DB objects** — the index `ux_reservation_active` (033:241) and the trigger
  `delivery_reservation_no_replace` (033:258) — around the NORMATIVE predicate below. The **Rust helper**
  `get_active_for_fn` (`delivery_reservation.rs:176` — NOT a DB object; SQLite has no stored fns) is
  updated to the SAME predicate **separately in the same D-slice** (plus the test-SQL copies + schema-pins;
  no 4th runtime object carries it).
- **NORMATIVE fence predicate (all three sites — index, trigger, Rust helper — share it verbatim):**
  ```sql
  state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
  OR ( state = 'OUTCOME_OBSERVED'
       AND ( apply_state = 'PENDING_APPLY'
          OR submission_certainty = 'SUBMITTED_UNKNOWN'
          OR (submission_certainty = 'SUBMITTED' AND routing_class IS NOT NULL) ) )
  ```
  i.e. the fence HOLDS through `OUTCOME_OBSERVED + PENDING_APPLY`, released **only after `APPLIED`** for a
  clean accept / safe `NotSubmitted` (`SubmittedUnknown` & routed-`Submitted` STAY fenced).
- **Token lifecycle + record-then-apply (keystone §74 — do NOT re-simplify):** 4-pre = **TWO guarded
  `UPDATE`s in ONE `BEGIN IMMEDIATE`** (bump `node_state.delivery_generation` + set
  `active_delivery_reservation_id`; set reservation `CALL_STARTED` + the `authorized_generation` snapshot),
  rollback on any `rows_affected != 1`, token returns **only after commit**; the engine-private
  `AuthorizedSubmission` token carries `{rid, generation, binding, hash, bytes}`. 4-b = TWO commits: (i)
  commit `ObservedOutcomeV1` + immutable `authorized_generation`; (ii) a SEPARATE full-tuple apply CAS
  `{rid ∧ stored authorized_generation == current node generation ∧ binding ∧ envelope_hash}` — match ⇒
  doc CAS + seed + fence-release (safe-`NotSubmitted`/clean-accept only) + `apply_state→APPLIED`;
  **node-advanced (stored != current) ⇒ drop, ledger/seed/fence unchanged**; guard-fail ⇒ leave PENDING,
  fence held. Include the **`RN→OO NotSubmitted`** branch. **Sole-issuance CAS (A4-3/D2) NOT moved.**
- **`ObservedOutcomeV1 → ApplyPlan` is MISSING and REQUIRED (audit blocker 3).** `ObservedOutcomeV1`
  (`mod.rs:1114`) carries certainty/provenance/routing/correlation/node_effect/generation but **NOT**
  `target_state`, audit-plan, trace-completion, or probe-semantics; the live `RoutingDecision` has **7**
  effect fields (`error_routing.rs:53`) and the drift-pin admits it does not compare `target_state`/full
  `node_effect` (`grpc.rs:584`). Before D, build the **NORMATIVE total matrix** `ObservedOutcomeV1 +
  immutable doc snapshot → ApplyPlan` (a **CS-3** deliverable — keystone:110 defers only the
  coordinator/actor to CS-4, NOT the apply semantics). It has **two grounded halves + a projection rule**,
  over a durable record that must be LOSSLESS:

  **(Bl1 precondition) The durable record must carry a discriminant.** `ObservedOutcomeV1` (`mod.rs:1114`)
  stores only `{certainty, provenance, routing, node_effect, remote_correlation_id, authorized_generation}`
  — NOT the evidence leaf / `DpsReject` code / `ProbeReason`. So distinct outcomes (`MissingStatus` /
  `CloseAmbiguous`−2 / `CloseAmbiguous`−15 / `OkButNoFiscalNumber` / TLS `RemoteStatus`) collapse to the
  SAME tuple yet need different probe/audit → the projection `record → ApplyPlan` is **undefined**.
  **LOCK:** add a **closed `evidence_discriminant`** to `ObservedOutcomeV1` — a NEW enum over the evidence
  LEAVES (the current `ProbeReason`, `error_routing.rs:242`, has only 3 variants and is NOT sufficient).
  Verbatim:
  ```
  enum EvidenceDiscriminant {
      PreconditionFailed,
      SigningFailed,
      NoResponse(NoResponseCause),
      RemoteAuthStatus,
      Accepted,
      Rejected(DpsReject),
      UnknownStatus(i32),
      SaveError,
      CloseAmbiguous(CloseAmbiguousCode),
      MissingStatus,
      OkButNoFiscalNumber,
  }
  enum CloseAmbiguousCode { Code2, Code15 }
  ```
  Any extra data a concrete audit/probe effect needs rides in that variant's payload. With it,
  `record → ApplyPlan` is a **total function** (boot-after-record reconstructs losslessly). (Alternative:
  compute+persist the full ApplyPlan at 4-b-i; the discriminant is chosen — smaller, composes with
  `node_effect`.)

  **(A) The classifier leaf table — VERBATIM from `classify` (`mod.rs:893`); rows are TOTAL over the graph:**

  | evidence leaf | certainty | provenance | routing (`ActiveRetryClass`) | `node_effect` |
  |---|---|---|---|---|
  | NotStarted / `PreconditionFailed` | NotSubmitted | NoResponse | TransientRetry | NoNodeEffect |
  | NotStarted / `SigningFailed` | NotSubmitted | NoResponse | **WrapperBug** | **WrapperBug** |
  | Started / NoResponse (any cause) | SubmittedUnknown | NoResponse | TransientRetry | NoNodeEffect |
  | Started / RemoteStatus | SubmittedUnknown | AuthenticatedPeer | ProbeRequired | ProbeRequired |
  | Started / Parsed / **Accepted** | Submitted | ParsedDpsEnvelope | (none) | NoNodeEffect |
  | Started / Parsed / Rejected(code) | Submitted | ParsedDpsEnvelope | `routing_for_reject(code)` ↓ | per code ↓ |
  | Started / Parsed / Indeterminate(`UnknownStatus`/`SaveError`) | SubmittedUnknown | ParsedDpsEnvelope | TransientRetry | NoNodeEffect |
  | Started / Parsed / Indeterminate(`CloseAmbiguous`/`MissingStatus`/`OkButNoFiscalNumber`) | SubmittedUnknown | ParsedDpsEnvelope | ProbeRequired | ProbeRequired |

  **`routing_for_reject` per `DpsReject` — VERBATIM (`mod.rs:983`):** `Verify/Type/Xml/XmlDate/XmlChk/
  XmlZReport/OfflineId/Close` → `(TerminalReject, NoNodeEffect)`; `NotPrevZReport` → `(OperatorEscalation,
  OperatorEscalation)`; `Offline168`(−11) → `(TerminalReject, NodeBlocked)`; `BadHashPrev`(−12) →
  `(MacRecovery, MacReseedPending)`; `NotRegisteredRro`/`NotRegisteredSigner` → `(FnConfigError,
  FnConfigError)`.

  **(B) The ApplyPlan projection `(certainty, routing, node_effect) → 6 output dims`:**
  - **`node_effect`** = the classifier's, verbatim (table A).
  - **issuance effects (SPLIT — snapshot-aware, Bl3):** only for clean Accepted (`Submitted ∧ routing=None`),
    and even then per origin: **`SFN stamp`** fires **always** (on Accepted); **`seed advance`** only for
    **online-origin** (`offline_fiscal_no.is_none()`, `stage_send.rs:1771`); **`shift-confirm`** only for
    online-origin ∧ the shift doc/edge (`confirm_shift_edge`, `stage_send.rs:1829`). An **offline-origin**
    clean accept stamps SFN only — seed/shift stay owned by the existing offline path, **not re-fired**.
    NONE of these for any non-Accepted leaf.
  - **`target_state` · `audit_event/severity` · `probe`** = **`route_send_result(DpsError, doc_type)` VERBATIM**
    (the behaviour-preserving oracle, `error_routing.rs:53`), NOT restated here — with the **D2 rule**: a
    reject/indeterminate resolved **pre-SENT** takes its `route_send_result.target_state`; **post-SENT**
    (issued-but-unconfirmed) → `RMR` (seed already advanced, not rolled back). **Accepted → `DocState::Sent`**
    (`stage_send.rs:1720` — NOT terminal `Ack`; DPS "Ack" means a fresh `Sent`, `stage_send.rs:1819`).
  - **`audit/trace`** — **conditional completion**: complete an EXISTING `transport_trace`; for a pre-wire
    refusal (`NotStarted`) where none exists, do NOT create one.
  - **`fence`** (per the migration-035 predicate above): **RELEASE** (at `APPLIED`) iff `certainty ∈
    {clean-Accepted, NotSubmitted}`; **HELD** iff `certainty=SubmittedUnknown` OR `(Submitted ∧ routing≠None)`.

  **Graph-pin (Bl2 — old→new PAIR graph, 7D):** the existing 3.2 drift-pin is **coarse** — it compares only
  `{retry_class, node-Blocked}` (`grpc.rs:584`) and declares 3 deltas at that granularity (`grpc.rs:759`).
  The ApplyPlan pin is **NEW work**: it extends comparison to the **full 7 dimensions**
  `(target_state, retry_class, seed/SFN/shift, node_effect, audit, probe, fence)` — `retry_class` (routing)
  is an **explicit** dimension so the divergences below name real columns — and **ESTABLISHES the complete
  delta set** (NOT assumed a-priori). Rule: **unchanged rows → exact-7-tuple-equal**; **declared cutover rows
  → their exact `(incumbent, target)` 7-tuple**; **any other divergence the pin surfaces → adjudicate
  (declare if intentional, else RED)** — so "no 4th delta" is a pin RESULT, not a dossier claim. The
  target-side ProbeRequired **audit is LOCKED** to the incumbent's `AuditEvent::StageSendProbeRequired /
  Warning` (`error_routing.rs:449`), and each ProbeRequired leaf has a **named `ProbeReason`** (keyed by the
  `EvidenceDiscriminant`). The 3 KNOWN deltas:
  1. **empty-id** — incumbent = a **`GuardAbort`/`NoApply` sentinel**: `EmptyServerFiscalNo` (`stage_send.rs:1589`)
     aborts BEFORE 4-b (doc stays `Sending` for W9, **no in-line ApplyPlan**) — the incumbent tuple is this
     sentinel. target = `OkButNoFiscalNumber` `(ErrorRetryable, ProbeRequired, none, ProbeRequired,
     StageSendProbeRequired/Warning, ProbeRequired[`ProbeReason::OkButNoFiscalNumber`], HELD)`.
  2. **unknown non-zero** — incumbent `Decode` `(ErrorRetryable, ProbeRequired, none, None,
     StageSendDecodeUnknown/Warning, ProbeRequired[DecodeUnknown], HELD)` (`error_routing.rs:360`); target
     `UnknownStatus` `(ErrorRetryable, TransientRetry, none, None, StageSendTransientRetry/Warning, no-probe,
     HELD)` — **diverges on {retry_class, audit, probe}** (routing reverses).
  3. **TLS `RemoteStatus`** — incumbent `(ErrorRetryable, TransientRetry, none, None,
     StageSendTransientRetry/Warning, no-probe, HELD)` (`error_routing.rs:314`); target `RemoteAuthStatus`
     `(ErrorRetryable, ProbeRequired, none, ProbeRequired, StageSendProbeRequired/Warning,
     ProbeRequired[`ProbeReason::AuthenticatedPeerReply`], HELD)` — **diverges on {retry_class, node_effect,
     audit, probe}**.

  `ProbeReason` gains one variant per ProbeRequired leaf (`AuthenticatedPeerReply`, `OkButNoFiscalNumber`, …
  keyed by the discriminant), alongside the existing `DecodeUnknown`/`Code2CloseShift`/`Code15CloseShift`.
  Declaring only #3, as an earlier rev did, would RED on rows 1–2.
  **Replay/drop (separate):** a stale-generation apply is a TOTAL no-op on ledger/seed/audit/fence (no
  release); a replay of an `APPLIED` row is idempotent. This removes implementer invention: every cell is
  either transcribed from `classify`/`route_send_result` verbatim or a stated structural rule, and the
  full-tuple pin makes it total.
- **Boot-mint needs a SECOND allowed mapper (Class-A):** minting `SendResponse::no_response(
  CrashedBeforeObservation)` at boot violates the authority gate, which allows it **only** inside
  `shadow_map::map_send_reply` (`digest_mint_source_gate.rs:179`) — add the boot mapper to the allowlist.
- **RED-pins:** the `AuthorizedSubmission` token has **no pub ctor = a real Rust seal** (compile-fail); the
  public `submit_raw` sole-caller is a **source-policy/review gate, NOT a Rust type** — keep the two claims
  **separate** (audit Major; fine under the trusted-dev model); stale-generation apply dropped; immutability
  trigger RED; **fence-held-through-`PENDING_APPLY`** (revert migration 035 → second same-FN reservation
  inserts → RED — the auditor's reproduced canary).
- **INV-1:** 4-pre + both record/apply commits are DB-only; the wire (4-a) is strictly between committed
  tx boundaries.

### E — WHOLE-FN fence enforcement + kill blind-resend (inseparable from D) — keystone §75
- **The fence is FN-WIDE, not doc-local (audit blocker 5).** Locked Spec #2 (§67) forbids, for the whole
  FN under fence: new issuance, offline-session, and seed advance. So E must: **gate all 7
  `stage_send::run` callers** by reservation certainty; **remove/guard the `(ErrorRetryable → Sending)`
  edge**; **stop the 4 seed-writers** (`offline_code_replenish` / `boot_phase` / `stage_offline_ack` /
  the online seed-UPDATE, §1); **block offline-ack / offline-session / offline-code-replenish**.
- **NS-3: kill the `-12` loop — the short-circuit must be BEFORE `run_mac_recovery` (audit blocker 4).**
  Removing the `Resigned => continue` (`stage_send.rs:1082`) is **too late**: by then `run_mac_recovery`
  (`stage_send.rs:1081`) has ALREADY burnt `mac_recovery_attempts`, re-signed, and overwritten
  `previous_hash` / `unsigned_xml_sha256` / `PAYLOAD_XML` / `SIGNED_XML` (`mac_recovery.rs:410/516`; proven
  by `run_mac_recovery_happy_resigned_persists_atomic_four_write`). The MacRecovery short-circuit must gate
  **before** the orchestrator call.
- **Legacy cutover is an E-DELIVERABLE, not a "risk" (audit blocker 5):** a reservation-less
  `SENDING`/`ERROR_RETRYABLE` doc is **fail-closed → RMR/HOLD** (never judged safe via `transport_trace`),
  OR a pre-deploy empty-in-flight gate.
- **Boot scanner** — the `CrashedBeforeObservation` consumer: `state='CALL_STARTED'` (no outcome) →
  read-only reconcile, never resend (pattern: `close_orphan_transport_traces`, `boot_phase.rs:1553`).
  (`validate_reconcile` = CS-6/Bridge, NOT E — absent from the keystone E-row.)
- **RED-pins (keystone):** **NS-1** wire-count ≤ 1 per intent; **NS-3** `-12` = one wire AND
  `counter/previous_hash/XML/CMS/envelope_hash` UNCHANGED (not just one-RPC); RP-A4-2 (no resend/seed/fence
  reads `transport_trace`); boot `CALL_STARTED` → 0 new `send_chk`; cutover reservation-less → RMR/HOLD.
- **RELEASE CONSTRAINT (hard):** **D and E ship in the SAME production release.**

---

## §4 Invariant guards (frozen list)
- **#1 no net/crypto in write-tx** — every new tx boundary (4-pre, 4-b-i, 4-b-ii) is DB-only; the wire
  (4-a) sits strictly between committed boundaries. Pin: the W3 scanner
  (`with_immediate_no_foreign_io.rs`) is a method-NAME **denylist** (`SUBSTRATE_METHODS`), and the new
  `submit_raw`/`submit_authorized` names are **absent** → the pin is **vacuously green** until the Bridge
  slice **adds them to `SUBSTRATE_METHODS` in the same slice** AND adds a **positive-control snippet**
  (like `case_1`) proving a `submit_authorized` call inside a `with_immediate` closure is caught. Without
  both, INV-1 is the exact CS-1 denylist-not-allowlist hole.
- **#2 single-writer / #8 recovery** — the reservation fence IS the single-writer marker across a
  crash; recovery routes through reconcile/RMR, never a silent state jump or blind resend.
- **#192-class projection** — no doc rests in a non-terminal state, and no reservation rests
  `CALL_STARTED` with a terminal doc, at a quiescent boundary; boot reconciles reservation↔doc.

## §5 Top risks (from the terrain map)
1. **INV-1** — the record-then-apply split must not re-enter the wire. CRITICAL.
2. **Sole-issuance CAS must not move** — wrap, don't reorder; a crash between issuance and
   reservation-apply must be boot-detectable.
3. **Legacy cutover** — promoted to an **E-deliverable + RED-pin** (§3-E), not just a risk:
   reservation-less in-flight `Sending`/`ErrorRetryable` docs fail-closed → RMR/HOLD at cutover.

## §6 Fuzzer-impact (rule: new feature → extend the fuzzer)
The alphabet must gain: `delivery_reservation` transitions — `state ∈ {RESERVED_NOT_STARTED,
CALL_STARTED, OUTCOME_OBSERVED}` × `apply_state ∈ {NULL, PENDING_APPLY, APPLIED}`; a **crash injected
between 4-b-i record and 4-b-ii apply** (boot must reconcile, not double-issue); a **blind-resend
attempt** on a fenced `SubmittedUnknown` (oracle: 0 new `send_chk`); the `-12` path post-NS-3 (exactly
one wire, no attempt #2). Reuse `assert_no_resend` / `assert_crash_send_recovery` oracles.

## §7 First concrete steps (ordered)
1. **Commit the spec family** to `main` as the oracle, after writing back the 9 confirmed drifts (§1).
2. **Slice-status audit** (§2) — confirm C-pure/C-DB/B residuals against the keystone-plan.
3. **Bridge** — `DpsSubmissionPort` + sole-caller gate.
4. **D + E together (one release)** — build `ObservedOutcomeV1 → ApplyPlan` (C-pure/D) + migration 035
   (fence-through-`PENDING_APPLY`) + the full token lifecycle (keystone §74) + FN-wide fence +
   NS-3-before-orchestrator + legacy-cutover (keystone §75).

Each of 3/4 is spec-first (the LOCKED specs are the oracle) → RED-pins → minimal-diff implement →
adversarial review, per the project charter.
