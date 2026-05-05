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
            "INSERT INTO offline_codes(fiscal_number, code_value) VALUES ('1234567890', ?)",
        )
        .bind(bad)
        .execute(&pool)
        .await
        .expect_err("non-positive code_value must violate CHECK");
        assert!(
            err.to_string().to_lowercase().contains("check"),
            "bad={bad}"
        );
    }
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_value) VALUES ('1234567890', 1)")
        .execute(&pool)
        .await
        .expect("code_value = 1 must be allowed");
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
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, status, opened_at) \
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
