//! Stage 5 — finalize (Kvt2 → Ack + atomic post-CAS bookkeeping).
//!
//! W8.3 lands the `pub async fn run` worker step.  Single
//! `with_immediate` envelope; five atomic writes:
//!
//!   1. CAS `Kvt2 → Ack` on `fiscal_documents`.
//!   2. UPDATE `node_state.last_known_unsigned_xml_sha256` (cross-doc
//!      MAC chain seed advance).
//!   3. UPDATE `ingress_inbox.status = 'DONE'`.
//!   4. INSERT `outbox` row (status=PENDING; cross-process publish
//!      is post-M3a).
//!   5. INSERT `audit_log` `STAGE_FINALIZE_ACK`.
//!
//! Anchored on:
//!   - W8 design freeze §1, §4.3 (pseudocode), §11 (review tracking).
//!   - ADR-M3-A8 (KVT2 forward-only; Ack as terminal-success).
//!   - W7 freeze decision #4 — DB clock CURRENT_TIMESTAMP, no
//!     `WorkerContext` clock seam.
//!
//! Out of W8.3 scope (per design freeze §6):
//!   - DPS / `transport_trace` interaction — finalize never touches
//!     wire state or the outgoing-attempt forensic table.
//!   - Sent → Kvt1 → Kvt2 quittance pipeline — post-W8 / W10.
//!   - App::boot recovery for stuck Kvt2 docs — W9.
//!   - OFFLINE_LOCAL_ACK / excise marks / shift side-effects — M3b.
//!
//! W3 static scan invariant: this module makes no `send_chk` /
//! `sign_cms_detached` / other foreign-IO calls; the closure body is
//! pure DB.  The runtime task_local guard panics in debug if any
//! denied method is reached from inside the `with_immediate`.
//!
//! **Cancellation safety.**  `run` is cancel-safe at every await
//! point.  If the caller drops the future before the enclosing
//! `with_immediate` commits, sqlx's `SqliteConnection::Drop` issues
//! an automatic ROLLBACK on the in-flight `BEGIN IMMEDIATE`
//! transaction — no partial state survives.  The CAS `Kvt2 → Ack`
//! and the four downstream writes are visible only after the
//! envelope's COMMIT, which is the closure's last syscall; any
//! earlier cancellation rolls all five writes back atomically.

use sqlx::SqlitePool;

