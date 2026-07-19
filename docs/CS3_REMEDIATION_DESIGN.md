# CS-3 Oracle Remediation — rev 3

**Status:** DESIGN ONLY. No production code or migration 035 exists yet.

**Ground truth:** `origin/main@9e6cf96`.

**Consumes:** `docs/CS3_ORACLE_AUDIT_FINDINGS.md`.

**Scope rule:** this revision does not add a table, a reservation state, a retry class, a node mode,
or a new domain aggregate. It reuses:

- `delivery_reservation`, its existing `apply_state`, and the already designed
  `EvidenceDiscriminant`;
- `node_state.mode = STOP_MODE` and the existing `admin::reset_stop_mode`;
- `fiscal_documents`, `transport_trace`, `audit_log`, and the existing transition repositories;
- the existing `last_chk` / reconciliation code where it can prove an exact match.

Migration 035 adds only columns, indexes, and triggers to the existing reservation table.

---

## 0. Independent re-audit corrections

The rev-2 verdict `SIMPLIFY_THEN_SOUND` was too optimistic. The independent re-audit found four
blocking defects. Rev 3 makes each one normative rather than leaving it as a residual.

### C1 — `seed_advanced` is removed

`seed_advanced` is not merely unnecessary; its claimed routed-reject state is unreachable:

- `route_send_result` maps `Ok` to `WireDecision::Sent` and `Err` to
  `WireDecision::Routed`, exclusively
  (`rust/prro/src/services/write_path/error_routing.rs:263-273`);
- SFN stamping and online seed advance occur only in the `Sent` arm
  (`stage_send.rs:1750-1817`);
- `classify` likewise makes Accepted (`routing=None`) disjoint from Rejected
  (`routing=Some`) (`prro-domain/src/delivery/mod.rs:892-973`).

Therefore a reservation cannot simultaneously be a routed reject and the clean `Sent` apply that
advanced the seed. Rev 3 has no `seed_advanced` column, trigger, fence disjunct, or pin.

### C2/P4 — the Rust payload is made durable

Adding an `EvidenceDiscriminant` field to `ObservedOutcomeV1` does not make it durable by itself.
Migration 035 therefore stores the already designed discriminant in four nullable union slots on
`delivery_reservation`:

```text
evidence_kind | evidence_text | evidence_code | evidence_digest
```

The exact leaf matrix is enforced by fail-closed INSERT/UPDATE triggers. Boot hydrates the same
existing `ObservedOutcomeV1` and rejects an unknown tag or illegal payload; there is no fallback
classification.

### C3/BRICK — no permanent SQL fence

Rev 2 held `SubmittedUnknown`, `MacRecovery`, and `OperatorEscalation` forever and referred to an
operator release that does not exist. Live code has no exit from document/shift RMR, and
`reset_stop_mode` currently does not touch reservations
(`admin.rs:281-390`; `fiscal_documents.rs:174-260`; `shifts.rs:74-94`).

Rev 3 does not add an invented release token or FSM state. An unresolved outcome remains in the
existing `PENDING_APPLY` state and the FN is put in the existing `STOP_MODE`. The existing
`reset_stop_mode` operation is strengthened:

1. ordinary STOP reset is refused while a CS-3 reservation is `PENDING_APPLY`;
2. an operator resolution completes that same reservation to `APPLIED` and resets
   `STOP_MODE -> GOING_ONLINE` in one `BEGIN IMMEDIATE`;
3. the decision and supplied reason are appended to the existing `audit_log`;
4. no outcome evidence is rewritten and the document is never wired again.

This is an extension of an existing operation over existing rows, not a new authority entity.

### C4 — `Sent + NotFound` halts the FN atomically

Changing only the document to RMR is unsafe: the document leaves the drain cohort and a successor can
become head. Rev 3 requires one existing `with_immediate` envelope that:

1. transitions the existing document `Sent -> RequiresManualReconciliation`;
2. sets existing `node_state.mode -> STOP_MODE`;
3. completes the existing transport trace and appends the audit row;
4. commits all four effects or none.

