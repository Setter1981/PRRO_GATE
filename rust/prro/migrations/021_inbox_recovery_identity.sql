-- 021 — ingress_inbox recovery identity (RS-2 A-H1)
--
-- The write-path SEAM (runtime::ingress::seam::WritePathEntry::fiscalize)
-- receives ONLY the persisted InboxRow.  A crash-recovery reaper that
-- re-drives a stuck PROCESSING row after a restart sees ONLY that row —
-- not the listener/runtime context the inline handler had.  So the row
-- must be a SELF-CONTAINED recovery record carrying the identity the
-- write-path consumes at sign time:
--   - driver_id            → drives driver_tax_mapping letter→canonical
--                            tax-group translation + outgress routing.  A
--                            missing driver_id silently identity-maps tax
--                            groups (the W4-Z2a non-identity hazard).
--   - signed_by_cashier_id → fiscal cashier attribution on the receipt.
--
-- Both NULLABLE for additive backward-compat: pre-021 rows have no value.
-- RS-3 MUST fail closed on a missing driver_id for any row it processes
-- (by then it is not recoverable from anywhere else).  The handler
-- populates BOTH for every new row — driver_id from the listener-stamped
-- config, signed_by_cashier_id from the VALIDATED command (not the raw
-- wire string).
--
-- Idempotency UNCHANGED: neither column enters the ux_inbox_fn_idem
-- (fiscal_number, idempotency_key) uniqueness key, nor the
-- payload_sha256_canonical conflict discriminator.

ALTER TABLE ingress_inbox ADD COLUMN signed_by_cashier_id TEXT;
ALTER TABLE ingress_inbox ADD COLUMN driver_id TEXT;
