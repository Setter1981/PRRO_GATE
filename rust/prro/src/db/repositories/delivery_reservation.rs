//! Repository for `delivery_reservation` (CS-2, Spec #4 part A §3).
//!
//! Schema: `rust/prro/migrations/032_delivery_reservation.sql` — the
//! durable certainty / fence authority for the Sending / CallStarted
//! window (Spec #4 §1).  The table owns the call lifecycle
//! `ReservedNotStarted → CallStarted → OutcomeObserved` and delivery
//! certainty via the three orthogonal Spec #2 §2 fields.
//!
//! **INACTIVE (CS-2 §2b).**  This repo exists but has **NO production
//! caller** — nothing in the write-path, boot-resume, or drain paths
//! reads or writes it.  Activation (record-then-apply with
//! `ObservedOutcomeV1`, the fence-token seed advance) is CS-3.  The only
//! consumers in CS-2 are the migration / persistence regression tests
//! (`tests/migration_032_delivery_reservation.rs`).  The static
//! call-graph pin (merge pin §6.4) asserts exactly that.
//!
//! Repo policy (mirrors `outbox.rs`):
//! - runtime-bound `sqlx::query` / `query_as` (NOT the compile-checked
//!   `query!`) so the `.sqlx` dual-cache is untouched.
//! - errors bubble up unmodified; PK / UNIQUE / CHECK / trigger `ABORT`
//!   violations surface as `sqlx::Error` for the caller to route.
//! - `insert` is **tx-only** (`&mut WriteTxConn`): the
//!   `attempt_no = MAX(attempt_no) + 1` read and the INSERT must live in
//!   the same `BEGIN IMMEDIATE` envelope so the per-document attempt
//!   counter is race-free.  The `UNIQUE (document_id, attempt_no)`
//!   constraint is the backstop.
//! - `get_active_for_fn` is pool-bound (read-only fence lookup).

use crate::db::models::ids::DocumentId;
use crate::db::tx::WriteTxConn;
use crate::db::types::DbDocumentId;

/// Fresh 16-byte reservation identity.  Minimal by design (CS-2 is
/// INACTIVE): a bare `[u8; 16]` rather than a domain newtype, mirroring
/// the raw-blob binding style `outbox.rs` uses for `payload_sha256`.
/// CS-3 promotes this to a typed id if the activation contract needs it.
pub type ReservationId = [u8; 16];

/// Inputs for [`insert`].  The caller supplies the identity + immutable
/// protocol binding; `attempt_no` is computed inside the tx (NOT passed);
/// `state` starts at `RESERVED_NOT_STARTED` (schema DEFAULT + the
/// `insert_state` trigger enforce it); `created_at` / `updated_at` are
/// DB-clock DEFAULTs; the outcome fields (`submission_certainty`,
/// `response_provenance`, `routing_class`, `remote_correlation_id`) and
/// `call_started_at` are all NULL at creation per the structural matrix.
#[derive(Clone, Debug)]
pub struct NewReservation {
    pub reservation_id: ReservationId,
    pub document_id: DocumentId,
    pub fiscal_number: String,
    /// `'FSCO_ZZD'` | `'EVPZ_DPS'` — the immutable bound protocol (A4-4).
    pub dps_protocol_id: String,
    pub protocol_contract_version: i64,
    /// NULL unless a capability profile is pinned; `>= 1` when present.
    pub capability_profile_version: Option<i64>,
    /// NULL unless an endpoint config revision is pinned; `>= 1` when present.
    pub endpoint_config_revision: Option<i64>,
    /// 32-byte protocol-specific envelope hash (`length = 32` CHECK).
    pub envelope_hash: [u8; 32],
}

