//! Migration 007 (UNIQUE INDEX on fiscal_documents(fiscal_number, lnd))
//! and migration 008 (state CHECK extension with 'SENDING' via table
//! rebuild).  Anchored on ADR-M3-A1 (UNIQUE guard against lnd drift),
//! ADR-M3-A8 (8-state pending set), ADR-M3-A9 step 1-3 (Sending variant
//! and table rebuild semantics).
//!
//! Pattern mirrors `migrations_apply.rs`: open a fresh pool (which runs
//! all migrations via `sqlx::migrate!`), then assert schema and
//! behavioural effects.

use prro::db::{
    models::{
        enums::{DocState, FiscalMode},
        ids::{DocumentId, RequestId},
    },
    open_pool,
    repositories::{
        fiscal_documents::{self as fd, NewDocument},
        fiscal_number_config::{self as fn_repo, NewFnConfig},
    },
};

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    (dir, pool)
}

async fn seed_fn(pool: &sqlx::SqlitePool, fn_id: &str) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: fn_id.into(),
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

fn doc_with_lnd(fn_id: &str, lnd: i64) -> NewDocument {
    use prro::db::models::enums::DocType;
    NewDocument {
        document_id: DocumentId::new(),
        request_id: RequestId::new(),
        fiscal_number: fn_id.into(),
        shift_id: None,
        offline_session_id: None,
        lnd,
        doc_type: DocType::Sell,
        backend_profile_id: "b".into(),
        transport_profile_id: "t".into(),
        fs_mode: "ONLINE",
        business_ts: "2026-04-22T12:00:00Z".into(),
        total_sum_kop: Some(15000),
        payload_json: r#"{"goods":[]}"#.into(),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        unsigned_xml_sha256: None,
        previous_hash: None,
        signed_by_cashier_id: None,
        signing_config_snapshot_id: None,
    }
}

#[tokio::test]
async fn migration_007_unique_index_blocks_duplicate_fn_lnd() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "4000000001").await;

    // First (fn=4000000001, lnd=1) — accepted.
    fd::insert_prepared(&pool, &doc_with_lnd("4000000001", 1))
        .await
        .unwrap();

    // Second (fn=4000000001, lnd=1) — must be rejected by ux_fd_fn_lnd.
    let dup_err = fd::insert_prepared(&pool, &doc_with_lnd("4000000001", 1)).await;
    let msg = format!("{:?}", dup_err.unwrap_err());
    assert!(
        msg.contains("UNIQUE") || msg.contains("constraint") || msg.contains("ux_fd_fn_lnd"),
        "expected UNIQUE constraint violation on (fiscal_number, lnd); got {msg}"
    );
}

#[tokio::test]
async fn migration_007_unique_index_listed_in_sqlite_master() {
    let (_d, pool) = fresh_pool().await;
    let row: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND name='ux_fd_fn_lnd'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(row.as_deref(), Some("ux_fd_fn_lnd"));
}

#[tokio::test]
async fn migration_008_sending_state_accepted_by_check() {
    // Behavioural confirmation: state CHECK admits 'SENDING' after the
    // table rebuild.  Insert via insert_prepared (always 'PREPARED'),
    // then UPDATE to 'SENDING' — succeeds iff CHECK lists 'SENDING'.
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "4000000001").await;
    let new = doc_with_lnd("4000000001", 1);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();

    sqlx::query("UPDATE fiscal_documents SET state = 'SENDING' WHERE document_id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .expect("UPDATE to SENDING must succeed: 008 added 'SENDING' to state CHECK");
}

#[tokio::test]
async fn migration_008_bogus_state_still_rejected_by_check() {
    // Negative control: the CHECK is still a closed list, not a
    // free-form string column.  An unlisted value must fail.
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "4000000001").await;
    let new = doc_with_lnd("4000000001", 1);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();

    let err =
        sqlx::query("UPDATE fiscal_documents SET state = 'BOGUS_STATE' WHERE document_id = ?")
            .bind(id)
            .execute(&pool)
            .await;
    let msg = format!("{:?}", err.unwrap_err());
    assert!(
        msg.contains("CHECK") || msg.contains("constraint"),
        "expected CHECK violation for unlisted state; got {msg}"
    );
}

