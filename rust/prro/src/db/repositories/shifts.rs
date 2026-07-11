//! Repository for `shifts`.
//!
//! Repo policy:
//! - SELECT decode goes through `sqlx::query!` (compile-time schema verification).
//! - INSERT and UPDATE use runtime-bound `sqlx::query()`; param types are already
//!   typed via `Encode<Sqlite>` on `ShiftId` / `ShiftState`.
//! - State transitions go through CAS UPDATE + a code-level `allowed_transition`
//!   whitelist.  CAS ensures concurrent transitions cannot race; the whitelist
//!   short-circuits forbidden moves before touching the DB.

use crate::db::models::{
    enums::{Severity, ShiftState},
    ids::{CashierId, DocumentId, ShiftId},
};
use crate::db::repositories::audit_log;
use crate::db::tx::WriteTxConn;
use crate::services::write_path::types::hex_encode_lower as hex_lower;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct ShiftRow {
    pub shift_id: ShiftId,
    pub fiscal_number: String,
    pub serial: Option<i64>,
    pub state: ShiftState,
    pub cash_balance_kop: i64,
    /// W14a-2b §2.2 — cashier identity from `shifts.opened_by_cashier_id`.
    /// PR #66 L3 was deferred; W14a-2b makes it in-scope so
    /// `signer_guard::enforce_signer_cashier_match` (Commit 3) can compare
    /// against this value.  Always `Some` post-W14a-1 (column is NOT NULL
    /// in 016_shift_state_expansion.sql via sentinel back-fill).
    pub opened_by_cashier_id: CashierId,
}

/// M3b W14a-2a — whitelist of allowed shift state transitions per spec
/// `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` §4.1.
///
/// Drift-guard: edge count is locked at **14**.  Any addition / removal
/// MUST update both this function body AND the matching scanner test
/// `tests/shift_state_whitelist_matrix.rs` (9×9 = 81 (from, to) pairs:
/// 14 Applied + 67 Forbidden).
///
/// Per spec §4.4 forbidden patterns:
/// - **`Error` is reachable via `force_to_error_with_audit` seam (operator-driven)
///   OR `boot_phase` branch e2 system-context recovery (audit shape
///   `SHIFT_BOOT_ORPHAN_ERROR`, distinct from `SHIFT_FORCE_TO_ERROR`).**  No
///   whitelist edge ever lands on `Error`.
/// - **`RequiresManualReconciliation` is reachable via edges 4 / 6 / 12 / 14 / 15
///   OR via `force_to_manual_reconciliation_with_audit` seam.**  Nothing
///   else.  (Edge 15 — `Opened → RequiresManualReconciliation` — added M2-N2a
///   for the strict-sequential drain-reject of an offline-origin predecessor on
///   a plain `Opened` shift; same class as 6 / 14.)
///
/// The 14 edges (#N matches spec §4.1 numbering):
///   1.  Created → Opening                      (online SHIFT_OPEN ingress)
///   2.  Created → OpenedLocalPendingDrain      (offline SHIFT_OPEN ingress, Pattern C)
///   3.  Opening → Opened                       (online send → DPS Ack)
///   4.  Opening → RequiresManualReconciliation (hard reject / operator give-up)
///   5.  OpenedLocalPendingDrain → Opened       (drain SHIFT_OPEN Ack + empty backlog)
///   6.  OpenedLocalPendingDrain → RequiresManualReconciliation (drain reject)
///   7.  OpenedLocalPendingDrain → ClosingLocalPendingDrain     (offline Z_REPORT before drain)
///   8.  Opened → Closing                       (online Z_REPORT / SHIFT_CLOSE ingress)
///   9.  Opened → ClosingLocalPendingDrain      (offline Z_REPORT, Pattern C)
///   10. Closing → Closed                       (online send → DPS Ack)
///   11. Closing → Opened                       (`Authorization::DocumentReject` only per §6.2)
///   12. Closing → RequiresManualReconciliation (hard reject)
///   13. ClosingLocalPendingDrain → Closed      (drain reached final Ack on all backlog)
///   14. ClosingLocalPendingDrain → RequiresManualReconciliation (drain reject)
///   15. Opened → RequiresManualReconciliation  (M2-N2a: strict-sequential drain
///       reject of an offline-origin predecessor on an online-opened shift that
///       went OFFLINE mid-shift — a normal state-machine branch, NOT an operator
///       override.  Same drain-reject class as 6 / 14, from a plain `Opened` shift.)
pub fn allowed_transition(from: ShiftState, to: ShiftState) -> bool {
    use ShiftState::*;
    matches!(
        (from, to),
        (Created, Opening)                                  // 1
            | (Created, OpenedLocalPendingDrain)            // 2
            | (Opening, Opened)                             // 3
            | (Opening, RequiresManualReconciliation)       // 4
            | (OpenedLocalPendingDrain, Opened)             // 5
            | (OpenedLocalPendingDrain, RequiresManualReconciliation)   // 6
            | (OpenedLocalPendingDrain, ClosingLocalPendingDrain)       // 7
            | (Opened, Closing)                             // 8
            | (Opened, ClosingLocalPendingDrain)            // 9
            | (Closing, Closed)                             // 10
            | (Closing, Opened)                             // 11
            | (Closing, RequiresManualReconciliation)       // 12
            | (ClosingLocalPendingDrain, Closed)            // 13
            | (ClosingLocalPendingDrain, RequiresManualReconciliation) // 14
            | (Opened, RequiresManualReconciliation) // 15 (M2-N2a)
    )
}

