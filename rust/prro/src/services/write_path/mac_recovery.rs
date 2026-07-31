//! W10.4 step 2c — MAC recovery orchestrator (Server `-12`
//! ERROR_BAD_HASH_PREV).
//!
//! Lives ABOVE the standard 4-pre/4a/4b cycle; invoked by
//! `stage_send::run` (step 2d) after attempt #1 commits with
//! `decision.retry_class == MacRecovery`.
//!
//! **Four-step state machine (per freeze §4.4.1 + §4.4.3, with the
//! step 2/3 reorder per R-W10.4-senior-review LOW 5):**
//!   1. **Hash extraction (pure-fn)** — regex-extract `store {64hex}`
//!      from the wire `error_message`.  Failure ⇒
//!      [`MacRecoveryOutcome::HashNotExtractable`].  Counter is NOT
//!      claimed yet — preserves the budget if DPS shipped a malformed
//!      message.
//!   2. **MR-NO-TX read** — pool-bound read of recovery inputs (doc
//!      row + fn_config).  Runs BEFORE MR-CLAIM so that a transient
//!      read failure (missing row, malformed `previous_hash`) does
//!      NOT burn the single-bit budget; a future tick can still
//!      attempt recovery after the operator fixes the row.
//!      (R-W10.4-senior-review LOW 5 close; earlier ordering was
//!      CLAIM → read.)
//!   3. **MR-CLAIM** (`with_immediate` #1) + **re-sign** — atomic CAS
//!      on `(state == ERROR_RETRYABLE AND mac_recovery_attempts == 0)
//!      → mac_recovery_attempts = 1`; on failure ⇒
//!      [`MacRecoveryOutcome::CounterExhausted`] (caller emits
//!      `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH` audit).  On
//!      success: `stage_sign::re_sign_after_mac_recovery` runs
//!      OUTSIDE any tx (pure CPU + crypto, no DB writes) producing
//!      fresh canonical XML + sha256 + CMS signature.
//!   4. **MR-PERSIST** (`with_immediate` #2) — atomic four-write:
//!      Pre-PERSIST assertion (PAYLOAD_XML + SIGNED_XML must exist
//!      per W6 stage-3 invariant; missing ⇒
//!      [`StageSendError::MacRecoveryArtifactMissing`]) +
//!      `fiscal_documents.previous_hash` + `unsigned_xml_sha256` +
//!      [`document_files::replace_tx`] for both PAYLOAD_XML and
//!      SIGNED_XML + audit `MAC_RECOVERY_RESIGNED`.
//!
//! **W3 invariant.**  `regex_extract_store_hash`, `re_sign_after_mac_recovery`,
//! and the pool-bound read all run OUTSIDE any `with_immediate`
//! envelope.  The two write envelopes (MR-CLAIM, MR-PERSIST) contain
//! NO foreign IO.  W3 scanner test stays green.
//!
//! **HIGH 2 (atomicity).**  Counter claim and artifact rewrite live
//! in *different* `with_immediate` envelopes by design.  Crash between
//! them: doc remains in `ERROR_RETRYABLE` with `mac_recovery_attempts = 1`
//! and OLD artifacts.  Worker re-entry hits MR-CLAIM `rows_affected=0`
//! (counter already 1) ⇒ `CounterExhausted` ⇒ caller emits
//! `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH` ⇒ doc Rejected.  No silent
//! progression; partial state is forensically visible (no
//! `MAC_RECOVERY_RESIGNED` audit row ⇒ recovery never completed).
//!
//! **Loop bound.**  Caller (`stage_send::run`) tracks
//! `mac_recovery_invoked: bool` per invocation; orchestrator is called
//! AT MOST ONCE per `stage_send::run` call.  Combined with the
//! single-bit DDL `mac_recovery_attempts CHECK IN (0, 1)` budget, no
//! infinite-loop is reachable.

use sqlx::SqlitePool;

use crate::db::models::enums::{DocType, Severity};
use crate::db::models::ids::DocumentId;
use crate::db::repositories::{
    audit_log, document_files, document_files::DocumentFileKind, fiscal_documents,
    fiscal_number_config, signing_config_snapshots,
};
use crate::db::tx::with_immediate;
use crate::db::types::{DbDocType, DbDocumentId};

