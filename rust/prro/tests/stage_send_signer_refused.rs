//! W14a-2b Commit 5 integration tests — stage_send signer-cashier
//! enforcement (spec §2.3/§2.5).
//!
//! Pins the wiring of `signer_guard::enforce_signer_cashier_match`
//! into the 4-pre `with_immediate` tx + audit emit contract:
//! - `SIGNER_CASHIER_MISMATCH` Warning audit COMMITS on Ok-return.
//! - No state mutation on refusal (doc stays `Signed`).
//! - No wire send (`send_chk` not invoked).
//! - Bypass set (`ShiftClose | ZReport | ShiftOpen`) reaches happy path
//!   regardless of signer attribution.

use sqlx::SqlitePool;

use prro::db::models::ids::DocumentId;
use prro::services::write_path::signer_guard::SignerCashierMismatch;
use prro::services::write_path::stage_send::{self, StageSendOutcome};
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

/// Seed a SIGNED doc + shift; `cashier_signer` is what gets written to
/// `fiscal_documents.signed_by_cashier_id`, and `cashier_opener` to
/// `shifts.opened_by_cashier_id`.  When they differ → Mismatch.
/// When `cashier_signer = None` → SignerIdMissing.
/// When `seed_shift = false` → ShiftMissingForFiscalDoc.
async fn seed(
    pool: &SqlitePool,
    doc_byte: u8,
    doc_type: &str,
    cashier_signer: Option<&str>,
    cashier_opener: Option<&str>,
    seed_shift: bool,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();

    let shift_id_bytes_opt: Option<Vec<u8>> = if seed_shift {
        let shift_bytes = vec![doc_byte ^ 0x80; 16];
        sqlx::query(
            "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
                cash_balance_kop, opened_by_cashier_id) \
             VALUES (?, '1234567890', 1, 'OPENED', 'ONLINE', 0, ?)",
        )
        .bind(&shift_bytes)
        .bind(cashier_opener.unwrap_or("default-opener"))
        .execute(pool)
        .await
        .expect("seed shift");
        Some(shift_bytes)
    } else {
        None
    };

    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', ?, ?, ?, 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-19T12:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(shift_id_bytes_opt.as_deref())
    .bind(doc_byte as i64)
    .bind(doc_type)
    .bind(&sha)
    .bind(cashier_signer)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"FAKE-CMS".to_vec())
    .execute(pool)
    .await
    .expect("seed SIGNED_XML");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn audit_count(pool: &SqlitePool, doc: DocumentId, event_type: &str) -> i64 {
    let id = format!("{doc:?}");
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? AND event_type = ?",
    )
    .bind(&id)
    .bind(event_type)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ─── Mismatch arm — happy refusal path ───────────────────────────────

#[tokio::test]
async fn stage_send_signer_mismatch_returns_signer_refused_with_audit() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed(
        &pool,
        0x11,
        "SELL",
        Some("cashier-petya"),  // signer
        Some("cashier-vasya"),  // opener
        true,
    )
    .await;
    let stub = StubDpsChannel::new(Ok(ack("UNUSED-NO-WIRE")));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("signer refusal must surface as Ok variant");

    match outcome {
        StageSendOutcome::SignerRefused(SignerCashierMismatch::Mismatch {
            expected_cashier_id,
            attempted_signer_id,
            ..
        }) => {
            assert_eq!(expected_cashier_id.as_str(), "cashier-vasya");
            assert_eq!(attempted_signer_id.as_str(), "cashier-petya");
        }
        other => panic!("expected SignerRefused(Mismatch), got {other:?}"),
    }

    // Audit: SIGNER_CASHIER_MISMATCH committed inside the 4-pre tx.
    assert_eq!(
        audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await,
        1,
        "SIGNER_CASHIER_MISMATCH audit MUST be committed via Ok-return",
    );

    // NIT-C5-3: SELECT the audit row payload + actor and verify spec
    // §2.5 contract — variant tag + attempted/expected cashier ids +
    // refused_at_stage + actor = attempted_signer_id.
    let entity_id = format!("{doc:?}");
    let (payload_json, actor): (String, Option<String>) = sqlx::query_as(
        "SELECT event_payload_json, actor FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
           AND event_type = 'SIGNER_CASHIER_MISMATCH' LIMIT 1",
    )
    .bind(&entity_id)
    .fetch_one(&pool)
    .await
    .expect("read SIGNER_CASHIER_MISMATCH audit row");
    assert_eq!(
        actor.as_deref(),
        Some("cashier-petya"),
        "actor MUST be attempted_signer_id per spec §2.5",
    );
    let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap();
    assert_eq!(payload["variant"], "Mismatch");
    assert_eq!(payload["refused_at_stage"], "stage_send_pre");
    assert_eq!(payload["expected_cashier_id"], "cashier-vasya");
    assert_eq!(payload["attempted_signer_id"], "cashier-petya");
    assert_eq!(payload["doc_type"], "SELL");
    assert!(payload["shift_id"].is_string());

    // No state mutation: doc stays SIGNED.
    assert_eq!(read_state(&pool, doc).await, "SIGNED");

    // No wire send.
    assert_eq!(stub.call_count(), 0, "send_chk MUST NOT fire on refusal");

    // No transport_trace row (no intent marked).
    let trace_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM transport_trace WHERE document_id = ?",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(trace_count, 0, "no transport_trace row on signer refusal");

    // No STAGE_SEND_INTENT_MARKED (we refused BEFORE marker).
    assert_eq!(
        audit_count(&pool, doc, "STAGE_SEND_INTENT_MARKED").await,
        0
    );
}

