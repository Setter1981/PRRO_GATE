-- 001 — core identity tables.  Per spec §4.
--
-- NB: connection-level pragmas (journal_mode, foreign_keys, busy_timeout,
-- synchronous) live in `db::open_pool` via SqliteConnectOptions — they cannot
-- be set inside a transaction, and sqlx wraps each migration in one.

CREATE TABLE fiscal_number_config (
    -- All-digit checks use `NOT GLOB '*[^0-9]*'` because GLOB '[0-9]*'
    -- only constrains the FIRST character ('*' = anything-after).  See
    -- migrations_apply::* tests for behavioural proof.
    fiscal_number          TEXT    PRIMARY KEY  CHECK (length(fiscal_number) = 10 AND NOT fiscal_number GLOB '*[^0-9]*'),
    tax_number             TEXT    NOT NULL,
    vat_payer_inn          TEXT    CHECK (vat_payer_inn IS NULL OR (length(vat_payer_inn) = 12 AND NOT vat_payer_inn GLOB '*[^0-9]*')),
    fiscal_mode            TEXT    NOT NULL  CHECK (fiscal_mode IN ('test','prod')),
    org_name               TEXT,
    point_name             TEXT,
    org_address            TEXT,
    tsp_enabled            INTEGER NOT NULL DEFAULT 0  CHECK (tsp_enabled IN (0,1)),
    offline_enabled        INTEGER NOT NULL DEFAULT 1  CHECK (offline_enabled IN (0,1)),
    national_check_enabled INTEGER NOT NULL DEFAULT 0  CHECK (national_check_enabled IN (0,1)),
    min_offline_codes      INTEGER NOT NULL DEFAULT 0,
    max_offline_codes      INTEGER NOT NULL DEFAULT 0,
    created_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE TRIGGER fnc_updated_at
AFTER UPDATE ON fiscal_number_config
BEGIN
    UPDATE fiscal_number_config SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;
END;

CREATE TABLE shifts (
    shift_id               BLOB    PRIMARY KEY  CHECK (length(shift_id) = 16),
    fiscal_number          TEXT    NOT NULL,
    serial                 INTEGER,
    state                  TEXT    NOT NULL  CHECK (state IN ('CREATED','OPENING','OPENED','CLOSING','CLOSED','ERROR')),
    open_mode              TEXT    NOT NULL  CHECK (open_mode IN ('ONLINE','OFFLINE')),
    opened_at              TEXT,
    closed_at              TEXT,
    open_document_id       BLOB,
    close_document_id      BLOB,
    z_report_document_id   BLOB,
    cash_balance_kop       INTEGER NOT NULL DEFAULT 0,
    created_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE INDEX ix_shifts_fn_state ON shifts(fiscal_number, state);

CREATE TRIGGER shifts_updated_at
AFTER UPDATE ON shifts
BEGIN
    UPDATE shifts SET updated_at = CURRENT_TIMESTAMP WHERE shift_id = NEW.shift_id;
END;

CREATE TABLE node_state (
    fiscal_number               TEXT    PRIMARY KEY,
    mode                        TEXT    NOT NULL  CHECK (mode IN ('ONLINE','GOING_OFFLINE','OFFLINE','GOING_ONLINE','BLOCKED','STOP_MODE','CRYPTO_DEGRADED')),
    shift_state                 TEXT    NOT NULL  CHECK (shift_state IN ('CREATED','OPENING','OPENED','CLOSING','CLOSED','ERROR')),
    current_shift_id            BLOB,
    current_offline_session_id  BLOB,
    next_lnd                    INTEGER NOT NULL  CHECK (next_lnd >= 1),
    backend_profile_id          TEXT,
    transport_profile_id        TEXT,
    readiness_state             TEXT    NOT NULL DEFAULT 'STARTING'  CHECK (readiness_state IN ('STARTING','RECOVERING','READY','DEGRADED','STOPPED')),
    recovery_stage              TEXT    NOT NULL DEFAULT 'BOOT'  CHECK (recovery_stage IN ('BOOT','PHASE1','PHASE2','DONE','FAILED')),
    current_month_offline_seconds INTEGER NOT NULL DEFAULT 0,
    last_known_unsigned_xml_sha256 BLOB  CHECK (last_known_unsigned_xml_sha256 IS NULL OR length(last_known_unsigned_xml_sha256) = 32),
    last_fs_ping_at             TEXT,
    updated_at                  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER node_state_updated_at
AFTER UPDATE ON node_state
BEGIN
    UPDATE node_state SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;
END;

-- audit_log carries no FK (append-only, can outlive entities)
CREATE TABLE audit_log (
    audit_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type        TEXT    NOT NULL,
    entity_id          TEXT    NOT NULL,
    event_type         TEXT    NOT NULL,
    severity           TEXT    NOT NULL  CHECK (severity IN ('INFO','WARNING','ERROR','CRITICAL')),
    actor              TEXT,
    event_payload_json TEXT,
    created_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

CREATE INDEX ix_audit_entity ON audit_log(entity_type, entity_id, created_at);
