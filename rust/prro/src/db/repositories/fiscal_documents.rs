//! Repository for `fiscal_documents`.
//!
//! Repo policy:
//! - SELECT decode through `sqlx::query!` (compile-time schema verification).
//! - INSERT and UPDATE runtime-bound (param types enforced by `Encode<Sqlite>`).
//! - State transitions go through CAS UPDATE + a code-level `allowed_transition`
//!   whitelist, returning a `TransitionOutcome` enum that distinguishes:
//!     * Applied  — CAS hit, row now in `to`;
//!     * Forbidden — whitelist rejected the (from, to) pair (no DB call);
//!     * Conflict — row exists but its state != `from` (caller should reload);
//!     * NotFound — no row with `document_id`.
//!
//!   This split is what write_path retry logic (M3) needs to decide between
//!   "reload + retry" (Conflict) and "escalate" (NotFound).  Per ADR-M3-A4
//!   and W0-2 §4.4 (M3a W2), `transition_state` takes
//!   `&mut WriteTxConn<'_>` rather than a pool: the disambiguation
//!   `SELECT` and the CAS `UPDATE` run on the same connection inside the
//!   same `with_immediate` BEGIN IMMEDIATE envelope, so the
//!   Conflict-vs-NotFound result is atomic by construction —
//!   PRRO_GATE-k99 closed structurally.

use crate::db::models::{
    enums::{DocState, DocType},
    ids::{CashierId, DocumentId, OfflineSessionId, RequestId, ShiftId},
};
use crate::db::tx::WriteTxConn;
use crate::db::types::{
    DbCashierId, DbDocState, DbDocType, DbDocumentId, DbOfflineSessionId, DbRequestId, DbShiftId,
};
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct NewDocument {
    pub document_id: DocumentId,
    pub request_id: RequestId,
    pub fiscal_number: String,
    pub shift_id: Option<ShiftId>,
    pub offline_session_id: Option<OfflineSessionId>,
    pub lnd: i64,
    pub doc_type: DocType,
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    pub fs_mode: &'static str, // "ONLINE" | "OFFLINE"
    pub business_ts: String,   // ISO-8601
    pub total_sum_kop: Option<i64>,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    /// RS-3 A1Z (migration 024) — the SOURCE hash: the wire-intent hash the
    /// inbox carries. For non-Z it equals `payload_sha256_canonical`; for a
    /// Z doc it is the WIRE hash while `payload_sha256_canonical` is the hash
    /// of the AGGREGATED body (D5). Persisted so boot PREPARED-replay recovery
    /// can distinguish a legitimate Z dual-hash from real drift.
    pub source_sha256: [u8; 32],
    pub unsigned_xml_sha256: Option<[u8; 32]>,
    pub previous_hash: Option<[u8; 32]>,
    /// W14a-2b §1.4 — cashier id that will sign this document.  Persisted
    /// on the ledger row at stage 1 INSERT PREPARED; consumed at
    /// stage_send 4-pre by signer_guard.  None for system-context paths
    /// without operator attribution (none currently).
    pub signed_by_cashier_id: Option<CashierId>,
    /// W4-Z2a piece 6b.1 (external mid-review CRIT-2) — FK to
    /// `signing_config_snapshots.id` for the snapshot row inserted
    /// by stage_acquire in the SAME with_immediate envelope as
    /// this row.  Setting FK at INSERT (not at 3-PRE pin) closes
    /// the two-envelope crash window: any taxable PREPARED doc on
    /// disk already references its frozen tax config snapshot.
    ///
    /// `Some(id)` is the only production value (stage_acquire
    /// always inserts a snapshot row for the receipt's FN+driver).
    /// `None` is reserved for pre-W4-Z2a docs persisted before
    /// migration 020 ran — those rows already exist with NULL,
    /// not via this fresh-INSERT path.
    pub signing_config_snapshot_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DocumentRow {
    pub document_id: DocumentId,
    pub fiscal_number: String,
    pub lnd: i64,
    pub state: DocState,
    pub doc_type: DocType,
    pub server_fiscal_no: Option<String>,
    pub submission_attempted_at: Option<String>,
    /// W5 — read along the doc to make resume path use PERSISTED
    /// profile bindings (not the current `node_state.*_profile_id`,
    /// which can drift between original PREPARED and a later resume
    /// pickup).  Schema-NOT-NULL on `fiscal_documents`, so always
    /// present for a successfully-inserted doc.
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    /// W6 — previous-doc unsigned-XML SHA256 (raw 32 bytes, NOT hex).
    /// Pinned in stage 3-PRE atomically with `signing_inputs_pinned_at`.
    /// Hex-encoded only at the XML builder boundary (`<MAC>` attr).
    /// `None` for a doc whose chain seed was unset at pin time
    /// (genuine first-after-bootstrap), distinguishable from "not
    /// pinned yet" via [`signing_inputs_pinned_at`].
    pub previous_hash: Option<[u8; 32]>,
    /// W6 — Z-report counter persisted on the doc; `Some(N)` only for
    /// `wire_artifact_kind == ZReport` (DocType::ShiftClose or
    /// DocType::ZReport after boundary mapping).  Pinned in stage
    /// 3-PRE; retry observes the persisted value and reuses it.
    pub z_report_number: Option<i64>,
    /// W6 — sha256 of the canonical unsigned XML.  Updated NULL→hash
    /// in stage 3-PERSIST (was set NULL by W5 INSERT PREPARED).
    pub unsigned_xml_sha256: Option<[u8; 32]>,
    /// W6 — pin-once flag (ISO8601 timestamp).  `None` = stage 3-PRE
    /// has not pinned signing inputs yet.  `Some(_)` = pinned; the
    /// `previous_hash` and `z_report_number` columns are now
    /// authoritative for retry.
    pub signing_inputs_pinned_at: Option<String>,
    /// W14a-2b §1.4 — cashier id that signed (or will sign) this
    /// document.  Persisted at INSERT PREPARED; consumed at stage_send
    /// 4-pre by signer_guard.  `None` for pre-W14a-2b ledger rows
    /// (column added in migration 017_signed_by_cashier_id.sql).
    pub signed_by_cashier_id: Option<CashierId>,
    /// W4-Z2a piece 6b mid-fix CRIT-2 (external mid-review) — FK to
    /// `signing_config_snapshots.id` for the snapshot row pinned to
    /// this document.  Set at INSERT PREPARED (atomic with lease in
    /// stage_acquire) and never drifts (COALESCE-preserve in pin's
    /// UPDATE).  Mirror of `PinnedSigningInputs.signing_config_snapshot_id`
    /// — same row, both struct shapes now expose it to callers that
    /// don't use the pinned-inputs read path (e.g., MAC recovery's
    /// MR-NO-TX read in piece 9 / boot recovery in stage_acquire).
    /// `None` for pre-W4-Z2a ledger rows (column added in migration
    /// 020_signing_config_snapshots.sql).
    pub signing_config_snapshot_id: Option<i64>,
}

/// W6 — decode a length-32 BLOB into a fixed `[u8; 32]`.  Fail-closed:
/// any non-NULL value whose length is not 32 surfaces as a
/// `sqlx::Error::Decode` rather than a silent truncation.  Schema
/// CHECK clauses (`previous_hash`, `unsigned_xml_sha256`) make this
/// path unreachable in production, but a corrupted or hand-edited DB
/// is then operator-visible at row load time.
fn decode_blob32(
    raw: Option<Vec<u8>>,
    column: &'static str,
) -> Result<Option<[u8; 32]>, sqlx::Error> {
    match raw {
        None => Ok(None),
        Some(v) => {
            let arr: [u8; 32] = v.as_slice().try_into().map_err(|_| {
                sqlx::Error::Decode(
                    format!(
                        "fiscal_documents.{column}: expected 32 bytes, got {}",
                        v.len()
                    )
                    .into(),
                )
            })?;
            Ok(Some(arr))
        }
    }
}

/// Outcome of a state transition attempt.  Replaces a bare `bool` so write_path
/// retry logic can tell "row diverged, reload" apart from "row vanished, escalate".
///
/// Precedence: `Forbidden` is decided in code BEFORE the DB is consulted, so a
/// forbidden transition for a non-existent `document_id` returns `Forbidden`,
/// not `NotFound`.  This is by design — `Forbidden` indicates a caller bug
/// (asked for a transition not in the whitelist) and should be surfaced
/// regardless of row existence.  `NotFound` is reserved for the case where
/// the requested transition WOULD have been allowed but the row is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionOutcome {
    Applied,
    Forbidden,
    Conflict,
    NotFound,
}

pub fn allowed_transition(from: DocState, to: DocState) -> bool {
    use DocState::*;
    matches!(
        (from, to),
        (Prepared, Signed)
            | (Prepared, Rejected)
            // Post-sign refusal orphan fix: a doc refused before issuance lands
            // in the non-issued TERMINAL `Aborted` (from PREPARED if refused
            // before sign, or from SIGNED if refused at dispatch/offline-ack),
            // restoring the ledger-only pin (no stuck non-terminal row).
            | (Prepared, Aborted)
            | (Signed, Aborted)
            | (Signed, Encrypted)
            | (Signed, ErrorRetryable)
            | (Signed, OfflineLocalAck)
            // A.3 PR-B step 6: (Encrypted, Sent) removed — sfn-less dormant edge;
            // Encrypted → Sending → Sent is the live Pattern B ladder.
            | (Encrypted, ErrorRetryable)
            | (Sent, Kvt1)
            | (Sent, ErrorRetryable)
            // A.3 PR-B step 6: (Sent, Rejected) removed — policy D3: a post-SENT
            // reject is NEVER routed to Rejected; it escalates to
            // RequiresManualReconciliation (issued-but-unconfirmed, lnd consumed,
            // sfn stamped, seed advanced at SEND — must not silently vanish).
            // W11 PR-2b — SENT last_chk mismatch escalation per W0-3 §6.4-b
            // (`docs/superpowers/specs/2026-05-06-m3-w0-3-retry-recovery.md:771-772`).
            // When boot-recovery probe yields `CheckAck { id != transport_request_id }`,
            // the doc cannot be reconciled automatically: we have on-record a SENT
            // marker but DPS reports a different fiscal id.  Direct transition to
            // RequiresManualReconciliation is the operator-handoff edge — no prior
            // hop through ErrorRetryable, because the situation is not retryable
            // (DPS state and local state diverged at the protocol layer).
            | (Sent, RequiresManualReconciliation)
            // CS-3 Slice 5 (design §3.4): the one missing live document edge for operator
            // completion of a crashed / SubmittedUnknown send — a `Sending` document with a
            // matching CALL_STARTED / PENDING reservation whose operator resolution is
            // not-accepted (or MAC) is moved to the existing manual/terminal state.  Callable
            // only from the operator-completion branch (`complete_operator_pending`); NOT a resend.
            | (Sending, RequiresManualReconciliation)
            | (Kvt1, Kvt2)
            | (Kvt1, ErrorRetryable)
            | (Kvt2, Ack)
            // M3b W6 — Pattern C edges per spec §5.3 / M3b plan §Task 6.  The
            // W4/W5 offline subsystem produces docs in `OfflineLocalAck` while
            // node is offline; on return-online the W7 drain stage flips them
            // through `Sending` (Pattern B intent-marker, mirroring the online
            // ladder) and `Cancelled` is the manual-operator escape if the drain
            // is abandoned mid-flight.  Locked edge set + count pinned in
            // `tests/fiscal_documents_offline_local_ack_edges_locked.rs`.
            //
            // AUD-L1-2 (2026-06-14) auto-invoker audit of this cluster:
            //   (OfflineLocalAck, Sending)  — WIRED: W7 drain via
            //     `stage_send.rs` 4-pre (the only OfflineLocalAck invoker).
            //   (OfflineLocalAck, Cancelled) — no auto-invoker;
            //     operator/force-seam only (runtime wiring deferred).
            //
            // A.3 PR-B step 6 (re-verified zero producers on today's main):
            //   (OfflineLocalAck, Sent) [legacy M3a placeholder superseded by
            //   the Sending ladder] and (OfflineLocalAck, Kvt2) [W9b lastChk
            //   short-circuit — SPEC'd but NEVER wired; drain converges via
            //   Sending → Sent → Kvt1 → Kvt2 in `kvt2_confirm.rs`] REMOVED as
            //   sfn-less dormant edges.
            | (OfflineLocalAck, Sending)
            | (OfflineLocalAck, Cancelled)
            // A.3 PR-B step 6: (ErrorRetryable, Sent) + (ErrorRetryable, Kvt1)
            // removed — sfn-less dormant; re-sends go ErrorRetryable → Sending →
            // (fresh wire) → Sent → Kvt1, never a direct issue-forward hop.
            | (ErrorRetryable, RequiresManualReconciliation)
            // Pattern B (ADR-M3-A5 / A9 step 3): Sending is the
            // intent-marker for stage 4 send.  The 6 additions below
            // wire it in (was 7; A.3 PR-B step 6 removed (Sending, Kvt1)).
            // Retries go through (ErrorRetryable, Sending) and then on
            // through a fresh wire call, never a direct issue-forward hop.
            | (Signed, Sending)
            | (Encrypted, Sending)
            | (Sending, Sent)
            // A.3 PR-B step 6: (Sending, Kvt1) removed — sfn-less.  It RETURNS
            // when the inline fast-path lands, and ONLY THEN carrying its own
            // atomic `server_fiscal_no` stamp + seed advance at this CAS (the
            // A.3 D3 sfn-lockstep pin, mirroring the WireDecision::Sent arm in
            // `stage_send`).  Re-add here in lockstep with that producer.
            | (Sending, ErrorRetryable)
            | (Sending, Rejected)
            | (ErrorRetryable, Sending)
            // W10.4 step 2d: MAC recovery failure overrides
            // (`HashNotExtractable` / `CounterExhausted` /
            // second-`-12` short-circuit) terminate the doc directly
            // from `ErrorRetryable` without a fresh wire send.
            // Semantically: "we tried recovery, it failed, give up".
            // Freeze §4.4.4 step 2d names this CAS explicitly; the
            // whitelist edge was added in W10.5 follow-up.
            | (ErrorRetryable, Rejected)
    )
}

