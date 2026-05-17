-- 016_shift_state_expansion.sql — M3b shift state machine expansion.
--
-- Lands the 9-state ShiftState CHECK constraint on both `shifts` AND
-- `node_state` (mirror invariant), plus three new columns:
--   * shifts.opened_by_cashier_id   TEXT NULLABLE  (no FK in W14a-1)
--   * shifts.closed_by_cashier_id   TEXT NULLABLE  (no FK in W14a-1)
--   * operator_certs.deferred       INTEGER NOT NULL DEFAULT 0
--
-- Spec reference: `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md`
--   §3.1   — 9-state enum table.
--   §9.2   — migration shape: shifts rebuild (15-step W4 dance with
--            fd_updated_at suppression) + node_state rebuild (8-step).
--   §16.8  — 1-cashier-per-shift invariant (REVERSES Round 6 B-M5).
--   §16.10 — 36h cert-expiry SHIFT_OPEN gate + deferred key swap.
--   §16.17 — consolidated schema additions for migration 016.
--
-- W14a-1 scope = SCHEMA + ENUM ONLY.  Repository whitelist edges, force
-- seams, scanner tests, boot bridge contract land in W14a-2 + W14a-3 per
-- operator split decision (2026-05-17).
--
-- W4 NULL-FK dance rationale (pinned in 015:96-121):
--   fiscal_documents.shift_id has FK to shifts(shift_id) with ON DELETE
--   RESTRICT (per 008:118).  PRAGMA defer_foreign_keys defers FK
--   *validation* to COMMIT, but FK *actions* (RESTRICT) fire at statement
--   time.  So DROP TABLE shifts on a populated DB with any
--   fiscal_documents.shift_id IS NOT NULL row would fail the RESTRICT
--   immediately.  Required workflow:
--     1. snapshot fd_updated_at trigger DDL for byte-identical restore
--     2. DROP TRIGGER fd_updated_at (so bookkeeping UPDATEs don't touch
--        fiscal_documents.updated_at — historical audit trail integrity)
--     3. stash fiscal_documents.shift_id into temp column
--     4. NULL the original column
--     5. snapshot shifts into a no-FK temp table
--     6. DROP shifts (succeeds: no children point at it)
--     7. CREATE shifts SAME NAME (9-state + 2 new cashier columns)
--     8. restore rows
--     9. restore fiscal_documents.shift_id from temp column
--    10. drop temp column
--    11. recreate fd_updated_at trigger byte-identical
--    12. recreate shifts_updated_at trigger byte-identical
--    13. recreate ix_shifts_fn_state index byte-identical
--
-- node_state has no inbound FK on shift_state — simpler rebuild
-- (snapshot/drop/create/restore + recreate trigger).
--
-- operator_certs gets a column-add only (no rebuild) — `deferred` boolean
-- (INTEGER 0/1 for STRICT mode) mirrors the existing `active` flag.  Auto-
-- swap mechanism (§16.10) implemented in W14a-2 repository layer.

PRAGMA defer_foreign_keys = ON;

-- ─── Fail-closed FK guard (W4 lesson, prep) ──────────────────────────
-- Temp table with CHECK constraint that fails the migration tx if
-- pragma_foreign_key_check finds any violations at end-of-migration.
CREATE TEMP TABLE __m016_fk_guard__ (
    fk_violation_count INTEGER CHECK (fk_violation_count = 0)
);
-- INSERT happens at the end of this migration; CHECK fires there.


-- ─── shifts rebuild — W4 NULL-FK + fd_updated_at suppression dance ───

-- Step 1: drop the `fd_updated_at` AFTER-UPDATE trigger.  Our bookkeeping
-- UPDATEs on fiscal_documents (steps 3 + 9 below) MUST NOT mutate
-- historical `updated_at` timestamps for unrelated documents.  Trigger is
-- recreated byte-identically at step 11.
DROP TRIGGER fd_updated_at;

-- Step 2: temp holding column on fiscal_documents for the inbound FK
-- values.  ALTER TABLE ADD COLUMN cannot include a FK constraint —
-- exactly what we want here (stash does NOT participate in FK enforcement).
ALTER TABLE fiscal_documents ADD COLUMN _w14a1_shift_id_tmp BLOB;

-- Step 3: stash + null the FK column so DROP TABLE shifts below can
-- succeed without triggering RESTRICT.  Single UPDATE copies + clears in
-- one pass.  `fd_updated_at` is dropped (step 1) so this UPDATE does NOT
-- touch fiscal_documents.updated_at.
UPDATE fiscal_documents
   SET _w14a1_shift_id_tmp = shift_id,
       shift_id            = NULL
 WHERE shift_id IS NOT NULL;

-- Step 4: snapshot shifts into a no-FK / no-CHECK temp table.
-- `CREATE TABLE … AS SELECT *` infers types but does NOT carry CHECK,
-- PK, UNIQUE, or FK — exactly the "non-FK holding table" pattern from
-- 008 + 015.
CREATE TABLE __w14a1_shifts_tmp__ AS SELECT * FROM shifts;

