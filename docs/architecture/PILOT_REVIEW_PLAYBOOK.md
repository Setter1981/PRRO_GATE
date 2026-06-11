# PILOT REVIEW PLAYBOOK — W4-Z4 Pilot GATE

> **Status: this is a GATE, not a checklist.** No pilot deployment is authorized
> until every exit criterion in §9 is satisfied with recorded evidence. A reviewer
> who cannot produce the evidence template (§7) for a finding has not reviewed it.
>
> **⛔ CURRENT VERDICT: PILOT NO-GO** (external review, code-verified). The Hard-Blocker
> list and path-to-GO are in §9. Top blockers: shift lifecycle non-functional on HEAD
> (DF-1), W4-Z3 native ATTACHED crypto unmerged / HEAD signer detached & not
> live-accepted (DF-2/DF-3), `PRRO_FISCAL_MODE` not harness-enforced (DF-5), INV-05/06
> channel guards UNWIRED, INV-09/10 offline limits UNWIRED.
>
> **Stack reality.** The pilot path is **Rust-only**. The crate under review is
> `rust/prro` (+ `rust/prro_crypto`). The Python tree `src/prro_gateway/` is a
> **dead reference** — it is not the pilot artifact, it is not gated here, and its
> behavior must **never** be assumed as parity for the Rust path. There is no
> `pytest` / `ruff` gate.
>
> **Honesty rule (the central discipline of this gate).** Every fiscal/state-machine
> guard reviewed here is tagged **WIRED** (a production driver exercises it and a
> regression-pin test locks it) or **UNWIRED** (whitelisted / gap-marked / xfail,
> with **no production caller today**). A guard being present in an `enum`, a
> transition table, or a 4-year-old Python file is **not** evidence that it fires on
> the Rust pilot path. Treat every "obviously this is enforced" assumption as a
> finding until a WIRED test proves it. Do **not** carry Python-era parity forward.

---

## 1. Scope, vocabulary, and ground truth

This playbook reviews the live Rust enum vocabulary. Earlier drafts used invented
state names (`COMPLETE`, `REQ_RCVD`, `INPUTS_PINNED`, `ENVELOPED`, `TRANSMITTING`,
"7 idempotency keys"). Those are **fabrications**. Reject any finding written in that
vocabulary and re-state it against the real model below
(`rust/prro/src/db/models/enums.rs`).

**`DocState`:** `PREPARED, SIGNED, ENCRYPTED, SENDING, SENT, KVT1, KVT2, ACK,
OFFLINE_LOCAL_ACK, REJECTED, CANCELLED, ERROR_RETRYABLE,
REQUIRES_MANUAL_RECONCILIATION`. Terminals: `ACK` / `REJECTED` / `ERROR_*`.

**`ShiftState` (9):** `CREATED, OPENING, OPENED_LOCAL_PENDING_DRAIN, OPENED,
CLOSING_LOCAL_PENDING_DRAIN, CLOSING, CLOSED, REQUIRES_MANUAL_RECONCILIATION,
ERROR`.

**`OfflineSessionState`:** `OPENING, OPEN, DRAINING, CLOSED, ABORTED`.
*No drift on the pilot path:* the live Rust DB column is named `state` (not `status`),
with `CHECK (state IN ('OPENING','OPEN','DRAINING','CLOSED','ABORTED'))` per migration
`rust/prro/migrations/015_offline_normalize.sql:140` (CHECK source; value set
mirrored in enum `enums.rs:54`). The repo `offline_sessions.rs:225` is an `UPDATE`
that *uses* the `state` column but does **not** hold the CHECK constraint — it is
repo-uses-`state` evidence only, not the CHECK source. The `status` column /
`CLOSING` value are the **DEAD pre-015 shape** (migration `004` / Python
`sql/001_hot_store_init.sql`); migration `015` normalized
`status`/`CLOSING` → `state`/`DRAINING`. A reviewer who finds a query keying on
`status` or asserting `CLOSING` files it (Medium, §2.1) as a stale dead-Python
pattern — querying `state` / asserting `DRAINING` is **correct**.

**`NodeMode`:** `ONLINE, GOING_OFFLINE, OFFLINE, GOING_ONLINE, BLOCKED, STOP_MODE,
CRYPTO_DEGRADED`.

**`InboxStatus`:** `NEW, PROCESSING, DONE, REJECTED, ERROR`.

**Recovery typing (Rust ids):** `RetryClass = { TerminalReject, TransientRetry,
FnConfigError, WrapperBug, ProbeRequired, MacRecovery, OperatorEscalation }`;
`ErRedriveDecision = { Redrive, BudgetExhausted, EscalateManual,
EscalateInconsistent, HoldProbeRequired, HoldIndeterminate }`.

**Operator recovery taxonomy (spec §16.3 — operator vocabulary, NOT Rust ids):**
`AutoOfflineFallback, TechSupportEscalation, KeyRotationPending, MacReseedRecovery,
TechSupportRepair`. Do not conflate these labels with the `RetryClass` ids above.

**Write-path stages:** `stage_acquire, stage_sign, stage_send, stage_finalize,
stage_offline_ack` (+ `dispatch`, `signer_guard`, `mac_recovery`, `error_routing`).

