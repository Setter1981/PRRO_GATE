-- 035 — delivery_reservation: lifetime call-once + durable evidence union
--       (INACTIVE — CS-3 Slice D/E §4.2 + §2 + §5)
--
-- WHY
-- ---
-- CS-3 activation requires two structural additions to the existing table
-- (already extended by 032/033/034):
--
--   1. **Durable evidence union** (§4.2) — four nullable union columns that
--      store the payload of the sealed `EvidenceDiscriminant` so a crash
--      between record and apply leaves a fully hydrateable row (P4 fix).
--      A fail-closed INSERT/UPDATE matrix trigger enforces exactly one
--      matching leaf per OUTCOME_OBSERVED row.  A separate immutability
--      trigger freezes the four columns after OUTCOME_OBSERVED.
--
--   2. **Lifetime call-once index** (§2) — a partial unique index on
--      `document_id WHERE call_started_at IS NOT NULL`, so any attempt to
--      INSERT a new reservation for a document that has already crossed
--      CALL_STARTED is rejected by SQLite before the application layer can
--      reason about it.  Combined with the updated `delivery_reservation_no_replace`
--      trigger (which adds the same historical-document-started clause), no
--      orphan RN can exist for a document that already started a wire call.
--
--   3. **Rebuilt fence + no-replace** (§3.1) — the §3.1 active-fence
--      predicate (`state IN ('RESERVED_NOT_STARTED','CALL_STARTED') OR
--      (state='OUTCOME_OBSERVED' AND apply_state='PENDING_APPLY')`) replaces
--      the 032/033 SubmittedUnknown/routing-class based predicate in BOTH
--      `ux_reservation_active` and `delivery_reservation_no_replace`.  The
--      byte-identical predicate appears in both objects so a consistency test
--      can compare them structurally.
--
-- NO `seed_advanced` COLUMN (§0 C1)
-- -----------------------------------
-- `seed_advanced` was proved a dead disjunct:
--   • `route_send_result` maps Ok → WireDecision::Sent and Err → Routed exclusively;
--   • SFN stamping + online seed advance occur only in the Sent arm;
--   • `classify` makes Accepted (routing=None) disjoint from Rejected (routing=Some).
-- A reservation cannot simultaneously be a routed-reject and a clean-Sent seed advance.
-- This migration deliberately omits the column.
--
-- REBUILD DECISION
-- ----------------
-- `delivery_reservation` is STRICT.  The four new evidence columns carry
-- CHECK constraints that STRICT-mode `ALTER TABLE ... ADD COLUMN` cannot
-- express (a non-trivial per-column CHECK on a STRICT table).  Since 032/033
-- made the table ALWAYS EMPTY at migration time (the fail-fast guard below
-- enforces this), the cleanest approach — mirroring 033 — is a full table
-- rebuild:
--   (a) fail-fast guard aborts if non-empty;
--   (b) drop all 034/033 triggers, 033/032 indexes, and the table itself;
--   (c) re-CREATE with the full 24-column DDL (all 033 columns verbatim +
--       the 4 new 035 evidence columns) + all 033 CHECKs + new 035 CHECKs;
--   (d) re-CREATE all 033/034 indexes and triggers + new 035 objects.
--
-- FAIL-FAST ACTIVATION GUARD
-- ---------------------------
-- Mirrors migration 033: a TEMP table with CHECK (row_count = 0) aborts the
-- migration transaction if `delivery_reservation` is non-empty.  sqlx wraps
-- migrations transactionally (db/mod.rs); the abort rolls back everything
-- and the migration is never recorded in `_sqlx_migrations`.
--
-- INACTIVE
-- --------
-- This migration creates/extends schema only.  NO production caller writes
-- or reads the new columns, the new indexes, or the new triggers in this
-- slice.  Boot applies this migration (schema + `_sqlx_migrations` row);
-- the only cost is the new DDL.  Activation (wiring record-then-apply,
-- `authorize_submission`, sole-caller gate, fence cutover) is Slice D/E.
--
-- BACKWARD COMPATIBILITY
-- ----------------------
-- `delivery_reservation` is rebuilt in-place from an empty state.  The
-- existing `node_state` columns added by 033 are untouched.  `ux_fd_docid_fn`
-- (on `fiscal_documents`, added by 032) survives the table drop and is NOT
-- recreated here.  All objects from migrations 001–034 not scoped to
-- `delivery_reservation` are byte-identical after apply.
--
-- ROLLBACK REASONING
-- ------------------
-- Forward: DDL only; the guard ensures no fiscal row is lost.
-- Reverse: restore the 034 `delivery_reservation` table (no rows either
-- way); the four evidence columns and the new indexes/triggers disappear.
-- Pre-pilot posture: rollback = DB reset (as with 032/033/034).
--
-- LIVE FILE SEQUENCE
-- ------------------
-- This file is 035; sqlx applies by filename prefix order.  032/033/034
-- are untouched (their checksums are preserved).

