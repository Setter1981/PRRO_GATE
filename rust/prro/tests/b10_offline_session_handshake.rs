//! B10 — offline-session drain handshake (DocType 9/10) integration tests.
//!
//! RED-first pins for the offline-session boundary-doc handshake:
//!   #4 (headline) drain wire order `[9, content…, 10]`;
//!   #5 lazy-mint idempotency;
//!   #6 timestamps;
//!   #7 crash-idempotent drain;
//!   #8 teeth (revert leading-9 → #4 fails).
//!
//! The XML/typCheck/MAC pins (#1/#2/#3) live as lib unit tests next to the
//! seams they cover (`xml::tests`, `stage_send::tests`).
//!
//! Harness mirrors `pilot_offline_full_drill_e2e.rs`: boot → online SHIFT_OPEN
//! → go_offline → seed codes → offline SELL(s) → go_online → drain.  A local
//! RECORDING DPS stub captures each `send_chk` envelope's `check_type` +
//! `local_number` in wire order so the ordering pin can assert `[9,…,10]`.

mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::app::App;
use prro::config::AppConfig;
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, Protocol, ShiftState};
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
use prro::transports::dps::dto::{
    CheckAck, CheckEnvelope, CheckSignBlob, DpsCheckType, OfflineCodesResponse, RroInfo,
    StatusSnapshot,
};
use prro::transports::dps::error::DpsError;
use prro::ScheduledDrainOutcome;
use sqlx::SqlitePool;

use common::{ack, det_signing_ctx, det_signing_ctx_for};

const FN: &str = "4000000019";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

// ─── recording DPS stub (captures wire order) ────────────────────────────────

/// A DPS stub that records each `send_chk` envelope's `(check_type,
/// local_number, id_offline_empty)` in wire order.  ACKs from a queue.
struct RecordingDps {
    sends: Mutex<std::collections::VecDeque<Result<CheckAck, DpsError>>>,
    last_chks: Mutex<std::collections::VecDeque<Result<CheckAck, DpsError>>>,
    recorded: Mutex<Vec<(DpsCheckType, i32)>>,
}

impl RecordingDps {
    fn new(
        sends: Vec<Result<CheckAck, DpsError>>,
        last_chks: Vec<Result<CheckAck, DpsError>>,
    ) -> Self {
        Self {
            sends: Mutex::new(sends.into()),
            last_chks: Mutex::new(last_chks.into()),
            recorded: Mutex::new(Vec::new()),
        }
    }
    fn wire_order(&self) -> Vec<(DpsCheckType, i32)> {
        self.recorded.lock().unwrap().clone()
    }
}

#[async_trait]
impl DpsChannel for RecordingDps {
    async fn send_chk(&self, e: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.recorded
            .lock()
            .unwrap()
            .push((e.check_type, e.local_number));
        self.sends
            .lock()
            .unwrap()
            .pop_front()
            .expect("RecordingDps send queue empty")
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_chks
            .lock()
            .unwrap()
            .pop_front()
            .expect("RecordingDps last_chk queue empty")
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("ping not exercised")
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("status_rro not exercised")
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("info_rro not exercised")
    }
    async fn ask_offline_codes(&self, _: CheckEnvelope) -> Result<OfflineCodesResponse, DpsError> {
        unreachable!("ask_offline_codes not exercised")
    }
}

// ─── boot harness (mirror of pilot_offline_full_drill_e2e) ───────────────────

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
    let db_path = dir.path().join("b10.db");
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