**Idempotency — the single real surface.** There is exactly **one** local
idempotency key: column `ingress_inbox.idempotency_key`, enforced by the composite
`UNIQUE (fiscal_number, idempotency_key)` index `ux_inbox_fn_idem`
(`rust/prro/migrations/002_fiscal_documents.sql:91`). The canonical
payload hash (`runtime/ingress/dto.rs:304-317`) is a content fingerprint and is
**NOT** an idempotency key — a finding that claims it dedupes submissions is wrong.
The **DPS idempotency surface** is server-side (`local_number` / `server_fiscal_no`)
and is a separate axis. (Note: `lnd` is the **persisted column name**; `local_number`
is the **wire-level name** for the same value.) (Enforces INV-07.)

---

## 2. Severity taxonomy and pilot blocker bands

### 2.1 Five-level severity (with fiscal examples)

| Level | Meaning | Fiscal example |
|---|---|---|
| **Critical** | Silent fiscal divergence, duplicate fiscalization, state-machine/data corruption, secret disclosure, or wrong production / live-DPS target. | A network partition after `stage_send` causes `stage_finalize` to re-run `send_chk_v2` and the same receipt is fiscalized twice (INV-07 breach). Or a `tracing::debug!` prints the decrypted KVT2 container. |
| **High** | Realistic race or state corruption, lost critical forensic event, wrong CMS/wire profile, write-path panic on realistic malformed input/date, or uncontrolled SQLite contention. | `stage_sign` reformats the canonical XML after the hash is pinned, so DPS returns `CryptBadSign`. Or two workers acquire the same `fiscal_number` lease and both advance `DocState`. |
| **Medium** | Risky missing coverage, parser/date edge, recovery ambiguity, operator runbook gap, or a stale dead-Python pattern (e.g. a query keying `OfflineSessionState` on `status` or asserting `CLOSING` instead of the live `state`/`DRAINING`, §1). | No regression test pins `(ZReport, OpenedLocalPendingDrain) → ZReportBlockedBacklogDrainPending` (INV-15) so a refactor could silently allow a Z over an undrained backlog. |
| **Low** | Naming, docs, or local cleanup with no behavioral risk. | A stale comment referencing the dead Python `status`/`CLOSING` offline shape. |
| **Info** | Accepted debt or future hardening, tracked in `bd`. | INV-09 36h continuous-offline ingress freeze is UNWIRED — risk-acceptable **ONLY** with explicit `bd` pilot sign-off **AND** offline disabled / operationally controlled. No production 36h-freeze (INV-09) or 168h-cap (INV-10) enforcement exists (§4). |

### 2.2 Pilot-Blocker vs Non-Blocker (explicit)

**Pilot-Blocker — P0 (Critical) / P1 (High). Pilot is refused until CLOSED:**

- duplicate-fiscalization risk on retry / resume / replay (INV-07)
- wrong CMS / wire profile causing `ERROR_VERIFY` / `CryptBadSign`
- crypto, network, or filesystem I/O inside a SQLite write transaction (INV-18)
- unbounded write transaction (long CPU or any blocking call inside `with_immediate`)
- raw write-path panic on realistic malformed date / input
- state-machine corruption (illegal `DocState` / `ShiftState` / `OfflineSessionState`
  transition reachable from a production driver)
- raw `SQLITE_BUSY` leaking as undefined business behavior (§2.4)
- lost **Critical** forensic audit event (e.g. the Manual-recon snapshot, INV-19)
- unsafe live-DPS host handling / production endpoint reachable from smoke
- unsafe secret handling (§2.3)
- test vs production environment ambiguity. **NOTE (CORRECTED):** `PRRO_FISCAL_MODE`
  is **NOT harness-enforced** — the W4-Z3 harness does not define or check this env var;
  it gates only on `PRRO_LIVE_DPS=1` + the host allowlist (the local DB is seeded
  test-mode internally). Treating `PRRO_FISCAL_MODE=TEST` as an enforced guard is a
  manual operator preflight only. A hard harness check for `PRRO_FISCAL_MODE=TEST` is a
  **required pilot fix** (deferred to the W4-Z3 branch) and is a Hard-Blocker (§9, DF-5).
- **a guard the pilot operationally depends on is UNWIRED** — this is P1 by default
  and may only be downgraded by an explicit, `bd`-recorded operator acceptance with
  a stated compensating control (§4).

**Non-Blocker — P3 / P4. May ship to pilot, must be tracked in `bd`:**

- naming debt, stale comments without behavioral risk
- performance tuning without correctness impact
- parser hardening for a **non-pilot** path, if tracked
- extra observability nice-to-haves
- documentation polish

> **P2 / "degraded":** there is no comfortable middle. A P2 must be reclassified at
> review close: either it is promoted to P1 (Pilot-Blocker) or explicitly accepted
> as Non-Blocker via `bd defer <id>` with a named owner. An un-triaged P2 at the
> gate is itself a gate failure.

---

## 3. The four invariant-path review focuses (VERBATIM rules preserved)

These three rules are **load-bearing and quoted verbatim** from the prior playbook;
do not paraphrase them in findings:

> **`with_immediate` no-I/O rule:** Verify `with_immediate` transactions contain NO
> network, filesystem, or crypto I/O.

> **UTC-internal rule:** Confirm all internal timestamps and SQLite timestamps are
> stored as strict UTC. Verify Kyiv local time projection is ONLY applied at the
> final render step for physical receipts.

> **No-reformat-after-sign rule:** Confirm XML is NEVER modified or re-formatted
> after the payload hash has been pinned for signature.

### 3.1 Shift Lifecycle Guards

