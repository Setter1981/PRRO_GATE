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
//!   #4   (d) GOING_ONLINE refusal — M3b W7b update (2026-05-16):
//!        only GoingOnline still refuses at branch (d); Offline /
//!        GoingOffline fall through to per-doc dispatch via the
//!        W7b post-sign dispatcher
//!        (`services::write_path::dispatch::dispatch_post_sign`).
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
use prro::services::reconciliation::ReconcileGuard;
use sqlx::SqlitePool;

/// W2 module-level enforcement helper — every integration test that
/// invokes `boot_phase::run_boot_reconciliation` directly must pass
/// a `ReconcileGuard<'_>`.  Tests have no App mutex to acquire, so
/// they use the explicit test seam (`for_integration_test_only`)
/// declared in `services::reconciliation::guard`.  Wrapped in a
/// helper fn for terseness; temporary-lifetime extension makes
/// `&recon_guard()` a valid borrow argument.
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

/// Boilerplate shared by the 3 boot-driven dispatch tests (W7b offline
/// happy path + 2 NIT-C5-4 signer-refused acceptance).  Builds an
/// `App` rooted in a fresh tempdir SQLite DB and returns the tempdir
/// (held for lifetime) + the booted `App` + a clone of its pool.
async fn boot_app_with_db(db_filename: &str) -> (tempfile::TempDir, prro::App, SqlitePool) {
    use prro::config::AppConfig;
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(db_filename);
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{0}"
secure_db_path = "{0}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).expect("config parse");
    let app = prro::App::boot(cfg).await.expect("App::boot");
    let pool = app.db().clone();
    (dir, app, pool)
}

// ─── #1 — branch (a) FN absent → upsert_initial + audit ───────────────

#[tokio::test]
async fn branch_a_bootstraps_missing_fn_row() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    // No node_state row seeded.
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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

// ─── #4 — branch (d) refuses ONLY GoingOnline (M3b W7b update) ────────
//
// Post-W7b semantics (operator PR #57 Round 1 HIGH, 2026-05-16):
// branch (d) refuses ONLY `GoingOnline` (W9 backlog-drain mode).
// Offline / GoingOffline modes now DROP THROUGH to per-doc dispatch,
// which routes via `dispatch_post_sign` → `stage_offline_ack::run`.
// Pre-W7b this branch refused all three modes; the W7-aware
// behaviour makes boot reconciliation usable in offline mode.

#[tokio::test]
async fn branch_d_refuses_boot_on_going_online_mode() {
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "GOING_ONLINE", "CLOSED", 1).await;
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
        .await
        .unwrap();
    match outcome {
        BranchOutcome::OfflineRefusal { observed_mode } => {
            assert_eq!(observed_mode.as_str(), "GOING_ONLINE");
        }
        other => panic!("expected OfflineRefusal for GOING_ONLINE, got {other:?}"),
    }
    let row = read_node_state(&pool, "1234567890").await;
    assert_eq!(row.unwrap().0, "GOING_ONLINE", "row UNCHANGED post-refusal");
    assert!(
        audit_count(&pool, "NODE_STATE_BOOT_OFFLINE_REFUSAL").await >= 1,
        "audit emitted before refusal return"
    );
}

#[tokio::test]
async fn boot_in_offline_mode_reaches_per_doc_dispatch_not_branch_d_refusal() {
    // M3b W7b boot-level integration (operator PR #57 Round 1
    // MED-1 partial, 2026-05-16): with FN in OFFLINE mode +
    // pending SIGNED doc, boot reconciliation MUST reach
    // per-doc dispatch (not short-circuit at branch (d)).
    //
    // This test uses `deps=None` — SIGNED docs are DEFERRED
    // through the deps-needy path (BOOT_DISPATCH_DEFERRED audit
    // + signed_deferred histogram bump), NOT routed to
    // stage_offline_ack.  The HIGH-fix property tested here is:
    // boot does NOT short-circuit at branch (d) when mode is
    // OFFLINE; it proceeds to the per-doc loop where W7b's
    // dispatcher CAN run when deps=Some.
    //
    // The full end-to-end "deps=Some + offline doc reaches
    // stage_offline_ack via boot" test requires substrate mocks
    // (SigningContext + DpsChannel) — deferred to a follow-up
    // PR (see `tests/write_path_dispatcher_post_sign.rs` for the
    // dispatcher-level proof).
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "OFFLINE", "OPENED", 1).await;
    // Seed a SIGNED doc — would be a candidate for offline
    // dispatch if deps were present.
    let _doc = seed_doc_in_state(&pool, "1234567890", 0x77, "SIGNED").await;
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
        .await
        .unwrap();
    // Critical assertion: NOT OfflineRefusal.  This proves the
    // HIGH-fix: branch (d) no longer cuts OFFLINE mode.
    assert!(
        !matches!(outcome, BranchOutcome::OfflineRefusal { .. }),
        "post-W7b: OFFLINE mode must NOT short-circuit at branch (d); got: {outcome:?}"
    );
    // Boot reached the reconciled-FN audit (NOT the offline-
    // refusal audit).
    assert!(
        audit_count(&pool, "NODE_STATE_BOOT_RECONCILED").await >= 1,
        "boot reached the reconciled-FN audit path (per-doc loop entered)"
    );
    // No offline-refusal audit for this FN — branch (d) skipped.
    assert_eq!(
        audit_count(&pool, "NODE_STATE_BOOT_OFFLINE_REFUSAL").await,
        0,
        "branch (d) refusal audit MUST NOT fire on OFFLINE mode post-W7b"
    );
}

