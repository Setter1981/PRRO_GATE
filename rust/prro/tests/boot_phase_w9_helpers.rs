//! W9.2 — `services::reconciliation::boot_phase` per-DocState helpers
//! integration tests.
//!
//! Three helpers shipped in W9.2 (per freeze §4.4 + §4.5 + §4.6):
//!   - `resume_sending_to_error_retryable` (§4.4)
//!   - `advance_sent_to_kvt1_from_probe`   (§4.5)
//!   - `passive_hold_kvt1`                 (§4.6)
//!
//! Each test exercises a single helper against a real `SqlitePool`
//! with minimal seed setup.  Tests are isolated by `tempfile::tempdir`.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::transport_trace;
use prro::db::tx::with_immediate;
use prro::services::reconciliation::boot_phase;
use prro::services::reconciliation::ReconcileGuard;
use prro::transports::dps::dto::CheckAck;
use sqlx::SqlitePool;

/// W2 module-level enforcement helper — see
/// `tests/app_boot_reconciliation.rs::recon_guard` for the contract.
fn recon_guard() -> ReconcileGuard<'static> {
    ReconcileGuard::for_integration_test_only()
}

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool");
    (dir, pool)
}

/// Seed an FN config + a fiscal_documents row in the requested state.
///
/// **W3 (M3b)**: for `state == "KVT1"`, also populates the W3
/// `first_kvt1_at` column with `CURRENT_TIMESTAMP`.  Pre-W3 tests
/// relied on `updated_at` as proxy + the `fd_updated_at` trigger
/// auto-populating it; post-W3 the canonical column is
/// `first_kvt1_at`, which has no auto-stamping for direct INSERTs
/// (the W3 stamp lives inside `fiscal_documents::transition_state`,
/// which test seeds bypass).  So tests must populate it explicitly
/// here to mimic production semantics.
async fn seed_doc_in_state(pool: &SqlitePool, doc_byte: u8, state: &str) -> DocumentId {
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
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, first_kvt1_at) \
         VALUES (?, ?, '1234567890', ?, 'SELL', ?, 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, \
            CASE WHEN ? = 'KVT1' THEN CURRENT_TIMESTAMP ELSE NULL END)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(state)
    .bind(&sha)
    .bind(state)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_state(pool: &SqlitePool, doc: DocumentId) -> String {
    let s: String = sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap();
    s
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─── resume_sending_to_error_retryable ─────────────────────────────────

#[tokio::test]
async fn resume_sending_applies_when_doc_is_sending() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xA1, "SENDING").await;
    let applied = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .expect("helper must not fail on in-state doc");
    assert!(applied, "applied = true when CAS rows_affected == 1");
    assert_eq!(read_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        audit_count(&pool, "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        1
    );
}

#[tokio::test]
async fn resume_sending_noop_when_doc_not_in_sending() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xA2, "SIGNED").await;
    let applied = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .expect("no-op must Ok(false), not Err");
    assert!(!applied, "applied = false when CAS rows_affected == 0");
    assert_eq!(read_state(&pool, doc).await, "SIGNED");
    assert_eq!(
        audit_count(&pool, "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        0
    );
}

#[tokio::test]
async fn resume_sending_idempotent_second_call_is_noop() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xA3, "SENDING").await;
    let first = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .unwrap();
    let second = boot_phase::resume_sending_to_error_retryable(&pool, doc)
        .await
        .unwrap();
    assert!(first, "first call applies");
    assert!(!second, "second call no-ops");
    assert_eq!(read_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        audit_count(&pool, "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        1
    );
}

// ─── advance_sent_to_kvt1_from_probe ───────────────────────────────────

async fn alloc_inflight_trace(pool: &SqlitePool, doc: DocumentId) -> i32 {
    use prro::db::repositories::transport_trace::NewAttempt;
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let n = transport_trace::allocate_and_insert_tx(
                tx,
                doc,
                NewAttempt {
                    backend_profile_id: "b1".into(),
                    transport_profile_id: "t1".into(),
                    request_envelope_sha256: [0u8; 32],
                    // M1 item 4: in-flight SENT-recovery probe trace.
                    is_probe: true,
                },
            )
            .await?;
            Ok::<i32, anyhow::Error>(n)
        })
    })
    .await
    .unwrap()
}

