# Batch D (LOW cleanup) + K8/K9 (durability pins) — implementer spec

**Architect-locked, 2026-06-14.** Base: `origin/main` **`ef28944`** (post #164/#165/#166).
Non-blocking follow-up (NO-GO already lifted). One PR `fix/batch-d-k8-k9` off fresh
`origin/main`. TDD. Minimal diff. **DO-NOT-MERGE** — architect reviews the delta.

**Gate (all under pinned 1.95.0, `~/.cargo/bin/cargo`):**
```
cargo nextest run -p prro --features test-support
cargo fmt -p prro -p prro_crypto -p prro_escpos -- --check
cargo clippy -p prro --all-targets --no-deps --features test-support -- -D warnings
```

> **Every file:line below was re-verified against `ef28944` by an adversarial 4-agent
> verification pass (2026-06-14).** That pass corrected the prior draft on five points —
> see the per-item notes flagged **[verify-fold]**. Re-check anchors after any rebase.

---

## Verification summary (what the adversarial pass changed)

| Item | Prior draft | Corrected truth (ef28944) |
|------|-------------|---------------------------|
| D1 severity | "active fiscal-truth blind spot check-5 is blind to" | **Latent / defense-in-depth.** No Rust write-path writes `ingress_inbox.status='ERROR'` (only PROCESSING/REJECTED/DONE). The widen is scanner↔replay **parity / forward-compat**, not a live-leak fix. |
| D1 anchor | `replay.rs:152` | `replay.rs:153` (line 152 is the comment). |
| D1 scope | (implicit) | Do **NOT** widen `fd.state` beyond `('ACK','OFFLINE_LOCAL_ACK')` — that is check-6d's set (`:309`), a different invariant. |
| D2 surface | "annotate `(OfflineLocalAck,Cancelled)` + siblings" | **13 of 29** whitelist edges have no production invoker, in **4 classes**; and **no `fiscal_documents` force-seam exists in code** → the "operator/force-seam only" annotation text was factually wrong. |
| D3 anchor | `app.rs:373` cited as STOP_MODE contract | `app.rs:373` is **stale** (boot DB-integrity code). Real contract = `NODE_STOP_MODE` in `ingress/handler.rs:162,1560` + `seam.rs:138`. |
| K9 `unsigned_xml_sha256` | "compute helper" | **Persisted column** on `fiscal_documents` (XML builders are private). Read it, don't recompute. |
| K8/K9 helpers | `k6`, `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` | `k6` is a **test name** (reuse its seed-sequence); the audit marker is a **string literal**, not a const. |

---

## D1 — AUD-L8-2: `invariant_scan` check-5 latent-blind to `ERROR`-inbox

**Where:** `rust/prro/src/db/invariant_scan.rs` — check-5 (`RejectedInboxWithAcceptedDoc`):
SQL block `:238-243`, WHERE clause `:242`, `Violation` variant decl `:72` (doc-comment `:69-71`),
push site `:247`. Current guard:
```sql
WHERE i.status = 'REJECTED' AND fd.state IN ('ACK', 'OFFLINE_LOCAL_ACK')
```

**Defect (latent — [verify-fold]):** check-5 is the AUD-1 oracle — a terminal-fail inbox row must
not coexist with an *accepted* ledger doc, else replay short-circuits a fiscalised receipt to a
false `Failed`. The replay resolver short-circuits on **both** terminal-fail inbox statuses —
`runtime/ingress/replay.rs:153`: `if matches!(replayed.status.as_str(), "REJECTED" | "ERROR")`
→ `Failed("INBOX_REJECTED")` (`:154-158`). check-5 guards only `'REJECTED'`, so an `'ERROR'`
inbox over an `ACK`/`OFFLINE_LOCAL_ACK` doc is the same fiscal-truth lie the scanner cannot see.

**Severity is LATENT, not active.** No Rust write-path mutator ever sets `ingress_inbox.status
= 'ERROR'` — `db/repositories/ingress_inbox.rs` writes only PROCESSING (`:294`), REJECTED
(`:361,392,420`), DONE (`:461`). `'ERROR'` is permitted by the CHECK constraint
(`migrations/001_baseline.sql:86`: `status IN ('NEW','PROCESSING','DONE','REJECTED','ERROR')`)
and defensively handled by replay, but is **not currently produced**. So there is no reachable
false-negative today. **The widen is still correct** — it keeps the scanner in lock-step with
replay's defensive contract (so the two never drift) and guards a future / migration-injected /
externally-written `ERROR` row. Frame the PR as *scanner↔replay parity*, not a live leak.

**Change (the only real code edit in Batch D):**
```sql
WHERE i.status IN ('REJECTED', 'ERROR') AND fd.state IN ('ACK', 'OFFLINE_LOCAL_ACK')
```
- This inbox-side set `{REJECTED, ERROR}` is **complete** — verified: it is the *only*
  inbox-status short-circuit-to-`Failed` before the ledger consult (replay module doc `:17`
  + the `matches!` at `:153` + the CHECK domain). The later `(DONE, …)` / `is_terminally_failed`
  arms (`replay.rs:166-`) are the **inverse axis** (inbox=DONE / ledger-failed) and are out of
  scope for check-5.
- **Do NOT widen `fd.state`.** Keep it `('ACK','OFFLINE_LOCAL_ACK')` — the authoritative
  accepted-ledger predicate is `is_accepted(state)` = `Ack | OfflineLocalAck`
  (`replay.rs:49-51`). SENT/KVT1/KVT2/ERROR_RETRYABLE are in-flight (replay → `InProgress`),
  not fiscalised — pairing them with a terminal-fail inbox is *not* a replay-lie. The 5-state
  widening at `invariant_scan.rs:309` belongs to **check-6d** (drain-cohort session stamp) —
  a different invariant; do not cross-apply it here.
- Update the `Violation::RejectedInboxWithAcceptedDoc` doc-comment (`:69-71`) **and** the
  check comment (`:236-237`) from "REJECTED inbox" → **"REJECTED/ERROR inbox"**, and reference
  `replay.rs:153` as the single source of the status set (the OFFLINE_ISSUED_STATES / M2-N2b
  parity pattern). Keeping the variant name/comment coherent after the widen avoids a
  name↔behavior drift like the M2-X3 rename. Hoisting the literal set to a shared const is
  optional (only if it reads cleanly).

**Pin (TDD)** — `tests/invariant_scan.rs`, mirror the existing REJECTED test
`detects_rejected_inbox_with_accepted_doc` (`:353`; ACK seed via `seed_doc(...,"ACK",...)`,
direct `ingress_inbox` INSERT `:372`, assert `:383`):
- ERROR-variant: seed an `ACK` (or `OFFLINE_LOCAL_ACK`) doc + **directly INSERT** an
  `ingress_inbox` row `status='ERROR'` for the same `request_id` → `scan()` returns one
  `RejectedInboxWithAcceptedDoc`. (Must INSERT directly — no service produces `ERROR`; there is
  no end-to-end repro, exactly as the REJECTED test inserts directly.) Parametrising over both
  statuses is acceptable.
- Benign: `status='ERROR'` + **no** accepted doc → clean (the widen must not over-fire).

**Invariant:** read-only scan, no state change. Strengthens AUD-1 (scanner↔replay parity), zero
behaviour change to production paths.

---

## D2 — AUD-L1-2: annotate no-production-invoker whitelist edges (doc-only)

**Where:** `rust/prro/src/db/repositories/fiscal_documents.rs` — `allowed_transition` (`:171`),
a single `matches!((from,to), …)` of **29 edges** (count pinned by the `:220` drift-guard
comment + `tests/fiscal_documents_offline_local_ack_edges_locked.rs`).

**Finding ([verify-fold] — bigger than the prior draft):** **13 of 29** whitelisted edges have
**no production CAS invoker**. Critically, **there is no `fiscal_documents` operator/force-seam
anywhere in `src`** (grep `force_transition|operator_force|force_seam|ForceTransition|manual_transition`
→ only `shifts.rs`, a different table). So the prior draft's annotation text *"operator/force-seam
only (runtime wiring deferred)"* describes a seam that does not exist — it must be rewritten
**per class**:

| Class | Edges (with whitelist line) | Accurate annotation |
|-------|------------------------------|---------------------|
| **A — Pattern-A (M3a) vestige** (Encrypted is never a transition *target* under Pattern B) | `(Signed,Encrypted)` L177, `(Encrypted,Sent)` L180, `(Encrypted,Sending)` L232 | `// no auto-invoker — Pattern-A (M3a) vestige; Encrypted is never a transition target under Pattern B. Retained add-only (W6).` |
| **B — legacy placeholder** | `(OfflineLocalAck,Sent)` L197 | already documented at `:205-209` ("legacy M3a placeholder … add-only") — **leave as-is**, do not duplicate. |
| **C — future-wired** | `(OfflineLocalAck,Kvt2)` L221 | already has the W12 lastChk comment at `:221` — **leave as-is**. |
| **D — no producer, no seam** | `(Prepared,Rejected)` L176, `(Signed,ErrorRetryable)` L178, `(Sent,Rejected)` L184, `(Kvt1,ErrorRetryable)` L195, `(OfflineLocalAck,Cancelled)` L211, `(ErrorRetryable,Sent)` L222, `(ErrorRetryable,Kvt1)` L223, `(Sending,Kvt1)` L234 | `// no production invoker; no operator/force seam exists today — forward-compat reserve, retained add-only (W6).` |

**Change (doc-only, no logic):** add the class-appropriate one-line annotation to each edge in
Class **A** and Class **D** that lacks a no-invoker marker. Leave Class B (`:205-209`) and Class C
(`:221`) comments intact — only augment if they lack a no-invoker note. **Do not add/remove any
edge**; keep the `:220` count and the locked-edges test green (this is annotation only).

**[architect flag — do NOT act in this batch]:** the 8 Class-D edges are genuinely dead today
(no producer, no seam, not future-planned). Resolving them — (a) wire a `fiscal_documents`
operator/force seam, or (b) formally downgrade them to a documented forward-compat reserve, or
(c) remove under a deliberate drift-guard-count change — is a **separate architectural decision**,
not a LOW doc batch. Record it as a follow-up; this batch only documents the reality accurately.
If the implementer finds an edge they believe is invoked after all (a dynamic/runtime-built
`(from,to)` the static trace missed), flag it to the architect rather than annotating it dead.

**Transition API (for the invoker audit):** general CAS `transition_state(tx,id,from,to)`
(`:306`, gates on `allowed_transition` `:312`); specialised `transition_to_offline_local_ack_tx`
(`:396`); boot wrapper `transition_with_audit` (`boot_phase.rs:92`). `offline_sessions::transition_state`
(`:204`) and `shifts::transition_state` (`:276`) are **different tables/whitelists** — ignore.

**Pin:** none beyond the existing locked-edges test staying green (no edge-set change).

---

## D3 — AUD-L3-1: fix the Tier-2 STOP_MODE auto-recovery comment (doc-only)

**Where:** `rust/prro/src/services/offline_sync/backlog_drain.rs` — doc-comment over
`async fn trigger_tier_2_stop_mode` at **`:2214-2232`** (fn decl `:2233`). Misleading text:
- `:2220` — `/// (existing STOP_MODE contract per app.rs:373 + return_online_probe);`
- `:2221-2222` — `/// existing held docs remain в Sent/Kvt1 з накопиченим counter для\n/// auto-drain post-recovery.`

**Finding ([verify-fold] — CONFIRMED):** STOP_MODE is **not** auto-recovered/auto-drained.
- `return_online_probe` **skips** a STOP_MODE node: `run_tick_for_fn` step-1 match returns
  `Skipped { reason: NodeNotOfflineOrTransition }` for `Blocked | StopMode | CryptoDegraded |
  GoingOffline` (`return_online_probe.rs:260-268`; skip-reason doc `:177-179`). Its only mutation
  is the success CAS `UPDATE node_state SET mode='GOING_ONLINE' WHERE … mode='OFFLINE'`
  (`:362-363`) — gated on `mode='OFFLINE'`, so a STOP_MODE row can never be flipped by it.
- `drain()` refuses any non-`GoingOnline` node: step-1 gate emits
  `OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE` and returns early (`backlog_drain.rs:687-704`).
- Chain: probe skips STOP_MODE → node never reaches `GoingOnline` via the probe → drain skips
  non-`GoingOnline` → **no automated path out of STOP_MODE**. Exit requires an operator-driven
  return-online within the 36h offline-cap window (cert.NotAfter − 2160 min), matching operator
  policy `feedback_manual_recon_catastrophe`.
- **`app.rs:373` is a stale reference** — that line is boot DB-integrity code (PRAGMA
  `quick_check` / `open_secure_pool`), *not* a STOP_MODE contract. The real ingress-reject
  contract is the `NODE_STOP_MODE` node-mode code in `runtime/ingress/handler.rs:162,1560`
  + `runtime/ingress/seam.rs:138`.

**Change (doc-only, STOP_MODE logic unchanged):**
- `:2220` → `/// (node-mode ingress reject — code NODE_STOP_MODE per runtime/ingress/handler.rs + seam.rs);`
  (drop `app.rs:373` and the `return_online_probe` citation).
- `:2221-2222` → state that held docs remain in Sent/Kvt1 with accumulated counter but are
  **NOT auto-drained** — `return_online_probe` skips STOP_MODE (`:260-268`) and `drain` runs only
  at `mode==GoingOnline` (`:687-704`); exit requires **operator-driven return-online** within the
  36h offline-cap window (cert.NotAfter − 2160 min), only after which the backlog may drain.
- Cite module/symbol names, **not** hard line numbers (avoid re-introducing drift like `app.rs:373`).

**Do NOT** "fix" this by adding any auto-recovery code path — refusing to auto-drain STOP_MODE is
the correct, operator-mandated behaviour.

**Pin:** none (comment-only). State the verified `return_online_probe` STOP_MODE behaviour in the PR.

---

## K8 — durability: crash mid strict-sequential drain → idempotent re-tick (terminal-reject class)

**Companion to M2-N1.** New test in `rust/prro/tests/kill_point_matrix.rs`, reusing the M2-N1
setup. **Crash class = TERMINAL reject** (the halt+escalate path), where the re-tick must be an
idempotent **no-op** (not a clean retry — that is the transient-hold class, out of scope here).

**Reuse (verified real names):**
- Base test to copy: `m2_n1_three_real_offline_sells_strict_drain_halts_on_reject` (`:1854`) —
  3 real offline SELLs via `inline::run` (`seed_inbox_sell_keyed` `:1066`), plain `Opened` shift,
  `GoingOnline`; DPS stub: doc1 send OK + lastChk ACK, doc2 → reject.
- DPS reject: in-file `KpStub` (≈`:60`) `push_send(Err(DpsError::Authorization{ code:-1,
  kind: AuthorizationKind::DocumentReject, message: "...".into() }))` — `AuthorizationKind::DocumentReject`
  at `transports/dps/error.rs:116`; example call `:1911`. **Size the stub queues to exactly the
  sends expected to reach the wire** (tick 1 = 2 sends; doc3 never sent) — an over-send pops an
  empty queue and panics.
- Drain entry: `backlog_drain::drain(&recon_guard(), &pool, &view, FN)` —
  `recon_guard()` (`:386`, wraps `ReconcileGuard::for_integration_test_only`),
  `view: RuntimeView` (`reconciliation/runtime.rs:64`, `{dps, signing_ctx, fn_sign}`).
- Audit marker: string literal `"OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL"` (`backlog_drain.rs:2352`,
  emitted by `escalate_drain_to_manual` `:2306`) — **not a const**; assert via
  `SELECT COUNT(*) FROM audit_log WHERE event_type='OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL'`.
- Shift state: `SELECT state FROM shifts WHERE shift_id=?` == `"REQUIRES_MANUAL_RECONCILIATION"`
  (pattern `:1956-1961`). (Plain `Opened` escalates via whitelist edge 15.)

**Scenario:**
- **Tick 1** = `drain(...)` → doc1 `ACK`, doc2 `REJECTED`, FN escalated
  `RequiresManualReconciliation` (edge 15), doc3 stays `OFFLINE_LOCAL_ACK`, returns `Ok`.
  (This is the M2-N1 pin — reuse it as the precondition.)
- **Tick 2** = `drain(...)` **again** with a **fresh `RuntimeView` + fresh `KpStub`** on the
  same `pool`/FN (simulates the post-crash boot / supervisor re-tick). ASSERT:
  - doc1 still `ACK`, doc2 still `REJECTED` (state-stable, no re-process),
  - doc3 still `OFFLINE_LOCAL_ACK` — **not sent** (tick-2 stub receives **0** sends; the halted
    cohort never advances past the reject),
  - exactly **ONE** `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` audit row and shift still
    `REQUIRES_MANUAL_RECONCILIATION` (no double-escalation on the re-tick — COUNT the audit rows),
  - `prro::db::invariant_scan::assert_clean(&pool).await` (`invariant_scan.rs:322`).

**Why:** pins that the strict-halt + escalate is a safe no-op on re-entry, so a crash between
doc2's escalate-commit and the loop's return cannot double-send doc3, double-escalate, or break
the ledger. (Re-tick mechanism precedent: `tests/backlog_drain_per_doc_loop.rs:1424` calls drain
twice with v1 then v2.)

---

## K9 — durability: crash mid offline-session → seed survives, chain unforked

**Companion to M2-01 / AUD-L6-1.** New test in `tests/kill_point_matrix.rs`, reusing the K6
offline-session **setup sequence** ([verify-fold] — `k6` is the *test* `k6_offline_local_ack_drains_to_ack`
at `:406`, **not** a helper fn).

**Reuse (verified real names):**
- Offline-session setup sequence: `seed_fn_config` (`:204`), `seed_open_shift` (`:226`),
  `seed_node_state_offline` (`:258`), `seed_open_offline_session` (`:283`), `seed_offline_code`
  (`:298`, ≥2 codes), then `inline::run` lands `OFFLINE_LOCAL_ACK`. (`set_node_mode` `:274` for
  the GoingOnline flip if a drain is exercised — not required for the chain assert.)
- Seed field: `node_state.last_known_unsigned_xml_sha256` is **public** on `NodeStateRow`
  (`node_state.rs:40`); read it directly via `SELECT last_known_unsigned_xml_sha256 FROM node_state`
  (M2-01 test does this ≈`:1529`).
- **`unsigned_xml_sha256` is a PERSISTED COLUMN, not a helper** ([verify-fold]) — computed inline
  in `stage_sign.rs:709` and written to `fiscal_documents.unsigned_xml_sha256`; the XML builders
  (`build_canonical_doc`/`build_canonical_xml`, `stage_sign.rs:974`) are **private**. Read the
  column in-test via `chain_col(pool, lnd, "unsigned_xml_sha256")` (`:1510`). **Do not** import or
  recompute it — a test calling a `unsigned_xml_sha256(...)` helper will not compile.
- `previous_hash` read: `chain_col(pool, lnd, "previous_hash")` (`:1510`) / `col_by_id` (`:1620`)
  / `col1` (`:1746`); chaining assert pattern at `:1519-1527`.

**Scenario:** the MAC seed advance at offline-ack commits under `synchronous=FULL`, so a crash
after doc1's offline-ack but before doc2 signs must leave doc2 chaining off doc1.
- Setup: node Offline, open offline session, ≥2 offline codes.
- Process doc1 via real `inline::run` → `OFFLINE_LOCAL_ACK`; capture `h1` =
  `SELECT last_known_unsigned_xml_sha256 FROM node_state` and confirm `h1 ==
  chain_col(pool, 1, "unsigned_xml_sha256")` (the seed advanced — M2-01).
- "Crash + reboot": do not re-derive anything (optionally re-open the pool on the same DB file to
  model a process restart — the seed is durable). doc2's inbox row is still NEW.
- Process doc2 via `inline::run`. ASSERT:
  - `chain_col(pool, 2, "previous_hash") == h1` (doc2 chains off doc1's survived unsigned seed),
  - `chain_col(pool, 1, "previous_hash") != chain_col(pool, 2, "previous_hash")` (distinct — a
    real chain, not the pre-M2-01 fork),
  - `SELECT last_known_unsigned_xml_sha256 FROM node_state == chain_col(pool, 2, "unsigned_xml_sha256")`
    afterwards,
  - `prro::db::invariant_scan::assert_clean(&pool).await` (catches `ChainBreak` `:56` /
    `ChainSeedMismatch` `:64`) — assert clean before drain too.

**Why:** pins that the seed advance is crash-durable and an offline session spanning a restart
produces a correct (unforked) MAC chain — the durability companion to the AUD-L6-1 boot projection.

---

## Sequencing
D1 (real fix) + D2/D3 (doc) + K8 + K9 in ONE PR `fix/batch-d-k8-k9`. Independent of the
online-lane batch (Batch C + AUD-L5-1). Per the dual-session split this is implementer work; the
architect reviews the delta. The D2 **architect flag** (8 dead Class-D edges; no force-seam) is a
separate follow-up — do not act on it in this batch.

---

## Appendix — verified symbol map (ef28944)

| Symbol | Location |
|--------|----------|
| check-5 SQL / WHERE / variant / push | `invariant_scan.rs` `:238-243` / `:242` / `:72` / `:247` |
| replay short-circuit `matches!(REJECTED\|ERROR)` | `runtime/ingress/replay.rs:153` (comment `:152`, Failed `:154-158`) |
| `is_accepted` (Ack\|OfflineLocalAck) | `replay.rs:49-51` |
| check-6d 5-state set (do NOT cross-apply) | `invariant_scan.rs:309` |
| REJECTED-variant test to mirror | `tests/invariant_scan.rs:353` |
| inbox status writers (no ERROR) | `db/repositories/ingress_inbox.rs` `:294/:361/:392/:420/:461` |
| `allowed_transition` (29 edges, drift-guard `:220`) | `fiscal_documents.rs:171` |
| `transition_state` / `transition_to_offline_local_ack_tx` | `fiscal_documents.rs:306` / `:396` |
| `transition_with_audit` (boot wrapper) | `boot_phase.rs:92` |
| `trigger_tier_2_stop_mode` comment / fn | `backlog_drain.rs:2214-2232` / `:2233` |
| `return_online_probe` StopMode skip / success CAS | `return_online_probe.rs:260-268` / `:362-363` |
| `drain` GoingOnline-only gate | `backlog_drain.rs:687-704` |
| real STOP_MODE ingress contract | `ingress/handler.rs:162,1560` + `ingress/seam.rs:138` |
| M2-N1 base test | `tests/kill_point_matrix.rs:1854` |
| `inline::run` | `services/write_path/inline.rs:385` |
| `backlog_drain::drain` | `backlog_drain.rs:671` |
| `recon_guard()` / `ReconcileGuard::for_integration_test_only` | `kill_point_matrix.rs:386` / `guard.rs:131` |
| `RuntimeView` | `reconciliation/runtime.rs:64` |
| `node_state.last_known_unsigned_xml_sha256` | `node_state.rs:40` (read SELECT ≈kpm `:1529`) |
| `unsigned_xml_sha256` (COLUMN; compute `stage_sign.rs:709`, builders private `:974`) | read `chain_col(pool,lnd,"unsigned_xml_sha256")` `:1510` |
| `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` (string literal) | `backlog_drain.rs:2352` (`escalate_drain_to_manual` `:2306`) |
| `AuthorizationKind::DocumentReject` | `transports/dps/error.rs:116` |
| K6 offline setup seeds | `kill_point_matrix.rs` `:204/:226/:258/:283/:298` |
| previous_hash readers | `kill_point_matrix.rs` `:1510/:1620/:1746` |
| `invariant_scan::assert_clean` / `scan` | `invariant_scan.rs:322` / `:115` |
