//! T3 — the UNCONDITIONAL shift-limit auto-Z (RULING 3.4).
//!
//! CORE PIN (1): when the open shift reaches its 24h document-derived budget, the
//! gateway makes a DURABLE Z attempt REGARDLESS of any enforcement toggle — even
//! with `enforce_shift_24h = false` in config. "A shift never crosses the limit
//! without a durable Z attempt/outcome (success, or an escalated failure — RMR —
//! but never silent continuation)."
//!
//! TEETH (7): reverting the unconditional-ness (making the auto-Z respect the
//! enforcement toggle, i.e. NOT closing when the toggle is off) makes pin 1 RED.
//! The teeth pin asserts the observable that the revert would break: the auto-Z
//! ISSUES a Z with the shift-24h ENFORCEMENT TOGGLE OFF. A guard-revert that
//! consulted the toggle would return `NotDue` here → the assertion fails.
//!
//! Mechanism under test: `App::auto_z_close_over_limit_for_fn(fn, clock, view)` —
//! the ops-loop tick body. Driven with a `FixedClock` 24h+ε after the shift open
//! anchor; the OFFLINE Pattern-C path is used (deterministic, no DPS Z wire; the
//! T2 close-reserve guarantees the pool code).

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
use prro::services::reconciliation::RuntimeView;
use prro::services::time_budget::FixedClock;
use prro::services::write_path::stage_sign::SigningContext;
use prro::services::write_path::AutoZOutcome;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use sqlx::SqlitePool;

use common::{ack, det_signing_ctx_for, StubDpsChannel};

const FN: &str = "4000000019";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

const OPEN_TS: &str = "2026-07-07T12:00:00Z";
const CLOCK_FRESH: &str = "2026-07-07T12:30:00Z";
// 24h + 1s after the shift-open anchor → the 24h shift budget is over the wall.
const CLOCK_OVER_24H: &str = "2026-07-08T12:00:01Z";

/// A config with the 24h-shift ENFORCEMENT TOGGLE OFF — proves the auto-Z is
/// unconditional (it must still close, independent of enforcement).
fn cfg_toml_enforcement_off(db_path: &str) -> String {
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

[offline]
enforce_shift_24h = false
enforce_offline_session_36h = false
enforce_offline_month_168h = false
"#
    )
}

async fn boot_app_enforcement_off() -> App {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("t3_autoz.db");
    std::mem::forget(dir);
    let toml_text = cfg_toml_enforcement_off(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    // Sanity: the toggle really is OFF (the unconditional pin depends on it).
    assert!(
        !cfg.offline.enforce_shift_24h,
        "test premise: enforce_shift_24h must be OFF to prove auto-Z is unconditional"
    );
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
        StubDpsChannel::with_queue(vec![Ok(ack("DPS-SFN-OPEN"))])
            .with_last_chk_queue(vec![Ok(kvt1("DPS-SFN-OPEN"))]),
    )
}

fn shift_open_entry() -> NewInboxEntry {
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SHIFT_OPEN_PAYLOAD.as_bytes()).into();
    NewInboxEntry {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: "SHIFT_OPEN".into(),
        idempotency_key: "idem-t3-autoz-OPEN".into(),
        payload_json: SHIFT_OPEN_PAYLOAD.into(),
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some(OPEN_TS.into()),
        total_sum_kop: None,
    }
}

async fn drive(wp: &dyn WritePathEntry, pool: &SqlitePool, e: NewInboxEntry) -> FiscalOutcome {
    let row: InboxRow = match inbox::insert(pool, &e).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await.expect("drive must succeed")
}

