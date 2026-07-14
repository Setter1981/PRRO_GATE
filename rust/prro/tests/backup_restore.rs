//! RS-4 (audit pass-2, item 4) — backup / restore / retention acceptance.
//!
//! PR-A tests 1-7 (spec §A5).  Test 5 (restore-and-continue e2e) reuses the
//! kill-point fixture style: it proves a restored DB is just an old crash-state
//! that the already-proven recovery path converges.

mod common;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::db::backup::{self, prune, snapshot, verify_snapshot};
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::{RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::types::DbShiftId;
use prro::db::{open_pool, open_secure_pool};
use prro::services::offline_sync::backlog_drain;
use prro::services::reconciliation::{boot_phase, ReconcileGuard, RuntimeView};
use prro::services::write_path::inline;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use sqlx::SqlitePool;

use common::det_signing_ctx;

const FN: &str = "4000000001";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SERVER_FISCAL_NO: &str = "DPS-FN-ONLINE-1";
const SELL_PAYLOAD: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const TOTAL_KOP: i64 = 15000;

async fn fresh_db(name: &str) -> (tempfile::TempDir, PathBuf, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    let pool = open_pool(&path).await.unwrap();
    (dir, path, pool)
}

async fn seed_fn_config(pool: &SqlitePool, fnum: &str) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fnum)
    .execute(pool)
    .await
    .unwrap();
}

// ════════════════════════════════════════════════════════════════════════════
// Test 1 — snapshot a live DB with data: verified, owner-only, readable.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn snapshot_creates_verified_owner_only_copy() {
    let (_dir, db_path, pool) = fresh_db("live.db").await;
    seed_fn_config(&pool, "4000000001").await;

    let backup_dir = tempfile::tempdir().unwrap();
    let report = snapshot(&pool, &db_path, backup_dir.path(), "main")
        .await
        .expect("snapshot succeeds");

    assert!(report.verified, "report marks the snapshot verified");
    assert!(report.path.exists(), "snapshot file exists");
    assert!(report.bytes > 0, "snapshot is non-empty");
    let fname = report.path.file_name().unwrap().to_str().unwrap();
    assert!(
        fname.starts_with("main-"),
        "named by label convention: {fname}"
    );
    assert!(fname.ends_with(".db"));

    // Owner-only perms checked BEFORE any mutating re-open (unix).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&report.path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "snapshot must be owner-only (0600)");
    }

    // Integrity + content: the snapshot opens and carries our row.
    assert!(verify_snapshot(&report.path).await.unwrap());
    let snap = open_pool(&report.path).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_number_config WHERE fiscal_number = '4000000001'",
    )
    .fetch_one(&snap)
    .await
    .unwrap();
    assert_eq!(n, 1, "snapshot contains the seeded row");
    snap.close().await;
}

// ════════════════════════════════════════════════════════════════════════════
// Test 2 — snapshot under concurrent writes: both succeed, snapshot consistent.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn snapshot_under_concurrent_writes_is_consistent() {
    let (_dir, db_path, pool) = fresh_db("live.db").await;
    seed_fn_config(&pool, "4000000001").await;

    let backup_dir = tempfile::tempdir().unwrap();
    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move {
        for i in 0..300i64 {
            let _ = sqlx::query(
                "INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES ('4000000001', ?)",
            )
            .bind(i)
            .execute(&writer_pool)
            .await;
        }
    });

    let report = snapshot(&pool, &db_path, backup_dir.path(), "main")
        .await
        .expect("snapshot under concurrent writes succeeds");
    writer.await.unwrap();

    assert!(report.verified);
    assert!(
        verify_snapshot(&report.path).await.unwrap(),
        "VACUUM INTO snapshot is internally consistent despite concurrent writes"
    );
    let snap = open_pool(&report.path).await.unwrap();
    let fn_n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_number_config")
        .fetch_one(&snap)
        .await
        .unwrap();
    assert_eq!(
        fn_n, 1,
        "consistent point-in-time includes the committed seed"
    );
    snap.close().await;
}

// ════════════════════════════════════════════════════════════════════════════
// Test 3 — prune retention (architect ruling): a snapshot is KEPT only while
// BOTH within keep_last AND younger than max_age (delete on cap OR expiry).
// Two isolated calls pin each axis separately; foreign files untouched.
// ════════════════════════════════════════════════════════════════════════════

fn touch(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), b"x").unwrap();
}

fn snap_name(label: &str, dt: chrono::NaiveDateTime, hex: &str) -> String {
    format!("{label}-{}-{hex}.db", dt.format("%Y%m%d-%H%M%S"))
}

