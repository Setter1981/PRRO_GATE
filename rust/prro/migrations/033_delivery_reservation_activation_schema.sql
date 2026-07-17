-- 033 — delivery_reservation activation schema + node_state generation columns
--       (INACTIVE — CS-3 Slice B, Spec #4 part A §5)
--
-- WHY
-- ---
-- CS-3 activation requires three durable structures beyond the 032 baseline:
--
--   1. `node_state.delivery_generation` — a monotonic integer token used by the
--      CS-3 fence predicate to detect stale reservations.  The generation is
--      incremented (by CS-3 caller logic) each time the node advances through a
--      guarded UPDATE and an outcome is committed.  DEFAULT 0 = no generation
--      consumed yet.
--
--   2. `node_state.active_delivery_reservation_id` — a durable pointer from the
--      node to the currently-active reservation (16-byte BLOB, nullable).  Set
--      atomically with the CS-3 guarded UPDATE; NULL when no reservation is
--      active.
--
--   3. `delivery_reservation.authorized_generation` — the generation snapshot
--      captured at the `RN → CALL_STARTED` transition.  Compared against the
--      current `node_state.delivery_generation` to detect intervening fence
--      advances.  NULL at `RESERVED_NOT_STARTED`; set at `RN→CS`; IMMUTABLE
--      thereafter.  CHECK (NULL or >= 1) — generation 0 is the uninitialised
--      default, never a valid authorization token.
--
--   4. `delivery_reservation.apply_state` — the two-commit record-then-apply
--      boundary (Spec #4 A4-6): the outcome evidence is committed first (authority
--      recorded); then the projection (document + seed advance) is applied in a
--      second commit.  A crash between the two commits leaves the row in
--      `PENDING_APPLY`; recovery re-applies idempotently.  `apply_state` is
--      orthogonal to `state` (never overloads it).  Legal transitions:
--      NULL → PENDING_APPLY → APPLIED.  Monotone: no regression.
--
--   5. `delivery_reservation.node_effect` — the semantic discriminant for
--      node-mode side-effects so they are reconstructable from durable evidence
--      (a recovery path that re-reads the row can derive the correct action
--      without re-running the DPS wire call).  Vocabulary grounded on
--      `error_routing.rs` (RetryClass × node_mode_flip):
--        'NodeBlocked'       — Server -11 ERROR_OFFLINE_168 → NodeMode::Blocked flip
--        'MacReseedPending'  — Server -12 ERROR_BAD_HASH_PREV → MAC recovery orchestrator
--        'ProbeRequired'     — Decode / Server -2 close-shift / Server -15 close-shift
--                              → needs last_chk probe (ProbeReason enum)
--        'OperatorEscalation'— Server -6 ERROR_NOT_PREV_ZREPORT → operator-recoverable
--        'FnConfigError'     — Server -13/-14 NOT_REGISTERED → FnConfigError class
--        'WrapperBug'        — Internal / NotFound / FiscalIdMismatch / unknown code
--        'NoNodeEffect'      — TerminalReject (clean accept/reject) / TransientRetry:
--                              no node-mode change; result is doc-level only
--      NULL until OUTCOME_OBSERVED; IMMUTABLE once set.
--
-- **INACTIVE (behaviour-neutral).**  This migration creates/extends schema only.
-- NO production caller writes or reads the new columns in CS-3 Slice B: the
-- write-path, boot-resume, and drain paths are untouched.  Boot DOES apply this
-- migration (schema + `_sqlx_migrations` row).
--
-- REBUILD DECISION
-- ----------------
-- `delivery_reservation` is STRICT.  `ALTER TABLE ... ADD COLUMN` on a STRICT
-- table in SQLite 3.45.1 cannot carry per-column CHECK constraints (the engine
-- validates the CHECK only for newly inserted/updated rows, but the column-level
-- syntax is rejected for STRICT tables with non-trivial CHECKs).  Since 032 is
-- INACTIVE and the table is ALWAYS EMPTY at migration time (the fail-fast guard
-- below enforces this), the cleanest approach is a full table rebuild:
--   (a) the fail-fast guard aborts the migration if the table is non-empty;
--   (b) drop all 032 triggers, indexes, and the table itself;
--   (c) re-CREATE with the full 20-column DDL (all 032 columns verbatim +
--       the 3 new 033 columns) + all 032 CHECKs + new 033 CHECKs;
--   (d) re-CREATE all 032 indexes and triggers + new 033 triggers.
-- This gives a clean `sqlite_master` row with the full DDL visible, preserves
-- all 032 guarantees verbatim (copy-exact), and avoids trigger-based workarounds
-- for missing column-level CHECKs.
--
-- `node_state` is NOT STRICT and has live rows (FN configs already inserted).
-- ADD COLUMN is the correct and safe approach there: both new columns are
-- nullable (or have a valid DEFAULT) and the CHECKs satisfy the SQLite ADD COLUMN
-- rules (CHECK evaluates true for the DEFAULT value).  An enforcement trigger is
-- added for the BLOB-length CHECK on `active_delivery_reservation_id` (ADD COLUMN
-- with a CHECK that references a non-NULL default is valid; the trigger is the
-- defence-in-depth layer for UPDATEs).
--
-- FAIL-FAST ACTIVATION GUARD
-- ---------------------------
-- Mirrors migration 027: a TEMP table with CHECK (count = 0) aborts the migration
-- transaction if `delivery_reservation` is non-empty.  sqlx wraps migrations
-- transactionally (db/mod.rs); the abort rolls back everything and the migration
-- is never recorded in `_sqlx_migrations`.  An operator who needs to activate on
-- a DB with rows must drain/archive the table first.
--
-- BACKWARD COMPATIBILITY
-- ----------------------
-- Additive to `node_state` (two ADD COLUMNs; existing rows read the DEFAULTs).
-- `delivery_reservation` is rebuilt in-place from an empty state; pre-existing
-- objects for 001–032 are unmodified.  032 is NOT edited.
--
-- ROLLBACK REASONING
-- ------------------
-- Forward: DDL only (no data); the guard ensures the rebuild loses no fiscal rows.
-- Reverse: `DROP COLUMN` the two `node_state` additions; restore the 032
-- `delivery_reservation` table (which has no rows either way).  Pre-pilot posture:
-- rollback = DB reset (as with 032).
--
-- LIVE FILE SEQUENCE
-- ------------------
-- This file is 033; sqlx applies by filename prefix order.  032 is untouched.

-- rust/prro/migrations/033_delivery_reservation_activation_schema.sql
--   (INACTIVE — CS-3 Slice B)

-- ══════════════════════════════════════════════════════════════════════════════
-- §1  FAIL-FAST ACTIVATION GUARD (mirrors migration 027)
-- ══════════════════════════════════════════════════════════════════════════════
-- Abort if delivery_reservation is non-empty.  A non-zero count violates
-- the CHECK (= 0), aborting the migration transaction.  On a clean DB the
-- count is 0, the INSERT succeeds, and the temp table is dropped.
DROP TABLE IF EXISTS _m033_activation_guard;
CREATE TEMP TABLE _m033_activation_guard (
    row_count INTEGER NOT NULL CHECK (row_count = 0)
);
INSERT INTO _m033_activation_guard (row_count)
SELECT COUNT(*) FROM delivery_reservation;
DROP TABLE _m033_activation_guard;

-- ══════════════════════════════════════════════════════════════════════════════
-- §2  TEAR DOWN 032 delivery_reservation objects (triggers → indexes → table)
-- ══════════════════════════════════════════════════════════════════════════════
-- Drop order matters: triggers first (depend on the table), then dependent
-- indexes, then the table itself.  ux_fd_docid_fn is on fiscal_documents (NOT
-- on delivery_reservation) so it survives and is NOT touched here.

DROP TRIGGER delivery_reservation_updated_at;
DROP TRIGGER delivery_reservation_append_only;
DROP TRIGGER delivery_reservation_immutable;
DROP TRIGGER delivery_reservation_transition;
DROP TRIGGER delivery_reservation_no_replace;
DROP TRIGGER delivery_reservation_insert_state;

DROP INDEX ix_reservation_call_started;
DROP INDEX ux_reservation_active;

DROP TABLE delivery_reservation;

-- ══════════════════════════════════════════════════════════════════════════════
-- §3  RE-CREATE delivery_reservation WITH 033 SCHEMA (verbatim 032 + 3 new cols)
-- ══════════════════════════════════════════════════════════════════════════════
-- All 032 columns + CHECKs are copied verbatim (byte-identical to what SQLite
-- stored in sqlite_master for 032).  The 3 new columns follow at the end.

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

    -- ── 033 new columns ───────────────────────────────────────────────────────
    -- Generation snapshot: NULL at RNS, set at RN→CS, IMMUTABLE thereafter.
    -- CHECK: NULL or >= 1 (generation 0 = uninitialised default, not a valid token).
    authorized_generation INTEGER CHECK (authorized_generation IS NULL OR authorized_generation >= 1),
    -- Apply-state: the two-commit record-then-apply boundary (A4-6).
    -- NULL until OUTCOME_OBSERVED; transitions only NULL→PENDING_APPLY→APPLIED (monotone).
    -- Orthogonal to `state` — never overloads it.
    apply_state           TEXT    CHECK (apply_state IS NULL OR apply_state IN ('PENDING_APPLY','APPLIED')),
    -- Node-effect discriminant: the semantic node-mode side-effect so recovery can
    -- reconstruct the correct action from durable evidence (grounded on error_routing.rs).
    -- NULL until OUTCOME_OBSERVED; IMMUTABLE once set.
    node_effect           TEXT    CHECK (node_effect IS NULL OR node_effect IN (
                            'NodeBlocked',       -- Server -11 → NodeMode::Blocked
                            'MacReseedPending',  -- Server -12 → MAC recovery orchestrator
                            'ProbeRequired',     -- Decode / -2 close-shift / -15 close-shift
                            'OperatorEscalation',-- Server -6 → operator-recoverable
                            'FnConfigError',     -- Server -13/-14 → FnConfigError class
                            'WrapperBug',        -- Internal / NotFound / FiscalIdMismatch
                            'NoNodeEffect'       -- clean terminal / transient: doc-level only
                          )),

    -- ── structural-consistency matrix (032, verbatim) ────────────────────────
    -- NOT the full valid-combos table; the normative classifier table + typed
    -- constructor are CS-3.  Per-row; the FSM legality is the transition trigger.
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

    -- ── 033 field↔state lifecycle CHECKs (structural, mirror 032's call_started_at /
    --    remote_correlation_id patterns so an illegal-lifecycle write is unrepresentable,
    --    not merely trigger-governed) ──
    -- authorized_generation is the RN→CS snapshot: NULL at RESERVED_NOT_STARTED.
    CHECK (state <> 'RESERVED_NOT_STARTED' OR authorized_generation IS NULL),
    -- apply_state (record-then-apply boundary) exists ONLY once an outcome is observed:
    CHECK (apply_state IS NULL OR state = 'OUTCOME_OBSERVED'),
    -- node_effect (semantic node-mode discriminant) exists ONLY once an outcome is observed:
    CHECK (node_effect IS NULL OR state = 'OUTCOME_OBSERVED'),

    FOREIGN KEY (document_id, fiscal_number)
        REFERENCES fiscal_documents(document_id, fiscal_number) ON DELETE RESTRICT,
    UNIQUE (document_id, attempt_no)
) STRICT;