fn fake_ack(id: &str, data_sign: &[u8]) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: data_sign.to_vec(),
    }
}

#[tokio::test]
async fn advance_sent_to_kvt1_applies_full_envelope() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB1, "SENT").await;
    let attempt_no = alloc_inflight_trace(&pool, doc).await;
    let ack = fake_ack("SRV-FISCAL-12345", &[0x11, 0x22, 0x33]);

    let ok = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        attempt_no,
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await
    .expect("helper must succeed on Sent + in-flight trace");
    assert!(ok, "CAS applied → Ok(true)");

    // (1) State advanced.
    assert_eq!(read_state(&pool, doc).await, "KVT1");

    // (2) KVT1_RAW persisted.
    let kvt1_raw: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(kvt1_raw.as_deref(), Some(&[0x11, 0x22, 0x33][..]));

    // (3) Trace completed with OK outcome + server_fiscal_no.
    let (completed_at, outcome, server_id): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT completed_at, outcome_kind, server_fiscal_no FROM transport_trace \
             WHERE document_id = ? AND attempt_no = ?",
        )
        .bind(doc)
        .bind(attempt_no)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(completed_at.is_some());
    assert_eq!(outcome.as_deref(), Some("OK"));
    assert_eq!(server_id.as_deref(), Some("SRV-FISCAL-12345"));

    // (4) Audit row.
    assert_eq!(audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await, 1);
}

#[tokio::test]
async fn advance_sent_to_kvt1_returns_false_when_doc_not_in_sent() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB2, "SIGNED").await;
    // NC-01: non-empty data_sign so the CAS-conflict path is what's exercised
    // here (the empty-data_sign guard would otherwise short-circuit first — that
    // case is pinned separately in kill_point_matrix::k4b_…).
    let ack = fake_ack("SRV-X", &[0xDE, 0xAD]);
    let ok = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        1, // attempt_no doesn't matter — CAS bails first
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await
    .expect("CAS no-op must Ok(false), not Err");
    assert!(!ok);
    assert_eq!(read_state(&pool, doc).await, "SIGNED");
    assert_eq!(audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await, 0);
    // No KVT1_RAW persisted on bail.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_files WHERE document_id = ?")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn advance_sent_to_kvt1_idempotent_on_repeat_call() {
    // MED 1 fix (freeze §10.3 row d): second invocation on an
    // already-advanced doc must no-op (CAS Sent→Kvt1 sees doc in
    // Kvt1 → rows_affected=0 → Ok(false)).  No duplicate audit, no
    // duplicate KVT1_RAW write, no duplicate trace completion.
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB4, "SENT").await;
    let attempt_no = alloc_inflight_trace(&pool, doc).await;
    let ack = fake_ack("SRV-FISCAL-IDEMP", &[0xAB]);

    // First call applies the full envelope.
    let r1 = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        attempt_no,
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await
    .expect("first call must succeed");
    assert!(r1, "first call applies → Ok(true)");
    assert_eq!(read_state(&pool, doc).await, "KVT1");

    // Second call: doc is now in Kvt1, CAS Sent→Kvt1 no-ops.
    let r2 = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        attempt_no,
        &ack,
        "2026-05-11T09:00:02Z",
        "2026-05-11T09:00:03Z",
    )
    .await
    .expect("second call must Ok(false), not Err");
    assert!(!r2, "second call no-ops → Ok(false)");

    // Forensic invariants: no duplicates.
    assert_eq!(
        audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await,
        1,
        "single audit row across the two calls"
    );
    // document_files Kvt1Raw: single row (replace_tx is INSERT OR
    // REPLACE but second call bails at CAS before touching it).
    let n_kvt1_raw: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_files WHERE document_id = ? AND kind = 'KVT1_RAW'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n_kvt1_raw, 1, "single KVT1_RAW row");
    // Trace row stays completed with the FIRST call's wire times
    // (second call bails before reaching complete_via_recovery_tx).
    let wire_started: Option<String> = sqlx::query_scalar(
        "SELECT wire_call_started_at FROM transport_trace WHERE document_id = ? AND attempt_no = ?",
    )
    .bind(doc)
    .bind(attempt_no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        wire_started.as_deref(),
        Some("2026-05-11T09:00:00Z"),
        "first call's wire times preserved (second call no-ops)"
    );
}