- **WIRED:** the read-only **162-cell** shift guard `check_shift_guard`
  (`rust/prro/src/services/write_path/stage_acquire.rs:845`; called at
  `:383`), pinned by oracle test `check_shift_guard_matches_oracle_for_all_162_cells`
  (9 `DocType` × 9 `ShiftState` × 2 channels). Verify these cells specifically:
  - `(ShiftOpen, Closed) → allow`
  - `(ShiftOpen, *any active state*) → ShiftAlreadyOpen`
  - `(Sell, Closed) → ShiftNotOpen` (this is **correct**, enforces INV-03)
  - `(ZReport, OpenedLocalPendingDrain) → ZReportBlockedBacklogDrainPending`
    (enforces INV-15)
  - `(Sell, OpenedLocalPendingDrain, Online) → ShiftOpenPendingDrainOpRefused`
  - NodeMode pre-guards: `GoingOnline / Blocked / StopMode / CryptoDegraded` refuse.
- **UNWIRED IN PRODUCTION — flag P1, Hard-Blocker (CORRECTED re-tag):** the drain
  **TRANSITION** + **manual-escalation** edges `5, 6, 7, 9, 13, 14`
  (`rust/prro/src/db/.../shifts.rs:67`; `backlog_drain.rs:2169` / `:2498`, prod caller
  `app.rs:620`). An earlier draft tagged these **WIRED (test-seeded rows only)**; an
  external reviewer tagged them "always crashes at backlog_drain.rs:2155". **BOTH are
  wrong.** The VERIFIED reality: the drain's shift-transition + manual-escalation logic
  is keyed on a **PENDING-DRAIN** `shift_state`
  (`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`) that **production NEVER
  SETS** — offline shift-creation edge 2 is UNWIRED, `stage_offline_ack` only **READS**
  `shift_state` (never sets it), and `node_state.current_shift_id` is never set in prod.
  Concretely: `escalate_drain_to_manual` (the `current_shift_id` check / the reviewer's
  "crash" at `backlog_drain.rs:2155`) is reached **only** `if shift_in_pending_drain`
  (`backlog_drain.rs:952`) → **UNREACHABLE in prod**. In `commit_finalize`
  (`backlog_drain.rs:2399-2418`): `Opened → None` (no transition, **no crash**);
  `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain` → needs `current_shift_id`
  (never reached); any **other** `shift_state` → `BootError::Internal`. **CF-R3 (corrects
  a recurring premise): the gateway CANNOT transact at all in prod today.** The ONLY
  production `upsert_initial` seeds `ShiftState::CLOSED`
  (`boot_phase.rs:1304`: `upsert_initial(pool, fn, NodeMode::Online, ShiftState::Closed, 1)`),
  NOT `Opened`; orphan-boot resolution only drives toward `CLOSED`
  (`boot_phase.rs:1491`), and the only `OPENED` write in `src` is a `#[cfg(test)]`
  fixture (`admin.rs:903`). Under `CLOSED`, `(Sell, Closed) → ShiftNotOpen` **refuses**
  online SELL (`stage_acquire.rs:897`) on either channel. **Offline is unreachable
  end-to-end:** `node_state` has **no** `Offline`/`GoingOffline` mode setter (only
  `set_mode_blocked_tx` / `set_mode_stop_mode_tx`); `OfflineSessionService::open_session`
  has **zero** production callers; and `stage_offline_ack` requires `Opened` + an active
  offline session (`stage_offline_ack.rs:268-318`). So the mode never flips `Offline`,
  no offline session opens, **no `OFFLINE_LOCAL_ACK` doc forms, no backlog exists**, and
  `drain()` early-returns "no active session" / "empty backlog" — the `Opened → None`
  finalize arm is itself **unreachable in prod**. The earlier "`shift_state` statically
  seeded `Opened` so SELLs are admitted → drain finalizes the backlog to `Ack` without
  escalation" mechanism is **WRONG and is replaced by this stronger end-to-end-unreachable
  framing**: that scenario does not occur because no backlog ever forms. Consequence: the
  offline Pattern C shift **SAFETY** semantics (pending-drain online-ops lockout per §3.3,
  drain-reject → `RequiresManualReconciliation` escalation INV-19) are **NON-FUNCTIONAL**,
  and the gateway is **silently non-functional** (online SELL refused on `Closed`; offline
  path unreachable end-to-end). This is **NOT a crash (fail-stop)** — it is a **silent
  absence of the safety machinery, which for a fiscal system is worse than a crash**, and
  it **STRENGTHENS the NO-GO**. Evidence: `boot_phase.rs:1304` (seeds `CLOSED`), `:1491`
  (orphan → `CLOSED`), `admin.rs:903` (`OPENED` only under `cfg(test)`),
  `stage_acquire.rs:897` (`(Sell, Closed) → ShiftNotOpen`),
  `stage_offline_ack.rs:268-318` (requires `Opened` + active session),
  `backlog_drain.rs:952` (escalate guarded by `shift_in_pending_drain`),
  `:2399-2418` (finalize match: `Opened → None` / pending-drain → `current_shift_id` /
  other → `Internal`). This is a Hard-Blocker / pilot-NO-GO item (§9).
