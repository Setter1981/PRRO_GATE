//! A′.3 PR-O3 STOP-O3-1 fix (v7) — wire-correctness fail-closed in stage_send.
//!
//! An OFFLINE-acquired doc (`fs_mode='OFFLINE'`) that is NOT yet offline-acked
//! (`offline_fiscal_no IS NULL`, state SIGNED) must NEVER be sent to DPS
//! online.  The GO_ONLINE mode-race: the doc committed its acquire edge + was
//! signed under `mode = Offline`, then the node mode flipped to `Online` before
//! `dispatch_post_sign` (which routes by NODE mode, not doc `fs_mode`) →
//! stage_send would otherwise wire it with an empty `id_offline`, wrongly
//! issuing to DPS a receipt that was never offline-numbered.
//!
//! Fix (v7): refuse BEFORE any CAS / `Sending` marker / wire → the doc stays
//! `SIGNED` (alive for the offline lane / the fix-(b) / boot-SW-2 recovery),
//! and the DPS channel is never invoked.

use sqlx::SqlitePool;

use prro::db::models::ids::DocumentId;
use prro::services::write_path::stage_send::{self, StageSendError};
use prro::transports::dps::dto::CheckAck;

mod common;
use common::StubDpsChannel;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.to_string(),
        id_sign: vec![],
        data_sign: vec![],
    }
}

/// Seed a SIGNED doc with an explicit `fs_mode` and NULL `offline_fiscal_no`,
/// plus a node in `ONLINE` mode (the mode-race precondition) + its SIGNED_XML.
async fn seed_signed(pool: &SqlitePool, doc_byte: u8, fs_mode: &str) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
    let shift_bytes = vec![doc_byte ^ 0x80; 16];
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, '1234567890', 1, 'OPENED', 'ONLINE', 0, 'csh')",
    )
    .bind(&shift_bytes)
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let unsigned = vec![doc_byte; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', ?, ?, 'SELL', 'SIGNED', 'b1', 't1', ?, \
            '2026-07-07T12:00:00Z', '{}', ?, ?, 'csh')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&shift_bytes)
    .bind(doc_byte as i64)
    .bind(fs_mode)
    .bind([0u8; 32].as_slice())
    .bind(&unsigned)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state \
            (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
             backend_profile_id, transport_profile_id, last_known_unsigned_xml_sha256) \
         VALUES ('1234567890', 'ONLINE', 'OPENED', ?, 1, 'b1', 't1', NULL)",
    )
    .bind(&shift_bytes)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"FAKE-CMS".to_vec())
    .execute(pool)
    .await
    .unwrap();
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn trace_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// (v7): an OFFLINE-acquired, not-yet-acked SIGNED doc routed to stage_send
/// (the GO_ONLINE race) is REFUSED fail-closed — no wire, no Sending marker,
/// no trace; the doc stays SIGNED for the offline lane / orphan recovery.
#[tokio::test]
async fn offline_acquired_signed_doc_is_refused_before_wire() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed(&pool, 0x21, "OFFLINE").await;
    // The stub would PANIC-loudly if wired (queue is a single ack we expect
    // NEVER to be consumed); the fail-closed refusal must precede any send.
    let stub = StubDpsChannel::new(Ok(ack("MUST-NOT-BE-SENT")));

    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("(v7) must refuse the online send of an offline-acquired doc");
    assert!(
        matches!(err, StageSendError::OfflineDocRoutedOnline { document_id } if document_id == doc),
        "expected OfflineDocRoutedOnline, got {err:?}"
    );

    // No wire, no CAS: the doc stays SIGNED and no transport_trace was opened
    // (the trace + Sending marker are written together, AFTER the guard).
    assert_eq!(
        read_state(&pool, doc).await,
        "SIGNED",
        "the doc is left untouched for the offline lane"
    );
    assert_eq!(trace_count(&pool).await, 0, "no send attempt was traced");
}

/// Control: an ONLINE-origin SIGNED doc is NOT caught by the (v7) guard — it
/// proceeds past the fs_mode check into the normal send ladder (here it reaches
/// the wire and the stub ack advances it, proving (v7) is scoped to fs_mode
/// OFFLINE only, not a blanket SIGNED refusal).
#[tokio::test]
async fn online_signed_doc_is_not_refused_by_v7() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_signed(&pool, 0x22, "ONLINE").await;
    let stub = StubDpsChannel::new(Ok(ack("DPS-ONLINE-OK")));

    let outcome = stage_send::run(&pool, &stub, doc, None).await;
    // Whatever the downstream outcome, it is NOT the (v7) refusal — the guard
    // let the online doc through.
    assert!(
        !matches!(outcome, Err(StageSendError::OfflineDocRoutedOnline { .. })),
        "(v7) must NOT fire for an fs_mode=ONLINE doc, got {outcome:?}"
    );
    assert_ne!(
        read_state(&pool, doc).await,
        "SIGNED",
        "the online doc advanced past SIGNED (the v7 guard did not block it)"
    );
}
