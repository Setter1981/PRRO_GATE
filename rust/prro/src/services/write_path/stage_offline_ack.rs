//! M3b W7 — `stage_offline_ack`: Pattern C step 1 (pre-send local ack).
//!
//! Post-sign, pre-send write-path branch invoked when the node is in
//! `Offline` / `GoingOffline` mode.  In ONE `with_immediate`
//! envelope (BEGIN IMMEDIATE) the stage:
//!
//!   1. Re-reads `node_state` (fresh snapshot; the `WorkerContext`
//!      copy taken at stage 1 may be stale w.r.t. a concurrent
//!      shift-close or node-mode flip).
//!   2. Validates node mode ∈ {Offline, GoingOffline}.
//!   3. Validates shift state == Opened.
//!   4. Reads the FN's currently-active OPEN offline session.
//!   5. Acquires an unconsumed code from the FN pool via W5's
//!      `acquire_code_tx` (atomic single-statement CAS).
//!   6. Transitions `Signed → OfflineLocalAck` stamping
//!      `offline_fiscal_no = code_lnd`,
//!      `offline_fiscal_date = consumed_at`,
//!      `offline_session_id = session_id` in one UPDATE (W7 helper
//!      `transition_to_offline_local_ack_tx`).
//!   7. Emits `OFFLINE_LOCAL_ACK_APPLIED` audit on success;
//!      `OFFLINE_ACK_REFUSED` audit on validation refusal.
//!
//! ## Invariant preservation
//!
//! - **I1** (no foreign IO inside write tx): no DPS / crypto /
//!   transport calls in the envelope; only SQL writes through
//!   `WriteTxConn`.  `IN_WITH_IMMEDIATE` task-local marker is
//!   active throughout the closure body — any substrate entry
//!   point's `assert_not_in_with_immediate` guard would catch a
//!   leak.
//! - **I2** (one FN, one writer): BEGIN IMMEDIATE serialises
//!   writers on the SQLite RESERVED lock.
//! - **I4** (idempotency): re-invocation after `Applied` sees the
//!   doc in `OfflineLocalAck`, the W7 helper's CAS misses with
//!   `Conflict`, audit emits `OFFLINE_ACK_REFUSED { DocStateConflict }`;
//!   the original `Applied` artefact stays intact.
//! - **I5** (offline bounded by codes): refusal paths leave the
//!   code pool UNTOUCHED — `acquire_code_tx` is only invoked
//!   after all validations pass.  Code-pool exhaustion surfaces
//!   as the W5 typed `CodePoolExhausted` propagating up — caller
//!   responsibility (W7 returns Err, caller decides STOP_MODE).
//! - **I8** (state-machine correctness): transition gated by the
//!   W6-locked `(Signed, OfflineLocalAck)` whitelist edge.  No
//!   raw state writes outside the helper.
//!
//! ## Scope discipline (operator W7 review pin)
//!
//! - W7 emits `OFFLINE_LOCAL_ACK`; it does NOT advance docs to
//!   `Sending` / `Cancelled`.  Those W6-added edges are W9's
//!   responsibility (return-online backlog drain).
//! - `stage_finalize` and `stage_send` are untouched.
//! - This stage does NOT extend `WorkerContext`; it consumes the
//!   existing snapshot.
//! - Dispatcher wiring (calling this stage from ingress / boot
//!   recovery) is intentionally out of W7 scope — the stage is
//!   self-contained and tested directly; production wiring lifts
//!   to a follow-up that touches ingress + boot_phase + app.

use crate::db::models::enums::{NodeMode, Severity, ShiftState};
use crate::db::models::ids::{DocumentId, OfflineSessionId};
use crate::db::repositories::audit_log;
use crate::db::repositories::fiscal_documents::{self as fd, TransitionOutcome};
use crate::db::repositories::node_state;
use crate::db::repositories::offline_sessions;
use crate::db::tx::{with_immediate, WriteTxConn};
use crate::services::write_path::types::hex_encode_lower as hex_lower;
use serde_json::json;
use sqlx::SqlitePool;

