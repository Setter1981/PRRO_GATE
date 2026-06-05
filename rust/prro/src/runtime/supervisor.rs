//! RS-1 Piece 5 — the runtime supervisor / composition root.
//!
//! `Cmd::Serve` calls [`run`] ONLY when `config.supervisor.enabled` (default
//! false — the M1-idle rollback seam).  The supervisor:
//!   1. validates the DPS endpoint (fail-closed) + opens the live channel;
//!   2. builds the per-FN [`BindingsRegistry`] (operator EDS keys);
//!   3. (5c) runs boot crash-recovery once under the global reconcile mutex;
//!   4. (5d) spawns the drain + return-online tick loops, each rebuilding
//!      `fn_sign` FRESH per tick (signingTime freshness — see
//!      [`crate::runtime::key_loader::build_fn_sign`]);
//!   5. (5e) on the shutdown signal, flips a watch + joins all tasks before
//!      dropping the [`App`] (graceful-shutdown invariant #9).
//!
//! It does NOT run an ingress server or a live write-path worker — those are
//! RS-2 / RS-3.  With `enabled = false` the binary stays byte-identical M1-idle.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::app::App;
use crate::db::models::enums::Severity;
use crate::db::repositories::audit_log;
use crate::runtime::bindings::BindingsRegistry;
use crate::runtime::key_loader::{build_fn_sign, JksOperatorKeyLoader};
use crate::services::offline_sync::return_online_probe::run_tick_for_fn;
use crate::services::reconciliation::{ReconciliationRuntime, RuntimeView};
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::CheckSignBlob;
use crate::transports::dps::grpc::GrpcDpsChannel;

