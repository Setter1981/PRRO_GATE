# CS-3 Bridge-0.1 (3.2) — transport/engine seam + honest decoded-content digest

**Status:** DRAFT **rev 3** for adversarial gate. rev 1 → NOT-YET (5 class-B, all closed in
rev 2); rev 2 → NOT-YET (6 class-B, closed here). Point-fix, no redesign. **Base:**
`origin/main 2dbae3c`. **Predecessors:** #4B rev-6, #4A A4-6,
[[project_digest_decoded_content_decision]], `project_cs3_bridge0_foundation_repair`.

**Scope pin (foundation-only).** 3.2 ends at a *pure, read-only* derivation
> `RawSendReply (+ store doc_type) → SendResponse → ClassifiedOutcome`
built **alongside** the live `CheckAck`/`DpsError`/`route_dps_error` path, which keeps driving
production. 3.2 mints **no** `ObservedOutcomeV1`, calls **no** `record` (needs D's
store-minted `AuthorizedGeneration`), changes **no** routing, applies **no** behaviour delta,
retires nothing, and does **not** touch the live `-12` second wire. Authoritative record,
routing cutover, `DpsChannel → DpsSubmissionPort`, and blind-resend kill are **Bridge + D/E**.

All file:line anchors on `2dbae3c`, re-verified by the author (rev-1/rev-2 Class-A corrected).

---

## §1 Problem

The digest can't be locked apart from **who mints it**, **which returned branches carry it**,
**where fabrication is forbidden**, and a **crate/module placement Rust actually enforces** —
and the transport must yield the shadow evidence from the **same single RPC** as the live
reply, or the implementer double-issues. Today the reply is split across two hierarchies, the
digest is partly absent / mislabeled "raw" / **publicly fabricable**, `SendResponse` is a
**public** enum, and the digest **cannot even be minted in `prro-domain`** (purity-gate bans
`prost`/`tonic`). 3.2 unifies the *returned* reply into one sealed type feeding the domain
classifier, read-only.

**Non-goal (containment §7):** byte-exact wire proof (custom tonic codec) is a future forensic
slice — its absence makes the rev-1 `AuthenticatedPeerGarbage` branch unreachable (removed §4.3).

---

## §2 Current architecture (grounded; rev-1/rev-2 Class-A corrected)

**Crate graph (blocker B1/B2).** `prro-domain` is a **separate, pure crate**: its ONLY deps
are `{uuid, serde}` and the `purity_gate` (purity_gate.rs:46) **forbids** `prost, tonic, tokio,
sqlx, …`. So the digest **framing+SHA cannot live in the domain**. The transport
(`prro/src/transports/dps/*`) and engine (`prro/src/services/*`) both live in the **`prro`**
crate — so cross-crate privacy stops *other crates*, never sibling modules within `prro`.

**Two parallel hierarchies:**

- **Transport → live path.** `grpc.rs` methods (`send_chk` :202, `last_chk` :213, `status_rro`
  :235, `info_rro` :246) → `map_tonic_status(status, peer_auth)` (grpc.rs:166) +
  `try_decode_{check,status,rro_info}_response` (dto.rs:198 / :250 / **:368**) → `Result<CheckAck,
  DpsError>`. `CheckAck` (dto.rs:66) = `{id, id_sign, data_sign}`, no digest; empty id rejected
  post-wire (stage_send.rs:1583). `DpsError` (error.rs:14) = **10 variants, not
  `#[non_exhaustive]`**. `route_send_result` (error_routing.rs:263) → `WireDecision`;
  `route_dps_error` (:289, exhaustive) → `RoutingDecision` (:58); `route_server_code` (:426).
  `StageSendOutcome` (stage_send.rs:573); `extract_wire_forensics` (:866) projects
  RemoteStatus/Indeterminate → `"Transport"`; the live `-12` handler reads the **full**
  `DpsError::Server{message}` for `mac_recovery_hint` (error_routing.rs:253).

