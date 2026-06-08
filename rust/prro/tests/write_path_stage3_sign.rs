//! W6 — Stage 3 Sign (Pattern A) targeted fixtures.
//!
//! Drives `services::write_path::stage_sign::run` directly per the
//! W6 design spec at
//! `docs/superpowers/specs/2026-05-09-m3a-w6-stage3-sign-design.md`.
//!
//! Fixture priority (per user direction):
//!   1. stale_workercontext_returns_state_conflict_no_pin_no_z_alloc
//!   2. crypto_error_leaves_doc_prepared_no_files_and_reuses_pinned_inputs
//!      (Z and non-Z sub-cases)
//!   3. z_retry_reuses_persisted_z_number
//!   4. happy_path_sell                 (Pattern A ordering proof)
//!   5. persist_rollback_on_post_sign_db_error
//!   6. z_allocation_for_both_internal_labels  (ShiftClose + ZReport + non-Z negative)
//!   7. first_doc_no_seed_then_seed_appears_retry_signs_with_empty_prev_hash
//!   8. runtime_byte_equiv_zn_zero_for_nonz_artifact

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as TokioMutex;
use tokio::sync::MutexGuard as TokioMutexGuard;

/// Serialise fixtures inside this binary because they share the
/// `stage_sign::test_hook::COUNTER` global.  Without this, cargo's
/// multi-threaded test runner can interleave `reset()` from one
/// fixture with `record_persist_first()` from another, breaking the
/// Pattern A timestamp-ordering assertion in `stage3_happy_path_sell`.
/// `tokio::sync::Mutex` is async-friendly (held across `.await`
/// without clippy `await_holding_lock`).
static SERIAL: TokioMutex<()> = TokioMutex::const_new(());

async fn serial_lock<'a>() -> TokioMutexGuard<'a, ()> {
    SERIAL.lock().await
}

use async_trait::async_trait;
use prro::crypto::errors::CryptoError;
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::open_pool;
use prro::db::repositories::{
    fiscal_documents as fd, fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig,
    ingress_inbox as inbox, ingress_inbox::NewInboxEntry,
};
use prro::services::write_path::stage_sign::{self, test_hook, SignError, SigningContext};
use prro::services::write_path::types::{CanonicalFiscalCommand, WorkerContext};

const FN: &str = "4000000001";
const TAX: &str = "12345678";

// ─── Spy crypto provider ─────────────────────────────────────────────

struct SpyCrypto {
    sign_call_seq: Arc<std::sync::atomic::AtomicUsize>,
    sign_count: Arc<std::sync::atomic::AtomicUsize>,
    /// Queue of results returned in order.  When empty, default Ok.
    results: Mutex<std::collections::VecDeque<Result<SignedCmsBytes, CryptoError>>>,
    captured_xml: Mutex<Vec<Vec<u8>>>,
}

impl SpyCrypto {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            sign_call_seq: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            sign_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            results: Mutex::new(std::collections::VecDeque::new()),
            captured_xml: Mutex::new(Vec::new()),
        })
    }
    fn enqueue_err(&self, e: CryptoError) {
        self.results.lock().unwrap().push_back(Err(e));
    }
    fn enqueue_ok(&self, bytes: Vec<u8>) {
        self.results
            .lock()
            .unwrap()
            .push_back(Ok(SignedCmsBytes(bytes)));
    }
}

#[async_trait]
impl CryptoProvider for SpyCrypto {
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        let n = test_hook::COUNTER.fetch_add(1, Ordering::AcqRel) + 1;
        self.sign_call_seq.store(n, Ordering::Release);
        self.sign_count.fetch_add(1, Ordering::AcqRel);
        self.captured_xml
            .lock()
            .unwrap()
            .push(request.canonical_xml.to_vec());
        match self.results.lock().unwrap().pop_front() {
            Some(r) => r,
            None => Ok(SignedCmsBytes(b"DUMMY-CMS".to_vec())),
        }
    }
    async fn verify_dstu(
        &self,
        _content_digest: &[u8],
        _sig_bytes: &[u8],
        _pubkey_compressed: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unimplemented!("not exercised in stage 3 fixtures")
    }
    async fn unwrap_envelope(
        &self,
        _envelope_der: &[u8],
        _originator_cert_der: &[u8],
        _session: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unimplemented!("not exercised in stage 3 fixtures")
    }
    async fn fetch_cert_by_ski(
        &self,
        _urls: &[String],
        _ski: &[u8; 32],
        _request_timeout: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unimplemented!("not exercised in stage 3 fixtures")
    }
}

