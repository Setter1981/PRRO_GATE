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

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use crate::app::App;
use crate::runtime::bindings::BindingsRegistry;
use crate::runtime::key_loader::JksOperatorKeyLoader;
use crate::transports::dps::channel::DpsChannel;
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
/// **Piece 5a: skeleton.**  Logs the per-FN deps it would drive and awaits
/// the shutdown signal.  Reconcile-once (5c) + the drain/probe tick loops
/// with per-tick `fn_sign` rebuild (5d) + the watch-flip-and-join graceful
/// shutdown (5e) land on top of this seam.
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
        "supervisor: running (M3 — recovery + loops land in 5c/5d)"
    );

    // 5e placeholder until the loops land: just await the shutdown signal.
    shutdown.await;

    tracing::info!(target: "prro::runtime::supervisor", "supervisor: shutting down");
    drop(app);
    Ok(())
}
