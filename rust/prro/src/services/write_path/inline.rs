//! RS-3 A2.1b-core — the inline `fiscalize` orchestrator (SELL/RETURN only).
//!
//! Lands DORMANT: the production binding (`UnimplementedWritePath` →
//! `InlineWritePath`) is flipped in A2.4, so nothing here is reachable from a
//! live request yet. See `docs/superpowers/plans/2026-06-09-rs3-a2-1b-core-impl.md`.
//!
//! ## Online `Sent → ACK` confirm (Q1 = option b, operator-signed)
//!
//! `stage_send::run` reaches `Sent { server_fiscal_no, attempt_no }` but
//! discards the DPS send-response `data_sign`; `kvt2_advance::advance_to_ack`
//! REQUIRES `kvt1_raw_bytes` (the lastChk `data_sign`).  So after `Sent` the
//! inline ladder performs an inline lastChk by `server_fiscal_no` via
//! [`online_confirm`] to recover the evidence, then drives `advance_to_ack`.
//!
//! [`online_confirm`] is a thin runtime-neutral wrapper that REUSES the drain's
//! pure classifier (`classify_check_result`) so the KVT1/hold/drift routing is
//! NOT duplicated — but it yields a 3-shape [`InlineConfirmOutcome`] (NO
//! `BootError`, NO drain source-routing leaking into the inline path).

#![allow(dead_code)] // consumed by the A2.4 binding; wired up incrementally.

use sqlx::SqlitePool;
use tokio::sync::OwnedMutexGuard;

use crate::db::models::enums::{DocState, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::audit_log;
use crate::db::repositories::fiscal_documents::{self, TerminalOutcome};
use crate::db::repositories::ingress_inbox::{self, InboxRow};
use crate::db::tx::with_immediate;
use crate::runtime::ingress::canonical_builder::build_canonical;
use crate::runtime::ingress::seam::{FiscalError, FiscalOutcome};
use crate::runtime::ingress::z_builder::ensure_full_z_surface_ready;
use crate::services::offline_sync::kvt2_confirm::{
    classify_check_result, Kvt2ConfirmOutcome, Kvt2ConfirmSource,
};
use crate::services::write_path::dispatch::{dispatch_post_sign, PostSignRoute};
use crate::services::write_path::inline_map::{
    classify_send_outcome, code_of, codes, map_build_reject, map_dispatcher_refusal,
    map_offline_refusal, map_rejection, map_send_error, map_sign_error, SendDisposition,
};
use crate::services::write_path::kvt2_advance::{advance_to_ack, ConfirmError};
use crate::services::write_path::stage_acquire;
use crate::services::write_path::stage_offline_ack::OfflineAckOutcome;
use crate::services::write_path::stage_send::{self, StageSendOutcome};
use crate::services::write_path::stage_sign::{self, SigningContext};
use crate::services::write_path::types::{hex_encode_lower, WorkerProcessResult};
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::CheckSignBlob;

/// Outcome of the inline online `Sent → ACK` confirm step ([`online_confirm`]).
/// Runtime-neutral 3-shape projection of the drain's `Kvt2ConfirmOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineConfirmOutcome {
    /// lastChk matched with non-empty `data_sign` — the KVT1 evidence the
    /// caller feeds into `advance_to_ack(kvt1_raw_bytes = .0)`.
    Acked(Vec<u8>),
    /// Transient / no-KVT1-evidence (DPS transport/server/auth/decode error,
    /// or matched id with EMPTY `data_sign`).  The caller leaves the doc at
    /// `Sent` → `Ok(FiscalOutcome{document_state: Sent})` → 202 IN_PROGRESS;
    /// drain/B1 completes the ACK later.  NEVER a terminal failure.
    Hold,
    /// Structural drift — `NotFound` / `ServerFiscalIdMismatch` from a
    /// Sent-fresh online send (the server does not recognise the id we just
    /// got from it).  The caller terminalises the inbox + returns
    /// `FiscalError::Internal` (500).
    Drift,
}

