//! B9 Slice A — conditional stamp-at-sign so the opaque offline code lands
//! in the SIGNED bytes as `<MAC ID='{offline_dps_code}'>`.
//!
//! Architect ruling (2026-07-08): for an OFFLINE-origin doc (`fs_mode='OFFLINE'`)
//! the `acquire_code_tx` + stamp of `offline_fiscal_no`/`offline_dps_code`/
//! `offline_session_id` moves from `stage_offline_ack` into the `stage_sign`
//! pin-tx, BEFORE the canonical XML is built — but ONLY when the offline
//! preconditions hold at sign (node mode ∈ {Offline,GoingOffline} fresh-read +
//! an OPEN session exists + not already stamped). Else sign online-shaped and
//! let `stage_offline_ack` acquire-OR-readback (fallback preserved).
//!
//! These fixtures drive `stage_sign::run` directly and assert on the CMS spy's
//! captured canonical XML (the SIGNED bytes) + the persisted stamp columns.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use prro::crypto::errors::CryptoError;
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{DocState, DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, RequestId};
use prro::db::open_pool;
use prro::db::repositories::{
    fiscal_documents as fd, fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig,
    ingress_inbox as inbox, ingress_inbox::NewInboxEntry,
};
use prro::services::write_path::stage_sign::{self, SigningContext};
use prro::services::write_path::types::{CanonicalFiscalCommand, WorkerContext};

const FN: &str = "4000000099";
const TAX: &str = "12345678";

// ─── CMS spy that captures the canonical (signed) XML ───────────────────

struct SpyCrypto {
    captured_xml: Mutex<Vec<Vec<u8>>>,
    sign_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl SpyCrypto {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            captured_xml: Mutex::new(Vec::new()),
            sign_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }
    fn last_signed_xml(&self) -> Option<Vec<u8>> {
        self.captured_xml.lock().unwrap().last().cloned()
    }
}

#[async_trait]
impl CryptoProvider for SpyCrypto {
    async fn sign_cms_detached(
        &self,
        request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        self.sign_count.fetch_add(1, Ordering::AcqRel);
        self.captured_xml
            .lock()
            .unwrap()
            .push(request.canonical_xml.to_vec());
        Ok(SignedCmsBytes(b"DUMMY-CMS".to_vec()))
    }
    async fn verify_dstu(
        &self,
        _c: &[u8],
        _s: &[u8],
        _p: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unimplemented!()
    }
    async fn unwrap_envelope(
        &self,
        _e: &[u8],
        _o: &[u8],
        _s: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unimplemented!()
    }
    async fn fetch_cert_by_ski(
        &self,
        _u: &[String],
        _s: &[u8; 32],
        _t: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unimplemented!()
    }
}

fn signing_ctx(spy: Arc<SpyCrypto>) -> SigningContext {
    SigningContext {
        provider: spy as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    }
}

// ─── Seed helpers ───────────────────────────────────────────────────────

fn tempdir_keepalive() -> &'static std::sync::Mutex<Vec<tempfile::TempDir>> {
    static K: std::sync::OnceLock<std::sync::Mutex<Vec<tempfile::TempDir>>> =
        std::sync::OnceLock::new();
    K.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b9.db");
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

/// Seed node_state with the given mode. Offline fixtures use `OFFLINE`.
async fn seed_node_state(pool: &sqlx::SqlitePool, mode: &str) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, next_lnd, backend_profile_id, \
          transport_profile_id, last_known_unsigned_xml_sha256) \
         VALUES (?, ?, 'OPENED', 1, 'b', 't', NULL)",
    )
    .bind(FN)
    .bind(mode)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_session(pool: &sqlx::SqlitePool) -> OfflineSessionId {
    let sid = OfflineSessionId::new();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-07-08T10:00:00Z')",
    )
    .bind(sid)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
    sid
}

async fn seed_real_code(pool: &sqlx::SqlitePool, code_lnd: i64, dps_code: &str) {
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd, dps_code) VALUES (?, ?, ?)")
        .bind(FN)
        .bind(code_lnd)
        .bind(dps_code)
        .execute(pool)
        .await
        .unwrap();
}

fn check_payload_json() -> &'static str {
    r#"{"items":[{"code":"A1","name":"X","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"CASH","sum_kop":15000,"type_code":"0"}]}"#
}