use super::error_routing::MacRecoveryHint;
use super::stage_send::StageSendError;
use super::stage_sign::{self, derive_wire_artifact_kind, SigningContext};

// ─── Outcome contract ────────────────────────────────────────────────

/// W10.4 step 2c — orchestrator outcome.
///
/// Caller (`stage_send::run`, step 2d) dispatches:
///
/// | Variant | Caller action |
/// |---|---|
/// | [`MacRecoveryOutcome::Resigned`] | re-enter 4-pre/4a/4b for attempt #2 |
/// | [`MacRecoveryOutcome::HashNotExtractable`] | override `wire_decision` to `TerminalReject`; CAS `ErrorRetryable → Rejected`; audit `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` (already emitted by orchestrator) |
/// | [`MacRecoveryOutcome::CounterExhausted`] | override to `TerminalReject`; CAS `ErrorRetryable → Rejected`; audit `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH` (caller emits) |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacRecoveryOutcome {
    /// Recovery completed successfully.  `previous_hash`,
    /// `unsigned_xml_sha256`, PAYLOAD_XML, SIGNED_XML all replaced;
    /// `MAC_RECOVERY_RESIGNED` audit row committed.  Doc remains in
    /// `ErrorRetryable` with `mac_recovery_attempts = 1`; caller
    /// re-enters Pattern B 4-pre to drive attempt #2.
    Resigned,
    /// Regex `r"store ([0-9a-fA-F]{64})"` did not match the wire
    /// `error_message`.  Counter NOT claimed; doc remains in
    /// `ErrorRetryable` with `mac_recovery_attempts = 0`.  Orchestrator
    /// emitted `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` audit; caller routes
    /// to `TerminalReject`.
    HashNotExtractable,
    /// MR-CLAIM CAS missed: state was not `ErrorRetryable` OR counter
    /// was already 1.  Triggered by: (a) second `-12` after a
    /// successful re-sign in the same `stage_send::run` call, or
    /// (b) crash recovery — counter burnt by a prior tick whose
    /// PERSIST never committed.  Either way the doc cannot be
    /// re-attempted; caller routes to `TerminalReject` and emits
    /// `MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH`.
    CounterExhausted,
    /// M2-X1 (external-critic HIGH, 2026-06-12): the doc is OFFLINE-ORIGIN
    /// (`offline_fiscal_no` NOT NULL) — its MAC chain was built LOCALLY and the
    /// seed already advanced at offline-ack (M2-01).  Re-signing it under a DPS
    /// `-12` hash would desync the local offline chain (ChainBreak).  Counter
    /// NOT claimed, NO re-sign; `MAC_RECOVERY_OFFLINE_ORIGIN_REFUSED` audit
    /// emitted.  Caller (`stage_send::run`) returns the original
    /// `Routed{MacRecovery}` outcome unchanged, which the drain routes to
    /// manual-recon escalation (mirror the M2-04 seam).  Unreachable for
    /// online-origin docs (`offline_fiscal_no` NULL → not this arm).
    OfflineOriginRefused,
}

// ─── Hash extraction (pure-fn) ───────────────────────────────────────

/// Extract a 64-hex `previous_hash` from a DPS wire `error_message`.
///
/// Pattern: the substring `"store "` followed by exactly 64
/// hexadecimal characters (case-insensitive — matches both upper and
/// lowercase, mirrors Python `dps_fiscal_server.py:494` regex
/// `r"store ([0-9a-fA-F]{64})"`).
///
/// Return:
/// - `Some([u8; 32])` — exactly 64 hex chars decoded big-endian.
/// - `None` — substring `"store "` absent, OR the next 64 chars are
///   not all hex, OR the message ends before 64 chars.
///
/// **Pure-fn, no allocation beyond the byte array.**  Hand-rolled
/// rather than `regex` crate to avoid the build-time dep + runtime
/// regex compile cost; the pattern is anchored at one substring and
/// never needs alternation / anchoring / capture groups.
pub fn regex_extract_store_hash(message: &str) -> Option<[u8; 32]> {
    const TAG: &str = "store ";
    let bytes = message.as_bytes();
    let tag = TAG.as_bytes();
    // Find "store " (case-sensitive — DPS wire format documented
    // lowercase in Python; if DPS ever ships uppercase, this returns
    // None and the orchestrator routes to HashNotExtractable, which
    // is the fail-loud behaviour we want for protocol drift).
    let mut i = 0usize;
    let start = loop {
        if i + tag.len() > bytes.len() {
            return None;
        }
        if &bytes[i..i + tag.len()] == tag {
            break i + tag.len();
        }
        i += 1;
    };
    if start + 64 > bytes.len() {
        return None;
    }
    let hex_slice = &bytes[start..start + 64];
    let mut out = [0u8; 32];
    for (idx, pair) in hex_slice.chunks_exact(2).enumerate() {
        let hi = decode_hex_nibble(pair[0])?;
        let lo = decode_hex_nibble(pair[1])?;
        out[idx] = (hi << 4) | lo;
    }
    Some(out)
}

