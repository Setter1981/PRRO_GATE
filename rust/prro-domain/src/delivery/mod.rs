//! Delivery contract types + total classifier (CS-3 C-pure slice).
//!
//! **Spec authority:** #4B rev-6 (GO) `2026-07-16-spec4b-dps-contract.md`.
//! **Amendment authority:** #4A §A4-6 [AMENDMENT 2026-07-17] `2026-07-15-spec4-authority-minilock.md`.
//!
//! # What lives here (§12 R3 prro-domain homes)
//! - The three orthogonal delivery axes: [`SubmissionCertainty`], [`ResponseProvenance`],
//!   [`ActiveRetryClass`] + the decode-only [`HydratedRetryClass`].
//! - [`NodeEffect`] — the durable semantic discriminant (A4-6 amendment).
//! - Wire-observation types: [`SubmissionEvidence`], [`SendResponse`], [`NoResponseCause`],
//!   [`RemoteStatusEvidence`], [`SendOutcome`], [`DpsReject`], [`SendIndeterminate`],
//!   [`SentAccepted`], [`ReconcileOutcome`], [`Kvt1Raw`].
//! - Supporting primitives: [`DpsProtocolBinding`], [`DpsProtocolId`], [`EnvelopeHash`],
//!   [`BoundedText`], [`RawResponseDigest`], [`PreflightRefusal`].
//! - [`ClassifiedOutcome`] + the total classifier [`classify`].
//! - [`ObservedOutcomeV1`] — the self-contained durable record payload (A4-6 + amendment).
//!
//! # What does NOT live here
//! - `prro-dps-contract` raw port traits (`DpsSubmissionPort`, `DpsReconciliationPort`,
//!   `validate_reconcile`, …) — those are CS-6 / Bridge.
//! - `AuthorizedSubmission` + `authorize_submission` mint — engine-private (CS-3 D slice).
//! - DB / async / sqlx / tonic — this crate is pure (purity gate enforces).

#![allow(dead_code)] // types will be used by later slices

use crate::enums::DocType;

// ─── Protocol binding ────────────────────────────────────────────────────────

/// Identifies the DPS wire protocol (032:81).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DpsProtocolId {
    /// FSCO ZZD protocol (current production wire).
    FscoZzd,
    /// EVPZ DPS protocol (CS-6 / future).
    EvpzDps,
}

impl DpsProtocolId {
    /// SQL CHECK literal matching 032:81 `dps_protocol_id IN ('FSCO_ZZD','EVPZ_DPS')`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FscoZzd => "FSCO_ZZD",
            Self::EvpzDps => "EVPZ_DPS",
        }
    }
}

/// Versioned protocol-contract version (032:82, ≥1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProtocolContractVersion(pub u32);

/// Capability profile version (032:83, optional, ≥1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CapabilityProfileVersion(pub u32);

/// Endpoint config revision (032:84, optional, ≥1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EndpointConfigRevision(pub u32);

/// Full protocol binding snapshot — immutable per reservation (PB-4b).
///
/// Shift fixes exactly `protocol_id` at shift-open (PB-4a); the doc snapshots the
/// full tuple at reservation creation and carries it immutably through retries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DpsProtocolBinding {
    pub protocol_id: DpsProtocolId,
    pub contract_version: ProtocolContractVersion,
    pub capability_profile_version: Option<CapabilityProfileVersion>,
    pub endpoint_config_revision: Option<EndpointConfigRevision>,
}

/// SHA-256 of the signed DPS envelope bytes (032:85, length=32).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeHash(pub [u8; 32]);

// ─── Bounded peer-string types (audit V18 DoS) ───────────────────────────────

/// A bounded, non-empty DPS peer string (DPS status code as text, remote id, etc.).
/// Enforces a maximum length to prevent unbounded allocations from malicious DPS replies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedText(String);

impl BoundedText {
    /// Maximum allowed byte length for a bounded peer string.
    pub const MAX_BYTES: usize = 512;

    /// Construct from a string, truncating if it exceeds `MAX_BYTES`.
    pub fn from_truncating(s: impl Into<String>) -> Self {
        let mut s = s.into();
        if s.len() > Self::MAX_BYTES {
            // Truncate at a char boundary.
            let mut boundary = Self::MAX_BYTES;
            while !s.is_char_boundary(boundary) {
                boundary -= 1;
            }
            s.truncate(boundary);
        }
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A fixed-size 32-byte digest of a raw DPS response body (prevents unbounded storage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawResponseDigest(pub [u8; 32]);

// ─── The three orthogonal delivery axes (§2) ─────────────────────────────────

/// Axis 1 — did the submission definitely reach DPS? (D3: total over `SubmissionEvidence`)
///
/// SQL CHECK: `submission_certainty IN ('NOT_SUBMITTED','SUBMITTED_UNKNOWN','SUBMITTED')` (032:73).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionCertainty {
    /// No bytes reached DPS (or we have durable proof the attempt never fired).
    /// Only possible from `SubmissionEvidence::NotStarted`.
    NotSubmitted,
    /// The attempt was made but we cannot determine whether DPS processed it.
    /// Fence held — no blind resend (D4), no issuance (Spec #2 §5).
    SubmittedUnknown,
    /// DPS returned a definitive verdict (Accepted or Rejected) — certainty established.
    Submitted,
}

impl SubmissionCertainty {
    /// SQL CHECK literal (032:73).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotSubmitted => "NOT_SUBMITTED",
            Self::SubmittedUnknown => "SUBMITTED_UNKNOWN",
            Self::Submitted => "SUBMITTED",
        }
    }
}

