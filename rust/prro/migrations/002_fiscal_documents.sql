-- 002 — fiscal_documents, document_files, ingress_inbox.  Per spec §4.2 + §5.4 + §6.2.

CREATE TABLE fiscal_documents (
    document_id                BLOB    PRIMARY KEY  CHECK (length(document_id) = 16),
    request_id                 BLOB    NOT NULL UNIQUE  CHECK (length(request_id) = 16),
    fiscal_number              TEXT    NOT NULL,
    shift_id                   BLOB,
    offline_session_id         BLOB,
    lnd                        INTEGER NOT NULL  CHECK (lnd >= 1),
    doc_type                   TEXT    NOT NULL  CHECK (doc_type IN (
        'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
        'CASH_WITHDRAWAL','X_REPORT','Z_REPORT'
    )),
    state                      TEXT    NOT NULL  CHECK (state IN (
        'PREPARED','SIGNED','ENCRYPTED','SENT','KVT1','KVT2','ACK',
        'OFFLINE_LOCAL_ACK','REJECTED','CANCELLED','ERROR_RETRYABLE',
        'REQUIRES_MANUAL_RECONCILIATION'
    )),
    backend_profile_id         TEXT    NOT NULL,
    transport_profile_id       TEXT    NOT NULL,
    fs_mode                    TEXT    NOT NULL  CHECK (fs_mode IN ('ONLINE','OFFLINE')),
    business_ts                TEXT    NOT NULL,
    server_fiscal_no           TEXT,
    server_fiscal_date         TEXT,
    offline_fiscal_no          INTEGER,
    offline_fiscal_date        TEXT,
    total_sum_kop              INTEGER,
    payload_json               TEXT    NOT NULL,
    payload_sha256_canonical   BLOB    NOT NULL  CHECK (length(payload_sha256_canonical) = 32),
    unsigned_xml_sha256        BLOB    CHECK (unsigned_xml_sha256 IS NULL OR length(unsigned_xml_sha256) = 32),
    previous_hash              BLOB    CHECK (previous_hash IS NULL OR length(previous_hash) = 32),
    submission_attempted_at    TEXT,                              -- spec §5.6 stage gating
    technical_return           INTEGER  CHECK (technical_return IS NULL OR technical_return IN (0,1)),
    related_receipt_id         BLOB,
    created_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (shift_id)     REFERENCES shifts(shift_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX ix_fd_fn_lnd     ON fiscal_documents(fiscal_number, lnd);
CREATE INDEX ix_fd_state_pending ON fiscal_documents(state, created_at)
    WHERE state IN ('PREPARED','SIGNED','ENCRYPTED','SENT','KVT1','ERROR_RETRYABLE');
CREATE INDEX ix_fd_recon_manual ON fiscal_documents(state)
    WHERE state = 'REQUIRES_MANUAL_RECONCILIATION';

CREATE TRIGGER fd_updated_at
AFTER UPDATE ON fiscal_documents
BEGIN
    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;
END;

-- document_files — derivative of fiscal_documents, CASCADE OK
CREATE TABLE document_files (
    document_id BLOB    NOT NULL,
    kind        TEXT    NOT NULL  CHECK (kind IN ('PAYLOAD_XML','SIGNED_XML','KVT1_RAW','KVT2_RAW','PAYLOAD_JSON_CANONICAL','RECEIPT_PDF')),
    content     BLOB    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (document_id, kind),
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE CASCADE
) STRICT;

-- ingress_inbox — operational queue, no FK to fiscal_documents
CREATE TABLE ingress_inbox (
    request_id               BLOB    PRIMARY KEY  CHECK (length(request_id) = 16),
    fiscal_number            TEXT    NOT NULL,
    protocol                 TEXT    NOT NULL  CHECK (protocol IN ('REST','XMLRPC','MARIA','MARIA304','CHECKBOX_COMPAT','INTERNAL')),
    operation_type           TEXT    NOT NULL,
    idempotency_key          TEXT    NOT NULL,
    status                   TEXT    NOT NULL DEFAULT 'NEW'  CHECK (status IN ('NEW','PROCESSING','DONE','REJECTED','ERROR')),
    payload_json             TEXT    NOT NULL,
    payload_sha256_canonical BLOB    NOT NULL  CHECK (length(payload_sha256_canonical) = 32),
    correlation_id           TEXT,
    received_at              TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    processed_at             TEXT,
    error_text               TEXT
) STRICT;

CREATE UNIQUE INDEX ux_inbox_fn_idem ON ingress_inbox(fiscal_number, idempotency_key);
CREATE INDEX ix_inbox_pending ON ingress_inbox(fiscal_number, received_at)
    WHERE status IN ('NEW','PROCESSING');
