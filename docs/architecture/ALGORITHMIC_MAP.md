# W4-Z4 Algorithmic Map — Pilot Gate Document

> **Status:** W4-Z4 Pilot Readiness gate artifact (Rust gateway).
> **Authority:** This is the regulatory algorithmic map mandated by
> `docs/architecture/W4_Z4_PILOT_READINESS_STABILIZATION.md` §1. Its structure
> mirrors that template (§1.1–§1.11). Every state name here is a **real enum
> string** from `rust/prro/src/db/models/enums.rs`; every file/line reference is
> a real landmark in the Rust tree. No invented vocabulary.
> **Stack:** RUST-ONLY. `src/prro_gateway/` (Python) is a dead reference
> contour; there is no pytest/ruff in the pilot gate.
>
> **Honesty contract (the central correction of this rewrite).** Many earlier
> drafts assumed Python-era parity. They do not have it. Every algorithm and
> every state edge below is explicitly marked **WIRED** (has a production caller
> AND a regression-pin / oracle test) or **UNWIRED** (whitelist / schema /
> gap-marker exists, but NO production driver today). Do not read an UNWIRED row
> as "implemented". See §1.11 for the consolidated gap table.
>
> **Cross-links:**
> - `docs/architecture/2026-05-29-pilot-integration-map.md` — pilot wiring map (companion).
> - WL-1 shift-lifecycle plan
>   (`docs/superpowers/plans/2026-05-29-online-shift-lifecycle-wiring.md`) — the
>   work-item that closes the UNWIRED online shift lifecycle edges (3/4/8/10/11/12).
> - `docs/LEGAL_INVARIANTS.md` — INV-01 … INV-20 (authoritative invariant text).
> - `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` — 9-state shift spec (§16 operational reality).

---

## 1.1 Online / Offline / Rejoin Branches

The gateway has three macro-branches. The branch is chosen at dispatch time
(after `stage_sign`) from `node_state.mode` (`NodeMode`). All three share the
same `stage_acquire → signer_guard → stage_sign` prefix.

### Online branch (`NodeMode = ONLINE`)

```
ingress (InboxStatus NEW→PROCESSING)
  → stage_acquire    : lease + LND + check_shift_guard (162-cell)   [WIRED]
  → signer_guard     : cert/session bind
  → stage_sign       : DocState PREPARED→SIGNED (CMS-detached, see §1.8)
  → dispatch         : NodeMode==ONLINE → stage_send
  → stage_send       : SIGNED→SENDING→SENT (wire send_chk_v2)        [WIRED on HEAD (mock); full W4-Z3 live cycle branch-proven / pending-merge]
  → stage_finalize   : SENT→KVT1→KVT2→ACK  (KVT2 decrypt, §1.8)
  → ingress DONE
```

### Offline branch (Pattern C, `NodeMode ∈ {OFFLINE, GOING_OFFLINE}`)

```
ingress (NEW→PROCESSING)
  → stage_acquire    : lease + LND + check_shift_guard               [WIRED]
  → signer_guard
  → stage_sign       : PREPARED→SIGNED
  → dispatch         : NodeMode∈{OFFLINE,GOING_OFFLINE} → stage_offline_ack
  → stage_offline_ack: acquire_code_tx + SIGNED→OFFLINE_LOCAL_ACK    [WIRED, stage_offline_ack.rs:320 (transition)]
                       stamps offline_fiscal_no=code_lnd,
                       offline_fiscal_date=consumed_at;
                       emits OFFLINE_LOCAL_ACK_APPLIED audit (stage_offline_ack.rs:350); pipeline TERMINATES.
  → ingress DONE  (customer-facing receipt is issued; NOT DPS-accepted — INV-13)
```

The document is **durably locally committed** as `OFFLINE_LOCAL_ACK`. It is a
real receipt in the customer's hand. It is retained until DPS `ACK` (INV-14).
NodeMode pre-guards refuse the offline ACK path for `BLOCKED / STOP_MODE /
CRYPTO_DEGRADED / GOING_ONLINE` (typed dispatcher refusal, NOT routed through
`stage_offline_ack`).

### Rejoin branch (return-online drain, W8 probe → W9b drain)

```
NodeMode OFFLINE
  → return-online probe succeeds (read-only over wire)              [WIRED, W8]
  → NodeMode OFFLINE→GOING_ONLINE (idempotent; no fiscal mutation)
  → W9b backlog drain orchestrator (lnd ASC):                       [doc-finalize WIRED, backlog_drain.rs;
      SHIFT-transition + escalation UNWIRED in production — keyed on a pending-drain shift_state prod never sets (insert_created undriven, current_shift_id never set)]
      for each OFFLINE_LOCAL_ACK doc:
        OFFLINE_LOCAL_ACK → SENDING → SENT → KVT1 → KVT2 → ACK    (finalize Opened→None arm, backlog_drain.rs:2399–2418; advances MAC chain; NO shift transition, NO crash)
      on drain TERMINAL-reject of a backlog doc on a *PENDING_DRAIN* shift:
        → failing DOC → REJECTED; only the SHIFT → RequiresManualReconciliation
          (escalate_drain_to_manual transitions the shift only)
          + Critical OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL; FN drain HALTS
          [UNWIRED in production — escalation guarded by shift_in_pending_drain (backlog_drain.rs:952); pending-drain state never set → escalate_drain_to_manual (:2155) / Critical audit (:2191) UNREACHABLE; the SAFETY is NON-FUNCTIONAL, a silent absence (not a crash). See §1.11 Hard-Blocker list.]
  → empty backlog → NodeMode GOING_ONLINE→ONLINE
```

There is **no two-consecutive-success guard** before `GOING_ONLINE` (unlike
PRRODPS). The intermediate `GOING_ONLINE` buffer state plus the W9b drain
itself catch a false-positive probe — a drain reject lands the shift in manual
reconciliation rather than silently re-fiscalising.

---

## 1.2 End-to-End Fiscal Flow (real stages, real DocState)

Real write-path stages (`rust/prro/src/services/write_path/`): `stage_acquire`,
`stage_sign`, `stage_send`, `stage_finalize`, `stage_offline_ack`, plus
`dispatch`, `signer_guard`, `mac_recovery`, `error_routing`. Real DocState enum
(`enums.rs:29`): `PREPARED, SIGNED, ENCRYPTED, SENDING, SENT, KVT1, KVT2, ACK,
OFFLINE_LOCAL_ACK, REJECTED, CANCELLED, ERROR_RETRYABLE,
REQUIRES_MANUAL_RECONCILIATION`. Terminals: `ACK` / `REJECTED` / `ERROR_*`.