pub async fn insert_prepared(pool: &SqlitePool, n: &NewDocument) -> sqlx::Result<()> {
    sqlx::query(
        r#"INSERT INTO fiscal_documents (
             document_id, request_id, fiscal_number, shift_id, offline_session_id,
             lnd, doc_type, state, backend_profile_id, transport_profile_id,
             fs_mode, business_ts, total_sum_kop, payload_json,
             payload_sha256_canonical, source_sha256, unsigned_xml_sha256, previous_hash,
             signed_by_cashier_id, signing_config_snapshot_id
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 'PREPARED', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(DbDocumentId(n.document_id))
    .bind(DbRequestId(n.request_id))
    .bind(&n.fiscal_number)
    .bind(n.shift_id.map(DbShiftId))
    .bind(n.offline_session_id.map(DbOfflineSessionId))
    .bind(n.lnd)
    .bind(n.doc_type.as_str())
    .bind(&n.backend_profile_id)
    .bind(&n.transport_profile_id)
    .bind(n.fs_mode)
    .bind(&n.business_ts)
    .bind(n.total_sum_kop)
    .bind(&n.payload_json)
    .bind(&n.payload_sha256_canonical[..])
    .bind(&n.source_sha256[..])
    .bind(n.unsigned_xml_sha256.as_ref().map(|b| &b[..]))
    .bind(n.previous_hash.as_ref().map(|b| &b[..]))
    .bind(n.signed_by_cashier_id.as_ref().map(|c| c.as_str()))
    .bind(n.signing_config_snapshot_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomic state transition.  See [`TransitionOutcome`] for the four possible
/// results.  The whitelist short-circuits forbidden moves before any DB call;
/// a successful CAS returns Applied; a missed CAS triggers a follow-up
/// existence check to disambiguate Conflict from NotFound.
///
/// Per ADR-M3-A4 / W0-2 §4.4 (M3a W2), takes `&mut WriteTxConn<'_>` —
/// callers obtain it from a `with_immediate` closure, which guarantees
/// the CAS UPDATE and the disambiguation SELECT run on the same
/// connection inside the same BEGIN IMMEDIATE envelope.
///
/// **M3b W3 — `first_kvt1_at` stamp on Kvt1 transitions.**  When
/// `to == DocState::Kvt1`, the CAS UPDATE additionally sets
/// `first_kvt1_at = COALESCE(first_kvt1_at, CURRENT_TIMESTAMP)` in
/// the same atomic statement.  `COALESCE` semantics:
///   - **first Kvt1 entry** (column is NULL): set to `CURRENT_TIMESTAMP`;
///   - **re-entry into Kvt1** (column already populated; happens
///     when a doc cycles back into Kvt1 via a whitelisted re-entry
///     path such as `ErrorRetryable → Kvt1`, e.g. after a transient
///     wire failure during the M3a `Sent → Kvt1` recovery probe):
///     preserve the original timestamp.
///
/// Non-Kvt1 transitions leave the column unchanged.  Tested in
/// `tests/boot_phase_w9_helpers.rs` (W3 acceptance fixtures).
pub async fn transition_state(
    tx: &mut WriteTxConn<'_>,
    id: DocumentId,
    from: DocState,
    to: DocState,
) -> sqlx::Result<TransitionOutcome> {
    if !allowed_transition(from, to) {
        return Ok(TransitionOutcome::Forbidden);
    }
    // M3b W3 — Kvt1 arm stamps `first_kvt1_at` atomically with the
    // state CAS.  Branching in Rust (not SQL) keeps the non-Kvt1
    // hot path identical to M3a and avoids parameter-driven
    // UPDATE complications.
    let res = if to == DocState::Kvt1 {
        sqlx::query(
            "UPDATE fiscal_documents \
             SET state = ?, first_kvt1_at = COALESCE(first_kvt1_at, CURRENT_TIMESTAMP) \
             WHERE document_id = ? AND state = ?",
        )
        .bind(to.as_str())
        .bind(DbDocumentId(id))
        .bind(from.as_str())
        .execute(&mut **tx)
        .await?
    } else {
        sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ? AND state = ?")
            .bind(to.as_str())
            .bind(DbDocumentId(id))
            .bind(from.as_str())
            .execute(&mut **tx)
            .await?
    };
    if res.rows_affected() == 1 {
        return Ok(TransitionOutcome::Applied);
    }
    // CAS missed — disambiguate row-missing vs state-diverged.  Same
    // connection inside the same BEGIN IMMEDIATE tx (via WriteTxConn
    // Deref), so this SELECT cannot interleave with another writer's
    // INSERT/DELETE.
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM fiscal_documents WHERE document_id = ? LIMIT 1")
            .bind(DbDocumentId(id))
            .fetch_optional(&mut **tx)
            .await?;
    Ok(if exists.is_some() {
        TransitionOutcome::Conflict
    } else {
        TransitionOutcome::NotFound
    })
}

/// M3b W7 — atomic `Signed → OfflineLocalAck` transition with
/// simultaneous stamping of the W4-era offline columns.
///
/// Single UPDATE statement: flips state AND stamps
/// `offline_fiscal_no` (= acquired `code_lnd`) AND
/// `offline_fiscal_date` (= the `consumed_at` returned by W5's
/// `acquire_code_tx`) AND `offline_session_id` (= the FN's current
/// active OPEN session).  All four columns become non-NULL
/// atomically with the state flip — caller never observes a doc
/// in OFFLINE_LOCAL_ACK with NULL offline_fiscal_no / _date /
/// session_id.
///
/// Parallel shape to [`transition_state`]: **whitelist gate runs
/// BEFORE the DB call** (mirrors W1 discipline; release-build-
/// effective, not debug-only).  Successful CAS returns `Applied`;
/// CAS miss disambiguates `Conflict` vs `NotFound` via a follow-up
/// SELECT.
///
/// **Cross-FN guard (operator W7 Round 1 HIGH-2 fix, 2026-05-15)**:
/// the UPDATE WHERE clause filters on `document_id = ? AND
/// fiscal_number = ? AND state = 'SIGNED'`.  Caller MUST pass the
/// FN that the doc actually belongs to; mismatched FN → zero
/// rows_affected → `Conflict` / `NotFound` disambiguation.  This
/// closes a fiscal-integrity hole where a caller misusing the
/// helper could stamp doc of FN A with code/session of FN B (the
/// W4 schema's FK on offline_session_id is NOT composite with
/// fiscal_number, so SQL constraints alone wouldn't catch it).
///
/// Pre-conditions (caller's responsibility — `stage_offline_ack`
/// enforces them inside the same `with_immediate` envelope as this
/// call, so a refusal aborts the tx before any column is touched):
///   1. Node mode ∈ {Offline, GoingOffline}.
///   2. Shift state == Opened.
///   3. Active OPEN session exists for the FN.
///   4. Code acquired from `offline_sessions::acquire_code_tx` —
///      `code_lnd` + `consumed_at` come from that helper's return.
///   5. Pre-check that the doc row exists, is in `Signed`, and
///      belongs to the same FN — surfaces typed refusals before
///      any code is consumed.
pub async fn transition_to_offline_local_ack_tx(
    tx: &mut WriteTxConn<'_>,
    id: DocumentId,
    fiscal_number: &str,
    code_lnd: i64,
    consumed_at: &str,
    offline_session_id: OfflineSessionId,
    // B8-2: opaque DPS-issued code string from `AcquiredCode.dps_code`.
    // Stamped into `offline_dps_code` in the SAME CAS UPDATE so the doc
    // carries the wire-ready id_offline value from the moment of issuance.
    dps_code: &str,
) -> sqlx::Result<TransitionOutcome> {
    // Whitelist gate — RELEASE-effective per operator W7 Round 1
    // HIGH-3 fix (2026-05-15).  Was `debug_assert!` previously;
    // that compiles to no-op in release and would let raw state
    // updates through if the W6 edge ever flipped to false.
    if !allowed_transition(DocState::Signed, DocState::OfflineLocalAck) {
        return Ok(TransitionOutcome::Forbidden);
    }

    let res = sqlx::query(
        "UPDATE fiscal_documents \
         SET state = 'OFFLINE_LOCAL_ACK', \
             offline_fiscal_no = ?, \
             offline_fiscal_date = ?, \
             offline_session_id = ?, \
             offline_dps_code = ? \
         WHERE document_id = ? AND fiscal_number = ? AND state = 'SIGNED'",
    )
    .bind(code_lnd)
    .bind(consumed_at)
    .bind(DbOfflineSessionId(offline_session_id))
    .bind(dps_code)
    .bind(DbDocumentId(id))
    .bind(fiscal_number)
    .execute(&mut **tx)
    .await?;

    if res.rows_affected() == 1 {
        return Ok(TransitionOutcome::Applied);
    }
    // CAS missed — disambiguate row-missing vs state-diverged
    // (vs cross-FN mismatch, which also surfaces as Conflict —
    // the doc EXISTS but its fiscal_number doesn't match).
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM fiscal_documents WHERE document_id = ? LIMIT 1")
            .bind(DbDocumentId(id))
            .fetch_optional(&mut **tx)
            .await?;
    Ok(if exists.is_some() {
        TransitionOutcome::Conflict
    } else {
        TransitionOutcome::NotFound
    })
}

/// B9 Slice A — stamp the offline-issuance columns onto a **PREPARED** doc,
/// WITHOUT any state transition.  This is the sign-time half of the reorder:
/// `stage_sign` acquires the offline code inside its pin-tx and stamps
/// `offline_fiscal_no` / `offline_fiscal_date` / `offline_session_id` /
/// `offline_dps_code` here, so the opaque code is available to the canonical
/// XML builder as `<MAC ID='{offline_dps_code}'>` in the SIGNED bytes.
///
/// **Idempotency guard (INV-4 / INV-5).**  The WHERE clause requires
/// `state = 'PREPARED' AND offline_dps_code IS NULL`.  On a re-entry / boot
/// re-drive the columns are already set, the UPDATE matches 0 rows, and the
/// caller reads the persisted stamp back instead of acquiring a second code —
/// the offline code is consumed **exactly once**.  (`offline_dps_code` is
/// NULL on every fresh PREPARED row and is only ever written here or by the
/// `stage_offline_ack` fallback, so its NULL-ness is a clean
/// "not-yet-stamped" signal.)
///
/// Returns `rows_affected`: `1` = stamped by this call; `0` = already stamped
/// (re-entry) OR the row moved off PREPARED (caller re-reads the truth).
pub async fn stamp_offline_issuance_tx(
    tx: &mut WriteTxConn<'_>,
    id: DocumentId,
    fiscal_number: &str,
    code_lnd: i64,
    consumed_at: &str,
    offline_session_id: OfflineSessionId,
    dps_code: &str,
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE fiscal_documents \
         SET offline_fiscal_no = ?, \
             offline_fiscal_date = ?, \
             offline_session_id = ?, \
             offline_dps_code = ? \
         WHERE document_id = ? AND fiscal_number = ? \
           AND state = 'PREPARED' AND offline_dps_code IS NULL",
    )
    .bind(code_lnd)
    .bind(consumed_at)
    .bind(DbOfflineSessionId(offline_session_id))
    .bind(dps_code)
    .bind(DbDocumentId(id))
    .bind(fiscal_number)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected())
}

/// B9 Slice A — read back the offline-issuance stamp on a doc (the columns
/// [`stamp_offline_issuance_tx`] wrote at sign).  Used by the re-entry branch
/// in `stage_sign` (to thread the already-stamped `offline_dps_code` into the
/// XML builder without re-acquiring) and by the `stage_offline_ack`
/// acquire-OR-readback branch.  Returns `None` if the row is missing or the
/// stamp is absent (`offline_dps_code IS NULL`).
pub async fn read_offline_stamp_tx(
    tx: &mut WriteTxConn<'_>,
    id: DocumentId,
) -> sqlx::Result<Option<OfflineStamp>> {
    let db_id = DbDocumentId(id);
    let row = sqlx::query!(
        r#"SELECT offline_fiscal_no,
                  offline_fiscal_date,
                  offline_session_id as "offline_session_id: DbOfflineSessionId",
                  offline_dps_code
           FROM fiscal_documents WHERE document_id = ?"#,
        db_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    match (
        r.offline_fiscal_no,
        r.offline_fiscal_date,
        r.offline_session_id,
        r.offline_dps_code,
    ) {
        (Some(code_lnd), Some(consumed_at), Some(offline_session_id), Some(dps_code)) => {
            Ok(Some(OfflineStamp {
                code_lnd,
                consumed_at,
                offline_session_id: offline_session_id.0,
                dps_code,
            }))
        }
        _ => Ok(None),
    }
}

/// The offline-issuance stamp read back by [`read_offline_stamp_tx`].
#[derive(Debug, Clone)]
pub struct OfflineStamp {
    pub code_lnd: i64,
    pub consumed_at: String,
    pub offline_session_id: OfflineSessionId,
    pub dps_code: String,
}

/// B10 — existence probe for the offline-session boundary docs (DocType
/// `OFFLINE_SESSION_BEGIN` / `OFFLINE_SESSION_END`).  Returns `true` iff an
/// ACTIVE (non-terminal-failed) boundary doc of `doc_type` already exists for
/// `(fiscal_number, shift_id)`.
///
/// **Keyed on `shift_id`, NOT `offline_session_id`** — the session_id is
/// stamped only at SIGN (`stamp_offline_issuance_tx`), so a crashed-PREPARED
/// boundary doc has a NULL `offline_session_id` and would be invisible to a
/// session-keyed probe → double-mint (the arch-locked idempotency trap).  The
/// `shift_id` is set at mint and is the stable per-session anchor that survives
/// the PREPARED-crash window (one offline session per open shift).
///
/// **State set**: any state OTHER than the terminal-FAILED ones
/// (`REJECTED`/`ABORTED`/`CANCELLED`/`REQUIRES_MANUAL_RECONCILIATION`).  A
/// still-live (PREPARED/SIGNED/OFFLINE_LOCAL_ACK/…) or successfully-issued
/// (ACK) boundary doc counts as "exists" → do NOT re-mint.  A boundary doc that
/// terminated in a failed state (a prior aborted attempt) does NOT count → a
/// fresh mint is allowed.  This mirrors the partial UNIQUE index
/// `ux_fd_active_offline_boundary` (migration 030) — the DB constraint is the
/// fail-closed backstop; this read is the idempotency gate.
pub async fn active_boundary_doc_state_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    shift_id: ShiftId,
    doc_type: DocType,
) -> sqlx::Result<Option<DocState>> {
    debug_assert!(
        matches!(
            doc_type,
            DocType::OfflineSessionBegin | DocType::OfflineSessionEnd
        ),
        "active_boundary_doc_state_tx is only meaningful for boundary doc types"
    );
    let row: Option<DocState> = sqlx::query_scalar::<_, DbDocState>(
        "SELECT state FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? AND doc_type = ? \
           AND state NOT IN ( \
               'REJECTED','ABORTED','CANCELLED','REQUIRES_MANUAL_RECONCILIATION' \
           ) \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .bind(DbShiftId(shift_id))
    .bind(doc_type.as_str())
    .fetch_optional(&mut **tx)
    .await?
    .map(|w| w.0);
    Ok(row)
}

/// B10 (review Finding B fix) — read the `state` of the fiscal_documents row with
/// the given `request_id`, or `None` if no row exists.
///
/// This is the crash-safe idempotency probe for the lazy DocType=9 BEGIN, keyed
/// on the BEGIN's DETERMINISTIC synthetic `request_id` (`sha256("b10-begin-<fn>")`)
/// — the ONLY anchor stamped at MINT (stage_acquire INSERT) that survives EVERY
/// crash window AND works for EVERY first-offline-doc type:
///   - keying on `shift_id` misses when the first offline doc is a SHIFT_OPEN
///     (the shift is not created until that SHIFT_OPEN's acquire);
///   - keying on `offline_session_id` misses a crashed PREPARED BEGIN (the column
///     is stamped only at sign).
///
/// The caller maps the returned state: issued → proceed, terminal-failed / None
/// → mint, else (below-OLA non-terminal) → fail-closed 503.
pub async fn doc_state_by_request_id_tx(
    tx: &mut WriteTxConn<'_>,
    request_id: &[u8; 16],
) -> sqlx::Result<Option<DocState>> {
    let row: Option<DocState> = sqlx::query_scalar::<_, DbDocState>(
        "SELECT state FROM fiscal_documents WHERE request_id = ? LIMIT 1",
    )
    .bind(&request_id[..])
    .fetch_optional(&mut **tx)
    .await?
    .map(|w| w.0);
    Ok(row)
}

/// CS-3 S7-1 — read the current `state` of a document by id (pool-bound, no tx).
///
/// Used by the inline write-path to report the ACTUAL persisted document state on a
/// `202 IN_PROGRESS` outcome, instead of the routing-target guess.  Post-cutover the composed
/// apply HOLDS a transient (`SubmittedUnknown`) in `SENDING` (`PENDING_APPLY` + STOP), so the
/// routing target (`ErrorRetryable`) is no longer the persisted state; the FnConfigError path DOES
/// persist `ErrorRetryable`.  Both classify to `InProgress` (202), but the reported
/// `document_state` string must be truthful (first-pass ↔ replay parity: the re-poll path already
/// reads `fd.state`).  `None` ⟺ no such row.
pub async fn state_by_document_id(
    pool: &SqlitePool,
    id: DocumentId,
) -> sqlx::Result<Option<DocState>> {
    let row: Option<DocState> = sqlx::query_scalar::<_, DbDocState>(
        "SELECT state FROM fiscal_documents WHERE document_id = ? LIMIT 1",
    )
    .bind(DbDocumentId(id))
    .fetch_optional(pool)
    .await?
    .map(|w| w.0);
    Ok(row)
}

/// B9 Slice A — slim `SIGNED → OFFLINE_LOCAL_ACK` state CAS with **no** column
/// writes.  Used by `stage_offline_ack` when the offline-issuance columns were
/// already stamped at sign (the [`stamp_offline_issuance_tx`] path): the
/// offline_fiscal_no / date / session_id / dps_code are already correct on the
/// row, so the ack only advances state.  Whitelist-gated + cross-FN guarded
/// exactly like [`transition_to_offline_local_ack_tx`] (which stays for the
/// unstamped fallback path that acquires at ack-time).
pub async fn transition_signed_to_offline_local_ack_tx(
    tx: &mut WriteTxConn<'_>,
    id: DocumentId,
    fiscal_number: &str,
) -> sqlx::Result<TransitionOutcome> {
    if !allowed_transition(DocState::Signed, DocState::OfflineLocalAck) {
        return Ok(TransitionOutcome::Forbidden);
    }
    let res = sqlx::query(
        "UPDATE fiscal_documents \
         SET state = 'OFFLINE_LOCAL_ACK' \
         WHERE document_id = ? AND fiscal_number = ? AND state = 'SIGNED'",
    )
    .bind(DbDocumentId(id))
    .bind(fiscal_number)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() == 1 {
        return Ok(TransitionOutcome::Applied);
    }
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM fiscal_documents WHERE document_id = ? LIMIT 1")
            .bind(DbDocumentId(id))
            .fetch_optional(&mut **tx)
            .await?;
    Ok(if exists.is_some() {
        TransitionOutcome::Conflict
    } else {
        TransitionOutcome::NotFound
    })
}

/// Returns documents in non-final, non-handed-off states for the given FN,
/// in deterministic order suitable for fiscal-chain recovery.
///
/// Pending set (8 states, per ADR-M3-A8):
/// - PREPARED, SIGNED, ENCRYPTED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE
///
/// SENDING is pending (ADR-M3-A9 step 2): a crash between the CAS
/// Signed/Encrypted -> Sending and the wire send leaves the document in
/// SENDING with unknown wire-state; the App::boot recovery rule transitions
/// it to ERROR_RETRYABLE without invoking send_chk, because DPS does not
/// deduplicate and a re-send could fiscalise the same canonical receipt
/// twice.
///
/// KVT2 IS pending: a crash between persisting KVT2 and transitioning to ACK
/// would otherwise strand the document.  ACK is the only true terminal-success
/// state.
///
/// Excluded:
/// - ACK / REJECTED / CANCELLED — terminal.
/// - OFFLINE_LOCAL_ACK — handed off to offline_sync_service (separate worker).
/// - REQUIRES_MANUAL_RECONCILIATION — operator-driven flow.
///
/// Ordering: `(lnd, created_at, document_id)`.  `created_at` alone is
/// second-granular in SQLite (`CURRENT_TIMESTAMP`), so multiple docs created
/// within one second would otherwise have unstable order.  `lnd` is the
/// Local Numerator of Document — strictly monotonic per FN — so it is the
/// authoritative chain-recovery key; the other two are tiebreakers.
pub async fn list_pending_for_fn(pool: &SqlitePool, fn_id: &str) -> sqlx::Result<Vec<DocumentRow>> {
    let rows = sqlx::query!(
        r#"SELECT document_id    as "document_id: DbDocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DbDocState",
                  doc_type        as "doc_type: DbDocType",
                  server_fiscal_no,
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at,
                  signed_by_cashier_id  as "signed_by_cashier_id: DbCashierId",
                  signing_config_snapshot_id
           FROM fiscal_documents
           WHERE fiscal_number = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')
           ORDER BY lnd, created_at, document_id"#,
        fn_id
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(DocumentRow {
                document_id: r.document_id.0,
                fiscal_number: r.fiscal_number,
                lnd: r.lnd,
                state: r.state.0,
                doc_type: r.doc_type.0,
                server_fiscal_no: r.server_fiscal_no,
                submission_attempted_at: r.submission_attempted_at,
                backend_profile_id: r.backend_profile_id,
                transport_profile_id: r.transport_profile_id,
                previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
                z_report_number: r.z_report_number,
                unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
                signing_inputs_pinned_at: r.signing_inputs_pinned_at,
                signed_by_cashier_id: r.signed_by_cashier_id.map(|c| c.0),
                signing_config_snapshot_id: r.signing_config_snapshot_id,
            })
        })
        .collect()
}

/// Raw row for [`list_shift_issued_receipts`] — `(doc_type, payload_json,
/// signing_config_snapshot_id)`.  The snapshot FK (nullable; NULL only for
/// pre-W4-Z2a docs) lets the Z TXS aggregation resolve each receipt's tax
/// groups with the SAME config it was signed under.
struct ShiftReceiptRow {
    doc_type: DocType,
    payload_json: String,
    signing_config_snapshot_id: Option<i64>,
}

// Manual `FromRow` (CS-1b): `doc_type` is the pure `prro_domain::DocType`,
// which is sqlx-free, so decode it through the store-side `DbDocType` wrapper
// then convert to the domain enum — keeping the struct field type unchanged.
impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for ShiftReceiptRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        use sqlx::Row;
        Ok(Self {
            doc_type: row.try_get::<DbDocType, _>("doc_type")?.0,
            payload_json: row.try_get("payload_json")?,
            signing_config_snapshot_id: row.try_get("signing_config_snapshot_id")?,
        })
    }
}

