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
//!   5. (RS-2 piece-5b-ii) runs the ingress startup preflight + binds one axum
//!      HTTP server per `RestHttp` listener ([`bind_rest_ingress`]), supervised
//!      alongside the loops;
//!   6. (5e) on the shutdown signal, flips a watch + joins all tasks before
//!      dropping the [`App`] (graceful-shutdown invariant #9).
//!
//! It runs the ingress servers (RS-2) but NOT the live write-path worker — that
//! is RS-3 (pre-RS-3 the ingress seam is `UnimplementedWritePath` → every
//! receipt 501s).  With `enabled = false` the binary stays byte-identical
//! M1-idle.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use tokio::net::TcpListener;

use crate::app::App;
use crate::config::ListenerKind;
use crate::db::models::enums::{Protocol, Severity};
use crate::db::models::ids::DriverId;
use crate::db::repositories::audit_log;
use crate::runtime::bindings::BindingsRegistry;
use crate::runtime::ingress::preflight::preflight_d1_slots;
use crate::runtime::ingress::seam::{UnimplementedWritePath, WritePathEntry};
use crate::runtime::ingress::server::{self, IngressState};
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

    // Deployment-invariant WARN: a DPS request timeout longer than the
    // shutdown grace means an in-flight drain DPS call can be cut at shutdown
    // before its own deadline (crash-safe — the per-doc drain is idempotent —
    // but it defeats the graceful-drain intent, #9).  Flag it; don't fail.
    let (grace_secs, _) = cfg.clamped_shutdown_grace_seconds();
    if timeout_secs > grace_secs {
        tracing::warn!(
            target: "prro::runtime::supervisor",
            dps_request_timeout_secs = timeout_secs,
            shutdown_grace_secs = grace_secs,
            "DPS request timeout exceeds the shutdown grace — set \
             supervisor.shutdown_grace_seconds >= supervisor.dps.request_timeout_seconds \
             so an in-flight drain call can finish within grace"
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
///   3. (5e/F1) hand off to [`supervise_task_set`] (via the two-task
///      [`supervise_until_shutdown`] wrapper), which waits on the `shutdown`
///      future AND every supervised task: a normal shutdown flips the watch
///      and JOINs ALL tasks within ONE shared grace before dropping the
///      [`App`]; a task dying before the flip → CRITICAL audit + `Err` for an
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

    // ── RS-2 piece-5b-ii: ingress startup preflight + bind (BEFORE any spawn) ──
    // A loopback-guard (D2) / D1-preflight / bind failure here is a BOOT
    // failure — it returns Err before ANY task is spawned, so a misconfigured
    // ingress never half-starts the spine.  Binding before spawn also makes a
    // bind error a boot failure, NOT a runtime loop-death.
    let write_path: Arc<dyn WritePathEntry> = Arc::new(UnimplementedWritePath);
    let bound_ingress = bind_rest_ingress(&app, &write_path).await?;

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
    let (converge_secs, converge_clamped) = app
        .config()
        .supervisor
        .clamped_online_convergence_interval_seconds();
    if converge_clamped {
        tracing::warn!(
            target: "prro::runtime::supervisor",
            raw = app.config().supervisor.online_convergence_interval_seconds,
            clamped = converge_secs,
            "supervisor.online_convergence_interval_seconds out of bounds; clamped"
        );
    }

    // One watch channel fans the shutdown signal to ALL loops; `biased`
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
        fn_ids.clone(),
        Duration::from_secs(probe_secs),
        shutdown_rx.clone(),
    );
    let convergence_handle = spawn_convergence_loop(
        app.clone(),
        Arc::clone(&registry),
        fn_ids,
        Duration::from_secs(converge_secs),
        shutdown_rx.clone(),
    );

    // ── 5b-ii: spawn one axum ingress server per pre-bound RestHttp listener ──
    // Each is a `GracefulOkAfterShutdown` supervised task: its
    // `with_graceful_shutdown` returns `Ok(())` once the watch flips; a serve
    // error / panic before the flip is a loop-death (F1).
    let ingress_count = bound_ingress.len();
    let mut tasks = vec![
        SupervisedTask::runs_until_shutdown("drain", drain_handle),
        SupervisedTask::runs_until_shutdown("probe", probe_handle),
        SupervisedTask::runs_until_shutdown("online-convergence", convergence_handle),
    ];
    for (listener, state, name) in bound_ingress {
        let handle = tokio::spawn(server::serve(listener, state, shutdown_rx.clone()));
        tasks.push(SupervisedTask::graceful(name, handle));
    }

    tracing::info!(
        target: "prro::runtime::supervisor",
        drain_interval_secs = drain_secs,
        probe_interval_secs = probe_secs,
        online_convergence_interval_secs = converge_secs,
        ingress_listeners = ingress_count,
        "supervisor: drain + return-online + online-convergence + ingress tasks running"
    );

    // F1 + F2: hand the wait/teardown lifecycle to the generalized (test-
    // injectable) supervise core over the full named task set.
    supervise_task_set(app, shutdown, shutdown_tx, tasks).await
}

