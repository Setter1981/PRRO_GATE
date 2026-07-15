# Spec #3 — Canonical ingress contract + Idempotency (POS → gateway)

**Status: 🟡 DRAFT rev 2 (post external audit round 1 → NOT-YET/BLOCKER). 2026-07-15. Grounded on `origin/main` `9ce76c2`** (fiscal-core citations re-verified on branch `specs-3-5-ingress-fleet @ b6a52a1`, same files; the Spec #3 doc itself is new on that branch).
Rev 2 closes the audit BLOCKER — **the rev-1 identity model could collapse two independent identical
sales** (the double-issue on the POS→gateway side) — by (a) splitting *policy* from *resolution
outcome*, (b) **namespacing** the effective key, (c) removing every content/payload/no-ref fallback,
(d) rewriting `NoSafeReplayIdentity` as a fail-closed contract, and (e) fixing the rev-1 false
grounding. Contract/types only — **no migration in CS-2**; the namespaced-key + `idempotency_strategy`
migrations are deferred (CS-3/CS-6). Scope = **POS → gateway** (distinct from Spec #2/#4 gateway → DPS).

---

## 0 · Thesis
The durable inbox is healthy. The gap is that its **replay identity is under-specified**: (1) it is
keyed only on `(fiscal_number, idempotency_key)` — protocol/operation are ignored; (2) one live
adapter (**maria304**) derives the key from **receipt content**, so two genuinely independent
identical sales alias to one key and the second is silently dropped; (3) there is **no fail-closed
refusal** when no trustworthy key exists. Spec #3 makes replay identity a **typed, namespaced,
resolved-before-mint** value, and turns "no safe identity" into an explicit refusal instead of a
weak-keyed row.

## 1 · What EXISTS today — corrected grounding (do NOT rebuild)
- `ingress_inbox` (`001_baseline.sql:80-100`); the `Created/Replay/Conflict` triple `InboxInsertOutcome` (`ingress_inbox.rs:87-100`) via an atomic probe-then-insert in one `with_immediate`.
- **Uniqueness is ONLY `(fiscal_number, idempotency_key)`** — `ux_inbox_fn_idem` (`001:96`); the `protocol` (`001:83`) and `operation_type` (`001:84`) columns **exist but participate in NO unique index** (`ingress_inbox.rs:142-143` probes `WHERE fiscal_number = ? AND idempotency_key = ?` only). Cross-protocol raw-key collision is therefore **structurally possible**, guarded today only by convention (adapters self-prefix: `maria304:`, `autoz-`, `b10-end-`).
- **Durable-before-ACK holds for SIGNABLE writes only** (`handler.rs:620-647`): the `NEW` inbox row commits before the client 2xx. **Read-only X-report is exempt by construction** — `classify_command → ReadOnly → handle_x_report` (`handler.rs:571-572`) answers **without any inbox row** (`:565-568` "never mints a row / lnd / seed / shift transition").
- **Finalize is NOT atomic with seed advance** (rev-1 was wrong): the chain seed advances **at SEND** — online at the `Sending→Sent` CAS + `set_server_fiscal_no` (`stage_send.rs`, `advance-at-SEND`, `stage_finalize.rs:286-296` "finalize advances NOTHING"); offline at `Signed→OfflineLocalAck` (`stage_offline_ack.rs:456/:495`). `finalize` bundles only `Kvt2→Ack` CAS (`:253`) + `mark_done_tx` (`:307`) + outbox (`:316`).
- **The reaper is TERMINALISE-ONLY** (`inbox_reaper.rs:9-22`): it converges a stuck `NEW`/`PROCESSING` row to a terminal inbox status (writes `ingress_inbox.status` + `audit_log` only, **zero** writes to `fiscal_documents`/`node_state`/`shifts`) so a replay gets an honest terminal instead of `202` forever. It **never re-fiscalizes**.
- **Payload hash is CONFLICT-only, never identity** (`dto.rs:475-482`): `Conflict` fires iff the economic payload differs under the same key; it is explicitly not the replay identity.
- **The Conflict audit is best-effort** (`handler.rs:867-909`): the `409` returns even if the audit append errors. (Contrast §5 — the refusal audit is stricter.)
- **`request_id`** is server-minted per POST (`handler.rs:555-558`) — not a replay identity.
- **Only `webcheck` is a live ingress source** (`server.rs:97-99` `matches!(source, "webcheck")`; any other → `404 UNKNOWN_SOURCE`). `native`/`maria304`/`checkbox`/`xmlrpc` are **PLANNED (CS-6)** consumers of the same canonical envelope; the maria304 driver was built against the **dead Python gateway**.
- Gateway-originated producers that mint real inbox rows: `auto_z` (`autoz-{fn}-{shift_hex}`, `auto_z.rs:63/:158/:175`) and the `ensure_end` **b10-end marker** (`b10-end-{fn}`, direct SQL `backlog_drain.rs:2694/:2734`). The `backlog_drain.rs:2854` `throwaway_inbox` is a **stage_sign scaffold, never inserted** — not a producer.

## 2 · GREENFIELD (this spec adds)
Two orthogonal types (§3), a **two-stage resolver** (§4), a **namespaced** effective key (§3/§7), the
**fail-closed `NoSafeReplayIdentity`** contract (§5), and a **per-operation capability matrix** that
removes every content/no-ref fallback (§6).

## 3 · Key types (land in `prro-ingress-contract`, domain-owned, sqlx-free)
Split the rev-1 4-way enum into **policy** (how identity is obtained) vs **resolution outcome**:
```rust
// (a) per-(protocol, operation_type) POLICY — declares where identity comes from.
enum IdempotencyPolicy {
    SourceStableId    { namespace: ReplayNamespace },   // source supplies a durable key
    GatewayOwnedIdentity { scheme: GatewayIdentityScheme }, // gateway mints a namespaced key (autoz-*, b10-end-*)
    // ProtocolStableTuple { contract_id, version }  — DORMANT: admitted ONLY with a proven stable+unique tuple.
}

// (b) per-request RESULT — what the resolver decided.
enum ReplayIdentityResolution {
    Resolved(ResolvedReplayIdentity),
    NoSafeReplayIdentity { reason: NoSafeReason },   // e.g. EmptyKey, AbsentKey, ContentDerivedOnly, NoReservationToken
}

// The NAMESPACED effective key — a proof-carrying newtype, NOT a raw String.
struct ResolvedReplayIdentity {
    fiscal_number: FiscalNumber,
    protocol: IngressProtocolId,
    operation_type: FiscalCommandKind,
    strategy_version: u16,     // so a policy migration (content-hash → reservation) cannot alias old/new keys
    key: String,               // the source/gateway key
}
// effective uniqueness = (fiscal_number, protocol, operation_type, strategy_version, key)

struct CanonicalIngressEnvelope { /* §1 fields */ identity_evidence: IngressIdentityEvidence, /* … */ }
struct IngressIdentityEvidence { client_key: KeyPresence, /* Present(s)|Empty|Absent */
    reservation_token: Option<String>, content_fingerprint: Option<[u8;32]>, /* … */ }
```
**Namespacing is normative now** even though the supporting migration is deferred: today's
`ux_inbox_fn_idem(fiscal_number, idempotency_key)` (`001:96`) must be augmented (CS-3/CS-6, a separate
additive migration) so `protocol`/`operation_type`/`strategy_version` stop being unbacked — otherwise
the same raw string from two adapters cross-collides.

## 4 · Two-stage resolution (the seam that makes `NoSafeReplayIdentity` pre-mint-detectable)
1. **Adapter → typed evidence.** Each ingress adapter extracts an `IngressIdentityEvidence` (client key present/empty/absent, any reservation token, a content fingerprint) and **does NOT decide the key**.
2. **Domain resolver → outcome.** A domain-owned `ReplayIdentityResolver` in `IngressService`, **before** the inbox insert, picks the `IdempotencyPolicy` for `(protocol, operation_type)` and returns `Resolved(ResolvedReplayIdentity)` or `NoSafeReplayIdentity{reason}`. This is what makes the refusal detectable pre-mint (§5).
3. **Repo accepts the RESOLVED type.** `ingress_inbox::insert` / `NewInboxEntry.idempotency_key` must take a `ResolvedReplayIdentity`, **not a raw `String`**. Today the synthetic callers bypass any resolver — `handler.rs:625` passes `cmd.idempotency_key.clone()`, `auto_z.rs:158` and the `ensure_end` bind (`backlog_drain.rs:2734`) pass raw `format!()` strings. Route both gateway producers through `GatewayOwnedIdentity` so they also mint a `ResolvedReplayIdentity` — the repo type is what structurally prevents a raw-String bypass.

## 5 · The fail-closed `NoSafeReplayIdentity` contract
When the resolver yields `NoSafeReplayIdentity`, the write is **refused before minting any row**:
- stable error **`NO_SAFE_REPLAY_IDENTITY`** over **HTTP 422**;
- the server **mints an attempt/correlation id** and writes a **ROWLESS `audit_log`** entry keyed by it (no `ingress_inbox` row, no `fiscal_documents` row);
- **if the audit insert FAILS → fail-closed `5xx`** (never 422-and-forget). This is deliberately **stricter** than today's best-effort Conflict audit (`handler.rs:867-909`, which returns `409` even if the append errors) — the asymmetry is intentional.
- **"pre-mint" = no inbox/fiscal row is minted**, NOT the absence of a correlation id (the correlation id is always server-minted for the rowless audit).
- `payload_hash` is **not** an identity — two identical economic payloads from independent intents must not collapse.

**`GatewayReservation` is a TWO-PHASE contract for an external POS.** A gateway-minted key is safe
**only** if the token is durably issued **before** submit, stored by the POS, and mandatory on every
retry. A gateway key minted at submit-time (with no prior durable token the POS saw) does **not**
dedupe a lost-ACK retry → that case is `NoSafeReplayIdentity`. Therefore **maria304** (no client UUID,
no reservation) **cannot** use `GatewayReservation` to rescue fiscal writes.

## 6 · Per-operation capability matrix (NORMATIVE — every fallback removed)
| ingress | live? | identity source | verdict | safe default |
|---|---|---|---|---|
| **Native/WebCheck** | **LIVE** (only accepted source) | client string verbatim (`handler.rs:625`); **not** content-derived | SAFE against false-collapse (distinct client keys don't alias) **except the empty-key edge** — no guard today (`dto.rs` guards `cashier_id` empty-vs-absent, not the key) | `SourceStableId` iff non-empty; **empty key ⇒ `NoSafeReplayIdentity`** |
| **maria304** | **LIVE Rust driver** (the keystone violation) | **CONTENT-derived**: `maria304:{fn}:{sha256(payload)}` (`dispatcher.rs:741-745/660-663`); no client UUID, no reservation | **UNSAFE** — two independent identical sales → same key → 2nd silently dropped. **PROVEN live**: `bridge_acceptance.rs:335-360` `assert_eq!(key_a1, key_a2, "same receipt content must produce the same key across sessions")`. Zero-body reports use a per-TCP-session key (`dispatcher.rs:773-777`) → lost-ACK retry on a new session is **not** deduped | **`NoSafeReplayIdentity` for fiscal writes** until a durable intent-id/reservation lands. **Retire the content-hash key** — this IS the CS-3 double-issue keystone on the ingress side |
| **checkbox** | PLANNED (dead Python) | `base.py:94-98` ladder `external_request_id → idempotency_hint → sha256(payload)` fallback | fallback is content-derived | `NoSafeReplayIdentity` unless a proven `external_request_id` contract; **kill the sha256(payload) fallback** |
| **xmlrpc** | PLANNED (dead Python) | `webcheck_xmlrpc.py` `external_request_id or doc_id`, terminal fallback = the literal constant **`'no-ref'`** → ALL keyless ops of a method collapse to one key | worse — constant-derived | `NoSafeReplayIdentity` until a durable `doc-id`/reservation contract; **kill the `no-ref` constant** |

The planned **WebCheck shim** (`docs/architecture/2026-05-30-webcheck-shim-ingress-spec.md`, "design spec, no code yet") plans the **same `sha256(payload)` fallback** ("mirror maria304") — it must be pinned to `NoSafeReplayIdentity`-on-absent-uuid, not shipped.

## 7 · Normative invariants (corrected)
- **I1 (request_id ≠ replay identity).** Dedup uses `(fiscal_number, key)`, never `request_id`. (Drop the rev-1 `handler.rs:708` "proof" — that guard is unrelated.)
- **I2 (payload-only Conflict) + namespacing.** `Conflict` fires iff the economic payload differs under the same **namespaced** key; the effective key is `(fiscal_number, protocol, operation_type, strategy_version, key)`, so identical raw strings from different protocols/operations do not alias.
- **I3 (Replay = success; Conflict audit is best-effort).** A `Replay` calls only read-only `resolve_replay` (`handler.rs:854-864`), never the engine; the `Conflict` audit records both ids + hashes **best-effort** (`handler.rs:867-909`) — state it as best-effort, not guaranteed.
- **I4 (durable-before-ACK — SIGNABLE writes only).** The `NEW` inbox row commits before the client 2xx for signable writes (`handler.rs:620-647`); **X-report is exempt** (`:571-572`). `mark_done_tx` is atomic with the `Kvt2→Ack` CAS + outbox; the **seed already advanced earlier at SEND** (online) / offline-ack. A lost POS→gateway ACK never yields a 2nd fiscal doc **only once a stable replay identity exists** (i.e. after §6 closes the maria304/empty-key gaps).
- **I5 (NoSafeReplayIdentity — fail-closed).** Per §5.
- **I6 (INACTIVE in CS-2).** No schema change; the namespaced-key + `idempotency_strategy` migrations are deferred (CS-3/CS-6); the resolver + capability matrix are the contract that lands.

## 8 · RED-pins
- **RP3-1 (Conflict, not Replay):** same namespaced key + differing payload hash ⇒ `Conflict`, never `Replay`.
- **RP3-2 (empty key refused pre-mint):** an **empty** `idempotency_key` from a stable-source protocol ⇒ `NoSafeReplayIdentity` ⇒ `422 NO_SAFE_REPLAY_IDENTITY`, rowless audit, **ZERO `ingress_inbox` row**; audit-insert failure ⇒ `5xx`.
- **RP3-3 (maria304 keystone — two identical sales don't collapse):** two independent identical maria304 fiscal receipts must NOT alias — under the content-hash key they DO (proven by `bridge_acceptance.rs:335-360`); rev-2 requires either a durable intent-id or `NoSafeReplayIdentity`. Teeth: the existing `assert_eq!(key_a1,key_a2,...)` must **invert** (distinct identities) or the write must refuse.
- **RP3-4 (namespaced key):** the same raw string for the same FN under two different `(protocol, operation_type)` does NOT cross-collide (needs the deferred index; pin as known-red until the migration).
- **RP3-5 (GatewayReservation two-phase):** a gateway key minted at submit-time with no prior durable POS-held token does NOT dedupe a lost-ACK retry ⇒ `NoSafeReplayIdentity`.
- **RP3-6 (repo rejects raw String):** `ingress_inbox::insert` accepts only a `ResolvedReplayIdentity`; a synthetic caller passing a raw `format!()` string does not compile / is rejected.
- **RP3-7 (reaper terminalises, never re-fiscalizes):** a stuck `NEW` row is converged to a terminal inbox status; zero writes to `fiscal_documents`/`node_state` (replaces the false rev-1 RP3-5).

## 9 · Open questions for re-audit
1. **Grading SHA:** confirm the auditor grades against `9ce76c2` (fiscal-core files identical on the draft branch `b6a52a1`; the Spec #3 doc is new there).
2. **Cross-protocol collision proof:** mandate a contract/fuzzer test forcing a native client to replay a `maria304:`-prefixed key (proving the latent `ux_inbox_fn_idem` collision) before the namespaced-key migration, or is the structural argument enough?
3. **maria304 live-wire state:** confirm whether the maria304→prro wire is actually connected in-tree (vs still the dead Python gateway) so §6 states live-vs-planned accurately — the content-key hazard exists regardless of backend.
4. **Conflict-audit symmetry:** should the existing best-effort Conflict audit (`handler.rs:890-909`) ALSO become fail-closed for consistency, or is best-effort acceptable there (the `409` carries forensic value)?
5. **b10-end double-form:** confirm the `ensure_end` marker (`:2694/:2734`) is the intended durable `GatewayOwnedIdentity` producer and the `:2854` throwaway is purely a signing scaffold.
6. **ProtocolStableTuple dormancy:** confirm no planned protocol (checkbox `external_request_id`? xmlrpc `doc_id`?) is intended to land as `ProtocolStableTuple` rather than `SourceStableId` — if one is, keep the variant modelled.
