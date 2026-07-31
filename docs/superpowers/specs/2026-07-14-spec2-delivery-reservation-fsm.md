# Spec #2 — Delivery Certainty + Reservation FSM (rev 2)

**Status: 🔒 DESIGN-LOCKED (rev 2). 2026-07-14.** External audit LOCK-READY (S2-V1…V5, S2-I1 all
CLOSED).
Rev 2 integrates the external-audit NOT-YET findings (S2-V1…V5, S2-I1). Scope is **gateway→DPS
ONLY**; the POS→gateway double-issue is closed by **Spec #3** (durable ingress inbox), cross-referenced
in §7. Grounded on `origin/main 8ec99ca`.

---

## 1 · Problem (verified on main)
`DpsError::Transport` conflates not-submitted, a **started-call timeout** (DPS may hold the doc, ACK
lost), and a **fully-parsed `-4`** (`dto.rs:170`). All tonic statuses collapse to `Transport`
(`grpc.rs:114`). `er_redrive` may blind-resend on `Transport`; DPS does **not** dedup → **double
issue**. One `DeliveryOutcome` enum is **too coarse** — certainty, peer-evidence and routing are
**independent** and must not be collapsed (S2-V1).

## 2 · Three orthogonal fields (replaces the single enum — S2-V1, S2-I1)

Every send outcome carries **three independent fields**:

**(a) `SubmissionCertainty`** — *did it reach DPS?*
`NotSubmitted` (provably not) · `SubmittedUnknown` (irreducibly ambiguous) · `Submitted` (proven at DPS).

**(b) `ResponseProvenance`** — *what did we observe from the far side?*
`NoResponse` · `AuthenticatedPeer` (TLS peer answered, not necessarily DPS) · `ParsedDpsEnvelope`
(a real DPS fiscal envelope). **Only `ParsedDpsEnvelope` is DPS forward-progress.** A WAF / reverse
proxy / captive portal can produce `AuthenticatedPeer` or garbage while DPS is unreachable (S2-I1).

**(c) `RoutingPolicy`** — the existing **8 `RetryClass`** (unchanged authority, all 8 must keep
migration/recovery semantics): `TerminalReject | TransientRetry | FnConfigError | WrapperBug |
ProbeRequired | MacRecovery | OperatorEscalation | DrainChainSettleRetry(legacy-decode-only)`.

These are orthogonal: e.g. **`-4`** = `{SubmittedUnknown, ParsedDpsEnvelope, TransientRetry}` — a
real DPS response was observed (liveness authoritative) yet the submit result is unknown → **neither
resend nor arm-offline** (S2-V2).

## 3 · The reservation FSM (durable, crash-safe)
```
ReservedNotStarted → CallStarted → OutcomeObserved  (→ applied atomically to the ledger, §4)
```
Reservation is durable, carries `attempt_id, protocol_binding, envelope_hash, generation,
fiscal_number`. **`CallStarted` is committed DURABLE *before* `send_chk`** (`stage_send.rs:1539`).

**Transport evidence = `(phase ∈ {Preflight, CallStarted}) × (ResponseProvenance)`.**
- `NotSubmitted` is admissible **only** at `Preflight` (local, before the marker) **or** a
  protocol-proven pre-handler refusal — **never** for a failure observed after `CallStarted`
  (connect-refused / timeout / gRPC-auth all return *after* the marker) (S2-V2, fixes old RP-6).
- Anything at `CallStarted` with no proven acceptance → `SubmittedUnknown`.

**Crash rules (three windows — S2-V3):**
| Reboot observes | Resolve to |
|---|---|
| `ReservedNotStarted` | safe cancel → `NotSubmitted` |
| `CallStarted`, no outcome record | **`SubmittedUnknown`** (reconcile; never `NotSubmitted`, never blind-retry) |
| `OutcomeRecordedPendingApply` | **boot-idempotent apply** of the recorded plan (see §4) |
| applied | already resolved |

**⟶ 3.2 realized-status (CS-3 recon; write-back).** The FSM is realized as an **INACTIVE schema shell**, not a
live driver: migrations 032/033 create the exact 3 states as a strict `CHECK` + the `delivery_reservation` repo,
but the repo has **zero production caller** (`delivery_reservation.rs` — tests only), and 033's `apply_state`
(`NULL → PENDING_APPLY → APPLIED`) scaffolding is INACTIVE. `CrashedBeforeObservation` is **un-mintable by any
live path**; today's live boot resolver runs off the `fiscal_documents` FSM → `ErrorRetryable`/`RMR`, **not**
`SubmittedUnknown`. Activating the FSM (the durable `CallStarted` marker, the crash-window resolution above, and
the atomic apply of §4) is **CS-3 Bridge/D/E** — this section is the design-locked target, not a description of 3.2.

