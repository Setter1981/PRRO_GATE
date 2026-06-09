//! M3b W7b — post-sign dispatcher.
//!
//! Routes a `Signed` doc to one of three outcomes based on the
//! current node mode:
//!
//! - **`Online`** → caller continues to `stage_send::run` (M3a
//!   happy path unchanged).  Dispatcher returns
//!   [`PostSignRoute::Online`] without invoking anything itself.
//! - **`Offline | GoingOffline`** → dispatcher invokes
//!   `stage_offline_ack::run` (W7a) and returns
//!   [`PostSignRoute::Offline(OfflineAckOutcome)`].  **Pipeline
//!   terminates here**; caller MUST NOT continue to `stage_send`.
//! - **`Blocked | StopMode | CryptoDegraded | GoingOnline`** →
//!   typed dispatcher refusal.  Returns
//!   [`PostSignRoute::Refused(DispatcherRefusalReason)`] + emits
//!   `WRITE_PATH_DISPATCH_REFUSED` audit row.  Doc state stays in
//!   `Signed`; no stage invocation.
//!
//! ## Why dispatcher-level refusal (operator W7b criterion 4)
//!
//! The W7a `stage_offline_ack` stage has its own
//! `NodeNotOffline { mode }` safety-net refusal for these modes,
//! but the dispatcher catches them at the OUTER routing level.
//! Rationale: the 4 non-routable modes represent operational
//! states where the node should refuse ALL fiscalisation (not
//! re-route to a "different" path).  Routing through the offline
//! stage just for it to refuse internally would (a) read node_state
//! twice, (b) muddle the audit trail, (c) accidentally consume code
//! pool side-effects in some future refactor.
//!
//! ## Why dispatcher-level read of node_state (operator W7b
//! criterion 1)
//!
//! Caller's `WorkerContext.node_state` snapshot may be stale by the
//! time the doc reaches the post-sign point (e.g., concurrent
//! shift_close, operator-initiated mode flip).  The dispatcher
//! re-reads node_state inside its own short read to make the
//! mode-routing decision on fresh data.  This read is pool-bound
//! (autocommit) — the actual write envelope (in stage_send or
//! stage_offline_ack) re-reads inside its `with_immediate` and
//! has the final say.  Dispatcher-level read is a routing-hint,
//! NOT a correctness gate.
//!
//! ## Invariant preservation
//!
//! - **I1**: dispatcher itself does no DPS / crypto / transport
//!   calls.  The `Online` branch returns immediately for caller
//!   to invoke stage_send; the `Offline` branch invokes
//!   stage_offline_ack which is itself I1-clean (W7a).
//! - **I8**: refused modes leave doc state untouched (still
//!   `Signed`); typed refusal surfaces structurally distinct from
//!   stage-level outcomes.

use crate::db::models::enums::{NodeMode, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::audit_log;
use crate::db::repositories::node_state;
use crate::services::write_path::stage_offline_ack::{self, OfflineAckOutcome};
use crate::services::write_path::types::hex_encode_lower as hex_lower;
use serde_json::json;
use sqlx::SqlitePool;

/// Outcome of `dispatch_post_sign`.
///
/// `Online` indicates the caller MUST proceed to `stage_send::run`
/// (M3a online ladder).  `Offline` indicates the offline stage
/// has run and the pipeline TERMINATES here — caller MUST NOT
/// invoke stage_send.  `Refused` indicates the doc cannot be
/// fiscalised in the current node mode; caller does nothing
/// further (audit already emitted by dispatcher).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostSignRoute {
    /// Route to online ladder.  Caller continues to
    /// `stage_send::run`.  Payload `mode` is the observed mode
    /// for telemetry (always `NodeMode::Online`).
    Online { mode: NodeMode },
    /// Routed through `stage_offline_ack::run`; pipeline
    /// terminated.  Payload carries the offline stage's outcome
    /// (Applied with the acquired code, OR Refused if the stage's
    /// internal validations failed AFTER the dispatcher passed).
    Offline {
        mode: NodeMode,
        outcome: OfflineAckOutcome,
    },
    /// Refused at dispatcher level; doc state unchanged.
    Refused(DispatcherRefusalReason),
}

