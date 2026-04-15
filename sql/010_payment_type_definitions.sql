-- 010_payment_type_definitions.sql: Payment type registry per PRRO.
--
-- Each PRRO/fiscal number has its own configured payment types.
-- POS sends type_code, gateway resolves type_group (T in XML) and name (NM).
--
-- Policy: CASH is hardcoded canonical (T=0, NM="Готівка").
-- All other types must be configured here or receipt is rejected before sign.
-- No silent fallback.
--
-- type_group per ФСКО:
--   0 = готівка
--   1 = інший засіб оплати (сертифікат, бонус)
--   2 = безготівковий платіжний інструмент (картка)

CREATE TABLE payment_type_definitions (
    fiscal_number    TEXT NOT NULL,
    type_code        TEXT NOT NULL,
    type_group       INTEGER NOT NULL CHECK (type_group IN (0, 1, 2)),
    name             TEXT NOT NULL,
    is_active        INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (fiscal_number, type_code)
);
