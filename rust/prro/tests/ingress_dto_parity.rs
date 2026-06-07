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
//! Contracts:
//!
//!   1. Each CommandType fixture round-trips through `serde_json::from_str`
//!      → `CanonicalCommand` → `serde_json::to_string` losslessly at the
//!      typed-value layer (NOT byte-equal vs original fixture whitespace
//!      — formatting differs — but `cmd_from(parse(orig)) ==
//!      cmd_from(parse(reserialise(parse(orig))))`).
//!   2. Each fixture maps cleanly through `to_canonical_fiscal_command`
//!      to a `CanonicalFiscalCommand` with the expected `doc_type`,
//!      `total_sum_kop`, and a non-zero `payload_sha256_canonical`.
//!      `signed_by_cashier_id` is asserted `Some` for fixtures that
//!      carry a non-empty `cashier_id` field and `None` for the
//!      fixture that omits it (see test
//!      `cashier_id_null_maps_to_signed_by_cashier_id_none`).
//!   3. Two semantically-identical CanonicalCommand values with
//!      different field-order JSON produce the SAME
//!      `payload_sha256_canonical` (canonical-JSON hash invariant).
//!   4. A `schema_version` other than "1.0" yields a typed
//!      `MappingError::SchemaVersionMismatch`.
//!   5. A `command_type` of `PERIODIC_REPORT` (driver-only,
//!      non-fiscal) yields a typed
//!      `MappingError::UnsupportedCommandType` — preserves Invariant
//!      #6 (no silent canonical-payload drift).
//!   6. All 9 fiscal CommandType variants (SELL, RETURN, SHIFT_OPEN,
//!      SHIFT_CLOSE, X_REPORT, Z_REPORT, SERVICE_IN, SERVICE_OUT,
//!      CASH_WITHDRAWAL) have a fixture exercising parse + map.

use prro::db::models::enums::DocType;
use prro::runtime::ingress::dto::{self, CanonicalCommand, CommandType, MappingError};

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

const FIXTURE_SHIFT_CLOSE: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "SHIFT_CLOSE",
  "idempotency_key": "maria304:3001234567:sess123:close",
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

const FIXTURE_X_REPORT: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "X_REPORT",
  "idempotency_key": "maria304:3001234567:sess123:x",
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

const FIXTURE_SERVICE_IN: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "SERVICE_IN",
  "idempotency_key": "maria304:3001234567:sess123:svc-in",
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

const FIXTURE_SERVICE_OUT: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "SERVICE_OUT",
  "idempotency_key": "maria304:3001234567:sess123:svc-out",
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

const FIXTURE_CASH_WITHDRAWAL: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "CASH_WITHDRAWAL",
  "idempotency_key": "maria304:3001234567:sess123:cw",
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

const FIXTURE_EMPTY_CASHIER: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "SHIFT_OPEN",
  "idempotency_key": "maria304:3001234567:sess123:open-empty-cashier",
  "cashier_id": "",
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

