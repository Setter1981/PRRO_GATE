//! RS-2 piece-4b — the ingress replay resolver.
//!
//! When `ingress_inbox::insert` returns `Replay` (the `(fn, idem_key)`
//! was already seen) or `Conflict`, the handler can NOT re-fiscalize.
//! This module turns that outcome into a TRUTHFUL response by joining the
//! **inbox status** with the **terminal fiscal-document state** for the
//! same `request_id` (the JOINT matrix, review H3).  All responses are
//! built from the piece-4a DTOs ([`CanonicalResponse`] /
//! [`CanonicalErrorResponse`]) with the hex [`request_id_to_string`] and
//! nullable `fiscal_id`/`fiscal_ts`.
//!
//! Read-only: it reads `fiscal_documents` (and is handed the already-read
//! inbox row) — NEVER inside a write transaction (invariant #1), NEVER a
//! write-path stage.
//!
//! Matrix (operator-locked acceptance):
//!   - inbox `REJECTED`/`ERROR` → `Failed` (typed error).
//!   - inbox `DONE` → MUST find a terminal-accepted fiscal doc; missing or
//!     non-accepted (drift) → `Failed` (do NOT trust the inbox alone).
//!   - inbox `NEW`/`PROCESSING` + fd accepted (`ACK`/`OFFLINE_LOCAL_ACK`) →
//!     `Completed` (incl. offline: `OFFLINE_LOCAL_ACK` is a client-terminal
//!     ACCEPTED state, NOT a failure).
//!   - inbox `NEW`/`PROCESSING` + fd terminally failed → `Failed`.
//!   - inbox `NEW`/`PROCESSING` + no fd / fd still in-flight → `InProgress`
//!     (deterministic retry; NEVER a fake success).

use super::dto::{request_id_to_string, CanonicalErrorResponse, CanonicalResponse, SCHEMA_VERSION};
use crate::db::models::enums::{DocState, DocType};
use crate::db::repositories::fiscal_documents::{self, TerminalOutcome};
use crate::db::repositories::ingress_inbox::InboxRow;
use sqlx::SqlitePool;

/// How a replayed (or conflicting) submission resolves into a response.
/// Every variant carries a piece-4a DTO; the handler (piece-5) maps the
/// variant to an HTTP status (Completed→2xx, InProgress→202/425,
/// Failed→4xx/5xx).
#[derive(Debug, Clone, PartialEq)]
pub enum ReplayResolution {
    /// The receipt is terminal-accepted — the truthful success response
    /// (from the fiscal document, NOT a re-fiscalization).
    Completed(CanonicalResponse),
    /// The receipt is still being processed (inbox NEW/PROCESSING, no
    /// terminal fiscal doc).  A deterministic "retry", NOT a fake success.
    InProgress(CanonicalErrorResponse),
    /// The receipt terminally failed, or the inbox/ledger drifted.
    Failed(CanonicalErrorResponse),
}

fn is_accepted(state: DocState) -> bool {
    matches!(state, DocState::Ack | DocState::OfflineLocalAck)
}

fn is_terminally_failed(state: DocState) -> bool {
    matches!(
        state,
        DocState::Rejected | DocState::Cancelled | DocState::RequiresManualReconciliation
    )
}

fn error(request_id: &str, error_code: &str, error_message: String) -> CanonicalErrorResponse {
    CanonicalErrorResponse {
        ok: false,
        request_id: request_id.to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        error_code: error_code.to_string(),
        error_message,
        config_drift: false,
    }
}

