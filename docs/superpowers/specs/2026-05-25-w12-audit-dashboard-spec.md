---
title: "W12 Hardening Audit Dashboard Spec — Operator Wiring Reference"
date: 2026-05-25
authors: ["claude (gateway-side spec author)"]
status: draft-v1
scope: operator-side observability (Grafana + optional Prometheus sidecar)
related:
  - docs/superpowers/specs/2026-05-25-w12-post-hardening-review-findings.md
  - docs/superpowers/specs/2026-05-25-w12-tiered-hold-degradation.md
  - docs/operations/admin-runbook.md
  - rust/prro/migrations/001_core_identities.sql (audit_log DDL)
audience: site reliability / operator-side dashboards / on-call rotation
---

# W12 Hardening Audit Dashboard Spec

## 0. TL;DR

Чотири нові структуровані audit events landed з W12 post-hardening cycle потребують operator-side wiring перед pilot launch. Ця spec містить:

- **Per-event mapping table** (event_type → entity_type → payload JSON keys → severity).
- **SQL query catalog** для Grafana panels (готовий-до-копії-вставити SQL для SQLite datasource).
- **Recommended schema fix** — `audit_log` має covering index gap для cross-event time-series queries; рекомендовано додати `(event_type, created_at DESC)` index перед production wiring (cheap migration, big query speedup).
- **Naming inconsistency callout** — `ADMIN_STOP_MODE_RESET` використовує `entity_type = "fn"`, тоді як інші FN-scoped events drain piece можуть бути joined через `fiscal_documents.fiscal_number`. Розрахункова стратегія документована § 6.
- **Architecture choice**: Rust gateway НЕ exposes Prometheus `/metrics` endpoint; sole durable signal source = audit_log SQLite table. Документуємо два workable approaches (§ 2).

---

## 1. Audit log schema (relevant subset)

DDL з `rust/prro/migrations/001_core_identities.sql` lines 83-98:

```sql
CREATE TABLE audit_log (
    audit_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type        TEXT    NOT NULL,
    entity_id          TEXT    NOT NULL,
    event_type         TEXT    NOT NULL,
    severity           TEXT    NOT NULL CHECK (severity IN ('INFO','WARNING','ERROR','CRITICAL')),
    actor              TEXT,                       -- nullable; system-emitted = NULL
    event_payload_json TEXT,                       -- nullable; JSON blob
    created_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

CREATE INDEX ix_audit_entity ON audit_log(entity_type, entity_id, audit_id DESC);
```

**Notes**:
- `severity` enum mapping (`src/db/models/enums.rs` 110-115): `Info → "INFO"`, `Warning → "WARNING"`, `Error → "ERROR"`, `Critical → "CRITICAL"`.
- `event_payload_json` is **single JSON column** — no per-field columns. Grafana queries must use SQLite `json_extract()`.
- `actor IS NULL` for all 4 W12 events (system-emitted). Operator-supplied actor fields reserved for future manual operator actions.
- Existing index `ix_audit_entity` is **per-entity** — efficient for "show last N events for this document" but **inefficient** for "count of event_type X per hour across all entities". See § 8 recommended migration.

---

## 2. Architecture options

Rust gateway has **no Prometheus / metrics-exporter-prometheus** dependency. The audit_log SQLite table is the sole durable signal source. Two viable wiring approaches:

### Option A — Direct SQLite datasource (RECOMMENDED for pilot)