- **UNWIRED — flag P1 unless accepted (shift CREATION):** edges `1` and `2` — the
  shift-**creation** step has **no production driver**. Edge 2
  (`CREATED → OPENED_LOCAL_PENDING_DRAIN`, offline `SHIFT_OPEN` ingress, Pattern C)
  is **NOT** wired despite an earlier WIRED tag: `shifts::insert_created`
  (`shifts.rs:119`) has **zero production callers** (only `tests/repo_shifts.rs`),
  the only `INSERT INTO shifts` in `src` is under `#[cfg(test)]`
  (`backlog_drain.rs:2953`, `cfg(test)` opens at `:2753`), `stage_offline_ack` only
  **READS** `ns.shift_state` to **GUARD** (`:268-289`) and never creates/transitions
  a shift, and `node_state.current_shift_id` is never set in production. Edge 2 is
  therefore the **same UNWIRED class as the online edges 3/8/10**. The drain
  TRANSITION edges above are real but only transition pre-existing rows — which
  production never creates. (Ties to the **WL-1 shift-lifecycle gap**: the `shifts`
  table is not production-populated today.)
- **UNWIRED — flag P1 unless accepted:** the **online** shift lifecycle drivers.
  Edges `3 (Opening→Opened), 4, 8 (Opened→Closing), 10 (Closing→Closed), 11, 12`
  are whitelisted in the transition table but have **no production caller**. W4-Z3
  confirmed `node_state.shift_state` never opens online on the pilot path. A finding
  that assumes "online SHIFT_OPEN drives `Opening→Opened`" is wrong — that path is
  not driven today. (Enforces INV-03, INV-04 — partially; see §4.)

### 3.2 Channel-Pinning (frozen invariant #3)

- **UNWIRED — flag P1.** Frozen invariant #3 ("channel switch is forbidden with an
  open shift", INV-05) and INV-06 ("failover only outside a shift") are **not
  enforced in the Rust path**. INV-06 is an explicit, recorded gap
  (`CHANNEL-FAILOVER-01`). A reviewer must confirm there is no code path that
  switches DPS channel / backend profile while a shift is in any active `ShiftState`
  and must file the absence of the guard, not the presence of a bug. Note INV-05
  governs the **DPS-side** channel pinned to the shift, not which ingress shell
  accepted the POS message. (Enforces INV-05, INV-06.)

### 3.3 Offline Limits & Drain

- **WIRED:** Pattern C local-ack and drain mechanics —
  - `stage_offline_ack` (`stage_offline_ack.rs:165` fn-entry) lands
    `DocState::OFFLINE_LOCAL_ACK` (transition `:327`) and emits
    `OFFLINE_LOCAL_ACK_APPLIED` (audit `:350`) (enforces INV-12, INV-13, INV-14);
  - code-pool exhaustion → typed `CodePoolExhausted` is **WIRED + tested**
    (`offline_sessions.rs:408`; test `offline_session_code_pool.rs:201`)
    (enforces INV-11);
  - drain preserves order and a drain-reject of an `OFFLINE_LOCAL_ACK` backlog doc on
    a pending-drain state escalates (see §3.4).
- **UNWIRED (the `STOP_MODE` half):** `CodePoolExhausted → caller enters STOP_MODE` has
  **no production handler**. `stage_offline_ack.rs:315` propagates the error via `?`
  ("caller's responsibility to enter STOP_MODE"); no production caller converts it to
  `STOP_MODE`. The **only** `STOP_MODE` driver is the **drain Tier-2** trigger
  `trigger_tier_2_stop_mode` (`backlog_drain.rs:2074`, fires at
  `consecutive_holds >= 50`, audit `OFFLINE_DRAIN_FN_STOP_MODE`) — a **distinct**
  trigger, NOT the code-pool path. (CF-R6: the `consecutive_holds >= 50` predicate also
  requires the `HeldAtSent` / `HeldAtKvt1` projection co-condition, `backlog_drain.rs:931-937`.)
- **UNWIRED — flag P1 unless accepted:**
  - **INV-09 (≤36h continuous offline):** no ingress freeze. There is no
    `offline_session_started_at`-driven check and no
    `OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED` audit. Ingress keeps accepting past 36h.
  - **INV-10 (≤168h / calendar month):** column `current_month_offline_seconds`
    exists but **no enforcement reader** consumes it.
  - **WebCheck 36h cert-expiry SHIFT_OPEN gate** (spec §16.10): not wired.
  - A finding here must say "limit is not enforced" (a gap), not "limit is wrong".

### 3.4 Manual-Reconciliation

- **UNWIRED IN PRODUCTION — flag P1, Hard-Blocker (CORRECTED re-tag, CF-R1):** the
  primary Manual-recon surface — any W9b drain-reject of an `OFFLINE_LOCAL_ACK`
  backlog doc on `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain` →
  `DocState::REQUIRES_MANUAL_RECONCILIATION` + `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL`
  **Critical** audit (`offline_sync/backlog_drain.rs:2191`, escalate block
  `:2138-2191`) is **code-present + test-pinned** (`backlog_drain.rs:2191`) but
  **UNREACHABLE in production**, consistent with DF-1 / §3.1 / the §4 UNWIRED ledger.
  An earlier draft (and fix3) tagged this flat **WIRED**; that is wrong. The escalation
  is reached only `if shift_in_pending_drain` (`backlog_drain.rs:952`) — keyed on a
  **pending-drain** `shift_state` (`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`)
  that **production never sets** (see §3.1, DF-1). FN-deregistered-while-offline is the
  observed real-world subtype, but no production path can form the
  `OFFLINE_LOCAL_ACK` backlog on a pending-drain shift that would trigger it (see CF-R3
  / §3.1: prod bootstrap is `CLOSED`, offline is unreachable end-to-end, so no backlog
  forms). Only the doc-**finalize**-to-`Ack` arm is (narrowly) wired — and even that is
  **moot in prod** because no backlog ever forms (CF-R3). Verify the **Critical audit**
  actually emits and survives (INV-19) — but note the **forensic snapshot + operator
  pager are UNWIRED per DF-4**: there is no snapshot-capture or pager code on HEAD, only
  the Critical audit emits. This is a Hard-Blocker / pilot-NO-GO item; see the RUNBOOK
  §4.9 hard-blocker list (DF-1 cross-reference) which must match MATRIX §5 / §9.
