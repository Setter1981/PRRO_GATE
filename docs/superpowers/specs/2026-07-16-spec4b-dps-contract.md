# Spec #4B — DPS boundary contract (rev 6 — the TYPE realization of locked Spec #2)

**Status: 🔒 GO rev 6 — full GO from the external decorrelated audit (2026-07-17); MERGED to `main` #311 (`7ff0cf2`);
the oracle for CS-3/CS-6.** The three near-GO closures on rev-5 (auditor): (1) `DpsReject` is now the **complete**
closed set of named verdict codes `proto:41-56` (−5/−6/−7..−10/−11/−16 added — partition completeness); (2) the
**raw** `DpsSubmissionPort` trait stays in `prro-dps-contract` (adapter depends only on the contract; DAG
preserved), while the private `AuthorizedSubmission` mint + `submit_authorized` wrapper move to the engine
(§6, §12 R3); (3) `NotFound` is moved **out** of attributed `ReconcileOutcome` into `FnLiveness` under the
`Unattributed` path (§5, §6). Everything else is the rev-5 SIMPLIFY body below.

**Final fix (mechanical type mismatch, auditor-confirmed → full GO):** `submit_raw` returned `AttemptObservation`, which the raw port
**cannot construct** — the port has only the `BoundSignedEnvelope` (binding + hash), not `reservation_id` /
`node_generation`. Fixed: `submit_raw -> Result<SendResponse, PortBindingMismatch>` (raw wire response — narrowed from
`SubmissionEvidence` because the port is always post-CAS, so `NotStarted` is structurally impossible for it); the
engine `submit_authorized` wrapper builds `SubmissionEvidence::Started{ response, binding, envelope_hash }` and
attaches the token's `{reservation_id, node_generation}` → `AttemptObservation`. Now symmetric with
`probe -> RawReconcileReply` + `validate_reconcile -> ReconciliationObservation`: raw ports yield raw wire
results; the identity-holding engine/validator layer builds the attributed observation (§6).

**rev-5 basis: SIMPLIFY-not-escalate pass after the rev-4 NOT-YET.** The rev-4 corrective-resend was
the wrong move: it repeated the DPS wire-call, which **locked** authority forbids
(`spec4-authority-minilock.md:32` A4-6 "the DPS wire-call is **NEVER** repeated"; Spec #2 §5 "fenced
`SubmittedUnknown` may do **only** read-only reconciliation / STOP / HOLD / operator — **no new issuance, no seed
advance**"), and it required a `generation` column 032 does not have (verified: `generation` is
`node_state.delivery_generation`, a **CS-3** token — `minilock:28,35`, `032:17`). rev 5 therefore **removes**
corrective-resend from #4B entirely (it is a CS-3 new-attempt edge + a locked-spec amendment, not a #4B type),
and closes the rest of the auditor's set by making the algebra a disjoint partition, splitting send-evidence from
reconciliation-evidence, splitting genuine no-response from a remote status, making correlation a validated
closed ctor, holding binding pre-wire, adopting the co-located-mint sealing (his option a), and defining every
referenced type. Grounded on `origin/main` (verbatim this pass: `minilock.md:19-35,180-190`,
`spec2…fsm.md:63-70`, plus the rev-4 basis).

