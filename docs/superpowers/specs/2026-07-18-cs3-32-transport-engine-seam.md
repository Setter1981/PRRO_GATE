# CS-3 Bridge-0.1 (3.2) — transport/engine seam + honest decoded-content digest

**Status:** DRAFT **rev 6** for adversarial spot-gate. class-B per round: 5 / 6 / 5 / 4 / 3 (rev-5's
3 closed here). Point-fix, no redesign. **Base:** `origin/main 2dbae3c`.
**Predecessors:** #4B rev-6, #4A A4-6, [[project_digest_decoded_content_decision]],
`project_cs3_bridge0_foundation_repair`.

**Scope pin (foundation-only).** 3.2 ends at a *pure, read-only* derivation
> `RawSendReply (+ store doc_type) → SendResponse → ClassifiedOutcome`
built **alongside** the live `CheckAck`/`DpsError`/`route_dps_error` path, which keeps driving
production. 3.2 mints **no** `ObservedOutcomeV1`, calls **no** `record` (needs D's store-minted
`AuthorizedGeneration`), changes **no** routing, applies **no** behaviour delta, retires
nothing, and does **not** touch the live `-12` second wire. Authoritative record, routing
cutover, `DpsChannel → DpsSubmissionPort`, and blind-resend kill are **Bridge + D/E**.

All file:line on `2dbae3c`, re-verified by the author each round.

---

## §1 Problem

The digest can't be locked apart from **who mints it**, **which returned branches carry it**,
**where fabrication is forbidden** (Rust cannot sibling-seal within one crate), a **placement
that respects the domain purity-gate** (no `prost`/`tonic` in `prro-domain`), and a **single-RPC
seam** (or the implementer double-issues). And the outcome constructor must be **total without a
fictitious/optional digest** (`Accepted` carries none). 3.2 unifies the *returned* reply into
one sealed type feeding the domain classifier, read-only.

**Non-goal (containment §7):** byte-exact wire proof (custom tonic codec) is a future forensic
slice — hence `AuthenticatedPeerGarbage` is unreachable and removed (§4.3).

---

## §2 Current architecture (grounded; Class-A cumulative)

**Crate graph.** `prro-domain` is a **separate, pure crate**; its direct deps are exactly
**`{serde, thiserror, uuid}`** and `purity_gate` (purity_gate.rs:46) **forbids** `{sqlx, tonic,
tokio, axum, prost, hyper, reqwest}`. So the digest **framing+SHA cannot live in the domain**.
The transport (`prro/src/transports/dps/*`) and engine (`prro/src/services/*`) both live in
**`prro`** — cross-crate privacy stops other crates, never sibling modules within `prro`.

**Two parallel hierarchies:**

- **Transport → live path.** `grpc.rs` methods (`send_chk` :202 …) → `map_tonic_status` (grpc.rs:166)
  + `try_decode_{check,status,rro_info}_response` (dto.rs:198 / :250 / **:368**) →
  `Result<CheckAck, DpsError>`. `CheckAck` (dto.rs:66) has no digest; empty id rejected post-wire
  by the **`EmptyServerFiscalNo` guard** (stage_send.rs:1583) — a distinct outcome, **not** a
  `RoutingDecision`. `DpsError` (error.rs:14) = 10 variants, **not `#[non_exhaustive]`**.
  `route_dps_error` (error_routing.rs:289, exhaustive) → `RoutingDecision` (:58);
  `route_server_code` (:426). A **truly-unknown non-zero code fails `Status::try_from` → `Decode`
  → ProbeRequired**; `-4` decodes to `Indeterminate → TransientRetry`; a *defensive*
  `Server{unknown i32}` → `WrapperBug` (**not** a returned decoder observation). The live `-12`
  handler reads the **full** `DpsError::Server{message}` for `mac_recovery_hint` (error_routing.rs:253).

- **Domain algebra — sealed *outcome*, UNSEALED *response*, zero prod consumers.** `SendOutcome`
  (mod.rs:346) opaque. **`SendResponse` (mod.rs:321) is a PUBLIC enum** (Class-A). `SentAccepted`
  (mod.rs:450) has a private field + `observe(id)->Option` (payload only — cannot be wrapped into
  the private `SendOutcomeInner::Accepted` from another module). Digest carriers were incomplete
  (fixed §4.1b). `from_dps_status(RawDpsStatus, DocType, RawResponseDigest)` (mod.rs:395) requires
  a digest **always**, even for `Accepted` (the B1 dishonesty). `classify` (mod.rs:702, 1-arg).

