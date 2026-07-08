//! A′.1 Phase C piece 3 — stage_acquire online shift-create (edge 1) +
//! Opened→Closing (edge 8), plus the P6-ruling fail-closed refusal of a
//! SHIFT_OPEN with no cashier identity.
//!
//! A′.3 PR-O3 slice 1 extends this catalog to the OFFLINE channel:
//!   - (g) INVERTED from the pre-O3 "offline writes no shift" pin to the
//!     edge-2 create (`Created → OpenedLocalPendingDrain`) — offline
//!     shift-create is no longer deferred to offline-ack.
//!   - (j) offline SHIFT_OPEN missing-cashier refusal (mirror of (i)) —
//!     the Step-6b′ pre-mint refusal is now channel-agnostic (§16.8).
//!
//! Drives `stage_acquire::run` directly (same idiom as
//! `write_path_stage1_acquire.rs`) and asserts BOTH the `WorkerProcessResult`
//! AND the persisted DB state (shifts row + node_state projection + doc
//! binding + lnd + audit), with explicit attention to the pre-mint /
//! four-negation discipline for the cashier refusal.
//!
//! Contract tests (a)-(h) + ruling addition (i) + PR-O3 (g-inversion, j).
//! Two hooks only; the guard matrix (`check_shift_guard`) is untouched.

use prro::db::models::enums::{DocType, FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::{DocumentId, RequestId, ShiftId};
use prro::db::repositories::{
    fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig, ingress_inbox as inbox,
    ingress_inbox::NewInboxEntry,
};
use prro::db::{open_pool, open_secure_pool};
use prro::services::write_path::{
    stage_acquire,
    types::{CanonicalFiscalCommand, RejectionReason, WorkerProcessResult},
};

const FN: &str = "5000000003";
const DRIVER: &str = "test-driver";
const CASHIER: &str = "csh-piece3";

// ─── harness ──────────────────────────────────────────────────────────

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p3.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn fresh_secure_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p3-secure.db");
    std::mem::forget(dir);
    open_secure_pool(&path).await.unwrap()
}

async fn seed_fn_config(pool: &sqlx::SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
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
        },
    )
    .await
    .unwrap();
}

async fn seed_node_state(
    pool: &sqlx::SqlitePool,
    mode: NodeMode,
    shift_state: ShiftState,
    current_shift_id: Option<ShiftId>,
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
    .bind(current_shift_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_shift(pool: &sqlx::SqlitePool, shift_id: ShiftId, state: ShiftState) {
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, ?, 'ONLINE', 0, 'seed-cashier')",
    )
    .bind(shift_id)
    .bind(FN)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_inbox(pool: &sqlx::SqlitePool, doc_type: DocType, cashier: Option<&str>) -> [u8; 16] {
    let req_id = *RequestId::new().as_bytes();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: req_id,
            fiscal_number: FN.into(),
            protocol: prro::db::models::enums::Protocol::Rest,
            operation_type: doc_type.as_str().into(),
            idempotency_key: format!("idem-{}", hexstr(&req_id)),
            payload_json: r#"{"goods":[]}"#.into(),
            payload_sha256_canonical: [0u8; 32],
            correlation_id: None,
            signed_by_cashier_id: cashier.map(|c| c.to_string()),
            driver_id: Some("drv".into()),
            business_ts: None,
            total_sum_kop: None,
        },
    )
    .await
    .unwrap();
    req_id
}

