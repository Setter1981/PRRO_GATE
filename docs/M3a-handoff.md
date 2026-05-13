# M3a Handoff — exit gate before M3b implementation plan

**Status:** M3a implementation phase closed.  rust-gateway HEAD = `a7369b9` (Merge PR #36 from Setter1981/m3a/W11-pr2b-runtime).  All 12 plan tasks (`docs/superpowers/plans/2026-05-07-m3a-implementation.md`) marked `completed` in commit `08fc6c4`.  Full crate test surface: **448 passed / 0 failed / 5 ignored** across 27 integration test files; all 9 W11 deterministic-replay fixtures green.

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

Every M3a code change landed via `gh pr merge --merge` (regular merge commit; preserves the per-PR ladder in `git log --merges`).  18 PRs total: 16 W-task PRs + 1 CI infra fix (#20) + 1 docs-only (#28).

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
| #36 | `m3a/W11-pr2b-runtime` | `a7369b9` | W11/PR-2b | SENT 3-way + PREPARED + fixtures #1/#4/#5/#6 (M3a closing PR) |

Plus one chore commit on top of `a7369b9`: `08fc6c4` (`chore(plan/m3a): mark W6/W7/W8/W10/W9/W11 completed`) — surgical plan tasks.json status flip + lastUpdated bump; no content rewrite.

---

## 2. Final test surface

`cargo test -p prro` on `rust-gateway` post-`a7369b9`:

- **448 passed / 0 failed / 5 ignored.**
- 27 integration test files plus the lib unit-test surface and 1 trybuild driver.
- W3 static scanner test (`tests/with_immediate_no_foreign_io.rs`) — 8 / 0 / 0 green; production source has zero foreign IO inside any `with_immediate` body.
- W11 deterministic-replay (`tests/write_path_deterministic_replay.rs`) — **9 / 0 / 0** green:
  - #1 PREPARED — `dispatch_prepared_via_chain` drives sign + send chain to SENT.
  - #2 SIGNED — `stage_send::run` drives Signed → Sending → Sent on happy `send_chk`.
  - #3 SENDING — Pattern B no-resend (`send_chk_count == 0` during recovery).
  - #4 SENT/§6.4-a — `last_chk` Match → KVT1 + KVT1_RAW from `ack.data_sign`.
  - #5 SENT/§6.4-b — `last_chk` Mismatch → RequiresManualReconciliation (operator handoff via PR #35 whitelist edge).
  - #6 SENT/§6.4-c — `last_chk` NotFound → ErrorRetryable (tick 1) → SENT via `stage_send::run` (tick 2); **explicit two-tick driver per ADR-M3-A9 step 3**.
  - #7 KVT1 — passive hold (no DPS query; M3b active-poll deferred).
  - #8 KVT2 — `stage_finalize::run` drives Kvt2 → Ack without DPS query (protocol-final).
  - #9 ERROR_RETRYABLE — happy retry without MAC budget burn.

Five `#[ignore]`d fixtures live in `tests/app_boot_quick_check_failure.rs` — corruption-fixture infra (sqlx::migrate! re-application self-heals).  Deferred to M3b infra cleanup; not gating M3a exit.

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
| **A6** | implemented | `error_routing::route_send_result` table-driven dispatch (W10); 21 routing fixtures + MAC recovery -12 fixture green. |
| **A7** | implemented | `App::boot` 6-branch decision tree in `boot_phase::run_boot_reconciliation` (W9); 9 fixtures in `tests/app_boot_reconciliation.rs`; PRRO_GATE-ah8 verbatim acceptance fixture green. |
| **A8** | implemented | `list_pending_for_fn` whitelist 7→8 with `Sending`; intentional whitelist gaps preserved (`intentional_whitelist_gaps_remain_forbidden`). |
| **A9** | implemented | `DocState::Sending` value + migration 008 + Pattern B crash-resume (CAS Sending → ErrorRetryable, **never** auto re-send).  Fixture #3 proves zero `send_chk` during SENDING recovery; fixture #6 proves the two-tick path through ErrorRetryable (NO direct `Sent → Sending`). |
| **A10** | implemented | ADR-M3-A10 codifies the M3a global-single-writer invariant; docstring rename `lease` → `invariant`; smoke test pins ADR existence (PR #31). |

### 3.2 Pattern A / Pattern B

- **Pattern A** (stage 3 sign): chain-pin in 3-PRE `with_immediate`, crypto outside, persist (CAS Prepared → Signed + PAYLOAD_XML + SIGNED_XML + audit) in 3-PERSIST `with_immediate`.  Timestamp ordering proof: `test_hook::COUNTER` + spy crypto provider; sign call seq < persist first stmt seq, structurally.
- **Pattern B** (stage 4 send): 4-pre `with_immediate` (CAS Signed/Encrypted/ErrorRetryable → Sending + allocate `transport_trace` + `submission_attempted_at`) → wire `send_chk` OUTSIDE any envelope → 4-b `with_immediate` (post-wire CAS Sending → Sent/routed + `set_server_fiscal_no_tx` + `transport_trace::complete_tx` + audit).  W3 scanner enforces structural separation: `send_chk` is never reached from inside `with_immediate`.

### 3.3 Deterministic-replay invariant (W11)

W0-3 §6 mandates: for every pending `DocState`, `App::reconcile_pending(_with)` converges to the same final state whether the prior process crashed mid-pipeline or completed uninterrupted.  PR-1a..PR-2b prove this end-to-end across all 7 pending states + the 3 SENT sub-cases (a / b / c).  Critical structural assertions:

- **§6.3 Pattern B no-resend** — SENDING recovery does NOT invoke `send_chk`.
- **§6.4-c two-tick contract** — SENT/NotFound recovery hops through ErrorRetryable; the retry happens on a separate boot tick with a new `ReconciliationRuntime`; **never** a direct `Sent → Sending` edge.
- **§6.5 KVT1 passive hold** — KVT2-receipt API not exposed by `DpsChannel` in M3a; active polling deferred to M3b.
- **§6.6 KVT2 protocol-final** — `stage_finalize::run` drives Kvt2 → Ack without DPS query.

---

## 4. bd issues closed by implementation proof

All 5 entry-decision bd issues (W0 exit criteria) have their closure-gate satisfied by code on `rust-gateway`:

| bd | Closure proof on `rust-gateway` |
|----|--------------------------------|
| **PRRO_GATE-ddn** | UNIQUE migration `007_lnd_unique.sql` (W1) + `next_lnd` sequencer via `node_state::allocate_next_lnd` (W5).  `tests/migrations_007_008.rs` + `tests/write_path_stage1_acquire.rs::stage1_unique_fn_lnd_collision_fails_closed` green. |
| **PRRO_GATE-zti** | `stage_sign::derive_wire_artifact_kind` maps `ShiftClose` and `ZReport` to `WireArtifactKind::ZReport` at the W6 builder boundary; ZReport-only fixtures in `tests/write_path_stage3_sign.rs`. |
| **PRRO_GATE-k99** | `WriteTxConn<'_>` sealed newtype in `db/tx.rs` (W2); 4 trybuild compile-fail fixtures + `transition_state_atomicity` 2/2 green. |
| **PRRO_GATE-6bj** | `error_routing::route_send_result` (W10) — 21 routing fixtures + MAC recovery -12 fixture green.  `DocState::Sending` + crash-resume rule in W11 fixture #3. |
| **PRRO_GATE-ah8** | `tests/app_boot_reconciliation.rs::ah8_shift_state_opened_preserved_across_boot` green (PRRO_GATE-ah8 verbatim acceptance). |

**Cross-link items:**
- **PRRO_GATE-9qd.1** (M3a epic) — should close once this handoff is approved AND all 5 children above are flipped to closed.  bd verification step lives in §6.2 below.
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

Before closing `PRRO_GATE-9qd.1` (M3a epic):

1. Verify each of the 5 entry-decision children (ddn / zti / k99 / 6bj / ah8) is physically in `closed` state via `bd list --parent PRRO_GATE-9qd.1 --status closed`; any still `open` must either be closed with a "superseded by code at <commit>" comment OR re-opened as M3b carry-forward.
2. Verify the M3 parent epic `PRRO_GATE-9qd` has only this one M3a child plus an M3b placeholder (open at M3b plan time).
3. Comment `PRRO_GATE-9qd.1` with the handoff commit hash + this document path; close.

### 6.3 ONLINE-against-test-DPS smoke (ADR D3 gate #4)

Mandatory if a non-production DPS contour is available.  Memory `project_sprint7_complete` already records a successful full live DPS cycle (SHIFT_OPEN → SELL → Z_REPORT on `cabinet.tax.gov.ua:9443`); that artifact is sufficient evidence for the ADR-D3 gate if the operator agrees to map Python-stack Sprint-7 evidence onto Rust-stack M3a exit.  Otherwise discharge by explicit owner waiver committed before M3b.

### 6.4 ADR open items O1 / O2 / O3

Closure of O1 (1С OLE scope) / O2 (onboarding / cert provisioning automation) / O3 (retention + shift aggregation depth) is REQUIRED before M4 / M5 sizing — these decisions are the bottleneck on plan writing for those milestones.  Pinging them at M3a exit is the right cadence.

### 6.5 Worktree cleanup

After this handoff lands:
- `git worktree remove /mnt/d/PRRO_GATE-m3a-w11-pr2b-runtime` — work tree no longer needed for code work.
- Local branch `m3a/W11-pr2b-runtime` (and other m3a/* feature branches) can be pruned via `git branch -d` once their PRs are confirmed merged on `origin/rust-gateway`.

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
