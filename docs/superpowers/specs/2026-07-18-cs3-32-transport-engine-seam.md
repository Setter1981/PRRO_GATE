# CS-3 Bridge-0.1 (3.2) — transport/engine seam + honest decoded-content digest

**Status:** DRAFT for adversarial gate (composition-first, per operator 2026-07-18).
**Base:** `origin/main 2dbae3c` (3.1 + 3.1b merged). **Predecessors:** #4B rev-6, #4A A4-6,
[[project_digest_decoded_content_decision]], `project_cs3_bridge0_foundation_repair`.
**Scope discipline:** additive until a full seam exists; no partial production cutover
(§7). The `DpsChannel → DpsSubmissionPort` port cutover and `kill blind-resend` (D/E)
remain **out of 3.2** (Bridge).

All file:line anchors are on `2dbae3c` and were verified by the author (not agent-relayed).

---

## §1 Problem

The digest cannot be locked in isolation from **who mints it**, **which reply branches
must carry it**, and **where fabrication is forbidden**. Today the DPS reply is split
across two incompatible hierarchies and the digest is (a) partly absent, (b) partly
mislabeled "raw/lossless", and (c) publicly constructible by any crate. 3.2 unifies the
reply into one **total, transport-minted** evidence type feeding the (already sealed but
unconsumed) domain algebra, and turns the digest into an honest **decoded-content**
fingerprint whose provenance guarantee is "the Bridge cannot fabricate it — it carries the
transport-minted value", not byte-for-byte wire fidelity.

**Non-goal (containment, §7):** byte-exact wire proof for DPS disputes is a separate
future forensic slice with a custom tonic codec. Not CS-3.

---

## §2 Current architecture (grounded)

**Two parallel hierarchies today:**

- **Transport → prod path (the live one).** `grpc.rs` client methods
  (`send_chk` grpc.rs:202, `last_chk` :213, `status_rro` :235, `info_rro` :246) run the
  wire call, then `map_tonic_status(status, peer_auth)` (grpc.rs:166) + `try_decode_*_response`
  (dto.rs:198/250/422) collapse the reply to **`Result<CheckAck, DpsError>`**.
  `CheckAck` (dto.rs:66) = `{id, id_sign, data_sign}` — **no digest**; empty id is rejected
  by a stage_send guard (stage_send.rs:1583) *after* the wire. `DpsError` (error.rs:14) =
  **10 variants** `{Transport, RemoteStatus, Indeterminate, Authorization{DocumentReject|
  FiscalNumberNotRegistered}, Decode, Server{code}, NotFound, ServerFiscalIdMismatch,
  QueryNotSupported, Internal}`, **not `#[non_exhaustive]`**. This is routed by
  `route_send_result` (error_routing.rs:263) → `WireDecision{Sent|Routed}`; `route_dps_error`
  (error_routing.rs:289, **exhaustive, no `_`**) → `RoutingDecision{target_state, retry_class,
  audit, node_mode_flip, probe_hint, mac_recovery_hint}` (error_routing.rs:58);
  `route_server_code` (error_routing.rs:426, 12 codes + fail-closed `WrapperBug`). The worker
  surface is `StageSendOutcome` (stage_send.rs:573) `{Sent, Routed(RoutingDecision),
  StateConflict, DocumentMissing, SignerRefused}`, with `extract_wire_forensics`
  (stage_send.rs:866) projecting `RemoteStatus`/`Indeterminate` → `"Transport"`
  (compatibility) and `wire_decision_to_outcome_kind` (stage_send.rs:839) writing
  `transport_trace.outcome_kind`.

- **Domain algebra → sealed but with ZERO production consumers.** `SendResponse`
  (mod.rs:321) `{NoResponse, RemoteStatus, Parsed(SendOutcome)}`; `SendOutcome` (mod.rs:346,
  sealed) minted only by `from_dps_status(RawDpsStatus, DocType, RawResponseDigest)`
  (mod.rs:395); `classify` (mod.rs:702, **1-arg**, 3.1b); `ClassifiedOutcome`;
  `ObservedOutcomeV1` (mod.rs:919) via `record` (mod.rs:949); `ActiveRetryClass` (mod.rs:172,
  7 live), `NodeEffect` (mod.rs:248, 7). None of these are wired into the live path.

**Digest today (the dishonesty):**

- `RawResponseDigest(pub [u8; 32])` — **field is PUBLIC** (mod.rs:109). Any crate,
  including a future Bridge, can write `RawResponseDigest([0u8; 32])`. This is the
  fabrication hole.
