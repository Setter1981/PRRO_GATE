//! W8b acceptance — App-owned return-online probe wiring seam.
//!
//! Covers the 5 W8b merge-criterion tests pinned by the operator:
//!
//!   1. `boot_probe_no_attempt_audit_while_fn_is_online` — FN that
//!      starts `Online` produces zero `_ATTEMPT` audit rows across
//!      the first tick (operator hard line 5; defence-in-depth at
//!      tick-level).
//!   2. `boot_probe_attempts_after_online_to_offline_flip` — FN
//!      that starts `Online` and is flipped to `Offline` between
//!      ticks gets its first `_ATTEMPT` on the next tick (validates
//!      that the boot rule enumerates ALL configured FNs — not just
//!      `(Offline | GoingOffline)` at boot time).
//!   3. `boot_probe_respects_shutdown_signal_at_app_level` — boot
//!      → `watch::Sender::send(true)` → task exits cleanly within a
//!      bounded timeout (I9 end-to-end).
//!   4. `boot_probe_emits_warn_audit_on_clamp` — operator-supplied
//!      out-of-bounds interval → `RETURN_ONLINE_PROBE_INTERVAL_CLAMPED`
//!      WARN audit emitted from the App method, with raw + clamped
//!      + bounds in payload.
//!   5. `boot_probe_emits_critical_audit_on_tick_infra_error` —
//!      drop the `node_state` table mid-loop → next tick's
//!      `node_state::get` returns sqlx Err → durable
//!      `RETURN_ONLINE_PROBE_LOOP_ERROR` CRITICAL audit row.
//!
//! Plus a bonus check exercising the missing-signer skip path —
//! folded into test #1 (config has 2 FNs; signer map has 1; the
//! unsigned FN is skipped with WARN audit, no _ATTEMPT for it ever).

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use prro::services::offline_sync::return_online_probe::ReturnOnlineProbeDeps;
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use prro::{config::AppConfig, App};
use sqlx::SqlitePool;
use tokio::sync::watch;
use tokio::time::{sleep, timeout};

const FN_A: &str = "1000000001";
const FN_B: &str = "1000000002";

// Clamp min is 5s per `config::PROBE_INTERVAL_MIN_SECONDS`; tests
// accept that real-time cost.  Per-test budget is bounded by the
// test assertions; the heaviest tests wait one extra tick (~5s).
const PROBE_TICK_SECONDS: u64 = 5;

// ─── Stub `DpsChannel` for `status_rro` ────────────────────────────

struct ProbeStubChannel {
    response: std::sync::Mutex<Option<Result<StatusSnapshot, DpsError>>>,
}

impl ProbeStubChannel {
    fn ok_online() -> Arc<dyn DpsChannel> {
        Arc::new(Self {
            response: std::sync::Mutex::new(Some(Ok(StatusSnapshot {
                open_shift: false,
                online: true,
                last_signer: "boot-test".into(),
            }))),
        })
    }
}

#[async_trait]
impl DpsChannel for ProbeStubChannel {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("ProbeStubChannel: send_chk not exercised by W8b boot tests")
    }
    async fn send_chk_observed(
        &self,
        envelope: CheckEnvelope,
    ) -> (
        Result<CheckAck, DpsError>,
        prro::transports::dps::raw_reply::RawSendObservation,
    ) {
        prro::transports::dps::dto::scripted_observation(self.send_chk(envelope).await)
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        unreachable!("ProbeStubChannel: last_chk not exercised")
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("ProbeStubChannel: ping not exercised")
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        // Re-prime each call with the same value: the loop may tick
        // multiple times in the same test, and `take()` would empty
        // the Mutex after the first call.  Clone the stored shape.
        let guard = self.response.lock().unwrap();
        match guard.as_ref().expect("response not primed") {
            Ok(snap) => Ok(snap.clone()),
            Err(_) => Err(DpsError::Transport("ProbeStubChannel: primed error".into())),
        }
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("ProbeStubChannel: info_rro not exercised")
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

// ─── Boot fixture ──────────────────────────────────────────────────

fn cfg_toml(db_path: &str, probe_interval_seconds: u64) -> String {
    format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"

[offline]
return_online_probe_interval_seconds = {probe_interval_seconds}
"#
    )
}

