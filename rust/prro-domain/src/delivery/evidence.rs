//! CS-3 Slice 2b — the durable, payload-carrying `EvidenceDiscriminant`.
//!
//! Design authority: `docs/CS3_REMEDIATION_DESIGN.md` §4.1 (payload-carrying leaves),
//! §4.3 (record) and §4.4 (boot hydration). This is the Rust half of the P4
//! durable-evidence substrate; the SQL half (columns + fail-closed matrix) is
//! migration `036_delivery_reservation_evidence_union.sql`, whose vocabulary this
//! module serializes to and hydrates from **byte-for-byte**.
//!
//! The type follows the R2 opaque-sealing pattern of the sibling wire types: a private
//! [`EvidenceDiscriminantInner`], a public opaque [`EvidenceDiscriminant`], a borrowed
//! [`EvidenceDiscriminantKind`] view, and the sole domain constructor
//! [`EvidenceDiscriminant::from_evidence`] (derives the exact leaf from the sealed
//! [`SubmissionEvidence`]). It carries only transport-minted / local payloads:
//!
//! | leaf | payload | evidence_text | evidence_code | evidence_digest |
//! |---|---|---|---|---|
//! | PreconditionFailed | — | NULL | NULL | NULL |
//! | SigningFailed | — | NULL | NULL | NULL |
//! | NoResponse | cause | cause name | NULL | NULL |
//! | RemoteAuthStatus | digest | NULL | NULL | 32B |
//! | Accepted | fiscal_number | F (non-empty) | NULL | NULL |
//! | Rejected | verdict + digest | DpsReject name | NULL | 32B |
//! | UnknownStatus | raw_code + digest | NULL | i32 | 32B |
//! | SaveError | digest | NULL | NULL | 32B |
//! | CloseAmbiguous | digest | NULL | NULL | 32B |
//! | MissingStatus | digest | NULL | NULL | 32B |
//! | OkButNoFiscalNumber | digest | NULL | NULL | 32B |
//!
//! `PreconditionFailed` / `SigningFailed` drop the [`PreflightRefusal`] `BoundedText`
//! reason: per design §4.1 those leaves carry no durable payload (the reason is a
//! forensic-trace diagnostic, not authority — design §4.3).

use super::{
    DecodedResponseDigest, DpsReject, GrpcStatusDigest, NoResponseCause, PreflightRefusal,
    RemoteStatusEvidence, ResponseProvenance, SendIndeterminateKind, SendOutcomeKind,
    SendResponseKind, SubmissionCertainty, SubmissionEvidence,
};

// ─────────────────────── string vocabularies (match migration 036) ───────────────

impl DpsReject {
    /// The canonical variant name stored in `evidence_text` for a `Rejected` leaf.
    /// Byte-identical to migration 036's Rejected verdict CASE.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verify => "Verify",
            Self::Type => "Type",
            Self::NotPrevZReport => "NotPrevZReport",
            Self::Xml => "Xml",
            Self::XmlDate => "XmlDate",
            Self::XmlChk => "XmlChk",
            Self::XmlZReport => "XmlZReport",
            Self::Offline168 => "Offline168",
            Self::BadHashPrev => "BadHashPrev",
            Self::NotRegisteredRro => "NotRegisteredRro",
            Self::NotRegisteredSigner => "NotRegisteredSigner",
            Self::OfflineId => "OfflineId",
            Self::Close => "Close",
        }
    }

    /// Parse a stored `evidence_text` back to a verdict (boot hydration). `None` on an
    /// unknown name — the caller fails loud (design §4.4).
    pub fn from_stored(s: &str) -> Option<Self> {
        Some(match s {
            "Verify" => Self::Verify,
            "Type" => Self::Type,
            "NotPrevZReport" => Self::NotPrevZReport,
            "Xml" => Self::Xml,
            "XmlDate" => Self::XmlDate,
            "XmlChk" => Self::XmlChk,
            "XmlZReport" => Self::XmlZReport,
            "Offline168" => Self::Offline168,
            "BadHashPrev" => Self::BadHashPrev,
            "NotRegisteredRro" => Self::NotRegisteredRro,
            "NotRegisteredSigner" => Self::NotRegisteredSigner,
            "OfflineId" => Self::OfflineId,
            "Close" => Self::Close,
            _ => return None,
        })
    }
}