// ─── SignerIdMissing arm ─────────────────────────────────────────────

#[tokio::test]
async fn stage_send_signer_id_missing_returns_signer_refused() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed(&pool, 0x22, "SELL", None, Some("cashier-vasya"), true).await;
    let stub = StubDpsChannel::new(Ok(ack("UNUSED")));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert!(matches!(
        outcome,
        StageSendOutcome::SignerRefused(SignerCashierMismatch::SignerIdMissing { .. })
    ));
    assert_eq!(audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await, 1);
    assert_eq!(stub.call_count(), 0);

    // LOW-C5-2: structural-variant audit row pin per spec §2.5 —
    // actor IS NULL (no operator identity for caller-bug class) +
    // variant tag + refused_at_stage.
    let entity_id = format!("{doc:?}");
    let (payload_json, actor): (String, Option<String>) = sqlx::query_as(
        "SELECT event_payload_json, actor FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
           AND event_type = 'SIGNER_CASHIER_MISMATCH' LIMIT 1",
    )
    .bind(&entity_id)
    .fetch_one(&pool)
    .await
    .expect("read structural-bug audit row");
    assert!(
        actor.is_none(),
        "actor MUST be NULL for structural-bug variants (no operator identity); got {actor:?}",
    );
    let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap();
    assert_eq!(payload["variant"], "SignerIdMissing");
    assert_eq!(payload["refused_at_stage"], "stage_send_pre");
    assert_eq!(payload["doc_type"], "SELL");
}

// ─── MED-C5-1 regression — source-state allowlist BEFORE signer ──────

/// MED-C5-1: stale terminal docs (SENT / SENDING / KVT1 / etc.) with
/// broken shift/signer attribution must surface as `StateConflict`,
/// NOT `SignerRefused`.  Pre-fix ordering ran signer_guard before the
/// source-state allowlist → forensic noise on stale rows.
#[tokio::test]
async fn stage_send_sent_state_with_null_shift_returns_state_conflict_not_signer_refused() {
    let (_d, pool) = fresh_pool().await;
    // Seed: SELL doc directly in SENT state (terminal-ish — not in
    // {Signed, ErrorRetryable, OfflineLocalAck} allowlist), with NO
    // shift binding and NO signer attribution.  Pre-MED-C5-1 fix
    // would have surfaced ShiftMissingForFiscalDoc + emitted
    // SIGNER_CASHIER_MISMATCH audit.
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let doc_bytes = vec![0x77u8; 16];
    let req_bytes = vec![0x88u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'SENT', 'b1', 't1', 'ONLINE', \
            '2026-05-20T12:00:00Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();
    let doc = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let stub = StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-FIRE")),
        Box::new(|| panic!("stale-state doc MUST NOT reach wire send")),
    );

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    match outcome {
        StageSendOutcome::StateConflict { observed } => {
            assert_eq!(observed, prro::db::models::enums::DocState::Sent);
        }
        other => panic!("expected StateConflict(Sent), got {other:?}"),
    }
    // CRITICAL: NO SIGNER_CASHIER_MISMATCH audit — source-state
    // allowlist filtered before signer_guard ran.
    assert_eq!(
        audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await,
        0,
        "MED-C5-1 invariant: stale terminal docs MUST NOT trigger signer_guard"
    );
    // No transport_trace row (no intent marked).
    let trace_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM transport_trace WHERE document_id = ?",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(trace_count, 0);
    // No STAGE_SEND_INTENT_MARKED.
    assert_eq!(
        audit_count(&pool, doc, "STAGE_SEND_INTENT_MARKED").await,
        0
    );
}

