# Spec #5B — Fleet command lifecycle (signed / epoch / PULL — semantics locked, code dormant)

**Status: 🟡 DRAFT rev 1 (for external audit). 2026-07-15. Grounded on `origin/main` `c107854` + locked plan §3.10.**
Home: **`prro-fleet-contract`** (the command port) + **`prro-fleet-agent`** (edge agent). Companion to
the LOCKED Spec #5A (telemetry read-model). Per the audit + plan §3.10, the fleet command **semantics
must be locked NOW**; the **code + schema stay dormant** (post-CS-5, runtime-activated at CS-6). The
control-plane server is a **SEPARATE deployment**, never inside the edge binary. Fleet =
**ADVISORY-only** for the pilot (N=1, agent OFF); enabling it later is deployment/config, not a
re-cut. This is greenfield (zero fleet-command code today) — the spec reuses three proven patterns:
the durable `ingress_inbox` (durable-before-ACK), the Spec #2 reservation **generation** (fencing),
and the Spec #1 **pure-oracle coordinator** (the only applier).

---

## 0 · Thesis
A fleet command is a **signed, epoch-versioned, PULL-delivered** intent that lands in a **durable
inbox before it is ACKed**, is applied **only by the local per-FN coordinator**, and can set **policy
but never override law**. The edge agent has **no store-mutation access**. Input provenance (what was
signed) and apply outcome (what the coordinator durably did) are **separate**.

