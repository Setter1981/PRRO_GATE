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

use crate::db::models::enums::DocState;
use crate::db::models::ids::DocumentId;
use crate::db::tx::WriteTxConn;
use crate::db::types::DbDocumentId;
use prro_domain::delivery::evidence::EvidenceDiscriminant;
use prro_domain::delivery::{NodeEffect, ObservedOutcomeV1, SubmissionCertainty};

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
/// The active-reservation fence predicate body — the `state` disjunction shared,
/// byte-identical, by [`get_active_for_fn`], [`fn_fence_active_tx`], the
/// `ux_reservation_active` partial index and the `delivery_reservation_no_replace`
/// trigger clause (migration 035).  The conformance test `fence_predicate_byte_identity`
/// pins it against the migration SQL, so any drift fails CI.  Only ever interpolated
/// with this compile-time constant (never user input) → no SQL-injection surface.
pub const ACTIVE_FENCE_STATE_PREDICATE: &str = "state IN ('RESERVED_NOT_STARTED','CALL_STARTED') \
       OR (state = 'OUTCOME_OBSERVED' AND apply_state = 'PENDING_APPLY')";

/// CS-3 S7-2 — the **tx-bound** FN active-reservation fence.  Returns `true` when the
/// FN has an in-flight delivery reservation (the record-then-apply window, per the
/// §3.1 predicate).  A foreign FN-chain / offline writer MUST call this INSIDE its own
/// `BEGIN IMMEDIATE`, BEFORE its mutation, and fail-closed (refuse) when `true`: a
/// pool-bound read ([`get_active_for_fn`]) races the fence across connections (TOCTOU).
///
/// `apply_outcome` is the active reservation itself and MUST NOT self-fence.
///
/// **INACTIVE until S7-1 cutover:** no reservation is ever active in production while
/// the delivery FSM is dormant, so this returns `false` for every live call today; the
/// wiring + teeth exist so the fence already guards each writer when the FSM activates.
pub async fn fn_fence_active_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
) -> sqlx::Result<bool> {
    let active: i64 = sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM delivery_reservation \
         WHERE fiscal_number = ? AND ({ACTIVE_FENCE_STATE_PREDICATE}))"
    ))
    .bind(fiscal_number)
    .fetch_one(&mut **tx)
    .await?;
    Ok(active == 1)
}

/// **INACTIVE:** invoked only from persistence tests in CS-2.
pub async fn get_active_for_fn(
    pool: &sqlx::SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<ReservationRow>> {
    let row: Option<ReservationRowRaw> = sqlx::query_as(&format!(
        "SELECT reservation_id, document_id, fiscal_number, attempt_no, state, \
                call_started_at, dps_protocol_id, protocol_contract_version, \
                capability_profile_version, endpoint_config_revision, envelope_hash, \
                remote_correlation_id, submission_certainty, response_provenance, \
                routing_class, created_at, updated_at \
         FROM delivery_reservation \
         WHERE fiscal_number = ? AND ({ACTIVE_FENCE_STATE_PREDICATE})"
    ))
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
/// **Sealed capability (S7-0 gap 3).**  All fields are private and there is **no `Clone`**: a
/// caller cannot fabricate one, copy one to authorize a second wire, or rebind its bytes.  It is
/// minted only after the authorization transaction commits and is meant to be **consumed by value**
/// at the sole wire function (`submit_authorized`, Slice 7), which recomputes the envelope hash and
/// checks it against the snapshot carried here.  A caller with no `Authorization` performs ZERO wire
/// I/O.  It carries the immutable protocol binding + envelope hash of the authorized reservation so
/// the wire seam cannot be re-bound to different bytes after commit.
#[derive(Debug, PartialEq, Eq)]
pub struct Authorization {
    reservation_id: ReservationId,
    document_id: DocumentId,
    attempt_no: i64,
    /// The `node_state.delivery_generation` snapshot consumed by this authorization (`>= 1`).
    authorized_generation: i64,
    /// The immutable 32-byte envelope hash the wire call must carry (rebind guard).
    envelope_hash: [u8; 32],
    /// The immutable bound protocol id (`FSCO_ZZD` / `EVPZ_DPS`).
    dps_protocol_id: String,
    /// The immutable protocol contract version.
    protocol_contract_version: i64,
    /// The immutable capability profile version (`NULL` unless pinned; `>= 1`).
    capability_profile_version: Option<i64>,
    /// The immutable endpoint config revision (`NULL` unless pinned; `>= 1`).
    endpoint_config_revision: Option<i64>,
}

impl Authorization {
    /// The authorized reservation identity.
    pub fn reservation_id(&self) -> ReservationId {
        self.reservation_id
    }
    /// The authorized document.
    pub fn document_id(&self) -> DocumentId {
        self.document_id
    }
    /// The reservation attempt number.
    pub fn attempt_no(&self) -> i64 {
        self.attempt_no
    }
    /// The consumed generation snapshot (`>= 1`).
    pub fn authorized_generation(&self) -> i64 {
        self.authorized_generation
    }
    /// The immutable envelope hash the wire call must carry.
    pub fn envelope_hash(&self) -> &[u8; 32] {
        &self.envelope_hash
    }
    /// The immutable bound protocol id.
    pub fn dps_protocol_id(&self) -> &str {
        &self.dps_protocol_id
    }
    /// The immutable protocol contract version.
    pub fn protocol_contract_version(&self) -> i64 {
        self.protocol_contract_version
    }
    /// The immutable capability profile version (`NULL` unless pinned) — the 4th binding
    /// component `submit_authorized` echo-checks against the port snapshot (AO-2).
    pub fn capability_profile_version(&self) -> Option<i64> {
        self.capability_profile_version
    }
    /// The immutable endpoint config revision (`NULL` unless pinned) — the 5th binding
    /// component `submit_authorized` echo-checks against the port snapshot (AO-2).
    pub fn endpoint_config_revision(&self) -> Option<i64> {
        self.endpoint_config_revision
    }
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
    // Capture the immutable protocol binding + envelope hash for the sealed token before `row`
    // moves into `insert` (they are the wire-seam rebind guard, plan §2.2).
    let envelope_hash = row.envelope_hash;
    let dps_protocol_id = row.dps_protocol_id.clone();
    let protocol_contract_version = row.protocol_contract_version;
    let capability_profile_version = row.capability_profile_version;
    let endpoint_config_revision = row.endpoint_config_revision;

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
        envelope_hash,
        dps_protocol_id,
        protocol_contract_version,
        capability_profile_version,
        endpoint_config_revision,
    })
}

