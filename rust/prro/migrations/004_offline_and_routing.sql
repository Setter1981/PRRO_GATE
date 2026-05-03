-- 004 — offline + routing.

CREATE TABLE offline_sessions (
    offline_session_id BLOB    PRIMARY KEY  CHECK (length(offline_session_id) = 16),
    fiscal_number      TEXT    NOT NULL,
    status             TEXT    NOT NULL  CHECK (status IN ('OPENING','OPEN','CLOSING','CLOSED','ABORTED')),
    opened_at          TEXT    NOT NULL,
    closed_at          TEXT,
    last_known_unsigned_xml_sha256 BLOB,
    docs_count         INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE INDEX ix_offline_active ON offline_sessions(fiscal_number, status)
    WHERE status IN ('OPENING','OPEN','CLOSING');

CREATE TABLE offline_codes (
    fiscal_number TEXT    NOT NULL,
    code_value    INTEGER NOT NULL,
    used_at       TEXT,
    used_by_doc   BLOB,
    PRIMARY KEY (fiscal_number, code_value),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;
CREATE INDEX ix_offline_codes_unused ON offline_codes(fiscal_number) WHERE used_at IS NULL;

CREATE TABLE backend_profiles (
    backend_profile_id TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL  CHECK (kind IN ('DPS_PRRO','CHECKBOX','OTHER')),
    config_json        TEXT,
    created_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE TABLE transport_profiles (
    transport_profile_id TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    channel_kind         TEXT NOT NULL  CHECK (channel_kind IN ('grpc_cabinet','edyne_vikno','soap_dps','checkbox_rest','sidecar_v2')),
    test_mode            INTEGER NOT NULL DEFAULT 0  CHECK (test_mode IN (0,1)),
    config_json          TEXT,
    created_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE TABLE prro_bindings (
    fiscal_number        TEXT PRIMARY KEY,
    backend_profile_id   TEXT NOT NULL,
    transport_profile_id TEXT NOT NULL,
    created_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number)        REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (backend_profile_id)   REFERENCES backend_profiles(backend_profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id) ON DELETE RESTRICT
) STRICT;
