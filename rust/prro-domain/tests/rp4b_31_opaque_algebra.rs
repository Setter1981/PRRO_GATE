//! CS-3 Bridge-0.1 (3.1) — opaque outcome algebra: the raw_code × doc_type MATRIX.
//!
//! The checkpoint audit found the outcome algebra could express illegal states
//! (`UnknownStatus("-1")`, `CloseAmbiguous` on a `Sell` doc). The fix seals the algebra
//! behind the SOLE constructor [`SendOutcome::from_dps_status`]. Per Bridge-0.1
//! correction 2, the *bypass-impossibility* is proven by trybuild; THIS test proves the
//! *matrix correctness* at runtime: every raw status × doc_type maps to the one honest
//! outcome — named codes to their verdict, `-2/-15` split strictly by doc type, and
//! `UnknownStatus` ONLY for codes with no dedicated verdict.

use prro_domain::delivery::{
    DpsReject, RawDpsStatus, RawResponseDigest, SendIndeterminateKind, SendOutcome, SendOutcomeKind,
};
use prro_domain::enums::DocType;

fn dg() -> RawResponseDigest {
    RawResponseDigest([0u8; 32])
}

/// Every named DPS code maps to its dedicated `DpsReject` verdict — on ANY doc type
/// (the named verdicts are doc-type-independent; only `-2/-15` split, tested separately).
#[test]
fn from_dps_status_maps_named_codes_to_verdicts() {
    let named: &[(i32, DpsReject)] = &[
        (-1, DpsReject::Verify),
        (-5, DpsReject::Type),
        (-6, DpsReject::NotPrevZReport),
        (-7, DpsReject::Xml),
        (-8, DpsReject::XmlDate),
        (-9, DpsReject::XmlChk),
        (-10, DpsReject::XmlZReport),
        (-11, DpsReject::Offline168),
        (-12, DpsReject::BadHashPrev),
        (-13, DpsReject::NotRegisteredRro),
        (-14, DpsReject::NotRegisteredSigner),
        (-16, DpsReject::OfflineId),
    ];
    for &(code, want) in named {
        for dt in [DocType::Sell, DocType::ZReport, DocType::ShiftClose] {
            match SendOutcome::from_dps_status(RawDpsStatus::Error(code), dt, dg()).kind() {
                SendOutcomeKind::Rejected(got) => assert_eq!(
                    got, want,
                    "code {code} on {dt:?} must map to {want:?}, got {got:?}"
                ),
                other => panic!("code {code} on {dt:?} must be Rejected({want:?}), got {other:?}"),
            }
        }
    }
}

/// `-2/-15` split STRICTLY by doc type: non-close ⇒ `Rejected(Close)`; close/Z ⇒
/// `Indeterminate(CloseAmbiguous)`. This is the illegal cross-product the checkpoint hit.
#[test]
fn from_dps_status_splits_minus2_minus15_by_doc_type() {
    for code in [-2, -15] {
        // Non-close doc types → Rejected(Close).
        for dt in [
            DocType::Sell,
            DocType::Return,
            DocType::ServiceIn,
            DocType::ShiftOpen,
        ] {
            match SendOutcome::from_dps_status(RawDpsStatus::Error(code), dt, dg()).kind() {
                SendOutcomeKind::Rejected(DpsReject::Close) => {}
                other => {
                    panic!("{code} on non-close {dt:?} must be Rejected(Close), got {other:?}")
                }
            }
        }
        // Close/Z doc types → Indeterminate(CloseAmbiguous).
        for dt in [DocType::ShiftClose, DocType::ZReport] {
            match SendOutcome::from_dps_status(RawDpsStatus::Error(code), dt, dg()).kind() {
                SendOutcomeKind::Indeterminate(ind) => match ind.kind() {
                    SendIndeterminateKind::CloseAmbiguous => {}
                    other => panic!("{code} on close {dt:?} must be CloseAmbiguous, got {other:?}"),
                },
                other => panic!("{code} on close {dt:?} must be Indeterminate, got {other:?}"),
            }
        }
    }
}

/// `-3` → `SaveError`; `-4` and every unrecognized code → `UnknownStatus` carrying the
/// raw code. A NAMED code is NEVER `UnknownStatus` (that's the checkpoint's `-1` bug).
#[test]
fn from_dps_status_unknown_only_for_non_named_codes() {
    // -3 is its own SaveError, not UnknownStatus.
    match SendOutcome::from_dps_status(RawDpsStatus::Error(-3), DocType::Sell, dg()).kind() {
        SendOutcomeKind::Indeterminate(ind) => {
            assert!(matches!(ind.kind(), SendIndeterminateKind::SaveError));
        }
        other => panic!("-3 must be Indeterminate(SaveError), got {other:?}"),
    }
    // -4 (canonical ERROR_UNKNOWN) + truly-unknown codes → UnknownStatus with the raw code.
    for code in [-4, -99, -1234, -50] {
        match SendOutcome::from_dps_status(RawDpsStatus::Error(code), DocType::Sell, dg()).kind() {
            SendOutcomeKind::Indeterminate(ind) => match ind.kind() {
                SendIndeterminateKind::UnknownStatus { code: got, .. } => assert_eq!(got, code),
                other => panic!("{code} must be UnknownStatus, got {other:?}"),
            },
            other => panic!("{code} must be Indeterminate(UnknownStatus), got {other:?}"),
        }
    }
}

/// OK + non-empty id → `Accepted`; OK + empty id → `OkButNoFiscalNumber` (AL-3).
#[test]
fn from_dps_status_ok_splits_accepted_vs_no_fiscal_number() {
    match SendOutcome::from_dps_status(RawDpsStatus::Ok { fiscal_id: "DPS-1" }, DocType::Sell, dg())
        .kind()
    {
        SendOutcomeKind::Accepted(a) => assert_eq!(a.fiscal_number(), "DPS-1"),
        other => panic!("OK+id must be Accepted, got {other:?}"),
    }
    match SendOutcome::from_dps_status(RawDpsStatus::Ok { fiscal_id: "" }, DocType::Sell, dg())
        .kind()
    {
        SendOutcomeKind::Indeterminate(ind) => {
            assert!(matches!(
                ind.kind(),
                SendIndeterminateKind::OkButNoFiscalNumber { .. }
            ));
        }
        other => panic!("OK+empty must be Indeterminate(OkButNoFiscalNumber), got {other:?}"),
    }
}
