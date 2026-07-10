//! A′.3 PR-O1 — PILOT offline-half REACHABILITY e2e THROUGH THE LIVE BINDING.
//!
//! The offline lane (W4/W5 + stage_offline_ack) is built and fuzz-tested but was
//! unreachable from prod. This capstone proves it is now reachable through the
//! SAME live `production_write_path` / `InlineWritePath` binding the online-half
//! pilot uses — WITHOUT going through the gated operator door (which stays shut
//! in O1). The machinery is exercised by DIRECT seam calls, exactly as the
//! architect's flag-semantics ruling requires:
//!
//!   boot → online SHIFT_OPEN (edge 3, live) → mode-set OFFLINE (seam) →
//!   open_session (seam) → seed-codes (admin) → offline SELL + RETURN
//!   (via the live binding at mode=OFFLINE) → OFFLINE_LOCAL_ACK ×2, codes
//!   consumed, shift still OPENED, ledger scan CLEAN.
//!
//! DECOUPLING PIN (RP-O1-5): the shift is opened ONLINE (edge 3) BEFORE the
//! mode flip; the offline SELL/RETURN rest on an `Opened` shift and NEVER drive
//! the offline shift-open edge 2 (Created→OpenedLocalPendingDrain) — that (and
//! the drain that converges the OLA backlog) is O2. So this e2e honestly ends
//! at OLA-resting docs + an OPEN session (legal durable states); `assert_clean`
//! MUST be clean.
//!
//! DIRECT-SEAM PROOF: this e2e reaches the offline machinery via direct seams
//! (mode-set + open_session), independent of the operator door. (In A′.3 O2 the
//! door went live — proven by the admin door tests + the full drill; this test
//! stays a valid direct-seam reachability proof.)
//!
//! TEETH: revert the mode-set seam → the SELL routes online (no DPS ack queued
//! for it) and the e2e REDs; skip the online SHIFT_OPEN preopen → the offline
//! SELL REDs with a shift-not-opened refusal.

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
use prro::db::repositories::{fiscal_number_config as fn_cfg, node_state, operators as ops_repo};
use prro::db::tx::with_immediate;
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::ingress::inline_binding::production_write_path_with_clock;
use prro::runtime::ingress::seam::{FiscalOutcome, WritePathEntry};
use prro::services::offline_session::OfflineSessionService;
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::CheckAck;
use sqlx::SqlitePool;

use common::{ack, det_signing_ctx_for, StubDpsChannel};

const FN: &str = "4000000009";
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
listen  = "127.0.0.1:8444"
"#
    )
}

async fn boot_app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("pilot_off.db");
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

/// A `last_chk` KVT1 evidence ack for the online SHIFT_OPEN.
fn kvt1(sfn: &str) -> CheckAck {
    CheckAck {
        id: sfn.into(),
        id_sign: vec![],
        data_sign: vec![0xDE, 0xAD, 0xBE, 0xEF],
    }
}

