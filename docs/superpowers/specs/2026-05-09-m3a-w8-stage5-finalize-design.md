# M3a W8 — Stage 5 Finalize (Kvt2 → Ack) — Design Freeze

**Date:** 2026-05-09
**Status:** Preview — pending GO before apply
**Anchors:** ADR-M3-A8 (KVT2 forward-only; Ack as terminal-success), W0-1 §3.5, W0-2 §2 row 5
**Predecessor:** W7 (PR #26, merged `d97178e`) — stage 4 send (Pattern B with SENDING marker)
**Successor:** W10 (full DpsError dispatch incl. Sent→Kvt1→Kvt2 quittance shape if landed there), W9 (App::boot reconciliation)

---

## 1. Purpose & scope

W8 lands the **terminal-success bookkeeping** step.  Single `with_immediate` envelope around five atomic writes:

1. CAS `Kvt2 → Ack` on `fiscal_documents`.
2. UPDATE `node_state.last_known_unsigned_xml_sha256` ← doc's `unsigned_xml_sha256` (advances next-doc MAC chain seed).
3. UPDATE `ingress_inbox.status = 'DONE'`.
4. INSERT `outbox` row (M3a stub schema; cross-process publish is post-commit, out of scope).
5. INSERT `audit_log` `STAGE_FINALIZE_ACK`.

W8 is the **shortest** stage of M3a — small surface, but high-risk because it touches terminal state, the cross-doc MAC chain seed, and the outbox publishing seam.

---

## 2. Risk-driven design (per pre-apply review)

### R1 — Don't advance `node_state.last_known_unsigned_xml_sha256` before final Ack
**Mitigation.**  Seed UPDATE lives **inside** the same `with_immediate` envelope as the CAS `Kvt2 → Ack`.  If the CAS returns `Conflict` (doc not in Kvt2), the closure short-circuits with `StageFinalizeOutcome::StateConflict` BEFORE the seed UPDATE runs.  If the seed UPDATE itself fails (DB error / row missing), the entire tx rolls back via `?`-propagation — CAS Applied is undone.  Atomicity by construction.

**Test (fixture b).**  Pre-seed doc in `Kvt2` with `unsigned_xml_sha256 = 0xAB...`; `node_state.last_known_unsigned_xml_sha256 = 0xCD...` (different).  Run `stage_finalize::run`.  Post: doc state == `Ack`, `node_state.last_known_unsigned_xml_sha256 == 0xAB...`.  Negative: pre-seed doc in `Kvt1` (not Kvt2); run.  Post: state stays `Kvt1`, seed stays `0xCD...` — atomic rollback proven.

### R2 — Don't lose KVT raw bytes
**Mitigation.**  W8 does NOT touch `document_files`.  KVT raw bytes (`KVT1_RAW`, `KVT2_RAW`) are written wherever the Sent → Kvt1 → Kvt2 transitions land (W10 dispatch or a separate quittance poller — **out of W8 scope**, see §6).  W8 only transitions Kvt2 → Ack and assumes the KVT raw rows are already persisted upstream.

**Invariant note.**  The `migrations/002` document_files PRIMARY KEY `(document_id, kind)` makes a duplicate KVT INSERT a hard sqlx error; W8's read-only relationship with document_files cannot lose or corrupt KVT bytes.

### R3 — Repeat finalize must be idempotent
**Mitigation.**  CAS `transition_state(Kvt2, Ack)` short-circuits in three observable shapes:
- `Applied` — happy path; doc moved to Ack.
- `Conflict` — doc exists but not in Kvt2.  If observed state is `Ack`, treat as **idempotent re-entry success** (`StageFinalizeOutcome::AlreadyAcked`).  Other states → `StageFinalizeOutcome::StateConflict { observed }` for caller / W9.
- `NotFound` — doc missing → `StageFinalizeOutcome::DocumentMissing`.
- `Forbidden` — `(Kvt2, Ack)` is in the W1 whitelist (`fiscal_documents.rs:147`); unreachable.

Re-run on Ack does NOT touch the seed (CAS short-circuits BEFORE seed UPDATE) and does NOT touch the outbox (no second row inserted).  Forensic audit: a `STAGE_FINALIZE_ACK` audit row is appended on every `Applied` transition, NEVER on Conflict/NotFound — operator sees exactly one ACK event per doc lifecycle.

**Test (fixture e).**  Pre-seed doc in `Ack`.  Run `stage_finalize::run`.  Post: state stays `Ack`; seed unchanged; outbox count == 0; audit unchanged from pre-state.

### R4 — Don't mix W8 finalize with W10 error-routing
**Mitigation.**  W8 ONLY observes the `Kvt2 → Ack` transition.  Error routing (`Sending → Rejected`, `Sending → ErrorRetryable`, `ErrorRetryable → RequiresManualReconciliation`, terminal-vs-transient `Server { code }` split) lives entirely in W7 stage_send (4-b) + W10 full dispatch table.  W8 does NOT consume `DpsError`, does NOT classify outcomes, does NOT touch `transport_trace`.

**Invariant note.**  `StageFinalizeError` enum (W8) and `StageSendError` enum (W7) are siblings, not subtypes.  No shared variants.  Cross-stage error routing is the dispatcher's job.

### R5 — Don't do App::boot recovery inside W8 (that's W9)
**Mitigation.**  W8 `pub async fn run` takes a specific `(pool, doc_id, fn_id, request_id)` — caller has already identified the doc.  W8 does NOT scan for stale `Kvt2` docs, does NOT iterate over the FN's pending set, does NOT decide whether to re-issue the wire call.  All of those belong to W9 App::boot reconciliation (with W10 dispatch as input).

**Invariant note.**  The dispatcher (out of M3a scope; mocked in M3a tests) is responsible for (a) detecting that a doc has reached `Kvt2`, and (b) calling `stage_finalize::run` to advance to `Ack`.  W8 trusts that contract.

---

## 3. Plan corrections (vs `2026-05-07-m3a-implementation.md` Task 8)

The plan is precise; only one expansion needed.

### 3.1 Schema gap — `outbox` table does not exist
**Plan said:** "outbox.enqueue_document (if outbox in scope; else stub)".

**Reality:** No `outbox` table in Rust migrations 001–010.  A no-op stub repo would fail to prove the "outbox INSERT inside lock" acceptance criterion (fixture d) — there'd be no row to assert against.

**Resolution.**  Add **migration 011 `outbox`** with the minimal M3a-stub schema:

```sql
CREATE TABLE outbox (
    document_id            BLOB    NOT NULL CHECK (length(document_id) = 16)
        REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT,
    fiscal_number          TEXT    NOT NULL,
    sequence_no            INTEGER NOT NULL,            -- = lnd at finalize time
    payload_sha256         BLOB    NOT NULL CHECK (length(payload_sha256) = 32),
    enqueued_at            TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status                 TEXT    NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'PUBLISHED')),
    published_at           TEXT,                        -- NULL until cross-process publish
    PRIMARY KEY (document_id)
) STRICT;

CREATE INDEX ix_outbox_pending ON outbox(enqueued_at) WHERE status = 'PENDING';
```

`ON DELETE RESTRICT` keeps outbox referentially honest if a doc gets archived; cross-process publishing flips `status` to `PUBLISHED` and sets `published_at` (out of W8 scope).

**Caveat — payload_path NOT included.**  Python carries `payload_path` (filesystem archive location); M3a Rust does not have an archive layer yet.  W8 stub omits the column.  When the archive layer lands (post-M3a), migration 012 adds it.

---

## 4. Decomposition

### 4.1 W8.1 — migration 011 outbox + outbox repo
- `migrations/011_outbox.sql` per §3.1.
- `rust/prro/src/db/repositories/outbox.rs`:
  - `pub struct NewOutboxRow { fiscal_number: String, sequence_no: i64, payload_sha256: [u8; 32] }` — caller supplies, doc_id is the FK.
  - `pub async fn enqueue_document_tx(tx: &mut WriteTxConn<'_>, doc_id: DocumentId, row: NewOutboxRow) -> sqlx::Result<()>` — single INSERT.  No no-op stub variant.
  - Register in `db/repositories/mod.rs`.
- Targeted tests in `tests/migration_011_outbox.rs` (5–7 fixtures): table+index exist, PK+FK enforced, status CHECK enforced, partial index present, single-doc PK uniqueness.

### 4.2 W8.2 — `node_state::update_last_known_xml_sha_tx` + `ingress_inbox::mark_done_tx`
- Convert/add `node_state::update_last_known_xml_sha_tx(tx, fn_id, sha)` — same SQL as the existing pool-bound `seed_prevhash`, but tx-bound for the W8 single-envelope.  Keep `seed_prevhash` (pool form is used elsewhere).
- Add `ingress_inbox::mark_done_tx(tx, request_id) -> sqlx::Result<bool>` — mirror `mark_rejected_tx` shape but with `status = 'DONE'`.  Returns `bool` (true = updated, false = inbox row missing) per the W7.2 helper convention.
- Targeted unit tests in `tests/fiscal_documents_send_helpers.rs` (existing W7.2 file) OR a new `node_state_finalize_helpers.rs` — 3–4 fixtures: happy update, missing row, idempotent re-update.

### 4.3 W8.3 — `stage_finalize.rs` worker step
- `rust/prro/src/services/write_path/stage_finalize.rs`:
  - `pub enum StageFinalizeError` (mirror W7 pattern):
    - `DocumentMissing { document_id }` — actually wait, this is non-error.  See enum below.
    - `UnsignedXmlShaMissing { document_id }` — doc in Kvt2 but `unsigned_xml_sha256 IS NULL`; state-invariant breach (W6 stage 3-PERSIST writes it on Prepared→Signed).
    - `SeedUpdateMissing { fn_id }` — `update_last_known_xml_sha_tx` returned `false`; node_state row missing (impossible after W5 acquire stage ran).
    - `InboxDoneMissing { request_id }` — `mark_done_tx` returned `false`; inbox row missing.
    - `Db(#[source] sqlx::Error)`.
    - `Internal(#[source] anyhow::Error)`.
  - `pub enum StageFinalizeOutcome`:
    - `Acked { fiscal_number: String }` — happy: state advanced + seed advanced + inbox DONE + outbox enqueued + audit appended.
    - `AlreadyAcked` — idempotent re-entry: doc was already `Ack` at CAS time; no-op success.
    - `StateConflict { observed: DocState }` — doc exists but not in Kvt2 (and not Ack); caller/W9 escalates.
    - `DocumentMissing` — race with delete; no-op.
  - `pub async fn run(pool: &SqlitePool, doc: DocumentId, fn_id: &str, request_id: &[u8; 16]) -> Result<StageFinalizeOutcome, StageFinalizeError>` — single `with_immediate` envelope:

```rust
let outcome = with_immediate(pool, move |tx| Box::pin(async move {
    // 1. CAS Kvt2 → Ack
    match fd::transition_state(tx, doc, DocState::Kvt2, DocState::Ack).await? {
        TransitionOutcome::Applied => {}
        TransitionOutcome::Conflict => {
            // Disambiguate already-Ack (idempotent) vs other states (real conflict).
            // Re-fetch state via fetch_send_inputs_tx (already returns state).
            let observed = match fd::fetch_send_inputs_tx(tx, doc).await? {
                Some(s) => s.state,
                None => return Ok::<_, anyhow::Error>(InternalOutcome::DocumentMissing),
            };
            return Ok(if observed == DocState::Ack {
                InternalOutcome::AlreadyAcked
            } else {
                InternalOutcome::StateConflict { observed }
            });
        }
        TransitionOutcome::NotFound => return Ok(InternalOutcome::DocumentMissing),
        TransitionOutcome::Forbidden => unreachable!("(Kvt2,Ack) is in the whitelist"),
    }

    // 2. Read doc's unsigned_xml_sha256 (state invariant: non-NULL post-W6).
    let row = fd::fetch_finalize_inputs_tx(tx, doc).await?
        .ok_or(anyhow::Error::new(StageFinalizeError::DocumentMissing { document_id: doc }))?;
    let seed = row.unsigned_xml_sha256.ok_or_else(|| {
        anyhow::Error::new(StageFinalizeError::UnsignedXmlShaMissing { document_id: doc })
    })?;

    // 3. Advance MAC chain seed.
    if !node_state::update_last_known_xml_sha_tx(tx, fn_id, &seed).await? {
        return Err(anyhow::Error::new(StageFinalizeError::SeedUpdateMissing {
            fn_id: fn_id.to_string(),
        }));
    }

    // 4. Mark inbox DONE.
    if !ingress_inbox::mark_done_tx(tx, request_id).await? {
        return Err(anyhow::Error::new(StageFinalizeError::InboxDoneMissing {
            request_id: *request_id,
        }));
    }

    // 5. Outbox INSERT.
    outbox::enqueue_document_tx(tx, doc, NewOutboxRow {
        fiscal_number: row.fiscal_number.clone(),
        sequence_no: row.lnd,
        payload_sha256: row.payload_sha256_canonical,
    }).await?;

    // 6. Audit.
    audit_log::append_tx(tx, "fiscal_document", &format!("{doc:?}"),
        "STAGE_FINALIZE_ACK", Severity::Info, None,
        Some(&serde_json::json!({"fiscal_number": row.fiscal_number, "lnd": row.lnd}).to_string())
    ).await?;

    Ok(InternalOutcome::Acked { fiscal_number: row.fiscal_number })
})).await.map_err(bridge_anyhow)?;
```

  - **No** `dps_channel` argument — finalize never calls the wire.  W3 static scanner over the closure body is automatically green (no foreign-IO method calls).
  - **`fetch_finalize_inputs_tx`** is a new tiny `fiscal_documents` helper returning `(fiscal_number, lnd, unsigned_xml_sha256, payload_sha256_canonical)`.  Could be inlined as a `query!` in `stage_finalize.rs`, but for repo-policy consistency I'll add it to `fiscal_documents.rs`.

### 4.4 W8.4 — Tests `tests/write_path_stage5_finalize.rs`
Six fixtures (plan said 4; +2 from R3 idempotency / R1 atomic-rollback proofs):

| # | Name | Verifies |
|---|---|---|
| a | `kvt2_to_ack_happy_path` | CAS applies; state == Ack; seed advanced; inbox DONE; outbox row inserted; audit `STAGE_FINALIZE_ACK` |
| b | `mac_chain_seed_advances_atomically_with_ack` | Read `node_state.last_known_unsigned_xml_sha256` BEFORE and AFTER; assert equal to doc's `unsigned_xml_sha256` post-finalize |
| c | `inbox_status_done_atomic_with_state_transition` | Read inbox status before (must be `PROCESSING`) and after (`DONE`) |
| d | `outbox_row_inserted_inside_lock_no_publish_yet` | Outbox row count = 1 with `status='PENDING'`, `published_at IS NULL` |
| e | `rerun_on_ack_is_idempotent_no_op` (R3) | Pre-seed doc in `Ack`; run; expect `AlreadyAcked` outcome; seed unchanged; outbox count unchanged; audit count unchanged |
| f | `non_kvt2_state_short_circuits_no_seed_advance` (R1 negative) | Pre-seed doc in `Kvt1`; run; expect `StateConflict { observed: Kvt1 }`; seed unchanged; outbox count == 0 |

`document_missing` (rare race) is sub-criterion test, can fold into fixture e shape if needed.

---

## 5. Invariants asserted by W8

| # | Invariant | How asserted |
|---|---|---|
| I1 | No network/crypto in lock | `stage_finalize::run` has no `dps_channel` parameter; closure body is pure DB; W3 scanner static-rejects any foreign-IO insertion |
| I2 | One `fiscal_number` = one writer | `WriteTxConn<'_>` sealed newtype throughout |
| I4 | Idempotency mandatory | Fixture e proves rerun-on-Ack is no-op; CAS short-circuit + audit-on-Applied-only |
| I8 | Recovery doesn't violate transitions | All five mutations atomic under one `BEGIN IMMEDIATE`; partial commits impossible |
| I9 | Graceful shutdown | Crash mid-finalize rolls back the entire tx; doc stays in Kvt2 for next-tick retry; W9 boot reconciliation handles long-stuck Kvt2 |
| **NEW** | MAC chain seed advances ONLY on Ack | Fixture b (positive), fixture e (idempotent no-op), fixture f (Kvt1 short-circuit) |
| **NEW** | Outbox row written exactly once per Ack | Fixture e (rerun-on-Ack — outbox count stays 1) + outbox PRIMARY KEY (document_id) |

---

## 6. Out of scope (intentionally deferred)

- **Sent → Kvt1 → Kvt2 wire-quittance pipeline.**  W7 freeze §6 said KVT1 inline production lands in W8; the canonical M3a plan Task 8 says only Kvt2 → Ack.  This freeze defers KVT1 inline / Kvt2 polling to **W10 (full DpsError dispatch)** or a separate quittance-poller slice.  W8 alone does not get docs to Ack end-to-end on the live wire — the dispatcher (or a future post-M3a slice) must close the gap.
- **OFFLINE_LOCAL_ACK path.**  Python `_stage_finalize_ack` handles offline; M3b deferred per CLAUDE.md.
- **Excise marks bookkeeping.**  M3b deferred.
- **Shift side-effects.**  W8 does NOT call `_apply_shift_side_effects_locked`; shift state mutations belong to a separate W-slice (W7 freeze noted shift_open / shift_close paths exist; their finalize-time bookkeeping is post-W8).
- **Outbox cross-process publishing.**  Stub schema accepts the row; the publish step (`status PENDING → PUBLISHED`) is a separate worker, post-M3a.
- **Archive `payload_path`.**  Filesystem archive layer is post-M3a.
- **App::boot reconciliation for stuck Kvt2 docs.**  W9 owns this.
- **W7 carry-forward cleanups** (`truncate_msg` byte→char, deduplicate inline truncate, audit payload enrichment).  Defer to W8/W9 final cleanup pass per W7 carry-forward note.

---

## 7. Open questions for sign-off

1. **Outbox schema realisation:** ship migration 011 (recommended; proves atomicity in fixture d) vs no-op stub (lighter diff but no real INSERT to assert against)?  Recommend **migration 011**.
2. **Audit event name:** `STAGE_FINALIZE_ACK` (mirrors W7 stage-naming `STAGE_SEND_INTENT_MARKED` / `STAGE_SEND_RESULT`) vs `DOCUMENT_ACK` (Python compat)?  Recommend **`STAGE_FINALIZE_ACK`** for Rust stage-naming consistency; cross-impl audit divergence is acceptable.
3. **Shape of `fetch_finalize_inputs_tx`:** new `fiscal_documents` helper vs inline `query!` in stage_finalize.rs?  Recommend **new helper** for repo-policy consistency.  Adds one more `.sqlx/` cache entry.
4. **Audit `payload_json` shape:** minimal `{"fiscal_number", "lnd"}` vs richer `{... + "unsigned_xml_sha256_hex" + "outbox_sequence_no"}`?  Recommend **richer** — operator forensics for cross-correlating with outbox/node_state.

---

## 8. Apply order (post-GO)

1. Migration 011 + outbox repo (W8.1) — small, isolated.
2. node_state + ingress_inbox helpers (W8.2).
3. `stage_finalize.rs` worker step (W8.3).
4. Fixtures (W8.4).
5. Final pass: `cargo test -p prro`, `cargo fmt --check`, `cargo clippy -p prro --tests --no-deps`.

Estimated 2-3 days, in line with plan.

---

## 9. Verify hooks

- `cargo test -p prro --test write_path_stage5_finalize` — 6 fixtures.
- `cargo test -p prro --test migration_011_outbox` — schema fixtures.
- `cargo test -p prro --test with_immediate_no_foreign_io` — W3 scanner over W8.3 closure (must stay 8/8).
- `cargo test -p prro` — full crate suite stays 261+ passed (plus W8 additions ~ +13).
- `cargo fmt -p prro --check` (locally before push — W7 reminder).

---

## 10. Deferred polish from W8.1 review (post-W8 / post-M3a)

Both W8.1 reviews (initial + max-effort re-pass) flagged three defensive items.  None block W8 progression; all three are recorded here so future passes can claim them deliberately, not by accident.

### F1 (closed) — FK ON DELETE RESTRICT negative test
**Status:** **landed in W8.1** (`fk_restrict_blocks_doc_delete_with_pending_outbox` fixture).  Negative test proves RESTRICT semantic; future migration accidentally re-declaring as CASCADE would be caught at CI.

### F2 (closed) — `get_for_document` returns `None` for missing row
**Status:** **landed in W8.1** (`get_for_document_returns_none_for_missing_row` fixture).

### F3-bis (deferred — post-M3a publisher worker) — typed `OutboxStatus` enum vs `String`
`OutboxRow.status` is currently `String`.  Stringly-typed comparisons (`row.status == "PENDING"`) are runtime-only checks; a typo would surface as a logic bug, not a compile error.  W7.1 `transport_trace::OutcomeKind` is the precedent for the cleaner shape:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboxStatus { Pending, Published }
impl OutboxStatus {
    pub fn as_str(self) -> &'static str { match self {
        Self::Pending => "PENDING", Self::Published => "PUBLISHED" } }
}
// OutboxRow.status: OutboxStatus  (decoded from raw String at SELECT boundary)
```

**Why deferred:** W8.3 stage_finalize::run only WRITES via `enqueue_document_tx` (which doesn't take `status` as a parameter — schema DEFAULT picks `'PENDING'`); it does NOT read `OutboxRow.status`.  The first real consumer is the post-M3a publisher worker, which will benefit from the typed enum at its query boundary.  Land the enum + decode shim alongside that worker.

### F4-bis (deferred — post-M3a hardening) — TRIGGER for core-field immutability post-INSERT
The current schema lets a future caller `UPDATE outbox SET sequence_no = ?` or `UPDATE outbox SET payload_sha256 = ?`.  No caller does this today (the W8 freeze constrains writes to `enqueue_document_tx` + the future publisher's status flip), but a defensive TRIGGER would catch the bug class at the DB level:

```sql
CREATE TRIGGER outbox_core_immutable
BEFORE UPDATE OF document_id, fiscal_number, sequence_no, payload_sha256 ON outbox
BEGIN
  SELECT RAISE(ABORT, 'outbox core fields are immutable post-INSERT');