/// Build the response for an ACCEPTED fiscal document (`Ack` /
/// `OfflineLocalAck`), **FAIL-CLOSED** on a malformed accepted row (which
/// would otherwise be a fake success):
///   - `Ack` MUST carry a non-empty `server_fiscal_no` (the DPS id);
///     missing/empty → `INBOX_LEDGER_DRIFT` (an ACK without a DPS id is a
///     corrupt ledger row, NOT an `ok:true` with `fiscal_id:null`).
///   - `Sell`/`Return` MUST carry a `total_sum_kop >= 0`; missing/negative
///     → drift (do not mask a corrupt total as 0 / a wrong-sum success).
///
/// `fiscal_id` = the DPS id (`None` only for `OfflineLocalAck`, which has
/// none yet); `fiscal_ts` = `first_kvt1_at` (the truthful DPS-confirmed-at
/// stamp; `None` for offline-local-ack, which never reaches KVT1).
fn build_accepted(request_id: &str, fd: &TerminalOutcome) -> ReplayResolution {
    let fiscal_id = match fd.state {
        DocState::Ack => match fd.server_fiscal_no.as_deref() {
            Some(s) if !s.is_empty() => Some(s.to_string()),
            _ => {
                return ReplayResolution::Failed(error(
                    request_id,
                    "INBOX_LEDGER_DRIFT",
                    "fiscal document is ACK but has no server_fiscal_no".to_string(),
                ))
            }
        },
        // OfflineLocalAck (the only other accepted state) → no DPS id yet.
        _ => None,
    };

    let (sale_total_kopecks, return_total_kopecks) = match fd.doc_type {
        DocType::Sell | DocType::Return => {
            let total = match fd.total_sum_kop {
                Some(t) if t >= 0 => t as u64,
                _ => {
                    return ReplayResolution::Failed(error(
                        request_id,
                        "INBOX_LEDGER_DRIFT",
                        format!(
                            "accepted {} fiscal document has missing/negative total_sum_kop",
                            fd.doc_type.as_str()
                        ),
                    ))
                }
            };
            if matches!(fd.doc_type, DocType::Sell) {
                (total, 0)
            } else {
                (0, total)
            }
        }
        _ => (0, 0),
    };

    ReplayResolution::Completed(CanonicalResponse {
        ok: true,
        request_id: request_id.to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        document_id: fd.document_id.clone(),
        fiscal_id,
        fiscal_ts: fd.first_kvt1_at.clone(),
        document_state: fd.state.as_str().to_string(),
        sale_total_kopecks,
        return_total_kopecks,
        // Q4 report_xml on a COMPLETED ZReport/ShiftClose REPLAY is
        // deferred to piece-5/RS-3: it must read the stored Z XML from
        // `document_files` (the SAME kind RS-3 emits in the first-pass
        // `seam::FiscalOutcome.report_xml`, for first-pass↔replay parity)
        // — see plan §0.4. Pre-RS-3 both paths are gated by the
        // NotImplemented seam, so `None` here manifests no gap yet.
        report_xml: None,
    })
}

/// Resolve a `Replay` inbox outcome (the already-read row) into a
/// response, joining inbox status with the terminal fiscal-document
/// state.  Read-only; the only DB access is the `fiscal_documents` read.
pub async fn resolve_replay(
    replayed: &InboxRow,
    main_pool: &SqlitePool,
) -> sqlx::Result<ReplayResolution> {
    let rid = request_id_to_string(&replayed.request_id);

    // Inbox terminally-failed states short-circuit (acceptance #6).
    if matches!(replayed.status.as_str(), "REJECTED" | "ERROR") {
        return Ok(ReplayResolution::Failed(error(
            &rid,
            "INBOX_REJECTED",
            format!("prior submission ended in inbox status {}", replayed.status),
        )));
    }

    // DONE / NEW / PROCESSING all consult the ledger — the inbox status is
    // never trusted on its own (acceptance #4).
    let fd =
        fiscal_documents::terminal_outcome_by_request_id(main_pool, &replayed.request_id).await?;

    let resolution = match (replayed.status.as_str(), fd) {
        // DONE must be backed by a terminal-accepted fiscal doc.
        ("DONE", Some(o)) if is_accepted(o.state) => build_accepted(&rid, &o),
        ("DONE", found) => ReplayResolution::Failed(error(
            &rid,
            "INBOX_LEDGER_DRIFT",
            match found {
                Some(o) => format!(
                    "inbox DONE but fiscal document is {} (not terminal-accepted)",
                    o.state.as_str()
                ),
                None => "inbox DONE but no fiscal document for this request_id".to_string(),
            },
        )),

        // NEW/PROCESSING: an accepted ledger doc means it IS complete
        // (incl. OFFLINE_LOCAL_ACK — a client-terminal accepted state).
        (_, Some(o)) if is_accepted(o.state) => build_accepted(&rid, &o),

        // A terminally-failed ledger doc.
        (_, Some(o)) if is_terminally_failed(o.state) => ReplayResolution::Failed(error(
            &rid,
            "FISCAL_REJECTED",
            format!("fiscal document terminally failed: {}", o.state.as_str()),
        )),

        // No ledger doc yet, or it is still in-flight → genuinely pending.
        _ => ReplayResolution::InProgress(error(
            &rid,
            "IN_PROGRESS",
            "the submission is still being processed; retry".to_string(),
        )),
    };
    Ok(resolution)
}