#[tokio::test]
async fn prune_caps_count_and_expires_age_and_skips_foreign() {
    let dir = tempfile::tempdir().unwrap();
    let now = chrono::Utc::now().naive_utc();

    // 4 young (0/1/2/3 days) + 1 expired-but-recent-ish (20 days) + 1 ancient
    // (1000 days).  Dates are relative to now so the test is date-independent.
    let days = [0i64, 1, 2, 3, 20, 1000];
    for (i, d) in days.iter().enumerate() {
        touch(
            dir.path(),
            &snap_name(
                "main",
                now - chrono::Duration::days(*d),
                &format!("0000000{}", i + 1),
            ),
        );
    }
    // Foreign files prune must NEVER touch: a different label + a non-snapshot.
    let foreign_secure = snap_name("secure", now - chrono::Duration::days(1000), "deadbeef");
    touch(dir.path(), &foreign_secure);
    touch(dir.path(), "operator-notes.txt");

    // ── Call A: loose cap (10) → ONLY the age axis acts: the 20d and 1000d
    //    snapshots are expired even though both are within keep_last.
    let report_a = prune(
        dir.path(),
        "main",
        /*keep_last*/ 10,
        /*max_age_days*/ 14,
    )
    .unwrap();
    assert_eq!(report_a.scanned, 6);
    assert_eq!(
        report_a.deleted, 2,
        "age expiry acts WITHIN the cap (20d + 1000d deleted)"
    );
    assert_eq!(report_a.kept, 4);

    // ── Call B: loose age (10000d) → ONLY the cap axis acts: of the 4 young
    //    survivors the 3-day-old one is beyond keep_last=3 and goes.
    let report_b = prune(
        dir.path(),
        "main",
        /*keep_last*/ 3,
        /*max_age_days*/ 10000,
    )
    .unwrap();
    assert_eq!(report_b.scanned, 4);
    assert_eq!(
        report_b.deleted, 1,
        "the cap deletes a YOUNG snapshot beyond keep_last"
    );
    assert_eq!(report_b.kept, 3);

    let remaining: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_str().unwrap().to_string())
        .collect();
    assert_eq!(
        remaining.iter().filter(|n| n.starts_with("main-")).count(),
        3,
        "exactly keep_last young main snapshots survive both passes"
    );
    assert!(
        remaining.contains(&foreign_secure),
        "different-label snapshot untouched"
    );
    assert!(
        remaining.iter().any(|n| n == "operator-notes.txt"),
        "non-snapshot file untouched"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Test 4 — verify-fail: a corrupt/garbage file fails verification.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn verify_rejects_corrupt_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let (_d, db_path, pool) = fresh_db("live.db").await;
    seed_fn_config(&pool, "4000000001").await;

    // A real snapshot verifies ok.
    let report = snapshot(&pool, &db_path, dir.path(), "main").await.unwrap();
    assert!(
        verify_snapshot(&report.path).await.unwrap(),
        "valid snapshot verifies ok"
    );

    // A garbage file (opens-fail / not-ok integrity) fails verification with
    // Ok(false) — the path `snapshot` uses to delete + error.  (Injecting a
    // mid-snapshot corruption to drive snapshot's own delete is the
    // spec-acknowledged awkward case; verify_snapshot is the tested seam.)
    let bogus = dir.path().join("main-20260101-000000-deadbeef.db");
    std::fs::write(&bogus, b"this is definitely not a sqlite database file").unwrap();
    assert!(
        !verify_snapshot(&bogus).await.unwrap(),
        "garbage file must fail verification"
    );
    let _ = backup::PruneReport::default(); // touch the re-export
}

// ─── Online-SELL fixture (mirror of tests/kill_point_matrix.rs) for test 5 ───

struct DpsStub {
    send_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    last_q: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    /// PR-B (tip-guard) — wire-call counters so the guard tests can assert
    /// "send == 0 on boot" (nothing re-sent) and "zero wire" (guard skipped /
    /// kill-switched) without relying on the empty-queue panic alone.
    send_calls: AtomicUsize,
    last_calls: AtomicUsize,
}

impl DpsStub {
    fn new() -> Self {
        Self {
            send_q: Mutex::new(VecDeque::new()),
            last_q: Mutex::new(VecDeque::new()),
            send_calls: AtomicUsize::new(0),
            last_calls: AtomicUsize::new(0),
        }
    }
    fn push_send(&self, r: Result<CheckAck, DpsError>) {
        self.send_q.lock().unwrap().push_back(r);
    }
    fn push_last(&self, r: Result<CheckAck, DpsError>) {
        self.last_q.lock().unwrap().push_back(r);
    }
    fn send_calls(&self) -> usize {
        self.send_calls.load(Ordering::SeqCst)
    }
    fn last_calls(&self) -> usize {
        self.last_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DpsChannel for DpsStub {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        self.send_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("send_q empty")
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_calls.fetch_add(1, Ordering::SeqCst);
        self.last_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("last_q empty")
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!()
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!()
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!()
    }

    async fn ask_offline_codes(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<
        prro::transports::dps::dto::OfflineCodesResponse,
        prro::transports::dps::error::DpsError,
    > {
        unreachable!("stub: ask_offline_codes not exercised");
    }
}

fn ack(id: &str, data_sign: Vec<u8>) -> CheckAck {
    CheckAck {
        id: id.to_string(),
        id_sign: vec![],
        data_sign,
    }
}

async fn seed_fn_config_full(pool: &SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
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

async fn seed_open_shift(pool: &SqlitePool) -> ShiftId {
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, ?)",
    )
    .bind(DbShiftId(shift_id))
    .bind(FN)
    .bind(CASHIER)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

async fn seed_node_online(pool: &SqlitePool, shift_id: ShiftId) {
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) VALUES (?, ?, ?, ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(NodeMode::Online.as_str())
    .bind(ShiftState::Opened.as_str())
    .bind(DbShiftId(shift_id))
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_inbox_sell(pool: &SqlitePool, n: i64) -> InboxRow {
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(SELL_PAYLOAD.as_bytes()).into();
    let idempotency_key = format!("idem-backup-{n}");
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: "SELL".into(),
            idempotency_key: idempotency_key.clone(),
            payload_json: SELL_PAYLOAD.into(),
            payload_sha256_canonical,
            correlation_id: None,
            signed_by_cashier_id: Some(CASHIER.into()),
            driver_id: Some(DRIVER.into()),
            business_ts: Some("2026-06-09T12:00:00Z".into()),
            total_sum_kop: Some(TOTAL_KOP),
        },
    )
    .await
    .unwrap();
    InboxRow {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: "SELL".into(),
        idempotency_key,
        status: "NEW".into(),
        payload_json: SELL_PAYLOAD.into(),
        payload_sha256_canonical,
        correlation_id: None,
        received_at: "2026-06-09T12:00:00Z".into(),
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-06-09T12:00:00Z".into()),
        total_sum_kop: Some(TOTAL_KOP),
    }
}

/// Drive one online SELL to terminal ACK via the real inline write-path
/// (send Ok + lastChk Match) — exactly the kill-matrix happy path.
async fn issue_receipt_to_ack(pool: &SqlitePool, pool_secure: &SqlitePool, n: i64) {
    let row = seed_inbox_sell(pool, n).await;
    let stub = DpsStub::new();
    stub.push_send(Ok(ack(SERVER_FISCAL_NO, vec![])));
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE; 64])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;
    let outcome = inline::run(
        pool,
        pool_secure,
        &stub,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await
    .unwrap_or_else(|e| panic!("receipt {n} must reach ACK: {e:?}"));
    assert_eq!(outcome.document_state, DocState::Ack, "receipt {n} → ACK");
}

