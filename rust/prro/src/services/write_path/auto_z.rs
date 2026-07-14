//! T3 (RULING 3.4) — the UNCONDITIONAL shift-limit auto-Z.
//!
//! When the open shift for an FN reaches its 24h document-derived budget
//! (`now(clock) − SHIFT_OPEN.business_ts ≥ 24h`), the gateway makes a DURABLE Z
//! attempt REGARDLESS of any enforcement toggle: "a shift never crosses the
//! limit without a durable Z attempt/outcome (success, or an escalated failure —
//! RMR — but never silent continuation)".
//!
//! Mechanism (deliberately reuses the whole live Z path, not a parallel one): a
//! synthetic `Z_REPORT` inbox row is minted with a DETERMINISTIC idempotency key
//! (`autoz-<fn>-<shift_hex>`, one per shift → crash-safe / re-tick idempotent)
//! and driven through [`inline::run`], which routes Z-class ops to
//! `run_z_dispatch`:
//!   - ONLINE shift  → normal online Z dispatch (edges 8/10; failure → edges
//!     4/12 → `RequiresManualReconciliation`).
//!   - OFFLINE shift → Pattern-C local Z (`OFFLINE_LOCAL_ACK`; the T2 close-
//!     reserve guarantees a pool code is available).
//!
//! A Z FAILURE therefore takes the EXISTING Z failure routes (retry / RMR),
//! never a silent continuation — the ruling's core guarantee, for free.
//!
//! The 24h budget is UNCONDITIONAL: it does NOT consult the per-budget
//! enforcement toggles (those gate only the admission REFUSAL of new ordinary
//! ops). The auto-Z's own inbox row is a `Z_REPORT` (a close-path doc), so the
//! admission time-budget gate bypasses it by doc-type — the Z is never blocked
//! by its own over-budget shift.

use sqlx::SqlitePool;
use tokio::sync::OwnedMutexGuard;

use crate::db::models::enums::{Protocol, ShiftState};
use crate::db::repositories::ingress_inbox::{self, InboxInsertOutcome, NewInboxEntry};
use crate::db::repositories::node_state;
use crate::db::types::DbShiftId;
use crate::runtime::ingress::seam::FiscalError;
use crate::services::time_budget::{self, Clock, EnforcementToggles, TimeBudgetGate};
use crate::services::write_path::inline;
use crate::services::write_path::stage_sign::SigningContext;
use crate::services::write_path::types::hex_encode_lower;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::CheckSignBlob;

/// Outcome of an auto-Z tick for one FN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoZOutcome {
    /// The shift is under the 24h budget, OR there is no open closable shift →
    /// nothing to do.
    NotDue,
    /// A durable Z was issued (reached `OFFLINE_LOCAL_ACK` offline or `ACK`
    /// online), OR an already-issued Z for this shift was resolved on a re-tick.
    Issued,
    /// The Z attempt did NOT succeed but was handled durably (escalated to
    /// `RequiresManualReconciliation` via the existing Z failure route, or a
    /// retryable transient recorded) — NEVER a silent continuation. Carries the
    /// HTTP-facing code from the underlying `FiscalError` for observability.
    Escalated { code: String },
}

/// The deterministic synthetic idempotency key / request_id anchor for the
/// auto-Z of a given shift. One per shift → a crash mid-Z + re-tick resolves the
/// SAME inbox row (idempotency, INV-7) instead of minting a second Z.
fn auto_z_idempotency_key(fiscal_number: &str, shift_id_hex: &str) -> String {
    format!("autoz-{fiscal_number}-{shift_id_hex}")
}

fn auto_z_request_id(idempotency_key: &str) -> [u8; 16] {
    let h: [u8; 32] = <sha2::Sha256 as sha2::Digest>::digest(idempotency_key.as_bytes()).into();
    let mut r = [0u8; 16];
    r.copy_from_slice(&h[..16]);
    r
}

/// Whether a shift state is one the auto-Z can close (an OPENED-family shift that
/// still owes a Z). Terminal / in-progress-close states are skipped (a `Closing`
/// / `ClosingLocalPendingDrain` is already being closed; `Closed` / `Error` /
/// `RequiresManualReconciliation` are terminal; `Created` / `Opening` have not
/// reached a closable steady state).
fn is_auto_z_closable(state: ShiftState) -> bool {
    matches!(
        state,
        ShiftState::Opened | ShiftState::OpenedLocalPendingDrain
    )
}

