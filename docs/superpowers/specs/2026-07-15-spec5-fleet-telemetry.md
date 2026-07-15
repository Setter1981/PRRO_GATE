# Spec #5 — Fleet lifecycle (ADVISORY-only for pilot)

**Status: 🟡 DRAFT rev 2 (post external audit round 1 → NOT-YET). 2026-07-15. Grounded on `origin/main` `4f51112`.**
Home: **`prro-fleet-contract`** (+ a new **`prro-fleet-agent`** crate, §4). Rev 2 closes three
load-bearing errors the audit found: (1) the rev-1 telemetry was **non-authoritative** — the "rich"
`node_state` columns are **schema slots with zero Rust producers**; (2) read-only-by-handle was
**incomplete** — the barrier must be at the **crate-DAG** level; (3) deferring the **command-lifecycle
contract** conflicts with the locked plan (§3.10) — the lifecycle is authored **now as DORMANT** (zero
callers), advisory-only at **runtime**. Scope = **authoritative telemetry projection (pilot,
read-only)** + **dormant command-lifecycle contract**; command runtime-activation is post-pilot (after
CS-4 + CS-5). Fleet = ADVISORY-only per the ratified pivot.

---

## 0 · Thesis
The pilot fleet is a **view computed from AUTHORITATIVE truth**, not a re-read of `node_state` slots; it
is **read-only enforced at the crate boundary** (the agent cannot even *name* a mutator); and its
**command lifecycle exists as a dormant, INACTIVE contract** so activation later is wiring, not a
re-cut. Advisory ≠ authority: the local per-FN coordinator is the final arbiter.

## 1 · What EXISTS today — corrected grounding
- `node_state` (`001_baseline.sql:521-548`) holds **live** `mode` (`NodeMode` 7-state, set by `set_mode_*_tx`) and `shift_state`, plus **columns that are SCHEMA SLOTS WITHOUT RUST PRODUCERS**: `readiness_state`/`recovery_stage`/`current_month_offline_seconds`/`last_fs_ping_at`/`current_offline_session_id` have **zero write-sites** (verified: 14 `UPDATE node_state`, none touch them) — they rest at their DDL DEFAULTs (`STARTING`/`BOOT`/`0`/`NULL`). They are **not** health signals today.
- The write-path `NodeMode` "telemetry" field is **discarded** by consumers — `PostSignRoute::Online { .. }` drops it (`inline.rs:909`, `boot_phase.rs:3682/3942`); it is not a live emitted signal.
- **The authoritative offline budget is computed, not stored:** `time_budget::compute_budgets_for_fn` (`time_budget.rs:276-335`) derives the 168h/mo + 36h-continuous budgets **from `offline_sessions`** (0 when no sessions) — `node_state.current_month_offline_seconds` is a non-authoritative slot; reading it would show `0` while admission already refuses.
- **Admin is NOT structurally CLI-only and does NOT deadlock:** `go_offline`/`go_online`/`reset_stop_mode` are **public** and take a plain `SqlitePool`; the singleton lock is **fail-fast** (`singleton.rs`: `file.try_lock_exclusive().map_err(|_| …)` → immediate error, not a hang). `open_pool` is a normal **read-write** pool with migrations (`db/mod.rs:123-136`), not a SQLite read-only handle.
- Node-local ops-loops (`supervisor.rs`) iterate only this node's own `BindingsRegistry`; **no PULL / control-plane**.
- **Zero fleet command representation in Rust** (no `ReceivedDurable/Applied/Rejected/Deferred`, epoch, signature, or command inbox). "epoch" in the source = the DPS Kyiv-date encoding (unrelated).

## 2 · Scope
- **Pilot (active):** an **authoritative** `FleetTelemetryProjection` + an **async read-only** `FleetTelemetrySource` port + the `prro-fleet-agent` crate-DAG barrier. No live commands.
- **Dormant (authored now, zero callers, INACTIVE):** the **command-lifecycle contract** (`FleetCommand*`, signed, epoch-versioned, PULL) — required by the locked plan §3.10:165-189 + crate-map:218-219 + roadmap CS-2. **Runtime activation** is post-pilot, only after CS-4 (local final arbiter) + CS-5 (legality table).

## 3 · Key types (`prro-fleet-contract`, sqlx-free)
```rust
// AUTHORITATIVE projection — each field carries its own authority + freshness, NOT a raw node_state slot.
struct FleetTelemetryProjection {
    fiscal_number: FiscalNumber,
    mode: Stamped<NodeMode>,                 // node_state (live)
    shift_state: Stamped<ShiftState>,        // node_state (live)
    offline_budget: OfflineBudgetView,       // from time_budget::compute_budgets_for_fn, NOT node_state
    liveness: LivenessView,                  // split: last real fiscal ACK vs last probe response
    current_offline_session_id: Option<OfflineSessionId>,
    // readiness_state/recovery_stage are UNPOPULATED slots today — projected ONLY if/when producers land,
    // and flagged `Unpopulated` meanwhile (never presented as a real health signal).
}
struct Stamped<T> { value: T, authority_source: AuthoritySource, observed_at: String, freshness: Freshness }
struct OfflineBudgetView { month_used: i64, month_limit: i64, session_used: i64, session_limit: i64,
                           enforcement_active: bool, observed_at: String, source: AuthoritySource /* =OfflineSessionsComputed */ }
struct LivenessView { last_fiscal_ack: Option<Stamped<()>>, last_probe_response: Option<Stamped<()>> }

// ASYNC read-only port. The node-side adapter holds the pool PRIVATELY; only this trait object crosses
// to the agent. (Sync would force blocking sqlx.)
#[async_trait] trait FleetTelemetrySource {
    async fn project(&self, fn_: &FiscalNumber) -> Result<FleetTelemetryProjection, FleetReadError>;
    async fn project_all(&self) -> Result<Vec<FleetTelemetryProjection>, FleetReadError>;
}

// DORMANT command lifecycle (zero callers in the pilot; INACTIVE). Locked plan §3.10.
enum FleetCommandState { ReceivedDurable, Applied, Rejected, Deferred }
struct FleetCommandProvenance { command_id: CommandId, epoch: FleetEpoch, signer_key_id: KeyId,
                                signature_digest: [u8; 32], coordinator_outcome: Option<CoordinatorOutcome> }
struct FleetCommand { /* kind, target FN, epoch, signature */ provenance: FleetCommandProvenance }
```