/// RS-2 piece-2b — the issued `SELL` / `RETURN` receipts of a shift, for
/// Z-report aggregation.  Returns `(doc_type, payload_json,
/// signing_config_snapshot_id)` for docs in the given `shift_id` whose state
/// is a terminal **issued receipt** (`ACK` or `OFFLINE_LOCAL_ACK` — the
/// `fiscal_documents`=issued-ledger set; in-flight `SENDING`/`KVT*`/
/// `ERROR_RETRYABLE` and `REJECTED` are intentionally excluded, matching the
/// W4-Z1 `<NC>` spec + the Python `shift_aggregation` parity).  `payload_json`
/// is the **converted** signer-ready `CheckJson` (RS-2 §0.4 H5), so callers
/// aggregate its `payments[]` + `items[]` directly; the nullable snapshot FK
/// (NULL only for pre-W4-Z2a docs) lets the W4-Z2 TXS aggregation resolve each
/// receipt's tax groups with the config it was signed under.
///
/// Runtime-bound `query_as` (NOT the `query!` macro) so it needs no
/// `.sqlx` offline-cache entry — mirrors the secure-pool repos.
pub async fn list_shift_issued_receipts(
    pool: &SqlitePool,
    fn_id: &str,
    shift_id: ShiftId,
) -> sqlx::Result<Vec<(DocType, String, Option<i64>)>> {
    let rows = sqlx::query_as::<_, ShiftReceiptRow>(
        "SELECT doc_type, payload_json, signing_config_snapshot_id \
         FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? \
           AND doc_type IN ('SELL','RETURN') \
           AND state IN ('ACK','OFFLINE_LOCAL_ACK') \
         ORDER BY lnd",
    )
    .bind(fn_id)
    .bind(DbShiftId(shift_id))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.doc_type, r.payload_json, r.signing_config_snapshot_id))
        .collect())
}

struct ShiftPendingRow {
    document_id: Vec<u8>,
    state: DocState,
}

// Manual `FromRow` (CS-1b): decode `state` via the store-side `DbDocState`
// wrapper, then convert back to the pure domain enum.
impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for ShiftPendingRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        use sqlx::Row;
        Ok(Self {
            document_id: row.try_get("document_id")?,
            state: row.try_get::<DbDocState, _>("state")?.0,
        })
    }
}

/// RS-3 A1Z quiescence — the IN-FLIGHT (non-issued, non-terminal) SELL /
/// RETURN receipts of a shift, in MAC-chain order (`ORDER BY lnd`), so the
/// Z-report quiescence pass can decide whether the shift's ledger is
/// complete before aggregating.
///
/// The "blocking pending" state set mirrors the runtime in-flight set
/// (everything between `PREPARED` and `Ack`, plus `ErrorRetryable`), and is
/// scoped to the current shift + SELL/RETURN only:
///   PREPARED, SIGNED, ENCRYPTED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE.
/// Issued (`ACK` / `OFFLINE_LOCAL_ACK`) are NOT pending — they are what the
/// aggregate counts. Terminal non-issued (`REJECTED` / `CANCELLED` /
/// `REQUIRES_MANUAL_RECONCILIATION`) are NOT pending — they neither block
/// nor count. Read-only (`fetch_all`), runtime `query_as`, NO write-tx.
pub async fn list_shift_pending_receipts_for_z_quiescence(
    pool: &SqlitePool,
    fn_id: &str,
    shift_id: ShiftId,
) -> sqlx::Result<Vec<(DocumentId, DocState)>> {
    let rows = sqlx::query_as::<_, ShiftPendingRow>(
        "SELECT document_id, state \
         FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? \
           AND doc_type IN ('SELL','RETURN','SERVICE_IN','SERVICE_OUT','CASH_ADVANCE_EPZ') \
           AND ( \
                 state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT', \
                           'KVT1','KVT2','ERROR_RETRYABLE') \
                 OR ( \
                   /* A.3 / C10 (design v3 §9.3 ruling): a post-SENT online RMR \
                      doc is ISSUED-BUT-UNCONFIRMED (lnd consumed, sfn set, seed \
                      advanced) — DPS may still hold it, so Z-quiescence MUST \
                      BLOCK on it rather than close the shift over it.  Aggregation \
                      is unchanged (counts only ACK/OFFLINE_LOCAL_ACK = confirmed). */ \
                   state = 'REQUIRES_MANUAL_RECONCILIATION' \
                   AND offline_fiscal_no IS NULL \
                   AND server_fiscal_no IS NOT NULL AND server_fiscal_no != '' \
                 ) \
               ) \
         ORDER BY lnd",
    )
    .bind(fn_id)
    .bind(DbShiftId(shift_id))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let bytes: [u8; 16] = r
                .document_id
                .try_into()
                .expect("fiscal_documents.document_id is a 16-byte BLOB");
            (DocumentId::from_bytes(bytes), r.state)
        })
        .collect())
}

/// RS-2 piece-4b — the current outcome of the receipt for a `request_id`,
/// for the ingress replay resolver.  `document_id` is rendered as
/// lowercase 32-char hex (`lower(hex(...))`) to match
/// `runtime::ingress::dto::request_id_to_string`.  Read-only
/// (`fetch_optional`), runtime `query_as` (no `.sqlx` cache), NO write-tx.
#[derive(Debug, Clone)]
pub struct TerminalOutcome {
    pub document_id: String,
    pub state: DocState,
    pub doc_type: DocType,
    pub server_fiscal_no: Option<String>,
    /// The gateway's "DPS-confirmed-at" stamp (`first_kvt1_at`, set at the
    /// `Sent → Kvt1` CAS, preserved to `Ack`).  `None` until a KVT1 lands
    /// — so it is `None` for an offline-local-ack (which never reaches
    /// KVT1).  This is the truthful fiscalization timestamp; `business_ts`
    /// (the client receipt time) and `server_fiscal_date` (never written)
    /// are NOT used.
    pub first_kvt1_at: Option<String>,
    pub total_sum_kop: Option<i64>,
}

// Manual `FromRow` (CS-1b): `state` / `doc_type` are the pure (sqlx-free)
// domain enums, decoded via the store-side `DbDocState` / `DbDocType` wrappers
// and converted back. The public field types stay the domain enums, so external
// consumers (`replay.rs`, `inbox_reaper.rs`, `is_terminally_failed`) are
// unchanged. Column-name based (matches the derived FromRow it replaces).
impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for TerminalOutcome {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        use sqlx::Row;
        Ok(Self {
            document_id: row.try_get("document_id")?,
            state: row.try_get::<DbDocState, _>("state")?.0,
            doc_type: row.try_get::<DbDocType, _>("doc_type")?.0,
            server_fiscal_no: row.try_get("server_fiscal_no")?,
            first_kvt1_at: row.try_get("first_kvt1_at")?,
            total_sum_kop: row.try_get("total_sum_kop")?,
        })
    }
}

pub async fn terminal_outcome_by_request_id(
    pool: &SqlitePool,
    request_id: &[u8; 16],
) -> sqlx::Result<Option<TerminalOutcome>> {
    sqlx::query_as::<_, TerminalOutcome>(
        "SELECT lower(hex(document_id)) AS document_id, \
                state, doc_type, server_fiscal_no, first_kvt1_at, total_sum_kop \
         FROM fiscal_documents \
         WHERE request_id = ? \
         LIMIT 1",
    )
    .bind(&request_id[..])
    .fetch_optional(pool)
    .await
}

