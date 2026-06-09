# RS-3 A2 — Wire the inline write-path orchestrator (`fiscalize`)

**Date:** 2026-06-09
**Author:** arch-planner + operator rev2
**Status:** PLAN rev2 (operator-reviewed-against-code; decisions signed; no code written)
**Predecessors (MERGED):** C2 · C1 (PR #116) · A1 (PR #117) · A1Z (PR #118) · A3 (PR #119) · A4 (PR #120)
**Successor:** B1 (stale-PROCESSING reaper) → Integration

---

## 0. rev2 — operator corrections (2026-06-09, signed against code)

Decisions **signed: (a) A1 · (b) accept + wider scope · (c) accept + caveat.** The
rev1 §5(a) recommendation (B) was WRONG and is overridden here; §4/§5 below are
patched to match this block, which is authoritative.

**(a) KVT2-confirm = A1 (inline runtime-neutral confirmer).** Verified: the
online happy-path ends at `Sent` (`stage_send.rs:490`); `stage_finalize::run`
starts at `Kvt2` (`stage_finalize.rs:240`); `Sent→Kvt1→Kvt2` is done ONLY by
`kvt2_confirm` (`kvt2_confirm.rs:600`). So A2's online ladder MUST drive the
`Sent→Kvt1→Kvt2` quittance via a **runtime-neutral confirmer extracted from
`kvt2_confirm`** — NOT a direct import of the drain wrapper (which leaks
`BootError` `kvt2_confirm.rs:51` + drain source-routing 1a/1b/1a-replay). The
extracted core yields `FiscalError`/IN_PROGRESS (no boot-fatal semantics); the
drain caller keeps its existing wrapper + tests green. Inline-window timeout →
leave doc `Sent`, return `Ok(FiscalOutcome{document_state: Sent})` → 202; drain/B1
completes it (NOT a hang).

**(b) C1 hook = accept, but scope is WIDER than finalize.** `run_with_shift_transition(..., Option<ShiftEdge>)`
is the CLOSE edge. But the OPEN edge cannot be a finalize-hook: today
`stage_acquire` for `ShiftOpen`-from-`Closed` yields `active_shift = None` and a
fiscal doc with NO `shift_id` (`stage_acquire.rs:627`; pinned by
`tests/write_path_stage1_acquire.rs:351`). A2.2 MUST port the master-plan
acquire-time shift-link contract: **on `stage_acquire`** create/link the shift +
set `node_state.current_shift_id` + `open_document_id`; for `Z_REPORT`/`SHIFT_CLOSE`
link **`z_report_document_id`, NOT `close_document_id`** (master-plan
`2026-06-08-rs3-live-write-path.md:65`). So the C1 hook is **OPEN-at-acquire +
CLOSE-at-finalize**, two callsites, ONE `apply_shift_transition` service
(`transition.rs:58`), each in its own short `with_immediate`.

**(c) `fetch_finalize_inputs_tx` = do NOT extend (accept).** Thread an explicit
`ShiftEdge { shift_id, from, to }`. CAVEAT: the edge is **persisted /
`WorkerContext`-derived**, NOT derivable from `operation_type` alone — `from`/`to`
depend on the created/active shift state + the chosen open/close intent. The
orchestrator computes it from `WorkerContext` (shift_id + acquire-time shift
state), not from the wire op string.

**Mandatory plan edits (folded into §4/§5):**
- A2.1 chain = acquire → sign → dispatch → send → **runtime-neutral KVT2 confirm** → finalize (NOT send → finalize).
- A2.2 = the C1 hook **+ the `stage_acquire` shift-link integration** (else the hook has no reliable `shift_id`).
- A2.0 must add the **A1Z gate mapping**: `FULL_Z_SURFACE_READY=false` (`z_builder.rs:41`) requires fail-closed for Z BEFORE aggregate/sign. The A3 `FiscalError`/HTTP taxonomy has NO `Z_SURFACE_NOT_READY` — **LOCK it before A2.4** (decision (d)).
- The `WorkerProcessResult::Noop` arm must be RESOLVED, not left "NotImplemented-or-replay?" (decision (e)).

**(d) NEW — `Z_SURFACE_NOT_READY` taxonomy (LOCKED, lands in A2.0; must exist before A2.4).**
Add `FiscalError::ZSurfaceNotReady { request_id }` → `error_code` `Z_SURFACE_NOT_READY`
→ **HTTP 501** (the live-Z fiscalization surface is not yet implemented/enabled
until W4-Z2 flips `FULL_Z_SURFACE_READY`; distinct from the whole-seam
`NOT_IMPLEMENTED`). A2's Z dispatch calls `ensure_full_z_surface_ready()?` FIRST
(gate→ensure→quiesce→aggregate_for_shift→build_z); on `Err` it returns
`ZSurfaceNotReady` (after taking the lease + terminalising the inbox row per the
A3 lease-before-refuse contract). After A2.4: SELL → 200/202, but `Z_REPORT`/`SHIFT_CLOSE`
→ 501 `Z_SURFACE_NOT_READY` (fail-closed pilot-gating, by design) until W4-Z2.
**CONFIRMED 2026-06-09: HTTP 501** (capability-not-yet-implemented, not a
transient outage). **Locked consequence (operator):** terminalising the inbox
as `REJECTED` means the SAME `idempotency_key` will NOT auto-fiscalize after
W4-Z2 — the client must submit with a NEW key. Accepted; the order is: take
lease/ownership → fail-closed → audit → REJECTED → no `fiscal_documents`.
**Implemented (A2.0 first commit): the `FiscalError::ZSurfaceNotReady` variant +
`Z_SURFACE_NOT_READY`→501 handler map + tests are in.**

**(e) NEW — `WorkerProcessResult::Noop` arm (RESOLVED).** `Noop` = "no `NEW` inbox
row / race-lost" (`stage_acquire.rs:61-69,190,212`; "no audit on Noop" contract
`:267`) = an idempotent re-entry / already-processed situation. Under A4 + the
handler's freshly-`Created`-`NEW` contract a Noop on the INLINE path is NOT
expected. Resolution: **Noop → defensive replay-resolve, NEVER `NotImplemented`**.
Look up the doc by `request_id`; if a terminal/in-flight doc exists, build the
matching `FiscalOutcome` via `replay::build_accepted` (terminal→Ack/etc,
in-flight→202); if NOTHING exists to resolve, that is a genuine invariant breach
→ audit Critical + return `FiscalError` (INTERNAL/500). ALWAYS audit an inline
Noop as unexpected (observability — it should not happen under A4). Rationale:
Noop's truthful answer is the existing ledger state, never a 501 phantom.
**CONFIRMED 2026-06-09 (operator), with emphasis:** (1) under A4 an inline Noop
is unexpected → **audit Critical is MANDATORY**; (2) **NEVER terminalise blindly**
— Noop means the row is no longer `NEW`, so a blind mark could clobber an already
`DONE`/`REJECTED` row; resolve from the ledger (terminal/in-flight → return it),
empty ledger → INTERNAL/500; (3) B1's stale-`PROCESSING`/no-doc case is a SEPARATE
reaper policy (its own ledger-check-first) — do NOT fold it into the inline-Noop arm.

