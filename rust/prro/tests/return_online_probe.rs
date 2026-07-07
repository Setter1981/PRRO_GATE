//! W8 acceptance — return-online detection probe.
//!
//! Covers the 8 W8 review axes (memory `m3b-w8-review-criteria`),
//! the operator-pinned 6 hard lines, and the negative scanner for
//! fiscal side effects (design freeze §10).
//!
//! Test inventory:
//!
//!   1. `probe_success_flips_offline_to_going_online` — happy
//!      path; mode flip + SUCCESS audit + DPS snapshot in payload.
//!   2. `probe_failure_dps_error_keeps_mode_and_audits_typed_class`
//!      — DPS Err → mode unchanged + FAILED audit with
//!      `dps_error_class`.
//!   3. `probe_failure_dps_reports_offline_keeps_mode_and_audits_snapshot`
//!      — DPS Ok(online=false) → mode unchanged + FAILED audit
//!      with `dps_snapshot` + `reason: "dps_reports_offline"`.
//!   4. `probe_idempotent_on_going_online_no_write_no_audit_no_wire`
//!      — operator hard line 4: GoingOnline + success = no-op.
//!      Stub panics on any call → proves wire skipped too.
//!   5. `probe_skips_online_mode_before_wire_call` — operator hard
//!      line 5: Online → skip BEFORE wire.  Stub panics on any
//!      call.
//!   6. `probe_no_fiscal_side_effects_on_success_failure_or_skip`
//!      — operator-pinned negative scanner: 3 paths (success /
//!      failure / skip); assert fiscal_documents + offline_sessions
//!      + offline_codes + transport_trace UNCHANGED.
//!   7. `spawn_probe_loop_respects_shutdown_signal` — boot-level
//!      task lifecycle; shutdown signal → clean task exit (I9).
//!   8. `probe_cas_miss_on_concurrent_mode_change_audits_failure`
//!      — concurrent mode flip after step-1 read → CAS miss in
//!      step 5 → FAILED audit with `cas_miss_concurrent_mode_change`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use prro::db::models::enums::{NodeMode, ShiftState};
use prro::services::offline_sync::return_online_probe::{
    self, FailureReason, ProbeSpec, SkipReason, TickOutcome,
};
use prro::transports::dps::dto::{CheckSignBlob, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use sqlx::SqlitePool;
use tokio::sync::watch;

const FN: &str = "1234567890";

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("w8.db"))
        .await
        .expect("open_pool");
    (dir, pool)
}

async fn seed_fn_config(pool: &SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state(pool: &SqlitePool, fn_id: &str, mode: NodeMode, shift: ShiftState) {
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, ?, 1)",
    )
    .bind(fn_id)
    .bind(mode)
    .bind(shift)
    .execute(pool)
    .await
    .unwrap();
}

async fn read_node_mode(pool: &SqlitePool, fn_id: &str) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(fn_id)
        .fetch_one(pool)
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

async fn audit_payload(pool: &SqlitePool, event_type: &str) -> Option<serde_json::Value> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log WHERE event_type = ? ORDER BY audit_id DESC LIMIT 1",
    )
    .bind(event_type)
    .fetch_optional(pool)
    .await
    .unwrap();
    raw.and_then(|s| serde_json::from_str(&s).ok())
}

fn fn_sign() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD, 0xEF, 0x12])
}

/// Content snapshot across fiscal-data tables.  Used by the
/// negative scanner (test #6) to assert the probe has zero side
/// effects on these tables — both row counts AND row contents for
/// the **selected critical columns** listed below.  Plain row
/// counts would miss accidental UPDATEs that preserve row count
/// (e.g. a state column flipped on an existing fiscal_document);
/// the per-row digest string of these columns catches that class
/// of regression.
///
/// **Scope of guarantee.**  The snapshot is column-selective, not
/// full-row byte equality.  We capture the columns the probe is
/// most plausibly tempted to mutate (state, lnd, fs_mode, the
/// payload identity columns; offline_session state + timestamps;
/// offline_code consumption marker; transport_trace start time +
/// completion / retry_class).  Columns NOT in the SELECT (e.g.
/// fiscal_documents.created_at, transport_trace.error_message)
/// are NOT covered — drift there must be caught by a future
/// scanner upgrade.  This is honest about what test #6 proves.
///
/// Each table contributes a `Vec<String>` of identifying + content
/// columns, ordered by primary key.  Two snapshots compare equal
/// iff every captured column on every row matches.
#[derive(Debug, PartialEq, Eq)]
struct FiscalTableSnapshot {
    fiscal_documents: Vec<String>,
    offline_sessions: Vec<String>,
    offline_codes: Vec<String>,
    transport_trace: Vec<String>,
}

