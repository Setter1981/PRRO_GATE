# Spec #5B — Fleet Hold/Release lifecycle (signed / scoped-contiguous-epoch / PULL)

**Status: 🟡 DRAFT rev 5 (post external audit round 4 → NOT-YET, near-lock). 2026-07-15. Grounded on `origin/main` `c107854` + plan §3.10.**
Rev 5 closes round-4's three residual blockers: (1) a **fresh, anti-replay `TrustHeadAttestation`**
(seq + TTL + anchor-tracked max, decoupled from the durable command) — an old root-signed head can no
longer be replayed; (2) a **real `HoldScope × OperationKind` matrix** (HOLD blocks discretionary new
business only; Z/X/Status/Reprint/drain/reconciliation/mandatory-return proceed); (3) an **unambiguous
anchor layout** (explicit per-scope high-water) + a **literal Canonical V1 appendix** (u16 tags, hash
profile, nested layout, golden vector). LOCK-READY carried forward: crypto-outside-tx (§3),
consume-on-expiry (§7). Scope = `Hold`/`Release` + `RotateKey`/`RevokeKey` + `EmergencyRevoke`; Policy
etc. → own specs. Semantics locked now; code+schema **dormant**; pilot ships **Unenrolled/advisory-OFF**.

---

## 0 · Thesis
Signed, scope-contiguous-epoch, PULL intents; **crypto before the tx**; a **fresh, anti-replay
root-attested trust head** gates every Register; **expiry consumes a slot, never skips**; a HOLD blocks
**only discretionary new business**; **enrollment + per-scope high-water are anti-rollback outside the
DB**; policy never overrides law.

## 1 · Reuse
Durable-before-ACK (`ingress_inbox` `001:80-100`, separate table); the pure-oracle coordinator (Spec #1,
sole applier); **`prro_crypto` = RAW primitive** (`crypto/provider.rs:60-74`: DSTU-4145 verify over a
**caller-supplied digest** + pubkey, **64-byte `r‖s` LE** signature) — the fleet verifier/trust-store/
root anchor are greenfield. Zero fleet-command code.

## 2 · Scope
`Hold`/`Release` (RegisterStream) + `RotateKey`/`RevokeKey` (TrustStream) + `EmergencyRevoke`
(RecoveryLane, root-signed). Policy/Config/Provision/ProtocolRevision → own payload-specs.

## 3 · Transport + intake — crypto OUTSIDE the tx (frozen-invariant #1) [LOCK-READY]
- **`prro-fleet-agent` (dumb):** PULLs opaque signed bytes **+ the fresh `TrustHeadAttestation`** → `FleetCommandIntake` trait object; per #5A no `prro`/store/`sqlx`/crypto.
- **`FleetCommandIntake` (trusted) — exact order:** (1) **outside any tx:** canonical-parse → crypto-verify the command signature **and** the fresh attestation (§5) → obtain a **snapshot `{keyset_generation, keyset_digest}`** + `attested_trust_head`; (2) `BEGIN IMMEDIATE`; (3) **CAS** the durable trust mirror **`(keyset_generation, keyset_digest)` == snapshot** *and* `local_trust_head ≥ attested_trust_head`; (4) INSERT inbox row **+** CAS register `stream_cursor n-1 → n`; (5) commit. **No crypto in the tx.** A CAS conflict ⇒ rollback + re-verify from (1). Auth-contiguous consumes its epoch (even if later Rejected/Deferred); pre-auth consumes nothing; an invalid envelope → bounded security-audit keyed by raw-envelope hash, no epoch advance.