#[tokio::test]
async fn doc_state_sending_round_trips_through_sqlx() {
    // Encode side: bind DocState::Sending in an UPDATE; decode side:
    // SELECT with `state: DocState` aliased query.  Both sides must
    // agree on the "SENDING" wire string.
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "4000000001").await;
    let new = doc_with_lnd("4000000001", 1);
    let id = new.document_id;
    fd::insert_prepared(&pool, &new).await.unwrap();

    sqlx::query("UPDATE fiscal_documents SET state = ? WHERE document_id = ?")
        .bind(DocState::Sending)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    // Use list_pending_for_fn — that single query exercises both the
    // SELECT decode path and the IN-clause membership of 'SENDING'.
    let pending = fd::list_pending_for_fn(&pool, "4000000001").await.unwrap();
    assert_eq!(pending.len(), 1, "Sending must be in pending set");
    assert_eq!(pending[0].state, DocState::Sending);
    assert_eq!(pending[0].document_id, id);
}

#[tokio::test]
async fn migration_008_pending_index_where_clause_includes_sending() {
    // The partial index ix_fd_state_pending must mention 'SENDING' in
    // its WHERE predicate; otherwise list_pending_for_fn's planner
    // silently misses Sending docs.
    let (_d, pool) = fresh_pool().await;
    let sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='index' AND name='ix_fd_state_pending'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        sql.contains("'SENDING'"),
        "ix_fd_state_pending WHERE clause must include 'SENDING' after migration 008; got: {sql}"
    );
}

