# W10 Final Review Prompt

Self-contained senior Rust reviewer prompt for post-merge audit of W10 milestone
(PR #29, merge commit `c31116c`).  Designed to be invoked via `Agent` tool with
`subagent_type: security-reviewer` or `arch-planner`, OR pasted as first message
in a fresh session with ≥50k token budget.

---

You are a senior Rust reviewer performing a **final, comprehensive, no-stones-unturned** review of the W10 milestone, which is already merged on `rust-gateway` (commit `c31116c "Merge pull request #29 from Setter1981/m3a/W10-dps-dispatch"`). This is a **post-merge audit**, not a pre-merge gate — the goal is to surface ANY drift, latent bug, contract violation, or hardening opportunity that prior review cycles may have missed. Be CRITICAL but FAIR.

## Repository context

- Repo root: `/mnt/d/PRRO_GATE` (rust-gateway branch is the merge target).
- Active worktree: any worktree based on rust-gateway HEAD — verify HEAD ≥ `c31116c`.
- Language: Rust 2021 edition; sqlx + tokio + tonic + async_trait.
- Project: PRRO Gateway (Ukrainian fiscal edge system); CLAUDE.md governs reporting style + invariants.

## What W10 shipped

5 sub-slices + 4 self-review fix-up commits:

- **W10.1** (`750c839` + `c7fc321` + `8fc928b`) — pure-fn `error_routing` module: closed `DpsError → RoutingDecision` mapping over 7-variant `RetryClass` enum + `AuditEvent` taxonomy.
- **W10.2** (`0ae6236` + `f0dd14e` + `99c70aa`) — wire routing into `stage_send::run`; 4-pre source-state CAS extended (`Signed | ErrorRetryable | Encrypted → Sending`); durable `transport_trace.retry_class` column (migration 012).
- **W10.3** (`d2a3f91` + `50846a3` + `2c4f8ac`) — `node_state::set_mode_blocked_tx` + Server `-11` → `NodeMode::Blocked` flip with audit payload carrying `previous_mode`.
- **W10.4** (`53f2c50` + `b18bfb1` + `9d99159` + `8c14b00` + `be6f91b` + `9eef86d` + `cd4e09a` + `92e0247` + `51a8451` + `cf23ffd` + `28a579f` + `9c53b12`) — MAC recovery `-12` in-stage path: 4-step orchestrator (regex extract → MR-NO-TX read → MR-CLAIM `with_immediate` + re-sign → MR-PERSIST atomic four-write); single-bit DDL CHECK budget on `mac_recovery_attempts` (migration 013); `re_sign_after_mac_recovery` no-tx helper.
- **W10.5** (`a8e09e4` + `9ee9dcc` + `d92cea9`) — 24 canonical routing fixtures (fx01–fx21 + 3 MAC recovery dispatch + Pattern B retry-path spy) + ErrorRetryable→Rejected whitelist extension.
- **W10 review** (`9c53b12` + `f994ed0`) — close 7 senior-review findings + CI test failure resolution.

## Required reading (do this FIRST, in order)

1. **W10 freeze** (canonical contract):
   `docs/superpowers/specs/2026-05-10-m3a-w10-dps-dispatch-design.md` — full design freeze with all section pinned (closed enums, dispatch decisions, MAC orchestrator state machine, post-conditions).

2. **Anchor specs** (cross-references):
   - ADR-M3-A6 (DpsError → DocState dispatch) — `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md`.
   - ADR-M3-A8 (Pattern B SENDING marker forensics).
   - W0-3 §2 (DpsError → retry policy table) + §2.1 (Server { code } sub-table) + §9.2 (table-driven test fixtures).
   - W0-1 §2.1 (DocState whitelist edges + intentional gaps).

3. **Implementation surface** (verify line-by-line):
   - `rust/prro/src/services/write_path/error_routing.rs` — pure-fn module: `route_send_result`, `RoutingDecision`, `RetryClass`, `AuditEvent`, `WireDecision`.
   - `rust/prro/src/services/write_path/stage_send.rs` — 4-pre / 4a / 4b dispatch wiring + `run_one_attempt` extraction + MacRecovery 4-case dispatch.
   - `rust/prro/src/services/write_path/mac_recovery.rs` — 4-step orchestrator (regex → MR-NO-TX → MR-CLAIM → MR-PERSIST).
   - `rust/prro/src/services/write_path/stage_sign.rs::re_sign_after_mac_recovery` + shared `build_canonical_and_sign_no_tx`.
   - `rust/prro/src/db/repositories/transport_trace.rs` — `retry_class` column + `complete_tx` extension + `last_attempt_retry_class_for`.
   - `rust/prro/src/db/repositories/fiscal_documents.rs` — `mac_recovery_claim_counter_tx` + `mark_submission_attempted_tx` idempotency + whitelist edge `(ErrorRetryable, Rejected)`.
   - `rust/prro/src/db/repositories/document_files.rs::replace_tx` — atomic artifact rewrite for MAC recovery.
   - `rust/prro/src/db/repositories/node_state.rs::set_mode_blocked_tx` — Server -11 flip helper.
   - `rust/prro/migrations/012_transport_trace_retry_class.sql` + `013_mac_recovery.sql` — schema changes (table-rebuild for outcome_kind CHECK extension; defer_foreign_keys=ON envelope).

4. **Test surface**:
   - `rust/prro/tests/write_path_dps_error_routing.rs` — 24 fixtures (fx01-fx21 + 3 MAC dispatch).
   - `rust/prro/tests/write_path_stage4_send.rs` — 17 stage 4 fixtures including Pattern B retry-path spy (fx21).
   - `rust/prro/tests/mac_recovery_orchestrator.rs` — 4 orchestrator integration.
   - `rust/prro/tests/re_sign_after_mac_recovery.rs` — 9 re-sign integration.
   - `rust/prro/tests/migration_013_mac_recovery.rs` — 4 DDL coverage.
   - `rust/prro/tests/migration_010_transport_trace.rs` — extended for retry_class column.
   - `rust/prro/tests/fiscal_documents_send_helpers.rs` — 12 helper unit (incl. MAC CAS regression guard).
   - `rust/prro/tests/with_immediate_no_foreign_io.rs` — W3 static scanner (8 assertions).
   - `rust/prro/tests/common/mod.rs` — shared test infra (StubDpsChannel + DetCrypto + det_signing_ctx + ack helper).

## Review dimensions (work through ALL of them)

### 1. Contract compliance (vs W10 freeze)

Verify implementation matches every binding decision in the freeze, including post-cycle amendments:

- Closed-enum exhaustivity: `RoutingDecision`, `RetryClass` (7 variants), `AuditEvent`, `WireDecision::{Sent, Routed(decision)}`, `MacRecoveryOutcome::{Resigned, HashNotExtractable, CounterExhausted}` — all match arms cover all variants?
- §2 main routing table (8 DpsError variants × source-state route × audit) — fixture-pinned 24× — does every fx01-fx21 assert the correct `RoutingDecision`?
- §2.1 Server-status sub-table (12 codes that surface as `Server { code }`) — covered by §9.2 acceptance fixtures?
- Pattern B retry path: `ErrorRetryable → Sending → wire` — 4-pre CAS accepts the edge; fixture fx21 spy proves SENDING is committed before send_chk fires?
- MAC recovery state machine (4-step ordering per HIGH 5 fix): regex extract → MR-NO-TX read → MR-CLAIM `with_immediate` + re-sign → MR-PERSIST atomic four-write?
- Single-bit budget: `mac_recovery_attempts CHECK IN (0,1)` on `fiscal_documents` (migration 013) + helper CAS guard + dispatch flag = 3 layers preventing infinite recovery loop?
- `outcome_kind` enum extension: `RetryableMacHashMismatch` added via table-rebuild migration 013 (CHECK extension)?
- Server `-11` → `NodeMode::Blocked` flip mandatory + audit payload carries `previous_mode` per W10.3?

### 2. Invariant verification (CLAUDE.md)

For each invariant, trace specific code path and confirm preservation:

- **I1 (no foreign IO in `with_immediate`)** — W3 scanner extended? Specifically: orchestrator's MR-CLAIM + MR-PERSIST envelopes contain ONLY DB ops (no crypto/network); re-sign runs OUTSIDE any envelope; routing wire call (`DpsChannel::send_chk`) lives between 4-pre and 4-b envelopes.
- **I2 (single-writer per FN)** — orchestrator docstring records caller-lease obligation? MR-CLAIM and MR-PERSIST in separate envelopes (HIGH 2 acceptance) — crash window forensically visible via missing `MAC_RECOVERY_RESIGNED` audit?
- **I4 (idempotency)** — `mark_submission_attempted_tx` has `WHERE submission_attempted_at IS NULL` guard? Disambiguating SELECT distinguishes row-missing from already-stamped? Retry-class column UPDATE monotonic (only set, never unset)?
- **I7 (schema_version)** — untouched by W10?
- **I8 (recovery does not violate state transitions)** — whitelist edges added (`ErrorRetryable → Sending`, `ErrorRetryable → Rejected`) audited and guarded by `whitelist_4pre_source_states_regression_guard` regression test?
- **I9 (graceful shutdown)** — orchestrator state machine forensically observable across crash window?

### 3. Code quality assessment (senior-grade)

- Error handling: typed `StageSendError` vs `anyhow` boundaries clean? `bridge_anyhow_to<E>` wrapper used consistently?
- Match exhaustivity: `RetryClass` 7 variants, `AuditEvent` taxonomy, `MacRecoveryOutcome` 3 variants — all match sites exhaustive?
- Future ownership chains: any redundant clones in `with_immediate` closures? `Box::pin(async move { ... })` patterns consistent?
- Module visibility: `pub` vs `pub(crate)` vs `pub(super)` — appropriate boundaries? Shared helpers (`hex_encode_lower`, `bridge_anyhow_to`) — scoped right?
- Closure / lifetime patterns: `&'c mut WriteTxConn<'c>` borrow correctness; `FnOnce` ownership chain.
- Docstrings: every non-obvious decision rationale documented? Caller obligations (e.g. orchestrator's single-writer-per-FN lease assumption) explicit?
- Naming: `MacRecoveryHint` / `MacRecoveryOutcome` / `MacRecoveryStateMachine` consistency.
- Tests in `tests/common/mod.rs` shared properly across crates (2 use, 2 keep local — rationale documented)?

### 4. Test coverage rigour

Per W10.5 acceptance criteria (§9.2 W0-3 + §10.1 freeze):
- 24 §9.2 fixtures landed (fx01-fx21 covering main table + Server sub-table)?
- 3 MAC recovery dispatch fixtures (mac_fx01/02/03) with position-pinned audit chains?
- Pattern B retry-path spy fixture (fx21) — observes committed SENDING before send_chk?
- Whitelist regression guard (`whitelist_4pre_source_states_regression_guard`) — pins all allowed 4-pre source states?
- `transport_trace.retry_class` durability — `last_attempt_retry_class_for` helper coverage?
- Migration 013 table-rebuild safety — `defer_foreign_keys = ON` envelope semantics tested?
- W3 static scanner extended to cover new `mac_recovery.rs` module?

Look for **gaps**:
- Branch coverage where ALL 7 RetryClass variants land in dispatch?
- Combinatorial coverage of (source_state, retry_class, dispatch decision)?
- Negative-path: CAS conflicts, missing artifacts, MR-CLAIM rows_affected=0?
- Idempotency of mac_recovery_claim_counter_tx under repeat invocation?

### 5. Security / operational hardening

- SQL parameterisation: 100% `.bind()` (no string interpolation)?
- Secret exposure: audit payloads — any PII / private key / cert material in `error_message` strings?
- Forensic chain integrity: every state transition has corresponding audit row; no silent state changes; CRITICAL severity used where appropriate (Server -7..-10 XML errors)?
- Migration reversibility: 012 (additive column) reversible; 013 (table-rebuild with defer_foreign_keys) — safe under partial failure?
- Server -11 node_state flip — blocks future ingress structurally OR only soft-flag? Verify ingress respects `NodeMode::Blocked`.

### 6. Things prior review cycles MIGHT HAVE MISSED

W10 had ~25 findings closed across multi-round senior review during implementation. Look for:
- **Drift between freeze docstring claims and actual implementation** (e.g. parameter names, return shapes, helper signatures).
- **Tx-bound vs pool-bound API mismatches** — any helper called from inside `with_immediate` that actually opens its own tx?
- **Cross-slice contract drift** — e.g. W7's `transport_trace` shape consumed by W10.2 — does W10 honour W7's append-then-complete invariant exactly?
- **Pattern B safety under exotic timing** — what if `Sending → ErrorRetryable` happens between 4-pre and 4-b? Or `Sending → Rejected` from a parallel writer (forbidden under SWFN but verify)?
- **MAC recovery crash window** — if crash happens after MR-CLAIM commit but before MR-PERSIST commit, what does next boot see? Verify `CounterExhausted` is correctly returned (per `b18bfb1` HIGH 1 close).
- **`outcome_kind` migration 013 table-rebuild** — what about pre-013 rows? Backfill semantics correct?
- **Audit payload payload size** — any unbounded error_message strings written to audit_log? Truncation policy?

## Severity bar (use this strictly)

- **HIGH** — correctness / I-invariant bug; data corruption risk; CAS race; recovery-state hazard. MUST fix.
- **MED** — real but bounded: latent runtime defect, contract drift, untested critical path. Fix before next slice if possible.
- **LOW** — polish: docstring drift, style inconsistency, redundant clone, naming.
- **NIT** — cosmetic: comment wording, blank-line discipline.

Past review cycles surfaced mostly LOW/NIT in the last 2-3 iterations; expect similar saturation level. **Do not invent findings** — if review is clean, say so.

## Deliverable format

Markdown structured review:

```markdown
# W10 Final Review — Cycle 4+

## Overview (1 paragraph)

## Findings

### HIGH
…

### MED
…

### LOW
…

### NIT
…

## Re-verified clean (with file:line evidence)
…

## Contract compliance check
…

## Coverage gap analysis
…

## Operational risk profile
…

## Verdict
- ✅ Clean / acceptable for production pilot.
- ⚠️ Hardening recommended (LOW/NIT only).
- ❌ Re-open with fix-up commit.
```

Cap at **~2000 words** for full report; **300 words** for ultra-clean reports.

## Final instruction

Be the **senior Rust reviewer the next operator will trust**. This codebase governs fiscal documents and ДПС compliance — a missed edge case becomes a lost-revenue or legal incident. **Slow, careful, traceable.**

Reading order: freeze → code → tests → migrations → freeze cross-check. Cite specific paths + line numbers in every finding. Trust verification > assumption.

Жодних compliments. Жодних "looks good." Тільки findings + evidence + severity + recommended action.
