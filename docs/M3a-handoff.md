# M3a Handoff — exit gate before M3b implementation plan

**Status:** M3a implementation phase closed + post-handoff hardening passes 1, 2, and 3 landed.  rust-gateway HEAD = `c12ba61` (Merge PR #41 from Setter1981/m3a/boot-quickcheck-two-phase).  All 12 plan tasks marked `completed` in commit `08fc6c4`.  Full crate test surface: **463 passed / 0 failed / 1 ignored** across 27 integration test files; **21 W11 deterministic-replay fixtures green** (9 original + 8 hardening-pass-1 + 4 hardening-pass-2); 4 quick_check fail-closed fixtures un-ignored under the two-phase open path.  Only ignored entry remaining is 1 illustrative markdown ` ```ignore ` doc-test (documentation, not a deferred test).

This handoff is the **gate document** — M3b implementation plan MUST NOT open until this handoff is approved.  Mirrors the M3-W0 handoff pattern (`docs/M3-W0-handoff.md`).

**Sources cited (do not re-summarise here):**
- `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` — ADR-M3-A1..A9 (committed `8c72a14`).
- `docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md` — ADR-M3-A10 (merged `bbd9e29`).
- `docs/superpowers/specs/2026-05-06-m3-w0-{1,2,3}-*.md` — W0 research (committed `e53a440`).
- `docs/superpowers/specs/2026-05-09-m3a-w6-stage3-sign-design.md` — W6 freeze (merged `c1acbfc`).
- `docs/superpowers/specs/2026-05-09-m3a-w7-stage4-send-design.md` — W7 freeze (merged `d97178e`).
- `docs/superpowers/specs/2026-05-09-m3a-w8-stage5-finalize-design.md` — W8 freeze (merged `f2ad510`).
- `docs/superpowers/specs/2026-05-10-m3a-w10-dps-dispatch-design.md` — W10 freeze (merged `c31116c`).
- `docs/superpowers/specs/2026-05-10-m3a-w9-boot-reconciliation-design.md` — W9 freeze (merged `6fc2731`).
- `docs/superpowers/specs/2026-05-11-w10-final-audit.md` + `2026-05-11-med1-lease-scope-design.md` — W10 audit + MED-1 design (merged `bbd9e29`).
- `docs/superpowers/specs/2026-05-12-w11-deterministic-replay-design.md` — W11 design freeze + operator decisions Q1..Q4.

---

## 1. M3a PR ladder (chronological)

Every M3a code change landed via `gh pr merge --merge` (regular merge commit; preserves the per-PR ladder in `git log --merges`).  23 PRs total through hardening pass 3: 16 W-task PRs + 1 CI infra fix (#20) + 1 docs-only (#28) + 1 handoff (#37) + 1 hardening pass 1 (#38) + 1 handoff post-hardening (#39) + 1 hardening pass 2 (#40) + 1 hardening pass 3 (#41).

| PR | Branch | Merge commit | W-task | Summary |
|----|--------|--------------|--------|---------|
| #19 | `m3a/W1-migrations` | `42a94e9` | W1 | Migrations 007 (UNIQUE fn,lnd) + 008 (DocState::Sending) + whitelist additions |
| #20 | `infra/cert-fixture-vendoring` | `1d2620d` | — | CI infra fix: cert fixture vendoring |
| #21 | `m3a/W4-dps-auth` | `110a092` | W4 | `DpsError::Authorization { code, kind, message }` + `AuthorizationKind` enum (M2/W3 additive amendment) |
| #22 | `m3a/W2-write-txconn` | `7ec8e75` | W2 | `WriteTxConn<'_>` sealed newtype + `transition_state` / `shifts::transition` signature change |
| #23 | `m3a/W3-with-immediate-enforcement` | `4d35a3f` | W3 | `with_immediate` hybrid enforcement: `tokio::task_local!` IN_WITH_IMMEDIATE + AST static scan |
| #24 | `m3a/W5-stages-1-2` | `ca0357a` | W5 | Write-path stages 1+2 — acquire+validate + guard |
| #25 | `m3a/W6-stage3-sign` | `c1acbfc` | W6 | Write-path stage 3 — sign (Pattern A) |
| #26 | `m3a/W7-send-routing` | `d97178e` | W7 | Write-path stage 4 — send (Pattern B with SENDING marker) |
| #27 | `m3a/W8-stage5-finalize` | `f2ad510` | W8 | Write-path stage 5 — finalize |
| #28 | `docs/W8-review-section` | `1d29315` | — | W8 review-section docs landing |
| #29 | `m3a/W10-dps-dispatch` | `c31116c` | W10 | DpsError routing dispatch — 8-variant + 12-status-code table + MAC recovery |
| #30 | `m3a/W9-boot-recovery` | `6fc2731` | W9 | `App::boot` reconciliation phase — 6-branch per-FN decision tree |
| #31 | `m3a/med1-lease-rename` | `bbd9e29` | — | W10 audit MED-1 close: ADR-M3-A10 + single-writer-per-FN invariant rename |
| #32 | `m3a/W11-deterministic-replay` | `b81f3c4` | W11/PR-1a | `reconcile_pending_with` + `ReconciliationRuntime` + SENDING fixture #3 |
| #33 | `m3a/W11-pr1b` | `dcab82f` | W11/PR-1b | KVT2 fixture #8 + KVT1 corrected fixture #7 |
| #34 | `m3a/W11-pr2a` | `1a8b4f5` | W11/PR-2a | SIGNED + ERROR_RETRYABLE wirings + fixtures #2 + #9 |
| #35 | `m3a/W11-sent-rm-edge` | `669dbe3` | W11 prep | `(Sent, RequiresManualReconciliation)` whitelist edge for §6.4-b operator handoff |
| #36 | `m3a/W11-pr2b-runtime` | `a7369b9` | W11/PR-2b | SENT 3-way + PREPARED + fixtures #1/#4/#5/#6 (W11 closing PR) |
| #37 | `m3a/handoff` | `79ca2d6` | — | M3a handoff doc draft + plan tasks.json status flip |
| #38 | `m3a/hardening-pass-1` | `acba165` | — | Post-handoff hardening pass 1: H1 ER retry_class filter + H2 budget cap + H3 per-FN resolver + M1 PREPARED drift detection (closes 3 HIGH + 1 MED) |
| #39 | `m3a/handoff-post-hardening` | `c3f9a71` | — | Handoff doc post-hardening-pass-1 updates (PR ladder, A6/A8 verdicts, M2/M3 carry-forward) |
| #40 | `m3a/hardening-pass-2` | `97d3e10` | — | Post-handoff hardening pass 2: H1 latest-attempt authoritative (duplicate-send hazard fix) + ADR-M3-A10 structural mutex + PREPARED payload_json byte-equality drift |
| #41 | `m3a/boot-quickcheck-two-phase` | `c12ba61` | — | Post-handoff hardening pass 3: `quick_check` two-phase open before migrations (closes Finding 2; un-ignores the 4 W9.1-era corruption fixtures via probe-only Phase A) |

Plus one chore commit on top of `a7369b9`: `08fc6c4` (`chore(plan/m3a): mark W6/W7/W8/W10/W9/W11 completed`) — surgical plan tasks.json status flip + lastUpdated bump; no content rewrite.

---

## 2. Final test surface

`cargo test -p prro` on `rust-gateway` post-`c12ba61` (M3a + hardening passes 1, 2, 3):

- **463 passed / 0 failed / 1 ignored.**
- 27 integration test files plus the lib unit-test surface and 1 trybuild driver.
- W3 static scanner test (`tests/with_immediate_no_foreign_io.rs`) — 8 / 0 / 0 green; production source has zero foreign IO inside any `with_immediate` body.
- App-boot integrity probe (`tests/app_boot_quick_check_failure.rs`) — **4 / 0 / 0** green (hardening pass 3 un-ignored the four W9.1-era corruption fixtures and consolidated them under the two-phase open):
  - `quick_check_ok_proceeds_to_reconcile` — clean existing DB happy path.
  - `quick_check_fail_returns_typed_error` — corruption-on-existing-DB returns `BootError::IntegrityCheckFailed`.
  - `quick_check_fail_main_file_bytes_unchanged_no_domain_writes` — mechanism-independent byte-equality proof on the main DB file across the failed boot path (`sha256` before vs after).  Explicit carve-out: WAL/SHM sidecar metadata touches are out of assertion scope — `journal_mode = WAL` PRAGMA may legitimately touch sidecars on every connection without indicating domain DML; the contract is "no migrations / no persisted domain writes before fail-closed return", proven by main-file byte equality.
  - `fresh_db_boots_through_migrations_with_post_quick_check` — positive fresh-DB path: missing file → Phase A skipped → Phase B creates + migrates → post-migrate quick_check passes.
- W11 deterministic-replay (`tests/write_path_deterministic_replay.rs`) — **21 / 0 / 0** green:
  - **Original M3a fixtures (PR #32–#36):**
    - #1 PREPARED — `dispatch_prepared_via_chain` drives sign + send chain to SENT.
    - #2 SIGNED — `stage_send::run` drives Signed → Sending → Sent on happy `send_chk`.
    - #3 SENDING — Pattern B no-resend (`send_chk_count == 0` during recovery).
    - #4 SENT/§6.4-a — `last_chk` Match → KVT1 + KVT1_RAW from `ack.data_sign`.
    - #5 SENT/§6.4-b — `last_chk` Mismatch → RequiresManualReconciliation (operator handoff via PR #35 whitelist edge).
    - #6 SENT/§6.4-c — `last_chk` NotFound → ErrorRetryable (tick 1) → SENT via `stage_send::run` (tick 2); **explicit two-tick driver per ADR-M3-A9 step 3**.
    - #7 KVT1 — passive hold (no DPS query; M3b active-poll deferred).
    - #8 KVT2 — `stage_finalize::run` drives Kvt2 → Ack without DPS query (protocol-final).
    - #9 ERROR_RETRYABLE — happy retry without MAC budget burn (now with explicit `attempts_used < MAX_BOOT_ATTEMPTS` boundary comment).
  - **Hardening pass 1 fixtures (PR #38):**
    - `multi_fn_reconcile_pending_with_resolves_runtime_per_fn` — LOAD-BEARING H3 proof; two FNs + two distinct `fn_sign` blobs; `PerFnRecordingDpsStub` records the blob bytes of every `last_chk` and asserts each FN received its own identity (no foreign-identity leak).
    - `fixture_9b_er_fn_config_error_escalates_to_manual_reconciliation` — H1 closure: FnConfigError → CAS ER → RM + Severity::Error.
    - `fixture_9c_er_probe_required_defers_with_audit` — H1: ProbeRequired → hold + Severity::Warning; submit-time `last_chk` reconciliation correctly deferred to M5 (per PRRO_GATE-6bj annotation).
    - `fixture_9d_er_indeterminate_retry_class_defers_with_audit` — H1: missing/NULL retry_class → hold + Severity::Error (durable evidence missing).
    - `fixture_9e_er_terminal_reject_escalates_critical` — H1: TerminalReject in ER (structural skew) → CAS ER → RM + Severity::Critical.
    - `fixture_9f_resolver_none_defers_with_audit` — H3 negative: per-FN resolver returns `None` → recovery falls through to deferred path, NEVER substitutes foreign identity.
    - `fixture_9g_er_transient_retry_budget_exhausted_escalates` — H2 closure: 5 seeded transport_trace rows → `attempts_used >= MAX_BOOT_ATTEMPTS` → CAS ER → RM + `BOOT_ER_BUDGET_EXHAUSTED` Severity::Error + zero DPS.
    - `fixture_1b_prepared_replay_drift_holds_with_critical_audit` — M1 closure: mismatched inbox/fiscal_documents payload hash → state stays PREPARED + `BOOT_PREPARED_REPLAY_DRIFT` Severity::Critical + zero sign/send.
  - **Hardening pass 2 fixtures (PR #40):**
    - `fixture_9h_er_latest_unfinished_trace_holds_no_send` — single-boot proof of H1 closure (latest unfinished `transport_trace` row dominates over older completed TransientRetry; routes to indeterminate hold instead of duplicate-send).
    - `fixture_3b_sending_crash_after_transient_retry_second_boot_no_resend` — **LOAD-BEARING** two-boot end-to-end proof: boot 1 SENDING→ER, boot 2 ER + unfinished latest trace → indeterminate hold; combined `send_chk_count == 0` across BOTH ticks.
    - `concurrent_reconcile_pending_with_same_app_serializes` — ADR-M3-A10 structural enforcement via `tokio::sync::Mutex` on `App::Inner::reconcile_mutex`.  `SequenceProbingDpsStub` records peak in-flight `last_chk`; under the mutex `max_in_flight == 1`.
    - `fixture_1c_prepared_replay_payload_json_drift_holds` — M1/F4 closure: extends pass-1 drift detection with explicit `payload_json` byte-equality (catches drift where hash column was not updated when payload was).

The 1 ignored entry:
- **1 illustrative doc-test** in `rust/prro/src/services/write_path/types.rs` line 119 (`services::write_path::types::bridge_anyhow_to`).  Markdown ` ```ignore ` code-fence inside the doc-comment showing a one-liner usage pattern; documentation-only, NOT a deferred test, and not an M3a gate.

The four W9.1-era `#[ignore]`d corruption fixtures were un-ignored under hardening pass 3 (two-phase open); their semantics now live in the 4 active `app_boot_quick_check_failure.rs` fixtures above.

CI: 5 platforms green on `rust-gateway` HEAD (`fmt + clippy (gnu)`, `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc`, `aarch64-unknown-linux-gnu`).

---

## 3. Closed contracts (anchored, non-negotiable)

### 3.1 ADR amendments A1–A10

| ADR | Status | Closure proof |
|-----|--------|----------------|
| **A1** | implemented | `node_state.next_lnd` transactional sequencer (W5 / `stage_acquire::allocate_next_lnd`); UNIQUE(fiscal_number, lnd) migration 007 (W1). |
| **A2** | implemented | `wire_artifact_kind` derivation BEFORE Z-allocation in `stage_sign::derive_wire_artifact_kind` (W6).  `tests/write_path_stage3_sign.rs` proves both `ShiftClose` and `ZReport` map to `WireArtifactKind::ZReport`. |
| **A3** | implemented | `tokio::task_local!` IN_WITH_IMMEDIATE + AST static scanner in `tests/with_immediate_no_foreign_io.rs` (W3); runtime guard `assert_not_in_with_immediate` at every M2 substrate entry. |
| **A4** | implemented | `WriteTxConn<'_>` sealed newtype with module-private `fn new` + 4 trybuild compile-fail fixtures (W2). |
| **A5** | implemented | Pattern A at stage 3 (W6 `stage_sign::run` — sign outside, persist inside); Pattern B mandatory at stage 4 (W7 `stage_send::run` — 4-pre / 4a / 4b with SENDING marker). |
| **A6** | implemented + consumed + duplicate-send hazard closed | `error_routing::route_send_result` table-driven dispatch (W10); 21 routing fixtures + MAC recovery -12 fixture green.  **Hardening pass 1 (PR #38):** `dispatch_error_retryable_by_class` now consumes `transport_trace.retry_class` durable label at recovery time — TransientRetry → `stage_send::run`; non-transient classes (FnConfigError / WrapperBug / OperatorEscalation / MacRecovery / TerminalReject) → CAS ER → RequiresManualReconciliation; ProbeRequired / None → hold without state change.  `MAX_BOOT_ATTEMPTS` budget cap enforced via `attempts_used()` gate before TransientRetry dispatch.  Closes the previously documented `stage_send.rs:33-40` "crash-loop hazard".  **Hardening pass 2 (PR #40):** `last_attempt_retry_class_for` drops the `WHERE completed_at IS NOT NULL` filter — latest attempt by `attempt_no` is now authoritative regardless of completion state.  Unfinished latest trace (`retry_class = NULL`) routes to indeterminate hold; closes the across-boot duplicate-send hazard that emerged from a SENDING crash + ER recovery sequence (fixtures `9h` + `3b`). |
| **A7** | implemented + integrity gate hardened | `App::boot` 6-branch decision tree in `boot_phase::run_boot_reconciliation` (W9); 9 fixtures in `tests/app_boot_reconciliation.rs`; PRRO_GATE-ah8 verbatim acceptance fixture green.  **Hardening pass 3 (PR #41):** `App::boot` now runs `PRAGMA quick_check(1)` BEFORE `sqlx::migrate!` on existing DBs via the new `db::open_pool_no_migrate` probe path.  4 W9.1-era `#[ignore]`d corruption fixtures un-ignored and consolidated under the two-phase open; fail-closed-no-domain-writes contract proven by main-file `sha256` byte-equality across the failed boot path. |
| **A8** | implemented + unhappy paths covered + cross-boot crash-safety closed | `list_pending_for_fn` whitelist 7→8 with `Sending`; intentional whitelist gaps preserved (`intentional_whitelist_gaps_remain_forbidden`).  **Hardening pass 1 (PR #38):** deterministic-replay surface extended to 17 fixtures covering non-transient ErrorRetryable classes (9b/9c/9d/9e/9f/9g), multi-FN per-FN resolver (`multi_fn_...`), and PREPARED replay drift detection (1b).  **Hardening pass 2 (PR #40):** surface extended to 21 fixtures with cross-boot crash-safety (9h latest-unfinished, 3b two-boot SENDING+ER zero-resend), App-scoped serialisation proof (`concurrent_...`), and `payload_json` byte-equality drift (1c).  Previous "happy paths only" and "single-boot only" gaps both closed. |
| **A9** | implemented | `DocState::Sending` value + migration 008 + Pattern B crash-resume (CAS Sending → ErrorRetryable, **never** auto re-send).  Fixture #3 proves zero `send_chk` during SENDING recovery; fixture #6 proves the two-tick path through ErrorRetryable (NO direct `Sent → Sending`). |
| **A10** | structurally enforced at App level | ADR-M3-A10 codifies the M3a global-single-writer invariant; docstring rename `lease` → `invariant`; smoke test pins ADR existence (PR #31).  **Hardening pass 2 (PR #40):** structural enforcement landed via `tokio::sync::Mutex<()>` on `App::Inner::reconcile_mutex`; `reconcile_pending_inner` acquires the mutex as its first line, so two concurrent callers on the same `App` (or distinct `Arc<Inner>` clones) serialise instead of racing through per-row CAS + per-FN envelopes.  Fixture `concurrent_reconcile_pending_with_same_app_serializes` asserts `max_in_flight == 1`.  **Residual** (carry-forward to multi-worker slices): direct `boot_phase::run_boot_reconciliation` callers bypass the App mutex (entry is `pub`); acceptable for the single-task M3a pilot since production callers go through `App::reconcile_pending_with`.  See §5.1. |

### 3.2 Pattern A / Pattern B

- **Pattern A** (stage 3 sign): chain-pin in 3-PRE `with_immediate`, crypto outside, persist (CAS Prepared → Signed + PAYLOAD_XML + SIGNED_XML + audit) in 3-PERSIST `with_immediate`.  Timestamp ordering proof: `test_hook::COUNTER` + spy crypto provider; sign call seq < persist first stmt seq, structurally.
- **Pattern B** (stage 4 send): 4-pre `with_immediate` (CAS Signed/Encrypted/ErrorRetryable → Sending + allocate `transport_trace` + `submission_attempted_at`) → wire `send_chk` OUTSIDE any envelope → 4-b `with_immediate` (post-wire CAS Sending → Sent/routed + `set_server_fiscal_no_tx` + `transport_trace::complete_tx` + audit).  W3 scanner enforces structural separation: `send_chk` is never reached from inside `with_immediate`.

### 3.3 Deterministic-replay invariant (W11 + hardening passes 1, 2, 3)

W0-3 §6 mandates: for every pending `DocState`, `App::reconcile_pending(_with)` converges to the same final state whether the prior process crashed mid-pipeline or completed uninterrupted.  Original PR-1a..PR-2b proved this across all 7 pending states + the 3 SENT sub-cases (a / b / c); **hardening pass 1 (PR #38)** extended coverage to non-transient ErrorRetryable classes, multi-FN identity binding, and PREPARED replay drift detection.  **Hardening pass 2 (PR #40)** closed the cross-boot duplicate-send hazard (latest-attempt authoritative) + landed structural single-writer enforcement + `payload_json` byte-equality drift.  **Hardening pass 3 (PR #41)** moved the integrity gate (`PRAGMA quick_check(1)`) ahead of `sqlx::migrate!` so a corrupted existing DB fails closed without any migration / domain write.

Critical structural assertions (all proven by fixtures on `rust-gateway`):

- **§6.3 Pattern B no-resend** — SENDING recovery does NOT invoke `send_chk` (fixture #3).
- **§6.4-c two-tick contract** — SENT/NotFound recovery hops through ErrorRetryable; the retry happens on a separate boot tick with a new `ReconciliationRuntime`; **never** a direct `Sent → Sending` edge (fixture #6).
- **§6.5 KVT1 passive hold** — KVT2-receipt API not exposed by `DpsChannel` in M3a; active polling deferred to M3b (fixture #7).
- **§6.6 KVT2 protocol-final** — `stage_finalize::run` drives Kvt2 → Ack without DPS query (fixture #8).
- **Hardening §H1 retry_class consumption** — `dispatch_error_retryable_by_class` reads `transport_trace.retry_class` and routes per class: TransientRetry → wire; FnConfigError / WrapperBug / OperatorEscalation / MacRecovery / TerminalReject → CAS to RequiresManualReconciliation; ProbeRequired / None → hold (fixtures 9b/9c/9d/9e).
- **Hardening §H2 budget cap** — TransientRetry path gated on `attempts_used(doc) >= MAX_BOOT_ATTEMPTS=5`; budget exhaust → CAS to RequiresManualReconciliation + `BOOT_ER_BUDGET_EXHAUSTED` Severity::Error (fixture 9g).
- **Hardening §H3 per-FN identity binding** — `ReconciliationRuntime` enum (`SingleFn` / `PerFn(resolver)`) resolves a `RuntimeView` per FN inside `reconcile_pending_inner`; resolver returning `None` falls through to deferred path; recovery NEVER substitutes foreign identity (fixtures `multi_fn_...` + 9f).
- **Hardening §M1 PREPARED replay drift** — `dispatch_prepared_via_chain` snapshot envelope cross-checks `(fd.fiscal_number, fd.payload_sha256_canonical, fd.doc_type, fd.payload_json)` against the matching inbox fields; mismatch → hold + `BOOT_PREPARED_REPLAY_DRIFT` Severity::Critical (fixtures 1b + 1c).
- **HP2 §H1 latest-attempt authoritative** — `last_attempt_retry_class_for` returns the latest attempt by `attempt_no` regardless of completion state; unfinished latest trace → indeterminate hold, NOT duplicate-send via stale completed TransientRetry (fixtures 9h + 3b).
- **HP2 §A10 structural** — `App::Inner::reconcile_mutex` (`tokio::sync::Mutex`) acquired as the first line of `reconcile_pending_inner`; concurrent callers on the same `App` serialise instead of racing through per-row CAS + per-FN envelopes (fixture `concurrent_..._serializes`).
- **HP3 quick_check fail-closed before migrations** — `App::boot` runs `PRAGMA quick_check(1)` on existing DBs via `db::open_pool_no_migrate` BEFORE `sqlx::migrate!`; corruption returns `IntegrityCheckFailed` with main DB file byte-equality across the failed boot (`quick_check_fail_main_file_bytes_unchanged_no_domain_writes`).

---

## 4. bd issues — M3a implementation proof vs full closure scope

The 5 entry-decision bd issues each have an M3a-scoped portion that is **demonstrably closed by code on `rust-gateway`** and, for two of them, a **non-M3a residual scope** (Python-stack audit / retry-pacing / offline-id path) that MUST stay open or be split before closing the parent bd issue.  Read the table as "implementation proof landed", NOT "issue ready to close".

| bd | M3a-scoped proof on `rust-gateway` | Residual scope outside M3a | Recommended action |
|----|-------------------------------------|----------------------------|--------------------|
| **PRRO_GATE-ddn** | UNIQUE migration `007_lnd_unique.sql` (W1) + `next_lnd` sequencer via `node_state::allocate_next_lnd` (W5).  `tests/migrations_007_008.rs` + `tests/write_path_stage1_acquire.rs::stage1_unique_fn_lnd_collision_fails_closed` green. | — (M3a-scoped issue; acceptance fully covered.) | **Closeable** with "superseded by W1 + W5 implementation at `a7369b9`" comment. |
| **PRRO_GATE-zti** | `stage_sign::derive_wire_artifact_kind` maps `ShiftClose` and `ZReport` to `WireArtifactKind::ZReport` at the W6 Rust builder boundary; ZReport-only fixtures in `tests/write_path_stage3_sign.rs`. | The bd acceptance text also lists: (1) audit of Python `src/prro_gateway/transports/` for `SHIFT_CLOSE` references; (2) audit of Python `src/prro_gateway/services/`; (3) schema audit of `fiscal_documents.doc_type` for what's currently stored.  These are Python-stack items, intentionally out of scope under ADR-D1 (Rust-only pilot path). | **Cannot stay open under `9qd.1`.**  Pick one before closing the epic: (zti-A) retitle to "M3a Rust builder boundary mapping (Python adapter audit superseded by ADR-D1)" and `bd close zti`, OR (zti-B) `bd update zti --parent <downstream-epic>` to remove from under `9qd.1` and keep `open` as a historical record.  Either path satisfies the §6.2 closure rule. |
| **PRRO_GATE-k99** | `WriteTxConn<'_>` sealed newtype in `db/tx.rs` (W2); 4 trybuild compile-fail fixtures + `transition_state_atomicity` 2/2 green. | — (M3a-scoped issue; acceptance fully covered.) | **Closeable** with "superseded by W2 implementation at `a7369b9`" comment. |
| **PRRO_GATE-6bj** | `error_routing::route_send_result` (W10) — 21 routing fixtures + MAC recovery -12 fixture green.  `DocState::Sending` + Pattern B crash-resume rule in W11 fixture #3.  W11 fixture #6 proves `last_chk` reconciliation on SENT crash.  **Hardening pass 1 (PR #38) additionally covers:** boot-time retry_class consumption (fixtures 9b/9c/9d/9e) + `MAX_BOOT_ATTEMPTS` budget cap (fixture 9g) — `dispatch_error_retryable_by_class` reads `transport_trace.retry_class` and gates on `attempts_used()` before TransientRetry → `stage_send::run`. | **Narrowed after hardening pass 1.**  M5 residual scope: (1) submit-time `last_chk` reconciliation on -15/0 (recovery covers boot-time only; live-worker variant `SubmitPtr.cs:50` describes still M5); (2) retry-pacing / exponential backoff between boot ticks (M5 generic SENDING reconciler — recovery currently retries at every boot if attempts_used < MAX, no inter-tick spacing).  M3b residual: status `-16` offline-id / technical-offline path decision (offline lifecycle). | **Resolved 2026-05-13 via 6bj-B + post-hardening narrowing:** `bd update 6bj --parent PRRO_GATE-9qd.4` (M5) already applied; M5 scope now formally narrowed by PR #38 closure of boot-time retry_class consumption + budget cap.  M3b sub-item (`-16` offline-id) carried as cross-link annotation. |
| **PRRO_GATE-ah8** | `tests/app_boot_reconciliation.rs::ah8_shift_state_opened_preserved_across_boot` green (PRRO_GATE-ah8 verbatim acceptance fixture). | — (M3a-scoped issue; acceptance fully covered.) | **Closeable** with "superseded by W9 implementation at `a7369b9`" comment. |

**Cross-link items:**
- **PRRO_GATE-9qd.1** (M3a epic) — closure requires **zero open children under 9qd.1** at the time of `bd close`.  bd refuses to close an epic that still has open children, so any child that stays `open` MUST be re-parented away from 9qd.1 before the close.  Resolution per §6.2: ddn / k99 / ah8 close cleanly; zti / 6bj either close-after-retitle-or-split OR re-parent to the appropriate downstream epic (`9qd.2`/M3b, `9qd.4`/M5, or a new carry-forward epic); the two P3 follow-ups (`9qd.1.1` / `9qd.1.2`) follow the same close-or-re-parent rule.
- **PRRO_GATE-iap** (COM/1C compat) — pilot decision; ADR-M3-A2 preserves the constraint but does NOT close the issue.  Stays open into M3b/M4.

**Procedural note:** the user explicitly asked NOT to close `PRRO_GATE-9qd.1` "по отчёту" — closure happens only after physical verification each child is `closed` OR explicitly superseded by implementation proof.

---

## 5. Remaining risks / explicit defers

### 5.1 Remaining technical debt (in-code, not blockers)

| Risk | Location | Justification | When to address |
|------|----------|----------------|-----------------|
| Inline raw SQL in PREPARED snapshot read | `boot_phase::dispatch_prepared_via_chain` step (1a)/(1b) | Sole caller; no second reader yet; minimal-diff per PR-2b scope.  Adds 2 raw `sqlx::query` invocations (fiscal_documents payload extras + ingress_inbox by request_id). | If a second reader of inbox by request_id emerges OR a second reader of fiscal_documents payload extras emerges in M3b — promote to repo helpers (`ingress_inbox::get_by_request_id_tx`, `fiscal_documents::DocumentRow` extension). |
| `DocumentRow` payload-extras gap | `db::repositories::fiscal_documents::DocumentRow` | Doesn't carry `request_id`, `business_ts`, `total_sum_kop`, `payload_json`, `payload_sha256_canonical` despite all being NOT NULL on the schema.  Recovery and any future readers must raw-SELECT to retrieve them. | When `DocumentRow` callers grow past the current ~12 read sites OR when a non-recovery code path needs the same extras. |
| Inbox `protocol` enum decoded via runtime `try_get::<Protocol, _>` | Same boot_phase snapshot read | Works (sqlx::Type round-trip) but bypasses the offline-prepared `query!` macro path the rest of `ingress_inbox.rs` uses. | Bundled with the promote-to-helper item above. |
| **Runtime W3 guard is `debug_assert!` only** | `db/tx.rs:64-70` `assert_not_in_with_immediate` | Static scanner (`tests/with_immediate_no_foreign_io.rs`) exhaustive over `src/` at compile time; runtime guard compiles to no-op in release.  Mitigated by the static scanner's inline-closure + UFCS + spawn_blocking detection.  Reviewer-classified LOW-MEDIUM. | Optional: either add CI job `cargo test -p prro --release` to keep the runtime guard live in CI, OR promote `debug_assert!` → `assert!` (paid cost: one task-local check per substrate call).  Document choice in `db/tx.rs` module docs.  Not blocking single-task pilot. |
| **Direct `boot_phase::run_boot_reconciliation` bypasses App reconcile mutex** | `services/reconciliation/boot_phase.rs` (public entry); App-scoped mutex on `App::Inner::reconcile_mutex` does not extend to direct callers of `run_boot_reconciliation`. | Hardening pass 2 (PR #40) added `tokio::sync::Mutex` inside `App::Inner` and gates `App::reconcile_pending_inner` behind it — covers production callers that go through `App::reconcile_pending_with`.  The public `boot_phase::run_boot_reconciliation` entry remains accessible to ops scripts / future test harnesses without acquiring the mutex.  Reviewer-classified LOW for the M3a single-task pilot (no production caller skips `App::reconcile_pending_with`). | Promote `boot_phase::run_boot_reconciliation` to `pub(crate)` + restrict the call surface, OR require callers to acquire an explicit lock token.  Pre-requisite to any multi-worker dispatcher slice (per ADR-M3-A10 §4 carry-forward). |
| **HP3 `quick_check_fail_main_file_bytes_unchanged_no_domain_writes` does not assert WAL/SHM sidecar untouched** | `tests/app_boot_quick_check_failure.rs` | Production code does not run migrations / persisted domain DML before the fail-closed return, so `sha256(main_db_file)` byte-equality is sufficient for the Finding-2 closure contract (no migrations, no domain writes before `BootError::IntegrityCheckFailed`).  Sidecar files (`*-wal` / `*-shm`) are touched by SQLite's `journal_mode = WAL` PRAGMA on every connection — including the Phase A probe — independently of domain DML.  Asserting "sidecar size unchanged" would be platform-dependent and brittle. | Carve-out is documented in the fixture's docstring (hardening pass 3 post-merge tightening).  No code change planned; sidecar touches are operational noise, not a safety contract. |
| **Canonical hash recompute on PREPARED replay** | `services/reconciliation/boot_phase.rs::dispatch_prepared_via_chain` snapshot envelope | Hardening pass 2 added explicit `fd.payload_json == inbox.payload_json` byte-equality check.  An additional `sha256(payload_json)` recompute + comparison against `payload_sha256_canonical` would catch single-side mutation between `payload_json` and the hash column, but requires knowing the canonicalization function — and no `payload_json` canonicalization helper exists in the M3a Rust write_path tree.  `payload_sha256_canonical` is an external contract supplied by the ingress adapter chain. | Land alongside the M4 ingress adapter wiring, which is where the canonicalization function will live. |

### 5.2 Cross-M3a-boundary defers (M3b / M4 / M5 / M6) — unchanged from M3-W0 §3

Carried forward verbatim (no rescope at M3a exit).  Summary; full text in `docs/M3-W0-handoff.md` §3.

- **M3b — Phase-6-min offline subsystem (Rust):** OFFLINE_LOCAL_ACK whitelist extension + `ix_offline_active` UNIQUE migration + offline session lifecycle + Pattern C (stage-then-flip) + auto-flip OFFLINE → ONLINE via `ping(fn_sign)`.  Active KVT2 polling via `status_rro` lives here.
- **M4 — Rust ingress + transport bridges:** REST / XML-RPC / Maria-shell Rust ingresses; `maria304_driver` re-target from Python `reqwest::blocking` to in-process Rust gateway; 1С OLE bridge subsystem.
- **M5 — services tail:** ingress writer (replaces Python `services/ingress.py`); generic `SENDING`-state reconciler (`last_chk` with cooldown / rate-limiting for operator-stuck docs — NOT M3b; not Phase-6-gated); long-running post-boot reconciliation hooks; operator manual-reconciliation CLI hooks; `cert_provisioning` (gated by ADR open item O2); `retention` + `shift_aggregation` (gated by O3).
- **M6 — admin surface:** `prro_admin` CLI subcommands (`status`, `set-config`, `cert show/rotate`, `node-state show/set`, `manual-reconcile <doc-id>`) — pilot delivers CLI only.  Web admin UI deferred post-pilot.

### 5.3 Pilot prerequisites running parallel to code milestones (ADR D3)

Unchanged from M3-W0 §3.5: `OPERATIONS.md` runbooks (backup / restore / key rotation / rollback rehearsal); M3a-end ONLINE-against-test-DPS smoke (mandatory unless owner-waived); closure of ADR open items O1 (1С OLE scope) / O2 (cert provisioning) / O3 (retention depth) before M4 / M5 sizing.

---

## 6. M3b entry gate

**M3b implementation plan MUST NOT be drafted until this handoff is approved.**  Specifically:

### 6.1 User approval of M3a exit posture (this document §1–§5)

The PR ladder, test surface, closed contracts, and bd-closure mapping are PROPOSED — they become committed M3a closure only after explicit user GO.  Approval can be all-or-nothing or per-section; deferred items stay flagged and the corresponding M3b prep tasks fall out of scope.

### 6.2 bd hygiene

**Closure rule.**  bd refuses to close an epic that still has open children.  Therefore `PRRO_GATE-9qd.1` cannot close while any of its children is `open` — every open child MUST either close OR re-parent away from `9qd.1` to a downstream epic (M3b / M4 / M5 / a new carry-forward epic) before the parent close.  "Open child remains under 9qd.1" + "9qd.1 closed" is not a reachable state.

Step-by-step before closing `PRRO_GATE-9qd.1`:

1. **Resolve the 5 entry-decision children** per the §4 recommendations.  Each MUST end up either `closed` OR re-parented away from `9qd.1`.
   - `PRRO_GATE-ddn` / `PRRO_GATE-k99` / `PRRO_GATE-ah8` — `bd close <id>` with "superseded by [W1+W5 / W2 / W9] implementation at `a7369b9`" comment.
   - **`PRRO_GATE-zti`** — pick exactly one:
     - (zti-A) `bd update zti --title "M3a Rust builder boundary mapping (Python adapter audit superseded by ADR-D1)"` and `bd close zti` with a comment pointing at W6 `derive_wire_artifact_kind` + ADR-D1 supersession.
     - (zti-B) `bd update zti --parent PRRO_GATE-9qd.4` (or a new carry-forward epic) with a comment that the residual Python audit no longer applies under ADR-D1 and the issue is being preserved only as a historical record; keep `open`.  This removes zti from under 9qd.1, allowing 9qd.1 to close.
   - **`PRRO_GATE-6bj`** — pick exactly one:
     - (6bj-A) Split into 4 child issues per acceptance bullet (`-3` retry/backoff → M5, submit-time lastChk on `-15`/`0` → M5, `-16` offline-id → M3b, plus the W10/W11-covered routing+boot-lastChk slice), close the M3a-covered slice, re-parent the rest to their respective downstream epics, then `bd close 6bj` with a "split-and-superseded" comment.
     - (6bj-B) `bd update 6bj --parent PRRO_GATE-9qd.4` (M5 — retry/backoff + submit-time lastChk dominate the residual scope; `-16` offline-id is acknowledged as a sub-item carried separately to M3b) with an annotation listing the M3a-covered portion and the deferred bullets; keep `open`.  This removes 6bj from under 9qd.1, allowing 9qd.1 to close.
2. **Resolve the 2 P3 W6-follow-up children** (`PRRO_GATE-9qd.1.1` document_files `query!` macro migration, `PRRO_GATE-9qd.1.2` feature-gate `test_hook`).  Same close-or-re-parent rule:
   - Either `bd close <id>` with "carry-forward to dedicated cleanup PR" comment, OR `bd update <id> --parent <new-cleanup-epic>` to remove from under `9qd.1`.  Both are non-functional code-hygiene items per `project_m3a_starting_point` memory's carry-forward section.
3. **Verify the M3 parent epic `PRRO_GATE-9qd` real phase chain.**  Current actual state (verified `bd list --parent PRRO_GATE-9qd`): the M-series chain `PRRO_GATE-9qd.{1,2,3,4,5}` (M3a, M3b, M4, M5, M6) is **already declared** as a single nested phase chain.  Confirm:
   - `9qd.1` (M3a) → ready to close once §6.2.1 + §6.2.2 resolved (zero open children).
   - `9qd.2` (M3b) → stays open / blocked as next phase.
   - `9qd.3` (M4) / `9qd.4` (M5) / `9qd.5` (M6) → preserved as placeholders for downstream sizing (ADR open items O1 / O2 / O3 are the bottleneck on filling these in — see §6.4).
   - No additional M3a child should be created on top of this structure; the chain is correct as-is.  Re-parenting from §6.2.1 / §6.2.2 attaches to these existing downstream epics, not new ones.
4. **Final-state verification:** `bd list --parent PRRO_GATE-9qd.1` MUST return zero rows before `bd close 9qd.1`.  If it does not, repeat §6.2.1 / §6.2.2 for the residue.
5. Comment `PRRO_GATE-9qd.1` with the handoff commit hash (from this PR, after merge) + this document path; `bd close 9qd.1`.

### 6.3 ONLINE-against-test-DPS smoke (ADR D3 gate #4)

Mandatory if a non-production DPS contour is available.  Memory `project_sprint7_complete` already records a successful full live DPS cycle (SHIFT_OPEN → SELL → Z_REPORT on `cabinet.tax.gov.ua:9443`); that artifact is sufficient evidence for the ADR-D3 gate if the operator agrees to map Python-stack Sprint-7 evidence onto Rust-stack M3a exit.  Otherwise discharge by explicit owner waiver committed before M3b.

### 6.4 ADR open items O1 / O2 / O3

Closure of O1 (1С OLE scope) / O2 (onboarding / cert provisioning automation) / O3 (retention + shift aggregation depth) is REQUIRED before M4 / M5 sizing — these decisions are the bottleneck on plan writing for those milestones.  Pinging them at M3a exit is the right cadence.

### 6.5 Worktree cleanup

All hardening worktrees produced during the M3a closure cycle have been removed at their respective merge gates:
- `/mnt/d/PRRO_GATE-m3a-w11-pr2b-runtime` — removed post-PR #36 merge.
- `/mnt/d/PRRO_GATE-m3a-hardening` — removed post-PR #38 merge.
- `/mnt/d/PRRO_GATE-m3a-hardening-pass-2` — removed post-PR #40 merge.
- `/mnt/d/PRRO_GATE-m3a-boot-quickcheck` — removed post-PR #41 merge.

Local feature branches (`m3a/W11-pr2b-runtime`, `m3a/W11-pr1b`, `m3a/hardening-pass-1`, `m3a/handoff-post-hardening`, `m3a/hardening-pass-2`, `m3a/boot-quickcheck-two-phase`) — deleted at the corresponding cleanup gates.

Other older `m3a/*` feature branches (`m3a/W1-migrations`, `m3a/W2-write-txconn`, `m3a/W3-with-immediate-enforcement`, `m3a/W4-dps-auth`, `m3a/W5-stages-1-2`, `m3a/W6-stage3-sign`, `m3a/W7-send-routing`, `m3a/W8-stage5-finalize`, `m3a/W9-boot-recovery`, `m3a/W10-dps-dispatch`, `m3a/med1-lease-rename`, `m3a/W11-deterministic-replay`, `m3a/W11-pr2a`, `m3a/W11-sent-rm-edge`, `m3a/handoff`) — prune as time permits; all merged to `rust-gateway`.

### 6.6 Memory hygiene

Once the bd epic is closed, update `MEMORY.md`:
- Flip `project_m3a_starting_point.md` from "starting point" to "completed milestone" (or replace with M3b starting-point reference once the M3b plan opens).

---

## 7. Without handoff approval

Opening the M3b plan is premature and risks re-litigating decisions M3a already closed.  Specifically:
- Pattern B SENDING marker semantics (closed by W7 + W11/§6.3).
- `App::boot` 6-branch decision tree (closed by W9 + 9 §9.1 fixtures).
- DpsError full routing (closed by W10 + 21 routing fixtures).
- Deterministic-replay invariant (closed by W11 + 9 fixtures).
- Global-single-writer invariant terminology (closed by ADR-M3-A10).

If any of these need re-opening for M3b reasons, that re-opening is itself a discrete decision that belongs in the M3b plan's entry gate, not in M3a tail churn.
