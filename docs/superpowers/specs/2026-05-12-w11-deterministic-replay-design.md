# W11 Deterministic-Replay Fixture Matrix

Status: **GATE-PASSED 2026-05-12** — Q1-Q4 closed by operator; ready for PR-1a.
Anchors: W0-3 §6 / §9.3 + ADR-M3-A8 / A9 / A10.

**Operator decisions (binding):**
- **Q1 (§6.4-c):** two-tick re-drive, NOT inline. Fixture #6 calls `reconcile_pending_with` twice.
- **Q2 (§6.5):** KVT1 stays passive hold under W11; W11 does NOT supersede W9. Active-poll KVT1→ACK is a separate slice (transport API extension).
- **Q3 (recovery_attempts):** no general column exists, no migration. Reuse `transport_trace::attempts_used` (W9) + `fiscal_documents.mac_recovery_attempts` (W10).
- **Q4 (wiring):** new `App::reconcile_pending_with(deps: ReconciliationRuntime<'_>)` entry point; `App::boot` unchanged; old `reconcile_pending()` co-exists as ctx-free/deferred path for W9 compat.

**Slice split:** PR-1a (wiring + SENDING #3) → PR-1b (KVT2 #8 + KVT1 #7) → PR-2 (remaining 7).

---

## 1. Source anchors

- §6 sub-sections (W0-3 spec):
  - §6.1 PREPARED — `2026-05-06-m3-w0-3-retry-recovery.md:669-685`
  - §6.2 SIGNED — `:686-709`
  - §6.3 SENDING — `:710-761` (critical; Pattern B no-resend contract)
  - §6.4 SENT (sub-cases a/b/c) — `:762-787`
  - §6.5 KVT1 — `:788-798`
  - §6.6 KVT2 — `:799-830` (critical; no DPS query)
  - §6.7 ERROR_RETRYABLE — `:831-854`
- §9.3 fixture matrix spec — `:1263-1282`.
- ADR cross-refs:
  - ADR-M3-A8 — `2026-05-04-m2-pre-plan-adr.md` (pending-set whitelist gaps).
  - ADR-M3-A9 — same (Sending crash-resume + ErrorRetryable retry-path forbids `ErrorRetryable → Sent`).
  - ADR-M3-A10 — `2026-05-12-adr-m3-a10-global-single-writer.md` (invariant terminology: single-writer-per-FN, NOT lease).

Harness reference:
- `rust/prro/tests/app_boot_reconciliation.rs:1-790` — W9 reconciliation harness
  (pool setup `:34-40`; `seed_fn_config` `:42-51`; `seed_node_state` `:53-71`;
  `seed_doc_in_state` `:73-100`; `audit_count` `:110-116`; `doc_state` `:118-124`;
  multi-FN `App::reconcile_pending` driver `:571-605`).
- `rust/prro/tests/common/mod.rs:79-225` — shared `StubDpsChannel` with
  `send_chk_calls` counter `:81`, `with_spy` constructor `:104-115`, `call_count()`
  accessor `:117-119`; `DetCrypto` + `det_signing_ctx()` `:171-216`.
- `rust/prro/tests/write_path_dps_error_routing.rs:69-118` — `seed_doc` shape
  that pre-INSERTs PAYLOAD_XML + SIGNED_XML (mirrors stage-3 PERSIST commit).

---

## 2. Existing harness contract

Inheritable shape:

- **DB:** `fresh_pool()` returns a `tempfile::TempDir` + `SqlitePool` over an
  on-disk SQLite file with full migration set applied via `prro::db::open_pool`
  (`app_boot_reconciliation.rs:34-40`). Use the on-disk variant (NOT in-memory)
  because `App::boot` reads the path from `AppConfig`.
- **Pre-seeding pending docs:** raw SQL via test pool. `seed_doc_in_state` in
  the boot-recon harness `:73-100` is minimal (no document_files); for §6.2 /
  §6.3 / §6.4 we need the richer shape from
  `write_path_dps_error_routing.rs:69-118` which also writes `document_files`
  rows `PAYLOAD_XML` + `SIGNED_XML`. Lift that helper, do not re-write it.
- **App::reconcile_pending invocation:** `App::boot(cfg).await` + `app.reconcile_pending().await`
  returning `ReconciliationSummary` (see `app_boot_reconciliation.rs:570-605`,
  `:646-709`). For W11 we read final `doc_state` via the existing
  `doc_state(pool, doc_id)` helper.
- **Forbidden-call spy:** `StubDpsChannel::call_count()` already exists
  (`common/mod.rs:117-119`). For "must be zero" assertions, query
  `call_count()` AFTER reconcile returns. For "must panic on invocation"
  (KVT2), construct a `StubDpsChannel` with an empty response queue +
  `with_spy(..., || panic!("DPS must not be called during KVT2 recovery"))`.

**Critical gap.** `App::reconcile_pending` today (`src/app.rs:212-243`) takes
NO `DpsChannel` / `SigningContext` arguments — it dispatches per-FN through
`boot_phase::run_boot_reconciliation(&pool, &fn)`. The four ctx-needy states
(PREPARED / SIGNED / SENT / ERROR_RETRYABLE) currently emit
`BOOT_DISPATCH_DEFERRED` and leave the doc untouched
(`boot_phase.rs:845-871`). **W11 must extend the App / boot_phase surface to
accept the missing dependencies, OR drive recovery via a separate
`App::reconcile_pending_with` entry point** — this is a wiring decision the
implementation slice must make BEFORE adding the fixture file. Design
recommendation: extend `App::boot` to optionally accept a `RuntimeDeps {
dps: Arc<dyn DpsChannel>, signing_ctx: SigningContext }` parameter; tests
inject `StubDpsChannel` + `det_signing_ctx()`.

---

## 3. Fixture matrix (9 rows)

Quotes are verbatim from §6 with line citations. Where §6 text is silent on
a precise post-state, the cell is marked **OPEN** and the question is
surfaced in §9.

| # | §6 sub-case + name | Pre-state seed | Mocked DPS / crypto | Expected final state | Forbidden / spy assertion |
|---|---|---|---|---|---|
| 1 | §6.1 `prepared_crash` | DocState=PREPARED; payload_json + payload_sha256_canonical persisted; NO SIGNED_XML in document_files | `DetCrypto` (returns `RECOVERED-CMS`); `StubDpsChannel` returns happy `CheckAck` | ACK | None specific. §6.1:680-684 — "re-build same canonical XML; re-sign; commit state=SIGNED" then drive forward. |
| 2 | §6.2 `signed_crash` | DocState=SIGNED; SIGNED_XML persisted in document_files; NO SENDING marker in transport_trace | `StubDpsChannel` returns happy `CheckAck`; no crypto needed | ACK | Per §6.2:693-708 — "fresh first send from DPS's perspective; no duplicate hazard"; `call_count == 1` (one wire send during recovery). |
| 3 | §6.3 `sending_crash` (CRITICAL) | DocState=SENDING; SIGNED_XML persisted; transport_trace row showing send started | `StubDpsChannel::with_spy(panic!, ...)` — must NOT be invoked | ErrorRetryable | **`call_count() == 0`**; audit `crash_resume_sending_to_error_retryable` (per §6.3:725) or `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE` (existing W9 audit name, `app_boot_reconciliation.rs:186`). |
| 4 | §6.4-a `sent_crash_match` | DocState=SENT; SIGNED_XML persisted; `transport_request_id` recorded | `StubDpsChannel` last_chk returns `CheckAck { id == transport_request_id }` | KVT1 (§6.4:769 "transitions Sent → Kvt1 (whitelist :95)"). **OPEN**: does fixture stop at KVT1 or drive forward to ACK? §6.4:781 says "Final state: ACK in cases (a) and (c)" — drive forward. | one `last_chk` call; zero `send_chk` calls. |
| 5 | §6.4-b `sent_crash_mismatch` | DocState=SENT | `StubDpsChannel` last_chk returns `CheckAck { id != transport_request_id }` | RequiresManualReconciliation (§6.4:771-772). | one `last_chk` call; zero `send_chk` calls. |
| 6 | §6.4-c `sent_crash_notfound` (CRITICAL) | DocState=SENT | tick 1: `StubDpsChannel` last_chk returns `NotFound`; tick 2: `send_chk` returns happy ack | **Operator decision (2026-05-12): two-tick driver, NOT inline.** After tick 1: state=ErrorRetryable, audit `BOOT_RESUME_SENT_TO_ERROR_RETRYABLE` (or equivalent), zero `send_chk`. After tick 2: state=ACK (via Sent/Kvt1/Kvt2). **Two ticks** enforce ADR-M3-A9 whitelist gap visibly — inline path would hide the contract inside one helper and weaken the proof. | **Two-step transition observed across two `reconcile_pending_with` calls**: tick 1 emits Sent→ErrorRetryable + `last_chk_count == 1` + `send_chk_count == 0`; tick 2 emits ErrorRetryable→Sending→Sent/Kvt1 trace + `send_chk_count == 1`. Direct `Sent → Sending` forbidden (no whitelist edge, §6.4:778). |
| 7 | §6.5 `kvt1_crash` (corrected scope) | DocState=KVT1 | `StubDpsChannel::with_spy(panic!, ...)` — empty response queue | **Operator decision (2026-05-12): KVT1 stays passive hold under W11; W11 does NOT supersede W9.** Expected final state = KVT1 (unchanged). Reason: `DpsChannel` has no per-doc KVT2-receipt API; KVT1→ACK via active polling is a separate slice (transport API extension), not W11. | audit `BOOT_KVT1_HOLD_DEFERRED` present; **`call_count() == 0`** on all DPS methods; W9 test `branch_c_dispatches_kvt1_to_passive_hold` co-exists as regression baseline. |
| 8 | §6.6 `kvt2_crash` (CRITICAL) | DocState=KVT2 | `StubDpsChannel::with_spy(\|\| panic!("KVT2 must not query DPS"), ...)` — empty response queue | ACK (§6.6:822 "Final state: ACK regardless of crash point"). | **`call_count() == 0`** (no `send_chk`); `last_chk` panic-mock proves no DPS query at all (§6.6:810-811 "there is no DPS query in this branch, because KVT2 is the protocol-level commit point"). `node_state.last_known_unsigned_xml_sha256` updated (§6.6:814). |
| 9 | §6.7 `error_retryable_crash` (corrected scope) | DocState=ERROR_RETRYABLE; `mac_recovery_attempts=0` (W10 single-bit budget); `transport_trace::attempts_used` baseline recorded | `StubDpsChannel` happy ack | ACK (§6.7:834 "each attempt either succeeds (forward to Sent → Kvt1)"). **Operator decision (2026-05-12): no general `recovery_attempts` column exists.** Use existing budgets: W9 `transport_trace::attempts_used` + W10 `fiscal_documents.mac_recovery_attempts`. | **`mac_recovery_attempts` unchanged at 0** (happy retry must NOT burn MAC-recovery budget — that budget is for MAC-recovery hint hash, not general retry); `transport_trace::attempts_used` increments by exactly the count of wire attempts recorded as trace rows; one happy `send_chk` invocation. No new migration. |

---

## 4. Three critical assertion contracts

### 4.1 SENDING (§6.3) — Pattern B no-resend

§6.3:724-727 mandates:

> "Recovery action (per §3 SENDING row): CAS `Sending→ErrorRetryable` + audit
> `crash_resume_sending_to_error_retryable`; do NOT auto-re-send."

ADR-M3-A9 step 3 anchors this further: even operator-initiated re-send from
ErrorRetryable must go through `ErrorRetryable → Sending → wire`, never direct
to Sent.

**Spy mechanism:** `StubDpsChannel::with_spy(response, Box::new(|| panic!("Pattern B violation")))` —
construct with empty response queue so any invocation panics before the
response is dequeued. Cross-check via `call_count() == 0` after reconcile
returns (defence-in-depth: if the spy is wired but the recovery somehow
bypasses it, the count is the second observable).

W9 fixture `branch_c_dispatches_sending_to_resume_helper`
(`app_boot_reconciliation.rs:165-190`) ALREADY verifies the state transition
(SENDING → ERROR_RETRYABLE + `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE` audit)
but does NOT inject a DpsChannel and does NOT assert "zero invocations".
W11 fixture #3 is the canonical Pattern B proof — it adds the spy.

### 4.2 KVT2 (§6.6) — protocol-final, no DPS query

§6.6:808-813 mandates:

> "App::boot recovery invokes §3 KVT2 rule 're-drive forward to ACK only'.
> Note: there is **no DPS query** in this branch, because KVT2 is the
> protocol-level commit point... Recovery executes the stage-5 finalize
> logic — `transition_state(doc_id, Kvt2, Ack)` CAS UPDATE +
> `node_state.last_known_unsigned_xml_sha256` update + audit_log append +
> inbox.status=DONE."

**Spy mechanism:** `StubDpsChannel` constructed with empty response queue
and `with_spy(..., panic!)`. Any DPS method (send_chk / last_chk / ping /
status_rro / info_rro) invocation panics — the existing stub's
`unreachable!()` defaults on `last_chk` / `ping` / `status_rro` /
`info_rro` (`common/mod.rs:136-150`) already enforce this. Just verify
`call_count() == 0` AFTER reconcile to also catch `send_chk` invocations.

W9 fixture coverage: NONE. KVT2 currently routes through
`stage_finalize::run` per `boot_phase.rs:815-843` but no W9 test asserts
"DPS not consulted". W11 fixture #8 is the canonical proof.

### 4.3 SENT case (c) (§6.4) — two-step transition

§6.4:773-780 mandates:

> "(c) `last_chk` returns NotFound: DPS has no record; recovery re-drives
> via the Pattern B path `Sent → ErrorRetryable → Sending → wire send →
> 4b CAS`... The intermediate ErrorRetryable hop is required because the
> whitelist has no direct `Sent → Sending`; the two-step transition keeps
> Pattern B's SENDING-marker contract on the recovery path identical to
> the live path."

ADR-M3-A9 step 3: direct `ErrorRetryable → Sent` is forbidden, so the
recovery loop must go through Sending.

**Observation primitive:** Read `transport_trace` rows for the doc, ordered
by `created_at` (or whatever durable ordering migration 010 provides).
Assert the row sequence includes both:
1. A row whose `outcome_kind` corresponds to the Sent→ErrorRetryable
   crash-recovery edge (or an audit `BOOT_RESUME_*` event in `audit_log`
   if `transport_trace` does not record this edge).
2. A subsequent row showing ErrorRetryable→Sending re-drive (the live
   stage-4 entry).
3. The final Sending→Sent/Kvt1 row from the re-send wire reply.

**Operator decision (2026-05-12): two-tick, NOT inline.** Fixture #6 calls `reconcile_pending_with` twice:
- Tick 1: `Sent + last_chk=NotFound` → CAS `Sent → ErrorRetryable`, audit, zero `send_chk` invocations. State observable as ErrorRetryable.
- Tick 2: `ErrorRetryable` → re-drive via Pattern B → `Sending` → wire `send_chk` → `Sent/Kvt1`.

Rationale: inline path would compress the two-step contract into one helper invocation and weaken the proof of the §6.4:778 whitelist gap. Two-tick fixture makes the intermediate ErrorRetryable directly observable and forces the implementation to honour the ADR-M3-A9 retry-path policy at the App boundary, not just internally.

---

## 5. Existing test surface / gap analysis

| §6 case | Already covered? | Where | Dedup decision |
|---|---|---|---|
| §6.1 PREPARED | Partially — `branch_c_ctx_needy_states_emit_deferred_audit` `app_boot_reconciliation.rs:227-248` asserts PREPARED stays in PREPARED (W9 deferral). W11 must FLIP this expectation: PREPARED now drives forward to ACK. | — | W9 deferral test stays as a regression guard for the W9 baseline; W11 fixture #1 proves the wired-up path. They co-exist. |
| §6.2 SIGNED | Same — `:227-248` covers W9 deferral. | — | Same as #1. |
| §6.3 SENDING | State transition covered by `branch_c_dispatches_sending_to_resume_helper` `:165-190`. **NOT covered: zero-send assertion.** | — | W11 fixture #3 is additive; existing test stays. |
| §6.4-a/b/c SENT | W9 deferral covered `:227-248`. Routing-decision side (live stage-4) covered by `write_path_dps_error_routing.rs`. **Crash-resume Sent recovery is NOT covered anywhere.** | — | W11 fixtures #4/#5/#6 are net-new. |
| §6.5 KVT1 | `branch_c_dispatches_kvt1_to_passive_hold` `:192-207` asserts passive hold (no advance). §6.5 demands drive-forward via re-query in W11. | — | W9 test must either be updated or W11 fixture #7 supersedes it. **Action: re-read W9 freeze rationale — does W11 implementation flip KVT1 behaviour? If yes, the W9 "passive hold" test must move from "expected" to "regression baseline" status.** |
| §6.6 KVT2 | `stage_finalize::run` covered indirectly via W8 tests; W9 dispatch covered `boot_phase.rs:815-843`. **NOT covered: zero-DPS-query assertion.** | — | W11 fixture #8 is additive. |
| §6.7 ERROR_RETRYABLE | W9 deferral covered `:227-248`. Live ErrorRetryable retries covered in `write_path_dps_error_routing.rs` and `mac_recovery_orchestrator.rs`. **Crash-mid-retry recovery is NOT covered.** | — | W11 fixture #9 is net-new. |

---

## 6. New harness primitives required

Estimated additions (LoC budget ~250-350 lines for the test file + ~50
lines of shared helpers):

1. **Sender-zero-call panic stub** — `StubDpsChannel::with_panic_on_send_chk()`
   convenience constructor. Underlying primitive exists. ~10 LoC, add to
   `tests/common/mod.rs`.
2. **`last_chk` scriptable response** — current stub's `last_chk` is
   `unreachable!()` `common/mod.rs:136-138`. SENT fixtures #4/#5/#6 need a
   scripted queue parallel to `send_chk_v2`. ~30 LoC: add
   `last_chk_responses: Mutex<VecDeque<Result<CheckAck, DpsError>>>` +
   `last_chk_calls: AtomicUsize` + `with_last_chk_queue(...)` constructor +
   replace `unreachable!()` body with queue pop. Mirrors the existing
   `send_chk` queue shape exactly.
3. **`seed_doc_with_signed_artefacts`** — pre-seeds a doc with PAYLOAD_XML
   + SIGNED_XML + (where needed) transport_trace row for Sending/Sent
   states. Adapt from `write_path_dps_error_routing.rs:69-118`. ~50 LoC.
   Decision: place in the new test file directly (per `common/mod.rs:21-26`
   convention that seed helpers stay per-file).
4. **App boot helper with injected deps** — once `App::boot` accepts
   `RuntimeDeps`, add a `boot_app_with_stub(pool_path, dps_stub,
   signing_ctx)` helper. ~20 LoC.

Prefer extension of `StubDpsChannel` over a new mock type — keeps the
single-source-of-truth invariant per `common/mod.rs:14-21`.

---

## 7. Risk surface

- **Counting send_chk during recovery only.** A naive `call_count()`
  assertion includes any pre-crash invocation. Since fixtures pre-seed
  state without driving the live stage-4 path, the live path never runs in
  the test process and pre-crash invocations are zero by construction.
  **Mitigation:** instantiate the stub AFTER seeding, in the same
  statement as `App::boot`, so its counter starts at zero relative to
  reconcile.
- **KVT2 mock distinction.** Must distinguish "DPS not called" from "DPS
  called and returned OK". Solution: empty response queue + `with_spy(...
  panic!)`. Returning OK requires a non-empty queue, so any invocation
  either panics in the spy OR panics popping the empty queue (line
  `common/mod.rs:131-133`). Defence-in-depth.
- **SENT case (c) inline vs deferred re-drive.** See §4.3 open question.
  Risk of asserting on the wrong driver shape. **Mitigation:** before
  writing fixture #6, run a minimal probe test that calls
  `reconcile_pending()` against a SENT-NotFound seed and prints
  intermediate state — let observed behaviour disambiguate.
- **`recovery_attempts` schema.** §6.7 assumes a persisted column on
  `fiscal_documents`. Confirm column exists (migration 013 / W7 land
  schema) before relying on it. If absent, surface as blocker — fixture
  #9 cannot encode the §6.7 invariant.
- **Audit event names.** §6.3 spec uses
  `crash_resume_sending_to_error_retryable`; W9 implementation emits
  `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE` `app_boot_reconciliation.rs:186`.
  Fixture must assert against the actual emitted name; spec is descriptive
  not normative on the audit-string casing.
- **Spec-acceptance bar.** A fixture "passes" only if it asserts the §6
  observable, not just final-state byte-match. E.g. fixture #6 must assert
  the two-step transition trace, not just `final state == ACK`. Without
  that, the test is happy-coincidence and would pass even under a
  whitelist-violating direct `Sent → Sending` edge.

---

## 8. Slice scope estimate

- New test file `rust/prro/tests/write_path_deterministic_replay.rs`:
  ~600-800 LoC (9 fixtures × ~60-80 LoC each + shared helpers).
- Extensions to `rust/prro/tests/common/mod.rs`: ~50 LoC (last_chk queue
  + panic-on-send constructor).
- **Production-code wiring required BEFORE fixtures land.** The four
  ctx-needy dispatch arms in `boot_phase.rs:845-871` (PREPARED / SIGNED /
  SENT / ERROR_RETRYABLE) must be implemented + `App::reconcile_pending`
  must accept the new runtime dependencies. This is the W11 implementation
  work; the fixture file is its acceptance proof.
- **Slice split.** Recommend two PRs:
  - PR-1: dispatch wiring (boot_phase + App), with §6.3 / §6.6 fixtures
    only (the two critical zero-call assertions land first).
  - PR-2: remaining 7 fixtures (PREPARED, SIGNED, SENT a/b/c, KVT1,
    ERROR_RETRYABLE).
  Rationale: PR-1 establishes the wiring shape under tight observable
  pressure (critical assertions are the gate); PR-2 fills the matrix
  without bikeshedding the harness shape again.

---

## 9. Open questions — RESOLVED (2026-05-12 operator decisions)

1. **§6.4-c inline vs two-tick → TWO-TICK** (see §3 row 6 + §4.3 above). Fixture #6 calls `reconcile_pending_with` twice; intermediate ErrorRetryable is directly observable; ADR-M3-A9 whitelist gap proven at App boundary.
2. **KVT1 W9 vs W11 expectation → COEXIST.** W11 does NOT supersede W9 passive-hold. KVT1→ACK via active polling is a separate slice (transport API extension for per-doc KVT2 receipt), not W11. Fixture #7 corrected to assert passive hold + `BOOT_KVT1_HOLD_DEFERRED` + zero DPS calls. W9 test `branch_c_dispatches_kvt1_to_passive_hold` stays as regression baseline.
3. **`recovery_attempts` column → DOES NOT EXIST and is not added.** No general `recovery_attempts` field on `fiscal_documents`. Existing budgets reused:
   - **W9 boot budget:** `transport_trace::attempts_used` (per-doc, per-wire-attempt monotonic counter; see W9 freeze `transport_trace::complete_via_recovery_tx`).
   - **W10 MAC-recovery single-bit budget:** `fiscal_documents.mac_recovery_attempts CHECK IN (0,1)` (migration 013, ADR-M3-A10 §2).
   Fixture #9 asserts: happy ERROR_RETRYABLE retry does NOT touch `mac_recovery_attempts` (which is MAC-hint-specific, not general retry budget); `transport_trace::attempts_used` increments by the count of wire attempts recorded as trace rows. NO new migration.
4. **Runtime-dep injection shape → `App::reconcile_pending_with(...)`, NOT `App::boot` extension.** `App::boot` keeps current shape (config + db + singleton + integrity check + W9 `reconcile_pending` chain). Add separate entry point:
   ```rust
   pub struct ReconciliationRuntime<'a> {
       pub dps: &'a dyn DpsChannel,
       pub signing_ctx: &'a SigningContext,  // mandatory — PREPARED/SIGNED/MAC paths need it
   }

   impl App {
       pub async fn reconcile_pending_with(
           &self,
           deps: ReconciliationRuntime<'_>,
       ) -> Result<ReconciliationSummary, BootError> { ... }
   }
   ```
   Old `reconcile_pending()` remains as ctx-free/deferred path for W9 compatibility (emits `BOOT_DISPATCH_DEFERRED` for the 4 ctx-needy states, identical to today). PR-1a wires `reconcile_pending_with`; W9 tests remain green against `reconcile_pending`.

---

## 10. Implementation order (operator slice split, 2026-05-12)

**PR-1a — Wiring skeleton + SENDING fixture #3 (the safety gate).**

1. Production code:
   - Add `ReconciliationRuntime<'_>` struct in `services/reconciliation/mod.rs` (or `services/reconciliation/runtime.rs`).
   - Add `App::reconcile_pending_with(&self, deps: ReconciliationRuntime<'_>)` in `src/app.rs`.
   - Implement ctx-needy dispatch arm for `DocState::Sending` only (PR-1a scope) — replace `BOOT_DISPATCH_DEFERRED` with the real call to `resume_sending_to_error_retryable` (already exists at `boot_phase.rs:205`, needs no DPS/signing arg). Pattern B no-resend: state CAS `Sending→ErrorRetryable` happens inside `with_immediate`; no wire call.
   - Other 3 ctx-needy arms (PREPARED, SIGNED, SENT, ERROR_RETRYABLE) continue to emit `BOOT_DISPATCH_DEFERRED` under the new `reconcile_pending_with` path until PR-1b / PR-2.
