//! M3b W7a — `stage_offline_ack`: Pattern C step 1 (pre-send local ack).
//!
//! Post-sign, pre-send write-path branch invoked when the node is in
//! `Offline` / `GoingOffline` mode.  In ONE `with_immediate`
//! envelope (BEGIN IMMEDIATE) the stage:
//!
//!   1. Re-reads `node_state` (fresh snapshot; any earlier
//!      WorkerContext copy may be stale w.r.t. a concurrent
//!      shift-close or node-mode flip).
//!   2. Validates node mode ∈ {Offline, GoingOffline}.
//!   3. Validates shift state == Opened.
//!   4. Reads the FN's currently-active OPEN offline session.
//!   5. **Pre-checks doc state + FN match** (operator W7 Round 1
//!      LOW/MED fix, 2026-05-15): reads the doc's `(state,
//!      fiscal_number)` and surfaces typed `Refused(DocNotFound)`
//!      / `Refused(DocStateConflict)` BEFORE any code touch.  Also
//!      doubles as the cross-FN guard (operator W7 Round 1 HIGH-2
//!      fix) — caller-supplied `fiscal_number` MUST match the
//!      doc's persisted FN.
//!   6. Acquires an unconsumed code from the FN pool via W5's
//!      `acquire_code_tx` (atomic single-statement CAS).
//!   7. Transitions `Signed → OfflineLocalAck` stamping
//!      `offline_fiscal_no = code_lnd`,
//!      `offline_fiscal_date = consumed_at`,
//!      `offline_session_id = session_id` in one UPDATE (W7 helper
//!      `transition_to_offline_local_ack_tx`).  The helper's
//!      UPDATE WHERE clause filters on `fiscal_number` too —
//!      defence-in-depth on the pre-check.
//!   8. Emits `OFFLINE_LOCAL_ACK_APPLIED` audit on success;
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
//!   doc in `OfflineLocalAck`; Step 5 pre-check catches it BEFORE
//!   `acquire_code_tx`, returns typed `Refused(DocStateConflict)`,
//!   audit emits `OFFLINE_ACK_REFUSED`; the original `Applied`
//!   artefact stays intact AND the code pool is never touched on
//!   the retry.  Three defence-in-depth layers: (a) Step 5 pre-
//!   check, (b) helper UPDATE WHERE state='SIGNED', (c) W4 partial
//!   UNIQUE `ux_offline_codes_consumed_by_doc`.
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
//! - W7a (this module) emits `OFFLINE_LOCAL_ACK`; it does NOT
//!   advance docs to `Sending` / `Cancelled`.  Those W6-added
//!   edges are W9's responsibility (return-online backlog drain).
//! - `stage_finalize` and `stage_send` are untouched.
//! - This stage takes `(pool, doc_id, fiscal_number)` directly;
//!   it does NOT extend `WorkerContext`.
//! - **W7b (mandatory follow-up)**: dispatcher wiring that calls
//!   this stage from ingress + boot_phase + app paths, plus the
//!   `write_path_dispatcher_post_sign` integration test per plan
//!   §Task 7 line 524.  W7a delivers the stage primitive in
//!   isolation; "M3b §Task 7 closed" requires both W7a (this PR)
//!   AND W7b merged.

