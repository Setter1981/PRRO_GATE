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
    let names: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1")
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
        assert!(
            names.contains(col),
            "fiscal_documents missing {col}; have {names:?}"
        );
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
    let names: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='index' ORDER BY 1")
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
    tx.commit()
        .await
        .expect("atomic rolling refresh must commit");
}

#[tokio::test]
async fn migration_004_offline_and_routing_tables_present() {
    let (_d, pool) = fresh_pool().await;
    let names: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY 1")
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
        assert!(
            names.contains(col),
            "transport_profiles missing {col}; have {names:?}"
        );
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
        assert!(
            names.contains(col),
            "licenses missing {col}; have {names:?}"
        );
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
    insert("basic", 1)
        .await
        .expect("first active license must insert");

    // Staged inactive license — same DB, must coexist with the active one.
    insert("pro", 0)
        .await
        .expect("staged inactive license must coexist");

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
async fn migration_001_rejects_non_digit_fiscal_number() {
    let (_d, pool) = fresh_pool().await;
    let err = sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1aaaaaaaaa', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .expect_err("non-digit fiscal_number must violate CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK error, got: {msg}"
    );
}

#[tokio::test]
async fn migration_001_rejects_non_digit_vat_payer_inn() {
    let (_d, pool) = fresh_pool().await;
    let err = sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, vat_payer_inn, fiscal_mode) \
         VALUES ('1234567890', '12345678', '1aaaaaaaaaaa', 'test')",
    )
    .execute(&pool)
    .await
    .expect_err("non-digit vat_payer_inn (right length) must violate CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK error, got: {msg}"
    );
    // Sanity: NULL vat_payer_inn is still allowed.
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, vat_payer_inn, fiscal_mode) \
         VALUES ('1234567890', '12345678', NULL, 'test')",
    )
    .execute(&pool)
    .await
    .expect("NULL vat_payer_inn must remain allowed");
}

#[tokio::test]
async fn migration_003_rejects_non_digit_operator_inn() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let err = sqlx::query(
        "INSERT INTO sidecar_operators(id, fiscal_number, operator_inn, jks_path, \
            jks_password_hex, cred_salt) \
         VALUES (X'00000000000000000000000000000001', '1234567890', '1aaaaaaaaa', \
            '/tmp/x.jks', 'deadbeef', X'00000000000000000000000000000001')",
    )
    .execute(&pool)
    .await
    .expect_err("non-digit operator_inn (right length) must violate CHECK");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK error, got: {msg}"
    );
}

#[tokio::test]
async fn migration_002_fiscal_documents_offline_session_fk_enforced() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let bogus_session = vec![0xAAu8; 16];
    let doc_id = vec![0x01u8; 16];
    let req_id = vec![0x02u8; 16];
    let sha = vec![0u8; 32];
    let err = sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, offline_session_id) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_id)
    .bind(&req_id)
    .bind(&sha)
    .bind(&bogus_session)
    .execute(&pool)
    .await
    .expect_err("non-existent offline_session_id must violate FK");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("foreign key"), "expected FK error, got: {msg}");
}

#[tokio::test]
async fn migration_002_fiscal_documents_related_receipt_self_fk_enforced() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let bogus_related = vec![0xBBu8; 16];
    let doc_id = vec![0x03u8; 16];
    let req_id = vec![0x04u8; 16];
    let sha = vec![0u8; 32];
    let err = sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, related_receipt_id) \
         VALUES (?, ?, '1234567890', 1, 'RETURN', 'PREPARED', 'b', 't', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_id)
    .bind(&req_id)
    .bind(&sha)
    .bind(&bogus_related)
    .execute(&pool)
    .await
    .expect_err("non-existent related_receipt_id must violate self-FK");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("foreign key"), "expected FK error, got: {msg}");
}

#[tokio::test]
async fn migration_001_offline_bounds_check_enforced() {
    let (_d, pool) = fresh_pool().await;
    // Negative min_offline_codes
    let err1 = sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode, min_offline_codes) \
         VALUES ('1234567890', '12345678', 'test', -1)",
    )
    .execute(&pool)
    .await
    .expect_err("negative min_offline_codes must be rejected");
    assert!(err1.to_string().to_lowercase().contains("check"));

    // max < min
    let err2 = sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode, \
            min_offline_codes, max_offline_codes) \
         VALUES ('2222222222', '12345678', 'test', 10, 5)",
    )
    .execute(&pool)
    .await
    .expect_err("max < min must be rejected");
    assert!(err2.to_string().to_lowercase().contains("check"));

    // Boundary case: max == min → allowed.
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode, \
            min_offline_codes, max_offline_codes) \
         VALUES ('3333333333', '12345678', 'test', 5, 5)",
    )
    .execute(&pool)
    .await
    .expect("max == min must be allowed");
}