## 4 · Signed wrapper + Canonical V1 (Appendix A normative, literal)
```rust
struct SignedFleetCommand { envelope: FleetCommandEnvelope, signature: FleetSignature } // 64-byte r‖s LE
struct FleetCommandEnvelope {
    domain_tag:[u8;8] /*b"PRROFLT1"*/, schema_version:u16 /*=1*/, canonical_bytes_version:u16 /*=1*/, suite_id:u16 /*=1*/,
    authority_id:AuthorityId, fleet_id:FleetId, environment:Environment, command_id:CommandId,
    epoch_scope:EpochScope, epoch:u64, required_trust_head:u64, signer_key_id:KeyId,
    issued_at_unix_ms:u64, expires_at_unix_ms:u64, body:FleetCommandBody,
}
// SEPARATE, FRESH, root-signed — NOT embedded in the durable command (so a long-waiting command's
// attestation cannot rot; the agent re-pulls a fresh one).
struct SignedTrustHeadAttestation { att: TrustHeadAttestation, root_signature: FleetSignature }
struct TrustHeadAttestation {
    domain_tag:[u8;8] /*b"PRROATT1"*/, authority_id:AuthorityId, fleet_id:FleetId, environment:Environment,
    attested_trust_head:u64, keyset_digest:[u8;32], attestation_seq:u64,
    issued_at_unix_ms:u64, expires_at_unix_ms:u64,   // TTL — see §5 anti-replay
}
```
**Appendix A — Canonical V1** (`schema_version=1`, `canonical_bytes_version=1`, `suite_id=1`):
`domain_tag=b"PRROFLT1"` (command) / `b"PRROATT1"` (attestation); **big-endian** fixed-width ints;
**`u32`-BE length-prefixed** byte strings; enum **`u16` tags**: `Environment{Test=1,Prod=2}`;
`EpochScope{RegisterStream=1,TrustStream=2,RecoveryLane=3}` (+ its `fleet_id`/`fiscal_number` fields);
`FleetCommandBody{Hold=1,Release=2,RotateKey=3,RevokeKey=4,EmergencyRevoke=5}`;
`HoldScope{NewBusinessAll=1,NewSalesOnly=2,NewShiftOpen=3}`; `HoldReasonCode{OperatorRequested=1,ComplianceReview=2,Provisioning=3,Suspected=4}`.
Strings UTF-8 **NFC**, ≤256 bytes. **Nested `body`/attestation** are encoded in declared field order,
each prefixed by its `u16` tag. **Digest profile:** `prro_crypto` verifies over a **caller-supplied digest**
(`provider.rs:62-71` — it fixes NO hash), so the fleet verifier **pins its own**: **`DSTU-7564`
(Kupyna-512) over `canonical_bytes`**. This is an **independent fleet-signing domain** (NOT the DPS
fiscal signature) — no external signer constraint; the golden vector fixes it. Signature = **64-byte `r‖s` LE**. **One
golden vector is REQUIRED before lock:** `canonical_bytes(envelope) → digest → 64-byte signature`.
Unknown `schema_version`/`canonical_bytes_version`/`suite_id` ⇒ fail-closed `Rejected(UnknownVersion)`.

## 5 · Epochs, streams, and the FRESH root-attested trust head (anti-replay)
- **Three scopes:** `RegisterStream(fleet_id, fn)`; `TrustStream(fleet_id)`; `RecoveryLane(fleet_id)` (root-signed `EmergencyRevoke`, not blocked by a missed/captured TrustStream epoch). Each has a contiguous cursor.
- **Fresh, anti-replay attestation (the round-4 fix):** the `SignedTrustHeadAttestation` is **root-signed** (never the operational key), **pulled fresh** alongside the command, and signs `authority_id + environment + fleet_id + attested_trust_head + keyset_digest + attestation_seq + issued/expires`. Admission requires ALL of: root-signature valid; `attestation_seq > anchor.max_observed_attestation_seq` **and** `attested_trust_head ≥ anchor.trust_head_floor` (monotone, anchor-tracked — a stale attestation is refused); **now ≤ expires_at** and **age ≤ a normative max (≤ 5 min)**; an **indeterminate/rolled-back clock ⇒ fail-closed**; and the **local `keyset_digest` == attested `keyset_digest`**. The anchor's `max_observed_attestation_seq`/`trust_head_floor` advance on acceptance (§8) — so an **old head-9 attestation cannot be replayed** once a newer one has been seen.
- **Guards:** a `Trust*` body only in `TrustStream`; `EmergencyRevoke` only in `RecoveryLane`; `new_keyset_epoch == envelope.epoch`; `epoch_scope.fleet_id == envelope.fleet_id`; `required_trust_head` is checked against the attested head, never self-asserted.
- **Check order:** signature/attestation/domain/version → else `Rejected(BadSignature)`, no advance; dup `command_id` ⇒ replay; same-id+different-hash ⇒ `IdempotencyConflict`; `epoch ≤ last_consumed` ⇒ `Rejected(Stale)`; register gap ⇒ `AwaitingPredecessors`; `local_trust_head < attested` ⇒ `AwaitingTrustPredecessors`; else consume + admit (atomic §3). **Checkpoints FORBIDDEN in V1** (full replay only).

