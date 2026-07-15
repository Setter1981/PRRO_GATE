# Spec #5B — Fleet Hold/Release lifecycle (signed / scoped-contiguous-epoch / PULL)

**Status: 🟡 DRAFT rev 3 (post external audit round 2 → NOT-YET, converging). 2026-07-15. Grounded on `origin/main` `c107854` + plan §3.10.**
Rev 3 closes round-2's residual security holes: (1) **atomic intake** (inbox-row + stream-cursor +
trust-generation in ONE tx); (2) a **TrustStream↔RegisterStream fence** (a hidden key-revocation can no
longer let a compromised-key command through); (3) **`AwaitingPredecessors` is a durable FSM state**;
(4) **closed payload types** + **narrowed scope to `Hold`/`Release`** (Policy/Config/Provision/
ProtocolRevision → their own payload-specs); (5) a **non-rollback enrollment/root anchor** outside the
main-DB backup domain; (6) an explicit **signed wrapper + canonical V1**; **checkpoints are FORBIDDEN in
V1** (full replay only). Semantics locked now; code+schema **dormant** (post-CS-5; runtime CS-6);
control-plane server is a **separate deployment**; pilot ships **Unenrolled / advisory-OFF**.

---

## 0 · Thesis
A fleet command is a **signed, scope-contiguous-epoch, PULL** intent whose **intake, epoch-consume, and
trust-generation check are one atomic tx**; a hidden epoch — in the register OR the trust stream —
cannot bypass a HOLD or a key-revocation; the **local coordinator is the only applier**, atomically; a
command sets **policy but never law**; **enrollment + accepted epoch are anti-rollback**, outside the
restorable DB.

## 1 · What EXISTS to reuse (greenfield otherwise)
- Durable-before-ACK — `ingress_inbox` (`001:80-100`); the fleet inbox is a **separate** table.
- The pure-oracle coordinator — Spec #1 — the **only** applier (CS-4).
- **`prro_crypto` is only a RAW primitive** (`crypto/provider.rs:60-74`, caller-supplied digest+pubkey) — the fleet **verifier + trust-store are greenfield** (§4, §8).
- Zero fleet-command code exists. Dormant contract.

## 2 · Scope (narrowed)
Lock **`Hold` / `Release`** (RegisterStream) + **`RotateKey` / `RevokeKey`** (TrustStream) + the
security machinery. **`Policy` / `Config` / `Provision` / `ProtocolRevision` are REJECTED** — each needs
its own payload-spec (closed capability + quiescence + supersession). Pilot: agent OFF, **Unenrolled**,
empty trust-store.

## 3 · Two-layer transport + ATOMIC trusted intake
- **`prro-fleet-agent` (dumb):** PULLs **opaque signed bytes**, hands them to a `FleetCommandIntake` trait object. Per #5A it depends on **no** `prro`/store/`sqlx`/crypto — a transport shim; it **cannot** verify or persist.
- **`FleetCommandIntake` (trusted, node-side) — ONE `BEGIN IMMEDIATE`:** verify(signature, domain, version, suite) → check the **contiguous** register epoch **and** the required **trust generation** → **INSERT the inbox row** and **CAS the stream cursor** `last_consumed_epoch: n-1 → n` **in the same tx**. **An authenticated contiguous command consumes its epoch even if later `Rejected`/`Deferred`; a pre-auth failure consumes NOTHING.** No cursor-advanced-without-row and no row-without-cursor window exists. An **invalid/untrusted** envelope never occupies the authoritative `command_id`: it produces a **separate, bounded security-audit keyed by the raw-envelope hash**, advancing no epoch.

## 4 · Signed wrapper + canonical V1 (defined, not promised)
```rust
struct SignedFleetCommand { envelope: FleetCommandEnvelope, signature: FleetSignature } // signature IS in the type
struct FleetCommandEnvelope {
    domain_tag: [u8; 8],           // literal `b"PRROFLT1"`
    schema_version: u16, canonical_bytes_version: u16,
    authority_id: AuthorityId, fleet_id: FleetId, environment: Environment, // Test|Prod — all signed
    command_id: CommandId,
    epoch_scope: EpochScope,       // RegisterStream(fleet_id, fiscal_number) | TrustStream(fleet_id)
    epoch: u64,                    // CONTIGUOUS within epoch_scope (§5)
    required_trust_epoch: u64,     // the TrustStream epoch this command REQUIRES to be already applied (§5 fence)
    signer_key_id: KeyId,
    issued_at_unix_ms: u64, expires_at_unix_ms: u64,
    body: FleetCommandBody,        // CLOSED (§6) — no generic JSON/map/config bag
}
```
**Canonical V1 (normative, `canonical_bytes_version=1`):** big-endian fixed-width integers;
length-prefixed byte strings; enum discriminants as `u16` tags; strings **UTF-8 NFC** with explicit
byte-length caps; a fixed field order; **no floats, no maps, no locale**. Suite: **DSTU-4145 digest →
`prro_crypto` verify**, `suite_id` pinned; the signature wire encoding is fixed. **Golden vectors** pin
it. An **unknown `schema_version`/`canonical_bytes_version`/`suite_id` ⇒ fail-closed
`Rejected(UnknownVersion)`** — never a best-effort parse.

