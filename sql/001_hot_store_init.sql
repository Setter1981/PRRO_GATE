PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;

-- ============================================================================
-- Multi-Protocol PRRO Gateway - Hot Store DDL v0.2
-- Target: SQLite 3.40+
-- Purpose: Phase 1.0 / MVP hot store for one fiscal_number per database file.
-- Notes:
--   * One database file per fiscal_number is the recommended topology.
--   * current_month_offline_seconds in node_state is the authoritative fast counter.
--   * offline_sessions.accumulated_month_seconds is a persisted snapshot for audit.
-- ============================================================================

BEGIN;

CREATE TABLE schema_registry (
    schema_id            INTEGER PRIMARY KEY AUTOINCREMENT,
    schema_name          TEXT NOT NULL,
    version              TEXT NOT NULL,
    compatible_since     TEXT NOT NULL,
    migration_ref        TEXT,
    applied_at           TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    checksum             TEXT,
    UNIQUE(schema_name, version)
);

CREATE TABLE node_state (
    node_id                         TEXT PRIMARY KEY,
    fiscal_number                   TEXT NOT NULL UNIQUE,
    mode                            TEXT NOT NULL CHECK (mode IN ('ONLINE','GOING_OFFLINE','OFFLINE','GOING_ONLINE','BLOCKED','STOP_MODE','CRYPTO_DEGRADED')),
    shift_state                     TEXT NOT NULL CHECK (shift_state IN ('CREATED','OPENING','OPENED','CLOSING','CLOSED','ERROR')),
    current_shift_id                TEXT,
    current_offline_session_id      TEXT,
    next_lnd                        INTEGER NOT NULL CHECK (next_lnd >= 1),
    current_channel_lock            TEXT,
    current_backend_profile_id      TEXT,
    current_transport_profile_id    TEXT,
    current_integration_owner       TEXT,
    readiness_state                 TEXT NOT NULL DEFAULT 'STARTING' CHECK (readiness_state IN ('STARTING','RECOVERING','READY','DEGRADED','STOPPED')),
    recovery_stage                  TEXT NOT NULL DEFAULT 'BOOT' CHECK (recovery_stage IN ('BOOT','PHASE1','PHASE2','DONE','FAILED')),
    current_month_bucket            TEXT NOT NULL DEFAULT '',
    current_month_offline_seconds   INTEGER NOT NULL DEFAULT 0 CHECK (current_month_offline_seconds >= 0),
    crypto_state_json               TEXT,
    last_fs_ping_at                 TEXT,
    last_integrity_check_at         TEXT,
    last_backup_at                  TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE backend_profiles (
    backend_profile_id              TEXT PRIMARY KEY,
    backend_type                    TEXT NOT NULL CHECK (backend_type IN ('CHECKBOX_CLOUD_COMPAT','LOCAL_PRRO_CORE','DPS_DIRECT','CUSTOM_VENDOR_BACKEND')),
    name                            TEXT NOT NULL,
    capability_flags_json           TEXT NOT NULL DEFAULT '{}',
    config_json                     TEXT NOT NULL,
    is_active                       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE transport_profiles (
    transport_profile_id            TEXT PRIMARY KEY,
    kind                            TEXT NOT NULL CHECK (kind IN ('CHECKBOX_REST_TRANSPORT','DPS_PRRO_GRPC_ECABINET','DPS_PRRO_XML_UNIFIED_WINDOW','CUSTOM_TRANSPORT')),
    name                            TEXT NOT NULL,
    endpoint                        TEXT,
    tls_policy                      TEXT,
    timeout_config_json             TEXT NOT NULL,
    retry_policy_json               TEXT NOT NULL,
    config_json                     TEXT NOT NULL,
    is_active                       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE prro_bindings (
    binding_id                      TEXT PRIMARY KEY,
    fiscal_number                   TEXT NOT NULL,
    route_key                       TEXT,
    backend_profile_id              TEXT NOT NULL,
    transport_profile_id            TEXT NOT NULL,
    is_primary                      INTEGER NOT NULL DEFAULT 1 CHECK (is_primary IN (0,1)),
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (backend_profile_id) REFERENCES backend_profiles(backend_profile_id),
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id)
);

CREATE UNIQUE INDEX uq_prro_bindings_primary
ON prro_bindings(fiscal_number, COALESCE(route_key, ''))
WHERE is_primary = 1;

CREATE TABLE ingress_inbox (
    request_id                      TEXT PRIMARY KEY,
    idempotency_key                 TEXT NOT NULL UNIQUE,
    protocol                        TEXT NOT NULL CHECK (protocol IN ('CHECKBOX_REST','WEBCHECK_XMLRPC','MARIA_TCP','INTERNAL')),
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

CREATE INDEX idx_ingress_inbox_status_created
ON ingress_inbox(status, created_at);

CREATE INDEX idx_ingress_inbox_fiscal_status
ON ingress_inbox(fiscal_number, status, created_at);

CREATE INDEX idx_ingress_inbox_lease
ON ingress_inbox(status, lease_expires_at);

CREATE TABLE shifts (
    shift_id                        TEXT PRIMARY KEY,
    fiscal_number                   TEXT NOT NULL,
    serial                          INTEGER,
    state                           TEXT NOT NULL CHECK (state IN ('CREATED','OPENING','OPENED','CLOSING','CLOSED','ERROR')),
    open_mode                       TEXT NOT NULL CHECK (open_mode IN ('ONLINE','OFFLINE')),
    opened_at                       TEXT,
    closed_at                       TEXT,
    opened_via_backend_profile_id   TEXT NOT NULL,
    opened_via_transport_profile_id TEXT NOT NULL,
    opened_via_protocol             TEXT NOT NULL,
    opened_via_integration_owner    TEXT NOT NULL,
    channel_lock_acquired_at        TEXT NOT NULL,
    open_document_id                TEXT,
    close_document_id               TEXT,
    z_report_document_id            TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (opened_via_backend_profile_id) REFERENCES backend_profiles(backend_profile_id),
    FOREIGN KEY (opened_via_transport_profile_id) REFERENCES transport_profiles(transport_profile_id)
);

CREATE UNIQUE INDEX uq_active_shift_per_fiscal
ON shifts(fiscal_number)
WHERE state IN ('OPENING','OPENED','CLOSING');

CREATE TABLE offline_ranges (
    range_id                        TEXT PRIMARY KEY,
    fiscal_number                   TEXT NOT NULL,
    first_fiscal_no                 INTEGER NOT NULL,
    last_fiscal_no                  INTEGER NOT NULL,
    next_fiscal_no                  INTEGER NOT NULL,
    issued_at                       TEXT NOT NULL,
    status                          TEXT NOT NULL CHECK (status IN ('ACTIVE','EXHAUSTED','REVOKED')),
    source_payload_json             TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (first_fiscal_no <= last_fiscal_no),
    CHECK (next_fiscal_no >= first_fiscal_no),
    CHECK (next_fiscal_no <= last_fiscal_no + 1)
);

CREATE INDEX idx_offline_ranges_active
ON offline_ranges(fiscal_number, status, next_fiscal_no);

CREATE TABLE offline_sessions (
    offline_session_id              TEXT PRIMARY KEY,
    fiscal_number                   TEXT NOT NULL,
    started_at                      TEXT NOT NULL,
    ended_at                        TEXT,
    status                          TEXT NOT NULL CHECK (status IN ('OPENING','OPEN','CLOSING','CLOSED','ABORTED')),
    start_notice_document_id        TEXT,
    finish_notice_document_id       TEXT,
    start_fiscal_no                 INTEGER,
    finish_fiscal_no                INTEGER,
    reason                          TEXT,
    continuous_limit_seconds        INTEGER NOT NULL DEFAULT 129600,
    monthly_limit_seconds           INTEGER NOT NULL DEFAULT 604800,
    accumulated_month_seconds       INTEGER NOT NULL DEFAULT 0,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uq_open_offline_session_per_fiscal
ON offline_sessions(fiscal_number)
WHERE status IN ('OPENING','OPEN','CLOSING');

CREATE TABLE fiscal_documents (
    document_id                     TEXT PRIMARY KEY,
    request_id                      TEXT NOT NULL UNIQUE,
    fiscal_number                   TEXT NOT NULL,
    shift_id                        TEXT,
    offline_session_id              TEXT,
    lnd                             INTEGER NOT NULL CHECK (lnd >= 1),
    doc_type                        TEXT NOT NULL CHECK (doc_type IN (
        'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
        'CASH_WITHDRAWAL','X_REPORT','Z_REPORT','OFFLINE_BEGIN','OFFLINE_END',
        'ASK_OFFLINE_CODES','STATUS'
    )),
    state                           TEXT NOT NULL CHECK (state IN (
        'PREPARED','SIGNED','ENCRYPTED','SENT','KVT1','KVT2','ACK',
        'REJECTED','CANCELLED','ERROR_RETRYABLE','REQUIRES_MANUAL_RECONCILIATION'
    )),
    backend_profile_id              TEXT NOT NULL,
    transport_profile_id            TEXT NOT NULL,
    fs_mode                         TEXT NOT NULL CHECK (fs_mode IN ('ONLINE','OFFLINE')),
    receipt_type                    TEXT,
    business_ts                     TEXT NOT NULL,
    offline_fiscal_no               INTEGER,
    offline_fiscal_date             TEXT,
    server_fiscal_no                TEXT,
    server_fiscal_date              TEXT,
    serial                          TEXT,
    control_number                  TEXT,
    previous_hash                   TEXT,
    related_receipt_id              TEXT,
    previous_receipt_id             TEXT,
    technical_return                INTEGER CHECK (technical_return IN (0,1)),
    delivery_json                   TEXT,
    rounding_enabled                INTEGER CHECK (rounding_enabled IN (0,1)),
    channel_lock_ref                TEXT,
    total_sum                       INTEGER,
    round_sum                       INTEGER,
    discounts_sum                   INTEGER,
    extra_charge_sum                INTEGER,
    payload_json                    TEXT NOT NULL,
    payload_sha256                  TEXT NOT NULL,
    response_json                   TEXT,
    transport_request_id            TEXT,
    submission_status               TEXT,
    kvt1_received_at                TEXT,
    kvt2_received_at                TEXT,
    sent_at                         TEXT,
    ack_at                          TEXT,
    canonical_error_code            TEXT,
    error_message                   TEXT,
    recovery_attempts               INTEGER NOT NULL DEFAULT 0 CHECK (recovery_attempts >= 0),
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (request_id) REFERENCES ingress_inbox(request_id),
    FOREIGN KEY (shift_id) REFERENCES shifts(shift_id),
    FOREIGN KEY (offline_session_id) REFERENCES offline_sessions(offline_session_id),
    FOREIGN KEY (backend_profile_id) REFERENCES backend_profiles(backend_profile_id),
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id)
);

CREATE UNIQUE INDEX uq_fiscal_documents_lnd
ON fiscal_documents(lnd);

CREATE UNIQUE INDEX uq_fiscal_documents_offline_no
ON fiscal_documents(offline_fiscal_no)
WHERE offline_fiscal_no IS NOT NULL;

CREATE INDEX idx_fiscal_documents_state
ON fiscal_documents(fiscal_number, state, created_at);

CREATE INDEX idx_fiscal_documents_previous_hash
ON fiscal_documents(previous_hash);

CREATE INDEX idx_fiscal_documents_shift
ON fiscal_documents(shift_id, created_at);

CREATE TABLE fiscal_document_goods (
    item_id                         TEXT PRIMARY KEY,
    document_id                     TEXT NOT NULL,
    item_no                         INTEGER NOT NULL CHECK (item_no >= 1),
    code                            TEXT,
    good_id                         TEXT,
    name                            TEXT NOT NULL,
    uktzed                          TEXT,
    barcode                         TEXT,
    excise_barcodes_json            TEXT,
    price                           INTEGER NOT NULL,
    quantity                        INTEGER NOT NULL,
    sum                             INTEGER NOT NULL,
    is_return                       INTEGER NOT NULL DEFAULT 0 CHECK (is_return IN (0,1)),
    header                          TEXT,
    footer                          TEXT,
    discounts_json                  TEXT,
    item_attributes_json            TEXT,
    raw_payload_json                TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX uq_fiscal_document_goods_itemno
ON fiscal_document_goods(document_id, item_no);

CREATE INDEX idx_fiscal_document_goods_barcode
ON fiscal_document_goods(barcode);

CREATE TABLE fiscal_document_payments (
    payment_id                      TEXT PRIMARY KEY,
    document_id                     TEXT NOT NULL,
    payment_type                    TEXT NOT NULL CHECK (payment_type IN ('CASH','CASHLESS','MIXED','OTHER')),
    provider_type                   TEXT,
    label                           TEXT,
    payment_code                    TEXT,
    amount                          INTEGER NOT NULL,
    commission                      INTEGER,
    card_mask                       TEXT,
    bank_name                       TEXT,
    auth_code                       TEXT,
    rrn                             TEXT,
    payment_system                  TEXT,
    owner_name                      TEXT,
    terminal                        TEXT,
    acquirer_and_seller             TEXT,
    receipt_no                      TEXT,
    signature_required              INTEGER CHECK (signature_required IN (0,1)),
    tapxphone_terminal              TEXT,
    acquiring_source                TEXT CHECK (acquiring_source IN ('MANUAL_FRONT','ERP','POS_INTEGRATION','BACKEND_ENRICHED')),
    acquiring_payload_json          TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE CASCADE
);

CREATE INDEX idx_fiscal_document_payments_document
ON fiscal_document_payments(document_id);

CREATE INDEX idx_fiscal_document_payments_rrn
ON fiscal_document_payments(rrn);

CREATE TABLE excise_marks (
    excise_mark_id                  TEXT PRIMARY KEY,
    normalized_mark_code            TEXT NOT NULL,
    raw_mark_code                   TEXT NOT NULL,
    mark_type                       TEXT,
    status                          TEXT NOT NULL CHECK (status IN ('RESERVED','SOLD','RETURNED','VOIDED','REJECTED')),
    receipt_document_id             TEXT,
    receipt_item_no                 INTEGER,
    product_code                    TEXT,
    sold_at                         TEXT,
    returned_at                     TEXT,
    voided_at                       TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (receipt_document_id) REFERENCES fiscal_documents(document_id)
);

CREATE UNIQUE INDEX uq_excise_mark_active
ON excise_marks(normalized_mark_code)
WHERE status IN ('RESERVED','SOLD');

CREATE TABLE document_files (
    file_id                         TEXT PRIMARY KEY,
    document_id                     TEXT NOT NULL,
    file_kind                       TEXT NOT NULL CHECK (file_kind IN (
        'REQUEST_JSON','PAYLOAD_XML','SIGNED_XML','PROTO_REQUEST','PROTO_RESPONSE',
        'XML_CONTAINER','KVT_1','KVT_2','RECEIPT_HTML','RECEIPT_TEXT','RECEIPT_PNG',
        'QRCODE','ZIP_PACKAGE'
    )),
    path                            TEXT NOT NULL UNIQUE,
    sha256                          TEXT NOT NULL,
    size_bytes                      INTEGER,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE CASCADE
);

CREATE INDEX idx_document_files_document
ON document_files(document_id, file_kind);

CREATE TABLE sync_outbox (
    outbox_id                       TEXT PRIMARY KEY,
    aggregate_type                  TEXT NOT NULL CHECK (aggregate_type IN ('DOCUMENT','SHIFT','OFFLINE_SESSION','REPORT','ARCHIVE')),
    aggregate_id                    TEXT NOT NULL,
    fiscal_number                   TEXT NOT NULL,
    artifact_class                  TEXT NOT NULL CHECK (artifact_class IN ('DOCUMENT','REPORT','TRACE','ARCHIVE')),
    sequence_no                     INTEGER NOT NULL DEFAULT 0,
    destination                     TEXT NOT NULL CHECK (destination IN ('CLOUD_HUB','S3','TELEGRAM')),
    payload_path                    TEXT NOT NULL,
    payload_sha256                  TEXT NOT NULL,
    status                          TEXT NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING','SENDING','DONE','DEAD')),
    attempts                        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at                 TEXT,
    hub_error_code                  TEXT,
    last_error                      TEXT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    sent_at                         TEXT
);

CREATE INDEX idx_sync_outbox_status
ON sync_outbox(status, next_attempt_at, fiscal_number, artifact_class, sequence_no);

CREATE TABLE protocol_trace_log (
    trace_id                        TEXT PRIMARY KEY,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    protocol                        TEXT NOT NULL CHECK (protocol IN ('CHECKBOX_REST','WEBCHECK_XMLRPC','MARIA_TCP','INTERNAL')),
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

CREATE INDEX idx_protocol_trace_time
ON protocol_trace_log(created_at);

CREATE INDEX idx_protocol_trace_fiscal
ON protocol_trace_log(fiscal_number, created_at);

CREATE TABLE transport_trace_log (
    trace_id                        TEXT PRIMARY KEY,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    fiscal_number                   TEXT,
    backend_profile_id              TEXT,
    transport_profile_id            TEXT,
    endpoint                        TEXT,
    direction                       TEXT NOT NULL CHECK (direction IN ('OUT','IN','EVENT')),
    request_envelope                TEXT,
    response_envelope               TEXT,
    technical_status                TEXT,
    business_status                 TEXT,
    kvt_meta_json                   TEXT,
    correlation_id                  TEXT,
    retry_no                        INTEGER NOT NULL DEFAULT 0 CHECK (retry_no >= 0),
    duration_ms                     INTEGER,
    FOREIGN KEY (backend_profile_id) REFERENCES backend_profiles(backend_profile_id),
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id)
);

CREATE INDEX idx_transport_trace_fiscal
ON transport_trace_log(fiscal_number, created_at);

CREATE TABLE audit_log (
    audit_id                        INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    entity_type                     TEXT NOT NULL,
    entity_id                       TEXT NOT NULL,
    event_type                      TEXT NOT NULL,
    severity                        TEXT NOT NULL CHECK (severity IN ('INFO','WARNING','ERROR','CRITICAL')),
    event_payload_json              TEXT
);

CREATE INDEX idx_audit_entity
ON audit_log(entity_type, entity_id, created_at);

CREATE VIEW v_current_operational_state AS
SELECT
    ns.fiscal_number,
    ns.mode,
    ns.shift_state,
    ns.current_shift_id,
    ns.current_offline_session_id,
    ns.next_lnd,
    ns.current_channel_lock,
    ns.readiness_state,
    ns.recovery_stage,
    ns.current_month_bucket,
    ns.current_month_offline_seconds,
    ns.crypto_state_json,
    s.opened_via_backend_profile_id,
    s.opened_via_transport_profile_id,
    s.opened_via_protocol,
    s.opened_via_integration_owner,
    s.channel_lock_acquired_at
FROM node_state ns
LEFT JOIN shifts s
  ON s.shift_id = ns.current_shift_id;

COMMIT;

BEGIN;

INSERT INTO schema_registry (schema_name, version, compatible_since, migration_ref, checksum)
VALUES
  ('canonical_envelope', '1.0.1', '1.0.0', 'schemas/common/envelope.schema.json', NULL),
  ('canonical_command',  '1.0.1', '1.0.0', 'schemas/canonical/CanonicalFiscalCommand.schema.json', NULL),
  ('canonical_receipt',  '1.0.1', '1.0.0', 'schemas/canonical/CanonicalReceipt.schema.json', NULL),
  ('canonical_item',     '1.0.1', '1.0.0', 'schemas/canonical/CanonicalReceiptItem.schema.json', NULL),
  ('canonical_payment',  '1.0.1', '1.0.0', 'schemas/canonical/CanonicalPayment.schema.json', NULL),
  ('formatted_line',     '1.0.1', '1.0.0', 'schemas/canonical/FormattedPrintLine.schema.json', NULL),
  ('hub_document',       '1.0.1', '1.0.0', 'schemas/cloud/HubDocumentEnvelope.schema.json', NULL),
  ('hub_error',          '1.0.1', '1.0.0', 'schemas/cloud/HubError.schema.json', NULL),
  ('canonical_error',    '1.0.1', '1.0.0', 'schemas/common/CanonicalError.schema.json', NULL);

COMMIT;
