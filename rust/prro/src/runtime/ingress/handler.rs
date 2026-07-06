//! RS-2 piece-5a — the inline-synchronous ingress handler.
//!
//! The axum-free core that turns a deserialised wire [`CanonicalCommand`]
//! into a typed response, threading the RS-2 pieces:
//!
//! ```text
//!   classify (1b) → map+validate (W3) → convert OR keep-wire-intent (2a/2b)
//!     → idempotent inbox (M1) → write-path SEAM (3) → response DTO (4a)
//!   replay / conflict branches → replay resolver (4b)
//! ```
//!
//! piece-5b wraps this in per-listener axum servers + auth middleware +
//! the D2 loopback guard call-site; it maps [`IngressResponse::http_status`]
//! to an axum `StatusCode` and serialises [`IngressResponse::body`] as
//! JSON.  Keeping THIS layer axum-free lets the whole accept→seam→response
//! path be unit/integration-tested without a running server.
//!
//! ## Three load-bearing dispositions (operator-locked 2026-06-06)
//!
//! 1. **Seam-error delete is `NotImplemented`-only and GUARDED.** A failed
//!    `fiscalize` deletes the inbox row ONLY for [`FiscalError::NotImplemented`]
//!    (RS-3 unwired), and only via [`ingress_inbox::delete_new_by_request_id`]
//!    (`WHERE status='NEW'`), so a retry re-attempts deterministically
//!    instead of becoming a stuck `IN_PROGRESS` replay.  The match is
//!    **exhaustive (no `_`)**: when RS-3 adds real failure variants
//!    (`DpsRejected`, …) this stops compiling, forcing an explicit decision
//!    NOT to delete a durable row for a genuine fiscal failure.
//!    *Pre-RS-3 caveat (review round-2 Med-2):* the NEW row is COMMITTED
//!    before the seam call, and there is no worker/reaper yet — so a crash
//!    or a failed release between the insert and the delete leaks a NEW row
//!    that wedges its `idempotency_key` at `202 IN_PROGRESS` forever.  The
//!    closing fix is the RS-3 `acquire_lease` worker + a stale-NEW reaper
//!    (tracked); pre-RS-3 a failed release is at least traced out-of-band.
//!
//! 2. **Z (`ZReport`/`ShiftClose`) is NOT pre-aggregated through
//!    [`convert`].** The ledger Z-aggregation
//!    ([`convert::convert_to_signer_payload`]'s Z arm) happens at
//!    convert-time, BEFORE any RS-3 quiescence/drain barrier — so the
//!    handler must NOT run it here.  Instead the inbox stores the **wire
//!    intent** (the mapped wire-shape payload).  This is purely a
//!    seam-INPUT contract: RS-3 decides whether to aggregate behind its
//!    single-writer barrier (two-payload model) or to update the persisted
//!    payload to signer-ready at that point.  Pre-RS-3 the seam returns
//!    `NotImplemented`, so no signer ever consumes the wire intent.
//!
//! 3. **HTTP status is mapped by stable `error_code`** (one source of
//!    truth, [`http_status_for_error_code`]) — NOT a blanket "failure →
//!    4xx".  Conflict→409, read-only/unsupported→422, in-progress→202,
//!    not-implemented→501, ledger drift→500, inbox/fiscal rejected→422.
//!    *5b contract (review round-2 D-Low):* `IN_PROGRESS`→202 is the ONE
//!    `ok:false` body that is NOT terminal — piece-5b MUST translate it for
//!    a synchronous/blocking caller (re-resolve, or signal a retry the
//!    shim understands), not surface it as a rejection.  And because the
//!    422 bucket folds *prior-receipt* outcomes (`INBOX_REJECTED` /
//!    `FISCAL_REJECTED`) in with *this-request* faults, a consumer MUST
//!    switch on `error_code`, not on the HTTP status alone.
//!
//! [`convert`]: super::convert
//! [`convert::convert_to_signer_payload`]: super::convert::convert_to_signer_payload

use super::convert::{convert_to_signer_payload, ConvertError};
use super::dto::{
    request_id_to_string, to_canonical_fiscal_command_with_context, CanonicalCommand,
    CanonicalErrorResponse, CanonicalResponse, CommandType, MappingError, Totals, SCHEMA_VERSION,
};
use super::policy::{classify_command, CommandClass};
use super::replay::{conflict_response, resolve_replay, ReplayResolution};
use super::seam::{FiscalError, FiscalOutcome, WritePathEntry};
use crate::db::models::enums::{DocState, Protocol, Severity};
use crate::db::models::ids::{DriverId, RequestId};
use crate::db::repositories::audit_log;
use crate::db::repositories::ingress_inbox::{self, InboxInsertOutcome, NewInboxEntry};
use sqlx::SqlitePool;

/// The handler's axum-free result: an HTTP status code + a typed body.
/// piece-5b maps `http_status` to an axum `StatusCode` and serialises the
/// body as JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct IngressResponse {
    pub http_status: u16,
    pub body: IngressBody,
}

/// Either a success envelope ([`CanonicalResponse`], `ok:true`) or a typed
/// error envelope ([`CanonicalErrorResponse`], `ok:false`).
#[derive(Debug, Clone, PartialEq)]
pub enum IngressBody {
    Success(CanonicalResponse),
    Error(CanonicalErrorResponse),
}

/// Single source of truth: a stable machine `error_code` → HTTP status.
/// Routing every error path through this keeps the status deterministic
/// and testable independently of the call site (operator-locked map).
///
/// An UNKNOWN code falls through to `500` — fail-safe, NEVER a silent 2xx.
pub fn http_status_for_error_code(code: &str) -> u16 {
    match code {
        "IDEMPOTENCY_CONFLICT" => 409,
        // still-processing replay — a "retry", not a terminal failure.
        "IN_PROGRESS" => 202,
        // RS-3 unwired — distinct, retriable, NOT a 2xx phantom success.
        "NOT_IMPLEMENTED" => 501,
        // client/control errors → 422 (unprocessable) / 400 (malformed).
        "READ_ONLY_COMMAND"
        | "UNSUPPORTED_COMMAND"
        // UNSUPPORTED_COMMAND_TYPE is policy-shadowed in-band (classify_command
        // rejects PeriodicReport as UNSUPPORTED_COMMAND before the mapper runs);
        // retained here for the out-of-band `to_canonical_fiscal_command` caller.
        | "UNSUPPORTED_COMMAND_TYPE"
        | "FN_MISMATCH"
        | "SCHEMA_VERSION_MISMATCH"
        | "INBOX_REJECTED"
        | "FISCAL_REJECTED"
        | "RAW_FRAMES_UNSUPPORTED"
        | "EMPTY_GOODS"
        | "SUM_NOT_DIVISIBLE"
        | "SECONDARY_TAX_REQUIRES_DUAL_MODE"
        | "MISSING_ITEM_CODE"
        | "ZERO_QUANTITY_LINE"
        | "VALUE_OVERFLOW"
        | "MISSING_PAYMENT_METHOD"
        | "INACTIVE_PAYMENT_METHOD"
        | "PAYMENT_SLOT_KIND_MISMATCH"
        | "ACQUIRER_SLIP_DEFERRED"
        | "NO_OPEN_SHIFT"
        // RS-3 A2 (T1) — ShiftGuardRefused carries one of these specific
        // shift-state codes; all are client/control 422s, NOT NO_OPEN_SHIFT.
        | "SHIFT_ALREADY_OPEN"
        | "SHIFT_OPEN_PENDING_DRAIN"
        | "POST_LOCAL_CLOSE_SALE_REFUSED"
        | "OFFLINE_SHIFT_CLOSE_NOT_SUPPORTED"
        | "SHIFT_CLOSING_IN_FLIGHT"
        | "Z_REPORT_BACKLOG_DRAIN_PENDING"
        // RS-3 A2.1b-core — SHIFT_OPEN is out of the SELL/RETURN inline core
        // (A2.2 owns it); fail-closed 422 (ShiftGuardRefused) until then.
        | "SHIFT_OPEN_NOT_SUPPORTED"
        // RS-3 A2 (Q-A) — the true signer-vs-opening-cashier mismatch is
        // client/operator-fixable (reissue with the correct cashier), pre-wire,
        // no fiscal commitment → 422, carried by ShiftGuardRefused.
        | "SIGNER_CASHIER_MISMATCH" => 422,
        "INVALID_CASHIER_ID" | "MALFORMED_JSON" => 400,
        // Adapter-shell codes (server.rs `adapter_error`) carry their own
        // hard-coded status; listed here so the map stays TOTAL over the
        // taxonomy — a future path that ever routes them through `err()`
        // gets the right status instead of the `_ => 500` fall-through.
        "UNKNOWN_SOURCE" | "NO_NODE_STATE" => 404,
        "FN_FORBIDDEN" => 403,
        // RS-3 A2 — a live Z was submitted while the full Z surface is not yet
        // implemented (z_builder::FULL_Z_SURFACE_READY == false, until W4-Z2):
        // capability-not-yet-implemented → 501, like NOT_IMPLEMENTED but
        // Z-specific (NOT a transient 503).
        "Z_SURFACE_NOT_READY" => 501,
        // RS-3 node-mode refusal — the node cannot fiscalize in its current
        // mode; not a client fault, not an internal fault, so 503 (service
        // unavailable until the node recovers / operator intervenes), distinct
        // from the 5xx internal-fault bucket.  M1: OfflineRefused carries the
        // PRECISE node-mode code (GOING_ONLINE is retryable, BLOCKED/STOP_MODE
        // need operator action) — all 503; OFFLINE_REFUSED kept as the generic.
        "OFFLINE_REFUSED"
        | "NODE_BLOCKED"
        | "NODE_STOP_MODE"
        | "NODE_CRYPTO_DEGRADED"
        | "NODE_GOING_ONLINE"
        // A.3 PR-C (D5 gate) — an older non-issued sibling rests on the FN;
        // transient RETRYABLE refusal (the online_convergence resolver
        // re-drives the blocker, then the client retries), NOT a 5xx breach.
        | "WRITE_GATE_SIBLING_PENDING" => 503,
        // ledger/internal faults → 500.  SIGN_FAILED is the RS-3 sign-path
        // fault (a real crypto-operation failure, not a node-mode shift).
        // RS-3 A2 (T2/T3): `Internal{code}` carries a structural/runtime-breach
        // code — SHIFT_MANUAL_RECON is the operator-named manual-recon one;
        // every other Internal code (the SignError-non-Crypto / StageSendError /
        // BuildReject / shift-invariant codes) also resolves to 500 via the
        // `_ => 500` fallback, so the map stays total over the Internal bucket.
        "INBOX_LEDGER_DRIFT"
        | "LEDGER_CORRUPTION"
        | "LEDGER_READ_FAILED"
        | "PAYMENT_LOOKUP_FAILED"
        | "NOT_SIGNABLE"
        | "SIGN_FAILED"
        | "SHIFT_MANUAL_RECON"
        | "INTERNAL" => 500,
        _ => 500,
    }
}

