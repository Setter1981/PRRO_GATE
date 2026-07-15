# Spec #4 (part A) — Authority Mini-Lock + Migration 032 Co-Draft

**Status: 🟡 DRAFT rev 3 (post external audit round 2 → NOT-YET, narrow). 2026-07-15. Grounded on `origin/main` `f2628ba`.**
Rev 3 closes the two residual **under-fence** holes (B1), tightens the outcome truth-table (B2),
removes the decorative `generation` (B3 → CS-3), and corrects the CS-3/CS-4 ordering + the A4-6
apply/recompute discipline. **Anchors:** Spec #1 + Spec #2 (rev 2) DESIGN-LOCKED. **The
schema-implementer does NOT run until this re-audits to LOCK-READY.**

---

## 0 · Why a mini-lock before the DDL (the gate)
`sqlx`'s checksum (`db/mod.rs:37`) freezes 032's columns + CHECK literals on apply. Four durable
structures touch the *Sending / CallStarted* window; fixing **which owns what** before minting
prevents the frozen file from cementing an unresolved contract.

## 1 · The FOUR durable structures + their authority (NORMATIVE)
| structure | **owns** | **NEVER** |
|---|---|---|
| `delivery_reservation` (NEW 032, **INACTIVE**) | call lifecycle `ReservedNotStarted→CallStarted→OutcomeObserved`; **delivery certainty** (3 orthogonal fields, Spec #2 §2) | — |
| `node_state` | the **chain-tip seed** (`last_known_unsigned_xml_sha256`, `stage_send.rs:1748`); in **CS-3** the fence **token/pointer** (`delivery_generation` + `active_delivery_reservation_id`, added then) | (CS-2: no new columns — §5) |
| `fiscal_documents` | the **fiscal doc-FSM** + the atomically-applied **projection** of the outcome (`Sending→Sent + sfn` CAS, `stage_send.rs:1704`) | is not the certainty/fence source |
| `transport_trace` | **forensic / observability ONLY** (`transport_trace.rs:6`) | never a basis for resend / seed / fence |

## 2 · Invariants (mini-lock — normative)
- **A4-1 (forensic-only — honest baseline).** **CURRENT (`f2628ba`):** `transport_trace` is the *legacy authority* for ER-redrive — `last_attempt_retry_class_for` (`transport_trace.rs:320`), `attempts_used` (`:390`), `er_redrive_policy.rs:80` → `Redrive`, called from `boot_phase.rs:3066` / `backlog_drain.rs:1537` / `online_convergence.rs:559`. Seed advance already does not read trace. **TARGET (after CS-3 cutover):** forensic-only; those three redrive paths move to the reservation's typed outcome.
- **A4-2 (single certainty/fence authority).** `delivery_reservation` is the only certainty source. **CS-2 fence = the partial unique index `ux_reservation_active`** (fail-closed; §3). **CS-3** adds the durable token (`node_state.delivery_generation`) matched to the reservation and rebuilds the index predicate; certainty/fence are never read from `fiscal_documents.state` or `transport_trace`.
- **A4-3 (fiscal_documents = projection).** Outcome applied atomically via the existing `Sending→Sent + sfn` CAS as the downstream projection of `OutcomeObserved`; `(Sent,Rejected)` edge stays removed; post-SENT reject → `RequiresManualReconciliation`, seed never rolled back.
- **A4-4 (immutable protocol binding).** The reservation's typed binding columns are snapshot at creation, immutable, carried through every retry; a doc retries only on its bound `dps_protocol_id` — extends frozen invariant #3 to protocol.
- **A4-5 (cross-protocol forbidden by default).** `SubmittedUnknown` on A is never permission to act on B; reconciliation runs first on the original protocol. Lifted **only** on ALL of: an official DPS identity/correlation contract, proven cross-protocol consistency/visibility, a declared `ReconciliationCapability`, and cross-adapter conformance + negative tests. Until then **unknown ⇒ deny**. Does not block CS-2. **(audit: LOCK-READY.)**
- **A4-6 (record-then-apply discipline — CS-3 activation contract; corrected per audit).** Split, never a naive "recompute on CAS miss":
  1. the **outcome evidence** (the three fields + `remote_correlation_id`, as a self-contained `ObservedOutcomeV1`) is committed **as authority** first;
  2. if the projection guards pass → `document` state + `node_state` seed + audit + fence-release commit **atomically** (one `BEGIN IMMEDIATE`);
  3. if a projection guard fails → commit `OUTCOME_RECORDED_PENDING_APPLY`, **the fence stays**;
  4. **only the ledger apply is idempotently repeated — the DPS wire-call is NEVER repeated, and recorded evidence is never lost.**
  The full **coordinator/actor-mediated** version of this CAS is **CS-4**; CS-3 does the narrower record-then-apply with `ObservedOutcomeV1`.

## 3 · Migration 032 co-draft rev 3 (governed by §1–§2) — `delivery_reservation` ONLY
No `ingress_inbox` delta (Spec #3 first). No `generation` (CS-3 activation, with the token/apply). No
apply-states or payload (CS-3, `ObservedOutcomeV1`). New file; full 5-section header.

```sql
-- rust/prro/migrations/032_delivery_reservation.sql   (INACTIVE — CS-2 §2b)

-- Supporting index for the composite FK, created BEFORE the child table (audit rec 4).
-- ADDITIVE index on the EXISTING fiscal_documents (NOT a self-only object). NO "IF NOT EXISTS":
-- a pre-existing collision must fail loud (schema-drift detector).
CREATE UNIQUE INDEX ux_fd_docid_fn ON fiscal_documents(document_id, fiscal_number);

CREATE TABLE delivery_reservation (
    reservation_id        BLOB    PRIMARY KEY CHECK (length(reservation_id) = 16),  -- independent identity
    document_id           BLOB    NOT NULL CHECK (length(document_id) = 16),
    fiscal_number         TEXT    NOT NULL,
    attempt_no            INTEGER NOT NULL CHECK (attempt_no >= 1),  -- independent of transport_trace
    state                 TEXT    NOT NULL DEFAULT 'RESERVED_NOT_STARTED'
        CHECK (state IN ('RESERVED_NOT_STARTED','CALL_STARTED','OUTCOME_OBSERVED')),  -- apply-states = CS-3
    -- Spec #2 §3: NotSubmitted admissible ONLY before the wire marker. call_started_at is that durable
    -- marker (== stage_send.rs wire_call_started_at, set at RN→CALL_STARTED). Its CHECKs below make the
    -- CALL_STARTED→NOT_SUBMITTED bypass structurally impossible (audit B1 hole 1).
    call_started_at       TEXT,
    -- A4-4 typed binding (audit rec 4). FSCO_ZZD|EVPZ_DPS is the real discriminant
    -- (fn_outgress_profile.rs:23). Versions: contract_version >= 1 mandatory; the two below are
    -- nullable = "not yet profiled / no revision pinned" (default profile).
    dps_protocol_id            TEXT    NOT NULL CHECK (dps_protocol_id IN ('FSCO_ZZD','EVPZ_DPS')),
    protocol_contract_version  INTEGER NOT NULL CHECK (protocol_contract_version >= 1),
    capability_profile_version INTEGER,
    endpoint_config_revision   INTEGER,
    -- compute_envelope_hash hashes prost gen::Check (stage_send.rs:795) — protocol-specific; EVPZ
    -- needs its own canonical-envelope seam (Spec #4 part B).
    envelope_hash         BLOB    NOT NULL CHECK (length(envelope_hash) = 32),
    remote_correlation_id TEXT,                       -- NULL pre-outcome
    -- Spec #2 §2 THREE orthogonal outcome fields (recorded before the collapse, inline_map.rs:396).
    -- routing_class mirrors the EXISTING retry_class wire contract VERBATIM — PascalCase
    -- (error_routing.rs:120), NOT UPPER_SNAKE, NOT camelCase.
    submission_certainty  TEXT CHECK (submission_certainty IN ('NOT_SUBMITTED','SUBMITTED_UNKNOWN','SUBMITTED')),
    response_provenance   TEXT CHECK (response_provenance IN ('NO_RESPONSE','AUTHENTICATED_PEER','PARSED_DPS_ENVELOPE')),
    routing_class         TEXT CHECK (routing_class IN ('TerminalReject','TransientRetry','FnConfigError',
                            'WrapperBug','ProbeRequired','MacRecovery','OperatorEscalation','DrainChainSettleRetry')),
    created_at            TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),

    -- ── MINIMAL structural-consistency matrix (audit B2: NOT the full valid-combos table; the
    --    normative classifier table + typed constructor are CS-3) ──
    CHECK (state <> 'RESERVED_NOT_STARTED' OR (call_started_at IS NULL
        AND submission_certainty IS NULL AND response_provenance IS NULL AND routing_class IS NULL)),
    CHECK (state <> 'CALL_STARTED' OR (call_started_at IS NOT NULL
        AND submission_certainty IS NULL AND response_provenance IS NULL AND routing_class IS NULL)),
    CHECK (state <> 'OUTCOME_OBSERVED' OR (submission_certainty IS NOT NULL AND response_provenance IS NOT NULL)),
    -- NotSubmitted ⇒ never called + no response (kills CALL_STARTED→NOT_SUBMITTED, B1 hole 1)
    CHECK (submission_certainty <> 'NOT_SUBMITTED' OR (call_started_at IS NULL AND response_provenance = 'NO_RESPONSE')),
    -- a submit / unknown implies the call started
    CHECK (submission_certainty NOT IN ('SUBMITTED_UNKNOWN','SUBMITTED') OR call_started_at IS NOT NULL),
    -- Submitted (proven at DPS) ⇒ a parsed DPS envelope
    CHECK (submission_certainty <> 'SUBMITTED' OR response_provenance = 'PARSED_DPS_ENVELOPE'),
    -- routing_class NULL at OUTCOME_OBSERVED ⇔ clean accept ⇒ Submitted (converse allowed:
    -- SUBMITTED + routing != NULL = an observed reject/degraded — kept, and stays FENCED, B1 hole 2)
    CHECK (routing_class IS NOT NULL OR state <> 'OUTCOME_OBSERVED' OR submission_certainty = 'SUBMITTED'),
    -- response-derived reject cannot coexist with no-response (kills NOT_SUBMITTED+NO_RESPONSE+TerminalReject)
    CHECK (routing_class <> 'TerminalReject' OR response_provenance = 'PARSED_DPS_ENVELOPE'),

    FOREIGN KEY (document_id, fiscal_number)
        REFERENCES fiscal_documents(document_id, fiscal_number) ON DELETE RESTRICT,  -- composite only
    UNIQUE (document_id, attempt_no)
) STRICT;

-- FENCE (audit B1 rev 3): hold the FN while in-flight, or SubmittedUnknown, or an observed
-- reject/degraded (SUBMITTED + routing != NULL). Release ONLY on clean accept (SUBMITTED + routing
-- NULL) or safe pre-call cancel (NOT_SUBMITTED, which by CHECK means call_started_at IS NULL).
-- Conservative: never under-fences. CS-3 rebuilds this with the PENDING_APPLY/APPLIED predicate.
CREATE UNIQUE INDEX ux_reservation_active ON delivery_reservation(fiscal_number)
    WHERE state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
       OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED_UNKNOWN')
       OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED' AND routing_class IS NOT NULL);

CREATE INDEX ix_reservation_call_started ON delivery_reservation(fiscal_number) WHERE state = 'CALL_STARTED';

-- Structural FSM enforcement (audit B1): insert-state, outcome/binding immutability, no-delete-fenced.
CREATE TRIGGER delivery_reservation_insert_state
BEFORE INSERT ON delivery_reservation WHEN NEW.state <> 'RESERVED_NOT_STARTED'
BEGIN SELECT RAISE(ABORT, 'reservation must be inserted as RESERVED_NOT_STARTED'); END;

CREATE TRIGGER delivery_reservation_immutable
BEFORE UPDATE ON delivery_reservation
WHEN OLD.document_id <> NEW.document_id OR OLD.fiscal_number <> NEW.fiscal_number
  OR OLD.attempt_no <> NEW.attempt_no  OR OLD.dps_protocol_id <> NEW.dps_protocol_id
  OR OLD.envelope_hash <> NEW.envelope_hash
  OR (OLD.call_started_at IS NOT NULL AND OLD.call_started_at <> NEW.call_started_at)
  OR (OLD.submission_certainty IS NOT NULL AND OLD.submission_certainty <> NEW.submission_certainty)
  OR (OLD.response_provenance  IS NOT NULL AND OLD.response_provenance  <> NEW.response_provenance)
  OR (OLD.routing_class IS NOT NULL AND OLD.routing_class <> NEW.routing_class)
  OR (OLD.state = 'OUTCOME_OBSERVED' AND NEW.state <> 'OUTCOME_OBSERVED')   -- outcome is terminal in 032
BEGIN SELECT RAISE(ABORT, 'reservation binding/outcome/marker fields are immutable once set'); END;

CREATE TRIGGER delivery_reservation_no_delete_fenced
BEFORE DELETE ON delivery_reservation
WHEN OLD.state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
  OR (OLD.state = 'OUTCOME_OBSERVED' AND OLD.submission_certainty = 'SUBMITTED_UNKNOWN')
  OR (OLD.state = 'OUTCOME_OBSERVED' AND OLD.submission_certainty = 'SUBMITTED' AND OLD.routing_class IS NOT NULL)
BEGIN SELECT RAISE(ABORT, 'cannot delete a fenced/unresolved reservation'); END;

CREATE TRIGGER delivery_reservation_updated_at
AFTER UPDATE ON delivery_reservation
BEGIN UPDATE delivery_reservation SET updated_at = CURRENT_TIMESTAMP WHERE reservation_id = NEW.reservation_id; END;
```
**Normative (repo API, not all SQL-enforceable):** insert-only-as-`RESERVED_NOT_STARTED`; legal edges
`RN→CS`, `RN→OO(NotSubmitted only)`, `CS→OO(never NotSubmitted)`; outcome fields append-only;
`DrainChainSettleRetry` is **legacy-hydration/backfill only — the CS-3 typed constructor forbids fresh
emission** (mirrors error_routing's "no current routing emits it"). **Repo (INACTIVE):**
`outbox.rs`-style runtime `sqlx::query`, tx-only `insert` + pool `get_active_for_fn`; **NO caller**;
`attempt_no` = `MAX(attempt_no)+1` **inside one `with_immediate`** (`BEGIN IMMEDIATE`, `tx.rs:118`),
append-only, no number-reuse after delete.

## 4 · RED-pins (CS-3 activation contract — known-red until CS-3)
- **RP-A4-1 (fence source):** seed-advance reads the fence token from `node_state.delivery_generation` vs `reservation`, not `fiscal_documents`/`transport_trace`.
- **RP-A4-2 (forensic cutover):** after CS-3, no resend/seed/fence path reads `transport_trace`; the three `f2628ba` consumer-paths are gone.
- **RP-A4-3a (SubmittedUnknown fences):** a 2nd reservation on an FN with `OUTCOME_OBSERVED + SUBMITTED_UNKNOWN` is refused (Spec #2 RP-2).
- **RP-A4-3b (observed-reject fences — B1 hole 2):** a 2nd reservation on an FN with `OUTCOME_OBSERVED + SUBMITTED + routing_class NOT NULL` is refused; a clean accept (`SUBMITTED + routing NULL`) releases.
- **RP-A4-3c (no post-call NotSubmitted — B1 hole 1):** a row with `call_started_at NOT NULL` (or `state=CALL_STARTED`) cannot become `NOT_SUBMITTED` (CHECK-rejected).
- **RP-A4-4 (bound protocol):** a doc whose `fn_outgress_profile` flips mid-shift still retries on its bound `dps_protocol_id`.
- **RP-A4-5 (record-then-apply):** a crash after `OutcomeObserved` but before projection re-applies the recorded `ObservedOutcomeV1` idempotently; the DPS call is never repeated (A4-6).
- **RP-A4-6 (no blind resend):** `er_redrive` does not blind-resend a possibly-submitted doc on Transport-timeout (Spec #2 RP-1) — the double-issue keystone.

## 5 · Deferred to CS-3 (a single activation migration; per operator + audit)
- `generation` + `node_state.delivery_generation` + `active_delivery_reservation_id` (fence token/pointer) + apply-states `OUTCOME_RECORDED_PENDING_APPLY`/`APPLIED` + the self-contained **`ObservedOutcomeV1`** payload (NOT `TransitionPlan` — that + the actor are **CS-4**; roadmap:44 puts CS-3 before CS-4). The activation migration **fail-fast requires an empty `delivery_reservation`**, rebuilds the CHECK/index, and completes **before** any caller is enabled.
- `ingress_inbox.idempotency_strategy` — Spec #3 first, separate additive migration.
- Fleet command lifecycle — Spec #5 = read-only telemetry projection for pilot.
- `ABORTED`/EPZ CHECK history — untouched (025/030/031 are the authority).

## 6 · INACTIVE merge pins (audit — boot DOES apply the migration; not an absolute "zero change")
Fiscal/write-path behaviour-neutral (empty table, self-scoped indexes/triggers, no callers) — but boot
applies 032 (schema + `_sqlx_migrations`). Merge gate:
1. **upgrade 031→032 on a non-empty representative DB** (not only a fresh pool);
2. **`sqlite_master` diff** — pre-existing objects byte-identical; only the expected new objects added (incl. the additive `ux_fd_docid_fn` **on `fiscal_documents`** — not self-only);
3. **production-flow test** — after a normal fiscalisation, `delivery_reservation` stays **empty**;
4. **static call-graph pin** — the repo is not referenced outside the migration test;
5. **constraint / phase-laundering matrix** — the full 3-field truth-table; a 2nd reservation after **every** unsafe state (esp. `OUTCOME_OBSERVED+SUBMITTED_UNKNOWN` and `OUTCOME_OBSERVED+SUBMITTED+routing!=NULL` both REJECTED); `NOT_SUBMITTED` with `call_started_at NOT NULL` rejected; outcome/binding immutability + no-delete-fenced (trigger `RAISE`); composite-FK mismatch (doc FN-A under fence FN-B) rejected; concurrent `attempt_no` (1,2) with no reuse; the CS-3 activation-empty / pre-feature gate.

## 7 · Residual questions for re-audit (most resolved)
1. **`attempt_no` concurrency:** `MAX+1` under a single `BEGIN IMMEDIATE` (`tx.rs:118`) — confirm no global allocator needed and no reuse-after-delete (the immutability + no-delete-fenced triggers already forbid deleting an unresolved row).
2. **Trigger set completeness:** are insert-state + immutability + no-delete-fenced the right structural floor for the INACTIVE baseline, or is a full transition-legality trigger (`RN→CS→OO` forward-only, block `RN→OO` with a set `call_started_at`) wanted now vs deferred to the CS-3 rebuild?
3. **Composite FK:** confirmed best; any remaining concern with dropping the redundant single-column FK now that `(document_id, fiscal_number)` RESTRICT covers the delete-guard?