| Step | Stage / Module | DocState Before | DocState After | Idempotency Key | Resume Trigger | Resume Actor | DPS Idempotency Surface | Failure Recovery | Enforces (INV-NN) | Tests / Status |
|---|---|---|---|---|---|---|---|---|---|---|
| 1. Ingress accept | `runtime/ingress` | (none) | (inbox `NEW`) | `ingress_inbox.idempotency_key`, UNIQUE `(fiscal_number, idempotency_key)` | client retry | client | — | reject at ingress + `audit_log`; never persisted to `fiscal_documents` | INV-07 | WIRED (`ux_inbox_fn_idem`, migr 002:91) |
| 2. Acquire + LND + shift guard | `stage_acquire` | (none) | `PREPARED` | inbox key | boot reconciler | boot reconciler | — | rollback; lease released; row stays absent | INV-01, INV-02, INV-03, INV-15 | WIRED (162-cell oracle) |
| 3. Sign (CMS-detached) | `stage_sign` | `PREPARED` | `SIGNED` | inbox key | background worker | worker | — | re-enter stage; bytes pinned at this step | INV-18 (no crypto in tx) | WIRED (sign primitive on HEAD; full W4-Z3 live cycle branch-proven / pending-merge) |
| 4a. Dispatch ONLINE | `dispatch` → `stage_send` | `SIGNED` | `SENDING` | inbox key | boot reconciler | boot reconciler | server-side `local_number` / `server_fiscal_no` | boot rule: `SENDING→ERROR_RETRYABLE`, **ZERO** re-sends | INV-07 (no dup fiscalise) | WIRED |
| 4b. Dispatch OFFLINE | `dispatch` → `stage_offline_ack` | `SIGNED` | `OFFLINE_LOCAL_ACK` | inbox key + `offline_fiscal_no` (UNIQUE) | W9b drain | offline drainer | — | drain later; durable local commit (no rollback past here) | INV-11, INV-12, INV-13, INV-14 | WIRED (stage_offline_ack.rs:320 transition; :350 audit) |
| 5. Wire send | `stage_send` | `SENDING` | `SENT` | inbox key | boot reconciler | boot reconciler | `server_fiscal_no` (W4-Z3 branch-proven `1g41M3jDt-Q` / `AOBSkplfIUU` / `L2AMnY2MkmA`, 2026-05-29 — on `feat/m4-w4-z3-dps-extended-smoke`, **PENDING MERGE, NOT on HEAD**) | timeout → `SENDING` persists → boot `ERROR_RETRYABLE` | INV-08 (auto-offline *trigger surface*, stub on HEAD), INV-18 | WIRED on HEAD (mock send); full live cycle branch-proven / pending-merge |
| 6. KVT1 receipt | `stage_finalize` | `SENT` | `KVT1` | inbox key | boot reconciler | boot reconciler | — | hold; re-poll | INV-19 | WIRED (mock); KVT2 confirm path W12 |
| 7. KVT2 decrypt + verify | `stage_finalize` | `KVT1` | `KVT2` | inbox key | boot reconciler | boot reconciler | — | KVT2 unwrap (decrypt, §1.8); on verify-fail → error_routing | INV-19 | WIRED |
| 8. Finalize | `stage_finalize` | `KVT2` | `ACK` | inbox key | — | — | — | idempotent mark; terminal | INV-19 | WIRED |
| R. Online reject | `error_routing` | `SENT`/`KVT*` | `REJECTED` | inbox key | — | operator | — | terminal; reject → `audit_log`, NOT a new fiscal doc | INV-07 | WIRED |
| E. Transport-class fail | `error_routing` | any non-terminal | `ERROR_RETRYABLE` | inbox key | boot reconciler / redrive | worker | — | typed RetryClass redrive; budget → `REQUIRES_MANUAL_RECONCILIATION` | INV-19 | WIRED |

**Where the document becomes byte-immutable.** At step 3 (`stage_sign`). The
exact CP1251 canonical-XML bytes that were hashed and signed are persisted; no
rebuild, reformat, attribute reorder, or re-encode is permitted afterward
(Crypto Immutable Rule, §3). Retry/resume use the **persisted signed bytes**.

**What happens on timeout after `send_chk_v2`.** The doc is left in `SENDING`
(persisted before the wire call). On boot the reconciler transitions
`SENDING → ERROR_RETRYABLE` with **ZERO** new `send_chk` invocations, because
DPS does not deduplicate — re-sending could duplicate-fiscalise. The redrive
classifier (RetryClass) then decides probe vs hold vs escalate.

**Why retry never fiscalises a duplicate.** One inbox key →
`(fiscal_number, idempotency_key)` UNIQUE → at most one `fiscal_documents` row;
the canonical hash is explicitly NOT the idempotency key (`dto.rs:304–317`).

**Naming note (`lnd` vs `local_number`).** `lnd` is the **persisted column
name** (`fiscal_documents.lnd`, migr 002:9); `local_number` /
`server_fiscal_no` are the **wire-level names** for the same local document
ordinal as it appears on the DPS protocol. They denote the same monotonic
ordinal (INV-02), just under storage-side vs wire-side naming.

---

## 1.3 State Machines (5 real machines)

For each machine the `State` column uses the **exact SQL string** stored in
SQLite TEXT columns.

### (1) `ingress_inbox.status` (enum `InboxStatus`, enums.rs:117)

| State | Allowed Next | Owner | Illegal Transition | Recovery | Status |
|---|---|---|---|---|---|
| `NEW` | `PROCESSING`, `REJECTED` | ingress | — | re-pick on boot | WIRED |
| `PROCESSING` | `DONE`, `REJECTED`, `ERROR` | write-path worker | — | boot re-pick stuck `PROCESSING` | WIRED |
| `DONE` | (terminal) | worker | reopen forbidden | — | WIRED |
| `REJECTED` | (terminal) | ingress/worker | — | — | WIRED |
| `ERROR` | (terminal for inbox) | worker | — | doc-level redrive owns recovery | WIRED |

### (2) `fiscal_documents.state` (enum `DocState`, enums.rs:29)

| State | Allowed Next | Owner | Illegal Transition (CAS) | Recovery | Status |
|---|---|---|---|---|---|
| `PREPARED` | `SIGNED` | `stage_sign` | `Forbidden`/`Conflict` typed | re-enter | WIRED |
| `SIGNED` | `SENDING`, `OFFLINE_LOCAL_ACK` | `dispatch` | typed | re-dispatch | WIRED |
| `ENCRYPTED` | `SENDING` | (legacy Pattern B) | typed | re-send | Present (M3a) |
| `SENDING` | `SENT`, `ERROR_RETRYABLE` | `stage_send` | typed | boot → `ERROR_RETRYABLE`, no re-send | WIRED |
| `SENT` | `KVT1`, `REJECTED`, `ERROR_RETRYABLE` | `stage_finalize` | typed | poll/hold | WIRED |
| `KVT1` | `KVT2`, `ERROR_RETRYABLE` | `stage_finalize` | typed | re-poll | WIRED |
| `KVT2` | `ACK`, `REJECTED` | `stage_finalize` | typed | verify | WIRED |
| `OFFLINE_LOCAL_ACK` | `SENDING`, `CANCELLED`, `REQUIRES_MANUAL_RECONCILIATION` | drain (W6 whitelist) | typed | W9b drain; reject→manual | WIRED |
| `ACK` | (terminal) | — | — | — | WIRED |
| `REJECTED` | (terminal) | — | — | — | WIRED |
| `CANCELLED` | (terminal) | — | — | — | WIRED |
| `ERROR_RETRYABLE` | `SENDING`, `REQUIRES_MANUAL_RECONCILIATION` | redrive | typed | RetryClass redrive | WIRED |
| `REQUIRES_MANUAL_RECONCILIATION` | (terminal) | operator | — | Critical audit WIRED; forensic snapshot + pager UNWIRED (operator-procedure only — DF-4) | WIRED (state); snapshot+pager UNWIRED |

