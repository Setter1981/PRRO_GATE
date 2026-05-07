# M3 W0-1 findings — state + sequence

> **Status update 2026-05-07:** ADR-M3-A1..A9 were approved and committed in `8c72a14` (`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md`).  Any `PROPOSED — NOT COMMITTED` wording below is historical research-time wording; canonical ADR status is the committed ADR block.

**Status:** research findings, not yet ratified.  Closes nothing — bd
issues PRRO_GATE-ddn and PRRO_GATE-zti remain open until M3a
implementation lands the chosen design in code.

**Inputs:**

- `docs/M2-handoff.md`
- `docs/superpowers/plans/2026-05-06-m3-w0-research.md`
- `CLAUDE.md`
- `docs/Multi-Protocol_PRRO_Gateway.md`
- `rust/prro/src/db/models/enums.rs`
- `rust/prro/migrations/001_core_identities.sql`
- `rust/prro/migrations/002_fiscal_documents.sql`
- `rust/prro/migrations/004_offline_and_routing.sql`
- `rust/prro/src/db/repositories/fiscal_documents.rs`
- `rust/prro/proto/fiscal_server.proto`
- `src/prro_gateway/services/write_path.py`
- `src/prro_gateway/services/reconciliation.py`
- `src/prro_gateway/services/offline_sync.py`
- `src/prro_gateway/repositories/node_state.py`
- `src/prro_gateway/repositories/fiscal_documents.py`
- `src/prro_gateway/adapters/webcheck_xmlrpc.py`
- `src/prro_gateway/serializers/dps_xml.py`
- `docs/webcheck_reverse/WEBCHECK_ANALYSIS.md`
- `docs/webcheck_reverse/WebCheckMain/WebCheck/CreateDB.cs`
- `docs/webcheck_reverse/WebCheckMain/WebCheck/StringXML.cs`

**Out of scope:** lock-discipline contracts (W0-2), retry/recovery
(W0-3), M3a implementation, ADR commits.

---

## 1. lnd source-of-truth

### 1.1 What `lnd` is — definition + citation

`lnd` is the Local Numerator of Document — a strictly monotonic
per-fiscal-number integer that drives the fiscal chain recovery key.
The Rust schema declares it `INTEGER NOT NULL CHECK (lnd >= 1)` on
`fiscal_documents` (`rust/prro/migrations/002_fiscal_documents.sql:9`)
and indexes it as `ix_fd_fn_lnd(fiscal_number, lnd)`
(`rust/prro/migrations/002_fiscal_documents.sql:50`).

The `node_state` table declares the per-FN monotonic counter
`next_lnd INTEGER NOT NULL CHECK (next_lnd >= 1)`
(`rust/prro/migrations/001_core_identities.sql:64`). This is the
allocation surface that `fiscal_documents.lnd` is supposed to be
fed from.

The pending-set ordering of `list_pending_for_fn` is
`ORDER BY lnd, created_at, document_id`
(`rust/prro/src/db/repositories/fiscal_documents.rs:204`), and the
inline doc-comment names `lnd` "the authoritative chain-recovery key;
the other two are tiebreakers" (`fiscal_documents.rs:191`).

### 1.2 How Python currently allocates `lnd` — citation

Python reference implementation: `NodeStateRepository.increment_lnd`
in `src/prro_gateway/repositories/node_state.py:21-46`:

```sql
UPDATE node_state
SET next_lnd = next_lnd + 1,
    updated_at = CURRENT_TIMESTAMP
WHERE fiscal_number = ?
RETURNING next_lnd - 1 AS allocated_lnd
```

Call site: `src/prro_gateway/services/write_path.py:414` —

```python
lnd = NodeStateRepository.increment_lnd(conn, fiscal_number=ctx.fiscal_number)
```

This call lives **inside** the `BEGIN IMMEDIATE` opened at
`write_path.py:195` by `_stage_acquire_and_validate`, and is committed
at `write_path.py:461` after `FiscalDocumentRepository.create_prepared`.
So the Python contract today is: lnd allocation + document insert are
in **the same** write transaction, under the inbox lease.

Python doc-comment at `node_state.py:23-32` is explicit on the
operational contract:

- "LND is allocated BEFORE the sign/send attempt."
- "If the subsequent send fails and the document is retried, the
  SAME LND is reused."
- "If the process crashes AFTER document creation but BEFORE retry
  resolution, the allocated LND may become a gap in the sequence."
- "DPS specification permits gaps in LND sequences ... fiscal
  numbering continuity is enforced at the Z-report / shift level,
  not per-document. Gaps are therefore expected, legal, and do NOT
  require correction."

Sign-retry resume at `write_path.py:351-363` proves the reuse
contract: when a prior `ERROR_RETRYABLE` document with no
`transport_request_id` is found, it is transitioned back to
`PREPARED` and reused — no new lnd is drawn.

Crash-resume at `write_path.py:364-377` proves the same for the
`SIGNED`/`ENCRYPTED` resume path: the existing document (with its
already-allocated lnd) is restored.

### 1.3 Race-condition surface

Four scenarios M3 must answer for whichever candidate is chosen:

1. **Concurrent writers under lease.** The inbox lease in
   `_stage_acquire_and_validate` (`write_path.py:194-205`) gives
   single-writer-per-FN at the application layer, but the DB layer
   must remain correct without trusting the lease (defence in
   depth). Two workers racing on `next_lnd` must NOT both observe
   the same value.
2. **Crash mid-allocation.** Process dies between the
   `next_lnd = next_lnd + 1` UPDATE and the
   `INSERT INTO fiscal_documents`. Today (Python) both are in one
   tx and roll back together — a "burnt" lnd cannot occur. M3 must
   preserve this property unless an alternative is chosen.
3. **Recovery on App::boot.** A pending document at
   `lnd = N` exists with `next_lnd = N+1` in `node_state`. App::boot
   must NOT decrement / reset `next_lnd` (would re-issue N+1 on the
   next allocation and collide). The pending doc must be picked up
   for re-drive at its existing lnd.
4. **lnd reuse on retry.** As cited at `write_path.py:351-363`, a
   retry on `ERROR_RETRYABLE` reuses the same lnd. The allocation
   mechanism must be no-op on retry — i.e. the allocator must NOT
   be re-invoked unconditionally per stage entry; it must be
   gated on "is this a fresh document, or a resumed one?".

### 1.4 Candidate evaluation

#### (a) [PRIMARY] `node_state.next_lnd` transactional sequencer + `UNIQUE(fiscal_number, lnd)`

A shared per-FN counter row is advanced inside `with_immediate`,
paired with a UNIQUE constraint that fails-closed on any drift.

- **Rust shape.** `next_lnd` already exists at
  `rust/prro/migrations/001_core_identities.sql:64`. A
  fail-closed UNIQUE index would be added on
  `fiscal_documents(fiscal_number, lnd)` (today only a non-unique
  index `ix_fd_fn_lnd` is present at
  `migrations/002_fiscal_documents.sql:50`).
- **Allocation pattern.** Inside `with_immediate`:
  `UPDATE node_state SET next_lnd = next_lnd + 1 WHERE
  fiscal_number = ? RETURNING next_lnd - 1 AS allocated_lnd`,
  then `INSERT INTO fiscal_documents (..., lnd) VALUES (..., ?)`.
