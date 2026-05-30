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
            .map_err(|e| {
                anyhow::anyhow!("supervisor: DPS connect to {endpoint} failed: {e:?}")
            })?,
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
///      registry's FNs; per-FN tick failures are logged, never fatal;
///   3. (5e) await `shutdown`, flip the watch, then JOIN both loops before
///      dropping the [`App`] — no in-flight tick is cut (graceful-shutdown
///      invariant #9; bounded by the DPS `request_timeout` the channel
///      enforces on each wire call).
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

    // 5e: await the external shutdown signal, then stop the loops.
    shutdown.await;
    tracing::info!(target: "prro::runtime::supervisor", "supervisor: shutdown signal received; stopping loops");

    // Flip the watch → each loop exits at its next biased select poll.  We do
    // NOT abort: a loop finishes its in-flight tick (bounded by the DPS
    // request_timeout) so a drain is never cut mid-flight (invariant #9).
    let _ = shutdown_tx.send(true);
    if let Err(e) = drain_handle.await {
        tracing::error!(target: "prro::runtime::supervisor", error = %e, "drain loop join failed (task panicked)");
    }
    if let Err(e) = probe_handle.await {
        tracing::error!(target: "prro::runtime::supervisor", error = %e, "probe loop join failed (task panicked)");
    }

    tracing::info!(target: "prro::runtime::supervisor", "supervisor: shut down");
    drop(app);
    Ok(())
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
                    drain_tick(&app, &registry, &fn_ids).await;
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
async fn drain_tick(app: &App, registry: &BindingsRegistry, fn_ids: &[String]) {
    for fn_id in fn_ids {
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
                    probe_tick(&app, &registry, &fn_ids).await;
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
async fn probe_tick(app: &App, registry: &BindingsRegistry, fn_ids: &[String]) {
    for fn_id in fn_ids {
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