#[tokio::test]
async fn migration_004_offline_codes_value_must_be_positive() {
    // M3b W4 (migration 015): post-normalization the column is
    // `code_lnd` not `code_value`; same CHECK predicate (> 0).
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    for bad in [0i64, -1, -1000] {
        let err = sqlx::query(
            "INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES ('1234567890', ?)",
        )
        .bind(bad)
        .execute(&pool)
        .await
        .expect_err("non-positive code_lnd must violate CHECK");
        assert!(
            err.to_string().to_lowercase().contains("check"),
            "bad={bad}"
        );
    }
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES ('1234567890', 1)")
        .execute(&pool)
        .await
        .expect("code_lnd = 1 must be allowed");
}

#[tokio::test]
async fn migration_002_delete_offline_session_blocked_by_doc_reference() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let session_id = vec![0xAAu8; 16];
    sqlx::query(
        // M3b W4 (migration 015): column renamed `status` → `state`.
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, '1234567890', 'OPEN', '2026-01-01T00:00:00Z')",
    )
    .bind(&session_id)
    .execute(&pool)
    .await
    .unwrap();

    let doc_id = vec![0x01u8; 16];
    let req_id = vec![0x02u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, offline_session_id) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_id)
    .bind(&req_id)
    .bind(&sha)
    .bind(&session_id)
    .execute(&pool)
    .await
    .unwrap();

    let err = sqlx::query("DELETE FROM offline_sessions WHERE offline_session_id = ?")
        .bind(&session_id)
        .execute(&pool)
        .await
        .expect_err("ON DELETE RESTRICT must block deletion while doc references session");
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("foreign key"), "expected FK error, got: {msg}");
}

#[tokio::test]
async fn migration_002_delete_related_receipt_blocked_by_self_referrer() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let original = vec![0x10u8; 16];
    let req_orig = vec![0x11u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'ACK', 'b', 't', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?)",
    )
    .bind(&original)
    .bind(&req_orig)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();

    let returnee = vec![0x20u8; 16];
    let req_ret = vec![0x21u8; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, related_receipt_id) \
         VALUES (?, ?, '1234567890', 2, 'RETURN', 'PREPARED', 'b', 't', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, ?)",
    )
    .bind(&returnee)
    .bind(&req_ret)
    .bind(&sha)
    .bind(&original)
    .execute(&pool)
    .await
    .unwrap();

    let err = sqlx::query("DELETE FROM fiscal_documents WHERE document_id = ?")
        .bind(&original)
        .execute(&pool)
        .await
        .expect_err(
            "ON DELETE RESTRICT must block deletion of original while RETURN references it",
        );
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("foreign key"), "expected FK error, got: {msg}");
}

#[tokio::test]
async fn migration_006_ca_endpoints_table_and_seed_present() {
    let (_d, pool) = fresh_pool().await;
    // Table exists with the expected column set.
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(ca_endpoints)")
            .fetch_all(&pool)
            .await
            .expect("ca_endpoints must exist post-006");
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in [
        "id",
        "name",
        "cmp_url",
        "issuer_pattern",
        "priority",
        "enabled",
        "created_at",
        "updated_at",
    ] {
        assert!(
            names.contains(col),
            "ca_endpoints missing {col}; have {names:?}"
        );
    }

    // Seed rows: exactly two production CMP URLs, ordered acskidd first
    // (priority 10), ca.tax.gov.ua second (priority 20).  Each MUST
    // include the `/services/cmp/` path component (M1's default lacked
    // it; W2's whole point is that ca_endpoints carries the correct
    // URLs).  `name` is asserted alongside `cmp_url` so the priority
    // contract is positionally explicit, not just substring-matched.
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT name, cmp_url, priority FROM ca_endpoints WHERE enabled = 1 ORDER BY priority",
    )
    .fetch_all(&pool)
    .await
    .expect("seed rows reachable");
    assert_eq!(
        rows.len(),
        2,
        "ca_endpoints seed must contain exactly 2 enabled rows; got {rows:?}"
    );
    assert_eq!(
        rows[0].0, "acskidd",
        "first-priority endpoint must be 'acskidd'; got {rows:?}"
    );
    assert_eq!(rows[0].2, 10, "acskidd priority must be 10; got {rows:?}");
    assert_eq!(
        rows[1].0, "ca.tax.gov.ua",
        "second-priority endpoint must be 'ca.tax.gov.ua'; got {rows:?}"
    );
    assert_eq!(
        rows[1].2, 20,
        "ca.tax.gov.ua priority must be 20; got {rows:?}"
    );
    for (_, url, _) in &rows {
        assert!(
            url.contains("/services/cmp/"),
            "all seeded ca_endpoints URLs must carry /services/cmp/ path; got {url}"
        );
    }

    // Partial index ix_ca_endpoints_priority must cover only enabled=1
    // rows (pre-existing pattern from the legacy schema; W5's static
    // check inspects index hygiene later).
    let index_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='ix_ca_endpoints_priority'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(index_count, 1, "ix_ca_endpoints_priority must exist");
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

