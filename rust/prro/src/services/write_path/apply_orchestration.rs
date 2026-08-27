//! CS-3 S7-1 — shared apply orchestration (Q3 / FIX-C).
//!
//! `apply_recorded_outcome` is the SINGLE apply entry-point called by BOTH
//! the live cutover (`stage_send::run_one_attempt`) and the boot pass
//! (`reservation_boot_pass::apply_one`).  It:
//!
//! 1. Loads the durable doc/shift context **outside** any write-tx (invariant #1).
//! 2. Derives closing cash **outside** the write-tx (RELOCATED from
//!    `stage_send.rs:1623-1700`).
//! 3. Opens ONE `BEGIN IMMEDIATE`: runs `apply_outcome` (repo), then fires the
//!    online shift edges 3/10 when the apply actually released an Accepted doc.
//!
//! **Error contract — FIX-C:** the return type is `anyhow::Result<ApplyResult>`.
//! `apply_outcome(tx).await.map_err(anyhow::Error::from)?` preserves the
//! `ApplyError` as the anyhow *source*, so the boot consumer's
//! `downcast_ref::<ApplyError>()` still classifies `HeldNotAutoRelease` and
//! `NodeStateMissing` correctly.  A `HeldNotAutoRelease` (the normal result for
//! `-12`/`-6`/SubmittedUnknown) propagates **as-is** — the caller treats it as
//! an expected hold.  No `ApplyOrchestrationError` type exists; it was
//! explicitly rejected (FIX-C).
//!
//! **ACTIVE** (CS-3 S7-1 cutover + B3 re-audit fix): wired by BOTH callers —
//! `stage_send::run_one_attempt` (the live send path) and
//! `reservation_boot_pass::apply_one` (crash-resume). The boot path previously
//! called `apply_outcome` directly and SKIPPED the shift edges (re-audit
//! external-F1 / internal-B3); routing it here closes that gap.

use sqlx::SqlitePool;

use crate::db::models::enums::{Severity, ShiftState};
use crate::db::models::ids::{DocumentId, ShiftId};
use crate::db::repositories::audit_log;
use crate::db::repositories::delivery_reservation::{self, ApplyResult, ReservationId};
use crate::db::tx::with_immediate;

// ─── Context loaded before the write-tx ──────────────────────────────────────

struct ApplyContext {
    fiscal_number: String,
    document_id: DocumentId,
    doc_type: String, // TEXT as stored: 'SELL'/'SHIFT_OPEN'/'Z_REPORT'/'SHIFT_CLOSE'/…
    shift_id: Option<ShiftId>,
    online: bool, // offline_fiscal_no IS NULL
}

/// Load the durable context this apply needs for the shift edge, OUTSIDE the
/// write-tx (read-only pool reads, invariant #1 OK).
///
/// Returns `None` if the reservation is NOT in `OUTCOME_OBSERVED + PENDING_APPLY`
/// state — the caller treats that as a benign skip (apply_outcome's own
/// idempotency / NotPendingApply handles it).
async fn load_apply_context(
    pool: &SqlitePool,
    reservation_id: ReservationId,
) -> anyhow::Result<Option<ApplyContext>> {
    // Read from delivery_reservation.
    let dr: Option<(String, String, Option<String>, Vec<u8>)> = sqlx::query_as(
        "SELECT state, fiscal_number, apply_state, document_id \
         FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&reservation_id[..])
    .fetch_optional(pool)
    .await?;

    let (state, fiscal_number, apply_state, doc_id_blob) = match dr {
        None => return Ok(None),
        Some(row) => row,
    };

    // Only proceed when the reservation is OUTCOME_OBSERVED + PENDING_APPLY.
    // Otherwise let apply_outcome handle idempotency.
    if !(state == "OUTCOME_OBSERVED" && apply_state.as_deref() == Some("PENDING_APPLY")) {
        return Ok(None);
    }

    let doc_id_arr: [u8; 16] = doc_id_blob
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("apply_orchestration: document_id blob is not 16 bytes"))?;
    let document_id = DocumentId::from_bytes(doc_id_arr);

    // Read from fiscal_documents.
    let fd: Option<(String, Option<Vec<u8>>, Option<i64>)> = sqlx::query_as(
        "SELECT doc_type, shift_id, offline_fiscal_no \
         FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(&doc_id_blob)
    .fetch_optional(pool)
    .await?;

    let (doc_type, shift_id_blob, offline_fiscal_no) = match fd {
        None => {
            // Document gone — let apply_outcome fail gracefully.
            return Ok(None);
        }
        Some(row) => row,
    };

    // Construct ShiftId from the 16-byte blob, mirroring fiscal_documents.rs fetch_send_inputs_tx.
    let shift_id: Option<ShiftId> = shift_id_blob
        .map(|blob| {
            let arr: [u8; 16] = blob.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!("apply_orchestration: shift_id blob is not 16 bytes")
            })?;
            Ok::<ShiftId, anyhow::Error>(ShiftId::from_bytes(arr))
        })
        .transpose()?;

    Ok(Some(ApplyContext {
        fiscal_number,
        document_id,
        doc_type,
        shift_id,
        online: offline_fiscal_no.is_none(),
    }))
}

// ─── Closing-cash derivation (RELOCATED from stage_send.rs:1623-1700) ────────

