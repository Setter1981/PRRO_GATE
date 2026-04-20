-- sql/018_sidecar_transport_profile.sql
-- Add DPS_PRRO_FISCAL_SIDECAR_V2 to transport_profiles.kind CHECK constraint.
--
-- SQLite does not support ALTER TABLE MODIFY CONSTRAINT — requires table rebuild.
-- The migration runner executes ALL PRAGMA statements from one file before BEGIN IMMEDIATE.
-- To keep foreign_keys OFF during DROP TABLE, we must not include PRAGMA foreign_keys=ON
-- in this same file (it would execute after OFF, re-enabling it before body runs).
-- FK enforcement is restored automatically on the next connection (or next migration).

PRAGMA foreign_keys=OFF;

BEGIN;

CREATE TABLE transport_profiles_new (
    transport_profile_id  TEXT PRIMARY KEY,
    kind                  TEXT NOT NULL CHECK (kind IN (
                              'CHECKBOX_REST_TRANSPORT',
                              'DPS_PRRO_GRPC_ECABINET',
                              'DPS_PRRO_XML_UNIFIED_WINDOW',
                              'CUSTOM_TRANSPORT',
                              'DPS_PRRO_FISCAL_SIDECAR_V2'
                          )),
    name                  TEXT NOT NULL,
    endpoint              TEXT,
    tls_policy            TEXT,
    timeout_config_json   TEXT NOT NULL,
    retry_policy_json     TEXT NOT NULL,
    config_json           TEXT NOT NULL,
    is_active             INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
    created_at            TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at            TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO transport_profiles_new SELECT * FROM transport_profiles;

DROP TABLE transport_profiles;

ALTER TABLE transport_profiles_new RENAME TO transport_profiles;

-- Seed: reference transport profile for prro_sidecar (local HTTP).
INSERT OR IGNORE INTO transport_profiles (
    transport_profile_id, kind, name, endpoint,
    tls_policy, timeout_config_json, retry_policy_json, config_json, is_active
) VALUES (
    'transport_sidecar_v2_default',
    'DPS_PRRO_FISCAL_SIDECAR_V2',
    'Fiscal Sidecar V2 (local)',
    'http://127.0.0.1:8765',
    'none',
    '{"connect_timeout":5,"request_timeout":120}',
    '{"max_retries":3,"retry_backoff_base":1,"strategy":"exponential"}',
    '{"notes":"Rust prro_sidecar HTTP fiscal driver"}',
    1
);

COMMIT;