/// Insert a fresh reservation as `RESERVED_NOT_STARTED`.
///
/// **Atomicity contract.**  `attempt_no` is derived as
/// `COALESCE(MAX(attempt_no), 0) + 1` over the rows for `document_id`,
/// read and written inside the SAME `with_immediate` `BEGIN IMMEDIATE`
/// envelope the caller opened (`&mut WriteTxConn`).  The
/// `UNIQUE (document_id, attempt_no)` index is the backstop against a
/// racing writer; the `no_replace` collision-guard trigger additionally
/// blocks any `INSERT OR REPLACE`-style eviction of the same
/// `reservation_id` / `(document_id, attempt_no)` / active-FN fence.
///
/// Returns the assigned `attempt_no`.  The row is created with the
/// outcome fields NULL and no `call_started_at` (the `insert_state`
/// trigger and the RESERVED_NOT_STARTED structural CHECK enforce it).
///
/// **INACTIVE:** invoked only from persistence tests in CS-2.
pub async fn insert(tx: &mut WriteTxConn<'_>, row: NewReservation) -> sqlx::Result<i64> {
    // Next attempt for this document, monotonic from 1.  Append-only:
    // the delete trigger forbids row removal, so MAX never regresses.
    let next_attempt: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt_no), 0) + 1 FROM delivery_reservation \
         WHERE document_id = ?",
    )
    .bind(DbDocumentId(row.document_id))
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO delivery_reservation \
             (reservation_id, document_id, fiscal_number, attempt_no, \
              dps_protocol_id, protocol_contract_version, \
              capability_profile_version, endpoint_config_revision, \
              envelope_hash) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.reservation_id[..])
    .bind(DbDocumentId(row.document_id))
    .bind(&row.fiscal_number)
    .bind(next_attempt)
    .bind(&row.dps_protocol_id)
    .bind(row.protocol_contract_version)
    .bind(row.capability_profile_version)
    .bind(row.endpoint_config_revision)
    .bind(&row.envelope_hash[..])
    .execute(&mut **tx)
    .await?;

    Ok(next_attempt)
}

