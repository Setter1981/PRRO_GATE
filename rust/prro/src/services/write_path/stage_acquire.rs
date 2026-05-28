//! Stage 1+2 — acquire+validate+guard.
//!
//! Per W0-1 §3.1–§3.2 + ADR-M3-A1 (lnd sequencer) + ADR-M3-A5
//! (Pattern A — pure DB stage) + ADR-M3-A7 (App::boot interaction).
//!
//! One `with_immediate` envelope per request.  All operations inside
//! the lock are pure DB — no `CryptoProvider`, no `DpsChannel`, no
//! `spawn_blocking` (W3 static scan + runtime guard enforce this).

use anyhow::Context;

use crate::db::models::enums::{DocType, NodeMode, Severity, ShiftState};
use crate::db::models::ids::{DocumentId, RequestId};
use crate::db::repositories::{
    audit_log,
    fiscal_documents::{self as fd, NewDocument},
    ingress_inbox, node_state, shifts,
};
use crate::db::tx::with_immediate;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use super::types::{Channel, CanonicalFiscalCommand, RejectionReason, WorkerContext, WorkerProcessResult};

/// Public stage-1 entry.  Opens one `with_immediate` envelope and
/// runs the lease + guard + lnd-allocate + INSERT PREPARED + audit
/// sequence atomically.  See module-level docs for ADR anchors.
///
/// Note: "lease" in this module means the inbox-row lease taken by
/// `ingress_inbox::acquire_lease`, which is keyed on `request_id`.
/// It is **not** an FN-scope lock — see ADR-M3-A10 for the M3a
/// single-writer-per-FN invariant and its current enforcement
/// mechanism (global-single-writer + `BEGIN IMMEDIATE`).
///
/// Invariants:
/// - On `Proceed`: lease=PROCESSING, lnd advanced, fiscal_documents
///   row PREPARED, audit `doc_prepared` appended.  All committed
///   together.
/// - On `Resumed`: lease=PROCESSING, NO lnd advance, NO new fiscal_
///   documents row (existing pending row reused), audit
///   `resume_detected` appended.
/// - On `Noop`: NO state mutation; tx commits an empty diff.
/// - On `Rejected`: ingress_inbox.status=REJECTED + audit row
///   appended.  NO fiscal_documents row, NO lnd advance.
pub async fn run(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    driver_id: &str,
    request_id: [u8; 16],
    command: CanonicalFiscalCommand,
) -> anyhow::Result<WorkerProcessResult> {
    // W4-Z2a piece 6b.3 — secure-pool hoist.  Load tax_snapshot
    // BEFORE opening the main with_immediate envelope to avoid
    // holding BEGIN IMMEDIATE during a cross-DB read (INV-1 spirit:
    // minimise non-essential work inside the write tx).
    //
    // Pre-tx peek for `fiscal_number` is advisory (read-only, no
    // CAS).  If the inbox has no NEW row → short-circuit Noop
    // without opening main tx.  Otherwise load snapshot from
    // pool_secure and proceed; the lease CAS inside the tx
    // re-checks NEW state — race-OK (we discard an unused snapshot
    // by simply not inserting it).
    let peeked_fn_id = match ingress_inbox::peek_fiscal_number_by_request_id(
        pool, &request_id,
    ).await? {
        Some(fn_id) => fn_id,
        None => return Ok(WorkerProcessResult::Noop),
    };
    let tax_snapshot = crate::runtime::tax_snapshot::load_for_fn_driver(
        pool_secure, &peeked_fn_id, driver_id,
    ).await?;

    let driver_id_owned = driver_id.to_string();
    let peeked_fn_id_owned = peeked_fn_id.clone();
    let tax_snapshot_for_tx = tax_snapshot.clone();

    with_immediate(pool, move |tx| {
        let driver_id = driver_id_owned.clone();
        let peeked_fn_id = peeked_fn_id_owned.clone();
        let tax_snapshot = tax_snapshot_for_tx.clone();
        Box::pin(async move {
            // [Step 1] Acquire lease — CAS NEW → PROCESSING.
            let inbox = match ingress_inbox::acquire_lease(tx, &request_id).await? {
                Some(row) => row,
                None => return Ok(WorkerProcessResult::Noop),
            };
            let fn_id = inbox.fiscal_number.clone();

            // [Step 1a] W4-Z2a piece 6b.3 — defensive assertion that
            //           the pre-tx peek read the same fiscal_number as
            //           the leased inbox row.  Unreachable under
            //           inbox PK invariant (request_id is unique),
            //           but fails loud on regression (e.g., if a
            //           future change ever lets request_id be reused
            //           across FNs).  The snapshot was loaded against
            //           `peeked_fn_id` — using it for a different fn
            //           would persist the wrong tax-config pinning.
            if inbox.fiscal_number != peeked_fn_id {
                anyhow::bail!(
                    "stage_acquire invariant: peeked fn={} but leased inbox fn={} for request_id={}",
                    peeked_fn_id, inbox.fiscal_number, hex_encode(&request_id),
                );
            }

            // W4-Z2a piece 6b-self-review Important #1 — snapshot
            // insert MOVED to immediately before Step 8 INSERT PREPARED
            // (after Step 6 Resume-detect and Step 6b terminal-detect
            // both returned early).  Net effect: snapshot row is
            // persisted ONLY on the Proceed path.  Resume / Reject /
            // Terminal paths no longer commit forensic orphan rows.

            // [Step 1b] Command-vs-inbox cross-check.  The leased
            //           inbox row carries `payload_json`,
            //           `payload_sha256_canonical`, and
            //           `operation_type` persisted by ingress; the
            //           in-process `command` argument MUST agree on
            //           hash and doc_type, otherwise the worker is
            //           about to PREPARE a doc against payload it did
            //           not receive.  Reject without lnd advance and
            //           without INSERT.
            if command.payload_sha256_canonical != inbox.payload_sha256_canonical {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::InvalidPayload {
                        detail: "command_payload_hash_mismatch".to_string(),
                    },
                    "command_inbox_mismatch",
                    Severity::Critical,
                    None,
                )
                .await;
            }
            if command.doc_type.as_str() != inbox.operation_type {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::InvalidPayload {
                        detail: "command_doc_type_mismatch".to_string(),
                    },
                    "command_inbox_mismatch",
                    Severity::Critical,
                    None,
                )
                .await;
            }

            // [Step 2] Snapshot node_state inside the same tx.
            let node_state = node_state::get_tx(tx, &fn_id)
                .await?
                .with_context(|| format!("node_state row missing for fn={fn_id}"))?;

            // [Step 3a] W14a-2b Commit 4 — mode guard rewrite per spec §3.3.
            // Replaces the pre-W14a-2b `mode != Online → NodeOffline` binary.
            //
            // Mapping:
            //   Online                      → Channel::Online, proceed
            //   Offline | GoingOffline      → Channel::Offline, proceed
            //   GoingOnline                 → reject (return-online drain in flight)
            //   Blocked                     → reject (operator manual recovery)
            //   StopMode                    → reject (legal hold)
            //   CryptoDegraded              → reject (crypto subsystem degraded)
            let channel = match node_state.mode {
                NodeMode::Online => Channel::Online,
                NodeMode::Offline | NodeMode::GoingOffline => Channel::Offline,
                NodeMode::GoingOnline => {
                    // NIT-C4-2 fix: Warning (not Info) for parity with
                    // other mode-side refusals.  Operator dashboards
                    // need consistent visibility on refused ops — the
                    // "transient" nature of GoingOnline is documented
                    // in the rationale but doesn't justify a lower
                    // audit-severity gate.
                    return reject(
                        tx,
                        &request_id,
                        RejectionReason::NodeGoingOnlineDrainInFlight,
                        "node_going_online_drain_in_flight",
                        Severity::Warning,
                        Some(mode_refusal_context(
                            &fn_id,
                            command.doc_type,
                            node_state.mode,
                            Some("Online"),
                        )),
                    )
                    .await;
                }
                NodeMode::Blocked => {
                    return reject(
                        tx,
                        &request_id,
                        RejectionReason::NodeBlocked,
                        "node_blocked",
                        Severity::Warning,
                        Some(mode_refusal_context(
                            &fn_id,
                            command.doc_type,
                            node_state.mode,
                            None,
                        )),
                    )
                    .await;
                }
                NodeMode::StopMode => {
                    return reject(
                        tx,
                        &request_id,
                        RejectionReason::NodeStopMode,
                        "node_stop_mode",
                        Severity::Warning,
                        Some(mode_refusal_context(
                            &fn_id,
                            command.doc_type,
                            node_state.mode,
                            None,
                        )),
                    )
                    .await;
                }
                NodeMode::CryptoDegraded => {
                    return reject(
                        tx,
                        &request_id,
                        RejectionReason::NodeCryptoDegraded,
                        "node_crypto_degraded",
                        Severity::Critical,
                        Some(mode_refusal_context(
                            &fn_id,
                            command.doc_type,
                            node_state.mode,
                            None,
                        )),
                    )
                    .await;
                }
            };

            // [Step 3b] Profile-binding guard — schema permits NULL,
            // submissions cannot proceed without resolved bindings.
            if node_state.backend_profile_id.is_none()
                || node_state.transport_profile_id.is_none()
            {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::MissingProfileBinding,
                    "profile_binding_missing",
                    Severity::Warning,
                    None,
                )
                .await;
            }

            // [Step 4] Shift-state guard — channel-aware (W14a-2b Commit 4).
            if let Some(reason) =
                check_shift_guard(command.doc_type, node_state.shift_state, channel)
            {
                return reject(
                    tx,
                    &request_id,
                    reason,
                    "guard_rejected",
                    Severity::Warning,
                    Some(shift_guard_refusal_context(
                        &fn_id,
                        command.doc_type,
                        node_state.shift_state,
                        channel,
                        node_state.current_shift_id.as_ref(),
                    )),
                )
                .await;
            }

            // [Step 5] Resolve active_shift via node_state.current_shift_id.
            //          Single source of truth; no scan-for-latest-open.
            //
            // HIGH-C4-1 fix (operator-flagged 2026-05-19): resolver widened
            // to `Opened | OpenedLocalPendingDrain` per spec §3.6a.  Without
            // this, regular fiscal docs in `OpenedLocalPendingDrain + Offline`
            // (now allowed by §3.4 matrix) would be inserted with
            // `fiscal_documents.shift_id = NULL` — breaking forensic
            // attribution + future W9b drain-time signer enforcement
            // (signer_guard would surface `ShiftMissingForFiscalDoc` for
            // legitimate offline docs).
            let active_shift =
                match (node_state.shift_state, &node_state.current_shift_id) {
                    (
                        ShiftState::Opened | ShiftState::OpenedLocalPendingDrain,
                        Some(shift_id),
                    ) => {
                        let expected = node_state.shift_state;
                        let row = shifts::get_tx(tx, *shift_id).await?;
                        match row {
                            Some(s) if s.state == expected => Some(s),
                            _ => {
                                // node_state.shift_state says (Opened |
                                // OpenedLocalPendingDrain) but resolved
                                // row missing OR state diverges from
                                // node_state mirror — structural breach.
                                return reject(
                                    tx,
                                    &request_id,
                                    RejectionReason::ShiftInvariantViolation,
                                    "shift_invariant_violation",
                                    Severity::Critical,
                                    None,
                                )
                                .await;
                            }
                        }
                    }
                    (
                        ShiftState::Opened | ShiftState::OpenedLocalPendingDrain,
                        None,
                    ) => {
                        // shift_state says open but current_shift_id IS NULL.
                        return reject(
                            tx,
                            &request_id,
                            RejectionReason::ShiftInvariantViolation,
                            "shift_invariant_violation",
                            Severity::Critical,
                            None,
                        )
                        .await;
                    }
                    _ => None,
                };

            // [Step 6] Resume-detect — pending lookup only.  Terminal
            //          rows (ACK / REJECTED / CANCELLED /
            //          OFFLINE_LOCAL_ACK / REQUIRES_MANUAL_RECONCILIATION)
            //          MUST NOT drive Resumed; their flow is concluded.
            let request_id_typed = RequestId::from_bytes(request_id);
            if let Some(existing) =
                fd::get_pending_by_request_id_tx(tx, &request_id_typed).await?
            {
                audit_log::append_tx(
                    tx,
                    "fiscal_document",
                    &format!("{:?}", existing.document_id),
                    "resume_detected",
                    Severity::Info,
                    None,
                    Some(&format!(
                        r#"{{"request_id":"{request_id_hex}","existing_state":"{state}","lnd":{lnd}}}"#,
                        request_id_hex = hex_encode(&request_id),
                        state = existing.state.as_str(),
                        lnd = existing.lnd
                    )),
                )
                .await?;
                // W4-Z2a piece 6b.2 (external mid-review IMP-1) — Resume
                // branch returns `tax_resolution_snapshot_id: None` to
                // FORCE downstream consumer (stage_sign 3-PRE re-entry,
                // MAC recovery re_sign) to load the persisted FK from
                // `fd::get_signing_inputs_tx`.  The fresh snapshot in
                // ctx.tax_resolution_snapshot is ONLY a config-current
                // view (kept for forensic / future use); piece-9 reload
                // will replace it with the historic snapshot keyed by
                // doc row's persisted FK.
                //
                // Why None over Some(fresh_id): doc-comment lock isn't
                // enough — type-system None forces the branching at
                // compile time.  Per locked design rule #9: "MAC
                // recovery uses persisted snapshot_id, NEVER current
                // config".
                return Ok(WorkerProcessResult::Resumed(WorkerContext {
                    inbox,
                    command,
                    node_state,
                    active_shift,
                    document: existing,
                    // W4-Z2a piece 6b external review (R1+R2 High):
                    // structural None on Resume — piece-8/9 author is
                    // compile-time forced to fetch the persisted
                    // snapshot via doc FK rather than accidentally
                    // using a fresh-config view.
                    tax_resolution_snapshot: None,
                    tax_resolution_snapshot_id: None,
                }));
            }

            // [Step 6b] Terminal-doc + NEW-inbox-for-same-request_id =
            //           invariant breach.  Refuse to INSERT a duplicate
            //           PREPARED; surface as InvalidPayload (Critical
            //           audit) so the operator can investigate the
            //           lifecycle clash.
            if fd::exists_terminal_by_request_id_tx(tx, &request_id_typed).await? {
                return reject(
                    tx,
                    &request_id,
                    RejectionReason::InvalidPayload {
                        detail: "terminal_document_for_request_id".to_string(),
                    },
                    "terminal_document_for_request_id",
                    Severity::Critical,
                    None,
                )
                .await;
            }

            // [Step 6c] W4-Z2a piece 6b-self-review Important #1 —
            //           insert tax_snapshot ONLY now that we're
            //           definitively on the Proceed path (Resume and
            //           Terminal already returned early above).
            //           Same `with_immediate` envelope → atomic with
            //           lnd alloc + INSERT PREPARED.  Snapshot bytes
            //           already computed outside tx; inside tx only
            //           `INSERT OR IGNORE` + `SELECT id` runs.
            let tax_snapshot_id =
                crate::db::repositories::signing_config_snapshots::insert_or_get_id_tx(
                    tx, &fn_id, &driver_id, &tax_snapshot,
                )
                .await?;

            // [Step 7] Allocate lnd atomically (UPDATE ... RETURNING).
            let lnd = node_state::allocate_next_lnd(tx, &fn_id).await?;

            // [Step 8] INSERT fiscal_documents (state=PREPARED).
            //          Profile bindings unwrap-safe — guarded at Step 3b.
            let backend_profile_id = node_state
                .backend_profile_id
                .clone()
                .expect("profile-binding guard ensures Some");
            let transport_profile_id = node_state
                .transport_profile_id
                .clone()
                .expect("profile-binding guard ensures Some");
            let document_id = DocumentId::new();
            // MED-C4-3 fix (operator-flagged 2026-05-19): derive fs_mode
            // from the channel resolved at Step 3a.  Pre-fix hardcoded
            // "ONLINE" produced a ledger-drift for offline-channel
            // inserts allowed by C4 (Sell|... + OpenedLocalPendingDrain
            // + Offline) — the persisted row would report ONLINE for a
            // doc that later transitions to OFFLINE_LOCAL_ACK.
            let fs_mode = match channel {
                Channel::Online => "ONLINE",
                Channel::Offline => "OFFLINE",
            };
            let new_doc = NewDocument {
                document_id,
                request_id: request_id_typed,
                fiscal_number: fn_id.clone(),
                shift_id: active_shift.as_ref().map(|s| s.shift_id),
                offline_session_id: None,
                lnd,
                doc_type: command.doc_type,
                backend_profile_id,
                transport_profile_id,
                fs_mode,
                business_ts: command.business_ts.clone(),
                total_sum_kop: command.total_sum_kop,
                payload_json: command.payload_json.clone(),
                payload_sha256_canonical: command.payload_sha256_canonical,
                unsigned_xml_sha256: None,
                previous_hash: None,
                // W4-Z2a piece 6b.1 (external mid-review CRIT-2) — FK set
                // at INSERT in the same with_immediate envelope as the
                // snapshot row INSERT.  Closes two-envelope crash window:
                // any taxable PREPARED doc on disk already references its
                // frozen tax config.
                signing_config_snapshot_id: Some(tax_snapshot_id),
                // W14a-2b Commit 2: threaded from CanonicalFiscalCommand
                // (Commit 1 plumbed `None` baseline; field now flows
                // from ingress through stage_acquire → INSERT PREPARED).
                signed_by_cashier_id: command.signed_by_cashier_id.clone(),
            };
            fd::insert_prepared_tx(tx, &new_doc).await?;

            // [Step 9] Audit success.
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{document_id:?}"),
                "doc_prepared",
                Severity::Info,
                None,
                Some(&format!(
                    r#"{{"request_id":"{}","lnd":{lnd},"doc_type":"{doc_type}"}}"#,
                    hex_encode(&request_id),
                    doc_type = command.doc_type.as_str()
                )),
            )
            .await?;

            // [Step 10] Build the freshly-inserted DocumentRow snapshot
            //          to hand to the next stage.  We could re-SELECT,
            //          but the values we just INSERTed are deterministic.
            let document = fd::DocumentRow {
                document_id,
                fiscal_number: fn_id.clone(),
                lnd,
                state: crate::db::models::enums::DocState::Prepared,
                doc_type: command.doc_type,
                server_fiscal_no: None,
                submission_attempted_at: None,
                backend_profile_id: new_doc.backend_profile_id.clone(),
                transport_profile_id: new_doc.transport_profile_id.clone(),
                // W6 — fresh PREPARED row: chain hash, Z, unsigned-hash,
                // and pin-marker are all genuinely None.  Stage 3-PRE
                // (W6) is the canonical pin site.
                previous_hash: None,
                z_report_number: None,
                unsigned_xml_sha256: None,
                signing_inputs_pinned_at: None,
                // W14a-2b Commit 1: carries the value from new_doc (None
                // until Commit 2 plumbs CanonicalFiscalCommand).
                signed_by_cashier_id: new_doc.signed_by_cashier_id.clone(),
                // W4-Z2a piece 6b — FK to signing_config_snapshots row
                // just inserted in this same with_immediate envelope.
                signing_config_snapshot_id: Some(tax_snapshot_id),
            };

            Ok(WorkerProcessResult::Proceed(WorkerContext {
                inbox,
                command,
                node_state,
                active_shift,
                document,
                // W4-Z2a piece 6b external review: Some on Proceed,
                // matches just-inserted snapshot row.
                tax_resolution_snapshot: Some(tax_snapshot),
                tax_resolution_snapshot_id: Some(tax_snapshot_id),
            }))
        })
    })
    .await
}

