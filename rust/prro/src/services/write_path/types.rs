//! Stage-local types for the write-path pipeline.
//!
//! `WorkerContext` is the snapshot stage 1 hands to stages 3+; all
//! mutable substrate handles (`Arc<dyn CryptoProvider>`,
//! `Arc<dyn DpsChannel>`) live in the worker dispatcher (out of W5
//! scope) and are NOT carried in the context — they are wired in at
//! stage boundaries by the dispatcher.

use crate::db::models::enums::{DocType, ShiftState};
use crate::db::repositories::fiscal_documents::DocumentRow;
use crate::db::repositories::ingress_inbox::InboxRow;
use crate::db::repositories::node_state::NodeStateRow;
use crate::db::repositories::shifts::ShiftRow;

/// Minimal canonical envelope view used by W5 guards and stage 1
/// INSERT.  Full canonicalisation / XML build happens in stage 3
/// (W6); W5 only needs enough to drive doc_type-shaped guards and
/// build a `NewDocument`.
#[derive(Debug, Clone)]
pub struct CanonicalFiscalCommand {
    pub doc_type: DocType,
    pub business_ts: String,
    pub total_sum_kop: Option<i64>,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
}

/// Snapshot handed from stage 1 to subsequent stages.  Contains
/// everything stages 3-5 need to build wire artifacts and persist
/// outcomes WITHOUT re-reading `node_state` (which can drift
/// between PREPARED and the resume pickup).
#[derive(Debug, Clone)]
pub struct WorkerContext {
    pub inbox: InboxRow,
    pub command: CanonicalFiscalCommand,
    pub node_state: NodeStateRow,
    /// `Some` when `node_state.shift_state == Opened` AND the
    /// referenced shift is itself in `Opened`; `None` for
    /// shift-management ops (SHIFT_OPEN) where there is no active
    /// shift yet.  Stage 2's shift-invariant guard rejects the
    /// inconsistent middle (`shift_state == Opened` but no resolvable
    /// `current_shift_id`).
    pub active_shift: Option<ShiftRow>,
    /// The fiscal_documents row that stages 3+ will continue
    /// processing.  For `Proceed` this is freshly INSERTed PREPARED;
    /// for `Resumed` it is the existing pending row read by
    /// `get_by_request_id_tx`.  In both cases its
    /// `backend_profile_id` / `transport_profile_id` carry the
    /// PERSISTED bindings — stages 3+ MUST use these, not the
    /// possibly-drifted `node_state.*_profile_id`.
    pub document: DocumentRow,
}

/// Reason for a stage-2 guard rejection.  Carried by
/// `WorkerProcessResult::Rejected`.  Distinct variants so the
/// audit / metrics layer can label without parsing strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionReason {
    NodeOffline,
    ShiftNotOpen {
        current: ShiftState,
    },
    ShiftAlreadyOpen,
    ShiftInError,
    /// `shift_state == Opened` but `current_shift_id IS NULL` OR the
    /// resolved shift is not itself `Opened`.  Per ADR-M3-A7 the
    /// canonical source of truth is `node_state.current_shift_id`;
    /// any inconsistency is a structural invariant breach (CRITICAL
    /// audit, no autorepair from stage 2).
    ShiftInvariantViolation,
    /// `node_state.backend_profile_id` and / or
    /// `transport_profile_id` are NULL.  Schema-permitted on a
    /// freshly-bootstrapped FN; a real submission cannot proceed
    /// without resolved bindings.
    MissingProfileBinding,
    InvalidPayload {
        detail: String,
    },
}

/// Result of stage 1 (acquire+validate+guard) per W0-1 §3.1.
#[derive(Debug, Clone)]
pub enum WorkerProcessResult {
    /// Happy path — fresh PREPARED doc inserted, stages 3+ to follow.
    Proceed(WorkerContext),
    /// Resume path — `get_by_request_id_tx` found an existing
    /// pending doc; stage dispatcher continues from its current
    /// state per the W0-1 resume table.  No fresh lnd allocated;
    /// no re-INSERT.
    Resumed(WorkerContext),
    /// Lease miss — inbox row was already `PROCESSING` / `DONE` /
    /// `REJECTED` / `ERROR`; another worker has it or processing
    /// is complete.  No state mutation.  Per W5 design, no audit
    /// row is appended on this path (would create churn under
    /// healthy retry loops).
    Noop,
    /// Guard rejected the request.  `ingress_inbox.status =
    /// REJECTED` is persisted; an audit row is appended; NO
    /// `fiscal_documents` row is created and NO lnd is allocated.
    Rejected { reason: RejectionReason },
}
