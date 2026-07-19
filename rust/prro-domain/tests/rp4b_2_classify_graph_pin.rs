//! RP4B-2 graph-pin (§10 #4B rev-6) — the total classifier test.
//!
//! **Purpose:** enumerate ALL `SubmissionEvidence` discriminant shapes; classify each;
//! assert the **full mapping** `{(evidence-discriminant, doc_type) → (certainty, provenance, routing)}`
//! equals the normative graph from #4B §2. An Accepted↔Rejected swap would yield the
//! SAME image set but a different graph — this test catches that because it compares
//! the full input→output pairs, not just the image.
//!
//! **Normative graph source:** #4B §2 table (quoted verbatim in NORMATIVE_GRAPH below).
//! Encoded INDEPENDENTLY from `classify` — not by calling `classify`.
//!
//! **Also covers RP4B-3:** `-4` ⇒ `{SubmittedUnknown, Parsed, TransientRetry}`;
//! `-3` same row; bare timeout ⇒ `{SubmittedUnknown, NoResponse, TransientRetry}`;
//! `-1` ⇒ `{Submitted, Parsed, TerminalReject}` — all distinct.
//!
//! RED-first: this test is written BEFORE the types/classifier exist; it must compile
//! and FAIL (type-not-found or classification mismatch) before the implementation lands.

use prro_domain::delivery::{
    classify, ActiveRetryClass, AuthorizedGeneration, BoundedText, DecodedResponseDigest,
    DpsProtocolBinding, DpsProtocolId, EnvelopeHash, GrpcStatusDigest, NoResponseCause, NodeEffect,
    NonEmptyFiscalNumber, NonOkStatusCode, PositiveGeneration, PreflightRefusal,
    ProtocolContractVersion, RemoteStatusEvidence, ResponseProvenance, SendOutcome, SendResponse,
    SentAccepted, SubmissionCertainty, SubmissionEvidence,
};
use prro_domain::enums::DocType;

