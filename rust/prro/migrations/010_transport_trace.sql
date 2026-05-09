-- 010 — transport_trace (W7).
--
-- Anchored on the W7 design freeze
-- (docs/superpowers/specs/2026-05-09-m3a-w7-stage4-send-design.md §3)
-- and ADR-M3-A5 (Pattern B mandatory) / ADR-M3-A9 step 5-6.
--
-- Operational forensics for outgoing wire `send_chk` calls.
-- Append-then-update shape (Option A in the freeze):
--   - 4-pre INSERTs the row with attempt/intent data, completion
--     columns left NULL.  Same with_immediate envelope as the
--     CAS Signed/Encrypted/ErrorRetryable -> Sending.
--   - 4a runs the wire send_chk OUTSIDE any lock.
--   - 4b UPDATEs the same row with wire timing + outcome.
--
-- Crash window forensics:
--   - Crash inside 4-pre: tx rolls back, no trace row, no SENDING
--     marker.  Document stays in its prior state.  Stage 4 is safe to
--     retry.
--   - Crash between 4-pre commit and 4b commit (the safety-critical
--     window):
--       * fiscal_documents.state = SENDING  (durable)
--       * transport_trace row exists with completed_at IS NULL  (durable)
--       * wire send may or may not have been received by DPS — unknown
--     The W9 App::boot recovery rule converts SENDING -> ERROR_RETRYABLE
--     WITHOUT calling send_chk.  This trace row gives the operator the
--     attempt envelope hash and start time for forensic correlation; the
--     `completed_at IS NULL` shape is the structural marker that the
--     attempt did not finish cleanly.  W9 leaves the trace row as-is —
--     it stands as the historical record of the unfinished attempt.
--   - Crash inside 4b: tx rolls back, trace row keeps NULL completion,
--     fiscal_documents.state stays SENDING — same recovery shape as
--     above.  send_chk side-effect was already accepted (or not) by
--     DPS regardless of our local commit; safety still holds because
--     W9 forces a non-send-call recovery path.
--
-- Why a separate table (vs columns on fiscal_documents):
--   - fiscal_documents is the fiscal state of record; trace is forensic.
--   - Pattern B retries (Sending -> ErrorRetryable -> Sending -> ...)
--     produce N attempts per document; fiscal_documents is unique per
--     document_id and cannot carry an attempt list.
--   - Keeps fiscal_documents narrow; trace can grow unbounded under
--     pathological retry without bloating the hot-path SELECTs.
--   - server_fiscal_no still lives on fiscal_documents (per migration
--     002 / 008) — it is a fiscal property of the document, not of an
--     attempt.  The trace row also carries server_fiscal_no when the
--     attempt yielded one, so a forensic dump is self-contained.
--
-- attempt_no semantics: 1-based, monotonic per document_id, allocated
-- inside the same with_immediate envelope as the SENDING CAS via
--    SELECT COALESCE(MAX(attempt_no), 0) + 1
--    FROM transport_trace WHERE document_id = ?
-- (BEGIN IMMEDIATE serialises writers, so the read-then-INSERT cannot
-- race with another stage_send instance.  The single-writer-per-FN
-- invariant from M3a W5 is the broader guarantee, but the IMMEDIATE
-- envelope is what makes the allocation correct in isolation.)
--
-- All three timing TEXT columns store ISO-8601 strings with second
-- resolution.  `started_at` is set from CURRENT_TIMESTAMP at INSERT
-- time (matches audit_log convention).  `wire_call_started_at`,
-- `wire_call_finished_at`, `completed_at` are set on UPDATE in 4b
-- (also from CURRENT_TIMESTAMP — see W7 §4.4 design freeze decision
-- to use DB clocks rather than expand WorkerContext).
--
-- outcome_kind closed set (W7 minimal classifier; W10 may extend the
-- raw text but the CHECK list below is the W7 frozen surface):
--   'OK'                  — Sending -> Sent
--   'REJECTED'            — Sending -> Rejected (Authorization{DocumentReject})
--   'RETRYABLE_TRANSPORT' — Sending -> ErrorRetryable, DpsError::Transport
--   'RETRYABLE_SERVER'    — Sending -> ErrorRetryable, DpsError::Server{code,message}
--   'RETRYABLE_AUTH_FN'   — Sending -> ErrorRetryable, Authorization{FiscalNumberNotRegistered}
--
-- The CHECK list is intentionally narrower than the freeze §3 sketch
-- to match the actual W7 classify_send_outcome surface (KVT1 deferred
-- to W8; the 'OK_KVT1' kind is not part of W7 schema).

CREATE TABLE transport_trace (
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
                  'RETRYABLE_AUTH_FN'
               )),
    server_fiscal_no         TEXT,
    server_status_code       INTEGER,
    error_kind               TEXT,
    error_message            TEXT
        CHECK (error_message IS NULL OR length(error_message) <= 512),
    -- Self-consistency: a row is either incomplete (all completion
    -- columns NULL) or complete (all four required completion columns
    -- non-NULL).  No partial-completion rows.
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
    -- Outcome-specific invariant: an 'OK' completion MUST carry the
    -- DPS-assigned fiscal id.  W7 maps `CheckAck.id` into
    -- `server_fiscal_no`; an OK trace without it is a forensic record
    -- that cannot be cross-checked against `fiscal_documents.
    -- server_fiscal_no` and so cannot prove "this attempt fiscalised
    -- the document".  Empty-string is treated as missing.  The
    -- non-OK outcome kinds (REJECTED, RETRYABLE_*) intentionally
    -- have looser shapes — DPS may or may not have responded with
    -- a status code or message, and the W10 dispatch table is what
    -- governs per-kind required fields beyond OK.
    CHECK (
        outcome_kind IS NULL
        OR outcome_kind != 'OK'
        OR (server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0)
    ),
    PRIMARY KEY (document_id, attempt_no)
) STRICT;

-- Forensic ordering: queries like "give me the most recent N attempts
-- across all documents" or "show me unfinished attempts" need a
-- started_at index.  PRIMARY KEY (document_id, attempt_no) covers
-- per-document scans.
CREATE INDEX ix_transport_trace_started ON transport_trace(started_at);

-- Operator-facing query "which attempts crashed mid-send" — the
-- `completed_at IS NULL` predicate.  Partial index keeps it cheap on
-- the steady-state trace size.
CREATE INDEX ix_transport_trace_unfinished
  ON transport_trace(document_id)
  WHERE completed_at IS NULL;