/// RS-2 piece-6 — the FN's most recent REAL server fiscal number, for the
/// read-only status endpoint.  Candidate = ANY terminal `ACK` whose
/// `server_fiscal_no` is present AND non-empty — this includes an
/// offline-drained doc once DPS confirms it to terminal `ACK` (the filter is
/// the terminal `state='ACK'`, NOT `fs_mode='ONLINE'`).  Excluded: an
/// `OFFLINE_LOCAL_ACK` (a TERMINAL local-accepted state, but excluded here
/// because it has no DPS `server_fiscal_no` yet), and an empty
/// `server_fiscal_no` on an ACK (corrupt — `replay::build_accepted` treats it
/// as ledger drift, so this helper must not surface it as a real number).
///
/// **Ranked by `lnd` DESC** — the strictly-monotonic per-FN local fiscal
/// sequence, i.e. true issuance recency.  We deliberately do NOT rank by
/// `first_kvt1_at`: it is stamped ONLY at the `Sent → Kvt1` CAS, so a real
/// later ACK can have `first_kvt1_at = NULL` — both for a legacy pre-014
/// `KVT2`/`ACK` row (migration 014 backfilled only `state='KVT1'`) AND for any
/// offline-drain ACK that reached terminal ACK without a KVT1 stamp.  SQLite
/// sorts NULL LAST under `DESC`, so ranking by `first_kvt1_at` would let an
/// OLDER stamped ACK shadow a NEWER NULL one and surface a STALE fiscal number
/// (review HIGH).  `lnd` has no NULL hazard and is co-monotonic with issuance.
///
/// Note: an in-flight `KVT1`/`KVT2` row already carries a `server_fiscal_no`
/// (stamped at the `Sending → Sent` CAS) but is intentionally EXCLUDED until
/// it reaches terminal `ACK` — the status shows the last CONFIRMED number,
/// not one still finalizing.  Pool-bound read, NO write-tx.
pub async fn last_server_fiscal_no(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        // ordering-justified: `ux_fd_fn_lnd(fiscal_number, lnd)` is UNIQUE and this query is
        // scoped to one `fiscal_number`, so `lnd` breaks every tie — a total order.
        "SELECT server_fiscal_no FROM fiscal_documents \
         WHERE fiscal_number = ? AND state = 'ACK' \
               AND server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0 \
         ORDER BY lnd DESC \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await
}

/// PR-B (RS-4 audit pass-2, spec §B ruling 3, 2026-06-11) — the FN's newest
/// **submitted** server fiscal number: the `server_fiscal_no` of the
/// max-`lnd` doc in any state that has crossed the `Sending → Sent` CAS and
/// therefore carries a DPS-stamped wire id — `{SENT, KVT1, KVT2, ACK}`.
///
/// This is the boot tip-guard's expected DPS tip (NOT
/// [`last_server_fiscal_no`], which is ACK-only and is intentionally kept for
/// its existing read-only-status callers).  The guard's question is "is DPS's
/// last check the last check WE submitted?", not "…the last we finalized": a
/// fresh in-flight `KVT1`/`KVT2` doc (crash mid-finalize, or a boot
/// `Sent → Kvt1` Match-advance) is a *legitimate* DPS tip and must NOT trip a
/// false stale-ledger BLOCK (the original §B-1 ACK-only comparison did — fixed
/// here per the architect's locked ruling 3, variant (a)).
///
/// `RequiresManualReconciliation` is **deliberately excluded**: a doc that
/// diverged into manual triage need not be DPS's tip, and that node is already
/// under operator attention.  Ranked `lnd DESC` (strictly-monotonic issuance
/// order; no NULL hazard — see [`last_server_fiscal_no`] for why not
/// `first_kvt1_at`).  Pool-bound read, NO write-tx.
pub async fn last_submitted_server_fiscal_no(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        // ordering-justified: `ux_fd_fn_lnd(fiscal_number, lnd)` is UNIQUE and this query is
        // scoped to one `fiscal_number`, so `lnd` breaks every tie — a total order.
        "SELECT server_fiscal_no FROM fiscal_documents \
         WHERE fiscal_number = ? AND state IN ('SENT', 'KVT1', 'KVT2', 'ACK') \
               AND server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0 \
         ORDER BY lnd DESC \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await
}

/// **AUD-L4-1 (2026-06-14)** — the `state` of the FN's newest SUBMITTED doc: the
/// SAME max-`lnd` cohort (`{SENT,KVT1,KVT2,ACK}` + non-empty `server_fiscal_no`,
/// `ORDER BY lnd DESC LIMIT 1`) as [`last_submitted_server_fiscal_no`], so the two
/// return the (state, sfn) of the SAME tip doc.  The boot tip-guard uses it on a
/// `lastChk` NotFound to distinguish a non-ACK in-flight `SENT` tip (the drain's
/// HIGH-C5-3 safe re-send — Sent→ER→Pattern-B — must NOT BLOCK) from a genuine
/// ACK/KVT-tail divergence (stale ledger — must BLOCK).  `None` ⟺ empty
/// submitted tail.  Pool-bound read, no write-tx.
pub async fn newest_submitted_state(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT state FROM fiscal_documents \
         WHERE fiscal_number = ? AND state IN ('SENT', 'KVT1', 'KVT2', 'ACK') \
               AND server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0 \
         ORDER BY lnd DESC \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await
}

/// M1 review item 2(b), 2026-06-11 — the FN's highest **submitted** `lnd`: the
/// max `lnd` over docs in `{SENT, KVT1, KVT2, ACK}` carrying a non-empty
/// `server_fiscal_no` (the same "submitted tip" cohort as
/// [`last_submitted_server_fiscal_no`], returning the position instead of the
/// id).  Used by the boot SENT-recovery / online-convergence Mismatch arm to
/// tell a SUPERSEDED doc (its `lnd` < this max — a newer submitted doc became
/// DPS's tip) from a genuine tip-Mismatch (operator handoff).  `None` when no
/// submitted doc exists.  Pool-bound read, no write-tx.
pub async fn max_submitted_lnd(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(lnd) FROM fiscal_documents \
         WHERE fiscal_number = ? AND state IN ('SENT', 'KVT1', 'KVT2', 'ACK') \
               AND server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0",
    )
    .bind(fiscal_number)
    .fetch_one(pool)
    .await
}

/// **M2-N4 (architect ruling, 2026-06-13)** — the `(lnd, server_fiscal_no)` of
/// every SUBMITTED doc on `fiscal_number` with `lnd > min_lnd_exclusive`.
/// "Submitted" is the same cohort as [`max_submitted_lnd`]
/// (`state IN {SENT,KVT1,KVT2,ACK}` with a non-empty `server_fiscal_no`).
///
/// Used by the superseded decision (boot M1-02 SENT-probe + drain SentReplay):
/// a `last_chk` tip id is a LEGITIMATE supersession ONLY if it equals the
/// `server_fiscal_no` of one of OUR newer submitted docs.  A foreign / garbage
/// tip (someone else fiscalised on the FN, or DPS-state divergence) is NOT a
/// benign supersession — it is genuine structural drift, and trusting local
/// `max_lnd` alone would mask it.  Pool-bound read, no write-tx.
pub async fn submitted_above_lnd(
    pool: &SqlitePool,
    fiscal_number: &str,
    min_lnd_exclusive: i64,
) -> sqlx::Result<Vec<(i64, String)>> {
    sqlx::query_as::<_, (i64, String)>(
        "SELECT lnd, server_fiscal_no FROM fiscal_documents \
         WHERE fiscal_number = ? AND lnd > ? AND state IN ('SENT', 'KVT1', 'KVT2', 'ACK') \
               AND server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0",
    )
    .bind(fiscal_number)
    .bind(min_lnd_exclusive)
    .fetch_all(pool)
    .await
}

/// NC-04 (external-critic finding, adjudicated 2026-06-11) — the FN's NEWEST
/// pending-submitted row by `lnd`, REGARDLESS of `server_fiscal_no` validity:
/// the max-`lnd` doc in `{SENT, KVT1, KVT2}` (the in-flight submitted cohort —
/// ACK is excluded, a confirmed terminal always carries its sfn).  Returns
/// `(lnd, server_fiscal_no)` with `server_fiscal_no = None` for a NULL column;
/// `None` when no pending-submitted doc exists.
///
/// Unlike [`last_submitted_server_fiscal_no`] / [`max_submitted_lnd`], this does
/// NOT filter `sfn IS NOT NULL`, so the boot tip-guard can SEE a malformed newer
/// tail (NULL/empty sfn — unreachable in healthy code where `SENT ⇐
/// WireDecision::Sent` stamps sfn, but exactly what a restored/corrupted DB
/// carries) that the sfn-filtered queries would silently skip.  Pool-bound read,
/// no write-tx.
pub async fn newest_pending_submittable(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<(i64, Option<String>)>> {
    sqlx::query_as::<_, (i64, Option<String>)>(
        // ordering-justified: `ux_fd_fn_lnd(fiscal_number, lnd)` is UNIQUE and this query is
        // scoped to one `fiscal_number`, so `lnd` breaks every tie — a total order.
        "SELECT lnd, server_fiscal_no FROM fiscal_documents \
         WHERE fiscal_number = ? AND state IN ('SENT', 'KVT1', 'KVT2') \
         ORDER BY lnd DESC \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await
}

/// NC-03 (external-critic FT, adjudicated 2026-06-11) — the FN's highest `lnd`
/// over ALL doc states (no state / sfn filter).  `None` ⟺ NO `fiscal_documents`
/// row exists for the FN (a genuinely fresh FN).  Boot branch (a) uses this to
/// distinguish "fresh FN" (bootstrap next_lnd=1) from "node_state lost but ledger
/// survived" (reconstruct next_lnd = `MAX(lnd)+1`, since the LND allocator reads
/// `node_state.next_lnd`).  Pool-bound read, no write-tx.
pub async fn max_lnd_any_state(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(lnd) FROM fiscal_documents WHERE fiscal_number = ?",
    )
    .bind(fiscal_number)
    .fetch_one(pool)
    .await
}

/// NC-03 — the `unsigned_xml_sha256` of the FN's last ACKed doc (max `lnd` in
/// state `ACK`).  ACK-ONLY: it does NOT see an offline-origin tail.
///
/// **AUD-L6-1 (2026-06-14): do NOT use this for the boot MAC-seed projection.**
/// Since M2-01, the seed also advances at `OFFLINE_LOCAL_ACK` for offline-origin
/// docs (and `stage_finalize` SKIPS the advance for them), so when an FN's true
/// tip is an offline-origin doc with `lnd` > the last online ACK, the last ACK
/// is a STALE seed.  Boot branch (a) now uses
/// [`last_issued_unsigned_xml_sha256`] (the ever-issued tip).  This helper is
/// retained for any ACK-only consumer; `None` ⟺ no ACKed doc.  Pool-bound read.
pub async fn last_ack_unsigned_xml_sha256(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<Vec<u8>>> {
    // ordering-justified: `ux_fd_fn_lnd(fiscal_number, lnd)` is UNIQUE and this query is
    // scoped to one `fiscal_number`, so `lnd` breaks every tie — a total order.
    let row: Option<Option<Vec<u8>>> = sqlx::query_scalar::<_, Option<Vec<u8>>>(
        "SELECT unsigned_xml_sha256 FROM fiscal_documents \
         WHERE fiscal_number = ? AND state = 'ACK' \
         ORDER BY lnd DESC \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// **M2-N2b / AUD-L6-1 (architect-locked) — SINGLE SOURCE OF TRUTH** for the
/// offline-origin states that count as "issued": an offline-origin doc
/// (`offline_fiscal_no IS NOT NULL`) that EVER reached `OFFLINE_LOCAL_ACK`
/// advanced the local MAC seed (M2-01), regardless of its later drain outcome.
/// Shared by the `invariant_scan` MAC-walk issued-set (M2-N2b) AND
/// [`last_issued_unsigned_xml_sha256`] (AUD-L6-1 boot seed projection) so the two
/// CANNOT diverge — the boot projection must equal the walk's final `expected`.
/// Online-origin docs issue ONLY at `ACK` (handled separately, NOT in this set).
///
/// **CS-3 S7-1 (2026-07-21):** `SENDING` joins the set. Under the composed HOLD contract an
/// offline-origin doc whose drain wire is HELD (offline reject / submitted-unknown) rests durably
/// in `SENDING` (`PENDING_APPLY`), yet it already crossed `OFFLINE_LOCAL_ACK` and advanced the
/// seed (M2-01) — so it is STILL issued. Omitting it would break the MAC-walk (a held offline
/// predecessor would drop out of the chain → false `ChainBreak` at its successor) and the boot
/// `last_issued_unsigned_xml_sha256` projection (a held tip would be skipped).
pub const OFFLINE_ISSUED_STATES: [&str; 8] = [
    "OFFLINE_LOCAL_ACK",
    "SENDING",
    "SENT",
    "KVT1",
    "ERROR_RETRYABLE",
    "KVT2",
    "REJECTED",
    "REQUIRES_MANUAL_RECONCILIATION",
];

/// A.3 / D4 (design v3 §5) — the SINGLE issued-predicate for BOTH lanes,
/// replacing the hardcoded `state == "ACK"` online literal that used to live
/// in the walk (C2) and the SQL projections (C3).
///   - **offline-origin** (`offline_fiscal_no IS NOT NULL`): issued ⟺ the state
///     is in [`OFFLINE_ISSUED_STATES`] (ever crossed `OFFLINE_LOCAL_ACK`, M2-01).
///   - **online-origin** (`offline_fiscal_no IS NULL`): issued ⟺ `server_fiscal_no`
///     is set (non-NULL, non-empty) — the D3 discriminator.  Under A.3 the seed
///     advances atomic with the `Sending → Sent` CAS + the sfn stamp, so
///     `sfn set ⟺ seed advanced`, state-independently (immune to later terminal
///     routing: a post-SENT `RMR` keeps its sfn → stays issued; a pre-SENT
///     reject never got one → stays non-issued).
///
/// Consumers in lockstep: the `invariant_scan` MAC-walk (C2),
/// [`last_issued_unsigned_xml_sha256`] via fetch-then-filter (C3), the
/// Z-quiescence set (C10), the fuzzer model mirror (C6/C7), webcheck replay (C8).
pub fn is_issued(
    state: &str,
    offline_fiscal_no: Option<i64>,
    server_fiscal_no: Option<&str>,
) -> bool {
    if offline_fiscal_no.is_some() {
        // Offline-origin: issued once it crossed `OFFLINE_LOCAL_ACK` (M2-01).
        // `ACK` is the fully-drained terminal — NOT in the const (which lists
        // the pre-ACK drain states), so it is admitted explicitly (mirrors the
        // pre-A.3 walk's universal `state == "ACK"` clause; an offline doc that
        // drained all the way to ACK is still issued).
        state == "ACK" || OFFLINE_ISSUED_STATES.contains(&state)
    } else {
        // Online-origin: issued ⟺ `server_fiscal_no` set (D3).  An online `ACK`
        // always carries sfn, so this subsumes the old ACK-only online rule
        // AND extends it forward to SENT+ (the A.3 advance-at-SEND change).
        matches!(server_fiscal_no, Some(sfn) if !sfn.is_empty())
    }
}

/// **A.3 PR-C (D5 gate, design v3 §6 step 7)** — does an OLDER non-issued
/// sibling rest on this FN?  The D4-complement predicate:
///
///   `∃ other doc on FN` in a non-terminal in-flight state
///   {`PREPARED`, `SIGNED`, `ENCRYPTED`, `SENDING`, `ERROR_RETRYABLE`}
///   that is NOT [`is_issued`].
///
/// Signing (or minting) a successor while such a doc rests would stale the
/// successor's chain-`previous_hash` AFTER the wire call — the blocker
/// later advances the seed, so the drift-assert fires too late (past the
/// point of fiscal commitment).  The gate refuses fail-closed instead.
///
/// Transparent for ISSUED rows (online `SENT`/`KVT1`/`KVT2` carry sfn;
/// offline `OFFLINE_LOCAL_ACK` and offline-`ERROR_RETRYABLE` are in
/// [`OFFLINE_ISSUED_STATES`]) and for DEAD-terminal rows
/// (`REJECTED`/`ABORTED`/`CANCELLED`/`RMR`, not in the state set).  It
/// reuses the single [`is_issued`] predicate verbatim, so the gate cannot
/// drift from the MAC-walk / boot-seed projection.
///
/// - `self_document_id`: excluded from the scan (`None` at acquire — the
///   successor is not yet minted, so no self row exists).
/// - `self_lnd`: only rows with `lnd < self_lnd` block — the anti-deadlock
///   rule (the chain-head passes; boot re-drive runs lnd-ASC so the head
///   clears first).  `None` at acquire (no lnd yet) → ANY non-issued
///   sibling blocks (a brand-new doc is younger than every sibling).
///
/// One SELECT (INV #1): the FN / state / self / lnd filter is pushed into
/// SQL; only the `is_issued` discriminator (offline-vs-online
/// `ERROR_RETRYABLE`) runs in-Rust over the small candidate set.
pub async fn exists_blocking_non_issued_sibling_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    self_document_id: Option<DocumentId>,
    self_lnd: Option<i64>,
) -> sqlx::Result<bool> {
    let candidates: Vec<(String, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT state, offline_fiscal_no, server_fiscal_no \
         FROM fiscal_documents \
         WHERE fiscal_number = ? \
           AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','ERROR_RETRYABLE') \
           AND (? IS NULL OR document_id != ?) \
           AND (? IS NULL OR lnd < ?)",
    )
    .bind(fiscal_number)
    .bind(self_document_id.map(DbDocumentId))
    .bind(self_document_id.map(DbDocumentId))
    .bind(self_lnd)
    .bind(self_lnd)
    .fetch_all(&mut **tx)
    .await?;
    Ok(candidates
        .iter()
        .any(|(state, ofn, sfn)| !is_issued(state, *ofn, sfn.as_deref())))
}

/// A.3 PR-C — is this doc ONLINE-origin (`fs_mode = 'ONLINE'`)?  The D5 gate is
/// an ONLINE-lane concern: an offline mint must NEVER be gated (offline
/// availability is unconditional; the runtime resolver is online-scoped).  At
/// sign time the origin is read back from the persisted `fs_mode` (the
/// WorkerContext does not carry it); the acquire layer uses the live `channel`
/// instead.  A missing row (already-terminalised / raced) reads as NOT online
/// → the gate does not fire (fail-open on the origin read, never fail-closed on
/// a phantom).
pub async fn is_online_origin_tx(
    tx: &mut WriteTxConn<'_>,
    document_id: DocumentId,
) -> sqlx::Result<bool> {
    let fs_mode: Option<String> =
        sqlx::query_scalar("SELECT fs_mode FROM fiscal_documents WHERE document_id = ?")
            .bind(DbDocumentId(document_id))
            .fetch_optional(&mut **tx)
            .await?;
    Ok(matches!(fs_mode.as_deref(), Some("ONLINE")))
}

/// **AUD-L6-1 (FT, 2026-06-14)** — the `unsigned_xml_sha256` of the FN's
/// highest-`lnd` EVER-ISSUED doc: online `ACK` OR offline-origin in any
/// [`OFFLINE_ISSUED_STATES`] state (`offline_fiscal_no IS NOT NULL`).  This is
/// the value the live flow leaves in `node_state.last_known_unsigned_xml_sha256`
/// once M2-01 advances the seed at OFFLINE_LOCAL_ACK — so boot branch (a) must
/// project the MAC seed from THIS (not the ACK-only [`last_ack_unsigned_xml_sha256`],
/// which is stale when the tail is offline-origin).  Result equals the
/// `invariant_scan` MAC-walk's final `expected` by construction (same issued
/// predicate + max-lnd).  `None` ⟺ no issued doc (genesis).  Pool-bound read.
pub async fn last_issued_unsigned_xml_sha256<'e, E>(
    executor: E,
    fiscal_number: &str,
) -> sqlx::Result<Option<Vec<u8>>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    // A.3 / D4 — FETCH-THEN-FILTER: a SQL string cannot host the Rust
    // [`is_issued`] predicate (the online arm keys on `server_fiscal_no`, not a
    // state literal), so fetch candidate rows in descending `lnd` order and
    // take the FIRST `is_issued` one in Rust — "by construction" beats "by
    // convention".  The `unsigned_xml_sha256 IS NOT NULL` filter stays in SQL;
    // the issued decision is the shared fn, so this projection and the
    // `invariant_scan` walk CANNOT diverge.  (The old hardcoded `'ACK'` literal
    // + the `OFFLINE_ISSUED_STATES` IN-clause are gone.)
    #[derive(sqlx::FromRow)]
    struct Cand {
        unsigned_xml_sha256: Option<Vec<u8>>,
        state: String,
        offline_fiscal_no: Option<i64>,
        server_fiscal_no: Option<String>,
    }
    let rows: Vec<Cand> = sqlx::query_as(
        "SELECT unsigned_xml_sha256, state, offline_fiscal_no, server_fiscal_no \
         FROM fiscal_documents \
         WHERE fiscal_number = ? AND unsigned_xml_sha256 IS NOT NULL \
         ORDER BY lnd DESC",
    )
    .bind(fiscal_number)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .find(|r| is_issued(&r.state, r.offline_fiscal_no, r.server_fiscal_no.as_deref()))
        .and_then(|r| r.unsigned_xml_sha256))
}

