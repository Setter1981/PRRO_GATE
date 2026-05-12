//! W9.3 / W9.4 acceptance fixtures — `App::reconcile_pending`
//! 6-branch decision tree + per-DocState dispatch.
//!
//! Per design freeze §10.1 + §9.1 inventory.  W9.3 ships the
//! ctx-free dispatch surface:
//!   - Branches (a)/(b)/(d)/(e2)/(f) at FN level.
//!   - Branch (c)/(e1) per-DocState: Sending/Kvt1/Encrypted/Kvt2
//!     (the last via W8 `stage_finalize::run`).  Ctx-needy states
//!     (Prepared/Signed/Sent/ErrorRetryable) emit `BOOT_DISPATCH_DEFERRED`.
//!
//! **Fixture mapping to freeze §10.1 table:**
//!   #1   (a) FN absent → bootstrap.
//!   #2   (b) FN+ONLINE+no pending → idempotent no-op.
//!   #3   (c) per-DocState dispatch matrix (subset for W9.3).
//!   #4   (d) OFFLINE refusal — parametrised over Offline /
//!        GoingOffline / GoingOnline (freeze §10.1 #4-bis).
//!   #5   (e1)→(c) cascade: PRRO_GATE-ah8 verbatim — shift_state=
//!        Opened + pending doc → no `upsert_initial`.
//!   #6   (e2) orphan shift no-doc → shift→Error + node_state.
//!        shift_state→Closed + CRITICAL audit (HIGH 10 fix).
//!   #6b  (e2) idempotency — second boot dispatches to (b).
//!   #7   (f) BLOCKED / STOP_MODE / CRYPTO_DEGRADED preserve.
//!   #9   Idempotency run-twice on a previously-OK FN.
//!
//! Some §10.1 entries are deferred to W11 / later fixtures: per-
//! DocState dispatch for ctx-needy states needs DpsChannel +
//! SigningContext wiring; PRRO_GATE-ah8 acceptance verbatim
//! handled by #5.

use prro::db::models::ids::DocumentId;
use prro::services::reconciliation::boot_phase::{self, BranchOutcome, SubBranch};
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool");
    (dir, pool)
}

