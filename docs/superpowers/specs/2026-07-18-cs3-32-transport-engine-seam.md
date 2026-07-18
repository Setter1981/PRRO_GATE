# CS-3 Bridge-0.1 (3.2) — transport/engine seam + honest decoded-content digest

**Status:** DRAFT **rev 2** for adversarial gate. rev 1 → NOT-YET (5 composition class-B,
closed here as point-fix, no redesign). **Base:** `origin/main 2dbae3c`.
**Predecessors:** #4B rev-6, #4A A4-6, [[project_digest_decoded_content_decision]],
`project_cs3_bridge0_foundation_repair`.

**Scope pin (rev 2, blocker 2):** 3.2 is **foundation-only**. It ends at a *pure, read-only*
derivation
> `RawSendReply (+ store doc_type) → SendResponse → ClassifiedOutcome`
built **alongside** the live `CheckAck`/`DpsError`/`route_dps_error` path, which keeps
driving production. 3.2 does **NOT**: mint `ObservedOutcomeV1`, call `record`, change any
production routing, apply any behaviour delta, or retire the old path. The authoritative
durable `record`, the routing cutover, and killing the live `-12 → Resigned → continue`
second wire (blind-resend) all require store-minted `AuthorizedGeneration` and belong to
**Bridge + D/E**. Extending 3.2 into a cutover would force it to grow into D/E (not
recommended).

All file:line anchors are on `2dbae3c`, re-verified by the author (rev-1 Class-A corrected).

---

## §1 Problem

The digest cannot be locked in isolation from **who mints it**, **which returned-reply
branches must carry it**, and **where fabrication is forbidden** — and the sealing
guarantee cannot be stated without a **crate/module placement** that Rust actually enforces.
Today the DPS reply is split across two incompatible hierarchies, the digest is partly
absent / partly mislabeled "raw" / **publicly fabricable**, and the domain surfaces
(`SendResponse`) are **not** sealed. 3.2 unifies the *returned* reply into one total type,
sealed by a placement Rust can enforce, feeding the (already-built, unconsumed) domain
classifier — read-only.

**Non-goal (containment §7):** byte-exact wire proof (custom tonic codec) is a separate
future forensic slice. Not CS-3. Its absence makes one rev-1 branch unreachable (§4.3).

---

## §2 Current architecture (grounded; rev-1 Class-A corrected)

**Crate graph (blocker 1).** `prro-domain` is a **separate crate** (pure; no prost/sqlx).
The digest type + its carriers (`SendIndeterminate`, `RemoteStatusEvidence`, `SendResponse`)
live there. The **mint** (`response_digest`, `status_digest`) and the whole transport
(`prro/src/transports/dps/*`) and engine (`prro/src/services/*`) live in the **`prro`**
crate. Consequence: a domain type minted from `prro` needs a `prro`-reachable constructor,
and **any `prro` module (transport OR engine) can then call it** — cross-crate privacy
stops other crates, not sibling modules. This is the core of blocker 1.

**Two parallel hierarchies today:**

- **Transport → live path.** `grpc.rs` methods (`send_chk` :202, `last_chk` :213,
  `status_rro` :235, `info_rro` :246) → `map_tonic_status(status, peer_auth)` (grpc.rs:166)
  + `try_decode_{check,status,rro_info}_response` (dto.rs:198 / :250 / **:368**) collapse the
  reply to `Result<CheckAck, DpsError>`. `CheckAck` (dto.rs:66) = `{id, id_sign, data_sign}`,
  **no digest**; empty id rejected by a stage_send guard (stage_send.rs:1583) *after* the
  wire. `DpsError` (error.rs:14) = **10 variants**, **not `#[non_exhaustive]`**. Routed by
  `route_send_result` (error_routing.rs:263) → `WireDecision{Sent|Routed}`; `route_dps_error`
  (error_routing.rs:289, **exhaustive, no `_`**) → `RoutingDecision{target_state, retry_class,
  audit_event, audit_severity, node_mode_flip, probe_hint, mac_recovery_hint}`
  (error_routing.rs:58); `route_server_code` (:426). Worker surface `StageSendOutcome`
  (stage_send.rs:573); `extract_wire_forensics` (stage_send.rs:866) projects
  `RemoteStatus`/`Indeterminate` → `"Transport"`.

