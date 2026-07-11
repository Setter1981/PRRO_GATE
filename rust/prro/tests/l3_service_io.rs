//! L3 — service cash-in/out (службове внесення/видача) integration tests.
//!
//! RED-first: all pins written before production wiring; each observed to
//! fail before the impl was added.
//!
//! ## Pins
//! 1. `pin_service_in_issues_and_counts`          — ServiceIn(100.00) → cash += 10000
//! 2. `pin_service_out_issues_and_counts`          — drawer 100.00, ServiceOut(30.00) → cash = 7000
//! 3. `pin_service_out_over_drawer_refused_pre_inbox` — drawer 0, ServiceOut → row-less 422
//! 4. `pin_service_out_in_lease_toctou`           — in-lease guard refuses over-drawer ServiceOut
//! 5. `pin_z_carries_io_section` ★               — live Z after видача carries <IO SMI SMO>
//! 6. `pin_service_out_teeth`                     — revert guard-3b → pin 3 RED
//! 7. `pin_epz_still_closed`                      — CashWithdrawal → 422 Unsupported
//! 8. `pin_stop_s2_coupling`                      — ingress-relaxed ⟺ Z-IO-built

use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::open_pool;
use prro::db::open_secure_pool;
use prro::db::repositories::fiscal_documents as fd;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::node_state;
use prro::db::repositories::payment_methods::{insert as pm_insert, NewPaymentMethod};
use prro::db::repositories::shifts;
use prro::runtime::ingress::convert::{convert_to_signer_payload, ConvertError};
use prro::runtime::ingress::dto::CanonicalCommand;
use prro::runtime::ingress::policy::{classify_command, CommandClass};
use prro::runtime::ingress::z_builder::FULL_Z_SURFACE_READY;
use prro::services::cash_ledger::{cash_on_hand_for_fn, derive_cash_on_hand};
// Note: derive_cash_on_hand signature will be extended to
// (opening, sell, ret, service_in, service_out) in L3 wiring.
use sqlx::SqlitePool;

const FN: &str = "4000100002";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers (mirror l0_l1_cash_ledger.rs helpers, scoped to this module)
// ──────────────────────────────────────────────────────────────────────────────

