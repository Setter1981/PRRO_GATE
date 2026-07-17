//! M3b W8 — return-online detection probe.
//!
//! Periodic background task that, when node mode is `Offline`,
//! calls a lightweight read-only DPS probe (`status_rro`) to
//! detect that DPS is reachable AND reports the FN as online.
//! On success: flips `node_state.mode` from `Offline` to
//! `GoingOnline`.  W9 backlog drain owns the subsequent
//! `GoingOnline → Online` transition.
//!
//! ## Design freeze
//!
//! See `docs/superpowers/specs/2026-05-16-m3b-w8-return-online-probe.md`
//! for the full operator-pinned design.  Six hard lines this
//! module enforces:
//!
//!   1. `StatusSnapshot::online == true` is the SINGLE success
//!      predicate.  No multi-field composition.
//!   2. `open_shift` / `last_signer` are AUDIT FIELDS ONLY.
//!   3. `Offline → GoingOnline` only inside `with_immediate` with
//!      explicit CAS `WHERE mode = 'OFFLINE'`.
//!   4. `GoingOnline + success → no DB write, no audit row`.
//!   5. `Online → skip BEFORE the DPS wire call`.
//!   6. Single tokio task for all FNs, `MissedTickBehavior::Skip`,
//!      `select!` over `interval.tick()` + `shutdown.recv()` with
//!      shutdown branch ALWAYS winning (no panic, no dangling tx).
//!
//! ## DPS surface choice: `statusRro` (not `ping`)
//!
//! The probe calls `DpsChannel::status_rro`, NOT
//! `DpsChannel::ping`.  Rationale (per design freeze §10 +
//! memory `m3b-w8-review-criteria` axis 6): `statusRro` is a
//! read-only surface that returns the full [`StatusSnapshot`]
//! shape — `online`, `open_shift`, `last_signer`.  The single
//! success predicate is `StatusSnapshot::online == true`
//! (operator hard line 1); `open_shift` and `last_signer` are
//! AUDIT FIELDS ONLY (operator hard line 2) and are recorded
//! verbatim in `RETURN_ONLINE_PROBE_SUCCESS` /
//! `RETURN_ONLINE_PROBE_FAILED` payloads for forensics.  `ping`
//! was the lighter alternative but does not carry the snapshot
//! fields, so it cannot satisfy hard line 2.  Future
//! maintainers tempted to switch to the cheaper call must
//! preserve the snapshot fields in the audit (or re-pin hard
//! line 2 with the operator).
//!
//! ## Invariant preservation
//!
//! - **I1** (no foreign IO inside SQLite write tx): probe's
//!   `status_rro` DPS call happens OUTSIDE any `with_immediate`
//!   envelope.  The success-path mode flip is one short envelope;
//!   the wire call is in its own pool-bound phase.
//! - **I8** (state-machine correctness): only the
//!   `Offline → GoingOnline` edge is written; `GoingOnline →
//!   Online` is W9's responsibility.  CAS WHERE clause prevents
//!   spurious writes on concurrent mode changes.
//! - **I9** (graceful shutdown): `tokio::select!` with `biased`
//!   priority on the shutdown branch ensures shutdown wins cleanly
//!   even if `interval.tick()` is ready at the same poll.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::db::models::enums::{NodeMode, Severity};
use crate::db::repositories::{audit_log, node_state};
use crate::db::tx::with_immediate;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::CheckSignBlob;
use crate::transports::dps::error::{AuthorizationKind, DpsError};

/// Stable string taxonomy for [`DpsError`] variants, embedded in
/// `RETURN_ONLINE_PROBE_FAILED` audit payloads under the
/// `dps_error_class` key.  Stable means: contract-grade — callers
/// (audit log consumers, dashboards, alert rules) can match exact
/// strings without parsing Debug repr.  Any future variant MUST
/// extend this match arm.
fn dps_error_class(err: &DpsError) -> &'static str {
    match err {
        DpsError::Transport(_) => "Transport",
        // CS-3 Slice A′: distinct per-variant audit class (this fn is a forensic string
        // key, not routing/CHECK-constrained — the doc invites extension). Behaviour is
        // unaffected; probe-failure audit payloads gain an accurate "RemoteStatus" label
        // distinguishing gRPC auth-rejection from genuine transport absence.
        DpsError::RemoteStatus { .. } => "RemoteStatus",
        // CS-3 Slice A: distinct per-variant audit class (this fn is a forensic string
        // key, not routing/CHECK-constrained — the doc invites extension). Behaviour is
        // unaffected; only the probe-failure audit payload gains an accurate `-4` label.
        DpsError::Indeterminate { .. } => "Indeterminate",
        DpsError::Authorization { .. } => "Authorization",
        DpsError::Decode(_) => "Decode",
        DpsError::Server { .. } => "Server",
        DpsError::NotFound => "NotFound",
        DpsError::ServerFiscalIdMismatch { .. } => "ServerFiscalIdMismatch",
        DpsError::QueryNotSupported(_) => "QueryNotSupported",
        DpsError::Internal(_) => "Internal",
    }
}