**Scope unchanged:** #4B = the Rust type/trait contract encoding Spec #2. The classifier *logic*, the 032
activation, `ObservedOutcomeV1`, `node_state.delivery_generation`, and the minimal incumbent gRPC seam are
**CS-3**; the concrete adapters + crate-DAG gate are **CS-6**; the schema/fence/anti-mask *semantics* +
record-then-apply are already locked (#4A / Spec #2).

---

## 0 · rev-4 blocker → rev-5 resolution
| # | rev-4 blocker | rev-5 resolution | §, grounding |
|---|---|---|---|
| B1 | **DP-2b re-opens blind-resend** (routing-only gate lets timeout/-4 through) **and contradicts locked #4A/#2** | **Removed.** #4B has NO wire-resend of any kind: `submit` mints only from `RESERVED_NOT_STARTED`; a fenced doc's only paths are read-only recon / STOP / HOLD / operator (Spec #2 §5) + record-then-apply ledger-repeat (A4-6). The incumbent `-12` MAC-recovery re-sign is an explicit **CS-3 new-attempt edge + locked-spec amendment**, out of #4B | §6, §11; `minilock:32`, `spec2:65` |
| B2 | **corrective transition undefined; no `generation` column** | Not a #4B type (B1). `generation` clarified = `node_state.delivery_generation` (CS-3 token, not a reservation column); AO-1's generation-check is CS-3 apply-time | §4; `minilock:28,35`, `032:17` |
| B3 | **algebra not a disjoint partition** (`Reject::Unknown`≡`Indeterminate::UnknownStatus`; `DpsCode` unconstrained; `ParsedOutcome` undeclared) | `SendOutcome` declared; `DpsReject` = **closed named verdicts only** (no free-form); every unrecognized code ⇒ `SendIndeterminate::UnknownStatus` (the sole free-form arm). Disjoint by construction | §5 |
| B3b/B5 | **send vs quittance phase confusion** — a matched `last_chk` with short `data_sign` loses already-proven `Submitted` | **Send-evidence and reconcile-evidence are different types.** `SubmissionEvidence` carries `SendOutcome` (establishes certainty); `ReconciliationObservation` carries `ReconcileOutcome` (monotone refine, **never regresses** certainty) | §3, §5, §6 |
| B4/B8 | **`NoResponse(ProtocolDecode)` false**; gRPC `Unauthenticated` over live TLS is a *peer response*, not absence | `SendResponse` = `NoResponse(cause)` **only** for genuine local absence; a `RemoteStatus` arm carries the TLS-proven remote auth-status **(CS-3 3.2:** the un-parseable authenticated-peer *garbage body* is NOT `RemoteStatus` — it collapses to `NoResponse{CallFailedWithoutTrustedDpsEnvelope}`, §3 SE-2**)** | §3 |
| B5 | **F7 correlation nominal** (proves FN, not the doc; `ProvenCorrelation` undefined) | Contract-owned `validate_reconcile(ticket, raw)`; the **only** ctor of `ReconciliationObservation`; `ProvenCorrelation` is a private witness built solely on an exact doc-level match; FN-level `last_chk` ⇒ `UnattributedProbeObservation` | §6 |
| B6 | **RP4B-2 still green-but-unsound** (image equality misses an Accepted/Rejected swap) | pin compares the **full graph** `{(evidence-discriminant, classify(evidence))} == normative-graph`, not just the image | §10 RP4B-2 |
| B7 | **binding not held at the port boundary** — a wrong port can wire-call before mismatch | `submit_raw(envelope) -> Result<SendResponse, PortBindingMismatch>` (never `SubmissionEvidence` — port is post-CAS, `NotStarted` impossible); MUST check `envelope.binding == self.binding()` **before any wire I/O** | §6 DP-3; RP4B-5 |
| B4-ans | **sealed-trait-across-crates doesn't work** (no friend crates) | Adopt his **option (a)**: co-locate `AuthorizedSubmission` (private ctor) + the mint + the `submit` boundary in **one module**; Rust privacy proves mint-only-here. Retire the sealed-trait idea | §6, §12 |
| A | **class-A**: undefined `ParsedOutcome`/`HydratedRetryClass`/`ProvenCorrelation`/`BoundDpsPorts`; `set_routing` unnamed | all defined (§2, §5, §6) | §2, §5, §6 |

## 1 · Grounded reality (SPEC-READY per auditor — live-seam citations confirmed)
- **Send seam** `DpsChannel::send_chk(envelope) -> Result<CheckAck, DpsError>` (`channel.rs:24`); `CheckAck`
  (`dto.rs:66`) = `{ id, id_sign, data_sign }`, `MIN_KVT1_DATA_SIGN_LEN=64`.
- **Two distinct phases (not atomic):** (1) **send acceptance** = OK status + non-empty `id`
  (`stage_send.rs:1570-1572`; `data_sign` NOT gated at send); (2) **KVT1 quittance** = a **later** `last_chk`
  with `data_sign.len()>=64` (`kvt2_confirm.rs:314`, `boot_phase.rs:755`), driving **two envelopes**
  (`kvt2_advance.rs:118-140`: Envelope 1 `Sent→Kvt1→Kvt2`, Envelope 2 `stage_finalize Kvt2→Ack`). `id_sign` is
  never read by a live advance.
- **`-4` loss:** `dto.rs:215` maps `-4`→`DpsError::Transport`; no `-4` arm in `error_routing.rs`.
- **Verbatim `DpsChannel` surface** (§6 basis): `send_chk(CheckEnvelope)->CheckAck` `:24` · `last_chk(&CheckSignBlob)->CheckAck`
  `:29` · `ping(CheckEnvelope)->CheckAck` `:33` · `status_rro(&CheckSignBlob)->StatusSnapshot` `:36` ·
  `info_rro(&CheckSignBlob)->RroInfo` `:40` · `ask_offline_codes(CheckEnvelope)->OfflineCodesResponse` `:57` ·
  `by_server_fiscal_no(&CheckSignBlob, expected_id:&str)->CheckAck` `:75` (default: `last_chk`, then empty→`NotFound`,
  `ack.id!=expected_id`→`ServerFiscalIdMismatch`, else `Ok`) · `query_by_local_identity(&str, i32)->CheckAck` `:104`
  (→`Err(QueryNotSupported)`).
- **Locked authority #4A / Spec #2** (rev-5's load-bearing constraints): A4-6 the DPS wire-call is **NEVER**
  repeated (`minilock:32`); A4-2 `node_state.delivery_generation` is the CS-3 fence token (`minilock:28`); Spec #2
  §5 a fenced `SubmittedUnknown` may do only read-only recon / STOP / HOLD / operator (`spec2:65`).
- **Verbatim 032 CHECK matrix** (`:74-187`): states `RESERVED_NOT_STARTED→CALL_STARTED→OUTCOME_OBSERVED`;
  `call_started_at` marker at RN→CS (`:80`); `envelope_hash=SHA256(prost Check)` @ `stage_send.rs:795` (`:85`);
  three fields (`:92-95`); CHECKs `:102-118`; fence `:127-133`; triggers `:162-187`. **No `generation` column.**

## 2 · Three orthogonal axes + the total SEND-derivation rule (B3)
```rust
enum SubmissionCertainty { NotSubmitted, SubmittedUnknown, Submitted }              // prro-domain
enum ResponseProvenance  { NoResponse, AuthenticatedPeer, ParsedDpsEnvelope }       // prro-domain
// (c) B/F8 — classifier + fresh-write input is ActiveRetryClass (7). HydratedRetryClass (8) is the read/decode
//     type (adds DrainChainSettleRetry).
//   ⚠️ R4 RELOCATION IS UNBUILT (CS-3 3.2 recon, §12 R4): `ActiveRetryClass` IS realized in prro-domain
//     (mod.rs:270), but `RetryClass` was NOT relocated — it stays the storage authority in prro
//     (error_routing.rs:69) with NO compat re-export, NO `From<ActiveRetryClass>` widening, and NO `set_routing`
//     store API. The relocation triple + a routing-store home is a Bridge/D work item (keystone), NOT a shipped
//     state. The three lines below are the DESIGN TARGET, not current code.
enum ActiveRetryClass   { TerminalReject, TransientRetry, FnConfigError, WrapperBug, ProbeRequired, MacRecovery, OperatorEscalation }  // REALIZED (prro-domain mod.rs:270)
enum HydratedRetryClass { Active(ActiveRetryClass), DrainChainSettleRetry }         // decode-only; DrainChain unreachable fresh (REALIZED)
impl From<ActiveRetryClass> for RetryClass { /* widening */ }                       // UNBUILT (D/E) — RetryClass not yet relocated
// fresh-write API (store): fn set_routing(&self, id: ReservationId, r: ActiveRetryClass);   // UNBUILT (D/E) — no routing-store home yet
```
**Total SEND-derivation** — a total function of `SubmissionEvidence` (§3). Reconciliation is a **separate**
monotone refinement (§5), never in this table:

| `SubmissionEvidence` (§3) | ⇒ certainty | ⇒ provenance | routing (CS-3) | 032 |
|---|---|---|---|---|
| `NotStarted{reason}` | `NotSubmitted` | `NoResponse` | `TransientRetry` / `WrapperBug` | `:107`,`:110` |
| `Started{NoResponse(cause)}` (incl. `CrashedBeforeObservation`) | `SubmittedUnknown` | `NoResponse` | `TransientRetry` / `WrapperBug` | `:108` |
| `Started{RemoteStatus(RemoteAuthStatus)}` (TLS-proven Unauth/PermDenied; CS-3 3.2 — garbage body is NOT here → `NoResponse`) | `SubmittedUnknown` | `AuthenticatedPeer` | `ProbeRequired` | `:114` |
| `Started{Parsed(Indeterminate)}` (`-4`,`-3`,close-ambig, unknown-code, OK-no-id) | `SubmittedUnknown` | `ParsedDpsEnvelope` | `TransientRetry` / `ProbeRequired` | `:110`,`:114` |
| `Started{Parsed(Accepted(SentAccepted))}` | `Submitted` | `ParsedDpsEnvelope` | *(NULL clean-accept)* | `:109`,`:110` |
| `Started{Parsed(Rejected(closed verdict))}` (`-1`,`-12`,`-13`,`-14`,close-verdict) | `Submitted` | `ParsedDpsEnvelope` | one of `:113`'s four | `:109`,`:113` |

Totality: `certainty(NotStarted)=NotSubmitted`; `certainty(Started{Parsed(Accepted|Rejected)})=Submitted`;
`certainty(every other Started)=SubmittedUnknown`. No `DeliveryOutcome` authoritative dual (Spec #2 §2).

## 3 · `SubmissionEvidence` — closed enum; genuine-absence vs remote-status (B4/B8)
```rust
enum SubmissionEvidence {                         // prro-dps-contract; no public struct-literal ctor
    NotStarted { reason: PreflightRefusal, binding: DpsProtocolBinding, envelope_hash: EnvelopeHash },
    Started    { response: SendResponse,   binding: DpsProtocolBinding, envelope_hash: EnvelopeHash },
}
// CS-3 3.2 (realized, prro-domain mod.rs:435): `SendResponse` ships as an OPAQUE struct over a PRIVATE inner
// enum, read via `kind() -> SendResponseKind<'_>` and built ONLY by source-gated ctors
// (`no_response` / `remote_status` / `parsed`) — Class-A sealing, NOT a public 3-arm enum. Same three arms.
struct SendResponse(SendResponseInner);           // opaque; view: kind(); ctors: no_response/remote_status/parsed
enum SendResponseInner {                          // PRIVATE — the three arms are the pre-3.2 public enum, now sealed
    NoResponse(NoResponseCause),                  // GENUINE local absence — no bytes / no session reached the far side
    RemoteStatus(RemoteStatusEvidence),           // the far side responded, but NOT with a parseable DPS envelope
    Parsed(SendOutcome),                          // a DPS envelope was received + parsed (§5)
}
enum SendResponseKind<'a> { NoResponse(&'a NoResponseCause), RemoteStatus(&'a RemoteStatusEvidence), Parsed(&'a SendOutcome) }
enum NoResponseCause {                            // B4 — no ProtocolDecode, no auth-status here
    LocalHandshakeFailure,                        // TCP/TLS/DNS never established a session
    Timeout, Cancelled,                           // per-call deadline / future dropped / shutdown
    CrashedBeforeObservation,                     // durable CALL_STARTED, then reboot before any response
    CallFailedWithoutTrustedDpsEnvelope,          // CS-3 3.2 (mod.rs:406): incumbent tonic `Internal` — a received body
                                                  //   decode-collapsed with NO TLS-proven status (honest genuine-absence)
}
enum RemoteStatusEvidence {                       // B8 — a peer response, NOT a transport-level absence
    // CS-3 3.2 (realized, mod.rs:416): `AuthenticatedPeerGarbage` was REMOVED — the incumbent tonic seam collapses a
    // decode-failure of a received body to `Internal` → `NoResponse{CallFailedWithoutTrustedDpsEnvelope}`, so the
    // un-parseable-WAF-body case never reaches this axis; only the TLS-proven Unauth/PermDenied arm survives.
    RemoteAuthStatus(GrpcStatusDigest),           // gRPC Unauthenticated/PermissionDenied over an established session
}
struct EnvelopeHash([u8; 32]);                    // == 032:85 length=32
struct DecodedResponseDigest([u8; 32]);           // CS-3 3.2 (mod.rs:121): honest fingerprint of DECODED DPS envelope content
struct GrpcStatusDigest([u8; 32]);                // CS-3 3.2 (mod.rs:141): DISTINCT type — fingerprints a transport-status reply
```
- **SE-1 (fail-safe):** absence of a `NotStarted` witness ⇒ `Started` (Spec #2 §3). `NotStarted` only before the
  durable `CALL_STARTED` marker (`032:80`) or a `PreflightRefusal`.
- **SE-2 (CS-3 3.2 realized — INVERTED from the rev-6 prose):** a decode failure of *received* bytes ships as
  `NoResponse(CallFailedWithoutTrustedDpsEnvelope)` (honest genuine-absence, `mod.rs:406`), **not** a `RemoteStatus`
  garbage arm — the incumbent tonic seam collapses the un-parseable authenticated-peer body to `Internal`. Only a
  TLS-proven `Unauthenticated`/`PermissionDenied` surfaces as `RemoteStatus::RemoteAuthStatus` (A′ seam, `grpc.rs`).
  A parsed-garbage evidence arm would need a custom codec (a 3.2 non-goal). A `RemoteStatus` never sets
  `proves_dps_forward_progress()` (§8).

## 4 · `AttemptObservation` — bind to reservation + node generation
```rust
struct AttemptObservation { reservation_id: ReservationId, node_generation: DeliveryGeneration, evidence: SubmissionEvidence }
```
- **AO-1 (apply is CS-3):** `DeliveryGeneration` = `node_state.delivery_generation` (the CS-3 fence token,
  `minilock:28`; **not** a 032 reservation column). #4B carries the generation the attempt was minted under; the
  apply/CAS that **drops a stale-generation observation** is CS-3 record-then-apply (A4-6).
- **AO-2 (binding echo-check):** the ctor requires `evidence.binding() == reservation.binding` (full tuple, §7);
  an unequal binding is unconstructable.

## 5 · Algebra — disjoint partition; send vs reconcile as different types (B3, B3b/B5)
**Send-outcome** (from `send_chk`; establishes certainty for the first time):
```rust
// CS-3 3.2 (realized): like `SendResponse` (§3), `SendOutcome` and `SendIndeterminate` ship SEALED — opaque
// structs over private inner enums, read via `kind()` (mod.rs:524, mod.rs:732). Same arms; Class-A sealing.
enum SendOutcome {                                // B3 — declared; the three arms are disjoint by construction
    Accepted(SentAccepted),                       // ⇒ Submitted
    Rejected(DpsReject),                          // a closed DPS verdict on THIS doc ⇒ Submitted
    Indeterminate(SendIndeterminate),             // parsed, does NOT establish processing ⇒ SubmittedUnknown
}
struct SentAccepted { fiscal_number: NonEmptyFiscalNumber }   // OK + non-empty id; data_sign irrelevant at send (auditor ans-3)
enum DpsReject {                                  // CLOSED — every named definitive verdict code (proto:41-56).
    Verify,                                       // -1  ERROR_VEREFY (DocumentReject)
    Type,                                         // -5  ERROR_TYPE (builder/adapter bug — terminal)
    NotPrevZReport,                               // -6  ERROR_NOT_PREV_ZREPORT (operator-recoverable ⇒ OperatorEscalation)
    Xml,                                          // -7  ERROR_XML
    XmlDate,                                      // -8  ERROR_XML_DATE
    XmlChk,                                       // -9  ERROR_XML_CHK
    XmlZReport,                                   // -10 ERROR_XML_ZREPORT
    Offline168,                                   // -11 ERROR_OFFLINE_168 (168h cap ⇒ node→BLOCKED side-effect)
    BadHashPrev,                                  // -12 ERROR_BAD_HASH_PREV (CS-3 MAC-recovery = NEW attempt, §11)
    NotRegisteredRro,                             // -13 ERROR_NOT_REGISTERED_RRO (⇒ FnConfigError)
    NotRegisteredSigner,                          // -14 ERROR_NOT_REGISTERED_SIGNER (⇒ FnConfigError)
    OfflineId,                                    // -16 ERROR_OFFLINE_ID (terminal + ALERT)
    Close,                                        // -2 ERROR_CHECK / -15 ERROR_NOT_OPEN_SHIFT for NON-close doc types (§12 R1)
}
enum SendIndeterminate {                          // the ONLY free-form arm is UnknownStatus
    UnknownStatus { raw_code: BoundedText, digest: DecodedResponseDigest }, // -4 ERROR_UNKNOWN + any code NOT in DpsReject
    SaveError,                                    // -3 ERROR_SAVE (transient)
    CloseAmbiguous,                               // -2 ERROR_CHECK / -15 ERROR_NOT_OPEN_SHIFT on close / Z-report ONLY (§12 R1)
    OkButNoFiscalNumber { digest: DecodedResponseDigest }, // status OK but empty id
}
// Partition completeness: every send status in proto:41-56 has exactly one home — a definitive verdict in the
// CLOSED DpsReject, or -3/-4/OK-no-id in SendIndeterminate; -2/-15 split by doc_type (verdict vs CloseAmbiguous).
// Only a code OUTSIDE this enumerated set reaches UnknownStatus. CS-3 owns the code→arm map; #4B owns totality.
```
**Reconcile-outcome** (from `last_chk` / `by_server_fiscal_no`; on an ALREADY-submitted doc; **monotone** — never
regresses certainty, B3b/B5):
```rust
enum ReconcileOutcome {                           // ATTRIBUTED outcomes ONLY — each requires a proven id-match (RC-1)
    Kvt1Confirmed { kvt1_raw: Kvt1Raw },          // id matched + data_sign>=64 ⇒ Submitted + quittance proven
    IdMatchedNoQuittance,                         // id matched, data_sign<64 ⇒ Submitted STAYS proven, quittance pending
}
// `NotFound` (last_chk empty id) is NOT here — it is FN-level and attributes to NO document; it surfaces as
// ReconcileValidation::Unattributed(UnattributedProbeObservation { fn_liveness: NotFound }) (§6), never an
// attributed outcome. Removing it keeps ReconcileOutcome strictly "this doc, proven".
struct Kvt1Raw(Vec<u8>);                          // data_sign, len >= MIN_KVT1_DATA_SIGN_LEN(64)
```
- **AL-1 (disjoint):** a code is in exactly one arm — every unrecognized/unmapped code ⇒
  `SendIndeterminate::UnknownStatus`; a definitive verdict is a **closed** `DpsReject` variant; `-3` is
  `SaveError`, never `Rejected`. `DpsReject` has no free-form constructor, so `-3/-4/close-ambig` cannot be nested.
- **AL-2 (no certainty regression):** a reconcile of an already-`Submitted` doc with `IdMatchedNoQuittance` keeps
  `Submitted` (only the quittance is pending) — reconciliation raises `SubmittedUnknown→Submitted` on a proven
  id-match, but **never** lowers `Submitted→SubmittedUnknown` (that was the rev-4 defect).
- **AL-3 (ctors):** `SentAccepted::observe(id)->Option` iff `!id.is_empty()`; `Kvt1Confirmed` via the validator
  (§6) iff id matched **and** `data_sign.len()>=64`. `id_sign` unmodelled (never read live). Peer strings are
  `BoundedText`/digests (audit V18 DoS).

## 6 · `DpsPort` capability-split + atomic authorization + validated correlation (B4-ans, B5, B7)
```rust
// ── prro-dps-contract — the RAW port trait. The adapter (CS-6) impls it and depends ONLY on the contract, so the
//    crate-DAG is preserved (no engine→adapter or adapter→engine cycle). It takes already-bound bytes; it does NOT
//    see the authorization token. (auditor instruction #2: raw-port trait in contract, private mint/wrapper in engine.)
#[async_trait] trait DpsSubmissionPort: Send + Sync {
    fn binding(&self) -> &DpsProtocolBinding;
    // Raw wire call over already-bound bytes. B7 — MUST check envelope.binding == self.binding() BEFORE any wire
    // I/O; a mismatch returns with zero wire calls. Yields ONLY the response (never a verdict, DP-1). Return type is
    // `SendResponse`, NOT `SubmissionEvidence`: the port is always post-CAS (RN→CALL_STARTED already fired), so a
    // `NotStarted` result is structurally impossible here — `NotStarted` is an engine-only preflight refusal minted
    // BEFORE authorization. The port also lacks reservation_id / node_generation, so it cannot build an
    // AttemptObservation. The engine wrapper wraps the response into `Started{..}` + attaches identity.
    async fn submit_raw(&self, envelope: BoundSignedEnvelope) -> Result<SendResponse, PortBindingMismatch>; // ~ send_chk
}

