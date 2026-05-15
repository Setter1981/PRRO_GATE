//! Repository for `offline_sessions` + `offline_codes` (M3b W4 schema).
//!
//! Surfaces the OPENING → OPEN → DRAINING → CLOSED (+ ABORTED)
//! state machine for offline sessions, plus the atomic FN-scoped
//! code-pool primitives (`seed_code_range`, `acquire_code_tx`,
//! `list_pending_for_session`).
//!
//! ## Repository policy
//!
//! - Mutating helpers take `&mut WriteTxConn<'_>` — callers obtain it
//!   from a `with_immediate` closure, which guarantees BEGIN IMMEDIATE
//!   envelope hygiene (no foreign IO inside; single-writer-per-FN
//!   via SQLite WAL RESERVED-lock serialisation).  This is the W5
//!   review axis 2 ("all writes through `WriteTxConn` /
//!   `with_immediate`").
//! - `seed_code_range` is the one exception: it is an **admin
//!   seam** invoked outside the runtime write path (e.g., during
//!   provisioning when an operator pushes a DPS-issued code range
//!   into local storage).  It runs in autocommit on the pool — the
//!   single INSERT OR IGNORE statement is itself atomic, and there
//!   is no other writer to contend with at provisioning time.
//! - `list_pending_for_session` is a read; pool-bound.
//!
//! ## Typed-error surface (W5 review axis 4)
//!
//! Schema-enforced invariants from W4 are mapped to typed variants
//! of [`OfflineSessionError`] so callers can branch on the structural
//! condition (STOP_MODE / re-attempt / operator escalation) without
//! string-matching `anyhow` messages:
//!
//! | W4 schema enforcement                            | typed variant                                |
//! |--------------------------------------------------|----------------------------------------------|
//! | partial UNIQUE `ux_offline_active`               | [`OfflineSessionError::AnotherSessionActive`] |
//! | trigger `offline_codes_consumed_immutable`       | [`OfflineSessionError::OfflineCodeAlreadyConsumed`] |
//! | partial UNIQUE `ux_offline_codes_consumed_by_doc`| [`OfflineSessionError::OfflineCodeAlreadyConsumed`] |
//! | (no available codes for FN)                      | [`OfflineSessionError::CodePoolExhausted`]    |
//! | ABORTED transition without reason_abort          | [`OfflineSessionError::MissingReasonAbort`]   |
//!
//! Whitelist misses surface as [`TransitionOutcome::Forbidden`] (an
//! enum variant — typed, not anyhow) mirroring
//! `fiscal_documents::transition_state`; the service-layer caller
//! decides whether `Forbidden` is a caller bug or an expected race
//! and either way it's structurally distinguishable.

use crate::db::models::enums::OfflineSessionState;
use crate::db::models::ids::{DocumentId, OfflineSessionId};
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::tx::WriteTxConn;
use sqlx::SqlitePool;

/// Errors that callers of this repository may need to branch on.
///
/// `Database(sqlx::Error)` is the catch-all for sqlx errors that
/// don't match a known constraint signature; production callers
/// should `?`-propagate it.  All other variants identify a
/// schema-enforced or contract-level condition that the caller can
/// react to (e.g., enter STOP_MODE on `CodePoolExhausted`).
#[derive(Debug, thiserror::Error)]
pub enum OfflineSessionError {
    /// `ux_offline_active` partial UNIQUE fired: another session in
    /// OPENING / OPEN / DRAINING already exists for this FN.
    /// Surfaced from [`insert_opening`].
    #[error("another active offline session exists for fiscal_number={fiscal_number}")]
    AnotherSessionActive { fiscal_number: String },

    /// [`acquire_code_tx`] found zero rows with `consumed_at IS NULL`
    /// for the FN's code pool.  Operational meaning: node MUST
    /// enter STOP_MODE (M3b spec §5.5) — cannot continue offline
    /// fiscalisation without a code.
    #[error("offline code pool exhausted for fiscal_number={fiscal_number}")]
    CodePoolExhausted { fiscal_number: String },

