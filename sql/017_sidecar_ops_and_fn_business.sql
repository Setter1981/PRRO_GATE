-- sql/017_sidecar_ops_and_fn_business.sql
-- Sprint: Rust Fiscal Driver v2 (ADR-004 v2).
--
-- Extends fiscal_number_config with business identity + per-FN driver
-- behavior flags. Introduces sidecar_operators (JKS credentials per
-- cashier) and licenses (per-TIN commercial licensing row).
--
-- See docs/PER_FN_CONFIG.md for field semantics.

-- ─── 1. fiscal_number_config additions ──────────────────────────────────────
ALTER TABLE fiscal_number_config ADD COLUMN tax_number             TEXT    NOT NULL DEFAULT '';
ALTER TABLE fiscal_number_config ADD COLUMN fiscal_mode            TEXT    NOT NULL DEFAULT 'test'
    CHECK (fiscal_mode IN ('prod','test'));
ALTER TABLE fiscal_number_config ADD COLUMN national_check_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (national_check_enabled IN (0,1));
ALTER TABLE fiscal_number_config ADD COLUMN offline_enabled        INTEGER NOT NULL DEFAULT 1
    CHECK (offline_enabled IN (0,1));
ALTER TABLE fiscal_number_config ADD COLUMN tsp_enabled            INTEGER NOT NULL DEFAULT 0
    CHECK (tsp_enabled IN (0,1));
ALTER TABLE fiscal_number_config ADD COLUMN org_name               TEXT;
ALTER TABLE fiscal_number_config ADD COLUMN org_address            TEXT;

-- ─── 2. sidecar_operators (1 FN → N cashiers) ───────────────────────────────
CREATE TABLE sidecar_operators (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    fiscal_number  TEXT    NOT NULL,
    operator_name  TEXT,
    operator_inn   TEXT    NOT NULL,     -- 10-digit INN (cashier)
    jks_path       TEXT    NOT NULL,     -- absolute path to JKS / ZS2 / dat container
    jks_password   TEXT    NOT NULL,     -- XOR-soft obfuscated hex OR plain text
                                         -- (see credentials_mode in sidecar.toml)
    active         INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
    created_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number)
);

CREATE INDEX ix_sidecar_operators_fn
    ON sidecar_operators (fiscal_number, active);
CREATE INDEX ix_sidecar_operators_active
    ON sidecar_operators (active) WHERE active = 1;

-- ─── 3. licenses (single active row per install) ─────────────────────────────
CREATE TABLE licenses (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    tin              TEXT    NOT NULL,       -- EDRPOU / INN of licensee
    fn_numbers_json  TEXT    NOT NULL,       -- JSON array of allowed FNs
    issued_at        TEXT    NOT NULL,       -- ISO-8601 UTC
    expires_at       TEXT    NOT NULL,       -- ISO-8601 UTC
    tier             TEXT    NOT NULL
        CHECK (tier IN ('demo','basic','pro','enterprise')),
    org_name         TEXT,
    demo_limits_json TEXT,                   -- NULL for paid tiers
    payload_b64      TEXT    NOT NULL,       -- base64 of JCS-canonical license JSON
    signature_b64    TEXT    NOT NULL,       -- base64 of detached DSTU signature
    installed_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    active           INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1))
);

-- At most one active license at a time.
CREATE UNIQUE INDEX ix_licenses_active_single
    ON licenses(active) WHERE active = 1;
