//! CS-3 3.2 PR2 — transport-minted provenance types + the new NoResponseCause.
//!
//! `NonEmptyFiscalNumber` / `NonOkStatusCode` enforce their invariants at the (transport-only) mint;
//! `CallFailedWithoutTrustedDpsEnvelope` classifies like every other no-response cause
//! (`SubmittedUnknown` / `TransientRetry`) — no blind resend.

use prro_domain::delivery::{
    classify, ActiveRetryClass, DpsProtocolBinding, DpsProtocolId, EnvelopeHash, NoResponseCause,
    NonEmptyFiscalNumber, NonOkStatusCode, ProtocolContractVersion, ResponseProvenance,
    SendResponse, SubmissionCertainty, SubmissionEvidence,
};

#[test]
fn non_empty_fiscal_number_rejects_empty() {
    assert!(NonEmptyFiscalNumber::from_transport(String::new()).is_none());
    let n = NonEmptyFiscalNumber::from_transport("DPS-42".to_string()).unwrap();
    assert_eq!(n.as_str(), "DPS-42");
}

#[test]
fn non_ok_status_code_rejects_ok_and_unknown() {
    assert!(NonOkStatusCode::from_transport(0).is_none()); // 0 = UNKNOWN → MissingStatus, not ServerCode
    assert!(NonOkStatusCode::from_transport(1).is_none()); // 1 = OK → Accepted / empty-id, not ServerCode
    assert_eq!(NonOkStatusCode::from_transport(-1).unwrap().get(), -1);
    assert_eq!(NonOkStatusCode::from_transport(-11).unwrap().get(), -11);
    assert_eq!(NonOkStatusCode::from_transport(-4).unwrap().get(), -4);
}

fn binding() -> DpsProtocolBinding {
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}

#[test]
fn call_failed_without_trusted_dps_envelope_classifies_submitted_unknown() {
    let ev = SubmissionEvidence::Started {
        response: SendResponse::NoResponse(NoResponseCause::CallFailedWithoutTrustedDpsEnvelope),
        binding: binding(),
        envelope_hash: EnvelopeHash([0u8; 32]),
    };
    let out = classify(&ev);
    // Same class as every other no-response cause: SubmittedUnknown / NoResponse / TransientRetry.
    assert_eq!(out.certainty(), SubmissionCertainty::SubmittedUnknown);
    assert_eq!(out.provenance(), ResponseProvenance::NoResponse);
    assert_eq!(out.routing(), Some(ActiveRetryClass::TransientRetry));
}