It deliberately does **not** move the shift to shift-RMR: live shift-RMR has no exit. `STOP_MODE`
already refuses ingress in `stage_acquire.rs:292-347` and `dispatch.rs:150-180`, and backlog drain
returns before its cohort scan when mode is not `GoingOnline`
(`offline_sync/backlog_drain.rs:687-704`). The existing reset surface therefore provides a real,
audited exit after operator review.

---

## 1. Safety properties and defending mechanisms

| Property | Defending mechanism |
|---|---|
| **P2: at most one DPS wire call per document lifetime** | Partial unique index on `document_id WHERE call_started_at IS NOT NULL`; the same `NOT EXISTS` condition in `authorize_submission`; marker committed before wire; all production send sites pass through the consumed non-Clone authorization. |
| **P3: the FN chain never forks** | Per-FN reservation fence holds RN, CS, and every PENDING apply; clean accept advances the seed before APPLIED in one transaction; unresolved outcomes enter STOP_MODE; `Sent+NotFound` enters STOP_MODE atomically. |
| **P4: no silent loss/double-apply across crash** | Exact durable evidence union; record and apply are separate commits; apply is full-tuple/generation guarded and idempotent; PENDING is boot-hydrated without wire I/O. |
| **BRICK: a legitimate next document is not refused forever** | No permanent outcome class remains in the SQL fence. Operator resolution finishes PENDING and resets STOP in one transaction. Definitive seed-unchanged rejects release automatically after APPLIED. |

The above is conditional on the named implementation gates in §7. Until they exist and bite,
the design is not an implementation GO.

---

## 2. P2 — lifetime call-once

Migration 035 adds:

```sql
CREATE UNIQUE INDEX ux_delivery_document_ever_started
    ON delivery_reservation(document_id)
    WHERE call_started_at IS NOT NULL;
```

Inside the existing `BEGIN IMMEDIATE` that performs
`RESERVED_NOT_STARTED -> CALL_STARTED`, `authorize_submission` must additionally prove:

```sql
NOT EXISTS (
    SELECT 1
      FROM delivery_reservation
     WHERE document_id = :document_id
       AND call_started_at IS NOT NULL
)
```

The marker, `authorized_generation`, active pointer, and returned authorization are minted only after
the transaction commits. A uniqueness violation or `rows_affected != 1` returns no authorization and
causes zero wire I/O.

`attempt_no = MAX(attempt_no)+1` remains legal only while all historical attempts are unstarted.
`delivery_reservation_no_replace` and the repository INSERT add this historical clause:

```sql
NOT EXISTS (
    SELECT 1
      FROM delivery_reservation
     WHERE document_id = :document_id
       AND call_started_at IS NOT NULL
)
```

After any attempt crossed CALL_STARTED, insertion of a fresh RN for that document is refused before
a row exists. This is necessary for BRICK as well as P2: allowing the RN and refusing only at
authorization would leave an unstartable active row that blocks the FN. The authorization query
retains the same condition as an independent pre-wire guard for legacy/race defence.

A connect refusal, timeout, cancellation, or dropped future after the committed marker still consumes
the document's one lifetime call. This is intentional: whether bytes reached DPS is unknown, so the
same document is reconciled/operator-resolved, never resent.

---

## 3. P3 + BRICK — the active fence and STOP handoff

### 3.1 Exact active-fence predicate

The same predicate must be byte-identical in `ux_reservation_active`,
`delivery_reservation_no_replace`, `get_active_for_fn`, and the D/E authorization query:

```sql
state IN ('RESERVED_NOT_STARTED', 'CALL_STARTED')
OR (
    state = 'OUTCOME_OBSERVED'
    AND apply_state = 'PENDING_APPLY'
)
```

There is no routing-class or certainty disjunct. Those conditions previously created permanent
fences. Safety is instead obtained by the atomic apply/STOP rules below.

### 3.2 Exhaustive release/hold table

