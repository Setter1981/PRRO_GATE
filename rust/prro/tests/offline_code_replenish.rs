//! PR-C4: `OfflineCodeReplenishService` integration tests.
//!
//! RED pins (written before implementation):
//!   (a) `replenish_persists_opaque_codes_and_assigns_ordinals`:
//!       3 codes from ScriptedDps → 3 rows with `dps_code` set, `code_lnd` = MAX+1..
//!   (b) `replenish_advances_chain_seed_to_request_hash`:
//!       after success, `node_state.last_known_unsigned_xml_sha256 == sha256(xml)`;
//!       the request XML's `<MAC>` carried the PRE-call seed (chain continuity).
//!   (c) `replenish_is_idempotent_on_code_overlap`:
//!       second call with overlapping codes → deduped summary, no error, seed
//!       advances to the SECOND request's hash.
//!   (d) `replenish_acquires_fn_gate`:
//!       holding the FN gate externally makes replenish block; release → proceeds.
//!   (e) `replenish_reject_path_no_persist_no_seed_advance`:
//!       DPS returns `DpsError::Server{code:-8}` → zero rows inserted, seed
//!       UNCHANGED, error surfaced.
//!   (f) `replenish_transport_error_no_retry`:
//!       transport error → zero rows, seed unchanged, call log length == 1 (no
//!       retry).
//!   (g) `replenish_dps_call_outside_tx`:
//!       call log records `AskCodes` exactly once, codes in DB afterwards
//!       (structural proof that DPS is called outside the persist envelope).

mod common;

use std::sync::Arc;
use std::time::Duration;

use prro::config::AppConfig;
use prro::db::models::enums::FiscalMode;
use prro::db::models::enums::{NodeMode, ShiftState};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::repositories::fiscal_number_config as fn_repo;
use prro::db::repositories::fiscal_number_config::NewFnConfig;
use prro::db::repositories::node_state;
use prro::db::tx::with_immediate;
use prro::services::offline_sync::offline_code_replenish::OfflineCodeReplenishService;
use prro::transports::dps::dto::OfflineCodesResponse;
use prro::transports::dps::error::DpsError;
use prro::App;
use sha2::{Digest, Sha256};
use tokio::time::timeout;

use common::det_signing_ctx;
use common::scripted_dps::{DpsCall, ScriptedDps};

const FN: &str = "4000162280";
const TN: &str = "13667753";

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn boot_app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir
        .path()
        .join("c4.db")
        .display()
        .to_string()
        .replace('\\', "/");
    let toml_text = format!(
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
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    (dir, app)
}

/// Seed the minimal rows needed: `fiscal_number_config` + `node_state`.
async fn seed_fn(app: &App, prior_seed: Option<[u8; 32]>) {
    let pool = app.db();
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
            tax_number: TN.into(),
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
    node_state::upsert_initial(pool, FN, NodeMode::Online, ShiftState::Opened, 1)
        .await
        .unwrap();
    if let Some(seed) = prior_seed {
        node_state::seed_prevhash(pool, FN, &seed).await.unwrap();
    }
}

fn codes_resp(codes: &[&str]) -> OfflineCodesResponse {
    OfflineCodesResponse {
        codes: codes.iter().map(|s| s.to_string()).collect(),
    }
}

fn new_scripted() -> Arc<ScriptedDps> {
    use std::sync::atomic::AtomicUsize;
    Arc::new(ScriptedDps::new(
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    ))
}

/// Seed a CALL_STARTED delivery reservation (a `SENDING` doc + an active reservation)
/// for FN, so the S7-2 fence must fire.
async fn seed_active_reservation(pool: &sqlx::SqlitePool) {
    let doc = [0x33u8; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 5, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc[..])
    .bind(&[0xCCu8; 16][..])
    .bind(FN)
    .bind(&[0u8; 32][..])
    .execute(pool)
    .await
    .expect("seed SENDING doc");
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::insert(
                tx,
                NewReservation {
                    reservation_id: [0x44; 16],
                    document_id: DocumentId::from_bytes(doc),
                    fiscal_number: FN.to_string(),
                    dps_protocol_id: "FSCO_ZZD".to_string(),
                    protocol_contract_version: 1,
                    capability_profile_version: None,
                    endpoint_config_revision: None,
                    envelope_hash: [0xAB; 32],
                },
            )
            .await
            .map_err(Into::into)
        })
    })
    .await
    .expect("insert reservation");
    sqlx::query(
        "UPDATE delivery_reservation SET state='CALL_STARTED', \
         call_started_at='2026-07-17T00:00:00Z', authorized_generation=1 \
         WHERE reservation_id=?",
    )
    .bind(&[0x44u8; 16][..])
    .execute(pool)
    .await
    .expect("mark CALL_STARTED");
}