// ── engine delivery module (PRIVATE) — the authorization gate; owns the mint + the SOLE production caller ──
// AuthorizedSubmission's ctor + authorize_submission + submit_authorized are PRIVATE to this module. Rust privacy
// proves the token can be minted ONLY here (strong: no fenced/OO reservation can fabricate a token). What privacy
// does NOT prove — that nobody calls the public `submit_raw` directly — is enforced by the crate-DAG + review gate:
// `submit_authorized` is the ONLY production caller of `submit_raw`. Honest split: token-mint = Rust-privacy-proven;
// no-direct-raw-call = DAG/review (the auditor's option-b acknowledgment for the half privacy cannot cover).
struct AuthorizedSubmission {                     // no pub ctor; built only by authorize_submission
    reservation_id: ReservationId, node_generation: DeliveryGeneration,
    binding: DpsProtocolBinding, envelope_hash: EnvelopeHash, bytes: SecretBytes,   // hash == SHA256(bytes)
}
fn authorize_submission(store: &Store, id: ReservationId, envelope: BoundSignedEnvelope) -> Option<AuthorizedSubmission>;
//   ONE atomic tx: verify durable RESERVED_NOT_STARTED ∧ unfenced ∧ reservation.binding == envelope.binding
//   (FULL tuple, PB-2) ∧ hash == SHA256(bytes); flip RN→CALL_STARTED (set call_started_at, 032:162); mint.
//   After the CAS the row is CALL_STARTED ⇒ a second call returns None (no double mint). A fenced /
//   OUTCOME_OBSERVED reservation NEVER yields a token — blind-resend is untypable (D4). NO corrective path (B1).
async fn submit_authorized(port: &dyn DpsSubmissionPort, auth: AuthorizedSubmission)
    -> Result<AttemptObservation, PortBindingMismatch>;