/// Why [`record_outcome`] refused (nothing is mutated on any of these — fail-closed).
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    /// The record write did not match exactly one row under the FULL authority tuple
    /// (reservation id + document id + authorized generation + protocol binding + envelope
    /// hash) AND `state = 'CALL_STARTED'`. The reservation is not the authorized in-flight
    /// call — superseded, rebound to different bytes, the wrong token, or already recorded.
    /// A HARD error (plan §2.3 step 7): it must NOT fall back to any legacy record path.
    #[error("record authority CAS miss (not CALL_STARTED under this exact authority tuple)")]
    AuthorityMismatch,
    /// The reservation row vanished between the CAS and the node-effect read (structural breach).
    #[error("reservation row missing after record CAS")]
    ReservationMissing,
    /// A lower-level SQLite error. A migration-036 evidence-matrix `ABORT` surfaces here — an
    /// illegal/partial evidence combination rolls the whole record back (no half-written authority).
    #[error(transparent)]
    Db(sqlx::Error),
}

/// Durably record a wire outcome: `CALL_STARTED → OUTCOME_OBSERVED + PENDING_APPLY` (CS-3
/// Slice 7 gap 1, design §8 step 3 / plan §2.3 "record commit").
///
/// This is the production record boundary the apply / operator / boot paths were all built
/// around — each requires an `OUTCOME_OBSERVED + PENDING_APPLY` row, and until now nothing
/// minted one (the tests wrote the row directly). In the caller's single `BEGIN IMMEDIATE`,
/// AFTER the sole wire call has returned and been classified, it:
///   1. serializes the sealed [`ObservedOutcomeV1`] axes + [`EvidenceDiscriminant`] columns
///      onto the reservation row and transitions it to `OUTCOME_OBSERVED + PENDING_APPLY`;
///   2. matches on the FULL authority tuple carried by the [`Authorization`] token
///      (reservation id, document id, authorized generation, protocol binding, envelope hash)
///      AND `state = 'CALL_STARTED'` — `rows_affected != 1` is [`RecordError::AuthorityMismatch`]
///      (a hard error, never a legacy fallback: plan §2.3 step 7; a second record of an already
///      `OUTCOME_OBSERVED` row also lands here — record is call-once);
///   3. in the SAME transaction, applies the EARLY node-safety mode — a `-11` (`NodeBlocked`)
///      flips the node `BLOCKED`; any UNRESOLVED outcome (a `SUBMITTED_UNKNOWN` leaf, or a held
///      `-12`/`-6`/probe verdict) flips it `STOP_MODE`. NO fence release happens here (that is
///      [`apply_outcome`]) — record owns the durable evidence + the safety halt; apply owns the
///      seed / SFN / document / pointer projection.
///
/// The four axes + four evidence columns are written verbatim from the sealed types, so the
/// migration-036 fail-closed matrix is satisfied by construction whenever the caller's
/// [`ObservedOutcomeV1`] and [`EvidenceDiscriminant`] agree (both derive from one
/// `SubmissionEvidence`). An inconsistent combination trips the matrix trigger and surfaces as
/// [`RecordError::Db`] with the whole record rolled back.
///
/// **INACTIVE (CS-3 Slice 7 gap 1):** no production caller wires it until the S7-1 cutover
/// composes `authorize → submit_authorized → record_outcome → apply_outcome`. Exercised by
/// `tests/record_outcome.rs` today.
pub async fn record_outcome(
    tx: &mut WriteTxConn<'_>,
    authority: &Authorization,
    outcome: &ObservedOutcomeV1,
    evidence: &EvidenceDiscriminant,
) -> Result<(), RecordError> {
    let cols = evidence.to_columns();
    let certainty = outcome.certainty();
    let node_effect = outcome.node_effect();
    let reservation_id = authority.reservation_id();

    // Record-then-apply commit under the full authority CAS + the CALL_STARTED predicate.
    let updated = sqlx::query(
        "UPDATE delivery_reservation SET \
             state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             submission_certainty = ?, response_provenance = ?, routing_class = ?, \
             node_effect = ?, remote_correlation_id = ?, \
             evidence_kind = ?, evidence_text = ?, evidence_code = ?, evidence_digest = ? \
         WHERE reservation_id = ? AND document_id = ? AND authorized_generation = ? \
             AND dps_protocol_id = ? AND protocol_contract_version = ? \
             AND envelope_hash = ? \
             AND capability_profile_version IS ? AND endpoint_config_revision IS ? \
             AND state = 'CALL_STARTED'",
    )
    .bind(certainty.as_str())
    .bind(outcome.provenance().as_str())
    .bind(outcome.routing().map(|r| r.as_str()))
    .bind(node_effect.as_str())
    .bind(
        outcome
            .remote_correlation_id()
            .map(|b| b.as_str().to_string()),
    )
    .bind(cols.kind)
    .bind(cols.text)
    .bind(cols.code)
    .bind(cols.digest.map(|d| d.to_vec()))
    .bind(&reservation_id[..])
    .bind(DbDocumentId(authority.document_id()))
    .bind(authority.authorized_generation())
    .bind(authority.dps_protocol_id())
    .bind(authority.protocol_contract_version())
    .bind(&authority.envelope_hash()[..])
    .bind(authority.capability_profile_version())
    .bind(authority.endpoint_config_revision())
    .execute(&mut **tx)
    .await
    .map_err(RecordError::Db)?
    .rows_affected();
    if updated != 1 {
        return Err(RecordError::AuthorityMismatch);
    }

    // Early node-safety (plan §2.3 step 8): record owns the halt, apply owns the release. The
    // FN key lives on the row (not the token); read it back for the mode CAS.
    let fiscal_number: Option<String> = sqlx::query_scalar(
        "SELECT fiscal_number FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&reservation_id[..])
    .fetch_optional(&mut **tx)
    .await
    .map_err(RecordError::Db)?;
    let fiscal_number = fiscal_number.ok_or(RecordError::ReservationMissing)?;

    if node_effect == NodeEffect::NodeBlocked {
        // `-11 ERROR_OFFLINE_168` — the FN is over the offline ceiling; block it (mirrors the
        // apply-time `-11` effect, set EARLY here so the halt survives even if apply is deferred).
        crate::db::repositories::node_state::set_mode_blocked_tx(tx, &fiscal_number)
            .await
            .map_err(RecordError::Db)?;
    } else if certainty == SubmissionCertainty::SubmittedUnknown
        || matches!(
            node_effect,
            NodeEffect::MacReseedPending
                | NodeEffect::OperatorEscalation
                | NodeEffect::ProbeRequired
                | NodeEffect::WrapperBug
        )
    {
        // UNRESOLVED — a `SUBMITTED_UNKNOWN` leaf (NoResponse / UnknownStatus / SaveError /
        // probe) or a held reject verdict (`-12` MacReseed, `-6` OperatorEscalation). Halt the
        // node pending operator completion. A clean `Accepted`, a terminal `Rejected`, or an
        // `FnConfigError` reject is RESOLVED — the apply path releases it, so NO halt here
        // (coherent with `apply_outcome`'s auto-release / HELD split).
        crate::db::repositories::node_state::set_mode_stop_mode_tx(tx, &fiscal_number)
            .await
            .map_err(RecordError::Db)?;
    }
    Ok(())
}

/// The result of an [`apply_outcome`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyResult {
    /// `true` = the effects were applied and the reservation marked `APPLIED` this call;
    /// `false` = an idempotent no-op (already `APPLIED`, or the generation CAS found the row
    /// stale / superseded — no ledger / seed / fence mutation happened).
    pub applied: bool,
    /// Whether the online MAC-chain seed (`last_known_unsigned_xml_sha256`) was advanced.
    pub seed_advanced: bool,
    /// The server fiscal number stamped on the document (Accepted only).
    pub server_fiscal_no: Option<String>,
}

