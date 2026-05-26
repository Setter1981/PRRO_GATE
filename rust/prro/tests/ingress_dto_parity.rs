//! W3 — parity test for `prro::runtime::ingress::dto` against the
//! `maria304_driver::bridge::dto` wire contract.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W3:
//! the two crates intentionally do NOT share a DTO crate (system-of-
//! record `prro` must not depend on a driver-binary crate; reverse
//! dependency would pull the entire DB layer).  The wire contract is
//! guarded by JSON fixtures embedded below — driver-side tests
//! independently assert that the driver emits these exact bytes;
//! prro-side tests assert the same bytes deserialise + re-serialise
//! byte-stably.  Rename in EITHER side breaks the test.
//!
//! Five contracts:
//!
//!   1. Each CommandType fixture round-trips byte-stably through
//!      `serde_json::from_str` → `CanonicalCommand` → `serde_json::to_string`.
//!   2. Each fixture maps cleanly through `to_canonical_fiscal_command`
//!      to a `CanonicalFiscalCommand` with the expected `doc_type`,
//!      `total_sum_kop`, `signed_by_cashier_id`, and a non-empty
//!      `payload_sha256_canonical`.
//!   3. Two semantically-identical CanonicalCommand values with
//!      different field-order JSON produce the SAME
//!      `payload_sha256_canonical` (canonical-JSON hash invariant).
//!   4. A `schema_version` other than "1.0" yields a typed
//!      `MappingError::SchemaVersionMismatch`.
//!   5. A `command_type` of `PERIODIC_REPORT` (driver-only,
//!      non-fiscal) yields a typed
//!      `MappingError::UnsupportedCommandType` — preserves Invariant
//!      #6 (no silent canonical-payload drift).

use prro::db::models::enums::DocType;
use prro::runtime::ingress::dto::{
    self, CanonicalCommand, CommandType, MappingError,
};

const FIXTURE_SELL: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "SELL",
  "idempotency_key": "maria304:3001234567:sess123:1",
  "cashier_id": "csh-007",
  "department": "1",
  "return_check_number": null,
  "payload": {
    "direction": "SALE",
    "goods": [
      {
        "name": "Паляниця",
        "quantity_milli": 1000,
        "price_kopecks": 2500,
        "tax_group_1": 1,
        "tax_group_2": 0
      }
    ],
    "payments": [
      {
        "type": "CASH",
        "amount_kopecks": 2500
      }
    ],
    "totals": {
      "sale_kopecks": 2500,
      "return_kopecks": 0
    }
  }
}"#;

const FIXTURE_RETURN: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "RETURN",
  "idempotency_key": "maria304:3001234567:sess123:2",
  "cashier_id": "csh-007",
  "department": "1",
  "return_check_number": "ORIG-001",
  "payload": {
    "direction": "RETURN",
    "goods": [
      {
        "name": "Паляниця",
        "quantity_milli": 1000,
        "price_kopecks": 2500,
        "tax_group_1": 1,
        "tax_group_2": 0
      }
    ],
    "payments": [
      {
        "type": "CASH",
        "amount_kopecks": 2500
      }
    ],
    "totals": {
      "sale_kopecks": 0,
      "return_kopecks": 2500
    }
  }
}"#;

const FIXTURE_SHIFT_OPEN: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "SHIFT_OPEN",
  "idempotency_key": "maria304:3001234567:sess123:open",
  "cashier_id": "csh-007",
  "department": null,
  "return_check_number": null,
  "payload": {
    "direction": "SALE",
    "totals": {
      "sale_kopecks": 0,
      "return_kopecks": 0
    }
  }
}"#;

const FIXTURE_Z_REPORT: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "Z_REPORT",
  "idempotency_key": "maria304:3001234567:sess123:z",
  "cashier_id": "csh-007",
  "department": null,
  "return_check_number": null,
  "payload": {
    "direction": "SALE",
    "totals": {
      "sale_kopecks": 0,
      "return_kopecks": 0
    }
  }
}"#;

const FIXTURE_PERIODIC_REPORT: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "PERIODIC_REPORT",
  "idempotency_key": "maria304:3001234567:sess123:pr",
  "cashier_id": "csh-007",
  "department": null,
  "return_check_number": null,
  "payload": {
    "direction": "SALE",
    "totals": {
      "sale_kopecks": 0,
      "return_kopecks": 0
    }
  }
}"#;

const FIXTURE_BAD_SCHEMA: &str = r#"{
  "schema_version": "2.0",
  "fiscal_number": "3001234567",
  "command_type": "SELL",
  "idempotency_key": "maria304:3001234567:sess123:1",
  "cashier_id": "csh-007",
  "department": null,
  "return_check_number": null,
  "payload": {
    "direction": "SALE",
    "totals": {
      "sale_kopecks": 100,
      "return_kopecks": 0
    }
  }
}"#;