**Digest today.** `RawResponseDigest(pub [u8;32])` (mod.rs:109); `response_digest`
(dto.rs:178, prost re-encode, self-contradictory doc); `status_digest` (grpc.rs:188), no
versioned framing. **Zero sites = 15 (10 in `/src/` + 5 fixtures).**

**Proto reply messages (authoritative: `prro/proto/fiscal_server.proto:36-112`).**
`CheckResponse{ id:str#1, status:i32#2, id_sign:bytes#3, data_sign:bytes#4, error_message:str#5 }`;
`StatusResponse{ open_shift:bool#1, online:bool#2, last_signer:str#3, status:i32#4, error_message:str#5 }`;
`RroInfoResponse{ status:i32#1, status_rro:i32#2, open_shift:bool#3, online:bool#4, last_signer:str#5,
name:str#6, name_to:str#7, addr:str#8, single_tax:bool#9, offline_allowed:bool#10, add_num:i32#11,
pn:str#12, operators: repeated Operator#13, tins:str#14, lnum:i32#15, name_pay:str#16 }`;
`Operator{ serial:str#1, status:i32#2, senior:bool#3, isname:str#4 }`. The gRPC `Status` (branch 8)
is `{ code, message, details }` (tonic). All four tables are normative for §4.1 framing.

**doc_type store-owned.** `fiscal_documents.doc_type` → `fetch_send_inputs_tx`
(**fiscal_documents.rs:1909** / **stage_send.rs:1248**). **Invariants (§5):** R1 TLS (grpc.rs:39/:102/:169);
network-outside-tx (stage_send.rs:1562, tx.rs:65 + syn-scan); transport identity-blind (dto.rs:32).

---

## §3 Sequence (one spec → whole-composition gate → 5 sub-PRs)

1. **contract/digest types + ownership** — opaque `DecodedResponseDigest`/`GrpcStatusDigest` in
   domain (§4.1, `pub from_transport_digest`, framing+SHA in transport); **opaque** `SendResponse`;
   digest carrier fields (§4.1b); **total `ParsedReply` input** + `from_parsed` (§4.1c); delete
   `RawResponseDigest` (D-3, not byte-neutral — Debug strings, pinned).
2. **single-RPC total transport evidence** — `send_chk_observed` (§4.2): one call + one decode →
   `(legacy, RawSendObservation)`; opaque `RawSendReply`.
3. **all-consumers + old→target pair graph** — the total normalized §4.6 graph (drift oracle).
4. **engine-owned pure mapping** — `RawSendReply` + store `doc_type` → `SendResponse` → `classify`
   → `ClassifiedOutcome`, read-only; drift-pin over §4.6.
5. **integration teeth + checkpoint.**

---

## §4 Contracts & tables (load-bearing)

### 4.1 Sealed digest types under the purity-gate (B2) + normative framing (B5, B2)

```text
// prro-domain::delivery — opaque 32-byte wrappers, private field, NO hashing (purity-gate)
struct DecodedResponseDigest([u8;32] /*private*/); struct GrpcStatusDigest([u8;32] /*private*/);
impl each { pub fn from_transport_digest(bytes:[u8;32]) -> Self;   // MUST be pub (cross-crate call)
            pub fn as_bytes(&self) -> &[u8;32]; }                  // — the source-gate is the fence, not privacy
```