/// Axis 2 — what is the provenance of the response we observed?
///
/// SQL CHECK: `response_provenance IN ('NO_RESPONSE','AUTHENTICATED_PEER','PARSED_DPS_ENVELOPE')` (032:74).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseProvenance {
    /// No bytes arrived from the far side (genuine local absence).
    NoResponse,
    /// An authenticated peer responded but NOT with a parseable DPS envelope
    /// (e.g. WAF, gRPC Unauthenticated over TLS). CS-3 seam value (AM-2).
    AuthenticatedPeer,
    /// A DPS envelope was received and parsed (`SendOutcome` established).
    ParsedDpsEnvelope,
}

impl ResponseProvenance {
    /// SQL CHECK literal (032:74).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoResponse => "NO_RESPONSE",
            Self::AuthenticatedPeer => "AUTHENTICATED_PEER",
            Self::ParsedDpsEnvelope => "PARSED_DPS_ENVELOPE",
        }
    }
}

/// Axis 3 (fresh-write) — retry/routing class for the `set_routing` store API (D9).
///
/// **Only 7 values** (not 8): `DrainChainSettleRetry` is decode-only and lives in
/// [`HydratedRetryClass`]. `set_routing` accepts only `ActiveRetryClass` so no
/// fresh-write path can ever emit the legacy B10 tag (D9).
///
/// SQL CHECK: `routing_class IN ('TerminalReject','TransientRetry',…)` (032:76-77).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveRetryClass {
    /// Terminal — doc routes to `Rejected`; no retry, no recovery.
    /// DPS codes: -1/-5/-7/-8/-9/-10/-11/-16, -2/-15 on non-close doc types.
    TerminalReject,
    /// Transient — re-driven via Pattern B (`ErrorRetryable→Sending→wire`).
    /// DPS codes: Transport, -3, CrashedBeforeObservation, NotStarted-preflight.
    TransientRetry,
    /// FN-config errors (-13/-14); routes to `ErrorRetryable`, W9 chains to RMR.
    FnConfigError,
    /// Wrapper-side bug or invariant breach; routes to `ErrorRetryable` + CRITICAL audit.
    WrapperBug,
    /// Needs a `last_chk` reconciliation probe (-2/-15 close-shift, Decode, RemoteStatus).
    ProbeRequired,
    /// Server `-12` ERROR_BAD_HASH_PREV — bounded ONE auto-MAC-recovery new-attempt.
    MacRecovery,
    /// Server `-6` ERROR_NOT_PREV_ZREPORT — operator-recoverable, not auto-retried.
    OperatorEscalation,
}

impl ActiveRetryClass {
    /// Wire string (byte-identical with `RetryClass::as_str()` in `error_routing.rs`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TerminalReject => "TerminalReject",
            Self::TransientRetry => "TransientRetry",
            Self::FnConfigError => "FnConfigError",
            Self::WrapperBug => "WrapperBug",
            Self::ProbeRequired => "ProbeRequired",
            Self::MacRecovery => "MacRecovery",
            Self::OperatorEscalation => "OperatorEscalation",
        }
    }
}

/// Decode-only hydrated retry class — includes the legacy B10 tag that can appear in
/// durable rows but must NEVER be emitted fresh (D9).
///
/// `set_routing` accepts only [`ActiveRetryClass`]; this type is for decoding existing rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HydratedRetryClass {
    /// Any of the 7 live routing classes.
    Active(ActiveRetryClass),
    /// Historical B10 tag retained solely to decode rows written by the withdrawn
    /// `-8` chain-settle experiment. No fresh-write path emits this (D9).
    DrainChainSettleRetry,
}

impl HydratedRetryClass {
    /// Parse from wire string. Returns `None` for truly unknown strings.
    pub fn from_wire_str(s: &str) -> Option<Self> {
        match s {
            "TerminalReject" => Some(Self::Active(ActiveRetryClass::TerminalReject)),
            "TransientRetry" => Some(Self::Active(ActiveRetryClass::TransientRetry)),
            "FnConfigError" => Some(Self::Active(ActiveRetryClass::FnConfigError)),
            "WrapperBug" => Some(Self::Active(ActiveRetryClass::WrapperBug)),
            "ProbeRequired" => Some(Self::Active(ActiveRetryClass::ProbeRequired)),
            "MacRecovery" => Some(Self::Active(ActiveRetryClass::MacRecovery)),
            "OperatorEscalation" => Some(Self::Active(ActiveRetryClass::OperatorEscalation)),
            "DrainChainSettleRetry" => Some(Self::DrainChainSettleRetry),
            _ => None,
        }
    }
}

// ─── NodeEffect — durable semantic discriminant (A4-6 amendment) ─────────────

/// Durable effect discriminant stored in `ObservedOutcomeV1` so that node-level
/// side-effects are recoverable from the payload alone (A4-6 [AMENDMENT 2026-07-17]).
///
/// Two DPS codes can share a `(certainty, provenance, routing)` triple but differ
/// in node-effect: e.g. `-11 ERROR_OFFLINE_168 → NodeBlocked` vs `-1 → NoNodeEffect`
/// both yield `(Submitted, ParsedDpsEnvelope, TerminalReject)`.
///
/// Mirrors `node_state.mode` side-effects from `error_routing.rs` (W10) +
/// migration 033 plan node_effect column vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeEffect {
    /// No node-state side effect (the common case).
    NoNodeEffect,
    /// `-11 ERROR_OFFLINE_168` → node transitions to `BLOCKED`.
    NodeBlocked,
    /// `-12 ERROR_BAD_HASH_PREV` → MAC chain seed needs reseeding before a new attempt.
    MacReseedPending,
    /// `Decode` / remote-status → needs a liveness probe before deciding.
    ProbeRequired,
    /// `-6 ERROR_NOT_PREV_ZREPORT` → operator must intervene.
    OperatorEscalation,
    /// `-13/-14 ERROR_NOT_REGISTERED_*` → FN configuration error.
    FnConfigError,
    /// Wrapper/internal bug — should never happen in correct code.
    WrapperBug,
}