- **Pros.** Direct port of the Python contract; doc-comment at
  `node_state.py:23-32` already justifies the gap-tolerance
  semantics; UNIQUE constraint promotes any logic bug from
  silent corruption to a fail-closed error. Single source of
  truth for "next" — recovery does not need to scan
  `fiscal_documents` to know the next lnd. Cheap on the hot path
  (one UPDATE...RETURNING).
- **Cons.** Adds a hot-write row contended on every document.
  Mitigated by the per-FN single-writer lease; sqlite WAL absorbs
  writer contention via `BEGIN IMMEDIATE`. A zombie process
  holding a transaction over the lease boundary could double-bump
  `next_lnd` and burn a number on rollback — that is the
  documented "gap is legal" outcome.
- **Race safety.** UNIQUE(fn, lnd) makes any drift between the
  counter and inserted lnd a hard error, never silent corruption.
  Two writers cannot both consume the same lnd because the
  allocator is inside `BEGIN IMMEDIATE`; the second will block
  on the writer lock and observe the post-commit value.
- **Recovery.** App::boot: read `next_lnd` once; do NOT touch it
  unless reseeding from DPS (out of scope for M3a). Pending docs
  re-drive at their existing `fiscal_documents.lnd`.

#### (b) `MAX(lnd)+1` computed inside `BEGIN IMMEDIATE` + `UNIQUE(fiscal_number, lnd)`

No shared counter row; relies on the lock + UNIQUE alone.

- **Pros.** No `node_state.next_lnd` column needed (could remove
  one mutable row); `MAX(lnd)` is "obviously correct" for
  reviewers. Self-healing: if any external party inserts a row
  with a higher lnd (e.g. operator data fix), the next allocation
  jumps over it.
- **Cons.** O(log N) scan per allocation (index seek to last
  row), vs O(1) row-update. More importantly, `MAX(lnd)` over
  `fiscal_documents` is **not the same as `next_lnd`** when gaps
  are deliberate: e.g. an operator moves an offline document
  into the table out of order — `MAX(lnd)+1` would skip the gap;
  Python today does NOT skip (it advances the counter
  monotonically). Behavioural drift from Python.
- **Race safety.** Same as (a) once UNIQUE is in place. Without
  UNIQUE, two concurrent writers each computing `MAX(lnd)+1`
  inside their own `BEGIN IMMEDIATE` would serialise on the
  writer lock — safe but the second is forced to recompute.
- **Recovery.** No counter to seed; App::boot is simpler. But
  loses the explicit "next_lnd" signal a reader can use to know
  what is about to be allocated without scanning.

#### (c) SQLite ROWID-style AUTOINCREMENT scoped per-FN

- **Pros.** Native SQLite primitive; can be combined with a
  composite key.
- **Cons.** SQLite AUTOINCREMENT is a property of an INTEGER
  PRIMARY KEY column; it cannot be scoped per-fiscal-number
  without one table per FN (operationally absurd) or an extra
  surrogate table per FN. The `sqlite_sequence` table is
  monotonic-globally, not per-FN. Fundamentally the wrong
  primitive for this requirement.
- **Race safety.** N/A — primitive does not exist.
- **Recovery.** N/A.
- **Verdict:** rejected as not-feasible-without-rewrite.

#### (d) In-memory counter held by the lease holder, persisted at finalize

- **Pros.** No DB write on the hot path for the allocation
  itself — counter advances in worker memory; persisted to
  `next_lnd` only when the document finalises.
- **Cons.** Crash-loss surface: if the process dies between
  in-memory bump and finalize-time persist, lnd is "lost" and
  the next process boot has to scan `fiscal_documents` to
  reconstruct (expensive). Reuse-on-retry semantics are harder
  to implement correctly. Worse: violates the
  "fiscal_documents.lnd is the authoritative chain-recovery
  key" property by introducing a window where the counter and
  the row disagree. Breaks the Python contract that lnd is
  observable in `node_state` at all times.
- **Race safety.** Brittle. Only one worker can hold the
  counter at a time (which is already the lease invariant), but
  any restart-without-finalize corrupts the chain.
- **Recovery.** Hard. Requires `MAX(lnd)+1` fallback at boot,
  collapsing into (b).

### 1.5 Decision: (a)

**`node_state.next_lnd` transactional sequencer +
`UNIQUE(fiscal_number, lnd)`**.

Rationale:

1. Direct behavioural port of Python (`node_state.py:21-46`,
   `write_path.py:414`), which is the M3 reference.
2. `next_lnd` already exists in the Rust schema
   (`migrations/001_core_identities.sql:64`); only the UNIQUE
   index needs to be added — minimal-diff posture.
3. UNIQUE(fn, lnd) is fail-closed: any drift becomes a hard
   constraint violation rather than silent fiscal-chain
   corruption — aligned with project priorities #1 (correctness)
   and #2 (recovery + auditability).
4. Recovery is trivial: `next_lnd` is the single source of
   truth; App::boot does not touch it.
5. Hot-path cost is O(1) UPDATE...RETURNING — equivalent to
   Python — vs O(log N) MAX(lnd) scan.

The UNIQUE index is a **new migration** for M3 — propose
`007_lnd_unique.sql` with
`CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd)`.
Existing `ix_fd_fn_lnd` (non-unique) stays for now; the unique
index supersedes it for query planning. Deduplicate post-pilot.

### 1.6 Rejected alternatives

- **(b) MAX(lnd)+1** — rejected: behavioural drift from Python
  (jumps gaps that Python preserves), more expensive on the hot
  path, no observable `next_lnd` for recovery readers.
- **(c) AUTOINCREMENT** — rejected: SQLite primitive cannot be
  scoped per-FN without one-table-per-FN. Not feasible.
- **(d) In-memory counter** — rejected: crash-loss surface,
  violates "lnd visible in `node_state` at all times" invariant,
  collapses into (b) at recovery.

---

## 2. State machines

### 2.1 DocState — 12 values

**Citation:** `rust/prro/src/db/models/enums.rs:29-42` (the
`str_enum!(DocState { ... })` block); SQL CHECK constraint
mirror at `rust/prro/migrations/002_fiscal_documents.sql:14-18`.

**Pending set citation:** `rust/prro/src/db/repositories/fiscal_documents.rs:175-185`
(7 states: PREPARED, SIGNED, ENCRYPTED, SENT, KVT1, KVT2,
ERROR_RETRYABLE; explicit exclusions for
ACK/REJECTED/CANCELLED, OFFLINE_LOCAL_ACK,
REQUIRES_MANUAL_RECONCILIATION).

#### 2.1.1 State table

| State | Class | Indexed in pending? | Notes |
|---|---|---|---|
| PREPARED | pending | yes | Created post-lnd-allocation; awaiting sign |
| SIGNED | pending | yes | CMS/PKCS#7 (DPS) or detached (Checkbox) attached; awaiting send |
| ENCRYPTED | pending | yes | Envelope wrapped; awaiting send (Checkbox-flow specific) |
| SENT | pending | yes | Wire submission attempted; awaiting KVT1 |
| KVT1 | pending | yes | First protocol receipt persisted; awaiting KVT2 |
| KVT2 | pending | yes | Second receipt persisted; awaiting ACK transition. INCLUDED in pending to avoid stranding between KVT2 and ACK (cited `fiscal_documents.rs:178-180`) |
| ACK | terminal-success | no | Only true terminal-success state |
| OFFLINE_LOCAL_ACK | handed-off | no | Handed to `offline_sync_service` worker (cited `fiscal_documents.rs:184`); excluded from re-drive loop |
| REJECTED | terminal | no | Business rejection by backend |
| CANCELLED | terminal | no | Operator/system cancellation |
| ERROR_RETRYABLE | pending | yes | Transient failure; recovery loop re-drives. Retry rules → W0-3 |
| REQUIRES_MANUAL_RECONCILIATION | operator | no | Operator-driven flow (cited `fiscal_documents.rs:185`); excluded from automated re-drive |

