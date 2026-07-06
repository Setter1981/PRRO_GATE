//! A.3-FINAL (RS-3 A2.4) — production binding flip: `InlineWritePath`.
//!
//! Drives the FLIPPED write-path binding (`InlineWritePath`, replacing
//! `UnimplementedWritePath` at the supervisor DI root) end-to-end: the binding
//! resolves per-FN deps from a boot-built `BindingsRegistry` (mock DPS + a
//! deterministic signing context), acquires the per-FN gate, and delegates to
//! `inline::run`.  These are the A2.4 gate tests:
//!
//! - 2d — first real fiscalization through the binding (SELL → ACK) + replay
//!   idempotency + `NotImplemented` is gone + Noop-unreachability (a fresh NEW
//!   row leases NEW→PROCESSING and Proceeds, never Noop);
//! - 2c — the four-variant inbox-terminalise gate THROUGH the binding: every
//!   real-failure arm leaves the inbox non-`NEW` (REJECTED) — the HARD merge
//!   gate (seam.rs `WritePathEntry` inbox-lifecycle obligation).
//!
//! RED-first: written against the `InlineWritePath` NotImplemented stub (= the
//! pre-flip behaviour), then GREEN once `fiscalize` delegates to `inline::run`.

mod common;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::app::App;
use prro::config::AppConfig;
use prro::crypto::errors::{CryptoError, SignKind};
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{
    self as inbox, InboxInsertOutcome, InboxRow, NewInboxEntry,
};
use prro::db::repositories::{fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::ingress::inline_binding::{production_write_path, InlineWritePath};
use prro::runtime::ingress::seam::{FiscalError, WritePathEntry};
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::CheckAck;
use prro::transports::dps::error::{AuthorizationKind, DpsError};
use sqlx::SqlitePool;

use common::{ack, det_signing_ctx_for, StubDpsChannel};

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SFN: &str = "DPS-FN-ONLINE-1";
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;
/// A real DER DSTU-4145 cert fixture (shared with `rs1_build_fn_sign.rs`) so the
/// binding's `build_fn_sign` (real CMS over the FN) succeeds — a `[0u8;32]` +
/// empty-cert dummy session would fail the CMS build.
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

// ─── App boot ────────────────────────────────────────────────────────────

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
listen  = "127.0.0.1:8443"
"#
    )
}

async fn boot_app() -> App {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a3final.db");
    std::mem::forget(dir);
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    App::boot(cfg).await.unwrap()
}

// ─── Loaders (registry sign_ctx source) ──────────────────────────────────

/// Returns a deterministic OK signing context (crypto always signs).
struct OkLoader;
#[async_trait]
impl OperatorKeyLoader for OkLoader {
    async fn load(
        &self,
        operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        // DetCrypto provider (drives stage_sign's CMS) + a REAL test key/cert in
        // the session so the binding's `build_fn_sign` (real DSTU-CMS) succeeds.
        let mut ctx = det_signing_ctx_for(operator_id);
        ctx.session = SigningSession::new_for_test(
            operator_id.to_string(),
            [7u8; 32],
            FIXTURE_CERT_DER.to_vec(),
        );
        Ok(ctx)
    }
}

/// `sign_cms_detached` always errors → drives the SignFailure gate arm.
struct FailingCrypto;
#[async_trait]
impl CryptoProvider for FailingCrypto {
    async fn sign_cms_detached(
        &self,
        _: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        Err(CryptoError::CmsSign {
            reason: SignKind::BackendError,
        })
    }
    async fn verify_dstu(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unreachable!("stub: verify_dstu not exercised")
    }
    async fn unwrap_envelope(
        &self,
        _: &[u8],
        _: &[u8],
        _: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unreachable!("stub: unwrap_envelope not exercised")
    }
    async fn fetch_cert_by_ski(
        &self,
        _: &[String],
        _: &[u8; 32],
        _: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unreachable!("stub: fetch_cert_by_ski not exercised")
    }
}

/// Loader whose signing context fails at the crypto boundary.
struct FailingSignLoader;
#[async_trait]
impl OperatorKeyLoader for FailingSignLoader {
    async fn load(
        &self,
        operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        // Valid session (build_fn_sign succeeds) but a FailingCrypto provider,
        // so the refusal surfaces at stage_sign's CMS boundary (SignFailure),
        // NOT earlier at fn_sign build.
        Ok(SigningContext {
            provider: Arc::new(FailingCrypto) as Arc<dyn CryptoProvider>,
            session: SigningSession::new_for_test(
                operator_id.to_string(),
                [7u8; 32],
                FIXTURE_CERT_DER.to_vec(),
            ),
            profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
        })
    }
}