#[tokio::test]
async fn stage_send_kvt1_state_with_null_shift_returns_state_conflict_not_signer_refused() {
    // Symmetric to the SENT test: KVT1 (different terminal-ish state)
    // also lands StateConflict regardless of shift/signer state.
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let doc_bytes = vec![0x88u8; 16];
    let req_bytes = vec![0x77u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', 2, 'SELL', 'KVT1', 'b1', 't1', 'ONLINE', \
            '2026-05-20T12:00:00Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();
    let doc = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let stub = StubDpsChannel::new(Ok(ack("UNUSED")));
    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert!(
        matches!(outcome, StageSendOutcome::StateConflict { .. }),
        "got {outcome:?}"
    );
    assert_eq!(audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await, 0);
    assert_eq!(stub.call_count(), 0);
}

// ─── ShiftMissingForFiscalDoc arm ────────────────────────────────────

#[tokio::test]
async fn stage_send_shift_missing_returns_signer_refused() {
    let (_d, pool) = fresh_pool().await;
    // seed_shift = false → fiscal_documents.shift_id stays NULL.
    let doc = seed(&pool, 0x33, "SELL", Some("cashier-vasya"), None, false).await;
    let stub = StubDpsChannel::new(Ok(ack("UNUSED")));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert!(matches!(
        outcome,
        StageSendOutcome::SignerRefused(
            SignerCashierMismatch::ShiftMissingForFiscalDoc { .. }
        )
    ));
    assert_eq!(audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await, 1);
    assert_eq!(stub.call_count(), 0);
}

// ─── Bypass arm — signer enforcement skipped for ShiftClose ──────────

#[tokio::test]
async fn stage_send_shift_close_bypasses_signer_check_even_on_mismatch() {
    let (_d, pool) = fresh_pool().await;
    // Mismatched signer / opener on SHIFT_CLOSE — bypass set must allow.
    let doc = seed(
        &pool,
        0x44,
        "SHIFT_CLOSE",
        Some("senior-vera"),
        Some("cashier-vasya"),
        false, // ShiftClose bypass — shift not needed for helper
    )
    .await;
    let stub = StubDpsChannel::new(Ok(ack("DPS-FN-44")));

    let outcome = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect("ShiftClose bypass path must reach wire send");
    assert!(
        matches!(outcome, StageSendOutcome::Sent { .. }),
        "got {outcome:?}"
    );

    // NO SIGNER_CASHIER_MISMATCH audit — helper bypassed.
    assert_eq!(audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await, 0);
    // STAGE_SEND_INTENT_MARKED emitted (happy path).
    assert!(audit_count(&pool, doc, "STAGE_SEND_INTENT_MARKED").await >= 1);
    assert_eq!(stub.call_count(), 1);
}

#[tokio::test]
async fn stage_send_z_report_bypasses_signer_check() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed(&pool, 0x55, "Z_REPORT", None, None, false).await;
    let stub = StubDpsChannel::new(Ok(ack("DPS-FN-55")));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert!(matches!(outcome, StageSendOutcome::Sent { .. }));
    assert_eq!(audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await, 0);
}

// ─── Happy path — signer == opener proceeds to wire send ─────────────

#[tokio::test]
async fn stage_send_signer_match_proceeds_to_wire_send() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed(
        &pool,
        0x66,
        "SELL",
        Some("cashier-vasya"),
        Some("cashier-vasya"),
        true,
    )
    .await;
    let stub = StubDpsChannel::new(Ok(ack("DPS-FN-66")));

    let outcome = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert!(
        matches!(outcome, StageSendOutcome::Sent { .. }),
        "happy path expected, got {outcome:?}"
    );
    assert_eq!(audit_count(&pool, doc, "SIGNER_CASHIER_MISMATCH").await, 0);
    assert_eq!(stub.call_count(), 1);
}