    /// Either the W4 trigger `offline_codes_consumed_immutable` or
    /// the partial UNIQUE `ux_offline_codes_consumed_by_doc` fired.
    /// Both signal "this code is already taken" from different
    /// angles (immutability of the consumed row vs uniqueness of
    /// the doc-link).  Operational meaning: race / programming bug
    /// in caller; caller may retry against the next available code
    /// or escalate.
    #[error("offline code already consumed by another document")]
    OfflineCodeAlreadyConsumed,

    /// Caller attempted an ABORTED transition without supplying a
    /// non-empty `reason_abort` value.  The W4 schema permits NULL
    /// for back-compat with pre-W4 rows, but W5's helper enforces
    /// the requirement in Rust per operator review pin (memory
    /// `m3b-w5-review-criteria`).
    #[error("reason_abort is required for ABORTED transitions")]
    MissingReasonAbort,

    /// Unclassified sqlx error.  Includes connection failures, DB
    /// IO errors, and constraint violations that no typed-variant
    /// match recognised (e.g., FK errors which indicate programming
    /// bugs).
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Input for [`insert_opening`].  Caller pre-generates the
/// `OfflineSessionId` so it can be returned synchronously from the
/// service-layer wrapper without a round-trip through the DB.
#[derive(Debug, Clone)]
pub struct NewOpeningSession<'a> {
    pub offline_session_id: OfflineSessionId,
    pub fiscal_number: &'a str,
    /// ISO-8601 timestamp string.  Pre-formatted by the caller (M3b
    /// keeps timestamp policy at the service layer where business
    /// time is canonical, see `services::write_path::stage_acquire`).
    pub opened_at: &'a str,
}

/// Result of [`acquire_code_tx`] on success.  Returned alongside
/// the chosen `code_lnd` is the `consumed_at` timestamp that the
/// DB stamped via `CURRENT_TIMESTAMP` so the service layer can
/// echo it back to the caller without a follow-up SELECT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquiredCode {
    pub code_lnd: i64,
    pub consumed_at: String,
}

/// Whitelist of legal `offline_sessions.state` transitions.
///
/// Parallel shape to `fiscal_documents::allowed_transition` per M3b
/// plan §Task 5 line 451:
///
/// - `Opening → Open` (open succeeded, audit `OFFLINE_SESSION_OPENED`)
/// - `Open → Draining` (return-online detected, audit `OFFLINE_SESSION_DRAIN_STARTED`)
/// - `Draining → Closed` (drain complete, audit `OFFLINE_SESSION_CLOSED`)
/// - `Opening → Aborted` / `Open → Aborted` / `Draining → Aborted`
///   (operator / drain failure, audit `OFFLINE_SESSION_ABORTED`)
///
/// Every other pair is rejected with [`TransitionOutcome::Forbidden`]
/// before any DB call.
pub fn allowed_transition(from: OfflineSessionState, to: OfflineSessionState) -> bool {
    use OfflineSessionState::*;
    matches!(
        (from, to),
        (Opening, Open)
            | (Opening, Aborted)
            | (Open, Draining)
            | (Open, Aborted)
            | (Draining, Closed)
            | (Draining, Aborted)
    )
}

/// INSERT a new session in OPENING state.
///
/// W4 schema enforces partial UNIQUE `ux_offline_active` on
/// `(fiscal_number) WHERE state IN ('OPENING','OPEN','DRAINING')` —
/// if any active session already exists for the FN, this INSERT
/// returns [`OfflineSessionError::AnotherSessionActive`].
pub async fn insert_opening(
    tx: &mut WriteTxConn<'_>,
    n: &NewOpeningSession<'_>,
) -> Result<(), OfflineSessionError> {
    let res = sqlx::query(
        "INSERT INTO offline_sessions (offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPENING', ?)",
    )
    .bind(n.offline_session_id)
    .bind(n.fiscal_number)
    .bind(n.opened_at)
    .execute(&mut **tx)
    .await;
    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(classify_insert_opening_error(e, n.fiscal_number)),
    }
}