- **TESTED-BUT-UNDRIVEN — flag P1 unless accepted:** the force / senior seams
  `force_to_error_with_audit` (`shifts.rs:444`),
  `force_to_manual_reconciliation_with_audit` (`shifts.rs:575`) and
  `senior_cashier_close_shift_with_audit` (`shifts.rs:840`) each carry a
  regression-pin test but have **NO production driver / operator entry-point today**
  (no admin CLI / runtime path invokes them; drain uses `shifts::transition_state`
  directly) — mirror the W8-probe caveat. Manual-recon trigger family (3), the
  "operator force / senior seam", is therefore **not operator-reachable on the pilot
  path today**.
- **UNWIRED — flag P1 unless accepted:** the **ambiguous online timeout** family.
  Manual-recon trigger family (2) — ambiguous wire timeout on online `SHIFT_OPEN`
  (edge 4) or online `Z_REPORT` (edge 12) — is **unreachable** because edges 4/12 are
  themselves unwired (§3.1); `shift_open_recovery.rs` is "proposed"/absent. The
  FN-deregistered-while-offline **classifier** (vs the generic drain-halt) is also
  not implemented. (Enforces INV-19.)

> Reviewer discipline: Manual-recon is "ЧП из ЧП" (4 years UA production: zero
> observed). Bias every recovery finding toward `HoldRetry` / `AutoOfflineFallback`
> over `EscalateManual`. Every Manual landing you do find must carry the Critical
> audit + forensic snapshot + ≤60s out-of-band pager, or it is a P0. **(CF-R2: the
> snapshot + pager are UNWIRED today — DF-4: no forensic-snapshot-capture or
> operator-pager code exists on HEAD, only the Critical audit emits. This line stands
> as an aspirational acceptance criterion, not a statement that they fire today.)**

---

## 4. The WIRED / UNWIRED ledger (carry into the go/no-go)

This ledger is the single most important reviewer artifact. Every row must be
revisited at the gate.

**WIRED (regression-pin test exists — verify the test, not just the code):**

- 162-cell shift guard (§3.1)
- Pattern C `OFFLINE_LOCAL_ACK` (`stage_offline_ack.rs:165` fn-entry; transition `:327`,
  audit `OFFLINE_LOCAL_ACK_APPLIED` `:350` — CF-R6)
- code-pool exhaustion → typed `CodePoolExhausted` WIRED+tested
  (`offline_sessions.rs:408`; test `offline_session_code_pool.rs:201`). **NOTE the
  `→ STOP_MODE` half is UNWIRED** (see UNWIRED ledger / §3.3): no production caller
  routes `CodePoolExhausted` to `STOP_MODE`.
- drain **doc-finalize** of `OFFLINE_LOCAL_ACK` backlog → `Ack` (advances the MAC
  chain), prod caller `app.rs:620` — **the only (narrowly) wired part, and MOOT in
  prod (CF-R3):** the production `upsert_initial` seeds `ShiftState::CLOSED`
  (`boot_phase.rs:1304`), not `Opened`; `(Sell, Closed) → ShiftNotOpen` refuses online
  SELL (`stage_acquire.rs:897`), and the offline path is unreachable end-to-end (no
  `Offline`/`GoingOffline` mode setter, no `OfflineSessionService::open_session`
  production caller, `stage_offline_ack` requires `Opened` + an active session
  `:268-318`). So **no `OFFLINE_LOCAL_ACK` backlog ever forms** and `drain()`
  early-returns "no active session" / "empty backlog" — the `Opened → None` finalize
  arm is itself unreachable in prod (CF-R3 supersedes the earlier "seeded `Opened`"
  premise). The drain-reject → `REQUIRES_MANUAL_RECONCILIATION` +
  `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL` Critical escalation and the drain
  **TRANSITION** edges `5, 6, 7, 9, 13, 14` are **UNWIRED IN PRODUCTION** (CORRECTED
  re-tag, CF-R1 — moved to the UNWIRED ledger below / §3.1 / §3.4): they key on
  pending-drain `shift_state` values production never sets, so the Pattern C shift
  safety machinery is silently absent. Hard-Blocker (§9).
- drain Tier-2 `STOP_MODE` (`trigger_tier_2_stop_mode`, `backlog_drain.rs:2074`,
  `consecutive_holds >= 50`, audit `OFFLINE_DRAIN_FN_STOP_MODE`) — the only wired
  `STOP_MODE` driver

**PROVEN ON BRANCH / PENDING MERGE (NOT on `rust-gateway` HEAD — do not count as WIRED at the gate):**

- native crypto + full live WIRE cycle `SHIFT_OPEN → SELL → Z_REPORT` was **PROVEN on
  branch `feat/m4-w4-z3-dps-extended-smoke`** (`server_fiscal_no` `1g41M3jDt-Q` /
  `AOBSkplfIUU` / `L2AMnY2MkmA`, proven 2026-05-29) but is **PENDING MERGE to
  `rust-gateway`**. The harness `rust/prro/tests/live_dps_extended_smoke.rs`, the
  `live-dps` Cargo feature, and that cycle **do NOT exist on `rust-gateway` HEAD**
  (where these caps live). The binding live-dps static-gate command (§10) is
  **not runnable on `rust-gateway`** until that branch merges. The live harness that
  **exists on HEAD** is `live_smoke_w12_hardening.rs` (`--features test-support`,
  connect/probe-only, dummy signing, no CMS).

