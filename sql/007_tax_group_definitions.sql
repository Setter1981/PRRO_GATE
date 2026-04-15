-- 007_tax_group_definitions.sql: Tax group definitions per fiscal number.
--
-- Each PRRO/fiscal number has its own set of tax groups registered with DPS.
-- Groups define VAT rate, excise (additional charge) rate, algorithm,
-- and compliance requirements (UKTZED, excise marks).
--
-- POS sends only tax_id (group number) per item. Gateway resolves
-- rates and validates compliance from this table.

CREATE TABLE tax_group_definitions (
    fiscal_number       TEXT NOT NULL,
    tax_id              TEXT NOT NULL,
    name                TEXT NOT NULL,
    tax_rate            REAL NOT NULL DEFAULT 0,
    additional_rate     REAL NOT NULL DEFAULT 0,
    tax_type            INTEGER NOT NULL DEFAULT 0,
    tax_algorithm       INTEGER NOT NULL DEFAULT 0,
    requires_uktzed     INTEGER NOT NULL DEFAULT 0,
    requires_excise_mark INTEGER NOT NULL DEFAULT 0,
    is_active           INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (fiscal_number, tax_id)
);