#### 2.1.2 allowed_transition matrix

Authoritative whitelist: `allowed_transition` at
`rust/prro/src/db/repositories/fiscal_documents.rs:81-103`.
Transcribed (left = from, header = to):

| from \ to | Signed | Encrypted | Sent | Kvt1 | Kvt2 | Ack | OfflineLocalAck | Rejected | ErrorRetryable | RequiresManualReconciliation |
|---|---|---|---|---|---|---|---|---|---|---|
| Prepared | yes | — | — | — | — | — | — | yes | — | — |
| Signed | — | yes | — | — | — | — | yes | — | yes | — |
| Encrypted | — | — | yes | — | — | — | — | — | yes | — |
| Sent | — | — | — | yes | — | — | — | yes | yes | — |
| Kvt1 | — | — | — | — | yes | — | — | — | yes | — |
| Kvt2 | — | — | — | — | — | yes | — | — | — | — |
| OfflineLocalAck | — | — | yes | — | — | — | — | — | — | — |
| ErrorRetryable | — | — | yes | yes | — | — | — | — | — | yes |

**Side-effects per transition (M3a-relevant subset):**

- `Prepared → Signed`: persist `document_files(SIGNED_XML)`
  and/or `PAYLOAD_XML`; CMS bytes computed OUTSIDE the
  transaction (invariant #1) and passed in by value, mirroring
  W2 cert_refresher precedent (cited M2-handoff §1 W2 — "compute_fingerprint hoisted **above** `with_immediate`").
- `Signed → Encrypted`: Checkbox-flow only — wraps signed payload in DPS envelope (out of M3a scope; pipeline exits at Sent for ONLINE M3a).
- `Encrypted → Sent`: gRPC `send_chk` call; happens **OUTSIDE** the transaction. State transition committed AFTER successful send returns.
- `Sent → Kvt1` / `Kvt1 → Kvt2`: DPS protocol receipts; persist `document_files(KVT1_RAW/KVT2_RAW)`. KVT byte-equiv goldens deferred to post-M2 (M2-handoff §2.3).
- `Kvt2 → Ack`: terminal-success bookkeeping; update `node_state` (e.g. `last_known_unsigned_xml_sha256` for MAC chain — cited `migrations/001_core_identities.sql:70`).
- `* → ErrorRetryable`: error compensation enters W0-3 retry policy domain.
- `* → Rejected`: enrich `audit_log` with backend rejection payload; terminal — no re-drive.
- `Signed → OfflineLocalAck`: offline-flow transition (out of M3a "ONLINE only" scope; documented for completeness).
- `OfflineLocalAck → Sent`: re-entry from offline_sync (cited `src/prro_gateway/services/offline_sync.py:10-14`).
- `ErrorRetryable → RequiresManualReconciliation`: terminal escalation after retry budget exhausted (W0-3 will define exact policy).

**Required transitions per acceptance criteria — verified present:**

- PREPARED→SIGNED ✓
- SIGNED→ENCRYPTED ✓
- ENCRYPTED→SENT ✓
- SENT→KVT1 ✓
- KVT1→KVT2 ✓
- KVT2→ACK ✓
**Whitelist gap analysis — failure-class × source-state matrix.**

The whitelist gaps are **intentional M3 design choices**, not omissions to be patched "just in case". The table below maps every pending source-state × M3 failure class to its target state under the existing Rust whitelist, with rationale per gap:

| from \ failure | TransportError (transient, retryable) | DPS BusinessReject (permanent) | DecodeError (server contract drift) | RetryBudgetExhausted | OperatorEscalation |
|---|---|---|---|---|---|
| Prepared | n/a (no network yet) | →Rejected (only via pre-sign business validation; cited whitelist line 86) | n/a | →Rejected | →Rejected |
| Signed | n/a (no network yet for ONLINE; offline branch routes →OfflineLocalAck) | **n/a — by design**: DPS business reject IMPOSSIBLE while persisted=Signed because the request has not yet gone over the wire. See "Design constraint" below. | n/a | →ErrorRetryable | (Signed→ErrorRetryable→RequiresManualReconciliation chain) |
| Encrypted | n/a (about to send) | n/a — same reason | n/a | →ErrorRetryable | (Encrypted→ErrorRetryable→RequiresManualReconciliation chain) |
| Sent | →ErrorRetryable | →Rejected (DPS rejected post-parse; whitelist line 94) | →ErrorRetryable | →ErrorRetryable | (Sent→ErrorRetryable→RequiresManualReconciliation chain) |
| Kvt1 | →ErrorRetryable | n/a — KVT1 is the transport receipt; reject after KVT1 is a server-protocol violation | →ErrorRetryable | →ErrorRetryable | (Kvt1→ErrorRetryable→RequiresManualReconciliation chain) |
| Kvt2 | n/a (recovery re-drives forward to Ack only) | n/a — Kvt2 is final DPS commit; reject after Kvt2 is a server bug | n/a | n/a | n/a |
| ErrorRetryable | re-drive (→Sent, →Kvt1) | n/a (already past wire) | n/a | →RequiresManualReconciliation | →RequiresManualReconciliation |

**Design constraint — SIGNED→REJECTED is INTENTIONALLY ABSENT.** A DPS business rejection (e.g. `ERROR_NOT_OPEN_SHIFT`, `ERROR_BAD_HASH_PREV`) can only be observed after the request lands at the server, which means the document MUST first be persisted as `Sent` (the state-flip happens after wire send returns; cited §3.4 below). Therefore the only legal target for a business-reject failure class is `Sent → Rejected`. M3 must NOT broaden the whitelist to add `Signed → Rejected` "for sidecar business reject" — if a sidecar pre-signs and rejects locally, that is a **pre-sign validation failure** which routes through the Prepared→Rejected gate, not the Signed→Rejected one.

**Design constraint — Prepared→ErrorRetryable absence is intentional.** A fresh PREPARED with no signed artefact has nothing to retry; the only exit on failure is Prepared→Rejected.

**Design constraint — Kvt2→ErrorRetryable absence is intentional.** A crash between persisting KVT2 and committing the Kvt2→Ack transition leaves the doc in pending; the recovery loop must re-drive Kvt2 forward to Ack, not back to ErrorRetryable. Cited `fiscal_documents.rs:178-180` "KVT2 IS pending".

**any pending→OFFLINE_LOCAL_ACK** — only from Signed today. Consistent with offline-flow design: signing happens locally before the offline branch decision; offline session is opened upstream of Signed, and once a doc is past Sent the offline branch is no longer applicable. Sent / Kvt1 / Kvt2 → OfflineLocalAck would represent a server-was-online-then-went-offline-mid-flight scenario that is not part of the M3 fault model.

**OFFLINE_LOCAL_ACK → ACK / KVT1 / KVT2 / REJECTED / REQUIRES_MANUAL_RECONCILIATION + self-loop** — explicit **M3b whitelist extension** required. The Rust whitelist at `fiscal_documents.rs:98` today has only `OfflineLocalAck → Sent`; the Python offline_sync state machine at `src/prro_gateway/services/offline_sync.py:10-14` documents 6 distinct outcomes for OFFLINE_LOCAL_ACK plus a retry self-loop:
- `OFFLINE_LOCAL_ACK → ACK` — DPS confirmed synchronously
- `OFFLINE_LOCAL_ACK → SENT` — DPS accepted async (already in M2 whitelist)
- `OFFLINE_LOCAL_ACK → KVT1` — DPS receipt arrived during sync
- `OFFLINE_LOCAL_ACK → KVT2` — DPS receipt arrived during sync (later stage)
- `OFFLINE_LOCAL_ACK → REJECTED` — DPS business rejection (terminal)
- `OFFLINE_LOCAL_ACK → REQUIRES_MANUAL_RECONCILIATION` — terminal failure / max retries
- `OFFLINE_LOCAL_ACK → OFFLINE_LOCAL_ACK` (self-loop) — retryable failure; `recovery_attempts++`

This is M3b scope (offline lifecycle); for M3a (ONLINE only) the gap is non-blocking.

**ERROR_RETRYABLE → re-driven** ✓ (→Sent, →Kvt1).
**REQUIRES_MANUAL_RECONCILIATION** ingress — present (only via ErrorRetryable per `fiscal_documents.rs:101`). Egress — none today; operator-driven; out of scope for M3a.

### 2.2 ShiftState — 6 values

**Citation:** `rust/prro/src/db/models/enums.rs:44-51`; SQL CHECK
mirror at `rust/prro/migrations/001_core_identities.sql:37`
(shifts table) and at `:61` (node_state.shift_state mirror).

#### 2.2.1 State table

| State | Class | Notes |
|---|---|---|
| CREATED | initial | Row inserted; SHIFT_OPEN command not yet driven |
| OPENING | pending | SHIFT_OPEN doc in flight |
| OPENED | active | Shift open; receipts allowed (SELL/RETURN/etc.) |
| CLOSING | pending | SHIFT_CLOSE / Z_REPORT in flight |
| CLOSED | terminal | Shift closed; Z report archived |
| ERROR | terminal-failure | Operator-recoverable; blocks new doc emission |

#### 2.2.2 allowed_transition

| from \ to | OPENING | OPENED | CLOSING | CLOSED | ERROR |
|---|---|---|---|---|---|
| CREATED | yes | — | — | — | yes |
| OPENING | — | yes | — | — | yes |
| OPENED | — | — | yes | — | yes |
| CLOSING | — | — | — | yes | yes |
| CLOSED | — | — | — | — | — |
| ERROR | — | — | — | — | — |

**Required transitions — verified present:**

- CREATED→OPENING ✓
- OPENING→OPENED ✓
- OPENED→CLOSING ✓
- CLOSING→CLOSED ✓
- any→ERROR ✓ (from any non-terminal)

(Note: there is no Rust-side `allowed_transition` whitelist for
shifts in M2 code — only the SQL CHECK at
`migrations/001_core_identities.sql:37`. M3 should add an
allowed_transition whitelist mirroring the doc-state pattern in
`fiscal_documents.rs:81-103`. Recorded in §6 as proposed.)

#### 2.2.3 Shift × Offline session interaction

When shift CLOSING begins, the offline session's status interacts
with shift transitions:

- **Pre-condition for CLOSING.** All offline-routed documents
  must have been drained (offline_sync reached terminal states
  for every OFFLINE_LOCAL_ACK doc) OR the operator has explicitly
  acknowledged the unsynced ones via REQUIRES_MANUAL_RECONCILIATION.
  Cited `services/offline_sync.py:589,602` show
  `ShiftRepository.update_state` writes happening from offline_sync
  context — i.e. shift close is gated on offline-sync progress.
- **Pre-condition for CLOSED.** Offline session must be in
  `CLOSED` (or `ABORTED` for an operator-cancelled flow). The
  partial UNIQUE index
  `ix_offline_active ON offline_sessions(fiscal_number) WHERE
  status IN ('OPENING','OPEN','CLOSING')`
  (`migrations/004_offline_and_routing.sql:16-17`) enforces "at
  most one live offline session per FN" — reaching shift CLOSED
  with the index still populated would be a corruption signal.
- **Channel switch invariant** (`CLAUDE.md` invariant #3 + M2-handoff §4.1): channel switch is forbidden with an open shift. M3 entry-state guard must read `shifts.state == OPENED` and refuse the switch.

### 2.3 Offline session — 5 values

**Citation (canonical Rust):** `rust/prro/migrations/004_offline_and_routing.sql:6` —
`status TEXT NOT NULL CHECK (status IN ('OPENING','OPEN','CLOSING','CLOSED','ABORTED'))`.

(Python legacy reference: the W0 plan calls out `sql/001_hot_store_init.sql:186`
for parity audit — Rust migration 004 is authoritative for M3.)

#### 2.3.1 State table

| State | Class | Notes |
|---|---|---|
| OPENING | pending | Operator/auto-trigger has filed open intent; ASK_OFFLINE_CODES not yet completed |
| OPEN | active | Offline mode active; new docs route to offline pool |
| CLOSING | pending | Online recovered; offline_sync draining queue |
| CLOSED | terminal | All docs synced or escalated; operator-acknowledged closure |
| ABORTED | terminal | Operator-cancelled or reset path |

#### 2.3.2 allowed_transition (proposed)

There is no Rust-side `allowed_transition` whitelist for offline
sessions in M2 code (only the CHECK constraint). Proposed for M3:

| from \ to | OPEN | CLOSING | CLOSED | ABORTED |
|---|---|---|---|---|
| OPENING | yes | — | — | yes |
| OPEN | — | yes | — | yes |
| CLOSING | — | — | yes | yes |
| CLOSED | — | — | — | — |
| ABORTED | — | — | — | — |

#### 2.3.3 Partial-UNIQUE index

`migrations/004_offline_and_routing.sql:16-17`:

```sql
CREATE INDEX ix_offline_active ON offline_sessions(fiscal_number, status)
    WHERE status IN ('OPENING','OPEN','CLOSING');
```

Note: this is **not** declared `UNIQUE` in the migration file. The
W0 plan acceptance criterion (line 86) refers to a "partial UNIQUE
index on offline_sessions(fiscal_number) WHERE status IN
('OPENING','OPEN','CLOSING')". **Finding:** the migration file
declares a non-unique partial index. If single-active-session
semantics are intended (and they are — confirmed by
`OfflineRepository.get_open_session` usage at
`write_path.py:391`), this should be UNIQUE. Recorded in §6 as
proposed amendment.

**Out-of-scope rationale.** M3a is "ONLINE only"; offline session
implementation is M3b. State machine documented here so shift
transitions interact with a concrete contract.

### 2.4 NodeMode — 7 values

**Citation:** `rust/prro/src/db/models/enums.rs:53-61`; SQL CHECK
mirror at `rust/prro/migrations/001_core_identities.sql:60`.

#### 2.4.1 State table

| State | Class | M3a entry? | Notes |
|---|---|---|---|
| ONLINE | active | **yes (only)** | Default operating mode; full pipeline allowed |
| GOING_OFFLINE | pending | no | In-flight transition (ASK_OFFLINE_CODES, etc.) |
| OFFLINE | active | no | Offline pool active; docs deferred to offline_sync |
| GOING_ONLINE | pending | no | Recovery from OFFLINE; offline_sync draining |
| BLOCKED | terminal-soft | no | Monthly offline-time limit reached; only management ops accepted (cited `write_path.py:262-268`) |
| STOP_MODE | terminal-soft | no | DB integrity check failed; reject everything except GET_STATUS (cited `write_path.py:218-224`) |
| CRYPTO_DEGRADED | recoverable | no | Crypto circuit breaker open; reject fiscal ops requiring sign (cited `write_path.py:232-259`) |

#### 2.4.2 allowed_transition (proposed)

No Rust-side whitelist exists today. Recovery-relevant transitions
flagged with **(R)**:

| from \ to | ONLINE | GOING_OFFLINE | OFFLINE | GOING_ONLINE | BLOCKED | STOP_MODE | CRYPTO_DEGRADED |
|---|---|---|---|---|---|---|---|
| ONLINE | — | yes | — | — | yes | yes (R) | yes |
| GOING_OFFLINE | — | — | yes | — | yes | yes (R) | yes |
| OFFLINE | — | — | — | yes | yes | yes (R) | yes |
| GOING_ONLINE | yes (R) | — | yes | — | yes | yes (R) | yes |
| BLOCKED | yes | — | — | — | — | yes (R) | — |
| STOP_MODE | — | — | — | — | — | — | — |
| CRYPTO_DEGRADED | yes (R) | — | — | — | yes | yes (R) | — |

Recovery transitions:
- `CRYPTO_DEGRADED → ONLINE` (R) — verified at `write_path.py:739-741` (`if _recovery_confirmed and _ns.mode == NodeMode.CRYPTO_DEGRADED: NodeStateRepository.update_mode(..., mode=NodeMode.ONLINE)`).
- `GOING_ONLINE → ONLINE` (R) — by offline_sync drain completion.
- `* → STOP_MODE` (R) — DB integrity boot check.
- `BLOCKED → ONLINE` — month rollover resets `current_month_offline_seconds` (`migrations/001_core_identities.sql:69`).

**M3a entry-state restriction:** only `ONLINE` is an entry state.
Documents arriving while node is in any other mode are rejected
in `_stage_acquire_and_validate` (`write_path.py:218-268`) before
lnd is allocated.

---

## 3. M3a happy-path sequence

Pipeline: **acquire+validate → guard → sign → send → finalize**.
(Note: Python condenses guard into acquire+validate; M3a separates
them for explicit documentation. The Rust implementation may keep
them as one stage if it wants, as long as the lock-discipline
invariants hold.)

Per-stage table:

### 3.1 Stage 1 — acquire+validate

| Aspect | Specification |
|---|---|
| Pre-condition (state) | NodeMode == ONLINE; FN config present |
| Pre-condition (input) | Inbox row in NEW; lease available |
| Lock open | `with_immediate` opens (mirrors `write_path.py:195`) |
| Operations inside lock | `InboxRepository.acquire_lease` → set status PROCESSING; read `node_state`; mode/breaker fast-paths; guard preconditions; allocate `lnd` via `next_lnd` UPDATE; `INSERT fiscal_documents(state=PREPARED, lnd, ...)`; `audit_log` append |
| Crypto/network sites | **NONE.** No DPS, no sidecar, no CMP. CMS is computed in stage 3 OUTSIDE this lock |
| Persistence writes | inbox.status=PROCESSING; node_state.next_lnd advanced; fiscal_documents inserted (state=PREPARED); audit_log row |
| Lock close | `commit` after all writes (mirrors `write_path.py:461`) |
| Output to next stage | `WorkerContext { inbox, command, node_state, active_shift, document(state=PREPARED) }` |

**Resume branch.** If `get_by_request_id` finds an existing doc,
the resume-table at §2.1 (PREPARED/SIGNED/ENCRYPTED/SENDING) is
honoured (cited `write_path.py:351-384`). Resume reuses the
existing lnd; no new allocation occurs.

### 3.2 Stage 2 — guard (sub-stage of acquire in Python; explicit here)

Already executed inside stage 1. Listed for diagram clarity:
shift-state guard, channel-switch-with-open-shift guard, offline-mode-required
guard, receipt validation. All read-only against the worker's
already-open transaction; no separate lock.

### 3.3 Stage 3 — sign

| Aspect | Specification |
|---|---|
| Pre-condition (state) | document.state == PREPARED |
| Pre-condition (node) | NodeMode != CRYPTO_DEGRADED with breaker open |
| Lock open | **No lock at start.** CMS bytes built OUTSIDE lock |
| Operations OUTSIDE lock | `prro::xml::build_canonical_xml(&CanonicalDoc)` (M2 frozen artefact; cited M2-handoff §2.3); `CryptoProvider::sign_cms_detached(SignCmsRequest)` (M2-handoff §2.1) — these are async, may use `spawn_blocking`, must NOT be called inside `with_immediate` (CLAUDE.md invariant #1; W2 cert_refresher precedent — `compute_fingerprint` hoisted above lock) |
| Lock open (post-sign) | `with_immediate` opens (mirrors `write_path.py:737`) |
| Operations inside lock | `transition_state(document_id, Prepared, Signed)` via CAS UPDATE; `document_files INSERT(SIGNED_XML)`; `document_files INSERT(PAYLOAD_XML)` for DPS; optional `node_state` mode-flip CRYPTO_DEGRADED→ONLINE on success hysteresis (`write_path.py:739-741`); `audit_log` append |
| Persistence writes | fiscal_documents.state=SIGNED; document_files rows; node_state.mode (rare); audit_log |
| Lock close | `commit` |
| Output to next stage | `signed_payload: bytes` (CMS DER) carried in WorkerContext |

**Hand-off note: prevhash chain.** Before stage 3 builds the XML,
`_resolve_dps_mac` (`write_path.py:477-509`) reads the previous
ACKed document's PAYLOAD_XML SHA256 to compute the MAC for this
doc. This read happens OUTSIDE any tx (the prev-doc PAYLOAD_XML is
immutable post-ACK). The MAC is then embedded in the canonical XML
**before sign**. Persistence of `last_known_unsigned_xml_sha256`
into `node_state` happens at the finalize stage (5).

### 3.4 Stage 4 — send

| Aspect | Specification |
|---|---|
| Pre-condition (state) | document.state == SIGNED (M3a ONLINE-only; ENCRYPTED is Checkbox-flow, out of M3a scope) |
| Lock open | **No lock at start** |
| Operations OUTSIDE lock | `DpsChannel::send_chk(...)` (M2-handoff §2.2 frozen contract); `grpc-timeout` per RPC; channel reuse |
| Lock open (post-send) | `with_immediate` opens for state transition + KVT persistence |
| Operations inside lock | `transition_state(SIGNED, Sent)` (or →ErrorRetryable on transport error → W0-3); `submission_attempted_at` set; `document_files INSERT(KVT1_RAW)` if returned; `transition_state(Sent, Kvt1)` if KVT1 received in same RPC; `audit_log` append |
| Persistence writes | fiscal_documents.state=SENT/KVT1; document_files.KVT1_RAW; audit_log |
| Lock close | `commit` |
| Output to next stage | `WorkerContext { document(state=SENT or KVT1), kvt1_raw? }` |

**KVT2 latch.** Some DPS endpoints return KVT1 immediately and
KVT2 asynchronously. M3a treats SENT and KVT1 both as awaiting-KVT2
states (per `fiscal_documents.rs:178-180` rationale on KVT2 being
in the pending set). The reconciliation loop (W0-3) drives KVT1→KVT2.

### 3.5 Stage 5 — finalize

| Aspect | Specification |
|---|---|
| Pre-condition (state) | document.state == KVT2 (post-KVT1→KVT2 from reconciliation, or synchronous KVT2-in-send-response if endpoint supports it) |
| Lock open | `with_immediate` opens |
| Operations inside lock | `transition_state(Kvt2, Ack)` CAS UPDATE; `node_state.last_known_unsigned_xml_sha256` updated for next-doc MAC chain; inbox.status=DONE; `audit_log` append |
| Crypto/network sites | NONE |
| Persistence writes | fiscal_documents.state=ACK; node_state.last_known_unsigned_xml_sha256; ingress_inbox.status; audit_log |
| Lock close | `commit` |

### 3.6 Sequence diagram (text-art)

```
Worker ──acquire+validate (LOCK1)──> DB
        │  inbox→PROCESSING
        │  node_state.next_lnd++
        │  fiscal_documents INSERT (PREPARED, lnd)
        │  audit_log
        └─ COMMIT LOCK1

Worker ──build_canonical_xml (NO LOCK)──> [in-process]
Worker ──CryptoProvider.sign_cms_detached (NO LOCK)──> InProcessProvider/sidecar
        │ (returns SignedCmsBytes)

Worker ──persist sign (LOCK2)──> DB
        │  transition_state(Prepared→Signed)
        │  document_files INSERT(SIGNED_XML, PAYLOAD_XML)
        │  audit_log
        └─ COMMIT LOCK2

Worker ──DpsChannel.send_chk (NO LOCK)──> DPS gRPC
        │ (returns server response or error)

Worker ──persist send (LOCK3)──> DB
        │  transition_state(Signed→Sent[→Kvt1?])
        │  document_files INSERT(KVT1_RAW?)
        │  submission_attempted_at
        │  audit_log
        └─ COMMIT LOCK3

(KVT1→KVT2 may be inline if DPS returns both; otherwise driven by reconciliation loop)

Worker ──finalize (LOCK4)──> DB
        │  transition_state(Kvt2→Ack)
        │  node_state.last_known_unsigned_xml_sha256
        │  ingress_inbox.status=DONE
        │  audit_log
        └─ COMMIT LOCK4
```

**Invariant #1 audit:** all crypto and network sites are between
locks, never inside. Mirrors W2 cert_refresher precedent
(`compute_fingerprint` hoisted above `with_immediate`, cited
M2-handoff §1 W2).

---

## 4. CloseShift → Z_REPORT mapping

### 4.1 Wire reality

**WebCheck COM/1C surface:** `CloseShift` is exposed as a public
COM method (`docs/webcheck_reverse/WEBCHECK_ANALYSIS.md:77` —
"CloseShift — закриття зміни (Z-звіт)" in the COM-objект method
list). The annotation explicitly states it produces a Z report.

**WebCheck SQLite schema:** `CreateDB.cs:624` —

```sql
CREATE UNIQUE INDEX IF NOT EXISTS 'closeshiftind' ON 'ksef'
    ('shiftid','DocType') WHERE doctype = '80' and offline <> '-1';
CREATE UNIQUE INDEX IF NOT EXISTS 'openshiftind' ON 'ksef'
    ('shiftid','DocType') WHERE doctype = '8'  and offline <> '-1';
```

`doctype='80'` = the wire code WebCheck assigns to "shift close /
Z report" (matches DPS protocol typCheck=2 / doctype=80 used in
the canonical XML serialiser).

**WebCheck submit path:** `StringXML.cs:2509` — the OpenCloseShift
branch of SubmitCheck:

```csharp
TypErrSubmit typErrSubmit = submitPtr.SubmitCheck(
    text7, CheckIDv, 2, dd, "", "", OpenCloseShift: true);
```

The hard-coded `2` is the wire `Type` enum — i.e. the same
enum-position as the DPS proto's `ZREPORT = 2`. Plus the comment
just below at line 2515 says
`"Отпралвяемый Z отчет не принят сервером"` — "the dispatched Z
report was not accepted by the server" — proving CloseShift is
sent **as Z report**.

**DPS proto:** `rust/prro/proto/fiscal_server.proto:24` —

```
enum Type {
    UNKNOWN = 0;
    CHK = 1;
    ZREPORT = 2;
    SERVICECHK = 3;
}
```

There is no separate `SHIFT_CLOSE` wire type. The DPS server only
sees `ZREPORT`.

**M2 conclusion (already taken):** M2 W4 commit `fd81b03` ("scope
fix (CloseShift == Z_REPORT)") and the Rust XML builder ship 4 doc
types (`ShiftOpen`, `Sell`, `Return`, `ZReport`) with **no
separate ShiftClose** — explicit in M2-handoff §1 W4 line 81: "WebCheck CloseShift IS DPS Z_REPORT (typCheck=2, doctype=80)".

### 4.2 Python current behaviour

**Adapter mappings — the asymmetry surfaces here:**

- `src/prro_gateway/adapters/webcheck_xmlrpc.py:15` —
  `"CloseShift": OperationType.SHIFT_CLOSE,` (and:17)
  `"ZReport": OperationType.Z_REPORT,`
- `src/prro_gateway/adapters/maria_tcp.py:15` —
  `"CLOSE_SHIFT": OperationType.SHIFT_CLOSE,` (and:17)
  `"Z_REPORT": OperationType.Z_REPORT,`
- `src/prro_gateway/adapters/maria304_native.py:50,52` — same
  split.
- `src/prro_gateway/adapters/checkbox_rest.py:23,61` — only emits
  Z_REPORT.

**Result:** WebCheck's `CloseShift` wire op maps to `OperationType.SHIFT_CLOSE` end-to-end through Python: the adapter at `webcheck_xmlrpc.py:15` produces SHIFT_CLOSE, and the DPS XML serialiser at `dps_xml.py:113` has a branch only for `OperationType.Z_REPORT` — SHIFT_CLOSE does NOT enter that branch. Python therefore is **NOT** a normative proof of the SHIFT_CLOSE → ZReport wire mapping. The normative proofs are:
- M2 W4 finding (commit `fd81b03`, `8d43882`) — WebCheck CloseShift IS DPS Z_REPORT (typCheck=2, doctype=80).
- WebCheck reverse-engineering: `CreateDB.cs:624` (doctype='80'), `StringXML.cs:2509` (`OpenCloseShift: true` close routes through `SubmitCheck(typCheck=2)`).
- DPS proto: `rust/prro/proto/fiscal_server.proto:24` (`Check.Type::ZREPORT = 2`).

For Rust, the mapping is fixed by W4: SHIFT_CLOSE remains the internal label; the wire artifact for both SHIFT_CLOSE and Z_REPORT internal ops is `ZReport`.

**Z number allocation in Python is keyed on internal `OperationType.Z_REPORT`, NOT on the wire-kind:** `write_path.py:535` — `if ctx.command.operation_type == OperationType.Z_REPORT:` allocates `next_z_report_number`. SHIFT_CLOSE does **not** trigger Z allocation in Python. **Rust must NOT inherit this internal-op-keyed predicate.** Per §4.5 design constraint, M3a Z-allocation MUST derive `wire_artifact_kind` first, then allocate when `wire_artifact_kind == ZReport`. That makes the gate fire correctly for BOTH `SHIFT_CLOSE` and `Z_REPORT` internal labels after the boundary mapping, regardless of any upstream adapter behaviour.

### 4.3 Schema impact: fiscal_documents.doc_type values today

`rust/prro/migrations/002_fiscal_documents.sql:10-13` declares the
9 doc_type values:

```
'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
'CASH_WITHDRAWAL','X_REPORT','Z_REPORT'
```

`SHIFT_CLOSE` and `Z_REPORT` coexist in the schema. Migration
`enums.rs:74-82` mirrors this (`ShiftClose`, `ZReport`).

If we eliminate SHIFT_CLOSE end-to-end (candidate (a) below), the
SQL CHECK constraint would need to drop `SHIFT_CLOSE` from the
list — a destructive migration. If we keep SHIFT_CLOSE as an
internal label and map at the boundary (candidate (b)), no
migration is needed.

### 4.4 Candidate (a): rename Python adapter SHIFT_CLOSE → Z_REPORT end-to-end

**Pros:**
- Aligns Python internal label with wire reality and with M2 Rust
  surface (4 doc types, no ShiftClose).
- Eliminates the asymmetry where Z-number allocation only happens
  for `Z_REPORT` operations — every shift-close becomes a Z
  report and gets a number.
- Reduces the number of doc_type values to track (SHIFT_CLOSE
  drops out).
- One less branching point in serializer.

**Cons:**
- Schema migration: drop `SHIFT_CLOSE` from `doc_type` CHECK
  constraint. Existing rows in production DBs with
  `doc_type='SHIFT_CLOSE'` would either need rewriting or the
  migration must be additive-only (keep the CHECK list).
- Adapter rename touches all 4 adapters (webcheck_xmlrpc,
  maria_tcp, maria304_native, checkbox_rest). Every test fixture
  referencing `SHIFT_CLOSE` must change.
- **Pilot/COM-1C compatibility (PRRO_GATE-iap):** WebCheck COM
  clients using `CloseShift` see no change at the wire (still
  Z report), but any 1C code that queries `doc_type='SHIFT_CLOSE'`
  out of operator audit/log views breaks. Unknown blast radius
  on pilot operator dashboards.
- M3a is supposed to consume frozen Python contracts; renaming
  them retroactively is a churn surface mid-stream.

### 4.5 Candidate (b): keep SHIFT_CLOSE as internal label, map at adapter boundary

**Pros:**
- Zero schema churn — `doc_type='SHIFT_CLOSE'` stays valid.
- Zero Python adapter churn — current behaviour preserved.
- Pilot/COM-1C compatibility (PRRO_GATE-iap) unaffected: any
  operator dashboard, audit log entry, or 1C query that already
  references SHIFT_CLOSE keeps working.
- **The mapping happens exactly once** — at the Rust XML builder
  boundary, where SHIFT_CLOSE → ZReport canonical doc.
- Mirrors what M2 already shipped: 4 Rust XML doc types, of
  which `ZReport` doubles as both Z report and shift-close.

**Cons:**
- Two internal labels for one wire concept — reviewer load.
- Future extension (e.g. a non-Z shift close) is no longer
  available without re-introducing the distinction explicitly.

**M3a design constraint — Z-number allocation is keyed by WIRE artifact kind, NOT internal operation label.** Because candidate (b) maps `SHIFT_CLOSE → ZReport` at the Rust XML builder boundary (i.e. on the wire, both internal labels resolve to the same artifact `ZReport / typCheck=2`), the Z-number allocation MUST trigger on the wire artifact `ZReport`, NOT on the internal `OperationType` label. Otherwise a SHIFT_CLOSE input would reach the wire as Z_REPORT but **without** an allocated Z-number, producing malformed wire output. This is an M3a design constraint, not a Python-side latent fix.

The Python current behaviour does NOT send a SHIFT_CLOSE to DPS as a Z report at all: `write_path.py:535` Z-allocation guards on `OperationType.Z_REPORT`, and `dps_xml.py:113` Z-shape XML branch also guards on `OperationType.Z_REPORT` — SHIFT_CLOSE enters neither branch. Python is therefore not a normative proof of the SHIFT_CLOSE → ZReport wire mapping; the proof is M2 W4 + WebCheck reverse-engineering (cited §4.2). **Rust must NOT key Z allocation on internal op.** The M3a write-path Z-allocation guard MUST derive `wire_artifact_kind` first and allocate when `wire_artifact_kind == ZReport`, so it fires correctly for both `internal_op == SHIFT_CLOSE` and `internal_op == Z_REPORT` after the boundary mapping.

### 4.6 Decision: (b)

**Keep SHIFT_CLOSE as the internal canonical label; map to
ZReport at the Rust XML builder boundary.**

Rationale:
1. Minimal-diff posture (CLAUDE.md decision rules: "If a task can
   be solved either by changing architecture or by wiring the
   existing seam: wire the seam"). The seam is the Rust XML
   builder, which already maps SHIFT_CLOSE→ZReport for the
   canonical doc type — M2 W4 commit `fd81b03` proves this
   already shipped.
2. No schema migration. No pilot-blast-radius risk on COM-1C
   (PRRO_GATE-iap).
3. Audit and operator UI continuity preserved.

**M3a entry-state actions** (both must land in M3a impl):

(i) The Rust write-path must, when constructing the canonical doc for the XML builder, treat `doc_type ∈ {SHIFT_CLOSE, Z_REPORT}` as equivalent inputs to the ZReport builder. The mapping is exactly: SHIFT_CLOSE → ZReport, Z_REPORT → ZReport. Adapter side stays untouched.

(ii) **Z-number allocation MUST be keyed by wire artifact kind, NOT internal operation label.** Per the design constraint in §4.5: M3a write-path code derives `wire_artifact_kind` first and allocates the Z-number when `wire_artifact_kind == ZReport`, which evaluates true for both `internal_op == SHIFT_CLOSE` and `internal_op == Z_REPORT` after the boundary mapping. M3a code MUST NOT replicate Python's `if ctx.command.operation_type == OperationType.Z_REPORT` guard at `write_path.py:535` — that predicate ties allocation to the internal label and would silently fail to allocate for SHIFT_CLOSE inputs (which Rust now legitimately routes to the wire as ZReport, per the §4.6 boundary mapping).

### 4.7 Rejected alternative

- **(a) Rename end-to-end** — rejected: schema churn + adapter
  churn + unknown PRRO_GATE-iap blast radius without a
  corresponding fiscal correctness gain. Wire reality is already
  captured by the M2 Rust XML builder; the rename adds churn
  without changing the wire.

---

## 5. Reviewer checklist

A future reviewer must re-verify the following if any of the
above changes:

- **If a DocState is added or removed:**
  - `rust/prro/src/db/models/enums.rs:29-42` updated.
  - `rust/prro/migrations/002_fiscal_documents.sql:14-18` CHECK constraint updated (or new migration).
  - `allowed_transition` whitelist at `rust/prro/src/db/repositories/fiscal_documents.rs:81-103` updated.
  - `list_pending_for_fn` SQL at `:203` updated if pending-set classification changes.
  - `ix_fd_state_pending` partial index at `migrations/002_fiscal_documents.sql:54-55` updated.
  - `ix_fd_recon_manual` index at `:56-57` updated if manual-recon set changes.
  - This document's §2.1 state table + transition matrix re-verified.
  - W0-2 lock-discipline doc re-verified (which transitions need `with_immediate`).
  - W0-3 retry/recovery doc re-verified (recovery action per state).

- **If a ShiftState is added or removed:**
  - `rust/prro/src/db/models/enums.rs:44-51` updated.
  - `rust/prro/migrations/001_core_identities.sql:37` AND `:61` (node_state mirror) CHECK constraints updated.
  - This document's §2.2 re-verified.
  - Shift × offline interaction at §2.2.3 re-checked — both directions (shift transition gated on offline state, and offline session lifecycle gated on shift state).

- **If an OfflineSession status is added or removed:**
  - `rust/prro/migrations/004_offline_and_routing.sql:6` CHECK updated.
  - Partial index at `:16-17` updated (and verified UNIQUE-or-not as intended).
  - This document's §2.3 re-verified.

- **If a NodeMode is added or removed:**
  - `rust/prro/src/db/models/enums.rs:53-61` updated.
  - `rust/prro/migrations/001_core_identities.sql:60` CHECK updated.
  - All M3a entry-state guards (`write_path.py:218-268` analogues) re-checked for completeness.
  - This document's §2.4 re-verified.
  - W0-3 boot reconciliation contract re-verified.

- **If CloseShift mapping flips from (b) to (a):**
  - All 4 Python adapters renamed.
  - SQL CHECK constraint dropped `SHIFT_CLOSE`.
  - `enums.rs:74-82` `DocType::ShiftClose` removed.
  - PRRO_GATE-iap pilot impact assessment refreshed.
  - All test fixtures audited.
  - This document's §4 decision re-recorded.

- **If lnd source-of-truth flips from (a) to a different candidate:**
  - `node_state.next_lnd` either retained, repurposed, or dropped.
  - `UNIQUE(fiscal_number, lnd)` migration added/removed.
  - Python `increment_lnd` (`src/prro_gateway/repositories/node_state.py:21`) divergence audited if Rust diverges.
  - W0-3 boot recovery contract re-verified (next_lnd reseeding semantics).
  - Sign-retry resume path (`write_path.py:351-363`) re-verified for lnd-reuse property.

- **If the M3a happy-path sequence changes:**
  - Per-stage lock boundaries re-audited against CLAUDE.md invariant #1.
  - Crypto/network sites re-confirmed OUTSIDE every `with_immediate`.
  - W2 cert_refresher precedent (compute_fingerprint hoisted above lock) cited as the pattern reference.
  - W0-2 lock-discipline doc re-verified.

---

## 6. Proposed ADR amendments

The following amendments to
`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` are
**PROPOSED — NOT COMMITTED**. Coordinator to surface to user for
approval before any edit.

### 6.1 PROPOSED — NOT COMMITTED — ADR-M3-A1: lnd source-of-truth

```
Decision: M3 adopts `node_state.next_lnd` transactional sequencer
+ a new UNIQUE INDEX `ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd)`
as the lnd source-of-truth (W0-1 candidate (a)).

Rationale: direct Python-port (`node_state.py:21-46`,
`write_path.py:414`); minimal-diff (next_lnd column already
exists, only the UNIQUE index is new); fail-closed via UNIQUE
constraint; O(1) hot path; trivial recovery.

Migration: `007_lnd_unique.sql` adds
`CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd)`.
Existing non-unique `ix_fd_fn_lnd` retained pending post-pilot
deduplication.

Research-addresses: PRRO_GATE-ddn (bd-issue closure deferred to
M3a implementation time, not at this research close).
```

### 6.2 PROPOSED — NOT COMMITTED — ADR-M3-A2: CloseShift adapter mapping

```
Decision: M3 keeps `OperationType.SHIFT_CLOSE` as the internal
canonical label and maps SHIFT_CLOSE → ZReport at the Rust XML
builder boundary (W0-1 candidate (b)). The Rust write-path must
treat doc_type ∈ {SHIFT_CLOSE, Z_REPORT} as equivalent inputs to
prro::xml::build_canonical_xml's ZReport branch.

M3a design constraint (binding): Rust must not key Z allocation
on the internal OperationType label; it must derive
`wire_artifact_kind` first and allocate when
`wire_artifact_kind == ZReport`. This makes the gate fire
correctly for both SHIFT_CLOSE and Z_REPORT internal labels
after boundary mapping, independent of any upstream adapter
behaviour.

Rationale: zero schema churn; zero adapter churn; pilot/COM-1C
compatibility (PRRO_GATE-iap) preserved; the wire mapping is
already shipped in M2 W4 (commit fd81b03); Z-allocation
correctness is restored at the Rust wire-artifact boundary
instead of depending on Python's internal operation predicate.

Research-addresses: PRRO_GATE-zti (bd-issue closure deferred to
M3a implementation time).
```

### 6.3 PROPOSED — NOT COMMITTED — schema clarifications

```
M3 adds explicit `allowed_transition` whitelists for ShiftState,
OfflineSession status, and NodeMode in
`rust/prro/src/db/repositories/`, mirroring the DocState whitelist
at `fiscal_documents.rs:81-103`. SQL CHECK constraints alone do
not enforce transition graphs — they enforce membership only.

Specifically:

(1) DocState whitelist asymmetries (per §2.1 failure-class table):
    Whitelist gaps are **intentional M3 design choices**, not omissions:
    - `Signed/Encrypted/Kvt1/Kvt2 → Rejected` ABSENT by design —
      DPS business reject is impossible while persisted < Sent
      because the request has not yet gone over the wire. M3 must
      NOT broaden the whitelist for "sidecar business reject"; pre-sign
      validation failures route through Prepared→Rejected.
    - `Prepared/Kvt2 → ErrorRetryable` ABSENT by design — fresh
      Prepared has nothing to retry; Kvt2 recovery re-drives
      forward to Ack only.
    - `OfflineLocalAck → {Ack, Kvt1, Kvt2, Rejected,
      RequiresManualReconciliation}` plus self-loop — explicit
      M3b whitelist extension required for parity with
      `src/prro_gateway/services/offline_sync.py:10-14` (6 distinct
      external targets + retry-staying-in-OFFLINE_LOCAL_ACK self-loop).
      Non-blocking for M3a (ONLINE only).

(2) `ix_offline_active` (`rust/prro/migrations/004_offline_and_routing.sql:16`)
    is declared NON-UNIQUE today, but `OfflineRepository.get_open_session`
    at `src/prro_gateway/services/write_path.py:391` assumes singleton
    semantics (one open session per FN). The W0 plan acceptance criterion
    references "partial UNIQUE index". **Proposed M3b migration**
    (`008_offline_active_unique.sql` or similar):

    ```sql
    DROP INDEX ix_offline_active;
    CREATE UNIQUE INDEX ux_offline_active_per_fn
        ON offline_sessions(fiscal_number)
        WHERE status IN ('OPENING','OPEN','CLOSING');
    ```

    The new index is partial (matches Python's intent: at most one
    not-yet-CLOSED session per FN) and replaces the non-unique form.
    A behavioural test is required in M3b: insert two OPENING/OPEN
    sessions for the same fiscal_number and assert UNIQUE constraint
    failure with row count == 1. Pre-existing closed/aborted sessions
    do not block the partial index.

    **Scope:** M3b blocker (offline lifecycle relies on singleton).
    M3a non-blocker — the M3a pipeline is ONLINE-only and never opens
    offline sessions, so the constraint absence cannot be exercised
    in M3a.

These extensions are tracked for M3a (where applicable) and M3b
(offline branches).
```

### 6.4 No-op note

If user rejects 6.1, 6.2, or 6.3, the bd issues PRRO_GATE-ddn,
PRRO_GATE-zti remain open and M3a implementation must re-litigate.
This document remains the research artefact regardless.