fn recon_guard() -> ReconcileGuard<'static> {
    ReconcileGuard::for_integration_test_only()
}

// ════════════════════════════════════════════════════════════════════════════
// Test 5 (KEY) — restore-and-continue e2e: a restored DB is just an old
// crash-state the proven recovery path converges and continues from.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn restore_and_continue_e2e() {
    let (_dir, db_path, pool) = fresh_db("live.db").await;
    let sec_dir = tempfile::tempdir().unwrap();
    let pool_secure = open_secure_pool(&sec_dir.path().join("secure.db"))
        .await
        .unwrap();
    seed_fn_config_full(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_online(&pool, shift_id).await;

    // Receipt #1 → ACK, then snapshot (snapshot knows only #1).
    issue_receipt_to_ack(&pool, &pool_secure, 1).await;
    let backup_dir = tempfile::tempdir().unwrap();
    let report = snapshot(&pool, &db_path, backup_dir.path(), "main")
        .await
        .expect("snapshot after receipt #1");

    // Receipt #2 → ACK on the LIVE DB (the snapshot does NOT have it).
    issue_receipt_to_ack(&pool, &pool_secure, 2).await;
    let live_docs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(live_docs, 2, "live DB has both receipts");

    // ── "Disk death" → restore the snapshot to a NEW path (knows only #1).
    let restore_dir = tempfile::tempdir().unwrap();
    let restored_db = restore_dir.path().join("restored.db");
    std::fs::copy(&report.path, &restored_db).expect("copy snapshot into place");

    // open_pool (migrations idempotent) → boot reconcile → ledger is clean.
    let restored = open_pool(&restored_db).await.expect("open restored DB");
    let restored_docs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert_eq!(
        restored_docs, 1,
        "restored DB is the pre-#2 crash-state (only #1)"
    );

    boot_phase::run_boot_reconciliation(&recon_guard(), &restored, FN, None)
        .await
        .expect("boot reconcile on the restored DB succeeds");
    prro::db::invariant_scan::assert_clean(&restored).await;

    // Continue trading on the restored node — issue the next receipt to ACK.
    let sec_dir2 = tempfile::tempdir().unwrap();
    let restored_secure = open_secure_pool(&sec_dir2.path().join("secure.db"))
        .await
        .unwrap();
    issue_receipt_to_ack(&restored, &restored_secure, 3).await;
    prro::db::invariant_scan::assert_clean(&restored).await;

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiscal_documents WHERE state='ACK'")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert_eq!(
        after, 2,
        "restored DB now carries #1 + the freshly-issued receipt"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Test 6 — backup-pass isolation: a failing snapshot is swallowed, loop lives.
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn backup_pass_swallows_failure_and_continues() {
    let (_dir, db_path, pool) = fresh_db("live.db").await;
    seed_fn_config(&pool, "4000000001").await;

    // A "directory" UNDER a regular file can never be created/used → the
    // snapshot errors → the pass logs and returns None (never panics, never
    // propagates: F1).  (Unit on the tick body, not on spawn — spec test 6.)
    let bad_dir = db_path.join("not-a-dir");
    let out = backup::snapshot_and_prune(&pool, &db_path, &bad_dir, "main", 30, 14).await;
    assert!(
        out.is_none(),
        "failed snapshot swallowed (None); loop continues"
    );

    // A healthy pass still works right after the failed one.
    let good = tempfile::tempdir().unwrap();
    let ok = backup::snapshot_and_prune(&pool, &db_path, good.path(), "main", 30, 14).await;
    assert!(ok.is_some(), "a healthy pass produces a snapshot");
}

// ════════════════════════════════════════════════════════════════════════════
// PR-B — boot stale-tip guard (spec §B-3, tests 8-12).
//
// The guard runs per FN, AFTER `run_boot_reconciliation` (arm-ups), when
// runtime deps are present and the FN has a non-empty ACK tail.  It takes the
// last (max-lnd) ACK doc's `server_fiscal_no`, reuses `last_chk_probe::probe`
// against DPS, and on divergence flips `node_state.mode → BLOCKED` (existing
// W10.3 setter) + a CRITICAL `TIP_GUARD_STALE_LEDGER` audit.  It NEVER mutates
// document state and NEVER re-sends — detect + block + audit only.
// ════════════════════════════════════════════════════════════════════════════

/// Like [`issue_receipt_to_ack`] but with a caller-chosen DPS server fiscal
/// number, so a test can mint two ACK docs with DISTINCT `server_fiscal_no`
/// (needed to drive a tip-guard Mismatch after a stale restore).
async fn issue_receipt_to_ack_sfn(
    pool: &SqlitePool,
    pool_secure: &SqlitePool,
    n: i64,
    server_fiscal_no: &str,
) {
    let row = seed_inbox_sell(pool, n).await;
    let stub = DpsStub::new();
    // send + online-confirm lastChk both carry the same wire id, so the
    // online write-path advances the doc to terminal ACK with this sfn.
    stub.push_send(Ok(ack(server_fiscal_no, vec![])));
    stub.push_last(Ok(ack(server_fiscal_no, vec![0xDE; 64])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;
    let outcome = inline::run(
        pool,
        pool_secure,
        &stub,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await
    .unwrap_or_else(|e| panic!("receipt {n} (sfn {server_fiscal_no}) must reach ACK: {e:?}"));
    assert_eq!(outcome.document_state, DocState::Ack, "receipt {n} → ACK");
}

/// One online FN with an open shift, ready to issue receipts.  Keeps the
/// `TempDir` guards alive (drop = cleanup) for the test's lifetime.
struct GuardEnv {
    _dir: tempfile::TempDir,
    _sec_dir: tempfile::TempDir,
    db_path: PathBuf,
    pool: SqlitePool,
    pool_secure: SqlitePool,
}

async fn setup_online_fn(name: &str) -> GuardEnv {
    let (dir, db_path, pool) = fresh_db(name).await;
    let sec_dir = tempfile::tempdir().unwrap();
    let pool_secure = open_secure_pool(&sec_dir.path().join("secure.db"))
        .await
        .unwrap();
    seed_fn_config_full(&pool).await;
    let shift_id = seed_open_shift(&pool).await;
    seed_node_online(&pool, shift_id).await;
    GuardEnv {
        _dir: dir,
        _sec_dir: sec_dir,
        db_path,
        pool,
        pool_secure,
    }
}

async fn node_mode(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Flip the FN to `GoingOnline` — the AUD-L4-1 scenario is an FN recovering
/// from offline, the ONLY mode in which the offline drain runs
/// (`backlog_drain.rs:687`).
async fn set_node_going_online(pool: &SqlitePool) {
    sqlx::query("UPDATE node_state SET mode = ? WHERE fiscal_number = ?")
        .bind(NodeMode::GoingOnline.as_str())
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
}

async fn read_doc_state(pool: &SqlitePool, lnd: i64) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE fiscal_number = ? AND lnd = ?")
        .bind(FN)
        .bind(lnd)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_audit_like(pool: &SqlitePool, event_like: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type LIKE ?")
        .bind(event_like)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ────────────────────────────────────────────────────────────────────────────
// Test 8 (KEY) — stale-restore detect e2e: ACK#1 → snapshot → ACK#2 → restore
// (ledger knows only #1) → boot with lastChk = sfn#2 → Mismatch → BLOCKED,
// CRITICAL audit present, send == 0 (nothing re-sent).
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_stale_restore_blocks_node_e2e() {
    let env = setup_online_fn("live.db").await;

    // ACK#1 (sfn SFN-1) → snapshot (backup knows only #1).
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    let backup_dir = tempfile::tempdir().unwrap();
    let report = snapshot(&env.pool, &env.db_path, backup_dir.path(), "main")
        .await
        .expect("snapshot after receipt #1");

    // ACK#2 (sfn SFN-2) on the LIVE DB — DPS has now moved on to SFN-2.
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 2, "SFN-2").await;

    // "Disk death" → restore the snapshot to a NEW path (knows only SFN-1).
    let restore_dir = tempfile::tempdir().unwrap();
    let restored_db = restore_dir.path().join("restored.db");
    std::fs::copy(&report.path, &restored_db).expect("copy snapshot into place");
    let restored = open_pool(&restored_db).await.expect("open restored DB");

    // Boot: arm-ups first (the restored DB has no pending docs → branch (b),
    // zero wire), then the tip-guard with a stub whose lastChk reports SFN-2
    // (DPS is ahead of our restored tip SFN-1).
    let stub = DpsStub::new();
    stub.push_last(Ok(ack("SFN-2", vec![0xDE; 64])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    boot_phase::run_boot_reconciliation(&recon_guard(), &restored, FN, Some(&view))
        .await
        .expect("arm-ups succeed on the restored DB");
    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &restored, FN, &view, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::Blocked { .. }),
        "stale restore must classify as Blocked, got {outcome:?}"
    );
    assert_eq!(
        node_mode(&restored).await,
        "BLOCKED",
        "stale-restore detection must flip the node to BLOCKED"
    );
    assert_eq!(
        stub.send_calls(),
        0,
        "guard must NOT re-send anything on boot"
    );
    assert_eq!(
        stub.last_calls(),
        1,
        "guard issues exactly one lastChk probe"
    );
    let crit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'TIP_GUARD_STALE_LEDGER' AND severity = 'CRITICAL'",
    )
    .fetch_one(&restored)
    .await
    .unwrap();
    assert_eq!(
        crit, 1,
        "exactly one CRITICAL TIP_GUARD_STALE_LEDGER audit row"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 9 — happy tip: tail ACK#2 + lastChk = sfn#2 → Match → mode untouched,
// INFO audit.
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_happy_tip_match_leaves_node_untouched() {
    let env = setup_online_fn("live.db").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 2, "SFN-2").await;

    // DPS agrees: its latest check is SFN-2, matching our last (max-lnd) ACK.
    let stub = DpsStub::new();
    stub.push_last(Ok(ack("SFN-2", vec![])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::TipConsistent),
        "matching tip must classify as TipConsistent, got {outcome:?}"
    );
    assert_eq!(
        node_mode(&env.pool).await,
        "ONLINE",
        "a consistent tip must leave node mode untouched"
    );
    assert_eq!(stub.send_calls(), 0, "guard never sends");
    assert_eq!(
        stub.last_calls(),
        1,
        "guard issues exactly one lastChk probe"
    );
    let ok_info: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'TIP_GUARD_OK' AND severity = 'INFO'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(ok_info, 1, "exactly one INFO TIP_GUARD_OK audit row");
}

// ────────────────────────────────────────────────────────────────────────────
// Test 10 — DPS unavailable: lastChk → Transport error → node NOT blocked, WARN
// (offline-first: a boot without network must not block the till).
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_dps_unavailable_does_not_block() {
    let env = setup_online_fn("live.db").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;

    let stub = DpsStub::new();
    stub.push_last(Err(DpsError::Transport("connection refused".into())));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs even when DPS is down");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::ProbeDeferred { .. }),
        "DPS transport error must defer (not block), got {outcome:?}"
    );
    assert_eq!(
        node_mode(&env.pool).await,
        "ONLINE",
        "DPS-down on boot must NOT block the node (offline-first)"
    );
    assert_eq!(
        stub.last_calls(),
        1,
        "guard attempted exactly one lastChk probe"
    );
    let stale: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'TIP_GUARD_STALE_LEDGER'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        stale, 0,
        "transport failure must NOT raise a stale-ledger alarm"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 11 — fresh FN: zero ACK docs → zero wire (guard silently skipped).
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_fresh_fn_skips_with_zero_wire() {
    let env = setup_online_fn("live.db").await;
    // No receipts issued → no ACK tail.

    // Empty queues: any wire call would panic ("…_q empty") — a strict
    // zero-wire assertion.
    let stub = DpsStub::new();
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs on a fresh FN");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::SkippedNoAckTail),
        "fresh FN must skip the guard, got {outcome:?}"
    );
    assert_eq!(stub.last_calls(), 0, "fresh FN: zero lastChk wire calls");
    assert_eq!(stub.send_calls(), 0, "fresh FN: zero send wire calls");
    assert_eq!(
        count_audit_like(&env.pool, "TIP_GUARD%").await,
        0,
        "fresh FN: no guard audit rows"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 12 — kill-switch: tip_guard_enabled = false → zero wire, zero guard
// audits, even with a non-empty ACK tail present.
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_kill_switch_disables_all_wire_and_audit() {
    let env = setup_online_fn("live.db").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;

    // Non-empty ACK tail exists, but the kill-switch must short-circuit BEFORE
    // any DB tail read or wire call.  Empty queues = strict zero-wire assertion.
    let stub = DpsStub::new();
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, false)
        .await
        .expect("tip-guard short-circuits when disabled");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::Disabled),
        "kill-switch must classify as Disabled, got {outcome:?}"
    );
    assert_eq!(stub.last_calls(), 0, "kill-switch: zero lastChk wire calls");
    assert_eq!(stub.send_calls(), 0, "kill-switch: zero send wire calls");
    assert_eq!(
        node_mode(&env.pool).await,
        "ONLINE",
        "kill-switch leaves node mode untouched"
    );
    assert_eq!(
        count_audit_like(&env.pool, "TIP_GUARD%").await,
        0,
        "kill-switch: zero guard audit rows"
    );
}

/// Raw-seed one in-flight KVT1 doc with a chosen `server_fiscal_no` and `lnd`
/// (kill-matrix direct-insert style).  Used by test 13 to place a fresh
/// in-flight doc NEWER than an earlier ACK, so the guard's "newest submitted"
/// expected tip is the in-flight doc rather than the last ACK.
async fn seed_inflight_kvt1(pool: &SqlitePool, lnd: i64, server_fiscal_no: &str) {
    let doc_bytes = vec![0xC1u8; 16];
    let req_bytes = vec![0xC2u8; 16];
    let canonical_sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, server_fiscal_no) \
         VALUES (?, ?, ?, ?, 'SELL', 'KVT1', 'b', 't', 'ONLINE', '2026-01-01T00:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(FN)
    .bind(lnd)
    .bind(&canonical_sha)
    .bind(server_fiscal_no)
    .execute(pool)
    .await
    .unwrap();
}

// ────────────────────────────────────────────────────────────────────────────
// Test 13 (architect ruling 3) — in-flight tip is CONSISTENT, not stale.
// The expected tip is the newest SUBMITTED doc (max-lnd in {SENT,KVT1,KVT2,
// ACK}), so a fresh in-flight KVT1 newer than the last ACK is DPS's legitimate
// latest check — NOT a stale-ledger BLOCK.  (With the old ACK-only comparison
// this case false-BLOCKed; this is the regression test for the fix.)
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_inflight_tip_is_consistent_not_stale() {
    let env = setup_online_fn("live.db").await;
    // ACK#1 (lnd=1, sfn SFN-1) via the proven online path.
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    // A FRESH in-flight KVT1#2 (lnd=2, sfn SFN-2) — newer than the ACK; this is
    // DPS's legitimate latest check (crash mid-finalize / boot Match-advance).
    seed_inflight_kvt1(&env.pool, 2, "SFN-2").await;

    // DPS's lastChk reports SFN-2 — our newest SUBMITTED doc, not the last ACK.
    let stub = DpsStub::new();
    stub.push_last(Ok(ack("SFN-2", vec![])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::TipConsistent),
        "a fresh in-flight tip matching DPS must be consistent, got {outcome:?}"
    );
    assert_eq!(
        node_mode(&env.pool).await,
        "ONLINE",
        "a legitimate in-flight tip must NOT block a healthy node"
    );
    assert_eq!(
        stub.last_calls(),
        1,
        "guard issues exactly one lastChk probe"
    );
    let crit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'TIP_GUARD_STALE_LEDGER'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        crit, 0,
        "no stale-ledger alarm for a legitimate in-flight tip"
    );
    let ok: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = 'TIP_GUARD_OK'")
            .fetch_one(&env.pool)
            .await
            .unwrap();
    assert_eq!(ok, 1, "exactly one INFO TIP_GUARD_OK audit row");
}

// ────────────────────────────────────────────────────────────────────────────
// Test 14 (architect ruling 4(b)) — degraded-mode skip.  A node in STOP_MODE
// (non-trading) gets NO guard: zero wire, mode unchanged (no BLOCKED overwrite
// that would clobber the operator-relevant degradation context).
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_skips_degraded_mode_node() {
    let env = setup_online_fn("live.db").await;
    // Non-empty ACK tail present — proves the skip is mode-driven, not tail-driven.
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    sqlx::query("UPDATE node_state SET mode = 'STOP_MODE' WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&env.pool)
        .await
        .unwrap();

    // Empty queues — any wire call would panic ("…_q empty"): strict zero-wire.
    let stub = DpsStub::new();
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::SkippedMode { mode } if mode == NodeMode::StopMode),
        "degraded mode must skip the guard, got {outcome:?}"
    );
    assert_eq!(
        stub.last_calls(),
        0,
        "degraded mode: zero lastChk wire calls"
    );
    assert_eq!(stub.send_calls(), 0, "degraded mode: zero send wire calls");
    assert_eq!(
        node_mode(&env.pool).await,
        "STOP_MODE",
        "the guard must NOT overwrite the degraded mode"
    );
    assert_eq!(
        count_audit_like(&env.pool, "TIP_GUARD%").await,
        0,
        "degraded mode: zero guard audit rows"
    );
}

/// Raw-seed one in-flight SENT doc with a NULL `server_fiscal_no` (a malformed
/// tail).  Unreachable in healthy code — `SENT ⇐ WireDecision::Sent` stamps the
/// sfn — but exactly what a restored/corrupted DB can carry.  NC-04.  The
/// `server_fiscal_no` column is omitted from the INSERT so it defaults to NULL.
async fn seed_inflight_sent_null_sfn(pool: &SqlitePool, lnd: i64) {
    let doc_bytes = vec![0xD1u8; 16];
    let req_bytes = vec![0xD2u8; 16];
    let canonical_sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENT', 'b', 't', 'ONLINE', '2026-01-01T00:00:00Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(FN)
    .bind(lnd)
    .bind(&canonical_sha)
    .execute(pool)
    .await
    .unwrap();
}

// ────────────────────────────────────────────────────────────────────────────
// Test 14 (NC-04) — malformed tail: the newest {SENT,KVT1,KVT2} doc by lnd has a
// NULL server_fiscal_no.  `last_submitted_server_fiscal_no` filters NULL/'' sfn,
// so it would skip the malformed lnd=2 row and validate the OLDER ACK (lnd=1) as
// TIP_GUARD_OK — even though the real max-lnd submitted doc is unrecoverable.
// The guard must treat this as a block-worthy anomaly (BLOCKED + CRITICAL),
// BEFORE/regardless of the probe.
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tip_guard_malformed_tail_null_sfn_blocks() {
    let env = setup_online_fn("live.db").await;
    // ACK#1 (lnd=1, sfn SFN-1) — a valid OLDER tail via the proven online path.
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    // A NEWER malformed SENT#2 (lnd=2) with NULL sfn — last_submitted skips it.
    seed_inflight_sent_null_sfn(&env.pool, 2).await;

    // lastChk would Match the older valid tip (SFN-1) → without NC-04 the guard
    // returns TIP_GUARD_OK.  The malformed newer tail must block first.
    let stub = DpsStub::new();
    stub.push_last(Ok(ack("SFN-1", vec![0xDE; 64])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::Blocked { .. }),
        "newest {{SENT,KVT1,KVT2}} doc with NULL sfn is a block-worthy malformed tail, got {outcome:?}"
    );
    assert_eq!(
        node_mode(&env.pool).await,
        "BLOCKED",
        "malformed tail must BLOCK the node"
    );
    let stale: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'TIP_GUARD_STALE_LEDGER' AND severity = 'CRITICAL'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        stale, 1,
        "exactly one CRITICAL stale-ledger audit for the malformed tail"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 15 (NC-03, FT) — node_state row LOST while the fiscal_documents ledger
// SURVIVED (partial restore / corruption / rebaseline).  Boot branch (a) must
// NOT silently bootstrap a fresh FN (next_lnd=1, genesis seed) — that resets the
// LND allocator (which reads node_state.next_lnd) → duplicate lnd / forked MAC
// chain.  Instead: reconstruct next_lnd = MAX(lnd)+1, project the MAC seed from
// the last ACK, and BLOCK + CRITICAL (a node whose node_state vanished is not
// healthy).  Pin = the candidate SQL repro + invariant_scan post-condition.
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn boot_branch_a_ledger_survived_node_state_lost_blocks_and_reconstructs() {
    let env = setup_online_fn("live.db").await;
    // Two ACKed receipts via the proven online path → ledger has lnd 1,2 with
    // real unsigned_xml_sha256; node_state.next_lnd advanced to 3, seed = doc2's
    // unsigned hash.
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 2, "SFN-2").await;

    // Capture the ledger truth BEFORE deleting node_state.
    let max_lnd: i64 =
        sqlx::query_scalar("SELECT MAX(lnd) FROM fiscal_documents WHERE fiscal_number = ?")
            .bind(FN)
            .fetch_one(&env.pool)
            .await
            .unwrap();
    let last_ack_seed: Vec<u8> = sqlx::query_scalar(
        "SELECT unsigned_xml_sha256 FROM fiscal_documents \
         WHERE fiscal_number = ? AND state = 'ACK' ORDER BY lnd DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_one(&env.pool)
    .await
    .unwrap();

    // SIMULATE the candidate: node_state row LOST, ledger survives.
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&env.pool)
        .await
        .unwrap();
    assert!(
        prro::db::repositories::node_state::get(&env.pool, FN)
            .await
            .unwrap()
            .is_none(),
        "precondition: node_state row deleted"
    );

    // Boot reconciliation (ctx-free — branch (a) needs no deps).
    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &env.pool, FN, None)
        .await
        .expect("boot reconciliation must succeed");

    // (1) NOT a silent bootstrap — reconstructed + BLOCKED.
    assert!(
        matches!(
            outcome,
            boot_phase::BranchOutcome::BlockedLedgerWithoutNodeState
        ),
        "ledger-survived-node_state-lost must BLOCK, not Bootstrap; got {outcome:?}"
    );
    let ns = prro::db::repositories::node_state::get(&env.pool, FN)
        .await
        .unwrap()
        .expect("node_state row recreated");
    assert_eq!(
        ns.mode,
        NodeMode::Blocked,
        "node BLOCKED — must not silently resume trading"
    );
    // (2) Allocator reconstructed, NOT reset to 1.
    assert_eq!(
        ns.next_lnd,
        max_lnd + 1,
        "next_lnd = MAX(lnd)+1 (the allocator reads node_state.next_lnd; a reset to 1 would dup lnd)"
    );
    // (3) MAC seed projected from the last ACK (not left NULL → no forked chain).
    assert_eq!(
        ns.last_known_unsigned_xml_sha256
            .as_ref()
            .map(|h| h.to_vec()),
        Some(last_ack_seed.clone()),
        "MAC seed projected from the last ACK's unsigned_xml_sha256"
    );
    // (4) CRITICAL audit.
    let crit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED' AND severity = 'CRITICAL'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        crit, 1,
        "exactly one CRITICAL ledger-without-node_state audit"
    );
    // (5) SEAM-D-1: the surviving OPEN shift (seeded OPENED by setup_online_fn,
    // survived the node_state delete) must be SURFACED in the audit payload so the
    // operator reconciles it before unblocking — NOT silently masked by the
    // Closed reconstruction.
    let payload_json: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED' LIMIT 1",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap();
    let sos = &payload["surviving_open_shift"];
    assert!(
        !sos.is_null(),
        "surviving OPEN shift must be surfaced (not masked); payload={payload}"
    );
    assert_eq!(
        sos["state"].as_str(),
        Some("OPENED"),
        "surviving shift state surfaced"
    );
    let surviving_id_hex: String = sqlx::query_scalar(
        "SELECT lower(hex(shift_id)) FROM shifts WHERE fiscal_number = ? AND state = 'OPENED'",
    )
    .bind(FN)
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        sos["shift_id"].as_str(),
        Some(surviving_id_hex.as_str()),
        "surfaced shift_id matches the surviving OPENED shifts row"
    );
    // (6) invariant_scan post-condition.
    prro::db::invariant_scan::assert_clean(&env.pool).await;
}

