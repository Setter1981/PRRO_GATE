# Spec #5B — Fleet Hold/Release lifecycle (signed / scoped-contiguous-epoch / PULL)

**Status: 🟡 DRAFT rev 4 (post external audit round 3 → NOT-YET, converging). 2026-07-15. Grounded on `origin/main` `c107854` + plan §3.10.**
Rev 4 closes round-3's blockers: (1) **crypto verify is OUTSIDE the write tx** (frozen-invariant #1);
(2) the hidden-revocation is closed by a **root-signed `TrustHeadAttestation`** bound to each Register
(no self-asserted trust epoch) + a **recovery lane**; (3) an **expired `Awaiting*` still CONSUMES its
epoch slot** (no expiry-wedge); (4) an **exhaustive HOLD × operation matrix**; (5) the **anchor carries
the monotonic high-water** + an anchor-first update protocol; (6) a **canonical/suite appendix** +
separated **policy vs trust generations** + the restored **ACK shape**. Scope = `Hold`/`Release`
(RegisterStream) + `RotateKey`/`RevokeKey` (TrustStream) + `EmergencyRevoke` (RecoveryLane);
Policy/Config/Provision/ProtocolRevision → own payload-specs. Semantics locked now; code+schema
**dormant** (post-CS-5; runtime CS-6); pilot ships **Unenrolled / advisory-OFF**.

---

## 0 · Thesis
A fleet command is a **signed, scope-contiguous-epoch, PULL** intent; **crypto is verified before the
tx**, then intake/epoch-consume/trust-CAS commit atomically; a **root-attested trust head** — not the
operational key — gates every Register, so a compromised key cannot outrun a pending revocation; an
**expired wait never lets an epoch be skipped**; **enrollment + monotonic high-water are anti-rollback,
outside the restorable DB**; a command sets **policy but never law**.

## 1 · Reuse (greenfield otherwise)
Durable-before-ACK (`ingress_inbox` `001:80-100`, separate table); the pure-oracle coordinator (Spec #1,
the only applier); **`prro_crypto` is a RAW primitive only** (`crypto/provider.rs:60-74` — DSTU-4145 over
a caller-supplied digest + pubkey, **64-byte `r‖s` little-endian** signature); the fleet **verifier +
trust-store + root anchor are greenfield**. Zero fleet-command code exists.

## 2 · Scope
`Hold`/`Release` (RegisterStream) + `RotateKey`/`RevokeKey` (TrustStream) + `EmergencyRevoke`
(RecoveryLane, root-signed). `Policy`/`Config`/`Provision`/`ProtocolRevision` **REJECTED** (own specs).

