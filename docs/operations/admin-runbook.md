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
- [ ] **W2 PII access-control gate** (NON-OPTIONAL for any non-single-operator deployment) — exactly ONE option chosen and documented in the deployment runbook:
    - [ ] Grafana RBAC restricts panels §4.9 / §4.10 / §4.12 / §4.13 of the W12 dashboard spec to a compliance/security folder; user list audited; OR
    - [ ] `audit_log_public` SQL view created that redacts `operator_id` (salted hash) and `key_path` (basename strip); Grafana datasource points at the view only (raw `audit_log` NOT exposed); OR
    - [ ] Deployment marked single-operator self-hosted in writing (the operator who registers cashiers IS the dashboard reader; PII exposure to that operator is acceptable since they registered the data).  This option pre-approved for pilot per `feedback_autonomous_isolated_env`; production multi-tenant deployment MUST pick option (a) or (b).
- [ ] W2 audit-event semantics smoke executed (one success + one duplicate-FN failure; verify §4.12 shows 1 success row, §4.13 shows 1 failure row — see dashboard spec §10 item 8).

---

## 6a. W2 add-operator (cashier EDS-key registration)

`prro admin add-operator` registers a cashier (operator) by binding their EDS key file to a fiscal_number. The row lands in the **secure** SQLite database (`var/secure.db` per `[database].secure_db_path` config; hard-isolated from the main ledger per HIGH-AUDIT-01 — separate file, chmod 0o600, separate migration directory `migrations_secure/`).

### Syntax

```bash
prro admin add-operator \
    --config /etc/prro/config.toml \
    --inn 3456789012 \
    --name "Cashier Iryna" \
    --key-path /var/keys/cashier-iryna.dat \
    --fn 4000000001
```

### Password input

Two modes, autodetected from `stdin`:

- **TTY** (interactive operator at a terminal): password prompted twice. Mismatch → exit 64 (`EX_USAGE`) with `PasswordMismatch` error. Empty input → exit 64 with `EmptyPassword`.
- **Non-TTY** (CI / scripted): single line read from stdin. Empty → exit 64.

The plaintext password lives only in the CLI process memory; it is obfuscated via the WebCheck-symmetric `Coding` helper (NOT cryptography — see `rust/prro/src/runtime/coding.rs` doc-block for the threat model) and stored as the `key_pass_enc` BLOB. **The audit row `ADMIN_OPERATOR_REGISTERED` carries `operator_id`, `name`, `key_path` only — never the password or the encoded BLOB.**

### Pre-INSERT validation

The CLI performs the cross-DB foreign-key check that SQLite cannot enforce structurally (foreign keys do not span database files):

- `--fn` must exist in `fiscal_number_config` (main DB). Missing → exit 64 with `FiscalNumberNotInConfig`. Prevents orphan rows that would only surface at boot via `OPERATOR_ORPHAN_FN` Critical audit.
- `--inn`, `--name`, `--key-path`, `--fn` must be non-empty/non-whitespace. Empty → exit 64 with `EmptyArgument(<which>)`.
- The partial unique index `operators_active_fn_uidx WHERE is_active = 1` rejects a second active cashier for the same FN. Mapped to `DuplicateActiveCashier` exit 64. Rotation procedure: mark the previous row `is_active = 0` (manual `UPDATE`), then re-run `add-operator`.

### Recovery scenarios