**Framing + SHA in `prro::transports::dps` only.** Fixed version (B2 — transport has no
`contract_version`):
```
DIGEST_FRAMING_VERSION : u8 = 1
digest = SHA-256(  b"PRRO-DPS-DIGEST"          // 15-byte literal tag
                 ‖ DIGEST_FRAMING_VERSION : u8  // = 1
                 ‖ msg_type : u8                // CheckResponse=0x01 StatusResponse=0x02 RroInfoResponse=0x03 GrpcStatus=0x10
                 ‖ block(fields...) )
block(fields) = for each field in the message's FIXED table (field-number ascending):
                  len(enc(field)) : u32 be ‖ enc(field)
enc(bool)     = 1 byte (0x00 | 0x01)
enc(i32|i64)  = i64 big-endian, 8 bytes (sign-extended)
enc(string|bytes) = raw bytes
enc(repeated<T>)  = count : u32 be ‖ for each elem: len(block(elem.fields)):u32 be ‖ block(elem.fields)
enc(nested msg)   = block(nested.fields)   // recursive, no tag/version prefix
```
Per-message field tables (fixed order) = the §2 proto lists **in full** — `CheckResponse` (5
fields), `StatusResponse` (5), `RroInfoResponse` (**16**, incl. `operators#13` recursive +
`tins#14`/`lnum#15`/`name_pay#16`), and `GrpcStatus{code,message,details}` (msg_type `0x10`).
`Operator` framed recursively. **`GrpcStatus.code` is encoded as the canonical gRPC numeric code
(`tonic::Code as i32`) → `i64` big-endian — NOT the `Debug` string** the current `status_digest`
uses (grpc.rs:188); `message` and `details` are `enc(string)`/`enc(bytes)`. Claim: **collision-resistant fingerprint of the KNOWN decoded
content** — NOT "distinct wire replies always differ". PR1 covers **all four** messages, each with
golden preimage vectors (§6.5) — a reply differing only in a late/added field (e.g. `name_pay`) must
yield a distinct digest.

**No default/zero/synthetic:** no `Default`, no public field, no `[0u8;32]`. Test minter in
**`prro-testkit`** (`testkit::decoded_digest_of(msg_type,&[fields])`) — an **explicit test-only
allowlist entry** in the source-gate (§4.4, resolves the §4.1↔§6.3 conflict).

### 4.1b Digest survives into the domain carriers (B3 — closed)

```text
SendOutcomeInner::Rejected { verdict: DpsReject, digest: DecodedResponseDigest }
SendIndeterminateInner:: UnknownStatus{code,digest} | SaveError{digest} | CloseAmbiguous{digest}
                       | MissingStatus{digest} (NEW, D-2) | OkButNoFiscalNumber{digest}
SendOutcomeInner::Accepted(SentAccepted)   // NO digest (D-4)
```

### 4.1c Total input sum-type + transport-minted provenance (B1, B2)

> **AMENDMENT (2026-07-18, variant-3 adjudication — realized in PR4).** `ParsedReply` / `RepliedCode`
> / `from_parsed` below were **RETIRED**: the engine mapper reads `RawSendReply::kind()` **directly**
> (its 6-arm view is the strict superset — it also carries `RemoteAuthStatus`/`NoResponse`, which
> `ParsedReply` cannot express), and the code×doc_type table lives in `SendOutcome::from_server_code`.
> `from_dps_status` + `RawDpsStatus` are deleted. The provenance types (`NonEmptyFiscalNumber`,
> `NonOkStatusCode`) and the digest-only-where-it-exists principle stand as written. See
> `services/write_path/shadow_map.rs::map_send_reply` + PR4 pin A/B.

Replace `from_dps_status(RawDpsStatus, doc_type, digest)` (digest-always) with a **total** input
that carries a digest **only where one exists** (no `Option`/fictitious) and whose id/code carry
**provenance**, not just form — the engine can fabricate neither:

```text
// prro-domain, opaque + private field; the SOLE `from_transport(...)` ctor is source-gated to the decoder
struct NonEmptyFiscalNumber(String /*private, non-empty*/);  // proves the id came OFF a parsed reply
struct NonOkStatusCode(i32 /*private, != 0 && != 1*/);       // a real non-OK / non-UNKNOWN DPS code

struct ParsedReply(ParsedReplyInner);           // OPAQUE; minted ONLY by the transport decoder (§4.4 gate)
enum ParsedReplyInner {                          // private inner — engine cannot construct a variant
  Accepted { fiscal_id: NonEmptyFiscalNumber },                // NO digest
  Replied  { code: RepliedCode, digest: DecodedResponseDigest },
}
enum RepliedCode { OkEmptyId | ServerCode(NonOkStatusCode) | MissingStatus }  // 0⇒MissingStatus, 1⇒Accepted/OkEmptyId — never ServerCode
fn SendOutcome::from_parsed(reply: ParsedReply, doc_type: DocType) -> SendOutcome   // the engine mapper
```

