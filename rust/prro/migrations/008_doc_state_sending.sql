-- 008 — Extend fiscal_documents.state CHECK list with 'SENDING' (Pattern B
-- intent-marker per ADR-M3-A9 step 1).  SQLite cannot ALTER a CHECK
-- constraint; the documented procedure is a full table rebuild
-- (https://www.sqlite.org/lang_altertable.html, "Making Other Kinds Of
-- Table Schema Changes").
--
-- Rebuild design (W1, audited 2026-05-07):
--
-- The naive "CREATE _new / INSERT / DROP old / RENAME _new" pattern is
-- known to fail at COMMIT under PRAGMA defer_foreign_keys=ON in the
-- presence of a self-FK + ALTER TABLE RENAME inside the same
-- transaction.  Empirical reproduction (sqlite3 3.45.1, prro
-- migrations 007/008 with two fiscal_documents where doc B references
-- doc A via related_receipt_id and document_files rows for both):
-- foreign_key_check returns no violations mid-transaction yet
-- COMMIT raises SQLITE_CONSTRAINT_FOREIGNKEY.  We therefore avoid
-- ALTER TABLE RENAME and instead drop fiscal_documents and recreate
-- it under the same name.  This keeps every FK reference (document_files
-- inbound and the self-FK) bound to a stable table name throughout.
--
-- Two FK hazards still apply and are addressed:
--
-- 1. document_files has `FOREIGN KEY (document_id) REFERENCES
--    fiscal_documents(document_id) ON DELETE CASCADE` (migration 002).
--    DROP TABLE fiscal_documents CASCADE-deletes every document_files
--    row even inside a transaction with PRAGMA defer_foreign_keys=ON,
--    because defer_foreign_keys defers VALIDATION of pending
--    constraints, not the EXECUTION of CASCADE actions, which fire at
--    the time of the triggering statement.  document_files data is
--    therefore relocated to a non-FK holding table for the duration
--    of the parent rebuild.
--
-- 2. fiscal_documents has a self-FK
--    `related_receipt_id REFERENCES fiscal_documents(document_id) ON
--    DELETE RESTRICT`.  We snapshot the parent rows into a non-FK
--    holding table before DROP, then drop, recreate, and reinsert.
--    With defer_foreign_keys=ON the self-FK validates only at COMMIT,
--    by which time every referenced document_id is back in
--    fiscal_documents.
--
-- Step layout:
--   A. Snapshot fiscal_documents into fiscal_documents_tmp (no FK).
--   B. Snapshot document_files into document_files_tmp (no FK).
--   C. Drop document_files.  Releases inbound CASCADE FK to fiscal_documents.
--   D. Drop fiscal_documents.  No inbound FKs at this point; self-FK is
--      a no-op for DROP TABLE (DROP does not fire ON DELETE actions).
--   E. Recreate fiscal_documents (same name) with 'SENDING' added to
--      the state CHECK list.
--   F. INSERT rows from fiscal_documents_tmp.  Self-FK validation is
--      deferred to COMMIT.
--   G. Recreate the four objects attached to fiscal_documents
--      (the 002 non-unique ix_fd_fn_lnd, the 007 UNIQUE
--      ux_fd_fn_lnd, ix_fd_state_pending with 'SENDING' added to
--      its WHERE predicate, ix_fd_recon_manual, and the
--      fd_updated_at trigger).
--   H. Recreate document_files with FK to (recreated) fiscal_documents.
--   I. INSERT rows from document_files_tmp.
--   J. Drop both holding tables.

PRAGMA defer_foreign_keys = ON;

-- Step A: snapshot fiscal_documents into a non-FK holding table.
-- `CREATE TABLE ... AS SELECT *` produces a table with column types
-- inferred from the source but no constraints, no PRIMARY KEY, no FK,
-- and no STRICT — exactly what we want for a temporary holder.
CREATE TABLE fiscal_documents_tmp AS SELECT * FROM fiscal_documents;

-- Step B: snapshot document_files into a non-FK holding table.
CREATE TABLE document_files_tmp AS SELECT * FROM document_files;

-- Step C: drop document_files.  This releases its inbound FK to
-- fiscal_documents.  document_files has no inbound FKs of its own.
DROP TABLE document_files;

-- Step D: drop fiscal_documents.  No inbound FKs remain; self-FK does
-- not fire on DROP (RESTRICT applies to DELETE, not DROP TABLE).
DROP TABLE fiscal_documents;

-- Step E: recreate fiscal_documents under the same name with the
-- extended state CHECK list.  Same-name recreation avoids the
-- ALTER TABLE RENAME quirk that breaks commit-time deferred FK
-- validation when a self-FK is present.
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
        'PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ACK',
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
    submission_attempted_at    TEXT,
    technical_return           INTEGER  CHECK (technical_return IS NULL OR technical_return IN (0,1)),
    related_receipt_id         BLOB,
    created_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number)       REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (shift_id)            REFERENCES shifts(shift_id)                    ON DELETE RESTRICT,
    FOREIGN KEY (offline_session_id)  REFERENCES offline_sessions(offline_session_id) ON DELETE RESTRICT,
    FOREIGN KEY (related_receipt_id)  REFERENCES fiscal_documents(document_id)        ON DELETE RESTRICT
) STRICT;