### (3) `shifts.state` (enum `ShiftState`, 9-state, enums.rs:62) — 14-edge table

Edge whitelist: `shifts.rs::allowed_transition` (shifts.rs:67). Each edge is
marked WIRED (a production caller exists via drain/boot) or UNWIRED (in the
whitelist, but **no production driver today**).

| # | Edge | Trigger | Wiring |
|---|---|---|---|
| 1 | `CREATED → OPENING` | online SHIFT_OPEN start | **UNWIRED** (online lifecycle not driven; the `CREATED` shift-CREATION step itself has no production driver — `shifts::insert_created` (shifts.rs:119) has ZERO production callers) |
| 2 | `CREATED → OPENED_LOCAL_PENDING_DRAIN` | offline SHIFT_OPEN ingress (Pattern C) | **UNWIRED** (shift CREATION undriven — same class as edges 3/8/10: `insert_created` has ZERO production callers; the only `INSERT INTO shifts` in src is `#[cfg(test)]` (backlog_drain.rs:2953); `node_state.current_shift_id` is never set in production) |
| 3 | `OPENING → OPENED` | online send → DPS Ack | **UNWIRED** (W4-Z3 confirmed shift never opens online) |
| 4 | `OPENING → REQUIRES_MANUAL_RECONCILIATION` | ambiguous online SHIFT_OPEN timeout | **UNWIRED** (edge unreachable; recovery "proposed"/absent) |
| 5 | `OPENED_LOCAL_PENDING_DRAIN → OPENED` | drain SHIFT_OPEN Ack + empty backlog | **UNWIRED in production** (keyed on a PENDING-DRAIN `shift_state` production NEVER sets — `insert_created` undriven, `node_state.current_shift_id` never set; `commit_finalize` for `OPENED_LOCAL_PENDING_DRAIN` needs `current_shift_id` → never reached, backlog_drain.rs:2399–2418. Transition code-wired + tested on test-seeded rows only) |
| 6 | `OPENED_LOCAL_PENDING_DRAIN → REQUIRES_MANUAL_RECONCILIATION` | drain reject of backlog | **UNWIRED in production** (escalation guarded by `shift_in_pending_drain`, backlog_drain.rs:952 → unreachable in prod; `escalate_drain_to_manual` / current_shift_id check at :2155 never reached. The drain-reject → manual-recon **safety** (INV-19) is NON-FUNCTIONAL because the pending-drain state it keys on is never set. Code-wired + tested on test-seeded rows only) |
| 7 | `OPENED_LOCAL_PENDING_DRAIN → CLOSING_LOCAL_PENDING_DRAIN` | offline Z_REPORT before drain | **UNWIRED in production** (pending-drain `shift_state` never set; code-wired + tested on test-seeded rows only) |
| 8 | `OPENED → CLOSING` | online Z_REPORT / SHIFT_CLOSE ingress | **UNWIRED** |
| 9 | `OPENED → CLOSING_LOCAL_PENDING_DRAIN` | offline Z_REPORT (Pattern C) | **UNWIRED in production** (pending-drain `shift_state` never set; code-wired + tested on test-seeded rows only) |
| 10 | `CLOSING → CLOSED` | online send → DPS Ack | **UNWIRED** |
| 11 | `CLOSING → OPENED` | `Authorization::DocumentReject` only (§6.2) | **UNWIRED** |
| 12 | `CLOSING → REQUIRES_MANUAL_RECONCILIATION` | ambiguous online Z_REPORT timeout | **UNWIRED** (edge unreachable) |
| 13 | `CLOSING_LOCAL_PENDING_DRAIN → CLOSED` | drain reached final Ack on all backlog | **UNWIRED in production** (pending-drain `shift_state` never set; code-wired + tested on test-seeded rows only) |
| 14 | `CLOSING_LOCAL_PENDING_DRAIN → REQUIRES_MANUAL_RECONCILIATION` | drain reject | **UNWIRED in production** (escalation guarded by `shift_in_pending_drain`, backlog_drain.rs:952 → unreachable; same NON-FUNCTIONAL safety gap as edge 6. Code-wired + tested on test-seeded rows only) |

> **Shift-CREATION gap (ties to WL-1) — and the SILENT SAFETY ABSENCE it
> creates.** No code edge that *creates* a shift row (`CREATED`) has a production
> driver: `shifts::insert_created` (shifts.rs:119) is called only from
> `tests/repo_shifts.rs`, the sole `INSERT INTO shifts` in `src` is `#[cfg(test)]`
> (backlog_drain.rs:2953; the `cfg(test)` module opens at :2753),
> `stage_offline_ack` only READS `ns.shift_state` to GUARD (:268–289) and never
> creates/transitions a shift, and `node_state.current_shift_id` is never set in
> production. So the `shifts` table is **not production-populated today**, and the
> offline Pattern C shift transition + manual-escalation logic is keyed on a
> PENDING-DRAIN `shift_state` (`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`)
> that production NEVER sets. The drain TRANSITION edges (5/6/7/9/13/14) are
> code-wired + tested (backlog_drain.rs:2169/:2498, prod caller app.rs:620) but
> only transition pre-existing (test-seeded) shift rows.
>
> **What this means at runtime — NOT a crash, and STRONGER than "seeded Opened"
> (CF-R3).** The earlier framing here claimed `shift_state` is statically seeded
> to `Opened` so SELLs are admitted and the drain finalizes the backlog via the
> `Opened → None` arm. **That premise is WRONG.** The ONLY production
> `upsert_initial` seeds `ShiftState::CLOSED` (boot_phase.rs:1304:
> `upsert_initial(pool, fn, NodeMode::Online, ShiftState::Closed, 1)`); orphan
> boot resolution only drives toward `CLOSED` (boot_phase.rs:1491); the sole
> `OPENED` write in `src` is a `#[cfg(test)]` fixture (admin.rs:903). Under
> `CLOSED`, `(Sell, Closed) → ShiftNotOpen` REFUSE (stage_acquire.rs:897) — SELL
> is NOT admitted on either channel. **Moreover offline is unreachable
> END-TO-END:** `node_state` has NO `Offline` / `GoingOffline` mode setter (only
> `set_mode_blocked_tx` / `set_mode_stop_mode_tx`); `OfflineSessionService::open_session`
> has ZERO production callers; `stage_offline_ack` requires `Opened` + an active
> offline session (stage_offline_ack.rs:268–318). So the mode never flips
> `Offline`, no offline session opens, no `OFFLINE_LOCAL_ACK` doc forms, no
> backlog exists, and `drain()` early-returns "no active session" / "empty
> backlog" — the `Opened → None` finalize arm (backlog_drain.rs:2399–2418) is
> itself UNREACHABLE in prod. **Net: prod bootstrap is `CLOSED`
> (boot_phase.rs:1304), so the gateway CANNOT transact at all today** (online SELL
> refused on `Closed`; offline path unreachable end-to-end — no mode setter, no
> session, no backlog). The "drain finalizes backlog to `Ack` without escalation"
> scenario does NOT occur in prod because no backlog ever forms. The pending-drain
> arms (which need `current_shift_id`) and the escalation
> (`escalate_drain_to_manual`, backlog_drain.rs:2155, guarded by
> `shift_in_pending_drain` at :952) are likewise UNREACHABLE. So the offline
> Pattern C shift safety machinery — pending-drain online-ops lockout (§3.3) and
> drain-reject → `RequiresManualReconciliation` escalation (INV-19) — is
> **NON-FUNCTIONAL: a silent ABSENCE of the safety machinery, not a fail-stop
> crash. For a fiscal system that is worse than a crash.** This STRENGTHENS the
> NO-GO. Closed by the WL-1 shift-lifecycle plan (which must drive a real
> `OPENED`/`current_shift_id` lifecycle and the offline pending-drain edges, not
> online-only). See §1.11 Hard-Blocker list.