---

## 1. Problem statement

A1 lands the InboxRow→`CanonicalFiscalCommand` builder. The production
`WritePathEntry::fiscalize` (`runtime/ingress/seam.rs:188` `UnimplementedWritePath`)
still returns `FiscalError::NotImplemented` — the inbox handler accepts
receipts but **nothing fiscalises**. All write-path stages exist and are
unit-tested but are **disconnected at the binary level**:

- `stage_acquire::run` (`stage_acquire.rs:48`) — **zero production callers**
  (verified: `grep stage_acquire::run` → only its own def). Confirms the
  runtime-spine gap.
- `stage_sign::run` (`stage_sign.rs:193`), `dispatch_post_sign`
  (`dispatch.rs:126`), `stage_send::run` (`stage_send.rs:936`),
  `stage_finalize::run` (`stage_finalize.rs:234`) — each individually tested,
  never chained.

A2 builds the **orchestrator** that chains acquire→sign→dispatch→{send→finalize | offline-ack}
and binds it as the real `WritePathEntry`, returning a typed `FiscalOutcome`
or one of the four real `FiscalError` variants. **A2 is the runtime spine for
the online + offline-ack ladders. KVT2-drain backlog stays on the W12 path
(`confirm_drain_doc`) — A2 does NOT duplicate it.**

