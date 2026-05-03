//! One verification path for all M1 migrations: apply via `sqlx::migrate!`,
//! assert table/index set, and prove STRICT typing rejects bad inserts.
//!
//! Runs against the bundled libsqlite3-sys (NOT the system `sqlite3` CLI),
//! which is the runtime SQLite the gateway will actually use.

use std::collections::HashSet;

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    (dir, pool)
}

#[tokio::test]
async fn migration_001_creates_core_tables() {
    let (_d, pool) = fresh_pool().await;
    let names: HashSet<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    for t in ["fiscal_number_config", "shifts", "node_state", "audit_log"] {
        assert!(names.contains(t), "missing table {t}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_002_fiscal_documents_carries_both_hash_columns() {
    let (_d, pool) = fresh_pool().await;
    // PRAGMA table_info returns rows: (cid, name, type, notnull, dflt, pk).
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(fiscal_documents)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in [
        "payload_sha256_canonical",
        "unsigned_xml_sha256",
        "submission_attempted_at",
    ] {
        assert!(names.contains(col), "fiscal_documents missing {col}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_002_ingress_inbox_has_unique_idempotency_index() {
    let (_d, pool) = fresh_pool().await;
    // Confirm the unique index exists.
    let idx: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND name='ux_inbox_fn_idem'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(idx.as_deref(), Some("ux_inbox_fn_idem"));

    // Behavioural confirmation: duplicate (fn, idem_key) is rejected.
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let req1 = vec![1u8; 16];
    let req2 = vec![2u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical) \
         VALUES (?, '1234567890', 'REST', 'sell', 'idem-1', '{}', ?)",
    )
    .bind(&req1)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();

    let dup_err = sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical) \
         VALUES (?, '1234567890', 'REST', 'sell', 'idem-1', '{}', ?)",
    )
    .bind(&req2)
    .bind(&sha)
    .execute(&pool)
    .await
    .expect_err("duplicate (fn, idem_key) must violate UNIQUE");
    let msg = dup_err.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected UNIQUE constraint error, got: {msg}"
    );
}

#[tokio::test]
async fn migration_003_partial_active_indexes_present() {
    let (_d, pool) = fresh_pool().await;
    let names: HashSet<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    for idx in ["ux_op_fn_inn_active", "ux_op_certs_active_per_fn"] {
        assert!(names.contains(idx), "missing index {idx}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_003_operator_certs_supports_rolling_refresh() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let ski_a = "a".repeat(64);
    let ski_b = "b".repeat(64);
    let der = vec![0u8; 4];

    // Stage cert A as active.
    sqlx::query(
        "INSERT INTO operator_certs(ski_hex, fiscal_number, cert_fingerprint, cert_der, \
            fetched_at, source, active) VALUES (?, '1234567890', 'fp-a', ?, '2026-01-01T00:00:00Z', 'manual', 1)",
    )
    .bind(&ski_a)
    .bind(&der)
    .execute(&pool)
    .await
    .expect("first active cert insert");

    // Stage cert B as inactive — same FN, different SKI: must succeed.
    sqlx::query(
        "INSERT INTO operator_certs(ski_hex, fiscal_number, cert_fingerprint, cert_der, \
            fetched_at, source, active) VALUES (?, '1234567890', 'fp-b', ?, '2026-02-01T00:00:00Z', 'manual', 0)",
    )
    .bind(&ski_b)
    .bind(&der)
    .execute(&pool)
    .await
    .expect("second cert with active=0 must coexist with active cert A");

    // Flipping B to active=1 while A is still active must violate the partial unique idx.
    let collision = sqlx::query("UPDATE operator_certs SET active = 1 WHERE ski_hex = ?")
        .bind(&ski_b)
        .execute(&pool)
        .await
        .expect_err("two active certs per FN must violate ux_op_certs_active_per_fn");
    let msg = collision.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected UNIQUE-constraint error, got: {msg}"
    );

    // Atomic rolling-refresh: deactivate A then activate B in one tx — must succeed.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE operator_certs SET active = 0 WHERE ski_hex = ?")
        .bind(&ski_a)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE operator_certs SET active = 1 WHERE ski_hex = ?")
        .bind(&ski_b)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.expect("atomic rolling refresh must commit");
}

#[tokio::test]
async fn migration_004_offline_and_routing_tables_present() {
    let (_d, pool) = fresh_pool().await;
    let names: HashSet<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .collect();
    for t in [
        "offline_sessions",
        "offline_codes",
        "backend_profiles",
        "transport_profiles",
        "prro_bindings",
    ] {
        assert!(names.contains(t), "missing table {t}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_004_transport_profiles_carries_channel_kind_and_test_mode() {
    let (_d, pool) = fresh_pool().await;
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(transport_profiles)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in ["channel_kind", "test_mode"] {
        assert!(names.contains(col), "transport_profiles missing {col}; have {names:?}");
    }

    // Behavioural: an invalid channel_kind must be rejected by the CHECK.
    let err = sqlx::query(
        "INSERT INTO transport_profiles(transport_profile_id, name, channel_kind, test_mode) \
         VALUES ('tp-bad', 'bad', 'no_such_kind', 0)",
    )
    .execute(&pool)
    .await
    .expect_err("CHECK on channel_kind must reject unknown values");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK-constraint error, got: {msg}"
    );
}

#[tokio::test]
async fn migration_005_licenses_carries_required_columns() {
    let (_d, pool) = fresh_pool().await;
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(licenses)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in ["tier", "expires_at", "payload_b64", "signature_b64"] {
        assert!(names.contains(col), "licenses missing {col}; have {names:?}");
    }
}

#[tokio::test]
async fn migration_005_at_most_one_active_license() {
    let (_d, pool) = fresh_pool().await;

    let insert = |tier: &'static str, active: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO licenses(tin, fn_numbers_json, issued_at, expires_at, tier, \
                    payload_b64, signature_b64, active) \
                 VALUES ('12345678', '[]', '2026-01-01T00:00:00Z', '2027-01-01T00:00:00Z', \
                    ?, 'p', 's', ?)",
            )
            .bind(tier)
            .bind(active)
            .execute(&pool)
            .await
        }
    };

    // First active license — OK.
    insert("basic", 1).await.expect("first active license must insert");

    // Staged inactive license — same DB, must coexist with the active one.
    insert("pro", 0).await.expect("staged inactive license must coexist");

    // Second active=1 — must violate ux_lic_active.
    let collision = insert("enterprise", 1)
        .await
        .expect_err("two active licenses must violate ux_lic_active");
    let msg = collision.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected UNIQUE constraint error, got: {msg}"
    );

    // Atomic upgrade: deactivate old + activate staged in one tx — must succeed.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE licenses SET active = 0 WHERE tier = 'basic'")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE licenses SET active = 1 WHERE tier = 'pro'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.expect("atomic license swap must commit");
}

#[tokio::test]
async fn migration_001_strict_typing_rejects_text_in_int_column() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .expect("baseline insert");

    let err = sqlx::query(
        "UPDATE fiscal_number_config SET tsp_enabled = 'abc' WHERE fiscal_number = '1234567890'",
    )
    .execute(&pool)
    .await
    .expect_err("STRICT must reject TEXT in INTEGER column");

    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("integer") || msg.contains("strict") || msg.contains("type"),
        "expected STRICT/type-mismatch error, got: {msg}"
    );
}