impl NoResponseCause {
    /// The canonical variant name stored in `evidence_text` for a `NoResponse` leaf.
    /// Byte-identical to migration 036's NoResponse `IN (...)` list.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalHandshakeFailure => "LocalHandshakeFailure",
            Self::Timeout => "Timeout",
            Self::Cancelled => "Cancelled",
            Self::CrashedBeforeObservation => "CrashedBeforeObservation",
            Self::CallFailedWithoutTrustedDpsEnvelope => "CallFailedWithoutTrustedDpsEnvelope",
        }
    }

    /// Parse a stored `evidence_text` back to a cause (boot hydration). `None` on unknown.
    pub fn from_stored(s: &str) -> Option<Self> {
        Some(match s {
            "LocalHandshakeFailure" => Self::LocalHandshakeFailure,
            "Timeout" => Self::Timeout,
            "Cancelled" => Self::Cancelled,
            "CrashedBeforeObservation" => Self::CrashedBeforeObservation,
            "CallFailedWithoutTrustedDpsEnvelope" => Self::CallFailedWithoutTrustedDpsEnvelope,
            _ => return None,
        })
    }
}

/// The set of DPS status codes that a `UnknownStatus` leaf's `evidence_code` may NOT
/// hold — the named `DpsReject` codes plus `0` (UNKNOWN/MissingStatus) and `1` (OK).
/// Byte-identical to migration 036's `evidence_code NOT IN (...)` guard. (`-3` = SaveError
/// and `-4` = the canonical UnknownStatus are NOT excluded — `-4` is the very code this
/// leaf carries.)
const UNKNOWN_STATUS_FORBIDDEN_CODES: [i32; 16] = [
    0, 1, -1, -2, -5, -6, -7, -8, -9, -10, -11, -12, -13, -14, -15, -16,
];

// ─────────────────────────── the discriminant ───────────────────────────────────

/// The durable, payload-carrying evidence leaf (opaque + sealed, R2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceDiscriminant(EvidenceDiscriminantInner);

#[derive(Clone, Debug, PartialEq, Eq)]
enum EvidenceDiscriminantInner {
    PreconditionFailed,
    SigningFailed,
    NoResponse(NoResponseCause),
    RemoteAuthStatus(GrpcStatusDigest),
    /// The exact accepted fiscal number. A plain `String` (not `NonEmptyFiscalNumber`): the
    /// non-empty invariant is guaranteed upstream by `SentAccepted` at construction and
    /// re-checked at hydration — re-minting via the transport-gated `from_transport` here
    /// would (correctly) trip the digest/id mint source-policy gate in `prro-domain/src`.
    Accepted(String),
    Rejected(DpsReject, DecodedResponseDigest),
    UnknownStatus(i32, DecodedResponseDigest),
    SaveError(DecodedResponseDigest),
    CloseAmbiguous(DecodedResponseDigest),
    MissingStatus(DecodedResponseDigest),
    OkButNoFiscalNumber(DecodedResponseDigest),
}