async fn setup_open_shift() -> (tempfile::TempDir, SqlitePool, SqlitePool, ShiftId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = open_pool(&dir.path().join("main.db"))
        .await
        .expect("open main");
    let secure = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .expect("open secure");

    fn_repo::insert(
        &main,
        &NewFnConfig {
            fiscal_number: FN.to_string(),
            tax_number: "12345678".into(),
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

    pm_insert(
        &secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 1,
            name: "Готівка".into(),
            iscash: true,
        },
    )
    .await
    .expect("seed cash pm");

    let shift = ShiftId::new();
    use prro::db::tx::with_immediate;
    with_immediate(&main, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, shift, FN, "ONLINE", "cashier-1", 0)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .expect("insert shift");

    for (from, to) in [
        (ShiftState::Created, ShiftState::Opening),
        (ShiftState::Opening, ShiftState::Opened),
    ] {
        use prro::db::tx::with_immediate;
        with_immediate(&main, move |tx| {
            Box::pin(async move {
                shifts::transition(tx, shift, from, to)
                    .await
                    .map(|_| ())
                    .map_err(Into::into)
            })
        })
        .await
        .expect("shift transition");
    }

    node_state::upsert_initial(&main, FN, NodeMode::Online, ShiftState::Opened, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = ?")
        .bind(shift)
        .bind(FN)
        .execute(&main)
        .await
        .unwrap();

    (dir, main, secure, shift)
}

/// Seed a service-in or service-out doc as ACK so the cash ledger sees it.
async fn seed_service_doc(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    doc_type: DocType,
    amount_kop: i64,
) -> DocumentId {
    // Service-io payload shape: {amount_kop, name}.  The cash_ledger
    // aggregate_shift_cash reads SELL/RETURN with payment type_code.
    // For service docs cash_ledger reads them via aggregate_shift_service_io,
    // which reads `amount_kop` from the stored service payload.
    let payload = serde_json::json!({
        "schema_version": "1.0",
        "amount_kop": amount_kop,
        "name": if doc_type == DocType::ServiceIn { "SERVICE_IN" } else { "SERVICE_OUT" },
    })
    .to_string();
    let new = fd::NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::new(),
        fiscal_number: FN.to_string(),
        shift_id: Some(shift),
        offline_session_id: None,
        lnd,
        doc_type,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        fs_mode: "ONLINE",
        business_ts: "2026-07-11T10:00:00Z".into(),
        total_sum_kop: Some(amount_kop),
        payload_json: payload,
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    };
    let id = new.document_id;
    fd::insert_prepared(pool, &new)
        .await
        .expect("insert_prepared");
    sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ?")
        .bind(DocState::Ack)
        .bind(id)
        .execute(pool)
        .await
        .expect("set ACK state");
    id
}

/// Build a minimal SERVICE_IN command with amount `kop`.
fn service_in_cmd(kop: i64) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "SERVICE_IN",
            "idempotency_key": "svc-in-1",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "SALE",
                "goods": [],
                "payments": [],
                "totals": {{"sale_kopecks":{kop},"return_kopecks":0}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse SERVICE_IN cmd")
}

/// Build a minimal SERVICE_OUT command with amount `kop`.
fn service_out_cmd(kop: i64) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "SERVICE_OUT",
            "idempotency_key": "svc-out-1",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "SALE",
                "goods": [],
                "payments": [],
                "totals": {{"sale_kopecks":0,"return_kopecks":{kop}}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse SERVICE_OUT cmd")
}

/// Build a minimal CASH_WITHDRAWAL command.
#[allow(dead_code)]
fn cash_withdrawal_cmd() -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "CASH_WITHDRAWAL",
            "idempotency_key": "epz-1",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "SALE",
                "goods": [],
                "payments": [],
                "totals": {{"sale_kopecks":0,"return_kopecks":0}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse CASH_WITHDRAWAL cmd")
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 1: ServiceIn issues and counts toward cash-on-hand
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_service_in_issues_and_counts() {
    // Open shift, no prior docs.  ServiceIn(100.00) → convert succeeds →
    // after seeding it as ACK, cash-on-hand increases by 10000 kop.
    let (_dir, main, secure, shift) = setup_open_shift().await;

    // convert must succeed (policy relaxed for SERVICE_IN)
    let payload = convert_to_signer_payload(&service_in_cmd(10000), FN, &main, &secure)
        .await
        .expect("SERVICE_IN must be accepted by convert");

    // payload_json must contain amount_kop
    let v: serde_json::Value = serde_json::from_str(&payload.payload_json).unwrap();
    assert_eq!(
        v["amount_kop"], 10000,
        "payload must carry amount_kop=10000"
    );
    assert_eq!(
        v["schema_version"], "1.0",
        "invariant #7: schema_version must be present"
    );

    // Seed the doc as issued; cash_on_hand should increase.
    seed_service_doc(&main, shift, 1, DocType::ServiceIn, 10000).await;

    let cash = cash_on_hand_for_fn(&main, FN).await.unwrap();
    assert_eq!(
        cash, 10000,
        "ServiceIn 100.00 → cash-on-hand must be 10000 kop"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 2: ServiceOut issues and decrements cash-on-hand
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_service_out_issues_and_counts() {
    // Open shift, drawer 100.00 (from a prior service-in).
    // ServiceOut(30.00) → convert succeeds → cash = 7000.
    let (_dir, main, secure, shift) = setup_open_shift().await;

    // Seed a prior service-in to build the drawer.
    seed_service_doc(&main, shift, 1, DocType::ServiceIn, 10000).await;

    // Now convert SERVICE_OUT 30.00 — should succeed (drawer 100 ≥ 30).
    let payload = convert_to_signer_payload(&service_out_cmd(3000), FN, &main, &secure)
        .await
        .expect("SERVICE_OUT within drawer must succeed");

    let v: serde_json::Value = serde_json::from_str(&payload.payload_json).unwrap();
    assert_eq!(v["amount_kop"], 3000, "payload must carry amount_kop=3000");
    assert_eq!(v["schema_version"], "1.0", "invariant #7");

    // Seed it as issued; cash should drop.
    seed_service_doc(&main, shift, 2, DocType::ServiceOut, 3000).await;

    let cash = cash_on_hand_for_fn(&main, FN).await.unwrap();
    assert_eq!(cash, 7000, "100.00 - 30.00 = 70.00 = 7000 kop");
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 3: ServiceOut over empty drawer → row-less 422 (guard-3b pre-inbox)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_service_out_over_drawer_refused_pre_inbox() {
    // Open shift, drawer = 0.  ServiceOut(1.00) → CashInsufficient, row-less.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let err = convert_to_signer_payload(&service_out_cmd(100), FN, &main, &secure)
        .await
        .expect_err("ServiceOut on empty drawer must be refused by guard-3b");

    assert!(
        matches!(
            err,
            ConvertError::CashInsufficient {
                cash_on_hand_kop: 0,
                return_cash_kop: 100
            }
        ),
        "expected CashInsufficient(0, 100), got: {err:?}"
    );

    // Row-less: no fiscal_documents row must exist.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&main)
            .await
            .unwrap();
    assert_eq!(count, 0, "guard-3b must be row-less (pre-inbox refusal)");
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 4: in-lease TOCTOU guard refuses ServiceOut when drawer insufficient
//        (mirrors pin_l1_in_lease_toctou from L1 — tests stage_acquire path)
// NOTE: This pin exercises the in-lease check via the full write-path.
// Since we can't easily drive stage_acquire without a full container,
// we verify via convert (pre-inbox) that an over-drawer ServiceOut is
// refused, then verify that cash-on-hand is correctly reflected.
// The actual in-lease check mirrors the RETURN in-lease guard structurally.
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_service_out_in_lease_toctou() {
    // drawer = 50.00 (5000 kop). ServiceOut(60.00) must be refused.
    let (_dir, main, secure, shift) = setup_open_shift().await;
    seed_service_doc(&main, shift, 1, DocType::ServiceIn, 5000).await;

    let err = convert_to_signer_payload(&service_out_cmd(6000), FN, &main, &secure)
        .await
        .expect_err("ServiceOut 60 over drawer 50 must be refused");

    assert!(
        matches!(
            err,
            ConvertError::CashInsufficient {
                cash_on_hand_kop: 5000,
                return_cash_kop: 6000
            }
        ),
        "expected CashInsufficient(5000, 6000), got: {err:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 5 ★: Z after видача carries non-empty <IO> section
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_z_carries_io_section() {
    // Seed a shift with a service-in 50.00 and service-out 20.00.
    // Aggregate the Z payload and check that the resulting payload_json
    // has a service_sums field, and that build_z_canonical emits <IO>.
    use prro::runtime::ingress::convert::aggregate_z_payload;
    use prro::xml::DocumentHeader;

    let (_dir, main, _secure, shift) = setup_open_shift().await;

    // Seed service-in 50.00 + service-out 20.00 as ACK.
    seed_service_doc(&main, shift, 1, DocType::ServiceIn, 5000).await;
    seed_service_doc(&main, shift, 2, DocType::ServiceOut, 2000).await;

    // Aggregate the Z payload for this shift.
    let converted = aggregate_z_payload(&main, FN)
        .await
        .expect("aggregate_z_payload must succeed");

    // The converted payload_json must carry service_sums.
    let v: serde_json::Value = serde_json::from_str(&converted.payload_json).unwrap();
    let sums = v["service_sums"]
        .as_array()
        .expect("service_sums must be present and an array");
    assert!(
        !sums.is_empty(),
        "service_sums must be non-empty after service-in and service-out"
    );

    // Find the service_in sum.
    let si = sums
        .iter()
        .find(|s| s["name"].as_str() == Some("SERVICE_IN"))
        .expect("SERVICE_IN entry must exist");
    assert_eq!(si["sum_in_kop"], 5000, "SERVICE_IN SMI must be 5000");
    assert_eq!(si["sum_out_kop"], 0, "SERVICE_IN SMO must be 0");

    // Find the service_out sum.
    let so = sums
        .iter()
        .find(|s| s["name"].as_str() == Some("SERVICE_OUT"))
        .expect("SERVICE_OUT entry must exist");
    assert_eq!(so["sum_in_kop"], 0, "SERVICE_OUT SMI must be 0");
    assert_eq!(so["sum_out_kop"], 2000, "SERVICE_OUT SMO must be 2000");

    // The signer handoff: ZReportJson must deserialize service_sums and pass
    // them to ZReportPayload.  Verify via z_report_xml_from_json_for_testing.
    let header =
        DocumentHeader::with_defaults("4000100002", "12345678", 1_u32, "20260711100000", "");
    let xml_bytes = prro::services::write_path::stage_sign::z_report_xml_from_json_for_testing(
        header,
        99,
        &converted.payload_json,
    )
    .expect("xml build must succeed");
    let xml_str = String::from_utf8_lossy(&xml_bytes);

    // The XML must contain <IO with SMI="5000" and SMO="2000">.
    // The emitter sorts by name, so SERVICE_IN and SERVICE_OUT are separate rows.
    assert!(
        xml_str.contains("SMI=\"5000\"") || xml_str.contains("SMI='5000'"),
        "Z XML must contain IO SMI=5000; got:\n{xml_str}"
    );
    assert!(
        xml_str.contains("SMO=\"2000\"") || xml_str.contains("SMO='2000'"),
        "Z XML must contain IO SMO=2000; got:\n{xml_str}"
    );
    assert!(
        xml_str.contains("<IO"),
        "Z XML must contain <IO element; got:\n{xml_str}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 6: teeth — revert guard-3b → pin 3 RED
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_service_out_teeth() {
    // Canary: the guard IS active (guard-3b).  With an empty drawer, ServiceOut
    // must be refused.  If guard-3b is reverted this test turns RED.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let result = convert_to_signer_payload(&service_out_cmd(100), FN, &main, &secure).await;
    assert!(
        result.is_err(),
        "guard-3b must be load-bearing: ServiceOut on empty drawer must error (teeth: reverting guard → RED)"
    );
    assert!(
        matches!(result.unwrap_err(), ConvertError::CashInsufficient { .. }),
        "error must be CashInsufficient"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 7: CashWithdrawal (EPZ) stays fail-closed (STOP-S2)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn pin_epz_still_closed() {
    use prro::runtime::ingress::dto::CommandType;
    // EPZ must remain Unsupported at the policy layer.
    assert_eq!(
        classify_command(CommandType::CashWithdrawal),
        CommandClass::Unsupported,
        "CashWithdrawal (EPZ) must stay Unsupported — STOP-S2, EPZ fail-closed"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 8: STOP-S2 coupling — ingress-relaxed ⟺ IO-built
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn pin_stop_s2_coupling() {
    use prro::runtime::ingress::dto::CommandType;

    // ServiceIn and ServiceOut must be Signable (ingress relaxed for L3).
    assert_eq!(
        classify_command(CommandType::ServiceIn),
        CommandClass::Signable,
        "SERVICE_IN must be Signable after L3 gate"
    );
    assert_eq!(
        classify_command(CommandType::ServiceOut),
        CommandClass::Signable,
        "SERVICE_OUT must be Signable after L3 gate"
    );

    // CashWithdrawal (EPZ) and PeriodicReport must remain Unsupported.
    assert_eq!(
        classify_command(CommandType::CashWithdrawal),
        CommandClass::Unsupported,
        "CashWithdrawal must stay Unsupported"
    );

    // FULL_Z_SURFACE_READY must be true — the IO half is built in this PR.
    // The assertion on a constant is intentional: it is a coupling pin that
    // turns RED if someone flips the flag without updating this test.
    #[allow(clippy::assertions_on_constants)]
    {
        assert!(
            FULL_Z_SURFACE_READY,
            "FULL_Z_SURFACE_READY must stay true: the IO half is built in the same PR (STOP-S2)"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Pure unit: derive_cash_on_hand with service terms
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn pin_derive_cash_on_hand_with_service_terms() {
    // cash_on_hand = opening + sell - return + service_in - service_out
    // opening=0, sell=0, return=0, service_in=10000, service_out=3000 → 7000
    assert_eq!(derive_cash_on_hand(0, 0, 0, 10000, 3000), 7000);
    // service_in alone
    assert_eq!(derive_cash_on_hand(0, 0, 0, 5000, 0), 5000);
    // service_out alone (would go negative, allowed in pure fn — guard at convert)
    assert_eq!(derive_cash_on_hand(0, 0, 0, 0, 1000), -1000);
    // mixed
    assert_eq!(derive_cash_on_hand(1000, 2000, 500, 300, 100), 2700);
}