/// Production entry — build the live deps, then run the supervised loops.
/// Called from `Cmd::Serve` when `app.config().supervisor.enabled` is true.
///
/// `shutdown` is awaited by the supervisor; when it resolves the loops are
/// flipped off and joined before this returns (`Cmd::Serve` passes the
/// SIGINT/SIGTERM future).  Connect failure is a HARD boot failure (operator
/// decision: an enabled supervisor must reach DPS; this is the online wire
/// path, not an offline fallback).
pub async fn run<F>(app: App, shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send,
{
    let cfg = app.config().supervisor.clone();

    // Fail-closed: an enabled supervisor MUST have an explicit DPS endpoint.
    let endpoint = match cfg.require_dps_endpoint()? {
        Some(ep) => ep,
        // Defensive — `Cmd::Serve` only calls `run` when enabled, but never
        // panic on a production path.
        None => anyhow::bail!("supervisor::run called with supervisor.enabled = false"),
    };

    let (timeout_secs, was_clamped) = cfg.dps.clamped_request_timeout_seconds();
    if was_clamped {
        tracing::warn!(
            target: "prro::runtime::supervisor",
            raw = cfg.dps.request_timeout_seconds,
            clamped = timeout_secs,
            "supervisor.dps.request_timeout_seconds out of bounds; clamped"
        );
    }

    // Open the live DPS channel — eager connect, hard-fail on a bad endpoint.
    let dps: Arc<dyn DpsChannel> = Arc::new(
        GrpcDpsChannel::connect(&endpoint, Duration::from_secs(timeout_secs))
            .await
            .map_err(|e| anyhow::anyhow!("supervisor: DPS connect to {endpoint} failed: {e:?}"))?,
    );

    // Build the per-FN bindings (operator EDS keys) from the secure DB.
    let registry = Arc::new(
        BindingsRegistry::build_from_db(
            app.db_secure(),
            app.db(),
            Arc::clone(&dps),
            &JksOperatorKeyLoader,
        )
        .await?,
    );

    run_with_registry(app, registry, shutdown).await
}

/// Injectable seam — run the supervised loops over a PRE-BUILT registry.
/// Tests construct a stub-channel registry + a controllable `shutdown`,
/// bypassing the live `GrpcDpsChannel::connect`.
///
/// Lifecycle:
///   1. (5c) boot crash-recovery ONCE under the global reconcile mutex;
///   2. (5d) spawn the drain + return-online tick loops — each rebuilds
///      `fn_sign` FRESH per tick (signingTime freshness) and iterates the
///      registry's FNs; a tick bails between FNs on shutdown (F2), and per-FN
///      tick failures are logged, never fatal;
///   3. (5e/F1) hand off to [`supervise_until_shutdown`], which waits on the
///      `shutdown` future AND both loop handles: a normal shutdown flips the
///      watch and JOINs both loops within the configured grace before dropping
///      the [`App`]; a loop dying first (panic) → CRITICAL audit + `Err` for an
///      orchestrator restart.
pub async fn run_with_registry<F>(
    app: App,
    registry: Arc<BindingsRegistry>,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send,
{
    tracing::info!(
        target: "prro::runtime::supervisor",
        operators = registry.len(),
        "supervisor: running"
    );

    // ── 5c: boot crash-recovery ONCE, before any live loop ──
    // A FRESH per-FN fn_sign for this one-shot pass (build_fn_sign stamps
    // signingTime = now()).  Held in a LOCAL that outlives the reconcile
    // call so the resolver can borrow from it — a fn_sign built INSIDE the
    // closure would dangle (the RuntimeView<'a> borrows it).
    let fn_signs = build_fn_signs(&registry, "reconcile");
    {
        let resolver = |fn_id: &str| -> Option<RuntimeView<'_>> {
            let b = registry.get(fn_id)?;
            let s = fn_signs.get(fn_id)?;
            Some(RuntimeView {
                dps: b.dps.as_ref(),
                signing_ctx: &b.sign_ctx,
                fn_sign: s,
            })
        };
        let summary = app
            .reconcile_pending_with(ReconciliationRuntime::with_resolver(resolver))
            .await?;
        tracing::info!(
            target: "prro::runtime::supervisor",
            ?summary,
            "supervisor: boot reconciliation complete"
        );
    }

    // ── 5d: spawn the drain + return-online tick loops ──
    // The FN set is fixed at registry build time (read-only post-boot), so
    // collect the keys ONCE and hand each loop an owned copy — sidesteps the
    // `fns()` borrow vs per-iteration `get()` borrow dance inside the task.
    let fn_ids: Vec<String> = registry.fns().map(|s| s.to_string()).collect();

    let (drain_secs, drain_clamped) = app.config().supervisor.clamped_drain_interval_seconds();
    if drain_clamped {
        tracing::warn!(
            target: "prro::runtime::supervisor",
            raw = app.config().supervisor.drain_interval_seconds,
            clamped = drain_secs,
            "supervisor.drain_interval_seconds out of bounds; clamped"
        );
    }
    let (probe_secs, probe_clamped) = app.config().offline.clamped_probe_interval_seconds();
    if probe_clamped {
        tracing::warn!(
            target: "prro::runtime::supervisor",
            raw = app.config().offline.return_online_probe_interval_seconds,
            clamped = probe_secs,
            "offline.return_online_probe_interval_seconds out of bounds; clamped"
        );
    }

    // One watch channel fans the shutdown signal to BOTH loops; `biased`
    // select makes the shutdown branch win even when a tick is ready.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let drain_handle = spawn_drain_loop(
        app.clone(),
        Arc::clone(&registry),
        fn_ids.clone(),
        Duration::from_secs(drain_secs),
        shutdown_rx.clone(),
    );
    let probe_handle = spawn_probe_loop(
        app.clone(),
        Arc::clone(&registry),
        fn_ids,
        Duration::from_secs(probe_secs),
        shutdown_rx,
    );

    tracing::info!(
        target: "prro::runtime::supervisor",
        drain_interval_secs = drain_secs,
        probe_interval_secs = probe_secs,
        "supervisor: drain + return-online loops running"
    );

    // F1 + F2: hand the wait/teardown lifecycle to the (test-injectable)
    // supervise core.
    supervise_until_shutdown(app, shutdown, shutdown_tx, drain_handle, probe_handle).await
}

