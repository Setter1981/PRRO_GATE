-- 009_cash_balance.sql: Cash balance settings per PRRO.
--
-- last_cash_balance: carry-over snapshot written at Z_REPORT boundary only.
--   Used for SHIFT_OPEN SM when cash_balance_mode='preserve'.
--   NOT a running counter. Current balance = derived from fiscal_documents.
--
-- cash_balance_mode: 'preserve' = carry over to next shift, 'reset' = 0 after Z.
-- rounding_enabled: local gate for NBU rounding (Постанова №115, 01.10.2025).
-- print_zero_change: include "change: 0" in response when change is zero.

ALTER TABLE node_state ADD COLUMN last_cash_balance INTEGER NOT NULL DEFAULT 0 CHECK (last_cash_balance >= 0);
ALTER TABLE node_state ADD COLUMN cash_balance_mode TEXT NOT NULL DEFAULT 'preserve' CHECK (cash_balance_mode IN ('preserve', 'reset'));
ALTER TABLE node_state ADD COLUMN rounding_enabled INTEGER NOT NULL DEFAULT 0 CHECK (rounding_enabled IN (0, 1));
ALTER TABLE node_state ADD COLUMN print_zero_change INTEGER NOT NULL DEFAULT 0 CHECK (print_zero_change IN (0, 1));