type FiscalDocRow = (Vec<u8>, String, i64, String, String, Vec<u8>);
type TransportTraceRow = (Vec<u8>, i64, String, String, String, String, String);

async fn fiscal_table_snapshot(pool: &SqlitePool) -> FiscalTableSnapshot {
    // fiscal_documents: identify + state-bearing columns.  Probe
    // is forbidden from touching state, business_ts, payload_json,
    // payload_sha256_canonical — so capturing those proves a clean
    // negative scanner.
    let fd: Vec<FiscalDocRow> = sqlx::query_as(
        "SELECT document_id, state, lnd, fs_mode, payload_json, payload_sha256_canonical \
         FROM fiscal_documents ORDER BY document_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let fd = fd
        .into_iter()
        .map(|(id, st, lnd, fs, pj, sha)| {
            format!(
                "{}:{}:{}:{}:{}:{}",
                hex_lower(&id),
                st,
                lnd,
                fs,
                pj,
                hex_lower(&sha)
            )
        })
        .collect();

    let os: Vec<(Vec<u8>, String, String, Option<String>)> = sqlx::query_as(
        "SELECT offline_session_id, state, opened_at, COALESCE(closed_at, '') \
         FROM offline_sessions ORDER BY offline_session_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let os = os
        .into_iter()
        .map(|(id, st, op, cl)| {
            format!(
                "{}:{}:{}:{}",
                hex_lower(&id),
                st,
                op,
                cl.unwrap_or_default()
            )
        })
        .collect();

    let oc: Vec<(String, i64, Option<String>)> = sqlx::query_as(
        "SELECT fiscal_number, code_lnd, consumed_at \
         FROM offline_codes ORDER BY fiscal_number, code_lnd",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let oc = oc
        .into_iter()
        .map(|(fnum, cl, ca)| format!("{}:{}:{}", fnum, cl, ca.unwrap_or_default()))
        .collect();

    // transport_trace: identifying PK + start time + the W7
    // backend/transport profile context + completion / retry
    // markers.  Columns match migration 010 (+ 012 retry_class).
    let tt: Vec<TransportTraceRow> = sqlx::query_as(
        "SELECT document_id, attempt_no, started_at, backend_profile_id, transport_profile_id, \
                COALESCE(outcome_kind, ''), COALESCE(retry_class, '') \
         FROM transport_trace ORDER BY document_id, attempt_no",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let tt = tt
        .into_iter()
        .map(|(id, n, sa, bp, tp, ok, rc)| {
            format!(
                "{}:{}:{}:{}:{}:{}:{}",
                hex_lower(&id),
                n,
                sa,
                bp,
                tp,
                ok,
                rc
            )
        })
        .collect();

    FiscalTableSnapshot {
        fiscal_documents: fd,
        offline_sessions: os,
        offline_codes: oc,
        transport_trace: tt,
    }
}

fn hex_lower(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

// ─── 1. Happy path ──────────────────────────────────────────────────

#[tokio::test]
async fn probe_success_flips_offline_to_going_online() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Closed).await;
    let stub = StubDpsChannel::with_status(StatusSnapshot {
        open_shift: false,
        online: true,
        last_signer: "op-1".into(),
    });
    let signer = fn_sign();
    let outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
        .await
        .unwrap();
    match outcome {
        TickOutcome::Success { fiscal_number } => assert_eq!(fiscal_number, FN),
        other => panic!("expected Success, got: {other:?}"),
    }
    assert_eq!(read_node_mode(&pool, FN).await, "GOING_ONLINE");
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_ATTEMPT").await, 1);
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_SUCCESS").await, 1);
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_FAILED").await, 0);
    // Audit payload includes DPS snapshot fields (operator hard line 2).
    let payload = audit_payload(&pool, "RETURN_ONLINE_PROBE_SUCCESS")
        .await
        .unwrap();
    assert_eq!(payload["observed_mode_pre"], "OFFLINE");
    assert_eq!(payload["observed_mode_post"], "GOING_ONLINE");
    assert_eq!(payload["dps_online"], true);
    assert_eq!(payload["dps_open_shift"], false);
    assert_eq!(payload["dps_last_signer"], "op-1");
}

// ─── 2. DPS error failure ───────────────────────────────────────────

#[tokio::test]
async fn probe_failure_dps_error_keeps_mode_and_audits_typed_class() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Closed).await;
    let stub =
        StubDpsChannel::with_status_result(Err(DpsError::Transport("test-transient".into())));
    let signer = fn_sign();
    let outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        TickOutcome::Failed {
            reason: FailureReason::DpsError,
            ..
        }
    ));
    // Mode UNCHANGED.
    assert_eq!(read_node_mode(&pool, FN).await, "OFFLINE");
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_ATTEMPT").await, 1);
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_SUCCESS").await, 0);
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_FAILED").await, 1);
    let payload = audit_payload(&pool, "RETURN_ONLINE_PROBE_FAILED")
        .await
        .unwrap();
    assert_eq!(payload["reason"], "dps_error");
    // Stable taxonomy: exact-string match, not Debug substring.
    assert_eq!(payload["dps_error_class"], "Transport");
    // Detail message preserved separately for forensics.
    assert!(
        payload["dps_error_detail"]
            .as_str()
            .unwrap()
            .contains("test-transient"),
        "audit must carry Display detail for forensics; got: {}",
        payload["dps_error_detail"]
    );
    // No authorization_kind on Transport variant.
    assert!(payload.get("authorization_kind").is_none());
}

