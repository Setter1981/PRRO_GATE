-- ══════════════════════════════════════════════════════════════════════════════
-- 038 — UnknownStatus → ProbeRequired  (CS-3 Slice E Pin 3, Track B)
-- ══════════════════════════════════════════════════════════════════════════════
-- The classifier flip `routing_for_indeterminate(UnknownStatus) → ProbeRequired`
-- (prro-domain `delivery/mod.rs`) makes a parsed envelope carrying an unnamed non-zero
-- status code (`-4`/`-17`/`-99`) a HELD `ProbeRequired` (node_effect ProbeRequired), not a
-- blind `TransientRetry` re-drive. This migration flips the evidence-matrix arm #7 in lock-
-- step. The two MUST ship together (ATOMIC): the un-flipped classifier writing ProbeRequired
-- would be rejected by 036's matrix, and this matrix without the flip would reject the live
-- writer's TransientRetry — either half alone bricks the record boundary.
--
-- ── Mechanism: DROP + re-CREATE the matrix trigger (NOT an edit of 036) ─────────
-- Shipped migrations are checksummed by `sqlx::migrate!`; 036 stays byte-frozen. Only arm #7
-- changes; arms 1–6 and 8–11 are reproduced VERBATIM from 036 so the trigger is a single
-- authoritative object (SQLite has no ALTER TRIGGER).
--
-- ── Upgrade-safe on a NON-empty table (no INACTIVE fail-fast) ───────────────────
-- Unlike 033/035/036/037 (which fail-fast on a non-empty `delivery_reservation` because they
-- ship in the D+E cutover release against an empty table), 038 is a Slice E follow-up that may
-- reach a table already holding rows written under 037. A DROP + re-CREATE of a trigger does
-- not touch existing rows, so it is safe without a guard — AND it must be, because the whole
-- point of the `OLD.state`-discriminated arm below is backward-compat for pre-038 rows.
--
-- ── `OLD.state` discriminator (design §5) ───────────────────────────────────────
--   * A FRESH transition into OUTCOME_OBSERVED (OLD.state = 'CALL_STARTED') may write ONLY the
--     new `(ProbeRequired, ProbeRequired)`. The live `record_outcome` writer (post-flip) does
--     exactly this. No fresh row may carry the legacy combo.
--   * A re-validation UPDATE of a row ALREADY at OUTCOME_OBSERVED (OLD.state = 'OUTCOME_OBSERVED'
--     — e.g. the operator/apply path setting `apply_state`) ALSO accepts the legacy
--     `(TransientRetry, NoNodeEffect)`, so a pre-038 UnknownStatus row can still be driven to a
--     terminal state without tripping the matrix. This lenient branch is defensive backward-compat;
--     no fresh writer ever exercises it (INSERT pins RESERVED_NOT_STARTED → 032, so the only route
--     into OO is the CALL_STARTED transition, and the live writer emits ProbeRequired).
-- The `evidence_immutable` trigger (036) freezes only the four evidence columns after OO — NOT
-- `routing_class`/`node_effect` — so a re-validation UPDATE re-fires this matrix and is checked here.

DROP TRIGGER IF EXISTS delivery_reservation_evidence_matrix_update;