/// Apply a guard rejection: mark inbox REJECTED + append audit row.
/// No `fiscal_documents` row, no lnd allocation — the inbox carries
/// the rejection.
/// MED-C4-2: build per-spec §3.6 audit payload for mode-guard refusals.
/// Shape: `{fiscal_number, doc_type, current_mode, requested_channel?}`.
fn mode_refusal_context(
    fn_id: &str,
    doc_type: DocType,
    current_mode: NodeMode,
    requested_channel: Option<&'static str>,
) -> Value {
    let mut obj = json!({
        "fiscal_number": fn_id,
        "doc_type": doc_type.as_str(),
        "current_mode": format!("{current_mode:?}"),
    });
    if let (Some(req), Value::Object(map)) = (requested_channel, &mut obj) {
        map.insert("requested_channel".to_string(), Value::String(req.into()));
    }
    obj
}

/// MED-C4-2: build per-spec §3.6 audit payload for shift-guard refusals.
/// Shape: `{fiscal_number, doc_type, current_state, current_channel,
/// shift_id?}` (shift_id from `node_state.current_shift_id` if present).
fn shift_guard_refusal_context(
    fn_id: &str,
    doc_type: DocType,
    current_state: ShiftState,
    current_channel: Channel,
    shift_id: Option<&crate::db::models::ids::ShiftId>,
) -> Value {
    let channel_str = match current_channel {
        Channel::Online => "Online",
        Channel::Offline => "Offline",
    };
    let mut obj = json!({
        "fiscal_number": fn_id,
        "doc_type": doc_type.as_str(),
        "current_state": current_state.as_str(),
        "current_channel": channel_str,
    });
    if let (Some(sid), Value::Object(map)) = (shift_id, &mut obj) {
        map.insert(
            "shift_id".to_string(),
            Value::String(hex_encode(sid.as_bytes())),
        );
    }
    obj
}