// ────────────────────────────────────────────────────────────────────────────
// Test 15b (SEAM-D-1) — ledger survived but the shift was already CLOSED before
// the node_state loss: branch (a) still BLOCKs/reconstructs, and the audit
// payload's `surviving_open_shift` is NULL (nothing to reconcile — no false
// positive that would send the operator chasing a closed shift).
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn boot_branch_a_ledger_survived_no_open_shift_surfaces_null() {
    let env = setup_online_fn("live.db").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    // The shift was closed before the crash → no active shift survives.
    sqlx::query("UPDATE shifts SET state = 'CLOSED' WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&env.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&env.pool)
        .await
        .unwrap();

    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &env.pool, FN, None)
        .await
        .expect("boot reconciliation must succeed");
    assert!(
        matches!(
            outcome,
            boot_phase::BranchOutcome::BlockedLedgerWithoutNodeState
        ),
        "ledger survived → still BLOCK; got {outcome:?}"
    );
    let payload_json: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED' LIMIT 1",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap();
    assert!(
        payload["surviving_open_shift"].is_null(),
        "no active shift → surviving_open_shift must be null; payload={payload}"
    );
    prro::db::invariant_scan::assert_clean(&env.pool).await;
}

// ────────────────────────────────────────────────────────────────────────────
// Test 16 (NC-03) — genuinely fresh FN (empty ledger): branch (a) behaviour is
// UNCHANGED — bootstrap Online, next_lnd=1, INFO audit (no false BLOCK).
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn boot_branch_a_fresh_fn_empty_ledger_bootstraps_unchanged() {
    let env = setup_online_fn("live.db").await;
    // No receipts → empty ledger.  Delete the node_state seeded by setup so
    // branch (a) fires on a truly fresh FN.
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .execute(&env.pool)
        .await
        .unwrap();

    let outcome = boot_phase::run_boot_reconciliation(&recon_guard(), &env.pool, FN, None)
        .await
        .expect("boot reconciliation must succeed");

    assert!(
        matches!(outcome, boot_phase::BranchOutcome::Bootstrapped),
        "empty ledger → fresh-FN bootstrap (unchanged); got {outcome:?}"
    );
    let ns = prro::db::repositories::node_state::get(&env.pool, FN)
        .await
        .unwrap()
        .expect("node_state bootstrapped");
    assert_eq!(ns.mode, NodeMode::Online, "fresh FN bootstraps Online");
    assert_eq!(ns.next_lnd, 1, "fresh FN allocator starts at 1");
    let blocked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'BOOT_LEDGER_WITHOUT_NODE_STATE_BLOCKED'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        blocked, 0,
        "fresh FN: no false ledger-without-node_state BLOCK"
    );
}

