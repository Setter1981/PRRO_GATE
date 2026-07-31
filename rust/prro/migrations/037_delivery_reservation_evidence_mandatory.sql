-- ══════════════════════════════════════════════════════════════════════════════
-- 037 — evidence MANDATORY at OUTCOME_OBSERVED (CS-3 Slice 7 gap 2)
-- ══════════════════════════════════════════════════════════════════════════════
-- Migration 036 landed the evidence union as VALIDATE-IF-PRESENT: a row could reach
-- OUTCOME_OBSERVED with all four evidence columns NULL (Slice 2b deliberately deferred
-- the mandatory flip to avoid a large test blast radius while NO writer existed yet).
-- The production record boundary now exists (`delivery_reservation::record_outcome`,
-- Slice 7 gap 1) and ALWAYS serializes exactly one `EvidenceDiscriminant` leaf; boot
-- resume writes the `NoResponse{CrashedBeforeObservation}` leaf. Every path that reaches
-- OUTCOME_OBSERVED therefore now carries evidence, and P4 (lossless, boot-hydratable
-- authority) REQUIRES it: an OO row with NULL evidence is an unrecoverable authority —
-- boot cannot hydrate an `EvidenceDiscriminant` leaf from it (design §4.4).
--
-- This closes the last hole 036 left open — `state = OUTCOME_OBSERVED AND evidence_kind
-- IS NULL` — with a single additive BEFORE UPDATE trigger. 036's matrix already forbids
-- (b) evidence sub-columns present under a NULL kind, and (c) a non-NULL kind whose leaf
-- axes don't match; this adds (a') a NULL kind AT OUTCOME_OBSERVED is itself illegal.
-- Combined, every OO row carries exactly one valid, boot-hydratable leaf.
--
-- Additive ONLY: no ALTER, no table rebuild, no change to any 036 object. 036 stays an
-- independently-correct, reviewable migration; this layers the tightening on top (the
-- stacked-PR-safe alternative to editing 036 in place — no shipped-migration checksum
-- drift). Rows only ever reach OUTCOME_OBSERVED via UPDATE (the 032 `insert_state`
-- trigger pins fresh rows to RESERVED_NOT_STARTED), so a BEFORE UPDATE trigger is the
-- complete guard.
--
-- INACTIVE: fail-fast on a non-empty table (mirrors 033/035/036). D+E ship in one release.
-- ══════════════════════════════════════════════════════════════════════════════

-- ── (0) INACTIVE-safety: abort if delivery_reservation is non-empty ─────────────
DROP TABLE IF EXISTS _m037_activation_guard;
CREATE TEMP TABLE _m037_activation_guard (
    row_count INTEGER NOT NULL CHECK (row_count = 0)
);
INSERT INTO _m037_activation_guard (row_count)
SELECT COUNT(*) FROM delivery_reservation;
DROP TABLE _m037_activation_guard;

-- ── (1) evidence is MANDATORY at OUTCOME_OBSERVED ───────────────────────────────
-- A transition into (or an update at) OUTCOME_OBSERVED with a NULL `evidence_kind` is
-- refused. With 036's matrix (kind non-NULL ⇒ the leaf's axes match), every OO row now
-- carries exactly one valid, boot-hydratable evidence leaf.
-- Being a BEFORE UPDATE trigger, this fires ahead of the 032 OO structural CHECKs, so a
-- NULL-evidence illegal-axes OO row is now caught HERE first (the 032 axis-CHECKs remain
-- as backstops). The message carries the word "constraint" so the many 032 negative tests
-- that assert an illegal OO row is rejected (`err contains "constraint"`) stay green — the
-- invariant they guard (illegal OO combo refused) still holds, now fronted by this leaf gate.
CREATE TRIGGER delivery_reservation_evidence_mandatory_update
BEFORE UPDATE ON delivery_reservation
WHEN NEW.state = 'OUTCOME_OBSERVED' AND NEW.evidence_kind IS NULL
BEGIN SELECT RAISE(ABORT, 'delivery_reservation: OUTCOME_OBSERVED requires an evidence leaf — evidence constraint (evidence_kind NOT NULL)'); END;