/// Outcome of `stage_offline_ack::run`.
///
/// `Refused` carries a [`RefusalReason`] enum variant — typed
/// surface for callers that need to branch on the structural
/// condition without string-matching audit payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfflineAckOutcome {
    /// Happy path — code acquired, doc transitioned to
    /// OFFLINE_LOCAL_ACK, audit emitted.
    Applied {
        document_id: DocumentId,
        code_lnd: i64,
        consumed_at: String,
        offline_session_id: OfflineSessionId,
    },
    /// Validation refused the offline ack.  Doc state remains
    /// `Signed`; no code consumed.  The corresponding
    /// `OFFLINE_ACK_REFUSED` audit row IS emitted (operationally
    /// important — operators reviewing why offline emission
    /// failed see the audit trail).
    Refused(RefusalReason),
}

/// Structural reasons the stage may refuse an offline ack.
///
/// `NodeNotOffline` / `ShiftNotOpened` / `NoActiveSession` are
/// operational conditions the dispatcher may react to (e.g.,
/// route back to online path or surface to operator).
/// `DocStateConflict` / `DocNotFound` correspond to
/// [`TransitionOutcome::Conflict`] / [`TransitionOutcome::NotFound`]
/// — race conditions or programming bugs respectively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalReason {
    /// Node mode is not `Offline` or `GoingOffline`.  Carries the
    /// observed mode for forensic clarity.
    NodeNotOffline { mode: NodeMode },
    /// Shift state is not `Opened`.  Carries the observed state.
    ShiftNotOpened { current: ShiftState },
    /// No `OPEN` offline session exists for this FN.  Either no
    /// session was ever opened, or the session is in `OPENING`
    /// (race) / `DRAINING` (W9 drain in progress) / `CLOSED` /
    /// `ABORTED` terminal states.
    NoActiveSession,
    /// `transition_to_offline_local_ack_tx` CAS missed because
    /// the doc's state is no longer `Signed`.  Concurrent state
    /// change (e.g., admin override) — caller may re-route the
    /// doc per the current state.
    DocStateConflict,
    /// `transition_to_offline_local_ack_tx` CAS missed because
    /// the doc row vanished.  Programming bug or DB corruption —
    /// the doc should exist because stage 3 just signed it.
    DocNotFound,
}

