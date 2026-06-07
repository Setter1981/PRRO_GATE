//! RS-2 piece-3 — the write-path SEAM.
//!
//! The fixed contract between the **ingress handler** (RS-2, piece-4/5)
//! and the future **live write-path worker** (RS-3).  After RS-2 accepts
//! a receipt, converts it (piece-2), and writes the inbox, it hands the
//! persisted [`InboxRow`] to a [`WritePathEntry`] to fiscalize it
//! **inline-synchronously** and return what the handler needs to build
//! the response envelope.
//!
//! Until RS-3 lands, the production binding is [`UnimplementedWritePath`],
//! whose [`fiscalize`](WritePathEntry::fiscalize) ALWAYS returns a typed
//! [`FiscalError::NotImplemented`] — **never a silent success**.  The
//! handler maps that to a non-2xx response (no phantom 200 with an empty
//! `fiscal_id`).
//!
//! **Pure contract.** This module must NOT call `stage_acquire` /
//! `stage_sign` / any write-path stage, nor touch the DB — that wiring is
//! RS-3.  It exists so RS-2 is mergeable and the ingress→write-path
//! boundary is frozen.
//!
//! **Z-quiescence room (review MEDIUM, RS-3 sequencing).** For
//! `ShiftClose` / `ZReport`, RS-3 may perform the ledger Z-aggregation
//! (piece-2b `aggregate_zreport`) INSIDE `fiscalize`, AFTER its
//! quiescence/drain barrier — so the handler need NOT pre-aggregate the Z
//! before the seam.  Keeping the seam input the raw [`InboxRow`] (not a
//! pre-signed payload) preserves that option; the seam is the
//! single-writer entrypoint where the "finalize/drain pending shift docs
//! before the Z aggregates" obligation becomes enforceable.
//!
//! **Identity + amounts come FROM THE PERSISTED ROW (A-H1 / migrations
//! 021+022).** The real RS-3 impl MUST build its `CanonicalFiscalCommand`
//! from [`InboxRow`] ALONE — never the listener/runtime context — so the
//! inline first-pass and a crash-recovery reaper that re-drives a stuck
//! `PROCESSING` row use the SAME source.  The row carries every field
//! `stage_acquire`/`stage_sign` consume: `doc_type` (← `operation_type`),
//! `payload_json` + `payload_sha256_canonical`, `driver_id`,
//! `signed_by_cashier_id`, `business_ts`, `total_sum_kop`.
//!
//! **Null-handling contract (RS-3 MUST enforce BEFORE `stage_acquire`, so a
//! malformed row is rejected/audited, NOT driven into a `PREPARED`
//! `fiscal_documents` row that fails late at signing):**
//!   - `driver_id` — REQUIRED for every processed row.  Missing (a pre-021
//!     legacy row) → a missing driver silently identity-maps tax groups (the
//!     W4-Z2a non-identity hazard); MUST NOT be defaulted.
//!   - `business_ts` — REQUIRED for every processed row.  It is the receipt
//!     timestamp (→ DPS Kyiv-local epoch); missing (a pre-022 legacy row) →
//!     reject, do NOT re-mint a fresh `now()` (that stamps the recovery time,
//!     not the sale time).
//!   - `total_sum_kop` — REQUIRED for SELL / RETURN (the stage_sign sum
//!     cross-check); NULL is valid ONLY for no-total doc types (SHIFT_OPEN /
//!     Z).  A SELL/RETURN row with NULL `total_sum_kop` → reject.
//!   - `signed_by_cashier_id` — optional (a command legitimately has no
//!     cashier); `None` is accepted as-is.

use crate::db::models::enums::DocState;
use crate::db::models::ids::DocumentId;
use crate::db::repositories::ingress_inbox::InboxRow;
use async_trait::async_trait;
use thiserror::Error;

/// What fiscalizing an inbox-accepted receipt produced — enough for the
/// handler to render the response envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalOutcome {
    /// Gateway-internal document id, **typed** — this is the INTERNAL
    /// ingress↔RS-3 contract, so it carries the canonical [`DocumentId`]
    /// (not a string).  The piece-4 handler serialises it to a string for
    /// the external `CanonicalResponse`; keeping it typed here stops RS-3
    /// from returning a non-canonical id and stops string formatting from
    /// leaking into the write-path.
    pub document_id: DocumentId,
    /// DPS-assigned fiscal id once known (`server_fiscal_no`).  `None`
    /// for an offline-local-acked receipt that has no DPS id yet.
    pub fiscal_id: Option<String>,
    /// Fiscalization timestamp (ISO-8601) once known.
    pub fiscal_ts: Option<String>,
    /// Terminal/durable document state (e.g. `Ack`, `OfflineLocalAck`).
    pub document_state: DocState,
    /// Q4 — raw Z-report XML for WebCheck, produced at RS-3's
    /// sign/build-artifact boundary.  PASSTHROUGH only: RS-2 never builds
    /// it, so it is `None` for every non-Z doc and for all docs pre-RS-3.
    pub report_xml: Option<String>,
}

