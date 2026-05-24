-- Migration 019: extend `transport_trace.outcome_kind` CHECK list з
-- 'SYSTEM_CRASH' variant для W12 Post-Closure Hardening Phase 3 / REC-3
-- (orphan trace garbage collection).
--
-- Context: SentReplay path allocates `transport_trace` recovery row в
-- Envelope 1c-pre BEFORE the DPS lastChk call (per W12 Commit 5b.1
-- §412 plan).  If process crashes mid-DPS-call (SIGKILL / OOM / power
-- loss), row залишається в стані `Allocated` (outcome_kind=NULL).
-- Next-tick SentReplay re-allocates a NEW row; orphaned row залишається
-- forever — accumulated noise + lost forensic visibility.
--
-- Boot scanner (см. `services/reconciliation/boot_phase.rs` orphan-trace
-- closure phase added in same Phase 3 commit) closes such rows з
-- outcome_kind=SYSTEM_CRASH + emits `TRANSPORT_TRACE_ORPHAN_CLOSED`
-- audit (Info severity per "operator-visible health metric, not failure").
--
-- SQLite ALTER TABLE doesn't support modifying CHECK constraints —
-- table-rebuild pattern per migrations 008 / 013 precedent.

PRAGMA defer_foreign_keys = ON;

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
                  'RETRYABLE_MAC_HASH_MISMATCH',
                  -- Phase 3 REC-3 addition (2026-05-24): orphan row
                  -- closed by boot scanner after crash mid-DPS-call.
                  'SYSTEM_CRASH'
               )),
    server_fiscal_no         TEXT,
    server_status_code       INTEGER,
    error_kind               TEXT,
    error_message            TEXT
        CHECK (error_message IS NULL OR length(error_message) <= 512),
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
    -- migration 010 (SYSTEM_CRASH does NOT require server_fiscal_no —
    -- по definition we don't know what server returned).
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

-- Re-create the indexes (table-rebuild dropped them along з original).

CREATE INDEX ix_transport_trace_started ON transport_trace(started_at);

CREATE INDEX ix_transport_trace_unfinished
  ON transport_trace(document_id)
  WHERE completed_at IS NULL;

CREATE INDEX idx_transport_trace_doc_retry_class
  ON transport_trace(document_id, attempt_no DESC, retry_class);
