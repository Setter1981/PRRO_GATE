//! Targeted verification for migration 016 (M3b shift state expansion,
//! W14a-1 scope).
//!
//! NOTE (2026-06 baseline squash): the incremental-step test
//! `migration_016_preserves_fd_updated_at_during_w4_dance` (it replayed
//! versions ≤15 then 016 via `Migrator::iter()` to prove the W4 trigger dance)
//! was removed — after the squash that incremental step is unreachable by
//! construction.  The pre-squash chain AND its tests live at git ref
//! 5c6b00a3a9895fd634c322d02dc6c3d925dfcc4b (recover via `git show`).  The
//! surviving tests below assert the resulting (baseline) schema/behaviour.
//!
//! Covers the contracts the migration installs per spec §9.2 + §16.17:
//!
//!   1. shifts.state CHECK extended to 9 values (3 new: OPENED_LOCAL_PENDING_DRAIN,
//!      CLOSING_LOCAL_PENDING_DRAIN, REQUIRES_MANUAL_RECONCILIATION).
//!   2. node_state.shift_state CHECK mirrors the same 9-value list.
//!   3. shifts.opened_by_cashier_id + shifts.closed_by_cashier_id columns
//!      present as TEXT NULLABLE (no FK in W14a-1 — promoted in W14a-2 once
//!      1-cashier-per-shift enforcement lands per §16.8).
//!   4. operator_certs.deferred INTEGER NOT NULL DEFAULT 0 + partial unique
//!      index ux_op_certs_deferred_per_fn — supports 36h key-expiry SHIFT_OPEN
//!      gate auto-swap per §16.10.
//!   5. **fd_updated_at trigger suppression** during the W4 NULL-FK dance on
//!      fiscal_documents.shift_id — historical updated_at timestamps MUST be
//!      preserved verbatim when migration's bookkeeping UPDATEs run.  This
//!      is the load-bearing W4 lesson (per 015:96-104 + spec §9.2 step 11).
//!   6. Triggers (fd_updated_at, shifts_updated_at, node_state_updated_at) all
//!      recreated byte-identically post-rebuild.
//!   7. Existing 6 state values (CREATED / OPENING / OPENED / CLOSING / CLOSED /
//!      ERROR) still accepted (forward-compat: identity map for pre-pilot rows).

use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations including 016");
    (dir, pool)
}

async fn seed_fn(pool: &SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
}

// ─── (1) shifts.state CHECK accepts new states ───────────────────────

#[tokio::test]
async fn migration_016_accepts_three_new_shift_states() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000000001").await;

    for (i, state) in [
        "OPENED_LOCAL_PENDING_DRAIN",
        "CLOSING_LOCAL_PENDING_DRAIN",
        "REQUIRES_MANUAL_RECONCILIATION",
    ]
    .iter()
    .enumerate()
    {
        let shift_id = vec![0xA0 + i as u8; 16];
        sqlx::query(
            "INSERT INTO shifts(shift_id, fiscal_number, state, open_mode, opened_by_cashier_id) \
             VALUES (?, '9000000001', ?, 'OFFLINE', 'test-cashier')",
        )
        .bind(&shift_id)
        .bind(*state)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("migration 016 must accept state {state}: {e}"));
        // RS-3 C2: remove before the next insert — the active-shift uniqueness
        // index (migration 023) forbids two ACTIVE shifts for one FN; this test
        // only verifies each state's CHECK acceptance, one at a time.
        sqlx::query("DELETE FROM shifts WHERE shift_id = ?")
            .bind(&shift_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}

// ─── (2) shifts.state CHECK rejects bogus values ─────────────────────

#[tokio::test]
async fn migration_016_rejects_bogus_shift_state() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000000002").await;
    let shift_id = vec![0xB0u8; 16];

    let res = sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, state, open_mode) \
         VALUES (?, '9000000002', 'BOGUS_STATE', 'ONLINE')",
    )
    .bind(&shift_id)
    .execute(&pool)
    .await;

    let err = res.expect_err("CHECK must reject unknown shift state");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK-class error, got: {msg}"
    );
}

// ─── (3) Existing 6 state values still accepted (identity map) ───────

#[tokio::test]
async fn migration_016_preserves_existing_six_shift_states() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000000003").await;

    for (i, state) in ["CREATED", "OPENING", "OPENED", "CLOSING", "CLOSED", "ERROR"]
        .iter()
        .enumerate()
    {
        let shift_id = vec![0xC0 + i as u8; 16];
        sqlx::query(
            "INSERT INTO shifts(shift_id, fiscal_number, state, open_mode, opened_by_cashier_id) \
             VALUES (?, '9000000003', ?, 'ONLINE', 'test-cashier')",
        )
        .bind(&shift_id)
        .bind(*state)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("migration 016 must preserve state {state}: {e}"));
        // RS-3 C2: remove before the next insert — the active-shift uniqueness
        // index (migration 023) forbids two ACTIVE shifts for one FN; this test
        // only verifies each state's CHECK acceptance, one at a time.
        sqlx::query("DELETE FROM shifts WHERE shift_id = ?")
            .bind(&shift_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}