/// **Active MAC-chain tip** — the seed the live chain currently rests at, which is NOT always the
/// last issued document's hash (bd PRRO_GATE-2nk). This is the projection **recovery + the MacReseed
/// guard-B** must use; [`last_issued_unsigned_xml_sha256`] keeps its honest "last issued doc"
/// semantics and is a lower-level primitive.
///
/// The only operation that moves the tip BELOW the last issued doc is a `NotAcceptedOffline`
/// completion: it rewinds `node_state`'s seed to the held doc's own immutable `previous_hash` and
/// marks that doc `chain_superseded_at` (delivery_reservation.rs). Crucially, that rewind target may
/// be a **non-document seed** — e.g. a T=112 code-replenish sets the seed to `sha256(request_xml)`
/// with NO producing fiscal document (offline_code_replenish.rs) — so it CANNOT be recovered by
/// "the previous issued doc's hash". It IS, however, stored verbatim as the superseded doc's
/// `previous_hash`, so we read it DIRECTLY.
///
/// Walk newest-first (`lnd DESC`), skipping dead cohort docs (`CANCELLED`/`ABORTED` — their
/// back-pointers are voided-sub-chain artifacts):
///   - first `chain_superseded_at` doc → its `previous_hash` (the exact rewind target, incl. genesis
///     `NULL` and non-doc seeds);
///   - else first `is_issued` doc → its `unsigned_xml_sha256` (a normal issuance AFTER any rewind
///     overrides the older marker, since it re-advanced the seed);
///   - else genesis (`None`).
///
/// NOTE (scope, bd PRRO_GATE-mcc + 2nk + hpc): this recovers the tip when the last seed-changing
/// event left a trace EITHER in the ledger (an issued doc, or a rewind captured on a superseded
/// doc's `previous_hash`) OR in the durable `chain_seed_transitions` witness (bd PRRO_GATE-hpc —
/// the standalone T=112 replenish advances the seed to a NON-DOCUMENT value `Hs = sha256(request_xml)`
/// with no producing fiscal document; the witness records it in the same per-FN `lnd` frame).  The
/// doc-walk and the witness are reconciled by the §4.2 ordering rule below (strict `>` tie-break).
/// All three seed consumers (NC-03 boot, MacReseed guard-B, invariant_scan) inherit both fixes
/// through this ONE projection, exactly as bd 2nk folded in `chain_superseded_at`.
pub async fn active_chain_tip_unsigned_xml_sha256<'e, E>(
    executor: E,
    fiscal_number: &str,
) -> sqlx::Result<Option<Vec<u8>>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    // hpc: the projection now folds TWO sources (the doc walk + the durable T=112 witness).  It runs
    // as ONE query so the public `E: Executor` signature — and all three call sites (boot `pool`,
    // guard-B `&mut **tx`, scan `pool`) — stay byte-for-byte untouched (`E` is consumed once by a
    // single `fetch_all`).  The query UNION-ALLs the fiscal_documents candidate rows with the single
    // newest `chain_seed_transitions` witness, tagged by a `src` discriminator; the Rust below folds
    // them via the §4.2 ordering rule.
    #[derive(sqlx::FromRow)]
    struct Row {
        src: String,
        ord: i64,
        state: Option<String>,
        previous_hash: Option<Vec<u8>>,
        unsigned_xml_sha256: Option<Vec<u8>>,
        offline_fiscal_no: Option<i64>,
        server_fiscal_no: Option<String>,
        chain_superseded_at: Option<String>,
        new_seed: Option<Vec<u8>>,
    }
    // Doc rows carry (src='DOC', ord=lnd, the raw columns the walk needs).  The witness row carries
    // (src='WIT', ord=lnd_at_write, new_seed).  A `sort` key ('DOC'/'WIT') is only cosmetic — the
    // Rust fold picks the doc-tip and the single witness independently.
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT 'DOC' AS src, lnd AS ord, state, previous_hash, unsigned_xml_sha256, \
                offline_fiscal_no, server_fiscal_no, chain_superseded_at, NULL AS new_seed \
           FROM fiscal_documents \
          WHERE fiscal_number = ?1 AND unsigned_xml_sha256 IS NOT NULL \
         UNION ALL \
         SELECT src, ord, state, previous_hash, unsigned_xml_sha256, offline_fiscal_no, \
                server_fiscal_no, chain_superseded_at, new_seed FROM ( \
             SELECT 'WIT' AS src, lnd_at_write AS ord, NULL AS state, NULL AS previous_hash, \
                    NULL AS unsigned_xml_sha256, NULL AS offline_fiscal_no, \
                    NULL AS server_fiscal_no, NULL AS chain_superseded_at, new_seed \
               FROM chain_seed_transitions \
              WHERE fiscal_number = ?1 \
              ORDER BY lnd_at_write DESC, created_at DESC, rowid DESC \
              LIMIT 1) \
         ORDER BY ord DESC",
    )
    .bind(fiscal_number)
    .fetch_all(executor)
    .await?;

    // Doc-derived tip AND the ordinal of the doc that produced it (lnd of the superseded doc for a
    // rewind marker, else lnd of the first issued doc).  Walk newest-first over the DOC rows only.
    let mut doc_tip: Option<(Option<Vec<u8>>, i64)> = None;
    let mut witness: Option<(Vec<u8>, i64)> = None;
    for r in rows {
        if r.src == "WIT" {
            // Only ever one witness row (LIMIT 1 in the sub-select).  Newest T=112 seed.
            if let Some(seed) = r.new_seed {
                witness = Some((seed, r.ord));
            }
            continue;
        }
        if doc_tip.is_some() {
            continue; // doc-tip already resolved; remaining DOC rows are older links
        }
        let state = r.state.unwrap_or_default();
        if state == "CANCELLED" || state == "ABORTED" {
            continue; // dead cohort — not a chain link
        }
        if r.chain_superseded_at.is_some() {
            // The last seed-changing event was this rewind → its previous_hash IS the durable tip.
            doc_tip = Some((r.previous_hash, r.ord));
            continue;
        }
        if is_issued(&state, r.offline_fiscal_no, r.server_fiscal_no.as_deref()) {
            // A normal issuance (newer than any marker) re-advanced the seed to its own hash.
            doc_tip = Some((r.unsigned_xml_sha256, r.ord));
            continue;
        }
        // A non-issued, non-dead doc (PREPARED/SIGNED/ENCRYPTED — a crash artifact) did not advance
        // the seed; keep walking to the last seed-changing doc below it.
    }

    // §4.2 selection rule.  `lnd_at_write` (witness) and `ordinal` (doc) share the SAME
    // strictly-monotonic per-FN frame (single-writer invariant #2), so:
    //   - a witness with lnd_at_write = k is NEWER than every doc with lnd < k → witness wins;
    //   - a doc that consumed lnd = k (issued AT/AFTER the witness reserved k) is NEWER → doc wins.
    // STRICT `>` on the witness side encodes the load-bearing tie-break: at equal ordinal the DOC
    // wins (it consumed the ordinal the witness merely reserved — the after-SELL case).
    match (doc_tip, witness) {
        (None, None) => Ok(None),                         // genesis
        (Some((seed, _ordinal)), None) => Ok(seed), // pure-doc chain (incl. genesis rewind None)
        (None, Some((w_seed, _lnd))) => Ok(Some(w_seed)), // replenish before any seed-changing doc
        (Some((d_seed, d_ordinal)), Some((w_seed, w_lnd))) => {
            if w_lnd > d_ordinal {
                Ok(Some(w_seed)) // witness strictly newer than the producing doc
            } else {
                Ok(d_seed) // doc chained at/past the witness ordinal → doc wins
            }
        }
    }
}