fn decode_hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

// ─── Recovery inputs read ────────────────────────────────────────────

/// Pool-bound MR-NO-TX read.  Pulls everything `re_sign_after_mac_recovery`
/// needs from the doc row + fn config.  Returns `None` if the doc row
/// is missing — caller treats as `StageSendError::DocumentMissing`-class
/// breach.  All reads are outside any `with_immediate` envelope.
struct RecoveryInputs {
    fiscal_number: String,
    tax_number: String,
    doc_type: DocType,
    business_ts: String,
    payload_json: String,
    lnd: i64,
    total_sum_kop: Option<i64>,
    z_report_number: Option<i64>,
    /// Pre-recovery `previous_hash`, captured for the
    /// `MAC_RECOVERY_RESIGNED` audit payload's `old_previous_hash_hex`
    /// forensic field.
    old_previous_hash: Option<[u8; 32]>,
    /// W4-Z2a piece 9 — persisted snapshot FK for the MR-NO-TX
    /// re-sign step.  Locked rule #9: MAC recovery uses the
    /// snapshot the doc was originally pinned with, NEVER current
    /// config.  `None` for pre-W4-Z2a docs (migration-window
    /// back-compat — fall back to empty map = unchanged behaviour).
    signing_config_snapshot_id: Option<i64>,
    /// M2-X1 (external-critic HIGH, 2026-06-12) — `Some` ⟺ offline-origin doc
    /// (fiscalised at offline-ack, where M2-01 advanced the chain seed).  Such a
    /// doc must NOT be re-signed under a DPS hash (it would desync the locally
    /// built offline MAC chain → `invariant_scan` ChainBreak).  `run_mac_recovery`
    /// refuses it before MR-CLAIM.  `None` = online-origin (unchanged behaviour).
    offline_fiscal_no: Option<i64>,
}