/// Why [`apply_outcome`] could not proceed (nothing is mutated on any of these).
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    /// No reservation exists for the id.
    #[error("reservation not found")]
    ReservationNotFound,
    /// The reservation is not at `OUTCOME_OBSERVED` + `PENDING_APPLY` (nothing to apply).
    #[error("reservation is not OUTCOME_OBSERVED / PENDING_APPLY")]
    NotPendingApply,
    /// No `node_state` row for the fiscal number.
    #[error("node_state row missing")]
    NodeStateMissing,
    /// The immutable document row is gone.
    #[error("document row missing")]
    DocumentMissing,
    /// The recorded outcome is a HOLD (a `SubmittedUnknown` leaf, or an offline-origin reject):
    /// it must remain `PENDING_APPLY` under STOP_MODE and be resolved by the operator (Slice 5),
    /// never auto-released here (design §3.2 rows 5 / 7–9).
    #[error("outcome is a HOLD (offline reject / submitted-unknown) — operator completion only")]
    HeldNotAutoRelease,
    /// An online `Accepted` whose document carries no `unsigned_xml_sha256` to seed from.
    #[error("online Accepted has no unsigned_xml_sha256 to advance the seed")]
    MissingSeedHash,
    /// An `Accepted` row with no `evidence_text` (the accepted fiscal number).
    #[error("Accepted has no fiscal number (evidence_text)")]
    MissingFiscalNumber,
    /// The document could not leave `Sending` for its terminal state (wrong from-state / missing).
    #[error("document could not transition out of Sending: {0:?}")]
    DocTransitionFailed(crate::db::repositories::fiscal_documents::TransitionOutcome),
    /// A lower-level SQLite error.
    #[error(transparent)]
    Db(sqlx::Error),
}