/// M3b W14a-2a — typed outcome of `transition_state` CAS attempt.
///
/// Replaces the M3a `bool` return of `transition` with a 4-way
/// distinction so callers can react precisely without re-reading the DB
/// on the cold path.  Per spec §11 acceptance #3.  Variant naming
/// mirrors [`crate::db::repositories::fiscal_documents::TransitionOutcome`]
/// (Applied / Forbidden / Conflict / NotFound) for cross-repository
/// consistency (PR #66 R2 MED-1).  This enum extends the convention
/// with metadata in `Forbidden` + `Conflict` for richer caller
/// diagnostics:
/// - `Applied` — exactly one row UPDATEd; the transition is durable.
/// - `Forbidden { from, to }` — the pair is not in the §4.1 whitelist;
///   no DB write attempted, no audit emitted (caller's responsibility to
///   surface as bug or recoverable state machine drift).
/// - `Conflict { observed }` — whitelist passed but the row's current
///   state differs from `from` (concurrent writer or stale snapshot).
///   Caller decides retry / refuse.  Matches `fiscal_documents::Conflict`
///   semantics.
/// - `NotFound` — whitelist passed and CAS UPDATE matched 0 rows AND
///   the row no longer exists at all (caller bug: stale `ShiftId` OR
///   shift deleted by an orthogonal admin path).  Matches
///   `fiscal_documents::NotFound`.  Symmetric with `load_state_tx` /
///   `force_to_*` `ShiftNotFound` handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionOutcome {
    Applied,
    Forbidden { from: ShiftState, to: ShiftState },
    Conflict { observed: ShiftState },
    NotFound,
}

pub async fn insert_created(
    pool: &SqlitePool,
    id: ShiftId,
    fiscal_number: &str,
    open_mode: &str,
    opened_by_cashier_id: &str,
    opening_cash_kop: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) \
         VALUES (?, ?, 'CREATED', ?, ?, ?)",
    )
    .bind(id)
    .bind(fiscal_number)
    .bind(open_mode)
    .bind(opening_cash_kop)
    .bind(opened_by_cashier_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Tx-bound sibling of [`insert_created`] — A′.1 piece 0b.  Same INSERT,
/// but enrolled in the caller's `with_immediate` envelope so the shift-row
/// create commits (or rolls back) atomically with the `create_shift_tx`
/// node_state projection CAS.
///
/// `opening_cash_kop` — the carry-over opening balance for this shift
/// (prior shift's closing cash in `shifts.cash_balance_kop`; 0 for the
/// FN's first shift).  L0 carry semantics: persisted in `cash_balance_kop`
/// at create-time, re-derivable from the journal via `invariant_scan`.
pub async fn insert_created_tx(
    tx: &mut WriteTxConn<'_>,
    id: ShiftId,
    fiscal_number: &str,
    open_mode: &str,
    opened_by_cashier_id: &str,
    opening_cash_kop: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) \
         VALUES (?, ?, 'CREATED', ?, ?, ?)",
    )
    .bind(id)
    .bind(fiscal_number)
    .bind(open_mode)
    .bind(opening_cash_kop)
    .bind(opened_by_cashier_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// L0 — update `cash_balance_kop` for a shift row.  Called at shift CLOSE
/// (inside the same `with_immediate` envelope as the `Closing → Closed`
/// transition) to persist the closing cash as the next shift's carry anchor.
/// Pure SQL; no crypto/network (invariant #1 preserved).
pub async fn update_cash_balance_tx(
    tx: &mut WriteTxConn<'_>,
    id: ShiftId,
    cash_balance_kop: i64,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE shifts SET cash_balance_kop = ? WHERE shift_id = ?")
        .bind(cash_balance_kop)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, id: ShiftId) -> sqlx::Result<Option<ShiftRow>> {
    let row = sqlx::query!(
        r#"SELECT shift_id      as "shift_id: ShiftId",
                  fiscal_number,
                  serial,
                  state          as "state: ShiftState",
                  cash_balance_kop,
                  opened_by_cashier_id as "opened_by_cashier_id: CashierId"
           FROM shifts WHERE shift_id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ShiftRow {
        shift_id: r.shift_id,
        fiscal_number: r.fiscal_number,
        serial: r.serial,
        state: r.state,
        cash_balance_kop: r.cash_balance_kop,
        opened_by_cashier_id: r.opened_by_cashier_id,
    }))
}