/// Raw-seed a terminal doc occupying `(FN, lnd)` so a later acquire that
/// allocates the same `lnd` collides on `ux_fd_fn_lnd` — the (h) fault lever.
async fn seed_colliding_doc(pool: &sqlx::SqlitePool, lnd: i64) {
    let doc_id = *DocumentId::new().as_bytes();
    let req_id = *RequestId::new().as_bytes();
    sqlx::query(
        "INSERT INTO fiscal_documents \
         (document_id, request_id, fiscal_number, lnd, doc_type, state, \
          backend_profile_id, transport_profile_id, fs_mode, business_ts, \
          payload_json, payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'ACK', 'b', 't', 'ONLINE', \
                 '2026-01-01T00:00:00Z', '{}', ?)",
    )
    .bind(doc_id.as_slice())
    .bind(req_id.as_slice())
    .bind(FN)
    .bind(lnd)
    .bind([0u8; 32].as_slice())
    .execute(pool)
    .await
    .unwrap();
}

fn cmd(doc_type: DocType, cashier: Option<&str>) -> CanonicalFiscalCommand {
    CanonicalFiscalCommand {
        doc_type,
        business_ts: "2026-04-22T12:00:00Z".into(),
        total_sum_kop: Some(15000),
        payload_json: r#"{"goods":[]}"#.into(),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        signed_by_cashier_id: cashier.map(|c| prro::db::models::ids::CashierId::new(c).unwrap()),
        driver_id: None,
    }
}

// ─── readers ──────────────────────────────────────────────────────────

async fn doc_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn shift_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM shifts")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn next_lnd(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT next_lnd FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn node_shift_state(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn node_current_shift_id(pool: &sqlx::SqlitePool) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT current_shift_id FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn shift_state_by_id(pool: &sqlx::SqlitePool, shift_id: ShiftId) -> Option<String> {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_optional(pool)
        .await
        .unwrap()
}
async fn doc_shift_id(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT shift_id FROM fiscal_documents WHERE document_id = ?")
        .bind(doc_id)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn shift_opener_by_id(pool: &sqlx::SqlitePool, shift_id: ShiftId) -> String {
    sqlx::query_scalar("SELECT opened_by_cashier_id FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn audit_count(pool: &sqlx::SqlitePool, event: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn hexstr(b: &[u8; 16]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ─── (a) edge 1 — fresh online SHIFT_OPEN creates+Opens shift atomically ─
#[tokio::test]
async fn a_fresh_shift_open_creates_shift_opening_and_binds_doc() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Closed, None).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, Some(CASHIER)).await;

    let result = stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await
    .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    let doc_id = ctx.document.document_id;
    let expected = ShiftId::deterministic_for_shift_open(doc_id);

    // one shifts row, CREATED→OPENING (edge 1 atomic with create)
    assert_eq!(shift_count(&pool).await, 1);
    assert_eq!(
        shift_state_by_id(&pool, expected).await.as_deref(),
        Some("OPENING")
    );
    // node_state projection mirrored + pointer set to the deterministic id
    assert_eq!(node_shift_state(&pool).await, "OPENING");
    assert_eq!(
        node_current_shift_id(&pool).await.as_deref(),
        Some(expected.as_bytes().as_slice())
    );
    // doc bound to the new shift
    assert_eq!(
        doc_shift_id(&pool, doc_id).await.as_deref(),
        Some(expected.as_bytes().as_slice())
    );
    // ruling parity: opener == command signer
    assert_eq!(shift_opener_by_id(&pool, expected).await, CASHIER);
    assert_eq!(next_lnd(&pool).await, 2, "lnd consumed on the happy path");
}

// ─── (b) re-drive same request_id → Resumed, no second create ───────────
#[tokio::test]
async fn b_redrive_same_request_id_no_second_create() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Closed, None).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, Some(CASHIER)).await;

    stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await
    .unwrap();
    assert_eq!(shift_count(&pool).await, 1);

    // Realistic client re-drive: the inbox lease is still PROCESSING from
    // the first run, so `acquire_lease` short-circuits with `Noop` BEFORE
    // reaching the create hook — the primary idempotency mechanism (the
    // deterministic shift_id is only the PK backstop).  No second create,
    // no error.  (A crashed PREPARED doc is resumed forward by the boot
    // dispatcher from its own state, not by a fresh acquire — see the
    // resume×guard residual noted in the delivery report.)
    let again = stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await
    .unwrap();
    assert!(
        matches!(again, WorkerProcessResult::Noop),
        "re-drive must short-circuit (lease held) with no second create, got {again:?}"
    );
    assert_eq!(shift_count(&pool).await, 1, "no second shifts row");
    assert_eq!(doc_count(&pool).await, 1, "no second doc");
    assert_eq!(node_shift_state(&pool).await, "OPENING");
}

