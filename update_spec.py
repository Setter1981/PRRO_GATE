import re

path = 'docs/superpowers/specs/2026-05-25-w12-audit-dashboard-spec.md'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Update intro text for timeFilter
content = content.replace(
    '''All queries written для grafana-sqlite-datasource. () shown — for raw SQLite, substitute created_at >= datetime('now', '-1 hour') чи Grafana variable.''',
    '''All queries written для grafana-sqlite-datasource. () macro is used for time bounds. For raw SQLite testing outside Grafana, substitute (created_at) with e.g. created_at >= datetime('now', '-1 hour').'''
)

# 2. Update queries to use  and UNIX timestamps
content = content.replace(
    '''SELECT
  strftime('%Y-%m-%dT%H:%M:00Z', created_at) AS time,
  COUNT(*)                                    AS count_emits,
  COUNT(DISTINCT entity_id)                   AS distinct_docs
FROM audit_log
WHERE event_type = 'KVT2_CONFIRM_PROLONGED_HOLD'
  AND created_at >= datetime('now', '-1 hour')
GROUP BY 1
ORDER BY 1 ASC;''',
    '''SELECT
  CAST(strftime('%s', strftime('%Y-%m-%dT%H:%M:00Z', created_at)) AS INTEGER) AS time,
  COUNT(*)                                    AS count_emits,
  COUNT(DISTINCT entity_id)                   AS distinct_docs
FROM audit_log
WHERE event_type = 'KVT2_CONFIRM_PROLONGED_HOLD'
  AND (created_at)
GROUP BY 1
ORDER BY 1 ASC;'''
)

content = content.replace(
    '''WHERE al.event_type = 'KVT2_CONFIRM_PROLONGED_HOLD'
  AND al.created_at >= datetime('now', '-1 day')''',
    '''WHERE al.event_type = 'KVT2_CONFIRM_PROLONGED_HOLD'
  AND (al.created_at)'''
)

content = content.replace(
    '''SELECT
  strftime('%Y-%m-%dT%H:00:00Z', created_at) AS time,
  COUNT(*)                                    AS escalations
FROM audit_log
WHERE event_type = 'OFFLINE_DRAIN_FN_STOP_MODE'
  AND created_at >= datetime('now', '-1 day')
GROUP BY 1
ORDER BY 1 ASC;''',
    '''SELECT
  CAST(strftime('%s', strftime('%Y-%m-%dT%H:00:00Z', created_at)) AS INTEGER) AS time,
  COUNT(*)                                    AS escalations
FROM audit_log
WHERE event_type = 'OFFLINE_DRAIN_FN_STOP_MODE'
  AND (created_at)
GROUP BY 1
ORDER BY 1 ASC;'''
)

content = content.replace(
    '''WHERE event_type = 'OFFLINE_DRAIN_FN_STOP_MODE'
  AND created_at >= datetime('now', '-1 day')''',
    '''WHERE event_type = 'OFFLINE_DRAIN_FN_STOP_MODE'
  AND (created_at)'''
)

content = content.replace(
    '''WHERE event_type = 'ADMIN_STOP_MODE_RESET'
  AND created_at >= datetime('now', '-7 days')''',
    '''WHERE event_type = 'ADMIN_STOP_MODE_RESET'
  AND (created_at)'''
)

content = content.replace(
    '''WHERE event_type = 'ADMIN_STOP_MODE_RESET'
  AND created_at >= datetime('now', '-1 day');''',
    '''WHERE event_type = 'ADMIN_STOP_MODE_RESET'
  AND (created_at);'''
)

content = content.replace(
    '''SELECT
  strftime('%Y-%m-%d', created_at) AS time,
  COUNT(*)                          AS orphan_count
FROM audit_log
WHERE event_type = 'TRANSPORT_TRACE_ORPHAN_CLOSED'
  AND created_at >= datetime('now', '-30 days')
GROUP BY 1
ORDER BY 1 ASC;''',
    '''SELECT
  CAST(strftime('%s', strftime('%Y-%m-%d', created_at)) AS INTEGER) AS time,
  COUNT(*)                          AS orphan_count
FROM audit_log
WHERE event_type = 'TRANSPORT_TRACE_ORPHAN_CLOSED'
  AND (created_at)
GROUP BY 1
ORDER BY 1 ASC;'''
)

content = content.replace(
    '''### 4.8 Forensic — Orphan TTL distribution

**Panel**: histogram (Grafana panel type).

''',
    '''### 4.8 Forensic — Orphan TTL distribution

**Panel**: Histogram (Grafana native).
*Note: Grafana Histogram panels expect raw data points, not pre-aggregated counts.*


*(If you want to use a Bar Chart panel instead of a Histogram, you can use COUNT(*) with GROUP BY ttl_secs).*

### 4.9 Dashboard Variables (Recommended Addition)

To make the dashboard useful for multi-tenant or multi-FN deployments, add a Dashboard Variable in Grafana:
- **Variable Name:** iscal_number
- **Type:** Query
- **Query:** SELECT DISTINCT fiscal_number FROM fiscal_documents;
- **Include All option:** Yes (Custom all value: %)

Then, update the queries above to filter by FN. For example, for events joined with iscal_documents:
AND fd.fiscal_number LIKE '''''
)

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print(