END;
```

**Why deferred:** triggers add maintenance surface; currently no caller has the means or motive to mutate these fields.  Re-evaluate at post-M3a hardening pass once publisher worker shape is concrete.

### F5-bis (deferred — W8 final-verify pass) — FK violation negative test on INSERT side
F1 covered FK RESTRICT on the DELETE side.  The INSERT side (bogus `document_id` references nothing in `fiscal_documents`) is still uncovered.  5 LoC fixture:

```rust
#[tokio::test]
async fn enqueue_with_bogus_doc_id_violates_fk() {
    let (_d, pool) = fresh_pool().await;
    let bogus = DocumentId::from_bytes([0xFFu8; 16]);
    let res: Result<(), anyhow::Error> = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            outbox::enqueue_document_tx(tx, bogus, NewOutboxRow {
                fiscal_number: "1234567890".into(),
                sequence_no: 1,
                payload_sha256: [0u8; 32],
            }).await?;
            Ok(())
        })
    }).await;
    let err = res.expect_err("bogus doc_id must violate FK");
    assert!(err.to_string().to_lowercase().contains("foreign key"));
}
```

**Why deferred:** the W8.1 commit is otherwise complete and the slice is at a clean stop-point; this 5-LoC defensive test belongs in a tightly-scoped polish pass that won't muddy the W8.1 review history.  Land in W8 final-verify alongside W8 fmt + clippy sweep.

### bd follow-ups carry-forward (P3, unchanged)

- `PRRO_GATE-9qd.1.1` — promote `document_files` + `transport_trace` + `outbox` runtime queries to `query!`/`query_as!` macros + `#[derive(FromRow)]`.
- `PRRO_GATE-9qd.1.2` — feature-gate `pub mod test_hook` + production call site (`stage_sign.rs:372`).
- W7 cosmetics (defer to W8 / final cleanup pass): `truncate_msg` byte → char count; deduplicate inline `truncate_msg`; richer audit payloads in `STAGE_SEND_INTENT_MARKED` / `STAGE_SEND_RESULT`.
