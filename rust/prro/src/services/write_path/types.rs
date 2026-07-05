//! Stage-local types for the write-path pipeline.
//!
//! `WorkerContext` is the snapshot stage 1 hands to stages 3+; all
//! mutable substrate handles (`Arc<dyn CryptoProvider>`,
//! `Arc<dyn DpsChannel>`) live in the worker dispatcher (out of W5
//! scope) and are NOT carried in the context — they are wired in at
//! stage boundaries by the dispatcher.

use crate::db::models::enums::{DocType, ShiftState};
use crate::db::models::ids::CashierId;
use crate::db::repositories::fiscal_documents::DocumentRow;
use crate::db::repositories::ingress_inbox::InboxRow;
use crate::db::repositories::node_state::NodeStateRow;
use crate::db::repositories::shifts::ShiftRow;

/// Minimal canonical envelope view used by W5 guards and stage 1
/// INSERT.  Full canonicalisation / XML build happens in stage 3
/// (W6); W5 only needs enough to drive doc_type-shaped guards and
/// build a `NewDocument`.
#[derive(Debug, Clone)]
pub struct CanonicalFiscalCommand {
    pub doc_type: DocType,
    pub business_ts: String,
    pub total_sum_kop: Option<i64>,
    pub payload_json: String,
    pub payload_sha256_canonical: [u8; 32],
    /// RS-3 D5 — the SOURCE hash: the sha256 of the inbox payload that
    /// produced this command, carried so `stage_acquire` can cross-check
    /// it against `ingress_inbox.payload_sha256_canonical` (the idempotency
    /// + crash-recovery anchor, invariant #4).
    ///
    /// For NON-Z documents this COINCIDES with `payload_sha256_canonical`
    /// (one payload, one hash). The A1Z Z path makes them DIVERGE:
    /// `source_sha256` stays the inbox wire-intent hash while
    /// `payload_sha256_canonical` becomes the hash of the *aggregated* Z
    /// report body. Keep the two distinct so a future reader never assumes
    /// the canonical (possibly-aggregated) hash equals the inbox hash.
    pub source_sha256: [u8; 32],
    /// W14a-2b §1.4 — operator/cashier id that will sign this document.
    /// Carries through stage 1 (PREPARED insert) → stage 3 (sign) →
    /// stage 4 (send envelope) and is consumed by `signer_guard` at
    /// stage_send 4-pre (see spec §1.4 + §2.3).  `None` whenever
    /// operator attribution is unavailable: system-context paths
    /// (e.g. boot-phase snapshot reconstruction), test fixtures that
    /// don't exercise signer enforcement, and current ingress
    /// adapters that have not yet been plumbed.
    ///
    /// **Operator-resolved (spec §8 OQ #1):** `CanonicalFiscalCommand`
    /// is not currently `Deserialize`, so adding a field is a Rust-
    /// struct-literal breakage only (all callers updated in this
    /// commit).  No serde concern.
    pub signed_by_cashier_id: Option<CashierId>,

    /// W4-Z0 piece 9 — listener-stamped driver vendor identifier.
    /// `Some` whenever the ingress listener supplies it from
    /// `ops/config.yaml` per-port `driver_id` config.  `None` for
    /// system-context paths (boot reconcile, test fixtures, etc.).
    ///
    /// Used by the W4-Z1 conversion layer to look up
    /// `driver_tax_mapping` for the vendor's letter→canonical
    /// translation table.  NEVER appears in W3 wire DTO — runtime
    /// context only.
    pub driver_id: Option<crate::db::models::ids::DriverId>,
}