/// Origin-sensitive repeatable apply of a recorded outcome (CS-3 Slice 4, design §3.2 + §4.3).
///
/// In the caller's single `BEGIN IMMEDIATE`, this re-reads the reservation + the immutable
/// document origin, runs the generation CAS
/// `{authorized_generation == node_state.delivery_generation AND active_delivery_reservation_id
/// == reservation_id}`, and — only for an **auto-release** outcome — performs the origin-split
/// effect and marks the reservation `APPLIED`, clearing the active pointer.  It is **repeatable
/// and idempotent**: a second call (or a stale-generation / already-APPLIED row) is a benign
/// no-op that mutates nothing.
///
/// Auto-release outcomes (design §3.2):
/// - `Accepted` (any origin): stamp the server fiscal number; **online** origin also advances the
///   seed (`offline_fiscal_no IS NULL`), **offline** origin performs **zero seed writes** (row 3);
/// - online-origin `Rejected` (definitive): no seed / no SFN; `NodeBlocked` flips node BLOCKED
///   (row 4 / row 6 online `-11`);
/// - `PreconditionFailed` / `SigningFailed` (safe NotSubmitted): local outcome only (row 10).
///
/// Everything else — a `SubmittedUnknown` leaf, or an **offline-origin** `Rejected` — is a HOLD:
/// [`ApplyError::HeldNotAutoRelease`] (it stays `PENDING_APPLY` under STOP_MODE for the operator,
/// Slice 5).
///
/// **INACTIVE (CS-3 Slice 4):** shadows the live `stage_send::run` 4-b; no production caller wires
/// it until the whole-fence cutover (Slice 7).
pub async fn apply_outcome(
    tx: &mut WriteTxConn<'_>,
    reservation_id: ReservationId,
) -> Result<ApplyResult, ApplyError> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        String,         // state
        Option<String>, // apply_state
        Option<i64>,    // authorized_generation
        String,         // fiscal_number
        Vec<u8>,        // document_id
        Option<String>, // evidence_kind
        Option<String>, // evidence_text
        Option<String>, // node_effect
    )> = sqlx::query_as(
        "SELECT state, apply_state, authorized_generation, fiscal_number, document_id, \
                evidence_kind, evidence_text, node_effect \
         FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&reservation_id[..])
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApplyError::Db)?;
    let (
        state,
        apply_state,
        authed_gen,
        fiscal_number,
        doc_id_blob,
        evidence_kind,
        evidence_text,
        node_effect,
    ) = row.ok_or(ApplyError::ReservationNotFound)?;

    // Idempotent no-op if already applied; refuse if not yet observed.
    match apply_state.as_deref() {
        Some("APPLIED") => {
            return Ok(ApplyResult {
                applied: false,
                seed_advanced: false,
                server_fiscal_no: None,
            })
        }
        Some("PENDING_APPLY") if state == "OUTCOME_OBSERVED" => {}
        _ => return Err(ApplyError::NotPendingApply),
    }

    // Generation CAS: the reservation must still be the node's active, current-generation one.
    let ns: Option<(i64, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT delivery_generation, active_delivery_reservation_id FROM node_state \
         WHERE fiscal_number = ?",
    )
    .bind(&fiscal_number)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApplyError::Db)?;
    let (cur_gen, active_ptr) = ns.ok_or(ApplyError::NodeStateMissing)?;
    let is_current =
        authed_gen == Some(cur_gen) && active_ptr.as_deref() == Some(&reservation_id[..]);
    if !is_current {
        // Stale / superseded — drop WITHOUT any mutation (design §7 apply-replay).
        return Ok(ApplyResult {
            applied: false,
            seed_advanced: false,
            server_fiscal_no: None,
        });
    }

    // Immutable document origin.
    let doc_id_arr: [u8; 16] = doc_id_blob
        .as_slice()
        .try_into()
        .map_err(|_| ApplyError::Db(sqlx::Error::Decode("document_id != 16 bytes".into())))?;
    let doc_id = DocumentId::from_bytes(doc_id_arr);
    let doc: Option<(Option<i64>, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT offline_fiscal_no, unsigned_xml_sha256 FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc_id))
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApplyError::Db)?;
    let (offline_fiscal_no, unsigned_sha) = doc.ok_or(ApplyError::DocumentMissing)?;
    let online = offline_fiscal_no.is_none();

    // Classify the recorded outcome into auto-release vs HOLD, and compute effects.
    let mut seed_advanced = false;
    let mut stamped_sfn: Option<String> = None;
    match evidence_kind.as_deref() {
        Some("Accepted") => {
            let sfn = evidence_text.ok_or(ApplyError::MissingFiscalNumber)?;
            fd_set_server_fiscal_no(tx, doc_id, &sfn).await?;
            if online {
                let sha = unsigned_sha.ok_or(ApplyError::MissingSeedHash)?;
                let arr: [u8; 32] = sha.as_slice().try_into().map_err(|_| {
                    ApplyError::Db(sqlx::Error::Decode(
                        "unsigned_xml_sha256 != 32 bytes".into(),
                    ))
                })?;
                node_advance_seed(tx, &fiscal_number, &arr).await?;
                seed_advanced = true;
            }
            // The issued document leaves `Sending` for the terminal issued state `Sent`
            // (D3: SFN stamped + online seed advanced ⟺ issued).
            doc_from_sending(tx, doc_id, DocState::Sent).await?;
            stamped_sfn = Some(sfn);
        }
        Some("Rejected") => {
            // Offline-origin reject is an operator HOLD (§3.2 row 5).
            if !online {
                return Err(ApplyError::HeldNotAutoRelease);
            }
            // Online-origin: `-12` (MacReseedPending) and `-6` (OperatorEscalation) are held
            // for MAC/operator resolution (§3.2, plan §0.5) — they must NOT auto-release. `-11`
            // (NodeBlocked) releases the reservation but flips the node BLOCKED. Every other
            // definitive verdict releases outright.
            match node_effect.as_deref() {
                Some("MacReseedPending") | Some("OperatorEscalation") => {
                    return Err(ApplyError::HeldNotAutoRelease);
                }
                Some("NodeBlocked") => {
                    node_set_blocked(tx, &fiscal_number).await?;
                    // A definitive pre-SENT reject leaves `Sending` for `Rejected` (D2:
                    // seed NOT advanced, SFN NOT stamped — the row rests non-issued).
                    doc_from_sending(tx, doc_id, DocState::Rejected).await?;
                }
                Some("FnConfigError") => {
                    // Round-2 #3: `-13`/`-14` (FN / signer not registered) retains the
                    // incumbent `ErrorRetryable` target — NOT the blanket `Rejected`
                    // projection. The R6 convergence policy then routes it to RMR/STOP
                    // WITHOUT a wire; a `Rejected` doc would never be escalated.
                    doc_from_sending(tx, doc_id, DocState::ErrorRetryable).await?;
                }
                _ => {
                    // A definitive pre-SENT reject leaves `Sending` for `Rejected` (D2:
                    // seed NOT advanced, SFN NOT stamped — the row rests non-issued).
                    doc_from_sending(tx, doc_id, DocState::Rejected).await?;
                }
            }
        }
        Some("PreconditionFailed") | Some("SigningFailed") => {
            // Safe NotSubmitted preflight failure — no wire, no issuance. The document is NOT
            // transitioned here: a NotSubmitted outcome pairs with a NotStarted reservation
            // (RN→OO, never CALL_STARTED — the generation↔certainty invariant), so it has no
            // in-flight `Sending` document to move. Local outcome only.
        }
        // SubmittedUnknown leaves (NoResponse / RemoteAuthStatus / UnknownStatus / SaveError /
        // CloseAmbiguous / MissingStatus / OkButNoFiscalNumber) and NULL evidence are HOLDs.
        _ => return Err(ApplyError::HeldNotAutoRelease),
    }

    // Mark APPLIED + clear the active pointer, atomically with the effects above.
    let applied_rows = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'APPLIED' \
         WHERE reservation_id = ? AND apply_state = 'PENDING_APPLY'",
    )
    .bind(&reservation_id[..])
    .execute(&mut **tx)
    .await
    .map_err(ApplyError::Db)?
    .rows_affected();
    if applied_rows != 1 {
        return Err(ApplyError::NotPendingApply);
    }
    sqlx::query(
        "UPDATE node_state SET active_delivery_reservation_id = NULL WHERE fiscal_number = ?",
    )
    .bind(&fiscal_number)
    .execute(&mut **tx)
    .await
    .map_err(ApplyError::Db)?;

    Ok(ApplyResult {
        applied: true,
        seed_advanced,
        server_fiscal_no: stamped_sfn,
    })
}

// Thin adapters to the sibling repositories, mapping their sqlx errors into ApplyError and
// treating a missing-row (`false`) as a structural breach.
async fn fd_set_server_fiscal_no(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    sfn: &str,
) -> Result<(), ApplyError> {
    let ok = crate::db::repositories::fiscal_documents::set_server_fiscal_no_tx(tx, doc_id, sfn)
        .await
        .map_err(ApplyError::Db)?;
    if ok {
        Ok(())
    } else {
        Err(ApplyError::DocumentMissing)
    }
}

async fn node_advance_seed(
    tx: &mut WriteTxConn<'_>,
    fn_id: &str,
    hash: &[u8; 32],
) -> Result<(), ApplyError> {
    let ok = crate::db::repositories::node_state::update_last_known_xml_sha_tx(tx, fn_id, hash)
        .await
        .map_err(ApplyError::Db)?;
    if ok {
        Ok(())
    } else {
        Err(ApplyError::NodeStateMissing)
    }
}

async fn node_set_blocked(tx: &mut WriteTxConn<'_>, fn_id: &str) -> Result<(), ApplyError> {
    crate::db::repositories::node_state::set_mode_blocked_tx(tx, fn_id)
        .await
        .map_err(ApplyError::Db)?;
    Ok(())
}

