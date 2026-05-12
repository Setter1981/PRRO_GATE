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
//! - PR-1a (initial commit): SENDING fixture #3 — the load-bearing
//!   Pattern B no-resend safety contract.
//! - **PR-1b (this PR):** KVT2 fixture #8 + KVT1 corrected fixture #7.
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

async fn read_node_seed(pool: &SqlitePool, fn_id: &str) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(fn_id)
    .fetch_one(pool)
    .await
    .unwrap_or(None)
}

async fn read_inbox_status(pool: &SqlitePool, req_id: &[u8]) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(req_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Seed a `fiscal_documents` row in KVT2 state with the chain context
/// `stage_finalize::run` requires: `unsigned_xml_sha256` set,
/// `previous_hash` NULL (genesis case — `node_state.last_known_unsigned_xml_sha256`
/// also NULL is the matching chain-continuity seed).  Returns the
/// request_id (for inbox seeding) and document_id.
async fn seed_doc_kvt2_for_finalize(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
    unsigned_xml_sha: [u8; 32],
) -> ([u8; 16], DocumentId) {
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let canonical_sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 'SELL', 'KVT2', 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(lnd)
    .bind(&canonical_sha)
    .bind(unsigned_xml_sha.as_slice())
    .execute(pool)
    .await
    .unwrap();
    let doc_id = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let req_id_arr: [u8; 16] = <[u8; 16]>::try_from(req_bytes.as_slice()).unwrap();
    (req_id_arr, doc_id)
}

/// Seed an `ingress_inbox` row in PROCESSING state — required by
/// `stage_finalize::run` step 5 (`ingress_inbox::mark_done_tx`).
async fn seed_inbox_processing(pool: &SqlitePool, fn_id: &str, req_id: &[u8; 16]) {
    let sha = vec![0u8; 32];
    let req_slice: &[u8] = req_id;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'sell', ?, '{}', ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(fn_id)
    .bind(format!("idem-{:02x}", req_id[0]))
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
}

/// Build a `StubDpsChannel` whose every method invocation panics —
/// `send_chk` via the spy callback, the rest via their `unreachable!()`
/// defaults at `tests/common/mod.rs:136-150`.  Used by recovery
/// fixtures that prove "DPS not consulted during recovery".
fn dps_panic_on_any_method(rationale: &'static str) -> StubDpsChannel {
    StubDpsChannel::with_spy(
        Ok(ack("unused")),
        Box::new(move || panic!("DPS violation: send_chk invoked — {rationale}")),
    )
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

// ─── Fixture #7 — §6.5 KVT1 crash-resume (passive hold, no DPS) ────
//
// W0-3 §6.5:794-798 spec text demands eventual drive-forward to KVT2
// via re-query; however the operator-decided W11 scope (2026-05-12
// gate-pass record) preserves the W9 passive-hold contract:
//
//   "KVT1 stays passive hold under W11; W11 does NOT supersede W9.
//   Reason: `DpsChannel` has no per-doc KVT2-receipt API; KVT1→ACK
//   via active polling is a separate slice (transport API extension),
//   not W11."
//
// This fixture pins the passive-hold contract under the new
// `reconcile_pending_with` entry: even with `ReconciliationRuntime`
// in place, KVT1 docs stay in KVT1 and `BOOT_KVT1_HOLD_DEFERRED`
// audit fires.  Zero DPS invocations.
//
// Co-exists with the W9 fixture
// `branch_c_dispatches_kvt1_to_passive_hold`
// (`tests/app_boot_reconciliation.rs:192-207`) which asserts the same
// behaviour under the legacy `reconcile_pending` (no deps) entry.

#[tokio::test]
async fn fixture_7_kvt1_crash_passive_hold_no_dps() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_in_state(app.db(), fn_id, 0x77, "KVT1").await;

    let stub = dps_panic_on_any_method(
        "§6.5 KVT1 recovery must not query DPS (no per-doc KVT2 receipt API in M3a)",
    );
    let signing_ctx = det_signing_ctx();
    let deps = ReconciliationRuntime {
        dps: &stub,
        signing_ctx: &signing_ctx,
    };

    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) Passive hold — state preserved at KVT1.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "KVT1",
        "operator decision (2026-05-12): KVT1 stays KVT1 under W11 — active poll deferred"
    );

    // (2) Forensic audit emitted.
    assert_eq!(
        audit_count(app.db(), "BOOT_KVT1_HOLD_DEFERRED").await,
        1,
        "§6.5 + W9 contract — passive hold trace mandatory"
    );

    // (3) Zero DPS invocations across the recovery — under the new
    // entry, even with deps available, KVT1 stays passive.
    assert_eq!(
        stub.call_count(),
        0,
        "KVT1 passive hold: no send_chk; combined with unreachable!() defaults on \
         last_chk/ping/status_rro/info_rro, ANY DPS invocation would have panicked"
    );
}