- **Domain algebra — sealed *outcome*, UNSEALED *response*, zero prod consumers.** `SendOutcome`
  (mod.rs:346) is opaque. **`SendResponse` (mod.rs:321) is a PUBLIC enum with PUBLIC variants**
  (Class-A). Its digest-bearing carriers are **incomplete**: `SendIndeterminateInner`
  (mod.rs:519) = `UnknownStatus{code,digest}`, `SaveError` (**no digest**), `CloseAmbiguous`
  (**no digest**), `OkButNoFiscalNumber{digest}`; `SendOutcomeInner::Rejected(DpsReject)` where
  `DpsReject` (mod.rs:477) is **payload-less** (**no digest**). `classify` (mod.rs:702, 1-arg),
  `ObservedOutcomeV1`/`record` (mod.rs:919/949) — unwired.

**Digest today.** `RawResponseDigest(pub [u8;32])` — public field (mod.rs:109). `response_digest`
(dto.rs:178) = `SHA-256(prost.encode_to_vec())` (decoded; self-contradictory "re-encode" +
"lossless raw" doc); `status_digest` (grpc.rs:188) = `SHA-256(code‖0‖message‖0‖details)` — a
different kind, same type, **no** versioned/length-prefixed framing. **Zero-digest sites = 15
(10 in `/src/` + 5 fixtures)** (rev-2 Class-A: was "11+4").

**doc_type store-owned.** `fiscal_documents.doc_type` → `fetch_send_inputs_tx`
(**fiscal_documents.rs:1909**, callsite **stage_send.rs:1248**) → engine. Never from wire.

**Invariants (§5).** R1 TLS `PeerAuth` (grpc.rs:39/:102/:169); network-outside-tx (stage_send.rs
:1562 "4a", tx.rs:65 + syn-scan); transport identity-blind (`CheckEnvelope` dto.rs:32).

---

## §3 Sequence (one spec → whole-composition gate → 5 sub-PRs)

Additive; ends at the pure derivation.

1. **contract/digest types + ownership** — `DecodedResponseDigest` + `GrpcStatusDigest` (D-1,
   §4.1, opaque `[u8;32]` in domain, framing+SHA in transport); **opaque** `SendResponse`;
   **digest fields added** to `Rejected`/`SaveError`/`CloseAmbiguous`/`MissingStatus` carriers
   (§4.1b); delete `RawResponseDigest` (D-3). **Not byte-neutral** (Debug string change,
   e.g. kvt2_confirm `{err:?}`) — declared + pinned.
2. **single-RPC total transport evidence** — the seam `send_chk_observed` (§4.2) does **one**
   physical call + **one** decode, yielding BOTH the legacy `Result<CheckAck,DpsError>` and a
   total `RawSendObservation`; wire-count canary. `RawSendReply` opaque struct + private inner
   in `transports::dps`.
3. **all-consumers propagation + old→target pair graph** — audit every consumer; publish the
   authoritative **old→target routing pair graph** (§4.6) — the drift-pin oracle; note (don't
   apply) catch-all hardening.
4. **engine-owned pure mapping** — engine joins `RawSendReply` + store `doc_type` →
   `SendResponse` → `classify` → `ClassifiedOutcome`, **read-only, alongside** the live path;
   drift-pin cross-checks against the §4.6 pair graph (equality on unchanged rows, exact pair on
   deltas). No `ObservedOutcomeV1`, no `record`, no routing change.
5. **integration teeth + checkpoint** — §6; re-checkpoint before Bridge.

---

## §4 Contracts & tables (load-bearing)

### 4.1 Sealed digest types — implementable under the purity-gate (blocker B2, B1)

Domain holds an **opaque 32-byte value only** (no `prost`/`tonic`/`sha2` — purity-gate):

```text
// prro-domain::delivery — opaque wrappers, private field, NO hashing here
struct DecodedResponseDigest([u8;32] /* private */)   // D-1
struct GrpcStatusDigest([u8;32] /* private */)
impl each { fn from_transport_digest(bytes:[u8;32]) -> Self  /* the ONLY ctor, source-gated */
            fn as_bytes(&self) -> &[u8;32] }
```

- **Framing + SHA live in `prro::transports::dps`** (which already deps `sha2`/`prost`). The
  transport computes the framed hash and calls `from_transport_digest(bytes)`.
