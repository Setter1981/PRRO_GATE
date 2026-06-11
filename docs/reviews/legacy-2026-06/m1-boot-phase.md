# M1 review dossier — boot_phase.rs + last_chk_probe.rs

**Module:** `services/reconciliation/boot_phase.rs` (3548 L) + `last_chk_probe.rs` (254 L)
**Lenses:** L2 (recovery-atomicity) · L3 (unverified assumptions / state enumerations) · L4 (terminal vs retryable) · L7 (gates/single-writer)
**Reviewer:** Opus 4.8 (hunter). **Base:** origin/main @92f8fea, suite 1383 green.
**Anchors cross-checked:** `db/models/enums.rs` (DocState/RetryClass/NodeMode), `db/repositories/fiscal_documents.rs:492` (list_pending_for_fn), `transport_trace.rs:399` (attempts_used), `services/write_path/{stage_send,error_routing,dispatch}.rs`, `er_redrive_policy.rs`, `online_convergence.rs`, `backlog_drain.rs`, `runtime/fn_gate.rs`, `guard.rs`, `app.rs`, `tests/kill_point_matrix.rs`, `tests/write_path_deterministic_replay.rs`.
**Format:** `ID | sev | lens | file:line | claim | repro | fix-class`. CONFIRMED carries a repro; HYPOTHESIS is a separate section.

---

## CONFIRMED