async fn fresh_app(db_path: &std::path::Path, probe_interval_seconds: u64) -> App {
    let toml = cfg_toml(
        &db_path.display().to_string().replace('\\', "/"),
        probe_interval_seconds,
    );
    let cfg = AppConfig::from_toml(&toml).unwrap();
    App::boot(cfg).await.expect("fresh boot must succeed")
}

async fn seed_fn(pool: &SqlitePool, fn_id: &str, mode: &str) {
    sqlx::query(
        "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '1234567890', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, ?, 'CLOSED', 1)",
    )
    .bind(fn_id)
    .bind(mode)
    .execute(pool)
    .await
    .unwrap();
}

fn sign_blob() -> CheckSignBlob {
    CheckSignBlob(vec![0xAB, 0xCD, 0xEF, 0x12])
}

fn deps_with(fn_signs: HashMap<String, CheckSignBlob>) -> ReturnOnlineProbeDeps {
    ReturnOnlineProbeDeps {
        dps: ProbeStubChannel::ok_online(),
        fn_signs,
    }
}

async fn count_audit(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_audit_for_entity(
    pool: &SqlitePool,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = ? AND entity_type = ? AND entity_id = ?",
    )
    .bind(event_type)
    .bind(entity_type)
    .bind(entity_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn read_node_mode(pool: &SqlitePool, fn_id: &str) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(fn_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// Stop the loop and await its termination.  Caller responsibility
// per the App method contract (App does not track the handle).
async fn stop_loop(shutdown_tx: watch::Sender<bool>, handle: tokio::task::JoinHandle<()>) {
    shutdown_tx.send(true).expect("shutdown send");
    timeout(Duration::from_secs(2), handle)
        .await
        .expect("loop must exit within 2s")
        .expect("loop task panicked");
}

// ─── 1. Online FN → no _ATTEMPT audit (covers missing-signer skip too) ─

#[tokio::test]
async fn boot_probe_no_attempt_audit_while_fn_is_online() {
    let dir = tempfile::tempdir().unwrap();
    let app = fresh_app(&dir.path().join("w8b.db"), PROBE_TICK_SECONDS).await;

    // Seed two FNs: FN_A online + signer present; FN_B online +
    // signer ABSENT (exercises the missing-signer WARN+skip path).
    seed_fn(app.db(), FN_A, "ONLINE").await;
    seed_fn(app.db(), FN_B, "ONLINE").await;

    let mut fn_signs = HashMap::new();
    fn_signs.insert(FN_A.to_string(), sign_blob());
    // FN_B intentionally omitted.

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = app
        .spawn_return_online_probe(deps_with(fn_signs), shutdown_rx)
        .await
        .unwrap();

    // Let the first tick fire (interval fires immediately).
    sleep(Duration::from_millis(500)).await;

    // FN_A: present + Online → tick skip-Online (no audit).
    // FN_B: missing signer → never in ProbeSpec list (no audit
    // either; WARN audit was emitted at spawn time, not at tick).
    assert_eq!(
        count_audit(app.db(), "RETURN_ONLINE_PROBE_ATTEMPT").await,
        0
    );
    assert_eq!(
        count_audit(app.db(), "RETURN_ONLINE_PROBE_SUCCESS").await,
        0
    );
    assert_eq!(count_audit(app.db(), "RETURN_ONLINE_PROBE_FAILED").await, 0);

    // FN_B missing-signer WARN audit was emitted exactly once during
    // App method composition (not per-tick).
    assert_eq!(
        count_audit_for_entity(
            app.db(),
            "RETURN_ONLINE_PROBE_FN_SKIPPED_NO_SIGNER",
            "fiscal_number_config",
            FN_B,
        )
        .await,
        1
    );
    // FN_A is NOT skipped — no skip audit for it.
    assert_eq!(
        count_audit_for_entity(
            app.db(),
            "RETURN_ONLINE_PROBE_FN_SKIPPED_NO_SIGNER",
            "fiscal_number_config",
            FN_A,
        )
        .await,
        0
    );

    // Mode unchanged for both.
    assert_eq!(read_node_mode(app.db(), FN_A).await, "ONLINE");
    assert_eq!(read_node_mode(app.db(), FN_B).await, "ONLINE");

    stop_loop(shutdown_tx, handle).await;
}

// ─── 2. Online → Offline flip → next tick emits _ATTEMPT ───────────

#[tokio::test]
async fn boot_probe_attempts_after_online_to_offline_flip() {
    let dir = tempfile::tempdir().unwrap();
    let app = fresh_app(&dir.path().join("w8b.db"), PROBE_TICK_SECONDS).await;
    seed_fn(app.db(), FN_A, "ONLINE").await;

    let mut fn_signs = HashMap::new();
    fn_signs.insert(FN_A.to_string(), sign_blob());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = app
        .spawn_return_online_probe(deps_with(fn_signs), shutdown_rx)
        .await
        .unwrap();

    // First tick (Online → skip).
    sleep(Duration::from_millis(500)).await;
    assert_eq!(
        count_audit(app.db(), "RETURN_ONLINE_PROBE_ATTEMPT").await,
        0
    );

    // Flip mode → Offline.  Probe was enumerated at boot regardless
    // of mode; this is the proof that "enumerate ALL FNs" rule
    // catches late transitions.
    sqlx::query("UPDATE node_state SET mode = 'OFFLINE' WHERE fiscal_number = ?")
        .bind(FN_A)
        .execute(app.db())
        .await
        .unwrap();

    // Wait for the next tick.  Interval fires every
    // `PROBE_TICK_SECONDS`s after the immediate first tick; one
    // tick plus a small grace window.
    sleep(Duration::from_secs(PROBE_TICK_SECONDS + 1)).await;

    // Probe should have fired ATTEMPT + SUCCESS (mode flipped to
    // GOING_ONLINE) since the stub returns online=true.
    assert!(
        count_audit(app.db(), "RETURN_ONLINE_PROBE_ATTEMPT").await >= 1,
        "expected at least one _ATTEMPT after Online->Offline flip"
    );
    assert_eq!(
        count_audit(app.db(), "RETURN_ONLINE_PROBE_SUCCESS").await,
        1
    );
    assert_eq!(read_node_mode(app.db(), FN_A).await, "GOING_ONLINE");

    stop_loop(shutdown_tx, handle).await;
}

// ─── 3. Shutdown signal → clean task exit ──────────────────────────

#[tokio::test]
async fn boot_probe_respects_shutdown_signal_at_app_level() {
    let dir = tempfile::tempdir().unwrap();
    let app = fresh_app(&dir.path().join("w8b.db"), PROBE_TICK_SECONDS).await;
    seed_fn(app.db(), FN_A, "OFFLINE").await;

    let mut fn_signs = HashMap::new();
    fn_signs.insert(FN_A.to_string(), sign_blob());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = app
        .spawn_return_online_probe(deps_with(fn_signs), shutdown_rx)
        .await
        .unwrap();

    // Let the first tick fire (immediate; Offline -> success path
    // exercises a full tick body before shutdown arrives).
    sleep(Duration::from_millis(500)).await;

    // Send shutdown.  Loop must exit within bound (operator hard
    // line 6: biased select on shutdown branch ALWAYS wins).
    shutdown_tx.send(true).expect("shutdown send");
    let exited = timeout(Duration::from_secs(2), handle).await;
    assert!(
        exited.is_ok(),
        "probe loop must exit within 2s of shutdown signal (I9)"
    );
    exited
        .unwrap()
        .expect("probe task panicked instead of clean exit");
}

// ─── 4. Out-of-bounds interval → WARN audit on clamp ───────────────

#[tokio::test]
async fn boot_probe_emits_warn_audit_on_clamp() {
    let dir = tempfile::tempdir().unwrap();
    // Operator-supplied value below the min (5s).  App method must
    // clamp to 5 AND emit WARN audit with both raw + clamped values
    // in the payload.
    let app = fresh_app(&dir.path().join("w8b.db"), 1).await;
    // No FN needed — clamp audit is emitted before FN enumeration.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = app
        .spawn_return_online_probe(deps_with(HashMap::new()), shutdown_rx)
        .await
        .unwrap();

    // Clamp audit is emitted synchronously during the App method;
    // no wait needed.
    assert_eq!(
        count_audit_for_entity(app.db(), "RETURN_ONLINE_PROBE_INTERVAL_CLAMPED", "app", "",).await,
        1
    );
    // Payload check: raw=1, clamped=5, min=5, max=3600.
    let payload_str: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'RETURN_ONLINE_PROBE_INTERVAL_CLAMPED' \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .fetch_one(app.db())
    .await
    .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap();
    assert_eq!(payload["raw_seconds"], 1);
    assert_eq!(payload["clamped_seconds"], 5);
    assert_eq!(payload["min_seconds"], 5);
    assert_eq!(payload["max_seconds"], 3600);

    stop_loop(shutdown_tx, handle).await;
}

// ─── 5. Tick infra error → CRITICAL audit ─────────────────────────

#[tokio::test]
async fn boot_probe_emits_critical_audit_on_tick_infra_error() {
    let dir = tempfile::tempdir().unwrap();
    let app = fresh_app(&dir.path().join("w8b.db"), PROBE_TICK_SECONDS).await;
    seed_fn(app.db(), FN_A, "OFFLINE").await;

    let mut fn_signs = HashMap::new();
    fn_signs.insert(FN_A.to_string(), sign_blob());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = app
        .spawn_return_online_probe(deps_with(fn_signs), shutdown_rx)
        .await
        .unwrap();

    // Let the first tick fire (Offline + online stub → SUCCESS).
    sleep(Duration::from_millis(500)).await;
    assert_eq!(
        count_audit(app.db(), "RETURN_ONLINE_PROBE_SUCCESS").await,
        1
    );

    // Force an infra-level failure for the NEXT tick by dropping
    // the `node_state` table.  `run_tick_for_fn` step 1 reads
    // `node_state::get` first, which will return sqlx::Err with
    // "no such table" — propagated as anyhow::Err out of the
    // primitive.  The loop's Err branch must then emit a durable
    // RETURN_ONLINE_PROBE_LOOP_ERROR CRITICAL audit row.  The
    // audit_log table itself is preserved so the audit insert
    // succeeds.
    sqlx::query("DROP TABLE node_state")
        .execute(app.db())
        .await
        .unwrap();

    sleep(Duration::from_secs(PROBE_TICK_SECONDS + 1)).await;

    let critical_count = count_audit(app.db(), "RETURN_ONLINE_PROBE_LOOP_ERROR").await;
    assert!(
        critical_count >= 1,
        "expected at least one RETURN_ONLINE_PROBE_LOOP_ERROR after dropping node_state; got {critical_count}"
    );

    // Severity must be CRITICAL (not WARNING/ERROR).
    let severity: String = sqlx::query_scalar(
        "SELECT severity FROM audit_log \
         WHERE event_type = 'RETURN_ONLINE_PROBE_LOOP_ERROR' \
         ORDER BY audit_id DESC LIMIT 1",
    )
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(severity, "CRITICAL");

    shutdown_tx.send(true).expect("shutdown send");
    timeout(Duration::from_secs(2), handle)
        .await
        .expect("loop must exit within 2s")
        .expect("loop task panicked");
}
