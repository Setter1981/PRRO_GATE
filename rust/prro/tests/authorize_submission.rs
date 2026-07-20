//! CS-3 Slice 3 — lifetime authorization + sole-caller wire gate.
//!
//! RED-first teeth for `delivery_reservation::authorize_submission` (design §2). The
//! authorization IS the wire gate: with no `Authorization` the caller performs zero wire I/O,
//! so "mock DPS sees at most one RPC" is proven here as "at most one authorization commits
//! CALL_STARTED per document lifetime". INACTIVE — no production caller is wired (Slice 7).
//!
//! Groups:
//! - `az01` success + the durable marker / generation / pointer are written;
//! - `az02` call-once: a 2nd authorize for the SAME document is refused by the explicit
//!   NOT-EXISTS query guard (`CallOnceAlreadyStarted`, not the trigger) — crash-after-marker;
//! - `az03` §3.1 fence: a 2nd document on an FN with an active reservation is refused;
//! - `az04` no-orphan-RN: after the first attempt is APPLIED the document still cannot
//!   re-authorize, but an unrelated next document can;
//! - `az05` generation advances monotonically across authorizations;
//! - `az06` a missing node_state row is refused and leaves NO reservation (rollback).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    authorize_submission, Authorization, AuthorizeError, NewReservation,
};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

const TS: &str = "2026-07-20T00:00:00Z";

async fn seed_node(pool: &SqlitePool, fscl: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CREATED', 1)",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed node_state");
}

async fn seed_doc(pool: &SqlitePool, fscl: &str, doc_byte: u8, lnd: i64) -> DocumentId {
    seed_node(pool, fscl).await;
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fscl)
    .bind(lnd)
    .bind(&sha)
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

/// Drive an already-authorized (CALL_STARTED) reservation to APPLIED so the §3.1 fence releases.
async fn drive_applied(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty='SUBMITTED', \
             response_provenance='PARSED_DPS_ENVELOPE', apply_state='PENDING_APPLY', \
             node_effect='NoNodeEffect' WHERE reservation_id=?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE delivery_reservation SET apply_state='APPLIED' WHERE reservation_id=?")
        .bind(&[res_byte; 16][..])
        .execute(pool)
        .await
        .unwrap();
}

async fn authorize(pool: &SqlitePool, row: NewReservation) -> anyhow::Result<Authorization> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

fn is_err(err: &anyhow::Error, pred: impl Fn(&AuthorizeError) -> bool) -> bool {
    err.downcast_ref::<AuthorizeError>().is_some_and(pred)
}

async fn count_reservations(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM delivery_reservation")
        .fetch_one(pool)
        .await
        .unwrap()
}

// ───────────────────────────── az01 — success ────────────────────────────────

#[tokio::test]
async fn az01_authorize_writes_marker_generation_and_pointer() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "1000000001";
    let doc = seed_doc(&pool, fscl, 0x11, 1).await;
    let auth = authorize(&pool, new_res(0x01, doc, fscl))
        .await
        .expect("fresh document authorizes");
    assert_eq!(
        auth.authorized_generation(),
        1,
        "first authorization consumes generation 1"
    );
    assert_eq!(auth.reservation_id(), [0x01; 16]);

    // reservation is CALL_STARTED with the marker + generation.
    let (state, cs_at, gen): (String, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT state, call_started_at, authorized_generation FROM delivery_reservation \
         WHERE reservation_id=?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "CALL_STARTED");
    assert_eq!(cs_at.as_deref(), Some(TS));
    assert_eq!(gen, Some(1));

    // node_state advanced its generation + points at the reservation.
    let (ns_gen, ptr): (i64, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT delivery_generation, active_delivery_reservation_id FROM node_state \
         WHERE fiscal_number=?",
    )
    .bind(fscl)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ns_gen, 1);
    assert_eq!(ptr.as_deref(), Some(&[0x01u8; 16][..]));
}

// ───────────────────── az02 — call-once (query guard) ─────────────────────────