// ─── 2b. Authorization variant — kind sub-field ────────────────────

#[tokio::test]
async fn probe_failure_authorization_emits_kind_subfield() {
    // Covers the W8a review MED #1 ask: stable taxonomy + the
    // Authorization-only `authorization_kind` discriminator so
    // audit consumers can split DocumentReject from
    // FiscalNumberNotRegistered without re-parsing.
    use prro::transports::dps::error::AuthorizationKind;
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Closed).await;
    let stub = StubDpsChannel::with_status_result(Err(DpsError::Authorization {
        code: -13,
        kind: AuthorizationKind::FiscalNumberNotRegistered,
        message: "RRO not registered (test)".into(),
    }));
    let signer = fn_sign();
    let outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        TickOutcome::Failed {
            reason: FailureReason::DpsError,
            ..
        }
    ));
    assert_eq!(read_node_mode(&pool, FN).await, "OFFLINE");
    let payload = audit_payload(&pool, "RETURN_ONLINE_PROBE_FAILED")
        .await
        .unwrap();
    assert_eq!(payload["reason"], "dps_error");
    assert_eq!(payload["dps_error_class"], "Authorization");
    assert_eq!(payload["authorization_kind"], "FiscalNumberNotRegistered");
}

// ─── 3. DPS reports online=false ────────────────────────────────────

#[tokio::test]
async fn probe_failure_dps_reports_offline_keeps_mode_and_audits_snapshot() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Closed).await;
    let stub = StubDpsChannel::with_status(StatusSnapshot {
        open_shift: true,
        online: false,
        last_signer: "op-2".into(),
    });
    let signer = fn_sign();
    let outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        TickOutcome::Failed {
            reason: FailureReason::DpsReportsOffline,
            ..
        }
    ));
    assert_eq!(read_node_mode(&pool, FN).await, "OFFLINE");
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_FAILED").await, 1);
    let payload = audit_payload(&pool, "RETURN_ONLINE_PROBE_FAILED")
        .await
        .unwrap();
    assert_eq!(payload["reason"], "dps_reports_offline");
    // Snapshot fields recorded for forensics (hard line 2).
    assert_eq!(payload["dps_snapshot"]["online"], false);
    assert_eq!(payload["dps_snapshot"]["open_shift"], true);
    assert_eq!(payload["dps_snapshot"]["last_signer"], "op-2");
}

