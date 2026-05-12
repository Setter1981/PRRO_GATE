//! W11 — Deterministic-replay invariant fixtures (W0-3 §6 / §9.3).
//!
//! The M3a frozen invariant §6 mandates: for every pending DocState,
//! `App::reconcile_pending(_with)` converges to the SAME final state
//! whether the prior process crashed mid-pipeline or completed
//! uninterrupted.  This file is the proof harness.
//!
//! Anchors:
//! - W0-3 §6 sub-cases (`docs/superpowers/specs/2026-05-06-m3-w0-3-retry-recovery.md:663-867`).
//! - W0-3 §9.3 fixture spec (`:1263-1282`).
//! - ADR-M3-A8 (pending-set whitelist gaps).
//! - ADR-M3-A9 (SENDING crash-resume + ErrorRetryable retry-path —
//!   direct `Sent → Sending` forbidden, must hop through ErrorRetryable).
//! - ADR-M3-A10 (single-writer-per-FN invariant — recovery executes
//!   under global-single-writer; see PR-1a `ReconciliationRuntime`).
//! - W11 design freeze
//!   `docs/superpowers/specs/2026-05-12-w11-deterministic-replay-design.md`.
//!
//! Slice landings:
//! - **PR-1a (this file, initial commit):** SENDING fixture #3 — the
//!   load-bearing Pattern B no-resend safety contract.
//! - PR-1b: KVT2 fixture #8 + KVT1 corrected fixture #7.
//! - PR-2: PREPARED (#1), SIGNED (#2), SENT a/b/c (#4/#5/#6 — #6 is
//!   the two-tick driver per operator decision), ERROR_RETRYABLE (#9).

mod common;

use common::{ack, det_signing_ctx, StubDpsChannel};

use prro::config::AppConfig;
use prro::db::models::ids::DocumentId;
use prro::services::reconciliation::ReconciliationRuntime;
use prro::App;
use sqlx::SqlitePool;

// ─── Harness ───────────────────────────────────────────────────────

async fn fresh_app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("w11.db");
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
    let cfg = AppConfig::from_toml(&toml_text).expect("config parse");
    let app = App::boot(cfg).await.expect("App::boot");
    (dir, app)
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

async fn doc_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
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

// ─── Fixture #3 — §6.3 SENDING crash-resume (Pattern B no-resend) ──
//
// W0-3 §6.3:724-727 mandates:
//   "Recovery action (per §3 SENDING row): CAS Sending→ErrorRetryable
//   + audit `crash_resume_sending_to_error_retryable`; do NOT
//   auto-re-send."
//
// ADR-M3-A9 step 3 anchors further: even operator-initiated re-send
// from ErrorRetryable must go through `ErrorRetryable → Sending →
// wire`, never direct to Sent.
//
// **Critical assertion:** `StubDpsChannel::call_count() == 0` —
// Pattern B no-resend.  Defence-in-depth: the stub's `on_send_chk`
// spy panics BEFORE the response queue is consulted; `last_chk` /
// `ping` / `status_rro` / `info_rro` are `unreachable!()` defaults
// (see `tests/common/mod.rs:136-150`), so ANY DPS method invocation
// panics the test.

#[tokio::test]
async fn fixture_3_sending_crash_pattern_b_no_resend() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_in_state(app.db(), fn_id, 0x33, "SENDING").await;

    let stub = StubDpsChannel::with_spy(
        Ok(ack("unused-pattern-b-violation")),
        Box::new(|| {
            panic!(
                "Pattern B violation: send_chk invoked during SENDING recovery (§6.3 forbids re-send)"
            )
        }),
    );
    let signing_ctx = det_signing_ctx();
    let deps = ReconciliationRuntime {
        dps: &stub,
        signing_ctx: &signing_ctx,
    };

    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) State transition: Sending → ErrorRetryable.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "§6.3 demands SENDING → ERROR_RETRYABLE on crash-resume"
    );

    // (2) Audit emitted (§6.3:725 — implementation name from W9
    // `branch_c_dispatches_sending_to_resume_helper`).
    assert_eq!(
        audit_count(app.db(), "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        1,
        "§6.3 mandates the crash-resume audit"
    );

    // (3) CRITICAL — zero send_chk invocations during recovery.
    // Pattern B safety contract: SENDING marker means "we have
    // already submitted to DPS"; re-sending would risk duplicate
    // fiscalisation.  ADR-M3-A9 + §6.3 forbid it.
    assert_eq!(
        stub.call_count(),
        0,
        "§6.3 Pattern B: send_chk must NOT be invoked during SENDING recovery"
    );
}