-- rust/prro/migrations/035_delivery_reservation_call_once_and_evidence.sql
--   (INACTIVE — CS-3 Slice D/E)

-- ══════════════════════════════════════════════════════════════════════════════
-- §1  FAIL-FAST ACTIVATION GUARD  (mirrors migration 033)
-- ══════════════════════════════════════════════════════════════════════════════
-- Abort if delivery_reservation is non-empty.  The CHECK (row_count = 0)
-- fires on INSERT, rolling back the entire migration transaction.  On a
-- clean DB the count is 0, the INSERT succeeds, and the temp table is dropped.
DROP TABLE IF EXISTS _m035_activation_guard;
CREATE TEMP TABLE _m035_activation_guard (
    row_count INTEGER NOT NULL CHECK (row_count = 0)
);
INSERT INTO _m035_activation_guard (row_count)
SELECT COUNT(*) FROM delivery_reservation;
DROP TABLE _m035_activation_guard;

-- ══════════════════════════════════════════════════════════════════════════════
-- §2  TEAR DOWN 034 TRIGGERS + 033 TRIGGERS → 033/032 INDEXES → TABLE
-- ══════════════════════════════════════════════════════════════════════════════
-- Drop order: triggers first (depend on the table), then indexes, then table.
-- ux_fd_docid_fn is on fiscal_documents — NOT touched here.
-- node_state triggers added by 033/034 stay (they reference node_state, not
-- delivery_reservation).

-- 034 triggers
DROP TRIGGER delivery_reservation_clean_accept_node_effect;
DROP TRIGGER delivery_reservation_oo_completeness;
DROP TRIGGER delivery_reservation_cs_pairing_update;
DROP TRIGGER delivery_reservation_cs_pairing_insert;

-- 033 triggers (in reverse-creation order)
DROP TRIGGER delivery_reservation_apply_state_transition;
DROP TRIGGER delivery_reservation_updated_at;
DROP TRIGGER delivery_reservation_append_only;
DROP TRIGGER delivery_reservation_immutable;
DROP TRIGGER delivery_reservation_transition;
DROP TRIGGER delivery_reservation_no_replace;
DROP TRIGGER delivery_reservation_insert_state;

-- 033/032 indexes
DROP INDEX ix_reservation_call_started;
DROP INDEX ux_reservation_active;

-- table
DROP TABLE delivery_reservation;

-- ══════════════════════════════════════════════════════════════════════════════
-- §3  RE-CREATE delivery_reservation WITH 035 SCHEMA
--     (verbatim 033 columns + CHECKs + 4 new 035 evidence columns)
-- ══════════════════════════════════════════════════════════════════════════════
-- All 033 columns + CHECKs are copied verbatim (byte-identical to what SQLite
-- stored in sqlite_master for 034/033).  The 4 new evidence columns follow at
-- the end with their per-leaf CHECKs.

