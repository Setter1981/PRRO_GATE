-- Migration 024: Add SENDING to fiscal_documents.state CHECK constraint.
--
-- Background: new intermediate state marking that transport_client.send()
-- was called but SENT was not yet committed to the database.
-- On crash-resume, SENDING → ERROR_RETRYABLE (reconciliation checks DPS).
--
-- SQLite does not support ALTER COLUMN to modify CHECK constraints.
-- This migration recreates fiscal_documents with the updated constraint.
-- No data migration required — no rows in SENDING state can exist before this migration.

PRAGMA foreign_keys = OFF;

BEGIN;

-- NOTE: DDL must remain in sync with the current fiscal_documents schema
-- (001 + 003 + 005 cumulative). Any future migration that alters
-- fiscal_documents must also update this block.
CREATE TABLE fiscal_documents_new (
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
        'PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ACK',
        'OFFLINE_LOCAL_ACK',
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
    z_report_number                 INTEGER,
    FOREIGN KEY (request_id) REFERENCES ingress_inbox(request_id),
    FOREIGN KEY (shift_id) REFERENCES shifts(shift_id),
    FOREIGN KEY (offline_session_id) REFERENCES offline_sessions(offline_session_id),
    FOREIGN KEY (backend_profile_id) REFERENCES backend_profiles(backend_profile_id),
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id)
);

INSERT INTO fiscal_documents_new SELECT * FROM fiscal_documents;

DROP TABLE fiscal_documents;

ALTER TABLE fiscal_documents_new RENAME TO fiscal_documents;

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

COMMIT;

PRAGMA foreign_keys = ON;
