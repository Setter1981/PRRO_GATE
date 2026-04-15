-- 008_excise_allowed.sql: Master switch for excise operations per PRRO.
--
-- excise_allowed=0: this PRRO has no excise license, block any excise goods.
-- excise_allowed=1: excise operations permitted (still validated by tax group rules).
-- Default 0 (safe): new PRROs cannot sell excise until explicitly enabled.

ALTER TABLE node_state ADD COLUMN excise_allowed INTEGER NOT NULL DEFAULT 0;
