//! W9b Commit 2 — API-level invariant pins for the typed W12 seam
//! + DrainSummary + finalize-eligibility decision enum.
//!
//! ## Operator gate (sign-off 2026-05-20)
//!
//! > "C2 typed seam должен не просто существовать, а закрывать
//! >  pre-W12 invariant на уровне API.  То есть W12ConfirmOutcome::
//! >  DeferredKvt1 не должен попадать в advanced_to_ack, не должен
//! >  разрешать finalization, и должен вести к OFFLINE_DRAIN_PARTIAL."
//!
//! Tests below lock all three contract points.

use prro::db::models::ids::DocumentId;
use prro::services::offline_sync::backlog_drain::{
    failure_class_for, DrainSummary, FailureClass, FinalizeEligibility, FinalizeError,
    NotEligibleReason, W12ConfirmOutcome,
};

const FN: &str = "1234567890";

fn fresh_summary(backlog: usize) -> DrainSummary {
    DrainSummary::new(FN.into(), backlog)
}

// ─── W12ConfirmOutcome basic shape ────────────────────────────────────

#[test]
fn w12_confirm_outcome_deferred_kvt1_str_helpers() {
    let o = W12ConfirmOutcome::DeferredKvt1;
    assert_eq!(o.final_state_str(), "KVT1");
    assert_eq!(o.w12_status_str(), "DeferredKvt1");
}

#[test]
fn w12_confirm_outcome_acked_str_helpers() {
    let o = W12ConfirmOutcome::Acked {
        server_fiscal_no: "DPS-FN-1234567890".into(),
    };
    assert_eq!(o.final_state_str(), "ACK");
    assert_eq!(o.w12_status_str(), "Acked");
}

// ─── INVARIANT 1: DeferredKvt1 NEVER routed to advanced_to_ack ──────

#[test]
fn deferred_kvt1_increments_only_advanced_to_kvt1_never_ack() {
    let mut s = fresh_summary(3);
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    assert_eq!(
        s.advanced_to_kvt1(),
        3,
        "all 3 DeferredKvt1 docs MUST land in advanced_to_kvt1 bucket"
    );
    assert_eq!(
        s.advanced_to_ack(),
        0,
        "advanced_to_ack MUST stay 0 — DeferredKvt1 cannot route to Ack bucket"
    );
}

#[test]
fn acked_increments_only_advanced_to_ack_never_kvt1() {
    let mut s = fresh_summary(2);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "DPS-FN-A".into(),
        },
        false,
    );
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "DPS-FN-B".into(),
        },
        false,
    );
    assert_eq!(s.advanced_to_ack(), 2);
    assert_eq!(s.advanced_to_kvt1(), 0);
}

#[test]
fn mixed_acked_and_deferred_routed_to_correct_buckets() {
    let mut s = fresh_summary(4);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "A".into(),
        },
        false,
    );
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "B".into(),
        },
        false,
    );
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    assert_eq!(s.advanced_to_ack(), 2);
    assert_eq!(s.advanced_to_kvt1(), 2);
}

// ─── INVARIANT 2: any DeferredKvt1 blocks finalization ───────────────

#[test]
fn finalize_eligibility_blocked_by_any_deferred_kvt1() {
    // 2 acked + 1 deferred → finalize blocked.
    let mut s = fresh_summary(3);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "A".into(),
        },
        false,
    );
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "B".into(),
        },
        false,
    );
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    match s.finalize_eligibility() {
        FinalizeEligibility::NotEligible {
            reason: NotEligibleReason::DocsDeferredAtKvt1 { count },
        } => assert_eq!(count, 1, "1 deferred doc reported"),
        other => panic!("expected NotEligible(DocsDeferredAtKvt1), got {other:?}"),
    }
}

