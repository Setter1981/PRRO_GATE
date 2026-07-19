# CS-3 — Double-issue keystone: implementation slice plan

**Status: PLAN rev 4 (2026-07-17) — the generation-replay soundness point-fix (Class-B: `authorized_generation`
must be durably stored, not re-derived from `node_state`) + the DAG-pin citation correction (Class-A). rev-3
closed the 5 original + 3 unowned invariants + 2 point-fixes. Auditor: "kickoff GO without a new full round" after
the generation fix.** Orchestrates the locked contracts into ordered, RED-first vertical slices. Grounded on
`origin/main`. **Contracts (do not re-derive):** Spec #4A (`2026-07-15-spec4-authority-minilock.md`, 🔒 LOCKED —
A4-1…A4-6 + RP-A4-1…6) · Spec #4B (`2026-07-16-spec4b-dps-contract.md`, auditor GO — the delivery types) ·
Spec #2 (delivery FSM) · migration 032 (`delivery_reservation`, INACTIVE).

> **⚠️ BASE-BRANCH.** CS-3 MUST be based on **`origin/main`** (has CS-2: 030/031/032 + `delivery_reservation`).
> The working branch `fuzzer-tier1-dossier` is **stale** (pre-CS-2). Ground every line number against `origin/main`
> (worktree isolation). **This plan is TO BE committed to `origin/main` as the oracle** (currently untracked on
> the stale branch — a `origin/main` worktree would not otherwise see it; committing is the §5 operational step).
>
> **⟶ STATUS 2026-07-19 (write-back):** DONE — this plan + the spec family are now **TRACKED on `origin/main`**
> (#285 `f2c17b1` spec1/spec2 · #311 `7ff0cf2` spec4b · #312 `b5c85e0` keystone); §5's "commit the oracle" + "#4B header → GO" + "#4A A4-6 amendment"
> operational steps are **executed**. THIS PR is the follow-up **drift write-back** (the 9 reconciliation-delta
> CONFIRMED drifts + the dossier-rev9 §2A refinements). The BASE-BRANCH / ground-on-`origin/main` guidance above
> still holds for implementers of the D/E slices.

## 0 · What CS-3 is + the north-star pins
CS-3 **activates** the INACTIVE `delivery_reservation` model so a **`SubmittedUnknown`** doc is **never
blind-resent** — killing double-issue. #4A locks the contract; #4B types the model; CS-3 is the activation.

**North-star (phase not done until ALL green):**
- **NS-1 = RP-A4-6 sharpened (fix 1): wire-count per document ≤ 1** — no matter how many times boot / drain /
  convergence run, a given `document_id` is sent to DPS **at most once**. This is the double-issue kill and it
  must hold across **all seven** `stage_send::run` callers + the fence-forbidden ops (§1), not just "redrive".
- **NS-2 = RP-A4-5: record-then-apply** — a crash after `OutcomeObserved` re-applies `ObservedOutcomeV1`
  idempotently; the **DPS wire-call is NEVER repeated**; **record and apply are TWO commit boundaries** (fix 2)
  — evidence commits durably first, the *repeatable* apply is a separate commit, so an apply failure can never
  roll back the evidence.
- **NS-3 (fix 3): the current `-12` loop is killed** — `-12 ERROR_BAD_HASH_PREV` yields **exactly one** wire-call,
  the signed bytes are **not** replaced, the fence / RMR is held. (The *corrective new-attempt* re-add is deferred,
  §4.)

## 1 · The current hazard (grounded on `origin/main`)
- **SEVEN production `stage_send::run` callers** (every one can drive a wire call): the happy-path
  `inline.rs:910`; the drains `backlog_drain.rs:1321` / `:2959`; the boot re-drives `boot_phase.rs:3072` / `:3685`
  / `:3943`; the online convergence `online_convergence.rs:561`. NS-1 must bound ALL of them to ≤1 per doc.
- **The re-send edge:** `fiscal_documents.rs:251 (ErrorRetryable, Sending)` (docstring `:233` "re-sends go
  ErrorRetryable → Sending") is a **live allowed transition** — the FSM currently *permits* the second wire.
  A `Sending` doc → CAS `Sending → ErrorRetryable` (`retry_class = TransientRetry`) → next tick
  `evaluate_er_redrive` (`er_redrive_policy.rs:86`) `Redrive` → `stage_send::run` again. **No `SubmittedUnknown`
  discriminant** exists (grep-empty); the only gate is `RetryClass`.
- **The live `-12` double-wire:** `stage_send.rs:1068-1069`
  `match mac_recovery::run_mac_recovery(...).await? { MacRecoveryOutcome::Resigned => continue, ...}` — the
  `continue` re-loops the send with **re-signed bytes** = a real second wire-call. NS-3 kills this.
- **`-4` collapse (CS-3 recon — TYPE-baseline REFRESHED; behavioral hazard remains):** Slice A **shipped** —
  `Status::ErrorUnknown => Err(DpsError::Indeterminate{ code: -4, .. })` (`dto.rs:277`), so `-4` is now a **typed
  `Indeterminate`, distinct from a bare timeout** (the old `dto.rs:215` `Transport` collapse is superseded). BUT
  `error_routing.rs:331` still projects `Indeterminate → ErrorRetryable / TransientRetry` (a documented
  "compatibility projection", audit-identical to `Transport`), so the **blind-resend hazard persists** until the
  fence / differentiation lands in **slice E**. The residual hazard is re-anchored to the routing layer, not the
  decode.
- **Chain-seed writers (must all respect the fence):** the seed-UPDATE fn `node_state::update_last_known_xml_sha_tx`
  (`node_state.rs:170`; sibling `seed_prevhash` `:137`) has **four** real callers (grounded, corrected):
  `offline_code_replenish.rs:267`, `boot_phase.rs:1814`, `stage_offline_ack.rs:495`, `stage_send.rs:1785`
  (NOT `stage_finalize` — docstrings only). NS-1 forbids a **foreign seed advance** at any of these four while an
  FN is fenced.
- **Recording:** `stage_send` 4-b — CAS `Sending → {Sent|Rejected|ErrorRetryable}` + `sfn` UPDATE (Sent only) +
  `transport_trace::complete_tx`; the `WireDecision::Sent` CAS is the **sole online-issuance moment** (seed + sfn +
  shift-edge atomic). **INV-1**: no net/crypto in the write tx — the 4-pre / 4-b split is the seam.
- **Absent (CS-3 introduces):** `delivery_reservation` has no `src/` caller (INACTIVE, correct);
  `node_state.delivery_generation`, `ObservedOutcomeV1` — do not exist.
- **The DAG-pin to update (fix 5, citation corrected):** `rust/prro-domain/tests/rp_cs1_4_contract_dag.rs` —
  `contract_crates_have_empty_direct_deps_phased` (**~:141-164**, NOT `:41` which is only the crate-name list)
  asserts each `*-contract` crate's direct non-dev/build dependency set is **∅ until specs #3-5 / CS-6**. The
  **first CS-3 slice that adds a real `[dependencies]` entry to `prro-dps-contract`** MUST update this phased pin
  in the same slice (not silently break it) — this is **C-pure** if it materialises the #4B wire-observation types
  in `prro-dps-contract` referencing `prro-domain` (likely, per #4B R3), otherwise **Bridge**. Implementer
  determines at the point of the first dep-add.

## 2 · The slices (RED-first; each vertical + independently-mergeable EXCEPT D+E — see order)

| # | Slice | Deliverable | RED-pin (author FIRST, watch it fail) | Risk | Deps |
|---|---|---|---|---|---|
| **B** | Migration 033 — activation schema | `node_state.delivery_generation INTEGER NOT NULL DEFAULT 0` + `active_delivery_reservation_id BLOB`; on `delivery_reservation`: **`authorized_generation INTEGER`** (immutable once `CALL_STARTED` — the durable snapshot the replay CAS compares, Class-B fix) + the apply-states `OUTCOME_RECORDED_PENDING_APPLY`/`APPLIED` + **the effect-discriminant column** (point-fix, see C) + rebuild CHECK/index/immutability-triggers; fail-fast on non-empty table. INACTIVE | RP-A4-3d/e (transition legality + no-INSERT-OR-REPLACE on new states); RP-A4-1 (generation present); `authorized_generation` immutable-after-set trigger; boot applies cleanly | LOW (migration-keeper) | — |
| **C-pure** | Typed classifier + `ObservedOutcomeV1` (pure) | #4B types in code + total `classify(evidence) → (certainty, provenance, routing)`; **`ObservedOutcomeV1` carries a durable semantic/effect discriminant** (fix 2) so `-11 Offline168 → node BLOCKED`, `-6 → operator`, etc. are reconstructable (the triple alone can't — Offline168 and other TerminalRejects share a triple) | **RP4B-2 graph-pin** — `{(evidence-discriminant, classify)} == normative graph` (catches Accepted↔Rejected swap); **effect-pin** — each terminal code's node-effect is recoverable from `ObservedOutcomeV1` alone | LOW (pure, off hot-path) | — |
| **C-DB** | Classifier ↔ real 033 roundtrip | every derivable `(certainty, provenance, routing)` + effect-discriminant round-trips the **real migration 033** CHECK matrix | RP4B-2 storage half (033-backed) | LOW | **B** |
| **A** | `-4` seam | `dto.rs:215` — surface `-4` as a typed **Indeterminate** (recommend a new `DpsError::Indeterminate` variant, propagated `route_send_result → WireDecision → 4-b`) → `Parsed(Indeterminate) ⇒ SubmittedUnknown`, distinct from a timeout | RP4B-3 (`-4` vs timeout distinct) | MED (transport decode) | C-pure |
| **A′** | RemoteStatus / AuthenticatedPeer split | `grpc.rs` seam so a WAF/garbage body from an authenticated peer surfaces as `RemoteStatus`, not `NoResponse` (#4B AM-2) — **explicitly ordered here**, was floating | RP4B-4 (auth-peer ⇒ ProbeRequired; incumbent-yields-NoResponse-until-seam pin) | MED (transport) | A |
| **Bridge** | Incumbent `DpsChannel → DpsSubmissionPort` + **static sole-caller gate** | adapt the live `DpsChannel` to `DpsSubmissionPort::submit_raw` (engine `submit_authorized` wrapper); update the phased DAG-pin (`contract_crates_have_empty_direct_deps_phased`, ~:141-164) IF C-pure did not already (see §1) | **static sole-caller gate** (source-level, like the CS-1 DAG pins — not prose): the ONLY production path to `send_chk` / `submit_raw` is via `submit_authorized`; + full-`binding`/`hash` check + **wrong-port ⇒ zero wire** (RP4B-5); the phased DAG-pin stays GREEN (updated by whichever slice added the dep, not silently removed) | MED | C-pure, **A, A′** |
| **D** | `authorize_submission` + record-then-apply + **token lifecycle** | **4-pre** (before wire): **`RN→CALL_STARTED` = TWO guarded `UPDATE`s in ONE `BEGIN IMMEDIATE`** (not a cross-table CAS): (1) `UPDATE node_state SET delivery_generation = delivery_generation+1, active_delivery_reservation_id = :rid` (guarded, `rows_affected == 1`); (2) `UPDATE delivery_reservation SET state='CALL_STARTED', call_started_at=:now, authorized_generation = (SELECT delivery_generation FROM node_state WHERE fiscal_number=:fn)` (guarded, `rows_affected == 1`); **rollback on any `rows_affected != 1`; the token returns ONLY after commit**. `authorized_generation` is **immutable** (frozen at CALL_STARTED, snapshot of the bumped node generation). **4-b two-commit**: (i) commit `ObservedOutcomeV1` (+ effect-discriminant + the **immutable `authorized_generation`**) as authority; (ii) a SEPARATE commit applies via a full-tuple CAS `{reservation_id ∧ **stored `authorized_generation` == current `node_state.delivery_generation`** ∧ binding ∧ envelope_hash}` — the comparison is the **stored** value vs the **current** node generation (NOT node-vs-node; closes the replay tautology). Match ⇒ doc CAS + seed + **fence-release ONLY for safe `NotSubmitted` / clean accept** (`SubmittedUnknown` & routed-`Submitted` STAY fenced) + clear pointer on release; **node-advanced (stored != current) ⇒ drop, ledger/seed/fence unchanged**; guard-fail ⇒ leave `OUTCOME_RECORDED_PENDING_APPLY`, fence held. **Sole-issuance CAS untouched**. **Crash-window:** a durable `CALL_STARTED` with no outcome ⇒ `CrashedBeforeObservation → SubmittedUnknown`, fence held, **no new wire** | RP-A4-5 (idempotent re-apply, wire never repeated); **RP4B-9** (a replayed observation whose stored `authorized_generation != current node generation` ⇒ dropped, ledger/seed/fence unchanged); **generation-replay pin** (outcome(G1) committed → crash → node bumped to G2 → boot apply **drops** (G1≠G2), ledger/seed/fence unchanged); **crash pin** (`CALL_STARTED`+no-outcome+reboot ⇒ `SubmittedUnknown`, fence held, zero new wire) + **dropped-future pin** (a dropped `submit` future never becomes `NotSubmitted`, SE-1); RP-A4-3a/b | **HIGH** (hot-path; INV-1) | B, C-DB, A, Bridge |
| **E** | **Whole-fence enforcement + kill blind-resend** | Gate **all 7** `stage_send::run` callers by the reservation certainty; **remove/guard the `(ErrorRetryable, Sending)` edge** under fence; **forbid under fence**: new issuance, offline-ack / offline-session / offline-code-replenish, **foreign seed advance** (the 4 writers, §1); **kill the `-12` loop** (NS-3); a boot-observed `CALL_STARTED` / `SubmittedUnknown` reservation routes to **read-only reconcile, never a resend** (the crash-window from D); **legacy-cutover** (fix 4): a reservation-less `SENDING`/`ERROR_RETRYABLE` doc is **fail-closed → RMR/HOLD** (never judged safe via `transport_trace`), OR a pre-deploy empty-in-flight gate | **NS-1 (wire-count ≤ 1)** + **NS-3 (`-12` one-wire)** + RP-A4-6 + RP-A4-2 (no resend/seed/fence reads `transport_trace`) | **HIGH** (recovery hot-path) | D |

**Order:** `C-pure ‖ B → C-DB → A → A′ → Bridge → D → E`. **D and E MUST ship in the same production release**
(fix 5): D creates reservations, E enforces the fence — D without E creates unenforced reservations; E without D
has nothing to enforce. Value (double-issue kill) lands at E.

## 2A · Normative D/E refinements (from dossier rev9 — write into the oracle)
These lift the two dossier-rev9 normative locks that belong in the spine: the durable-record **discriminant**
(C-pure) and the total **`ObservedOutcomeV1 → ApplyPlan`** matrix + its full-tuple pin (D). The token/fence
contract stays in §2's D/E slice rows (slices D, E); these are the pieces not yet in a spec.

### 2A.1 · `EvidenceDiscriminant` — the durable record must be LOSSLESS (C-pure; audit Bl1)
`ObservedOutcomeV1` (`mod.rs:1114`) stores `{certainty, provenance, routing, remote_correlation_id, node_effect,
authorized_generation}` — NOT the evidence leaf / `DpsReject` code / `ProbeReason`. Distinct outcomes
(`MissingStatus` / `CloseAmbiguous`−2 / `CloseAmbiguous`−15 / `OkButNoFiscalNumber` / TLS `RemoteStatus`) collapse
to the SAME tuple yet need different probe/audit, so `record → ApplyPlan` would be **undefined**. **LOCK:** add a
closed `evidence_discriminant` to `ObservedOutcomeV1` (the current `ProbeReason`, `error_routing.rs:242`, has only
3 variants and is NOT sufficient). Verbatim:
```rust
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
With it, `record → ApplyPlan` is a **total function** (boot-after-record reconstructs losslessly). This is the
concrete realization of the C-pure "durable semantic/effect discriminant" (§2 C-pure row).

### 2A.2 · `ObservedOutcomeV1 → ApplyPlan` — the total matrix (D; audit Bl3)
`ObservedOutcomeV1` carries no `target_state` / audit-plan / trace-completion / probe-semantics; the live
`RoutingDecision` has 7 effect fields (`error_routing.rs:53`). Before D, build the NORMATIVE total matrix over the
durable record — two grounded halves + a projection:

**(A) classifier leaf table — VERBATIM from `classify` (`mod.rs:893`); rows TOTAL over the graph:**

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

`routing_for_reject` per `DpsReject` — VERBATIM (`mod.rs:983`): `Verify / Type / Xml / XmlDate / XmlChk /
XmlZReport / OfflineId / Close` → `(TerminalReject, NoNodeEffect)`; `NotPrevZReport` → `(OperatorEscalation,
OperatorEscalation)`; `Offline168`(−11) → `(TerminalReject, NodeBlocked)`; `BadHashPrev`(−12) → `(MacRecovery,
MacReseedPending)`; `NotRegisteredRro` / `NotRegisteredSigner` → `(FnConfigError, FnConfigError)`.

**(B) ApplyPlan projection `(certainty, routing, node_effect) → 6 output dims`:**
- **`node_effect`** = the classifier's, verbatim (table A).
- **issuance effects (SPLIT — snapshot-aware, Bl3):** only for clean Accepted (`Submitted ∧ routing=None`), and
  per origin: **`SFN stamp`** fires **always** (on Accepted); **`seed advance`** only for **online-origin**
  (`offline_fiscal_no.is_none()`, `stage_send.rs:1771`); **`shift-confirm`** only for online-origin ∧ the shift
  doc/edge (`stage_send.rs:1829`). An offline-origin clean accept stamps SFN only (seed/shift stay owned by the
  offline path, not re-fired). NONE of these for any non-Accepted leaf.
- **`target_state` · `audit_event/severity` · `probe`** = **`route_send_result(DpsError, doc_type)` VERBATIM**
  (the behaviour-preserving oracle, `error_routing.rs:53`), with the **D2 rule**: a reject/indeterminate resolved
  **pre-SENT** takes its `route_send_result.target_state`; **post-SENT** (issued-but-unconfirmed) → **RMR** (seed
  already advanced, not rolled back). **Accepted → `DocState::Sent`** (`stage_send.rs:1720` — NOT terminal `Ack`).
- **`audit/trace`** — conditional completion: complete an EXISTING `transport_trace`; for a pre-wire refusal
  (`NotStarted`) where none exists, do NOT create one.
- **`fence`** (per the migration-035 predicate — Spec #2 §5): **RELEASE** (at `APPLIED`) iff `certainty ∈
  {clean-Accepted, NotSubmitted}`; **HELD** iff `certainty=SubmittedUnknown` OR `(Submitted ∧ routing≠None)`.

### 2A.3 · The ApplyPlan pair-graph pin (7D — Bl2; NEW work)
The 3.2 drift-pin is **coarse** — it compares only `{retry_class, node-Blocked}` (`grpc.rs:584`). The ApplyPlan pin
extends comparison to the **full 7 dimensions** `(target_state, retry_class, seed/SFN/shift, node_effect, audit,
probe, fence)` — `retry_class` (routing) an **explicit** dimension — and **ESTABLISHES the complete delta set**
(NOT assumed a-priori). Rule: **unchanged rows → exact-7-tuple-equal**; **declared cutover rows → their exact
`(incumbent, target)` 7-tuple**; **any other divergence the pin surfaces → adjudicate** (declare if intentional,
else RED) — so "no 4th delta" is a pin RESULT, not a claim. The target-side ProbeRequired **audit is LOCKED** to the
incumbent's `AuditEvent::StageSendProbeRequired / Warning` (`error_routing.rs:449`); each ProbeRequired leaf has a
**named `ProbeReason`** (keyed by `EvidenceDiscriminant`), added alongside the existing `DecodeUnknown` /
`Code2CloseShift` / `Code15CloseShift`. The **3 KNOWN deltas**:
1. **empty-id** — incumbent is a **`GuardAbort` / `NoApply` sentinel**: `EmptyServerFiscalNo` (`stage_send.rs:1589`)
   aborts BEFORE 4-b (doc stays `Sending` for W9, **no in-line ApplyPlan**). target = `OkButNoFiscalNumber`
   `(ErrorRetryable, ProbeRequired, none, ProbeRequired, StageSendProbeRequired/Warning,
   ProbeRequired[ProbeReason::OkButNoFiscalNumber], HELD)`.
2. **unknown non-zero** — incumbent `Decode` `(ErrorRetryable, ProbeRequired, none, None, StageSendDecodeUnknown/
   Warning, ProbeRequired[DecodeUnknown], HELD)` (`error_routing.rs:360`); target `UnknownStatus` `(ErrorRetryable,
   TransientRetry, none, None, StageSendTransientRetry/Warning, no-probe, HELD)` — **diverges on {retry_class,
   audit, probe}** (routing reverses).
3. **TLS `RemoteStatus`** — incumbent `(ErrorRetryable, TransientRetry, none, None, StageSendTransientRetry/Warning,
   no-probe, HELD)` (`error_routing.rs:314`); target `RemoteAuthStatus` `(ErrorRetryable, ProbeRequired, none,
   ProbeRequired, StageSendProbeRequired/Warning, ProbeRequired[ProbeReason::AuthenticatedPeerReply], HELD)` —
   **diverges on {retry_class, node_effect, audit, probe}**.

Declaring only #3 (as an earlier rev did) would RED on rows 1–2. **Replay/drop (separate):** a stale-generation
apply is a TOTAL no-op on ledger/seed/audit/fence (no release); a replay of an `APPLIED` row is idempotent.
**Boot-mint** of `SendResponse::no_response(CrashedBeforeObservation)` needs a **second allowed mapper** on the
digest-mint allowlist (`digest_mint_source_gate.rs:179`) — the authority gate today allows minting only inside
`shadow_map::map_send_reply`.

## 3 · Invariant & risk guards (every slice)
- **INV-1** no net/crypto in a write tx — reservation write (D, 4-pre) + record-then-apply (D, 4-b, two commits)
  are tx-local; the wire (4-a) stays between them.
- **INV-2** single-writer per FN — the reservation fence (`ux_reservation_active`) is the durable cross-crash
  extension of the lease; the two-boot-tick `Sending→ER→re-send` gap is closed by the fence (E).
- **A4-3 / D2** the `WireDecision::Sent` CAS (seed + sfn + shift edge) is the sole issuance moment — **D wraps it,
  never moves it**.
- **A4-6 / NS-2** two commit boundaries: evidence-as-authority first, repeatable apply second — an apply failure
  never rolls back evidence; only the ledger apply repeats, the wire never does.
- **NS-1 whole-fence** the fence forbids, per fenced FN: new issuance, offline-ack/session/code-replenish, and a
  **foreign seed advance** (the 4 writers §1: `offline_code_replenish` / `boot_phase` / `stage_offline_ack` /
  `stage_send`) — not merely the redrive path.
- **NS-3** `-12` is one wire-call, bytes unchanged, fence/RMR held — the `Resigned => continue` second-wire
  (`stage_send.rs:1068`) is removed.
- **legacy-cutover** at E-activation, an in-flight doc without a reservation is fail-closed (RMR/HOLD) or blocked
  by a pre-deploy empty gate — `transport_trace` is forensic-only and cannot certify it safe (A4-1).
- **token lifecycle (fix 1 + Class-B)** `RN→CALL_STARTED` = two guarded `UPDATE`s in one `BEGIN IMMEDIATE`
  (node-generation bump + pointer-set; reservation state + **immutable `authorized_generation` snapshot**),
  rollback on any `rows_affected != 1`, token only after commit. The apply CAS matches the full tuple
  `{reservation_id, stored `authorized_generation` == current `node_state.delivery_generation`, binding,
  envelope_hash}` — comparing the **durably-stored** snapshot vs the **live** node generation (never node-vs-node,
  which would be a tautology on replay); a stale observation is dropped, ledger/seed/fence unchanged (RP4B-9). The
  pointer clears only on a fence-releasing apply.
- **fence-release rule (point-fix)** the fence releases ONLY for a safe `NotSubmitted` (pre-call cancel) or a
  clean accept; `SubmittedUnknown` and a routed-`Submitted` (observed reject/degraded) STAY fenced (Spec #2 §5;
  032:127-133).
- **crash-window (fix 2)** a durable `CALL_STARTED` with no outcome ⇒ `SubmittedUnknown`, fence held, no new wire;
  a dropped `submit` future never becomes `NotSubmitted` (SE-1).

## 4 · Deliberately deferred (not CS-3)
- The **coordinator/actor** record-then-apply → CS-4 (A4-6; roadmap:44). CS-3 does the in-line two-commit form.
- The **`-12` corrective as a NEW attempt** (fresh reservation, new bytes/hash) + its **locked-spec amendment**
  (Spec #2 §5 / #4A A4-6) → CS-3 **follow-up**. NOTE the split: CS-3 **kills** the current unsafe `-12` re-send
  (NS-3); the *safe* corrective re-add is what's deferred, not the kill. [[project_spec4b_dps_contract_go]].
- `ingress_inbox.idempotency_strategy` → Spec #3. Concrete adapters + full crate-DAG → CS-6.

## 5 · Operational (do before/with the first slice)
- **Commit this plan to `origin/main`** as the CS-3 oracle (else a `origin/main` worktree can't see it) — **✅ DONE
  (#312 `b5c85e0`; see the header STATUS note).** Paired in **one docs PR** with:
  - the **#4B header fix**: the merged #4B still reads `DRAFT / CONDITIONAL-GO` though the body carries the final
    GO'd fix — correct the header to GO;
  - a **#4A A4-6 mini-amendment** (point-fix): the effect-discriminant formally extends A4-6's `ObservedOutcomeV1`
    ("three fields + `remote_correlation_id`") to ALSO carry a durable **effect/semantic discriminant** (so node
    effects like `-11 → BLOCKED`, `-6 → operator` are reconstructable from the payload alone). A one-paragraph
    amendment co-committed to the LOCKED #4A.
- **First slice:** `C-pure` (makes #4B executable; its graph-pin is the strongest single guard) ‖ `B` (migration,
  independent). Implementer: branch from `origin/main`, worktree-isolated, RED-first (watch each pin fail first).