/// Move the document out of `Sending` to its terminal apply target (`Sent` on a clean accept,
/// `Rejected` on a definitive pre-SENT reject). Any non-`Applied` outcome (the document is not in
/// `Sending`) surfaces as an error so the whole apply rolls back — the pointer must never clear
/// while the document rests in `Sending`.
async fn doc_from_sending(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    to: DocState,
) -> Result<(), ApplyError> {
    use crate::db::repositories::fiscal_documents::{transition_state, TransitionOutcome};
    let outcome = transition_state(tx, doc_id, DocState::Sending, to)
        .await
        .map_err(ApplyError::Db)?;
    match outcome {
        TransitionOutcome::Applied => Ok(()),
        other => Err(ApplyError::DocTransitionFailed(other)),
    }
}

/// List the reservations resting at `CALL_STARTED` (the crash-mid-send set) for boot resume.
///
/// A reservation only ever rests at `CALL_STARTED` if the node crashed after committing the
/// wire marker but before the outcome was recorded — a completed wire moves it to
/// `OUTCOME_OBSERVED`.  Returns `(reservation_id, fiscal_number)` for each; the caller resumes
/// each via [`resume_crashed_reservation`] in its own transaction (design §4.3 step 5).
/// Read-only, pool-bound.
pub async fn list_call_started_without_outcome(
    pool: &sqlx::SqlitePool,
) -> sqlx::Result<Vec<(ReservationId, String)>> {
    let rows: Vec<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT reservation_id, fiscal_number FROM delivery_reservation WHERE state = 'CALL_STARTED'",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, fscl) in rows {
        let arr: [u8; 16] = id.as_slice().try_into().map_err(|_| {
            sqlx::Error::Decode("delivery_reservation.reservation_id != 16 bytes".into())
        })?;
        out.push((arr, fscl));
    }
    Ok(out)
}

/// Boot-resume a crashed `CALL_STARTED` reservation (CS-3 Slice 4, design §4.3 step 5).
///
/// Converts, in the caller's single `BEGIN IMMEDIATE`, a `CALL_STARTED` reservation with no
/// recorded outcome to the durable `NoResponse { CrashedBeforeObservation }` leaf
/// (`SUBMITTED_UNKNOWN` / `NO_RESPONSE`) at `OUTCOME_OBSERVED` + `PENDING_APPLY`, and sets node
/// `STOP_MODE`.  This is a **local recovery write, NOT a synthetic DPS response and NOT a
/// resend** — boot performs no wire I/O.  The row then holds the FN fence (the §3.1
/// `OUTCOME_OBSERVED` + `PENDING_APPLY` slot) until operator completion (Slice 5).
///
/// Returns `true` if a `CALL_STARTED` row was converted, `false` if it was not in that state
/// (idempotent — a re-run after conversion is a no-op).
pub async fn resume_crashed_reservation(
    tx: &mut WriteTxConn<'_>,
    reservation_id: ReservationId,
    fiscal_number: &str,
) -> Result<bool, ApplyError> {
    let converted = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'NoResponse', evidence_text = 'CrashedBeforeObservation' \
         WHERE reservation_id = ? AND state = 'CALL_STARTED'",
    )
    .bind(&reservation_id[..])
    .execute(&mut **tx)
    .await
    .map_err(ApplyError::Db)?
    .rows_affected()
        == 1;
    if converted {
        crate::db::repositories::node_state::set_mode_stop_mode_tx(tx, fiscal_number)
            .await
            .map_err(ApplyError::Db)?;
    }
    Ok(converted)
}

/// The typed operator resolution for a PENDING reservation (CS-3 Slice 5, design §3.4).
/// Trusted administrative input, always durable in the audit log at the call site.
#[derive(Clone, Debug)]
pub enum OperatorResolution {
    /// DPS accepted the document; the operator supplies the observed non-empty fiscal number.
    Accepted { fiscal_number: String },
    /// DPS did not accept it; the document goes to its manual/terminal state. ONLINE origin only
    /// (the offline analogue is [`NotAcceptedOffline`], which additionally cleans the OLA cohort).
    NotAccepted,
    /// CS-3 gap 4b — an OFFLINE-origin document DPS did not accept. Carries NO operator seed: the
    /// chain is rewound to the document's own immutable `previous_hash` (the value it saw at
    /// signing), and every LATER offline `OFFLINE_LOCAL_ACK` successor in the session is cancelled
    /// atomically (any later ISSUED successor fails the completion closed — a fork guard).
    NotAcceptedOffline,
    /// `MacReseedPending` (-12): the operator supplies the corrected chain seed. ONLINE origin only.
    MacReseed { seed: [u8; 32] },
}

/// The node mode selected after a successful operator completion (design §3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeTarget {
    /// No active offline session — the FN returns directly to ONLINE.
    Online,
    /// An active OPEN/DRAINING offline session must finish draining first.
    GoingOnline,
}

impl ModeTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::Online => "ONLINE",
            Self::GoingOnline => "GOING_ONLINE",
        }
    }
}

/// The result of a successful [`complete_operator_pending`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionResult {
    pub applied: bool,
    pub mode_target: ModeTarget,
    pub seed_advanced: bool,
    pub server_fiscal_no: Option<String>,
    /// CS-3 gap 4b — the OLA cohort successors cancelled by an offline not-accepted completion
    /// (`(document_id, lnd)`, in `lnd` order). Empty for every other resolution. The caller's
    /// Critical audit itemises these (P4 — the cancellation set must be on the record).
    pub cancelled_cohort: Vec<(DocumentId, i64)>,
    /// CS-3 gap 4b — the predecessor seed the chain was rewound to by an offline not-accepted
    /// completion: `None` = no rewind (any other resolution); `Some(None)` = rewound to GENESIS
    /// (the document chained off genesis); `Some(Some(h))` = rewound to the 32-byte predecessor hash.
    pub rewound_predecessor_seed: Option<Option<[u8; 32]>>,
}