-- ══════════════════════════════════════════════════════════════════════════════
-- §4  RE-CREATE 032 INDEXES (verbatim)
-- ══════════════════════════════════════════════════════════════════════════════
-- ux_fd_docid_fn is on fiscal_documents (NOT delivery_reservation) — survives the
-- table drop above; NOT recreated here.

-- FENCE: hold the FN while in-flight, SubmittedUnknown, OR an observed reject/degraded.
-- CS-3 rebuilds this with PENDING_APPLY/APPLIED once activation wires the callers.
CREATE UNIQUE INDEX ux_reservation_active ON delivery_reservation(fiscal_number)
    WHERE state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
       OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED_UNKNOWN')
       OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED' AND routing_class IS NOT NULL);

CREATE INDEX ix_reservation_call_started ON delivery_reservation(fiscal_number) WHERE state = 'CALL_STARTED';

-- ══════════════════════════════════════════════════════════════════════════════
-- §5  RE-CREATE 032 TRIGGERS (verbatim — byte-identical copies)
-- ══════════════════════════════════════════════════════════════════════════════

-- Insert only as RESERVED_NOT_STARTED.
CREATE TRIGGER delivery_reservation_insert_state
BEFORE INSERT ON delivery_reservation WHEN NEW.state <> 'RESERVED_NOT_STARTED'
BEGIN SELECT RAISE(ABORT, 'reservation must be inserted as RESERVED_NOT_STARTED'); END;