/// Borrowed view of a sealed [`EvidenceDiscriminant`] — read (match) but never construct.
#[derive(Debug)]
pub enum EvidenceDiscriminantKind<'a> {
    PreconditionFailed,
    SigningFailed,
    NoResponse(NoResponseCause),
    RemoteAuthStatus(&'a GrpcStatusDigest),
    Accepted(&'a str),
    Rejected(DpsReject, &'a DecodedResponseDigest),
    UnknownStatus(i32, &'a DecodedResponseDigest),
    SaveError(&'a DecodedResponseDigest),
    CloseAmbiguous(&'a DecodedResponseDigest),
    MissingStatus(&'a DecodedResponseDigest),
    OkButNoFiscalNumber(&'a DecodedResponseDigest),
}

/// The four migration-036 evidence columns, owned (serialization output).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceColumns {
    /// `evidence_kind` — the leaf tag (never NULL for a present discriminant).
    pub kind: &'static str,
    /// `evidence_text` — cause / verdict / fiscal number, or NULL.
    pub text: Option<String>,
    /// `evidence_code` — the raw UnknownStatus code, or NULL.
    pub code: Option<i32>,
    /// `evidence_digest` — the 32-byte decoded/gRPC digest, or NULL.
    pub digest: Option<[u8; 32]>,
}

/// Borrowed view of the four evidence columns as read from a durable row (hydration input).
#[derive(Clone, Copy, Debug)]
pub struct EvidenceColumnsRef<'a> {
    pub kind: &'a str,
    pub text: Option<&'a str>,
    pub code: Option<i32>,
    pub digest: Option<&'a [u8]>,
}

/// Why boot hydration of a durable evidence row failed (design §4.4 — fail loud).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceHydrationError {
    /// `evidence_kind` is not one of the 11 known leaf tags.
    UnknownKind(String),
    /// A `Rejected` row carried an `evidence_text` that is not a known `DpsReject` name.
    UnknownVerdict(String),
    /// A `NoResponse` row carried an `evidence_text` that is not a known `NoResponseCause`.
    UnknownCause(String),
    /// A digest-bearing leaf's `evidence_digest` was absent or not exactly 32 bytes.
    BadDigest {
        leaf: &'static str,
        len: Option<usize>,
    },
    /// A required `evidence_text` was absent (or empty for `Accepted`).
    MissingText(&'static str),
    /// A leaf carried a payload column it must not (fail-closed shape check).
    UnexpectedPayload {
        leaf: &'static str,
        column: &'static str,
    },
    /// An `UnknownStatus` row carried no `evidence_code`, or a forbidden (named) one.
    BadUnknownCode(Option<i32>),
}

impl EvidenceDiscriminant {
    /// Derive the exact evidence leaf from the sealed [`SubmissionEvidence`] (design §4.1).
    /// The sole domain constructor — guarantees the leaf is a real classifier input, never
    /// a hand-assembled illegal combination.
    pub fn from_evidence(evidence: &SubmissionEvidence) -> Self {
        let inner = match evidence {
            SubmissionEvidence::NotStarted { reason, .. } => match reason {
                // The BoundedText reason is a forensic diagnostic, not durable authority
                // (design §4.1/§4.3) — dropped here; the leaf is payload-free.
                PreflightRefusal::PreconditionFailed(_) => {
                    EvidenceDiscriminantInner::PreconditionFailed
                }
                PreflightRefusal::SigningFailed(_) => EvidenceDiscriminantInner::SigningFailed,
            },
            SubmissionEvidence::Started { response, .. } => match response.kind() {
                SendResponseKind::NoResponse(cause) => {
                    EvidenceDiscriminantInner::NoResponse(*cause)
                }
                SendResponseKind::RemoteStatus(ev) => match ev {
                    RemoteStatusEvidence::RemoteAuthStatus(digest) => {
                        EvidenceDiscriminantInner::RemoteAuthStatus(*digest)
                    }
                },
                SendResponseKind::Parsed(outcome) => match outcome.kind() {
                    SendOutcomeKind::Accepted(accepted) => {
                        // SentAccepted guarantees the fiscal number is non-empty (AL-3); we take
                        // the string directly (no transport re-mint — see the Accepted doc above).
                        EvidenceDiscriminantInner::Accepted(accepted.fiscal_number().to_string())
                    }
                    SendOutcomeKind::Rejected { verdict, digest } => {
                        EvidenceDiscriminantInner::Rejected(verdict, *digest)
                    }
                    SendOutcomeKind::Indeterminate(ind) => match ind.kind() {
                        SendIndeterminateKind::UnknownStatus { code, digest } => {
                            EvidenceDiscriminantInner::UnknownStatus(code, *digest)
                        }
                        SendIndeterminateKind::SaveError { digest } => {
                            EvidenceDiscriminantInner::SaveError(*digest)
                        }
                        SendIndeterminateKind::CloseAmbiguous { digest } => {
                            EvidenceDiscriminantInner::CloseAmbiguous(*digest)
                        }
                        SendIndeterminateKind::MissingStatus { digest } => {
                            EvidenceDiscriminantInner::MissingStatus(*digest)
                        }
                        SendIndeterminateKind::OkButNoFiscalNumber { digest } => {
                            EvidenceDiscriminantInner::OkButNoFiscalNumber(*digest)
                        }
                    },
                },
            },
        };
        Self(inner)
    }

    /// Read the sealed leaf for matching.
    pub fn kind(&self) -> EvidenceDiscriminantKind<'_> {
        match &self.0 {
            EvidenceDiscriminantInner::PreconditionFailed => {
                EvidenceDiscriminantKind::PreconditionFailed
            }
            EvidenceDiscriminantInner::SigningFailed => EvidenceDiscriminantKind::SigningFailed,
            EvidenceDiscriminantInner::NoResponse(c) => EvidenceDiscriminantKind::NoResponse(*c),
            EvidenceDiscriminantInner::RemoteAuthStatus(d) => {
                EvidenceDiscriminantKind::RemoteAuthStatus(d)
            }
            EvidenceDiscriminantInner::Accepted(f) => {
                EvidenceDiscriminantKind::Accepted(f.as_str())
            }
            EvidenceDiscriminantInner::Rejected(v, d) => EvidenceDiscriminantKind::Rejected(*v, d),
            EvidenceDiscriminantInner::UnknownStatus(c, d) => {
                EvidenceDiscriminantKind::UnknownStatus(*c, d)
            }
            EvidenceDiscriminantInner::SaveError(d) => EvidenceDiscriminantKind::SaveError(d),
            EvidenceDiscriminantInner::CloseAmbiguous(d) => {
                EvidenceDiscriminantKind::CloseAmbiguous(d)
            }
            EvidenceDiscriminantInner::MissingStatus(d) => {
                EvidenceDiscriminantKind::MissingStatus(d)
            }
            EvidenceDiscriminantInner::OkButNoFiscalNumber(d) => {
                EvidenceDiscriminantKind::OkButNoFiscalNumber(d)
            }
        }
    }

    /// The `evidence_kind` tag (leaf name) — byte-identical to migration 036's CASE keys.
    pub fn kind_str(&self) -> &'static str {
        match &self.0 {
            EvidenceDiscriminantInner::PreconditionFailed => "PreconditionFailed",
            EvidenceDiscriminantInner::SigningFailed => "SigningFailed",
            EvidenceDiscriminantInner::NoResponse(_) => "NoResponse",
            EvidenceDiscriminantInner::RemoteAuthStatus(_) => "RemoteAuthStatus",
            EvidenceDiscriminantInner::Accepted(_) => "Accepted",
            EvidenceDiscriminantInner::Rejected(..) => "Rejected",
            EvidenceDiscriminantInner::UnknownStatus(..) => "UnknownStatus",
            EvidenceDiscriminantInner::SaveError(_) => "SaveError",
            EvidenceDiscriminantInner::CloseAmbiguous(_) => "CloseAmbiguous",
            EvidenceDiscriminantInner::MissingStatus(_) => "MissingStatus",
            EvidenceDiscriminantInner::OkButNoFiscalNumber(_) => "OkButNoFiscalNumber",
        }
    }

    /// Serialize to the four migration-036 evidence columns (the record write, design §4.3).
    pub fn to_columns(&self) -> EvidenceColumns {
        let kind = self.kind_str();
        match &self.0 {
            EvidenceDiscriminantInner::PreconditionFailed
            | EvidenceDiscriminantInner::SigningFailed => EvidenceColumns {
                kind,
                text: None,
                code: None,
                digest: None,
            },
            EvidenceDiscriminantInner::NoResponse(c) => EvidenceColumns {
                kind,
                text: Some(c.as_str().to_string()),
                code: None,
                digest: None,
            },
            EvidenceDiscriminantInner::Accepted(f) => EvidenceColumns {
                kind,
                text: Some(f.clone()),
                code: None,
                digest: None,
            },
            EvidenceDiscriminantInner::Rejected(v, d) => EvidenceColumns {
                kind,
                text: Some(v.as_str().to_string()),
                code: None,
                digest: Some(*d.as_bytes()),
            },
            EvidenceDiscriminantInner::UnknownStatus(c, d) => EvidenceColumns {
                kind,
                text: None,
                code: Some(*c),
                digest: Some(*d.as_bytes()),
            },
            EvidenceDiscriminantInner::RemoteAuthStatus(d) => EvidenceColumns {
                kind,
                text: None,
                code: None,
                digest: Some(*d.as_bytes()),
            },
            EvidenceDiscriminantInner::SaveError(d)
            | EvidenceDiscriminantInner::CloseAmbiguous(d)
            | EvidenceDiscriminantInner::MissingStatus(d)
            | EvidenceDiscriminantInner::OkButNoFiscalNumber(d) => EvidenceColumns {
                kind,
                text: None,
                code: None,
                digest: Some(*d.as_bytes()),
            },
        }
    }

    /// Hydrate from the four durable evidence columns (boot resume, design §4.4).
    /// Fail-loud: an unknown tag / verdict / cause, a malformed digest, a missing required
    /// text, an out-of-set code, or an unexpected payload column each returns an error and
    /// leaves the caller to hold the fence rather than fabricate a leaf.
    pub fn from_columns(cols: EvidenceColumnsRef<'_>) -> Result<Self, EvidenceHydrationError> {
        use EvidenceHydrationError as E;

        // A digest-bearing leaf: require exactly 32 bytes; forbid text + code.
        let digest = |leaf: &'static str| -> Result<DecodedResponseDigest, E> {
            let bytes = cols.digest.ok_or(E::BadDigest { leaf, len: None })?;
            let arr: [u8; 32] = bytes.try_into().map_err(|_| E::BadDigest {
                leaf,
                len: Some(bytes.len()),
            })?;
            Ok(DecodedResponseDigest::from_durable_evidence(arr))
        };
        let no_text = |leaf: &'static str| -> Result<(), E> {
            match cols.text {
                None => Ok(()),
                Some(_) => Err(E::UnexpectedPayload {
                    leaf,
                    column: "evidence_text",
                }),
            }
        };
        let no_code = |leaf: &'static str| -> Result<(), E> {
            match cols.code {
                None => Ok(()),
                Some(_) => Err(E::UnexpectedPayload {
                    leaf,
                    column: "evidence_code",
                }),
            }
        };
        let no_digest = |leaf: &'static str| -> Result<(), E> {
            match cols.digest {
                None => Ok(()),
                Some(_) => Err(E::UnexpectedPayload {
                    leaf,
                    column: "evidence_digest",
                }),
            }
        };

        let inner = match cols.kind {
            "PreconditionFailed" => {
                no_text("PreconditionFailed")?;
                no_code("PreconditionFailed")?;
                no_digest("PreconditionFailed")?;
                EvidenceDiscriminantInner::PreconditionFailed
            }
            "SigningFailed" => {
                no_text("SigningFailed")?;
                no_code("SigningFailed")?;
                no_digest("SigningFailed")?;
                EvidenceDiscriminantInner::SigningFailed
            }
            "NoResponse" => {
                no_code("NoResponse")?;
                no_digest("NoResponse")?;
                let t = cols.text.ok_or(E::MissingText("NoResponse"))?;
                let cause =
                    NoResponseCause::from_stored(t).ok_or_else(|| E::UnknownCause(t.into()))?;
                EvidenceDiscriminantInner::NoResponse(cause)
            }
            "Accepted" => {
                no_code("Accepted")?;
                no_digest("Accepted")?;
                let t = cols.text.ok_or(E::MissingText("Accepted"))?;
                if t.is_empty() {
                    return Err(E::MissingText("Accepted"));
                }
                EvidenceDiscriminantInner::Accepted(t.to_string())
            }
            "Rejected" => {
                no_code("Rejected")?;
                let d = digest("Rejected")?;
                let t = cols.text.ok_or(E::MissingText("Rejected"))?;
                let v = DpsReject::from_stored(t).ok_or_else(|| E::UnknownVerdict(t.into()))?;
                EvidenceDiscriminantInner::Rejected(v, d)
            }
            "UnknownStatus" => {
                no_text("UnknownStatus")?;
                let d = digest("UnknownStatus")?;
                let code = cols.code.ok_or(E::BadUnknownCode(None))?;
                if UNKNOWN_STATUS_FORBIDDEN_CODES.contains(&code) {
                    return Err(E::BadUnknownCode(Some(code)));
                }
                EvidenceDiscriminantInner::UnknownStatus(code, d)
            }
            "RemoteAuthStatus" => {
                no_text("RemoteAuthStatus")?;
                no_code("RemoteAuthStatus")?;
                let bytes = cols.digest.ok_or(E::BadDigest {
                    leaf: "RemoteAuthStatus",
                    len: None,
                })?;
                let arr: [u8; 32] = bytes.try_into().map_err(|_| E::BadDigest {
                    leaf: "RemoteAuthStatus",
                    len: Some(bytes.len()),
                })?;
                EvidenceDiscriminantInner::RemoteAuthStatus(
                    GrpcStatusDigest::from_durable_evidence(arr),
                )
            }
            "SaveError" => {
                no_text("SaveError")?;
                no_code("SaveError")?;
                EvidenceDiscriminantInner::SaveError(digest("SaveError")?)
            }
            "CloseAmbiguous" => {
                no_text("CloseAmbiguous")?;
                no_code("CloseAmbiguous")?;
                EvidenceDiscriminantInner::CloseAmbiguous(digest("CloseAmbiguous")?)
            }
            "MissingStatus" => {
                no_text("MissingStatus")?;
                no_code("MissingStatus")?;
                EvidenceDiscriminantInner::MissingStatus(digest("MissingStatus")?)
            }
            "OkButNoFiscalNumber" => {
                no_text("OkButNoFiscalNumber")?;
                no_code("OkButNoFiscalNumber")?;
                EvidenceDiscriminantInner::OkButNoFiscalNumber(digest("OkButNoFiscalNumber")?)
            }
            other => return Err(E::UnknownKind(other.to_string())),
        };
        Ok(Self(inner))
    }

    /// The `(certainty, provenance)` axes this leaf implies — used to cross-check the
    /// stored axis columns during hydration / apply (design §4.1 leaf↔axes). `routing` and
    /// `node_effect` are verdict-derived for `Rejected`, so they are NOT asserted here (the
    /// SQL matrix + the classifier own that mapping); this covers the leaf-invariant axes.
    pub fn implied_axes(&self) -> (SubmissionCertainty, ResponseProvenance) {
        match &self.0 {
            EvidenceDiscriminantInner::PreconditionFailed
            | EvidenceDiscriminantInner::SigningFailed => (
                SubmissionCertainty::NotSubmitted,
                ResponseProvenance::NoResponse,
            ),
            EvidenceDiscriminantInner::NoResponse(_) => (
                SubmissionCertainty::SubmittedUnknown,
                ResponseProvenance::NoResponse,
            ),
            EvidenceDiscriminantInner::RemoteAuthStatus(_) => (
                SubmissionCertainty::SubmittedUnknown,
                ResponseProvenance::AuthenticatedPeer,
            ),
            EvidenceDiscriminantInner::Accepted(_) | EvidenceDiscriminantInner::Rejected(..) => (
                SubmissionCertainty::Submitted,
                ResponseProvenance::ParsedDpsEnvelope,
            ),
            EvidenceDiscriminantInner::UnknownStatus(..)
            | EvidenceDiscriminantInner::SaveError(_)
            | EvidenceDiscriminantInner::CloseAmbiguous(_)
            | EvidenceDiscriminantInner::MissingStatus(_)
            | EvidenceDiscriminantInner::OkButNoFiscalNumber(_) => (
                SubmissionCertainty::SubmittedUnknown,
                ResponseProvenance::ParsedDpsEnvelope,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::{
        BoundedText, DpsProtocolBinding, DpsProtocolId, EnvelopeHash, NonEmptyFiscalNumber,
        NonOkStatusCode, ProtocolContractVersion, SendOutcome, SendResponse,
    };
    use crate::enums::DocType;

    fn binding() -> DpsProtocolBinding {
        DpsProtocolBinding {
            protocol_id: DpsProtocolId::FscoZzd,
            contract_version: ProtocolContractVersion(1),
            capability_profile_version: None,
            endpoint_config_revision: None,
        }
    }
    fn hash() -> EnvelopeHash {
        EnvelopeHash([0u8; 32])
    }
    fn dig() -> DecodedResponseDigest {
        DecodedResponseDigest::from_transport_digest([0xAB; 32])
    }
    fn grpc() -> GrpcStatusDigest {
        GrpcStatusDigest::from_transport_digest([0xCD; 32])
    }
    fn started(response: SendResponse) -> SubmissionEvidence {
        SubmissionEvidence::Started {
            response,
            binding: binding(),
            envelope_hash: hash(),
        }
    }
    fn not_started(reason: PreflightRefusal) -> SubmissionEvidence {
        SubmissionEvidence::NotStarted {
            reason,
            binding: binding(),
            envelope_hash: hash(),
        }
    }
    fn parsed(o: SendOutcome) -> SubmissionEvidence {
        started(SendResponse::parsed(o))
    }
    fn from_code(code: i32, dt: DocType) -> SubmissionEvidence {
        parsed(SendOutcome::from_server_code(
            NonOkStatusCode::from_transport(code).unwrap(),
            dt,
            dig(),
        ))
    }

    /// One `SubmissionEvidence` per leaf (name expected from `kind_str`).
    fn all_leaves() -> Vec<(&'static str, SubmissionEvidence)> {
        vec![
            (
                "PreconditionFailed",
                not_started(PreflightRefusal::PreconditionFailed(
                    BoundedText::from_truncating("g"),
                )),
            ),
            (
                "SigningFailed",
                not_started(PreflightRefusal::SigningFailed(
                    BoundedText::from_truncating("s"),
                )),
            ),
            (
                "NoResponse",
                started(SendResponse::no_response(NoResponseCause::Timeout)),
            ),
            (
                "RemoteAuthStatus",
                started(SendResponse::remote_status(
                    RemoteStatusEvidence::RemoteAuthStatus(grpc()),
                )),
            ),
            (
                "Accepted",
                parsed(SendOutcome::accepted(
                    NonEmptyFiscalNumber::from_transport("4000123456".into()).unwrap(),
                )),
            ),
            ("Rejected", from_code(-1, DocType::Sell)),
            ("UnknownStatus", from_code(-99, DocType::Sell)),
            ("SaveError", from_code(-3, DocType::Sell)),
            ("CloseAmbiguous", from_code(-2, DocType::ShiftClose)),
            ("MissingStatus", parsed(SendOutcome::missing_status(dig()))),
            (
                "OkButNoFiscalNumber",
                parsed(SendOutcome::ok_but_no_fiscal_number(dig())),
            ),
        ]
    }

    #[test]
    fn roundtrip_all_eleven_leaves() {
        let leaves = all_leaves();
        assert_eq!(leaves.len(), 11, "one evidence per leaf");
        for (name, ev) in leaves {
            let disc = EvidenceDiscriminant::from_evidence(&ev);
            assert_eq!(disc.kind_str(), name, "kind_str mismatch for {name}");
            let cols = disc.to_columns();
            assert_eq!(cols.kind, name);
            let re = EvidenceDiscriminant::from_columns(EvidenceColumnsRef {
                kind: cols.kind,
                text: cols.text.as_deref(),
                code: cols.code,
                digest: cols.digest.as_ref().map(|d| d.as_slice()),
            })
            .unwrap_or_else(|e| panic!("hydration failed for {name}: {e:?}"));
            assert_eq!(disc, re, "round-trip mismatch for {name}");
        }
    }

    #[test]
    fn accepted_carries_exact_fiscal_number() {
        let ev = parsed(SendOutcome::accepted(
            NonEmptyFiscalNumber::from_transport("4000987654".into()).unwrap(),
        ));
        let cols = EvidenceDiscriminant::from_evidence(&ev).to_columns();
        assert_eq!(cols.kind, "Accepted");
        assert_eq!(cols.text.as_deref(), Some("4000987654"));
        assert!(cols.digest.is_none() && cols.code.is_none());
    }

    #[test]
    fn hydration_fails_loud_on_garbage() {
        use EvidenceHydrationError as E;
        type Case<'a> = (EvidenceColumnsRef<'a>, fn(&E) -> bool);
        let d = [0u8; 32];
        let short = [0u8; 16];
        let cases: Vec<Case<'_>> = vec![
            (
                EvidenceColumnsRef {
                    kind: "Bogus",
                    text: None,
                    code: None,
                    digest: None,
                },
                |e| matches!(e, E::UnknownKind(_)),
            ),
            (
                EvidenceColumnsRef {
                    kind: "Rejected",
                    text: Some("NotAVerdict"),
                    code: None,
                    digest: Some(&d),
                },
                |e| matches!(e, E::UnknownVerdict(_)),
            ),
            (
                EvidenceColumnsRef {
                    kind: "NoResponse",
                    text: Some("Nonsense"),
                    code: None,
                    digest: None,
                },
                |e| matches!(e, E::UnknownCause(_)),
            ),
            (
                EvidenceColumnsRef {
                    kind: "SaveError",
                    text: None,
                    code: None,
                    digest: Some(&short),
                },
                |e| matches!(e, E::BadDigest { .. }),
            ),
            (
                EvidenceColumnsRef {
                    kind: "Accepted",
                    text: Some(""),
                    code: None,
                    digest: None,
                },
                |e| matches!(e, E::MissingText("Accepted")),
            ),
            (
                EvidenceColumnsRef {
                    kind: "UnknownStatus",
                    text: None,
                    code: Some(-12),
                    digest: Some(&d),
                },
                |e| matches!(e, E::BadUnknownCode(Some(-12))),
            ),
            (
                EvidenceColumnsRef {
                    kind: "NoResponse",
                    text: Some("Timeout"),
                    code: None,
                    digest: Some(&d),
                },
                |e| matches!(e, E::UnexpectedPayload { .. }),
            ),
        ];
        for (cols, pred) in cases {
            let kind = cols.kind;
            let err = EvidenceDiscriminant::from_columns(cols)
                .expect_err(&format!("{kind} must fail loud"));
            assert!(pred(&err), "wrong error for {kind}: {err:?}");
        }
    }
}