// ─── (4) node_state.shift_state CHECK mirrors shifts.state ───────────

#[tokio::test]
async fn migration_016_node_state_check_accepts_nine_values() {
    let (_d, pool) = fresh_pool().await;

    for (i, state) in [
        "CREATED",
        "OPENING",
        "OPENED_LOCAL_PENDING_DRAIN",
        "OPENED",
        "CLOSING_LOCAL_PENDING_DRAIN",
        "CLOSING",
        "CLOSED",
        "REQUIRES_MANUAL_RECONCILIATION",
        "ERROR",
    ]
    .iter()
    .enumerate()
    {
        let fn_id = format!("90000000{i:02}");
        seed_fn(&pool, &fn_id).await;
        sqlx::query(
            "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
             VALUES (?, 'ONLINE', ?, 1)",
        )
        .bind(&fn_id)
        .bind(*state)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("node_state CHECK must accept {state}: {e}"));
    }
}

// ─── (5) New cashier columns present + nullable ──────────────────────

#[tokio::test]
async fn migration_016_shifts_has_new_cashier_columns() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000001000").await;
    let shift_id = vec![0xD0u8; 16];

    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, state, open_mode, opened_by_cashier_id) \
         VALUES (?, '9000001000', 'OPENED', 'ONLINE', 'cashier-vasya')",
    )
    .bind(&shift_id)
    .execute(&pool)
    .await
    .expect("opened_by_cashier_id column must accept TEXT");

    let (opened, closed): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT opened_by_cashier_id, closed_by_cashier_id FROM shifts WHERE shift_id = ?",
    )
    .bind(&shift_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(opened.as_deref(), Some("cashier-vasya"));
    assert_eq!(closed, None, "closed_by_cashier_id must default NULL");
}

// ─── (6) cashier_certs table — cashier-scoped primary + deferred binding ───
// Per PR #65 R1 H2 fix: replaced earlier FN-scoped operator_certs.deferred
// boolean with proper cashier-scoped cashier_certs table per spec §16.17.

async fn seed_op_cert(pool: &SqlitePool, fn_id: &str, ski_hex: &str) {
    sqlx::query(
        "INSERT INTO operator_certs(ski_hex, fiscal_number, cert_fingerprint, cert_der, \
            fetched_at, source, active) \
         VALUES (?, ?, 'fp', X'AB', '2026-05-17T00:00:00Z', 'manual', 0)",
    )
    .bind(ski_hex)
    .bind(fn_id)
    .execute(pool)
    .await
    .expect("seed operator_cert");
}

#[tokio::test]
async fn migration_016_cashier_certs_table_accepts_primary_and_deferred() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000002000").await;
    let primary_ski = "a".repeat(64);
    let deferred_ski = "b".repeat(64);
    seed_op_cert(&pool, "9000002000", &primary_ski).await;
    seed_op_cert(&pool, "9000002000", &deferred_ski).await;

    sqlx::query(
        "INSERT INTO cashier_certs(cashier_id, fiscal_number, primary_cert_ski_hex, deferred_cert_ski_hex) \
         VALUES ('cashier-vasya', '9000002000', ?, ?)",
    )
    .bind(&primary_ski)
    .bind(&deferred_ski)
    .execute(&pool)
    .await
    .expect("primary + deferred binding must be accepted");

    let (got_primary, got_deferred): (String, Option<String>) = sqlx::query_as(
        "SELECT primary_cert_ski_hex, deferred_cert_ski_hex FROM cashier_certs \
         WHERE cashier_id = 'cashier-vasya' AND fiscal_number = '9000002000'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(got_primary, primary_ski);
    assert_eq!(got_deferred.as_deref(), Some(deferred_ski.as_str()));
}