- **Domain algebra — sealed *outcome*, but UNSEALED *response*, zero prod consumers.**
  `SendOutcome` (mod.rs:346) IS sealed (opaque + `from_dps_status` sole ctor). **But
  `SendResponse` (mod.rs:321) is a PUBLIC enum with PUBLIC variants** `{NoResponse,
  RemoteStatus, Parsed}` — rev-1 wrongly called it "already sealed" (Class-A). `classify`
  (mod.rs:702, 1-arg), `ClassifiedOutcome`, `ObservedOutcomeV1` (mod.rs:919) / `record`
  (mod.rs:949), `ActiveRetryClass` (mod.rs:172, 7), `NodeEffect` (mod.rs:248, 7) — none wired.

**Digest today (the dishonesty):**

- `RawResponseDigest(pub [u8; 32])` — **public field** (mod.rs:109); any crate/module writes
  `RawResponseDigest([0u8;32])`.
- `response_digest` (dto.rs:178) = `SHA-256(prost.encode_to_vec())` of the **decoded** envelope;
  its doc (dto.rs:170-177) says both "a re-encode (not the wire bytes)" **and** "lossless
  raw-reply" — contradictory. `status_digest` (grpc.rs:188) = `SHA-256(code ‖ 0 ‖ message ‖ 0 ‖
  details)` of the gRPC status — a **different kind**, squeezed into the same type. Neither has
  a versioned/length-prefixed framing or domain separation.
- **Zero-digest sites = 15** (rev-1 said "11 + fixtures" — Class-A): 11 in-fn test sites
  (grpc.rs:408; stage_send.rs:2596/2602; error_routing.rs:1031; last_chk_probe.rs:313/335;
  kvt2_confirm.rs:2360/2365; return_online_probe.rs:564/572) + 4 fixtures (cs3_c_db `ev_digest`
  dto:242; rp4b_2 `digest`; rp4b_31 `dg`; rp4b_r2 `dg`). All test scope, but the public field
  is what permits them.

**doc_type already store-owned.** `fiscal_documents.doc_type` → `fetch_send_inputs_tx`
(**fiscal_documents.rs:1909**, callsite **stage_send.rs:1248**) → `SendInputs.doc_type` →
`route_send_result` (stage_send.rs:1576). `transport_trace` (transport_trace.rs:232) has no
`doc_type`; W9 JOINs it. Never read from the wire.

**Invariants in force (§5).** R1 TLS: `PeerAuth{TlsProven,Unproven}` (grpc.rs:39),
`scheme_str()=="https"` (grpc.rs:102), `RemoteStatus` only when `TlsProven` (grpc.rs:169).
Network-outside-tx: wire at stage_send.rs:1562 in "4a" outside `with_immediate`; runtime
`assert_not_in_with_immediate` (tx.rs:65) + `with_immediate_no_foreign_io` syn-scan.
Transport identity-blind: `CheckEnvelope` (dto.rs:32) has no `reservation_id`.

---

## §3 Proposed sequence (one spec → whole-composition gate → 5 sub-PRs)

3.2 is additive and ends at a pure derivation (scope pin). Sub-PRs:

1. **contract/digest types + ownership** — sealed `DecodedResponseDigest` + `GrpcStatusDigest`
   (D-1) in prro-domain (private field, versioned+length-prefixed framing); **opaque**
   `SendResponse` (private inner + `kind()` view); delete `RawResponseDigest` (D-3). **NOT
   byte-neutral** (D-3): derived `Debug` changes strings (e.g. kvt2_confirm `format!("{err:?}")`)
   — declared, pinned.
2. **total typed transport evidence** — `transports::dps` emits a **total** `RawSendObservation
   {evidence: RawSendReply, diagnostics: WireDiagnostics}` for every **returned call
   observation** (§4.2–4.3). `RawSendReply` is an **opaque struct + private inner** in
   `transports::dps` (module privacy seals construction to that module even within `prro`).
   Digests minted at this single seam. Old `CheckAck`/`DpsError` retained (compat).
3. **all-consumers propagation + behaviour-delta table** — audit every consumer (§4.6);
   publish the **full behaviour-delta table now** (applied at Bridge, not 3.2); add pins;
   note (do not yet apply) catch-all hardening.
4. **engine-owned pure mapping** — engine joins `RawSendReply` + store `doc_type` →
   `SendResponse` → `classify` → `ClassifiedOutcome`, **read-only, alongside** the live
   `route_dps_error`. Cross-check the derived `ClassifiedOutcome` against the live
   `RoutingDecision` (drift-pin). **No `ObservedOutcomeV1`, no `record`, no routing change.**
5. **integration teeth + checkpoint** — §6 teeth; general re-checkpoint before Bridge.

Each sub-PR reverts independently. `ObservedOutcomeV1`/`record`/routing-cutover/blind-resend
kill → **Bridge + D/E**.

---

## §4 Contracts & tables (load-bearing)

### 4.1 Sealed digest types + honest enforcement (blocker 1, D-1, D-3, teeth §6)

```text
DecodedResponseDigest(private [u8;32])   // decoded DPS envelope content (D-1)
GrpcStatusDigest(private [u8;32])         // gRPC transport-status content
```

- **Framing (byte-exact, blocker 5):**
  `SHA-256( DOMAIN_TAG_v1 ‖ msg_type_u8 ‖ schema_version_u32_be ‖ Σ len_u32_be(field) ‖ field )`
  where content fields are the **known decoded** prost fields in a fixed order; status fields
  are `code`, `message`, `details`. Fixed versioned prefix + length-prefixed fields (no
  ambiguous concatenation). Claim: **collision-resistant fingerprint of decoded content** —
  NOT "distinct wire replies always differ" (a re-encode drops unknown fields / encoding
  quirks; §1 non-goal).
- **Ownership (honest, blocker 1):** private field ⇒ **cross-crate** Rust privacy stops every
  crate except prro-domain from constructing one. That is **not enough**: the mint must run in
  `prro` (needs prost), and any `prro` module could then call a domain constructor. So:
  - the ONLY mint API is `DecodedResponseDigest::of_decoded(&M)` / `GrpcStatusDigest::of_status(&Status)`
    (prro-domain, `pub`), and
  - a **source/AST allowlist gate** (a syn-scan sibling of `with_immediate_no_foreign_io`)
    forbids calling either mint outside `prro/src/transports/dps/*`. **External trybuild does
    NOT prove sibling-module sealing** (blocker 5) — the source gate is the real enforcement.
- **No default/zero/synthetic:** no `Default`, no public field, no `[0u8;32]`. The 15 zero
  sites are rewired to a real content minter. **Test minter placement (blocker 5):** exposed
  via `prro-testkit` (a dev-dep both prro-domain unit tests and `prro/tests/*` integration
  tests can reach) as `testkit::decoded_digest_of(&[u8])` — **not** `#[cfg(test)]` in
  prro-domain (unreachable from downstream integration tests).

### 4.2 Total transport evidence: `RawSendObservation` (blocker 3, 4)

Transport-minted, **doc_type-free**, total ONLY over **returned call observations** (a call
that came back with a `Response` or a `Status`). Crash/drop is **not** here (§4.3 note).

```text
struct RawSendReply(RawSendReplyInner)          // OPAQUE; private inner enum
                                                //   defined in prro::transports::dps → module-sealed
enum RawSendReplyInner {                        //   (engine, a sibling module, cannot construct it)
  Accepted { fiscal_id: NonEmptyId },                          // OK + non-empty id; NO digest
  OkNoFiscalId { digest: DecodedResponseDigest },              // OK + empty id
  ServerCode  { raw_code: i32, digest: DecodedResponseDigest },// any non-OK, non-zero code
  MissingStatus { digest: DecodedResponseDigest },             // status == 0 (proto default) (D-2)
  RemoteAuthStatus { grpc: GrpcStatusDigest },                 // TLS-proven Unauth/PermDenied
  NoResponse { cause: NoResponseCause },                       // NO digest
}
struct RawSendObservation { evidence: RawSendReply, diagnostics: WireDiagnostics }
struct WireDiagnostics {                        // NON-AUTHORITY forensic sidecar (blocker 4)
  status_code: Option<i32>, grpc_code: Option<String>, message: Option<BoundedText>,
}                                               //   preserves what evidence drops: trace + MAC hint + audit
```

`NoResponseCause` (domain) gains **`CallFailedWithoutTrustedReply`** (blocker 3): a reply/status
arrived but **without trusted provenance** (plaintext Unauth/PermDenied, post-connect failure,
untrusted non-DPS status). It is **not** a genuine local absence — but classifies the same
(`SubmittedUnknown`), so no blind resend. The engine boot-recovery edge keeps minting
`CrashedBeforeObservation` (below).

### 4.3 Normative mapping: returned observation → `RawSendReply` → `SendResponse`

Total over **returned observations**. Engine joins store `doc_type` only in the last column.

| # | returned observation | `RawSendReply` | digest | → `SendResponse` (engine + doc_type) |
|---|---|---|---|---|
| 1 | `Response` OK, id non-empty | `Accepted{fiscal_id}` | none | `Parsed(from_dps_status(Ok{id}, dt))` → `Accepted` |
| 2 | `Response` OK, id empty | `OkNoFiscalId{d}` | content | `Parsed(from_dps_status(Ok{""}, dt, d))` → `OkButNoFiscalNumber` |
| 3 | `Response` named code (-1,-5,-6,-7..-10,-11,-12,-13,-14,-16) | `ServerCode{code,d}` | content | `Parsed(from_dps_status(Error(code), dt, d))` → `Rejected(verdict)` |
| 4 | `Response` code -2/-15 | `ServerCode{code,d}` | content | `from_dps_status` splits by `dt`: close/Z → `CloseAmbiguous`; else → `Rejected(Close)` |
| 5 | `Response` code -3 | `ServerCode{-3,d}` | content | `Parsed(...)` → `SaveError` |
| 6 | `Response` unknown **non-zero** i32 | `ServerCode{code,d}` | content | `Parsed(from_dps_status(Error(code)))` → `UnknownStatus{code,d}` |
| 7 | `Response` **status == 0** (proto default) | `MissingStatus{d}` (D-2) | content | dedicated indeterminate → **ProbeRequired** |
| 8 | gRPC `Unauthenticated`/`PermissionDenied`, **TlsProven** | `RemoteAuthStatus{g}` | grpc | `RemoteStatus(RemoteAuthStatus(g))` |
| 9 | untrusted reply: plaintext Unauth/PermDenied · post-connect failure · non-DPS status over Unproven | `NoResponse{CallFailedWithoutTrustedReply}` | **none** | `NoResponse(CallFailedWithoutTrustedReply)` |
| 10 | genuine absence: timeout · cancel · local-handshake fail (no session / no return) | `NoResponse{Timeout\|Cancelled\|LocalHandshakeFailure}` | **none** | `NoResponse(cause)` |

**Removed vs rev 1 (blocker 3):**
- **`AuthenticatedPeerGarbage` deleted** — unreachable honestly: tonic collapses a prost decode
  failure to `Status::Internal`, indistinguishable from a server-sent `Internal`, raw body
  already lost. Reintroduced only with the future custom codec (§1 non-goal). `RemoteStatusEvidence`
  keeps the variant *future-unpopulated*.
- **`CrashedBeforeObservation` is NOT a transport branch** — a crash/drop returns no
  `RawSendReply`. It is an **engine boot-recovery edge**: boot mints
  `SubmissionEvidence::Started{ NoResponse(CrashedBeforeObservation) }` from the durable
  `CALL_STARTED` marker. Documented here; owned by W9/boot, not the transport table.

**Digest-per-branch rule:** 2–7 carry `DecodedResponseDigest`; 8 carries `GrpcStatusDigest`;
**1, 9, 10 carry NO digest** (type-level — the variant has no digest field). No branch both
lacks a real reply and carries a digest.

### 4.4 Crate/module placement + sealing (blocker 1)

| item | crate::module | field/variant vis | who constructs | enforced by |
|---|---|---|---|---|
| `DecodedResponseDigest` / `GrpcStatusDigest` | `prro-domain::delivery` | private | `of_decoded` / `of_status` | cross-crate privacy (other crates) **+ source-allowlist gate** (transport-only within prro) |
| `RawSendReply` (opaque) + inner | `prro::transports::dps` | private inner | that module only | **module privacy** (sibling engine module cannot construct — compile-time) |
| `RawSendObservation` / `WireDiagnostics` | `prro::transports::dps` | private inner (evidence) | that module only | module privacy |
| `SendResponse` (now opaque) | `prro-domain::delivery` | private inner + `kind()` | `from_dps_status` seam only | privacy + trybuild |
| `SendOutcome` (already opaque) | `prro-domain::delivery` | private inner | `from_dps_status` | privacy + trybuild (3.1) |

**Precise guarantee (not overclaimed).** The engine IS the sanctioned mapper — it legitimately
*constructs* `SendResponse` from a `RawSendReply` (via `from_dps_status` for `Parsed`, and
`SendResponse::{no_response, remote_status}` constructors for the others). What it **cannot** do
is **inject a fresh digest**: a `DecodedResponseDigest`/`GrpcStatusDigest` is only obtainable by
*reading it out of* the opaque, transport-minted `RawSendReply` (a borrowed/consuming view) — the
mint (`of_decoded`/`of_status`) is source-gated to `transports::dps`, so the engine never holds a
constructible digest to inject. So: `RawSendReply` opaqueness is **compile-time** (module
privacy); `SendResponse` opaqueness stops *non-engine* fabrication (compile-time); the
**transport-only digest provenance** is the **source-allowlist gate** (Rust cannot sibling-seal a
cross-crate mint). No claim rests on external trybuild proving sibling sealing.

### 4.5 Single doc_type source

`fiscal_documents.doc_type` → `fetch_send_inputs_tx` (fiscal_documents.rs:1909, callsite
stage_send.rs:1248) → engine → `from_dps_status(raw, store_doc_type, digest)`. `RawSendReply`
never carries doc_type. W9 JOINs `doc_type` (transport_trace has none).

### 4.6 Full behaviour-delta table (blocker 4 — documented NOW, applied at Bridge)

3.2 applies **none** of these (scope pin). They are the deltas the eventual Bridge cutover
will make; the `WireDiagnostics` sidecar preserves the data the live path needs meanwhile.

| input | live path today | Bridge target (via SendResponse/classify) | forensic preserved by |
|---|---|---|---|
| TLS `RemoteStatus` | `route_dps_error` → TransientRetry (compat, error_routing.rs:314) | `RemoteStatus` → **ProbeRequired** | `WireDiagnostics.grpc_code` |
| `MissingStatus` (status 0) | `Decode` → ProbeRequired | dedicated indeterminate → **ProbeRequired** (consistent) | digest + diagnostics |
| OK + empty id | stage_send guard `EmptyServerFiscalNo` after wire (stage_send.rs:1583) | `OkButNoFiscalNumber` (typed, pre-classify) | — |
| `-12 ERROR_BAD_HASH_PREV` | Server → MacRecovery, `mac_recovery_hint` from **message**, **live second wire** (`Resigned → continue`) | `BadHashPrev` → MacRecovery; **second-wire kill is D/E, not 3.2** | `WireDiagnostics.message` feeds the existing hint |
| `-4` / unknown code | Indeterminate/Server → TransientRetry | `UnknownStatus{code,digest}` → TransientRetry | digest + diagnostics |

`route_dps_error`'s `mac_recovery_hint` (regex over the raw message) and the audit
event/severity/probe-reason are **not** authority in `ClassifiedOutcome`; they are read from
`WireDiagnostics` so the live path is unchanged during 3.2.

---

## §5 Invariants (must not weaken)

1. **R1 TLS provenance** — branch 8 (`RemoteAuthStatus`, `GrpcStatusDigest`) reachable ONLY
   when `TlsProven` (grpc.rs:169); plaintext → branch 9 (`CallFailedWithoutTrustedReply`, no
   digest). `scheme_str()=="https"` unchanged.
2. **Network outside SQLite tx** — `RawSendObservation` minted in "4a" (stage_send.rs:1562);
   digest/decode never move inside a tx; `assert_not_in_with_immediate` + syn-scan intact.
3. **Bridge/engine cannot fabricate evidence** — scoped to §4.4 mechanisms: module privacy
   seals `RawSendReply`/`SendResponse`; the source-allowlist gate seals the cross-crate digest
   mint. **No claim rests on external trybuild proving sibling sealing.**
4. **Transport has no reservation identity** — `RawSendReply`/`RawSendObservation`/`CheckEnvelope`
   carry no `reservation_id`.
5. **No 3.2 behaviour delta / no authoritative record** — 3.2 changes no production routing,
   mints no `ObservedOutcomeV1`, calls no `record` (needs store-minted `AuthorizedGeneration`
   from D), and does **not** touch the live `-12` second wire. PR4 derivation is read-only,
   cross-checked against the live `RoutingDecision`. Deltas of §4.6 land at Bridge.

---

## §6 Teeth (empirical; revert → RED)

1. **Exhaustive mapping** — table-driven over §4.3 (every returned-observation branch ×
   relevant doc_types) asserts wire→`RawSendReply`→`SendResponse`; a missing/renamed branch
   fails. Includes the two REMOVED cases as explicit "unreachable/engine-edge" assertions.
2. **trybuild: cross-crate fabrication forbidden** — `DecodedResponseDigest([0u8;32])`,
   `GrpcStatusDigest(_)`, `SendResponse::Parsed(_)` from **another crate** must not compile.
   Make a field/ctor `pub` → RED.
3. **source-allowlist gate: transport-only mint (blocker 1/5)** — a syn-scan test asserts
   `of_decoded`/`of_status`/`RawSendReply` construction appear ONLY under
   `prro/src/transports/dps/*`. Add a call from `services/*` → gate RED. (This — not trybuild —
   is the sibling-module proof.)
4. **module-privacy: sibling engine cannot build `RawSendReply`** — a `prro`-internal
   `compile_fail` (or the source gate) shows a `services` module constructing `RawSendReply`
   fails. Make the inner `pub(crate)` → RED.
5. **same-digest propagation + independent preimage (blocker 5)** — mint at the seam, read off
   the domain carrier: byte-identical. PLUS an **independent golden preimage**: recompute
   `H(framing(fields))` in the test from raw fields and assert `== carrier.digest`; a
   mismatch (or a constant/re-framed digest) → RED. Proves `digest == H(reply fields)`, not
   just "stable".
6. **unknown-code + digest** — non-zero unknown i32 → `ServerCode{raw_code,digest}` →
   `UnknownStatus` preserving BOTH; drop either → RED.
7. **empty-id + digest** — OK+empty → `OkNoFiscalId{real digest}` → `OkButNoFiscalNumber`,
   unconstructible without a transport-minted digest; zero/absent → won't compile.
8. **no-response-has-no-digest (renamed from rev-1 "wrong-binding→zero-wire", blocker 5)** —
   branches 1/9/10 have **no digest field** (type-level); attaching a digest won't compile.
   *This does not count wire calls*; the real wire-call-binding pin (RP4B-5) stays **Bridge**.
9. **TLS/plaintext matrix** — over `TlsProven`, Unauth/PermDenied → branch 8 (grpc digest);
   over plaintext → branch 9 (no digest). Flip the guard → RED (R1 forgery canary, #322).
10. **all-consumers pins + PR4 drift** — one pin per §4.6 consumer for each evidence branch;
    PR4 cross-checks derived `ClassifiedOutcome` vs live `RoutingDecision` (any divergence in
    the read-only path → RED, since 3.2 must be behaviour-neutral).

---

## §7 Containment

- **Foundation-only, additive.** `RawSendReply`/digests/opaque-`SendResponse`/`classify`
  derivation are built **beside** the live `CheckAck`/`DpsError`/`route_dps_error` path, which
  keeps driving production unchanged. **No production routing change, no `ObservedOutcomeV1`,
  no `record`, no old-path retirement, no behaviour delta** in 3.2.
- **No partial production cutover.** The `DpsChannel → DpsSubmissionPort` swap, the
  authoritative `record` (needs D's store-minted `AuthorizedGeneration`), and the `-12`
  blind-resend kill are **Bridge + D/E**. 3.2 ends at PR5's checkpoint with the pure derivation
  proven and cross-checked, old path intact.
- **Each sub-PR reverts independently.** PR1 is **not** byte-neutral (Debug string change,
  D-3) — declared + pinned; PR2/PR4 are behaviour-neutral (additive/read-only); PR3 is
  doc+pins; PR5 is teeth.

---

## D-1…D-4 (auditor rulings, adopted)

- **D-1:** `DecodedResponseDigest` (precise > ambiguous `ResponseContentDigest`).
- **D-2:** dedicated `MissingStatus` (status 0) → ProbeRequired; do **not** reuse `UnknownStatus`.
- **D-3:** delete `RawResponseDigest` in **PR1** (after the ownership fix) — no private alias
  (it would keep mixing two digest classes). PR1 is **not** byte-neutral: derived `Debug`
  changes strings (≥ kvt2_confirm `format!("{err:?}")`) — declared + pinned.
- **D-4:** Accepted carries **no** digest — **GO**, conditioned on `RawSendReply` being truly
  opaque and `NonEmptyId` sealed (reconciliation uses the fiscal id + a later `last_chk`, not a
  send-response fingerprint).

## Class-A corrections (rev 1 → rev 2)

`SendResponse` was **not** sealed (public enum) — now made opaque in PR1 · zero-digest sites =
**15** (not "11 + fixtures") · `try_decode_rro_info_response` = **dto.rs:368** · `fetch_send_inputs_tx`
= **fiscal_documents.rs:1909** / callsite **stage_send.rs:1248**.
