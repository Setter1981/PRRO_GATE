# M3b W8 — return-online detection probe — design freeze

**Date**: 2026-05-16
**Author**: implementation pass post-W7b merge (`rust-gateway` `12a14bb`)
**Status**: design freeze; pending operator GO before implementation.

## 1. Decision: `statusRro` (NOT `ping`)

The W8 probe uses the existing `DpsChannel::status_rro` surface:

```rust
async fn status_rro(&self, fn_sign: &CheckSignBlob) -> Result<StatusSnapshot, DpsError>;
```

Returns:

```rust
pub struct StatusSnapshot {
    pub open_shift: bool,    // DPS-side shift state
    pub online: bool,        // DPS-reported "FN is online" flag
    pub last_signer: String, // last signing operator on DPS
}
```

Existing implementations: real gRPC at `rust/prro/src/transports/dps/grpc.rs:148`; test stubs in `rust/prro/tests/common/mod.rs` + `services/reconciliation/last_chk_probe.rs:149`.

### Rationale (operator pin, 2026-05-16)

W8 is NOT just "is the internet to DPS reachable" — it's "can we start the return-online lifecycle for THIS FN".

- **statusRro**:
  - Read-only over wire.
  - FN-bound (carries `fn_sign` argument).
  - Returns observable DPS-side state (`open_shift`, `online`, `last_signer`).
  - Sufficient signal for "DPS reachable + FN status usable" decision.
  - Allows audit payloads to record observed DPS state — operator forensic review can correlate post-probe DPS state with local state.
- **ping** (rejected):
  - Liveness-only (proves channel reachable, NOT RRO/FN state).
  - Doesn't distinguish "DPS up but this FN unknown to DPS" from "DPS up + FN exists".
  - Too weak for the "start return-online" semantic.

If a future refactor surfaces a cheaper, FN-bound DPS read (e.g., a hypothetical `ping_fn`), W8 design freeze can be amended in a follow-up.  As of 2026-05-16 the cleanest choice is `statusRro`.

## 2. Probe-success decision logic

The probe interprets `StatusSnapshot` to decide whether to flip `Offline → GoingOnline`:

| `online` | `open_shift` | local shift_state | Probe outcome |
|----------|--------------|-------------------|---------------|
| true     | (any)        | (any)             | **Success** — flip mode to `GoingOnline`.  Shift drift (if any) is W9 / operator territory, NOT auto-reconciled by W8 (operator W8 criterion 7). |
| false    | (any)        | (any)             | **Drift / failure** — DPS reports FN offline.  Mode UNCHANGED; emit `RETURN_ONLINE_PROBE_FAILED` audit with the observed snapshot as forensic payload. |

The `online == true` flag is the **load-bearing signal** — it's DPS's own "this FN is currently online-eligible" indicator.  We trust DPS's view here; if DPS reports `online == false`, W8 does NOT flip mode locally.

`open_shift` and `last_signer` are recorded in the audit payload (success AND failure) for forensic context but do NOT participate in the routing decision.  This keeps W8's logic minimal and defers any divergence handling to W9.

### Failure modes

| Condition                                  | Outcome                                                                 |
|--------------------------------------------|-------------------------------------------------------------------------|
| `DpsChannel::status_rro` returns `Err(_)`  | `RETURN_ONLINE_PROBE_FAILED` audit + payload carries `DpsError` class; mode unchanged. |
| `Ok(StatusSnapshot { online: false, .. })` | `RETURN_ONLINE_PROBE_FAILED` audit + payload carries the snapshot itself + a `"reason": "dps_reports_offline"` field; mode unchanged. |
| Ambiguous response (gRPC future-extension) | Treat as failure; payload `"reason": "ambiguous_response"`.             |

In all failure cases: **mode stays unchanged**, **no fiscal-side-effect**, **no auto-reconcile**.

## 3. State transitions owned by W8