// ─── M3b W4: migration 015 offline normalization ─────────────────────
//
// Operator review criteria (saved 2026-05-14 in
// memory `project_m3b_w4_review_criteria`) tested by the fixtures
// below.  Layout: 2 distinct "apply" fixtures (criterion #7) + 3
// invariant fixtures (criteria #4, #5, #6).

/// W4 criterion #1 + #7a — migration applies cleanly on a fresh DB
/// and the post-015 schema has the target shape: tables renamed,
/// new indices in place, immutability trigger registered.
#[tokio::test]
async fn migration_015_normalize_apply_fresh_db() {
    let (_d, pool) = fresh_pool().await;

    // Tables exist with new shape — query pragma to confirm columns.
    let session_cols: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('offline_sessions') ORDER BY cid")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        session_cols.contains(&"state".to_string()),
        "offline_sessions must have `state` column post-W4, got: {session_cols:?}"
    );
    assert!(
        !session_cols.contains(&"status".to_string()),
        "offline_sessions must NOT have legacy `status` column post-W4, got: {session_cols:?}"
    );
    assert!(
        session_cols.contains(&"drained_at".to_string()),
        "offline_sessions must have new `drained_at` column"
    );
    assert!(
        session_cols.contains(&"reason_abort".to_string()),
        "offline_sessions must have new `reason_abort` column"
    );

    let code_cols: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('offline_codes') ORDER BY cid")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        code_cols.contains(&"code_lnd".to_string()),
        "offline_codes must have `code_lnd` column post-W4, got: {code_cols:?}"
    );
    assert!(
        !code_cols.contains(&"code_value".to_string()),
        "offline_codes must NOT have legacy `code_value` column"
    );
    assert!(
        code_cols.contains(&"consumed_at".to_string()),
        "offline_codes must have `consumed_at` column"
    );
    assert!(
        !code_cols.contains(&"used_at".to_string()),
        "offline_codes must NOT have legacy `used_at` column"
    );
    assert!(
        code_cols.contains(&"consumed_by_document_id".to_string()),
        "offline_codes must have `consumed_by_document_id` column"
    );
    assert!(
        !code_cols.contains(&"used_by_doc".to_string()),
        "offline_codes must NOT have legacy `used_by_doc` column"
    );
    // No `consumed` bool — semantic is `consumed_at IS NULL`.
    assert!(
        !code_cols.contains(&"consumed".to_string()),
        "offline_codes must NOT have a `consumed` bool column; semantic is `consumed_at IS NULL`"
    );

    // Indices: partial UNIQUE on active sessions + partial UNIQUE on consumed-link.
    let index_names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='index' ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        index_names.contains(&"ux_offline_active".to_string()),
        "missing partial UNIQUE ux_offline_active"
    );
    assert!(
        index_names.contains(&"ux_offline_codes_consumed_by_doc".to_string()),
        "missing partial UNIQUE ux_offline_codes_consumed_by_doc"
    );
    assert!(
        index_names.contains(&"ix_offline_codes_available".to_string()),
        "missing ix_offline_codes_available (consumed_at IS NULL)"
    );

    // Trigger registered.
    let trigger_names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger' ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        trigger_names.contains(&"offline_codes_consumed_immutable".to_string()),
        "missing immutability trigger, got triggers: {trigger_names:?}"
    );
}