| Row | Durable condition | Action | Fence after commit | Why safe / live |
|---|---|---|---|---|
| 1 | `RESERVED_NOT_STARTED` | no wire yet | HOLD | protects the single active FN plan |
| 2 | `CALL_STARTED` | wire outcome absent | HOLD | crash cannot authorize a new call |
| 3 | Accepted, `PENDING_APPLY` | stamp F, apply ledger, advance online seed, mark APPLIED in one tx | RELEASE | seed and ledger are visible with release |
| 4 | definitive seed-unchanged reject | apply target/audit, mark APPLIED | RELEASE | DPS did not accept; seed did not advance |
| 5 | `-11 Offline168` | apply reject + node `BLOCKED`, mark APPLIED in one tx | RELEASE reservation; node HOLD | existing admin reset gains a guarded BLOCKED-resolution branch after the 168h condition is cleared |
| 6 | SubmittedUnknown / ProbeRequired | keep PENDING; set node STOP in the record tx | HOLD | no new reservation and no same-doc wire |
| 7 | `-12 MacRecovery` | keep PENDING; set node STOP in the record tx | HOLD | chain repair must precede operator completion |
| 8 | `-6 OperatorEscalation` | keep PENDING; set node STOP in the record tx | HOLD | shift/order review must precede completion |
| 9 | safe NotSubmitted preflight failure | apply local outcome, mark APPLIED | RELEASE | no call occurred |
| 10 | previously Sent + reconciliation NotFound | doc RMR + node STOP + trace/audit in one tx | no active reservation; node HOLD | seed was already advanced; STOP prevents successor until review |

For rows 6–8, the record transaction stores evidence and flips node mode to STOP atomically while
leaving `apply_state=PENDING_APPLY`. SQLite `BEGIN IMMEDIATE` serializes the FN writer, and the
PENDING fence remains authoritative even if a future path forgets the mode gate.

### 3.3 Whole-fence enforcement

The reservation fence, not STOP_MODE alone, is authoritative while a row is active. Before any
issuance or chain-seed mutation, the caller must prove there is no conflicting active reservation
using §3.1. The D/E cutover covers:

- every production caller of `stage_send::run`;
- the `(ErrorRetryable, Sending)` redrive edge;
- `stage_send`, `stage_offline_ack`, `offline_code_replenish`, and boot recovery seed writers;
- new offline issuance/session/code allocation for the fenced FN.

This is a single production-release condition. A source/sole-caller gate and runtime tests must show
that no alternate path reaches wire or seed mutation around authorization. STOP_MODE is the durable
operator-facing halt; it is not used as a substitute for the reservation check.

### 3.4 Existing `reset_stop_mode` extension

The current `reset_stop_mode(pool, fiscal_number, reason)` is the only release surface. Its contract
is strengthened, not duplicated:

- if no CS-3 PENDING reservation and no CS-3 NotFound-RMR marker exists, retain current behavior;
- if a CS-3 PENDING row exists, a plain reset fails closed;
- the operator supplies one of the existing semantic resolutions through this command:
  accepted with the observed non-empty fiscal number; not accepted; or corrected chain seed for
  `MacReseedPending`;
- the transaction re-reads the reservation, document, node mode, and current generation;
- accepted resolution stamps the supplied fiscal number and marks APPLIED; it advances the seed from
  the document's immutable `unsigned_xml_sha256` only for online-origin, while offline-origin proves
  the already locally advanced chain and never rewrites the seed backwards;
- not-accepted resolution moves the document to its existing manual/terminal state without advancing
  the seed and marks APPLIED;
- MAC resolution writes the operator-confirmed seed through the existing
  `node_state::update_last_known_xml_sha_tx`, marks the document manual, and marks APPLIED;
- the same transaction performs `STOP_MODE -> GOING_ONLINE` and appends a Critical audit row;
- any stale generation, wrong effect, missing document, invalid seed length, CAS miss, or forbidden
  document transition rolls back the entire operation.

The same existing admin operation also gains a distinct guarded branch for
`BLOCKED -> GOING_ONLINE`. It is accepted only when the latest cause is the applied `Offline168`
effect, no active reservation exists, the operator records why the 168-hour condition is now cleared,
and the return-online path will revalidate before ONLINE. A plain STOP reset cannot clear BLOCKED, and
a plain BLOCKED reset cannot bypass these checks. This closes the pre-existing `-11` terminal-mode
brick without adding a mode or a command.

The one missing live document edge is added to the existing whitelist:

```text
Sending -> RequiresManualReconciliation
```