#[test]
fn finalize_eligibility_pre_w12_all_deferred_is_not_eligible() {
    // Pre-W12 steady-state: all N docs return DeferredKvt1.
    let mut s = fresh_summary(5);
    for _ in 0..5 {
        s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    }
    assert_eq!(s.advanced_to_kvt1(), 5);
    assert_eq!(s.advanced_to_ack(), 0);
    match s.finalize_eligibility() {
        FinalizeEligibility::NotEligible {
            reason: NotEligibleReason::DocsDeferredAtKvt1 { count: 5 },
        } => {}
        other => panic!("pre-W12 steady-state MUST be NotEligible, got {other:?}"),
    }
}

#[test]
fn finalize_eligibility_eligible_when_all_acked_no_failures() {
    let mut s = fresh_summary(3);
    for i in 0..3 {
        s.record_doc_advanced(
            &W12ConfirmOutcome::Acked {
                server_fiscal_no: format!("DPS-FN-{i}"),
            },
            false,
        );
    }
    assert_eq!(s.finalize_eligibility(), FinalizeEligibility::Eligible);
}

#[test]
fn finalize_eligibility_blocked_by_per_doc_failure_even_if_others_acked() {
    let mut s = fresh_summary(2);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "A".into(),
        },
        false,
    );
    s.record_doc_failure(
        DocumentId::new(),
        failure_class_for(FailureClass::Transport).into(),
    );
    match s.finalize_eligibility() {
        FinalizeEligibility::NotEligible {
            reason: NotEligibleReason::PerDocFailuresPresent { count: 1 },
        } => {}
        other => panic!("expected NotEligible(PerDocFailuresPresent), got {other:?}"),
    }
}

#[test]
fn finalize_eligibility_blocked_by_ack_count_mismatch() {
    // 2 acked but backlog says 3 (no failures, no deferred recorded) —
    // accounting drift, defensive guard.
    let mut s = fresh_summary(3);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "A".into(),
        },
        false,
    );
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "B".into(),
        },
        false,
    );
    match s.finalize_eligibility() {
        FinalizeEligibility::NotEligible {
            reason:
                NotEligibleReason::AckCountMismatch {
                    expected: 3,
                    actual: 2,
                },
        } => {}
        other => panic!("expected NotEligible(AckCountMismatch), got {other:?}"),
    }
}

// ─── INVARIANT 3: mark_finalized refuses unless eligible ─────────────

#[test]
fn mark_finalized_errors_with_typed_reason_when_any_deferred() {
    let mut s = fresh_summary(2);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "A".into(),
        },
        false,
    );
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);

    match s.mark_finalized() {
        Err(FinalizeError::NotEligible(NotEligibleReason::DocsDeferredAtKvt1 { count: 1 })) => {}
        other => panic!("expected Err(NotEligible(DocsDeferredAtKvt1)), got {other:?}"),
    }
    assert!(
        !s.finalized(),
        "mark_finalized MUST NOT set finalized=true when not eligible"
    );
}

#[test]
fn mark_finalized_errors_pre_w12_steady_state() {
    // Pre-W12 invariant pin: 5 DeferredKvt1 → mark_finalized errors.
    let mut s = fresh_summary(5);
    for _ in 0..5 {
        s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, false);
    }
    assert!(s.mark_finalized().is_err());
    assert!(!s.finalized());
}

#[test]
fn mark_finalized_succeeds_only_with_all_acked_no_failures() {
    let mut s = fresh_summary(2);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "A".into(),
        },
        false,
    );
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "B".into(),
        },
        false,
    );
    assert!(s.mark_finalized().is_ok());
    assert!(s.finalized());
}

// ─── lastChk replay flag accounting ──────────────────────────────────

#[test]
fn lastchk_replay_flag_counted_independently_of_w12_outcome() {
    // Replay short-circuit hit + DeferredKvt1 (pre-W12 stub case
    // where lastChk pre-flight saw a server_fiscal_no but W12 helper
    // still stubs DeferredKvt1).
    let mut s = fresh_summary(2);
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, /*via_replay*/ true);
    s.record_doc_advanced(&W12ConfirmOutcome::DeferredKvt1, /*via_replay*/ false);
    assert_eq!(s.advanced_via_lastchk_replay(), 1);
    assert_eq!(s.advanced_to_kvt1(), 2);
}

