# Spec #5 — Fleet lifecycle (ADVISORY-only for pilot)

**Status: 🟡 DRAFT rev 1 (for external audit). 2026-07-15. Grounded on `origin/main` `4f51112`.**
Home: **`prro-fleet-contract`** (empty CS-1d skeleton). This spec is **contract/types only** — it
authors the **read-only telemetry projection** the pilot needs and **structurally forbids** the
fleet-agent from mutating a register. The **command lifecycle** (durable/signed/epoch/PULL) is
**deferred post-pilot**; it must land only after CS-4 (per-FN coordinator = local final arbiter) and
CS-5 (data-driven transition table = the legality gate). Fleet = **ADVISORY-only** per the ratified
pivot decision.

---

## 0 · Thesis
For the pilot the fleet is a **view, not a controller**: a read-only projection of already-present
`node_state` health + `audit_log`, plus a push/expose seam. **Advisory ≠ authority** — the local
per-FN coordinator is the final arbiter; a projection can never be a control input. Read-only is
enforced **by type**, not discipline.

## 1 · What EXISTS today (grounded)
- **`node_state` already carries rich per-FN telemetry** (`001_baseline.sql:521-548`): `readiness_state` (`STARTING/RECOVERING/READY/DEGRADED/STOPPED`, `:540`), `recovery_stage` (`BOOT/PHASE1/PHASE2/DONE/FAILED`, `:541`), `current_month_offline_seconds` (`:542`), `last_fs_ping_at` (DPS liveness, `:544`), `current_offline_session_id` (`:536`), `updated_at` (`:545`), plus `mode` (`NodeMode` 7-state) and `shift_state`.
- **But `NodeStateRow` decodes only a SUBSET** (`node_state.rs`): `fiscal_number, mode, shift_state, next_lnd, last_known_unsigned_xml_sha256, current_shift_id, backend_profile_id, transport_profile_id, next_z_report_number`. The richest health signals (`readiness_state / recovery_stage / current_month_offline_seconds / last_fs_ping_at / updated_at`) are **persisted but never surfaced**.
- `NodeMode` is already threaded as a **telemetry signal** in the write-path (`dispatch.rs:75` "observed mode for telemetry") but only **logged locally**, never emitted off-node.
- **Mutators exist and are write-lease-bound:** `set_mode_blocked_tx` / `set_mode_stop_mode_tx` / `set_mode_offline_tx` / `set_mode_going_online_tx` (`node_state.rs:205/236/257/276`), all on `&mut WriteTxConn`.
- **Operator commands are CLI-only + hold an EXCLUSIVE singleton lock** (`admin.rs`, `singleton::acquire` at `:1261/:1370` — "stop prro serve first"): they cannot run while the node is live-serving.
- Node-local ops-loops (`supervisor.rs`) iterate only this node's own registry — **no PULL / push to any control-plane**.
- **ENTIRELY GREENFIELD (zero representation today):** any `ReceivedDurable/Applied/Rejected/Deferred` command state, signed commands, epoch versioning, a fleet command inbox, or a fleet read-model. (Disambiguation: "epoch" in the source is the DPS Kyiv-date encoding; "coordinator"/"advisory"/"projection" in the source are unrelated local concepts — the shift→node_state mirror, not a fleet projection.)

## 2 · GREENFIELD — pilot scope vs deferred
- **Pilot (this spec):** a `FleetTelemetryProjection` read-model + a `FleetTelemetrySource` read-only port + the widening of the decode to surface the health columns. **No commands.**
- **Deferred post-pilot (named, NOT authored here):** the command lifecycle (`ReceivedDurable | Applied | Rejected | Deferred`), signed commands, epoch-versioned PULL, and a durable fleet command inbox — all land only **after CS-4 + CS-5**.

## 3 · Key types (`prro-fleet-contract`, read-shaped)
```rust
// A per-FN projection of already-present node_state health (+ audit-derived counters). READ-only.
struct FleetTelemetryProjection {
    fiscal_number: FiscalNumber,
    mode: NodeMode,
    shift_state: ShiftState,
    readiness_state: ReadinessState,          // currently undecoded
    recovery_stage: RecoveryStage,            // currently undecoded
    current_month_offline_seconds: i64,       // currently undecoded — the 168h/mo legal budget signal
    last_fs_ping_at: Option<String>,          // DPS liveness — currently undecoded
    current_offline_session_id: Option<OfflineSessionId>,
    updated_at: String,
}

// The node implements this; the fleet-agent receives a READ-ONLY handle. It CANNOT yield a
// WriteTxConn and CANNOT name any node_state mutator or admin fn (enforced by the type, I1).
trait FleetTelemetrySource {
    fn project(&self, fiscal_number: &FiscalNumber) -> Result<FleetTelemetryProjection, ...>;
    fn project_all(&self) -> Result<Vec<FleetTelemetryProjection>, ...>;   // whole local registry
}
// Pilot degenerate case: N=1 (one FN, one row); the agent is OFF/advisory. Turning it on later is
// deployment/config, NOT an architecture re-cut.
```
The projection is fed from a **read-only pool handle** (never `WriteTxConn`); it takes **no write
lease** and never blocks a live write-path lease (RP5-4).