/// Data-preservation regression test for migration 008.
///
/// All other tests in this file run against an empty fiscal_documents.
/// On an empty table, the rebuild has no rows to lose and no FK actions
/// to fire, so they would all pass even with a naive (broken) DROP TABLE
/// rebuild that ignores `document_files ON DELETE CASCADE`.
///
/// This test seeds pre-008 state with two fiscal_documents (B references
/// A via `related_receipt_id` self-FK) and two `document_files` rows,
/// then drives migrations 007+008 manually and asserts:
///   - both fiscal_documents rows survive (B.related_receipt_id link
///     intact),
///   - both document_files rows survive (CASCADE did not fire on the
///     parent rebuild),
///   - PRAGMA foreign_key_check is clean,
///   - 'SENDING' is accepted post-rebuild.
///
/// Approach: load the embedded migrator via `sqlx::migrate!()` and
/// apply migrations in two batches by version (1..=6, then 7..=8) using
/// `sqlx::raw_sql` for multi-statement support.  This sidesteps
/// `sqlx::migrate::Migrator::run` (which always applies everything) and
/// the migration-state tracking, both of which are irrelevant here.
#[tokio::test]
async fn migration_008_preserves_fiscal_documents_and_document_files() {
    use sqlx::migrate::Migrator;
    static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

    // Keep the TempDir alive across the test; std::mem::forget mirrors
    // the pattern used in repo_fiscal_documents_state_cas.rs::fresh_with_fn
    // (the TempDir guard would otherwise drop and clean up the directory
    // before the pool releases its handle).  OS-level cleanup of the
    // leaked dir is acceptable for a one-shot test process.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preserve.db");
    std::mem::forget(dir);
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await
        .unwrap();

    // Apply migrations 001-006 only (pre-008 base).  Each migration
    // runs inside its own explicit transaction to mirror what
    // `sqlx::migrate::Migrator` does in production: PRAGMA
    // defer_foreign_keys (used by 008) is per-transaction and would be
    // a no-op under raw_sql autocommit.
    for m in MIGRATOR.iter() {
        if m.version <= 6 {
            let mut tx = pool.begin().await.unwrap();
            sqlx::raw_sql(&m.sql).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
        }
    }

    // Seed fiscal_number_config (FK target).
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('4000000001', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Doc A: SELL, KVT2 — non-final pending state to also confirm the
    // state value survives the CHECK rebuild.
    let id_a: Vec<u8> = vec![0xAAu8; 16];
    let id_b: Vec<u8> = vec![0xBBu8; 16];
    let req_a: Vec<u8> = vec![0xA1u8; 16];
    let req_b: Vec<u8> = vec![0xB1u8; 16];
    let sha: Vec<u8> = vec![0u8; 32];

    sqlx::query(
        "INSERT INTO fiscal_documents(\
            document_id, request_id, fiscal_number, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical) \
         VALUES (?, ?, '4000000001', 1, 'SELL', 'KVT2', 'b', 't', 'ONLINE', \
                 '2026-04-22T12:00:00Z', '{}', ?)",
    )
    .bind(&id_a)
    .bind(&req_a)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();

    // Doc B: RETURN, SIGNED, related_receipt_id = A.document_id (self-FK).
    sqlx::query(
        "INSERT INTO fiscal_documents(\
            document_id, request_id, fiscal_number, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, related_receipt_id) \
         VALUES (?, ?, '4000000001', 2, 'RETURN', 'SIGNED', 'b', 't', 'ONLINE', \
                 '2026-04-22T12:00:00Z', '{}', ?, ?)",
    )
    .bind(&id_b)
    .bind(&req_b)
    .bind(&sha)
    .bind(&id_a)
    .execute(&pool)
    .await
    .unwrap();

    // document_files for both docs — these are the rows that a naive
    // DROP TABLE fiscal_documents would CASCADE-delete.
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) VALUES (?, 'PAYLOAD_XML', ?)",
    )
    .bind(&id_a)
    .bind(b"<xml>A</xml>".as_slice())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO document_files(document_id, kind, content) VALUES (?, 'KVT1_RAW', ?)")
        .bind(&id_b)
        .bind([0xAAu8, 0xBB].as_slice())
        .execute(&pool)
        .await
        .unwrap();

    // Apply migrations 007 + 008.  Each in its own explicit
    // transaction (see comment above on defer_foreign_keys scope).
    // We set defer_foreign_keys=ON via sqlx::query *before* raw_sql so
    // the pragma binds firmly to the transaction's connection state;
    // setting it as the first statement inside raw_sql turned out not
    // to apply to subsequent statements in the same execution batch
    // (empirical 2026-05-07 — the migration would then fail with
    // SQLITE_CONSTRAINT_FOREIGNKEY at commit despite the deferral
    // intent).
    // Apply migrations 007 + 008 the way `sqlx::migrate::Migrator`
    // does in production: each migration in its own transaction, with
    // foreign_keys=ON on the connection.  Migration 008 sets
    // PRAGMA defer_foreign_keys=ON internally and is structured to
    // drop + recreate fiscal_documents under the same name (no
    // ALTER TABLE RENAME), which sidesteps a SQLite quirk where
    // RENAME + self-FK + deferred validation breaks at COMMIT.
    for m in MIGRATOR.iter() {
        if m.version == 7 || m.version == 8 {
            let mut tx = pool.begin().await.unwrap();
            sqlx::raw_sql(&m.sql).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
        }
    }

    // Both fiscal_documents rows survive.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "both fiscal_documents rows must survive 008 rebuild"
    );

    // Self-FK link preserved.
    let related: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT related_receipt_id FROM fiscal_documents WHERE document_id = ?")
            .bind(&id_b)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        related.as_deref(),
        Some(id_a.as_slice()),
        "self-FK related_receipt_id link must survive rebuild"
    );

    // State column preserved (CHECK rebuild did not corrupt values).
    let state_a: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(&id_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state_a, "KVT2");

    // document_files rows survive (CASCADE did NOT fire on parent rebuild).
    let files_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_files")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        files_count, 2,
        "document_files rows must survive parent rebuild — naive DROP TABLE \
         would CASCADE-delete them despite defer_foreign_keys=ON"
    );

    // Specific file content preserved (verify holding-table round trip).
    let xml: Vec<u8> = sqlx::query_scalar(
        "SELECT content FROM document_files WHERE document_id = ? AND kind = 'PAYLOAD_XML'",
    )
    .bind(&id_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(xml, b"<xml>A</xml>");

    // foreign_key_check returns no rows when all FKs are satisfied.
    // PRAGMA foreign_key_check returns (table, rowid, parent, fkid)
    // tuples; an empty result set is the success condition.
    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        violations.is_empty(),
        "PRAGMA foreign_key_check must be clean post-rebuild; got {violations:?}"
    );

    // SENDING is accepted post-rebuild (proves CHECK extension landed).
    sqlx::query("UPDATE fiscal_documents SET state = 'SENDING' WHERE document_id = ?")
        .bind(&id_b)
        .execute(&pool)
        .await
        .unwrap();
}
