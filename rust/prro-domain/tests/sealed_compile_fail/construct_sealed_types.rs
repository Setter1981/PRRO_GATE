//! CS-3 Bridge-0 R2 sealing canary fixture — MUST NOT COMPILE.
//!
//! Proves the sealed delivery algebra cannot be constructed with illegal states from
//! OUTSIDE `prro-domain`. Every struct below has PRIVATE fields, so an external
//! struct-literal — which could otherwise fabricate an empty `fiscal_number`, a
//! too-short `Kvt1Raw`, or an axis quadruple `classify` can never emit — is rejected
//! by the privacy checker (E0451 / E0603). If ANY of these fields were made `pub`,
//! this fixture would start compiling and the driver test
//! (`sealed_delivery_types_cannot_be_constructed_externally`) would FAIL.
//!
//! This is the checkpoint-audit "illegal-state canary": the sealing is only real if
//! the illegal construction genuinely cannot be written.

use prro_domain::delivery::{
    AuthorizedGeneration, ClassifiedOutcome, Kvt1Raw, NodeEffect, ObservedOutcomeV1,
    ResponseProvenance, SentAccepted, SubmissionCertainty,
};

fn main() {
    // 1. SentAccepted with an EMPTY fiscal_number — bypasses the AL-3 `observe()` guard.
    let _empty_fiscal = SentAccepted {
        fiscal_number: String::new(),
    };

    // 2. Kvt1Raw with a too-short blob — bypasses the `len >= 64` `new()` invariant.
    let _short_quittance = Kvt1Raw(vec![0u8; 8]);

    // 3. ClassifiedOutcome with an ILLEGAL axis combo (Submitted + NoResponse — a
    //    definitive verdict with no response, which `classify` can never produce).
    let _illegal_axes = ClassifiedOutcome {
        certainty: SubmissionCertainty::Submitted,
        provenance: ResponseProvenance::NoResponse,
        routing: None,
        node_effect: NodeEffect::NoNodeEffect,
    };

    // 4. ObservedOutcomeV1 hand-assembled with an illegal axis combo (Submitted + NoResponse
    //    + a NotStarted generation that contradicts the certainty) — bypasses `record()`'s
    //    validation and the classifier-sourced quadruple. (A negative generation is no longer
    //    even expressible: `AuthorizedGeneration::Started` wraps a `PositiveGeneration >= 1`.)
    //    The value type-checks, so the ONLY reason this is rejected is the private fields —
    //    which keeps the teeth honest (unseal the fields → this line compiles).
    let _forged_record = ObservedOutcomeV1 {
        certainty: SubmissionCertainty::Submitted,
        provenance: ResponseProvenance::NoResponse,
        routing: None,
        remote_correlation_id: None,
        node_effect: NodeEffect::NoNodeEffect,
        authorized_generation: AuthorizedGeneration::NotStarted,
    };
}
