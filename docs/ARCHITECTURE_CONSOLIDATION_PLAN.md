# Architecture Consolidation Plan — for External Audit

**Status: PLAN — rev 5 · 🔒 LOCKED (2026-07-14). Ready for strangler execution.**
**LOCKED:** operator ratified §16.7 = **V1** (whole-shift RMR fail-safe default; per-doc quarantine
= separate later ADR) + fleet **ADVISORY-ONLY** for the pilot (§9); the auditor's final text fixes
are applied — §5 step-2 split into 2a–2d (one semantic change per PR), §8 honest sizing (no single
conflated range; §7 + adapters estimated separately), §5.1/§10 test-gate nuance (black-box unchanged
only in behaviour-neutral PRs; a semantic bugfix carries its own regression PR; `prro-testkit`
`publish=false`; CI matrix excludes manual live-dps). No new audit round required. Baseline `8ec99ca`.
First move: `prro-domain` behaviour-neutral extraction; typed delivery + double-issue as the separate
next semantic PR (§5 step 2c).
Rev 5 folds in the external auditor's final NOT-YET amendments (foundation accepted; no new audit
round needed): single type-ownership — `domain` owns `FiscalCommand`/value-types/protocol-neutral
`SubmissionEvidence`, `ingress-contract` owns `CanonicalIngressEnvelope`, adapters have no store
mutation (§4, §5.1); `IdempotencyStrategy` incl. `NoSafeReplayIdentity` → write-forbidden without a
provable key (§3.8); durable-reservation crash semantics `ReservedNotStarted→CallStarted→
OutcomeObserved` (§3.6); full fleet command lifecycle `ReceivedDurable|Applied|Rejected|Deferred` +
independent local/fleet HOLD + open-shift-Deferred + backup-epoch-preserved (§3.10); test-moat
preservation via `prro` facade + `prro-testkit`, black-box tests unchanged (§5.1); normalized
sizing ~10–17 pw + 1–2 pw tests (§8). **Remaining to LOCK: operator ratifies §16.7 (recommended
V1: whole-shift RMR default, per-doc quarantine = later ADR) + fleet OFF for pilot (§9).**

Rev 4 made **FLEET a first-class architectural boundary** (the product moat): a
`prro-fleet-contract` port + edge `prro-fleet-agent`, signed/epoch-versioned/pull commands with
per-register ack, local coordinator as final safety arbiter (fleet gives policy, local enforces
law), orthogonal state axes as the fleet telemetry read-model, N=1 degeneration for the pilot
(agent OFF/advisory) with no architecture re-cut to enable later (§3.10, §4, §8, §10).
Rev 2 folded in the external-audit corrections for the **second DPS protocol (ЕВПЗ)**: DPS
contract-split crate graph (§4), durable protocol binding + cross-protocol-fallback prohibition
(§4), the strengthened delivery model + conservative `SubmittedUnknown` (§3.2, §6), staleness-
protected `TransitionPlan` + actor execution model (§3.5-3.6), common-mode-safe fuzzer oracle
(§3.4, §10), commit-SHA-pinned references (Appendix). **Rev 3 adds the symmetric INGRESS boundary
(many compatibility protocols):** M+N-not-M×N canonical ingress command + strict `FiscalCommand`
enum + central durable-inbox idempotency + the three-way `IngressProtocolId / DpsProtocolId /
FiscalMode` split (§3.7-3.9), the ingress crate family + native-DLL-sidecar rule (§4), symmetric
extraction (§5), and the shared ingress contract-suite gate (§10). **Sound as an execution basis;
lock after operator sign-off.**

Audience: external auditor + implementers. For every decision this plan states the
**problem, the alternatives considered, the choice, and why**, plus invariant-preservation
and per-phase acceptance gates so the plan itself can be reviewed before any code moves.

> **Framing decision (the reason this plan exists NOW).** There is **no installed base yet
> — zero live cash registers.** This is the cheapest possible moment for an irreversible
> architectural correction: we may break internal APIs, reset test DBs, and drop old formats
> with no migration burden and no legally-significant in-flight documents to preserve. After
> the first real deployment the same work costs ~2-3× (persisted state, in-place upgrades,
> compliance-bound unfinished documents). **This is NOT a rewrite** — the fiscal core is
> strong and stays. It is a **consolidation of authority**.

---

## 1 · What we KEEP (verified strong — do not touch)

| Area | Score (external audit) | Why it stays |
|---|---|---|
| Ledger / SQLite / short tx / CAS / durable truth / audit-trace | 8.5 | correct edge model |
| Fiscal write-path (acquire/sign/send/finalize) | 8 | explicit commit points |
| Idempotency & chain safety | 8 | real, deliberate |
| Test culture (fuzzer / mutation teeth / live-DPS) | 9 | rare strength — the moat |

Invariants that stay hard: ledger-first; **no network/crypto inside a write tx**; durable
intermediate states; **per-FN isolation**; recovery-as-product. The 10 frozen invariants
(CLAUDE.md) remain binding.

## 2 · The problem (why B11 broke)

Correctness is currently held by **many local guards**; each is sound, but their **composition
is not a single machine**. Today there are ~8 overlapping FSMs — document, shift,
offline-session, node-mode, inbox/lease, transport-attempt, reconciliation, (future)
fleet-control — and a transition in one **assumes the implicit state of the others**. B11 fell
apart not inside a function but **at the seams** (mode=OFFLINE yet session=DRAINING;
doc=ERROR_RETRYABLE yet the detector waits for a new request; fleet-hold set yet drain runs
under a different mutex).

