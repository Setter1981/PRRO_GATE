//! A.3 PR-B step 9a — migration 027 one-time fail-closed boundary assert.
//!
//! 027 is a schema-neutral, one-time assertion (a TEMP-table CHECK trick): it
//! ABORTS the 026→027 upgrade if the DB carries a pre-A.3 online-issued-but-
//! seed-unadvanced row, and leaves NO permanent trace (post-A.3 healthy DBs rest
//! online-issued docs at SENT with sfn set — a permanent check would false-flag
//! them). These pins lock all three properties.

use sqlx::SqlitePool;

/// The migration SQL, executed directly to exercise the assertion against a
/// hazard-present DB (the embedded migrator runs 027 at first migrate, before a
/// hazard could be injected, so the failure path is exercised by re-running the
/// migration's own SQL — the assertion is idempotent + leaves no residue).
const M027: &str = include_str!("../migrations/027_online_issued_boundary_assert.sql");

const FN: &str = "5000000001";

async fn migrated_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = prro::db::open_pool(&dir.path().join("m027.db"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    (dir, pool)
}

/// Insert one fiscal_documents row with explicit D3 fields (shift_id left NULL —
/// the column is nullable and the assert never reads it).
async fn insert_doc(pool: &SqlitePool, lnd: i64, state: &str, sfn: Option<&str>, ofn: Option<i64>) {
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, server_fiscal_no, offline_fiscal_no) \
         VALUES (randomblob(16), randomblob(16), ?, ?, 'SELL', ?, 'b', 't', 'ONLINE', \
            '2026-07-06T00:00:00Z', '{}', ?, ?, ?)",
    )
    .bind(FN)
    .bind(lnd)
    .bind(state)
    .bind(&[0u8; 32][..])
    .bind(sfn)
    .bind(ofn)
    .execute(pool)
    .await
    .unwrap();
}

/// Run the 027 assertion SQL directly. Returns Err iff the CHECK aborts it.
async fn run_027(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(M027).execute(pool).await.map(|_| ())
}

async fn rerun_full_migrator(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations"
    )))
    .await
    .expect("load migrations");
    migrator.run(pool).await
}

/// (i) A pre-A.3 online-issued-unadvanced row (offline_fiscal_no NULL, sfn set,
/// state != ACK) makes 027 ABORT — fail-closed at the semantic boundary.
#[tokio::test]
async fn hazard_row_aborts_migration_027() {
    let (_dir, pool) = migrated_pool().await;
    // online-origin (ofn NULL), got a DPS id (sfn set), but rests at SENT (< ACK)
    // → under the OLD semantics the seed was NOT advanced → hazard.
    insert_doc(&pool, 1, "SENT", Some("70000001"), None).await;

    let err = run_027(&pool)
        .await
        .expect_err("027 MUST abort on a pre-A.3 online-issued-unadvanced hazard row");
    let msg = err.to_string();
    assert!(
        msg.contains("CHECK") || msg.to_lowercase().contains("constraint"),
        "027 must abort via the boundary CHECK, got: {msg}"
    );
}

/// A KVT2 / RMR hazard variant also aborts (any non-ACK online-issued state).
#[tokio::test]
async fn hazard_row_non_ack_states_abort_migration_027() {
    for state in [
        "KVT1",
        "KVT2",
        "REQUIRES_MANUAL_RECONCILIATION",
        "ERROR_RETRYABLE",
    ] {
        let (_dir, pool) = migrated_pool().await;
        insert_doc(&pool, 1, state, Some("70000002"), None).await;
        assert!(
            run_027(&pool).await.is_err(),
            "027 must abort on an online-issued-unadvanced row resting at {state}"
        );
    }
}

/// (ii) A clean DB (no hazard) passes 027 — both as part of the initial migrate
/// (implicit — `migrated_pool` succeeded) and on a direct re-run. Rows that are
/// NOT hazards are ignored: an ACK online doc (seed advanced under old code), an
/// offline-origin doc (offline_fiscal_no set), and a doc with no sfn.
#[tokio::test]
async fn clean_db_passes_migration_027() {
    let (_dir, pool) = migrated_pool().await;
    insert_doc(&pool, 1, "ACK", Some("70000003"), None).await; // ACK → not a hazard
    insert_doc(&pool, 2, "SENT", None, None).await; // no sfn → not issued
    insert_doc(&pool, 3, "OFFLINE_LOCAL_ACK", Some("70000004"), Some(5001)).await; // offline-origin
    run_027(&pool)
        .await
        .expect("027 must pass on a DB with zero hazard rows");
}

/// (iii) NO PERMANENT TRACE: after 027 has been applied (recorded), a NEW legit
/// post-A.3 online-issued doc resting at SENT (sfn set, seed advanced atomically)
/// must NOT be re-flagged — a re-run of the FULL migrator is a clean no-op (027
/// is recorded → skipped). Proves the assert is one-time, not a permanent check.
#[tokio::test]
async fn legit_post_a3_sent_doc_not_reflagged_on_remigrate() {
    let (_dir, pool) = migrated_pool().await; // 027 already applied (clean)
                                              // A healthy post-A.3 online-issued doc rests at SENT with sfn set — exactly
                                              // the shape a PERMANENT check would false-flag.
    insert_doc(&pool, 1, "SENT", Some("70000005"), None).await;

    rerun_full_migrator(&pool)
        .await
        .expect("re-running the migrator MUST be a no-op (027 recorded → not re-checked)");
}
