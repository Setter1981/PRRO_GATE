//! Stage 3 — sign (Pattern A: compute outside, persist inside).
//!
//! Per W0-1 §3.3, ADR-M3-A2 (CloseShift→ZReport at builder; Z-allocation
//! by `wire_artifact_kind`), ADR-M3-A5 (Pattern A), and the W6 design
//! spec at `docs/superpowers/specs/2026-05-09-m3a-w6-stage3-sign-design.md`.
//!
//! Four sub-stages:
//! 1. **3-PRE-READ** — pool read of `fiscal_number_config` for tax_number.
//!    Not part of the chain-pinning invariant; kept out of the write tx.
//! 2. **3-PRE** — short `with_immediate` envelope.  State gate
//!    (`state == Prepared` else `SignError::StateConflict`) → pin-or-
//!    reuse branch (atomic UPDATE writing `previous_hash`,
//!    `z_report_number`, `signing_inputs_pinned_at`).  On retry the
//!    persisted values are reused; the Z allocator is NOT called twice.
//! 3. **3-NO-TX** — outside any lock: typed-payload parse, Kyiv-local
//!    TS formatting, canonical XML build, sha256, `sign_cms_detached`.
//!    W3 static scan + runtime guard ensure `sign_cms_detached` is
//!    never reached from inside `with_immediate`.
//! 4. **3-PERSIST** — second `with_immediate` envelope: CAS
//!    `Prepared→Signed`, INSERT `PAYLOAD_XML` + `SIGNED_XML`, UPDATE
//!    `unsigned_xml_sha256`, append `doc_signed` audit row.
//!
//! No DPS send, no `Sending` transition, no finalize, no `App::boot`
//! recovery, no cert/session loading — all out of W6 scope.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Kiev;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::crypto::errors::CryptoError;
use crate::crypto::provider::{CryptoProvider, SignCmsRequest, SignedCmsBytes};
use crate::crypto::session::SigningSession;
use crate::db::models::{
    enums::{DocState, DocType, Severity},
    ids::DocumentId,
};
use crate::db::repositories::{
    audit_log, document_files,
    document_files::DocumentFileKind,
    fiscal_documents::{self as fd, DocumentRow, TransitionOutcome},
    fiscal_number_config as fn_config, node_state,
};
use crate::db::tx::with_immediate;
use crate::xml::{
    build_canonical_xml, AdjustmentMode, CanonicalDoc, CheckItem, CheckLevelAdjustment,
    CheckLevelAdjustmentKind, CheckPayload, CheckPayment, DocumentHeader, LineAdjustment,
    LineAdjustmentKind, ZReportCheckCount, ZReportPayload, ZReportPaymentSum,
};

use super::tax_summary::{derive_check_tax_summaries, ResolvedTaxGroup};
use super::types::WorkerContext;

use std::collections::HashMap;

// ─── Public types ─────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireArtifactKind {
    ShiftOpen,
    Sell,
    Return,
    ZReport,
}

pub struct SigningContext {
    pub provider: Arc<dyn CryptoProvider>,
    pub session: SigningSession,
    pub profile: prro_crypto::cms::profile::CmsProfile,
}