/// Why [`complete_operator_pending`] refused (nothing is mutated on any of these).
#[derive(Debug, thiserror::Error)]
pub enum CompletionError {
    #[error("reservation not found")]
    ReservationNotFound,
    #[error("reservation is not OUTCOME_OBSERVED / PENDING_APPLY")]
    NotPendingApply,
    #[error("node_state row missing")]
    NodeStateMissing,
    #[error("document row missing")]
    DocumentMissing,
    /// The full authority predicate failed: the stored generation no longer equals the node's,
    /// or the active pointer no longer names this reservation — the operator resolution is stale.
    #[error("stale authority (generation / active-pointer CAS miss)")]
    StaleAuthority,
    #[error("Accepted requires a non-empty fiscal number")]
    EmptyFiscalNumber,
    #[error("online Accepted has no unsigned_xml_sha256 to advance the seed")]
    MissingSeedHash,
    #[error("the document could not move Sending -> RMR: {0:?}")]
    DocTransitionFailed(crate::db::repositories::fiscal_documents::TransitionOutcome),
    /// CS-3 gap 4b — the resolution variant's origin claim contradicts the DB-derived origin
    /// (`NotAccepted` on an offline doc, or `NotAcceptedOffline` on an online doc). The DB row is
    /// the sole origin authority; a mismatch is fail-closed (else an offline seed rewind could
    /// clobber a live online chain).
    #[error("resolution origin does not match the document's origin")]
    OriginMismatch,
    /// CS-3 gap 4b — a `MacReseed` resolution on an OFFLINE-origin document. §3.4 has no offline
    /// MacReseed cell; refused rather than blindly advancing the offline seed (a fork hazard).
    #[error("MacReseed is not defined for an offline-origin document")]
    MacReseedNotOfflineDefined,
    /// CS-3 gap 4b — a later offline successor in the session is already ISSUED (advanced the local
    /// MAC seed at OFFLINE_LOCAL_ACK: SENT / KVT1 / KVT2 / ERROR_RETRYABLE / REJECTED / RMR / ACK).
    /// Rewinding this predecessor would fork the chain — fail-closed, nothing mutated.
    #[error(
        "later offline successor is already issued (doc {0}, lnd {1}, state {2}) — fork guard"
    )]
    LaterSuccessorIssued(String, i64, String),
    /// CS-3 gap 4b — a later offline successor is mid-wire on the online ladder (`SENDING`).
    /// Fail-closed, nothing mutated.
    #[error("later offline successor is in-flight (doc {0}) — fork guard")]
    LaterSuccessorInFlight(String),
    /// CS-3 gap 4b — a later offline successor rests in a pre-OLA staging state
    /// (`PREPARED` / `SIGNED` / `ENCRYPTED`) that cannot legitimately exist for a committed offline
    /// cohort. Structural breach — fail-closed, nothing mutated.
    #[error("later offline successor in invalid staging state (doc {0}, state {1})")]
    LaterSuccessorInvalidState(String, String),
    /// The `STOP_MODE -> target` mode CAS matched no row: the node left STOP_MODE concurrently
    /// (a BLOCKED `-11` node, or a racing reset). The operator resolution is stale — the whole tx
    /// rolls back rather than APPLYING under an unexpected mode.
    #[error("node is not in STOP_MODE (mode CAS matched no row — stale resolution)")]
    NodeNotStopMode,
    #[error(transparent)]
    Db(sqlx::Error),
}

