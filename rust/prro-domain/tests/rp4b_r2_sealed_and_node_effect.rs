//! CS-3 Bridge-0 R2 — sealed delivery algebra + classifier RETURNS node_effect.
//!
//! **Audit finding (checkpoint NO-GO):** `classify` computed the `NodeEffect` and
//! DISCARDED it (`let (routing, _node_effect) = …`), so `ClassifiedOutcome` did not
//! carry it — every downstream consumer (the 033 `node_effect` column, cs3_c_db) had
//! to RE-DERIVE it independently → drift risk. R4 then made `node_effect` a REQUIRED
//! durable column (H3). This test pins that `classify` is the single source of truth
//! for `node_effect` on EVERY path, and that the sealed mint (`ObservedOutcomeV1::record`)
//! copies it from the classifier output (no independent injection).
//!
//! RED-first: written BEFORE `ClassifiedOutcome::node_effect()`, the getters, and
//! `ObservedOutcomeV1::record` exist — must fail to compile, then pass.

use prro_domain::delivery::{
    classify, ActiveRetryClass as ARC, BoundedText, DpsProtocolBinding, DpsProtocolId, DpsReject,
    EnvelopeHash, Kvt1Raw, NoResponseCause, NodeEffect as NE, ObservedOutcomeV1, PreflightRefusal,
    ProtocolContractVersion, RawResponseDigest, RemoteStatusEvidence, ResponseProvenance as RP,
    SendIndeterminate, SendOutcome, SendResponse, SentAccepted, SubmissionCertainty as SC,
    SubmissionEvidence,
};
use prro_domain::enums::DocType;

// ─── minimal evidence builders ───────────────────────────────────────────────

fn binding() -> DpsProtocolBinding {
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}

fn started(response: SendResponse) -> SubmissionEvidence {
    SubmissionEvidence::Started {
        response,
        binding: binding(),
        envelope_hash: EnvelopeHash([0u8; 32]),
    }
}

fn not_started(reason: PreflightRefusal) -> SubmissionEvidence {
    SubmissionEvidence::NotStarted {
        reason,
        binding: binding(),
        envelope_hash: EnvelopeHash([0u8; 32]),
    }
}

fn parsed_reject(v: DpsReject) -> SubmissionEvidence {
    started(SendResponse::Parsed(SendOutcome::Rejected(v)))
}

// ─── R2-A: classify RETURNS node_effect on EVERY path (the keystone) ─────────

/// The keystone: `-11 Offline168` and `-1 Verify` share the SAME
/// `(Submitted, ParsedDpsEnvelope, TerminalReject)` triple but have DIFFERENT
/// node effects — `classify` must surface that discriminant, not discard it.
#[test]
fn classify_surfaces_node_effect_offline168_vs_verify() {
    let offline168 = classify(&parsed_reject(DpsReject::Offline168), DocType::Sell);
    let verify = classify(&parsed_reject(DpsReject::Verify), DocType::Sell);

    // Same routing triple...
    assert_eq!(offline168.routing(), Some(ARC::TerminalReject));
    assert_eq!(verify.routing(), Some(ARC::TerminalReject));
    assert_eq!(offline168.certainty(), SC::Submitted);
    assert_eq!(verify.certainty(), SC::Submitted);
    // ...but DIFFERENT node_effect (previously DISCARDED by the classifier).
    assert_eq!(offline168.node_effect(), NE::NodeBlocked);
    assert_eq!(verify.node_effect(), NE::NoNodeEffect);
}