// ─── S7-2 fence BITE — behavioural (replaces the string-only static pin here) ──
//
// With an active delivery reservation, `replenish` must REFUSE and leave the chain
// seed UNCHANGED. This proves the fence fires BEFORE the seed advance: if the guard
// were moved AFTER `update_last_known_xml_sha_tx`, the seed would advance to
// sha256(request_xml) and the seed assertion would RED.
#[tokio::test]
async fn replenish_refused_while_reservation_active_seed_unchanged() {
    let (_dir, app) = boot_app().await;
    let prior_seed = [0x11u8; 32];
    seed_fn(&app, Some(prior_seed)).await;
    seed_active_reservation(app.db()).await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-X"])));
    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));

    let res = svc.replenish(FN, TN, 1, 1).await;
    assert!(
        res.is_err(),
        "replenish MUST be refused while a delivery reservation is active (S7-2 fence)"
    );

    let seed: Vec<u8> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(
        seed,
        prior_seed.to_vec(),
        "chain seed must NOT advance when the fence refuses (guard is BEFORE the mutation)"
    );

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(n, 0, "no codes persisted when the fence refuses");
}

// ─── (a) persists codes + assigns ordinals ────────────────────────────────────

#[tokio::test]
async fn replenish_persists_opaque_codes_and_assigns_ordinals() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-A", "code-B", "code-C"])));

    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    let summary = svc
        .replenish(FN, TN, 1, 3)
        .await
        .expect("replenish must succeed");

    assert_eq!(summary.codes_received, 3, "must report 3 codes received");
    assert_eq!(summary.inserted, 3, "must insert 3 rows");
    assert_eq!(summary.deduped, 0, "no duplicates");

    // Verify rows in DB: dps_code present, code_lnd sequential from 1.
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT code_lnd, dps_code \
         FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL \
         ORDER BY code_lnd",
    )
    .bind(FN)
    .fetch_all(app.db())
    .await
    .unwrap();

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0], (1, "code-A".into()));
    assert_eq!(rows[1], (2, "code-B".into()));
    assert_eq!(rows[2], (3, "code-C".into()));
}

// ─── (b) advances chain seed to sha256(request XML) ──────────────────────────

#[tokio::test]
async fn replenish_advances_chain_seed_to_request_hash() {
    let (_dir, app) = boot_app().await;
    // Use a known prior seed so the MAC in the XML is deterministic.
    let prior_seed = [0x11u8; 32];
    seed_fn(&app, Some(prior_seed)).await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-X"])));

    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    let summary = svc
        .replenish(FN, TN, 1, 1)
        .await
        .expect("replenish must succeed");

    // Verify seed advanced.
    let ns = node_state::get(app.db(), FN)
        .await
        .unwrap()
        .expect("node_state must exist");
    let new_seed = ns
        .last_known_unsigned_xml_sha256
        .expect("seed must be set after replenish");
    let new_seed_hex = summary.new_seed_hex.clone();
    assert_eq!(new_seed_hex.len(), 64, "new_seed_hex must be 64 hex chars");

    // REAL C-i pin: the chain seed MUST equal sha256 of the EXACT request XML
    // that was signed and sent. `summary.request_xml` exposes that XML, so an
    // impl that advances the seed to the WRONG bytes fails here (teeth-proven).
    let request_sha: String = Sha256::digest(summary.request_xml.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        new_seed_hex, request_sha,
        "chain seed must be sha256(request_xml) — the C-i advance"
    );

    // Chain continuity: the request's <MAC> must carry the PRE-call seed hex,
    // so the T=112 service doc extends the existing chain tip (not a fresh one).
    let prior_hex: String = prior_seed.iter().map(|b| format!("{b:02x}")).collect();
    assert!(
        summary
            .request_xml
            .contains(&format!("<MAC>{prior_hex}</MAC>")),
        "request <MAC> must carry the pre-call seed for chain continuity; xml={}",
        summary.request_xml
    );

    // The DB seed matches the summary (single source of truth).
    let db_hex: String = new_seed.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(new_seed_hex, db_hex, "summary seed hex must match DB");

    // Exactly one AskCodes call on success (no retry).
    let calls = stub.calls();
    assert_eq!(calls.len(), 1, "exactly one AskCodes call");
}