// ─── Fixture #8 — §6.6 KVT2 crash-resume (ACK via stage_finalize, no DPS) ──
//
// W0-3 §6.6:808-822 mandates:
//
//   "App::boot recovery invokes §3 KVT2 rule 're-drive forward to ACK
//   only'.  Note: there is no DPS query in this branch, because KVT2
//   is the protocol-level commit point... Recovery executes the
//   stage-5 finalize logic — transition_state(doc_id, Kvt2, Ack) CAS
//   UPDATE + node_state.last_known_unsigned_xml_sha256 update +
//   audit_log append + inbox.status=DONE."
//
// **Critical assertion:** `StubDpsChannel::call_count() == 0` — KVT2
// is protocol-final.  All DPS methods are panic-armed
// (`with_spy(_, panic!)` on send_chk + `unreachable!()` defaults on
// last_chk / ping / status_rro / info_rro).  Any DPS invocation
// panics the test.
//
// Seed shape (per `stage_finalize::run` preconditions):
//   - fiscal_documents: state=KVT2, unsigned_xml_sha256=X,
//     previous_hash=NULL (genesis case).
//   - node_state: last_known_unsigned_xml_sha256=NULL (matches
//     previous_hash for chain-continuity guard).
//   - ingress_inbox: status=PROCESSING (so mark_done_tx can flip it).

#[tokio::test]
async fn fixture_8_kvt2_crash_no_dps_query() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let unsigned_xml_sha = [0xCCu8; 32];
    let (req_id, doc) = seed_doc_kvt2_for_finalize(app.db(), fn_id, 0x88, unsigned_xml_sha).await;
    seed_inbox_processing(app.db(), fn_id, &req_id).await;

    // Pre-state sanity — `node_state.last_known_unsigned_xml_sha256`
    // starts NULL (matches doc's NULL `previous_hash` — genesis).
    assert!(
        read_node_seed(app.db(), fn_id).await.is_none(),
        "genesis case: seed must start NULL"
    );

    let stub = dps_panic_on_any_method(
        "§6.6 KVT2 is protocol-final — recovery executes stage_finalize::run only, NO DPS",
    );
    let signing_ctx = det_signing_ctx();
    let deps = ReconciliationRuntime {
        dps: &stub,
        signing_ctx: &signing_ctx,
    };

    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) State transitions KVT2 → ACK (§6.6:822 final state).
    assert_eq!(
        doc_state(app.db(), doc).await,
        "ACK",
        "§6.6 demands KVT2 → ACK on crash-resume"
    );

    // (2) Cross-doc MAC chain seed advanced (§6.6:814).
    assert_eq!(
        read_node_seed(app.db(), fn_id).await.as_deref(),
        Some(unsigned_xml_sha.as_slice()),
        "§6.6:814 — node_state.last_known_unsigned_xml_sha256 must advance to the finalised doc's seed"
    );

    // (3) Inbox flipped PROCESSING → DONE (stage_finalize step 5).
    assert_eq!(
        read_inbox_status(app.db(), &req_id).await.as_deref(),
        Some("DONE"),
        "stage_finalize step 5 — inbox row must be marked DONE"
    );

    // (4) CRITICAL — zero DPS invocations across the recovery.
    // §6.6:810-811 "there is no DPS query in this branch, because
    // KVT2 is the protocol-level commit point".  Combined with
    // `unreachable!()` defaults on last_chk/ping/status_rro/info_rro,
    // any DPS method invocation would have panicked the test.
    assert_eq!(
        stub.call_count(),
        0,
        "§6.6 KVT2 protocol-final: send_chk must NOT be invoked during KVT2 recovery"
    );
}