## 4 · Atomic OutcomeObserved → ledger apply (S2-V3)
**Verified real order:** the wire call runs first, then a **separate 4b tx** does
`Sending→Sent + server_fiscal_no + seed + trace + audit` (`stage_send.rs:1539/1685/1865`). The
ambiguous timeout happens **before** that CAS, so `server_fiscal_no` is **not yet known** (old §6
was wrong). Fix: the `OutcomeObserved` evidence **and** its ledger effect (`TransitionPlan`) commit
in **ONE transaction**; where that is impossible, record `OutcomeRecordedPendingApply` durably and
apply **boot-idempotently** — **"resolved" is forbidden before apply.**
**⟶ 3.2 realized-status:** the `OutcomeRecordedPendingApply` two-commit apply is 033 scaffolding **INACTIVE** (no
production caller); CS-3 Slice D wires the two commit boundaries (record `ObservedOutcomeV1` first, repeatable
apply CAS second — keystone §2 slice D).

## 5 · SubmittedUnknown → chain-generation FENCE + honest reconciliation (S2-V4)
A `SubmittedUnknown` sets a **durable per-FN chain-generation fence** — NOT just "no second call
during this call." While fenced, the FN may do **only** read-only reconciliation / `STOP` / `HOLD` /
operator resolution; **no new issuance, no new offline-session, no seed advance.** *Why:* the seed
advances only after a **known** `Accepted` (`stage_send.rs:1725`); starting doc B on the old seed
while doc A was actually accepted **forks the chain** and B can evict A from `lastChk`.

**⟶ CORRECTED by `CS3_REMEDIATION_DESIGN.md` (rev3) — the fence is NOT a permanent SQL fence.** An earlier
draft (rev2) held `SubmittedUnknown` / routed-`Submitted` in the SQL fence **forever**; a model-decorrelated
re-audit proved this **unsound**: a first-attempt transport blip → `SubmittedUnknown` → an un-releasable SQL
fence → the whole FN **bricks** with no operator exit (the spec family had zero SubmittedUnknown fence-release
op). Rev3 fixes it by **reusing existing machinery, no new table/state/token**:

- **Active-fence predicate (NORMATIVE, verbatim — design §3.1)** — reduced to the record-then-apply window only:
  ```sql
  state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
  OR ( state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY' )
  ```
  No routing-class or certainty disjunct. Byte-identical across `ux_reservation_active`,
  `delivery_reservation_no_replace`, `get_active_for_fn`, and the D/E authorization query.
- **Unresolved outcomes** (`SubmittedUnknown` / `-12 MacRecovery` / `-6 OperatorEscalation`) stay
  `PENDING_APPLY` **and** flip `node_state.mode = STOP_MODE` in the SAME record tx (design §3.2). `STOP_MODE`
  already refuses ingress (`stage_acquire.rs:301`) → no new issuance → no fork; the PENDING fence stays
  authoritative even if a future mode gate is forgotten.
- **Release = the strengthened existing `reset_stop_mode`** (`admin.rs:300`, today CASes `STOP_MODE →
  GOING_ONLINE`): the operator supplies the resolution (accepted-with-observed-`F` / not-accepted / MAC-seed);
  it completes the PENDING reservation to `APPLIED` and CASes `STOP_MODE → GOING_ONLINE` in one
  `BEGIN IMMEDIATE`. A plain STOP reset **fails closed** while a CS-3 PENDING row exists. No new release
  token / FSM state (design §3.4). This is the SubmittedUnknown fence-release the rev2 fence lacked.
  **(rev3.1)** the release is gated on a **verified read-only `status_rro`** (probe OUTSIDE the tx,
  `online=true`, `snapshot.open_shift` agrees) — not bare operator trust; it then selects `ONLINE` (no
  offline session) or `GOING_ONLINE` (an active OPEN/DRAINING session must drain), and **clears the active
  pointer atomically with `PENDING → APPLIED`**. An **offline-origin reject / `Offline168` HOLDS the fence**
  (stays PENDING+BLOCKED) until its local chain is repaired (origin-sensitive — design §C5/§3.4).
- **Definitive seed-unchanged rejects** (`TerminalReject` / `FnConfigError`, and `-11` with an atomic node
  `BLOCKED` + a guarded `BLOCKED → GOING_ONLINE` operator branch) **RELEASE** at `APPLIED` — no permanent
  brick (design §3.2 rows 4/5, §3.4).
