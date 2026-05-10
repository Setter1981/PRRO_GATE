-- 012 — transport_trace.retry_class durable encoding (W10.2 review fix-up).
--
-- Anchored on:
--   * W10 design freeze v3.1 §3 (RoutingDecision.retry_class)
--   * docs/superpowers/specs/2026-05-10-m3a-w10-dps-dispatch-design.md §4.2
--     "Caller obligation — retry-loop policy" addendum
--
-- Why:
--   `stage_send.rs` module docstring + freeze §4.2 oblige the worker
--   dispatcher (W11+) to filter docs in `ErrorRetryable` by the
--   `RetryClass` of their last attempt: only `TransientRetry` warrants
--   another wire send; `FnConfigError` / `WrapperBug` / `ProbeRequired`
--   / `MacRecovery` / `OperatorEscalation` would crash-loop without
--   higher-layer orchestration.  Without a durable column the
--   dispatcher would have to reconstruct `retry_class` from
--   `(outcome_kind, error_kind, server_status_code)` — a lossy inverse
--   of the routing fn that duplicates the W10.1 dispatch table and
--   silently goes stale when the routing surface shifts.
--
--   `retry_class` is added as a nullable TEXT column.  Stage 4-b
--   `complete_tx` writes it on the routed arm (`WireDecision::Routed`)
--   alongside the existing forensic columns.  Sent arm leaves it NULL
--   (the success path has no retry class).
--
-- Encoding:
--   The seven `RetryClass` variants are encoded as their PascalCase
--   variant tags via `RetryClass::as_str()`:
--     - 'TransientRetry'
--     - 'TerminalReject'
--     - 'FnConfigError'
--     - 'WrapperBug'
--     - 'ProbeRequired'
--     - 'MacRecovery'
--     - 'OperatorEscalation'
--   No DDL CHECK constraint — adding a CHECK to an existing SQLite
--   column requires a table rebuild, and the closed-enum invariant is
--   already enforced by `RetryClass::as_str()` at the writer side and
--   `RetryClass::from_wire_str()` at the reader side.  W10.4 / W10.5
--   may opt to add a CHECK during the next table rebuild driven by an
--   unrelated schema change.
--
-- Backwards compatibility:
--   - Existing rows have retry_class = NULL.  W9 reconciliation must
--     treat NULL as "indeterminate from durable evidence; reconstruct
--     from outcome_kind + error_kind + server_status_code if needed".
--   - The `(completed_at IS NULL XOR outcome_kind IS NULL)` invariant
--     from migration 010 is preserved — retry_class is orthogonal to
--     completion state.

ALTER TABLE transport_trace
    ADD COLUMN retry_class TEXT NULL;

-- Helpful index for the dispatcher's "last attempt for this doc"
-- read.  `attempt_no` already has implicit ordering through the
-- (document_id, attempt_no) PK; this index lets us read just the last
-- attempt without a full row scan when only retry_class is needed.
CREATE INDEX IF NOT EXISTS idx_transport_trace_doc_retry_class
    ON transport_trace(document_id, attempt_no DESC, retry_class);
