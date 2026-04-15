-- 005: Add Z-report numbering persistence seam.
--
-- node_state.next_z_report_number: monotonic counter per fiscal_number, starts at 1.
-- fiscal_documents.z_report_number: allocated Z number persisted on the document
-- so retries/recovery keep the same value.
--
-- Simple ALTER TABLE ADD COLUMN — no table rebuild required.

ALTER TABLE node_state ADD COLUMN next_z_report_number INTEGER NOT NULL DEFAULT 1;
ALTER TABLE fiscal_documents ADD COLUMN z_report_number INTEGER;