- **Byte-exact framing (normative, blocker B5):**
  ```
  digest = SHA-256(  DOMAIN_TAG                      // b"PRRO-DPS-DIGEST\x01"  (16 bytes, literal)
                   ‖ msg_type : u8                   // Check=0x01 Status=0x02 RroInfo=0x03 GrpcStatus=0x10
                   ‖ schema_version : u32 big-endian // DpsProtocolBinding.contract_version
                   ‖ for field in FIXED ORDER (proto field-number ascending; gRPC: code,message,details):
                        len(field) : u32 big-endian ‖ field_bytes )
  ```
  Encoding: integer scalars → `i64` big-endian (8 bytes); `string`/`bytes` → raw bytes; every
  field length-prefixed (`u32` be). No ambiguous concatenation. Claim: **collision-resistant
  fingerprint of the KNOWN decoded content** — NOT "distinct wire replies always differ"
  (unknown fields / encoding quirks are out of scope, §1).
- **No default/zero/synthetic:** no `Default`, no public field, no `[0u8;32]`. The 15 zero sites
  rewire to a real content minter exposed via **`prro-testkit`** (`testkit::decoded_digest_of
  (msg_type, schema, &[fields])`), reachable from both prro-domain unit tests and `prro/tests/*`
  — **not** `#[cfg(test)]` in prro-domain (unreachable downstream).

### 4.1b Digest must survive into the domain carriers (blocker B3)

Today `Rejected(DpsReject)`, `SaveError`, `CloseAmbiguous` are digest-less, so branches 3–5
would drop the digest before any carrier — making §6.5 untestable. 3.2 gives every
digest-bearing branch a carrier field:

```text
SendOutcomeInner::Rejected { verdict: DpsReject, digest: DecodedResponseDigest }
SendIndeterminateInner:: UnknownStatus { code, digest }      // exists
                       | SaveError      { digest }            // NEW field
                       | CloseAmbiguous { digest }            // NEW field
                       | MissingStatus  { digest }            // NEW variant (D-2, status==0)
                       | OkButNoFiscalNumber { digest }        // exists
SendOutcomeInner::Accepted(SentAccepted)                      // NO digest (D-4)
```

`from_dps_status` threads the transport-minted digest into each digest-bearing variant;
`Accepted` carries none (D-4). `.kind()` views expose the digest read-only.

### 4.2 Single-RPC fan-out seam (blocker B4) + total evidence

```text
// prro::transports::dps — ONE physical wire call, ONE decode, TWO projections
fn send_chk_observed(env: CheckEnvelope) -> (Result<CheckAck, DpsError>, RawSendObservation)

struct RawSendReply(RawSendReplyInner)      // OPAQUE; private inner; module-sealed to transports::dps
enum RawSendReplyInner {
  Accepted { fiscal_id: NonEmptyId },                          // OK + non-empty id; NO digest
  OkNoFiscalId { digest: DecodedResponseDigest },              // OK + empty id
  ServerCode  { raw_code: i32, digest: DecodedResponseDigest },// non-OK, non-zero code
  MissingStatus { digest: DecodedResponseDigest },             // status == 0 (D-2)
  RemoteAuthStatus { grpc: GrpcStatusDigest },                 // TLS-proven Unauth/PermDenied
  NoResponse { cause: NoResponseCause },                       // NO digest
}
struct RawSendObservation { evidence: RawSendReply, diagnostics: WireDiagnostics }
struct WireDiagnostics { status_code: Option<i32>, grpc_code: Option<String>,
                         message: Option<BoundedText> }        // SHADOW forensic only (see B6 note)
```

`send_chk_observed` performs exactly one `dps_channel` call and one decode, then projects the
**same** decoded reply into (legacy, raw). The legacy tuple keeps driving production
unchanged; `RawSendObservation` feeds the read-only shadow. `NoResponseCause` (domain) gains
**`CallFailedWithoutTrustedReply`** (untrusted reply: plaintext Unauth/PermDenied, post-connect
failure, non-DPS status over Unproven — *not* a genuine local absence; classifies
`SubmittedUnknown`).