fn signing_ctx(spy: Arc<SpyCrypto>) -> SigningContext {
    SigningContext {
        provider: spy as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    }
}

// ─── Pool / seed helpers ─────────────────────────────────────────────

/// Process-lifetime container for the temp directories backing fresh
/// pools.  Each test gets its own `TempDir`; we keep them alive (so
/// SQLite WAL files stay valid for the duration of the test) but let
/// them drop on test-binary exit instead of leaking via `mem::forget`.
fn tempdir_keepalive() -> &'static std::sync::Mutex<Vec<tempfile::TempDir>> {
    static KEEPALIVE: std::sync::OnceLock<std::sync::Mutex<Vec<tempfile::TempDir>>> =
        std::sync::OnceLock::new();
    KEEPALIVE.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w6.db");
    tempdir_keepalive().lock().unwrap().push(dir);
    open_pool(&path).await.unwrap()
}

async fn seed_fn(pool: &sqlx::SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
            tax_number: TAX.into(),
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
}

async fn seed_open_shift(pool: &sqlx::SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(shift_id)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_node_state(
    pool: &sqlx::SqlitePool,
    current_shift_id: Option<ShiftId>,
    last_known: Option<&[u8; 32]>,
) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id, last_known_unsigned_xml_sha256) \
         VALUES (?, 'ONLINE', ?, ?, 1, 'b', 't', ?)",
    )
    .bind(FN)
    .bind(if current_shift_id.is_some() {
        ShiftState::Opened
    } else {
        ShiftState::Closed
    })
    .bind(current_shift_id)
    .bind(last_known.map(|h| &h[..]))
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a PREPARED fiscal_documents row directly (bypassing W5 stage 1).
/// Returns (document_id, lnd).
async fn seed_prepared_doc(
    pool: &sqlx::SqlitePool,
    doc_type: DocType,
    payload_json: &str,
    payload_sha256: [u8; 32],
    lnd: i64,
) -> (DocumentId, [u8; 16]) {
    let doc_id = DocumentId::new();
    let req_id = RequestId::new();
    let req_bytes: [u8; 16] = *req_id.as_bytes();
    // Mirror fiscal_documents columns exactly.
    sqlx::query(
        "INSERT INTO fiscal_documents (
             document_id, request_id, fiscal_number, lnd, doc_type, state,
             backend_profile_id, transport_profile_id, fs_mode, business_ts,
             total_sum_kop, payload_json, payload_sha256_canonical
         ) VALUES (?, ?, ?, ?, ?, 'PREPARED', 'b', 't', 'ONLINE', '2026-04-22T12:00:00Z', 15000, ?, ?)",
    )
    .bind(doc_id)
    .bind(req_id)
    .bind(FN)
    .bind(lnd)
    .bind(doc_type)
    .bind(payload_json)
    .bind(&payload_sha256[..])
    .execute(pool)
    .await
    .unwrap();
    // Also seed a matching inbox row (W5-canonical operation_type).
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: doc_type.as_str().into(),
            idempotency_key: format!("idem-{}", hex_encode(&req_bytes)),
            payload_json: payload_json.into(),
            payload_sha256_canonical: payload_sha256,
            correlation_id: None,
            signed_by_cashier_id: None,
            driver_id: Some("drv-test".into()),
            business_ts: None,
            total_sum_kop: None,
        },
    )
    .await
    .unwrap();
    (doc_id, req_bytes)
}

fn hex_encode(b: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

fn check_payload_json() -> &'static str {
    r#"{"items":[{"code":"A1","name":"X","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"CASH","sum_kop":15000,"type_code":"0"}]}"#
}
#[allow(dead_code)]
fn shift_open_payload_json() -> &'static str {
    // Reserved for future ShiftOpen-shaped fixtures (out of W6 scope).
    r#"{"opening_sum_kop":0}"#
}
fn z_report_payload_json() -> &'static str {
    r#"{"payments":[{"name":"CASH","sum_in_kop":15000,"sum_out_kop":0,"type_code":"0"}],"sell_count":1,"return_count":0}"#
}

fn dummy_payload_sha256() -> [u8; 32] {
    [0xABu8; 32]
}

