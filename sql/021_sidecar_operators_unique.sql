-- 021_sidecar_operators_unique.sql — Phase 12a hardening.
--
-- sidecar_operators originally had no uniqueness constraint on
-- (fiscal_number, operator_inn) — the admin-UI CRUD (Phase 12a)
-- and the sidecar signing consumer both need exactly one ACTIVE
-- row per (FN, INN): signer identity must be unambiguous
-- (invariant #10).
--
-- A *partial* unique index only over active=1 rows lets us keep
-- historical soft-deleted rows for audit while preventing two
-- active duplicates.

CREATE UNIQUE INDEX ix_sidecar_operators_fn_inn_active
    ON sidecar_operators (fiscal_number, operator_inn)
    WHERE active = 1;