/// Inline online confirm: lastChk by `server_fiscal_no` → recover the KVT1
/// `data_sign` evidence.  Reuses [`classify_check_result`] (`SentFresh`
/// context) so the KVT1/hold/drift classification is single-sourced.
///
/// **I1**: the only IO is `by_server_fiscal_no` (a `last_chk` wire call), which
/// `assert_not_in_with_immediate`s — it MUST be called OUTSIDE every
/// `with_immediate` envelope.  No DB writes here.
pub async fn online_confirm(
    dps: &dyn DpsChannel,
    fn_sign: &CheckSignBlob,
    server_fiscal_no: &str,
) -> InlineConfirmOutcome {
    let result = dps.by_server_fiscal_no(fn_sign, server_fiscal_no).await;
    match classify_check_result(result, Kvt2ConfirmSource::SentFresh, None) {
        Kvt2ConfirmOutcome::Acked { kvt1_raw_bytes, .. } => {
            InlineConfirmOutcome::Acked(kvt1_raw_bytes)
        }
        Kvt2ConfirmOutcome::Hold { .. } => InlineConfirmOutcome::Hold,
        Kvt2ConfirmOutcome::StructuralDrift { .. } => InlineConfirmOutcome::Drift,
        Kvt2ConfirmOutcome::SentNotFoundDowngrade { .. } => {
            // Structurally unreachable: `classify_check_result` only emits
            // `SentNotFoundDowngrade` for the `SentReplay` source (the
            // safe-redrive path); we always pass `SentFresh`.  If this ever
            // fires, the classifier routing regressed.
            unreachable!(
                "online_confirm: SentNotFoundDowngrade is SentReplay-exclusive; \
                 SentFresh cannot produce it (classify_check_result routing breach)"
            )
        }
    }
}

/// Decode a `lower(hex(document_id))` (32 lowercase hex chars — the shape
/// `terminal_outcome_by_request_id` produces) back to a typed `DocumentId`.
/// `DocumentId` has no `FromStr`, so decode the 16 bytes explicitly.
fn hex32_to_document_id(s: &str) -> Option<DocumentId> {
    if s.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(DocumentId::from_bytes(bytes))
}

/// Pure ledger-state → `FiscalOutcome`/`FiscalError` projection (no IO).
///
/// Used by the replay-resolve arms (Noop / ResolveReplay / a post-`Sent`
/// advance failure): the durable ledger row is the truth, mapped per the seam
/// `document_state` contract —
///   - `Ack` / `OfflineLocalAck` (terminal SUCCESS) → `FiscalOutcome` (200);
///   - `Rejected` (terminal FAILURE) → `DpsRejected` (422);
///   - `RequiresManualReconciliation` → `Internal` (`SHIFT_MANUAL_RECON`, 500);
///   - `Cancelled` / a malformed `document_id` → `Internal`
///     (`REPLAY_LEDGER_DRIFT`, 500);
///   - anything else (in-flight: Prepared/Signed/…/Sent/Kvt1/Kvt2/
///     ErrorRetryable) → `FiscalOutcome` (202 IN_PROGRESS).
fn terminal_to_outcome(
    o: TerminalOutcome,
    request_id: [u8; 16],
) -> Result<FiscalOutcome, FiscalError> {
    let document_id = hex32_to_document_id(&o.document_id).ok_or(FiscalError::Internal {
        request_id,
        code: codes::REPLAY_LEDGER_DRIFT,
    })?;
    match o.state {
        DocState::Ack | DocState::OfflineLocalAck => Ok(FiscalOutcome {
            document_id,
            fiscal_id: o.server_fiscal_no,
            fiscal_ts: o.first_kvt1_at,
            document_state: o.state,
            report_xml: None,
        }),
        DocState::Rejected => Err(FiscalError::DpsRejected { request_id }),
        DocState::RequiresManualReconciliation => Err(FiscalError::Internal {
            request_id,
            code: codes::SHIFT_MANUAL_RECON,
        }),
        DocState::Cancelled => Err(FiscalError::Internal {
            request_id,
            code: codes::REPLAY_LEDGER_DRIFT,
        }),
        // In-flight states → 202 IN_PROGRESS (deterministic replay/poll).
        _ => Ok(FiscalOutcome {
            document_id,
            fiscal_id: o.server_fiscal_no,
            fiscal_ts: o.first_kvt1_at,
            document_state: o.state,
            report_xml: None,
        }),
    }
}

