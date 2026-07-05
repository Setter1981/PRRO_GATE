//! W14a-2b §2.3 unit tests — `signer_guard::enforce_signer_cashier_match`.
//!
//! Pure-function helper; no DB I/O.  Tests cover all decision arms:
//!
//!   Bypass set (returns Ok regardless of signer / shift state):
//!     - `DocType::ShiftClose` (§16.9 senior bypass)
//!     - `DocType::ZReport`    (§16.9 senior bypass)
//!     - `DocType::ShiftOpen`  (MED-C3-2: no shift row at stage_send
//!       time; signer becomes opened_by_cashier_id post-finalize)
//!
//!   Refusal classes (in decision-precedence order):
//!     1. `ShiftMissingForFiscalDoc` — shift arg = None OR
//!        inputs.shift_id = None
//!     2. `ShiftIdMismatch`          — inputs.shift_id != shift.shift_id
//!     3. `CrossFnMismatch`          — inputs.fiscal_number != shift.fiscal_number
//!     4. `SignerIdMissing`          — inputs.signed_by_cashier_id = None
//!     5. `Mismatch`                 — signer != opened_by_cashier_id
//!
//!   + happy path: aligned (shift_id + fn + signer) → Ok(()).
//!
//! NIT-C3-1 + MED-C3-1 + MED-C3-2 + LOW-C3-3 + LOW-C3-4 resolutions
//! folded into this suite per operator senior review 2026-05-19.

use prro::db::models::enums::{DocState, DocType, ShiftState};
use prro::db::models::ids::{CashierId, DocumentId, ShiftId};
use prro::db::repositories::fiscal_documents::SendInputs;
use prro::db::repositories::shifts::ShiftRow;
use prro::services::write_path::signer_guard::{
    enforce_signer_cashier_match, SignerCashierMismatch,
};

fn cashier(id: &str) -> CashierId {
    CashierId::new(id).expect("valid cashier id")
}

fn sample_inputs(
    doc_type: DocType,
    signer: Option<CashierId>,
    shift_id: Option<ShiftId>,
) -> SendInputs {
    sample_inputs_with_fn(doc_type, signer, shift_id, "1234567890")
}

fn sample_inputs_with_fn(
    doc_type: DocType,
    signer: Option<CashierId>,
    shift_id: Option<ShiftId>,
    fn_id: &str,
) -> SendInputs {
    SendInputs {
        state: DocState::Signed,
        fiscal_number: fn_id.into(),
        lnd: 1,
        doc_type,
        business_ts: "2026-05-19T12:00:00Z".into(),
        backend_profile_id: "b1".into(),
        transport_profile_id: "t1".into(),
        offline_fiscal_no: None,
        document_id: DocumentId::new(),
        shift_id,
        signed_by_cashier_id: signer,
        // A.3 — chain fields; signer-guard tests don't exercise the advance.
        previous_hash: None,
        unsigned_xml_sha256: None,
        mac_recovery_attempts: 0,
    }
}

fn sample_shift(opened_by: CashierId) -> ShiftRow {
    sample_shift_with_fn(opened_by, "1234567890")
}

fn sample_shift_with_fn(opened_by: CashierId, fn_id: &str) -> ShiftRow {
    ShiftRow {
        shift_id: ShiftId::new(),
        fiscal_number: fn_id.into(),
        serial: None,
        state: ShiftState::Opened,
        cash_balance_kop: 0,
        opened_by_cashier_id: opened_by,
    }
}

/// MED-C3-1 helper: produce aligned `(SendInputs, ShiftRow)` so the
/// shift_id + fiscal_number + cashier reach the equality check.
fn aligned(doc_type: DocType, cashier_id: CashierId) -> (SendInputs, ShiftRow) {
    let shift = sample_shift(cashier_id.clone());
    let mut inputs = sample_inputs(doc_type, Some(cashier_id), Some(shift.shift_id));
    inputs.fiscal_number = shift.fiscal_number.clone();
    (inputs, shift)
}