/// RS-2 piece-5b-ii — run the ingress startup preflight, then BIND every
/// `RestHttp` listener.  Done BEFORE any task is spawned so a preflight/bind
/// failure cleanly fails the boot (returns `Err` → process exits non-zero →
/// orchestrator restart), and so a bind error is a BOOT failure rather than a
/// runtime loop-death.  Returns the bound listeners + their per-listener axum
/// [`IngressState`] + supervised-task names, ready to spawn.
///
/// Preflight order (fail-closed):
///   1. D2 loopback-only guard over ALL listeners (a non-loopback `RestHttp`
///      bind refuses to boot until the LAN token-resolver lands);
///   2. main-DB FN existence — every `RestHttp` listener FN MUST be in
///      `fiscal_number_config` (the `ListenerCfg` startup contract; the
///      secure-DB D1 preflight alone cannot vouch for an unconfigured FN);
///   3. per-`RestHttp`-FN D1 frozen-slot preflight (cash baseline + kind drift);
///   4. `driver_id` validation + `TcpListener::bind`.
async fn bind_rest_ingress(
    app: &App,
    write_path: &Arc<dyn WritePathEntry>,
) -> anyhow::Result<Vec<(TcpListener, IngressState, String)>> {
    let listeners = app.config().listeners.clone();

    // (1) D2 loopback-only pilot — refuse a non-loopback RestHttp bind.
    crate::config::validate_rs2_loopback_binds(&listeners)
        .map_err(|e| anyhow::anyhow!("RS-2 ingress loopback guard: {e}"))?;

    // (2) Load the MAIN-DB configured FNs ONCE.  The `ListenerCfg` contract is
    // that `fn` MUST exist in `fiscal_number_config`, validated at supervisor
    // startup — so a RestHttp listener for an FN absent from main is a BOOT
    // failure (the secure-DB D1 preflight alone is NOT sufficient: a stray
    // `payment_methods` slot for an unconfigured FN must not let a bogus
    // listener bind + accept POSTs).  Checked per-listener BEFORE D1.
    let configured_fns: std::collections::HashSet<String> =
        crate::db::repositories::fiscal_number_config::list_all(app.db())
            .await
            .map_err(|e| anyhow::anyhow!("RS-2 ingress: read fiscal_number_config: {e}"))?
            .into_iter()
            .map(|c| c.fiscal_number)
            .collect();

    // (2b) one ingress front per FN — PRE-PASS (fail fast before binding).
    // Two RestHttp listeners for the same fiscal_number (e.g. on `127.0.0.1`
    // and `::1`) would stand up two inline write-path entrypoints for one FN,
    // leaning entirely on the RS-3 single-writer lease (invariant #2).  Reject
    // at config instead.
    let mut seen_fns: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for l in listeners
        .iter()
        .filter(|l| l.kind == ListenerKind::RestHttp)
    {
        if !seen_fns.insert(l.fiscal_number.as_str()) {
            anyhow::bail!(
                "RS-2 ingress: duplicate RestHttp listener for fn {} — exactly one \
                 ingress front per fiscal_number is allowed (single-writer invariant)",
                l.fiscal_number
            );
        }
    }

    let mut bound = Vec::new();
    for l in listeners
        .iter()
        .filter(|l| l.kind == ListenerKind::RestHttp)
    {
        // (3) main-DB FN existence (documented startup contract).
        if !configured_fns.contains(&l.fiscal_number) {
            anyhow::bail!(
                "RS-2 ingress listener fn {} is not configured in fiscal_number_config \
                 (a RestHttp listener requires a known FN)",
                l.fiscal_number
            );
        }

        // (4) D1 frozen-slot preflight per RestHttp FN (secure DB).
        preflight_d1_slots(app.db_secure(), &l.fiscal_number)
            .await
            .map_err(|e| {
                anyhow::anyhow!("RS-2 ingress D1 preflight (fn {}): {e}", l.fiscal_number)
            })?;

        // (5) validate driver_id + bind (bind error = boot failure).
        let driver_id = DriverId::new(&l.driver_id).map_err(|e| {
            anyhow::anyhow!(
                "RS-2 ingress listener (fn {}) invalid driver_id: {e}",
                l.fiscal_number
            )
        })?;
        let ip = l
            .bind_ip()
            .map_err(|e| anyhow::anyhow!("RS-2 ingress bind: {e}"))?;
        // SO_REUSEADDR — the supervisor is fail-stop-and-restart by design, so a
        // restart that races an ingress connection in TIME_WAIT must still be
        // able to re-bind the loopback port instead of boot-looping on
        // EADDRINUSE.  (tokio's `TcpListener::bind` does not set it.)
        let addr = std::net::SocketAddr::new(ip, l.port);
        let socket = if ip.is_ipv4() {
            tokio::net::TcpSocket::new_v4()
        } else {
            tokio::net::TcpSocket::new_v6()
        }
        .map_err(|e| anyhow::anyhow!("RS-2 ingress socket {addr}: {e}"))?;
        socket
            .set_reuseaddr(true)
            .map_err(|e| anyhow::anyhow!("RS-2 ingress set_reuseaddr {addr}: {e}"))?;
        socket
            .bind(addr)
            .map_err(|e| anyhow::anyhow!("RS-2 ingress bind {addr}: {e}"))?;
        let listener = socket
            .listen(1024)
            .map_err(|e| anyhow::anyhow!("RS-2 ingress listen {addr}: {e}"))?;

        let state = IngressState {
            main_pool: app.db().clone(),
            secure_pool: app.db_secure().clone(),
            listener_fn: l.fiscal_number.clone(),
            driver_id,
            protocol: Protocol::Rest,
            write_path: Arc::clone(write_path),
        };
        // Name keys the supervised-task / audit entity_id — include the bind IP
        // so the same fn+port on `127.0.0.1` vs `::1` cannot collide.
        let name = format!("ingress:rest:{}:{}:{}", l.fiscal_number, ip, l.port);
        tracing::info!(
            target: "prro::runtime::supervisor",
            fiscal_number = %l.fiscal_number,
            bind_ip = %ip,
            port = l.port,
            "RS-2 ingress listener bound"
        );
        bound.push((listener, state, name));
    }
    Ok(bound)
}

