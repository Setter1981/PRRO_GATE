# Spec #5B — Fleet command lifecycle: Hold / Release / Policy (signed / scoped-epoch / PULL)

**Status: 🟡 DRAFT rev 2 (post external audit round 1 → NOT-YET, security-reshaped). 2026-07-15. Grounded on `origin/main` `c107854` + plan §3.10.**
Rev 2 closes round-1's security holes: (1) **scoped, CONTIGUOUS epochs** (a hidden epoch can no longer
bypass a HOLD/revocation); (2) a **trusted node-side intake** (the #5A agent has no crypto/store, so it
only PULLs opaque bytes); (3) **atomic apply** (effect + state + generation in one tx); (4) **fail-closed
restore / keyset anti-rollback**; (5) **law-over-policy made structural**. **Scope narrowed** to the core
**Hold / Release / Policy** commands — `Config` / `Provision` / `ProtocolRevision` are **not accepted**
until their own payload-specs define capability + quiescence + supersession. Semantics locked now; code
+ schema **dormant** (post-CS-5; runtime CS-6). The control-plane server is a **separate deployment**.

---

## 0 · Thesis
A fleet command is a **signed, scope-contiguous-epoch, PULL-delivered policy/hold intent**. The edge
**agent is dumb** (pulls opaque bytes only); a **trusted node-side intake** verifies and persists it in
a durable inbox **before ACK**; the **local coordinator is the only applier**, atomically; a command
can set **policy but never law**; a **restore or key-rollback fails closed**.

## 1 · What EXISTS to reuse (greenfield otherwise)
- Durable-before-ACK — `ingress_inbox` (`001:80-100`); the fleet inbox mirrors it as a **separate** table.
- The pure-oracle coordinator — Spec #1 (`admission = f(axes)`) — the **only** applier (CS-4).
- **`prro_crypto` is only a RAW primitive** (`crypto/provider.rs:60-74`: DSTU sign/verify over a caller-supplied digest + pubkey) — **NOT** a fleet verifier or key-store. The verifier + trust-store are **greenfield** (§4, §8).
- **Zero fleet-command code exists.** Dormant contract.

## 2 · Scope
Lock the semantics of **`Hold` / `Release` / `Policy`** commands only. **`Config` / `Provision` /
`ProtocolRevision` are REJECTED** until separate payload-specs define their capability set, quiescence
requirement, and supersession family. Code/DDL are dormant (post-CS-5); runtime pull/apply is CS-6; the
pilot ships the agent **OFF/advisory** with an **empty trust-store**.

## 3 · Two-layer transport (the #5A crate-DAG forces this)
- **`prro-fleet-agent` (dumb):** PULLs **opaque signed bytes** from the control-plane and hands them to a `FleetCommandIntake` **trait object**. Per #5A it depends on **no** `prro`/store/`sqlx`/crypto — so it **cannot verify or persist**; it is a transport shim only.
- **`FleetCommandIntake` (trusted, node-side):** verifies the signature + scoped epoch (§4–§5) and writes the durable `ReceivedDurable` inbox row. An **invalid/untrusted signature never enters the authoritative inbox** under its claimed `command_id` — it produces a **separate bounded security rejection + audit**, and **advances no epoch high-water**.

## 4 · Signed envelope + canonical bytes (fail-closed)
```rust
struct FleetCommandEnvelope {
    // DOMAIN SEPARATION — all SIGNED, so a key reuse cannot move a command across fleets/environments.
    domain_tag: [u8; 8],          // fixed protocol magic
    schema_version: u16,          // envelope schema
    canonical_bytes_version: u16, // the serialization rule id — a change cannot alias an old signature
    authority_id: AuthorityId,    // which control-plane
    fleet_id: FleetId,
    environment: Environment,     // Test | Prod
    // IDENTITY + ORDER
    command_id: CommandId,        // inbox idempotency key
    epoch_scope: EpochScope,      // RegisterStream(fleet_id, fiscal_number) | TrustStream(fleet_id)
    epoch: u64,                   // CONTIGUOUS within epoch_scope (§5)
    signer_key_id: KeyId,
    issued_at_unix_ms: u64,       // fixed-width, cross-language deterministic (NOT a String)
    expires_at_unix_ms: u64,      // signed TTL (§7)
    kind: FleetCommandKind,       // Hold | Release | Policy   (Config/Provision/ProtocolRevision REJECTED, §2)
    payload: FleetCommandPayload, // typed; NO legal caps / constants / enforcement toggles (I-law)
}
```
- **`canonical_bytes(envelope)`** = a deterministic, **versioned**, fixed-width/length-prefixed encoding (no float, no locale, no map-iteration order); it is the **only** bytes signed and hashed. **Golden test vectors** pin it. An **unknown `schema_version`/`canonical_bytes_version` ⇒ fail-closed `Rejected(UnknownVersion)`**, never a best-effort parse. The **hash + signature suite** is pinned (DSTU digest → `prro_crypto` verify). `prro_crypto` supplies only the primitive; the digest construction + suite id live in the fleet verifier.
- **INPUT provenance** (persisted, distinct from outcome): `FleetInputProvenance { command_id, epoch_scope, epoch, signer_key_id, signature_digest, envelope_hash, authority_id, fleet_id, environment }`.