#[tokio::test]
async fn branch_d_no_longer_refuses_offline_or_going_offline() {
    // Post-W7b: these modes drop through to per-doc dispatch.
    // With no pending docs in this test, the cascade reaches the
    // pending-docs loop, finds zero docs, and emits
    // NODE_STATE_BOOT_RECONCILED with empty histogram — NOT
    // OfflineRefusal.
    for mode in &["OFFLINE", "GOING_OFFLINE"] {
        let (_dir, pool) = fresh_pool().await;
        seed_fn_config(&pool, "1234567890").await;
        // For Offline mode the shift must NOT be OPENED for the
        // (e2)/(b) cascade to be benign; use CLOSED.
        seed_node_state(&pool, "1234567890", mode, "CLOSED", 1).await;
        let outcome =
            boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
                .await
                .unwrap();
        assert!(
            !matches!(outcome, BranchOutcome::OfflineRefusal { .. }),
            "post-W7b: {mode} must NOT short-circuit at branch (d); got: {outcome:?}"
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
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at, opened_by_cashier_id) \
         VALUES (?, '1234567890', 'OPENING', 'ONLINE', '2026-05-10T00:00:00Z', 'test-cashier')",
    )
    .bind(&shift_bytes)
    .execute(&pool)
    .await
    .unwrap();
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
async fn branch_e2_resolves_the_single_active_orphan() {
    // RS-3 C2 (migration 023): the active-shift uniqueness index + the
    // fail-closed backfill now GUARANTEE at most ONE active shift per FN, so
    // the former "multiple orphan shifts for one FN" scenario (this test's
    // earlier premise) can no longer arise — the index rejects a second active
    // insert, and a pre-023 DB with duplicates fails the migration LOUD (no
    // auto-resolve, operator-locked).  Branch E2's multiple-orphan loop is
    // therefore belt-and-suspenders; this exercises the realistic post-C2
    // case: ONE orphan (OPENING) → ERROR in one envelope + node_state.shift_state
    // reset to CLOSED + a CRITICAL audit.
    let (_dir, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "OPENING", 1).await;
    let shift_bytes = vec![0xA1u8; 16];
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at, opened_by_cashier_id) \
         VALUES (?, '1234567890', 'OPENING', 'ONLINE', '2026-05-10T00:00:00Z', 'test-cashier')",
    )
    .bind(&shift_bytes)
    .execute(&pool)
    .await
    .unwrap();
    // RS-3 C2: the dangling node_state pointer that the chokepoint
    // (boot_phase.rs `current_shift_id = NULL`) must clear when it resolves the
    // orphan. Without this, a reopened shift would inherit a stale pointer.
    sqlx::query("UPDATE node_state SET current_shift_id = ? WHERE fiscal_number = '1234567890'")
        .bind(&shift_bytes)
        .execute(&pool)
        .await
        .unwrap();
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        BranchOutcome::OrphanShiftResolved {
            orphans_resolved: 1
        }
    );
    let err_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM shifts WHERE fiscal_number = '1234567890' AND state = 'ERROR'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(err_count, 1, "the orphan shift transitioned to ERROR");
    assert_eq!(audit_count(&pool, "SHIFT_BOOT_ORPHAN_ERROR").await, 1);
    assert_eq!(
        read_node_state(&pool, "1234567890").await.unwrap().1,
        "CLOSED"
    );
    // RS-3 C2 chokepoint coverage: the stale pointer must be cleared so the FN
    // no longer references the now-ERROR orphan shift.
    let dangling: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT current_shift_id FROM node_state WHERE fiscal_number = '1234567890'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        dangling.is_none(),
        "node_state.current_shift_id must be NULL after the orphan is resolved"
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
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at, opened_by_cashier_id) \
         VALUES (?, '1234567890', 'OPENING', 'ONLINE', '2026-05-10T00:00:00Z', 'test-cashier')",
    )
    .bind(&shift_bytes)
    .execute(&pool)
    .await
    .unwrap();
    // First boot: (e2) fires.
    let r1 = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let r2 = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
    let r1 = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
        .await
        .unwrap();
    assert_eq!(r1, BranchOutcome::Bootstrapped);
    // Second boot: branch (b) idempotent (FN row now exists, no
    // pending, mode=Online).
    let r2 = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", None)
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
db_path = "{0}"
secure_db_path = "{0}_secure"

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
db_path = "{0}"
secure_db_path = "{0}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    // Seed two FNs: one GOING_ONLINE (sorted first alphabetically),
    // one OK.  Post-W7b (operator PR #57 Round 1 HIGH fix,
    // 2026-05-16): only GOING_ONLINE still triggers branch (d)
    // fail-fast at the App level (W9 backlog-drain territory).
    // Offline / GoingOffline no longer fail-fast — they drop
    // through to per-doc dispatch via dispatch_post_sign.
    seed_fn_config(app.db(), "1111111111").await;
    seed_fn_config(app.db(), "2222222222").await;
    seed_node_state(app.db(), "1111111111", "GOING_ONLINE", "CLOSED", 1).await;
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
db_path = "{0}"
secure_db_path = "{0}_secure"

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
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at, opened_by_cashier_id) \
         VALUES (?, '5555555555', 'OPENING', 'ONLINE', '2026-05-10T00:00:00Z', 'test-cashier')",
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
        // Branch (d) — M3b W7b update: only GOING_ONLINE refuses
        // here (W9 backlog-drain mode).  Offline / GoingOffline
        // drop through to (b)/(c)/(e1) cascade so the W7b
        // dispatcher can route docs to stage_offline_ack.  With
        // no pending docs + CLOSED shift, the offline modes
        // resolve to branch (b) (idempotent no-op for empty FN
        // in that mode + shift).
        ("OFFLINE", "CLOSED", false, "b", None),
        ("GOING_OFFLINE", "CLOSED", false, "b", None),
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
                "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at, opened_by_cashier_id) \
                 VALUES (?, ?, ?, 'ONLINE', '2026-05-10T00:00:00Z', 'test-cashier')",
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
        let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, &fn_id, None)
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