async fn seed_fn_config(pool: &SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state(
    pool: &SqlitePool,
    fn_id: &str,
    mode: &str,
    shift_state: &str,
    next_lnd: i64,
) {
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(fn_id)
    .bind(mode)
    .bind(shift_state)
    .bind(next_lnd)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_doc_in_state(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
    state: &str,
) -> DocumentId {
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', ?, 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(lnd)
    .bind(state)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_node_state(pool: &SqlitePool, fn_id: &str) -> Option<(String, String, i64)> {
    sqlx::query_as("SELECT mode, shift_state, next_lnd FROM node_state WHERE fiscal_number = ?")
        .bind(fn_id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn doc_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─── #1 — branch (a) FN absent → upsert_initial + audit ───────────────

#[tokio::test]
async fn branch_a_bootstraps_missing_fn_row() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    // No node_state row seeded.
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(outcome, BranchOutcome::Bootstrapped);
    let row = read_node_state(&pool, "1234567890").await;
    assert_eq!(row, Some(("ONLINE".into(), "CLOSED".into(), 1)));
    assert_eq!(audit_count(&pool, "NODE_STATE_INITIALISED").await, 1);
}

// ─── #2 — branch (b) FN+ONLINE+no pending → idempotent no-op ──────────

#[tokio::test]
async fn branch_b_idempotent_no_op_preserves_row() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    // Pre-seed with shift_state=Opened (NOT Closed) — proves no
    // upsert_initial-style overwrite (PRRO_GATE-ah8 invariant).
    seed_node_state(&pool, "1234567890", "ONLINE", "OPENED", 42).await;
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(outcome, BranchOutcome::IdempotentNoop);
    let row = read_node_state(&pool, "1234567890").await;
    assert_eq!(row, Some(("ONLINE".into(), "OPENED".into(), 42)));
    assert_eq!(audit_count(&pool, "NODE_STATE_BOOT_IDEMPOTENT").await, 1);
    // Branch (b) MUST NOT emit NODE_STATE_INITIALISED.
    assert_eq!(audit_count(&pool, "NODE_STATE_INITIALISED").await, 0);
}

// ─── #3 — branch (c) per-DocState dispatch matrix ─────────────────────

#[tokio::test]
async fn branch_c_dispatches_sending_to_resume_helper() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_in_state(&pool, "1234567890", 0x11, "SENDING").await;
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    match outcome {
        BranchOutcome::Reconciled {
            ref histogram,
            sub_branch,
        } => {
            assert_eq!(sub_branch, SubBranch::C);
            assert_eq!(histogram.sending_resumed, 1);
            assert_eq!(histogram.total_visited(), 1);
        }
        other => panic!("expected Reconciled(c) with sending_resumed=1, got {other:?}"),
    }
    assert_eq!(doc_state(&pool, doc).await, "ERROR_RETRYABLE");
    assert_eq!(
        audit_count(&pool, "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        1
    );
    assert_eq!(audit_count(&pool, "NODE_STATE_BOOT_RECONCILED").await, 1);
}

#[tokio::test]
async fn branch_c_dispatches_kvt1_to_passive_hold() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_in_state(&pool, "1234567890", 0x12, "KVT1").await;
    boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(
        doc_state(&pool, doc).await,
        "KVT1",
        "passive hold preserves state"
    );
    assert_eq!(audit_count(&pool, "BOOT_KVT1_HOLD_DEFERRED").await, 1);
}

#[tokio::test]
async fn branch_c_dispatches_encrypted_to_error_retryable() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_in_state(&pool, "1234567890", 0x13, "ENCRYPTED").await;
    boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(
        doc_state(&pool, doc).await,
        "ERROR_RETRYABLE",
        "1-tick deferral"
    );
    assert_eq!(audit_count(&pool, "BOOT_ENCRYPTED_REROUTED").await, 1);
}

#[tokio::test]
async fn branch_c_ctx_needy_states_emit_deferred_audit() {
    // Per W9.3 freeze: PREPARED/SIGNED/SENT/ERROR_RETRYABLE need
    // DpsChannel + SigningContext (W11+ wiring).  Until then, boot
    // emits BOOT_DISPATCH_DEFERRED for each + leaves state untouched.
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;
    let dp = seed_doc_in_state(&pool, "1234567890", 0x20, "PREPARED").await;
    let ds = seed_doc_in_state(&pool, "1234567890", 0x21, "SIGNED").await;
    let dt = seed_doc_in_state(&pool, "1234567890", 0x22, "SENT").await;
    let de = seed_doc_in_state(&pool, "1234567890", 0x23, "ERROR_RETRYABLE").await;
    boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    // All 4 stayed in source state.
    assert_eq!(doc_state(&pool, dp).await, "PREPARED");
    assert_eq!(doc_state(&pool, ds).await, "SIGNED");
    assert_eq!(doc_state(&pool, dt).await, "SENT");
    assert_eq!(doc_state(&pool, de).await, "ERROR_RETRYABLE");
    // 4 deferred-dispatch audit rows.
    assert_eq!(audit_count(&pool, "BOOT_DISPATCH_DEFERRED").await, 4);
}

// ─── #4 — branch (d) OFFLINE-class refusal ────────────────────────────

#[tokio::test]
async fn branch_d_refuses_boot_on_offline_mode() {
    for mode in &["OFFLINE", "GOING_OFFLINE", "GOING_ONLINE"] {
        let (_dir, pool) = fresh_pool().await;
        seed_fn_config(&pool, "1234567890").await;
        seed_node_state(&pool, "1234567890", mode, "CLOSED", 1).await;
        let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
            .await
            .unwrap();
        match outcome {
            BranchOutcome::OfflineRefusal { observed_mode } => {
                assert_eq!(observed_mode.as_str(), *mode);
            }
            other => panic!("expected OfflineRefusal for {mode}, got {other:?}"),
        }
        let row = read_node_state(&pool, "1234567890").await;
        assert_eq!(row.unwrap().0, *mode, "row UNCHANGED post-refusal");
        assert!(
            audit_count(&pool, "NODE_STATE_BOOT_OFFLINE_REFUSAL").await >= 1,
            "audit emitted before refusal return"
        );
    }
}

// ─── #5 — PRRO_GATE-ah8 verbatim (branch (c) preserves shift_state) ───

#[tokio::test]
async fn fixture_5_ah8_verbatim_preserves_opened_shift_state() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    // PRRO_GATE-ah8 hazard fixture: shift_state=Opened (NOT in
    // {Opening, Closing}, so dispatches to branch (c) per §3.7).
    seed_node_state(&pool, "1234567890", "ONLINE", "OPENED", 5).await;
    // One pending SHIFT_OPEN-equivalent doc — we use SELL for simplicity
    // (the assertion is about node_state.shift_state, not the doc type).
    let doc = seed_doc_in_state(&pool, "1234567890", 0x30, "SENT").await;
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    match outcome {
        BranchOutcome::Reconciled {
            ref histogram,
            sub_branch,
        } => {
            // shift_state=Opened is NOT in {Opening,Closing} → branch (c).
            assert_eq!(sub_branch, SubBranch::C);
            assert_eq!(histogram.sent_deferred, 1);
            assert_eq!(histogram.total_visited(), 1);
        }
        other => panic!("expected Reconciled(c) sent_deferred=1, got {other:?}"),
    }
    // ah8 acceptance: shift_state STILL Opened (no upsert_initial mask).
    let row = read_node_state(&pool, "1234567890").await;
    assert_eq!(
        row,
        Some(("ONLINE".into(), "OPENED".into(), 5)),
        "shift_state and next_lnd MUST be untouched"
    );
    assert_eq!(
        audit_count(&pool, "NODE_STATE_INITIALISED").await,
        0,
        "no upsert"
    );
    // Doc is in Sent (ctx-needy) → BOOT_DISPATCH_DEFERRED.
    assert_eq!(doc_state(&pool, doc).await, "SENT");
    assert_eq!(audit_count(&pool, "BOOT_DISPATCH_DEFERRED").await, 1);
}