`NonEmptyFiscalNumber`/`NonOkStatusCode` are **transport-minted** (their `from_transport` ctors are
source-gated to the decoder, §4.4) — so a non-empty id proves *provenance* (it came off the parsed
reply), not merely form, and `ServerCode` can never hold `0`/`1`. `from_parsed` maps
`Accepted→SendOutcome::Accepted` by **wrapping the already-validated `NonEmptyFiscalNumber`** — the
engine mapper does **not** call `SentAccepted::observe` (validation belongs to the transport mint) —
`OkEmptyId→OkButNoFiscalNumber{digest}`, `ServerCode→Rejected/CloseAmbiguous/SaveError/UnknownStatus`
by code×doc_type, `MissingStatus→MissingStatus{digest}`. No path builds `Accepted` with a bogus
digest or a fabricated id.

### 4.2 Single-RPC fan-out seam (B4) — total over tonic outcomes (B3-partition)

```text
async fn send_chk_observed(env: CheckEnvelope) -> (Result<CheckAck, DpsError>, RawSendObservation)
struct RawSendReply(RawSendReplyInner)      // OPAQUE; private inner; module-sealed to transports::dps
enum RawSendReplyInner {
  Accepted { fiscal_id: NonEmptyFiscalNumber } | OkNoFiscalId{digest} | ServerCode{code:NonOkStatusCode,digest}
  | MissingStatus{digest} | RemoteAuthStatus{grpc:GrpcStatusDigest} | NoResponse{cause}
}
struct RawSendObservation { evidence: RawSendReply, diagnostics: WireDiagnostics }
struct WireDiagnostics { status_code:Option<i32>, grpc_code:Option<String>, message:Option<BoundedText> } // SHADOW only
```

`send_chk_observed` does **exactly one** physical `dps_channel` call + **one** decode, projecting
the same decoded reply into both. `NoResponseCause` gains a **final, PeerAuth-independent catch-all
`CallFailedWithoutTrustedDpsEnvelope`** (any tonic `Status` that is neither a decoded DPS reply nor
the branch-8 TLS-proven Unauth/PermDenied — `Internal` / `Unavailable` / `Unknown` /
`ResourceExhausted` / `DeadlineExceeded` / plaintext-auth / …) in addition to the genuine-absence
causes (`Timeout`/`Cancelled`/`LocalHandshakeFailure`); all classify `SubmittedUnknown`.

### 4.3 Normative mapping: returned observation → `RawSendReply` → `SendResponse`

Total over **returned observations** (engine joins store `doc_type`):

| # | returned observation | `RawSendReply` | digest | → `SendResponse` |
|---|---|---|---|---|
| 1 | OK, id non-empty | `Accepted{fiscal_id}` | none | `Parsed(Accepted)` |
| 2 | OK, id empty | `OkNoFiscalId{d}` | content | `Parsed(OkButNoFiscalNumber{d})` |
| 3 | named code (-1,-5,-6,-7..-10,-11,-12,-13,-14,-16) | `ServerCode{code,d}` | content | `Parsed(Rejected{verdict,d})` |
| 4 | code -2/-15 | `ServerCode{code,d}` | content | close/Z→`CloseAmbiguous{d}`; else→`Rejected{Close,d}` |
| 5 | code -3 | `ServerCode{-3,d}` | content | `Parsed(SaveError{d})` |
| 6 | unknown **non-zero** i32 | `ServerCode{code,d}` | content | `Parsed(UnknownStatus{code,d})` |
| 7 | **status == 0** | `MissingStatus{d}` | content | `Parsed(MissingStatus{d})` |
| 8 | Unauth/PermDenied, **TlsProven** | `RemoteAuthStatus{g}` | grpc | `RemoteStatus(RemoteAuthStatus(g))` |
| 9 | **any other tonic `Status`** (PeerAuth-independent catch-all: `Internal`, `Unavailable`, `Unknown`, `ResourceExhausted`, `DeadlineExceeded`, plaintext Unauth/PermDenied, post-connect fail, …) | `NoResponse{CallFailedWithoutTrustedDpsEnvelope}` | none | `NoResponse(...)` |
| 10 | genuine absence — no `Status`, no `Response` (timeout future · cancel · local-handshake) | `NoResponse{Timeout\|Cancelled\|LocalHandshakeFailure}` | none | `NoResponse(cause)` |