/// A `delivery_reservation` row as read by [`get_active_for_fn`].  Field
/// order matches the SELECT below.  The outcome triple + marker are
/// `Option` (NULL until observed); the identity / binding columns are
/// always present.
#[derive(Clone, Debug)]
pub struct ReservationRow {
    pub reservation_id: ReservationId,
    pub document_id: DocumentId,
    pub fiscal_number: String,
    pub attempt_no: i64,
    pub state: String,
    pub call_started_at: Option<String>,
    pub dps_protocol_id: String,
    pub protocol_contract_version: i64,
    pub capability_profile_version: Option<i64>,
    pub endpoint_config_revision: Option<i64>,
    pub envelope_hash: [u8; 32],
    pub remote_correlation_id: Option<String>,
    pub submission_certainty: Option<String>,
    pub response_provenance: Option<String>,
    pub routing_class: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Wire shape of a `delivery_reservation` row (runtime `query_as`).  A
/// derived `FromRow` struct (17 columns exceeds the 16-tuple `FromRow`
/// arity limit) — column-name based, mirrors the `payment_methods` /
/// `driver_tax_mapping` derive style.  The two identity BLOBs +
/// `envelope_hash` decode as `Vec<u8>` and are length-checked into fixed
/// arrays in [`get_active_for_fn`].
#[derive(sqlx::FromRow)]
struct ReservationRowRaw {
    reservation_id: Vec<u8>,
    document_id: Vec<u8>,
    fiscal_number: String,
    attempt_no: i64,
    state: String,
    call_started_at: Option<String>,
    dps_protocol_id: String,
    protocol_contract_version: i64,
    capability_profile_version: Option<i64>,
    endpoint_config_revision: Option<i64>,
    envelope_hash: Vec<u8>,
    remote_correlation_id: Option<String>,
    submission_certainty: Option<String>,
    response_provenance: Option<String>,
    routing_class: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Return the single ACTIVE (fenced) reservation for `fiscal_number`, if
/// any.  "Active" is exactly the narrowed §3.1 `ux_reservation_active`
/// partial-unique predicate (migration 035, design §3.1): the in-flight
/// states (`RESERVED_NOT_STARTED` / `CALL_STARTED`) OR the record-then-apply
/// window (`OUTCOME_OBSERVED` with `apply_state = 'PENDING_APPLY'`).  Once
/// the outcome is applied (`apply_state = 'APPLIED'`) the row is NOT active
/// and releases the fence.  Unresolved outcomes (SubmittedUnknown / -12 / -6)
/// are held by `node_state.mode = STOP_MODE` (Slice 5), NOT this SQL fence,
/// so they do NOT keep a reservation active here.
///
/// This predicate is byte-identical to the `ux_reservation_active` index and
/// the `delivery_reservation_no_replace` trigger clause (migration 035).  The
/// partial-unique index guarantees at most one such row per FN, so this
/// returns `Option`.  Read-only, pool-bound.
///
/// **INACTIVE:** invoked only from persistence tests in CS-2.
pub async fn get_active_for_fn(
    pool: &sqlx::SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<ReservationRow>> {
    let row: Option<ReservationRowRaw> = sqlx::query_as(
        "SELECT reservation_id, document_id, fiscal_number, attempt_no, state, \
                call_started_at, dps_protocol_id, protocol_contract_version, \
                capability_profile_version, endpoint_config_revision, envelope_hash, \
                remote_correlation_id, submission_certainty, response_provenance, \
                routing_class, created_at, updated_at \
         FROM delivery_reservation \
         WHERE fiscal_number = ? \
           AND (state IN ('RESERVED_NOT_STARTED','CALL_STARTED') \
             OR (state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY'))",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else { return Ok(None) };

    let reservation_id: [u8; 16] = r.reservation_id.as_slice().try_into().map_err(|_| {
        sqlx::Error::Decode(
            format!(
                "delivery_reservation.reservation_id: expected 16 bytes, got {}",
                r.reservation_id.len()
            )
            .into(),
        )
    })?;
    let doc_id_arr: [u8; 16] = r.document_id.as_slice().try_into().map_err(|_| {
        sqlx::Error::Decode(
            format!(
                "delivery_reservation.document_id: expected 16 bytes, got {}",
                r.document_id.len()
            )
            .into(),
        )
    })?;
    let envelope_hash: [u8; 32] = r.envelope_hash.as_slice().try_into().map_err(|_| {
        sqlx::Error::Decode(
            format!(
                "delivery_reservation.envelope_hash: expected 32 bytes, got {}",
                r.envelope_hash.len()
            )
            .into(),
        )
    })?;

    Ok(Some(ReservationRow {
        reservation_id,
        document_id: DocumentId::from_bytes(doc_id_arr),
        fiscal_number: r.fiscal_number,
        attempt_no: r.attempt_no,
        state: r.state,
        call_started_at: r.call_started_at,
        dps_protocol_id: r.dps_protocol_id,
        protocol_contract_version: r.protocol_contract_version,
        capability_profile_version: r.capability_profile_version,
        endpoint_config_revision: r.endpoint_config_revision,
        envelope_hash,
        remote_correlation_id: r.remote_correlation_id,
        submission_certainty: r.submission_certainty,
        response_provenance: r.response_provenance,
        routing_class: r.routing_class,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// The authorization minted by [`authorize_submission`] — the SOLE permit for a wire call
/// (CS-3 Slice 3, design §2 lifetime authorization + sole-caller wire gate).
///
/// Carries the reservation identity + the `authorized_generation` snapshot taken at the
/// `RESERVED_NOT_STARTED → CALL_STARTED` transition.  A caller with no `Authorization` performs
/// ZERO wire I/O; the token is minted only after the authorization transaction commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authorization {
    pub reservation_id: ReservationId,
    pub document_id: DocumentId,
    pub attempt_no: i64,
    /// The `node_state.delivery_generation` snapshot consumed by this authorization (`>= 1`).
    pub authorized_generation: i64,
}

/// Why [`authorize_submission`] refused (no wire I/O happens on any of these).
#[derive(Debug, thiserror::Error)]
pub enum AuthorizeError {
    /// P2 lifetime call-once: this document EVER crossed `CALL_STARTED` before.  Refused by the
    /// explicit NOT-EXISTS guard (design §2) — the `ux_delivery_document_ever_started` index +
    /// `delivery_reservation_no_replace` historical clause are the fail-closed backstop.
    #[error("call-once: document already crossed CALL_STARTED")]
    CallOnceAlreadyStarted,
    /// The fresh RESERVED_NOT_STARTED insert was refused — the §3.1 FN fence already holds an
    /// active reservation, or a reservation-id / (document_id, attempt_no) collision.
    #[error("fence/collision on reserve: {0}")]
    FenceOrCollision(#[source] sqlx::Error),
    /// No `node_state` row exists for the fiscal number (the FN is not configured).
    #[error("node_state row missing for the fiscal number")]
    NodeStateMissing,
    /// The generation / pointer UPDATE did not affect exactly one row.
    #[error("node_state generation/pointer update did not affect exactly one row")]
    GenerationUpdateFailed,
    /// The RN → CALL_STARTED transition did not affect exactly one row.
    #[error("RN → CALL_STARTED transition did not affect exactly one row")]
    TransitionFailed,
    /// A lower-level SQLite error.
    #[error(transparent)]
    Db(sqlx::Error),
}

/// Authorize a wire submission for a document (CS-3 Slice 3, design §2).
///
/// In the caller's SINGLE `BEGIN IMMEDIATE`:
///   1. **call-once** (design §2): refuse if the document EVER crossed `CALL_STARTED`
///      (explicit `NOT EXISTS` on a non-NULL `call_started_at`);
///   2. **fence + reserve**: insert a fresh `RESERVED_NOT_STARTED` row — the `no_replace`
///      trigger enforces the §3.1 FN fence + the historical-document-started guard, so a
///      second active reservation for the FN or a re-reserve of a started document aborts;
///   3. **generation**: snapshot `node_state.delivery_generation`, advance it by one (the
///      monotone `034` trigger permits the increase), and point
///      `active_delivery_reservation_id` at the new reservation;
///   4. **marker**: transition `RESERVED_NOT_STARTED → CALL_STARTED`, committing
///      `call_started_at` + `authorized_generation` together (the `034` cs-pairing trigger
///      requires both).
///
/// Returns an [`Authorization`] on success.  Any refusal (call-once / fence / missing node /
/// non-single-row update) returns an [`AuthorizeError`]; the caller propagates it so the
/// whole transaction rolls back and NO wire I/O is performed (sole-caller wire gate).
///
/// **INACTIVE (CS-3 Slice 3):** no production caller is wired yet; the live send path
/// (`stage_send::run`) is gated on this only at the whole-fence cutover (Slice 7).  Exercised
/// by authorization tests today.
pub async fn authorize_submission(
    tx: &mut WriteTxConn<'_>,
    row: NewReservation,
    call_started_at: &str,
) -> Result<Authorization, AuthorizeError> {
    let reservation_id = row.reservation_id;
    let document_id = row.document_id;
    let fiscal_number = row.fiscal_number.clone();

    // 1. Explicit call-once guard (design §2) — belt with the DDL index + no_replace clause.
    let already_started: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM delivery_reservation \
         WHERE document_id = ? AND call_started_at IS NOT NULL)",
    )
    .bind(DbDocumentId(document_id))
    .fetch_one(&mut **tx)
    .await
    .map_err(AuthorizeError::Db)?;
    if already_started == 1 {
        return Err(AuthorizeError::CallOnceAlreadyStarted);
    }

    // 2. Reserve (RESERVED_NOT_STARTED). The no_replace trigger is the fail-closed backstop
    //    for the FN fence + historical-document-started guard.
    let attempt_no = insert(tx, row)
        .await
        .map_err(AuthorizeError::FenceOrCollision)?;

    // 3. Snapshot + advance the node fence generation, and point at the new reservation.
    let current_gen: i64 =
        sqlx::query_scalar("SELECT delivery_generation FROM node_state WHERE fiscal_number = ?")
            .bind(&fiscal_number)
            .fetch_optional(&mut **tx)
            .await
            .map_err(AuthorizeError::Db)?
            .ok_or(AuthorizeError::NodeStateMissing)?;
    let authorized_generation = current_gen + 1;

    let gen_rows = sqlx::query(
        "UPDATE node_state SET delivery_generation = ?, active_delivery_reservation_id = ? \
         WHERE fiscal_number = ?",
    )
    .bind(authorized_generation)
    .bind(&reservation_id[..])
    .bind(&fiscal_number)
    .execute(&mut **tx)
    .await
    .map_err(AuthorizeError::Db)?
    .rows_affected();
    if gen_rows != 1 {
        return Err(AuthorizeError::GenerationUpdateFailed);
    }

    // 4. RN → CALL_STARTED with the durable marker + generation snapshot (pair required by 034).
    let cs_rows = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', call_started_at = ?, authorized_generation = ? \
         WHERE reservation_id = ? AND state = 'RESERVED_NOT_STARTED'",
    )
    .bind(call_started_at)
    .bind(authorized_generation)
    .bind(&reservation_id[..])
    .execute(&mut **tx)
    .await
    .map_err(AuthorizeError::Db)?
    .rows_affected();
    if cs_rows != 1 {
        return Err(AuthorizeError::TransitionFailed);
    }

    Ok(Authorization {
        reservation_id,
        document_id,
        attempt_no,
        authorized_generation,
    })
}
