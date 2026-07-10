//! A′.3 O2 — PILOT offline FULL DRILL e2e THROUGH THE LIVE BINDING + LIVE DOOR.
//!
//! The canonical Pattern C round-trip on a clean prod config, driven end-to-end
//! by the LIVE operator door (`admin::go_offline` / `go_online`, now flipped
//! live) + the LIVE `production_write_path` binding + the LIVE drain
//! (`App::drain_offline_backlog_scheduled`, the same entry the supervisor loop
//! calls when `supervisor.enabled = true`):
//!
//!   boot → online SHIFT_OPEN → GO_OFFLINE (live door) → offline SELL + RETURN →
//!   GO_ONLINE (live door) → drain → convergence
//!
//! Convergence proof: both offline docs OFFLINE_LOCAL_ACK → … → ACK, the offline
//! session Open → CLOSED, node GOING_ONLINE → ONLINE, and the ledger scan CLEAN.
//!
//! EDGE-SET (honest, per O2 ruling condition 3): the shift is opened ONLINE and
//! STAYS `Opened` throughout — the drill exercises doc-drain (OLA→ACK) + session
//! close + mode convergence, but NOT the offline shift-lifecycle edges 2/7/9
//! (offline shift OPEN/CLOSE → O3). The drain shift-finalize edges 5/13 remain
//! covered by the existing `app_drain_offline_backlog` tests.
//!
//! TEETH: revert the flag (`FULL_OFFLINE_SURFACE_READY=false`) → the live door
//! REDs (GO_OFFLINE refuses) and the drill REDs at the GO_OFFLINE leg.

mod common;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::app::App;
use prro::config::AppConfig;
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::RequestId;
use prro::db::repositories::ingress_inbox::{
    self as inbox, InboxInsertOutcome, InboxRow, NewInboxEntry,
};
use prro::db::repositories::{fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::ingress::inline_binding::production_write_path_with_clock;
use prro::runtime::ingress::seam::{FiscalOutcome, WritePathEntry};
use prro::services::reconciliation::runtime::RuntimeView;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::ScheduledDrainOutcome;
use sqlx::SqlitePool;

use common::{ack, det_signing_ctx, det_signing_ctx_for, StubDpsChannel};

const FN: &str = "4000000019";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

fn cfg_toml(db_path: &str) -> String {
    format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8445"
"#
    )
}

async fn boot_app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("pilot_drill.db");
    std::mem::forget(dir);
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    App::boot(cfg).await.unwrap()
}

struct FixtureLoader;
#[async_trait]
impl OperatorKeyLoader for FixtureLoader {
    async fn load(
        &self,
        operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        let mut ctx = det_signing_ctx_for(operator_id);
        ctx.session = SigningSession::new_for_test(
            operator_id.to_string(),
            [7u8; 32],
            FIXTURE_CERT_DER.to_vec(),
        );
        Ok(ctx)
    }
}

fn fn_config() -> fn_cfg::NewFnConfig {
    fn_cfg::NewFnConfig {
        fiscal_number: FN.into(),
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
    }
}

async fn build_registry(app: &App, dps: Arc<dyn DpsChannel>) -> BindingsRegistry {
    fn_cfg::insert(app.db(), &fn_config())
        .await
        .expect("seed FN config");
    ops_repo::insert(
        app.db_secure(),
        &ops_repo::NewOperator {
            operator_id: "OP-1".into(),
            fiscal_number: FN.into(),
            name: "Cashier".into(),
            key_path: "/tmp/k1.dat".into(),
            key_pass_enc: Coding::encode(b"secret1").expect("encode test password"),
        },
    )
    .await
    .expect("seed operator");
    BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, &FixtureLoader)
        .await
        .expect("build_from_db")
}

async fn seed_boot_baseline(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, NULL, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(NodeMode::Online)
    .bind(ShiftState::Closed)
    .execute(pool)
    .await
    .unwrap();
}

fn kvt1(sfn: &str) -> CheckAck {
    CheckAck {
        id: sfn.into(),
        id_sign: vec![],
        data_sign: vec![0xDE, 0xAD, 0xBE, 0xEF],
    }
}