- Grafana з [grafana-sqlite-datasource plugin](https://github.com/fr-ser/grafana-sqlite-datasource) reads `var/prro.db` (read-only mount).
- Pro: zero deploy footprint, queries в spec нижче ready-to-paste.
- Con: SQLite WAL read concurrency limit on busy node; recommended `PRAGMA wal_autocheckpoint` + `journal_mode=WAL` (already set by Rust gateway init).
- **Read-only mount mandatory**: dashboard plugin must not be able to write — audit_log integrity is forensic-grade.
- **⚠ HARD ISOLATION REQUIREMENT (HIGH-AUDIT-01 fix, 2026-05-25):** Grafana **MUST NEVER** be granted access to `var/secure.db`. That file (chmod 600, root + prro service user only) holds the `operators` table з cashier EDS-key paths + obfuscated passwords. Mounting `secure.db` read-only into Grafana would expose `SELECT key_path, key_pass_enc FROM operators` to any user з editor rights (or via SQL injection in panel queries) — the symmetric obfuscation is NOT crypto-strong, and recovered passwords would enable forged fiscal documents. **Only `var/prro.db` is Grafana-mountable.** See M4 plan §11 (`docs/superpowers/plans/2026-05-25-m4-ingress-plan.md`) for the secure-db split decision in full.

### Option B — Periodic exporter sidecar (post-pilot evolution)

- Small `prro-audit-exporter` process scrapes audit_log → emits Prometheus counters + gauges.
- Pro: standard Prometheus tooling, retention via remote_write to long-term store.
- Con: extra process, double-counting risk on scraper restart, exporter must track last-seen `audit_id` durably.
- **Out of scope for this spec** — call this out для post-pilot M3+ roadmap, не pilot-gating.

**Pilot decision**: Option A. Pilot scope = single-node deployments where SQLite WAL читання stays comfortably under capacity.

---

## 3. Per-event mapping table

| event_type | entity_type | entity_id | severity | Tier | Source PR | Emit site |
|---|---|---|---|---|---|---|
| `KVT2_CONFIRM_PROLONGED_HOLD` | `fiscal_document` | doc_id hex | WARNING | 1 | #77 | `services/offline_sync/backlog_drain.rs:2022` |
| `OFFLINE_DRAIN_FN_STOP_MODE` | `fiscal_document` | doc_id hex | CRITICAL | 2 | #77 | `services/offline_sync/backlog_drain.rs:2074` |
| `ADMIN_STOP_MODE_RESET` | **`fn`** ⚠ | fiscal_number | CRITICAL | 3 | #79 | `admin.rs:118` |
| `TRANSPORT_TRACE_ORPHAN_CLOSED` | `transport_trace` | doc_id hex | INFO | forensic | #80 | `services/reconciliation/boot_phase.rs:1273` |
| `OPERATOR_KEY_LOAD_FAILED` | **`operator`** | fiscal_number | CRITICAL | W2 boot | #98 | `runtime/bindings.rs::build_from_db` |
| `OPERATOR_ORPHAN_FN` | **`operator`** | fiscal_number | CRITICAL | W2 boot | #98 | `runtime/bindings.rs::build_from_db` |
| `OPERATOR_NOT_REGISTERED` | **`operator`** | fiscal_number | INFO | W2 boot | #98 | `runtime/bindings.rs::build_from_db` |
| `ADMIN_OPERATOR_REGISTERED` | **`operator`** | fiscal_number | INFO | W2 admin | #98 | `admin.rs::add_operator` |

⚠ Entity-type naming inconsistency callout: `ADMIN_STOP_MODE_RESET` uses entity_type `"fn"`; drain Tier 2 event uses entity_type `"fiscal_document"`; W2 operator-binding events use entity_type `"operator"`.  See § 6 for join strategy.

### 3.0a W2 operator-binding events — payload shapes (PR #98)

#### `OPERATOR_KEY_LOAD_FAILED`

```json
{
  "operator_id": "<cashier INN>",
  "key_path":    "<absolute path to .dat/.jks>",
  "reason":      "FileNotFound | WrongPassword | EmptyEncoded | Other"
}
```

**Critical alert**: every occurrence pages on-call.  FN is absent from the BindingsRegistry until the operator re-registers; the HTTP handler will respond 503 OPERATOR_NOT_REGISTERED for any request on that FN.

#### `OPERATOR_ORPHAN_FN`

```json
{
  "operator_id": "<cashier INN>",
  "key_path":    "<absolute path>"
}
```

**Critical alert**: cross-DB FK violation — an `operators` row references a `fiscal_number` not in `fiscal_number_config`.  Caused by either (a) `fiscal_number_config` row deleted post-registration, or (b) operator typo at registration that PR-B's CLI pre-check missed.  Recovery: per admin-runbook §6a.

#### `OPERATOR_NOT_REGISTERED`

```json
{}
```

Empty payload — entity_id is the FN itself.  **Info severity, not alerting.**  Configured FN has no active operators row; expected during initial deployment before `prro admin add-operator` is run, or after cashier rotation without re-registration.  Dashboard should panel-display the current count of "configured FNs without active cashier" for ops awareness.

#### `ADMIN_OPERATOR_REGISTERED`

```json
{
  "operator_id": "<cashier INN>",
  "name":        "<human-readable cashier name>",
  "key_path":    "<absolute path>"
}
```

**Info-severity forensic trail** for every `prro admin add-operator` invocation.  Payload constructed via `serde_json::json!` so embedded quotes / control chars escape correctly.  **Password and `key_pass_enc` BLOB NEVER appear** in payload (security-reviewer pin + PR-B external audit F1 fix).

### 3.1 Payload JSON shapes

#### `KVT2_CONFIRM_PROLONGED_HOLD`

```json
{
  "document_id":       "<doc_id_hex>",
  "projection":        "HeldAtSent | HeldAtKvt1 | ErRedriveQueued",
  "consecutive_holds": <int>,
  "tier":              1,
  "tier_threshold":    10
}
```

**Important**: Re-fires every drain tick the doc stays held (intentional rate signal). Dashboard panels must aggregate (per-FN, per-hour) or chart distinct documents — raw counts overstate.

#### `OFFLINE_DRAIN_FN_STOP_MODE`

```json
{
  "document_id":       "<doc_id_hex>",
  "fiscal_number":     "<FN>",
  "consecutive_holds": <int>,
  "tier":              2,
  "tier_threshold":    50,
  "node_mode_target":  "STOP_MODE"
}
```

**Atomicity**: emitted INSIDE `with_immediate` envelope з node_state CAS → atomicity guarantee (event row exists IFF FN entered STOP_MODE). Per-event correlation reliable.

#### `ADMIN_STOP_MODE_RESET`

```json
{
  "fiscal_number":     "<FN>",
  "reason":            "<operator-supplied non-empty>",
  "mode_before":       "STOP_MODE",
  "mode_after":        "GOING_ONLINE",
  "docs_reset_count":  <int>,
  "tier":              3
}
```

**Critical alert**: every occurrence pages on-call (per § 7.1 alerting rules). `reason` field surfaces в panel detail для post-incident review.

#### `TRANSPORT_TRACE_ORPHAN_CLOSED`

```json
{
  "document_id":  "<doc_id_hex>",
  "attempt_no":   <int>,
  "started_at":   "<ISO8601>",
  "closed_at":    "<ISO8601>",
  "outcome_kind": "SYSTEM_CRASH",
  "reason":       "Process exited mid-wire-call; no outcome captured (boot scanner)",
  "ttl_secs":     <int>
}
```

**Forensic-only**: INFO severity, emitted only at boot phase after a crash recovery. Used для crash-rate baseline + sanity check that boot reconciliation is actually clearing orphans.

---

## 4. SQL query catalog (Grafana SQLite datasource)

All queries written для grafana-sqlite-datasource. `$__timeFilter()` shown — for raw SQLite, substitute `created_at >= datetime('now', '-1 hour')` чи Grafana variable.

### 4.1 Tier 1 — Prolonged Hold rate (KVT2_CONFIRM_PROLONGED_HOLD)

**Panel**: time-series, "Prolonged holds per minute (Tier 1)".

```sql
SELECT
  strftime('%Y-%m-%dT%H:%M:00Z', created_at) AS time,
  COUNT(*)                                    AS count_emits,
  COUNT(DISTINCT entity_id)                   AS distinct_docs
FROM audit_log
WHERE event_type = 'KVT2_CONFIRM_PROLONGED_HOLD'
  AND created_at >= datetime('now', '-1 hour')
GROUP BY 1
ORDER BY 1 ASC;
```

**Why both columns**: `count_emits` shows tick noise (re-fires per drain tick); `distinct_docs` shows unique-document footprint. Healthy operation: both near zero.

### 4.2 Tier 1 — Top-N FNs з held docs (last 24h)

**Panel**: table.

```sql
SELECT
  fd.fiscal_number,
  COUNT(DISTINCT al.entity_id) AS distinct_docs,
  COUNT(*)                     AS total_emits,
  MAX(al.created_at)           AS last_emit_at
FROM audit_log al
JOIN fiscal_documents fd ON fd.document_id = al.entity_id
WHERE al.event_type = 'KVT2_CONFIRM_PROLONGED_HOLD'
  AND al.created_at >= datetime('now', '-1 day')
GROUP BY fd.fiscal_number
ORDER BY distinct_docs DESC
LIMIT 20;
```

### 4.3 Tier 2 — STOP_MODE escalations (OFFLINE_DRAIN_FN_STOP_MODE)

**Panel**: time-series + stats single value, "STOP_MODE escalations per hour".

```sql
SELECT
  strftime('%Y-%m-%dT%H:00:00Z', created_at) AS time,
  COUNT(*)                                    AS escalations
FROM audit_log
WHERE event_type = 'OFFLINE_DRAIN_FN_STOP_MODE'
  AND created_at >= datetime('now', '-1 day')
GROUP BY 1
ORDER BY 1 ASC;
```

**Threshold**: ANY non-zero count = pages on-call (per § 7.1).

### 4.4 Tier 2 — Currently-in-STOP_MODE FNs (last 24h)

**Panel**: table, refresh 30s.

```sql
SELECT
  json_extract(event_payload_json, '$.fiscal_number') AS fiscal_number,
  json_extract(event_payload_json, '$.document_id')   AS triggering_doc,
  json_extract(event_payload_json, '$.consecutive_holds') AS holds_at_escalation,
  created_at
FROM audit_log
WHERE event_type = 'OFFLINE_DRAIN_FN_STOP_MODE'
  AND created_at >= datetime('now', '-1 day')
ORDER BY created_at DESC;
```

**Operator action**: each row is a candidate for `prro admin doctor` review + potentially `prro admin reset-stop-mode` (per `docs/operations/admin-runbook.md`).

### 4.5 Tier 3 — Manual admin resets (ADMIN_STOP_MODE_RESET)

**Panel**: table з `reason` column visible.

```sql
SELECT
  entity_id                                              AS fiscal_number,
  json_extract(event_payload_json, '$.reason')           AS reason,
  json_extract(event_payload_json, '$.docs_reset_count') AS docs_reset,
  created_at
FROM audit_log
WHERE event_type = 'ADMIN_STOP_MODE_RESET'
  AND created_at >= datetime('now', '-7 days')
ORDER BY created_at DESC;
```

**Why 7-day window**: Tier 3 resets are rare (per `feedback_manual_recon_catastrophe` — operator-pinned "ЧП из ЧП" baseline). Weekly window keeps full incident memory available на panel.

### 4.6 Tier 3 — Reset rate sanity (alert on excessive use)

**Panel**: stats single value, "Admin resets per 24h".

```sql
SELECT COUNT(*) AS resets_24h
FROM audit_log
WHERE event_type = 'ADMIN_STOP_MODE_RESET'
  AND created_at >= datetime('now', '-1 day');
```

**Threshold**: >2 per 24h = anomaly investigation (single-digit per-day expected only during pilot stabilization; production should stay near-zero).

### 4.7 Forensic — Boot orphan rate (TRANSPORT_TRACE_ORPHAN_CLOSED)

**Panel**: time-series, "Orphan traces closed per boot (per day)".

```sql
SELECT
  strftime('%Y-%m-%d', created_at) AS time,
  COUNT(*)                          AS orphan_count
FROM audit_log
WHERE event_type = 'TRANSPORT_TRACE_ORPHAN_CLOSED'
  AND created_at >= datetime('now', '-30 days')
GROUP BY 1
ORDER BY 1 ASC;
```

**Interpretation**: each event = one transport_trace что went orphan через crash. Healthy ops baseline = 0/day. Spike = container restart loop / OOM / panic flushing in-flight DPS calls. Cross-correlate з node restart logs.

### 4.8 Forensic — Orphan TTL distribution

**Panel**: histogram (Grafana panel type).

```sql
SELECT
  CAST(json_extract(event_payload_json, '$.ttl_secs') AS INTEGER) AS ttl_secs,
  COUNT(*) AS occurrences
FROM audit_log
WHERE event_type = 'TRANSPORT_TRACE_ORPHAN_CLOSED'
  AND created_at >= datetime('now', '-30 days')
GROUP BY ttl_secs
ORDER BY ttl_secs ASC;
```

### 4.9 W2 — Operator key-load failures (last 24h)

**Panel**: stat + table, "Boot-time operator load failures (24h)".

```sql
SELECT
  entity_id AS fiscal_number,
  json_extract(event_payload_json, '$.reason') AS reason,
  json_extract(event_payload_json, '$.key_path') AS key_path,
  created_at
FROM audit_log
WHERE entity_type = 'operator'
  AND event_type = 'OPERATOR_KEY_LOAD_FAILED'
  AND created_at >= datetime('now', '-1 day')
ORDER BY created_at DESC;
```

**Interpretation**: every row = one FN that started boot without a usable cashier key.  `reason ∈ {FileNotFound, WrongPassword, EmptyEncoded, Other}`.  Each FN listed here is currently 503-ing every fiscal request (no `BindingsRegistry` entry).  Runbook §6a recovery scenarios match each `reason`.

### 4.10 W2 — Orphan operator rows (cross-DB FK violations)

**Panel**: stat + table, "Orphan operator rows (HIGH-PR90-01)".

```sql
SELECT
  entity_id AS fiscal_number,
  json_extract(event_payload_json, '$.operator_id') AS operator_id,
  json_extract(event_payload_json, '$.key_path') AS key_path,
  created_at
FROM audit_log
WHERE entity_type = 'operator'
  AND event_type = 'OPERATOR_ORPHAN_FN'
  AND created_at >= datetime('now', '-30 days')
ORDER BY created_at DESC;
```

**Interpretation**: every row = an `operators.fiscal_number` that has no matching `fiscal_number_config` row.  Caused by either (a) `fiscal_number_config` row deleted post-registration, or (b) operator typo at registration that PR-B's CLI pre-check missed (shouldn't happen but defence-in-depth).