/// M3b W9b §3.1 + spec amendment 2026-05-21 (HIGH-C4-1 / HIGH-C4-8
/// resolution; HIGH-C5-1 session scoping; MED-C5-4 KVT2 deferral
/// reversed by **M3b W12 Commit 3**) — strict `lnd ASC` walker for
/// the unfinished drain cohort, scoped to a specific offline
/// session.  Returns docs in `state IN ('OFFLINE_LOCAL_ACK','SENT',
/// 'KVT1','ERROR_RETRYABLE','KVT2')` AND `offline_session_id = ?`
/// AND `fs_mode = 'OFFLINE'` for the FN, ordered by MAC chain
/// position.
///
/// **KVT2 included (W12 Commit 3, reverses MED-C5-4):** mid-tick
/// crash between Envelope 1 (W12 Kvt1→Kvt2 advance) and Envelope 2
/// (`stage_finalize::run` Kvt2→Ack) leaves the doc in `Kvt2`.
/// W9b's cohort previously deferred KVT2 to W12 PR because pre-W12
/// drain had no clean discharge path; now `process_via_w12_kvt2_advance`
/// in `backlog_drain` invokes `stage_finalize::run` (idempotent under
/// M3a `AlreadyAcked` contract) so the same drain tick converges to
/// `Ack` without waiting for boot.
///
/// **Cohort rationale (operator-pinned 2026-05-21):**
/// - `OFFLINE_LOCAL_ACK` — primary backlog (offline-acked docs awaiting
///   wire send via Pattern C drain).
/// - `SENT` — crashed-mid-drain rediscovery: doc went OFFLINE_LOCAL_ACK
///   → Sending → Sent before the orchestrator crashed; next drain
///   tick rediscovers via `lastChk` pre-flight (spec §6 I4 idempotency).
/// - `KVT1` — post-wire-send, awaiting W12 KVT2 confirmation.
/// - `ERROR_RETRYABLE` — drain produced TransientRetry / ProbeRequired
///   on previous tick; current tick re-drives via W9a 4-pre source
///   whitelist.  Without this state in the cohort, transient-class
///   failures would strand pending-drain shifts forever (HIGH-C4-8
///   operator finding).
///
/// **KVT2 deferred to W12 PR (MED-C5-4)**: pre-W12 a KVT2 doc has
/// `data_sign` evidence already persisted (boot_phase's
/// `advance_kvt1_to_kvt2_from_probe` is the M3a path) and the proper
/// advance is `Kvt2 → Ack` via `stage_finalize::run`.  Pre-W12 drain
/// has no clean way to discharge KVT2 — counting them as
/// `advanced_to_kvt1` would mis-audit; advancing to Ack would
/// violate the operator-pinned "drain cannot finalize without real
/// Ack proof" invariant.  W12 PR re-adds KVT2 to the cohort with
/// the Kvt2 → Ack path.
///
/// **Session scoping (HIGH-C5-1)**: filter by `offline_session_id =
/// active_session_id` AND `fs_mode = 'OFFLINE'` so the widened
/// cohort cannot accidentally capture online docs of the same FN
/// (online SENT/KVT1/ERROR_RETRYABLE docs have
/// `offline_session_id = NULL` and are M3a `boot_phase` territory).
///
/// **Why `lnd` is authoritative**: `lnd` is the Local Numerator of
/// Document — strictly monotonic per FN (W7a `acquire_code_tx` +
/// `transition_to_offline_local_ack_tx` enforce this atomically).
/// `created_at` is second-granular in SQLite (`CURRENT_TIMESTAMP`),
/// so multi-doc bursts can share the same timestamp; `lnd` is the
/// only stable chain-recovery key.  `document_id` is the final
/// tiebreaker (random UUID; never matches across docs).
///
/// **Why include MAC-chain-pinned fields?** Caller (drain_orchestrator)
/// needs `server_fiscal_no` to decide between the lastChk pre-flight
/// short-circuit (replay) and the full wire-send (pure-offline) path.
/// Read in the same SELECT — single round-trip vs N+1 reads per doc.
pub async fn list_drain_candidates_for_fn_ordered_by_lnd(
    pool: &SqlitePool,
    fn_id: &str,
    session_id: OfflineSessionId,
) -> sqlx::Result<Vec<DocumentRow>> {
    let db_session_id = DbOfflineSessionId(session_id);
    let rows = sqlx::query!(
        r#"SELECT document_id    as "document_id: DbDocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DbDocState",
                  doc_type        as "doc_type: DbDocType",
                  server_fiscal_no,
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at,
                  signed_by_cashier_id  as "signed_by_cashier_id: DbCashierId",
                  signing_config_snapshot_id
           FROM fiscal_documents
           WHERE fiscal_number = ?
             AND offline_session_id = ?
             AND fs_mode = 'OFFLINE'
             AND state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')
           ORDER BY lnd, created_at, document_id"#,
        fn_id,
        db_session_id,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(DocumentRow {
                document_id: r.document_id.0,
                fiscal_number: r.fiscal_number,
                lnd: r.lnd,
                state: r.state.0,
                doc_type: r.doc_type.0,
                server_fiscal_no: r.server_fiscal_no,
                submission_attempted_at: r.submission_attempted_at,
                backend_profile_id: r.backend_profile_id,
                transport_profile_id: r.transport_profile_id,
                previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
                z_report_number: r.z_report_number,
                unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
                signing_inputs_pinned_at: r.signing_inputs_pinned_at,
                signed_by_cashier_id: r.signed_by_cashier_id.map(|c| c.0),
                signing_config_snapshot_id: r.signing_config_snapshot_id,
            })
        })
        .collect()
}

/// **M3b W12 Commit 3 Δ** (MED-W12C3-01 fix, 2026-05-22) —
/// drain-finalize crash-recovery predicate.
///
/// Returns `true` iff the given offline session has at least one
/// `fiscal_documents` row scoped to it (`offline_session_id = ?`,
/// `fs_mode = 'OFFLINE'`) **AND** ALL such rows are in terminal
/// `ACK` state.  Drain orchestrator's empty-cohort branch uses this
/// to detect the post-Commit-3 crash window: prior tick advanced
/// the last KVT2 doc to ACK via `stage_finalize::run` (durably
/// committed), but the process crashed before reaching
/// `finalize_drain`, leaving node/session/shift state stranded.
///
/// **Conservative scope (operator-pinned 2026-05-22)**: only `ACK`
/// counts toward "completable".  `REJECTED` / `CANCELLED` /
/// `REQUIRES_MANUAL_RECONCILIATION` deliberately do NOT make the
/// session auto-finalizable — those terminal states require
/// explicit operator treatment (Manual recon or future W12 explicit
/// branch) before session closure is safe.  The conservative shape
/// matches the existing drain success contract:
/// `finalize_eligibility` only goes Eligible when
/// `advanced_to_ack == backlog_size_before` — no other terminal
/// state counts.
///
/// **Returns false** when:
/// - session has 0 docs (genuinely empty session; not a crash
///   recovery case) → drain skips normally;
/// - session has at least one non-ACK doc (Sent / Kvt1 / ER /
///   Rejected / Manual / etc.) → drain skips, normal Eligibility
///   check on next tick.
///
/// **Crash-recovery liveness invariant**: caller must guard with
/// `session_state == Draining` BEFORE invoking this predicate.
/// Session in `Open` with all-ACK docs is a structural drift
/// (drain Open→Draining mid-pass transition would have happened
/// before any doc reached ACK) and should not auto-finalize.
pub async fn is_session_drain_completable(
    pool: &SqlitePool,
    session_id: OfflineSessionId,
) -> sqlx::Result<bool> {
    // `query!` binds params by reference, so the `Db*` wrapper needs a named
    // binding to outlive the macro-generated borrow (CS-1b′).
    let db_session_id = DbOfflineSessionId(session_id);
    let row = sqlx::query!(
        r#"SELECT
              COUNT(*)                                       AS "total!: i64",
              SUM(CASE WHEN state = 'ACK' THEN 1 ELSE 0 END) AS "ack_count: i64"
           FROM fiscal_documents
           WHERE offline_session_id = ?
             AND fs_mode = 'OFFLINE'"#,
        db_session_id,
    )
    .fetch_one(pool)
    .await?;
    let total = row.total;
    let ack_count = row.ack_count.unwrap_or(0);
    Ok(total > 0 && total == ack_count)
}

/// W5 / W0-1 §3.1 stage 1 — resume-detect lookup by `request_id`
/// inside the worker's `with_immediate` envelope.
///
/// **Filtered to pending (resumable) states only**: PREPARED, SIGNED,
/// ENCRYPTED, SENDING, SENT, KVT1, KVT2, ERROR_RETRYABLE.  Mirrors
/// the pending list of `list_pending_for_fn`.  Terminal-success
/// (ACK), terminal-fail (REJECTED, CANCELLED, REQUIRES_MANUAL_
/// RECONCILIATION), and offline-handoff (OFFLINE_LOCAL_ACK) rows
/// MUST NOT be returned — they would otherwise drive
/// `WorkerProcessResult::Resumed` for a document whose flow is
/// already concluded.  Terminal coexistence with a `NEW` inbox row
/// for the same `request_id` is detected separately by
/// [`exists_terminal_by_request_id_tx`] and surfaces as a guard
/// rejection; this method's job is strictly the resume path.
pub async fn get_pending_by_request_id_tx(
    tx: &mut WriteTxConn<'_>,
    request_id: &RequestId,
) -> sqlx::Result<Option<DocumentRow>> {
    let db_request_id = DbRequestId(*request_id);
    let row = sqlx::query!(
        r#"SELECT document_id    as "document_id: DbDocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DbDocState",
                  doc_type        as "doc_type: DbDocType",
                  server_fiscal_no,
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at,
                  signed_by_cashier_id  as "signed_by_cashier_id: DbCashierId",
                  signing_config_snapshot_id
           FROM fiscal_documents
           WHERE request_id = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')"#,
        db_request_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(DocumentRow {
        document_id: r.document_id.0,
        fiscal_number: r.fiscal_number,
        lnd: r.lnd,
        state: r.state.0,
        doc_type: r.doc_type.0,
        server_fiscal_no: r.server_fiscal_no,
        submission_attempted_at: r.submission_attempted_at,
        backend_profile_id: r.backend_profile_id,
        transport_profile_id: r.transport_profile_id,
        previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
        z_report_number: r.z_report_number,
        unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
        signing_inputs_pinned_at: r.signing_inputs_pinned_at,
        signed_by_cashier_id: r.signed_by_cashier_id.map(|c| c.0),
        signing_config_snapshot_id: r.signing_config_snapshot_id,
    }))
}

/// W4-Z2a piece 15 (review round 2 M2 — Resume reload hoist) —
/// pool-bound minimal-info peek of a pending fiscal_documents row
/// by `request_id`.  Returns `(document_id, signing_config_
/// snapshot_id)` if a pending row exists.  Same state filter as
/// [`get_pending_by_request_id_tx`].
///
/// **Use**: stage_acquire pre-tx peek to pre-load the persisted
/// snapshot via `signing_config_snapshots::get_by_id` BEFORE
/// opening the main `with_immediate` envelope.  Locked rule #9
/// + INV-1 spirit: keep the write tx short, fail-loud reload
///   failure via a separately-committed audit (pattern from
///   `boot_phase::run_for_doc_prepared` piece 13 fix).
///
/// **Race semantics**: pool read happens before the lease CAS.
/// Between this peek and the inside-tx `get_pending_by_request_
/// id_tx`, the doc may transition to terminal (boot recon, MAC
/// recovery, admin SQL).  Caller treats peek result as advisory:
/// if tx-time check finds no pending row, the pre-loaded snapshot
/// is silently discarded.  Tx-time check is authoritative.
pub async fn peek_pending_doc_id_and_snapshot_id_by_request_id(
    pool: &SqlitePool,
    request_id: &RequestId,
) -> sqlx::Result<Option<(DocumentId, Option<i64>)>> {
    let db_request_id = DbRequestId(*request_id);
    let row = sqlx::query!(
        r#"SELECT document_id as "document_id: DbDocumentId",
                  signing_config_snapshot_id
           FROM fiscal_documents
           WHERE request_id = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')"#,
        db_request_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.document_id.0, r.signing_config_snapshot_id)))
}

/// W5 / W0-1 §3.1 stage 1 — companion to
/// [`get_pending_by_request_id_tx`].  Returns `true` when a
/// fiscal_documents row with the given `request_id` exists in a
/// terminal state: ACK, REJECTED, CANCELLED, OFFLINE_LOCAL_ACK,
/// REQUIRES_MANUAL_RECONCILIATION.
///
/// Stage 1 calls this AFTER a clean pending miss to detect the
/// "terminal-doc + NEW-inbox-for-same-request_id" invariant breach
/// — coexistence here means a previous run reached terminal but a
/// fresh ingress row was admitted under the same `request_id`,
/// which the worker MUST refuse rather than INSERT a duplicate
/// PREPARED.
pub async fn exists_terminal_by_request_id_tx(
    tx: &mut WriteTxConn<'_>,
    request_id: &RequestId,
) -> sqlx::Result<bool> {
    let row: Option<i64> = sqlx::query_scalar(
        r#"SELECT 1 FROM fiscal_documents
            WHERE request_id = ?
              AND state IN ('ACK','REJECTED','CANCELLED','OFFLINE_LOCAL_ACK','REQUIRES_MANUAL_RECONCILIATION','ABORTED')
            LIMIT 1"#,
    )
    .bind(DbRequestId(*request_id))
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.is_some())
}

