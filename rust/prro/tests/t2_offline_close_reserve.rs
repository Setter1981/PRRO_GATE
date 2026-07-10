//! T2 — offline code close-reserve (RULING 3.5; operator «пиздец важно» 2026-07-08).
//!
//! INVARIANT PINNED HERE: **«a shift is NEVER wedged un-closable for lack of a
//! code»**. An ordinary offline op (SELL/RETURN) is refused fail-closed PRE-MINT
//! (row-less 503 `FiscalError::OfflineRefused`) when granting its code would
//! leave fewer free codes than the dynamic legal close-reserve needs. Close-path
//! ops (offline Z_REPORT, the lazy DocType=9 BEGIN mint, the DocType=10 END)
//! ALWAYS may draw the reserve — they are never blocked by this gate.
//!
//! Dynamic reserve (per T2 contract, reduced form — see arch adjudication):
//!   admit an ordinary offline SELL/RETURN  ⟺  free_codes >= 1 + reserve
//!   reserve = (session BEGIN missing ? 1 : 0) + (offline Z still needed ? 1 : 0)
//! where BEGIN is "present" only in the ISSUED set {OLA,Sent,Kvt1,Kvt2,Ack}
//! (a terminal-FAILED / absent BEGIN counts as missing — it will be re-minted),
//! and "offline Z still needed" ⟺ shift_state ∈ {Opened, OpenedLocalPendingDrain}.
//!
//! RED-first (strict TDD): these pins FAIL at T1 base — today the offline SELL
//! CONSUMES a code from the pool with no reserve gate, so the "refused + pool
//! unchanged + no row minted" assertions are RED. The GREEN commit adds the gate
//! in `run_staged` (before the lazy-BEGIN mint). The teeth pin reverts the gate.
//!
//! Harness idioms mirror `pilot_offline_full_drill_e2e.rs`: boot a real `App`,
//! wire the live `production_write_path` binding + a `StubDpsChannel`, open the
//! shift ONLINE, GO_OFFLINE via the live door, seed a precise pool, then drive
//! offline ops through `wp.fiscalize`.

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
use prro::runtime::ingress::inline_binding::production_write_path;
use prro::runtime::ingress::seam::{FiscalError, FiscalOutcome, WritePathEntry};
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use sqlx::SqlitePool;

use common::{ack, det_signing_ctx_for, StubDpsChannel};

const FN: &str = "4000000019";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

// ─── boot + registry harness (mirrors pilot_offline_full_drill_e2e.rs) ───────

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
    let db_path = dir.path().join("t2_reserve.db");
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
        // Refill watermarks — DELIBERATELY 0 so they can NEVER stand in for the
        // legal close-reserve (the T2 gate is orthogonal to replenish thresholds).
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

/// The registry DPS acks ONLY the online SHIFT_OPEN — offline SELL/RETURN/Z never
/// touch DPS.
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
    e: NewInboxEntry,
) -> Result<FiscalOutcome, FiscalError> {
    let row: InboxRow = match inbox::insert(pool, &e).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await
}

// ─── probes ──────────────────────────────────────────────────────────────────

