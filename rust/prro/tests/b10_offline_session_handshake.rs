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
use prro::runtime::ingress::inline_binding::production_write_path;
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

// ══════════════════════════════════════════════════════════════════════════
// #4 (HEADLINE) — drain wire order is [9, SHIFT_OPEN? content…, 10]
// ══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn b10_drain_sends_begin_first_content_then_end_last() {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path(app.clone(), Arc::new(registry));

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
    let write_path = production_write_path(app.clone(), Arc::new(registry));

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
    let write_path = production_write_path(app.clone(), Arc::new(registry));

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
    let write_path = production_write_path(app.clone(), Arc::new(registry));

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
    let write_path = production_write_path(app.clone(), Arc::new(registry));

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