// ─── Test-local raw-status shim ──────────────────────────────────────────────
//
// The domain `RawDpsStatus` enum + `SendOutcome::from_dps_status` were RETIRED
// (PR4: honest transport-minted provenance; no `SendOutcome` is built from a raw
// wire status any more). This test's graph is keyed on the SAME raw (code / fiscal_id,
// doc_type) inputs, so we keep a byte-identical local shim of the deleted shape and
// translate it to the new sealed ctors in the SOLE `parsed()` helper below. Every
// graph row therefore still reads with the exact same `RawDpsStatus::Error(-N)` /
// `RawDpsStatus::Ok { fiscal_id }` input, preserving the normative-table intent verbatim.
#[derive(Clone, Copy, Debug)]
enum RawDpsStatus<'a> {
    Ok { fiscal_id: &'a str },
    Error(i32),
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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

fn digest() -> DecodedResponseDigest {
    DecodedResponseDigest::from_transport_digest([0xAB; 32])
}

fn grpc_digest() -> GrpcStatusDigest {
    GrpcStatusDigest::from_transport_digest([0xAB; 32])
}

fn not_started_preflight() -> SubmissionEvidence {
    SubmissionEvidence::NotStarted {
        reason: PreflightRefusal::PreconditionFailed(BoundedText::from_truncating("guard failed")),
        binding: binding(),
        envelope_hash: hash(),
    }
}

fn not_started_signing() -> SubmissionEvidence {
    SubmissionEvidence::NotStarted {
        reason: PreflightRefusal::SigningFailed(BoundedText::from_truncating("sign error")),
        binding: binding(),
        envelope_hash: hash(),
    }
}

fn started(response: SendResponse) -> SubmissionEvidence {
    SubmissionEvidence::Started {
        response,
        binding: binding(),
        envelope_hash: hash(),
    }
}

/// Build a `Started{Parsed}` evidence from a raw DPS status via the new sealed ctors.
/// `dt` MUST equal the row's classify doc_type: the `-2/-15` close/non-close split happens
/// INSIDE `SendOutcome::from_server_code`, so a mismatched `dt` would build the wrong outcome.
/// This is the SOLE translation point from the retired `from_dps_status` shape to the
/// transport-minted ctors (`from_server_code` / `accepted` / `ok_but_no_fiscal_number`).
fn parsed(status: RawDpsStatus<'_>, dt: DocType) -> SubmissionEvidence {
    let outcome = match status {
        RawDpsStatus::Error(code) => SendOutcome::from_server_code(
            NonOkStatusCode::from_transport(code).unwrap(),
            dt,
            digest(),
        ),
        RawDpsStatus::Ok { fiscal_id: "" } => SendOutcome::ok_but_no_fiscal_number(digest()),
        RawDpsStatus::Ok { fiscal_id } => SendOutcome::accepted(
            NonEmptyFiscalNumber::from_transport(fiscal_id.to_string()).unwrap(),
        ),
    };
    started(SendResponse::parsed(outcome))
}

struct ExpectedAxes {
    certainty: SubmissionCertainty,
    provenance: ResponseProvenance,
    routing: Option<ActiveRetryClass>,
    node_effect: NodeEffect,
}

fn expected(
    certainty: SubmissionCertainty,
    provenance: ResponseProvenance,
    routing: Option<ActiveRetryClass>,
    node_effect: NodeEffect,
) -> ExpectedAxes {
    ExpectedAxes {
        certainty,
        provenance,
        routing,
        node_effect,
    }
}

use ActiveRetryClass as ARC;
use NodeEffect as NE;
use ResponseProvenance as RP;
use SubmissionCertainty as SC;

// ─── Normative graph (from #4B §2 — encoded independently) ──────────────────
//
// Rows: (label, evidence, doc_type, expected_certainty, expected_provenance, expected_routing)
// The normative §2 table:
//   NotStarted{reason=preflight} → NotSubmitted / NoResponse / TransientRetry
//   NotStarted{reason=signing}   → NotSubmitted / NoResponse / WrapperBug
//   Started{NoResponse(Timeout)} → SubmittedUnknown / NoResponse / TransientRetry
//   Started{NoResponse(Crashed)} → SubmittedUnknown / NoResponse / TransientRetry
//   Started{NoResponse(Handshake)} → SubmittedUnknown / NoResponse / TransientRetry
//   Started{NoResponse(Cancelled)} → SubmittedUnknown / NoResponse / TransientRetry
//   Started{RemoteStatus(AuthStatus)} → SubmittedUnknown / AuthenticatedPeer / ProbeRequired
//     (the RemoteStatus(Garbage) row was deleted — variant removed in PR4)
//   Started{Parsed(Accepted)}   → Submitted / ParsedDpsEnvelope / None
//   Started{Parsed(Rejected(-1 Verify))}   → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(-5 Type))}     → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(-6 NotPrevZ))} → Submitted / ParsedDpsEnvelope / OperatorEscalation
//   Started{Parsed(Rejected(-7 Xml))}      → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(-8 XmlDate))}  → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(-9 XmlChk))}   → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(-10 XmlZR))}   → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(-11 Off168))}  → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(-12 BadHash))} → Submitted / ParsedDpsEnvelope / MacRecovery
//   Started{Parsed(Rejected(-13 NotRroReg))} → Submitted / ParsedDpsEnvelope / FnConfigError
//   Started{Parsed(Rejected(-14 NotSignReg))} → Submitted / ParsedDpsEnvelope / FnConfigError
//   Started{Parsed(Rejected(-16 OffId))}   → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Rejected(Close on SELL))} → Submitted / ParsedDpsEnvelope / TerminalReject
//   Started{Parsed(Indeterminate(Unknown))} → SubmittedUnknown / ParsedDpsEnvelope / TransientRetry
//   Started{Parsed(Indeterminate(SaveError))} → SubmittedUnknown / ParsedDpsEnvelope / TransientRetry
//   Started{Parsed(Indeterminate(CloseAmbig on Z_REPORT))} → SubmittedUnknown / ParsedDpsEnvelope / ProbeRequired
//   Started{Parsed(Indeterminate(OkNoFn))} → SubmittedUnknown / ParsedDpsEnvelope / ProbeRequired

struct GraphRow {
    label: &'static str,
    evidence: SubmissionEvidence,
    doc_type: DocType,
    expected: ExpectedAxes,
}

fn normative_graph() -> Vec<GraphRow> {
    vec![
        // ── NotStarted ──────────────────────────────────────────────────────
        GraphRow {
            label: "NotStarted/PreconditionFailed → NotSubmitted/NoResponse/TransientRetry",
            evidence: not_started_preflight(),
            doc_type: DocType::Sell,
            expected: expected(SC::NotSubmitted, RP::NoResponse, Some(ARC::TransientRetry), NE::NoNodeEffect),
        },
        GraphRow {
            label: "NotStarted/SigningFailed → NotSubmitted/NoResponse/WrapperBug",
            evidence: not_started_signing(),
            doc_type: DocType::Sell,
            expected: expected(SC::NotSubmitted, RP::NoResponse, Some(ARC::WrapperBug), NE::WrapperBug),
        },

        // ── Started{NoResponse} ─────────────────────────────────────────────
        GraphRow {
            label: "Started/NoResponse(Timeout) → SubmittedUnknown/NoResponse/TransientRetry",
            evidence: started(SendResponse::no_response(NoResponseCause::Timeout)),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::NoResponse, Some(ARC::TransientRetry), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/NoResponse(Cancelled) → SubmittedUnknown/NoResponse/TransientRetry",
            evidence: started(SendResponse::no_response(NoResponseCause::Cancelled)),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::NoResponse, Some(ARC::TransientRetry), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/NoResponse(LocalHandshake) → SubmittedUnknown/NoResponse/TransientRetry",
            evidence: started(SendResponse::no_response(NoResponseCause::LocalHandshakeFailure)),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::NoResponse, Some(ARC::TransientRetry), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/NoResponse(Crashed) → SubmittedUnknown/NoResponse/TransientRetry (RP4B-1)",
            evidence: started(SendResponse::no_response(NoResponseCause::CrashedBeforeObservation)),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::NoResponse, Some(ARC::TransientRetry), NE::NoNodeEffect),
        },

        // ── Started{RemoteStatus} ───────────────────────────────────────────
        // NOTE: the `AuthenticatedPeerGarbage` row was DELETED — that
        // `RemoteStatusEvidence` variant was removed in PR4. The surviving
        // `RemoteAuthStatus` row keeps the SubmittedUnknown/AuthenticatedPeer/
        // ProbeRequired graph edge for the RemoteStatus provenance class.
        GraphRow {
            label: "Started/RemoteStatus(AuthStatus) → SubmittedUnknown/AuthenticatedPeer/ProbeRequired",
            evidence: started(SendResponse::remote_status(RemoteStatusEvidence::RemoteAuthStatus(grpc_digest()))),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::AuthenticatedPeer, Some(ARC::ProbeRequired), NE::ProbeRequired),
        },

        // ── Started{Parsed(Accepted)} ───────────────────────────────────────
        GraphRow {
            label: "Started/Parsed(Accepted) → Submitted/ParsedDpsEnvelope/None (NULL clean-accept)",
            evidence: parsed(RawDpsStatus::Ok { fiscal_id: "DPS-123" }, DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, None, NE::NoNodeEffect),
        },

        // ── Started{Parsed(Rejected)} — named DpsReject codes via from_server_code ──
        GraphRow {
            label: "Started/Parsed(Rejected(Verify/-1)) → Submitted/Parsed/TerminalReject (RP4B-3)",
            evidence: parsed(RawDpsStatus::Error(-1), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(Type/-5)) → Submitted/Parsed/TerminalReject",
            evidence: parsed(RawDpsStatus::Error(-5), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(NotPrevZReport/-6)) → Submitted/Parsed/OperatorEscalation",
            evidence: parsed(RawDpsStatus::Error(-6), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::OperatorEscalation), NE::OperatorEscalation),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(Xml/-7)) → Submitted/Parsed/TerminalReject",
            evidence: parsed(RawDpsStatus::Error(-7), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(XmlDate/-8)) → Submitted/Parsed/TerminalReject",
            evidence: parsed(RawDpsStatus::Error(-8), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(XmlChk/-9)) → Submitted/Parsed/TerminalReject",
            evidence: parsed(RawDpsStatus::Error(-9), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(XmlZReport/-10)) → Submitted/Parsed/TerminalReject",
            evidence: parsed(RawDpsStatus::Error(-10), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(Offline168/-11)) → Submitted/Parsed/TerminalReject",
            evidence: parsed(RawDpsStatus::Error(-11), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NodeBlocked),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(BadHashPrev/-12)) → Submitted/Parsed/MacRecovery",
            evidence: parsed(RawDpsStatus::Error(-12), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::MacRecovery), NE::MacReseedPending),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(NotRegisteredRro/-13)) → Submitted/Parsed/FnConfigError",
            evidence: parsed(RawDpsStatus::Error(-13), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::FnConfigError), NE::FnConfigError),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(NotRegisteredSigner/-14)) → Submitted/Parsed/FnConfigError",
            evidence: parsed(RawDpsStatus::Error(-14), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::FnConfigError), NE::FnConfigError),
        },
        GraphRow {
            label: "Started/Parsed(Rejected(OfflineId/-16)) → Submitted/Parsed/TerminalReject",
            evidence: parsed(RawDpsStatus::Error(-16), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },
        // -2 on a non-close doc type → Rejected(Close) → TerminalReject (§12 R1)
        GraphRow {
            label: "Started/Parsed(Rejected(Close)) on Sell → Submitted/Parsed/TerminalReject (§12 R1)",
            evidence: parsed(RawDpsStatus::Error(-2), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::Submitted, RP::ParsedDpsEnvelope, Some(ARC::TerminalReject), NE::NoNodeEffect),
        },

        // ── Started{Parsed(Indeterminate)} ──────────────────────────────────
        GraphRow {
            label: "Started/Parsed(Indeterminate(Unknown/-4)) → SubmittedUnknown/Parsed/TransientRetry (RP4B-3)",
            evidence: parsed(RawDpsStatus::Error(-4), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::ParsedDpsEnvelope, Some(ARC::TransientRetry), NE::NoNodeEffect),
        },
        GraphRow {
            label: "Started/Parsed(Indeterminate(SaveError/-3)) → SubmittedUnknown/Parsed/TransientRetry (RP4B-3)",
            evidence: parsed(RawDpsStatus::Error(-3), DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::ParsedDpsEnvelope, Some(ARC::TransientRetry), NE::NoNodeEffect),
        },
        // -2 on Z_REPORT → CloseAmbiguous → ProbeRequired (§12 R1 close-shift split)
        GraphRow {
            label: "Started/Parsed(Indeterminate(CloseAmbiguous)) on ZReport → SubmittedUnknown/Parsed/ProbeRequired",
            evidence: parsed(RawDpsStatus::Error(-2), DocType::ZReport),
            doc_type: DocType::ZReport,
            expected: expected(SC::SubmittedUnknown, RP::ParsedDpsEnvelope, Some(ARC::ProbeRequired), NE::ProbeRequired),
        },
        // -2 on ShiftClose → CloseAmbiguous → ProbeRequired (§12 R1)
        GraphRow {
            label: "Started/Parsed(Indeterminate(CloseAmbiguous)) on ShiftClose → SubmittedUnknown/Parsed/ProbeRequired",
            evidence: parsed(RawDpsStatus::Error(-2), DocType::ShiftClose),
            doc_type: DocType::ShiftClose,
            expected: expected(SC::SubmittedUnknown, RP::ParsedDpsEnvelope, Some(ARC::ProbeRequired), NE::ProbeRequired),
        },
        GraphRow {
            label: "Started/Parsed(Indeterminate(OkButNoFiscalNumber)) → SubmittedUnknown/Parsed/ProbeRequired",
            evidence: parsed(RawDpsStatus::Ok { fiscal_id: "" }, DocType::Sell),
            doc_type: DocType::Sell,
            expected: expected(SC::SubmittedUnknown, RP::ParsedDpsEnvelope, Some(ARC::ProbeRequired), NE::ProbeRequired),
        },
    ]
}