/// W5 / W0-1 §3.1 stage 1 — same INSERT as [`insert_prepared`] but
/// driven from inside a `with_immediate` envelope through
/// `&mut WriteTxConn<'_>`.  The pool-version is preserved for
/// pre-W5 call sites (admin / migration tooling); W5 stage 1 uses
/// this one so the lease CAS, lnd allocate, INSERT PREPARED, and
/// audit append all run on the same connection inside the same
/// BEGIN IMMEDIATE transaction.
pub async fn insert_prepared_tx(tx: &mut WriteTxConn<'_>, n: &NewDocument) -> sqlx::Result<()> {
    sqlx::query(
        r#"INSERT INTO fiscal_documents (
             document_id, request_id, fiscal_number, shift_id, offline_session_id,
             lnd, doc_type, state, backend_profile_id, transport_profile_id,
             fs_mode, business_ts, total_sum_kop, payload_json,
             payload_sha256_canonical, source_sha256, unsigned_xml_sha256, previous_hash,
             signed_by_cashier_id, signing_config_snapshot_id
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 'PREPARED', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(DbDocumentId(n.document_id))
    .bind(DbRequestId(n.request_id))
    .bind(&n.fiscal_number)
    .bind(n.shift_id.map(DbShiftId))
    .bind(n.offline_session_id.map(DbOfflineSessionId))
    .bind(n.lnd)
    .bind(n.doc_type.as_str())
    .bind(&n.backend_profile_id)
    .bind(&n.transport_profile_id)
    .bind(n.fs_mode)
    .bind(&n.business_ts)
    .bind(n.total_sum_kop)
    .bind(&n.payload_json)
    .bind(&n.payload_sha256_canonical[..])
    .bind(&n.source_sha256[..])
    .bind(n.unsigned_xml_sha256.as_ref().map(|b| &b[..]))
    .bind(n.previous_hash.as_ref().map(|b| &b[..]))
    .bind(n.signed_by_cashier_id.as_ref().map(|c| c.as_str()))
    .bind(n.signing_config_snapshot_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// W6 stage 3-PRE — pin status + state snapshot of a doc, atomic with
/// the rest of the 3-PRE write tx.  Returns `None` if the row is
/// missing.  `state` is included so the caller can early-fail with
/// `SignError::StateConflict` BEFORE pin or Z allocation, preventing
/// stale-WorkerContext from advancing `next_z_report_number` for a doc
/// whose flow already concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedSigningInputs {
    pub state: DocState,
    /// `signing_inputs_pinned_at IS NOT NULL` — disambiguates "not
    /// pinned yet" from "pinned with empty/None previous_hash"
    /// (genuine first-doc-after-bootstrap edge case).
    pub is_pinned: bool,
    pub previous_hash: Option<[u8; 32]>,
    pub z_report_number: Option<i64>,
    /// W4-Z2a piece 4 — pinned `signing_config_snapshots.id` FK.
    /// `None` for two semantically distinct cases:
    ///   1. Pre-W4-Z2a doc (migration column added as nullable) —
    ///      app-layer rule: NULL + no tax_group_1 items → ALLOW with
    ///      info audit; NULL + ANY tax_group_1 item →
    ///      RequiresManualReconciliation.
    ///   2. Doc not yet pinned (`is_pinned == false`).
    ///      Disambiguate with `is_pinned`: pinned=true + None = case 1;
    ///      pinned=false = pin hasn't happened yet (any path).
    pub signing_config_snapshot_id: Option<i64>,
}

/// W6 stage 3-PRE — read state + pin status atomically inside the
/// `with_immediate` envelope.  See [`PinnedSigningInputs`] for shape.
pub async fn get_signing_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<Option<PinnedSigningInputs>> {
    let db_doc_id = DbDocumentId(doc_id);
    let row = sqlx::query!(
        r#"SELECT state                       as "state: DbDocState",
                  previous_hash               as "previous_hash: Vec<u8>",
                  z_report_number,
                  signing_inputs_pinned_at,
                  signing_config_snapshot_id
           FROM fiscal_documents WHERE document_id = ?"#,
        db_doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(PinnedSigningInputs {
        state: r.state.0,
        is_pinned: r.signing_inputs_pinned_at.is_some(),
        previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
        z_report_number: r.z_report_number,
        signing_config_snapshot_id: r.signing_config_snapshot_id,
    }))
}

/// W6 stage 3-PRE — pin signing inputs onto the doc atomically.  One
/// UPDATE writes `previous_hash`, `z_report_number`, AND
/// `signing_inputs_pinned_at = CURRENT_TIMESTAMP`.
///
/// **Pin-once + state-gate guard**: WHERE-guarded on
/// `state = 'PREPARED' AND signing_inputs_pinned_at IS NULL`.  Returns
/// `rows_affected` so the caller can distinguish:
/// - `1` — pin happened (this caller is authoritative)
/// - `0` — either state moved (concurrent finalize/reject) OR row
///   was already pinned by an earlier 3-PRE re-entry; caller already
///   has [`get_signing_inputs_tx`] read in the same tx and acts on
///   that truth (re-fetch + StateConflict OR reuse-branch).
///
/// Idempotent under concurrent re-entry: a second pin attempt finds
/// `signing_inputs_pinned_at IS NOT NULL`, UPDATE matches 0 rows,
/// caller falls through to reuse.
pub async fn pin_signing_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    previous_hash: Option<&[u8; 32]>,
    z_report_number: Option<i64>,
    signing_config_snapshot_id: Option<i64>,
) -> sqlx::Result<u64> {
    // W4-Z2a piece 8a (CRIT-1 close per `PRRO_GATE-vax` +
    // spec rule #14): UPDATE preserves INSERT-set FK via
    // `COALESCE(existing, ?)` AND a WHERE-guard rejects the
    // structural-conflict case where caller's `Some(Y)` differs
    // from existing `Some(X)`.  Combined truth-table:
    //   existing Some(X) + caller None     → keeps X       (rows=1, no wipe)
    //   existing Some(X) + caller Some(X)  → keeps X       (rows=1, idempotent match)
    //   existing Some(X) + caller Some(Y)  → guard rejects (rows=0, SnapshotIdMismatch)
    //   existing NULL    + caller Some(Y)  → Y             (rows=1, legacy backfill)
    //   existing NULL    + caller None     → NULL          (rows=1, unchanged)
    //
    // Disambiguation pattern (per ADR-M3-A4 / W0-2 §4.4): on
    // rows_affected=0, caller (stage_sign 3-PRE) re-reads via
    // `get_signing_inputs_tx` inside the same `with_immediate`
    // envelope.  SQLite RESERVED-lock guarantees visibility.
    // Caller compares persisted FK against its `caller_snapshot_id`
    // to distinguish StateConflict / Reused / RowMissing from
    // SnapshotIdMismatch (the CRIT-1 fail-loud case).
    let res = sqlx::query(
        "UPDATE fiscal_documents \
         SET previous_hash              = ?, \
             z_report_number            = ?, \
             signing_config_snapshot_id = COALESCE(signing_config_snapshot_id, ?), \
             signing_inputs_pinned_at   = CURRENT_TIMESTAMP \
         WHERE document_id = ? \
           AND state = 'PREPARED' \
           AND signing_inputs_pinned_at IS NULL \
           AND ( \
                 signing_config_snapshot_id IS NULL \
                 OR ? IS NULL \
                 OR signing_config_snapshot_id = ? \
               )",
    )
    .bind(previous_hash.map(|h| &h[..]))
    .bind(z_report_number)
    .bind(signing_config_snapshot_id)
    .bind(DbDocumentId(doc_id))
    .bind(signing_config_snapshot_id)
    .bind(signing_config_snapshot_id)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected())
}

/// W6 stage 3-PERSIST — UPDATE `unsigned_xml_sha256` from NULL to the
/// canonical sha256 of the unsigned XML.  W5 INSERT PREPARED leaves
/// it NULL; this is the canonical write site.  Returns `true` if the
/// row exists and was updated.
pub async fn update_unsigned_xml_sha256_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    hash: &[u8; 32],
) -> sqlx::Result<bool> {
    let res =
        sqlx::query("UPDATE fiscal_documents SET unsigned_xml_sha256 = ? WHERE document_id = ?")
            .bind(&hash[..])
            .bind(DbDocumentId(doc_id))
            .execute(&mut **tx)
            .await?;
    Ok(res.rows_affected() == 1)
}

/// W7 stage 4 — minimal field set the send stage needs to construct a
/// `CheckEnvelope` and a `transport_trace::NewAttempt`.  Fields are a
/// strict subset of [`DocumentRow`]: only what stage 4 reads.  No
/// unsigned-xml hash, no Z-allocation seed, no offline session id —
/// those are 4-pre's not-our-concern (`document_files::SignedXml` is
/// read separately).  `id_cancel` is future cancel territory and
/// stays empty in W7.
///
/// **M3b W9a (2026-05-16):** `offline_fiscal_no` is now read here so
/// the W9 backlog drain (which pushes `OfflineLocalAck → Sending → …`
/// through this same stage) can populate `CheckEnvelope.id_offline`
/// with the offline-acquired fiscal-no per DPS wire contract
/// (`docs/superpowers/specs/2026-05-04-m2-w0-1-dps-wire.md:116`).  For
/// `Signed` / `ErrorRetryable` online docs the column is `NULL` and
/// `id_offline` stays empty; for `OfflineLocalAck` docs the W7a
/// `transition_to_offline_local_ack_tx` invariant guarantees it is
/// set to the consumed `code_lnd`.
#[derive(Debug, Clone)]
pub struct SendInputs {
    /// Pre-CAS observed state.  Stage 4-pre reads this BEFORE the
    /// `transition_state(Signed, Sending)` CAS so that, on a CAS miss
    /// (`TransitionOutcome::Conflict`), the worker can surface the
    /// actual current state to the dispatch layer (e.g. doc already
    /// `Sent` from a prior worker).
    pub state: DocState,
    /// Maps to `CheckEnvelope.rro_fn`.
    pub fiscal_number: String,
    /// Maps to `CheckEnvelope.local_number` for SELL/RETURN/Z_REPORT/
    /// SHIFT_CLOSE.  `WireArtifactKind::ShiftOpen` overrides this to 0
    /// inside the envelope builder per the proven Sprint 7 contract
    /// (see `dps_fiscal_server.py:190`); raw `lnd` is still surfaced
    /// here so the override site is the single source of truth.
    pub lnd: i64,
    /// Drives [`derive_wire_artifact_kind`] in stage_sign / stage_send;
    /// `DpsCheckType` is then derived in the envelope builder.
    pub doc_type: DocType,
    /// ISO-8601 business timestamp; envelope builder converts to the
    /// DPS Kyiv-local-as-epoch shape (`CheckEnvelope.date_time`).
    pub business_ts: String,
    /// Persisted profile bindings — snapshot from the doc, NOT
    /// re-resolved against `node_state` (mirrors W5 resume semantics).
    /// Pass-through to `transport_trace::NewAttempt`.
    pub backend_profile_id: String,
    pub transport_profile_id: String,
    /// **M3b W9a.**  `offline_fiscal_no` column (W7a writes this =
    /// consumed `code_lnd` when staging `Signed → OfflineLocalAck`).
    /// `None` for docs never staged offline (M3a online happy path,
    /// also ErrorRetryable retries of M3a online docs).  `Some(n)`
    /// is required when `state == OfflineLocalAck` (W7a invariant);
    /// `build_send_envelope` enforces this with a typed error before
    /// any CAS attempt.
    pub offline_fiscal_no: Option<i64>,
    /// W14a-2b §2.2 — document_id surfaced for signer_guard's
    /// `SignerCashierMismatch::*` outcomes (Commit 3); needed for
    /// audit row attribution.
    pub document_id: DocumentId,
    /// W14a-2b §2.2 — shift_id (Option because system-level docs may
    /// have no shift binding, e.g. SHIFT_OPEN itself).  Drives
    /// signer_guard's "non-close fiscal doc must have a resolvable
    /// shift" structural check (`ShiftMissingForFiscalDoc`).
    pub shift_id: Option<ShiftId>,
    /// W14a-2b §1.4 — operator/cashier id that signs this document.
    /// Consumed by signer_guard 4-pre check.  `None` for pre-W14a-2b
    /// ledger rows (column added in migration 017).
    pub signed_by_cashier_id: Option<CashierId>,
    /// A.3 (advance-at-SEND, design v3 §6 step 1) — the doc's signed-chain
    /// fields, read in the SAME pre-CAS input fetch (write-once at stage_sign;
    /// unchanged between SIGNED and the `Sending → Sent` CAS).  The 4-b
    /// advance reads `previous_hash` from THIS pre-CAS snapshot (audit F7 — NO
    /// post-CAS re-read), asserts `ns.seed == previous_hash`, then advances the
    /// seed to `unsigned_xml_sha256`.  `mac_recovery_attempts` drives the C14
    /// Variant P branch: for a recovered doc (`>= 1`) the equality gate is
    /// SKIPPED (recovery deliberately re-anchored `previous_hash` onto the
    /// DPS-supplied hash) while the advance target is unchanged.
    pub previous_hash: Option<Vec<u8>>,
    pub unsigned_xml_sha256: Option<Vec<u8>>,
    pub mac_recovery_attempts: i64,
    /// B8-3: opaque DPS-issued code stamped at OfflineLocalAck (B8-2).
    /// `Some(s)` for offline-origin docs that went through `stage_offline_ack`.
    /// `None` for online docs and for offline-origin docs where the stamp is
    /// missing (a fail-closed invariant breach surfaced by `stage_send`).
    pub offline_dps_code: Option<String>,
}

/// W7 stage 4-pre — read the minimal field set required by stage 4 in
/// a single SELECT inside the `with_immediate` envelope.  Returns
/// `None` when the row is missing; caller treats that as
/// `StageSendError::DocumentMissing`.
///
/// **Pre-CAS read.** This MUST be called BEFORE
/// `transition_state(Signed, Sending)` so the returned `state` is the
/// pre-transition observation.  After a successful CAS the doc is in
/// `Sending`; reading state post-CAS would only ever return `Sending`
/// and would be useless for the StateConflict diagnostic.
pub async fn fetch_send_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<Option<SendInputs>> {
    let db_doc_id = DbDocumentId(doc_id);
    let row = sqlx::query!(
        r#"SELECT state                as "state: DbDocState",
                  fiscal_number,
                  lnd,
                  doc_type             as "doc_type: DbDocType",
                  business_ts,
                  backend_profile_id,
                  transport_profile_id,
                  offline_fiscal_no,
                  offline_dps_code,
                  shift_id             as "shift_id: DbShiftId",
                  signed_by_cashier_id as "signed_by_cashier_id: DbCashierId",
                  previous_hash        as "previous_hash: Vec<u8>",
                  unsigned_xml_sha256  as "unsigned_xml_sha256: Vec<u8>",
                  mac_recovery_attempts
           FROM fiscal_documents WHERE document_id = ?"#,
        db_doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(SendInputs {
        state: r.state.0,
        fiscal_number: r.fiscal_number,
        lnd: r.lnd,
        doc_type: r.doc_type.0,
        business_ts: r.business_ts,
        backend_profile_id: r.backend_profile_id,
        transport_profile_id: r.transport_profile_id,
        offline_fiscal_no: r.offline_fiscal_no,
        offline_dps_code: r.offline_dps_code,
        document_id: doc_id,
        shift_id: r.shift_id.map(|s| s.0),
        signed_by_cashier_id: r.signed_by_cashier_id.map(|c| c.0),
        previous_hash: r.previous_hash,
        unsigned_xml_sha256: r.unsigned_xml_sha256,
        mac_recovery_attempts: r.mac_recovery_attempts,
    }))
}

/// W14a-2b Commit 6 §3.7 — minimal field set `stage_offline_ack::run`
/// needs to drive its doc-type-scoped shift-state validation +
/// pre-CAS cross-FN / doc-state checks.  Strict subset of
/// `DocumentRow` — only what stage_offline_ack reads.
///
/// **Defence-in-depth (operator correction #4):**
/// `stage_offline_ack::run` does NOT trust upstream `stage_acquire` to
/// have filtered `doc_type` correctly — the widened shift-state set
/// (`Opened | OpenedLocalPendingDrain`) applies ONLY to regular
/// fiscal docs (Sell / Return / ServiceIn / ServiceOut /
/// CashWithdrawal / XReport).  Other doc types stay scoped to
/// `Opened` only.
///
/// Returned `state` is the pre-CAS observation — caller uses it for
/// `DocStateConflict` diagnostic if not `SIGNED`.
#[derive(Debug, Clone)]
pub struct OfflineAckInputs {
    pub state: DocState,
    pub doc_type: DocType,
    pub fiscal_number: String,
    /// M2-01 (review fold, 2026-06-12): the doc's signed-chain fields, read in
    /// the SAME input fetch (write-once at `stage_sign`; unchanged between SIGNED
    /// and the offline-ack CAS — so a pre-CAS read is equivalent to the prior
    /// post-CAS sibling reads).  `stage_offline_ack` step 7b asserts
    /// `ns_seed == previous_hash` then advances the seed to `unsigned_xml_sha256`.
    pub previous_hash: Option<Vec<u8>>,
    pub unsigned_xml_sha256: Option<Vec<u8>>,
}