## 4 · Normative invariants
- **I1 (read-only by type).** The pilot fleet-agent handle **cannot obtain a `WriteTxConn`** and **cannot name** any `set_mode_*_tx` (`node_state.rs:205-285`) or `admin.rs` fn — enforced by the handle **type**, not by discipline. A fleet crate that could reach a mutator is a contract breach.
- **I2 (advisory ≠ authority).** The local per-FN coordinator is the final arbiter; a projection is a view, never a control input. Fleet policy may **never** force an illegal offline / cap-breach / return-block.
- **I3 (no admin reuse — it deadlocks).** An in-process apply path (if ever built post-pilot) must go through the supervisor loop, **not** the CLI `admin.rs` entrypoints — reusing them would **deadlock on the exclusive singleton lock** (`admin.rs:1261/:1370`) while the node is live-serving.
- **I4 (command lifecycle lands after CS-4 + CS-5).** When built, it must sit **after** the coordinator (the local final arbiter) and the data-driven transition table (the legality gate) — today mode legality is only scattered inline CAS `WHERE mode=…` guards, no `allowed_transition` whitelist.
- **I5 (future command inbox = separate + INACTIVE-first).** A durable command inbox SHOULD reuse the `ingress_inbox` durability pattern (idempotency-keyed, worker-triggered) but as a **SEPARATE table** with epoch + signature + fleet-provenance columns — landed INACTIVE-first (the CS-1 skeleton discipline), never overloading `ingress_inbox`.
- **I6 (distinct fleet audit provenance).** A fleet-originated action needs a distinct `audit_log` provenance (which coordinator / epoch / signature) — do not overload the local `ADMIN_*` attribution.

## 5 · RED-pins
- **RP5-1 (read-only by type):** the pilot fleet handle **fails to compile** against any `set_mode_*_tx` / `admin` fn and cannot construct a `WriteTxConn` (a `trybuild` compile-fail, like `write_tx_conn_compile_fail.rs`).
- **RP5-2 (surface the health columns):** `FleetTelemetryProjection` decodes `readiness_state + recovery_stage + current_month_offline_seconds + last_fs_ping_at + updated_at` — a regression test that widening the decode (`node_state.rs` `get_tx`, or a dedicated projection query) actually reads them (they are persisted but currently dropped).
- **RP5-3 (no command surface in pilot — deferred, known-red):** the `prro-fleet-contract` crate exposes **no** command-lifecycle type in the pilot (`assert` the crate has no `ReceivedDurable/Applied/Rejected/Deferred` / epoch / signature type).
- **RP5-4 (advisory takes no lease):** the projection read path acquires **no** write lease and never blocks a live write-path lease (a concurrency test: a projection during an active `fiscalize` does not serialize behind it).

## 6 · Open questions for the audit
1. **Decode widening vs dedicated projection query:** surface the health columns by widening `NodeStateRow` (touches every `get_tx` consumer) or via a **separate read-only projection query** (leaves `NodeStateRow` alone)? The latter is a smaller blast radius.
2. **`current_month_offline_seconds` semantics:** is the persisted counter authoritative for the fleet view, or must the projection reconcile it against the durable time-budget ledger (168h/mo)? (Telemetry vs the enforced legal budget.)
3. **`audit_log`-derived counters:** should the pilot projection include any `audit_log` aggregates (e.g. recent RMR / stuck-doc counts), or stay strictly `node_state`-column-derived to keep the read path cheap?
4. **`FleetTelemetrySource` transport:** does the pilot need a push/expose seam now (HTTP `/metrics`-style, or the existing metrics endpoint), or only the in-process projection type until the fleet-agent server is built (CS-6+)?
5. **N=1 posture:** confirm the pilot ships the agent OFF/advisory (projection type present, no emitter), and enabling it is config-only.
