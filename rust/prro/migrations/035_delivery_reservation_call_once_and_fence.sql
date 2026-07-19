-- ══════════════════════════════════════════════════════════════════════════════
-- Migration 035 — delivery_reservation: per-document lifetime CALL-ONCE guard
--                 + narrowed §3.1 active-fence.  INACTIVE (schema only).
--
-- CS-3 D/E Slice 1 (design docs/CS3_REMEDIATION_DESIGN.md §2 + §3.1).
--
-- Scope (deliberately NARROW — see §8 slice order):
--   * P2 — a per-document lifetime call-once guard (index + no_replace clause):
--          at most one reservation per document_id may EVER cross call_started_at.
--   * P3 — the narrowed active-fence predicate `state IN (RN,CS) OR (OO AND
--          apply_state='PENDING_APPLY')`, byte-identical across the index and the
--          no_replace trigger, replacing the 032/033 cert/routing fence.
--
-- NOT here (moved to Slice 2, migration 036, alongside ObservedOutcomeV1::record):
--   * the durable evidence union columns + the fail-closed evidence matrix triggers
--     + the evidence immutability trigger (design §4).  Enforcing the evidence
--     matrix in this slice — before the record path that populates it exists —
--     would break every pre-existing test that transitions a reservation to
--     OUTCOME_OBSERVED without evidence, for zero operational benefit (the
--     reservation FSM is INACTIVE until the whole-fence cutover, Slice 7).
--
-- No `seed_advanced` column (proven a dead disjunct, design §-1 C1).
-- No table rebuild: this migration only ADDS an index and REPLACES the two
-- fence DB objects; the 032/033/034 table definition + triggers are untouched.
-- INACTIVE: fail-fast on a non-empty table (mirrors 033).  D+E ship in one release.
-- ══════════════════════════════════════════════════════════════════════════════

-- ── (0) INACTIVE-safety: abort if delivery_reservation is non-empty ─────────────
-- The CHECK (row_count = 0) fires on INSERT, rolling back the whole migration tx.
DROP TABLE IF EXISTS _m035_activation_guard;
CREATE TEMP TABLE _m035_activation_guard (
    row_count INTEGER NOT NULL CHECK (row_count = 0)
);
INSERT INTO _m035_activation_guard (row_count)
SELECT COUNT(*) FROM delivery_reservation;
DROP TABLE _m035_activation_guard;

-- ── (1) P2 — lifetime call-once index ──────────────────────────────────────────
-- At most one row per document_id may have call_started_at IS NOT NULL. A second
-- CALL_STARTED for the same document fails with SQLITE_CONSTRAINT_UNIQUE. Fresh
-- RESERVED_NOT_STARTED rows (call_started_at NULL) are excluded by the partial
-- predicate, so attempt_no = MAX+1 fresh reservations are unaffected.
CREATE UNIQUE INDEX ux_delivery_document_ever_started
    ON delivery_reservation(document_id)
    WHERE call_started_at IS NOT NULL;

-- ── (2) P3 — narrowed active-fence: rebuild the index ──────────────────────────
-- §3.1 predicate: only the record-then-apply window holds the FN fence. Unresolved
-- outcomes (SubmittedUnknown / -12 / -6) are held by STOP_MODE (Slice 5), NOT the
-- SQL fence; definitively-resolved rows release. Byte-identical to the no_replace
-- clause below and to get_active_for_fn / the authorization query (Slice D).
DROP INDEX ux_reservation_active;
CREATE UNIQUE INDEX ux_reservation_active ON delivery_reservation(fiscal_number)
    WHERE state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
       OR (state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY');

-- ── (3) P2 + P3 — rebuild delivery_reservation_no_replace ───────────────────────
-- Forbids INSERT-OR-REPLACE-style collisions AND enforces, at INSERT time, the
-- §3.1 active fence + the lifetime call-once (a fresh RN for a document that ever
-- started is refused before a row exists — necessary so an unstartable orphan
-- active row cannot block the FN).
DROP TRIGGER delivery_reservation_no_replace;
CREATE TRIGGER delivery_reservation_no_replace
BEFORE INSERT ON delivery_reservation
WHEN EXISTS (SELECT 1 FROM delivery_reservation WHERE reservation_id = NEW.reservation_id)
  OR EXISTS (SELECT 1 FROM delivery_reservation WHERE document_id = NEW.document_id AND attempt_no = NEW.attempt_no)
  OR EXISTS (SELECT 1 FROM delivery_reservation WHERE fiscal_number = NEW.fiscal_number
        AND (state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
          OR (state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY')))
  OR EXISTS (SELECT 1 FROM delivery_reservation
        WHERE document_id = NEW.document_id AND call_started_at IS NOT NULL)
BEGIN SELECT RAISE(ABORT, 'delivery_reservation: collision on reservation_id / (document_id,attempt_no) / active fence / document-ever-started — INSERT OR REPLACE forbidden'); END;