### 4.3 Normative mapping: returned observation → `RawSendReply` → `SendResponse`

Total over **returned observations**. Engine joins store `doc_type` only in the last column.

| # | returned observation | `RawSendReply` | digest | → `SendResponse` (engine + doc_type) |
|---|---|---|---|---|
| 1 | `Response` OK, id non-empty | `Accepted{fiscal_id}` | none | `Parsed(Accepted)` |
| 2 | `Response` OK, id empty | `OkNoFiscalId{d}` | content | `Parsed(OkButNoFiscalNumber{d})` |
| 3 | `Response` named code (-1,-5,-6,-7..-10,-11,-12,-13,-14,-16) | `ServerCode{code,d}` | content | `Parsed(Rejected{verdict, d})` |
| 4 | `Response` code -2/-15 | `ServerCode{code,d}` | content | by `dt`: close/Z → `CloseAmbiguous{d}`; else → `Rejected{Close, d}` |
| 5 | `Response` code -3 | `ServerCode{-3,d}` | content | `Parsed(SaveError{d})` |
| 6 | `Response` unknown **non-zero** i32 | `ServerCode{code,d}` | content | `Parsed(UnknownStatus{code,d})` |
| 7 | `Response` **status == 0** | `MissingStatus{d}` (D-2) | content | `Parsed(MissingStatus{d})` → ProbeRequired |
| 8 | gRPC Unauth/PermDenied, **TlsProven** | `RemoteAuthStatus{g}` | grpc | `RemoteStatus(RemoteAuthStatus(g))` |
| 9 | untrusted reply: plaintext Unauth/PermDenied · post-connect failure · non-DPS status over Unproven | `NoResponse{CallFailedWithoutTrustedReply}` | none | `NoResponse(CallFailedWithoutTrustedReply)` |
| 10 | genuine absence: timeout · cancel · local-handshake fail | `NoResponse{Timeout\|Cancelled\|LocalHandshakeFailure}` | none | `NoResponse(cause)` |

**Not in this table (blocker B3):**
- **`AuthenticatedPeerGarbage` — removed entirely** (not "future-unpopulated"): tonic collapses a
  prost decode failure to `Status::Internal`, indistinguishable from a server `Internal`, raw
  body lost. The `RemoteStatusEvidence` variant is deleted until the future codec (§1).
- **`CrashedBeforeObservation` is an engine boot-recovery edge**, not a returned observation:
  boot mints `SubmissionEvidence::Started{NoResponse(CrashedBeforeObservation)}` from durable
  `CALL_STARTED`. (Row 10 is genuine-absence-with-a-completed-call; it does **not** cover crash.)

**Digest-per-branch:** 2–7 → `DecodedResponseDigest`; 8 → `GrpcStatusDigest`; **1, 9, 10 → no
digest field** (type-level).

### 4.4 Placement + sealing — Rust where possible, source-gate where not (blocker B1)

| item | crate::module | vis | who constructs | enforced by |
|---|---|---|---|---|
| `DecodedResponseDigest`/`GrpcStatusDigest` | `prro-domain::delivery` | private field, sole `from_transport_digest` | transport seam | cross-crate privacy (other crates) **+ workspace source-gate** (transport-only within prro) |
| `RawSendReply` (opaque) + inner | `prro::transports::dps` | private inner | that module | **module privacy** (compile-time; sibling engine cannot construct) |
| `SendResponse` (now opaque) + `SendOutcome`/`SentAccepted` authority ctors | `prro-domain::delivery` | private inner; `from_dps_status`/`no_response`/`remote_status`/`observe` are `pub` | ONE engine mapper | privacy stops *literals*; **the workspace source-gate is the real fence** |

**Precise guarantee (blocker B1, corrected).** Rust privacy forbids external *literals* but
**not calls** to the public authority ctors (`from_dps_status`, `SendResponse::{no_response,
remote_status}`, `SentAccepted::observe`). `Accepted` is the sharpest gap — it has **no digest**,
so the digest source-gate cannot see it. Therefore the **source/AST allowlist gate is
workspace-wide over ALL authority ctors AND the digest mint**, permitting exactly **one** engine
mapper callsite (e.g. `services/write_path/shadow_map.rs`) plus the transport mint site; every
other `services/*` callsite is RED. The engine is the *sanctioned mapper* — it constructs
`SendResponse` — but can only carry the transport-minted digest read out of the opaque
`RawSendReply` (it never holds a constructible digest to inject). Nothing rests on external
trybuild proving sibling sealing (teeth §6.3/6.4).

