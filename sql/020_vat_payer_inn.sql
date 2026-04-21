-- 020_vat_payer_inn.sql — Phase 9.
--
-- Some merchants are registered as VAT payers and must print their
-- 12-digit ІПН платника ПДВ alongside (but distinct from) ЄДРПОУ on
-- every receipt.  ЄДРПОУ (tax_number) is identity of the legal entity
-- and never changes; VAT IPN comes and goes with registration status,
-- so it is a separate NULLable column editable through the admin UI.
--
-- Conservative check: exactly 12 digits or NULL.  Empty string is
-- normalised to NULL by the handler — do NOT allow '' to live here.

ALTER TABLE fiscal_number_config
    ADD COLUMN vat_payer_inn TEXT
    CHECK (vat_payer_inn IS NULL OR vat_payer_inn GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]');