/// W4 criterion #7b + #1 + #2 — migration applies on a 004-era
/// populated DB without row loss; `CLOSING → DRAINING` semantic
/// transform applied; column renames preserve data.
///
/// Method: after `open_pool` (which has already applied 015),
/// drop the post-015 tables, recreate the 004-era shape, INSERT
/// representative rows (including `CLOSING` to exercise the value
/// transform), then re-run the 015 migration SQL inline.  Assert
/// (a) row counts preserved, (b) `CLOSING → DRAINING`, (c)
/// columns renamed, (d) `drained_at` back-filled for ex-CLOSING.
#[tokio::test]
async fn migration_015_normalize_apply_populated_db_preserves_rows() {
    let (_d, pool) = fresh_pool().await;

    // Reset to 004-era shape.
    sqlx::query("DROP TRIGGER IF EXISTS offline_codes_consumed_immutable")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE offline_codes")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE offline_sessions")
        .execute(&pool)
        .await
        .unwrap();

    // Re-create the 004-era shape verbatim (from migrations/004).
    sqlx::query(
        "CREATE TABLE offline_sessions (
            offline_session_id BLOB PRIMARY KEY CHECK (length(offline_session_id) = 16),
            fiscal_number      TEXT NOT NULL,
            status             TEXT NOT NULL CHECK (status IN ('OPENING','OPEN','CLOSING','CLOSED','ABORTED')),
            opened_at          TEXT NOT NULL,
            closed_at          TEXT,
            last_known_unsigned_xml_sha256 BLOB,
            docs_count         INTEGER NOT NULL DEFAULT 0,
            created_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            updated_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
        ) STRICT",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE offline_codes (
            fiscal_number TEXT NOT NULL,
            code_value    INTEGER NOT NULL CHECK (code_value > 0),
            used_at       TEXT,
            used_by_doc   BLOB,
            PRIMARY KEY (fiscal_number, code_value),
            FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
        ) STRICT",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Seed 5 FN configs — one per legacy status.  Use distinct FNs
    // because M3b's tightened `ux_offline_active` partial UNIQUE
    // permits at most one active (OPENING/OPEN/DRAINING) session
    // per FN; M3a runtime maintained this by convention without the
    // UNIQUE enforcement, but realistic pre-migration data MUST
    // respect it (otherwise the W4 migration fails fail-closed at
    // CREATE UNIQUE INDEX — operator's responsibility to clean up
    // pre-W4 violations before deploying W4).
    let session_seeds: &[(&[u8], &str, &str)] = &[
        (&[0xA1; 16], "1111111111", "OPENING"),
        (&[0xA2; 16], "2222222222", "OPEN"),
        (&[0xA3; 16], "3333333333", "CLOSING"), // ← maps to DRAINING
        (&[0xA4; 16], "4444444444", "CLOSED"),
        (&[0xA5; 16], "5555555555", "ABORTED"),
    ];
    for (_id, fn_id, _status) in session_seeds {
        sqlx::query(
            "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
             VALUES (?, '12345678', 'test')",
        )
        .bind(*fn_id)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Also seed the FN used below for offline_codes test data.
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    for (id, fn_id, status) in session_seeds {
        sqlx::query(
            "INSERT INTO offline_sessions(offline_session_id, fiscal_number, status, opened_at, updated_at) \
             VALUES (?, ?, ?, '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z')",
        )
        .bind(*id)
        .bind(*fn_id)
        .bind(*status)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Codes: 1 unused + 1 used.  Per FK requirement, used_by_doc
    // must reference an existing fiscal_documents row; create one.
    let doc_id = vec![0xC1u8; 16];
    let req_id = vec![0xC2u8; 16];
    let sha = vec![0u8; 32];
    // HIGH-fix coverage (operator review 2026-05-14): the
    // fiscal_documents row references one of the migrated
    // offline_sessions via `offline_session_id` FK.  This proves
    // the W4 migration is FK-safe — without `defer_foreign_keys =
    // ON` the DROP TABLE offline_sessions step would fail RESTRICT
    // on this reference.  We pick the FN+session that the seeder
    // labelled `OPEN` (FN `2222222222`, session id 0xA2…) so the
    // doc references an active session that survives the rename.
    let referenced_session_id: Vec<u8> = vec![0xA2u8; 16];
    // Pin `updated_at` to a far-historical value via explicit INSERT
    // column (INSERT does NOT fire the `fd_updated_at` AFTER-UPDATE
    // trigger, so the literal value is preserved verbatim).  W4
    // operator HIGH-finding 2026-05-15: migration's service-UPDATEs
    // on fiscal_documents must NOT mutate this timestamp.
    let pinned_updated_at = "2000-01-01T00:00:00Z";
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, offline_session_id, updated_at) \
         VALUES (?, ?, '2222222222', 1, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, ?, ?)",
    )
    .bind(&doc_id)
    .bind(&req_id)
    .bind(&sha)
    .bind(&referenced_session_id)
    .bind(pinned_updated_at)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_value) VALUES ('1234567890', 10)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_value, used_at, used_by_doc) \
         VALUES ('1234567890', 20, '2026-01-03T00:00:00Z', ?)",
    )
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();

    // Pre-migration counts.
    let sessions_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM offline_sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let codes_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions_before, 5);
    assert_eq!(codes_before, 2);

    // Apply migration 015 inline via `sqlx::raw_sql` wrapped in an
    // explicit `pool.begin()/tx.commit()`.  This MIRRORS production
    // `sqlx::migrate!` semantics: sqlx-sqlite 0.8.6's `Migrator::apply`
    // always wraps each migration's SQL in `self.begin().await` (see
    // sqlx-sqlite-0.8.6/src/migrate.rs:136).  The `-- no-transaction`
    // directive that sqlx-core 0.8.6 parses is NOT honoured by sqlx-
    // sqlite 0.8.6, so a migration that "needs autocommit" simply
    // doesn't have that option on SQLite.  Consequently the migration
    // MUST be safe inside an open transaction; this test proves it.
    //
    // The migration itself uses `PRAGMA defer_foreign_keys = ON` +
    // the NULL-the-FK-column trick + the 008-style DROP-and-recreate-
    // under-same-name pattern (NOT ALTER RENAME — RENAME breaks
    // deferred FK validation; documented in migration 008).  At the
    // end the migration runs a hard-abort FK guard via an INSERT
    // into a TEMP CHECK-constrained table seeded from
    // `pragma_foreign_key_check`; if any FK violation exists the
    // CHECK fails and the whole migration aborts with
    // SQLITE_CONSTRAINT_CHECK — fail-closed BEFORE COMMIT.
    let sql_015 = include_str!("../migrations/015_offline_normalize.sql");
    let mut tx = pool
        .begin()
        .await
        .expect("begin tx mirroring sqlx::migrate!");
    sqlx::raw_sql(sql_015).execute(&mut *tx).await.expect(
        "migration 015 must apply on populated 004-era DB inside a tx (mirrors sqlx::migrate!)",
    );
    tx.commit()
        .await
        .expect("commit tx — deferred FK validation must succeed");

    // Post-migration integrity scan: no orphan FK references.  The
    // migration's own hard-abort guard already enforces this inside
    // the tx (CHECK-table INSERT trick — see migration 015 tail);
    // this assertion is belt-and-braces, proving the wrapping COMMIT
    // also sees a clean state.
    let fk_check: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    assert!(
        fk_check.is_empty(),
        "post-migration foreign_key_check must be empty; got violations: {fk_check:?}"
    );

    // Post-migration assertions.
    let sessions_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM offline_sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let codes_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM offline_codes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions_after, 5, "row count must be preserved");
    assert_eq!(codes_after, 2, "row count must be preserved");

    // CLOSING → DRAINING transform applied.
    let draining_id: Vec<u8> = sqlx::query_scalar(
        "SELECT offline_session_id FROM offline_sessions WHERE state = 'DRAINING'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        draining_id,
        vec![0xA3u8; 16],
        "CLOSING row mapped to DRAINING"
    );
    let still_closing: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM offline_sessions WHERE state = 'CLOSING'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        still_closing, 0,
        "no row may remain in CLOSING post-migration"
    );

    // drained_at back-filled for ex-CLOSING row (criterion #1 forensics).
    let drained_at: Option<String> =
        sqlx::query_scalar("SELECT drained_at FROM offline_sessions WHERE offline_session_id = ?")
            .bind(&[0xA3u8; 16][..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        drained_at.is_some(),
        "drained_at must be back-filled for ex-CLOSING row"
    );

    // code_value → code_lnd; used_at → consumed_at.
    let consumed_for_doc: Option<i64> =
        sqlx::query_scalar("SELECT code_lnd FROM offline_codes WHERE consumed_by_document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        consumed_for_doc,
        Some(20),
        "used_by_doc → consumed_by_document_id rename"
    );
    let consumed_at: Option<String> =
        sqlx::query_scalar("SELECT consumed_at FROM offline_codes WHERE code_lnd = 20")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        consumed_at.as_deref(),
        Some("2026-01-03T00:00:00Z"),
        "used_at → consumed_at rename preserves value"
    );

    // HIGH-fix assertion (operator review 2026-05-14/15):
    // `fiscal_documents.offline_session_id` FK must still resolve
    // to the rebuilt (same-name) offline_sessions table after
    // migration.  Verifies that the migration's "stash + NULL the
    // inbound FK column → DROP+CREATE-same-name → restore the FK
    // column" recipe, combined with `PRAGMA defer_foreign_keys = ON`,
    // correctly preserved the FK targets through the rebuild.
    let resolved_session: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT s.offline_session_id \
         FROM fiscal_documents d \
         JOIN offline_sessions s ON s.offline_session_id = d.offline_session_id \
         WHERE d.document_id = ?",
    )
    .bind(&doc_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        resolved_session.as_deref(),
        Some(&[0xA2u8; 16][..]),
        "fiscal_documents.offline_session_id FK must resolve to the renamed offline_sessions row post-migration"
    );
    // Also verify the referenced session ended up in the expected
    // post-migration state (OPEN was a legal active session, so
    // it should still be OPEN; the migration value transform only
    // touches CLOSING).
    let session_state: String =
        sqlx::query_scalar("SELECT state FROM offline_sessions WHERE offline_session_id = ?")
            .bind(&[0xA2u8; 16][..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_state, "OPEN", "OPEN session must remain OPEN");

    // HIGH-fix assertion (operator review 2026-05-15, Round 4):
    // the migration's service-UPDATEs on fiscal_documents (stash +
    // restore inbound FK) must NOT mutate historical `updated_at`.
    // The pre-migration value `2000-01-01T00:00:00Z` was set via
    // explicit INSERT column (INSERT bypasses the `fd_updated_at`
    // AFTER-UPDATE trigger).  Migration 015 step 2a drops that
    // trigger before its UPDATEs and step 9b recreates it
    // identically — so this assertion proves the trigger
    // suppression is scoped correctly to the migration's
    // bookkeeping writes and historical metadata is preserved.
    let preserved_updated_at: String =
        sqlx::query_scalar("SELECT updated_at FROM fiscal_documents WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        preserved_updated_at, "2000-01-01T00:00:00Z",
        "W4 migration must NOT mutate fiscal_documents.updated_at — fd_updated_at trigger must be suppressed across the service-UPDATEs and restored after"
    );

    // Belt-and-braces: the trigger must exist again post-migration
    // so normal runtime UPDATEs on fiscal_documents still refresh
    // `updated_at` correctly.
    let trigger_present: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'fd_updated_at'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        trigger_present, 1,
        "fd_updated_at trigger must be recreated by migration 015 step 9b"
    );

    // Prove the recreated trigger still functions: a runtime UPDATE
    // on the doc (not via the migration) must refresh `updated_at`
    // away from the pinned historical value.  We update an
    // unrelated business column (`state`) to trigger the AFTER
    // UPDATE.  Use a column the doc actually permits transitioning
    // to from PREPARED; SIGNED is the canonical first transition.
    sqlx::query("UPDATE fiscal_documents SET state = 'SIGNED' WHERE document_id = ?")
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();
    let after_runtime_update: String =
        sqlx::query_scalar("SELECT updated_at FROM fiscal_documents WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(
        after_runtime_update, "2000-01-01T00:00:00Z",
        "post-migration runtime UPDATE on fiscal_documents must fire `fd_updated_at` (proves trigger was recreated functional)"
    );
}