It is callable only from the operator completion branch for a document that has a matching
CALL_STARTED/PENDING reservation. It is not a resend or a new state.

The complete origin × document-family shift/seed matrix is:

| Origin / document | Accepted resolution | Not-accepted resolution |
|---|---|---|
| online regular | stamp F; advance seed to this doc hash | doc `Sending -> RMR`; seed unchanged |
| online SHIFT_OPEN | stamp F; seed advance; existing `Opening -> Opened` | doc `Sending -> RMR`; narrow `Opening -> Closed` rollback; seed unchanged |
| online close/Z | stamp F; seed advance; existing `Closing -> Closed` | doc `Sending -> RMR`; existing `Closing -> Opened`; seed unchanged |
| offline regular | stamp F; **no seed write**; validate existing offline chain ownership | doc `Sending -> RMR`; cancel later OLA successors or refuse; install operator-confirmed predecessor seed |
| offline SHIFT_OPEN | stamp F; no seed write; existing `OLPD -> Opened` when cohort permits | doc `Sending -> RMR`; cancel dependent OLA cohort; narrow `OLPD -> Closed` rollback; install confirmed predecessor seed |
| offline close/Z | stamp F; no seed write; existing `CLPD -> Closed` when cohort permits | doc `Sending -> RMR`; cancel dependent OLA cohort; narrow `CLPD -> OLPD` rollback; install confirmed predecessor seed |

`OLPD` and `CLPD` mean the existing `OpenedLocalPendingDrain` and
`ClosingLocalPendingDrain` states. The three rollback additions
`Opening -> Closed`, `OLPD -> Closed`, and `CLPD -> OLPD` are narrowly authorized only by this
operator completion. They add no state. `Created` is deliberately not a rollback target:
`stage_acquire` treats it as mid-transition and would still refuse a new SHIFT_OPEN. Without the
terminal/open rollback targets above, resetting STOP would expose an FN whose shift remains
permanently unusable.

For an offline-origin document, `not accepted` is allowed only if the transaction also proves there
is no later nonterminal offline successor, or atomically cancels every such successor through the
already existing `OfflineLocalAck -> Cancelled` edge. Skipping a locally chained predecessor while a
successor remains live would make that successor a fork candidate. The corrected seed is installed
through the existing tx-bound node-state update in the same completion transaction; partial cleanup
is forbidden.

The CLI shape may add flags to the existing command, but no database entity or domain authority token
is introduced. Operator evidence is trusted administrative input and is always durable in the
existing audit log.

The exact-match `last_chk_probe` path remains preferred when an expected server id is already durable:
only `ack.id == expected_id` proves attribution
(`services/reconciliation/last_chk_probe.rs:85-112`). NotFound, mismatch, or an FN-level observation
does not auto-release a reservation.

### 3.5 `Sent + NotFound`

Both live producers are retargeted:

- boot/convergence shared path in `services/reconciliation/boot_phase.rs`;
- offline drain path in `services/write_path/kvt2_confirm.rs`.

They call one tx-bound helper inside one existing `with_immediate`:

```text
document Sent -> RequiresManualReconciliation
node_state.mode -> STOP_MODE
transport_trace complete
audit_log append
```

The helper must use tx-bound repository functions only. Calling a pool-bound escalation after the
document CAS is forbidden because it creates a crash window. The operation does not use
`shifts::force_to_manual_reconciliation_with_audit`: that function does not update the node mirror and
shift-RMR has no exit.

---

## 4. P4 — durable, total evidence

### 4.1 Existing discriminant, now payload-carrying

The already planned `EvidenceDiscriminant` is the durable input sum. No second evidence type is added.
Its leaves correspond one-for-one to existing `SubmissionEvidence` / `SendResponse` outcomes:

```rust
EvidenceDiscriminant::{
    PreconditionFailed,
    SigningFailed,
    NoResponse { cause },
    RemoteAuthStatus { digest },
    Accepted { fiscal_number },
    Rejected { verdict, digest },
    UnknownStatus { raw_code, digest },
    SaveError { digest },
    CloseAmbiguous { digest },
    MissingStatus { digest },
    OkButNoFiscalNumber { digest },
}
```