// ─── (c) second SHIFT_OPEN (diff req) while Opened → ShiftAlreadyOpen ────
#[tokio::test]
async fn c_second_shift_open_while_opened_is_already_open() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let existing = ShiftId::new();
    seed_shift(&pool, existing, ShiftState::Opened).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Opened, Some(existing)).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, Some(CASHIER)).await;

    let result = stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await
    .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftAlreadyOpen
            }
        ),
        "got {result:?}"
    );
    assert_eq!(
        shift_count(&pool).await,
        1,
        "guard refused before any create"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1, "lnd not consumed");
}

// ─── (d) Z at Opened → edge 8 Opened→Closing + doc bound ────────────────
#[tokio::test]
async fn d_z_report_at_opened_drives_closing() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Opened, Some(shift)).await;
    let req = seed_inbox(&pool, DocType::ZReport, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ZReport, None))
        .await
        .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(node_shift_state(&pool).await, "CLOSING");
    assert_eq!(
        shift_state_by_id(&pool, shift).await.as_deref(),
        Some("CLOSING")
    );
    assert_eq!(
        doc_shift_id(&pool, ctx.document.document_id)
            .await
            .as_deref(),
        Some(shift.as_bytes().as_slice()),
        "Z doc bound to current_shift_id"
    );
}

// ─── (e) Z at Closed → guard refuse (ShiftNotOpen), no shift-writes ─────
#[tokio::test]
async fn e_z_report_at_closed_is_shift_not_open() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Closed, None).await;
    let req = seed_inbox(&pool, DocType::ZReport, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ZReport, None))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftNotOpen {
                    current: ShiftState::Closed
                }
            }
        ),
        "got {result:?}"
    );
    assert_eq!(shift_count(&pool).await, 0);
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1);
}

// ─── (f) SELL@Opened allowed + shift-neutral; SELL@Opening refused ──────
#[tokio::test]
async fn f_sell_at_opened_allowed_shift_neutral() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Opened, Some(shift)).await;
    let req = seed_inbox(&pool, DocType::Sell, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::Sell, None))
        .await
        .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    // shift state UNCHANGED (SELL does not drive the shift ladder)
    assert_eq!(node_shift_state(&pool).await, "OPENED");
    assert_eq!(
        shift_state_by_id(&pool, shift).await.as_deref(),
        Some("OPENED")
    );
    // but bound to current_shift_id (existing Step 5 behavior, P2)
    assert_eq!(
        doc_shift_id(&pool, ctx.document.document_id)
            .await
            .as_deref(),
        Some(shift.as_bytes().as_slice())
    );
    assert_eq!(shift_count(&pool).await, 1, "no new shift");
}

#[tokio::test]
async fn f_sell_at_opening_is_refused_inv03() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opening).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Opening, Some(shift)).await;
    let req = seed_inbox(&pool, DocType::Sell, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::Sell, None))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftNotOpen {
                    current: ShiftState::Opening
                }
            }
        ),
        "INV-03: SELL refused while shift is Opening (pre-DPS-confirm), got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0);
    assert_eq!(next_lnd(&pool).await, 1);
}