/// The supervisor's wait + teardown lifecycle (F1), factored out as a test
/// seam.  Watches the external `shutdown` future AND both loop handles:
///   - normal shutdown → flip the watch, join both loops within the configured
///     grace, drop the App, `Ok`;
///   - a loop completing BEFORE shutdown → an invariant-bug PANIC (operational
///     errors are caught inside the tick bodies), so emit a CRITICAL
///     `SUPERVISOR_LOOP_DIED` audit, wind down the sibling, drop the App, and
///     return `Err` — `Cmd::Serve` propagates it, the process exits non-zero,
///     and the process supervisor (systemd `Restart=on-failure` / docker
///     `restart: on-failure`) re-launches.  Boot reconcile + the crash-safe
///     W9b drain make the restart safe.
///
/// `biased` keeps a normal shutdown from being misread as a task death when
/// both are ready.  Integration tests inject a panicking handle here to drive
/// the loop-death path without a real tick panic.
pub async fn supervise_until_shutdown<F>(
    app: App,
    shutdown: F,
    shutdown_tx: watch::Sender<bool>,
    mut drain_handle: JoinHandle<()>,
    mut probe_handle: JoinHandle<()>,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send,
{
    let wake = {
        tokio::pin!(shutdown);
        tokio::select! {
            // `biased`: shutdown is polled first, so a normal shutdown is never
            // misread as a loop death.  In the rare poll where a loop panics AT
            // THE SAME TIME as shutdown resolves, Shutdown wins and the
            // SUPERVISOR_LOOP_DIED audit is suppressed — benign: the process is
            // already going down on operator request, and the panic's effects
            // are crash-equivalent (re-drained next boot).
            biased;
            () = &mut shutdown => Wake::Shutdown,
            res = &mut drain_handle => Wake::LoopDied { which: "drain", res },
            res = &mut probe_handle => Wake::LoopDied { which: "probe", res },
        }
    };

    // Flip the watch → the still-running loop(s) exit at their next biased
    // select poll, and the in-flight tick bails between FNs (F2).
    let _ = shutdown_tx.send(true);
    let (grace_secs, grace_clamped) = app.config().supervisor.clamped_shutdown_grace_seconds();
    if grace_clamped {
        tracing::warn!(
            target: "prro::runtime::supervisor",
            raw = app.config().supervisor.shutdown_grace_seconds,
            clamped = grace_secs,
            "supervisor.shutdown_grace_seconds out of bounds; clamped"
        );
    }
    let grace = Duration::from_secs(grace_secs);

    let result = match wake {
        Wake::Shutdown => {
            tracing::info!(target: "prro::runtime::supervisor", "supervisor: shutdown signal received; stopping loops");
            // Join BOTH loops concurrently under ONE shared deadline (not two
            // sequential graces) so total shutdown wall-clock is bounded by a
            // single `grace` — matching the runbook's `TimeoutStopSec > grace`
            // contract (1×, not 2×).
            join_both_with_grace(drain_handle, probe_handle, grace).await;
            tracing::info!(target: "prro::runtime::supervisor", "supervisor: shut down");
            Ok(())
        }
        Wake::LoopDied { which, res } => {
            audit_loop_died(&app, which, res).await;
            // Wind down + join the SIBLING (the dead one is already consumed).
            let sibling = if which == "drain" {
                probe_handle
            } else {
                drain_handle
            };
            let sibling_name = if which == "drain" { "probe" } else { "drain" };
            join_with_grace(sibling, sibling_name, grace).await;
            Err(anyhow::anyhow!(
                "supervisor: {which} loop exited before shutdown — failing for orchestrator restart (see SUPERVISOR_LOOP_DIED audit)"
            ))
        }
    };

    drop(app);
    result
}

/// Discriminates why [`supervise_until_shutdown`] woke from its wait: a normal
/// external shutdown, or a tick loop dying first (an invariant-bug panic).
enum Wake {
    Shutdown,
    LoopDied {
        which: &'static str,
        res: Result<(), tokio::task::JoinError>,
    },
}

/// Join one loop handle, bounded by the shutdown grace.  On grace-elapse we
/// log + proceed (the handle is detached, not aborted): the per-doc W9b drain
/// is crash-safe, so an in-flight tick cut at process exit is crash-equivalent
/// and re-drained on next boot.  Used on the loop-death path (join the one
/// surviving sibling).
async fn join_with_grace(handle: JoinHandle<()>, name: &str, grace: Duration) {
    match tokio::time::timeout(grace, handle).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::error!(
            target: "prro::runtime::supervisor",
            loop_name = name,
            error = %e,
            "loop join failed (task panicked)"
        ),
        Err(_elapsed) => tracing::warn!(
            target: "prro::runtime::supervisor",
            loop_name = name,
            grace_secs = grace.as_secs(),
            "loop did not finish within the shutdown grace; proceeding (per-doc drain is crash-safe)"
        ),
    }
}

