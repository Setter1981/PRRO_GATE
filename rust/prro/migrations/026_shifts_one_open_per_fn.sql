-- A′.1 piece 0b — fail-closed backstop for the "one open shift per FN" invariant.
--
-- The shift-create path (`services::shift::transition::create_shift_tx`) CASes
-- `node_state` CLOSED→CREATED under the FN single-writer lease, but a partial
-- UNIQUE index guarantees at the SCHEMA level that at most one `shifts` row per
-- `fiscal_number` is in an OPEN (non-terminal) state — catching any race or
-- CAS-bypass (defence in depth; `ix_shifts_fn_state` is only a non-unique lookup
-- index).
--
-- Open (non-terminal) states = the 6 lifecycle-active shift states. The terminal
-- states CLOSED / REQUIRES_MANUAL_RECONCILIATION / ERROR are excluded so a FN can
-- open a fresh shift after a prior one closes or is quarantined. Vocabulary matches
-- the 9-state `shifts.state` CHECK (001_baseline.sql) and `ShiftState` (enums.rs).
CREATE UNIQUE INDEX ux_shifts_one_open_per_fn
    ON shifts(fiscal_number)
    WHERE state IN (
        'CREATED',
        'OPENING',
        'OPENED_LOCAL_PENDING_DRAIN',
        'OPENED',
        'CLOSING_LOCAL_PENDING_DRAIN',
        'CLOSING'
    );
