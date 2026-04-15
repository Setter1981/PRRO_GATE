-- 004: Add content column to document_files for persisting actual file content.
--
-- Previously document_files stored only metadata (path, sha256, size_bytes).
-- The actual content (e.g. signed XML payload) was ephemeral in WorkerContext.
-- This column enables transport-neutral egress recovery: offline sync can read
-- the persisted signed payload instead of relying on Checkbox-specific re-send.
--
-- Simple ALTER TABLE ADD COLUMN — no table rebuild required for nullable column.

ALTER TABLE document_files ADD COLUMN content BLOB;