#[tokio::test]
async fn advance_sent_to_kvt1_errors_when_trace_row_missing() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xB3, "SENT").await;
    // NO transport_trace row allocated — recovery helper requires one.
    let ack = fake_ack("SRV-X", &[1]);
    let result = boot_phase::advance_sent_to_kvt1_from_probe(
        &pool,
        doc,
        1, // no row matches
        &ack,
        "2026-05-11T09:00:00Z",
        "2026-05-11T09:00:01Z",
    )
    .await;
    assert!(
        result.is_err(),
        "missing trace row → envelope rolls back, anyhow::Err"
    );
    // State unchanged (transaction rolled back).
    assert_eq!(read_state(&pool, doc).await, "SENT");
    assert_eq!(audit_count(&pool, "BOOT_LAST_CHK_MATCH_KVT1").await, 0);
}

// ─── passive_hold_kvt1 ─────────────────────────────────────────────────

#[tokio::test]
async fn passive_hold_kvt1_emits_audit_only() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC1, "KVT1").await;
    boot_phase::passive_hold_kvt1(&pool, doc)
        .await
        .expect("passive_hold_kvt1 must succeed");
    assert_eq!(read_state(&pool, doc).await, "KVT1", "state unchanged");
    assert_eq!(audit_count(&pool, "BOOT_KVT1_HOLD_DEFERRED").await, 1);
    // No transport_trace row created.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace WHERE document_id = ?")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "no transport_trace row created by passive hold");
    // No document_files row created.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_files WHERE document_id = ?")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "no document_files row created");
}

#[tokio::test]
async fn passive_hold_kvt1_idempotent_each_call_emits_one_audit() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC2, "KVT1").await;
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    assert_eq!(audit_count(&pool, "BOOT_KVT1_HOLD_DEFERRED").await, 2);
    assert_eq!(read_state(&pool, doc).await, "KVT1");
}

/// W3 (M3b) — set `first_kvt1_at` to a literal SQLite timestamp
/// string.  Replaces the pre-W3
/// `drop_fd_updated_at_trigger` + `UPDATE updated_at` shape: now
/// that `first_kvt1_at` is a dedicated column with no auto-stamp
/// trigger, tests can write to it directly.  Also used for the
/// malformed-timestamp degrade test (literal `"not-a-timestamp"`).
async fn set_first_kvt1_at_literal(pool: &SqlitePool, doc: DocumentId, sqlite_ts: &str) {
    sqlx::query("UPDATE fiscal_documents SET first_kvt1_at = ? WHERE document_id = ?")
        .bind(sqlite_ts)
        .bind(doc)
        .execute(pool)
        .await
        .expect("set first_kvt1_at to literal");
}

/// Read the most recent BOOT_KVT1_HOLD_DEFERRED audit row's severity
/// + payload for the given doc.  Used by the three age-bucket tests.
async fn read_latest_kvt1_hold_audit(
    pool: &SqlitePool,
    doc: DocumentId,
) -> (String, serde_json::Value) {
    let entity_id = doc
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    let (severity, payload_json): (String, String) = sqlx::query_as(
        "SELECT severity, event_payload_json FROM audit_log \
         WHERE event_type = 'BOOT_KVT1_HOLD_DEFERRED' AND entity_id = ? \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(&entity_id)
    .fetch_one(pool)
    .await
    .expect("audit row present");
    let payload: serde_json::Value =
        serde_json::from_str(&payload_json).expect("payload is valid JSON");
    (severity, payload)
}

#[tokio::test]
async fn passive_hold_kvt1_emits_info_severity_for_fresh_hold() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC3, "KVT1").await;
    // Default seed → updated_at = CURRENT_TIMESTAMP → age ~ 0s → Info.
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    let (severity, payload) = read_latest_kvt1_hold_audit(&pool, doc).await;
    assert_eq!(severity, "INFO", "fresh hold must stay Info");
    assert_eq!(payload["severity_threshold"], "INFO");
    let age = payload["kvt1_age_seconds"]
        .as_i64()
        .expect("kvt1_age_seconds is i64");
    assert!(
        (0..60).contains(&age),
        "fresh seed should have age in [0, 60), got {age}"
    );
    assert!(
        payload["first_kvt1_at"].is_string(),
        "first_kvt1_at must be present as a string"
    );
}