### 4.5 Single doc_type source

`fiscal_documents.doc_type` → `fetch_send_inputs_tx` (fiscal_documents.rs:1909 / stage_send.rs:1248)
→ engine → `from_dps_status(raw, store_doc_type, digest)`. `RawSendReply` never carries doc_type.

### 4.6 Old→target routing pair graph (blocker B5 — the drift-pin oracle)

3.2 applies **none** of these; this is the authoritative graph the §6.10 drift-pin checks:
**equality** on unchanged rows, **exact (old,target) pair** on declared deltas. "Behaviour-neutral"
in 3.2 means **the shadow does not drive production state** (read-only) — *not* "its result equals
the incumbent".

| input | live `RoutingDecision` (unchanged in 3.2) | shadow-derived `ClassifiedOutcome` (target) | drift-pin |
|---|---|---|---|
| named rejects -1/-5/-7../-16, -13/-14, -11, -12, -6 | as today (error_routing.rs:426) | same routing class + node_effect | **equal** |
| TLS `RemoteStatus` | TransientRetry (compat, :314) | **ProbeRequired** | exact pair (delta) |
| unknown non-zero code | `Decode`→ProbeRequired *(via -4=Indeterminate)* / Server→… | `UnknownStatus`→**TransientRetry** | exact pair (delta) |
| `MissingStatus` (status 0) | `Decode`→ProbeRequired | `MissingStatus`→ProbeRequired | **equal** |
| OK + empty id | stage_send `EmptyServerFiscalNo` guard (post-wire) | `OkButNoFiscalNumber`→ProbeRequired | exact pair (not equal to the guard) |

The live `mac_recovery_hint`/audit event/severity/probe-reason are **not** authority in
`ClassifiedOutcome`; the live path keeps reading them from `DpsError` (unchanged).

---

## §5 Invariants (must not weaken)

1. **R1 TLS provenance** — branch 8 reachable ONLY when `TlsProven` (grpc.rs:169); plaintext →
   branch 9 (no digest).
2. **Network outside SQLite tx** — `send_chk_observed` runs in "4a" (stage_send.rs:1562); one
   physical call; digest/decode never inside a tx; `assert_not_in_with_immediate` + syn-scan intact.
3. **Engine cannot inject a fabricated digest** — via §4.4: opaque `RawSendReply` (module
   privacy) + workspace source-gate over all authority ctors + digest mint. The engine is the
   sanctioned mapper; it carries transport-minted digests only.
4. **Transport has no reservation identity** — `RawSendReply`/`RawSendObservation`/`CheckEnvelope`
   carry no `reservation_id`.
5. **No 3.2 behaviour delta / no authoritative record** — no production routing change, no
   `ObservedOutcomeV1`, no `record` (needs D), no `-12` second-wire change. PR4 is read-only,
   cross-checked against the §4.6 pair graph. Deltas land at Bridge.
6. **Exactly one wire call per stage-4 attempt** — `send_chk_observed` is one physical RPC;
   the shadow must never trigger a second DPS call (blocker B4). Wire-count canary (§6.11).

---

## §6 Teeth (empirical; revert → RED)

1. **Exhaustive mapping** — table over §4.3 (every returned-observation branch × doc_types);
   missing/renamed branch fails. The two removed cases asserted as unreachable/engine-edge.
2. **trybuild: cross-crate literal forbidden** — `DecodedResponseDigest([0;32])`,
   `SendResponse::Parsed(_)` **literal** from another crate must not compile.
3. **source-gate: transport-only mint (B1/B2)** — syn-scan: `from_transport_digest` and the
   framing SHA appear ONLY under `transports::dps`. A call elsewhere → gate RED.