fn shift_open_only_dps() -> Arc<dyn DpsChannel> {
    Arc::new(
        common::StubDpsChannel::with_queue(vec![Ok(ack("DPS-SFN-OPEN"))])
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
    e: NewInboxEntry,
) -> Result<FiscalOutcome, prro::runtime::ingress::seam::FiscalError> {
    let row: InboxRow = match inbox::insert(pool, &e).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await
}

// ─── drain carriers ──────────────────────────────────────────────────────────

struct DrainCarriers {
    dps: Arc<RecordingDps>,
    signing_ctx: SigningContext,
    fn_sign: CheckSignBlob,
}

fn drain_carriers(n_docs: usize) -> DrainCarriers {
    // Each drained doc: one send_chk + one last_chk.
    let sends: Vec<_> = (0..n_docs)
        .map(|i| Ok(ack(&format!("DPS-DRAIN-{i}"))))
        .collect();
    let lasts: Vec<_> = (0..n_docs)
        .map(|i| {
            Ok(CheckAck {
                id: format!("DPS-DRAIN-{i}"),
                id_sign: vec![],
                data_sign: vec![(i as u8).wrapping_add(0xA0); 32],
            })
        })
        .collect();
    DrainCarriers {
        dps: Arc::new(RecordingDps::new(sends, lasts)),
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

// ─── probes ──────────────────────────────────────────────────────────────────

async fn doc_types_by_lnd(pool: &SqlitePool) -> Vec<(i64, String, String)> {
    sqlx::query_as(
        "SELECT lnd, doc_type, state FROM fiscal_documents WHERE fiscal_number = ? ORDER BY lnd ASC",
    )
    .bind(FN)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn count_doc_type(pool: &SqlitePool, dt: DocType) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND doc_type = ?",
    )
    .bind(FN)
    .bind(dt)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The unsigned canonical XML (PAYLOAD_XML) + stamped offline_dps_code of the
/// single boundary doc of `dt`.  The `<MAC ID='...'>` lives in the unsigned
/// canonical XML (the signed CMS is an opaque .p7s blob).
async fn boundary_payload_xml_and_code(pool: &SqlitePool, dt: DocType) -> (String, Option<String>) {
    let (doc_id, code): (Vec<u8>, Option<String>) = sqlx::query_as(
        "SELECT document_id, offline_dps_code FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = ?",
    )
    .bind(FN)
    .bind(dt)
    .fetch_one(pool)
    .await
    .unwrap();
    let xml_bytes: Vec<u8> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'PAYLOAD_XML'",
    )
    .bind(&doc_id)
    .fetch_one(pool)
    .await
    .unwrap();
    // cp1251 payload; the MAC/ID region is ASCII so lossy UTF-8 is fine for the
    // substring assertion.
    (String::from_utf8_lossy(&xml_bytes).into_owned(), code)
}

async fn consumed_code_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Send-shape fields of the single boundary doc of `dt`:
/// `(fs_mode, offline_fiscal_no, server_fiscal_no)`.  The online-shaped END has
/// `fs_mode='ONLINE'`, `offline_fiscal_no IS NULL` (→ empty wire `id_offline`),
/// and a non-empty `server_fiscal_no` (stamped at the advance-at-SEND CAS).
async fn boundary_send_shape(
    pool: &SqlitePool,
    dt: DocType,
) -> (String, Option<i64>, Option<String>) {
    sqlx::query_as(
        "SELECT fs_mode, offline_fiscal_no, server_fiscal_no FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = ?",
    )
    .bind(FN)
    .bind(dt)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ══════════════════════════════════════════════════════════════════════════
// #4 (HEADLINE) — drain wire order is [9, SHIFT_OPEN? content…, 10]
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_drain_sends_begin_first_content_then_end_last() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    // online SHIFT_OPEN
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-OPEN", None),
    )
    .await
    .expect("online SHIFT_OPEN must ACK");
    assert_eq!(open.document_state, DocState::Ack);

    // GO_OFFLINE + seed codes (need >= content(1) + 2 boundary = 3)
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("go_offline");
    let codes: Vec<String> = (0..6).map(|i| format!("CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed codes");

    // one offline SELL → lazily mints the DocType=9 BEGIN as the FIRST offline doc.
    let sell = drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL");
    assert_eq!(sell.document_state, DocState::OfflineLocalAck);

    // The DocType=9 must exist and have a LOWER lnd than the SELL.
    let docs = doc_types_by_lnd(app.db()).await;
    let begin = docs.iter().find(|(_, dt, _)| dt == "OFFLINE_SESSION_BEGIN");
    assert!(
        begin.is_some(),
        "a DocType=9 BEGIN must be lazily minted: {docs:?}"
    );
    let begin_lnd = begin.unwrap().0;
    let sell_lnd = docs.iter().find(|(_, dt, _)| dt == "SELL").unwrap().0;
    assert!(
        begin_lnd < sell_lnd,
        "BEGIN(lnd={begin_lnd}) must precede SELL(lnd={sell_lnd})"
    );
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        1
    );

    // GO_ONLINE + DRAIN. Backlog = BEGIN + SELL = 2; drain mints+sends END last.
    prro::admin::go_online(app.db(), FN, "restored")
        .await
        .expect("go_online");
    let carriers = drain_carriers(3); // BEGIN + SELL + END
    let view = drain_view(&carriers);
    let outcome = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("drain must run");
    let summary = match outcome {
        ScheduledDrainOutcome::Ran(s) => s,
        ScheduledDrainOutcome::SkippedBackoff { .. } => panic!("first tick must run"),
    };
    assert!(
        summary.finalized(),
        "drain must finalize (all docs Ack + END sent)"
    );

    // Wire order: ServiceChk(9) FIRST, then the SELL(Chk), then ServiceChk(10) LAST.
    let order = carriers.dps.wire_order();
    assert!(order.len() >= 3, "expected >= 3 wire sends, got {order:?}");
    assert_eq!(
        order.first().unwrap().0,
        DpsCheckType::ServiceChk,
        "FIRST wire send must be the DocType=9 BEGIN (ServiceChk): {order:?}"
    );
    assert_eq!(
        order.last().unwrap().0,
        DpsCheckType::ServiceChk,
        "LAST wire send must be the DocType=10 END (ServiceChk): {order:?}"
    );
    // The content SELL (Chk) sits strictly between the two boundary ServiceChks.
    let sell_pos = order.iter().position(|(t, _)| *t == DpsCheckType::Chk);
    assert!(
        sell_pos.is_some(),
        "content SELL(Chk) must appear on the wire: {order:?}"
    );
    let sell_pos = sell_pos.unwrap();
    assert!(
        sell_pos > 0 && sell_pos < order.len() - 1,
        "SELL must be between BEGIN and END: {order:?}"
    );

    // Both boundary docs converged to ACK; session closed.
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        1
    );
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionEnd).await,
        1
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;
}

