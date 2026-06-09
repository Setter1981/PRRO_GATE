# M3a Implementation Plan — ONLINE happy-path write-path + guarded recovery

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers-extended-cc:subagent-driven-development` (recommended) or `superpowers-extended-cc:executing-plans` to execute this plan task-by-task.

**Goal.** Land the Rust ONLINE-only fiscal write-path: 5-stage pipeline (acquire+validate / guard / sign / send / finalize) with Pattern B SENDING-marker safety + App::boot reconciliation phase + DpsError routing dispatch.  Exit with a working fiscal endpoint suitable for pilot smoke-testing on real DPS.

**Anchored on (canonical, committed):** ADR-M3-A1..A9 in `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` (committed `8c72a14`).  Implementation contracts + 51 explicit test fixtures in:
- `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md` (W0-1 — state machines, lnd, CloseShift)
- `docs/superpowers/specs/2026-05-06-m3-w0-2-lock-discipline.md` (W0-2 — `with_immediate` + `WriteTxConn` + boundary patterns + §9 test gate)
- `docs/superpowers/specs/2026-05-06-m3-w0-3-retry-recovery.md` (W0-3 — DpsError routing + App::boot decision tree + deterministic-replay + §9 test gate)
- `docs/M3-W0-handoff.md` (gate document; §2 M3a scope, §3 M3b deferrals)

**Architecture.** Rust crate `prro` extends with `services::write_path` (5-stage pipeline), `services::reconciliation` (App::boot phase + post-boot reconciler stub), `db::tx::WriteTxConn<'_>` (sealed newtype gating transactional writes).  Pattern B at stage 4 send adds new `DocState::Sending` (8th pending state).  All transactional helpers (`transition_state`, `shifts::transition`) take `&mut WriteTxConn<'_>` — compile-time enforcement of "write happens inside `with_immediate`".

**Tech stack.** Rust 1.95 + sqlx 0.8 (SQLite STRICT, WAL) + tonic 0.12 (DPS gRPC) + tokio 1.x (multi-threaded runtime) + syn 2 / quote 1 (W5-sibling static scan extension) + trybuild (compile-fail tests) + tracing 0.1 + `tokio::task_local!`.

**Bundle code + tests in every production W-task.** No separate production-code tail tasks — for M3a a task is not landed without its §9 fixtures green.  Without tests, the contract has no proof.  **W11 is the explicit cross-stage deterministic-replay gate**: it is test-only by design because the replay invariant cannot be proven inside any single stage task.

**Day budgets are confidence ranges, not commitments.**  Aim: **6 weeks optimistic / 8 weeks realistic** end-to-end, including per-task review cycles.

---

## Inputs (frozen)

- `docs/M2-handoff.md` (M2 contract; §2 frozen contracts; §4.1 invariants; §4.3 entry-decisions).
- `docs/M3-W0-handoff.md` (gate document; §1 ADR matrix, §2 M3a scope, §3 M3b deferrals, §4 bd closure-gates, §5 entry approval).
- `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` (committed ADR-M3-A1..A9).
- `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md` + `…-w0-2-lock-discipline.md` + `…-w0-3-retry-recovery.md`.
- `docs/Multi-Protocol_PRRO_Gateway.md` (technical spec, full state machines).
- Python behavioural reference: `src/prro_gateway/services/write_path.py`, `reconciliation.py`, `offline_sync.py`, `runtime/container.py`, `transports/dps_fiscal_server.py`, `repositories/node_state.py`.
- Rust substrate (M2 frozen): `rust/prro/src/{crypto,transports::dps,xml,services::cert_refresher,db::tx,db::repositories,db::models}/`.
- `CLAUDE.md` frozen invariants.

---

## Dependency graph

```
W0a (admin)
  │
  ├──> W1  (schema + DocState::Sending)         [PARALLEL]
  └──> W4  (DpsError::Authorization amendment)  [PARALLEL]
            │
W1 ─────────┴──────────> W2  (WriteTxConn + transition_state signature)
                              │
                              W3  (with_immediate hybrid enforcement)
                                  │
W2,W3 ────────────────────────────W5  (write-path stage 1+2: acquire+validate+guard)
                                      │
                                      W6  (write-path stage 3: sign — Pattern A)
                                          │
W6,W3,W4 ──────────────────────────────────W7  (write-path stage 4: send — Pattern B)
                                              │
                                              W8  (write-path stage 5: finalize)
                                                  │
W4,W7 ────────────────────────────────────────────W10 (DpsError routing dispatch)
                                                       │
W8,W10 ────────────────────────────────────────────────W9  (App::boot reconciliation)
                                                            │
