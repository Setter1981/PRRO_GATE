-- 006 — CA endpoint registry for cert_provisioning multi-URL retry.
--
-- M2/W2 vendors the legacy `sql/016_ca_endpoints.sql` schema (Python
-- branch) into the Rust migration tree.  The IIT-proprietary CMP-look-
-- alike wire protocol used by every listed endpoint is identical across
-- hosts; only the URL differs.  cert_refresher uses this table as the
-- authoritative source of priority-ordered, enabled-only CMP URLs.
--
-- The legacy `cert_provisioning_config.primary_cmp_url` /
-- `fallback_cmp_url` columns are kept (M1 schema, no breaking change)
-- but become deprecated/unused for M2 routing.  Removal is an M3+
-- schema-hygiene follow-up.
--
-- For the per-URL CMP request timeout, M2 reuses the EXISTING
-- `cert_provisioning_config.timeout_seconds` column (default 10s) —
-- no new column is added.

CREATE TABLE ca_endpoints (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL UNIQUE,
    cmp_url         TEXT    NOT NULL,
    issuer_pattern  TEXT,           -- case-insensitive substring vs cert issuer DN; nullable
    priority        INTEGER NOT NULL DEFAULT 0,
    enabled         INTEGER NOT NULL DEFAULT 1  CHECK (enabled IN (0,1)),
    created_at      TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at      TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE INDEX ix_ca_endpoints_priority ON ca_endpoints(priority) WHERE enabled = 1;

-- Seed the two production CMP URLs WITH the /services/cmp/ path
-- component the IIT CMP wire client expects (per
-- `prro_crypto::cms::cmp::fetch_cert_by_ski`).  M1's default
-- `primary_cmp_url='http://acskidd.gov.ua:80'` lacks this path and is
-- incomplete for a direct CMP request — that is why W2 routes through
-- ca_endpoints instead.
INSERT INTO ca_endpoints (name, cmp_url, issuer_pattern, priority) VALUES
    ('acskidd', 'http://acskidd.gov.ua:80/services/cmp/', 'acskidd', 10),
    ('ca.tax.gov.ua', 'http://ca.tax.gov.ua:80/services/cmp/', 'tax', 20);