### 4.11 W2 — Configured FNs without operator registration

**Panel**: stat, "Configured FNs without active cashier".

```sql
SELECT COUNT(DISTINCT entity_id) AS unregistered_fn_count
FROM audit_log
WHERE entity_type = 'operator'
  AND event_type = 'OPERATOR_NOT_REGISTERED'
  AND created_at >= datetime('now', '-1 hour');
```

**Interpretation**: each emit = one boot tick where a configured FN had no operators row.  Steady-state non-zero indicates an FN configured in `fiscal_number_config` is unusable until `prro admin add-operator` lands the registration.  Info severity — operator-awareness only.

### 4.12 W2 — Operator registration trail (forensic)

**Panel**: table, "Recent operator registrations (30d)".

```sql
SELECT
  created_at,
  entity_id AS fiscal_number,
  json_extract(event_payload_json, '$.operator_id') AS operator_id,
  json_extract(event_payload_json, '$.name')        AS cashier_name,
  json_extract(event_payload_json, '$.key_path')    AS key_path
FROM audit_log
WHERE entity_type = 'operator'
  AND event_type = 'ADMIN_OPERATOR_REGISTERED'
  AND created_at >= datetime('now', '-30 days')
ORDER BY created_at DESC;
```

**Interpretation**: every `prro admin add-operator` invocation lands a row here.  Used for cashier-rotation audit + reconciliation of "add-operator attempted but operators table unexpectedly empty" failure-mode (see atomicity discipline in `admin.rs::add_operator` — audit precedes INSERT so a partial failure leaves the audit trail).