/// SEAM-D-1 (functional seam-pass, 2026-06-12) — the FN's ACTIVE
/// (non-terminal) shift, if any: a `shifts` row whose `state` is NOT in
/// the terminal set {`CLOSED`, `REQUIRES_MANUAL_RECONCILIATION`, `ERROR`}.
/// Returns `(shift_id_bytes, state_string)` for the most-recent such row
/// (`serial DESC`); `None` when the FN has no active shift.  Pool-bound
/// read, no write-tx.
///
/// Used by boot branch (a) NC-03 recovery (`reconciliation::boot_phase`):
/// when `node_state` was lost but the `fiscal_documents` ledger survived,
/// a surviving OPEN `shifts` row would otherwise be MASKED from boot
/// shift-recovery (which keys on `node_state.shift_state == Opened`).  The
/// recovery surfaces the active-shift id+state in its CRITICAL audit so the
/// operator reconciles it before clearing the block — it does NOT
/// auto-resume the shift (a node whose node_state vanished is operator-led,
/// not auto-recovered).  Returns raw id bytes + state string because the
/// sole caller only needs them for the audit payload (hex id + label).
pub async fn active_shift_for_fn(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<(Vec<u8>, String)>> {
    sqlx::query_as::<_, (Vec<u8>, String)>(
        "SELECT shift_id, state FROM shifts \
         WHERE fiscal_number = ? \
           AND state NOT IN ('CLOSED', 'REQUIRES_MANUAL_RECONCILIATION', 'ERROR') \
         ORDER BY serial DESC \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await
}

/// W5 — same as [`get`] but takes `&mut WriteTxConn<'_>` so stage 1
/// reads the shift row inside its `with_immediate` envelope alongside
/// `node_state.current_shift_id`.  No CAS, no UPDATE — strictly read.
pub async fn get_tx(tx: &mut WriteTxConn<'_>, id: ShiftId) -> sqlx::Result<Option<ShiftRow>> {
    let row = sqlx::query!(
        r#"SELECT shift_id      as "shift_id: ShiftId",
                  fiscal_number,
                  serial,
                  state          as "state: ShiftState",
                  cash_balance_kop,
                  opened_by_cashier_id as "opened_by_cashier_id: CashierId"
           FROM shifts WHERE shift_id = ?"#,
        id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| ShiftRow {
        shift_id: r.shift_id,
        fiscal_number: r.fiscal_number,
        serial: r.serial,
        state: r.state,
        cash_balance_kop: r.cash_balance_kop,
        opened_by_cashier_id: r.opened_by_cashier_id,
    }))
}

/// Atomic CAS state transition (M3a — bool return).  Returns true if
/// exactly one row changed (transition succeeded), false otherwise.
/// Caller decides what to do on `false` (typically: load current state
/// and decide whether to retry or give up).
///
/// Preserved for backward-compat with M3a callers (test harness).
/// M3b W14a-2a callers SHOULD prefer [`transition_state`] for the typed
/// `TransitionOutcome` distinction between Forbidden / Conflict.
///
/// The `allowed_transition` whitelist is enforced in code (cheap)
/// before hitting the DB.
///
/// Per ADR-M3-A4 / W0-2 §4.4 (M3a W2), takes `&mut WriteTxConn<'_>` —
/// callers obtain it from a `with_immediate` closure, mirroring the
/// `fiscal_documents::transition_state` discipline.
pub async fn transition(
    tx: &mut WriteTxConn<'_>,
    id: ShiftId,
    from: ShiftState,
    to: ShiftState,
) -> sqlx::Result<bool> {
    if !allowed_transition(from, to) {
        return Ok(false);
    }
    let res = sqlx::query("UPDATE shifts SET state = ? WHERE shift_id = ? AND state = ?")
        .bind(to)
        .bind(id)
        .bind(from)
        .execute(&mut **tx)
        .await?;
    Ok(res.rows_affected() == 1)
}

/// M3b W14a-2a — typed-outcome CAS state transition.
///
/// Same semantics as [`transition`] but returns [`TransitionOutcome`]
/// for precise caller reaction:
/// - `Applied` — exactly one row UPDATEd; the transition is durable.
/// - `Forbidden { from, to }` — the pair is not in the [`allowed_transition`]
///   whitelist; **no DB write attempted, no audit emitted**.  Caller
///   should surface as bug / structural drift (not a normal recovery
///   path; per spec §4.4 forbidden patterns).
/// - `CASMiss { observed }` — whitelist passed but the row's current
///   state differs from `from` (concurrent writer or stale snapshot).
///   Caller decides retry / refuse based on `observed`.
///
/// `with_immediate` envelope discipline preserved per W2 ReconcileGuard:
/// caller obtains `&mut WriteTxConn<'_>` from `with_immediate` closure.
pub async fn transition_state(
    tx: &mut WriteTxConn<'_>,
    id: ShiftId,
    from: ShiftState,
    to: ShiftState,
) -> sqlx::Result<TransitionOutcome> {
    if !allowed_transition(from, to) {
        return Ok(TransitionOutcome::Forbidden { from, to });
    }
    let res = sqlx::query("UPDATE shifts SET state = ? WHERE shift_id = ? AND state = ?")
        .bind(to)
        .bind(id)
        .bind(from)
        .execute(&mut **tx)
        .await?;
    if res.rows_affected() == 1 {
        return Ok(TransitionOutcome::Applied);
    }
    // CAS miss: re-read current state for the caller's diagnostics.
    // Use the same tx so the read is serialised with our failed UPDATE.
    // PR #66 R1 M1: `fetch_optional` (not `fetch_one`) — symmetric with
    // `load_state_tx` / force-seam helpers below.  `None` → caller's
    // `ShiftId` is stale OR the row was deleted by an admin path;
    // surface as `NotFound` (PR #66 R2 MED-1: matches
    // `fiscal_documents::TransitionOutcome::NotFound` naming) so callers
    // don't have to map opaque `sqlx::Error::RowNotFound`.
    let observed: Option<ShiftState> =
        sqlx::query_scalar(r#"SELECT state as "state: ShiftState" FROM shifts WHERE shift_id = ?"#)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    match observed {
        Some(state) => Ok(TransitionOutcome::Conflict { observed: state }),
        None => Ok(TransitionOutcome::NotFound),
    }
}

// ─── M3b W14a-2a: force seams + source-state guard + audit ────────────
//
// Per spec §4.5 ("Force-error / manual seam") + §16.7 (real Manual recon
// trigger surface) + §8 audit vocabulary.  Two distinct methods (not one
// with `target: ShiftState` parameter) so grep `force_to_error_with_audit`
// vs `force_to_manual_reconciliation_with_audit` cleanly distinguishes
// intent in audit / forensic queries.
//
// Source-state restriction per Round 6 A-H1:
// - `force_to_error_with_audit` allowed from: Opening, OpenedLocalPending
//   Drain, Opened, Closing, ClosingLocalPendingDrain, RequiresManual
//   Reconciliation.  Forbidden from: Created, Closed, Error.
// - `force_to_manual_reconciliation_with_audit` allowed from: Opening,
//   OpenedLocalPendingDrain, Opened, Closing, ClosingLocalPendingDrain.
//   Forbidden from: Created, Closed, Error, RequiresManualReconciliation.

/// Maximum byte length of evidence_json payload for force seams.  Sized
/// generously for forensic completeness while preventing accidental
/// large-blob storage in audit_log.  Spec §8: secrets-forbidden by
/// convention; size cap defends against attack-shaped evidence floods.
pub const FORCE_SEAM_EVIDENCE_MAX_BYTES: usize = 8 * 1024;

/// W14a-2a force-seam invocation outcome.  Returned inside `Ok(_)` so
/// the surrounding `with_immediate` envelope COMMITS even on
/// `ForbiddenSource` / `TxIsolationViolation` — preserving the audit
/// row per spec §8 forensic-traceability requirement (Round 7 §8.1
/// load-bearing pin).  Caller pattern-matches on this enum to know
/// whether the state actually changed.
///
/// PR #66 R4 L-R4-2: `#[non_exhaustive]` — future seam outcome
/// variants (e.g. partial-commit edge cases) MUST NOT break callers
/// in the absence of a wildcard arm.  Downstream consumers must
/// pattern-match with `_ =>` to remain forward-compatible.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForceSeamOutcome {
    /// State transitioned to the seam's target; Critical audit emitted.
    Applied,
    /// Current state was in the forbidden-sources list per spec §4.5;
    /// state UNCHANGED; `SHIFT_FORCE_SEAM_REFUSED` Warning audit emitted
    /// with full evidence_json envelope (forensic record of the attempt).
    ForbiddenSource {
        current_state: ShiftState,
        attempted_seam: &'static str,
    },
    /// PR #66 R3 H-R2-1: CAS-WHERE-state UPDATE hit `rows_affected != 1`
    /// despite the state having been read inside the same tx.  This
    /// indicates either: (a) `BEGIN IMMEDIATE` isolation broke (SQLite /
    /// kernel bug), (b) the seam was invoked outside a `with_immediate`
    /// envelope (caller bug — future refactor), OR (c) the row vanished
    /// between read and UPDATE (orthogonal admin path).  Replaces R2
    /// MED-3 panic — failure mode is now graceful per established
    /// `fiscal_documents.rs:351` HIGH-3 convention ("Was `debug_assert!`
    /// previously").  State UNCHANGED; `SHIFT_FORCE_SEAM_TX_ISOLATION_
    /// VIOLATION` Critical audit emitted with `observed_state_at_update`
    /// + `rows_affected` for forensic forensics.
    TxIsolationViolation {
        observed_state_at_update: Option<ShiftState>,
        attempted_seam: &'static str,
        rows_affected: u64,
    },
}