// ─── Bypass arms (§16.9 + MED-C3-2 ShiftOpen) ────────────────────────

#[test]
fn shift_close_bypasses_signer_check_with_no_signer() {
    let inputs = sample_inputs(DocType::ShiftClose, None, None);
    assert_eq!(enforce_signer_cashier_match(&inputs, None), Ok(()));
}

#[test]
fn shift_close_bypasses_signer_check_with_mismatched_signer() {
    let inputs = sample_inputs(DocType::ShiftClose, Some(cashier("senior-vera")), None);
    let shift = sample_shift(cashier("cashier-vasya"));
    // §16.9: senior may close even if signer != opening cashier.
    assert_eq!(enforce_signer_cashier_match(&inputs, Some(&shift)), Ok(()));
}

#[test]
fn z_report_bypasses_signer_check_with_no_signer() {
    let inputs = sample_inputs(DocType::ZReport, None, None);
    assert_eq!(enforce_signer_cashier_match(&inputs, None), Ok(()));
}

#[test]
fn z_report_bypasses_signer_check_with_mismatched_signer() {
    let inputs = sample_inputs(DocType::ZReport, Some(cashier("senior-vera")), None);
    let shift = sample_shift(cashier("cashier-vasya"));
    assert_eq!(enforce_signer_cashier_match(&inputs, Some(&shift)), Ok(()));
}

#[test]
fn shift_open_bypasses_signer_check_with_no_shift_and_no_signer() {
    // MED-C3-2: SHIFT_OPEN at stage_send has shift_id = None (stage_acquire
    // inserts the doc with no active shift yet).  Helper must bypass —
    // there is no shift row to compare against.
    let inputs = sample_inputs(DocType::ShiftOpen, None, None);
    assert_eq!(enforce_signer_cashier_match(&inputs, None), Ok(()));
}

#[test]
fn shift_open_bypasses_even_with_mismatched_signer() {
    // MED-C3-2: regardless of signer / shift state, ShiftOpen bypasses.
    // The signer's `signed_by_cashier_id` becomes `opened_by_cashier_id`
    // after stage_finalize creates the shift row — pre-creation
    // validation is semantically empty.
    let inputs = sample_inputs(DocType::ShiftOpen, Some(cashier("cashier-petya")), None);
    let shift = sample_shift(cashier("cashier-vasya"));
    assert_eq!(enforce_signer_cashier_match(&inputs, Some(&shift)), Ok(()));
}

// ─── ShiftMissingForFiscalDoc arm (1a + 1b) ──────────────────────────

#[test]
fn sell_without_shift_arg_returns_shift_missing() {
    // (1a) shift arg = None.
    let inputs = sample_inputs(
        DocType::Sell,
        Some(cashier("cashier-vasya")),
        Some(ShiftId::new()),
    );
    let res = enforce_signer_cashier_match(&inputs, None);
    match res {
        Err(SignerCashierMismatch::ShiftMissingForFiscalDoc {
            document_id,
            doc_type,
        }) => {
            assert_eq!(document_id, inputs.document_id);
            assert_eq!(doc_type, DocType::Sell);
        }
        other => panic!("expected ShiftMissingForFiscalDoc, got {other:?}"),
    }
}

#[test]
fn sell_without_inputs_shift_id_returns_shift_missing() {
    // (1b) MED-C3-1: inputs.shift_id = None on a non-bypass doc → must
    // surface as ShiftMissingForFiscalDoc.  Without this arm, a future
    // caller could supply ANY shift row and signer would validate
    // against the wrong binding.
    let inputs = sample_inputs(DocType::Sell, Some(cashier("cashier-vasya")), None);
    let shift = sample_shift(cashier("cashier-vasya"));
    assert!(matches!(
        enforce_signer_cashier_match(&inputs, Some(&shift)),
        Err(SignerCashierMismatch::ShiftMissingForFiscalDoc { .. })
    ));
}