/// DPS stub that ACKs ONLY the online SHIFT_OPEN — the offline SELL/RETURN
/// never touch DPS (stage_offline_ack is I1-clean), so the queue holds exactly
/// one send_chk + one last_chk.
fn shift_open_only_dps() -> Arc<dyn DpsChannel> {
    Arc::new(
        StubDpsChannel::with_queue(vec![Ok(ack("DPS-SFN-OPEN"))])
            .with_last_chk_queue(vec![Ok(kvt1("DPS-SFN-OPEN"))]),
    )
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

// ─── Probes ─────────────────────────────────────────────────────────────────

async fn shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn node_row(pool: &SqlitePool) -> (String, String) {
    sqlx::query_as("SELECT mode, shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn ola_doc_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND state = 'OFFLINE_LOCAL_ACK'",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn consumed_codes_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(FN)
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
// PILOT offline-half reachability e2e
// ════════════════════════════════════════════════════════════════════════

/// RP-O1-3 / RP-O1-4 / RP-O1-5 / RP-O1-12: offline SELL + RETURN are reachable
/// through the live binding at mode=OFFLINE, consume codes, reach OLA, leave
/// the shift Opened (edge 2 untouched), and rest on a CLEAN ledger scan.
#[tokio::test]
async fn pilot_offline_sell_and_return_reachable_via_live_binding() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    // ─── 1) ONLINE SHIFT_OPEN — pre-open the shift (edge 3, live) ──────────
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-off-OPEN", None),
    )
    .await
    .expect("online SHIFT_OPEN must reach terminal ACK");
    assert_eq!(open.document_state, DocState::Ack);
    assert_eq!(shift_state(app.db()).await, "OPENED");
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 2) GO OFFLINE via DIRECT SEAMS (not the gated door) ──────────────
    // 2a. mode-set ONLINE → OFFLINE (leaves shift_state=Opened, Frozen #3).
    let flipped = with_immediate(app.db(), |tx| {
        Box::pin(async move {
            Ok::<bool, anyhow::Error>(node_state::set_mode_offline_tx(tx, FN).await?)
        })
    })
    .await
    .unwrap();
    assert!(flipped, "ONLINE→OFFLINE CAS applied");
    // 2b. open an OFFLINE session (the open_session seam).
    OfflineSessionService::new(Arc::new(app.db().clone()))
        .open_session(FN)
        .await
        .expect("open offline session");
    assert_eq!(
        offline_session_state(app.db()).await.as_deref(),
        Some("OPEN")
    );
    // 2c. seed real DPS-issued codes via the admin pilot-drill affordance.
    // B8-1: switched to seed_dps_offline_codes so codes carry dps_code (non-NULL),
    // satisfying the acquire_code_tx guard added in B8-1.
    let codes: Vec<String> = (0..6).map(|i| format!("DRILL-H-{i}")).collect();
    let seeded = prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed offline codes");
    assert_eq!(seeded.inserted, 6);

    // (A′.3 O2: the operator door is now LIVE — proven by the admin door tests
    // + the full drill. This e2e still reaches the machinery via DIRECT seams,
    // so it remains a valid direct-seam reachability proof.)

    // ─── 3) OFFLINE SELL + RETURN via the live binding (mode=OFFLINE) ──────
    let sell = drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-off-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL reachable");
    assert_eq!(
        sell.document_state,
        DocState::OfflineLocalAck,
        "offline SELL must reach OFFLINE_LOCAL_ACK"
    );
    assert_eq!(
        consumed_codes_count(app.db()).await,
        2,
        "B10: the lazy DocType=9 BEGIN (minted at the first offline SELL) + the \
         SELL consumed two codes"
    );

    let ret = drive(
        &*write_path,
        app.db(),
        entry("RETURN", SELL_PAYLOAD, "idem-off-RETURN", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline RETURN reachable");
    assert_eq!(
        ret.document_state,
        DocState::OfflineLocalAck,
        "offline RETURN must reach OFFLINE_LOCAL_ACK"
    );
    assert_eq!(
        consumed_codes_count(app.db()).await,
        3,
        "B10: RETURN consumed a third code (BEGIN + SELL + RETURN)"
    );

    // ─── 4) O1 boundary: OLA-resting docs + OPEN session, shift STILL Opened ─
    assert_eq!(
        ola_doc_count(app.db()).await,
        3,
        "B10: BEGIN + SELL + RETURN all rest at OLA"
    );
    let (mode, shift) = node_row(app.db()).await;
    assert_eq!(mode, "OFFLINE");
    assert_eq!(
        shift, "OPENED",
        "shift stayed Opened — edge 2 (offline shift-open) is NOT driven in O1 (that is O2)"
    );
    assert_eq!(shift_state(app.db()).await, "OPENED");
    assert_eq!(
        offline_session_state(app.db()).await.as_deref(),
        Some("OPEN")
    );

    // RP-O1-12: OLA + OPEN session are legal durable states — the scan is CLEAN.
    prro::db::invariant_scan::assert_clean(app.db()).await;
}