-- Step F: copy rows back into fiscal_documents.  Self-FK references
-- are deferred to COMMIT thanks to PRAGMA defer_foreign_keys=ON; by
-- COMMIT every related_receipt_id has its parent row in the rebuilt
-- table.
INSERT INTO fiscal_documents (
    document_id, request_id, fiscal_number, shift_id, offline_session_id,
    lnd, doc_type, state, backend_profile_id, transport_profile_id,
    fs_mode, business_ts, server_fiscal_no, server_fiscal_date,
    offline_fiscal_no, offline_fiscal_date, total_sum_kop, payload_json,
    payload_sha256_canonical, unsigned_xml_sha256, previous_hash,
    submission_attempted_at, technical_return, related_receipt_id,
    created_at, updated_at
)
SELECT
    document_id, request_id, fiscal_number, shift_id, offline_session_id,
    lnd, doc_type, state, backend_profile_id, transport_profile_id,
    fs_mode, business_ts, server_fiscal_no, server_fiscal_date,
    offline_fiscal_no, offline_fiscal_date, total_sum_kop, payload_json,
    payload_sha256_canonical, unsigned_xml_sha256, previous_hash,
    submission_attempted_at, technical_return, related_receipt_id,
    created_at, updated_at
FROM fiscal_documents_tmp;

-- Step G: recreate the objects attached to fiscal_documents.
CREATE INDEX ix_fd_fn_lnd            ON fiscal_documents(fiscal_number, lnd);
CREATE UNIQUE INDEX ux_fd_fn_lnd     ON fiscal_documents(fiscal_number, lnd);
CREATE INDEX ix_fd_state_pending     ON fiscal_documents(state, created_at)
    WHERE state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE');
CREATE INDEX ix_fd_recon_manual      ON fiscal_documents(state)
    WHERE state = 'REQUIRES_MANUAL_RECONCILIATION';

CREATE TRIGGER fd_updated_at
AFTER UPDATE ON fiscal_documents
BEGIN
    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;
END;

-- Step H: recreate document_files with FK to the rebuilt
-- fiscal_documents.
CREATE TABLE document_files (
    document_id BLOB    NOT NULL,
    kind        TEXT    NOT NULL  CHECK (kind IN ('PAYLOAD_XML','SIGNED_XML','KVT1_RAW','KVT2_RAW','PAYLOAD_JSON_CANONICAL','RECEIPT_PDF')),
    content     BLOB    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (document_id, kind),
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE CASCADE
) STRICT;

-- Step I: restore document_files rows.
INSERT INTO document_files (document_id, kind, content, created_at)
SELECT document_id, kind, content, created_at FROM document_files_tmp;

-- Step J: drop both holding tables.
DROP TABLE document_files_tmp;
DROP TABLE fiscal_documents_tmp;
