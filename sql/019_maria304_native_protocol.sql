-- sql/019_maria304_native_protocol.sql
-- Add MARIA_304_NATIVE to protocol CHECK on ingress_inbox and protocol_trace_log.
--
-- Background: the Maria 304 Rust fiscal driver (prro_crypto_v2 / plan 2026-04-21, M7-Py-2)
-- introduces a new ingress protocol value 'MARIA_304_NATIVE'. SQLite does not support
-- ALTER TABLE ... DROP/MODIFY CONSTRAINT, so both tables are rebuilt using the same
-- transactional rename pattern established in 003_offline_local_ack_state.sql.
--
-- FK direction:
--   fiscal_documents.request_id → ingress_inbox(request_id)
--   No FK references protocol_trace_log from any other table.
-- PRAGMA foreign_keys = OFF is required for the ingress_inbox rebuild so that
-- fiscal_documents does not block DROP TABLE. FK enforcement is re-enabled
-- automatically on the next connection (runner does not carry PRAGMA state across
-- files, per 018_sidecar_transport_profile.sql commentary).
--
-- No data mutation: INSERT ... SELECT * copies all rows unchanged.
-- Idempotency: the migration runner guards re-application via schema_migrations;
-- the DDL itself uses non-colliding _new suffix names whose existence would
-- fail loudly if the transaction were somehow partially applied.

PRAGMA foreign_keys = OFF;

BEGIN;

-- ── ingress_inbox rebuild ────────────────────────────────────────────────────
-- Full column list from 001_hot_store_init.sql lines 95-125 (25 columns).
-- Only change: protocol CHECK gains 'MARIA_304_NATIVE'.

CREATE TABLE ingress_inbox_new (
    request_id                      TEXT PRIMARY KEY,
    idempotency_key                 TEXT NOT NULL UNIQUE,
    protocol                        TEXT NOT NULL CHECK (protocol IN (
                                        'CHECKBOX_REST',
                                        'WEBCHECK_XMLRPC',
                                        'MARIA_TCP',
                                        'MARIA_304_NATIVE',
                                        'INTERNAL'
                                    )),
    operation_type                  TEXT NOT NULL CHECK (operation_type IN (
        'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
        'CASH_WITHDRAWAL','X_REPORT','Z_REPORT','GO_OFFLINE','GO_ONLINE',
        'ASK_OFFLINE_CODES','GET_STATUS','PING','CUSTOM_PROTOCOL_METHOD'
    )),
    fiscal_number                   TEXT NOT NULL,
    route_key                       TEXT,
    backend_profile_id              TEXT,
    transport_profile_id            TEXT,
    channel_owner                   TEXT,
    protocol_session_id             TEXT,
    external_request_id             TEXT,
    payload_json                    TEXT NOT NULL,
    payload_sha256                  TEXT NOT NULL,
    status                          TEXT NOT NULL DEFAULT 'NEW' CHECK (status IN ('NEW','PROCESSING','DONE','ERROR','DEAD')),
    requeue_count                   INTEGER NOT NULL DEFAULT 0 CHECK (requeue_count >= 0),
    lease_owner                     TEXT,
    lease_token                     TEXT,
    lease_expires_at                TEXT,
    response_deadline_at            TEXT,
    result_document_id              TEXT,
    canonical_error_code            TEXT,
    error_message                   TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at                      TEXT,
    finished_at                     TEXT
);

INSERT INTO ingress_inbox_new SELECT * FROM ingress_inbox;

DROP TABLE ingress_inbox;

ALTER TABLE ingress_inbox_new RENAME TO ingress_inbox;

-- Recreate all three indexes from 001_hot_store_init.sql lines 127-134.
CREATE INDEX idx_ingress_inbox_status_created
ON ingress_inbox(status, created_at);

CREATE INDEX idx_ingress_inbox_fiscal_status
ON ingress_inbox(fiscal_number, status, created_at);

CREATE INDEX idx_ingress_inbox_lease
ON ingress_inbox(status, lease_expires_at);

-- ── protocol_trace_log rebuild ───────────────────────────────────────────────
-- Full column list from 001_hot_store_init.sql lines 400-423 (20 columns, no FK refs).
-- Only change: protocol CHECK gains 'MARIA_304_NATIVE'.

CREATE TABLE protocol_trace_log_new (
    trace_id                        TEXT PRIMARY KEY,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    protocol                        TEXT NOT NULL CHECK (protocol IN (
                                        'CHECKBOX_REST',
                                        'WEBCHECK_XMLRPC',
                                        'MARIA_TCP',
                                        'MARIA_304_NATIVE',
                                        'INTERNAL'
                                    )),
    fiscal_number                   TEXT,
    route_key                       TEXT,
    connection_id                   TEXT,
    session_id                      TEXT,
    direction                       TEXT NOT NULL CHECK (direction IN ('IN','OUT','EVENT')),
    method_name                     TEXT,
    command_code                    TEXT,
    trace_level                     TEXT NOT NULL CHECK (trace_level IN ('OFF','SAFE','FULL_TRACE')),
    raw_payload                     BLOB,
    raw_payload_hex                 TEXT,
    parsed_payload_json             TEXT,
    mapped_operation                TEXT,
    canonical_request_id            TEXT,
    result_code                     TEXT,
    error_class                     TEXT,
    error_message                   TEXT,
    duration_ms                     INTEGER,
    source_ip                       TEXT,
    source_port                     INTEGER
);

INSERT INTO protocol_trace_log_new SELECT * FROM protocol_trace_log;

DROP TABLE protocol_trace_log;

ALTER TABLE protocol_trace_log_new RENAME TO protocol_trace_log;

-- Recreate both indexes from 001_hot_store_init.sql lines 425-429.
CREATE INDEX idx_protocol_trace_time
ON protocol_trace_log(created_at);

CREATE INDEX idx_protocol_trace_fiscal
ON protocol_trace_log(fiscal_number, created_at);

COMMIT;