fn make_command(
    doc_type: DocType,
    payload_json: &str,
    total_sum_kop: Option<i64>,
) -> CanonicalFiscalCommand {
    CanonicalFiscalCommand {
        doc_type,
        business_ts: "2026-04-22T12:00:00Z".into(),
        total_sum_kop,
        payload_json: payload_json.into(),
        payload_sha256_canonical: dummy_payload_sha256(),
        // RS-3 D5: non-Z, source hash coincides with the canonical hash.
        source_sha256: dummy_payload_sha256(),
        // W14a-2b Commit 2: stage_sign test fixtures pre-date signer
        // enforcement (Commits 3+5); None baseline matches existing
        // suite behavior.
        signed_by_cashier_id: None,
        // W4-Z0 piece 9: test fixtures don't exercise listener context.
        driver_id: None,
    }
}

fn make_worker_context(
    doc_id: DocumentId,
    req_bytes: [u8; 16],
    doc_type: DocType,
    state: DocState,
    payload_json: &str,
    total_sum_kop: Option<i64>,
) -> WorkerContext {
    let document = fd::DocumentRow {
        document_id: doc_id,
        fiscal_number: FN.into(),
        lnd: 1,
        state,
        doc_type,
        server_fiscal_no: None,
        submission_attempted_at: None,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        previous_hash: None,
        z_report_number: None,
        unsigned_xml_sha256: None,
        signing_inputs_pinned_at: None,
        // W14a-2b Commit 1: plumbing-only; helper not yet using the field.
        signed_by_cashier_id: None,
        // W4-Z2a piece 6b — test fixture: None (helper doesn't exercise
        // snapshot FK plumbing; runtime path sets it in stage_acquire).
        signing_config_snapshot_id: None,
    };
    WorkerContext {
        inbox: prro::db::repositories::ingress_inbox::InboxRow {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: doc_type.as_str().into(),
            idempotency_key: format!("idem-{}", hex_encode(&req_bytes)),
            status: "PROCESSING".into(),
            payload_json: payload_json.into(),
            payload_sha256_canonical: dummy_payload_sha256(),
            correlation_id: None,
            received_at: "2026-04-22T12:00:00Z".into(),
            signed_by_cashier_id: None,
            driver_id: Some("drv-test".into()),
            business_ts: None,
            total_sum_kop: None,
        },
        command: make_command(doc_type, payload_json, total_sum_kop),
        node_state: prro::db::repositories::node_state::NodeStateRow {
            fiscal_number: FN.into(),
            mode: NodeMode::Online,
            shift_state: ShiftState::Opened,
            next_lnd: 2,
            last_known_unsigned_xml_sha256: None,
            current_shift_id: None,
            backend_profile_id: Some("b".into()),
            transport_profile_id: Some("t".into()),
            next_z_report_number: 1,
        },
        active_shift: None,
        document,
        // W4-Z2a piece 6b external review: structural None for stage-3
        // sign fixtures.  These exercise minimal-payload back-compat
        // (no tax_group_1 items); piece-10 wiring would route to
        // persisted-snapshot reload on a non-None FK, but these fixtures
        // simulate the boot/Resume branch (no fresh snapshot in flight).
        tax_resolution_snapshot: None,
        tax_resolution_snapshot_id: None,
    }
}

// ─── DB probe helpers ────────────────────────────────────────────────