// ─── Batch B (AUD-L4-1) — tip-guard NotFound on a non-ACK in-flight SENT tip ──
//     must DEFER to the drain's safe-redrive, not BLOCK the FN.

/// Seed an in-flight offline-origin doc (sfn set) at `state` — the AUD-L4-1
/// scenario: the FN's newest SUBMITTED tip is a non-ACK in-flight doc.
async fn seed_inflight_offline_tip(
    pool: &SqlitePool,
    lnd: i64,
    sfn: &str,
    session: &[u8],
    state: &str,
) {
    let doc_bytes = vec![(lnd as u8) ^ 0x70; 16];
    let req_bytes = vec![(lnd as u8) ^ 0x71; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, server_fiscal_no, offline_fiscal_no, offline_session_id) \
         VALUES (?, ?, ?, ?, 'SELL', ?, 'b', 't', 'OFFLINE', '2026-01-01T00:00:00Z', '{}', \
            ?, ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(FN)
    .bind(lnd)
    .bind(state)
    .bind(vec![0u8; 32])
    .bind(sfn)
    .bind(lnd)
    .bind(session)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed an active OPEN offline session for `FN`.
async fn seed_open_offline_session(pool: &SqlitePool, session: &[u8]) {
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-01-01T00:00:00Z')",
    )
    .bind(session)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn tip_guard_inflight_sent_notfound_defers_to_drain() {
    let env = setup_online_fn("live.db").await;
    // ACK#1 (lnd=1, sfn SFN-1) via the proven online path.
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    // The AUD-L4-1 scenario: an FN RECOVERING from offline (GoingOnline — the
    // ONLY mode in which the drain runs) with an active offline session and a
    // NON-ACK in-flight offline-origin SENT tip (lnd=2, sfn SFN-2), the FN's
    // newest SUBMITTED doc.
    set_node_going_online(&env.pool).await;
    let session = vec![0x5eu8; 16];
    seed_open_offline_session(&env.pool, &session).await;
    seed_inflight_offline_tip(&env.pool, 2, "SFN-2", &session, "SENT").await;

    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);

    // ── Phase 1: boot tip-guard. lastChk(SFN-2) → NotFound (the in-flight SENT
    //    doc never landed at DPS). AUD-L4-1: this MUST DEFER, not BLOCK. ──
    let stub_tg = DpsStub::new();
    stub_tg.push_last(Err(DpsError::NotFound));
    let view_tg = RuntimeView {
        dps: &stub_tg,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view_tg, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(
            outcome,
            boot_phase::TipGuardOutcome::DeferredInFlightSent { .. }
        ),
        "NotFound on an in-flight SENT tip MUST defer, got {outcome:?}"
    );
    // Crucially the node stays GoingOnline (NOT BLOCKED) — so the drain that
    // owns the safe-redrive is NOT gated out.  (The AUD-L4-1 wedge was: tip-guard
    // BLOCKs first → the drain's GoingOnline mode-gate then skips the FN forever.)
    assert_eq!(
        node_mode(&env.pool).await,
        "GOING_ONLINE",
        "an in-flight SENT tip NotFound must NOT block (defer to drain safe-redrive)"
    );
    let stale: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'TIP_GUARD_STALE_LEDGER'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(stale, 0, "no stale-ledger BLOCK for an in-flight SENT tip");
    let deferred: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'TIP_GUARD_DEFERRED_INFLIGHT'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(deferred, 1, "one TIP_GUARD_DEFERRED_INFLIGHT audit");

    // ── Phase 2: the drain now OWNS the deferred SENT doc.  lastChk(SFN-2) →
    //    NotFound → HIGH-C5-3 safe-redrive (Sent → ER).  This proves the defer
    //    enables recovery END-TO-END — no permanent wedge (spec §B pin). ──
    let stub_drain = DpsStub::new();
    stub_drain.push_last(Err(DpsError::NotFound));
    let view_drain = RuntimeView {
        dps: &stub_drain,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };
    backlog_drain::drain(&recon_guard(), &env.pool, &view_drain, FN)
        .await
        .expect("drain runs");

    // The drain was NOT skipped (mode stayed GoingOnline after the defer) ...
    let skipped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'OFFLINE_DRAIN_SKIPPED_NOT_GOING_ONLINE'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(skipped, 0, "drain must NOT be gated out after the defer");
    // ... and safe-redrove the deferred SENT doc to ERROR_RETRYABLE (next tick
    // re-sends it via Pattern-B) — the SENT doc is no longer stranded.
    assert_eq!(
        read_doc_state(&env.pool, 2).await,
        "ERROR_RETRYABLE",
        "the deferred SENT doc safe-redrives (Sent → ER), proving no wedge"
    );
}

#[tokio::test]
async fn tip_guard_ack_tail_notfound_still_blocks() {
    // Preserve the real stale-ledger guard: an ACK tail (no in-flight SENT) +
    // NotFound is a genuine divergence (DPS lost a doc we confirmed) → BLOCK.
    let env = setup_online_fn("live.db").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;

    let stub = DpsStub::new();
    stub.push_last(Err(DpsError::NotFound));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::Blocked { .. }),
        "NotFound on an ACK tail (no in-flight SENT) MUST still BLOCK, got {outcome:?}"
    );
    assert_eq!(node_mode(&env.pool).await, "BLOCKED");
    let crit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'TIP_GUARD_STALE_LEDGER' AND severity = 'CRITICAL'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        crit, 1,
        "genuine ACK-tail NotFound still raises the stale-ledger block"
    );
}

