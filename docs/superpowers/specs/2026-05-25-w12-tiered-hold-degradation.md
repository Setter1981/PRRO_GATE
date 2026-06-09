# Spec: W12 Tiered Hold Degradation (REC-1 + REC-2 + Tier 3 admin)

**Status**: APPROVED + IMPLEMENTED (PRs #76 + #77 + #79 + #81)
**Author**: Operator + co-architect, captured 2026-05-25
**Replaces**: pre-W12 boot_phase Hold semantics (manual_recon=true on transient DPS failures).
**Scope**: Cross-PR architectural specification for the 3-tier degradation framework introduced by REC-1 (Tier 1+2), REC-2 (backoff scheduling), and Tier 3 admin recovery.  Captures invariants + state-machine + audit contract з single authoritative source.

---

## 1. Problem statement

Post-W12 + Phase 2 wiring, transient DPS failures на drain orchestrator path produce `DocVerdict::HoldFnDrain` per-doc verdicts that halt FN drain per-tick (per W0b state-unchanged contract).  Без degradation framework:

- A persistently-broken FN (network outage / cert expiry / DPS-side config issue) accumulates indefinite Hold ticks.
- Operator gets no escalation signal until manually inspecting logs.
- Retry storm (60+ DPS wire-calls/hour per stuck FN) consumes resources + creates audit-log noise.
- 36h offline cap (cert.NotAfter-2160min) approaches без operator visibility.

**Operator-pinned constraint** (memory `feedback_manual_recon_catastrophe`): 4 years of UA PRRO production saw zero observed Manual-recon incidents.  Auto-escalation to `REQUIRES_MANUAL_RECONCILIATION` is **prohibited** — system must bias toward operator-decided recovery з safe intermediate degradation states.

---

## 2. 3-Tier degradation framework

### 2.1 State machine

```
            ┌──────────────────────────────────────────────────────────┐
            │           PER-FN DEGRADATION STATE MACHINE               │
            └──────────────────────────────────────────────────────────┘

[Initial]   GOING_ONLINE / Acked drain
    ↓
[Hold-1]    1st transient DPS Hold
    consecutive_holds = 1
    backoff = 60s
    ↓
[Hold-N]    Nth Hold (counter accumulates)
    consecutive_holds = N
    backoff = min(2^N * 30s, 30min cap)
    ↓
[Tier 1]    counter >= 10
    KVT2_CONFIRM_PROLONGED_HOLD Warning audit per tick
    No state change; operator dashboard signal
    ↓
[Tier 2]    counter >= 50
    OFFLINE_DRAIN_FN_STOP_MODE Critical audit
    node_state.mode CAS: GOING_ONLINE → STOP_MODE (atomic)
    New чек ingress на цю FN rejected at adapter layer
    Existing held docs remain in Sent/Kvt1 awaiting operator
    ↓
[Operator inspection]
    Verify root cause (DPS / cert / network / contract)
    Resolve upstream
    ↓
[Tier 3]    Operator-invoked admin reset
    prro admin reset-stop-mode --fn X --reason "..."
    Atomic envelope:
      - node_state.mode CAS: STOP_MODE → GOING_ONLINE
      - UPDATE fiscal_documents SET consecutive_holds=0 WHERE fn=X
      - INSERT audit_log ADMIN_STOP_MODE_RESET Critical
    ↓
[Recovery]  W8 return_online_probe re-validates → ONLINE
    Drain resumes: Kvt1Reentry / SentReplay chains advance docs to ACK
    fiscal_documents.consecutive_holds reset by advance envelope on first Ack
```

### 2.2 Counter mechanics

**Source of truth**: `fiscal_documents.consecutive_holds INTEGER NOT NULL DEFAULT 0 CHECK (>= 0)` (DDL migration 018, REC-1 6.1.1).

- **Persistent** (survives crash/restart per `feedback_manual_recon_catastrophe` rationale).
- **Per-doc** (not per-FN; each held doc has its own counter).
- **Increment**: inside `Envelope 1c-hold-light` (SentFresh/Kvt1Reentry) AND `Envelope 1c-hold` bundled (SentReplay) atomically з audit emit.
- **Reset**: inside Advance envelopes (1a, 1b, 1a-replay, 1c-post) atomically з state CAS + audit.

**Tier triggers** evaluate `consecutive_holds` value returned from envelope (plumbed through `ConfirmDrainOutcome::HoldFnDrain { ..., consecutive_holds: i64 }` → `DocVerdict::HoldFnDrain { ..., consecutive_holds }`).

### 2.3 In-memory backoff (REC-2) — separate concern

`App.backoff_state: HashMap<String, BackoffState>` — **distinct from persistent counter**:
- Purely tick-scheduler concern (not Tier escalation).
- In-memory only; resets on App restart (pragmatic operator-pinned choice).
- Decouples drain tick cadence от persistent doc state.
- Per-FN isolation: backoff on FN-A не torcha FN-B.

Backoff schedule: `min(2^consecutive_holds * 30s, 30min)`.

---

## 3. Audit contract

### 3.1 New audit events (Post-W12 hardening)

| Event | Severity | Tier | Fires when | Payload fields |
|---|---|---|---|---|
| `KVT2_CONFIRM_PROLONGED_HOLD` | Warning | 1 | counter >= 10 per tick | `document_id`, `projection`, `consecutive_holds`, `tier`, `tier_threshold` |
| `OFFLINE_DRAIN_FN_STOP_MODE` | **Critical** | 2 | counter >= 50 (first transition only) | `document_id`, `fiscal_number`, `consecutive_holds`, `tier`, `tier_threshold`, `node_mode_target` |
| `ADMIN_STOP_MODE_RESET` | **Critical** | 3 | Operator-invoked admin reset | `fiscal_number`, `reason`, `mode_before`, `mode_after`, `docs_reset_count`, `tier=3` |

### 3.2 Audit ordering invariant

Per Tier degradation tick:
1. `KVT2_CONFIRM_HOLD` (per-doc, Warning) — emitted by Envelope 1c-hold-light/bundled.
2. **EITHER** `KVT2_CONFIRM_PROLONGED_HOLD` (Warning, Tier 1) **OR** `OFFLINE_DRAIN_FN_STOP_MODE` (Critical, Tier 2) — mutually exclusive per drain orchestrator `if/else if` ordering (Tier 2 takes precedence).
3. NEVER simultaneous Tier 1 + Tier 2 on same tick.

Test fixture: `c612_tier_2_stop_mode_escalation_fires_at_50_consecutive_holds` locks this invariant explicitly (40 Tier-1 audits on ticks 10..=49 + 1 Tier-2 audit on tick 50; Tier 1 NOT re-fired on Tier-2 tick).

---

## 4. State machine invariants

| Invariant | Description |
|---|---|
| **I1** (no DPS/crypto в SQLite tx) | All Tier triggers emit audit through pool-only OR pre-existing `with_immediate`; no DPS calls inside tx. |
| **I2** (single-writer per FN) | Drain orchestrator mutex; admin CLI singleton lock; Tier 2 STOP_MODE CAS atomically guarded `WHERE mode='STOP_MODE'`. |
| **I4** (atomicity) | Increment + audit bundled in Envelope 1c-hold; Tier 2 CAS + audit bundled; admin reset (mode CAS + counter reset + audit) all atomic. |
| **I5** (offline 36h cap) | 30min backoff cap + Tier 2 escalation timing aligns з cap window; operator has full 36h intervention budget. |
| **I8** (audit chain) | 4 new structured events з full forensic payload; operator dashboards can grep/aggregate. |
| **I10** (minimal diff) | All Tier mechanics additive; pre-existing drain paths untouched. |

---

## 5. Operator decision tree

```
[Watch for KVT2_CONFIRM_HOLD Warning]
        ↓
< 10 ticks: monitor; nothing required
        ↓
>= 10 ticks (Tier 1 fires): investigate proactively
        ↓
Issue resolves naturally? → Counter resets on next Ack; no action
        ↓
Issue persists, reaches Tier 2 (Critical OFFLINE_DRAIN_FN_STOP_MODE):
        ↓
1. Inspect: DPS reachable? Cert valid? Auth working? Contract drift?
        ↓
2. Resolve root cause upstream
        ↓
3. prro admin reset-stop-mode --fn X --reason "<resolution description>"
        ↓
4. Restart prro serve (to clear in-memory backoff) OR wait ≤ 30min
        ↓
5. Verify recovery via next drain tick:
   - OFFLINE_DRAIN_KVT2_ADVANCED audit fires
   - Doc reaches ACK state
   - node_state.mode transitions to ONLINE (post W8 probe success)
```

### 5.1 When NOT to use Tier 3 admin reset

- FN NOT in STOP_MODE — command refuses з `AdminError::NotInStopMode`.  Use only after Tier 2 auto-escalation; do NOT pre-emptively reset.
- Root cause unresolved — reset only clears counter; if DPS still broken, next tick will re-accumulate and re-escalate.
- Empty/whitespace reason — refused з `AdminError::EmptyReason`.  Forensic accountability требує operator-supplied non-empty description.

---

## 6. Implementation references

| Component | File | Source PR |
|---|---|---|
| DDL migration | `rust/prro/migrations/018_consecutive_holds.sql` | #76 |
| Repo helpers | `rust/prro/src/db/repositories/fiscal_documents.rs::{increment,reset}_consecutive_holds_tx` | #76 |
| Tier 1 trigger | `rust/prro/src/services/offline_sync/backlog_drain.rs::trigger_tier_1_prolonged_hold` | #77 |
| Tier 2 trigger | `rust/prro/src/services/offline_sync/backlog_drain.rs::trigger_tier_2_stop_mode` | #77 |
| node_state CAS | `rust/prro/src/db/repositories/node_state.rs::set_mode_stop_mode_tx` | #77 |
| Tier 3 admin module | `rust/prro/src/admin.rs` | #79 |
| Tier 3 CLI subcommand | `rust/prro/src/main.rs::AdminCmd::ResetStopMode` | #79 |
| Backoff calculator | `rust/prro/src/services/offline_sync/backoff.rs` | #81 |
| Scheduled drain wrapper | `rust/prro/src/app.rs::drain_offline_backlog_scheduled` | #81 |
| End-to-end test | `rust/prro/tests/app_drain_offline_backlog.rs::polish_tier_degradation_then_admin_reset_then_drain_succeeds_end_to_end` | #82 |

---

## 7. Open issues / future considerations

| ID | Description | Tracker |
|---|---|---|
| CONCERN-1 | `App.backoff_state` admin reset coordination (separate-process scenario today; in-process future) | `docs/superpowers/specs/2026-05-25-w12-post-hardening-review-findings.md` |
| TD-4 | Reqwest connection pool clamping (REC-2 sub-item deferred) | Create PRRO_GATE-??? ticket |
| TD-7 | `W12ConfirmOutcome::DeferredKvt1` deprecated-but-retained variant cleanup | Post-pilot |
| TD-8 | Periodic orphan-trace scanner (long-uptime processes) | M3+ runtime supervisor |
