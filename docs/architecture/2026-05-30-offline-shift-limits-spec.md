# Offline & Shift Limits — enforcement spec (HB5)

> **⚠️ SUPERSEDED IN PART (2026-07-10) by `docs/RULINGS_2026-07-10_SHIFT_T112_AUTOZ.md` RULING 3 + implemented as task T3.**
> Where this spec and RULING 3 conflict, **RULING 3 governs**. Specifically:
> - **The shift-limit auto-Z is now UNCONDITIONAL** (RULING 3.4). Operator decision #2 below (per-FN "*whether* it auto-closes") is **withdrawn**: a shift never crosses the 24h wall without a durable Z attempt/outcome (success, or an escalated failure → RMR — never silent continuation), regardless of any toggle. The per-FN `shift_autoclose_enabled` key is **deprecated: parsed-but-ignored** with a one-time boot deprecation audit (`CONFIG_KEY_DEPRECATED_IGNORED`).
> - **Three document-derived budgets** (24h shift / 36h session / 168h month) are computed from durable rows against **one injected clock**; **tracking is always on**, **enforcement is per-budget toggleable** (default ON), the **legal close path is never blocked**. See `services/time_budget.rs` + the `run_staged` admission gate + the supervisor `auto-z` ticker.
> The "*at what time*" part of decision #2 (a configurable close hour distinct from the 24h legal wall) is **not implemented** and is out of T3 scope.

**Date:** 2026-05-30 · **Branch:** `rust-gateway` · **Status:** design spec — enforce ALL limits, operator-decided · no code yet
**Inputs:** WebCheck decompile (`docs/webcheck_reverse`), PRRODPS C# source (`/mnt/d/prrodps_src`), ФСКО protocol
(`docs/dps_protocol/251051_(1).md`), `LEGAL_INVARIANTS.md` §8 / INV-09 / INV-10. Two 7-agent reference sweeps.

> **Operator decisions (2026-05-30), this spec is their resolution:**
> 1. **168h = LOCAL accumulator** (gateway computes + resets locally; server `-11` is a defense-in-depth hard block, not the primary).
> 2. **Shift close = PER-FN configurable** — both *whether* it auto-closes and *at what time* (auto-Z per operator practice, but settings live per-FN, not global as in PRRODPS).
> 3. **W10 (offline Z local close) = IN pilot scope** (offline is mandatory; an offline shift must be able to close without DPS).

> **Headless principle (operator clarification 2026-05-30):** the gateway does **not** communicate with the operator
> directly. The WebCheck/PRRODPS operator dialogs/warnings (60s auto-close countdown, "X хв залишилось", the
> "advisability of closing the shift" prompt) are **UI features, N/A here**. The gateway surfaces enforcement two ways
> only: (a) **typed refusal / error codes** returned to the ingress/POS (which surfaces them to the cashier), and
> (b) **audit events** (+ out-of-band pager via monitoring). No synchronous operator prompts. → confirms fork 2 is
> **binary** (per-FN `auto-Z` **or** 24h backstop) — **no "warn-only" third mode**.

---

## §1 — Reference comparison (what each product does)

| Limit | WebCheck | PRRODPS | ФСКО protocol | **Rust decision** |
|-------|----------|---------|---------------|-------------------|
| **36h continuous offline** | `min(2160, 10080−OfflineTime)` min; block ops (err42); 9-min margin (`All.cs:281-289`) | `FromHours(36)` on `now−DtBegOfflineMode`; independent of 168h; UI-lock (`ViewModelBaseWithOfflineOnlineLockPanel.cs:329`) | per-session, zeroes on online return (`251051:49,51`) | **freeze ingress** at `max−margin`; per-session; zeroes on return |
| **168h cumulative monthly** | LOCAL `OfflineTime` per-FN ini (`All.cs:283`) | **server** `OfflineSessionsMonthlyDuration` + local delta; no local reset (`...LockPanel.cs:316,327`) | server-authoritative, reported in X-report (`251051:86`) | **LOCAL accumulator** (op decision 1) + local 1st-of-month reset; `-11` → Blocked too |
| **Shift max** | hard **24h** (1440 min), block-no-autoclose, NOT configurable (`All.cs:211`) | **end-of-opening-day auto-Z** @ configurable 23:54 global (`ShiftDurationChecker.cs:38-49,113-115`); skipped offline | inform if >24h OR before first check of next day (`251051:39`) | **per-FN auto-Z @ per-FN time** (op decision 2) **+ 24h hard backstop**; offline → W10 |
| **Return-online** | FormTimer drain | 10s probe, **2-consecutive** + 1-2min backoff | ping every ~1 min | keep Rust single-probe + W9b drain (deliberate; NOT 2-consecutive) |
| **Enforcement layer** | client/UI | client/UI (WPF VM) | server-authoritative | **write-path (hot zone)** — gateway is the authority, no UI |