// ─── #6 — branch (e2) orphan shift no-doc ─────────────────────────────

#[tokio::test]
async fn branch_e2_orphan_shift_resolves_to_error_and_resets_shift_state() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    // node_state in mid-transition.
    seed_node_state(&pool, "1234567890", "ONLINE", "OPENING", 1).await;
    // Orphan shift row in OPENING with NO matching pending doc.
    let shift_bytes = vec![0xAB; 16];
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at) \
         VALUES (?, '1234567890', 'OPENING', 'ONLINE', '2026-05-10T00:00:00Z')",
    )
    .bind(&shift_bytes)
    .execute(&pool)
    .await
    .unwrap();
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        BranchOutcome::OrphanShiftResolved {
            orphans_resolved: 1
        }
    );
    // shifts.state → ERROR.
    let shift_state: String = sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(&shift_bytes)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shift_state, "ERROR");
    // node_state.shift_state → CLOSED (HIGH 10 fix).
    let row = read_node_state(&pool, "1234567890").await;
    assert_eq!(row.unwrap().1, "CLOSED");
    assert_eq!(audit_count(&pool, "SHIFT_BOOT_ORPHAN_ERROR").await, 1);
}

#[tokio::test]
async fn fixture_5_strict_branch_e1_with_opening_shift_and_pending_doc() {
    // LOW 1 fix: freeze §10.1 #5-strict — branch (e1) strict pre-
    // condition (shift_state ∈ {Opening, Closing}) + a matching
    // pending doc.  This dispatches to (c) per-doc loop via the
    // (e1) → (c) cascade; the FN-level audit MUST tag "branch": "e1"
    // (NOT "c") so operators distinguish strict mid-transition
    // recovery from ordinary branch (c).
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "OPENING", 1).await;
    let doc = seed_doc_in_state(&pool, "1234567890", 0x31, "SENDING").await;
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    match outcome {
        BranchOutcome::Reconciled {
            ref histogram,
            sub_branch,
        } => {
            assert_eq!(sub_branch, SubBranch::E1, "shift_state=Opening → branch e1");
            assert_eq!(histogram.sending_resumed, 1);
        }
        other => panic!("expected Reconciled(e1), got {other:?}"),
    }
    assert_eq!(
        doc_state(&pool, doc).await,
        "ERROR_RETRYABLE",
        "Sending dispatched"
    );
    // Verify the FN-level audit payload tagged the (e1) branch — not (c).
    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'NODE_STATE_BOOT_RECONCILED' AND entity_id = ?",
    )
    .bind("1234567890")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        payload.contains("\"branch\":\"e1\""),
        "audit payload must tag branch as e1, got: {payload}"
    );
}

