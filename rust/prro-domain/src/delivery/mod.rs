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

/// Closed enum: the send-phase outcome (§5, B3). Disjoint by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// DPS accepted the document (non-empty `id`) — certainty = `Submitted`.
    Accepted(SentAccepted),
    /// DPS issued a definitive verdict on THIS document — certainty = `Submitted`.
    Rejected(DpsReject),
    /// Parsed but does NOT establish processing certainty — `SubmittedUnknown`.
    Indeterminate(SendIndeterminate),
}

/// Proof of DPS acceptance: non-empty fiscal number returned (AL-3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentAccepted {
    /// The DPS-assigned fiscal number (non-empty; the empty case ⇒ `OkButNoFiscalNumber`).
    pub fiscal_number: String,
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

/// Parsed but indeterminate outcomes — the sole free-form arm is `UnknownStatus` (B3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendIndeterminate {
    /// Any code NOT in `DpsReject` + `-4 ERROR_UNKNOWN`.
    /// The sole free-form arm — no other indeterminate variant has a raw code.
    UnknownStatus {
        raw_code: BoundedText,
        digest: RawResponseDigest,
    },
    /// -3 ERROR_SAVE — transient server error, retry is safe.
    SaveError,
    /// -2 ERROR_CHECK / -15 ERROR_NOT_OPEN_SHIFT on close/Z-report doc type ONLY (§12 R1).
    CloseAmbiguous,
    /// DPS returned status OK but `id` was empty — cannot prove acceptance.
    OkButNoFiscalNumber { digest: RawResponseDigest },
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kvt1Raw(pub Vec<u8>);

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
/// `SubmissionEvidence` per §2 (D1, D2, D3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedOutcome {
    /// Submission certainty axis (D3: total function of `SubmissionEvidence`).
    pub certainty: SubmissionCertainty,
    /// Response provenance axis.
    pub provenance: ResponseProvenance,
    /// Routing class (CS-3 store API). `None` only for clean acceptance
    /// (routing cleared after a `Submitted`+`Accepted` is applied). (§2 table).
    pub routing: Option<ActiveRetryClass>,
}

/// Total classifier: derives the three axes from `SubmissionEvidence` + `DocType` (§2/D3).
///
/// **Total** over all `SubmissionEvidence` variants (D3):
/// - `NotStarted` ⇒ `NotSubmitted / NoResponse / routing` (preflight or cancel).
/// - `Started{Parsed(Accepted)}` ⇒ `Submitted / ParsedDpsEnvelope / None`.
/// - `Started{Parsed(Rejected(_))}` ⇒ `Submitted / ParsedDpsEnvelope / routing`.
/// - Every other `Started` ⇒ `SubmittedUnknown / provenance / routing`.
///
/// The `-2/-15` split on `DocType` (§12 R1): close/Z ⇒ `CloseAmbiguous` ⇒
/// `SubmittedUnknown / ParsedDpsEnvelope / ProbeRequired`; other ⇒ `DpsReject::Close` ⇒
/// `Submitted / ParsedDpsEnvelope / TerminalReject`.
pub fn classify(evidence: &SubmissionEvidence, doc_type: DocType) -> ClassifiedOutcome {
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
            }
        }
        SubmissionEvidence::Started { response, .. } => classify_started(response, doc_type),
    }
}

fn classify_started(response: &SendResponse, doc_type: DocType) -> ClassifiedOutcome {
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
            }
        }
        // RemoteStatus: authenticated peer replied but NOT with a DPS envelope (B8).
        SendResponse::RemoteStatus(_) => ClassifiedOutcome {
            certainty: SubmissionCertainty::SubmittedUnknown,
            provenance: ResponseProvenance::AuthenticatedPeer,
            routing: Some(ActiveRetryClass::ProbeRequired),
        },
        // Parsed: a real DPS envelope — apply the disjoint algebra (§5).
        SendResponse::Parsed(outcome) => classify_parsed(outcome, doc_type),
    }
}

fn classify_parsed(outcome: &SendOutcome, doc_type: DocType) -> ClassifiedOutcome {
    match outcome {
        // Accepted: the only clean terminal — routing cleared (§2 table, NULL clean-accept).
        SendOutcome::Accepted(_) => ClassifiedOutcome {
            certainty: SubmissionCertainty::Submitted,
            provenance: ResponseProvenance::ParsedDpsEnvelope,
            routing: None,
        },
        // Rejected: a definitive DPS verdict — certainty=Submitted, routing per code.
        SendOutcome::Rejected(verdict) => {
            let (routing, _node_effect) = routing_for_reject(verdict, doc_type);
            ClassifiedOutcome {
                certainty: SubmissionCertainty::Submitted,
                provenance: ResponseProvenance::ParsedDpsEnvelope,
                routing: Some(routing),
            }
        }
        // Indeterminate: parsed but doesn't settle certainty.
        SendOutcome::Indeterminate(ind) => {
            let routing = routing_for_indeterminate(ind);
            ClassifiedOutcome {
                certainty: SubmissionCertainty::SubmittedUnknown,
                provenance: ResponseProvenance::ParsedDpsEnvelope,
                routing: Some(routing),
            }
        }
    }
}

/// Map a `DpsReject` verdict to its `(ActiveRetryClass, NodeEffect)`.
/// `doc_type` is not needed here because `DpsReject::Close` already encodes the
/// non-close branch (the close branch becomes `SendIndeterminate::CloseAmbiguous`
/// before this function is called, at the ingress adapter level).
pub fn routing_for_reject(
    verdict: &DpsReject,
    _doc_type: DocType,
) -> (ActiveRetryClass, NodeEffect) {
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
    match ind {
        SendIndeterminate::UnknownStatus { .. } => ActiveRetryClass::TransientRetry,
        SendIndeterminate::SaveError => ActiveRetryClass::TransientRetry,
        SendIndeterminate::CloseAmbiguous => ActiveRetryClass::ProbeRequired,
        SendIndeterminate::OkButNoFiscalNumber { .. } => ActiveRetryClass::ProbeRequired,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedOutcomeV1 {
    /// Delivery certainty axis.
    pub certainty: SubmissionCertainty,
    /// Response provenance axis.
    pub provenance: ResponseProvenance,
    /// Routing class (None for clean Accepted).
    pub routing: Option<ActiveRetryClass>,
    /// DPS remote correlation id (from `CheckAck.id` on acceptance; None otherwise).
    pub remote_correlation_id: Option<BoundedText>,
    /// Durable node-effect discriminant (A4-6 amendment — recoverable from payload alone).
    pub node_effect: NodeEffect,
    /// Snapshot of `node_state.delivery_generation` at `RN→CALL_STARTED`.
    /// The apply CAS compares this stored value vs the CURRENT live generation
    /// (not node-vs-node). A mismatch ⇒ drop; ledger/seed/fence unchanged (RP4B-9).
    pub authorized_generation: i64,
}