`SaveError` and `MissingStatus` carry their existing decoded-response digest. `CloseAmbiguous`
remains one leaf because live `from_server_code` currently collapses `-2/-15` for close/Z documents
(`prro-domain/src/delivery/mod.rs:571-611`) and both have the same ProbeRequired apply.

`ObservedOutcomeV1::record` receives the sealed classified outcome and this matching discriminant.
It rejects a leaf whose axes, generation, routing, or node effect do not match.

### 4.2 Storage union

Migration 035 adds four nullable columns to the existing table:

```sql
ALTER TABLE delivery_reservation ADD COLUMN evidence_kind TEXT;
ALTER TABLE delivery_reservation ADD COLUMN evidence_text TEXT;
ALTER TABLE delivery_reservation ADD COLUMN evidence_code INTEGER;
ALTER TABLE delivery_reservation ADD COLUMN evidence_digest BLOB;
```

`evidence_text` is context-specific: exact accepted fiscal number, `DpsReject` name, or
`NoResponseCause`. It is not stored in `remote_correlation_id`; that field has a different meaning
and its bounded representation is not the lossless authority for Accepted.

`evidence_code` is used only by `UnknownStatus`. `evidence_digest` is used by every existing
digest-bearing response and must be a 32-byte SQLite BLOB.

The fail-closed matrix is:

| Leaf | Certainty / provenance | Routing / node effect | Payload |
|---|---|---|---|
| PreconditionFailed | NOT_SUBMITTED / NO_RESPONSE | TransientRetry / NoNodeEffect | all NULL |
| SigningFailed | NOT_SUBMITTED / NO_RESPONSE | WrapperBug / WrapperBug | all NULL |
| NoResponse | SUBMITTED_UNKNOWN / NO_RESPONSE | TransientRetry / NoNodeEffect | text = an existing `NoResponseCause` |
| RemoteAuthStatus | SUBMITTED_UNKNOWN / AUTHENTICATED_PEER | ProbeRequired / ProbeRequired | digest length 32 |
| Accepted | SUBMITTED / PARSED_DPS_ENVELOPE | NULL / NoNodeEffect | non-empty exact fiscal number; `remote_correlation_id = evidence_text` |
| Rejected | SUBMITTED / PARSED_DPS_ENVELOPE | verdict-derived / verdict-derived | text = closed `DpsReject`, digest length 32 |
| UnknownStatus | SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE | TransientRetry / NoNodeEffect | raw code outside named/0/1 set, digest length 32 |
| SaveError | SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE | TransientRetry / NoNodeEffect | digest length 32 |
| CloseAmbiguous | SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE | ProbeRequired / ProbeRequired | digest length 32 |
| MissingStatus | SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE | ProbeRequired / ProbeRequired | digest length 32 |
| OkButNoFiscalNumber | SUBMITTED_UNKNOWN / PARSED_DPS_ENVELOPE | ProbeRequired / ProbeRequired | digest length 32 |

For `Rejected`, the trigger additionally enforces the exact live mapping:

- Verify, Type, Xml, XmlDate, XmlChk, XmlZReport, OfflineId, Close:
  `TerminalReject / NoNodeEffect`;
- NotPrevZReport: `OperatorEscalation / OperatorEscalation`;
- Offline168: `TerminalReject / NodeBlocked`;
- BadHashPrev: `MacRecovery / MacReseedPending`;
- NotRegisteredRro and NotRegisteredSigner: `FnConfigError / FnConfigError`.

Before `OUTCOME_OBSERVED`, all four evidence columns are NULL. At `OUTCOME_OBSERVED`, exactly one
matrix row must match. The INSERT and UPDATE triggers use:

```sql
WHEN COALESCE((CASE ... ELSE 0 END), 0) <> 1
```

not `WHEN NOT(predicate)`, because SQLite NULL would otherwise bypass the trigger. A separate trigger
freezes all four evidence columns after OUTCOME_OBSERVED using null-safe `IS NOT`.

Every non-Accepted leaf requires `remote_correlation_id IS NULL`. Accepted keeps the existing
correlation field, but it must be byte-equal to the lossless `evidence_text`; a value that cannot be
represented without truncation fails closed rather than creating two disagreeing authorities.

