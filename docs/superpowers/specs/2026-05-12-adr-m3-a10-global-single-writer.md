# ADR-M3-A10 — Global-single-writer invariant + carry-forward to multi-worker

**Date:** 2026-05-12
**Status:** ACCEPTED
**Companion to:** ADR-M3-A1..A9 (`docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md`).
**Closes:** MED-1 from W10 post-merge audit (`docs/superpowers/specs/2026-05-11-w10-final-audit.md`).
**Anchors:** W9 boot-reconciliation freeze §2.5 (`docs/superpowers/specs/2026-05-10-m3a-w9-boot-reconciliation-design.md:76-85`) — the originating clarification for W9 scope; this ADR generalises §2.5 to all of M3a.

---

## 1. Context

The W10 post-merge audit flagged docstring drift in `services/write_path/mac_recovery.rs` and adjacent stages: the term `single-writer-per-FN lease` overloads the word *lease*, implying a per-FN lock primitive that does **not** exist in the M3a runtime.

The W9 freeze §2.5 already withdrew an earlier claim about a `services::lease::acquire_per_fn` helper and explicitly noted that the only "lease" the codebase actually owns is `ingress_inbox::acquire_lease`, which is keyed on `request_id`, **not** `fiscal_number`.

This ADR codifies the resulting invariant for the whole of M3a (not just W9 boot) and names the obligation any future multi-worker slice must close.

## 2. Decision

The M3a runtime enforces a **global-single-writer** model. The phrase "single-writer-per-FN" is retained as a **logical invariant** (at any moment at most one writer mutates state for a given fiscal number), but it is enforced today by mechanisms **stronger than per-FN exclusion**, not by a per-FN lease.

### 2.1 The mechanisms (today)

1. **One tokio worker** drives the write-path orchestrator. `App` spawns a single orchestrator task; there is no worker pool in M3a. (Verified: `grep "Mutex|RwLock|DashMap" rust/prro/src/services/ rust/prro/src/db/repositories/ rust/prro/src/app.rs` returns zero hits — no FN-keyed lock primitive exists anywhere in the binary.)
2. **SQLite `BEGIN IMMEDIATE`** serialises writers globally on the WAL writer (`rust/prro/src/db/tx.rs:118-124` — the `with_immediate` helper). At most one write transaction is in flight across the whole database at any moment.
3. **Per-row CAS guards** on every state transition (`UPDATE fiscal_documents SET state = new WHERE state = expected_prior`). Lost CAS races return `RowsAffected = 0` and the loser bails without corrupting state.
4. **Request-scoped inbox CAS** via `ingress_inbox::acquire_lease(tx, &request_id)` (`rust/prro/src/db/repositories/ingress_inbox.rs:181-204`). This is a one-shot CAS `status = NEW → PROCESSING` keyed on `request_id`, not on `fiscal_number`. Its purpose is to prevent the same inbox row from being driven twice; it is **not** an FN-scope lock.

Together, (1) + (2) + (3) imply: no two write transactions ever overlap for any FN, regardless of which document, stage, or recovery branch is involved.

### 2.2 Why this is **stronger** than per-FN exclusion under M3a

A per-FN lock model permits N parallel writers on N distinct FNs. The M3a global-single-writer model permits at most 1 writer total. The global model trivially satisfies every guarantee a per-FN lease would provide; it just provides them with coarser granularity.

In particular, the two-step writes that previously named a "per-FN lease" as their safety dependency are safe today **because no other writer of any kind can run** between their two transactions:

- **MAC recovery split** (`mac_recovery.rs`): `MR-CLAIM` tx → re-sign (outside any tx) → `MR-PERSIST` tx. Between MR-CLAIM commit and MR-PERSIST begin, no other writer transaction can mutate `node_state.last_known_unsigned_xml_sha256` (or any state) for any FN, because there is only one worker.
- **Stage 5 finalize** (`stage_finalize.rs`): CAS `Kvt2 → Ack` + finalize-inputs read + seed update inside one `with_immediate` envelope. Each step inside the envelope is serialised by BEGIN IMMEDIATE.
- **Stage 4 send Pattern B** (`stage_send.rs`): 4-pre `Signed → Sending` CAS commit → wire send (no tx) → 4-b post-wire CAS commit. Between the two envelopes, no other writer can disturb the `Sending` marker.

### 2.3 The terminology rule (for docstrings and comments)

