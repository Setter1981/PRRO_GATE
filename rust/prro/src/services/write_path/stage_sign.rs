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
    audit_log, delivery_reservation, document_files,
    document_files::DocumentFileKind,
    fiscal_documents::{self as fd, DocumentRow, TransitionOutcome},
    fiscal_number_config as fn_config, node_state, offline_sessions,
};
use crate::db::tx::with_immediate;
use crate::xml::{
    build_canonical_xml, AdjustmentMode, CanonicalDoc, CheckItem, CheckLevelAdjustment,
    CheckLevelAdjustmentKind, CheckPayload, CheckPayment, DocumentHeader, LineAdjustment,
    LineAdjustmentKind, ServiceCheckPayload, ZReportCheckCount, ZReportPayload, ZReportPaymentSum,
    ZReportServiceSum, ZReportTaxSummary,
};

use super::tax_summary::{derive_check_tax_summaries, TaxResolutionSnapshot};
use super::types::WorkerContext;

// ─── Public types ─────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireArtifactKind {
    ShiftOpen,
    Sell,
    Return,
    ZReport,
    /// B10 — offline-session BEGIN (`<C T="109">`) service receipt.
    OfflineSessionBegin,
    /// B10 — offline-session END (`<C T="110">`) service receipt.
    OfflineSessionEnd,
    /// L3 — service cash-in (`<C T="2">…<I N='1' T='0' SM=…/>`).
    /// WebCheck `DealCheck` / `SubmitCheck` code 3.
    ServiceIn,
    /// L3 — service cash-out (`<C T="2">…<O N='1' T='0' SM=…/>`).
    /// WebCheck `DealCheck` / `SubmitCheck` code 3 (shared type with ServiceIn).
    ServiceOut,
    /// EPZ — видача готівки за ЕПЗ (`<C T="8">` + good + card `<M>`).
    CashAdvanceEpz,
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
    /// A.3 PR-C (D5 gate, design v3 §6 step 7) — refused to sign because an
    /// OLDER non-issued sibling rests on this FN (the ER-parked interleave:
    /// the blocker later advances the seed, so this doc's chain-`previous_hash`
    /// would go stale AFTER the wire call → the drift-assert fires too late).
    /// Fail-closed: the pin-tx rolled back, the doc stays `PREPARED` (no pin,
    /// no crypto, no files) — no worse than it was.  RETRYABLE — the
    /// `online_convergence` resolver re-drives the blocker; the client retries
    /// once the FN un-gates (mapped to 503, NOT the structural 500 default).
    #[error("stage 3-PRE D5 gate: doc {document_id:?} on fn {fiscal_number} blocked by an older non-issued sibling")]
    D5GateBlocked {
        document_id: DocumentId,
        fiscal_number: String,
    },
    /// W4-Z2a piece 8a (CRIT-1 close per spec rule #14): caller's
    /// `signing_config_snapshot_id = Some(caller)` differs from
    /// the persisted `Some(existing)`.  Fail-loud drift surface —
    /// signing path MUST NOT proceed, MAC chain would otherwise
    /// be re-derived from a tax-config the persisted doc was not
    /// pinned to.  Reject doc (deterministic config defect — never
    /// retry).
    #[error("stage 3-PRE snapshot_id mismatch: doc {document_id:?} persisted={existing}, caller={caller}")]
    SnapshotIdMismatch {
        existing: i64,
        caller: i64,
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
        // B10 — offline-session boundary docs are signable service receipts
        // (empty `<C T="109">` / `<C T="110">`).  They ride the same offline
        // sign path (code acquire + `<MAC ID>` stamp) as any offline doc.
        DocType::OfflineSessionBegin => Ok(WireArtifactKind::OfflineSessionBegin),
        DocType::OfflineSessionEnd => Ok(WireArtifactKind::OfflineSessionEnd),
        // L3 — service cash-in/out wired:
        DocType::ServiceIn => Ok(WireArtifactKind::ServiceIn),
        DocType::ServiceOut => Ok(WireArtifactKind::ServiceOut),
        // EPZ — видача готівки за ЕПЗ wired (`<C T='8'>`):
        DocType::CashAdvanceEpz => Ok(WireArtifactKind::CashAdvanceEpz),
        // CashWithdrawal is the fail-closed placeholder; XReport is read-only.
        DocType::CashWithdrawal | DocType::XReport => {
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
        command,
        document,
        tax_resolution_snapshot,
        tax_resolution_snapshot_id,
        ..
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
    // W4-Z2a piece 8b — caller's snapshot id is the FK that
    // stage_acquire INSERT-set in the same fiscal_documents row.
    // Variants:
    //   Some(_)  Proceed path: matches existing FK; pin idempotent
    //            (COALESCE existing-wins keeps the same value).
    //   None     Resume / boot recovery / fixture paths: WHERE-guard
    //            matches `? IS NULL`, COALESCE preserves existing FK.
    //            Piece 9 wires MR-NO-TX to read persisted FK separately.
    let caller_snapshot_id: Option<i64> = tax_resolution_snapshot_id;
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

            // A.3 PR-C (D5 gate) — fail-closed refusal to sign while an
            // OLDER non-issued sibling rests on this FN.  This is the pin-tx
            // enforcement layer (MANDATORY: boot `dispatch_prepared_via_chain`
            // enters sign BYPASSING acquire, so the acquire-level refuse alone
            // is not enough).  Self-excluded; only `lnd < self.lnd` blocks
            // (chain-head / boot lnd-ASC re-drive passes → no boot deadlock).
            // Runs BEFORE the pin UPDATE and the 3-NO-TX crypto sign, so a
            // block rolls the pin-tx back with ZERO side effects (doc stays
            // PREPARED, unpinned, no crypto).  One SELECT (INV #1).
            // ONLINE-LANE ONLY: an OFFLINE-origin doc (`fs_mode = 'OFFLINE'`) is
            // never gated — offline availability is unconditional and the
            // runtime resolver is online-scoped.  This mirrors the acquire-layer
            // `channel == Online` scope (at sign the channel is read back from
            // the persisted `fs_mode`, since the WorkerContext does not carry it).
            if fd::is_online_origin_tx(tx, doc_id).await?
                && fd::exists_blocking_non_issued_sibling_tx(tx, &fn_id, Some(doc_id), Some(lnd))
                    .await?
            {
                return Ok(PinResult::D5GateBlocked);
            }

            // W4-Z2a piece 8a — fail-loud drift surface.  If the
            // caller passes `Some(caller)` AND existing FK is
            // `Some(existing != caller)`, reject BEFORE the pin
            // UPDATE (the SQL WHERE-guard is defence-in-depth for
            // a hypothetical race between this re-read and the
            // UPDATE — RESERVED-lock makes that impossible, but
            // the guard documents intent).  Applies whether the
            // row is already pinned (re-entry drift) or fresh
            // (first-pin drift).
            if let (Some(existing), Some(caller)) =
                (inputs.signing_config_snapshot_id, caller_snapshot_id)
            {
                if existing != caller {
                    return Ok(PinResult::SnapshotIdMismatch { existing, caller });
                }
            }

            if inputs.is_pinned {
                // Reuse persisted inputs — do NOT re-read seed,
                // do NOT re-allocate Z.  B9: on a re-entry the offline code was
                // already stamped at the first pin → read it back (no
                // re-acquire; INV-4 / INV-5).
                let offline_dps_code = match resolve_offline_dps_code(tx, doc_id, &fn_id).await? {
                    OfflineResolve::OnlineShaped => None,
                    OfflineResolve::Stamped(code) => Some(code),
                };
                return Ok(PinResult::Reused {
                    previous_hash: inputs.previous_hash,
                    z_report_number: inputs.z_report_number,
                    offline_dps_code,
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
            let rows =
                fd::pin_signing_inputs_tx(tx, doc_id, seed.as_ref(), z, caller_snapshot_id).await?;

            if rows == 0 {
                // Pin-once guard rejected: re-read truth.  Either
                // a concurrent re-entry pinned (rare; reuse) or
                // state moved (StateConflict).
                let after = fd::get_signing_inputs_tx(tx, doc_id).await?;
                return Ok(match after {
                    Some(p) if p.state != DocState::Prepared => {
                        PinResult::StateConflict { observed: p.state }
                    }
                    Some(p) if p.is_pinned => {
                        // Concurrent re-entry pinned first; reuse.  B9: read back
                        // the offline stamp (or acquire if still unstamped).
                        let offline_dps_code =
                            match resolve_offline_dps_code(tx, doc_id, &fn_id).await? {
                                OfflineResolve::OnlineShaped => None,
                                OfflineResolve::Stamped(code) => Some(code),
                            };
                        PinResult::Reused {
                            previous_hash: p.previous_hash,
                            z_report_number: p.z_report_number,
                            offline_dps_code,
                        }
                    }
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

            // B9 Slice A — offline acquire+stamp in the SAME pin-tx, AFTER the
            // pin UPDATE (doc confirmed PREPARED-pinned) and BEFORE the 3-NO-TX
            // XML build, so the opaque code reaches `<MAC ID>` in the signed
            // bytes.  Online docs / offline-without-session / offline-pool-
            // exhausted → None (online-shaped; exhaustion defers to the
            // offline-ack Abort path — see `resolve_offline_dps_code`).
            let offline_dps_code = match resolve_offline_dps_code(tx, doc_id, &fn_id).await? {
                OfflineResolve::OnlineShaped => None,
                OfflineResolve::Stamped(code) => Some(code),
            };

            Ok(PinResult::Pinned {
                previous_hash: seed,
                z_report_number: z,
                offline_dps_code,
            })
        })
    })
    .await
    .map_err(bridge_anyhow)?;

    let (previous_hash_raw, z_report_number, offline_dps_code) = match pinned {
        PinResult::Pinned {
            previous_hash,
            z_report_number,
            offline_dps_code,
        }
        | PinResult::Reused {
            previous_hash,
            z_report_number,
            offline_dps_code,
        } => (previous_hash, z_report_number, offline_dps_code),
        PinResult::StateConflict { observed } => {
            return Err(SignError::StateConflict {
                observed,
                document_id: doc_id,
            })
        }
        PinResult::D5GateBlocked => {
            return Err(SignError::D5GateBlocked {
                document_id: doc_id,
                fiscal_number: fn_id.clone(),
            })
        }
        PinResult::SnapshotIdMismatch { existing, caller } => {
            return Err(SignError::SnapshotIdMismatch {
                existing,
                caller,
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
            // B9 Slice A — the opaque offline code (acquired+stamped in the
            // pin-tx above) becomes `<MAC ID='{code}'>` in the SIGNED bytes.
            // `None` → bare `<MAC>` (online / offline-online-shaped).
            offline_dps_code: offline_dps_code.clone(),
            // W4-Z2a piece 14 + 15 (CRIT-1 + R1 round-3 fixes) — full
            // snapshot through for driver_mapping translation of BOTH
            // tax_group_1 AND tax_group_2 (`translate_tax_group` invoked
            // per-field with field discriminator), plus canonical_tx ∈
            // groups validation, plus tax aggregation.  Variants:
            //   - Proceed: Some(snapshot matches-FK)
            //   - Resume (piece 15 hoist): Some(historic snapshot)
            //     pre-loaded outside lease tx via pool-bound get_by_id
            //   - Boot recovery (piece 13): Some(historic snapshot)
            //     pre-loaded by boot_phase before constructing
            //     WorkerContext
            //   - Pre-W4-Z2a back-compat: None
            tax_resolution: tax_resolution_snapshot,
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
    /// CMS signature produced by the configured provider over
    /// `unsigned_xml` (ATTACHED encapsulation — the XML is embedded as
    /// `eContent`, per DPS sendChkV2).  Caller writes it into
    /// `document_files.SIGNED_XML` via `document_files::replace_tx`.
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
/// **Argument count.**  11 args is intentional (piece 9 adds
/// `tax_groups`) — every input is a distinct per-call value the
/// orchestrator pulls from a different row / column / repository.
/// Wrapping into an `ReSignInputs` struct adds one indirection
/// without reducing the actual surface area.  Clippy
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
    // W4-Z2a piece 14 (CRIT-1 fix) — pre-loaded persisted snapshot
    // (was `tax_groups: HashMap` in piece 9; now the full
    // TaxResolutionSnapshot so check_payload_from can translate
    // driver_number → canonical TX via resolve_driver_number).
    // Caller (`mac_recovery::run_mac_recovery`) reads the doc's
    // `signing_config_snapshot_id`, hits `signing_config_snapshots::
    // get_by_id` (sha256-verified, V1-validated), and passes the
    // result here.  None for pre-W4-Z2a docs (FK NULL).  Preserves
    // the "no DB read inside re_sign" contract — function stays
    // pure-CPU + crypto.
    tax_resolution: Option<TaxResolutionSnapshot>,
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
            // MAC-recovery re-sign is online-lane (post-DPS-reject re-anchor);
            // offline docs local-ack + drain, never re-sign here → bare `<MAC>`.
            offline_dps_code: None,
            tax_resolution,
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
    /// B9 Slice A — opaque offline code emitted as `<MAC ID='{code}'>` in the
    /// SIGNED bytes.  `Some` only for OFFLINE-origin docs stamped at sign;
    /// `None` → bare `<MAC>` (online path, byte-identical; also the MAC-recovery
    /// re-sign path, which is online-lane).
    offline_dps_code: Option<String>,
    /// W4-Z2a piece 14 (external review CRIT-1 close) — full
    /// `TaxResolutionSnapshot` instead of just the canonical-keyed
    /// calc_map.  Drives BOTH:
    ///   1. `check_payload_from`'s per-item `tax_group_1`
    ///      translation via `resolve_driver_number` (POS
    ///      driver_number → canonical TX).  Empty driver_mapping
    ///      = identity.  Lookup miss → `CalcTaxError::DriverMapping
    ///      Miss` fail-loud.
    ///   2. `derive_check_tax_summaries`'s canonical-keyed map
    ///      via `snapshot.to_calc_map()` (derived inside
    ///      check_payload_from after translation).
    ///      Live sources per caller (updated piece 15 + 17):
    ///   - W6 stage 3-NO-TX: `ctx.tax_resolution_snapshot` —
    ///     Some on Proceed (fresh, matches FK), Some(historic)
    ///     on Resume + boot recovery (pre-tx pool-bound reload
    ///     via pieces 13 + 15 hoists), None for pre-W4-Z2a NULL-FK.
    ///   - MAC recovery: pre-loaded by `mac_recovery::run_mac_
    ///     recovery` via `signing_config_snapshots::get_by_id`
    ///     (rule #9 persisted-snapshot reload).
    ///     None = pre-W4-Z2a NULL-FK back-compat path (no driver
    ///     mapping, no tax_groups → derive_check_tax_summaries
    ///     surfaces TaxMappingNotWired if items carry tax_group_1
    ///     OR tax_group_2 — translate_tax_group invoked per-field).
    tax_resolution: Option<TaxResolutionSnapshot>,
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

    let mut header = DocumentHeader::with_defaults(
        inputs.fn_id.to_string(),
        inputs.tax_number.to_string(),
        z_number_u32,
        ts_str,
        previous_hash_hex,
    );
    // B9 Slice A — offline docs carry `<MAC ID='{offline_dps_code}'>` in the
    // signed bytes; online docs keep `mac_id = None` (bare `<MAC>`,
    // byte-identical).
    header.mac_id = inputs.offline_dps_code.clone();

    // W4-Z2a piece 14 (external review CRIT-1 close): pass the
    // full snapshot through to `check_payload_from` so item
    // `tax_group_1` (POS driver_number form) gets translated to
    // canonical TX via `resolve_driver_number` BEFORE downstream
    // aggregation.  Piece 6b.4 seam carried only the calc_map
    // (canonical-keyed), which dropped the driver_mapping →
    // silent fiscal divergence on non-identity drivers.
    let canonical_doc = build_canonical_doc(
        inputs.wire_artifact_kind,
        header,
        local_number_u32,
        typed_payload,
        inputs.tax_resolution.as_ref(),
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
        /// B9 Slice A — the opaque offline code to emit as `<MAC ID>` in the
        /// SIGNED bytes.  `Some` only for OFFLINE-origin docs whose offline
        /// preconditions held at sign (fresh Offline/GoingOffline mode + an
        /// OPEN session): the code was acquired + stamped in THIS pin-tx.
        /// `None` for online docs AND for offline docs signed online-shaped
        /// (no OPEN session at sign → `stage_offline_ack` acquires later).
        offline_dps_code: Option<String>,
    },
    Reused {
        previous_hash: Option<[u8; 32]>,
        z_report_number: Option<i64>,
        /// B9 Slice A — on a re-entry (already-pinned), the offline code was
        /// stamped on the first pin; read it back (do NOT re-acquire) so the
        /// re-signed bytes carry the SAME `<MAC ID>` (INV-4 / INV-5).
        offline_dps_code: Option<String>,
    },
    StateConflict {
        observed: DocState,
    },
    /// A.3 PR-C (D5 gate) — an older non-issued sibling rests on this FN;
    /// the pin-tx refuses fail-closed (rollback → doc stays PREPARED).
    D5GateBlocked,
    /// W4-Z2a piece 8a (CRIT-1 close per spec rule #14): the
    /// caller's `signing_config_snapshot_id` is `Some(caller)` but
    /// the persisted row has `Some(existing)` with `existing != caller`.
    /// Indicates config drift between INSERT-set FK and caller's
    /// view — surfaced fail-loud rather than swallowed via
    /// COALESCE existing-wins.  See `pin_signing_inputs_tx`
    /// WHERE-guard SQL.
    SnapshotIdMismatch {
        existing: i64,
        caller: i64,
    },
    RowMissing,
    PinLost,
    NodeStateMissing,
}

/// B9 Slice A — outcome of the offline acquire-OR-readback step inside the
/// pin-tx.
enum OfflineResolve {
    /// Doc is online-origin, OR offline-origin but its offline preconditions
    /// did not hold at sign (no fresh Offline/GoingOffline mode / no OPEN
    /// session).  Sign online-shaped (bare `<MAC>`); `stage_offline_ack`
    /// acquires the code later via the fallback path.  Carries `None`.
    OnlineShaped,
    /// Offline-origin doc with an available code: acquired + stamped in this
    /// pin-tx (or read back from a prior pin).  Carries the opaque code.
    Stamped(String),
}

/// B9 Slice A (architect-ratified 2026-07-08) — the sign-time offline
/// acquire-OR-readback, run INSIDE the pin-tx after the pin/reuse resolves and
/// BEFORE the canonical XML is built, so the opaque `offline_dps_code` lands in
/// the SIGNED bytes as `<MAC ID>`.
///
/// Conditional stamp-at-sign (the ratified conservative variant): the code is
/// acquired + stamped here ONLY when ALL hold —
///
/// 1. the row is OFFLINE-origin (`fs_mode = 'OFFLINE'`);
/// 2. node mode ∈ {Offline, GoingOffline} on a FRESH tx-read (not the
///    possibly-stale `WorkerContext` copy);
/// 3. an OPEN offline session exists for the FN;
/// 4. the doc is not already stamped (`offline_dps_code IS NULL` — idempotency
///    guard, INV-4 / INV-5).
///
/// Else it returns `OnlineShaped` and the doc signs online-shaped (bare
/// `<MAC>`), deferring acquisition to `stage_offline_ack` (the acquire-OR-
/// readback fallback).  This covers three deferral cases, all preserving the
/// pre-B9 behaviour: (a) no OPEN session at sign — the STOP-O3-1 soft deferral
/// for SELL/RETURN (the intentional asymmetry the architect kept); (b) node
/// flipped Online mid-flight; (c) **the offline code pool is EXHAUSTED at
/// sign** — a no-code offline sell is never issued, so it signs online-shaped
/// and `stage_offline_ack` terminates it in `Aborted` (the established #192
/// terminal-refusal path), NOT a non-terminal PREPARED rest.
///
/// Idempotent re-entry: if the doc is already stamped (re-drive / already-pinned
/// reuse), the persisted code is read back — NO second `acquire_code_tx`, so the
/// offline code is consumed exactly once (INV-5).
///
/// I1: pure SQLite (mode read + session read + one-statement acquire CAS + one
/// stamp UPDATE) — no crypto / network inside the tx.
async fn resolve_offline_dps_code(
    tx: &mut crate::db::tx::WriteTxConn<'_>,
    doc_id: DocumentId,
    fn_id: &str,
) -> Result<OfflineResolve, anyhow::Error> {
    // (1) OFFLINE-origin? Reuse the negation of the online-origin probe the D5
    // gate already uses — a single scalar read of `fs_mode`.
    if fd::is_online_origin_tx(tx, doc_id).await? {
        return Ok(OfflineResolve::OnlineShaped);
    }

    // (4) Idempotency: already stamped (re-entry / boot re-drive)? Read back the
    // code — do NOT acquire a second one.  (Runs BEFORE the doc-type dispatch so
    // a re-driven END reuses its code without re-acquiring — INV-5.)
    if let Some(stamp) = fd::read_offline_stamp_tx(tx, doc_id).await? {
        return Ok(OfflineResolve::Stamped(stamp.dps_code));
    }

    // B10 END-online fix — the DocType=10 (OFFLINE_SESSION_END) is minted
    // `fs_mode='ONLINE'` at DRAIN finalize (it is a real online issuance — the
    // node is GOING online), so step (1) above (`is_online_origin_tx`) already
    // returned `OnlineShaped` and this fn never reaches here for the END.  It
    // therefore signs a BARE `<MAC>` (no forced offline code), which is the
    // DPS-accepted shape for a drain-to-online close (WebCheck `CloseOfflineDoc`;
    // proven live).  The earlier forced-offline-code path (`resolve_offline_dps_
    // code_forced`) was built on the WRONG premise that the END needs an offline
    // code — it does not — and is removed.  SELL/RETURN keep the strict node-mode
    // + OPEN-session gates below verbatim (no leak, W7 criterion 6 intact).

    // (2) FRESH node-mode read: only Offline / GoingOffline stamp at sign.  A
    // node that has since flipped Online (GO_ONLINE race) signs online-shaped.
    let ns = match node_state::get_tx(tx, fn_id).await? {
        Some(ns) => ns,
        None => return Ok(OfflineResolve::OnlineShaped),
    };
    if !matches!(
        ns.mode,
        crate::db::models::enums::NodeMode::Offline
            | crate::db::models::enums::NodeMode::GoingOffline
    ) {
        return Ok(OfflineResolve::OnlineShaped);
    }

    // (3) An OPEN offline session must exist to stamp `offline_session_id`.
    // Absent → sign online-shaped; `stage_offline_ack` handles it (STOP-O3-1
    // soft-deferral asymmetry preserved for SELL/RETURN).
    let session_id = match offline_sessions::current_active_session_id_tx(tx, fn_id).await? {
        Some(sid) => sid,
        None => return Ok(OfflineResolve::OnlineShaped),
    };

    // Preconditions hold — try to acquire + stamp atomically in this pin-tx.
    //
    // POOL EXHAUSTED AT SIGN → sign ONLINE-SHAPED (bare `<MAC>`), do NOT refuse.
    // A no-code offline sell was NEVER going to be issued: the established #192
    // design (invariant_scan.rs:209-226 + the fuzzer's `mint_aborted_refusal`
    // model) is "reality reaches SIGNED, then `stage_offline_ack` refuses
    // (CodePoolExhausted) → the doc terminates in `Aborted`".  Deferring the
    // exhaustion to offline-ack keeps that terminal-Abort path (no doc rests
    // non-terminal at a quiescent boundary — #192 preserved) and is byte-for-
    // byte the pre-B9 behaviour for the exhausted case.  The doc's `<MAC ID>`
    // is irrelevant here — it never reaches the wire (aborted before drain).
    // (This supersedes the earlier "PREPARED-retryable at sign" shape, which
    // the fuzzer proved violates #192: a PREPARED doc resting on an empty pool
    // is a `StuckNonTerminalDoc` because nothing re-drives it before the next
    // quiescent scan.)
    // CS-3 S7-2 — fail-closed FN fence: refuse consuming an offline code (which
    // stamps offline issuance + advances the chain) while a delivery reservation
    // is in-flight (INACTIVE today).
    if delivery_reservation::fn_fence_active_tx(tx, fn_id).await? {
        return Err(anyhow::anyhow!(
            "stage_sign offline-code refused: FN {fn_id} has an active delivery reservation (S7-2 fence)"
        ));
    }
    let acquired = match offline_sessions::acquire_code_tx(tx, fn_id, doc_id).await {
        Ok(a) => a,
        Err(offline_sessions::OfflineSessionError::CodePoolExhausted { .. }) => {
            return Ok(OfflineResolve::OnlineShaped)
        }
        Err(e) => return Err(e.into()),
    };
    let rows = fd::stamp_offline_issuance_tx(
        tx,
        doc_id,
        fn_id,
        acquired.code_lnd,
        &acquired.consumed_at,
        session_id,
        &acquired.dps_code,
    )
    .await?;
    // The `offline_dps_code IS NULL AND state='PREPARED'` guard matched (we are
    // pre-CAS PREPARED and just confirmed unstamped above under the FN write
    // lease) → exactly one row stamped.  A 0 here is a real invariant breach.
    anyhow::ensure!(
        rows == 1,
        "stage_sign: stamp_offline_issuance_tx affected {rows} rows for doc {doc_id:?} \
         (expected 1 — PREPARED + offline_dps_code IS NULL under the FN write lease)"
    );
    Ok(OfflineResolve::Stamped(acquired.dps_code))
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

/// L3 — one service-io `<IO>` row in the Z JSON.
/// Mirrors `xml::ZReportServiceSum` MINUS the split by direction
/// (direction is encoded in `sum_in_kop` vs `sum_out_kop`).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct ZReportServiceIoJson {
    name: String,
    sum_in_kop: i64,
    sum_out_kop: i64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ZReportJson {
    payments: Vec<ZReportPaymentSumJson>,
    /// W4-Z2 (PR-Z1) — aggregated `<TXS>` tax-group turnover.  `#[serde(default)]`
    /// so a pre-W4-Z2 minimal Z payload (no `tax_summaries` key) still parses.
    #[serde(default)]
    tax_summaries: Vec<ZReportTaxSumJson>,
    /// L3 — aggregated `<IO>` service cash-in/out rows.  `#[serde(default)]`
    /// so pre-L3 Z payloads (no `service_sums` key) still parse.
    #[serde(default)]
    service_sums: Vec<ZReportServiceIoJson>,
    /// EPZ — aggregated `<EPZ EPC EPCS EPSM>` card-advance totals.
    /// `#[serde(default)]` so pre-EPZ Z payloads (no `epz` key) still parse.
    #[serde(default)]
    epz: Option<ZReportEpzJson>,
    sell_count: u32,
    return_count: u32,
}

/// EPZ Z-section totals in the Z JSON.  Mirrors `xml::ZReportEpzTotals`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ZReportEpzJson {
    epc: i64,
    epcs: i64,
    epsm: i64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ZReportPaymentSumJson {
    name: String,
    sum_in_kop: i64,
    sum_out_kop: i64,
    type_code: String,
}

/// W4-Z2 (PR-Z1) — one aggregated `<TXS>` row from the Z conversion layer.
/// Mirrors `xml::ZReportTaxSummary` MINUS `ts_prefix` (the Z's date is stamped
/// from the header at build time, not carried in the aggregated body).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ZReportTaxSumJson {
    tx: i64,
    tx_short_form: bool,
    txpr: String,
    txal: i64,
    txty: i64,
    dtpr: String,
    smi: i64,
    smo: i64,
    txi: i64,
    txo: i64,
}

/// L3 — service cash-in/out payload.
/// Produced by `convert.rs` ServiceIn/Out arm.  Carries `schema_version`
/// (invariant #7).  `name` is the service-op label stored in Z `<IO NM>`.
/// The fields are deserialized for `deny_unknown_fields` validation;
/// only `amount_kop` is read structurally in `build_canonical_doc`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ServiceIoJson {
    #[allow(dead_code)]
    schema_version: String,
    amount_kop: i64,
    #[allow(dead_code)]
    name: String,
}

/// EPZ — видача готівки за ЕПЗ signer payload.  Produced by `convert.rs`
/// CashAdvanceEpz arm.  Carries the cash-out `sum_kop`, the fixed good
/// (`code`/`name`), and the card requisites (`paymentid`/`pay_name`/`pa..rrn`).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct EpzJson {
    #[allow(dead_code)]
    schema_version: String,
    sum_kop: i64,
    code: String,
    name: String,
    paymentid: i64,
    pay_name: String,
    pa: String,
    pb: String,
    pc: String,
    pd: String,
    pe: String,
    psnm: String,
    rrn: String,
}

enum TypedPayload {
    ShiftOpen(ShiftOpenJson),
    Check {
        body: CheckJson,
        total_sum_kop: i64,
    },
    ZReport(ZReportJson),
    /// B10 — offline-session boundary (BEGIN/END).  Carries no body: the
    /// wire doc is an EMPTY `<C T="109">`/`<C T="110">` service receipt
    /// (header + `<TS>` + `<MAC>` only).  The payload_json is an inert
    /// marker (`{}`), so no fields are parsed.
    OfflineBoundary,
    /// L3 — service cash-in/out.
    ServiceIo(ServiceIoJson),
    /// EPZ — видача готівки за ЕПЗ.
    Epz(EpzJson),
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
        // B10 — boundary docs carry no body; the JSON is an inert marker.
        WireArtifactKind::OfflineSessionBegin | WireArtifactKind::OfflineSessionEnd => {
            Ok(TypedPayload::OfflineBoundary)
        }
        // L3 — service cash-in/out.
        WireArtifactKind::ServiceIn | WireArtifactKind::ServiceOut => {
            serde_json::from_str::<ServiceIoJson>(payload_json)
                .map(TypedPayload::ServiceIo)
                .map_err(|e| SignError::PayloadSchema {
                    detail: format!("ServiceIo: {e}"),
                })
        }
        // EPZ — видача готівки за ЕПЗ.
        WireArtifactKind::CashAdvanceEpz => serde_json::from_str::<EpzJson>(payload_json)
            .map(TypedPayload::Epz)
            .map_err(|e| SignError::PayloadSchema {
                detail: format!("Epz: {e}"),
            }),
    }
}

fn build_canonical_doc(
    kind: WireArtifactKind,
    header: DocumentHeader,
    local_number: u32,
    payload: TypedPayload,
    tax_resolution: Option<&TaxResolutionSnapshot>,
) -> Result<CanonicalDoc, SignError> {
    match (kind, payload) {
        (WireArtifactKind::ShiftOpen, TypedPayload::ShiftOpen(p)) => {
            Ok(CanonicalDoc::ShiftOpen(crate::xml::ShiftOpenPayload {
                header,
                opening_sum: p.opening_sum_kop,
            }))
        }
        // B10 — offline-session boundary docs: empty service receipt, DI =
        // the doc's local number.  BEGIN → `<C T="109">`, END → `<C T="110">`.
        (WireArtifactKind::OfflineSessionBegin, TypedPayload::OfflineBoundary) => Ok(
            CanonicalDoc::OfflineSessionBegin(crate::xml::OfflineSessionBoundaryPayload {
                header,
                local_number,
            }),
        ),
        (WireArtifactKind::OfflineSessionEnd, TypedPayload::OfflineBoundary) => Ok(
            CanonicalDoc::OfflineSessionEnd(crate::xml::OfflineSessionBoundaryPayload {
                header,
                local_number,
            }),
        ),
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
            tax_resolution,
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
            tax_resolution,
        )?)),
        (WireArtifactKind::ZReport, TypedPayload::ZReport(p)) => {
            // W4-Z2 (PR-Z1) — TXS is now aggregated by the Z conversion layer
            // (`convert::derive_z_report_tax_summaries`, Python `:444-458`
            // short/full-form parity) and arrives in `p.tax_summaries`.  The Z's
            // date (`<TXS TS=>`) is a DOCUMENT property — stamped here from the
            // header, NOT carried in the aggregated body.  `service_sums` (IO)
            // and `epz` stay empty: no service-op / card-acquiring ingress
            // exists, so an ABSENT section is the DPS-accepted, spec-sanctioned
            // form (`2026-05-26-w4-z1-dps-xml-wire-shape.md:329`; IO source is
            // M5 work per §270, EPZ mapping deferred per §Q1 — do NOT synthesise
            // present-but-zero sections here).
            let ts_prefix = header.ts_str.get(..8).unwrap_or("").to_string();
            let tax_summaries = p
                .tax_summaries
                .into_iter()
                .map(|t| ZReportTaxSummary {
                    tx: t.tx,
                    tx_short_form: t.tx_short_form,
                    txpr: t.txpr,
                    txal: t.txal,
                    txty: t.txty,
                    dtpr: t.dtpr,
                    smi: t.smi,
                    smo: t.smo,
                    txi: t.txi,
                    txo: t.txo,
                    ts_prefix: ts_prefix.clone(),
                })
                .collect();
            Ok(CanonicalDoc::ZReport(ZReportPayload {
                header,
                local_number,
                tax_summaries,
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
                // ★ L3 — ZReportJson handoff: thread service_sums through to ZReportPayload
                // so the existing xml/mod.rs IO emitter at :1313 sees the data.
                // `#[serde(default)]` on ZReportJson.service_sums ensures pre-L3
                // payloads (no `service_sums` key) continue to parse correctly.
                service_sums: p
                    .service_sums
                    .into_iter()
                    .map(|s| ZReportServiceSum {
                        name: s.name,
                        sum_in: s.sum_in_kop,
                        sum_out: s.sum_out_kop,
                    })
                    .collect(),
                check_count: ZReportCheckCount {
                    sell_count: p.sell_count,
                    return_count: p.return_count,
                },
                // EPZ — thread the aggregated <EPZ> totals through so the
                // xml/mod.rs emitter sees the card-advance turnover (STOP-S2).
                // `#[serde(default)]` on ZReportJson.epz keeps pre-EPZ payloads
                // (no `epz` key) parsing to None.
                epz: p.epz.map(|e| crate::xml::ZReportEpzTotals {
                    epc: e.epc,
                    epcs: e.epcs,
                    epsm: e.epsm,
                }),
            }))
        }
        // L3 — service cash-in: <C T='2'><I N='1' T='0' SM=…/><E N='2'/></C>
        (WireArtifactKind::ServiceIn, TypedPayload::ServiceIo(p)) => {
            Ok(CanonicalDoc::ServiceIn(ServiceCheckPayload {
                header,
                local_number,
                amount_kop: p.amount_kop,
            }))
        }
        // L3 — service cash-out: <C T='2'><O N='1' T='0' SM=…/><E N='2'/></C>
        (WireArtifactKind::ServiceOut, TypedPayload::ServiceIo(p)) => {
            Ok(CanonicalDoc::ServiceOut(ServiceCheckPayload {
                header,
                local_number,
                amount_kop: p.amount_kop,
            }))
        }
        // EPZ — видача готівки за ЕПЗ: <C T='8'> + good + card <M> + <E>.
        (WireArtifactKind::CashAdvanceEpz, TypedPayload::Epz(p)) => {
            Ok(CanonicalDoc::CashAdvanceEpz(crate::xml::EpzCheckPayload {
                header,
                local_number,
                sum_kop: p.sum_kop,
                code: p.code,
                name: p.name,
                paymentid: p.paymentid,
                pay_name: p.pay_name,
                pa: p.pa,
                pb: p.pb,
                pc: p.pc,
                pd: p.pd,
                pe: p.pe,
                psnm: p.psnm,
                rrn: p.rrn,
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
/// W4-Z2a piece 16 (review round 3 R1 M2 close) — pub test seam.
/// Parses the raw `body_json` (Sell / Return payload wire format)
/// into the internal CheckJson DTO and dispatches to
/// `check_payload_from`.  Used by `tests/w4_z2a_emit_integration.rs`
/// to verify `translate_tax_group` is actually invoked at the
/// conversion boundary for BOTH `tax_group_1` and `tax_group_2`
/// (round 2 R1 noted: type system only guarantees snapshot reaches
/// the function — not that it's USED per-field).
///
/// **Use**: tests only.  Production callers use
/// `build_canonical_and_sign_no_tx` via `stage_sign::run` /
/// `re_sign_after_mac_recovery`.
///
/// Piece 17 (round-4 D/J close): gated behind `test-support`
/// feature so the test seam does not leak into production
/// builds.  Matches the existing convention for integration-
/// only seams.
#[cfg(any(test, feature = "test-support"))]
pub fn check_payload_from_json_for_testing(
    header: DocumentHeader,
    local_number: u32,
    body_json: &str,
    total_sum_kop: i64,
    tax_resolution: Option<&TaxResolutionSnapshot>,
) -> Result<CheckPayload, SignError> {
    let body: CheckJson =
        serde_json::from_str(body_json).map_err(|e| SignError::PayloadSchema {
            detail: format!("CheckJson parse: {e}"),
        })?;
    check_payload_from(header, local_number, body, total_sum_kop, tax_resolution)
}

/// RS-2 Med-6 — test-support validator covering ALL THREE signer payload
/// shapes (`ShiftOpen` / `Check` / `ZReport`).  Confirms a produced
/// `payload_json` parses through the SAME private `parse_payload` the
/// signer uses (`deny_unknown_fields`), so `convert.rs` (RS-2 piece-2)
/// can prove its output is signer-ready WITHOUT the private payload
/// structs being exposed.  Returns the parse error string on mismatch.
///
/// **Use**: tests / `test-support` only.  `parse_payload` stays private;
/// this is the narrow seam the round-2 review (Med-6) called for instead
/// of asserting against `stage_sign` internals.
#[cfg(any(test, feature = "test-support"))]
pub fn validate_signer_payload_shape_for_testing(
    kind: WireArtifactKind,
    payload_json: &str,
    total_sum_kop: Option<i64>,
) -> Result<(), String> {
    parse_payload(kind, payload_json, total_sum_kop)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// W4-Z2 (PR-Z1) — test seam: drive an AGGREGATED Z `payload_json` through the
/// SAME private `parse_payload` + `build_canonical_doc` the signer uses, then
/// emit the unsigned canonical XML.  Lets the aggregation-seam pins assert the
/// full `aggregate → build_z_canonical → build_canonical_doc → XML` path
/// (`<TXS>` bytes + `TS` stamped from the header + IO/EPZ absent) WITHOUT CMS
/// signing.  `parse_payload` / `build_canonical_doc` stay private.
///
/// **Use**: tests / `test-support` only.
#[cfg(any(test, feature = "test-support"))]
pub fn z_report_xml_from_json_for_testing(
    header: DocumentHeader,
    local_number: u32,
    payload_json: &str,
) -> Result<Vec<u8>, String> {
    let payload =
        parse_payload(WireArtifactKind::ZReport, payload_json, None).map_err(|e| e.to_string())?;
    let doc = build_canonical_doc(
        WireArtifactKind::ZReport,
        header,
        local_number,
        payload,
        None,
    )
    .map_err(|e| e.to_string())?;
    build_canonical_xml(&doc).map_err(|e| e.to_string())
}

/// EPZ — test seam: drive a converted EPZ `payload_json` through the SAME
/// private `parse_payload` + `build_canonical_doc` the signer uses, then emit
/// the unsigned canonical XML (`<C T='8'>` + good + card `<M>`).  Lets the EPZ
/// wire pin assert the full `convert → build_canonical_doc → XML` path WITHOUT
/// CMS signing.  `parse_payload` / `build_canonical_doc` stay private.
///
/// **Use**: tests / `test-support` only.
#[cfg(any(test, feature = "test-support"))]
pub fn epz_check_xml_from_json_for_testing(
    header: DocumentHeader,
    local_number: u32,
    payload_json: &str,
) -> Result<Vec<u8>, String> {
    let payload = parse_payload(WireArtifactKind::CashAdvanceEpz, payload_json, None)
        .map_err(|e| e.to_string())?;
    let doc = build_canonical_doc(
        WireArtifactKind::CashAdvanceEpz,
        header,
        local_number,
        payload,
        None,
    )
    .map_err(|e| e.to_string())?;
    build_canonical_xml(&doc).map_err(|e| e.to_string())
}

fn check_payload_from(
    header: DocumentHeader,
    local_number: u32,
    body: CheckJson,
    total_sum_kop: i64,
    tax_resolution: Option<&TaxResolutionSnapshot>,
) -> Result<CheckPayload, SignError> {
    // W4-Z2a piece 14 (CRIT-1) + 15 (round 2 H1+M1) — translate
    // each item's `tax_group_1` AND `tax_group_2` from POS
    // driver_number form to canonical TX BEFORE storing in
    // CheckItem.  Per-field translation semantics:
    //   - Some(raw) + snapshot Some + driver_mapping non-empty +
    //     hit → Some(canonical_tx_num)
    //   - Some(raw) + snapshot Some + driver_mapping empty →
    //     Some(raw) (identity fallback)
    //   - Some(raw) + snapshot Some + non-empty mapping + miss →
    //     CalcTaxError::DriverMappingMiss { field } (fail-loud)
    //   - Some(raw) + snapshot None → Some(raw) pass-through
    //     (pre-W4-Z2a back-compat; downstream
    //     derive_check_tax_summaries surfaces TaxMappingNotWired)
    //   - None field → None (no translation attempted)
    //
    // After translation, validate that the canonical_tx_num
    // resolves to a `groups` entry in the snapshot.  Without
    // this guard, a mapping like (driver 5 → canonical 99) where
    // groups has no tx=99 would silently emit `<I TX="99">` and
    // `derive_check_tax_summaries` would skip the missing canonical
    // group → silent fiscal divergence one step later (round 2 H1).
    let translated_items: Vec<CheckItem> = body
        .items
        .into_iter()
        .map(|it| {
            let canonical_tg1 = translate_tax_group(it.tax_group_1, tax_resolution, "tax_group_1")?;
            let canonical_tg2 = translate_tax_group(it.tax_group_2, tax_resolution, "tax_group_2")?;
            Ok(CheckItem {
                code: it.code,
                name: it.name,
                price: it.price_kop,
                quantity: it.quantity_thousandths,
                sum: it.sum_kop,
                barcode: it.barcode,
                uktzed: it.uktzed,
                tax_group_1: canonical_tg1,
                tax_group_2: canonical_tg2,
                excise_stamps: it.excise_stamps,
                adjustments: it
                    .adjustments
                    .into_iter()
                    .map(line_adjustment_from)
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, SignError>>()?;
    let items = translated_items;

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
    // W4-Z2a piece 14: tax_groups map derived from snapshot here
    // (callsite no longer pre-computes it — single source of truth).
    let tax_groups = tax_resolution.map(|s| s.to_calc_map()).unwrap_or_default();
    let tax_summaries = derive_check_tax_summaries(&items, &tax_groups)?;

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

/// W4-Z2a piece 15 — per-field tax_group translation helper.
///
/// Returns canonical TX after applying snapshot's driver_mapping +
/// validating the canonical resolves to a `groups` entry.  Field-
/// aware error surfaces (`tax_group_1` vs `tax_group_2`) tell
/// operator which item slot drifted on miss / inconsistency.
///
/// Variants per (raw, snapshot, mapping state):
///   - None                  → None (no translation, no validation)
///   - Some(raw), None       → Some(raw) (pre-W4-Z2a back-compat)
///   - Some(raw), Some(snap) → resolve_driver_number + groups check:
///       * hit (canonical in groups) → Some(canonical)
///       * hit (canonical NOT in groups) → CanonicalTxNotInGroups
///       * miss (non-empty mapping, no driver entry) → DriverMappingMiss
fn translate_tax_group(
    raw: Option<i64>,
    tax_resolution: Option<&TaxResolutionSnapshot>,
    field: &'static str,
) -> Result<Option<i64>, SignError> {
    let Some(raw) = raw else { return Ok(None) };
    let Some(snapshot) = tax_resolution else {
        // Pre-W4-Z2a back-compat — raw passes through; downstream
        // derive_check_tax_summaries surfaces TaxMappingNotWired
        // if items carry tax_group_1 (canonical-keyed map empty).
        return Ok(Some(raw));
    };
    let canonical = match snapshot.resolve_driver_number(raw) {
        Some(c) => c,
        None => {
            return Err(SignError::TaxSummary(
                crate::xml::CalcTaxError::DriverMappingMiss {
                    driver_number: raw,
                    field,
                },
            ));
        }
    };
    // H1: validate snapshot self-consistency at use site.
    // Mapping says driver_number → canonical, but groups may not
    // carry that canonical (admin-side inconsistency, corruption,
    // manual SQL drift).  Fail-closed BEFORE XML emit.
    if !snapshot.groups().iter().any(|g| g.tx == canonical) {
        return Err(SignError::TaxSummary(
            crate::xml::CalcTaxError::CanonicalTxNotInGroups {
                driver_number: raw,
                canonical_tx_num: canonical,
                field,
            },
        ));
    }
    Ok(Some(canonical))
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