/// Typed reasons the dispatcher may refuse to route a doc.
///
/// All 4 variants correspond to node modes that the operator W7b
/// pin explicitly named as non-routable: docs in `Signed` state
/// cannot proceed in these modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatcherRefusalReason {
    /// Node is `Blocked` (manual operator block or auto-block
    /// from a fatal DPS error per spec §5.5).  Recovery requires
    /// operator action.
    NodeBlocked,
    /// Node is in `StopMode` (offline code pool exhausted per
    /// spec §5.5; doc cannot fiscalise locally either).
    NodeStopMode,
    /// Node is in `CryptoDegraded` (cert / sidecar failure;
    /// signing newly emitted docs would not be safe even though
    /// THIS doc already passed stage_sign — preserves I10).
    NodeCryptoDegraded,
    /// Node is in `GoingOnline` (W9 backlog-drain in progress).
    /// Emitting a new offline_local_ack would extend the drain
    /// set, which is W7-out-of-scope per criterion 6 of
    /// `m3b-w7-review-criteria`.
    NodeGoingOnline,
}

/// Read current node mode + branch.  See module docs for the full
/// routing decision table.
///
/// `fiscal_number` is the doc's FN (caller passes from
/// `DocumentRow.fiscal_number`).  Dispatcher re-reads
/// `node_state.mode` inside this fn; it does NOT consult the
/// caller's stale `WorkerContext.node_state` (operator W7b
/// criterion 1 — fresh routing decision).
///
/// Pool-bound for the routing read; `stage_offline_ack::run` opens
/// its own `with_immediate` envelope internally on the Offline
/// branch.
pub async fn dispatch_post_sign(
    pool: &SqlitePool,
    doc_id: DocumentId,
    fiscal_number: &str,
) -> anyhow::Result<PostSignRoute> {
    let ns = match node_state::get(pool, fiscal_number).await? {
        Some(ns) => ns,
        None => {
            // node_state row missing — invariant breach (FN must
            // be bootstrapped before any doc can reach this
            // dispatcher).  Treat as refusal (closest semantic);
            // `NodeBlocked` is the safest fallback because it
            // requires operator action to clear.
            return audit_and_return_refused(
                pool,
                doc_id,
                fiscal_number,
                "<missing-node_state>",
                DispatcherRefusalReason::NodeBlocked,
            )
            .await;
        }
    };

    match ns.mode {
        NodeMode::Online => Ok(PostSignRoute::Online { mode: ns.mode }),
        NodeMode::Offline | NodeMode::GoingOffline => {
            // Pipeline terminates here for this doc.  W7a stage
            // handles its own validations + audit; we just thread
            // its outcome up to the caller.
            let outcome = stage_offline_ack::run(pool, doc_id, fiscal_number).await?;
            Ok(PostSignRoute::Offline {
                mode: ns.mode,
                outcome,
            })
        }
        NodeMode::Blocked => {
            audit_and_return_refused(
                pool,
                doc_id,
                fiscal_number,
                "BLOCKED",
                DispatcherRefusalReason::NodeBlocked,
            )
            .await
        }
        NodeMode::StopMode => {
            audit_and_return_refused(
                pool,
                doc_id,
                fiscal_number,
                "STOP_MODE",
                DispatcherRefusalReason::NodeStopMode,
            )
            .await
        }
        NodeMode::CryptoDegraded => {
            audit_and_return_refused(
                pool,
                doc_id,
                fiscal_number,
                "CRYPTO_DEGRADED",
                DispatcherRefusalReason::NodeCryptoDegraded,
            )
            .await
        }
        NodeMode::GoingOnline => {
            audit_and_return_refused(
                pool,
                doc_id,
                fiscal_number,
                "GOING_ONLINE",
                DispatcherRefusalReason::NodeGoingOnline,
            )
            .await
        }
    }
}

/// Emit `WRITE_PATH_DISPATCH_REFUSED` audit + return `Refused`.
///
/// Audit payload carries `{document_id, fiscal_number, observed_mode,
/// reason}` so operator review can reconstruct exactly which mode
/// caused the refusal without joining tables.  Pool-bound — audit
/// emission is fire-and-forget at this layer (no atomic invariant
/// with other writes since refusal performs no other write).
async fn audit_and_return_refused(
    pool: &SqlitePool,
    doc_id: DocumentId,
    fiscal_number: &str,
    observed_mode: &str,
    reason: DispatcherRefusalReason,
) -> anyhow::Result<PostSignRoute> {
    let doc_hex = hex_lower(doc_id.as_bytes());
    let payload = json!({
        "document_id": doc_hex,
        "fiscal_number": fiscal_number,
        "observed_mode": observed_mode,
        "reason": format!("{reason:?}"),
    });
    audit_log::append(
        pool,
        "fiscal_document",
        &doc_hex,
        "WRITE_PATH_DISPATCH_REFUSED",
        Severity::Warning,
        None,
        Some(&payload.to_string()),
    )
    .await?;
    Ok(PostSignRoute::Refused(reason))
}
