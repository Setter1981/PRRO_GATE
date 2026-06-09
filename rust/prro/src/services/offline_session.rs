//! Service-layer composition for the OfflineSession lifecycle (M3b W5).
//!
//! Wires `db::repositories::offline_sessions::*` to the runtime via
//! `with_immediate` envelopes — each public method is one BEGIN
//! IMMEDIATE transaction that performs the repository call(s) and
//! emits the corresponding audit event atomically.
//!
//! ## Audit event vocabulary (W5 plan §Task 5 line 454)
//!
//! | Transition          | event_type                    |
//! |---------------------|-------------------------------|
//! | Opening → Open      | `OFFLINE_SESSION_OPENED`      |
//! | Open → Draining     | `OFFLINE_SESSION_DRAIN_STARTED` |
//! | Draining → Closed   | `OFFLINE_SESSION_CLOSED`      |
//! | * → Aborted         | `OFFLINE_SESSION_ABORTED`     |
//!
//! Audit `entity_type` is always `"offline_session"`; `entity_id`
//! is the hex-lower encoding of the session id (parallel to the
//! `fiscal_document` audit convention in
//! [`crate::services::reconciliation::boot_phase`]).
//!
//! ## Typed-error preservation (W5 review axis 4)
//!
//! Service methods return `anyhow::Result<T>` per the existing
//! services convention (`cert_refresher`, `reconciliation`), but
//! [`crate::db::repositories::offline_sessions::OfflineSessionError`]
//! variants survive the wrap intact — callers recover the typed
//! variant via `err.downcast_ref::<OfflineSessionError>()`.  This
//! is the operator-pinned discipline (memory
//! `m3b-w5-review-criteria`): "service-layer may wrap into anyhow
//! for ergonomics, but the structural distinction must survive to
//! callers that need to branch on outcome".

use std::sync::Arc;

use anyhow::bail;
use sqlx::SqlitePool;

use crate::db::models::enums::{OfflineSessionState, Severity};
use crate::db::models::ids::OfflineSessionId;
use crate::db::repositories::audit_log;
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::offline_sessions::{self, NewOpeningSession};
use crate::db::tx::with_immediate;
use crate::services::write_path::types::hex_encode_lower as hex_lower;

/// Coordinator for offline-session state-machine writes.
///
/// Holds an `Arc<SqlitePool>` (not a reference) so the service can
/// be cloned into spawned tasks (return-online drain workers).
#[derive(Clone)]
pub struct OfflineSessionService {
    pool: Arc<SqlitePool>,
}

impl OfflineSessionService {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Open a new offline session for `fiscal_number`.
    ///
    /// Composes (in one `with_immediate` envelope):
    ///   1. `insert_opening` — INSERT OPENING row; partial UNIQUE
    ///      `ux_offline_active` rejects if any active session already
    ///      exists for this FN → typed `AnotherSessionActive`.
    ///   2. `transition_state(Opening → Open)` — CAS arrival into
    ///      OPEN.  Must produce `Applied` (we just INSERTed the row
    ///      one statement earlier in the same tx); any other outcome
    ///      is a programming bug.
    ///   3. `audit_log::append_tx` — emit `OFFLINE_SESSION_OPENED`.
    ///
    /// Returns the generated `OfflineSessionId` on success.
    pub async fn open_session(&self, fiscal_number: &str) -> anyhow::Result<OfflineSessionId> {
        let session_id = OfflineSessionId::new();
        let opened_at = chrono::Utc::now().to_rfc3339();
        let fn_owned: String = fiscal_number.to_string();

        with_immediate(&self.pool, move |tx| {
            Box::pin(async move {
                offline_sessions::insert_opening(
                    tx,
                    &NewOpeningSession {
                        offline_session_id: session_id,
                        fiscal_number: &fn_owned,
                        opened_at: &opened_at,
                    },
                )
                .await?;
                let outcome = offline_sessions::transition_state(
                    tx,
                    session_id,
                    OfflineSessionState::Opening,
                    OfflineSessionState::Open,
                    None,
                )
                .await?;
                if outcome != TransitionOutcome::Applied {
                    bail!("Opening→Open transition produced unexpected outcome: {outcome:?}");
                }
                let id_hex = hex_lower(session_id.as_bytes());
                audit_log::append_tx(
                    tx,
                    "offline_session",
                    &id_hex,
                    "OFFLINE_SESSION_OPENED",
                    Severity::Info,
                    None,
                    None,
                )
                .await?;
                Ok::<OfflineSessionId, anyhow::Error>(session_id)
            })
        })
        .await
    }