Row 9 makes the partition **total over every tonic outcome** (branch 8 is the *only* status that
becomes `RemoteStatus`; everything else non-DPS is `NoResponse`). `ServerCode.code` is a
`NonOkStatusCode` (never `0`/`1`). Removed: **`AuthenticatedPeerGarbage`** (tonic collapses a
decode-failure to `Internal` → row 9; needs the future codec). **`CrashedBeforeObservation`** is an
engine boot edge (durable `CALL_STARTED`), **not** a returned observation. Digest: 2–7 content; 8
grpc; 1,9,10 none (type-level).

### 4.4 Placement + wrapper-proof symbol source-gate (B1, B3)

| item | crate::module | vis | who | enforced by |
|---|---|---|---|---|
| `DecodedResponseDigest`/`GrpcStatusDigest` | `prro-domain::delivery` | private field; `from_transport_digest` **pub** | transport decoder | cross-crate privacy (other crates) **+ symbol source-gate** |
| `NonEmptyFiscalNumber`/`NonOkStatusCode` | `prro-domain::delivery` | private field; `from_transport` **pub** | transport decoder | cross-crate privacy **+ symbol source-gate** (provenance, not just form) |
| framing/SHA helpers | `prro::transports::dps` (decoder) | **`fn`-private, not re-exported** | decoder only | symbol source-gate |
| `RawSendReply` + inner + `ParsedReply` mint | `prro::transports::dps` | private inner | decoder | **module privacy** (compile-time) |
| authority ctors: `from_parsed`, `SendResponse::{parsed,no_response,remote_status}` (`SentAccepted::observe` is **not** an engine-mapper ctor — the id arrives pre-validated as `NonEmptyFiscalNumber`) | `prro-domain` | pub | ONE engine mapper file | symbol source-gate |

**Symbol source-gate (wrapper-proof, B3).** A syn-scan (sibling of `with_immediate_no_foreign_io`)
with **per-symbol allowlists**. **Decoder-only** (`transports::dps`): `from_transport_digest`,
`NonEmptyFiscalNumber::from_transport`, `NonOkStatusCode::from_transport`, framing, **and both
`RawSendReply` and `ParsedReply` construction** (`ParsedReply` is an **opaque struct with a private
inner enum**, minted only here — §4.1c). **Engine-mapper-only** (one file, e.g.
`services/write_path/shadow_map.rs`): `SendOutcome::from_parsed`, `SendResponse::{parsed,
no_response, remote_status}`. **`SentAccepted::observe` is NOT a mapper ctor** — the id arrives
pre-validated as `NonEmptyFiscalNumber`, so `observe` stays wherever it is today, never in the
mapper allowlist. The gate **also forbids** re-export (`pub use`), forwarding wrappers, and
function-pointer capture of any gated symbol — so a `mint_for_engine` wrapper in the allowed module
cannot launder the mint. **`prro-testkit` is an explicit test-only allowlist entry.** Precise
guarantee: the engine mapper *constructs* `SendResponse` but can only forward evidence
(digest/id/code) read out of the opaque, transport-minted `RawSendReply`/`ParsedReply` — it never
obtains a constructible digest, id, code, or `ParsedReply` to inject. Nothing rests on external
trybuild proving sibling sealing.

### 4.5 Single doc_type source

`fiscal_documents.doc_type` → `fetch_send_inputs_tx` (fiscal_documents.rs:1909 / stage_send.rs:1248)
→ engine → `from_parsed(reply, store_doc_type)`. `RawSendReply` carries no doc_type.

### 4.6 Total normalized old→target pair graph (B4 — the drift-pin oracle)

Compare **normalized** outcomes, not incomparable types (empty-id has no `RoutingDecision`):

```text
LiveOutcome  = Sent | Guard(EmptyServerFiscalNo) | Routed { retry_class, node_effect }
ShadowNormal = from ClassifiedOutcome → { retry_class, node_effect }  (Accepted→Sent-equiv; NoResponse→Routed)
```

3.2 applies none of these; the drift-pin asserts **equal** on unchanged rows, the **exact pair**
on declared deltas. "Behaviour-neutral" = the shadow does not drive state.