CREATE TRIGGER delivery_reservation_evidence_matrix_update
BEFORE UPDATE ON delivery_reservation
WHEN (NEW.state <> 'OUTCOME_OBSERVED'
        AND (NEW.evidence_kind IS NOT NULL OR NEW.evidence_text IS NOT NULL
          OR NEW.evidence_code IS NOT NULL OR NEW.evidence_digest IS NOT NULL))
  OR (NEW.state = 'OUTCOME_OBSERVED' AND NEW.evidence_kind IS NULL
        AND (NEW.evidence_text IS NOT NULL OR NEW.evidence_code IS NOT NULL
          OR NEW.evidence_digest IS NOT NULL))
  OR (NEW.state = 'OUTCOME_OBSERVED' AND NEW.evidence_kind IS NOT NULL
        AND COALESCE((CASE NEW.evidence_kind

    -- 1. PreconditionFailed — pre-wire refusal (NOT_SUBMITTED). No payload.
    WHEN 'PreconditionFailed' THEN (
         NEW.submission_certainty = 'NOT_SUBMITTED' AND NEW.response_provenance = 'NO_RESPONSE'
     AND NEW.routing_class = 'TransientRetry' AND NEW.node_effect = 'NoNodeEffect'
     AND NEW.evidence_text IS NULL AND NEW.evidence_code IS NULL AND NEW.evidence_digest IS NULL
     AND NEW.remote_correlation_id IS NULL)

    -- 2. SigningFailed — local wrapper bug (NOT_SUBMITTED). No payload.
    WHEN 'SigningFailed' THEN (
         NEW.submission_certainty = 'NOT_SUBMITTED' AND NEW.response_provenance = 'NO_RESPONSE'
     AND NEW.routing_class = 'WrapperBug' AND NEW.node_effect = 'WrapperBug'
     AND NEW.evidence_text IS NULL AND NEW.evidence_code IS NULL AND NEW.evidence_digest IS NULL
     AND NEW.remote_correlation_id IS NULL)

    -- 3. NoResponse — bytes may have left, no ack (SUBMITTED_UNKNOWN). text = cause.
    WHEN 'NoResponse' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'NO_RESPONSE'
     AND NEW.routing_class = 'TransientRetry' AND NEW.node_effect = 'NoNodeEffect'
     AND NEW.evidence_text IN ('LocalHandshakeFailure','Timeout','Cancelled',
                               'CrashedBeforeObservation','CallFailedWithoutTrustedDpsEnvelope')
     AND NEW.evidence_code IS NULL AND NEW.evidence_digest IS NULL
     AND NEW.remote_correlation_id IS NULL)

    -- 4. RemoteAuthStatus — authenticated peer, no parsed envelope. digest(32).
    WHEN 'RemoteAuthStatus' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'AUTHENTICATED_PEER'
     AND NEW.routing_class = 'ProbeRequired' AND NEW.node_effect = 'ProbeRequired'
     AND NEW.evidence_text IS NULL AND NEW.evidence_code IS NULL
     AND NEW.evidence_digest IS NOT NULL AND length(NEW.evidence_digest) = 32
     AND NEW.remote_correlation_id IS NULL)

    -- 5. Accepted — parsed accept. text = exact F (non-empty); correlation = text.
    WHEN 'Accepted' THEN (
         NEW.submission_certainty = 'SUBMITTED' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND NEW.routing_class IS NULL AND NEW.node_effect = 'NoNodeEffect'
     AND NEW.evidence_text IS NOT NULL AND length(NEW.evidence_text) >= 1
     AND NEW.evidence_code IS NULL AND NEW.evidence_digest IS NULL
     AND NEW.remote_correlation_id IS NOT NULL AND NEW.remote_correlation_id = NEW.evidence_text)

    -- 6. Rejected — parsed reject. text = DpsReject name; (routing,node) = verdict map
    --    (live routing_for_reject, mod.rs:985-1002); digest(32); correlation NULL.
    WHEN 'Rejected' THEN (
         NEW.submission_certainty = 'SUBMITTED' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND NEW.evidence_code IS NULL
     AND NEW.evidence_digest IS NOT NULL AND length(NEW.evidence_digest) = 32
     AND NEW.remote_correlation_id IS NULL
     AND (CASE NEW.evidence_text
            WHEN 'Verify'              THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'Type'                THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'Xml'                 THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'XmlDate'             THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'XmlChk'              THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'XmlZReport'          THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'OfflineId'           THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'Close'               THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NoNodeEffect')
            WHEN 'NotPrevZReport'      THEN (NEW.routing_class = 'OperatorEscalation' AND NEW.node_effect = 'OperatorEscalation')
            WHEN 'Offline168'          THEN (NEW.routing_class = 'TerminalReject'     AND NEW.node_effect = 'NodeBlocked')
            WHEN 'BadHashPrev'         THEN (NEW.routing_class = 'MacRecovery'        AND NEW.node_effect = 'MacReseedPending')
            WHEN 'NotRegisteredRro'    THEN (NEW.routing_class = 'FnConfigError'      AND NEW.node_effect = 'FnConfigError')
            WHEN 'NotRegisteredSigner' THEN (NEW.routing_class = 'FnConfigError'      AND NEW.node_effect = 'FnConfigError')
            ELSE 0 END))

    -- 7. UnknownStatus — parsed, code outside the named/0/1 set. code + digest(32).
    --    CS-3 Slice E Pin 3: a FRESH CS→OO transition (OLD.state='CALL_STARTED') must write the new
    --    (ProbeRequired, ProbeRequired); a re-validation UPDATE of an existing OO row
    --    (OLD.state='OUTCOME_OBSERVED') ALSO accepts the legacy (TransientRetry, NoNodeEffect) so a
    --    pre-038 row stays drivable-to-terminal (defensive backward-compat; no fresh writer uses it).
    WHEN 'UnknownStatus' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND ((NEW.routing_class = 'ProbeRequired' AND NEW.node_effect = 'ProbeRequired')
          OR (OLD.state = 'OUTCOME_OBSERVED'
              AND NEW.routing_class = 'TransientRetry' AND NEW.node_effect = 'NoNodeEffect'))
     AND NEW.evidence_text IS NULL
     AND NEW.evidence_code IS NOT NULL
     AND NEW.evidence_code NOT IN (0,1,-1,-2,-5,-6,-7,-8,-9,-10,-11,-12,-13,-14,-15,-16)
     AND NEW.evidence_digest IS NOT NULL AND length(NEW.evidence_digest) = 32
     AND NEW.remote_correlation_id IS NULL)

    -- 8. SaveError — parsed -3 ERROR_SAVE. digest(32).
    WHEN 'SaveError' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND NEW.routing_class = 'TransientRetry' AND NEW.node_effect = 'NoNodeEffect'
     AND NEW.evidence_text IS NULL AND NEW.evidence_code IS NULL
     AND NEW.evidence_digest IS NOT NULL AND length(NEW.evidence_digest) = 32
     AND NEW.remote_correlation_id IS NULL)

    -- 9. CloseAmbiguous — close/Z -2/-15 collapsed. digest(32).
    WHEN 'CloseAmbiguous' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND NEW.routing_class = 'ProbeRequired' AND NEW.node_effect = 'ProbeRequired'
     AND NEW.evidence_text IS NULL AND NEW.evidence_code IS NULL
     AND NEW.evidence_digest IS NOT NULL AND length(NEW.evidence_digest) = 32
     AND NEW.remote_correlation_id IS NULL)

    -- 10. MissingStatus — parsed envelope, no status. digest(32).
    WHEN 'MissingStatus' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND NEW.routing_class = 'ProbeRequired' AND NEW.node_effect = 'ProbeRequired'
     AND NEW.evidence_text IS NULL AND NEW.evidence_code IS NULL
     AND NEW.evidence_digest IS NOT NULL AND length(NEW.evidence_digest) = 32
     AND NEW.remote_correlation_id IS NULL)

    -- 11. OkButNoFiscalNumber — parsed ok=1, empty fiscal number. digest(32).
    WHEN 'OkButNoFiscalNumber' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND NEW.routing_class = 'ProbeRequired' AND NEW.node_effect = 'ProbeRequired'
     AND NEW.evidence_text IS NULL AND NEW.evidence_code IS NULL
     AND NEW.evidence_digest IS NOT NULL AND length(NEW.evidence_digest) = 32
     AND NEW.remote_correlation_id IS NULL)

    ELSE 0 END), 0) <> 1)
BEGIN SELECT RAISE(ABORT, 'delivery_reservation: evidence matrix violation (illegal/partial evidence at OUTCOME_OBSERVED, or non-NULL evidence before it)'); END;