// ─── Registry build (seeds FN config + operator into the App's pools) ─────

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

async fn build_registry(
    app: &App,
    dps: Arc<dyn DpsChannel>,
    loader: &dyn OperatorKeyLoader,
) -> BindingsRegistry {
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
    BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, loader)
        .await
        .expect("build_from_db")
}

// ─── Seed helpers (into app.db()) ─────────────────────────────────────────

async fn seed_shift(pool: &SqlitePool, state: ShiftState) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, ?, 'ONLINE', 0, ?)",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(state)
    .bind(CASHIER)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_node_state(
    pool: &SqlitePool,
    mode: NodeMode,
    shift_state: ShiftState,
    shift_id: Option<ShiftId>,
) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(mode)
    .bind(shift_state)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
}

fn sell_entry() -> NewInboxEntry {
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    NewInboxEntry {
        request_id,
        fiscal_number: FN.into(),
        protocol: prro::db::models::enums::Protocol::Rest,
        operation_type: "SELL".into(),
        idempotency_key: "idem-a3final-SELL".into(),
        payload_json: SELL_PAYLOAD.into(),
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-06-09T12:00:00Z".into()),
        total_sum_kop: Some(TOTAL_KOP),
    }
}

/// Insert a NEW SELL inbox row and return the `Created` row (fails the test if
/// the insert deduped — the caller wants a fresh row to fiscalize).
async fn insert_created_sell(pool: &SqlitePool) -> InboxRow {
    match inbox::insert(pool, &sell_entry()).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    }
}

// ─── Probes ───────────────────────────────────────────────────────────────