/// The registry DPS acks ONLY the online SHIFT_OPEN (offline SELL/RETURN never
/// touch DPS). The drain uses a SEPARATE RuntimeView DPS (see `drain_carriers`).
fn shift_open_only_dps() -> Arc<dyn DpsChannel> {
    Arc::new(
        StubDpsChannel::with_queue(vec![Ok(ack("DPS-SFN-OPEN"))])
            .with_last_chk_queue(vec![Ok(kvt1("DPS-SFN-OPEN"))]),
    )
}

/// Registry DPS acking each ONLINE binding doc in `sfns` order (send_chk +
/// last_chk each) — the combined gate drives SHIFT_OPEN + one online SELL + Z
/// through the binding.
fn online_dps(sfns: &[&str]) -> Arc<dyn DpsChannel> {
    let sends = sfns.iter().map(|s| Ok(ack(s))).collect::<Vec<_>>();
    let lasts = sfns.iter().map(|s| Ok(kvt1(s))).collect::<Vec<_>>();
    Arc::new(StubDpsChannel::with_queue(sends).with_last_chk_queue(lasts))
}

fn entry(op: &str, payload: &str, idem: &str, total: Option<i64>) -> NewInboxEntry {
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(payload.as_bytes()).into();
    NewInboxEntry {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: op.into(),
        idempotency_key: idem.into(),
        payload_json: payload.into(),
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-07-07T12:00:00Z".into()),
        total_sum_kop: total,
    }
}

async fn drive(
    wp: &dyn WritePathEntry,
    pool: &SqlitePool,
    entry: NewInboxEntry,
) -> Result<FiscalOutcome, prro::runtime::ingress::seam::FiscalError> {
    let row: InboxRow = match inbox::insert(pool, &entry).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await
}

// ─── drain carriers (separate DPS: send_chk + last_chk for the 2 offline docs) ─

struct DrainCarriers {
    dps: Arc<StubDpsChannel>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn drain_carriers() -> DrainCarriers {
    // B10: a drained offline session now also carries the two boundary docs
    // (DocType=9 BEGIN minted at the first offline sell + DocType=10 END minted
    // at drain finalize).  So the drain wire sequence is up to 4 send_chk +
    // 4 last_chk (BEGIN + content×N + END).  Provide a generous queue.
    let sends: Vec<_> = (0..4).map(|i| Ok(ack(&format!("DPS-DRAIN-{i}")))).collect();
    let lasts: Vec<_> = (0..4)
        .map(|i| {
            Ok(CheckAck {
                id: format!("DPS-DRAIN-{i}"),
                id_sign: vec![],
                data_sign: vec![(i as u8).wrapping_add(0xA0); 32],
            })
        })
        .collect();
    DrainCarriers {
        dps: Arc::new(StubDpsChannel::with_queue(sends).with_last_chk_queue(lasts)),
        signing_ctx: det_signing_ctx(),
        fn_sign: CheckSignBlob(vec![0xAB, 0xCD]),
    }
}

fn drain_view(c: &DrainCarriers) -> RuntimeView<'_> {
    RuntimeView {
        dps: c.dps.as_ref(),
        signing_ctx: &c.signing_ctx,
        fn_sign: &c.fn_sign,
    }
}

// ─── probes ─────────────────────────────────────────────────────────────────