/// Snapshot handed from stage 1 to subsequent stages.  Contains
/// everything stages 3-5 need to build wire artifacts and persist
/// outcomes WITHOUT re-reading `node_state` (which can drift
/// between PREPARED and the resume pickup).
#[derive(Debug, Clone)]
pub struct WorkerContext {
    pub inbox: InboxRow,
    pub command: CanonicalFiscalCommand,
    pub node_state: NodeStateRow,
    /// `Some` when `node_state.shift_state` is either `Opened` OR
    /// `OpenedLocalPendingDrain` (W14a-2b HIGH-C4-1 widening per spec
    /// §3.6a) AND the referenced shift row mirrors that state.  `None`
    /// for shift-management ops (SHIFT_OPEN) where there is no active
    /// shift yet.  Stage 2's shift-invariant guard rejects the
    /// inconsistent middle (`shift_state` says open but
    /// `current_shift_id` IS NULL — `ShiftInvariantViolation` Critical
    /// audit).
    pub active_shift: Option<ShiftRow>,
    /// The fiscal_documents row that stages 3+ will continue
    /// processing.  For `Proceed` this is freshly INSERTed PREPARED;
    /// for `Resumed` it is the existing pending row read by
    /// `get_by_request_id_tx`.  In both cases its
    /// `backend_profile_id` / `transport_profile_id` carry the
    /// PERSISTED bindings — stages 3+ MUST use these, not the
    /// possibly-drifted `node_state.*_profile_id`.
    pub document: DocumentRow,
    /// W4-Z2a piece 6 — tax-resolution snapshot for this receipt.
    /// Loaded at stage_acquire time from `pool_secure` via
    /// `runtime::tax_snapshot::load_for_fn_driver` and persisted in
    /// main pool via `signing_config_snapshots::insert_or_get_id`.
    ///
    /// **Resume / recovery semantics** (updated for piece 15 pre-tx
    /// hoist):
    /// - `Some(snapshot)` on `WorkerProcessResult::Proceed` —
    ///   matches the just-inserted snapshot row referenced by
    ///   `tax_resolution_snapshot_id`.  Safe to consume directly.
    /// - `Some(historic_snapshot)` on
    ///   `WorkerProcessResult::Resumed` for W4-Z2a-pinned docs —
    ///   pre-tx pool-bound `signing_config_snapshots::get_by_id`
    ///   reload of the snapshot the doc was originally pinned with
    ///   (locked rule #9).  Companion `tax_resolution_snapshot_id`
    ///   stays `None` to mark "this is a Resume, not a fresh
    ///   Proceed" — downstream consumers branch on `_id` for the
    ///   "is this a freshly-INSERTed snapshot" decision.
    /// - `None` on `WorkerProcessResult::Resumed` for pre-W4-Z2a
    ///   docs (FK NULL — migration window) AND on boot recovery
    ///   re-entry paths that don't pre-load.  `derive_check_tax_
    ///   summaries` surfaces `TaxMappingNotWired` if such a doc
    ///   carries `tax_group_1`.
    ///   Compile-time `Option` wrapper prevents the MAC-recovery
    ///   footgun where a fresh-config tax_snapshot would silently
    ///   re-sign with the wrong configuration.
    pub tax_resolution_snapshot:
        Option<crate::services::write_path::tax_summary::TaxResolutionSnapshot>,
    /// W4-Z2a piece 6 — id from `signing_config_snapshots` for the
    /// snapshot above.  Used by stage_sign 3-PRE pin (piece 8) to
    /// atomically write `fiscal_documents.signing_config_snapshot_id`
    /// alongside `previous_hash` + `z_report_number`.
    ///
    /// `Some(id)` — stage_acquire happy path successfully inserted /
    /// retrieved a snapshot row; piece 8 writes this FK.
    /// `None` — boot recovery placeholder OR test fixtures that
    /// don't exercise snapshot persistence.  Piece 8 MUST branch on
    /// this and either reload-by-id-from-doc-row or skip the pin's
    /// snapshot_id write (per locked design rule #9: recovery uses
    /// persisted id, NEVER current ctx).
    pub tax_resolution_snapshot_id: Option<i64>,
}

