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

use crate::app::App;
use crate::runtime::bindings::BindingsRegistry;
use crate::runtime::key_loader::{build_fn_sign, JksOperatorKeyLoader};
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
        "supervisor: running (M3)"
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

    // 5e placeholder until the tick loops land (5d): await the shutdown signal.
    shutdown.await;

    tracing::info!(target: "prro::runtime::supervisor", "supervisor: shutting down");
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