## 5 · Scoped, contiguous epochs + cross-stream trust fence
- **Two signed scopes:** `RegisterStream(fleet_id, fiscal_number)` (Hold/Release) and `TrustStream(fleet_id)` (RotateKey/RevokeKey). Each has its own **contiguous** cursor.
- **Contiguity:** `epoch == last_consumed+1` admits; `epoch > last_consumed+1` ⇒ **`AwaitingPredecessors`** (a **durable** state, §7) — held, **not** applied, cursor **not** advanced — until the predecessors replay. **Checkpoints are FORBIDDEN in V1** (a `state_digest` cannot prove applied effects); the only way past a gap is full replay. Any checkpoint form is a **separate future spec**.
- **Cross-stream trust fence (closes the hidden-revocation bypass):** a Register command carries a signed `required_trust_epoch`; if the local `TrustStream` cursor is **behind** it ⇒ **`AwaitingTrustPredecessors`** (do not apply — the awaited revocation may invalidate this command's key). The **trust generation is re-checked by CAS in the apply tx** (§8), not by a prior read. A compromised key is stopped because its command can require no more than the last trust epoch it knew, and the pending `RevokeKey` fences it.
- **Check order:** (1) signature/domain/version/suite → else `Rejected(BadSignature)`, **no cursor advance** (high-epoch garbage cannot DoS); (2) **duplicate `command_id`** ⇒ replay the persisted outcome; **same `command_id` + different `envelope_hash`** ⇒ `IdempotencyConflict` (security incident), never replay; (3) `epoch ≤ last_consumed` ⇒ `Rejected(Stale)`; (4) gap ⇒ `AwaitingPredecessors`; (5) trust behind ⇒ `AwaitingTrustPredecessors`; (6) contiguous + trust-current ⇒ consume epoch + admit (atomic §3).

## 6 · Closed command bodies (structural law-over-policy)
```rust
enum FleetCommandBody {                    // CLOSED — no generic bag; caps/constants/toggles are UNREPRESENTABLE
    Hold(HoldBody),                        // RegisterStream
    Release(ReleaseBody),                  // RegisterStream
    RotateKey(RotateKeyBody),              // TrustStream
    RevokeKey(RevokeKeyBody),              // TrustStream
    // Policy / Config / Provision / ProtocolRevision — NOT a variant here; each lands via its own payload-spec.
}
struct HoldBody { hold_scope: HoldScope /* finite */, reason_code: HoldReasonCode /* finite */ }
struct ReleaseBody { hold_id: HoldId }     // addresses ONE specific hold
struct RotateKeyBody { new_key: PubKey, new_keyset_epoch: u64 }
struct RevokeKeyBody { revoked_key_id: KeyId, new_keyset_epoch: u64 }
```
A fleet command **cannot even express** disabling the 168h/36h/50k caps or a blanket hold over a
mandatory path — those fields are **absent from the type**. **Total HOLD operation-matrix:** a HOLD
blocks **only new business**; **mandatory drain / reconciliation / online-return proceed regardless**.

## 7 · Lifecycle FSM (AwaitingPredecessors is durable) + atomic apply
```
(intake) → ReceivedDurable | AwaitingPredecessors | AwaitingTrustPredecessors | SecurityRejected(no epoch)
ReceivedDurable            → Applied | Rejected | Deferred
Awaiting*                  → ReceivedDurable (on predecessor/ trust catch-up) | Rejected(Expired)
Deferred                   → Applied | Rejected(Expired | Superseded | RevokedKey)
Applied | Rejected         → immutable
```
- **Atomic apply (ONE tx):** the **effect** (the HOLD record / key change), `state=Applied`, the **trust-generation CAS**, and the `FleetPolicyGeneration` bump commit together. **`generation` increments ONLY on an `Applied` effect**; `Deferred`/`Rejected`/`Awaiting*` return the current generation unchanged. Crash before `Applied` ⇒ idempotent re-derive by `(scope, epoch)`.
- **Supersession — ONE typed mechanism:** a `ConflictKey` (typed). `Hold` and `Release` **never** supersede each other; a `Release` targets a specific `hold_id`. Superseding an old `Deferred` and applying the new command commit **atomically**.

## 8 · HOLD records · TTL · restore · trust-store (anti-rollback)
- **HOLD = durable record** `{ hold_id, source: Local|Fleet, hold_scope, applied_epoch }`; a `Release` clears one `hold_id`; **`effective_hold` = OR over un-released records**; a fleet release never clears a local hold.
- **Two clocks:** message validity (`expires_at_unix_ms`) ⇒ an expired command is **never applied**; an already-`Applied` HOLD **never auto-releases** on any TTL — held **+ alerting** until a **signed `Release`**. An indeterminate/rolled-back clock ⇒ **fail-closed** (do not apply, do not auto-release).
- **Trust-store (separate, secure, INACTIVE):** a `TrustStream`-versioned keyset (`keyset_epoch`) with a **root/recovery anchor**. Routine `RotateKey` is signed by the current key; **emergency `RevokeKey` requires the root/recovery key** (a separate recovery lane). Apply re-checks the keyset generation by **CAS**; an already-`Applied` command stays valid. **Trust cannot bootstrap from an unknown key** — enrollment establishes the root **locally, out-of-band**.
- **Restore anti-rollback (the round-2 gap):** the node's **enrollment** is an **irreversible `FleetEnrollmentState` + root fingerprint stored OUTSIDE the main DB's backup/rollback domain** (a dedicated anchor file / secure store). `Unenrolled` (pilot) ⇒ not blocked. Once **`Enrolled`**, a restore **cannot** revert to `Unenrolled`, and a restored DB whose `last_accepted_epoch` / keyset is **behind** the anchor ⇒ **fail-safe HOLD** on write-admission until an **authenticated forward-sync**; control-plane unreachable ⇒ **stay held**. A restored empty in-DB trust-store on an `Enrolled` node is thus **not** mistaken for advisory-OFF. This must **not** reuse the baseline's DPS-down-stays-ONLINE path (`backup_restore.rs:809-840`) nor its kill-switch (`:895-929`).

## 9 · RED-pins (semantics testable now; runtime dormant until CS-6)
- **RP5B-1 (canonical/signature):** golden vectors pin canonical V1; a tampered/wrong-domain/wrong-environment/unknown-version/unknown-suite envelope ⇒ fail-closed `Rejected`.
- **RP5B-2 (atomic intake, no epoch loss):** a crash between the inbox INSERT and the cursor CAS is impossible (one tx); an authenticated contiguous command consumes its epoch even if later `Rejected`; a pre-auth failure consumes none.
- **RP5B-3 (contiguity — hidden HOLD):** `E(n+2)` with `E(n+1)` missing ⇒ durable `AwaitingPredecessors`, cursor not advanced; a later `E(n+1)=Hold` still applies.
- **RP5B-4 (cross-stream trust fence — hidden REVOKE):** a Register command whose `required_trust_epoch` exceeds the local TrustStream cursor ⇒ `AwaitingTrustPredecessors`; a held `RevokeKey` that catches up invalidates the pending command (its key is revoked) — the compromised-key bypass fails.
- **RP5B-5 (dumb agent / trusted intake):** the `prro-fleet-agent` crate cannot verify or persist (#5A DAG); untrusted envelopes are audited by raw-hash, never occupying an authoritative `command_id`.
- **RP5B-6 (atomic apply / generation):** effect + `Applied` + trust-CAS + generation are one tx; `generation` bumps only on `Applied`; `ConflictKey` is the sole supersession; `Hold`/`Release` never supersede; crash before `Applied` re-derives.
- **RP5B-7 (law over policy — structural):** the closed `FleetCommandBody` cannot express a cap/constant/toggle; a HOLD blocks only new business and never a mandatory drain/reconciliation/return; the oracle recomputes law each admission.
- **RP5B-8 (independent HOLD + TTL):** a fleet release clears only its `hold_id`; a local hold survives; an applied HOLD never auto-releases; indeterminate clock ⇒ fail-closed.
- **RP5B-9 (enrollment/restore anti-rollback):** an `Enrolled` node cannot revert to `Unenrolled` via restore; a restored behind-epoch/keyset ⇒ fail-safe HOLD until authenticated forward-sync; unreachable control-plane ⇒ stay held.
- **RP5B-10 (trust anti-rollback):** a revoked key ⇒ `Rejected(RevokedKey)`; emergency revocation needs the root key; a since-rotated key does not invalidate an `Applied` command; trust never bootstraps from an unknown key.

## 10 · Open questions for re-audit
1. **Scope confirm:** `Hold`/`Release` + `RotateKey`/`RevokeKey` is the #5B cut; `Policy` (+ Config/Provision/ProtocolRevision) deferred to payload-specs — acceptable?
2. **Enrollment anchor:** the exact "outside the backup domain" mechanism (a dedicated file with an OS-level guard, or a secure element) — spec it now, or name it normatively and leave the medium to CS-6?
3. **`required_trust_epoch` semantics:** must **every** Register command carry it (even a Release), or only key-sensitive ones? (Simplest: always, = the trust epoch the signer last saw.)
4. **`AwaitingPredecessors` retention:** how long does a durable gap-hold live before `Rejected(Expired)` by its own TTL, and does an expired predecessor-wait ever unblock later epochs (it must not silently skip)?
5. **`FleetPolicyGeneration`:** a distinct per-`RegisterStream` monotonic counter (not the Spec #2 delivery generation, `032:17-18`) — confirm.