    /// Open → Draining: return-online detected, start backlog drain.
    ///
    /// Emits `OFFLINE_SESSION_DRAIN_STARTED` audit row on
    /// `TransitionOutcome::Applied`; non-Applied outcomes propagate
    /// without audit (mirrors the W1 `transition_with_audit`
    /// discipline: audit only on Applied).
    pub async fn start_drain(
        &self,
        session_id: OfflineSessionId,
    ) -> anyhow::Result<TransitionOutcome> {
        self.transition_with_audit(
            session_id,
            OfflineSessionState::Open,
            OfflineSessionState::Draining,
            None,
            "OFFLINE_SESSION_DRAIN_STARTED",
            Severity::Info,
        )
        .await
    }

    /// Draining → Closed: backlog drained, session finalised.
    pub async fn close_session(
        &self,
        session_id: OfflineSessionId,
    ) -> anyhow::Result<TransitionOutcome> {
        self.transition_with_audit(
            session_id,
            OfflineSessionState::Draining,
            OfflineSessionState::Closed,
            None,
            "OFFLINE_SESSION_CLOSED",
            Severity::Info,
        )
        .await
    }

    /// `from` → Aborted: abnormal exit with operator-supplied
    /// `reason_abort`.  Valid `from` values per whitelist: Opening,
    /// Open, Draining.  Empty `reason` → typed `MissingReasonAbort`.
    /// Severity is WARNING (not ERROR — abort can be operationally
    /// expected, e.g., operator-driven shift cutoff).
    pub async fn abort_session(
        &self,
        session_id: OfflineSessionId,
        from: OfflineSessionState,
        reason: &str,
    ) -> anyhow::Result<TransitionOutcome> {
        self.transition_with_audit(
            session_id,
            from,
            OfflineSessionState::Aborted,
            Some(reason),
            "OFFLINE_SESSION_ABORTED",
            Severity::Warning,
        )
        .await
    }

    /// Internal composer: runs `transition_state` + audit (on Applied)
    /// inside one `with_immediate` envelope.  Parallel shape to W1's
    /// `transition_with_audit` in
    /// `services::reconciliation::boot_phase`, but
    /// `entity_type = "offline_session"` and the inner repository
    /// call is `offline_sessions::transition_state` (not
    /// `fiscal_documents::transition_state`).
    async fn transition_with_audit(
        &self,
        session_id: OfflineSessionId,
        from: OfflineSessionState,
        to: OfflineSessionState,
        reason_abort: Option<&str>,
        event_type: &'static str,
        severity: Severity,
    ) -> anyhow::Result<TransitionOutcome> {
        let reason_owned: Option<String> = reason_abort.map(|s| s.to_string());

        with_immediate(&self.pool, move |tx| {
            Box::pin(async move {
                let outcome = offline_sessions::transition_state(
                    tx,
                    session_id,
                    from,
                    to,
                    reason_owned.as_deref(),
                )
                .await?;
                if outcome == TransitionOutcome::Applied {
                    let id_hex = hex_lower(session_id.as_bytes());
                    // Operator review pin (PR #54 Round 1 MED-2,
                    // 2026-05-15): include `reason_abort` in the
                    // ABORTED audit payload so the audit trail
                    // captures the operator's rationale, not just
                    // the bare from→to transition.  Non-ABORTED
                    // transitions emit null for the field; this
                    // keeps the audit payload schema uniform.
                    let payload = serde_json::json!({
                        "offline_session_id": id_hex,
                        "from": from.as_str(),
                        "to": to.as_str(),
                        "reason_abort": reason_owned,
                    });
                    audit_log::append_tx(
                        tx,
                        "offline_session",
                        &id_hex,
                        event_type,
                        severity,
                        None,
                        Some(&payload.to_string()),
                    )
                    .await?;
                }
                Ok::<TransitionOutcome, anyhow::Error>(outcome)
            })
        })
        .await
    }

    /// Admin/provisioning seam: seed the FN-scoped code pool with
    /// codes in the inclusive range `[first_lnd ..= last_lnd]`.
    /// Idempotent via INSERT OR IGNORE.  Returns the count of rows
    /// actually inserted.
    ///
    /// Pool-level ergonomic wrapper around the tx-bound
    /// [`offline_sessions::seed_code_range_tx`] — opens its own
    /// `with_immediate` envelope so admin callers don't need to
    /// hold a `WriteTxConn` themselves.  Operator review pin (PR
    /// #54 Round 1 HIGH, 2026-05-15): the seed primitive itself
    /// MUST be tx-bound (W5 axis 2 discipline); pool-level
    /// ergonomics live in the service layer.
    pub async fn seed_code_range(
        &self,
        fiscal_number: &str,
        first_lnd: i64,
        last_lnd: i64,
    ) -> anyhow::Result<u64> {
        let fn_owned: String = fiscal_number.to_string();
        with_immediate(&self.pool, move |tx| {
            Box::pin(async move {
                let inserted =
                    offline_sessions::seed_code_range_tx(tx, &fn_owned, first_lnd, last_lnd)
                        .await?;
                Ok::<u64, anyhow::Error>(inserted)
            })
        })
        .await
    }
}