**Polarity/unit trap (both products):** all thresholds in **MINUTES**. WebCheck `KeyShiftTimeINN` returns `true=GOOD`.
The two offline caps interacted in WebCheck (`continuous = min(36h, 168h−used)`) — **we keep them independent** per
PRRODPS (simpler, and the 168h breach blocks anyway).

---

## §2 — The three limits, exact Rust behavior

### 2.1 — 36h continuous offline (INV-09)
- **Source of time:** `offline_sessions.opened_at` (`offline_session.rs:76`, schema `015:141`).
- **Check:** `now − opened_at ≥ (offline_continuous_max_minutes − offline_safety_margin_minutes)`.
- **Action:** **freeze ingress** for the FN — refuse all NEW docs with `OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED`
  (Critical audit + operator pager, per INV-09). Existing `OFFLINE_LOCAL_ACK` docs are preserved and drain on return.
  This is a freeze, **not** manual-recon. Node mode → `Blocked` (`set_mode_blocked_tx`, `node_state.rs:177`).
- **Pre-lock signal:** at `remaining ≤ offline_prelock_warning_minutes` (10) emit a one-shot **Warning audit** (telemetry for ops dashboards / pager — **not** an operator dialog).
- **Reset:** zeroes naturally — a new offline session has a fresh `opened_at`.

### 2.2 — 168h cumulative monthly offline (INV-10) — **LOCAL accumulator**
- **State (wire the dead column):** `node_state.current_month_offline_seconds` (exists `001:69`, **unused** today) +
  a new `node_state.offline_accum_month` TEXT (`'YYYY-MM'`).
- **Accumulate:** on every offline-time update (probe tick + offline-session close), add the session delta.
- **Local monthly reset:** when the current calendar month ≠ `offline_accum_month` → reset accumulator to the current
  session's in-month elapsed and stamp the new month. This handles a session spanning the month boundary (the protocol's
  *BLOCKED→OFFLINE at midnight of the 1st, 168h zeroes, 36h continues* — `251051:55`): at rollover, zero 168h, keep 36h.
- **Check:** `current_month_offline_seconds ≥ (cumulative_monthly_max_minutes·60 − margin)` → node mode **Blocked**
  (no settlement ops). Critical audit + out-of-band pager **before** Blocked; the protocol's "shift-close advisory"
  (`251051:53`) becomes an **audit event**, not an operator dialog.
- **Defense-in-depth:** server `-11 ERROR_OFFLINE_168` still routes to Blocked (already wired, `error_routing.rs:455-466`).
  Local accumulator is primary; `-11` is the backstop if the server ever does enforce.

### 2.3 — Shift max-duration — **per-FN, two layers**
- **Layer A (operator practice — primary):** per-FN **auto-Z at a per-FN configured local time** (default 23:54).
  - A runtime **shift-duration ticker** (new spawned task, supervisor/RS-4 sibling; analogue of PRRODPS
    `ShiftDurationChecker`, 1-min tick) checks, per FN with an open shift: if `now.time ≥ (autoclose_hour:autoclose_minute)`
    AND `now.date == shift.opened_at.date` (same opening day) AND `shift_autoclose_enabled` AND mode Online AND shift Opened
    → **emit an automatic Z_REPORT** (real close) **directly at the cutoff** (no 60s operator dialog — headless); once-per-day guard.
  - `now.date != shift.opened_at.date` (survived past midnight) → penalty window: auto-close **disabled**, fall through to Layer B.
  - **Offline at cutoff:** auto-Z over the wire is impossible → use **W10 offline-Z local close** (in scope) so the shift
    still closes; if W10 path is blocked (e.g. code pool exhausted) → `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED` Critical.
- **Layer B (legal backstop):** `now − shifts.opened_at ≥ shift_max_continuous_minutes` (1440 = 24h) → **block new
  sale-flavour ops** at `stage_acquire` (Z_REPORT/close still allowed so the shift can exit). This is the WebCheck-style
  hard wall for the case the same-day auto-close didn't fire (e.g. shift opened late, ran >24h across the day boundary).
- **Source of time:** `shifts.opened_at` (`001:39`, currently never read for duration).

### 2.4 — Cert-expiry 36h gate at SHIFT_OPEN (WebCheck `LimitCertificate`)
- At `stage_acquire` for `SHIFT_OPEN`: refuse if `cert.NotAfter − now < cert_expiry_gate_minutes` (2160 = 36h) — or
  deferred-key auto-swap if a successor key exists (WebCheck `ClassFiscal.cs:381` errCode 66). Mathematically pairs with
  the 36h offline cap so a cert can't expire mid-offline (`251051:83`). PRRODPS has no equivalent; WebCheck does.