// ─── RP4B-2 — full graph comparison ─────────────────────────────────────────

#[test]
fn rp4b_2_classify_full_graph_matches_normative() {
    let graph = normative_graph();
    let mut failures: Vec<String> = Vec::new();

    for row in &graph {
        let got = classify(&row.evidence);
        let e = &row.expected;
        if got.certainty() != e.certainty
            || got.provenance() != e.provenance
            || got.routing() != e.routing
            || got.node_effect() != e.node_effect
        {
            failures.push(format!(
                "\n  FAIL [{}]\n    doc_type={:?}\n    expected=(certainty={:?}, provenance={:?}, routing={:?}, node_effect={:?})\n    got     =(certainty={:?}, provenance={:?}, routing={:?}, node_effect={:?})",
                row.label, row.doc_type,
                e.certainty, e.provenance, e.routing, e.node_effect,
                got.certainty(), got.provenance(), got.routing(), got.node_effect(),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "RP4B-2 graph-pin: the following evidence→(certainty,provenance,routing) \
         mappings diverge from the normative #4B §2 table:{}\n",
        failures.join("")
    );
}

// ─── RP4B-3 — distinct rows don't collapse ───────────────────────────────────

/// RP4B-3: `-4` (UnknownStatus), `-3` (SaveError), bare timeout, and `-1` (Verify)
/// all produce DISTINCT triples. An Accepted↔Rejected swap would be caught by the
/// full graph above; this test additionally asserts the {SubmittedUnknown vs Submitted}
/// axis doesn't collapse across parsed-reject codes.
#[test]
fn rp4b_3_parsed_rows_are_distinct_from_timeout() {
    // -4 → Parsed (SubmittedUnknown + ParsedDpsEnvelope)
    let minus4 = classify(&parsed(RawDpsStatus::Error(-4), DocType::Sell));
    // Bare timeout → NoResponse (SubmittedUnknown + NoResponse)
    let timeout = classify(&started(SendResponse::no_response(
        NoResponseCause::Timeout,
    )));
    // -1 Verify → Submitted + ParsedDpsEnvelope + TerminalReject
    let minus1 = classify(&parsed(RawDpsStatus::Error(-1), DocType::Sell));

    // -4 and timeout must differ in provenance
    assert_ne!(
        minus4.provenance(),
        timeout.provenance(),
        "RP4B-3: -4 (Parsed) and timeout (NoResponse) must differ in ResponseProvenance"
    );
    // Both -4 and timeout are SubmittedUnknown
    assert_eq!(minus4.certainty(), SC::SubmittedUnknown);
    assert_eq!(timeout.certainty(), SC::SubmittedUnknown);
    // -1 must be Submitted (not SubmittedUnknown)
    assert_eq!(minus1.certainty(), SC::Submitted);
    // Full triples are all distinct
    assert_ne!(minus4, timeout, "RP4B-3: -4 triple != timeout triple");
    assert_ne!(minus4, minus1, "RP4B-3: -4 triple != -1 triple");
    assert_ne!(timeout, minus1, "RP4B-3: timeout triple != -1 triple");
}

// ─── D3 totality: Parsed(Rejected) → Submitted, never SubmittedUnknown ───────

#[test]
fn d3_parsed_reject_yields_submitted_not_unknown() {
    // Every named DPS reject code is a definitive verdict → certainty MUST be Submitted (D3).
    // (-2 on a non-close doc → Rejected(Close); the close-doc split → CloseAmbiguous is
    // Indeterminate and covered separately.)
    for code in [-1, -5, -6, -7, -8, -9, -10, -11, -12, -13, -14, -16, -2] {
        let out = classify(&parsed(RawDpsStatus::Error(code), DocType::Sell));
        assert_eq!(
            out.certainty(),
            SC::Submitted,
            "D3: Parsed(Rejected(code {code})) must yield Submitted certainty"
        );
        assert_eq!(
            out.provenance(),
            RP::ParsedDpsEnvelope,
            "D3: Parsed(Rejected(code {code})) must yield ParsedDpsEnvelope provenance"
        );
    }
}

// ─── D3 totality: Parsed(Indeterminate) → SubmittedUnknown, never Submitted ──

#[test]
fn d3_parsed_indeterminate_yields_submitted_unknown() {
    // (raw status, doc_type) inputs whose parsed outcome is Indeterminate.
    // CloseAmbiguous only arises on a close/Z doc type (§12 R1).
    let cases: &[(RawDpsStatus, DocType)] = &[
        (RawDpsStatus::Error(-3), DocType::Sell),    // SaveError
        (RawDpsStatus::Error(-2), DocType::ZReport), // CloseAmbiguous (close doc)
        (RawDpsStatus::Ok { fiscal_id: "" }, DocType::Sell), // OkButNoFiscalNumber
        (RawDpsStatus::Error(-4), DocType::Sell),    // UnknownStatus
    ];
    for &(status, dt) in cases {
        let out = classify(&parsed(status, dt));
        assert_eq!(
            out.certainty(),
            SC::SubmittedUnknown,
            "D3: Parsed(Indeterminate) from {status:?} on {dt:?} must yield SubmittedUnknown"
        );
    }
}

// ─── SentAccepted::observe guards empty id ────────────────────────────────────

#[test]
fn sent_accepted_observe_rejects_empty_id() {
    assert!(
        SentAccepted::observe("").is_none(),
        "AL-3: SentAccepted::observe('') must return None"
    );
    assert!(
        SentAccepted::observe("DPS-123").is_some(),
        "AL-3: SentAccepted::observe('DPS-123') must return Some"
    );
}

// ─── HydratedRetryClass round-trips all 8 values ─────────────────────────────

#[test]
fn hydrated_retry_class_round_trips_all_8() {
    use prro_domain::delivery::HydratedRetryClass;

    let pairs = [
        (
            "TerminalReject",
            HydratedRetryClass::Active(ARC::TerminalReject),
        ),
        (
            "TransientRetry",
            HydratedRetryClass::Active(ARC::TransientRetry),
        ),
        (
            "FnConfigError",
            HydratedRetryClass::Active(ARC::FnConfigError),
        ),
        ("WrapperBug", HydratedRetryClass::Active(ARC::WrapperBug)),
        (
            "ProbeRequired",
            HydratedRetryClass::Active(ARC::ProbeRequired),
        ),
        ("MacRecovery", HydratedRetryClass::Active(ARC::MacRecovery)),
        (
            "OperatorEscalation",
            HydratedRetryClass::Active(ARC::OperatorEscalation),
        ),
        (
            "DrainChainSettleRetry",
            HydratedRetryClass::DrainChainSettleRetry,
        ),
    ];
    for (wire, expected) in pairs {
        assert_eq!(
            HydratedRetryClass::from_wire_str(wire),
            Some(expected),
            "HydratedRetryClass round-trip failed for '{wire}'"
        );
    }
    assert_eq!(HydratedRetryClass::from_wire_str("Bogus"), None);
}

// ─── ActiveRetryClass wire strings match error_routing.rs ─────────────────────

#[test]
fn active_retry_class_wire_strings_match_error_routing() {
    // These strings are byte-identical with RetryClass::as_str() in error_routing.rs.
    // A divergence here would break the 032 routing_class column contract.
    assert_eq!(ARC::TerminalReject.as_str(), "TerminalReject");
    assert_eq!(ARC::TransientRetry.as_str(), "TransientRetry");
    assert_eq!(ARC::FnConfigError.as_str(), "FnConfigError");
    assert_eq!(ARC::WrapperBug.as_str(), "WrapperBug");
    assert_eq!(ARC::ProbeRequired.as_str(), "ProbeRequired");
    assert_eq!(ARC::MacRecovery.as_str(), "MacRecovery");
    assert_eq!(ARC::OperatorEscalation.as_str(), "OperatorEscalation");
}

// ─── ObservedOutcomeV1 construction ──────────────────────────────────────────

#[test]
fn observed_outcome_v1_carries_all_fields() {
    use prro_domain::delivery::{classify, ObservedOutcomeV1};

    // Build a real classifier output (Offline168 → Submitted/Parsed/TerminalReject/NodeBlocked),
    // then mint the durable record from it. The axes + node_effect come from `classify`,
    // never hand-assembled (sealed mint).
    let classified = classify(&parsed(RawDpsStatus::Error(-11), DocType::Sell));
    let outcome = ObservedOutcomeV1::record(
        &classified,
        Some(BoundedText::from_truncating("DPS-123")),
        AuthorizedGeneration::Started(PositiveGeneration::new(42).unwrap()),
    )
    .expect("Submitted + Started(42) is a valid pairing");

    assert_eq!(outcome.certainty(), SC::Submitted);
    assert_eq!(outcome.authorized_generation().as_db_value(), Some(42));
    assert_eq!(outcome.node_effect(), NE::NodeBlocked);
    assert_eq!(
        outcome.remote_correlation_id().map(|t| t.as_str()),
        Some("DPS-123")
    );
}

// ─── NodeEffect — -11 and -1 differ despite sharing routing ──────────────────

#[test]
fn node_effect_discriminates_offline168_from_verify() {
    // Via the PUBLIC classifier (routing_for_reject is now a private helper): -11 Offline168
    // and -1 Verify share TerminalReject routing but differ in node_effect.
    let offline168 = classify(&parsed(RawDpsStatus::Error(-11), DocType::Sell));
    let verify = classify(&parsed(RawDpsStatus::Error(-1), DocType::Sell));

    // Both are TerminalReject routing...
    assert_eq!(offline168.routing(), Some(ARC::TerminalReject));
    assert_eq!(verify.routing(), Some(ARC::TerminalReject));
    // ...but the node effects DIFFER (this is why the discriminant is needed).
    assert_eq!(offline168.node_effect(), NE::NodeBlocked);
    assert_eq!(verify.node_effect(), NE::NoNodeEffect);
    assert_ne!(
        offline168.node_effect(),
        verify.node_effect(),
        "NodeEffect must discriminate -11 from -1"
    );
}
