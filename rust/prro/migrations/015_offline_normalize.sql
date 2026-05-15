-- 015 — M3b W4: normalize 004-era `offline_sessions` + `offline_codes`
-- onto the M3b state-machine vocabulary.
--
-- Why a normalization migration (vs CREATE-from-scratch):
-- both tables already exist (migration 004); pre-W4 they used
-- Python-era column names that don't align with M3b's W5/W6/W7 state-
-- machine vocabulary (`status` vs `state`, `code_value` vs `code_lnd`,
-- `used_at` vs `consumed_at`, `used_by_doc` vs `consumed_by_document_id`).
--
-- Data-loss safety: every row is preserved.  Row counts pre-/post-
-- migration must match.  Semantic value transform: `status =
-- 'CLOSING'` maps to `state = 'DRAINING'` (M3b vocabulary).  Any
-- row with a pre-migration `status` value outside the legacy set
-- (`OPENING`, `OPEN`, `CLOSING`, `CLOSED`, `ABORTED`) will fail the
-- new CHECK constraint on insert into the rebuilt table — fail-closed.
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
--     `consumed_by_document_id` nor `consumed_at` can be mutated
--     (covers re-attribution, un-consume, AND timestamp mutation).
--     Forbids ALL three; admin repair must DROP + recreate the
--     trigger.  Allows exact no-op replay (same value writes).
--
-- No `consumed` boolean column — the semantic is `consumed_at IS NULL`
-- (unused) / IS NOT NULL (consumed).  Tested in migrations_apply.rs.
--
-- ─── FK-safe rebuild strategy (HIGH-fix, operator review 2026-05-15) ──
--
-- `fiscal_documents` has
-- `FOREIGN KEY (offline_session_id) REFERENCES offline_sessions
-- (offline_session_id) ON DELETE RESTRICT` (migration 002).  This
-- migration runs inside sqlx-sqlite 0.8.6's wrapping transaction
-- (`apply()` calls `self.begin()`), so:
--   - `PRAGMA foreign_keys = OFF` is a no-op inside the wrapping tx;
--   - `PRAGMA defer_foreign_keys = ON` IS honoured and defers FK
--     VALIDATION to COMMIT (same as the migration 008 pattern); but
--   - `defer_foreign_keys` does NOT defer execution of FK ACTIONS
--     (CASCADE, RESTRICT) — those fire at the time of the triggering
--     statement.
--
-- Consequence: `DROP TABLE offline_sessions` while any
-- `fiscal_documents.offline_session_id` row points at a session
-- would fail RESTRICT immediately, even with defer_foreign_keys=ON.
--
-- The 008 migration solved a similar problem (rebuild fiscal_documents
-- + its inbound CASCADE document_files) by snapshotting + dropping
-- + recreating under the SAME name.  We apply the same shape here,
-- with a smaller perimeter: instead of rebuilding fiscal_documents
-- (which would cascade to document_files etc.), we NULL the inbound
-- FK column on fiscal_documents around the offline_sessions rebuild,
-- then restore it.  Step layout:
--
--   1. `PRAGMA defer_foreign_keys = ON` — defer self-FK / inbound-FK
--      VALIDATION to COMMIT; the rebuild stays consistent at COMMIT.
--   2. Add a temporary holding column on fiscal_documents to stash
--      `offline_session_id` values across the rebuild.
--   3. NULL `fiscal_documents.offline_session_id` so the RESTRICT
--      action on DROP TABLE offline_sessions sees no children.
--   4. Snapshot offline_sessions into a no-FK temp table.
--   5. DROP TABLE offline_sessions (succeeds — no children).
--   6. CREATE offline_sessions under the SAME name with the new
--      shape (NOT `_new` + ALTER RENAME — RENAME breaks deferred FK
--      validation per 008 lesson).
--   7. INSERT rows from snapshot with `CLOSING → DRAINING` transform.
--   8. Restore `fiscal_documents.offline_session_id` from the
--      temp column.  At this point FK refs point at the rebuilt
--      offline_sessions (same name); defer_foreign_keys defers
--      validation to COMMIT, by which time everything resolves.
--   9. Drop the temp column on fiscal_documents.
--  10. Drop the offline_sessions snapshot table.
--  11. Build offline_sessions indices.
--  12. Same DROP-and-recreate-same-name pattern for offline_codes
--      (no inbound FK refs, but we keep the pattern for symmetry).
--  13. Build offline_codes indices + immutability trigger.
--  14. Hard-abort FK integrity guard: `pragma_foreign_key_check`
--      result piped into a temp table with a CHECK constraint that
--      fails if any rows materialise.  Operator-pinned MED fix
--      (fail-closed inside the migration, not relying on the
--      surrounding COMMIT).