/// Build the response for an inbox `Conflict` (same `idempotency_key`,
/// different payload).  MED-2: flagged `config_drift = true` so an
/// operator payment-slot rename (a benign payload-hash change) reads as
/// config drift, NOT tampering.
pub fn conflict_response(request_id: &[u8; 16]) -> CanonicalErrorResponse {
    CanonicalErrorResponse {
        ok: false,
        request_id: request_id_to_string(request_id),
        schema_version: SCHEMA_VERSION.to_string(),
        error_code: "IDEMPOTENCY_CONFLICT".to_string(),
        error_message: "a different payload was already submitted under this idempotency_key; \
                        this may be benign payment-method config drift (e.g. a slot rename), \
                        not tampering"
            .to_string(),
        config_drift: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::enums::Protocol;

    fn row(request_id: [u8; 16], status: &str) -> InboxRow {
        InboxRow {
            request_id,
            fiscal_number: "4000000001".to_string(),
            protocol: Protocol::Rest,
            operation_type: "SELL".to_string(),
            idempotency_key: "k".to_string(),
            status: status.to_string(),
            payload_json: "{}".to_string(),
            payload_sha256_canonical: [0u8; 32],
            correlation_id: None,
            received_at: "2026-06-06T00:00:00Z".to_string(),
            signed_by_cashier_id: None,
            driver_id: Some("drv-test".to_string()),
        }
    }

    #[test]
    fn conflict_is_labeled_config_drift_not_tampering() {
        let e = conflict_response(&[9u8; 16]);
        assert!(!e.ok);
        assert_eq!(e.error_code, "IDEMPOTENCY_CONFLICT");
        assert!(
            e.config_drift,
            "MED-2 — Conflict must be labeled config_drift"
        );
        // request_id rendered as hex.
        assert_eq!(e.request_id, "09090909090909090909090909090909");
    }

    fn outcome(
        state: DocState,
        doc_type: DocType,
        server_fiscal_no: Option<&str>,
        first_kvt1_at: Option<&str>,
        total_sum_kop: Option<i64>,
    ) -> TerminalOutcome {
        TerminalOutcome {
            document_id: "abcd".to_string(),
            state,
            doc_type,
            server_fiscal_no: server_fiscal_no.map(str::to_string),
            first_kvt1_at: first_kvt1_at.map(str::to_string),
            total_sum_kop,
        }
    }

    #[test]
    fn accepted_offline_local_ack_completed_null_fiscal_id_and_ts() {
        let fd = outcome(
            DocState::OfflineLocalAck,
            DocType::Sell,
            None,
            None,
            Some(15000),
        );
        match build_accepted("rid", &fd) {
            ReplayResolution::Completed(r) => {
                assert!(r.ok);
                assert_eq!(r.document_state, "OFFLINE_LOCAL_ACK");
                assert_eq!(r.fiscal_id, None, "offline-local-ack has no DPS id");
                assert_eq!(r.fiscal_ts, None, "offline-local-ack never reaches KVT1");
                assert_eq!(r.sale_total_kopecks, 15000);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn accepted_online_ack_carries_fiscal_id_and_first_kvt1_at() {
        // fiscal_ts must come from first_kvt1_at, NOT business_ts.
        let fd = outcome(
            DocState::Ack,
            DocType::Return,
            Some("12345"),
            Some("2026-06-06T01:00:00Z"),
            Some(3000),
        );
        match build_accepted("rid", &fd) {
            ReplayResolution::Completed(r) => {
                assert_eq!(r.fiscal_id.as_deref(), Some("12345"));
                assert_eq!(r.fiscal_ts.as_deref(), Some("2026-06-06T01:00:00Z"));
                assert_eq!(r.document_state, "ACK");
                assert_eq!(r.return_total_kopecks, 3000);
                assert_eq!(r.sale_total_kopecks, 0);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    /// External-review High: an `Ack` with no `server_fiscal_no` is ledger
    /// drift, NOT an `ok:true` success with `fiscal_id:null`.
    #[test]
    fn accepted_ack_without_server_fiscal_no_is_drift() {
        for sfn in [None, Some("")] {
            let fd = outcome(DocState::Ack, DocType::Sell, sfn, Some("t"), Some(15000));
            match build_accepted("rid", &fd) {
                ReplayResolution::Failed(e) => assert_eq!(e.error_code, "INBOX_LEDGER_DRIFT"),
                other => panic!("ACK w/o server_fiscal_no must be drift, got {other:?}"),
            }
        }
    }

    /// External-review Medium: an accepted SELL/RETURN with a missing
    /// total is drift, NOT a fake-0 success.
    #[test]
    fn accepted_sell_without_total_is_drift() {
        let fd = outcome(DocState::Ack, DocType::Sell, Some("12345"), Some("t"), None);
        match build_accepted("rid", &fd) {
            ReplayResolution::Failed(e) => assert_eq!(e.error_code, "INBOX_LEDGER_DRIFT"),
            other => panic!("SELL w/o total must be drift, got {other:?}"),
        }
    }

    /// Pure-branch: inbox REJECTED short-circuits to Failed without an fd
    /// read (we can't reach the DB here, so a status that returns BEFORE
    /// the read is the only one unit-testable without a pool — the JOINT
    /// fd-backed branches are covered by the integration test).
    #[tokio::test]
    async fn inbox_rejected_is_failed() {
        // A throwaway in-memory pool is not needed: REJECTED returns
        // before the ledger read.  Use a pool that would error if touched.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let res = resolve_replay(&row([1u8; 16], "REJECTED"), &pool)
            .await
            .unwrap();
        match res {
            ReplayResolution::Failed(e) => assert_eq!(e.error_code, "INBOX_REJECTED"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