`ERROR` is reachable only via the `force_to_error_with_audit` seam
(shifts.rs:444); `REQUIRES_MANUAL_RECONCILIATION` also via
`force_to_manual_reconciliation_with_audit` (shifts.rs:575); the senior-cashier
close seam is `senior_cashier_close_shift_with_audit` (shifts.rs:840). All three
are **primitive WIRED + regression-pinned, but have NO production driver /
operator entry-point today** (no admin CLI or runtime path invokes them; drain
uses `shifts::transition_state` directly) — mirror the W8-probe caveat. The
Manual-recon trigger family (3) "operator force/senior seam" is therefore **not
operator-reachable on the pilot path today**. **Net (CF-R3): prod bootstrap
seeds `ShiftState::CLOSED` (boot_phase.rs:1304), so the gateway CANNOT transact
at all today — online SELL is refused on `Closed` (`(Sell, Closed) →
ShiftNotOpen`, stage_acquire.rs:897) and the offline path is unreachable
END-TO-END (no `Offline`/`GoingOffline` mode setter; `open_session` has zero
prod callers; `stage_offline_ack` needs `Opened` + an active session). No
`OFFLINE_LOCAL_ACK` doc forms, no backlog exists, and `drain()` early-returns
"no active session" / "empty backlog" — the `commit_finalize` Opened→None arm
(backlog_drain.rs:2399–2418) is itself UNREACHABLE in prod. The drain TRANSITION
edges 5,6,7,9,13,14 are code-wired + tested via drain/boot but are UNWIRED in
production — they key on a pending-drain `shift_state` production never sets
(`insert_created` undriven + `current_shift_id` never set, WL-1 gap). So even if
the lifecycle were driven, the shift-transition / online-ops-lockout /
drain-reject-escalation SAFETY semantics are NON-FUNCTIONAL (silent absence, not
a crash — see the note above and the §1.11 Hard-Blocker list); the force/senior
seams are test-only; the shift-CREATION edges 1,2 and the online edges
3,4,8,10,11,12 are UNWIRED (online + offline shift CREATION not driven, closed by
the WL-1 plan).**

### (4) `offline_sessions.state` (enum `OfflineSessionState`, enums.rs:54)

> **No drift in the pilot (the prior note was inverted).** The live PILOT
> column is `state`, with `CHECK (state IN
> ('OPENING','OPEN','DRAINING','CLOSED','ABORTED'))`. The CHECK constraint is
> sourced from migration `015_offline_normalize.sql:140`; the value set matches
> enum `OfflineSessionState` (`enums.rs:54`). (Repo `offline_sessions.rs:225` is
> an UPDATE statement that *uses* the `state` column — it does NOT hold the
> CHECK; it is repo-uses-state evidence only, not the CHECK source.) The
> `status` / `CLOSING` shape is the **DEAD pre-015 schema** (migration 004 /
> Python `sql/001_hot_store_init.sql`); migration 015 normalized `status → state` and
> `CLOSING → DRAINING`. So in the pilot there is NO column drift: the column is
> `state` and the value is `DRAINING`. `status` / `CLOSING` is old/dead naming
> that 015 already fixed.

| State | Allowed Next | Owner | Illegal Transition | Recovery | Status |
|---|---|---|---|---|---|
| `OPENING` | `OPEN`, `ABORTED` | offline session repo | typed | re-open | WIRED |
| `OPEN` | `DRAINING`, `ABORTED` | offline session repo | typed | — | WIRED |
| `DRAINING` | `CLOSED`, `ABORTED` | W9b drain | typed | resume drain | WIRED |
| `CLOSED` | (terminal) | — | — | — | WIRED |
| `ABORTED` | (terminal) | — | — | — | WIRED |

Code-pool exhaustion: `acquire_code_tx` on empty FN pool → typed
`OfflineSessionError::CodePoolExhausted` (offline_sessions.rs:408; test
`offline_session_code_pool.rs:201`) is **WIRED + tested**. The
"→ caller enters `STOP_MODE`" half is **UNWIRED**: `stage_offline_ack.rs:315`
propagates the error via `?` ("caller's responsibility to enter STOP_MODE"); no
production caller converts `CodePoolExhausted` into a `STOP_MODE` transition.
The only production `STOP_MODE` driver is the **drain Tier-2** trigger
`trigger_tier_2_stop_mode` (backlog_drain.rs:2074, fires at
`consecutive_holds >= 50`, audit `OFFLINE_DRAIN_FN_STOP_MODE`) — a **different**
trigger. **Re-mark: typed error WIRED+tested; STOP_MODE caller-routing UNWIRED.**

### (5) `node_state.mode` (enum `NodeMode`, enums.rs:74)

| State | Allowed Next | Owner | Recovery | Status |
|---|---|---|---|---|
| `ONLINE` | `GOING_OFFLINE`, `OFFLINE`, `BLOCKED`, `STOP_MODE`, `CRYPTO_DEGRADED` | node_state repo | — | WIRED (state); auto-offline trigger = stub (INV-08) |
| `GOING_OFFLINE` | `OFFLINE` | node_state repo | — | WIRED |
| `OFFLINE` | `GOING_ONLINE`, `STOP_MODE`, `BLOCKED` | node_state repo / W8 probe | drain on rejoin | WIRED |
| `GOING_ONLINE` | `ONLINE`, `OFFLINE` | W8 probe → W9b | drain catches false positive | WIRED |
| `BLOCKED` | `ONLINE` (operator) | operator | manual unblock | WIRED (state) |
| `STOP_MODE` | `ONLINE` (operator) | operator | manual; production entry only via drain Tier-2 `trigger_tier_2_stop_mode` (backlog_drain.rs:2074). `CodePoolExhausted` does NOT route here today (caller-routing UNWIRED) | WIRED (state); CodePoolExhausted→STOP_MODE caller-routing UNWIRED |
| `CRYPTO_DEGRADED` | `ONLINE` (operator) | operator | key/cert repair | WIRED (state) |

---

## 1.4 Cross-Machine Invariants