use crate::db::models::enums::{DocState, DocType, NodeMode, Severity, ShiftState};
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
/// `DocStateConflict` / `CrossFnMismatch` / `DocNotFound` are
/// pre-check failures, carrying observed values for audit
/// forensics (operator W7 Round 2 LOW-4 fix, 2026-05-16).
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
    /// Step 5 pre-check found the doc in a state other than
    /// `Signed` (FN matches the caller's FN).  Concurrent state
    /// change (e.g., admin override) or idempotent replay —
    /// caller may re-route the doc per the observed state.
    /// `observed_state` is the SQLite-text form (e.g.,
    /// `"OFFLINE_LOCAL_ACK"`, `"PREPARED"`).
    DocStateConflict { observed_state: String },
    /// Step 5 pre-check found the doc but with a different
    /// `fiscal_number` than the caller passed.  Operationally a
    /// caller bug (mis-passed FN) or, worse, an attempt at cross-
    /// FN attribution.  Distinct variant from `DocStateConflict`
    /// per operator W7 Round 2 LOW-4 — fiscal integrity violation
    /// deserves its own audit signal.
    CrossFnMismatch { observed_fiscal_number: String },
    /// Step 5 pre-check did not find the doc row.  Programming
    /// bug or DB corruption — the doc should exist because stage 3
    /// just signed it.
    DocNotFound,
    /// Step 6 `acquire_code_tx` found NO available offline code for
    /// the FN (the pool is exhausted).  A LEGITIMATE operational
    /// refusal (not a race/structural bug) — surfaced as a TYPED
    /// `Refused` instead of the former raw-anyhow Err so the caller
    /// aborts the doc + maps a precise `OFFLINE_CODE_POOL_EXHAUSTED`
    /// refusal rather than the catch-all `DISPATCH_INTERNAL`.  The
    /// code pool is left untouched (the CAS acquired nothing).
    CodePoolExhausted,
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

            // ─── Step 2b: read doc inputs (W14a-2b Commit 6 §3.7) ───
            //
            // Defence-in-depth (operator correction #4): read doc_type
            // + state + fiscal_number BEFORE shift-state validation so
            // we can scope the widened {Opened | OpenedLocalPendingDrain}
            // allowlist by doc_type — non-bypass regular fiscal docs
            // get the widened set; other doc types stay scoped to
            // Opened only.  Without this, an arbitrary upstream caller
            // could route a non-fiscal doc through the offline-ack
            // path on OpenedLocalPendingDrain — defeating the §3.4
            // matrix contract.
            //
            // Returned `inputs.state` is the pre-CAS observation;
            // surfaces typed `DocStateConflict` if not SIGNED.
            let inputs = match fd::fetch_offline_ack_inputs_tx(tx, doc_id).await? {
                Some(i) => i,
                None => {
                    return audit_and_return_refused(
                        tx,
                        doc_id,
                        &fn_id,
                        RefusalReason::DocNotFound,
                    )
                    .await;
                }
            };
            // Cross-FN guard (operator W7 Round 1 HIGH-2 + Round 2
            // LOW-4): caller passed FN_B but doc belongs to FN_A.
            if inputs.fiscal_number != fn_id {
                return audit_and_return_refused(
                    tx,
                    doc_id,
                    &fn_id,
                    RefusalReason::CrossFnMismatch {
                        observed_fiscal_number: inputs.fiscal_number,
                    },
                )
                .await;
            }
            // Doc-state guard: only SIGNED docs proceed to offline-ack.
            if inputs.state != DocState::Signed {
                return audit_and_return_refused(
                    tx,
                    doc_id,
                    &fn_id,
                    RefusalReason::DocStateConflict {
                        observed_state: inputs.state.as_str().to_string(),
                    },
                )
                .await;
            }

            // ─── Step 3: validate shift state (doc_type-scoped) ─────
            //
            // W14a-2b Commit 6 §3.7 widening (operator correction #4):
            // regular fiscal docs (Sell / Return / ServiceIn /
            // ServiceOut / CashWithdrawal / XReport) accept
            // `Opened | OpenedLocalPendingDrain` per spec §3.3 +
            // §5.6 — Pattern C resilience surface.  Other doc types
            // (SHIFT_OPEN — handled here for offline Pattern C open;
            // others should not reach stage_offline_ack per the §3.4
            // matrix, but the helper enforces it defensively) stay
            // scoped to `Opened` only.
            let shift_state_ok = match inputs.doc_type {
                DocType::Sell
                | DocType::Return
                | DocType::ServiceIn
                | DocType::ServiceOut
                | DocType::CashWithdrawal
                | DocType::XReport => matches!(
                    ns.shift_state,
                    ShiftState::Opened | ShiftState::OpenedLocalPendingDrain,
                ),
                // A′.3 PR-O3 edge-2 tail: an offline SHIFT_OPEN doc local-acks
                // in the state edge 2 leaves the shift in
                // (`OpenedLocalPendingDrain`), consuming an offline code like a
                // receipt (numbering (ii)).
                DocType::ShiftOpen => ns.shift_state == ShiftState::OpenedLocalPendingDrain,
                // SHIFT_CLOSE / Z_REPORT (edges 7/9 — PR-O3 slice 2) and any
                // other doc type stay scoped to Opened only here — non-regular
                // fiscal docs do NOT participate in the §3.7 widening.
                _ => ns.shift_state == ShiftState::Opened,
            };
            if !shift_state_ok {
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

            // ─── Step 6: acquire code (atomic single-statement CAS) ─
            //
            // Validations + pre-check all passed.  This is the first
            // point a write touches `offline_codes` — any earlier
            // refusal returned BEFORE this call, so code pool stays
            // intact on refusal.  An EXHAUSTED pool is a LEGITIMATE
            // operational refusal: surface it as a TYPED
            // `Refused(CodePoolExhausted)` (committing the refusal
            // audit, code pool untouched) so the inline orchestrator
            // ABORTS the SIGNED doc + maps a precise
            // `OFFLINE_CODE_POOL_EXHAUSTED` refusal — NOT the former
            // raw-anyhow Err that rolled back into the catch-all
            // `DISPATCH_INTERNAL` and orphaned the SIGNED doc.  Any
            // OTHER (structural) error still propagates as `Err`.
            let acquired = match offline_sessions::acquire_code_tx(tx, &fn_id, doc_id).await {
                Ok(a) => a,
                Err(offline_sessions::OfflineSessionError::CodePoolExhausted { .. }) => {
                    return audit_and_return_refused(
                        tx,
                        doc_id,
                        &fn_id,
                        RefusalReason::CodePoolExhausted,
                    )
                    .await;
                }
                Err(e) => return Err(e.into()),
            };

            // ─── Step 7: transition Signed → OfflineLocalAck ────────
            //
            // Single UPDATE stamps state + offline_fiscal_no +
            // offline_fiscal_date + offline_session_id atomically
            // (operator W7 criterion 5).  Helper now also takes
            // `fiscal_number` and filters on it in the WHERE clause
            // (operator W7 Round 1 HIGH-2 fix).
            let outcome = fd::transition_to_offline_local_ack_tx(
                tx,
                doc_id,
                &fn_id,
                acquired.code_lnd,
                &acquired.consumed_at,
                session_id,
            )
            .await?;

            match outcome {
                TransitionOutcome::Applied => {
                    // ─── Step 7b: advance the MAC chain seed (M2-01) ───────
                    //
                    // M2-01 (Fable-locked, 2026-06-12): an `OfflineLocalAck` IS
                    // a local fiscalisation — the receipt is legally issued now;
                    // drain is deferred *transmission*, not re-fiscalisation.
                    // So the chain seed must advance HERE (the ONLY seed-advance
                    // for offline-origin docs — `stage_finalize` skips it for
                    // them).  In the SAME envelope as the CAS: read the doc's
                    // signed-chain fields, assert the seed has not drifted since
                    // sign (under the per-FN write gate nothing moves it between
                    // sign and offline-ack; a mismatch is a real drift →
                    // fail-closed, symmetric with `stage_finalize` §3), then
                    // advance the seed to this doc's `unsigned_xml_sha256` so the
                    // NEXT offline receipt of the session chains off it.  Without
                    // this every offline receipt signs the same `previous_hash`
                    // → broken legal chain on the wire + an undrainable 2+ doc
                    // session.  (Review fold 2026-06-12: the chain fields come
                    // from the same envelope's input fetch
                    // `fetch_offline_ack_inputs_tx`, read pre-CAS while the doc is
                    // SIGNED.  They are write-once at stage_sign and unchanged by
                    // the Signed→OfflineLocalAck CAS, so this is byte-for-byte
                    // equivalent to the prior post-CAS sibling reads.)
                    let prev = inputs.previous_hash;
                    let unsigned = inputs.unsigned_xml_sha256;
                    anyhow::ensure!(
                        ns.last_known_unsigned_xml_sha256.as_ref().map(|h| h.as_slice())
                            == prev.as_deref(),
                        "stage_offline_ack: chain-seed drift for doc {doc_id:?} — node_state seed \
                         != doc.previous_hash (offline issuance must extend the chain from the last \
                         issued doc); refusing to advance"
                    );
                    let unsigned_arr: [u8; 32] = unsigned
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "stage_offline_ack: SIGNED doc {doc_id:?} missing unsigned_xml_sha256"
                            )
                        })?
                        .as_slice()
                        .try_into()
                        .map_err(|_| {
                            anyhow::anyhow!(
                                "stage_offline_ack: doc {doc_id:?} unsigned_xml_sha256 is not 32 bytes"
                            )
                        })?;
                    if !node_state::update_last_known_xml_sha_tx(tx, &fn_id, &unsigned_arr).await? {
                        anyhow::bail!(
                            "stage_offline_ack: node_state row vanished mid-envelope for FN {fn_id}"
                        );
                    }

                    // ─── Step 8: emit Applied audit ─────────────────
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
                    // Whitelist gate failed — would only happen if
                    // a future PR removed `(Signed, OfflineLocalAck)`
                    // from `allowed_transition` without updating
                    // this stage.  Operator W7 Round 1 HIGH-3 fix:
                    // the helper now actually returns this in
                    // release builds (was `debug_assert!` no-op
                    // previously).  Bail to surface the regression
                    // loud.
                    anyhow::bail!(
                        "(Signed, OfflineLocalAck) whitelist gate returned Forbidden — W6 locked-edge test regression?"
                    )
                }
                TransitionOutcome::Conflict => {
                    // Defence-in-depth: pre-check (step 5) caught
                    // state/FN mismatch already, so we should not
                    // see this in single-writer BEGIN IMMEDIATE
                    // semantics.  If we do, it's a real bug —
                    // bail to roll back the envelope (code stays
                    // unconsumed; I5 preserved).
                    anyhow::bail!(
                        "stage_offline_ack: CAS Conflict after pre-check passed — \
                         logic bug or external write to fiscal_documents mid-envelope; \
                         tx rollback to preserve I5"
                    )
                }
                TransitionOutcome::NotFound => {
                    // Same as Conflict: pre-check (step 5) caught
                    // missing row already.  Defence-in-depth bail.
                    anyhow::bail!(
                        "stage_offline_ack: CAS NotFound after pre-check passed — \
                         doc row vanished mid-envelope; tx rollback to preserve I5"
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
