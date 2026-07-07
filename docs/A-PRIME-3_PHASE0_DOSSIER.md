# A′.3 — Offline Reachability — Phase-0 Dossier + LOCKED PR-O1 Contract

**Base:** `main @ 2f6a5fa` (#235 PR-Z2 live-Z dispatch merged; online-half PILOT GATE open).
**Branch:** `feat/aprime3-offline-enable` (worktree `.claude/worktrees/aprime3-o1`).
**Goal:** make INV-08 legal offline fallback *reachable from prod* + close the offline-drill half of the PILOT GATE.
**Discipline:** strict RED-first + teeth. Architect merges. Any deviation = STOP.

---

## 1. Recon verdict (Phase-0 fan-out, 9 agents; load-bearing facts re-verified first-hand)

| Dim | Finding | prod-status |
|---|---|---|
| **O-0a** mode-seam | 6 prod mode-setters exist (blocked/stop_mode/probe/drain/admin-reset) but **no operator command to initiate `ONLINE→GOING_OFFLINE`**. | missing |
| **O-0b** offline codes | **No prod code-provisioning**: DPS channel (7 methods) fetches no codes; `open_session`/`seed_code_range` have **zero prod callers** (all in `tests/`); `min/max_offline_codes` hardcoded 0. | missing → **STOP-O1** |
| **O-0c** limits #5/INV-08 | 168h server-enforced (`ERROR_OFFLINE_168`→BLOCKED); code-exhaust client-side (`CodePoolExhausted`); no client time-check (server-authoritative). New path routes through both via `stage_offline_ack` + drain `stage_send`. | partial |
| **O-0d** 8 shift edges | **2/7/9 whitelisted (`shifts.rs:73-92`) but NOT wired**; **5/13** drain-finalize wired; **6/14** escalate→RMR wired. No whitelist extension / migration needed. | partial |
| **O-0e** convergence | probe→GO_ONLINE→drain→finalize→converge code exists BUT **gated behind `supervisor.enabled` (default false)** — inert until enabled. | see §4 O2 |
| **O-0f** open_session gap | `open_session` defined, **zero prod callers**; OFFLINE+SELL → `stage_offline_ack` → `NoActiveSession` refuse. | missing |

**O-0e correction (verified):** `supervisor.rs:3` — *"`Cmd::Serve` calls `run` ONLY when `config.supervisor.enabled` (default false)"*. The `spawn_*_loop` calls are unconditional **inside** `run()`, but `run()` is gated. Recon O-0e's "unconditionally prod-wired" was wrong; drain/probe/converge are **inert by default**.

**First-hand verifications (main@2f6a5fa):**
- Whitelist `shifts.rs:73-92 allowed_transition` explicitly matches edges 2 `(Created,OpenedLocalPendingDrain)`, 7 `(OpenedLocalPendingDrain,ClosingLocalPendingDrain)`, 9 `(Opened,ClosingLocalPendingDrain)` — **no extension needed**.
- `set_mode_blocked_tx` (`node_state.rs:204-210`) = pure `UPDATE node_state SET mode … WHERE fiscal_number` — touches **only `mode`**, never `shift_state` ⇒ GO_OFFLINE by this shape leaves `shift_state=Opened` intact (Frozen #3 safe).
- `stage_offline_ack` requires `shift_state ∈ {Opened, OpenedLocalPendingDrain}` (Step 3) **and** an active OPEN session (Step 4 → `NoActiveSession`). ⇒ offline SELL/RETURN works **without** driving edge 2 if the shift is pre-opened ONLINE.
- **No node-mode `allowed_transition`** exists (only shift edges are whitelisted) ⇒ mode setters are guarded CAS `UPDATE`s, **no DDL**.
- Flag pattern (`z_builder.rs:55`): `pub const FULL_Z_SURFACE_READY: bool = true` + `ZSurfaceNotReady` + gate fn + coupling-pin + tripwire → mirror target for the offline door.

---

## 2. Rulings (architect-adjudicated, LOCKED)

**STOP-O1 = (b) operator manual-seed + named follow-up (a).** Conditions:
1. Runbook caveat MUST state: seed **only real DPS-issued ranges** for this FN (from cabinet / prior provisioning), never invented numbers — invented codes make drain send to DPS, DPS rejects, cascade of RMR escalations on drain. The command is a **pilot-drill affordance, not a permanent mechanism**.
2. `prro fn seed-codes` modeled on `admin.rs` (pre-read → CAS-guard → Critical audit with range in payload); range validation (positive/ordered; non-overlap with existing pool).
3. Named follow-up (a) **co-scoped with the live-campaign**: first live test-DPS contact captures the ask-codes contract → a transport-PR.

**Slicing = amended O0/O1/O2 + ship-together (approved).** Additional locked conditions:
1. **Flag semantics:** `FULL_OFFLINE_SURFACE_READY` gates **the door** — the admin `GO_OFFLINE`/`GO_ONLINE` commands (mirror `ensure_full_z_surface_ready`). O1 proves machinery by **direct seam-calls** (mode-set + `open_session` + offline SELL/RETURN via `inline::run` in the offline mode-family); the door gets a **gated-pin** (`GO_OFFLINE` at flag=false → typed refusal). The live door + flip land in **O2** (Z2 lesson "flip in the same change" applied preemptively).
2. **Flip in O2** with a coupling-pin (Z2 precedent): `=true` valid only when the drain path is reachable — *"opening the door without the drain re-opens the stranded-backlog hazard."*
3. **Drain-enablement hypothesis (verify in O2 design):** since prod-binding lives in `supervisor::run` and the pilot must run `supervisor.enabled=true` (else no ingress), the drain/probe loops come free with it ⇒ "drain-enablement" is likely **pilot-config + e2e-via-tick, not new code**. Build synchronous drain-on-`GO_ONLINE` only if the tick-path proves insufficient (minimal diff wins).
4. **O1-e2e honestly ends** at OLA-resting docs + OPEN session (legal durable states; `assert_clean` MUST be clean). Drain is **not** O1 scope.

**Migrations = any DDL → STOP.** `min/max_offline_codes=0` is config-data (not DDL); if O1 changes `fn_config` defaults, **name it in the report** (not STOP-class).

---

## 3. LOCKED PR-O1 CONTRACT — mode-seam + open_session + offline SELL/RETURN reachability (flag OFF)

### 3.1 Scope fence
**IN:** operator mode-seam (`GO_OFFLINE`/`GO_ONLINE`, gated), atomic session-open on GO_OFFLINE, manual code-seed admin command + runbook, offline SELL/RETURN reachability e2e via direct seams, gated-pin + tripwire.
**OUT (→ O2):** offline shift edges 2/7/9 (W10a/W10b), drain-enablement, `FULL_OFFLINE_SURFACE_READY` flip + coupling-pin, full drill, combined PILOT GATE.
**Flag stays `false` in O1. No migration. Online-preopen keeps edge 2 out of O1.**

### 3.2 Build list (implementer implements test-first to green the RED pins)
- **B1 — node-mode setters** (`db/repositories/node_state.rs`): `set_mode_going_offline_tx` (`ONLINE→GOING_OFFLINE`, guarded CAS `WHERE mode='ONLINE'`, rows==1 race-loud) and `set_mode_going_online_tx` (`OFFLINE|GOING_OFFLINE→GOING_ONLINE`). Mode-only; never touch `shift_state`. `GOING_OFFLINE` is the operator-initiated transitional mode (dispatch routes `Offline|GoingOffline`→`stage_offline_ack`; `return_online_probe` leaves `GoingOffline` alone → stable resting mode for the drill).
- **B2 — offline-surface gate** (new small module, e.g. `services/offline_sync/offline_surface.rs`, mirroring `z_builder`): `pub const FULL_OFFLINE_SURFACE_READY: bool = false;` + `struct OfflineSurfaceNotReady` typed error + `ensure_full_offline_surface_ready() -> Result<(), OfflineSurfaceNotReady>`. Coupling-pin lands in O2 with the flip; O1 ships the tripwire.
- **B3 — admin commands** (`admin.rs`, mirror `reset_stop_mode`): `go_offline(pool, fn, reason)` and `go_online(pool, fn, reason)`. Shape: validate reason → **gate `ensure_full_offline_surface_ready()`** → pre-read mode (actionable error if not the expected mode) → atomic `with_immediate` envelope { guarded mode-CAS + [GO_OFFLINE: open session, see B4] + Critical audit `ADMIN_GO_OFFLINE`/`ADMIN_GO_ONLINE` with mode_before/after payload }. New `AdminError` variants: `OfflineSurfaceNotReady`, `NotInExpectedMode { expected, observed }`.
- **B4 — atomic session-open on GO_OFFLINE**: wire `OfflineSessionService`/`open_session` so an OPEN session exists **within/adjacent to** the GO_OFFLINE envelope — **no Offline-but-no-session window**. Prefer a tx-bound `open_session` primitive inside the same `with_immediate` (extract one if only the service-level own-tx variant exists, mirroring how `seed_code_range` wraps `seed_code_range_tx`).
- **B5 — manual code-seed** (`admin.rs` + `main.rs` `AdminCmd`): `seed_codes(pool, fn, first_lnd, last_lnd, reason)` wrapping `seed_code_range_tx`, with pre-validation (positive/ordered range → else `InvalidRange`; overlap with existing pool → `RangeOverlapsExistingPool`, loud reject even though the primitive is `INSERT OR IGNORE`) + Critical audit `ADMIN_SEED_OFFLINE_CODES` with range in payload. CLI: `AdminCmd::SeedOfflineCodes`, `AdminCmd::GoOffline`, `AdminCmd::GoOnline`.
- **B6 — runbook** (`docs/operations/…`): the STOP-O1 (b) caveat (only real DPS-issued ranges; pilot-drill affordance; invented codes → RMR cascade on drain).
- **B7 — O1 e2e** (extend `tests/pilot_online_half_e2e.rs` substrate): the reachability drill below.

### 3.3 RED pins (spec anchors — implementer writes each test-first, RED → GREEN)
Reachability core:
- **RP-O1-1** GO_OFFLINE on ONLINE node → `mode=GOING_OFFLINE` (guarded CAS); `shift_state` unchanged; Critical audit `ADMIN_GO_OFFLINE` (before/after). Not-ONLINE → typed `NotInExpectedMode`.
- **RP-O1-2** After GO_OFFLINE, `current_active_session_id_tx` returns `Some` (OPEN session exists) — no Offline-but-no-session window.
- **RP-O1-3** Offline SELL via `inline::run` (mode ∈ offline-family, shift `Opened`, OPEN session, seeded codes) → reaches `OFFLINE_LOCAL_ACK`, consumes exactly one code (`offline_codes` row stamped `consumed_by_document_id`). Not refused Na/Shift/CodePool.
- **RP-O1-4** Offline RETURN (`DocType::Return`, Step-3 allowlist) → `OFFLINE_LOCAL_ACK`, one code consumed.
- **RP-O1-5** (decoupling pin) e2e opens shift ONLINE (edge 3 → `Opened`) **before** GO_OFFLINE; asserts `shift_state == Opened` throughout (edge 2 **never** driven) — proves O1 doesn't touch offline shift-open.

Code provisioning (STOP-O1 (b)):
- **RP-O1-6** `seed-codes` populates `offline_codes` (count == range size); Critical audit with range; non-positive/inverted → `InvalidRange`; overlap → `RangeOverlapsExistingPool`.
- **RP-O1-7** Codes seeded by the admin command are exactly those `acquire_code_tx` consumes in RP-O1-3 (ties seed → consume).

Door gating (machinery proven by direct calls; flip deferred to O2):
- **RP-O1-8** (gated-pin) with `FULL_OFFLINE_SURFACE_READY == false`, `go_offline`/`go_online` return typed `OfflineSurfaceNotReady`; mode NOT flipped.
- **RP-O1-9** (tripwire) `offline_door_gated_until_full_offline_surface` asserts the flag is deliberately `false` in O1, AND that direct-seam machinery (RP-O1-3/4) works **independently of the flag** (proven via `inline::run`, not the gated command).

GO_ONLINE defined (mode-only in O1):
- **RP-O1-10** GO_ONLINE on `OFFLINE|GOING_OFFLINE` → `GOING_ONLINE` guarded CAS + Critical audit. Convergence (drain→ONLINE) explicitly NOT asserted (inert until O2). Also behind the flag (RP-O1-8).

Invariants:
- **RP-O1-11** (Frozen #3) GO_OFFLINE changes **only mode**, not channel; an open shift is held across the flip without tripping channel-switch-forbidden. Assert channel unchanged.
- **RP-O1-12** (assert_clean) after offline SELL/RETURN the FN rests at `OFFLINE_LOCAL_ACK` + OPEN session → `invariant_scan::assert_clean` CLEAN (no non-terminal PREPARED/SIGNED/ENCRYPTED; OLA is legal durable). Drain not in scope.

### 3.4 Teeth protocol (mandatory)
- **T-reach:** revert the B1 mode-setter wiring (or B4 session-open) → RP-O1-3/4 must go RED (`NotInExpectedMode`-family / `NoActiveSession`). Proves reachability pins bite.
- **T-door:** flip `FULL_OFFLINE_SURFACE_READY = true` → RP-O1-8 gated-pin must go RED (door opens). Confirms the gate is real. (Keep it `false`.)
- **T-decouple:** stub the online SHIFT_OPEN preopen in the e2e → RP-O1-3 must go RED with `ShiftNotOpened` (proves the shift precondition is genuine, not incidental).

### 3.5 Invariant posture (must be affirmed in the delivery report)
- **#5 + INV-08 (central):** offline docs route through `stage_offline_ack` (code-check) + drain `stage_send` (server 168h) — both limits on the path; no bypass (dispatch hard-routes Offline→`stage_offline_ack`). Preserved.
- **#2:** GO_OFFLINE/open_session are per-`fiscal_number` CAS, single-writer. Preserved.
- **#3:** GO_OFFLINE = MODE flip, not channel switch (verified setter shape); drill holds an open shift across the flip. Not weakened.
- **#8:** no state-machine edges touched in O1 (edges 2/7/9 are O2); mode CAS is guarded + audited. Preserved.
- **#9:** no supervisor changes in O1 (O2). Unchanged.
- **D2 (offline-arm):** offline SELL consumes an lnd/code but never advances the online seed (advance-at-SEND is online `Sending→Sent`; offline goes `OFFLINE_LOCAL_ACK`). Preserved.
- **Config-knobs:** `GO_OFFLINE`/`GO_ONLINE` = operator runtime **commands**, not behavior toggles; `FULL_OFFLINE_SURFACE_READY` = ship-gate (Z2 precedent), not a behavior knob. Zero new behavior config-knobs.
- **Migrations:** none (whitelist has edges; `offline_codes`/`node_state`/`offline_sessions` exist). If any DDL surfaces → STOP.

### 3.6 Delivery gate (PR-O1)
Adversarial lenses (offline-invariants/limits · state-machine · break-it · diff-hygiene) + full `nextest -p prro --features test-support` 0-failed (whole fuzzer green, untouched) + `fmt` + `clippy --all-targets -D warnings` + 7-point report with RED-outcomes and teeth protocol.

---

## 4. Forward scope (PR-O2 — not locked yet; design-dossier to architect first)
- Wire offline shift edges **2/7/9** (W10a offline SHIFT_OPEN `Created→OpenedLocalPendingDrain`; W10b offline Z_REPORT `Opened/OpenedLocalPendingDrain→ClosingLocalPendingDrain`) via `shift_transition` sole-writer CAS (whitelist already permits).
- **Drain-enablement:** verify the "free with `supervisor.enabled=true`" hypothesis (§2 cond. 3) before writing any synchronous drain trigger.
- **Flip** `FULL_OFFLINE_SURFACE_READY=true` + coupling-pin ("no door without drain") + full drill e2e (GO_OFFLINE→offline sells→GO_ONLINE→drain→converge + `assert_clean`) + combined PILOT GATE. Ship-together: **one flag**, do not flip until O2 lands.

## 5. Named follow-up (a) — DPS offline code-fetch
Co-scoped with the live-campaign. First live test-DPS contact captures the real ask-codes/reserve-range contract → a transport-PR adding a DPS code-fetch method + prod provisioning, retiring the manual-seed affordance for production.

**Live-campaign prerequisites (operator → implementer, by combined PILOT GATE close):** JKS key + password + test-FN.
