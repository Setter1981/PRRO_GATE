//! RS-4 (audit pass-2, item 4) — backup / restore / retention acceptance.
//!
//! PR-A tests 1-7 (spec §A5).  Test 5 (restore-and-continue e2e) reuses the
//! kill-point fixture style: it proves a restored DB is just an old crash-state
//! that the already-proven recovery path converges.

mod common;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::db::backup::{self, prune, snapshot, verify_snapshot};
use prro::db::models::enums::{DocState, FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::{RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{self as inbox, InboxRow, NewInboxEntry};
use prro::db::repositories::{fiscal_number_config as fn_repo, fiscal_number_config::NewFnConfig};
use prro::db::{open_pool, open_secure_pool};
use prro::services::reconciliation::{boot_phase, ReconcileGuard};
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
}

impl DpsStub {
    fn new() -> Self {
        Self {
            send_q: Mutex::new(VecDeque::new()),
            last_q: Mutex::new(VecDeque::new()),
        }
    }
    fn push_send(&self, r: Result<CheckAck, DpsError>) {
        self.send_q.lock().unwrap().push_back(r);
    }
    fn push_last(&self, r: Result<CheckAck, DpsError>) {
        self.last_q.lock().unwrap().push_back(r);
    }
}

#[async_trait]
impl DpsChannel for DpsStub {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_q
            .lock()
            .unwrap()
            .pop_front()
            .expect("send_q empty")
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
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
    .bind(shift_id)
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
    .bind(NodeMode::Online)
    .bind(ShiftState::Opened)
    .bind(shift_id)
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
    stub.push_last(Ok(ack(SERVER_FISCAL_NO, vec![0xDE, 0xAD, 0xBE, 0xEF])));
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;
    let outcome = inline::run(pool, pool_secure, &stub, &sign_ctx, &fn_sign, &guard, &row)
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