| Invariant | Machines / Tables | Enforced By | Failure Behavior | Status |
|---|---|---|---|---|
| **INV-03** closed shift forbids signing | `shifts.state` × `DocState` | `check_shift_guard` 162-cell matrix (stage_acquire.rs:845); `(Sell, Closed) → ShiftNotOpen` | refuse at acquire; no `PREPARED` row | **WIRED** (oracle: all 162 cells) |
| **INV-15** online Z blocked w/ backlog | `shifts.state` × `OFFLINE_LOCAL_ACK` backlog | `(ZReport, OpenedLocalPendingDrain) → ZReportBlockedBacklogDrainPending` | refuse; `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED` audit; offline Z is the escape hatch (edge 7/9) | **WIRED** |
| INV-13 offline ACK ≠ DPS accept | `DocState OFFLINE_LOCAL_ACK` | Pattern C: durable local commit, sign-at-drain | doc provisional; legitimacy only at `ACK` after W9b drain | **WIRED** (concept); W12 closes KVT2 |
| INV-04 no two active shifts/FN | `shifts.state` | 9-state partial UNIQUE index | second active SHIFT_OPEN rejected | **UNWIRED** (Rust has only non-unique `ix_shifts_fn_state`, migr 001:50/016:196; 9-state `uq_active_shift_per_fiscal` is `sql/001_hot_store_init.sql:158` — **dead Python contour, historical** / 3-state; runtime guard `(ShiftOpen,*active*) → ShiftAlreadyOpen` is WIRED) |
| **INV-05** channel pinned w/ open shift | shift × routing profile | (frozen invariant #3) | mid-shift channel switch must be refused | **UNWIRED** in Rust — guard not implemented |
| INV-01 single-writer/FN | lease × write-path | `BEGIN IMMEDIATE` lease | — | WIRED |
| INV-02 LND monotonic | `node_state.next_fiscal_no` | atomic increment under `with_immediate`; rollback = VOID, never reuse | — | WIRED |

`(Sell, OpenedLocalPendingDrain, Online) → ShiftOpenPendingDrainOpRefused`;
`(ShiftOpen, *active*) → ShiftAlreadyOpen`. NodeMode pre-guards in the matrix
refuse `GoingOnline / Blocked / StopMode / CryptoDegraded`.

---

## 1.5 SQLite Transaction Map

**Frozen invariant #1 (INV-18):** `with_immediate` envelopes do **NO network,
crypto, or filesystem I/O**. CMS sign + wire send + KVT2 decrypt happen
*between* transactions. A network/crypto/fs call inside `with_immediate` is a
pilot blocker.

| Tx Envelope | Caller | Purpose | External I/O | Rollback Meaning | Audit Semantics |
|---|---|---|---|---|---|
| acquire lease + LND + shift guard | `stage_acquire` | atomic lease, `next_fiscal_no++`, guard read | **none** | no doc row; LND not consumed | audit only after commit |
| persist SIGNED bytes | `stage_sign` | store signed CP1251 bytes + `SIGNED` | **none** (sign happened before) | doc stays `PREPARED` | — |
| CAS `SIGNED→SENDING` | `stage_send` | mark intent before wire | **none** | doc stays `SIGNED` | persisted before send (Pattern B) |
| persist `SENT`/KVT/state | `stage_finalize` | record wire/KVT outcome | **none** (decrypt before) | doc stays prior state | — |
| `acquire_code_tx` + `SIGNED→OFFLINE_LOCAL_ACK` | `stage_offline_ack` | atomic offline-code consume + transition (crash atomicity) | **none** | code not consumed; doc stays `SIGNED` | `OFFLINE_LOCAL_ACK_APPLIED` only on `Applied` |
| drain transition | `backlog_drain` | per-doc CAS during drain | **none** (send/decrypt outside) | doc stays prior | escalation audits survive |

Rules in force: bounded CPU only inside write tx; no implicit nested
transactions; `SAVEPOINT` documented where present; raw `SQLITE_BUSY` must
become a bounded wait / typed retry / controlled failure — never undefined
business behavior (pool busy-timeout reviewed in PILOT_REVIEW_PLAYBOOK §2.4).

---

## 1.6 External I/O Map

| I/O | Where (relative to SQLite tx) | Failure Class | Status |
|---|---|---|---|
| CMS-detached sign | `stage_sign`, **outside** any `with_immediate` | `CryptoError` → `ERROR_RETRYABLE` | WIRED |
| Wire send `send_chk_v2` | `stage_send`, **between** SENDING-commit and SENT-commit | timeout → leave `SENDING`, boot recovers | WIRED on HEAD (mock); full W4-Z3 live cycle branch-proven / pending-merge |
| KVT2 decrypt (`unwrap_envelope`) | `stage_finalize`, **outside** tx | decrypt/verify fail → error_routing | WIRED |
| Return-online probe | W8 task, read-only over wire, **no** tx mutation | DpsError-class audit | WIRED |
| Cert fetch by SKI (CMP-look-alike) | service layer, **outside** tx | per-URL timeout | Present |
| JKS / key container read | signer bootstrap, **outside** tx | fail-fast at startup | WIRED |
| live-DPS smoke | `#[ignore]` + `live-dps` feature; **not in CI**; harness `live_dps_extended_smoke.rs` lives only on branch `feat/m4-w4-z3-dps-extended-smoke` (PENDING MERGE, NOT on HEAD) | manual runbook | branch-only / pending-merge; HEAD has only `live_smoke_w12_hardening.rs` (connect/probe-only, dummy sign, no CMS) |

---

## 1.7 Time / Date Map

| Field | Representation | Rule |
|---|---|---|
| internal chronology, all DB timestamps | **UTC only** | idempotency, ordering, LND, and state transitions NEVER depend on Kyiv-local wall time |
| `SQLite CURRENT_TIMESTAMP` | UTC | — |
| XML `TS` / DPS `date_time` on the wire | **Europe/Kyiv local** | local-time projection is **render-only**, applied at the wire boundary, never re-imported as a key |
| CMS `signingTime` | UTC | far-future / invalid → fail-fast, no silent normalize |
| cert `valid_from` / `valid_to` | UTC (UTCTime / GeneralizedTime) | 2049/2050 UTCTIME cliff handled by parser policy |
| KVT age / transport-trace TTL | UTC delta | — |

DST fallback (repeated EEST→EET hour) must not create key/ordering collisions —
collisions are structurally impossible because keys are UTC. Invalid dates fail
loud. System-clock-rollback behavior must be documented in the runbook.

---

## 1.8 Crypto / Wire Profile

> **CORRECTION (the prior draft mislabelled this).** DSTU 4145-2002 is the
> **SIGNATURE** algorithm, NOT encryption. There is no "encrypt outbound with
> the DPS public key" step.

**OUTBOUND to DPS = CMS SignedData, NOT encryption.**
- Content: CP1251-encoded canonical XML bytes (`sign_cms_detached`,
  `crypto/provider.rs:50`; `SignCmsRequest.canonical_xml` field at
  `provider.rs:33`).
- Signature: **DSTU 4145-2002** (curve PB-257).
- Hash: **GOST 34.311 / DSTU 7564 (Kupyna)**.
- **DETACHED vs ATTACHED — these are DIFFERENT signers and must not be conflated.**
  rust-gateway **HEAD**'s `InProcessProvider` signs **DETACHED** CMS (no
  `eContent`) and is **NOT live-DPS-accepted** (HEAD
  `rust/prro/src/crypto/in_process.rs` = detached). The **ATTACHED** CMS +
  `signingTime` signer that DPS actually accepted exists ONLY on the unmerged
  `feat/m4-w4-z3` branch (`in_process.rs` = attached). **HEAD = detached signer
  (not live-accepted); the pilot-accepted native ATTACHED signer is branch-only,
  pending merge + external review** (see DF-2 / §1.11 and Hard-Blocker (2)).
