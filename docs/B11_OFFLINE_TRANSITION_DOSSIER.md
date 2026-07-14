# B11 — Automatic ONLINE↔OFFLINE Transition — Design Dossier (ADR)

**Status: DESIGN LOCKED (2026-07-14).** Frame + architecture + all 7 RED-pins resolved.
Implementation not started. This dossier is the flagship decision-record for the
offline/B11 external audit — it states, for each decision, the **problem, the
alternatives considered, the choice, and why**.

> **Method note.** The architecture was produced by an adversarial multi-agent synthesis
> (4 independent strategy proposals → 4 adversarial judge-lenses each → synthesis → completeness
> critic → revise). The losing strategies and their **fatal flaws** are recorded in §5 as the
> audit trail of alternatives. All `file:line` were read from the live code by the agents
> (Ground phase partially failed; re-verify exact lines at implementation).

---

## 1 · Problem

Today offline **entry is manual-CLI only** (`prro admin go-offline`). On a DPS outage the
register STALLS: documents route to `ErrorRetryable` and retry forever, the cash desk is
stuck. B11 designs **when and how** the gateway automatically enters offline and returns
online — correctly (no double-issue), legally (offline is failure-forced, not elected), and
with minimal risk to a **live-proven** core (online + offline cycles were accepted by the
real DPS test cabinet on 2026-07-14).

## 2 · The three orthogonal axes (frame — operator-locked)

The problem is not "pick a detection strategy"; it decomposes into three independent axes.
Each was pinned by an operator objection that killed a naive variant.

| Axis | Options considered | **Chosen** | Why / who |
|---|---|---|---|
| **1 · Signal** | (a) transport-exception only; (b) ping/health-endpoint; (c) **forward-progress** | **(c) forward-progress** — "can we get a fiscal doc ACCEPTED", covers transport-unreachable AND reachable-but-byzantine | Operator: *"transport reachability ≠ server health"*. Ping proves nothing about fiscal acceptance. Probe = **real traffic**, never a synthetic double-issue-risky send. |
| **2 · Commit** | (a) eager (open session when DPS-down detected); (b) **lazy / demand-driven** | **(b) lazy** — the offline session (DocType-9 = budget clock start) opens on the **first real receipt**, not on the command; in EVERY mode incl. MANUAL | Operator: *"budget is burned by TIME not receipts — a no-sales arming wastes the legal 168h/36h cap"*. Arming without sales = 0 budget. |
| **3 · Control** | (a) full-auto; (b) full-manual; (c) **policy: AUTO / MANUAL(fleet) / ADVISORY** | **(c)** — AUTO for clean outages; MANUAL = durable fleet manager command for byzantine "looks-alive-but-dead"; ADVISORY = auto-detect + human-confirm | Operator: *"sometimes DPS seems to work but doesn't — need a manual mode, manual in AND out; also for a bad/unstable channel"*. |

**State split from Axis-2:** `GOING_OFFLINE` = detected/armed, **no session, budget not
ticking**, probing · `OFFLINE` = first offline doc issued, session open, clock running.
Recovery during a no-sales hold → `ONLINE`, zero budget spent.

## 3 · Chosen architecture

