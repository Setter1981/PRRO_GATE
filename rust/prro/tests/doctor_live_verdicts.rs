//! TDD RED-first pins for `doctor --live` verdict logic.
//!
//! Four pins (written BEFORE the impl exists):
//!   (a) ledger-sync verdict: IN_SYNC / DIVERGED / NO_LOCAL_STATE
//!   (b) offline-pool warn threshold
//!   (c) `--live` flag gating — without flag, `run` never attempts channel construction
//!   (d) cert-window classification: Ok / Warn (<30 days) / Expired
//!
//! Network-touching live section is NOT tested here — it is verified by
//! a real operator run with key + FN.  Only pure verdict functions and
//! the flag-gate are unit-testable without network.

use prro::doctor::live::{
    classify_cert_window, classify_ledger_sync, classify_offline_pool, CertWindowVerdict,
    LedgerSyncVerdict, OfflinePoolVerdict,
};

// ── (d) cert-window classification ────────────────────────────────────────

#[test]
fn cert_window_ok_when_more_than_30_days_remain() {
    // 100 days from a known anchor
    let not_after = "2026-10-15T00:00:00Z";
    let now = chrono::DateTime::parse_from_rfc3339("2026-07-07T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let v = classify_cert_window(not_after, now);
    assert!(
        matches!(v, CertWindowVerdict::Ok { days_remaining, .. } if days_remaining >= 30),
        "expected Ok with >=30 days, got {v:?}"
    );
}

#[test]
fn cert_window_warn_when_less_than_30_days_remain() {
    let not_after = "2026-07-27T00:00:00Z"; // 20 days from anchor
    let now = chrono::DateTime::parse_from_rfc3339("2026-07-07T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let v = classify_cert_window(not_after, now);
    assert!(
        matches!(v, CertWindowVerdict::Warn { days_remaining, .. } if days_remaining < 30),
        "expected Warn with <30 days, got {v:?}"
    );
}

#[test]
fn cert_window_expired_when_past_not_after() {
    let not_after = "2026-01-01T00:00:00Z"; // past
    let now = chrono::DateTime::parse_from_rfc3339("2026-07-07T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let v = classify_cert_window(not_after, now);
    assert!(
        matches!(v, CertWindowVerdict::Expired { .. }),
        "expected Expired for past date, got {v:?}"
    );
}

// ── (a) ledger-sync verdict ────────────────────────────────────────────────

#[test]
fn ledger_sync_in_sync_when_local_matches_server() {
    let v = classify_ledger_sync(Some("SFN-42".to_string()), "SFN-42");
    assert!(
        matches!(v, LedgerSyncVerdict::InSync { ref sfn } if sfn == "SFN-42"),
        "expected InSync, got {v:?}"
    );
}

#[test]
fn ledger_sync_diverged_when_local_differs_from_server() {
    let v = classify_ledger_sync(Some("SFN-41".to_string()), "SFN-42");
    assert!(
        matches!(
            v,
            LedgerSyncVerdict::Diverged { ref local, ref server }
            if local == "SFN-41" && server == "SFN-42"
        ),
        "expected Diverged, got {v:?}"
    );
}

#[test]
fn ledger_sync_no_local_state_when_local_tip_absent() {
    let v = classify_ledger_sync(None, "SFN-42");
    assert!(
        matches!(v, LedgerSyncVerdict::NoLocalState),
        "expected NoLocalState, got {v:?}"
    );
}

// ── (b) offline-pool warn threshold ───────────────────────────────────────

#[test]
fn offline_pool_ok_when_available_meets_min() {
    let v = classify_offline_pool(100, 100);
    assert!(
        matches!(
            v,
            OfflinePoolVerdict::Ok {
                available: 100,
                min: 100
            }
        ),
        "expected Ok, got {v:?}"
    );
}

#[test]
fn offline_pool_ok_when_available_exceeds_min() {
    let v = classify_offline_pool(150, 100);
    assert!(
        matches!(
            v,
            OfflinePoolVerdict::Ok {
                available: 150,
                min: 100
            }
        ),
        "expected Ok, got {v:?}"
    );
}

#[test]
fn offline_pool_warn_when_available_below_min() {
    let v = classify_offline_pool(5, 100);
    assert!(
        matches!(
            v,
            OfflinePoolVerdict::Warn {
                available: 5,
                min: 100
            }
        ),
        "expected Warn, got {v:?}"
    );
}

#[test]
fn offline_pool_ok_at_zero_when_min_is_zero() {
    let v = classify_offline_pool(0, 0);
    assert!(
        matches!(
            v,
            OfflinePoolVerdict::Ok {
                available: 0,
                min: 0
            }
        ),
        "expected Ok when min=0, got {v:?}"
    );
}

// ── (c) --live flag gating ─────────────────────────────────────────────────
//
// Without the --live flag, `doctor::run` must complete successfully even when
// PRRO_LIVE_DPS_JKS_PATH / PRRO_LIVE_DPS_JKS_PASS are not set.  Proves the
// live section is behind an explicit gate, not attempted unconditionally.

use std::fs;
use std::path::{Path, PathBuf};

fn cfg_toml(db_path: &str) -> String {
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
"#
    )
}

fn write_cfg(dir: &Path, db_path: &Path) -> PathBuf {
    let cfg_path = dir.join("prro.toml");
    let text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    fs::write(&cfg_path, text).unwrap();
    cfg_path
}

#[tokio::test]
async fn live_flag_gate_no_live_args_succeeds_without_key_env() {
    // Env vars deliberately absent (or whatever the CI env has is irrelevant
    // since we pass None → the live section is never entered).
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("gate.sqlite3");
    let cfg_path = write_cfg(dir.path(), &db_path);

    // Pass None → no --live → no channel construction.
    prro::doctor::run(&cfg_path, None)
        .await
        .expect("doctor without --live must succeed even when JKS env vars are absent");
}