/// Why fiscalization could not complete.  Every variant carries enough
/// context (at minimum the `request_id`) for the handler to build a
/// typed, non-2xx error envelope WITHOUT losing the request identity.
#[derive(Debug, Error)]
pub enum FiscalError {
    /// RS-3 is not yet wired — the entrypoint is [`UnimplementedWritePath`].
    /// The handler MUST map this to a non-2xx (e.g. 501/503), NEVER a 2xx
    /// success.  Carries `request_id` so the response can still reference
    /// the submitted command.
    #[error("write-path not yet implemented (RS-3 pending); request_id={request_id:02x?}")]
    NotImplemented { request_id: [u8; 16] },
    // RS-3 will add real failure variants (ShiftNotOpen, SignFailure,
    // DpsRejected, OfflineRefused, …), each likewise carrying request_id.
    //
    // CONTRACT for RS-3 (the highest-consequence decision): `FiscalError` is
    // the DETERMINISTIC-refusal channel only.  A transport / ambiguous DPS
    // failure that auto-offlines is a SUCCESS, not an error — it returns
    // `Ok(FiscalOutcome { document_state: OfflineLocalAck, fiscal_id: None, … })`
    // (200 + OFFLINE_LOCAL_ACK), NEVER `Err`.  Only a hard DPS reject /
    // guard refusal (shift-not-open, sign failure) is `Err`.  `replay.rs`
    // already encodes this asymmetry (OfflineLocalAck is an ACCEPTED replay
    // state; OfflineRefusal is Failed) — keep the two in lock-step.
}

/// The inline-synchronous write-path entrypoint.  RS-2's handler calls
/// [`fiscalize`](Self::fiscalize) with the persisted inbox row; RS-3's
/// real impl drives the write-path (`stage_acquire → sign →
/// dispatch/send` or offline-local-ack) behind the single-writer lease.
///
/// CONCURRENCY (RS-3 caller obligation): the inline handler does NOT
/// serialize per-FN — `fiscalize` MAY be invoked concurrently for distinct
/// receipts of the SAME `fiscal_number` (axum runs each POST in its own
/// task; the inbox `insert` only serializes the same idempotency key).  The
/// implementation MUST establish the per-FN single-writer lease itself
/// (invariant #2; `acquire_lease` CAS NEW→PROCESSING).  It also owns the
/// stale-`PROCESSING` reaper that re-drives a row leased-then-crashed (which
/// reads its identity from the row — see [`InboxRow`] A-H1).  The
/// `operation_type` on the row is the Z-vs-non-Z discriminator: for
/// `Z_REPORT` / `SHIFT_CLOSE` the `payload_json` is WIRE intent (aggregate
/// behind the drain barrier); for all others it is signer-ready.
#[async_trait]
pub trait WritePathEntry: Send + Sync {
    /// Fiscalize an inbox-accepted receipt.  Returns the outcome the
    /// handler renders, or a typed error — **never a silent success on
    /// failure**.
    async fn fiscalize(&self, row: &InboxRow) -> Result<FiscalOutcome, FiscalError>;
}

/// The pre-RS-3 production binding: every call fails closed with
/// [`FiscalError::NotImplemented`].  It can NEVER return `Ok` — a missing
/// write-path must surface as a non-2xx, not a phantom success.
pub struct UnimplementedWritePath;

#[async_trait]
impl WritePathEntry for UnimplementedWritePath {
    async fn fiscalize(&self, row: &InboxRow) -> Result<FiscalOutcome, FiscalError> {
        Err(FiscalError::NotImplemented {
            request_id: row.request_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::enums::Protocol;

    fn sample_row(request_id: [u8; 16]) -> InboxRow {
        InboxRow {
            request_id,
            fiscal_number: "4000000001".to_string(),
            protocol: Protocol::Rest,
            operation_type: "SELL".to_string(),
            idempotency_key: "k".to_string(),
            status: "NEW".to_string(),
            payload_json: "{}".to_string(),
            payload_sha256_canonical: [0u8; 32],
            correlation_id: None,
            received_at: "2026-06-06T00:00:00Z".to_string(),
            signed_by_cashier_id: Some("csh-007".to_string()),
            driver_id: Some("drv-1".to_string()),
            business_ts: Some("2026-06-06T00:00:00Z".to_string()),
            total_sum_kop: Some(2500),
        }
    }

    /// Acceptance: the pre-RS-3 path can NEVER return a successful
    /// fiscalization, and the error preserves `request_id` for the
    /// response envelope.
    #[tokio::test]
    async fn unimplemented_fails_closed_never_success() {
        let req = [7u8; 16];
        let res = UnimplementedWritePath.fiscalize(&sample_row(req)).await;
        assert!(
            res.is_err(),
            "pre-RS-3 write-path must never return Ok (no phantom success)"
        );
        match res {
            Err(FiscalError::NotImplemented { request_id }) => assert_eq!(
                request_id, req,
                "request_id must survive into the seam error for the response envelope"
            ),
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }

    /// Acceptance: the outcome can express fiscal_id, document_state, and
    /// the optional Q4 report_xml passthrough.
    #[test]
    fn outcome_expresses_fiscal_id_state_and_report_xml() {
        let o = FiscalOutcome {
            document_id: DocumentId::from_bytes([1u8; 16]),
            fiscal_id: Some("12345".to_string()),
            fiscal_ts: Some("2026-06-06T00:00:00Z".to_string()),
            document_state: DocState::Ack,
            report_xml: Some("<ZREP/>".to_string()),
        };
        assert_eq!(o.fiscal_id.as_deref(), Some("12345"));
        assert_eq!(o.document_state, DocState::Ack);
        assert_eq!(o.report_xml.as_deref(), Some("<ZREP/>"));
    }
}