/// W4 criterion #4 — partial UNIQUE `ux_offline_active` covers only
/// active states (OPENING/OPEN/DRAINING).  Closed/Aborted rows are
/// invisible to the index so they don't block a new active session
/// on the same FN.
#[tokio::test]
async fn migration_015_ux_offline_active_rejects_duplicate_active_only() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert a CLOSED session — should NOT block a subsequent active session.
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at, closed_at) \
         VALUES (?, '1234567890', 'CLOSED', '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z')",
    )
    .bind(&[0xC0u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, '1234567890', 'OPEN', '2026-02-01T00:00:00Z')",
    )
    .bind(&[0xAAu8; 16][..])
    .execute(&pool)
    .await
    .expect("CLOSED row must NOT block a new active session on the same FN");

    // ABORTED also must NOT count as active.
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, '1234567890', 'ABORTED', '2026-03-01T00:00:00Z')",
    )
    .bind(&[0xBBu8; 16][..])
    .execute(&pool)
    .await
    .expect("ABORTED row must coexist with OPEN row on same FN");

    // Second OPEN on same FN: must fail (UNIQUE violation).
    let err = sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, '1234567890', 'OPEN', '2026-04-01T00:00:00Z')",
    )
    .bind(&[0xDDu8; 16][..])
    .execute(&pool)
    .await
    .expect_err("second OPEN on same FN must violate ux_offline_active");
    assert!(
        err.to_string().to_lowercase().contains("unique"),
        "expected UNIQUE error, got: {err}"
    );

    // Same FN: OPENING also blocked by existing OPEN (any active state combo).
    let err2 = sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, '1234567890', 'OPENING', '2026-05-01T00:00:00Z')",
    )
    .bind(&[0xEEu8; 16][..])
    .execute(&pool)
    .await
    .expect_err("OPENING vs existing OPEN on same FN must violate ux_offline_active");
    assert!(err2.to_string().to_lowercase().contains("unique"));
}

