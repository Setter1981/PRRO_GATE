-- Sprint 14 — operator signing-cert provisioning cache.
--
-- Persistent store of resolved end-entity signing certificates, keyed by
-- fiscal_number. Populated at onboarding either from the container itself
-- (JKS/PFX typically carry the chain) or, if the container has no cert,
-- via a CMP round-trip to the Ukrainian CA front (acskidd.gov.ua by
-- default). Feeds the signing path without asking operators to manually
-- upload a .cer file.
--
-- `source` records how this cert reached us:
--   'container'  — embedded in the operator's key container (JKS chain, PFX)
--   'cmp'        — fetched via CMP lookup-by-SKI
--   'manual'     — admin-uploaded cert (verified to match the stored SKI)
--
-- Idempotent on fiscal_number (FI-4): repeated provisioning with the
-- same key material collapses to a single row.
CREATE TABLE operator_certs (
    fiscal_number      TEXT PRIMARY KEY,
    cert_fingerprint   TEXT NOT NULL,           -- SHA-256 hex over cert DER
    ski_hex            TEXT NOT NULL,           -- 64 chars upper hex
    cert_der           BLOB NOT NULL,
    subject_dn         TEXT,
    issuer_dn          TEXT,
    valid_from         TIMESTAMP,
    valid_to           TIMESTAMP,
    fetched_at         TIMESTAMP NOT NULL,
    source             TEXT NOT NULL CHECK (source IN ('container','cmp','manual')),
    last_refresh_at    TIMESTAMP
);
CREATE UNIQUE INDEX ix_operator_certs_ski ON operator_certs(ski_hex);

-- Sprint 14 — cert provisioning runtime configuration (single row).
-- Holds the CMP endpoint(s) and timeouts. fallback_cmp_url is a stub
-- for future multi-endpoint support (WebCheck `OtherAddresses=`); left
-- empty on fresh install — one endpoint covers the whole Ukrainian CA
-- federation today.
CREATE TABLE cert_provisioning_config (
    id                       INTEGER PRIMARY KEY CHECK (id = 1),
    primary_cmp_url          TEXT NOT NULL DEFAULT 'http://acskidd.gov.ua:80',
    fallback_cmp_url         TEXT,
    timeout_seconds          INTEGER NOT NULL DEFAULT 10,
    cache_ttl_seconds        INTEGER NOT NULL DEFAULT 3600,
    refresh_within_days      INTEGER NOT NULL DEFAULT 30,
    updated_at               TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO cert_provisioning_config (id) VALUES (1);