- W4-Z3 live cycle (`SHIFT_OPEN → SELL → Z_REPORT`, 2026-05-29) was
  **signed-only** — ATTACHED CMS SignedData, `sendChkV2` accepted by ФСКО.
  Proven path: `PREPARED → SIGNED → SENT` (`server_fiscal_no` values in §1.2).
  **This cycle is branch-proven on the unmerged `feat/m4-w4-z3` /
  `feat/m4-w4-z3-dps-extended-smoke` branch / PENDING MERGE — NOT on rust-gateway
  HEAD. HEAD signs DETACHED and that detached output is NOT live-DPS-accepted.**

**INBOUND (KVT2) = decryption.** `unwrap_envelope` (`provider.rs:79`) DECRYPTs
DPS EnvelopedData; the recipient private key (in `SigningSession`) plus the
originator's public key derive the ECDH CEK. **Encryption exists on the inbound
path only.**

Byte-immutability rules (Crypto Immutable Rule, §3): the exact CP1251 bytes
passed to `sign_cms_detached` are the exact bytes hashed and embedded as CMS
`eContent`. After signing: no XML rebuild, reformat, attribute reorder,
whitespace rewrite, or encoding conversion. Retry/resume use the persisted
signed bytes unless an explicit controlled re-sign path is taken. DER `SET OF`
lexicographic sorting in `signedAttrs` (incl. `signingTime`,
`SigningCertificateV2`) must be documented and tested.

---

## 1.9 Recovery Algorithms

| Algorithm | Trigger | State Transition | Audit Event | Idempotency Guarantee | Status |
|---|---|---|---|---|---|
| Boot `SENDING` recovery | stuck `SENDING` at boot | `SENDING → ERROR_RETRYABLE`, **ZERO** re-sends | recovery audit | inbox key + no DPS dedup → no dup fiscalise | **WIRED** |
| Redrive classifier | `ERROR_RETRYABLE` doc | `Redrive` / `HoldProbeRequired` / `HoldIndeterminate` / `BudgetExhausted` / `EscalateManual` / `EscalateInconsistent` (`ErRedriveDecision`) | per-class audit | bounded budget then escalate | **WIRED** |
| RetryClass mapping | DPS error class | `TerminalReject` / `TransientRetry` / `FnConfigError` / `WrapperBug` / `ProbeRequired` / `MacRecovery` / `OperatorEscalation` | classified audit | — | **WIRED** |
| W9b backlog drain (doc-finalize) | rejoin / GOING_ONLINE | `OFFLINE_LOCAL_ACK → … → ACK` in **lnd ASC** (finalize Opened→None arm, backlog_drain.rs:2399–2418; advances MAC chain; NO shift transition) | drain audits | drain never skips a pending offline doc; one offline-no = one doc (INV-12) | **WIRED (doc-finalize only)** |
| Drain-reject escalation (shift safety) | drain TERMINAL-reject of backlog doc on `*_PENDING_DRAIN` shift | failing DOC → `REJECTED`; only the SHIFT → `REQUIRES_MANUAL_RECONCILIATION` (`escalate_drain_to_manual` transitions the shift only; edges 6/14). The doc-level → `REQUIRES_MANUAL_RECONCILIATION` edge applies to the DIFFERENT ER-budget-exhausted subtype (backlog_drain.rs:1552). | **Critical** `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` (backlog_drain.rs:2191, block 2138–2191) | FN drain halts; no silent re-fiscalise | **UNWIRED in production** — escalation guarded by `shift_in_pending_drain` (backlog_drain.rs:952); pending-drain `shift_state` never set in prod → :2155/:2191 UNREACHABLE. Code-wired + tested on test-seeded rows. The SAFETY is NON-FUNCTIONAL (silent absence, not a crash). See §1.11 Hard-Blocker list. |
| MAC reseed recovery | MAC chain desync | auto-fetch DPS anchor via probe doc (WebCheck pattern; `MacReseedRecovery` class) | reseed audit | — | Present (spec §16.3) |
| Code-pool exhaustion | empty FN code pool at `acquire_code_tx` | typed `CodePoolExhausted` raised (offline_sessions.rs:408); STOP_MODE caller-routing NOT wired | `CodePoolExhausted` typed | offline path frozen for FN | **typed error WIRED+tested; STOP_MODE caller-routing UNWIRED** (stage_offline_ack.rs:315 propagates `?`; distinct from drain Tier-2 `trigger_tier_2_stop_mode`, backlog_drain.rs:2074) |
| Return-online probe | OFFLINE periodic tick | `OFFLINE → GOING_ONLINE` (idempotent) | DpsError-class on fail | read-only; no fiscal mutation | **WIRED** (W8) |
| Operator recovery taxonomy (§16.3) | per class | `AutoOfflineFallback` / `TechSupportEscalation` / `KeyRotationPending` / `MacReseedRecovery` / `TechSupportRepair` | Critical on Manual landing (audit WIRED); forensic snapshot + ≤60s pager are spec-procedure only, **UNWIRED in code** (DF-4) | — | spec taxonomy (NOT a Rust enum); snapshot+pager UNWIRED |
| Ambiguous online SHIFT_OPEN/Z timeout → manual | edges 4 / 12 | (would route to `REQUIRES_MANUAL_RECONCILIATION`) | — | — | **UNWIRED** (edges unreachable; `shift_open_recovery` "proposed"/absent) |
| FN-deregistered-while-offline classifier | drain reject subtype | (subtype of edges 6/14) | — | — | **UNWIRED** (no dedicated classifier) |

**LND-rollback semantics (INV-02).** An aborted/rolled-back attempt **VOIDs**
the LND — the consumed local number is never reused (no gaps are filled by
re-issuing a number). Once a doc reaches `OFFLINE_LOCAL_ACK` it is a **durable
local commit**: rollback semantics no longer apply, so a later drain-reject does
not roll back — it routes to `REQUIRES_MANUAL_RECONCILIATION` (INV-13/14, edges
6/14).

---

## 1.10 Audit / Forensics Map