/// The terminal-completion contract for a supervised task — how to read a
/// task that has stopped running.
///
/// The live discriminator is whether the shutdown watch was already flipped
/// when the task completed (see [`classify_completion`]); the tag is carried
/// for audit context and so a future task class with different semantics
/// (e.g. a one-shot that is SUPPOSED to finish early) slots in without
/// re-touching this RS-1 F1 seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedTerminal {
    /// drain / probe tick loops: run until the shutdown watch flips.  ANY
    /// completion BEFORE the flip (`Ok`, `Err`, or panic) is an invariant-bug
    /// loop-death.
    RunsUntilShutdown,
    /// axum ingress servers (RS-2 piece-5b-ii): `with_graceful_shutdown`
    /// returns `Ok(())` NORMALLY once the watch flips.  `Ok`-AFTER-flip is the
    /// normal graceful exit; `Ok`-BEFORE-flip (e.g. a swallowed bind/serve
    /// error) and any `Err`/panic are loop-death.
    GracefulOkAfterShutdown,
}

/// A supervised background task: a spawned handle + the name used in
/// logs/audits + its terminal-completion contract.
///
/// The handle yields `anyhow::Result<()>` (NOT bare `()`) so the F1 seam can
/// distinguish a graceful `Ok(())` from a real task `Err` — e.g. an axum
/// `serve().await` failure (RS-2 piece-5b-ii) — and audit the actual cause
/// instead of a generic "returned unexpectedly".  drain/probe loops return
/// `Ok(())` on watch-exit.
///
/// Fields are private — build via [`runs_until_shutdown`](Self::runs_until_shutdown)
/// or [`graceful`](Self::graceful) so a future precondition (e.g. name
/// validation for the audit entity_id) can be enforced in one place without a
/// breaking API change.
pub struct SupervisedTask {
    name: String,
    handle: JoinHandle<anyhow::Result<()>>,
    expected: ExpectedTerminal,
}

