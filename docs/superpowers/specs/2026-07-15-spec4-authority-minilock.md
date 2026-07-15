# Spec #4 (part A) — Authority Mini-Lock + Migration 032 Co-Draft

**Status: 🟡 DRAFT for external audit. 2026-07-15. Grounded on `origin/main` `f2628ba`.**
Scope of THIS doc = the **authority-distribution mini-lock** the operator gated CS-2 on, plus the
**co-drafted migration 032** it governs. The full DPS-contract / typed-delivery / binding surface is
authored later as Spec #4 (part B); this part exists so the **checksum-frozen** migration cements
*decided* semantics, not open questions. **Anchors:** Spec #1 (transition contract) + Spec #2
(3-field delivery + reservation FSM) are DESIGN-LOCKED and are the authority for the FSM this table
backs.

---

## 0 · Why a mini-lock before the DDL (the gate)
`sqlx`'s native migration checksum (`_sqlx_migrations`, `db/mod.rs:37`) **refuses any altered applied
file** — once 032 merges and runs, its column set + CHECK literals are frozen. Three durable
structures already touch the *Sending / CallStarted* window, so before minting the table we must fix
**which structure owns what**, or the frozen file will encode an unresolved contract. **The
schema-implementer does NOT run until this map is audit-locked.**

## 1 · The three durable structures + their authority (NORMATIVE)
| structure | **owns (source of truth for)** | **NEVER** |
|---|---|---|
| `delivery_reservation` (NEW — migration 032, **INACTIVE in CS-2**) | (a) the **call lifecycle** `ReservedNotStarted → CallStarted → OutcomeObserved` (Spec #2 §3); (b) **delivery certainty** — the three orthogonal fields (Spec #2 §2); (c) the per-FN **chain-generation FENCE** (Spec #2 §5) | — |
| `fiscal_documents` | the **fiscal doc-FSM** (14 `DocState`) + the **atomically-applied projection** of the delivery result — the existing `Sending → Sent` + `server_fiscal_no` + seed-advance CAS (`stage_send.rs:1704/1725`) | is **not** the call-lifecycle / certainty / fence source; `DocState::Sending` is a **doc-FSM marker**, not the reservation state |
| `transport_trace` | **forensic / observability ONLY** (its declared role, `transport_trace.rs:6`) | is **NEVER** a basis for a resend decision, a seed-advance, or setting/lifting the fence |

## 2 · Invariants (the mini-lock — normative for CS-3 activation)
- **A4-1 (forensic-only).** No code path may read `transport_trace` to decide **resend**, **seed-advance**, or **fence** state. Re-affirms `transport_trace.rs:6`.
- **A4-2 (single certainty/fence source).** `delivery_reservation` is the **only** source for delivery certainty and the generation fence. The CS-3 seed-advance gate reads `generation` / `state` from `delivery_reservation` — never from `fiscal_documents.state` nor `transport_trace`.
- **A4-3 (fiscal_documents = projection).** `fiscal_documents` applies the outcome **atomically** (the existing `Sending→Sent + sfn + seed` CAS) as the **downstream projection** of the reservation's `OutcomeObserved` — not a parallel authority. The `(Sent, Rejected)` edge stays removed (A.3 PR-B); a post-SENT reject escalates to `RequiresManualReconciliation`, never rolls back the seed.
- **A4-4 (immutable protocol binding).** `delivery_reservation.protocol_binding` is snapshot at reservation creation and carried through **every** retry; a document retries **only** on its bound protocol. This **extends frozen invariant #3** (no channel switch with an open shift) explicitly to **protocol**.
- **A4-5 (cross-protocol forbidden by default).** A `SubmittedUnknown` on protocol A is **never** permission to act on protocol B; reconciliation runs first, on the **original** protocol. The cross-protocol path is lifted **only** if an official DPS source proves FSCO_ZZD and EVPZ share **one authoritative fiscal registration/ledger**. **Absence of that proof does NOT block CS-2** — the binding is stored; enforcement is CS-3.
- **A4-6 (INACTIVE in CS-2).** CS-2 lands the table + columns + guards-as-schema **inert**: no writer wired, no enforcement. Activation — the seed-advance gate on the fence, typed-delivery recording (record-before-collapse, Spec #2 §8), and killing the blind-resend (`er_redrive`) — is **CS-3**.

## 3 · Migration 032 co-draft (governed by §1–§2) — `delivery_reservation` ONLY
Per the operator: **no `ingress_inbox` delta here** — Spec #3 freezes the `IdempotencyStrategy`
literals + no-key policy first, then a *separate additive* migration. 032 mints only the reservation
table. New file only; do **not** edit 001–031. Full 5-section header (WHY / STRICT TABLE NOTE /
BACKWARD COMPATIBILITY / ROLLBACK REASONING / LIVE FILE SEQUENCE) per the 028–031 convention.

```sql
-- rust/prro/migrations/032_delivery_reservation.sql   (INACTIVE — CS-2 §2b)
CREATE TABLE delivery_reservation (
    -- soft-ref BLOBs, NO hard FK to fiscal_documents: a reservation may precede
    -- the doc mint, so a hard FK under foreign_keys=ON would deadlock ordering.
    reservation_id        BLOB    PRIMARY KEY CHECK (length(reservation_id) = 16),
    fiscal_number         TEXT    NOT NULL
        REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    document_id           BLOB    CHECK (document_id IS NULL OR length(document_id) = 16),
    attempt_id            INTEGER NOT NULL CHECK (attempt_id >= 1),
    -- Spec #2 §3 FSM. First 3 states are the live lifecycle; the last two are
    -- reserved for CS-3 apply-idempotency (Spec #2 §4 OutcomeRecordedPendingApply).
    -- UPPER_SNAKE, matching shifts / fiscal_documents / offline_sessions.
    state                 TEXT    NOT NULL DEFAULT 'RESERVED_NOT_STARTED'
        CHECK (state IN ('RESERVED_NOT_STARTED','CALL_STARTED','OUTCOME_OBSERVED',
                         'OUTCOME_RECORDED_PENDING_APPLY','APPLIED')),
    -- A4-4 immutable protocol binding (composite of backend/transport profile ids).
    protocol_binding      TEXT    NOT NULL,
    -- reuse compute_envelope_hash (stage_send.rs:1440); do NOT re-derive.
    envelope_hash         BLOB    NOT NULL CHECK (length(envelope_hash) = 32),
    remote_correlation_id TEXT,   -- generalises CheckAck.id / transport_request_id; NULL pre-outcome
    -- A4-2 the fence lives here (Spec #2 §5). INACTIVE: stored, not enforced in CS-2.
    generation            INTEGER NOT NULL DEFAULT 0 CHECK (generation >= 0),
    -- Spec #2 §2/§8 three orthogonal outcome fields — recorded BEFORE the collapse
    -- (inline_map.rs:396) so certainty/provenance are not lost. NULL pre-outcome.
    submission_certainty  TEXT    CHECK (submission_certainty IS NULL OR
        submission_certainty IN ('NOT_SUBMITTED','SUBMITTED_UNKNOWN','RESPONSE_OBSERVED')),
    response_provenance   TEXT,   -- INACTIVE free-form until Spec #4 part B types land
    created_at            TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at            TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    -- Self-consistency (mirrors transport_trace 001:637-655): pre-outcome states
    -- carry no certainty; OUTCOME_OBSERVED+ must.
    CHECK (
        (state IN ('RESERVED_NOT_STARTED','CALL_STARTED') AND submission_certainty IS NULL)
        OR (state IN ('OUTCOME_OBSERVED','OUTCOME_RECORDED_PENDING_APPLY','APPLIED')
            AND submission_certainty IS NOT NULL)
    ),
    UNIQUE (document_id, attempt_id)
) STRICT;

-- One active reservation per FN — single-writer at the schema level (mirrors
-- ux_shifts_one_open_per_fn 026 / ux_offline_active 001:429). INERT while
-- inactive; BITES the first CS-3 writer (documented as a designed guard).
CREATE UNIQUE INDEX ux_reservation_active ON delivery_reservation(fiscal_number)
    WHERE state IN ('RESERVED_NOT_STARTED','CALL_STARTED');

-- Crash-window scan target (mirrors ix_transport_trace_unfinished 001:663).
CREATE INDEX ix_reservation_call_started ON delivery_reservation(fiscal_number)
    WHERE state = 'CALL_STARTED';

CREATE TRIGGER delivery_reservation_updated_at
AFTER UPDATE ON delivery_reservation
BEGIN
    UPDATE delivery_reservation SET updated_at = CURRENT_TIMESTAMP
    WHERE reservation_id = NEW.reservation_id;
END;
```
**Repo (INACTIVE):** `pub mod delivery_reservation;` in `src/db/repositories/mod.rs`; model on
`outbox.rs` (runtime-bound `sqlx::query`, **not** `query!` — avoids `.sqlx` dual-cache churn);
tx-only `insert` on `&mut WriteTxConn` + pool-bound `get_active_for_fn`; **wire NO caller**.
**Test:** `tests/migration_032_delivery_reservation.rs`, `tempfile` fresh pool
(`migration_011_outbox.rs:25-31`), 8-fixture contract: table+index existence (`sqlite_master`),
`PRAGMA table_info` column-set, INSERT round-trip + defaults, PK-dup rejected, `state` CHECK rejects
unknown, `generation` CHECK rejects negative, self-consistency CHECK rejects
`OUTCOME_OBSERVED`-with-NULL-certainty, `ux_reservation_active` rejects a 2nd active row per FN,
`get_active_for_fn` on missing → `None`.

## 4 · RED-pins — the CS-3 activation contract (authored now, **known-red until CS-3**)
- **RP-A4-1 (fence source):** the seed-advance gate reads the fence from `delivery_reservation.generation`, not `fiscal_documents` — revert the source → RED.
- **RP-A4-2 (forensic-only):** any resend / seed-advance / fence decision that reads `transport_trace` (e.g. gated on `transport_trace.completed_at IS NULL`) → FAIL.
- **RP-A4-3 (bound protocol):** a doc whose `fn_outgress_profile` flips mid-shift still retries on its **bound** protocol (proves A4-4).
- **RP-A4-4 (no blind resend):** `er_redrive` does not blind-resend a possibly-submitted doc on a Transport-timeout (Spec #2 RP-1) — stays RED until the typed outcome lands in CS-3.

## 5 · Deferred per operator (explicitly OUT of CS-2)
- `ingress_inbox.idempotency_strategy` column — **not** added now (Spec #3 first, then a separate additive migration).
- Fleet command lifecycle (epoch / signed / PULL / command-inbox) — Spec #5 is **read-only telemetry projection only** for the pilot.
- `ABORTED` / EPZ-doctype CHECK history — untouched; migrations 025/030/031 are the subsequent authority.

## 6 · Open questions REMAINING for the audit (operator resolved the rest)
1. **`attempt_id` identity:** strictly `== transport_trace.attempt_no` (reuse the `allocate_and_insert_tx` allocator, single-sourced numbering) or an independent id? Reuse avoids a second allocator but couples the reservation's lifetime to `transport_trace`'s per-doc PK. Decide before CS-3 populates it.
2. **Fence storage co-location:** does the per-FN generation fence live **only** on `delivery_reservation.generation`, or **also** as an INACTIVE column on `node_state` (next to `last_known_unsigned_xml_sha256`, `001:543`) so the seed-advance gate has the fence co-located with the seed it guards? A4-2 names `delivery_reservation` as authority either way; this is a storage-placement call.
3. **Soft-ref vs hard FK on `document_id`:** the sketch uses a soft-ref BLOB because a reservation *may* precede the doc mint. **Confirm** the reservation can genuinely precede `fiscal_documents` — if it is *always* minted after, a hard FK `ON DELETE CASCADE` (like `transport_trace 001:605`) gives referential integrity for free.
4. **`protocol_binding` shape:** the co-draft stores a composite TEXT `(backend_profile_id, transport_profile_id)`. Spec #4 part B may want the richer `{protocol_id, protocol_version, capability_profile_version, endpoint_config_revision}` (plan:134-137). Since 032 is checksum-frozen, decide **now** whether the column is a single opaque TEXT (future-proof, parse in code) or a fixed set of typed columns.
5. **Cross-protocol registration (external DPS fact — A4-5):** does the audit have authority to state whether FSCO_ZZD and EVPZ observe one authoritative registration? If unknown, A4-5's default-forbid stands and the question carries to the EVPZ adapter sprint (CS-6).