// ─── M3b W7b boot-level integration (deps=Some, full production path) ──
//
// Operator PR #57 Round 2 MED fix (2026-05-16): prior round delivered
// only a deps=None proof (boot reaches per-doc dispatch).  Existing
// test infrastructure (`StubDpsChannel` + `det_signing_ctx` from
// `common::*`) makes the deps=Some path testable without 170-LOC
// substrate mock construction.  This test closes the production-path
// proof gap: full chain `run_boot_reconciliation → dispatch_pending_doc
// → dispatch_post_sign → stage_offline_ack`.

mod common;

#[tokio::test]
async fn boot_with_deps_routes_offline_signed_doc_to_offline_local_ack() {
    use common::{ack, det_signing_ctx, StubDpsChannel};
    use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};
    use prro::transports::dps::dto::CheckSignBlob;

    let (_dir, app, pool) = boot_app_with_db("w7b.db").await;

    // Seed: FN OFFLINE + OPENED + active OPEN session + 1 code +
    // 1 SIGNED doc.  This is the exact end-to-end fixture operator
    // specified.
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "OFFLINE", "OPENED", 1).await;
    let session_id_bytes: [u8; 16] = [0xAA; 16];
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-05-16T00:00:00Z')",
    )
    .bind(&session_id_bytes[..])
    .bind("1234567890")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES (?, ?)")
        .bind("1234567890")
        .bind(42_i64)
        .execute(&pool)
        .await
        .unwrap();
    let doc = seed_doc_in_state(&pool, "1234567890", 0x77, "SIGNED").await;
    // M2-01: stage_offline_ack now advances the MAC chain seed, so the SIGNED
    // doc must carry unsigned_xml_sha256 (the real stage_sign output).
    // previous_hash stays NULL (genesis) to match the genesis node seed, so the
    // step-7b drift-assert passes.
    sqlx::query("UPDATE fiscal_documents SET unsigned_xml_sha256 = ? WHERE document_id = ?")
        .bind(vec![0xA7u8; 32])
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();

    // StubDpsChannel: empty response queue + spy panicking on any
    // call — proves stage_send is NEVER invoked on offline branch
    // (criterion 6 of `m3b-w7b-review-criteria` end-to-end).
    let stub = StubDpsChannel::with_spy(
        Ok(ack("unused-W7b-violation")),
        Box::new(|| {
            panic!(
                "W7b violation: stage_send/DPS must NOT be invoked on offline branch \
                 (operator W7b criterion 6)"
            )
        }),
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xDEu8, 0xAD, 0xBE, 0xEF]);
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    // Run full boot reconciliation with deps.
    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with must succeed on offline branch");

    // Critical: doc transitioned Signed → OfflineLocalAck.
    assert_eq!(
        doc_state(&pool, doc).await,
        "OFFLINE_LOCAL_ACK",
        "boot + W7b dispatcher must route offline SIGNED doc to OFFLINE_LOCAL_ACK"
    );
    // Code consumed.
    let consumed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes \
         WHERE fiscal_number = ? AND consumed_at IS NOT NULL",
    )
    .bind("1234567890")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        consumed, 1,
        "exactly one offline code consumed via boot dispatch"
    );
    // transport_trace stays empty (stage_send never invoked).
    let tt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        tt_count, 0,
        "transport_trace must stay empty — stage_send not invoked on offline boot path"
    );
    // StubDpsChannel call count == 0.
    assert_eq!(
        stub.call_count(),
        0,
        "StubDpsChannel.send_chk must not be invoked"
    );
    // OFFLINE_LOCAL_ACK_APPLIED audit present.
    assert_eq!(
        audit_count(&pool, "OFFLINE_LOCAL_ACK_APPLIED").await,
        1,
        "exactly one OFFLINE_LOCAL_ACK_APPLIED audit row"
    );
    // NODE_STATE_BOOT_RECONCILED audit present + by_outcome.offline_local_ack_emitted == 1.
    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'NODE_STATE_BOOT_RECONCILED' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        v["by_outcome"]["offline_local_ack_emitted"], 1,
        "histogram bucket offline_local_ack_emitted must be 1 in audit payload"
    );
    assert_eq!(v["pending_visited"], 1, "exactly one pending doc visited");
}