impl NodeEffect {
    /// Stable column literal for migration 033 `node_effect` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoNodeEffect => "NoNodeEffect",
            Self::NodeBlocked => "NodeBlocked",
            Self::MacReseedPending => "MacReseedPending",
            Self::ProbeRequired => "ProbeRequired",
            Self::OperatorEscalation => "OperatorEscalation",
            Self::FnConfigError => "FnConfigError",
            Self::WrapperBug => "WrapperBug",
        }
    }
}

// ─── SubmissionEvidence + sub-types (§3, §5) ─────────────────────────────────

/// Why a submission did not start (the pre-flight refusal reason).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreflightRefusal {
    /// A precondition on the local state prevented the wire call.
    PreconditionFailed(BoundedText),
    /// The signing or encryption step failed before wire.
    SigningFailed(BoundedText),
}

/// Cause of a genuine local-absence response (B4 fix — no ProtocolDecode here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoResponseCause {
    /// TCP/TLS/DNS never established a session with the far side.
    LocalHandshakeFailure,
    /// Per-call deadline expired / future dropped / clean shutdown.
    Timeout,
    /// Future cancelled (e.g. shutdown signal).
    Cancelled,
    /// Durable `CALL_STARTED` marker written, then reboot before any response observed.
    /// The document is `SubmittedUnknown` — fence held, no new wire (D4).
    CrashedBeforeObservation,
}

/// A peer response that is NOT a parseable DPS envelope (B8 fix).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteStatusEvidence {
    /// TLS-authenticated peer, un-parseable body (e.g. WAF intercept).
    /// CS-3 seam required to prove peer-auth (AM-2).
    AuthenticatedPeerGarbage(RawResponseDigest),
    /// gRPC Unauthenticated / PermissionDenied over an established TLS session.
    /// CS-3 seam required to distinguish from `NoResponse` in the incumbent (AM-2).
    RemoteAuthStatus(RawResponseDigest),
}

/// The raw send result (from `DpsSubmissionPort::submit_raw` — CS-6/Bridge).
///
/// Split from `SubmissionEvidence` because the port is always post-CAS
/// (`CALL_STARTED` already fired), so `NotStarted` is structurally impossible
/// at the port boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendResponse {
    /// Genuine local absence — no bytes / no session reached the far side (B4).
    NoResponse(NoResponseCause),
    /// The far side responded, but NOT with a parseable DPS envelope (B8).
    RemoteStatus(RemoteStatusEvidence),
    /// A DPS envelope was received and parsed successfully (§5).
    Parsed(SendOutcome),
}

impl SendResponse {
    /// Per AM-1: `true` only when a real DPS envelope was parsed.
    /// `RemoteStatus` and `NoResponse` do NOT prove DPS forward-progress.
    pub fn proves_dps_forward_progress(&self) -> bool {
        matches!(self, Self::Parsed(_))
    }
}

/// The send-phase outcome (§5, B3) — **opaque + sealed (Bridge-0.1)**.
///
/// Constructible ONLY via [`from_dps_status`](Self::from_dps_status), the single honest
/// total mapping from a raw DPS status + the store-owned `doc_type`. No illegal
/// cross-product is buildable (e.g. `Rejected(Close)` on a close doc type, or
/// `UnknownStatus` holding a named code). Read it via [`kind`](Self::kind).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendOutcome(SendOutcomeInner);

#[derive(Clone, Debug, PartialEq, Eq)]
enum SendOutcomeInner {
    Accepted(SentAccepted),
    Rejected(DpsReject),
    Indeterminate(SendIndeterminate),
}