- **P2 lifetime call-once (NORMATIVE — design §2):** a UNIQUE partial index
  `ux_delivery_document_ever_started ON delivery_reservation(document_id) WHERE call_started_at IS NOT NULL`
  + a `NOT EXISTS(… call_started_at IS NOT NULL)` clause in `authorize_submission` + the same historical
  clause in `delivery_reservation_no_replace` — **at most one wire per `document_id` over its whole life**; a
  started-then-ambiguous attempt is **never re-wired** (a connect-refused/timeout after the durable marker
  consumes the document's one lifetime call → reconcile/operator, never resend).
- **Whole-fence cutover (E):** every `stage_send::run` caller + the `(ErrorRetryable, Sending)` edge + the
  4 seed-writers (`offline_code_replenish` / `boot_phase` / `stage_offline_ack` / the online seed-UPDATE) +
  offline issuance/session/code prove no conflicting active reservation via §3.1 (design §3.3). `STOP_MODE`
  is the durable operator-facing halt, **not** a substitute for the reservation check.

> The `seed_advanced` column proposed in an earlier draft is **dropped** (proven a dead disjunct: a routed
> reject never reaches `WireDecision::Sent`, so it is never issued). **Full normative detail — the exact
> predicates, the durable evidence-union storage, the operator resolution matrix, migration 035, and the
> RED-pins — lives in `CS3_REMEDIATION_DESIGN.md` (rev3) §2–§7, the design-of-record.**

**Reconciliation is per-protocol, capability-gated.** `envelope_hash` is **not** a query key — the
current DPS offers **no query-by-local-identity**, only `lastChk` + comparison of an **already-known
server id** (`channel.rs:62`), which a `SubmittedUnknown` lacks. So each DPS protocol declares a
**`ReconciliationCapability`**; **without a proven capability the default is immediate
`RequiresManualReconciliation` behind the fence** (a bounded read-only probe is allowed only on the
original protocol, only under a declared capability, and a deadline does **not** lift the fence).

## 6 · Anti-mask (corrected)
- `ParsedDpsEnvelope` **with a rejected verdict** (incl. `-1`/`-13`/`-14`) = **proof-of-life** →
  does not arm offline, does not become `SubmittedUnknown`.
- `AuthenticatedPeer`-only or garbage body = **DEGRADED**, **peer-evidence only** — it does **NOT**
  reset the anti-mask counter and is **NOT** clean forward-progress (S2-I1). Alert + stay-online
  (B11 posture), never further chain issuance.

## 7 · Scope + Spec #3 dependency (S2-V5)
**This spec closes ONLY gateway→DPS.** POS→gateway is closed by the durable **ingress inbox**
(`ingress_inbox.rs:1/121` already has the `Created/Replay/Conflict` triple + atomic payload-hash
compare) — **Spec #3**. Integration RED-pins live there: inbox durable before ACK; a replay never
re-invokes the engine; same-key/different-hash → `Conflict`; `NoSafeReplayIdentity` forbids write.
**Terminology:** ingress uses `NoSafeReplayIdentity`; DPS uses `NoSafeReconciliationIdentity` (§5).

## 8 · Classification cut point (S2-V1 fix)
Keep `DpsError + RetryClass + evidence` intact **up to** the collapse at
`inline_map.rs:394` (`StageSendOutcome`/`SendDisposition`); the collapse to `target_state` must
happen **after** the three fields are recorded, not before.

**⟶ 3.2 realized-status (CS-3 3.2 PR4; write-back).** `map_send_reply` derives `RawSendReply → SendResponse` and
binds it as `_shadow_response` (`stage_send.rs:1573`, `shadow_map.rs:15`), but it is a **READ-ONLY shadow — the
three fields are computed and NOT recorded before the `target_state` collapse**; the live decision is still the
legacy `route_send_result` collapse (`stage_send.rs:1587`). So this cut-point is a **design-locked target, not
already-realized** — **D/E wires the shadow load-bearing** (records the three fields, drives routing off them).

## 9 · RED-pins (rev 2)
- **RP-1 (started-call ⇒ SubmittedUnknown):** a `Transport`/timeout after `CallStarted` yields
  `SubmissionCertainty=SubmittedUnknown`; revert → `NotSubmitted` → blind resend possible → **FAIL**.
- **RP-2 (fence):** while a `SubmittedUnknown` fence is set, any new issuance / offline-session /
  seed-advance on that FN is refused; only read-only recon / STOP / HOLD / operator pass.
- **RP-3 (atomic apply):** crash between the wire result and the ledger apply → boot re-applies the
  recorded plan idempotently; no doc is "resolved" without its ledger effect; no double effect.
- **RP-4 (-4 orthogonality):** `-4` records `{SubmittedUnknown, ParsedDpsEnvelope}` → neither resent
  nor offline-armed.
- **RP-5 (WAF/garbage ≠ life):** an `AuthenticatedPeer`/garbage response does not reset the anti-mask
  counter and does not permit chain issuance.
- **RP-6 (local ≠ DPS response):** `Internal`/`QueryNotSupported` (local `WrapperBug`) and
  `NotFound`/`ServerFiscalIdMismatch` (read-only lookup) are **not** `ParsedDpsEnvelope` submit
  verdicts.
- **RP-7 (cross-protocol):** resolving a protocol-A `SubmittedUnknown` via protocol B → FAIL.
- **RP-8 (durability cost pin):** a benchmark on real Windows storage + a power-cut pin proving
  `CallStarted`-durable-before-call holds across power loss (WAL + `synchronous=FULL`, `db/mod.rs:100`).

## 10 · Open questions (answered per audit)
1. No provable correlation → immediate RMR/fence by default; bounded read-only probe only on the
   original protocol under a declared `ReconciliationCapability`; the deadline never lifts the fence.
2. `Malformed`/`AuthenticatedPeer` → alert + DEGRADED; keep "no auto-offline", but never clean-life
   nor further issuance.
3. `CallStarted`-durable-per-call is mandatory for V1 (no batching until proven a call never starts
   before the durable group commit); add the RP-8 Windows/power-cut benchmark.
