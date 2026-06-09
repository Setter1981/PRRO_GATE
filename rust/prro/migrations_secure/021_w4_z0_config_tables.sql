-- Migration 021: W4-Z0 per-FN configuration tables.
--
-- Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
-- §1.  Adds 5 new tables to `var/secure.db` for the M4 conversion
-- layer + outgress routing:
--
--   * `tax_groups`           — per-FN PDV/excise rates per TX number
--                              + letter label (operator-readable)
--   * `payment_methods`      — per-FN configured payment forms
--                              (extends WebCheck-style defaults)
--   * `driver_tax_mapping`   — per-driver-vendor (Maria, WebCheck,
--                              Eccelio, ...) translation table
--                              `driver_number ↔ canonical_tx_num`
--   * `fn_integration_flags` — per-FN feature flags (Національний чек,
--                              future others)
--   * `fn_outgress_profile`  — per-FN outgress protocol selection
--                              (FSCO_ZZD pilot / EVPZ_DPS post-pilot)
--
-- ## Hard-isolation context (HIGH-AUDIT-01)
--
-- All 5 tables live in `var/secure.db` (NOT main `var/prro.db`) for
-- the same reasons enumerated in migration `020_operators.sql`: the
-- operator-config surface is privacy-relevant and must not flow
-- through backup tooling targeting the ledger.  Pool accessor:
-- `App::db_secure()`.  Migration dir: `migrations_secure/` (distinct
-- from main `migrations/` dir; checksums tracked in this file's
-- `_sqlx_migrations`).
--
-- ## Cross-DB FK gap (same pattern as 020)
--
-- The `fn` column references `fiscal_number_config.fiscal_number`
-- (in the main ledger DB).  SQLite **cannot** enforce FK across
-- attached DBs — implemented as runtime compensating check in the
-- admin CLI commands + `BindingsRegistry::build_from_db` boot phase
-- (same pattern as W2 PR-B).
--
-- ## Constraints summary
--
--   tax_groups:
--     * `tx_num` IN 1..=99 (CHECK)
--     * `txal`   IN 0..=3  (CHECK — tax algorithm per FSCO spec)
--     * `is_active` IN (0, 1)
--     * Partial unique idx on `(fn, letter)` WHERE `is_active = 1`
--       — at most one active row per letter per FN; rotation history
--       is unconstrained.
--
--   payment_methods:
--     * `pay_index` IN 0..=99
--     * `iscash`    IN (0, 1)
--     * `is_active` IN (0, 1)
--     * Partial unique idx on `(fn, name)` WHERE `is_active = 1`.
--
--   driver_tax_mapping:
--     * `canonical_tx_num` IN 1..=99
--     * `is_active` IN (0, 1)
--
--   fn_outgress_profile:
--     * `profile` IN ('FSCO_ZZD', 'EVPZ_DPS') — typed CHECK enforces
--       the architectural pin per `project_m4_outgress_architecture`
--       memory.

-- ───────────────────────────────────────────────────────────────────
-- Table: tax_groups
-- ───────────────────────────────────────────────────────────────────
CREATE TABLE tax_groups (
    fn         TEXT    NOT NULL,
    tx_num     INTEGER NOT NULL CHECK (tx_num BETWEEN 1 AND 99),
    letter     TEXT    NOT NULL,
    dtpr       REAL    NOT NULL DEFAULT 0.0,
    txpr       REAL    NOT NULL DEFAULT 0.0,
    txal       INTEGER NOT NULL DEFAULT 0 CHECK (txal BETWEEN 0 AND 3),
    txty       INTEGER NOT NULL DEFAULT 0,
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (fn, tx_num)
) STRICT;

CREATE UNIQUE INDEX idx_tax_groups_fn_letter
    ON tax_groups (fn, letter)
    WHERE is_active = 1;

-- ───────────────────────────────────────────────────────────────────
-- Table: payment_methods
-- ───────────────────────────────────────────────────────────────────
CREATE TABLE payment_methods (
    fn         TEXT    NOT NULL,
    -- pay_index BETWEEN 1 AND 99: 0 is intentionally rejected because the
    -- XML `<M T=...>` value is derived as `pay_index - 1` (per
    -- 2026-05-26-w4-z1-dps-xml-wire-shape spec §M element + WebCheck
    -- StringXML.cs:1014).  A pay_index=0 would emit T="-1" which is
    -- semantically invalid.  Audit Round-1 finding (2026-05-27).
    pay_index  INTEGER NOT NULL CHECK (pay_index BETWEEN 1 AND 99),
    name       TEXT    NOT NULL,
    iscash     INTEGER NOT NULL DEFAULT 0 CHECK (iscash IN (0, 1)),
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (fn, pay_index)
) STRICT;

CREATE UNIQUE INDEX idx_payment_methods_fn_name
    ON payment_methods (fn, name)
    WHERE is_active = 1;

-- ───────────────────────────────────────────────────────────────────
-- Table: driver_tax_mapping
-- ───────────────────────────────────────────────────────────────────
CREATE TABLE driver_tax_mapping (
    driver_id        TEXT    NOT NULL,
    driver_number    INTEGER NOT NULL,
    driver_letter    TEXT,
    canonical_tx_num INTEGER NOT NULL CHECK (canonical_tx_num BETWEEN 1 AND 99),
    is_active        INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at       TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (driver_id, driver_number)
) STRICT;

-- ───────────────────────────────────────────────────────────────────
-- Table: fn_integration_flags
-- ───────────────────────────────────────────────────────────────────
CREATE TABLE fn_integration_flags (
    fn         TEXT NOT NULL,
    flag_name  TEXT NOT NULL,
    flag_value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (fn, flag_name)
) STRICT;

-- ───────────────────────────────────────────────────────────────────
-- Table: fn_outgress_profile
-- ───────────────────────────────────────────────────────────────────
CREATE TABLE fn_outgress_profile (
    fn         TEXT NOT NULL PRIMARY KEY,
    profile    TEXT NOT NULL CHECK (profile IN ('FSCO_ZZD', 'EVPZ_DPS')),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