**UNWIRED (gap-marker / xfail; NO production driver today — each is P1 by default):**

- shift **CREATION** (edges 1/2) — `shifts::insert_created` (`shifts.rs:119`) has
  **zero production callers** (only `tests/repo_shifts.rs`); the only `INSERT INTO
  shifts` in `src` is `#[cfg(test)]` (`backlog_drain.rs:2953`, `cfg(test)` at `:2753`).
  Edge 2 (`CREATED → OPENED_LOCAL_PENDING_DRAIN`, offline `SHIFT_OPEN`, Pattern C) is
  UNWIRED — same class as the online edges below (corrects an earlier WIRED tag). The
  drain TRANSITION edges only move pre-existing rows; production never creates one.
  (WL-1 shift-lifecycle gap: `shifts` table not production-populated today.)
- drain **TRANSITION + escalation** edges 5/6/7/9/13/14 (CORRECTED re-tag, §3.1) —
  keyed on pending-drain `shift_state` (`OpenedLocalPendingDrain` /
  `ClosingLocalPendingDrain`) that production never sets; `escalate_drain_to_manual`
  is reached only `if shift_in_pending_drain` (`backlog_drain.rs:952`) → unreachable in
  prod, and `commit_finalize` runs `Opened → None` (`:2399-2418`) with **no transition,
  no escalation, no crash**. The drain's doc-finalize works (advances the MAC chain) but
  the offline Pattern C shift safety semantics (§3.3 pending-drain lockout; INV-19 drain-
  reject → `RequiresManualReconciliation`) are **silently NON-FUNCTIONAL** — worse than a
  crash for a fiscal system. Hard-Blocker / pilot-NO-GO (§9).
- online shift lifecycle drivers (edges 3/4/8/10/11/12; online `SHIFT_OPEN→Opened`,
  online `Z→Closed` not driven; W4-Z3 confirmed `shift_state` never opens online)
- force / senior seams `force_to_error_with_audit` (`shifts.rs:444`),
  `force_to_manual_reconciliation_with_audit` (`shifts.rs:575`),
  `senior_cashier_close_shift_with_audit` (`shifts.rs:840`) — primitives WIRED +
  regression-pinned, but **NO production driver / operator entry-point today**
  (test-only; drain uses `shifts::transition_state` directly). Manual-recon family (3)
  "operator force / senior seam" is **not operator-reachable on the pilot path today**.
- `CodePoolExhausted → STOP_MODE` caller-routing — typed error WIRED+tested but the
  `STOP_MODE` half has no production handler (`stage_offline_ack.rs:315` propagates via
  `?`; distinct from the wired drain Tier-2 `STOP_MODE`, §3.3)
- active-shift partial-UNIQUE index — only the **dead Python**
  `sql/001_hot_store_init.sql:158` (historical) 3-state index exists; Rust has only
  non-unique `ix_shifts_fn_state`. The
  INV-04 9-state unique index is **aspirational**.
- INV-09 36h continuous-offline ingress freeze (no `offline_session_started_at`
  check / no `OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED`)
