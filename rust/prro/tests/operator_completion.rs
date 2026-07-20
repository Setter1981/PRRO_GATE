//! CS-3 Slice 5 — STOP operator completion of a PENDING reservation.
//!
//! RED-first teeth for `delivery_reservation::complete_operator_pending` (design §3.4). A
//! reservation held PENDING under STOP_MODE (a SubmittedUnknown / crashed send) is released only by
//! a typed operator resolution + the full authority CAS; the origin-split effect + APPLIED +
//! pointer-clear + verified mode target all commit together. INACTIVE — driven by the extended
//! reset_stop_mode admin path only at the cutover (Slice 7).
//!
//! - `oc01` accepted online: stamp F + seed advance + APPLIED + pointer clear + mode ONLINE;
//! - `oc02` accepted offline: stamp F + ZERO seed + APPLIED;
//! - `oc03` not-accepted online: doc Sending→RMR + APPLIED + seed unchanged;
//! - `oc04` MAC reseed: operator seed + doc→RMR + APPLIED;
//! - `oc05` mode target GOING_ONLINE when an active offline session must drain;
//! - `oc06` offline not-accepted → refused (cohort cleanup, Slice 5b) — nothing mutated;
//! - `oc07` shift-family doc → refused (Slice 5b) — nothing mutated;
//! - `oc08` stale generation → StaleAuthority — nothing mutated;
//! - `oc09` after completion the pointer is clear and the next document can authorize.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    self, complete_operator_pending, resume_crashed_reservation, CompletionError, CompletionResult,
    ModeTarget, NewReservation, OperatorResolution,
};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

const TS: &str = "2026-07-20T00:00:00Z";
const SEED: [u8; 32] = [0x77; 32];
const OPSEED: [u8; 32] = [0x99; 32];

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

async fn seed_doc(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    doc_type: &str,
    offline_no: Option<i64>,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CREATED', 1)",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no) \
         VALUES (?, ?, ?, ?, ?, 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(doc_byte as i64)
    .bind(doc_type)
    .bind(vec![0u8; 32])
    .bind(&SEED[..])
    .bind(offline_no)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

fn new_res(res_byte: u8, doc: DocumentId, fscl: &str) -> NewReservation {
    NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fscl.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    }
}

/// Authorize (RN→CALL_STARTED) then boot-resume the crash → OUTCOME_OBSERVED + PENDING_APPLY +
/// node STOP_MODE. This is the realistic held state the operator completes.
async fn held_pending(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) {
    let row = new_res(res_byte, doc, fscl);
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize");
    let fscl_owned = fscl.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            resume_crashed_reservation(tx, [res_byte; 16], &fscl_owned)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("resume to PENDING+STOP");
}

async fn complete(
    pool: &SqlitePool,
    res_byte: u8,
    resolution: OperatorResolution,
) -> Result<CompletionResult, anyhow::Error> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            complete_operator_pending(tx, [res_byte; 16], resolution)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

async fn read_sfn(pool: &SqlitePool, doc_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id=?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn read_seed(pool: &SqlitePool, fscl: &str) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number=?",
    )
    .bind(fscl)
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn read_apply_state(pool: &SqlitePool, res_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT apply_state FROM delivery_reservation WHERE reservation_id=?")
        .bind(&[res_byte; 16][..])
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn read_pointer(pool: &SqlitePool, fscl: &str) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT active_delivery_reservation_id FROM node_state WHERE fiscal_number=?",
    )
    .bind(fscl)
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn read_mode(pool: &SqlitePool, fscl: &str) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number=?")
        .bind(fscl)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn read_doc_state(pool: &SqlitePool, doc_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id=?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}
fn is_err(err: &anyhow::Error, pred: impl Fn(&CompletionError) -> bool) -> bool {
    err.downcast_ref::<CompletionError>().is_some_and(pred)
}

// ───────────────────────────── oc01 accepted online ──────────────────────────

#[tokio::test]
async fn oc01_accepted_online_stamps_seed_applies_and_returns_online() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000001";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
    let r = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000111111".into(),
        },
    )
    .await
    .expect("accepted completes");
    assert!(r.applied && r.seed_advanced && r.mode_target == ModeTarget::Online);
    assert_eq!(read_sfn(&pool, 0x11).await.as_deref(), Some("4000111111"));
    assert_eq!(read_seed(&pool, fscl).await.as_deref(), Some(&SEED[..]));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
    assert!(read_pointer(&pool, fscl).await.is_none());
    assert_eq!(read_mode(&pool, fscl).await, "ONLINE");
}

// ───────────────────────────── oc02 accepted offline ─────────────────────────