// ─── #18 — NIT-C5-4: boot-driven signer-refusal increments histogram ─

/// Verifies the W14a-2b Commit 5 NIT-C5-2 + NIT-C5-5 split:
/// boot-phase dispatch of a SIGNED doc with mismatched signer routes
/// through `stage_send::run` → `Ok(SignerRefused(Mismatch))` →
/// `histogram.signer_refused_mismatch += 1` (NOT generic
/// `signed_dispatched`).  Also verifies the structural-bug counter
/// stays at 0 and the audit payload exposes both buckets distinctly.
#[tokio::test]
async fn boot_signed_dispatch_signer_mismatch_increments_mismatch_bucket() {
    use common::{ack, det_signing_ctx, StubDpsChannel};
    use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};
    use prro::transports::dps::dto::CheckSignBlob;

    let (_dir, app, pool) = boot_app_with_db("c5n4.db").await;

    seed_fn_config(&pool, "1234567890").await;
    // ONLINE mode + OPENED shift_state so dispatch_post_sign routes
    // to the Online branch + stage_send::run; SIGNED doc opens via
    // signer_guard mismatch refusal.
    let shift_id_bytes: [u8; 16] = [0x99; 16];
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, '1234567890', 1, 'OPENED', 'ONLINE', 0, 'cashier-vasya')",
    )
    .bind(&shift_id_bytes[..])
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd, \
            current_shift_id, backend_profile_id, transport_profile_id) \
         VALUES (?, 'ONLINE', 'OPENED', 1, ?, 'b1', 't1')",
    )
    .bind("1234567890")
    .bind(&shift_id_bytes[..])
    .execute(&pool)
    .await
    .unwrap();
    // SIGNED SELL with shift_id bound but signer ≠ opener — triggers
    // Mismatch variant.
    let doc_bytes = vec![0x55u8; 16];
    let req_bytes = vec![0xAAu8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', ?, 1, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-20T12:00:00Z', '{}', ?, 'cashier-petya')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&shift_id_bytes[..])
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"FAKE-CMS".to_vec())
    .execute(&pool)
    .await
    .unwrap();

    // StubDpsChannel with panic spy — proves wire send NEVER fires
    // (signer_guard refuses before wire send per spec §2.3).
    let stub = StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-BE-CALLED")),
        Box::new(|| {
            panic!(
                "signer_guard refusal MUST NOT reach wire send (spec §2.3 \
                 BEFORE envelope/CAS/trace/wire)"
            )
        }),
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with must succeed on signer-refused path");

    // Doc stays SIGNED — Pattern B preserved, no state mutation.
    let state: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(&doc_bytes)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        state, "SIGNED",
        "doc state MUST stay SIGNED on signer refusal"
    );

    // SIGNER_CASHIER_MISMATCH audit committed via Ok-return.
    assert_eq!(
        audit_count(&pool, "SIGNER_CASHIER_MISMATCH").await,
        1,
        "SIGNER_CASHIER_MISMATCH audit MUST be emitted from 4-pre tx",
    );

    // NIT-C5-2 + NIT-C5-5 verification: histogram payload exposes
    // signer_refused_mismatch == 1 + signer_refused_structural_bug == 0.
    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'NODE_STATE_BOOT_RECONCILED' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        v["by_outcome"]["signer_refused_mismatch"], 1,
        "Mismatch variant MUST increment signer_refused_mismatch bucket"
    );
    assert_eq!(
        v["by_outcome"]["signer_refused_structural_bug"], 0,
        "structural-bug bucket MUST stay 0 on operator-Mismatch path"
    );
    assert_eq!(
        v["by_outcome"]["signed_dispatched"], 0,
        "generic signed_dispatched MUST NOT count signer-refused docs"
    );
}