#[tokio::test]
async fn passive_hold_kvt1_escalates_to_warning_after_one_hour() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC4, "KVT1").await;
    // Back-date `first_kvt1_at` by 2 hours — well into the Warning
    // bucket (1h ≤ age < 24h).  Use SQLite `datetime('now', '-2 hours')`
    // so the age stays inside (3600, 86400) regardless of when the
    // test runs.  W3: dedicated column has no auto-stamp trigger,
    // so direct UPDATE works without trigger surgery.
    sqlx::query(
        "UPDATE fiscal_documents SET first_kvt1_at = datetime('now', '-2 hours') WHERE document_id = ?",
    )
    .bind(doc)
    .execute(&pool)
    .await
    .expect("backdate first_kvt1_at to -2h");
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    let (severity, payload) = read_latest_kvt1_hold_audit(&pool, doc).await;
    assert_eq!(severity, "WARNING", "2h hold must escalate to Warning");
    assert_eq!(payload["severity_threshold"], "WARNING");
    let age = payload["kvt1_age_seconds"]
        .as_i64()
        .expect("kvt1_age_seconds is i64");
    assert!(
        (3600..86_400).contains(&age),
        "2h back-date age should be in [3600, 86400), got {age}"
    );
}

#[tokio::test]
async fn passive_hold_kvt1_escalates_to_error_after_twenty_four_hours() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC5, "KVT1").await;
    // Back-date `first_kvt1_at` to 25 hours ago — clears the
    // 86400s Error threshold.  W3: dedicated column, no trigger.
    sqlx::query(
        "UPDATE fiscal_documents SET first_kvt1_at = datetime('now', '-25 hours') WHERE document_id = ?",
    )
    .bind(doc)
    .execute(&pool)
    .await
    .expect("backdate first_kvt1_at to -25h");
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    let (severity, payload) = read_latest_kvt1_hold_audit(&pool, doc).await;
    assert_eq!(severity, "ERROR", "25h hold must escalate to Error");
    assert_eq!(payload["severity_threshold"], "ERROR");
    let age = payload["kvt1_age_seconds"]
        .as_i64()
        .expect("kvt1_age_seconds is i64");
    assert!(
        age >= 86_400,
        "25h back-date should give age ≥ 86400s, got {age}"
    );
}

#[tokio::test]
async fn passive_hold_kvt1_unparseable_first_kvt1_at_falls_back_to_info_age_zero() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC6, "KVT1").await;
    // Deliberate corruption of first_kvt1_at to verify the parser
    // failure mode is "degrade-and-emit" rather than "crash".
    set_first_kvt1_at_literal(&pool, doc, "not-a-timestamp").await;
    boot_phase::passive_hold_kvt1(&pool, doc).await.unwrap();
    let (severity, payload) = read_latest_kvt1_hold_audit(&pool, doc).await;
    assert_eq!(
        severity, "INFO",
        "unparseable updated_at must fall back to Info (degrade-and-emit, not crash)"
    );
    assert_eq!(payload["kvt1_age_seconds"], 0);
    // Echoed verbatim — operator can see the malformed value.
    assert_eq!(payload["first_kvt1_at"], "not-a-timestamp");
}

// ─── M3b W3: fiscal_documents::transition_state Kvt1 arm + first_kvt1_at column ────────