W5..W9 ─────────────────────────────────────────────────────W11 (deterministic-replay invariant)
```

W1 + W4 parallel (different failure domains; W4 is small additive API prep that should not wait on schema).  W9 explicitly AFTER W10 — boot recovery uses final DpsError routing, not a draft version.  Task IDs follow execution order; W labels keep ADR/work-package names, so Task 9 = W10 intentionally runs before Task 10 = W9.

---

## Task structure

### Task 0 (W0a): M3a epic + bd cross-link (administrative)

**Goal.** Create `PRRO_GATE-M3a` epic; link 5 entry-decision bd issues (PRRO_GATE-ddn / -zti / -k99 / -6bj / -ah8) as `child-of`; link M3a epic as `child-of` PRRO_GATE-9qd (M3 epic).  Mirror M2 / M3-W0 admin pattern.

**Day budget:** ~30 min.

**Files:** none (bd-only).

**Acceptance.**
- M3a epic created with title containing "M3a" and "implementation".
- 5 child-of edges (ddn/zti/k99/6bj/ah8 → M3a epic).
- child-of edge M3a epic → PRRO_GATE-9qd.

**Verify.** `bd list --status open | grep -A 5 'M3a.*implementation'`.

**BlockedBy.** none.

```json:metadata
{"files":[],"verifyCommand":"bd list --status open | grep -A 5 'M3a.*implementation'","acceptanceCriteria":["M3a epic created","5 child-of edges (ddn/zti/k99/6bj/ah8)","child-of edge to PRRO_GATE-9qd"]}
```

---

### Task 1 (W1): Schema foundation — migration 007 (UNIQUE fn,lnd) + 008 (DocState::Sending) + whitelist additions

**Goal.** Land the schema + DocState enum + transition whitelist amendments per ADR-M3-A1, A8, A9 step 1-3.  Foundation for all subsequent W-tasks.

**Day budget:** 1-2 days (confidence range).

**Anchored ADRs:** A1 (lnd UNIQUE), A8 (pending-set 8 states), A9 step 1-4 (Sending + migration 008 + whitelist).

**Files.**
- Create `rust/prro/migrations/007_lnd_unique.sql` — `CREATE UNIQUE INDEX ux_fd_fn_lnd ON fiscal_documents(fiscal_number, lnd);`
- Create `rust/prro/migrations/008_doc_state_sending.sql` — SQLite table-rebuild migration to extend `fiscal_documents.state` CHECK to include `'SENDING'`.  `ALTER TABLE ... ADD CHECK` is not available: migration MUST create `fiscal_documents_new` with the full updated schema + `STRICT`, copy rows, recreate FKs/indexes/triggers (`ix_fd_fn_lnd`, `ix_fd_state_pending`, `ix_fd_recon_manual`, `fd_updated_at`), then swap tables.  Existing rows require no data backfill.
- Modify `rust/prro/src/db/models/enums.rs:29-42` — add `Sending => "SENDING"` (12 → 13 DocState values).
- Modify `rust/prro/src/db/repositories/fiscal_documents.rs:81-103` — extend `allowed_transition` whitelist with: `(Signed,Sending)`, `(Encrypted,Sending)`, `(Sending,Sent)`, `(Sending,Kvt1)`, `(Sending,ErrorRetryable)`, `(Sending,Rejected)`, `(ErrorRetryable,Sending)`.
- Modify `rust/prro/src/db/repositories/fiscal_documents.rs:172-205` — `list_pending_for_fn`: doc-comment 7 → 8 pending states; SQL `state IN (...)` clause includes `'SENDING'`.
- Test: `rust/prro/tests/repo_fiscal_documents_state_cas.rs` — extend `allowed_transition_exhaustive_matrix` with new transitions.
- Test: `rust/prro/tests/migrations_007_008.rs` (new) — applies migrations to fresh DB; asserts (a) UNIQUE constraint fails on duplicate (fn,lnd); (b) CHECK constraint accepts `'SENDING'`; (c) existing rows untouched.

**Acceptance.**
- [ ] Migrations 007 + 008 apply cleanly via `sqlx::migrate!()`; no existing-row backfill.
- [ ] `DocState::Sending` round-trips through sqlx (Encode + Decode for Sqlite).
- [ ] `allowed_transition` matrix extended with 7 new entries; exhaustive matrix test green.
- [ ] `list_pending_for_fn` returns docs in `Sending` state; doc-comment lists 8 pending states.
- [ ] `cargo sqlx prepare` regenerates `.sqlx/` cache; SQLX_OFFLINE build green.

**Verify.** `cargo test -p prro --test repo_fiscal_documents_state_cas --test migrations_007_008`.

**BlockedBy.** Task 0.

```json:metadata
{"files":["rust/prro/migrations/007_lnd_unique.sql","rust/prro/migrations/008_doc_state_sending.sql","rust/prro/src/db/models/enums.rs","rust/prro/src/db/repositories/fiscal_documents.rs","rust/prro/tests/repo_fiscal_documents_state_cas.rs","rust/prro/tests/migrations_007_008.rs"],"verifyCommand":"cargo test -p prro --test repo_fiscal_documents_state_cas --test migrations_007_008","acceptanceCriteria":["migrations 007+008 apply cleanly","DocState::Sending round-trips through sqlx","allowed_transition matrix extended with 7 new entries","list_pending_for_fn returns Sending docs","cargo sqlx prepare regenerates cache"]}
```

---

### Task 2 (W4): M2/W3 additive amendment — `DpsError::Authorization { code, kind, message }`

**Goal.** Land the additive `DpsError::Authorization` variant extension per ADR-M3-A6 prereq.  Small isolated API prep that unblocks W7 (send) routing.

**Day budget:** 1-2 days.

**Anchored ADRs:** A6 prereq (additive amendment).

**Files.**
- Modify `rust/prro/src/transports/dps/error.rs:14` — change `Authorization(String)` to `Authorization { code: i32, kind: AuthorizationKind, message: String }`; add `pub enum AuthorizationKind { DocumentReject, FiscalNumberNotRegistered }`.
- Modify `rust/prro/src/transports/dps/dto.rs:178-184` — split `Status::ErrorVerefy | ErrorNotRegisteredRro | ErrorNotRegisteredSigner` arm into per-status arms populating `code` and `kind`: `-1 → DocumentReject`, `-13 / -14 → FiscalNumberNotRegistered`.
- Modify `rust/prro/tests/dps_channel_smoke.rs` — extend status-routing tests to assert the new fields populated correctly per status code.

**Acceptance.**
- [ ] `Authorization` variant exposes `code`, `kind`, `message` fields; backward-compat note: this is breaking for any caller pattern-matching on `Authorization(s)` with positional binding (none today; verify via grep).
- [ ] `dto.rs` decoder splits `-1` vs `-13`/`-14` into distinct `kind` populations; raw status code preserved in `code`.
- [ ] `dps_channel_smoke.rs` adds 3 new fixtures (one per status) asserting `kind == DocumentReject` for `-1` and `kind == FiscalNumberNotRegistered` for `-13`/`-14`.
- [ ] Existing W3 tests stay green (W2 `cargo test -p prro` baseline preserved).

**Verify.** `cargo test -p prro --test dps_channel_smoke && cargo test -p prro` (full suite green).

**BlockedBy.** Task 0.  **NOT blocked by W1** (parallel — different failure domain).

```json:metadata
{"files":["rust/prro/src/transports/dps/error.rs","rust/prro/src/transports/dps/dto.rs","rust/prro/tests/dps_channel_smoke.rs"],"verifyCommand":"cargo test -p prro --test dps_channel_smoke && cargo test -p prro","acceptanceCriteria":["Authorization variant has code/kind/message fields","dto.rs splits -1 vs -13/-14","3 new smoke fixtures green","full prro suite stays green"]}
```

---

### Task 3 (W2): `WriteTxConn<'_>` sealed newtype + `transition_state` / `shifts::transition` signature change