#[derive(Debug, Clone)]
pub struct SigningOutcome {
    pub document: DocumentRow,
    pub signed_payload: SignedCmsBytes,
    pub unsigned_xml: Vec<u8>,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SignError {
    #[error("stage 3-PRE state conflict: doc {document_id:?} observed in {observed:?}, expected Prepared")]
    StateConflict {
        observed: DocState,
        document_id: DocumentId,
    },
    #[error("stage 3 unsupported doc type: {doc_type:?}")]
    UnsupportedDocType { doc_type: DocType },
    #[error("stage 3 row missing for doc_id {document_id:?}")]
    RowMissing { document_id: DocumentId },
    #[error("stage 3-PRE-READ fn_config missing for fiscal_number {fn_id}")]
    FnConfigMissing { fn_id: String },
    #[error("stage 3-PRE node_state missing for fiscal_number {fn_id}")]
    NodeStateMissing { fn_id: String },
    #[error("stage 3-NO-TX payload schema mismatch: {detail}")]
    PayloadSchema { detail: String },
    #[error("stage 3 numeric range: {field} = {value} out of u32 range")]
    Range { field: &'static str, value: i64 },
    #[error("stage 3-NO-TX timestamp conversion: {detail}")]
    TimestampConversion { detail: String },
    #[error("stage 3-PERSIST CAS Prepared→Signed unexpected outcome: {outcome:?}")]
    PersistCasFailed { outcome: TransitionOutcome },
    #[error("stage 3-NO-TX canonical XML build failed: {0}")]
    Build(#[from] crate::xml::XmlBuildError),
    /// W4-Z1 piece 7 — `derive_check_tax_summaries` returned a typed
    /// `CalcTaxError`: UnsupportedAlgorithm / InvalidRate /
    /// IntermediateOverflow / AggregationOverflow / TaxMappingNotWired.
    /// Surface to audit_log + reject doc (deterministic config /
    /// payload defect — NEVER retry).
    #[error("stage 3-NO-TX tax-summary aggregation failed: {0}")]
    TaxSummary(#[from] crate::xml::CalcTaxError),
    #[error("stage 3-NO-TX CMS sign failed: {0}")]
    Crypto(#[from] CryptoError),
    #[error("stage 3 db error: {0}")]
    Db(#[from] sqlx::Error),
    /// Catch-all for `with_immediate` envelopes whose closure body
    /// surfaces non-sqlx anyhow chains.  Cause chain preserved.
    #[error("stage 3 internal: {0}")]
    Internal(anyhow::Error),
}

// ─── Public derivation helper (also used by tests) ───────────────────

pub fn derive_wire_artifact_kind(doc_type: DocType) -> Result<WireArtifactKind, SignError> {
    match doc_type {
        DocType::ShiftOpen => Ok(WireArtifactKind::ShiftOpen),
        DocType::Sell => Ok(WireArtifactKind::Sell),
        DocType::Return => Ok(WireArtifactKind::Return),
        DocType::ShiftClose | DocType::ZReport => Ok(WireArtifactKind::ZReport),
        // W4 builder ships ShiftOpen/Sell/Return/ZReport only.  Other
        // op-types are not signable in W6: fail-closed BEFORE any
        // pin / Z allocation / state mutation occurs.
        DocType::ServiceIn | DocType::ServiceOut | DocType::CashWithdrawal | DocType::XReport => {
            Err(SignError::UnsupportedDocType { doc_type })
        }
    }
}

// ─── Ordering hook (Pattern A proof aid) ─────────────────────────────
//
// Always-on but inert in production: `record_persist_first` is a noop
// unless an integration test has called `test_hook::install`.  The
// counter is `OnceLock`-protected so production threads never observe
// a stable installed hook.  W6 stage 7 fixtures install a hook and
// assert `sign_call_seq < persist_first_stmt_seq` to prove
// Pattern A timestamp ordering structurally.

pub mod test_hook {
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Monotonic counter shared between the spy crypto provider and
    /// stage 3-PERSIST.  Production never reads/writes these (no
    /// caller path).  Fixtures `reset()` at the start of each test
    /// to keep ordering observations independent.
    pub static COUNTER: AtomicUsize = AtomicUsize::new(0);
    /// Order at which stage 3-PERSIST runs its first inside-tx
    /// statement (the CAS Prepared→Signed).  `0` means the persist
    /// tx never started; nonzero is the strict-monotonic position.
    pub static PERSIST_FIRST_SEQ: AtomicUsize = AtomicUsize::new(0);

    /// Reset both atomics.  Call at the start of every fixture that
    /// asserts `sign_call_seq < persist_first_stmt_seq`.
    pub fn reset() {
        COUNTER.store(0, Ordering::Release);
        PERSIST_FIRST_SEQ.store(0, Ordering::Release);
    }

    /// Recorded by stage_sign as the FIRST statement of the persist
    /// envelope.  Spy crypto provider records the sign call into
    /// `COUNTER` directly via `fetch_add`, so spy_seq < persist_seq
    /// proves Pattern A ordering structurally.
    pub fn record_persist_first() {
        let n = COUNTER.fetch_add(1, Ordering::AcqRel) + 1;
        PERSIST_FIRST_SEQ.store(n, Ordering::Release);
    }
}

// ─── Public entry point ───────────────────────────────────────────────

pub async fn run(
    pool: &SqlitePool,
    ctx: &SigningContext,
    incoming: WorkerContext,
) -> Result<SigningOutcome, SignError> {
    let WorkerContext {
        command, document, ..
    } = incoming;
    let doc_id = document.document_id;
    let fn_id = document.fiscal_number.clone();
    let lnd = document.lnd;
    let business_ts = command.business_ts.clone();
    let total_sum_kop = command.total_sum_kop;
    let payload_json = command.payload_json.clone();
    let wire_artifact_kind = derive_wire_artifact_kind(command.doc_type)?;

    // ─── Stage 3-PRE-READ ─────────────────────────────────────────────
    //
    // tax_number is config-table data; not part of chain-pinning.  Pool
    // read keeps the 3-PRE write tx minimal.
    let tax_number = match fn_config::get(pool, &fn_id).await? {
        Some(c) => c.tax_number,
        None => return Err(SignError::FnConfigMissing { fn_id }),
    };

    // ─── Stage 3-PRE ──────────────────────────────────────────────────
    //
    // State gate + pin-or-reuse atomically inside a single envelope.
    // Clone fn_id BEFORE with_immediate so the second argument is a
    // bare inline `Expr::Closure` — the W3 static scan rejects any
    // non-inline form (block expression, variable, fn pointer) since
    // those bypass closure-body inspection.
    let fn_id_for_pin = fn_id.clone();
    let pinned: PinResult = with_immediate(pool, move |tx| {
        let fn_id = fn_id_for_pin;
        Box::pin(async move {
            let inputs = match fd::get_signing_inputs_tx(tx, doc_id).await? {
                Some(p) => p,
                None => return Ok::<PinResult, anyhow::Error>(PinResult::RowMissing),
            };

            if inputs.state != DocState::Prepared {
                return Ok(PinResult::StateConflict {
                    observed: inputs.state,
                });
            }

            if inputs.is_pinned {
                // Reuse persisted inputs — do NOT re-read seed,
                // do NOT re-allocate Z.
                return Ok(PinResult::Reused {
                    previous_hash: inputs.previous_hash,
                    z_report_number: inputs.z_report_number,
                });
            }

            // First pin: read seed inside this tx (snapshot is
            // commit-stable; W6-intentional amendment to W0-1 §3.3
            // read timing — see design spec §1 anchors).
            let ns = match node_state::get_tx(tx, &fn_id).await? {
                Some(r) => r,
                None => return Ok(PinResult::NodeStateMissing),
            };
            let seed = ns.last_known_unsigned_xml_sha256;

            let z = match wire_artifact_kind {
                WireArtifactKind::ZReport => {
                    Some(node_state::allocate_z_report_number(tx, &fn_id).await?)
                }
                _ => None,
            };

            // W4-Z2a piece 4: pin_signing_inputs_tx signature widened with
            // signing_config_snapshot_id.  Piece 8 will populate from
            // WorkerContext after stage_acquire loads + inserts snapshot.
            // For piece 4 we pass None — preserves existing behaviour
            // (column stays NULL); back-compat semantic per locked design:
            // NULL + no tax_group_1 items → ALLOW (this commit's path).
            let rows = fd::pin_signing_inputs_tx(tx, doc_id, seed.as_ref(), z, None).await?;

            if rows == 0 {
                // Pin-once guard rejected: re-read truth.  Either
                // a concurrent re-entry pinned (rare; reuse) or
                // state moved (StateConflict).
                let after = fd::get_signing_inputs_tx(tx, doc_id).await?;
                return Ok(match after {
                    Some(p) if p.state != DocState::Prepared => {
                        PinResult::StateConflict { observed: p.state }
                    }
                    Some(p) if p.is_pinned => PinResult::Reused {
                        previous_hash: p.previous_hash,
                        z_report_number: p.z_report_number,
                    },
                    Some(_) => PinResult::PinLost,
                    None => PinResult::RowMissing,
                });
            }

            let payload_json = format!(
                r#"{{"seed_was_none":{},"z_allocated":{}}}"#,
                seed.is_none(),
                z.is_some()
            );
            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc_id:?}"),
                "sign_inputs_pinned",
                Severity::Info,
                None,
                Some(&payload_json),
            )
            .await?;

            Ok(PinResult::Pinned {
                previous_hash: seed,
                z_report_number: z,
            })
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    let (previous_hash_raw, z_report_number) = match pinned {
        PinResult::Pinned {
            previous_hash,
            z_report_number,
        }
        | PinResult::Reused {
            previous_hash,
            z_report_number,
        } => (previous_hash, z_report_number),
        PinResult::StateConflict { observed } => {
            return Err(SignError::StateConflict {
                observed,
                document_id: doc_id,
            })
        }
        PinResult::RowMissing | PinResult::PinLost => {
            return Err(SignError::RowMissing {
                document_id: doc_id,
            })
        }
        PinResult::NodeStateMissing => return Err(SignError::NodeStateMissing { fn_id }),
    };

    // ─── Stage 3-NO-TX ────────────────────────────────────────────────
    //
    // OUTSIDE any lock: typed parse, Kyiv-local TS, build, sha256, sign.

    // R-W10.4-step2b-review MED 1 close: stage 3-NO-TX body shared
    // with the W10.4 MAC recovery path via `build_canonical_and_sign_no_tx`.
    // Single source of truth — drift between W6 sign + recovery
    // re-sign is no longer possible.
    let (unsigned_xml, unsigned_xml_sha256, signed_payload) =
        build_canonical_and_sign_no_tx(NoTxBuildSignInputs {
            ctx,
            wire_artifact_kind,
            fn_id: &fn_id,
            tax_number: &tax_number,
            business_ts: &business_ts,
            payload_json: &payload_json,
            total_sum_kop,
            lnd,
            z_report_number,
            previous_hash: previous_hash_raw.as_ref(),
        })
        .await?;

    // ─── Stage 3-PERSIST ──────────────────────────────────────────────
    //
    // Second `with_immediate` envelope: CAS, files, hash UPDATE, audit.

    let signed_bytes = signed_payload.0.clone();
    let unsigned_xml_for_persist = unsigned_xml.clone();
    let persist_outcome: TransitionOutcome = with_immediate(pool, move |tx| {
        Box::pin(async move {
            // Always-on but inert in production (no caller reads
            // test_hook counters).  See `test_hook` module doc.
            test_hook::record_persist_first();

            let outcome =
                fd::transition_state(tx, doc_id, DocState::Prepared, DocState::Signed).await?;
            if outcome != TransitionOutcome::Applied {
                return Ok::<TransitionOutcome, anyhow::Error>(outcome);
            }

            document_files::insert_tx(
                tx,
                doc_id,
                DocumentFileKind::PayloadXml,
                &unsigned_xml_for_persist,
            )
            .await?;
            document_files::insert_tx(tx, doc_id, DocumentFileKind::SignedXml, &signed_bytes)
                .await?;

            fd::update_unsigned_xml_sha256_tx(tx, doc_id, &unsigned_xml_sha256).await?;

            audit_log::append_tx(
                tx,
                "fiscal_document",
                &format!("{doc_id:?}"),
                "doc_signed",
                Severity::Info,
                None,
                Some(&format!(
                    r#"{{"lnd":{lnd},"unsigned_xml_sha256":"{}"}}"#,
                    hex_encode(&unsigned_xml_sha256)
                )),
            )
            .await?;

            Ok(TransitionOutcome::Applied)
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    if persist_outcome != TransitionOutcome::Applied {
        return Err(SignError::PersistCasFailed {
            outcome: persist_outcome,
        });
    }

    // Build outcome snapshot.  All fields are deterministic from inputs
    // we already have; no re-SELECT needed.
    let now_iso = chrono::Utc::now().to_rfc3339();
    let document = DocumentRow {
        document_id: document.document_id,
        fiscal_number: document.fiscal_number,
        lnd: document.lnd,
        state: DocState::Signed,
        doc_type: document.doc_type,
        server_fiscal_no: document.server_fiscal_no,
        submission_attempted_at: document.submission_attempted_at,
        backend_profile_id: document.backend_profile_id,
        transport_profile_id: document.transport_profile_id,
        previous_hash: previous_hash_raw,
        z_report_number,
        unsigned_xml_sha256: Some(unsigned_xml_sha256),
        signing_inputs_pinned_at: document.signing_inputs_pinned_at.or(Some(now_iso)),
        // W14a-2b Commit 1: pass-through from input DocumentRow.
        signed_by_cashier_id: document.signed_by_cashier_id,
        // W4-Z2a piece 6b — FK pinned at INSERT PREPARED, never
        // mutated by stage_sign; pass-through.
        signing_config_snapshot_id: document.signing_config_snapshot_id,
    };

    Ok(SigningOutcome {
        document,
        signed_payload,
        unsigned_xml,
    })
}

// ─── W10.4 step 2b — MAC recovery re-sign ─────────────────────────────

/// Output of [`re_sign_after_mac_recovery`].  Mirrors the relevant
/// slice of [`SigningOutcome`] but without the document row + with
/// the new SHA fixed by the input `new_previous_hash`.
#[derive(Debug, Clone)]
pub struct ReSignedArtifacts {
    /// Canonical unsigned XML rebuilt with the recovered
    /// `previous_hash` substituted into the header.
    pub unsigned_xml: Vec<u8>,
    /// SHA-256 of `unsigned_xml`.  Caller (`mac_recovery::orchestrate`)
    /// writes this into `fiscal_documents.unsigned_xml_sha256` inside
    /// the MR-PERSIST `with_immediate` envelope.
    pub unsigned_xml_sha256: [u8; 32],
    /// CMS detached signature produced by the configured provider over
    /// `unsigned_xml`.  Caller writes it into `document_files.SIGNED_XML`
    /// via `document_files::replace_tx`.
    pub signed_xml_cms: SignedCmsBytes,
}

/// Pure no-tx canonical-XML rebuild + CMS sign for the MAC recovery
/// path.  Mirrors the W6 stage 3-NO-TX block (`stage_sign::run` lines
/// 311-362) **but** with three deliberate omissions:
///
///   - **NO Z allocation.**  The doc already has its `lnd` / Z number
///     fixed at attempt #1; recovery only swaps `previous_hash` in the
///     header.  Caller passes `z_report_number` from the doc row.
///
///   - **NO `Prepared → Signed` CAS.**  The doc is in `ErrorRetryable`
///     post-attempt-#1 4-b; the orchestrator's MR-PERSIST envelope
///     wraps its own writes (counter claim, `previous_hash` UPDATE,
///     PAYLOAD_XML / SIGNED_XML replace, audit) atomically.  This
///     helper only produces the artifacts; persistence is the
///     caller's responsibility.
///
///   - **NO db read.**  Caller (orchestrator MR-NO-TX step) passes
///     all inputs that previously came from `fiscal_documents` /
///     `fiscal_number_config` / `node_state` — the function is
///     genuinely pure-CPU + crypto.
///
/// **W3 invariant.**  No `with_immediate` wrap; no DB writes; no IO
/// besides the crypto provider call (which itself runs outside any
/// tx in `stage_sign::run` and continues to do so here).  The
/// `production_src_has_no_foreign_io_inside_with_immediate` scanner
/// test continues to pass.
///
/// **Error surface.**  All `SignError` variants reachable by stage
/// 3-NO-TX are reachable here too: payload schema mismatch, Kyiv
/// timestamp parse failure, lnd / Z numeric range, canonical XML
/// build failure, crypto provider failure.  `Db` / `Internal` /
/// CAS / state variants are NOT reachable because this fn does no
/// DB I/O.
///
/// **Argument count.**  10 args is intentional — every input is a
/// distinct per-call value the orchestrator pulls from a different
/// row / column.  Wrapping into an `ReSignInputs` struct adds one
/// indirection without reducing the actual surface area.  Clippy
/// `too_many_arguments` lint suppressed at the function level.
#[allow(clippy::too_many_arguments)]
pub async fn re_sign_after_mac_recovery(
    ctx: &SigningContext,
    wire_artifact_kind: WireArtifactKind,
    fn_id: &str,
    tax_number: &str,
    business_ts: &str,
    payload_json: &str,
    total_sum_kop: Option<i64>,
    lnd: i64,
    z_report_number: Option<i64>,
    new_previous_hash: [u8; 32],
) -> Result<ReSignedArtifacts, SignError> {
    // R-W10.4-step2b-review MED 1 close: shared no-tx body with
    // `stage_sign::run` via `build_canonical_and_sign_no_tx`.
    let (unsigned_xml, unsigned_xml_sha256, signed_xml_cms) =
        build_canonical_and_sign_no_tx(NoTxBuildSignInputs {
            ctx,
            wire_artifact_kind,
            fn_id,
            tax_number,
            business_ts,
            payload_json,
            total_sum_kop,
            lnd,
            z_report_number,
            previous_hash: Some(&new_previous_hash),
        })
        .await?;
    Ok(ReSignedArtifacts {
        unsigned_xml,
        unsigned_xml_sha256,
        signed_xml_cms,
    })
}

// ─── Shared no-tx body (W6 stage 3 + W10.4 recovery) ─────────────────

/// Inputs for [`build_canonical_and_sign_no_tx`].  Lifetime of all
/// `&'a` references must outlive the await of the returned future.
struct NoTxBuildSignInputs<'a> {
    ctx: &'a SigningContext,
    wire_artifact_kind: WireArtifactKind,
    fn_id: &'a str,
    tax_number: &'a str,
    business_ts: &'a str,
    payload_json: &'a str,
    total_sum_kop: Option<i64>,
    lnd: i64,
    z_report_number: Option<i64>,
    /// `None` for shifts without a prior chain (W6 first SHIFT_OPEN);
    /// `Some(_)` for everyday SELL/RETURN/Z_REPORT and for MAC
    /// recovery (where the recovered hash is always known).
    previous_hash: Option<&'a [u8; 32]>,
}

/// W6 stage 3-NO-TX body, factored out so the W10.4 MAC recovery
/// re-sign path uses identical canonical-XML build + sign logic.
/// Single source of truth — adding validation / fixing a date bug
/// is a one-place edit; both W6 sign and recovery re-sign pick up
/// the change automatically (R-W10.4-step2b-review MED 1 close).
async fn build_canonical_and_sign_no_tx(
    inputs: NoTxBuildSignInputs<'_>,
) -> Result<(Vec<u8>, [u8; 32], SignedCmsBytes), SignError> {
    let ts_str = format_kyiv_local(inputs.business_ts)?;
    let typed_payload = parse_payload(
        inputs.wire_artifact_kind,
        inputs.payload_json,
        inputs.total_sum_kop,
    )?;

    let local_number_u32: u32 = u32::try_from(inputs.lnd).map_err(|_| SignError::Range {
        field: "lnd",
        value: inputs.lnd,
    })?;
    let z_number_u32: u32 = match inputs.z_report_number {
        Some(z) => u32::try_from(z).map_err(|_| SignError::Range {
            field: "z_report_number",
            value: z,
        })?,
        // Python parity: <DAT ZN="0"> for non-Z artifacts.
        None => 0,
    };

    let previous_hash_hex = inputs
        .previous_hash
        .map(|h| hex_encode(h))
        .unwrap_or_default();

    let header = DocumentHeader::with_defaults(
        inputs.fn_id.to_string(),
        inputs.tax_number.to_string(),
        z_number_u32,
        ts_str,
        previous_hash_hex,
    );

    // W4-Z1 piece 7: tax-group snapshot.  TEMPORARY empty map —
    // unknown groups are skipped (Python parity).  Wiring to
    // `driver_tax_mapping` repo lands in a follow-up worklet
    // (W4-Z2 / piece 7b); for now this preserves byte-equivalence
    // with the 4 minimal goldens (no items carry tax_group_1).
    let tax_groups: HashMap<i64, ResolvedTaxGroup> = HashMap::new();
    let canonical_doc = build_canonical_doc(
        inputs.wire_artifact_kind,
        header,
        local_number_u32,
        typed_payload,
        &tax_groups,
    )?;
    let unsigned_xml: Vec<u8> = build_canonical_xml(&canonical_doc)?;

    let unsigned_xml_sha256: [u8; 32] = {
        let digest = Sha256::digest(&unsigned_xml);
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_slice());
        out
    };

    let signed_payload = inputs
        .ctx
        .provider
        .sign_cms_detached(SignCmsRequest {
            session: &inputs.ctx.session,
            canonical_xml: &unsigned_xml,
            profile: inputs.ctx.profile,
        })
        .await?;

    Ok((unsigned_xml, unsigned_xml_sha256, signed_payload))
}

// ─── Private helpers ──────────────────────────────────────────────────

#[derive(Debug)]
enum PinResult {
    Pinned {
        previous_hash: Option<[u8; 32]>,
        z_report_number: Option<i64>,
    },
    Reused {
        previous_hash: Option<[u8; 32]>,
        z_report_number: Option<i64>,
    },
    StateConflict {
        observed: DocState,
    },
    RowMissing,
    PinLost,
    NodeStateMissing,
}

/// Bridge `anyhow::Error` from `with_immediate` closures to typed
/// `SignError`.  Thin wrapper over the shared
/// [`super::types::bridge_anyhow_to`] (R-W10.4-senior-review LOW 1
/// close — deduplicated from three modules to one shared helper).
///
/// Side benefit: aligning with the shared helper adds a typed-`SignError`
/// downcast attempt BEFORE the `sqlx::Error` downcast.  W6 stage 3
/// closures don't currently throw `anyhow::Error::new(SignError::...)`
/// (typed errors fire post-closure on persist outcomes), but future
/// code paths that DO that pattern will round-trip cleanly without
/// being silently wrapped in `Internal`.
fn bridge_anyhow(e: anyhow::Error) -> SignError {
    super::types::bridge_anyhow_to(e, SignError::Db, SignError::Internal)
}

/// Thin wrapper over the shared
/// [`super::types::hex_encode_lower`] (R-W10.4-senior-review LOW 2
/// close — deduplicated with `mac_recovery::hex_lower`).
fn hex_encode(bytes: &[u8]) -> String {
    super::types::hex_encode_lower(bytes)
}

/// Convert UTC ISO-8601 `business_ts` to Kyiv-local `YYYYMMDDHHMMSS`.
/// chrono-tz handles Europe/Kiev DST transitions; manual offset
/// table is a footgun.
fn format_kyiv_local(business_ts: &str) -> Result<String, SignError> {
    let dt: DateTime<Utc> =
        business_ts
            .parse::<DateTime<Utc>>()
            .map_err(|e| SignError::TimestampConversion {
                detail: format!("parse {business_ts:?}: {e}"),
            })?;
    let kyiv = dt.with_timezone(&Kiev);
    Ok(kyiv.format("%Y%m%d%H%M%S").to_string())
}

// ─── Typed JSON payloads (W6 OQ-B: serde, fail-closed) ───────────────

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ShiftOpenJson {
    opening_sum_kop: i64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct CheckJson {
    items: Vec<CheckItemJson>,
    payments: Vec<CheckPaymentJson>,
    /// W4-Z1 piece 7 — optional check-level extensions.  All
    /// `#[serde(default)]` → back-compat with minimal-payload
    /// adapter wire formats (existing callers unchanged).
    #[serde(default)]
    header_lines: Vec<String>,
    #[serde(default)]
    footer_lines: Vec<String>,
    #[serde(default)]
    check_level_adjustments: Vec<CheckLevelAdjustmentJson>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct CheckItemJson {
    code: String,
    name: String,
    price_kop: i64,
    quantity_thousandths: i64,
    sum_kop: i64,
    // W4-Z1 piece 7 optional extensions — all default to None /
    // empty Vec for back-compat with minimal callers.
    #[serde(default)]
    barcode: Option<String>,
    #[serde(default)]
    uktzed: Option<String>,
    #[serde(default)]
    tax_group_1: Option<i64>,
    #[serde(default)]
    tax_group_2: Option<i64>,
    #[serde(default)]
    excise_stamps: Vec<String>,
    #[serde(default)]
    adjustments: Vec<LineAdjustmentJson>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct LineAdjustmentJson {
    kind: AdjustmentKindJson,
    sum: i64,
    mode: AdjustmentModeJson,
    #[serde(default)]
    percent: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    privilege: Option<String>,
    #[serde(default)]
    tax_code: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct CheckLevelAdjustmentJson {
    kind: AdjustmentKindJson,
    sum: i64,
    mode: AdjustmentModeJson,
    #[serde(default)]
    percent: Option<String>,
    #[serde(default)]
    name: Option<String>,
    /// Empty → auto-fill with all p_item_numbers (Python parity);
    /// non-empty → POS-precomputed subset (operator real-world).
    #[serde(default)]
    applies_to_item_ns: Vec<u32>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum AdjustmentKindJson {
    Discount,
    Surcharge,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum AdjustmentModeJson {
    Value,
    Percent,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct CheckPaymentJson {
    name: String,
    sum_kop: i64,
    type_code: String,
    // W4-Z1 piece 7 — EPZ acquirer-slip + cash-only RM/SMP.
    #[serde(default)]
    pa: Option<String>,
    #[serde(default)]
    pb: Option<String>,
    #[serde(default)]
    pc: Option<String>,
    #[serde(default)]
    pd: Option<String>,
    #[serde(default)]
    pe: Option<String>,
    #[serde(default)]
    pf: Option<i64>,
    #[serde(default)]
    psnm: Option<String>,
    #[serde(default)]
    rrn: Option<String>,
    #[serde(default)]
    rm: Option<i64>,
    #[serde(default)]
    smp: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ZReportJson {
    payments: Vec<ZReportPaymentSumJson>,
    sell_count: u32,
    return_count: u32,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ZReportPaymentSumJson {
    name: String,
    sum_in_kop: i64,
    sum_out_kop: i64,
    type_code: String,
}

enum TypedPayload {
    ShiftOpen(ShiftOpenJson),
    Check { body: CheckJson, total_sum_kop: i64 },
    ZReport(ZReportJson),
}

fn parse_payload(
    kind: WireArtifactKind,
    payload_json: &str,
    total_sum_kop: Option<i64>,
) -> Result<TypedPayload, SignError> {
    match kind {
        WireArtifactKind::ShiftOpen => serde_json::from_str::<ShiftOpenJson>(payload_json)
            .map(TypedPayload::ShiftOpen)
            .map_err(|e| SignError::PayloadSchema {
                detail: format!("ShiftOpen: {e}"),
            }),
        WireArtifactKind::Sell | WireArtifactKind::Return => {
            let body: CheckJson =
                serde_json::from_str(payload_json).map_err(|e| SignError::PayloadSchema {
                    detail: format!("Check: {e}"),
                })?;
            let total = total_sum_kop.ok_or_else(|| SignError::PayloadSchema {
                detail: "Check: command.total_sum_kop is required".into(),
            })?;
            Ok(TypedPayload::Check {
                body,
                total_sum_kop: total,
            })
        }
        WireArtifactKind::ZReport => serde_json::from_str::<ZReportJson>(payload_json)
            .map(TypedPayload::ZReport)
            .map_err(|e| SignError::PayloadSchema {
                detail: format!("ZReport: {e}"),
            }),
    }
}

fn build_canonical_doc(
    kind: WireArtifactKind,
    header: DocumentHeader,
    local_number: u32,
    payload: TypedPayload,
    tax_groups: &HashMap<i64, ResolvedTaxGroup>,
) -> Result<CanonicalDoc, SignError> {
    match (kind, payload) {
        (WireArtifactKind::ShiftOpen, TypedPayload::ShiftOpen(p)) => {
            Ok(CanonicalDoc::ShiftOpen(crate::xml::ShiftOpenPayload {
                header,
                opening_sum: p.opening_sum_kop,
            }))
        }
        (
            WireArtifactKind::Sell,
            TypedPayload::Check {
                body,
                total_sum_kop,
            },
        ) => Ok(CanonicalDoc::Sell(check_payload_from(
            header,
            local_number,
            body,
            total_sum_kop,
            tax_groups,
        )?)),
        (
            WireArtifactKind::Return,
            TypedPayload::Check {
                body,
                total_sum_kop,
            },
        ) => Ok(CanonicalDoc::Return(check_payload_from(
            header,
            local_number,
            body,
            total_sum_kop,
            tax_groups,
        )?)),
        (WireArtifactKind::ZReport, TypedPayload::ZReport(p)) => {
            // AUDIT5-CRIT-2 — Z-report aggregation gap (deferred):
            // `tax_summaries` / `service_sums` / `epz` are hardcoded
            // empty here.  Adapter pathway does NOT exist yet
            // (ZReportJson is intentionally minimal — see definition
            // for the supported subset).  Z-report wire-shape
            // verification today uses a HAND-BUILT
            // `z_report_extended_doc()` in goldens_byte_equiv.rs
            // bypassing this arm.
            //
            // W4-Z2 will extend ZReportJson with optional
            // `tax_sums` / `service_sums` / `epz_totals`,
            // resolve them through a NEW `derive_z_report_tax_
            // summaries` helper (Python `:444-458` short-form
            // fallback semantics, distinct from the check-level
            // `derive_check_tax_summaries`), and replace these
            // hardcoded Vec::new() / None bindings with the
            // resolved aggregations.  Until then the field shape
            // is byte-identical to the W4 minimal Z-report
            // golden.
            Ok(CanonicalDoc::ZReport(ZReportPayload {
                header,
                local_number,
                tax_summaries: Vec::new(),
                payments: p
                    .payments
                    .into_iter()
                    .map(|m| ZReportPaymentSum {
                        name: m.name,
                        sum_in: m.sum_in_kop,
                        sum_out: m.sum_out_kop,
                        type_code: m.type_code,
                    })
                    .collect(),
                service_sums: Vec::new(),
                check_count: ZReportCheckCount {
                    sell_count: p.sell_count,
                    return_count: p.return_count,
                },
                epz: None,
            }))
        }
        // (kind, payload) mismatch is unreachable: parse_payload
        // discriminates on `kind` and returns a TypedPayload variant
        // whose discriminant matches.  Defensive panic — surfaces a
        // bug in this module rather than silently shipping a wrong
        // canonical doc.
        _ => unreachable!("derive_wire_artifact_kind / parse_payload discriminant mismatch"),
    }
}

/// W4-Z1 piece 7 — full JSON → xml::CheckPayload conversion.
///
/// Carries every optional W4-Z1 attr from the JSON shim plus
/// derives `tax_summaries` via the pure `derive_check_tax_summaries`
/// helper (no DB / network).  Caller MUST pass a pre-resolved
/// `tax_groups` snapshot (loaded once per receipt via
/// `driver_tax_mapping` repo).
///
/// **Fail-closed behaviour** (AUDIT5-CRIT-1): when items reference
/// `tax_group_1 == Some(_)` but the resolved map is EMPTY, the
/// helper returns `CalcTaxError::TaxMappingNotWired` rather than
/// silently emit no `<TX>`.  Partial maps (with miss) follow Python
/// parity and skip the unknown group.
///
/// Operator pin: aggregation lives HERE, not in adapters; adapter
/// stays thin and transcribes wire fields only.  See module-level
/// `tax_summary` for rationale.
fn check_payload_from(
    header: DocumentHeader,
    local_number: u32,
    body: CheckJson,
    total_sum_kop: i64,
    tax_groups: &HashMap<i64, ResolvedTaxGroup>,
) -> Result<CheckPayload, SignError> {
    let items: Vec<CheckItem> = body
        .items
        .into_iter()
        .map(|it| CheckItem {
            code: it.code,
            name: it.name,
            price: it.price_kop,
            quantity: it.quantity_thousandths,
            sum: it.sum_kop,
            barcode: it.barcode,
            uktzed: it.uktzed,
            tax_group_1: it.tax_group_1,
            tax_group_2: it.tax_group_2,
            excise_stamps: it.excise_stamps,
            adjustments: it.adjustments.into_iter().map(line_adjustment_from).collect(),
        })
        .collect();

    let payments: Vec<CheckPayment> = body
        .payments
        .into_iter()
        .map(|p| CheckPayment {
            name: p.name,
            sum: p.sum_kop,
            type_code: p.type_code,
            pa: p.pa,
            pb: p.pb,
            pc: p.pc,
            pd: p.pd,
            pe: p.pe,
            pf: p.pf,
            psnm: p.psnm,
            rrn: p.rrn,
            rm: p.rm,
            smp: p.smp,
        })
        .collect();

    let check_level_adjustments: Vec<CheckLevelAdjustment> = body
        .check_level_adjustments
        .into_iter()
        .map(check_level_adjustment_from)
        .collect();

    // Derived per operator pin: NEVER taken from wire, always
    // computed from items + caller-resolved tax_groups snapshot.
    let tax_summaries = derive_check_tax_summaries(&items, tax_groups)?;

    Ok(CheckPayload {
        header,
        local_number,
        items,
        payments,
        total_sum: total_sum_kop,
        check_level_adjustments,
        header_lines: body.header_lines,
        footer_lines: body.footer_lines,
        tax_summaries,
    })
}

fn line_adjustment_from(adj: LineAdjustmentJson) -> LineAdjustment {
    LineAdjustment {
        kind: match adj.kind {
            AdjustmentKindJson::Discount => LineAdjustmentKind::Discount,
            AdjustmentKindJson::Surcharge => LineAdjustmentKind::Surcharge,
        },
        sum: adj.sum,
        mode: match adj.mode {
            AdjustmentModeJson::Value => AdjustmentMode::Value,
            AdjustmentModeJson::Percent => AdjustmentMode::Percent,
        },
        percent: adj.percent,
        name: adj.name,
        privilege: adj.privilege,
        tax_code: adj.tax_code,
    }
}

fn check_level_adjustment_from(adj: CheckLevelAdjustmentJson) -> CheckLevelAdjustment {
    CheckLevelAdjustment {
        kind: match adj.kind {
            AdjustmentKindJson::Discount => CheckLevelAdjustmentKind::Discount,
            AdjustmentKindJson::Surcharge => CheckLevelAdjustmentKind::Surcharge,
        },
        sum: adj.sum,
        mode: match adj.mode {
            AdjustmentModeJson::Value => AdjustmentMode::Value,
            AdjustmentModeJson::Percent => AdjustmentMode::Percent,
        },
        percent: adj.percent,
        name: adj.name,
        applies_to_item_ns: adj.applies_to_item_ns,
    }
}