| Symptom at boot | Audit signal | Operator action |
|---|---|---|
| Handler returns 503 with `error_code = OPERATOR_NOT_REGISTERED` | `OPERATOR_NOT_REGISTERED` Info on the FN | Run `add-operator` for that FN |
| Boot log shows `OPERATOR_ORPHAN_FN` Critical | Audit payload carries `operator_id` + `key_path` | Either add the missing FN to `fiscal_number_config` OR `DELETE FROM operators WHERE id = <id>` and re-register with correct FN |
| Boot log shows `OPERATOR_KEY_LOAD_FAILED` reason=FileNotFound | Critical audit + FN absent from registry | Verify `key_path` exists on disk; if cashier rotated, `UPDATE operators SET is_active = 0 WHERE …` and re-add |
| Boot log shows `OPERATOR_KEY_LOAD_FAILED` reason=WrongPassword | Critical | Cashier supplied wrong password during registration; rotate (mark old row inactive) and re-add |
| `ADMIN_OPERATOR_REGISTRATION_ATTEMPTED` audit row with **no matching `_REGISTERED` or `_FAILED` within 5 min** for the same FN+operator_id (dashboard panel §4.14) | Info → Action | Process crashed between the audit append and the secure-DB INSERT (or the post-INSERT REGISTERED audit append failed — audit-of-audit case from R4-3). Reconciliation: (1) `sudo -u prro sqlite3 var/secure.db "SELECT id, operator_id, is_active, created_at FROM operators WHERE fiscal_number = '<FN>'"`; (2) if a matching active row exists → cashier IS registered (post-INSERT audit append failed); emit a back-fill audit manually: `sudo -u prro sqlite3 var/prro.db "INSERT INTO audit_log (entity_type, entity_id, event_type, severity, event_payload_json) VALUES ('operator', '<FN>', 'ADMIN_OPERATOR_REGISTERED', 'INFO', json('{\"backfilled_from\": \"orphan_ATTEMPTED\"}'))"`; (3) if NO matching active row → process crashed pre-INSERT; re-run `prro admin add-operator` to retry; the orphan ATTEMPTED is forensic noise only. |

### Key-password rotation procedure

Cashier turnover or scheduled key rotation:

1. `sudo -u prro sqlite3 var/secure.db "UPDATE operators SET is_active = 0 WHERE fiscal_number = '<FN>' AND is_active = 1"` — must run as the `prro` service user; `secure.db` is `chmod 0o600 prro:prro` per HIGH-AUDIT-01, so a plain admin shell hits permission-denied.
2. `prro admin add-operator --fn <FN> ...` with the new cashier's data.
3. Verify boot audit log shows NO `OPERATOR_ORPHAN_FN` / `OPERATOR_KEY_LOAD_FAILED` for that FN on next start.

Historical rows (`is_active = 0`) accumulate intentionally for forensic continuity — they are NOT pruned automatically. Periodic ops cleanup (annual) may `DELETE` rows older than the legal retention window.

### Secure DB directory permissions (HIGH-AUDIT-01 supplemental)

`chmod 0o600` on `secure.db` + `secure.db-wal` + `secure.db-shm` prevents reading the cashier-key obfuscated BLOB by other local users.  **However**, write permissions on the *containing directory* allow any user with directory write access to **delete, rename, or truncate** the file regardless of its mode.  This is a Unix filesystem semantic, not a chmod bug.

**Operational mandate**: the directory containing `secure_db_path` MUST be:

- owned by the `prro` service user;
- mode `0o700` (recommended) or `0o750` (acceptable if the service group also needs the secure DB visible for backup tooling);
- NOT inside `/tmp`, `/var/tmp`, or any other world-writable location.

Recommended path: `/var/lib/prro/secure/secure.db` with `/var/lib/prro/secure/` at `0o700 prro:prro`.

`prro admin doctor` (W2 follow-up) MAY warn (not fail) when the parent directory mode is broader than `0o755`; for now the discipline is operator-enforced via this runbook.

### W2 manual rollback (LOW-PR90-02)

If migration 020 must be reverted (e.g., schema change in a follow-up makes the existing data incompatible):

All `sqlite3` invocations below must run **as the `prro` service user** (via `sudo -u prro`); the secure DB is `chmod 0o600 prro:prro` per HIGH-AUDIT-01 and refuses access from other accounts.

1. Stop `prro` (every connection holding the secure pool must close).
2. `sudo -u prro sqlite3 var/secure.db "DELETE FROM _sqlx_migrations WHERE version = 20"`
3. `sudo -u prro sqlite3 var/secure.db "DROP TABLE operators"` (and `DROP INDEX operators_active_fn_uidx`, `DROP INDEX operators_fiscal_number_idx` if present in earlier checksum revisions).
4. Re-deploy with the corrected `migrations_secure/020*.sql` file (or revert the binary to a version that does not require the migration).
5. Restart `prro` — `sqlx::migrate!` re-applies the corrected file and records a fresh checksum.