/// W3 plan §Task 3 acceptance: `transition_state` stamps
/// `first_kvt1_at` on the `Sent → Kvt1` arm.  Lives in this test
/// file (not a fiscal_documents-repo test) because it exercises the
/// W3 end-to-end CAS-then-read path via a seeded fiscal doc, which
/// is the existing pattern this module already supports.
#[tokio::test]
async fn transition_state_stamps_first_kvt1_at_on_sent_to_kvt1() {
    use prro::db::models::enums::DocState;
    use prro::db::repositories::fiscal_documents::{self, TransitionOutcome};
    use prro::db::tx::with_immediate;

    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC7, "SENT").await;

    let outcome: TransitionOutcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o = fiscal_documents::transition_state(tx, doc, DocState::Sent, DocState::Kvt1)
                .await
                .map_err(anyhow::Error::from)?;
            Ok::<_, anyhow::Error>(o)
        })
    })
    .await
    .expect("transition_state envelope");
    assert!(matches!(outcome, TransitionOutcome::Applied));

    let first_kvt1_at: Option<String> =
        sqlx::query_scalar("SELECT first_kvt1_at FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        first_kvt1_at.is_some(),
        "first_kvt1_at must be stamped on Sent → Kvt1; got NULL"
    );
    // Sanity check on shape: SQLite `CURRENT_TIMESTAMP` returns
    // `YYYY-MM-DD HH:MM:SS`.  Don't compare the actual value (race
    // with wall clock), just shape.
    let ts = first_kvt1_at.unwrap();
    assert_eq!(
        ts.len(),
        19,
        "stamp must be SQLite default ISO shape, got: {ts:?}"
    );
    assert!(ts.chars().nth(4) == Some('-'), "missing date separator");
    assert!(
        ts.chars().nth(10) == Some(' '),
        "missing date/time separator"
    );
}

/// W3 plan §Task 3 acceptance: `transition_state` COALESCE branch
/// preserves `first_kvt1_at` on Kvt1 RE-ENTRY.  Exercises the
/// COALESCE-with-non-NULL path explicitly via the whitelisted
/// `ErrorRetryable → Kvt1` re-entry edge (which exists in
/// `allowed_transition` for the M3a two-tick retry path).
///
/// Cycle (3 transitions, all whitelisted):
///   1. `Sent → Kvt1`          — stamps first_kvt1_at via COALESCE NULL arm
///   2. (manual UPDATE: replace stamp with sentinel `'2020-01-01 00:00:00'`)
///   3. `Kvt1 → ErrorRetryable` — non-Kvt1 arm; column untouched
///   4. `ErrorRetryable → Kvt1` — Kvt1 RE-ENTRY; COALESCE-non-NULL arm
///                                 fires → ORIGINAL sentinel preserved
///
/// Directly exercises COALESCE with both NULL input (step 1) and
/// non-NULL input (step 4).  Operator review pin (2026-05-14): this
/// is the load-bearing acceptance proof; the pre-PR fixture only
/// covered the non-Kvt1 no-touch case indirectly.
#[tokio::test]
async fn transition_state_coalesce_preserves_first_kvt1_at_on_kvt1_re_entry() {
    use prro::db::models::enums::DocState;
    use prro::db::repositories::fiscal_documents::{self, TransitionOutcome};
    use prro::db::tx::with_immediate;

    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC8, "SENT").await;

    // Step 1: Sent → Kvt1 — COALESCE NULL arm stamps CURRENT_TIMESTAMP.
    let outcome_1: TransitionOutcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o = fiscal_documents::transition_state(tx, doc, DocState::Sent, DocState::Kvt1)
                .await
                .map_err(anyhow::Error::from)?;
            Ok::<_, anyhow::Error>(o)
        })
    })
    .await
    .expect("Sent → Kvt1");
    assert!(matches!(outcome_1, TransitionOutcome::Applied));
    let stamp_after_step1: Option<String> =
        sqlx::query_scalar("SELECT first_kvt1_at FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stamp_after_step1.is_some(), "step 1 must stamp");

    // Step 2: replace stamp with a known sentinel.  Both subsequent
    // transitions must leave this exact string in place; any clobber
    // (by either the non-Kvt1 arm at step 3 OR the COALESCE-non-NULL
    // arm at step 4) is detectable.
    const SENTINEL: &str = "2020-01-01 00:00:00";
    sqlx::query("UPDATE fiscal_documents SET first_kvt1_at = ? WHERE document_id = ?")
        .bind(SENTINEL)
        .bind(doc)
        .execute(&pool)
        .await
        .expect("backdate first_kvt1_at to sentinel");

    // Step 3: Kvt1 → ErrorRetryable — non-Kvt1 arm of transition_state;
    // first_kvt1_at must stay = SENTINEL.
    let outcome_3: TransitionOutcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o = fiscal_documents::transition_state(
                tx,
                doc,
                DocState::Kvt1,
                DocState::ErrorRetryable,
            )
            .await
            .map_err(anyhow::Error::from)?;
            Ok::<_, anyhow::Error>(o)
        })
    })
    .await
    .expect("Kvt1 → ErrorRetryable");
    assert!(matches!(outcome_3, TransitionOutcome::Applied));
    let stamp_after_step3: Option<String> =
        sqlx::query_scalar("SELECT first_kvt1_at FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stamp_after_step3.as_deref(),
        Some(SENTINEL),
        "non-Kvt1 transition must NOT touch first_kvt1_at"
    );

    // Step 4: ErrorRetryable → Kvt1 — Kvt1 RE-ENTRY.  The
    // transition_state Kvt1-arm SQL is
    //   SET state = ?, first_kvt1_at = COALESCE(first_kvt1_at, CURRENT_TIMESTAMP)
    // COALESCE sees the SENTINEL (non-NULL) and preserves it.  This
    // is the load-bearing COALESCE-non-NULL test that operator
    // review pinned.
    let outcome_4: TransitionOutcome = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let o = fiscal_documents::transition_state(
                tx,
                doc,
                DocState::ErrorRetryable,
                DocState::Kvt1,
            )
            .await
            .map_err(anyhow::Error::from)?;
            Ok::<_, anyhow::Error>(o)
        })
    })
    .await
    .expect("ErrorRetryable → Kvt1");
    assert!(matches!(outcome_4, TransitionOutcome::Applied));
    let stamp_after_step4: Option<String> =
        sqlx::query_scalar("SELECT first_kvt1_at FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stamp_after_step4.as_deref(),
        Some(SENTINEL),
        "Kvt1 re-entry via ErrorRetryable → Kvt1 must preserve original first_kvt1_at via COALESCE"
    );
}