Three root faults (scored 4-4.5/10):
1. **NodeMode is overloaded** — one axis mixes connectivity + fiscal session + admin ban +
   legal cap + crypto degradation + recovery/RMR. These must be **orthogonal** states.
2. **No single per-FN transition owner** — `fn_gate`, `reconcile_mutex`, probe, admin and raw
   CAS are different serialization domains. "one FN = one writer" is not systemically true.
3. **Delivery certainty is lost** — the transport/server/decode discriminant is computed then
   discarded (see §6), so downstream cannot reason about whether a document was delivered.

## 3 · Target architecture (consolidate authority — ports & adapters)

1. **One per-FN transition coordinator/actor.** Every mutation for an FN — fiscalize, drain
   tick, probe, fleet command, admin command, timer, boot recovery — passes through it. It is
   the single writer and the final arbiter; the single-writer lease already serializes flips,
   so the coordinator is **thin once the pieces below exist**.
   **⚠ Anti-god-object guardrail (a bad coordinator just RELOCATES the monolith — consolidating
   *authority* is not consolidating *logic*):** the coordinator **ORCHESTRATES; it does NOT own
   fiscal logic.** Decisions come from the **pure `prro-domain` oracle** (`(command, state) →
   TransitionPlan`, §3.4-3.5); side effects go **through ports** (store/dps/ingress/fleet/signer/
   clock); durable state is owned by the **store**, not the coordinator (it is **stateless between
   commands** bar the in-flight reservation). Command handling is **ONE uniform lifecycle**
   (reserve → call port → observe outcome → apply plan → emit event), **not** N bespoke per-command
   handlers with inlined rules. **Canaries:** a hard **size/complexity budget** on the coordinator
   crate; it must be **unit-testable with FAKE ports** — if it needs real SQLite/gRPC to test,
   fiscal logic has leaked in. This is a REQUIRED review + acceptance gate (§10).
2. **Typed delivery outcome** —
   `NotSubmitted | SubmittedUnknown | ResponseObserved(Accepted | Rejected | Malformed)`.
   We stop guessing whether a doc reached DPS. **Correction (external audit): `retry_class =
   Transport` does NOT prove `NotSubmitted`.** Once the gRPC call has *started*, most
   `timeout`/`Unavailable` must be classified **conservatively as `SubmittedUnknown`**. A
   `SubmittedUnknown` document is **never blind-resent** — it goes to reconciliation on its
   original protocol. (So merely threading the existing discriminant is **necessary but NOT
   sufficient** to fix double-issue — see §6.)
3. **Orthogonal durable state axes** — split NodeMode into independent fields/tables:
   `connectivity evidence + fiscal session state + local/fleet sales holds + recovery health
   + shift state` → **effective admission policy = f(axes)** (may we acquire / sign / issue /
   send this document?).
4. **One data-driven transition table** — drives the runtime edges AND the documentation.
   **It is NOT the fuzzer's only oracle:** deriving both the runtime and the test model from a
   single table is a **common-mode error** (a wrong table yields an equally-wrong runtime and
   test). The fuzzer therefore ALSO carries **independent frozen invariants**, and the
   adversarial double-issue / cap / seed / fleet scenarios are **not** generated from the table.
5. **Domain returns a whole `TransitionPlan`; the SQLite adapter applies it atomically — with
   staleness protection.** The plan carries `expected_state_versions` + a coordinator
   `generation`/fencing token + CAS-preconditions; application returns **`Applied | Conflict`**;
   on `Conflict` the actor **recomputes** against fresh state. (Else an actor may decide on a
   stale snapshot while boot/admin/another process has already moved state.) This is how the
   "atomic mode/session/shift/doc change" invariant survives a crate boundary — the engine
   never assembles one fiscal transaction from ten CRUD calls, and storage never leaks a raw
   `SqliteTransaction` outward.
6. **Actor execution model (define UP FRONT):** whether the coordinator holds its queue during
   a network await; max in-flight ops per FN; priorities (fiscalize / drain / probe / STOP /
   admin); starvation + backpressure; graceful shutdown; a durable command inbox; boot fencing.
   **Safe V1: exactly ONE state-mutating command per FN** — the network stage runs via a durable
   reservation and its result returns to the actor as a **new event carrying a generation token.**
   **Durable-reservation crash semantics (closes the crash-between-call-and-result window):**
   `ReservedNotStarted → CallStarted → OutcomeObserved`. `CallStarted` is fixed **durable BEFORE**
   the network call. Reboot at `ReservedNotStarted` → safe cancel / `NotSubmitted`; reboot at
   `CallStarted` without a result → **strictly `SubmittedUnknown`**. The reservation carries
   `attempt_id + protocol_binding + envelope_hash + generation`; during a call a second fiscal
   submission cannot start; `STOP` / local `HOLD` may still apply as a safety-tightening command;
   and the **old raw mutation APIs become inaccessible outside the coordinator / store adapter**
   (else the old crash window survives under a new name).