/// Borrowed view of a sealed [`SendOutcome`] — read (match) but never construct.
#[derive(Debug)]
pub enum SendOutcomeKind<'a> {
    /// DPS accepted the document (non-empty `id`) — certainty = `Submitted`.
    Accepted(&'a SentAccepted),
    /// DPS issued a definitive verdict on THIS document — certainty = `Submitted`.
    Rejected(DpsReject),
    /// Parsed but does NOT establish processing certainty — `SubmittedUnknown`.
    Indeterminate(&'a SendIndeterminate),
}

/// A raw DPS reply status handed to [`SendOutcome::from_dps_status`] (Bridge-0.1).
/// The engine builds this from the wire facts; the domain owns the total mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawDpsStatus<'a> {
    /// Status OK — carries the server fiscal id (empty ⇒ `OkButNoFiscalNumber`).
    Ok { fiscal_id: &'a str },
    /// A non-OK DPS status code (as emitted on the wire).
    Error(i32),
}

impl SendOutcome {
    /// Read the sealed outcome for matching.
    pub fn kind(&self) -> SendOutcomeKind<'_> {
        match &self.0 {
            SendOutcomeInner::Accepted(a) => SendOutcomeKind::Accepted(a),
            SendOutcomeInner::Rejected(r) => SendOutcomeKind::Rejected(*r),
            SendOutcomeInner::Indeterminate(i) => SendOutcomeKind::Indeterminate(i),
        }
    }

    /// The SOLE honest, total constructor (Bridge-0.1). Maps a raw DPS status +
    /// the STORE-OWNED `doc_type` to a sealed outcome:
    /// - OK + non-empty id → `Accepted`; OK + empty id → `OkButNoFiscalNumber`.
    /// - `-1..-16` named codes → the matching `DpsReject` verdict.
    /// - `-2/-15` split STRICTLY by doc type: close/Z ⇒ `CloseAmbiguous`, else `Close`.
    /// - `-3` → `SaveError`; `-4` and every other unrecognized code → `UnknownStatus`.
    ///
    /// Correction 1 (Bridge-0.1): `doc_type` is supplied by the engine from the durable
    /// store, NEVER carried on the wire — a wire-carried doc type would just move forgery.
    pub fn from_dps_status(
        status: RawDpsStatus<'_>,
        doc_type: DocType,
        digest: RawResponseDigest,
    ) -> SendOutcome {
        let is_close_shift = matches!(doc_type, DocType::ShiftClose | DocType::ZReport);
        let inner = match status {
            RawDpsStatus::Ok { fiscal_id } => match SentAccepted::observe(fiscal_id) {
                Some(accepted) => SendOutcomeInner::Accepted(accepted),
                None => SendOutcomeInner::Indeterminate(SendIndeterminate(
                    SendIndeterminateInner::OkButNoFiscalNumber { digest },
                )),
            },
            RawDpsStatus::Error(code) => match code {
                -1 => SendOutcomeInner::Rejected(DpsReject::Verify),
                -5 => SendOutcomeInner::Rejected(DpsReject::Type),
                -6 => SendOutcomeInner::Rejected(DpsReject::NotPrevZReport),
                -7 => SendOutcomeInner::Rejected(DpsReject::Xml),
                -8 => SendOutcomeInner::Rejected(DpsReject::XmlDate),
                -9 => SendOutcomeInner::Rejected(DpsReject::XmlChk),
                -10 => SendOutcomeInner::Rejected(DpsReject::XmlZReport),
                -11 => SendOutcomeInner::Rejected(DpsReject::Offline168),
                -12 => SendOutcomeInner::Rejected(DpsReject::BadHashPrev),
                -13 => SendOutcomeInner::Rejected(DpsReject::NotRegisteredRro),
                -14 => SendOutcomeInner::Rejected(DpsReject::NotRegisteredSigner),
                -16 => SendOutcomeInner::Rejected(DpsReject::OfflineId),
                // §12 R1 — the -2/-15 split is doc-type dependent and lives HERE (the sole
                // constructor), so `Close` vs `CloseAmbiguous` can never be mismatched.
                -2 | -15 if is_close_shift => SendOutcomeInner::Indeterminate(SendIndeterminate(
                    SendIndeterminateInner::CloseAmbiguous,
                )),
                -2 | -15 => SendOutcomeInner::Rejected(DpsReject::Close),
                -3 => SendOutcomeInner::Indeterminate(SendIndeterminate(
                    SendIndeterminateInner::SaveError,
                )),
                // -4 (canonical ERROR_UNKNOWN) + every other unrecognized code. Sealed
                // `UnknownStatusCode` guarantees this can never hold a named DpsReject code.
                other => SendOutcomeInner::Indeterminate(SendIndeterminate(
                    SendIndeterminateInner::UnknownStatus {
                        code: UnknownStatusCode(other),
                        digest,
                    },
                )),
            },
        };
        SendOutcome(inner)
    }
}

/// Proof of DPS acceptance: non-empty fiscal number returned (AL-3).
///
/// **Sealed (R2):** the field is private and [`observe`](Self::observe) is the sole
/// constructor, so a `SentAccepted` with an empty `fiscal_number` is unconstructible —
/// the empty case is represented by `SendIndeterminate::OkButNoFiscalNumber` instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentAccepted {
    fiscal_number: String,
}

impl SentAccepted {
    /// Construct only if `id` is non-empty (AL-3). Returns `None` for empty ids.
    pub fn observe(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        if id.is_empty() {
            None
        } else {
            Some(Self { fiscal_number: id })
        }
    }

    /// The DPS-assigned fiscal number (guaranteed non-empty by construction, AL-3).
    pub fn fiscal_number(&self) -> &str {
        &self.fiscal_number
    }
}

/// Closed set of every named definitive DPS verdict code (proto:41-56, B3/AL-1).
///
/// Every unrecognized/unmapped code ⇒ `SendIndeterminate::UnknownStatus`.
/// `-3` is `SaveError`, never here. `-4` is `UnknownStatus`. `-2/-15` split by
/// [`DocType`] at the classifier boundary (§12 R1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpsReject {
    /// -1  ERROR_VEREFY — document-level verification failure.
    Verify,
    /// -5  ERROR_TYPE — wrong document type (builder/adapter bug).
    Type,
    /// -6  ERROR_NOT_PREV_ZREPORT — Z-report ordering violation (operator-recoverable).
    NotPrevZReport,
    /// -7  ERROR_XML — malformed XML.
    Xml,
    /// -8  ERROR_XML_DATE — date field error.
    XmlDate,
    /// -9  ERROR_XML_CHK — check-level XML error.
    XmlChk,
    /// -10 ERROR_XML_ZREPORT — Z-report XML error.
    XmlZReport,
    /// -11 ERROR_OFFLINE_168 — 168-hour cumulative offline cap exceeded.
    /// Side-effect: node → BLOCKED.
    Offline168,
    /// -12 ERROR_BAD_HASH_PREV — MAC chain hash mismatch.
    /// CS-3 new-attempt edge (not a blind resend); the corrective is a NEW
    /// attempt with reseeded MAC (locked-spec amendment, §11).
    BadHashPrev,
    /// -13 ERROR_NOT_REGISTERED_RRO — FN not registered.
    NotRegisteredRro,
    /// -14 ERROR_NOT_REGISTERED_SIGNER — signer not registered.
    NotRegisteredSigner,
    /// -16 ERROR_OFFLINE_ID — offline ID error (terminal + ALERT).
    OfflineId,
    /// -2 ERROR_CHECK / -15 ERROR_NOT_OPEN_SHIFT for NON-close doc types (§12 R1).
    /// For close/Z-report doc types these become `SendIndeterminate::CloseAmbiguous`.
    Close,
}