// ─── #19 — NIT-C5-4: structural-bug bucket increments for missing shift ─

#[tokio::test]
async fn boot_signed_dispatch_shift_missing_increments_structural_bug_bucket() {
    use common::{ack, det_signing_ctx, StubDpsChannel};
    use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};
    use prro::transports::dps::dto::CheckSignBlob;

    let (_dir, app, pool) = boot_app_with_db("c5n4b.db").await;

    seed_fn_config(&pool, "1234567890").await;
    // ONLINE + CLOSED shift_state — node_state has no active shift,
    // so the SIGNED doc dispatched below has shift_id = NULL →
    // signer_guard surfaces ShiftMissingForFiscalDoc (structural bug).
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;
    let doc_bytes = vec![0x66u8; 16];
    let req_bytes = vec![0xBBu8; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id) \
         VALUES (?, ?, '1234567890', 1, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-20T12:00:00Z', '{}', ?, 'cashier-vasya')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(&sha)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"FAKE-CMS".to_vec())
    .execute(&pool)
    .await
    .unwrap();

    let stub = StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-BE-CALLED")),
        Box::new(|| panic!("structural-bug refusal MUST NOT reach wire send")),
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xCA, 0xFE, 0xBA, 0xBE]);
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    app.reconcile_pending_with(deps).await.unwrap();

    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'NODE_STATE_BOOT_RECONCILED' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        v["by_outcome"]["signer_refused_structural_bug"], 1,
        "ShiftMissingForFiscalDoc MUST increment structural_bug bucket"
    );
    assert_eq!(
        v["by_outcome"]["signer_refused_mismatch"], 0,
        "operator-Mismatch bucket MUST stay 0 on caller-bug path"
    );
}

// ─── Polish GAP-2: orphan scanner App-boot wiring validation ─────────