// ─── 4. Idempotent on GoingOnline ───────────────────────────────────

#[tokio::test]
async fn probe_idempotent_on_going_online_no_write_no_audit_no_wire() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::GoingOnline, ShiftState::Opened).await;
    // Stub panics on any call — proves wire was NOT invoked.
    let stub = StubDpsChannel::with_status_spy(
        Ok(ack_status()),
        Box::new(|| panic!("operator hard line 4 violation: status_rro called on GoingOnline")),
    );
    let signer = fn_sign();
    let outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
        .await
        .unwrap();
    match outcome {
        TickOutcome::Skipped {
            reason: SkipReason::NodeAlreadyGoingOnline,
            ..
        } => {}
        other => panic!("expected Skipped(NodeAlreadyGoingOnline), got: {other:?}"),
    }
    // Mode UNCHANGED.
    assert_eq!(read_node_mode(&pool, FN).await, "GOING_ONLINE");
    // No audit rows whatsoever (hard line 4: no spam).
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_ATTEMPT").await, 0);
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_SUCCESS").await, 0);
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_FAILED").await, 0);
}

// ─── 5. Online mode → skip BEFORE wire ──────────────────────────────

#[tokio::test]
async fn probe_skips_online_mode_before_wire_call() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::Online, ShiftState::Opened).await;
    // Stub panics on any call — proves wire was NOT invoked (hard line 5).
    let stub = StubDpsChannel::with_status_spy(
        Ok(ack_status()),
        Box::new(|| panic!("operator hard line 5 violation: status_rro called on Online")),
    );
    let signer = fn_sign();
    let outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        TickOutcome::Skipped {
            reason: SkipReason::NodeOnline,
            ..
        }
    ));
    assert_eq!(read_node_mode(&pool, FN).await, "ONLINE");
    // No audit rows (skip-Online doesn't audit to avoid spam).
    assert_eq!(audit_count(&pool, "RETURN_ONLINE_PROBE_ATTEMPT").await, 0);
}

// ─── 6. Negative scanner: zero fiscal side effects ─────────────────