**Goal.** Land sealed newtype gating transactional writes per ADR-M3-A4.  Change `with_immediate` closure signature; update repository helpers.

**Day budget:** 3-4 days.  **Risk:** lifetime-shape ergonomics (3 fallback HRTB shapes documented in W0-2 §4.4).

**Anchored ADRs:** A4 (newtype + signature change).

**Files.**
- Modify `rust/prro/src/db/tx.rs` — add `pub struct WriteTxConn<'a>` with `inner: &'a mut SqliteConnection` + `_seal: ()` private field; module-private `fn new` constructor; `Deref`/`DerefMut` to `SqliteConnection`; `#[cfg(test)] pub(super) fn new_for_test`.  Change `with_immediate` closure signature from `for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, _>` to `for<'c> FnOnce(&'c mut WriteTxConn<'c>) -> BoxFuture<'c, _>` (or fallback HRTB shape per W0-2 §4.4).
- Modify `rust/prro/src/db/repositories/fiscal_documents.rs:139-170` — `transition_state` signature change to take `&mut WriteTxConn<'_>` instead of `&SqlitePool`.  Remove module doc-comment `:14-25` "known limitation deferred to M3" (resolved by construction).
- Modify `rust/prro/src/db/repositories/shifts.rs:83-99` — `transition` signature change to take `&mut WriteTxConn<'_>`.
- Modify existing closure-style call sites: `rust/prro/src/db/repositories/ingress_inbox.rs:67`, `rust/prro/src/services/cert_refresher.rs:292,365` — closures accept `&mut WriteTxConn<'_>`; inline `sqlx::query(...).execute(&mut *conn)` becomes `…execute(&mut **conn)` (Deref chain).
- Modify existing tests using `transition_state` / `transition` / `with_immediate` — update call sites.
- Add `rust/prro/tests/write_tx_conn_compile_fail/` (trybuild fixture dir) — 5 compile-fail cases per W0-2 §9.2 (raw `&mut SqliteConnection` rejected; private `WriteTxConn::new` outside `db::tx`; struct-literal `_seal` field private; valid usage compiles in pass.rs; `new_for_test` cfg(test)-gated).
- Add `rust/prro/tests/transition_state_atomicity.rs` — Phase B post-fix CI test per W0-2 §9.3 (concurrent deleter blocks on RESERVED lock; `transition_state` outcome consistent).

**Acceptance.**
- [ ] `WriteTxConn` sealed: external module attempt to call `WriteTxConn::new` is `error[E0603]: function "new" is private`; struct-literal attempt is `error[E0451]: field "_seal" is private`.
- [ ] `transition_state` and `shifts::transition` take `&mut WriteTxConn<'_>`; raw `&mut SqliteConnection` callers fail to compile.
- [ ] All M2-shipped tests (164 baseline) stay green after the refactor.
- [ ] 5 trybuild compile-fail fixtures green.
- [ ] Phase B atomicity test green (deterministic via SQLite RESERVED lock ordering).
- [ ] Doc-comment "known limitation" at `fiscal_documents.rs:14-25` removed.

**Verify.** `cargo test -p prro --test write_tx_conn_compile_fail --test transition_state_atomicity && cargo test -p prro` (full suite, 164+ baseline preserved).

**BlockedBy.** Task 1 (W1) — uses extended whitelist + DocState::Sending.  **NOT blocked by W4** (different failure domain).

**Risk note.** If primary HRTB `for<'c> FnOnce(&'c mut WriteTxConn<'c>)` rejected by borrow checker, fall back to (i) separate inner/outer lifetimes with `'a: 'c` bound, (ii) by-value `WriteTxConn<'c>` move into closure.  ADR-M3-A4 fallback (option a POLICY ONLY) is the last-resort escape if all 3 shapes fail.

```json:metadata
{"files":["rust/prro/src/db/tx.rs","rust/prro/src/db/repositories/fiscal_documents.rs","rust/prro/src/db/repositories/shifts.rs","rust/prro/src/db/repositories/ingress_inbox.rs","rust/prro/src/services/cert_refresher.rs","rust/prro/tests/write_tx_conn_compile_fail/","rust/prro/tests/transition_state_atomicity.rs"],"verifyCommand":"cargo test -p prro --test write_tx_conn_compile_fail --test transition_state_atomicity && cargo test -p prro","acceptanceCriteria":["WriteTxConn sealed; trybuild compile-fail green","transition_state + shifts::transition take &mut WriteTxConn<'_>","5 trybuild compile-fail fixtures green","Phase B atomicity test green","M2 baseline 164+ tests stay green","doc-comment known-limitation removed"]}
```

---

### Task 4 (W3): `with_immediate` hybrid enforcement — static scan + `tokio::task_local!`

**Goal.** Land the runtime + static-scan dual gate per ADR-M3-A3.