/// MED-C4-2: per-spec §3.6 audit-event names for W14a-2b variants.
/// `None` falls back to the legacy `event_type` argument passed by the
/// caller (`guard_rejected` for pre-existing shift refusals, etc.).
fn spec_event_for(reason: &RejectionReason) -> Option<&'static str> {
    use RejectionReason::*;
    match reason {
        NodeGoingOnlineDrainInFlight => Some("STAGE_ACQUIRE_GOING_ONLINE_REFUSED"),
        NodeBlocked => Some("STAGE_ACQUIRE_BLOCKED_REFUSED"),
        NodeStopMode => Some("STAGE_ACQUIRE_STOP_MODE_REFUSED"),
        NodeCryptoDegraded => Some("STAGE_ACQUIRE_CRYPTO_DEGRADED_REFUSED"),
        ShiftOpenPendingDrainOpRefused => Some("SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED"),
        PostLocalCloseSaleRefused => Some("POST_LOCAL_CLOSE_SALE_REFUSED"),
        OfflineShiftCloseNotSupported => Some("OFFLINE_SHIFT_CLOSE_REFUSED"),
        ShiftClosingInFlight => Some("SHIFT_CLOSING_IN_FLIGHT_OP_REFUSED"),
        ZReportBlockedBacklogDrainPending => Some("OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED"),
        _ => None,
    }
}

