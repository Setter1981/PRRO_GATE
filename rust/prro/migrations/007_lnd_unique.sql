-- 007 — UNIQUE INDEX on (fiscal_number, lnd) for fiscal_documents.
--
-- Anchored on ADR-M3-A1: `node_state.next_lnd` is the single source of
-- the local numerator; this index is the fail-closed guard against
-- accidental drift (any double-allocation or recovery bug surfaces at
-- INSERT time, not silently downstream).  Spec: docs/superpowers/specs/
-- 2026-05-04-m2-pre-plan-adr.md (ADR-M3-A1) and
-- docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md.
--
-- The non-unique companion `ix_fd_fn_lnd` (migration 002) is left in
-- place; it is now strictly redundant for query plans but its removal
-- is a separate hygiene concern (out of W1 scope).

CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);