#[tokio::test]
async fn branch_e2_handles_multiple_orphan_shifts() {
    // LOW 2 fix: rare but possible — multiple shifts in OPENING/
    // CLOSING for one FN (prior boots failed at different times).
    // All MUST transition to ERROR in one envelope + node_state.shift_state
    // resets to CLOSED + per-shift CRITICAL audit.
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "OPENING", 1).await;
    // Seed 3 orphan shifts: 2 OPENING + 1 CLOSING.
    for (i, state) in &[
        (0xA1u8, "OPENING"),
        (0xA2u8, "OPENING"),
        (0xA3u8, "CLOSING"),
    ] {
        let shift_bytes = vec![*i; 16];
        sqlx::query(
            "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at) \
             VALUES (?, '1234567890', ?, 'ONLINE', '2026-05-10T00:00:00Z')",
        )
        .bind(&shift_bytes)
        .bind(*state)
        .execute(&pool)
        .await
        .unwrap();
    }
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        BranchOutcome::OrphanShiftResolved {
            orphans_resolved: 3
        }
    );
    // All 3 shifts in ERROR.
    let err_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM shifts WHERE fiscal_number = '1234567890' AND state = 'ERROR'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(err_count, 3, "all 3 orphan shifts transitioned to ERROR");
    // 3 CRITICAL audit rows.
    assert_eq!(audit_count(&pool, "SHIFT_BOOT_ORPHAN_ERROR").await, 3);
    // node_state.shift_state reset to CLOSED.
    assert_eq!(
        read_node_state(&pool, "1234567890").await.unwrap().1,
        "CLOSED"
    );
}

#[tokio::test]
async fn branch_e2_idempotent_second_boot_dispatches_to_b() {
    // Freeze §9.1 #6-bis (HIGH 10 idempotency).
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "OPENING", 1).await;
    let shift_bytes = vec![0xAB; 16];
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at) \
         VALUES (?, '1234567890', 'OPENING', 'ONLINE', '2026-05-10T00:00:00Z')",
    )
    .bind(&shift_bytes)
    .execute(&pool)
    .await
    .unwrap();
    // First boot: (e2) fires.
    let r1 = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(
        r1,
        BranchOutcome::OrphanShiftResolved {
            orphans_resolved: 1
        }
    );
    // Second boot: shifts.state is now ERROR, node_state.shift_state is
    // Closed, no pending docs → branch (b).
    let r2 = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(r2, BranchOutcome::IdempotentNoop);
    assert_eq!(
        audit_count(&pool, "SHIFT_BOOT_ORPHAN_ERROR").await,
        1,
        "no duplicate orphan-resolution audit"
    );
    assert_eq!(audit_count(&pool, "NODE_STATE_BOOT_IDEMPOTENT").await, 1);
}

// ─── #7 — branch (f) preserve Blocked / StopMode / CryptoDegraded ─────

#[tokio::test]
async fn branch_f_preserves_blocked_mode() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "BLOCKED", "CLOSED", 7).await;
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(outcome, BranchOutcome::PreservedBlocked);
    assert_eq!(
        read_node_state(&pool, "1234567890").await.unwrap().0,
        "BLOCKED"
    );
    assert_eq!(
        audit_count(&pool, "NODE_STATE_BOOT_BLOCKED_PRESERVED").await,
        1
    );
}

#[tokio::test]
async fn branch_f_preserves_stop_mode() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "STOP_MODE", "CLOSED", 7).await;
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(outcome, BranchOutcome::PreservedStopMode);
    assert_eq!(
        audit_count(&pool, "NODE_STATE_BOOT_STOP_MODE_PRESERVED").await,
        1
    );
}

#[tokio::test]
async fn branch_f_preserves_crypto_degraded() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "CRYPTO_DEGRADED", "CLOSED", 7).await;
    let outcome = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(outcome, BranchOutcome::PreservedCryptoDegraded);
    assert_eq!(
        audit_count(&pool, "NODE_STATE_BOOT_CRYPTO_DEGRADED_PRESERVED").await,
        1
    );
}

// ─── #9 — idempotency: run twice on previously-OK FN ──────────────────

#[tokio::test]
async fn idempotency_two_consecutive_boots_on_ok_fn() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    // First boot: branch (a) bootstraps.
    let r1 = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(r1, BranchOutcome::Bootstrapped);
    // Second boot: branch (b) idempotent (FN row now exists, no
    // pending, mode=Online).
    let r2 = boot_phase::run_boot_reconciliation(&pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(r2, BranchOutcome::IdempotentNoop);
    // Counts: 1 initialised + 1 idempotent.
    assert_eq!(audit_count(&pool, "NODE_STATE_INITIALISED").await, 1);
    assert_eq!(audit_count(&pool, "NODE_STATE_BOOT_IDEMPOTENT").await, 1);
}

