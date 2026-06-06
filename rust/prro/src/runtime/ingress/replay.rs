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

/// Build the truthful success response from a terminal-accepted fiscal
/// document.  `fiscal_id`/`fiscal_ts` are present only with a DPS id
/// (online `ACK`); an `OFFLINE_LOCAL_ACK` has neither yet (→ `null`).
fn completed(request_id: &str, fd: &TerminalOutcome) -> CanonicalResponse {
    let (sale_total_kopecks, return_total_kopecks) = match fd.doc_type {
        DocType::Sell => (fd.total_sum_kop.unwrap_or(0).max(0) as u64, 0),
        DocType::Return => (0, fd.total_sum_kop.unwrap_or(0).max(0) as u64),
        _ => (0, 0),
    };
    // fiscal_ts is the DPS fiscalization stamp — only meaningful when a
    // DPS fiscal id exists; for an offline-local-ack there is none yet.
    let fiscal_ts = fd.server_fiscal_no.as_ref().map(|_| fd.business_ts.clone());
    CanonicalResponse {
        ok: true,
        request_id: request_id.to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        document_id: fd.document_id.clone(),
        fiscal_id: fd.server_fiscal_no.clone(),
        fiscal_ts,
        document_state: fd.state.as_str().to_string(),
        sale_total_kopecks,
        return_total_kopecks,
        report_xml: None,
    }
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
        ("DONE", Some(o)) if is_accepted(o.state) => {
            ReplayResolution::Completed(completed(&rid, &o))
        }
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
        (_, Some(o)) if is_accepted(o.state) => ReplayResolution::Completed(completed(&rid, &o)),

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

    #[test]
    fn completed_offline_local_ack_has_null_fiscal_id() {
        let fd = TerminalOutcome {
            document_id: "abcd".to_string(),
            state: DocState::OfflineLocalAck,
            doc_type: DocType::Sell,
            server_fiscal_no: None,
            business_ts: "2026-06-06T00:00:00Z".to_string(),
            total_sum_kop: Some(15000),
        };
        let r = completed("rid", &fd);
        assert!(r.ok);
        assert_eq!(r.document_state, "OFFLINE_LOCAL_ACK");
        assert_eq!(r.fiscal_id, None, "offline-local-ack has no DPS id");
        assert_eq!(r.fiscal_ts, None);
        assert_eq!(r.sale_total_kopecks, 15000);
    }

    #[test]
    fn completed_online_ack_carries_fiscal_id_and_ts() {
        let fd = TerminalOutcome {
            document_id: "ef01".to_string(),
            state: DocState::Ack,
            doc_type: DocType::Return,
            server_fiscal_no: Some("12345".to_string()),
            business_ts: "2026-06-06T01:00:00Z".to_string(),
            total_sum_kop: Some(3000),
        };
        let r = completed("rid", &fd);
        assert_eq!(r.fiscal_id.as_deref(), Some("12345"));
        assert_eq!(r.fiscal_ts.as_deref(), Some("2026-06-06T01:00:00Z"));
        assert_eq!(r.document_state, "ACK");
        assert_eq!(r.return_total_kopecks, 3000);
        assert_eq!(r.sale_total_kopecks, 0);
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