| Event | Severity | Emitted In Tx? | Survives Rollback | Operator Meaning | Status |
|---|---|---|---|---|---|
| `OFFLINE_LOCAL_ACK_APPLIED` | Info | post-commit (only on `Applied`) | n/a (after commit) | offline receipt durably issued | WIRED |
| `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED` | Warning | guard refuse | yes | online Z refused — drain first (INV-15); guard at stage_acquire.rs:782 (doc-comment types.rs:213) | WIRED |
| `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` | **Critical** | escalation path | yes | drain reject → manual recon; FN halted | WIRED |
| `CodePoolExhausted` (typed) | Critical | typed error | yes | offline path frozen; STOP_MODE caller-routing UNWIRED (distinct from drain Tier-2 `OFFLINE_DRAIN_FN_STOP_MODE`) | typed error WIRED+tested; STOP_MODE routing UNWIRED |
| boot `SENDING→ERROR_RETRYABLE` | Error | recovery | yes | timed-out send recovered, no re-send | WIRED |
| ingress reject | Warning/Error | ingress | yes | invalid payload — `audit_log` ONLY, never `fiscal_documents` | WIRED |
| Manual-recon landing | **Critical** | escalation | yes | "ЧП из ЧП"; the **Critical audit is WIRED** (backlog_drain.rs:2191 `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL`; force seam in shifts.rs). The **forensic snapshot + operator pager are UNWIRED** — no code evidence for a snapshot capture or a pager dispatch; operator-procedure only / deferred | **Critical audit WIRED; forensic snapshot + pager UNWIRED (no code — operator-procedure only / deferred)** |

Rule: failed DPS rejections + invalid ingress payloads go to `audit_log` only,
NOT to `fiscal_documents` (ledger = issued receipts only). Critical forensic
events must survive transaction rollback.

---

## 1.11 Known Deferrals + WIRED / UNWIRED Gap Table

> **This is the most operationally important table in the document.** Read no
> UNWIRED row as "done". All items must be tracked in `bd`, linked
> `discovered-from:<W4-Z4 epic>`.

> ## ⛔ PILOT GATE VERDICT: **NO-GO** (external review, code-verified)
>
> The gate's current honest verdict is **PILOT NO-GO**. Recorded here as the
> authoritative reference; mirrored in `PILOT_REVIEW_MATRIX §5` (exit gate) and
> `PILOT_REVIEW_PLAYBOOK` exit criteria.
>
> **Hard blockers (must clear or explicitly risk-accept before GO):**
> 1. **Shift lifecycle NON-FUNCTIONAL on HEAD — gateway cannot transact at all
>    today (CF-R3).** Prod bootstrap seeds `ShiftState::CLOSED`
>    (boot_phase.rs:1304), so online SELL is refused on `Closed` (`(Sell, Closed)
>    → ShiftNotOpen`, stage_acquire.rs:897); there are no online open/close
>    drivers; and offline is unreachable END-TO-END (no `Offline`/`GoingOffline`
>    mode setter; `OfflineSessionService::open_session` has zero prod callers;
>    `stage_offline_ack` needs `Opened` + an active session, stage_offline_ack.rs:268–318).
>    So no `OFFLINE_LOCAL_ACK` backlog ever forms and the offline Pattern C shift
>    SAFETY (pending-drain online-ops lockout §3.3 + drain-reject →
>    `RequiresManualReconciliation` escalation INV-19) is **silently absent** —
>    the pending-drain `shift_state` it keys on is never set, the drain
>    early-returns "no active session" / "empty backlog", and the Opened→None
>    finalize arm is itself unreachable. **Not a crash — a silent absence of the
>    safety machinery, which for a fiscal system is worse than a crash** (DF-1 /
>    CF-R3; evidence boot_phase.rs:1304 / stage_acquire.rs:897 /
>    backlog_drain.rs:952 / :2155 / :2399–2418).
> 2. **W4-Z3 native ATTACHED crypto unmerged + not externally reviewed.** HEAD's
>    in-process signer is **DETACHED** and **NOT live-DPS-accepted**; the
>    DPS-accepted ATTACHED CAdES-BES signer lives only on the unmerged
>    `feat/m4-w4-z3` branch (DF-2 / DF-3; §1.8).
> 3. **`PRRO_FISCAL_MODE` not harness-enforced** (DF-5). The W4-Z3 harness gates
>    only on `PRRO_LIVE_DPS=1` + host allowlist; a hard harness check for
>    `PRRO_FISCAL_MODE=TEST` is a required pilot fix (deferred to the W4-Z3
>    branch).
> 4. **INV-05 / INV-06 channel guards UNWIRED** — risk-accept only with an
>    operational channel-switch freeze.
> 5. **INV-09 / INV-10 offline limits UNWIRED + 24h continuous-SHIFT wall UNWIRED
>    (CF-R4)** — no production 36h-freeze, 168h-cap, OR 24h-shift-duration
>    enforcement (the 24h wall is a third distinct limit, LEGAL §8 item 1, whose
>    only compliant exit is an offline Z_REPORT local close (W10) — itself
>    UNWIRED); risk-accept only with offline descoped / operationally controlled.
>
> **Path to GO:** WL-1 **full** shift lifecycle (including offline
> `current_shift_id`, NOT online-only) **OR** an explicit offline descope; plus
> WL-3 MAC internal-advance; plus W4-Z3 merge **and** external review of the
> ATTACHED signer.

| Gap | What exists today | What is MISSING (the gap) | Invariant | Severity |
|---|---|---|---|---|
| **Shift lifecycle (online AND offline Pattern C safety)** | edges 3/4/8/10/11/12 in whitelist; drain DOC-FINALIZE WIRED | NO production driver for online `SHIFT_OPEN → OPENED` / online `Z_REPORT → CLOSED`; AND the offline Pattern C shift SAFETY (drain edges 5/6/7/9/13/14) is **UNWIRED in production** — keyed on a pending-drain `shift_state` prod never sets, so the lockout (§3.3) + drain-reject escalation (INV-19) are NON-FUNCTIONAL (silent absence, not a crash — DF-1). `shift_state` is statically seeded; `insert_created` (shifts.rs:119) + `current_shift_id` undriven | edges 3/8/10 + 5/6/7/9/13/14 (offline safety) | **Hard-Blocker (1); closed by WL-1 full lifecycle (incl. offline `current_shift_id`)** |
| **INV-04 9-state active-shift UNIQUE index** | runtime guard `(ShiftOpen,*active*)→ShiftAlreadyOpen` WIRED; non-unique `ix_shifts_fn_state` (migr 001:50, 016:196) | 9-state partial UNIQUE absent in Rust; only 3-state `uq_active_shift_per_fiscal` at `sql/001_hot_store_init.sql:158` (**dead Python contour, historical**) — 9-state index aspirational | INV-04 | Medium |
| **INV-05 channel-switch-with-open-shift guard** | frozen invariant #3 stated | NOT enforced in Rust | INV-05 | High (pilot decision) |
| **INV-06 failover-outside-shift** | documented | explicit GAP (`CHANNEL-FAILOVER-01`) | INV-06 | accepted gap |
| **INV-09 36h continuous-offline ingress freeze** | threshold defined in spec | no `offline_session_started_at` reader; no `OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED` | INV-09 | High (enforce or risk-accept) |
| **INV-10 168h monthly cap** | `current_month_offline_seconds` column exists | no enforcement reader (DPS does not return Server-11 in practice) | INV-10 | Medium |
| **24h continuous-SHIFT-duration wall (CF-R4)** | LEGAL_INVARIANTS.md §8 compliance-gate item 1 ("Active engineering risk") | a THIRD distinct UNWIRED limit, separate from INV-09 (36h continuous offline) and INV-10 (168h monthly): no 24h enforcement in `src` (grep `24*3600` / `86400` / `MAX_SHIFT` / `shift_duration` empty). Only compliant exit is an offline Z_REPORT local close (W10) — itself UNWIRED | LEGAL §8 item 1 | High (enforce or risk-accept) |
| **WebCheck 36h cert-expiry SHIFT_OPEN gate** | spec §16.10 | not implemented | INV-09 synergy (KeyRotationPending = INV-19 recovery class) | Medium |
| **Ambiguous online SHIFT_OPEN/Z timeout → manual** | edges 4/12 whitelisted | edges unreachable; `shift_open_recovery` "proposed"/absent | INV-19 | tied to online lifecycle |
| **FN-deregistered-while-offline classifier** | handled as generic drain-reject (edges 6/14) | no dedicated classifier | INV-19 | Low |
| **Auto-GO_OFFLINE on transport error** | manual `update_mode` works | automatic trigger = stub | INV-08 | Medium |
| **KVT2 final-confirm of offline backlog** | KVT2 decrypt WIRED | end-to-end offline-backlog → `ACK` closure is W12 (pilot-gating if pilot needs real DPS Ack of offline backlog) | INV-13/14 | pilot scope decision |
| **Native crypto DPS-verify** | native ATTACHED CAdES-BES signature is **branch-resolved** (W4-Z3 live cycle accepted by DPS); the fix (signing-cert selection + detached→attached) lives on the unmerged `feat/m4-w4-z3` branch | HEAD's in-process signer is still **DETACHED** and **NOT live-DPS-accepted**; merge + external review of the branch attached signer is the remaining work (see DF-3 / §1.8 and Hard-Blocker (2)) | INV-17 | **Hard-Blocker (2) — branch-resolved, HEAD-blocked** |