#[tokio::test]
async fn az02_second_authorize_same_document_refused_by_query_guard() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "1000000002";
    let doc = seed_doc(&pool, fscl, 0x11, 1).await;
    authorize(&pool, new_res(0x01, doc, fscl))
        .await
        .expect("first authorizes");
    // A SECOND authorize for the SAME document (fresh reservation id) must be refused by the
    // explicit NOT-EXISTS query guard — CallOnceAlreadyStarted (proving it is the query, not
    // the no_replace trigger which would surface FenceOrCollision).
    let err = authorize(&pool, new_res(0x02, doc, fscl))
        .await
        .expect_err("a started document never re-authorizes (crash-after-marker safe)");
    assert!(
        is_err(&err, |e| matches!(
            e,
            AuthorizeError::CallOnceAlreadyStarted
        )),
        "expected CallOnceAlreadyStarted; got {err:?}"
    );
    // Exactly one reservation exists (the second never inserted → no orphan RN).
    assert_eq!(count_reservations(&pool).await, 1);
}

// ─────────────────────────── az03 — §3.1 fence ───────────────────────────────

#[tokio::test]
async fn az03_second_document_on_active_fn_refused() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "1000000003";
    let doc_a = seed_doc(&pool, fscl, 0x11, 1).await;
    let doc_b = seed_doc(&pool, fscl, 0x22, 2).await;
    authorize(&pool, new_res(0x01, doc_a, fscl))
        .await
        .expect("A authorizes");
    // B on the SAME FN while A is active (CALL_STARTED holds the §3.1 fence) is refused.
    let err = authorize(&pool, new_res(0x02, doc_b, fscl))
        .await
        .expect_err("the FN fence blocks a concurrent second document");
    assert!(
        is_err(&err, |e| matches!(e, AuthorizeError::FenceOrCollision(_))),
        "expected FenceOrCollision; got {err:?}"
    );
}

// ────────────────────── az04 — no orphan RN + release ─────────────────────────

#[tokio::test]
async fn az04_no_orphan_rn_after_started_history_but_next_doc_ok() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "1000000004";
    let doc_a = seed_doc(&pool, fscl, 0x11, 1).await;
    let doc_b = seed_doc(&pool, fscl, 0x22, 2).await;
    authorize(&pool, new_res(0x01, doc_a, fscl))
        .await
        .expect("A authorizes");
    drive_applied(&pool, 0x01).await; // A resolves; §3.1 fence releases.

    // The SAME document A can never re-authorize (historical call-once survives release).
    let err = authorize(&pool, new_res(0x03, doc_a, fscl))
        .await
        .expect_err("a document that ever started never re-authorizes");
    assert!(is_err(&err, |e| matches!(
        e,
        AuthorizeError::CallOnceAlreadyStarted
    )));

    // An unrelated next document on the FN authorizes cleanly (fence released, generation 2).
    let auth_b = authorize(&pool, new_res(0x02, doc_b, fscl))
        .await
        .expect("the next document reserves after release");
    assert_eq!(auth_b.authorized_generation(), 2);
}

// ───────────────────── az05 — monotone generation ────────────────────────────

#[tokio::test]
async fn az05_generation_advances_monotonically() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "1000000005";
    let doc_a = seed_doc(&pool, fscl, 0x11, 1).await;
    let doc_b = seed_doc(&pool, fscl, 0x22, 2).await;
    let doc_c = seed_doc(&pool, fscl, 0x33, 3).await;
    let a = authorize(&pool, new_res(0x01, doc_a, fscl)).await.unwrap();
    drive_applied(&pool, 0x01).await;
    let b = authorize(&pool, new_res(0x02, doc_b, fscl)).await.unwrap();
    drive_applied(&pool, 0x02).await;
    let c = authorize(&pool, new_res(0x03, doc_c, fscl)).await.unwrap();
    assert_eq!(
        (
            a.authorized_generation(),
            b.authorized_generation(),
            c.authorized_generation()
        ),
        (1, 2, 3)
    );
}

// ───────────────────── az06 — missing node_state → rollback ───────────────────

#[tokio::test]
async fn az06_missing_node_state_refused_and_rolls_back() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "1000000006";
    // Seed the FN config + a fiscal document but NO node_state row.
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(&pool)
    .await
    .unwrap();
    let doc_bytes = vec![0x11u8; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 1, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![0xEEu8; 16])
    .bind(fscl)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();
    let doc = DocumentId::from_bytes([0x11; 16]);

    let err = authorize(&pool, new_res(0x01, doc, fscl))
        .await
        .expect_err("no node_state row → refused");
    assert!(is_err(&err, |e| matches!(
        e,
        AuthorizeError::NodeStateMissing
    )));
    // The whole tx rolled back — the reservation the insert created does NOT persist.
    assert_eq!(
        count_reservations(&pool).await,
        0,
        "no orphan reservation after rollback"
    );
}