/// Join BOTH loops concurrently under a SINGLE shared `grace` deadline (used on
/// the normal-shutdown path).  Sequential per-loop graces would make worst-case
/// shutdown `2 × grace`, breaking the runbook's `TimeoutStopSec > grace` (1×)
/// contract; one shared deadline keeps total shutdown bounded by one grace.  On
/// elapse both handles detach (crash-safe — see [`join_with_grace`]).
async fn join_both_with_grace(drain: JoinHandle<()>, probe: JoinHandle<()>, grace: Duration) {
    match tokio::time::timeout(grace, async { tokio::join!(drain, probe) }).await {
        Ok((d, p)) => {
            if let Err(e) = d {
                tracing::error!(target: "prro::runtime::supervisor", loop_name = "drain", error = %e, "loop join failed (task panicked)");
            }
            if let Err(e) = p {
                tracing::error!(target: "prro::runtime::supervisor", loop_name = "probe", error = %e, "loop join failed (task panicked)");
            }
        }
        Err(_elapsed) => tracing::warn!(
            target: "prro::runtime::supervisor",
            grace_secs = grace.as_secs(),
            "loops did not finish within the shutdown grace; proceeding (per-doc drain is crash-safe)"
        ),
    }
}

/// Emit the durable CRITICAL `SUPERVISOR_LOOP_DIED` audit when a tick loop
/// exits before shutdown.  Panic-guarded (best-effort) like the probe-tick
/// audit contract — if the audit insert itself fails we fall back to tracing.
async fn audit_loop_died(app: &App, which: &str, res: Result<(), tokio::task::JoinError>) {
    let panicked = res.as_ref().err().map(|e| e.is_panic()).unwrap_or(false);
    let detail = match &res {
        Ok(()) => "task returned unexpectedly (loops run until shutdown)".to_string(),
        Err(e) => format!("{e}"),
    };
    tracing::error!(
        target: "prro::runtime::supervisor",
        loop_name = which,
        panicked,
        detail = %detail,
        "supervisor: loop died before shutdown — failing the supervisor for orchestrator restart"
    );
    let payload = serde_json::json!({
        "loop": which,
        "panicked": panicked,
        "detail": detail,
    });
    if let Err(audit_err) = audit_log::append(
        app.db(),
        "supervisor",
        which,
        "SUPERVISOR_LOOP_DIED",
        Severity::Critical,
        None,
        Some(&payload.to_string()),
    )
    .await
    {
        tracing::error!(
            target: "prro::runtime::supervisor",
            audit_error = %audit_err,
            loop_name = which,
            "supervisor: CRITICAL SUPERVISOR_LOOP_DIED audit insert failed"
        );
    }
}

/// Build a fresh per-FN `fn_sign` map.  `build_fn_sign` stamps
/// `signingTime = now()`, so this is rebuilt for EACH consuming pass (the
/// one-shot reconcile here; per-tick by the loops in 5d) — NEVER cached for
/// the process lifetime.  An FN whose `fn_sign` build fails is skipped
/// (logged) so one bad key defers that FN rather than killing the pass.
fn build_fn_signs(registry: &BindingsRegistry, ctx: &str) -> HashMap<String, CheckSignBlob> {
    let mut map = HashMap::new();
    for fn_id in registry.fns() {
        if let Some(b) = registry.get(fn_id) {
            match build_fn_sign(&b.sign_ctx.session, fn_id) {
                Ok(blob) => {
                    map.insert(fn_id.to_string(), blob);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "prro::runtime::supervisor",
                        fiscal_number = fn_id,
                        ctx,
                        error = ?e,
                        "fn_sign build failed; FN skipped this pass"
                    );
                }
            }
        }
    }
    map
}

/// Spawn the offline-backlog drain ticker — one tokio task iterating every
/// registered FN each `interval`.  `MissedTickBehavior::Skip` keeps a slow
/// drain from queueing missed ticks; `biased` select makes shutdown win.
fn spawn_drain_loop(
    app: App,
    registry: Arc<BindingsRegistry>,
    fn_ids: Vec<String>,
    interval: Duration,
    mut shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // A second receiver for the between-FN check inside drain_tick — the
        // select! below holds `&mut shutdown_rx` for its `changed()` arm, so
        // the tick body cannot borrow the same receiver.
        let tick_shutdown = shutdown_rx.clone();
        let mut iv = tokio::time::interval(interval);
        iv.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!(target: "prro::runtime::supervisor", "drain loop: shutdown; exiting");
                        return;
                    }
                }
                _ = iv.tick() => {
                    drain_tick(&app, &registry, &fn_ids, &tick_shutdown).await;
                }
            }
        }
    })
}