fn hex_encode(b: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

/// Seed a PREPARED offline SELL doc + matching inbox row. Returns (doc_id, req).
async fn seed_prepared_offline_sell(pool: &sqlx::SqlitePool, lnd: i64) -> (DocumentId, [u8; 16]) {
    let doc_id = DocumentId::new();
    let req_id = RequestId::new();
    let req_bytes: [u8; 16] = *req_id.as_bytes();
    let payload = check_payload_json();
    let sha = [0xABu8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents (
             document_id, request_id, fiscal_number, lnd, doc_type, state,
             backend_profile_id, transport_profile_id, fs_mode, business_ts,
             total_sum_kop, payload_json, payload_sha256_canonical
         ) VALUES (?, ?, ?, ?, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
             '2026-04-22T12:00:00Z', 15000, ?, ?)",
    )
    .bind(doc_id)
    .bind(req_id)
    .bind(FN)
    .bind(lnd)
    .bind(payload)
    .bind(&sha[..])
    .execute(pool)
    .await
    .unwrap();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: "SELL".into(),
            idempotency_key: format!("idem-{}", hex_encode(&req_bytes)),
            payload_json: payload.into(),
            payload_sha256_canonical: sha,
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

fn offline_sell_worker_ctx(doc_id: DocumentId, req_bytes: [u8; 16], lnd: i64) -> WorkerContext {
    let payload = check_payload_json();
    let document = fd::DocumentRow {
        document_id: doc_id,
        fiscal_number: FN.into(),
        lnd,
        state: DocState::Prepared,
        doc_type: DocType::Sell,
        server_fiscal_no: None,
        submission_attempted_at: None,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        previous_hash: None,
        z_report_number: None,
        unsigned_xml_sha256: None,
        signing_inputs_pinned_at: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    };
    WorkerContext {
        inbox: prro::db::repositories::ingress_inbox::InboxRow {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: "SELL".into(),
            idempotency_key: format!("idem-{}", hex_encode(&req_bytes)),
            status: "PROCESSING".into(),
            payload_json: payload.into(),
            payload_sha256_canonical: [0xABu8; 32],
            correlation_id: None,
            received_at: "2026-04-22T12:00:00Z".into(),
            signed_by_cashier_id: None,
            driver_id: Some("drv-test".into()),
            business_ts: None,
            total_sum_kop: None,
        },
        command: CanonicalFiscalCommand {
            doc_type: DocType::Sell,
            business_ts: "2026-04-22T12:00:00Z".into(),
            total_sum_kop: Some(15000),
            payload_json: payload.into(),
            payload_sha256_canonical: [0xABu8; 32],
            source_sha256: [0xABu8; 32],
            signed_by_cashier_id: None,
            driver_id: None,
        },
        node_state: prro::db::repositories::node_state::NodeStateRow {
            fiscal_number: FN.into(),
            mode: NodeMode::Offline,
            shift_state: ShiftState::Opened,
            next_lnd: lnd + 1,
            last_known_unsigned_xml_sha256: None,
            current_shift_id: None,
            backend_profile_id: Some("b".into()),
            transport_profile_id: Some("t".into()),
            next_z_report_number: 1,
        },
        active_shift: None,
        document,
        tax_resolution_snapshot: None,
        tax_resolution_snapshot_id: None,
    }
}

async fn fetch_offline_dps_code(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT offline_dps_code FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn fetch_offline_fiscal_no(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> Option<i64> {
    sqlx::query_scalar("SELECT offline_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn fetch_state(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn fetch_consumed_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ─── B9-A1: offline sign stamps the code AND puts <MAC ID> in signed bytes ─

#[tokio::test]
async fn offline_sign_stamps_code_and_emits_mac_id_in_signed_bytes() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    seed_node_state(&pool, "OFFLINE").await;
    seed_open_session(&pool).await;
    seed_real_code(&pool, 77, "eYme-jhnkWQ").await;
    let (doc_id, req) = seed_prepared_offline_sell(&pool, 1).await;

    let spy = SpyCrypto::new();
    let ctx = signing_ctx(spy.clone());
    let outcome = stage_sign::run(&pool, &ctx, offline_sell_worker_ctx(doc_id, req, 1)).await;
    let outcome = outcome.expect("offline sign with an OPEN session + code must succeed");

    // Doc advanced to SIGNED.
    assert_eq!(outcome.document.state, DocState::Signed);
    assert_eq!(fetch_state(&pool, doc_id).await, "SIGNED");

    // The code was consumed + stamped at SIGN (not at offline-ack).
    assert_eq!(
        fetch_offline_dps_code(&pool, doc_id).await.as_deref(),
        Some("eYme-jhnkWQ"),
        "offline_dps_code must be stamped in the pin-tx at sign"
    );
    assert_eq!(
        fetch_offline_fiscal_no(&pool, doc_id).await,
        Some(77),
        "offline_fiscal_no must be the consumed code_lnd"
    );
    assert_eq!(
        fetch_consumed_count(&pool).await,
        1,
        "exactly one code consumed"
    );

    // CORE B9 pin: the opaque code is in the SIGNED bytes as <MAC ID='...'>.
    let signed_xml = spy.last_signed_xml().expect("crypto was called");
    let s = String::from_utf8_lossy(&signed_xml);
    assert!(
        s.contains("<MAC ID='eYme-jhnkWQ'>"),
        "signed offline XML must carry <MAC ID='eYme-jhnkWQ'>: {s}"
    );
}

// ─── B9-A2: online sign is byte-unaffected (bare <MAC>, no stamp) ─────────

#[tokio::test]
async fn online_sign_stays_bare_mac_and_never_touches_offline_columns() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    seed_node_state(&pool, "ONLINE").await;
    // No offline session, no codes — online path must not care.
    // Insert an ONLINE prepared doc by overriding fs_mode.
    let (doc_id, req) = seed_prepared_offline_sell(&pool, 1).await;
    sqlx::query("UPDATE fiscal_documents SET fs_mode = 'ONLINE' WHERE document_id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .unwrap();

    let spy = SpyCrypto::new();
    let ctx = signing_ctx(spy.clone());
    let mut wc = offline_sell_worker_ctx(doc_id, req, 1);
    wc.node_state.mode = NodeMode::Online;
    stage_sign::run(&pool, &ctx, wc)
        .await
        .expect("online sign must succeed");

    // Online path: bare <MAC>, no ID attribute.
    let signed_xml = spy.last_signed_xml().expect("crypto was called");
    let s = String::from_utf8_lossy(&signed_xml);
    assert!(s.contains("<MAC>"), "online MAC must be bare-opened: {s}");
    assert!(
        !s.contains("<MAC ID="),
        "online MAC must NOT carry an ID attribute: {s}"
    );

    // Offline columns untouched, no code consumed.
    assert_eq!(fetch_offline_dps_code(&pool, doc_id).await, None);
    assert_eq!(fetch_offline_fiscal_no(&pool, doc_id).await, None);
    assert_eq!(fetch_consumed_count(&pool).await, 0);
}

// ─── B9-A3 (#192): pool-exhausted-at-sign SIGNS ONLINE-SHAPED (defers Abort) ──
//
// A no-code offline sell is never issued.  The established #192 design
// (invariant_scan.rs + the fuzzer's `mint_aborted_refusal` model) is "reach
// SIGNED, then stage_offline_ack refuses (CodePoolExhausted) → the doc
// terminates in Aborted".  So on pool-exhaustion AT SIGN the doc must sign
// ONLINE-SHAPED (bare `<MAC>`, no code consumed, no stamp) and let offline-ack
// abort it — it must NOT rest non-terminal PREPARED (which would be a
// `StuckNonTerminalDoc` #192 violation, since nothing re-drives it before the
// next quiescent scan).

#[tokio::test]
async fn offline_sign_pool_exhausted_signs_online_shaped_and_defers_to_offline_ack() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    seed_node_state(&pool, "OFFLINE").await;
    seed_open_session(&pool).await;
    // No code seeded → pool exhausted at sign.
    let (doc_id, req) = seed_prepared_offline_sell(&pool, 1).await;

    let spy = SpyCrypto::new();
    let ctx = signing_ctx(spy.clone());
    let outcome = stage_sign::run(&pool, &ctx, offline_sell_worker_ctx(doc_id, req, 1))
        .await
        .expect("pool-exhausted offline sign must SIGN online-shaped, not refuse");

    // Signs online-shaped: reaches SIGNED, crypto WAS called, bare `<MAC>`.
    assert_eq!(outcome.document.state, DocState::Signed);
    assert_eq!(fetch_state(&pool, doc_id).await, "SIGNED");
    assert_eq!(
        spy.sign_count.load(Ordering::Acquire),
        1,
        "exhausted-at-sign must still sign (online-shaped), deferring Abort to offline-ack"
    );
    let s = String::from_utf8_lossy(&spy.last_signed_xml().unwrap()).to_string();
    assert!(
        s.contains("<MAC>"),
        "exhausted offline sign emits bare <MAC>: {s}"
    );
    assert!(
        !s.contains("<MAC ID="),
        "exhausted offline sign has NO <MAC ID> (no code to stamp): {s}"
    );

    // No code consumed, no stamp — offline-ack will refuse + Abort later.
    assert_eq!(fetch_consumed_count(&pool).await, 0);
    assert_eq!(fetch_offline_dps_code(&pool, doc_id).await, None);
    assert_eq!(fetch_offline_fiscal_no(&pool, doc_id).await, None);
}

// ─── B9-A4 (idempotency / INV-4 / INV-5): re-sign does NOT double-consume ─

#[tokio::test]
async fn offline_re_sign_reuses_stamped_code_no_double_consume() {
    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    seed_node_state(&pool, "OFFLINE").await;
    seed_open_session(&pool).await;
    seed_real_code(&pool, 77, "eYme-jhnkWQ").await;
    seed_real_code(&pool, 78, "SECOND-CODE").await;
    let (doc_id, req) = seed_prepared_offline_sell(&pool, 1).await;

    let spy = SpyCrypto::new();
    let ctx = signing_ctx(spy.clone());

    // First sign consumes exactly one code and stamps it.
    stage_sign::run(&pool, &ctx, offline_sell_worker_ctx(doc_id, req, 1))
        .await
        .expect("first offline sign ok");
    assert_eq!(fetch_consumed_count(&pool).await, 1);
    let first_code = fetch_offline_dps_code(&pool, doc_id).await;
    assert_eq!(first_code.as_deref(), Some("eYme-jhnkWQ"));

    // Force the doc back to PREPARED (simulate a crash-before-persist re-drive):
    // the pin-tx (pin + offline stamp) committed, but 3-PERSIST never ran, so
    // the doc is still PREPARED-pinned and its document_files were NOT written.
    // Delete the files the first (fully-completed) sign inserted so we faithfully
    // model the crash-before-persist window on re-sign.
    sqlx::query("UPDATE fiscal_documents SET state = 'PREPARED' WHERE document_id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM document_files WHERE document_id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .unwrap();

    // Re-sign: the idempotency guard (offline_dps_code IS NULL) must SKIP the
    // acquire and reuse the already-stamped code — no second code consumed.
    stage_sign::run(&pool, &ctx, offline_sell_worker_ctx(doc_id, req, 1))
        .await
        .expect("re-sign ok");
    assert_eq!(
        fetch_consumed_count(&pool).await,
        1,
        "re-sign must NOT consume a second code (INV-4 / INV-5)"
    );
    assert_eq!(
        fetch_offline_dps_code(&pool, doc_id).await.as_deref(),
        Some("eYme-jhnkWQ"),
        "re-sign reuses the same stamped code"
    );

    // The re-signed bytes still carry the SAME <MAC ID>.
    let s = String::from_utf8_lossy(&spy.last_signed_xml().unwrap()).to_string();
    assert!(
        s.contains("<MAC ID='eYme-jhnkWQ'>"),
        "re-sign keeps the id: {s}"
    );
}

// ─── B9-A5 (#192): exhausted-at-sign SIGNED doc reaches offline-ack and ──────
//     ABORTS terminally (unstamped bare-MAC), never a non-terminal rest ───────
//
// An exhausted-at-sign offline doc signs ONLINE-SHAPED (SIGNED, bare `<MAC>`,
// unstamped) — it never got a code, so it can never validly drain.  At
// offline-ack it takes the UNSTAMPED-ABORT branch (SIGNED→Aborted in-envelope)
// — the #192 terminal-refusal outcome (Aborted end-state matches the pre-B9
// no-code path + the fuzzer's `mint_aborted_refusal` model).  This proves B9
// leaves NO non-terminal rest: exhausted-at-sign → SIGNED → offline-ack →
// Aborted, all terminal-clean.

#[tokio::test]
async fn exhausted_at_sign_signed_doc_aborts_at_offline_ack() {
    use prro::services::write_path::stage_offline_ack::{self, OfflineAckOutcome, RefusalReason};

    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    seed_node_state(&pool, "OFFLINE").await;
    seed_open_session(&pool).await;
    // Pool empty — no code at sign OR at offline-ack.
    let (doc_id, req) = seed_prepared_offline_sell(&pool, 1).await;

    let spy = SpyCrypto::new();
    let ctx = signing_ctx(spy.clone());

    // Step 1: exhausted-at-sign → signs ONLINE-SHAPED → SIGNED (bare `<MAC>`,
    // unstamped).
    let outcome = stage_sign::run(&pool, &ctx, offline_sell_worker_ctx(doc_id, req, 1))
        .await
        .expect("exhausted offline sign signs online-shaped");
    assert_eq!(outcome.document.state, DocState::Signed);
    assert_eq!(fetch_consumed_count(&pool).await, 0);

    // Step 2: stage_offline_ack sees the doc is UNSTAMPED (bare `<MAC>`) →
    // aborts it terminally IN-ENVELOPE (SIGNED→Aborted), regardless of pool
    // state (the unstamped-abort precedes any pool interaction).
    let ack = stage_offline_ack::run(&pool, doc_id, FN)
        .await
        .expect("offline_ack run ok");
    assert!(
        matches!(
            ack,
            OfflineAckOutcome::Refused(RefusalReason::UnstampedBareMacAbort)
        ),
        "exhausted-at-sign (unstamped) offline-ack must refuse+abort, got: {ack:?}"
    );
    assert_eq!(fetch_state(&pool, doc_id).await, "ABORTED");
    assert_eq!(fetch_consumed_count(&pool).await, 0);
}

// ─── B9-A6 (MERGE-BLOCKER FIX, RED-first + TEETH): an UNSTAMPED SIGNED offline ─
//     doc reaching offline-ack — even WITH a session + codes available — must ──
//     ABORT (SIGNED→Aborted), NOT become a drainable OFFLINE_LOCAL_ACK ─────────
//
// THE BUG (reviewer-found): a doc signed with a BARE `<MAC>` (no OPEN session at
// sign → not stamped) that later reaches offline-ack when a session+codes DO
// exist would, on the old acquire-fallback, get a code stamped into the DB
// column + become a drainable OFFLINE_LOCAL_ACK — but its SIGNED BYTES stay
// bare (offline-ack does NOT re-sign) → on drain DPS rejects `-9 "not ID in
// MAC"`.  That re-introduces the exact bug B9 exists to fix.  Invariant:
// stage_offline_ack produces OLA ONLY from the readback (stamped-at-sign) path;
// any unstamped SIGNED offline doc ABORTS ⇒ every drained offline doc provably
// carries `<MAC ID>`.
//
// TEETH: revert the None-branch to acquire → this doc becomes OFFLINE_LOCAL_ACK
// with a bare-MAC → the `state == ABORTED` assert FAILS (RED).

#[tokio::test]
async fn unstamped_signed_offline_doc_aborts_at_offline_ack_even_with_session_and_codes() {
    use prro::services::write_path::stage_offline_ack::{self, OfflineAckOutcome, RefusalReason};

    let pool = fresh_pool().await;
    seed_fn(&pool).await;
    seed_node_state(&pool, "OFFLINE").await;
    seed_open_session(&pool).await;
    // Codes ARE available at offline-ack — the OLD fallback would acquire one.
    seed_real_code(&pool, 88, "LATE-CODE").await;

    // Insert a SIGNED offline doc that was NOT stamped at sign (bare `<MAC>`):
    // the crash-resume / lease-released-between-sign-and-ack scenario.
    let (doc_id, _req) = seed_prepared_offline_sell(&pool, 1).await;
    sqlx::query(
        "UPDATE fiscal_documents SET state = 'SIGNED', unsigned_xml_sha256 = ? \
         WHERE document_id = ?",
    )
    .bind(vec![0xA1u8; 32])
    .bind(doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let ack = stage_offline_ack::run(&pool, doc_id, FN)
        .await
        .expect("offline_ack run ok");

    // Must ABORT — NOT acquire, NOT become OFFLINE_LOCAL_ACK.
    assert!(
        matches!(
            ack,
            OfflineAckOutcome::Refused(RefusalReason::UnstampedBareMacAbort)
        ),
        "unstamped bare-MAC doc must refuse+abort, got: {ack:?}"
    );
    assert_eq!(
        fetch_state(&pool, doc_id).await,
        "ABORTED",
        "unstamped bare-MAC doc must be Aborted (never a drainable OLA)"
    );
    // Code pool untouched — no code consumed for a doc that can't drain.
    assert_eq!(
        fetch_consumed_count(&pool).await,
        0,
        "the None-branch must NOT acquire a code (the bug); pool stays intact"
    );
}