#[tokio::test]
async fn probe_no_fiscal_side_effects_on_success_failure_or_skip() {
    // Seed all 4 fiscal-data tables with at least one row each;
    // run 3 probe paths (success, DPS error, DPS-offline); assert
    // row counts UNCHANGED.  Operator-pinned negative scanner
    // (design freeze §10).
    use uuid::Uuid;
    for (case_name, stub) in [
        (
            "success",
            StubDpsChannel::with_status(StatusSnapshot {
                open_shift: false,
                online: true,
                last_signer: "op-1".into(),
            }),
        ),
        (
            "dps_error",
            StubDpsChannel::with_status_result(Err(DpsError::Transport("negative-scanner".into()))),
        ),
        (
            "dps_offline",
            StubDpsChannel::with_status(StatusSnapshot {
                open_shift: false,
                online: false,
                last_signer: "op-2".into(),
            }),
        ),
    ] {
        let (_d, pool) = fresh_pool().await;
        seed_fn_config(&pool, FN).await;
        seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Opened).await;

        // Seed fiscal_documents row.
        let doc_id = vec![0xAAu8; 16];
        let req_id = Uuid::now_v7();
        let sha = vec![0u8; 32];
        sqlx::query(
            "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
                state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
                payload_sha256_canonical) \
             VALUES (?, ?, ?, 1, 'SELL', 'PREPARED', 'b', 't', 'OFFLINE', \
                '2026-05-16T00:00:00Z', '{}', ?)",
        )
        .bind(&doc_id)
        .bind(req_id.as_bytes().to_vec())
        .bind(FN)
        .bind(&sha)
        .execute(&pool)
        .await
        .unwrap();

        // Seed offline_session.
        let sess_id = vec![0xBBu8; 16];
        sqlx::query(
            "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
             VALUES (?, ?, 'OPEN', '2026-05-16T00:00:00Z')",
        )
        .bind(&sess_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();

        // Seed offline_code.
        sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES (?, ?)")
            .bind(FN)
            .bind(1_i64)
            .execute(&pool)
            .await
            .unwrap();

        // Seed transport_trace row directly using the real W7
        // schema (migration 010 + 012).  An incomplete-attempt row
        // — completion columns NULL — is the cheapest shape that
        // also exercises retry_class (migration 012 nullable column).
        // Unwrap deliberately: if the seed shape ever drifts from
        // schema, this test must FAIL, not silently skip the
        // transport_trace branch of the negative scanner.
        let trace_doc_id = doc_id.clone();
        let envelope_sha = vec![0xCCu8; 32];
        sqlx::query(
            "INSERT INTO transport_trace(document_id, attempt_no, started_at, \
                backend_profile_id, transport_profile_id, request_envelope_sha256) \
             VALUES (?, 1, '2026-05-16T00:00:00Z', 'b', 't', ?)",
        )
        .bind(&trace_doc_id)
        .bind(&envelope_sha)
        .execute(&pool)
        .await
        .unwrap();

        let pre = fiscal_table_snapshot(&pool).await;

        let signer = fn_sign();
        let _outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
            .await
            .unwrap();

        let post = fiscal_table_snapshot(&pool).await;
        // Content-level equality catches UPDATEs that preserve row
        // count as well as INSERT/DELETE — stronger than the
        // count-only baseline that the W8a review flagged.
        assert_eq!(
            pre, post,
            "probe path '{case_name}' must NOT touch fiscal-data table CONTENTS (operator pin)"
        );
    }
}

// ─── 7. Shutdown signal — task exits cleanly ───────────────────────

#[tokio::test]
async fn spawn_probe_loop_respects_shutdown_signal() {
    let (_d, pool) = fresh_pool().await;
    let pool = Arc::new(pool);
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Closed).await;
    // Stub returns success on every tick — should fire at least
    // once before shutdown signal arrives.
    let stub: Arc<dyn prro::transports::dps::channel::DpsChannel> =
        Arc::new(StubDpsChannel::with_status(StatusSnapshot {
            open_shift: false,
            online: true,
            last_signer: "shutdown-test".into(),
        }));
    let specs = vec![ProbeSpec {
        fiscal_number: FN.to_string(),
        fn_sign: fn_sign(),
    }];
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = return_online_probe::spawn_probe_loop(
        Arc::clone(&pool),
        stub,
        specs,
        Duration::from_millis(50),
        shutdown_rx,
    );
    // Let one tick fire.
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Signal shutdown.
    shutdown_tx.send(true).expect("shutdown_tx send");
    // Task must exit within a reasonable bound.
    let res = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(
        res.is_ok(),
        "probe task must exit cleanly after shutdown signal (I9)"
    );
    res.unwrap()
        .expect("task panicked instead of exiting cleanly");
    // Mode should have flipped at least once.
    assert_eq!(read_node_mode(&pool, FN).await, "GOING_ONLINE");
}

// ─── 8. CAS miss on concurrent mode change ──────────────────────────

