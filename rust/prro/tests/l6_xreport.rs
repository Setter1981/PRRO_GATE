//! L6 — X-report (поточний звіт / mid-shift snapshot) integration tests.
//!
//! RED-first: every pin below was written and observed to FAIL before the
//! `handle_x_report` wiring landed (the ReadOnly arm returned a hard 422
//! `READ_ONLY_COMMAND`).
//!
//! ## What X-report is
//! A local-only, SIDE-EFFECT-FREE read of the current open shift's turnover.
//! It is NOT a fiscal document: never sent to DPS, consumes no fiscal number,
//! advances no chain, writes no row, closes no shift. The read-only sibling of
//! the Z-report, reusing the SAME aggregation (`aggregate_z_payload`) as a pure
//! snapshot, PLUS cash-on-hand.
//!
//! ## Pins
//! 1. `pin_x_report_positive`          — open shift + SELL/RETURN/service-io →
//!    200 XReportPayload whose turnover matches the seeded docs + cash-on-hand.
//! 2. `pin_x_report_side_effect_free` ★ — after the call: 0 new inbox rows,
//!    0 new fiscal_documents rows, next_lnd unchanged, MAC seed unchanged,
//!    shift state unchanged. THE core invariant (the teeth).
//! 3. `pin_x_report_no_open_shift`     — no open shift → 422 NO_OPEN_SHIFT, row-less.
//! 4. `pin_x_report_bimodal`           — OpenedLocalPendingDrain shift with an
//!    OFFLINE_LOCAL_ACK doc returns the same turnover from the durable ledger.

use prro::db::models::enums::Protocol;
use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::DriverId;
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::open_pool;
use prro::db::open_secure_pool;
use prro::db::repositories::fiscal_documents as fd;
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::node_state;
use prro::db::repositories::payment_methods::{insert as pm_insert, NewPaymentMethod};
use prro::db::repositories::shifts;
use prro::runtime::ingress::dto::CanonicalCommand;
use prro::runtime::ingress::handler::{handle_command, IngressBody};
use prro::runtime::ingress::seam::UnimplementedWritePath;
use sqlx::SqlitePool;

const FN: &str = "4000100006";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers (mirror l3_service_io.rs; scoped to this module)
// ──────────────────────────────────────────────────────────────────────────────

