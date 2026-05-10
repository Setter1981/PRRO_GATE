//! W10.4 step 2c — orchestrator-direct integration fixtures
//! (R-W10.4-step2c-review MED 1 close).
//!
//! End-to-end coverage for `mac_recovery::run_mac_recovery` BEFORE
//! `stage_send::run` integration (W10.4 step 2d) lands.  Pinned
//! contracts:
//!
//!   1. **Happy `Resigned`**: extractable hash → atomic 4-write
//!      (previous_hash + sha256 + PAYLOAD_XML + SIGNED_XML) +
//!      `MAC_RECOVERY_RESIGNED` audit row with full forensic payload.
//!   2. **`HashNotExtractable`**: malformed message → no state change,
//!      counter stays 0, `MAC_RECOVERY_HASH_NOT_EXTRACTABLE` audit
//!      row emitted.
//!   3. **`CounterExhausted`** (already-burnt counter): pre-set
//!      counter = 1 → CAS fails → orchestrator returns
//!      `CounterExhausted` with no new audit row, no state change.
//!   4. **Pre-PERSIST assertion rollback**: PAYLOAD_XML missing → typed
//!      `MacRecoveryArtifactMissing` error → counter was claimed
//!      (committed in MR-CLAIM tx) but artifacts NOT replaced
//!      (MR-PERSIST rolled back) — the W6-stage-3-invariant breach
//!      is forensically visible.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use prro::crypto::errors::CryptoError;
use prro::crypto::provider::{
    CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes,
};
use prro::crypto::session::SigningSession;
use prro::db::models::ids::DocumentId;
use prro::db::repositories::document_files::DocumentFileKind;
use prro::services::write_path::error_routing::MacRecoveryHint;
use prro::services::write_path::mac_recovery::{self, MacRecoveryOutcome};
use prro::services::write_path::stage_send::StageSendError;
use prro::services::write_path::stage_sign::SigningContext;
use sqlx::SqlitePool;

// ─── Stub crypto: deterministic CMS bytes ─────────────────────────────

struct DetCrypto {
    response_queue: Mutex<VecDeque<Vec<u8>>>,
}

impl DetCrypto {
    fn new(responses: Vec<Vec<u8>>) -> Arc<Self> {
        Arc::new(Self {
            response_queue: Mutex::new(responses.into()),
        })
    }
}

#[async_trait]
impl CryptoProvider for DetCrypto {
    async fn sign_cms_detached(
        &self,
        _request: SignCmsRequest<'_>,
    ) -> Result<SignedCmsBytes, CryptoError> {
        let resp = self
            .response_queue
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| b"DEFAULT-CMS".to_vec());
        Ok(SignedCmsBytes(resp))
    }

    async fn verify_dstu(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> Result<DstuVerifyResult, CryptoError> {
        unimplemented!("not exercised");
    }

    async fn unwrap_envelope(
        &self,
        _: &[u8],
        _: &[u8],
        _: &SigningSession,
    ) -> Result<Vec<u8>, CryptoError> {
        unimplemented!("not exercised");
    }

    async fn fetch_cert_by_ski(
        &self,
        _: &[String],
        _: &[u8; 32],
        _: std::time::Duration,
    ) -> Result<CertDer, CryptoError> {
        unimplemented!("not exercised");
    }
}

fn ctx(crypto: Arc<DetCrypto>) -> SigningContext {
    SigningContext {
        provider: crypto as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test("operator-1".into(), [0u8; 32], vec![]),
        profile: prro_crypto::cms::profile::CmsProfile::Dstu4145WithGost34311Pb,
    }
}

// ─── Pool / seed helpers ─────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    (dir, pool)
}

const SELL_PAYLOAD_JSON: &str = r#"{
    "items": [
        {"code": "p-1", "name": "Item A", "price_kop": 1500, "quantity_thousandths": 1000, "sum_kop": 1500}
    ],
    "payments": [
        {"name": "Cash", "sum_kop": 1500, "type_code": "0"}
    ]
}"#;