/// `ZReport` / `ShiftClose` — the Z-class that must NOT be pre-aggregated
/// through [`convert`](super::convert) at ingress (disposition 2).
fn is_z_class(ct: CommandType) -> bool {
    matches!(ct, CommandType::ShiftClose | CommandType::ZReport)
}

/// Wire string for the inbox `operation_type` column — explicit per
/// variant (no serde reflection in the hot path).
fn command_type_wire(ct: CommandType) -> &'static str {
    match ct {
        CommandType::Sell => "SELL",
        CommandType::Return => "RETURN",
        CommandType::ShiftOpen => "SHIFT_OPEN",
        CommandType::ShiftClose => "SHIFT_CLOSE",
        CommandType::XReport => "X_REPORT",
        CommandType::ZReport => "Z_REPORT",
        CommandType::ServiceIn => "SERVICE_IN",
        CommandType::ServiceOut => "SERVICE_OUT",
        CommandType::CashWithdrawal => "CASH_WITHDRAWAL",
        CommandType::PeriodicReport => "PERIODIC_REPORT",
    }
}

fn err(request_id_hex: &str, error_code: &str, message: String) -> IngressResponse {
    IngressResponse {
        http_status: http_status_for_error_code(error_code),
        body: IngressBody::Error(CanonicalErrorResponse {
            ok: false,
            request_id: request_id_hex.to_string(),
            schema_version: SCHEMA_VERSION.to_string(),
            error_code: error_code.to_string(),
            error_message: message,
            config_drift: false,
        }),
    }
}

/// Wrap a pre-built error envelope (from the replay resolver / conflict),
/// deriving the status from its own `error_code`.
fn wrap_error(e: CanonicalErrorResponse) -> IngressResponse {
    IngressResponse {
        http_status: http_status_for_error_code(&e.error_code),
        body: IngressBody::Error(e),
    }
}

fn hex32(b: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(64);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

/// Stable error_code for a [`MappingError`] — exhaustive.
fn mapping_error_code(e: &MappingError) -> &'static str {
    match e {
        MappingError::FnConfigMismatch { .. } => "FN_MISMATCH",
        MappingError::SchemaVersionMismatch { .. } => "SCHEMA_VERSION_MISMATCH",
        MappingError::UnsupportedCommandType(_) => "UNSUPPORTED_COMMAND_TYPE",
        MappingError::InvalidCashierId(_) => "INVALID_CASHIER_ID",
        MappingError::CanonicalSerialise(_) => "INTERNAL",
    }
}

/// Stable error_code for a [`ConvertError`] — exhaustive.  Client payload
/// faults map to `422`, ledger/internal faults to `500` (via
/// [`http_status_for_error_code`]).
fn convert_error_code(e: &ConvertError) -> &'static str {
    match e {
        ConvertError::MissingItemCode { .. } => "MISSING_ITEM_CODE",
        ConvertError::EmptyGoods => "EMPTY_GOODS",
        ConvertError::ZeroQuantityLine { .. } => "ZERO_QUANTITY_LINE",
        ConvertError::RawFramesNotSupported { .. } => "RAW_FRAMES_UNSUPPORTED",
        ConvertError::SumNotDivisible { .. } => "SUM_NOT_DIVISIBLE",
        ConvertError::SecondaryTaxRequiresDualTaxMode { .. } => "SECONDARY_TAX_REQUIRES_DUAL_MODE",
        ConvertError::ValueOverflow { .. } => "VALUE_OVERFLOW",
        ConvertError::MissingPaymentMethod { .. } => "MISSING_PAYMENT_METHOD",
        ConvertError::InactivePaymentMethod { .. } => "INACTIVE_PAYMENT_METHOD",
        ConvertError::PaymentSlotKindMismatch { .. } => "PAYMENT_SLOT_KIND_MISMATCH",
        ConvertError::AcquirerSlipMappingDeferred { .. } => "ACQUIRER_SLIP_DEFERRED",
        ConvertError::NoOpenShiftForZReport { .. } => "NO_OPEN_SHIFT",
        ConvertError::NegativeStoredPaymentSum { .. } => "LEDGER_CORRUPTION",
        ConvertError::ZReportSumOverflow { .. } => "LEDGER_CORRUPTION",
        ConvertError::UnexpectedShiftReceiptDocType(_) => "LEDGER_CORRUPTION",
        ConvertError::LedgerRead(_) => "LEDGER_READ_FAILED",
        ConvertError::NotSignable(_) => "NOT_SIGNABLE",
        ConvertError::PaymentLookup(_) => "PAYMENT_LOOKUP_FAILED",
        ConvertError::Serialise(_) => "INTERNAL",
    }
}

/// Disposition of a *successful* `fiscalize` outcome, keyed on its durable
/// document state.
#[derive(Debug, PartialEq, Eq)]
enum OutcomeDisposition {
    /// Terminal success (`Ack` / `OfflineLocalAck`) → 200 success envelope
    /// (offline-ack carries a null `fiscal_id`).
    Done,
    /// In-flight (`Prepared`..`Kvt2`, `ErrorRetryable`): the receipt is
    /// persisted and being driven to DPS → render the SAME `202 IN_PROGRESS`
    /// `CanonicalErrorResponse` the replay path emits (the `dto.rs`
    /// IN_PROGRESS contract: a blocking client switches on the 202 status and
    /// re-polls).  Keeping first-pass and replay on ONE 202 shape is the
    /// parity the DTO promises.
    InProgress,
    /// A terminal FAILURE state via `Ok` — a seam contract breach (failures
    /// must be `Err(FiscalError)`) → the caller audits + 500s it.
    Breach,
}

/// Classify a successful outcome's state.  Exhaustive over `DocState` (no
/// `_`) so a newly-added state forces a deliberate decision here.
fn classify_outcome(state: DocState) -> OutcomeDisposition {
    match state {
        DocState::Ack | DocState::OfflineLocalAck => OutcomeDisposition::Done,
        DocState::Prepared
        | DocState::Signed
        | DocState::Encrypted
        | DocState::Sending
        | DocState::Sent
        | DocState::Kvt1
        | DocState::Kvt2
        | DocState::ErrorRetryable => OutcomeDisposition::InProgress,
        DocState::Rejected
        | DocState::Cancelled
        | DocState::RequiresManualReconciliation
        // Aborted is a non-issued terminal FAILURE — a refusal always returns
        // Err(FiscalError), so an Aborted document_state on an Ok outcome is a
        // seam-contract breach (same class as Rejected/Cancelled).
        | DocState::Aborted => OutcomeDisposition::Breach,
    }
}