/// Resolve a no-wire / replay / post-`Sent` outcome against the durable
/// ledger (read-only; NOT inside a `with_immediate`).  An absent ledger doc,
/// a malformed id, or a DB error is a structural breach → `Internal`/500
/// (decision e: NEVER blind-terminalise — the ledger is the truth).
async fn resolve_against_ledger(
    pool: &SqlitePool,
    request_id: [u8; 16],
) -> Result<FiscalOutcome, FiscalError> {
    match fiscal_documents::terminal_outcome_by_request_id(pool, &request_id).await {
        Ok(Some(o)) => terminal_to_outcome(o, request_id),
        Ok(None) | Err(_) => Err(FiscalError::Internal {
            request_id,
            code: codes::REPLAY_LEDGER_DRIFT,
        }),
    }
}

/// Outcome of the pre-acquire terminalise (the Z/SHIFT_OPEN / BuildReject
/// arms, which run BEFORE `stage_acquire` while the inbox is still `NEW`).
enum PreAcquireTerminalise {
    /// The inbox was `NEW`: leased + REJECTED + audited ATOMICALLY (one tx).
    /// Caller returns the fail-closed `FiscalError`.
    Terminalised,
    /// The inbox was NOT `NEW` (already PROCESSING/DONE/REJECTED — a race or
    /// replay): nothing was mutated.  Caller MUST resolve against the ledger.
    NotNew,
}

/// Terminalise the inbox for a real-failure arm whose row is already
/// `PROCESSING` (stage_acquire leased it): CAS `PROCESSING → REJECTED` +
/// audit, in ONE `with_immediate` (no foreign IO → invariant #1).  REJECTED
/// is the terminal status for a refusal (the persistence pin forbids a
/// `fiscal_documents` ledger row for one; replay reads REJECTED as failure).
/// A `!marked` (row not PROCESSING) or DB fault is itself a structural breach
/// → `Internal`/500 (NEVER swallowed — the reaper re-drives a stuck row).
async fn terminalise_inbox(
    pool: &SqlitePool,
    row: &InboxRow,
    event_type: &'static str,
    code: &'static str,
) -> Result<(), FiscalError> {
    let request_id = row.request_id;
    let payload = serde_json::json!({
        "request_id": hex_encode_lower(&request_id),
        "fiscal_number": row.fiscal_number,
        "operation_type": row.operation_type,
        "code": code,
    })
    .to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let id_hex = hex_encode_lower(&request_id);
            let marked = ingress_inbox::mark_rejected_if_processing_tx(tx, &request_id).await?;
            if !marked {
                return Err(anyhow::anyhow!(
                    "terminalise_inbox: inbox row {id_hex} was not PROCESSING — cannot \
                     terminalise a non-leased / already-terminal row"
                ));
            }
            audit_log::append_tx(
                tx,
                "ingress_inbox",
                &id_hex,
                event_type,
                Severity::Warning,
                None,
                Some(&payload),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await
    .map_err(|_| FiscalError::Internal {
        request_id,
        code: codes::REPLAY_LEDGER_DRIFT,
    })?;
    Ok(())
}

/// Terminalise the inbox for a pre-acquire fail-closed arm (Z / SHIFT_OPEN /
/// BuildReject), where the row is still `NEW`.  ONE `with_immediate`: on
/// `acquire_lease` (NEW→PROCESSING) returning `Some`, run
/// `mark_rejected_if_processing_tx` then the audit append IN THE SAME tx
/// (atomic — a crash between the lease and the reject cannot leave an eternal
/// PROCESSING, invariant #4); on `None` (the row was NOT NEW — race/replay) no
/// mutation happens and we signal [`PreAcquireTerminalise::NotNew`] so the
/// caller resolves against the ledger.
async fn terminalise_inbox_pre_acquire(
    pool: &SqlitePool,
    row: &InboxRow,
    event_type: &'static str,
    code: &'static str,
) -> Result<PreAcquireTerminalise, FiscalError> {
    let request_id = row.request_id;
    let payload = serde_json::json!({
        "request_id": hex_encode_lower(&request_id),
        "fiscal_number": row.fiscal_number,
        "operation_type": row.operation_type,
        "code": code,
    })
    .to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let id_hex = hex_encode_lower(&request_id);
            match ingress_inbox::acquire_lease(tx, &request_id).await? {
                Some(_leased) => {
                    let marked =
                        ingress_inbox::mark_rejected_if_processing_tx(tx, &request_id).await?;
                    if !marked {
                        return Err(anyhow::anyhow!(
                            "terminalise_inbox_pre_acquire: just-leased inbox row {id_hex} \
                             was not PROCESSING (single-writer invariant breach)"
                        ));
                    }
                    audit_log::append_tx(
                        tx,
                        "ingress_inbox",
                        &id_hex,
                        event_type,
                        Severity::Warning,
                        None,
                        Some(&payload),
                    )
                    .await?;
                    Ok::<PreAcquireTerminalise, anyhow::Error>(PreAcquireTerminalise::Terminalised)
                }
                // Row was not NEW — a race/replay; do NOT terminalise.
                None => Ok(PreAcquireTerminalise::NotNew),
            }
        })
    })
    .await
    .map_err(|_| FiscalError::Internal {
        request_id,
        code: codes::REPLAY_LEDGER_DRIFT,
    })
}