## 6 · Closed bodies + `HoldScope × OperationKind` matrix (structural law)
```rust
enum FleetCommandBody { Hold(HoldBody), Release(ReleaseBody), RotateKey(..), RevokeKey(..), EmergencyRevoke(..) }
struct HoldBody { hold_scope: HoldScope, reason_code: HoldReasonCode }   // hold_id := the command_id
enum HoldScope { NewBusinessAll, NewSalesOnly, NewShiftOpen }
struct ReleaseBody { hold_id: HoldId }   // = the Hold command's command_id
```
`hold_id` **is the Hold command's `command_id`** (a `Release` names it). Caps/toggles are unrepresentable.
**Matrix (exhaustive, compile-checked over the real `OperationKind`; no `_ =>`):**
| OperationKind | NewBusinessAll | NewSalesOnly | NewShiftOpen |
|---|---|---|---|
| Sale | BLOCK | BLOCK | allow |
| Return (cash) | BLOCK | BLOCK | allow |
| ServiceIn / ServiceOut | BLOCK | allow | allow |
| OpenShift | BLOCK | allow | **BLOCK** |
| ZReport / XReport / Status / Reprint | **allow** (Z closes a shift / is safety-tightening; never HOLD-blocked) |
| Drain / Reconciliation / BootRecover / Probe | **allow** (mandatory / recovery) |
| mandatory online-return (legally required) | **allow** |
The oracle recomputes law each admission (Spec #1); a HOLD never blocks a **law-mandated** path (incl. a
mandatory auto-Z at a legal cap). A HOLD is **discretionary new-business gating only**.

## 7 · Lifecycle FSM — expired/revoked waits CONSUME their slot [LOCK-READY + one add]
```
(intake) → ReceivedDurable | AwaitingPredecessors | AwaitingTrustPredecessors | SecurityRejected(no epoch, no slot)
Awaiting* → ReceivedDurable (caught up) ; OR terminal-with-slot-consume (expired / revoked-key)
ReceivedDurable → Applied | Rejected | Deferred ;  Deferred → Applied | Rejected(Expired|Superseded|RevokedKey)
Applied | Rejected → immutable
```
- **No skip:** an `Awaiting*` that becomes terminal — `Rejected(Expired)` **or `Rejected(RevokedKey)`** (after trust catch-up) — still **consumes its slot** when predecessors arrive: one tx `Awaiting + cursor=n-1 → Rejected(..) + cursor=n`. The immutable tombstone is retained until consumed (or de-enrollment). A TTL/revoke **never** authorizes a silent skip.
- **Atomic apply (ONE tx):** effect + `Applied` + the trust-`(gen,digest)` CAS + the correct generation bump. **`Hold`/`Release` → per-`RegisterStream` `FleetPolicyGeneration`; `RotateKey`/`RevokeKey`/`EmergencyRevoke` → `TrustKeysetGeneration`** — never each other's; `generation` moves only on `Applied`. Supersession = one typed `ConflictKey`; `Hold`/`Release` never supersede; `Release` targets a `hold_id`.
- **ACK (all three scopes):** `{ epoch_scope, epoch, outcome_state, effective_generation, reason }`, transport-authenticated; `effective_generation` is **tagged** — `Register(FleetPolicyGeneration) | Trust(TrustKeysetGeneration) | Recovery(TrustKeysetGeneration)`.