/// Parsed but indeterminate outcomes — **opaque + sealed (Bridge-0.1)**.
///
/// Constructible only inside a [`SendOutcome`] via [`SendOutcome::from_dps_status`], so
/// `UnknownStatus` can never hold a named `DpsReject` code and `CloseAmbiguous` can never
/// arise on a non-close doc type. Read via [`kind`](Self::kind).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendIndeterminate(SendIndeterminateInner);

#[derive(Clone, Debug, PartialEq, Eq)]
enum SendIndeterminateInner {
    UnknownStatus {
        code: UnknownStatusCode,
        digest: RawResponseDigest,
    },
    SaveError,
    CloseAmbiguous,
    OkButNoFiscalNumber {
        digest: RawResponseDigest,
    },
}

/// Borrowed view of a sealed [`SendIndeterminate`] — read (match) but never construct.
#[derive(Debug)]
pub enum SendIndeterminateKind<'a> {
    /// Any code NOT in `DpsReject` (incl. `-4 ERROR_UNKNOWN`). `code` is the raw status.
    UnknownStatus {
        code: i32,
        digest: &'a RawResponseDigest,
    },
    /// -3 ERROR_SAVE — transient server error, retry is safe.
    SaveError,
    /// -2 ERROR_CHECK / -15 ERROR_NOT_OPEN_SHIFT on a close/Z-report doc type ONLY (§12 R1).
    CloseAmbiguous,
    /// DPS returned status OK but `id` was empty — cannot prove acceptance.
    OkButNoFiscalNumber { digest: &'a RawResponseDigest },
}

impl SendIndeterminate {
    /// Read the sealed indeterminate outcome for matching.
    pub fn kind(&self) -> SendIndeterminateKind<'_> {
        match &self.0 {
            SendIndeterminateInner::UnknownStatus { code, digest } => {
                SendIndeterminateKind::UnknownStatus {
                    code: code.0,
                    digest,
                }
            }
            SendIndeterminateInner::SaveError => SendIndeterminateKind::SaveError,
            SendIndeterminateInner::CloseAmbiguous => SendIndeterminateKind::CloseAmbiguous,
            SendIndeterminateInner::OkButNoFiscalNumber { digest } => {
                SendIndeterminateKind::OkButNoFiscalNumber { digest }
            }
        }
    }
}

/// A DPS status code PROVEN not to be a named [`DpsReject`] (Bridge-0.1).
///
/// Private field: only [`SendOutcome::from_dps_status`] mints one (for `-4` and truly
/// unrecognized codes), so `UnknownStatus` can never carry a code that has a dedicated
/// verdict — the checkpoint's `UnknownStatus("-1")` illegal state is now unconstructible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownStatusCode(i32);

impl UnknownStatusCode {
    /// The raw status code (guaranteed not a named `DpsReject`).
    pub fn get(self) -> i32 {
        self.0
    }
}

/// Closed enum of submission evidence (§3, SE-1/SE-2).
///
/// `NotStarted` is minted ONLY before the durable `CALL_STARTED` marker (032:80)
/// or on a `PreflightRefusal`. Once `CALL_STARTED` is durable, only `Started` is
/// possible — even a crash after that marker ⇒ `Started{NoResponse(CrashedBeforeObservation)}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmissionEvidence {
    /// Attempt did not start: either a preflight refusal or pre-`CALL_STARTED` cancel.
    /// Certainty: `NotSubmitted`. Provenance: `NoResponse`.
    NotStarted {
        reason: PreflightRefusal,
        binding: DpsProtocolBinding,
        envelope_hash: EnvelopeHash,
    },
    /// Attempt was started (durable `CALL_STARTED`); response as observed.
    /// Certainty: total over `response` (D3).
    Started {
        response: SendResponse,
        binding: DpsProtocolBinding,
        envelope_hash: EnvelopeHash,
    },
}

// ─── Reconcile types (§5, B3b/B5) ────────────────────────────────────────────

/// Raw KVT1 quittance data (data_sign bytes, len ≥ 64).
///
/// **Sealed (R2):** the byte vector is private and [`new`](Self::new) enforces the
/// `len >= MIN_LEN` quittance invariant, so a too-short `Kvt1Raw` (which is
/// `IdMatchedNoQuittance` territory, not a proven quittance) is unconstructible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kvt1Raw(Vec<u8>);

impl Kvt1Raw {
    /// Minimum `data_sign` length that proves a real KVT1 quittance (AL-2).
    pub const MIN_LEN: usize = 64;

    /// Construct only if `bytes.len() >= MIN_LEN`. Returns `None` for shorter blobs
    /// (a short `data_sign` is `IdMatchedNoQuittance`, not a proven quittance).
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        if bytes.len() >= Self::MIN_LEN {
            Some(Self(bytes))
        } else {
            None
        }
    }

    /// The raw quittance bytes (guaranteed `len >= MIN_LEN` by construction).
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Attributed reconcile outcome — only for doc-level proven matches (RC-1, AL-2).
///
/// `NotFound` (FN-level `last_chk` empty id) is NOT here — it surfaces in
/// `UnattributedProbeObservation` (§6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// Id matched AND `data_sign.len() >= 64` — quittance proven.
    Kvt1Confirmed { kvt1_raw: Kvt1Raw },
    /// Id matched, `data_sign < 64` — `Submitted` stays proven, quittance pending (AL-2).
    IdMatchedNoQuittance,
}

// ─── Classified outcome (§2 total derivation) ────────────────────────────────

