-- 031 — add CASH_ADVANCE_EPZ to fiscal_documents.doc_type
--
-- WHY
-- ---
-- EPZ (видача готівки за ЕПЗ — cash advance / cashback against a card) becomes a
-- first-class, fully-wired receipt type.  Its documents travel the normal
-- fiscal_documents write-path (acquire lnd, sign `<C T='8'>`, stamp ACK /
-- OFFLINE_LOCAL_ACK, drain) and must be persisted with doc_type='CASH_ADVANCE_EPZ'.
-- The pre-031 doc_type CHECK (last set by 030) admits the nine DPS receipt types +
-- the two B10 offline-boundary kinds, but NOT 'CASH_ADVANCE_EPZ' — so an EPZ mint
-- fails the CHECK.  This migration expands the CHECK by exactly one literal.
--
-- CASH_WITHDRAWAL is retained (the pre-EPZ fail-closed placeholder); EPZ is a
-- NEW, distinct doc_type so the ledger (`aggregate_shift_epz`), z-quiescence, and
-- Z `<EPZ>` aggregation filters stay unambiguous.
--
-- STRICT TABLE NOTE
-- -----------------
-- `fiscal_documents` is STRICT (001_baseline.sql).  SQLite cannot ALTER a CHECK
-- constraint in place — a table rebuild (create replacement, copy rows, drop old,
-- rename, recreate indexes/trigger) is required.  Migrations 025 and 030 are the
-- exact precedents; this migration mirrors 030 verbatim, changing ONLY the
-- doc_type CHECK (adds 'CASH_ADVANCE_EPZ') and preserving every column, index,
-- trigger, and the state CHECK / ABORTED state / offline-boundary index.
--
-- The self-referencing FK on `related_receipt_id` is handled as in 025/030:
-- `PRAGMA defer_foreign_keys = ON` defers FK checks to transaction-end, by which
-- time every row from the old table is present in the new table.
--
-- Column set: identical to 030 (no columns were added by 031's absence — 030 is
-- the immediately-prior fiscal_documents rebuild; nothing between 030 and this
-- file touches the table's column set).
--
-- BACKWARD COMPATIBILITY
-- ----------------------
-- No existing rows carry the new doc_type value; the copy is verbatim.  All
-- existing indexes and the fd_updated_at trigger are recreated identically,
-- including the 030 partial UNIQUE index `ux_fd_active_offline_boundary`.
--
-- ROLLBACK REASONING
-- ------------------
-- Forward: table rebuild (atomic within the sqlx-wrapped transaction) + index +
-- trigger recreation.  Reverse: 'CASH_ADVANCE_EPZ' becomes inadmissible again;
-- any EPZ rows would be unreadable post-rollback — but pre-pilot schema evolution
-- expects rollback = DB reset, not a live downgrade.
--
-- LIVE FILE SEQUENCE
-- ------------------
-- 030 = offline-boundary doctypes + ux_fd_active_offline_boundary.  This file is
-- 031; sqlx applies migrations by filename prefix order.

PRAGMA defer_foreign_keys = ON;

-- ── Step 1: create replacement table with the expanded doc_type CHECK ──────
CREATE TABLE fiscal_documents_new (
    document_id                BLOB    PRIMARY KEY  CHECK (length(document_id) = 16),
    request_id                 BLOB    NOT NULL UNIQUE  CHECK (length(request_id) = 16),
    fiscal_number              TEXT    NOT NULL,
    shift_id                   BLOB,
    offline_session_id         BLOB,
    lnd                        INTEGER NOT NULL  CHECK (lnd >= 1),
    doc_type                   TEXT    NOT NULL  CHECK (doc_type IN (
        'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
        'CASH_WITHDRAWAL','CASH_ADVANCE_EPZ','X_REPORT','Z_REPORT',
        'OFFLINE_SESSION_BEGIN','OFFLINE_SESSION_END'
    )),
    state                      TEXT    NOT NULL  CHECK (state IN (
        'PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ACK',
        'OFFLINE_LOCAL_ACK','REJECTED','CANCELLED','ERROR_RETRYABLE',
        'REQUIRES_MANUAL_RECONCILIATION',
        'ABORTED'
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
    z_report_number            INTEGER
        CHECK (z_report_number IS NULL OR z_report_number >= 1),
    signing_inputs_pinned_at   TEXT,
    mac_recovery_attempts      INTEGER NOT NULL DEFAULT 0
        CHECK (mac_recovery_attempts IN (0, 1)),
    first_kvt1_at              TEXT,
    signed_by_cashier_id       TEXT,
    consecutive_holds          INTEGER NOT NULL DEFAULT 0
        CHECK (consecutive_holds >= 0),
    signing_config_snapshot_id INTEGER
        REFERENCES signing_config_snapshots(id),
    source_sha256              BLOB
        CHECK (source_sha256 IS NULL OR length(source_sha256) = 32),
    offline_dps_code           TEXT,
    FOREIGN KEY (fiscal_number)       REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (shift_id)            REFERENCES shifts(shift_id)                    ON DELETE RESTRICT,
    FOREIGN KEY (offline_session_id)  REFERENCES offline_sessions(offline_session_id) ON DELETE RESTRICT,
    FOREIGN KEY (related_receipt_id)  REFERENCES fiscal_documents_new(document_id)   ON DELETE RESTRICT
) STRICT;

-- ── Step 2: copy all rows verbatim (no state/type transformation) ──────────
INSERT INTO fiscal_documents_new SELECT
    document_id,
    request_id,
    fiscal_number,
    shift_id,
    offline_session_id,
    lnd,
    doc_type,
    state,
    backend_profile_id,
    transport_profile_id,
    fs_mode,
    business_ts,
    server_fiscal_no,
    server_fiscal_date,
    offline_fiscal_no,
    offline_fiscal_date,
    total_sum_kop,
    payload_json,
    payload_sha256_canonical,
    unsigned_xml_sha256,
    previous_hash,
    submission_attempted_at,
    technical_return,
    related_receipt_id,
    created_at,
    updated_at,
    z_report_number,
    signing_inputs_pinned_at,
    mac_recovery_attempts,
    first_kvt1_at,
    signed_by_cashier_id,
    consecutive_holds,
    signing_config_snapshot_id,
    source_sha256,
    offline_dps_code
FROM fiscal_documents;

-- ── Step 3: drop all dependent objects on the OLD table ───────────────────
DROP TRIGGER IF EXISTS fd_updated_at;
DROP INDEX IF EXISTS ix_fd_fn_lnd;
DROP INDEX IF EXISTS ux_fd_fn_lnd;
DROP INDEX IF EXISTS ix_fd_state_pending;
DROP INDEX IF EXISTS ix_fd_recon_manual;
DROP INDEX IF EXISTS ux_fd_fn_zrn;
DROP INDEX IF EXISTS idx_fiscal_documents_signing_config_snapshot_id;
DROP INDEX IF EXISTS ux_fd_active_offline_boundary;

-- ── Step 4: drop the old table ────────────────────────────────────────────
DROP TABLE fiscal_documents;

-- ── Step 5: rename the replacement table into the canonical name ──────────
ALTER TABLE fiscal_documents_new RENAME TO fiscal_documents;

-- ── Step 6: recreate all pre-existing indexes (verbatim from 030 rebuild) ─
CREATE INDEX ix_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);
CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);
CREATE INDEX ix_fd_state_pending ON fiscal_documents(state, created_at)
    WHERE state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE');
CREATE INDEX ix_fd_recon_manual ON fiscal_documents(state)
    WHERE state = 'REQUIRES_MANUAL_RECONCILIATION';
CREATE UNIQUE INDEX ux_fd_fn_zrn
    ON fiscal_documents(fiscal_number, z_report_number)
    WHERE z_report_number IS NOT NULL;
CREATE INDEX idx_fiscal_documents_signing_config_snapshot_id
    ON fiscal_documents(signing_config_snapshot_id)
    WHERE signing_config_snapshot_id IS NOT NULL;

-- ── Step 7: recreate the 030 partial UNIQUE index for offline boundary dedup
CREATE UNIQUE INDEX ux_fd_active_offline_boundary
    ON fiscal_documents (fiscal_number, shift_id, doc_type)
    WHERE doc_type IN ('OFFLINE_SESSION_BEGIN','OFFLINE_SESSION_END')
      AND state NOT IN (
          'REJECTED','CANCELLED','REQUIRES_MANUAL_RECONCILIATION','ABORTED'
      );

-- ── Step 8: recreate the updated_at trigger ───────────────────────────────
CREATE TRIGGER fd_updated_at
AFTER UPDATE ON fiscal_documents
BEGIN
    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;
END;