/// W14a-2a force-seam pre-flight validation error.  Reserved for caller
/// bugs (malformed evidence_json, stale shift_id) + DB-side failures.
/// `ForbiddenSource` is NOT here — it's a legitimate outcome (see
/// [`ForceSeamOutcome::ForbiddenSource`]) because the tx must commit the
/// refused-audit row.
#[derive(Debug, thiserror::Error)]
pub enum ForceSeamError {
    /// `evidence_json` is not valid JSON OR exceeds [`FORCE_SEAM_EVIDENCE_MAX_BYTES`].
    /// No transition committed; no audit emitted (caller bug; not an operator
    /// action worth recording).
    #[error("evidence_json invalid: {0}")]
    InvalidEvidenceJson(String),

    /// Shift row not found for the given id.  Caller bug (stale id) — no
    /// transition committed.
    #[error("shift {shift_id_hex} not found")]
    ShiftNotFound { shift_id_hex: String },

    /// Wrapped sqlx error (DB-side failure).
    #[error("sqlx error: {0}")]
    Db(#[from] sqlx::Error),
}

const FORCE_TO_ERROR_SEAM_NAME: &str = "force_to_error_with_audit";
const FORCE_TO_MANUAL_SEAM_NAME: &str = "force_to_manual_reconciliation_with_audit";

fn force_to_error_allows_source(state: ShiftState) -> bool {
    use ShiftState::*;
    matches!(
        state,
        Opening
            | OpenedLocalPendingDrain
            | Opened
            | Closing
            | ClosingLocalPendingDrain
            | RequiresManualReconciliation
    )
}

fn force_to_manual_allows_source(state: ShiftState) -> bool {
    use ShiftState::*;
    matches!(
        state,
        Opening | OpenedLocalPendingDrain | Opened | Closing | ClosingLocalPendingDrain
    )
}

fn validate_evidence_json(s: &str) -> Result<(), ForceSeamError> {
    if s.len() > FORCE_SEAM_EVIDENCE_MAX_BYTES {
        return Err(ForceSeamError::InvalidEvidenceJson(format!(
            "evidence_json {} bytes exceeds cap {}",
            s.len(),
            FORCE_SEAM_EVIDENCE_MAX_BYTES
        )));
    }
    serde_json::from_str::<serde_json::Value>(s)
        .map(|_| ())
        .map_err(|e| ForceSeamError::InvalidEvidenceJson(format!("not valid JSON: {e}")))
}

// PR #66 R2 LOW-1: `load_state_tx` removed — both force seams now load
// state + fiscal_number in a single `query_as` (matches senior_close
// pattern).  Symmetry retained at the SQL layer.

/// Internal: build the refused-seam audit payload combining metadata +
/// the full evidence_json the caller provided per spec §8 (forensic
/// traceability of refused force-seam invocations).
fn build_refused_audit_payload(
    fiscal_number: &str,
    shift_id_hex: &str,
    current_state: ShiftState,
    attempted_seam: &str,
    evidence_json: &str,
) -> String {
    // We embed the raw evidence_json as a string value (NOT a parsed
    // sub-object) so the audit row remains a single self-contained JSON
    // document even if the evidence was invalid JSON.  Wrap defensively
    // via serde_json::Value to escape quotes/backslashes correctly.
    let envelope = serde_json::json!({
        "fiscal_number": fiscal_number,
        "shift_id": shift_id_hex,
        "current_state": current_state.as_str(),
        "attempted_seam": attempted_seam,
        "evidence_json": evidence_json,
    });
    envelope.to_string()
}

/// Operator-driven force-transition into [`ShiftState::Error`].  Bypasses
/// the whitelist intentionally; the seam IS the only **operator-driven**
/// entry to `Error` per spec §4.5.  System-context boot recovery
/// (`boot_phase` branch e2, audit shape `SHIFT_BOOT_ORPHAN_ERROR`) is a
/// distinct second surface per spec §4.4.  Requires explicit
/// `evidence_json` (parse-validated as JSON, size-capped at
/// [`FORCE_SEAM_EVIDENCE_MAX_BYTES`]) per spec §8.
///
/// On success: UPDATEs state (CAS-guarded on `WHERE shift_id = ? AND
/// state = ?`); on `rows_affected == 1` emits `SHIFT_FORCE_TO_ERROR`
/// Critical audit AFTER, returns `Ok(ForceSeamOutcome::Applied)`.  On
/// `rows_affected != 1` emits `SHIFT_FORCE_SEAM_TX_ISOLATION_VIOLATION`
/// Critical audit + returns `Ok(ForceSeamOutcome::TxIsolationViolation
/// { .. })` per PR #66 R6 MED audit-ordering fix (success audit must
/// NEVER precede the guarded UPDATE).
///
/// On forbidden source (per spec §4.5): emits `SHIFT_FORCE_SEAM_REFUSED`
/// Warning audit with full evidence_json metadata (forensic) then returns
/// `Ok(ForceSeamOutcome::ForbiddenSource { .. })`.  No state change.
pub async fn force_to_error_with_audit(
    tx: &mut WriteTxConn<'_>,
    shift_id: ShiftId,
    actor_id: Option<&str>,
    evidence_json: &str,
) -> Result<ForceSeamOutcome, ForceSeamError> {
    validate_evidence_json(evidence_json)?;
    // PR #66 R2 LOW-1: collapse state+fiscal_number to single round-trip
    // (matches senior_close pattern at line 655).
    // PR #66 R3 LOW-R3-1: renamed local from shadowing parameter `actor_id`
    // to `normalized_actor` for readability.
    let normalized_actor = actor_id.filter(|s| !s.is_empty()); // R2 LOW-4: drop Some("")
    let row: Option<(ShiftState, String)> = sqlx::query_as(
        r#"SELECT state as "state: ShiftState", fiscal_number FROM shifts WHERE shift_id = ?"#,
    )
    .bind(shift_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (current_state, fiscal_number) = row.ok_or_else(|| ForceSeamError::ShiftNotFound {
        shift_id_hex: hex_lower(shift_id.as_bytes()),
    })?;
    let shift_id_hex = hex_lower(shift_id.as_bytes());

    if !force_to_error_allows_source(current_state) {
        let payload = build_refused_audit_payload(
            &fiscal_number,
            &shift_id_hex,
            current_state,
            FORCE_TO_ERROR_SEAM_NAME,
            evidence_json,
        );
        // PR #66 R1 M2 + R2 LOW-4: actor_id populated on `audit_log.actor`
        // column with empty-string filter (was: None; forensic-attribution
        // gap per reviewer M2 + accidental Some("") sentinel pollution).
        audit_log::append_tx(
            tx,
            "shift",
            &shift_id_hex,
            "SHIFT_FORCE_SEAM_REFUSED",
            Severity::Warning,
            normalized_actor,
            Some(&payload),
        )
        .await?;
        // Forensic audit committed via Ok return — spec §8 + Round 7 §8.1.
        return Ok(ForceSeamOutcome::ForbiddenSource {
            current_state,
            attempted_seam: FORCE_TO_ERROR_SEAM_NAME,
        });
    }

    // Allowed source — attempt CAS UPDATE FIRST; emit success audit
    // only after rows_affected == 1.  PR #66 R6 MED fix: previously
    // success audit was emitted BEFORE the UPDATE; if rows_affected
    // != 1 fired the violation path, audit log carried BOTH
    // SHIFT_FORCE_TO_ERROR success AND SHIFT_FORCE_SEAM_TX_ISOLATION_
    // VIOLATION on the same shift — false-success forensic record.
    //
    // PR #66 R2 MED-3 + R3 H-R2-1: CAS-WHERE-state guard with graceful
    // failure path (replaces R2's `assert_eq!` panic) per established
    // `fiscal_documents.rs:351` HIGH-3 convention.
    let res = sqlx::query("UPDATE shifts SET state = ? WHERE shift_id = ? AND state = ?")
        .bind(ShiftState::Error)
        .bind(shift_id)
        .bind(current_state)
        .execute(&mut **tx)
        .await?;
    if res.rows_affected() != 1 {
        let rows_affected = res.rows_affected();
        let observed = emit_cas_isolation_violation_audit(
            tx,
            "SHIFT_FORCE_SEAM_TX_ISOLATION_VIOLATION",
            FORCE_TO_ERROR_SEAM_NAME,
            CasFailureContext {
                shift_id,
                shift_id_hex: &shift_id_hex,
                fiscal_number: &fiscal_number,
                current_state_read_was: current_state,
                actor: normalized_actor,
                rows_affected,
            },
            None,
        )
        .await?;
        return Ok(ForceSeamOutcome::TxIsolationViolation {
            observed_state_at_update: observed,
            attempted_seam: FORCE_TO_ERROR_SEAM_NAME,
            rows_affected,
        });
    }
    // Success path — UPDATE applied; emit canonical Critical audit.
    let success_payload = serde_json::json!({
        "fiscal_number": fiscal_number,
        "shift_id": shift_id_hex,
        "from_state": current_state.as_str(),
        "to_state": ShiftState::Error.as_str(),
        "evidence_json": evidence_json,
    })
    .to_string();
    audit_log::append_tx(
        tx,
        "shift",
        &shift_id_hex,
        "SHIFT_FORCE_TO_ERROR",
        Severity::Critical,
        normalized_actor,
        Some(&success_payload),
    )
    .await?;
    Ok(ForceSeamOutcome::Applied)
}

/// Operator-driven force-transition into [`ShiftState::RequiresManualReconciliation`].
/// Allowed alongside whitelist edges 4 / 6 / 12 / 14 per spec §4.5; the
/// seam is the operator-initiated escalation surface when normal recovery
/// flow doesn't fit.
///
/// On success: UPDATEs state (CAS-guarded on `WHERE shift_id = ? AND
/// state = ?`); on `rows_affected == 1` emits
/// `SHIFT_FORCE_TO_MANUAL_RECONCILIATION` Critical audit AFTER, returns
/// `Ok(ForceSeamOutcome::Applied)`.  On `rows_affected != 1` emits
/// `SHIFT_FORCE_SEAM_TX_ISOLATION_VIOLATION` Critical audit + returns
/// `Ok(ForceSeamOutcome::TxIsolationViolation { .. })` per PR #66 R6
/// MED audit-ordering fix (success audit must NEVER precede the
/// guarded UPDATE).
///
/// On forbidden source: emits `SHIFT_FORCE_SEAM_REFUSED` Warning audit
/// with full evidence_json envelope + returns
/// `Ok(ForceSeamOutcome::ForbiddenSource { .. })`.  The Ok return ensures
/// the surrounding `with_immediate` tx commits — preserving the refused-
/// audit row per spec §8 + Round 7 §8.1 forensic-traceability invariant.
pub async fn force_to_manual_reconciliation_with_audit(
    tx: &mut WriteTxConn<'_>,
    shift_id: ShiftId,
    actor_id: Option<&str>,
    evidence_json: &str,
) -> Result<ForceSeamOutcome, ForceSeamError> {
    validate_evidence_json(evidence_json)?;
    // PR #66 R2 LOW-1 + LOW-4 + R3 LOW-R3-1: single round-trip + actor_id
    // empty filter + rename to avoid parameter shadowing.
    let normalized_actor = actor_id.filter(|s| !s.is_empty());
    let row: Option<(ShiftState, String)> = sqlx::query_as(
        r#"SELECT state as "state: ShiftState", fiscal_number FROM shifts WHERE shift_id = ?"#,
    )
    .bind(shift_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (current_state, fiscal_number) = row.ok_or_else(|| ForceSeamError::ShiftNotFound {
        shift_id_hex: hex_lower(shift_id.as_bytes()),
    })?;
    let shift_id_hex = hex_lower(shift_id.as_bytes());

    if !force_to_manual_allows_source(current_state) {
        let payload = build_refused_audit_payload(
            &fiscal_number,
            &shift_id_hex,
            current_state,
            FORCE_TO_MANUAL_SEAM_NAME,
            evidence_json,
        );
        // PR #66 R1 M2 + R2 LOW-4: actor_id forensic attribution + empty filter.
        audit_log::append_tx(
            tx,
            "shift",
            &shift_id_hex,
            "SHIFT_FORCE_SEAM_REFUSED",
            Severity::Warning,
            normalized_actor,
            Some(&payload),
        )
        .await?;
        return Ok(ForceSeamOutcome::ForbiddenSource {
            current_state,
            attempted_seam: FORCE_TO_MANUAL_SEAM_NAME,
        });
    }

    // PR #66 R6 MED + R2 MED-3 + R3 H-R2-1: CAS-WHERE-state guard FIRST,
    // success audit AFTER (avoids false-success audit on isolation
    // violation path — see force_to_error_with_audit for rationale).
    let res = sqlx::query("UPDATE shifts SET state = ? WHERE shift_id = ? AND state = ?")
        .bind(ShiftState::RequiresManualReconciliation)
        .bind(shift_id)
        .bind(current_state)
        .execute(&mut **tx)
        .await?;
    if res.rows_affected() != 1 {
        let rows_affected = res.rows_affected();
        let observed = emit_cas_isolation_violation_audit(
            tx,
            "SHIFT_FORCE_SEAM_TX_ISOLATION_VIOLATION",
            FORCE_TO_MANUAL_SEAM_NAME,
            CasFailureContext {
                shift_id,
                shift_id_hex: &shift_id_hex,
                fiscal_number: &fiscal_number,
                current_state_read_was: current_state,
                actor: normalized_actor,
                rows_affected,
            },
            None,
        )
        .await?;
        return Ok(ForceSeamOutcome::TxIsolationViolation {
            observed_state_at_update: observed,
            attempted_seam: FORCE_TO_MANUAL_SEAM_NAME,
            rows_affected,
        });
    }
    let success_payload = serde_json::json!({
        "fiscal_number": fiscal_number,
        "shift_id": shift_id_hex,
        "from_state": current_state.as_str(),
        "to_state": ShiftState::RequiresManualReconciliation.as_str(),
        "evidence_json": evidence_json,
    })
    .to_string();
    audit_log::append_tx(
        tx,
        "shift",
        &shift_id_hex,
        "SHIFT_FORCE_TO_MANUAL_RECONCILIATION",
        Severity::Critical,
        normalized_actor,
        Some(&success_payload),
    )
    .await?;
    Ok(ForceSeamOutcome::Applied)
}

/// PR #66 R4 L-R4-4 — bundles the CAS-WHERE-state failure diagnostic
/// arguments into a single struct.  Reduces helper arg count below
/// clippy's 7-arg threshold and makes call sites self-documenting.
struct CasFailureContext<'a> {
    shift_id: ShiftId,
    shift_id_hex: &'a str,
    fiscal_number: &'a str,
    current_state_read_was: ShiftState,
    actor: Option<&'a str>,
    rows_affected: u64,
}

/// PR #66 R3 H-R2-1 + R4 L-R4-3 + R5 L-R5-1/L-R5-3 — shared helper for
/// force-seam AND senior-close CAS-WHERE-state graceful failure paths.
/// Re-reads observed state for diagnostics + emits the given Critical
/// `audit_event_type` audit (forensic via Ok-return contract) + returns
/// `Option<ShiftState>` (observed-at-update) so caller can wrap into
/// its specific outcome variant (`ForceSeamOutcome::TxIsolationViolation`
/// OR `SeniorCloseOutcome::TxIsolationViolation`).
///
/// `extra_payload_fields` (R5 L-R5-1 fix): type-safe via
/// `Option<serde_json::Map<String, serde_json::Value>>` — caller cannot
/// accidentally pass `Number` / `Array` / `String` value that would
/// silently no-op.  Senior close passes `Some(map)` with
/// `senior_cashier_id` + `z_report_document_id`; force seams pass
/// `None` for no extras.
async fn emit_cas_isolation_violation_audit(
    tx: &mut WriteTxConn<'_>,
    audit_event_type: &str,
    attempted_seam: &'static str,
    ctx: CasFailureContext<'_>,
    extra_payload_fields: Option<serde_json::Map<String, serde_json::Value>>,
) -> sqlx::Result<Option<ShiftState>> {
    // Re-read for diagnostics — same tx, serialised with the failed UPDATE.
    let observed: Option<ShiftState> =
        sqlx::query_scalar(r#"SELECT state as "state: ShiftState" FROM shifts WHERE shift_id = ?"#)
            .bind(ctx.shift_id)
            .fetch_optional(&mut **tx)
            .await?;
    let mut payload = serde_json::json!({
        "fiscal_number": ctx.fiscal_number,
        "shift_id": ctx.shift_id_hex,
        "current_state_at_read": ctx.current_state_read_was.as_str(),
        "observed_state_at_update": observed.map(|s| s.as_str()),
        "attempted_seam": attempted_seam,
        "rows_affected": ctx.rows_affected,
        "reason": "cas_where_state_hit_zero_rows",
    });
    // Merge extras (e.g. senior_cashier_id + z_report_document_id).
    // Type-safe via Option<Map> — no silent no-op on wrong shape.
    if let Some(extras) = extra_payload_fields {
        if let serde_json::Value::Object(ref mut base) = payload {
            for (k, v) in extras {
                base.insert(k, v);
            }
        }
    }
    audit_log::append_tx(
        tx,
        "shift",
        ctx.shift_id_hex,
        audit_event_type,
        Severity::Critical,
        ctx.actor,
        Some(&payload.to_string()),
    )
    .await?;
    Ok(observed)
}

// ─── M3b W14a-2a: senior cashier close seam ──────────────────────────
//
// Per spec §16.9 — legitimate alternative close path via 5-ПРРО senior
// cashier role privilege.  NOT a force seam (no force-to-Error / force-
// to-Manual semantics); explicit clean-close surface where the closing
// cashier differs from `opened_by_cashier_id`.
//
// Runtime validation per W14a-2a operator decision (2026-05-17):
// - `senior_cashier_id` MUST exist in `cashier_certs` for the SAME
//   fiscal_number as the shift.  Role/privilege validation is UI/operator
//   responsibility until a real role registry exists.
// - Source state MUST be Opened or ClosingLocalPendingDrain (the only
//   states where senior close is meaningful per spec §5.6 matrix).

/// W14a-2a senior-close-seam outcome.  Mirrors the [`ForceSeamOutcome`]
/// Ok-return pattern (PR #66 R1 M3): legitimate refusals are returned
/// inside `Ok(_)` so the surrounding `with_immediate` envelope COMMITS
/// the `SHIFT_SENIOR_CLOSE_REFUSED` Warning audit row.  This preserves
/// the forensic trail for abuse detection (bad actor scanning senior
/// cashier IDs leaves a record), mirroring spec §8 + Round 7 §8.1.
///
/// PR #66 R4 L-R4-2: `#[non_exhaustive]` per [`ForceSeamOutcome`]
/// rationale — downstream consumers must use wildcard pattern arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SeniorCloseOutcome {
    /// State transitioned to `Closed`; Info audit emitted with actor =
    /// senior cashier id.
    Applied,
    /// Shift state was not closable per spec §5.6 (only `Opened` /
    /// `ClosingLocalPendingDrain` accept senior close); state UNCHANGED;
    /// `SHIFT_SENIOR_CLOSE_REFUSED` Warning audit emitted with
    /// `reason: "not_closable"`.
    RefusedNotClosable { current_state: ShiftState },
    /// `senior_cashier_id` is not registered in `cashier_certs` for the
    /// shift's fiscal_number; state UNCHANGED;
    /// `SHIFT_SENIOR_CLOSE_REFUSED` Warning audit emitted with
    /// `reason: "cashier_not_registered"`.
    RefusedCashierNotRegistered {
        senior_cashier_id: CashierId,
        fiscal_number: String,
    },
    /// PR #66 R3 H-R2-1: CAS-WHERE-state UPDATE hit `rows_affected != 1`
    /// despite the state having been read + validated inside the same tx.
    /// Symmetric with [`ForceSeamOutcome::TxIsolationViolation`].
    /// State UNCHANGED; `SHIFT_SENIOR_CLOSE_TX_ISOLATION_VIOLATION`
    /// Critical audit emitted with `observed_state_at_update` +
    /// `rows_affected` for forensics.  Replaces R2 MED-3 panic per
    /// `fiscal_documents.rs:351` HIGH-3 convention.
    TxIsolationViolation {
        observed_state_at_update: Option<ShiftState>,
        rows_affected: u64,
    },
}

/// W14a-2a senior-close-seam pre-flight validation error.  Reserved for
/// caller bugs (malformed evidence_json, stale shift_id) + DB-side
/// failures.  Legitimate refusal cases (`NotClosable`,
/// `SeniorCashierNotRegistered`) are NOT here — they're outcomes
/// because the tx must commit the refused-audit row.
#[derive(Debug, thiserror::Error)]
pub enum SeniorCloseError {
    /// `evidence_json` is not valid JSON OR exceeds
    /// [`FORCE_SEAM_EVIDENCE_MAX_BYTES`].
    #[error("evidence_json invalid: {0}")]
    InvalidEvidenceJson(String),

    /// Shift row not found for the given id.
    #[error("shift {shift_id_hex} not found")]
    ShiftNotFound { shift_id_hex: String },

    /// Wrapped sqlx error.
    #[error("sqlx error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Senior cashier close-shift seam per spec §16.9.
///
/// Allowed source states: `Opened` (online close path) and
/// `ClosingLocalPendingDrain` (offline-Z drain finalisation path).  Per
/// spec §5.6 matrix, no other states accept senior close.
///
/// Side effects on success (in this order per PR #66 R6 MED audit-
/// ordering fix — success audit must NEVER precede the guarded UPDATE):
/// - UPDATE `shifts.state = 'CLOSED'`, `closed_by_cashier_id = senior_cashier_id`,
///   `z_report_document_id = z_report_doc_id`, `closed_at = CURRENT_TIMESTAMP`
///   (CAS-guarded on `WHERE shift_id = ? AND state = ?`).
/// - On `rows_affected == 1`: emit `SHIFT_CLOSED_BY_SENIOR_CASHIER` Info audit
///   AFTER, return `Ok(SeniorCloseOutcome::Applied)`.
/// - On `rows_affected != 1`: emit `SHIFT_SENIOR_CLOSE_TX_ISOLATION_VIOLATION`
///   Critical audit + return `Ok(SeniorCloseOutcome::TxIsolationViolation
///   { .. })` (no success audit emitted on this defensive branch).
///
/// Source-state restriction is NOT routed through the [`allowed_transition`]
/// whitelist — this seam has its own contract.  Audit event distinguishes
/// this path from normal `Opened → Closing → Closed` whitelist edges 8 + 10.
pub async fn senior_cashier_close_shift_with_audit(
    tx: &mut WriteTxConn<'_>,
    shift_id: ShiftId,
    senior_cashier_id: &CashierId,
    z_report_doc_id: DocumentId,
    evidence_json: &str,
) -> Result<SeniorCloseOutcome, SeniorCloseError> {
    // 1. Validate evidence (size + JSON shape).
    if evidence_json.len() > FORCE_SEAM_EVIDENCE_MAX_BYTES {
        return Err(SeniorCloseError::InvalidEvidenceJson(format!(
            "evidence_json {} bytes exceeds cap {}",
            evidence_json.len(),
            FORCE_SEAM_EVIDENCE_MAX_BYTES
        )));
    }
    serde_json::from_str::<serde_json::Value>(evidence_json)
        .map_err(|e| SeniorCloseError::InvalidEvidenceJson(format!("not valid JSON: {e}")))?;

    // 2. Load current shift state + fiscal_number (single tx-bound read).
    let row: Option<(ShiftState, String)> = sqlx::query_as(
        r#"SELECT state as "state: ShiftState", fiscal_number FROM shifts WHERE shift_id = ?"#,
    )
    .bind(shift_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (current_state, fiscal_number) = row.ok_or_else(|| SeniorCloseError::ShiftNotFound {
        shift_id_hex: hex_lower(shift_id.as_bytes()),
    })?;
    let shift_id_hex = hex_lower(shift_id.as_bytes());
    // PR #66 R2 LOW-2: hoist z_report hex (computed twice in R1 — once
    // inside refused closure, once in success payload).
    let z_report_doc_hex = hex_lower(z_report_doc_id.as_bytes());

    // PR #66 R1 M3: build refused-audit payload helper for both
    // legitimate-refusal branches (NotClosable + CashierNotRegistered).
    // Per spec §8 + Round 7 §8.1: Ok-return contract preserves the
    // refused audit row via with_immediate commit (forensic trail for
    // abuse detection — bad actor scanning senior IDs leaves a record).
    let make_refused_payload = |reason: &str| -> String {
        serde_json::json!({
            "fiscal_number": fiscal_number,
            "shift_id": shift_id_hex,
            "current_state": current_state.as_str(),
            "attempted_seam": "senior_cashier_close_shift_with_audit",
            "senior_cashier_id": senior_cashier_id.as_str(),
            "z_report_document_id": z_report_doc_hex,
            "reason": reason,
            "evidence_json": evidence_json,
        })
        .to_string()
    };

    // 3. Source-state restriction per spec §5.6 — refused if not closable.
    use ShiftState::*;
    if !matches!(current_state, Opened | ClosingLocalPendingDrain) {
        let payload = make_refused_payload("not_closable");
        audit_log::append_tx(
            tx,
            "shift",
            &shift_id_hex,
            "SHIFT_SENIOR_CLOSE_REFUSED",
            Severity::Warning,
            Some(senior_cashier_id.as_str()),
            Some(&payload),
        )
        .await?;
        return Ok(SeniorCloseOutcome::RefusedNotClosable { current_state });
    }

    // 4. Runtime existence check: senior cashier MUST be registered in
    //    cashier_certs for THIS fiscal_number.  Refused with audit if not.
    let registered: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cashier_certs WHERE cashier_id = ? AND fiscal_number = ?",
    )
    .bind(senior_cashier_id.as_str())
    .bind(&fiscal_number)
    .fetch_one(&mut **tx)
    .await?;
    if registered == 0 {
        let payload = make_refused_payload("cashier_not_registered");
        audit_log::append_tx(
            tx,
            "shift",
            &shift_id_hex,
            "SHIFT_SENIOR_CLOSE_REFUSED",
            Severity::Warning,
            Some(senior_cashier_id.as_str()),
            Some(&payload),
        )
        .await?;
        return Ok(SeniorCloseOutcome::RefusedCashierNotRegistered {
            senior_cashier_id: senior_cashier_id.clone(),
            fiscal_number,
        });
    }

    // 5. Allowed path — CAS UPDATE FIRST, Info audit AFTER.  PR #66 R6
    //    MED fix: was emitting success audit BEFORE the CAS UPDATE; if
    //    rows_affected != 1 hit the violation path, audit log carried
    //    BOTH SHIFT_CLOSED_BY_SENIOR_CASHIER + SHIFT_SENIOR_CLOSE_TX_
    //    ISOLATION_VIOLATION — false-success forensic record.
    // PR #66 R2 MED-3 + R3 H-R2-1: CAS-WHERE-state guard with graceful
    // failure path (see emit_force_seam_tx_isolation_violation helper +
    // ForceSeamOutcome::TxIsolationViolation rationale).
    let res = sqlx::query(
        "UPDATE shifts SET state = ?, closed_by_cashier_id = ?, z_report_document_id = ?, \
            closed_at = CURRENT_TIMESTAMP \
         WHERE shift_id = ? AND state = ?",
    )
    .bind(ShiftState::Closed)
    .bind(senior_cashier_id.as_str())
    .bind(z_report_doc_id)
    .bind(shift_id)
    .bind(current_state)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() != 1 {
        // PR #66 R4 L-R4-3: DRY via shared helper with extras for the
        // senior-close-specific payload fields (senior_cashier_id +
        // z_report_document_id).  Helper handles re-read + audit emit +
        // returns observed state for outcome wrapping.
        let rows_affected = res.rows_affected();
        let observed = emit_cas_isolation_violation_audit(
            tx,
            "SHIFT_SENIOR_CLOSE_TX_ISOLATION_VIOLATION",
            "senior_cashier_close_shift_with_audit",
            CasFailureContext {
                shift_id,
                shift_id_hex: &shift_id_hex,
                fiscal_number: &fiscal_number,
                current_state_read_was: current_state,
                actor: Some(senior_cashier_id.as_str()),
                rows_affected,
            },
            Some({
                // PR #66 R5 L-R5-1: typed Map (was serde_json::Value).
                let mut extras = serde_json::Map::new();
                extras.insert(
                    "senior_cashier_id".to_string(),
                    serde_json::Value::String(senior_cashier_id.as_str().to_string()),
                );
                extras.insert(
                    "z_report_document_id".to_string(),
                    serde_json::Value::String(z_report_doc_hex.clone()),
                );
                extras
            }),
        )
        .await?;
        return Ok(SeniorCloseOutcome::TxIsolationViolation {
            observed_state_at_update: observed,
            rows_affected,
        });
    }

    // 6. Success — UPDATE applied; emit canonical Info audit AFTER the
    //    CAS UPDATE success per PR #66 R6 MED audit-ordering fix.
    let payload = serde_json::json!({
        "fiscal_number": fiscal_number,
        "shift_id": shift_id_hex,
        "from_state": current_state.as_str(),
        "to_state": ShiftState::Closed.as_str(),
        "closed_by_senior_cashier_id": senior_cashier_id.as_str(),
        "z_report_document_id": z_report_doc_hex,
        "evidence_json": evidence_json,
    })
    .to_string();
    audit_log::append_tx(
        tx,
        "shift",
        &shift_id_hex,
        "SHIFT_CLOSED_BY_SENIOR_CASHIER",
        Severity::Info,
        Some(senior_cashier_id.as_str()),
        Some(&payload),
    )
    .await?;
    Ok(SeniorCloseOutcome::Applied)
}
