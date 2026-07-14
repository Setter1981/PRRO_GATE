//! A′.3 PR-O3 — PILOT offline SHIFT-LIFECYCLE drills THROUGH THE LIVE BINDING
//! + LIVE DOOR (the O3 capstones over slices 1-3).
//!
//! Two operator stories the O2 full-drill deliberately did NOT cover (its
//! shift was opened online and stayed `Opened`):
//!
//!   Drill A — "утро без сети" (morning without network):
//!     boot → GO_OFFLINE (live door, shift still Closed) → OFFLINE SHIFT_OPEN
//!     (edge 2 → OpenedLocalPendingDrain, doc local-acks + consumes a code) →
//!     offline SELLs → GO_ONLINE → drain (SHIFT_OPEN doc drains FIRST,
//!     strict-sequential lnd) → edge 5 LIVE (OLPD → Opened) → online Z-close
//!     (aggregating the drained-offline receipts) → shift CLOSED, scan CLEAN.
//!
//!   Drill B — "полный offline-день" (full offline day):
//!     boot → GO_OFFLINE → OFFLINE SHIFT_OPEN (edge 2) → offline SELLs →
//!     OFFLINE Z-close (edge 7 OLPD → CLPD; the local-Z aggregates the OLA
//!     backlog — C10) → GO_ONLINE → drain of EVERYTHING (order pin: the
//!     shift-open doc first, the Z doc last — strict-sequential lnd) →
//!     edge 13 LIVE (CLPD → Closed) → converge, scan CLEAN.
//!
//! TEETH (per O3 contract): revert `FULL_OFFLINE_SURFACE_READY = false` → the
//! live door REDs (GO_OFFLINE refuses) and BOTH drills RED at their first
//! offline leg, while the online-half suites stay green.

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

const FN: &str = "4000000023";
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

async fn boot_app(db_name: &str) -> App {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join(db_name);
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
    .bind(NodeMode::Online.as_str())
    .bind(ShiftState::Closed.as_str())
    .execute(pool)
    .await
    .unwrap();
}

fn kvt1(sfn: &str) -> CheckAck {
    CheckAck {
        id: sfn.into(),
        id_sign: vec![],
        data_sign: vec![0xDE; 64],
    }
}

/// Registry DPS acking each ONLINE binding doc in `sfns` order (send_chk +
/// last_chk each).  Offline docs never touch the registry DPS; the drain uses
/// a SEPARATE RuntimeView DPS (`drain_carriers_for`).
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

// ─── drain carriers (separate DPS; one send+last pair per backlog doc) ──────