## 8 · Anchor + trust-store + restore (explicit layout, anti-rollback)
- **`FleetAnchor` (outside the main DB's backup domain) — ONE explicit layout:**
  `{ enrollment_state: Unenrolled|Enrolled|EnrolledUnknown, root_fingerprint, trust_stream_cursor, recovery_lane_cursor, per_fn_register_cursor: map<fn, u64>, trust_keyset_generation, trust_head_floor, max_observed_attestation_seq }`.
  A **fresh install MUST write an explicit `Unenrolled` anchor**; a **missing/corrupt anchor ⇒ `EnrolledUnknown` + fail-safe HOLD** (never `Unenrolled`).
- **Restore rule:** `Unenrolled` ⇒ not blocked. `Enrolled` + a restored DB whose any per-scope cursor / `trust_keyset_generation` is **behind** the anchor ⇒ **fail-safe HOLD** until an authenticated forward-sync; control-plane unreachable ⇒ stay held. `Enrolled` can never revert to `Unenrolled` via restore.
- **Update protocol (crash-safe, anchor-first):** advance the external anchor **before** the main DB; a crash yields a **false HOLD** (safe), never a rollback bypass.
- **Trust-store:** `keyset_epoch`/digest; routine `RotateKey` signed by the current key; **`EmergencyRevoke` requires the root/recovery key**; apply re-checks `(generation, digest)` by CAS; an `Applied` command stays valid; trust never bootstraps from an unknown key (enrollment sets the root **locally, out-of-band**). Must **not** reuse `backup_restore.rs:809-840` (DPS-down-stays-ONLINE) nor its kill-switch (`:895-929`).

## 9 · RED-pins
- **RP5B-1 (canonical/suite/no-crypto-in-tx):** the golden vector pins Appendix A; crypto is outside the tx; unknown version/suite / bad domain / bad attestation ⇒ fail-closed.
- **RP5B-2 (atomic intake):** inbox + register-cursor + trust-`(gen,digest)` CAS in one tx; auth-contiguous consumes, pre-auth does not; CAS conflict re-verifies.
- **RP5B-3 (hidden HOLD):** a register gap holds durably; a later predecessor `Hold` applies.
- **RP5B-4 (hidden REVOKE, anti-replay):** a stale but validly-root-signed attestation (`attestation_seq ≤ anchor.max` or expired) ⇒ refused; a compromised key cannot replay an old head; `EmergencyRevoke` (RecoveryLane) is not maskable by a captured TrustStream.
- **RP5B-5 (no skip on expiry/revoke):** an expired **or** revoked-key `Awaiting*` still consumes its slot; the stream never wedges.
- **RP5B-6 (atomic apply / split generations / tagged ACK):** effect + `Applied` + trust-CAS + the correct generation are one tx; policy vs trust generations separate; ACK generation is tagged.
- **RP5B-7 (HoldScope × OperationKind — exhaustive):** compile-exhaustive; Z/X/Status/Reprint/drain/reconciliation/mandatory-return proceed under every HoldScope; only discretionary new business per scope is blocked.
- **RP5B-8 (independent HOLD + TTL):** a fleet release clears only its `hold_id`; an applied HOLD never auto-releases; indeterminate clock ⇒ fail-closed.
- **RP5B-9 (enrollment/restore anti-rollback):** fresh install writes `Unenrolled`; missing/corrupt ⇒ `EnrolledUnknown`+HOLD; behind-cursor restore ⇒ HOLD; anchor-first.
- **RP5B-10 (trust anti-rollback):** revoked key ⇒ `Rejected(RevokedKey)`; emergency revoke needs the root key; a rotated key does not invalidate an `Applied` command; no unknown-key bootstrap.

## 10 · Open questions for re-audit
1. Confirm the **anti-replay attestation** (seq + TTL + anchor-tracked `max_observed_attestation_seq`/`trust_head_floor` + local-digest match) fully closes the stale-attestation replay.
2. Confirm the **`HoldScope × OperationKind`** cut (allow all Z/X/Status/Reprint; block only new business per scope) against the frozen Spec #1 admission oracle.
3. Confirm the **explicit anchor layout** + anchor-first protocol + fresh-install-`Unenrolled` is sufficient for autonomous behind-restore detection.
4. **`OperationKind` source:** derive it from the locked `FiscalCommandKind`/`DocType` (+ Status/Reprint/Probe/BootRecover as non-DocType ops) — confirm the enum is closed and complete.
5. **Digest/hash profile (§4) — RESOLVED by code:** `prro_crypto` takes a **caller-supplied** digest (`provider.rs:62-71`), so the fleet is an **independent signing domain** and PINS its own profile (`DSTU-7564`/Kupyna-512) — no external signer constraint. Confirm the choice; the golden vector can be frozen now.
