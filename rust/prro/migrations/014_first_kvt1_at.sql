-- 014 — M3b W3: dedicated `first_kvt1_at` column on `fiscal_documents`.
--
-- Replaces PR #45's `updated_at`-as-proxy approach for KVT1 age
-- bucketing in `boot_phase::passive_hold_kvt1`.  The proxy worked
-- under M3a's "no other UPDATE during Kvt1 hold" property + the
-- `fd_updated_at` trigger, but it required tests to drop the trigger
-- before back-dating timestamps and was fragile to future schema
-- changes that might UPDATE rows while in Kvt1.
--
-- W3 introduces an explicit column.  `fiscal_documents::transition_state`
-- (extended in this same task) stamps it via
--   `SET first_kvt1_at = COALESCE(first_kvt1_at, CURRENT_TIMESTAMP)`
-- on every `to == Kvt1` CAS, so:
--   - the column is set on the FIRST Kvt1 entry (Sent → Kvt1);
--   - the column is PRESERVED on idempotent re-entry (e.g. crash-
--     replay where boot recovery touches an already-Kvt1 doc);
--   - the column is NEVER touched on non-Kvt1 transitions.
--
-- Backfill: existing Kvt1 rows pre-migration get the column populated
-- from `updated_at` for forward-compat (under M3a semantics
-- `updated_at` IS the Sent→Kvt1 timestamp because no other UPDATE
-- happens to a doc while it's in Kvt1).  Non-Kvt1 rows keep the
-- column NULL.

ALTER TABLE fiscal_documents ADD COLUMN first_kvt1_at TEXT;

-- Backfill — single statement, only touches Kvt1 rows.  Idempotent
-- under re-application (guarded by `first_kvt1_at IS NULL`).
UPDATE fiscal_documents
   SET first_kvt1_at = updated_at
 WHERE state = 'KVT1' AND first_kvt1_at IS NULL;