struct DrainCarriers {
    dps: Arc<StubDpsChannel>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn drain_carriers_for(sfns: &[&str]) -> DrainCarriers {
    let sends = sfns.iter().map(|s| Ok(ack(s))).collect::<Vec<_>>();
    let lasts = sfns
        .iter()
        .map(|s| {
            Ok(CheckAck {
                id: (*s).into(),
                id_sign: vec![],
                data_sign: vec![0xAAu8; 64],
            })
        })
        .collect::<Vec<_>>();
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

async fn consumed_codes_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes \
         WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ════════════════════════════════════════════════════════════════════════
// Drill A — "утро без сети": offline OPEN → sells → reconnect → drain →
// edge 5 live → ONLINE Z-close.
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn drill_a_morning_without_network_offline_open_drain_online_z_close() {
    let app = boot_app("drill_a.db").await;
    // Registry DPS is used ONLY by the final online Z-close (offline docs and
    // the drain never touch it).
    let registry = build_registry(&app, online_dps(&["DPS-A-Z"])).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(
        app.clone(),
        Arc::new(registry),
        std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339(
            "2026-07-07T12:30:00Z",
        )),
    );

    // ─── 1) the net is down BEFORE the shift opens: GO_OFFLINE on Closed ──
    prro::admin::go_offline(app.db(), FN, "morning net down")
        .await
        .expect("live door: GO_OFFLINE with shift still Closed");
    assert_eq!(
        node_row(app.db()).await,
        ("OFFLINE".into(), "CLOSED".into())
    );
    assert_eq!(
        offline_session_state(app.db()).await.as_deref(),
        Some("OPEN")
    );
    // B8-1: seed with real dps_code strings (acquire_code_tx requires dps_code IS NOT NULL).
    let codes_a: Vec<String> = (0..6).map(|i| format!("DRILL-SL-A-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes_a)
        .await
        .expect("seed codes");

    // ─── 2) OFFLINE SHIFT_OPEN — edge 2 LIVE through the binding ──────────
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-a-OPEN", None),
    )
    .await
    .expect("offline SHIFT_OPEN must local-ack");
    assert_eq!(
        open.document_state,
        DocState::OfflineLocalAck,
        "the offline SHIFT_OPEN doc rests at OLA pending drain"
    );
    assert_eq!(
        shift_state(app.db()).await,
        "OPENED_LOCAL_PENDING_DRAIN",
        "edge 2: Created → OpenedLocalPendingDrain"
    );
    assert_eq!(
        consumed_codes_count(app.db()).await,
        2,
        "B10 numbering (ii): the lazy DocType=9 BEGIN (minted BEFORE the offline \
         SHIFT_OPEN, as the session's first offline doc) + the SHIFT_OPEN each \
         consumed a code"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 3) offline SELLs on the OLPD shift (Pattern C surface) ───────────
    for (i, idem) in ["idem-a-SELL-1", "idem-a-SELL-2"].iter().enumerate() {
        let sell = drive(
            &*write_path,
            app.db(),
            entry("SELL", SELL_PAYLOAD, idem, Some(TOTAL_KOP)),
        )
        .await
        .expect("offline SELL");
        assert_eq!(
            sell.document_state,
            DocState::OfflineLocalAck,
            "offline SELL {i} rests at OLA"
        );
    }
    // B10: lazy DocType=9 BEGIN (minted BEFORE the offline SHIFT_OPEN, as the
    // session's FIRST offline doc) + SHIFT_OPEN + SELL#1 + SELL#2 = 4 OLA docs.
    assert_eq!(doc_count_in_state(app.db(), "OFFLINE_LOCAL_ACK").await, 4);
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 4) reconnect: GO_ONLINE + drain (SHIFT_OPEN doc first, lnd order) ─
    prro::admin::go_online(app.db(), FN, "net restored")
        .await
        .expect("live door: GO_ONLINE");
    // B10: 4 backlog docs (SHIFT_OPEN + BEGIN + 2 SELL) + the DocType=10 END
    // minted at drain finalize = up to 5 wire sends.
    let carriers =
        drain_carriers_for(&["DPS-A-D1", "DPS-A-D2", "DPS-A-D3", "DPS-A-D4", "DPS-A-D5"]);
    let view = drain_view(&carriers);
    let summary = match app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("drain runs for a GOING_ONLINE FN")
    {
        ScheduledDrainOutcome::Ran(s) => s,
        ScheduledDrainOutcome::SkippedBackoff { .. } => panic!("first drain tick must run"),
    };
    assert_eq!(
        summary.backlog_size_before(),
        4,
        "B10: SHIFT_OPEN + BEGIN + 2 SELLs in the content backlog"
    );
    assert_eq!(
        summary.advanced_to_ack(),
        4,
        "all four content docs drain to ACK (END is a finalize precondition, \
         not counted in the cohort)"
    );
    drop(carriers);

    // ─── 5) edge 5 LIVE: the drained SHIFT_OPEN converges the shift ───────
    assert_eq!(
        shift_state(app.db()).await,
        "OPENED",
        "edge 5: OpenedLocalPendingDrain → Opened (drain converged the shift)"
    );
    let (mode, node_shift) = node_row(app.db()).await;
    assert_eq!(mode, "ONLINE", "node converged back to ONLINE");
    assert_eq!(node_shift, "OPENED", "node_state mirrors edge 5");
    assert_eq!(
        offline_session_state(app.db()).await.as_deref(),
        Some("CLOSED"),
        "offline session closed by the drain"
    );
    assert_eq!(doc_count_in_state(app.db(), "OFFLINE_LOCAL_ACK").await, 0);
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 6) ONLINE Z-close over the drained shift ──────────────────────────
    let z = drive(
        &*write_path,
        app.db(),
        entry("Z_REPORT", r#"{}"#, "idem-a-Z", None),
    )
    .await
    .expect("online Z_REPORT closes the drained shift");
    assert_eq!(z.document_state, DocState::Ack, "Z issued online");
    assert_eq!(shift_state(app.db()).await, "CLOSED");

    // Everything rests terminal — the drill's quiescent boundary is CLEAN.
    prro::db::invariant_scan::assert_clean(app.db()).await;
}

// ════════════════════════════════════════════════════════════════════════
// Drill B — "полный offline-день": offline OPEN → sells → OFFLINE Z-close
// (local-Z over the OLA backlog) → reconnect → drain EVERYTHING → edge 13.
// ════════════════════════════════════════════════════════════════════════

async fn backlog_lnd_doc_types(pool: &SqlitePool) -> Vec<(i64, String, String)> {
    sqlx::query_as(
        "SELECT lnd, doc_type, state FROM fiscal_documents \
         WHERE fiscal_number = ? ORDER BY lnd",
    )
    .bind(FN)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn drill_b_full_offline_day_offline_open_sells_offline_z_close_drain_converges() {
    let app = boot_app("drill_b.db").await;
    // The WHOLE day is offline — the registry DPS is never touched (empty
    // queues would panic loudly if any leg tried the wire).
    let registry = build_registry(&app, online_dps(&[])).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(
        app.clone(),
        Arc::new(registry),
        std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339(
            "2026-07-07T12:30:00Z",
        )),
    );

    // ─── 1) GO_OFFLINE on Closed + codes ───────────────────────────────────
    prro::admin::go_offline(app.db(), FN, "full offline day")
        .await
        .expect("live door: GO_OFFLINE");
    // B8-1: seed with real dps_code strings.
    let codes_b: Vec<String> = (0..10).map(|i| format!("DRILL-SL-B-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes_b)
        .await
        .expect("seed codes");

    // ─── 2) OFFLINE SHIFT_OPEN (edge 2) ────────────────────────────────────
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-b-OPEN", None),
    )
    .await
    .expect("offline SHIFT_OPEN");
    assert_eq!(open.document_state, DocState::OfflineLocalAck);
    assert_eq!(shift_state(app.db()).await, "OPENED_LOCAL_PENDING_DRAIN");

    // ─── 3) offline SELLs on the OLPD shift ────────────────────────────────
    for idem in ["idem-b-SELL-1", "idem-b-SELL-2"] {
        let sell = drive(
            &*write_path,
            app.db(),
            entry("SELL", SELL_PAYLOAD, idem, Some(TOTAL_KOP)),
        )
        .await
        .expect("offline SELL");
        assert_eq!(sell.document_state, DocState::OfflineLocalAck);
    }
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 4) OFFLINE Z-close — edge 7 LIVE (OLPD → CLPD), local-Z over the
    //        undrained OLA backlog (C10 quiesces Clear by construction) ─────
    let z = drive(
        &*write_path,
        app.db(),
        entry("Z_REPORT", r#"{}"#, "idem-b-Z", None),
    )
    .await
    .expect("OFFLINE Z-close must local-ack (edge 7 + local-Z)");
    assert_eq!(
        z.document_state,
        DocState::OfflineLocalAck,
        "the local-Z doc rests at OLA pending drain"
    );
    assert_eq!(
        shift_state(app.db()).await,
        "CLOSING_LOCAL_PENDING_DRAIN",
        "edge 7: OpenedLocalPendingDrain → ClosingLocalPendingDrain"
    );
    assert_eq!(
        consumed_codes_count(app.db()).await,
        5,
        "B10 numbering (ii): SHIFT_OPEN + BEGIN + 2 SELLs + Z each consumed a code"
    );
    // The local-Z AGGREGATED the shift's OLA receipts at close time (C10):
    // 2 sells → sell_count 2, turnover 2×15000.
    let z_payload: serde_json::Value = {
        let raw: String = sqlx::query_scalar(
            "SELECT payload_json FROM fiscal_documents \
             WHERE fiscal_number = ? AND doc_type = 'Z_REPORT'",
        )
        .bind(FN)
        .fetch_one(app.db())
        .await
        .unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    assert_eq!(
        z_payload["sell_count"], 2,
        "local-Z aggregated the OLA sells (C10 counts OLA as issued)"
    );
    assert_eq!(
        z_payload["payments"][0]["sum_in_kop"], 30000,
        "local-Z turnover = 2 × 15000"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 5) ORDER PIN (strict-sequential lnd): the drain sends the BEGIN
    //        FIRST (opens the DPS offline window) and the Z doc LAST ─────────
    let backlog = backlog_lnd_doc_types(app.db()).await;
    // B10: BEGIN(1, lazily minted as the session's FIRST offline doc, BEFORE the
    // offline SHIFT_OPEN) + SHIFT_OPEN(2) + SELL(3) + SELL(4) + Z(5) = 5 docs.
    assert_eq!(backlog.len(), 5);
    assert_eq!(
        (backlog[0].1.as_str(), backlog[0].2.as_str()),
        ("OFFLINE_SESSION_BEGIN", "OFFLINE_LOCAL_ACK"),
        "B10: the lazy DocType=9 BEGIN is the lowest lnd → drains FIRST (opens \
         the DPS offline window before the offline SHIFT_OPEN)"
    );
    assert_eq!(
        (backlog[1].1.as_str(), backlog[1].2.as_str()),
        ("SHIFT_OPEN", "OFFLINE_LOCAL_ACK"),
        "the offline SHIFT_OPEN sits at lnd 2, right after the BEGIN"
    );
    assert_eq!(
        (backlog[4].1.as_str(), backlog[4].2.as_str()),
        ("Z_REPORT", "OFFLINE_LOCAL_ACK"),
        "highest content lnd = the Z doc → drains LAST (before the END boundary)"
    );

    // ─── 5b) CLPD-BLOCKING PIN (contract): a NEW shift can NOT open until
    //         the closed one drains — "day 2 waits for day 1's drain".  The
    //         guard refuses (ShiftOpen, CLPD, _) → ShiftClosingInFlight, and
    //         the uq-index (migration 026) counts CLPD as active underneath.
    //         INV-09 consequence (runbook): the 36h offline window spans
    //         shifts, but a second offline DAY cannot start until day 1
    //         drains — an honest operational limit, not a hazard. ────────────
    let day2 = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-b-OPEN-DAY2", None),
    )
    .await;
    assert!(
        day2.is_err(),
        "SHIFT_OPEN on an undrained CLPD shift must be refused, got {day2:?}"
    );
    assert_eq!(
        shift_state(app.db()).await,
        "CLOSING_LOCAL_PENDING_DRAIN",
        "the refusal left the CLPD shift untouched"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 6) reconnect: GO_ONLINE + drain of EVERYTHING ─────────────────────
    prro::admin::go_online(app.db(), FN, "evening net restored")
        .await
        .expect("live door: GO_ONLINE");
    // B10: 5 content docs (SHIFT_OPEN + BEGIN + 2 SELL + Z) + the DocType=10 END
    // minted at drain finalize = up to 6 wire sends.
    let carriers = drain_carriers_for(&[
        "DPS-B-D1", "DPS-B-D2", "DPS-B-D3", "DPS-B-D4", "DPS-B-D5", "DPS-B-D6",
    ]);
    let view = drain_view(&carriers);
    let summary = match app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("drain runs")
    {
        ScheduledDrainOutcome::Ran(s) => s,
        ScheduledDrainOutcome::SkippedBackoff { .. } => panic!("first drain tick must run"),
    };
    assert_eq!(
        summary.backlog_size_before(),
        5,
        "B10: SHIFT_OPEN + BEGIN + 2 SELLs + Z all in the content backlog"
    );
    assert_eq!(
        summary.advanced_to_ack(),
        5,
        "all five content docs drain to ACK (END is a finalize precondition)"
    );
    drop(carriers);

    // ─── 7) edge 13 LIVE: the drained Z converges the shift to Closed ──────
    assert_eq!(
        shift_state(app.db()).await,
        "CLOSED",
        "edge 13: ClosingLocalPendingDrain → Closed (drain converged the close)"
    );
    let (mode, node_shift) = node_row(app.db()).await;
    assert_eq!(mode, "ONLINE", "node converged back to ONLINE");
    assert_eq!(node_shift, "CLOSED", "node_state mirrors edge 13");
    assert_eq!(
        offline_session_state(app.db()).await.as_deref(),
        Some("CLOSED"),
        "offline session closed by the drain"
    );
    assert_eq!(doc_count_in_state(app.db(), "OFFLINE_LOCAL_ACK").await, 0);
    // B10: SHIFT_OPEN + BEGIN + 2 SELL + Z + END = 6 all ACK.
    assert_eq!(doc_count_in_state(app.db(), "ACK").await, 6);

    // The full offline day rests terminal — the quiescent boundary is CLEAN.
    prro::db::invariant_scan::assert_clean(app.db()).await;
}
