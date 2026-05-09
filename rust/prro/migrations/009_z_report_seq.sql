-- 009 — Z-report sequencer + signing-inputs pin marker (W6).
--
-- Anchored on ADR-M3-A2 (CloseShift→ZReport at builder; Z-allocation
-- by wire_artifact_kind) and the W6 design spec
-- (docs/superpowers/specs/2026-05-09-m3a-w6-stage3-sign-design.md §5.1).
--
-- Three additive columns + one partial UNIQUE index:
--
--   1. node_state.next_z_report_number — allocator state, mirrors the
--      next_lnd allocator from W1.  CHECK (>=1) catches wraparound.
--      The existing `node_state_updated_at` trigger (migration 001)
--      auto-bumps `updated_at` on every UPDATE — bare CAS UPDATE in
--      the Rust allocator suffices for parity with W1
--      `allocate_next_lnd`.
--
--   2. fiscal_documents.z_report_number — persisted Z-allocation per
--      doc.  NULL for non-Z artifacts.  The W6 retry-safety property
--      (sign-fail → retry reuses same z_report_number, no second
--      advance, no gap) requires the value live on the document row,
--      not in volatile memory.  CHECK (>=1) catches accidental zero
--      writes.
--
--   3. fiscal_documents.signing_inputs_pinned_at — pin-once flag,
--      ISO8601 timestamp.  Disambiguates two states the previous_hash
--      column alone cannot:
--        - "not pinned yet"   (signing_inputs_pinned_at IS NULL)
--        - "pinned with empty/None previous_hash" (legitimate first-
--          doc-after-bootstrap case where node_state.last_known_
--          unsigned_xml_sha256 IS NULL at pin time)
--      The W6 stage 3-PRE pin UPDATE is WHERE-guarded on
--      signing_inputs_pinned_at IS NULL (pin-once, idempotent under
--      concurrent re-entry).
--
--   4. ux_fd_fn_zrn — partial UNIQUE on (fiscal_number, z_report_number)
--      WHERE z_report_number IS NOT NULL.  Mirrors the W1 fail-closed
--      pattern (ux_fd_fn_lnd): any double-allocation or recovery bug
--      surfaces at INSERT/UPDATE time, not silently downstream.
--
-- All four are pure additive ALTERs / CREATE; no table rebuild
-- required (cf. migration 008 which had to rebuild fiscal_documents
-- to extend a CHECK list).  ALTER ADD COLUMN with constant DEFAULT
-- and CHECK is supported in SQLite 3.35+ (verified vs SQLite 3.45
-- docs).

ALTER TABLE node_state
  ADD COLUMN next_z_report_number INTEGER NOT NULL DEFAULT 1
  CHECK (next_z_report_number >= 1);

ALTER TABLE fiscal_documents
  ADD COLUMN z_report_number INTEGER
  CHECK (z_report_number IS NULL OR z_report_number >= 1);

ALTER TABLE fiscal_documents
  ADD COLUMN signing_inputs_pinned_at TEXT;

CREATE UNIQUE INDEX ux_fd_fn_zrn
  ON fiscal_documents(fiscal_number, z_report_number)
  WHERE z_report_number IS NOT NULL;
