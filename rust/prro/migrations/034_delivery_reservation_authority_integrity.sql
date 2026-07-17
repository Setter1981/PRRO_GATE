-- 034 — delivery_reservation authority integrity (CS-3 Bridge-0 R4)
--       (INACTIVE — trigger-only; 033's delivery_reservation is INACTIVE + empty)
--
-- Closes the four one-sided-CHECK holes the external CHECKPOINT AUDIT found in 033
-- (green-but-unsound: 033's CHECKs only forbade the "too early" direction, leaving
-- partially-written authority + an ABA-generation hazard reachable):
--
--   H1  node_state.delivery_generation was NOT monotonic — 033 enforced only `>= 0`,
--       so it could be DECREASED, an ABA hazard for the CS-3 replay-CAS.
--   H2  authorized_generation was not paired with call_started_at — a CALL_STARTED row
--       with authorized_generation = NULL was allowed.
--   H3  at OUTCOME_OBSERVED, apply_state = NULL and/or node_effect = NULL were allowed
--       (partially-written authority; #4A A4-6 records evidence WITH apply_state +
--       node_effect at OO).
--   H4  a clean accept (OUTCOME_OBSERVED, routing_class NULL) could be given a non-NULL
--       node_effect other than NoNodeEffect (e.g. NodeBlocked) — semantic nonsense.
--
-- ADDITIVE + trigger-only (no table rebuild): 033 is unedited; boot applies 034.
-- INACTIVE — no production caller writes delivery_reservation in CS-3 yet.

-- ── H1: monotonic delivery_generation (cannot decrease) ──────────────────────────
CREATE TRIGGER node_state_delivery_generation_monotone
BEFORE UPDATE OF delivery_generation ON node_state
WHEN NEW.delivery_generation < OLD.delivery_generation
BEGIN SELECT RAISE(ABORT, 'node_state.delivery_generation is monotonic — cannot decrease (ABA hazard)'); END;

-- ── H2: authorized_generation ↔ call_started_at pairing (set together at RN→CS) ──
-- `(X IS NULL)` yields 0/1; `<>` fires when exactly one is NULL. Both-NULL (RNS) and
-- both-set (CS) pass; a CALL_STARTED with authorized_generation NULL aborts.
CREATE TRIGGER delivery_reservation_cs_pairing_insert
BEFORE INSERT ON delivery_reservation
WHEN (NEW.call_started_at IS NULL) <> (NEW.authorized_generation IS NULL)
BEGIN SELECT RAISE(ABORT, 'authorized_generation and call_started_at must be set together (RN→CS pairing)'); END;

CREATE TRIGGER delivery_reservation_cs_pairing_update
BEFORE UPDATE ON delivery_reservation
WHEN (NEW.call_started_at IS NULL) <> (NEW.authorized_generation IS NULL)
BEGIN SELECT RAISE(ABORT, 'authorized_generation and call_started_at must be set together (RN→CS pairing)'); END;

-- ── H3: OUTCOME_OBSERVED completeness — apply_state + node_effect both non-NULL ──
-- Records the two-commit record-then-apply boundary (#4A A4-6): the evidence commit
-- writes OO WITH apply_state='PENDING_APPLY' and node_effect; neither may rest NULL at OO.
CREATE TRIGGER delivery_reservation_oo_completeness
BEFORE UPDATE ON delivery_reservation
WHEN NEW.state = 'OUTCOME_OBSERVED' AND (NEW.apply_state IS NULL OR NEW.node_effect IS NULL)
BEGIN SELECT RAISE(ABORT, 'OUTCOME_OBSERVED requires apply_state and node_effect (no partial authority)'); END;

-- ── H4: clean accept ⇒ NoNodeEffect ─────────────────────────────────────────────
-- A clean accept (OUTCOME_OBSERVED with routing_class NULL) has no node-mode side
-- effect; node_effect must be NoNodeEffect. (Non-clean paths, routing_class NOT NULL,
-- are unaffected and may carry NodeBlocked / MacReseedPending / etc.)
CREATE TRIGGER delivery_reservation_clean_accept_node_effect
BEFORE UPDATE ON delivery_reservation
WHEN NEW.state = 'OUTCOME_OBSERVED' AND NEW.routing_class IS NULL
     AND NEW.node_effect IS NOT NULL AND NEW.node_effect <> 'NoNodeEffect'
BEGIN SELECT RAISE(ABORT, 'a clean accept (routing_class NULL) must have node_effect = NoNodeEffect'); END;
