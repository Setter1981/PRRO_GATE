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

#[test]
#[ignore = "documents known DTO↔stage_sign payload-shape gap; see \
            module-level 'Known scope gap' docs; conversion stage \
            deferred to W4 / W5 supervisor wiring"]
fn mapped_payload_json_is_wire_shape_not_stage_sign_ready() {
    // External audit Round-1 finding HIGH-1 (2026-05-26):
    //
    // `to_canonical_fiscal_command` populates `payload_json` with the
    // driver-wire-shape canonical JSON of `cmd.payload`.  But
    // `services/write_path/stage_sign::parse_payload` uses
    // `deny_unknown_fields` and expects a DIFFERENT internal canonical
    // shape (`CheckJson { items[].code/price_kop/quantity_thousandths/
    // sum_kop, payments[].type_code, ... }` for SELL/RETURN;
    // `ZReportJson { sell_count, return_count, payments[].sum_in_kop /
    // sum_out_kop }` for Z_REPORT; `ShiftOpenJson { opening_sum_kop }`
    // for SHIFT_OPEN).
    //
    // Fields do not align by name OR by data — `ZReportJson.sell_count`
    // and `ZReportJson.return_count` are not in the W3 DTO at all
    // (derived from ledger rows since shift_open_at_business_ts, which
    // is repository-touching code — not pure DTO transformation, so
    // it does not belong in W3).
    //
    // This test exists to:
    //   (a) document the gap in CI-visible form (so a future
    //       worklet review sees it explicitly);
    //   (b) act as the failing precondition test for W4/W5 — when the
    //       conversion stage lands, this test gets unignored and
    //       inverted (assert that mapped + converted payload DOES
    //       parse through stage_sign::parse_payload).
    //
    // Until W4/W5: `#[ignore]`'d so CI stays green but anyone running
    // `cargo test -- --include-ignored` sees the gap.  Acceptance
    // criterion in plan §3 W3 ("payload_sha256_canonical matches the
    // one that ingress_inbox::insert would compute") IS satisfied
    // (DTO→hash is byte-stable); the OTHER half of the original
    // §3 W3 implicit expectation ("payload_json passes through
    // stage_sign") is the explicit W4/W5 surface.

    let cmd: CanonicalCommand = serde_json::from_str(FIXTURE_SELL).unwrap();
    let mapped =
        dto::to_canonical_fiscal_command(&cmd).expect("DTO mapping is fine; the GAP is downstream");

    // Demonstrate the gap structurally — the wire-shape payload_json
    // contains DTO field names that stage_sign's CheckJson would
    // reject with deny_unknown_fields.  We do NOT import stage_sign
    // here (private internal types); the structural string check is
    // sufficient to make the gap a CI-grep-able fact rather than
    // tribal knowledge.
    assert!(
        mapped.payload_json.contains("price_kopecks"),
        "wire DTO field 'price_kopecks' present — stage_sign \
         CheckItemJson expects 'price_kop' (and would reject \
         'price_kopecks' under deny_unknown_fields)"
    );
    assert!(
        mapped.payload_json.contains("quantity_milli"),
        "wire DTO field 'quantity_milli' present — stage_sign \
         CheckItemJson expects 'quantity_thousandths' (and would \
         reject 'quantity_milli' under deny_unknown_fields)"
    );

    // This negative assertion is the inversion target for W4/W5: once
    // the conversion lands, replace it with a positive parse-through
    // assertion using stage_sign's payload parser.
    panic!(
        "DTO→CheckJson conversion not yet implemented; deferred to \
         W4/W5 supervisor wiring per plan §3 W3 acceptance note + \
         W4 Algorithm step 0 addendum"
    );
}

#[test]
#[ignore = "documents coupled-defer for XReport/ServiceIn/ServiceOut/\
            CashWithdrawal — these CommandTypes map cleanly through \
            to_canonical_fiscal_command but stage_sign::\
            derive_wire_artifact_kind rejects them with typed \
            UnsupportedDocType; reject-at-mapper-boundary deferred \
            to coupled W4/W5 work"]
fn xreport_servicein_serviceout_cashwithdrawal_map_but_signer_will_reject() {
    // External audit Round-1 finding MED-2 (2026-05-26):
    //
    // Stage 3 (stage_sign::derive_wire_artifact_kind) returns
    // `SignError::UnsupportedDocType { doc_type }` for X_REPORT,
    // SERVICE_IN, SERVICE_OUT, CASH_WITHDRAWAL.  The W3 mapper
    // accepts all 9 fiscal CommandTypes (only PERIODIC_REPORT is
    // rejected at boundary — that one is non-fiscal driver-only).
    //
    // The defect surface is typed-error-shifted-late: an accepted
    // ingress command can be persisted into the write path and fail
    // later at signing instead of returning a typed boundary error
    // upfront.  But (a) the late error IS typed (UnsupportedDocType
    // not a panic / silent drop) and (b) reject-at-mapper-boundary
    // means duplicating the unsupported set across two layers (DTO
    // boundary + signer boundary) which drifts.
    //
    // Coupled-defer with HIGH-1: the same W4/W5 conversion-stage
    // worklet that decides DTO→CheckJson conversion will also decide
    // whether ServiceIn/ServiceOut/CashWithdrawal/XReport (a) gain
    // conversion support (write-path implements their wire artifacts)
    // OR (b) get rejected at boundary (single source of truth for
    // unsupported set is `derive_wire_artifact_kind`'s match).

    for (label, fixture, expected_doc_type) in [
        ("X_REPORT", FIXTURE_X_REPORT, DocType::XReport),
        ("SERVICE_IN", FIXTURE_SERVICE_IN, DocType::ServiceIn),
        ("SERVICE_OUT", FIXTURE_SERVICE_OUT, DocType::ServiceOut),
        (
            "CASH_WITHDRAWAL",
            FIXTURE_CASH_WITHDRAWAL,
            DocType::CashWithdrawal,
        ),
    ] {
        let cmd: CanonicalCommand = serde_json::from_str(fixture).unwrap();
        let mapped = dto::to_canonical_fiscal_command(&cmd)
            .unwrap_or_else(|e| panic!("{label}: mapper currently accepts, got error: {e:?}"));
        assert_eq!(mapped.doc_type, expected_doc_type, "{label}");
    }

    // Inversion target for W4/W5: replace the above accept-loop with
    // an expect_err loop asserting `MappingError::UnsupportedCommandType`
    // at the boundary OR with an expect_ok loop that verifies the
    // signer-side accepts the converted payload, depending on the
    // chosen direction.
    panic!(
        "Coupled-defer with HIGH-1 (DTO↔stage_sign conversion); \
         see plan §3 W3 + W4 addendum"
    );
}