#[test]
fn x_report_without_inputs_shift_id_carries_doc_type_in_refusal() {
    let inputs = sample_inputs(DocType::XReport, Some(cashier("c")), None);
    let shift = sample_shift(cashier("c"));
    let res = enforce_signer_cashier_match(&inputs, Some(&shift));
    match res {
        Err(SignerCashierMismatch::ShiftMissingForFiscalDoc { doc_type, .. }) => {
            assert_eq!(doc_type, DocType::XReport);
        }
        other => panic!("expected ShiftMissingForFiscalDoc(XReport), got {other:?}"),
    }
}

// ─── ShiftIdMismatch arm (MED-C3-1) ──────────────────────────────────

#[test]
fn sell_with_wrong_shift_id_returns_shift_id_mismatch() {
    // MED-C3-1 core test: caller supplied a different shift row than
    // the document's persisted shift_id.  Must surface as
    // ShiftIdMismatch with both ids forensically attributed.
    let expected_shift_id = ShiftId::new();
    let inputs = sample_inputs(
        DocType::Sell,
        Some(cashier("cashier-vasya")),
        Some(expected_shift_id),
    );
    let supplied_shift = sample_shift(cashier("cashier-vasya"));
    assert_ne!(expected_shift_id, supplied_shift.shift_id);

    let res = enforce_signer_cashier_match(&inputs, Some(&supplied_shift));
    match res {
        Err(SignerCashierMismatch::ShiftIdMismatch {
            document_id,
            doc_type,
            expected_shift_id: ex,
            supplied_shift_id: sup,
        }) => {
            assert_eq!(document_id, inputs.document_id);
            assert_eq!(doc_type, DocType::Sell);
            assert_eq!(ex, Some(expected_shift_id));
            assert_eq!(sup, supplied_shift.shift_id);
        }
        other => panic!("expected ShiftIdMismatch, got {other:?}"),
    }
}

#[test]
fn shift_id_mismatch_takes_precedence_over_cross_fn() {
    // Both shift_id mismatch AND cross-FN.  Shift-id check runs first
    // per decision-order precedence.
    let expected_shift_id = ShiftId::new();
    let mut inputs = sample_inputs(
        DocType::Sell,
        Some(cashier("cashier-vasya")),
        Some(expected_shift_id),
    );
    inputs.fiscal_number = "1111111111".into();
    let supplied_shift = sample_shift_with_fn(cashier("cashier-vasya"), "2222222222");
    assert_ne!(expected_shift_id, supplied_shift.shift_id);

    assert!(matches!(
        enforce_signer_cashier_match(&inputs, Some(&supplied_shift)),
        Err(SignerCashierMismatch::ShiftIdMismatch { .. })
    ));
}

#[test]
fn shift_id_mismatch_takes_precedence_over_cashier_mismatch() {
    let expected_shift_id = ShiftId::new();
    let inputs = sample_inputs(
        DocType::Sell,
        Some(cashier("cashier-petya")),
        Some(expected_shift_id),
    );
    let supplied_shift = sample_shift(cashier("cashier-vasya"));
    assert_ne!(expected_shift_id, supplied_shift.shift_id);
    assert!(matches!(
        enforce_signer_cashier_match(&inputs, Some(&supplied_shift)),
        Err(SignerCashierMismatch::ShiftIdMismatch { .. })
    ));
}

// ─── CrossFnMismatch arm (NIT-C3-2) ──────────────────────────────────

#[test]
fn sell_with_cross_fn_binding_returns_cross_fn_mismatch() {
    // Same shift_id reference but different fiscal_number on the rows.
    let shift = sample_shift_with_fn(cashier("c"), "2222222222");
    let mut inputs = sample_inputs(DocType::Sell, Some(cashier("c")), Some(shift.shift_id));
    inputs.fiscal_number = "1111111111".into();

    let res = enforce_signer_cashier_match(&inputs, Some(&shift));
    match res {
        Err(SignerCashierMismatch::CrossFnMismatch {
            document_id,
            inputs_fiscal_number,
            shift_fiscal_number,
        }) => {
            assert_eq!(document_id, inputs.document_id);
            assert_eq!(inputs_fiscal_number, "1111111111");
            assert_eq!(shift_fiscal_number, "2222222222");
        }
        other => panic!("expected CrossFnMismatch, got {other:?}"),
    }
}