async fn node_row(pool: &SqlitePool) -> (String, String) {
    sqlx::query_as("SELECT mode, shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn doc_count_in_state(pool: &SqlitePool, state: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND state = ?",
    )
    .bind(FN)
    .bind(state)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn offline_session_state(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar("SELECT state FROM offline_sessions WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_optional(pool)
        .await
        .unwrap()
}

// ════════════════════════════════════════════════════════════════════════
// PILOT offline FULL DRILL
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pilot_offline_full_drill_go_offline_sell_return_go_online_drain_converges() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(
        app.clone(),
        Arc::new(registry),
        std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339(
            "2026-07-07T12:30:00Z",
        )),
    );

    // ─── 1) online SHIFT_OPEN (edge 3, live) ──────────────────────────────
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-drill-OPEN", None),
    )
    .await
    .expect("online SHIFT_OPEN must ACK");
    assert_eq!(open.document_state, DocState::Ack);
    assert_eq!(shift_state(app.db()).await, "OPENED");
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 2) GO_OFFLINE through the LIVE door (mode flip + atomic session) ──
    let go_off = prro::admin::go_offline(app.db(), FN, "operator net drop")
        .await
        .expect("the live door: GO_OFFLINE");
    assert_eq!(go_off.fiscal_number, FN);
    assert_eq!(node_row(app.db()).await.0, "OFFLINE");
    assert_eq!(
        offline_session_state(app.db()).await.as_deref(),
        Some("OPEN")
    );

    // ─── 3) seed real codes + offline SELL + RETURN via the live binding ──
    // B8-1: seed with explicit dps_code strings so acquire_code_tx (which now
    // requires dps_code IS NOT NULL) can drain them.
    let codes_a: Vec<String> = (0..6).map(|i| format!("DRILL-A-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes_a)
        .await
        .expect("seed codes");
    let sell = drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-drill-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL");
    assert_eq!(sell.document_state, DocState::OfflineLocalAck);
    let ret = drive(
        &*write_path,
        app.db(),
        entry("RETURN", SELL_PAYLOAD, "idem-drill-RETURN", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline RETURN");
    assert_eq!(ret.document_state, DocState::OfflineLocalAck);
    // B10: 3 OLA docs pre-drain — the lazy DocType=9 BEGIN (minted at the first
    // offline SELL) + the SELL + the RETURN.
    assert_eq!(doc_count_in_state(app.db(), "OFFLINE_LOCAL_ACK").await, 3);
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 4) GO_ONLINE through the LIVE door (mode → GOING_ONLINE) ─────────
    prro::admin::go_online(app.db(), FN, "dps connectivity restored")
        .await
        .expect("the live door: GO_ONLINE");
    assert_eq!(node_row(app.db()).await.0, "GOING_ONLINE");

    // ─── 5) DRAIN (the entry the supervisor loop calls) → convergence ─────
    let carriers = drain_carriers();
    let view = drain_view(&carriers);
    let outcome = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("drain must run for a GOING_ONLINE FN");
    let summary = match outcome {
        ScheduledDrainOutcome::Ran(s) => s,
        ScheduledDrainOutcome::SkippedBackoff { .. } => panic!("first drain tick must run"),
    };
    assert_eq!(
        summary.backlog_size_before(),
        3,
        "B10: 3 OLA docs in the backlog — BEGIN + SELL + RETURN"
    );
    assert_eq!(
        summary.advanced_to_ack(),
        3,
        "all 3 content-cohort offline docs converge to ACK (END is a finalize \
         precondition, not counted in the cohort)"
    );
    assert!(
        summary.finalized(),
        "all content Acked + END Acked → finalize Eligible"
    );
    drop(carriers);

    // ─── 6) convergence assertions ────────────────────────────────────────
    assert_eq!(
        doc_count_in_state(app.db(), "ACK").await,
        5,
        "B10: SHIFT_OPEN + BEGIN + SELL + RETURN + END all ACK"
    );
    assert_eq!(
        doc_count_in_state(app.db(), "OFFLINE_LOCAL_ACK").await,
        0,
        "backlog drained"
    );
    let (mode, shift) = node_row(app.db()).await;
    assert_eq!(mode, "ONLINE", "node converged back to ONLINE");
    assert_eq!(
        shift, "OPENED",
        "shift stayed Opened (edge 2/7/9 not driven — O3)"
    );
    assert_eq!(shift_state(app.db()).await, "OPENED");
    assert_eq!(
        offline_session_state(app.db()).await.as_deref(),
        Some("CLOSED"),
        "session closed"
    );

    // Everything rests terminal + converged — the ledger scan is CLEAN.
    prro::db::invariant_scan::assert_clean(app.db()).await;
}

async fn issued_receipt_count_on_shift(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents \
         WHERE fiscal_number = ? AND state = 'ACK' AND doc_type IN ('SELL', 'RETURN')",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn z_report_payload(pool: &SqlitePool) -> serde_json::Value {
    let payload: String = sqlx::query_scalar(
        "SELECT payload_json FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = 'Z_REPORT' ORDER BY rowid DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap();
    serde_json::from_str(&payload).unwrap()
}

// ════════════════════════════════════════════════════════════════════════
// COMBINED PILOT GATE — the A+A′ phase-exit capstone
// ════════════════════════════════════════════════════════════════════════

/// The full phase-exit scenario in ONE lifecycle: boot → online SHIFT_OPEN →
/// online SELL → GO_OFFLINE → offline SELL + RETURN → GO_ONLINE → drain →
/// Z-close. Load-bearing pin: the Z after the drain aggregates BOTH cohorts —
/// the online-issued SELL AND the drained-offline SELL — so its `sell_count`
/// is 2, not 1. (If the drained-offline cohort were missed, sell_count would be
/// the online-only 1.) Teeth: reverting `FULL_OFFLINE_SURFACE_READY=false` REDs
/// the offline leg (GO_OFFLINE refuses) while the online-half legs still pass.
#[tokio::test]
async fn combined_pilot_gate_online_and_offline_cohorts_in_one_shift() {
    let app = boot_app().await;
    // Online binding acks, in drive order: SHIFT_OPEN, online SELL, Z_REPORT.
    let registry = build_registry(
        &app,
        online_dps(&["DPS-CG-OPEN", "DPS-CG-SELL", "DPS-CG-Z"]),
    )
    .await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(
        app.clone(),
        Arc::new(registry),
        std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339(
            "2026-07-07T12:30:00Z",
        )),
    );

    // 1) online SHIFT_OPEN + 2) an ONLINE SELL (issued online → ACK).
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-cg-OPEN", None),
    )
    .await
    .expect("SHIFT_OPEN");
    assert_eq!(open.document_state, DocState::Ack);
    let online_sell = drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-cg-ONLINE-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("online SELL");
    assert_eq!(
        online_sell.document_state,
        DocState::Ack,
        "online SELL issued"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // 3) GO_OFFLINE + offline SELL + RETURN (cohort 2, held at OLA).
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("live GO_OFFLINE");
    // B8-1: seed with real dps_code strings.
    let codes_b: Vec<String> = (0..6).map(|i| format!("DRILL-B-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes_b)
        .await
        .expect("seed codes");
    drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-cg-OFF-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL");
    drive(
        &*write_path,
        app.db(),
        entry(
            "RETURN",
            SELL_PAYLOAD,
            "idem-cg-OFF-RETURN",
            Some(TOTAL_KOP),
        ),
    )
    .await
    .expect("offline RETURN");

    // 4) GO_ONLINE + drain → both cohorts now terminal ACK on the shift.
    prro::admin::go_online(app.db(), FN, "dps restored")
        .await
        .expect("live GO_ONLINE");
    let carriers = drain_carriers();
    let view = drain_view(&carriers);
    let summary = match app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("drain runs")
    {
        ScheduledDrainOutcome::Ran(s) => s,
        ScheduledDrainOutcome::SkippedBackoff { .. } => panic!("drain must run"),
    };
    assert_eq!(
        summary.advanced_to_ack(),
        3,
        "B10: offline cohort (BEGIN + SELL + RETURN) drains to ACK; END is a \
         finalize precondition (not counted in the cohort)"
    );
    drop(carriers);
    assert_eq!(node_row(app.db()).await.0, "ONLINE");
    assert_eq!(
        issued_receipt_count_on_shift(app.db()).await,
        3,
        "1 online SELL + 1 drained-offline SELL + 1 drained-offline RETURN"
    );

    // 5) online Z_REPORT closes the shift, aggregating BOTH cohorts.
    let z = drive(
        &*write_path,
        app.db(),
        entry("Z_REPORT", r#"{}"#, "idem-cg-Z", None),
    )
    .await
    .expect("Z_REPORT closes the shift");
    assert_eq!(z.document_state, DocState::Ack, "Z issued");
    assert_eq!(shift_state(app.db()).await, "CLOSED");

    // LOAD-BEARING both-cohort pin: the Z aggregation counted the online SELL
    // AND the drained-offline SELL + RETURN — not just the online cohort.
    // (If the drained-offline cohort were missed, sell_count would be 1 and
    // sum_in_kop would be 15000.)
    let z_payload = z_report_payload(app.db()).await;
    assert_eq!(
        z_payload["sell_count"], 2,
        "2 sells counted: online + drained-offline"
    );
    assert_eq!(
        z_payload["return_count"], 1,
        "1 drained-offline return counted"
    );
    assert_eq!(
        z_payload["payments"][0]["sum_in_kop"], 30000,
        "turnover reflects BOTH sells (2 x 15000)"
    );
    assert_eq!(
        z_payload["payments"][0]["sum_out_kop"], 15000,
        "turnover reflects the drained-offline return"
    );

    prro::db::invariant_scan::assert_clean(app.db()).await;
}
