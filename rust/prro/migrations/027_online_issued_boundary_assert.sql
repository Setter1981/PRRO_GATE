-- A.3 PR-B step 9a — ONE-TIME fail-closed boundary assert (AUD-L3).
--
-- The A.3 seed-fork fix moves the online chain-seed advance from ACK
-- (`stage_finalize`) to SEND (`stage_send`, atomic with the `server_fiscal_no`
-- stamp). This migration is the semantic-boundary guard for a RESTORED /
-- FOREIGN database crossing that boundary: if the DB carries a row that was
-- written under the OLD (pre-A.3) semantics as an online-issued document whose
-- seed was NOT advanced, the NEW semantics would (via the D3 discriminator)
-- declare it ISSUED — a silent chain inconsistency. Refuse to cross the boundary
-- and hand it to a human.
--
-- HAZARD row (pre-A.3 online-issued-but-seed-unadvanced):
--     offline_fiscal_no IS NULL           -- online-origin (D3)
--     AND server_fiscal_no IS NOT NULL    -- ...that got a DPS id (post-send)
--     AND server_fiscal_no != ''
--     AND state NOT IN ('ACK')            -- but never reached ACK, where the
--                                         -- OLD code advanced the seed → the
--                                         -- seed is stale under the new rule.
-- (A pre-A.3 doc that reached ACK DID advance the seed → NOT a hazard.)
--
-- ONE-TIME, NO PERMANENT TRACE: post-A.3 a HEALTHY DB legitimately rests
-- online-issued docs at SENT/KVT1/KVT2 (advance-at-SEND stamps sfn AND advances
-- the seed atomically) — a PERMANENT check would false-positive on every such
-- row. So this assert runs EXACTLY ONCE, at the 026→027 upgrade boundary
-- (recorded in `_sqlx_migrations`, never re-run), and leaves NO schema object
-- behind. It is schema-neutral: ZERO DDL on persistent data.
--
-- MECHANISM (checksum-runner compatible; no RAISE-outside-trigger, no app code):
-- a TEMP-schema table (never in the DB file) with a CHECK constraint that a
-- non-zero hazard count violates → the INSERT aborts the migration transaction
-- (sqlx 0.8 wraps migrations transactionally → full rollback, nothing recorded,
-- fail-closed). On a clean DB the count is 0, the INSERT succeeds, and the temp
-- table is dropped → the migration commits with no residue.

DROP TABLE IF EXISTS _m027_online_issued_boundary_assert;

CREATE TEMP TABLE _m027_online_issued_boundary_assert (
    -- The migration ABORTS here if any pre-A.3 online-issued-unadvanced row
    -- exists (CHECK violated by a non-zero count). Operator must reconcile those
    -- rows (converge to ACK or quarantine) before re-attempting the upgrade.
    hazard_count INTEGER NOT NULL CHECK (hazard_count = 0)
);

INSERT INTO _m027_online_issued_boundary_assert (hazard_count)
SELECT COUNT(*)
FROM fiscal_documents
WHERE offline_fiscal_no IS NULL
  AND server_fiscal_no IS NOT NULL
  AND server_fiscal_no != ''
  AND state NOT IN ('ACK');

DROP TABLE _m027_online_issued_boundary_assert;