- Mint 1: `response_digest` (dto.rs:178) = `SHA-256(prost.encode_to_vec())` of the **decoded**
  envelope. Its own doc (dto.rs:170-177) admits "a re-encode (not the wire bytes)" **and**
  calls it "lossless raw-reply evidence" — self-contradictory. No domain separation.
- Mint 2: `status_digest` (grpc.rs:188) = `SHA-256(code ‖ message ‖ details)` of the gRPC
  status — a **different kind** of digest, currently squeezed into the same
  `RawResponseDigest` type. Doc (grpc.rs:183-187) also says "lossless raw-reply".
- Carriers: `SendIndeterminate::UnknownStatus{digest}` (mod.rs:520), `OkButNoFiscalNumber
  {digest}` (mod.rs:527), `RemoteStatusEvidence::{AuthenticatedPeerGarbage, RemoteAuthStatus}`
  (mod.rs:310/313); `DpsError::{RemoteStatus, Indeterminate}` (error.rs:34/67).
- **Zero-digest sites** = `RawResponseDigest([0u8; 32])`: all **11 are test code** (grpc.rs:408,
  stage_send.rs:2596/2602, error_routing.rs:1031, last_chk_probe.rs:313/335, kvt2_confirm.rs:
  2360/2365, return_online_probe.rs:564/572) plus test fixtures (cs3_c_db `ev_digest` :242,
  rp4b_2 `digest`, rp4b_31/rp4b_r2 `dg`). No prod runtime path fabricates a zero digest today,
  but the **public field lets one appear** and the fixtures institutionalize the antipattern.

**doc_type is already store-owned (single source).** `fiscal_documents.doc_type` →
`fetch_send_inputs_tx` (stage_send.rs:1918) → `SendInputs.doc_type` (fiscal_documents.rs:1939)
→ `route_send_result` (stage_send.rs:1576). `transport_trace` (transport_trace.rs:232) does
**not** carry `doc_type`; W9 reconciliation must JOIN it from `fiscal_documents`.
`is_live_send=false` is RESERVED for W9 (error_routing.rs:26). Doc_type is **never** read
from the wire.

**Invariants already in force (3.2 must not weaken — see §5).** R1 TLS provenance: private
`PeerAuth{TlsProven, Unproven}` (grpc.rs:39), scheme derived from parsed URI
(`scheme_str()=="https"`, grpc.rs:102), `map_tonic_status` emits `RemoteStatus` **only** when
`TlsProven` (grpc.rs:169). Network-outside-tx: wire call at stage_send.rs:1562 sits in the
"4a" segment **outside** both `with_immediate` blocks (Pattern B), guarded at runtime by
`assert_not_in_with_immediate` (tx.rs:65, called grpc.rs:203/…/261, channel.rs:80/109) and
statically by the `with_immediate_no_foreign_io` syn-scan. Transport identity-blind:
`CheckEnvelope` (dto.rs:32) carries **no** `reservation_id`; reservation identity never
reaches `prro/src/transports/dps/*`.

---

## §3 Proposed sequence (one spec → adversarial gate → 5 sub-PRs)

One short grounded spec (this doc) → **audit of the whole composition** → then:

1. **contract/digest types** — introduce sealed `ResponseContentDigest` + `GrpcStatusDigest`
   (rename/replace `RawResponseDigest`); `RawSendReply` total type; strip all
   `raw`/`lossless` claims; add domain separation. Additive: no consumer rewired yet.
2. **total typed transport evidence** — `grpc.rs`/`dto.rs` produce a **total** `RawSendReply`
   for every wire outcome; digests minted at the single seam, sealed. Old
   `CheckAck`/`DpsError` retained as a compatibility projection.
3. **all-consumers propagation** — every `DpsError`/`RawSendReply` consumer audited (§4
   table); explicit arms for new evidence; declare each deliberate behaviour change; harden
   catch-alls that would silently swallow a new branch.
4. **engine-owned mapping / doc-type split** — engine joins `RawSendReply` with store-owned
   `doc_type` → `SendResponse` (via `from_dps_status` for the `Parsed` branch) → `classify`
   → `ObservedOutcomeV1`. `doc_type` stays store-owned; transport never sees it.
5. **integration teeth + final checkpoint** — end-to-end teeth (§6) + a general re-checkpoint
   gate before Bridge.

Each sub-PR is independently revertible (§7). Narrow audit after each; general re-checkpoint
after PR5.

---

## §4 Required contracts & tables (the load-bearing part)

### 4.1 New sealed digest types (kill the public field)