**WIRED baseline (regression-pinned, do NOT re-litigate):** 162-cell shift
guard (oracle test, all 162 cells); Pattern C `OFFLINE_LOCAL_ACK`
(SIGNED→OFFLINE_LOCAL_ACK transition at stage_offline_ack.rs:320;
`OFFLINE_LOCAL_ACK_APPLIED` audit at :350); code-pool exhaustion → typed `CodePoolExhausted`
(offline_sessions.rs:408; **STOP_MODE caller-routing UNWIRED — not part of this
baseline**); drain DOC-FINALIZE of the `OFFLINE_LOCAL_ACK` backlog to `Ack`
(`commit_finalize` Opened→None arm, backlog_drain.rs:2399–2418; advances the MAC
chain; NO shift transition); force/senior seams (shifts.rs:444 / :575 / :840 —
**primitive WIRED + regression-pinned, NO production driver / operator
entry-point today**).
>
> **NOT on this baseline (UNWIRED in production — the load-bearing correction):**
> the offline Pattern C SHIFT SAFETY semantics. The drain-reject of
> `OFFLINE_LOCAL_ACK` on a pending-drain shift → `RequiresManualReconciliation` +
> Critical `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` (backlog_drain.rs:2191, block
> 2138–2191) and the edge-5 drain-finalize "opens shift" transition, and the
> pending-drain online-ops lockout (§3.3), all KEY ON a pending-drain `shift_state`
> (`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`) that production NEVER
> sets. The escalation is guarded by `shift_in_pending_drain` (backlog_drain.rs:952)
> and `escalate_drain_to_manual` / `current_shift_id` (backlog_drain.rs:2155) is
> therefore UNREACHABLE in prod; `insert_created` (shifts.rs:119) is undriven and
> `node_state.current_shift_id` is never set (the WL-1 gap). The drain TRANSITION
> edges 5/6/7/9/13/14 are code-wired + tested but operate on **test-seeded shift
> rows only**. **CF-R3 — stronger end-to-end framing:** prod bootstrap seeds
> `ShiftState::CLOSED` (boot_phase.rs:1304), online SELL is refused on `Closed`
> (`(Sell, Closed) → ShiftNotOpen`, stage_acquire.rs:897), AND offline is
> unreachable end-to-end (no `Offline`/`GoingOffline` mode setter;
> `open_session` has zero prod callers; `stage_offline_ack` needs `Opened` + an
> active session) — so **no `OFFLINE_LOCAL_ACK` backlog ever forms**, `drain()`
> early-returns "no active session" / "empty backlog", and the Opened→None
> finalize arm is itself UNREACHABLE. In production the gateway therefore performs
> **no online transaction, no offline ACK, no drain finalize, no shift transition
> and no escalation** — the safety machinery is **NON-FUNCTIONAL: a silent
> ABSENCE, not a fail-stop crash, which for a fiscal system is worse than a
> crash.** This is Hard-Blocker (1) below.

> **W4-Z3 live cycle is NOT on this baseline (NOT on HEAD).** The full live WIRE
> cycle `SHIFT_OPEN → SELL → Z_REPORT` (`server_fiscal_no` `1g41M3jDt-Q` /
> `AOBSkplfIUU` / `L2AMnY2MkmA`, **proven 2026-05-29**) plus its harness
> `rust/prro/tests/live_dps_extended_smoke.rs`, the `live-dps` Cargo feature,
> and the binding static-gate `cargo test -p prro --features live-dps --test
> live_dps_extended_smoke --no-run` live ONLY on the **UNMERGED** branch
> `feat/m4-w4-z3-dps-extended-smoke`. They are **PROVEN on that branch / PENDING
> MERGE to rust-gateway — NOT present on HEAD**. The live harness that EXISTS on
> HEAD is `live_smoke_w12_hardening.rs` (`--features test-support`,
> connect/probe-only, dummy signing, NO CMS). The `server_fiscal_no` values
> above are **branch-proven / pending-merge**, not HEAD-WIRED.

### Static gate commands (Rust-only)

```bash
cargo fmt --check
cargo clippy -p prro --features test-support --tests -- -D warnings
cargo clippy -p prro_crypto --all-targets -- -D warnings
cargo build -p prro --tests --features test-support
cargo test  -p prro --features test-support
# live-DPS COMPILE-ONLY static gate — NOT runnable on rust-gateway HEAD today;
# the harness + `live-dps` feature live ONLY on the UNMERGED branch
# feat/m4-w4-z3-dps-extended-smoke (PENDING MERGE). Becomes runnable on HEAD
# only after that branch merges:
cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run
# On HEAD today the only live harness is (connect/probe-only, dummy sign, no CMS):
cargo test -p prro --features test-support --test live_smoke_w12_hardening --no-run
```

---

## 3. Tax Mapping Invariants

> Preserved verbatim from the prior draft (load-bearing — the Crypto Immutable
> Rule is referenced throughout §1.2 / §1.8).

- **tax_id preservation**: Physical POS `tax_id` settings must not be overwritten or mutated by driver-side metadata. The `driver_tax_mapping` module executes *before* pinning inputs.
- **zero-amount rate edge cases**: Zero-amount lines (e.g. 100% discount) must still emit correctly formatted tax groups to avoid DPS rejection.
- **Crypto Immutable Rule**: After the 'Pin Signing Inputs' step, no XML adjustments (including tax or formatting) are permitted. Re-formatting breaks the CMS signature.
