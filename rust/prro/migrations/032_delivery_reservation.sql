-- 032 — delivery_reservation (INACTIVE — CS-2 §2b, Spec #4 part A §3)
--
-- WHY
-- ---
-- CS-2 lands the durable `delivery_reservation` table as the single
-- certainty / fence authority for the Sending / CallStarted window
-- (Spec #4 §1 — the FOUR durable structures + their authority).  It owns
-- the call lifecycle `ReservedNotStarted → CallStarted → OutcomeObserved`
-- and delivery certainty via the three orthogonal Spec #2 §2 fields
-- (`submission_certainty`, `response_provenance`, `routing_class`).  Per
-- A4-2 this is the ONLY certainty source; `fiscal_documents.state` and
-- `transport_trace` are never the certainty / fence basis.
--
-- **INACTIVE (behaviour-neutral).**  This migration creates schema only.
-- NO production caller writes or reads the table in CS-2: the write-path,
-- boot-resume, and drain paths are untouched.  The activation (record-
-- then-apply with `ObservedOutcomeV1`, `node_state.delivery_generation`,
-- the rebuilt fence predicate, apply-states) is CS-3 (Spec #4 §5).  Boot
-- DOES apply this migration (schema + `_sqlx_migrations` row); the only
-- runtime cost is the additive `ux_fd_docid_fn` unique index on the
-- existing `fiscal_documents` (small write/disk cost, NOT self-scoped).
--
-- STRICT TABLE NOTE
-- -----------------
-- `delivery_reservation` is created STRICT.  Under STRICT, every column
-- must declare one of the allowed storage classes; BLOB / TEXT / INTEGER
-- are used here.  The two 16-byte identity BLOBs and the 32-byte
-- `envelope_hash` are length-CHECKed (`length(...) = 16 / 32`) because
-- STRICT enforces affinity but NOT length.  `CURRENT_TIMESTAMP` DEFAULTs
-- on `created_at` / `updated_at` are TEXT, matching the STRICT `fiscal_
-- documents` timestamp convention (001_baseline.sql).  The composite
-- foreign key `(document_id, fiscal_number) → fiscal_documents` requires
-- a UNIQUE index on the parent pair; `ux_fd_docid_fn` supplies it and is
-- therefore created BEFORE the child table so the FK resolves at CREATE.
--
-- BACKWARD COMPATIBILITY
-- ----------------------
-- Purely additive.  One new table, one new unique index on the existing
-- `fiscal_documents(document_id, fiscal_number)`, two partial indexes and
-- six triggers scoped to the new table.  No existing table, column,
-- index, or trigger is modified — the `sqlite_master` rows for every
-- pre-032 object are byte-identical after apply (merge pin §6.2).  Existing
-- rows are untouched; the new table starts empty and stays empty in CS-2
-- (no caller).  `ux_fd_docid_fn` deliberately omits `IF NOT EXISTS`: a
-- pre-existing index of that name (schema drift) MUST fail the migration
-- loud rather than silently no-op.
--
-- ROLLBACK REASONING
-- ------------------
-- Forward: `CREATE UNIQUE INDEX` + `CREATE TABLE ... STRICT` + partial
-- indexes + triggers (all atomic in SQLite's implicit DDL transaction).
-- Reverse: the table is append-only (the delete trigger forbids row
-- removal, not `DROP TABLE`); a schema rollback would `DROP TABLE
-- delivery_reservation` then `DROP INDEX ux_fd_docid_fn`.  Because CS-2
-- is INACTIVE the table carries no rows, so a rollback loses no fiscal
-- state.  Pre-pilot schema posture: rollback = DB reset.
--
-- LIVE FILE SEQUENCE
-- ------------------
-- This file is 032; sqlx applies migrations by filename prefix order and
-- freezes the file's checksum on apply (`db/mod.rs`).  031 added the EPZ
-- doc_type to `fiscal_documents`; 032 adds the new `delivery_reservation`
-- table.  This is a NEW file — 001–031 are untouched so their sqlx
-- checksums are preserved.

-- rust/prro/migrations/032_delivery_reservation.sql   (INACTIVE — CS-2 §2b)

-- Additive UNIQUE index on the EXISTING fiscal_documents (NOT self-scoped: adds a small write/disk
-- cost to fiscal_documents), created BEFORE the child table so the composite FK resolves. NO
-- "IF NOT EXISTS": a pre-existing collision must fail loud (schema-drift detector).
CREATE UNIQUE INDEX ux_fd_docid_fn ON fiscal_documents(document_id, fiscal_number);

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

    -- ── structural-consistency matrix (NOT the full valid-combos table; the normative classifier
    --    table + typed constructor are CS-3). Per-row; the FSM legality is the transition trigger. ──
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

    FOREIGN KEY (document_id, fiscal_number)
        REFERENCES fiscal_documents(document_id, fiscal_number) ON DELETE RESTRICT,
    UNIQUE (document_id, attempt_no)
) STRICT;

-- FENCE: hold the FN while in-flight, SubmittedUnknown, OR an observed reject/degraded (SUBMITTED +
-- routing != NULL). Release ONLY on clean accept (SUBMITTED + routing NULL) or safe pre-call cancel
-- (NOT_SUBMITTED ⇒ call_started_at NULL by CHECK). CS-3 rebuilds this with PENDING_APPLY/APPLIED.
CREATE UNIQUE INDEX ux_reservation_active ON delivery_reservation(fiscal_number)
    WHERE state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
       OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED_UNKNOWN')
       OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED' AND routing_class IS NOT NULL);

CREATE INDEX ix_reservation_call_started ON delivery_reservation(fiscal_number) WHERE state = 'CALL_STARTED';

-- Insert only as RESERVED_NOT_STARTED.
CREATE TRIGGER delivery_reservation_insert_state
BEFORE INSERT ON delivery_reservation WHEN NEW.state <> 'RESERVED_NOT_STARTED'
BEGIN SELECT RAISE(ABORT, 'reservation must be inserted as RESERVED_NOT_STARTED'); END;

-- No-REPLACE collision-guard (audit round-4 §1): `INSERT OR REPLACE` resolves a PK / unique-index
-- conflict by DELETE+INSERT, which (with recursive_triggers OFF, db/mod.rs) bypasses the
-- UPDATE/DELETE triggers and can evict a live reservation or clear a marker. `INSERT OR REPLACE` is
-- a real path in this codebase (document_files.rs:124). Fail-closed BEFORE INSERT on ANY collision:
-- same reservation_id, same (document_id, attempt_no), or an existing ACTIVE (fenced) row on the FN.
CREATE TRIGGER delivery_reservation_no_replace
BEFORE INSERT ON delivery_reservation
WHEN EXISTS (SELECT 1 FROM delivery_reservation WHERE reservation_id = NEW.reservation_id)
  OR EXISTS (SELECT 1 FROM delivery_reservation WHERE document_id = NEW.document_id AND attempt_no = NEW.attempt_no)
  OR EXISTS (SELECT 1 FROM delivery_reservation WHERE fiscal_number = NEW.fiscal_number
        AND (state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
          OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED_UNKNOWN')
          OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED' AND routing_class IS NOT NULL)))
BEGIN SELECT RAISE(ABORT, 'delivery_reservation: collision on reservation_id / (document_id,attempt_no) / active fence — INSERT OR REPLACE forbidden'); END;

-- Transition legality (audit round-3 §1): the ONLY legal edges. `IS`/`IS NOT` are null-safe.
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

-- Immutability (audit round-3 §3): identity/binding/marker always frozen (null-safe IS NOT);
-- outcome fields frozen once OUTCOME_OBSERVED (incl NULL→value on a later update).
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
BEGIN SELECT RAISE(ABORT, 'immutable field mutation on delivery_reservation'); END;

-- Append-only (audit round-3 §3/§6): NO delete ever — closes the attempt_no-reuse hole and the
-- fence-delete hole in one. The table is a durable audit trail (like transport_trace).
CREATE TRIGGER delivery_reservation_append_only
BEFORE DELETE ON delivery_reservation
BEGIN SELECT RAISE(ABORT, 'delivery_reservation is append-only'); END;

CREATE TRIGGER delivery_reservation_updated_at
AFTER UPDATE ON delivery_reservation
BEGIN UPDATE delivery_reservation SET updated_at = CURRENT_TIMESTAMP WHERE reservation_id = NEW.reservation_id; END;