/// W4 criterion #5 — consumed-link uniqueness + immutability trigger.
/// Verifies all three boundaries:
///   (a) legal first consume (NULL → first-doc) is allowed;
///   (b) re-attribution (first-doc → other-doc) is blocked;
///   (c) un-consume (first-doc → NULL) is blocked.
/// Also (d): `ux_offline_codes_consumed_by_doc` blocks two different
/// code rows from linking to the same document.
#[tokio::test]
async fn migration_015_consumed_link_uniqueness_and_immutability_trigger() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create two distinct docs to attribute codes to.
    let doc_a = vec![0xA0u8; 16];
    let doc_b = vec![0xB0u8; 16];
    for (doc_id, req_byte) in [(&doc_a, 0xA1u8), (&doc_b, 0xB1u8)] {
        let req_id = vec![req_byte; 16];
        let sha = vec![0u8; 32];
        sqlx::query(
            "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
                state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
                payload_sha256_canonical) \
             VALUES (?, ?, '1234567890', ?, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
                '2026-01-01T00:00:00Z', '{}', ?)",
        )
        .bind(doc_id)
        .bind(&req_id)
        .bind(req_byte as i64)
        .bind(&sha)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Seed two code rows; pre-consume.
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES ('1234567890', 1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES ('1234567890', 2)")
        .execute(&pool)
        .await
        .unwrap();

    // (a) First consume: NULL → doc_a.  Trigger MUST allow this
    // because OLD.consumed_at IS NULL (the trigger gates on
    // OLD.consumed_at IS NOT NULL).
    sqlx::query(
        "UPDATE offline_codes SET consumed_at = '2026-01-01T01:00:00Z', \
            consumed_by_document_id = ? \
         WHERE fiscal_number = '1234567890' AND code_lnd = 1",
    )
    .bind(&doc_a)
    .execute(&pool)
    .await
    .expect("first consume (NULL → doc_a) must be allowed");

    // (b) Re-attribution: doc_a → doc_b.  Trigger MUST block.
    let err_reattr = sqlx::query(
        "UPDATE offline_codes SET consumed_by_document_id = ? \
         WHERE fiscal_number = '1234567890' AND code_lnd = 1",
    )
    .bind(&doc_b)
    .execute(&pool)
    .await
    .expect_err("re-attribution of consumed code must be blocked by trigger");
    let msg_reattr = err_reattr.to_string().to_lowercase();
    assert!(
        msg_reattr.contains("immutable") || msg_reattr.contains("admin repair"),
        "expected immutability error, got: {msg_reattr}"
    );

    // (c) Un-consume: consumed_at → NULL.  Trigger MUST block.
    let err_unconsume = sqlx::query(
        "UPDATE offline_codes SET consumed_at = NULL \
         WHERE fiscal_number = '1234567890' AND code_lnd = 1",
    )
    .execute(&pool)
    .await
    .expect_err("un-consume must be blocked by trigger");
    let msg_unconsume = err_unconsume.to_string().to_lowercase();
    assert!(
        msg_unconsume.contains("immutable") || msg_unconsume.contains("admin repair"),
        "expected immutability error, got: {msg_unconsume}"
    );

    // (c2) Timestamp mutation: consumed_at non-NULL → different
    // non-NULL.  Trigger MUST block (MED fix, operator review
    // 2026-05-14 — pre-fix the trigger only caught NEW IS NULL,
    // allowing silent timestamp rewrites if doc-id was unchanged).
    let err_ts_mutate = sqlx::query(
        "UPDATE offline_codes SET consumed_at = '2099-12-31T23:59:59Z' \
         WHERE fiscal_number = '1234567890' AND code_lnd = 1",
    )
    .execute(&pool)
    .await
    .expect_err("consumed_at timestamp mutation must be blocked by trigger");
    let msg_ts = err_ts_mutate.to_string().to_lowercase();
    assert!(
        msg_ts.contains("immutable") || msg_ts.contains("admin repair"),
        "expected immutability error on timestamp mutation, got: {msg_ts}"
    );

    // (c3) No-op UPDATE with same consumed_at value: trigger does
    // NOT fire (idempotent replay).  Verifies the `IS NOT same`
    // condition correctly excludes no-ops.
    sqlx::query(
        "UPDATE offline_codes SET consumed_at = '2026-01-01T01:00:00Z' \
         WHERE fiscal_number = '1234567890' AND code_lnd = 1",
    )
    .execute(&pool)
    .await
    .expect("no-op UPDATE (same consumed_at) must succeed for idempotent replay");

    // (d) Link uniqueness: another code row trying to link to doc_a
    // (which is already linked to code_lnd=1) must violate the
    // partial UNIQUE ux_offline_codes_consumed_by_doc.  Since
    // code_lnd=2 has OLD.consumed_at IS NULL, the trigger DOES NOT
    // block this UPDATE; the partial UNIQUE index is the guard.
    let err_dup_link = sqlx::query(
        "UPDATE offline_codes SET consumed_at = '2026-01-01T02:00:00Z', \
            consumed_by_document_id = ? \
         WHERE fiscal_number = '1234567890' AND code_lnd = 2",
    )
    .bind(&doc_a)
    .execute(&pool)
    .await
    .expect_err("second code linking to doc_a must violate ux_offline_codes_consumed_by_doc");
    let msg_dup = err_dup_link.to_string().to_lowercase();
    assert!(
        msg_dup.contains("unique"),
        "expected UNIQUE error on second link, got: {msg_dup}"
    );
}

