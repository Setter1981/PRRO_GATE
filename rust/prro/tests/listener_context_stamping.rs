//! W4-Z0 piece 9 — listener-stamped driver_id + FN validation.
//!
//! Per spec §4.  Listener supplies `(driver_id, fn)` from
//! `ops/config.yaml` per-port config to `to_canonical_fiscal_command_with_context`;
//! mismatch between wire FN and listener-config FN is caught early
//! as a typed error.

use prro::db::models::ids::DriverId;
use prro::runtime::ingress::dto::{self, CanonicalCommand, MappingError};

const SAMPLE_WIRE: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "4538765845",
  "command_type": "SHIFT_OPEN",
  "idempotency_key": "maria304:4538765845:sess:open",
  "cashier_id": "csh-007",
  "department": null,
  "return_check_number": null,
  "payload": {
    "direction": "SALE",
    "totals": { "sale_kopecks": 0, "return_kopecks": 0 }
  }
}"#;

#[test]
fn listener_context_stamps_driver_id_on_canonical_command() {
    let wire: CanonicalCommand = serde_json::from_str(SAMPLE_WIRE).unwrap();
    let driver_id = DriverId::new("maria304").unwrap();

    let canonical =
        dto::to_canonical_fiscal_command_with_context(&wire, driver_id.clone(), "4538765845")
            .expect("listener context wraps mapper successfully");

    assert_eq!(canonical.driver_id.as_ref().unwrap().as_str(), "maria304");
}

#[test]
fn listener_fn_mismatch_returns_typed_error_before_mapping() {
    let wire: CanonicalCommand = serde_json::from_str(SAMPLE_WIRE).unwrap();
    let driver_id = DriverId::new("maria304").unwrap();

    let err = dto::to_canonical_fiscal_command_with_context(
        &wire,
        driver_id,
        "9999999999", // misconfigured listener — different FN than wire
    )
    .expect_err("FN mismatch must surface typed error");

    match err {
        MappingError::FnConfigMismatch {
            wire_fn,
            listener_fn,
        } => {
            assert_eq!(wire_fn, "4538765845");
            assert_eq!(listener_fn, "9999999999");
        }
        other => panic!("expected FnConfigMismatch, got: {other:?}"),
    }
}

#[test]
fn driver_id_validates_empty_string() {
    let err = DriverId::new("").expect_err("empty rejected");
    assert!(matches!(err, prro::db::models::ids::DriverIdError::Empty));
}

/// Audit Round-2 (2026-05-27): whitespace-only strings would silently
/// fail driver_tax_mapping lookups at runtime.  Trim at construction.
#[test]
fn driver_id_rejects_whitespace_only_string() {
    for whitespace in ["   ", "\t", "\n\n", " \t \n"] {
        let err = DriverId::new(whitespace).expect_err("whitespace-only rejected");
        assert!(matches!(err, prro::db::models::ids::DriverIdError::Empty));
    }
}

#[test]
fn driver_id_trims_surrounding_whitespace() {
    let id = DriverId::new("  maria304\n  ").expect("trim succeeds");
    assert_eq!(id.as_str(), "maria304");
}

#[test]
fn driver_id_validates_max_length() {
    let too_long = "x".repeat(65);
    let err = DriverId::new(too_long).expect_err("too-long rejected");
    assert!(matches!(
        err,
        prro::db::models::ids::DriverIdError::TooLong(65)
    ));
}

#[test]
fn plain_mapper_leaves_driver_id_none() {
    let wire: CanonicalCommand = serde_json::from_str(SAMPLE_WIRE).unwrap();
    let canonical = dto::to_canonical_fiscal_command(&wire).unwrap();
    assert!(
        canonical.driver_id.is_none(),
        "plain mapper (no listener context) leaves driver_id None"
    );
}