### 4.3 Record then apply

The post-wire flow is:

1. wire I/O completes outside a SQLite write transaction;
2. record transaction writes evidence union, axes, authorized generation, node effect,
   `PENDING_APPLY`, transport trace, and audit;
3. if the row is unresolved (rows 6–8), the same transaction also sets existing STOP_MODE and ends;
4. otherwise a separate repeatable apply transaction re-reads the reservation and document, checks
   `{reservation_id, authorized_generation, binding, envelope_hash}`, performs the ledger/seed/node
   effect, marks APPLIED, and clears the active pointer;
5. a crash at any boundary leaves either CALL_STARTED or PENDING evidence; boot performs no wire call.

On boot, a durable CALL_STARTED row with no outcome is converted once to the existing
`NoResponse { CrashedBeforeObservation }` leaf, `PENDING_APPLY`, and STOP_MODE. This is a local recovery
write, not a synthetic DPS response and not a resend.

The document row supplies existing immutable apply inputs: origin (`offline_fiscal_no`), `doc_type`,
shift id, previous hash, unsigned XML hash, closing cash, and live/offline ownership. Accepted fiscal
number and verdict/digest come from the reservation evidence.

Diagnostics needed only for forensic trace are completed in the record transaction from the live
`WireDiagnostics`; they are not reclassified as authority. No claim is made that a digest reconstructs
the old `DpsError` message or `MacRecoveryHint`.

### 4.4 Boot hydration

Boot selects all four evidence slots for every PENDING row, parses exact strings, validates 32-byte
digests and non-empty Accepted F, reconstructs the existing `ObservedOutcomeV1`, and rechecks the
leaf-to-axes/generation matrix in Rust. Unknown tags, unknown reject names, malformed digests, or
extraneous payload fail loud and leave the fence/STOP unchanged.

Hydration adds narrow constructors/methods to existing opaque payload types; it does not add a
parallel public evidence algebra. Transport mint constructors remain transport-only. The total
boot result is either a deterministic apply plan, a deliberate manual PENDING/STOP hold, or a
fail-loud corrupt-row error; “total” does not mean fabricating an automatic result for ambiguous
evidence.

---

## 5. Migration 035 contract

Migration name:

```text
035_delivery_reservation_call_once_and_evidence.sql
```

It performs, in order:

1. a fail-fast TEMP-table guard requiring `delivery_reservation` row count zero, matching migration
   033's activation posture;
2. the four additive evidence columns;
3. `ux_delivery_document_ever_started`;
4. rebuild of `ux_reservation_active` with §3.1's predicate;
5. rebuild of `delivery_reservation_no_replace` with the byte-identical FN predicate **and** the
   historical-document-started rejection from §2;
6. fail-closed evidence matrix triggers for INSERT and UPDATE;
7. evidence immutability trigger.

The empty-table guard is mandatory because D/E and migration 035 land in one production release.
Migration application with any reservation row aborts transactionally and is not recorded in
`_sqlx_migrations`.

The call-once index is DDL authority; the pre-wire NOT-EXISTS is the zero-wire early refusal.
`get_active_for_fn` and authorization SQL must use exactly §3.1. A consistency test extracts and
compares all four copies structurally; comments are not parsed.

---

## 6. Oracle/spec edit map

| Existing document | Required correction |
|---|---|
| Spec #2 delivery reservation FSM | Add lifetime call-once; replace active predicate with §3.1; make PENDING the only observed hold; describe STOP/operator completion. |
| Spec #1 transition contract | Replace both `Sent + NotFound -> ErrorRetryable` rows with the atomic doc-RMR + node-STOP transaction; delete issued-doc redrive language. |
| Spec #4A / CS-3 keystone | Make the existing discriminant payload-carrying and durable; assign record, hydration, and operator completion; retire live `-12 Resigned => continue`. |
| Spec #4B DPS contract | Add authorize NOT-EXISTS; state that a started ambiguous attempt is never rewired; retain the current single CloseAmbiguous leaf. |
| Bridge D/E dossier | Replace permanent routing fences with PENDING+STOP; add exact storage matrix, boot hydration, reset-stop guard, and call-once index. |