```text
ResponseContentDigest(private [u8;32])   // decoded DPS envelope content
GrpcStatusDigest(private [u8;32])         // gRPC transport-status content
```

- **Private field. No public constructor.** Sole mint points: `dto.rs::response_digest` →
  `ResponseContentDigest`; `grpc.rs::status_digest` → `GrpcStatusDigest`. Both live in
  `prro/src/transports/dps/*` (the transport seam). A `#[cfg(test)]`-only content minter
  (`ResponseContentDigest::for_test_content(&[u8])` etc.) hashes **real** bytes so tests
  cannot inject a zero/default and cannot reach the raw array. No `Default`, no `[0u8;32]`.
- **Definition (honest):** `SHA-256( domain_sep )` where
  `domain_sep = message_type_tag ‖ schema_version ‖ prost.encode_to_vec(known_decoded_fields)`
  for content, and `message_type_tag ‖ schema_version ‖ code ‖ message ‖ details` for gRPC
  status. Deterministic; distinct **decoded content** → distinct digest. **We do NOT claim**
  raw/lossless/"any distinct wire replies differ".
- Carriers migrate: `SendIndeterminate::{UnknownStatus, OkButNoFiscalNumber}` carry
  `ResponseContentDigest`; `RemoteStatusEvidence::{AuthenticatedPeerGarbage, RemoteAuthStatus}`
  carry `GrpcStatusDigest`. `RawResponseDigest` is removed (or reduced to a private alias
  during the additive window, then deleted in PR5).

### 4.2 Total transport evidence: `RawSendReply`

Transport-minted, total over every wire outcome, **doc_type-free** (doc_type is store-owned,
joined by the engine). Sealed: constructor visible only inside `prro/src/transports/dps/*`.

```text
enum RawSendReply {
  Accepted { fiscal_id: NonEmptyId },                 // OK + non-empty id; NO digest
  OkNoFiscalId { digest: ResponseContentDigest },     // OK + empty id
  ServerCode  { code: i32, digest: ResponseContentDigest },  // any non-OK envelope code
  SemanticDecodeFailure { digest: ResponseContentDigest },   // proto decoded, content invalid
  RemoteAuthStatus       { grpc: GrpcStatusDigest },  // TLS-proven Unauth/PermDenied
  AuthenticatedPeerGarbage { grpc: GrpcStatusDigest },// TLS-proven non-DPS body (WAF)
  NoResponse { cause: NoResponseCause },              // NO digest (Transport/timeout/plaintext)
}
```

### 4.3 Normative mapping table: tonic/prost result → `RawSendReply` → `SendResponse`

Engine joins the **store-owned** `doc_type` only in the last column (via `from_dps_status`).

| # | tonic/prost result (source) | `RawSendReply` | digest source | → `SendResponse` (engine + doc_type) |
|---|---|---|---|---|
| 1 | `Ok`, status OK, id non-empty | `Accepted{fiscal_id}` | none | `Parsed(from_dps_status(Ok{id}, dt))` → `Accepted` |
| 2 | `Ok`, status OK, id empty | `OkNoFiscalId{d}` | `ResponseContentDigest` (dto.rs:178) | `Parsed(from_dps_status(Ok{""}, dt, d))` → `OkButNoFiscalNumber` |
| 3 | `Ok`, named code (-1,-5,-6,-7..-10,-11,-12,-13,-14,-16) | `ServerCode{code,d}` | `ResponseContentDigest` | `Parsed(from_dps_status(Error(code), dt, d))` → `Rejected(verdict)` |
| 4 | `Ok`, code -2/-15 | `ServerCode{code,d}` | `ResponseContentDigest` | `from_dps_status` splits by `dt`: close/Z → `CloseAmbiguous`; else → `Rejected(Close)` |
| 5 | `Ok`, code -3 | `ServerCode{-3,d}` | `ResponseContentDigest` | `Parsed(...)` → `SaveError` |
| 6 | `Ok`, code -4 / unknown int | `ServerCode{code,d}` | `ResponseContentDigest` | `Parsed(...)` → `UnknownStatus{code,d}` |
| 7 | `Ok`, proto decoded but semantically invalid | `SemanticDecodeFailure{d}` | `ResponseContentDigest` | `Parsed(...)` → `UnknownStatus` (or a decode-indeterminate) |
| 8 | gRPC `Unauthenticated`/`PermissionDenied`, **TlsProven** | `RemoteAuthStatus{g}` | `GrpcStatusDigest` (grpc.rs:188) | `RemoteStatus(RemoteAuthStatus(g))` |
| 9 | gRPC non-DPS garbage over **TlsProven** | `AuthenticatedPeerGarbage{g}` | `GrpcStatusDigest` | `RemoteStatus(AuthenticatedPeerGarbage(g))` |
| 10 | transport error / timeout / cancel / crash / **plaintext** Unauth / no proven peer | `NoResponse{cause}` | **none** | `NoResponse(cause)` |