/// Output of the total classifier [`classify`] — the three axes derived from
/// `SubmissionEvidence` per §2 (D1, D2, D3) **plus** the durable `node_effect`
/// discriminant (A4-6).
///
/// **Sealed (R2):** all fields are private and the only constructor is [`classify`]
/// (same module). No external code can fabricate an illegal `(certainty, provenance,
/// routing, node_effect)` quadruple; consumers read via the getters. This is what
/// makes [`ObservedOutcomeV1::record`] a safe mint (it copies a validated quadruple).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedOutcome {
    certainty: SubmissionCertainty,
    provenance: ResponseProvenance,
    routing: Option<ActiveRetryClass>,
    node_effect: NodeEffect,
}

impl ClassifiedOutcome {
    /// Submission certainty axis (D3: total function of `SubmissionEvidence`).
    pub fn certainty(&self) -> SubmissionCertainty {
        self.certainty
    }
    /// Response provenance axis.
    pub fn provenance(&self) -> ResponseProvenance {
        self.provenance
    }
    /// Routing class (CS-3 store API). `None` only for clean acceptance
    /// (routing cleared after a `Submitted`+`Accepted` is applied). (§2 table).
    pub fn routing(&self) -> Option<ActiveRetryClass> {
        self.routing
    }
    /// Durable node-effect discriminant (A4-6). The classifier is the single source
    /// of truth for this — it was previously computed and DISCARDED, forcing every
    /// consumer to re-derive it (checkpoint-audit finding, R2).
    pub fn node_effect(&self) -> NodeEffect {
        self.node_effect
    }
}

/// Total classifier: derives the three axes + `node_effect` from `SubmissionEvidence` (§2/D3).
///
/// **`doc_type` is single-source (Bridge-0.1 3.1b):** `classify` takes NO `doc_type`. The
/// only doc-type-dependent decision — the `-2/-15` close/non-close split (§12 R1) — is
/// consumed exactly ONCE, at [`SendOutcome::from_dps_status`] construction, and is already
/// encoded in the outcome variant (`Rejected(Close)` on non-close ⇒
/// `Submitted / ParsedDpsEnvelope / TerminalReject`; `Indeterminate(CloseAmbiguous)` on
/// close/Z ⇒ `SubmittedUnknown / ParsedDpsEnvelope / ProbeRequired`). The classifier has no
/// doc-type-dependent logic left, so a sealed outcome can never be re-bound to a foreign
/// doc-type context (the checkpoint's re-binding vector). See the `rebind_doc_type` canary.
///
/// **Total** over all `SubmissionEvidence` variants (D3):
/// - `NotStarted` ⇒ `NotSubmitted / NoResponse / routing` (preflight or cancel).
/// - `Started{Parsed(Accepted)}` ⇒ `Submitted / ParsedDpsEnvelope / None`.
/// - `Started{Parsed(Rejected(_))}` ⇒ `Submitted / ParsedDpsEnvelope / routing`.
/// - Every other `Started` ⇒ `SubmittedUnknown / provenance / routing`.
pub fn classify(evidence: &SubmissionEvidence) -> ClassifiedOutcome {
    match evidence {
        SubmissionEvidence::NotStarted { reason, .. } => {
            let routing = match reason {
                PreflightRefusal::PreconditionFailed(_) => ActiveRetryClass::TransientRetry,
                PreflightRefusal::SigningFailed(_) => ActiveRetryClass::WrapperBug,
            };
            ClassifiedOutcome {
                certainty: SubmissionCertainty::NotSubmitted,
                provenance: ResponseProvenance::NoResponse,
                routing: Some(routing),
                node_effect: node_effect_for_active(routing),
            }
        }
        SubmissionEvidence::Started { response, .. } => classify_started(response),
    }
}

fn classify_started(response: &SendResponse) -> ClassifiedOutcome {
    match response {
        // NoResponse: bytes may or may not have left — SubmittedUnknown (B4/RP4B-1).
        SendResponse::NoResponse(cause) => {
            let routing = match cause {
                NoResponseCause::LocalHandshakeFailure
                | NoResponseCause::Timeout
                | NoResponseCause::Cancelled
                | NoResponseCause::CrashedBeforeObservation => ActiveRetryClass::TransientRetry,
            };
            ClassifiedOutcome {
                certainty: SubmissionCertainty::SubmittedUnknown,
                provenance: ResponseProvenance::NoResponse,
                routing: Some(routing),
                node_effect: node_effect_for_active(routing),
            }
        }
        // RemoteStatus: authenticated peer replied but NOT with a DPS envelope (B8).
        SendResponse::RemoteStatus(_) => ClassifiedOutcome {
            certainty: SubmissionCertainty::SubmittedUnknown,
            provenance: ResponseProvenance::AuthenticatedPeer,
            routing: Some(ActiveRetryClass::ProbeRequired),
            node_effect: node_effect_for_active(ActiveRetryClass::ProbeRequired),
        },
        // Parsed: a real DPS envelope — apply the disjoint algebra (§5).
        SendResponse::Parsed(outcome) => classify_parsed(outcome),
    }
}

