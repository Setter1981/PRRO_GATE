-- 011 — outbox (W8).
--
-- Anchored on the W8 design freeze
-- (docs/superpowers/specs/2026-05-09-m3a-w8-stage5-finalize-design.md §3.1)
-- and ADR-M3-A8 (KVT2 forward-only; Ack as terminal-success).
--
-- Cross-process publishing seam for documents that have reached
-- terminal Ack.  Stage 5 finalize INSERTs one row per Ack inside the
-- same `with_immediate` envelope as the CAS `Kvt2 → Ack`, so an Ack
-- without an outbox row is structurally impossible: rollback of either
-- write rolls back the other.
--
-- Lifecycle:
--   * INSERT in stage_finalize::run → status='PENDING', published_at NULL.
--   * (out of M3a scope) cross-process publisher reads the partial
--     index `ix_outbox_pending`, materialises the wire payload, flips
--     status='PUBLISHED', sets published_at = CURRENT_TIMESTAMP.
--
-- One row per document_id (PK).  Rerun-on-Ack is a no-op (the CAS in
-- stage_finalize short-circuits BEFORE the outbox INSERT, AND the PK
-- would reject a duplicate INSERT loudly if the short-circuit ever
-- regressed — defence in depth).
--
-- ON DELETE RESTRICT on the doc FK: keeps the outbox queue
-- referentially honest if a doc gets archived (archive layer is
-- post-M3a).  An archive worker would have to drain published rows
-- first, then archive the doc — clean failure mode.
--
-- payload_path NOT included.  Python carries it; M3a Rust has no
-- archive layer.  When the archive layer lands (post-M3a), a
-- separate migration adds the column.
--
-- Why a separate table (vs columns on fiscal_documents):
--   - fiscal_documents is the fiscal state of record; outbox is the
--     publishing queue.  Two distinct concerns.
--   - Outbox row count grows with publishing volume; fiscal_documents
--     row count grows with submission volume — different cardinality
--     pressure profiles.
--   - Forward compat: future status enum values (PUBLISH_FAILED,
--     RETRY_SCHEDULED, ...) live here without bloating the hot-path
--     `fiscal_documents` SELECT.

CREATE TABLE outbox (
    document_id            BLOB    NOT NULL CHECK (length(document_id) = 16)
        REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT,
    fiscal_number          TEXT    NOT NULL,
    sequence_no            INTEGER NOT NULL CHECK (sequence_no >= 1),
                                                       -- = lnd at finalize time; monotonic per FN
    payload_sha256         BLOB    NOT NULL CHECK (length(payload_sha256) = 32),
                                                       -- canonical payload sha256 from
                                                       -- fiscal_documents.payload_sha256_canonical
    enqueued_at            TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status                 TEXT    NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'PUBLISHED')),
    published_at           TEXT,
    -- Status / published_at consistency: PENDING ⇒ published_at NULL;
    -- PUBLISHED ⇒ published_at NOT NULL.  Mirrors W7.1 transport_trace
    -- all-or-none completion CHECK.
    CHECK (
        (status = 'PENDING' AND published_at IS NULL)
        OR
        (status = 'PUBLISHED' AND published_at IS NOT NULL)
    ),
    PRIMARY KEY (document_id)
) STRICT;

-- Operator-facing publisher query "which docs need to be published
-- next".  Partial index keeps it cheap on the steady-state outbox
-- size (most rows are PUBLISHED post-publishing; PENDING is the
-- working set).
CREATE INDEX ix_outbox_pending
  ON outbox(enqueued_at)
  WHERE status = 'PENDING';