// ─── (g) edge 2 — fresh offline SHIFT_OPEN creates+opens shift at
//         OpenedLocalPendingDrain atomically.  A′.3 PR-O3 INVERTS the
//         pre-O3 "offline writes no shift" pin: create is no longer
//         deferred to offline-ack — the offline channel mirrors edge 1
//         but drives `Created → OpenedLocalPendingDrain` (Pattern C). ──
#[tokio::test]
async fn g_offline_shift_open_creates_opened_local_pending_drain_and_binds_doc() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Closed, None).await;
    // A′.3 PR-O3 fix (a): offline shift-lifecycle acquire now REQUIRES an OPEN
    // session + a non-empty code pool (else pre-mint refused) — seed both so
    // the edge under test is reached.
    seed_offline_session_open(&pool).await;
    seed_offline_code(&pool, 9100).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, Some(CASHIER)).await;

    let result = stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await
    .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    let doc_id = ctx.document.document_id;
    let expected = ShiftId::deterministic_for_shift_open(doc_id);

    // one shifts row, CREATED→OPENED_LOCAL_PENDING_DRAIN (edge 2 atomic with create)
    assert_eq!(shift_count(&pool).await, 1);
    assert_eq!(
        shift_state_by_id(&pool, expected).await.as_deref(),
        Some("OPENED_LOCAL_PENDING_DRAIN")
    );
    // node_state projection mirrored + pointer set to the deterministic id
    assert_eq!(node_shift_state(&pool).await, "OPENED_LOCAL_PENDING_DRAIN");
    assert_eq!(
        node_current_shift_id(&pool).await.as_deref(),
        Some(expected.as_bytes().as_slice())
    );
    // doc bound to the new shift
    assert_eq!(
        doc_shift_id(&pool, doc_id).await.as_deref(),
        Some(expected.as_bytes().as_slice())
    );
    // §16.8 parity (channel-agnostic): opener == command signer
    assert_eq!(shift_opener_by_id(&pool, expected).await, CASHIER);
    assert_eq!(next_lnd(&pool).await, 2, "lnd consumed on the happy path");
}

// ─── (h) crash-atomicity: post-create failure rolls the shift back ──────
#[tokio::test]
async fn h_create_is_atomic_with_doc_insert_rollback() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Closed, None).await;
    // Pre-occupy lnd=1 so the acquire's insert_prepared_tx collides on
    // ux_fd_fn_lnd AFTER create_shift_tx + edge 1 already ran in-tx.
    seed_colliding_doc(&pool, 1).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, Some(CASHIER)).await;

    let result = stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await;
    assert!(
        result.is_err(),
        "lnd collision must surface as a stage error"
    );
    // Full rollback: no new shift row, projection untouched, pointer clear.
    assert_eq!(shift_count(&pool).await, 0, "shifts INSERT rolled back");
    assert_eq!(
        node_shift_state(&pool).await,
        "CLOSED",
        "projection rolled back"
    );
    assert_eq!(
        node_current_shift_id(&pool).await,
        None,
        "pointer rolled back"
    );
    assert_eq!(doc_count(&pool).await, 1, "only the pre-seeded doc");
    assert_eq!(next_lnd(&pool).await, 1, "lnd allocation rolled back");
}

// ─── (i) online SHIFT_OPEN with no cashier → ShiftOpenMissingCashier ────
#[tokio::test]
async fn i_online_shift_open_missing_cashier_is_refused_premint() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Online, ShiftState::Closed, None).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ShiftOpen, None))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftOpenMissingCashier
            }
        ),
        "got {result:?}"
    );
    // four negations: no doc / no shift / no node_state change / no lnd
    assert_eq!(
        doc_count(&pool).await,
        0,
        "no doc minted (pre-mint refusal)"
    );
    assert_eq!(shift_count(&pool).await, 0, "no shifts row");
    assert_eq!(
        node_shift_state(&pool).await,
        "CLOSED",
        "shift_state untouched"
    );
    assert_eq!(
        node_current_shift_id(&pool).await,
        None,
        "pointer untouched"
    );
    assert_eq!(next_lnd(&pool).await, 1, "lnd not consumed");
    // audit event present
    assert_eq!(audit_count(&pool, "SHIFT_OPEN_MISSING_CASHIER").await, 1);
}