// ─── App::reconcile_pending — multi-FN iteration ──────────────────────

#[tokio::test]
async fn app_reconcile_pending_iterates_all_configured_fns() {
    use prro::config::AppConfig;
    use prro::App;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{}"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    // Seed 3 FNs.
    for fn_id in ["1111111111", "2222222222", "3333333333"] {
        seed_fn_config(app.db(), fn_id).await;
    }
    app.reconcile_pending().await.unwrap();
    // Each FN got a NODE_STATE_INITIALISED audit (all 3 absent pre-boot).
    assert_eq!(
        audit_count(app.db(), "NODE_STATE_INITIALISED").await,
        3,
        "one bootstrap per configured FN"
    );
}

#[tokio::test]
async fn app_reconcile_pending_fails_fast_on_first_offline_fn() {
    use prro::config::AppConfig;
    use prro::{App, BootError};

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{}"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    // Seed two FNs: one OFFLINE (sorted first alphabetically), one OK.
    seed_fn_config(app.db(), "1111111111").await;
    seed_fn_config(app.db(), "2222222222").await;
    seed_node_state(app.db(), "1111111111", "OFFLINE", "CLOSED", 1).await;
    // Note: 2222222222 has no node_state — would dispatch to (a) if reached.
    let result = app.reconcile_pending().await;
    match result {
        Err(BootError::OfflineModeRefusal { fiscal_number }) => {
            assert_eq!(fiscal_number, "1111111111");
        }
        other => panic!("expected OfflineModeRefusal, got {other:?}"),
    }
    // 2222222222 NOT iterated → no audit emitted for it.
    let row2 = read_node_state(app.db(), "2222222222").await;
    assert!(row2.is_none(), "second FN never reached (fail-fast)");
}

// ─── W9.4 M1 — App::reconcile_pending returns ReconciliationSummary ───

#[tokio::test]
async fn app_reconcile_pending_returns_summary_with_per_branch_counts() {
    // M1 fix: App::reconcile_pending aggregates per-FN BranchOutcome
    // into ReconciliationSummary per freeze §2.1.  Test seeds 5 FNs
    // exercising different branches, verifies summary fields.
    use prro::config::AppConfig;
    use prro::App;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{}"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    // FN1 — absent node_state → branch (a) bootstrap.
    seed_fn_config(app.db(), "1111111111").await;
    // FN2 — Online + no pending → branch (b) idempotent.
    seed_fn_config(app.db(), "2222222222").await;
    seed_node_state(app.db(), "2222222222", "ONLINE", "CLOSED", 1).await;
    // FN3 — Online + 1 pending Sending → branch (c) with sending_resumed=1.
    seed_fn_config(app.db(), "3333333333").await;
    seed_node_state(app.db(), "3333333333", "ONLINE", "CLOSED", 1).await;
    seed_doc_in_state(app.db(), "3333333333", 0x71, "SENDING").await;
    // FN4 — Blocked → branch (f1) preserved.
    seed_fn_config(app.db(), "4444444444").await;
    seed_node_state(app.db(), "4444444444", "BLOCKED", "CLOSED", 1).await;
    // FN5 — Online + Opening + orphan shift → branch (e2).
    seed_fn_config(app.db(), "5555555555").await;
    seed_node_state(app.db(), "5555555555", "ONLINE", "OPENING", 1).await;
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at) \
         VALUES (?, '5555555555', 'OPENING', 'ONLINE', '2026-05-10T00:00:00Z')",
    )
    .bind(vec![0x55u8; 16])
    .execute(app.db())
    .await
    .unwrap();

    let summary = app.reconcile_pending().await.expect("reconcile must Ok");

    assert_eq!(summary.branch_a, 1, "FN1 bootstrap");
    assert_eq!(summary.branch_b, 1, "FN2 idempotent");
    assert_eq!(summary.branch_c, 1, "FN3 reconciled");
    assert_eq!(summary.branch_e2, 1, "FN5 orphan resolved");
    assert_eq!(summary.branch_f_blocked, 1, "FN4 preserved blocked");
    assert_eq!(summary.shift_orphans_to_error, 1);
    assert_eq!(summary.docs_advanced.sending_resumed, 1);
    assert_eq!(summary.docs_advanced.total_visited(), 1);
}