async fn read_recovery_inputs(
    pool: &SqlitePool,
    doc_id: DocumentId,
) -> Result<RecoveryInputs, StageSendError> {
    // Runtime-bound `query_as` (not `query!`) to avoid the sqlx
    // compile-time cache regen for a single MAC-recovery read.
    type Row = (
        String,          // fiscal_number
        DbDocType,       // doc_type (store-side wrapper; converted to DocType below)
        String,          // business_ts
        String,          // payload_json
        i64,             // lnd
        Option<i64>,     // total_sum_kop
        Option<i64>,     // z_report_number
        Option<Vec<u8>>, // previous_hash
        Option<i64>,     // signing_config_snapshot_id (W4-Z2a piece 9)
        Option<i64>,     // offline_fiscal_no (M2-X1 offline-origin guard)
    );
    let row: Option<Row> = sqlx::query_as(
        r#"SELECT fiscal_number, doc_type, business_ts, payload_json,
                  lnd, total_sum_kop, z_report_number, previous_hash,
                  signing_config_snapshot_id, offline_fiscal_no
           FROM fiscal_documents WHERE document_id = ?"#,
    )
    .bind(DbDocumentId(doc_id))
    .fetch_optional(pool)
    .await
    .map_err(StageSendError::Db)?;
    let row = row.ok_or(StageSendError::DocumentMissingForRecovery {
        document_id: doc_id,
    })?;
    let (
        fn_id,
        doc_type,
        business_ts,
        payload_json,
        lnd,
        total_sum_kop,
        z_report_number,
        previous_hash_bytes,
        signing_config_snapshot_id,
        offline_fiscal_no,
    ) = row;
    // Convert the store-side wrapper back to the pure domain enum so the rest of
    // recovery (and the `RecoveryInputs { doc_type: DocType, .. }` field) is
    // unchanged (CS-1b).
    let doc_type: DocType = doc_type.0;

    let tax_number = match fiscal_number_config::get(pool, &fn_id)
        .await
        .map_err(StageSendError::Db)?
    {
        Some(c) => c.tax_number,
        None => {
            return Err(StageSendError::FnConfigMissingForRecovery {
                fn_id,
                document_id: doc_id,
            });
        }
    };

    let old_previous_hash = match previous_hash_bytes {
        Some(v) if v.len() == 32 => {
            let mut a = [0u8; 32];
            a.copy_from_slice(&v);
            Some(a)
        }
        Some(v) => {
            return Err(StageSendError::Db(sqlx::Error::Decode(
                format!(
                    "fiscal_documents.previous_hash: expected 32 bytes, got {}",
                    v.len()
                )
                .into(),
            )));
        }
        None => None,
    };

    Ok(RecoveryInputs {
        fiscal_number: fn_id,
        tax_number,
        doc_type,
        business_ts,
        payload_json,
        lnd,
        total_sum_kop,
        z_report_number,
        old_previous_hash,
        signing_config_snapshot_id,
        offline_fiscal_no,
    })
}

// ─── Orchestrator entry point ────────────────────────────────────────