7. **Symmetric ingress boundary — M + N, never M × N.** Many ingress protocols (compatibility
   with existing tills) × two DPS protocols must NOT become a matrix of separate paths. All
   ingress adapters normalize to ONE `CanonicalIngressEnvelope` → one `FnCoordinator` → N DPS
   protocols, so complexity is **M + N**. The envelope is fully typed: `schema_version,
   source_protocol, compatibility_profile_version, source_installation_id, source_request_id,
   internal_operation_id, fiscal_number, payload_hash, FiscalCommand`. `FiscalCommand` is a
   **strict enum** (`OpenShift | Sale | Return | ZReport | XReport | ServiceIn | ServiceOut |
   Status | Reprint`) with per-variant payloads — **not** one giant DTO of 50 `Option`s.
   Money / taxes / payments / discounts / rounding are **canonical before the engine.** A legacy
   protocol missing a required field: the adapter **never silently invents** a value — only an
   explicit, versioned mapping-snapshot or fail-closed.
8. **Central idempotency (durable inbox BEFORE the ingress ACK).** Every envelope lands in a
   durable inbox **before** the ingress client is acknowledged. Key =
   `(source_protocol, source_installation_id, source_request_id, operation_kind)`. Same key +
   same `payload_hash` → return the **stored** result; same key + different payload →
   `IdempotencyConflict`; a lost POS→gateway reply **never** mints a second fiscal document; the
   adapter does not re-invoke the engine if the op is already `SubmittedUnknown` or done. Every
   ingress adapter declares an explicit **`IdempotencyStrategy = SourceStableId | ProtocolStableTuple
   | GatewayReservation | NoSafeReplayIdentity`**. `NoSafeReplayIdentity` means **write commands are
   FORBIDDEN** until the adapter supplies a provable key — a **payload hash is NOT an identity**. **No
   write-enabled ingress adapter registers without a crash/replay contract-test.** This closes the
   POS→gateway double-issue for **all** protocols (the concrete design for operational item §7.1).
9. **Split the overloaded word "channel" into three axes:** `IngressProtocolId` (how a POS talks
   to the gateway), `DpsProtocolId` (how the gateway talks to DPS), `FiscalMode` (ONLINE/OFFLINE).
   For an open shift, durably fix the **write-enabled `IngressProfileId` AND `DpsProtocolId`**;
   other ingress protocols may serve **read-only status**, but simultaneous *write* via multiple
   compatibility channels requires a separate frozen-invariant review.