**⚠ PII NOTE**: panels 4.9 / 4.10 / 4.12 expose `operator_id` (cashier INN) and `key_path` (filesystem path that may contain cashier names) to anyone who can read this datasource.  See §9 "Out of scope" for the operational mandate on Grafana datasource access control.

---

## 5. Composite dashboard layout

Recommended 6-panel single-screen layout для on-call rotation:

```
+---------------------------------------------------+
| Row 1 (alerts)                                    |
|  [Stat: Tier 3 resets/24h]  [Stat: STOP_MODE       |
|                              escalations/24h]      |
+---------------------------------------------------+
| Row 2 (Tier 2 detail)                              |
|  [Table: Currently-in-STOP_MODE FNs (last 24h)]    |
+---------------------------------------------------+
| Row 3 (Tier 1 rate)                                |
|  [Time-series: Prolonged holds per minute]         |
|  [Table: Top-N held FNs (24h)]                     |
+---------------------------------------------------+
| Row 4 (forensic)                                   |
|  [Time-series: Orphan traces per boot (30d)]       |
|  [Table: Recent Tier 3 resets (7d, з reason)]      |
+---------------------------------------------------+
```

---

## 6. Cross-event correlation (entity_type routing)

Three distinct entity_type values спричиняють need for explicit join discipline:

| event_type | entity_type | entity_id | Join to fiscal_number |
|---|---|---|---|
| KVT2_CONFIRM_PROLONGED_HOLD | `fiscal_document` | doc_id hex | `JOIN fiscal_documents ON fd.document_id = al.entity_id` |
| OFFLINE_DRAIN_FN_STOP_MODE | `fiscal_document` | doc_id hex | direct: `json_extract(event_payload_json, '$.fiscal_number')` OR same join as above |
| ADMIN_STOP_MODE_RESET | `fn` | fiscal_number | direct: `entity_id` = fiscal_number |
| TRANSPORT_TRACE_ORPHAN_CLOSED | `transport_trace` | doc_id hex | `JOIN fiscal_documents ON fd.document_id = al.entity_id` |
| OPERATOR_KEY_LOAD_FAILED | `operator` | fiscal_number | direct: `entity_id` = fiscal_number |
| OPERATOR_ORPHAN_FN | `operator` | fiscal_number | direct: `entity_id` = fiscal_number (BUT FN is missing from fiscal_number_config by definition — no join target on main side) |
| OPERATOR_NOT_REGISTERED | `operator` | fiscal_number | direct: `entity_id` = fiscal_number |
| ADMIN_OPERATOR_REGISTERED | `operator` | fiscal_number | direct: `entity_id` = fiscal_number |