Scope boundary (what A2 is NOT):
- NOT the axum server / supervisor wiring (RS-2 piece 5b).
- NOT a rewrite of any stage. Each stage's `run` signature is reused as-is.
- NOT W12 drain. The KVT2-confirm tail is reused, not forked (see §2 decision a).

---

## 2. Current relevant architecture (file:line anchored)

### Entry seam
- `runtime/ingress/seam.rs:174` `trait WritePathEntry { async fn fiscalize(&self, row: &InboxRow) -> Result<FiscalOutcome, FiscalError>; }`
- `seam.rs:185-194` `UnimplementedWritePath` — the binding A2 replaces.
- `seam.rs:150-173` **caller obligation contract** (load-bearing): the impl
  MUST establish the per-FN lease itself (invariant #2); returning any real
  `FiscalError` MUST leave the inbox row **non-`NEW` and terminal/audited**
  (else replay resolves to `202 IN_PROGRESS` forever — `replay.rs`). Z-vs-non-Z
  discriminator is `row.operation_type` (`Z_REPORT`/`SHIFT_CLOSE` → WIRE intent
  behind drain barrier).
- `handler.rs:472` releases inbox row **only** on `NotImplemented`; the four
  real failures (`ShiftNotOpen`/`SignFailure`/`DpsRejected`/`OfflineRefused`,
  `seam.rs:111`+) must self-terminalise.

### Stages (all reused, signatures verified)
- `stage_acquire::run(pool, pool_secure, driver_id, request_id, command) -> WorkerProcessResult`
  — already owns lease CAS NEW→PROCESSING, tax-snapshot hoist, resume-preload,
  PREPARED INSERT. Returns `Proceed(WorkerContext)` / `Resumed(WorkerContext)` /
  `Noop` / `Rejected{reason}` (`types.rs:248-277`). **This is the A2 entry stage;
  A2 does NOT re-implement leasing.**
- `stage_sign::run(pool, ctx: &SigningContext, incoming: WorkerContext) -> Result<SigningOutcome, SignError>` (`stage_sign.rs:193`)
- `dispatch_post_sign(pool, doc_id, fiscal_number) -> anyhow::Result<PostSignRoute>` (`dispatch.rs:126`)
  → `Online{mode}` (caller continues to send) | `Offline{outcome}` (pipeline
  terminates, W7a already ran) | `Refused(reason)` (`dispatch.rs:71-87`).
- `stage_send::run(pool, dps_channel, doc, sign_ctx) -> Result<StageSendOutcome, StageSendError>` (`stage_send.rs:936`)
  — internal MAC-recovery loop; drives to SENT/KVT1.
- `stage_finalize::run(pool, doc) -> Result<StageFinalizeOutcome, StageFinalizeError>` (`stage_finalize.rs:234`)
  — CAS Kvt2→Ack + chain-continuity guard (`stage_finalize.rs:286`), inside one
  `with_immediate`. Calls `fd::fetch_finalize_inputs_tx` (`fiscal_documents.rs:1372`).

### Shift transition service (C1, MERGED PR #116)
- `services/shift/transition.rs:58` `apply_shift_transition(tx, fiscal_number, shift_id, from, to)` — atomic dual-write shifts⇔node_state projection.
- **`transition.rs` doc (lines 40-57) explicitly defers**: "Intent edges that
  OPEN a *new* shift and SET `current_shift_id` for the first time are added
  when `stage_acquire` is wired (RS-3 A-pieces)." → **this is the A2 C1-hook
  decision (§2 decision b).** Normal-close `current_shift_id` clear is also an
  "RS-3 A-piece decision."

### W12 drain tail (reuse target)
- `services/offline_sync/kvt2_confirm.rs:528` `pub(in crate::services::offline_sync) async fn confirm_drain_doc(...)` — **visibility-restricted to `offline_sync`.** Inline A2 lives in `services/write_path` → cannot call it as-is. (verify at impl: exact module path of the new orchestrator.)
- `backlog_drain.rs:1224/1695/1799` are the only callers today.

---

## 3. Proposed minimal change

Add ONE new orchestrator module `services/write_path/inline.rs` exposing
`pub async fn run(...) -> Result<FiscalOutcome, FiscalError>`, and a thin
production `WritePathEntry` impl (`InlineWritePath`) in `runtime/ingress` that
builds the `CanonicalFiscalCommand` via A1's `build_canonical` and delegates to
`inline::run`. No stage is modified except the C1-finalize hook (decision b),
which is added behind a shared seam reused by boot/drain.

The orchestrator is a **flat match ladder**, not a new abstraction:

```
fiscalize(row):                          # whole future under App::acquire_fn_gate (A4)
  if is_z_class(row): ensure_full_z_surface_ready()?   # decision d: 501 ZSurfaceNotReady (after lease+terminalise)
  cmd = build_canonical(row)            # A1 (non-Z) / build_z_canonical after quiesce+aggregate_for_shift (A1Z)
                                        #   BuildReject → terminalise + map error
  acq = stage_acquire::run(... cmd)     # lease NEW→PROCESSING + PREPARED + ACQUIRE-TIME SHIFT-LINK (decision b):
                                        #   SHIFT_OPEN → create/link shift + current_shift_id + open_document_id
                                        #   Z/SHIFT_CLOSE → link z_report_document_id
  match acq:
    Noop                → replay-resolve via ledger (decision e): build_accepted / 202 / breach→INTERNAL; never NotImplemented
    Rejected{reason}    → already inbox=REJECTED+audited → map to typed FiscalError
    Proceed(ctx)|Resumed(ctx):
      sign = stage_sign::run(... ctx)   # SignError → SignFailure (+terminalise inbox)
      route = dispatch_post_sign(... doc, fn)
      match route:
        Refused(r)      → map to FiscalError (Blocked/Stop/Crypto/GoingOnline) + terminalise
        Offline{outcome}→ build OfflineLocalAck FiscalOutcome (terminal)  [W7a already ran]
        Online{..}:
          send = stage_send::run(... doc, sign_ctx)            # → Sent | Rejected→DpsRejected/422 | ErrorRetryable→202
          if Sent:
            confirm = kvt2_confirm_core(... doc)               # A2.1a runtime-neutral: Sent→Kvt1→Kvt2
            if reached Kvt2:
              finalize_with_shift(... doc, ShiftEdge::CLOSE?)  # decision b: Kvt2→Ack + shift CLOSE in ONE with_immediate
              build Ack FiscalOutcome
            else (inline window timeout) → Ok(FiscalOutcome{document_state: Sent}) → 202; drain/B1 finishes
```

Every arm MUST satisfy the inbox-lifecycle obligation (`seam.rs:162-173`):
terminalise the inbox row on every real-failure return.

---

## 4. Decomposition (ordered, mergeable sub-pieces)

| # | Name | Lands | Review checkpoint | Mergeable |
|---|------|-------|-------------------|-----------|
| **A2.0** | Outcome/error mapping table **+ Z-gate + Noop mapping** | A pure `map_*` module: `WorkerProcessResult`→{continue, **Noop=replay-resolve (decision e)**, FiscalError, terminal}, `SignError`→`SignFailure`, `StageSendError`→{`DpsRejected`,offline,...} branching on `decision.target_state` (`Sent`→continue, `Rejected`→`DpsRejected`/422, `ErrorRetryable`→IN_PROGRESS/202, preserve `node_mode_flip`), `DispatcherRefusalReason`→FiscalError, `BuildReject`→terminalise+error. **+ new `FiscalError::ZSurfaceNotReady`→`Z_SURFACE_NOT_READY`→501 (decision d)** in `seam.rs` + `handler.rs` http map. Table-driven unit tests only. | self-review (pure; the `seam.rs`/`handler.rs` taxonomy add is the one hot-zone touch → light mid-review) | **independent** |
| **A2.1a** | Runtime-neutral KVT2 confirmer (decision a) | Extract the `Sent→Kvt1→Kvt2` core from `kvt2_confirm` (`:600`) into a confirmer that yields `FiscalError`/IN_PROGRESS WITHOUT `BootError`/drain source-routing. The existing drain wrapper + its tests stay green (it calls the new core). | **mid-review** (offline_sync refactor; drain tests must stay green) | must precede A2.1b |
| **A2.1b** | Online-happy orchestrator | `inline::run` chaining acquire→sign→dispatch(Online)→send→**KVT2-confirm (A2.1a)**→finalize, returning `FiscalOutcome::Ack` / `Sent`(202 on inline-window timeout). Offline/Refused/error arms typed-guarded (NOT silent). Wires A2.0. | **mid-review** (first hot-zone chain; tx-boundary §7 audit) | must-land w/ A2.2 before binding |
| **A2.2** | `stage_acquire` shift-link **+** C1-finalize hook (decision b) | (1) acquire-time: create/link shift + `current_shift_id` + `open_document_id` for SHIFT_OPEN; link `z_report_document_id` for Z/SHIFT_CLOSE. (2) `stage_finalize::run_with_shift_transition(.., Option<ShiftEdge>)` applying the CLOSE transition atomically with `Kvt2→Ack`. Shared `apply_shift_transition`, reused by inline-A2 AND boot/drain. | **mid-review** (C1 invariant + edges 1/8 @acquire, 3/10 @finalize; the acquire-link is the higher-risk half) | must-land w/ A2.1b |
| **A2.3** | Offline-ack + Refused arms | Wire `PostSignRoute::Offline` → terminal `OfflineLocalAck` outcome; `Refused` → typed FiscalError + inbox terminalise. | mid-review (offline-ack lifecycle) | independent of A2.4 |
| **A2.4** | Production binding + inbox-terminalise audit | Replace `UnimplementedWritePath` with `InlineWritePath` in the DI root; every real-failure arm drives inbox non-`NEW`+audited (the four-variant gate test is a HARD merge gate). **Flip-the-switch piece.** | **mandatory external review** (binding + replay-forever risk) | **must land last** |
| **A2.5** | Resume-path coverage | Exercise `WorkerProcessResult::Resumed` (boot-crash re-drive) through the full chain; assert no double-lnd, no re-INSERT. | mid-review | independent (additive tests + any resume-specific dispatch) |

Suggested PR grouping: **A2.0 alone** (pure + the small taxonomy add); **A2.1a
alone** (offline_sync refactor, isolated so drain tests gate it); **A2.1b+A2.2
bundled** (chain + shift-link/finalize are coupled by the tx-boundary); A2.3
alone; **A2.4 alone** (highest blast radius); A2.5 alone. Per memory
`feedback_multi_round_external_review_pattern`, A2.1b+A2.2 and A2.4 each need 3-5
fresh-eyes rounds; A2.1a needs ≥1 (it touches the W12 drain module).

---

## 5. Design decisions needing operator sign-off

### (a) KVT2-confirm re-expose — widen `confirm_drain_doc` vs extract a thin confirmer

`confirm_drain_doc` (`kvt2_confirm.rs:528`) is `pub(in crate::services::offline_sync)`.
Inline A2's online ladder drives KVT1→KVT2→Ack and needs the same confirm
semantics. BUT — the online ladder's tail is `stage_finalize::run` (already
the canonical KVT2→Ack CAS), whereas `confirm_drain_doc` is the *drain*-specific
wrapper (Envelope 1a/1b/2, SentFresh/Replay/Kvt1Reentry source-routing).