/// W4 criterion #6 — FK on `consumed_by_document_id` references
/// `fiscal_documents.document_id` with ON DELETE RESTRICT.
/// Attempting to DELETE the referenced doc must be rejected.
#[tokio::test]
async fn migration_015_consumed_by_document_id_fk_restrict() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let doc_id = vec![0xD0u8; 16];
    let req_id = vec![0xD1u8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
            '2026-01-01T00:00:00Z', '{}', ?)",
    )
    .bind(&doc_id)
    .bind(&req_id)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();

    // Insert an unconsumed code, then mark it consumed pointing at doc_id.
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES ('1234567890', 1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE offline_codes SET consumed_at = CURRENT_TIMESTAMP, consumed_by_document_id = ? \
         WHERE fiscal_number = '1234567890' AND code_lnd = 1",
    )
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();

    // DELETE fiscal_documents row referenced by a code → FK RESTRICT must reject.
    let err = sqlx::query("DELETE FROM fiscal_documents WHERE document_id = ?")
        .bind(&doc_id)
        .execute(&pool)
        .await
        .expect_err(
            "ON DELETE RESTRICT must block deletion of fiscal_documents row \
             referenced by offline_codes.consumed_by_document_id",
        );
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("foreign key"), "expected FK error, got: {msg}");
}

