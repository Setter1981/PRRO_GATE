# Spec #4B — DPS boundary contract (rev 2 — the TYPE realization of locked Spec #2)

**Status: DRAFT rev 2 — full rework after the professional NOT-YET (23 findings).** Grounded on `origin/main`
`a97bf76`. rev 1 was **withdrawn**: it collapsed the three orthogonal axes into one enum (violating locked
**Spec #2** §2), grounded §1 on dead types (`DpsOutcome::RetryablePending` is never constructed), missed the
`AUTHENTICATED_PEER` provenance value and the entire `routing_class` dimension (3/5 of its table rows are
rejected by the real 032 CHECKs), and asserted an unverified macro claim. rev 2 **does not re-invent** — it is
the **contract-type realization of the already-locked Spec #2 model**, verified against the live seam + the
real migration-032 CHECK matrix.

**What #4B is (and is NOT).**
- **#4B** = the **Rust type/trait contract** that encodes Spec #2's semantics so its invariants are
  *structural*: `SubmissionEvidence`, the three orthogonal axis types, `AttemptObservation`, the `DpsPort`
  capability-split, `DpsProtocolBinding` + registry, one-shot `SubmissionPermit`. Home: `prro-dps-contract`
  (traits/wire) + `prro-domain` (pure value types — plan line 245, *no second copy*).
- **NOT #4B — CS-3:** the classifier *logic* `DpsError+evidence → the three fields` (the collapse cut point
  is `inline_map.rs:394`, Spec #2 §8), the 032 activation, `ObservedOutcomeV1`, and the **minimal incumbent
  gRPC seam** to stop losing `-4` at `dto.rs:170` (Spec #2 §1; audit V04 — this MUST be in CS-3, not deferred
  to CS-6, else `-4` provenance is physically unrecoverable).
- **NOT #4B — CS-6:** the concrete `prro-dps-grpc` / `prro-dps-protocol2` adapters + the crate-DAG gate.
- **NOT #4B — already locked:** the delivery_reservation schema + authority (#4A); the FSM/fence/anti-mask
  *semantics* (**Spec #2** — #4B only gives them types); the ingress inbox (**Spec #3**).

---

## 1 · Grounded reality (live seam — verified, not the rev-1 dead types)
- **Live send seam:** `DpsChannel::send_chk(CheckEnvelope) -> Result<CheckAck, DpsError>` (`channel.rs:24`).
  Success = **`CheckAck`** (`dto.rs:66`: `{ id: String, id_sign, data_sign }`, `MIN_KVT1_DATA_SIGN_LEN=64`).
  Failure = **`DpsError`** (`error.rs:15`) — and a `-4`/`Unknown` is mapped to a typed `DpsError` at
  `dto.rs:170` **before any classifier**, collapsing parsed-`-4` with timeout/reset (Spec #2 §1). `DpsOutcome`
  / `RetryablePending` (`outgress.rs:85/110`) is **planned/partly-dead** (`RetryablePending` never
  constructed) — do NOT ground on it.
- **`DpsChannel` full surface** (`channel.rs`): `send_chk` · `last_chk(&CheckSignBlob)` · `ping` ·
  `status_rro` · `info_rro` · `ask_offline_codes` · `by_server_fiscal_no` (default) · `query_by_local_identity`
  (**carried as a typed `Err` — the capability the current DPS LACKS**, Spec #2 §5). This is why #4B must be a
  **capability-split**, not a one-method `submit`.
- **`RoutingPolicy` already exists** = `RetryClass` (`error_routing.rs:69`), the **8 literals**: `TerminalReject
  · TransientRetry · FnConfigError · WrapperBug · ProbeRequired · MacRecovery · OperatorEscalation ·
  DrainChainSettleRetry` (legacy-decode-only). Spec #2 §2c: *unchanged authority*. #4B **reuses** it, does not
  redefine.
- **`SubmissionCertainty` / `ResponseProvenance` / `ReconciliationCapability` do NOT exist yet** (grep-empty) —
  #4B DEFINES them (Spec #2 §2/§5 names them).
- **The 032 CHECK matrix is the storage-side truth** (verified on SQLite). `submission_certainty ∈
  {NOT_SUBMITTED, SUBMITTED_UNKNOWN, SUBMITTED}` (`032:92`); `response_provenance ∈ {NO_RESPONSE,
  AUTHENTICATED_PEER, PARSED_DPS_ENVELOPE}` (`032:93` — **THREE**); `routing_class` = a `RetryClass` literal.
  Key CHECKs: `NOT_SUBMITTED ⇒ call_started_at NULL ∧ NO_RESPONSE` (`:107`); `{UNKNOWN,SUBMITTED} ⇒
  call_started_at NOT NULL` (`:108`); `SUBMITTED ⇒ PARSED_DPS_ENVELOPE` (`:109`); `routing NULL ∨ state≠OO ∨
  certainty=SUBMITTED` (`:110`); response-derived routing `{TerminalReject,FnConfigError,MacRecovery,
  OperatorEscalation} ⇒ SUBMITTED ∧ PARSED_DPS_ENVELOPE` (`:113`); `ProbeRequired ⇒ provenance≠NO_RESPONSE`
  (`:114`).

## 2 · The three orthogonal axes (Spec #2 §2 — NEVER one collapsed enum)
```rust
// (a) prro-domain — did it reach DPS?
enum SubmissionCertainty { NotSubmitted, SubmittedUnknown, Submitted }
// (b) prro-domain — what did the far side show? ONLY ParsedDpsEnvelope is DPS forward-progress.
enum ResponseProvenance { NoResponse, AuthenticatedPeer, ParsedDpsEnvelope }
// (c) reuse the existing authority — the 8 RetryClass (error_routing.rs)
use crate::write_path::error_routing::RetryClass; // RoutingPolicy
```
These are **independent** — `-4` = `{SubmittedUnknown, ParsedDpsEnvelope, TransientRetry}` (real DPS response
observed → liveness authoritative; submit result unknown → neither resend nor arm-offline). A single
`DeliveryOutcome` is **forbidden as an authoritative dual** (Spec #2 §2, audit V01/V03); a *derived*
control-flow projection is allowed only downstream of the three durable fields (Spec #2 §8 cut point).

**Storage duality — the CHECK-accepted `(certainty, provenance, routing)` triples (empirically valid on 032):**
| Case | certainty | provenance | routing_class | fence |
|---|---|---|---|---|
| safe pre-call cancel | NOT_SUBMITTED | NO_RESPONSE | TransientRetry | released |
| wire timeout after CallStarted | SUBMITTED_UNKNOWN | NO_RESPONSE | TransientRetry | **HELD** |
| parsed `-4` | SUBMITTED_UNKNOWN | PARSED_DPS_ENVELOPE | TransientRetry | **HELD** |
| WAF / garbage from an authenticated peer | SUBMITTED_UNKNOWN | AUTHENTICATED_PEER | ProbeRequired | **HELD** |
| clean accept | SUBMITTED | PARSED_DPS_ENVELOPE | *(NULL)* | released |
| observed DPS reject | SUBMITTED | PARSED_DPS_ENVELOPE | TerminalReject | **HELD** |

(No `*/NO_RESPONSE/NULL` at OUTCOME_OBSERVED — `:110` forbids it; the rev-1 table's 3 such rows were rejects.)

## 3 · `SubmissionEvidence` — private algebra, illegal states unrepresentable (audit V07/V09/V15)
The port's output — the *raw material* to derive the three fields **without guessing**; NOT the classified
fields (that is CS-3). Private fields + closed constructors so an adapter cannot fabricate a clean accept.
```rust
struct SubmissionEvidence {                      // prro-domain, pure
    initiation: CallInitiationCertainty,         // epistemic name (audit V09): NOT "fact"
    response: ResponseEvidence,
    binding: DpsProtocolBinding,                 // §7 — immutable, snapshot at reservation
    envelope_hash: [u8; 32],
}
// Authority = the durable FSM marker, NOT the transport (Spec #2 §3; audit V09/O-3):
enum CallInitiationCertainty { DefinitelyNotStarted, MayHaveStarted }
//   DefinitelyNotStarted admissible ONLY at Preflight (before the durable CallStarted marker) OR a
//   protocol-proven pre-handler refusal. After the marker: MayHaveStarted (a transport MUST NOT downgrade
//   to DefinitelyNotStarted on a bare error status — Spec #2 §3).
enum ResponseEvidence {
    NoResponse(TransportFailure),                // typed cause — routing must be derivable (audit V08)
    AuthenticatedPeerGarbage(RawResponseDigest), // TLS peer, not a DPS envelope (Spec #2 §6 anti-mask)
    Parsed(ParsedDpsEnvelope),
}
enum ParsedDpsEnvelope { Accepted(FiscalAcceptance), Rejected(DpsReject) }  // §5
// A protocol-neutral failure cause so the classifier can pick exactly one RetryClass (audit V08):
enum TransportFailure { TransientTransport, LocalConfiguration, WrapperInvariant, AuthenticationTransport, ProtocolDecode, Cancelled, Timeout }
```
**SE-1 (fail-safe):** a port that cannot prove `DefinitelyNotStarted` MUST report `MayHaveStarted` — never the
reverse. **SE-2:** `SubmissionEvidence` is *observed*, never assumed; there is no public constructor for a
clean accept without a `FiscalAcceptance` (§5).

## 4 · `AttemptObservation` — bind evidence to reservation + generation (audit V06)
`envelope_hash` identifies bytes, **not a call** — the same signed envelope may span attempts. A late
attempt-1 callback must not apply to attempt-2 / lift its fence.
```rust
struct AttemptObservation { reservation_id: ReservationId, generation: DeliveryGeneration, evidence: SubmissionEvidence }
```
**AO-1:** the store applies an `AttemptObservation` only via a **CAS on the full `{reservation_id, generation,
binding, envelope_hash}`** (Spec #2 §3; a stale-generation observation is dropped, never applied). `attempt_no`
need not be duplicated in the value type (the store owns the `(document_id, attempt_no)` tuple), but
`reservation_id + generation` are mandatory.

## 5 · Fiscal acceptance / reject algebra (audit V15 — `accepted: bool` is insufficient)
Grounded on the live `CheckAck` (KVT1 vs KVT2 vs EVPZ) + `DpsError`:
```rust
enum FiscalAcceptance {                          // constructed ONLY with a non-empty fiscal number
    FscoKvt1 { fiscal_number: NonEmptyFiscalNumber, id_sign_len_ok: () /* ≥ MIN_KVT1_DATA_SIGN_LEN */ },
    FscoKvt2 { fiscal_number: NonEmptyFiscalNumber, kvt2_hash: [u8; 32] },
    EvpzFinal { fiscal_number: NonEmptyFiscalNumber },
}
enum DpsReject { Known { code: DpsCode, message: Option<BoundedText> }, Unknown { raw_code: BoundedText, digest: RawResponseDigest } }
```
**FA-1:** `FiscalAcceptance` has **no** constructor accepting an empty fiscal number — preserving the live
production guard `stage_send.rs:1565` (empty `server_fiscal_no` refused) **structurally**. **FA-2:** only a
`FscoKvt2` / a *second* KVT for an already-applied doc must NOT re-apply the ledger effect (fence + generation
gate this, §4). Peer-controlled strings (`message`, `raw_code`, `remote_correlation_id`) are **`BoundedText` /
a digest**, never unbounded response TEXT (audit V18 DoS).

## 6 · `DpsPort` capability-split + one-shot permit (audit V05/V11/V20)
A single `submit` is insufficient (reconciliation/status have no surface) and lets any code re-hit the wire.
```rust
#[async_trait] trait DpsSubmissionPort: Send + Sync {
    fn binding(&self) -> &DpsProtocolBinding;
    // Consumes a one-shot permit + a protocol-bound envelope → an observation. NO resend affordance.
    async fn submit(&self, permit: SubmissionPermit, envelope: BoundSignedEnvelope) -> AttemptObservation;
}
#[async_trait] trait DpsReconciliationPort: Send + Sync { fn capability(&self) -> ReconciliationCapability; /* last_chk / by_server_fiscal_no; query_by_local_identity iff capability proves it */ }
#[async_trait] trait DpsStatusPort: Send + Sync { /* status_rro / info_rro / ping */ }
#[async_trait] trait DpsOfflineCodesPort: Send + Sync { /* ask_offline_codes */ }
struct SubmissionPermit(/* opaque; issued ONLY for a fenced ReservedNotStarted attempt */);
struct BoundSignedEnvelope { binding: DpsProtocolBinding, hash: [u8; 32], bytes: SecretBytes }  // closed ctor
enum ReconciliationCapability { None /* ⇒ immediate RMR behind the fence, Spec #2 §5 */, ByServerFiscalNo, ByLocalIdentity }
```
- **DP-1 (evidence, not verdict):** ports yield `SubmissionEvidence`/`AttemptObservation` — never a
  `DocState` / `RetryClass` / `SendDisposition` (classification is CS-3). No `DpsPort` method names those.
- **DP-2 (untypable blind-resend, audit V05):** `submit` **consumes** a `SubmissionPermit`; a `SubmittedUnknown`
  yields a `ReconciliationTicket` from which a `SubmissionPermit` **cannot be constructed**. RP is a
  **compile-fail** (`ReconciliationTicket: !Into<SubmissionPermit>`), not a grep for `resend`.
- **DP-3 (bound bytes, audit V20):** `submit` takes only a `BoundSignedEnvelope` whose `binding == self.binding()`;
  cross-protocol bytes are unconstructable for the wrong port.
- **DP-4 (one protocol per port):** `binding()` is constant; evidence carries `binding` (§3).
- **DP-5 (engine ∉ adapter):** `prro-engine` names these traits / `prro-dps-contract` only, never a concrete
  adapter (plan line 227; structurally gated in CS-6).

## 7 · Protocol binding (full) + registry + correct-initial-binding (audit V12/V13/V14/V19)
```rust
struct DpsProtocolBinding {
    protocol_id: DpsProtocolId,                  // FscoZzd | EvpzDps
    contract_version: ProtocolContractVersion,   // ≥ 1
    capability_profile_version: CapabilityProfileVersion,
    endpoint_config_revision: EndpointConfigRevision,
}
trait DpsPortRegistry { fn resolve(&self, b: &DpsProtocolBinding) -> Option<Arc<dyn BoundDpsPorts>>; }
```
- **PB-1 (immutable, extends frozen invariant #3, #4A A4-4):** a doc's binding is snapshotted at reservation
  creation and immutable through every retry/reconciliation (032 immutability trigger `:171`).
- **PB-2 (correct INITIAL binding, audit V14):** the immutability trigger only forbids *later* change — a
  single store constructor MUST, in one tx, check `reservation.binding == shift.locked_dps_binding ==
  envelope.binding`; a **negative pin** rejects a wrong initial binding (not only a later UPDATE).
- **PB-3 (exact-version resolution, audit V13):** `resolve` matches the **exact** bound `{protocol_id,
  contract_version, capability_profile_version, endpoint_config_revision}`; **no version fallback** — a missing
  exact port ⇒ **fail-closed / RMR behind the fence**, never "use the latest".
- **PB-4 (single owner, audit V19):** binding lives on the reservation-bound request; the port *returns*
  observation; the engine attaches the already-verified immutable binding — the three copies (reservation /
  evidence / port) are reconciled by an exact-equality check in the `AttemptObservation` constructor.

## 8 · Cross-protocol + anti-mask (Spec #2 §5/§6; audit V17)
- **XP-1 (cross-protocol forbidden):** a `SubmittedUnknown` (or observed-reject) is reconciled **only** on its
  bound protocol under a declared `ReconciliationCapability`; protocol-B evidence for a protocol-A doc is
  inadmissible (RP-7).
- **XP-2 (fence ⇒ no issuance):** a fenced FN may do only read-only recon / STOP / HOLD / operator; no new
  issuance / offline-session / seed-advance (Spec #2 §5).
- **AM-1 (proof-of-life is narrow, audit V17):** liveness is a narrow method
  `ResponseEvidence::proves_dps_forward_progress() -> bool` that is `true` **only** for `Parsed(_)` (accept OR
  reject). `AuthenticatedPeerGarbage` / `NoResponse` return `false` — they DEGRADE, do not reset the anti-mask,
  do not permit issuance (Spec #2 §6). No `match ResponseObserved(_)` liveness discriminator exists.

## 9 · Normative invariants
- **D1** the three axes are stored/derived independently; no type encodes an authoritative single `DeliveryOutcome`.
- **D2** every derivable `(certainty, provenance, routing)` triple is one the real 032 CHECKs accept (§2 table).
- **D3** `NotSubmitted ⟺ DefinitelyNotStarted`; anything at `MayHaveStarted` with no `Parsed(Accepted)` ⇒ `SubmittedUnknown` (Spec #2 §3; never inferred from an error class).
- **D4** blind-resend is untypable (DP-2); a `SubmittedUnknown` has no path to `submit`.
- **D5** `FiscalAcceptance` requires a non-empty fiscal number (FA-1); no clean-accept fence-release otherwise.
- **D6** observation applies only via full-`{reservation_id,generation,binding,envelope_hash}` CAS (AO-1).
- **D7** binding is immutable AND correct-at-creation (PB-1/PB-2); exact-version resolve or fail-closed (PB-3).
- **D8** liveness = `Parsed` only (AM-1); `AuthenticatedPeer`/garbage never resets anti-mask.

## 10 · RED-pins (rewritten + the audit's missing set)
- **RP4B-1** started-call (`MayHaveStarted`) + any non-`Parsed` completion/cancel/reboot ⇒ `SubmittedUnknown`; a durable-`CallStarted`→no-outcome→reboot integration pin (not a hand-set `WireReach`).
- **RP4B-2** property test: every axis triple round-trips through the **real 032** (all 3 certainties × 3 provenances × 8 routing × NULL, with the CHECK-forbidden combinations rejected) — independently enumerated, not from the same enum (no common-mode).
- **RP4B-3** parsed-`-4` ⇒ `{SubmittedUnknown, ParsedDpsEnvelope, TransientRetry}`; a bare network timeout ⇒ `{SubmittedUnknown, NoResponse, TransientRetry}` — the two are distinct (the double-issue root).
- **RP4B-4** WAF/garbage-from-authenticated-peer ⇒ `{SubmittedUnknown, AuthenticatedPeer, ProbeRequired}`, `proves_dps_forward_progress()==false`, anti-mask NOT reset.
- **RP4B-5** three binding pins: immutable Rust type; DB mutation reject (032 trigger); runtime ALWAYS selects the exact reservation binding (PB-2/PB-3) — incl. a wrong-initial-binding negative pin and a global-profile-flip-does-not-affect-a-bound-doc pin.
- **RP4B-6 (compile-fail)** `ReconciliationTicket` cannot become a `SubmissionPermit`; a `SubmittedUnknown` state cannot reach `submit` / a `BoundSignedEnvelope`. (trybuild, not AST.)
- **RP4B-7** cross-protocol: protocol-B reconciliation of a protocol-A `SubmittedUnknown` is never invoked (not merely rejected after the fact); + a DAG gate that `prro-engine` names no concrete adapter.
- **RP4B-8..17 (audit's missing set):** crash-after-`CallStarted`-before-first-poll ⇒ `SubmittedUnknown`; stale-generation observation dropped; same envelope_hash on two attempts distinguished by generation; accepted-without-fiscal-number is unconstructable; KVT1/KVT2 do not double-apply; missing exact reconciliation capability ⇒ immediate RMR (fence held); all 8 `RetryClass` round-trip on 032; `DrainChainSettleRetry` not fresh-constructable (hydration-only); malformed digest fixed algorithm/size; a dropped future never becomes `NotSubmitted`.

## 11 · Scope boundaries (explicit)
- **CS-3:** the `DpsError+evidence → three fields` classifier; the 032 activation + `ObservedOutcomeV1`
  (#4A:189); the **minimal incumbent gRPC seam** so `-4` (`dto.rs:170`) survives to the classifier (audit V04 —
  cannot wait for CS-6). **CS-6:** the concrete adapters + crate-DAG gate. **Spec #3:** ingress inbox /
  `IdempotencyStrategy`. **CS-4:** `TransitionPlan` + the coordinator actor.

## 12 · Open questions for re-audit
1. Does the store-constructor-level binding-equality check (PB-2) belong in #4B (the contract asserts the
   invariant) or CS-3 (the store implements it)? — proposed: #4B pins the invariant, CS-3 implements.
2. `SubmissionPermit` / `ReconciliationTicket` home — `prro-dps-contract` (they gate the wire) vs a thin
   `prro-domain` capability module? Proposed: `prro-dps-contract`.
3. `RetryClass` currently lives in `write_path/error_routing.rs` (engine) — does #4B move it to a pure module
   both the engine and the contract can name, or does the contract depend on it in place? (Spec #2 says
   authority unchanged — proposed: relocate the *pure enum* to `prro-domain`, keep the routing *logic* in the
   engine.)

---
*Grounded/verified on `a97bf76`: Spec #2 (2026-07-14-spec2…) §1-§10 (the model) · 032_delivery_reservation.sql:92-114,171 (CHECK matrix + immutability, run on SQLite) · channel.rs:21-104 (DpsChannel surface) · dto.rs:66,170 · error.rs:15 · error_routing.rs:69 (RetryClass 8 literals) · stage_send.rs:1539/1565/1725 (CallStarted-durable, non-empty-fiscal guard, seed-advance) · SubmissionCertainty/ResponseProvenance/ReconciliationCapability grep-empty (new).*