/// Seed an FN + a shift in the given `shift_state`, wire it as the current
/// open shift on an `Online` (or `Offline` for bimodal) node.
async fn setup_shift(
    node_mode: NodeMode,
    shift_state: ShiftState,
) -> (tempfile::TempDir, SqlitePool, SqlitePool, ShiftId) {
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
    let fs_mode = if node_mode == NodeMode::Online {
        "ONLINE"
    } else {
        "OFFLINE"
    };
    with_immediate(&main, move |tx| {
        Box::pin(async move {
            shifts::insert_created_tx(tx, shift, FN, fs_mode, "cashier-1", 0)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .expect("insert shift");

    // Walk the shift to the requested (open) state.
    let path: &[(ShiftState, ShiftState)] = match shift_state {
        ShiftState::Opened => &[
            (ShiftState::Created, ShiftState::Opening),
            (ShiftState::Opening, ShiftState::Opened),
        ],
        ShiftState::OpenedLocalPendingDrain => &[
            (ShiftState::Created, ShiftState::Opening),
            (ShiftState::Opening, ShiftState::OpenedLocalPendingDrain),
        ],
        other => panic!("setup_shift only supports open states, got {other:?}"),
    };
    for &(from, to) in path {
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

    node_state::upsert_initial(&main, FN, node_mode, shift_state, 1)
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

/// Seed a SELL (or RETURN) receipt with ONE cash payment leg, in the given
/// `state` (ACK online / OFFLINE_LOCAL_ACK offline).  Payment shape mirrors the
/// converted CheckJson the aggregation reads: `{payments:[{name,sum_kop,type_code}]}`.
async fn seed_receipt(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    doc_type: DocType,
    cash_kop: i64,
    state: DocState,
) -> DocumentId {
    let payload = serde_json::json!({
        "schema_version": "1.0",
        "payments": [{"name": "Готівка", "sum_kop": cash_kop, "type_code": "0"}],
        "items": [{"tax_group_1": 1, "sum_kop": cash_kop}],
    })
    .to_string();
    let fs_mode = if state == DocState::OfflineLocalAck {
        "OFFLINE"
    } else {
        "ONLINE"
    };
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
        fs_mode,
        business_ts: "2026-07-11T10:00:00Z".into(),
        total_sum_kop: Some(cash_kop),
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
        .bind(state)
        .bind(id)
        .execute(pool)
        .await
        .expect("set state");
    id
}

/// Seed a service-in / service-out doc (mirrors l3_service_io helper).
async fn seed_service_doc(
    pool: &SqlitePool,
    shift: ShiftId,
    lnd: i64,
    doc_type: DocType,
    amount_kop: i64,
    state: DocState,
) -> DocumentId {
    let payload = serde_json::json!({
        "schema_version": "1.0",
        "amount_kop": amount_kop,
        "name": if doc_type == DocType::ServiceIn { "SERVICE_IN" } else { "SERVICE_OUT" },
    })
    .to_string();
    let fs_mode = if state == DocState::OfflineLocalAck {
        "OFFLINE"
    } else {
        "ONLINE"
    };
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
        fs_mode,
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
        .bind(state)
        .bind(id)
        .execute(pool)
        .await
        .expect("set state");
    id
}

fn drv() -> DriverId {
    DriverId::new("drv-1").unwrap()
}

fn xreport_cmd(idem: &str) -> CanonicalCommand {
    let json = format!(
        r#"{{"schema_version":"1.0","fiscal_number":"{FN}","command_type":"X_REPORT",
            "idempotency_key":"{idem}","cashier_id":null,"department":null,
            "return_check_number":null,
            "payload":{{"direction":"SALE","totals":{{"sale_kopecks":0,"return_kopecks":0}}}}}}"#
    );
    serde_json::from_str(&json).expect("parse X_REPORT cmd")
}

async fn call_x_report(
    main: &SqlitePool,
    secure: &SqlitePool,
    idem: &str,
) -> prro::runtime::ingress::handler::IngressResponse {
    let wp = UnimplementedWritePath;
    handle_command(
        &xreport_cmd(idem),
        FN,
        drv(),
        Protocol::Rest,
        main,
        secure,
        &wp,
    )
    .await
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_next_lnd(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT next_lnd FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_seed(pool: &SqlitePool) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn read_shift_state(pool: &SqlitePool) -> ShiftState {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 1 — positive: 200 + XReportPayload matching the seeded turnover
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_x_report_positive() {
    let (_dir, main, secure, shift) = setup_shift(NodeMode::Online, ShiftState::Opened).await;

    // Seed turnover: SELL cash 150.00, RETURN cash 40.00, service-in 20.00.
    seed_receipt(&main, shift, 1, DocType::Sell, 15000, DocState::Ack).await;
    seed_receipt(&main, shift, 2, DocType::Return, 4000, DocState::Ack).await;
    seed_service_doc(&main, shift, 3, DocType::ServiceIn, 2000, DocState::Ack).await;

    let r = call_x_report(&main, &secure, "x-pos").await;
    assert_eq!(
        r.http_status, 200,
        "X-report on an open shift must be 200, not 422"
    );

    let payload = match r.body {
        IngressBody::XReport(p) => p,
        other => panic!("expected XReport body, got {other:?}"),
    };

    assert_eq!(payload.fiscal_number, FN);

    // cash-on-hand = 15000 (SELL) - 4000 (RETURN) + 2000 (service-in) = 13000.
    assert_eq!(
        payload.cash_on_hand_kop, 13000,
        "cash-on-hand must reflect SELL - RETURN + service-in"
    );

    // Turnover carries the aggregated payform + counts + service_sums.
    let t = &payload.turnover;
    let sell_count = t["sell_count"].as_i64().expect("sell_count");
    let return_count = t["return_count"].as_i64().expect("return_count");
    assert_eq!(sell_count, 1, "one SELL");
    assert_eq!(return_count, 1, "one RETURN");

    // service_sums must carry the SERVICE_IN row (20.00 = 2000 kop).
    let sums = t["service_sums"].as_array().expect("service_sums array");
    let si = sums
        .iter()
        .find(|s| s["name"].as_str() == Some("SERVICE_IN"))
        .expect("SERVICE_IN row");
    assert_eq!(si["sum_in_kop"], 2000);

    // Payments (cash) must aggregate: SMI 15000 (SELL) SMO 4000 (RETURN cash-out).
    let payments = t["payments"].as_array().expect("payments array");
    let cash = payments
        .iter()
        .find(|p| p["type_code"].as_str() == Some("0"))
        .expect("cash payform row");
    assert_eq!(cash["sum_in_kop"], 15000, "cash SMI = SELL cash");
    assert_eq!(cash["sum_out_kop"], 4000, "cash SMO = RETURN cash");
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 2 ★ — side-effect-free (THE core invariant / the teeth)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_x_report_side_effect_free() {
    let (_dir, main, secure, shift) = setup_shift(NodeMode::Online, ShiftState::Opened).await;
    seed_receipt(&main, shift, 1, DocType::Sell, 15000, DocState::Ack).await;

    let inbox_before = count(&main, "ingress_inbox").await;
    let docs_before = count(&main, "fiscal_documents").await;
    let lnd_before = read_next_lnd(&main).await;
    let seed_before = read_seed(&main).await;
    let shift_before = read_shift_state(&main).await;

    // Call twice — idempotent read, still zero side effects.
    let r1 = call_x_report(&main, &secure, "x-sef-1").await;
    let r2 = call_x_report(&main, &secure, "x-sef-2").await;
    assert_eq!(r1.http_status, 200);
    assert_eq!(r2.http_status, 200);

    assert_eq!(
        count(&main, "ingress_inbox").await,
        inbox_before,
        "X-report must NOT enter ingress_inbox"
    );
    assert_eq!(
        count(&main, "fiscal_documents").await,
        docs_before,
        "X-report must NOT create a fiscal_documents row"
    );
    assert_eq!(
        read_next_lnd(&main).await,
        lnd_before,
        "X-report must NOT consume an lnd"
    );
    assert_eq!(
        read_seed(&main).await,
        seed_before,
        "X-report must NOT advance the MAC seed"
    );
    assert_eq!(
        read_shift_state(&main).await,
        shift_before,
        "X-report must NOT transition shift state"
    );

    // Idempotent: both bodies identical.
    match (r1.body, r2.body) {
        (IngressBody::XReport(a), IngressBody::XReport(b)) => {
            assert_eq!(a.cash_on_hand_kop, b.cash_on_hand_kop);
            assert_eq!(a.turnover, b.turnover);
        }
        _ => panic!("both must be XReport bodies"),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 3 — no open shift → 422 NO_OPEN_SHIFT, row-less
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_x_report_no_open_shift() {
    // Seed an FN + Online node with NO current_shift_id.
    let dir = tempfile::tempdir().unwrap();
    let main = open_pool(&dir.path().join("main.db")).await.unwrap();
    let secure = open_secure_pool(&dir.path().join("secure.db"))
        .await
        .unwrap();
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
    .unwrap();
    node_state::upsert_initial(&main, FN, NodeMode::Online, ShiftState::Closed, 1)
        .await
        .unwrap();

    let r = call_x_report(&main, &secure, "x-noshift").await;
    assert_eq!(r.http_status, 422, "no open shift → 422");
    match r.body {
        IngressBody::Error(e) => assert_eq!(e.error_code, "NO_OPEN_SHIFT"),
        other => panic!("expected NO_OPEN_SHIFT error, got {other:?}"),
    }
    assert_eq!(
        count(&main, "ingress_inbox").await,
        0,
        "row-less refusal — no inbox row"
    );
    assert_eq!(
        count(&main, "fiscal_documents").await,
        0,
        "row-less refusal — no doc row"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Pin 4 — bimodal: OpenedLocalPendingDrain + OFFLINE_LOCAL_ACK doc
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pin_x_report_bimodal_offline() {
    let (_dir, main, secure, shift) =
        setup_shift(NodeMode::Offline, ShiftState::OpenedLocalPendingDrain).await;

    // Seed an OFFLINE_LOCAL_ACK SELL (cash 88.00) — durable ledger, not yet drained.
    seed_receipt(
        &main,
        shift,
        1,
        DocType::Sell,
        8800,
        DocState::OfflineLocalAck,
    )
    .await;

    let r = call_x_report(&main, &secure, "x-bimodal").await;
    assert_eq!(
        r.http_status, 200,
        "X-report on OpenedLocalPendingDrain must aggregate the durable ledger"
    );
    let payload = match r.body {
        IngressBody::XReport(p) => p,
        other => panic!("expected XReport body, got {other:?}"),
    };
    assert_eq!(
        payload.cash_on_hand_kop, 8800,
        "OFFLINE_LOCAL_ACK doc must count in the offline turnover snapshot"
    );
    let t = &payload.turnover;
    assert_eq!(t["sell_count"].as_i64(), Some(1), "the offline SELL counts");
}