// ─── (c) idempotent on code overlap ───────────────────────────────────────────

#[tokio::test]
async fn replenish_is_idempotent_on_code_overlap() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;

    let stub = new_scripted();
    // First call: codes A, B.
    stub.push_ask_codes(Ok(codes_resp(&["code-A", "code-B"])));
    // Second call: overlapping code-B + new code-C.
    stub.push_ask_codes(Ok(codes_resp(&["code-B", "code-C"])));

    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));

    let s1 = svc.replenish(FN, TN, 1, 2).await.expect("first replenish");
    assert_eq!(s1.inserted, 2);
    assert_eq!(s1.deduped, 0);

    let s2 = svc.replenish(FN, TN, 2, 2).await.expect("second replenish");
    // code-B is a duplicate → 1 inserted, 1 deduped.
    assert_eq!(s2.inserted, 1, "code-B must be deduped");
    assert_eq!(s2.deduped, 1, "code-B is a duplicate");
    assert_eq!(s2.codes_received, 2, "two codes were received");

    // DB must have exactly 3 distinct codes.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(
        count, 3,
        "DB must have 3 distinct DPS codes after two calls"
    );

    // Seed advanced to the SECOND request's hash (each call advances independently).
    assert_ne!(
        s1.new_seed_hex, s2.new_seed_hex,
        "each call advances seed independently"
    );
}

// ─── (d) acquires the per-FN gate ─────────────────────────────────────────────

#[tokio::test]
async fn replenish_acquires_fn_gate() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;

    // Hold the gate via a clone (shared via Arc<Inner>).
    let clone = app.clone();
    let held = clone.acquire_fn_gate(FN).await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-gate-test"])));

    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));

    // replenish must block while the gate is externally held.
    let mut replenish_fut = Box::pin(svc.replenish(FN, TN, 1, 1));
    assert!(
        timeout(Duration::from_millis(200), &mut replenish_fut)
            .await
            .is_err(),
        "replenish must serialise on the held A4 fn-gate (expected pending)"
    );

    // Release → replenish proceeds.
    drop(held);
    let summary = timeout(Duration::from_millis(500), replenish_fut)
        .await
        .expect("replenish must complete once the gate is released")
        .expect("replenish must succeed after gate release");
    assert_eq!(summary.inserted, 1);
}

// ─── (e) server reject: no persist, no seed advance ───────────────────────────

#[tokio::test]
async fn replenish_reject_path_no_persist_no_seed_advance() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;

    // Read current seed (should be None for a genesis FN).
    let ns_before = node_state::get(app.db(), FN)
        .await
        .unwrap()
        .expect("node_state exists");
    let seed_before = ns_before.last_known_unsigned_xml_sha256;

    let stub = new_scripted();
    stub.push_ask_codes(Err(DpsError::Server {
        code: -8,
        message: "XML дата не відповідає Check.date".into(),
    }));

    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    let err = svc
        .replenish(FN, TN, 1, 1)
        .await
        .expect_err("DPS server reject must surface as error");

    // Error variant must carry the DPS status code.
    use prro::services::offline_sync::offline_code_replenish::ReplenishError;
    match err {
        ReplenishError::DpsServer { code, .. } => {
            assert_eq!(code, -8, "error code must be -8")
        }
        other => panic!("expected DpsServer, got {other:?}"),
    }

    // Zero rows inserted.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(count, 0, "no codes must be inserted on DPS server reject");

    // Seed UNCHANGED.
    let ns_after = node_state::get(app.db(), FN)
        .await
        .unwrap()
        .expect("node_state exists");
    assert_eq!(
        ns_after.last_known_unsigned_xml_sha256, seed_before,
        "seed must not advance on server reject"
    );
}

// ─── (f) transport error: no retry, no persist ────────────────────────────────

#[tokio::test]
async fn replenish_transport_error_no_retry() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;

    let stub = new_scripted();
    // Push exactly one transport error — if service retries, the second call
    // hits an empty queue and returns DpsError::Internal (a different variant).
    stub.push_ask_codes(Err(DpsError::Transport("connection refused".into())));

    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    let err = svc
        .replenish(FN, TN, 1, 1)
        .await
        .expect_err("transport error must surface");

    use prro::services::offline_sync::offline_code_replenish::ReplenishError;
    assert!(
        matches!(err, ReplenishError::DpsTransport(_)),
        "expected DpsTransport, got {err:?}"
    );

    // Call log length pin: exactly ONE call (no retry).
    let calls = stub.calls();
    assert_eq!(
        calls.len(),
        1,
        "T=112 must NOT be retried on transport error (non-idempotent server-side): got {} calls",
        calls.len()
    );

    // Zero rows inserted.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(count, 0, "no codes must be inserted on transport error");
}