/// W14a-2b Commit 6 §3.7 — single-SELECT inputs reader for
/// `stage_offline_ack::run`.  Returns `None` when the row is missing;
/// caller maps to `RefusalReason::DocNotFound`.
pub async fn fetch_offline_ack_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<Option<OfflineAckInputs>> {
    let db_doc_id = DbDocumentId(doc_id);
    let row = sqlx::query!(
        r#"SELECT state              as "state: DbDocState",
                  doc_type           as "doc_type: DbDocType",
                  fiscal_number,
                  previous_hash      as "previous_hash: Vec<u8>",
                  unsigned_xml_sha256 as "unsigned_xml_sha256: Vec<u8>"
           FROM fiscal_documents WHERE document_id = ?"#,
        db_doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(OfflineAckInputs {
        state: r.state.0,
        doc_type: r.doc_type.0,
        fiscal_number: r.fiscal_number,
        previous_hash: r.previous_hash,
        unsigned_xml_sha256: r.unsigned_xml_sha256,
    }))
}

/// W8 stage 5 — minimal field set the finalize stage needs to write
/// `node_state.last_known_unsigned_xml_sha256` (seed advance), the
/// `outbox` row (sequence + canonical payload hash), the inbox-DONE
/// UPDATE, and the rich `STAGE_FINALIZE_ACK` audit row.  Strict
/// subset of [`DocumentRow`] and disjoint from [`SendInputs`] (W7)
/// — finalize doesn't need state / doc_type / business_ts / profile
/// bindings.
///
/// **Source-of-truth contract (W8 review F1 close):** every field is
/// read from the `fiscal_documents` row inside the same
/// `with_immediate` envelope as the CAS.  `stage_finalize::run` does
/// NOT accept `fn_id` / `request_id` parameters — using anything but
/// the doc's own canonical fields would risk crossing data between
/// docs (Ack one doc, advance another FN's seed, mark another inbox
/// row DONE).
#[derive(Debug, Clone)]
pub struct FinalizeInputs {
    pub fiscal_number: String,
    /// W8 review F1 close: read from the doc row, NOT from caller.
    /// Used to drive `ingress_inbox::mark_done_tx` so a wrong
    /// caller-supplied request_id can never advance an unrelated
    /// inbox row.
    pub request_id: [u8; 16],
    pub lnd: i64,
    /// `Some` after W6 stage 3-PERSIST writes it; `None` only on a
    /// freshly-PREPARED row.  Post-Kvt2 invariant: `Some` MUST hold;
    /// stage_finalize::run surfaces `None` as
    /// `StageFinalizeError::UnsignedXmlShaMissing`.
    pub unsigned_xml_sha256: Option<[u8; 32]>,
    /// `Some` for non-genesis docs (set in W6 stage 3-PRE pin); `None`
    /// for the very first doc-after-bootstrap.  Used (W8 review F2
    /// close) for the chain-continuity guard: must equal the current
    /// `node_state.last_known_unsigned_xml_sha256` (None == None for
    /// genesis), else `ChainSeedMismatch` typed error + rollback.
    /// Also surfaced into the `STAGE_FINALIZE_ACK` audit payload.
    pub previous_hash: Option<[u8; 32]>,
    /// Schema-NOT-NULL on `fiscal_documents` (set at W5 stage 1
    /// INSERT PREPARED).  Copied into `outbox.payload_sha256` so the
    /// post-M3a publisher worker can cross-correlate the queue row
    /// with the canonical payload archive.
    pub payload_sha256_canonical: [u8; 32],
    /// M2-01/M2-04 (review fold, 2026-06-12): non-NULL ⟺ the doc fiscalised at
    /// `stage_offline_ack` (offline-origin), where the MAC chain seed already
    /// advanced past it.  `stage_finalize` uses this to SKIP the chain-continuity
    /// guard + seed-advance for offline-origin docs.  Read here (in the same
    /// envelope's input fetch) instead of a sibling runtime query.
    pub offline_fiscal_no: Option<i64>,
}

/// W8 stage 5 finalize — read the minimal field set required for
/// post-CAS bookkeeping in a single SELECT inside the same
/// `with_immediate` envelope as the CAS `Kvt2 → Ack`.  Returns
/// `None` when the row vanished between the CAS Applied and this
/// read (impossible under M3a's single-writer-per-FN invariant —
/// see ADR-M3-A10 — + the CAS we just committed, but typed
/// defensively rather than panicking).
pub async fn fetch_finalize_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<Option<FinalizeInputs>> {
    let db_doc_id = DbDocumentId(doc_id);
    let row = sqlx::query!(
        r#"SELECT fiscal_number,
                  request_id                as "request_id!: Vec<u8>",
                  lnd,
                  unsigned_xml_sha256       as "unsigned_xml_sha256: Vec<u8>",
                  previous_hash             as "previous_hash: Vec<u8>",
                  payload_sha256_canonical  as "payload_sha256_canonical!: Vec<u8>",
                  offline_fiscal_no
           FROM fiscal_documents WHERE document_id = ?"#,
        db_doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    let payload_sha256: [u8; 32] =
        r.payload_sha256_canonical
            .as_slice()
            .try_into()
            .map_err(|_| {
                sqlx::Error::Decode(
                    format!(
                        "fiscal_documents.payload_sha256_canonical: expected 32 bytes, got {}",
                        r.payload_sha256_canonical.len()
                    )
                    .into(),
                )
            })?;
    let request_id: [u8; 16] = r.request_id.as_slice().try_into().map_err(|_| {
        sqlx::Error::Decode(
            format!(
                "fiscal_documents.request_id: expected 16 bytes, got {}",
                r.request_id.len()
            )
            .into(),
        )
    })?;
    Ok(Some(FinalizeInputs {
        fiscal_number: r.fiscal_number,
        request_id,
        lnd: r.lnd,
        unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
        previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
        payload_sha256_canonical: payload_sha256,
        offline_fiscal_no: r.offline_fiscal_no,
    }))
}

/// W7 stage 4-pre — stamp `submission_attempted_at = CURRENT_TIMESTAMP`
/// for `doc_id`.
///
/// **Scope.** Lives ONLY inside the 4-pre `with_immediate` envelope,
/// alongside the CAS `Signed → Sending`, the
/// `transport_trace::allocate_and_insert_tx`, and the
/// `STAGE_SEND_INTENT_MARKED` audit row.  This UPDATE does NOT mean
/// "send happened" — it means "an attempt to send started".  The
/// distinction matters because Pattern B places the durable intent
/// marker BEFORE the wire call: a crash between 4-pre commit and 4-b
/// commit leaves `submission_attempted_at IS NOT NULL` regardless of
/// whether DPS received the request.
///
/// **Clock seam.** The timestamp comes from SQLite's
/// `CURRENT_TIMESTAMP` rather than a caller-supplied string, per W7
/// freeze decision #4 (no clock seam in `WorkerContext`).  Format
/// matches `audit_log.created_at` and other DEFAULT-CURRENT_TIMESTAMP
/// columns (`'YYYY-MM-DD HH:MM:SS'`); ordering against those columns
/// is monotonic-by-construction.
///
/// Returns `true` if the row exists and was updated; `false`
/// indicates a missing `document_id` and MUST be treated by the caller
/// as `StageSendError::DocumentMissing`, not silently ignored.
pub async fn mark_submission_attempted_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<bool> {
    // Idempotent stamp (R-W10.4-senior-review MED 1 close): the
    // column promise is "first submission attempt time", not "last
    // submission attempt time".  W10.4 step 2d introduced a Pattern B
    // retry path through 4-pre on attempt #2 (Resigned re-entry); the
    // earlier unconditional UPDATE silently overwrote attempt-#1's
    // timestamp.  Per-attempt timing is preserved in
    // `transport_trace.started_at`; this column documents the
    // single first-submission moment and must not be rewritten.
    //
    // Two-statement implementation (atomic in the wrapping tx) so we
    // can distinguish "row missing" from "already stamped" without
    // sqlx Database-error-text matching.  The cost is one extra
    // SELECT on stage 4-pre — negligible vs the wire send + audit
    // writes that follow.
    let res = sqlx::query(
        "UPDATE fiscal_documents SET submission_attempted_at = CURRENT_TIMESTAMP \
         WHERE document_id = ? AND submission_attempted_at IS NULL",
    )
    .bind(DbDocumentId(doc_id))
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() == 1 {
        // First-time stamp committed.
        return Ok(true);
    }
    // rows_affected == 0 could mean: (a) row missing, (b) already
    // stamped.  Disambiguate with a SELECT in the same tx.
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM fiscal_documents WHERE document_id = ?")
            .bind(DbDocumentId(doc_id))
            .fetch_optional(&mut **tx)
            .await?;
    Ok(exists.is_some())
}

/// W7 stage 4-b — persist the DPS-assigned fiscal id (`CheckAck.id`)
/// onto the document row.
///
/// **Scope.** Lives ONLY inside the 4-b `with_immediate` envelope,
/// alongside the CAS `Sending → Sent` (or `Sending → Kvt1` on inline
/// KVT1 piggyback in W8), the matching `transport_trace::complete_tx`,
/// and the audit row.  Not an idempotency seam: the CAS in 4-b is
/// what guarantees the write happens at most once per attempt; this
/// UPDATE is the side-effect of that CAS having succeeded.
///
/// Returns `true` if the row exists and was updated; `false` indicates
/// a missing `document_id` and MUST be treated by the caller as a
/// stage error (caller bug — the row was alive in 4-pre, so a
/// 4-b-time miss is unrecoverable mid-stage), NOT a silent ignore.
pub async fn set_server_fiscal_no_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    server_fiscal_no: &str,
) -> sqlx::Result<bool> {
    let res = sqlx::query("UPDATE fiscal_documents SET server_fiscal_no = ? WHERE document_id = ?")
        .bind(server_fiscal_no)
        .bind(DbDocumentId(doc_id))
        .execute(&mut **tx)
        .await?;
    Ok(res.rows_affected() == 1)
}

/// W10.4 — claim the single-bit MAC-recovery counter for `doc_id`.
/// CAS shape: succeed only if the doc is in `ERROR_RETRYABLE` AND
/// `mac_recovery_attempts == 0`.  On success bumps the counter to 1
/// and returns `true`; on any other state — wrong doc state,
/// counter already burned, missing row — returns `false`.
///
/// **Lifecycle (per freeze §4.4.1 + R-W10.4 HIGH 2 split).**  Called
/// by `mac_recovery::run_mac_recovery` AFTER the attempt-#1 4-b commit
/// lands the doc in `ErrorRetryable` with `STAGE_SEND_MAC_HASH_MISMATCH`
/// audit + `RETRYABLE_MAC_HASH_MISMATCH` trace.
///
/// **MR-CLAIM and MR-PERSIST run in SEPARATE `with_immediate`
/// envelopes by design** — this is NOT a bug.  The claim envelope
/// (MR-CLAIM, this helper) commits the counter bump alone, before
/// the no-tx re-sign step runs; the rewrite envelope (MR-PERSIST)
/// commits `previous_hash` + replaced SIGNED_XML + `MAC_RECOVERY_RESIGNED`
/// audit together.  Crash between them leaves the doc in
/// `ERROR_RETRYABLE` with `mac_recovery_attempts = 1` and OLD
/// artifacts; worker re-entry hits this helper's `rows_affected = 0`
/// branch (counter already 1) ⇒ `CounterExhausted` ⇒ caller emits
/// `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH` ⇒ doc Rejected.  No
/// silent progression; partial state forensically visible via
/// missing `MAC_RECOVERY_RESIGNED` audit row.  See
/// `services/write_path/mac_recovery.rs` module docs (HIGH 2 section).
///
/// **Why CAS guard `state = 'ERROR_RETRYABLE'`.**  A doc in any other
/// state shouldn't go through MAC recovery (e.g. operator manually
/// flipped to Rejected, or W9 promoted to RequiresManualReconciliation).
/// Bare counter UPDATE without state guard would silently burn the
/// budget for a doc that no longer needs it.
///
/// **Why CAS guard `mac_recovery_attempts = 0`.**  Single-bit budget
/// per W0-3 §2.1 row -12: ONE auto-recovery per doc.  A second `-12`
/// for a doc that already burned its budget routes to TerminalReject
/// with audit `MacRecoveryFailedRepeatHashMismatch` (closed-enum
/// `AuditEvent` per freeze §3.4).
///
/// **DDL CHECK as belt-and-braces.**  Migration 013 enforces
/// `mac_recovery_attempts IN (0, 1)`; this helper guarantees we only
/// ever transition 0→1 (never 1→2 — that would be a CHECK violation
/// + observable error, not silent corruption).
pub async fn mac_recovery_claim_counter_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<bool> {
    let res = sqlx::query(
        "UPDATE fiscal_documents SET mac_recovery_attempts = 1 \
         WHERE document_id = ? \
           AND state = 'ERROR_RETRYABLE' \
           AND mac_recovery_attempts = 0",
    )
    .bind(DbDocumentId(doc_id))
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected() == 1)
}

/// **M3b W12 Post-Closure Hardening Phase 2a.1 / REC-1 (2026-05-24)**
/// — increment `fiscal_documents.consecutive_holds` for `doc_id` and
/// return the new value.  Called inside Envelope 1c-hold
/// (light + bundled варіанти) atomically з audit emission per I4
/// transactional invariant.
///
/// Returns the post-increment value (i64); caller (drain orchestrator)
/// uses it for Tier-1 (>= 10 → KVT2_CONFIRM_PROLONGED_HOLD audit) +
/// Tier-2 (>= 50 → STOP_MODE CAS) trigger checks per plan §REC-1.
///
/// Missing doc row surfaces as `sqlx::Error::RowNotFound` from
/// `fetch_one` (UPDATE ... RETURNING produces no row when WHERE
/// matches nothing); caller propagates via `?` to `BootError::Internal`
/// — cohort walker should never surface non-existent doc, so this
/// is a structural breach.
pub async fn increment_consecutive_holds_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<i64> {
    let row = sqlx::query_scalar::<_, i64>(
        "UPDATE fiscal_documents \
         SET consecutive_holds = consecutive_holds + 1 \
         WHERE document_id = ? \
         RETURNING consecutive_holds",
    )
    .bind(DbDocumentId(doc_id))
    .fetch_one(&mut **tx)
    .await?;
    Ok(row)
}

/// **M3b W12 Post-Closure Hardening Phase 2a.1 / REC-1 (2026-05-24)**
/// — reset `fiscal_documents.consecutive_holds` to 0 for `doc_id`.
/// Called inside non-Hold advance envelopes (Envelope 1a, 1b,
/// 1a-replay, 1c-post — every path that moves the doc forward з Hold
/// stuck state OR completes successfully).  Atomic з state CAS +
/// audit per I4.
///
/// Idempotent — UPDATE without CAS guard; setting 0→0 is no-op.
/// `rows_affected == 0` → missing doc row (structural breach);
/// returned bool lets caller fail-loud.
pub async fn reset_consecutive_holds_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<bool> {
    let res =
        sqlx::query("UPDATE fiscal_documents SET consecutive_holds = 0 WHERE document_id = ?")
            .bind(DbDocumentId(doc_id))
            .execute(&mut **tx)
            .await?;
    Ok(res.rows_affected() == 1)
}