/// Atomic state transition for a single session row.
///
/// Whitelist gate runs BEFORE the DB call; an off-whitelist
/// transition returns `Ok(TransitionOutcome::Forbidden)` without
/// touching the DB.  Successful CAS returns `Applied`; CAS miss
/// triggers a follow-up existence check to disambiguate
/// `Conflict` (row exists but state diverged) from `NotFound`.
///
/// ## Column-population semantics (W4 columns + W5 helper contract)
///
/// Per operator review pin (memory `m3b-w5-review-criteria`):
/// each column has exactly one transition that stamps it; other
/// transitions leave the column untouched.
///
/// - `→ Draining`: stamps `drained_at = COALESCE(drained_at, CURRENT_TIMESTAMP)`
///   in the same UPDATE.  COALESCE preserves the original timestamp
///   on re-entry (defence-in-depth — the whitelist already forbids
///   re-entry into DRAINING, but if a future whitelist edit adds
///   such an edge the stamp stays correct).
/// - `→ Closed`: stamps `closed_at = COALESCE(closed_at, CURRENT_TIMESTAMP)`.
///   Only DRAINING → CLOSED triggers this — `closed_at` is the
///   "normal shutdown" timestamp.
/// - `→ Aborted`: stamps `reason_abort = ?` (caller-supplied,
///   required non-empty; else [`OfflineSessionError::MissingReasonAbort`]).
///   Does NOT stamp `closed_at` — operator pin: ABORTED is an
///   abnormal-exit state distinct from CLOSED; the absence of
///   `closed_at` is itself a structural signal that the session
///   did not complete normally.
/// - All other transitions: state-only UPDATE; no timestamp /
///   reason columns touched.
///
/// Branching in Rust (not SQL) keeps each arm's UPDATE simple and
/// matches the W3 pattern in `fiscal_documents::transition_state`.
pub async fn transition_state(
    tx: &mut WriteTxConn<'_>,
    session_id: OfflineSessionId,
    from: OfflineSessionState,
    to: OfflineSessionState,
    reason_abort: Option<&str>,
) -> Result<TransitionOutcome, OfflineSessionError> {
    if !allowed_transition(from, to) {
        return Ok(TransitionOutcome::Forbidden);
    }
    // W5 helper contract: ABORTED transitions require a non-empty
    // reason.  Enforced in Rust because the W4 schema column is
    // nullable for back-compat with pre-W4 rows.
    if to == OfflineSessionState::Aborted && reason_abort.map(str::is_empty).unwrap_or(true) {
        return Err(OfflineSessionError::MissingReasonAbort);
    }

    let res =
        match to {
            OfflineSessionState::Draining => {
                sqlx::query(
                    "UPDATE offline_sessions \
                 SET state = ?, drained_at = COALESCE(drained_at, CURRENT_TIMESTAMP) \
                 WHERE offline_session_id = ? AND state = ?",
                )
                .bind(to)
                .bind(session_id)
                .bind(from)
                .execute(&mut **tx)
                .await?
            }
            OfflineSessionState::Closed => {
                sqlx::query(
                    "UPDATE offline_sessions \
                 SET state = ?, closed_at = COALESCE(closed_at, CURRENT_TIMESTAMP) \
                 WHERE offline_session_id = ? AND state = ?",
                )
                .bind(to)
                .bind(session_id)
                .bind(from)
                .execute(&mut **tx)
                .await?
            }
            OfflineSessionState::Aborted => {
                // Per operator review pin: ABORTED stamps reason_abort
                // ONLY.  closed_at is reserved for the normal CLOSED
                // exit; absence of closed_at is a structural signal
                // that the session did not complete normally.
                sqlx::query(
                    "UPDATE offline_sessions \
                 SET state = ?, reason_abort = ? \
                 WHERE offline_session_id = ? AND state = ?",
                )
                .bind(to)
                .bind(reason_abort)
                .bind(session_id)
                .bind(from)
                .execute(&mut **tx)
                .await?
            }
            // Opening / Open are arrival states (Open from Opening, Opening from
            // INSERT); they're never the `to` of a transition_state call once
            // OPENING is INSERT-stamped.  But we keep the arm so the helper is
            // total over the enum and a future whitelist extension (e.g.,
            // `Draining → Open` for retried-online) wouldn't silently drop the
            // CAS UPDATE.
            OfflineSessionState::Opening | OfflineSessionState::Open => sqlx::query(
                "UPDATE offline_sessions SET state = ? WHERE offline_session_id = ? AND state = ?",
            )
            .bind(to)
            .bind(session_id)
            .bind(from)
            .execute(&mut **tx)
            .await?,
        };

    if res.rows_affected() == 1 {
        return Ok(TransitionOutcome::Applied);
    }
    // CAS missed — disambiguate row-missing vs state-diverged.
    // Same connection inside the same BEGIN IMMEDIATE tx via the
    // WriteTxConn Deref, so this SELECT cannot interleave with
    // another writer's INSERT/DELETE.
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM offline_sessions WHERE offline_session_id = ? LIMIT 1")
            .bind(session_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(if exists.is_some() {
        TransitionOutcome::Conflict
    } else {
        TransitionOutcome::NotFound
    })
}

/// Admin seam.  Idempotent INSERT OR IGNORE for every
/// `(fiscal_number, code_lnd)` in the inclusive range
/// `[first_lnd ..= last_lnd]`.  Returns the count of rows actually
/// inserted (i.e., the count of codes that were NOT already
/// present).  An empty range (`first_lnd > last_lnd`) is a no-op
/// returning 0.
///
/// Pool-bound (not transactional) because this is the
/// provisioning seam — runs in autocommit, before the runtime
/// write path is engaged, with no other writer to contend with.
/// The single CTE-driven INSERT is itself atomic w.r.t. SQLite
/// statement semantics.
pub async fn seed_code_range(
    pool: &SqlitePool,
    fiscal_number: &str,
    first_lnd: i64,
    last_lnd: i64,
) -> sqlx::Result<u64> {
    if first_lnd > last_lnd {
        return Ok(0);
    }
    // Recursive CTE generates the integer sequence `[first_lnd ..=
    // last_lnd]`, then INSERT OR IGNORE skips rows already present
    // (compound PK `(fiscal_number, code_lnd)` makes the IGNORE
    // deterministic per FN-pair).
    let res = sqlx::query(
        "WITH RECURSIVE seed(code_lnd) AS ( \
             VALUES (?) \
             UNION ALL SELECT code_lnd + 1 FROM seed WHERE code_lnd < ? \
         ) \
         INSERT OR IGNORE INTO offline_codes (fiscal_number, code_lnd) \
         SELECT ?, code_lnd FROM seed",
    )
    .bind(first_lnd)
    .bind(last_lnd)
    .bind(fiscal_number)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Atomically pick the lowest available `code_lnd` for the FN and
/// mark it consumed by `document_id`.
///
/// Single SQL statement (W5 review axis 3: "no SELECT-then-UPDATE").
/// The UPDATE's WHERE clause filters on `consumed_at IS NULL` AND
/// matches the lowest available `code_lnd` via a correlated
/// subquery; the `RETURNING` clause echoes the chosen `(code_lnd,
/// consumed_at)` pair.  If zero rows are matched (pool exhausted
/// for the FN), the result is an empty `RETURNING` and we return
/// [`OfflineSessionError::CodePoolExhausted`].
///
/// Concurrency: every caller runs inside `with_immediate` (BEGIN
/// IMMEDIATE), so SQLite WAL serialises writers via the RESERVED
/// lock.  Two `tokio::task`s on separate pool connections each
/// invoking this helper for the same FN will serialise at the tx
/// level — the first one acquires + commits, the second one starts
/// fresh and naturally picks the next available row (or sees an
/// empty pool and returns `CodePoolExhausted`).
///
/// Trigger / index error mapping:
///
/// - W4 trigger `offline_codes_consumed_immutable` would fire if a
///   caller passed a `document_id` that's somehow updating an
///   already-consumed row.  The single-statement UPDATE's WHERE
///   filter `consumed_at IS NULL` makes that case impossible by
///   construction — the row's `consumed_at` IS NULL pre-UPDATE,
///   so trigger's `WHEN OLD.consumed_at IS NOT NULL` is false.
///   The mapping below is defence-in-depth.
/// - W4 partial UNIQUE `ux_offline_codes_consumed_by_doc` would
///   fire if two distinct code rows tried to link the same
///   `consumed_by_document_id`.  This CAN happen under raw-SQL
///   misuse (not via this helper) — but if it ever surfaces here,
///   the mapping flags it as `OfflineCodeAlreadyConsumed` so the
///   caller can react without string-parsing sqlx error text.
pub async fn acquire_code_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    document_id: DocumentId,
) -> Result<AcquiredCode, OfflineSessionError> {
    let row: Result<Option<(i64, String)>, sqlx::Error> = sqlx::query_as(
        "UPDATE offline_codes \
         SET consumed_at = CURRENT_TIMESTAMP, consumed_by_document_id = ? \
         WHERE fiscal_number = ? \
           AND consumed_at IS NULL \
           AND code_lnd = ( \
               SELECT code_lnd FROM offline_codes \
               WHERE fiscal_number = ? AND consumed_at IS NULL \
               ORDER BY code_lnd ASC LIMIT 1 \
           ) \
         RETURNING code_lnd, consumed_at",
    )
    .bind(document_id)
    .bind(fiscal_number)
    .bind(fiscal_number)
    .fetch_optional(&mut **tx)
    .await;

    match row {
        Ok(Some((code_lnd, consumed_at))) => Ok(AcquiredCode {
            code_lnd,
            consumed_at,
        }),
        Ok(None) => Err(OfflineSessionError::CodePoolExhausted {
            fiscal_number: fiscal_number.to_string(),
        }),
        Err(e) => Err(classify_acquire_error(e)),
    }
}

/// All `fiscal_documents` rows tied to this session that are still
/// in flight — i.e., NOT terminal (ACK / REJECTED / CANCELLED /
/// REQUIRES_MANUAL_RECONCILIATION) and NOT the post-final
/// `OFFLINE_LOCAL_ACK` (already locally durable, awaiting drain).
///
/// Ordered by `lnd ASC` so the drain stage can replay in fiscal
/// chain order.  This is the input set for W7's `stage_offline_ack`
/// drain loop and W8's return-online sync.
///
/// Pool-bound (read only).
pub async fn list_pending_for_session(
    pool: &SqlitePool,
    session_id: OfflineSessionId,
) -> sqlx::Result<Vec<DocumentId>> {
    sqlx::query_scalar::<_, DocumentId>(
        "SELECT document_id FROM fiscal_documents \
         WHERE offline_session_id = ? \
           AND state IN ( \
               'PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2', \
               'ERROR_RETRYABLE','OFFLINE_LOCAL_ACK' \
           ) \
         ORDER BY lnd ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
}

// ─── Error classification helpers ─────────────────────────────────────

/// Map sqlx errors from [`insert_opening`] into typed variants.
///
/// Only the `ux_offline_active` partial UNIQUE violation is
/// recognised; everything else propagates as `Database`.  SQLite
/// surfaces partial-UNIQUE violations as
/// `UNIQUE constraint failed: offline_sessions.fiscal_number`
/// (the index column name in the message).  Match on substring;
/// the full message also includes the constraint kind string.
fn classify_insert_opening_error(e: sqlx::Error, fiscal_number: &str) -> OfflineSessionError {
    if let sqlx::Error::Database(ref db_err) = e {
        let msg = db_err.message();
        if msg.contains("UNIQUE constraint failed")
            && msg.contains("offline_sessions.fiscal_number")
        {
            return OfflineSessionError::AnotherSessionActive {
                fiscal_number: fiscal_number.to_string(),
            };
        }
    }
    OfflineSessionError::Database(e)
}

/// Map sqlx errors from [`acquire_code_tx`] into typed variants.
///
/// Two distinct W4 schema features can fire:
///   - trigger `offline_codes_consumed_immutable` → message
///     contains `consumed row is immutable` (RAISE text).
///   - partial UNIQUE `ux_offline_codes_consumed_by_doc` → message
///     contains `UNIQUE constraint failed:
///     offline_codes.consumed_by_document_id`.
fn classify_acquire_error(e: sqlx::Error) -> OfflineSessionError {
    if let sqlx::Error::Database(ref db_err) = e {
        let msg = db_err.message();
        if msg.contains("consumed row is immutable") {
            return OfflineSessionError::OfflineCodeAlreadyConsumed;
        }
        if msg.contains("UNIQUE constraint failed")
            && msg.contains("offline_codes.consumed_by_document_id")
        {
            return OfflineSessionError::OfflineCodeAlreadyConsumed;
        }
    }
    OfflineSessionError::Database(e)
}