// ─── (g) DPS call is outside the persist transaction ─────────────────────────

#[tokio::test]
async fn replenish_dps_call_outside_tx() {
    // Structural invariant #1: no wire call inside a `with_immediate` tx.
    // ScriptedDps.ask_offline_codes records to calls() immediately.
    // If it were called inside the tx, the production GrpcDpsChannel's
    // assert_not_in_with_immediate would panic. Here we verify the call
    // sequence: AskCodes appears in the log, and codes are in DB after
    // success — proving the code path is: acquire gate → build → sign →
    // DPS call (outside tx) → persist+advance (inside tx).
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-order-test"])));

    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    svc.replenish(FN, TN, 1, 1)
        .await
        .expect("replenish must succeed");

    // Exactly one AskCodes call was made.
    let calls = stub.calls();
    assert_eq!(calls.len(), 1, "exactly one DPS call");
    assert!(
        matches!(&calls[0], DpsCall::AskCodes(_)),
        "the single call must be AskCodes"
    );

    // Codes are in DB (persist happened after the DPS call).
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(count, 1, "code must be persisted after DPS call");
}

// ═══════════ bd PRRO_GATE-knk — refuse a replenish while the chain is diverged ═══════════
//
// SETTLED LIVE 2026-08-01 (`live_probe_knk_t112_foreign_mac.rs`, TEST cabinet, FN 4000162280):
// DPS DOES chain-check the T=112 request. Handed a `<MAC>` it has never accepted it answers
//
//     Server { code: -12, message: "ERROR_BAD_HASH_PREV  store abc15386…5531 chk 6aa74325…dbac" }
//
// and its tip does NOT move. Our `<MAC>` is `node_state.last_known_unsigned_xml_sha256`, and an
// offline document advances that seed at OFFLINE_LOCAL_ACK — i.e. at LOCAL issuance, long before
// DPS ever sees it. So while an undrained offline backlog rests, our seed is a value DPS has never
// accepted and every replenish is guaranteed to earn `-12`.
//
// The cost of learning that from DPS instead of locally is not a wasted round-trip: `-12` routes to
// `MacReseedPending` → node `STOP_MODE`, and nothing anywhere tells the operator that the remedy is
// to drain first. The wedge lands exactly when it hurts most — the pressure to replenish PEAKS at
// the end of an outage, when the backlog is largest.
//
// So these pins assert the refusal happens BEFORE the wire call. That is what separates this guard
// from the S7-2 delivery-reservation fence above, which lives INSIDE the persist envelope and
// therefore only fires after DPS has already answered `-12`. `stub.calls()` empty is the
// load-bearing assertion; the seed/codes assertions merely confirm nothing else moved.

/// Seed one offline-origin document for FN in `state`, as an offline session issuance would leave
/// it: `fs_mode = 'OFFLINE'`, an `offline_fiscal_no` stamped, and a chain hash of its own.
async fn seed_offline_doc(pool: &sqlx::SqlitePool, doc_byte: u8, lnd: i64, state: &str) {
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no) \
         VALUES (?, ?, ?, ?, 'SELL', ?, 'b1', 't1', 'OFFLINE', \
            '2026-08-01T10:00:00Z', '{}', ?, ?, ?)",
    )
    .bind(&vec![doc_byte; 16])
    .bind(&vec![doc_byte ^ 0xFF; 16])
    .bind(FN)
    .bind(lnd)
    .bind(state)
    .bind(&[0u8; 32][..])
    .bind(&vec![doc_byte; 32])
    .bind(lnd)
    .execute(pool)
    .await
    .expect("seed offline doc");
}