### M1-01 | HIGH | L3 | boot_phase.rs:2381, 3163 (+ prepared arm ~2979) vs answered_wire_contact:329-349
**Claim.** `answered_wire_contact()` (the gate deciding whether the boot stale-tip guard runs) counts the three `*_dispatched` buckets as "answered wire exchange", but those buckets are incremented on the `Ok(_)` catch-all of `stage_send::run`, which absorbs `StageSendOutcome::StateConflict` and `DocumentMissing` — both **zero-wire** outcomes (`stage_send.rs:503,507`: "Stage 4 did NOT call `send_chk`"). So an FN whose boot dispatched a SIGNED/PREPARED/ER doc that hit StateConflict (cohort doc no longer in the 4-pre allowlist) made **no wire contact**, yet `answered_wire_contact()==true` → the tip-guard is **skipped** → a stale restored tip goes undetected (the exact failure spec §0 / PR#141 exists to prevent). This is the named suspect (PR#141 bucket completeness) and the proven bug class (outcome outside a gate's intended classification).
**Repro (grep-fact).** `stage_send.rs:516-538` `StageSendOutcome::{Sent,Routed,StateConflict,DocumentMissing,SignerRefused}`; dispatch arms `boot_phase.rs:3157-3163 / 2375-2381` match only `SignerRefused(_)` then `Ok(_) => *_dispatched += 1`; `answered_wire_contact` (`:331-333`) returns true if `signed_dispatched|error_retryable_dispatched|prepared_dispatched > 0`. Pinning-test: seed FN with non-empty ACK tail + 1 SIGNED doc forced to StateConflict; `reconcile_pending_with` (stub `send_chk=unreachable!()`, `last_chk`= a *different* tip) → assert the guard FIRED (BLOCKED + `TIP_GUARD_STALE_LEDGER`). Today it is skipped (`signed_dispatched=1`). The unit test (`:3352+`) only asserts the abstract `signed_dispatched:1 ⇒ true`, never the StateConflict→bucket mapping.
**Fix-class.** Semantic (Fable). Match `StageSendOutcome::{Sent,Routed}` explicitly into `*_dispatched`; route `StateConflict|DocumentMissing` to a non-answered bucket (e.g. a `*_noop` / `dispatch_errors`-style) so the guard still runs.

### M1-02 | HIGH | L4 | boot_phase.rs:2088-2100 (Mismatch arm) + cas_sent_to_manual_reconciliation_from_probe:673 + online_convergence.rs:158
**Claim.** A doc in `SENT` was **provably DPS-acked** (`SENT` is reached only via `WireDecision::Sent` after an OK `CheckAck`; `server_fiscal_no` = that wire id). `last_chk(fn_sign)` is **FN-scoped** (returns the FN's single latest receipt — `channel.rs:24-27`), not doc-scoped. The boot loop (`for doc in &pending`, `:1619`) probes **every** SENT doc against its **own** `server_fiscal_no`. With ≥2 acked SENT docs A(older)+B(newer) both crashed before KVT1, probing A returns B's id → `Mismatch` → A is terminalized `Sent → RequiresManualReconciliation` — even though A is a healthy receipt DPS holds. The probe cannot distinguish "ours never received" (genuine manual) from "ours received but no longer the tip" (benign) — but a SENT doc by construction was received, so Manual is the wrong default for the multi-SENT case.
**Amplified:** `online_convergence::converge_one_doc` (`online_convergence.rs:156-158`) reuses the SAME arm for *resting* online SENT docs — on a busy 24/7 lane, multiple resting SENT docs per FN is the common case → steady-state false-terminalization, not just a boot race.
**Repro (pinning-test).** Seed FN + 2 SENT docs A(`lnd=1, server_fiscal_no='A1'`) and B(`lnd=2, server_fiscal_no='B2'`); stub `last_chk` → `ack.id='B2'`. Run `run_boot_reconciliation` (deps Some). Assert A.state == `REQUIRES_MANUAL_RECONCILIATION` (the bug; A is healthy) while B → `KVT1`. No existing test seeds two SENT docs (fixture_5 uses one).
**Fix-class.** Semantic (Fable). Don't terminalize a SENT doc whose id ≠ tip when its `lnd` < the tip doc's lnd (it was superseded, not lost); or make confirmation doc-scoped. Single-SENT Mismatch → Manual stays defensible.

### M1-03 | MED | L4/L3 | boot_phase.rs:2137-2154 (DecodeEscalate) + 2156-2174 (Unexpected) vs last_chk_probe.rs:26-27,64-65
**Claim.** `ProbeOutcome::DecodeEscalate` is contracted (`last_chk_probe.rs:64-65`, module hdr `:26-27`) as "protocol drift, **no bounded retry**, caller escalates to `RequiresManualReconciliation`". The actual caller routes **both** DecodeEscalate and Unexpected to `complete_probe_trace_no_state_change` → "**No state transition — doc stays in SENT; next boot re-attempts**" (`:859-861`). A persistent Decode (DPS contract drift) therefore **defers unbounded** (re-probed every boot AND every online-convergence tick), never reaching an operator — the inverse of the contract and of L4 (wrongly *retryable* instead of *terminal*). No `MAX_BOOT_ATTEMPTS`-equivalent caps the SENT-probe path.
**Repro (grep-fact).** Contract: `last_chk_probe.rs:64-65`. Behaviour: `boot_phase.rs:2137-2154` → `complete_probe_trace_no_state_change` (`:863`, "no CAS"). Doc-drift bonus: header `:24-27` lists 2 fall-throughs (Transport, Decode) but there are 3 — `Unexpected` (`:67-73`) is undocumented there.
**Fix-class.** Semantic (Fable): decide which side is right — route DecodeEscalate → Manual (bounded), or amend the contract doc to "defer". `Unexpected`-defer is defensible (transient auth/cert blip); `DecodeEscalate`-defer is the contract violation.

### M1-04 | MED | L2 | boot_phase.rs:2050-2065 (allocate_and_insert_tx) + transport_trace.rs:397-405 (attempts_used) + er_redrive_policy.rs:92-101
**Claim.** `dispatch_sent_via_probe` allocates a `transport_trace` `attempt_no` row for a **read-only** `last_chk` probe, in the SAME namespace as real `send_chk` attempts. `attempts_used = COALESCE(MAX(attempt_no),0)` (`:399`) has **no completed/outcome filter** — it counts probe rows (incl. crashed in-flight ones). The ER-redrive budget gate escalates `attempts_used >= MAX_BOOT_ATTEMPTS(5)` → `BudgetExhausted → Manual` (`er_redrive_policy.rs:94`). So a doc that cycled `SENT→(probe NotFound)→ER` with prior crashed probes can be escalated to Manual via **fewer than 5 real send attempts** — read-only probes burn the SEND budget.
**Repro (SQL).** Seed 1 doc + 5 `transport_trace` rows representing probe attempts (`attempt_no 1..5`, e.g. `outcome_kind=NULL` orphans). `SELECT COALESCE(MAX(attempt_no),0)` → 5. Drive the ER-redrive path (TransientRetry) → `BudgetExhausted` with `send_chk` count 0.
**Fix-class.** Mechanical-MED (Opus): count only send attempts toward the budget (filter `attempts_used` by `request_envelope_sha256 != zero` or an explicit kind), or give probes a separate attempt namespace.

### M1-05 | MED | L7 | app.rs:726 (drain) vs app.rs:402,819 (convergence) + fn_gate.rs:49-55 (forward-contract #4)
**Claim.** boot/drain serialize on `reconcile_mutex` (`app.rs:535,726`); online-convergence serializes on the **distinct** `fn_write_gate` (`app.rs:402,819`). Both live tick loops reuse the SAME arms (`dispatch_sent_via_probe`, `confirm_drain_doc`) on the SAME states (SENT/KVT1) under **non-overlapping locks**. fn_gate forward-contract #4 ("run the offline drain UNDER the same per-FN gate so a drain pass and an inline fiscalize are mutually exclusive", `fn_gate.rs:49-55`) is **not implemented** — `drain_offline_backlog_with` takes only `reconcile_mutex`. Current safety rests on the `GoingOnline`(drain)/`Online`(convergence) mode-partition + per-row CAS, NOT a shared lock.
**Repro (grep-fact).** `app.rs:726` `reconcile_mutex.lock()` in drain; `app.rs:402` `acquire_fn_gate → fn_write_gate`; `fn_gate.rs:22-28` "DISTINCT from reconcile_mutex … MUST NOT be unified"; `fn_gate.rs:49-55` contract #4 unfulfilled.
**Fix-class.** A2-integration (Fable): unify (drain under fn_gate) or pin the mode-partition as the load-bearing invariant with a concurrency test. Today MED (not HIGH) — mode-partition + CAS prevent corruption; see M1-H2.

### M1-06 | LOW | L7 | boot_phase.rs:506,576,673,767,964,1278,2017,2200
**Claim.** The W2 `ReconcileGuard` token is required by only the two top-level entries (`run_boot_reconciliation:1375`, `run_boot_tip_guard:1749`); **every** per-DocState mutating helper is `pub`/`pub(crate)` and takes only `pool`+doc — no token. A direct in-crate caller can mutate `fiscal_documents`/`node_state` bypassing the App recon mutex. Mitigated: each helper is one `with_immediate` with a per-row CAS guard (`WHERE state=from`), so a lost race returns `Ok(false)`, not corruption (ADR-M3-A10 §2.1.3).
**Repro (grep-fact).** `pub async fn resume_sending_to_error_retryable(pool, doc_id)` (`:506`) — no token; vs `run_boot_reconciliation(_guard: &ReconcileGuard, …)` (`:1375`). `guard.rs:17-18` frames W2 as closing the bypass on the top entry only.
**Fix-class.** LOW/doc — thread the token through helpers, or document as the ADR-M3-A10 §4 multi-worker carry-forward.

### M1-07 | LOW | L4 | boot_phase.rs:2477-2503 (HoldIndeterminate) + er_redrive_policy.rs:112 + error_routing.rs:126-128
**Claim.** An ER doc with `None`/indeterminate `retry_class` is held in `ERROR_RETRYABLE` indefinitely (audit-only, no CAS). The contract says such a doc is "forwarded to W9 reconciliation / manual triage" (`error_routing.rs:126-128`, dispatcher doc `:2480-2482`) — but nothing surfaces it on a Manual dashboard. Safe direction (no false terminalization, no duplicate send — the H1/fixture_9h goal) but **stuck-forever** with only repeated audit rows as signal.
**Repro.** `fixture_9h_er_latest_unfinished_trace_holds_no_send` asserts `doc_state=='ERROR_RETRYABLE'` post-reconcile (`write_path_deterministic_replay.rs`).
**Fix-class.** Doc/semantic-LOW (Fable): escalate to Manual after K holds, or accept + amend the "manual triage" wording.

### M1-08 | LOW | L2 | boot_phase.rs:2050→2072 (probe alloc→complete window) + close_orphan_transport_traces:1278 (TTL 60s, runs once @app.rs:548)
**Claim.** Kill -9 between the probe's trace **allocate** (E10, `:2050`) and its completing CAS (E2/E3/E4/E5) leaves an orphan trace (`outcome_kind=NULL`) + the doc still `SENT`. The orphan scanner runs **once** per boot before the per-FN loop with a **60s TTL** (`WHERE outcome_kind IS NULL AND started_at < now-60s`); a sub-TTL orphan is skipped this boot, and the per-FN re-probe allocates a **second** orphan (no resume of the existing `attempt_no`). Self-heals (doc converges; orphans close a later boot) but the transient double-orphan feeds the M1-04 budget skew.
**Repro (kill-point sketch / K7 candidate).** Seed SENT doc → run dispatch_sent_via_probe to just after E10 alloc → drop the future → reboot → assert: 1 orphan now; if <60s, scanner skips → re-probe → 2 orphans; doc still SENT; **0 `send_chk`** (probe is read-only). Only kill-point touching a boot multi-envelope window that durably allocates state before its CAS; `kill_point_matrix` K1-K6 don't cover it (REC-3 tests the scanner in isolation with a hand-seeded trace).
**Fix-class.** Test (K7) + mechanical: resume the existing in-flight `attempt_no` instead of re-allocating.

---

## HYPOTHESIS

### M1-H1 | HIGH(hyp) | L2 | boot_phase.rs:1738-1748 (residual docblock) + app.rs:652 gate + runtime startup (OUT of M1)
A stale restored node may **trade before the tip-guard converges** in two windows: (a) a restored PREPARED/SIGNED doc is re-driven via `send_chk` *before* the guard → `answered_wire_contact()==true` → guard **skipped** for that FN (architect-accepted residual at `:1738-1748` — and **M1-01 widens it**: StateConflict/DocumentMissing also skip with zero wire); (b) kill between the tip-guard probe and the BLOCK envelope leaves the node un-blocked. Whether ingress can accept a sale before the post-boot guard runs depends on health-gate vs boot-loop ordering in `runtime/container.rs` + the ingress shells — **outside M1**, hence HYPOTHESIS. If ingress is gated on `/health/ready` AFTER the full boot loop, window (b) closes for first boot; window (a) is a confirmed structural hole. **Recommend the M1/M3 seam-pass (§2b) verify startup sequencing.**

### M1-H2 | LOW(hyp) | L7 | online_convergence.rs:103 vs backlog_drain.rs:665
The drain/convergence mode-partition is a read-then-act outside any shared lock. Benign in current code: `GoingOnline→Online` flips ONLY inside the drain's `commit_finalize_envelope` and ONLY when every offline-session doc is already ACK (`backlog_drain.rs:2250-2434`), so at the flip instant the offline cohort has no resting SENT/KVT1 for convergence to grab; reverse edge doesn't exist. Safety is **emergent** (mode-state coupling), not lock-enforced — fragile to future changes that relax the eligibility gate or add a second online writer.

---

## Suspects cleared (negative results — verified)
- **ER-class dispatcher completeness** — all 7 `RetryClass` variants handled exhaustively + explicit `None` arm (`er_redrive_policy.rs:86-114`); no transient class → terminal. CLEAN.
- **MAX_BOOT_ATTEMPTS semantics** — cap checked *before* the attempt; counts wire `attempt_no`; ≤5 sends then escalate (`fixture_9g`). No off-by-one. CLEAN. *(But see M1-04: probe rows pollute the count.)*
- **DocState dispatch completeness** — match handles all 8 non-terminal states, explicit `bail!` on the 5 terminal, no catch-all `_` (`:3054-3245`); cohort WHERE (`fiscal_documents.rs:492`) == the 8 → no dead arm, no silent drop. CLEAN.
- **run_boot_reconciliation partition** — all 7 NodeMode + Opening/Closing ShiftState gating verified complete (`:1424-1617`). CLEAN.
- **Invariant #1 (no wire/crypto in tx)** — 14 `with_immediate` envelopes inventoried; none contains/reaches a wire or crypto call. CLEAN.
- **Guard token authenticity** — `for_integration_test_only` is `#[cfg(any(test, test-support))]`, `default=[]`; `from_app_mutex` consumes a held `MutexGuard`; production cannot mint a token without the mutex (`guard.rs:81-136`). CLEAN.
- **boot-vs-ingress ordering** — boot reconcile awaited to completion *before* any loop/ingress spawn (`supervisor.rs:149-181`); inline write-path dormant (`UnimplementedWritePath`). CLEAN. *(Caveat: feeds M1-H1 once A2.4 wires the live path.)*

## OUT-OF-SCOPE (one line each)
- M5: DPS status `-3` ERROR_SAVE → terminal `REJECTED` (should be retryable) — known M5 suspect, not in boot_phase.
- M4: `attempts_used` no-filter semantics (`transport_trace.rs:399`) is a repository contract — root of M1-04; fix may live in M4.

## Architect rulings
_(Fable: append rulings here; re-run M1-01/M1-02/M1-04 repros first.)_

**(Fable 5, 2026-06-11; M1-01/02 re-verified in code; seam-pass §2b folded in below.)**

- **M1-01 (HIGH) — CONFIRMED by architect read** (`Ok(_) => *_dispatched` absorbs the two
  zero-wire outcomes; same class as the B1 Mismatch-filter bug). FIX (batch item 1, semantic
  but narrow): match `{Sent,Routed}` explicitly into `*_dispatched`; route
  `StateConflict|DocumentMissing` into new non-answered buckets; pinning test per the dossier
  repro + a unit pinning the FULL `StageSendOutcome→bucket` mapping (so a future variant
  cannot be silently absorbed — the classification-obligation comment gets teeth).
- **M1-02 (HIGH) — arm defect CONFIRMED; reachability UNRESOLVED.** Seam-pass finding: the
  MAC-chain seed (`stage_sign.rs:286`, seed = `node_state.last_known_unsigned_xml_sha256`)
  suggests a second doc signed while the first rests SENT would carry a stale
  `previous_hash` — DPS would likely reject it, making "≥2 acked SENT" unreachable online.
  But this is exactly an unverified "likely". FIX (batch item 2): (a) **reachability pin
  test** — two sequential `inline::run` on one FN, first resting SENT via Hold; record what
  actually happens to the second (acquire? sign seed? send?) + `invariant_scan`; this decides
  final severity mechanically. (b) regardless of (a), harden the shared arm (one fix point —
  boot + B1 tick both use it): on `Mismatch`, if the probed doc is NOT the newest submitted
  doc (`lnd < max lnd over {SENT,KVT1,KVT2,ACK}` with sfn), do NOT terminalise to Manual —
  no-state-change + `TIP_SUPERSEDED`-style audit (doc-scoped confirmation is the deferred
  ER-probe/B1-v2 protocol spec; monitoring v1's stuck-detector owns visibility). Mismatch on
  the actual tip → Manual stays.
- **M1-03 (MED) — ruled: defer is the INTENDED semantics** (safe direction; no false
  terminal). Fix the CONTRACT, not the behaviour: amend `last_chk_probe.rs` docs
  (DecodeEscalate → "caller defers"; add `Unexpected` as the third fall-through in the
  header). Unbounded-defer visibility belongs to monitoring v1 (stuck-FN detector) — noted
  as an input to that spec. Doc-only batch item 3.
- **M1-04 (MED) — ACCEPTED as mechanical fix** (batch item 4): `attempts_used` counts only
  send attempts (filter by the column that distinguishes a real envelope from a probe row —
  if the existing schema cannot distinguish them unambiguously, STOP and escalate; a 002
  migration adding an explicit kind is acceptable post-squash if needed) + SQL pin test.
  M4-contract root noted for the M4 dossier.
- **M1-05 (MED) + M1-H2 — ruled: pin now, unify at A2.4.** The mode-partition
  (GoingOnline↔drain / Online↔convergence) is declared LOAD-BEARING: batch item 5 adds
  cross-referenced comments at all three sites (drain entry, convergence entry, fn_gate
  contract #4 with "deferred to A2.4 integration") + the dossier's partition rationale.
  Unification under fn_gate goes to the A2.4 checklist (already on the deferred list).
- **M1-06 (LOW) — ruled: document, don't thread.** ADR-M3-A10 §4 carry-forward comment on
  the helpers' module header. Token-threading through ~8 helpers is churn without a
  current attacker (single-process; CAS protects). Batch item 6 (comment only).
- **M1-07 (LOW) — ruled: accept + amend wording** ("manual triage" → "held; surfaced via
  monitoring stuck-detector"); input to monitoring v1 spec. Batch item 7 (doc-only).
- **M1-08 (LOW) — ruled: K7 kill-point test YES** (the only multi-envelope boot window
  with durable pre-CAS state — belongs in the matrix); orphan double-allocation accepted
  as benign ONCE M1-04 lands (budget pollution was its only harm) — comment, no resume
  mechanics. Batch item 8.
- **M1-H1 — seam-pass verdict:** window (b) closed for first boot today (Opus verified
  boot-awaited-before-ingress; inline path dormant). Window (a) = the documented PR#141
  residual, WIDENED by M1-01 — batch item 1 narrows it back; full closure remains the
  pre-arms ACK-tail check spec (post-legacy-review, unchanged). **A2.4 checklist gains:
  re-verify health-gate vs boot ordering with the live write path.**
- **Seam-pass summary (§2b):** gate↔arms (M1-01) and probe↔ledger (M1-02 chain-seed
  question) were the two live seams; both are now pinned by batch items. drain↔convergence
  lock seam ruled in M1-05. No additional cross-module finding beyond the dossier.

**Fix batch:** single branch `fix/m1-batch` (Opus, by the B1-era fence rules: reuse, no new
transitions beyond ruled ones, escalate on divergence), items 1-8 above; gates: full
nextest + fmt + clippy + the new pins. Items 1, 2 and 4 carry production-semantics changes —
architect re-reviews the delta before merge.

## External critic (GPT, different lab) — adjudicated by architect (Fable, 2026-06-11)

Independent second-lab critic pass over the dossier (esp. the "Suspects cleared" section).
Every candidate re-verified in code by the architect before ruling. **Two of our seven
CLEAN verdicts were BROKEN — the uncorrelated contour earned its keep.**

- **NC-03 | FT | CONFIRMED — breaks "run_boot_reconciliation partition CLEAN".**
  `run_boot_reconciliation` branch (a) (`boot_phase.rs:1386`): a missing `node_state` row
  bootstraps `upsert_initial(Online, Closed, next_lnd=1)` with **ZERO cross-check of existing
  `fiscal_documents`/`shifts`** for the FN (verified: the `let Some(row) = row else {…}` resets
  the LND allocator + MAC-seed projection unconditionally). If a restore/corruption/rebaseline
  loses only `node_state` while the ledger survives, boot silently resets the allocator → next
  write either fail-closes on `ux_fd_fn_lnd` OR (sparse/imported history) allocates BELOW the
  existing tail and signs with `previous_hash=None` → duplicate lnd / broken MAC chain.
  Reachability is low under whole-file snapshot restore (node_state + docs travel together),
  but the **defensive gap is real and FT-class** — an allocator/seed reset with no ledger
  guard is a latent landmine, and it composes with the tip-guard restore-safety story.
  FIX (batch, FT — architect re-review): branch (a) must query the ledger first; if ANY
  `fiscal_documents` row exists for the FN → do NOT reset to 1 — derive `next_lnd` from
  `MAX(lnd)+1`, project the MAC seed from the last ACK, and **BLOCK + CRITICAL** rather than
  silently bootstrap (a node whose node_state vanished under it is not healthy). Pin: SQL repro
  from the candidate + `invariant_scan` post-condition.
- **NC-02 | MED (downgraded from GPT's HIGH) | CONFIRMED — breaks "MAX_BOOT_ATTEMPTS CLEAN"
  at the DOMAIN level.** Arithmetic is clean (dossier holds), but the budget is `doc_type`- and
  wall-clock-blind: 5 TransientRetry attempts → Manual contradicts §16 (transport-class for
  shift open/close is unbounded; `-3` ERROR_SAVE needs ≥30 attempts over ≥60 min). Downgraded
  because the affected paths aren't live: SHIFT_OPEN/Z_REPORT are fail-closed until A2.2, and
  `-3 ERROR_SAVE→retryable` is a known M5 backlog item. NOT a fix-now in the M1 batch.
  Ruling: PIN a test (shift-doc TransientRetry → premature Manual) marked `#[ignore]` with a
  comment, and route the doc_type/wall-clock policy decision to **A2.2 (shift goes live) + M5
  (ERROR_SAVE)**. The dossier's "MAX_BOOT_ATTEMPTS CLEAN" is amended to "arithmetic clean;
  domain policy deferred to A2.2/M5".
- **NC-01 | HIGH→MED (reachability) | CONFIRMED structural inconsistency.** `last_chk_probe::probe`
  (`:85`) Matches on `ack.id == expected` **ignoring `data_sign` emptiness**, and
  `advance_sent_to_kvt1_from_probe` persists `ack.data_sign` into `Kvt1Raw` with **no
  `is_empty` guard** — verified. This diverges from (a) the W12 classify contract (empty
  data_sign → Hold, never advance) and (b) `kvt2_advance::advance_to_ack`'s own
  `StructuralDrift`-on-empty guard. So the probe path can advance SENT→KVT1 persisting EMPTY
  forensic KVT1 bytes. Downgraded to MED on reachability (DPS returning a matching id with
  empty `data_sign` is abnormal), but the fix is cheap and the inconsistency is exactly the
  L3 class. FIX (batch): mirror the empty-guard — `probe` routes empty `data_sign` to a
  Hold-equivalent (no advance), OR `advance_sent_to_kvt1_from_probe` fail-louds on empty like
  `advance_to_ack`. Pin: K4-variant with matching id + `vec![]` → assert NO advance / non-empty
  KVT1_RAW invariant.
- **NC-04 | MED | CONFIRMED defensive gap — composes with NC-03.** `last_submitted_server_fiscal_no`
  filters `sfn IS NOT NULL AND length>0` (verified `:713`), so a malformed newer SENT/KVT1 row
  with NULL/'' sfn is excluded → tip-guard can validate an OLDER tail as `TIP_GUARD_OK` while
  the real max-lnd submitted row is unrecoverable+pending. SENT-with-NULL-sfn is unreachable in
  healthy code (SENT ⇐ WireDecision::Sent stamps sfn), but malformed rows are exactly what a
  restored/corrupted DB carries — the same domain the tip-guard exists for. FIX (batch, with
  M1-02 arm work): if the newest `{SENT,KVT1,KVT2}` row by lnd has NULL/empty sfn (i.e.
  `last_submitted` skipped a higher-lnd pending row) → tip-guard treats it as a block-worthy
  anomaly, not silent OK.

**Cleared-section verdict after adjudication:** 5/7 hold; 2 BROKEN (partition→NC-03,
MAX_BOOT_ATTEMPTS→NC-02). GPT raised no hard disagreement with our 8 CONFIRMED; concurs M1-02
stays HIGH until the reachability pin exists.

**Fix-batch scope grows by 3** (NC-01, NC-03, NC-04 join items 1-8); NC-02 is pin-only
(deferred A2.2/M5). NC-03 is FT → architect re-reviews its delta with the same scrutiny as
the 002 migration.
