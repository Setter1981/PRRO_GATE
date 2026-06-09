# RS-1 Piece 5 — runtime supervisor / Serve wiring / graceful shutdown (REVISED)

**Date:** 2026-05-30 · **Branch:** `feat/rs1-runtime-supervisor` · supersedes the Piece-5 section of `2026-05-30-rs1-runtime-supervisor.md`.
**Revision driver:** the H1 finding (`fn_sign` must be FRESH per-tick, not boot-cached — [[project-rs1-fn-sign-freshness]]) invalidates the original "build fn_sign into a boot HashMap" assumption. The rest of the original Piece-5 design holds.

## Crux resolved — fn_sign freshness (verified against the existing loops)

The existing consumers all take `fn_sign` **per call**, so the supervisor can rebuild it fresh per tick with NO change to their APIs:
- `App::drain_offline_backlog_scheduled(fn, deps: &RuntimeView)` — `&RuntimeView` per call (`app.rs:654`).
- `return_online_probe::run_tick_for_fn(pool, dps, fn_sign: &CheckSignBlob, …)` — `&fn_sign` per call (`return_online_probe.rs:222`). The probe signs `status_rro(fn_sign)` (`:290`), so it genuinely needs fresh.
- `App::reconcile_pending_with(deps: ReconciliationRuntime)` — one-shot; a resolver builds the per-FN `RuntimeView` (`app.rs:477`).
- Source of truth per FN: `BindingsRegistry::get(fn) -> &OperatorBindings { dps, sign_ctx }`; `sign_ctx.session` is `pub` (`stage_sign.rs:68`) → `build_fn_sign(&bindings.sign_ctx.session, fn)`.

**The ONLY stale path is the boot-freezing wrappers** `App::spawn_return_online_probe` / `spawn_probe_loop` (freeze `Vec<ProbeSpec>.fn_sign` at spawn, `return_online_probe.rs:447,479`). **Design decision: the supervisor does NOT use the spawn-wrappers; it owns its own tick loops** that, each tick, rebuild `fn_sign` fresh per FN and call the existing per-call logic (`run_tick_for_fn`, `drain_offline_backlog_scheduled`). Blast radius: ZERO change to the tested per-call logic; the supervisor reimplements only the ~10-line interval+biased-select-shutdown scaffold (mirroring `return_online_probe.rs:459-461`). (Alternative — change `ProbeSpec`/`spawn_probe_loop` to carry the session + rebuild — was rejected: bigger blast radius on tested code.)

**Lifetime reconciliation** (operator pinned `Arc<…>` for `'static` spawned tasks): the ticker holds `Arc<BindingsRegistry>` + `Arc<dyn DpsChannel>` + the `App` handle (all `'static`). `fn_sign` is a per-tick LOCAL rebuilt inside the tick scope; the `RuntimeView` borrows that local + the session from the `Arc<registry>` and lives only across the tick's `.await` — never stored, never `'static`. So per-tick freshness and the `'static` task bound coexist cleanly.

> **SELF-REVIEW CORRECTION (2026-05-30) — fn_sign lifetime.** A helper of shape `fresh_runtime_view(reg, fn) -> RuntimeView` (or a `with_resolver` closure that builds `fn_sign` internally) **does NOT compile**: `RuntimeView<'a>` borrows `fn_sign`, so a locally-built blob returned by-ref dangles. The blob MUST live in the CONSUMING scope:
> - **Tickers (5d):** the tick body does `let fn_sign = build_fn_sign(&sess, fn)?;` then assembles `RuntimeView { … fn_sign: &fn_sign }` INLINE and calls drain/probe in the same scope. (`build_fn_sign` already exists — no `fresh_runtime_view` helper; 5b is dropped as a standalone piece and folded here.)
> - **Reconcile-once (5c):** PRE-BUILD a `Vec`/map of fresh `CheckSignBlob` for all FNs into a local that outlives the `reconcile_pending_with` call; the `with_resolver` closure borrows from that store (one-shot → signingTime is fresh at that moment).
>
> **Other self-review corrections:** (a) 5d's "own ticker" is bigger than ~10 lines — it must replicate `spawn_return_online_probe`'s FN-enumeration + interval clamp + skip-no-signer + per-FN non-fatal error handling, not just the interval+shutdown select; weigh again vs modifying `spawn_probe_loop` to rebuild `fn_sign` per tick. (b) confirm **node_state is seeded** (boot_phase `upsert_initial` via reconcile-once) for every configured FN BEFORE the drain/probe loops read `node_state.mode`/`shift_state` — else seed explicitly in 5a. (c) add a **`supervisor.drain_interval_seconds`** config knob (clamp+audit, like the probe interval) — it was owed but not landed in Piece 1; add in 5a, consume in 5d. (d) 5e: bound shutdown join with a deadline OR rely on `dps.request_timeout` to cap a mid-tick RPC so the join can't hang.