---

## §3 — Configuration surface (all clamp+audit via the existing `clamped_*` pattern, `config/mod.rs:139-149`)

**Global (legal constants — same for all FN; in `OfflineCfg`/new `LimitsCfg`, `config/mod.rs`):**

| Key | Default | Min/Max clamp | Gates |
|-----|---------|---------------|-------|
| `offline_continuous_max_minutes` | 2160 (36h) | TBD bounds | 36h freeze |
| `offline_cumulative_monthly_max_minutes` | 10080 (168h) | TBD | 168h Blocked |
| `offline_safety_margin_minutes` | 9 | 0..60 | enforce early |
| `offline_prelock_warning_minutes` | 10 | 0..120 | pre-lock warning |
| `cert_expiry_gate_minutes` | 2160 (36h) | TBD | SHIFT_OPEN cert gate |
| `shift_max_continuous_minutes` | 1440 (24h) | TBD | 24h legal backstop (Layer B) |

**Per-FN (operational — in `fiscal_number_config`, new migration; co-located with `max/min_offline_codes`):**

| Column | Default | Gates |
|--------|---------|-------|
| `shift_autoclose_enabled` | 1 | Layer-A auto-Z on/off (per FN) |
| `shift_autoclose_hour` | 23 | Layer-A close hour (per FN) |
| `shift_autoclose_minute` | 54 | Layer-A close minute (per FN) |

(Existing probe knob unchanged: `return_online_probe_interval_seconds`=60, clamp [5,3600].)
**No `DTC`-style global kill-switch** (WebCheck has one; it contradicts "enforce all" — omitted).

---

## §4 — Rust placement, state, and build scope

**Enforcement is in the write-path (hot zone), not a UI:**
- 36h / 168h offline caps → checked in the offline-admit path (`stage_acquire` offline-channel gate +
  `dispatch_post_sign`/`stage_offline_ack` precondition) **and** on the return-online probe tick.
- 24h backstop (Layer B) → `stage_acquire::check_shift_guard` (`stage_acquire.rs:845`) extended with a duration check.
- Per-FN auto-Z (Layer A) → **new runtime ticker task** (supervisor / RS-4 sibling) — this is new spine wiring.
- Cert gate → `stage_acquire` SHIFT_OPEN branch.

**What exists vs builds:**
| Piece | Status |
|-------|--------|
| `node_state.current_month_offline_seconds` column | **exists, unused** (`001:69`) — wire it |
| `node_state.offline_accum_month` | **build** (new column) |
| `set_mode_blocked_tx` | exists + wired (only via `-11`) — reuse for gateway-computed caps |
| `offline_sessions.opened_at` / `shifts.opened_at` | exist — start reading for duration |
| per-FN `shift_autoclose_*` columns | **build** (migration on `fiscal_number_config`) |
| `LimitsCfg` / `OfflineCfg` extensions | **build** (config/mod.rs) |
| shift-duration ticker task | **build** (runtime; depends on the spine / RS-4) |
| W10 offline-Z local close | **build** (in scope) — the offline shift-close exit |
| clamp+audit helpers | reuse `clamped_*` pattern |

**Invariant guard:** all node-mode / shift-state writes go through the single transition-service (per
[[project-q1-shift-arch-decision]] — Q1 refined-A′); the auto-Z emits via the normal Z_REPORT write-path (no ad-hoc
SQL); offline caps freeze ingress **before** doc creation (INV — invalid/over-limit never persists to `fiscal_documents`).

---

## §5 — Open / deferred
- **Clamp bounds** for the minute thresholds (the `TBD` rows) — pick sane min/max per limit before coding.
- **Auto-Z while offline** routes through W10; W10's own code-pool reserve rule (`LEGAL_INVARIANTS.md:194`, hard
  reserve = 1) must hold so the offline-Z always has a code.
- **Shift-duration ticker** depends on the runtime spine (RS-4) existing — sequence after the spine.
- This is **WL-1 / offline-reachability + a new HB5 worklet**; it rides on the spine (RS) and the refined-A′ shift arch.

---

*Synthesis of two reference products + the ФСКО protocol, all file:line-grounded. The Rust design diverges from both
deliberately: enforcement in the write-path (not UI), 168h local-accumulator (op decision), per-FN shift-close (op
decision), independent 36h/168h caps, single-probe return-online. Defaults are WebCheck/PRRODPS-derived; all configurable.*