**Digest-per-branch rule (§4 requirement):** exactly branches 2–7 carry `ResponseContentDigest`;
8–9 carry `GrpcStatusDigest`; **1 and 10 carry NO digest** (Accepted needs none; a genuine
absence has nothing to fingerprint). There is no branch that both lacks a real reply and
carries a digest.

### 4.4 Visibility / constructor ownership (§4 requirement)

| type | field vis | sole ctor | who may construct |
|---|---|---|---|
| `ResponseContentDigest` | private | `response_digest` (+ `#[cfg(test)] for_test_content`) | transport seam only |
| `GrpcStatusDigest` | private | `status_digest` (+ test-content) | transport seam only |
| `RawSendReply` | — (sealed variants) | transport decode fns | `prro/src/transports/dps/*` only |
| `SendResponse` / `SendOutcome` | already sealed | engine via `from_dps_status` | engine (Bridge) only |

**No default/zero/synthetic digest (§4 requirement):** no `Default`, no `[0u8;32]`, no public
tuple field on any digest type; the 11 test zero-sites + 4 fixtures are rewired to
`for_test_content(real_bytes)`.

### 4.5 Single doc_type source (§4 requirement)

`doc_type` flows `fiscal_documents.doc_type → fetch_send_inputs_tx → SendInputs → engine`.
The engine calls `from_dps_status(raw_status, store_doc_type, digest)`. `RawSendReply` never
carries doc_type; transport never reads it. W9 reconciliation JOINs `doc_type` from
`fiscal_documents` (transport_trace has none).

### 4.6 Deliberate consumer behaviour changes (§4 requirement — full list)

Current `DpsError` consumers (audited): `route_dps_error` (error_routing.rs:289, exhaustive,
breaks build on new variant), `route_server_code` (:426), `last_chk_probe::probe`
(last_chk_probe.rs:89 — explicit RA arms :113 + **catch-all** :123), `kvt2_confirm::
classify_check_result` (kvt2_confirm.rs:301 — explicit RA :402 + **catch-all** :408),
`offline_code_replenish::replenish` (:235, Server explicit + **catch-all** :239),
`return_online_probe::dps_error_class` (:80, exhaustive, no `_`), `doctor/live::run_live`
(live.rs:204, NotFound + **catch-all** :240), `extract_wire_forensics` (stage_send.rs:866,
projects RemoteStatus/Indeterminate→"Transport").

- **Byte-neutral (must stay):** every consumer's *current* output for existing inputs is
  unchanged in PR1–PR2 (RawSendReply is additive; the old path still drives behaviour).
- **Deliberate changes land only in PR3–PR4 and are enumerated here when made:** e.g.
  `extract_wire_forensics` RemoteStatus/Indeterminate may stop projecting to "Transport"
  (error_routing.rs seam at :310/:329 was reserved for exactly this); `route_dps_error` is
  *replaced* by read-only derivation from `ClassifiedOutcome`. Each such change ships with a
  named pin and a one-line "behaviour delta" note. **The 4 catch-all consumers are hardened**
  (explicit arms) so a new evidence branch cannot be silently swallowed.

---

## §5 Invariants (must not weaken)

1. **R1 TLS provenance** — `RemoteAuthStatus`/`AuthenticatedPeerGarbage` (and thus branches
   8–9) are reachable **only** when `PeerAuth::TlsProven` (grpc.rs:169). Plaintext
   Unauth/PermDenied → branch 10 (`NoResponse`/Transport), never a `GrpcStatusDigest`-bearing
   branch. The `scheme_str()=="https"` derivation (grpc.rs:102) is unchanged.
2. **Network outside SQLite write tx** — `RawSendReply` is minted in the "4a" segment
   (stage_send.rs:1562), outside both `with_immediate` blocks; the `assert_not_in_with_immediate`
   + syn-scan guards remain. No digest/decode work moves inside a tx.
3. **Bridge/engine cannot fabricate evidence** — sealed digest types + sealed `RawSendReply`
   + sealed `SendResponse`: the engine can only *carry* transport-minted values. Enforced by
   trybuild (§6).