PRAGMA defer_foreign_keys = ON;

-- ─── offline_sessions normalization ──────────────────────────────────

-- Step 2: temporary holding column on fiscal_documents for the
-- inbound FK values.  ALTER TABLE ADD COLUMN is supported since
-- SQLite 3.2; ADD COLUMN cannot include a FK constraint, which is
-- exactly what we want — we want this column to NOT participate in
-- FK enforcement during the rebuild.
ALTER TABLE fiscal_documents ADD COLUMN _w4_offline_session_id_tmp BLOB;

-- Step 3: stash + null the FK column so DROP TABLE offline_sessions
-- below can succeed without triggering RESTRICT.  Single UPDATE
-- copies and clears in one pass.
UPDATE fiscal_documents
   SET _w4_offline_session_id_tmp = offline_session_id,
       offline_session_id         = NULL
 WHERE offline_session_id IS NOT NULL;

-- Step 4: snapshot offline_sessions into a no-FK / no-CHECK temp
-- table.  `CREATE TABLE … AS SELECT *` infers types from the
-- source but does NOT carry CHECK, PK, UNIQUE, or FK — exactly the
-- "non-FK holding table" pattern from migration 008.
CREATE TABLE __w4_offline_sessions_tmp__ AS SELECT * FROM offline_sessions;

-- Step 5: drop the old table.  No inbound FK refs remain (step 3
-- nulled them); self-FK on offline_sessions does not exist.  DROP
-- succeeds even with foreign_keys ON.
DROP TABLE offline_sessions;