#[tokio::test]
async fn migration_016_cashier_certs_deferred_ski_unique_across_bindings() {
    // Same physical deferred cert MUST NOT bind to two different (cashier,
    // FN) tuples — defence against accidental dual-binding per §16.17.
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000003000").await;
    let p1 = "c".repeat(64);
    let p2 = "d".repeat(64);
    let deferred = "e".repeat(64);
    seed_op_cert(&pool, "9000003000", &p1).await;
    seed_op_cert(&pool, "9000003000", &p2).await;
    seed_op_cert(&pool, "9000003000", &deferred).await;

    sqlx::query(
        "INSERT INTO cashier_certs(cashier_id, fiscal_number, primary_cert_ski_hex, deferred_cert_ski_hex) \
         VALUES ('cashier-A', '9000003000', ?, ?)",
    )
    .bind(&p1)
    .bind(&deferred)
    .execute(&pool)
    .await
    .unwrap();

    // Second binding referring to the same deferred SKI — must violate the
    // ux_cashier_certs_deferred_ski partial unique index.
    let res = sqlx::query(
        "INSERT INTO cashier_certs(cashier_id, fiscal_number, primary_cert_ski_hex, deferred_cert_ski_hex) \
         VALUES ('cashier-B', '9000003000', ?, ?)",
    )
    .bind(&p2)
    .bind(&deferred)
    .execute(&pool)
    .await;
    let err = res.expect_err("ux_cashier_certs_deferred_ski must block dual-bind of same deferred");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("constraint"),
        "expected UNIQUE-class error, got: {msg}"
    );
}

// ─── PR #65 R2 H2 fix — cross-FN cert ownership enforced at schema ──

#[tokio::test]
async fn migration_016_cashier_certs_composite_fk_rejects_cross_fn_binding() {
    // Setup: cert physically belongs to FN_A; attempt to bind it as
    // primary for cashier@FN_B must fail composite FK validation per
    // §16.10 same-FN cert ownership invariant.
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000010001").await; // FN_A
    seed_fn(&pool, "9000010002").await; // FN_B
    let cert_ski = "f".repeat(64);
    seed_op_cert(&pool, "9000010001", &cert_ski).await; // cert owned by FN_A

    let res = sqlx::query(
        "INSERT INTO cashier_certs(cashier_id, fiscal_number, primary_cert_ski_hex) \
         VALUES ('cashier-X', '9000010002', ?)",
    )
    .bind(&cert_ski)
    .execute(&pool)
    .await;
    let err = res.expect_err("composite FK must block cross-FN cert binding");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("foreign") || msg.contains("constraint"),
        "expected FK-class error, got: {msg}"
    );
}

#[tokio::test]
async fn migration_016_cashier_certs_check_rejects_same_primary_and_deferred() {
    // Setup: cert exists at FN_A; attempt to bind it as BOTH primary
    // and deferred for the same cashier — must fail same-cert CHECK
    // per §16.10 (deferred IS NULL OR deferred != primary).
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, "9000011000").await;
    let cert_ski = "9".repeat(64);
    seed_op_cert(&pool, "9000011000", &cert_ski).await;

    let res = sqlx::query(
        "INSERT INTO cashier_certs(cashier_id, fiscal_number, primary_cert_ski_hex, deferred_cert_ski_hex) \
         VALUES ('cashier-Y', '9000011000', ?, ?)",
    )
    .bind(&cert_ski)
    .bind(&cert_ski)
    .execute(&pool)
    .await;
    let err = res.expect_err("CHECK must block primary == deferred (degenerate self-swap)");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("check") || msg.contains("constraint"),
        "expected CHECK-class error, got: {msg}"
    );
}

// ─── (7) Triggers preserved byte-identical post-rebuild ──────────────

#[tokio::test]
async fn migration_016_triggers_preserved_post_rebuild() {
    let (_d, pool) = fresh_pool().await;

    let expected_triggers = [
        (
            "fd_updated_at",
            "CREATE TRIGGER fd_updated_at\nAFTER UPDATE ON fiscal_documents\nBEGIN\n    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;\nEND",
        ),
        (
            "shifts_updated_at",
            "CREATE TRIGGER shifts_updated_at\nAFTER UPDATE ON shifts\nBEGIN\n    UPDATE shifts SET updated_at = CURRENT_TIMESTAMP WHERE shift_id = NEW.shift_id;\nEND",
        ),
        (
            "node_state_updated_at",
            "CREATE TRIGGER node_state_updated_at\nAFTER UPDATE ON node_state\nBEGIN\n    UPDATE node_state SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;\nEND",
        ),
        (
            "cashier_certs_updated_at",
            "CREATE TRIGGER cashier_certs_updated_at\nAFTER UPDATE ON cashier_certs\nBEGIN\n    UPDATE cashier_certs SET updated_at = CURRENT_TIMESTAMP\n    WHERE cashier_id = NEW.cashier_id AND fiscal_number = NEW.fiscal_number;\nEND",
        ),
    ];

    for (name, expected) in expected_triggers {
        let actual: String =
            sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type='trigger' AND name = ?")
                .bind(name)
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|e| panic!("trigger {name} must exist after migration 016: {e}"));
        assert_eq!(
            actual, expected,
            "trigger {name} DDL must be byte-identical post-rebuild (W4 lesson per 015:96-104)"
        );
    }
}