/// Build the first-pass success envelope (HTTP 200) from the seam outcome.
/// Only a TERMINAL-success state reaches here ([`OutcomeDisposition::Done`]);
/// an in-flight outcome is rendered as the shared `202 IN_PROGRESS` error
/// envelope (parity with replay), not a success body.  The receipt totals
/// come from the wire `Totals` (truthful: it is what was fiscalized),
/// matching the replay path's ledger-derived totals so first-pass↔replay
/// responses agree.
fn build_success(
    request_id_hex: &str,
    command_type: CommandType,
    totals: &Totals,
    fo: &FiscalOutcome,
) -> IngressResponse {
    let (sale_total_kopecks, return_total_kopecks) = match command_type {
        CommandType::Sell => (totals.sale_kopecks, 0),
        CommandType::Return => (0, totals.return_kopecks),
        _ => (0, 0),
    };
    IngressResponse {
        http_status: 200,
        body: IngressBody::Success(CanonicalResponse {
            ok: true,
            request_id: request_id_hex.to_string(),
            schema_version: SCHEMA_VERSION.to_string(),
            // DocumentId is a 16-byte uuid; render lowercase-32-hex — the
            // SAME shape the replay path emits via SQL `lower(hex(...))`.
            document_id: request_id_to_string(fo.document_id.as_bytes()),
            fiscal_id: fo.fiscal_id.clone(),
            fiscal_ts: fo.fiscal_ts.clone(),
            document_state: fo.document_state.as_str().to_string(),
            sale_total_kopecks,
            return_total_kopecks,
            report_xml: fo.report_xml.clone(),
        }),
    }
}