- **Option A — widen visibility** of `confirm_drain_doc` to `pub(crate)` and
  call it from inline. Trade-off: pulls drain-specific source-routing
  (SentReplay/Kvt1Reentry) into the online path where those sources are
  impossible → dead branches + a scope guard that must be re-reasoned. Couples
  online ladder to drain module's churn.
- **Option B (recommended)** — **inline A2 does NOT call `confirm_drain_doc` at
  all.** The online ladder already terminates at `stage_finalize::run`
  (`stage_finalize.rs:234`) which IS the KVT2→Ack CAS. A2 reuses `stage_finalize`,
  not the drain wrapper. `confirm_drain_doc` stays `offline_sync`-private.
  Trade-off: requires confirming `stage_finalize::run` alone produces a complete
  Ack `FiscalOutcome` for the online case (it does — CAS + chain guard + seed
  advance). **No visibility change needed.** Smallest seam.
- ~~Recommendation: B.~~ **OVERRIDDEN by rev2 §0(a) → choose A1 (extract a
  runtime-neutral confirmer).** Option B was based on a wrong premise:
  `stage_finalize::run` REQUIRES `Kvt2` (`stage_finalize.rs:240`) but the online
  ladder only reaches `Sent` (`stage_send.rs:490`); the `Sent→Kvt1→Kvt2` step
  exists ONLY in `kvt2_confirm` (`:600`). Reusing `stage_finalize` alone would
  strand every online receipt at `Sent` (eternal 202, never inline ACK). So A2
  MUST drive KVT2-confirm — via an extracted runtime-neutral core (no
  `BootError`, no drain source-routing), NOT the private drain wrapper. See §0(a).