impl SupervisedTask {
    /// A [`RunsUntilShutdown`](ExpectedTerminal::RunsUntilShutdown) task — the
    /// drain / probe loops (any completion before the shutdown flip is a
    /// loop-death).
    pub fn runs_until_shutdown(
        name: impl Into<String>,
        handle: JoinHandle<anyhow::Result<()>>,
    ) -> Self {
        Self {
            name: name.into(),
            handle,
            expected: ExpectedTerminal::RunsUntilShutdown,
        }
    }

    /// A [`GracefulOkAfterShutdown`](ExpectedTerminal::GracefulOkAfterShutdown)
    /// task — an axum ingress server (RS-2 piece-5b-ii) whose
    /// `with_graceful_shutdown` returns `Ok(())` once the watch flips.  An
    /// `Ok`-after-flip is the normal exit; `Ok`-before-flip / `Err` / panic
    /// are loop-death.  The handle must yield the server result
    /// (`axum::serve(...).await.map_err(Into::into)`), NOT swallow it, so a
    /// `serve()` failure is captured in the F1 audit.
    pub fn graceful(name: impl Into<String>, handle: JoinHandle<anyhow::Result<()>>) -> Self {
        Self {
            name: name.into(),
            handle,
            expected: ExpectedTerminal::GracefulOkAfterShutdown,
        }
    }
}

/// What a supervised task's run produced once joined — lets the F1 seam tell a
/// graceful `Ok(())` from a real task `Err` (e.g. an axum `serve()` failure)
/// from a panic, so a pre-flip `Err` is audited WITH its cause rather than a
/// generic "returned unexpectedly".
enum TaskOutcome {
    /// `Ok(())` — graceful exit (axum) / watch-exit (drain/probe).
    ReturnedOk,
    /// `Err(_)` — e.g. an axum `serve()` error; carries the cause.
    ReturnedErr(anyhow::Error),
    /// Panicked (or cancelled — we never abort, so in practice a panic).
    Panicked(tokio::task::JoinError),
}

impl TaskOutcome {
    fn from_join(res: Result<anyhow::Result<()>, tokio::task::JoinError>) -> Self {
        match res {
            Ok(Ok(())) => TaskOutcome::ReturnedOk,
            Ok(Err(e)) => TaskOutcome::ReturnedErr(e),
            Err(je) => TaskOutcome::Panicked(je),
        }
    }

    /// `true` ONLY for a genuine panic (`JoinError::is_panic`) — the audit
    /// payload's `panicked` field.
    fn panicked(&self) -> bool {
        matches!(self, TaskOutcome::Panicked(je) if je.is_panic())
    }

    /// Audit / log detail.  For `ReturnedErr` this is the task's own error
    /// (the load-bearing improvement: a swallowed axum serve error is now
    /// surfaced).
    fn detail(&self) -> String {
        match self {
            TaskOutcome::ReturnedOk => "task returned Ok before the shutdown flip".to_string(),
            TaskOutcome::ReturnedErr(e) => format!("task returned Err: {e:#}"),
            TaskOutcome::Panicked(je) => format!("{je}"),
        }
    }
}

/// How a completed supervised task is treated.
enum Disposition {
    /// Expected wind-down — a panic is logged but does NOT trigger a restart.
    Normal,
    /// Invariant-bug loop death — CRITICAL audit + fail the supervisor for an
    /// orchestrator restart.
    LoopDeath,
}

