-- 015 — M3b W4: normalize 004-era `offline_sessions` + `offline_codes`
-- onto the M3b state-machine vocabulary.
--
-- Why a normalization migration (vs CREATE-from-scratch):
-- both tables already exist (migration 004); pre-W4 they used
-- Python-era column names that don't align with M3b's W5/W6/W7 state-
-- machine vocabulary (`status` vs `state`, `code_value` vs `code_lnd`,
-- `used_at` vs `consumed_at`, `used_by_doc` vs `consumed_by_document_id`).
-- W4 renames them via the canonical 4-step SQLite idiom (CREATE _new
-- → INSERT…SELECT → DROP old → RENAME) because SQLite does not
-- support `ALTER TABLE … RENAME COLUMN` followed by `ALTER TABLE …
-- ALTER CONSTRAINT` (the CHECK list change on `state` requires a
-- rebuild).
--
-- Data-loss safety: every row is preserved.  Row counts pre-/post-
-- migration must match.  Semantic value transform: `status =
-- 'CLOSING'` maps to `state = 'DRAINING'` (M3b vocabulary).  Any
-- row with a pre-migration `status` value outside the legacy set
-- (`OPENING`, `OPEN`, `CLOSING`, `CLOSED`, `ABORTED`) will fail the
-- new CHECK constraint on insert into the `_new` table — fail-closed.
--
-- Invariants tightened by this migration:
--   - Partial UNIQUE INDEX `ux_offline_active` on
--     `offline_sessions(fiscal_number) WHERE state IN
--     ('OPENING','OPEN','DRAINING')`: "at most one active session
--     per FN" — Pattern C precondition.  Migration 004 had a
--     non-unique index on the same predicate; W4 tightens to UNIQUE.
--   - Partial UNIQUE INDEX `ux_offline_codes_consumed_by_doc` on
--     `offline_codes(consumed_by_document_id) WHERE
--     consumed_by_document_id IS NOT NULL`: each consumed code links
--     to at most one document (defence-in-depth on top of the W5
--     `acquire_code_tx` CAS).
--   - Trigger `offline_codes_consumed_immutable`: once a code is
--     consumed (`consumed_at IS NOT NULL`), neither
--     `consumed_by_document_id` nor `consumed_at` can be mutated.
--     Forbids both (a) re-attribution to a different doc and (b)
--     un-consume (`consumed_at` → NULL).  Admin repair must
--     explicitly DROP + recreate the trigger.
--
-- No `consumed` boolean column — the semantic is `consumed_at IS NULL`
-- (unused) / IS NOT NULL (consumed).  Tested in migrations_apply.rs.

-- ─── offline_sessions normalization ──────────────────────────────────

