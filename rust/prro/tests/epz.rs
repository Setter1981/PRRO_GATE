//! EPZ — видача готівки за ЕПЗ (cash advance / cashback against a card).
//!
//! RED-first pins (V0) for the EPZ increment.  Each pin is written BEFORE the
//! production wiring and observed to fail first.
//!
//! ## Wire reality (COORDINATOR-CONFIRMED, corrects EPZ_DOSSIER §2)
//! Our production ships the COMPACT `<RQ><DAT><C T=..>` dialect (0 `operationtype`
//! refs).  WebCheck's verbose `<check operationtype='-8'>` is the COM-input form;
//! `StringXML.DealCheck`/`num17=Math.Abs(-8)=8` transforms it to the DPS wire
//! `<C T='8'>`.  So EPZ's DPS wire = `<C T='8'>` (like `<C T='2'>` service,
//! `<C T='108'>` shift-open, `<C T='0/1'>` sell/return).  No tax `<TX>` on the good
//! (StringXML.cs:1424-1427 skips tax for -8).  Card `<M T='0'>` carries the slip
//! (PA/PB/PC/PD/PE/PF/PSNM/RRN); cash-out is a LEDGER effect only (no cash `<M>`).
//!
//! ## Pins
//! 1. `pin_epz_wire_c_type_8`               — signed body has `<C T='8'>` + fixed good NM + card `<M>`
//! 2. `pin_epz_over_drawer_refused_pre_inbox` — sum>cash → 422 CashInsufficient, row-less (guard-3c)
//! 3. `pin_epz_ledger_subtracts`            — EPZ ACK sum X → cash drops by X (−epz_out)
//! 4. `pin_z_carries_epz_section` ★         — live Z after EPZ carries `<EPZ EPC EPCS='0' EPSM>`
//! 5. `pin_epz_z_quiescence_blocks`         — a non-terminal EPZ doc blocks Z-close
//! 6. `pin_epz_paymentid_lt_2_refused`      — paymentid<2 → 422 (card-form-only)
//! 7. `pin_epz_policy_signable`             — EPZ CommandType → Signable
//! 8. `pin_epz_teeth`                       — revert guard-3c → pin 2 RED

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
use prro::services::cash_ledger::{aggregate_shift_epz, cash_on_hand_for_fn, derive_cash_on_hand};
use sqlx::SqlitePool;

const FN: &str = "4000100003";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers (mirror l3_service_io.rs, scoped to this module)
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

    // Cash slot (pay_index 1) + a card slot (pay_index 2, non-cash) for EPZ.
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
    pm_insert(
        &secure,
        &NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index: 2,
            name: "Картка".into(),
            iscash: false,
        },
    )
    .await
    .expect("seed card pm");

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

/// Seed a cash SERVICE_IN doc as ACK so the drawer has cash (for guard-3c setup).
async fn seed_service_in(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    amount_kop: i64,
) -> DocumentId {
    let payload = serde_json::json!({
        "schema_version": "1.0",
        "amount_kop": amount_kop,
        "name": "SERVICE_IN",
    })
    .to_string();
    seed_doc(
        pool,
        shift,
        lnd,
        DocType::ServiceIn,
        amount_kop,
        payload,
        DocState::Ack,
    )
    .await
}

