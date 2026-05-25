# PRRO Gateway — Operator Admin Runbook

**Audience**: Operators responsible for PRRO gateway day-to-day operations.
**Scope**: Administrative commands + escalation procedures + audit-event interpretation.
**Last updated**: 2026-05-25 (post-W12 hardening cycle).

---

## 1. Quick reference

| Situation | Operator action |
|---|---|
| FN entered STOP_MODE (Tier 2 auto-escalation) | `prro admin reset-stop-mode` після root-cause resolution |
| FN seeing repeated Tier 1 warnings (>= 10 holds) | Investigate DPS / cert / network; intervene preventively if pattern persists |
| `TRANSPORT_TRACE_ORPHAN_CLOSED` audit spike | Investigate host stability (OOM / SIGKILL / power events) |
| Pre-pilot deployment | Run `prro doctor --config <path>` preflight checks |
| First-time DB init | Run `prro migrate --config <path>` |
| Production startup | Run `prro serve --config <path>` |

---

## 2. Tier escalation decision tree

Per **REC-1 Tiered Hold Degradation** (post-W12 hardening, PRs #76+#77+#79):

```
Document hits transient DPS Hold
        ↓
[Tick 1-9]  Hold counter increments silently (transport_trace persists)
        ↓
[Tick 10+]  KVT2_CONFIRM_PROLONGED_HOLD Warning audit per tick
            ↓ Operator action: Investigate; intervene if pattern persists
        ↓
[Tick 50]   OFFLINE_DRAIN_FN_STOP_MODE Critical audit + STOP_MODE CAS
            ↓ Operator action: REQUIRED — investigate + admin reset
        ↓
[Operator]  prro admin reset-stop-mode --fn X --reason "..."
            ↓
[Recovery]  node_state.mode → GOING_ONLINE
            consecutive_holds → 0 for all FN docs
            Next drain tick: W8 probe re-validates DPS → ONLINE
            Doc resumes through Kvt1Reentry/SentReplay chain → ACK
```

### Operator-pinned principle

**Manual reconciliation (`REQUIRES_MANUAL_RECONCILIATION` doc state) is ЧП из ЧП** — 4 years of UA PRRO production saw zero observed Manual-recon incidents.  System design biases strongly toward Hold/Retry/Reset paths over Manual.  Tier 2 STOP_MODE is intermediate — operator-decided recovery, NOT auto-Manual.

---

## 3. Admin commands reference

### 3.1 `prro admin reset-stop-mode` — Tier 3 manual recovery

**Use when**: An FN auto-escalated to STOP_MODE (Tier 2, see `OFFLINE_DRAIN_FN_STOP_MODE` audit) AND operator verified root cause resolved (DPS restored, creds rotated, contract fix deployed, etc.).

**Command**:
```bash
prro admin reset-stop-mode \
    --config /etc/prro/config.toml \
    --fiscal-number <10-digit FN> \
    --reason "<non-empty operator-supplied description for forensic trail>"
```

**Example**:
```bash
prro admin reset-stop-mode \
    --config /etc/prro/config.toml \
    --fiscal-number 1234567890 \
    --reason "DPS network outage resolved 2026-05-25T10:30 UTC; verified ping OK"
```

**Atomic side-effects** (one `with_immediate` envelope):
1. CAS `node_state.mode` `STOP_MODE → GOING_ONLINE` (fail-loud if current mode != STOP_MODE).
2. `UPDATE fiscal_documents SET consecutive_holds=0 WHERE fiscal_number=? AND consecutive_holds > 0`.
3. INSERT `audit_log.ADMIN_STOP_MODE_RESET` Critical з payload `{fiscal_number, reason, mode_before, mode_after, docs_reset_count, tier=3}`.

**Exit codes**:
| Code | Meaning | Operator response |
|---|---|---|
| 0 | OK; stdout: `ADMIN_STOP_MODE_RESET OK fiscal_number=X docs_reset_count=N` | Continue — recovery initiated |
| 64 (EX_USAGE) | --reason empty/whitespace OR FN not found OR FN not in STOP_MODE | Fix command args |
| 66 (EX_NOINPUT) | Config read/parse failure OR DB pool open failure OR singleton lock contention | Investigate config/DB/concurrent process |

### 3.2 In-process coordination caveat (CONCERN-1 from review)

⚠️ **Important** (from post-hardening review CONCERN-1):

`App.backoff_state` is **in-memory** (HashMap у running `prro serve` process).  Admin CLI runs as **separate process** — clears persistent counter (`fiscal_documents.consecutive_holds`) + flips `node_state.mode`, but does NOT clear in-memory backoff state of the running `prro serve` instance.

**Operator workflow consequence**:

After `prro admin reset-stop-mode`:
- **Option A (recommended)**: Restart `prro serve` to clear in-memory backoff state → fresh tick schedule.
- **Option B**: Wait up to 30 min (REC-2 cap) for current backoff window to elapse → drain resumes naturally.

Future M3+ HTTP/RPC admin endpoint exposed within running `prro serve` will be coordinated to clear in-memory state automatically.

---

## 4. Audit event glossary

### 4.1 Tier degradation events (REC-1)

| Event | Severity | Fires when | Operator action |
|---|---|---|---|
| `KVT2_CONFIRM_HOLD` | Warning | Per-tick on transient DPS Hold (any doc) | Monitor; no action needed individually |
| `KVT2_CONFIRM_PROLONGED_HOLD` | Warning | counter >= 10 consecutive holds on single doc | Investigate persistent issue (network / cert / auth) |
| `OFFLINE_DRAIN_FN_STOP_MODE` | **Critical** | counter >= 50 → FN auto-escalates to STOP_MODE | **REQUIRED**: investigate + admin reset |
| `ADMIN_STOP_MODE_RESET` | **Critical** | Operator-invoked Tier 3 manual reset | Recovery initiated — verify next drain tick succeeds |

### 4.2 Forensic / durability events

| Event | Severity | Fires when | Operator action |
|---|---|---|---|
| `TRANSPORT_TRACE_ORPHAN_CLOSED` | Info | Boot scanner closes trace row orphaned by crash (SIGKILL/OOM/power) | Monitor rate; spike → investigate host stability |
| `KVT2_CONFIRM_STRUCTURAL_DRIFT` | Error | StructuralDrift halts FN drain (NotFound/Mismatch) | Investigate — possible state-machine breach |
| `OFFLINE_DRAIN_DOC_FAILED` | Warning | SentReplay NotFound → ER cohort (safe Pattern B redrive) | Monitor; auto-retries via W9b |

### 4.3 Drain orchestrator events

| Event | Severity | Fires when |
|---|---|---|
| `OFFLINE_DRAIN_COMPLETED` | Info | Eligible-arm finalize: all docs ACK'd, session closed |
| `OFFLINE_DRAIN_PARTIAL` | Info | Some docs held; finalize NotEligible per `DocsHeldAtSent`/`DocsHeldAtKvt1`/`DocsErRedriveQueued` reason |
| `OFFLINE_DRAIN_KVT2_ADVANCED` | Info | Per-doc Envelope 1a/1b/1a-replay advance (Sent/Kvt1 → Kvt2) |
| `STAGE_FINALIZE_ACK` | Info | Per-doc Envelope 2 (Kvt2 → Ack terminal) |

---

## 5. Backoff scheduling (REC-2)

After any Hold outcome on FN-A, per-FN backoff schedules next drain tick:

| consecutive_holds | next_tick_delay |
|---|---|
| 1 | 60s |
| 2 | 2min |
| 3 | 4min |
| 4 | 8min |
| 5 | 16min |
| ≥ 6 | 30min (cap) |

**Per-FN isolation**: Backoff on FN-A не torcha FN-B.  Healthy FNs continue normal drain cadence.

**Reset triggers**: Any non-Hold advance (Acked / StructuralDrift halt) immediately clears counter → next tick eligible immediately.

**30min cap rationale**: aligns з 36h offline cap window (cert.NotAfter-2160min) — ~72 retries fit, sufficient coverage without operator alert spam.

---

## 6. Pre-pilot checklist

- [ ] `prro doctor --config <path>` passes all checks
- [ ] `prro migrate --config <path>` applied (all migrations through 019)
- [ ] Live DPS smoke cycle re-run: SHIFT_OPEN → SELL → Z_REPORT on test DPS (post-hardening)
- [ ] Dashboards configured для 8 audit events listed above
- [ ] Operator on-call rotation knows admin reset-stop-mode procedure
- [ ] CI infrastructure repaired (TD-1 — see findings doc)

---

## 7. Related documents

- `docs/superpowers/plans/2026-05-24-m3b-w12-post-closure-hardening.md` — hardening plan (8 RECs)
- `docs/superpowers/specs/2026-05-25-w12-post-hardening-review-findings.md` — review findings (CONCERN/GAP/TD registry)
- `docs/superpowers/specs/2026-05-25-w12-tiered-hold-degradation.md` — Tiered degradation spec (REC-1 + Tier 3)
- `docs/LEGAL_INVARIANTS.md` — fiscal-correctness invariants reference