/// Classify a task that stopped running, given its policy + whether the
/// shutdown watch had already been flipped at completion time.
///
/// The live axis is `watch_flipped`:
///   - BEFORE the flip nothing should stop — every policy ⇒ [`LoopDeath`]
///     (drain/probe early exit; axum `Ok`-before-flip / `Err` / panic);
///   - AFTER the flip every task is winding down ⇒ [`Normal`] (a panic is
///     logged by the caller, never a NEW restart — operator: post-shutdown
///     join results don't change F1 restart semantics).
///
/// `_expected` is part of the signature for audit context + future task
/// classes; today the two policies coincide on the `watch_flipped` axis, so it
/// is intentionally unused here.  When RS-2 piece-5b-ii adds a policy that
/// genuinely diverges, branch on it HERE (and add a variant-distinguishing
/// test).
///
/// [`LoopDeath`]: Disposition::LoopDeath
/// [`Normal`]: Disposition::Normal
fn classify_completion(_expected: ExpectedTerminal, watch_flipped: bool) -> Disposition {
    if watch_flipped {
        Disposition::Normal
    } else {
        Disposition::LoopDeath
    }
}

/// Log an abnormal task exit WITHOUT triggering a restart — used for
/// completions AFTER the watch flip (normal wind-down).  A graceful `Ok`
/// exit is silent; a panic or a real `Err` (e.g. an axum serve error during
/// graceful shutdown) is logged with its cause.
fn log_join_failure(name: &str, outcome: &TaskOutcome) {
    match outcome {
        TaskOutcome::ReturnedOk => {}
        TaskOutcome::ReturnedErr(e) => tracing::error!(
            target: "prro::runtime::supervisor",
            loop_name = name,
            error = %e,
            "supervised task returned Err during wind-down"
        ),
        TaskOutcome::Panicked(je) => tracing::error!(
            target: "prro::runtime::supervisor",
            loop_name = name,
            error = %je,
            "supervised task panicked during wind-down"
        ),
    }
}