#[tokio::test]
async fn oc02_accepted_offline_no_seed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000002";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", Some(500)).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let r = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000222222".into(),
        },
    )
    .await
    .expect("offline accepted completes");
    assert!(r.applied && !r.seed_advanced);
    assert_eq!(read_sfn(&pool, 0x11).await.as_deref(), Some("4000222222"));
    assert!(
        read_seed(&pool, fscl).await.is_none(),
        "offline never advances the seed"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
}

// ───────────────────────────── oc03 not-accepted online ──────────────────────

#[tokio::test]
async fn oc03_not_accepted_online_moves_doc_to_rmr() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000003";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let r = complete(&pool, 0x01, OperatorResolution::NotAccepted)
        .await
        .expect("not-accepted completes");
    assert!(r.applied && !r.seed_advanced && r.server_fiscal_no.is_none());
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    assert!(
        read_seed(&pool, fscl).await.is_none(),
        "seed unchanged on not-accepted"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
    assert_eq!(read_mode(&pool, fscl).await, "ONLINE");
}

// ───────────────────────────── oc04 MAC reseed ───────────────────────────────

#[tokio::test]
async fn oc04_mac_reseed_installs_operator_seed_and_manual() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000004";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let r = complete(&pool, 0x01, OperatorResolution::MacReseed { seed: OPSEED })
        .await
        .expect("MAC reseed completes");
    assert!(r.applied && r.seed_advanced);
    assert_eq!(
        read_seed(&pool, fscl).await.as_deref(),
        Some(&OPSEED[..]),
        "operator seed installed"
    );
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
}

// ───────────────────── oc05 GOING_ONLINE with active drain ────────────────────

#[tokio::test]
async fn oc05_mode_going_online_when_offline_session_active() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000005";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    // An active OPEN offline session must finish draining → mode target GOING_ONLINE.
    sqlx::query(
        "INSERT INTO offline_sessions (offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-07-19T00:00:00Z')",
    )
    .bind(vec![0xC0u8; 16])
    .bind(fscl)
    .execute(&pool)
    .await
    .expect("seed active offline session");
    let r = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000555555".into(),
        },
    )
    .await
    .expect("completes with active drain");
    assert_eq!(r.mode_target, ModeTarget::GoingOnline);
    assert_eq!(read_mode(&pool, fscl).await, "GOING_ONLINE");
}

// ───────────────── oc06 offline not-accepted refused (cohort) ─────────────────

#[tokio::test]
async fn oc06_offline_not_accepted_refused_nothing_mutated() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000006";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", Some(500)).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let err = complete(&pool, 0x01, OperatorResolution::NotAccepted)
        .await
        .expect_err("offline not-accepted needs cohort cleanup (Slice 5b)");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::OfflineCohortCleanupRequired
    )));
    // Nothing mutated — still PENDING under STOP, doc still SENDING.
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
    assert_eq!(read_doc_state(&pool, 0x11).await, "SENDING");
}

// ───────────────────── oc07 shift-family refused ─────────────────────────────

#[tokio::test]
async fn oc07_shift_family_refused() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000007";
    let doc = seed_doc(&pool, fscl, 0x11, "SHIFT_OPEN", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let err = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000777777".into(),
        },
    )
    .await
    .expect_err("shift-family completion is Slice 5b");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::ShiftFamilyNotSupported
    )));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_sfn(&pool, 0x11).await.is_none());
}

// ───────────────────── oc08 stale authority refused ──────────────────────────

#[tokio::test]
async fn oc08_stale_generation_refused_nothing_mutated() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000008";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    // Simulate an intervening fence advance past this reservation's generation.
    sqlx::query(
        "UPDATE node_state SET delivery_generation = delivery_generation + 3 WHERE fiscal_number=?",
    )
    .bind(fscl)
    .execute(&pool)
    .await
    .unwrap();
    let err = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000888888".into(),
        },
    )
    .await
    .expect_err("stale generation refuses");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::StaleAuthority
    )));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_sfn(&pool, 0x11).await.is_none());
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
}

// ───────────────────── oc09 pointer lifecycle → next doc ──────────────────────

#[tokio::test]
async fn oc09_completion_clears_pointer_and_next_doc_authorizes() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000009";
    let doc_a = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    let doc_b = seed_doc(&pool, fscl, 0x22, "SELL", None).await;
    held_pending(&pool, 0x01, doc_a, fscl).await;
    complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000999999".into(),
        },
    )
    .await
    .expect("A completes");
    assert!(read_pointer(&pool, fscl).await.is_none());
    // The next legitimate document authorizes (fence released, pointer clear).
    let row = new_res(0x02, doc_b, fscl);
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("next document authorizes after operator completion");
}