/// Sub-classification key for [`DpsError::Authorization`], emitted
/// alongside `dps_error_class = "Authorization"` so consumers can
/// route `DocumentReject` vs `FiscalNumberNotRegistered` without
/// re-parsing.  `None` for all other variants.
fn authorization_kind_class(err: &DpsError) -> Option<&'static str> {
    match err {
        DpsError::Authorization { kind, .. } => Some(match kind {
            AuthorizationKind::DocumentReject => "DocumentReject",
            AuthorizationKind::FiscalNumberNotRegistered => "FiscalNumberNotRegistered",
        }),
        _ => None,
    }
}

/// One FN entry in the probe's iteration list.  Frozen at spawn —
/// the probe task iterates the same `Vec<ProbeSpec>` each tick.
/// FN add/remove dynamics require respawn (rare runtime event;
/// out of W8 scope).
#[derive(Debug, Clone)]
pub struct ProbeSpec {
    pub fiscal_number: String,
    pub fn_sign: CheckSignBlob,
}

/// W8b — runtime dependencies supplied by the composition root to
/// [`crate::App::spawn_return_online_probe`].  Carries only what the
/// App cannot derive from its own state:
///
///   - `dps` — the DPS wire channel, owned (`Arc<dyn DpsChannel>`)
///     so the spawned probe task can hold it `'static`.  Future
///     two-channel deployments will substitute an `Arc<dyn
///     DpsChannelRouter>` here without touching App boot semantics.
///   - `fn_signs` — per-FN signer blob, owned by value.  The App
///     method enumerates ALL configured FNs via
///     `fiscal_number_config::list_all` and joins each FN with this
///     map.  FNs present in `fiscal_number_config` but absent from
///     this map are NOT added to the probe loop's `Vec<ProbeSpec>`;
///     a `RETURN_ONLINE_PROBE_FN_SKIPPED_NO_SIGNER` WARN audit is
///     emitted instead, and other FNs continue.  Graceful skip
///     keeps the probe loop multi-FN-resilient against config drift
///     on a single FN.
///
/// Tick interval is NOT in this struct — the App method reads it
/// from `config.offline.clamped_probe_interval_seconds()` and emits
/// the WARN clamp-audit itself.  Production callers cannot pre-clamp
/// or override; the clamp + audit live under one App-owned seam.
///
/// `Vec<ProbeSpec>` is NOT in this struct — the App method enumerates
/// FNs and builds the spec list internally; callers cannot
/// pre-build it or skip enumeration.  This is what makes "enumerate
/// ALL configured FNs" a verifiable App-method property.
pub struct ReturnOnlineProbeDeps {
    pub dps: Arc<dyn DpsChannel>,
    pub fn_signs: std::collections::HashMap<String, CheckSignBlob>,
}