//   the SOLE production caller of submit_raw. Consumes the token by value; forwards its bound bytes as a
//   BoundSignedEnvelope to port.submit_raw → SendResponse; builds SubmissionEvidence::Started { response,
//   binding: auth.binding, envelope_hash: auth.envelope_hash }; then the AttemptObservation by attaching the
//   token's {reservation_id, node_generation} (AO-2 holds by construction: the Started binding IS auth.binding).
//   This is the ONLY place the reservation identity meets the wire response — the raw port never sees it.
#[async_trait] trait DpsReconciliationPort: Send + Sync {
    fn capability(&self) -> ReconciliationCapability;
    // Returns the RAW far-side reply; attribution is the validator's job (B5), never the port's.
    async fn probe(&self, ticket: &ReconciliationTicket) -> Result<RawReconcileReply, DpsError>;
}
#[async_trait] trait DpsStatusPort: Send + Sync {             // verbatim signatures (§1)
    async fn status_rro(&self, fn_sign: &CheckSignBlob) -> Result<StatusSnapshot, DpsError>;
    async fn info_rro(&self,   fn_sign: &CheckSignBlob) -> Result<RroInfo, DpsError>;
    async fn ping(&self,       envelope: CheckEnvelope) -> Result<CheckAck, DpsError>;
}
#[async_trait] trait DpsOfflineCodesPort: Send + Sync {
    async fn ask_offline_codes(&self, envelope: CheckEnvelope) -> Result<OfflineCodesResponse, DpsError>;
}
trait BoundDpsPorts { fn submission(&self) -> &dyn DpsSubmissionPort; fn reconciliation(&self) -> &dyn DpsReconciliationPort;
                      fn status(&self) -> &dyn DpsStatusPort; fn offline_codes(&self) -> &dyn DpsOfflineCodesPort; } // registry value