/// W3 plan §Task 3 acceptance: migration 014 backfills `first_kvt1_at`
/// from `updated_at` for rows already in KVT1 at the time of migration.
/// Test scenario: seed a doc with state KVT1 + NULL first_kvt1_at
/// (simulating pre-W3 state), then run the backfill manually
/// (simulates re-running migration on a populated DB) and verify
/// the row got `first_kvt1_at == updated_at`.
#[tokio::test]
async fn migration_014_backfill_populates_first_kvt1_at_for_pre_w3_kvt1_rows() {
    let (_dir, pool) = fresh_pool().await;
    let doc = seed_doc_in_state(&pool, 0xC9, "KVT1").await;
    // Simulate pre-W3 state: clear first_kvt1_at (seed helper
    // auto-populates it post-W3; clear it back to mimic a row that
    // existed before the migration ran).
    sqlx::query("UPDATE fiscal_documents SET first_kvt1_at = NULL WHERE document_id = ?")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    let cleared: Option<String> =
        sqlx::query_scalar("SELECT first_kvt1_at FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(cleared.is_none(), "precondition: first_kvt1_at NULL");

    // Capture updated_at BEFORE the backfill.  The backfill sets
    // `first_kvt1_at = updated_at`, but the `fd_updated_at` AFTER-UPDATE
    // trigger (migrations/002) then BUMPS `updated_at = CURRENT_TIMESTAMP` —
    // so the POST-backfill `updated_at` no longer equals the value the
    // backfill copied.  Comparing the backfilled value to the post-backfill
    // `updated_at` flaked across a one-second boundary under load; snapshot
    // the pre-backfill value and compare to THAT.
    let updated_at_before: String =
        sqlx::query_scalar("SELECT updated_at FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Re-run the migration backfill statement (matches
    // migrations/014_first_kvt1_at.sql).
    sqlx::query(
        "UPDATE fiscal_documents SET first_kvt1_at = updated_at \
         WHERE state = 'KVT1' AND first_kvt1_at IS NULL",
    )
    .execute(&pool)
    .await
    .expect("backfill statement");

    let backfilled: Option<String> =
        sqlx::query_scalar("SELECT first_kvt1_at FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        backfilled.as_deref(),
        Some(updated_at_before.as_str()),
        "backfill must copy the updated_at value present at backfill time \
         (the fd_updated_at trigger bumps updated_at afterward)"
    );
}

// ─── run_boot_reconciliation stub ──────────────────────────────────────

#[tokio::test]
async fn run_boot_reconciliation_stub_returns_ok() {
    let (_dir, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
        .await
        .expect("W9.2 stub returns Ok(()) — W9.3 wires dispatch");
}