```
Offline       --probe success-->   GoingOnline   (audit RETURN_ONLINE_PROBE_SUCCESS)
GoingOnline   --probe success-->   GoingOnline   (no-op; idempotent)
Online        --probe runs?--->    no probe      (skip; criterion 2)
(any other)   --probe runs?--->    no probe      (skip; criterion 2)
```

W8 does NOT own:
- `GoingOnline → Online` transition — that's W9's responsibility after backlog drain.
- Any shift-state mutation.
- Any fiscal_documents mutation.
- Any offline_sessions / offline_codes mutation.

The success-path mode write goes through `with_immediate` (W2 / W5 axis 2 discipline).  The DPS call is OUTSIDE the envelope.

## 4. Audit event vocabulary

| Event type                       | Severity | Payload (JSON)                                                                                              |
|----------------------------------|----------|-------------------------------------------------------------------------------------------------------------|
| `RETURN_ONLINE_PROBE_ATTEMPT`    | Info     | `{fiscal_number, observed_mode_pre, tick_at}`                                                               |
| `RETURN_ONLINE_PROBE_SUCCESS`    | Info     | `{fiscal_number, observed_mode_pre, observed_mode_post, dps_open_shift, dps_online, dps_last_signer}`       |
| `RETURN_ONLINE_PROBE_FAILED`     | Warning  | `{fiscal_number, observed_mode_pre, dps_error_class?, dps_error_detail?, authorization_kind?, dps_snapshot?, reason}` |

`entity_type` = `"node_state"`; `entity_id` = fiscal_number.

`dps_error_class` and `dps_snapshot` are mutually exclusive — DPS Err sets the former, DPS Ok-with-offline sets the latter.

`dps_error_class` is a **stable string taxonomy** mapped from `DpsError` variants — `"Transport"`, `"Authorization"`, `"Decode"`, `"Server"`, `"NotFound"`, `"ServerFiscalIdMismatch"`, `"QueryNotSupported"`, `"Internal"`.  Audit consumers (dashboards, alert rules) can match exact strings without parsing Debug repr.  `authorization_kind` is emitted only when class is `"Authorization"` — values `"DocumentReject"` / `"FiscalNumberNotRegistered"`.  `dps_error_detail` carries the `Display` message verbatim for forensics.

The `reason` field on `_FAILED` is a typed-string enum: `"dps_error"`, `"dps_reports_offline"`, `"cas_miss_concurrent_mode_change"`.  Typed via Rust enum mapped to fixed string at audit emission site; downstream consumers can string-match safely.

## 5. Runtime task lifecycle

> **W8a / W8b split (2026-05-16).**  W8a ships the tested **primitive** (`run_tick_for_fn` + `spawn_probe_loop`) plus the `OfflineCfg` schema delta and the `clamped_probe_interval_seconds()` helper.  W8a does NOT wire the loop into `App::boot` — that is W8b's scope (ownership of `JoinHandle`, watch::Sender placement, FN → `fn_sign` resolution, shutdown plumbing from `main`, boot-level "no probe for Online FNs" + "clean shutdown" integration tests, and emission of the WARN audit when the operator-supplied interval is outside `[5, 3600]`).  §Task 8 remains OPEN until W8b lands.  The bullet list below describes the **end state** that W8b realises; W8a's `spawn_probe_loop` already implements the per-task discipline, only the App-level wiring is deferred.

- Probe is a single tokio task spawned at App boot (one task that iterates FNs each tick, NOT per-FN tasks — simpler shutdown discipline).
- Tick driven by `tokio::time::interval` with `MissedTickBehavior::Skip` (no queue-up of missed ticks).
- Default interval: 60 seconds (per plan §Task 8 line 576).
- Configurable via `AppConfig.offline.return_online_probe_interval_seconds` (raw operator value).
- Boot callers obtain the safe value via `OfflineCfg::clamped_probe_interval_seconds()` (helper-side clamp; the field itself stores the raw value verbatim).
- Lower bound enforced by the helper: 5 seconds (operator safety against accidental DPS overload).
- Upper bound enforced by the helper: 3600 seconds (1 hour).
- App shutdown: `select!` over `interval.tick()` + `shutdown.recv()`; task exits cleanly.