/// Operator completion of a PENDING reservation held under STOP_MODE (CS-3 Slice 5, design §3.4).
///
/// The single release surface for a `SubmittedUnknown` / crashed reservation. Runs in the caller's
/// `BEGIN IMMEDIATE` **after** the caller has performed the pre-transaction `status_rro` probe
/// (outside the tx — invariant #1); the caller passes the derived [`OperatorResolution`]. Steps:
///
/// 1. re-read the reservation (must be `OUTCOME_OBSERVED` + `PENDING_APPLY`);
/// 2. the full authority CAS — stored `authorized_generation` == node generation AND
///    `active_delivery_reservation_id` == this reservation ([`CompletionError::StaleAuthority`]
///    otherwise, whole tx rolls back);
/// 3. the origin-split effect per resolution:
///    - `Accepted{F}` — stamp F; **online** advances the seed from the document's immutable
///      `unsigned_xml_sha256`, **offline** performs zero seed writes (chain already advanced);
///    - `NotAccepted` (online) — document `Sending -> RequiresManualReconciliation`, seed unchanged;
///    - `MacReseed{seed}` — install the operator seed, move the document to its manual state;
/// 4. mark `APPLIED` and clear the active pointer (the sole pointer-clearing branch, §3.4);
/// 5. select the mode target — `GOING_ONLINE` iff an active OPEN/DRAINING offline session exists,
///    else `ONLINE` — and CAS the node out of STOP_MODE.
///
/// **Fail-closed boundaries (Slice 5b):** a shift-family document → [`ShiftFamilyNotSupported`];
/// an **offline** `NotAccepted` → [`OfflineCohortCleanupRequired`] (never half-applied — cancelling
/// an OLA cohort or refusing is mandatory to avoid a fork). The `-11` BLOCKED branch + the
/// admin-command probe wiring / `open_shift` agreement are also Slice 5b.
///
/// **INACTIVE (CS-3 Slice 5):** invoked by the extended `reset_stop_mode` admin path only after the
/// whole-fence cutover (Slice 7); exercised by operator-completion tests today.
pub async fn complete_operator_pending(
    tx: &mut WriteTxConn<'_>,
    reservation_id: ReservationId,
    resolution: OperatorResolution,
) -> Result<CompletionResult, CompletionError> {
    #[allow(clippy::type_complexity)]
    let row: Option<(String, Option<String>, Option<i64>, String, Vec<u8>)> = sqlx::query_as(
        "SELECT state, apply_state, authorized_generation, fiscal_number, document_id \
         FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&reservation_id[..])
    .fetch_optional(&mut **tx)
    .await
    .map_err(CompletionError::Db)?;
    let (state, apply_state, authed_gen, fiscal_number, doc_id_blob) =
        row.ok_or(CompletionError::ReservationNotFound)?;
    if state != "OUTCOME_OBSERVED" || apply_state.as_deref() != Some("PENDING_APPLY") {
        return Err(CompletionError::NotPendingApply);
    }

    // Full authority CAS (design §3.4): generation + active pointer must still name this row.
    let ns: Option<(i64, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT delivery_generation, active_delivery_reservation_id FROM node_state \
         WHERE fiscal_number = ?",
    )
    .bind(&fiscal_number)
    .fetch_optional(&mut **tx)
    .await
    .map_err(CompletionError::Db)?;
    let (cur_gen, active_ptr) = ns.ok_or(CompletionError::NodeStateMissing)?;
    if authed_gen != Some(cur_gen) || active_ptr.as_deref() != Some(&reservation_id[..]) {
        return Err(CompletionError::StaleAuthority);
    }

    // Immutable document origin + family.
    let doc_id_arr: [u8; 16] = doc_id_blob
        .as_slice()
        .try_into()
        .map_err(|_| CompletionError::Db(sqlx::Error::Decode("document_id != 16 bytes".into())))?;
    let doc_id = DocumentId::from_bytes(doc_id_arr);
    #[allow(clippy::type_complexity)]
    let doc: Option<(
        Option<i64>,     // offline_fiscal_no
        Option<Vec<u8>>, // unsigned_xml_sha256
        Option<Vec<u8>>, // offline_session_id
        i64,             // lnd
        Option<Vec<u8>>, // previous_hash
    )> = sqlx::query_as(
        "SELECT offline_fiscal_no, unsigned_xml_sha256, offline_session_id, lnd, previous_hash \
         FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc_id))
    .fetch_optional(&mut **tx)
    .await
    .map_err(CompletionError::Db)?;
    let (offline_fiscal_no, unsigned_sha, offline_session_id, doc_lnd, doc_previous_hash) =
        doc.ok_or(CompletionError::DocumentMissing)?;
    let online = offline_fiscal_no.is_none();

    // CS-3 gap 4b — origin cross-check: the DB row is the SOLE origin authority (the resolution
    // variant supplies payload/intent only). A variant whose origin contradicts the row is
    // fail-closed BEFORE any mutation — an offline seed rewind on an online doc (or vice versa)
    // would fork a live chain. Online shift-family is handled uniformly here (the shift-state
    // projection is the operator-completion service's `apply_shift_transition`); offline
    // shift-family is likewise doc/cohort/seed here + the OLPD/CLPD rollback in the service.
    match &resolution {
        OperatorResolution::NotAccepted if !online => return Err(CompletionError::OriginMismatch),
        OperatorResolution::NotAcceptedOffline if online => {
            return Err(CompletionError::OriginMismatch)
        }
        OperatorResolution::MacReseed { .. } if !online => {
            return Err(CompletionError::MacReseedNotOfflineDefined)
        }
        _ => {}
    }

    let mut seed_advanced = false;
    let mut stamped_sfn: Option<String> = None;
    let mut cancelled_cohort: Vec<(DocumentId, i64)> = Vec::new();
    let mut rewound_predecessor_seed: Option<Option<[u8; 32]>> = None;
    match &resolution {
        OperatorResolution::Accepted { fiscal_number: f } => {
            if f.is_empty() {
                return Err(CompletionError::EmptyFiscalNumber);
            }
            fd_set_server_fiscal_no(tx, doc_id, f)
                .await
                .map_err(completion_from_apply)?;
            if online {
                let sha = unsigned_sha.ok_or(CompletionError::MissingSeedHash)?;
                let arr: [u8; 32] = sha.as_slice().try_into().map_err(|_| {
                    CompletionError::Db(sqlx::Error::Decode(
                        "unsigned_xml_sha256 != 32 bytes".into(),
                    ))
                })?;
                node_advance_seed(tx, &fiscal_number, &arr)
                    .await
                    .map_err(completion_from_apply)?;
                seed_advanced = true;
            }
            // Accepted = issued: the document leaves `Sending` for the terminal issued state
            // `Sent` (core fix — completion previously stamped SFN/seed + cleared the pointer but
            // left the document resting in `Sending`, violating the no-non-terminal-at-quiescence
            // pin). Atomic with the SFN/seed above.
            doc_from_sending(tx, doc_id, DocState::Sent)
                .await
                .map_err(completion_from_apply)?;
            stamped_sfn = Some(f.clone());
        }
        OperatorResolution::NotAccepted => {
            // Online-origin not-accepted (the cross-check ensured `online`): manual/terminal,
            // seed unchanged.
            doc_to_rmr(tx, doc_id).await?;
        }
        OperatorResolution::NotAcceptedOffline => {
            // CS-3 gap 4b (the cross-check ensured `!online`): atomically cancel the later OLA
            // cohort (fork guard — any later ISSUED successor fails closed) + rewind the chain to
            // THIS document's own immutable `previous_hash`, then move the doc manual/terminal.
            let session = offline_session_id.as_deref().ok_or_else(|| {
                CompletionError::Db(sqlx::Error::Decode(
                    "offline not-accepted document has no offline_session_id".into(),
                ))
            })?;
            let cancelled = offline_cohort_cleanup(tx, &fiscal_number, session, doc_lnd).await?;
            let prev: Option<[u8; 32]> = match &doc_previous_hash {
                Some(v) => Some(v.as_slice().try_into().map_err(|_| {
                    CompletionError::Db(sqlx::Error::Decode("previous_hash != 32 bytes".into()))
                })?),
                None => None,
            };
            // Rewind (Some -> predecessor, None -> genesis NULL). `false` = missing FN -> breach.
            let ok = crate::db::repositories::node_state::set_last_known_xml_sha_nullable_tx(
                tx,
                &fiscal_number,
                prev.as_ref(),
            )
            .await
            .map_err(CompletionError::Db)?;
            if !ok {
                return Err(CompletionError::NodeStateMissing);
            }
            cancelled_cohort = cancelled;
            rewound_predecessor_seed = Some(prev);
            doc_to_rmr(tx, doc_id).await?;
        }
        OperatorResolution::MacReseed { seed } => {
            node_advance_seed(tx, &fiscal_number, seed)
                .await
                .map_err(completion_from_apply)?;
            seed_advanced = true;
            doc_to_rmr(tx, doc_id).await?;
        }
    }

    // Mark APPLIED + clear the active pointer (the sole pointer-clearing branch).
    let applied_rows = sqlx::query(
        "UPDATE delivery_reservation SET apply_state = 'APPLIED' \
         WHERE reservation_id = ? AND apply_state = 'PENDING_APPLY'",
    )
    .bind(&reservation_id[..])
    .execute(&mut **tx)
    .await
    .map_err(CompletionError::Db)?
    .rows_affected();
    if applied_rows != 1 {
        return Err(CompletionError::NotPendingApply);
    }
    sqlx::query(
        "UPDATE node_state SET active_delivery_reservation_id = NULL WHERE fiscal_number = ?",
    )
    .bind(&fiscal_number)
    .execute(&mut **tx)
    .await
    .map_err(CompletionError::Db)?;

    // Verified mode target: GOING_ONLINE iff an active drain must finish, else ONLINE.
    let active_session: Option<String> = sqlx::query_scalar(
        "SELECT state FROM offline_sessions \
         WHERE fiscal_number = ? AND state IN ('OPENING','OPEN','DRAINING') LIMIT 1",
    )
    .bind(&fiscal_number)
    .fetch_optional(&mut **tx)
    .await
    .map_err(CompletionError::Db)?;
    let mode_target = if active_session.is_some() {
        ModeTarget::GoingOnline
    } else {
        ModeTarget::Online
    };
    let mode_rows = sqlx::query(
        "UPDATE node_state SET mode = ? WHERE fiscal_number = ? AND mode = 'STOP_MODE'",
    )
    .bind(mode_target.as_str())
    .bind(&fiscal_number)
    .execute(&mut **tx)
    .await
    .map_err(CompletionError::Db)?
    .rows_affected();
    // Core fix: the STOP_MODE → target CAS must move exactly one row. A PENDING reservation is
    // held under STOP_MODE, so a 0-row CAS means the node left STOP_MODE concurrently (a BLOCKED
    // `-11` node or a racing reset) — the operator resolution is stale; roll the whole tx back
    // rather than silently APPLYING under an unexpected mode.
    if mode_rows != 1 {
        return Err(CompletionError::NodeNotStopMode);
    }

    Ok(CompletionResult {
        applied: true,
        mode_target,
        seed_advanced,
        server_fiscal_no: stamped_sfn,
        cancelled_cohort,
        rewound_predecessor_seed,
    })
}

/// CS-3 gap 4b — atomically clean the OFFLINE OLA cohort of successors LATER than `this_lnd` in
/// the same offline session, on the caller's `WriteTxConn` (design §3.4, fork-safety rev2 §2.2).
///
/// **Fork fence.** The later-successor read is TX-BOUND — the same `BEGIN IMMEDIATE` snapshot the
/// RESERVED write-lock already froze against concurrent drain writers (NEVER the pool-bound
/// `list_pending_for_session`). ALL rows are classified BEFORE any mutation: only a later
/// `OFFLINE_LOCAL_ACK` successor is cancellable (via the sole `(OfflineLocalAck, Cancelled)` edge);
/// a `CANCELLED` / `ABORTED` successor is dead and ignored; a `SENDING` successor is in-flight; a
/// pre-OLA `PREPARED` / `SIGNED` / `ENCRYPTED` successor is a structural breach; anything else is
/// ISSUED (advanced the local MAC seed at OLA — `OFFLINE_ISSUED_STATES`) and rewinding this
/// predecessor would fork the chain. Each of those fails the completion closed, mutating nothing
/// (a late `Err` still rolls the whole tx back). Each cancel CAS is asserted `Applied` (the in-tx
/// TOCTOU re-check). Returns the cancelled `(document_id, lnd)` list for the caller's P4 audit.
async fn offline_cohort_cleanup(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    offline_session_id: &[u8],
    this_lnd: i64,
) -> Result<Vec<(DocumentId, i64)>, CompletionError> {
    use crate::db::repositories::fiscal_documents::{transition_state, TransitionOutcome};
    // C2 — tx-bound later-successor read (same snapshot as the RESERVED lock).
    let rows: Vec<(Vec<u8>, String, i64)> = sqlx::query_as(
        "SELECT document_id, state, lnd FROM fiscal_documents \
         WHERE fiscal_number = ? AND offline_session_id = ? AND lnd > ? ORDER BY lnd ASC",
    )
    .bind(fiscal_number)
    .bind(offline_session_id)
    .bind(this_lnd)
    .fetch_all(&mut **tx)
    .await
    .map_err(CompletionError::Db)?;

    // C3 — classify ALL rows first (fork fence); never mutate mid-scan.
    let mut cancellable: Vec<(DocumentId, i64)> = Vec::new();
    for (id_blob, state, lnd) in &rows {
        let arr: [u8; 16] = id_blob.as_slice().try_into().map_err(|_| {
            CompletionError::Db(sqlx::Error::Decode(
                "successor document_id != 16 bytes".into(),
            ))
        })?;
        let id = DocumentId::from_bytes(arr);
        let id_hex = hex_lower_bytes(id_blob);
        match state.as_str() {
            "OFFLINE_LOCAL_ACK" => cancellable.push((id, *lnd)),
            // Dead / non-issued — not a live fork, ignore.
            "CANCELLED" | "ABORTED" => {}
            // Mid-wire on the online ladder.
            "SENDING" => return Err(CompletionError::LaterSuccessorInFlight(id_hex)),
            // Pre-OLA staging cannot legitimately exist for a committed offline cohort.
            "PREPARED" | "SIGNED" | "ENCRYPTED" => {
                return Err(CompletionError::LaterSuccessorInvalidState(
                    id_hex,
                    state.clone(),
                ))
            }
            // Everything else (SENT / KVT1 / KVT2 / ERROR_RETRYABLE / REJECTED / RMR / ACK / any
            // unknown) is ISSUED or unrecognised → fork guard.
            _ => {
                return Err(CompletionError::LaterSuccessorIssued(
                    id_hex,
                    *lnd,
                    state.clone(),
                ))
            }
        }
    }

    // C4 — cancel every cancellable OLA successor; assert `Applied` (in-tx TOCTOU re-check).
    for (id, _lnd) in &cancellable {
        let outcome = transition_state(tx, *id, DocState::OfflineLocalAck, DocState::Cancelled)
            .await
            .map_err(CompletionError::Db)?;
        if outcome != TransitionOutcome::Applied {
            return Err(CompletionError::DocTransitionFailed(outcome));
        }
    }
    Ok(cancellable)
}

/// Lowercase-hex a byte slice (for typed-error document ids). Local — avoids a repo→service
/// dependency on `write_path::types::hex_encode_lower`.
fn hex_lower_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Move a `Sending` document to `RequiresManualReconciliation` (the operator manual/terminal edge,
/// design §3.4). Any non-`Applied` outcome (wrong from-state / missing) surfaces as an error so the
/// whole completion rolls back.
async fn doc_to_rmr(tx: &mut WriteTxConn<'_>, doc_id: DocumentId) -> Result<(), CompletionError> {
    use crate::db::repositories::fiscal_documents::{transition_state, TransitionOutcome};
    let outcome = transition_state(
        tx,
        doc_id,
        DocState::Sending,
        DocState::RequiresManualReconciliation,
    )
    .await
    .map_err(CompletionError::Db)?;
    match outcome {
        TransitionOutcome::Applied => Ok(()),
        other => Err(CompletionError::DocTransitionFailed(other)),
    }
}

/// Map an [`ApplyError`] from a shared adapter (`fd_set_server_fiscal_no` / `node_advance_seed`)
/// into the corresponding [`CompletionError`].
fn completion_from_apply(e: ApplyError) -> CompletionError {
    match e {
        ApplyError::DocumentMissing => CompletionError::DocumentMissing,
        ApplyError::NodeStateMissing => CompletionError::NodeStateMissing,
        ApplyError::DocTransitionFailed(o) => CompletionError::DocTransitionFailed(o),
        ApplyError::Db(db) => CompletionError::Db(db),
        // The adapters + `doc_from_sending` only ever produce the four above.
        other => CompletionError::Db(sqlx::Error::Decode(format!("unexpected: {other}").into())),
    }
}