#[tokio::test]
async fn tip_guard_kvt1_tail_notfound_still_blocks() {
    // The defer is SENT-only.  A KVT1 in-flight tip was ALREADY confirmed once by
    // DPS, so a later lastChk NotFound is a genuine divergence — the drain's
    // Kvt1Reentry path treats it as fatal StructuralDrift (NOT the safe-redrive),
    // so the tip-guard must BLOCK, not defer.  Locks the no-over-defer boundary.
    let env = setup_online_fn("live.db").await;
    issue_receipt_to_ack_sfn(&env.pool, &env.pool_secure, 1, "SFN-1").await;
    set_node_going_online(&env.pool).await;
    let session = vec![0x5eu8; 16];
    seed_open_offline_session(&env.pool, &session).await;
    seed_inflight_offline_tip(&env.pool, 2, "SFN-2", &session, "KVT1").await;

    let stub = DpsStub::new();
    stub.push_last(Err(DpsError::NotFound));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let view = RuntimeView {
        dps: &stub,
        signing_ctx: &sign_ctx,
        fn_sign: &fn_sign,
    };

    let outcome = boot_phase::run_boot_tip_guard(&recon_guard(), &env.pool, FN, &view, true)
        .await
        .expect("tip-guard runs");

    assert!(
        matches!(outcome, boot_phase::TipGuardOutcome::Blocked { .. }),
        "NotFound on a KVT1 (confirmed-once) tip MUST still BLOCK, got {outcome:?}"
    );
    assert_eq!(node_mode(&env.pool).await, "BLOCKED");
    let crit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'TIP_GUARD_STALE_LEDGER' AND severity = 'CRITICAL'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(
        crit, 1,
        "a KVT1-tail NotFound still raises the stale-ledger block"
    );
    let deferred: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE event_type = 'TIP_GUARD_DEFERRED_INFLIGHT'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(deferred, 0, "KVT1 is NOT deferred (defer is SENT-only)");
}