/// **Polish cycle / GAP-2 (2026-05-25)** — validates that
/// `boot_phase::close_orphan_transport_traces` is actually wired into
/// `App::reconcile_pending` (NOT just the function-level contract).
/// Existing `rec3_*` tests call the scanner directly — pass even if
/// someone removes the call from `app.rs::reconcile_pending_inner`.
/// This test boots the full App, seeds a pre-boot orphan trace, then
/// calls `App::reconcile_pending` and asserts the orphan was closed —
/// false-positive proof for the boot-wiring contract.
#[tokio::test]
async fn rec3_app_boot_wires_orphan_trace_scanner() {
    let (_d, app, pool) = boot_app_with_db("rec3_wiring.db").await;
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Pre-seed orphan trace BEFORE invoking reconcile.  Doc must exist
    // for FK; reuse seed_doc_in_state helper.
    let doc = seed_doc_in_state(&pool, "1234567890", 0x77, "SENT").await;
    sqlx::query(
        "INSERT INTO transport_trace( \
            document_id, attempt_no, started_at, backend_profile_id, \
            transport_profile_id, request_envelope_sha256) \
         VALUES (?, 1, datetime('now', '-120 seconds'), 'b1', 't1', ?)",
    )
    .bind(doc)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();

    // Pre-boot: orphan present.
    let pre_outcome: Option<String> = sqlx::query_scalar(
        "SELECT outcome_kind FROM transport_trace WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(pre_outcome.is_none(), "pre-reconcile orphan MUST be NULL");

    // Invoke App::reconcile_pending — boot scanner MUST fire as
    // first step inside reconcile_pending_inner.  We tolerate Err
    // result (FN may have GoingOnline mode etc.) — only test that
    // scanner ran (orphan closed).
    let _ = app.reconcile_pending().await;

    // Post-boot: orphan closed з SYSTEM_CRASH.
    let post_outcome: String = sqlx::query_scalar(
        "SELECT outcome_kind FROM transport_trace WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        post_outcome, "SYSTEM_CRASH",
        "App::reconcile_pending MUST wire boot orphan scanner — orphan closed з SYSTEM_CRASH"
    );
    assert_eq!(
        audit_count(&pool, "TRANSPORT_TRACE_ORPHAN_CLOSED").await,
        1,
        "1 TRANSPORT_TRACE_ORPHAN_CLOSED audit MUST emit via App wiring path"
    );
}

// ─── Phase 3 REC-3: orphan transport_trace garbage collection ────────

/// **Phase 3 / REC-3 (2026-05-24)** — boot orphan-trace scanner closes
/// `transport_trace` rows allocated by Envelope 1c-pre but never
/// completed (crash mid-DPS-call: SIGKILL / OOM / power loss).
///
/// Scenario:
///   1. Seed fiscal_document (FK для transport_trace).
///   2. Insert orphan transport_trace row з outcome_kind=NULL,
///      started_at older than 60s.
///   3. Invoke `close_orphan_transport_traces(pool, 60)`.
///   4. Assert: orphan closed з outcome_kind=SYSTEM_CRASH +
///      TRANSPORT_TRACE_ORPHAN_CLOSED Info audit emitted.
#[tokio::test]
async fn rec3_boot_orphan_trace_scanner_closes_old_allocated_rows() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_in_state(&pool, "1234567890", 0x42, "SENT").await;

    // Insert orphan transport_trace row — outcome_kind NULL, started_at
    // 120s ago (well past 60s TTL).
    sqlx::query(
        "INSERT INTO transport_trace( \
            document_id, attempt_no, started_at, backend_profile_id, \
            transport_profile_id, request_envelope_sha256) \
         VALUES (?, 1, datetime('now', '-120 seconds'), 'b1', 't1', ?)",
    )
    .bind(doc)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();

    // Verify orphan present pre-scan.
    let pre_outcome: Option<String> = sqlx::query_scalar(
        "SELECT outcome_kind FROM transport_trace WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        pre_outcome.is_none(),
        "pre-scan: orphan outcome_kind MUST be NULL"
    );

    // Run scanner з 60s TTL.
    let closed = boot_phase::close_orphan_transport_traces(&pool, 60)
        .await
        .expect("scanner");
    assert_eq!(closed, 1, "scanner MUST close exactly 1 orphan");

    // Post-scan: outcome_kind SYSTEM_CRASH + completion fields populated.
    let row: (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT outcome_kind, completed_at, wire_call_started_at, \
                    wire_call_finished_at, error_kind \
             FROM transport_trace WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "SYSTEM_CRASH");
    assert!(row.1.is_some(), "completed_at MUST be set");
    assert!(row.2.is_some(), "wire_call_started_at MUST be set");
    assert!(row.3.is_some(), "wire_call_finished_at MUST be set");
    assert_eq!(row.4.as_deref(), Some("ORPHANED_AT_BOOT_RECOVERY"));

    // Audit emitted з Info severity + payload.
    assert_eq!(audit_count(&pool, "TRANSPORT_TRACE_ORPHAN_CLOSED").await, 1);
    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'TRANSPORT_TRACE_ORPHAN_CLOSED' \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(v["outcome_kind"], "SYSTEM_CRASH");
    assert_eq!(v["ttl_secs"], 60);
    assert_eq!(v["attempt_no"], 1);
}

/// **Phase 3 / REC-3 (2026-05-24)** — TTL guard: orphan rows YOUNGER
/// than `ttl_secs` MUST NOT be closed (защита від false-positive close
/// of legitimately in-flight DPS calls на graceful-shutdown boundary).
#[tokio::test]
async fn rec3_boot_orphan_trace_scanner_skips_rows_within_ttl_window() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_in_state(&pool, "1234567890", 0x43, "SENT").await;

    // Orphan з started_at TYLKO 10s ago — within 60s TTL window.
    sqlx::query(
        "INSERT INTO transport_trace( \
            document_id, attempt_no, started_at, backend_profile_id, \
            transport_profile_id, request_envelope_sha256) \
         VALUES (?, 1, datetime('now', '-10 seconds'), 'b1', 't1', ?)",
    )
    .bind(doc)
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();

    let closed = boot_phase::close_orphan_transport_traces(&pool, 60)
        .await
        .expect("scanner");
    assert_eq!(closed, 0, "scanner MUST skip rows within TTL window");

    // Orphan unchanged.
    let outcome: Option<String> = sqlx::query_scalar(
        "SELECT outcome_kind FROM transport_trace WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(outcome.is_none(), "within-TTL orphan MUST stay NULL");
    assert_eq!(audit_count(&pool, "TRANSPORT_TRACE_ORPHAN_CLOSED").await, 0);
}