// ─── W9.4 L5 — partition exhaustive matrix (freeze §3.7) ──────────────

#[tokio::test]
async fn branch_partition_exhaustive_matrix() {
    // L5 fix: enumerate representative `(mode, shift_state, has_pending_doc,
    // seeded_node_state)` quadruples and assert exactly one branch fires per
    // quadruple — i.e. §3.7 partition is structurally mutually exclusive.
    //
    // W9.4 cycle-2 LOW-2 + LOW-3 fix:
    //   - LOW-3: add branch (a) case (no node_state row).
    //   - LOW-2: seed `shifts` row in OPENING/CLOSING for (e1) cases so
    //     the partition test exercises the full schema state, not just
    //     node_state.shift_state.
    //
    // Quadruple layout:
    //   (mode | "<absent>", shift_state | "<n/a>", has_pending_doc,
    //    expected_branch_tag, seed_shifts_row_in_state | None)
    let cases: &[(&str, &str, bool, &str, Option<&str>)] = &[
        // Branch (a) — node_state row absent (mode="<absent>" sentinel).
        ("<absent>", "<n/a>", false, "a", None),
        // Branch (d) — Offline-class modes regardless of shift_state/pending.
        ("OFFLINE", "CLOSED", false, "d", None),
        ("GOING_OFFLINE", "CLOSED", false, "d", None),
        ("GOING_ONLINE", "CLOSED", false, "d", None),
        // Branch (f) — preserved modes.
        ("BLOCKED", "CLOSED", false, "f1", None),
        ("STOP_MODE", "CLOSED", false, "f2", None),
        ("CRYPTO_DEGRADED", "CLOSED", false, "f3", None),
        // Online + no pending + shift_state ∉ {Opening, Closing} → (b).
        ("ONLINE", "CLOSED", false, "b", None),
        ("ONLINE", "OPENED", false, "b", None),
        // Online + no pending + shift_state ∈ {Opening, Closing} → (e2).
        ("ONLINE", "OPENING", false, "e2", Some("OPENING")),
        // Online + pending + shift_state ∉ {Opening, Closing} → (c).
        ("ONLINE", "CLOSED", true, "c", None),
        ("ONLINE", "OPENED", true, "c", None),
        // Online + pending + shift_state ∈ {Opening, Closing} → (e1).
        // LOW-2: also seed a `shifts` row in the matching state so the
        // schema state is internally consistent (not just node_state).
        ("ONLINE", "OPENING", true, "e1", Some("OPENING")),
        ("ONLINE", "CLOSING", true, "e1", Some("CLOSING")),
    ];

    for (i, (mode, shift_state, has_pending, expected, seed_shifts_row)) in cases.iter().enumerate()
    {
        let (_dir, pool) = fresh_pool().await;
        let fn_id = format!("99999999{:02}", i);
        seed_fn_config(&pool, &fn_id).await;
        // LOW-3 fix: branch (a) — skip node_state seed entirely so
        // run_boot_reconciliation observes Ok(None) from node_state::get.
        if *mode != "<absent>" {
            seed_node_state(&pool, &fn_id, mode, shift_state, 1).await;
        }
        // LOW-2 fix: seed shifts row whenever the case explicitly
        // requires one (e1 strict + e2).
        if let Some(shift_state) = seed_shifts_row {
            sqlx::query(
                "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at) \
                 VALUES (?, ?, ?, 'ONLINE', '2026-05-10T00:00:00Z')",
            )
            .bind(vec![i as u8; 16])
            .bind(&fn_id)
            .bind(shift_state)
            .execute(&pool)
            .await
            .unwrap();
        }
        if *has_pending {
            seed_doc_in_state(&pool, &fn_id, (0x80 + i) as u8, "KVT1").await;
        }
        let outcome = boot_phase::run_boot_reconciliation(&pool, &fn_id, None)
            .await
            .unwrap_or_else(|e| panic!("case {i} ({mode},{shift_state},{has_pending}): {e}"));
        assert_eq!(
            outcome.branch_tag(),
            *expected,
            "case {i}: ({mode}, {shift_state}, pending={has_pending}) — \
             expected {expected}, got {outcome:?}"
        );
    }
}
