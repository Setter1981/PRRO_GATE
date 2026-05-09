# M3a W7 — Stage 4 Send (Pattern B with SENDING marker) — Design Freeze

**Date:** 2026-05-09
**Status:** Preview — pending GO before apply
**Anchors:** ADR-M3-A5 (Pattern B mandatory), ADR-M3-A6 (DpsError routing scaffold), ADR-M3-A9 step 5-6, W0-1 §3.4, W0-2 §2 row 4 + §9.4 fixture #2
**Predecessor:** W6 (PR #25, merged `c1acbfc`) — stage 3 sign (Pattern A) + W3 task_local guard
**Successor:** W8 (stage 5 finalize), W10 (full DpsError dispatch table)

---

## 1. Purpose & scope

Land the **stage 4 send** worker step on the write-path. Three execution segments under **two write locks** (Pattern B), with the wire `send_chk` call between the two locks. The committed `SENDING` marker is the safety mechanism that prevents duplicate fiscalisation if the process crashes mid-send: the App::boot recovery rule (W9) will refuse to re-call `send_chk` for a `SENDING` row precisely because the marker is already durable.

W7 carries **minimal but complete** classification: happy path, inline KVT1, terminal reject, transport-retryable. The **full table-driven DpsError → DocState dispatch** is W10. W7 must wire enough that the `with_immediate` lock topology, audit trail, and `transport_trace` schema are correct; W10 widens the codomain.

---

## 2. Plan corrections (vs `2026-05-07-m3a-implementation.md` Task 7)

The plan's original Task-7 wording has four mismatches with the actual codebase. Each is verified below.

### 2.1 Schema gap — `transport_trace` table does not exist

**Plan said:** "Add `rust/prro/src/db/repositories/transport_trace.rs` (new) — `insert(...)` helper."

**Reality:** Search of `migrations/*.sql` returns no match for `transport_trace`. Repo helper has no table to write to.

**Resolution:** Add **migration 010 `transport_trace`** as part of W7.1 (see §4.1).

### 2.2 Schema gap — `transport_request_id` column does not exist on `fiscal_documents`

**Plan said:** "transport_request_id set" inside 4b.

**Reality:** `migrations/008_doc_state_sending.sql` recreates `fiscal_documents`; the column set is the canonical one and includes `server_fiscal_no TEXT` (line 103). There is no `transport_request_id` column. `CheckAck.id` is documented (`dto.rs:67-68`) as "Server-assigned fiscal id" — semantically identical to `server_fiscal_no`.

**Resolution (minimal-diff):**
- Do **not** add a new column on `fiscal_documents` in W7.
- On 4b success, persist `CheckAck.id` into the existing `server_fiscal_no` column.
- All raw send metadata (request envelope hash, response blob, status code, transport timing, error variant if any) lives in the new `transport_trace` row keyed by `document_id` + monotonic `attempt_no`. This is the right boundary anyway — `fiscal_documents` is fiscal state of record, `transport_trace` is operational forensics.

### 2.3 Lock-topology wording — "3 distinct `with_immediate` opens"

**Plan said:** "3-segment lock structure (4-pre / 4a / 4b) verified via 3 distinct `with_immediate` opens per request (spy on `BEGIN IMMEDIATE` SQL events)."

**Reality:** Pattern B is **3 segments under 2 locks**:
- **4-pre** opens lock #1 (`with_immediate`) → CAS `Signed→Sending` + intent-marker work → commit/release
- **4a** runs **outside any lock** — wire `send_chk(envelope).await`
- **4b** opens lock #2 (`with_immediate`) → CAS `Sending→{Sent|Kvt1|Rejected|ErrorRetryable}` + persist trace → commit/release

There are exactly two `BEGIN IMMEDIATE` envelopes per stage 4 invocation, not three.

**Resolution:** Acceptance criterion rewritten as:
> Two distinct `with_immediate` opens per request, with the wire `send_chk` invocation observably ordered **after lock #1 commit** and **before lock #2 begin**. Spy proves `sending_commit_ts < send_chk_call_ts < kvt_persist_ts`.

### 2.4 Idempotency outcome — `Conflict`, not `Forbidden`

**Plan said:** "re-running stage 4 on already-Sent doc is a `Forbidden` outcome (CAS short-circuits via whitelist)."

**Reality:** `(Signed, Sending)` IS in the whitelist (`fiscal_documents.rs:158`). The whitelist short-circuit therefore **does not fire**. Calling `transition_state(Signed, Sending)` on a doc whose actual state is `Sent` runs the SQL CAS, misses (0 rows affected), the disambiguation `SELECT` finds the row, and the call returns `TransitionOutcome::Conflict`.

**Resolution:** Acceptance criterion rewritten as:
> Re-running stage 4 on a `Sent` document yields `TransitionOutcome::Conflict` from 4-pre's CAS, with **zero** `send_chk` invocations and a typed `SendOutcome::StateConflict` returned to the worker dispatch layer. `Forbidden` would require a malformed transition and is treated as a defect (panic in debug, error log + escalate in release).

---

## 3. Schema — migration 010

```sql
-- migrations/010_transport_trace.sql

-- Operational forensics for outgoing wire calls. One row per send_chk
-- attempt, immutable post-insert. Ordered by (document_id, attempt_no);
-- attempt_no is monotonic per document_id, assigned inside the same
-- with_immediate envelope that performs the 4b CAS.
--
-- Why a separate table (vs columns on fiscal_documents):
--  - fiscal_documents is fiscal state of record; trace is forensic.
--  - One document can have multiple attempts (Pattern B retry path:
--    Sending → ErrorRetryable → Sending → ...); fiscal_documents row
--    is unique per document_id and cannot carry an attempt list.
--  - Keeps fiscal_documents narrow; trace can grow unbounded under
--    pathological retry scenarios without bloating hot-path SELECTs.

CREATE TABLE transport_trace (
    document_id            INTEGER NOT NULL
        REFERENCES fiscal_documents(document_id) ON DELETE CASCADE,
    attempt_no             INTEGER NOT NULL,           -- 1-based per document
    started_at             TEXT    NOT NULL,           -- 4-pre commit timestamp (= sending_commit_ts)
    completed_at           TEXT    NOT NULL,           -- 4b commit timestamp
    wire_call_started_at   TEXT    NOT NULL,           -- send_chk(...) Instant snapshot, ISO-8601
    wire_call_finished_at  TEXT    NOT NULL,           -- send_chk(...) return Instant snapshot, ISO-8601
    backend_profile_id     TEXT    NOT NULL,           -- snapshot from fiscal_documents
    transport_profile_id   TEXT    NOT NULL,           -- snapshot from fiscal_documents
    request_envelope_sha256 BLOB   NOT NULL CHECK (length(request_envelope_sha256) = 32),
                                                       -- sha256 of the FULL CheckEnvelope wire form (every field: rro_fn,
                                                       -- date_time, check_sign, local_number, check_type, id_offline,
                                                       -- id_cancel).  Hashing only check_sign would miss drift in the
                                                       -- non-CMS fields between retry attempts.
    outcome_kind           TEXT    NOT NULL,           -- 'OK' | 'OK_KVT1' | 'REJECTED' | 'RETRYABLE_TRANSPORT' | 'RETRYABLE_SERVER'
    server_fiscal_no       TEXT,                       -- non-NULL on OK / OK_KVT1
    server_status_code     INTEGER,                    -- non-NULL on REJECTED / RETRYABLE_SERVER
    error_kind             TEXT,                       -- DpsError variant name on non-OK; NULL on OK*
    error_message          TEXT,                       -- truncated to 512 chars
    PRIMARY KEY (document_id, attempt_no)
) STRICT;

CREATE INDEX ix_transport_trace_started ON transport_trace(started_at);
```

**Determinism note.** `attempt_no` is computed as `MAX(attempt_no)+1 OR 1` inside lock #1. This means **4-pre allocates the row identity**; 4b updates the same row with `completed_at`, `wire_call_*`, `outcome_kind`, etc. Two `INSERT`s would conflict if the lock #1 transaction is rolled back; one `INSERT` (4-pre) + one `UPDATE` (4b) keeps trace and fiscal state consistent under crash-between-locks.

**Open question for sign-off:** insert in 4-pre vs insert in 4b?
- **Option A (chosen):** 4-pre INSERTs the row with `wire_call_*` and `completed_at` left NULL; 4b UPDATEs them. Crash between locks → trace row exists with NULL `completed_at`, App::boot can detect and forensically classify.
- **Option B:** 4b INSERTs the row in one go. Crash between locks → no trace at all; only `submission_attempted_at` on `fiscal_documents` survives.

Option A is recommended (better forensics on crash-between-locks) but requires `completed_at`, `wire_call_*`, `outcome_kind` to be **nullable** in the DDL above. Will adjust the DDL to `NULL`-allow those four if Option A is approved.

---

## 4. Decomposition (5 sub-units, applied sequentially)

### 4.1 W7.1 — Migration 010 + `transport_trace` repo

- `migrations/010_transport_trace.sql` (per §3 above, with Option A NULL-tolerant columns if approved).
- `rust/prro/src/db/repositories/transport_trace.rs`:
  - `pub struct NewAttempt { … }` — fields needed at 4-pre time (started_at, profiles, request_envelope_sha256, attempt_no resolution).
  - `pub async fn allocate_and_insert_tx(tx: &mut WriteTxConn<'_>, doc: DocumentId, init: NewAttempt) -> sqlx::Result<i32>` — returns assigned `attempt_no`.
  - `pub async fn complete_tx(tx: &mut WriteTxConn<'_>, doc: DocumentId, attempt_no: i32, completion: AttemptCompletion) -> sqlx::Result<()>` — UPDATE row with 4b results.
  - Register in `db/repositories/mod.rs`.
- Sqlx prepare regen for the new queries.
- **Verify:** `cargo check -p prro` clean; migration runs in test fixture.

### 4.2 W7.2 — `fiscal_documents` send-input/result helpers

- Add to `db/repositories/fiscal_documents.rs`:
  - `pub async fn fetch_send_inputs_tx(tx: &mut WriteTxConn<'_>, doc: DocumentId) -> sqlx::Result<Option<SendInputs>>` — returns the columns needed for envelope construction (`fiscal_number`, `lnd`, `doc_type`, `business_ts`, `state`, `submission_attempted_at`). Read inside lock #1, post-CAS.
  - `pub async fn mark_submission_attempted_tx(tx: &mut WriteTxConn<'_>, doc: DocumentId, ts: &str) -> sqlx::Result<()>` — `UPDATE fiscal_documents SET submission_attempted_at = ? WHERE document_id = ?`. Single-row UPDATE under lock #1.
  - `pub async fn set_server_fiscal_no_tx(tx: &mut WriteTxConn<'_>, doc: DocumentId, fiscal_no: &str) -> sqlx::Result<()>` — UPDATE under lock #2 on success branches.
- No new columns on `fiscal_documents`.
- **Verify:** `cargo test -p prro` existing tests still green.

### 4.3 W7.3 — `classify_send_outcome` + envelope builder

- New helpers inside `services/write_path/stage_send.rs` (no separate module):

```rust
pub(crate) enum SendOutcome {
    Sent,                                     // OK, no inline KVT1
    Kvt1 { kvt1_payload: Vec<u8> },           // OK with inline KVT1 piggyback (W7 stub: never produced; surface kept for W8/W10)
    Rejected { code: i32, message: String },  // Authorization{ DocumentReject } | Server with terminal-reject codes
    Retryable { reason: RetryableReason },    // Transport | Server transient | Authorization{ FiscalNumberNotRegistered }
    StateConflict,                            // 4-pre CAS missed (re-entry on already-Sent / -ErrorRetryable / -...)
}

pub(crate) enum RetryableReason {
    Transport(String),
    Server { code: i32, message: String },
    AuthorizationFnNotRegistered { code: i32, message: String },
}

pub(crate) fn classify_send_outcome(r: Result<CheckAck, DpsError>) -> SendOutcome { … }
```

- W7 minimal table (full W10 superset documented as a comment):
  - `Ok(ack)` → `SendOutcome::Sent` (W7 ignores any KVT1 piggyback discovery — `SendOutcome::Kvt1` is reserved for W8 wiring).
  - `Err(DpsError::Transport(_))` → `Retryable::Transport`.
  - `Err(DpsError::Server { code, message })` → `Retryable::Server` for transient codes; **W7 retains a TODO list** (per W0-3 §2 dispatch matrix, comment-anchored) of which `Server { code }` values are terminal vs transient. W7 conservatively classifies all `Server` errors as `Retryable::Server`; W10 lands the precise table.
  - `Err(DpsError::Authorization { kind: DocumentReject, code, message })` → `SendOutcome::Rejected`.
  - `Err(DpsError::Authorization { kind: FiscalNumberNotRegistered, code, message })` → `Retryable::AuthorizationFnNotRegistered` (will route through `ErrorRetryable → RequiresManualReconciliation` in W10; W7 just lands at `ErrorRetryable`).
  - `Err(DpsError::Decode(_) | DpsError::ServerFiscalIdMismatch{..} | DpsError::NotFound | DpsError::QueryNotSupported(_) | DpsError::Internal(_))` → **panic in debug** (these cannot occur for `send_chk`'s success/failure shape) / log+`Retryable::Transport` in release.

- Envelope builder:

```rust
fn build_send_envelope(
    inputs: &SendInputs,            // from fetch_send_inputs_tx
    signed_payload: &[u8],          // from document_files SIGNED kind
) -> CheckEnvelope { … }
```

  - `check_type` derived from `derive_wire_artifact_kind(inputs.doc_type)` per W6 helper, mapped:
    - `WireArtifactKind::ShiftOpen` → `DpsCheckType::ServiceChk`
    - `WireArtifactKind::Sell | Return` → `DpsCheckType::Chk`
    - `WireArtifactKind::ZReport` → `DpsCheckType::ZReport` (covers both `SHIFT_CLOSE` and `Z_REPORT` internal labels — derived through the existing W6 helper, NOT keyed on `DocType`).
  - **`local_number` rule (proven in Sprint 7 Python `dps_fiscal_server.py:190`):**
    - `WireArtifactKind::ShiftOpen` → `local_number = 0` regardless of `inputs.lnd`.
    - All other kinds → `local_number = inputs.lnd`.
  - `id_offline` / `id_cancel` empty for the W7 happy path; will be wired in W11 (offline) and a future cancel slice.
  - `date_time` derived from `inputs.business_ts` via the Kyiv-local-as-epoch convention already documented on `CheckEnvelope.date_time` (`dto.rs:35-42`). W7 will reuse the existing helper if one exists; otherwise small adapter.

- **Unsupported doc type path (fail-closed before intent marker):** if `derive_wire_artifact_kind` errors, stage_send returns the error from **inside lock #1 BEFORE** the CAS — no `Sending` marker is written, no audit `STAGE_SEND_INTENT_MARKED` entry. `submission_attempted_at` is also not touched. The doc stays in `Signed` and the worker surfaces `WorkerProcessResult::Failed { stage: 4, kind: UnsupportedDocType }` (or equivalent) for the dispatch layer.

### 4.4 W7.4 — `stage_send.rs` (the worker step)

```
pub async fn run(
    ctx: &WorkerContext,
    doc: DocumentId,
) -> Result<StageSendOutcome, StageSendError> {
    // 4-pre under lock #1
    let pre = with_immediate(&ctx.pool, |tx| async move {
        let inputs = fiscal_documents::fetch_send_inputs_tx(tx, doc).await?
            .ok_or(StageSendError::DocumentMissing)?;
        match fiscal_documents::transition_state(tx, doc, DocState::Signed, DocState::Sending).await? {
            TransitionOutcome::Applied => { /* fall through */ }
            TransitionOutcome::Conflict => return Ok(PreOutcome::StateConflict { current: inputs.state }),
            TransitionOutcome::NotFound => return Err(StageSendError::DocumentMissing),
            TransitionOutcome::Forbidden => unreachable!("(Signed,Sending) is in the whitelist"),
        }
        let ts = ctx.clock.now_iso8601();
        fiscal_documents::mark_submission_attempted_tx(tx, doc, &ts).await?;
        let signed = document_files::get_tx(tx, doc, DocumentFileKind::SignedXml).await?
            .ok_or(StageSendError::SignedArtifactMissing)?;
        let envelope = build_send_envelope(&inputs, &signed.content)?;
        let attempt_no = transport_trace::allocate_and_insert_tx(tx, doc, NewAttempt {
            started_at: ts.clone(),
            backend_profile_id: inputs.backend_profile_id.clone(),
            transport_profile_id: inputs.transport_profile_id.clone(),
            request_envelope_sha256: sha256(&envelope.check_sign),
        }).await?;
        audit_log::append_tx(tx, EntityType::Document, &doc, "STAGE_SEND_INTENT_MARKED", …).await?;
        Ok(PreOutcome::Marked { envelope, inputs, attempt_no, sending_commit_ts: ts })
    }).await?;

    let (envelope, inputs, attempt_no, sending_commit_ts) = match pre {
        PreOutcome::Marked { envelope, inputs, attempt_no, sending_commit_ts } => (envelope, inputs, attempt_no, sending_commit_ts),
        PreOutcome::StateConflict { current } => return Ok(StageSendOutcome::StateConflict { current }),
    };

    // 4a — outside any lock
    let wire_started_at = ctx.clock.now_iso8601();
    let wire_result = ctx.dps_channel.send_chk(envelope.clone()).await;
    let wire_finished_at = ctx.clock.now_iso8601();

    let outcome = classify_send_outcome(wire_result);

    // 4b under lock #2
    with_immediate(&ctx.pool, |tx| async move {
        // CAS Sending → target per outcome
        // server_fiscal_no UPDATE on Sent / Kvt1 paths
        // transport_trace::complete_tx(...) with wire timestamps + outcome_kind
        // audit_log::append_tx with STAGE_SEND_RESULT
        Ok(())
    }).await?;

    Ok(StageSendOutcome::from(outcome))
}
```

- **`SendInputs`** carries everything 4a needs so 4-pre's lock can release cleanly. Wire envelope is **built inside** lock #1 using freshly-read `signed_payload`; it crosses the await boundary by `Clone`.
- **`signed_payload` typing.** `document_files::get_tx` already returns bytes (W6). The `WorkerContext`/`TransportClient` `str` annotation noted in memory (`feedback_signed_payload_typing.md`) is in a different layer and not on the critical path here; W7 will use bytes throughout the new code, no migration of existing types attempted.
- **Clock seam.** `WorkerContext` exposes a clock; W7 uses it for both `submission_attempted_at` and `wire_call_*` so tests can produce deterministic ordering proofs without relying on monotonic system clock.

### 4.5 W7.5 — Tests (`tests/write_path_stage4_send.rs`)

Seven fixtures, all driven through the W6-style harness with a stub `DpsChannel` impl:

| # | Name | Outcome | Verifies |
|---|---|---|---|
| 1 | `happy_sent` | `Sending → Sent` | CAS chain, `server_fiscal_no` set, `transport_trace.outcome_kind = 'OK'` |
| 2 | `kvt1_inline_stub` | `Sending → Kvt1` (artificial via classify shim) | Branch reachability for W8; `kvt1_payload` round-trip |
| 3 | `terminal_reject` | DPS returns `Authorization{DocumentReject, code=-1}` → `Sending → Rejected` | `outcome_kind = 'REJECTED'`, audit reason recorded |
| 4 | `transport_retryable` | DPS channel returns `Transport(...)` → `Sending → ErrorRetryable` | `outcome_kind = 'RETRYABLE_TRANSPORT'`, doc retains `submission_attempted_at` |
| 5 | `pattern_b_ordering` | Spy on stub channel + clock | `sending_commit_ts < wire_call_started_at < wire_call_finished_at < kvt_persist_ts` per W0-2 §9.4 fixture #2 |
| 6 | `rerun_on_sent_is_state_conflict` | Pre-seed doc as `Sent`, call stage 4 again | `StageSendOutcome::StateConflict { current: Sent }`; spy proves **0** `send_chk` calls |
| 7 | `with_immediate_scanner_green` | Static scan from W3 (`tests/with_immediate_no_foreign_io.rs` denylist) | Stage 4's `send_chk` site is OUTSIDE any `with_immediate`; no panic from W3 runtime guard |

Plus reuse of the W6 harness fixtures around `WorkerContext` and the in-memory pool with auto-migrated 010.

**Verify:** `cargo test -p prro --test write_path_stage4_send` — 7 fixtures green; `cargo test -p prro --test with_immediate_no_foreign_io` — still green.

---

## 5. Invariants asserted by W7

| # | Invariant | How asserted |
|---|---|---|
| I1 | Pattern B mandatory: SENDING marker durable BEFORE wire send | Fixture 5; W3 scanner over `stage_send.rs` |
| I2 | `(Signed, Sending)` and the four `(Sending, X)` whitelist entries already in W1 are sufficient — no schema change | §3 (no migration to allowed_transition) |
| I3 | DPS retry path: `ErrorRetryable → Sending → wire`, never `ErrorRetryable → Sent` direct | W7 only lands `Signed → Sending` for the entry case; the retry-from-`ErrorRetryable` entry case is W10's concern. W7's `(Sending, ErrorRetryable)` write does NOT short-circuit to `Sent` on next run — it goes via the `ErrorRetryable → Sending` whitelist entry on the next worker tick. |
| I4 | No network call inside `with_immediate` | W3 task_local guard + static scan; stage_send's `send_chk` invocation is at module top-level between two `with_immediate` blocks |
| I5 | One `fiscal_number` = one logical writer | W5 acquire stage owns this; W7 inherits unchanged |
| I6 | All canonical envelopes carry `schema_version` | W6's `build_canonical_doc` sets it; W7 only routes the already-signed payload |
| I7 | Idempotency: re-entry on `Sent` is safe | Fixture 6: `TransitionOutcome::Conflict` short-circuits 4-pre; zero wire calls |

---

## 6. Out of scope (explicitly deferred)

- **Full DpsError → DocState dispatch table** with terminal-vs-transient `Server { code }` resolution → **W10**.
- **App::boot SENDING reconciliation** (the `Sending → ErrorRetryable` recovery rule for crash-between-locks) → **W9**.
- **KVT1 inline production path** (envelope-side handling of piggybacked KVT1 from DPS) → **W8** (W7 only carries the SendOutcome variant, never produces it from real DPS responses).
- **Cancel / id_offline wiring** → future slices (W11 offline; cancel TBD).
- **Outbox notification** post-Sent → **W8**.
- **Type alignment of `signed_payload` across `WorkerContext`/`TransportClient` `str` annotations** (memory `feedback_signed_payload_typing.md`) → cleanup task, not on W7 critical path.

---

## 7. Open questions for sign-off

1. **Trace insert mode:** Option A (4-pre INSERT + 4b UPDATE, NULL-tolerant trace columns, better crash forensics) vs Option B (4b INSERT only). Recommend A.
2. **Server-error transient/terminal split:** W7 conservatively treats all `DpsError::Server { code }` as `Retryable::Server`. Acceptable to defer the per-code split to W10? (Risk: W7 fixtures will not exercise terminal `Server { code }` rejection — that path enters at W10 with a real dispatch table.)
3. **`KVT1` synthesis in fixture 2:** since W7 does not produce `Kvt1` from the real classify path, the fixture either (a) uses a test-only `classify_send_outcome` shim, or (b) skips and `Kvt1` reachability is proved instead in W8. Recommend (b) — fewer test-only seams.
4. **Clock seam in `WorkerContext`:** does the W6 `WorkerContext` already expose a mockable `now_iso8601()` or do we add one? (If the latter: tiny additive change in `services/write_path/types.rs`, but worth flagging.)

---

## 8. Apply order (post-GO)

1. Migration 010 + repo helper (W7.1) — small, isolated, self-tested by migration runner.
2. fiscal_documents helpers (W7.2).
3. `classify_send_outcome` + envelope builder (W7.3) — pure functions, unit-testable in-place.
4. `stage_send.rs` skeleton (W7.4) wired through W6 harness; ensure `mod stage_send` registered in `write_path/mod.rs`.
5. Test fixtures (W7.5) — incrementally: 1 → 6 → 5 → 4 → 3 → 2 (drop) → 7.
6. Final pass: `cargo test -p prro`, `cargo test -p prro --test with_immediate_no_foreign_io`, sqlx prepare regen, `cargo clippy -p prro --tests`.

Estimated 4–5 days, in line with plan.

---

## 9. Verify hooks

- `cargo test -p prro --test write_path_stage4_send` — 7 fixtures.
- `cargo test -p prro --test with_immediate_no_foreign_io` — W3 static scan still green.
- `cargo test -p prro` — full crate suite remains green.
- Migration runner over fresh DB: `010_transport_trace.sql` applied, FK to `fiscal_documents` valid.