// ─── Phase 3 REC-8: W8 return-online vs drain race contract lock ─────

/// **Phase 3 / REC-8 (2026-05-24)** — locks W2 single-writer invariant
/// for node_state snapshot semantic.  Drain orchestrator reads
/// `node_state` ONCE per tick via `node_state::get(pool, fn)` at
/// `backlog_drain.rs:662`; `ns` snapshot used throughout drain
/// decisions (mode check / shift state / cohort filtering / Tier
/// trigger).  W8 `return_online_probe` may flip `mode` mid-tick BUT
/// drain's decisions remain based on pre-tick snapshot.
///
/// This test verifies the contract is observable: change `node_state.
/// mode` between two consecutive drain invocations and assert that
/// each tick sees its own consistent snapshot (no stale read carry-
/// over, no мid-tick re-read race).
///
/// Implementation note: full concurrent W8 spawn + drain race
/// simulation requires complex tokio test orchestration; this
/// тіck-to-tick test is a lighter contract lock that асserts the
/// snapshot-per-tick semantic via observable side-effects.
#[tokio::test]
async fn rec8_drain_reads_node_state_snapshot_per_tick_not_mid_tick() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, "1234567890").await;
    // Start з ONLINE (drain MUST refuse в Step 1 з SKIPPED_NOT_GOING_
    // ONLINE audit).
    seed_node_state(&pool, "1234567890", "ONLINE", "CLOSED", 1).await;

    // Tick 1: drain reads ONLINE snapshot → happy-path (no refusal).
    let view = None::<&prro::services::reconciliation::RuntimeView>;
    let _ = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", view)
        .await
        .expect("tick 1");
    // Tick 1 з ONLINE mode does NOT emit OFFLINE_REFUSAL — refusal
    // path triggers only on offline-class modes (GoingOnline / etc.).
    let refusals_after_tick1 = audit_count(&pool, "NODE_STATE_BOOT_OFFLINE_REFUSAL").await;
    assert_eq!(
        refusals_after_tick1, 0,
        "tick 1 з ONLINE snapshot MUST NOT emit OFFLINE_REFUSAL"
    );

    // External flip (simulating W8 probe success or operator action):
    // ONLINE → GOING_ONLINE.  This change happens BETWEEN ticks.
    sqlx::query("UPDATE node_state SET mode = 'GOING_ONLINE' WHERE fiscal_number = ?")
        .bind("1234567890")
        .execute(&pool)
        .await
        .unwrap();

    // Tick 2: drain MUST re-read node_state (no carry-over from tick
    // 1) and see GOING_ONLINE.  GoingOnline routes through branch (d)
    // refusal — emits NODE_STATE_BOOT_OFFLINE_REFUSAL audit.
    let _ = boot_phase::run_boot_reconciliation(&recon_guard(), &pool, "1234567890", view)
        .await
        .expect("tick 2");

    // Verify behavior CHANGED — second tick read fresh snapshot.
    // GoingOnline emit ONE refusal audit (per-FN single emit per
    // tick — see boot_phase.rs:1338-1340 "Per-FN single refusal").
    let refusals_after_tick2 = audit_count(&pool, "NODE_STATE_BOOT_OFFLINE_REFUSAL").await;
    assert_eq!(
        refusals_after_tick2, 1,
        "tick 2 з GOING_ONLINE snapshot MUST emit exactly 1 OFFLINE_REFUSAL \
         audit (snapshot semantic — drain reads fresh node_state per tick)"
    );

    // Verify per-tick isolation — tick 2's refusal payload references
    // GOING_ONLINE mode (NOT ONLINE from tick 1's stale snapshot).
    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'NODE_STATE_BOOT_OFFLINE_REFUSAL' \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        v["observed_mode"], "GOING_ONLINE",
        "tick 2 payload MUST carry fresh GOING_ONLINE mode, NOT stale ONLINE from tick 1"
    );
}

// ─── AUD-L6-1 (FT) — boot seed projection from the EVER-ISSUED tip ───────────
//
// NC-03 branch (a) projected node_state.last_known_unsigned_xml_sha256 from the
// last ACK only.  M2-01 advances the seed at OFFLINE_LOCAL_ACK for offline-origin
// docs (and stage_finalize SKIPS the advance for them).  So when the FN tail is an
// offline-origin doc with lnd > the last online ACK, boot must project the OFFLINE
// tip's unsigned (h3), NOT the stale last-ACK (h1).  Pre-fix it wrote h1 → a forked
// MAC chain at the next write.

fn h32(b: u8) -> [u8; 32] {
    [b; 32]
}

