-- 022 — ingress_inbox recovery: business_ts + total_sum_kop (RS-2 A-H1 follow-up)
--
-- Completes migration 021's "InboxRow is a SELF-CONTAINED recovery record"
-- intent.  Beyond identity (driver_id / signed_by_cashier_id), RS-3's
-- `fiscalize` builds a `CanonicalFiscalCommand` whose `stage_acquire`/`stage_sign`
-- consume TWO more fields the mapper computed and then dropped:
--   - business_ts    → the receipt/document timestamp.  stage_send converts it
--                      to the DPS Kyiv-local epoch (the doc's TS on the wire) and
--                      it is persisted on `fiscal_documents`.  The mapper mints
--                      it as `Utc::now()` at INGEST; a crash-recovery reaper that
--                      re-drives a stuck PROCESSING row (no `fiscal_documents`
--                      row yet) would otherwise re-mint a LATER `now()` —
--                      stamping the receipt with the recovery time, not the sale
--                      time (first-pass vs replay drift + wrong DPS TS).
--   - total_sum_kop  → the wire's DECLARED total (SELL→sale, RETURN→return).
--                      stage_sign cross-checks the converted CheckJson sum
--                      AGAINST it; recomputing it from the payload would make the
--                      check vacuous.  NULL for SHIFT_OPEN / Z (no total).
--
-- Both NULLABLE for additive backward-compat (pre-022 rows read NULL).  The
-- handler populates business_ts for every new row; total_sum_kop only for
-- SELL/RETURN.  Idempotency UNCHANGED: neither column enters the
-- ux_inbox_fn_idem key nor the payload_sha256_canonical discriminator.

ALTER TABLE ingress_inbox ADD COLUMN business_ts TEXT;
ALTER TABLE ingress_inbox ADD COLUMN total_sum_kop INTEGER;