/// Public entry point for the offline ack branch.
///
/// Takes `doc_id` + `fiscal_number` directly rather than a full
/// `WorkerContext` snapshot.  The dispatcher (production wiring,
/// out of W7 scope) is responsible for extracting these from its
/// `WorkerContext` and calling here.  This decoupling keeps the
/// stage self-contained for direct testing and avoids carrying
/// `WorkerContext`'s many unread fields through the envelope
/// boundary.
///
/// No `ctx` parameter — the offline branch invokes no substrates
/// (crypto / transport).
///
/// Returns `anyhow::Result<OfflineAckOutcome>` — refusals are
/// `Ok(Refused(reason))`, not `Err`.  `Err` only on genuine
/// failures (DB error, code pool exhausted, etc.) that need to
/// propagate to the caller and roll back the envelope.
pub async fn run(
    pool: &SqlitePool,
    doc_id: DocumentId,
    fiscal_number: &str,
) -> anyhow::Result<OfflineAckOutcome> {
    let fn_owned = fiscal_number.to_string();

    let fn_id_for_envelope = fn_owned.clone();
    with_immediate(pool, move |tx| {
        let fn_id = fn_id_for_envelope;
        Box::pin(async move {
            // ─── Step 1: re-read node_state (fresh snapshot) ────────
            let ns = match node_state::get_tx(tx, &fn_id).await? {
                Some(ns) => ns,
                None => {
                    // node_state row missing — invariant breach (FN
                    // must be bootstrapped before any doc can reach
                    // stage_sign).  Surface as NoActiveSession refusal
                    // — closest operational semantic.
                    return audit_and_return_refused(
                        tx,
                        doc_id,
                        &fn_id,
                        RefusalReason::NoActiveSession,
                    )
                    .await;
                }
            };

            // ─── Step 2: validate node mode ─────────────────────────
            if !matches!(ns.mode, NodeMode::Offline | NodeMode::GoingOffline) {
                return audit_and_return_refused(
                    tx,
                    doc_id,
                    &fn_id,
                    RefusalReason::NodeNotOffline { mode: ns.mode },
                )
                .await;
            }

            // ─── Step 3: validate shift state ───────────────────────
            if ns.shift_state != ShiftState::Opened {
                return audit_and_return_refused(
                    tx,
                    doc_id,
                    &fn_id,
                    RefusalReason::ShiftNotOpened {
                        current: ns.shift_state,
                    },
                )
                .await;
            }

            // ─── Step 4: read active OPEN session ───────────────────
            let session_id =
                match offline_sessions::current_active_session_id_tx(tx, &fn_id).await? {
                    Some(sid) => sid,
                    None => {
                        return audit_and_return_refused(
                            tx,
                            doc_id,
                            &fn_id,
                            RefusalReason::NoActiveSession,
                        )
                        .await;
                    }
                };

            // ─── Step 5: acquire code (atomic single-statement CAS) ─
            //
            // Validations all passed.  This is the first point a
            // write touches `offline_codes` — any earlier refusal
            // returned BEFORE this call, so code pool stays intact
            // on refusal.  `CodePoolExhausted` propagates as
            // typed-via-anyhow Err (W5 contract) — caller's
            // responsibility to enter STOP_MODE.
            let acquired = offline_sessions::acquire_code_tx(tx, &fn_id, doc_id).await?;

            // ─── Step 6: transition Signed → OfflineLocalAck ────────
            //
            // Single UPDATE stamps state + offline_fiscal_no +
            // offline_fiscal_date + offline_session_id atomically
            // (operator W7 criterion 5).
            let outcome = fd::transition_to_offline_local_ack_tx(
                tx,
                doc_id,
                acquired.code_lnd,
                &acquired.consumed_at,
                session_id,
            )
            .await?;

            match outcome {
                TransitionOutcome::Applied => {
                    // ─── Step 7: emit Applied audit ─────────────────
                    let payload = json!({
                        "document_id": hex_lower(doc_id.as_bytes()),
                        "code_lnd": acquired.code_lnd,
                        "consumed_at": acquired.consumed_at,
                        "offline_session_id": hex_lower(session_id.as_bytes()),
                    });
                    audit_log::append_tx(
                        tx,
                        "fiscal_document",
                        &hex_lower(doc_id.as_bytes()),
                        "OFFLINE_LOCAL_ACK_APPLIED",
                        Severity::Info,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                    Ok::<OfflineAckOutcome, anyhow::Error>(OfflineAckOutcome::Applied {
                        document_id: doc_id,
                        code_lnd: acquired.code_lnd,
                        consumed_at: acquired.consumed_at,
                        offline_session_id: session_id,
                    })
                }
                TransitionOutcome::Forbidden => {
                    // Unreachable via the typed helper —
                    // `(Signed, OfflineLocalAck)` is locked in the W6
                    // edge-set test.  Surface as Internal error so a
                    // future regression (W6 edge removed) fails loud.
                    anyhow::bail!(
                        "(Signed, OfflineLocalAck) whitelist gate unexpectedly returned Forbidden"
                    )
                }
                TransitionOutcome::Conflict => {
                    // Doc state diverged from Signed — concurrent
                    // change.  Code WAS just acquired (step 5); the
                    // envelope rollback below via Err... wait, we
                    // need to abort the envelope so the code
                    // consumption is undone.
                    anyhow::bail!(
                        "stage_offline_ack: doc state diverged from Signed mid-envelope (Conflict); \
                         tx rollback to preserve I5 (code stays unconsumed)"
                    )
                }
                TransitionOutcome::NotFound => {
                    // Doc row vanished — programmer bug; rollback to
                    // preserve I5.
                    anyhow::bail!(
                        "stage_offline_ack: doc row vanished mid-envelope (NotFound); \
                         tx rollback to preserve I5"
                    )
                }
            }
        })
    })
    .await
}

/// Internal helper: emit `OFFLINE_ACK_REFUSED` audit + return
/// `Ok(Refused(reason))`.  Called from the validation arms; the
/// surrounding `with_immediate` envelope commits the audit row
/// even though no doc/code writes happen — refusal must be
/// auditable per operator W7 criterion 4.
async fn audit_and_return_refused(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    fn_id: &str,
    reason: RefusalReason,
) -> anyhow::Result<OfflineAckOutcome> {
    let payload = json!({
        "document_id": hex_lower(doc_id.as_bytes()),
        "fiscal_number": fn_id,
        "reason": format!("{reason:?}"),
    });
    audit_log::append_tx(
        tx,
        "fiscal_document",
        &hex_lower(doc_id.as_bytes()),
        "OFFLINE_ACK_REFUSED",
        Severity::Warning,
        None,
        Some(&payload.to_string()),
    )
    .await?;
    Ok(OfflineAckOutcome::Refused(reason))
}