// ─── (j) edge 2 guard — offline SHIFT_OPEN with no cashier →
//         ShiftOpenMissingCashier (A′.3 PR-O3: §16.8 1-cashier-per-shift
//         is channel-agnostic, so the Step-6b′ pre-mint refusal now
//         covers offline too — the offline create arm can safely
//         `.expect` a cashier without a write-path panic).  Mirror of (i). ─
#[tokio::test]
async fn j_offline_shift_open_missing_cashier_is_refused_premint() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Closed, None).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ShiftOpen, None))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::ShiftOpenMissingCashier
            }
        ),
        "got {result:?}"
    );
    // four negations: no doc / no shift / no node_state change / no lnd
    assert_eq!(
        doc_count(&pool).await,
        0,
        "no doc minted (pre-mint refusal)"
    );
    assert_eq!(shift_count(&pool).await, 0, "no shifts row");
    assert_eq!(
        node_shift_state(&pool).await,
        "CLOSED",
        "shift_state untouched"
    );
    assert_eq!(
        node_current_shift_id(&pool).await,
        None,
        "pointer untouched"
    );
    assert_eq!(next_lnd(&pool).await, 1, "lnd not consumed");
    assert_eq!(audit_count(&pool, "SHIFT_OPEN_MISSING_CASHIER").await, 1);
}

// ─── (k) edge 9 — offline Z_REPORT at Opened → Opened→CLPD + doc bound.
//         A′.3 PR-O3 slice 2: the offline mirror of (d) — instead of edge 8
//         (Opened→Closing, which the online send-ladder drives to Closed via
//         edge 10), the offline channel drives Opened→ClosingLocalPendingDrain
//         (edge 9); the local-Z doc then local-acks + drains later (edge 13). ─
#[tokio::test]
async fn k_offline_z_report_at_opened_drives_closing_local_pending_drain() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened, Some(shift)).await;
    seed_offline_session_open(&pool).await; // fix (a) precondition
    seed_offline_code(&pool, 9101).await;
    let req = seed_inbox(&pool, DocType::ZReport, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ZReport, None))
        .await
        .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(node_shift_state(&pool).await, "CLOSING_LOCAL_PENDING_DRAIN");
    assert_eq!(
        shift_state_by_id(&pool, shift).await.as_deref(),
        Some("CLOSING_LOCAL_PENDING_DRAIN")
    );
    assert_eq!(
        doc_shift_id(&pool, ctx.document.document_id)
            .await
            .as_deref(),
        Some(shift.as_bytes().as_slice()),
        "offline Z doc bound to current_shift_id"
    );
}

// ─── (l) edge 9 — offline SHIFT_CLOSE at Opened → Opened→CLPD (edge 9
//         admits both Z_REPORT and SHIFT_CLOSE on an Opened shift). ─────────
#[tokio::test]
async fn l_offline_shift_close_at_opened_drives_closing_local_pending_drain() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened, Some(shift)).await;
    seed_offline_session_open(&pool).await; // fix (a) precondition
    seed_offline_code(&pool, 9102).await;
    let req = seed_inbox(&pool, DocType::ShiftClose, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ShiftClose, None))
        .await
        .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(node_shift_state(&pool).await, "CLOSING_LOCAL_PENDING_DRAIN");
    assert_eq!(
        shift_state_by_id(&pool, shift).await.as_deref(),
        Some("CLOSING_LOCAL_PENDING_DRAIN")
    );
    assert_eq!(
        doc_shift_id(&pool, ctx.document.document_id)
            .await
            .as_deref(),
        Some(shift.as_bytes().as_slice()),
        "offline SHIFT_CLOSE doc bound to current_shift_id"
    );
}

// ─── STOP-O3-1 fix (a) helpers: offline session + code-pool seeding ────────