/// Reason for a stage-2 guard rejection.  Carried by
/// `WorkerProcessResult::Rejected`.  Distinct variants so the
/// audit / metrics layer can label without parsing strings.
///
/// W14a-2b Commit 4: `#[non_exhaustive]` added together with the 9
/// new channel-aware / mode-guard variants.  Downstream consumers
/// (audit dispatch, metrics labelling) MUST use a wildcard `_` arm so
/// future variant additions don't compile-break the matrix.  The 9
/// new variants explicitly REPLACE the prior `NodeOffline` semantic
/// bucket — the binary "mode == Online or refuse" guard is replaced
/// by `Online / Offline / GoingOffline → Channel` + typed refusals
/// for the four unsupportable modes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RejectionReason {
    ShiftNotOpen {
        current: ShiftState,
    },
    ShiftAlreadyOpen,
    /// A′.1 piece 3 (P6 ruling A) — an ONLINE `SHIFT_OPEN` reached the
    /// fresh-Proceed path with `command.signed_by_cashier_id == None`.
    /// A shift cannot be opened without its opening cashier (§16.8
    /// 1-cashier-per-shift), and the sentinel `__pre_w14a1__` is
    /// forbidden — so this fail-closes BEFORE any mint (audit-only, no
    /// lnd, no shift row). Client-input error → 4xx (same channel as
    /// the neighbouring shift-guard refusals).  Audit shape:
    /// `SHIFT_OPEN_MISSING_CASHIER`.
    ShiftOpenMissingCashier,
    ShiftInError,
    /// Shift is in `RequiresManualReconciliation` (M3b §16.7 — drain
    /// rejected an OFFLINE_LOCAL_ACK backlog doc OR ambiguous wire
    /// timeout OR operator force seam).  Distinct from `ShiftInError`
    /// because Manual is operator-action-territory (compensation
    /// filings) rather than structural-breach territory; semantically
    /// distinct surface for audit / metrics / operator UI labelling.
    /// Per spec §5.6 (shift_state × doc_type matrix): ANY doc type
    /// against `RequiresManualReconciliation` returns this reason.
    ShiftRequiresOperatorAttention,
    /// `shift_state == Opened` but `current_shift_id IS NULL` OR the
    /// resolved shift is not itself `Opened`.  Per ADR-M3-A7 the
    /// canonical source of truth is `node_state.current_shift_id`;
    /// any inconsistency is a structural invariant breach (CRITICAL
    /// audit, no autorepair from stage 2).
    ShiftInvariantViolation,
    /// `node_state.backend_profile_id` and / or
    /// `transport_profile_id` are NULL.  Schema-permitted on a
    /// freshly-bootstrapped FN; a real submission cannot proceed
    /// without resolved bindings.
    MissingProfileBinding,
    InvalidPayload {
        detail: String,
    },

    // ─── W14a-2b Commit 4 — channel-aware + mode guard rewrite ────────
    //
    // The four mode-side variants below replace the prior `NodeOffline`
    // bucket.  Per spec §3.3: `Online` and `Offline | GoingOffline` map
    // to `Channel::Online` / `Channel::Offline` and proceed to the
    // shift guard; the four below are refused BEFORE the shift guard.
    /// Mode = `GoingOnline` — return-online drain in flight.  Operator
    /// MUST wait for `GoingOnline → Online` transition before issuing
    /// new fiscal ops.  Transient; expected during return-online cycle.
    NodeGoingOnlineDrainInFlight,
    /// Mode = `Blocked` — operator manual-recovery state.  Forensic
    /// (W14a-2a force seam may have brought us here).
    NodeBlocked,
    /// Mode = `StopMode` — legal hold / regulatory pause.  Rare.
    NodeStopMode,
    /// Mode = `CryptoDegraded` — crypto subsystem degraded
    /// (key expiry imminent, sidecar down).  Operator MUST intervene
    /// before next op succeeds.
    NodeCryptoDegraded,

    // ─── Channel-aware shift-guard refusals ──────────────────────────
    /// Online op attempted while shift is in `OpenedLocalPendingDrain`
    /// (offline `SHIFT_OPEN` landed local ack but DPS hasn't confirmed
    /// the open yet).  Audit shape:
    /// `SHIFT_OPEN_PENDING_DRAIN_OP_REFUSED`.
    ShiftOpenPendingDrainOpRefused,
    /// Any op attempted while shift is in `ClosingLocalPendingDrain`
    /// (offline `Z_REPORT` issued; post-local-close lockout per
    /// PR #62 §W10).  Channel-irrelevant.  Audit shape:
    /// `POST_LOCAL_CLOSE_SALE_REFUSED`.
    PostLocalCloseSaleRefused,
    /// `SHIFT_CLOSE` attempted on `OpenedLocalPendingDrain` shift.
    /// Per spec §5.7 L2 — offline shift close is not modeled (only
    /// online close after drain reaches `Opened` OR offline `Z_REPORT`
    /// → `ClosingLocalPendingDrain`).  Audit shape:
    /// `OFFLINE_SHIFT_CLOSE_REFUSED`.
    OfflineShiftCloseNotSupported,
    /// Op attempted while shift is in `ClosingLocalPendingDrain` AND
    /// the doc is a shift-management type (`SHIFT_OPEN` / `SHIFT_CLOSE`).
    /// Transient; operator should retry after drain completion.
    /// Audit shape: `SHIFT_CLOSING_IN_FLIGHT_OP_REFUSED`.
    ShiftClosingInFlight,
    /// `Z_REPORT` attempted on `OpenedLocalPendingDrain` shift in
    /// either channel.  Pre-W10 guardrail (coupled pool/backlog/edge-7
    /// logic lands in W10).  W14a-2b refuses unconditionally to avoid
    /// the window where Z-report can be issued while backlog non-empty.
    /// Audit shape: `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED`.
    ZReportBlockedBacklogDrainPending,
}

/// W14a-2b Commit 4 — channel derived from node mode at stage_acquire
/// time.  Threaded into `check_shift_guard` so the matrix can refuse
/// online ops on `OpenedLocalPendingDrain` while allowing offline ops
/// per spec §3.3.  Not persisted; lives in write-path scope only.
///
/// Mapping per spec §3.3:
/// - `NodeMode::Online` → `Channel::Online`
/// - `NodeMode::Offline` / `NodeMode::GoingOffline` → `Channel::Offline`
/// - `NodeMode::GoingOnline` / `NodeMode::Blocked` / `NodeMode::StopMode`
///   / `NodeMode::CryptoDegraded` → REJECTED at mode guard (BEFORE
///   `check_shift_guard` is invoked); no `Channel` value produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Online,
    Offline,
}