/// Derive closing cash OUTSIDE the write-tx (invariant #1).  Only for online-
/// origin CLOSE docs (`Z_REPORT` / `SHIFT_CLOSE`) with a `shift_id`.
///
/// Mirrors the `closing_cash_kop_for_4b` block at `stage_send.rs:1623-1700`,
/// but since this is an async fn we can `.await` the audit directly — no
/// `tokio::task::block_in_place` / `block_on` needed.
async fn derive_closing_cash_for_apply(pool: &SqlitePool, ctx: &ApplyContext) -> Option<i64> {
    if !ctx.online {
        return None;
    }
    if ctx.doc_type != "Z_REPORT" && ctx.doc_type != "SHIFT_CLOSE" {
        return None;
    }
    let shift_id = ctx.shift_id?;

    // Re-read the opening anchor from shifts.cash_balance_kop
    // (written at shift-open acquire; this is the opening value
    // before the close overwrites it).
    let opening_kop: i64 =
        sqlx::query_scalar::<_, i64>("SELECT cash_balance_kop FROM shifts WHERE shift_id = ?")
            .bind(shift_id.as_bytes().to_vec())
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .unwrap_or(0);

    let doc_hex = format!("{:?}", ctx.document_id);
    match crate::services::cash_ledger::derive_closing_cash(
        pool,
        &ctx.fiscal_number,
        shift_id,
        opening_kop,
    )
    .await
    {
        Ok(v) => Some(v),
        Err(e) => {
            // HOLE-3 audit — observable fallback (mirrors stage_send.rs:1649-1688). The fallback is
            // safe (the close proceeds with the opening anchor) but must be recorded so a wrong
            // `cash_balance_kop` is traceable. Because this is an async fn (not the original sync
            // `unwrap_or_else` closure) we AWAIT the audit DIRECTLY — no `block_in_place` and no
            // detached `tokio::spawn` (which could drop the audit on shutdown, invariant #9). This
            // pool-based append runs OUTSIDE the write-tx (invariant #1 honoured).
            let payload = serde_json::json!({
                "doc": doc_hex,
                "fiscal_number": ctx.fiscal_number,
                "doc_type": ctx.doc_type,
                "error": e.to_string(),
                "fallback_kop": opening_kop,
            })
            .to_string();
            let _ = audit_log::append(
                pool,
                "fiscal_document",
                &doc_hex,
                "l0_derive_closing_cash_fallback",
                Severity::Warning,
                None,
                Some(&payload),
            )
            .await;
            Some(opening_kop)
        }
    }
}

// ─── Public entry-point ───────────────────────────────────────────────────────

/// Apply a recorded delivery-reservation outcome, firing the online shift edges
/// (3/10) when the apply released an Accepted doc.
///
/// **Error contract (FIX-C):** returns `anyhow::Result<ApplyResult>`.  An
/// `ApplyError::HeldNotAutoRelease` propagates as an anyhow error whose
/// `downcast_ref::<ApplyError>()` is `Some(HeldNotAutoRelease)` — this is the
/// EXPECTED result for `-12`/`-6`/SubmittedUnknown holds and the boot consumer
/// MUST treat it as non-fatal.
///
/// **LIVE since the S7-1 cutover (#336):** called by `stage_send.rs:1992` (live send) and
/// `reservation_boot_pass.rs:98` (boot) — the single apply funnel.
pub async fn apply_recorded_outcome(
    pool: &SqlitePool,
    reservation_id: ReservationId,
) -> anyhow::Result<ApplyResult> {
    // (0) Load context OUTSIDE the write-tx (invariant #1).
    let ctx_opt = load_apply_context(pool, reservation_id).await?;

    // (1) Derive closing cash OUTSIDE the write-tx (invariant #1).
    //     Only computed when context is present (PENDING_APPLY) and it is an online CLOSE.
    let closing_cash_kop: Option<i64> = if let Some(ref ctx) = ctx_opt {
        derive_closing_cash_for_apply(pool, ctx).await
    } else {
        None
    };

    // (2) ONE BEGIN IMMEDIATE: apply_outcome (repo) + shift edge.
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            // Apply the reservation (CAS, SFN stamp, seed advance, node effects).
            let res = delivery_reservation::apply_outcome(tx, reservation_id)
                .await
                .map_err(anyhow::Error::from)?;

            // Fire shift edge only when:
            //   - apply actually released something (`res.applied`)
            //   - the doc is online-origin (ctx.online — drain owns offline edges)
            //   - apply stamped a server_fiscal_no (== the Accepted discriminator)
            if let Some(ctx) = ctx_opt {
                if res.applied && ctx.online && res.server_fiscal_no.is_some() {
                    match ctx.doc_type.as_str() {
                        "SHIFT_OPEN" => {
                            super::stage_send::confirm_shift_edge(
                                tx,
                                &ctx.fiscal_number,
                                ctx.shift_id,
                                ctx.document_id,
                                ShiftState::Opening,
                                ShiftState::Opened,
                                "edge3_open",
                                None,
                            )
                            .await?;
                        }
                        "Z_REPORT" | "SHIFT_CLOSE" => {
                            super::stage_send::confirm_shift_edge(
                                tx,
                                &ctx.fiscal_number,
                                ctx.shift_id,
                                ctx.document_id,
                                ShiftState::Closing,
                                ShiftState::Closed,
                                "edge10_close",
                                closing_cash_kop,
                            )
                            .await?;
                        }
                        _ => {}
                    }
                }
            }

            Ok::<ApplyResult, anyhow::Error>(res)
        })
    })
    .await
}
