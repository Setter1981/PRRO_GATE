//! PR-Z2 step 5 — PILOT online-half lifecycle e2e THROUGH THE LIVE BINDING.
//!
//! This is the capstone integration proof for the live Z surface (PR-Z2): a
//! single `production_write_path` / `InlineWritePath` binding drives the WHOLE
//! online-half shift lifecycle end-to-end —
//!
//!   boot → SHIFT_OPEN → SELL → SELL → Z_REPORT (close)
//!
//! — with a real per-FN signing context (`FixtureLoader` cert → `build_fn_sign`
//! produces a well-formed CMS), the real per-FN gate (invariant #2), the real
//! registry resolution, and the real staged pipeline. No stage is called
//! directly; every command enters through `WritePathEntry::fiscalize`, exactly
//! as the ingress handler drives it in production.
//!
//! What makes this DISTINCT from the existing binding tests (`a3_final_binding_
//! flip.rs` seeds an already-`Opened` shift and fiscalizes one SELL): here the
//! shift is CREATED by driving a live `SHIFT_OPEN` (edges 1+3, PR-Z2 step 1) and
//! CLOSED by driving a live `Z_REPORT` (quiesce → aggregate → edges 8+10, PR-Z2
//! steps 2+4) — the two Z-surface legs this PR wired, composed over real SELLs,
//! proven to rest on a CLEAN ledger invariant scan at every quiescent boundary.
//!
//! RED-first is INHERITED, not re-staged: this capstone is green because the
//! legs it composes were each RED-first proven in steps 1–4 — `online_shift_
//! open_reaches_ack` (RED before the fail-closed SHIFT_OPEN arm was removed),
//! `online_z_report_closes_shift_reaches_ack` (RED before the flag flip), and
//! the tripwire `z_live_dispatch_is_gated_until_full_z_surface` (REDs the whole
//! Z surface on a flag revert). Reverting either leg REDs this e2e at that leg.
//!
//! PILOT SCOPE: online-half only. Offline open/drain (Pattern C) is deferred to
//! A′.3; this test never leaves `Online`.

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
use prro::transports::dps::dto::CheckAck;
use sqlx::SqlitePool;

use common::{ack, det_signing_ctx_for, StubDpsChannel};

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const Z_WIRE_INTENT: &str = r#"{}"#;
const TOTAL_KOP: i64 = 15000;
/// Real DER DSTU-4145 cert so the binding's per-FN `build_fn_sign` (real CMS
/// over the FN) succeeds — an empty-cert dummy session would fail the CMS build.
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

// ─── App boot + registry (mirrors a3_final_binding_flip scaffolding) ───────

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
    let db_path = dir.path().join("pilot.db");
    std::mem::forget(dir);
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    App::boot(cfg).await.unwrap()
}

/// Loader that returns a DetCrypto signing context whose session carries the
/// REAL cert fixture, so `build_fn_sign` succeeds and the staged pipeline signs.
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

/// The post-reconcile boot baseline: an Online node with a CLOSED shift state
/// and no open shift — exactly what the supervisor reconcile leaves for a fresh
/// FN before the first SHIFT_OPEN. (The reconcile bootstrap itself is covered by
/// `rs1_supervisor_boot.rs`; this test seeds the baseline explicitly to keep the
/// focus on the live shift lifecycle through the binding.)
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

// ─── DPS: happy multi-doc queue (1 send_chk + 1 last_chk per doc, in order) ─

/// A `last_chk` KVT1 evidence ack (non-empty `data_sign`) for `sfn`.
fn kvt1(sfn: &str) -> CheckAck {
    CheckAck {
        id: sfn.into(),
        id_sign: vec![],
        data_sign: vec![0xDE, 0xAD, 0xBE, 0xEF],
    }
}

/// A DPS stub that ACKs each doc in `sfns` order: `send_chk` → Sent (empty
/// data_sign), `last_chk` → KVT1 evidence. Each doc consumes exactly one of
/// each on the happy path, so the queue length must equal the doc count.
fn happy_multi_dps(sfns: &[&str]) -> Arc<dyn DpsChannel> {
    let sends = sfns.iter().map(|s| Ok(ack(s))).collect::<Vec<_>>();
    let lasts = sfns.iter().map(|s| Ok(kvt1(s))).collect::<Vec<_>>();
    Arc::new(StubDpsChannel::with_queue(sends).with_last_chk_queue(lasts))
}

