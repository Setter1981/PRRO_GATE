# RS-3 — Live write-path worker (fill the `WritePathEntry::fiscalize` seam)

**Status:** PLAN rev5 (post operator + 3 external plan-review rounds) · **Date:** 2026-06-08 · **Branch target:** `rust-gateway`
**Scope:** Core + WL-1 + live Z (operator-locked). **Predecessor:** RS-2 (merged, PRs #109–#113).

---

## 0. Scope

RS-2 froze the SEAM `runtime::ingress::seam::WritePathEntry::fiscalize(&InboxRow) -> Result<FiscalOutcome, FiscalError>` (prod binding `UnimplementedWritePath` → 501). **RS-3 implements the real `fiscalize`**: build the command from the self-contained `InboxRow` (021+022), serialize per-FN, drive `stage_acquire → sign → dispatch → send/offline-ack`, return a typed `FiscalOutcome`; recover crashed in-flight rows; wire the online shift-lifecycle (WL-1); and CLOSE the shift via a real aggregated Z.

**RS-3 is mostly WIRING.** All write-path primitives EXIST (stages, `dispatch_post_sign`, `acquire_lease`, `FiscalOutcome`, the shift-guards). The chain, the per-FN gate, the reaper, the shift transition-driver, and the **Z dual-payload model** are NET-NEW.

**Deferred (the ONLY follow-up):** **transient-Z-reject classification** — a transient DPS reject on a Z lands `terminal REJECTED` (not `ErrorRetryable`); a Z could strand. Needs the DPS reject-code captured + reclassified. (W4-Z3 piece-7.) **Z aggregation + quiescence ordering is NOT deferred** (Integration closes the shift via Z → it must be real).

---

## 1. Locked decisions (operator, 2026-06-08 + plan-review)

**D1 — A4 per-FN serialization = a per-FN ASYNC GATE in the runtime** (NOT a DB-lease-on-FN-row). `acquire_lease` serializes the inbox ROW; the gate serializes the whole live `fiscalize` of one FN WITHOUT holding a SQLite write-tx around crypto/DPS (#1). Crash-safety = B1 reaper, not lock-holding.
**D1a (deployment):** the in-process gate is sufficient because `App` holds a **singleton pid-lock per DB** (one `prro serve` per DB). Multi-instance over the SAME FN-set is UNSUPPORTED until a DB-level per-FN lease exists — document this boundary.

**D2 — B1 reaper = RE-DRIVE, boot + periodic, LEDGER-CHECK-FIRST** (NOT fail-out). Reaper takes the SAME per-FN gate, reads the persisted `InboxRow`, FIRST checks the terminal ledger BY `request_id` (already-terminal → resolve, don't re-drive), THEN re-drives only genuinely-stale leased rows.
**D2a (interval):** own `supervisor.ingress_reaper_interval_seconds` (NOT the drain/probe cadence). Default **5s**, clamp **1..60s**, `MissedTickBehavior::Skip`.

**D3 — C1 shift transition-service = ONE service, atomic dual-write (`shifts` PRIMARY + `node_state.shift_state`/`current_shift_id` PROJECTION).** Fires at (a) `stage_acquire` reservation/intent edges, (b) the finalize/ACK edge for the actual OPENED/CLOSED. **ZERO raw `node_state` shift writes survive** — this includes rerouting BOTH `boot_phase.rs:1502` AND the existing `offline_sync/backlog_drain.rs` atomic-shift-transition + node_state-mirror through the service (migrate, not exception).

**D4 — C2 active-shift uniqueness = CREATE the partial-unique index NOW (migration `023`), BEFORE C1.** The index does NOT exist in the crate migration set (`shifts` has only the non-unique `ix_shifts_fn_state`, `001:50` / rebuilt `016:196`; the `sql/001:158` partial-unique is the unused legacy Python tree). **Active-state set is LOCKED:** `CREATED, OPENING, OPENED_LOCAL_PENDING_DRAIN, OPENED, CLOSING_LOCAL_PENDING_DRAIN, CLOSING`. **Exclude:** `CLOSED, REQUIRES_MANUAL_RECONCILIATION, ERROR` (terminal / operator-action — dangerous in a unique index). **Backfill = FAIL-CLOSED, no auto-resolve** (a pre-existing duplicate-active DB fails the migration loud).

**D5 — Z dual-payload/dual-hash (resolves plan-review High-1).** The inbox holds the Z WIRE intent (hash over wire); the signer needs the aggregated `ZReportJson`. `stage_acquire` today cross-checks `command.payload_sha256_canonical == inbox.payload_sha256_canonical` AND writes `command.payload_json` to `fiscal_documents` (which `stage_sign` parses as `ZReportJson`). A single payload cannot satisfy both (wire fails the parse; aggregated fails the cross-check). **Model:** `CanonicalFiscalCommand` carries TWO hashes — `source_sha256` (the INGRESS/wire hash, = inbox hash always; used for the `stage_acquire` drift cross-check) and the **persisted/signed** `payload_json` + its own `payload_sha256` (= source for non-Z, = aggregated for Z). `stage_acquire` cross-checks `command.source_sha256 == inbox.payload_sha256_canonical`, persists `command.payload_json` + `hash(payload_json)` as the doc hash. (A1Z owns this.)

---

## 2. Dependency DAG / build order

```
C2 (migr 023: uq active-shift index + fail-closed backfill + chokepoint current_shift_id)   [foundation]
  └─> C1 (shift transition-service; reroute boot_phase:1502 + backlog_drain through it)
        └─> A1 (InboxRow->CanonicalFiscalCommand builder; non-Z; null-guard TERMINALIZES inbox)
              └─> A1Z (Z dual-payload/dual-hash + aggregate + quiescence + stage_acquire cross-check adapt)
                    └─> A3 (handler exhaustive FiscalError->HTTP + offline-success scaffold)
                          └─> A4 (per-FN runtime gate infra)
                                └─> A2 (live WritePathEntry worker, BOUND ONLY BEHIND the A4 gate)
                                      └─> B1 (stale-PROCESSING reaper; takes A4 gate, ledger-check-first)
                                            └─> Integration / acceptance (end-to-end incl. Z close)
```

**Build order:** `C2 → C1 → A1 → A1Z → A3 → A4 → A2 → B1 → Integration`.
**Critical (plan-review High-2):** the live worker (A2) is NEVER bound to a listener until the A4 gate exists — no intermediate mergeable piece binds a live `fiscalize` without per-FN serialization (#2). A4 lands BEFORE A2 binds; A2 wires the worker strictly inside the gate.

---

## 3. Pieces

### C2 — active-shift uniqueness migration + chokepoint (FOUNDATION)
- **Files:** `rust/prro/migrations/023_*.sql` (NET-NEW); `services/reconciliation/boot_phase.rs:1502`.
- **Work:** migration `023`: `CREATE UNIQUE INDEX uq_active_shift_per_fiscal ON shifts(fiscal_number) WHERE state IN ('CREATED','OPENING','OPENED_LOCAL_PENDING_DRAIN','OPENED','CLOSING_LOCAL_PENDING_DRAIN','CLOSING')` (the D4 set — verify the exact enum wire strings vs the M3b 9-state machine). **Fail-closed backfill:** the migration detects pre-existing duplicate active shifts and FAILS loud (no auto-resolve). Fix `boot_phase.rs:1502` to also clear `current_shift_id` on CLOSE.
- **Tests:** clean apply; second active-shift insert for a FN rejected; chokepoint clears current_shift_id; seeded-duplicate DB fails the migration loudly.
- **Invariants:** #8; state-machine (the active set matches the 9-state spec exactly).
- **Review-gate:** migration-keeper + index-predicate vs 9-state machine.

### C1 — shift transition-service (single API, atomic dual-write)
- **Files:** NET-NEW `services/shift/transition.rs`; reroute `boot_phase.rs:1502` + `offline_sync/backlog_drain.rs` (its atomic shift-transition + node_state mirror) through it; `stage_acquire.rs:383/396/402/414` stays a read-projection.
- **Work:** typed `apply_shift_transition(tx, fn, edge)` covering intent edges (open/close reservation) + ACK edges (OPENED/CLOSED) per the 9-state machine; atomic dual-write (`shifts` primary + `node_state` projection + `current_shift_id`) in ONE `with_immediate`; no IO inside (#1). **Migrate `backlog_drain` + `boot_phase` raw writes into the service** (plan-review High-3). A grep-guard/test that NO raw `node_state` shift write survives outside the service.
- **stage_acquire integration (plan-review High-2 — the shift↔document link):** today `stage_acquire` attaches an active shift only for `OPENED/OPENED_LOCAL_PENDING_DRAIN`, so a `SHIFT_OPEN` from a CLOSED state would mint a `fiscal_documents` row with `shift_id = NULL` and the transition-service would have no authoritative `shift_id`/`open_document_id` to flip the right shift to OPENED at ACK. C1 MUST specify the integration: on `SHIFT_OPEN` at `stage_acquire` (intent edge) → **create/transition the shift, persist `current_shift_id`, set `NewDocument.shift_id`, and link `shifts.open_document_id` to the minted SHIFT_OPEN doc**; on `Z_REPORT`/`SHIFT_CLOSE` → link **`shifts.z_report_document_id`** to the minted Z doc (rev4-review High — the M3b spec §187 + the existing close seam `shifts.rs:945` use `z_report_document_id` as the AUTHORITATIVE Z-close link; recovery/drain predicates read it. Do NOT use `close_document_id` unless a distinct close-document semantics is explicitly defined + the recovery predicates updated); at the ACK edge → flip OPENED/CLOSED on that linked shift. Every fiscal doc carries a non-NULL `shift_id`. **The ACK-edge transition is invoked ATOMICALLY inside the `stage_finalize` envelope via the shared C1-aware finalize hook (see A2, rev3-review High-1) — NEVER as a separate `node_state` write after finalize commits (that would orphan the shift on a crash-between).** The hook is the SAME for the inline A2 path and the boot/replay/drain finalize paths.
- **Tests:** each edge (dual-write asserted on both tables); read-projection == `shifts.state` after each edge; concurrent-open rejected (via C2); backlog_drain + boot_phase route through the service.
- **Invariants:** #1, #2 (C2), #8.
- **Review-gate:** arch + state-machine review (WL-1 core) — dual-write biconditional + no-bypass (incl. backlog_drain migrated).

### A1 — InboxRow → CanonicalFiscalCommand builder (non-Z) + terminalizing null-guard
- **Files:** NET-NEW builder module; `services/write_path/types.rs` (`CanonicalFiscalCommand` gains `source_sha256: [u8;32]` per D5 — RESOLVED as a new struct FIELD, not threaded separately). **D5 caller ripple (compile-forced — list + update ALL):** the struct-literal callers `runtime/ingress/dto.rs`, `services/reconciliation/boot_phase.rs`, and tests `tests/{outgress_trait_quartet, write_path_stage1_acquire, write_path_stage3_sign}.rs` — each sets `source_sha256 = payload_sha256_canonical` (non-Z: the two hashes COINCIDE; only the A1Z Z path makes them diverge).
- **Work:** map `InboxRow → CanonicalFiscalCommand` for non-Z (operation_type→doc_type, payload_json, business_ts, total_sum_kop, cashier, driver; `source_sha256 = inbox.payload_sha256_canonical`, `payload_sha256 = hash(payload_json)` — coincide for non-Z). **Null-guard (seam null-contract):** driver_id + business_ts REQUIRED; total_sum_kop REQUIRED for SELL/RETURN. **The guard runs AFTER `acquire_lease` (the row is PROCESSING), so a reject MUST terminalize the inbox: `mark_rejected_tx` + audit, NO `fiscal_documents` row** (plan-review Medium) — else the reaper loops a legacy/malformed row forever.
- **Tests:** builder per non-Z doc-type (NON-IDENTITY fixtures); each reject (missing driver_id / business_ts / SELL-no-total) → `mark_rejected_tx` + audit + status REJECTED (reaper-safe) + no fiscal doc.
- **Invariants:** #6, #4 (source hash), #8, the null-contract.
- **Review-gate:** guard completeness + the terminalize-on-reject.

### A1Z — Z dual-payload/dual-hash + aggregate + quiescence + stage_acquire adapt (resolves High-1)
- **Files:** the builder (Z branch); `runtime/ingress/convert.rs::aggregate_zreport` (EXISTS); `services/write_path/stage_acquire.rs` (the cross-check adaptation per D5); `types.rs` (`source_sha256`).
- **Work:** for `Z_REPORT`/`SHIFT_CLOSE`: (1) **quiescence — GATE-ALREADY-HELD (plan-review Medium):** finalize/drain pending shift docs so the ledger is complete BEFORE aggregating (NOT deferred), but run it INLINE inside the ALREADY-HELD per-FN fiscalize gate (A4) with its own SHORT DB transactions — it MUST NOT call the drain loop / B1 reaper (which would try to RE-ACQUIRE the same per-FN gate → deadlock). No recursive gate acquisition; (2) `aggregate_zreport(ledger for current_shift_id)` → signer-ready `ZReportJson`; (3) build the command with `source_sha256 = inbox.payload_sha256_canonical` (wire), `payload_json = aggregated`, `payload_sha256 = hash(aggregated)`; (4) **adapt `stage_acquire` cross-check** to `command.source_sha256 == inbox.payload_sha256_canonical` (the wire hash) while persisting the aggregated payload + its hash to the doc.
- **Tests:** a Z with seeded shift ledger → aggregated ZReportJson parses through stage_sign; the source-hash cross-check passes against the wire inbox hash; the doc persists the aggregated payload + `hash(aggregated)`; quiescence: a pending in-flight shift doc is finalized before the Z aggregates (no undercount); non-Z still uses the coincident hash.
- **Invariants:** #4 (the dual-hash drift check stays meaningful), #6, #8 (quiescence ordering), the persistence pin.
- **Review-gate:** MANDATORY — the dual-hash model + the stage_acquire change + the quiescence ordering (the highest-subtlety seam after A2).

### A3 — handler exhaustive FiscalError → HTTP + offline-success scaffold
- **Files:** `runtime/ingress/handler.rs` (the RS-2 exhaustive `match fe` + `http_status_for_error_code`; **change `handler.rs:242` — Ok(FiscalOutcome) is no longer always-200, it branches on `document_state`**); `runtime/ingress/seam.rs` (the `FiscalOutcome.document_state` doc — extend from terminal-only to allow a non-terminal in-flight state for the 202 path).
- **Work:** add `FiscalError::{ShiftNotOpen, SignFailure, DpsRejected, OfflineRefused}` (each carries `request_id`); map `DpsRejected→422 FISCAL_REJECTED` (TERMINAL reject only), `ShiftNotOpen→422`, `SignFailure→500`, `OfflineRefused→…`. The delete-on-`NotImplemented` path must NOT fire for a real fiscal failure (a `DpsRejected` PERSISTS, the exhaustive match forces it). **TERMINAL-vs-RETRYABLE split (rev3-review High-2):** a transient/`ErrorRetryable` send outcome is NOT a `DpsRejected` and NOT an `Err` — `fiscalize` returns `Ok(FiscalOutcome{document_state: <non-terminal: Sending/ErrorRetryable/Kvt1>})` and the handler maps a NON-TERMINAL document_state → **202 IN_PROGRESS** (with document_id), the drain/B1 carries it to terminal. So the handler outcome map is: `Ack`→200, `OfflineLocalAck`→200 (null fiscal_id), non-terminal→202 IN_PROGRESS, `Err(FiscalError)`→4xx/5xx. (Scaffolded BEFORE A2 wires the variants — error taxonomy first.)
- **Tests:** each new FiscalError → its HTTP + the inbox row NOT released (persists); offline → 200 null.
- **Invariants:** persistence pin (failed DPS → fiscal doc, NOT deleted), #4.
- **Review-gate:** delete-vs-persist per variant; the 202/IN_PROGRESS contract.

### A4 — per-FN runtime serialization gate
- **Files:** the gate in `App`/supervisor (per-FN `tokio::Mutex` keyed; e.g. `DashMap<Fn, Arc<Mutex<()>>>`); wraps A2 + B1.
- **Work (plan-review Medium):** the per-FN async gate is **held across the WHOLE per-FN `fiscalize` future — INCLUDING the DPS send + KVT2-confirm waits** (that IS the serialization: no second receipt of the same FN runs the write-path concurrently). The two-level distinction: the GATE spans the full fiscalize (a `tokio::Mutex`, not a DB lock); each `with_immediate` write-tx INSIDE it stays short-lived and holds NO network/crypto (#1). In-process (D1a: singleton pid-lock per DB makes this sufficient; multi-instance-per-FN UNSUPPORTED until a DB-level per-FN lease exists).
- **Tests:** same-FN concurrent fiscalize BLOCKS during a fake slow DPS call (the 2nd waits for the 1st's full fiscalize incl. the DPS wait); different FNs run concurrently (no cross-FN block); the gate ≠ db-tx (the static `with_immediate_no_foreign_io` guard still passes — no IO inside any tx); shutdown releases the gate.
- **Invariants:** #2, #1, #9.
- **Review-gate:** gate granularity + no-tx-span; B1 shares it.

### A2 — live `WritePathEntry` worker (BOUND ONLY BEHIND the A4 gate)
- **Files:** NET-NEW `LiveWritePath` (impl `WritePathEntry`); `seam.rs` (FiscalError variants from A3); `supervisor.rs` binds it for the listeners — INSIDE the A4 gate. **+ a re-exposed KVT2-confirm seam** (see below).
- **Work:** `fiscalize(&InboxRow)`: A4 gate → `acquire_lease` → A1/A1Z builder → `stage_acquire::run` → `stage_sign` → `dispatch_post_sign` → **Online: `stage_send` → (branch on the send outcome, below) → KVT2/lastChk confirm (SENT→KVT1→KVT2) → C1-aware finalize (KVT2→ACK + atomic shift transition)** / Offline: terminate at `OfflineLocalAck` / Refused: `Err`. Build `FiscalOutcome` (reuse `replay::build_accepted` projection). **Auto-offline = `Ok(OfflineLocalAck)`, NEVER `Err`** (seam.rs:96).
- **KVT2-confirm re-expose (rev3 High-1 + rev3-review Medium — NET-NEW, RUNTIME-NEUTRAL seam):** `stage_send::run` only advances `Sending → {Sent|Rejected|ErrorRetryable}`; `stage_finalize::run` requires `Kvt2`. The `SENT→KVT1→KVT2` quittance step lives in `services/offline_sync/kvt2_confirm.rs::confirm_drain_doc` (`pub(in crate::services::offline_sync)`, and it returns `Result<ConfirmDrainOutcome, BootError>` — boot/drain policy language). RS-3 must **extract a LOWER-LEVEL, runtime-neutral confirmer** (or wrap it) that yields `FiscalError`/`IN_PROGRESS` outcomes WITHOUT leaking boot-fatal `BootError` semantics into inline ingress. Call it inline after `stage_send` (inline-synchronous DPS poll; W4-Z3 live-proved prompt return). If KVT2 is not reached in the inline window → leave the doc `SENT`, return `IN_PROGRESS` (202), drain/B1 picks it up — NOT a hang.
- **Send-outcome routing (rev3-review High-2 — `StageSendOutcome::Routed` is NOT always a terminal reject):** branch on `decision.target_state` (`stage_send.rs` / `error_routing.rs:278`): `Sent` → proceed to KVT2-confirm; `Rejected` (terminal DPS reject) → `Err(FiscalError::DpsRejected)` → 422; **`ErrorRetryable` (transient transport/server/decode) → `IN_PROGRESS` (202), NOT a terminal failure** (the ledger retries via the drain — the client must not see "rejected" for a retryable doc). **PRESERVE the explicit `node_mode_flip` the routing computes (rev4-review Medium): `DpsError::Transport` → `ErrorRetryable` with `node_mode_flip: None` (stays 202 — NOT auto-offline; e.g. server `-11` → `Blocked`). Do NOT invent an auto-offline-on-transport policy here** — that is a separate, reviewed change.
- **IN_PROGRESS response shape (rev4-review Medium — LOCK):** `CanonicalErrorResponse` (`dto.rs:139`) has NO `document_id`/`retry_class` field, and `handle_command` (`handler.rs:242`) currently maps EVERY `Ok(FiscalOutcome)` → HTTP 200. LOCK: the 202 IN_PROGRESS is **request-id + `error_code: IN_PROGRESS` ONLY** (no new envelope fields — the client polls via `GET /v1/status/:fn`); A3 changes the handler to map a NON-TERMINAL `FiscalOutcome.document_state` → 202 (it is no longer always-200); update `seam.rs` `FiscalOutcome.document_state` doc (it currently documents TERMINAL states only — it may now carry a non-terminal in-flight state).
- **C1-aware ATOMIC finalize (rev3-review High-1 — the ACK-edge transition must be in the finalize envelope):** `stage_finalize::run` does only `Kvt2→Ack` + chain-seed + inbox DONE + outbox + audit — it does NOT touch shift state. For a SHIFT_OPEN/Z doc, the C1 shift transition (Opening→Opened / Closing→Closed) MUST commit ATOMICALLY with the doc's `Kvt2→Ack` (same `with_immediate`), else a crash between them orphans the shift (boot sees `shift_state=Opening` with no pending doc → orphan ERROR recovery). Define a finalize HOOK — `stage_finalize::run_with_shift_transition(...)` (or a KVT2 seam that stops at Kvt2 + a C1-aware finalizer A2 calls). **BOTH the inline A2 path AND the boot/replay/drain KVT2 paths (`confirm_drain_doc`→finalize) MUST use this same hook for shift docs**, so a shift doc finalized via any path fires C1. **FinalizeInputs extension (rev4-review Medium):** `fetch_finalize_inputs_tx` (`fiscal_documents.rs:1281`) intentionally OMITS `doc_type`, `shift_id`, `signed_by_cashier_id` — the hook cannot decide "is this SHIFT_OPEN/Z?" or WHICH shift to transition without them. Extend `fetch_finalize_inputs_tx` (or add a shift-aware fetch INSIDE the same finalize tx) to carry at least `doc_type` + `shift_id` (+ the close signer/cashier identity if `closed_by_cashier_id` is populated).
- **Tests:** full inline per outcome (Online ACK via send→KVT2-confirm→atomic-C1-finalize, Offline, each Refused, sign-fail, terminal DPS-reject→422, transient→IN_PROGRESS/202, KVT2-not-immediate→IN_PROGRESS); crash between doc-ACK and shift-transition is IMPOSSIBLE (same tx) — assert the atomic finalize on a shift doc flips shift_state in the same envelope; assert NO net/crypto inside any `with_immediate` (extend `with_immediate_no_foreign_io`); shutdown mid-fiscalize crash-safe.
- **Invariants:** #1 (CRITICAL), #2 (gate), #4, #8, #9, offline-as-Ok.
- **Review-gate:** MANDATORY multi-round (highest-risk) — chain ordering, tx boundaries, FiscalError/offline semantics, first-pass↔replay parity, the binding-behind-gate.

### B1 — stale-PROCESSING reaper
- **Files:** NET-NEW reaper (boot + a periodic tick in the supervisor); reuses A1/A1Z/A2/A4; `config` (`ingress_reaper_interval_seconds`).
- **Work (D2/D2a):** find `PROCESSING` rows; per row take the A4 gate, read the `InboxRow`, FIRST check terminal ledger BY `request_id` (terminal → resolve, no re-drive), else re-drive via A2. Boot pass + periodic tick (`ingress_reaper_interval_seconds` default 5s, clamp 1..60s, `MissedTickBehavior::Skip`). Idempotent.
- **Tests:** crash-mid-lease → re-drives to terminal; crash-after-terminal → resolves WITHOUT re-drive (no double-fiscalize); idempotent re-run; shutdown-safe; A1-rejected (terminalized) rows are NOT picked up (they're REJECTED, not PROCESSING).
- **Invariants:** #8, #4 (no double-fiscalize), #2, #9.
- **Review-gate:** ledger-check-first ordering + no-double-fiscalize proof.

### Integration / acceptance — end-to-end
- **Tests (`--features test-support`, mock DPS):** (1) SHIFT_OPEN→ACK opens shift (WL-1) then SELL→Online ACK; (2) Offline SELL → 200 OfflineLocalAck; (3) replay parity; (4) graceful shutdown mid-fiscalize; (5) **Z_REPORT → aggregate (dual-hash) + close shift** (the High-1 path end-to-end); (6) crash-recovery via reaper (re-drive + resolve-terminal); (7) per-FN serialization under concurrent load; (8) SELL refused on CLOSED shift (WL-1 negative). NON-IDENTITY fixtures.
- **Review-gate:** full external multi-round (convergence = 2× MERGE-only-Info).

---

## 4. Review cadence
Hot-zone, 9 architectural seams → review at EVERY seam; multi-round external on A1Z, A2, C1, B1 + the final Integration (3–5 rounds, convergence = 2 reviewers same round MERGE-only-Info). Mid-review at the C→A seam (after C1) and before the Integration golden capture. Each piece = its own small PR → `rust-gateway`; `bash scripts/merge-debt.sh` at session start + a closure note per piece.

## 5. Acceptance criteria
- `fiscalize` live: SELL on an OPEN shift → ACK / OFFLINE_LOCAL_ACK; the seam never 501s for a bound listener.
- WL-1: SHIFT_OPEN→ACK opens (shift_state OPENED + current_shift_id); SELL on CLOSED refused; **Z→ACK aggregates + closes** (dual-hash path).
- Crash-recovery: killed mid-fiscalize → reaper re-drives or resolves with no double-fiscalize.
- Invariants: `with_immediate_no_foreign_io` passes; per-FN single-writer under load; active-shift uniqueness enforced (C2); NO raw node_state shift write outside the transition-service (C1).
- Full `cargo test -p prro --features test-support` green; external review converged.

## 6. Open questions (after rev2 + the external plan-review)

RESOLVED by the external plan-review:
- C2 enum wire strings — CONFIRMED `CREATED, OPENING, OPENED_LOCAL_PENDING_DRAIN, OPENED, CLOSING_LOCAL_PENDING_DRAIN, CLOSING` (exclude CLOSED/REQUIRES_MANUAL_RECONCILIATION/ERROR). Locked into C2's predicate.
- A1Z `source_sha256` placement — RESOLVED as a new `CanonicalFiscalCommand` FIELD (A1), with the full caller-ripple list.
- KVT2-confirm gap (online send→finalize) — now an explicit NET-NEW seam in A2 (re-expose `offline_sync::kvt2_confirm`).
- shift↔document link (SHIFT_OPEN shift_id NULL) — now pinned in C1's stage_acquire integration.

REMAINING (watch during implementation):
1. **C1 backlog_drain migration blast radius** — `backlog_drain` is a heavily-reviewed M3b path; migrating its shift/node writes into the transition-service must preserve its exact edge semantics (a focused diff + the existing backlog_drain tests must stay green). If the blast radius proves large, fall back to a scoped, documented exception for backlog_drain rather than a risky rewrite.
2. **KVT2-confirm re-expose surface** — widening `confirm_drain_doc` visibility vs extracting a thin runtime seam: decide at A2 so the offline drain's caller path is not disturbed (its existing tests must stay green).