**Day budget:** 3-4 days.

**Anchored ADRs:** A3 (hybrid enforcement).

**Files.**
- Modify `rust/prro/src/db/tx.rs` — add `tokio::task_local! IN_WITH_IMMEDIATE`; `with_immediate` enters via `IN_WITH_IMMEDIATE.scope((), async { f(&mut wt).await })`.
- Modify `rust/prro/src/crypto/in_process.rs` and other `prro::crypto::*` public-API entry points — add `debug_assert!(IN_WITH_IMMEDIATE.try_with(|_| ()).is_err(), "foreign IO inside with_immediate")` at entry of `sign_cms_detached`, `verify_dstu`, `unwrap_envelope`, `fetch_cert_by_ski`.
- Modify `rust/prro/src/transports/dps/grpc.rs` and `channel.rs` trait — add same `debug_assert!` at entry of `send_chk`, `last_chk`, `ping`, `status_rro`, `info_rro`, `query_by_local_identity`, `by_server_fiscal_no`.
- Add `rust/prro/tests/with_immediate_no_foreign_io.rs` (new) — W5-sibling syn-based AST scanner.  Walks every `with_immediate(...)` closure body in `rust/prro/src/`; denylist = M2 substrate method names + literal `tokio::task::spawn_blocking` / `tokio::task::block_in_place` call expressions.  Emits compile-test failure per offending site.
- Add `rust/prro/tests/with_immediate_runtime_guard.rs` (new) — 5 fixtures per W0-2 §9.1: (1) static-scan substrate methods; (2) runtime indirect helper; (3) static-scan ad-hoc spawn_blocking; (4) runtime provider entry positive control; (5) negative control outside tx.

**Acceptance.**
- [ ] `tokio::task_local!` setup compiles and `try_with` returns `Err` outside scope, `Ok(())` inside scope.
- [ ] Every M2 substrate public-API entry has `debug_assert!`; coverage spot-check via grep matches the M2-handoff §2.1/§2.2 method inventories.
- [ ] Static-scan test catches the 11 substrate method names AND `spawn_blocking` / `block_in_place` literal call expressions.
- [ ] 5 §9.1 fixtures behave per W0-2 spec (2 static-scan FAIL, 2 runtime panic, 1 negative control).
- [ ] Release build compiles out the `debug_assert!` (zero perf cost).

**Verify.** `cargo test -p prro --test with_immediate_no_foreign_io --test with_immediate_runtime_guard`.

**BlockedBy.** Task 3 (W2) — extends `with_immediate` signature already changed in W2.

```json:metadata
{"files":["rust/prro/src/db/tx.rs","rust/prro/src/crypto/in_process.rs","rust/prro/src/transports/dps/grpc.rs","rust/prro/src/transports/dps/channel.rs","rust/prro/tests/with_immediate_no_foreign_io.rs","rust/prro/tests/with_immediate_runtime_guard.rs"],"verifyCommand":"cargo test -p prro --test with_immediate_no_foreign_io --test with_immediate_runtime_guard","acceptanceCriteria":["tokio::task_local! setup correct","debug_assert! at every M2 substrate entry","static scan catches substrate methods + spawn_blocking + block_in_place","5 §9.1 fixtures behave per spec","release build zero perf cost"]}
```

---

### Task 5 (W5): Write-path stages 1+2 — acquire+validate + guard

**Goal.** Land write-path entry stages per W0-1 §3.1-§3.2 + W0-2 §2 row 1 + ADR-M3-A1 lnd sequencer.

**Day budget:** 3-4 days.

**Anchored ADRs:** A1 (lnd sequencer), A5 (Pattern A — pure DB stage), A7 (App::boot interaction surface for shift state).

**Files.**
- Create `rust/prro/src/services/write_path/mod.rs` + `stage_acquire.rs` (new module).
- Create `rust/prro/src/services/write_path/types.rs` — `WorkerContext` shape, `WorkerProcessResult` enum (mirror Python `write_path.py` types).
- Modify existing `rust/prro/src/db/repositories/node_state.rs` — add `allocate_next_lnd(conn: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<i64>` per Python `node_state.py:21-46` shape (UPDATE … RETURNING).
- Modify existing `rust/prro/src/db/repositories/ingress_inbox.rs` — add `acquire_lease(conn: &mut WriteTxConn<'_>, ...) -> sqlx::Result<Option<InboxRow>>` (CAS NEW→PROCESSING; mirror Python `services/ingress.py` shape).
- Modify existing `rust/prro/src/db/repositories/audit_log.rs` — add `append_tx(conn: &mut WriteTxConn<'_>, event: AuditEvent)` helper (do not create a duplicate audit module).
- Add `rust/prro/tests/write_path_stage1_acquire.rs` — fixtures: (a) happy path (acquire + lnd alloc + INSERT PREPARED); (b) lease miss (NOOP); (c) duplicate inbox replay-detect; (d) UNIQUE(fn,lnd) collision (concurrent writers — assertion fires per A1).