// ─── Inbox entry builders ──────────────────────────────────────────────────

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
        business_ts: Some("2026-06-09T12:00:00Z".into()),
        total_sum_kop: total,
    }
}

/// Insert a fresh NEW inbox row and drive it through the LIVE binding.
async fn drive(
    wp: &dyn WritePathEntry,
    pool: &SqlitePool,
    entry: NewInboxEntry,
) -> Result<FiscalOutcome, FiscalError> {
    let row: InboxRow = match inbox::insert(pool, &entry).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await
}

// ─── Probes ─────────────────────────────────────────────────────────────────

/// The current shift's state (there is exactly one shift in this lifecycle).
async fn shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The node_state shift mirror (must track the shifts row — invariant-scan pins).
async fn node_shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn acked_doc_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND state = 'ACK'",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ════════════════════════════════════════════════════════════════════════
// PILOT online-half e2e
// ════════════════════════════════════════════════════════════════════════

/// The full online-half lifecycle through ONE live binding: open a shift, sell
/// two receipts, close with a Z — every leg reaching terminal ACK, the shift
/// mirror consistent, and the ledger invariant scan clean at every boundary.
#[tokio::test]
async fn pilot_online_half_open_sell_sell_z_close_via_live_binding() {
    let app = boot_app().await;
    // One ack per doc, in drive order: SHIFT_OPEN, SELL-1, SELL-2, Z_REPORT.
    let sfns = [
        "DPS-SFN-OPEN",
        "DPS-SFN-SELL-1",
        "DPS-SFN-SELL-2",
        "DPS-SFN-Z",
    ];
    let registry = build_registry(&app, happy_multi_dps(&sfns)).await;
    seed_boot_baseline(app.db()).await;

    // Construct the binding EXACTLY as the supervisor DI root does.
    let write_path = production_write_path(app.clone(), Arc::new(registry));

    // ─── 1) SHIFT_OPEN — edges 1 (Created→Opening) + 3 (Opening→Opened) ───
    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-pilot-OPEN", None),
    )
    .await
    .expect("SHIFT_OPEN must reach terminal ACK through the live binding");
    assert_eq!(open.document_state, DocState::Ack, "SHIFT_OPEN issued");
    assert_eq!(shift_state(app.db()).await, "OPENED", "shift is open");
    assert_eq!(
        node_shift_state(app.db()).await,
        "OPENED",
        "node_state shift mirror tracks the open shift"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 2) SELL ×2 — receipts on the open shift ──────────────────────────
    for (i, idem) in ["idem-pilot-SELL-1", "idem-pilot-SELL-2"]
        .iter()
        .enumerate()
    {
        let sell = drive(
            &*write_path,
            app.db(),
            entry("SELL", SELL_PAYLOAD, idem, Some(TOTAL_KOP)),
        )
        .await
        .unwrap_or_else(|e| panic!("SELL #{i} must reach terminal ACK: {e:?}"));
        assert_eq!(sell.document_state, DocState::Ack, "SELL #{i} issued");
    }
    // The shift stays OPEN across sales.
    assert_eq!(shift_state(app.db()).await, "OPENED");
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // ─── 3) Z_REPORT — quiesce (Clear) → aggregate → edges 8 + 10 (close) ──
    let z = drive(
        &*write_path,
        app.db(),
        entry("Z_REPORT", Z_WIRE_INTENT, "idem-pilot-Z", None),
    )
    .await
    .expect("Z_REPORT must close the shift and reach terminal ACK through the live binding");
    assert_eq!(z.document_state, DocState::Ack, "Z issued");
    assert_eq!(shift_state(app.db()).await, "CLOSED", "shift is closed");
    assert_eq!(
        node_shift_state(app.db()).await,
        "CLOSED",
        "node_state shift mirror tracks the close"
    );

    // The WHOLE online-half lifecycle rests terminal: 4 issued receipts
    // (OPEN + 2 SELL + Z), no doc in PREPARED/SIGNED/ENCRYPTED/SENDING, MAC
    // chain intact, shift mirror consistent — the ledger-pin the reaper /
    // boot-resume twins (#192/#196) exist to protect.
    assert_eq!(
        acked_doc_count(app.db()).await,
        4,
        "SHIFT_OPEN + 2 SELL + Z_REPORT all issued to ACK"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;
}
