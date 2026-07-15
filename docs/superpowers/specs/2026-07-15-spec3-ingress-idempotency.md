# Spec #3 — Canonical ingress contract + IdempotencyStrategy (POS → gateway)

**Status: 🟡 DRAFT rev 1 (for external audit). 2026-07-15. Grounded on `origin/main` `9ce76c2`.**
Home: **`prro-ingress-contract`** (empty CS-1d skeleton). This spec is **contract/types only** — it
authors the `CanonicalIngressEnvelope` + `IdempotencyStrategy` types and their invariants; it mints
**no migration** in CS-2 (per operator: the `ingress_inbox.idempotency_strategy` column is deferred —
Spec #3 freezes the literals first, then a *separate* additive migration lands the column when a
resolver exists). Scope = **POS → gateway** (distinct from Spec #2/#4 gateway → DPS).

---

## 0 · Thesis
The durable inbox already exists and is battle-tested. The gap is that its **one idempotency
behaviour is implicit** — there is no typed strategy, and there is **no fail-closed guard** for a
source that should carry a stable key but doesn't. Spec #3 (a) names the strategy that is already in
force, (b) adds the missing `NoSafeReplayIdentity` refusal, (c) fixes the M×N→M+N seam by giving every
ingress adapter one `CanonicalIngressEnvelope` to normalise into. **No behaviour change lands in
CS-2** beyond the contract types; the resolver + enforcement are CS-3/CS-6.

## 1 · What EXISTS today — do NOT rebuild (grounded, verify)
- `ingress_inbox` table (`001_baseline.sql:80-100`) with `ux_inbox_fn_idem(fiscal_number, idempotency_key)` (`001:96`) as the dedup key.
- The **`Created / Replay / Conflict` triple** — `InboxInsertOutcome` (`ingress_inbox.rs:87-100`), decided by an **atomic probe-then-insert in one `with_immediate`** (RESERVED-locked).
- **Durable-NEW-before-ACK**: the inbox row commits before any client 2xx (`handler.rs:~642` insert precedes fiscalize); `mark_done_tx` is atomic with the doc `Kvt2→Ack` CAS + seed advance + outbox insert.
- **Payload-only canonical hash** (`dto.rs:475-486`): the conflict hash covers the **economic payload only**, so routing-metadata drift (cashier/department) under the same key resolves to `Replay`, not `Conflict`.
- **Crash reaper** terminalising stale `NEW`/`PROCESSING` (`inbox_reaper.rs`); **RS-2 self-contained recovery columns** (`driver_id` / `business_ts` / `total_sum_kop` / `signed_by_cashier_id`) so a row is re-drivable from persistence alone.
- `request_id` is **server-minted fresh per POST** (`handler.rs:~558`) — it is **not** a replay identity.

## 2 · GREENFIELD (this spec adds)
The `IdempotencyStrategy` type, a per-protocol policy, the `NoSafeReplayIdentity` fail-closed refusal,
and the `CanonicalIngressEnvelope` normalisation boundary.

## 3 · Key types (land in `prro-ingress-contract`, domain-owned, sqlx-free)
```rust
// The single canonical view every ingress adapter (native / maria304 / checkbox / xmlrpc)
// normalises into. Carries the RS-2 self-contained recovery fields so the reaper's
// re-drivable-from-persistence property survives the crate move.
struct CanonicalIngressEnvelope {
    fiscal_number: FiscalNumber,
    protocol: IngressProtocolId,
    operation_type: /* FiscalCommand discriminant */,
    payload: /* canonical economic payload */,
    idempotency_key: Option<String>,     // as supplied by the source (may be absent — see §5)
    correlation_id: Option<String>,
    // RS-2 recovery fields (carry-through, not re-derived):
    driver_id: Option<DriverId>,
    business_ts: String,
    total_sum_kop: Option<i64>,
    signed_by_cashier_id: Option<CashierId>,
}

enum IdempotencyStrategy {
    SourceStableId,        // the source supplies a durable key; dedup by (fiscal_number, key). == TODAY.
    ProtocolStableTuple,   // key derived from a protocol-stable tuple (modelled; UNBACKED today).
    GatewayReservation,    // the gateway mints a namespaced synthetic key for gateway-originated ops.
    NoSafeReplayIdentity,  // no trustworthy key resolvable → REFUSE before minting (§5).
}
```
**Realisations grounded in code:**
- **TODAY == `SourceStableId`** — the client string taken verbatim off the wire (`handler.rs:~625`), deduped by `ux_inbox_fn_idem`. The concrete schema already matches; this is **additive naming, not a rewrite**.
- **`GatewayReservation`** subsumes the two ad-hoc synthetic producers already in the tree — `autoz-{fn}-{shift_hex}` (`auto_z.rs:~62`) and `b10-end-{fn}` (`backlog_drain.rs:~2859`) — under one namespaced constructor.
- **`ProtocolStableTuple`** is modelled but **UNBACKED** (the `protocol`/`operation_type` columns exist but do not participate in uniqueness).

## 4 · Normative invariants
- **I1 (request_id ≠ replay identity).** Only `(fiscal_number, idempotency_key)` is the replay identity; `request_id` (server-minted per POST) is not. State it so the seam-mismatch defences (`handler.rs:~708`) rest on it explicitly.
- **I2 (payload-only conflict).** `Conflict` fires **iff** the economic payload differs under the same key; routing-metadata drift under the same key + goods → `Replay` by design. Re-affirm; do not change.
- **I3 (Replay is success, never re-process).** A `Replay` returns the stored result and MUST NOT re-invoke the engine; a `Conflict` is **never** a silent replay — it records both `request_id`s + both hashes to `audit_log`.
- **I4 (durable-before-ACK).** The `NEW` inbox row commits before any client 2xx; finalize (`mark_done_tx`) is atomic with the fiscal-doc `Kvt2→Ack` CAS + seed advance + outbox insert. A lost POS→gateway ACK never yields a 2nd fiscal doc.
- **I5 (NoSafeReplayIdentity — NEW, the sharpest gap).** If the resolved strategy yields no trustworthy key (empty/absent `idempotency_key` from a source **expected** to supply one), the request is **refused before minting a row** — `audit_log` only, matching the pre-acquire/invalid-ingress refusal class — **never** a weak-keyed `NEW` row. Today there is **no such guard**; a stable-source protocol with an empty key would mint an under-keyed row.
- **I6 (INACTIVE in CS-2).** No `ingress_inbox.idempotency_strategy` column in CS-2 (deferred); the strategy is a resolution decision in code, persisted only when a resolver + a separate additive migration land (CS-3/CS-6).

## 5 · The `NoSafeReplayIdentity` refusal (the fail-closed contract)
Each ingress protocol declares its `IdempotencyStrategy`. A **write** may proceed only under a strategy
that yields a **provable** key:
- `SourceStableId` requires a non-empty source key; an empty/absent key from such a source ⇒ `NoSafeReplayIdentity` ⇒ **refuse pre-mint**.
- `GatewayReservation` requires the gateway to have minted a namespaced key.
- **`NoSafeReplayIdentity` forbids the write** — no `NEW` row, `audit_log` only. `payload_hash` is **not** an identity (two identical economic payloads from independent intents must not collapse). A write-adapter is not admitted without a crash/replay contract-test proving I4 through the new envelope.
Contrast the existing `cashier_id` empty-vs-absent distinction (`dto.rs:488-507`) — the same rigor must apply to the idempotency key, which today it does not.

## 6 · RED-pins
- **RP3-1 (Conflict, not Replay):** same `(fiscal_number, key)` + differing economic payload hash ⇒ `Conflict` (both ids + hashes audited), never `Replay`. Teeth: revert the hash-compare → RED.
- **RP3-2 (NoSafeReplayIdentity — the sharpest):** empty/absent `idempotency_key` from a stable-source protocol ⇒ refused **pre-mint**, `audit_log` only, **ZERO `ingress_inbox` row**. No guard exists today — this is the primary new correctness pin.
- **RP3-3 (GatewayReservation namespacing):** two gateway producers collide **iff** same namespaced key, never across namespaces (`autoz-*` vs `b10-end-*`).
- **RP3-4 (request_id divergence):** a `request_id` that differs from the `idempotency_key` never causes a duplicate fiscalize (I1).
- **RP3-5 (reaper survives the move):** the crash reaper re-drives a `NEW` row from persistence alone after the wire shape becomes `CanonicalIngressEnvelope` (RS-2 fields carried, not re-derived).

## 7 · Open questions for the audit
1. **Strategy resolution point:** is the `IdempotencyStrategy` resolved in the **protocol adapter** (which knows whether the source supplies a stable id) or in `IngressService`? This decides whether `NoSafeReplayIdentity` can be detected **pre-mint** (I5) and whether the deferred column can ever be populated correctly.
2. **`ProtocolStableTuple` scope:** should CS-2 model it at all, or leave it out until a protocol actually needs it (avoid a speculative variant)?
3. **Migration timing:** confirm the deferred `ingress_inbox.idempotency_strategy` column is genuinely CS-3/CS-6, not CS-2 — i.e. the 4-variant taxonomy is final enough to freeze *later* without a 2nd rename.
4. **`NoSafeReplayIdentity` per-protocol default:** which of native/maria304/checkbox/xmlrpc are "stable-source expected"? A protocol with no stable id at all (if any) may legitimately need `GatewayReservation` rather than a refusal.