async fn seed_offline_session_open(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-07-07T00:00:00Z')",
    )
    .bind(prro::db::models::ids::OfflineSessionId::new())
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_offline_code(pool: &sqlx::SqlitePool, code_lnd: i64) {
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES (?, ?)")
        .bind(FN)
        .bind(code_lnd)
        .execute(pool)
        .await
        .unwrap();
}

// ─── (o) STOP-O3-1 fix (a) — offline SHIFT_OPEN with NO OPEN session →
//         PRE-MINT refusal (audit-only): no doc, NO shift transition, no lnd.
//         The pre-acquire class (6b′/D5): the lifecycle doc must not be born
//         into an abort that would orphan the shift. ─────────────────────────
#[tokio::test]
async fn o_offline_shift_open_without_open_session_refused_premint() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Closed, None).await;
    // A code exists but NO offline session is OPEN.
    seed_offline_code(&pool, 9000).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, Some(CASHIER)).await;

    let result = stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await
    .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::OfflineLifecycleNoActiveSession
            }
        ),
        "got {result:?}"
    );
    // The DOUBLE ABSENCE pin (ruling): neither a doc nor a shift transition.
    assert_eq!(
        doc_count(&pool).await,
        0,
        "no doc minted (pre-mint refusal)"
    );
    assert_eq!(shift_count(&pool).await, 0, "no shifts row / no edge 2");
    assert_eq!(
        node_shift_state(&pool).await,
        "CLOSED",
        "projection untouched"
    );
    assert_eq!(next_lnd(&pool).await, 1, "lnd not consumed");
    assert_eq!(
        audit_count(&pool, "OFFLINE_LIFECYCLE_NO_ACTIVE_SESSION_REFUSED").await,
        1
    );
}

// ─── (p) STOP-O3-1 fix (a) — offline SHIFT_OPEN with an EMPTY code pool →
//         PRE-MINT refusal (the mundane "morning without seeded codes"):
//         no doc, NO shift transition, no lnd. ───────────────────────────────
#[tokio::test]
async fn p_offline_shift_open_with_empty_code_pool_refused_premint() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Closed, None).await;
    // Session is OPEN but the pool has ZERO unconsumed codes.
    seed_offline_session_open(&pool).await;
    let req = seed_inbox(&pool, DocType::ShiftOpen, Some(CASHIER)).await;

    let result = stage_acquire::run(
        &pool,
        &secure,
        DRIVER,
        req,
        cmd(DocType::ShiftOpen, Some(CASHIER)),
    )
    .await
    .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::OfflineLifecycleCodePoolEmpty
            }
        ),
        "got {result:?}"
    );
    // The DOUBLE ABSENCE pin (ruling): neither a doc nor a shift transition.
    assert_eq!(
        doc_count(&pool).await,
        0,
        "no doc minted (pre-mint refusal)"
    );
    assert_eq!(shift_count(&pool).await, 0, "no shifts row / no edge 2");
    assert_eq!(
        node_shift_state(&pool).await,
        "CLOSED",
        "projection untouched"
    );
    assert_eq!(next_lnd(&pool).await, 1, "lnd not consumed");
    assert_eq!(
        audit_count(&pool, "OFFLINE_LIFECYCLE_CODE_POOL_EMPTY_REFUSED").await,
        1
    );
}