/// Result of stage 1 (acquire+validate+guard) per W0-1 §3.1.
#[derive(Debug, Clone)]
pub enum WorkerProcessResult {
    /// Happy path — fresh PREPARED doc inserted, stages 3+ to follow.
    Proceed(WorkerContext),
    /// Resume path — `get_by_request_id_tx` found an existing
    /// pending doc; stage dispatcher continues from its current
    /// state per the W0-1 resume table.  No fresh lnd allocated;
    /// no re-INSERT.
    Resumed(WorkerContext),
    /// Lease miss — inbox row was already `PROCESSING` / `DONE` /
    /// `REJECTED` / `ERROR`; another worker has it or processing
    /// is complete.  **No business-state mutation**.  Per W5 design,
    /// the routine lease-miss path appends no audit row (would
    /// create churn under healthy retry loops).
    ///
    /// Piece 17 (round-4 B/C/G clarification): the
    /// `c-acquire-snapshot-reload-failed` Critical audit is
    /// appended ONLY by the winning worker (the one whose
    /// `mark_rejected_if_new_tx` flipped NEW→REJECTED, `was_new
    /// == true`).  Race-lost workers (`was_new == false`) still
    /// return `Noop` but emit NO duplicate audit — preserves the
    /// "no audit on Noop" contract under thundering-herd
    /// scenarios where multiple workers race to reject the same
    /// stuck request_id.
    Noop,
    /// Guard rejected the request.  `ingress_inbox.status =
    /// REJECTED` is persisted; an audit row is appended; NO
    /// `fiscal_documents` row is created and NO lnd is allocated.
    Rejected { reason: RejectionReason },
}

// ─── Shared bridge_anyhow helper (R-W10.4-senior-review LOW 1) ───────

/// Generic `anyhow::Error` → typed-stage-error bridge.  Mirrors the
/// pattern previously triplicated as private `bridge_anyhow` fns in
/// `stage_send.rs` / `stage_sign.rs` / `mac_recovery.rs`.
///
/// Behaviour (matches the original three private impls):
///   1. Try to downcast to the concrete typed error `E` first — that
///      handles the `anyhow::Error::new(E::Variant(..))` round-trip
///      from inside `with_immediate` closures.
///   2. Fall back to downcasting to `sqlx::Error` and wrap via
///      `wrap_db` — distinguishes DB errors from generic anyhow
///      chains.
///   3. Anything else: wrap via `wrap_internal`.
///
/// Each module's local `bridge_anyhow` becomes a one-liner:
/// ```ignore
/// fn bridge_anyhow(e: anyhow::Error) -> StageSendError {
///     bridge_anyhow_to(e, StageSendError::Db, StageSendError::Internal)
/// }
/// ```
///
/// Why generic over wrap fns (not over the typed error directly):
/// each module's typed error has its own `Db(sqlx::Error)` and
/// `Internal(anyhow::Error)` variant constructors with subtly
/// different naming; passing them as fn pointers keeps each module's
/// surface unchanged while deduplicating the downcast logic.
pub(super) fn bridge_anyhow_to<E>(
    e: anyhow::Error,
    wrap_db: fn(sqlx::Error) -> E,
    wrap_internal: fn(anyhow::Error) -> E,
) -> E
where
    E: 'static + std::error::Error + Send + Sync,
{
    match e.downcast::<E>() {
        Ok(typed) => typed,
        Err(rest) => match rest.downcast::<sqlx::Error>() {
            Ok(sqlx_err) => wrap_db(sqlx_err),
            Err(other) => wrap_internal(other),
        },
    }
}

// ─── Shared lowercase hex encoder (R-W10.4-senior-review LOW 2) ──────

/// Lowercase hex encoder for byte slices.  Used for:
///   - canonical XML `<…PREV_DOC_HASH>` rendering (W6 stage 3,
///     `re_sign_after_mac_recovery`).
///   - audit-payload JSON `*_hex` fields (W10.4 MAC recovery audits).
///
/// Closed contract: lowercase via `format!("{b:02x}")`.  W10.1 review
/// pinned this case explicitly via fixture
/// `re_sign_propagates_new_previous_hash_into_canonical_xml`; any
/// regression to UPPERCASE hex would fail loudly.  De-duplicated
/// from `stage_sign::hex_encode` + `mac_recovery::hex_lower` per
/// senior review.
pub(crate) fn hex_encode_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