- INV-10 168h monthly cap (`current_month_offline_seconds` present, no reader)
- WebCheck 36h cert-expiry `SHIFT_OPEN` gate (spec §16.10)
- INV-05 channel-switch-with-open-shift guard (frozen invariant #3 — not in Rust)
- INV-06 failover-outside-shift (explicit GAP `CHANNEL-FAILOVER-01`)
- ambiguous online `SHIFT_OPEN` / `Z` timeout → manual (edges 4/12 unreachable,
  `shift_open_recovery.rs` proposed/absent)
- FN-deregistered-while-offline classifier

> **Downgrade rule.** An UNWIRED row may be accepted for pilot **only** with a
> `bd`-recorded operator acceptance naming (a) the compensating control and (b) the
> pilot-scope reason. Pilot ships FSCO/ZZD only; EVPZ is a future per-FN profile
> slot. Several offline-limit gaps are acceptable **iff** the pilot operating
> envelope keeps sessions well under the limits and tech-support paging is live.

---

## 5. Sensitive-data hygiene review (§2.3)

This is a **P0** review. Any path that can emit a secret is Critical.

**NEVER logged / traced / audited (any level, including ERROR):**

- the JKS / P12 password
- the `param_d` private scalar (DSTU 4145 private key component)
- decrypted private-container bytes
- decrypted inbound XML / decrypted KVT2 `EnvelopedData` payload bytes
- any `SignerKey` / `P12Pass` / `PrivateKey` value — concretely: **no
  `tracing::debug!` (or any level) on `SignerKey`, `P12Pass`, or `PrivateKey`
  types**; these must not implement a value-revealing `Debug` / `Display`.

**Allowed (non-secret identifiers):**

- certificate fingerprint, SKI (subject key identifier)
- hashes / digests
- public certificate metadata (subject, validity window, serial)
- truncated / opaque IDs

A reviewer greps the crate for `Debug`/`Display`/format calls touching these types
and for raw-XML logging on the inbound decrypt path, and files each hit as P0.

---

## 6. SQLite concurrency / `SQLITE_BUSY` review (§2.4)

Verify, with evidence:

- the SQLite pool sets `PRAGMA busy_timeout` (a bounded wait, not 0)
- `journal_mode = WAL`
- `synchronous = NORMAL`
- write transactions use `BEGIN IMMEDIATE` (`with_immediate`) so contention surfaces
  at `BEGIN`, not mid-statement
- **raw `SQLITE_BUSY` must not leak** as an undefined business outcome — contention
  must resolve to a bounded wait, a typed retry, a Noop, or a controlled typed
  failure. A `?`-propagated raw busy error reaching a caller as an untyped 500 is
  P1.
- the `with_immediate` no-I/O rule (§3) holds for every envelope
- concurrency tests cover same-FN contention and reader/writer overlap

(Enforces INV-01, INV-18.)

---

## 7. Required-evidence template (every finding)

A finding without all eight fields is **not accepted** into the gate and cannot be
counted toward closure.

```
ID:              <bd id, linked discovered-from:<W4-Z4-epic>>
Severity:        Critical | High | Medium | Low | Info
Blocker band:    P0 | P1 | P3 | P4
File / line:     rust/prro/src/...:NNN
Invariant(s):    INV-NN[, INV-NN...]  (or "no invariant — quality only")
WIRED / UNWIRED: which, and the pinning test name (or "no test — that is the finding")
Repro / path:    concrete execution path or reproduction steps
Suggested fix:   minimal-diff proposal
Expected test:   the regression-pin test that must exist after the fix
```

Findings are logged only in `bd`, linked `discovered-from:<W4-Z4-epic-id>`.

---

## 8. Review rounds A–E and the chaos round

Run as adversarial passes with fresh eyes per round. Fiscal/hot-zone worklets need
3–5 rounds; CRITICAL findings often surface in round 2–3, not round 1. Convergence =
two reviewers in the same round both reach MERGE with only Info findings.

- **Round A — fiscal / state-machine correctness.** Walk `DocState`, the 9-state
  `ShiftState`, `OfflineSessionState`, `NodeMode`, and `InboxStatus`. Confirm every
  WIRED transition has a pinning test and every UNWIRED transition is ledgered (§4).
  The 162-cell guard (§3.1), Channel-pinning (§3.2), Manual-recon (§3.4).
- **Round B — SQLite / concurrency / recovery.** §6, the `with_immediate` map,
  boot recovery, resume, orphan transport-trace closure, lease single-writer (INV-01).
- **Round C — crypto / date / ASN.1 / XML.** The no-reformat-after-sign rule (§3),
  the UTC-internal rule (§3), CP1251 canonical XML bytes, DER `SET OF` lexicographic
  sorting, `signingTime`, `SigningCertificateV2`, UTCTime/GeneralizedTime 2049/2050
  cliff, DST repeated-hour. **Crypto wording (correct it in findings):**
  - **Outbound to DPS = a CMS *signature*, not encryption.** The outbound envelope is
    **CMS-detached/attached SignedData over CP1251 canonical XML**
    (`crypto/provider.rs` `sign_cms_detached:50`, over `SignCmsRequest.canonical_xml`
    field `:33`). The algorithms are **DSTU 4145-2002 (PB-257) *signature*** + **GOST
    34.311 / DSTU 7564 (Kupyna) *hash***. **DSTU 4145 is a signature scheme, NOT
    encryption.** Any finding that says "encrypt outbound with DSTU 4145" is
    mis-stated and must be rewritten.
  - **Encryption is inbound only.** Decryption happens on the inbound KVT2 path:
    `unwrap_envelope` decrypts the DPS `EnvelopedData` (`provider.rs:79`).
  - W4-Z3's proven live cycle was **signed-only** (attached CMS `SignedData`,
    `sendChkV2` accepted): `PREPARED → SIGNED → SENT`.
- **Round D — DPS / live-ops / security.** §5 secret hygiene, live-DPS host safety,
  `PRRO_FISCAL_MODE=TEST` preflight (**NOT harness-enforced** — manual operator
  preflight only; the harness gates on `PRRO_LIVE_DPS=1` + host allowlist, see DF-5 /
  §9), rate-limit / cooldown behavior, INV-17 production guard (passthrough/mock refused
  in prod — note this is itself a GAP).
- **Round E — tests / coverage.** Confirm the static gate (§10) is green, that every
  WIRED row in §4 maps to a named test, and that UNWIRED rows carry an xfail / gap
  marker so a future driver cannot land silently unguarded.

### 8.1 Chaos / fault-injection round

Automated or semi-automated:

- process kill between state transitions
- network loss during `send_chk_v2`
- concurrent workers on the same DB / same FN
- malformed timestamps; corrupt snapshot / cert metadata
- replay the same request (idempotency, §1)
- fail at the pin / sign / send boundaries

Manual lab: ENOSPC; VM/process kill during a SQLite write; WAL recovery inspection.

Acceptance: no state corruption, no duplicate fiscal document, no raw uncontrolled
`SQLITE_BUSY`, no stuck intermediate `DocState` without a named recovery owner.

---

## 9. Exit criteria (the GATE)

