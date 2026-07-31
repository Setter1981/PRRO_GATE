//! Migration 038 — UnknownStatus → ProbeRequired backward-compat (CS-3 Slice E Pin 3).
//!
//! 038 flips the evidence-matrix `UnknownStatus` arm in lock-step with the classifier flip
//! (`routing_for_indeterminate(UnknownStatus) → ProbeRequired`). The arm is discriminated by
//! `OLD.state`:
//!   * a FRESH transition into OUTCOME_OBSERVED (`OLD.state='CALL_STARTED'`) accepts ONLY the new
//!     `(ProbeRequired, ProbeRequired)` — the live `record_outcome` writer's post-flip output —
//!     and REJECTS the legacy `(TransientRetry, NoNodeEffect)`;
//!   * a re-validation UPDATE of a row ALREADY at OUTCOME_OBSERVED (`OLD.state='OUTCOME_OBSERVED'`)
//!     ALSO accepts the legacy combo, so a PRE-038 UnknownStatus row (written under 037) can still
//!     be driven to a terminal state without tripping the matrix (defensive backward-compat).
//!
//! `fresh_pool` runs ALL migrations incl. 038 (`sqlx::migrate!`). The legacy-row test seeds a
//! pre-038 OO row by dropping the matrix trigger, writing the legacy combo (as 037 accepted it),
//! then re-arming 038 verbatim — the "the row existed before the upgrade" scenario, on one DB.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use sqlx::sqlite::SqliteQueryResult;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 038");
    (dir, pool)
}

async fn seed_doc(pool: &SqlitePool, fscl: &str, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 1, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fscl)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn insert_res(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) {
    let row = NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fscl.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    };
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::insert(tx, row)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .expect("insert reservation");
}

async fn mark_call_started(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='CALL_STARTED', call_started_at='2026-07-17T00:00:00Z', authorized_generation=1 \
         WHERE reservation_id=?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("advance to CALL_STARTED");
}

/// UPDATE the reservation to OUTCOME_OBSERVED carrying an `UnknownStatus` leaf (code -99 + digest)
/// with the given `(routing_class, node_effect)`. Returns the raw `Result` so callers assert accept
/// (Ok) vs matrix-abort (Err).
async fn update_to_oo_unknown(
    pool: &SqlitePool,
    res_byte: u8,
    routing: &str,
    node_effect: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', apply_state='PENDING_APPLY', \
             submission_certainty='SUBMITTED_UNKNOWN', response_provenance='PARSED_DPS_ENVELOPE', \
             routing_class=?, node_effect=?, \
             evidence_kind='UnknownStatus', evidence_text=NULL, evidence_code=-99, evidence_digest=?, \
             remote_correlation_id=NULL \
         WHERE reservation_id=?",
    )
    .bind(routing)
    .bind(node_effect)
    .bind(vec![0u8; 32])
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
}

/// FRESH transition (`OLD.state='CALL_STARTED'`): the new `(ProbeRequired, ProbeRequired)` is the
/// ONLY accepted UnknownStatus combo; the legacy `(TransientRetry, NoNodeEffect)` is REJECTED.
#[tokio::test]
async fn fresh_unknown_status_transition_requires_probe_required() {
    let (_d, pool) = fresh_pool().await;

    // (a) fresh ProbeRequired → accepted.
    let doc = seed_doc(&pool, "7900000001", 0x01).await;
    insert_res(&pool, 0x01, doc, "7900000001").await;
    mark_call_started(&pool, 0x01).await;
    update_to_oo_unknown(&pool, 0x01, "ProbeRequired", "ProbeRequired")
        .await
        .expect("038 fresh arm accepts (ProbeRequired, ProbeRequired) — the live writer's output");

    // (b) fresh legacy TransientRetry → REJECTED (OLD.state='CALL_STARTED' forbids the legacy combo).
    let doc2 = seed_doc(&pool, "7900000002", 0x02).await;
    insert_res(&pool, 0x02, doc2, "7900000002").await;
    mark_call_started(&pool, 0x02).await;
    let err = update_to_oo_unknown(&pool, 0x02, "TransientRetry", "NoNodeEffect")
        .await
        .expect_err("038 fresh arm MUST reject the legacy (TransientRetry, NoNodeEffect)");
    assert!(
        err.to_string().to_lowercase().contains("matrix"),
        "the abort is the evidence-matrix violation, got: {err}"
    );
}

/// RE-VALIDATION (`OLD.state='OUTCOME_OBSERVED'`): a PRE-038 legacy UnknownStatus OO row survives an
/// OO-preserving UPDATE (the operator/apply path re-fires the matrix). This is the backward-compat
/// guarantee — the lenient arm keeps the row drivable-to-terminal.
#[tokio::test]
async fn pre_038_legacy_oo_row_survives_revalidation_update() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, "7900000003", 0x03).await;
    insert_res(&pool, 0x03, doc, "7900000003").await;
    mark_call_started(&pool, 0x03).await;

    // Seed a PRE-038 legacy OO row: drop the matrix guard, write the legacy combo (as 037 accepted),
    // then re-arm 038 verbatim — reproduces "the row existed at OO before 038 was applied".
    sqlx::query("DROP TRIGGER delivery_reservation_evidence_matrix_update")
        .execute(&pool)
        .await
        .expect("drop the matrix trigger to seed a pre-038 row");
    update_to_oo_unknown(&pool, 0x03, "TransientRetry", "NoNodeEffect")
        .await
        .expect("legacy OO row seeds while the guard is dropped (this is a 037-era row)");
    sqlx::raw_sql(include_str!(
        "../migrations/038_delivery_reservation_unknown_status_probe.sql"
    ))
    .execute(&pool)
    .await
    .expect("re-arm the 038 matrix trigger");

    // The OO-preserving UPDATE the operator/apply path performs (`apply_state → APPLIED`) re-fires the
    // BEFORE UPDATE matrix trigger with OLD.state='OUTCOME_OBSERVED'. The lenient arm accepts the
    // row's unchanged legacy `(TransientRetry, NoNodeEffect)` → no abort.
    sqlx::query("UPDATE delivery_reservation SET apply_state='APPLIED' WHERE reservation_id=?")
        .bind(&[0x03u8; 16][..])
        .execute(&pool)
        .await
        .expect(
            "038 lenient arm: a pre-038 legacy UnknownStatus OO row survives an OO-preserving UPDATE \
             (revert the `OLD.state='OUTCOME_OBSERVED'` branch → this aborts)",
        );

    let (state, apply): (String, String) = sqlx::query_as(
        "SELECT state, apply_state FROM delivery_reservation WHERE reservation_id=?",
    )
    .bind(&[0x03u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "OUTCOME_OBSERVED");
    assert_eq!(apply, "APPLIED", "the legacy row was driven to APPLIED");
}