use crate::db::models::enums::{DocState, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::fiscal_documents::TransitionOutcome;
use crate::db::repositories::outbox::NewOutboxRow;
use crate::db::repositories::{
    audit_log, fiscal_documents as fd, ingress_inbox, node_state, outbox,
};
use crate::db::tx::with_immediate;

// ─── Errors ──────────────────────────────────────────────────────────

/// Stage-5 typed error surface.  All variants describe state-invariant
/// breaches or DB failures; idempotent re-entry and document-missing
/// paths are NOT errors (see [`StageFinalizeOutcome`]).
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StageFinalizeError {
    /// Doc reached `Kvt2` but `unsigned_xml_sha256 IS NULL` —
    /// state-invariant breach (W6 stage 3-PERSIST writes the column
    /// before any state can advance past `Signed`).  Without the
    /// hash there is no MAC chain seed to advance, so finalize
    /// fail-closes; the tx rolls back entirely (no Ack persisted).
    #[error("stage 5 unsigned_xml_sha256 missing for doc {document_id:?} despite reaching Kvt2")]
    UnsignedXmlShaMissing { document_id: DocumentId },

    /// Doc's `previous_hash` does not match the FN's current
    /// `node_state.last_known_unsigned_xml_sha256` (the chain seed
    /// from the previously-Acked doc).  Out-of-order finalize
    /// (e.g. W9 boot recovery iterating Kvt2 docs in wrong lnd
    /// order) would silently corrupt the next-doc MAC chain seed
    /// without this guard.
    ///
    /// Per W8 review F2: chain-continuity is enforced at finalize,
    /// not just by the W9 design constraint that recovery must sort
    /// by lnd.  `expected` is the doc's `previous_hash` (what the
    /// chain SHOULD point to upon entering finalize), `actual` is
    /// the FN's `last_known_unsigned_xml_sha256` (what the chain
    /// DOES point to).  Both `None` is the genesis case (legitimate;
    /// no error).  Any other inequality is a structural breach;
    /// rollback the entire envelope.
    #[error("stage 5 chain seed mismatch for doc {document_id:?}: expected {expected:?}, actual {actual:?}")]
    ChainSeedMismatch {
        document_id: DocumentId,
        expected: Option<[u8; 32]>,
        actual: Option<[u8; 32]>,
    },

    /// `node_state::update_last_known_xml_sha_tx` returned `false` —
    /// `node_state` row missing for the FN.  W5 acquire stage upserts
    /// this row before any doc lands in PREPARED, so a missing row
    /// here is structural breach.  Tx rolls back; Ack is undone.
    #[error("stage 5 seed UPDATE returned 0 for fn {fn_id:?} — node_state row missing")]
    SeedUpdateMissing { fn_id: String },

    /// `ingress_inbox::mark_done_tx` returned `false` — inbox row
    /// missing for the request.  Impossible under M3a's single-
    /// writer-per-FN invariant (see ADR-M3-A10): the inbox row is
    /// what drives the worker; it must exist for stage 5 to run.
    /// Tx rolls back.
    #[error("stage 5 mark_done returned 0 for request_id {request_id:?} — inbox row missing")]
    InboxDoneMissing { request_id: [u8; 16] },

    /// Pass-through DB error (sqlx-typed).  Distinct variant so
    /// callers can route DB issues separately from state-invariant
    /// breaches.
    #[error("stage 5 db: {0}")]
    Db(#[source] sqlx::Error),

    /// Catch-all for non-sqlx, non-typed `anyhow::Error` chains
    /// surfacing from the `with_immediate` closure.  Cause chain
    /// preserved.  Production callers should never see this in
    /// healthy flow.
    #[error("stage 5 internal: {0}")]
    Internal(#[source] anyhow::Error),
}

// ─── Outcome ─────────────────────────────────────────────────────────

/// Top-level outcome of [`run`].  Four variants — three "success-shape"
/// (success + idempotent re-entry + race-with-delete) and one
/// "state-machine surprise" (caller / W9 escalates).
///
///   - `Acked` — happy: doc advanced to terminal Ack; seed advanced;
///     inbox DONE; outbox enqueued; audit appended.
///   - `AlreadyAcked` — idempotent re-entry: doc was already `Ack`
///     at CAS time.  No side effects; success-shape.
///   - `StateConflict` — doc exists but neither in `Kvt2` nor `Ack`
///     (e.g. Sent / Kvt1 / Rejected / ErrorRetryable).  Caller / W9
///     decides whether to wait, retry, or escalate.
///   - `DocumentMissing` — doc row not present at CAS time.  Race
///     with delete or operator-driven cleanup; no-op success-shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageFinalizeOutcome {
    Acked { fiscal_number: String, lnd: i64 },
    AlreadyAcked,
    StateConflict { observed: DocState },
    DocumentMissing,
}

// ─── Internal helpers ────────────────────────────────────────────────

/// Internal closure-result shape; mapped to [`StageFinalizeOutcome`]
/// after the `with_immediate` returns.  Distinct from the public
/// outcome because `Acked` carries owned `String` for `fiscal_number`
/// — moved out of the closure cleanly.
enum InternalOutcome {
    Acked { fiscal_number: String, lnd: i64 },
    AlreadyAcked,
    StateConflict { observed: DocState },
    DocumentMissing,
}

/// Bridge `anyhow::Error` from the closure back to typed
/// [`StageFinalizeError`].  Mirrors the W6 / W7 pattern: typed errors
/// thrown via `anyhow::Error::new(StageFinalizeError::...)` round-trip
/// cleanly; raw `sqlx::Error` becomes `Db`; everything else surfaces
/// as `Internal` preserving the cause chain.
///
/// **Bridge discipline (callers MUST follow):** typed errors are
/// constructed at the **top level** of the `anyhow::Error` chain via
/// `anyhow::Error::new(StageFinalizeError::...)`.  Wrapping a typed
/// error with `.context("...")` (which prepends the context as the
/// new top of the chain) defeats this downcast — the error would
/// surface as `Internal(anyhow::Error)` instead of the typed variant.
/// If a future refactor needs context strings, switch this bridge to
/// walk the chain via `e.chain().find_map(...)` rather than relaxing
/// the discipline.
fn bridge_anyhow(e: anyhow::Error) -> StageFinalizeError {
    match e.downcast::<StageFinalizeError>() {
        Ok(typed) => typed,
        Err(rest) => match rest.downcast::<sqlx::Error>() {
            Ok(sqlx_err) => StageFinalizeError::Db(sqlx_err),
            Err(other) => StageFinalizeError::Internal(other),
        },
    }
}

/// Lower-cased hex encode for audit payload fields.  Matches the
/// W6 stage_sign convention (`hex_encode` private helper); kept
/// duplicated here rather than promoted to a shared module — both
/// uses are local and a shared utility module would invite scope
/// creep into a polish slice.
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

// ─── Worker step ─────────────────────────────────────────────────────

/// Stage 5 worker step — single-`with_immediate` finalize.
///
/// **Source-of-truth contract (W8 review F1 close).**  The signature
/// takes ONLY `doc` (DocumentId).  `fiscal_number` and `request_id`
/// are read from the doc row via [`fd::fetch_finalize_inputs_tx`]
/// inside the same envelope as the CAS — caller-supplied FN /
/// request_id would let a wrong dispatch (worker bug, race) Ack one
/// doc but advance another FN's seed or mark another inbox row
/// DONE.  The new shape makes that bug class structurally impossible.
///
/// **Atomicity envelope.**  All five writes (CAS, seed, inbox, outbox,
/// audit) commit under one `BEGIN IMMEDIATE`; partial commits are
/// impossible by construction.  Any post-CAS write returning a
/// state-invariant breach (`ChainSeedMismatch`, `SeedUpdateMissing`,
/// `InboxDoneMissing`) rolls the entire tx back via `?`-propagation
/// — the chain pointer cannot leak past a failed Ack, and
/// `STAGE_FINALIZE_ACK` audit row is written iff the doc actually
/// reached terminal Ack.
///
/// **Chain-continuity guard (W8 review F2 close).**  Before the seed
/// advance, the closure reads `node_state.last_known_unsigned_xml_sha256`
/// inside the same tx and asserts it equals `inputs.previous_hash`
/// (None == None for genesis).  Out-of-order finalize (e.g. W9 boot
/// recovery iterating Kvt2 docs in wrong lnd order) would silently
/// corrupt the chain seed without this guard.
///
/// **Idempotency.**  CAS `Kvt2 → Ack` on a doc already in `Ack`
/// returns `Conflict`; the closure disambiguates via a follow-up
/// state read and surfaces `StageFinalizeOutcome::AlreadyAcked` (no
/// side effects).  Outbox PK on `document_id` is the second-line
/// defence: even if the CAS short-circuit ever regressed, a duplicate
/// finalize would fail loudly via PK violation.
///
/// **Audit payload (rich, per design freeze §7 Q4).**  Carries
/// `request_id`, `document_id`, `fiscal_number`, `lnd`,
/// `unsigned_xml_sha256_hex`, `previous_hash_hex` — for cross-correlation
/// with `outbox` (sequence_no = lnd) and the W9 boot reconciliation
/// view of the chain seed.
pub async fn run(
    pool: &SqlitePool,
    doc: DocumentId,
) -> Result<StageFinalizeOutcome, StageFinalizeError> {
    let outcome = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // 1. CAS Kvt2 → Ack.  Conflict → disambiguate already-Ack
            //    (idempotent) vs other states (StateConflict).
            match fd::transition_state(tx, doc, DocState::Kvt2, DocState::Ack).await? {
                TransitionOutcome::Applied => {}
                TransitionOutcome::Conflict => {
                    let observed = match fd::fetch_send_inputs_tx(tx, doc).await? {
                        Some(s) => s.state,
                        None => return Ok::<_, anyhow::Error>(InternalOutcome::DocumentMissing),
                    };
                    return Ok(if observed == DocState::Ack {
                        InternalOutcome::AlreadyAcked
                    } else {
                        InternalOutcome::StateConflict { observed }
                    });
                }
                TransitionOutcome::NotFound => return Ok(InternalOutcome::DocumentMissing),
                TransitionOutcome::Forbidden => {
                    unreachable!(
                        "(Kvt2,Ack) is whitelisted in fiscal_documents::allowed_transition"
                    )
                }
            }

            // 2. Read finalize inputs (post-CAS — state == Ack).
            //    Race-with-delete between CAS Applied and this read
            //    is impossible under M3a's single-writer-per-FN
            //    invariant (see ADR-M3-A10) + the same BEGIN
            //    IMMEDIATE envelope, but typed defensively.
            let inputs = fd::fetch_finalize_inputs_tx(tx, doc)
                .await?
                .ok_or_else(|| {
                    anyhow::Error::new(StageFinalizeError::Internal(anyhow::anyhow!(
                        "doc {doc:?} vanished between CAS Applied and finalize_inputs read"
                    )))
                })?;
            let seed = inputs.unsigned_xml_sha256.ok_or_else(|| {
                anyhow::Error::new(StageFinalizeError::UnsignedXmlShaMissing { document_id: doc })
            })?;

            // M2-01 (Fable-locked, 2026-06-12): is this an OFFLINE-ORIGIN doc?
            // An `offline_fiscal_no` was stamped at `stage_offline_ack`, where
            // the chain seed ALREADY advanced past this doc (M2-01 step 2).  For
            // such docs the drain's finalize must NOT re-validate the chain
            // (the seed is now further along — the §3 guard would false-fail)
            // and must NOT re-advance it (double-move).  Online-origin docs are
            // unchanged: they fiscalise at ACK, so finalize owns the guard +
            // advance.  (Sibling runtime read — same envelope; kept out of the
            // `fetch_finalize_inputs_tx` query! macro to avoid `.sqlx`
            // regeneration; same semantics as the locked design.)
            let offline_origin: Option<i64> = sqlx::query_scalar(
                "SELECT offline_fiscal_no FROM fiscal_documents WHERE document_id = ?",
            )
            .bind(doc)
            .fetch_one(&mut **tx)
            .await?;

            if offline_origin.is_none() {
                // 3. Chain-continuity guard (W8 review F2 close).  Read
                //    the FN's current chain seed inside the same tx and
                //    assert it equals this doc's `previous_hash` — i.e.
                //    this doc extends the existing chain, not jumps over
                //    it.  Genesis case: both `None`.  Out-of-order
                //    finalize (W9 boot recovery in wrong lnd order) is
                //    structurally rejected here.  (Online-origin only —
                //    offline-origin docs guarded at offline-ack, M2-01.)
                let ns_row = node_state::get_tx(tx, &inputs.fiscal_number)
                    .await?
                    .ok_or_else(|| {
                        anyhow::Error::new(StageFinalizeError::SeedUpdateMissing {
                            fn_id: inputs.fiscal_number.clone(),
                        })
                    })?;
                if ns_row.last_known_unsigned_xml_sha256 != inputs.previous_hash {
                    return Err(anyhow::Error::new(StageFinalizeError::ChainSeedMismatch {
                        document_id: doc,
                        expected: inputs.previous_hash,
                        actual: ns_row.last_known_unsigned_xml_sha256,
                    }));
                }

                // 4. Cross-doc MAC chain seed advance.  fiscal_number is
                //    sourced from the doc row (W8 review F1 close).
                //    (Online-origin only — offline seed advanced at offline-ack.)
                if !node_state::update_last_known_xml_sha_tx(tx, &inputs.fiscal_number, &seed)
                    .await?
                {
                    return Err(anyhow::Error::new(StageFinalizeError::SeedUpdateMissing {
                        fn_id: inputs.fiscal_number.clone(),
                    }));
                }
            }

            // 5. Inbox DONE.  request_id is sourced from the doc row
            //    (W8 review F1 close).
            if !ingress_inbox::mark_done_tx(tx, &inputs.request_id).await? {
                return Err(anyhow::Error::new(StageFinalizeError::InboxDoneMissing {
                    request_id: inputs.request_id,
                }));
            }

            // 6. Outbox INSERT.  PK on document_id is defence-in-depth
            //    against duplicate finalize (CAS short-circuit above
            //    is the primary guard).
            outbox::enqueue_document_tx(
                tx,
                doc,
                NewOutboxRow {
                    fiscal_number: inputs.fiscal_number.clone(),
                    sequence_no: inputs.lnd,
                    payload_sha256: inputs.payload_sha256_canonical,
                },
            )
            .await?;

            // 7. Audit STAGE_FINALIZE_ACK with rich cross-correlation
            //    payload.
            let payload = serde_json::json!({
                "request_id": hex_encode(&inputs.request_id),
                "document_id": hex_encode(doc.as_bytes()),
                "fiscal_number": inputs.fiscal_number,
                "lnd": inputs.lnd,
                "unsigned_xml_sha256_hex": hex_encode(&seed),
                "previous_hash_hex": inputs.previous_hash.map(|h| hex_encode(&h)),
            })
            .to_string();
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                "STAGE_FINALIZE_ACK",
                Severity::Info,
                None,
                Some(&payload),
            )
            .await?;

            Ok(InternalOutcome::Acked {
                fiscal_number: inputs.fiscal_number,
                lnd: inputs.lnd,
            })
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    Ok(match outcome {
        InternalOutcome::Acked { fiscal_number, lnd } => {
            StageFinalizeOutcome::Acked { fiscal_number, lnd }
        }
        InternalOutcome::AlreadyAcked => StageFinalizeOutcome::AlreadyAcked,
        InternalOutcome::StateConflict { observed } => {
            StageFinalizeOutcome::StateConflict { observed }
        }
        InternalOutcome::DocumentMissing => StageFinalizeOutcome::DocumentMissing,
    })
}
