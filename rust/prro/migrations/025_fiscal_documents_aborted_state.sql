-- 025 — add 'ABORTED' to fiscal_documents.state (post-sign refusal orphan fix)
--
-- WHY
-- ---
-- Pattern B (online) and the PREPARED→SIGNED→ENCRYPTED pipeline can reach a
-- point where the document has been locally signed but DPS refuses issuance
-- before any fiscal number is assigned (e.g. FN deregistered between signing
-- and send, or crypto sidecar rejects the envelope in the signing stage itself).
-- Under the pre-025 schema those documents have no legal terminal state: they
-- are NOT issued receipts (so they must never appear as ACK/REJECTED in the
-- ledger) and they are NOT retriable indefinitely (so ERROR_RETRYABLE is
-- semantically wrong).  ABORTED is a NON-ISSUED terminal: the document consumed
-- an lnd slot and a signing attempt but produced no DPS-registered receipt.
-- This restores the "ledger of issued receipts only" pin (CLAUDE.md §persistence
-- model) — aborted docs stay in the ledger for auditability but are clearly
-- distinguished from ever-submitted documents.
--
-- LIVE FILE SEQUENCE
-- ------------------
-- Migrations 003-024 were squashed into 001_baseline.sql (see its header
-- comment "[from 007]" / "[from 002]" archaeology).  The two live files are
-- 001_baseline.sql and 002_transport_trace_is_probe.sql.  This file is 025
-- because that is the architect's locked pre-squash sequence number; sqlx
-- applies migrations by version order (filename prefix), so the nominal gap
-- after 002 is correct and intentional — do not renumber.
--
-- REBUILD APPROACH
-- ----------------
-- SQLite cannot ALTER a CHECK constraint.  The standard workaround is a
-- 12-step table rebuild: create a replacement table, copy data, drop the
-- old table, rename.
--
-- sqlx wraps each migration in a BEGIN … COMMIT transaction.  This means
-- `PRAGMA foreign_keys = OFF` is a NO-OP inside the migration (SQLite
-- docs: foreign_keys pragma is ignored when there is a pending transaction).
-- The connection has foreign_keys=ON (set via SqliteConnectOptions).
--
-- To handle the self-referencing FK on `related_receipt_id REFERENCES
-- fiscal_documents(document_id)` safely during the INSERT … SELECT copy
-- (rows in the old table reference other rows that may not yet exist in the
-- new table), we use:
--
--     PRAGMA defer_foreign_keys = ON;
--
-- This pragma IS effective inside a transaction (it defers all FK checks
-- to the end of the transaction, not just the end of the statement).
-- By the time the transaction commits, every row from the old table has been
-- copied to the new table, so the self-FK is satisfied.  The other FKs
-- (fiscal_number_config, shifts, offline_sessions, signing_config_snapshots)
-- reference tables that already exist and are not touched by this migration,
-- so they are satisfied immediately.
--
-- The new table is created FIRST under a temporary name; the old table is
-- dropped only after the copy succeeds; rename is the final atomic step.
-- This is the pattern recommended by the SQLite documentation §7
-- ("Making Other Kinds of Table Schema Changes").
--
-- NO DATA MIGRATION: existing rows are copied verbatim.  No state value
-- changes.  'ABORTED' is only added as an accepted value; it will not
-- appear in any existing row.

PRAGMA defer_foreign_keys = ON;

-- ── Step 1: create replacement table with the expanded state CHECK ─────────
CREATE TABLE fiscal_documents_new (
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
    FOREIGN KEY (fiscal_number)       REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (shift_id)            REFERENCES shifts(shift_id)                    ON DELETE RESTRICT,
    FOREIGN KEY (offline_session_id)  REFERENCES offline_sessions(offline_session_id) ON DELETE RESTRICT,
    FOREIGN KEY (related_receipt_id)  REFERENCES fiscal_documents_new(document_id)   ON DELETE RESTRICT
) STRICT;

-- ── Step 2: copy all rows verbatim (no state transformation) ──────────────
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
    source_sha256
FROM fiscal_documents;

-- ── Step 3: drop all dependent objects on the OLD table ───────────────────
-- Indexes and triggers referencing fiscal_documents must be dropped before
-- the table is dropped; they will be recreated against fiscal_documents_new
-- after the rename.
DROP TRIGGER IF EXISTS fd_updated_at;
DROP INDEX IF EXISTS ix_fd_fn_lnd;
DROP INDEX IF EXISTS ux_fd_fn_lnd;
DROP INDEX IF EXISTS ix_fd_state_pending;
DROP INDEX IF EXISTS ix_fd_recon_manual;
DROP INDEX IF EXISTS ux_fd_fn_zrn;
DROP INDEX IF EXISTS idx_fiscal_documents_signing_config_snapshot_id;

-- ── Step 4: drop the old table ────────────────────────────────────────────
DROP TABLE fiscal_documents;

-- ── Step 5: rename the replacement table into the canonical name ──────────
ALTER TABLE fiscal_documents_new RENAME TO fiscal_documents;

-- ── Step 6: recreate all indexes (verbatim from 001_baseline.sql) ─────────

-- Non-unique companion index (from 002 / 001 squash — kept per baseline comment)
CREATE INDEX ix_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);

-- [from 007] ADR-M3-A1: fail-closed guard against lnd drift / double-allocation.
CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);

-- Pending-work index.  ABORTED is a terminal state — intentionally NOT added
-- here, matching the omission of ACK/REJECTED/CANCELLED/REQUIRES_MANUAL_RECONCILIATION.
CREATE INDEX ix_fd_state_pending ON fiscal_documents(state, created_at)
    WHERE state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE');

-- Manual reconciliation fast-path.
CREATE INDEX ix_fd_recon_manual ON fiscal_documents(state)
    WHERE state = 'REQUIRES_MANUAL_RECONCILIATION';

-- [from 009] W6 / ADR-M3-A2: Z-report sequencer — per-FN uniqueness guard.
CREATE UNIQUE INDEX ux_fd_fn_zrn
    ON fiscal_documents(fiscal_number, z_report_number)
    WHERE z_report_number IS NOT NULL;

-- [from 024] signing_config_snapshot_id FK lookup index.
CREATE INDEX idx_fiscal_documents_signing_config_snapshot_id
    ON fiscal_documents(signing_config_snapshot_id)
    WHERE signing_config_snapshot_id IS NOT NULL;

-- ── Step 7: recreate the updated_at trigger ───────────────────────────────
CREATE TRIGGER fd_updated_at
AFTER UPDATE ON fiscal_documents
BEGIN
    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;
END;