/// One drain pass over all FNs.  Rebuilds `fn_sign` FRESH per FN (signingTime
/// must be current at the wire call — NEVER boot-cached) and routes through
/// the backoff-gated [`App::drain_offline_backlog_scheduled`].  A per-FN
/// `fn_sign` build failure or drain error is logged and skipped — one bad FN
/// never stops the others or kills the loop.
async fn drain_tick(
    app: &App,
    registry: &BindingsRegistry,
    fn_ids: &[String],
    shutdown: &watch::Receiver<bool>,
) {
    for fn_id in fn_ids {
        // F2: bail BETWEEN FNs so a shutdown during a long multi-FN pass is
        // honored promptly instead of only after the whole pass returns.  The
        // residual unboundedness inside a single FN's per-doc drain is bounded
        // by the supervisor's grace-timeout join (crash-safe to cut).
        if *shutdown.borrow() {
            return;
        }
        let Some(b) = registry.get(fn_id) else {
            continue;
        };
        let fn_sign = match build_fn_sign(&b.sign_ctx.session, fn_id) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    target: "prro::runtime::supervisor",
                    fiscal_number = fn_id,
                    error = ?e,
                    "drain tick: fn_sign build failed; FN skipped this tick"
                );
                continue;
            }
        };
        let view = RuntimeView {
            dps: b.dps.as_ref(),
            signing_ctx: &b.sign_ctx,
            fn_sign: &fn_sign,
        };
        match app.drain_offline_backlog_scheduled(fn_id, &view).await {
            Ok(outcome) => {
                tracing::debug!(
                    target: "prro::runtime::supervisor",
                    fiscal_number = fn_id,
                    ?outcome,
                    "drain tick complete"
                );
            }
            Err(e) => {
                tracing::error!(
                    target: "prro::runtime::supervisor",
                    fiscal_number = fn_id,
                    error = ?e,
                    "drain tick failed"
                );
            }
        }
    }
}

/// Spawn the return-online probe ticker — one tokio task iterating every
/// registered FN each `interval`.  Same skip/biased discipline as the drain
/// loop.
fn spawn_probe_loop(
    app: App,
    registry: Arc<BindingsRegistry>,
    fn_ids: Vec<String>,
    interval: Duration,
    mut shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Second receiver for the between-FN check inside probe_tick (see
        // spawn_drain_loop).
        let tick_shutdown = shutdown_rx.clone();
        let mut iv = tokio::time::interval(interval);
        iv.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!(target: "prro::runtime::supervisor", "probe loop: shutdown; exiting");
                        return;
                    }
                }
                _ = iv.tick() => {
                    probe_tick(&app, &registry, &fn_ids, &tick_shutdown).await;
                }
            }
        }
    })
}

/// One return-online probe pass over all FNs.  Rebuilds `fn_sign` FRESH per FN
/// then delegates to [`run_tick_for_fn`] (read-only over the wire — flips
/// `Offline → GoingOnline` on a reachable DPS; the W9b drain does the backlog
/// flush + final `GoingOnline → Online`).  Mirrors the W8 return-online probe
/// loop's CRITICAL-audit-on-infra-error contract (see
/// [`crate::services::offline_sync::return_online_probe`]) so a DB/audit
/// failure is durably visible, not merely traced.
async fn probe_tick(
    app: &App,
    registry: &BindingsRegistry,
    fn_ids: &[String],
    shutdown: &watch::Receiver<bool>,
) {
    for fn_id in fn_ids {
        // F2: bail between FNs on shutdown (see drain_tick).
        if *shutdown.borrow() {
            return;
        }
        let Some(b) = registry.get(fn_id) else {
            continue;
        };
        let fn_sign = match build_fn_sign(&b.sign_ctx.session, fn_id) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    target: "prro::runtime::supervisor",
                    fiscal_number = fn_id,
                    error = ?e,
                    "probe tick: fn_sign build failed; FN skipped this tick"
                );
                continue;
            }
        };
        match run_tick_for_fn(app.db(), b.dps.as_ref(), fn_id, &fn_sign).await {
            Ok(_) => {}
            Err(e) => {
                tracing::error!(
                    target: "prro::runtime::supervisor",
                    fiscal_number = fn_id,
                    error = %e,
                    "probe tick error (DB or audit insert failure)"
                );
                let payload = serde_json::json!({
                    "fiscal_number": fn_id,
                    "error": format!("{e:#}"),
                });
                if let Err(audit_err) = audit_log::append(
                    app.db(),
                    "return_online_probe",
                    fn_id,
                    "RETURN_ONLINE_PROBE_LOOP_ERROR",
                    Severity::Critical,
                    None,
                    Some(&payload.to_string()),
                )
                .await
                {
                    tracing::error!(
                        target: "prro::runtime::supervisor",
                        audit_error = %audit_err,
                        fiscal_number = fn_id,
                        "probe tick: CRITICAL audit insert failed; loop continues"
                    );
                }
            }
        }
    }
}