/// Seed FN config + a doc in `ERROR_RETRYABLE` state with
/// `mac_recovery_attempts = 0` + PAYLOAD_XML / SIGNED_XML rows
/// pre-INSERTed (mirrors the post-attempt-#1 4-b commit shape per
/// freeze §4.4.1).  `total_sum_kop` matches the SELL payload above.
async fn seed_recoverable_doc(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    let prev_hash = [0xAAu8; 32]; // pre-recovery previous_hash
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, total_sum_kop, previous_hash) \
         VALUES (?, ?, '1234567890', ?, 'SELL', 'ERROR_RETRYABLE', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', ?, ?, 1500, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(SELL_PAYLOAD_JSON)
    .bind(&sha)
    .bind(&prev_hash[..])
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'PAYLOAD_XML', ?), (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"OLD-PAYLOAD-XML".to_vec())
    .bind(&doc_bytes)
    .bind(b"OLD-SIGNED-CMS".to_vec())
    .execute(pool)
    .await
    .expect("seed document_files (PAYLOAD_XML + SIGNED_XML)");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_doc_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_counter(pool: &SqlitePool, doc: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT mac_recovery_attempts FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_previous_hash(pool: &SqlitePool, doc: DocumentId) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT previous_hash FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_audit_events(pool: &SqlitePool, doc: DocumentId) -> Vec<(String, Option<String>)> {
    let entity_id = format!("{doc:?}");
    sqlx::query_as(
        "SELECT event_type, event_payload_json FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id",
    )
    .bind(&entity_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn read_artifact(
    pool: &SqlitePool,
    doc: DocumentId,
    kind: DocumentFileKind,
) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT content FROM document_files WHERE document_id = ? AND kind = ?")
        .bind(doc)
        .bind(kind)
        .fetch_optional(pool)
        .await
        .unwrap()
}

const RECOVERED_HASH_HEX: &str = "deadbeef0123456789abcdef0123456789abcdef0123456789abcdef01234567";

fn hint_with_extractable_hash() -> MacRecoveryHint {
    MacRecoveryHint {
        raw_error_message: format!("ERROR_BAD_HASH_PREV: store {RECOVERED_HASH_HEX} server-side"),
    }
}

// ─── Fixture 1 — happy Resigned ──────────────────────────────────────

#[tokio::test]
async fn run_mac_recovery_happy_resigned_persists_atomic_four_write() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_recoverable_doc(&pool, 0xA1).await;
    let crypto = DetCrypto::new(vec![b"NEW-RESIGNED-CMS".to_vec()]);
    let ctx = ctx(Arc::clone(&crypto));

    let outcome = mac_recovery::run_mac_recovery(&pool, &ctx, doc, &hint_with_extractable_hash())
        .await
        .expect("happy recovery must not surface a typed error");
    assert_eq!(outcome, MacRecoveryOutcome::Resigned);

    // Counter bumped 0 → 1.
    assert_eq!(
        read_counter(&pool, doc).await,
        1,
        "MR-CLAIM must burn budget"
    );

    // Doc state unchanged (still ERROR_RETRYABLE; caller's job to
    // CAS to Sending in the re-enter loop).
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");

    // previous_hash replaced with the recovered value.
    let new_hash = read_previous_hash(&pool, doc)
        .await
        .expect("previous_hash NOT NULL");
    let mut expected = [0u8; 32];
    for (i, pair) in RECOVERED_HASH_HEX.as_bytes().chunks_exact(2).enumerate() {
        let hi = (pair[0] as char).to_digit(16).unwrap() as u8;
        let lo = (pair[1] as char).to_digit(16).unwrap() as u8;
        expected[i] = (hi << 4) | lo;
    }
    assert_eq!(
        new_hash.as_slice(),
        &expected[..],
        "previous_hash must be the recovered hash"
    );

    // PAYLOAD_XML + SIGNED_XML replaced.
    let new_payload = read_artifact(&pool, doc, DocumentFileKind::PayloadXml)
        .await
        .expect("PAYLOAD_XML row");
    assert_ne!(
        new_payload, b"OLD-PAYLOAD-XML",
        "PAYLOAD_XML must be replaced with the rebuilt canonical XML"
    );
    let new_signed = read_artifact(&pool, doc, DocumentFileKind::SignedXml)
        .await
        .expect("SIGNED_XML row");
    assert_eq!(
        new_signed, b"NEW-RESIGNED-CMS",
        "SIGNED_XML must be the new CMS bytes from the crypto provider"
    );

    // Audit row: MAC_RECOVERY_RESIGNED with full forensic payload.
    let events = read_audit_events(&pool, doc).await;
    assert_eq!(events.len(), 1, "exactly one audit row from recovery");
    assert_eq!(events[0].0, "MAC_RECOVERY_RESIGNED");
    let payload = events[0]
        .1
        .as_deref()
        .expect("MAC_RECOVERY_RESIGNED must carry payload");
    // Old hash hex (0xAA × 32) — pinned forensic.
    let old_hex = "aa".repeat(32);
    assert!(
        payload.contains(&format!("\"old_previous_hash_hex\":\"{old_hex}\"")),
        "payload must carry old_previous_hash_hex: {payload}"
    );
    assert!(
        payload.contains(&format!(
            "\"new_previous_hash_hex\":\"{RECOVERED_HASH_HEX}\""
        )),
        "payload must carry new_previous_hash_hex: {payload}"
    );
    assert!(
        payload.contains("\"new_unsigned_xml_sha256_hex\":"),
        "payload must carry new_unsigned_xml_sha256_hex: {payload}"
    );
}

// ─── Fixture 2 — HashNotExtractable ──────────────────────────────────

#[tokio::test]
async fn run_mac_recovery_hash_not_extractable_emits_audit_no_state_change() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_recoverable_doc(&pool, 0xA2).await;
    let crypto = DetCrypto::new(vec![]); // no responses — provider must NOT be called
    let ctx = ctx(Arc::clone(&crypto));

    let bad_hint = MacRecoveryHint {
        raw_error_message: "ERROR_BAD_HASH_PREV: malformed message no hash here".into(),
    };
    let outcome = mac_recovery::run_mac_recovery(&pool, &ctx, doc, &bad_hint)
        .await
        .expect("malformed hint surfaces as outcome, not error");
    assert_eq!(outcome, MacRecoveryOutcome::HashNotExtractable);

    // Counter NOT claimed (preserves budget for the case where DPS
    // ships a bad message format transiently).
    assert_eq!(
        read_counter(&pool, doc).await,
        0,
        "HashNotExtractable must NOT burn the counter"
    );

    // State unchanged.
    assert_eq!(read_doc_state(&pool, doc).await, "ERROR_RETRYABLE");

    // Artifacts unchanged.
    let payload = read_artifact(&pool, doc, DocumentFileKind::PayloadXml).await;
    assert_eq!(payload.as_deref(), Some(b"OLD-PAYLOAD-XML".as_slice()));

    // previous_hash unchanged (still 0xAA × 32).
    let prev = read_previous_hash(&pool, doc).await.unwrap();
    assert_eq!(prev, vec![0xAAu8; 32]);

    // Audit: exactly one MAC_RECOVERY_HASH_NOT_EXTRACTABLE row.
    let events = read_audit_events(&pool, doc).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "MAC_RECOVERY_HASH_NOT_EXTRACTABLE");
    let pl = events[0].1.as_deref().unwrap();
    assert!(
        pl.contains("raw_error_message_truncated"),
        "audit payload must carry truncated message: {pl}"
    );
}

