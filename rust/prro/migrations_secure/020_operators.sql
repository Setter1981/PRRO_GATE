-- Migration 020: `operators` table — cashier EDS-key registry (W2).
--
-- Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2
-- (REVISED-v4).  Each row binds one cashier to one fiscal_number and
-- carries the path to their EDS .jks/.dat key file + the obfuscated
-- password (`key_pass_enc`) needed to unlock it at boot.
--
-- ## Hard-isolation context (HIGH-AUDIT-01)
--
-- This table lives in a **separate physical SQLite file**
-- (`var/secure.db` per `DatabaseCfg.secure_db_path`) — NOT the main
-- `var/prro.db` ledger.  The split achieves two properties:
--
--   1. Backup / replication tooling targeting the main ledger never
--      copies the obfuscated password BLOBs — they remain on the
--      protected file with stricter filesystem permissions
--      (chmod 0o600 enforced by `db::open_secure_pool`).
--   2. A misbehaving migration / repository / write-path operation on
--      the main DB CANNOT touch the operators table — no shared
--      transaction, no shared pool, no shared connection.
--
-- The migration runs through `sqlx::migrate!("migrations_secure")` —
-- a distinct directory from the main `migrations/` set so checksum
-- tracking lives in `_sqlx_migrations` of THIS file alone.
--
-- ## Cross-DB foreign-key gap (HIGH-PR90-01)
--
-- The W2 plan + PR #90 senior review require an FK from
-- `operators.fiscal_number` to `fiscal_number_config.fiscal_number`
-- with `ON DELETE RESTRICT`.  SQLite **physically cannot** enforce
-- foreign keys across two attached database files (even via ATTACH
-- DATABASE the FK PRAGMA is per-connection scope), so this constraint
-- is implemented as a **runtime compensating check** in two places:
--
--   - `prro admin add-operator` CLI pre-INSERT: lookup the FN in
--     `pool_main.fiscal_number_config` and refuse if missing.
--   - `BindingsRegistry::build_from_db` boot phase: emit Critical
--     audit `OPERATOR_ORPHAN_FN` for any operators row whose FN
--     does not exist in `fiscal_number_config`; skip the entry.
--
-- These compensating checks land in W2 PR-B (pieces 6 + 7); the
-- migration itself enforces only the structural CHECK constraints
-- listed below.
--
-- ## Constraints (all CHECK-enforced at the DB layer)
--
--   - `fiscal_number`: exactly 10 digits, all numeric (HIGH-PR90-01
--     literal text format).
--   - `is_active`: 0 or 1.
--   - Partial unique index `operators_active_fn_uidx` (MED-PR90-01):
--     at most one row with `is_active = 1` per fiscal_number.
--     Rotation history rows (`is_active = 0`) are unconstrained.

CREATE TABLE operators (
    -- Surrogate `id` primary key.  `operator_id` is the cashier-facing
    -- identifier (typically the cashier's INN per `prro admin
    -- add-operator --inn`); it is intentionally NOT unique on its own
    -- so the same cashier can appear as historical (is_active = 0)
    -- rows across multiple FNs or repeat stints on the same FN.  The
    -- partial unique index `operators_active_fn_uidx` enforces the
    -- only structural uniqueness requirement: exactly one active
    -- cashier per fiscal_number.  See PR-B's repository layer for
    -- the CRUD contract.
    --
    -- Note on rowid reuse: `INTEGER PRIMARY KEY` (without AUTOINCREMENT)
    -- is a rowid alias.  SQLite may reuse a rowid after DELETE once the
    -- table's max rowid is reached, or during VACUUM.  Consumers MUST
    -- NOT rely on `id` for monotonic ordering or as a stable external
    -- identifier — use `created_at` for chronology and `operator_id`
    -- for the cashier identity.  `id` exists purely to give the table
    -- an explicit PK, matching the convention of every main migration
    -- since 010.
    id              INTEGER PRIMARY KEY,
    operator_id     TEXT    NOT NULL,
    fiscal_number   TEXT    NOT NULL
        CHECK (length(fiscal_number) = 10
               AND fiscal_number GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]'),
    name            TEXT    NOT NULL,
    key_path        TEXT    NOT NULL,
    key_pass_enc    BLOB    NOT NULL,
    is_active       INTEGER NOT NULL DEFAULT 1
        CHECK (is_active IN (0, 1)),
    created_at      TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

-- MED-PR90-01: exactly one active cashier per fiscal_number.
-- Historical rows (`is_active = 0`) are NOT covered by this index,
-- so any number of rotations may accumulate.
CREATE UNIQUE INDEX operators_active_fn_uidx
    ON operators (fiscal_number)
    WHERE is_active = 1;

-- Lookup path for `BindingsRegistry::build_from_db` and
-- `find_by_fiscal_number` repository query.
CREATE INDEX operators_fiscal_number_idx
    ON operators (fiscal_number);