4. **source-gate: one engine mapper (B1)** — the authority ctors (`from_dps_status`,
   `SendResponse::{no_response,remote_status}`, `SentAccepted::observe`) and `RawSendReply`
   construction appear ONLY at the single allowed mapper + transport. **Direct `Accepted` /
   `SendResponse` from any other `services/*` → RED.**
5. **same-digest propagation + independent golden preimage (B5)** — digest read off the carrier
   equals the seam value; PLUS recompute `SHA-256(framing(fields))` independently in the test and
   assert `== carrier.digest`; a constant/re-framed/mismatched digest → RED. Proves
   `digest == H(framing(fields))`, not "stable".
6. **unknown-code + digest** — non-zero unknown → `ServerCode{code,d}` → `UnknownStatus{code,d}`
   preserving both; drop either → RED.
7. **rejected/save/close carry digest (B3)** — `Rejected{verdict,d}`, `SaveError{d}`,
   `CloseAmbiguous{d}`, `MissingStatus{d}` each round-trip the seam digest; a digest-less variant
   → won't compile / RED.
8. **empty-id + digest** — OK+empty → `OkButNoFiscalNumber{real digest}`; unconstructible without
   a transport-minted digest.
9. **no-response-has-no-digest** (renamed from rev-1 "wrong-binding→zero-wire", B5) — branches
   1/9/10 have **no digest field** (type-level). *Does not count wire calls*; the wire-binding pin
   (RP4B-5) stays **Bridge**.
10. **drift-pin over the §4.6 pair graph (B5)** — equality on unchanged rows, exact (old,target)
    on delta rows; a derived class that neither equals nor matches its declared pair → RED. (Not
    "derived==live" universally.)
11. **TLS/plaintext matrix + wire-count (B4)** — TlsProven Unauth → branch 8; plaintext → branch
    9; flip guard → RED (#322). Plus: `send_chk_observed` issues exactly **one** wire call
    (mock counter) — a second call → RED.

---

## §7 Containment

- **Foundation-only, additive.** Built beside the live path, which drives production unchanged.
  No routing change, no `ObservedOutcomeV1`, no `record`, no retirement, no behaviour delta.
- **No partial cutover.** Port swap, authoritative `record` (needs D), and `-12` blind-resend
  kill are Bridge + D/E. 3.2 ends at PR5's checkpoint, old path intact.
- **Each sub-PR reverts independently.** PR1 is **not** byte-neutral (Debug string change, D-3) —
  declared + pinned; PR2/PR4 additive/read-only; PR3 doc+pins; PR5 teeth.

## D-1…D-4 (adopted)

- **D-1** `DecodedResponseDigest`. **D-2** dedicated `MissingStatus`→ProbeRequired.
- **D-3** delete `RawResponseDigest` in PR1; **not** byte-neutral (Debug strings) — declared+pinned.
- **D-4** `Accepted` no digest — GO (opaque `RawSendReply` + sealed `NonEmptyId`; reconciliation
  uses the fiscal id + a later `last_chk`, not a send-response fingerprint).

## B6 note — MAC-hint (blocker B6)

`WireDiagnostics.message: Option<BoundedText>` truncates at 512 bytes; the live `-12`
`mac_recovery_hint` regex needs the **full** message. **Resolution:** the live path is unchanged
in 3.2 — it keeps reading the full `DpsError::Server{message}` for its hint. `WireDiagnostics`
is a **shadow** sidecar only; extracting a typed fixed-size MAC hint *before* truncation is
**deferred to D/E** (where the shadow becomes authoritative). rev-2's "feeds the existing hint"
claim is **retracted**.

## Class-A corrections (cumulative)

`SendResponse` was not sealed (public enum) — opaque in PR1 · zero-digest sites = **15 (10 in
`/src/` + 5 fixtures)** · `try_decode_rro_info_response` = **dto.rs:368** · `fetch_send_inputs_tx`
= **fiscal_documents.rs:1909 / stage_send.rs:1248** · row 10 is a completed-call genuine absence
(crash/dropped-future belongs to boot recovery, not a returned observation) · digest mint cannot
live in prro-domain (purity-gate bans prost/tonic).
