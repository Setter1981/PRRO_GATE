# Excise-stamp check — scope dossier (OPTIONAL feature)

**Status:** DRAFT scope for backlog. NOT greenlit for implementation.
**Intent:** an OPTIONAL, off-by-default, per-FN feature with TWO halves against a **self-owned**
service:
1. **Pre-sale check (read)** — "is this excise stamp (акцизна марка) permitted?" for excise line
   items; refuses the sale (audit-only, pre-mint) on a negative answer.
2. **Stamp write-back lifecycle (report)** — each rung-up (пробита) stamp is reported **RESERVED**
   (зарезервована) while the sale is unconfirmed (the offline window), and **WRITTEN-OFF** (списана)
   once the receipt is accepted by DPS. This is a **projection of the fiscal-document offline-drain
   lifecycle** — driven by the SAME state transitions, NOT a new state machine.
**Grounding:** all file:line on the main tree `rust/prro/src/` at scope time; re-verify at
implementation. The Python tree is dead — Rust only.

---

## §0 The load-bearing constraint: OPTIONAL ⇒ off-by-default ⇒ byte-identical when off

The feature is a **pure additive gate**. When `excise.enabled = false` (the default, and the state of
every existing contour/pilot), the write-path executes EXACTLY as today: no probe, no guard, no
latency, no new failure surface, no new state. This is the single most important property and the
first RED-pin (§6 P1). It means existing behaviour needs no re-verification and the feature can ship
dark, enabled per-FN.

Toggle granularity is **per-FN** (the core is already per-FN via `runtime/bindings.rs:52`
`OperatorBindings`), not global: some cassa sell excise goods, some do not.

---

## §1 What the check is — and the ONE open decision (a/b)

The service is **self-owned** (we design its API). What a "permitted" answer MEANS legally is the
only unresolved fork, and it sets the offline/trust policy (NOT the code volume):

- **(a) Proxy/cache over the STATE e-excise registry** (ДПС/Мінцифра). The service is ours but the
  *authority is the state*: correctness surface = **freshness of the sync**; a "permitted" answer is
  only as good as the cache age. Offline check against a stale local snapshot carries staleness risk.
- **(b) Our own business allowlist** (stamps we legitimately received/booked). The service is **fully
  authoritative**; a local cached snapshot is trivially sound; no staleness question.

**Recommendation:** design the contract to support (b)-style local snapshot semantics even if backed
by (a), i.e. the service owns "freshness" and exposes an explicit `snapshot_version` + `as_of` so the
gateway records WHICH snapshot authorized an offline sale (deferred-verify reconciliation, §4).

---

## §2 Grounded integration seams (file:line)

| Seam | Anchor | Note |
|---|---|---|
| Line items already carry stamps | `runtime/ingress/dto.rs:276` — `FiscalLine.excise_stamps: Vec<String>` | **No ingress/adapter/schema work** — stamps already canonical |
| Pre-sign guard + audit-only refusal | `services/write_path/stage_acquire.rs:1100` `reject()` | marks inbox REJECTED + `audit_log::append_tx`; **never mints a fiscal row / consumes an lnd** |
| Existing fail-closed example | `stage_acquire.rs:250` (hash mismatch → `reject(InvalidPayload, Critical)`) | template for a new refusal |
| Refusal reason enum | `RejectionReason` (used at `stage_acquire.rs:250`) | add `ExciseValidationFailed { detail }` |
| Audit-only write | `db/repositories/audit_log.rs:38` `append_tx` | commits atomically inside the same `with_immediate` |
| Network client abstraction | `transports/dps/channel.rs:21` `DpsChannel` trait | **only DPS is abstracted — NO generic external-client trait** |
| Closer template | `crypto.provider = passthrough \| sidecar` (local HTTP sidecar) | model the excise client on the **crypto sidecar**, not the gRPC `DpsChannel` |
| Config struct pattern | `config/mod.rs:483` `DpsCfg`; `:277` `SupervisorCfg`; `:541` `require_dps_endpoint()` | add sibling `ExciseCfg` + `require_excise_endpoint()` |
| Wiring root | `runtime/supervisor.rs:100` (`Arc<dyn DpsChannel>`); `runtime/bindings.rs:52` per-FN | add `Arc<dyn ExciseValidator>` sister field |
| Node/offline policy | `db/models/enums.rs:81` `NodeMode`; `stage_acquire.rs:292` mode guard | **DPS-transport-specific only — no hook for a foreign dependency**; excise unavailability must have its OWN semantics, NOT a node-state flip |

