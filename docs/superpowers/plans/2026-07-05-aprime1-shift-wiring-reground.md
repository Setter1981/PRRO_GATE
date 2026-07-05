# A′.1 Phase A — re-ground DOSSIER + re-scoped piece-plan (shift-lifecycle wiring)

**Status:** Phase-A deliverable (read-only re-ground). Companion to
[`2026-05-29-online-shift-lifecycle-wiring.md`](./2026-05-29-online-shift-lifecycle-wiring.md) (§0 REVISION 2,
Option A′ operator-locked 2026-05-30, which supersedes §1/§3/§4/§5/§9). **For architect GO** → then Phase B
(code: pieces 0b+1). No production code / tests / migrations touched; RED-pin `m1_02` untouched; binding
(`supervisor.rs:188`) untouched; A.1 docs (PR #220) untouched.
**Provenance:** every `file:line` below was machine-verified by `grep`/`Read` against `main` (`c9f96f4`).
Plan-cited anchors are shown against the machine-verified-now anchor; drift is called out.

---

## 0. Verdict — all three Phase-A STOP-gates CLEAR → GO recommended

| STOP-gate | Result | Evidence |
|-----------|--------|----------|
| **(a)** any §0.1 premise inverted (beyond the 3 known drifts) | **CLEAR** | boot still seeds `Closed`, `insert_created` still zero prod callers, `current_shift_id` still never set on the online path, Offline Pattern C still unreachable (no Offline mode setter). One NEW drift found (`ShiftId::new()` now exists in prod) but it is a §0.3 *design-rationale* premise, not a blocking §0.1 premise, and does not break the design (§4.3). |
| **(b)** inline.rs already moves shift-state | **CLEAR** | `inline.rs` touches only `fiscal_documents::transition_state` (doc→`Aborted` @ `inline.rs:300`); zero refs to `shifts`/`shift_state`/`current_shift_id`/`apply_shift_transition`. Driver dormant behind `UnimplementedWritePath` (`supervisor.rs:188`). |
| **(c)** `apply_shift_transition` needs SEMANTIC extension | **CLEAR** | `allowed_transition` (`shifts.rs:73-93`) already whitelists **all 7** needed edges (1/2/3/7/8/9/10). No new pairs, no mechanism change. The only non-covered thing is the row **CREATE** (an INSERT, not a CAS) — a caller-plumbing task, not a semantic extension (§4.1). |

**Net:** the Option-A′ design holds. Piece 0a is removed (pre-confirmed drift 1). Pieces 0b–5 re-scope under
today's code; **three pieces shrink** because the code moved ahead of the plan (orphan-clear + drain-classifier
already done; §4). Recommendation: **GO** to Phase B on the re-scoped 0b+1.

---

## 1. Open-question rulings §10 (fixed as CLOSED per operator/architect)