async fn reject(
    tx: &mut crate::db::tx::WriteTxConn<'_>,
    request_id: &[u8; 16],
    reason: RejectionReason,
    legacy_event_type: &'static str,
    severity: Severity,
    extra_context: Option<Value>,
) -> anyhow::Result<WorkerProcessResult> {
    ingress_inbox::mark_rejected_tx(tx, request_id).await?;
    // MED-C4-2: derive spec §3.6 event name for W14a-2b variants.
    let event_type = spec_event_for(&reason).unwrap_or(legacy_event_type);
    // MED-C4-2: build per-spec audit payload; legacy callers pass
    // `None` and keep the minimal `{"reason": ...}` shape.
    let mut payload = json!({"reason": format!("{reason:?}")});
    if let Some(Value::Object(extras)) = extra_context {
        if let Value::Object(base) = &mut payload {
            for (k, v) in extras {
                base.insert(k, v);
            }
        }
    }
    audit_log::append_tx(
        tx,
        "ingress_inbox",
        &hex_encode(request_id),
        event_type,
        severity,
        None,
        Some(&payload.to_string()),
    )
    .await?;
    Ok(WorkerProcessResult::Rejected { reason })
}

/// Stage 2 shift-state guard — W14a-2b Commit 4 channel-aware.
/// Returns `Some(reason)` if the (doc_type, shift_state, channel)
/// triple is forbidden, `None` if allowed.
///
/// **Order matters**: terminal / operator-action shift states are
/// matched FIRST (channel-irrelevant), then doc-type-specific arms.
/// Per spec §3.4 + §5.6:
///   - `(_, Error, _)` → `ShiftInError` (structural-breach surface).
///   - `(_, RequiresManualReconciliation, _)` → `ShiftRequiresOperatorAttention`.
///   - W14a-2b matrix: 9 doc types × 9 shift states × 2 channels = 162
///     cells.  Explicit arms cover all non-trivial pairs; catch-all
///     `ShiftNotOpen` handles the residual.
///
/// **Channel semantics:**
///   - `Channel::Online`: classical write-path.  Refused on
///     `OpenedLocalPendingDrain` for regular fiscal ops + Z_REPORT
///     (operator must wait for drain or reissue offline).
///   - `Channel::Offline`: Pattern C resilience surface.  Regular
///     fiscal ops succeed on `OpenedLocalPendingDrain` (pre-W10
///     Z_REPORT is blocked unconditionally pending coupled pool/
///     backlog/edge-7 logic).
///
/// **`ClosingLocalPendingDrain`** is the post-local-close lockout
/// (PR #62 §W10): every doc type is refused regardless of channel.
fn check_shift_guard(
    doc_type: DocType,
    shift_state: ShiftState,
    channel: Channel,
) -> Option<RejectionReason> {
    use Channel::*;
    use DocType::*;
    use ShiftState::*;
    match (doc_type, shift_state, channel) {
        // ── Terminal / operator-action arms — channel-irrelevant ──
        (_, Error, _) => Some(RejectionReason::ShiftInError),
        (_, RequiresManualReconciliation, _) => {
            Some(RejectionReason::ShiftRequiresOperatorAttention)
        }

        // ── Shift-management ops ──
        (ShiftOpen, Closed, _) => None,
        // NIT-C4-1 fix: spec §3.4 matrix specifies distinct refusal
        // reason for (ShiftOpen, ClosingLocalPendingDrain).  Shift is
        // mid-close, not "already open"; ShiftClosingInFlight is the
        // forensic-accurate label.
        (ShiftOpen, ClosingLocalPendingDrain, _) => {
            Some(RejectionReason::ShiftClosingInFlight)
        }
        (ShiftOpen, _, _) => Some(RejectionReason::ShiftAlreadyOpen),
        (ShiftClose, Opened, _) => None,
        (ShiftClose, OpenedLocalPendingDrain, _) => {
            // Spec §5.7 L2 — offline shift close not modeled.
            Some(RejectionReason::OfflineShiftCloseNotSupported)
        }
        (ShiftClose, ClosingLocalPendingDrain, _) => {
            Some(RejectionReason::ShiftClosingInFlight)
        }
        (ZReport, Opened, _) => None,
        (ZReport, OpenedLocalPendingDrain, _) => {
            // Pre-W10 guardrail (both channels).  Spec §3.4 + operator
            // correction #3 (2026-05-19): W10 later replaces this
            // refusal with coupled pool/backlog/edge-7 logic.
            Some(RejectionReason::ZReportBlockedBacklogDrainPending)
        }
        (ZReport, ClosingLocalPendingDrain, _) => {
            Some(RejectionReason::ShiftClosingInFlight)
        }
        // ShiftClose / ZReport against `Closed` — shift is terminal,
        // operator should issue ShiftOpen first.
        (ShiftClose | ZReport, Closed, _) => {
            Some(RejectionReason::ShiftNotOpen { current: shift_state })
        }

        // ── Mid-transition (Created / Opening / Closing) — block all.
        (_, Created | Opening | Closing, _) => {
            Some(RejectionReason::ShiftNotOpen { current: shift_state })
        }

        // ── Regular fiscal ops in Opened — channel-irrelevant happy.
        (
            Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
            Opened,
            _,
        ) => None,

        // ── Regular fiscal ops in Closed — channel-irrelevant refusal.
        (
            Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
            Closed,
            _,
        ) => Some(RejectionReason::ShiftNotOpen { current: shift_state }),

        // ── W14a-2b channel-aware OpenedLocalPendingDrain ──
        // Offline channel: Pattern C resilience surface — allowed.
        (
            Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
            OpenedLocalPendingDrain,
            Offline,
        ) => None,
        // Online channel: operator should reissue offline or wait
        // for drain — refused with typed audit shape.
        (
            Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
            OpenedLocalPendingDrain,
            Online,
        ) => Some(RejectionReason::ShiftOpenPendingDrainOpRefused),

        // ── ClosingLocalPendingDrain — post-local-close lockout ──
        // ALL regular fiscal ops refused (PR #62 §W10).  Channel-
        // irrelevant — once a Z-report has been locally acked, no
        // further sale-flavour ops accepted regardless of channel.
        (
            Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
            ClosingLocalPendingDrain,
            _,
        ) => Some(RejectionReason::PostLocalCloseSaleRefused),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            use std::fmt::Write;
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

#[cfg(test)]
mod channel_aware_matrix_tests {
    //! W14a-2b Commit 7 §3 — pure-function 162-cell coverage matrix
    //! for [`check_shift_guard`].  9 DocType × 9 ShiftState × 2
    //! Channel = 162 cells; each cell has an explicit verdict
    //! locked by [`expected_outcome`].
    //!
    //! Pins spec §3.4 matrix contract: every channel-aware refusal
    //! variant + every happy-path None is verified per-cell, NOT
    //! via group fallthroughs.  Future matrix edits MUST update
    //! [`expected_outcome`] in lockstep with the production arm.
    //!
    //! Run with `cargo test -p prro --features test-support
    //! channel_aware_matrix`.
    use super::*;

    const ALL_DOC_TYPES: &[DocType] = &[
        DocType::ShiftOpen,
        DocType::ShiftClose,
        DocType::Sell,
        DocType::Return,
        DocType::ServiceIn,
        DocType::ServiceOut,
        DocType::CashWithdrawal,
        DocType::XReport,
        DocType::ZReport,
    ];
    const ALL_SHIFT_STATES: &[ShiftState] = &[
        ShiftState::Created,
        ShiftState::Opening,
        ShiftState::OpenedLocalPendingDrain,
        ShiftState::Opened,
        ShiftState::ClosingLocalPendingDrain,
        ShiftState::Closing,
        ShiftState::Closed,
        ShiftState::RequiresManualReconciliation,
        ShiftState::Error,
    ];
    const ALL_CHANNELS: &[Channel] = &[Channel::Online, Channel::Offline];

    /// Independent re-implementation of the spec §3.4 matrix used as
    /// the test oracle.  Authoritative `check_shift_guard` MUST agree
    /// with this verdict per cell.
    fn expected_outcome(
        doc: DocType,
        state: ShiftState,
        ch: Channel,
    ) -> Option<RejectionReason> {
        use Channel::*;
        use DocType::*;
        use ShiftState::*;
        // Terminal / operator-action arms — channel-irrelevant.
        match state {
            Error => return Some(RejectionReason::ShiftInError),
            RequiresManualReconciliation => {
                return Some(RejectionReason::ShiftRequiresOperatorAttention)
            }
            _ => {}
        }
        // Shift-management surfaces.
        match (doc, state) {
            (ShiftOpen, Closed) => return None,
            (ShiftOpen, ClosingLocalPendingDrain) => {
                return Some(RejectionReason::ShiftClosingInFlight)
            }
            (ShiftOpen, _) => return Some(RejectionReason::ShiftAlreadyOpen),
            (ShiftClose, Opened) => return None,
            (ShiftClose, OpenedLocalPendingDrain) => {
                return Some(RejectionReason::OfflineShiftCloseNotSupported)
            }
            (ShiftClose, ClosingLocalPendingDrain) => {
                return Some(RejectionReason::ShiftClosingInFlight)
            }
            (ZReport, Opened) => return None,
            (ZReport, OpenedLocalPendingDrain) => {
                return Some(RejectionReason::ZReportBlockedBacklogDrainPending)
            }
            (ZReport, ClosingLocalPendingDrain) => {
                return Some(RejectionReason::ShiftClosingInFlight)
            }
            (ShiftClose | ZReport, Closed) => {
                return Some(RejectionReason::ShiftNotOpen { current: state })
            }
            _ => {}
        }
        // Mid-transition channel-irrelevant.
        if matches!(state, Created | Opening | Closing) {
            return Some(RejectionReason::ShiftNotOpen { current: state });
        }
        // Regular fiscal ops.
        match (doc, state, ch) {
            (
                Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
                Opened,
                _,
            ) => None,
            (
                Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
                Closed,
                _,
            ) => Some(RejectionReason::ShiftNotOpen { current: state }),
            (
                Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
                OpenedLocalPendingDrain,
                Offline,
            ) => None,
            (
                Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
                OpenedLocalPendingDrain,
                Online,
            ) => Some(RejectionReason::ShiftOpenPendingDrainOpRefused),
            (
                Sell | Return | ServiceIn | ServiceOut | CashWithdrawal | XReport,
                ClosingLocalPendingDrain,
                _,
            ) => Some(RejectionReason::PostLocalCloseSaleRefused),
            _ => unreachable!(
                "matrix oracle: unhandled cell ({doc:?}, {state:?}, {ch:?})"
            ),
        }
    }

    #[test]
    fn check_shift_guard_matches_oracle_for_all_162_cells() {
        let mut total = 0usize;
        let mut none_count = 0usize;
        let mut some_count = 0usize;
        for &doc in ALL_DOC_TYPES {
            for &state in ALL_SHIFT_STATES {
                for &ch in ALL_CHANNELS {
                    let actual = check_shift_guard(doc, state, ch);
                    let expected = expected_outcome(doc, state, ch);
                    assert_eq!(
                        actual, expected,
                        "cell ({doc:?}, {state:?}, {ch:?}): actual {actual:?} != expected {expected:?}"
                    );
                    total += 1;
                    match actual {
                        Some(_) => some_count += 1,
                        None => none_count += 1,
                    }
                }
            }
        }
        assert_eq!(
            total, 162,
            "matrix MUST cover 9 doc_types × 9 shift_states × 2 channels = 162"
        );
        // Drift-guard: pin the absolute None vs Some split to catch
        // accidental matrix widening / narrowing.
        // Allowed (None) cells:
        // - (ShiftOpen, Closed, _) → 2
        // - (ShiftClose, Opened, _) → 2
        // - (ZReport, Opened, _) → 2
        // - (Sell|Return|ServiceIn|ServiceOut|CashWithdrawal|XReport, Opened, _) → 6×2 = 12
        // - (Sell|Return|ServiceIn|ServiceOut|CashWithdrawal|XReport, OpenedLocalPendingDrain, Offline) → 6×1 = 6
        // Total None = 24; Some = 162 - 24 = 138.
        assert_eq!(none_count, 24, "spec §3.4: 24 happy-path None cells");
        assert_eq!(some_count, 138, "spec §3.4: 138 refusal cells");
    }

    #[test]
    fn matrix_constants_have_exactly_expected_arity() {
        // Drift guard: if a new DocType / ShiftState lands, the
        // matrix above MUST be updated.  Pinning arity here breaks
        // the matrix test loud + early.
        assert_eq!(ALL_DOC_TYPES.len(), 9, "M3b W14a-1: 9 doc types");
        assert_eq!(ALL_SHIFT_STATES.len(), 9, "M3b W14a-1: 9 shift states");
        assert_eq!(ALL_CHANNELS.len(), 2, "W14a-2b Commit 4: Online / Offline");
    }
}