| input (returned observation) | Live (normalized) | Shadow (normalized) | verdict |
|---|---|---|---|
| OK id-non-empty | `Sent` | `Accepted` → Sent-equiv | **equal** |
| OK id-empty | `Guard(EmptyServerFiscalNo)` | `OkButNoFiscalNumber` → ProbeRequired | **pair (delta)** |
| -1 / -5 / -7..-10 / -16 | TerminalReject | TerminalReject | equal |
| -6 | OperatorEscalation | OperatorEscalation | equal |
| -11 | TerminalReject + NodeBlocked | TerminalReject + NodeBlocked | equal |
| -12 | MacRecovery | MacRecovery | equal |
| -13 / -14 | FnConfigError | FnConfigError | equal |
| -2/-15, non-close dt | TerminalReject | Rejected(Close) → TerminalReject | equal |
| -2/-15, close/Z dt | ProbeRequired | CloseAmbiguous → ProbeRequired | equal |
| -3 | TransientRetry | SaveError → TransientRetry | equal |
| **-4** (known) | Indeterminate → TransientRetry | UnknownStatus → TransientRetry | equal |
| **unknown non-zero** (Status::try_from fail) | Decode → ProbeRequired | UnknownStatus → TransientRetry | **pair (delta)** |
| status == 0 | Decode → ProbeRequired | MissingStatus → ProbeRequired | equal |
| TLS RemoteStatus | TransientRetry (compat :314) | ProbeRequired | **pair (delta)** |
| any other tonic `Status` (Internal/Unavailable/Unknown/ResourceExhausted/DeadlineExceeded/plaintext-auth) | Transport → TransientRetry | CallFailedWithoutTrustedDpsEnvelope → TransientRetry | equal |
| genuine absence (timeout/cancel/handshake) | TransientRetry | NoResponse → TransientRetry | equal |

Excluded (not returned decoder observations): defensive `Server{unknown i32}` → `WrapperBug`;
crash/drop (boot edge). Three declared deltas: **empty-id, unknown-non-zero, TLS-RemoteStatus**.

---

## §5 Invariants (must not weaken)

1. **R1 TLS** — branch 8 only under `TlsProven` (grpc.rs:169); plaintext → branch 9 (no digest).
2. **Network outside tx** — `send_chk_observed` in "4a" (stage_send.rs:1562); one call; no
   digest/decode in a tx.
3. **Engine cannot inject a fabricated digest** — §4.4: opaque `RawSendReply` (module privacy) +
   wrapper-proof symbol source-gate over the mint + authority ctors.
4. **Transport has no reservation identity** — no `reservation_id` on any transport type.
5. **No 3.2 behaviour delta / no record** — no routing change, no `ObservedOutcomeV1`/`record`
   (needs D), no `-12` second-wire change; PR4 read-only, cross-checked against §4.6.
6. **Exactly one wire call per stage-4 attempt** — composition-level canary (§6.11).

---

## §6 Teeth (empirical; revert → RED)

1. **Exhaustive mapping** over §4.3; removed cases asserted unreachable/engine-edge.
2. **trybuild: cross-crate literal** — digest / `SendResponse` literal from another crate → won't
   compile.
3. **symbol source-gate: transport-only mint (+ macro/re-export laundering)** —
   `from_transport_digest`/`from_transport`/framing/`RawSendReply`/`ParsedReply` ctor appear ONLY in
   the decoder. The gate is **syntactic** (syn does not expand macros): it **forbids any
   `macro_rules!` / `#[macro_export]` / token-tree that mentions a gated symbol**, plus `pub use` /
   forwarding wrapper / fn-pointer capture, anywhere outside the decoder → RED. Canary: a
   `#[macro_export] macro_rules!` in the decoder that emits the mint, invoked from `services/*`,
   must be caught by the syntactic ban (proven, not assumed).
4. **symbol source-gate: one engine mapper** — `from_parsed` / `SendResponse::{parsed, no_response,
   remote_status}` appear ONLY in the mapper file; `ParsedReply`/digest/`from_transport` construction
   ONLY in the decoder; **`SentAccepted::observe` is NOT in the mapper allowlist**. Direct
   `ParsedReply` / `SendResponse::*` / `Accepted` construction from any other `services/*` → RED.