| Q | Ruling | Ground |
|---|--------|--------|
| Q1/Q2/Q5 | **Option A′** — wire both channels; `shifts` table is populated; `node_state.shift_state`/`current_shift_id` are the READ-projection kept in lock-step via the **`apply_shift_transition` discipline** (`transition.rs`). | operator 2026-05-30; drift-guard `shift_transition_service_is_sole_node_state_writer.rs` enforces projection lock-step. |
| Q3 | **lnd = per-FN-continuous** (`ux_fd_fn_lnd` / ADR-M3-A1; WebCheck U1 derive-don't-adopt confirmed vs field reality) → SHIFT_OPEN does **NOT** touch `next_lnd`. **LOCKED.** | §7 plan; SHIFT_OPEN sends `local_number=0` forced, first SELL uses lnd. |
| Q4 | SENT-then-mismatch shift disposition — **defer to piece 5**. Architect prior = M3b §4.1 edge 4 + trigger-family (2) (ambiguous online SHIFT_OPEN/Z → manual recon). Ground in the M3b spec during design; final ruling = architect's. | §7 plan `stage_acquire`/`stage_send` SENT-then-mismatch. |
| Q6 | **No new shift-machinery wiring** since 2026-05-30. Every prod shift-move call-site is known-legit (offline drain `backlog_drain.rs:2460`/`:2799`; boot orphan `boot_phase.rs:2032`/`:2059`; FN bootstrap `backlog_drain.rs:3238`). `inline.rs` does NOT move shift-state. **Scaffolding premise holds.** | Q6 sweep (STOP-gate b). |

---

## 2. Drift catalog (plan-cited anchor → machine-verified-now)

| # | Premise / anchor | Plan-cited | Machine-verified NOW | Drift class |
|---|------------------|-----------|----------------------|-------------|
| D-1 | `stage_acquire::run` has ZERO prod callers | `stage_acquire.rs:48` | **INVERTED** — `inline.rs:534` calls it (A2.1b orchestrator, merged post-plan); dormant behind `UnimplementedWritePath` `supervisor.rs:188` | **pre-confirmed inversion** → Piece 0a REMOVED |
| D-2 | `mirror_node_state_shift_state_tx` (the mirror) | `backlog_drain.rs:2226-2253` | **REPLACED** by `apply_shift_transition` (`transition.rs:58`) + internal `mirror_projection_tx` (`transition.rs:132-140`); sole node_state-projection writer | **pre-confirmed replacement** |
| D-3 | `insert_created` line | `shifts.rs:119` | `shifts.rs:126` (pool-bound INSERT, hardcodes `state='CREATED'`, requires `opened_by_cashier_id`) | line drift (pre-confirmed) |
| D-4 | boot seeds `Closed` via `upsert_initial(…, Online, Closed, 1)` | `boot_phase.rs:1304` | `boot_phase.rs:1835` | line drift |
| D-5 | `(Sell, Closed) → ShiftNotOpen` refusal | `stage_acquire.rs:897` | `stage_acquire.rs:~900-903` (arm `(Sell\|Return\|…\|XReport, Closed, _)`); guard fn `check_shift_guard` @ `:849`; ShiftOpen rows `:865-871` | line drift; **guard matrix intact** (MUST NOT change per §1 plan) |
| D-6 | `ShiftInvariantViolation` (open-state + NULL `current_shift_id`) | `stage_acquire.rs:440-453` | `stage_acquire.rs:432`, `:449` | line drift |
| D-7 | fresh-Proceed: resume-detect / terminal-dup | `stage_acquire.rs:554-563` / `:570` | resume → `Resumed(WorkerContext` @ `:555`; terminal-dup nearby | ~stable |
| D-8 | orphan boot resolution ERRORs OPENING shift + sets CLOSED but **does NOT clear `current_shift_id`** | `boot_phase.rs:1491` | **FUNCTIONALLY DRIFTED** — `boot_phase.rs:2032` `force_orphan_shift_to_error` **+ `:2059` `clear_active_shift_projection`** (RS-3 C1) **already CLEARS `current_shift_id`** (`transition.rs:50` doc) | **material drift → Piece 5 shrinks** |
| D-9 | `stage_offline_ack` gate `Opened` + active session | `:268-318` | doc_type match `:276-289` (regular ops widened to `Opened\|OpenedLocalPendingDrain`; all else `Opened` only) | line drift; **W10b absent** (D-13) |
| D-10 | `opened_by_cashier_id` NOT NULL | migration `016:131` | `rust/prro/migrations/001_baseline.sql:496` (folded into baseline; no standalone 016 file); `__pre_w14a1__` sentinel referenced only in `ids.rs:229` comment | source moved (baseline) |
| D-11 | partial UNIQUE index on `shifts(fiscal_number) WHERE open-states` | proposed / absent | **ABSENT** — only non-unique `ix_shifts_fn_state` (`001_baseline.sql:518`) | confirmed-absent → Piece 0b migration |
| D-12 | "no prod ShiftId generator" (deterministic derivation justified by absence) | §0.3 rationale | **INVERTED** — `ShiftId::new()` (UUIDv7) exists in prod: `boot_phase.rs:4275`, `backlog_drain.rs:3252` (test: `inline_map.rs:524`) | **NEW drift** — rationale outdated; **design survives** (§4.3) |
| D-13 | W10a (reserve ≥2 code-pool gate) + W10b (`stage_offline_ack` accept `DocType::ShiftOpen`) UNIMPLEMENTED | §0.3 | **BOTH still ABSENT** — W10a: `min_offline_codes` in `FiscalNumberConfig` but never consulted at open; W10b: no `DocType::ShiftOpen` arm (falls to `_` → `Opened` only) | confirmed |
| D-14 | drain classifies rejects `DocVerdict::Failed{manual_recon:true}` (needed for edges 6/14) | verify | **ALREADY DONE** — `backlog_drain.rs:1384` (ChainSeedMismatch, hardwired) + `:1398` (`is_manual_recon_retry_class`) | **material drift → Piece 5 shrinks** |
| D-15 | `stage_send` send-inputs carry `doc_type`+`shift_id` at 4-b | plan §4:136 "no new reads"; §0.3 corrects "not in scope at 4-b" | **CONFIRMED not-in-scope** — `SendInputs` has `doc_type`+`shift_id:Option<ShiftId>` (`fiscal_documents.rs:1438-1484`), but 4-pre extracts only `(envelope, attempt_no, doc_type, fiscal_number)` into `PreOutcome::Marked` and the 4-b closure captures only `decision/forensics/started/finished/fiscal_number/doc` — neither `doc_type` nor `shift_id` reach 4-b | confirmed → Piece 2 threading |
| D-16 | next migration number | — | `026` (authoritative `rust/prro/migrations/`: 001, 002, **025**_fiscal_documents_aborted_state); `sql/` 013-024 = **dead Python gateway**, ignore | — |

---

## 3. Re-scoped piece decomposition (A′, today's code)

**Piece 0a — REMOVED** (D-1). The live-ingress driver exists (`inline.rs:534 → stage_acquire::run`) but sleeps
behind `UnimplementedWritePath`; waking it = the **A.3 binding flip**, out of A′.1 scope. A′.1 tests drive stages
directly (as `kill_point_matrix` does).

**Piece 0b — create primitive** (Phase B, with Piece 1):
- Add **`insert_created_tx(tx: &mut WriteTxConn<'_>, …)`** (D-3: current `insert_created` is pool-bound, cannot
  run in a `with_immediate` envelope).
- **Deterministic `shift_id`** from the SHIFT_OPEN `document_id`/`request_id` (NOT `ShiftId::new()`): an accidental
  re-create then collides on PK. **NB (D-12):** the plan's justification "no prod generator exists" is now false
  (`ShiftId::new()` runs in prod), but the design decision stands on its own — determinism is required for
  idempotent re-drive, independent of whether a random generator exists.
- **Partial UNIQUE index** `ON shifts(fiscal_number) WHERE state IN (<open-states>)` — **MIGRATION** (D-11: only
  the non-unique `ix_shifts_fn_state` exists). **Next free number = `026`** in `rust/prro/migrations/` (D-16).
  Migration-discipline justification required in the PR (fail-closed backstop for the one-open-shift-per-FN invariant).
- `opened_by_cashier_id` (NOT NULL, `001_baseline.sql:496`) sourced from the canonical command's **signer identity**;
  if the cashier-registry FK is deferred pre-pilot, **flag it — never pass the `__pre_w14a1__` sentinel** (D-10;
  it silently defeats §16.8).

**Piece 1 — node_state mirror via `apply_shift_transition`** (Phase B, with Piece 0b):
- Edges needed and their CAS-pair coverage (all COVERED by `allowed_transition` `shifts.rs:73-93`): **1** `Created→Opening`,
  **3** `Opening→Opened`, **8** `Opened→Closing`, **10** `Closing→Closed`, **2** `Created→OpenedLocalPendingDrain`,
  **7** `OpenedLocalPendingDrain→ClosingLocalPendingDrain`, **9** `Opened→ClosingLocalPendingDrain`.
- **Create-init ordering (KEY refinement, §4.1/§4.2):** `apply_shift_transition`'s mirror CAS keys on
  `(fiscal_number, current_shift_id=shift_id, shift_state=from)`. Before edge 1/2 can fire, the create envelope must
  atomically (i) INSERT the `shifts` row `Created` and (ii) set `node_state.current_shift_id=new_id` +
  `shift_state=Created`. `(Closed, Created)` is **not** a whitelisted edge and `apply_shift_transition` does **not**
  do row-create, so this init is a caller step — **and it must NOT be a bare `UPDATE` from `stage_acquire`/`stage_offline_ack`**
  (the `shift_transition_service_is_sole_node_state_writer` drift-guard restricts bare `UPDATE …shift_state/current_shift_id`
  to `services/shift/transition.rs`; INSERT/`ON CONFLICT DO UPDATE` is allowed for `node_state.rs`). **Design choice for the
  architect (§4.2):** house the create-init in `services/shift/transition.rs` (allowlisted) as a new
  `create_shift_tx`, OR route the pointer set through a `node_state.rs` upsert. Either keeps the drift-guard green.

**Piece 2 — `stage_send` confirm edges 3/10** (Phase C, separate contract):
- Thread `inputs.doc_type` + `inputs.shift_id` INTO the 4-b `with_immediate` closure (D-15: neither is captured today;
  extend `PreOutcome::Marked` or re-capture). Fire edge 3/10 on the fresh `WireDecision::Sent` arm (`stage_send.rs:1373`).
- **CRITICAL (§2 plan):** confirm at **SENT + `server_fiscal_no`**, NOT the terminal `Ack` — hooking at `stage_finalize`
  would block SELLs for the whole reconcile window (INV-03 usability). **See §5 coordination note — same 4-b closure as A.3.**

**Piece 3 — `stage_acquire` online create + edges 1/8** (Phase C):
- Strictly in the **fresh-Proceed path** (AFTER resume-detect `stage_acquire.rs:555` + terminal-dup) so re-drives
  short-circuit before INSERT. Co-write `shift_state`+`current_shift_id` in one envelope (else `ShiftInvariantViolation`
  `:432/:449`).

**Piece 4 — `stage_offline_ack` offline: W10a + W10b + edges 2/7/9** (Phase C):
- **W10a ABSENT** (D-13): implement the reserve-≥2 code-pool gate (config field `min_offline_codes` exists, dead-wired).
- **W10b ABSENT** (D-13): `stage_offline_ack` must ACCEPT `DocType::ShiftOpen` (add the arm at `:276-289` + the
  `OfflineLocalAck` path + code acquisition).
- Edges 2/7/9; **create co-located INSIDE the offline-ack envelope** (NOT in `stage_acquire` — else an orphan-shift
  window if offline-ack later refuses on `NoActiveSession`/`CodePoolExhausted`).

**Piece 5 — crash-recovery + drain-classifier + Pattern-C e2e** (Phase C — **SHRUNK**):
- **Orphan `current_shift_id` clear — ALREADY DONE** (D-8: `clear_active_shift_projection` `boot_phase.rs:2059`).
  Piece 5 reduces to: **add the regression test** proving no dangling pointer, and verify the clear covers the new
  online-OPENING orphan case.
- **Drain-classifier `manual_recon:true` — ALREADY DONE** (D-14: `backlog_drain.rs:1384/1398`). Reduces to: **assert**
  edges 6/14 fire once `current_shift_id`+pending-drain are set (no drain change).
- **Pattern-C e2e** (still needed): offline open → SELL → return-online → drain → `Opened`; + drain-reject → `RMR`.

---

## 4. Key design refinements (surfaced by the re-ground)

### 4.1 `apply_shift_transition` covers all edges; the gap is row-CREATE only
`apply_shift_transition` (`transition.rs:58-70`) = `shifts::transition_state` CAS → on `Applied`, `mirror_projection_tx`
CAS on `node_state`. Precondition (docstring `:41-45`): `shift_id` must ALREADY be `current_shift_id`. All 7 needed
edge pairs are in `allowed_transition`. **No semantic extension** (STOP-c clear). The only non-covered operation is the
initial shift-row INSERT (→`Created`) + the first-touch `node_state` pointer init.

### 4.2 The create-init is drift-guard-constrained (new, load-bearing)
The `shift_transition_service_is_sole_node_state_writer` drift-guard restricts a bare
`UPDATE node_state SET shift_state/current_shift_id` to `services/shift/transition.rs` only (INSERT / `ON CONFLICT DO
UPDATE` also allowed for `node_state.rs`). `(Closed, Created)` is not a whitelisted edge. **⇒ the create-init cannot be a
raw `UPDATE` from `stage_acquire`/`stage_offline_ack`.** Piece 0b must house it in the transition-service allowlist (a new
`create_shift_tx`) or a `node_state.rs` upsert. **Open design decision for the architect** (do not invent): which home.

### 4.3 `ShiftId::new()` drift is non-blocking
`ShiftId::new()` (UUIDv7) now exists in prod (D-12). The plan's determinism rationale ("no generator exists") is stale,
but deterministic derivation is still the correct choice for **idempotent re-drive** (re-create collides on PK). No design
change; the justification is re-based onto idempotency. (Existing prod `ShiftId::new()` sites — boot recovery + drain
bootstrap — use random UUIDv7 in different contexts and are not the SHIFT_OPEN create path.)

### 4.4 Two pieces pre-satisfied by post-plan code
Orphan `current_shift_id` clear (D-8) and drain `manual_recon:true` classification (D-14) both **already landed** (RS-3 C1
/ M2-N2a). Piece 5 collapses to regression tests + the Pattern-C e2e; no new recovery/classification code.

---

## 5. Coordination note — the 4-b `with_immediate` closure is shared with A.1/A.3

Edge 3/10 confirm (Piece 2) lands in the **same** `stage_send` 4-b `with_immediate` closure (`stage_send.rs:1373`,
`WireDecision::Sent` arm) where **A.3** will place the **advance-at-SEND seed** write (D1–D7 LOCKED, PR #220, on external
audit). **No column conflict** — A′.1 writes `shifts` / `node_state.shift_state`+`current_shift_id`; A.3 writes
`node_state.last_known_unsigned_xml_sha256`. But the closure gets **dense**, so:
- **Write-order inside the closure (record now):** CAS `Sending→Sent` (Applied) → `set_server_fiscal_no_tx` (A.3
  discriminator + seed advance ride here) → **edge 3/10 shift CAS via `apply_shift_transition`** (A′.1) → trace
  `complete_tx`. Each shift + seed change is **independently CAS-guarded** (shift: `allowed_transition` + mirror CAS;
  seed: A.3's pre-advance drift-assert), so a Conflict on one is a no-op for that change, not a closure-wide failure.
- **Landing order:** A′.1 likely lands **first** (A.3 waits audit → LOCK → impl). Whichever lands second threads its
  write into the already-present closure; the Phase-B PR that lands first should leave a clear seam comment naming the
  other's insertion point.

---

## 6. Phase-B constraints (pieces 0b+1; strict RED-first TDD, minimal diff)

- **Frozen #1** — all shift writes DB-only inside existing envelopes; no I/O (wire call already returned before 4-b). ✓ by construction.
- **Frozen #2** — both hooks run under the FN single-writer lease. ✓
- **Frozen #8** — every transition CAS-guarded; idempotent re-drive → CAS `Conflict` = no-op + Info audit (not error).
- **INV-03** — the `Opening` window refuses SELL until DPS-confirm; this is a *strengthening*, not a regression.
- TDD: test-before-code, paired negative teeth (e.g. `(Closed,Created)`-not-whitelisted refusal; drift-guard stays green;
  re-drive no-op). Pieces 2/3/4/5 = separate contracts after the 0b+1 review.

---

## Appendix A — machine-verified anchor index

- Binding: `UnimplementedWritePath` `supervisor.rs:188` · driver `inline.rs:534 → stage_acquire::run`.
- Transition service: `apply_shift_transition` `transition.rs:58-70`; `mirror_projection_tx` `:132-140`;
  `force_orphan_shift_to_error`/`clear_active_shift_projection` used at `boot_phase.rs:2032`/`:2059`.
- Whitelist: `shifts::allowed_transition` `shifts.rs:73-93` (15 edges incl. M2-N2a edge 15).
- Repo: `insert_created` `shifts.rs:126` (pool-bound); `ix_shifts_fn_state` `001_baseline.sql:518`;
  `opened_by_cashier_id NOT NULL` `001_baseline.sql:496`.
- Guards: `check_shift_guard` `stage_acquire.rs:849`; ShiftOpen rows `:865-871`; `(Sell,Closed)→ShiftNotOpen` `:~900-903`;
  `ShiftInvariantViolation` `:432/:449`; fresh-Proceed resume `:555`.
- boot seed: `upsert_initial(…Closed…)` `boot_phase.rs:1835`.
- offline-ack gate: `stage_offline_ack.rs:276-289` (no `DocType::ShiftOpen` arm — W10b absent).
- stage_send: `SendInputs` `fiscal_documents.rs:1438-1484`; 4-pre extract `stage_send.rs:1256-1262`; 4-b closure
  `:1334-1350`; `WireDecision::Sent`/`set_server_fiscal_no_tx` `:1373-1374`.
- drain classifier: `DocVerdict::Failed{manual_recon}` `backlog_drain.rs:1384` (hardwired) / `:1398`
  (`is_manual_recon_retry_class`).
- Sole-writer drift-guard: `tests/shift_transition_service_is_sole_node_state_writer.rs` (PROJECTION_WRITER_ALLOWLIST =
  `services/shift/transition.rs`).
- Migrations: authoritative `rust/prro/migrations/` (001, 002, 025) → next `026`; `sql/` 013-024 = dead Python gateway.