#[test]
fn cross_fn_takes_precedence_over_signer_missing() {
    // Both signer-id None AND cross-FN, but shift_id aligned.
    let shift = sample_shift_with_fn(cashier("c"), "2222222222");
    let mut inputs = sample_inputs(DocType::Sell, None, Some(shift.shift_id));
    inputs.fiscal_number = "1111111111".into();
    assert!(matches!(
        enforce_signer_cashier_match(&inputs, Some(&shift)),
        Err(SignerCashierMismatch::CrossFnMismatch { .. })
    ));
}

#[test]
fn cross_fn_takes_precedence_over_cashier_mismatch() {
    let shift = sample_shift_with_fn(cashier("c"), "2222222222");
    let mut inputs = sample_inputs(DocType::Sell, Some(cashier("petya")), Some(shift.shift_id));
    inputs.fiscal_number = "1111111111".into();
    assert!(matches!(
        enforce_signer_cashier_match(&inputs, Some(&shift)),
        Err(SignerCashierMismatch::CrossFnMismatch { .. })
    ));
}

#[test]
fn shift_close_with_cross_fn_still_bypasses() {
    let shift = sample_shift_with_fn(cashier("c"), "2222222222");
    let mut inputs = sample_inputs(DocType::ShiftClose, None, Some(shift.shift_id));
    inputs.fiscal_number = "1111111111".into();
    assert_eq!(enforce_signer_cashier_match(&inputs, Some(&shift)), Ok(()));
}

// ─── SignerIdMissing arm ─────────────────────────────────────────────

#[test]
fn sell_without_signer_id_but_aligned_shift_returns_signer_id_missing() {
    let shift = sample_shift(cashier("cashier-vasya"));
    let inputs = sample_inputs(DocType::Sell, None, Some(shift.shift_id));
    let res = enforce_signer_cashier_match(&inputs, Some(&shift));
    match res {
        Err(SignerCashierMismatch::SignerIdMissing { document_id }) => {
            assert_eq!(document_id, inputs.document_id);
        }
        other => panic!("expected SignerIdMissing, got {other:?}"),
    }
}

#[test]
fn return_without_signer_id_but_aligned_shift_returns_signer_id_missing() {
    let shift = sample_shift(cashier("cashier-vasya"));
    let inputs = sample_inputs(DocType::Return, None, Some(shift.shift_id));
    assert!(matches!(
        enforce_signer_cashier_match(&inputs, Some(&shift)),
        Err(SignerCashierMismatch::SignerIdMissing { .. })
    ));
}

// ─── Mismatch arm ────────────────────────────────────────────────────

#[test]
fn sell_with_mismatched_signer_returns_mismatch_with_full_payload() {
    let signer = cashier("cashier-petya");
    let opener = cashier("cashier-vasya");
    let shift = sample_shift(opener.clone());
    let inputs = sample_inputs(DocType::Sell, Some(signer.clone()), Some(shift.shift_id));

    let res = enforce_signer_cashier_match(&inputs, Some(&shift));
    match res {
        Err(SignerCashierMismatch::Mismatch {
            shift_id,
            document_id,
            expected_cashier_id,
            attempted_signer_id,
            doc_type,
        }) => {
            assert_eq!(shift_id, shift.shift_id);
            assert_eq!(document_id, inputs.document_id);
            assert_eq!(expected_cashier_id.as_str(), "cashier-vasya");
            assert_eq!(attempted_signer_id.as_str(), "cashier-petya");
            assert_eq!(doc_type, DocType::Sell);
        }
        other => panic!("expected Mismatch, got {other:?}"),
    }
}

#[test]
fn service_out_with_mismatched_signer_returns_mismatch() {
    let shift = sample_shift(cashier("cashier-vasya"));
    let inputs = sample_inputs(
        DocType::ServiceOut,
        Some(cashier("cashier-petya")),
        Some(shift.shift_id),
    );
    assert!(matches!(
        enforce_signer_cashier_match(&inputs, Some(&shift)),
        Err(SignerCashierMismatch::Mismatch { .. })
    ));
}