struct BoundSignedEnvelope { binding: DpsProtocolBinding, hash: EnvelopeHash, bytes: SecretBytes }  // closed ctor
enum PortBindingMismatch { WrongProtocol { expected: DpsProtocolId, got: DpsProtocolId } }

// ── validated correlation (B5) — contract-owned; the ONLY ctor of ReconciliationObservation ──
struct ReconciliationTicket { reservation_id: ReservationId, node_generation: DeliveryGeneration,
                              binding: DpsProtocolBinding, correlation: ExpectedCorrelation }
enum ExpectedCorrelation {
    ByServerFiscalNo { fn_sign: CheckSignBlob, expected_id: String },  // sfn is DOC-specific ⇒ can attribute
    BySign(CheckSignBlob),                                             // FN-level last_chk ⇒ unattributed unless content matches
    ByLocalIdentity { fn_id: String, local_number: i32 },             // DPS-unsupported (typed Err)
}
fn validate_reconcile(ticket: ReconciliationTicket, raw: RawReconcileReply) -> ReconcileValidation;
enum ReconcileValidation {
    Attributed(ReconciliationObservation),        // raw provably corresponds to THIS doc (ack.id == expected_id)
    Unattributed(UnattributedProbeObservation),   // valid DPS reply, but FN-level only (BySign / different doc)
    Mismatch,                                      // ServerFiscalIdMismatch — never attributed
}
struct ReconciliationObservation { reservation_id: ReservationId, node_generation: DeliveryGeneration,
                                   matched: ProvenCorrelation, outcome: ReconcileOutcome }