10. **Fleet is a FIRST-CLASS boundary (the product's moat), not an afterthought.** The per-FN
    coordinator IS the fleet's unit of control — the core is already fleet-shaped (per-FN
    isolation). Fleet support is an explicit axis, built now while there is no installed base
    (re-cutting it after the first register is expensive):
    - **(a) A `prro-fleet-contract` PORT, symmetric to ingress/dps.** The fleet control-plane is
      an EXTERNAL authority (a separate deployment — **not** inside the edge binary); the edge
      exposes only a fleet-agent. Commands are **durable, authenticated (signed), epoch-versioned,
      PULL-honored** (policy / hold / release / config + protocol-revision / provisioning). Command
      **lifecycle = `ReceivedDurable | Applied | Rejected | Deferred`**: the command lands in a
      durable fleet inbox **before** ACK; the ACK carries `epoch + outcome + effective_generation +
      reason`. `local` and `fleet` HOLDs are stored **independently** (a release clears **only its
      own source's** HOLD); a protocol/profile/config change with an **open shift** is `Deferred`
      or `Rejected`; a **backup restore never rolls back an accepted fleet epoch**; the fleet-agent
      has **no direct store-mutation access** (same fencing discipline as §3.5).
    - **(b) The local coordinator is the final SAFETY arbiter.** A fleet command sets *policy* but
      can NEVER force illegal offline entry, a legal-cap breach, or block a mandatory return —
      **fleet gives policy, local enforces law** (mirrors §7.6). A forged/duplicate/reordered
      command is rejected by signature + epoch guard.
    - **(c) The orthogonal state axes (§3.3) ARE the fleet's observable read-model / telemetry**
      (per-FN connectivity, fiscal-mode, holds, budget, recovery-health) — the control-plane reads
      them; it does not reach into local state.
    - **(d) N=1 degeneration.** For the single pilot "fleet" is one FN + one `node_state` row and
      the fleet-agent is **OFF / advisory-only**, but the boundary is built so enabling it later is
      a **deployment/config change, not an architecture re-cut.** Fleet-by-semantics,
      single-instance-by-deployment.

**Alternative rejected: full rewrite.** The fiscal core scores 8-9; rewriting discards proven
correctness + the test moat and re-introduces solved bugs. **Alternative rejected: keep patching
B11 onto the current model.** With no installed base, correcting the coordination model now is
cheaper than accumulating more defensive layers on a model B11 already outgrew.

## 4 · Crate split — by STATE-OWNERSHIP boundaries (not folders)

Splitting by "folder = crate" would reproduce today's cycles between crates. Split by
**dependency direction and state owner**:

```
prro                     composition root / wiring (config, admin/CLI, supervisor, Windows service, adapter registration)
├── prro-domain          pure model + transition oracle: FiscalNumber, FiscalCommand, fiscal value types,
│                        protocol-neutral SubmissionEvidence, docs/shifts/offline-sessions, cap policy, pure FSM.  NO sqlx/tonic/tokio/axum.
├── prro-engine          FnCoordinator, use-cases (Fiscalize/Drain/Probe/FleetHold/BootRecover), PORTS
├── prro-store-sqlite    migrations, repositories, audit/outbox, durable INBOX — applies a whole TransitionPlan in ONE tx
│
├── prro-ingress-contract  CanonicalIngressEnvelope (WRAPS domain::FiscalCommand) + ingress identity/idempotency + the ingress PORT + shared contract-suite
├── prro-ingress-native    (each concrete ingress protocol implements the contract)
├── prro-ingress-maria304
├── prro-ingress-checkbox
├── prro-ingress-xmlrpc
│
├── prro-dps-contract    the DPS PORT + wire DTO; USES domain::SubmissionEvidence (NO second copy) — the ONLY DPS crate prro-engine depends on
├── prro-dps-grpc        gRPC / sendChkV2 adapter (protocol 1)      — implements prro-dps-contract
├── prro-dps-protocol2   the SECOND DPS protocol adapter (ЕВПЗ)     — implements prro-dps-contract
│
├── prro-fleet-contract  fleet control-plane PORT: signed + epoch-versioned commands (policy/hold/release/config/provision) + telemetry read-model
├── prro-fleet-agent     edge-side agent — pulls/acks commands; OFF/advisory for the pilot   (the control-plane SERVER is a SEPARATE deployment, not this binary)
│
├── prro_crypto          existing signer (unchanged)
└── prro_escpos          existing printing (unchanged)
   (optional later: prro-fiscal-format — canonical XML/CP1251; not required up front)
```

**Ingress + DPS wiring (both ports symmetric).** `prro-engine` depends ONLY on the two
**contract** crates (`prro-ingress-contract`, `prro-dps-contract`), never on a concrete adapter.
`prro` (runtime) registers the concrete adapters; the `FnCoordinator` selects the DPS protocol
per the durable binding below. Every ingress adapter emits the full `CanonicalIngressEnvelope`;
every DPS adapter preserves the delivery outcome (`NotSubmitted / SubmittedUnknown /
ResponseObserved`). **Not every minor dialect needs its own crate** — a separate crate is
justified only by its own heavy deps / parser / listener lifecycle / security surface; protocol
*versions* are held as **versioned compatibility profiles**, not new crates. Contract-splits cost
≈ +0.5–1 wk (DPS) + ~1–2 wk (ingress-contract + durable-inbox API); concrete second adapters are
separate slices.

**Native-DLL / `unsafe` adapters get a SIDECAR PROCESS, not just a crate.** A crate boundary does
not protect the fiscal engine from memory corruption or a process crash in an adapter that links a
vendor DLL / raw hardware protocol — isolate those out-of-process.

**Single type ownership (no duplicate definitions).** Each shared type has exactly ONE owner:
`prro-domain` owns `FiscalCommand`, the fiscal value types, and the **protocol-neutral
`SubmissionEvidence`**; `prro-ingress-contract` owns `CanonicalIngressEnvelope` + ingress
identity/idempotency (it *wraps* a `domain::FiscalCommand`, never redefines it);
`prro-dps-contract` *uses* `domain::SubmissionEvidence` (no second copy). `prro-engine` depends on
all three contract crates (ingress, dps, fleet). **No ingress / DPS / fleet adapter has
mutation access to the store** — adapters produce/consume typed contract values only; all state
mutation goes through the coordinator → store adapter.

**Durable protocol binding (per FN / shift / document).** Persist and carry with each doc:
`protocol_id + version`, `capability profile`, `endpoint/config revision`, `envelope hash`,
`remote correlation id`, `delivery evidence`. The protocol is **bound at shift-open**;
**switching protocol with an open shift is forbidden** (extends frozen invariant #3 — no channel
switch with an open shift); a **document retry ALWAYS uses its original protocol**.

**Cross-protocol fallback is FORBIDDEN (hard invariant).** `SubmittedUnknown` on protocol A is
**never** permission to send the document on protocol B. Reconciliation runs **first, on the
original protocol**. A cross-protocol *status query* is admissible only where it is provably
documented that both protocols observe **one authoritative registration** (same fiscal ledger).

**Hard crate rules (compiler-enforced invariants — the point of the split):**
- `prro-domain` never depends on any outer system crate.
- `prro-engine` knows nothing of SQLite / gRPC / Axum / a concrete signer (only ports).
- adapters depend **inward**, never on each other.
- one FN stays one logical writer.
- an atomic mode/session/shift/doc change is **never cut by a crate boundary**.
- transport classification never loses delivery certainty.
- **printing is never inside the atomic fiscalization.**

## 5 · Extraction order (strangler — never big-bang)

1. Create `prro-domain`; move only types + pure rules (`FiscalCommand`, fiscal value types,
   protocol-neutral `SubmissionEvidence`; **NOT** `CanonicalIngressEnvelope` — that is owned by
   `prro-ingress-contract` in step 2); **re-export from `prro`** (behaviour-neutral).
2. **Split into four PRs (respect "one semantic change per PR"):**
   - **2a (behaviour-neutral):** the contract crates (`prro-ingress-contract`, `prro-dps-contract`,
     `prro-fleet-contract`) + facade / re-export — no behaviour change.
   - **2b (behaviour-neutral):** the **INACTIVE** durable-inbox + reservation schema + persistence
     tests (tables/migrations wired, not yet on the hot path).
   - **2c (SEMANTIC, its own PR):** the typed DPS outcome + **elimination of the blind resend**
     (the double-issue fix) — with new regression evidence.
   - **2d:** migrate the existing native / maria304 / checkbox / xmlrpc adapters onto the
     `CanonicalIngressEnvelope` **one at a time**, each behind the shared contract-suite (this is
     where the M×N→M+N collapse lands).
3. Create `prro-engine` + the per-FN coordinator.
4. Move `inline`, `drain`, `probe`, `fleet/admin`, `boot` **through the coordinator one at a time.**
5. Extract `prro-store-sqlite` **after** the commands stabilize.
6. Keep `prro` a thin composition root.
7. Then (optional) extract XML and split the giant modules.

After the contracts exist, **each new compatible ingress protocol and the second DPS protocol
(ЕВПЗ) are LOCAL adapter slices — not fiscal-core changes.** That is the whole point of the two
symmetric contract boundaries.

**Per-seam discipline (one PR = one of these, never mixed):** behaviour-neutral extraction →
equivalence tests → **one** semantic change → crash/property tests → next seam. The single
most dangerous strategy is rewriting FSM + persistence + boot simultaneously — that destroys
the ability to prove *where* behaviour changed. Giant modules to decompose during this:
`boot_phase.rs` **4469**, `backlog_drain.rs` **4096**, `stage_send.rs` **2643** lines (verified
on origin/main) — a correctness risk, not cosmetics.

### 5.1 · Test-moat preservation — the refactor's safety NET, not its casualty

The 9/10 test moat is what MAKES this refactor safe: the black-box tests catch any silent
fiscal-semantics change during a "pretty" relocation. Preserve it by rule:

1. **`prro` stays a compatibility facade** (`pub use prro_domain::*; pub use prro_engine::*;`)
   until the end of the strangler — so most integration tests keep compiling.
2. **Unit tests move WITH their module** — no rewriting assertions.
3. **Shared fixtures/builders → a `prro-testkit` crate — `publish = false`, dev-dependency only.**
4. **The top-crate `test-support` feature is forwarded into every new crate.**
5. **Adapter tests become the shared ingress / DPS contract suites** (§10).
6. **A behaviour-neutral PR changes only paths/imports** — any expected-behaviour change is a
   **separate** PR.
7. **Every PR leaves the WHOLE workspace green** — no multi-week "red branch".
8. **CI switches** from a single `cargo test -p prro` to a **workspace + feature-matrix** run
   (the matrix does **NOT** include the manual live-dps tests — those stay `#[ignore]` + env-gated).

**Do NOT adapt the old BLACK-BOX tests to the new architecture** — in a *behaviour-neutral* PR their
assertions stay **unchanged** (they catch a relocation that silently moved fiscal semantics). The
**one exception:** a **semantic bugfix** (e.g. forbidding the blind resend) may replace a test that
pinned the OLD buggy behaviour — but only in a **separate semantic PR carrying new regression
evidence**, never inside a relocation PR.

Expected impact: **~60–75%** of tests survive unchanged; **~15–25%** need mechanical import/fixture
edits; the painful **~5–10%** are white-box tests bound to private modules / concrete SQL repos /
source paths — compile-fail + static source scans, tests importing `stage_send`/`boot_phase`
internals, migration fixtures after the new schema, file-bound mutation targets, `test-support`
wiring. Budget **+1–2 person-weeks** for test relocation + CI wiring.

### 5.2 · Spec layer — authored BEFORE any semantic code

This ADR is the "why / what"; the **executable specs** are the "exactly how" and are authored (in
`docs/superpowers/specs/`) before the semantic PRs. Behaviour-neutral extraction (step 1, 2a, 2b)
may run in parallel — it needs no new spec. Each spec carries its **RED-pins**; the implementer
writes test-first against the spec (dual-session TDD). The set, in dependency order:

1. **Executable transition contract / state model** (§3.4) — the single data-driven table
   (states / edges / guards) + its schema; runtime + docs derive from it; the fuzzer keeps
   **independent** invariants (common-mode).
2. **Delivery-outcome + reservation state machine** (§3.2, §3.6) — `NotSubmitted | SubmittedUnknown
   | ResponseObserved(...)` + `ReservedNotStarted → CallStarted → OutcomeObserved`, the crash rules,
   the started-call → `SubmittedUnknown` rule, no-blind-resend. (Feeds the 2c double-issue fix.)
3. **Canonical ingress contract** (§3.7-3.8) — `CanonicalIngressEnvelope` + `FiscalCommand` enum +
   `IdempotencyStrategy` + durable-inbox key/rules + the shared contract-suite.
4. **DPS contract** (§4) — the port + `SubmissionEvidence` usage + durable protocol binding +
   cross-protocol invariant.
5. **Fleet command lifecycle** (§3.10) — `ReceivedDurable | Applied | Rejected | Deferred` +
   independent local/fleet HOLD + epoch/signature (advisory-only for the pilot).
6. **Coordinator command-lifecycle + admission = f(axes)** (§3.1, §3.3) — the orthogonal axes →
   effective admission function; the uniform command lifecycle; the anti-god-object contract
   (unit-testable with fake ports) + `TransitionPlan` fencing.

## 6 · B11 on the new foundation (verified against origin/main)

A re-verification on clean `origin/main` (the first external audit ran on a branch 33 commits
behind main) collapsed the alarming "14 blockers" to a thin, real core:

**Artifacts (false alarms — already on main, do NOT re-solve):** 24h auto-Z (`auto_z.rs`, T3
#256); offline code close-reserve (T2 #255); DocType 9/10 END handshake (B10 #252); "GOING_OFFLINE
dormant" is **false** — `GoingOffline/GoingOnline/CryptoDegraded` are live, fully-dispatched
NodeMode variants (only the auto-arm *entry* is missing); budget double-count (168h is
interval-recomputed on-read, no counter to double-count); "DpsError has no server/transport split"
(it does — `error.rs:15`).

**Real, thin work:**
- **Keystone (delivery-certainty):** the Transport-vs-Server/Auth/Decode discriminant is
  **already computed** (`RoutingDecision.retry_class + node_mode_flip + probe_hint`,
  `error_routing.rs:58-66`) but **discarded** at `classify_send_outcome` (`inline_map.rs:430-435`,
  collapses to `target_state`). Threading it into the typed delivery outcome is **necessary but
  NOT sufficient** — the discriminant only preserves the classification; the *double-issue* fix
  additionally requires that a **started-call timeout/`Unavailable` be classified conservatively
  as `SubmittedUnknown`** (not `NotSubmitted`) and routed to **reconciliation on its original
  protocol, never a blind resend** (§3.2, §4 cross-protocol invariant). The current `er_redrive`
  blind-resend on `Transport` is the live hazard this replaces (still to nail — below).
- **Transition-arbiter:** the drain finalize CAS `GoingOnline→Online` (`backlog_drain.rs:3182-3195`)
  is guarded only on mode+session, consulting **no fleet-hold / probe** → a fleet HOLD leaks into
  auto-return. The coordinator must guard this CAS. The node-mode-flip apply seam already exists
  (`stage_send.rs:1845`, today used only by -11→Blocked) so arming rides it.
- **Fleet-hold primitive** — net-new durable columns (greenfield, expected).

Design premises now corrected: **B11's "3 thin durable additions, not a new subsystem" is
correct on origin/main** once it rides the coordinator + typed delivery.

**⚠️ To nail independently (open):** the double-issue hazard (does the current `er_redrive`
blind-resend a possibly-submitted doc on a transport-timeout? — a live fiscal bug if so). This
is the first thing the typed delivery outcome must prove-fix.

## 7 · Operational layer — must solve BEFORE the first physical register

The DPS test cabinet proves the *protocol*, not the whole till system. Before a live register:
1. **Two levels of delivery uncertainty** — not only gateway→DPS but **POS→gateway** (a POS
   that lost the reply will retry): durable `request_id`, status lookup, idempotent replay.
2. **Time model** — separate the **UTC fiscal-event time** from **monotonic** time for
   dwell/timeouts; handle NTP jumps, DST, reboot, month rollover, cert expiry, clock rollback.
3. **Backup/restore & machine replacement** — you cannot just restore an old SQLite DB and keep
   selling (rolls back seed / offline-codes / an already-accepted doc); restore/clone must route
   through reconciliation/RMR.
4. **Physical-failure behaviour** — power-off at **every** durable point, full disk, WAL
   corruption, key loss, no DPS on boot, stuck printer spooler, forced Windows-service stop.
5. **Printing separate from fiscalization** — a doc may be DPS-accepted but not printed; reprint
   must not re-fiscalize → distinct `FiscalAccepted` vs `PrintPending/Printed` states.
6. **Fleet must not own register safety** — the local per-FN coordinator is the final arbiter;
   fleet may give policy/hold but cannot force illegal offline entry, a cap breach, or block a
   mandatory return. **For the first pilot, fleet OFF or advisory-only.**
7. **Operator recovery** — no "fix SQLite by hand": forensic snapshot, alert, diagnostic command,
   bounded repair ops, actor/reason in audit, and a clear "when to stop selling" rule.
8. **Keys & updates** — test/prod split, cert rotation, **signed** fleet commands + update
   packages, local DB/secret protection.
9. **First version already creates an installed base** — `schema_version`, multi-version upgrade
   rules, no unsafe downgrade; test on a real Windows host + real printer + test cert + network
   shaper + power-cut rig.
10. **Monitoring & alerts** — health/readiness/metrics surfaced (per-FN mode, connectivity,
    backlog depth, budget remaining, recovery-health); alert classes: transport-unreachable vs
    DPS-degraded, RMR landing, cap-approach, cert-window, HOLD-with-budget-accruing.
11. **Worst-case backlog / drain capacity** — bound and test the maximum offline backlog and the
    drain throughput (a multi-day outage's backlog must drain within the legal window without
    exhausting codes / time budget on return).
12. **Update & rollback rehearsal** — a real upgrade (multi-version-skip) and a real rollback
    rehearsed end-to-end (rollback routes through reconciliation, never a raw DB swap).
13. **Reporting read-models (observability) — derived, rebuildable, OFF the write-path.** The
    signed payload/XML stays the **canonical** fiscal truth; reporting projections (notably a
    **`document_lines`** normalized line-item table — goods/qty/price/tax, absent today: lines live
    inside `payload_json` + the canonical `<P>` elements) are **derived read-models** fed off the
    **event stream (outbox)**, never written on the hot fiscal path, and **rebuildable** by
    replaying payloads. **Reconciliation invariant:** a projection must always reconcile to the
    signed payload (`sum(lines) == doc total`; rebuild-equivalence) — an `invariant_scan`/fuzzer
    tooth pins that it can never silently diverge from fiscal truth. Powers "browse shifts →
    receipts → **lines**" and the fleet console. **Post-consolidation** (clean domain + outbox
    seams from CS-3/CS-7); **not pilot-critical** — the pilot views lines by rendering the payload.
    **Reference-validated (2026-07-14):** BOTH WebCheck (`CHECKBODY` table + `checkxml` blob) and the
    official ДПС app (`Operation`+`Goods`+`ReceiptRate` tables + `XMLBlob`) store lines **normalized
    & queryable** and keep the XML/blob for **transmission/reprint only** (reports derive from the
    tables, not the blob). We take the **WebCheck-style DENORMALIZED `document_lines`** (line-as-
    received: `code, uktzed, goods_name, unit, qty, unit_price, line_sum, tax_letter, excise,
    discount`, **`barcode` OPTIONAL/nullable** — the POS may not send it) — **NOT** a ДПС-style
    `Goods` master (the POS owns the catalog; a gateway does not).
    WebCheck uses its line table for the **Z-report tax breakdown** (`GROUP BY tax_letter`), so this
    is early-useful, not just post-pilot analytics → land it as an **early read-model slice after
    CS-3** (once outbox/domain seams exist), unless our `aggregate_z` already carries doc-level tax.

## 8 · Effort & sizing (no installed base — the discount is real)

| Slice | Effort |
|---|---|
| Architecture frame (domain + ports skeleton) | 1–2 wk |
| Typed delivery outcome + double-issue fix | 1–2 wk |
| per-FN coordinator + migrate inline/drain/probe/boot | 2–4 wk |
| B11 + fleet-policy + cap-engine wiring + crash-tests | 2–3 wk |
| Final fuzzer / mutation / live-DPS pass | 1–2 wk |
| **Crate split** (adds ~15-25%) | +1.5–3 wk |
| **DPS contract-split** (3-way: contract/grpc/protocol2) | +0.5–1 wk |
| **Ingress-contract + durable inbox API** (M×N→M+N) | +1–2 wk |
| **Fleet-contract + edge fleet-agent skeleton** (OFF/advisory for pilot; control-plane server separate/post-pilot) | +0.5–1 wk |
| **Test relocation + CI (facade re-exports, testkit, workspace + feature-matrix)** | +1–2 wk |
| **Each new ingress adapter / second DPS adapter (ЕВПЗ)** | separate local slice (own estimate) |

**Normalized total: the architecture program is ~10–17 person-weeks** (core coordination + typed
delivery + the three contract boundaries + durable inbox + reservation/fencing; **+ ~1–2 wk test
relocation + CI wiring**, §5.1), **excluding** specific new adapters and the operational layer (§7). For two engineers with a clean zone-split,
plan **~8–12 calendar weeks** (some hardware/ops in parallel). After the two contract boundaries
exist, every additional compatible protocol is a **local adapter slice**, not a fiscal-core change
— so the M+N structure pays back on the 2nd protocol onward. ~30% of the effort is code; ~70% is
crash-window modeling, seed-chain preservation, property/fuzzer/mutation tests, and real-rig
verification.

**Honest sizing (no single conflated range):** architecture program **~10–17 pw**; test
relocation + CI **~1–2 pw**; the **operational layer (§7) and each concrete adapter are estimated
SEPARATELY**. A single "path to physical register" number is **deliberately NOT fixed** until §7 is
decomposed into its own slices.

## 9 · Decisions needing operator ratification

1. **§16.7 — RATIFIED: V1 (2026-07-14).** Whole-shift RMR stays the **fail-safe DEFAULT**; the
   per-doc quarantine (shift-sells-with-a-quarantined-doc + its chain/seed semantics) is **deferred
   to a separate later ADR.** Preserves the current fiscal semantics; unblocks the lock.
2. **Fleet posture — RATIFIED: ADVISORY-ONLY for the pilot (2026-07-14).** The fleet-agent reads
   telemetry and may alert/advise but issues **NO commands** (no HOLD / no policy); the local
   coordinator remains the sole arbiter. Full command mode is a later enablement — **config, not
   re-cut.**
3. Ratified (not blockers): offline-is-failure-forced axiom; unmanned-byzantine stall (decision #3);
   fleet N=1 = one node_state row.

## 10 · How to audit THIS plan (acceptance gates)

- **Behaviour-neutrality:** each extraction PR must pass an equivalence test suite before any
  semantic change lands in the same area.
- **Test-moat gate (§5.1):** every PR leaves the **whole workspace green** (no multi-week red
  branch); the old **black-box tests stay unchanged in a behaviour-neutral PR** (a semantic bugfix
  may replace a bug-pinning test only in a *separate* PR with new regression evidence); a
  behaviour-neutral PR changes only paths/imports; CI runs the full workspace + feature matrix (not
  just `-p prro`, excluding manual live-dps).
- **Invariant preservation per phase:** no network/crypto in a write tx; one FN = one writer;
  atomic mode/session/shift/doc change never split across a crate boundary; delivery certainty
  never dropped; printing outside atomic fiscalization.
- **Executable contract (common-mode-safe):** the transition table drives the runtime AND the
  documentation; a runtime/table divergence is a gate failure. But the fuzzer's oracle is **NOT**
  generated from the same table — it carries **independent frozen invariants**, and the
  adversarial double-issue / cap / seed / fleet scenarios are **hand-authored**, so a wrong table
  cannot produce an equally-wrong runtime-and-test.
- **Delivery / two-protocol gates:** a `SubmittedUnknown` document is never blind-resent
  (reconciliation on its original protocol first); **cross-protocol fallback forbidden** (A's
  `SubmittedUnknown` ≠ send on B); protocol **bound at shift-open**, no switch with an open shift,
  retry uses the original protocol; a started-call timeout is classified `SubmittedUnknown`.
- **Coordinator gates:** `TransitionPlan` carries expected-versions + fencing token + CAS-
  preconditions and returns `Applied | Conflict` (recompute on conflict); V1 = one state-mutating
  command per FN with the network stage on a durable reservation.
- **Coordinator-thinness gate (anti-god-object, §3.1):** the coordinator holds **no fiscal decision
  logic** (it calls the pure domain oracle) and **no durable state of its own**; it is
  **unit-testable with fake ports**; one uniform command lifecycle (no per-command inlined rules);
  it stays within its size/complexity budget. A coordinator that accrues fiscal rules or
  SQLite/gRPC knowledge is a **gate failure** — that is the relocated monolith.
- **Shared ingress contract-suite (EVERY ingress adapter must pass the SAME tests):** full
  canonical payload; taxes / rounding / payments / returns; duplicate request; same id + different
  payload (→ `IdempotencyConflict`); concurrent duplicate; lost reply + status replay; malformed /
  oversized input; UTF-8 / CP1251 + ambiguous fields; auth + FN authorization; backpressure; no
  write-channel switch with an open shift; correct `SubmittedUnknown` → legacy-system response
  mapping. A missing required field never silently defaulted (versioned mapping-snapshot or
  fail-closed). Durable inbox is written **before** the ingress ACK (central idempotency, §3.8).
- **Fleet safety gates (§3.10):** a fleet command NEVER overrides local law (no forced illegal
  offline entry / cap breach / blocked mandatory return); commands are signature-verified +
  epoch-guarded (forged / duplicate / reordered rejected); per-register ack is observable; the
  model degenerates to N=1 (pilot fleet-agent OFF/advisory) with no architecture change.
- **Still-open pilot-gate items** (independent of this refactor, tracked in
  `PILOT_GATE_CHECKLIST.md`): channel guards (INV-05/06), `PRRO_FISCAL_MODE` harness (DF-5),
  cert-expiry gate, Byzantine decode, CodePool→STOP, active-shift unique index; crash-recovery
  and two-FN isolation not yet separately confirmed.
- **Verdict:** current gate **NO-GO for a live register** (high confidence); but the fix is
  cheapest **now** — strong protocol + test foundation, and no backward-compatibility yet.

## 11 · Docs hygiene (part of the consolidation)

Reduce documentation to a **canonical ADR set + the executable state model.** Today there is too
much "authoritative / supersedes / locked", and some sources are already stale (the pilot
checklist itself flags other gate-docs as stale; the MATRIX/PLAYBOOK bodies still describe
closed blockers as open). One canonical transition contract replaces the prose state machines.

---

### Appendix · Reference map (external-audit-pinned)

> **Baseline commit.** All code line-references in this plan are against **`origin/main` =
> commit `8ec99ca`** (2026-07-14). Re-verify against that SHA, not a moving branch. (The first
> external audit ran on `88aab7b`, 33 commits behind `8ec99ca`; several of its findings were
> stale-branch artifacts — see §6.)

Auditor-accessible artifacts (all in-repo at `8ec99ca`):
- **Fiscal core & invariants:** `CLAUDE.md`, `docs/LEGAL_INVARIANTS.md`.
- **B11 design + verified findings:** `docs/B11_OFFLINE_TRANSITION_DOSSIER.md`.
- **Gate status:** `docs/PILOT_GATE_CHECKLIST.md`.
- **Caps engine (verified present, §6 artifact-refutation):** `rust/prro/src/services/time_budget.rs`,
  `rust/prro/src/services/write_path/auto_z.rs`; tests `rust/prro/tests/t3_time_budgets.rs`,
  `t3_auto_z_ticker.rs`.
- **Keystone seam (§6):** `rust/prro/src/services/write_path/inline_map.rs:430-435`,
  `rust/prro/src/services/write_path/error_routing.rs:58-66`,
  `rust/prro/src/transports/dps/error.rs`, `transports/dps/dto.rs`.
- **Fleet-HOLD leak seam (§6):** `rust/prro/src/services/offline_sync/backlog_drain.rs:3182-3195`.
- **Live-DPS proof / scenario / fuzzer:** `rust/prro/tests/live_dps_extended_smoke.rs`,
  `tests/shift_life_matrix.rs`, `tests/invariant_fuzzer.rs`, `tests/webcheck_replay.rs`.
- **Spec (§16.3/§16.7 conflict, §9):** `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`.

(Internal working notes — the re-verification workflow run and session memory — are not part of the
auditable set; the file/line/test references above stand on their own against `8ec99ca`.)
