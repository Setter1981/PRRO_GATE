# Spec #3 — Canonical ingress contract + Idempotency (POS → gateway)

**Status: 🟡 DRAFT rev 3 (post external audit round 2 → NOT-YET, converging). 2026-07-15. Grounded on `origin/main` `9ce76c2`** (fiscal-core re-verified on `specs-3-5-ingress-fleet @ f41b06e`; diff `9ce76c2..` contains only Spec #3).
Rev 3 closes round-2's three load-bearing gaps: (1) the replay identity now uses the **locked-plan
tuple** and `strategy_version` becomes a **provenance fence, not part of the key** (kills the
upgrade-double-issue); (2) a **sealed store boundary** routes **all three** internal producers
(`auto_z`, `b10-begin`, `b10-end`) + external writes through a resolved, session/shift-scoped identity
(closes the direct-`INSERT` bypass); (3) the identity binds to a durable `IngressProfileId` so a
channel switch cannot re-issue. Contract/types only — **no migration in CS-2** (the identity-key +
`idempotency_strategy` migrations are deferred; the index choice is pinned in §3). Scope = **POS →
gateway**.

---

## 0 · Thesis
Replay identity must be a **typed, sealed, resolved-before-mint, session-scoped** value using the
**authoritative tuple** — never content, never a version-in-the-key, never a raw String an internal
producer can hand-assemble. "No safe identity" is an explicit fail-closed refusal.

## 1 · What EXISTS today — corrected grounding (do NOT rebuild)
- `ingress_inbox` (`001_baseline.sql:80-100`); `Created/Replay/Conflict` (`InboxInsertOutcome`, `ingress_inbox.rs:87-100`) via atomic probe-then-insert.
- **Uniqueness is ONLY `(fiscal_number, idempotency_key)`** — `ux_inbox_fn_idem` (`001:96`); `protocol`/`operation_type` columns exist (`001:83-84`) but are in **no** unique index (`ingress_inbox.rs:142-143`). Cross-protocol raw-key collision is structural, guarded only by convention (`maria304:`/`autoz-`/`b10-*` prefixes).
- **Durable-before-ACK holds for SIGNABLE writes only** (`handler.rs:620-647`); **read-only X-report is exempt** (`handler.rs:571-572`, no inbox row).
- **Finalize is NOT atomic with seed advance**: the chain seed advances **at SEND** (online `Sending→Sent` CAS; offline `Signed→OfflineLocalAck`, `stage_offline_ack.rs:456/:495`); `finalize` = `Kvt2→Ack` CAS + `mark_done_tx` + outbox only (`stage_finalize.rs:253/:307/:316/:286-296`).
- **The reaper is TERMINALISE-ONLY** (`inbox_reaper.rs:9-22`): converges a stuck row to a terminal inbox status; writes `ingress_inbox.status`+`audit_log` only; never re-fiscalizes.
- **Payload hash is CONFLICT-only, never identity** (`dto.rs:475-482`); the **Conflict audit is best-effort** (`handler.rs:867-909`).
- **Only `webcheck` is a live ingress source** (`server.rs:97-98` `matches!(source, "webcheck")`). **maria304: driver implemented but its prro wiring is GATED OFF** (→ `404 UNKNOWN_SOURCE`, `server.rs:89-98`, re-enable after the `Option<fiscal_id>` mirror, RS-3); `checkbox`/`xmlrpc` have no live Rust adapter (dead Python). **PLANNED (CS-6)**.
- **Three internal producers mint real inbox rows** — none is external POS traffic:
  - `auto_z` → `NewInboxEntry`, key `autoz-{fn}-{shift_hex}` (**shift-scoped**), but tagged `Protocol::Rest` (`auto_z.rs:63/:158`) so `(protocol, op)` cannot tell it from an external WebCheck Z;
  - **B10 BEGIN** → `NewInboxEntry`, key `b10-begin-{fn}` (**FN-only**, `inline.rs:~15/1622`);
  - **B10 END** → **direct `INSERT OR IGNORE INTO ingress_inbox`** (bypasses the repo entirely), key `b10-end-{fn}` (**FN-only**, `backlog_drain.rs:2725`).
  The `backlog_drain.rs:2854` `throwaway_inbox` is a stage_sign scaffold, never inserted — not a producer.
  **FN-only keys are unsafe across shifts:** on shift 2, the END-probe by the new `shift_id` returns absent, the inbox insert is IGNORE'd on the stale key, and the fresh fiscal doc then collides on `UNIQUE(request_id)`.

## 2 · GREENFIELD (this spec adds)
The authoritative-tuple identity (§3), the sealed store boundary + two-stage resolver (§4), the total
capability matrix (§6), and the fail-closed `NoSafeReplayIdentity` (§5).

## 3 · Key types (`prro-ingress-contract`, domain-owned, sqlx-free)
```rust
// POLICY — how identity is obtained for (IngressProfile | InternalProducer, operation).
enum IdempotencyPolicy {
    SourceStableId,                        // external source supplies a durable request id
    GatewayDeterministic { producer: InternalProducerId },  // auto_z / b10-begin / b10-end
    // ClientHeldReservation { .. }  — DORMANT: two-phase token (single-use, durable-before-response,
    //                                 bound to FN/profile/operation, mandatory on retry).
    // ProtocolStableTuple { contract_id, version } — DORMANT: only with a proven stable+unique tuple.
}
enum InternalProducerId { AutoZ, B10Begin, B10End }

// RESULT — per-request outcome; the registry returns an EXPLICIT Reject, never guesses a policy.
enum ReplayIdentityResolution { Resolved(ResolvedReplayIdentity), NoSafeReplayIdentity { reason: NoSafeReason } }
enum NoSafeReason { EmptyKey, AbsentKey, ContentDerivedOnly, NoReservationToken, ProfileSwitchWithOpenShift }

// The identity — the LOCKED-PLAN tuple (ARCHITECTURE_CONSOLIDATION_PLAN.md §3.7). SEALED constructor
// (only the resolver / store builds it) so no caller hand-assembles a raw key.
struct ResolvedReplayIdentity {
    fiscal_number: FiscalNumber,
    origin: ReplayOrigin,               // External{ source_protocol, source_installation_id } | Internal{ producer }
    source_request_id: String,          // external stable request id; internal deterministic key
    operation_kind: FiscalCommandKind,
    session_scope: SessionScope,        // shift_id / offline_session generation → internal producers are per-shift unique
    // NOT part of the identity — a fence, resolved separately:
    // policy_version is carried in IdentityProvenance, NOT in the effective key (see below).
}
```
**Authoritative effective key = `(fiscal_number, source_protocol|internal_producer, source_installation_id, source_request_id, operation_kind, session_scope)`** — matching the locked plan's
`schema_version, source_protocol, compatibility_profile_version, source_installation_id,
source_request_id, …` envelope. **`strategy_version`/`policy_version` is a PROVENANCE FENCE, never a
key component:** a version bump must NOT turn an already-accepted `(v0, K)` into a new `Created` after
a lost-ACK retry as `(v1, K)`. A version mismatch on the same identity ⇒ **legacy lookup / block /
reconciliation**, not an automatic new row.

**Index decision (pin):** the existing narrow `ux_inbox_fn_idem(fiscal_number, idempotency_key)`
(`001:96`) **cannot be "augmented" by an additive wider index** — while the narrow unique index
exists it still admits the collision. CS-3/CS-6 must **either canonical-encode the full tuple into the
single `idempotency_key` TEXT** (so the one existing index becomes correct) **or drop-and-replace**
the index. Namespacing is normative now; the migration is deferred.

## 4 · Sealed store boundary + two-stage resolution
1. **Adapter → typed evidence.** Each adapter extracts `IngressIdentityEvidence` (client key Present/Empty/Absent, reservation token, content fingerprint); it does **not** decide the key.
2. **Domain resolver → outcome.** A domain-owned `ReplayIdentityResolver` in `IngressService`, **before** the inbox insert, selects the `IdempotencyPolicy` for `(IngressProfile | InternalProducer, operation)` and returns `Resolved(ResolvedReplayIdentity)` or an explicit `NoSafeReplayIdentity{reason}`.
3. **Sealed store API — the only path to an inbox row.** Add `insert_processing_tx(&mut WriteTxConn, ResolvedReplayIdentity, …)`; **statically forbid any `INSERT … ingress_inbox` outside the store module** (a compile/grep pin) so the current bypasses are structurally impossible: the `ensure_end` direct `INSERT OR IGNORE` (`backlog_drain.rs:2725`), the `auto_z` `NewInboxEntry` (`auto_z.rs:158`), and the `B10 BEGIN` `NewInboxEntry` (`inline.rs:1622`) all route through the sealed API with a `GatewayDeterministic{producer}` identity carrying the `session_scope`. The repo taking a `ResolvedReplayIdentity` (not a raw `String`) is necessary **but not sufficient** — the static no-direct-INSERT pin is what actually closes it.
4. **`IngressProfileId` binding.** The identity binds to a durable `IngressProfileId`; a retry of the same intent via a **different** profile while a shift is open ⇒ `NoSafeReplayIdentity{ProfileSwitchWithOpenShift}` (extends the frozen no-channel-switch rule to replay identity) — else ACK-lost-on-A + retry-via-B mints a second document.

## 5 · The fail-closed `NoSafeReplayIdentity` contract
Refused **before minting any inbox/fiscal row**:
- domain-level stable outcome **`NO_SAFE_REPLAY_IDENTITY`** (the **HTTP `422`** is only the transport binding; non-HTTP ingress carries the same domain code);
- the server **mints a correlation id** and writes a **ROWLESS `audit_log`** entry keyed by it (`audit_log` has no FK, so a rowless entry is structurally valid); **"pre-mint" = no inbox/fiscal row**, not the absence of a correlation id;
- **audit insert fails ⇒ fail-closed `5xx`** (never 422-and-forget) — deliberately **stricter** than today's best-effort Conflict audit (`handler.rs:867-909`), which is acceptable there because the existing durable inbox row is its own evidence and a retry re-Conflicts, whereas a rowless refusal has no other durable trace.
- `payload_hash` is never an identity.
- **`ClientHeldReservation`** (two-phase POS token) is a **separate dormant** `IdempotencyPolicy` — not mixed with the deterministic auto-Z/B10 gateway identity.

## 6 · Total capability matrix (NORMATIVE) — over `(IngressProfile | InternalProducer, operation)`
Each cell is **`ReadOnlyNoInbox | Accept(policy) | Reject(NoSafeReason)`**. Rows (not just 4
per-ingress):
| origin | operation | decision |
|---|---|---|
| WebCheck (LIVE) | X-report | `ReadOnlyNoInbox` |
| WebCheck (LIVE) | signable write | `Accept(SourceStableId)` iff non-empty key; **empty ⇒ `Reject(EmptyKey)`** |
| InternalProducer::AutoZ | Z-report | `Accept(GatewayDeterministic{AutoZ})`, `session_scope=shift` (already shift-keyed; add producer tag so it is not mistaken for external WebCheck Z) |
| InternalProducer::B10Begin | offline-session-begin | `Accept(GatewayDeterministic{B10Begin})`, **`session_scope`=shift/session** (fix FN-only) |
| InternalProducer::B10End | offline-session-end | `Accept(GatewayDeterministic{B10End})`, **`session_scope`=shift/session** (fix FN-only + kill the direct INSERT) |
| **maria304** (driver impl, prro-wiring **gated off**) | fiscal write | **`Reject(ContentDerivedOnly)`** until a durable intent-id/reservation — retire the content-hash key `dispatcher.rs:741-745` (the CS-3 double-issue keystone on the ingress side, proven by `bridge_acceptance.rs:335-360`) |
| checkbox / xmlrpc (PLANNED, dead Python) | write | `Reject(...)` unless a proven `external_request_id`/`doc_id` (`SourceStableId`); **kill** the `sha256(payload)` fallback and the `'no-ref'` constant |
**Every content/no-ref fallback is removed.** The planned WebCheck shim's `sha256(payload)` fallback
(`docs/architecture/2026-05-30-webcheck-shim-ingress-spec.md`) must be pinned to `Reject` on absent uuid.

## 7 · Normative invariants (corrected)
- **I1 (request_id ≠ replay identity).** Dedup uses the tuple key, never the server-minted `request_id`.
- **I2 (payload-only Conflict) over the tuple key.** `Conflict` iff the economic payload differs under the same **tuple** key; identical raw strings from different origins/operations/sessions do not alias.
- **I3 (Replay resolves persisted truth, never re-processes).** A `Replay` calls only read-only `resolve_replay` (`handler.rs:854-864`) and returns the persisted outcome — which may be `Completed`, `InProgress`, or `Failed` (not necessarily "success"); it never re-invokes the engine. The Conflict audit is best-effort.
- **I4 (durable-before-ACK — signable writes only).** The `NEW` row commits before the 2xx for signable writes; X-report exempt. `mark_done_tx` is atomic with `Kvt2→Ack` + outbox; the seed advanced earlier at SEND. A lost ACK yields no 2nd doc **only once a stable identity exists** (§6 closes maria304/empty/FN-only).
- **I5 (NoSafeReplayIdentity — fail-closed).** Per §5.
- **I6 (INACTIVE in CS-2).** No schema change; the tuple-index + `idempotency_strategy` migrations + the sealed-store refactor are CS-3/CS-6; this spec is the contract.

## 8 · RED-pins
- **RP3-1 (Conflict, not Replay):** same tuple key + differing payload ⇒ `Conflict`.
- **RP3-2 (empty key refused):** empty `idempotency_key` from a stable source ⇒ `Reject(EmptyKey)` ⇒ `422`, rowless audit, **zero inbox row**; audit-insert failure ⇒ `5xx`.
- **RP3-3 (maria304 keystone — no intent-id ⇒ REFUSE, not assert_ne):** while no durable intent-id exists, **both** identical maria304 fiscal writes are **refused** (`Reject(ContentDerivedOnly)`) — inverting `assert_eq!(key_a1,key_a2)` to `assert_ne!` would trade under-issue for double-issue-after-reconnect. **Once an intent-id lands:** (a) same durable intent_id on a new TCP session ⇒ **same** identity; (b) different intent_id, same payload ⇒ **distinct** identity.
- **RP3-4 (upgrade-double-issue):** a `policy_version` bump does NOT turn an accepted `(v0,K)` into a new `Created` on retry as `(v1,K)`; version mismatch ⇒ legacy lookup/reconcile, not a new row.
- **RP3-5 (sealed store):** a static pin proves **no** `INSERT … ingress_inbox` exists outside the store module; the three internal producers mint via `insert_processing_tx(ResolvedReplayIdentity)` only.
- **RP3-6 (session-scoped internal identity):** an internal producer retry within the **same** shift/session ⇒ same identity; the **next** shift/session ⇒ distinct identity (fixes `b10-begin`/`b10-end` FN-only cross-shift collision on `UNIQUE(request_id)`).
- **RP3-7 (no profile-switch re-issue):** the same intent retried via a different `IngressProfileId` with an open shift ⇒ `Reject(ProfileSwitchWithOpenShift)`, no 2nd doc.
- **RP3-8 (namespace separation, known-red until the tuple-index migration):** the same raw string for the same FN under two different origins/operations does not cross-collide.
- **RP3-9 (reaper terminalises, never re-fiscalizes).**

## 9 · Open questions for re-audit (round-2 residuals mostly resolved)
1. **Index mechanism:** canonical-encode the full tuple into the single `idempotency_key` TEXT (keep one index) vs drop/replace `ux_inbox_fn_idem` — which does the audit prefer for the deferred migration?
2. **`SessionScope` for `auto_z`:** it is already `shift_hex`-scoped; confirm the internal-producer tag is the only addition needed there (vs re-keying).
3. **maria304 intent-id source:** does the maria304 wire protocol have any field that could carry a durable client intent-id (making `SourceStableId` reachable), or is `ClientHeldReservation`/refusal the only path?
4. **`ClientHeldReservation` dormancy:** keep it dormant (no live two-phase POS today), or is any pilot POS expected to hold a reservation token?
5. **Two mandatory CS-3-activation RED-pins** (per round-2 answer): namespace separation (RP3-8) + refuse write-profile switch with open shift / unresolved uncertainty (RP3-7) — confirm these are the right gate before activation.
