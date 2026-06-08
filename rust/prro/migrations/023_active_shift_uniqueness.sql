-- 023 — active-shift uniqueness (RS-3 C2 / WL-1 foundation)
--
-- Enforce the legal/state invariant "at most ONE active shift per
-- fiscal_number" at the DB level, BEFORE the RS-3 live write-path (C1's
-- shift transition-service) is built on top of it.  Until now the crate
-- migration set gave `shifts` only the NON-unique `ix_shifts_fn_state`
-- (001:50, rebuilt byte-identical at 016:196) — active-shift uniqueness was
-- UNENFORCED.  (The `sql/001_hot_store_init.sql:158` partial-unique index is
-- the unused LEGACY Python tree, not this crate.)
--
-- ACTIVE states (per the M3b 9-state CHECK at 016:113) — locked set:
--   CREATED, OPENING, OPENED_LOCAL_PENDING_DRAIN, OPENED,
--   CLOSING_LOCAL_PENDING_DRAIN, CLOSING.
-- EXCLUDED (terminal / operator-action — unsafe in a unique index):
--   CLOSED, REQUIRES_MANUAL_RECONCILIATION, ERROR.
--
-- FAIL-CLOSED backfill (operator-locked, no auto-resolve): `CREATE UNIQUE
-- INDEX` ITSELF fails loud if a pre-existing duplicate active shift exists,
-- which fails the migration → fails boot.  An operator must reconcile the
-- duplicate manually; the gateway must NOT silently violate the invariant.

CREATE UNIQUE INDEX uq_active_shift_per_fiscal
    ON shifts(fiscal_number)
    WHERE state IN (
        'CREATED',
        'OPENING',
        'OPENED_LOCAL_PENDING_DRAIN',
        'OPENED',
        'CLOSING_LOCAL_PENDING_DRAIN',
        'CLOSING'
    );