-- Step 5: drop the old shifts table.  Inbound FK from fiscal_documents
-- was nulled at step 3.  Outbound FK from shifts to fiscal_number_config
-- doesn't block DROP (it's a reference shifts holds, not a child of shifts).
DROP TABLE shifts;

-- Step 6: drop the shifts_updated_at trigger (already orphaned by step 5
-- but explicit drop is cleaner).  Recreated byte-identically at step 12.
-- DROP TRIGGER IF EXISTS is safe — if SQLite already removed it implicitly
-- with the table, the IF EXISTS clause prevents an error.
DROP TRIGGER IF EXISTS shifts_updated_at;

-- Step 7: recreate shifts under the SAME name with the M3b shape:
--   * 9-state CHECK constraint (3 new states added).
--   * opened_by_cashier_id TEXT NULLABLE (no FK in W14a-1; W14a-2 may
--     promote to NOT NULL once 1-cashier-per-shift enforcement lands).
--   * closed_by_cashier_id TEXT NULLABLE (for senior cashier close path
--     per §16.9 — value differs from opened_by_cashier_id when senior
--     closes another cashier's shift).
-- Same-name recreation (NOT _new + RENAME) avoids deferred-FK-validation
-- breakage documented in 008.
CREATE TABLE shifts (
    shift_id               BLOB    PRIMARY KEY  CHECK (length(shift_id) = 16),
    fiscal_number          TEXT    NOT NULL,
    serial                 INTEGER,
    state                  TEXT    NOT NULL  CHECK (state IN (
        'CREATED',
        'OPENING',
        'OPENED_LOCAL_PENDING_DRAIN',
        'OPENED',
        'CLOSING_LOCAL_PENDING_DRAIN',
        'CLOSING',
        'CLOSED',
        'REQUIRES_MANUAL_RECONCILIATION',
        'ERROR'
    )),
    open_mode              TEXT    NOT NULL  CHECK (open_mode IN ('ONLINE','OFFLINE')),
    opened_at              TEXT,
    closed_at              TEXT,
    open_document_id       BLOB,
    close_document_id      BLOB,
    z_report_document_id   BLOB,
    cash_balance_kop       INTEGER NOT NULL DEFAULT 0,
    opened_by_cashier_id   TEXT,                        -- NEW (W14a-1): cashier identity, NULL allowed in W14a-1
    closed_by_cashier_id   TEXT,                        -- NEW (W14a-1): senior cashier close support per §16.9
    created_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

-- Step 8: restore rows from snapshot.  Identity map for the 6 existing
-- state values (CREATED / OPENING / OPENED / CLOSING / CLOSED / ERROR);
-- new columns default to NULL.  Pre-pilot has zero rows but the dance
-- MUST work for populated DBs too (fixture test pinned in spec §9.2
-- acceptance).
INSERT INTO shifts (
    shift_id, fiscal_number, serial, state, open_mode, opened_at, closed_at,
    open_document_id, close_document_id, z_report_document_id, cash_balance_kop,
    opened_by_cashier_id, closed_by_cashier_id, created_at, updated_at
)
SELECT
    shift_id, fiscal_number, serial, state, open_mode, opened_at, closed_at,
    open_document_id, close_document_id, z_report_document_id, cash_balance_kop,
    NULL AS opened_by_cashier_id,     -- back-fill: pre-W14a-1 had no cashier identity captured
    NULL AS closed_by_cashier_id,     -- same
    created_at, updated_at
FROM __w14a1_shifts_tmp__;

-- Step 9: restore the inbound FK column on fiscal_documents.  At this
-- point fiscal_documents.shift_id references the rebuilt shifts table
-- (same name, same row data).  `defer_foreign_keys=ON` defers validation
-- to COMMIT.  The `fd_updated_at` trigger remains dropped here so this
-- restore does NOT touch fiscal_documents.updated_at.
UPDATE fiscal_documents
   SET shift_id = _w14a1_shift_id_tmp
 WHERE _w14a1_shift_id_tmp IS NOT NULL;

-- Step 10: drop the temp column on fiscal_documents.  Requires SQLite ≥
-- 3.35 (DROP COLUMN); libsqlite3-sys 0.30 bundles 3.46+ per 015 lesson.
ALTER TABLE fiscal_documents DROP COLUMN _w14a1_shift_id_tmp;

-- Step 11: recreate `fd_updated_at` trigger byte-identical to the live
-- definition from 008:154 + 015:196.  Per-doc updated_at refresh on UPDATE
-- resumes normal post-migration runtime behavior.
CREATE TRIGGER fd_updated_at
AFTER UPDATE ON fiscal_documents
BEGIN
    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;
END;

-- Step 12: recreate `shifts_updated_at` trigger byte-identical to the live
-- definition from 001:55-59.
CREATE TRIGGER shifts_updated_at
AFTER UPDATE ON shifts
BEGIN
    UPDATE shifts SET updated_at = CURRENT_TIMESTAMP WHERE shift_id = NEW.shift_id;
END;

-- Step 13: recreate `ix_shifts_fn_state` index byte-identical to the live
-- definition from 001:53.
CREATE INDEX ix_shifts_fn_state ON shifts(fiscal_number, state);

-- Step 14: drop the shifts snapshot.
DROP TABLE __w14a1_shifts_tmp__;


-- ─── node_state rebuild — no inbound FK, simpler dance ──────────────

-- Step 15: snapshot node_state into a no-FK / no-CHECK temp table.
CREATE TABLE __w14a1_node_state_tmp__ AS SELECT * FROM node_state;

-- Step 16: drop the node_state_updated_at trigger (orphaned by step 17).
DROP TRIGGER IF EXISTS node_state_updated_at;

-- Step 17: drop node_state.  No inbound FK to remove — node_state is
-- terminal in the FK graph.
DROP TABLE node_state;

-- Step 18: recreate node_state with the 9-state CHECK on shift_state.
-- All other columns identical to the 001 definition + the `next_z_report_number`
-- column added by migration 009.  No schema changes beyond the shift_state
-- CHECK extension — same column list, same defaults, same FK.
CREATE TABLE node_state (
    fiscal_number               TEXT    PRIMARY KEY,
    mode                        TEXT    NOT NULL  CHECK (mode IN ('ONLINE','GOING_OFFLINE','OFFLINE','GOING_ONLINE','BLOCKED','STOP_MODE','CRYPTO_DEGRADED')),
    shift_state                 TEXT    NOT NULL  CHECK (shift_state IN (
        'CREATED',
        'OPENING',
        'OPENED_LOCAL_PENDING_DRAIN',
        'OPENED',
        'CLOSING_LOCAL_PENDING_DRAIN',
        'CLOSING',
        'CLOSED',
        'REQUIRES_MANUAL_RECONCILIATION',
        'ERROR'
    )),
    current_shift_id            BLOB,
    current_offline_session_id  BLOB,
    next_lnd                    INTEGER NOT NULL  CHECK (next_lnd >= 1),
    backend_profile_id          TEXT,
    transport_profile_id        TEXT,
    readiness_state             TEXT    NOT NULL DEFAULT 'STARTING'  CHECK (readiness_state IN ('STARTING','RECOVERING','READY','DEGRADED','STOPPED')),
    recovery_stage              TEXT    NOT NULL DEFAULT 'BOOT'  CHECK (recovery_stage IN ('BOOT','PHASE1','PHASE2','DONE','FAILED')),
    current_month_offline_seconds INTEGER NOT NULL DEFAULT 0,
    last_known_unsigned_xml_sha256 BLOB  CHECK (last_known_unsigned_xml_sha256 IS NULL OR length(last_known_unsigned_xml_sha256) = 32),
    last_fs_ping_at             TEXT,
    updated_at                  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    next_z_report_number        INTEGER NOT NULL DEFAULT 1  CHECK (next_z_report_number >= 1),  -- added by 009
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;

-- Step 19: restore rows.  Identity map for all 6 existing shift_state
-- values; no other column changes.
INSERT INTO node_state SELECT * FROM __w14a1_node_state_tmp__;

-- Step 20: recreate node_state_updated_at trigger byte-identical to the
-- live definition from 001:79.
CREATE TRIGGER node_state_updated_at
AFTER UPDATE ON node_state
BEGIN
    UPDATE node_state SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;
END;

-- Step 21: drop the node_state snapshot.
DROP TABLE __w14a1_node_state_tmp__;


-- ─── operator_certs extension — deferred replacement key support ────
-- Mirrors the existing `active` flag pattern from 003.  When an operator
-- pre-registers a backup cert for the same fiscal_number, gateway W14a-2
-- KeyRotationPending recovery class (§16.3) checks deferred=1 at SHIFT_OPEN
-- time and auto-swaps if primary cert is within 36h of expiry (§16.10).
ALTER TABLE operator_certs ADD COLUMN deferred INTEGER NOT NULL DEFAULT 0  CHECK (deferred IN (0, 1));

-- Partial unique index — at most one deferred cert per fiscal_number at any
-- given time.  Mirrors `ux_op_certs_active_per_fn` from 003.
CREATE UNIQUE INDEX ux_op_certs_deferred_per_fn
    ON operator_certs(fiscal_number) WHERE deferred = 1;


-- ─── FK guard exit ──────────────────────────────────────────────────
-- INSERT into the fail-closed temp guard.  count(*) FROM pragma_foreign_key_check
-- returns rows for any FK violations remaining at end-of-migration (after
-- defer_foreign_keys validation kicks in at the surrounding COMMIT).  If
-- count != 0, CHECK constraint on the temp table fails the migration
-- before the surrounding sqlx tx commits.
INSERT INTO __m016_fk_guard__ (fk_violation_count)
SELECT count(*) FROM pragma_foreign_key_check;