---

## §3 Design (minimal-diff, on existing seams)

1. **`ExciseValidator` trait** (sibling to `DpsChannel`, modelled on the crypto sidecar HTTP client):
   `async fn validate(&self, stamps: &[String], ctx: ExciseCtx) -> Result<ExciseVerdict, ExciseError>`.
   `ExciseVerdict = { permitted: Vec<Stamp>, refused: Vec<(Stamp, Reason)>, snapshot_version, as_of }`.
   `ExciseError::{ Unavailable, Timeout, Malformed }`. **Batch** (one call per receipt) — kills the
   "N calls per line" latency concern.
2. **`ExciseCfg`** in `SupervisorCfg`: `{ enabled: bool = false, provider: none|sidecar, endpoint,
   request_timeout_ms, offline_policy }` + `require_excise_endpoint()` fail-closed when
   `enabled && endpoint.is_none()`. Per-FN override via bindings.
3. **Probe placement (INVARIANT #1 — the correction).** The network call is a **pre-acquire async
   probe OUTSIDE any write-tx** (like stage-4a wire send, `stage_send.rs:1568`), NOT inside
   `stage_acquire`'s `with_immediate` envelope (lines 204–1099). The probe RESULT
   (`permitted / refused / unavailable`) is then passed INTO `stage_acquire`, which uses the existing
   `reject()` (audit-only) on a negative verdict. **A network call inside the guard tx would violate
   frozen invariant #1.**
4. **Refusal** = pre-acquire class: `RejectionReason::ExciseValidationFailed` → `reject()` → `audit_log`
   only, no `fiscal_documents` row, no lnd consumed. Clean fit to the existing refusal taxonomy.

---

## §4 Offline policy (the (a)/(b) fork made concrete)

`NodeMode` offline = DPS transport down; it says nothing about the excise service. Three options for
"excise service unavailable OR cassa offline", **recommend (B)**:

- **(A) node-state flip** (`ExciseServiceDegraded` mode) — REJECTED: couples FN lifecycle to a foreign
  dependency; heavy; wrong blast radius.
- **(B) per-request fail-closed online + local-snapshot check offline** — RECOMMENDED. Online: if the
  service refuses → audit-only refusal; if the service is *unavailable* → policy-gated (`offline_policy
  = fail_closed | fail_open_deferred`). Offline: check against the last-synced local snapshot (turns
  "skip verification" into "verify against cached set" — the legally stronger posture), record
  `snapshot_version` on the sale for **deferred-verify reconciliation** when back online (mirrors the
  DPS offline drain).
- **(C) unconditional skip when offline** — weakest; only if legal/business explicitly allows unverified
  offline excise sales.

**Legal input required (not an engineering call):** may an excise good be sold offline / when the
service is unreachable? That choice selects `offline_policy` + whether (b)-snapshot is mandatory.

---

## §4b Stamp write-back lifecycle — RESERVED → WRITTEN-OFF (projection of the doc drain)

The second half is a **report-back**, and its whole value is that it is NOT new machinery: the stamp's
state is a projection of the fiscal document's, driven by the events the gateway already emits.

**Stamp outbox state machine** (mirrors the doc, per `docs/superpowers/specs/…m3b-shift-state-expansion`
+ [[project_b10_offline_drain_handshake]]):

```
ring-up (sale minted)            →  RESERVED        (зарезервована; durable-local)
   ├─ DPS ACCEPTS the receipt    →  WRITTEN_OFF     (списана; the ONLY terminal-consumed state)
   ├─ DPS REJECTS the receipt    →  RELEASED / ESCALATED  (NOT written off — see below)
   └─ (online sale)              →  RESERVED→WRITTEN_OFF collapses on the near-instant ACK
```

- **RESERVED on the offline sale.** An offline sale mints an `OFFLINE_LOCAL_ACK` doc; at that boundary
  the stamp is recorded RESERVED in a **durable local stamp-ledger/outbox**. The report to the service
  is **NOT synchronous on the hot path** — it is an OUTBOX row drained by a background worker (mirrors
  the DPS drain worker), so it survives "offline = no network at all" and respects **invariant #1**
  (no network in the sale tx). Anchor to re-verify at impl: the offline-ack seam `stage_offline_ack.rs`
  and the drain/reconciliation worker (`services/reconciliation*`).
- **WRITTEN_OFF is triggered by DPS acceptance, never by the local sale.** The stamp is written off ONLY
  when the receipt is legally issued — online: at the `Sending→Sent` CAS / ACK (`stage_send.rs:1568`
  region, the online-issuance moment per the persistence pin); offline: at **drain acceptance**. A stamp
  is NEVER written off for a sale that DPS has not confirmed (this is the load-bearing correctness rule).
- **REJECT is the sharp edge.** A drain reject of an `OFFLINE_LOCAL_ACK` backlog doc is a **manual-recon
  trigger family** (§16.7 (1): universal EscalateManual, drain crossed the local-commit threshold). The
  stamp must then be **RELEASED / ESCALATED, never silently written off** — the good was physically
  sold, but the sale is not legally issued → the stamp state must NOT diverge from the doc's RMR outcome.
  Reuse the existing manual-reconciliation path; do not invent a stamp-only escape.
- **Crash-safety (the #192 class, projected onto stamps).** No stamp may rest in a non-terminal
  outbox state at a quiescent boundary that contradicts its doc's terminal state; boot-resume must
  reconcile stamp-ledger vs doc-state. This is the same invariant that bug #192 / the P1 boot-resume
  twin violated, cast onto the stamp projection.
- **Idempotency / ordering (#4).** RESERVED must be reported no later than WRITTEN_OFF for the same
  stamp; the service contract must make `reserved` and `written_off` **idempotent** and accept a
  `written_off` that supersedes a `reserved` (and, for online sales, a direct `written_off`). Bind the
  report to the canonical command / doc identity so replay is safe.

**Design delta on §3:** the `ExciseValidator` trait gains a report side —
`async fn report(&self, StampReport{ stamp, state: Reserved|WrittenOff|Released, doc_ref, snapshot_ref })
-> Result<(), ExciseError>` — consumed by the **stamp-drain worker**, not the sale path. New durable
table: `excise_stamp_outbox(stamp, doc_id, state, attempts, …)` (schema/migration → hot zone, involve
migration discipline).

## §5 Fuzzer-impact (rule: new feature → extend the fuzzer)

Per [[project_invariant_fuzzer_plan]] / [[project_fuzzer_alphabet_gaps]] the alphabet must gain:
- `Op::ExciseCheck{outcome}` where outcome ∈ {permitted, refused, unavailable} × node ∈ {online, offline};
- **feature-off must also be modelled** — with `enabled=false` the excise ops are no-ops and the model
  must prove byte-identical outcomes to the no-excise baseline (guards P1 below);
- offline deferred-verify: an offline sale authorized by snapshot vN must reconcile on return-online;
- oracle: a refused stamp NEVER yields a fiscal_documents row (pre-mint refusal), an unavailable+online
  sale follows `offline_policy` deterministically;
- **stamp write-back projection** — the stamp outbox must track the doc: model `Reserved` on offline
  sale, `WrittenOff` ONLY on DPS acceptance, `Released/Escalated` on drain reject; oracle = **the stamp
  state ALWAYS agrees with the doc terminal state** (no stamp WRITTEN_OFF while its doc is rejected/RMR;
  no stamp stuck non-terminal at a quiescent boundary — the #192 projection). Crash/boot-resume between
  sale and drain must not desync stamp-ledger vs doc-state.

---

## §6 RED-pins (test-first; each must bite empirically per the teeth bar)

- **P1 — feature-off byte-identity.** With `excise.enabled=false`, the full write-path fixture set is
  BYTE-identical to pre-feature (revert = no diff). Bite: flip a guard to run when disabled → RED.
- **P2 — refusal is audit-only, pre-mint.** A refused stamp → `audit_log` row keyed on request_id +
  inbox REJECTED, and **zero** `fiscal_documents` rows / **no** lnd consumed. Bite: mint a fiscal row
  on refusal → RED.
- **P3 — invariant #1: probe is OUTSIDE the write-tx.** A W3-style syn/static pin (mirror
  `with_immediate_no_foreign_io.rs`) forbids the excise network call inside any `with_immediate` scope.
  Bite: move the probe inside the guard tx → RED.
- **P4 — offline path.** Offline (or service-unavailable per policy) resolves DETERMINISTICALLY:
  fail_closed → refusal; fail_open_deferred → sale + `snapshot_version` recorded. Bite: nondeterministic
  branch → RED.
- **P5 — deferred-verify reconciliation.** An offline sale authorized by snapshot vN reconciles when
  back online; a stamp refused post-hoc escalates (audit/ALERT), never silently drops.
- **P6 — idempotency (#4).** Re-submitting the same request_id after a probe does not double-call the
  service in a way that changes outcome; the verdict is bound to the canonical command.
- **P7 — batch fidelity.** N stamps in one receipt → one `validate` call; each stamp's verdict maps to
  the correct line (no cross-line leakage).
- **P8 — WRITTEN_OFF only after DPS acceptance.** A stamp reaches WRITTEN_OFF ONLY when its doc is
  legally issued (online ACK / offline drain-accept). Bite: write off on the local offline sale (before
  drain) → RED.
- **P9 — reject never writes off.** A drain REJECT of the backlog doc → stamp RELEASED/ESCALATED and the
  doc → RMR (§16.7); the stamp is NEVER WRITTEN_OFF. Bite: write off on reject → RED.
- **P10 — stamp ⇄ doc agreement (the #192 projection).** At every quiescent boundary the stamp outbox
  state is consistent with its doc terminal state; boot-resume reconciles. Bite: leave a RESERVED stamp
  whose doc is already terminal-rejected → RED (scanner mirrors `StuckNonTerminalDoc`).
- **P11 — report is off the hot path + durable.** No excise `report` network call inside the sale
  write-tx (invariant #1, W3-style pin); the outbox row is durable and drained by the worker. Bite: call
  `report` inside the sale tx → RED.
- **P12 — idempotent replay.** Re-draining the same stamp outbox row (crash/retry) does not double-consume
  at the service; `reserved`→`written_off` is monotone and replay-safe.

---

## §7 Effort & sequencing

Off-by-default makes this a low-risk vertical slice; **S–M**, no rewrite.

1. `ExciseCfg` + `require_excise_endpoint()` + per-FN binding field — **LOW**.
2. `ExciseValidator` trait + a `passthrough` (always-permit, dev) + `sidecar` (HTTP) impl, on the crypto
   sidecar template — **LOW–MEDIUM** (the only real new plumbing: no generic external-client exists).
3. Pre-acquire probe (outside tx) + `RejectionReason::ExciseValidationFailed` + `reject()` wire — **LOW**.
4. Offline policy (B): local snapshot store + `offline_policy` gate + deferred-verify recon — **MEDIUM**
   (depends on the (a)/(b) legal decision).
5. **Stamp write-back lifecycle (§4b):** `excise_stamp_outbox` table + migration, stamp-drain worker,
   reserved/written-off/released transitions hung off the doc lifecycle + reject→RMR wiring — **MEDIUM,
   HOT ZONE** (offline / drain / reconciliation / schema). The bulk of the real work is here, not in the
   read-check. Involve migration + reconciliation discipline.
6. Fuzzer alphabet + P1–P12 teeth — **MEDIUM** (test-first, per charter).

**Blockers before build:** (i) the (a)/(b) legal/business decision (§1), (ii) `offline_policy` legal
input (§4). Neither blocks writing this dossier; both block finalizing a single design. The write-back
(§4b) is engineering-complete once (i)/(ii) are set — it rides the existing drain/RMR machinery.

**Revised overall effort:** with the write-back half, this is **M** (was S–M) — still no rewrite, but
the stamp-outbox drain lands squarely in the hot zone and needs the same recovery rigor as the DPS drain.

---

## §8 Invariant check (frozen list)

- **#1 no network in write-tx** — enforced by P3 (probe is pre-acquire, outside `with_immediate`).
- **#4 idempotency** — P6 (verdict bound to canonical command).
- **#5 offline time/code limits** — untouched; excise offline is orthogonal to the 168h/36h caps.
- **#6 full canonical payload** — stamps already in `FiscalLine.excise_stamps`; no summary-only path.
- **#7 schema_version** — the `ExciseVerdict`/snapshot carries a version; the canonical envelope is
  unchanged.
- **#8 recovery/reconciliation must not violate transitions** — deferred-verify (P5) routes through
  audit/ALERT; the stamp outbox (§4b) is a projection of the doc lifecycle: WRITTEN_OFF only on DPS
  acceptance (P8), reject → RELEASED/RMR not written-off (P9), and stamp⇄doc agreement at quiescence
  (P10, the #192 projection). The stamp report is off the sale tx + durable-drained (P11), like the DPS
  drain — no network in tx (#1), crash-safe replay (P12).
- All others (single-writer #2, channel-switch #3, graceful shutdown #9, checkbox-bypass #10) — not
  touched by an off-by-default pre-acquire guard.