**Philosophy: wire the existing seams — 3 thin durable additions, not a new subsystem.**
Every transition rides an existing `with_immediate` (invariant #1 safe); every crash window
is single-tx or covered by the `#192` StuckNonTerminalDoc boot scanner.

### 3.1 Detection + the anti-mask rule (the crux)
Durable per-FN counter `node_state.consecutive_failed_acks`, mutated in the SAME
`with_immediate` that persists the send outcome (crash-atomic). NOT a per-request loop —
otherwise every new sale resets attempts to 0 and the register never trips.

- **Increment** only on `DpsError::Transport` (no parseable DPS envelope = UNREACHABLE), only
  on the **first** failure of a given `request_id` (drain re-drives don't re-increment).
  Threshold N=3 distinct docs → `ONLINE→GOING_OFFLINE`.
- **Reset** on a clean ACK (`Sending→Sent`).
- **🎯 Reset on a PARSED DPS reject (incl. `-1`).** A parsed envelope is **proof-of-life**:
  DPS is processing (forward progress), so the counter resets even though this doc was
  rejected. **This one rule simultaneously defines the byzantine classifier AND forbids going
  offline on our own signing/config bug** (`-1`/`-13`/`-14` never climb the counter, so we
  never issue offline docs that would themselves never drain).

**The classifier is free:** `DpsError` (`error_routing.rs:282`) already splits `Transport`
(→ UNREACHABLE, AUTO may act) from `Server/Authorization/Decode` (→ **DEGRADED**: DPS responds
but doesn't process/lies — a second counter `consecutive_degraded_responses` drives ONLY a
management-console alert, **never** an auto-flip; a manager decides — O1).

### 3.2 State machine (revive the dormant GOING_OFFLINE)
New **mode-only** guarded CAS in `node_state.rs` (shift_state untouched → inv #3; a node-mode
flip is legal across an open shift):
- `set_mode_going_offline_tx` (`WHERE mode='ONLINE'`) — arm.
- `set_mode_offline_from_going_offline_tx` — lazy-commit at first offline doc (budget starts).
- `set_mode_online_from_going_offline_tx` — no-sales clean recovery → ONLINE, 0 budget.
- `set_mode_offline_from_going_online_tx` — rollback (transient drain fail mid-return).
- **Precedence:** recovery family (STOP_MODE / BLOCKED / **CRYPTO_DEGRADED**) wins — if the
  outage is our own signer, CRYPTO_DEGRADED preempts arming → we never issue offline docs on a
  broken signature. Single-writer lease serializes all flips.

### 3.3 Lazy-commit & return oracle
- Budget clock (DocType-9) starts at the **first real offline receipt**, not at arming.
- **Return = forward-progress, not ping.** `status_rro().online` stays only as a cheap **gate**
  ("should we attempt"); the **commit** to ONLINE requires **2 consecutive clean DRAIN ACKs of
  real backlog docs** (stronger than a ping — proves DPS processes; idempotent by construction).
  Backoff 1→2 min. The first `GOING_ONLINE` drain doc IS the probe.

### 3.4 Flap / budget-fragmentation defense (lazy-commit makes it worse)
`online_since` + **`MIN_ONLINE_DWELL`** (default 300s): re-arm forbidden within the dwell.
Exceeding `flap_sessions_per_hour` → register STOPS auto-arming, raises a DEGRADED
"unstable channel — manager takeover" alert (bad channel → manager, not a shredded 168h budget).

### 3.5 Double-issue-safety
Ambiguous `Transport` timeout at arming, AFTER the `Sending→Sent` CAS may/may-not have stamped
`server_fiscal_no`:
- **before CAS:** seed not advanced, sfn absent (D2 pin) → transport-retryable, safe re-send.
- **after CAS (Sent-but-unconfirmed):** **quarantine the DOC, not the SHIFT** — the ambiguous
  doc goes to a per-doc recon lane (boot-KVT2 tick resolves it against DPS); the shift stays
  OPENED and keeps selling offline (V1 max-availability). *(Whole-shift RMR remains for W9b
  drain-rejects of committed backlog — M3b §16.7.)* **⚠️ needs §16.7 amendment — see §6.**
- **seed-fork at lazy-commit:** if the arming doc landed, the offline chain seeds from the same
  advance-at-SEND value DPS expects → consistent; if not, the first drained doc rejects
  `ChainSeedMismatch` → existing SW-4 arm (`inline.rs:980`) → RMR. B11 only pins "quarantine
  resolves before lazy-commit reads the seed".

### 3.6 Fleet command (epoch-versioned PULL primitive)
Not push (push fails exactly when DPS-adjacent infra is down). Per-FN on `node_state`:
`fleet_hold_active/epoch/acked_epoch/reason/actor/max_seconds`, epoch monotonic.
- Apply/Release = guarded CAS `WHERE fleet_hold_epoch < :E` → reorder/dupes idempotent;
  higher-epoch RELEASE supersedes a stale local HOLD (no self-imprisonment).
- Honor (pull): re-read at every quiescent boundary + on boot; `active=1` → arm +
  hard-suppress `return_online_probe` (`SkipReason::FleetHold`).
- Source ∈ {**local** operator CLI, **fleet** manager}; unified hold-precedence:
  **return ⟺ no local hold AND no fleet hold** (`SkipReason::AnyHold`). A cashier cannot lift a
  fleet HOLD; a local hold lifts locally.
- N=1: "fleet" = one FN, one row UPDATE. Fleet by semantics, single-row by implementation.
- `keep-alive` (warm channel): its **failure** = early UNREACHABLE pre-tick (front-runs the
  counter → instant first offline sale); its **success** is NOT an oracle.

## 4 · Invariant preservation (all 10 held)
Crypto-sign stays outside the tx (#1); all under lease (#2); mode-only CAS across open shift
(#3); counter by first-failure + epoch-guarded holds (#4); lazy-budget + T2 reserve + T3 caps +
unconditional 24h auto-Z (#5); no adapter/envelope change (#6/#7); paired audit row per tx +
#192 boot scanner + rollback edge (#8); tick loops bail on shutdown, boot re-honors HOLD (#9);
signing-profile untouched, gated on offline profile (#10).

## 5 · Alternatives considered (adversarial audit trail)
Four independent strategies were designed and adversarially judged. **Winner:
`state-machine-first` (avg 6.875, 0 fatal flaws).** The highest raw score lost on fatal flaws —
the anti-mask rule (§3.1) is what the winner does that the losers didn't.

| Strategy | Avg | Fatal flaws (verified) |
|---|---|---|
| **state-machine-first** ✅ | 6.875 | **none** |
| forward-progress-engine | 7.125 | DEGRADED classifier **masks `-13/-14`** config bugs like `-1`; arms on operator-recoverable/probe-pending conditions |
| minimal-seam-ship | 6.125 | **fleet-HOLD leaks into auto-return** (`backlog_drain.rs:1069` does `GoingOnline→Online` independently of the probe); moving session-open into `stage_sign` breaks live-proven STOP-O3-1 + INV-3/8; 24h auto-Z under hold asserted not proven |
| fleet-control-plane-first | 6.75 | classifier's discriminant is **already discarded** — `classify_send_outcome` (`inline_map.rs:387`) collapses `ErrorRetryable→InProgress` and drops `retry_class` before `inline.rs:891` |

**These flaws are now designed out:** the fleet-HOLD leak → guard the drain CAS too; the
retry_class discard → thread the variant from `error_routing.rs:282`; the `-13/-14` masking →
the anti-mask reset rule; the STOP-O3-1 break → lazy-commit goes through offline SHIFT_OPEN
edge-2, not `stage_sign`.

## 6 · RED-pin decisions (resolved 2026-07-14)

**Operator decisions (4):**
1. **auto-Z under GOING_OFFLINE** → **force a lazy offline session** to close the >24h shift
   (budget sliver = lesser evil; 24h auto-Z is unconditional, the shift MUST close; T3 toggle
   does not disable it).
2. **Ambiguous Sent-but-unconfirmed doc** → **quarantine the DOC, keep the shift selling**
   (V1 max-availability), NOT whole-shift RMR.
   **⚠️ FOLLOW-UP: amend/verify M3b §16.7** — the spec currently has RMR freeze the write-path;
   this decision requires allowing sale on a shift carrying a recon-quarantined doc WITHOUT
   freeze. If §16.7 forbids it, operator must re-adjudicate V1 vs the spec.
3. **Unmanned byzantine** → **stall online + alert** (never auto-offline on byzantine, never
   mask a bug; fail-closed-to-online until a manager or recovery).
4. **fleet-HOLD TTL unreachable** → **fail-safe-held + loud alert** (manager HOLD is
   authoritative; the alert surfaces a leaked hold long before a cap breach).

**Engineering decisions (3, owned by implementer):**
5. budget double-counting (`online_since`/dwell × `current_month_offline_seconds`) — verify by
   test (slice S3).  6. `GOING_OFFLINE→ONLINE` probe precedence — test-pin (S4).
   7. config-toggle race on caps → graceful auto-Z-and-drain before BLOCKED, not a mid-shift
   freeze (dedicated slice).

## 7 · Verification plan
**Fuzzer** (per "feature → fuzzer" rule): +3 ops (`GoOffline/GoOnline/FleetHold`) + a fault
axis `FailToAck{transport}` vs `{parsed_reject}` (must behave differently) + 5 teeth-proven
oracles: anti-mask (parsed-reject run stays ONLINE), budget-0-until-first-doc, fleet-hold-epoch,
flap-cap, double-issue quarantine. **Live-smoke:** real transport outage arms at zero budget;
real ACK returns online.

## 8 · Phased implementation (RED-first, per-slice PR)
`S1` migration 030 + GOING_OFFLINE CAS · `S2` durable counter + anti-mask · `S3` lazy-commit +
orphan scanner · `S4` return-oracle swap + dwell · `S5` fleet-HOLD epoch primitive · `S6`
ambiguous-quarantine · `S7` fuzzer alphabet + oracles + live-smoke. Each with a concrete
RED-pin (see the synthesis result / memory).

## 9 · Open risks needing sign-off / not-yet-verified
- **§16.7 amendment** (decision #2) — the one spec change; must be adjudicated before S6.
- unmanned-byzantine stall (decision #3): a persistent byzantine DPS keeps an unmanned register
  ONLINE-failing (no sales) until a human — accepted as the safe default.
- fleet-hold fail-safe-held (decision #4): can accrue toward BLOCKED if a release is lost —
  mitigated by the TTL alert.
- exact line numbers to re-verify at implementation (Ground phase partially failed).

## 10 · References
Memory: `project_b11_offline_transition_design_frame`, `project_backlog_byzantine_dps_handling`,
`reference_official_dps_prro_source`, `reference_webcheck_offline_entry_criteria`,
`project_offline_legal_limits`, `project_product_vision`. Full synthesis output:
`tasks/w0g3umqd9.output`. Canonical gate: `PILOT_GATE_CHECKLIST.md`, MATRIX/PLAYBOOK.
