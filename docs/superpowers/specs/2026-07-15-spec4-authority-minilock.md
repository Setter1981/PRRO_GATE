# Spec #4 (part A) — Authority Mini-Lock + Migration 032 Co-Draft

**Status: 🟡 DRAFT rev 5 (external audit round 4 + internal Sonnet SQL pass → two SQL fixes, then mint). 2026-07-15. Grounded on `origin/main` `f2628ba`.**
Architecture settled (rounds 1-2); SQL triggers/matrix settled (rounds 3-4). Rev 5 closes the last two
gaps both reviewers converged on: (a) `INSERT OR REPLACE` bypassed the UPDATE/DELETE triggers (a
BEFORE-INSERT collision-guard now blocks it), and (b) `DrainChainSettleRetry` could pair with a
fence-releasing `NOT_SUBMITTED`. Both the external auditor (round 4) and the decorrelated internal
Sonnet pass (real SQLite 3.45.1, ~30 attacks) reported everything else clean. **Anchors:** Spec #1 +
Spec #2 (rev 2) DESIGN-LOCKED. **The schema-implementer runs only after this re-audits to `mint 032`.**

---

## 0 · Why a mini-lock before the DDL (the gate)
`sqlx`'s checksum (`db/mod.rs:37`) freezes 032's columns + CHECK/trigger literals on apply. Four
durable structures touch the *Sending / CallStarted* window; fixing **which owns what** before minting
prevents the frozen file from cementing an unresolved contract.