/// Run the MAC-recovery cycle for `doc`.  Caller (`stage_send::run`)
/// invokes AT MOST ONCE per `run()` invocation; the
/// `mac_recovery_invoked: bool` flag in `stage_send` enforces that
/// loop bound.
///
/// See module docstring for the full state machine.  Caller dispatches
/// on the returned [`MacRecoveryOutcome`] per the table in the enum
/// docs.
///
/// # Caller obligation: single-writer-per-FN invariant
///
/// (R-W10.4-senior-review MED 2 close; W10 post-merge audit MED-1
/// close — see ADR-M3-A10 at
/// `docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md`.)
///
/// The orchestrator's MR-CLAIM and MR-PERSIST envelopes do **NOT**
/// acquire any per-FN lock — no such primitive exists in M3a (see
/// ADR-M3-A10).  They rely on the M3a runtime invariant that at most
/// one writer mutates state for a given `fiscal_number` at any
/// moment.
///
/// Today that invariant is enforced by the strictly stronger
/// **global-single-writer** model: one tokio worker drives the
/// write-path orchestrator, and every write transaction is wrapped
/// in `with_immediate` (SQLite `BEGIN IMMEDIATE`), which serialises
/// all writers globally on the WAL writer.  Between MR-CLAIM commit
/// and MR-PERSIST begin no other writer transaction of any kind can
/// run, regardless of FN.
///
/// **Production caller `stage_send::run`** invokes the orchestrator
/// inside its single-worker run, so the invariant is satisfied by
/// construction.  Ad-hoc callers (admin tools, test harnesses, W9
/// boot reconciliation) inherit the same guarantee because they
/// run in the same single-worker context.
///
/// **What would go wrong if the invariant were broken.**  Two
/// parallel callers on the same doc, assuming the invariant fails
/// (e.g. a future multi-worker dispatcher without FN-scope
/// exclusion):
///   - First MR-CLAIM wins, transitions counter 0→1.
///   - Second MR-CLAIM CAS fails (counter already 1) ⇒
///     `CounterExhausted`.
///   - Doc state stays `ErrorRetryable`; first caller's PERSIST
///     proceeds normally; second caller's caller observes
///     `CounterExhausted` and routes to `TerminalReject`.
///
/// SQLite's BEGIN IMMEDIATE serialisation makes this **functionally
/// safe today** — no torn state, no double-spend.  But any slice
/// that introduces concurrent writers (multi-worker dispatcher,
/// counter claim moved outside a tx for performance) MUST add a
/// real FN-scope exclusion primitive per ADR-M3-A10 §4.
pub async fn run_mac_recovery(
    pool: &SqlitePool,
    ctx: &SigningContext,
    doc: DocumentId,
    hint: &MacRecoveryHint,
) -> Result<MacRecoveryOutcome, StageSendError> {
    // ── Step 1: hash extraction (pure-fn) ───────────────────────────
    let new_previous_hash = match regex_extract_store_hash(&hint.raw_error_message) {
        Some(h) => h,
        None => {
            // Emit MAC_RECOVERY_HASH_NOT_EXTRACTABLE audit + return.
            // Counter NOT claimed; doc retains attempts=0 so a future
            // tick (e.g. after operator fixes the DPS message format)
            // can still attempt recovery.
            emit_hash_not_extractable_audit(pool, doc, &hint.raw_error_message).await?;
            return Ok(MacRecoveryOutcome::HashNotExtractable);
        }
    };

    // ── Step 2: MR-NO-TX read (BEFORE MR-CLAIM) ─────────────────────
    //
    // R-W10.4-senior-review LOW 5 close: read recovery inputs BEFORE
    // claiming the budget.  If the doc / fn_config row went missing
    // (race with delete) OR `previous_hash` is malformed, we surface
    // a typed error WITHOUT burning the single-bit budget — a future
    // tick (after operator restores the row / config) can still
    // attempt recovery.  Earlier ordering (CLAIM → read) permanently
    // spent the counter on transient read failures.
    //
    // Reordering is safe under the M3a single-writer-per-FN
    // invariant (see ADR-M3-A10): today's global-single-writer +
    // BEGIN IMMEDIATE guarantees no other writer can mutate these
    // inputs between this read and the MR-CLAIM CAS.
    let inputs = read_recovery_inputs(pool, doc).await?;

    // ── M2-X1 guard (external-critic HIGH, 2026-06-12): REFUSE offline-origin
    //    docs BEFORE MR-CLAIM.  An offline doc fiscalised at offline-ack, where
    //    M2-01 already advanced the chain seed; re-signing it under a DPS `-12`
    //    hash would desync the locally built offline MAC chain (doc#2.prev goes
    //    stale → invariant_scan ChainBreak).  W10.4 mac-recovery was designed for
    //    ONLINE docs (A.3: the seed advances at SEND, and a recovered doc
    //    re-anchors via the Variant P branch — its re-send skips the equality
    //    gate and reseeds to the re-signed sha); offline chains are built
    //    locally and must NEVER be re-signed under a DPS hash on the fly.  Counter
    //    NOT claimed, NO re-sign — return OfflineOriginRefused, which the caller
    //    routes to manual-recon escalation (mirror M2-04).
    if inputs.offline_fiscal_no.is_some() {
        emit_offline_origin_refused_audit(pool, doc, inputs.offline_fiscal_no).await?;
        return Ok(MacRecoveryOutcome::OfflineOriginRefused);
    }

    let wire_artifact_kind = derive_wire_artifact_kind(inputs.doc_type)
        .map_err(StageSendError::MacRecoverySignFailed)?;

    // ── Step 3: MR-CLAIM ────────────────────────────────────────────
    let claimed: bool = with_immediate(pool, move |tx| {
        Box::pin(async move {
            fiscal_documents::mac_recovery_claim_counter_tx(tx, doc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .map_err(bridge_anyhow)?;
    if !claimed {
        // State was not ERROR_RETRYABLE OR counter already 1.
        // Caller emits MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH audit
        // alongside the override-to-TerminalReject path.
        return Ok(MacRecoveryOutcome::CounterExhausted);
    }
    // W4-Z2a piece 9 + 14 — reload persisted snapshot via doc FK
    // BEFORE re-sign.  Locked rule #9: MAC recovery uses the
    // snapshot the doc was originally pinned with, NEVER current
    // config (would catastrophically break MAC chain on retry).
    // Pool-bound read outside any with_immediate envelope;
    // `get_by_id` verifies SHA-256 and rejects non-V1 kind
    // variants.  Pre-W4-Z2a docs (FK NULL) → None → empty path
    // (back-compat).  Piece 14 passes the full snapshot (was just
    // to_calc_map() result) so check_payload_from can translate
    // driver_number → canonical TX.
    let mr_tax_resolution = match inputs.signing_config_snapshot_id {
        Some(id) => Some(
            signing_config_snapshots::get_by_id(pool, id)
                .await
                .map_err(StageSendError::MacRecoverySnapshotReloadFailed)?,
        ),
        None => None,
    };

    let resigned = stage_sign::re_sign_after_mac_recovery(
        ctx,
        wire_artifact_kind,
        &inputs.fiscal_number,
        &inputs.tax_number,
        &inputs.business_ts,
        &inputs.payload_json,
        inputs.total_sum_kop,
        inputs.lnd,
        inputs.z_report_number,
        new_previous_hash,
        mr_tax_resolution,
    )
    .await
    .map_err(StageSendError::MacRecoverySignFailed)?;

    // ── Step 4: MR-PERSIST (atomic four-write) ──────────────────────
    //
    // R-W10.4-senior-review LOW 4 close: `old_previous_hash` is
    // serialised as JSON `null` when the doc had no prior chain (NULL
    // column).  Earlier implementation rendered `""` (empty string),
    // which is ambiguous between "no prior hash" and "decode-failed-
    // to-empty".  JSON null is unambiguous "absent".
    let old_previous_hash_field: serde_json::Value = match inputs.old_previous_hash {
        Some(h) => serde_json::Value::String(hex_lower(Some(&h[..]))),
        None => serde_json::Value::Null,
    };
    let new_hash_hex = hex_lower(Some(&new_previous_hash[..]));
    let new_sha_hex = hex_lower(Some(&resigned.unsigned_xml_sha256[..]));
    let payload_xml = resigned.unsigned_xml.clone();
    let signed_xml = resigned.signed_xml_cms.0.clone();
    let new_sha_for_persist = resigned.unsigned_xml_sha256;

    with_immediate(pool, move |tx| {
        let payload_xml = payload_xml.clone();
        let signed_xml = signed_xml.clone();
        let old_previous_hash_field = old_previous_hash_field.clone();
        let new_hash_hex = new_hash_hex.clone();
        let new_sha_hex = new_sha_hex.clone();
        Box::pin(async move {
            // ── Pre-PERSIST assertion (R-W10.4-step2a-review LOW 3) ──
            // W6 stage-3 invariant: PAYLOAD_XML + SIGNED_XML are
            // INSERTed before stage 4 ever runs, so by the time MAC
            // recovery is invoked both rows MUST exist.  If either
            // is missing, the doc is structurally broken — surface a
            // typed error and roll back the entire MR-PERSIST envelope
            // before invoking `replace_tx` (which would otherwise
            // silently INSERT the new artifact, masking the breach).
            if document_files::get_tx(tx, doc, DocumentFileKind::PayloadXml)
                .await?
                .is_none()
            {
                return Err(anyhow::Error::new(
                    StageSendError::MacRecoveryArtifactMissing {
                        document_id: doc,
                        kind: DocumentFileKind::PayloadXml,
                    },
                ));
            }
            if document_files::get_tx(tx, doc, DocumentFileKind::SignedXml)
                .await?
                .is_none()
            {
                return Err(anyhow::Error::new(
                    StageSendError::MacRecoveryArtifactMissing {
                        document_id: doc,
                        kind: DocumentFileKind::SignedXml,
                    },
                ));
            }

            // ── Atomic four-write ──────────────────────────────────
            // 1. fiscal_documents.previous_hash + unsigned_xml_sha256.
            sqlx::query(
                "UPDATE fiscal_documents SET \
                    previous_hash = ?, \
                    unsigned_xml_sha256 = ? \
                 WHERE document_id = ?",
            )
            .bind(&new_previous_hash[..])
            .bind(&new_sha_for_persist[..])
            .bind(DbDocumentId(doc))
            .execute(&mut **tx)
            .await?;

            // 2. document_files.PAYLOAD_XML.
            document_files::replace_tx(tx, doc, DocumentFileKind::PayloadXml, &payload_xml).await?;

            // 3. document_files.SIGNED_XML.
            document_files::replace_tx(tx, doc, DocumentFileKind::SignedXml, &signed_xml).await?;

            // 4. Audit MAC_RECOVERY_RESIGNED with forensic correlation.
            let payload = serde_json::json!({
                "old_previous_hash_hex": old_previous_hash_field,
                "new_previous_hash_hex": new_hash_hex,
                "new_unsigned_xml_sha256_hex": new_sha_hex,
            })
            .to_string();
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                "MAC_RECOVERY_RESIGNED",
                Severity::Warning,
                None,
                Some(&payload),
            )
            .await?;

            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    Ok(MacRecoveryOutcome::Resigned)
}

// ─── Helpers ─────────────────────────────────────────────────────────

/// Emit the `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` audit row in its own
/// `with_immediate` envelope (no other writes — the doc state is
/// untouched on this path; counter NOT claimed).
/// M2-X1 — forensic audit when mac-recovery refuses an offline-origin doc.
/// Distinct event_type + `manual_recon_class: true` + the offline ordinal, so an
/// operator dashboard sees exactly why the FN drain escalated (the drain also
/// emits its own `OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL`).
async fn emit_offline_origin_refused_audit(
    pool: &SqlitePool,
    doc: DocumentId,
    offline_fiscal_no: Option<i64>,
) -> Result<(), StageSendError> {
    let payload = serde_json::json!({
        "failure_class": "mac_recovery_offline_origin",
        "manual_recon_class": true,
        "offline_fiscal_no": offline_fiscal_no,
        "rationale":
            "mac-recovery refuses offline-origin docs — the offline MAC chain is built locally \
             and the seed already advanced at offline-ack (M2-01); re-signing under a DPS hash \
             would desync the chain. Escalated to manual reconciliation, no re-sign.",
    })
    .to_string();
    with_immediate(pool, move |tx| {
        let payload = payload.clone();
        Box::pin(async move {
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                "MAC_RECOVERY_OFFLINE_ORIGIN_REFUSED",
                Severity::Error,
                None,
                Some(&payload),
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .map_err(bridge_anyhow)?;
    Ok(())
}

async fn emit_hash_not_extractable_audit(
    pool: &SqlitePool,
    doc: DocumentId,
    raw_error_message: &str,
) -> Result<(), StageSendError> {
    // Truncate the raw message to keep audit_log payload small;
    // forensics already has the full message via transport_trace.
    // R-W10.4-step2c-review LOW 4 close: byte-bounded UTF-8 safe
    // truncation via `stage_send::truncate_msg` (≤ 512 bytes,
    // codepoint integrity preserved) — consistent with the
    // transport_trace.error_message CHECK convention rather than
    // an ad-hoc char-bounded `take(256)`.
    let truncated = super::stage_send::truncate_msg(raw_error_message);
    let payload = serde_json::json!({
        "raw_error_message_truncated": truncated,
    })
    .to_string();
    with_immediate(pool, move |tx| {
        let payload = payload.clone();
        Box::pin(async move {
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc:?}"),
                "MAC_RECOVERY_HASH_NOT_EXTRACTABLE",
                Severity::Error,
                None,
                Some(&payload),
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .map_err(bridge_anyhow)?;
    Ok(())
}

/// Lowercase hex with `Option<&[u8]>` shape (None → empty string).
/// Wraps the shared [`super::types::hex_encode_lower`]
/// (R-W10.4-senior-review LOW 2 close).  Audit payload composition
/// уже uses JSON null instead of empty for missing prior hash
/// (LOW 4 close), so the empty-string fallback here is reserved for
/// non-audit call sites that prefer empty strings in formatted
/// output.
fn hex_lower(bytes: Option<&[u8]>) -> String {
    bytes
        .map(super::types::hex_encode_lower)
        .unwrap_or_default()
}

/// Bridge `anyhow::Error` from `with_immediate` closures back to typed
/// `StageSendError`.  Thin wrapper over the shared
/// [`super::types::bridge_anyhow_to`] (R-W10.4-senior-review LOW 1
/// close — deduplicated from three modules to one shared helper).
fn bridge_anyhow(e: anyhow::Error) -> StageSendError {
    super::types::bridge_anyhow_to(e, StageSendError::Db, StageSendError::Internal)
}

// ─── Unit tests for the pure-fn surface ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Exactly 64-hex sample hashes — used across the regex tests.
    const HEX64_LOWER: &str = "deadbeef0123456789abcdef0123456789abcdef0123456789abcdef01234567";
    const HEX64_UPPER: &str = "DEADBEEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF01234567";

    #[test]
    fn regex_extract_happy_lowercase() {
        let msg = format!("ERROR_BAD_HASH_PREV: store {HEX64_LOWER}");
        let h = regex_extract_store_hash(&msg).expect("must extract");
        assert_eq!(h[0], 0xde);
        assert_eq!(h[1], 0xad);
        // Last byte = decode of "67" (positions 62-63 in the hex run).
        assert_eq!(h[31], 0x67);
    }

    #[test]
    fn regex_extract_happy_uppercase() {
        let msg = format!("store {HEX64_UPPER}");
        let h = regex_extract_store_hash(&msg).expect("must extract uppercase too");
        assert_eq!(h[0], 0xde);
        assert_eq!(h[31], 0x67);
    }

    #[test]
    fn regex_extract_happy_mixed_case() {
        // Python regex is case-insensitive; we mirror it.  Mixed-case
        // 64 hex chars (alternating cases on the alphabetic nibbles).
        let msg = "store DeAdBeEf0123456789AbCdEf0123456789AbCdEf0123456789AbCdEf01234567";
        assert!(regex_extract_store_hash(msg).is_some());
    }

    #[test]
    fn regex_extract_no_store_substring_returns_none() {
        let msg = "ERROR_BAD_HASH_PREV: hash mismatch deadbeef...";
        assert!(regex_extract_store_hash(msg).is_none());
    }

    #[test]
    fn regex_extract_short_hex_after_store_returns_none() {
        // Only 60 hex chars after "store " (4 short of the required 64).
        let msg = "store deadbeef0123456789abcdef0123456789abcdef0123456789abcdef0123";
        assert_eq!(
            (msg.len() - 6),
            60,
            "fixture sanity: 60 hex chars after 'store '"
        );
        assert!(regex_extract_store_hash(msg).is_none());
    }

    #[test]
    fn regex_extract_non_hex_in_payload_returns_none() {
        // 64 chars but with a non-hex 'g'.
        let msg = "store gggdbeef0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab";
        assert!(regex_extract_store_hash(msg).is_none());
    }

    #[test]
    fn regex_extract_empty_message_returns_none() {
        assert!(regex_extract_store_hash("").is_none());
    }

    #[test]
    fn regex_extract_finds_hash_after_first_store_occurrence() {
        // Two "store " substrings; we take the first.
        let msg = "store aabb...store ccdd...";
        // Neither is a complete 64-hex run; both fail.  But the
        // function tries the FIRST match — so if the first fails,
        // None is returned (we don't backtrack to the second).
        // This pin documents the contract.
        assert!(regex_extract_store_hash(msg).is_none());
    }

    #[test]
    fn regex_extract_decodes_byte_order_correctly() {
        let msg = "store 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let h = regex_extract_store_hash(msg).unwrap();
        assert_eq!(h[0], 0x00);
        assert_eq!(h[1], 0x11);
        assert_eq!(h[7], 0x77);
        assert_eq!(h[15], 0xff);
        assert_eq!(h[16], 0x00);
        assert_eq!(h[31], 0xff);
    }

    #[test]
    fn outcome_variants_are_distinct() {
        // Sanity for closed-enum dispatch: PartialEq pins.
        assert_ne!(
            MacRecoveryOutcome::Resigned,
            MacRecoveryOutcome::HashNotExtractable
        );
        assert_ne!(
            MacRecoveryOutcome::Resigned,
            MacRecoveryOutcome::CounterExhausted
        );
        assert_ne!(
            MacRecoveryOutcome::HashNotExtractable,
            MacRecoveryOutcome::CounterExhausted
        );
    }

    #[test]
    fn hex_lower_round_trips() {
        assert_eq!(hex_lower(None), "");
        assert_eq!(hex_lower(Some(&[])), "");
        assert_eq!(hex_lower(Some(&[0x00, 0xff, 0xab])), "00ffab");
        assert_eq!(hex_lower(Some(&[0xde, 0xad, 0xbe, 0xef])), "deadbeef");
    }
}