/// Run the unconditional shift-limit auto-Z for one FN (see module docs).
/// Caller holds the FN write-lease (`_gate`) for the whole scope (INV-2).
#[allow(clippy::too_many_arguments)]
pub async fn run_auto_z_if_over_limit(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_sign: &CheckSignBlob,
    _gate: &OwnedMutexGuard<()>,
    fiscal_number: &str,
    clock: &dyn Clock,
) -> anyhow::Result<AutoZOutcome> {
    // (1) Is there an open, closable shift?
    let ns = match node_state::get(pool, fiscal_number).await? {
        Some(ns) => ns,
        None => return Ok(AutoZOutcome::NotDue),
    };
    if !is_auto_z_closable(ns.shift_state) {
        return Ok(AutoZOutcome::NotDue);
    }
    let Some(shift_id) = ns.current_shift_id else {
        return Ok(AutoZOutcome::NotDue);
    };

    // (2) Is the 24h shift budget over the boundary? UNCONDITIONAL — no toggle.
    let budgets = time_budget::compute_budgets_for_fn(pool, clock, fiscal_number).await?;
    if !budgets.shift_over_24h() {
        return Ok(AutoZOutcome::NotDue);
    }

    // The synthetic Z must carry the shift's OWN recovery identity (driver_id +
    // signing cashier) — the Z build path requires both (z_builder). Recover them
    // from the shift's SHIFT_OPEN doc: `signed_by_cashier_id` is on
    // `fiscal_documents`; `driver_id` lives on the originating `ingress_inbox`
    // row (joined by request_id). So the auto-Z signs as the shift owner.
    let ident: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ii.driver_id, fd.signed_by_cashier_id \
         FROM fiscal_documents fd \
         JOIN ingress_inbox ii ON ii.request_id = fd.request_id \
         WHERE fd.fiscal_number = ? AND fd.shift_id = ? AND fd.doc_type = 'SHIFT_OPEN' \
         ORDER BY fd.lnd ASC LIMIT 1",
    )
    .bind(fiscal_number)
    .bind(DbShiftId(shift_id))
    .fetch_optional(pool)
    .await?;
    let (open_driver_id, open_cashier_id) = match ident {
        Some((d, c)) => (d, c),
        None => {
            // No SHIFT_OPEN doc for an open, over-budget shift is a structural
            // breach — but never panic on the tick path; escalate observably.
            tracing::error!(
                fiscal_number,
                "auto_z: over-limit shift has no SHIFT_OPEN doc to recover identity"
            );
            return Ok(AutoZOutcome::Escalated {
                code: "AUTO_Z_NO_SHIFT_OPEN_DOC".to_string(),
            });
        }
    };

    // (3) Mint (idempotently) the synthetic Z_REPORT inbox row for THIS shift and
    // drive it through the real write path. A deterministic key means a re-tick
    // after a crash mid-Z resolves the SAME row (INV-7), never a second Z.
    let shift_hex = hex_encode_lower(shift_id.as_bytes());
    let idem = auto_z_idempotency_key(fiscal_number, &shift_hex);
    let request_id = auto_z_request_id(&idem);
    // A Z carries no total; business_ts = the injected now (the close instant).
    let business_ts = clock.now_utc().to_rfc3339();
    let payload_json = "{}".to_string();
    let payload_sha256_canonical: [u8; 32] =
        <sha2::Sha256 as sha2::Digest>::digest(payload_json.as_bytes()).into();
    let entry = NewInboxEntry {
        request_id,
        fiscal_number: fiscal_number.to_string(),
        protocol: Protocol::Rest,
        operation_type: "Z_REPORT".to_string(),
        idempotency_key: idem.clone(),
        payload_json,
        payload_sha256_canonical,
        correlation_id: Some("auto_z".to_string()),
        // The Z is signed as the shift's OWNER (recovered from the SHIFT_OPEN
        // doc) — the Z build path requires both driver_id + signing cashier.
        signed_by_cashier_id: open_cashier_id,
        driver_id: open_driver_id,
        business_ts: Some(business_ts),
        total_sum_kop: None,
    };

    let row = match ingress_inbox::insert(pool, &entry).await? {
        InboxInsertOutcome::Created(row) | InboxInsertOutcome::Replay(row) => row,
        InboxInsertOutcome::Conflict { .. } => {
            // A DIFFERENT payload already used this deterministic key — impossible
            // for our fixed synthetic payload, but never panic on a runtime path.
            tracing::error!(
                fiscal_number,
                shift = %shift_hex,
                "auto_z: idempotency-key conflict on the synthetic Z row (unexpected)"
            );
            return Ok(AutoZOutcome::Escalated {
                code: "AUTO_Z_INBOX_CONFLICT".to_string(),
            });
        }
    };

    // The auto-Z's own row is a close-path Z → the time-budget admission gate
    // bypasses it by doc-type. Pass an enforcement-OFF gate carrying the injected
    // clock (behaviour-neutral for a Z; keeps the ONE clock threaded).
    let budget_gate = TimeBudgetGate::new(
        clock,
        EnforcementToggles {
            shift_24h: false,
            session_36h: false,
            month_168h: false,
        },
    );

    match inline::run(
        pool,
        pool_secure,
        dps,
        sign_ctx,
        fn_sign,
        _gate,
        &row,
        budget_gate,
    )
    .await
    {
        Ok(outcome) => {
            tracing::info!(
                fiscal_number,
                shift = %shift_hex,
                doc_state = ?outcome.document_state,
                "auto_z: durable Z issued at the 24h shift boundary"
            );
            Ok(AutoZOutcome::Issued)
        }
        Err(e) => {
            // NEVER silent: run_z_dispatch already routed a hard reject to
            // RequiresManualReconciliation (edges 4/12) or recorded a retryable
            // transient; surface the code for the tick's audit. A subsequent tick
            // re-drives (retryable) or sees the terminal (RMR) and does not loop.
            let code = fiscal_error_code(&e);
            tracing::warn!(
                fiscal_number,
                shift = %shift_hex,
                code = %code,
                error = %format!("{e:?}"),
                "auto_z: Z attempt did not succeed — handled via the existing Z \
                 failure route (retry / RMR), never silent"
            );
            Ok(AutoZOutcome::Escalated { code })
        }
    }
}