## 1 · The FOUR durable structures + their authority (NORMATIVE)
| structure | **owns** | **NEVER** |
|---|---|---|
| `delivery_reservation` (NEW 032, **INACTIVE**) | call lifecycle `ReservedNotStarted→CallStarted→OutcomeObserved`; **delivery certainty** (3 orthogonal fields, Spec #2 §2) | — |
| `node_state` | **chain-tip seed** (`last_known_unsigned_xml_sha256`, `stage_send.rs:1748`); **CS-3** fence token/pointer | (CS-2: no new columns — §5) |
| `fiscal_documents` | fiscal doc-FSM + atomically-applied **projection** of the outcome (`Sending→Sent + sfn` CAS) | not the certainty/fence source |
| `transport_trace` | **forensic / observability ONLY** (`transport_trace.rs:6`) | never a basis for resend / seed / fence |

## 2 · Invariants (mini-lock — normative)
- **A4-1 (forensic-only — honest baseline).** **CURRENT (`f2628ba`):** `transport_trace` is the *legacy authority* for ER-redrive — `transport_trace.rs:320/:390`, `er_redrive_policy.rs:80`, callers `boot_phase.rs:3066` / `backlog_drain.rs:1537` / `online_convergence.rs:559`. Seed advance already does not read trace. **TARGET (after CS-3 cutover):** forensic-only; those three redrive paths move to the reservation's typed outcome.
- **A4-2 (single certainty/fence authority).** `delivery_reservation` is the only certainty source. CS-2 fence = the partial unique index `ux_reservation_active` (fail-closed). CS-3 adds the durable token (`node_state.delivery_generation`) and rebuilds the predicate; certainty/fence are never read from `fiscal_documents.state`/`transport_trace`.
- **A4-3 (fiscal_documents = projection).** Outcome applied atomically via the existing `Sending→Sent + sfn` CAS; `(Sent,Rejected)` edge stays removed; post-SENT reject → `RequiresManualReconciliation`, seed never rolled back.
- **A4-4 (immutable protocol binding).** Binding columns snapshot at creation, immutable, carried through every retry; a doc retries only on its bound `dps_protocol_id` — extends frozen invariant #3 to protocol.
- **A4-5 (cross-protocol forbidden by default — LOCK-READY).** `SubmittedUnknown` on A is never permission to act on B; reconciliation first on the original protocol. Lifted only on ALL of: an official DPS identity/correlation contract, proven cross-protocol consistency/visibility, a declared `ReconciliationCapability`, cross-adapter conformance + negative tests. Until then unknown ⇒ deny. Does not block CS-2.
- **A4-6 (record-then-apply — LOCK-READY; CS-3 activation contract).** (1) the outcome evidence (the three fields + `remote_correlation_id`, a self-contained `ObservedOutcomeV1`) commits **as authority** first; (2) if projection guards pass → `document` + `node_state` seed + audit + fence-release commit atomically in one `BEGIN IMMEDIATE`; (3) if a guard fails → commit `OUTCOME_RECORDED_PENDING_APPLY`, the fence stays; (4) **only the ledger apply repeats — the DPS wire-call is NEVER repeated, evidence never lost.** The coordinator/actor-mediated form is **CS-4**; CS-3 does the narrower record-then-apply with `ObservedOutcomeV1`.

## 3 · Migration 032 co-draft rev 4 (governed by §1–§2) — `delivery_reservation` ONLY
No `ingress_inbox` delta (Spec #3 first). No `generation` (CS-3 activation). No apply-states/payload
(CS-3, `ObservedOutcomeV1`). Full 5-section header.

```sql
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
```
**Normative (repo API):** `outbox.rs`-style runtime `sqlx::query`, tx-only `insert` + pool
`get_active_for_fn`; **NO caller**; `attempt_no = MAX(attempt_no)+1` inside one `BEGIN IMMEDIATE`
(`tx.rs:118`; UNIQUE backstop); append-only (no reuse — enforced by the delete trigger).
`DrainChainSettleRetry` is legacy-hydration only — the CS-3 typed constructor forbids fresh emission.

## 4 · RED-pins (CS-3 activation contract — known-red until CS-3)
- **RP-A4-1 (fence source):** seed-advance reads the fence token from `node_state.delivery_generation` vs `reservation`, not `fiscal_documents`/`transport_trace`.
- **RP-A4-2 (forensic cutover):** after CS-3, no resend/seed/fence path reads `transport_trace`; the three `f2628ba` consumer-paths are gone.
- **RP-A4-3a/b/c (fence holds):** a 2nd reservation is refused when the first is `OUTCOME_OBSERVED` with `SUBMITTED_UNKNOWN` (a) or `SUBMITTED + routing!=NULL` (b); `NOT_SUBMITTED` with `call_started_at NOT NULL` is CHECK-rejected (c).
- **RP-A4-3d (transition legality):** `CALL_STARTED→NOT_SUBMITTED`, `RESERVED_NOT_STARTED→OUTCOME_OBSERVED(SUBMITTED*)`, `CALL_STARTED→RESERVED_NOT_STARTED`, and marker/outcome mutation are all trigger-`ABORT`ed; `RN→CS→OO` succeeds.
- **RP-A4-3e (no INSERT OR REPLACE):** `INSERT OR REPLACE` (or any colliding INSERT) on an existing `reservation_id` / `(document_id, attempt_no)` / active-FN is `ABORT`ed by the collision-guard — no eviction of a live reservation, no trigger-bypassing DELETE+INSERT.
- **RP-A4-4 (bound protocol):** a doc whose `fn_outgress_profile` flips mid-shift still retries on its bound `dps_protocol_id`.
- **RP-A4-5 (record-then-apply):** a crash after `OutcomeObserved` re-applies the recorded `ObservedOutcomeV1` idempotently; the DPS call is never repeated.
- **RP-A4-6 (no blind resend):** `er_redrive` does not blind-resend a possibly-submitted doc on Transport-timeout (Spec #2 RP-1).

## 5 · Deferred to CS-3 (single activation migration; LOCK-READY per audit)
`generation` + `node_state.delivery_generation` + `active_delivery_reservation_id` + apply-states +
the self-contained **`ObservedOutcomeV1`** payload (NOT `TransitionPlan` — that + the actor are
**CS-4**; roadmap:44). The activation migration **fail-fast requires an empty `delivery_reservation`**,
rebuilds CHECK/index, completes **before** any caller. — `ingress_inbox.idempotency_strategy`: Spec #3
first. — Fleet command lifecycle: Spec #5 read-only telemetry only. — `ABORTED`/EPZ CHECK history:
untouched (025/030/031 authority).

## 6 · INACTIVE merge pins (audit — boot DOES apply the migration)
Fiscal/write-path behaviour-neutral (empty table, no callers) — but boot applies 032 (schema +
`_sqlx_migrations`; `ux_fd_docid_fn` adds a small write/disk cost to `fiscal_documents`, NOT
self-scoped). Merge gate:
1. **upgrade 031→032 on a non-empty representative DB** (not only a fresh pool);
2. **`sqlite_master` diff** — pre-existing objects byte-identical; only the expected new objects added (incl. `ux_fd_docid_fn` on `fiscal_documents`);
3. **production-flow test** — after a normal fiscalisation, `delivery_reservation` stays **empty**;
4. **static call-graph pin** — allowed: migration + repository persistence tests; **forbidden: any production caller** of the `delivery_reservation` repo;
5. **constraint / truth-table / phase-laundering matrix** — the 3-field structural matrix; a 2nd reservation after every unsafe state (`SUBMITTED_UNKNOWN`; `SUBMITTED + routing!=NULL` — both rejected); plus the negative pins: marker `value→NULL`; `RN→OO(SUBMITTED)`; `CS→RN`; clean `routing NULL→TerminalReject`; mutation of protocol versions / `remote_correlation_id`; a resolved-row DELETE (rejected — append-only) proving no `attempt_no` reuse; **`INSERT OR REPLACE` by `reservation_id` rejected** (no trigger bypass); **`INSERT OR REPLACE` that would evict an active-FN reservation rejected**; **`DrainChainSettleRetry + NOT_SUBMITTED/NO_RESPONSE` rejected** (no legacy fence-release); a **negative `capability_profile_version` / `endpoint_config_revision` rejected**; composite-FK mismatch (doc FN-A under fence FN-B); concurrent `attempt_no` (1,2); the positive `RN→CS→OO`; the CS-3 activation-empty / pre-feature gate.

## 7 · Mint-candidate status
Round-4 (external) resolved: transition legality, immutability, append-only, fence, composite FK,
`attempt_no`, `BEFORE UPDATE OF` completeness — all confirmed; the two residuals (`INSERT OR REPLACE`,
legacy `DrainChainSettleRetry`) are closed above (collision-guard + the DrainChain CHECK). The
decorrelated internal Sonnet pass (real SQLite 3.45.1) independently confirmed the same clean surface
and its one finding (`SUBMITTED_UNKNOWN + NO_RESPONSE`) is adjudicated **valid** (blessed in §3). The
only deferred item is the full routing↔(certainty,provenance) **typed classifier — explicitly CS-3**.
**On a clean round-5 re-verify of the two rev-5 fixes, this is `mint 032`.**