async fn read_inbox_status(pool: &SqlitePool, request_id: &[u8; 16]) -> String {
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(&request_id[..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_doc_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn doc_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─── DPS builders ─────────────────────────────────────────────────────────

fn kvt1_ack() -> CheckAck {
    CheckAck {
        id: SFN.into(),
        id_sign: vec![],
        data_sign: vec![0xDE, 0xAD, 0xBE, 0xEF],
    }
}

/// send_chk Ok(Sent) + last_chk Ok(Kvt1 evidence) → the doc converges to ACK.
fn happy_dps() -> Arc<dyn DpsChannel> {
    Arc::new(StubDpsChannel::new(Ok(ack(SFN))).with_last_chk_queue(vec![Ok(kvt1_ack())]))
}

fn reject_dps() -> Arc<dyn DpsChannel> {
    Arc::new(StubDpsChannel::new(Err(DpsError::Authorization {
        code: -1,
        kind: AuthorizationKind::DocumentReject,
        message: "document rejected by DPS".into(),
    })))
}

// ════════════════════════════════════════════════════════════════════════
// 2d — first real fiscalization through the flipped binding + replay + Noop
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn binding_online_sell_reaches_ack_then_replay_is_idempotent() {
    let app = boot_app().await;
    let registry = build_registry(&app, happy_dps(), &OkLoader).await;
    let shift_id = seed_shift(app.db(), ShiftState::Opened).await;
    seed_node_state(
        app.db(),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;

    let binding = InlineWritePath::new(app.clone(), Arc::new(registry));

    // A fresh NEW row → the binding leases it (NEW→PROCESSING, Proceed — NOT
    // Noop) and fiscalizes the online SELL all the way to ACK.
    let row = insert_created_sell(app.db()).await;
    let outcome = binding
        .fiscalize(&row)
        .await
        .expect("SELL fiscalizes to a terminal ACK through the flipped binding");

    assert_eq!(
        outcome.document_state,
        DocState::Ack,
        "the receipt is issued"
    );
    assert_eq!(outcome.fiscal_id.as_deref(), Some(SFN));
    assert_eq!(read_doc_state(app.db()).await, "ACK");
    assert_eq!(read_inbox_status(app.db(), &row.request_id).await, "DONE");
    assert_eq!(doc_count(app.db()).await, 1, "exactly one issued receipt");
    prro::db::invariant_scan::assert_clean(app.db()).await;

    // Replay idempotency: re-inserting the SAME idempotency key resolves to a
    // Replay (NOT a second Created row) → the handler would answer from the
    // ledger, NEVER fiscalize a second time.
    let replay = inbox::insert(app.db(), &sell_entry()).await.unwrap();
    assert!(
        matches!(replay, InboxInsertOutcome::Replay(_)),
        "same idempotency key must dedup to Replay, got {replay:?}"
    );
    assert_eq!(
        doc_count(app.db()).await,
        1,
        "replay must NOT fiscalize twice"
    );
    assert_eq!(read_inbox_status(app.db(), &row.request_id).await, "DONE");
}

// ════════════════════════════════════════════════════════════════════════
// 2c — four-variant inbox-terminalise gate THROUGH the binding (HARD gate)
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn binding_shift_not_open_terminalises_inbox() {
    let app = boot_app().await;
    let registry = build_registry(&app, happy_dps(), &OkLoader).await;
    // Closed shift → a SELL is refused ShiftNotOpen at the acquire guard.
    seed_node_state(app.db(), NodeMode::Online, ShiftState::Closed, None).await;
    let binding = InlineWritePath::new(app.clone(), Arc::new(registry));

    let row = insert_created_sell(app.db()).await;
    let err = binding
        .fiscalize(&row)
        .await
        .expect_err("no open shift → refusal");
    assert!(
        matches!(err, FiscalError::ShiftNotOpen { .. }),
        "got {err:?}"
    );
    assert_eq!(
        read_inbox_status(app.db(), &row.request_id).await,
        "REJECTED",
        "the inbox-terminalise obligation holds through the binding"
    );
}

#[tokio::test]
async fn binding_dps_reject_terminalises_inbox() {
    let app = boot_app().await;
    let registry = build_registry(&app, reject_dps(), &OkLoader).await;
    let shift_id = seed_shift(app.db(), ShiftState::Opened).await;
    seed_node_state(
        app.db(),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let binding = InlineWritePath::new(app.clone(), Arc::new(registry));

    let row = insert_created_sell(app.db()).await;
    let err = binding
        .fiscalize(&row)
        .await
        .expect_err("DPS hard reject → refusal");
    assert!(
        matches!(err, FiscalError::DpsRejected { .. }),
        "got {err:?}"
    );
    assert_eq!(
        read_inbox_status(app.db(), &row.request_id).await,
        "REJECTED"
    );
}

#[tokio::test]
async fn binding_node_blocked_terminalises_inbox() {
    let app = boot_app().await;
    let registry = build_registry(&app, happy_dps(), &OkLoader).await;
    let shift_id = seed_shift(app.db(), ShiftState::Opened).await;
    // Node BLOCKED → OfflineRefused at the acquire guard.
    seed_node_state(
        app.db(),
        NodeMode::Blocked,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let binding = InlineWritePath::new(app.clone(), Arc::new(registry));

    let row = insert_created_sell(app.db()).await;
    let err = binding
        .fiscalize(&row)
        .await
        .expect_err("blocked node → refusal");
    assert!(
        matches!(err, FiscalError::OfflineRefused { .. }),
        "got {err:?}"
    );
    assert_eq!(
        read_inbox_status(app.db(), &row.request_id).await,
        "REJECTED"
    );
}

#[tokio::test]
async fn binding_sign_failure_terminalises_inbox() {
    let app = boot_app().await;
    let registry = build_registry(&app, happy_dps(), &FailingSignLoader).await;
    let shift_id = seed_shift(app.db(), ShiftState::Opened).await;
    seed_node_state(
        app.db(),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;
    let binding = InlineWritePath::new(app.clone(), Arc::new(registry));

    let row = insert_created_sell(app.db()).await;
    let err = binding
        .fiscalize(&row)
        .await
        .expect_err("crypto sign failure → refusal");
    assert!(
        matches!(err, FiscalError::SignFailure { .. }),
        "got {err:?}"
    );
    assert_eq!(
        read_inbox_status(app.db(), &row.request_id).await,
        "REJECTED"
    );
}

// ════════════════════════════════════════════════════════════════════════
// finding-2 — DI-site pin: the production factory binds the INLINE path, never
// UnimplementedWritePath.  A revert of the supervisor swap fails this pin (which
// a raw `Arc::new(InlineWritePath::new(..))` at the DI root did not catch).
// ════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn production_write_path_binds_inline_not_notimplemented() {
    let app = boot_app().await;
    let registry = build_registry(&app, happy_dps(), &OkLoader).await;
    let shift_id = seed_shift(app.db(), ShiftState::Opened).await;
    seed_node_state(
        app.db(),
        NodeMode::Online,
        ShiftState::Opened,
        Some(shift_id),
    )
    .await;

    // Construct the binding the SAME way the supervisor DI root does.
    let write_path = production_write_path(app.clone(), Arc::new(registry));
    let row = insert_created_sell(app.db()).await;
    let result = write_path.fiscalize(&row).await;

    assert!(
        !matches!(result, Err(FiscalError::NotImplemented { .. })),
        "production_write_path must bind the INLINE path, never UnimplementedWritePath; got {result:?}"
    );
    // Positive: the happy SELL actually fiscalizes to ACK through the factory.
    assert!(
        matches!(&result, Ok(o) if o.document_state == DocState::Ack),
        "got {result:?}"
    );
}