// ─── (q) STOP-O3-1 fix (a) — offline Z_REPORT on an Opened shift with an
//         EMPTY pool → PRE-MINT refusal and the shift STAYS Opened (edge 9
//         NOT driven) — the close is refused honestly instead of being born
//         into an abort that would orphan the shift at CLPD (the 24h-trap
//         becomes a typed retryable refusal, not an RMR event). ──────────────
#[tokio::test]
async fn q_offline_z_report_with_empty_code_pool_refused_premint_shift_stays_opened() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::Opened).await;
    seed_node_state(&pool, NodeMode::Offline, ShiftState::Opened, Some(shift)).await;
    seed_offline_session_open(&pool).await;
    // ZERO unconsumed codes — the close would have nothing to consume.
    let req = seed_inbox(&pool, DocType::ZReport, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ZReport, None))
        .await
        .unwrap();
    assert!(
        matches!(
            result,
            WorkerProcessResult::Rejected {
                reason: RejectionReason::OfflineLifecycleCodePoolEmpty
            }
        ),
        "got {result:?}"
    );
    assert_eq!(doc_count(&pool).await, 0, "no doc minted");
    assert_eq!(
        node_shift_state(&pool).await,
        "OPENED",
        "edge 9 NOT driven — the shift stays Opened, retry after re-provisioning"
    );
    assert_eq!(
        shift_state_by_id(&pool, shift).await.as_deref(),
        Some("OPENED"),
        "shifts row untouched"
    );
    assert_eq!(next_lnd(&pool).await, 1, "lnd not consumed");
}

// ─── (m) edge 7 — offline Z_REPORT at OpenedLocalPendingDrain → OLPD→CLPD.
//         The "full offline day" close: the SHIFT_OPEN itself hasn't drained
//         yet.  Reachable only after the slice-3 guardrail-lift (the guard
//         admits offline close of an OLPD shift). ────────────────────────────
#[tokio::test]
async fn m_offline_z_report_at_opened_local_pending_drain_drives_closing_local_pending_drain() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::OpenedLocalPendingDrain).await;
    seed_node_state(
        &pool,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
        Some(shift),
    )
    .await;
    seed_offline_session_open(&pool).await; // fix (a) precondition
    seed_offline_code(&pool, 9103).await;
    let req = seed_inbox(&pool, DocType::ZReport, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ZReport, None))
        .await
        .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(node_shift_state(&pool).await, "CLOSING_LOCAL_PENDING_DRAIN");
    assert_eq!(
        shift_state_by_id(&pool, shift).await.as_deref(),
        Some("CLOSING_LOCAL_PENDING_DRAIN")
    );
    assert_eq!(
        doc_shift_id(&pool, ctx.document.document_id)
            .await
            .as_deref(),
        Some(shift.as_bytes().as_slice()),
        "offline Z doc bound to current_shift_id"
    );
}

// ─── (n) edge 7 — offline SHIFT_CLOSE at OpenedLocalPendingDrain → OLPD→CLPD. ─
#[tokio::test]
async fn n_offline_shift_close_at_opened_local_pending_drain_drives_closing_local_pending_drain() {
    let pool = fresh_pool().await;
    let secure = fresh_secure_pool().await;
    seed_fn_config(&pool).await;
    let shift = ShiftId::new();
    seed_shift(&pool, shift, ShiftState::OpenedLocalPendingDrain).await;
    seed_node_state(
        &pool,
        NodeMode::Offline,
        ShiftState::OpenedLocalPendingDrain,
        Some(shift),
    )
    .await;
    seed_offline_session_open(&pool).await; // fix (a) precondition
    seed_offline_code(&pool, 9104).await;
    let req = seed_inbox(&pool, DocType::ShiftClose, None).await;

    let result = stage_acquire::run(&pool, &secure, DRIVER, req, cmd(DocType::ShiftClose, None))
        .await
        .unwrap();
    let ctx = match result {
        WorkerProcessResult::Proceed(c) => c,
        other => panic!("expected Proceed, got {other:?}"),
    };
    assert_eq!(node_shift_state(&pool).await, "CLOSING_LOCAL_PENDING_DRAIN");
    assert_eq!(
        shift_state_by_id(&pool, shift).await.as_deref(),
        Some("CLOSING_LOCAL_PENDING_DRAIN")
    );
    assert_eq!(
        doc_shift_id(&pool, ctx.document.document_id)
            .await
            .as_deref(),
        Some(shift.as_bytes().as_slice()),
        "offline SHIFT_CLOSE doc bound to current_shift_id"
    );
}
