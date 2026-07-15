# Spec #5A — Fleet telemetry projection (ADVISORY-only, pilot)

**Status: 🟡 DRAFT rev 3 (post external audit round 2 → NOT-YET, near-lock). 2026-07-15. Grounded on `origin/main` `4f51112`.**
Home: **`prro-fleet-contract`** (+ a new empty **`prro-fleet-agent`** crate, §4). **Split (audit +
operator):** the fleet **command lifecycle** — which the locked plan §3.10 requires with **full
semantics** (signed envelope, epoch, PULL, HOLD, resume, rotation) — is now **Spec #5B**, authored
separately; **#5A is the read-only telemetry projection only**. Rev 3 closes round-2's telemetry-shape
residuals + the precise crate-DAG allowlist. Fleet = ADVISORY-only.

---

## 0 · Thesis
The pilot fleet is a **view computed from AUTHORITATIVE truth** (not `node_state` slots), **read-only
enforced at the crate boundary** (the agent cannot *name* a mutator). Advisory ≠ authority.

## 1 · What EXISTS today — grounded (LOCK-READY per audit)
- `node_state` (`001:521-548`) holds **live** `mode` + `shift_state`, plus **schema slots with ZERO Rust producers** — `readiness_state`/`recovery_stage`/`current_month_offline_seconds`/`last_fs_ping_at`/`current_offline_session_id` have **no write-sites** (14 `UPDATE node_state`, none touch them); they rest at DDL DEFAULTs (`STARTING`/`BOOT`/`0`/`NULL`). Not health signals.
- The write-path `NodeMode` "telemetry" field is **discarded** (`PostSignRoute::Online { .. }`, `inline.rs:909`, `boot_phase.rs:3682/3942`).
- **The authoritative offline budget is computed, not stored:** `time_budget::compute_budgets_for_fn` (`time_budget.rs:276-335`) from `offline_sessions`; enforcement is **separate** for the 36h-continuous session and the 168h-month budgets (`time_budget.rs:186-190`), and the session budget is `Option` (`None` = no active session, `:257-264`).
- **Admin is public + fail-fast, not CLI-only / not deadlock:** `go_offline`/`go_online`/`reset_stop_mode` are public and take a plain `SqlitePool`; the singleton lock is `file.try_lock_exclusive().map_err(…)` → **immediate error**; `open_pool` is a normal **RW** pool (`db/mod.rs:123-136`).
- The active offline session is authoritatively in `offline_sessions` (states `OPENING|OPEN|DRAINING`, protected by `ux_offline_active`, `001:413-431`) — **not** `node_state.current_offline_session_id` (a slot).
- Node-local ops-loops (`supervisor.rs`); **no PULL / control-plane**; zero fleet command surface.

## 2 · Scope (#5A)
The **authoritative** `FleetTelemetryProjection` + an **async read-only** `FleetTelemetrySource` +
the **empty `prro-fleet-agent`** crate with a **cargo-metadata read-only DAG gate**. **No commands**
(→ Spec #5B). No `node_state` write; a **separate read-only projection query** (do not widen
`NodeStateRow`/`get_tx`).

## 3 · Key types (`prro-fleet-contract`, sqlx-free)
```rust
struct FleetTelemetryProjection {
    fiscal_number: FiscalNumber,
    mode: Stamped<NodeMode>,               // node_state (live)
    shift_state: Stamped<ShiftState>,      // node_state (live)
    offline_budget: OfflineBudgetView,     // time_budget::compute_budgets_for_fn — NOT a node_state slot
    liveness: LivenessView,
    active_offline_session_id: Observed3<OfflineSessionId>,   // from offline_sessions active states, NOT the node_state slot
    // readiness_state / recovery_stage are OUT of V1 (no producers today); re-introduce only when a
    // producer lands — never presented as a fake READY.
}
struct OfflineBudgetView {
    month_used: i64, month_limit: i64, month_enforcement_active: bool,     // 168h — its OWN flag
    session_used: Option<i64>, session_limit: i64, session_enforcement_active: bool, // 36h — its OWN flag; None = no active session
    observed_at: String, source: AuthoritySource /* = OfflineSessionsComputed */,
}
struct LivenessView { last_fiscal_ack: Observed3<()>, last_probe_response: Observed3<()> }

// Distinguishes "no producer exists yet" from "producer exists, no event yet" from an actual value.
enum Observed3<T> { Unpopulated, NeverObserved, Observed(Stamped<T>) }
struct Stamped<T> { value: T, authority_source: AuthoritySource, observed_at: String, freshness: Freshness }

// ASYNC + Send + Sync read-only port. The node-side adapter holds the pool PRIVATELY; only this trait
// object crosses to the agent.
#[async_trait] trait FleetTelemetrySource: Send + Sync {
    async fn project(&self, fn_: &FiscalNumber) -> Result<FleetTelemetryProjection, FleetReadError>;
    async fn project_all(&self) -> Result<Vec<FleetTelemetryProjection>, FleetReadError>;
}
```
`Stamped`/`Freshness`/`AuthoritySource`/`Observed3` are **observation semantics** → they live in
`prro-fleet-contract`, not `prro-domain`.