### (b) C1-atomic-finalize hook shape — `stage_finalize::run_with_shift_transition` vs KVT2-stop-seam + C1-finalizer

The shift OPEN edge (SHIFT_OPEN: `Opening→Opened`, edges 1/8) and CLOSE edge
(Z_REPORT: `Closing→Closed`, edges 3/10) must apply the shift transition
**atomically with the doc's terminal state advance** — `apply_shift_transition`
(`transition.rs:58`) requires a `WriteTxConn`, so it MUST run inside the same
`with_immediate` as the doc CAS. `transition.rs:40` explicitly defers these
edges to "the A-pieces." Boot/drain also close shifts and need the same hook.

- **Option A — `stage_finalize::run_with_shift_transition(pool, doc, Option<ShiftEdge>)`**:
  extend `stage_finalize::run` to optionally take a shift edge and call
  `apply_shift_transition` inside its existing `with_immediate` (`stage_finalize.rs:238`).
  Trade-off: smallest diff (one extra closure body inside the existing envelope,
  preserving the IO-free §6 boundary); but mutates a hot-zone stage signature →
  every existing `stage_finalize::run` caller (boot/drain) must pass `None` or
  the edge. Clean atomicity.
- **Option B — KVT2-stop-seam + separate C1-finalizer**: stop `stage_finalize`
  at KVT2, run a separate `finalize_with_shift` orchestration step. Trade-off:
  splits the KVT2→Ack CAS from the shift transition across two envelopes →
  **breaks atomicity** (crash between → Ack'd doc with un-transitioned shift →
  exactly the manual-recon surface §16.7 trigger family 1). Rejected on
  invariant grounds (#8 recovery must not violate transitions).
- Recommendation: **A**, with the edge computed by the orchestrator (online or
  boot/drain) and passed in. Keeps the shift transition in the same envelope as
  the terminal CAS. The `Option<ShiftEdge>` is `None` for non-shift docs (SELL),
  `Some(Open)` for SHIFT_OPEN, `Some(Close)` for Z_REPORT/SHIFT_CLOSE. Boot/drain
  pass the same enum → one shared atomic finalizer. **(verify at impl: whether
  SHIFT_OPEN's edge fires at finalize or earlier at stage_acquire — open may need
  to land at acquire-time `Opened` projection, not at the doc's KVT2; if so the
  hook is OPEN-at-acquire + CLOSE-at-finalize, two callsites, same helper.)**

### (c) `fetch_finalize_inputs_tx` extension scope

`stage_finalize::run` reads `fd::fetch_finalize_inputs_tx` (`fiscal_documents.rs:1372`)
post-CAS for chain-continuity. The C1 hook needs the doc's `shift_id` +
`from`/`to` shift states inside the same tx to call `apply_shift_transition`.

- **Option A — extend `fetch_finalize_inputs_tx`** to also return
  `shift_id` + current `shift_state` (join node_state/shifts). Trade-off: one
  query, one tx read; but widens a shared struct used by every finalize caller.
- **Option B (recommended)** — **do NOT extend it.** The orchestrator already
  knows `shift_id` from `WorkerContext` (acquire stage resolved it) and the
  edge is determined by `row.operation_type`. Pass `Option<ShiftEdge { shift_id,
  from, to }>` into the hook explicitly; `apply_shift_transition` does its own
  CAS with `from`/`to` (it reads current state via `shifts::transition_state`).
  No new read inside the finalize tx beyond what `apply_shift_transition` already
  does. Trade-off: orchestrator must compute the edge correctly (covered by
  decision b's enum). Smallest diff; no shared-struct churn.
- Recommendation: **B.** Keep `fetch_finalize_inputs_tx` untouched; thread the
  shift edge as an explicit parameter. (verify at impl: `apply_shift_transition`
  takes explicit `from`/`to` — `transition.rs:58` confirms it does, so the
  orchestrator supplies them.)

---

## 6. Highest-risk seams + pinning test

| Seam | Risk | Pinning test |
|------|------|--------------|
| **Inbox-lifecycle obligation** (`seam.rs:162-173`) | A real-failure arm returns with inbox row still `NEW` → replay resolves `202 IN_PROGRESS` **forever**. | For EACH of `ShiftNotOpen`/`SignFailure`/`DpsRejected`/`OfflineRefused`: after `fiscalize` returns Err, assert inbox status is non-`NEW` (DONE/REJECTED) AND an audit row exists. (The seam doc itself mandates "An A2 gate test MUST assert this for every real-failure path.") |
| **C1 atomic finalize** (decision b) | Crash between KVT2→Ack CAS and shift transition → Ack'd doc + un-transitioned shift = manual-recon trigger family 1 (§16.7). | Kill-point test: doc reaches KVT2, inject failure at `apply_shift_transition`; assert the whole `with_immediate` rolls back (doc stays KVT2, shift unchanged) — proves single-envelope atomicity. |
| **Lease not double-acquired** (invariant #2) | Orchestrator re-implements leasing instead of reusing `stage_acquire::run` → two writers per FN. | Concurrent `fiscalize` for same FN/distinct receipts: assert exactly one `Proceed`, others serialize (lease CAS in stage_acquire is the only seam). NON-IDENTITY fixtures (per memory `feedback_type_system_reaching_fn_not_using`). |
| **Offline branch terminates** (`dispatch.rs:11`) | Orchestrator continues to `stage_send` after `PostSignRoute::Offline` → double fiscalisation. | Force node Offline; assert `stage_send::run` is NOT called (spy) and outcome is `OfflineLocalAck`. |
| **Z/SHIFT_CLOSE wire-intent not signer-fed** (`seam.rs:158-160`) | Z payload is WIRE intent (aggregate behind drain); feeding it to `stage_sign` as signer-ready corrupts the chain. | Z_REPORT row through `fiscalize`: assert it routes to aggregate/drain-barrier handling, NOT direct stage_sign of the wire intent. (A1Z dual-hash already gates; A2 must respect it.) |
| **dispatch_post_sign fresh node read** (`dispatch.rs:126-148`) | Orchestrator passes stale `WorkerContext.node_state` instead of letting dispatcher re-read → wrong route. | Flip node mode between sign and dispatch; assert route follows the FRESH mode (dispatcher's own read wins). |

---

## 7. tx-boundary map (proves invariant #1: no network/crypto inside write tx)

Gate = `with_immediate` (BEGIN IMMEDIATE) per FN. Every network/crypto op sits
OUTSIDE every `with_immediate`.

| Step | Inside `with_immediate` (short, IO-free) | Outside (network/crypto/pool-read) |
|------|------------------------------------------|------------------------------------|
| build_canonical (A1) | — | pure CPU (no IO) |
| stage_acquire | lease CAS NEW→PROCESSING + PREPARED INSERT (`stage_acquire.rs` main envelope) | tax-snapshot load from `pool_secure` (hoisted pre-tx, `stage_acquire.rs:62-72`); resume-preload (hoisted, `:84+`) |
| stage_sign | 3-PRE pin-or-reuse CAS (`stage_sign.rs:237` envelope) | **crypto signing** — done OUTSIDE the pin envelope (3-PRE-READ pool read `:215`; sign call is between envelopes) |
| dispatch_post_sign | — | pool-bound node_state read (`dispatch.rs:131`, autocommit, NOT a write tx) |
| stage_offline_ack | acquire_code_tx + transition in ONE `with_immediate` (W7a) | none (I1-clean by design, `dispatch.rs:46-49`) |
| stage_send | per-attempt CAS to SENT/KVT1 | **DPS network call** + MAC-recovery re-sign — OUTSIDE the CAS envelope (`stage_send.rs` run-loop drives network between envelopes) |
| stage_finalize **+ C1 hook** | KVT2→Ack CAS + chain guard + **`apply_shift_transition`** — all in the SINGLE existing `with_immediate` (`stage_finalize.rs:238`) | none (finalize is DB-only; this is why decision b keeps the shift edge inside the SAME envelope rather than a second one) |

**Gate-held-across-OUTSIDE proof:** the two IO ops (crypto sign, DPS send) each
occur strictly *between* `with_immediate` envelopes — no envelope is open while
either runs. The C1 shift transition is the only NEW write A2 introduces inside
a tx, and it is co-located in the finalize envelope (decision b option A), adding
zero IO inside any tx. Invariant #1 preserved; invariant #8 (recovery/transition
atomicity) strengthened by co-locating shift CAS with doc CAS.

---

## 8. Invariants potentially affected

- **#1** (no net/crypto in write tx) — preserved; §7 map is the proof obligation
  for review.
- **#2** (one FN = single writer) — preserved by REUSING `stage_acquire`'s lease;
  the top risk is re-implementing it (§6 test pins).
- **#4** (idempotency) — Resume path (A2.5) + inbox idempotency key; replay must
  not double-fiscalise.
- **#8** (recovery/transition atomicity) — the C1 finalize hook is the live edge;
  decision b option A keeps it atomic.
- **#9** (graceful shutdown) — A2 must be cancellation-safe between stages (a
  shutdown between sign and send must leave a resumable doc, not a torn state);
  Resume-path A2.5 covers re-drive.

---

## 9. Verification plan (before coding)

1. Targeted suite during iteration (memory `feedback_cargo_test_scope`):
   `cargo test -p prro` for write_path + ingress modules; NOT full workspace.
2. Per-piece: the §6 pinning test must exist and pass before that piece merges.
3. A2.4 (binding flip): full `cargo test -p prro` ONCE pre-merge
   (memory `feedback_test_run_cadence`), plus the four-variant inbox-lifecycle
   gate test (§6 row 1) is a HARD merge gate.
4. NON-IDENTITY fixtures for any FN/shift_id threading
   (memory `feedback_type_system_reaching_fn_not_using`).
5. External review per memory `feedback_multi_round_external_review_pattern`
   on A2.1+A2.2 and A2.4 (local-path prompts, memory
   `feedback_external_reviewer_local_access`).

---

## 10. Rollback / containment

- The production binding is a ONE-LINE DI swap (`UnimplementedWritePath` →
  `InlineWritePath`). Rollback = revert that line; the inbox handler falls back
  to `NotImplemented` (fail-closed, no silent success — `seam.rs:182`). All A2.0–A2.3
  code is dormant until A2.4 flips the binding.
- Land A2.0–A2.3 behind the unflipped binding so the orchestrator is fully
  tested while production still returns `NotImplemented` — zero blast radius
  until the deliberate A2.4 flip.
- If the C1 finalize hook (A2.2) regresses boot/drain, revert just the
  `Option<ShiftEdge>` parameter (callers pass `None` = pre-A2 behavior).