// ─── Fixture 3 — CounterExhausted ────────────────────────────────────

#[tokio::test]
async fn run_mac_recovery_counter_already_burnt_returns_counter_exhausted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_recoverable_doc(&pool, 0xA3).await;
    // Pre-burn the counter (simulates: prior worker tick claimed,
    // then crashed mid-PERSIST).
    sqlx::query("UPDATE fiscal_documents SET mac_recovery_attempts = 1 WHERE document_id = ?")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();

    let crypto = DetCrypto::new(vec![]); // provider must NOT be called
    let ctx = ctx(Arc::clone(&crypto));

    let outcome = mac_recovery::run_mac_recovery(&pool, &ctx, doc, &hint_with_extractable_hash())
        .await
        .expect("CounterExhausted is an outcome, not an error");
    assert_eq!(outcome, MacRecoveryOutcome::CounterExhausted);

    // Counter unchanged at 1.
    assert_eq!(read_counter(&pool, doc).await, 1);

    // Artifacts unchanged.
    let payload = read_artifact(&pool, doc, DocumentFileKind::PayloadXml).await;
    assert_eq!(payload.as_deref(), Some(b"OLD-PAYLOAD-XML".as_slice()));

    // No audit row (caller emits MAC_RECOVERY_FAILED_REPEAT_HASH_MISMATCH
    // when stage_send::run dispatches on this outcome).
    let events = read_audit_events(&pool, doc).await;
    assert_eq!(
        events.len(),
        0,
        "orchestrator must NOT emit on CounterExhausted (caller's job)"
    );
}

