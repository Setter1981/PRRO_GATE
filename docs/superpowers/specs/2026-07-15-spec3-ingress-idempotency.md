# Spec #3 — Canonical ingress contract + Idempotency (POS → gateway)

**Status: 🟡 DRAFT rev 4 (post external audit round 3 → NOT-YET, converging). 2026-07-15. Grounded on `origin/main` `9ce76c2`** (fiscal-core re-verified on `specs-3-5-ingress-fleet @ c6a2d2e`; `9ce76c2..` diff = only Spec #3).
Rev 4 closes round-3's three blockers: (1) **`session_scope` is removed from the external identity**
(it broke cross-shift external replay) — the identity is a **sum-type**, session scope lives **only**
inside the internal-producer operation id; profile/policy/version are **provenance fences, never key
components**; (2) the sealed store splits into **two** methods (`insert_new` vs the special
B10-END `insert_processing_with_prepared_doc`), and internal `request_id` is **derived from the
resolved identity** (kills the FN-only cross-session collision); (3) the profile-switch guard covers
**any unresolved intent** (not just an open shift) and the capability matrix is **total** (default-deny
+ exhaustive over the closed `FiscalCommandKind`). Contract/types only; **no migration in CS-2**.

---

## 0 · Thesis
Replay identity is a **typed sum-type**: an **external** POS identity that is **session-independent**
(so a retry across a shift boundary still dedupes) and an **internal** producer identity that is
**session-scoped**. Profile/policy/version are a **provenance fence**, never a way to mint a new key.
"No safe identity" is an explicit fail-closed refusal.

## 1 · What EXISTS today — corrected grounding (do NOT rebuild)
- `ingress_inbox` (`001_baseline.sql:80-100`); `Created/Replay/Conflict` (`InboxInsertOutcome`, `ingress_inbox.rs:87-100`, insert `:187-226`); handler fiscalizes only after `Created` (`handler.rs:642-651`).
- **Uniqueness is ONLY `(fiscal_number, idempotency_key)`** (`ux_inbox_fn_idem`, `001:96`); `protocol`/`operation_type` exist (`001:83-84`) but in **no** unique index (`ingress_inbox.rs:142-143`).
- **Durable-before-ACK for SIGNABLE writes only** (`handler.rs:620-647`); read-only X-report exempt (`:571-572`, no inbox row).
- **Finalize NOT atomic with seed advance**: seed advances at SEND (online `Sending→Sent`; offline `Signed→OfflineLocalAck`, `stage_offline_ack.rs:456/:495`); `finalize` = `Kvt2→Ack` + `mark_done_tx` + outbox (`stage_finalize.rs:253/:307/:316/:286-296`).
- **Reaper is TERMINALISE-ONLY** (`inbox_reaper.rs:9-22`), never re-fiscalizes.
- **Payload hash is CONFLICT-only** (`dto.rs:475-482`); **Conflict audit is best-effort** (`handler.rs:867-909`).
- **`idempotency_key` is a mandatory non-Option `String`** (`dto.rs:50-60`): an **absent** key ⇒ JSON deserialization failure ⇒ `400 MALFORMED_JSON` **before** the handler (`server.rs:164-181`); an **empty** key deserializes fine and today **can reach the inbox** (no guard). Making `AbsentKey → 422 NO_SAFE_REPLAY_IDENTITY` requires an explicit **DTO / evidence-boundary change** (Option-typed key), called out as such.
- **Only `webcheck` is a live ingress source** (`server.rs:97-98`); **maria304: driver implemented, prro wiring GATED OFF** (→ `404 UNKNOWN_SOURCE`, re-enable after the `Option<fiscal_id>` mirror, RS-3); `checkbox`/`xmlrpc` = dead Python, **PLANNED (CS-6)**.
- **Three internal producers** (never external POS traffic):
  - `auto_z` → `NewInboxEntry` NEW, key `autoz-{fn}-{shift_hex}` (already shift-scoped), tagged `Protocol::Rest` (`auto_z.rs:63/:158-176`);
  - **B10 BEGIN** → `NewInboxEntry` NEW, key `b10-begin-{fn}` (function `inline.rs:1622`, row/insert `:1672/:1687`); `request_id` = `begin_request_id(fn)` **FN-only** (`inline.rs:1202-1207`);
  - **B10 END** → **direct `INSERT OR IGNORE`** creating **PROCESSING atomically with a PREPARED doc** (deliberately bypassing the acquire that is forbidden under `GoingOnline`, `backlog_drain.rs:2719-2779`), key `b10-end-{fn}`; `request_id` = `session_end_request_id(fn)` **FN-only** (`:2691-2698`). The `:2854` throwaway is a stage_sign scaffold, not a producer.
  **FN-only `request_id` is unsafe across sessions:** on a 2nd offline session the inbox insert IGNOREs on the stale key and the fresh doc then collides on `UNIQUE(request_id)`.

## 2 · GREENFIELD
The sum-type identity + provenance fence (§3), the two-method sealed store + derived internal
request_id (§4), the fail-closed refusal (§5), and the total capability matrix (§6).

## 3 · Key types (`prro-ingress-contract`, domain-owned, sqlx-free)
```rust
enum IdempotencyPolicy {
    SourceStableId,                              // external source supplies a durable request id
    GatewayDeterministic { producer: InternalProducerId },
    // ClientHeldReservation { .. }  — DORMANT (two-phase POS token).
    // ProtocolStableTuple { contract_id, version } — DORMANT.
}
enum InternalProducerId { AutoZ, B10Begin, B10End }

// The identity — a SUM-TYPE. External is SESSION-INDEPENDENT; session scope is ONLY inside Internal.
enum ReplayIdentity {
    External { source_protocol: IngressProtocolId, source_installation_id: InstallationId,
               source_request_id: String, operation_kind: FiscalCommandKind },
    Internal { producer: InternalProducerId, internal_operation_id: InternalOperationId,  // embeds ShiftId/OfflineSessionId
               operation_kind: FiscalCommandKind },
}
// SEALED constructor: only the resolver / store builds it. fiscal_number is the local partition key.
struct ResolvedReplayIdentity { fiscal_number: FiscalNumber, identity: ReplayIdentity }

// FENCE — stored atomically alongside the row, NEVER part of the effective key.
struct IdentityProvenance { ingress_profile_id: IngressProfileId, policy_id: PolicyId, policy_version: u16 }

enum ReplayIdentityResolution { Resolved(ResolvedReplayIdentity, IdentityProvenance), NoSafeReplayIdentity { reason: NoSafeReason } }
enum NoSafeReason { EmptyKey, AbsentKey, ContentDerivedOnly, NoReservationToken, ProfileSwitchWithUnresolvedIntent, UnlistedOriginOperation }
```
**Effective key** = the `ReplayIdentity` variant fields + `fiscal_number` (partition):
- **External** = `(fiscal_number, source_protocol, source_installation_id, source_request_id, operation_kind)` — **no session** ⇒ a retry of the same `source_request_id` **after a shift boundary still dedupes** (fixes the round-3 counterexample).
- **Internal** = `(fiscal_number, producer, internal_operation_id, operation_kind)` — `internal_operation_id` embeds the `ShiftId`/`OfflineSessionId` ⇒ each session is distinct.
**Provenance fence:** `ingress_profile_id`/`policy_id`/`policy_version` are stored atomically as `IdentityProvenance`; a **version or profile mismatch on the same identity ⇒ lookup / block / reconciliation, NEVER an automatic `Created`** (RP3-4). This is the durable `IdentityProvenance` the round-2/3 audit asked for.

**Index decision (§9-answer):** **drop/replace** the narrow `ux_inbox_fn_idem` with a `UNIQUE` over the
**normalized non-null identity columns** (not an opaque TEXT canonical-encoding, which needs a forever
format + escaping + a legacy decoder). Deferred migration (CS-3/CS-6); normative now.

## 4 · Sealed store boundary — TWO methods + derived request_id
Direct `INSERT … ingress_inbox` is **statically forbidden outside the store module** (a compile/grep
pin). Two sealed methods (the semantics differ — round-3):
- **`insert_new_tx(&mut WriteTxConn, ResolvedReplayIdentity, IdentityProvenance, …)`** → a `NEW` row (the normal `Created|Replay` contract). Used by **external writes, `auto_z`, and B10 BEGIN**.
- **`insert_processing_with_prepared_doc_tx(&mut WriteTxConn, ResolvedReplayIdentity, PreparedDoc, …)`** → a `PROCESSING` row **atomically with the PREPARED doc** — the B10-END path only (`backlog_drain.rs:2719-2779`), which deliberately bypasses the `GoingOnline`-forbidden acquire.
**Internal `request_id` is DERIVED from the resolved identity** (or minted only after the identity
lookup and then reused) — **not** an FN-only `format!()` — so `b10-begin`/`b10-end`/`auto_z` become
per-session unique and never re-collide on `UNIQUE(request_id)`. Resolution stays two-stage: adapter →
typed `IngressIdentityEvidence`; a domain resolver in `IngressService` **before** the insert picks the
policy and returns `Resolved(…)` or an explicit `NoSafeReplayIdentity{reason}` (it **rejects**, it does
not guess a missing policy). The `IngressProfileId` binding is checked **before** a new identity is
formed (§6).

## 5 · The fail-closed `NoSafeReplayIdentity` contract
Refused **before minting any inbox/fiscal row**: a domain-level stable outcome
**`NO_SAFE_REPLAY_IDENTITY`** (the **HTTP `422`** is only the transport binding; non-HTTP ingress
carries the same domain code); the server mints a correlation id and writes a **ROWLESS `audit_log`**
entry keyed by it (`audit_log` has no FK); **audit insert fails ⇒ fail-closed `5xx`** (stricter than
today's best-effort Conflict audit — the asymmetry is justified: the inbox row is its own evidence, a
rowless refusal has none). `payload_hash` is never an identity. **`ClientHeldReservation`** (two-phase
POS token: single-use, durable-before-response, bound to FN/profile/operation, mandatory on retry) is
a **separate dormant** policy, not mixed with the deterministic gateway identity.

## 6 · Total capability matrix (NORMATIVE) — exhaustive, default-deny
Decided over **`(origin, operation_kind)`** where `origin ∈ {WebCheck, InternalProducer::{AutoZ,
B10Begin, B10End}, maria304, checkbox, xmlrpc}` and `operation_kind` ranges over the **closed
`FiscalCommandKind`**. Each cell is exactly one of `ReadOnlyNoInbox | Accept(policy) |
Reject(NoSafeReason)`; **any unlisted `(origin, operation_kind)` ⇒ `Reject(UnlistedOriginOperation)`
(default-deny)**.
| origin | operation | decision |
|---|---|---|
| WebCheck (LIVE) | X-report (read-only) | `ReadOnlyNoInbox` |
| WebCheck (LIVE) | signable write | `Accept(SourceStableId)` iff key non-empty; **empty ⇒ `Reject(EmptyKey)`**. (A non-empty key does not by itself prove uniqueness — the tuple + the no-collision pins do.) |
| InternalProducer::AutoZ | Z-report | `Accept(GatewayDeterministic{AutoZ})` (identity `Internal`, `internal_operation_id` = ShiftId) |
| InternalProducer::B10Begin | offline-session-begin | `Accept(GatewayDeterministic{B10Begin})` (`internal_operation_id` = OfflineSessionId) |
| InternalProducer::B10End | offline-session-end | `Accept(GatewayDeterministic{B10End})` **via `insert_processing_with_prepared_doc`** (`internal_operation_id` = OfflineSessionId) |
| maria304 (gated off) | fiscal write | **`Reject(ContentDerivedOnly)`** — retire the content-hash key (`dispatcher.rs:741-745`; CS-3 keystone) until a durable intent-id/reservation |
| checkbox / xmlrpc (PLANNED) | write | `Reject(NoReservationToken)` unless a proven `external_request_id`/`doc_id` (`SourceStableId`); **kill** the `sha256(payload)` fallback + the `'no-ref'` constant |
| any | unlisted | **`Reject(UnlistedOriginOperation)`** |
**Exhaustiveness pin:** a compile/test pin proves every `(origin, FiscalCommandKind)` maps to exactly one decision (closed match, no `_ =>` silent Accept).

## 7 · Normative invariants (corrected)
- **I1** request_id ≠ replay identity. **I2** Conflict iff payload differs under the same **variant** key. **I3** Replay resolves persisted truth (`Completed|InProgress|Failed`), never re-processes; Conflict audit best-effort. **I4** durable-before-ACK for signable writes (X-report exempt); seed advanced earlier at SEND; **a lost external ACK dedupes across a shift boundary** because the external identity is session-independent. **I5** NoSafeReplayIdentity fail-closed (§5). **I6** INACTIVE in CS-2 (index + sealed-store refactor are CS-3/CS-6).

## 8 · RED-pins
- **RP3-1** same variant key + differing payload ⇒ `Conflict`.
- **RP3-2** empty key ⇒ `Reject(EmptyKey)` ⇒ 422, rowless audit, zero inbox row; audit-fail ⇒ 5xx.
- **RP3-3 (maria304 keystone)** no intent-id ⇒ **both** identical fiscal writes `Reject(ContentDerivedOnly)` (not `assert_ne!`). With an intent-id: same intent on a new session ⇒ same identity; different intent, same payload ⇒ distinct.
- **RP3-4 (upgrade/profile fence)** a `policy_version` or `ingress_profile_id` mismatch on the same identity ⇒ lookup/block/reconcile, never `Created`.
- **RP3-5 (sealed store)** a static pin proves no `INSERT … ingress_inbox` outside the store; the three producers mint via the two sealed methods only.
- **RP3-6 (session vs cross-shift)** (a) an **internal** producer retry in the **same** session ⇒ same identity+request_id, the **next** session ⇒ distinct; (b) an **external** retry of the same `source_request_id` **across a shift boundary** ⇒ **same** identity (dedupes — the round-3 counterexample).
- **RP3-7 (no profile-switch re-issue on unresolved intent)** switching `IngressProfileId` while **any** non-terminal inbox row / reservation / `SubmittedUnknown` / `PendingApply` exists ⇒ `Reject(ProfileSwitchWithUnresolvedIntent)` — including after the shift closed but the intent is unresolved.
- **RP3-8 (namespace separation, known-red until the migration)** same raw string, same FN, different `(origin, operation)` ⇒ no cross-collision.
- **RP3-9** reaper terminalises, never re-fiscalizes.
- **RP3-10 (B10-END mode)** B10-END uses `insert_processing_with_prepared_doc` (PROCESSING+PREPARED atomic); the `insert_new` path is not admissible for it, and vice-versa.
- **RP3-11 (matrix totality)** every `(origin, FiscalCommandKind)` has exactly one decision; unlisted ⇒ `Reject(UnlistedOriginOperation)`.

## 9 · Decisions + open items
- **Index:** drop/replace with normalized non-null identity columns + a new `UNIQUE` (not opaque TEXT-encoding).
- **auto-Z:** `InternalProducerId::AutoZ` + `ShiftId` (in `internal_operation_id`); the request_id follows from that identity.
- **maria304 intent-id:** the legacy 304X-2 wire has no general client intent-id (PREP = department; COMP.p7 = a cashless payment-transaction id; CONF = a receipt counter for reconciliation) — none is intent identity. So a general fiscal write is **refusal** or a separate `ClientHeldReservation` extension.
- **ClientHeldReservation:** dormant until a POS holds a token before mint and presents it on every retry.
- **Activation gate (before CS-3 wires anything):** RP3-4, RP3-5, the strengthened RP3-6, RP3-7 (with unresolved-uncertainty), and RP3-8 are all mandatory.
- **Open for re-audit:** confirm the External/Internal split + `IdentityProvenance` fence fully close the cross-shift/upgrade double-issue; confirm the two-method store + derived request_id are the right shape; confirm the matrix totality pin (closed `FiscalCommandKind`, default-deny) is sufficient.
