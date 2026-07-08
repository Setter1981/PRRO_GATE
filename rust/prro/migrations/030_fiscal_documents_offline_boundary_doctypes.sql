-- 030 — add OFFLINE_SESSION_BEGIN / OFFLINE_SESSION_END to fiscal_documents.doc_type
--       + partial UNIQUE index enforcing at most one active boundary doc per
--       (fiscal_number, shift_id, doc_type)
--
-- WHY
-- ---
-- B10: the gateway mints two gateway-internal boundary documents for each
-- offline session — one at session open (OFFLINE_SESSION_BEGIN, lazy-9 / W10a)
-- and one at session close (OFFLINE_SESSION_END, end-10 / W10b).  These docs
-- travel through the normal fiscal_documents write-path (acquire lnd, sign,
-- stamp OFFLINE_LOCAL_ACK, drain) and must be persisted in `fiscal_documents`
-- for auditability and recovery.  The pre-030 doc_type CHECK admits only the
-- nine DPS-protocol receipt types and rejects these two gateway-internal kinds.
--
-- The partial UNIQUE index `ux_fd_active_offline_boundary` is the fail-closed
-- backstop against a concurrent double-mint of either boundary doc within a
-- single (FN, shift) scope: it prevents a second active OFFLINE_SESSION_BEGIN
-- or OFFLINE_SESSION_END row from being inserted while the first is still in a
-- non-terminal state.  This closes the race surface for lazy-9 / end-10 mint
-- without requiring callers to hold an application-level lock across the insert.
--
-- STRICT TABLE NOTE
-- -----------------
-- `fiscal_documents` was created STRICT in 001_baseline.sql.  SQLite cannot
-- ALTER a CHECK constraint in place — a table rebuild (create replacement table,
-- copy rows, drop old, rename) is required.  Migration 025 is the exact
-- precedent for this pattern; this migration mirrors it verbatim.
--
-- The self-referencing FK on `related_receipt_id` is handled the same way as in
-- 025: `PRAGMA defer_foreign_keys = ON` defers all FK checks to transaction-end,
-- by which time every row from the old table is already present in the new table.
--
-- Column set: 025's 25 columns + `offline_dps_code TEXT` (added by 029).
-- No columns were added by 026/027/028 (026 = shifts index; 027 = temp assert;
-- 028 = offline_codes.dps_code) — verified by reading all intervening files.
--
-- BACKWARD COMPATIBILITY
-- ----------------------
-- No existing rows carry the new doc_type values; the copy is verbatim.  All
-- existing indexes and the fd_updated_at trigger are recreated identically.
-- The new partial UNIQUE index applies only to the two new doc_type values and
-- is therefore invisible to all existing query paths.
--
-- ROLLBACK REASONING
-- ------------------
-- Forward: table rebuild (atomic within the sqlx-wrapped transaction) + two
-- index creations.
-- Reverse: the two new doc_type literals become inadmissible again, and
-- `ux_fd_active_offline_boundary` disappears.  Any rows written with the new
-- doc_type values would be unreadable post-rollback — but pre-pilot schema
-- evolution expects rollback = DB reset, not a live downgrade migration.
-- The rebuild does NOT alter the state CHECK, so the ABORTED terminal state
-- added by 025 is preserved verbatim.
--
-- LIVE FILE SEQUENCE
-- ------------------
-- 025 = ABORTED state + table rebuild.  026 = shifts partial UNIQUE index.
-- 027 = one-time boundary assert (no DDL residue).  028 = offline_codes.dps_code.
-- 029 = fiscal_documents.offline_dps_code ALTER ADD COLUMN.
-- This file is 030; sqlx applies migrations by filename prefix order.

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
        'CASH_WITHDRAWAL','X_REPORT','Z_REPORT',
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
-- All indexes and triggers that reference fiscal_documents must be dropped
-- before the table is dropped; they are recreated after the rename.
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

-- ── Step 6: recreate all pre-existing indexes (verbatim from 025 rebuild) ─

-- Non-unique companion index (from 002 / 001 squash — kept per baseline comment)
CREATE INDEX ix_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);

-- [from 007] ADR-M3-A1: fail-closed guard against lnd drift / double-allocation.
CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);

-- Pending-work index.  ABORTED/OFFLINE_SESSION_BEGIN/OFFLINE_SESSION_END are
-- terminal or boundary — intentionally NOT added here, consistent with omission
-- of ACK/REJECTED/CANCELLED/REQUIRES_MANUAL_RECONCILIATION.
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

-- ── Step 7: NEW — partial UNIQUE index for offline boundary doc dedup ──────
-- Prevents concurrent double-mint of OFFLINE_SESSION_BEGIN or OFFLINE_SESSION_END
-- within the same (fiscal_number, shift_id) scope while either is still active
-- (i.e. not yet in a terminal state).
--
-- Terminal-FAILED states excluded from the partial filter (a prior boundary-doc
-- attempt that FAILED terminally frees the slot for a fresh mint):
--   REJECTED, CANCELLED, REQUIRES_MANUAL_RECONCILIATION, ABORTED
-- Note: OFFLINE_LOCAL_ACK is NOT terminal — it is the durable offline-commit
-- state awaiting drain — so a boundary doc at OFFLINE_LOCAL_ACK still holds the
-- slot and rightly blocks a duplicate mint.
-- ERROR_RETRYABLE is INCLUDED in the active set (slot HELD): it is a TRANSIENT
-- send failure that will retry, NOT a terminal failure — a re-mint over it would
-- duplicate the boundary doc while the first is mid-retry.  This MUST match the
-- code-side idempotency predicate (`fiscal_documents::active_boundary_doc_state_tx`
-- + the `doc_state_by_request_id_tx` BEGIN match, which both treat ERROR_RETRYABLE
-- as below-OLA / still-active → fail-closed) so the DB backstop and the code gate
-- never disagree (review LOW-3 reconciliation, 2026-07-08).
-- There is NO bare 'ERROR' state in fiscal_documents (confirmed against the state
-- CHECK above).
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