## 5 · Scoped, CONTIGUOUS epochs (the BLOCKER fix)
- **Two signed scopes:** `RegisterStream(fleet_id, fiscal_number)` for anything mutating an FN's policy/hold; `TrustStream(fleet_id)` for key rotation/revocation. A **fleet-wide** mutation is **fanned out by the control-plane into separately-signed `Register` commands** — the edge never applies a "fleet" command directly.
- **Contiguity (kills the hidden-epoch bypass):** within a scope the accepted sequence must be **contiguous**. `epoch == last_accepted+1` ⇒ eligible; a **gap** (`epoch > last_accepted+1`) ⇒ `AwaitingPredecessors` (held, **not** applied, high-water **not** advanced) until the missing epochs replay or a **signed checkpoint** authorizes the jump. A hidden `E10=Hold/Revoke` can no longer be skipped by delivering `E11`.
- **Check order (explicit):** (1) verify signature/domain/version → else `Rejected(BadSignature)` **without advancing high-water** (else high-epoch garbage is a DoS); (2) **duplicate `command_id`** → return the persisted outcome (replay, no re-apply); **same `command_id` + different `envelope_hash`** ⇒ `IdempotencyConflict` (security incident), never replay; (3) `epoch ≤ last_accepted` ⇒ `Rejected(Stale)`; (4) gap ⇒ `AwaitingPredecessors`; (5) contiguous ⇒ admit. A same-epoch/different-command race resolves by an **atomic CAS** on `(scope, epoch)`.

## 6 · Lifecycle FSM + atomic apply + resume
```
ReceivedDurable → Applied | Rejected | Deferred
Deferred        → Applied | Rejected(Expired | Superseded | RevokedKey)
Applied | Rejected → immutable
```
- **Atomic apply:** the command **effect** (e.g. the HOLD record), the `state=Applied`, and the `effective_generation` bump commit in **ONE** transaction (a `TransitionPlan` / SQLite tx). A crash after the effect but before `Applied` is impossible — resume re-derives idempotently by `(command_id, epoch)`; re-apply of an `Applied` command is a no-op (generation matches).
- **Scoped supersession:** a newer command supersedes an older `Deferred` one **only within the same `supersession_family`** (a `conflict_key` / explicit `supersedes_command_id`) — a new `Hold` does **not** cancel a `Deferred` policy change of a different family.
- **ACK** carries `{epoch_scope, epoch, outcome_state, effective_generation, reason}`; the ACK is **authenticated by the transport** and correlates the signed scope/target. One `Register` command ⇒ one ACK ⇒ one generation.

## 7 · HOLD records + TTL (two clocks)
- **HOLD is a durable RECORD, not a boolean:** `{ hold_id, source: {Local|Fleet}, scope, applied_epoch }`. A `Release` references a **specific `hold_id`** and clears only it; **`effective_hold` = OR over all un-released records**. A fleet `Release` can never clear a `Local` safety hold.
- **HOLD never applies to a mandatory path:** law-permitted / mandatory **drain / reconciliation / mandatory return** proceed regardless of a fleet HOLD (I-law).
- **Two clocks (TTL):** **message validity** (`expires_at_unix_ms`) — an expired command is **never applied** (`Rejected(Expired)`); **effect lifetime** — an already-`Applied` HOLD does **NOT** auto-release on any TTL; it stays **held + alerting** until a **signed `Release`**. Clock-skew / clock-rollback are **fail-closed** (an indeterminate clock ⇒ do not apply, do not auto-release).

## 8 · Restore + keys (anti-rollback, fail-closed)
- **Restore = REFUSE-until-forward-sync.** On a fleet-enrolled node a restored `last_accepted_epoch` / keyset is **not trusted** without an authenticated replay/checkpoint or an external monotonic anchor. Write-admission for that FN is **held (fail-safe)** until the control-plane re-syncs; if the control-plane is unreachable, **stay held**. This must **not** reuse the baseline path that leaves an FN `ONLINE` when DPS is unavailable (`backup_restore.rs:809-840`) nor its kill-switch (`:895-929`) — those are not anti-rollback.
- **Trust-store (separate, secure, INACTIVE):** a `TrustStream`-versioned keyset (`keyset_epoch`) with a **root/recovery trust anchor**. **Routine rotation** may be signed by the current key; **emergency revocation** requires the root/recovery key. `ReceivedDurable`/`Deferred` **re-check the keyset generation before apply**; an already-`Applied` command stays valid (the epoch, not the live key, is authority). **Enrollment establishes the root LOCALLY** — trust can **never** be bootstrapped by a command signed by a not-yet-known key. An empty trust-store at advisory-OFF is fine.