#[tokio::test]
async fn probe_cas_miss_on_concurrent_mode_change_audits_failure() {
    let (_d, pool) = fresh_pool().await;
    seed_fn_config(&pool, FN).await;
    seed_node_state(&pool, FN, NodeMode::Offline, ShiftState::Closed).await;
    // Stub fires DPS success — but between step 1 (mode read) and
    // step 5 (CAS UPDATE) we simulate a concurrent mode change by
    // flipping the row before the probe gets to step 5.  We do
    // this via a custom spy that mutates node_state before
    // returning the status_rro response.
    let pool_for_spy = pool.clone();
    let stub = StubDpsChannel::with_status_spy(
        Ok(StatusSnapshot {
            open_shift: false,
            online: true,
            last_signer: "concurrent-flip".into(),
        }),
        Box::new(move || {
            // Use blocking SQLite via a separate runtime context.
            // For simplicity, we trigger the flip via a tokio
            // current-thread runtime block_on — the test is
            // single-threaded so this is safe.
            let pool = pool_for_spy.clone();
            futures::executor::block_on(async move {
                sqlx::query("UPDATE node_state SET mode = 'GOING_OFFLINE' WHERE fiscal_number = ?")
                    .bind(FN)
                    .execute(&pool)
                    .await
                    .unwrap();
            });
        }),
    );
    let signer = fn_sign();
    let outcome = return_online_probe::run_tick_for_fn(&pool, &stub, FN, &signer)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        TickOutcome::Failed {
            reason: FailureReason::CasMiss,
            ..
        }
    ));
    // Mode is whatever the concurrent writer left it (GOING_OFFLINE).
    assert_eq!(read_node_mode(&pool, FN).await, "GOING_OFFLINE");
    let payload = audit_payload(&pool, "RETURN_ONLINE_PROBE_FAILED")
        .await
        .unwrap();
    assert_eq!(payload["reason"], "cas_miss_concurrent_mode_change");
}

// ─── Test helpers (StubDpsChannel extension for status_rro) ────────

fn ack_status() -> StatusSnapshot {
    StatusSnapshot {
        open_shift: false,
        online: true,
        last_signer: "stub".into(),
    }
}

// Local stub that drives status_rro directly.  The common test
// helper's StubDpsChannel covers send_chk and answers status_rro
// with `unreachable!`, so W8 needs a parallel surface.  Kept inline
// to avoid touching tests/common/mod.rs.
struct LocalStatusStub {
    response: std::sync::Mutex<Option<Result<StatusSnapshot, DpsError>>>,
    on_call: Option<Box<dyn Fn() + Send + Sync>>,
}

impl LocalStatusStub {
    fn with_status(snapshot: StatusSnapshot) -> Self {
        Self {
            response: std::sync::Mutex::new(Some(Ok(snapshot))),
            on_call: None,
        }
    }
    fn with_status_result(result: Result<StatusSnapshot, DpsError>) -> Self {
        Self {
            response: std::sync::Mutex::new(Some(result)),
            on_call: None,
        }
    }
    fn with_status_spy(
        result: Result<StatusSnapshot, DpsError>,
        spy: Box<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            response: std::sync::Mutex::new(Some(result)),
            on_call: Some(spy),
        }
    }
}

#[async_trait::async_trait]
impl prro::transports::dps::channel::DpsChannel for LocalStatusStub {
    async fn send_chk(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<prro::transports::dps::dto::CheckAck, DpsError> {
        unreachable!("LocalStatusStub: send_chk not exercised");
    }
    async fn last_chk(
        &self,
        _: &CheckSignBlob,
    ) -> Result<prro::transports::dps::dto::CheckAck, DpsError> {
        unreachable!("LocalStatusStub: last_chk not exercised");
    }
    async fn ping(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<prro::transports::dps::dto::CheckAck, DpsError> {
        unreachable!("LocalStatusStub: ping not exercised");
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        if let Some(spy) = &self.on_call {
            spy();
        }
        self.response
            .lock()
            .unwrap()
            .take()
            .expect("LocalStatusStub: status_rro called more than once without re-priming")
    }
    async fn info_rro(
        &self,
        _: &CheckSignBlob,
    ) -> Result<prro::transports::dps::dto::RroInfo, DpsError> {
        unreachable!("LocalStatusStub: info_rro not exercised");
    }
    async fn ask_offline_codes(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<prro::transports::dps::dto::OfflineCodesResponse, DpsError> {
        unreachable!("LocalStatusStub: ask_offline_codes not exercised");
    }
}

// Re-bind `StubDpsChannel` in this test file to the local stub —
// the import at the top of the file uses common::StubDpsChannel but
// we replace its surface with LocalStatusStub for W8 tests.  Pure
// test-file convention; production code unaffected.
#[allow(non_camel_case_types)]
type StubDpsChannel = LocalStatusStub;