CREATE TABLE delivery_reservation (
    reservation_id        BLOB    PRIMARY KEY CHECK (length(reservation_id) = 16),
    document_id           BLOB    NOT NULL CHECK (length(document_id) = 16),
    fiscal_number         TEXT    NOT NULL,
    attempt_no            INTEGER NOT NULL CHECK (attempt_no >= 1),
    state                 TEXT    NOT NULL DEFAULT 'RESERVED_NOT_STARTED'
        CHECK (state IN ('RESERVED_NOT_STARTED','CALL_STARTED','OUTCOME_OBSERVED')),
    call_started_at       TEXT,   -- durable wire marker (== stage_send wire_call_started_at); set at RN→CS
    dps_protocol_id            TEXT    NOT NULL CHECK (dps_protocol_id IN ('FSCO_ZZD','EVPZ_DPS')),
    protocol_contract_version  INTEGER NOT NULL CHECK (protocol_contract_version >= 1),
    capability_profile_version INTEGER CHECK (capability_profile_version IS NULL OR capability_profile_version >= 1),
    endpoint_config_revision   INTEGER CHECK (endpoint_config_revision IS NULL OR endpoint_config_revision >= 1),
    envelope_hash         BLOB    NOT NULL CHECK (length(envelope_hash) = 32),  -- protocol-specific (prost Check, stage_send.rs:795)
    remote_correlation_id TEXT,
    -- The three Spec #2 §2 orthogonal fields. NOTE (blessed, both reviewers): SUBMITTED_UNKNOWN +
    -- NO_RESPONSE is the CANONICAL wire-timeout (bytes may have left, no ack came back — Spec #2
    -- §3/§9-RP1); it is VALID, not a hole, and must NOT be forbidden. The full routing ↔
    -- (certainty, provenance) classifier is the CS-3 typed constructor; this matrix is the minimal
    -- structural floor.
    submission_certainty  TEXT CHECK (submission_certainty IN ('NOT_SUBMITTED','SUBMITTED_UNKNOWN','SUBMITTED')),
    response_provenance   TEXT CHECK (response_provenance IN ('NO_RESPONSE','AUTHENTICATED_PEER','PARSED_DPS_ENVELOPE')),
    -- PascalCase, byte-identical with the retry_class wire contract (error_routing.rs:120).
    routing_class         TEXT CHECK (routing_class IN ('TerminalReject','TransientRetry','FnConfigError',
                            'WrapperBug','ProbeRequired','MacRecovery','OperatorEscalation','DrainChainSettleRetry')),
    created_at            TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),

    -- ── 033 new columns (verbatim) ───────────────────────────────────────────
    authorized_generation INTEGER CHECK (authorized_generation IS NULL OR authorized_generation >= 1),
    apply_state           TEXT    CHECK (apply_state IS NULL OR apply_state IN ('PENDING_APPLY','APPLIED')),
    node_effect           TEXT    CHECK (node_effect IS NULL OR node_effect IN (
                            'NodeBlocked',
                            'MacReseedPending',
                            'ProbeRequired',
                            'OperatorEscalation',
                            'FnConfigError',
                            'WrapperBug',
                            'NoNodeEffect'
                          )),

    -- ── 035 new columns — durable evidence union (§4.2) ─────────────────────
    -- Stores the EvidenceDiscriminant leaf payload so a crash between record
    -- and apply leaves a fully hydrateable row (P4 fix).  All four are NULL
    -- before OUTCOME_OBSERVED; at OUTCOME_OBSERVED, exactly one matrix row
    -- must match (enforced by the fail-closed triggers in §6).
    --
    -- evidence_kind TEXT:  the discriminant tag, one of the eleven leaf names.
    -- evidence_text TEXT:  context-specific: exact fiscal number (Accepted),
    --                      DpsReject name (Rejected), or NoResponseCause (NoResponse).
    --                      NOT stored in remote_correlation_id (different meaning).
    -- evidence_code INTEGER: used ONLY by UnknownStatus (the raw DPS error code).
    -- evidence_digest BLOB:  used by every digest-bearing leaf; must be 32 bytes.
    evidence_kind   TEXT CHECK (evidence_kind IS NULL OR evidence_kind IN (
                        'PreconditionFailed',
                        'SigningFailed',
                        'NoResponse',
                        'RemoteAuthStatus',
                        'Accepted',
                        'Rejected',
                        'UnknownStatus',
                        'SaveError',
                        'CloseAmbiguous',
                        'MissingStatus',
                        'OkButNoFiscalNumber'
                    )),
    evidence_text   TEXT,
    evidence_code   INTEGER,
    evidence_digest BLOB CHECK (evidence_digest IS NULL OR length(evidence_digest) = 32),

    -- ── structural-consistency matrix (033, verbatim) ────────────────────────
    CHECK (state <> 'RESERVED_NOT_STARTED' OR (call_started_at IS NULL
        AND submission_certainty IS NULL AND response_provenance IS NULL AND routing_class IS NULL)),
    CHECK (state <> 'CALL_STARTED' OR (call_started_at IS NOT NULL
        AND submission_certainty IS NULL AND response_provenance IS NULL AND routing_class IS NULL)),
    CHECK (state <> 'OUTCOME_OBSERVED' OR (submission_certainty IS NOT NULL AND response_provenance IS NOT NULL)),
    CHECK (submission_certainty <> 'NOT_SUBMITTED' OR (call_started_at IS NULL AND response_provenance = 'NO_RESPONSE')),
    CHECK (submission_certainty NOT IN ('SUBMITTED_UNKNOWN','SUBMITTED') OR call_started_at IS NOT NULL),
    CHECK (submission_certainty <> 'SUBMITTED' OR response_provenance = 'PARSED_DPS_ENVELOPE'),
    CHECK (routing_class IS NOT NULL OR state <> 'OUTCOME_OBSERVED' OR submission_certainty = 'SUBMITTED'),
    -- response-derived classes cannot exist without a parsed DPS response (audit round-3 §4):
    CHECK (routing_class NOT IN ('TerminalReject','FnConfigError','MacRecovery','OperatorEscalation')
        OR (submission_certainty = 'SUBMITTED' AND response_provenance = 'PARSED_DPS_ENVELOPE')),
    CHECK (routing_class <> 'ProbeRequired' OR response_provenance <> 'NO_RESPONSE'),
    -- DrainChainSettleRetry is the legacy -8 tag (error_routing.rs:104): a parsed DPS artifact,
    -- never pre-call/no-response, and must never release the fence as a cancel (audit round-4 §4).
    CHECK (routing_class <> 'DrainChainSettleRetry'
        OR (submission_certainty <> 'NOT_SUBMITTED' AND response_provenance = 'PARSED_DPS_ENVELOPE')),
    -- remote_correlation_id only exists once an outcome is observed:
    CHECK (remote_correlation_id IS NULL OR state = 'OUTCOME_OBSERVED'),

    -- ── 033 field↔state lifecycle CHECKs (verbatim) ─────────────────────────
    CHECK (state <> 'RESERVED_NOT_STARTED' OR authorized_generation IS NULL),
    CHECK (apply_state IS NULL OR state = 'OUTCOME_OBSERVED'),
    CHECK (node_effect IS NULL OR state = 'OUTCOME_OBSERVED'),

    -- ── 035 field↔state lifecycle CHECKs ─────────────────────────────────────
    -- Evidence columns are NULL before OUTCOME_OBSERVED; the kind column controls
    -- the rest (matrix trigger is the normative enforcer, these are structural floor).
    CHECK (evidence_kind IS NULL OR state = 'OUTCOME_OBSERVED'),
    CHECK (evidence_text IS NULL OR state = 'OUTCOME_OBSERVED'),
    CHECK (evidence_code IS NULL OR state = 'OUTCOME_OBSERVED'),
    CHECK (evidence_digest IS NULL OR state = 'OUTCOME_OBSERVED'),

    FOREIGN KEY (document_id, fiscal_number)
        REFERENCES fiscal_documents(document_id, fiscal_number) ON DELETE RESTRICT,
    UNIQUE (document_id, attempt_no)
) STRICT;