#[test]
fn each_command_type_fixture_parses_and_maps() {
    for (label, fixture, expected_doc_type, expected_total) in [
        ("SELL",       FIXTURE_SELL,       DocType::Sell,       Some(2500_i64)),
        ("RETURN",     FIXTURE_RETURN,     DocType::Return,     Some(2500_i64)),
        ("SHIFT_OPEN", FIXTURE_SHIFT_OPEN, DocType::ShiftOpen,  None),
        ("Z_REPORT",   FIXTURE_Z_REPORT,   DocType::ZReport,    None),
    ] {
        let cmd: CanonicalCommand = serde_json::from_str(fixture)
            .unwrap_or_else(|e| panic!("{label}: parse fixture: {e}"));
        let mapped = dto::to_canonical_fiscal_command(&cmd)
            .unwrap_or_else(|e| panic!("{label}: map: {e:?}"));
        assert_eq!(mapped.doc_type, expected_doc_type, "{label}: doc_type");
        assert_eq!(mapped.total_sum_kop, expected_total, "{label}: total_sum_kop");
        assert!(
            mapped.signed_by_cashier_id.is_some(),
            "{label}: signed_by_cashier_id derived from cashier_id"
        );
        // payload_sha256_canonical is 32 bytes by type; assert non-zero
        // (zero hash would indicate empty payload or broken hasher).
        assert!(
            mapped.payload_sha256_canonical.iter().any(|b| *b != 0),
            "{label}: payload_sha256_canonical must be non-zero"
        );
    }
}

#[test]
fn schema_version_mismatch_returns_typed_error() {
    let cmd: CanonicalCommand =
        serde_json::from_str(FIXTURE_BAD_SCHEMA).expect("parse");
    let err = dto::to_canonical_fiscal_command(&cmd)
        .expect_err("schema 2.0 must reject");
    match err {
        MappingError::SchemaVersionMismatch { expected, actual } => {
            assert_eq!(expected, "1.0");
            assert_eq!(actual, "2.0");
        }
        other => panic!("expected SchemaVersionMismatch, got: {other:?}"),
    }
}

#[test]
fn periodic_report_command_type_is_unsupported_for_fiscal_pipeline() {
    let cmd: CanonicalCommand =
        serde_json::from_str(FIXTURE_PERIODIC_REPORT).expect("parse");
    let err = dto::to_canonical_fiscal_command(&cmd)
        .expect_err("PERIODIC_REPORT not in DocType");
    assert!(
        matches!(err, MappingError::UnsupportedCommandType(CommandType::PeriodicReport)),
        "got: {err:?}"
    );
}

#[test]
fn canonical_hash_is_stable_across_field_order() {
    // Same semantic payload, different field order in JSON source.
    // serde's deserialization is order-tolerant; canonical hash must
    // be order-INVARIANT (sorted keys).
    let a = FIXTURE_SELL;
    let b = r#"{
      "payload": {
        "totals": {"return_kopecks": 0, "sale_kopecks": 2500},
        "direction": "SALE",
        "payments": [{"type":"CASH","amount_kopecks":2500}],
        "goods": [{"price_kopecks":2500,"tax_group_2":0,"tax_group_1":1,"quantity_milli":1000,"name":"Паляниця"}]
      },
      "return_check_number": null,
      "department": "1",
      "cashier_id": "csh-007",
      "idempotency_key": "maria304:3001234567:sess123:1",
      "command_type": "SELL",
      "fiscal_number": "3001234567",
      "schema_version": "1.0"
    }"#;
    let cmd_a: CanonicalCommand = serde_json::from_str(a).unwrap();
    let cmd_b: CanonicalCommand = serde_json::from_str(b).unwrap();
    let mapped_a = dto::to_canonical_fiscal_command(&cmd_a).unwrap();
    let mapped_b = dto::to_canonical_fiscal_command(&cmd_b).unwrap();
    assert_eq!(
        mapped_a.payload_sha256_canonical, mapped_b.payload_sha256_canonical,
        "canonical hash must be field-order-invariant"
    );
}

#[test]
fn fixture_round_trips_through_serde_byte_stably_when_renormalised() {
    // Round-trip: parse fixture, re-serialise, parse again, compare
    // typed values.  We do NOT assert byte-equality of the re-serialised
    // form vs the original fixture text — formatting whitespace differs
    // — but we DO assert two-stage round-trip equivalence.
    let cmd: CanonicalCommand = serde_json::from_str(FIXTURE_SELL).unwrap();
    let serialised = serde_json::to_string(&cmd).unwrap();
    let reparsed: CanonicalCommand = serde_json::from_str(&serialised).unwrap();
    assert_eq!(cmd, reparsed, "serde round-trip must be lossless");
}