5. **same-digest + independent golden preimage (B2/B5)** — carrier digest == seam value; PLUS the
   test recomputes `SHA-256(framing(fields))` from raw fields **for all four messages
   (CheckResponse/StatusResponse/RroInfoResponse + GrpcStatus, the latter using the canonical
   numeric code)** and asserts `==`; a constant / re-framed / wrong-version / Debug-coded digest →
   RED. Proves `digest == H(framing(fields))`.
6. **rejected/save/close/missing carry digest (B3)** — each round-trips the seam digest.
7. **Accepted has no digest, honestly built (B1)** — `ParsedReply::Accepted` has no digest field;
   `from_parsed(Accepted,dt)` builds `Accepted`; adding an `Option<digest>` or a fabricated digest
   → won't compile / RED.
8. **empty-id + digest** — OK+empty → `OkButNoFiscalNumber{real digest}`.
9. **no-response-has-no-digest** — branches 1/9/10 have no digest field (type-level).
10. **drift-pin over the §4.6 pair graph (B4)** — normalized `LiveOutcome` vs `ShadowNormal`:
    equal on unchanged, exact pair on the three deltas; a class that neither equals nor matches its
    declared pair → RED.
11. **TLS/plaintext + composition wire-count + shadow-derivation (B4)** — TlsProven→branch 8,
    plaintext/other-status→branch 9, flip guard → RED (#322). **Wire-count runs the full stage-4
    composition against a mock DPS server** and asserts (a) exactly ONE real RPC — a second call
    (e.g. stage_send invoking both old `send_chk` and the new seam) → RED; **and (b) the shadow is
    derived from the SAME unique reply**, proven on a **digest-bearing** reply (D-4-compatible:
    `Accepted` has no digest). The mock returns `-3 ERROR_SAVE` with a **unique `error_message`
    marker**; the test then asserts: the legacy `DpsError::Server` carries that marker; the shadow
    carrier digest equals the **independent** `SHA-256(framing(that reply))`; the shadow
    classification is `SaveError → TransientRetry`; RPC count == 1. Deleting `shadow_map` (no shadow
    produced) → RED — a mere one-RPC count would otherwise stay green.

---

## §7 Containment

- **Foundation-only, additive**; live path drives production unchanged; no routing change / no
  `record` / no retirement / no behaviour delta.
- **No partial cutover** — port swap, authoritative `record` (needs D), `-12` blind-resend kill →
  Bridge + D/E. 3.2 ends at PR5's checkpoint, old path intact.
- **Each sub-PR reverts independently.** PR1 **not** byte-neutral (Debug strings, D-3) — declared +
  pinned.

## D-1…D-4 (adopted)

`DecodedResponseDigest` · dedicated `MissingStatus`→ProbeRequired · delete `RawResponseDigest` in
PR1 (not byte-neutral) · `Accepted` no digest — GO (opaque `RawSendReply`, transport-minted
`NonEmptyFiscalNumber`, total `ParsedReply`).

## B6 note (closed)

`WireDiagnostics.message: BoundedText` truncates at 512B; the live `-12` hint needs the full
message. 3.2 leaves the live path unchanged (full `DpsError::Server{message}` extraction);
`WireDiagnostics` is shadow-only; a typed pre-truncation MAC hint is **deferred to D/E**.
rev-2's "feeds the existing hint" is **retracted**.

## Class-A corrections (cumulative)

`SendResponse` not sealed (public enum) → opaque in PR1 · zero sites = **15 (10 `/src/` + 5
fixtures)** · `try_decode_rro_info_response` = **dto.rs:368** · `fetch_send_inputs_tx` =
**fiscal_documents.rs:1909 / stage_send.rs:1248** · row 10 = completed-call genuine absence
(crash → boot) · digest mint cannot live in prro-domain (purity-gate) · **prro-domain deps =
`{serde, thiserror, uuid}`** · `from_transport_digest` is **`pub`** (source-gate is the fence) ·
**proto is `prro/proto/fiscal_server.proto:36-112`** (not sidecar `check.proto`) — `RroInfoResponse`
has `tins#14`/`lnum#15`/`name_pay#16` · seam is **`async fn send_chk_observed`** · `NonEmptyId` →
transport-minted `NonEmptyFiscalNumber` (provenance) + `NonOkStatusCode` (≠0,1).