/// Inline `fiscalize` orchestrator (A2.1b-core, SELL/RETURN only) — chains
/// `build_canonical → stage_acquire → stage_sign → dispatch → (Online) send →
/// online_confirm → advance_to_ack → finalize` into a `FiscalOutcome`.
///
/// **DORMANT**: no production caller yet (the binding flip is A2.4). `fn_gate`
/// is the A4 per-FN gate proof — the A2.4 binding MUST hold
/// `App::acquire_fn_gate(&row.fiscal_number)` across this call (invariant #2 at
/// the runtime level; the DB lease CAS in `stage_acquire` is the durable
/// backstop). `run` opens NO `with_immediate` itself; every envelope is owned
/// by a reused stage, and the only IO (crypto sign, DPS send, inline lastChk)
/// sits strictly BETWEEN envelopes (invariant #1).
///
/// Non-happy arms are wired incrementally (see the TDD increments in the impl
/// spec); each lands with its own pinning test.
#[allow(clippy::too_many_arguments)] // individual deps keep `run` unit-testable
                                     // without `&App` (the binding bundles them).
pub async fn run(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_sign: &CheckSignBlob,
    _fn_gate: &OwnedMutexGuard<()>,
    row: &InboxRow,
) -> Result<FiscalOutcome, FiscalError> {
    let request_id = row.request_id;

    // A2.1b-core is SELL/RETURN only. Z-class (Z_REPORT/SHIFT_CLOSE) and
    // SHIFT_OPEN are fail-closed BEFORE build/acquire — the inbox is still
    // NEW, so the pre-acquire terminalise leases+REJECTs atomically (no
    // fiscal_documents minted); on a race (row not NEW) it resolves against
    // the ledger.
    match row.operation_type.as_str() {
        "Z_REPORT" | "SHIFT_CLOSE" => {
            // Bound to the real live-Z surface gate (not theatrical): it is
            // Err today → 501 ZSurfaceNotReady. A2.1b-core (SELL/RETURN-only)
            // does NOT handle live-Z even if the gate flips — the debug_assert
            // makes a future gate-flip break tests (a deliberate revisit),
            // never a silent fall-through to build_canonical (a later A2.4-Z
            // piece owns the live-Z path).
            let surface = ensure_full_z_surface_ready();
            debug_assert!(
                surface.is_err(),
                "A2.1b-core predates the live-Z surface; the gate must be Err \
                 (if it flipped, live-Z is a LATER piece — revisit this branch)"
            );
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_SURFACE_NOT_READY",
                // `ZSurfaceNotReady` is a standalone FiscalError variant (not
                // code-bearing); the audit-payload code is the established
                // wire string (handler maps it → 501).
                "Z_SURFACE_NOT_READY",
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => {
                    Err(FiscalError::ZSurfaceNotReady { request_id })
                }
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
        "SHIFT_OPEN" => {
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_SHIFT_OPEN_NOT_SUPPORTED",
                codes::SHIFT_OPEN_NOT_SUPPORTED,
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(FiscalError::ShiftGuardRefused {
                    request_id,
                    code: codes::SHIFT_OPEN_NOT_SUPPORTED,
                }),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
        _ => {}
    }

    let command = match build_canonical(row) {
        Ok(c) => c,
        Err(reject) => {
            // Pre-acquire (inbox still NEW): atomic lease+REJECT; on a race
            // (row not NEW) resolve against the ledger.
            let fe = map_build_reject(&reject, request_id);
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_BUILD_REJECT",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
    };
    // build_canonical validated driver_id is present + well-formed.
    let driver_id = row
        .driver_id
        .as_deref()
        .expect("build_canonical guarantees driver_id present");

    let acq = match stage_acquire::run(pool, pool_secure, driver_id, request_id, command).await {
        Ok(r) => r,
        Err(_e) => {
            // stage_acquire's lease+PREPARED is one tx that rolls back on
            // error → the inbox is still NEW → pre-acquire terminalise (atomic).
            let fe = FiscalError::Internal {
                request_id,
                code: codes::ACQUIRE_INTERNAL,
            };
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_ACQUIRE_ERROR",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
    };
    let ctx = match acq {
        WorkerProcessResult::Proceed(ctx) | WorkerProcessResult::Resumed(ctx) => ctx,
        WorkerProcessResult::Noop => {
            // decision e: an inline Noop is UNEXPECTED under A4 (the handler
            // hands a freshly-Created NEW row) → audit Critical for
            // observability; NEVER blind-terminalise — resolve the truth from
            // the ledger (terminal/in-flight → return it; empty → Internal/500).
            let id_hex = hex_encode_lower(&request_id);
            let _ = audit_log::append(
                pool,
                "ingress_inbox",
                &id_hex,
                "INLINE_NOOP_UNEXPECTED",
                Severity::Critical,
                None,
                None,
            )
            .await;
            return resolve_against_ledger(pool, request_id).await;
        }
        WorkerProcessResult::Rejected { reason } => {
            // stage_acquire ALREADY terminalised the inbox (REJECTED + audit);
            // map ONLY — do NOT re-terminalise (CORRECTION 5).
            return Err(map_rejection(&reason, request_id));
        }
    };
    let doc_id = ctx.document.document_id;
    let fiscal_number = ctx.document.fiscal_number.clone();

    match stage_sign::run(pool, sign_ctx, ctx).await {
        Ok(_signing) => {}
        Err(e) => {
            let fe = map_sign_error(&e, request_id);
            terminalise_inbox(pool, row, "INLINE_SIGN_FAIL", code_of(&fe)).await?;
            return Err(fe);
        }
    }

    let route = match dispatch_post_sign(pool, doc_id, &fiscal_number).await {
        Ok(r) => r,
        Err(_e) => {
            let fe = FiscalError::Internal {
                request_id,
                code: codes::DISPATCH_INTERNAL,
            };
            terminalise_inbox(pool, row, "INLINE_DISPATCH_ERROR", code_of(&fe)).await?;
            return Err(fe);
        }
    };
    match route {
        PostSignRoute::Refused(reason) => {
            let fe = map_dispatcher_refusal(&reason, request_id);
            terminalise_inbox(pool, row, "INLINE_DISPATCH_REFUSED", code_of(&fe)).await?;
            Err(fe)
        }
        PostSignRoute::Offline { outcome, .. } => {
            match outcome {
                // A transient/ambiguous DPS auto-offline is a SUCCESS, not an Err:
                // the doc is durably at OFFLINE_LOCAL_ACK (200). No DPS id yet.
                OfflineAckOutcome::Applied { document_id, .. } => Ok(FiscalOutcome {
                    document_id,
                    fiscal_id: None,
                    fiscal_ts: None,
                    document_state: DocState::OfflineLocalAck,
                    report_xml: None,
                }),
                OfflineAckOutcome::Refused(reason) => {
                    // CORRECTION 4: granular — node-mode → OfflineRefused/503
                    // with precise code; race/structural → Internal/500 (NOT a
                    // blanket 503).
                    let fe = map_offline_refusal(&reason, request_id);
                    terminalise_inbox(pool, row, "INLINE_OFFLINE_REFUSED", code_of(&fe)).await?;
                    Err(fe)
                }
            }
        }
        PostSignRoute::Online { .. } => {
            let send = match stage_send::run(pool, dps, doc_id, Some(sign_ctx)).await {
                Ok(o) => o,
                Err(e) => {
                    let fe = map_send_error(&e, request_id);
                    terminalise_inbox(pool, row, "INLINE_SEND_ERROR", code_of(&fe)).await?;
                    return Err(fe);
                }
            };
            // GOTCHA (arch-planner): `classify_send_outcome` drops `attempt_no`
            // on the Proceed arm — capture it from `Sent` BEFORE classifying,
            // since `advance_to_ack`'s audit payload needs it.
            let sent_attempt_no = match &send {
                StageSendOutcome::Sent { attempt_no, .. } => Some(*attempt_no),
                _ => None,
            };
            match classify_send_outcome(send, request_id) {
                SendDisposition::Reject(fe) => {
                    // `fe` is ALREADY mapped by classify_send_outcome (terminal
                    // DPS reject / signer mismatch / structural) — terminalise
                    // with its own code, then return it; no re-mapping.
                    terminalise_inbox(pool, row, "INLINE_SEND_REJECT", code_of(&fe)).await?;
                    Err(fe)
                }
                SendDisposition::InProgress => Ok(FiscalOutcome {
                    // Transient wire failure: the doc is persisted at
                    // `ErrorRetryable`; the ledger re-drives it via drain/B1.
                    // 202 IN_PROGRESS — NOT a terminal failure. No DPS id yet.
                    document_id: doc_id,
                    fiscal_id: None,
                    fiscal_ts: None,
                    document_state: DocState::ErrorRetryable,
                    report_xml: None,
                }),
                SendDisposition::ResolveReplay { .. } => {
                    // No-wire idempotent re-entry / race (stage_send StateConflict
                    // / DocumentMissing): resolve the durable truth from the
                    // ledger, never a phantom terminal 500.
                    resolve_against_ledger(pool, request_id).await
                }
                SendDisposition::Proceed { server_fiscal_no } => {
                    let attempt_no = sent_attempt_no
                        .expect("Sent disposition implies a captured Sent attempt_no");
                    // Online Sent→ACK confirm (Q1 = b): inline lastChk recovers
                    // the KVT1 evidence stage_send discarded, then advance_to_ack.
                    match online_confirm(dps, fn_sign, &server_fiscal_no).await {
                        InlineConfirmOutcome::Acked(kvt1_raw_bytes) => {
                            match advance_to_ack(
                                pool,
                                doc_id,
                                kvt1_raw_bytes,
                                &server_fiscal_no,
                                DocState::Sent,
                                Some(i64::from(attempt_no)),
                            )
                            .await
                            {
                                Ok(()) => Ok(FiscalOutcome {
                                    document_id: doc_id,
                                    fiscal_id: Some(server_fiscal_no),
                                    fiscal_ts: None,
                                    document_state: DocState::Ack,
                                    report_xml: None,
                                }),
                                Err(ConfirmError::StructuralDrift { .. }) => {
                                    // The doc stays durably at `Sent` (the
                                    // advance envelopes rolled back). Terminalise
                                    // the inbox REJECTED + audit — an INTENTIONAL,
                                    // AUDITED divergence (Sent doc + REJECTED
                                    // inbox = the breach surface for B1/recon,
                                    // invariant #8, not silent) — and 500.
                                    let fe = FiscalError::Internal {
                                        request_id,
                                        code: codes::REPLAY_LEDGER_DRIFT,
                                    };
                                    terminalise_inbox(
                                        pool,
                                        row,
                                        "INLINE_ADVANCE_DRIFT",
                                        code_of(&fe),
                                    )
                                    .await?;
                                    Err(fe)
                                }
                                Err(ConfirmError::Database { .. })
                                | Err(ConfirmError::Infrastructure { .. }) => {
                                    // The advance envelope rolled back: the doc is
                                    // still durably `Sent`.  Resolve against the
                                    // ledger (→ 202), NEVER a blind terminal 500.
                                    resolve_against_ledger(pool, request_id).await
                                }
                            }
                        }
                        InlineConfirmOutcome::Hold => Ok(FiscalOutcome {
                            // Inline lastChk had no KVT1 evidence (transient /
                            // empty data_sign): leave the doc at `Sent` → 202
                            // IN_PROGRESS; drain/B1 completes the KVT2 confirm.
                            // NOT a terminal failure, NOT a fake ACK. The DPS
                            // id IS known (stamped by stage_send) — informational.
                            document_id: doc_id,
                            fiscal_id: Some(server_fiscal_no),
                            fiscal_ts: None,
                            document_state: DocState::Sent,
                            report_xml: None,
                        }),
                        InlineConfirmOutcome::Drift => {
                            // Server does not recognise the id it just gave us;
                            // the doc stays `Sent`. Terminalise inbox + 500
                            // (audited divergence — same as the advance-drift arm).
                            let fe = FiscalError::Internal {
                                request_id,
                                code: codes::REPLAY_LEDGER_DRIFT,
                            };
                            terminalise_inbox(pool, row, "INLINE_CONFIRM_DRIFT", code_of(&fe))
                                .await?;
                            Err(fe)
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transports::dps::dto::{CheckAck, CheckEnvelope, RroInfo, StatusSnapshot};
    use crate::transports::dps::error::DpsError;
    use async_trait::async_trait;
    use std::sync::Mutex;

    const FN_SIGN: &[u8] = &[0xAB, 0xCD];
    const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";

    /// Minimal `DpsChannel` stub: a single scripted `last_chk` reply (consumed
    /// once).  `online_confirm` → `by_server_fiscal_no` (default trait method)
    /// → `last_chk`.  Other RPCs are unreachable for this seam.
    struct StubLastChk(Mutex<Option<Result<CheckAck, DpsError>>>);

    impl StubLastChk {
        fn new(reply: Result<CheckAck, DpsError>) -> Self {
            Self(Mutex::new(Some(reply)))
        }
    }

    #[async_trait]
    impl DpsChannel for StubLastChk {
        async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
            unreachable!("online_confirm never sends");
        }
        async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
            self.0
                .lock()
                .unwrap()
                .take()
                .expect("StubLastChk: last_chk called more than once")
        }
        async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
            unreachable!("stub: ping not exercised");
        }
        async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
            unreachable!("stub: status_rro not exercised");
        }
        async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
            unreachable!("stub: info_rro not exercised");
        }
    }

    fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
        CheckAck {
            id: id.to_string(),
            id_sign: vec![],
            data_sign,
        }
    }

    fn fn_sign() -> CheckSignBlob {
        CheckSignBlob(FN_SIGN.to_vec())
    }

    /// Match + non-empty data_sign → Acked carrying the exact evidence bytes
    /// (which the caller feeds into advance_to_ack as kvt1_raw_bytes).
    #[tokio::test]
    async fn acked_returns_data_sign_evidence() {
        let dps = StubLastChk::new(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(
            outcome,
            InlineConfirmOutcome::Acked(vec![0xDE, 0xAD, 0xBE, 0xEF])
        );
    }

    /// Match but EMPTY data_sign → Hold (no KVT1 evidence → 202, NOT a fake ACK).
    #[tokio::test]
    async fn empty_data_sign_returns_hold() {
        let dps = StubLastChk::new(Ok(ack(SERVER_FISCAL_NO, vec![])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Hold);
    }

    /// Transport error → Hold (transient → 202, NOT terminal 500).
    #[tokio::test]
    async fn transport_error_returns_hold() {
        let dps = StubLastChk::new(Err(DpsError::Transport("conn reset".into())));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Hold);
    }

    /// Empty id → NotFound → Drift (server does not recognise the id it just
    /// returned to us → structural breach → caller maps to Internal/500).
    #[tokio::test]
    async fn not_found_returns_drift() {
        let dps = StubLastChk::new(Ok(ack("", vec![0x01])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Drift);
    }

    /// Mismatched id → ServerFiscalIdMismatch → Drift.
    #[tokio::test]
    async fn id_mismatch_returns_drift() {
        let dps = StubLastChk::new(Ok(ack("DPS-FN-OTHER", vec![0x01])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Drift);
    }

    // ─── terminal_to_outcome (replay-resolve ledger projection) ─────────

    use crate::db::models::enums::DocType;

    const RID: [u8; 16] = [0x11; 16];

    fn term(state: DocState, server_fiscal_no: Option<&str>) -> TerminalOutcome {
        TerminalOutcome {
            document_id: hex_encode_lower(DocumentId::new().as_bytes()),
            state,
            doc_type: DocType::Sell,
            server_fiscal_no: server_fiscal_no.map(str::to_string),
            first_kvt1_at: None,
            total_sum_kop: Some(15000),
        }
    }

    /// Terminal-accepted Ack → success outcome carrying the DPS id (→ 200).
    #[test]
    fn terminal_ack_is_success_with_fiscal_id() {
        let r = terminal_to_outcome(term(DocState::Ack, Some("DPS-FN-9")), RID)
            .expect("Ack resolves to a success outcome");
        assert_eq!(r.document_state, DocState::Ack);
        assert_eq!(r.fiscal_id.as_deref(), Some("DPS-FN-9"));
    }

    /// OfflineLocalAck → success, no DPS id (→ 200).
    #[test]
    fn terminal_offline_local_ack_is_success_no_id() {
        let r = terminal_to_outcome(term(DocState::OfflineLocalAck, None), RID)
            .expect("offline-local-ack resolves to a success outcome");
        assert_eq!(r.document_state, DocState::OfflineLocalAck);
        assert_eq!(r.fiscal_id, None);
    }

    /// In-flight Sent → 202 IN_PROGRESS (a success-shape, not an Err).
    #[test]
    fn terminal_sent_is_in_flight_in_progress() {
        let r = terminal_to_outcome(term(DocState::Sent, Some("DPS-FN-9")), RID)
            .expect("Sent resolves to an in-flight outcome");
        assert_eq!(r.document_state, DocState::Sent);
    }

    /// Terminal-failed Rejected → DpsRejected (422).
    #[test]
    fn terminal_rejected_is_dps_rejected() {
        let e = terminal_to_outcome(term(DocState::Rejected, None), RID).unwrap_err();
        assert!(matches!(e, FiscalError::DpsRejected { .. }));
    }

    /// RequiresManualReconciliation → Internal(SHIFT_MANUAL_RECON) (500).
    #[test]
    fn terminal_manual_recon_is_internal_manual_recon() {
        let e = terminal_to_outcome(term(DocState::RequiresManualReconciliation, None), RID)
            .unwrap_err();
        assert!(
            matches!(e, FiscalError::Internal { code, .. } if code == codes::SHIFT_MANUAL_RECON)
        );
    }

    /// A malformed `document_id` is ledger drift → Internal(REPLAY_LEDGER_DRIFT).
    #[test]
    fn terminal_malformed_document_id_is_internal_drift() {
        let mut o = term(DocState::Ack, Some("DPS-FN-9"));
        o.document_id = "not-a-valid-document-id".to_string();
        let e = terminal_to_outcome(o, RID).unwrap_err();
        assert!(
            matches!(e, FiscalError::Internal { code, .. } if code == codes::REPLAY_LEDGER_DRIFT)
        );
    }
}