/// Seed an EPZ doc in an arbitrary state with `{sum_kop}` payload.
async fn seed_epz_doc(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    sum_kop: i64,
    state: DocState,
) -> DocumentId {
    let payload = serde_json::json!({
        "schema_version": "1.0",
        "sum_kop": sum_kop,
    })
    .to_string();
    seed_doc(
        pool,
        shift,
        lnd,
        DocType::CashAdvanceEpz,
        sum_kop,
        payload,
        state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn seed_doc(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    doc_type: DocType,
    amount_kop: i64,
    payload_json: String,
    state: DocState,
) -> DocumentId {
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
        payload_json,
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
        .bind(state)
        .bind(id)
        .execute(pool)
        .await
        .expect("set state");
    id
}

/// Build an EPZ command with cash-out `sum` (kop) and `paymentid` card slot.
/// The card requisites ride on the single card `CanonicalPayment.acquirer_slip`.
fn epz_cmd(sum_kop: u64, payment_form_index: u8) -> CanonicalCommand {
    let json = format!(
        r#"{{
            "schema_version": "1.0",
            "fiscal_number": "{FN}",
            "command_type": "CASH_ADVANCE_EPZ",
            "idempotency_key": "epz-1",
            "cashier_id": null,
            "department": null,
            "return_check_number": null,
            "payload": {{
                "direction": "SALE",
                "goods": [],
                "payments": [
                    {{
                        "type": "CASHLESS_1",
                        "amount_kopecks": {sum_kop},
                        "acquirer_slip": {{
                            "payment_form_index": {payment_form_index},
                            "merchant_id": "M-1",
                            "terminal_id": "T-1",
                            "operation_type": "PURCHASE",
                            "pan": "**** 4242",
                            "approval_code": "APPR1",
                            "payment_system": "Visa",
                            "transaction_code": "RRN-1",
                            "fee_kopecks": 0,
                            "cashier_signature_placeholder": false,
                            "cardholder_signature_placeholder": false
                        }}
                    }}
                ],
                "totals": {{"sale_kopecks":0,"return_kopecks":0}}
            }}
        }}"#
    );
    serde_json::from_str(&json).expect("parse EPZ cmd")
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 1 ★: EPZ signed body is the compact <C T='8'> form with fixed good + card <M>
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_epz_wire_c_type_8() {
    use prro::xml::DocumentHeader;
    // drawer = 200.00 so guard-3c passes; EPZ 50.00.
    let (_dir, main, secure, shift) = setup_open_shift().await;
    seed_service_in(&main, shift, 1, 20000).await;

    let converted = convert_to_signer_payload(&epz_cmd(5000, 2), FN, &main, &secure)
        .await
        .expect("EPZ within drawer must convert");

    // Drive the converted payload through the signer's XML builder (test seam).
    let header = DocumentHeader::with_defaults(FN, "12345678", 1_u32, "20260711100000", "");
    let xml_bytes = prro::services::write_path::stage_sign::epz_check_xml_from_json_for_testing(
        header,
        99,
        &converted.payload_json,
    )
    .expect("EPZ xml build must succeed");
    // The wire is cp1251-encoded; ASCII structure survives byte-for-byte, the
    // Cyrillic good-label does not (asserted on the UTF-8 payload_json below).
    let xml = String::from_utf8_lossy(&xml_bytes);

    // THE identity: compact `<C T='8'>` (NOT verbose operationtype='-8').
    assert!(
        xml.contains("<C T='8'>") || xml.contains("<C T=\"8\">"),
        "EPZ DPS wire must carry <C T='8'>; got:\n{xml}"
    );
    assert!(
        !xml.contains("operationtype"),
        "compact dialect must NOT carry operationtype; got:\n{xml}"
    );
    // Card payment leg <M ... T='0' ...>; NO tax <TX> on the good.
    assert!(
        xml.contains("<M ") && (xml.contains("T='0'") || xml.contains("T=\"0\"")),
        "EPZ must carry a card <M> leg with T='0'; got:\n{xml}"
    );
    assert!(
        !xml.contains("<TX "),
        "EPZ good carries no tax element (StringXML.cs:1424-1427 skip); got:\n{xml}"
    );
    // The good <P> element carries the operation sum in SM/PRC.
    assert!(
        xml.contains("SM='5000'") || xml.contains("SM=\"5000\""),
        "EPZ good/payment SM must be the sum 5000; got:\n{xml}"
    );

    // ── Card <M> requisites (INV-6 payload): present-when-set ──
    // Each acquirer-slip field maps (convert.rs:1158-1164) to a card <M> attr:
    //   merchant_id       "M-1"       → PA
    //   terminal_id       "T-1"       → PB
    //   operation_type    "PURCHASE"  → PC
    //   pan               "**** 4242" → PD
    //   approval_code     "APPR1"     → PE
    //   payment_system    "Visa"      → PSNM
    //   transaction_code  "RRN-1"     → RRN
    // The `epz_cmd` fixture populates all seven with non-empty ASCII values, so
    // every attr MUST appear name+value in the signed <M> (tag_attrs emits
    // double-quoted, ASCII survives cp1251).  Pinning each value KILLS the
    // `delete !` mutants on the `if !p.<attr>.is_empty()` emit guards — flipping
    // the guard would omit the set attr (or emit an empty one), failing here.
    for (attr, value) in [
        ("PA", "M-1"),
        ("PB", "T-1"),
        ("PC", "PURCHASE"),
        ("PD", "**** 4242"),
        ("PE", "APPR1"),
        ("PSNM", "Visa"),
        ("RRN", "RRN-1"),
    ] {
        assert!(
            xml.contains(&format!("{attr}='{value}'"))
                || xml.contains(&format!("{attr}=\"{value}\"")),
            "EPZ card <M> must carry {attr}='{value}' (present-when-set); got:\n{xml}"
        );
    }

    // The fixed cash-advance good label — asserted on the UTF-8 converted
    // payload_json (the cp1251-encoded wire mangles Cyrillic under from_utf8).
    assert!(
        converted.payload_json.contains(
            "ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ ДЕРЖАТЕЛЮ ЕЛЕКТРОННОГО ПЛАТІЖНОГО ЗАСОБУ"
        ),
        "EPZ good NM must be the fixed cash-advance label; got:\n{}",
        converted.payload_json
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 1b ★: EPZ card <M> OMITS empty slip requisites (no `PA=''` on the wire)
//
// The inverse of pin 1's present-when-set: when an acquirer-slip field is EMPTY
// the corresponding <M> attr must NOT appear at all (`emit_epz_check` only
// pushes it under `if !p.<attr>.is_empty()`).  This kills the OTHER half of the
// `delete !` mutants — flipping the guard would emit `PA=''` for the empty
// field (and drop it for a set one).  We drive the signer seam directly with an
// EpzJson-shaped payload whose PA/PC/RRN are empty and PB/PD/PE/PSNM set, so we
// pin BOTH the absent-empty AND the present-set directions in one wire.
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_epz_wire_omits_empty_slip_attrs() {
    use prro::xml::DocumentHeader;

    // EpzJson-shaped payload (stage_sign::EpzJson, snake_case, deny_unknown):
    // PA/PC/RRN empty → attr absent; PB/PD/PE/PSNM set → attr present.
    let payload_json = serde_json::json!({
        "schema_version": "1.0",
        "sum_kop": 5000,
        "code": "0",
        "name": "EPZ",
        "paymentid": 2,
        "pay_name": "Card",
        "pa": "",
        "pb": "T-1",
        "pc": "",
        "pd": "**** 4242",
        "pe": "APPR1",
        "psnm": "Visa",
        "rrn": ""
    })
    .to_string();

    let header = DocumentHeader::with_defaults(FN, "12345678", 1_u32, "20260711100000", "");
    let xml_bytes = prro::services::write_path::stage_sign::epz_check_xml_from_json_for_testing(
        header,
        99,
        &payload_json,
    )
    .expect("EPZ xml build must succeed");
    let xml = String::from_utf8_lossy(&xml_bytes);

    // Empty slip fields → attr ABSENT (no empty-valued attr on the card leg).
    for attr in ["PA", "PC", "RRN"] {
        assert!(
            !xml.contains(&format!("{attr}=''")) && !xml.contains(&format!("{attr}=\"\"")),
            "EPZ card <M> must OMIT empty {attr} (no {attr}=''); got:\n{xml}"
        );
        // Belt-and-suspenders: the attr name must not appear on the <M> line at
        // all when its slip field is empty (guards against a bare `PA=` form).
        assert!(
            !xml.contains(&format!(" {attr}=")),
            "EPZ card <M> must not emit the {attr} attribute when its slip field is empty; got:\n{xml}"
        );
    }
    // The set fields MUST still be present (guards the mutation that would drop
    // a set attr while emitting the empty ones).
    for (attr, value) in [
        ("PB", "T-1"),
        ("PD", "**** 4242"),
        ("PE", "APPR1"),
        ("PSNM", "Visa"),
    ] {
        assert!(
            xml.contains(&format!("{attr}='{value}'"))
                || xml.contains(&format!("{attr}=\"{value}\"")),
            "EPZ card <M> must carry set {attr}='{value}'; got:\n{xml}"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 2: EPZ over drawer → row-less 422 (guard-3c pre-inbox, INV-21 errCode-47)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_epz_over_drawer_refused_pre_inbox() {
    // drawer = 0.  EPZ(1.00) → CashInsufficient, row-less.
    let (_dir, main, secure, _shift) = setup_open_shift().await;

    let err = convert_to_signer_payload(&epz_cmd(100, 2), FN, &main, &secure)
        .await
        .expect_err("EPZ on empty drawer must be refused by guard-3c");

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

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&main)
            .await
            .unwrap();
    assert_eq!(count, 0, "guard-3c must be row-less (pre-inbox refusal)");
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 3: EPZ ACK subtracts from cash-on-hand (−epz_out)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_epz_ledger_subtracts() {
    // drawer 100.00 (service-in), EPZ 30.00 ACK → cash = 7000.
    let (_dir, main, _secure, shift) = setup_open_shift().await;
    seed_service_in(&main, shift, 1, 10000).await;
    seed_epz_doc(&main, shift, 2, 3000, DocState::Ack).await;

    let cash = cash_on_hand_for_fn(&main, FN).await.unwrap();
    assert_eq!(cash, 7000, "100.00 − 30.00 (EPZ) = 70.00 = 7000 kop");

    // aggregate_shift_epz must report the 3000 EPZ-out.
    let epz_out = aggregate_shift_epz(&main, FN, shift).await.unwrap();
    assert_eq!(epz_out, 3000, "aggregate_shift_epz must sum EPZ ACK docs");
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 3b (pure): derive_cash_on_hand carries the −epz_out term
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn pin_derive_cash_on_hand_with_epz_term() {
    // (opening, sell, ret, svc_in, svc_out, epz_out)
    // 0 + 0 − 0 + 10000 − 0 − 3000 = 7000
    assert_eq!(derive_cash_on_hand(0, 0, 0, 10000, 0, 3000), 7000);
    // EPZ alone drives negative in the pure fn (guard is at convert)
    assert_eq!(derive_cash_on_hand(0, 0, 0, 0, 0, 1000), -1000);
    // full mix
    assert_eq!(derive_cash_on_hand(1000, 2000, 500, 300, 100, 200), 2500);
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 4 ★: live Z after EPZ carries a non-empty <EPZ EPC EPCS='0' EPSM> section
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_z_carries_epz_section() {
    use prro::runtime::ingress::convert::aggregate_z_payload;
    use prro::xml::DocumentHeader;

    let (_dir, main, _secure, shift) = setup_open_shift().await;
    // drawer for realism + two EPZ ACKs totalling 70.00.
    seed_service_in(&main, shift, 1, 20000).await;
    seed_epz_doc(&main, shift, 2, 5000, DocState::Ack).await;
    seed_epz_doc(&main, shift, 3, 2000, DocState::Ack).await;

    let converted = aggregate_z_payload(&main, FN)
        .await
        .expect("aggregate_z_payload must succeed");

    // The converted payload_json must carry an epz object.
    let v: serde_json::Value = serde_json::from_str(&converted.payload_json).unwrap();
    let epz = &v["epz"];
    assert_eq!(epz["epc"], 2, "EPC = count of EPZ ops");
    assert_eq!(epz["epcs"], 0, "EPCS hardcoded 0 (byte-parity WebCheck)");
    assert_eq!(epz["epsm"], 7000, "EPSM = total EPZ sum (5000+2000)");

    // Signer handoff: the Z XML must emit <EPZ EPC='2' EPCS='0' EPSM='7000'>.
    let header = DocumentHeader::with_defaults(FN, "12345678", 1_u32, "20260711100000", "");
    let xml_bytes = prro::services::write_path::stage_sign::z_report_xml_from_json_for_testing(
        header,
        99,
        &converted.payload_json,
    )
    .expect("Z xml build must succeed");
    let xml = String::from_utf8_lossy(&xml_bytes);
    assert!(
        xml.contains("<EPZ "),
        "Z XML must contain <EPZ; got:\n{xml}"
    );
    assert!(
        xml.contains("EPC='2'") || xml.contains("EPC=\"2\""),
        "Z XML EPZ EPC=2; got:\n{xml}"
    );
    assert!(
        xml.contains("EPSM='7000'") || xml.contains("EPSM=\"7000\""),
        "Z XML EPZ EPSM=7000; got:\n{xml}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 5: a non-terminal EPZ doc blocks Z-close (z-quiescence, #192/P1 class)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_epz_z_quiescence_blocks() {
    use prro::db::repositories::fiscal_documents::list_shift_pending_receipts_for_z_quiescence;
    let (_dir, main, _secure, shift) = setup_open_shift().await;
    // A SIGNED (non-terminal) EPZ doc must appear in the z-quiescence blocker set.
    seed_epz_doc(&main, shift, 1, 3000, DocState::Signed).await;

    let blockers = list_shift_pending_receipts_for_z_quiescence(&main, FN, shift)
        .await
        .unwrap();
    assert_eq!(
        blockers.len(),
        1,
        "a non-terminal EPZ doc must block Z-quiescence (#192/P1 class); got {blockers:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 6: paymentid < 2 → 422 (card-form only; errCode-94 analog)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_epz_paymentid_lt_2_refused() {
    let (_dir, main, secure, shift) = setup_open_shift().await;
    seed_service_in(&main, shift, 1, 20000).await; // drawer OK so we isolate the paymentid gate

    let err = convert_to_signer_payload(&epz_cmd(5000, 1), FN, &main, &secure)
        .await
        .expect_err("EPZ with paymentid<2 must be refused (card-form-only)");
    assert!(
        matches!(err, ConvertError::EpzPaymentIdTooLow { .. }),
        "expected EpzPaymentIdTooLow, got: {err:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 7: EPZ CommandType is Signable
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn pin_epz_policy_signable() {
    use prro::runtime::ingress::dto::CommandType;
    assert_eq!(
        classify_command(CommandType::CashAdvanceEpz),
        CommandClass::Signable,
        "EPZ (CashAdvanceEpz) must be Signable after this increment"
    );
    // The fail-closed placeholder CashWithdrawal must STAY Unsupported.
    assert_eq!(
        classify_command(CommandType::CashWithdrawal),
        CommandClass::Unsupported,
        "CashWithdrawal placeholder must stay Unsupported"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 8: teeth — revert guard-3c → pin 2 goes RED
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_epz_teeth() {
    // Canary: guard-3c is load-bearing.  Empty drawer + EPZ must error.
    let (_dir, main, secure, _shift) = setup_open_shift().await;
    let result = convert_to_signer_payload(&epz_cmd(100, 2), FN, &main, &secure).await;
    assert!(
        result.is_err(),
        "guard-3c must be load-bearing: EPZ on empty drawer must error (teeth: reverting guard → RED)"
    );
    assert!(
        matches!(result.unwrap_err(), ConvertError::CashInsufficient { .. }),
        "error must be CashInsufficient"
    );
}