- **"single-writer-per-FN invariant"** — OK as the name of the logical property.
- **"single-writer-per-FN lease"** — NOT OK; implies a mechanism that does not exist. Replace with "single-writer-per-FN invariant (see ADR-M3-A10 for current enforcement mechanism)" or, where space allows, a direct reference to the global-single-writer mechanism.
- **"lease"** unqualified — OK only where it refers to `ingress_inbox::acquire_lease` (request-id-scoped inbox-row lease). The stage 1 module docstring at `stage_acquire.rs` uses it correctly in this narrow sense.

## 3. What this ADR does **not** decide

This ADR does **not** introduce a per-FN lock primitive. It records the absence of one and explains why the current implementation is correct without one.

## 4. Carry-forward — what a future multi-worker slice MUST add

Any slice that lifts M3a's single-worker constraint (a worker pool, a `ThreadPoolExecutor` parity with Python `reconciliation.py:296-316`, or any other concurrent-writer dispatch) **MUST** ship the following in the same slice:

1. **FN-scope exclusion primitive.** Either:
   - in-memory `Arc<DashMap<FiscalNumber, Arc<Mutex<()>>>>` acquired at the top of every write-path entry point (orchestrator, recovery, boot-dispatch) and held for the lifetime of one logical operation; OR
   - DB-level `fn_writer_lock` table with `INSERT ... ON CONFLICT FAIL` to take, `DELETE WHERE owner_token = ?` to release. (Trades observable runtime state for crash-leaked-lock recovery complexity.)
2. **Lock-leak recovery.** A process crash with a lock held must not block the FN forever. In-memory locks self-clear on process restart; DB locks need either TTL/heartbeat or boot-time cleanup keyed on dead `owner_token`s.
3. **Contention metrics.** Lock acquisition wait time, queue depth per FN, lock-holder identity per active acquisition. Emitted as Prometheus histograms keyed on `fiscal_number` (cardinality-bounded — only active FNs).
4. **Docstring sweep.** Replace "global-single-writer + BEGIN IMMEDIATE" wording in `mac_recovery.rs`, `stage_send.rs`, `stage_finalize.rs`, `transport_trace.rs:complete_via_recovery_tx`, and `boot_phase.rs:resume_sending_to_error_retryable` with the new lock primitive's contract. Cross-reference the new ADR.
5. **Tests.** A contention smoke test (two parallel writers race on same FN, lock must serialise them) + a stress test (N parallel writers across M FNs, deadlock-free, fairness reasonable).
6. **Update of this ADR.** Either supersede ADR-M3-A10 with a new ADR-M3-Axx, or add a `**SUPERSEDED-BY:** ADR-M3-Axx` header at the top and link forward.

## 5. Where this ADR is referenced

Live Rust modules cross-reference this ADR in their docstrings:

- `rust/prro/src/services/write_path/mac_recovery.rs` — "Caller obligation: single-writer-per-FN invariant" section + the §340 reordering-safety comment.
- `rust/prro/src/services/write_path/stage_send.rs` — `PostWireCasFailed` and `MarkSubmissionAttemptedMissing` variant docs.
- `rust/prro/src/services/write_path/stage_finalize.rs` — `InboxDoneMissing` variant doc and §264 race-with-delete comment.
- `rust/prro/src/services/write_path/stage_acquire.rs` — module docstring clarifier distinguishing inbox-row lease from FN-scope.
- `rust/prro/src/services/reconciliation/boot_phase.rs` — `resume_sending_to_error_retryable` outcome-shape doc.
- `rust/prro/src/db/repositories/transport_trace.rs` — `complete_via_recovery_tx` idempotency doc.

## 6. Verification

Mechanical verification accompanying this ADR:

- `cargo fmt -p prro` — unchanged.
- `cargo clippy -p prro --all-targets --no-deps -D warnings` — unchanged.
- `cargo test -p prro` — unchanged test count (no compiled-code change).
- `grep -rn "single-writer-per-FN lease\|per-FN lease" rust/prro/src/` — zero hits after this slice.

A smoke test pins the ADR's existence so it cannot be silently removed: `rust/prro/tests/adr_m3_a10_exists.rs` (introduced in the same commit) `include_str!`s this file and asserts it is non-empty.

## 7. Open questions deferred

Two minor questions intentionally deferred (not blocking this ADR):

- **CLAUDE.md invariant 2** ("One `fiscal_number` = one logical single-writer write-path") is unchanged. It describes the logical model, which the global-single-writer mechanism satisfies. No edit required.
- **Frozen design specs** (W0-1, W0-2, W0-3, W8 freeze, W9 freeze) that contain "single-writer-per-FN lease" wording are NOT amended. They are historical record of the design at freeze time; W9 §2.5 already provides the in-spec correction for W9 scope. Future spec readers reach the corrected understanding via this ADR.

---

**End of ADR-M3-A10.**