**Acceptance.**
- [ ] Stage 1 opens `with_immediate` exactly once per request; all writes (lease + node_state.next_lnd UPDATE + INSERT fiscal_documents + audit) inside the same tx.
- [ ] No network/crypto inside `with_immediate` (verified by W3 §9.1 fixture #1 + #4).
- [ ] lnd allocation atomic: concurrent writers hitting the same FN serialise on RESERVED lock; UNIQUE(fn,lnd) acts as fail-closed.
- [ ] Lease miss returns `WorkerProcessResult::Noop` without state mutation.
- [ ] Inbox replay-detect via existing M1 `(fiscal_number, idempotency_key)` UNIQUE.
- [ ] 4 stage-1 fixtures green; M2 baseline preserved.

**Verify.** `cargo test -p prro --test write_path_stage1_acquire`.

**BlockedBy.** Task 3 (W2 — uses WriteTxConn) + Task 4 (W3 — guarded by task_local).

```json:metadata
{"files":["rust/prro/src/services/write_path/mod.rs","rust/prro/src/services/write_path/stage_acquire.rs","rust/prro/src/services/write_path/types.rs","rust/prro/src/db/repositories/node_state.rs","rust/prro/src/db/repositories/ingress_inbox.rs","rust/prro/src/db/repositories/audit_log.rs","rust/prro/tests/write_path_stage1_acquire.rs"],"verifyCommand":"cargo test -p prro --test write_path_stage1_acquire","acceptanceCriteria":["stage 1 opens with_immediate once per request","no network/crypto inside lock","lnd allocation atomic + UNIQUE fail-closed","lease miss returns Noop","inbox replay-detect","4 stage-1 fixtures green"]}
```

---

### Task 6 (W6): Write-path stage 3 — sign (Pattern A: compute outside, persist inside)

**Goal.** Land sign stage per W0-1 §3.3 + W0-2 §2 row 3a/3b + ADR-M3-A2 (Z-allocation by wire artifact) + A5 (Pattern A).

**Day budget:** 3-4 days.

**Anchored ADRs:** A2 (CloseShift→ZReport at builder; Z-allocation by wire_artifact_kind), A5 (Pattern A: hoist sign above lock).

**Files.**
- Create `rust/prro/src/services/write_path/stage_sign.rs`.
- `stage_sign` does: (a) load FN config + active cert (read-only, no lock); (b) build canonical XML via `prro::xml::build_canonical_xml(&CanonicalDoc)`; (c) **derive `wire_artifact_kind` first**; (d) if `wire_artifact_kind == ZReport`, allocate Z-number via `node_state::allocate_z_report_number(...)` inside its own short `with_immediate`; (e) call `provider.sign_cms_detached(req).await` OUTSIDE any `with_immediate`; (f) open `with_immediate` and persist `transition_state(Prepared, Signed)` + `INSERT document_files(SIGNED_XML)` + audit.
- Add `rust/prro/src/db/repositories/document_files.rs` (new) — `insert(conn: &mut WriteTxConn<'_>, ...)` helper.
- Modify existing `rust/prro/src/db/repositories/node_state.rs` — add `allocate_z_report_number(conn: &mut WriteTxConn<'_>, fn_id: &str) -> sqlx::Result<i64>` (mirror Python `node_state.py:48-...`).
- Add `rust/prro/tests/write_path_stage3_sign.rs` — fixtures: (a) happy path (Prepared → Signed; sign called BEFORE lock per spy timestamp); (b) Z-allocation fires for both `internal_op == SHIFT_CLOSE` and `internal_op == Z_REPORT` (proving wire_artifact_kind keying); (c) Crypto provider error → ErrorRetryable + audit (no Sending, no wire); (d) byte-equivalence: signed payload matches W4 golden.

**Acceptance.**
- [ ] `wire_artifact_kind` derived BEFORE Z-allocation gate; both internal labels fire allocation when the wire-kind is ZReport.
- [ ] `provider.sign_cms_detached(...)` called OUTSIDE any `with_immediate` (W3 task_local guard would panic if violated).
- [ ] Sign result persisted inside `with_immediate` via `transition_state(Prepared, Signed)` CAS + `document_files INSERT`.
- [ ] Pattern A timestamp ordering test (sign before lock) per W0-2 §9.4 fixture #1 green.
- [ ] 4 stage-3 fixtures green.

**Verify.** `cargo test -p prro --test write_path_stage3_sign`.

**BlockedBy.** Task 5 (W5).

```json:metadata
{"files":["rust/prro/src/services/write_path/stage_sign.rs","rust/prro/src/db/repositories/document_files.rs","rust/prro/src/db/repositories/node_state.rs","rust/prro/tests/write_path_stage3_sign.rs"],"verifyCommand":"cargo test -p prro --test write_path_stage3_sign","acceptanceCriteria":["wire_artifact_kind derived before Z-allocation","sign called outside with_immediate","sign result persisted inside with_immediate","Pattern A timestamp ordering proved","4 stage-3 fixtures green"]}
```

---

### Task 7 (W7): Write-path stage 4 — send (Pattern B with SENDING marker)

**Goal.** Land Pattern B 3-segment send stage (4-pre / 4a / 4b) per W0-2 §2 row 4 + ADR-M3-A5 + A9 step 5.  Critical safety task — duplicate-send hazard prevention.

**Day budget:** 4-5 days.  **Largest M3a task.**

**Anchored ADRs:** A5 (Pattern B mandatory), A6 (DpsError routing; needs W4 amendment), A9 step 5-6 (stage-4 implementation).

**Files.**
- Create `rust/prro/src/services/write_path/stage_send.rs`.
- `stage_send` does: (4-pre) open `with_immediate` → `transition_state(Signed, Sending)` CAS → `submission_attempted_at` UPDATE → `audit_log` `STAGE_SEND_INTENT_MARKED` → commit → release; (4a) call `dps_channel.send_chk(envelope).await` OUTSIDE the lock; (4b) open `with_immediate` → CAS `Sending → Sent` (or `Sending → Kvt1` if KVT1 inline; or `Sending → Rejected` for terminal reject; or `Sending → ErrorRetryable` for transient with known wire reply) per response → `transport_request_id` set → `transport_trace` INSERT → audit.
- Add `rust/prro/src/db/repositories/transport_trace.rs` (new) — `insert(conn: &mut WriteTxConn<'_>, ...)` helper.
- Wire dispatch: response classification helper `classify_send_outcome(&Result<SendChkResponse, DpsError>) -> SendOutcome` mapping DpsError variants to target DocState per W0-3 §2 (full table-driven dispatch lands in W10; W7 carries minimal happy + rejected + retryable variants).
- Add `rust/prro/tests/write_path_stage4_send.rs` — fixtures: (a) happy path Sending → Sent via tonic mock; (b) inline KVT1 → Sending → Kvt1; (c) terminal reject (mock returns -1 ERROR_VEREFY) → Sending → Rejected; (d) transport error → Sending → ErrorRetryable; (e) Pattern B intent-marker order proof (SENDING commit timestamp BEFORE wire send timestamp via DpsChannel mock spy) per W0-2 §9.4 fixture #2.

**Acceptance.**
- [ ] 3-segment lock structure (4-pre / 4a / 4b) verified via 3 distinct `with_immediate` opens per request (spy on `BEGIN IMMEDIATE` SQL events).
- [ ] Wire send happens BETWEEN the two locks (4a outside any lock); W3 task_local guard would panic if violated.
- [ ] All 4 happy-path Sending → Sent / Kvt1 / Rejected / ErrorRetryable transitions exercised.
- [ ] Pattern B timestamp ordering (`sending_commit_ts < send_chk_call_ts`) per W0-2 §9.4 #2 green.
- [ ] Idempotency: re-running stage 4 on already-Sent doc is a `Forbidden` outcome (CAS short-circuits via whitelist).
- [ ] 5 stage-4 fixtures green.

**Verify.** `cargo test -p prro --test write_path_stage4_send`.

**BlockedBy.** Task 6 (W6) + Task 4 (W3 — task_local guard) + Task 2 (W4 — DpsError::Authorization shape).

```json:metadata
{"files":["rust/prro/src/services/write_path/stage_send.rs","rust/prro/src/db/repositories/transport_trace.rs","rust/prro/tests/write_path_stage4_send.rs"],"verifyCommand":"cargo test -p prro --test write_path_stage4_send","acceptanceCriteria":["3-segment lock structure verified","wire send between locks","4 happy-path transitions exercised","Pattern B timestamp ordering proved","idempotency Forbidden outcome","5 stage-4 fixtures green"]}
```

---

### Task 8 (W8): Write-path stage 5 — finalize

**Goal.** Land terminal-success bookkeeping per W0-1 §3.5 + W0-2 §2 row 5.

**Day budget:** 2-3 days.

**Anchored ADRs:** A8 (KVT2 forward-only; Ack as terminal-success).

**Files.**
- Create `rust/prro/src/services/write_path/stage_finalize.rs`.
- `stage_finalize` does: open `with_immediate` → `transition_state(Kvt2, Ack)` CAS → `node_state.last_known_unsigned_xml_sha256` UPDATE (next-doc MAC chain seed) → `ingress_inbox.status = DONE` → `outbox.enqueue_document` (if outbox in scope; else stub) → `audit_log` → commit; outbox publish (cross-process notification) is post-commit.
- Modify existing `rust/prro/src/db/repositories/node_state.rs` — add `update_last_known_xml_sha(conn: &mut WriteTxConn<'_>, fn_id: &str, sha: [u8; 32])` helper.
- Add `rust/prro/src/db/repositories/outbox.rs` (new, stub for M3a) — `enqueue_document(conn: &mut WriteTxConn<'_>, doc_id: DocumentId)` helper.
- Add `rust/prro/tests/write_path_stage5_finalize.rs` — fixtures: (a) Kvt2 → Ack happy path; (b) MAC chain seed (`last_known_unsigned_xml_sha256`) updated atomically with Ack; (c) inbox.status = DONE atomic with state transition; (d) outbox INSERT inside lock; outbox publish stub-callable post-commit.

**Acceptance.**
- [ ] Finalize is single `with_immediate`; all writes (state + node_state + inbox + outbox INSERT + audit) atomic.
- [ ] No network/crypto inside finalize lock.
- [ ] MAC chain seed updated only on real Ack (not on KVT2 alone).
- [ ] 4 stage-5 fixtures green.

**Verify.** `cargo test -p prro --test write_path_stage5_finalize`.

**BlockedBy.** Task 7 (W7).

```json:metadata
{"files":["rust/prro/src/services/write_path/stage_finalize.rs","rust/prro/src/db/repositories/node_state.rs","rust/prro/src/db/repositories/outbox.rs","rust/prro/tests/write_path_stage5_finalize.rs"],"verifyCommand":"cargo test -p prro --test write_path_stage5_finalize","acceptanceCriteria":["finalize is single with_immediate","no network/crypto inside finalize","MAC chain seed updated only on Ack","4 stage-5 fixtures green"]}
```

---

### Task 9 (W10): DpsError routing dispatch — full 8-variant + 12-status-code table

**Goal.** Land complete DpsError → DocState transition routing per W0-3 §2 + §2.1 + ADR-M3-A6.  Exercises both live (Sending source) and reconciliation (Sent source) paths.

**Day budget:** 3-4 days.

**Anchored ADRs:** A6 (full retry policy table), A9 (retry path uses ErrorRetryable → Sending, not direct → Sent).

**Files.**
- Modify `rust/prro/src/services/write_path/stage_send.rs` — replace W7's minimal `classify_send_outcome` with full `DpsError` → `SendOutcome` mapping per W0-3 §2 main table (8 variants) + `Server { code, .. }` sub-table (12 codes).
- Add `rust/prro/src/services/write_path/error_routing.rs` (new) — pure-fn `route_dps_error(err: &DpsError, doc_type: DocType, is_live_send: bool) -> RoutingDecision { target_state, audit_event, retry_class }`.  Live vs reconciliation context disambiguates Sending source vs Sent source.
- Add `rust/prro/tests/write_path_dps_error_routing.rs` — 21 fixtures per W0-3 §9.2:
  - 10 covering §2 main 8 DpsError variants (Transport, Authorization with both AuthorizationKind, Decode, Server { code }, NotFound, ServerFiscalIdMismatch, QueryNotSupported, Internal).
  - 11 covering §2.1 sub-table 12 Server-routed codes (-2/-3/-5/-6/-7-10/-11/-12/-15 with two variants/-16) — `-7..-10` collapsed into one parametrised XML-class fixture.
- Add `rust/prro/tests/write_path_mac_recovery.rs` — fixture for `Server { code: -12, .. }` MAC recovery path (regex-extract `store {64hex}` from error_message; one bounded re-derive + re-sign + re-send via Pattern B).

**Acceptance.**
- [ ] All 8 DpsError variants have explicit routing decision; no fall-through.
- [ ] All 12 Server-routed status codes have explicit routing decision; -2 and -15 honour their two-variant logic per W0-3 §2.1.
- [ ] AuthorizationKind::DocumentReject → Sending → Rejected (live) / Sent → Rejected (recon); FiscalNumberNotRegistered → Sending → ErrorRetryable → RequiresManualReconciliation chain.
- [ ] M3a DPS retry path uses `ErrorRetryable → Sending` ONLY (verify via test that `ErrorRetryable → Sent` whitelist `:99` is NEVER invoked by M3a DPS code; provider spy asserts).
- [ ] MAC recovery for -12 fires bounded re-attempt; re-sign uses W6 stage-3 pattern; re-send goes through Pattern B SENDING marker.
- [ ] 21 routing fixtures + MAC recovery fixture green.

**Verify.** `cargo test -p prro --test write_path_dps_error_routing --test write_path_mac_recovery`.

**BlockedBy.** Task 7 (W7 — exercises routing in send) + Task 2 (W4 — DpsError::Authorization shape).

```json:metadata
{"files":["rust/prro/src/services/write_path/stage_send.rs","rust/prro/src/services/write_path/error_routing.rs","rust/prro/tests/write_path_dps_error_routing.rs","rust/prro/tests/write_path_mac_recovery.rs"],"verifyCommand":"cargo test -p prro --test write_path_dps_error_routing --test write_path_mac_recovery","acceptanceCriteria":["all 8 DpsError variants routed","all 12 Server-routed codes routed","AuthorizationKind dispatch correct","ErrorRetryable→Sent never invoked by M3a DPS","MAC recovery -12 path green","21 routing + MAC fixtures green"]}
```

---

### Task 10 (W9): App::boot reconciliation phase — 6-branch per-FN decision tree

**Goal.** Land App::boot reconciliation per W0-3 §4 + ADR-M3-A7.  Closes PRRO_GATE-ah8 acceptance test verbatim.  After W10 so boot uses final DpsError routing.

**Day budget:** 4-5 days.

**Anchored ADRs:** A7 (App::boot 6-branch decision tree).

**Files.**
- Modify `rust/prro/src/app.rs` — add `App::reconcile_pending(&self) -> anyhow::Result<()>` method called after `App::boot` returns, before runtime accepts ingress.
- Create `rust/prro/src/services/reconciliation/mod.rs` + `boot_phase.rs` (new module).
- `boot_phase::run` does: (1) acquire singleton via `runtime::singleton::acquire`; (2) `PRAGMA quick_check` — fail-closed without DB writes if not OK (CRITICAL log + non-zero exit; mirror Python `container.py:144`); (3) per-FN-row decision tree per W0-3 §4.3 branches (a)-(f); (4) per-FN reconcile via `list_pending_for_fn` + W0-3 §3 per-state recovery rules.
- Wire DpsChannel + CryptoProvider into reconciliation worker (uses W7 stage-4 + W10 routing for Sent → last_chk + ErrorRetryable → Sending re-drive).
- Add `rust/prro/tests/app_boot_reconciliation.rs` — 9 fixtures per W0-3 §9.1 covering branches (a)-(f) + idempotency + quick_check failure.  PRRO_GATE-ah8 acceptance test verbatim (create row with `shift_state=Opened`; run boot; assert no overwrite).
- Add `rust/prro/tests/app_boot_quick_check_failure.rs` — separate fixture for fixture #8: deliberate DB corruption; assert no FN-row writes after failed probe.

**Acceptance.**
- [ ] `App::boot` keeps current shape (pool + migrations only); `reconcile_pending` is a separate method.
- [ ] PRAGMA quick_check fail-closed: no `node_state` / `shifts` / `audit_log` writes after the failed probe; CRITICAL log emitted; non-zero exit.
- [ ] Branch (a) absent FN → `upsert_initial` — only safe call site for that helper.
- [ ] Branch (b)-(c) idempotent / pending docs — no `upsert_initial` invocation (provider-spy assertion).
- [ ] Branch (d) OFFLINE-on-boot → hard refusal; node_state UNCHANGED.
- [ ] Branch (e) PRRO_GATE-ah8 verbatim test green: shift_state=Opened preserved; no overwrite.
- [ ] Branch (e) orphan: shift transitions to Error + CRITICAL audit when no pending doc found.
- [ ] Branch (f) BLOCKED/STOP_MODE/CRYPTO_DEGRADED preserve.
- [ ] Idempotency (run boot twice in immediate succession): observationally equivalent to branch (b).
- [ ] 9 §9.1 fixtures green (branches + idempotency + quick_check).

**Verify.** `cargo test -p prro --test app_boot_reconciliation --test app_boot_quick_check_failure`.

**BlockedBy.** Task 8 (W8 — pipeline complete) + Task 9 (W10 — boot recovery uses final DpsError routing).

```json:metadata
{"files":["rust/prro/src/app.rs","rust/prro/src/services/reconciliation/mod.rs","rust/prro/src/services/reconciliation/boot_phase.rs","rust/prro/tests/app_boot_reconciliation.rs","rust/prro/tests/app_boot_quick_check_failure.rs"],"verifyCommand":"cargo test -p prro --test app_boot_reconciliation --test app_boot_quick_check_failure","acceptanceCriteria":["App::boot keeps pool+migrations shape","quick_check fail-closed without DB writes","upsert_initial only for branch (a)","PRRO_GATE-ah8 verbatim test green","branch (e) orphan to Error + CRITICAL","idempotency run-twice","9 §9.1 fixtures green"]}
```

---

### Task 11 (W11): Deterministic-replay invariant — 9 crash-point fixtures

**Goal.** Prove §6 deterministic-replay invariant per W0-3 §9.3 — recovery converges to same final state regardless of crash-vs-uninterrupted run.

**Day budget:** 2-3 days.

**Anchored ADRs:** A8 (pending-set; intentional whitelist gaps), A9 (Sending crash-resume contract).

**Files.**
- Add `rust/prro/tests/write_path_deterministic_replay.rs` — 9 crash-point fixtures per W0-3 §9.3 (PREPARED, SIGNED, SENDING, SENT cases a/b/c, KVT1, KVT2, ERROR_RETRYABLE).  Each fixture: pre-seed doc in target state; run `App::reconcile_pending`; assert final state matches the §6 deterministic-mapping verdict.
- Each crash-point fixture uses a DpsChannel mock pre-programmed for the §6 sub-case (e.g. SENT case (a) mock's `last_chk` returns matching id; SENT case (c) mock returns NotFound).
- §6.3 SENDING fixture is the canonical safety contract: provider-spy verifies `send_chk_count == 0` for the doc id during recovery; final state is ErrorRetryable; audit `crash_resume_sending_to_error_retryable` present.

**Acceptance.**
- [ ] 9 crash-point fixtures cover all 7 pending states (SENT has 3 sub-cases, others one each = 7+3-1=9).
- [ ] §6.3 SENDING fixture: ZERO send_chk invocations during recovery (provider-spy assertion).
- [ ] §6.6 KVT2 fixture: NO DPS query made (KVT2 is protocol-final per W0-3 §3 design constraint).
- [ ] §6.4 SENT case (c) fixture: two-step transition observed (Sent → ErrorRetryable → Sending → wire) per ADR-M3-A9 retry-path policy.
- [ ] All 9 fixtures green.

**Verify.** `cargo test -p prro --test write_path_deterministic_replay`.

**BlockedBy.** Tasks 5..9 (W5..W10) — full pipeline + boot reconciliation must be in place to drive the replay scenarios.

```json:metadata
{"files":["rust/prro/tests/write_path_deterministic_replay.rs"],"verifyCommand":"cargo test -p prro --test write_path_deterministic_replay","acceptanceCriteria":["9 crash-point fixtures cover all 7 pending states","§6.3 SENDING zero send_chk during recovery","§6.6 KVT2 no DPS query","§6.4 SENT case (c) two-step transition","9 fixtures green"]}
```

---

## Exit criteria for M3a

M3a phase is closed when ALL of the following hold:

- [ ] All 12 tasks (W0a + W1..W11) marked `completed` in `.tasks.json`.
- [ ] `cargo test -p prro` green; baseline 164 M2 tests preserved + ~80 new M3a tests across the §9 fixture set (51 from W0-2/W0-3 explicit + ~30 incidental).
- [ ] All 9 ADRs (M3-A1..A9) implemented in code; no PROPOSED/draft references in M3a code paths.
- [ ] 5 entry-decision bd issues closed: PRRO_GATE-ddn (UNIQUE + sequencer in code; W1), -zti (boundary mapping in code; W6), -k99 (WriteTxConn newtype green; W2), -6bj (DpsError routing + Pattern B SENDING green; W7+W9+W10), -ah8 (App::boot acceptance test verbatim green; W10).
- [ ] PRRO_GATE-M3a epic closed; PRRO_GATE-9qd (M3 epic) advances toward closure (M3b remaining).
- [ ] M3a handoff document drafted (mirror M2-handoff.md / M3-W0-handoff.md pattern) before M3b plan opens.

---

## What this plan does NOT do

- **Does NOT implement offline lifecycle** — open/drain/close OFFLINE_LOCAL_ACK pool is M3b per `docs/M3-W0-handoff.md` §3.
- **Does NOT extend OFFLINE_LOCAL_ACK whitelist** to 6 targets + retry self-loop — M3b extension per W0-1 §6.3.
- **Does NOT add `ix_offline_active` UNIQUE migration** — M3b blocker (M3a never opens offline sessions).
- **Does NOT implement Pattern C** ("stage and flip") — M3b reservation per W0-2 §5.3.
- **Does NOT build operator recovery UI / manual reconciliation flows** — M3b scope.
- **Does NOT build automated SENDING reconciler** (`last_chk` with cooldown / rate-limiting to resolve operator-stuck docs) — M3b per W0-3 §6.3.
- **Does NOT implement OFFLINE→ONLINE auto-flip** via `ping(fn_sign)` confirmation — M3b per W0-3 §4.3 branch (d) option (iii).
- **Does NOT touch ingress shells** (REST/XML-RPC/Maria) — M3a write-path is the staged worker; ingress wiring is a separate milestone.
- **Does NOT change the M2 frozen substrate** beyond the additive `DpsError::Authorization` extension in W4 (per ADR-M3-A6 prereq).
- **Does NOT introduce non-DPS backends** (Checkbox / Maria / sidecar profiles) — M3a is DPS gRPC only; ENCRYPTED state is out-of-scope per W0-3 §3.

---

## Companion files

- `docs/superpowers/plans/2026-05-07-m3a-implementation.md.tasks.json` — task persistence file (12 tasks: 0, 1..11; blockedBy chain per the dependency graph above).
