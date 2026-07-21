-- ══════════════════════════════════════════════════════════════════════════════
-- Migration 036 — delivery_reservation: durable evidence union + fail-closed
--                 evidence matrix.  INACTIVE (schema only).
--
-- CS-3 D/E Slice 2a (design docs/CS3_REMEDIATION_DESIGN.md §4.2 + §5).
--
-- WHY (P4 — durable, total evidence)
-- ----------------------------------
-- The record-then-apply boundary (033 §-note, A4-6) commits the OUTCOME_OBSERVED
-- evidence first (PENDING_APPLY) and applies the ledger/seed projection in a second
-- commit.  A crash between the two must LOSE NOTHING: boot re-reads the reservation
-- and reconstructs the exact classified outcome — the accepted fiscal number, the
-- DpsReject verdict, the NoResponseCause, the decoded-response digest — from durable
-- columns, never from a re-issued wire call.  Before this migration ObservedOutcomeV1
-- carried only the axes triple + node_effect + generation; the *payload* that makes
-- the outcome lossless (F / verdict / cause / digest) had no home.  This migration
-- adds that home and a fail-closed matrix that makes an illegal or partial evidence
-- row UNREPRESENTABLE at OUTCOME_OBSERVED.
--
-- SCOPE (Slice 2a — persistence substrate only)
-- ---------------------------------------------
--   * four additive nullable evidence columns (evidence_kind/text/code/digest);
--   * a fail-closed evidence matrix (INSERT + UPDATE) binding evidence_kind to the
--     exact (certainty, provenance, routing_class, node_effect) axes + payload shape
--     for each of the 11 EvidenceDiscriminant leaves (design §4.1/§4.2), with the
--     Rejected leaf additionally pinned to the live routing_for_reject verdict map
--     (prro-domain/src/delivery/mod.rs:985-1002);
--   * an evidence-immutability trigger freezing all four columns after OO.
--
-- NOT here (Slice 2b): the Rust EvidenceDiscriminant enum, ObservedOutcomeV1::record
-- extension, and boot hydration that WRITE/READ these columns.  This migration is the
-- schema contract those Rust paths must serialize to; the canonical evidence_text /
-- evidence_kind vocabularies below are frozen HERE and Slice 2b conforms to them.
--
-- DELIBERATE DEVIATION — evidence is VALIDATE-IF-PRESENT here, not yet mandatory
-- ----------------------------------------------------------------------------
-- Design §4.2 requires "at OUTCOME_OBSERVED, exactly one matrix row must match"
-- (evidence MANDATORY at OO).  Enforcing mandatory in THIS slice — before the
-- Slice-2b record path that always populates evidence exists — would break every
-- pre-existing test that drives a reservation to OO without evidence (the same
-- ~46-test blast radius that made us defer the matrix out of Slice 1).  So the
-- matrix here is FAIL-CLOSED on SHAPE (if evidence is present it MUST match exactly
-- one leaf; partial evidence without a kind is refused) but OPTIONAL on PRESENCE (an
-- OO row may still carry all-NULL evidence).  Slice 2b lands the record path (sole
-- writer, always populates evidence) and FLIPS presence to mandatory (add migration
-- that refuses `state=OO AND evidence_kind IS NULL`).  P4 (lossless evidence) is
-- therefore delivered by 2b; since D+E ship in one release and nothing activates
-- until the whole-fence cutover, no unreleased OO row is ever evidence-less at
-- activation.  This staging mirrors the Slice-1 Option-B discipline: defer a
-- blast-radius enforcement to the slice whose writer justifies it.
--
-- ADD COLUMN vs REBUILD
-- ---------------------
-- delivery_reservation is STRICT.  All four columns are nullable with NO DEFAULT and
-- NO column-level CHECK, so `ALTER TABLE ... ADD COLUMN` is valid on a STRICT table
-- (033's REBUILD note only bars ADD COLUMN carrying non-trivial per-column CHECKs).
-- The matrix + immutability are enforced by TRIGGERS, not column CHECKs.  The 035
-- fence objects (ux_reservation_active, ux_delivery_document_ever_started,
-- delivery_reservation_no_replace) reference none of the evidence columns and are
-- untouched by the ADD — no table rebuild, no fence-object rebuild.
--
-- INACTIVE: fail-fast on a non-empty table (mirrors 033/035).  D+E ship in one
-- production release.
-- ══════════════════════════════════════════════════════════════════════════════

-- ── (0) INACTIVE-safety: abort if delivery_reservation is non-empty ─────────────
DROP TABLE IF EXISTS _m036_activation_guard;
CREATE TEMP TABLE _m036_activation_guard (
    row_count INTEGER NOT NULL CHECK (row_count = 0)
);
INSERT INTO _m036_activation_guard (row_count)
SELECT COUNT(*) FROM delivery_reservation;
DROP TABLE _m036_activation_guard;

-- ── (1) evidence union columns (additive, nullable, no per-column CHECK) ─────────
-- evidence_kind:   the EvidenceDiscriminant leaf tag (one of the 11 names below).
-- evidence_text:   lossless context string — exact accepted fiscal number (Accepted),
--                  DpsReject variant name (Rejected), or NoResponseCause name
--                  (NoResponse).  NOT stored in remote_correlation_id (different
--                  meaning; §4.2).
-- evidence_code:   the raw DPS status code, used ONLY by UnknownStatus.
-- evidence_digest: 32-byte decoded-response digest, used by every digest-bearing leaf.
ALTER TABLE delivery_reservation ADD COLUMN evidence_kind   TEXT;
ALTER TABLE delivery_reservation ADD COLUMN evidence_text   TEXT;
ALTER TABLE delivery_reservation ADD COLUMN evidence_code   INTEGER;
ALTER TABLE delivery_reservation ADD COLUMN evidence_digest BLOB;

-- ── (2) rule (a): evidence is NULL before OUTCOME_OBSERVED ───────────────────────
-- INSERT is always RESERVED_NOT_STARTED (insert_state trigger), so a freshly-minted
-- row must carry no evidence.  Fail-closed: any non-NULL evidence column at insert
-- aborts.
CREATE TRIGGER delivery_reservation_evidence_insert
BEFORE INSERT ON delivery_reservation
WHEN NEW.evidence_kind IS NOT NULL
  OR NEW.evidence_text IS NOT NULL
  OR NEW.evidence_code IS NOT NULL
  OR NEW.evidence_digest IS NOT NULL
BEGIN SELECT RAISE(ABORT, 'delivery_reservation: evidence columns must be NULL before OUTCOME_OBSERVED'); END;

-- ── (3) fail-closed evidence matrix (UPDATE) — VALIDATE-IF-PRESENT ──────────────
-- Aborts the UPDATE when the post-UPDATE row is evidence-inconsistent:
--   (a) state <> OO AND any evidence column non-NULL   → evidence before OO;
--   (b) state  = OO AND evidence_kind NULL AND any of text/code/digest non-NULL
--                                                       → partial evidence, no kind;
--   (c) state  = OO AND evidence_kind NOT NULL AND the leaf does not match
--                                                       → illegal/partial leaf.
-- Presence is OPTIONAL at OO (all-NULL evidence passes) — see the DEVIATION note in
-- the header; Slice 2b flips (b)/absence to mandatory.  The matrix is keyed on
-- evidence_kind (a distinct tag per leaf), so "exactly one" is automatic: the CASE
-- selects one arm; the arm returns 1 iff EVERY condition for that leaf holds, else 0.
-- The outer COALESCE(...,0) closes the SQLite NULL-bypass: a comparison against a
-- NULL axis makes the arm NULL, COALESCE folds it to 0, and 0 <> 1 aborts.
--
-- Null-safety: value equality uses `=` (a NULL operand ⇒ NULL ⇒ folded to abort);
-- required-NULL uses `IS NULL`; required-non-NULL uses `IS NOT NULL`.  Accepted's
-- routing_class is required NULL, so it uses `IS NULL` (not `= something`).
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
    WHEN 'UnknownStatus' THEN (
         NEW.submission_certainty = 'SUBMITTED_UNKNOWN' AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
     AND NEW.routing_class = 'TransientRetry' AND NEW.node_effect = 'NoNodeEffect'
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

-- ── (4) evidence immutability (UPDATE) ──────────────────────────────────────────
-- Once OUTCOME_OBSERVED, the four evidence columns are frozen (null-safe IS NOT).
-- The matrix above already forbids downgrading state away from OO (transition
-- trigger), so this pins the recorded authority against any post-OO mutation.
CREATE TRIGGER delivery_reservation_evidence_immutable
BEFORE UPDATE ON delivery_reservation
WHEN OLD.state = 'OUTCOME_OBSERVED'
  AND (OLD.evidence_kind   IS NOT NEW.evidence_kind
    OR OLD.evidence_text   IS NOT NEW.evidence_text
    OR OLD.evidence_code   IS NOT NEW.evidence_code
    OR OLD.evidence_digest IS NOT NEW.evidence_digest)
BEGIN SELECT RAISE(ABORT, 'delivery_reservation: evidence columns are immutable after OUTCOME_OBSERVED'); END;