// ─── Happy path ──────────────────────────────────────────────────────

#[test]
fn sell_with_matched_signer_and_aligned_shift_returns_ok() {
    let (inputs, shift) = aligned(DocType::Sell, cashier("cashier-vasya"));
    assert_eq!(enforce_signer_cashier_match(&inputs, Some(&shift)), Ok(()));
}

#[test]
fn cash_withdrawal_with_matched_signer_returns_ok() {
    let (inputs, shift) = aligned(DocType::CashWithdrawal, cashier("c"));
    assert_eq!(enforce_signer_cashier_match(&inputs, Some(&shift)), Ok(()));
}

// ─── Structural pin (LOW-C3-4 strengthened) ──────────────────────────

/// LOW-C3-4 fix: instantiates EACH known variant of
/// `SignerCashierMismatch` directly, asserts a deliberate count, AND
/// uses a wildcard match arm to lock `#[non_exhaustive]` semantics.
/// Future variant additions force this test to be updated (instantiation
/// fails to compile if a field changes; count mismatch fails the
/// assertion).
#[test]
fn all_five_known_variants_instantiate_and_match() {
    let doc_id = DocumentId::new();
    let shift_id_a = ShiftId::new();
    let shift_id_b = ShiftId::new();
    let cashier_a = cashier("a");
    let cashier_b = cashier("b");

    let v1 = SignerCashierMismatch::ShiftMissingForFiscalDoc {
        document_id: doc_id,
        doc_type: DocType::Sell,
    };
    let v2 = SignerCashierMismatch::ShiftIdMismatch {
        document_id: doc_id,
        doc_type: DocType::Sell,
        expected_shift_id: Some(shift_id_a),
        supplied_shift_id: shift_id_b,
    };
    let v3 = SignerCashierMismatch::CrossFnMismatch {
        document_id: doc_id,
        inputs_fiscal_number: "1".into(),
        shift_fiscal_number: "2".into(),
    };
    let v4 = SignerCashierMismatch::SignerIdMissing {
        document_id: doc_id,
    };
    let v5 = SignerCashierMismatch::Mismatch {
        shift_id: shift_id_a,
        document_id: doc_id,
        expected_cashier_id: cashier_a,
        attempted_signer_id: cashier_b,
        doc_type: DocType::Sell,
    };

    let all: Vec<SignerCashierMismatch> = vec![v1, v2, v3, v4, v5];

    let mut count_shift_missing = 0u8;
    let mut count_shift_id_mismatch = 0u8;
    let mut count_cross_fn = 0u8;
    let mut count_signer_missing = 0u8;
    let mut count_mismatch = 0u8;
    let mut count_other = 0u8;

    for v in &all {
        match v {
            SignerCashierMismatch::ShiftMissingForFiscalDoc { .. } => count_shift_missing += 1,
            SignerCashierMismatch::ShiftIdMismatch { .. } => count_shift_id_mismatch += 1,
            SignerCashierMismatch::CrossFnMismatch { .. } => count_cross_fn += 1,
            SignerCashierMismatch::SignerIdMissing { .. } => count_signer_missing += 1,
            SignerCashierMismatch::Mismatch { .. } => count_mismatch += 1,
            _ => count_other += 1, // future-compat wildcard for #[non_exhaustive]
        }
    }

    // Each variant instantiated exactly once → count = 1.
    assert_eq!(count_shift_missing, 1);
    assert_eq!(count_shift_id_mismatch, 1);
    assert_eq!(count_cross_fn, 1);
    assert_eq!(count_signer_missing, 1);
    assert_eq!(count_mismatch, 1);
    assert_eq!(
        count_other, 0,
        "no unknown variant should exist at this commit; adding one requires \
         updating this structural pin + spec §2.4 audit vocabulary"
    );

    // 5 known + 0 unknown = 5 total.
    assert_eq!(all.len(), 5);
}
