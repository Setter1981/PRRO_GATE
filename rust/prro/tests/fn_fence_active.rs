//! CS-3 S7-2 — the FN active-reservation fence (`fn_fence_active_tx`).
//!
//! Teeth for the tx-bound fence that foreign FN-chain / offline writers call inside
//! their own `BEGIN IMMEDIATE` to refuse-if-active:
//! - `fence_predicate_byte_identity` — the shared predicate ≡ migration 035 index + trigger.
//! - `fence_active_tx_detects_active_reservation` — behavioural: the tx-bound helper reports
//!   an in-flight reservation and stays open otherwise (revert-canary on the predicate).
//! - `fence_wired_into_writers_static_pin` — each fenced foreign writer actually calls the
//!   helper before its mutation (revert-canary on the wiring).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    self, NewReservation, ACTIVE_FENCE_STATE_PREDICATE,
};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

/// Collapse every run of ASCII whitespace to a single space so a comparison is
/// insensitive to SQL-vs-Rust line-continuation / indentation formatting.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The fence predicate must be byte-identical (whitespace-normalised) to BOTH the
/// `ux_reservation_active` partial index and the `delivery_reservation_no_replace`
/// trigger clause in migration 035. The shared `ACTIVE_FENCE_STATE_PREDICATE` const
/// already gives `fn_fence_active_tx` ≡ `get_active_for_fn` at compile time; this pins
/// the Rust const against the migration SQL so any drift on either side fails CI.
///
/// Revert-canary (manual): change one character of `ACTIVE_FENCE_STATE_PREDICATE` and
/// this test goes RED — proving the pin bites, not just decorates.
#[test]
fn fence_predicate_byte_identity() {
    let pred = norm(ACTIVE_FENCE_STATE_PREDICATE);
    let sql = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/035_delivery_reservation_call_once_and_fence.sql"
    ))
    .expect("migration 035 must be readable");
    let sql_n = norm(&sql);

    let hits = sql_n.matches(&pred).count();
    assert!(
        hits >= 2,
        "fence predicate must appear byte-identical (whitespace-normalised) in BOTH the \
         ux_reservation_active index AND the delivery_reservation_no_replace trigger of \
         migration 035 — found {hits} occurrence(s).\n  predicate = {pred:?}"
    );
}

const FN_A: &str = "1234567890";

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

async fn seed_fn(pool: &SqlitePool, fscl: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
}

async fn seed_doc(pool: &SqlitePool, fscl: &str, doc_byte: u8, lnd: i64) -> DocumentId {
    seed_fn(pool, fscl).await;
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

async fn insert_res(pool: &SqlitePool, row: NewReservation) -> anyhow::Result<i64> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::insert(tx, row)
                .await
                .map_err(Into::into)
        })
    })
    .await
}

async fn mark_call_started(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', call_started_at = '2026-07-17T00:00:00Z', \
             authorized_generation = 1 \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("mark CALL_STARTED");
}

/// Run the tx-bound fence exactly as a foreign writer would — inside a `BEGIN IMMEDIATE`.
async fn fence(pool: &SqlitePool, fscl: &str) -> bool {
    let f = fscl.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move { Ok(delivery_reservation::fn_fence_active_tx(tx, &f).await?) })
    })
    .await
    .expect("fence query")
}

/// Behavioural: the tx-bound fence reports an in-flight reservation (RESERVED_NOT_STARTED
/// and CALL_STARTED are both active) and stays OPEN for an FN with no reservation. This is
/// the load-bearing proof that a wired writer's refuse-if-active guard actually fires.
#[tokio::test]
async fn fence_active_tx_detects_active_reservation() {
    let (_dir, pool) = fresh_pool().await;

    // No reservation → fence OPEN.
    assert!(!fence(&pool, FN_A).await, "no reservation ⇒ fence open");

    // A RESERVED_NOT_STARTED reservation → active (fence CLOSED).
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x22, doc, FN_A)).await.unwrap();
    assert!(
        fence(&pool, FN_A).await,
        "RESERVED_NOT_STARTED ⇒ fence closed"
    );

    // Drive it to CALL_STARTED → still active.
    mark_call_started(&pool, 0x22).await;
    assert!(fence(&pool, FN_A).await, "CALL_STARTED ⇒ fence closed");

    // A DIFFERENT FN with no reservation is unaffected (per-FN fence).
    assert!(!fence(&pool, "9999999999").await, "other FN ⇒ fence open");
}

/// Static revert-canary: each foreign FN-chain / mint writer in the S7-2 CLOSED inventory
/// must call `fn_fence_active_tx` before its mutation. Deleting a wiring makes this RED.
///
/// The closed inventory is 5 fenced writers. THREE §6 candidates are deliberately EXCLUDED
/// (each a grounded implementation finding, flagged to the architect):
/// - `stage_send.rs:1809` — cutover-doomed: the legacy 4-b block is DELETED at S7-1 cutover,
///   so fencing it is throwaway work.
/// - `boot_phase.rs:1814` (NC-03 seed repair) — the §7.1 boot-first reservation pass leaves a
///   PENDING reservation live during boot; a blanket fence would REFUSE legitimate NC-03
///   recovery. Boot-time chain-fork safety is owned by the boot-pass ordering, not this fence.
/// - `stage_acquire.rs:714` — REDUNDANT: the existing online write-gate
///   `exists_blocking_non_issued_sibling_tx` already refuses minting a fresh doc while any
///   non-issued sibling rests on the FN, and a doc under an active reservation is ALWAYS in
///   `SENDING` (non-issued) for the whole active window (authorize_submission does
///   Signed→Sending in the reservation's tx; apply_outcome exits Sending only at APPLIED,
///   which releases the fence). The reservation-active window ⊂ "non-issued sibling present".
///   Offline mints are unconditionally ungated by design (offline availability is mandatory).
#[test]
fn fence_wired_into_writers_static_pin() {
    for (path, label) in [
        (
            "src/services/offline_sync/offline_code_replenish.rs",
            "offline_code_replenish (seed install, no equality gate)",
        ),
        (
            "src/services/offline_sync/backlog_drain.rs",
            "backlog_drain (direct END mint, bypasses stage_acquire)",
        ),
        (
            "src/services/write_path/stage_offline_ack.rs",
            "stage_offline_ack (offline-ack CAS + chain-seed advance)",
        ),
        (
            "src/services/write_path/stage_sign.rs",
            "stage_sign (offline-code consume → offline issuance)",
        ),
        (
            "src/services/offline_session.rs",
            "offline_session (open_session)",
        ),
    ] {
        let src = std::fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("read {path}: {e}"));
        assert!(
            src.contains("fn_fence_active_tx"),
            "S7-2: {label} must call fn_fence_active_tx before its mutation (fail-closed fence) — \
             wiring missing in {path}"
        );
    }
}