/// The supervisor's wait + teardown lifecycle (F1), generalized to a NAMED
/// SET of supervised tasks (RS-2 piece-5b-i — drain/probe today, + per-listener
/// axum servers in 5b-ii).  Preserves the RS-1 F1 invariants:
///   - **biased** shutdown priority: the external `shutdown` is polled FIRST,
///     so a normal shutdown is never misread as a task death (and a task dying
///     at the SAME poll as shutdown is benignly suppressed — the process is
///     already going down, crash-equivalent / re-drained next boot);
///   - **one shared grace** bounds the join of ALL remaining tasks (never a
///     per-task sequential grace), matching the runbook's `TimeoutStopSec >
///     grace` (1×) contract;
///   - a task completing BEFORE the flip → CRITICAL `SUPERVISOR_LOOP_DIED`
///     audit + `Err` for an orchestrator restart; AFTER the flip → normal
///     wind-down (panics logged, no new restart).
///
/// Only the FIRST pre-flip loop-death is the audited restart cause; flipping
/// the watch then winds the siblings down under the shared grace.  Integration
/// tests inject handles here to drive each path without a real tick panic.
pub async fn supervise_task_set<F>(
    app: App,
    shutdown: F,
    shutdown_tx: watch::Sender<bool>,
    tasks: Vec<SupervisedTask>,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send,
{
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

    tokio::pin!(shutdown);

    // Defensive: an empty set has nothing to supervise — just await shutdown.
    if tasks.is_empty() {
        shutdown.await;
        let _ = shutdown_tx.send(true);
        drop(app);
        return Ok(());
    }

    // Move each handle into a tagged future so a completion tells us WHICH
    // task ended, under which policy, and HOW (Ok / Err / panic).
    let mut pending: FuturesUnordered<_> = tasks
        .into_iter()
        .map(|t| async move {
            let outcome = TaskOutcome::from_join(t.handle.await);
            (t.name, t.expected, outcome)
        })
        .collect();

    // ── Phase 1: WAIT — biased shutdown priority (F1 invariant). ──
    let first = tokio::select! {
        biased;
        () = &mut shutdown => None,
        completed = pending.next() => completed,
    };

    // Flip the watch → the still-running loops exit at their next biased select
    // poll (the in-flight tick bails between FNs, F2); axum servers begin
    // graceful shutdown.
    let _ = shutdown_tx.send(true);

    // A pre-flip completion (Phase 1 won over shutdown) is a loop-death.
    let mut loop_death: Option<String> = None;
    if let Some((name, expected, outcome)) = first {
        match classify_completion(expected, /* watch_flipped = */ false) {
            Disposition::LoopDeath => {
                audit_loop_died(&app, &name, expected, outcome).await;
                loop_death = Some(name);
            }
            // Unreachable today (pre-flip ⇒ LoopDeath); defensive for a future
            // "completes-early-Ok" task class.
            Disposition::Normal => log_join_failure(&name, &outcome),
        }
    } else {
        tracing::info!(
            target: "prro::runtime::supervisor",
            "supervisor: shutdown signal received; stopping tasks"
        );
    }

    // ── Phase 2: JOIN the rest under ONE shared grace (F1 invariant). ──
    // The watch is now flipped, so every remaining completion is expected
    // wind-down — `classify_completion(_, watch_flipped = true)` is `Normal`
    // for every policy by construction, so we log a panic but never trigger a
    // NEW restart (operator: post-shutdown join results don't change F1
    // restart semantics).
    let join_rest = async {
        while let Some((name, _expected, outcome)) = pending.next().await {
            log_join_failure(&name, &outcome);
        }
    };
    if tokio::time::timeout(grace, join_rest).await.is_err() {
        tracing::warn!(
            target: "prro::runtime::supervisor",
            grace_secs = grace.as_secs(),
            "supervised tasks did not finish within the shutdown grace; proceeding (per-doc drain is crash-safe)"
        );
    }

    // On grace-elapse the still-running tasks are DETACHED (not aborted —
    // crash-safe per the per-doc drain).  A detached task that captured an
    // `app.clone()` keeps the `Arc<App::Inner>` (PidLock + DB pools) alive
    // until IT ends; under the current single-supervisor-then-exit topology
    // that is bounded by process exit, so this `drop(app)` only releases the
    // supervisor's own clone.  (If a future caller runs `supervise_task_set`
    // and keeps the process alive afterwards, abort the survivors here first.)
    drop(app);
    match loop_death {
        Some(which) => Err(anyhow::anyhow!(
            "supervisor: {which} task exited before shutdown — failing for orchestrator restart (see SUPERVISOR_LOOP_DIED audit)"
        )),
        None => {
            tracing::info!(target: "prro::runtime::supervisor", "supervisor: shut down");
            Ok(())
        }
    }
}

/// RS-1 F1 compatibility wrapper — the drain + probe pair as a two-task
/// supervised set.  Preserved so existing call sites/tests stay byte-stable;
/// new call sites (RS-2 piece-5b-ii) build the [`SupervisedTask`] set directly.
pub async fn supervise_until_shutdown<F>(
    app: App,
    shutdown: F,
    shutdown_tx: watch::Sender<bool>,
    drain_handle: JoinHandle<anyhow::Result<()>>,
    probe_handle: JoinHandle<anyhow::Result<()>>,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send,
{
    supervise_task_set(
        app,
        shutdown,
        shutdown_tx,
        vec![
            SupervisedTask::runs_until_shutdown("drain", drain_handle),
            SupervisedTask::runs_until_shutdown("probe", probe_handle),
        ],
    )
    .await
}

/// Emit the durable CRITICAL `SUPERVISOR_LOOP_DIED` audit when a supervised
/// task exits before shutdown.  Panic-guarded (best-effort) like the probe-tick
/// audit contract — if the audit insert itself fails we fall back to tracing.
async fn audit_loop_died(app: &App, which: &str, expected: ExpectedTerminal, outcome: TaskOutcome) {
    let panicked = outcome.panicked();
    let detail = outcome.detail();
    tracing::error!(
        target: "prro::runtime::supervisor",
        loop_name = which,
        expected = ?expected,
        panicked,
        detail = %detail,
        "supervisor: task died before shutdown — failing the supervisor for orchestrator restart"
    );
    let payload = serde_json::json!({
        "loop": which,
        "expected": format!("{expected:?}"),
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
) -> JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        // A second receiver for the between-FN check inside drain_tick — the
        // select! below holds `&mut shutdown_rx` for its `changed()` arm, so
        // the tick body cannot borrow the same receiver.
        let tick_shutdown = shutdown_rx.clone();
        let mut iv = tokio::time::interval(interval);
        iv.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // F1 INVARIANT: this loop must return `Ok(())` ONLY (on watch-exit).
        // A `?`/`return Err(_)` added here would surface as a pre-flip
        // loop-death → CRITICAL audit + process restart.  Per-FN errors are
        // logged-and-skipped INSIDE `drain_tick`, never propagated.
        loop {
            tokio::select! {
                biased;
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!(target: "prro::runtime::supervisor", "drain loop: shutdown; exiting");
                        return Ok(());
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

/// Spawn the B1 online-convergence ticker — one tokio task iterating every
/// registered FN each `interval`, driving resting online `SENT`/`KVT1` docs to
/// `ACK` via the reused recovery arms (OCF-5 runtime owner).  Exact structural
/// copy of [`spawn_drain_loop`]: `MissedTickBehavior::Skip` + `biased` select
/// makes shutdown win, and the loop returns `Ok(())` only (F1).
fn spawn_convergence_loop(
    app: App,
    registry: Arc<BindingsRegistry>,
    fn_ids: Vec<String>,
    interval: Duration,
    mut shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        // Second receiver for the between-FN check inside convergence_tick (see
        // spawn_drain_loop).
        let tick_shutdown = shutdown_rx.clone();
        let mut iv = tokio::time::interval(interval);
        iv.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // F1 INVARIANT: this loop must return `Ok(())` ONLY (on watch-exit).
        // A `?`/`return Err(_)` here would surface as a pre-flip loop-death →
        // CRITICAL audit + process restart.  Per-FN errors are
        // logged-and-skipped INSIDE `convergence_tick`, never propagated.
        loop {
            tokio::select! {
                biased;
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!(target: "prro::runtime::supervisor", "online-convergence loop: shutdown; exiting");
                        return Ok(());
                    }
                }
                _ = iv.tick() => {
                    convergence_tick(&app, &registry, &fn_ids, &tick_shutdown).await;
                }
            }
        }
    })
}

/// One online-convergence pass over all FNs.  Rebuilds `fn_sign` FRESH per FN
/// (signingTime must be current at the wire call — NEVER cached) and routes
/// through the A4-gated [`App::converge_online_for_fn`].  A per-FN `fn_sign`
/// build failure or tick error is logged and skipped — one bad FN never stops
/// the others or kills the loop.  Mirror of [`drain_tick`].
async fn convergence_tick(
    app: &App,
    registry: &BindingsRegistry,
    fn_ids: &[String],
    shutdown: &watch::Receiver<bool>,
) {
    for fn_id in fn_ids {
        // F2: bail BETWEEN FNs so a shutdown during a long multi-FN pass is
        // honored promptly instead of only after the whole pass returns.
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
                    "online-convergence tick: fn_sign build failed; FN skipped this tick"
                );
                continue;
            }
        };
        let view = RuntimeView {
            dps: b.dps.as_ref(),
            signing_ctx: &b.sign_ctx,
            fn_sign: &fn_sign,
        };
        match app.converge_online_for_fn(fn_id, &view).await {
            Ok(summary) => {
                tracing::debug!(
                    target: "prro::runtime::supervisor",
                    fiscal_number = fn_id,
                    ?summary,
                    "online-convergence tick complete"
                );
            }
            Err(e) => {
                tracing::error!(
                    target: "prro::runtime::supervisor",
                    fiscal_number = fn_id,
                    error = ?e,
                    "online-convergence tick failed"
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
) -> JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        // Second receiver for the between-FN check inside probe_tick (see
        // spawn_drain_loop).
        let tick_shutdown = shutdown_rx.clone();
        let mut iv = tokio::time::interval(interval);
        iv.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // F1 INVARIANT: this loop must return `Ok(())` ONLY (on watch-exit).
        // A `?`/`return Err(_)` added here would surface as a pre-flip
        // loop-death → CRITICAL audit + process restart.  Per-FN errors are
        // logged-and-skipped INSIDE `probe_tick`, never propagated.
        loop {
            tokio::select! {
                biased;
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!(target: "prro::runtime::supervisor", "probe loop: shutdown; exiting");
                        return Ok(());
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