## 3 · Transport + intake — crypto OUTSIDE the tx (frozen-invariant #1)
- **`prro-fleet-agent` (dumb):** PULLs **opaque signed bytes** → a `FleetCommandIntake` trait object; per #5A it has no `prro`/store/`sqlx`/crypto — cannot verify or persist.
- **`FleetCommandIntake` (trusted, node-side) — the exact order:**
  1. **outside any tx:** parse → `canonical_bytes` → **crypto-verify** signature + the root-signed `TrustHeadAttestation` (§5) → obtain `verified_keyset_generation` + `attested_trust_head`;
  2. `BEGIN IMMEDIATE`;
  3. **CAS** the durable `trust_generation` mirror **==** `verified_keyset_generation` **and** `local_trust_head ≥ attested_trust_head` (both in the same SQLite tx domain);
  4. INSERT the inbox row **+** CAS the register `stream_cursor` `n-1 → n`;
  5. commit. **No crypto inside the tx** (invariant #1). A CAS conflict ⇒ rollback + re-verify from step 1.
  **An authenticated contiguous command consumes its epoch even if later `Rejected`/`Deferred`; a pre-auth failure consumes nothing.** An invalid/untrusted envelope never occupies the authoritative `command_id` — a **separate bounded security-audit keyed by the raw-envelope hash**, no epoch advance.

## 4 · Signed wrapper + Canonical V1 (Appendix A is normative)
```rust
struct SignedFleetCommand { envelope: FleetCommandEnvelope, signature: FleetSignature /* 64-byte r‖s LE */ }
struct FleetCommandEnvelope {
    domain_tag: [u8; 8] /* b"PRROFLT1" */, schema_version: u16, canonical_bytes_version: u16, suite_id: u16, // suite IN the signed bytes
    authority_id: AuthorityId, fleet_id: FleetId, environment: Environment,
    command_id: CommandId, epoch_scope: EpochScope, epoch: u64, required_trust_epoch: u64,
    signer_key_id: KeyId, issued_at_unix_ms: u64, expires_at_unix_ms: u64, body: FleetCommandBody,
    trust_head_attestation: TrustHeadAttestation,   // root-signed, §5
}
```
**Appendix A — Canonical V1 (`canonical_bytes_version=1`, `suite_id=1`):** `domain_tag=b"PRROFLT1"`;
big-endian fixed-width ints; length-prefixed (`u32` BE) byte strings; enum discriminants `u16`;
strings UTF-8 **NFC**, ≤256 bytes each; a **fixed field order** (as declared); no float/map/locale.
`suite_id=1` = **DSTU-4145** digest (profile pinned) → `prro_crypto` verify; `FleetSignature` = **64-byte
`r‖s` little-endian** (matching `crypto/provider.rs:60-74`). **Golden vectors** pin the bytes. Unknown
`schema_version`/`canonical_bytes_version`/`suite_id` ⇒ fail-closed `Rejected(UnknownVersion)`.

## 5 · Epochs, streams, and the root-attested trust head (the hidden-revocation fix)
- **Three signed scopes:** `RegisterStream(fleet_id, fiscal_number)` (Hold/Release); `TrustStream(fleet_id)` (RotateKey/RevokeKey); `RecoveryLane(fleet_id)` (root/recovery-signed `EmergencyRevoke` — **NOT** blocked by a missed/captured normal TrustStream epoch). Each has its own contiguous cursor.
- **`TrustHeadAttestation` (root-signed, not the operational key):** `{ fleet_id, attested_trust_head_epoch, keyset_digest, attested_at_unix_ms }`, signed by the **root/recovery anchor key**. Every Register (incl. Release) carries the latest one. The edge **verifies it against the local root anchor** (§8) and applies the Register **only after `local_trust_head ≥ attested_trust_head_epoch`** — so a **compromised operational key cannot self-assert a stale trust head**; a pending `RevokeKey`/`EmergencyRevoke` that raises the attested head **fences** the compromised command (`AwaitingTrustPredecessors`).
- **Structural guards:** a `Trust*` body is admissible **only** in `TrustStream`; `EmergencyRevoke` only in `RecoveryLane`; `new_keyset_epoch == envelope.epoch`; `epoch_scope.fleet_id == envelope.fleet_id`; `required_trust_epoch` is **never** self-asserted authority — the root attestation is.
- **Contiguity + check order:** (1) signature/domain/version/suite/attestation → else `Rejected(BadSignature)`, no cursor advance; (2) dup `command_id` ⇒ replay; same id + different `envelope_hash` ⇒ `IdempotencyConflict`; (3) `epoch ≤ last_consumed` ⇒ `Rejected(Stale)`; (4) register gap ⇒ `AwaitingPredecessors`; (5) `local_trust_head < attested` ⇒ `AwaitingTrustPredecessors`; (6) contiguous + trust-current ⇒ consume + admit (atomic §3). **Checkpoints are FORBIDDEN in V1** (full replay only).

## 6 · Closed bodies + exhaustive HOLD × operation matrix (structural law)
```rust
enum FleetCommandBody { Hold(HoldBody), Release(ReleaseBody), RotateKey(RotateKeyBody), RevokeKey(RevokeKeyBody), EmergencyRevoke(EmergencyRevokeBody) }
struct HoldBody { hold_scope: HoldScope, reason_code: HoldReasonCode }
enum HoldScope { NewBusinessAll, NewSalesOnly, NewShiftOpen }         // FINITE — no free-form
enum HoldReasonCode { OperatorRequested, ComplianceReview, Provisioning, Suspected } // FINITE
struct ReleaseBody { hold_id: HoldId }
```
Caps/constants/enforcement toggles are **unrepresentable** (absent from every body). **HOLD × `OperationKind`
(exhaustive, compile-checked — no `_ =>`):**
| OperationKind | under an effective HOLD |
|---|---|
| Sale / cash Return / ServiceIn / ServiceOut / OpenShift | **BLOCKED** (new business) |
| Z-report / X-report | **BLOCKED** as *new operator action* (but a **mandatory auto-Z at a legal cap is NOT** — it is law-driven, proceeds) |
| offline-drain / reconciliation | **PROCEEDS** (mandatory, law/recovery-driven) |
| mandatory online-return (legally required) | **PROCEEDS** |
A HOLD blocks only discretionary new business; every **law-mandated** path proceeds. The oracle
recomputes law each admission (Spec #1), so a stored HOLD never overrides a legal obligation.

## 7 · Lifecycle FSM — expired waits CONSUME their slot (no wedge)
```
(intake) → ReceivedDurable | AwaitingPredecessors | AwaitingTrustPredecessors | SecurityRejected(no epoch, no slot)
Awaiting*          → ReceivedDurable (predecessors + trust caught up) ;  OR expired: see below
ReceivedDurable    → Applied | Rejected | Deferred
Deferred           → Applied | Rejected(Expired | Superseded | RevokedKey)
Applied | Rejected → immutable
```
- **Expiry never skips an epoch (the wedge fix):** an expired `Awaiting*` forbids the **effect** but **keeps its sequence slot**. When its predecessors (+ trust) catch up, **one tx** advances the cursor **through** it: `Awaiting(expired) + cursor=n-1 → Rejected(Expired) + cursor=n`. The immutable tombstone is retained until consumed (or de-enrollment). A TTL **never** authorizes a silent skip.
- **Atomic apply (ONE tx):** the **effect** + `state=Applied` + the **trust-generation CAS** + the correct **generation bump** commit together. **`Hold`/`Release` bump the per-`RegisterStream` `FleetPolicyGeneration`; `RotateKey`/`RevokeKey`/`EmergencyRevoke` bump the `TrustKeysetGeneration`** — never each other's. `generation` moves **only** on an `Applied` effect; `Deferred`/`Rejected`/`Awaiting*` leave it unchanged. Supersession is one typed `ConflictKey`; `Hold`/`Release` never supersede each other; a `Release` targets a specific `hold_id`.
- **ACK (restored, plan §3.10):** `{ epoch_scope, epoch, outcome_state, effective_generation, reason }` — transport-authenticated; one `Register` ⇒ one ACK ⇒ one generation.

## 8 · Anchor + trust-store + restore (anti-rollback, content + protocol)
- **The external anchor CARRIES the monotonic high-water** (so behind-restore is provable): `FleetAnchor { enrollment_state: Unenrolled|Enrolled|EnrolledUnknown, root_fingerprint, trust_keyset_epoch_floor, register_high_water OR sealed_snapshot_generation }` — stored **outside the main DB's backup/rollback domain**.
- **Restore rule:** `Unenrolled` (pilot) ⇒ not blocked. `Enrolled` + a restored DB whose `last_accepted_epoch`/`keyset` is **behind** the anchor's floor/high-water ⇒ **fail-safe HOLD** on write-admission until an **authenticated forward-sync**; control-plane unreachable ⇒ **stay held**. A **missing/corrupt anchor ⇒ `EnrolledUnknown` + fail-safe HOLD**, **never** `Unenrolled` (a restored empty in-DB trust-store cannot be mistaken for advisory-OFF).
- **Update protocol (crash-safe, anchor-first):** without cross-store atomicity, update the **external anchor BEFORE** the main DB — a crash then yields a **false HOLD** (safe), never a rollback bypass.
- **Trust-store:** `keyset_epoch`, root/recovery anchor; routine `RotateKey` signed by the current key, **`EmergencyRevoke` requires the root/recovery key** (RecoveryLane); apply re-checks keyset generation by CAS; an `Applied` command stays valid; **trust never bootstraps from an unknown key** — enrollment sets the root **locally, out-of-band**. This must **not** reuse the baseline DPS-down-stays-ONLINE path (`backup_restore.rs:809-840`) nor its kill-switch (`:895-929`).

## 9 · RED-pins (semantics testable now; runtime dormant until CS-6)
- **RP5B-1 (canonical/suite/no-crypto-in-tx):** golden vectors pin Appendix A; crypto-verify happens **outside** the tx; a tampered/unknown-version/unknown-suite/bad-attestation envelope ⇒ fail-closed.
- **RP5B-2 (atomic intake):** inbox INSERT + cursor CAS + trust-gen CAS are one tx; auth-contiguous consumes its epoch; pre-auth consumes none; a CAS conflict rolls back + re-verifies.
- **RP5B-3 (hidden HOLD):** `E(n+2)` with `E(n+1)` missing ⇒ durable `AwaitingPredecessors`; a later `E(n+1)=Hold` applies.
- **RP5B-4 (hidden REVOKE — root-attested):** a Register whose `attested_trust_head` exceeds `local_trust_head` ⇒ `AwaitingTrustPredecessors`; a compromised operational key **cannot** self-assert a stale trust head; `EmergencyRevoke` on the RecoveryLane is not blocked by a captured normal TrustStream epoch.
- **RP5B-5 (expiry never skips):** an expired `Awaiting*` still consumes its slot as `Rejected(Expired)` when predecessors catch up; the stream never wedges.
- **RP5B-6 (atomic apply / split generations):** effect + `Applied` + trust-CAS + the correct generation are one tx; policy vs trust generations are separate; `generation` moves only on `Applied`.
- **RP5B-7 (law over policy — exhaustive):** the closed `FleetCommandBody` cannot express a cap/toggle; the HOLD×`OperationKind` matrix is compile-exhaustive; mandatory drain/reconciliation/auto-Z-at-cap/legal-return proceed under HOLD.
- **RP5B-8 (independent HOLD + TTL):** a fleet release clears only its `hold_id`; an applied HOLD never auto-releases; indeterminate clock ⇒ fail-closed.
- **RP5B-9 (enrollment/restore anti-rollback):** `Enrolled` cannot revert to `Unenrolled`; a behind-epoch restore ⇒ fail-safe HOLD; missing/corrupt anchor ⇒ `EnrolledUnknown` + HOLD; anchor updated before the main DB.
- **RP5B-10 (trust anti-rollback):** revoked key ⇒ `Rejected(RevokedKey)`; emergency revocation needs the root key; a since-rotated key does not invalidate an `Applied` command; no bootstrap from an unknown key.

## 10 · Open questions for re-audit
1. **Root anchor medium:** the interface/content/update-order are locked here; the concrete medium (OS-guarded file / secure element) is CS-6 — acceptable?
2. **`TrustHeadAttestation` freshness:** does it need its own `attested_at`/TTL to stop a stale (but validly-root-signed) attestation from pinning an old head, or is monotonic `attested_trust_head_epoch` sufficient?
3. **RecoveryLane replay:** confirm `EmergencyRevoke` is contiguous within its own lane and that a captured normal `TrustStream` cannot mask a RecoveryLane revocation.
4. **Auto-Z-at-cap under HOLD:** confirm the "mandatory auto-Z proceeds, discretionary Z blocked" split is the right cut for the HOLD matrix.