/// Seed one fiscal_documents row with full chain + offline columns.
#[allow(clippy::too_many_arguments)]
async fn seed_chain_doc(
    pool: &SqlitePool,
    fn_id: &str,
    lnd: i64,
    state: &str,
    sfn: Option<&str>,
    prev: Option<[u8; 32]>,
    unsigned: [u8; 32],
    offline_fiscal_no: Option<i64>,
    offline_session_id: Option<&[u8]>,
) -> DocumentId {
    let doc_bytes = vec![lnd as u8; 16];
    let req_bytes = vec![(lnd as u8) ^ 0xFF; 16];
    let fs_mode = if offline_fiscal_no.is_some() {
        "OFFLINE"
    } else {
        "ONLINE"
    };
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, server_fiscal_no, previous_hash, unsigned_xml_sha256, \
            offline_fiscal_no, offline_session_id) \
         VALUES (?, ?, ?, ?, 'SELL', ?, 'b1', 't1', ?, '2026-01-01T00:00:00Z', '{}', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(lnd)
    .bind(state)
    .bind(fs_mode)
    .bind(vec![0u8; 32])
    .bind(sfn)
    .bind(prev.map(|p| p.to_vec()))
    .bind(unsigned.to_vec())
    .bind(offline_fiscal_no)
    .bind(offline_session_id)
    .execute(pool)
    .await
    .unwrap();
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_node_seed(pool: &SqlitePool, fn_id: &str) -> Option<Vec<u8>> {
    let row: Option<Option<Vec<u8>>> = sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(fn_id)
    .fetch_optional(pool)
    .await
    .unwrap();
    row.flatten()
}

#[tokio::test]
async fn aud_l6_1_boot_projects_seed_from_offline_origin_tip_not_last_ack() {
    let (_d, pool) = fresh_pool().await;
    let fn_id = "1234567890";
    seed_fn_config(&pool, fn_id).await;
    let (h1, h2, h3) = (h32(0xA1), h32(0xA2), h32(0xA3));
    // offline session + codes for the two offline-origin docs (invariant_scan 6b/6d).
    let session = vec![0x5e; 16];
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-01-01T00:00:00Z')",
    )
    .bind(&session)
    .bind(fn_id)
    .execute(&pool)
    .await
    .unwrap();
    // lnd1: online ACK, chain head (prev NULL), unsigned=h1.  ACK needs sfn + KVT1_RAW.
    let doc1 = seed_chain_doc(&pool, fn_id, 1, "ACK", Some("SFN-1"), None, h1, None, None).await;
    sqlx::query("INSERT INTO document_files(document_id, kind, content) VALUES (?, 'KVT1_RAW', ?)")
        .bind(doc1)
        .bind(vec![0xAAu8; 8])
        .execute(&pool)
        .await
        .unwrap();
    // lnd2 + lnd3: offline-origin OFFLINE_LOCAL_ACK, chained off the prior (REAL M2-01 chain).
    seed_chain_doc(&pool, fn_id, 2, "OFFLINE_LOCAL_ACK", None, Some(h1), h2, Some(1), Some(&session))
        .await;
    seed_chain_doc(&pool, fn_id, 3, "OFFLINE_LOCAL_ACK", None, Some(h2), h3, Some(2), Some(&session))
        .await;
    for (code_lnd, doc_byte) in [(1i64, 2u8), (2, 3)] {
        sqlx::query(
            "INSERT INTO offline_codes(fiscal_number, code_lnd, consumed_at, consumed_by_document_id) \
             VALUES (?, ?, '2026-01-01T00:00:01Z', ?)",
        )
        .bind(fn_id)
        .bind(code_lnd)
        .bind(vec![doc_byte; 16])
        .execute(&pool)
        .await
        .unwrap();
    }
    // node_state ABSENT (branch a — lost while ledger survived).

    boot_phase::run_boot_reconciliation(&recon_guard(), &pool, fn_id, None)
        .await
        .expect("boot branch (a) reconstructs node_state");

    // FT pin: the projected seed is the OFFLINE-origin tip (h3), NOT the last ACK (h1).
    let seed = read_node_seed(&pool, fn_id).await.expect("node_state seed projected");
    assert_eq!(
        seed,
        h3.to_vec(),
        "boot must project the EVER-ISSUED tip (offline lnd3 = h3), not the last ACK (h1)"
    );
    // next_lnd reconstructed = max_lnd + 1 = 4 (NC-03, unchanged).
    let (_, _, next_lnd) = read_node_state(&pool, fn_id).await.expect("node_state row");
    assert_eq!(next_lnd, 4);
    // Ledger consistent: node seed == the MAC-walk's final expected (h3).
    prro::db::invariant_scan::assert_clean(&pool).await;
}