fn classify_parsed(outcome: &SendOutcome) -> ClassifiedOutcome {
    match outcome.kind() {
        // Accepted: the only clean terminal — routing cleared (§2 table, NULL clean-accept).
        SendOutcomeKind::Accepted(_) => ClassifiedOutcome {
            certainty: SubmissionCertainty::Submitted,
            provenance: ResponseProvenance::ParsedDpsEnvelope,
            routing: None,
            node_effect: NodeEffect::NoNodeEffect,
        },
        // Rejected: a definitive DPS verdict — certainty=Submitted, routing + effect per code.
        // The effect comes from `routing_for_reject` (NOT the class default) because
        // `-11 Offline168` diverges: (TerminalReject, NodeBlocked) vs `-1`'s NoNodeEffect.
        SendOutcomeKind::Rejected(verdict) => {
            let (routing, node_effect) = routing_for_reject(&verdict);
            ClassifiedOutcome {
                certainty: SubmissionCertainty::Submitted,
                provenance: ResponseProvenance::ParsedDpsEnvelope,
                routing: Some(routing),
                node_effect,
            }
        }
        // Indeterminate: parsed but doesn't settle certainty.
        SendOutcomeKind::Indeterminate(ind) => {
            let routing = routing_for_indeterminate(ind);
            ClassifiedOutcome {
                certainty: SubmissionCertainty::SubmittedUnknown,
                provenance: ResponseProvenance::ParsedDpsEnvelope,
                routing: Some(routing),
                node_effect: node_effect_for_active(routing),
            }
        }
    }
}

/// Map a `DpsReject` verdict to its `(ActiveRetryClass, NodeEffect)`.
///
/// **Private + doc-type-free (Bridge-0.1 3.1b):** this is an internal helper of [`classify`],
/// not a public seam. It needs no `doc_type` because `DpsReject::Close` already encodes the
/// non-close branch — the close branch became `SendIndeterminate::CloseAmbiguous` at
/// [`SendOutcome::from_dps_status`] construction (the single doc_type source). Keeping it
/// private + paramless removes the vestigial `doc_type` seam that allowed re-binding.
fn routing_for_reject(verdict: &DpsReject) -> (ActiveRetryClass, NodeEffect) {
    match verdict {
        DpsReject::Verify => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
        DpsReject::Type => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
        DpsReject::NotPrevZReport => (
            ActiveRetryClass::OperatorEscalation,
            NodeEffect::OperatorEscalation,
        ),
        DpsReject::Xml => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
        DpsReject::XmlDate => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
        DpsReject::XmlChk => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
        DpsReject::XmlZReport => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
        DpsReject::Offline168 => (ActiveRetryClass::TerminalReject, NodeEffect::NodeBlocked),
        DpsReject::BadHashPrev => (ActiveRetryClass::MacRecovery, NodeEffect::MacReseedPending),
        DpsReject::NotRegisteredRro => (ActiveRetryClass::FnConfigError, NodeEffect::FnConfigError),
        DpsReject::NotRegisteredSigner => {
            (ActiveRetryClass::FnConfigError, NodeEffect::FnConfigError)
        }
        DpsReject::OfflineId => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
        DpsReject::Close => (ActiveRetryClass::TerminalReject, NodeEffect::NoNodeEffect),
    }
}

fn routing_for_indeterminate(ind: &SendIndeterminate) -> ActiveRetryClass {
    match ind.kind() {
        SendIndeterminateKind::UnknownStatus { .. } => ActiveRetryClass::TransientRetry,
        SendIndeterminateKind::SaveError => ActiveRetryClass::TransientRetry,
        SendIndeterminateKind::CloseAmbiguous => ActiveRetryClass::ProbeRequired,
        SendIndeterminateKind::OkButNoFiscalNumber { .. } => ActiveRetryClass::ProbeRequired,
    }
}

/// Default node-effect for a routing class on the NON-reject paths, where the class
/// alone determines the effect (no code-level ambiguity).
///
/// The reject path uses [`routing_for_reject`] instead — that table OVERRIDES
/// `TerminalReject → NodeBlocked` for `-11 Offline168`, the one DPS code whose node
/// effect diverges from its routing-class default. For every other reject code (and
/// every non-reject path) the effect equals this default, so the two sources agree.
fn node_effect_for_active(class: ActiveRetryClass) -> NodeEffect {
    match class {
        ActiveRetryClass::TerminalReject => NodeEffect::NoNodeEffect,
        ActiveRetryClass::TransientRetry => NodeEffect::NoNodeEffect,
        ActiveRetryClass::FnConfigError => NodeEffect::FnConfigError,
        ActiveRetryClass::WrapperBug => NodeEffect::WrapperBug,
        ActiveRetryClass::ProbeRequired => NodeEffect::ProbeRequired,
        ActiveRetryClass::MacRecovery => NodeEffect::MacReseedPending,
        ActiveRetryClass::OperatorEscalation => NodeEffect::OperatorEscalation,
    }
}

// ─── AuthorizedGeneration — total generation witness (Bridge-0.1) ────────────

/// A validated positive delivery-generation snapshot (`>= 1`).
///
/// Private field + validating [`new`](Self::new): a non-positive generation is
/// unconstructible, so [`AuthorizedGeneration::Started`] always carries a real counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveGeneration(i64);

impl PositiveGeneration {
    /// Construct only if `n >= 1`. Returns `None` otherwise.
    pub fn new(n: i64) -> Option<Self> {
        if n >= 1 {
            Some(Self(n))
        } else {
            None
        }
    }
    /// The snapshot value (guaranteed `>= 1`).
    pub fn get(self) -> i64 {
        self.0
    }
}

/// The authorized-generation witness paired with a delivery outcome (Bridge-0.1).
///
/// **Total over legal outcomes.** A `NOT_SUBMITTED` outcome never crossed
/// `CALL_STARTED`, so it has NO generation (`NotStarted` → DB `NULL`); a `Started`-path
/// outcome carries the immutable `node_state.delivery_generation` snapshot taken at
/// `RN→CALL_STARTED` (`Started(n>=1)` → DB `n`). The prior `i64 (>=1)` field could NOT
/// represent a legal `NotSubmitted` record — the checkpoint hole this closes.
///
/// **Correction (Bridge-0.1):** this type is only the *shape*. The *truth* — that a
/// `Started` witness corresponds to a durable `CALL_STARTED` marker — is minted in the
/// engine/store layer against durable state, NOT in this pure crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizedGeneration {
    /// No `CALL_STARTED` (a `NotStarted` submission / `NOT_SUBMITTED` outcome). DB `NULL`.
    NotStarted,
    /// The `RN→CALL_STARTED` generation snapshot (`>= 1`). DB non-`NULL`.
    Started(PositiveGeneration),
}

