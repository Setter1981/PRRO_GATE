-- Sprint 13 — cert_watch global configuration (single row).
--
-- Single-row table (enforced by PK CHECK (id = 1)) holding the polling
-- cadence and timeout knobs for the certificate-status monitoring
-- service. Disabled by default: operators opt in once their per-CA
-- configuration is known.
CREATE TABLE cert_watch_config (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    enabled                   BOOLEAN NOT NULL DEFAULT 0,
    polling_interval_seconds  INTEGER NOT NULL DEFAULT 1800,
    request_timeout_seconds   INTEGER NOT NULL DEFAULT 5,
    ocsp_url_override         TEXT,
    fallback_to_crl           BOOLEAN NOT NULL DEFAULT 1,
    updated_at                TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO cert_watch_config (id) VALUES (1);