/// Outcome of one probe tick for one FN.  Used by the tick-level
/// API (`run_tick_for_fn`) — the spawn-loop wrapper discards
/// outcomes (audits are the durable artefact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickOutcome {
    /// Probe ran AND flipped mode `Offline → GoingOnline`.
    Success { fiscal_number: String },
    /// Probe ran but skipped — mode is not `Offline` so no work
    /// to do.  `reason` is typed for forensics.
    Skipped {
        fiscal_number: String,
        reason: SkipReason,
    },
    /// Probe ran AND failed — DPS error OR DPS reported
    /// `online == false` OR CAS missed.  Mode unchanged.
    Failed {
        fiscal_number: String,
        reason: FailureReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Mode is `Online` — probe skipped BEFORE wire call (hard line 5).
    NodeOnline,
    /// Mode is `GoingOnline` already — idempotent no-op (hard line 4).
    /// No DB write, no audit row.
    NodeAlreadyGoingOnline,
    /// Mode is `Blocked` / `StopMode` / `CryptoDegraded` /
    /// `GoingOffline` — out of W8 scope (W7b / W9 territory).
    NodeNotOfflineOrTransition,
    /// `node_state` row missing — probe has no FN to act on.
    /// Bootstrap-time race; next tick will re-evaluate.
    NodeStateMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReason {
    /// DPS wire call returned `Err(DpsError)`.  Detailed class in
    /// audit payload `dps_error_class`.
    DpsError,
    /// DPS call succeeded but returned `StatusSnapshot { online:
    /// false, ... }`.  Audit payload carries the full snapshot.
    DpsReportsOffline,
    /// Mode flip CAS missed — concurrent mode change after the
    /// step-1 read.  No-op + audit; next tick re-evaluates.
    CasMiss,
}

/// Single-tick probe for one FN.
///
/// Pool-bound.  Step sequence (operator hard lines pinned in
/// design freeze):
///
///   1. Read `node_state.mode` (pool-bound).  Skip if mode is not
///      `Offline` — `Online` / `GoingOnline` / etc each map to a
///      `SkipReason` typed variant.  This implements **hard line
///      5** (Online skip BEFORE wire call) + **hard line 4**
///      (GoingOnline idempotent no-op without write or audit).
///   2. Emit `RETURN_ONLINE_PROBE_ATTEMPT` audit (Info).
///   3. Call `dps.status_rro(fn_sign)` — OUTSIDE any
///      `with_immediate` envelope (I1).
///   4. Branch on result.  **Hard line 1**: only
///      `Ok(StatusSnapshot { online: true, .. })` is success;
///      `online == false` and DPS errors both → failure path.
///      **Hard line 2**: `open_shift` and `last_signer` are
///      recorded in audit payload (success AND failure) but do
///      NOT participate in branching.
///   5. Success path: flip `Offline → GoingOnline` via
///      `with_immediate` with explicit `WHERE mode = 'OFFLINE'`
///      CAS (**hard line 3**).  CAS miss → typed failure
///      (concurrent mode change).
///   6. Emit `RETURN_ONLINE_PROBE_SUCCESS` audit.
pub async fn run_tick_for_fn(
    pool: &SqlitePool,
    dps: &dyn DpsChannel,
    fiscal_number: &str,
    fn_sign: &CheckSignBlob,
) -> anyhow::Result<TickOutcome> {
    // ─── Step 1: read mode (pool-bound, BEFORE wire call) ───────
    let ns_pre = match node_state::get(pool, fiscal_number).await? {
        Some(ns) => ns,
        None => {
            return Ok(TickOutcome::Skipped {
                fiscal_number: fiscal_number.to_string(),
                reason: SkipReason::NodeStateMissing,
            });
        }
    };

    match ns_pre.mode {
        NodeMode::Online => {
            // Hard line 5: skip BEFORE wire call.  No audit
            // (would spam every tick).
            return Ok(TickOutcome::Skipped {
                fiscal_number: fiscal_number.to_string(),
                reason: SkipReason::NodeOnline,
            });
        }
        NodeMode::GoingOnline => {
            // Hard line 4: GoingOnline + success → no write, no
            // audit row.  We skip the wire call too — re-probing
            // doesn't add value while W9 drain is in progress.
            return Ok(TickOutcome::Skipped {
                fiscal_number: fiscal_number.to_string(),
                reason: SkipReason::NodeAlreadyGoingOnline,
            });
        }
        NodeMode::Offline => {
            // Proceed.
        }
        NodeMode::Blocked
        | NodeMode::StopMode
        | NodeMode::CryptoDegraded
        | NodeMode::GoingOffline => {
            return Ok(TickOutcome::Skipped {
                fiscal_number: fiscal_number.to_string(),
                reason: SkipReason::NodeNotOfflineOrTransition,
            });
        }
    }

    // ─── Step 2: ATTEMPT audit ──────────────────────────────────
    let payload_attempt = serde_json::json!({
        "fiscal_number": fiscal_number,
        "observed_mode_pre": ns_pre.mode.as_str(),
        "tick_at": chrono::Utc::now().to_rfc3339(),
    });
    audit_log::append(
        pool,
        "node_state",
        fiscal_number,
        "RETURN_ONLINE_PROBE_ATTEMPT",
        Severity::Info,
        None,
        Some(&payload_attempt.to_string()),
    )
    .await?;

    // ─── Step 3: DPS wire call (OUTSIDE with_immediate; I1) ─────
    let probe_result: Result<crate::transports::dps::dto::StatusSnapshot, DpsError> =
        dps.status_rro(fn_sign).await;

    let snapshot = match probe_result {
        Ok(s) => s,
        Err(err) => {
            // Failure: emit audit + return.  Mode unchanged.
            // `dps_error_class` is a stable string taxonomy (NOT
            // Debug repr) so audit consumers can match exact
            // variants without parsing.  See `dps_error_class()`
            // helper at module top.
            let mut payload_failed = serde_json::json!({
                "fiscal_number": fiscal_number,
                "observed_mode_pre": ns_pre.mode.as_str(),
                "dps_error_class": dps_error_class(&err),
                "dps_error_detail": format!("{err}"),
                "reason": "dps_error",
            });
            if let Some(kind) = authorization_kind_class(&err) {
                payload_failed["authorization_kind"] = serde_json::json!(kind);
            }
            audit_log::append(
                pool,
                "node_state",
                fiscal_number,
                "RETURN_ONLINE_PROBE_FAILED",
                Severity::Warning,
                None,
                Some(&payload_failed.to_string()),
            )
            .await?;
            return Ok(TickOutcome::Failed {
                fiscal_number: fiscal_number.to_string(),
                reason: FailureReason::DpsError,
            });
        }
    };

    // ─── Step 4: success predicate (hard line 1: online == true only) ─
    if !snapshot.online {
        // open_shift / last_signer recorded for forensics (hard
        // line 2 — audit only, not branch condition).
        let payload_failed = serde_json::json!({
            "fiscal_number": fiscal_number,
            "observed_mode_pre": ns_pre.mode.as_str(),
            "dps_snapshot": {
                "open_shift": snapshot.open_shift,
                "online": snapshot.online,
                "last_signer": snapshot.last_signer,
            },
            "reason": "dps_reports_offline",
        });
        audit_log::append(
            pool,
            "node_state",
            fiscal_number,
            "RETURN_ONLINE_PROBE_FAILED",
            Severity::Warning,
            None,
            Some(&payload_failed.to_string()),
        )
        .await?;
        return Ok(TickOutcome::Failed {
            fiscal_number: fiscal_number.to_string(),
            reason: FailureReason::DpsReportsOffline,
        });
    }

    // ─── Step 5: mode flip via with_immediate CAS (hard line 3) ─
    let fn_owned = fiscal_number.to_string();
    let rows_affected: u64 = with_immediate(pool, move |tx| {
        Box::pin(async move {
            let res = sqlx::query(
                "UPDATE node_state SET mode = 'GOING_ONLINE' \
                 WHERE fiscal_number = ? AND mode = 'OFFLINE'",
            )
            .bind(&fn_owned)
            .execute(&mut **tx)
            .await?;
            Ok::<u64, anyhow::Error>(res.rows_affected())
        })
    })
    .await?;

    if rows_affected == 0 {
        // CAS miss — mode flipped under us between step-1 read
        // and step-5 write.  Concurrent W4-W6 offline session
        // machinery or W9 drain may have moved the node mode.
        // Emit a FAILED audit with reason `cas_miss` so operator
        // can correlate; mode is whatever the concurrent writer
        // left it.
        let payload_cas_miss = serde_json::json!({
            "fiscal_number": fiscal_number,
            "observed_mode_pre": ns_pre.mode.as_str(),
            "dps_snapshot": {
                "open_shift": snapshot.open_shift,
                "online": snapshot.online,
                "last_signer": snapshot.last_signer,
            },
            "reason": "cas_miss_concurrent_mode_change",
        });
        audit_log::append(
            pool,
            "node_state",
            fiscal_number,
            "RETURN_ONLINE_PROBE_FAILED",
            Severity::Warning,
            None,
            Some(&payload_cas_miss.to_string()),
        )
        .await?;
        return Ok(TickOutcome::Failed {
            fiscal_number: fiscal_number.to_string(),
            reason: FailureReason::CasMiss,
        });
    }

    // ─── Step 6: SUCCESS audit ──────────────────────────────────
    let payload_success = serde_json::json!({
        "fiscal_number": fiscal_number,
        "observed_mode_pre": ns_pre.mode.as_str(),
        "observed_mode_post": "GOING_ONLINE",
        "dps_open_shift": snapshot.open_shift,
        "dps_online": snapshot.online,
        "dps_last_signer": snapshot.last_signer,
    });
    audit_log::append(
        pool,
        "node_state",
        fiscal_number,
        "RETURN_ONLINE_PROBE_SUCCESS",
        Severity::Info,
        None,
        Some(&payload_success.to_string()),
    )
    .await?;
    Ok(TickOutcome::Success {
        fiscal_number: fiscal_number.to_string(),
    })
}

/// Spawn the single-task probe loop for a fixed set of FNs.
///
/// Operator hard line 6 implementation: ONE tokio task iterates
/// `fns` each tick.  `tokio::time::interval` with
/// `MissedTickBehavior::Skip` — long DPS calls do NOT queue up
/// missed ticks.  `tokio::select!` with `biased` priority on the
/// shutdown branch — shutdown ALWAYS wins cleanly even if a tick
/// is ready at the same poll moment.
///
/// Per-FN tick failures are non-fatal — `run_tick_for_fn` returns
/// `Ok(TickOutcome::Failed)` for operational failures and `Err(_)`
/// only for true infrastructure errors (DB unreachable etc.).  The
/// outer loop ignores per-FN errors after logging — operator
/// reviews via audit_log.
pub fn spawn_probe_loop(
    pool: Arc<SqlitePool>,
    dps: Arc<dyn DpsChannel>,
    fns: Vec<ProbeSpec>,
    tick_interval: Duration,
    mut shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick_iv = tokio::time::interval(tick_interval);
        tick_iv.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // The first `tick().await` returns immediately — this is
        // tokio's well-known interval behaviour.  Caller treats
        // the first tick as a "boot probe"; if the operator wants
        // to defer the first probe by `tick_interval`, they can
        // sleep before calling spawn.
        loop {
            tokio::select! {
                biased;
                // Shutdown branch wins (operator hard line 6).
                changed = shutdown_rx.changed() => {
                    // `Err(_)` means the shutdown channel was
                    // dropped — treat as shutdown too.  `Ok(())`
                    // with value=true is the explicit shutdown
                    // signal.
                    if changed.is_err() || *shutdown_rx.borrow() {
                        tracing::info!("return_online_probe: shutdown received; task exiting");
                        return;
                    }
                }
                _ = tick_iv.tick() => {
                    for spec in &fns {
                        match run_tick_for_fn(
                            &pool,
                            dps.as_ref(),
                            &spec.fiscal_number,
                            &spec.fn_sign,
                        )
                        .await
                        {
                            Ok(_) => {}
                            Err(e) => {
                                // W8b: durable CRITICAL audit on tick
                                // infrastructure error (DB unreachable
                                // / audit insert failure inside
                                // run_tick_for_fn).  Operator MUST see
                                // a persistent record; tracing alone
                                // is not sufficient.  If the audit
                                // append itself fails (e.g. same DB
                                // issue that caused the tick Err),
                                // fall back to tracing::error! and
                                // continue — the loop ticks again on
                                // the next interval, and audit will
                                // succeed when the DB recovers.
                                tracing::error!(
                                    error = %e,
                                    fiscal_number = %spec.fiscal_number,
                                    "return_online_probe: tick error (DB or audit insert failure)"
                                );
                                let payload = serde_json::json!({
                                    "fiscal_number": &spec.fiscal_number,
                                    "error": format!("{e:#}"),
                                });
                                // W8b Round 1 LOW #3: anchor on
                                // `return_online_probe` (generic
                                // tick-source) — the underlying
                                // `Err` may come from `node_state`
                                // read OR from audit_log insert
                                // inside the primitive; entity_type
                                // = "node_state" would mis-attribute
                                // the latter.  entity_id stays the
                                // FN since the tick is per-FN.
                                if let Err(audit_err) = audit_log::append(
                                    &pool,
                                    "return_online_probe",
                                    &spec.fiscal_number,
                                    "RETURN_ONLINE_PROBE_LOOP_ERROR",
                                    Severity::Critical,
                                    None,
                                    Some(&payload.to_string()),
                                )
                                .await
                                {
                                    tracing::error!(
                                        audit_error = %audit_err,
                                        fiscal_number = %spec.fiscal_number,
                                        "return_online_probe: CRITICAL audit insert failed; loop continues"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod ra_tests {
    use super::*;
    use crate::transports::dps::error::DpsError;

    #[test]
    fn dps_error_class_labels_remote_status_and_indeterminate_ra() {
        // RA all-consumers pin: the taxonomy string is DISTINCT for the CS-3 variants
        // (NOT collapsed to "Transport") — this is exactly why slice A/A′ was never
        // behaviour-neutral. Pinned so the taxonomy cannot silently regress.
        assert_eq!(
            dps_error_class(&DpsError::RemoteStatus {
                code: "Unauthenticated".into(),
                message: "x".into(),
                digest: prro_domain::delivery::RawResponseDigest([0u8; 32]),
            }),
            "RemoteStatus"
        );
        assert_eq!(
            dps_error_class(&DpsError::Indeterminate {
                code: -4,
                message: "y".into(),
                digest: prro_domain::delivery::RawResponseDigest([0u8; 32]),
            }),
            "Indeterminate"
        );
    }
}