2. Test code:
   - Extend `tests/common/mod.rs` with `StubDpsChannel::with_panic_on_all_methods()` convenience (~10 LoC) for the SENDING fixture's "panic if invoked" stub.
   - New `tests/write_path_deterministic_replay.rs` with single fixture #3.
3. Verify: `cargo test -p prro --test write_path_deterministic_replay`.

**PR-1b — KVT2 fixture #8 + KVT1 corrected fixture #7.**

1. Production code:
   - Implement ctx-needy dispatch arm for `DocState::Kvt2` — call `stage_finalize::run` (existing).
   - KVT1 dispatch already correct as passive hold (no wiring change); fixture asserts the existing behaviour.
2. Test code:
   - Append fixtures #7 and #8 to `tests/write_path_deterministic_replay.rs`.
3. Verify: `cargo test -p prro --test write_path_deterministic_replay`.

**PR-2 — Remaining 7 fixtures (PREPARED, SIGNED, SENT a/b/c, ERROR_RETRYABLE).**

1. Production code:
   - Implement remaining ctx-needy dispatch arms: PREPARED, SIGNED, SENT, ERROR_RETRYABLE.
   - Each arm wires either the live stage entry point or the per-state recovery action per W0-3 §3.
2. Test code:
   - Extend `tests/common/mod.rs` with `last_chk` scriptable queue (~30 LoC).
   - Append fixtures #1, #2, #4, #5, #6 (two-tick driver), #9 to `tests/write_path_deterministic_replay.rs`.
3. Verify: full suite green.

**Probe step removed** — Q1 closed by operator (two-tick decision).
