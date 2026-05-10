-- 013 — W10.4 MAC recovery (Server `-12` ERROR_BAD_HASH_PREV).
--
-- Anchored on:
--   * W10 design freeze §4.4 (HIGH 1 + HIGH 2 + LOW 1 close —
--     Pattern B-consistent two-attempt sequence with atomic single-tx
--     PERSIST).
--   * docs/superpowers/specs/2026-05-10-m3a-w10-dps-dispatch-design.md
--     §4.4.1 sequence diagram.
--   * Migration 008 precedent for SQLite table-rebuild (the
--     `outcome_kind` CHECK extension cannot be done by `ALTER TABLE`).
--
-- Two changes bundled into a single migration so the MAC-recovery DDL
-- lands atomically:
--
--   (A) `fiscal_documents.mac_recovery_attempts` — INTEGER counter
--       with DDL-enforced `IN (0, 1)` budget.  Single-bit semantics:
--       per-doc lifetime allows AT MOST one MAC recovery cycle.  The
--       claim helper (`fiscal_documents::mac_recovery_claim_counter_tx`)
--       guards `WHERE state='ERROR_RETRYABLE' AND mac_recovery_attempts = 0`,
--       so a second `-12` after a re-sign already burned the budget
--       fails the CAS and routes to TerminalReject + audit
--       `MacRecoveryFailedRepeatHashMismatch` (per §3.4 closed enum).
--
--   (B) `transport_trace.outcome_kind` CHECK list extended with
--       `'RETRYABLE_MAC_HASH_MISMATCH'` for the `-12` first-attempt
--       trace row.  SQLite cannot ALTER an existing column-level
--       CHECK in place; the only way is the table-rebuild pattern
--       established in migration 008.  We rebuild `transport_trace`
--       preserving all rows + indexes verbatim.
--
-- Backwards-compatibility:
--   - Existing `fiscal_documents` rows get `mac_recovery_attempts = 0`
--     via the DEFAULT clause; the CHECK constraint accepts.
--   - Existing `transport_trace` rows are SELECT-INSERT-copied during
--     the rebuild; their `outcome_kind` values remain in the original
--     CHECK list (subset of the extended list).
--   - retry_class column added in migration 012 is preserved through
--     the rebuild.

-- ─── (A) fiscal_documents.mac_recovery_attempts ─────────────────────

ALTER TABLE fiscal_documents
    ADD COLUMN mac_recovery_attempts INTEGER NOT NULL DEFAULT 0
        CHECK (mac_recovery_attempts IN (0, 1));

-- ─── (B) transport_trace.outcome_kind CHECK extension via rebuild ───
--
-- Per migration 008 precedent: ALTER cannot extend a column-level
-- CHECK in SQLite.  We:
--   1. Disable foreign-key enforcement during the rebuild (recursive
--      DROP would otherwise cascade to CHECK rows mid-DDL).
--   2. Create the new table shape under `transport_trace_new`.
--   3. INSERT-SELECT all existing columns including `retry_class`.
--   4. DROP old table; RENAME new → `transport_trace`.
--   5. Re-create the indexes (010 + 012).
--   6. Re-enable foreign-key enforcement.

PRAGMA foreign_keys = OFF;

CREATE TABLE transport_trace_new (
    document_id              BLOB    NOT NULL CHECK (length(document_id) = 16)
        REFERENCES fiscal_documents(document_id) ON DELETE CASCADE,
    attempt_no               INTEGER NOT NULL CHECK (attempt_no >= 1),
    started_at               TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    backend_profile_id       TEXT    NOT NULL,
    transport_profile_id     TEXT    NOT NULL,
    request_envelope_sha256  BLOB    NOT NULL
        CHECK (length(request_envelope_sha256) = 32),
    -- Completion columns — NULL until 4b UPDATE.
    completed_at             TEXT,
    wire_call_started_at     TEXT,
    wire_call_finished_at    TEXT,
    outcome_kind             TEXT
        CHECK (outcome_kind IS NULL
               OR outcome_kind IN (
                  'OK',
                  'REJECTED',
                  'RETRYABLE_TRANSPORT',
                  'RETRYABLE_SERVER',
                  'RETRYABLE_AUTH_FN',
                  'RETRYABLE_MAC_HASH_MISMATCH'
               )),
    server_fiscal_no         TEXT,
    server_status_code       INTEGER,
    error_kind               TEXT,
    error_message            TEXT
        CHECK (error_message IS NULL OR length(error_message) <= 512),
    -- W10.2 review fix-up: durable RetryClass encoding (migration 012).
    retry_class              TEXT,
    -- Self-consistency: a row is either incomplete or complete.
    -- Identical to migration 010.
    CHECK (
        (completed_at IS NULL
         AND wire_call_started_at IS NULL
         AND wire_call_finished_at IS NULL
         AND outcome_kind IS NULL)
        OR
        (completed_at IS NOT NULL
         AND wire_call_started_at IS NOT NULL
         AND wire_call_finished_at IS NOT NULL
         AND outcome_kind IS NOT NULL)
    ),
    -- OK ⇒ server_fiscal_no NOT NULL AND length > 0.  Identical to
    -- migration 010.
    CHECK (
        outcome_kind IS NULL
        OR outcome_kind != 'OK'
        OR (server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0)
    ),
    PRIMARY KEY (document_id, attempt_no)
) STRICT;

INSERT INTO transport_trace_new
    (document_id, attempt_no, started_at, backend_profile_id,
     transport_profile_id, request_envelope_sha256, completed_at,
     wire_call_started_at, wire_call_finished_at, outcome_kind,
     server_fiscal_no, server_status_code, error_kind, error_message,
     retry_class)
SELECT
    document_id, attempt_no, started_at, backend_profile_id,
    transport_profile_id, request_envelope_sha256, completed_at,
    wire_call_started_at, wire_call_finished_at, outcome_kind,
    server_fiscal_no, server_status_code, error_kind, error_message,
    retry_class
FROM transport_trace;

DROP TABLE transport_trace;
ALTER TABLE transport_trace_new RENAME TO transport_trace;

-- Re-create the indexes (the table-rebuild dropped them along with
-- the original table).

CREATE INDEX ix_transport_trace_started ON transport_trace(started_at);

CREATE INDEX ix_transport_trace_unfinished
  ON transport_trace(document_id)
  WHERE completed_at IS NULL;

CREATE INDEX idx_transport_trace_doc_retry_class
  ON transport_trace(document_id, attempt_no DESC, retry_class);

PRAGMA foreign_keys = ON;