**Naming inconsistency note**: `entity_type = "fn"` для admin resets vs `"fiscal_document"` для drain Tier 1/2 vs `"operator"` для W2 boot-time events is a real asymmetry в codebase. NOT recommended to retrofit ("fn" was the choice операторської фактично-FN-scoped action; "fiscal_document" reflects drain emission's per-doc-loop origin). Dashboards must use the column tags explicitly per table above.

---

## 7. Alerting rules (recommended)

### 7.1 Page on-call (P1)

- `ADMIN_STOP_MODE_RESET` count > 0 in last 60min → page (operator should be aware of every reset).
- `OFFLINE_DRAIN_FN_STOP_MODE` count > 0 in last 5min → page (Tier 2 = critical FN suspension).
- **W2**: `OPERATOR_KEY_LOAD_FAILED` count > 0 in last 5min → page (FN cannot sign — every customer request 503s; operator runbook §6a recovery is required).
- **W2**: `OPERATOR_ORPHAN_FN` count > 0 in last 60min → page (HIGH-PR90-01 cross-DB FK violation; either main-DB drift or operator typo at registration; runbook §6a recovery — DELETE operator row + re-register OR add missing `fiscal_number_config` row).

### 7.2 Notify on-call (P2, no page)

- `KVT2_CONFIRM_PROLONGED_HOLD` distinct_docs > 5 in last 30min on single FN → notify (Tier 1 escalation candidate).
- `TRANSPORT_TRACE_ORPHAN_CLOSED` > 0 in last 60min after a fresh boot → notify (verifies recovery worked).
- **W2**: `OPERATOR_NOT_REGISTERED` count for a given FN > 0 sustained over multiple boots → notify (configured FN has no active cashier; operator should run `prro admin add-operator` per runbook §6a or accept the FN-deregistered state).
- **W2**: `ADMIN_OPERATOR_REGISTERED` count > 0 → notify with payload (informational forensic trail; legitimate during cashier onboarding / rotation, anomalous outside operational windows).

### 7.3 Slow-burn anomaly (P3, daily digest)

- `ADMIN_STOP_MODE_RESET` weekly count trend up — investigate whether Tier 2 thresholds need re-tuning per `docs/superpowers/specs/2026-05-25-w12-tiered-hold-degradation.md` §6.
- `TRANSPORT_TRACE_ORPHAN_CLOSED` 30-day count trend up — investigate container stability / OOM kills.

---

## 8. Recommended schema migration (perf — pre-pilot wiring)

Existing `ix_audit_entity(entity_type, entity_id, audit_id DESC)` is **insufficient** для §4 dashboard queries що time-window across all entities of one event_type. SQLite EXPLAIN shows full-scan для:

```sql
WHERE event_type = '...' AND created_at >= datetime('now', '-1 hour')
```

**Recommended addition** (new migration, e.g. `020_audit_log_event_time_index.sql`):

```sql
-- Composite index для dashboard time-window queries.
-- Covers all §4 panels що filter (event_type, created_at).
CREATE INDEX IF NOT EXISTS ix_audit_event_time
ON audit_log(event_type, created_at DESC);
```

**Cost analysis**:
- Disk: ~24 bytes/row × audit_log row count.  Pilot scale (estimated <10M rows/year) = ~240MB worst-case, negligible.
- Write amplification: 1 extra B-tree insert per audit row.  Audit emit is already off the hot path (no in-tx network calls per I1), so write-rate is bounded by drain tick rate (~1/sec/FN ceiling) → measurable overhead = nanoseconds.
- Read benefit: queries §4.1/4.3/4.5/4.7 become index-only scans.

**Status**: NOT yet landed.  Recommended pre-pilot. Owner: gateway-side (small migration PR, can be authored next).

---

## 9. Out of scope (documented для clarity)

- **Prometheus exporter sidecar** — Option B § 2, post-pilot M3+ scope.
- **Long-term retention** — audit_log в SQLite grows unbounded; post-pilot need archival policy (rollover-to-parquet або similar). NOT in pilot scope; SQLite handles years of audit data fine for pilot scale.
- **PII redaction** — `reason` field в ADMIN_STOP_MODE_RESET може містити operator notes. Pilot operator scope = trusted (per `feedback_autonomous_isolated_env`); production multi-tenant deployment would need redaction.
- **W2 operator-event PII access control — OPERATIONAL MANDATE (NOT optional for production).** Panels §4.9 / §4.10 / §4.12 expose `operator_id` (cashier INN — Personal Identifiable Information under Ukrainian privacy law / GDPR) and `key_path` (filesystem path that may carry cashier names). The Grafana SQLite datasource that backs these panels reads `var/prro.db` directly; any user with read access to that datasource sees the PII. **Required before production deployment**:
    1. EITHER restrict the Grafana datasource to a subset of Grafana users via Grafana RBAC + folder permissions (panels §4.9–§4.12 live in a "forensic / operator-PII" folder accessible only to compliance/security roles);
    2. OR introduce a `audit_log_public` SQL view that redacts `operator_id` (hash with a salt held outside the datasource) and `key_path` (strip to basename / null), and point Grafana at the view only;
    3. OR (multi-tenant only) move the operator-event payloads to a dedicated audit DB with its own ACL — out of pilot scope; deferred design.
  Pilot scope (single-tenant, single-operator self-hosted per `feedback_autonomous_isolated_env`) accepts unredacted access because the dashboard reader IS the operator who registered the cashiers. The mandate above MUST be acted on BEFORE the gateway is exposed to any reader who is not the registering operator.
- **Per-operator-tenant slicing** — single-tenant pilot; multi-tenant slicing deferred.

---

## 10. Verification before "wired"

To consider this dashboard wiring complete:

1. ☐ Grafana SQLite datasource configured against read-only mount of `var/prro.db`.
2. ☐ 6 panels imported per §5 layout.
3. ☐ Index `ix_audit_event_time` landed via migration 020 (§8).
4. ☐ Alerting rules §7.1/7.2/7.3 wired в alerting provider (Grafana Alerting / Alertmanager / on-call PagerDuty integration).
5. ☐ On-call rotation has read this spec + `docs/operations/admin-runbook.md`.
6. ☐ Smoke test: trigger one of each event у dev environment, confirm panel reflects it within 30s refresh window.

---

## 11. Open questions для оператора

1. Confirm Grafana platform of record (Grafana OSS / Grafana Cloud / something else) — affects alerting wiring choice.
2. Confirm Slack/PagerDuty/email routing для §7.1 P1 alerts.
3. Confirm retention policy preference — keep audit_log indefinitely в SQLite до post-pilot, OR start parquet archival з day-one?
4. Approve schema migration `020_audit_log_event_time_index.sql` (§8) для pre-pilot landing? — small, pure additive, no data migration.