/// node_effect is produced on the NON-reject paths too (not just reject).
#[test]
fn classify_surfaces_node_effect_on_all_paths() {
    // NotStarted / SigningFailed → WrapperBug routing → WrapperBug effect.
    let signing = classify(
        &not_started(PreflightRefusal::SigningFailed(
            BoundedText::from_truncating("sign"),
        )),
        DocType::Sell,
    );
    assert_eq!(signing.routing(), Some(ARC::WrapperBug));
    assert_eq!(signing.node_effect(), NE::WrapperBug);

    // NotStarted / PreconditionFailed → TransientRetry → NoNodeEffect.
    let precond = classify(
        &not_started(PreflightRefusal::PreconditionFailed(
            BoundedText::from_truncating("guard"),
        )),
        DocType::Sell,
    );
    assert_eq!(precond.node_effect(), NE::NoNodeEffect);

    // NoResponse(Timeout) → TransientRetry → NoNodeEffect.
    let timeout = classify(
        &started(SendResponse::NoResponse(NoResponseCause::Timeout)),
        DocType::Sell,
    );
    assert_eq!(timeout.node_effect(), NE::NoNodeEffect);

    // RemoteStatus → ProbeRequired routing → ProbeRequired effect.
    let remote = classify(
        &started(SendResponse::RemoteStatus(
            RemoteStatusEvidence::RemoteAuthStatus(RawResponseDigest([0u8; 32])),
        )),
        DocType::Sell,
    );
    assert_eq!(remote.routing(), Some(ARC::ProbeRequired));
    assert_eq!(remote.node_effect(), NE::ProbeRequired);

    // Accepted → clean (routing None) → NoNodeEffect.
    let accepted = classify(
        &started(SendResponse::Parsed(SendOutcome::Accepted(
            SentAccepted::observe("DPS-1").unwrap(),
        ))),
        DocType::Sell,
    );
    assert_eq!(accepted.routing(), None);
    assert_eq!(accepted.node_effect(), NE::NoNodeEffect);

    // Indeterminate(CloseAmbiguous) → ProbeRequired → ProbeRequired.
    let close = classify(
        &started(SendResponse::Parsed(SendOutcome::Indeterminate(
            SendIndeterminate::CloseAmbiguous,
        ))),
        DocType::Sell,
    );
    assert_eq!(close.node_effect(), NE::ProbeRequired);

    // Indeterminate(SaveError, -3) → TransientRetry → NoNodeEffect.
    let save = classify(
        &started(SendResponse::Parsed(SendOutcome::Indeterminate(
            SendIndeterminate::SaveError,
        ))),
        DocType::Sell,
    );
    assert_eq!(save.node_effect(), NE::NoNodeEffect);
}

// ─── R2-B: ObservedOutcomeV1::record — sole ctor, mints FROM a classified out ──

/// `record` copies the classifier's node_effect — it cannot be independently
/// injected (there is no node_effect parameter), so payload can never drift from
/// what `classify` derived.
#[test]
fn record_copies_node_effect_from_classified() {
    let classified = classify(&parsed_reject(DpsReject::Offline168), DocType::Sell);
    let rec = ObservedOutcomeV1::record(&classified, Some(BoundedText::from_truncating("corr")), 5)
        .expect("gen >= 1 is valid");

    assert_eq!(rec.certainty(), SC::Submitted);
    assert_eq!(rec.provenance(), RP::ParsedDpsEnvelope);
    assert_eq!(rec.routing(), Some(ARC::TerminalReject));
    assert_eq!(rec.node_effect(), NE::NodeBlocked); // copied from classify, not injected
    assert_eq!(rec.authorized_generation(), 5);
    assert_eq!(
        rec.remote_correlation_id().map(|t| t.as_str()),
        Some("corr")
    );
}

/// `record` rejects a non-positive authorized_generation (033 CHECK: NULL or >= 1;
/// at OO it is non-NULL, so >= 1).
#[test]
fn record_rejects_non_positive_generation() {
    let classified = classify(&parsed_reject(DpsReject::Verify), DocType::Sell);
    assert!(ObservedOutcomeV1::record(&classified, None, 0).is_err());
    assert!(ObservedOutcomeV1::record(&classified, None, -1).is_err());
    assert!(ObservedOutcomeV1::record(&classified, None, 1).is_ok());
}

// ─── R2-C: Kvt1Raw sealed — non-empty / length invariant ─────────────────────

/// `Kvt1Raw::new` enforces the len >= 64 quittance invariant (was a bare
/// `pub Vec<u8>` that let `Kvt1Raw(vec![])` violate it).
#[test]
fn kvt1_raw_enforces_min_length() {
    assert!(Kvt1Raw::new(vec![0u8; 63]).is_none());
    assert!(Kvt1Raw::new(vec![0u8; 64]).is_some());
    let k = Kvt1Raw::new(vec![7u8; 100]).unwrap();
    assert_eq!(k.as_bytes().len(), 100);
}

// ─── R2-D: SentAccepted getter (field is now private) ────────────────────────

#[test]
fn sent_accepted_getter_reads_fiscal_number() {
    let sa = SentAccepted::observe("DPS-42").unwrap();
    assert_eq!(sa.fiscal_number(), "DPS-42");
    assert!(SentAccepted::observe("").is_none());
}