## 4 · Normative invariants
- **I1 (read-only at the CRATE-DAG — the real barrier).** A **separate `prro-fleet-agent` crate**; a **cargo-metadata allowlist pin** (primary) proves its normal+build dependency closure contains **`prro-fleet-contract` only** — **never `prro` / `prro-store-sqlite` / `sqlx` / the admin module**. The pool lives inside a **private node-side adapter**; only the `FleetTelemetrySource` **trait object** crosses the boundary. A handle-only restriction is insufficient (the agent could otherwise import `prro::admin::*`, call `App::db()` (`app.rs:903`), `with_immediate(pool, …)` (`tx.rs:118`), or raw SQL on the RW `open_pool`).
- **I2 (advisory ≠ authority).** The local per-FN coordinator is the final arbiter; a projection is a view, never a control input. Fleet policy may never force an illegal offline / cap-breach / return-block.
- **I3 (no admin reuse — fail-fast, not deadlock).** A future in-process apply path routes through the **coordinator's mailbox/API**, not the CLI `admin.rs` entrypoints (which **fail-fast** on the singleton `try_lock_exclusive`, not hang) — and the supervisor need not be the sole transport.
- **I4 (command runtime-activation after CS-4 + CS-5).** The dormant contract is authored now; **wiring** a live apply/emit path waits for the coordinator + the legality table.
- **I5 (MUST — separate INACTIVE fleet inbox).** A durable command inbox MUST be a **separate table** (idempotency-keyed, worker-triggered) with epoch + signature + fleet-provenance columns, landed INACTIVE-first — it **must not** overload `ingress_inbox`.
- **I6 (typed provenance).** Fleet-originated actions carry a **typed** `FleetCommandProvenance` (`command_id`, `epoch`, `signer_key_id`, `signature_digest`, `coordinator_outcome`) in `audit_log` — not arbitrary JSON, and distinct from local `ADMIN_*` attribution.
- **I7 (projection ∉ control path).** `FleetTelemetryProjection` appears in **no** admission / transition-oracle signature — a structural pin so a view can never become a control input.

## 5 · RED-pins
- **RP5-1 (crate-DAG read-only — primary):** a cargo-metadata pin proves `prro-fleet-agent`'s dependency closure excludes `prro`/`prro-store-sqlite`/`sqlx`/admin (mirrors the CS-1 purity gate); a `trybuild` canary that the port cannot yield a `WriteTxConn` (secondary).
- **RP5-2 (authoritative telemetry, not slots):** the projection's offline budget equals `time_budget::compute_budgets_for_fn`, **not** `node_state.current_month_offline_seconds` — a test where the column reads `0` but the computed budget is non-zero (an open offline session) proves the authoritative source; readiness/recovery are flagged `Unpopulated`, never a fake `READY`.
- **RP5-3 (dormant lifecycle, INACTIVE):** the `FleetCommand*` types exist in `prro-fleet-contract` but have **zero live callers / no wired apply or emit path** (a static pin) — dormant, per the CS-1 skeleton discipline.
- **RP5-4 (advisory takes no lease):** the projection read acquires no write lease and never blocks a live write-path lease (a concurrency test).
- **RP5-5 (projection ∉ oracle):** a static pin that no admission/transition-oracle signature names `FleetTelemetryProjection`.

## 6 · Decisions (from the audit)
- **Projection source:** a **separate read-only query/adaptor** — do **not** widen the hot `NodeStateRow`/`get_tx`.
- **168h/36h budget:** from `time_budget::compute_budgets_for_fn` (`offline_sessions`), carrying `{month_used, month_limit, session_used, session_limit, enforcement_active, observed_at, source}` — never the `node_state` slot.
- **Liveness:** split `last_probe_response` from the last real **fiscal ACK**; both stamped with freshness; do not label a bare ping "DPS forward progress".
- **Audit aggregates:** **not** in the base snapshot; a separate bounded read-model later, and RMR/stuck counts come from **authoritative tables**, not reconstructed from `audit_log`.
- **Push seam:** an **async in-process port** now; there is no `/metrics` fleet endpoint in the baseline; the emitter/agent server is **CS-6**.
- **N=1:** ship the agent **OFF/advisory**; "config-only enable" becomes true only once the CS-6 emitter/agent exists.

## 7 · Open questions for re-audit
1. **Dormant-lifecycle depth:** is authoring the `FleetCommand*` types + I5/I6 (zero callers) enough to satisfy the locked plan §3.10 now, or does the plan expect the full PULL/epoch state-machine semantics locked too (vs a later Spec #5B)?
2. **`prro-fleet-agent` crate now vs CS-6:** create the empty crate + the cargo-metadata allowlist pin in CS-2 (like CS-1d contract crates), or only pin the boundary in the contract and add the agent crate at CS-6?
3. **`Stamped`/`AuthoritySource` placement:** do these value types belong in `prro-domain` (shared) or `prro-fleet-contract`?
4. **Liveness source:** which durable rows carry the "last real fiscal ACK" and "last probe response" (so the projection reads authoritative freshness, not a slot)?