## 4 · Normative invariants
- **I1 (read-only at the CRATE-DAG — precise allowlist).** A **separate `prro-fleet-agent` crate**; a cargo-metadata pin proves that among **workspace** crates its normal+build+optional+target-specific closure (under **all features**) contains **only `prro-fleet-contract` (+ its transitive `prro-domain`)** — and **never `prro`, the store crate, any engine/composition crate, or `sqlx`**. (Non-workspace deps like `async_trait` are allowed.) The pool stays in a **private node-side adapter**; only the `FleetTelemetrySource` trait object crosses. Handle-only would be insufficient (the agent could otherwise import `prro::admin::*`, `App::db()` `app.rs:903`, `with_immediate` `tx.rs:118`, or raw SQL on the RW pool).
- **I2 (advisory ≠ authority).** The local per-FN coordinator is the final arbiter; a projection is never a control input; fleet policy may never force an illegal offline / cap-breach / return-block.
- **I3 (no admin reuse — fail-fast, not deadlock).** A future apply path (Spec #5B) routes through the coordinator's mailbox/API, not the CLI `admin.rs` entrypoints (fail-fast on the singleton `try_lock_exclusive`); the supervisor need not be the sole transport.
- **I7 (projection ∉ control path).** `FleetTelemetryProjection` (and any alias/wrapper of it) appears in **no** admission / transition-oracle signature — a structural pin (covering aliases/wrappers, not just the literal name), consistent with the Spec #1 pure-oracle contract.

## 5 · RED-pins
- **RP5A-1 (crate-DAG read-only — primary):** the cargo-metadata pin proves `prro-fleet-agent`'s workspace-dep closure excludes `prro`/store/engine/`sqlx` (mirrors the CS-1 purity gate); a `trybuild` canary that the port cannot yield a `WriteTxConn` (secondary). **Create the empty `prro-fleet-agent` in CS-2 so this is a real gate; CS-6 only fills it.**
- **RP5A-2 (authoritative budget, not slots):** the projection's offline budget equals `time_budget::compute_budgets_for_fn`, **not** `node_state.current_month_offline_seconds` — a test where the column reads `0` but the computed budget is non-zero (an open session) proves it; the two enforcement flags are independent; `session_used` is `None` with no active session.
- **RP5A-3 (Observed3 honesty):** a signal with no producer decodes to `Unpopulated`, not a fabricated value; readiness/recovery are absent from V1.
- **RP5A-4 (advisory takes no lease):** the projection read takes no write lease and never blocks a live write-path lease.
- **RP5A-5 (projection ∉ oracle):** the static pin covers aliases/wrappers of `FleetTelemetryProjection`.

## 6 · Decisions (from the audit)
- Separate read-only projection query; do not widen `NodeStateRow`/`get_tx`.
- Budget from `time_budget::compute_budgets_for_fn`; two enforcement flags; `session_used: Option`.
- `active_offline_session_id` from `offline_sessions` active states (`ux_offline_active`), not the slot.
- **Fiscal progress:** after CS-3, the last real fiscal ACK comes from the **clean-accepted `delivery_reservation` outcome**, not `transport_trace`. **Probe-response:** no durable typed producer exists today ⇒ `Unpopulated` (never `last_fs_ping_at` or an audit-derived guess).
- Audit aggregates: not in the base snapshot; RMR/stuck from authoritative tables later.
- Push seam: async in-process port now; the emitter/agent server is CS-6; N=1 ships OFF/advisory.

## 7 · Open questions for re-audit
1. **`prro-fleet-agent` in CS-2:** create the empty crate + the cargo-metadata gate now (like CS-1d), confirmed? (Makes RP5A-1 a real gate.)
2. **`Observed3` naming/placement:** acceptable in `prro-fleet-contract`, or should `Stamped`/`Observed3` be a shared observation module?
3. **Liveness producer:** `last_fiscal_ack` reads the accepted `delivery_reservation` outcome (post-CS-3); until then it is `Unpopulated` — confirm no interim source is acceptable (vs an audit-derived stopgap).

---
**Companion:** Spec #5B (fleet command lifecycle — signed/epoch/PULL/HOLD/resume/rotation, dormant
code but full **semantics** locked now per plan §3.10) is authored separately.