/// The stable HTTP-facing code carried by a `FiscalError` (mirror of
/// `inline_map::code_of`, re-derived here to avoid widening that crate-private
/// helper's visibility for this observability-only use).
fn fiscal_error_code(e: &FiscalError) -> String {
    match e {
        FiscalError::ShiftGuardRefused { code, .. }
        | FiscalError::OfflineRefused { code, .. }
        | FiscalError::Internal { code, .. } => code.to_string(),
        FiscalError::ShiftNotOpen { .. } => "NO_OPEN_SHIFT".to_string(),
        FiscalError::DpsRejected { .. } => "FISCAL_REJECTED".to_string(),
        FiscalError::SignFailure { .. } => "SIGN_FAILED".to_string(),
        FiscalError::ZSurfaceNotReady { .. } => "Z_SURFACE_NOT_READY".to_string(),
        FiscalError::NotImplemented { .. } => "NOT_IMPLEMENTED".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FW-1 mutation teeth — `auto_z_idempotency_key -> "xyzzy"` (constant).
    /// The key is `autoz-<fn>-<shift_hex>`, UNIQUE PER SHIFT (the module invariant
    /// "one per shift"). A constant collapses the key across shifts, so a SECOND
    /// over-limit shift on the same FN replays the FIRST shift's Z inbox row →
    /// reports Issued but mints NO Z for shift #2, leaving it over the 24h wall
    /// with no durable Z (RULING 3.4 violation, frozen INV #4 idempotency). The
    /// `t3_auto_z_ticker` suite only ever drives ONE shift per FN, so the
    /// cross-shift collision is unpinned. This pins per-shift distinctness (and
    /// the request_id anchor derived from it) directly at the source.
    #[test]
    fn auto_z_key_and_request_id_are_distinct_per_shift() {
        let fnum = "4000000042";
        let k1 = auto_z_idempotency_key(fnum, "aaaaaaaa");
        let k2 = auto_z_idempotency_key(fnum, "bbbbbbbb");
        // Different shifts (same FN) → different keys. A constant fails this.
        assert_ne!(
            k1, k2,
            "auto-Z idempotency key must be per-shift; a constant collapses \
             shift #2 onto shift #1's Z (RULING 3.4: no shift crosses 24h without a Z)"
        );
        // The request_id anchor (sha256 of the key) must therefore also differ.
        assert_ne!(
            auto_z_request_id(&k1),
            auto_z_request_id(&k2),
            "auto-Z request_id (sha256 of the key) must be per-shift"
        );
        // Same shift → same key: the crash-resume re-tick idempotency (INV-7)
        // the mutation would keep — pinned so the teeth are specific to the
        // per-shift half, not the determinism half.
        assert_eq!(
            auto_z_idempotency_key(fnum, "aaaaaaaa"),
            k1,
            "same shift must yield the same key (crash-mid-Z re-tick resolves the same row)"
        );
    }
}