> ## ⛔ CURRENT VERDICT: **PILOT NO-GO** (external review, code-verified)
>
> The gate's current honest verdict is **PILOT NO-GO**. The exit criteria below are
> **NOT** met on `rust-gateway` HEAD. The following **Hard Blockers** must each be
> closed (or carry an explicit `bd`-recorded operator acceptance with a stated
> compensating control where noted) before pilot is authorized:
>
> 1. **Shift lifecycle NON-FUNCTIONAL on HEAD (DF-1).** Prod bootstrap seeds
>    `ShiftState::CLOSED` (`boot_phase.rs:1304`), NOT `Opened` (CF-R3): the only `OPENED`
>    write in `src` is `#[cfg(test)]` (`admin.rs:903`), and there are no online
>    open/close drivers and no offline shift-creation. Under `CLOSED` the gateway
>    **cannot transact at all** — `(Sell, Closed) → ShiftNotOpen` refuses online SELL
>    (`stage_acquire.rs:897`), and the offline path is **unreachable end-to-end** (no
>    `Offline`/`GoingOffline` mode setter, no `OfflineSessionService::open_session`
>    production caller, `stage_offline_ack` requires `Opened` + active session). So **no
>    `OFFLINE_LOCAL_ACK` backlog ever forms** — the offline Pattern C shift **safety**
>    machinery (pending-drain online-ops lockout §3.3, drain-reject →
>    `RequiresManualReconciliation` escalation INV-19) is **silently absent**, and the
>    `Opened → None` doc-finalize arm is itself unreachable (no backlog to drain). The
>    earlier "seeded `Opened` so SELLs are admitted; drain finalizes backlog to `Ack`"
>    framing is **superseded** by this stronger end-to-end-unreachable reality (drain
>    edges 5/6/7/9/13/14 key on pending-drain states production never sets). Worse than a
>    crash for a fiscal system; this STRENGTHENS the NO-GO.
> 2. **W4-Z3 native ATTACHED crypto unmerged + not externally reviewed (DF-2/DF-3).**
>    `rust-gateway` HEAD's in-process signer is **DETACHED** CMS (no `eContent`) and is
>    **NOT live-DPS-accepted**. The ATTACHED CAdES-BES signer DPS actually accepted lives
>    **only** on the unmerged `feat/m4-w4-z3` branch, pending merge + external review.
> 3. **`PRRO_FISCAL_MODE` not harness-enforced (DF-5).** Test/prod separation is a manual
>    operator preflight only; a hard harness check for `PRRO_FISCAL_MODE=TEST` is a
>    required pilot fix (deferred to the W4-Z3 branch).
> 4. **INV-05 / INV-06 channel guards UNWIRED (§3.2).** Channel-switch-with-open-shift and
>    failover-outside-shift are not enforced in the Rust path — risk-accept **only** with
>    an explicit operations freeze (no channel/backend switch during pilot).
> 5. **INV-09 / INV-10 offline limits UNWIRED (§3.3, DF-6).** No production 36h-freeze or
>    168h-cap enforcement — risk-accept **only** with offline descoped / operationally
>    controlled.
>
> **Path to GO:** **WL-1 full shift lifecycle** (including offline `current_shift_id` —
> **NOT online-only**) **OR** an explicit offline descope; **+ WL-3 MAC internal-advance;
> + W4-Z3 merge & external review.** Until then, every "0 open P0/P1" claim below is
> unsatisfied.

Pilot is authorized only when **all** of the following hold with recorded evidence:

1. **0 open Critical / High (0 open P0 / P1).**
2. Every Medium is fixed or explicitly accepted in `bd` with a named owner.
3. Every Low / Info is tracked in `bd`.
4. **The WIRED / UNWIRED ledger (§4) is reconciled:** each UNWIRED row is either
   wired or carries a `bd`-recorded operator acceptance with a compensating control.
5. `ALGORITHMIC_MAP.md` is current and matches code (real vocabulary, no fabrications).
6. The pilot test matrix is green; the static gate (§10) passes.
7. The live-DPS smoke runbook is ready; the emergency off-switch is documented.
8. Secrets policy (§5) verified by grep evidence.
9. Test vs production separation verified (host allowlist + `PRRO_LIVE_DPS=1` gate).
   **NOTE (DF-5):** `PRRO_FISCAL_MODE=TEST` is currently a **manual operator preflight,
   NOT harness-enforced** (the W4-Z3 harness defines/checks no such var; the local DB is
   seeded test-mode internally). A hard harness check for `PRRO_FISCAL_MODE=TEST` is a
   required pilot fix (deferred to the W4-Z3 branch) and a Hard-Blocker until landed.
10. The W4-Z3 live-DPS path is reproducible — **PENDING MERGE**: proven on branch
    `feat/m4-w4-z3-dps-extended-smoke`, not yet on `rust-gateway` HEAD; this criterion
    is satisfied only once that branch merges and the §10 live-dps gate is runnable.

This is the gate contract: make the gateway **operationally** pilot-ready, with every
guard honestly tagged WIRED or UNWIRED — not merely implementation-complete.

---

## 10. Static gate (Rust-only — the exact commands)

```bash
cargo fmt --check
cargo clippy -p prro --features test-support --tests -- -D warnings
cargo clippy -p prro_crypto --all-targets -- -D warnings
cargo build -p prro --tests --features test-support
cargo test  -p prro --features test-support
# live-DPS: COMPILE-ONLY, must NOT execute in CI.
# PENDING MERGE: the live-dps feature + live_dps_extended_smoke.rs harness live ONLY on
# branch feat/m4-w4-z3-dps-extended-smoke and are NOT present on rust-gateway HEAD, so
# the command below is NOT runnable on rust-gateway until that branch merges.
cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run
```

Do **not** use `--all-features`. There is no Python gate (`src/prro_gateway/` is dead
reference). `cargo test -p prro` is the pilot scope; a full-workspace test run is for
the pre-merge final only.