impl AuthorizedGeneration {
    /// DB projection: `None` for `NotStarted` (NULL column), `Some(n)` for `Started`.
    pub fn as_db_value(self) -> Option<i64> {
        match self {
            Self::NotStarted => None,
            Self::Started(g) => Some(g.get()),
        }
    }

    /// `true` iff the attempt crossed `CALL_STARTED` (a `Started` witness).
    pub fn is_started(self) -> bool {
        matches!(self, Self::Started(_))
    }
}

// ─── ObservedOutcomeV1 (A4-6 + amendment 2026-07-17) ─────────────────────────

/// The self-contained durable record payload committed as authority first
/// (A4-6 record-then-apply).
///
/// Beyond the three fields + `remote_correlation_id`, this additionally carries:
/// (a) `node_effect` — durable semantic discriminant so node-level effects are
///     recoverable from the payload alone (A4-6 [AMENDMENT 2026-07-17]);
/// (b) `authorized_generation` — immutable snapshot of `node_state.delivery_generation`
///     at `RN→CALL_STARTED`, so a replayed apply can compare stored vs live
///     (never node-vs-node, which would be a tautology).
/// **Sealed (R2):** all fields are private and the sole constructor is
/// [`record`](Self::record), which mints FROM a [`ClassifiedOutcome`]. The
/// `(certainty, provenance, routing, node_effect)` quadruple is therefore always a
/// real `classify` output (never a hand-assembled illegal combination), and
/// `node_effect` cannot drift from the classifier because there is no `node_effect`
/// parameter to inject. `record` enforces the generation↔certainty invariant
/// ([`AuthorizedGeneration`]): a `NotSubmitted` outcome must be `NotStarted`, a
/// `SubmittedUnknown`/`Submitted` outcome must be `Started` — total over legal outcomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedOutcomeV1 {
    certainty: SubmissionCertainty,
    provenance: ResponseProvenance,
    routing: Option<ActiveRetryClass>,
    remote_correlation_id: Option<BoundedText>,
    node_effect: NodeEffect,
    authorized_generation: AuthorizedGeneration,
}

/// Error minting an [`ObservedOutcomeV1`] via [`ObservedOutcomeV1::record`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedOutcomeError {
    /// The generation witness contradicts the outcome's certainty: a `NotSubmitted`
    /// outcome must pair with `NotStarted` (it never crossed `CALL_STARTED`), and a
    /// `SubmittedUnknown`/`Submitted` outcome must pair with `Started` (it did).
    GenerationCertaintyMismatch {
        certainty: SubmissionCertainty,
        generation_started: bool,
    },
}

impl ObservedOutcomeV1 {
    /// Sole constructor — mint the durable record FROM a classifier output.
    ///
    /// Copies the four axes (incl. `node_effect`) verbatim from `classified`, adds the
    /// `remote_correlation_id` (known only at the record step, not to the pure classifier)
    /// and the [`AuthorizedGeneration`] witness. Enforces the generation↔certainty
    /// invariant — a `NotSubmitted` outcome must be `NotStarted`; a `SubmittedUnknown` /
    /// `Submitted` outcome must be `Started`. Total over both legal pairings; a mismatch
    /// (the only illegal input the types still allow) is rejected.
    pub fn record(
        classified: &ClassifiedOutcome,
        remote_correlation_id: Option<BoundedText>,
        authorized_generation: AuthorizedGeneration,
    ) -> Result<Self, ObservedOutcomeError> {
        let is_not_submitted = classified.certainty == SubmissionCertainty::NotSubmitted;
        if is_not_submitted == authorized_generation.is_started() {
            return Err(ObservedOutcomeError::GenerationCertaintyMismatch {
                certainty: classified.certainty,
                generation_started: authorized_generation.is_started(),
            });
        }
        Ok(Self {
            certainty: classified.certainty,
            provenance: classified.provenance,
            routing: classified.routing,
            remote_correlation_id,
            node_effect: classified.node_effect,
            authorized_generation,
        })
    }

    /// Delivery certainty axis.
    pub fn certainty(&self) -> SubmissionCertainty {
        self.certainty
    }
    /// Response provenance axis.
    pub fn provenance(&self) -> ResponseProvenance {
        self.provenance
    }
    /// Routing class (None for clean Accepted).
    pub fn routing(&self) -> Option<ActiveRetryClass> {
        self.routing
    }
    /// DPS remote correlation id (from `CheckAck.id` on acceptance; None otherwise).
    pub fn remote_correlation_id(&self) -> Option<&BoundedText> {
        self.remote_correlation_id.as_ref()
    }
    /// Durable node-effect discriminant (A4-6 — recoverable from the payload alone).
    pub fn node_effect(&self) -> NodeEffect {
        self.node_effect
    }
    /// The [`AuthorizedGeneration`] witness (`NotStarted` for a `NOT_SUBMITTED` record,
    /// else the `Started` snapshot of `node_state.delivery_generation` at
    /// `RN→CALL_STARTED`). The apply CAS compares the stored `Started` value vs the
    /// CURRENT live generation (not node-vs-node); a mismatch ⇒ drop (RP4B-9).
    pub fn authorized_generation(&self) -> AuthorizedGeneration {
        self.authorized_generation
    }
}