## 1 · What EXISTS to reuse (greenfield otherwise)
- **Durable-before-ACK** pattern — the `ingress_inbox` (`001:80-100`): a durable row committed before the client is told "accepted". The fleet inbox mirrors it (§3, I5) but is a **separate table**.
- **The generation/fence** — Spec #2's `delivery_reservation.generation` + the per-FN chain-generation fence: the fleet ACK's `effective_generation` reuses this "monotonic durable token" idea.
- **The pure-oracle coordinator** — Spec #1 (`admission = f(axes)`, `Allow|Denied|Deferred|NoTransition`): the coordinator is the **only** applier of a fleet command (CS-4).
- **The signer** — `prro_crypto` (existing) verifies command signatures.
- **Zero fleet-command representation exists today** (verified in Spec #5A §1); this is a dormant contract.

## 2 · Scope
Lock the **full semantics** (§3–§6). **Dormant:** the `prro-fleet-contract` command types + the
separate INACTIVE fleet-inbox schema land as code/DDL post-CS-5; **runtime** pull/apply is CS-6. Do
NOT wire a live emitter/applier now.

## 3 · Key types (`prro-fleet-contract`, sqlx-free) — INPUT vs OUTCOME split
```rust
// ── SIGNED INPUT (what the control-plane signed; the edge verifies, never fabricates) ──
struct SignedFleetCommand {
    envelope: FleetCommandEnvelope,        // the canonical-bytes-hashed, signed payload
    signature: Signature,                  // over canonical_bytes(envelope); verified vs signer_key_id
}
struct FleetCommandEnvelope {
    command_id: CommandId,                 // stable identity — the inbox idempotency key
    epoch: FleetEpoch,                     // monotonic per (fleet-scope); see I2
    target: CommandTarget,                 // { Fleet | Register(FiscalNumber) }
    kind: FleetCommandKind,                // Policy | Hold | Release | Config | ProtocolRevision | Provision
    signer_key_id: KeyId,
    issued_at: String,
    ttl: Duration,                         // expiry (I8)
    payload: FleetCommandPayload,          // kind-specific, typed (never arbitrary JSON)
}
// `canonical_bytes(envelope)` is a DETERMINISTIC, versioned serialization — the ONLY bytes signed and
// hashed; a `canonical_bytes_version` is part of the envelope so a format change cannot alias signatures.

// The signed INPUT provenance (carried into the durable inbox row) — distinct from the apply outcome.
struct FleetInputProvenance { command_id: CommandId, epoch: FleetEpoch, signer_key_id: KeyId, signature_digest: [u8; 32], envelope_hash: [u8; 32] }

// ── DURABLE APPLY OUTCOME (what the LOCAL coordinator durably decided — separate from the input) ──
enum FleetCommandState { ReceivedDurable, Applied, Rejected, Deferred }
struct FleetApplyOutcome {
    state: FleetCommandState,
    effective_generation: Generation,      // the durable token the ACK reports (reuses Spec #2 generation)
    reason: FleetOutcomeReason,            // stale/dup/reorder/gap/expired/open-shift/law-violation/applied
    // NOTE: the contract does NOT embed the engine's `CoordinatorOutcome` type — only a protocol-neutral reason.
    decided_at: String,
}
// The PULL ACK the agent returns to the control-plane:
struct FleetAck { command_id: CommandId, epoch: FleetEpoch, outcome_state: FleetCommandState, effective_generation: Generation, reason: FleetOutcomeReason }

// Independent HOLDs (I6): a release clears ONLY its own source's hold.
enum HoldSource { Local, Fleet }
// effective_hold(FN) = ∃ an un-released hold from ANY source.
```

## 4 · Lifecycle FSM + crash resume
```
PULL → verify(signature, epoch, ttl) → persist ReceivedDurable (in the fleet inbox, BEFORE ACK)
     → coordinator applies →  Applied            (policy set; ACK{epoch,Applied,gen,reason})
                          |→  Rejected            (bad sig / stale|dup|reorder|gap epoch / expired TTL / would break law)
                          |→  Deferred            (open shift for a Config/ProtocolRevision change; retried; a HIGHER-epoch command supersedes a Deferred one)
```
- **Durable-before-ACK:** the `ReceivedDurable` row commits **before** any ACK; the ACK reports the coordinator's durable outcome, never an optimistic one.
- **Crash/boot resume (idempotent):** a `ReceivedDurable` row not yet `Applied`/`Rejected` is re-driven on boot by the coordinator **idempotently** (keyed by `command_id` + `epoch`); re-apply of an already-`Applied` command is a no-op (the effective_generation matches). No fleet command re-pulls or re-signs on resume.
- **Deferred supersession:** `Deferred` commands are ordered by `epoch`; a newer-epoch command for the same target **supersedes** an older `Deferred` one (which becomes `Rejected(Superseded)`), so a stale config never applies after a newer one.

## 5 · Normative invariants
- **I1 (signed + canonical bytes).** A command applies only if its `signature` verifies over `canonical_bytes(envelope)` against a **currently-valid** `signer_key_id`; the canonical serialization is deterministic + versioned (a `canonical_bytes_version` change cannot alias an old signature). A forged/tampered command ⇒ `Rejected(BadSignature)`.
- **I2 (monotonic epoch; stale/dup/reorder/gap).** `epoch` is monotonic per fleet-scope. `epoch ≤ last_accepted_epoch` ⇒ `Rejected(Stale)` (dedup + reorder guard); a **duplicate** `command_id` returns the persisted outcome (replay, no re-apply); a detected **gap** does not block (later epochs may apply) but is surfaced. **The accepted epoch is durable + monotonic.**
- **I3 (durable-before-ACK; ACK shape).** The inbox row is durable before the ACK; the ACK carries exactly `{epoch, outcome, effective_generation, reason}` (plan §3.10a).
- **I4 (coordinator-only apply; no agent store-mutation).** ONLY the local per-FN coordinator applies a command (via its mailbox/API — not the CLI admin, not the supervisor as sole transport); the **fleet-agent has no direct store-mutation access** (the §3.5 fencing discipline — same as the ingress adapters). The agent pulls, verifies, persists `ReceivedDurable`, and hands off; it never writes fiscal/node state.
- **I5 (separate INACTIVE fleet inbox — MUST).** The durable command inbox is a **separate table** (idempotency-keyed on `command_id`, worker-triggered) with `epoch` + `signature_digest` + `FleetInputProvenance` + the apply outcome columns — INACTIVE-first, **never** overloading `ingress_inbox`.
- **I6 (independent HOLDs).** `Local` and `Fleet` HOLDs are stored independently; a `Release` clears **only its own source's** HOLD; `effective_hold` = OR across sources. A fleet release can never clear a local safety HOLD.
- **I7 (fleet gives policy, local enforces law).** A command sets *policy* but can **NEVER** force illegal offline entry, a legal-cap breach (168h/36h/50k), or block a mandatory return — such a command ⇒ `Rejected(LawViolation)` at the coordinator (mirrors plan §3.10b + §7.6). Advisory ≠ authority.
- **I8 (TTL).** A command past its `ttl` ⇒ `Rejected(Expired)`, never applied late.
- **I9 (open-shift deferral).** A `Config`/`ProtocolRevision`/profile change with an **open write shift** ⇒ `Deferred` (or `Rejected`) — it may not switch the write-enabled `IngressProfileId`/`DpsProtocolId` mid-shift (extends the frozen no-channel-switch invariant, plan §3.9).
- **I10 (backup/restore never rolls back an accepted epoch).** A restore to an older DB state must **not** revert `last_accepted_epoch`; the accepted epoch is reconciled **forward** (or the restore is refused for that FN) — an accepted fleet policy is durable across restore (plan §3.10a).
- **I11 (key rotation/revocation).** `signer_key_id` is resolved against a durable, fleet-signed key set; a **revoked** key's future commands ⇒ `Rejected(RevokedKey)`; a rotation is itself a signed command; a command already `Applied` under a since-rotated key stays valid (the epoch, not the live key, is the authority).
- **I12 (input ≠ outcome).** `FleetInputProvenance` (signed input) and `FleetApplyOutcome` (durable coordinator decision) are **separate** types; the contract carries only a protocol-neutral `FleetOutcomeReason`, never the engine's `CoordinatorOutcome`.

## 6 · RED-pins (dormant/known-red until CS-6 activation; semantics testable now)
- **RP5B-1 (signature/canonical):** a tampered envelope or a `canonical_bytes_version` change that would alias a signature ⇒ `Rejected(BadSignature)`; a valid command applies.
- **RP5B-2 (epoch monotonic):** `epoch ≤ last_accepted` ⇒ `Rejected(Stale)`; a duplicate `command_id` replays the persisted outcome (no re-apply); a reordered lower epoch does not overwrite a higher accepted one.
- **RP5B-3 (durable-before-ACK + resume):** a crash between `ReceivedDurable` and `Applied` re-drives idempotently on boot; the ACK never precedes the durable row; re-apply of an `Applied` command is a no-op.
- **RP5B-4 (coordinator-only / no agent mutation):** a static pin — the fleet-agent crate cannot reach a store mutator (reuses the Spec #5A crate-DAG gate); the only apply path is the coordinator mailbox.
- **RP5B-5 (law over policy):** a command that would force illegal offline / cap-breach / return-block ⇒ `Rejected(LawViolation)`, applied by no coordinator.
- **RP5B-6 (independent HOLD):** a `Fleet` release does not clear a `Local` HOLD (and vice versa); `effective_hold` stays set while any source holds.
- **RP5B-7 (open-shift deferral):** a `ProtocolRevision`/profile change with an open shift ⇒ `Deferred`/`Rejected`, never a mid-shift channel switch.
- **RP5B-8 (TTL):** an expired command ⇒ `Rejected(Expired)`.
- **RP5B-9 (backup no-rollback):** restoring an older DB does not revert `last_accepted_epoch`; the accepted epoch reconciles forward.
- **RP5B-10 (key revocation):** a revoked key's command ⇒ `Rejected(RevokedKey)`; a since-rotated key does not invalidate an already-`Applied` command.
- **RP5B-11 (Deferred supersession):** a higher-epoch command supersedes an older `Deferred` one (`Rejected(Superseded)`).

## 7 · Open questions for the audit
1. **Epoch scope:** monotonic per **fleet** (one global epoch line) or per **FN**? Plan §3.10 says "epoch-versioned"; per-FN is simpler for N=1 but a fleet-wide policy may need a fleet epoch — which is authoritative for a `Fleet`-target command vs a `Register`-target one?
2. **`effective_generation` vs Spec #2 `generation`:** reuse the same durable token/type, or a distinct fleet generation? (They fence different things — delivery vs policy.)
3. **Backup/restore reconciliation (I10):** is "reconcile forward" (fetch the current accepted epoch from the control-plane on boot) acceptable, or must the pilot simply **refuse** to serve an FN whose restored epoch is behind until an operator re-syncs?
4. **Key set storage:** where does the durable fleet-signed key set live (a separate INACTIVE table?), and is the pilot's advisory-OFF posture consistent with shipping a key set at all?
5. **Deferred retry cadence:** who re-drives a `Deferred` command (the coordinator on shift-close, or the next pull)? — the plan implies the coordinator; confirm the transport.
6. **Split vs merge:** is this the right #5B scope, or should provisioning/config land as a later spec once CS-6 defines the transport?