async fn free_codes(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes \
         WHERE fiscal_number = ? AND consumed_at IS NULL AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn consumed_codes(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn doc_count_by_type(pool: &SqlitePool, doc_type: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND doc_type = ?",
    )
    .bind(FN)
    .bind(doc_type)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn doc_count_by_request_id(pool: &SqlitePool, request_id: &[u8; 16]) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE request_id = ?")
        .bind(&request_id[..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn doc_count_in_state(pool: &SqlitePool, state: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND state = ?")
        .bind(FN)
        .bind(state)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY rowid DESC LIMIT 1")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Boot → online SHIFT_OPEN (Opened) → GO_OFFLINE (live door: mode=OFFLINE +
/// OPEN offline session). Returns the wired write-path. Leaves the pool EMPTY —
/// each test seeds its own precise count next.
async fn boot_offline_opened_shift() -> (App, Arc<dyn WritePathEntry>) {
    let app = boot_app().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path(app.clone(), Arc::new(registry));

    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-t2-OPEN", None),
    )
    .await
    .expect("online SHIFT_OPEN must ACK");
    assert_eq!(open.document_state, DocState::Ack);
    assert_eq!(shift_state(app.db()).await, "OPENED");

    prro::admin::go_offline(app.db(), FN, "operator net drop")
        .await
        .expect("live door: GO_OFFLINE opens the offline session");

    (app, write_path)
}

async fn seed_codes(pool: &SqlitePool, n: usize) {
    let codes: Vec<String> = (0..n).map(|i| format!("T2-CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(pool, FN, &codes)
        .await
        .expect("seed codes");
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 1 — pool=2, NO BEGIN yet, open shift → offline SELL REFUSED (would leave
//         free=1 < required close-reserve=2: BEGIN(1)+Z(1)); NO row/lnd/code.
//         This is the ORDERING pin: the gate must fire BEFORE the lazy BEGIN
//         mint, so the pool stays intact for the eventual BEGIN+Z close.
//         Today (T1 base) this is RED — the SELL mints the BEGIN + consumes.
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin1_pool2_no_begin_open_shift_offline_sell_refused_no_consume() {
    let (app, wp) = boot_offline_opened_shift().await;
    seed_codes(app.db(), 2).await;
    assert_eq!(free_codes(app.db()).await, 2, "precondition: pool=2");
    assert_eq!(
        doc_count_by_type(app.db(), "OFFLINE_SESSION_BEGIN").await,
        0,
        "precondition: no BEGIN minted yet"
    );

    let sell_entry = entry("SELL", SELL_PAYLOAD, "idem-t2-p1-SELL", Some(TOTAL_KOP));
    let sell_request_id = sell_entry.request_id;
    let res = drive(&*wp, app.db(), sell_entry).await;

    // Refused fail-closed with the row-less 503 family (see pin 5 for the code).
    assert!(
        matches!(res, Err(FiscalError::OfflineRefused { .. })),
        "SELL must be refused (close-reserve held), got {res:?}"
    );
    // NO code consumed — the pool is intact for BEGIN + Z.
    assert_eq!(free_codes(app.db()).await, 2, "pool must stay 2 (no draw)");
    assert_eq!(consumed_codes(app.db()).await, 0, "no code consumed");
    // NO row minted — not the SELL, and CRITICALLY not its lazy BEGIN either
    // (the gate fires BEFORE ensure_offline_session_begin).
    assert_eq!(
        doc_count_by_request_id(app.db(), &sell_request_id).await,
        0,
        "no fiscal_documents row for the refused SELL"
    );
    assert_eq!(
        doc_count_by_type(app.db(), "SELL").await,
        0,
        "no SELL row minted"
    );
    assert_eq!(
        doc_count_by_type(app.db(), "OFFLINE_SESSION_BEGIN").await,
        0,
        "the refused SELL must NOT have triggered a lazy BEGIN mint (ordering pin)"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 2 — SAME state (pool=2, no BEGIN, open shift) → offline Z_REPORT ALLOWED.
//         The Z is a close-path op: it draws the reserve. Driving it mints the
//         lazy BEGIN + the Z (both OFFLINE_LOCAL_ACK), consuming 2 codes, and
//         the shift moves off `Opened` (Pattern-C offline close).
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin2_pool2_no_begin_offline_z_allowed_draws_reserve_closes_shift() {
    let (app, wp) = boot_offline_opened_shift().await;
    seed_codes(app.db(), 2).await;
    assert_eq!(free_codes(app.db()).await, 2);

    let res = drive(&*wp, app.db(), entry("Z_REPORT", r#"{}"#, "idem-t2-p2-Z", None)).await;
    let outcome = res.expect("offline Z must be ALLOWED (close-path draws the reserve)");
    assert_eq!(
        outcome.document_state,
        DocState::OfflineLocalAck,
        "offline Z lands OFFLINE_LOCAL_ACK"
    );

    // BEGIN + Z both minted + acked-local → 2 codes consumed, pool drained.
    assert_eq!(
        doc_count_by_type(app.db(), "OFFLINE_SESSION_BEGIN").await,
        1,
        "the Z's first-offline-doc seam minted the lazy BEGIN"
    );
    assert_eq!(doc_count_by_type(app.db(), "Z_REPORT").await, 1, "Z minted");
    assert_eq!(
        doc_count_in_state(app.db(), "OFFLINE_LOCAL_ACK").await,
        2,
        "BEGIN + Z both OFFLINE_LOCAL_ACK"
    );
    assert_eq!(free_codes(app.db()).await, 0, "both reserve codes drawn");
    // The offline Z drove the shift off `Opened` toward local close.
    assert_ne!(
        shift_state(app.db()).await,
        "OPENED",
        "offline Z closes the shift locally (leaves Opened)"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 3 — pool=1, BEGIN already issued → offline SELL REFUSED (would leave
//         free=0 < required=1: only Z left to reserve), but offline Z ALLOWED
//         (draws the last code). BEGIN is pre-issued by driving one Z-less
//         offline SELL first with a fatter pool, then trimming — but the
//         cleanest construction is: seed pool=3, drive an offline SELL (mints
//         BEGIN + SELL, pool→1), then assert a SECOND SELL is refused while a Z
//         is allowed on that remaining single code.
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin3_pool1_begin_present_sell_refused_z_allowed() {
    let (app, wp) = boot_offline_opened_shift().await;
    // Seed 3: the first offline SELL mints BEGIN (1) + itself (1) → pool 3→1,
    // and free=1 with reserve=1 (BEGIN present, Z needed) is the admit boundary
    // (1 >= 1 + 0? no — see below). To reach "BEGIN present, pool=1" we seed 3
    // and let the first SELL consume BEGIN+SELL, leaving exactly 1.
    seed_codes(app.db(), 3).await;
    let first = drive(
        &*wp,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-t2-p3-SELL-A", Some(TOTAL_KOP)),
    )
    .await
    .expect("first offline SELL admitted (free=3 >= 1 + reserve=2)");
    assert_eq!(first.document_state, DocState::OfflineLocalAck);
    assert_eq!(
        doc_count_by_type(app.db(), "OFFLINE_SESSION_BEGIN").await,
        1,
        "BEGIN now issued"
    );
    assert_eq!(free_codes(app.db()).await, 1, "pool trimmed to 1 (BEGIN+SELL)");

    // Second SELL: free=1, BEGIN present (reserve = 0+Z(1) = 1) → need 1+1=2 > 1
    // → REFUSE, no consume.
    let sell2 = entry("SELL", SELL_PAYLOAD, "idem-t2-p3-SELL-B", Some(TOTAL_KOP));
    let sell2_rid = sell2.request_id;
    let res = drive(&*wp, app.db(), sell2).await;
    assert!(
        matches!(res, Err(FiscalError::OfflineRefused { .. })),
        "second offline SELL must be refused (last code reserved for Z), got {res:?}"
    );
    assert_eq!(free_codes(app.db()).await, 1, "pool still 1 (Z's reserve)");
    assert_eq!(
        doc_count_by_request_id(app.db(), &sell2_rid).await,
        0,
        "no row for the refused second SELL"
    );

    // Z on that same single code → ALLOWED (close-path draws the reserve).
    let z = drive(&*wp, app.db(), entry("Z_REPORT", r#"{}"#, "idem-t2-p3-Z", None))
        .await
        .expect("offline Z ALLOWED on the last reserved code");
    assert_eq!(z.document_state, DocState::OfflineLocalAck);
    assert_eq!(free_codes(app.db()).await, 0, "Z drew the last code");
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 4 — the close-reserve NEVER blocks ONLINE ops. An ONLINE SELL on a fat
//         pool proceeds normally (the reserve gate is offline-scoped). Prove by
//         driving an online SELL right after the online SHIFT_OPEN (no
//         GO_OFFLINE), with pool=0 (irrelevant online): it ACKs.
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin4_reserve_never_blocks_online_ops() {
    let app = boot_app().await;
    // Online DPS acks SHIFT_OPEN + one online SELL (send_chk + last_chk each).
    let sends = vec![Ok(ack("DPS-OPEN")), Ok(ack("DPS-SELL"))];
    let lasts = vec![Ok(kvt1("DPS-OPEN")), Ok(kvt1("DPS-SELL"))];
    let registry = build_registry(
        &app,
        Arc::new(StubDpsChannel::with_queue(sends).with_last_chk_queue(lasts)),
    )
    .await;
    seed_boot_baseline(app.db()).await;
    let wp = production_write_path(app.clone(), Arc::new(registry));

    drive(
        &*wp,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-t2-p4-OPEN", None),
    )
    .await
    .expect("online SHIFT_OPEN ACK");

    // Node stays ONLINE, pool is empty — the reserve gate must NOT even look.
    assert_eq!(free_codes(app.db()).await, 0, "empty pool, but online");
    let sell = drive(
        &*wp,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-t2-p4-SELL", Some(TOTAL_KOP)),
    )
    .await
    .expect("online SELL must NOT be blocked by the offline close-reserve");
    assert_eq!(
        sell.document_state,
        DocState::Ack,
        "online SELL ACKs regardless of the (empty) offline pool"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 5 — the refusal is the ROW-LESS 503 family (audit-only), consistent with
//         the shift-class lane: exact code == codes::OFFLINE_CODE_RESERVE_HELD,
//         and it maps to a retryable 503 (same class as
//         OFFLINE_SESSION_BEGIN_PENDING). No fiscal_documents row; the inbox row
//         stays retryable (a fresh drive with the SAME state must refuse again).
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin5_refusal_is_rowless_503_reserve_held_code() {
    let (app, wp) = boot_offline_opened_shift().await;
    seed_codes(app.db(), 2).await;

    let res = drive(
        &*wp,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-t2-p5-SELL", Some(TOTAL_KOP)),
    )
    .await;
    match res {
        Err(FiscalError::OfflineRefused { code, .. }) => {
            // Stable HTTP-facing code string (the `codes` module is crate-private;
            // the wire contract is the literal, round-trip-fenced in inline_map).
            assert_eq!(
                code, "OFFLINE_CODE_RESERVE_HELD",
                "the reserve refusal carries the dedicated OFFLINE_CODE_RESERVE_HELD code"
            );
        }
        other => panic!("expected OfflineRefused{{OFFLINE_CODE_RESERVE_HELD}}, got {other:?}"),
    }
    // Row-less: no fiscal_documents row at all for this FN's SELL/BEGIN.
    assert_eq!(doc_count_by_type(app.db(), "SELL").await, 0);
    assert_eq!(doc_count_by_type(app.db(), "OFFLINE_SESSION_BEGIN").await, 0);
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 6 (RESIDUAL-HOLE #2, arch adjudication) — NO active offline session →
//         the gate BYPASSES (admits). With node OFFLINE but the offline session
//         aborted/closed, there is no offline shift to protect; a low pool must
//         NOT trip the reserve gate (that path is governed by BEGIN/offline-ack,
//         not the close-reserve). The SELL proceeds online-shaped (defers at
//         offline-ack — established asymmetry), NOT a reserve refusal.
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin6_no_active_session_bypasses_reserve_gate() {
    let (app, wp) = boot_offline_opened_shift().await;
    seed_codes(app.db(), 1).await;
    // Abort the offline session so no OPEN session exists (node stays OFFLINE).
    sqlx::query(
        "UPDATE offline_sessions SET state = 'ABORTED', reason_abort = 't2-test' \
         WHERE fiscal_number = ? AND state = 'OPEN'",
    )
    .bind(FN)
    .execute(app.db())
    .await
    .unwrap();

    let res = drive(
        &*wp,
        app.db(),
        entry("SELL", SELL_PAYLOAD, "idem-t2-p6-SELL", Some(TOTAL_KOP)),
    )
    .await;
    // The reserve gate must NOT be the thing that refuses here. Whatever the
    // downstream outcome (defer / offline-ack path), it must NOT be an
    // OFFLINE_CODE_RESERVE_HELD refusal — that would mean the gate fired without
    // an offline session to protect.
    if let Err(FiscalError::OfflineRefused { code, .. }) = &res {
        assert_ne!(
            *code, "OFFLINE_CODE_RESERVE_HELD",
            "no active session → the close-reserve gate must BYPASS, not refuse"
        );
    }
}