-- Step 6: recreate offline_sessions under the SAME name with the
-- M3b shape.  Same-name recreation (not ALTER RENAME) avoids the
-- deferred-FK-validation breakage documented in migration 008.
CREATE TABLE offline_sessions (
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

-- Step 7: copy + transform.  `status` → `state`; `CLOSING` →
-- `DRAINING`.  Rows with unexpected `status` values fail the new
-- CHECK on INSERT, aborting the migration (fail-closed).
-- `drained_at` back-filled from `updated_at` for ex-`CLOSING` rows
-- (best-effort forensic timestamp); NULL otherwise.  `reason_abort`
-- is NULL for all migrated rows (no pre-W4 source).
INSERT INTO offline_sessions
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
FROM __w4_offline_sessions_tmp__;

-- Step 8: restore the inbound FK column.  At this point
-- `fiscal_documents.offline_session_id` FK targets the rebuilt
-- offline_sessions table (same name, same row data).  With
-- defer_foreign_keys=ON, validation is deferred to COMMIT.
UPDATE fiscal_documents
   SET offline_session_id = _w4_offline_session_id_tmp
 WHERE _w4_offline_session_id_tmp IS NOT NULL;

-- Step 9: drop the temp column on fiscal_documents.  Requires
-- SQLite ≥ 3.35 (DROP COLUMN); libsqlite3-sys 0.30 bundles 3.46+.
ALTER TABLE fiscal_documents DROP COLUMN _w4_offline_session_id_tmp;

-- Step 10: drop the offline_sessions snapshot.
DROP TABLE __w4_offline_sessions_tmp__;

-- Step 11: offline_sessions indices.  Tighten from 004's non-unique
-- `ix_offline_active` to partial UNIQUE `ux_offline_active`
-- (the legacy index was already dropped together with the old
-- table at step 5).
CREATE UNIQUE INDEX ux_offline_active
    ON offline_sessions(fiscal_number)
    WHERE state IN ('OPENING','OPEN','DRAINING');


-- ─── offline_codes normalization ─────────────────────────────────────
--
-- No inbound FK refs to offline_codes (only outbound: to
-- fiscal_number_config and fiscal_documents.document_id).  So
-- DROP+CREATE same-name works without the holding-column dance,
-- but we keep the snapshot pattern for symmetry with offline_sessions
-- and so the FK on `consumed_by_document_id` validates against the
-- current fiscal_documents at COMMIT.

CREATE TABLE __w4_offline_codes_tmp__ AS SELECT * FROM offline_codes;

DROP TABLE offline_codes;

CREATE TABLE offline_codes (
    fiscal_number            TEXT    NOT NULL,
    code_lnd                 INTEGER NOT NULL  CHECK (code_lnd > 0),
    consumed_at              TEXT,
    consumed_by_document_id  BLOB,
    PRIMARY KEY (fiscal_number, code_lnd),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (consumed_by_document_id) REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT
) STRICT;

INSERT INTO offline_codes (fiscal_number, code_lnd, consumed_at, consumed_by_document_id)
SELECT fiscal_number, code_value, used_at, used_by_doc
FROM __w4_offline_codes_tmp__;

DROP TABLE __w4_offline_codes_tmp__;

-- Indices.
CREATE INDEX ix_offline_codes_available
    ON offline_codes(fiscal_number, code_lnd)
    WHERE consumed_at IS NULL;
-- Defence-in-depth: each consumed code links to AT MOST one doc.
CREATE UNIQUE INDEX ux_offline_codes_consumed_by_doc
    ON offline_codes(consumed_by_document_id)
    WHERE consumed_by_document_id IS NOT NULL;

-- Immutability trigger.  Mandatory in W4 acceptance.  Forbids
-- ALL mutation of a consumed row's link/timestamp once
-- `consumed_at IS NOT NULL`:
--   (a) `consumed_by_document_id` change (re-attribution OR clear);
--   (b) `consumed_at` change to NULL (un-consume);
--   (c) `consumed_at` change to a different non-NULL value.
-- The NULL-safe inequality `NEW IS NOT OLD` catches all three; same-
-- value no-ops are intentionally allowed (idempotent replay).
CREATE TRIGGER offline_codes_consumed_immutable
    BEFORE UPDATE OF consumed_by_document_id, consumed_at ON offline_codes
    WHEN OLD.consumed_at IS NOT NULL
     AND (NEW.consumed_by_document_id IS NOT OLD.consumed_by_document_id
          OR NEW.consumed_at IS NOT OLD.consumed_at)
    BEGIN
        SELECT RAISE(ABORT, 'offline_codes consumed row is immutable; admin repair must DROP + recreate trigger');
    END;


-- ─── Hard-abort FK integrity guard (MED-fix, operator review 2026-05-15) ──
--
-- `PRAGMA foreign_key_check` returns a result set of FK violations.
-- sqlx::migrate consumes the result set but does NOT abort the
-- migration on non-empty results — only the surrounding COMMIT
-- would catch them via `defer_foreign_keys`.  Operator MED criterion:
-- the migration MUST hard-abort itself on FK violations, not rely on
-- the wrapper's COMMIT.
--
-- Implementation: create a TEMP holding table with a CHECK
-- constraint that fails if any row materialises.  Then INSERT into
-- it from `pragma_foreign_key_check`; if pragma returns any rows,
-- the INSERT triggers the CHECK and SQLite aborts the entire
-- migration with `SQLITE_CONSTRAINT_CHECK`.  If pragma returns
-- zero rows (the success path), the INSERT is a no-op and we
-- proceed to DROP the validator.

CREATE TEMP TABLE __w4_fk_guard__ (
    fk_violation_count INTEGER NOT NULL CHECK (fk_violation_count = 0)
);
INSERT INTO __w4_fk_guard__ (fk_violation_count)
SELECT 1 FROM pragma_foreign_key_check;
DROP TABLE __w4_fk_guard__;