-- ══════════════════════════════════════════════════════════════════════════════
-- §4  RE-CREATE INDEXES
-- ══════════════════════════════════════════════════════════════════════════════

-- New: lifetime call-once index (§2 / P2).
-- Ensures at most one row per document_id has call_started_at IS NOT NULL.
-- A collision on this index causes any INSERT/UPDATE that would set a second
-- call_started_at for the same document_id to fail with SQLITE_CONSTRAINT_UNIQUE.
CREATE UNIQUE INDEX ux_delivery_document_ever_started
    ON delivery_reservation(document_id)
    WHERE call_started_at IS NOT NULL;

-- Rebuilt: active-fence index with §3.1 predicate (replaces 032/033 version).
-- The predicate is BYTE-IDENTICAL to the clause in delivery_reservation_no_replace
-- (§5) and the get_active_for_fn / authorization query (Slice D).  A consistency
-- test (m3_* in the test file) compares them structurally.
CREATE UNIQUE INDEX ux_reservation_active ON delivery_reservation(fiscal_number)
    WHERE state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
       OR (state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY');

-- Preserved: call-started lookup index (033, verbatim).
CREATE INDEX ix_reservation_call_started ON delivery_reservation(fiscal_number)
    WHERE state = 'CALL_STARTED';

-- ══════════════════════════════════════════════════════════════════════════════
-- §5  RE-CREATE 033 TRIGGERS (verbatim byte-identical copies)
--     plus the 034 triggers (verbatim)
--     plus updated delivery_reservation_no_replace (§3.1 predicate + §2 clause)
-- ══════════════════════════════════════════════════════════════════════════════

-- Insert only as RESERVED_NOT_STARTED (033, verbatim).
CREATE TRIGGER delivery_reservation_insert_state
BEFORE INSERT ON delivery_reservation WHEN NEW.state <> 'RESERVED_NOT_STARTED'
BEGIN SELECT RAISE(ABORT, 'reservation must be inserted as RESERVED_NOT_STARTED'); END;

-- No-REPLACE collision-guard — REBUILT for 035 (§3.1 active-fence predicate + §2 historical clause).
-- The FN-fence predicate is BYTE-IDENTICAL to ux_reservation_active.
-- The historical-document-started clause additionally prevents a new RN from being inserted
-- for any document that has already crossed CALL_STARTED (call_started_at IS NOT NULL),
-- even after the old reservation row is APPLIED/fence-released.
-- This is necessary for both P2 (call-once) and BRICK (no unstartable orphan active row):
-- allowing the RN and refusing only at authorization would leave an unstartable active row
-- that blocks the FN.
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

-- Transition legality (033, verbatim).
CREATE TRIGGER delivery_reservation_transition
BEFORE UPDATE OF state, submission_certainty, response_provenance, routing_class, call_started_at
ON delivery_reservation
WHEN NOT (
       (OLD.state = 'RESERVED_NOT_STARTED' AND NEW.state = 'CALL_STARTED')
    OR (OLD.state = 'RESERVED_NOT_STARTED' AND NEW.state = 'OUTCOME_OBSERVED' AND NEW.submission_certainty = 'NOT_SUBMITTED')
    OR (OLD.state = 'CALL_STARTED' AND NEW.state = 'OUTCOME_OBSERVED' AND NEW.submission_certainty IN ('SUBMITTED_UNKNOWN','SUBMITTED'))
    OR (OLD.state = NEW.state)   -- technical same-state update (updated_at); field mutation blocked below
)
BEGIN SELECT RAISE(ABORT, 'illegal delivery_reservation state transition'); END;

-- Immutability (033, verbatim — 033 already extended for authorized_generation and node_effect).
-- 035 adds evidence columns to the frozen-at-OO set (handled by the separate evidence immutability
-- trigger in §7 below, so this trigger stays byte-identical to 033).
CREATE TRIGGER delivery_reservation_immutable
BEFORE UPDATE ON delivery_reservation
WHEN OLD.reservation_id IS NOT NEW.reservation_id
  OR OLD.document_id IS NOT NEW.document_id
  OR OLD.fiscal_number IS NOT NEW.fiscal_number
  OR OLD.attempt_no IS NOT NEW.attempt_no
  OR OLD.dps_protocol_id IS NOT NEW.dps_protocol_id
  OR OLD.protocol_contract_version IS NOT NEW.protocol_contract_version
  OR OLD.capability_profile_version IS NOT NEW.capability_profile_version
  OR OLD.endpoint_config_revision IS NOT NEW.endpoint_config_revision
  OR OLD.envelope_hash IS NOT NEW.envelope_hash
  OR OLD.created_at IS NOT NEW.created_at
  OR (OLD.call_started_at IS NOT NULL AND OLD.call_started_at IS NOT NEW.call_started_at)  -- set once at RN→CS
  OR (OLD.state = 'OUTCOME_OBSERVED' AND (
        OLD.submission_certainty IS NOT NEW.submission_certainty
     OR OLD.response_provenance  IS NOT NEW.response_provenance
     OR OLD.routing_class        IS NOT NEW.routing_class
     OR OLD.remote_correlation_id IS NOT NEW.remote_correlation_id))
  OR (OLD.authorized_generation IS NOT NULL AND OLD.authorized_generation IS NOT NEW.authorized_generation)
  OR (OLD.node_effect IS NOT NULL AND OLD.node_effect IS NOT NEW.node_effect)
BEGIN SELECT RAISE(ABORT, 'immutable field mutation on delivery_reservation'); END;

-- Append-only (033, verbatim).
CREATE TRIGGER delivery_reservation_append_only
BEFORE DELETE ON delivery_reservation
BEGIN SELECT RAISE(ABORT, 'delivery_reservation is append-only'); END;

-- updated_at maintenance (033, verbatim).
CREATE TRIGGER delivery_reservation_updated_at
AFTER UPDATE ON delivery_reservation
BEGIN UPDATE delivery_reservation SET updated_at = CURRENT_TIMESTAMP WHERE reservation_id = NEW.reservation_id; END;

-- apply_state monotone transition guard (033, verbatim).
CREATE TRIGGER delivery_reservation_apply_state_transition
BEFORE UPDATE OF apply_state ON delivery_reservation
WHEN NOT (
       OLD.apply_state IS NEW.apply_state
    OR (OLD.apply_state IS NULL AND NEW.apply_state IS 'PENDING_APPLY')
    OR (OLD.apply_state IS 'PENDING_APPLY' AND NEW.apply_state IS 'APPLIED')
)
BEGIN SELECT RAISE(ABORT, 'illegal apply_state transition on delivery_reservation'); END;

-- 034 triggers (verbatim)

CREATE TRIGGER delivery_reservation_cs_pairing_insert
BEFORE INSERT ON delivery_reservation
WHEN (NEW.call_started_at IS NULL) <> (NEW.authorized_generation IS NULL)
BEGIN SELECT RAISE(ABORT, 'authorized_generation and call_started_at must be set together (RN→CS pairing)'); END;

CREATE TRIGGER delivery_reservation_cs_pairing_update
BEFORE UPDATE ON delivery_reservation
WHEN (NEW.call_started_at IS NULL) <> (NEW.authorized_generation IS NULL)
BEGIN SELECT RAISE(ABORT, 'authorized_generation and call_started_at must be set together (RN→CS pairing)'); END;

CREATE TRIGGER delivery_reservation_oo_completeness
BEFORE UPDATE ON delivery_reservation
WHEN NEW.state = 'OUTCOME_OBSERVED' AND (NEW.apply_state IS NULL OR NEW.node_effect IS NULL)
BEGIN SELECT RAISE(ABORT, 'OUTCOME_OBSERVED requires apply_state and node_effect (no partial authority)'); END;

CREATE TRIGGER delivery_reservation_clean_accept_node_effect
BEFORE UPDATE ON delivery_reservation
WHEN NEW.state = 'OUTCOME_OBSERVED' AND NEW.routing_class IS NULL
     AND NEW.node_effect IS NOT NULL AND NEW.node_effect <> 'NoNodeEffect'
BEGIN SELECT RAISE(ABORT, 'a clean accept (routing_class NULL) must have node_effect = NoNodeEffect'); END;

-- ══════════════════════════════════════════════════════════════════════════════
-- §6  FAIL-CLOSED EVIDENCE MATRIX TRIGGERS (INSERT + UPDATE)
--
-- At OUTCOME_OBSERVED, exactly one leaf from the §4.2 matrix must match.
-- The triggers fire on INSERT and UPDATE when state = 'OUTCOME_OBSERVED'.
--
-- DESIGN NOTE — why COALESCE(CASE … ELSE 0 END, 0) <> 1
-- -------------------------------------------------------
-- `WHEN NOT (predicate)` is NULL-bypass-unsafe: if the predicate evaluates to
-- NULL (because a sub-expression is NULL), `NOT NULL` is NULL, which is NOT
-- TRUE, so the trigger does NOT fire — a NULL-filled row slips through.
-- Using `WHEN COALESCE((CASE WHEN <match> THEN 1 ELSE 0 END), 0) <> 1`
-- ensures: (a) if CASE evaluates to 1 → COALESCE(1,0) = 1 ≠ 1 is FALSE →
-- trigger does NOT fire (row is legal); (b) if CASE evaluates to 0 or NULL →
-- COALESCE(0/NULL, 0) = 0 ≠ 1 is TRUE → trigger fires (illegal row rejected).
--
-- MATRIX (§4.2, verbatim):
--
-- Leaf              | certainty       | provenance         | routing        | node_effect      | payload
-- PreconditionFailed| NOT_SUBMITTED   | NO_RESPONSE        | TransientRetry | NoNodeEffect     | all NULL
-- SigningFailed     | NOT_SUBMITTED   | NO_RESPONSE        | WrapperBug     | WrapperBug       | all NULL
-- NoResponse        | SUBMITTED_UNKNOWN| NO_RESPONSE       | TransientRetry | NoNodeEffect     | text=NoResponseCause
-- RemoteAuthStatus  | SUBMITTED_UNKNOWN| AUTHENTICATED_PEER| ProbeRequired  | ProbeRequired    | digest 32B
-- Accepted          | SUBMITTED       | PARSED_DPS_ENVELOPE| NULL (none)    | NoNodeEffect     | text=nonempty F; rcid=text
-- Rejected          | SUBMITTED       | PARSED_DPS_ENVELOPE| verdict-derived| verdict-derived  | text=DpsReject, digest 32B
-- UnknownStatus     | SUBMITTED_UNKNOWN| PARSED_DPS_ENVELOPE| TransientRetry| NoNodeEffect     | code=raw, digest 32B
-- SaveError         | SUBMITTED_UNKNOWN| PARSED_DPS_ENVELOPE| TransientRetry| NoNodeEffect     | digest 32B
-- CloseAmbiguous    | SUBMITTED_UNKNOWN| PARSED_DPS_ENVELOPE| ProbeRequired  | ProbeRequired    | digest 32B
-- MissingStatus     | SUBMITTED_UNKNOWN| PARSED_DPS_ENVELOPE| ProbeRequired  | ProbeRequired    | digest 32B
-- OkButNoFiscalNumber|SUBMITTED_UNKNOWN| PARSED_DPS_ENVELOPE| ProbeRequired  | ProbeRequired    | digest 32B
--
-- Rejected sub-mapping (routing_class / node_effect) per verdict:
--   Verify,Type,Xml,XmlDate,XmlChk,XmlZReport,OfflineId,Close → TerminalReject / NoNodeEffect
--   NotPrevZReport                                              → OperatorEscalation / OperatorEscalation
--   Offline168                                                  → TerminalReject / NodeBlocked
--   BadHashPrev                                                 → MacRecovery / MacReseedPending
--   NotRegisteredRro, NotRegisteredSigner                       → FnConfigError / FnConfigError
-- ══════════════════════════════════════════════════════════════════════════════

-- Evidence matrix trigger — fires on INSERT when state = OUTCOME_OBSERVED.
-- (INSERTs directly as OO are only legal for NOT_SUBMITTED pre-call paths;
--  the transition trigger already constrains which INSERT paths reach OO.
--  We include the INSERT trigger for defence-in-depth.)
CREATE TRIGGER delivery_reservation_evidence_matrix_insert
BEFORE INSERT ON delivery_reservation
WHEN NEW.state = 'OUTCOME_OBSERVED'
AND COALESCE((
    CASE
        -- PreconditionFailed: NOT_SUBMITTED / NO_RESPONSE / TransientRetry / NoNodeEffect / all NULL
        WHEN NEW.evidence_kind = 'PreconditionFailed'
             AND NEW.submission_certainty = 'NOT_SUBMITTED'
             AND NEW.response_provenance = 'NO_RESPONSE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- SigningFailed: NOT_SUBMITTED / NO_RESPONSE / WrapperBug / WrapperBug / all NULL
        WHEN NEW.evidence_kind = 'SigningFailed'
             AND NEW.submission_certainty = 'NOT_SUBMITTED'
             AND NEW.response_provenance = 'NO_RESPONSE'
             AND NEW.routing_class = 'WrapperBug'
             AND NEW.node_effect = 'WrapperBug'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- NoResponse: SUBMITTED_UNKNOWN / NO_RESPONSE / TransientRetry / NoNodeEffect / text=cause
        WHEN NEW.evidence_kind = 'NoResponse'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'NO_RESPONSE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NOT NULL
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- RemoteAuthStatus: SUBMITTED_UNKNOWN / AUTHENTICATED_PEER / ProbeRequired / ProbeRequired / digest 32B
        WHEN NEW.evidence_kind = 'RemoteAuthStatus'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'AUTHENTICATED_PEER'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Accepted: SUBMITTED / PARSED_DPS_ENVELOPE / NULL routing / NoNodeEffect / text=F (nonempty); rcid=text
        WHEN NEW.evidence_kind = 'Accepted'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class IS NULL
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NOT NULL AND length(NEW.evidence_text) > 0
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id = NEW.evidence_text
        THEN 1
        -- Rejected sub-mapping (§4.2): verdict determines routing + node_effect.
        -- Verify/Type/Xml/XmlDate/XmlChk/XmlZReport/OfflineId/Close → TerminalReject / NoNodeEffect
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TerminalReject'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IN ('Verify','Type','Xml','XmlDate','XmlChk','XmlZReport','OfflineId','Close')
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: NotPrevZReport → OperatorEscalation / OperatorEscalation
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'OperatorEscalation'
             AND NEW.node_effect = 'OperatorEscalation'
             AND NEW.evidence_text = 'NotPrevZReport'
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: Offline168 → TerminalReject / NodeBlocked
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TerminalReject'
             AND NEW.node_effect = 'NodeBlocked'
             AND NEW.evidence_text = 'Offline168'
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: BadHashPrev → MacRecovery / MacReseedPending
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'MacRecovery'
             AND NEW.node_effect = 'MacReseedPending'
             AND NEW.evidence_text = 'BadHashPrev'
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: NotRegisteredRro/NotRegisteredSigner → FnConfigError / FnConfigError
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'FnConfigError'
             AND NEW.node_effect = 'FnConfigError'
             AND NEW.evidence_text IN ('NotRegisteredRro','NotRegisteredSigner')
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- UnknownStatus: SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / TransientRetry / NoNodeEffect / code + digest 32B
        WHEN NEW.evidence_kind = 'UnknownStatus'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NOT NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- SaveError: SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / TransientRetry / NoNodeEffect / digest 32B
        WHEN NEW.evidence_kind = 'SaveError'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- CloseAmbiguous: SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / ProbeRequired / ProbeRequired / digest 32B
        WHEN NEW.evidence_kind = 'CloseAmbiguous'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- MissingStatus: SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / ProbeRequired / ProbeRequired / digest 32B
        WHEN NEW.evidence_kind = 'MissingStatus'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- OkButNoFiscalNumber: SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE / ProbeRequired / ProbeRequired / digest 32B
        WHEN NEW.evidence_kind = 'OkButNoFiscalNumber'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        ELSE 0
    END
), 0) <> 1
BEGIN SELECT RAISE(ABORT, 'delivery_reservation evidence: no valid matrix leaf matched at OUTCOME_OBSERVED (fail-closed)'); END;

-- Evidence matrix trigger — fires on UPDATE when transitioning to OUTCOME_OBSERVED
-- or when any evidence/axis column changes while already at OUTCOME_OBSERVED.
-- Uses the same COALESCE(CASE…ELSE 0 END, 0) <> 1 form to close the NULL-bypass.
CREATE TRIGGER delivery_reservation_evidence_matrix_update
BEFORE UPDATE ON delivery_reservation
WHEN NEW.state = 'OUTCOME_OBSERVED'
AND COALESCE((
    CASE
        -- PreconditionFailed
        WHEN NEW.evidence_kind = 'PreconditionFailed'
             AND NEW.submission_certainty = 'NOT_SUBMITTED'
             AND NEW.response_provenance = 'NO_RESPONSE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- SigningFailed
        WHEN NEW.evidence_kind = 'SigningFailed'
             AND NEW.submission_certainty = 'NOT_SUBMITTED'
             AND NEW.response_provenance = 'NO_RESPONSE'
             AND NEW.routing_class = 'WrapperBug'
             AND NEW.node_effect = 'WrapperBug'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- NoResponse
        WHEN NEW.evidence_kind = 'NoResponse'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'NO_RESPONSE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NOT NULL
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- RemoteAuthStatus
        WHEN NEW.evidence_kind = 'RemoteAuthStatus'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'AUTHENTICATED_PEER'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Accepted
        WHEN NEW.evidence_kind = 'Accepted'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class IS NULL
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NOT NULL AND length(NEW.evidence_text) > 0
             AND NEW.evidence_code IS NULL
             AND NEW.evidence_digest IS NULL
             AND NEW.remote_correlation_id = NEW.evidence_text
        THEN 1
        -- Rejected: Verify/Type/Xml/XmlDate/XmlChk/XmlZReport/OfflineId/Close → TerminalReject / NoNodeEffect
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TerminalReject'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IN ('Verify','Type','Xml','XmlDate','XmlChk','XmlZReport','OfflineId','Close')
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: NotPrevZReport → OperatorEscalation / OperatorEscalation
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'OperatorEscalation'
             AND NEW.node_effect = 'OperatorEscalation'
             AND NEW.evidence_text = 'NotPrevZReport'
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: Offline168 → TerminalReject / NodeBlocked
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TerminalReject'
             AND NEW.node_effect = 'NodeBlocked'
             AND NEW.evidence_text = 'Offline168'
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: BadHashPrev → MacRecovery / MacReseedPending
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'MacRecovery'
             AND NEW.node_effect = 'MacReseedPending'
             AND NEW.evidence_text = 'BadHashPrev'
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- Rejected: NotRegisteredRro/NotRegisteredSigner → FnConfigError / FnConfigError
        WHEN NEW.evidence_kind = 'Rejected'
             AND NEW.submission_certainty = 'SUBMITTED'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'FnConfigError'
             AND NEW.node_effect = 'FnConfigError'
             AND NEW.evidence_text IN ('NotRegisteredRro','NotRegisteredSigner')
             AND length(NEW.evidence_digest) = 32
             AND NEW.evidence_code IS NULL
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- UnknownStatus
        WHEN NEW.evidence_kind = 'UnknownStatus'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NOT NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- SaveError
        WHEN NEW.evidence_kind = 'SaveError'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'TransientRetry'
             AND NEW.node_effect = 'NoNodeEffect'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- CloseAmbiguous
        WHEN NEW.evidence_kind = 'CloseAmbiguous'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- MissingStatus
        WHEN NEW.evidence_kind = 'MissingStatus'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        -- OkButNoFiscalNumber
        WHEN NEW.evidence_kind = 'OkButNoFiscalNumber'
             AND NEW.submission_certainty = 'SUBMITTED_UNKNOWN'
             AND NEW.response_provenance = 'PARSED_DPS_ENVELOPE'
             AND NEW.routing_class = 'ProbeRequired'
             AND NEW.node_effect = 'ProbeRequired'
             AND NEW.evidence_text IS NULL
             AND NEW.evidence_code IS NULL
             AND length(NEW.evidence_digest) = 32
             AND NEW.remote_correlation_id IS NULL
        THEN 1
        ELSE 0
    END
), 0) <> 1
BEGIN SELECT RAISE(ABORT, 'delivery_reservation evidence: no valid matrix leaf matched at OUTCOME_OBSERVED (fail-closed)'); END;

-- ══════════════════════════════════════════════════════════════════════════════
-- §7  EVIDENCE IMMUTABILITY TRIGGER
--
-- Freezes the four evidence columns (evidence_kind, evidence_text, evidence_code,
-- evidence_digest) after OUTCOME_OBSERVED.  Uses null-safe `IS NOT` comparisons
-- so a NULL → NULL transition does not trigger falsely, and a NULL → non-NULL
-- or non-NULL → different-value mutation is caught.
-- ══════════════════════════════════════════════════════════════════════════════
CREATE TRIGGER delivery_reservation_evidence_immutable
BEFORE UPDATE ON delivery_reservation
WHEN OLD.state = 'OUTCOME_OBSERVED'
  AND (OLD.evidence_kind    IS NOT NEW.evidence_kind
    OR OLD.evidence_text    IS NOT NEW.evidence_text
    OR OLD.evidence_code    IS NOT NEW.evidence_code
    OR OLD.evidence_digest  IS NOT NEW.evidence_digest)
BEGIN SELECT RAISE(ABORT, 'delivery_reservation evidence columns are immutable after OUTCOME_OBSERVED'); END;