async fn seed_codes(pool: &SqlitePool, n: usize) {
    let codes: Vec<String> = (0..n).map(|i| format!("T3AZ-CODE-{i}")).collect();
    prro::admin::seed_dps_offline_codes(pool, FN, &codes)
        .await
        .expect("seed codes");
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

/// Boot → online SHIFT_OPEN (Opened, business_ts=OPEN_TS) → GO_OFFLINE → seed a
/// code. Returns the App (the offline shift is now 24h-agnostic; the auto-Z tick
/// reads the injected clock). Enforcement is OFF in config.
async fn boot_offline_opened_shift_enforcement_off() -> App {
    let app = boot_app_enforcement_off().await;
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    // Open the shift with a FRESH clock (so the open itself isn't gated — moot
    // since enforcement is off, but keeps the open clean).
    let write_path = production_write_path_with_clock(
        app.clone(),
        Arc::new(registry),
        Arc::new(FixedClock::from_rfc3339(CLOCK_FRESH)),
    );
    let open = drive(&*write_path, app.db(), shift_open_entry()).await;
    assert_eq!(open.document_state, DocState::Ack);
    assert_eq!(shift_state(app.db()).await, "OPENED");
    prro::admin::go_offline(app.db(), FN, "net drop")
        .await
        .expect("GO_OFFLINE opens the offline session");
    seed_codes(app.db(), 2).await; // enough for BEGIN + Z (T2 reserve)
    app
}

/// Build a `RuntimeView` for the auto-Z tick body (offline path signs locally, so
/// a dummy `CheckSignBlob` suffices; the DPS stub is never hit on the offline Z).
fn offline_view() -> (Arc<dyn DpsChannel>, SigningContext, CheckSignBlob) {
    let dps: Arc<dyn DpsChannel> = Arc::new(StubDpsChannel::with_queue(vec![]));
    let sign_ctx = det_signing_ctx_for("OP-1");
    let fn_sign = CheckSignBlob(vec![0xABu8, 0xCD]);
    (dps, sign_ctx, fn_sign)
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 1 (core) — shift at 24h+ε, enforcement toggle OFF → the auto-Z makes a
//                DURABLE Z attempt (Z issued, shift leaves OPENED). This is the
//                UNCONDITIONAL guarantee: it fires even though enforcement is
//                off. RED at base (no auto-Z ticker exists → the shift just sits
//                over the wall with no Z).
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin1_unconditional_auto_z_issues_durable_z_even_with_enforcement_off() {
    let app = boot_offline_opened_shift_enforcement_off().await;
    let (dps, sign_ctx, fn_sign) = offline_view();
    let view = RuntimeView {
        dps: dps.as_ref(),
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    // The clock is 24h+1s past the shift-open anchor → the 24h budget is crossed.
    let clock = FixedClock::from_rfc3339(CLOCK_OVER_24H);

    assert_eq!(
        doc_count_by_type(app.db(), "Z_REPORT").await,
        0,
        "precondition: no Z yet"
    );

    let outcome = app
        .auto_z_close_over_limit_for_fn(FN, &clock, &view)
        .await
        .expect("auto-Z tick must not error");

    // The core assertion: a DURABLE Z was issued — UNCONDITIONALLY (toggle off).
    assert_eq!(
        outcome,
        AutoZOutcome::Issued,
        "the 24h auto-Z is UNCONDITIONAL — it issues a durable Z even with \
         enforce_shift_24h = false"
    );
    // The Z is real + durable (OFFLINE_LOCAL_ACK, Pattern C), and the shift left
    // OPENED toward local close — a durable outcome, not silent continuation.
    assert_eq!(
        doc_count_by_type(app.db(), "Z_REPORT").await,
        1,
        "one Z minted"
    );
    assert_ne!(
        shift_state(app.db()).await,
        "OPENED",
        "the shift is being closed (left OPENED) — the 24h wall did not silently pass"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 1b — UNDER 24h → the auto-Z is NotDue (no Z, shift stays OPENED). Bounds
//          the trigger: it fires at the boundary, not before.
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin1b_under_24h_auto_z_not_due() {
    let app = boot_offline_opened_shift_enforcement_off().await;
    let (dps, sign_ctx, fn_sign) = offline_view();
    let view = RuntimeView {
        dps: dps.as_ref(),
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    // 30min after open — well under 24h.
    let clock = FixedClock::from_rfc3339(CLOCK_FRESH);
    let outcome = app
        .auto_z_close_over_limit_for_fn(FN, &clock, &view)
        .await
        .expect("auto-Z tick must not error");
    assert_eq!(outcome, AutoZOutcome::NotDue, "under 24h → nothing to do");
    assert_eq!(
        doc_count_by_type(app.db(), "Z_REPORT").await,
        0,
        "no Z minted"
    );
    assert_eq!(shift_state(app.db()).await, "OPENED", "shift stays OPENED");
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 1c — idempotent re-tick: after the auto-Z issued the Z, a SECOND tick does
//          not mint a second Z (deterministic idempotency key; INV-7). The shift
//          has left OPENED → NotDue on the re-tick.
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin1c_auto_z_re_tick_is_idempotent() {
    let app = boot_offline_opened_shift_enforcement_off().await;
    let (dps, sign_ctx, fn_sign) = offline_view();
    let view = RuntimeView {
        dps: dps.as_ref(),
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let clock = FixedClock::from_rfc3339(CLOCK_OVER_24H);

    let first = app
        .auto_z_close_over_limit_for_fn(FN, &clock, &view)
        .await
        .unwrap();
    assert_eq!(first, AutoZOutcome::Issued);
    assert_eq!(doc_count_by_type(app.db(), "Z_REPORT").await, 1);

    // Second tick: the shift has left OPENED (Pattern-C local close) → NotDue,
    // and NO second Z is minted.
    let second = app
        .auto_z_close_over_limit_for_fn(FN, &clock, &view)
        .await
        .unwrap();
    assert_eq!(
        second,
        AutoZOutcome::NotDue,
        "a re-tick after the Z closed the shift is NotDue (idempotent)"
    );
    assert_eq!(
        doc_count_by_type(app.db(), "Z_REPORT").await,
        1,
        "still exactly one Z — no double-Z"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 7 (🦷 TEETH) — the auto-Z ISSUES with the shift-24h ENFORCEMENT TOGGLE OFF.
//   This is the exact observable a "make auto-Z respect the toggle" revert would
//   break: a guard that returned `NotDue` when `enforce_shift_24h == false` would
//   fail this assertion (no Z, shift still OPENED). The unconditional-ness IS the
//   teeth. (Distinct from pin 1 in intent: pin 1 asserts the durable Z happens;
//   pin 7 pins that the toggle-off path is NOT a bypass — reverting to a
//   toggle-gated auto-Z REDs here.)
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin7_teeth_auto_z_ignores_enforcement_toggle() {
    let app = boot_offline_opened_shift_enforcement_off().await;
    // Assert the premise the teeth depend on: the toggle really is OFF.
    assert!(
        !app.config().offline.enforce_shift_24h,
        "teeth premise: enforce_shift_24h is OFF"
    );
    let (dps, sign_ctx, fn_sign) = offline_view();
    let view = RuntimeView {
        dps: dps.as_ref(),
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let clock = FixedClock::from_rfc3339(CLOCK_OVER_24H);

    let outcome = app
        .auto_z_close_over_limit_for_fn(FN, &clock, &view)
        .await
        .unwrap();

    // If someone reverts the unconditional-ness (auto-Z consults the toggle and
    // skips when off), this is NotDue with no Z → these assertions RED.
    assert_eq!(
        outcome,
        AutoZOutcome::Issued,
        "TEETH: auto-Z must IGNORE the enforcement toggle (unconditional) — a \
         toggle-gated revert REDs here"
    );
    assert_eq!(
        doc_count_by_type(app.db(), "Z_REPORT").await,
        1,
        "TEETH: a durable Z exists despite the toggle being off"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 1d — the ONLINE auto-Z leg (RULING 3.4 "online → normal Z dispatch"). A
//          shift over 24h on an ONLINE node → the auto-Z drives a NORMAL online
//          Z (quiesce Clear → aggregate → edges 8+10) to terminal ACK. Proves
//          the auto-Z is not offline-only; it reuses run_z_dispatch's online path
//          (the same one pilot_online_half_e2e exercises via ingress).
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin1d_online_auto_z_issues_normal_z_to_ack() {
    // Default config (enforcement ON — moot; the auto-Z ignores it). ONLINE node.
    let app = {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t3_autoz_online.db");
        std::mem::forget(dir);
        let toml = format!(
            "app_name=\"prro\"\nversion=\"0.1.0\"\n[database]\ndb_path=\"{}\"\nsecure_db_path=\"{}_secure\"\n[admin_ui]\nenabled=false\nlisten=\"127.0.0.1:8445\"\n",
            db_path.display().to_string().replace('\\', "/"),
            db_path.display().to_string().replace('\\', "/"),
        );
        App::boot(AppConfig::from_toml(&toml).unwrap())
            .await
            .unwrap()
    };
    // DPS acks SHIFT_OPEN (via write path) — the online Z is driven by the tick's
    // OWN RuntimeView DPS below, so this registry DPS only needs the SHIFT_OPEN.
    let registry = build_registry(&app, shift_open_only_dps()).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path_with_clock(
        app.clone(),
        Arc::new(registry),
        Arc::new(FixedClock::from_rfc3339(CLOCK_FRESH)),
    );
    let open = drive(&*write_path, app.db(), shift_open_entry()).await;
    assert_eq!(open.document_state, DocState::Ack);
    assert_eq!(shift_state(app.db()).await, "OPENED");
    // Node stays ONLINE (no go_offline) — the auto-Z takes the online Z leg.

    // The tick's RuntimeView: a DPS that acks the online Z (send_chk + last_chk).
    let z_dps: Arc<dyn DpsChannel> = Arc::new(
        StubDpsChannel::with_queue(vec![Ok(ack("DPS-Z"))])
            .with_last_chk_queue(vec![Ok(kvt1("DPS-Z"))]),
    );
    let sign_ctx = det_signing_ctx_for("OP-1");
    let fn_sign = CheckSignBlob(vec![0xABu8, 0xCD]);
    let view = RuntimeView {
        dps: z_dps.as_ref(),
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let clock = FixedClock::from_rfc3339(CLOCK_OVER_24H);

    let outcome = app
        .auto_z_close_over_limit_for_fn(FN, &clock, &view)
        .await
        .expect("online auto-Z tick must not error");
    assert_eq!(
        outcome,
        AutoZOutcome::Issued,
        "the ONLINE auto-Z issues a normal Z at the 24h boundary"
    );
    // The online Z reached terminal ACK and the shift is CLOSED (edges 8→10).
    assert_eq!(
        doc_count_by_type(app.db(), "Z_REPORT").await,
        1,
        "one Z minted"
    );
    let z_state: String = sqlx::query_scalar(
        "SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND doc_type = 'Z_REPORT' LIMIT 1",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(z_state, "ACK", "the online Z reached terminal ACK");
    assert_eq!(
        shift_state(app.db()).await,
        "CLOSED",
        "the shift is CLOSED via the normal online Z (edges 8+10)"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// PIN 1e (DURABILITY on a build-reject) — if the recovered identity is somehow
//         unusable (the SHIFT_OPEN inbox row's driver_id is NULL — should never
//         happen since ingress listeners always stamp it, but defense-in-depth),
//         the auto-Z's Z build REJECTS → run_z_dispatch terminalises the inbox +
//         returns Err → auto_z maps to `Escalated` (DURABLE, never silent, never
//         panic). RULING 3.4: failure → existing route, never silent continuation.
// ════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn pin1e_null_driver_reject_escalates_durably_never_silent() {
    let app = boot_offline_opened_shift_enforcement_off().await;
    // Corrupt the recovered identity: NULL the SHIFT_OPEN inbox row's driver_id.
    let n = sqlx::query(
        "UPDATE ingress_inbox SET driver_id = NULL \
         WHERE fiscal_number = ? AND operation_type = 'SHIFT_OPEN'",
    )
    .bind(FN)
    .execute(app.db())
    .await
    .unwrap()
    .rows_affected();
    assert_eq!(n, 1, "nulled the SHIFT_OPEN driver_id");

    let (dps, sign_ctx, fn_sign) = offline_view();
    let view = RuntimeView {
        dps: dps.as_ref(),
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let clock = FixedClock::from_rfc3339(CLOCK_OVER_24H);

    let outcome = app
        .auto_z_close_over_limit_for_fn(FN, &clock, &view)
        .await
        .expect("auto-Z tick must not error (a build-reject is a durable Escalated, not a panic)");

    // DURABLE, not silent: an Escalated outcome (the Z build rejected, the row was
    // terminalised) — NEVER Issued, NEVER a silent NotDue that would leave the
    // shift over the wall with no attempt recorded.
    assert!(
        matches!(outcome, AutoZOutcome::Escalated { .. }),
        "a build-reject auto-Z must ESCALATE durably, got {outcome:?}"
    );
}