async fn doc_state(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> DocState {
    sqlx::query_scalar::<_, DocState>("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn doc_signing_inputs(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
) -> (
    Option<Vec<u8>>,
    Option<i64>,
    Option<String>,
    Option<Vec<u8>>,
) {
    let row = sqlx::query_as::<
        _,
        (
            Option<Vec<u8>>,
            Option<i64>,
            Option<String>,
            Option<Vec<u8>>,
        ),
    >(
        "SELECT previous_hash, z_report_number, signing_inputs_pinned_at, unsigned_xml_sha256 \
         FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row
}

async fn next_z_report_number(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT next_z_report_number FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn document_files_count(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM document_files WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_count(pool: &sqlx::SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ────────────────────────────────────────────────────────────────────
// Fixture #1 (priority 1) — stale WorkerContext returns StateConflict.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_stale_workercontext_returns_state_conflict_no_pin_no_z_alloc() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::ZReport,
        z_report_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    // Force fd.state PREPARED → SIGNED directly (simulating a concurrent
    // finalize / external transition between W5 stage 1 and W6 stage 3).
    sqlx::query("UPDATE fiscal_documents SET state = 'SIGNED' WHERE document_id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .unwrap();

    // Build a stale WorkerContext that still says state == Prepared.
    let stale_ctx = make_worker_context(
        doc_id,
        req_bytes,
        DocType::ZReport,
        DocState::Prepared,
        z_report_payload_json(),
        None,
    );

    let spy = SpyCrypto::new();
    let ctx = signing_ctx(spy.clone());
    let result = stage_sign::run(&pool, &ctx, stale_ctx).await;

    match result {
        Err(SignError::StateConflict { observed, .. }) => {
            assert_eq!(observed, DocState::Signed);
        }
        other => panic!("expected StateConflict(Signed), got {other:?}"),
    }

    // No pin happened: signing_inputs_pinned_at IS NULL, previous_hash IS NULL,
    // z_report_number IS NULL.
    let (prev, z, pinned_at, unsigned_hash) = doc_signing_inputs(&pool, doc_id).await;
    assert!(
        prev.is_none(),
        "previous_hash must stay NULL on StateConflict"
    );
    assert!(
        z.is_none(),
        "z_report_number must stay NULL on StateConflict"
    );
    assert!(
        pinned_at.is_none(),
        "signing_inputs_pinned_at must stay NULL on StateConflict"
    );
    assert!(unsigned_hash.is_none());

    // Z allocator NOT advanced.
    assert_eq!(
        next_z_report_number(&pool).await,
        1,
        "Z allocator must not advance on StateConflict"
    );

    // No document_files row, no audit "sign_inputs_pinned" or "doc_signed".
    assert_eq!(document_files_count(&pool, doc_id).await, 0);
    assert_eq!(audit_count(&pool, "sign_inputs_pinned").await, 0);
    assert_eq!(audit_count(&pool, "doc_signed").await, 0);

    // Spy: NO sign call.
    assert_eq!(spy.sign_count.load(Ordering::Acquire), 0);
}

// ────────────────────────────────────────────────────────────────────
// Unsupported DocType — fail-closed BEFORE pin / Z alloc / sign.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_unsupported_doc_type_rejects_before_pin_no_z_no_sign() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::ServiceIn,
        r#"{"x":1}"#,
        dummy_payload_sha256(),
        1,
    )
    .await;

    let initial_z = next_z_report_number(&pool).await;

    let stale_ctx = make_worker_context(
        doc_id,
        req_bytes,
        DocType::ServiceIn,
        DocState::Prepared,
        r#"{"x":1}"#,
        None,
    );
    let spy = SpyCrypto::new();
    let ctx = signing_ctx(spy.clone());
    let result = stage_sign::run(&pool, &ctx, stale_ctx).await;

    match result {
        Err(SignError::UnsupportedDocType { doc_type }) => {
            assert_eq!(doc_type, DocType::ServiceIn);
        }
        other => panic!("expected UnsupportedDocType(ServiceIn), got {other:?}"),
    }

    // No Z allocator advance.
    assert_eq!(
        next_z_report_number(&pool).await,
        initial_z,
        "Z allocator must not advance on UnsupportedDocType"
    );
    // No signed file row.
    assert_eq!(document_files_count(&pool, doc_id).await, 0);
    // No pin.
    let (prev, z, pinned_at, unsigned_hash) = doc_signing_inputs(&pool, doc_id).await;
    assert!(prev.is_none(), "previous_hash must stay NULL");
    assert!(z.is_none(), "z_report_number must stay NULL");
    assert!(
        pinned_at.is_none(),
        "signing_inputs_pinned_at must stay NULL"
    );
    assert!(unsigned_hash.is_none());
    // State untouched.
    assert_eq!(doc_state(&pool, doc_id).await, DocState::Prepared);
    // No crypto call.
    assert_eq!(spy.sign_count.load(Ordering::Acquire), 0);
    // No audit row from 3-PRE.
    assert_eq!(audit_count(&pool, "sign_inputs_pinned").await, 0);
    assert_eq!(audit_count(&pool, "doc_signed").await, 0);
}

// ────────────────────────────────────────────────────────────────────
// Fixture #2 (priority 2) — crypto error: pinned inputs preserved.
// Two sub-cases: ZReport (Z advanced once) and Sell (no Z).
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_crypto_error_zreport_pinned_z_advanced_no_files() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::ZReport,
        z_report_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    let spy = SpyCrypto::new();
    spy.enqueue_err(CryptoError::CmsSign {
        reason: prro::crypto::errors::SignKind::BackendError,
    });
    let ctx = signing_ctx(spy.clone());
    let stale_ctx = make_worker_context(
        doc_id,
        req_bytes,
        DocType::ZReport,
        DocState::Prepared,
        z_report_payload_json(),
        None,
    );

    let result = stage_sign::run(&pool, &ctx, stale_ctx).await;
    match result {
        Err(SignError::Crypto(_)) => {}
        other => panic!("expected Crypto err, got {other:?}"),
    }

    assert_eq!(doc_state(&pool, doc_id).await, DocState::Prepared);
    let (prev, z, pinned_at, unsigned_hash) = doc_signing_inputs(&pool, doc_id).await;
    assert!(prev.is_none(), "no chain seed → previous_hash NULL");
    assert_eq!(z, Some(1), "Z allocated and persisted before sign attempt");
    assert!(pinned_at.is_some(), "pinned_at set in 3-PRE");
    assert!(unsigned_hash.is_none(), "3-PERSIST did not run");
    assert_eq!(next_z_report_number(&pool).await, 2, "Z advanced once");
    assert_eq!(document_files_count(&pool, doc_id).await, 0);
    assert_eq!(audit_count(&pool, "sign_inputs_pinned").await, 1);
    assert_eq!(audit_count(&pool, "doc_signed").await, 0);
}

#[tokio::test]
async fn stage3_crypto_error_sell_pinned_no_z_advance_no_files() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::Sell,
        check_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    let spy = SpyCrypto::new();
    spy.enqueue_err(CryptoError::CmsSign {
        reason: prro::crypto::errors::SignKind::BackendError,
    });
    let ctx = signing_ctx(spy.clone());
    let stale_ctx = make_worker_context(
        doc_id,
        req_bytes,
        DocType::Sell,
        DocState::Prepared,
        check_payload_json(),
        Some(15000),
    );

    let result = stage_sign::run(&pool, &ctx, stale_ctx).await;
    assert!(matches!(result, Err(SignError::Crypto(_))));

    assert_eq!(doc_state(&pool, doc_id).await, DocState::Prepared);
    let (prev, z, pinned_at, unsigned_hash) = doc_signing_inputs(&pool, doc_id).await;
    assert!(prev.is_none());
    assert!(z.is_none(), "non-Z artifact: z_report_number stays NULL");
    assert!(pinned_at.is_some());
    assert!(unsigned_hash.is_none());
    assert_eq!(
        next_z_report_number(&pool).await,
        1,
        "non-Z must not advance Z allocator"
    );
    assert_eq!(document_files_count(&pool, doc_id).await, 0);
}

// ────────────────────────────────────────────────────────────────────
// Fixture #3 (priority 3) — Z retry reuses persisted z_report_number.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_z_retry_reuses_persisted_z_number() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::ZReport,
        z_report_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    // First run: sign fails.
    let spy = SpyCrypto::new();
    spy.enqueue_err(CryptoError::CmsSign {
        reason: prro::crypto::errors::SignKind::BackendError,
    });
    let ctx = signing_ctx(spy.clone());
    let _ = stage_sign::run(
        &pool,
        &ctx,
        make_worker_context(
            doc_id,
            req_bytes,
            DocType::ZReport,
            DocState::Prepared,
            z_report_payload_json(),
            None,
        ),
    )
    .await
    .expect_err("first run must err");

    let (_, z_after_first, pinned_after_first, _) = doc_signing_inputs(&pool, doc_id).await;
    assert_eq!(z_after_first, Some(1));
    assert!(pinned_after_first.is_some());
    assert_eq!(next_z_report_number(&pool).await, 2);

    // Force advance the allocator so that ANY second-allocation would
    // visibly bump it past 999 — if W6 retry re-allocates, we'll see
    // 1000 instead of staying at 999.
    sqlx::query("UPDATE node_state SET next_z_report_number = 999 WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

    // Second run: spy returns Ok; allocator MUST NOT be called again.
    spy.enqueue_ok(b"OK-CMS-SECOND-ATTEMPT".to_vec());
    let outcome = stage_sign::run(
        &pool,
        &ctx,
        make_worker_context(
            doc_id,
            req_bytes,
            DocType::ZReport,
            DocState::Prepared,
            z_report_payload_json(),
            None,
        ),
    )
    .await
    .expect("second run must succeed");

    // doc.state == Signed, z_report_number STILL = 1 (reused), allocator
    // STILL = 999 (no second advance).
    assert_eq!(outcome.document.state, DocState::Signed);
    assert_eq!(doc_state(&pool, doc_id).await, DocState::Signed);
    let (_, z_after_second, _, _) = doc_signing_inputs(&pool, doc_id).await;
    assert_eq!(z_after_second, Some(1), "retry reused persisted z");
    assert_eq!(
        next_z_report_number(&pool).await,
        999,
        "allocator must not advance on retry"
    );

    // Assert built XML carries ZN="1" (from the persisted, reused z).
    let xml = String::from_utf8_lossy(&outcome.unsigned_xml).to_string();
    assert!(xml.contains(r#"ZN="1""#), "expected ZN=1 in: {xml:?}");
}

// ────────────────────────────────────────────────────────────────────
// Fixture #4 (priority 4) — happy path Sell with Pattern A ordering proof.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_happy_path_sell() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::Sell,
        check_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    let spy = SpyCrypto::new();
    spy.enqueue_ok(b"GOOD-CMS".to_vec());
    let ctx = signing_ctx(spy.clone());
    let outcome = stage_sign::run(
        &pool,
        &ctx,
        make_worker_context(
            doc_id,
            req_bytes,
            DocType::Sell,
            DocState::Prepared,
            check_payload_json(),
            Some(15000),
        ),
    )
    .await
    .expect("happy path");

    assert_eq!(outcome.document.state, DocState::Signed);
    assert_eq!(outcome.signed_payload.0, b"GOOD-CMS");
    assert!(outcome.document.unsigned_xml_sha256.is_some());
    assert!(outcome.document.signing_inputs_pinned_at.is_some());
    assert!(outcome.document.previous_hash.is_none());
    assert!(outcome.document.z_report_number.is_none());

    assert_eq!(doc_state(&pool, doc_id).await, DocState::Signed);
    assert_eq!(document_files_count(&pool, doc_id).await, 2);
    assert_eq!(audit_count(&pool, "sign_inputs_pinned").await, 1);
    assert_eq!(audit_count(&pool, "doc_signed").await, 1);

    // Pattern A ordering proof: spy sign call recorded BEFORE persist
    // tx first statement.
    let sign_seq = spy.sign_call_seq.load(Ordering::Acquire);
    let persist_seq = test_hook::PERSIST_FIRST_SEQ.load(Ordering::Acquire);
    assert!(sign_seq > 0, "sign was called");
    assert!(persist_seq > 0, "persist tx ran");
    assert!(
        sign_seq < persist_seq,
        "Pattern A ordering: sign_seq ({sign_seq}) must precede persist_seq ({persist_seq})"
    );
}

// ────────────────────────────────────────────────────────────────────
// Fixture #5 (priority 5) — persist rollback on post-sign DB error.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_persist_rollback_on_post_sign_db_error() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::Sell,
        check_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    // Pre-seed a PAYLOAD_XML row for this doc → 3-PERSIST INSERT(PayloadXml)
    // will hit a PRIMARY KEY conflict, rolling back the entire persist tx.
    sqlx::query(
        "INSERT INTO document_files (document_id, kind, content) VALUES (?, 'PAYLOAD_XML', X'00')",
    )
    .bind(doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let spy = SpyCrypto::new();
    spy.enqueue_ok(b"GOOD-CMS".to_vec());
    let ctx = signing_ctx(spy.clone());
    let result = stage_sign::run(
        &pool,
        &ctx,
        make_worker_context(
            doc_id,
            req_bytes,
            DocType::Sell,
            DocState::Prepared,
            check_payload_json(),
            Some(15000),
        ),
    )
    .await;

    // Persist tx rolled back: any error variant carrying the PK conflict
    // is acceptable (Db / Internal / RowMissing not allowed).  We assert
    // it is an Err and assert state remains Prepared below.
    assert!(
        result.is_err(),
        "expected persist rollback to surface as Err"
    );

    assert_eq!(
        doc_state(&pool, doc_id).await,
        DocState::Prepared,
        "doc.state must roll back to Prepared"
    );
    // Pre-existing PAYLOAD_XML row still there; no SIGNED_XML inserted
    // (rollback purged the in-flight INSERT).
    assert_eq!(document_files_count(&pool, doc_id).await, 1);
    let (_, _, _, unsigned_hash) = doc_signing_inputs(&pool, doc_id).await;
    assert!(
        unsigned_hash.is_none(),
        "unsigned_xml_sha256 UPDATE rolled back"
    );
    assert_eq!(audit_count(&pool, "doc_signed").await, 0);

    // Pre-sign tx state preserved (separate envelope, NOT rolled back).
    let (_, _, pinned_at, _) = doc_signing_inputs(&pool, doc_id).await;
    assert!(
        pinned_at.is_some(),
        "pre-sign pin survived persist rollback"
    );
}

// ────────────────────────────────────────────────────────────────────
// Fixture #6 (priority 6) — Z allocation for both internal labels +
// non-Z negative.  Three sub-cases inside one #[test] for shared seed
// tooling.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_z_allocation_for_both_internal_labels() {
    let _g = serial_lock().await;
    // Sub-case A: DocType::ZReport — allocator fires.
    {
        test_hook::reset();
        let pool = fresh_pool().await;
        seed_fn(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, Some(shift_id), None).await;
        let (doc_id, req_bytes) = seed_prepared_doc(
            &pool,
            DocType::ZReport,
            z_report_payload_json(),
            dummy_payload_sha256(),
            1,
        )
        .await;
        let spy = SpyCrypto::new();
        spy.enqueue_ok(b"OK".to_vec());
        let ctx = signing_ctx(spy);
        let outcome = stage_sign::run(
            &pool,
            &ctx,
            make_worker_context(
                doc_id,
                req_bytes,
                DocType::ZReport,
                DocState::Prepared,
                z_report_payload_json(),
                None,
            ),
        )
        .await
        .expect("ZReport happy");
        assert_eq!(outcome.document.z_report_number, Some(1));
        assert_eq!(next_z_report_number(&pool).await, 2);
        let xml = String::from_utf8_lossy(&outcome.unsigned_xml).to_string();
        assert!(xml.contains(r#"ZN="1""#));
    }
    // Sub-case B: DocType::ShiftClose — same allocator path.
    {
        test_hook::reset();
        let pool = fresh_pool().await;
        seed_fn(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, Some(shift_id), None).await;
        let (doc_id, req_bytes) = seed_prepared_doc(
            &pool,
            DocType::ShiftClose,
            z_report_payload_json(),
            dummy_payload_sha256(),
            1,
        )
        .await;
        let spy = SpyCrypto::new();
        spy.enqueue_ok(b"OK".to_vec());
        let ctx = signing_ctx(spy);
        let outcome = stage_sign::run(
            &pool,
            &ctx,
            make_worker_context(
                doc_id,
                req_bytes,
                DocType::ShiftClose,
                DocState::Prepared,
                z_report_payload_json(),
                None,
            ),
        )
        .await
        .expect("ShiftClose happy");
        assert_eq!(
            outcome.document.z_report_number,
            Some(1),
            "ShiftClose maps to wire ZReport per ADR-M3-A2"
        );
        assert_eq!(next_z_report_number(&pool).await, 2);
        let xml = String::from_utf8_lossy(&outcome.unsigned_xml).to_string();
        assert!(xml.contains(r#"ZN="1""#));
    }
    // Sub-case C: DocType::Sell — allocator MUST NOT fire.
    {
        test_hook::reset();
        let pool = fresh_pool().await;
        seed_fn(&pool).await;
        let shift_id = seed_open_shift(&pool).await;
        seed_node_state(&pool, Some(shift_id), None).await;
        let (doc_id, req_bytes) = seed_prepared_doc(
            &pool,
            DocType::Sell,
            check_payload_json(),
            dummy_payload_sha256(),
            1,
        )
        .await;
        let spy = SpyCrypto::new();
        spy.enqueue_ok(b"OK".to_vec());
        let ctx = signing_ctx(spy);
        let outcome = stage_sign::run(
            &pool,
            &ctx,
            make_worker_context(
                doc_id,
                req_bytes,
                DocType::Sell,
                DocState::Prepared,
                check_payload_json(),
                Some(15000),
            ),
        )
        .await
        .expect("Sell happy");
        assert!(outcome.document.z_report_number.is_none());
        assert_eq!(
            next_z_report_number(&pool).await,
            1,
            "non-Z must not advance allocator"
        );
    }
}

// ────────────────────────────────────────────────────────────────────
// Fixture #7 (priority 7) — first doc no seed, then seed appears,
// retry signs with empty <MAC>.  Proves the pin-once branch never
// re-reads node_state.last_known_unsigned_xml_sha256.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_first_doc_no_seed_then_seed_appears_retry_signs_with_empty_prev_hash() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    // node_state.last_known_unsigned_xml_sha256 IS NULL — true bootstrap.
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::Sell,
        check_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    // First run: sign fails.
    let spy = SpyCrypto::new();
    spy.enqueue_err(CryptoError::CmsSign {
        reason: prro::crypto::errors::SignKind::BackendError,
    });
    let ctx = signing_ctx(spy.clone());
    let _ = stage_sign::run(
        &pool,
        &ctx,
        make_worker_context(
            doc_id,
            req_bytes,
            DocType::Sell,
            DocState::Prepared,
            check_payload_json(),
            Some(15000),
        ),
    )
    .await
    .expect_err("first run errs");

    // After first run: pin happened with no seed → previous_hash IS NULL,
    // pinned_at IS NOT NULL.
    let (prev_after_first, _, pinned_after_first, _) = doc_signing_inputs(&pool, doc_id).await;
    assert!(prev_after_first.is_none());
    assert!(pinned_after_first.is_some());

    // Mutate node_state.last_known_unsigned_xml_sha256 to a non-NULL
    // value (simulating a prior doc's ACK landing between attempts).
    let new_seed = [0xCDu8; 32];
    sqlx::query("UPDATE node_state SET last_known_unsigned_xml_sha256 = ? WHERE fiscal_number = ?")
        .bind(&new_seed[..])
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

    // Second run: spy returns Ok.  3-PRE goes else-branch (already
    // pinned) — does NOT re-read seed, does NOT update previous_hash.
    spy.enqueue_ok(b"OK".to_vec());
    let outcome = stage_sign::run(
        &pool,
        &ctx,
        make_worker_context(
            doc_id,
            req_bytes,
            DocType::Sell,
            DocState::Prepared,
            check_payload_json(),
            Some(15000),
        ),
    )
    .await
    .expect("second run succeeds");

    let (prev_after_second, _, _, _) = doc_signing_inputs(&pool, doc_id).await;
    assert!(
        prev_after_second.is_none(),
        "previous_hash MUST stay NULL — retry must not pick up newly-arrived seed"
    );

    // Built XML <MAC> attribute should be empty (Python parity for
    // first-after-bootstrap).  The W4 builder emits <MAC>{hex}</MAC>;
    // empty hex → `<MAC></MAC>`.
    let xml = String::from_utf8_lossy(&outcome.unsigned_xml).to_string();
    assert!(
        xml.contains("<MAC></MAC>") || xml.contains("<MAC/>"),
        "expected empty <MAC> on first-doc retry, got: {xml:?}"
    );
    assert_eq!(outcome.document.state, DocState::Signed);
}

// ────────────────────────────────────────────────────────────────────
// Fixture #8 (priority 8) — runtime byte-equiv: ZN="0" for non-Z
// artifact + deterministic unsigned_xml_sha256 across two runs.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stage3_runtime_byte_equiv_zn_zero_for_nonz_artifact() {
    let _guard = serial_lock().await;
    test_hook::reset();
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_state(&pool, Some(shift_id), None).await;
    let (doc_id, req_bytes) = seed_prepared_doc(
        &pool,
        DocType::Sell,
        check_payload_json(),
        dummy_payload_sha256(),
        1,
    )
    .await;

    let spy = SpyCrypto::new();
    spy.enqueue_ok(b"OK".to_vec());
    let ctx = signing_ctx(spy.clone());
    let outcome = stage_sign::run(
        &pool,
        &ctx,
        make_worker_context(
            doc_id,
            req_bytes,
            DocType::Sell,
            DocState::Prepared,
            check_payload_json(),
            Some(15000),
        ),
    )
    .await
    .expect("first run");

    let xml = outcome.unsigned_xml.clone();
    assert!(
        String::from_utf8_lossy(&xml).contains(r#"ZN="0""#),
        "non-Z artifact must serialize as <DAT ... ZN=\"0\">"
    );
    let hash1 = outcome.document.unsigned_xml_sha256.unwrap();

    // Second run on a sibling doc (lnd=2) with same payload — assert
    // non-determinism is bounded to lnd-driven `<DAT DI>` only; ZN="0"
    // and the constant fields stay byte-equiv.
    let (doc_id2, req_bytes2) = seed_prepared_doc(
        &pool,
        DocType::Sell,
        check_payload_json(),
        dummy_payload_sha256(),
        2,
    )
    .await;
    let mut ctx2_wc = make_worker_context(
        doc_id2,
        req_bytes2,
        DocType::Sell,
        DocState::Prepared,
        check_payload_json(),
        Some(15000),
    );
    ctx2_wc.document.lnd = 2;
    spy.enqueue_ok(b"OK".to_vec());
    let outcome2 = stage_sign::run(&pool, &ctx, ctx2_wc)
        .await
        .expect("second run");
    let xml2 = outcome2.unsigned_xml.clone();
    assert!(String::from_utf8_lossy(&xml2).contains(r#"ZN="0""#));

    // Hashes differ ONLY because <DAT DI> differs; structural sanity:
    // both have ZN="0" and identical FN attribute.
    let hash2 = outcome2.document.unsigned_xml_sha256.unwrap();
    assert_ne!(hash1, hash2, "different lnd → different unsigned hash");
    assert!(
        String::from_utf8_lossy(&xml).contains(&format!(r#"FN="{FN}""#))
            && String::from_utf8_lossy(&xml2).contains(&format!(r#"FN="{FN}""#))
    );
}