No locked spec may continue to claim automatic resend for a document that crossed CALL_STARTED.

---

## 7. RED-first verification

Each guard must be broken locally and shown to make its named test fail.

| Pin | Required scenario and bite |
|---|---|
| Lifetime call-once | Two attempts for one document race authorization; exactly one commits CALL_STARTED and mock DPS sees at most one RPC. Removing either DB index or query guard must fail a separate test. |
| No orphan RN after started history | After attempt 1 is APPLIED, attempt 2 INSERT for the same document fails and leaves no row; an unrelated next document on the FN can still reserve. Removing the no-replace historical clause must fail. |
| Crash after marker | Commit CALL_STARTED, drop future before response, reopen DB; no path reaches submit_raw for that document. |
| Fence consistency | Index, no-replace trigger, repository query, and authorization use the exact §3.1 predicate. Dropping PENDING from any copy must fail. |
| Durable Accepted | record Accepted(F), close pool, reopen, hydrate, apply; SFN equals F and online seed advances once. Removing `evidence_text` must fail. |
| Full evidence round-trip | Every eleven leaf rows records, cold-reopens, hydrates, and derives the expected axes/effect. The test calls production `classify`/`record`, not a copied classifier. |
| SQL matrix tightness | For every leaf, delete one required payload, add one forbidden payload, or swap routing/effect; each mutation is rejected by SQLite. Include NULL-bypass cases. |
| Apply replay | Crash after record and after each apply write; cold replay produces one ledger/effect result, zero RPC, and APPLIED. Stale generation drops without ledger/seed/fence mutation. |
| SubmittedUnknown liveness | Timeout after marker -> evidence PENDING + STOP. Plain reset fails. Operator completion + reset commits atomically; next legitimate document can authorize; original document never rewires. |
| `-12/-6` liveness | Both enter PENDING+STOP. Plain reset fails. Correct operator completion releases PENDING; -12 seed correction uses existing tx-bound seed update; next doc extends the corrected/current seed. |
| `-11` liveness | APPLIED `Offline168` releases the reservation but remains BLOCKED. Plain reset fails; guarded operator resolution CASes BLOCKED→GOING_ONLINE only after the cause is recorded as cleared. |
| Operator matrix | Accepted/rejected regular, shift-open, shift-close/Z, and online/offline origins each exercise the real document + shift whitelists. Offline Accepted performs zero seed writes. Rejected shift documents exercise `Opening->Closed`, `OLPD->Closed`, or `CLPD->OLPD`; a mutation to `Created` must remain refused. An offline rejection with a live successor refuses reset unless successors are atomically cancelled/reconciled. |
| Sent+NotFound atomicity | Boot and offline variants produce doc RMR + STOP + trace/audit. Inject failure after each write; reopen sees either all old or all new state. Success followed by drain/ingress produces zero RPC until reset. |
| Clean-accept atomic release | Inject a failure before seed/SFN/APPLIED commit; row remains PENDING and next doc is refused. Successful apply exposes all effects and releases. |
| Legacy cutover | Reservation-less Sending/ErrorRetryable at activation fails closed before any wire; the pre-deploy empty-in-flight gate is the preferred production condition. |

Run the migration against SQLite with `foreign_keys=ON`, an empty 034 database, and a deliberately
non-empty 034 database. The first must migrate; the second must abort with no partial objects.

---

## 8. Implementation order and gate

Minimal implementation sequence:

1. spec corrections and migration-035 tests;
2. evidence field + record/hydration using the existing algebra;
3. lifetime authorization and sole-caller wire gate;
4. repeatable apply and PENDING boot resume;
5. STOP/operator completion extension;
6. atomic `Sent+NotFound`;
7. whole-fence cutover and retirement of every blind-resend edge.

D and E still land in one production release. No network or crypto operation occurs inside
`BEGIN IMMEDIATE`.

**Design verdict after rev-3 correction:** `DESIGN_SOUND, IMPLEMENTATION NOT YET GATED`.

The design now gives an explicit defender for P2, P3, P4, and BRICK without a new table or domain
entity. Implementation is `NO-GO` until the §7 teeth are observed RED on guard removal and green on
the complete workspace gate.