-- No-REPLACE collision-guard (audit round-4 §1).
CREATE TRIGGER delivery_reservation_no_replace
BEFORE INSERT ON delivery_reservation
WHEN EXISTS (SELECT 1 FROM delivery_reservation WHERE reservation_id = NEW.reservation_id)
  OR EXISTS (SELECT 1 FROM delivery_reservation WHERE document_id = NEW.document_id AND attempt_no = NEW.attempt_no)
  OR EXISTS (SELECT 1 FROM delivery_reservation WHERE fiscal_number = NEW.fiscal_number
        AND (state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
          OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED_UNKNOWN')
          OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED' AND routing_class IS NOT NULL)))
BEGIN SELECT RAISE(ABORT, 'delivery_reservation: collision on reservation_id / (document_id,attempt_no) / active fence — INSERT OR REPLACE forbidden'); END;

-- Transition legality (audit round-3 §1): the ONLY legal edges.
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

-- Immutability (audit round-3 §3 + 033 extension for authorized_generation and node_effect).
-- Identity/binding/marker always frozen (null-safe IS NOT).
-- Outcome fields frozen once OUTCOME_OBSERVED (incl NULL→value on a later update).
-- authorized_generation frozen once non-NULL (set once at RN→CS).
-- node_effect frozen once non-NULL (set once at OUTCOME_OBSERVED).
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

-- Append-only (audit round-3 §3/§6): NO delete ever.
CREATE TRIGGER delivery_reservation_append_only
BEFORE DELETE ON delivery_reservation
BEGIN SELECT RAISE(ABORT, 'delivery_reservation is append-only'); END;

-- updated_at maintenance.
CREATE TRIGGER delivery_reservation_updated_at
AFTER UPDATE ON delivery_reservation
BEGIN UPDATE delivery_reservation SET updated_at = CURRENT_TIMESTAMP WHERE reservation_id = NEW.reservation_id; END;

-- ══════════════════════════════════════════════════════════════════════════════
-- §6  033 NEW TRIGGER — apply_state monotone transition guard
-- ══════════════════════════════════════════════════════════════════════════════
-- Legal edges: NULL→PENDING_APPLY, PENDING_APPLY→APPLIED.
-- All other transitions (including regression and jump from NULL→APPLIED) are
-- illegal.  `IS` / `IS NOT` are null-safe.
-- Note: same-state no-op (OLD.apply_state IS NEW.apply_state) is allowed
-- (the updated_at trigger fires; no field-level mutation on apply_state itself).
-- All comparisons use null-safe `IS` / `IS NOT` so NULL in OLD/NEW is handled
-- correctly.  `X = Y` is NULL (falsy) when either side is NULL; `X IS Y` is
-- always TRUE or FALSE.
CREATE TRIGGER delivery_reservation_apply_state_transition
BEFORE UPDATE OF apply_state ON delivery_reservation
WHEN NOT (
       OLD.apply_state IS NEW.apply_state                                                     -- no-op / same-state
    OR (OLD.apply_state IS NULL AND NEW.apply_state IS 'PENDING_APPLY')                      -- NULL → PENDING_APPLY
    OR (OLD.apply_state IS 'PENDING_APPLY' AND NEW.apply_state IS 'APPLIED')                 -- PENDING_APPLY → APPLIED
)
BEGIN SELECT RAISE(ABORT, 'illegal apply_state transition on delivery_reservation'); END;

-- ══════════════════════════════════════════════════════════════════════════════
-- §7  node_state ADDITIONS (ADD COLUMN — safe for non-STRICT table with live rows)
-- ══════════════════════════════════════════════════════════════════════════════

-- Generation counter: monotonic fence token.  DEFAULT 0 = no generation consumed.
-- NOT NULL with DEFAULT 0 satisfies ADD COLUMN rules; CHECK (>= 0) enforced by
-- the supplementary trigger below for defence-in-depth on UPDATE.
ALTER TABLE node_state ADD COLUMN delivery_generation INTEGER NOT NULL DEFAULT 0
    CHECK (delivery_generation >= 0);

-- Pointer to the currently-active delivery_reservation (nullable 16-byte BLOB).
-- NULL when no reservation is active.  CHECK enforced by the supplementary
-- trigger below (ADD COLUMN cannot enforce length-CHECKs on non-NULL values
-- for existing rows in all SQLite versions; the trigger is authoritative).
ALTER TABLE node_state ADD COLUMN active_delivery_reservation_id BLOB
    CHECK (active_delivery_reservation_id IS NULL OR length(active_delivery_reservation_id) = 16);

-- Supplementary trigger: enforce the 16-byte length CHECK for UPDATE paths
-- (defence-in-depth; the column-level CHECK handles INSERT/UPDATE for new rows
-- under SQLite 3.45.1 but the trigger makes it explicit and version-agnostic).
CREATE TRIGGER node_state_active_reservation_id_check
BEFORE UPDATE OF active_delivery_reservation_id ON node_state
WHEN NEW.active_delivery_reservation_id IS NOT NULL
  AND length(NEW.active_delivery_reservation_id) <> 16
BEGIN SELECT RAISE(ABORT, 'node_state.active_delivery_reservation_id must be NULL or 16 bytes'); END;

-- Supplementary trigger: enforce delivery_generation >= 0 on UPDATE.
CREATE TRIGGER node_state_delivery_generation_check
BEFORE UPDATE OF delivery_generation ON node_state
WHEN NEW.delivery_generation < 0
BEGIN SELECT RAISE(ABORT, 'node_state.delivery_generation must be >= 0'); END;