-- 1. Create the new-shape table.
CREATE TABLE offline_sessions_new (
    offline_session_id  BLOB    PRIMARY KEY  CHECK (length(offline_session_id) = 16),
    fiscal_number       TEXT    NOT NULL,
    state               TEXT    NOT NULL CHECK (state IN ('OPENING','OPEN','DRAINING','CLOSED','ABORTED')),
    opened_at           TEXT    NOT NULL,
    drained_at          TEXT,                       -- NEW (M3b W4): timestamp at DRAINING entry
    closed_at           TEXT,
    reason_abort        TEXT,                       -- NEW (M3b W4): rationale for ABORTED
    last_known_unsigned_xml_sha256 BLOB,            -- preserved from 004
    docs_count          INTEGER NOT NULL DEFAULT 0, -- preserved from 004
    created_at          TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at          TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

-- 2. Copy + transform.  `status` → `state`; `CLOSING` → `DRAINING`.
-- Rows with unexpected `status` values fail the new CHECK on INSERT,
-- aborting the migration (fail-closed).  `drained_at` is back-filled
-- from `updated_at` for rows that were `CLOSING` (best-effort
-- forensic value); other rows get NULL.  `reason_abort` is NULL for
-- all migrated rows (no pre-W4 source).
INSERT INTO offline_sessions_new
    (offline_session_id, fiscal_number, state, opened_at, drained_at, closed_at, reason_abort,
     last_known_unsigned_xml_sha256, docs_count, created_at, updated_at)
SELECT
    offline_session_id,
    fiscal_number,
    CASE status WHEN 'CLOSING' THEN 'DRAINING' ELSE status END AS state,
    opened_at,
    CASE status WHEN 'CLOSING' THEN COALESCE(updated_at, CURRENT_TIMESTAMP) ELSE NULL END AS drained_at,
    closed_at,
    NULL AS reason_abort,
    last_known_unsigned_xml_sha256,
    docs_count,
    created_at,
    updated_at
FROM offline_sessions;

-- 3. Drop old + rename.
DROP TABLE offline_sessions;
ALTER TABLE offline_sessions_new RENAME TO offline_sessions;

-- 4. Indices — W4 tightens the pre-existing ix_offline_active from
-- non-unique to partial UNIQUE.
DROP INDEX IF EXISTS ix_offline_active;
CREATE UNIQUE INDEX ux_offline_active
    ON offline_sessions(fiscal_number)
    WHERE state IN ('OPENING','OPEN','DRAINING');


-- ─── offline_codes normalization ─────────────────────────────────────

-- 1. Create new-shape table.  Column renames; NO `consumed` flag —
-- the semantic is `consumed_at IS NULL` (unused) / IS NOT NULL
-- (consumed).
CREATE TABLE offline_codes_new (
    fiscal_number            TEXT    NOT NULL,
    code_lnd                 INTEGER NOT NULL  CHECK (code_lnd > 0),
    consumed_at              TEXT,
    consumed_by_document_id  BLOB,
    PRIMARY KEY (fiscal_number, code_lnd),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (consumed_by_document_id) REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT
) STRICT;

-- 2. Copy + transform.  `code_value → code_lnd`; `used_at →
-- consumed_at`; `used_by_doc → consumed_by_document_id`.  Pure
-- rename; no value mapping.
INSERT INTO offline_codes_new (fiscal_number, code_lnd, consumed_at, consumed_by_document_id)
SELECT fiscal_number, code_value, used_at, used_by_doc
FROM offline_codes;

-- 3. Drop old + rename.
DROP TABLE offline_codes;
ALTER TABLE offline_codes_new RENAME TO offline_codes;

-- 4. Indices.
CREATE INDEX ix_offline_codes_available
    ON offline_codes(fiscal_number, code_lnd)
    WHERE consumed_at IS NULL;
-- Defence-in-depth: each consumed code links to AT MOST one doc.
-- The W5 `acquire_code_tx` CAS is the primary guard; this index
-- catches admin-tool / raw-SQL paths that bypass the CAS.
CREATE UNIQUE INDEX ux_offline_codes_consumed_by_doc
    ON offline_codes(consumed_by_document_id)
    WHERE consumed_by_document_id IS NOT NULL;

-- 5. Immutability trigger.  Mandatory in W4 acceptance.  Forbids
-- BOTH (a) mutation of consumed_by_document_id once non-NULL AND
-- (b) un-consume via consumed_at → NULL.  Admin repair must
-- explicitly DROP + recreate the trigger.  Allows the legal first
-- consume (NULL → first-doc-id) because OLD.consumed_at IS NULL
-- on that UPDATE.
CREATE TRIGGER offline_codes_consumed_immutable
    BEFORE UPDATE OF consumed_by_document_id, consumed_at ON offline_codes
    WHEN OLD.consumed_at IS NOT NULL
     AND (NEW.consumed_by_document_id IS NOT OLD.consumed_by_document_id
          OR NEW.consumed_at IS NULL)
    BEGIN
        SELECT RAISE(ABORT, 'offline_codes consumed row is immutable; admin repair must DROP + recreate trigger');
    END;
