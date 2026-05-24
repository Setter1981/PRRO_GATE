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
    pub unsigned_xml_sha256: Option<[u8; 32]>,
    pub previous_hash: Option<[u8; 32]>,
    /// W14a-2b §1.4 — cashier id that will sign this document.  Persisted
    /// on the ledger row at stage 1 INSERT PREPARED; consumed at
    /// stage_send 4-pre by signer_guard.  None for system-context paths
    /// without operator attribution (none currently).
    pub signed_by_cashier_id: Option<CashierId>,
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
            | (Signed, Encrypted)
            | (Signed, ErrorRetryable)
            | (Signed, OfflineLocalAck)
            | (Encrypted, Sent)
            | (Encrypted, ErrorRetryable)
            | (Sent, Kvt1)
            | (Sent, ErrorRetryable)
            | (Sent, Rejected)
            // W11 PR-2b — SENT last_chk mismatch escalation per W0-3 §6.4-b
            // (`docs/superpowers/specs/2026-05-06-m3-w0-3-retry-recovery.md:771-772`).
            // When boot-recovery probe yields `CheckAck { id != transport_request_id }`,
            // the doc cannot be reconciled automatically: we have on-record a SENT
            // marker but DPS reports a different fiscal id.  Direct transition to
            // RequiresManualReconciliation is the operator-handoff edge — no prior
            // hop through ErrorRetryable, because the situation is not retryable
            // (DPS state and local state diverged at the protocol layer).
            | (Sent, RequiresManualReconciliation)
            | (Kvt1, Kvt2)
            | (Kvt1, ErrorRetryable)
            | (Kvt2, Ack)
            | (OfflineLocalAck, Sent)
            // M3b W6 — Pattern C edges per spec §5.3 / M3b plan
            // §Task 6.  The W4/W5 offline subsystem produces docs
            // in `OfflineLocalAck` while node is offline; on
            // return-online the W7 drain stage flips them through
            // `Sending` (Pattern B intent-marker, mirroring the
            // online ladder) and `Cancelled` is the manual-operator
            // escape if the drain is abandoned mid-flight.  Locked
            // edge set + count pinned in
            // `tests/fiscal_documents_offline_local_ack_edges_locked.rs`.
            // The legacy M3a `(OfflineLocalAck, Sent)` placeholder
            // immediately above is preserved (W6 scope: add-only,
            // no removals; runtime wiring of the new edges is W7).
            | (OfflineLocalAck, Sending)
            | (OfflineLocalAck, Cancelled)
            // M3b W9b §5.1 — lastChk replay short-circuit edge.
            // When backlog_drain issues a lastChk pre-flight on a
            // doc with `server_fiscal_no IS NOT NULL` AND DPS confirms
            // `status == OK` + id match + non-empty data_sign, the
            // doc has already been wire-acknowledged by DPS — drain
            // skips wire send and advances Kvt2 directly (W12 PR
            // reuses the same lastChk response as KVT2 evidence).
            // Final hop Kvt2 → Ack is the existing M3a edge below.
            // Locked-edge count drift-guard: 28 → 29.
            | (OfflineLocalAck, Kvt2)
            | (ErrorRetryable, Sent)
            | (ErrorRetryable, Kvt1)
            | (ErrorRetryable, RequiresManualReconciliation)
            // Pattern B (ADR-M3-A5 / A9 step 3): Sending is the
            // intent-marker for stage 4 send.  The 7 additions below
            // wire it in.  M3a DPS code MUST NOT use the legacy
            // (ErrorRetryable, Sent) entry for wire send; retries go
            // through (ErrorRetryable, Sending) and then on through
            // a fresh wire call.
            | (Signed, Sending)
            | (Encrypted, Sending)
            | (Sending, Sent)
            | (Sending, Kvt1)
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
             payload_sha256_canonical, unsigned_xml_sha256, previous_hash,
             signed_by_cashier_id
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 'PREPARED', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(n.document_id)
    .bind(n.request_id)
    .bind(&n.fiscal_number)
    .bind(n.shift_id)
    .bind(n.offline_session_id)
    .bind(n.lnd)
    .bind(n.doc_type)
    .bind(&n.backend_profile_id)
    .bind(&n.transport_profile_id)
    .bind(n.fs_mode)
    .bind(&n.business_ts)
    .bind(n.total_sum_kop)
    .bind(&n.payload_json)
    .bind(&n.payload_sha256_canonical[..])
    .bind(n.unsigned_xml_sha256.as_ref().map(|b| &b[..]))
    .bind(n.previous_hash.as_ref().map(|b| &b[..]))
    .bind(n.signed_by_cashier_id.as_ref().map(|c| c.as_str()))
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
        .bind(to)
        .bind(id)
        .bind(from)
        .execute(&mut **tx)
        .await?
    } else {
        sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ? AND state = ?")
            .bind(to)
            .bind(id)
            .bind(from)
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
            .bind(id)
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
             offline_session_id = ? \
         WHERE document_id = ? AND fiscal_number = ? AND state = 'SIGNED'",
    )
    .bind(code_lnd)
    .bind(consumed_at)
    .bind(offline_session_id)
    .bind(id)
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
            .bind(id)
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
        r#"SELECT document_id    as "document_id: DocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DocState",
                  doc_type        as "doc_type: DocType",
                  server_fiscal_no,
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at,
                  signed_by_cashier_id  as "signed_by_cashier_id: CashierId"
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
                document_id: r.document_id,
                fiscal_number: r.fiscal_number,
                lnd: r.lnd,
                state: r.state,
                doc_type: r.doc_type,
                server_fiscal_no: r.server_fiscal_no,
                submission_attempted_at: r.submission_attempted_at,
                backend_profile_id: r.backend_profile_id,
                transport_profile_id: r.transport_profile_id,
                previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
                z_report_number: r.z_report_number,
                unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
                signing_inputs_pinned_at: r.signing_inputs_pinned_at,
                signed_by_cashier_id: r.signed_by_cashier_id,
            })
        })
        .collect()
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
    let rows = sqlx::query!(
        r#"SELECT document_id    as "document_id: DocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DocState",
                  doc_type        as "doc_type: DocType",
                  server_fiscal_no,
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at,
                  signed_by_cashier_id  as "signed_by_cashier_id: CashierId"
           FROM fiscal_documents
           WHERE fiscal_number = ?
             AND offline_session_id = ?
             AND fs_mode = 'OFFLINE'
             AND state IN ('OFFLINE_LOCAL_ACK','SENT','KVT1','ERROR_RETRYABLE','KVT2')
           ORDER BY lnd, created_at, document_id"#,
        fn_id,
        session_id,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(DocumentRow {
                document_id: r.document_id,
                fiscal_number: r.fiscal_number,
                lnd: r.lnd,
                state: r.state,
                doc_type: r.doc_type,
                server_fiscal_no: r.server_fiscal_no,
                submission_attempted_at: r.submission_attempted_at,
                backend_profile_id: r.backend_profile_id,
                transport_profile_id: r.transport_profile_id,
                previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
                z_report_number: r.z_report_number,
                unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
                signing_inputs_pinned_at: r.signing_inputs_pinned_at,
                signed_by_cashier_id: r.signed_by_cashier_id,
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
    let row = sqlx::query!(
        r#"SELECT
              COUNT(*)                                       AS "total!: i64",
              SUM(CASE WHEN state = 'ACK' THEN 1 ELSE 0 END) AS "ack_count: i64"
           FROM fiscal_documents
           WHERE offline_session_id = ?
             AND fs_mode = 'OFFLINE'"#,
        session_id,
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
    let row = sqlx::query!(
        r#"SELECT document_id    as "document_id: DocumentId",
                  fiscal_number,
                  lnd,
                  state           as "state: DocState",
                  doc_type        as "doc_type: DocType",
                  server_fiscal_no,
                  submission_attempted_at,
                  backend_profile_id,
                  transport_profile_id,
                  previous_hash         as "previous_hash: Vec<u8>",
                  z_report_number,
                  unsigned_xml_sha256   as "unsigned_xml_sha256: Vec<u8>",
                  signing_inputs_pinned_at,
                  signed_by_cashier_id  as "signed_by_cashier_id: CashierId"
           FROM fiscal_documents
           WHERE request_id = ?
             AND state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE')"#,
        request_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(DocumentRow {
        document_id: r.document_id,
        fiscal_number: r.fiscal_number,
        lnd: r.lnd,
        state: r.state,
        doc_type: r.doc_type,
        server_fiscal_no: r.server_fiscal_no,
        submission_attempted_at: r.submission_attempted_at,
        backend_profile_id: r.backend_profile_id,
        transport_profile_id: r.transport_profile_id,
        previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
        z_report_number: r.z_report_number,
        unsigned_xml_sha256: decode_blob32(r.unsigned_xml_sha256, "unsigned_xml_sha256")?,
        signing_inputs_pinned_at: r.signing_inputs_pinned_at,
        signed_by_cashier_id: r.signed_by_cashier_id,
    }))
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
              AND state IN ('ACK','REJECTED','CANCELLED','OFFLINE_LOCAL_ACK','REQUIRES_MANUAL_RECONCILIATION')
            LIMIT 1"#,
    )
    .bind(request_id)
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
             payload_sha256_canonical, unsigned_xml_sha256, previous_hash,
             signed_by_cashier_id
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 'PREPARED', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(n.document_id)
    .bind(n.request_id)
    .bind(&n.fiscal_number)
    .bind(n.shift_id)
    .bind(n.offline_session_id)
    .bind(n.lnd)
    .bind(n.doc_type)
    .bind(&n.backend_profile_id)
    .bind(&n.transport_profile_id)
    .bind(n.fs_mode)
    .bind(&n.business_ts)
    .bind(n.total_sum_kop)
    .bind(&n.payload_json)
    .bind(&n.payload_sha256_canonical[..])
    .bind(n.unsigned_xml_sha256.as_ref().map(|b| &b[..]))
    .bind(n.previous_hash.as_ref().map(|b| &b[..]))
    .bind(n.signed_by_cashier_id.as_ref().map(|c| c.as_str()))
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
}

/// W6 stage 3-PRE — read state + pin status atomically inside the
/// `with_immediate` envelope.  See [`PinnedSigningInputs`] for shape.
pub async fn get_signing_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<Option<PinnedSigningInputs>> {
    let row = sqlx::query!(
        r#"SELECT state                    as "state: DocState",
                  previous_hash            as "previous_hash: Vec<u8>",
                  z_report_number,
                  signing_inputs_pinned_at
           FROM fiscal_documents WHERE document_id = ?"#,
        doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(PinnedSigningInputs {
        state: r.state,
        is_pinned: r.signing_inputs_pinned_at.is_some(),
        previous_hash: decode_blob32(r.previous_hash, "previous_hash")?,
        z_report_number: r.z_report_number,
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
) -> sqlx::Result<u64> {
    let res = sqlx::query(
        "UPDATE fiscal_documents \
         SET previous_hash            = ?, \
             z_report_number          = ?, \
             signing_inputs_pinned_at = CURRENT_TIMESTAMP \
         WHERE document_id = ? \
           AND state = 'PREPARED' \
           AND signing_inputs_pinned_at IS NULL",
    )
    .bind(previous_hash.map(|h| &h[..]))
    .bind(z_report_number)
    .bind(doc_id)
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
            .bind(doc_id)
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
    let row = sqlx::query!(
        r#"SELECT state                as "state: DocState",
                  fiscal_number,
                  lnd,
                  doc_type             as "doc_type: DocType",
                  business_ts,
                  backend_profile_id,
                  transport_profile_id,
                  offline_fiscal_no,
                  shift_id             as "shift_id: ShiftId",
                  signed_by_cashier_id as "signed_by_cashier_id: CashierId"
           FROM fiscal_documents WHERE document_id = ?"#,
        doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(SendInputs {
        state: r.state,
        fiscal_number: r.fiscal_number,
        lnd: r.lnd,
        doc_type: r.doc_type,
        business_ts: r.business_ts,
        backend_profile_id: r.backend_profile_id,
        transport_profile_id: r.transport_profile_id,
        offline_fiscal_no: r.offline_fiscal_no,
        document_id: doc_id,
        shift_id: r.shift_id,
        signed_by_cashier_id: r.signed_by_cashier_id,
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
}

/// W14a-2b Commit 6 §3.7 — single-SELECT inputs reader for
/// `stage_offline_ack::run`.  Returns `None` when the row is missing;
/// caller maps to `RefusalReason::DocNotFound`.
pub async fn fetch_offline_ack_inputs_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
) -> sqlx::Result<Option<OfflineAckInputs>> {
    let row = sqlx::query!(
        r#"SELECT state        as "state: DocState",
                  doc_type     as "doc_type: DocType",
                  fiscal_number
           FROM fiscal_documents WHERE document_id = ?"#,
        doc_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(r) = row else { return Ok(None) };
    Ok(Some(OfflineAckInputs {
        state: r.state,
        doc_type: r.doc_type,
        fiscal_number: r.fiscal_number,
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
    let row = sqlx::query!(
        r#"SELECT fiscal_number,
                  request_id                as "request_id!: Vec<u8>",
                  lnd,
                  unsigned_xml_sha256       as "unsigned_xml_sha256: Vec<u8>",
                  previous_hash             as "previous_hash: Vec<u8>",
                  payload_sha256_canonical  as "payload_sha256_canonical!: Vec<u8>"
           FROM fiscal_documents WHERE document_id = ?"#,
        doc_id
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
    .bind(doc_id)
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
            .bind(doc_id)
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
        .bind(doc_id)
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
    .bind(doc_id)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected() == 1)
}