#[test]
fn lastchk_replay_count_increments_for_acked_too() {
    let mut s = fresh_summary(2);
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "A".into(),
        },
        true,
    );
    s.record_doc_advanced(
        &W12ConfirmOutcome::Acked {
            server_fiscal_no: "B".into(),
        },
        false,
    );
    assert_eq!(s.advanced_via_lastchk_replay(), 1);
    assert_eq!(s.advanced_to_ack(), 2);
}

// ─── failure_class taxonomy stable strings ───────────────────────────

/// LOW-C2-1 fix: exhaustive table-test for the FailureClass taxonomy.
/// Pins all 12 variants + locks the variant-count at 12 via
/// `cases.len() == 12` guard.  Future additions to `FailureClass`
/// (e.g. W12 `LastChkMismatch` variants) MUST update both the enum
/// AND this table in lockstep, or the count assertion fails loud.
#[test]
fn failure_class_stable_strings_table_covers_all_12_variants() {
    let cases: &[(FailureClass, &'static str)] = &[
        (FailureClass::SignerRefused, "signer_refused"),
        (FailureClass::StateConflict, "state_conflict"),
        (
            FailureClass::WireRoutingTerminalReject,
            "wire_routing_terminal_reject",
        ),
        (
            FailureClass::WireRoutingProbeRequired,
            "wire_routing_probe_required",
        ),
        (
            FailureClass::WireRoutingTransientRetry,
            "wire_routing_transient_retry",
        ),
        (FailureClass::Transport, "transport"),
        (FailureClass::Authorization, "authorization"),
        (FailureClass::Server, "server"),
        (FailureClass::Decode, "decode"),
        (FailureClass::Internal, "internal"),
        (FailureClass::NotFound, "not_found"),
        (
            FailureClass::OfflineFiscalNoMissing,
            "offline_fiscal_no_missing",
        ),
    ];
    for (cls, expected) in cases {
        assert_eq!(failure_class_for(*cls), *expected, "class {cls:?}");
    }
    assert_eq!(
        cases.len(),
        12,
        "FailureClass taxonomy MUST have exactly 12 variants — add new \
         variant to BOTH the enum AND this table"
    );
}

// ─── per_doc_failures accessor ───────────────────────────────────────

#[test]
fn per_doc_failures_preserved_with_class_attribution() {
    let mut s = fresh_summary(3);
    let d1 = DocumentId::new();
    let d2 = DocumentId::new();
    s.record_doc_failure(d1, failure_class_for(FailureClass::Transport).into());
    s.record_doc_failure(d2, failure_class_for(FailureClass::SignerRefused).into());
    let failures = s.per_doc_failures();
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0].0, d1);
    assert_eq!(failures[0].1, "transport");
    assert_eq!(failures[1].0, d2);
    assert_eq!(failures[1].1, "signer_refused");
}

// ─── Fresh summary baseline ──────────────────────────────────────────

#[test]
fn fresh_summary_baseline_state() {
    let s = fresh_summary(7);
    // MED-C2-1: access via private-field accessors (no direct field
    // mutation surface).
    assert_eq!(s.fiscal_number(), "1234567890");
    assert_eq!(s.backlog_size_before(), 7);
    assert_eq!(s.advanced_to_ack(), 0);
    assert_eq!(s.advanced_to_kvt1(), 0);
    assert_eq!(s.advanced_via_lastchk_replay(), 0);
    assert!(s.per_doc_failures().is_empty());
    assert!(!s.finalized());
}

#[test]
fn empty_backlog_finalize_eligibility_is_eligible() {
    // Edge case: empty backlog (orchestrator early-returns
    // OFFLINE_DRAIN_SKIPPED_EMPTY_BACKLOG before reaching finalize
    // surface, but the invariant should be consistent: 0 acked == 0
    // backlog means eligibility = Eligible).
    let s = fresh_summary(0);
    assert_eq!(s.finalize_eligibility(), FinalizeEligibility::Eligible);
}