// ══════════════════════════════════════════════════════════════════════════
// #5 — lazy-mint idempotency: a 2nd offline SELL does NOT mint a 2nd DocType=9;
//      a session with zero offline business docs mints NO DocType=9.
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_lazy_begin_minted_once_across_multiple_offline_docs() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-OPEN", None),
    )
    .await
    .expect("SHIFT_OPEN");
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("go_offline");
    let codes: Vec<String> = (0..8).map(|i| format!("CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed codes");

    // Two offline SELLs.
    drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-SELL-1", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL 1");
    drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-SELL-2", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL 2");

    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        1,
        "exactly ONE DocType=9 across two offline business docs"
    );
}

#[tokio::test]
async fn b10_no_begin_for_session_with_zero_offline_business_docs() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-OPEN", None),
    )
    .await
    .expect("SHIFT_OPEN");
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("go_offline");
    // No offline business doc issued.
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        0,
        "no spurious DocType=9 for an empty offline session"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// #6 — the DocType=9 BEGIN is stamped with the session's offline-entry time
//      (`opened_at`), NOT the first-SELL time (docs-as-canon / 168h fidelity).
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_begin_stamped_with_session_opened_at() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-OPEN", None),
    )
    .await
    .expect("SHIFT_OPEN");
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("go_offline");
    let codes: Vec<String> = (0..6).map(|i| format!("CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed codes");

    let session_opened_at: String =
        sqlx::query_scalar("SELECT opened_at FROM offline_sessions WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(app.db())
            .await
            .unwrap();

    drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL");

    let begin_ts: String = sqlx::query_scalar(
        "SELECT business_ts FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = 'OFFLINE_SESSION_BEGIN'",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();

    assert_eq!(
        begin_ts, session_opened_at,
        "DocType=9 BEGIN business_ts must equal the session opened_at (offline-entry time)"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Decision-5 crash guard — a crashed-mid-sign BEGIN (below OFFLINE_LOCAL_ACK)
// must fail-close a fresh offline SELL (RETRYABLE 503), NOT let it sign against
// a non-issued predecessor (offline lane bypasses the D5 sibling gate).
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_crashed_prepared_begin_fails_closed_fresh_sell() {
    use prro::runtime::ingress::seam::FiscalError;

    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-OPEN", None),
    )
    .await
    .expect("SHIFT_OPEN");
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("go_offline");
    let codes: Vec<String> = (0..6).map(|i| format!("CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed codes");

    // Inject a crashed-mid-sign BEGIN: a PREPARED boundary doc bound to the
    // current shift (offline_session_id NULL, exactly as a crash before sign
    // leaves it).
    let shift_id: Vec<u8> =
        sqlx::query_scalar("SELECT current_shift_id FROM node_state WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(app.db())
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO fiscal_documents \
         (document_id, request_id, fiscal_number, shift_id, offline_session_id, lnd, doc_type, \
          state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
          payload_sha256_canonical, source_sha256) \
         VALUES (randomblob(16), randomblob(16), ?, ?, NULL, 999, 'OFFLINE_SESSION_BEGIN', \
          'PREPARED', 'b', 't', 'OFFLINE', '2026-07-07T00:00:00Z', '{}', \
          zeroblob(32), zeroblob(32))",
    )
    .bind(FN)
    .bind(&shift_id)
    .execute(app.db())
    .await
    .unwrap();

    // A fresh offline SELL must fail-closed RETRYABLE (503), NOT proceed to sign.
    let err = drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect_err("SELL must fail-closed while the BEGIN is stuck below OLA");
    assert!(
        matches!(err, FiscalError::OfflineRefused { .. }),
        "expected a RETRYABLE OfflineRefused, got {err:?}"
    );
    // No second BEGIN was minted (idempotency held).
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        1,
        "the crashed BEGIN must NOT be re-minted"
    );
    // The SELL did not sign (no SELL doc row reached SIGNED/OLA).
    let sell_states: Vec<String> = sqlx::query_scalar(
        "SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND doc_type = 'SELL'",
    )
    .bind(FN)
    .fetch_all(app.db())
    .await
    .unwrap();
    assert!(
        sell_states
            .iter()
            .all(|s| s != "OFFLINE_LOCAL_ACK" && s != "SIGNED"),
        "SELL must not have signed against a non-issued BEGIN: {sell_states:?}"
    );
}

/// Run the full offline cycle (SHIFT_OPEN → go_offline → seed → offline SELL →
/// go_online → drain) and return the app + drain carriers so callers can probe
/// the drained ledger.  Panics on any unexpected outcome.
async fn run_full_offline_drain(n_drain_docs: usize) -> (App, DrainCarriers) {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-OPEN", None),
    )
    .await
    .expect("SHIFT_OPEN");
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("go_offline");
    let codes: Vec<String> = (0..6).map(|i| format!("CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed codes");
    drive(
        &*write_path,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("offline SELL");
    prro::admin::go_online(app.db(), FN, "restored")
        .await
        .expect("go_online");

    let carriers = drain_carriers(n_drain_docs);
    let view = drain_view(&carriers);
    let outcome = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("drain must run");
    match outcome {
        ScheduledDrainOutcome::Ran(s) => {
            assert!(
                s.finalized(),
                "drain must finalize (all content Ack + END Ack)"
            );
        }
        ScheduledDrainOutcome::SkippedBackoff { .. } => panic!("first tick must run"),
    }
    (app, carriers)
}

// ══════════════════════════════════════════════════════════════════════════
// #8 (TEETH) — the DocType=10 END, minted at drain finalize (node GoingOnline),
// is an ONLINE issuance: it MUST sign an ONLINE-shaped receipt — a BARE `<MAC>`
// (NO `ID=`), consume NO pool code, and carry an empty wire `id_offline` +
// a `server_fiscal_no` stamped at the advance-at-SEND CAS.  This mirrors
// WebCheck's `CloseOfflineDoc` (proven live: a bare-`<MAC>` online END was
// ACCEPTED by DPS and closed the session).  The old offline-shaped END
// (`<MAC ID='{code}'>` + a consumed code) earned DPS `-5 "no id offline"` on
// a drain-to-online close.  TEETH: revert the END to offline-shaped
// (`<MAC ID>` + a forced code) → the bare-`<MAC>` / no-`ID=` pins RED.
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_end_signed_online_shaped_bare_mac_under_going_online() {
    let (app, _carriers) = run_full_offline_drain(3).await; // BEGIN + SELL + END

    let (end_xml, end_code) =
        boundary_payload_xml_and_code(app.db(), DocType::OfflineSessionEnd).await;

    // TEETH: the END carries a BARE `<MAC>` (online shape), NOT `<MAC ID='...'>`.
    assert!(
        !end_xml.contains("<MAC ID="),
        "END must sign ONLINE-shaped bare <MAC> (offline path signs <MAC ID='...'> → DPS -5): {end_xml}"
    );
    assert!(
        end_xml.contains("<MAC>"),
        "END must carry a bare <MAC> element: {end_xml}"
    );
    // The online END consumes NO pool code → no stamped offline_dps_code.
    assert!(
        end_code.is_none(),
        "END must NOT stamp an offline_dps_code (online issuance consumes no code): {end_code:?}"
    );
    // It is a `<C T="110">` service receipt.
    assert!(
        end_xml.contains(r#"<C T="110">"#),
        "END must be <C T=110>: {end_xml}"
    );
}

#[tokio::test]
async fn b10_offline_boundary_begin_consumes_one_code_end_consumes_none() {
    // BEGIN consumes 1 code (minted while Offline → offline-shaped `<MAC ID>`).
    // The content SELL consumes 1.  The END is an ONLINE issuance minted at
    // drain finalize (bare `<MAC>`) → it consumes NO code.  So 2 codes total for
    // a 1-content session: the offline BEGIN + the offline SELL.
    let (app, _carriers) = run_full_offline_drain(3).await;
    assert_eq!(
        consumed_code_count(app.db()).await,
        2,
        "BEGIN + SELL each consume one pool code; the online END consumes NONE"
    );
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        1
    );
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionEnd).await,
        1
    );
}

// ══════════════════════════════════════════════════════════════════════════
// RED-first pin #4 — the DocType=10 END is an ONLINE ISSUANCE: the row carries
// `fs_mode='ONLINE'`, `offline_fiscal_no IS NULL` (→ empty wire `id_offline`),
// and a non-empty `server_fiscal_no` stamped at the `Sending → Sent` CAS
// (advance-at-SEND).  The BEGIN stays offline-shaped (`fs_mode='OFFLINE'`,
// a stamped `offline_fiscal_no`, NO `server_fiscal_no` — an offline-acked doc).
// This asymmetry mirrors WebCheck (`CloseOfflineDocOffline` vs `CloseOfflineDoc`).
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_end_is_online_issuance_with_sfn_empty_id_offline() {
    let (app, _carriers) = run_full_offline_drain(3).await;

    // END: online-shaped issued doc.
    let (end_fs_mode, end_offline_no, end_sfn) =
        boundary_send_shape(app.db(), DocType::OfflineSessionEnd).await;
    assert_eq!(
        end_fs_mode, "ONLINE",
        "END must be minted fs_mode=ONLINE (online issuance at drain finalize)"
    );
    assert!(
        end_offline_no.is_none(),
        "END must have NULL offline_fiscal_no (→ empty wire id_offline): {end_offline_no:?}"
    );
    let sfn = end_sfn.expect("END must be stamped a server_fiscal_no at the Sending→Sent CAS");
    assert!(
        !sfn.is_empty(),
        "END server_fiscal_no must be non-empty (advance-at-SEND online issuance)"
    );

    // BEGIN: still offline-shaped (the asymmetry is intentional).
    let (begin_fs_mode, begin_offline_no, _begin_sfn) =
        boundary_send_shape(app.db(), DocType::OfflineSessionBegin).await;
    assert_eq!(
        begin_fs_mode, "OFFLINE",
        "BEGIN stays offline-shaped (minted while OFFLINE, opens the session)"
    );
    assert!(
        begin_offline_no.is_some(),
        "BEGIN must stamp offline_fiscal_no (offline-shaped <MAC ID>): {begin_offline_no:?}"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// #7 — crash-idempotent drain: re-running drain after a converged session does
// NOT re-mint / re-send / re-consume-code the boundary docs.
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_redrain_after_convergence_does_not_double_mint_or_double_consume() {
    let (app, _c1) = run_full_offline_drain(3).await;

    let codes_before = consumed_code_count(app.db()).await;
    let begin_before = count_doc_type(app.db(), DocType::OfflineSessionBegin).await;
    let end_before = count_doc_type(app.db(), DocType::OfflineSessionEnd).await;

    // Re-run the drain tick (idempotent re-entry — session already CLOSED /
    // node ONLINE).  Must not mint a 2nd BEGIN/END or consume a 2nd code.
    let c2 = drain_carriers(0);
    let view = drain_view(&c2);
    let _ = app.drain_offline_backlog_scheduled(FN, &view).await;

    assert_eq!(
        consumed_code_count(app.db()).await,
        codes_before,
        "re-drain must NOT consume additional codes (INV-5)"
    );
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        begin_before,
        "re-drain must NOT re-mint the BEGIN"
    );
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionEnd).await,
        end_before,
        "re-drain must NOT re-mint the END"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;
}

// ══════════════════════════════════════════════════════════════════════════
// Finding B (Pattern C) — the FIRST offline doc of a session can be an offline
// SHIFT_OPEN (edge 2), NOT a SELL.  The lazy BEGIN must STILL be the lowest-lnd
// offline doc, so the drain sends [9, SHIFT_OPEN, …, 10].  RED before the fix
// (BEGIN was gated to Sell/Return → SHIFT_OPEN minted at lnd=1 with no BEGIN →
// drain would send SHIFT_OPEN before the DPS offline window opened → DPS -5).
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_offline_shift_open_first_doc_still_mints_begin_lowest_lnd() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    // Pattern C: the shift opens OFFLINE (no prior online SHIFT_OPEN).
    prro::admin::go_offline(app.db(), FN, "morning without network")
        .await
        .expect("go_offline");
    let codes: Vec<String> = (0..6).map(|i| format!("CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed codes");

    // The FIRST offline doc is a SHIFT_OPEN (edge 2, Created→OpenedLocalPendingDrain).
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-c-OPEN", None),
    )
    .await
    .expect("offline SHIFT_OPEN must local-ack");
    assert_eq!(open.document_state, DocState::OfflineLocalAck);

    // The lazy BEGIN must exist AND be the LOWEST lnd (below the SHIFT_OPEN).
    let docs = doc_types_by_lnd(app.db()).await;
    let begin = docs
        .iter()
        .find(|(_, dt, _)| dt == "OFFLINE_SESSION_BEGIN")
        .unwrap_or_else(|| {
            panic!("a DocType=9 BEGIN must be minted for offline SHIFT_OPEN: {docs:?}")
        });
    let so = docs
        .iter()
        .find(|(_, dt, _)| dt == "SHIFT_OPEN")
        .expect("the offline SHIFT_OPEN row");
    assert!(
        begin.0 < so.0,
        "BEGIN(lnd={}) must precede the offline SHIFT_OPEN(lnd={}) so it drains FIRST: {docs:?}",
        begin.0,
        so.0
    );
    assert_eq!(
        count_doc_type(app.db(), DocType::OfflineSessionBegin).await,
        1
    );

    // Drain: wire order must be [9(ServiceChk) FIRST, SHIFT_OPEN, 10 LAST].
    prro::admin::go_online(app.db(), FN, "restored")
        .await
        .expect("go_online");
    let carriers = drain_carriers(3); // BEGIN + SHIFT_OPEN + END
    let view = drain_view(&carriers);
    let outcome = app
        .drain_offline_backlog_scheduled(FN, &view)
        .await
        .expect("drain must run");
    match outcome {
        ScheduledDrainOutcome::Ran(s) => assert!(s.finalized(), "drain finalizes"),
        ScheduledDrainOutcome::SkippedBackoff { .. } => panic!("first tick must run"),
    }
    let order = carriers.dps.wire_order();
    assert!(order.len() >= 3, "expected >= 3 wire sends, got {order:?}");
    // FIRST is the BEGIN (ServiceChk) — it opens the DPS offline window BEFORE
    // the SHIFT_OPEN is sent (the whole point: no -5 on the SHIFT_OPEN).
    assert_eq!(
        order.first().unwrap().0,
        DpsCheckType::ServiceChk,
        "FIRST wire send must be the BEGIN (opens the DPS offline window): {order:?}"
    );
    assert_eq!(
        order.last().unwrap().0,
        DpsCheckType::ServiceChk,
        "LAST wire send must be the END: {order:?}"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;
}

// ══════════════════════════════════════════════════════════════════════════
// HOLE A (#192-class wedge) — a crash-stuck SIGNED BEGIN (durably SIGNED, seed
// NOT advanced: the sign↔OLA crash window) must NOT be signed-past by an
// incoming offline Z_REPORT (offline-Z surface is live).  If the Z signed past
// it and advanced the seed, boot-resume driving the BEGIN to OLA would hit the
// seed-drift assert and bail → BEGIN wedged SIGNED forever = undrainable
// session (#192 violation).  The first-offline-doc guard must fail-closed
// REGARDLESS of doc type.  RED before the fix (Z bypassed the BEGIN guard).
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_offline_z_does_not_sign_past_a_crash_stuck_begin() {
    use prro::runtime::ingress::seam::FiscalError;

    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(app.clone(), Arc::new(registry), std::sync::Arc::new(prro::services::time_budget::FixedClock::from_rfc3339("2026-07-07T12:30:00Z")));

    drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-z-OPEN", None),
    )
    .await
    .expect("online SHIFT_OPEN");
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("go_offline");
    let codes: Vec<String> = (0..6).map(|i| format!("CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(app.db(), FN, &codes)
        .await
        .expect("seed codes");

    // Inject a crash-stuck SIGNED BEGIN: a durably-SIGNED boundary doc bound to
    // the shift with offline_session_id NULL + NO code stamp (exactly what a
    // crash between sign and offline-ack leaves), FN seed NOT advanced.
    let shift_id: Vec<u8> =
        sqlx::query_scalar("SELECT current_shift_id FROM node_state WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(app.db())
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO fiscal_documents \
         (document_id, request_id, fiscal_number, shift_id, offline_session_id, lnd, doc_type, \
          state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
          payload_sha256_canonical, source_sha256, unsigned_xml_sha256, previous_hash) \
         VALUES (randomblob(16), randomblob(16), ?, ?, NULL, 900, 'OFFLINE_SESSION_BEGIN', \
          'SIGNED', 'b', 't', 'OFFLINE', '2026-07-07T00:00:00Z', '{}', \
          zeroblob(32), zeroblob(32), zeroblob(32), NULL)",
    )
    .bind(FN)
    .bind(&shift_id)
    .execute(app.db())
    .await
    .unwrap();

    // An incoming offline Z_REPORT must NOT sign past the stuck SIGNED BEGIN.
    let z_res = drive(
        &*write_path,
        app.db(),
        entry("Z_REPORT", r#"{}"#, "idem-z-Z", None),
    )
    .await;
    assert!(
        matches!(z_res, Err(FiscalError::OfflineRefused { .. })),
        "offline Z must fail-closed while a BEGIN is stuck below OLA, got {z_res:?}"
    );
    // No Z row minted past the non-issued BEGIN (no wedge).
    let z_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND doc_type = 'Z_REPORT'",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(
        z_rows, 0,
        "no Z_REPORT row may be signed past a non-issued BEGIN"
    );
}