const FIXTURE_NULL_CASHIER: &str = r#"{
  "schema_version": "1.0",
  "fiscal_number": "3001234567",
  "command_type": "SHIFT_OPEN",
  "idempotency_key": "maria304:3001234567:sess123:open-no-cashier",
  "cashier_id": null,
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
fn each_fiscal_command_type_fixture_parses_and_maps() {
    // Contract #6 — all 9 fiscal CommandType variants exercised.
    // Cash-movement ops (ServiceIn/ServiceOut/CashWithdrawal) map to
    // None total per dto.rs derivation doc — the source `Totals`
    // struct only carries SELL/RETURN amounts; cash-op amounts live
    // in `raw_frames` and are parsed by M5.
    for (label, fixture, expected_doc_type, expected_total) in [
        ("SELL", FIXTURE_SELL, DocType::Sell, Some(2500_i64)),
        ("RETURN", FIXTURE_RETURN, DocType::Return, Some(2500_i64)),
        ("SHIFT_OPEN", FIXTURE_SHIFT_OPEN, DocType::ShiftOpen, None),
        (
            "SHIFT_CLOSE",
            FIXTURE_SHIFT_CLOSE,
            DocType::ShiftClose,
            None,
        ),
        ("X_REPORT", FIXTURE_X_REPORT, DocType::XReport, None),
        ("Z_REPORT", FIXTURE_Z_REPORT, DocType::ZReport, None),
        ("SERVICE_IN", FIXTURE_SERVICE_IN, DocType::ServiceIn, None),
        (
            "SERVICE_OUT",
            FIXTURE_SERVICE_OUT,
            DocType::ServiceOut,
            None,
        ),
        (
            "CASH_WITHDRAWAL",
            FIXTURE_CASH_WITHDRAWAL,
            DocType::CashWithdrawal,
            None,
        ),
    ] {
        let cmd: CanonicalCommand =
            serde_json::from_str(fixture).unwrap_or_else(|e| panic!("{label}: parse fixture: {e}"));
        let mapped = dto::to_canonical_fiscal_command(&cmd)
            .unwrap_or_else(|e| panic!("{label}: map: {e:?}"));
        assert_eq!(mapped.doc_type, expected_doc_type, "{label}: doc_type");
        assert_eq!(
            mapped.total_sum_kop, expected_total,
            "{label}: total_sum_kop"
        );
        assert!(
            mapped.signed_by_cashier_id.is_some(),
            "{label}: signed_by_cashier_id derived from non-empty cashier_id"
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
fn cashier_id_null_maps_to_signed_by_cashier_id_none() {
    // Contract #2 explicit `None`-branch coverage (self-review #7).
    let cmd: CanonicalCommand =
        serde_json::from_str(FIXTURE_NULL_CASHIER).expect("parse null-cashier fixture");
    let mapped = dto::to_canonical_fiscal_command(&cmd).expect("map null-cashier fixture");
    assert!(
        mapped.signed_by_cashier_id.is_none(),
        "cashier_id = null must map to signed_by_cashier_id = None"
    );
}

#[test]
fn schema_version_mismatch_returns_typed_error() {
    let cmd: CanonicalCommand = serde_json::from_str(FIXTURE_BAD_SCHEMA).expect("parse");
    let err = dto::to_canonical_fiscal_command(&cmd).expect_err("schema 2.0 must reject");
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
    let cmd: CanonicalCommand = serde_json::from_str(FIXTURE_PERIODIC_REPORT).expect("parse");
    let err = dto::to_canonical_fiscal_command(&cmd).expect_err("PERIODIC_REPORT not in DocType");
    assert!(
        matches!(
            err,
            MappingError::UnsupportedCommandType(CommandType::PeriodicReport)
        ),
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
fn fixture_round_trips_through_serde_losslessly_at_typed_layer() {
    // Round-trip: parse fixture, re-serialise, parse again, compare
    // typed values.  We do NOT assert byte-equality of the re-serialised
    // form vs the original fixture text — formatting whitespace differs
    // — but we DO assert two-stage typed-value equivalence per the
    // module-level Contract #1.
    let cmd: CanonicalCommand = serde_json::from_str(FIXTURE_SELL).unwrap();
    let serialised = serde_json::to_string(&cmd).unwrap();
    let reparsed: CanonicalCommand = serde_json::from_str(&serialised).unwrap();
    assert_eq!(cmd, reparsed, "serde round-trip must be lossless");
}

#[test]
fn canonical_hash_scope_is_payload_only_not_envelope() {
    // Self-review #3 (2026-05-26): `payload_sha256_canonical` MUST hash
    // `cmd.payload` only, not the full envelope.  Two commands with
    // the same goods/payments/totals but different `idempotency_key`
    // and `cashier_id` MUST produce the same hash so retries with a
    // fresh idempotency key on identical fiscal content remain
    // detectable as payload-equivalent.
    let cmd_a: CanonicalCommand = serde_json::from_str(FIXTURE_SELL).unwrap();
    let mut cmd_b = cmd_a.clone();
    cmd_b.idempotency_key = "completely-different-key-string".to_string();
    cmd_b.cashier_id = Some("different-cashier".to_string());
    cmd_b.department = Some("99".to_string());
    let mapped_a = dto::to_canonical_fiscal_command(&cmd_a).unwrap();
    let mapped_b = dto::to_canonical_fiscal_command(&cmd_b).unwrap();
    assert_eq!(
        mapped_a.payload_sha256_canonical, mapped_b.payload_sha256_canonical,
        "payload_sha256_canonical must hash cmd.payload only — \
         envelope routing metadata (idempotency_key, cashier_id, \
         department) MUST NOT affect the hash"
    );
}

#[test]
fn cashier_id_empty_string_returns_typed_invalid_error() {
    // Round-2 audit finding #3 (2026-05-26): empty `cashier_id` string
    // is MALFORMED wire input, NOT semantic-absent.  The previous
    // `.filter(|s| !s.is_empty())` collapsed it to `None` and the
    // failure surfaced downstream at the signer with the original
    // context erased.  Boundary rejection: `CashierId::new("")` →
    // `CashierIdError::Empty` → `MappingError::InvalidCashierId(_)`.
    let cmd: CanonicalCommand =
        serde_json::from_str(FIXTURE_EMPTY_CASHIER).expect("parse empty-cashier fixture");
    let err = dto::to_canonical_fiscal_command(&cmd)
        .expect_err("empty cashier_id must yield typed boundary error");
    assert!(
        matches!(err, MappingError::InvalidCashierId(_)),
        "got: {err:?} — expected MappingError::InvalidCashierId(_)"
    );
}

/// RS-2 piece-7 (M5 inversion of the former `#[ignore]`'d
/// `mapped_payload_json_is_wire_shape_not_stage_sign_ready`): the W3 DTO↔
/// stage_sign payload-shape gap is CLOSED by `convert::convert_to_signer_payload`
/// (RS-2 piece-2a).  POSITIVE parse-through: a SELL → `to_canonical_fiscal_command`
/// (wire shape) → `convert_to_signer_payload` (signer-ready `CheckJson`) PARSES
/// through `stage_sign`'s `deny_unknown_fields` validator, instead of being
/// rejected.
///
/// NOTE: uses a SYNTHETIC SELL carrying `article_code` — NOT the shared
/// `FIXTURE_SELL`, which omits it (`article_code` is `Option` in the wire
/// contract, but `convert` is fail-closed on a missing item code; the negative
/// `..._without_article_code_is_rejected_by_convert` test below pins exactly
/// that, so this is not read as "the parity fixture is convert-ready").
///
/// Gated on `test-support` (the validator is a test-support seam); the rest of
/// this file's parity tests compile without it.  Run with
/// `cargo test -p prro --features test-support`.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn sell_with_article_code_converts_to_stage_sign_ready_payload() {
    use prro::db::models::enums::FiscalMode;
    use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
    use prro::db::repositories::payment_methods::{self, NewPaymentMethod};
    use prro::db::{open_pool, open_secure_pool};
    use prro::runtime::ingress::convert::convert_to_signer_payload;
    use prro::services::write_path::stage_sign::{
        derive_wire_artifact_kind, validate_signer_payload_shape_for_testing,
    };

    const FN: &str = "3001234567"; // the SELL fixture's FN

    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool");
    let secure = open_secure_pool(&dir.path().join("s.db"))
        .await
        .expect("open_secure_pool");
    fn_repo::insert(
        &main,
        &NewFnConfig {
            fiscal_number: FN.to_string(),
            tax_number: "12345678".to_string(),
            vat_payer_inn: None,
            fiscal_mode: FiscalMode::Test,
            org_name: None,
            point_name: None,
            org_address: None,
            tsp_enabled: false,
            offline_enabled: true,
            national_check_enabled: false,
            min_offline_codes: 0,
            max_offline_codes: 0,
        },
    )
    .await
    .expect("seed fn_config");
    // The SELL fixture pays CASH → convert needs the D1 slot-1 payment method.
    payment_methods::insert(
        &secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 1,
            name: "Готівка".to_string(),
            iscash: true,
        },
    )
    .await
    .expect("seed cash slot");

    // A SELL with `article_code` (convert is fail-closed on a missing item
    // code — no line-index fallback; the shared FIXTURE_SELL omits it on
    // purpose for the mapper-level parity tests).
    let sell = r#"{
        "schema_version": "1.0", "fiscal_number": "3001234567",
        "command_type": "SELL", "idempotency_key": "k", "cashier_id": "csh-007",
        "department": "1", "return_check_number": null,
        "payload": {
            "direction": "SALE",
            "goods": [{"name":"Паляниця","quantity_milli":1000,"price_kopecks":2500,
                       "tax_group_1":1,"tax_group_2":0,"article_code":42}],
            "payments": [{"type":"CASH","amount_kopecks":2500}],
            "totals": {"sale_kopecks":2500,"return_kopecks":0}
        }
    }"#;
    let cmd: CanonicalCommand = serde_json::from_str(sell).unwrap();
    // The mapper still emits the WIRE shape (price_kopecks / quantity_milli) …
    let mapped = dto::to_canonical_fiscal_command(&cmd).expect("map");
    assert!(
        mapped.payload_json.contains("price_kopecks"),
        "to_canonical_fiscal_command still emits the wire shape"
    );
    // … and the converter turns it into the signer-ready shape that PARSES
    // through stage_sign's deny_unknown_fields validator (gap closed).
    let converted = convert_to_signer_payload(&cmd, FN, &main, &secure)
        .await
        .expect("convert SELL to signer-ready payload");
    let kind = derive_wire_artifact_kind(DocType::Sell).expect("wire artifact kind");
    validate_signer_payload_shape_for_testing(kind, &converted.payload_json, Some(2500)).expect(
        "the converted SELL CheckJson must parse through stage_sign \
         (deny_unknown_fields) — the W3 gap is closed",
    );
}

/// review (M2): the SHARED `FIXTURE_SELL` omits `article_code` — it is `Option`
/// in the wire contract, so the MAPPER accepts it, but RS-2 `convert` is
/// FAIL-CLOSED on a missing item code (no line-index fallback, operator-pinned)
/// → a maria304 SELL without `article_code` gets a typed `MissingItemCode`
/// reject, NOT a silent fabricated code.  Pinning this here means the positive
/// test above is never misread as "the parity fixture is convert-ready", and
/// makes the driver's article_code obligation a CI-visible fact.
#[tokio::test]
async fn sell_fixture_without_article_code_is_rejected_by_convert() {
    use prro::db::{open_pool, open_secure_pool};
    use prro::runtime::ingress::convert::{convert_to_signer_payload, ConvertError};

    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool");
    let secure = open_secure_pool(&dir.path().join("s.db"))
        .await
        .expect("open_secure_pool");

    let cmd: CanonicalCommand = serde_json::from_str(FIXTURE_SELL).unwrap();
    // The mapper accepts it (article_code is Option in the wire DTO) …
    dto::to_canonical_fiscal_command(&cmd).expect("mapper accepts; article_code is Option");
    // … but convert fails closed on the missing item code (no fallback), before
    // any payment lookup.
    let err = convert_to_signer_payload(&cmd, "3001234567", &main, &secure)
        .await
        .expect_err("FIXTURE_SELL has no article_code → convert must reject");
    assert!(
        matches!(err, ConvertError::MissingItemCode { .. }),
        "expected MissingItemCode, got {err:?}"
    );
}

/// RS-2 piece-7 (M5 inversion of the former `#[ignore]`'d
/// `xreport_servicein_serviceout_cashwithdrawal_map_but_signer_will_reject`):
/// the W3 audit MED-2 defer is CLOSED.  These CommandTypes are now rejected at
/// the INGRESS BOUNDARY by `classify_command` (RS-2 piece-1b) — BEFORE convert
/// / sign — instead of being accepted by the mapper and failing late at the
/// signer.  X_REPORT is read-only; the cash-movement ops + PERIODIC_REPORT are
/// unsupported.  Boundary rejection, single source of truth (the policy), no
/// late-typed-error drift.
#[test]
fn read_only_and_unsupported_command_types_rejected_at_ingress_boundary() {
    use prro::runtime::ingress::policy::{classify_command, CommandClass};

    // X_REPORT — read-only (LEGAL X_REPORT): never fiscalized; served (if at
    // all) by the read-only status surface, not the fiscal POST.
    assert_eq!(
        classify_command(CommandType::XReport),
        CommandClass::ReadOnly
    );

    // Cash-movement ops + the driver-only PERIODIC_REPORT — typed-unsupported
    // BEFORE any inbox write (the signer's old `UnsupportedDocType` is no
    // longer the first line of defence).
    for ct in [
        CommandType::ServiceIn,
        CommandType::ServiceOut,
        CommandType::CashWithdrawal,
        CommandType::PeriodicReport,
    ] {
        assert_eq!(classify_command(ct), CommandClass::Unsupported, "{ct:?}");
    }

    // The signable set still maps cleanly (the boundary lets these through).
    for (label, fixture) in [
        ("SELL", FIXTURE_SELL),
        ("RETURN", FIXTURE_RETURN),
        ("SHIFT_OPEN", FIXTURE_SHIFT_OPEN),
        ("SHIFT_CLOSE", FIXTURE_SHIFT_CLOSE),
        ("Z_REPORT", FIXTURE_Z_REPORT),
    ] {
        let cmd: CanonicalCommand = serde_json::from_str(fixture).unwrap();
        assert_eq!(
            classify_command(cmd.command_type),
            CommandClass::Signable,
            "{label} must be signable at the boundary"
        );
    }
}