// ─── Fixture 4 — Pre-PERSIST assertion rolls back typed error ────────

#[tokio::test]
async fn run_mac_recovery_pre_persist_assertion_rolls_back_typed_error() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_recoverable_doc(&pool, 0xA4).await;
    // DELETE PAYLOAD_XML to simulate a W6 stage-3 invariant breach
    // (impossible in normal operation, but the orchestrator MUST
    // surface it typed rather than silently INSERT via replace_tx).
    sqlx::query("DELETE FROM document_files WHERE document_id = ? AND kind = 'PAYLOAD_XML'")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();

    let crypto = DetCrypto::new(vec![b"NEW-CMS".to_vec()]);
    let ctx = ctx(Arc::clone(&crypto));

    let res = mac_recovery::run_mac_recovery(&pool, &ctx, doc, &hint_with_extractable_hash()).await;
    match res {
        Err(StageSendError::MacRecoveryArtifactMissing { document_id, kind }) => {
            assert_eq!(document_id, doc);
            assert_eq!(kind, DocumentFileKind::PayloadXml);
        }
        other => panic!("expected MacRecoveryArtifactMissing, got {other:?}"),
    }

    // **Counter WAS claimed** (MR-CLAIM committed in its own
    // `with_immediate` envelope BEFORE MR-PERSIST began).
    // The W6 stage-3 invariant breach is forensically visible:
    // counter=1 + no MAC_RECOVERY_RESIGNED audit ⇒ recovery never
    // completed, doc is structurally broken.
    assert_eq!(
        read_counter(&pool, doc).await,
        1,
        "MR-CLAIM must have committed before the Pre-PERSIST rollback"
    );

    // No MAC_RECOVERY_RESIGNED audit (MR-PERSIST tx rolled back
    // entirely).
    let events = read_audit_events(&pool, doc).await;
    assert!(
        events.iter().all(|e| e.0 != "MAC_RECOVERY_RESIGNED"),
        "MR-PERSIST rollback must drop the RESIGNED audit row: {events:?}"
    );

    // SIGNED_XML still has the OLD bytes (MR-PERSIST never touched
    // it; the closure errored before the second replace_tx).
    let signed = read_artifact(&pool, doc, DocumentFileKind::SignedXml).await;
    assert_eq!(
        signed.as_deref(),
        Some(b"OLD-SIGNED-CMS".as_slice()),
        "SIGNED_XML must remain at attempt-#1 bytes (MR-PERSIST rolled back)"
    );

    // previous_hash also unchanged (UPDATE rolled back).
    let prev = read_previous_hash(&pool, doc).await.unwrap();
    assert_eq!(
        prev,
        vec![0xAAu8; 32],
        "previous_hash UPDATE must roll back with the rest of MR-PERSIST"
    );
}