## 6. AppConfig schema delta

```toml
[offline]
return_online_probe_interval_seconds = 60  # default; helper-clamped to [5, 3600] at boot
```

Validation contract: the field stores the raw operator value verbatim — the `Deserialize` impl does NOT clamp.  Boot callers MUST invoke `OfflineCfg::clamped_probe_interval_seconds() -> (clamped: u64, was_clamped: bool)` to obtain the safe value and emit a WARN audit when `was_clamped == true`.  Reading the raw field directly in a runtime hot path is an API contract violation.

## 7. Out of W8 scope (explicit deferral)

- Shift state auto-reconcile on DPS-side drift — operator W8 criterion 7 + plan §Task 8 line 583.
- Backlog drain on `GoingOnline → Online` transition — W9 territory.
- Stage_acquire offline-mode ingress — W7 follow-up territory (M3b W8b / M3c per operator decision).
- Multi-FN coordination beyond single-task iteration — defer to a future scaling task if pilot reveals contention.

### 7a. W8b scope (deferred from W8a, closes §Task 8)

- **App-owned runtime seam** wires `spawn_probe_loop` when called by the future composition root: reads `clamped_probe_interval_seconds()`, enumerates **ALL configured FNs** (not just `(Offline | GoingOffline)` ones), constructs `Vec<ProbeSpec>`, returns the `JoinHandle<()>` to the caller (caller owns the `watch::Sender<bool>` + `JoinHandle`; `App` does not track lifecycle).  W8b ships the seam as `App::spawn_return_online_probe`; the first production caller is deferred to a future runtime-composition task (`main.rs` Serve remains M1-idle).  Rationale: `ProbeSpec` is frozen at spawn (per primitive §5), so a boot-time mode filter would orphan FNs that start `Online` and later transition to `Offline` — they would never be probed.  The tick-level mode re-read (operator hard line 5) already filters `Online` / `GoingOnline` cheaply BEFORE the wire call, so enumerating all FNs at boot is the correct selection rule and costs nothing in steady state.
- `main` propagates shutdown into the watch::Sender on SIGINT / SIGTERM **once the production caller lands**; the loop's clean-exit guarantee is already exercised end-to-end at the App-seam level (W8b boot test #3).
- WARN audit on helper-side interval clamp (operator-supplied value outside `[5, 3600]`) emitted from the boot site, not from the primitive.  Boot reads the raw config field, passes it through `clamped_probe_interval_seconds()`, and emits the audit when `was_clamped == true`.
- Loop-level error visibility (review MED #2): on `run_tick_for_fn` `Err(_)`, emit a CRITICAL `RETURN_ONLINE_PROBE_LOOP_ERROR` audit row in addition to `tracing::error!`; supervisor restart NOT required (loop already continues on next tick), but the operator must see a durable record.
- **Module doc cross-link** (W8a Round 3 review L3): a one-paragraph module-doc note in `services/offline_sync/return_online_probe.rs` explains the `statusRro` vs `ping` design choice (rationale: `statusRro` gives a read-only surface AND records `open_shift` / `last_signer` for forensics in the success/failure audit; `ping` was the lighter alternative but does not carry the snapshot fields, and the snapshot is operator-pinned hard line 2).  Cross-linked to design freeze §10 + memory `m3b-w8-review-criteria` axis 6.  Lands in W8b PR #59 (Round 2).  Not load-bearing on runtime behaviour but resists future drift toward the lighter call.
- Boot-level integration tests:
  - `probe_no_attempt_audit_while_fn_is_online` — FN that starts `Online` produces zero `_ATTEMPT` audit rows across N ticks (validates tick-level skip-Online + the "enumerate all FNs" rule together).
  - `probe_attempts_after_online_to_offline_flip` — FN that starts `Online` and is flipped to `Offline` mid-test gets its first `_ATTEMPT` audit on the next tick (validates that the "enumerate all" boot rule actually catches the late transition).
  - `probe_respects_shutdown_signal_at_app_level` — boot → SIGTERM → clean task exit within bounded timeout.

### 7b. W8b accepted residuals (PR #59 Rounds 1 + 2 review)

Documented as residuals; **not** addressed in W8b.  Listed here so a future runtime-composition / probe-respawn task picks them up explicitly rather than re-discovering them.

- **L1 — Clamp WARN audit dedup on respawn.**  `App::spawn_return_online_probe` currently emits the `RETURN_ONLINE_PROBE_INTERVAL_CLAMPED` WARN audit unconditionally on every call when `was_clamped == true`.  At W8b's single-boot caller this is one row per process; future runtime composition that re-spawns the probe on config reload would emit a duplicate WARN row per reload with the same payload.  **Why deferred**: the dedup decision (process-wide, per-App-lifecycle, or per-effective-config-hash) is a property of the lifecycle owner, which does not exist in the codebase yet.  Picking one now would constrain the future runtime-composition layer.  **When to fix**: when the first runtime-composition caller that re-spawns the probe lands; emit-once-per-effective-config (hash of clamped value + bounds) is the likely shape, but defer the choice to the lifecycle owner.
- **L2 — Partial WARN audit trail on missing-signer-loop audit failure.**  `App::spawn_return_online_probe` walks configured FNs and emits a `RETURN_ONLINE_PROBE_FN_SKIPPED_NO_SIGNER` WARN audit per FN absent from `deps.fn_signs`.  These appends are NOT wrapped in a single `with_immediate` envelope: if the audit insert fails mid-iteration, the method returns `Err` and the probe never spawns, leaving the WARN rows for FNs already processed durably persisted as a partial trail.  **Why accepted**: this is a cold-boot forensic write path, not a fiscal state-transition path; a partial WARN trail is operationally clearer than silent suppression on audit failure.  Wrapping N+1 audit rows in one envelope to gain atomicity would force the App method to compose its own transaction and conflict with primitive-side `with_immediate` discipline.  **When to fix**: only if a future incident shows partial WARN rows mislead operator triage; otherwise leave as a documented behaviour.
- **L3 — `RETURN_ONLINE_PROBE_FAILED` cardinality / dedup.**  Deferred from W8b.  `run_tick_for_fn` currently emits one `_FAILED` row per failed tick.  This is intentionally left as the primitive behaviour.  Loop-level dedup / rate-limit is future runtime-composition work because the correct dedup lifetime depends on the eventual production caller, respawn / config-reload semantics, and operational telemetry needs.  **Likely shape when picked up** (subject to revision by the production lifecycle owner): collapse consecutive same-class `_FAILED` rows (same `dps_error_class` *and* same `reason`) into a single durable row carrying `first_failure_at` + `last_failure_at` + `consecutive_count`, refreshed every M ticks (configurable) or after a wall-clock window; first failure of a new class always emits a fresh row; mode flip back to `Online` resets the dedup state; reset also on shutdown / respawn.  **Why deferred from W8b**: this is not a boot seam — it is runtime policy for a long-running production loop.  The cardinality policy, refresh cadence, reset rules, per-FN memory state lifetime, and behaviour under config reload / respawn all belong to the future runtime-composition layer.  W8b closes the App-owned wiring seam; audit-cardinality policy is the next layer's concern and requires pilot feedback to size correctly.

## 8. Test plan

Three acceptance tests (plan §Task 8 line 591-595):

1. `return_online_probe_success` — stub returns `StatusSnapshot { online: true, ... }`; mode flips `Offline → GoingOnline`; audit row exists.
2. `return_online_probe_failure` — stub returns `Err(DpsError::Transport(...))`; mode unchanged; failed audit with `dps_error_class = "Transport"` (stable taxonomy mapped from variant name).
3. `return_online_probe_idempotent` — first success flips to GoingOnline; second success on GoingOnline is no-op (no mode write, no duplicate `_SUCCESS` audit).

Plus the boot-level integration tests enumerated in §7a (W8b scope): `probe_no_attempt_audit_while_fn_is_online`, `probe_attempts_after_online_to_offline_flip`, `probe_respects_shutdown_signal_at_app_level`.  These exercise the "enumerate all configured FNs at boot, tick-level skip filters Online/GoingOnline" rule end-to-end.

## 9. Operator-pinned implementation hard lines (2026-05-16)

These six hard lines were pinned by the operator post-design-freeze acceptance, immediately before implementation start.  Every W8 implementation review will verify them by inspection + test coverage.

1. **`StatusSnapshot::online == true` is the SINGLE success predicate.**  No multi-field composition (no "online OR open_shift", no "online AND last_signer == expected", etc).  Code path: `if snapshot.online { /* success */ } else { /* fail */ }`.
2. **`open_shift` and `last_signer` are AUDIT FIELDS ONLY, never branch conditions.**  Recorded verbatim in `_SUCCESS` / `_FAILED` payload for forensics.  No `if snapshot.open_shift { ... }` anywhere in W8 logic.
3. **`Offline → GoingOnline` only inside `with_immediate`, with explicit CAS `WHERE mode = 'OFFLINE'`.**  Mirrors W7a/W7b CAS discipline.  Successful UPDATE returns 1 affected row; 0 rows means concurrent state change (mode flipped under us) — treat as no-op (no audit spam) and let the next tick re-evaluate.
4. **`GoingOnline` + success → no DB write, no audit row.**  Strict no-op (NOT "downgraded debug audit").  Operator wants ZERO spam.
5. **`Online` mode → skip BEFORE the DPS wire call.**  Re-read `node_state.mode` at the start of each tick; if Online → continue to next FN without invoking `status_rro`.  Defence-in-depth: even though W8 task-spawn discipline filters Online FNs, the per-tick re-check guards against a mid-operation `Offline → Online` flip (W9 territory).
6. **Single tokio task for all FNs.**  `tokio::time::interval` with `MissedTickBehavior::Skip` (no queue-up).  `select!` over `interval.tick()` + `shutdown.recv()`; shutdown branch ALWAYS wins cleanly (no panic, no dangling tx, no half-finished audit).  App boot doesn't block on probe init.

## 10. Operator-pinned negative test (2026-05-16)

A scanner/negative test verifies the probe NEVER writes to fiscal-data tables.  Cheaper to catch by test than by manual review on every future edit.

Test shape: `tests/return_online_probe_no_fiscal_side_effects.rs`:
- Seed FN in `Offline` mode + at least one fiscal_document, one offline_session, one offline_code, one transport_trace row (pre-existing state).
- Stub DpsChannel returns `Ok(StatusSnapshot { online: true, open_shift: false, last_signer: "test" })`.
- Run probe one tick.
- Assert mode flipped to `GoingOnline`.
- Assert ROW COUNTS for `fiscal_documents`, `offline_sessions`, `offline_codes`, `transport_trace` are UNCHANGED.
- Assert ROW CONTENTS for those tables are UNCHANGED (no silent UPDATEs).

Same shape repeated for `Ok(... online: false ...)` and `Err(DpsError)` cases — failure paths must ALSO leave fiscal tables untouched.

This is the structural guard for criterion 1 (strictly read-only over DB).  Any future edit that accidentally adds a write to a fiscal table breaks this test immediately.

## 11. Cross-references

- Plan §Task 8: `docs/superpowers/plans/2026-05-14-m3b-implementation.md` lines 570–602.
- W8 review criteria: memory `m3b-w8-review-criteria` (8 criteria).
- DPS substrate: `rust/prro/src/transports/dps/channel.rs:34` (`status_rro` signature).
- Existing `StatusSnapshot` consumers: `last_chk_probe.rs` (for context — W11 probe uses `last_chk`, NOT `status_rro`).