/// W4 LOW-fix (operator review 2026-05-15, Round 4) — negative
/// fixture for the migration's hard-abort FK guard.
///
/// The guard at the tail of `015_offline_normalize.sql`:
///
/// ```sql
/// CREATE TEMP TABLE __w4_fk_guard__ (
///     fk_violation_count INTEGER NOT NULL CHECK (fk_violation_count = 0)
/// );
/// INSERT INTO __w4_fk_guard__ (fk_violation_count)
/// SELECT 1 FROM pragma_foreign_key_check;
/// DROP TABLE __w4_fk_guard__;
/// ```
///
/// is load-bearing fail-closed: if `pragma_foreign_key_check`
/// returns ANY violations, the INSERT puts at least one row with
/// value `1` into the CHECK-constrained table; CHECK fails with
/// `SQLITE_CONSTRAINT_CHECK` and the entire migration aborts.
///
/// Test methodology: reset to 004-era shape, seed FN + a
/// fiscal_documents row, then INSERT an offline_codes row whose
/// `used_by_doc` references a NON-EXISTENT document (orphan FK).
/// This is only possible with `PRAGMA foreign_keys = OFF` in
/// autocommit — the 004 schema DOES enforce the FK at INSERT time
/// when foreign_keys is ON, so we must disable enforcement to
/// engineer a pre-migration violation.  Then we apply migration
/// 015 inside `pool.begin()/tx.commit()` (production semantics)
/// and assert it fails with a CHECK constraint error originating
/// from the guard.
#[tokio::test]
async fn migration_015_hard_abort_fk_guard_rejects_orphan_consumed_by_doc() {
    let (_d, pool) = fresh_pool().await;

    // Reset to 004-era shape (same approach as the populated-DB test).
    sqlx::query("DROP TRIGGER IF EXISTS offline_codes_consumed_immutable")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE offline_codes")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE offline_sessions")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE offline_sessions (
            offline_session_id BLOB PRIMARY KEY CHECK (length(offline_session_id) = 16),
            fiscal_number      TEXT NOT NULL,
            status             TEXT NOT NULL CHECK (status IN ('OPENING','OPEN','CLOSING','CLOSED','ABORTED')),
            opened_at          TEXT NOT NULL,
            closed_at          TEXT,
            last_known_unsigned_xml_sha256 BLOB,
            docs_count         INTEGER NOT NULL DEFAULT 0,
            created_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            updated_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
        ) STRICT",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE offline_codes (
            fiscal_number TEXT NOT NULL,
            code_value    INTEGER NOT NULL CHECK (code_value > 0),
            used_at       TEXT,
            used_by_doc   BLOB,
            PRIMARY KEY (fiscal_number, code_value),
            FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
        ) STRICT",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Seed FN config.
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Engineer the orphan: bypass FK enforcement and insert a code
    // referencing a non-existent document_id.  `PRAGMA foreign_keys
    // = OFF` works in autocommit (not inside a tx — confirmed
    // through sqlx source inspection); we are in autocommit here.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    let orphan_doc_id: Vec<u8> = vec![0xDEu8; 16]; // does NOT exist in fiscal_documents
    sqlx::query(
        "INSERT INTO offline_codes(fiscal_number, code_value, used_at, used_by_doc) \
         VALUES ('1234567890', 99, '2026-01-01T00:00:00Z', ?)",
    )
    .bind(&orphan_doc_id)
    .execute(&pool)
    .await
    .expect("INSERT must succeed with foreign_keys=OFF");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    // Note: pre-migration `PRAGMA foreign_key_check` returns
    // EMPTY here.  The 004-era `offline_codes` schema declares NO
    // FK on `used_by_doc` (only on `fiscal_number`).  The W4
    // rebuild adds `FOREIGN KEY (consumed_by_document_id)
    // REFERENCES fiscal_documents(document_id)` — that's the FK
    // that makes the orphan visible.  After step 11–14 of 015
    // (snapshot → drop → recreate with new FK → INSERT…SELECT, all
    // under defer_foreign_keys=ON), the orphan row exists in the
    // rebuilt offline_codes with `consumed_by_document_id =
    // 0xDE…` pointing at a non-existent document.  THAT's what the
    // hard-abort guard at the tail catches via
    // `pragma_foreign_key_check`.

    // Apply migration 015 inside a tx (mirrors sqlx::migrate!).
    // EXPECT failure at the hard-abort guard.
    let sql_015 = include_str!("../migrations/015_offline_normalize.sql");
    let mut tx = pool.begin().await.expect("begin tx");
    let err = sqlx::raw_sql(sql_015).execute(&mut *tx).await.expect_err(
        "migration 015 must fail at the hard-abort FK guard when an orphan \
         consumed_by_document_id is present",
    );
    // The SQLite error originates from the CHECK constraint on the
    // `__w4_fk_guard__` temp table.  Accept any error wording that
    // identifies a CHECK constraint or the guard table (sqlx may
    // wrap SQLite errors differently across versions; match
    // loosely on either signal).
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("__w4_fk_guard__") || msg.contains("constraint"),
        "expected CHECK/constraint error from hard-abort FK guard, got: {msg}"
    );
    // Tx is dropped (rolled back) — the orphan row remains in
    // pre-migration shape (004-era), and the migration's table
    // rewrites are undone.  No assertion needed on post-state; the
    // important property is that the migration refused to commit.
    drop(tx);
}
