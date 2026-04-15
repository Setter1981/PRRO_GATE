-- 006_mac_seed.sql: Add last_known_mac to node_state for MAC chain bootstrap.
--
-- When a fresh DB is bootstrapped against a DPS fiscal number that already
-- has document history, _resolve_dps_mac() needs a seed MAC to continue
-- the hash chain. This field is populated from DPS ERROR_BAD_HASH_PREV
-- response or from operator-provided seed.

ALTER TABLE node_state ADD COLUMN last_known_mac TEXT NOT NULL DEFAULT '';