4. **Transport has no reservation identity** — `RawSendReply` and `CheckEnvelope` carry no
   `reservation_id`; the reservation stays engine-side. No new field crosses the seam.
5. **Incumbent consumers unchanged by accident** — exhaustive matches stay exhaustive;
   behaviour deltas are only the enumerated §4.6 ones, each pinned. `ActiveRetryClass`(7)/
   `NodeEffect`(7) wire strings stay byte-identical to `error_routing.rs` (already pinned).

---

## §6 Teeth (empirical; each revert → RED)

1. **Exhaustive mapping** — a table-driven test asserts every row of §4.3 (all 10 branches ×
   relevant doc_types) maps wire→`RawSendReply`→`SendResponse` exactly; a missing/renamed
   branch fails. Revert a branch → RED.
2. **trybuild: fabrication forbidden** — fixtures that must NOT compile: `ResponseContentDigest([0u8;32])`,
   `GrpcStatusDigest(x)`, `RawSendReply::ServerCode{..}` **outside** the transport crate/module,
   and any engine-side construction of `SendResponse`/`SendOutcome`. TEETH: make a field/ctor
   `pub` → fixture compiles → RED. (Mirrors the 3.1/3.1b canaries.)
3. **trybuild: re-binding forbidden** — carried forward from 3.1b: `classify` stays 1-arg;
   `from_dps_status` doc_type is store-supplied only.
4. **Same-digest propagation** — a digest minted at the transport seam is byte-identical when
   read back off the domain carrier (`UnknownStatus`/`OkButNoFiscalNumber`/`RemoteStatusEvidence`);
   the Bridge adds/derives nothing. Fabricate a different digest downstream → RED.
5. **unknown-code + digest** — code -4 / arbitrary negative int → `ServerCode` → `UnknownStatus`
   preserving BOTH `code` and a real content digest. Drop either → RED.
6. **empty-id + digest** — OK+empty → `OkNoFiscalId{real digest}` → `OkButNoFiscalNumber`;
   `OkButNoFiscalNumber` is unconstructible **without** a transport-minted digest. Zero/absent
   digest → won't compile / RED.
7. **wrong-binding → zero-wire** — no branch yields a digest without a real reply; a genuine
   `NoResponse`/Transport carries **no** digest field (type-level). Attempt to attach a digest
   to branch 1/10 → won't compile.
8. **TLS/plaintext matrix** — over `TlsProven`, Unauth/PermDenied → branch 8 (GrpcStatusDigest);
   over plaintext, same status → branch 10 (NoResponse, no digest). Flip the guard → RED
   (R1 forgery canary, as in #322).
9. **all-consumers pins** — one pin per consumer in §4.6 asserting its output for each evidence
   branch; the 4 hardened catch-alls now break the build on a new branch. Add a branch without
   updating a consumer → RED (compile) for hardened ones, pin-RED for the rest.

---

## §7 Containment

- **Additive until a full seam exists.** PR1–PR2 introduce `RawSendReply` + sealed digests
  **alongside** the live `CheckAck`/`DpsError` path (which keeps driving behaviour). No
  production decision changes until PR3–PR4 wire the engine, and each behaviour delta is
  enumerated (§4.6) + pinned.
- **No partial production cutover.** The `DpsChannel → DpsSubmissionPort` port swap and
  blind-resend kill (D/E) are **not** in 3.2; 3.2 ends at PR5's checkpoint with the domain
  algebra wired as the source and the old path retired only once the total seam is proven.
- **Each sub-PR reverts independently.** PR1 (types) and PR2 (transport evidence) are
  behaviour-neutral and revert cleanly; PR3/PR4 deltas are individually revertible with their
  pins; PR5 is teeth-only.

---

## Open decisions for the gate (front-loaded)

- **D-1 (name):** `ResponseContentDigest` vs `DecodedResponseDigest` — author picks
  `ResponseContentDigest` (emphasizes *content, not wire*); auditor may override.
- **D-2 (SemanticDecodeFailure, branch 7):** map to `UnknownStatus` (reuse) vs a dedicated
  decode-indeterminate carrier — author leans reuse (fewer surfaces); flag if the auditor
  wants a distinct forensic label.
- **D-3 (RawResponseDigest removal timing):** delete in PR1 vs keep as a private alias through
  the additive window and delete in PR5 — author leans PR1 delete (no dead alias), accepting a
  larger PR1 rename touch.
- **D-4 (Accepted digest):** branch 1 carries no digest (author position). Confirm no
  reconciliation path needs a content fingerprint on the accepted envelope.