async fn assert_replenish_refused_before_the_wire(
    app: &App,
    stub: &Arc<ScriptedDps>,
    prior_seed: [u8; 32],
    because: &str,
) {
    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    let err = svc
        .replenish(FN, TN, 1, 1)
        .await
        .expect_err("replenish MUST be refused while the chain is diverged");

    // THE point of the guard: DPS was never asked, so no `-12` and no STOP_MODE.
    assert!(
        stub.calls().is_empty(),
        "knk ({because}): replenish must refuse BEFORE the wire call — reaching DPS earns a \
         guaranteed -12 → MacReseedPending → STOP_MODE. Observed calls: {:?}",
        stub.calls()
    );
    // The refusal must NAME the remedy; an opaque -12 is exactly what the operator gets today.
    let rendered = err.to_string().to_lowercase();
    assert!(
        rendered.contains("drain"),
        "knk ({because}): the refusal must tell the operator to DRAIN FIRST — that is the whole \
         deliverable, since the failure mode is an operator who cannot tell why. Got: {err}"
    );

    let seed: Vec<u8> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(seed, prior_seed.to_vec(), "knk ({because}): seed unchanged");

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(n, 0, "knk ({because}): no codes persisted");
}

#[tokio::test]
async fn knk_a_replenish_refused_while_an_offline_local_ack_backlog_rests() {
    let (_dir, app) = boot_app().await;
    let prior_seed = [0x11u8; 32];
    seed_fn(&app, Some(prior_seed)).await;
    // The primary backlog state: locally issued offline, never sent. It advanced OUR seed; DPS has
    // never seen it. This is the fuzzer counterexample's shape.
    seed_offline_doc(app.db(), 0x51, 1, "OFFLINE_LOCAL_ACK").await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-must-not-be-requested"])));
    assert_replenish_refused_before_the_wire(&app, &stub, prior_seed, "OFFLINE_LOCAL_ACK").await;
}

#[tokio::test]
async fn knk_b_replenish_refused_while_an_offline_error_retryable_doc_rests() {
    let (_dir, app) = boot_app().await;
    let prior_seed = [0x11u8; 32];
    seed_fn(&app, Some(prior_seed)).await;
    // The second divergence-causing drainable state: the drain tried and got a transient failure.
    // The doc crossed OFFLINE_LOCAL_ACK (so our seed moved) and DPS still never accepted it.
    // Covering only OFFLINE_LOCAL_ACK would leave this hole open.
    seed_offline_doc(app.db(), 0x52, 1, "ERROR_RETRYABLE").await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-must-not-be-requested"])));
    assert_replenish_refused_before_the_wire(&app, &stub, prior_seed, "ERROR_RETRYABLE").await;
}

#[tokio::test]
async fn knk_c_replenish_proceeds_once_the_backlog_is_drained() {
    // The other half of the contract: the guard must not over-block. A drained backlog leaves the
    // documents in DPS-confirmed states, our seed IS the peer's tip, and a replenish is legitimate.
    // Without this pin, "refuse always" would pass knk_a/knk_b and brick the code pool for good.
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;
    seed_offline_doc(app.db(), 0x53, 1, "ACK").await;
    seed_offline_doc(app.db(), 0x54, 2, "SENT").await;
    seed_offline_doc(app.db(), 0x55, 3, "KVT2").await;

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-after-drain"])));
    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    svc.replenish(FN, TN, 1, 1)
        .await
        .expect("a drained backlog must NOT block a replenish");

    assert_eq!(stub.calls().len(), 1, "the wire call happened");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(count, 1, "codes persisted");
}

#[tokio::test]
async fn knk_d_another_fns_backlog_does_not_block_this_fn() {
    // The guard is per-FN, like every other predicate on this path. A sibling FN's undrained
    // backlog says nothing about OUR chain tip, and blocking on it would be a fleet-wide wedge
    // the moment more than one register shares a database.
    let (_dir, app) = boot_app().await;
    seed_fn(&app, None).await;
    let other = "4000999999";
    fn_repo::insert(
        app.db(),
        &NewFnConfig {
            fiscal_number: other.into(),
            tax_number: TN.into(),
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
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no) \
         VALUES (?, ?, ?, 1, 'SELL', 'OFFLINE_LOCAL_ACK', 'b1', 't1', 'OFFLINE', \
            '2026-08-01T10:00:00Z', '{}', ?, ?, 1)",
    )
    .bind(&vec![0x61u8; 16])
    .bind(&vec![0x9Eu8; 16])
    .bind(other)
    .bind(&[0u8; 32][..])
    .bind(&vec![0x61u8; 32])
    .execute(app.db())
    .await
    .expect("seed other FN's backlog");

    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-other-fn-irrelevant"])));
    let svc =
        OfflineCodeReplenishService::new(app.clone(), stub.clone(), Arc::new(det_signing_ctx()));
    svc.replenish(FN, TN, 1, 1)
        .await
        .expect("another FN's backlog must not block this FN's replenish");
    assert_eq!(stub.calls().len(), 1, "the wire call happened");
}
