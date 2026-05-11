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

// ─── Shared bridge_anyhow helper (R-W10.4-senior-review LOW 1) ───────

/// Generic `anyhow::Error` → typed-stage-error bridge.  Mirrors the
/// pattern previously triplicated as private `bridge_anyhow` fns in
/// `stage_send.rs` / `stage_sign.rs` / `mac_recovery.rs`.
///
/// Behaviour (matches the original three private impls):
///   1. Try to downcast to the concrete typed error `E` first — that
///      handles the `anyhow::Error::new(E::Variant(..))` round-trip
///      from inside `with_immediate` closures.
///   2. Fall back to downcasting to `sqlx::Error` and wrap via
///      `wrap_db` — distinguishes DB errors from generic anyhow
///      chains.
///   3. Anything else: wrap via `wrap_internal`.
///
/// Each module's local `bridge_anyhow` becomes a one-liner:
/// ```ignore
/// fn bridge_anyhow(e: anyhow::Error) -> StageSendError {
///     bridge_anyhow_to(e, StageSendError::Db, StageSendError::Internal)
/// }
/// ```
///
/// Why generic over wrap fns (not over the typed error directly):
/// each module's typed error has its own `Db(sqlx::Error)` and
/// `Internal(anyhow::Error)` variant constructors with subtly
/// different naming; passing them as fn pointers keeps each module's
/// surface unchanged while deduplicating the downcast logic.
pub(super) fn bridge_anyhow_to<E>(
    e: anyhow::Error,
    wrap_db: fn(sqlx::Error) -> E,
    wrap_internal: fn(anyhow::Error) -> E,
) -> E
where
    E: 'static + std::error::Error + Send + Sync,
{
    match e.downcast::<E>() {
        Ok(typed) => typed,
        Err(rest) => match rest.downcast::<sqlx::Error>() {
            Ok(sqlx_err) => wrap_db(sqlx_err),
            Err(other) => wrap_internal(other),
        },
    }
}

// ─── Shared lowercase hex encoder (R-W10.4-senior-review LOW 2) ──────

/// Lowercase hex encoder for byte slices.  Used for:
///   - canonical XML `<…PREV_DOC_HASH>` rendering (W6 stage 3,
///     `re_sign_after_mac_recovery`).
///   - audit-payload JSON `*_hex` fields (W10.4 MAC recovery audits).
///
/// Closed contract: lowercase via `format!("{b:02x}")`.  W10.1 review
/// pinned this case explicitly via fixture
/// `re_sign_propagates_new_previous_hash_into_canonical_xml`; any
/// regression to UPPERCASE hex would fail loudly.  De-duplicated
/// from `stage_sign::hex_encode` + `mac_recovery::hex_lower` per
/// senior review.
pub(crate) fn hex_encode_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