## Sub-pieces (small vertical slices)

| # | Piece | Files / seam | Test | Invariant note |
|---|-------|--------------|------|----------------|
| **5a** | Supervisor module + Serve gate + deps construction | new `runtime/supervisor.rs`; `main.rs:359-369` calls `supervisor::run(app, &config)` iff `config.supervisor.enabled`, else the existing idle. `run` = `require_dps_endpoint()?` → `GrpcDpsChannel::connect(endpoint, timeout)` (hard-fail) → `Arc<dyn DpsChannel>` → `BindingsRegistry::build_from_db(db_secure, db, dps, &JksOperatorKeyLoader)` (`bindings.rs:179`) → `Arc<BindingsRegistry>` | `enabled=false` → byte-identical M1-idle (no behavior change); `enabled=true`+valid → builds registry; bad endpoint when enabled → fail-closed | #10 (gated, explicit) |
| **5b** | Per-tick deps builder (fn_sign FRESHNESS core) | helper `fresh_runtime_view(reg, fn) -> RuntimeView` rebuilding `fn_sign` via `build_fn_sign(&reg.get(fn).sign_ctx.session, fn)` | two calls → both well-formed, fresh `signingTime` (not the same cached blob) | the H1 fix made concrete |
| **5c** | Reconcile-once at startup | `ReconciliationRuntime::with_resolver` (`runtime.rs:121`) whose closure calls `fresh_runtime_view`; `app.reconcile_pending_with(deps)` ONCE before spawning loops, under `reconcile_mutex` | reconcile runs once at supervisor start | #2 global single-writer (mutex held); runs before live loops |
| **5d** | Drain + probe tickers (per-tick rebuild + biased shutdown) | the supervisor spawns 2 `tokio::time::interval` loops (drain cadence + probe interval); each tick, per FN: `fresh_runtime_view` → `drain_offline_backlog_scheduled` / `run_tick_for_fn`; `tokio::select!{ biased; shutdown_rx.changed() => break; tick }` | ticker runs N ticks then stops on shutdown flip; fn_sign rebuilt each tick | #1 (drain/probe run crypto+wire OUTSIDE any tx, as today) |
| **5e** | Graceful shutdown | one `watch::channel(false)`; thread `Receiver` into each ticker; on `await_shutdown_signal()` (`main.rs:366`) flip Sender → **join ALL `JoinHandle`s** → then `drop(app)` | no task outlives the watch flip; join completes | **#9 graceful shutdown** (the load-bearing one) |
| **5f** | Boot-to-shutdown integration test | new `tests/rs1_supervisor_boot.rs`; inject a stub `DpsChannel` + a test registry; assert: reconcile-once ran, tickers spawned + ticked, shutdown joins cleanly, no orphan | end-to-end supervisor lifecycle | proves #9 + #2 ordering |

**Riskiest: 5d + 5e** — the only run-loop mutation + the sole guardian of #9; owns the `'static` task ownership. A bug = orphaned tasks past `drop(app)` or a join hang. **Review cadence:** mid-review after 5c (deps + reconcile seam), focused FINAL on 5d+5e (shutdown + task ownership) — likely an external round given the multi-round pattern.

## What Piece 5 does NOT do
Still **no ingress HTTP server** and **no live write-path worker** driving `stage_acquire` on fresh inbox rows — those are **RS-2 / RS-3**. After Piece 5 the binary (with `supervisor.enabled=true`) boots → builds per-FN deps → runs crash-recovery once → runs the drain + return-online loops with FRESH per-tick `fn_sign` → shuts down gracefully. It does not yet accept a live receipt.