## 9 · Law over policy (structural — I-law)
- A fleet command changes **ONLY** the fleet policy/hold axis. It **NEVER** sets `NodeMode`, never carries a ready `TransitionPlan`, and **legal caps / constants / enforcement toggles (168h / 36h / 50k) are ABSENT from the payload type** (so a command cannot even express "disable the cap").
- The **oracle recomputes law on EVERY admission/transition** (Spec #1), not only at command receipt — so a stored policy can never override a later legal check.
- **`Applied` means "policy durably stored"**, NOT "the requested mode was forcibly reached". A policy that would force illegal offline / a cap breach / block a mandatory return is `Rejected(LawViolation)` at the coordinator.

## 10 · RED-pins (semantics testable now; runtime dormant until CS-6)
- **RP5B-1 (canonical/signature):** a tampered envelope, wrong `domain_tag`/`environment`/`fleet_id`, or an unknown version ⇒ `Rejected` fail-closed; golden vectors pin `canonical_bytes`.
- **RP5B-2 (contiguous epoch — the BLOCKER):** a delivered `E(n+2)` while `E(n+1)` is missing ⇒ `AwaitingPredecessors`, high-water NOT advanced; a later `E(n+1)=Hold/Revoke` still applies. A hidden HOLD/revocation cannot be bypassed.
- **RP5B-3 (check order):** `BadSignature` advances no high-water; duplicate `command_id` replays; same id + different hash ⇒ `IdempotencyConflict`; stale epoch ⇒ `Rejected(Stale)`.
- **RP5B-4 (trusted intake / dumb agent):** the `prro-fleet-agent` crate cannot verify or persist (no crypto/store/sqlx per #5A); only `FleetCommandIntake` writes the inbox; an untrusted signature never occupies the authoritative `command_id`.
- **RP5B-5 (atomic apply/resume):** effect + `Applied` + generation are one tx; a crash before `Applied` re-derives idempotently; re-apply is a no-op.
- **RP5B-6 (law over policy — structural):** the payload type cannot express a cap/constant/toggle; a policy that would breach law ⇒ `Rejected(LawViolation)`; HOLD exempts mandatory drain/reconciliation/return.
- **RP5B-7 (independent HOLD records):** a `Fleet` release clears only its `hold_id`; a `Local` hold survives; `effective_hold` = OR.
- **RP5B-8 (TTL two-clock):** an expired command is never applied; an applied HOLD never auto-releases (held+alert until signed Release); indeterminate clock ⇒ fail-closed.
- **RP5B-9 (restore anti-rollback):** a restore behind the accepted epoch holds write-admission until an authenticated forward-sync; unreachable control-plane ⇒ stay held.
- **RP5B-10 (keyset anti-rollback):** a revoked key ⇒ `Rejected(RevokedKey)`; emergency revocation needs the root key; a since-rotated key does not invalidate an `Applied` command; trust cannot bootstrap from an unknown key.
- **RP5B-11 (scoped supersession):** supersession only within a `conflict_key`/family; a `Hold` never cancels a `Deferred` policy of another family.

## 11 · Open questions for re-audit
1. **`effective_generation` type:** a **new** `FleetPolicyGeneration` (NOT the Spec #2 delivery generation — 032 has only a *comment* about a future `node_state.delivery_generation`, `032:17-18`, and it fences a different authority). Confirm a distinct fleet generation.
2. **Signed checkpoint:** the exact shape of the "signed checkpoint" that authorizes an epoch jump after a real gap (a signed `(scope, up_to_epoch, state_digest)`?).
3. **Restore posture:** is "refuse-until-forward-sync + fail-safe hold" acceptable for the pilot (which is advisory-OFF, so arguably no accepted epoch exists yet), or is it purely a post-CS-6 obligation?
4. **Root/recovery anchor:** where does the local enrollment root live (a secure INACTIVE table + an out-of-band provisioning step), and does the advisory-OFF pilot ship any of it?
5. **Deferred retry transport:** the coordinator re-drives on shift-close/quiescent + boot; the next PULL may wake it but is not the sole mechanism — confirm.
6. **Scope:** is core `Hold`/`Release`/`Policy` the right #5B cut, with `Config`/`Provision`/`ProtocolRevision` as separate payload-specs?