struct ProvenCorrelation(());                     // private witness; built ONLY by validate_reconcile on an exact match
struct UnattributedProbeObservation { fn_liveness: FnLiveness }  // FN liveness only; attributes to NO reservation
enum FnLiveness {                                 // where NotFound now lives (moved out of ReconcileOutcome)
    NotFound,                                     // last_chk empty id — the FN has no matching last check (inconclusive for THIS doc)
    OtherDocLast(DecodedResponseDigest),              // the FN's last check is a DIFFERENT doc — live, but not ours
}
enum ReconciliationCapability { None /* ⇒ immediate RMR behind the fence */, ByServerFiscalNo, ByLocalIdentity }
```
- **DP-1 (evidence, not verdict):** ports yield raw replies / evidence / read snapshots — never a `DocState` /
  `RetryClass` / `SendDisposition`.
- **DP-2 (blind-resend untypable — D4):** the engine wrapper `submit_authorized` consumes `AuthorizedSubmission`
  by value; the token is minted **only** by `authorize_submission` (engine-private) from durable
  `RESERVED_NOT_STARTED`. No fenced / `SubmittedUnknown` reservation can obtain it; there is **no** corrective
  wire-resend anywhere in #4B (B1). Direct `submit_raw` is prevented from production use by the DAG/review gate
  (`submit_authorized` is its sole caller). RP4B-6 (trybuild) is a canary over the mint.
- **DP-3 (binding held pre-wire — B7):** `submit_raw` checks `envelope.binding == self.binding()` before any wire
  I/O → `PortBindingMismatch` with zero wire calls; and the wrapper's `auth` bytes hash to `envelope_hash`.
- **DP-4:** `binding()` constant; evidence carries `binding`.
- **DP-5 (engine ∉ adapter):** the adapter (CS-6) impls these traits; the DAG gate forbids `prro-engine` naming a
  concrete adapter.
- **RC-1 (correlation is proven — B5):** a `ReconciliationObservation` exists **only** via `validate_reconcile`,
  which compares the ticket's `ExpectedCorrelation` to the raw reply and mints `ProvenCorrelation` **only** on an
  exact doc-level match (`ack.id == expected_id`, `channel.rs:75-101`). `BySign` / a non-matching reply ⇒
  `Unattributed` (FN-liveness only, attributes to no reservation) — never a fabricated attribution.

## 7 · Protocol binding (full) + ownership hierarchy (B7/F6; `Option` fixed)
```rust
struct DpsProtocolBinding {
    protocol_id: DpsProtocolId,                              // FscoZzd | EvpzDps (032:81)
    contract_version: ProtocolContractVersion,              // >= 1             (032:82)
    capability_profile_version: Option<CapabilityProfileVersion>, // (032:83)
    endpoint_config_revision: Option<EndpointConfigRevision>,     // (032:84)
}
trait DpsPortRegistry { fn resolve(&self, b: &DpsProtocolBinding) -> Option<Arc<dyn BoundDpsPorts>>; }
```
- **PB-4a (shift owns the coarse lock):** the shift fixes exactly `DpsProtocolId` at shift-open (plan `161-163`;
  extends frozen invariant #3).
- **PB-4b (doc owns the full snapshot):** the reservation snapshots the full binding at creation (`032:81-84`),
  immutable through retries (plan `250-254`; `032:170-187`).
- **PB-4c (evidence/port are echoes):** checked-equal in the `AttemptObservation` ctor (AO-2); unequal ⇒
  unconstructable.
- **PB-2 (correct INITIAL binding, full tuple — F6):** the store ctor checks `reservation.binding ==
  envelope.binding` on the **full tuple** and `reservation.binding.protocol_id == shift.locked_dps_protocol_id`
  (the shift locks only `protocol_id`); a negative pin rejects a wrong initial binding. The minted
  `AuthorizedSubmission` carries the same `binding`/`envelope_hash`, and `submit` re-checks pre-wire (DP-3).
- **PB-3 (exact-version resolve):** `resolve` matches the exact tuple; no fallback — a missing exact port ⇒
  fail-closed / RMR behind the fence.

## 8 · Cross-protocol + anti-mask + authenticated-peer seam (Spec #2 §5/§6; F5)
- **XP-1/XP-2:** a `SubmittedUnknown`/observed-reject is reconciled only on its bound protocol under a declared
  capability; a fenced FN may do only read-only recon / STOP / HOLD / operator — no issuance / offline / seed
  (fence `032:127-133`; Spec #2 §5).
- **AM-1 (narrow liveness):** `proves_dps_forward_progress()` is `true` **only** for `SendResponse::Parsed(_)`
  (Accepted / Rejected / Indeterminate — a real DPS envelope). `RemoteStatus(_)` / `NoResponse(_)` ⇒ `false`
  (degrade; never reset the anti-mask; never permit issuance).
- **AM-2 (CS-3 3.2 — A′ seam realized for TLS-proven status; garbage body stays genuine-absence):** the incumbent
  `map_tonic_status` (`grpc.rs:127-128`) collapsed **every** tonic `Status` into `Transport(...)`. **A′ (shipped)**
  now live-converts a **TLS-proven** `Unauthenticated`/`PermissionDenied` into `RemoteStatus::RemoteAuthStatus`
  (`grpc.rs`), and the shadow yields it read-only. An un-parseable **WAF/garbage body is NOT proven** — it surfaces
  honestly as `NoResponse(CallFailedWithoutTrustedDpsEnvelope)` (tonic `Internal`, `mod.rs:406`); a parsed-garbage
  evidence arm would need a custom codec (a 3.2 non-goal). #4B pins the type + law; the `AuthenticatedPeer`
  provenance is populated only for the TLS-proven `RemoteAuthStatus` arm.

## 9 · Normative invariants
- **D1** three axes independent; no authoritative single `DeliveryOutcome`.
- **D2** every derivable `(certainty, provenance, routing)` triple is 032-CHECK-accepted (§2).
- **D3 (total)** `certainty` is a total function of `SubmissionEvidence`: `NotStarted⇒NotSubmitted`;
  `Started{Parsed(Accepted|Rejected)}⇒Submitted`; every other `Started⇒SubmittedUnknown`.
- **D4 (no wire-resend — locked)** the DPS wire-call is never repeated (A4-6): the `AuthorizedSubmission` token is
  minted only from `RESERVED_NOT_STARTED`, and `submit_authorized` is the sole caller of `submit_raw`; a fenced doc
  has **no** resubmit path (Spec #2 §5). Recovery is ledger-apply-repeat only
  (A4-6); the incumbent `-12` re-sign is a CS-3 **new-attempt** edge + a locked-spec amendment (§11), not a #4B type.
- **D5** acceptance is send-phase (`SentAccepted`, non-empty id) vs reconcile-phase (`Kvt1Confirmed`, id-match +
  `data_sign>=64`); reconciliation is **monotone** (AL-2), never regressing `Submitted`.
- **D6** an observation applies only via a full-tuple CAS carrying `node_generation` (AO-1) with
  `evidence.binding == reservation.binding` (AO-2).
- **D7** binding: shift owns `DpsProtocolId`, doc owns the full immutable snapshot, evidence/port are checked-equal
  echoes; PB-2 compares the full tuple; `submit` holds binding pre-wire (DP-3); exact-version resolve or fail-closed.
- **D8** reconciliation attribution is **proven** (RC-1): a `ReconciliationObservation` exists only via
  `validate_reconcile` on an exact doc-level match; FN-level replies are `Unattributed`.
- **D9** the classifier / `set_routing` accept only `ActiveRetryClass` (7); `DrainChainSettleRetry` lives only in
  the decode-only `HydratedRetryClass` — no fresh-write path (`032:117`).

## 10 · RED-pins
- **RP4B-2 (graph, not image — B6):** enumerate all `SubmissionEvidence` shapes; assert the **full mapping**
  `{ (evidence-discriminant, classify(evidence)) } == the normative graph` (each specific evidence ↦ its specific
  triple) — so an Accepted↔Rejected swap is caught (same image, wrong graph) — AND the DB accepts each — AND the
  classifier emits none of the other CHECK-legal-but-non-normative combos.
- **RP4B-1** durable `CALL_STARTED` (`032:80`) then any non-`Parsed` completion / cancel / reboot ⇒
  `SubmittedUnknown` (incl. `CrashedBeforeObservation`).
- **RP4B-3** `-4` ⇒ `{SubmittedUnknown, Parsed, TransientRetry}`; `-3` same row; bare timeout ⇒ `{SubmittedUnknown,
  NoResponse, TransientRetry}`; `-1` ⇒ `{Submitted, Parsed, TerminalReject}` — all distinct.
- **RP4B-4** a **TLS-proven** `RemoteAuthStatus` (Unauth/PermDenied) ⇒ `{SubmittedUnknown, AuthenticatedPeer,
  ProbeRequired}`, `proves_..()==false`; a WAF/garbage body ⇒ `NoResponse(CallFailedWithoutTrustedDpsEnvelope)`
  (CS-3 3.2 A′ realized — **not** a `RemoteStatus` arm); + the AM-2 pin that the pre-A′ incumbent yielded
  `NoResponse` for the TLS-proven wire too.
- **RP4B-5 (F6/B7)** binding: immutable Rust type; DB mutation reject (`032:170-187`); PB-2 **full-tuple** equality
  (same `protocol_id`, different `contract_version` ⇒ rejected at creation); **wrong-port `submit_raw` ⇒
  `PortBindingMismatch` with ZERO wire calls** (mock asserts no send); global profile-flip does not affect a bound doc.
- **RP4B-6 (compile-fail canary)** a fenced / `SubmittedUnknown` reservation cannot become an `AuthorizedSubmission`
  (mint is engine-private, RN-only); `submit_raw` cannot be reached with an unbound envelope (`BoundSignedEnvelope`
  has a closed ctor); `set_routing(HydratedRetryClass::DrainChainSettleRetry)` does not typecheck (only
  `ActiveRetryClass` is accepted, D9).
- **RP4B-7 (B5)** a foreign-doc `last_chk` (`ack.id != expected_id`) ⇒ `ReconcileValidation::Mismatch`; a `BySign`
  FN-level reply ⇒ `Unattributed` (attributes to no reservation) — **never** a fabricated `ReconciliationObservation`;
  cross-protocol recon of a protocol-A doc is never invoked; a DAG gate that `prro-engine` names no concrete adapter.
- **RP4B-8 (no wire-resend — D4)** a fenced `SubmittedUnknown` doc has no code path reaching `authorize_submission`
  / `submit_authorized`; the only fenced paths are read-only `probe` / STOP / HOLD / operator (Spec #2 §5).
- **RP4B-9..17** stale-`node_generation` observation dropped (AO-1, CS-3 apply); a second acceptance/quittance does
  not double-apply (AL-2/FA); `sent_accepted("")` ⇒ `None`; a short `data_sign` on a matched `last_chk` ⇒
  `IdMatchedNoQuittance` keeping `Submitted` (not a regress); `ReconciliationCapability::None` ⇒ immediate RMR; all
  8 `RetryClass` round-trip; malformed digest fixed size; a dropped `submit` future never becomes `NotSubmitted`
  (SE-1).

## 11 · Scope boundaries
- **CS-3:** the `evidence → three fields` classifier + `set_routing`; 032 activation + `ObservedOutcomeV1` +
  `node_state.delivery_generation` + record-then-apply (A4-6); the atomic `authorize_submission` tx + the
  durable-state witness (co-located with the mint, §6); the minimal incumbent gRPC seam (`-4` at `dto.rs:215`;
  `RemoteStatus`/`AuthenticatedPeer` separable). **The `-12` MAC-recovery re-sign** = a CS-3 **new-attempt** edge
  (a fresh reservation with new bytes/hash), **requiring a locked-spec amendment** to Spec #2 §5 / #4A A4-6 to
  admit a corrective new-attempt for a fenced doc — explicitly NOT a #4B type. **CS-6:** concrete adapters +
  `EVPZ_DPS` `FiscalAcceptance` variant + crate-DAG gate. **Spec #3:** ingress inbox. **CS-4:** coordinator.

## 12 · Resolved rulings (auditor) + status
- **R1** `-2/-15` → `SendIndeterminate::CloseAmbiguous` for close / Z-report; `DpsReject::Close` for other doc
  types (§5). **R2** PB-2 invariant + negative pin = #4B; atomic tx = CS-3; **full tuple**. **R3** homes
  (auditor instruction #2): `prro-domain` = IDs / binding / semantic axes / `RetryClass` (relocation UNBUILT — R4) +`ActiveRetryClass`+
  `HydratedRetryClass`; `prro-dps-contract` = wire-observation types + the **raw** `DpsPort` traits
  (`submit_raw`, `probe`, status/codes) + `validate_reconcile`; **the `AuthorizedSubmission` token +
  `authorize_submission` mint + the `submit_authorized` wrapper live in the engine delivery module** (private
  ctor). Honest split: token-mint = Rust-privacy-proven engine-only; no-direct-`submit_raw` = DAG/review. **R4 (CS-3 3.2 recon — PARTIAL):**
  `ActiveRetryClass` (fresh-write) and `HydratedRetryClass` (decode) are distinct types and **realized**
  (prro-domain `mod.rs:270`); but the `RetryClass`→`prro-domain` relocation + compat re-export +
  `From<ActiveRetryClass>` + `set_routing` are **UNBUILT** — `RetryClass` stays in prro `error_routing.rs:69`.
  Carried as a Bridge/D routing-store work item (§2(c)).
- **Corrective-resend / `-12`:** **deferred out of #4B** (D4, §11) — a CS-3 new-attempt edge + a locked-spec
  amendment. This is the one place #4B says "not here, and here is exactly what CS-3 must additionally lock."

---
*Grounded/verified this pass on `origin/main`: Spec #2 §1-§10 + §5 fenced-ops (`spec2…fsm.md:65`) ·
`spec4-authority-minilock.md:19-35,180-190` (A4-6 wire-call never repeated; A4-2 `node_state.delivery_generation`
= CS-3 token; RP-A4-6 no blind resend) · `channel.rs:21-108` (verbatim `DpsChannel` + `by_server_fiscal_no`
correlation) · `kvt2_advance.rs:118-140` (two envelopes) · `kvt2_confirm.rs:314` + `boot_phase.rs:755`
(`data_sign>=64` gate; `id_sign` unused) · `stage_send.rs:1570-1572` + `:795` · `error_routing.rs:69,269-271` +
no `-4` arm · `dto.rs:66,215` · `grpc.rs:127-128` · `032_delivery_reservation.sql:74-187` (no `generation`
column) · plan `161-163`/`250-254`.*