/// Handle one ingress command end-to-end (accept → seam → response).
///
/// `listener_fn` / `listener_driver_id` / `protocol` are the per-listener
/// identity (RS-2 piece-1a/W4-Z0); piece-5b supplies them from the
/// listener config.  `write_path` is the inline seam — [`UnimplementedWritePath`]
/// pre-RS-3, the real worker after.
///
/// [`UnimplementedWritePath`]: super::seam::UnimplementedWritePath
#[allow(clippy::too_many_arguments)]
pub async fn handle_command(
    cmd: &CanonicalCommand,
    listener_fn: &str,
    listener_driver_id: DriverId,
    protocol: Protocol,
    main_pool: &SqlitePool,
    secure_pool: &SqlitePool,
    write_path: &dyn WritePathEntry,
) -> IngressResponse {
    // Server-minted request id — the inbox identity for this submission
    // (the wire carries only `idempotency_key`).  Used for every error
    // response built before the inbox insert.
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let rid_hex = request_id_to_string(&request_id);

    // Step 0 — command-class policy (piece-1b), BEFORE any inbox write.
    match classify_command(cmd.command_type) {
        CommandClass::Signable => {}
        CommandClass::ReadOnly => {
            return err(
                &rid_hex,
                "READ_ONLY_COMMAND",
                "read-only command is not supported by fiscal POST".to_string(),
            );
        }
        CommandClass::Unsupported => {
            return err(
                &rid_hex,
                "UNSUPPORTED_COMMAND",
                format!(
                    "command_type {:?} is not supported by the RS-2 fiscal pipeline",
                    cmd.command_type
                ),
            );
        }
    }

    // Step 1 — map + validate (schema_version / listener-FN match / cashier).
    let canonical =
        match to_canonical_fiscal_command_with_context(cmd, listener_driver_id, listener_fn) {
            Ok(c) => c,
            Err(e) => return err(&rid_hex, mapping_error_code(&e), e.to_string()),
        };

    // Step 2 — payload for the inbox.
    //   Z-class → WIRE INTENT (disposition 2): do NOT aggregate here.
    //   non-Z signable → converted signer-ready shape + recomputed hash.
    let (payload_json, payload_sha256_canonical) = if is_z_class(cmd.command_type) {
        (
            canonical.payload_json.clone(),
            canonical.payload_sha256_canonical,
        )
    } else {
        match convert_to_signer_payload(cmd, listener_fn, main_pool, secure_pool).await {
            Ok(cp) => (cp.payload_json, cp.payload_sha256_canonical),
            Err(e) => return err(&rid_hex, convert_error_code(&e), e.to_string()),
        }
    };

    // Step 3 — idempotent inbox insert.  A-H1: persist the recovery
    // identity so the row is self-contained for the seam + a crash-recovery
    // reaper — `driver_id` from the listener-stamped config (the mapper
    // stored it into `canonical.driver_id`; always `Some` here), and
    // `signed_by_cashier_id` from the VALIDATED command (not raw wire;
    // legitimately `None` when the command has no cashier).
    let entry = NewInboxEntry {
        request_id,
        fiscal_number: listener_fn.to_string(),
        protocol,
        operation_type: command_type_wire(cmd.command_type).to_string(),
        idempotency_key: cmd.idempotency_key.clone(),
        payload_json,
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: canonical
            .signed_by_cashier_id
            .as_ref()
            .map(|c| c.as_str().to_string()),
        driver_id: canonical.driver_id.as_ref().map(|d| d.as_str().to_string()),
        // A-H1 follow-up (022): the receipt timestamp (always present) + the
        // wire's declared total (None for SHIFT_OPEN / Z), so the reaper drives
        // the write-path from the row without re-minting `now()` or losing the
        // stage_sign sum cross-check.
        business_ts: Some(canonical.business_ts.clone()),
        total_sum_kop: canonical.total_sum_kop,
    };

    let outcome = match ingress_inbox::insert(main_pool, &entry).await {
        Ok(o) => o,
        Err(_) => {
            return err(&rid_hex, "INTERNAL", "inbox insert failed".to_string());
        }
    };

    match outcome {
        // First time seen — fiscalize inline through the seam.
        InboxInsertOutcome::Created(row) => match write_path.fiscalize(&row).await {
            Ok(fo) => match classify_outcome(fo.document_state) {
                // Terminal success (Ack / OfflineLocalAck) → 200 success body.
                OutcomeDisposition::Done => {
                    build_success(&rid_hex, cmd.command_type, &cmd.payload.totals, &fo)
                }
                // In-flight → the SAME `202 IN_PROGRESS` error envelope the
                // replay path emits (dto.rs IN_PROGRESS contract + first-pass↔
                // replay parity: ONE 202 shape across the first POST and every
                // re-poll).  The client correlates by `request_id` (carried in
                // the envelope) and switches on the 202 status; document_id is
                // intentionally NOT surfaced here — adding it would be a
                // deliberate change to the error DTO + replay together.
                OutcomeDisposition::InProgress => err(
                    &rid_hex,
                    "IN_PROGRESS",
                    "the submission is still being processed; retry".to_string(),
                ),
                // A terminal FAILURE state via `Ok` is a seam contract breach
                // (failures must be `Err(FiscalError)`).  Audit + 500 rather
                // than mint a phantom 200/202 success.
                OutcomeDisposition::Breach => {
                    let _ = audit_log::append(
                        main_pool,
                        "ingress_inbox",
                        &rid_hex,
                        "SEAM_OK_WITH_FAILURE_STATE",
                        Severity::Warning,
                        None,
                        Some(
                            &serde_json::json!({
                                "document_state": fo.document_state.as_str(),
                            })
                            .to_string(),
                        ),
                    )
                    .await;
                    err(
                        &rid_hex,
                        "INTERNAL",
                        "write-path returned a terminal failure state as a success".to_string(),
                    )
                }
            },
            // Exhaustive (no `_`): each RS-3 failure variant decides its own
            // delete-vs-persist policy below.
            Err(fe) => match fe {
                FiscalError::NotImplemented { request_id } => {
                    // Defense-in-depth (operator review): the handler OWNS the
                    // persisted `row`, so the destructive release keys off
                    // `row.request_id`, NEVER the id echoed back in the seam
                    // error.  A buggy or future seam impl returning a
                    // MISMATCHED id must not be able to delete a DIFFERENT
                    // submission's NEW row (the `WHERE status='NEW'` guard
                    // blocks PROCESSING/DONE, but not wrong-identity).  The
                    // error's `request_id` is used ONLY to detect/audit a seam
                    // contract breach.
                    if request_id != row.request_id {
                        tracing::warn!(
                            row_request_id = %request_id_to_string(&row.request_id),
                            seam_request_id = %request_id_to_string(&request_id),
                            "write-path seam returned a request_id that does not match the inbox \
                             row; releasing by the row's id (defense-in-depth)"
                        );
                        let _ = audit_log::append(
                            main_pool,
                            "ingress_inbox",
                            &rid_hex,
                            "SEAM_REQUEST_ID_MISMATCH",
                            Severity::Warning,
                            None,
                            Some(
                                &serde_json::json!({
                                    "row_request_id": request_id_to_string(&row.request_id),
                                    "seam_request_id": request_id_to_string(&request_id),
                                })
                                .to_string(),
                            ),
                        )
                        .await;
                    }
                    // Disposition 1 — release the still-NEW row so a retry
                    // re-attempts (guarded `WHERE status='NEW'`; can never
                    // drop a durable row).  The release is LOAD-BEARING: if
                    // it does not happen, every retry of this idempotency_key
                    // resolves to a `202 IN_PROGRESS` replay forever (NEW
                    // inbox row + no fiscal doc).  So a failed or unexpected
                    // release MUST be observable (audit), never swallowed —
                    // under the multi-FN concurrent load a transient
                    // `SQLITE_BUSY` could otherwise wedge a receipt silently.
                    match ingress_inbox::delete_new_by_request_id(main_pool, &row.request_id).await
                    {
                        Ok(1) => {}
                        Ok(rows) => {
                            if let Err(ae) = audit_log::append(
                                main_pool,
                                "ingress_inbox",
                                &rid_hex,
                                "INGRESS_NEW_ROW_RELEASE_UNEXPECTED",
                                Severity::Warning,
                                None,
                                Some(&serde_json::json!({ "rows_affected": rows }).to_string()),
                            )
                            .await
                            {
                                tracing::warn!(
                                    request_id = %rid_hex,
                                    rows_affected = rows,
                                    audit_err = %ae,
                                    "inbox NEW-row release affected an unexpected row count; \
                                     its audit append also failed"
                                );
                            }
                        }
                        Err(e) => {
                            // review round-2 Med-1: the audit append lives on
                            // the SAME `main_pool` and so shares the very
                            // SQLITE_BUSY that just failed the DELETE.  If both
                            // fail, fall back to an OUT-OF-BAND trace (no DB) so
                            // the (potentially 202-wedged) receipt is observable
                            // even when the DB is unwritable.
                            if let Err(ae) = audit_log::append(
                                main_pool,
                                "ingress_inbox",
                                &rid_hex,
                                "INGRESS_NEW_ROW_RELEASE_FAILED",
                                Severity::Warning,
                                None,
                                Some(&serde_json::json!({ "error": e.to_string() }).to_string()),
                            )
                            .await
                            {
                                tracing::error!(
                                    request_id = %rid_hex,
                                    db_err = %e,
                                    audit_err = %ae,
                                    "inbox NEW-row release FAILED and its audit append also failed \
                                     (DB contention) — receipt may wedge at IN_PROGRESS until the \
                                     RS-3 reaper sweeps it"
                                );
                            }
                        }
                    }
                    err(
                        &rid_hex,
                        "NOT_IMPLEMENTED",
                        "write-path not yet implemented (RS-3 pending)".to_string(),
                    )
                }

                // RS-3 real fiscal failures.  Unlike `NotImplemented` (where
                // nothing durable happened, so the NEW row is RELEASED to let
                // a retry re-attempt), these come back AFTER `fiscalize` has
                // taken the per-FN lease (the inbox row is already PROCESSING,
                // not NEW) and written whatever durable state the persistence
                // pin requires.  The handler therefore does NOT release the
                // row (a `WHERE status='NEW'` delete would no-op anyway) and
                // does NOT re-audit (the underlying cause is audited inside
                // `fiscalize`) — it only translates the typed refusal to its
                // HTTP envelope.
                FiscalError::ShiftNotOpen { .. } => err(
                    &rid_hex,
                    "NO_OPEN_SHIFT",
                    "no open shift for this fiscal number".to_string(),
                ),
                FiscalError::SignFailure { .. } => err(
                    &rid_hex,
                    "SIGN_FAILED",
                    "signing the receipt failed".to_string(),
                ),
                FiscalError::DpsRejected { .. } => err(
                    &rid_hex,
                    "FISCAL_REJECTED",
                    "DPS rejected the receipt".to_string(),
                ),
                // M1 — the precise node-mode `code` IS the error_code (all → 503).
                FiscalError::OfflineRefused { code, .. } => err(
                    &rid_hex,
                    code,
                    format!("node refused fiscalization ({code})"),
                ),
                FiscalError::ZSurfaceNotReady { .. } => err(
                    &rid_hex,
                    "Z_SURFACE_NOT_READY",
                    "live Z fiscalization is not yet enabled (pending the full Z surface)"
                        .to_string(),
                ),
                // T1/T2 — the specific stable `code` IS the error_code (the
                // http map routes ShiftGuardRefused codes → 422, Internal codes
                // → 500).  Keyed off `rid_hex` (the owned id) like every real
                // failure; the carried `request_id` is unused here per the A3
                // defense-in-depth.
                FiscalError::ShiftGuardRefused { code, .. } => {
                    err(&rid_hex, code, format!("shift guard refused ({code})"))
                }
                FiscalError::Internal { code, .. } => err(
                    &rid_hex,
                    code,
                    format!("internal write-path breach ({code})"),
                ),
            },
        },

        // Same (fn, idem_key, payload) already seen — resolve truthfully
        // against the ledger (piece-4b), NEVER re-fiscalize.
        InboxInsertOutcome::Replay(row) => match resolve_replay(&row, main_pool).await {
            Ok(ReplayResolution::Completed(r)) => IngressResponse {
                http_status: 200,
                body: IngressBody::Success(r),
            },
            Ok(ReplayResolution::InProgress(e)) => wrap_error(e),
            Ok(ReplayResolution::Failed(e)) => wrap_error(e),
            Err(_) => err(&rid_hex, "INTERNAL", "replay resolution failed".to_string()),
        },

        // Same idem_key, DIFFERENT payload — MED-2 config-drift conflict.
        // Response references the PERSISTED id; audit records both ids +
        // both hashes (best-effort: an audit error must not mask the 409).
        InboxInsertOutcome::Conflict {
            existing_request_id,
            existing_payload_hash,
            submitted_payload_hash,
        } => {
            let e = conflict_response(&existing_request_id);
            let existing_hex = request_id_to_string(&existing_request_id);
            let submitted_hex = request_id_to_string(&request_id);
            let existing_sha = hex32(&existing_payload_hash);
            let submitted_sha = hex32(&submitted_payload_hash);
            let audit_payload = serde_json::json!({
                "existing_request_id": existing_hex,
                "submitted_request_id": submitted_hex,
                "existing_payload_sha256": existing_sha,
                "submitted_payload_sha256": submitted_sha,
            })
            .to_string();
            // An audit error must not mask the 409 — but it must not vanish
            // silently either (operator review): the forensic pair
            // (existing/submitted ids + hashes) is the whole point of the
            // conflict record, so fall back to an out-of-band trace.
            if let Err(ae) = audit_log::append(
                main_pool,
                "ingress_inbox",
                &e.request_id,
                "IDEMPOTENCY_CONFLICT",
                Severity::Warning,
                None,
                Some(&audit_payload),
            )
            .await
            {
                tracing::warn!(
                    existing_request_id = %existing_hex,
                    submitted_request_id = %submitted_hex,
                    existing_payload_sha256 = %existing_sha,
                    submitted_payload_sha256 = %submitted_sha,
                    audit_err = %ae,
                    "idempotency conflict audit append failed; forensic pair traced out-of-band"
                );
            }
            wrap_error(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::enums::{DocState, FiscalMode};
    use crate::db::models::ids::DocumentId;
    use crate::db::repositories::fiscal_number_config::{insert as fn_insert, NewFnConfig};
    use crate::db::repositories::ingress_inbox::InboxRow;
    use crate::db::repositories::payment_methods::{self, NewPaymentMethod};
    use crate::db::{open_pool, open_secure_pool};
    use crate::runtime::ingress::seam::UnimplementedWritePath;
    use std::sync::Mutex;

    const FN: &str = "4000000001";

    async fn fresh_pools() -> (tempfile::TempDir, SqlitePool, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let main = open_pool(&dir.path().join("main.db")).await.unwrap();
        let secure = open_secure_pool(&dir.path().join("secure.db"))
            .await
            .unwrap();
        fn_insert(
            &main,
            &NewFnConfig {
                fiscal_number: FN.to_string(),
                tax_number: "12345678".to_string(),
                vat_payer_inn: None,
                fiscal_mode: FiscalMode::Test,
                org_name: None,
                point_name: None,
                org_address: None,
                tsp_enabled: false,
                offline_enabled: true,
                national_check_enabled: false,
                min_offline_codes: 0,
                max_offline_codes: 0,
            },
        )
        .await
        .unwrap();
        (dir, main, secure)
    }

    fn drv() -> DriverId {
        DriverId::new("drv-1").unwrap()
    }

    fn parse(json: String) -> CanonicalCommand {
        serde_json::from_str(&json).expect("parse fixture")
    }

    fn sell_cmd(idem: &str, price_kopecks: u64) -> CanonicalCommand {
        parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SELL",
                "idempotency_key":"{idem}","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE",
                  "goods":[{{"name":"Bread","quantity_milli":1000,"price_kopecks":{price_kopecks},
                            "tax_group_1":0,"tax_group_2":0,"article_code":42}}],
                  "payments":[],
                  "totals":{{"sale_kopecks":{price_kopecks},"return_kopecks":0}}}}}}"#
        ))
    }

    fn zreport_cmd(idem: &str) -> CanonicalCommand {
        parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"Z_REPORT",
                "idempotency_key":"{idem}","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
        ))
    }

    fn shift_open_cmd(idem: &str) -> CanonicalCommand {
        parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SHIFT_OPEN",
                "idempotency_key":"{idem}","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
        ))
    }

    fn xreport_cmd(idem: &str) -> CanonicalCommand {
        parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"X_REPORT",
                "idempotency_key":"{idem}","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
        ))
    }

    fn service_in_cmd(idem: &str) -> CanonicalCommand {
        parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SERVICE_IN",
                "idempotency_key":"{idem}","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
        ))
    }

    /// A SELL with one CASH payment (drives the `payment_methods`-backed
    /// convert path, so the converted hash includes the slot `name`).
    fn sell_cash_cmd(idem: &str) -> CanonicalCommand {
        parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SELL",
                "idempotency_key":"{idem}","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE",
                  "goods":[{{"name":"Bread","quantity_milli":1000,"price_kopecks":10000,
                            "tax_group_1":0,"tax_group_2":0,"article_code":42}}],
                  "payments":[{{"type":"CASH","amount_kopecks":10000}}],
                  "totals":{{"sale_kopecks":10000,"return_kopecks":0}}}}}}"#
        ))
    }

    /// A test seam that captures the inbox row it was handed and returns a
    /// fixed successful outcome — lets us exercise the success / replay /
    /// conflict branches (the production [`UnimplementedWritePath`] can
    /// only ever fail).
    struct RecordingOk {
        captured: Mutex<Option<InboxRow>>,
        out: FiscalOutcome,
    }

    #[async_trait::async_trait]
    impl WritePathEntry for RecordingOk {
        async fn fiscalize(&self, row: &InboxRow) -> Result<FiscalOutcome, FiscalError> {
            *self.captured.lock().unwrap() = Some(row.clone());
            Ok(self.out.clone())
        }
    }

    /// A buggy/hostile seam that returns `NotImplemented` with a request_id
    /// that does NOT belong to the row it was handed — exercises the
    /// defense-in-depth that the release keys off `row.request_id`.
    struct WrongIdNotImplemented {
        wrong_id: [u8; 16],
    }

    #[async_trait::async_trait]
    impl WritePathEntry for WrongIdNotImplemented {
        async fn fiscalize(&self, _row: &InboxRow) -> Result<FiscalOutcome, FiscalError> {
            Err(FiscalError::NotImplemented {
                request_id: self.wrong_id,
            })
        }
    }

    /// Which RS-3 *real* failure a [`RecordingErr`] seam emits (built from the
    /// owned `row.request_id`, since `FiscalError` is not `Clone`).
    #[derive(Clone, Copy)]
    enum ErrKind {
        ShiftNotOpen,
        SignFailure,
        DpsRejected,
        OfflineRefused(&'static str),
        ZSurfaceNotReady,
        ShiftGuardRefused(&'static str),
        Internal(&'static str),
    }

    /// A seam that captures the inbox row and returns a chosen real fiscal
    /// failure WITHOUT taking the lease (so the row stays NEW) — lets us
    /// assert the handler maps each variant to its HTTP status AND does NOT
    /// release the row (unlike `NotImplemented`).
    struct RecordingErr {
        captured: Mutex<Option<InboxRow>>,
        kind: ErrKind,
    }

    #[async_trait::async_trait]
    impl WritePathEntry for RecordingErr {
        async fn fiscalize(&self, row: &InboxRow) -> Result<FiscalOutcome, FiscalError> {
            *self.captured.lock().unwrap() = Some(row.clone());
            let request_id = row.request_id;
            Err(match self.kind {
                ErrKind::ShiftNotOpen => FiscalError::ShiftNotOpen { request_id },
                ErrKind::SignFailure => FiscalError::SignFailure { request_id },
                ErrKind::DpsRejected => FiscalError::DpsRejected { request_id },
                ErrKind::OfflineRefused(code) => FiscalError::OfflineRefused { request_id, code },
                ErrKind::ZSurfaceNotReady => FiscalError::ZSurfaceNotReady { request_id },
                ErrKind::ShiftGuardRefused(code) => {
                    FiscalError::ShiftGuardRefused { request_id, code }
                }
                ErrKind::Internal(code) => FiscalError::Internal { request_id, code },
            })
        }
    }

    fn ack_outcome() -> FiscalOutcome {
        FiscalOutcome {
            document_id: DocumentId::new(),
            fiscal_id: Some("777001".to_string()),
            fiscal_ts: Some("2026-06-06T12:00:00Z".to_string()),
            document_state: DocState::Ack,
            report_xml: None,
        }
    }

    fn offline_outcome() -> FiscalOutcome {
        FiscalOutcome {
            document_id: DocumentId::new(),
            fiscal_id: None,
            fiscal_ts: None,
            document_state: DocState::OfflineLocalAck,
            report_xml: None,
        }
    }

    /// An in-flight outcome (the receipt is persisted + being driven to DPS):
    /// the handler renders `202 IN_PROGRESS`, fiscal_id still unknown.
    fn sending_outcome() -> FiscalOutcome {
        FiscalOutcome {
            document_id: DocumentId::new(),
            fiscal_id: None,
            fiscal_ts: None,
            document_state: DocState::Sending,
            report_xml: None,
        }
    }

    /// RS-2 A-H1 — the inbox row handed to the seam is a SELF-CONTAINED
    /// recovery record: `driver_id` from the listener-stamped config and
    /// `signed_by_cashier_id` from the VALIDATED command (not the raw wire).
    /// Captured at the seam boundary (a `RecordingOk` returns Ok so the NEW
    /// row is not released), which is exactly what RS-3 / a crash-recovery
    /// reaper will read.
    #[tokio::test]
    async fn handler_created_row_carries_listener_driver_and_validated_cashier() {
        let (_d, main, secure) = fresh_pools().await;
        // Seed the CASH slot so the SELL converts to a signer-ready payload.
        payment_methods::insert(
            &secure,
            &NewPaymentMethod {
                fn_id: FN.to_string(),
                pay_index: 1,
                name: "Готівка".to_string(),
                iscash: true,
            },
        )
        .await
        .unwrap();

        let seam = RecordingOk {
            captured: Mutex::new(None),
            out: ack_outcome(),
        };
        // A SELL carrying an explicit cashier "K-7".
        let cmd = parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SELL",
                "idempotency_key":"idem-id","cashier_id":"K-7","department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE",
                  "goods":[{{"name":"Bread","quantity_milli":1000,"price_kopecks":10000,
                            "tax_group_1":0,"tax_group_2":0,"article_code":42}}],
                  "payments":[{{"type":"CASH","amount_kopecks":10000}}],
                  "totals":{{"sale_kopecks":10000,"return_kopecks":0}}}}}}"#
        ));
        let r = handle_command(&cmd, FN, drv(), Protocol::Rest, &main, &secure, &seam).await;
        assert_eq!(r.http_status, 200, "Created → Ok seam → success");

        let row = seam
            .captured
            .lock()
            .unwrap()
            .clone()
            .expect("the seam must have received the persisted inbox row");
        assert_eq!(
            row.driver_id.as_deref(),
            Some("drv-1"),
            "driver_id MUST be the listener-stamped value (drv())"
        );
        assert_eq!(
            row.signed_by_cashier_id.as_deref(),
            Some("K-7"),
            "signed_by_cashier_id MUST be the VALIDATED command's cashier"
        );
        // A-H1 follow-up (022): the row also carries the receipt timestamp +
        // the wire's declared total, so the reaper need not re-mint now() nor
        // lose the stage_sign sum cross-check.
        assert!(
            row.business_ts.as_deref().is_some_and(|s| !s.is_empty()),
            "business_ts MUST be persisted (non-empty), got {:?}",
            row.business_ts
        );
        assert_eq!(
            row.total_sum_kop,
            Some(10000),
            "total_sum_kop MUST be the SELL's declared sale total"
        );
    }

    /// ACCEPTANCE #1 — a `NotImplemented` seam failure RELEASES the NEW
    /// inbox row so a retry RE-ATTEMPTS (Created→seam again), instead of
    /// the row sticking and every retry resolving to a `202 IN_PROGRESS`
    /// replay forever.
    #[tokio::test]
    async fn retry_after_not_implemented_reattempts_not_stuck() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = UnimplementedWritePath;
        let cmd = shift_open_cmd("idem-retry");

        let r1 = handle_command(&cmd, FN, drv(), Protocol::Rest, &main, &secure, &wp).await;
        assert_eq!(r1.http_status, 501, "pre-RS-3 seam → NOT_IMPLEMENTED");
        let c1: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox WHERE idempotency_key = ?")
                .bind("idem-retry")
                .fetch_one(&main)
                .await
                .unwrap();
        assert_eq!(c1, 0, "NotImplemented must release the NEW row");

        let r2 = handle_command(&cmd, FN, drv(), Protocol::Rest, &main, &secure, &wp).await;
        assert_eq!(
            r2.http_status, 501,
            "retry re-attempts (501), it must NOT become a stuck 202 replay"
        );
    }

    /// Defense-in-depth: the `NotImplemented` release keys off the OWNED
    /// `row.request_id`, NOT the id echoed in the seam error.  A buggy seam
    /// returning a DIFFERENT (existing) request_id must release only the
    /// current submission's row and leave the other one intact — and audit
    /// the contract breach.
    #[tokio::test]
    async fn release_deletes_only_owned_row_not_wrong_seam_id() {
        let (_d, main, secure) = fresh_pools().await;
        // A victim NEW row from an unrelated submission.
        let victim_id = [0x11u8; 16];
        ingress_inbox::insert(
            &main,
            &NewInboxEntry {
                request_id: victim_id,
                fiscal_number: FN.to_string(),
                protocol: Protocol::Rest,
                operation_type: "SELL".to_string(),
                idempotency_key: "victim".to_string(),
                payload_json: "{}".to_string(),
                payload_sha256_canonical: [0xAAu8; 32],
                correlation_id: None,
                signed_by_cashier_id: None,
                driver_id: Some("drv-test".to_string()),
                business_ts: None,
                total_sum_kop: None,
            },
        )
        .await
        .unwrap();

        // A seam that echoes the VICTIM's id on NotImplemented.
        let wp = WrongIdNotImplemented {
            wrong_id: victim_id,
        };
        let r = handle_command(
            &shift_open_cmd("current"),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r.http_status, 501);

        let current_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox WHERE idempotency_key = ?")
                .bind("current")
                .fetch_one(&main)
                .await
                .unwrap();
        assert_eq!(current_rows, 0, "the handler must release its OWN NEW row");

        let victim_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox WHERE request_id = ?")
                .bind(&victim_id[..])
                .fetch_one(&main)
                .await
                .unwrap();
        assert_eq!(
            victim_rows, 1,
            "a wrong seam request_id must NOT delete another submission's row"
        );

        let mismatch: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE event_type = 'SEAM_REQUEST_ID_MISMATCH'",
        )
        .fetch_one(&main)
        .await
        .unwrap();
        assert_eq!(mismatch, 1, "the seam id mismatch must be audited");
    }

    /// ACCEPTANCE #2 — a Z (`ZReport`/`ShiftClose`) is stored as WIRE
    /// INTENT, NOT pre-aggregated through `convert` into a `ZReportJson`.
    /// The seam input (== the inbox row) must carry the wire shape.
    #[tokio::test]
    async fn z_class_stores_wire_intent_not_aggregated_zreport() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: ack_outcome(),
        };
        let r = handle_command(
            &zreport_cmd("idem-z"),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r.http_status, 200);

        let captured = wp
            .captured
            .lock()
            .unwrap()
            .clone()
            .expect("seam must have been called");
        assert!(
            captured.payload_json.contains("totals"),
            "wire intent keeps the wire shape: {}",
            captured.payload_json
        );
        assert!(
            !captured.payload_json.contains("sell_count"),
            "must NOT be aggregated ZReportJson (no sell_count): {}",
            captured.payload_json
        );
        assert!(
            !captured.payload_json.contains("sum_in_kop"),
            "must NOT be aggregated ZReportJson (no sum_in_kop): {}",
            captured.payload_json
        );
        // Persisted inbox row (seam returned Ok → not deleted) agrees.
        let stored: String =
            sqlx::query_scalar("SELECT payload_json FROM ingress_inbox WHERE idempotency_key = ?")
                .bind("idem-z")
                .fetch_one(&main)
                .await
                .unwrap();
        assert!(
            !stored.contains("sell_count"),
            "persisted wire intent: {stored}"
        );
    }

    /// ACCEPTANCE #3 — an idempotency `Conflict` response references the
    /// PERSISTED (original) request id, and the audit records BOTH request
    /// ids + BOTH payload hashes.
    #[tokio::test]
    async fn conflict_references_persisted_id_and_audits_both() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: ack_outcome(),
        };
        // First submission persists (seam Ok → NEW row stays).
        let r1 = handle_command(
            &sell_cmd("idem-c", 10000),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r1.http_status, 200);
        let persisted_hex =
            request_id_to_string(&wp.captured.lock().unwrap().clone().unwrap().request_id);

        // Same idem, DIFFERENT payload → Conflict.
        let r2 = handle_command(
            &sell_cmd("idem-c", 20000),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r2.http_status, 409);
        match r2.body {
            IngressBody::Error(e) => {
                assert_eq!(e.error_code, "IDEMPOTENCY_CONFLICT");
                assert!(e.config_drift, "MED-2 — config_drift, not tampering");
                assert_eq!(
                    e.request_id, persisted_hex,
                    "conflict response must reference the PERSISTED request id"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }

        let payload: String = sqlx::query_scalar(
            "SELECT event_payload_json FROM audit_log WHERE event_type = 'IDEMPOTENCY_CONFLICT' LIMIT 1",
        )
        .fetch_one(&main)
        .await
        .unwrap();
        assert!(
            payload.contains(&persisted_hex),
            "audit has existing id: {payload}"
        );
        assert!(payload.contains("submitted_request_id"), "{payload}");
        assert!(payload.contains("existing_payload_sha256"), "{payload}");
        assert!(payload.contains("submitted_payload_sha256"), "{payload}");
    }

    /// MED-2 (operator-locked option **a**): the idempotency key is the
    /// hash over the CONVERTED (signer-ready) payload, whose payment `name`
    /// is sourced from the editable `payment_methods` row.  A benign slot
    /// RENAME between a submit and its retry changes the fiscal input — so
    /// the retry of the SAME wire receipt is a `IDEMPOTENCY_CONFLICT`
    /// (config_drift, NOT tampering), audited with both ids + both hashes —
    /// NEVER a silent Replay of a now-stale payload (a silent Replay after
    /// a rename would return the old fiscal input, which is worse).
    #[tokio::test]
    async fn slot_rename_mid_retry_is_config_drift_conflict() {
        let (_d, main, secure) = fresh_pools().await;
        payment_methods::insert(
            &secure,
            &NewPaymentMethod {
                fn_id: FN.to_string(),
                pay_index: 1,
                name: "Готівка".to_string(),
                iscash: true,
            },
        )
        .await
        .unwrap();
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: ack_outcome(),
        };

        // First submit persists (seam Ok → NEW row; hash over name "Готівка").
        let r1 = handle_command(
            &sell_cash_cmd("idem-rename"),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r1.http_status, 200);
        let persisted_hex =
            request_id_to_string(&wp.captured.lock().unwrap().clone().unwrap().request_id);

        // Operator renames the CASH slot — a benign, allowed admin op.
        payment_methods::update(&secure, FN, 1, "Готівка UAH", true)
            .await
            .unwrap();

        // The SAME wire receipt, retried → converted hash now differs.
        let r2 = handle_command(
            &sell_cash_cmd("idem-rename"),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(
            r2.http_status, 409,
            "renamed-slot retry must Conflict, not silently Replay a stale payload"
        );
        match r2.body {
            IngressBody::Error(e) => {
                assert_eq!(e.error_code, "IDEMPOTENCY_CONFLICT");
                assert!(e.config_drift, "slot rename is config_drift, not tampering");
                assert_eq!(
                    e.request_id, persisted_hex,
                    "conflict references the persisted submission id"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
        let audit: String = sqlx::query_scalar(
            "SELECT event_payload_json FROM audit_log WHERE event_type = 'IDEMPOTENCY_CONFLICT' LIMIT 1",
        )
        .fetch_one(&main)
        .await
        .unwrap();
        assert!(
            audit.contains(&persisted_hex),
            "audit has existing id: {audit}"
        );
        assert!(audit.contains("submitted_request_id"), "{audit}");
        assert!(audit.contains("existing_payload_sha256"), "{audit}");
        assert!(audit.contains("submitted_payload_sha256"), "{audit}");
    }

    /// An offline-local-ack success serialises `fiscal_id: null` with
    /// `document_state = OFFLINE_LOCAL_ACK` and the wire sale total.
    #[tokio::test]
    async fn offline_success_has_null_fiscal_id() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: offline_outcome(),
        };
        let r = handle_command(
            &sell_cmd("idem-off", 15000),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r.http_status, 200);
        match r.body {
            IngressBody::Success(resp) => {
                assert!(resp.ok);
                assert_eq!(resp.fiscal_id, None);
                assert_eq!(resp.document_state, "OFFLINE_LOCAL_ACK");
                assert_eq!(resp.sale_total_kopecks, 15000);
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    /// A read-only command (`X_REPORT`) is rejected 422 and NEVER enters
    /// the fiscal inbox.
    #[tokio::test]
    async fn read_only_command_is_rejected_422() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = UnimplementedWritePath;
        let r = handle_command(
            &xreport_cmd("idem-x"),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r.http_status, 422);
        match r.body {
            IngressBody::Error(e) => assert_eq!(e.error_code, "READ_ONLY_COMMAND"),
            other => panic!("expected Error, got {other:?}"),
        }
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox")
            .fetch_one(&main)
            .await
            .unwrap();
        assert_eq!(c, 0, "read-only command must not enter the inbox");
    }

    /// An unsupported cash-movement command (`SERVICE_IN`) is rejected 422
    /// UNSUPPORTED_COMMAND at the boundary and NEVER enters the inbox (the
    /// "before any inbox write" claim, asserted at the HANDLER level — piece-7
    /// review Low; complements the policy-level classify test).
    #[tokio::test]
    async fn unsupported_command_is_rejected_422_without_inbox_row() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = UnimplementedWritePath;
        let r = handle_command(
            &service_in_cmd("idem-svc"),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r.http_status, 422);
        match r.body {
            IngressBody::Error(e) => assert_eq!(e.error_code, "UNSUPPORTED_COMMAND"),
            other => panic!("expected Error, got {other:?}"),
        }
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox")
            .fetch_one(&main)
            .await
            .unwrap();
        assert_eq!(c, 0, "an unsupported command must not enter the inbox");
    }

    /// A convert failure (SELL with empty goods → `EMPTY_GOODS`) is rejected
    /// 422 and, crucially, leaves NO inbox row — convert runs BEFORE the
    /// inbox insert, so a client-faulty payload can never plant a NEW row
    /// that a later retry would replay as `202 IN_PROGRESS`.
    #[tokio::test]
    async fn convert_error_rejected_422_without_inbox_row() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = UnimplementedWritePath;
        let cmd = parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SELL",
                "idempotency_key":"idem-bad","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE","goods":[],"payments":[],
                  "totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
        ));
        let r = handle_command(&cmd, FN, drv(), Protocol::Rest, &main, &secure, &wp).await;
        assert_eq!(r.http_status, 422);
        match r.body {
            IngressBody::Error(e) => assert_eq!(e.error_code, "EMPTY_GOODS"),
            other => panic!("expected Error(EMPTY_GOODS), got {other:?}"),
        }
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox")
            .fetch_one(&main)
            .await
            .unwrap();
        assert_eq!(
            c, 0,
            "a convert-rejected payload must not write an inbox row"
        );
    }

    /// External-review High-2 — a SHIFT_OPEN carrying non-empty `raw_frames`
    /// is rejected 422 RAW_FRAMES_UNSUPPORTED (the convert guard is hoisted
    /// ABOVE the doc-type match) and leaves NO inbox row, so two SHIFT_OPENs
    /// differing ONLY in raw_frames can't collapse to one `{opening_sum_kop:0}`
    /// converted payload + hash (an idempotency-key content collision).
    #[tokio::test]
    async fn shift_open_with_raw_frames_is_rejected_422_without_inbox_row() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = UnimplementedWritePath;
        let cmd = parse(format!(
            r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"SHIFT_OPEN",
                "idempotency_key":"idem-rf","cashier_id":null,"department":null,
                "return_check_number":null,
                "payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}},
                  "raw_frames":[{{"opcode":"DISC","body":"x"}}]}}}}"#
        ));
        let r = handle_command(&cmd, FN, drv(), Protocol::Rest, &main, &secure, &wp).await;
        assert_eq!(r.http_status, 422);
        match r.body {
            IngressBody::Error(e) => assert_eq!(e.error_code, "RAW_FRAMES_UNSUPPORTED"),
            other => panic!("expected Error(RAW_FRAMES_UNSUPPORTED), got {other:?}"),
        }
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox")
            .fetch_one(&main)
            .await
            .unwrap();
        assert_eq!(c, 0, "a raw_frames reject must not write an inbox row");
    }

    /// A `Replay` of the SAME payload while the ledger has no terminal doc
    /// resolves to `202 IN_PROGRESS` (deterministic retry, not a fake 200).
    #[tokio::test]
    async fn replay_same_payload_maps_in_progress_202() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: ack_outcome(),
        };
        let r1 = handle_command(
            &sell_cmd("idem-rp", 10000),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r1.http_status, 200);
        let r2 = handle_command(
            &sell_cmd("idem-rp", 10000),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(
            r2.http_status, 202,
            "replay with no terminal doc → IN_PROGRESS"
        );
        match r2.body {
            IngressBody::Error(e) => assert_eq!(e.error_code, "IN_PROGRESS"),
            other => panic!("expected Error(IN_PROGRESS), got {other:?}"),
        }
    }

    /// Every error_code the handler can emit maps to a NON-2xx (or the
    /// explicit 202 for IN_PROGRESS) — never a silent success.
    #[test]
    fn status_map_never_silent_2xx() {
        for code in [
            "IDEMPOTENCY_CONFLICT",
            "READ_ONLY_COMMAND",
            "UNSUPPORTED_COMMAND",
            "UNSUPPORTED_COMMAND_TYPE",
            "FN_MISMATCH",
            "SCHEMA_VERSION_MISMATCH",
            "INVALID_CASHIER_ID",
            "NOT_IMPLEMENTED",
            "INBOX_LEDGER_DRIFT",
            "INBOX_REJECTED",
            "FISCAL_REJECTED",
            "EMPTY_GOODS",
            "MISSING_ITEM_CODE",
            "NO_OPEN_SHIFT",
            "LEDGER_READ_FAILED",
            "INTERNAL",
            // adapter-shell codes now in the map (review-polish B-low).
            "UNKNOWN_SOURCE",
            "NO_NODE_STATE",
            "FN_FORBIDDEN",
            "MALFORMED_JSON",
            "SOME_UNKNOWN_CODE",
        ] {
            let s = http_status_for_error_code(code);
            assert!(s >= 400, "{code} mapped to non-error {s}");
        }
        // the adapter-shell codes' map status must equal their hard-coded
        // `server.rs::adapter_error` status (coherence; review-r2 LOW-1).
        assert_eq!(http_status_for_error_code("UNKNOWN_SOURCE"), 404);
        assert_eq!(http_status_for_error_code("NO_NODE_STATE"), 404);
        assert_eq!(http_status_for_error_code("FN_FORBIDDEN"), 403);
        assert_eq!(http_status_for_error_code("MALFORMED_JSON"), 400);
        assert_eq!(http_status_for_error_code("IN_PROGRESS"), 202);
        assert_eq!(http_status_for_error_code("IDEMPOTENCY_CONFLICT"), 409);
        assert_eq!(http_status_for_error_code("NOT_IMPLEMENTED"), 501);
        assert_eq!(http_status_for_error_code("INBOX_LEDGER_DRIFT"), 500);
        assert_eq!(http_status_for_error_code("SOME_UNKNOWN_CODE"), 500);
        // RS-3 A3 fiscal-failure codes.
        assert_eq!(http_status_for_error_code("NO_OPEN_SHIFT"), 422);
        assert_eq!(http_status_for_error_code("FISCAL_REJECTED"), 422);
        assert_eq!(http_status_for_error_code("SIGN_FAILED"), 500);
        assert_eq!(http_status_for_error_code("OFFLINE_REFUSED"), 503);
        // M1 — every precise node-mode code shares the 503 class.
        for code in [
            "NODE_BLOCKED",
            "NODE_STOP_MODE",
            "NODE_CRYPTO_DEGRADED",
            "NODE_GOING_ONLINE",
        ] {
            assert_eq!(http_status_for_error_code(code), 503, "{code}");
        }
        assert_eq!(http_status_for_error_code("Z_SURFACE_NOT_READY"), 501);
        // T1 — every ShiftGuardRefused code is a 422 (NOT collapsed to
        // NO_OPEN_SHIFT); + the Q-A signer-cashier 422 (carried by ShiftGuardRefused).
        for code in [
            "SHIFT_ALREADY_OPEN",
            "SHIFT_OPEN_PENDING_DRAIN",
            "POST_LOCAL_CLOSE_SALE_REFUSED",
            "OFFLINE_SHIFT_CLOSE_NOT_SUPPORTED",
            "SHIFT_CLOSING_IN_FLIGHT",
            "Z_REPORT_BACKLOG_DRAIN_PENDING",
            "SIGNER_CASHIER_MISMATCH",
        ] {
            assert_eq!(http_status_for_error_code(code), 422, "{code}");
        }
        // T2/T3 — Internal codes are 500 (SHIFT_MANUAL_RECON explicit; others via fallback).
        assert_eq!(http_status_for_error_code("SHIFT_MANUAL_RECON"), 500);
        assert_eq!(http_status_for_error_code("SOME_INTERNAL_BREACH_CODE"), 500);
    }

    /// A3 — `classify_outcome` keys the success disposition on the durable doc
    /// state: terminal-success → Done (200), in-flight → InProgress (202
    /// IN_PROGRESS, parity with replay), terminal-FAILURE → Breach (the seam
    /// contract breach the handler 500s).  Exhaustive over `DocState` so a new
    /// state forces a decision.
    #[test]
    fn classify_outcome_classifies_every_doc_state() {
        use DocState::*;
        assert_eq!(classify_outcome(Ack), OutcomeDisposition::Done);
        assert_eq!(classify_outcome(OfflineLocalAck), OutcomeDisposition::Done);
        for s in [
            Prepared,
            Signed,
            Encrypted,
            Sending,
            Sent,
            Kvt1,
            Kvt2,
            ErrorRetryable,
        ] {
            assert_eq!(
                classify_outcome(s),
                OutcomeDisposition::InProgress,
                "{s:?} is in-flight → 202 IN_PROGRESS"
            );
        }
        for s in [Rejected, Cancelled, RequiresManualReconciliation] {
            assert_eq!(
                classify_outcome(s),
                OutcomeDisposition::Breach,
                "{s:?} is a terminal failure — never an Ok outcome"
            );
        }
    }

    #[test]
    fn z_class_is_only_zreport_and_shiftclose() {
        assert!(is_z_class(CommandType::ZReport));
        assert!(is_z_class(CommandType::ShiftClose));
        for ct in [
            CommandType::Sell,
            CommandType::Return,
            CommandType::ShiftOpen,
            CommandType::XReport,
        ] {
            assert!(!is_z_class(ct), "{ct:?}");
        }
    }

    /// A3 — an `OfflineLocalAck` outcome is a SUCCESS: `200`, with a null
    /// `fiscal_id` (no DPS id yet) but the durable state surfaced.
    #[tokio::test]
    async fn offline_local_ack_maps_200_with_null_fiscal_id() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: offline_outcome(),
        };
        let r = handle_command(
            &sell_cmd("idem-off", 10000),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r.http_status, 200, "offline-local-ack is a 200 success");
        match r.body {
            IngressBody::Success(b) => {
                assert!(b.ok);
                assert_eq!(b.document_state, "OFFLINE_LOCAL_ACK");
                assert_eq!(b.fiscal_id, None, "offline-acked receipt has no DPS id yet");
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    /// A3 (review HIGH — first-pass↔replay parity) — an in-flight outcome
    /// (`Sending`) renders the SAME `202 IN_PROGRESS` ERROR envelope the
    /// replay path emits (dto.rs IN_PROGRESS contract), NOT a success body.
    /// So a blocking client sees ONE 202 shape across the first POST and every
    /// re-poll.
    #[tokio::test]
    async fn in_flight_sending_maps_202_in_progress_error_envelope() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: sending_outcome(),
        };
        let r = handle_command(
            &sell_cmd("idem-sending", 10000),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(r.http_status, 202, "an in-flight send is 202 IN_PROGRESS");
        match r.body {
            IngressBody::Error(e) => {
                assert!(!e.ok, "IN_PROGRESS is the non-terminal ERROR envelope");
                assert_eq!(e.error_code, "IN_PROGRESS");
            }
            other => panic!("expected the IN_PROGRESS error envelope, got {other:?}"),
        }
    }

    /// A3 — every RS-3 real fiscal failure maps to its HTTP status AND, unlike
    /// `NotImplemented`, does NOT release the (still-present) inbox row: the
    /// write-path owns the row's durable lifecycle.
    #[tokio::test]
    async fn real_fiscal_failures_map_status_and_keep_inbox_row() {
        for (kind, idem, want_status, want_code) in [
            (
                ErrKind::ShiftNotOpen,
                "idem-noshift",
                422u16,
                "NO_OPEN_SHIFT",
            ),
            (ErrKind::SignFailure, "idem-sign", 500, "SIGN_FAILED"),
            (ErrKind::DpsRejected, "idem-rej", 422, "FISCAL_REJECTED"),
            (
                ErrKind::ZSurfaceNotReady,
                "idem-zsurf",
                501,
                "Z_SURFACE_NOT_READY",
            ),
            (
                ErrKind::ShiftGuardRefused("SHIFT_ALREADY_OPEN"),
                "idem-sg",
                422,
                "SHIFT_ALREADY_OPEN",
            ),
            (
                ErrKind::Internal("SHIFT_MANUAL_RECON"),
                "idem-int",
                500,
                "SHIFT_MANUAL_RECON",
            ),
            (
                ErrKind::OfflineRefused("NODE_GOING_ONLINE"),
                "idem-refused",
                503,
                "NODE_GOING_ONLINE",
            ),
        ] {
            let (_d, main, secure) = fresh_pools().await;
            let wp = RecordingErr {
                captured: Mutex::new(None),
                kind,
            };
            let r = handle_command(
                &shift_open_cmd(idem),
                FN,
                drv(),
                Protocol::Rest,
                &main,
                &secure,
                &wp,
            )
            .await;
            assert_eq!(r.http_status, want_status, "{want_code}: status");
            match r.body {
                IngressBody::Error(e) => {
                    assert_eq!(e.error_code, want_code, "{want_code}: error_code")
                }
                other => panic!("{want_code}: expected Error, got {other:?}"),
            }
            // The inbox row the seam was handed MUST still be present (not
            // released) — the write-path, not the handler, owns its lifecycle.
            // `RecordingErr` deliberately does NOT take the lease, so the row
            // stays NEW: this is the adversarial fixture for the no-release
            // assertion (a regression that fired `delete_new_by_request_id`
            // here would drop the count to 0). In production the lease makes
            // the row PROCESSING, which the `WHERE status='NEW'` guard already
            // protects — so "handler issues no release at all" is the property
            // under test.
            let rid = wp
                .captured
                .lock()
                .unwrap()
                .clone()
                .expect("seam must have received the row")
                .request_id;
            let rows: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM ingress_inbox WHERE request_id = ?")
                    .bind(&rid[..])
                    .fetch_one(&main)
                    .await
                    .unwrap();
            assert_eq!(
                rows, 1,
                "{want_code}: a real fiscal failure must NOT release the inbox row"
            );
        }
    }

    /// A3 — defense-in-depth: an `Ok(FiscalOutcome)` carrying a terminal
    /// FAILURE state (a seam contract breach — failures must be `Err`) is
    /// turned into a `500 INTERNAL` + audited, NEVER a phantom 200/202.
    #[tokio::test]
    async fn ok_with_terminal_failure_state_is_500_and_audited() {
        let (_d, main, secure) = fresh_pools().await;
        let wp = RecordingOk {
            captured: Mutex::new(None),
            out: FiscalOutcome {
                document_id: DocumentId::new(),
                fiscal_id: None,
                fiscal_ts: None,
                document_state: DocState::Rejected,
                report_xml: None,
            },
        };
        let r = handle_command(
            &shift_open_cmd("idem-breach"),
            FN,
            drv(),
            Protocol::Rest,
            &main,
            &secure,
            &wp,
        )
        .await;
        assert_eq!(
            r.http_status, 500,
            "Ok(failure-state) is a contract breach → 500"
        );
        match r.body {
            IngressBody::Error(e) => assert_eq!(e.error_code, "INTERNAL"),
            other => panic!("expected Error, got {other:?}"),
        }
        let audited: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE event_type = 'SEAM_OK_WITH_FAILURE_STATE'",
        )
        .fetch_one(&main)
        .await
        .unwrap();
        assert_eq!(audited, 1, "the contract breach must be audited");
    }
}
