# Spec #4 (part A) — Authority Mini-Lock + Migration 032 Co-Draft

**Status: 🟡 DRAFT rev 2 (post external audit round 1 → NOT-YET). 2026-07-15. Grounded on `origin/main` `f2628ba`.**
Rev 2 closes the audit's 5 blockers + MAJOR: (B1) the fence-index released the FN in the
`SubmittedUnknown` window; (B2) the DDL didn't encode Spec #2's three fields; (B3) `PendingApply` had
no replayable payload; (B5) the map was really 4-sided (seed lives in `node_state`); (MAJOR) A4-1 is
violated on `f2628ba` today. **Anchors:** Spec #1 + Spec #2 (rev 2) DESIGN-LOCKED. **The
schema-implementer does NOT run until this map re-audits to LOCK-READY.**

---

## 0 · Why a mini-lock before the DDL (the gate)
`sqlx`'s native checksum (`db/mod.rs:37`) freezes 032's column-set + CHECK literals once it applies.
Four durable structures touch the *Sending / CallStarted* window; fixing **which owns what** before
minting prevents the frozen file from cementing an unresolved contract.

## 1 · The FOUR durable structures + their authority (NORMATIVE — corrected per audit B5)
| structure | **owns** | **NEVER** |
|---|---|---|
| `delivery_reservation` (NEW 032, **INACTIVE**) | call lifecycle `ReservedNotStarted→CallStarted→OutcomeObserved`; **delivery certainty** (3 orthogonal fields, Spec #2 §2); the **fence** *snapshot* (`generation`) | — |
| `node_state` | the **chain-tip seed** (`last_known_unsigned_xml_sha256`, `stage_send.rs:1748`); in CS-3 the **current fence token/pointer** (`delivery_generation` + `active_delivery_reservation_id`) | (CS-2: no new columns — see §5) |
| `fiscal_documents` | the **fiscal doc-FSM** (14 `DocState`) + the atomically-applied **projection** of the outcome (`Sending→Sent + sfn` CAS, `stage_send.rs:1704`) | is not the certainty/fence source; `Sending` is a doc marker, not the reservation |
| `transport_trace` | **forensic / observability ONLY** (`transport_trace.rs:6`) | is **never** a basis for resend / seed-advance / fence |

## 2 · Invariants (mini-lock — normative)
- **A4-1 (forensic-only — with honest baseline).** **CURRENT (`f2628ba`):** `transport_trace` is the *legacy authority* for ER-redrive — `last_attempt_retry_class_for` (`transport_trace.rs:320`) picks the retry class, `attempts_used` (`:390`) sets the retry budget, `er_redrive_policy.rs:80` returns `Redrive`, called from `boot_phase.rs:3066` / `backlog_drain.rs:1537` / `online_convergence.rs:559`. Seed advance already does **not** read trace (it reads/writes `node_state.last_known_unsigned_xml_sha256`, `stage_send.rs:1748`). **TARGET (after the CS-3 atomic cutover):** `transport_trace` is forensic-only; those three redrive consumer-paths move to the reservation's typed outcome. This invariant is a **CS-3 target**, not a `f2628ba` fact.
- **A4-2 (single certainty/fence authority).** `delivery_reservation` is the only source for delivery certainty and the fence. In **CS-2** the schema-level fence = the partial unique index `ux_reservation_active` (holds the FN while in-flight **or** `SubmittedUnknown`). In **CS-3** the seed-advance gate reads the current fence token from `node_state.delivery_generation` (added then) and matches it against `reservation.generation`; it never reads `fiscal_documents.state` or `transport_trace` for certainty/fence.
- **A4-3 (fiscal_documents = projection).** The outcome is applied atomically via the existing `Sending→Sent + sfn` CAS as the downstream projection of `OutcomeObserved`; the `(Sent,Rejected)` edge stays removed; post-SENT reject → `RequiresManualReconciliation`, seed never rolled back.
- **A4-4 (immutable protocol binding).** The reservation's typed binding columns are snapshot at creation and carried through every retry; a doc retries **only** on its bound protocol — extends frozen invariant #3 to **protocol**.
- **A4-5 (cross-protocol forbidden by default).** `SubmittedUnknown` on protocol A is never permission to act on protocol B; reconciliation runs first on the original protocol. The forbid is lifted **only** on ALL of: an official DPS identity/correlation contract, proven cross-protocol consistency/visibility, a declared `ReconciliationCapability`, and cross-adapter conformance + negative tests. Until then **unknown ⇒ deny** (not an assumption). Absence of proof does not block CS-2 (binding stored; enforcement CS-3).
- **A4-6 (atomic cross-table CAS — the CS-3 activation contract, B5).** Activation performs, in **one** `BEGIN IMMEDIATE`: (1) CAS reservation by `(reservation_id, generation)`; (2) CAS document by expected `(state, version)`; (3) seed update in `node_state` by expected previous seed + generation; (4) fence release; (5) audit/trace. No partial cross-table apply; on any precondition miss the actor recomputes. **CS-2 lands the columns inert; this CAS is CS-3.**

## 3 · Migration 032 co-draft rev 2 (governed by §1–§2) — `delivery_reservation` ONLY
`delivery_reservation`-only (no `ingress_inbox` delta — Spec #3 first). New file; full 5-section
header. **Apply-states + durable outcome payload are DEFERRED to CS-3** (they need the `TransitionPlan`
type from CS-4; freezing an under-specified payload now would repeat audit B3). CS-2 carries the **3
live lifecycle states** + the **3 Spec-#2 outcome fields** + **typed binding** + a **hard doc FK**.

```sql
-- rust/prro/migrations/032_delivery_reservation.sql   (INACTIVE — CS-2 §2b)
CREATE TABLE delivery_reservation (
    reservation_id        BLOB    PRIMARY KEY CHECK (length(reservation_id) = 16),  -- independent identity (audit rec 1)
    -- Hard FK (audit rec 3): the doc is minted PREPARED at stage_acquire (stage_acquire.rs:858)
    -- BEFORE sign/send, so a reservation is never grounded before its doc. RESTRICT (not the
    -- transport_trace CASCADE 001:605): deleting a doc must NOT silently drop an unresolved fence.
    document_id           BLOB    NOT NULL CHECK (length(document_id) = 16),
    fiscal_number         TEXT    NOT NULL,
    attempt_no            INTEGER NOT NULL CHECK (attempt_no >= 1),  -- independent of transport_trace.attempt_no
    -- Spec #2 §3 lifecycle — 3 LIVE states only. Apply-states (OUTCOME_RECORDED_PENDING_APPLY,
    -- APPLIED) + their durable payload are CS-3 (audit B3). UPPER_SNAKE like the other FSMs.
    state                 TEXT    NOT NULL DEFAULT 'RESERVED_NOT_STARTED'
        CHECK (state IN ('RESERVED_NOT_STARTED','CALL_STARTED','OUTCOME_OBSERVED')),
    -- A4-4 typed binding (audit rec 4) — NOT an opaque composite. FSCO_ZZD|EVPZ_DPS is the real
    -- domain discriminant (fn_outgress_profile.rs:23); (backend,transport)-profile-ids are NOT it.
    dps_protocol_id           TEXT    NOT NULL CHECK (dps_protocol_id IN ('FSCO_ZZD','EVPZ_DPS')),
    protocol_contract_version INTEGER NOT NULL,
    capability_profile_version INTEGER,
    endpoint_config_revision  INTEGER,
    -- Protocol-specific canonical envelope: compute_envelope_hash hashes prost gen::Check
    -- (stage_send.rs:795), so EVPZ needs its own canonical-envelope seam (Spec #4 part B).
    envelope_hash         BLOB    NOT NULL CHECK (length(envelope_hash) = 32),
    remote_correlation_id TEXT,                       -- CheckAck.id / transport_request_id; NULL pre-outcome
    generation            INTEGER NOT NULL DEFAULT 0 CHECK (generation >= 0),  -- fence SNAPSHOT; token/CAS = CS-3
    -- Spec #2 §2 THREE orthogonal outcome fields (audit B2). Recorded BEFORE the collapse
    -- (inline_map.rs:396). NULL pre-outcome. routing_class mirrors the EXISTING retry_class wire
    -- contract VERBATIM (camelCase, error_routing.rs:120-127) — NOT UPPER_SNAKE.
    submission_certainty  TEXT CHECK (submission_certainty IN ('NOT_SUBMITTED','SUBMITTED_UNKNOWN','SUBMITTED')),
    response_provenance   TEXT CHECK (response_provenance IN ('NO_RESPONSE','AUTHENTICATED_PEER','PARSED_DPS_ENVELOPE')),
    routing_class         TEXT CHECK (routing_class IN ('TerminalReject','TransientRetry','FnConfigError',
                            'WrapperBug','ProbeRequired','MacRecovery','OperatorEscalation','DrainChainSettleRetry')),
    created_at            TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    -- Self-consistency (audit B2): pre-outcome ⇒ all 3 fields NULL; OUTCOME_OBSERVED ⇒ certainty+
    -- provenance required; Spec #2 cross-field: NotSubmitted⇒NoResponse, Submitted⇒ParsedDpsEnvelope;
    -- routing_class NULL ⇔ clean accept ⇒ Submitted.
    CHECK (
        (state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
            AND submission_certainty IS NULL AND response_provenance IS NULL AND routing_class IS NULL)
        OR (state = 'OUTCOME_OBSERVED'
            AND submission_certainty IS NOT NULL AND response_provenance IS NOT NULL
            AND (submission_certainty <> 'NOT_SUBMITTED' OR response_provenance = 'NO_RESPONSE')
            AND (submission_certainty <> 'SUBMITTED'     OR response_provenance = 'PARSED_DPS_ENVELOPE')
            AND (routing_class IS NOT NULL OR submission_certainty = 'SUBMITTED'))
    ),
    -- audit rec 3: reservation's fiscal_number must equal the doc's (composite FK; needs the
    -- supporting unique index below — an ADDITIVE index on fiscal_documents, not an 001-031 edit).
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT,
    FOREIGN KEY (document_id, fiscal_number)
        REFERENCES fiscal_documents(document_id, fiscal_number) ON DELETE RESTRICT,
    UNIQUE (document_id, attempt_no)
) STRICT;

-- Supporting index for the composite FK above (additive; enables the doc↔fiscal_number guarantee).
CREATE UNIQUE INDEX IF NOT EXISTS ux_fd_docid_fn ON fiscal_documents(document_id, fiscal_number);

-- FENCE (audit B1 fix): hold the FN while IN-FLIGHT or FENCED (SubmittedUnknown). Releases ONLY on
-- clean accept (OUTCOME_OBSERVED + SUBMITTED) or safe-cancel (OUTCOME_OBSERVED + NOT_SUBMITTED).
-- Conservative: never under-fences (the seed-fork direction). CS-3 may relax via a later migration.
CREATE UNIQUE INDEX ux_reservation_active ON delivery_reservation(fiscal_number)
    WHERE state IN ('RESERVED_NOT_STARTED','CALL_STARTED')
       OR (state = 'OUTCOME_OBSERVED' AND submission_certainty = 'SUBMITTED_UNKNOWN');

CREATE INDEX ix_reservation_call_started ON delivery_reservation(fiscal_number) WHERE state = 'CALL_STARTED';

CREATE TRIGGER delivery_reservation_updated_at
AFTER UPDATE ON delivery_reservation
BEGIN
    UPDATE delivery_reservation SET updated_at = CURRENT_TIMESTAMP WHERE reservation_id = NEW.reservation_id;
END;
```
**Repo (INACTIVE):** `pub mod delivery_reservation;`; model on `outbox.rs` (runtime `sqlx::query`, not
`query!`); tx-only `insert` on `&mut WriteTxConn` + pool `get_active_for_fn`; **wire NO caller**.

## 4 · RED-pins (CS-3 activation contract — known-red until CS-3)
- **RP-A4-1 (fence source):** seed-advance reads the fence token from `node_state.delivery_generation` matched to `reservation.generation`, not `fiscal_documents`/`transport_trace` — revert → RED.
- **RP-A4-2 (forensic-only cutover):** after CS-3, no resend/seed/fence path reads `transport_trace` (a redrive gated on trace → FAIL); the three `f2628ba` consumer-paths (A4-1) are gone.
- **RP-A4-3 (SubmittedUnknown fences):** while a reservation is `OUTCOME_OBSERVED + SUBMITTED_UNKNOWN`, a new issuance / offline-session / seed-advance on that FN is refused (Spec #2 RP-2) — the exact hole the fence-index now closes.
- **RP-A4-4 (bound protocol):** a doc whose `fn_outgress_profile` flips mid-shift still retries on its bound `dps_protocol_id` (proves A4-4).
- **RP-A4-5 (atomic cross-table CAS):** a crash mid-apply re-derives deterministically from the reservation payload; no partial cross-table effect (A4-6).
- **RP-A4-6 (no blind resend):** `er_redrive` does not blind-resend a possibly-submitted doc on Transport-timeout (Spec #2 RP-1) — the double-issue keystone; stays RED until CS-3.

## 5 · Deferred to CS-3 (explicitly OUT of CS-2 — per operator + audit B3)
- Apply-states `OUTCOME_RECORDED_PENDING_APPLY` / `APPLIED` **and** the versioned durable outcome payload / `TransitionPlan` needed to boot-idempotently replay (needs CS-4's `TransitionPlan` type).
- `node_state.delivery_generation` + `active_delivery_reservation_id` (the fence **token/pointer**) + the atomic cross-table CAS (A4-6). CS-2's fence is the reservation index only.
- `ingress_inbox.idempotency_strategy` — Spec #3 first, then a separate additive migration.
- Fleet command lifecycle — Spec #5 is read-only telemetry projection for pilot.
- `ABORTED`/EPZ CHECK history — untouched (025/030/031 are the subsequent authority).

## 6 · INACTIVE pins (audit — not an absolute "zero behaviour change"; boot DOES apply the migration)
The slice is fiscal/write-path behaviour-neutral (empty table, self-only indexes/trigger, no callers)
— but boot applies 032 (schema + `_sqlx_migrations` change, DDL/disk-fail surface). Merge gate adds:
1. **upgrade** 031→032 on a **non-empty representative DB** (not only a fresh pool);
2. **`sqlite_master` diff** — pre-existing objects byte-identical, only the expected new objects added;
3. **production-flow test** — after a normal fiscalisation, `delivery_reservation` stays **empty**;
4. **static call-graph pin** — `delivery_reservation` repo is not referenced outside the migration test;
5. **constraint matrix** — incl. a **second reservation after every unsafe state** (esp. `OUTCOME_OBSERVED + SUBMITTED_UNKNOWN` must be REJECTED by `ux_reservation_active`).

## 7 · Open questions for re-audit
1. **Composite FK cost:** the doc↔fiscal_number guarantee needs `ux_fd_docid_fn` on `fiscal_documents` (additive index in 032). Acceptable, or keep the single-column FK + a trigger, or drop `fiscal_number` from the reservation and derive it by join (loses the direct `ux_reservation_active(fiscal_number)` index)?
2. **`attempt_no` allocation:** independent per-`document_id` ordinal allocated from this table (`MAX(attempt_no)+1` under the write lease) — confirm this needs no global allocator and cannot race under the FN single-writer.
3. **Deferral acceptance (the two operator-aligned defers):** does the audit accept deferring (a) the apply-states + payload and (b) the `node_state` fence token/CAS to CS-3, given CS-2 is INACTIVE and the reservation index is a conservative fail-closed fence? Or must the fence token land now to avoid a second frozen migration later?
4. **`generation` in CS-2:** stored snapshot only (enforcement CS-3) — is a stored-but-unenforced `generation` acceptable, or should 032 omit it until the token/CAS lands (avoid a "decorative" column)?
