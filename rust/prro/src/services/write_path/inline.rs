//! RS-3 A2.1b-core — the inline `fiscalize` orchestrator (SELL/RETURN only).
//!
//! Lands DORMANT: the production binding (`UnimplementedWritePath` →
//! `InlineWritePath`) is flipped in A2.4, so nothing here is reachable from a
//! live request yet. See `docs/superpowers/plans/2026-06-09-rs3-a2-1b-core-impl.md`.
//!
//! **A2.4 ACTIVATION PREREQUISITE (AUD-L2-1a)**: before that binding flip,
//! resolve the online-lane chain SEED-FORK — the seed advances only at
//! `stage_finalize` (ACK), so two online SELLs resting at `SENT` fork the chain
//! (doc2 signs against the stale pre-doc1 seed).  Gate: un-#[ignore]
//! `tests/kill_point_matrix.rs::m1_02_online_seed_fork_a24_prerequisite` (a RED
//! pin today).  Barrier mirrored at `runtime::supervisor` (the binding site).
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

use sqlx::SqlitePool;
use tokio::sync::OwnedMutexGuard;

use crate::db::models::enums::{DocState, DocType, NodeMode, Severity, ShiftState};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::audit_log;
use crate::db::repositories::fiscal_documents::{self, TerminalOutcome};
use crate::db::repositories::ingress_inbox::{self, InboxRow};
use crate::db::repositories::node_state;
use crate::db::repositories::offline_sessions;
use crate::db::tx::with_immediate;
use crate::db::types::{DbDocState, DbDocType};
use crate::runtime::ingress::canonical_builder::build_canonical;
use crate::runtime::ingress::convert::aggregate_z_payload_for_shift;
use crate::runtime::ingress::seam::{FiscalError, FiscalOutcome};
use crate::runtime::ingress::z_builder::{
    build_z_canonical, ensure_full_z_surface_ready, quiesce_shift_before_z, QuiescenceOutcome,
};
use crate::services::offline_sync::kvt2_confirm::{
    classify_check_result, Kvt2ConfirmOutcome, Kvt2ConfirmSource,
};
use crate::services::time_budget::{self, TimeBudgetGate};
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
use crate::services::write_path::types::{
    hex_encode_lower, CanonicalFiscalCommand, WorkerProcessResult,
};
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
    // `superseded: false` — SEAM-B-3's superseded exception is
    // SentReplay-exclusive; the inline online-confirm path is always
    // SentFresh (the doc was just sent this call), so it can never be
    // superseded by a newer submitted doc.
    match classify_check_result(result, Kvt2ConfirmSource::SentFresh, None, false) {
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
        Kvt2ConfirmOutcome::SupersededHold { .. } => {
            // Structurally unreachable: SEAM-B-3's `SupersededHold` only
            // fires when the caller passes `superseded == true`, which is
            // SentReplay-exclusive; this path always passes `false`
            // (SentFresh).  If this fires, classifier routing regressed.
            unreachable!(
                "online_confirm: SupersededHold is SentReplay-exclusive; \
                 SentFresh + superseded=false cannot produce it (routing breach)"
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
        // Non-issued terminal: an operation refused before issuance.  A re-submit
        // resolving against this row gets a typed refusal (the original reason is
        // not stored on the row) — never a false 202-in-flight or 200-success.
        DocState::Aborted => Err(FiscalError::OfflineRefused {
            request_id,
            code: codes::DOC_ABORTED,
        }),
        // In-flight states → 202 IN_PROGRESS (deterministic replay/poll).
        // EXPLICITLY enumerated (review OCF-4): a future DocState variant
        // must be deliberately classified here — a new TERMINAL state
        // silently mapping to 202-in-flight would be a fiscal-truth bug.
        DocState::Prepared
        | DocState::Signed
        | DocState::Encrypted
        | DocState::Sending
        | DocState::Sent
        | DocState::Kvt1
        | DocState::Kvt2
        | DocState::ErrorRetryable => Ok(FiscalOutcome {
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

/// Is a ledger-resolve `Err` a TERMINAL VERDICT (the ledger POSITIVELY says
/// the receipt terminally failed / needs manual recon) — vs an UNKNOWN-truth
/// drift (no doc row / DB read error / malformed id / Cancelled-lumped)?
///
/// Only a terminal verdict may terminalise our inbox lease: marking REJECTED
/// on unknown truth would let a doc that is durably `Sent` (and later
/// converges to ACK via the boot SENT-probe) replay as `Failed` — the
/// REJECTED inbox short-circuits `replay::resolve_replay` BEFORE the ledger
/// join (replay.rs), i.e. the gateway would lie about a fiscalized receipt.
/// On unknown truth the lease stays PROCESSING: replay answers from the
/// ledger (202 / Failed honestly), and B1/boot recovery owns convergence.
fn is_terminal_ledger_verdict(fe: &FiscalError) -> bool {
    match fe {
        FiscalError::DpsRejected { .. } => true,
        FiscalError::Internal { code, .. } => *code == codes::SHIFT_MANUAL_RECON,
        _ => false,
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
/// SW-4 (A.3 PR-B step 8): route an inline-confirm `ChainSeedMismatch` to the
/// shared FN → Manual-reconciliation escalation seam
/// ([`escalate_fn_to_manual_recon`]), the SAME seam the online-convergence tick
/// and boot-KVT2 arms use.  Returns the `FiscalError` the confirm arm surfaces:
/// `SHIFT_MANUAL_RECON` (500) once the FN has a durable operator surface (RMR
/// shift OR the no-shift audit), or `REPLAY_LEDGER_DRIFT` (500) if the escalate
/// envelope hits a structural desync (the drift detector is preserved).  Does
/// NOT terminalise the inbox — the doc is issued-unconfirmed (`Sent`/`Kvt2`),
/// not rejected; the escalation is the surface.  Extracted so the routing is
/// unit-pinnable via synthetic injection (the `ConfirmError::ChainSeedMismatch`
/// producer is DORMANT post-PR-A — the finalize producer was removed — so this
/// arm cannot be reached through the full inline path until PR-C re-wires it).
pub(crate) async fn escalate_inline_chain_seed_mismatch(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    request_id: [u8; 16],
) -> FiscalError {
    let drift = FiscalError::Internal {
        request_id,
        code: codes::REPLAY_LEDGER_DRIFT,
    };
    let ns = match node_state::get(pool, fiscal_number).await {
        Ok(Some(ns)) => ns,
        _ => return drift,
    };
    match crate::services::offline_sync::backlog_drain::escalate_fn_to_manual_recon(
        pool,
        fiscal_number,
        &ns,
        doc_id,
        "chain_seed_mismatch",
        0,
        "INLINE_ADVANCE_CHAIN_SEED_MISMATCH_ESCALATE_MANUAL",
        "INLINE_ADVANCE_CHAIN_SEED_MISMATCH_NO_SHIFT",
    )
    .await
    {
        // Escalated (→ RMR) or NoEscalatableShift (no-shift audit is the surface):
        // both leave a durable operator surface → the manual-recon 500.
        Ok(_) => FiscalError::Internal {
            request_id,
            code: codes::SHIFT_MANUAL_RECON,
        },
        // Structural desync inside the escalate envelope (NULL/Forbidden CAS that
        // is NOT the idempotent already-RMR no-op) → preserve the drift detector.
        Err(_) => drift,
    }
}

/// The shift-class doc types whose DPS reject escalates the FN shift to RMR
/// (PR-Z2 step 3, edges 4/12), as opposed to a SELL/RETURN per-doc terminal.
fn is_shift_class(doc_type: DocType) -> bool {
    matches!(
        doc_type,
        DocType::ShiftOpen | DocType::ShiftClose | DocType::ZReport
    )
}

/// PR-Z2 step 3 (S5-A minimal-viable) — a `SHIFT_OPEN` / `Z_REPORT` /
/// `SHIFT_CLOSE` whose send DPS-REJECTED escalates the FN shift to
/// `RequiresManualReconciliation` (edge 4 `Opening→RMR` / edge 12
/// `Closing→RMR`) via the shared idempotent `escalate_fn_to_manual_recon` seam
/// and its §8.1 forensic audit.  The doc rests `Rejected` (D2, pre-SENT); this
/// drives ONLY the shift edge.  Mirrors `escalate_inline_chain_seed_mismatch`,
/// idempotent (already-RMR → clean no-op).  See the reject-arm doc-comment for
/// the DocumentReject / edge-11 residual.
async fn escalate_shift_class_reject(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    request_id: [u8; 16],
) -> FiscalError {
    let drift = FiscalError::Internal {
        request_id,
        code: codes::REPLAY_LEDGER_DRIFT,
    };
    let ns = match node_state::get(pool, fiscal_number).await {
        Ok(Some(ns)) => ns,
        // No node_state → no shift to escalate; the manual-recon surface is the
        // honest answer (an operator anomaly either way).
        _ => {
            return FiscalError::Internal {
                request_id,
                code: codes::SHIFT_MANUAL_RECON,
            }
        }
    };
    match crate::services::offline_sync::backlog_drain::escalate_fn_to_manual_recon(
        pool,
        fiscal_number,
        &ns,
        doc_id,
        "z_shift_dps_reject",
        0,
        "INLINE_Z_SHIFT_REJECT_ESCALATE_MANUAL",
        "INLINE_Z_SHIFT_REJECT_NO_SHIFT",
    )
    .await
    {
        // Escalated (→ RMR) or NoEscalatableShift (the no-shift audit is the
        // surface): both leave a durable operator surface → the manual-recon 500.
        Ok(_) => FiscalError::Internal {
            request_id,
            code: codes::SHIFT_MANUAL_RECON,
        },
        // Structural desync inside the escalate envelope (NOT the idempotent
        // already-RMR no-op) → preserve the drift detector.
        Err(_) => drift,
    }
}

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
    let aborted_shift_class = with_immediate(pool, move |tx| {
        Box::pin(async move {
            let id_hex = hex_encode_lower(&request_id);
            let marked = ingress_inbox::mark_rejected_if_processing_tx(tx, &request_id).await?;
            if !marked {
                return Err(anyhow::anyhow!(
                    "terminalise_inbox: inbox row {id_hex} was not PROCESSING — cannot \
                     terminalise a non-leased / already-terminal row"
                ));
            }
            // Ledger-only pin (post-sign refusal orphan fix): a refused-before-
            // issuance op must NOT leave a non-terminal doc orphaned.  In the SAME
            // envelope as the inbox CAS (a crash between would re-orphan), abort any
            // dangling {PREPARED, SIGNED} doc for this request_id → the non-issued
            // terminal `Aborted`.  Online in-flight states (SENDING/SENT/…) and
            // already-terminal rows are EXCLUDED by the WHERE, so the send-path
            // arms (INLINE_SEND_*) no-op here; this fires for the post-sign refusal
            // arms (dispatch-internal / dispatch-refused / offline-refused) and a
            // pre-sign sign-failure (PREPARED) alike — one place fixes the class.
            let dangling: Option<(Vec<u8>, DocState, DocType)> =
                sqlx::query_as::<_, (Vec<u8>, DbDocState, DbDocType)>(
                    "SELECT document_id, state, doc_type FROM fiscal_documents \
                     WHERE request_id = ? AND state IN ('PREPARED','SIGNED') LIMIT 1",
                )
                .bind(&request_id[..])
                .fetch_optional(&mut **tx)
                .await?
                .map(|(id, state, doc_type)| (id, state.0, doc_type.0));
            // A′.3 PR-O3 STOP-O3-1 fix (b): report the aborted doc back to the
            // caller when it is SHIFT-CLASS — the offline lifecycle edges
            // (2/9/7) commit the shift transition in stage_acquire's envelope,
            // so aborting the lifecycle doc orphans a pending-drain shift that
            // then needs the RMR escalation (spec §16.7 family 1).
            let mut aborted_shift_class: Option<DocumentId> = None;
            if let Some((doc_bytes, from_state, doc_type)) = dangling {
                let doc_id = <[u8; 16]>::try_from(doc_bytes.as_slice())
                    .map(DocumentId::from_bytes)
                    .map_err(|_| {
                        anyhow::anyhow!("terminalise_inbox: malformed document_id for {id_hex}")
                    })?;
                match fiscal_documents::transition_state(tx, doc_id, from_state, DocState::Aborted)
                    .await?
                {
                    fiscal_documents::TransitionOutcome::Applied => {}
                    other => {
                        return Err(anyhow::anyhow!(
                            "terminalise_inbox: {from_state:?}→Aborted CAS not Applied \
                             ({other:?}) for the doc of {id_hex}"
                        ))
                    }
                }
                // Distinct audit for the doc-abort (NOT the generic inbox event).
                audit_log::append_tx(
                    tx,
                    "fiscal_document",
                    &hex_encode_lower(doc_id.as_bytes()),
                    "INLINE_REFUSED_DOC_ABORTED",
                    Severity::Warning,
                    None,
                    Some(&payload),
                )
                .await?;
                if is_shift_class(doc_type) {
                    aborted_shift_class = Some(doc_id);
                }
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
            Ok::<Option<DocumentId>, anyhow::Error>(aborted_shift_class)
        })
    })
    .await
    .map_err(|err| {
        // The refusal could not be durably recorded (DB fault, or a !marked
        // row-state surprise).  Log the cause — the audit row rolled back
        // with the tx, so this trace is the only forensic record.
        tracing::error!(
            request_id = %hex_encode_lower(&request_id),
            event_type,
            error = %format!("{err:#}"),
            "terminalise_inbox failed — inbox row NOT terminalised"
        );
        FiscalError::Internal {
            request_id,
            code: codes::INBOX_TERMINALISE_FAILED,
        }
    })?;
    // STOP-O3-1 fix (b), runtime half — OUTSIDE the abort envelope (the
    // escalation seam owns its own CAS+mirror envelope, RS-3 C1); the
    // crash/error gap between the committed abort and this escalation is
    // closed by the boot half (sweep SW-2, the #196 re-run pattern).
    if let Some(doc_id) = aborted_shift_class {
        escalate_orphaned_pending_drain_shift(pool, &row.fiscal_number, doc_id, request_id).await;
    }
    Ok(())
}

/// STOP-O3-1 fix (b) — after a SHIFT-CLASS doc was aborted by
/// [`terminalise_inbox`], escalate the pending-drain shift it orphaned
/// (`OpenedLocalPendingDrain` / `ClosingLocalPendingDrain`, whose only exit —
/// edge 5/13 — needs the now-dead anchor doc) to
/// `RequiresManualReconciliation` via the shared idempotent seam (edges 6/14).
/// Any other shift state is left untouched (online lifecycle rejects own their
/// paths; a Closed/Opened shift is not orphaned by this abort).
///
/// BEST-EFFORT by design: the abort is already durably committed, and the
/// caller is an error-arm — a failure here must not mask the client's typed
/// refusal.  On an escalation error we emit the Critical audit and rely on the
/// boot half (sweep SW-2) to re-run the same compensation idempotently.
async fn escalate_orphaned_pending_drain_shift(
    pool: &SqlitePool,
    fiscal_number: &str,
    doc_id: DocumentId,
    request_id: [u8; 16],
) {
    let ns = match node_state::get(pool, fiscal_number).await {
        Ok(Some(ns)) => ns,
        Ok(None) => return,
        Err(e) => {
            tracing::error!(
                request_id = %hex_encode_lower(&request_id),
                error = %format!("{e:#}"),
                "fix (b): node_state read failed — boot SW-2 will re-run the escalation"
            );
            return;
        }
    };
    use crate::db::models::enums::ShiftState;
    if !matches!(
        ns.shift_state,
        ShiftState::OpenedLocalPendingDrain | ShiftState::ClosingLocalPendingDrain
    ) {
        return;
    }
    match crate::services::offline_sync::backlog_drain::escalate_fn_to_manual_recon(
        pool,
        fiscal_number,
        &ns,
        doc_id,
        "offline_lifecycle_doc_aborted",
        0,
        "INLINE_OFFLINE_LIFECYCLE_ABORT_ESCALATE_MANUAL",
        "INLINE_OFFLINE_LIFECYCLE_ABORT_NO_SHIFT",
    )
    .await
    {
        Ok(_outcome) => {}
        // Best-effort: the wedge stays observable (scan tripwire, fix (c)) and
        // the boot half re-runs the compensation; do NOT mask the client error.
        Err(e) => tracing::error!(
            request_id = %hex_encode_lower(&request_id),
            error = %format!("{e:?}"),
            "fix (b): orphaned pending-drain shift escalation failed — boot SW-2 re-runs it"
        ),
    }
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
    .map_err(|err| {
        // KNOWN letter-gap (review F1): on a DB fault here the row stays NEW
        // and we still return a FiscalError — the obligation is unsatisfiable
        // at fault time (a failing DB cannot record the terminalisation
        // either).  The key then resolves to 202 on replay until re-driven;
        // B1's reaper scope must include NEW rows with no fiscal_documents.
        // This trace is the only forensic record.
        tracing::error!(
            request_id = %hex_encode_lower(&request_id),
            event_type,
            error = %format!("{err:#}"),
            "terminalise_inbox_pre_acquire failed — inbox row left NEW"
        );
        FiscalError::Internal {
            request_id,
            code: codes::INBOX_TERMINALISE_FAILED,
        }
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
    budget_gate: TimeBudgetGate<'_>,
) -> Result<FiscalOutcome, FiscalError> {
    let request_id = row.request_id;

    // PR-Z2 (online-half): SELL/RETURN **and** SHIFT_OPEN route live through the
    // happy-path stages below (SHIFT_OPEN reuses the same acquire→sign→send→
    // confirm→advance chain — the stages are doc-type-aware: acquire fires
    // edge 1 Created→Opening, send confirms edge 3 Opening→Opened).  Z-class
    // (Z_REPORT/SHIFT_CLOSE) stays fail-closed BEFORE build/acquire until the
    // Z-dispatch arm lands (next PR-Z2 step) — the inbox is still NEW, so the
    // pre-acquire terminalise leases+REJECTs atomically (no fiscal_documents
    // minted); on a race (row not NEW) it resolves against the ledger.
    match row.operation_type.as_str() {
        "Z_REPORT" | "SHIFT_CLOSE" => {
            // PR-Z2 (online-half): route the live Z dispatcher IFF the release
            // flag is set.  While `FULL_Z_SURFACE_READY` is false the arm stays
            // fail-closed (501 ZSurfaceNotReady); `run_z_dispatch` is wired +
            // tested directly and goes live when the step-4 flip flips the const
            // (in lock-step with the coupling-pin + the tripwire).  The old
            // `debug_assert(surface.is_err())` is superseded — the routing IS the
            // deliberate handling now.
            if ensure_full_z_surface_ready().is_ok() {
                return run_z_dispatch(pool, pool_secure, dps, sign_ctx, fn_sign, row, budget_gate)
                    .await;
            }
            // Build the error FIRST so the audit code comes from code_of —
            // audit ↔ returned error agree by construction (review HTTP-1;
            // the literal lives in one place, fence-tested → 501).
            let fe = FiscalError::ZSurfaceNotReady { request_id };
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_SURFACE_NOT_READY",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
        // SHIFT_OPEN falls through to the live happy-path stages below.
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

    // B10 (W10a) — the lazy DocType=9 BEGIN is ensured inside `run_staged` (the
    // common first-offline-doc seam), so it fires for SHIFT_OPEN / SELL / RETURN
    // AND the Z path (`run_z_dispatch` → `run_staged`) — regardless of which doc
    // type is the session's first offline doc.  (Review Finding B + HOLE A.)
    run_staged(
        pool,
        pool_secure,
        dps,
        sign_ctx,
        fn_sign,
        row,
        command,
        budget_gate,
    )
    .await
}

/// The shared staged pipeline (acquire → sign → dispatch → send → confirm →
/// advance → finalize) for an ALREADY-BUILT `command`.  Both the
/// SELL/RETURN/SHIFT_OPEN path (via `build_canonical`) and the live Z path
/// (`run_z_dispatch` → `build_z_canonical`) feed it verbatim — the stages are
/// doc-type-aware (acquire fires the shift edge, sign allocates the Z number,
/// send confirms the SENT edge), so the tail is shared, not duplicated.
#[allow(clippy::too_many_arguments)]
async fn run_staged(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_sign: &CheckSignBlob,
    row: &InboxRow,
    command: CanonicalFiscalCommand,
    budget_gate: TimeBudgetGate<'_>,
) -> Result<FiscalOutcome, FiscalError> {
    let request_id = row.request_id;
    // Captured BEFORE `command` is moved into stage_acquire — the reject arm
    // (PR-Z2 step 3) needs it to scope the shift-class escalate.
    let doc_type = command.doc_type;
    // build_canonical / build_z_canonical validated driver_id is present + well-formed.
    let driver_id = row
        .driver_id
        .as_deref()
        .expect("build_canonical/build_z_canonical guarantees driver_id present");

    // T2 (RULING 3.5) — offline code CLOSE-RESERVE gate.  BEFORE the lazy-BEGIN
    // mint AND before stage_acquire mints any row/lnd, refuse an ORDINARY offline
    // op (SELL/RETURN) when granting its code would starve the codes the shift
    // needs to CLOSE offline (the lazy BEGIN, if not yet issued, + the offline Z).
    // A row-less, pool-less, fail-closed 503 (mirrors ensure_offline_session_begin
    // below).  Close-path ops (offline Z / BEGIN / END) bypass by doc-type — they
    // always draw the reserve.  Placed HERE (not deeper at acquire) so a refused
    // SELL does NOT even trigger its lazy-BEGIN mint, keeping the pool intact for
    // the eventual BEGIN+Z close.  Invariant: «a shift is NEVER wedged
    // un-closable for lack of a code».
    enforce_offline_close_reserve(pool, row, doc_type).await?;

    // T3 (RULING 3.2/3.3) — the document-derived TIME-budget enforcement gate.
    // Placed in the SAME pre-mint lane as the T2 close-reserve: a NEW ordinary
    // SELL/RETURN over an ENFORCED budget (24h shift / 36h session / 168h month)
    // is refused row-less (503), BEFORE the lazy BEGIN mint / stage_acquire, so
    // no row/lnd/code is consumed. Close-path ops bypass by doc-type inside the
    // gate (the legal close is never blocked). Tracking is always on; the auto-Z
    // ticker owns the UNCONDITIONAL 24h close (RULING 3.4), separate from here.
    enforce_time_budgets(pool, budget_gate, row, doc_type).await?;

    // B10 (W10a) — the COMMON first-offline-doc seam.  BEFORE this doc mints,
    // ensure the session's DocType=9 BEGIN is issued (or fail-closed if a crashed
    // BEGIN rests non-terminal) — for SHIFT_OPEN / SELL / RETURN / Z_REPORT /
    // SHIFT_CLOSE alike (Finding B + HOLE A).  The BEGIN takes the LOWEST offline
    // lnd (drains first, opens the DPS offline window) and its
    // `unsigned_xml_sha256` becomes this doc's chain `previous_hash`.  Runs on the
    // caller's held FN gate (sequential sub-drive) and OUTSIDE every write tx
    // (INV-1: the BEGIN signs in its own `run_staged` sign stage).  The boundary
    // docs themselves are excluded inside the helper (no recursion).
    ensure_offline_session_begin(
        pool,
        pool_secure,
        dps,
        sign_ctx,
        fn_sign,
        row,
        &command,
        budget_gate,
    )
    .await?;

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
            if let Err(err) = audit_log::append(
                pool,
                "ingress_inbox",
                &id_hex,
                "INLINE_NOOP_UNEXPECTED",
                Severity::Critical,
                None,
                None,
            )
            .await
            {
                // Best-effort by design (the ledger answer must still go
                // out), but a failed CRITICAL audit is never silent.
                tracing::error!(
                    request_id = %id_hex,
                    error = %err,
                    "INLINE_NOOP_UNEXPECTED critical audit append failed"
                );
            }
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
                    // PR-Z2 step 3 (S5-A minimal-viable): a SHIFT-CLASS doc reject
                    // escalates the FN shift to RMR (edge 4 Opening→RMR / edge 12
                    // Closing→RMR).  The reject REASON (a pure DocumentReject vs a
                    // Server hard-reject) is INDISTINGUISHABLE at this layer —
                    // `RoutingDecision` collapses both to `(Rejected, TerminalReject)`
                    // and does NOT carry the `AuthorizationKind` — so both
                    // conservatively escalate to RMR (the designed catch-all
                    // operator surface for shift/Z anomalies, CLAUDE.md
                    // trigger-family 2).  Edge 11 (a pure DocumentReject →
                    // `Closing→Opened` rollback + operator reissue) and the
                    // reason-surfacing `ShiftRecoveryClass` are a NAMED RESIDUAL
                    // (the recovery increment, which owns that layer).  A
                    // SELL/RETURN reject stays a per-doc terminal — it MUST NOT kill
                    // the shift.  The doc rests `Rejected` (D2, pre-SENT) either way.
                    if is_shift_class(doc_type) {
                        let rmr =
                            escalate_shift_class_reject(pool, &fiscal_number, doc_id, request_id)
                                .await;
                        terminalise_inbox(pool, row, "INLINE_Z_SHIFT_REJECT", code_of(&rmr))
                            .await?;
                        Err(rmr)
                    } else {
                        // `fe` is ALREADY mapped by classify_send_outcome (terminal
                        // DPS reject / signer mismatch / structural) — terminalise
                        // with its own code, then return it; no re-mapping.
                        terminalise_inbox(pool, row, "INLINE_SEND_REJECT", code_of(&fe)).await?;
                        Err(fe)
                    }
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
                SendDisposition::ResolveReplay { observed } => {
                    // No-wire idempotent re-entry / race (stage_send StateConflict
                    // / DocumentMissing): resolve the durable truth from the
                    // ledger, never a phantom terminal 500.  On a TERMINAL
                    // resolver verdict, also terminalise OUR lease (review
                    // OCF-3/F2 — this arm owns the PROCESSING row, unlike the
                    // Noop/NotNew arms which must not touch a foreign lease).
                    tracing::warn!(
                        request_id = %hex_encode_lower(&request_id),
                        observed = ?observed,
                        "inline send hit a no-wire replay/race; resolving from the ledger"
                    );
                    match resolve_against_ledger(pool, request_id).await {
                        Ok(outcome) => Ok(outcome),
                        Err(fe) => {
                            if is_terminal_ledger_verdict(&fe) {
                                terminalise_inbox(
                                    pool,
                                    row,
                                    "INLINE_RESOLVE_TERMINAL",
                                    code_of(&fe),
                                )
                                .await?;
                            } else {
                                // Unknown truth — leave OUR lease PROCESSING
                                // so recovery converges; replay answers from
                                // the ledger (never a false REJECTED).
                                tracing::warn!(
                                    request_id = %hex_encode_lower(&request_id),
                                    code = code_of(&fe),
                                    "ledger resolve was inconclusive; lease left \
                                     PROCESSING for recovery"
                                );
                            }
                            Err(fe)
                        }
                    }
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
                                Ok(()) => {
                                    // First-pass↔replay parity (review OCF-1):
                                    // surface the `first_kvt1_at` stamp the
                                    // advance just wrote (the Sent→Kvt1 CAS
                                    // stamps it).  Best-effort read-only — a
                                    // failed read degrades to fiscal_ts=None
                                    // rather than failing an ACKed receipt.
                                    let fiscal_ts =
                                        fiscal_documents::terminal_outcome_by_request_id(
                                            pool,
                                            &request_id,
                                        )
                                        .await
                                        .ok()
                                        .flatten()
                                        .and_then(|o| o.first_kvt1_at);
                                    Ok(FiscalOutcome {
                                        document_id: doc_id,
                                        fiscal_id: Some(server_fiscal_no),
                                        fiscal_ts,
                                        document_state: DocState::Ack,
                                        report_xml: None,
                                    })
                                }
                                Err(ConfirmError::ChainSeedMismatch { .. }) => {
                                    // SW-4 (A.3 PR-B step 8): a chain-seed breach is NOT
                                    // a StructuralDrift — it is the online-lane seed-fork
                                    // surface (AUD-L2-1b). ESCALATE the FN to Manual
                                    // reconciliation (durable operator surface), mirroring
                                    // the online-convergence tick + boot-KVT2 arms, instead
                                    // of blind-terminalising the receipt REJECTED. The doc
                                    // rests durably at `Sent`/`Kvt2` (the advance envelope
                                    // rolled back); the RMR shift IS the operator surface,
                                    // so we do NOT terminalise the inbox (that would falsely
                                    // REJECT an issued-unconfirmed receipt) — return the same
                                    // SHIFT_MANUAL_RECON 500 any RMR-blocked request yields,
                                    // leaving the lease for operator-driven resolution.
                                    //
                                    // DORMANT post-PR-A: no producer reaches this arm today
                                    // (the inline lane is UnimplementedWritePath-bound in
                                    // prod, and finalize's ChainSeedMismatch producer was
                                    // removed in PR-A — see StageFinalizeError::
                                    // ChainSeedMismatch). This wires the ROUTING so a
                                    // producer returning here (PR-C scan→escalate) lands on
                                    // the manual-recon seam, not the drift terminaliser.
                                    // Escalate is idempotent (already-RMR → clean no-op).
                                    // Extracted to `escalate_inline_chain_seed_mismatch` so
                                    // the routing is unit-pinnable via synthetic injection
                                    // (the producer is DORMANT, so it cannot be reached
                                    // through the full inline path today).
                                    Err(escalate_inline_chain_seed_mismatch(
                                        pool,
                                        &row.fiscal_number,
                                        doc_id,
                                        request_id,
                                    )
                                    .await)
                                }
                                Err(ConfirmError::StructuralDrift { .. }) => {
                                    // A routing/structural drift (NOT a chain-seed breach):
                                    // the doc stays durably at `Sent`/`Kvt2` (the advance
                                    // envelopes rolled back). Terminalise the inbox REJECTED
                                    // + audit — an INTENTIONAL, AUDITED divergence (Sent doc
                                    // + REJECTED inbox = the breach surface for B1/recon,
                                    // invariant #8, not silent) — and 500. This inline lane
                                    // is DORMANT (UnimplementedWritePath-bound in prod).
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
                                    // On a TERMINAL resolver verdict, terminalise
                                    // OUR lease too (review OCF-3/F2).
                                    match resolve_against_ledger(pool, request_id).await {
                                        Ok(outcome) => Ok(outcome),
                                        Err(fe) => {
                                            if is_terminal_ledger_verdict(&fe) {
                                                terminalise_inbox(
                                                    pool,
                                                    row,
                                                    "INLINE_RESOLVE_TERMINAL",
                                                    code_of(&fe),
                                                )
                                                .await?;
                                            } else {
                                                tracing::warn!(
                                                    request_id =
                                                        %hex_encode_lower(&request_id),
                                                    code = code_of(&fe),
                                                    "ledger resolve was inconclusive; \
                                                     lease left PROCESSING for recovery"
                                                );
                                            }
                                            Err(fe)
                                        }
                                    }
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

/// B10 (W10a) — ensure the active offline session has its DocType=9
/// (OFFLINE_SESSION_BEGIN) minted+issued BEFORE the FIRST offline doc of the
/// session signs.
///
/// **Fires for the FIRST offline doc of ANY type** (SHIFT_OPEN / SELL / RETURN /
/// Z_REPORT / SHIFT_CLOSE) — hoisted to the common `run_staged` seam (review
/// Finding B + HOLE A, 2026-07-08).  A Pattern-C shift opens OFFLINE, so the
/// first offline doc is a SHIFT_OPEN, NOT a SELL; and the offline-Z surface is
/// live — both must be bracketed by the BEGIN too.  The BEGIN must ALWAYS be
/// the lowest-lnd offline doc so the drain sends `[9, <first doc>, …, 10]` and
/// the DPS offline window is opened before ANY offline doc is drained.  Only
/// the boundary docs themselves (`OfflineSessionBegin`/`End`) are excluded (they
/// ARE the BEGIN, or the drain-time END — no recursion).
///
/// No-op unless: (a) the doc is NOT a boundary doc, AND (b) the node is in an
/// offline mode with an active OPEN session bound to the current shift.  Then,
/// keyed on `(fiscal_number, shift_id)` (NOT the sign-stamped
/// `offline_session_id`, which is NULL until sign — the crash-safe anchor):
///   - **BEGIN already issued** (`OFFLINE_LOCAL_ACK`/`SENT`/`KVT1`/`KVT2`/`ACK`)
///     → proceed (the seed already advanced; this doc chains off it).
///   - **BEGIN present but BELOW `OFFLINE_LOCAL_ACK`** (a crashed-mid-sign
///     PREPARED/SIGNED/… BEGIN) → fail-closed RETRYABLE 503
///     (`OFFLINE_SESSION_BEGIN_PENDING`): the offline lane bypasses the D5 gate,
///     so this guard stops ANY offline doc (incl. an offline Z — HOLE A) from
///     signing past a non-issued BEGIN predecessor (chain-fork + #192-wedge
///     prevention).  Boot-resume drives the BEGIN to OLA; the client retries.
///   - **BEGIN absent** → mint+sign+OLA it via [`mint_offline_session_begin`]
///     on the SAME FN gate the caller holds (sequential sub-drive), then
///     proceed.  It takes the lower lnd (allocated first) → drains first.
///
/// Runs BEFORE the doc's `stage_acquire`; INV-1 preserved (the BEGIN signs in
/// its own `run_staged` sign stage, outside every write tx).  The caller's FN
/// gate is held across this whole `run_staged` scope (no re-lock).
///
/// B10 — the DETERMINISTIC synthetic `request_id` for the session-BEGIN
/// (`sha256("b10-begin-<fn>")[..16]`).  One per FN, stamped at mint; the
/// crash-safe idempotency anchor shared by `ensure_offline_session_begin`'s
/// existence probe and `mint_offline_session_begin`'s inbox/doc insert.
fn begin_request_id(fiscal_number: &str) -> [u8; 16] {
    let idem = format!("b10-begin-{fiscal_number}");
    let h: [u8; 32] = <sha2::Sha256 as sha2::Digest>::digest(idem.as_bytes()).into();
    let mut r = [0u8; 16];
    r.copy_from_slice(&h[..16]);
    r
}

/// T2 (RULING 3.5) — the offline code CLOSE-RESERVE gate.
///
/// Refuses an ORDINARY offline op (SELL / RETURN) fail-closed PRE-MINT when
/// granting its pool code would leave fewer free codes than the shift needs to
/// be CLOSED offline.  The dynamic legal reserve (per contract):
///
/// ```text
///   reserve = (session BEGIN missing ? 1 : 0) + (offline Z still needed ? 1 : 0)
///   admit an ordinary offline SELL/RETURN  ⟺  free_codes >= 1 + reserve
/// ```
///
/// The `1 +` is the op's OWN code.  The BEGIN term is complementary across the
/// admit-cost / close-cost sides (admitting the op mints the BEGIN, so it is
/// present thereafter and does not need reserving twice) — the reduced form
/// above is exactly the contract's `required_codes_to_close` (arch adjudication
/// 2026-07-10).  Example pins: pool=2 + BEGIN-missing + Z-needed → need
/// `1+2=3 > 2` → REFUSE; pool=1 + BEGIN-present + Z-needed → need `1+1=2 > 1` →
/// REFUSE; pool=3 + BEGIN-missing + Z-needed → `3 >= 3` → ADMIT.
///
/// SCOPE (bypass = fall through to the normal flow, admit):
///   - doc-type: gate ONLY `Sell` / `Return`.  Close-path ops
///     (`ZReport` / `ShiftClose` / `OfflineSessionBegin` / `OfflineSessionEnd`)
///     always draw the reserve → excluded.  The lazy-BEGIN sub-drive re-enters
///     `run_staged` with `doc_type == OfflineSessionBegin`, so it is excluded too.
///   - node mode: only `Offline` / `GoingOffline`.  Online ops are never gated
///     (INV-5 is an OFFLINE budget; the reserve protects the offline pool only).
///   - shift: only `Opened` / `OpenedLocalPendingDrain` (a shift that still owes
///     an offline Z).  Other shift states → nothing to protect → bypass.
///   - an active OPEN offline session MUST exist (residual-hole #2): with no
///     session there is no offline shift to protect and no BEGIN to reserve for;
///     the op proceeds online-shaped (deferred at offline-ack) — bypass.
///
/// BEGIN-present predicate: `state ∈ {OfflineLocalAck, Sent, Kvt1, Kvt2, Ack}`
/// (the ISSUED set — the code is already consumed).  EVERYTHING else — `None`
/// AND terminal-FAILED (Aborted/Rejected/Cancelled/RMR, which
/// `ensure_offline_session_begin` RE-mints) — counts as MISSING, so its code is
/// reserved (residual-hole #1).  This mirrors that helper's admit set verbatim.
///
/// One `with_immediate` READ probe (INV-1: pure SQLite reads, no crypto/network)
/// computing the decision atomically under the caller's held FN write-lease
/// (INV-2: single-writer per fiscal_number — no other writer can interleave
/// between this probe and the mints that follow).  Read failure → fail-CLOSED
/// `Internal` (never fail-open — a read fault must not admit an op that could
/// strand the shift).
async fn enforce_offline_close_reserve(
    pool: &SqlitePool,
    row: &InboxRow,
    doc_type: DocType,
) -> Result<(), FiscalError> {
    // Doc-type filter: ordinary business ops only.  Everything else bypasses.
    if !matches!(doc_type, DocType::Sell | DocType::Return) {
        return Ok(());
    }
    let request_id = row.request_id;
    let fiscal_number = row.fiscal_number.as_str();

    // Node-mode gate (pool read; no write tx).  Online → not an offline op → no
    // reserve to protect.  Reuse the same offline-family predicate as the BEGIN
    // seam (inline.rs `ensure_offline_session_begin`).
    let ns = match node_state::get(pool, fiscal_number).await {
        Ok(Some(ns)) => ns,
        // No node_state row is a structural breach the op's own stages surface
        // with precise diagnostics; do not pre-empt them here.
        Ok(None) => return Ok(()),
        Err(e) => {
            tracing::error!(error=%e, "enforce_offline_close_reserve: node_state read failed");
            return Err(FiscalError::Internal {
                request_id,
                code: codes::ACQUIRE_INTERNAL,
            });
        }
    };
    if !matches!(ns.mode, NodeMode::Offline | NodeMode::GoingOffline) {
        return Ok(());
    }
    // Shift gate: only a shift that still owes an offline Z has a close-reserve.
    if !matches!(
        ns.shift_state,
        ShiftState::Opened | ShiftState::OpenedLocalPendingDrain
    ) {
        return Ok(());
    }

    let fn_owned = fiscal_number.to_string();
    let begin_rid = begin_request_id(fiscal_number);
    // One atomic read snapshot: (has active OPEN session, free code count, BEGIN
    // state).  `None` for the whole tuple ⇒ no active OPEN session ⇒ bypass.
    let probe: Option<(i64, Option<DocState>)> = crate::db::tx::with_immediate(pool, move |tx| {
        Box::pin(async move {
            // Residual-hole #2: gate only when an active OPEN session exists (the
            // same early-return the BEGIN seam's probe uses).
            if offline_sessions::current_active_session_id_tx(tx, &fn_owned)
                .await?
                .is_none()
            {
                return Ok::<Option<(i64, Option<DocState>)>, anyhow::Error>(None);
            }
            // free_codes — identical predicate to `acquire_code_tx`'s WHERE and to
            // the BEGIN seam's `has_code` probe (kept in lockstep on purpose).
            let free: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM offline_codes \
                 WHERE fiscal_number = ? AND consumed_at IS NULL AND dps_code IS NOT NULL",
            )
            .bind(&fn_owned)
            .fetch_one(&mut **tx)
            .await?;
            let begin_state = fiscal_documents::doc_state_by_request_id_tx(tx, &begin_rid).await?;
            Ok(Some((free, begin_state)))
        })
    })
    .await
    .map_err(|_| FiscalError::Internal {
        request_id,
        code: codes::ACQUIRE_INTERNAL,
    })?;

    let Some((free_codes, begin_state)) = probe else {
        // No active OPEN session → nothing to protect → bypass.
        return Ok(());
    };

    // BEGIN "present" iff it rests in the ISSUED set (code already consumed).
    // None + terminal-FAILED both count as MISSING (residual-hole #1): the BEGIN
    // seam re-mints those, so their code must be reserved.
    let begin_present = matches!(
        begin_state,
        Some(
            DocState::OfflineLocalAck
                | DocState::Sent
                | DocState::Kvt1
                | DocState::Kvt2
                | DocState::Ack
        )
    );
    // Boundary hand-off: when the pool is EMPTY and the BEGIN is not yet minted,
    // the session cannot even be OPENED — that is the more-specific domain of the
    // lazy-BEGIN pre-mint guard (`ensure_offline_session_begin` → 503
    // `OFFLINE_SESSION_BEGIN_PENDING`), which also refuses row-less/retryable.
    // Defer to it so the empty-pool diagnosis stays "can't open" rather than
    // "close-reserve held" (both are retryable 503; the specific code aids the
    // operator).  The reserve gate owns every OTHER starve case, INCLUDING
    // `free == 0` with a BEGIN already present (a subsequent sell on a drained
    // pool — the shift still owes a Z, so refusing here is exactly the invariant).
    if free_codes == 0 && !begin_present {
        return Ok(());
    }
    let reserve = i64::from(!begin_present) + 1; // (BEGIN missing?1:0) + Z(1, shift owes a Z here)
    let need = 1 + reserve; // this op's own code + the close-reserve
    if free_codes < need {
        return Err(FiscalError::OfflineRefused {
            request_id,
            code: codes::OFFLINE_CODE_RESERVE_HELD,
        });
    }
    Ok(())
}

/// T3 (RULING 3.2/3.3) — the document-derived TIME-budget enforcement gate.
///
/// Refuses a NEW ordinary op (SELL / RETURN) fail-closed PRE-MINT (row-less 503
/// `FiscalError::OfflineRefused`, the same lane as the T2 close-reserve) when a
/// budget is over its legal limit AND that budget's enforcement toggle is ON:
///   - 24h shift duration (`now − SHIFT_OPEN.business_ts`) — the widest wedge;
///   - 36h continuous offline session (`now − offline_sessions.opened_at`);
///   - 168h cumulative offline / calendar month (recomputed from sessions).
///
/// The legal CLOSE path is NEVER blocked (RULING 3.2): close-path ops
/// (`ZReport` / `ShiftClose` / `OfflineSessionBegin` / `OfflineSessionEnd`)
/// bypass by doc-type — a shift over 24h must still be closable, and the offline
/// budgets must not trap the offline Z / session END / drain. Tracking is ALWAYS
/// on (RULING 3.2); only refusal is toggled per budget via `gate.toggles`
/// (RULING 3.3). The auto-Z ticker acts on the 24h boundary UNCONDITIONALLY
/// (RULING 3.4) — separate from this gate.
///
/// Reads only (pure SQLite SELECTs via [`time_budget::compute_budgets_for_fn`];
/// INV-1 safe) under the caller's held FN write-lease (INV-2). A read fault →
/// fail-CLOSED `Internal` (never fail-open — RULING 3.6). Placed BEFORE the lazy
/// BEGIN mint + `stage_acquire`, so a refused op mints no row/lnd/code.
async fn enforce_time_budgets(
    pool: &SqlitePool,
    gate: TimeBudgetGate<'_>,
    row: &InboxRow,
    doc_type: DocType,
) -> Result<(), FiscalError> {
    // Doc-type filter: ordinary business ops only. Close-path + boundary ops
    // (Z / SHIFT_CLOSE / BEGIN / END) always pass — the legal close is never
    // blocked (RULING 3.2). SHIFT_OPEN is a lifecycle op that STARTS the shift
    // clock; refusing it on a stale budget makes no sense → excluded.
    if !matches!(doc_type, DocType::Sell | DocType::Return) {
        return Ok(());
    }
    let request_id = row.request_id;
    let fiscal_number = row.fiscal_number.as_str();

    let budgets = match time_budget::compute_budgets_for_fn(pool, gate.clock, fiscal_number).await {
        Ok(b) => b,
        Err(e) => {
            // Fail-CLOSED: a read fault must not admit an op that could push a
            // budget past its legal wall (RULING 3.6 — never fail-open).
            tracing::error!(error=%e, "enforce_time_budgets: budget read failed");
            return Err(FiscalError::Internal {
                request_id,
                code: codes::ACQUIRE_INTERNAL,
            });
        }
    };

    if let Some(refusal) = budgets.admission_refusal(gate.toggles) {
        // Map to the canonical `codes::` registry constant (the round-trip fence
        // in inline_map + the handler's 503 status map key off these literals).
        // `BudgetRefusal::code()` returns the SAME string (SSOT-checked below).
        let code = match refusal {
            time_budget::BudgetRefusal::Shift24h => codes::SHIFT_DURATION_LIMIT_EXCEEDED,
            time_budget::BudgetRefusal::Session36h => codes::OFFLINE_SESSION_LIMIT_EXCEEDED,
            time_budget::BudgetRefusal::Month168h => codes::OFFLINE_MONTH_LIMIT_EXCEEDED,
        };
        debug_assert_eq!(
            code,
            refusal.code(),
            "code registry ↔ BudgetRefusal::code drift"
        );
        return Err(FiscalError::OfflineRefused { request_id, code });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // T3 threaded the budget gate (deps kept explicit)
async fn ensure_offline_session_begin(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_sign: &CheckSignBlob,
    row: &InboxRow,
    command: &CanonicalFiscalCommand,
    budget_gate: TimeBudgetGate<'_>,
) -> Result<(), FiscalError> {
    // (a) The boundary docs themselves never recurse here (the BEGIN's own
    // `run_staged`, and the drain-time END, are excluded).  EVERY other offline
    // doc type — SHIFT_OPEN, SELL, RETURN, Z_REPORT, SHIFT_CLOSE — triggers the
    // lazy BEGIN when it is the session's first offline doc.
    if matches!(
        command.doc_type,
        DocType::OfflineSessionBegin | DocType::OfflineSessionEnd
    ) {
        return Ok(());
    }
    let request_id = row.request_id;
    let fiscal_number = row.fiscal_number.as_str();

    // (b) Offline-mode + active-shift gate.  A pool read (no write tx); if the
    // node is online, or no shift is bound, there is no offline session to
    // bracket — proceed to the normal (online) path.
    let ns = match node_state::get(pool, fiscal_number).await {
        Ok(Some(ns)) => ns,
        // No node_state row is a structural breach the SELL's own stages surface
        // with precise diagnostics; do not pre-empt them here.
        Ok(None) => return Ok(()),
        Err(e) => {
            tracing::error!(error=%e, "ensure_offline_session_begin: node_state read failed");
            return Err(FiscalError::Internal {
                request_id,
                code: codes::ACQUIRE_INTERNAL,
            });
        }
    };
    if !matches!(ns.mode, NodeMode::Offline | NodeMode::GoingOffline) {
        return Ok(());
    }

    // Shift-admissibility gate: only bracket a doc that its OWN shift guard would
    // ADMIT — otherwise the doc is refused and no BEGIN should be minted for it.
    //   - SHIFT_OPEN opens the shift → admissible while `Closed`; it is the ONLY
    //     doc that legitimately mints while `Closed` (Pattern-C offline open).
    //   - SELL / RETURN (and other business docs) require an OPEN-ish shift
    //     (`Opened` / `OpenedLocalPendingDrain`); on a `Closed` shift they are
    //     refused (`ShiftNotOpen`) — so do NOT interpose a BEGIN (the SellWith-
    //     ClosedShift no-op must mint no row).
    {
        use crate::db::models::enums::ShiftState;
        let admissible = match command.doc_type {
            DocType::ShiftOpen => matches!(
                ns.shift_state,
                ShiftState::Closed | ShiftState::Opened | ShiftState::OpenedLocalPendingDrain
            ),
            _ => matches!(
                ns.shift_state,
                ShiftState::Opened | ShiftState::OpenedLocalPendingDrain
            ),
        };
        if !admissible {
            // The doc's own acquire guard will refuse it (no BEGIN, no row).
            return Ok(());
        }
    }

    // Existence + state probe keyed on the BEGIN's DETERMINISTIC `request_id`
    // (`sha256("b10-begin-<fn>")[..16]`, the same id `mint_offline_session_begin`
    // stamps).  This is the ONLY crash-safe anchor that works for EVERY first-doc
    // type (Finding B):
    //   - `shift_id` fails when the first offline doc is a SHIFT_OPEN (the shift
    //     does not exist yet — the SHIFT_OPEN creates it), so a shift-keyed probe
    //     would miss and the SHIFT_OPEN would mint at lnd=1 with no BEGIN.
    //   - `offline_session_id` fails for a crashed PREPARED BEGIN (the column is
    //     NULL until sign) → a session-keyed probe would double-mint.
    // The synthetic `request_id` is stamped at MINT (stage_acquire INSERT) and is
    // present on a PREPARED / SIGNED / OLA / ACK BEGIN alike → no miss, no
    // double-mint.  Still gated on an active OPEN session (nothing to bracket
    // otherwise).  INV-1 static-scan: INLINE closure, captures hoisted.
    let fn_owned = fiscal_number.to_string();
    let begin_request_id = begin_request_id(fiscal_number);
    // Returns `Some((begin_state, pool_has_code))` when an active OPEN session
    // exists, else `None`.  `pool_has_code` is the pre-mint fail-closed guard: an
    // empty pool must refuse the BEGIN BEFORE minting one (else we'd mint a bare
    // BEGIN that immediately aborts — needless doc churn; the pre-B10 offline
    // shift-lifecycle refusal was clean pre-mint).
    let probe = crate::db::tx::with_immediate(pool, move |tx| {
        Box::pin(async move {
            // Require an active OPEN session (the doc would defer at offline-ack
            // otherwise; do not mint a BEGIN with no session).
            if offline_sessions::current_active_session_id_tx(tx, &fn_owned)
                .await?
                .is_none()
            {
                return Ok::<Option<(Option<DocState>, bool)>, anyhow::Error>(None);
            }
            let st = fiscal_documents::doc_state_by_request_id_tx(tx, &begin_request_id).await?;
            // A DPS-issued code is available iff a row has `consumed_at IS NULL AND
            // dps_code IS NOT NULL` (mirrors `acquire_code_tx`'s WHERE).
            let has_code: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM offline_codes \
                 WHERE fiscal_number = ? AND consumed_at IS NULL AND dps_code IS NOT NULL LIMIT 1",
            )
            .bind(&fn_owned)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(Some((st, has_code.is_some())))
        })
    })
    .await
    .map_err(|_| FiscalError::Internal {
        request_id,
        code: codes::ACQUIRE_INTERNAL,
    })?;

    let Some((begin_state, pool_has_code)) = probe else {
        // No active OPEN session → nothing to bracket yet.  The doc proceeds;
        // offline-ack defers it (NoActiveSession) per the established asymmetry.
        return Ok(());
    };

    match begin_state {
        // Issued (or drained): the seed already advanced — proceed.
        Some(
            DocState::OfflineLocalAck
            | DocState::Sent
            | DocState::Kvt1
            | DocState::Kvt2
            | DocState::Ack,
        ) => Ok(()),
        // Terminal-FAILED (a prior BEGIN attempt aborted / was rejected /
        // escalated): the slot is free (mirrors the migration-030 partial UNIQUE
        // `ux_fd_active_offline_boundary` which excludes these) → mint a fresh one.
        Some(
            DocState::Aborted
            | DocState::Rejected
            | DocState::Cancelled
            | DocState::RequiresManualReconciliation,
        )
        | None
            if !pool_has_code =>
        {
            // Pre-mint fail-closed: an empty code pool refuses the first offline
            // doc RETRYABLE (503) WITHOUT minting a bare BEGIN that would only
            // abort — the operator seeds codes (seed-codes / T=112) and retries.
            // Keeps the empty-pool case clean (no aborted-BEGIN churn), matching
            // the pre-B10 offline shift-lifecycle pre-mint refusal spirit.
            Err(FiscalError::OfflineRefused {
                request_id,
                code: codes::OFFLINE_SESSION_BEGIN_PENDING,
            })
        }
        Some(
            DocState::Aborted
            | DocState::Rejected
            | DocState::Cancelled
            | DocState::RequiresManualReconciliation,
        )
        | None => {
            mint_offline_session_begin(
                pool,
                pool_secure,
                dps,
                sign_ctx,
                fn_sign,
                fiscal_number,
                row.driver_id.as_deref(),
                request_id,
                budget_gate,
            )
            .await
        }
        // Crashed mid-sign, below OLA (PREPARED/SIGNED/ENCRYPTED/SENDING/
        // ERROR_RETRYABLE) → fail-closed RETRYABLE (Decision 5 / HOLE A): no doc
        // of any type may sign past a non-issued BEGIN.  Boot-resume drives it.
        Some(_) => Err(FiscalError::OfflineRefused {
            request_id,
            code: codes::OFFLINE_SESSION_BEGIN_PENDING,
        }),
    }
}

/// B10 — mint+sign+offline-ack a DocType=9 (OFFLINE_SESSION_BEGIN) via a
/// synthetic inbox row + the shared `run_staged` pipeline (Mechanism A —
/// reuses the proven offline mint path: lnd alloc, code acquire, `<MAC ID>`
/// stamp, `offline_session_id` stamp, chain-seed advance, crash recovery).
///
/// The command is built DIRECTLY (bypassing ingress `build_canonical`, which
/// rejects the gateway-internal boundary op-types by design).  Idempotent: the
/// synthetic inbox `idempotency_key` is deterministic per FN (`b10-begin-<fn>`),
/// so a re-drive after crash resolves to the existing row rather than a second
/// mint; the partial UNIQUE `ux_fd_active_offline_boundary` is the fail-closed
/// backstop.  Runs on the caller's FN gate (sequential sub-drive; no re-lock).
#[allow(clippy::too_many_arguments)]
async fn mint_offline_session_begin(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_sign: &CheckSignBlob,
    fiscal_number: &str,
    driver_id: Option<&str>,
    parent_request_id: [u8; 16],
    budget_gate: TimeBudgetGate<'_>,
) -> Result<(), FiscalError> {
    let internal_fail = |code: &'static str| FiscalError::Internal {
        request_id: parent_request_id,
        code,
    };

    // The BEGIN business_ts = the session's offline-entry time (`opened_at`) so
    // the DPS 168h budget counts from the real go-offline moment (docs-as-canon).
    // Fall back to now() only if the session row has no timestamp (never in
    // practice — insert_opening always stamps it).
    let opened_at: Option<String> = sqlx::query_scalar(
        "SELECT opened_at FROM offline_sessions \
         WHERE fiscal_number = ? AND state = 'OPEN' LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await
    .map_err(|_| internal_fail(codes::ACQUIRE_INTERNAL))?;
    let business_ts = opened_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let payload_json = "{}".to_string();
    let payload_sha256_canonical: [u8; 32] =
        <sha2::Sha256 as sha2::Digest>::digest(payload_json.as_bytes()).into();

    // Deterministic synthetic inbox row (idempotent per FN) — the SAME
    // request_id the existence probe keys on (`begin_request_id`).
    let idem = format!("b10-begin-{fiscal_number}");
    let synth_request_id: [u8; 16] = begin_request_id(fiscal_number);
    let entry = ingress_inbox::NewInboxEntry {
        request_id: synth_request_id,
        fiscal_number: fiscal_number.to_string(),
        protocol: crate::db::models::enums::Protocol::Internal,
        operation_type: DocType::OfflineSessionBegin.as_str().to_string(),
        idempotency_key: idem,
        payload_json: payload_json.clone(),
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: None,
        driver_id: driver_id.map(|s| s.to_string()),
        business_ts: Some(business_ts.clone()),
        total_sum_kop: None,
    };

    let synth_row: InboxRow = match ingress_inbox::insert(pool, &entry)
        .await
        .map_err(|_| internal_fail(codes::ACQUIRE_INTERNAL))?
    {
        ingress_inbox::InboxInsertOutcome::Created(r) => r,
        // Replay/Conflict → a prior tick already minted (or is minting) the
        // BEGIN.  The existence-probe in `ensure_offline_session_begin` should
        // have caught an issued/pending BEGIN; a Replay here means a crash
        // between inbox-insert and the fiscal_documents INSERT.  Fail-closed
        // RETRYABLE so the SELL retries after the redrive resolves the BEGIN.
        _ => {
            return Err(FiscalError::OfflineRefused {
                request_id: parent_request_id,
                code: codes::OFFLINE_SESSION_BEGIN_PENDING,
            })
        }
    };

    // Build the boundary CanonicalFiscalCommand DIRECTLY (ingress build rejects
    // the internal op-type by design).
    let command = CanonicalFiscalCommand {
        doc_type: DocType::OfflineSessionBegin,
        business_ts,
        total_sum_kop: None,
        payload_json,
        payload_sha256_canonical,
        source_sha256: payload_sha256_canonical,
        signed_by_cashier_id: None,
        driver_id: driver_id
            .map(crate::db::models::ids::DriverId::new)
            .transpose()
            .map_err(|_| internal_fail(codes::ACQUIRE_INTERNAL))?,
    };

    // Drive the BEGIN through the SAME staged pipeline as any offline doc.  On
    // the offline path it lands `OFFLINE_LOCAL_ACK` (seed advanced).  Anything
    // else → fail-closed RETRYABLE so the SELL does not sign off a non-issued
    // BEGIN (INV-4 chain integrity, Decision 5).
    //
    // `Box::pin` breaks the async-fn recursion cycle `run_staged →
    // ensure_offline_session_begin → mint_offline_session_begin → run_staged`.
    // The recursion is depth-1 by construction: the sub-`run_staged` runs a
    // boundary doc (`doc_type = OfflineSessionBegin`), which
    // `ensure_offline_session_begin` excludes → no further nesting.
    let begin_run = Box::pin(run_staged(
        pool,
        pool_secure,
        dps,
        sign_ctx,
        fn_sign,
        &synth_row,
        command,
        budget_gate,
    ));
    match begin_run.await {
        Ok(outcome) if outcome.document_state == DocState::OfflineLocalAck => Ok(()),
        _ => Err(FiscalError::OfflineRefused {
            request_id: parent_request_id,
            code: codes::OFFLINE_SESSION_BEGIN_PENDING,
        }),
    }
}

/// Live `Z_REPORT` / `SHIFT_CLOSE` dispatch (PR-Z2 online-half).  Pre-acquire:
/// resolve the open shift → quiesce it (C10 gate, INV #8) → aggregate the
/// shift's receipts → build the Z canonical (D5 dual-hash) → feed the shared
/// staged pipeline `run_staged` (acquire edge 8 Opened→Closing, sign the Z
/// number, send edge 10 Closing→Closed, advance→Ack).  Gated by
/// `FULL_Z_SURFACE_READY` at the caller (production stays fail-closed until the
/// step-4 flip); tested directly (flag-independent).
async fn run_z_dispatch(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    dps: &dyn DpsChannel,
    sign_ctx: &SigningContext,
    fn_sign: &CheckSignBlob,
    row: &InboxRow,
    budget_gate: TimeBudgetGate<'_>,
) -> Result<FiscalOutcome, FiscalError> {
    let request_id = row.request_id;
    let fiscal_number = row.fiscal_number.clone();

    // Resolve the FN's open shift (the one to quiesce / aggregate / close).  No
    // open shift → refuse (ShiftNotOpen 422); stage_acquire's Z@Closed guard
    // would reach the same verdict, but quiescence needs the shift_id up front.
    let shift_id = match node_state::get(pool, &fiscal_number).await {
        Ok(ns) => ns.and_then(|n| n.current_shift_id),
        Err(_e) => {
            let fe = FiscalError::Internal {
                request_id,
                code: codes::Z_DISPATCH_INTERNAL,
            };
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_NODE_READ_ERROR",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
    };
    let shift_id = match shift_id {
        Some(s) => s,
        None => {
            let fe = FiscalError::ShiftNotOpen { request_id };
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_NO_OPEN_SHIFT",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
    };

    // C10 quiescence gate (INV #8 — Z closes only via the C10 gate).  quiesce +
    // aggregate are benign PRE-guard: quiesce only inline-finalizes the leading
    // contiguous KVT2 run (short tx, no IO/crypto), aggregate is read-only — the
    // shift-state guard (Z@Opened) runs at stage_acquire below.
    match quiesce_shift_before_z(pool, &fiscal_number, shift_id).await {
        Ok(QuiescenceOutcome::Clear) => {}
        Ok(QuiescenceOutcome::Pending { blocking }) => {
            // STOP-S6 ruling (B): the retryable Z-quiescence-pending is CARRIED
            // on `OfflineRefused{Z_QUIESCENCE_PENDING}` (503) for boundary
            // discipline — a dedicated `FiscalError::QuiescencePending` variant +
            // a handler-aware SAME-KEY retry is a NAMED RESIDUAL (option C,
            // co-scoped with the recovery increment).  Seam obligation:
            // terminalise the inbox REJECTED (a same-key replay then resolves to
            // 500 INBOX_REJECTED — NOT "shift closed with error"; no Z issued).
            // Client contract: retry the close with a NEW idempotency key after
            // the runtime drives the blockers issued.
            let fe = FiscalError::OfflineRefused {
                request_id,
                code: codes::Z_QUIESCENCE_PENDING,
            };
            // Forensic (best-effort, S6 condition 2): which docs blocked the Z.
            let blockers: Vec<serde_json::Value> = blocking
                .iter()
                .map(|(id, st)| {
                    serde_json::json!({
                        "document_id": hex_encode_lower(id.as_bytes()),
                        "state": st.as_str(),
                    })
                })
                .collect();
            let payload = serde_json::json!({
                "blocking": blockers,
                "code": codes::Z_QUIESCENCE_PENDING,
            })
            .to_string();
            let _ = audit_log::append(
                pool,
                "ingress_inbox",
                &hex_encode_lower(&request_id),
                "INLINE_Z_QUIESCENCE_PENDING",
                Severity::Warning,
                None,
                Some(&payload),
            )
            .await;
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_QUIESCENCE_PENDING",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
        Err(_e) => {
            let fe = FiscalError::Internal {
                request_id,
                code: codes::Z_DISPATCH_INTERNAL,
            };
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_QUIESCENCE_ERROR",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
    }

    // Aggregate the quiesced shift's receipts into the signer-ready Z body.
    let aggregated = match aggregate_z_payload_for_shift(pool, &fiscal_number, shift_id).await {
        Ok(a) => a,
        Err(_e) => {
            let fe = FiscalError::Internal {
                request_id,
                code: codes::Z_DISPATCH_INTERNAL,
            };
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_AGGREGATE_ERROR",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
    };

    // Build the Z canonical (D5 dual-hash: source = wire-intent hash, canonical
    // = the recomputed aggregated-body hash).
    let command = match build_z_canonical(row, &aggregated) {
        Ok(c) => c,
        Err(reject) => {
            let fe = map_build_reject(&reject, request_id);
            return match terminalise_inbox_pre_acquire(
                pool,
                row,
                "INLINE_Z_BUILD_REJECT",
                code_of(&fe),
            )
            .await?
            {
                PreAcquireTerminalise::Terminalised => Err(fe),
                PreAcquireTerminalise::NotNew => resolve_against_ledger(pool, request_id).await,
            };
        }
    };

    run_staged(
        pool,
        pool_secure,
        dps,
        sign_ctx,
        fn_sign,
        row,
        command,
        budget_gate,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    // CS-1b′: store-side id wrappers for the sqlx boundary in test fixtures.
    use crate::db::types::{DbDocumentId, DbShiftId};
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
        async fn ask_offline_codes(
            &self,
            _: CheckEnvelope,
        ) -> Result<crate::transports::dps::dto::OfflineCodesResponse, DpsError> {
            unreachable!("stub: ask_offline_codes not exercised");
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
        let dps = StubLastChk::new(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])));
        let outcome = online_confirm(&dps, &fn_sign(), SERVER_FISCAL_NO).await;
        assert_eq!(outcome, InlineConfirmOutcome::Acked(vec![0xDE; 64]));
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

    /// The terminalise-on-resolver-Err discriminator: ONLY a positive
    /// terminal verdict (DpsRejected / manual-recon) may terminalise our
    /// lease; unknown-truth drift must NOT (a doc still converging to ACK
    /// via boot recovery would otherwise replay as a false `Failed`).
    #[test]
    fn terminal_ledger_verdict_discriminates_known_terminal_from_unknown_drift() {
        const RID2: [u8; 16] = [0x22; 16];
        assert!(is_terminal_ledger_verdict(&FiscalError::DpsRejected {
            request_id: RID2
        }));
        assert!(is_terminal_ledger_verdict(&FiscalError::Internal {
            request_id: RID2,
            code: codes::SHIFT_MANUAL_RECON,
        }));
        // Unknown truth — never terminalise.
        assert!(!is_terminal_ledger_verdict(&FiscalError::Internal {
            request_id: RID2,
            code: codes::REPLAY_LEDGER_DRIFT,
        }));
        assert!(!is_terminal_ledger_verdict(&FiscalError::Internal {
            request_id: RID2,
            code: codes::INBOX_TERMINALISE_FAILED,
        }));
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

    // ── SW-4 (A.3 PR-B step 8): inline confirm-error arm hygiene ────────────
    //
    // The `ConfirmError::ChainSeedMismatch` producer is DORMANT post-PR-A
    // (finalize's typed producer was removed), so the arm cannot be reached
    // through the full inline path — pin the ROUTING via synthetic injection:
    // call the extracted handler directly. `escalate_inline_chain_seed_mismatch`
    // routes to the shared FN → Manual-recon seam (→ RMR, idempotent); the
    // paired negative proves the StructuralDrift action (terminalise) leaves the
    // shift UNTOUCHED (no RMR).

    use crate::db::models::enums::Protocol;
    use crate::db::models::ids::ShiftId;
    use crate::db::repositories::ingress_inbox::NewInboxEntry;

    const SW4_FN: &str = "1234567890";

    async fn sw4_pool() -> sqlx::SqlitePool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sw4.db");
        std::mem::forget(dir);
        let pool = crate::db::open_pool(&path).await.unwrap();
        sqlx::query(
            "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
             VALUES (?, '12345678', 'test')",
        )
        .bind(SW4_FN)
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn seed_opened_shift_and_node(pool: &sqlx::SqlitePool) -> ShiftId {
        let shift_id = ShiftId::new();
        sqlx::query(
            "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
                cash_balance_kop, opened_by_cashier_id) \
             VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
        )
        .bind(DbShiftId(shift_id))
        .bind(SW4_FN)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO node_state(fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
                backend_profile_id, transport_profile_id) \
             VALUES (?, 'ONLINE', 'OPENED', ?, 1, 'b', 't')",
        )
        .bind(SW4_FN)
        .bind(DbShiftId(shift_id))
        .execute(pool)
        .await
        .unwrap();
        shift_id
    }

    async fn read_shift_state(pool: &sqlx::SqlitePool, shift_id: ShiftId) -> String {
        sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
            .bind(DbShiftId(shift_id))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn escalate_audit_count(pool: &sqlx::SqlitePool) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log \
             WHERE event_type = 'INLINE_ADVANCE_CHAIN_SEED_MISMATCH_ESCALATE_MANUAL'",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// SW-4 GREEN: an inline-confirm ChainSeedMismatch ESCALATES the FN shift to
    /// RequiresManualReconciliation (mirror of the online-convergence/boot arms)
    /// and returns the `SHIFT_MANUAL_RECON` 500 — NOT a blind REJECTED terminal.
    /// RED against the pre-SW-4 combined arm (which terminalised without ever
    /// escalating → the shift would stay OPENED).
    #[tokio::test]
    async fn chain_seed_mismatch_escalates_fn_to_manual() {
        let pool = sw4_pool().await;
        let shift = seed_opened_shift_and_node(&pool).await;
        let doc_id = DocumentId::new();
        let request_id = [0x5Au8; 16];

        let fe = escalate_inline_chain_seed_mismatch(&pool, SW4_FN, doc_id, request_id).await;

        assert!(
            matches!(fe, FiscalError::Internal { code, .. } if code == codes::SHIFT_MANUAL_RECON),
            "chain-seed mismatch must surface the manual-recon 500, got {fe:?}"
        );
        assert_eq!(
            read_shift_state(&pool, shift).await,
            "REQUIRES_MANUAL_RECONCILIATION",
            "the FN shift must be escalated to Manual (durable operator surface)"
        );
        assert_eq!(escalate_audit_count(&pool).await, 1, "one escalate audit");
    }

    /// SW-4: the escalation is IDEMPOTENT — a second injection (re-tick / retry)
    /// on an already-RMR shift is a clean no-op: shift stays RMR, NO duplicate
    /// escalate audit (the ledger is not flooded).
    #[tokio::test]
    async fn chain_seed_mismatch_escalation_is_idempotent() {
        let pool = sw4_pool().await;
        let shift = seed_opened_shift_and_node(&pool).await;
        let doc_id = DocumentId::new();
        let request_id = [0x5Bu8; 16];

        let _ = escalate_inline_chain_seed_mismatch(&pool, SW4_FN, doc_id, request_id).await;
        let fe2 = escalate_inline_chain_seed_mismatch(&pool, SW4_FN, doc_id, request_id).await;

        assert!(
            matches!(fe2, FiscalError::Internal { code, .. } if code == codes::SHIFT_MANUAL_RECON),
            "second injection still surfaces the manual-recon 500"
        );
        assert_eq!(
            read_shift_state(&pool, shift).await,
            "REQUIRES_MANUAL_RECONCILIATION"
        );
        assert_eq!(
            escalate_audit_count(&pool).await,
            1,
            "idempotent: already-RMR re-injection must NOT re-audit"
        );
    }

    /// PR-Z2 step 3 — the shift-class reject escalation is IDEMPOTENT (it reuses
    /// the shared escalate seam's already-RMR early-return): a second call on an
    /// already-RMR shift is a clean no-op — the shift stays RMR, NO duplicate
    /// escalate audit.  Mirrors the chain-seed idempotency proof.
    #[tokio::test]
    async fn shift_class_reject_escalation_is_idempotent() {
        let pool = sw4_pool().await;
        let shift = seed_opened_shift_and_node(&pool).await;
        let doc_id = DocumentId::new();
        let request_id = [0x5Du8; 16];

        let _ = escalate_shift_class_reject(&pool, SW4_FN, doc_id, request_id).await;
        let fe2 = escalate_shift_class_reject(&pool, SW4_FN, doc_id, request_id).await;

        assert!(
            matches!(fe2, FiscalError::Internal { code, .. } if code == codes::SHIFT_MANUAL_RECON),
            "second injection still surfaces the manual-recon 500"
        );
        assert_eq!(
            read_shift_state(&pool, shift).await,
            "REQUIRES_MANUAL_RECONCILIATION"
        );
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log \
             WHERE event_type = 'INLINE_Z_SHIFT_REJECT_ESCALATE_MANUAL'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            n, 1,
            "idempotent: already-RMR re-injection must NOT re-audit"
        );
    }

    /// SW-4 paired negative: the StructuralDrift arm behaves the OLD way —
    /// `terminalise_inbox` marks the inbox REJECTED (+500) and does NOT touch the
    /// shift (no RMR escalation). Proves the split is real: only ChainSeedMismatch
    /// escalates.
    #[tokio::test]
    async fn structural_drift_terminalises_without_escalation() {
        let pool = sw4_pool().await;
        let shift = seed_opened_shift_and_node(&pool).await;
        let request_id = [0x5Cu8; 16];
        ingress_inbox::insert(
            &pool,
            &NewInboxEntry {
                request_id,
                fiscal_number: SW4_FN.into(),
                protocol: Protocol::Rest,
                operation_type: "SELL".into(),
                idempotency_key: "sw4-drift".into(),
                payload_json: "{}".into(),
                payload_sha256_canonical: [0u8; 32],
                correlation_id: None,
                signed_by_cashier_id: None,
                driver_id: None,
                business_ts: Some("2026-07-06T00:00:00Z".into()),
                total_sum_kop: Some(1500),
            },
        )
        .await
        .unwrap();
        // Lease the row (NEW → PROCESSING) — the state terminalise_inbox requires.
        let row = with_immediate(&pool, move |tx| {
            Box::pin(async move { Ok(ingress_inbox::acquire_lease(tx, &request_id).await?) })
        })
        .await
        .unwrap()
        .expect("leased inbox row");

        terminalise_inbox(
            &pool,
            &row,
            "INLINE_ADVANCE_DRIFT",
            codes::REPLAY_LEDGER_DRIFT,
        )
        .await
        .unwrap();

        let inbox_status: String =
            sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
                .bind(&request_id[..])
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            inbox_status, "REJECTED",
            "StructuralDrift terminalises the inbox"
        );
        assert_eq!(
            read_shift_state(&pool, shift).await,
            "OPENED",
            "StructuralDrift must NOT escalate the shift (no RMR) — that is the split"
        );
    }

    // ── STOP-O3-1 fix (b) runtime half — terminalise_inbox escalates the
    //    orphaned pending-drain shift when the ABORTED dangling doc is
    //    shift-class.  (The only remaining PRODUCTION trigger after fix (a) is
    //    the GO_ONLINE mode-race: session+pool present at acquire → edge 2/9/7
    //    commits + doc SIGNED → mode flips GoingOnline → dispatch_post_sign
    //    returns Refused → this terminalise arm.  Tested directly here.) ──────

    async fn seed_pending_drain_shift(pool: &sqlx::SqlitePool, shift_state: &str) -> ShiftId {
        let shift_id = ShiftId::new();
        sqlx::query(
            "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
                cash_balance_kop, opened_by_cashier_id) \
             VALUES (?, ?, 1, ?, 'OFFLINE', 0, 'csh')",
        )
        .bind(DbShiftId(shift_id))
        .bind(SW4_FN)
        .bind(shift_state)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO node_state(fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
                backend_profile_id, transport_profile_id) \
             VALUES (?, 'OFFLINE', ?, ?, 2, 'b', 't')",
        )
        .bind(SW4_FN)
        .bind(shift_state)
        .bind(DbShiftId(shift_id))
        .execute(pool)
        .await
        .unwrap();
        shift_id
    }

    /// Seed a leased (PROCESSING) inbox row + a dangling SIGNED doc of
    /// `doc_type` bound to `shift_id`, sharing `request_id` — the exact
    /// post-acquire/pre-offline-ack shape `terminalise_inbox` compensates.
    async fn seed_leased_inbox_with_signed_doc(
        pool: &sqlx::SqlitePool,
        request_id: [u8; 16],
        doc_type: &str,
        shift_id: ShiftId,
    ) -> InboxRow {
        ingress_inbox::insert(
            pool,
            &NewInboxEntry {
                request_id,
                fiscal_number: SW4_FN.into(),
                protocol: Protocol::Rest,
                operation_type: doc_type.into(),
                idempotency_key: format!("k-{}", hex_encode_lower(&request_id)),
                payload_json: "{}".into(),
                payload_sha256_canonical: [0u8; 32],
                correlation_id: None,
                signed_by_cashier_id: None,
                driver_id: None,
                business_ts: Some("2026-07-07T00:00:00Z".into()),
                total_sum_kop: None,
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
                doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
                payload_json, payload_sha256_canonical, unsigned_xml_sha256) \
             VALUES (?, ?, ?, ?, 1, ?, 'SIGNED', 'b', 't', 'OFFLINE', '2026-07-07T00:00:00Z', \
                '{}', ?, ?)",
        )
        .bind(DbDocumentId(DocumentId::new()))
        .bind(&request_id[..])
        .bind(SW4_FN)
        .bind(DbShiftId(shift_id))
        .bind(doc_type)
        .bind(&[0u8; 32][..])
        .bind(&[0xA1u8; 32][..])
        .execute(pool)
        .await
        .unwrap();
        with_immediate(pool, move |tx| {
            Box::pin(async move { Ok(ingress_inbox::acquire_lease(tx, &request_id).await?) })
        })
        .await
        .unwrap()
        .expect("leased inbox row")
    }

    async fn doc_state_by_request(pool: &sqlx::SqlitePool, request_id: &[u8; 16]) -> String {
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE request_id = ?")
            .bind(&request_id[..])
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn terminalise_aborts_shift_open_and_escalates_olpd_orphan_to_rmr() {
        let pool = sw4_pool().await;
        let shift = seed_pending_drain_shift(&pool, "OPENED_LOCAL_PENDING_DRAIN").await;
        let request_id = [0x61u8; 16];
        let row = seed_leased_inbox_with_signed_doc(&pool, request_id, "SHIFT_OPEN", shift).await;

        terminalise_inbox(
            &pool,
            &row,
            "INLINE_DISPATCH_REFUSED",
            codes::OFFLINE_CODE_POOL_EXHAUSTED,
        )
        .await
        .unwrap();

        // The dangling doc aborted (#192) AND the orphaned OLPD shift escalated.
        assert_eq!(doc_state_by_request(&pool, &request_id).await, "ABORTED");
        assert_eq!(
            read_shift_state(&pool, shift).await,
            "REQUIRES_MANUAL_RECONCILIATION",
            "fix (b): the SHIFT_OPEN abort escalates its orphaned OLPD shift (edge 6)"
        );
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log \
             WHERE event_type = 'INLINE_OFFLINE_LIFECYCLE_ABORT_ESCALATE_MANUAL'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 1, "one forensic escalation audit");
    }

    #[tokio::test]
    async fn terminalise_aborts_z_report_and_escalates_clpd_orphan_to_rmr() {
        let pool = sw4_pool().await;
        let shift = seed_pending_drain_shift(&pool, "CLOSING_LOCAL_PENDING_DRAIN").await;
        let request_id = [0x62u8; 16];
        let row = seed_leased_inbox_with_signed_doc(&pool, request_id, "Z_REPORT", shift).await;

        terminalise_inbox(
            &pool,
            &row,
            "INLINE_DISPATCH_REFUSED",
            codes::OFFLINE_CODE_POOL_EXHAUSTED,
        )
        .await
        .unwrap();

        assert_eq!(doc_state_by_request(&pool, &request_id).await, "ABORTED");
        assert_eq!(
            read_shift_state(&pool, shift).await,
            "REQUIRES_MANUAL_RECONCILIATION",
            "fix (b): the Z abort escalates its orphaned CLPD shift (edge 14)"
        );
    }

    /// Doc-type gate: a REGULAR fiscal doc (SELL) aborted on an OLPD shift does
    /// NOT escalate — a receipt refusal never orphans the shift (its anchor
    /// SHIFT_OPEN doc is healthy).  The shift stays OLPD.
    #[tokio::test]
    async fn terminalise_aborts_sell_on_olpd_shift_without_escalation() {
        let pool = sw4_pool().await;
        let shift = seed_pending_drain_shift(&pool, "OPENED_LOCAL_PENDING_DRAIN").await;
        let request_id = [0x63u8; 16];
        let row = seed_leased_inbox_with_signed_doc(&pool, request_id, "SELL", shift).await;

        terminalise_inbox(
            &pool,
            &row,
            "INLINE_DISPATCH_REFUSED",
            codes::OFFLINE_CODE_POOL_EXHAUSTED,
        )
        .await
        .unwrap();

        assert_eq!(doc_state_by_request(&pool, &request_id).await, "ABORTED");
        assert_eq!(
            read_shift_state(&pool, shift).await,
            "OPENED_LOCAL_PENDING_DRAIN",
            "a SELL abort is NOT shift-class → no escalation (the shift is not orphaned)"
        );
    }
}