`prro_gate.db` is **not touched** by this procedure — the main ledger remains intact through the rollback. This is the design payoff of the HIGH-AUDIT-01 split: rollback blast radius is contained to the secure DB.

---

## 6c. RS-1 runtime supervisor deployment (`supervisor.enabled = true`)

The runtime supervisor (`prro serve` with `[supervisor] enabled = true`) drives boot reconciliation + the offline-backlog drain loop + the return-online probe loop. It ships **gated off by default** (`enabled = false` → the binary boots and idles, M1 behaviour). Before flipping it on for the pilot, the process must run under a **process supervisor with a restart policy**:

- **systemd:** `Restart=on-failure` (and `TimeoutStopSec` GREATER than `supervisor.shutdown_grace_seconds`).
- **docker / compose:** `restart: on-failure` (and `stop_grace_period` greater than `supervisor.shutdown_grace_seconds`).

**Why `Restart=on-failure` is mandatory (F1):** a panic in a drain/probe tick loop is an *invariant bug*, not an operational error (wire/DB failures are caught and logged inside the tick). When a loop dies, the supervisor emits a CRITICAL `SUPERVISOR_LOOP_DIED` audit (best-effort — see below), winds down the sibling loop, and **exits non-zero** so the process supervisor re-launches. A fresh boot re-runs reconciliation and the W9b drain (both crash-safe), so no offline receipt is lost. The design is **fail-stop, not silent-degrade**: a dead loop takes the whole `prro serve` process down. So WITHOUT a restart policy the gateway **exits and stays down until an operator restarts it** — fiscal traffic halts entirely (loud, not a silent dead loop), which is why the restart policy is mandatory before enabling the supervisor.

The `SUPERVISOR_LOOP_DIED` audit is **best-effort**: if the failure is itself a DB problem, the audit insert can also fail, in which case the panic is recorded via `tracing` only (the non-zero exit + the OS supervisor's restart log remain the durable signal).

**Why `TimeoutStopSec` / `stop_grace_period` must exceed `shutdown_grace_seconds`:** on SIGTERM the supervisor flips a shutdown watch and joins all tasks within `shutdown_grace_seconds` (default 30s — aligned with the default DPS request timeout so an in-flight drain call finishes within grace; clamp `[1, 80]`). If the orchestrator's stop timeout is shorter, it SIGKILLs mid-join. A SIGKILL is *safe* (the per-doc drain is crash-equivalent and re-drained next boot) but non-graceful; sizing the stop timeout above the grace lets the clean path run.

### New audit event

| Event | Severity | Meaning |
|-------|----------|---------|
| `SUPERVISOR_LOOP_DIED` | Critical | A supervised task (drain/probe tick loop, or — RS-2 — an ingress server) exited BEFORE the shutdown flip. Payload: `{ loop, expected, panicked, detail }` where `expected` is the task's terminal policy (`RunsUntilShutdown` / `GracefulOkAfterShutdown`), `panicked` is true only for a genuine panic, and `detail` carries the cause (a panic message OR a real task error such as an axum `serve()` failure). The supervisor then exits non-zero for an orchestrator restart. Investigate the `detail` — it is an invariant bug, not a routine failure. |

---

## 7. Related documents

- `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2 — W2 plan + review-gated acceptance items (HIGH-PR90-01, MED-PR90-01, MED-PR90-02, MED-PR90-03, MED-PR90-04, LOW-PR90-01, LOW-PR90-02)
- `docs/superpowers/plans/2026-05-24-m3b-w12-post-closure-hardening.md` — hardening plan (8 RECs)
- `docs/superpowers/specs/2026-05-25-w12-post-hardening-review-findings.md` — review findings (CONCERN/GAP/TD registry)
- `docs/superpowers/specs/2026-05-25-w12-tiered-hold-degradation.md` — Tiered degradation spec (REC-1 + Tier 3)
- `docs/LEGAL_INVARIANTS.md` — fiscal-correctness invariants reference
