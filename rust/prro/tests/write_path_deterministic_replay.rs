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

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use prro::config::AppConfig;
use prro::db::models::ids::DocumentId;
use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, CheckSignBlob, RroInfo, StatusSnapshot};
use prro::transports::dps::error::DpsError;
use prro::App;
use sqlx::SqlitePool;

/// Dummy DPS identity blob for fixtures.  The stub channel does not
/// verify the blob's contents — this matches the existing
/// `last_chk_probe::tests::fn_sign()` shape at
/// `rust/prro/src/services/reconciliation/last_chk_probe.rs:157-158`.
fn dummy_fn_sign() -> CheckSignBlob {
    CheckSignBlob(vec![0xDEu8, 0xAD, 0xBE, 0xEF])
}

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
    // W14a-2b Commit 5: SELL is a non-bypass doc; signer_guard at
    // stage_send 4-pre needs shift_id + signed_by_cashier_id matching
    // the shift's opening cashier.  Seed a shift row + bind both.
    let shift_byte = doc_byte ^ 0x80;
    let shift_bytes = vec![shift_byte; 16];
    sqlx::query(
        "INSERT OR IGNORE INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(&shift_bytes)
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();

    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, signed_by_cashier_id) \
         VALUES (?, ?, ?, ?, ?, 'SELL', ?, 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, 'test-cashier')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(&shift_bytes)
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
    .expect("read node_state seed")
}

async fn read_inbox_status(pool: &SqlitePool, req_id: &[u8]) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(req_id)
        .fetch_optional(pool)
        .await
        .expect("read inbox status")
}

async fn read_document_file_kind(
    pool: &SqlitePool,
    doc: DocumentId,
    kind: &str,
) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT content FROM document_files WHERE document_id = ? AND kind = ?")
        .bind(doc)
        .bind(kind)
        .fetch_optional(pool)
        .await
        .expect("read document_files row")
}

async fn count_outbox_for(pool: &SqlitePool, doc: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .expect("count outbox rows for doc")
}

async fn insert_document_file(pool: &SqlitePool, doc: DocumentId, kind: &str, content: &[u8]) {
    sqlx::query("INSERT INTO document_files (document_id, kind, content) VALUES (?, ?, ?)")
        .bind(doc)
        .bind(kind)
        .bind(content)
        .execute(pool)
        .await
        .expect("seed document_files row");
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
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

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
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

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

    // KVT2 crash-state realism: at this crash point the wire receipts
    // KVT1_RAW and KVT2_RAW are ALREADY persisted (the worker reached
    // KVT2 by having received and stored them).  Recovery must
    // finalize without re-writing or mutating them.  Fingerprintable
    // byte patterns let post-recovery readback prove unchanged-ness.
    let kvt1_raw: &[u8] = &[0xAA; 64];
    let kvt2_raw: &[u8] = &[0xBB; 64];
    insert_document_file(app.db(), doc, "KVT1_RAW", kvt1_raw).await;
    insert_document_file(app.db(), doc, "KVT2_RAW", kvt2_raw).await;

    // Pre-state sanity — `node_state.last_known_unsigned_xml_sha256`
    // starts NULL (matches doc's NULL `previous_hash` — genesis).
    assert!(
        read_node_seed(app.db(), fn_id).await.is_none(),
        "genesis case: seed must start NULL"
    );
    // Pre-state sanity — outbox empty before reconcile.
    assert_eq!(
        count_outbox_for(app.db(), doc).await,
        0,
        "pre-state: outbox row must not yet exist for this doc"
    );

    let stub = dps_panic_on_any_method(
        "§6.6 KVT2 is protocol-final — recovery executes stage_finalize::run only, NO DPS",
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

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

    // (4) Outbox INSERT executed exactly once (stage_finalize step 4 /
    // step 6 per W8 freeze).  PK on `document_id` makes any duplicate
    // a hard error inside the tx; assert COUNT=1 to prove the
    // finalize tx committed the outbox row (defence-in-depth: a
    // commit without outbox row would have rolled back upstream).
    assert_eq!(
        count_outbox_for(app.db(), doc).await,
        1,
        "stage_finalize must INSERT exactly one outbox row on Ack"
    );

    // (5) STAGE_FINALIZE_ACK audit count == 1 — proves the finalize
    // codepath actually ran end-to-end (not just CAS short-circuit on
    // already-Ack).  Anchored at `stage_finalize.rs:212` + `:331-346`.
    assert_eq!(
        audit_count(app.db(), "STAGE_FINALIZE_ACK").await,
        1,
        "stage_finalize step 7 — STAGE_FINALIZE_ACK audit row must fire on first-time Ack"
    );

    // (6) Raw KVT artifacts unchanged.  Finalize is a state-machine
    // close, not a wire-side write — pre-existing receipts MUST be
    // preserved byte-for-byte.  W11 #8 is the first end-to-end proof
    // of the read-only-on-finalize contract.
    assert_eq!(
        read_document_file_kind(app.db(), doc, "KVT1_RAW")
            .await
            .as_deref(),
        Some(kvt1_raw),
        "KVT1_RAW must remain byte-for-byte unchanged across finalize"
    );
    assert_eq!(
        read_document_file_kind(app.db(), doc, "KVT2_RAW")
            .await
            .as_deref(),
        Some(kvt2_raw),
        "KVT2_RAW must remain byte-for-byte unchanged across finalize"
    );

    // (7) CRITICAL — zero DPS invocations across the recovery.
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

// ─── Helper for PR-2 fixtures #2 / #9 (SIGNED / ERROR_RETRYABLE) ───
//
// Seeds a `fiscal_documents` row in the requested state + the
// `SIGNED_XML` artifact `stage_send::run`'s 4-pre read requires.
async fn seed_doc_with_signed_xml(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
    state: &str,
) -> DocumentId {
    let doc = seed_doc_in_state(pool, fn_id, doc_byte, state).await;
    insert_document_file(pool, doc, "SIGNED_XML", &[0xEE; 64]).await;
    doc
}

async fn read_mac_recovery_attempts(pool: &SqlitePool, doc: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT mac_recovery_attempts FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .expect("read mac_recovery_attempts")
}

// ─── Fixture #2 — §6.2 SIGNED crash-resume (fresh first send) ──────
//
// W0-3 §6.2:693-708 mandates: SIGNED crash means the worker reached
// SIGNED state but never submitted to DPS.  Recovery: drive forward
// via stage 4 (Pattern B 4-pre/4a/4b).  No duplicate-send hazard at
// DPS because no prior submission ever happened.
//
// Assertions:
// 1. State advances past SIGNED — happy `send_chk` produces SENT.
// 2. `send_chk_count == 1` (one fresh wire send during recovery).
// 3. `histogram.signed_dispatched == 1` (W11 PR-2 wiring observable).

#[tokio::test]
async fn fixture_2_signed_crash_replays_to_sent_one_send_chk() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0x22, "SIGNED").await;

    let stub = StubDpsChannel::new(Ok(ack("server-fiscal-sent-22")));
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) State advances past SIGNED via Pattern B send path.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "SENT",
        "§6.2 happy `send_chk` drives Signed → Sending → Sent"
    );

    // (2) Exactly one send_chk invocation (fresh first send).
    assert_eq!(
        stub.call_count(),
        1,
        "§6.2 — one wire send during recovery (no resend hazard since no prior submission)"
    );

    // (3) W11 PR-2a wiring observable in the dispatch histogram —
    // SIGNED arm produced exactly one dispatch (not deferred).
    assert_eq!(
        summary.docs_advanced.signed_dispatched, 1,
        "PR-2a wiring: SIGNED Some(deps) path increments signed_dispatched"
    );
    assert_eq!(
        summary.docs_advanced.signed_deferred, 0,
        "PR-2a wiring: SIGNED must NOT fall through to the DEFERRED arm when deps Some"
    );
    assert_eq!(
        summary.docs_advanced.total_visited(),
        1,
        "exactly one pending doc dispatched, all other counters zero"
    );

    // (4) No `BOOT_DISPATCH_DEFERRED` audit fired — proves the
    // legacy ctx-free path was NOT taken under `reconcile_pending_with`.
    assert_eq!(
        audit_count(app.db(), "BOOT_DISPATCH_DEFERRED").await,
        0,
        "SIGNED dispatch arm wired in PR-2a; deferred audit must not fire"
    );
}

// ─── Fixture #9 — §6.7 ERROR_RETRYABLE crash-resume (no MAC burn) ──
//
// W0-3 §6.7:831-854 mandates: ERROR_RETRYABLE retry drives through
// `stage_send::run` (ADR-M3-A9 step 5-6 allows the
// `ErrorRetryable → Sending` CAS).  Happy retry must NOT consume the
// W10 MAC-recovery single-bit budget (`mac_recovery_attempts`) —
// that budget is reserved for hash-mismatch MAC-recovery, not for
// general retry.
//
// Assertions:
// 1. State advances past ERROR_RETRYABLE (happy `send_chk` → SENT).
// 2. `send_chk_count == 1`.
// 3. `mac_recovery_attempts` unchanged at 0.
// 4. No `MAC_RECOVERY_*` audit emitted.
// 5. `histogram.error_retryable_dispatched == 1`.

#[tokio::test]
async fn fixture_9_error_retryable_retries_without_mac_counter_burn() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0x99, "ERROR_RETRYABLE").await;

    // M3a hardening pass 1 — seed durable retry_class evidence.  The
    // new `dispatch_error_retryable_by_class` dispatcher routes ER
    // docs by their last-attempt `retry_class`; without a
    // transport_trace row the dispatcher would hold the doc as
    // indeterminate.  This fixture's intent (happy retry without
    // MAC budget burn) requires `TransientRetry` to invoke
    // stage_send::run.
    //
    // **Boundary note (H2 closure boundary).**  `seed_completed_
    // transport_trace` writes exactly one row (`attempt_no=1`); the
    // H2 budget cap fires only when
    // `transport_trace::attempts_used(doc) >= MAX_BOOT_ATTEMPTS`
    // (=5).  1 < 5, so the TransientRetry arm proceeds to
    // stage_send::run as designed.  Fixture #9g covers the
    // budget-exhausted case (5 seeded attempts).
    seed_completed_transport_trace(app.db(), doc, "RETRYABLE_TRANSPORT", Some("TransientRetry"))
        .await;

    // Pre-state sanity — MAC budget starts at 0 (no prior recovery).
    assert_eq!(
        read_mac_recovery_attempts(app.db(), doc).await,
        0,
        "pre-state: mac_recovery_attempts must start at 0"
    );

    let stub = StubDpsChannel::new(Ok(ack("server-fiscal-retry-99")));
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) State advances past ERROR_RETRYABLE.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "SENT",
        "§6.7 + ADR-M3-A9 step 5-6 — happy retry drives ErrorRetryable → Sending → Sent"
    );

    // (2) Exactly one send_chk invocation.
    assert_eq!(stub.call_count(), 1, "§6.7 — one wire send during recovery");

    // (2a) W11 PR-2a wiring observable in the dispatch histogram —
    // ERROR_RETRYABLE arm produced exactly one dispatch (not deferred).
    assert_eq!(
        summary.docs_advanced.error_retryable_dispatched, 1,
        "PR-2a wiring: ERROR_RETRYABLE Some(deps) path increments error_retryable_dispatched"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_deferred, 0,
        "PR-2a wiring: ERROR_RETRYABLE must NOT fall through to DEFERRED arm under Some(deps)"
    );
    assert_eq!(
        summary.docs_advanced.total_visited(),
        1,
        "exactly one pending doc dispatched, all other counters zero"
    );

    // (2b) No `BOOT_DISPATCH_DEFERRED` audit fired.
    assert_eq!(
        audit_count(app.db(), "BOOT_DISPATCH_DEFERRED").await,
        0,
        "ERROR_RETRYABLE dispatch arm wired in PR-2a; deferred audit must not fire"
    );

    // (3) CRITICAL — MAC-recovery budget untouched.
    // The W10 single-bit `mac_recovery_attempts` is reserved for
    // hash-mismatch MAC-recovery (ADR-M3-A10 §2 + migration 013); a
    // happy ERROR_RETRYABLE retry must NOT burn it.  Pre/post check
    // proves the W10 budget is orthogonal to the W9 retry counter.
    assert_eq!(
        read_mac_recovery_attempts(app.db(), doc).await,
        0,
        "mac_recovery_attempts must stay 0 — happy retry must not burn the MAC-hint budget"
    );

    // (4) No MAC-recovery audits fired.
    assert_eq!(
        audit_count(app.db(), "MAC_RECOVERY_CLAIM").await,
        0,
        "MAC recovery must not be invoked for non-MAC retry"
    );
    assert_eq!(
        audit_count(app.db(), "MAC_RECOVERY_RESIGNED").await,
        0,
        "MAC recovery resign must not fire for non-MAC retry"
    );
}

// ─── Local recovery stub — DpsChannel with last_chk + send_chk queues ──
//
// W11 PR-2b operator-decided design (2026-05-12 §9 Q4): the SENT
// recovery fixtures need a DpsChannel that exercises `last_chk` AND
// `send_chk` independently — the shared `StubDpsChannel` from
// `tests/common/mod.rs:79-150` has `last_chk = unreachable!()` as a
// default, which would panic the moment `dispatch_sent_via_probe`
// fires its first probe.  We did NOT extend the shared stub: SENT
// recovery is a recovery-specific shape (separate response queues per
// method + per-method counters); shaping the shared stub around
// `last_chk` would balloon the API surface and complicate the W7.5
// `send_chk`-only fixtures.  Local stub keeps each test surface
// minimal.
//
// **Concurrency contract.**  `std::sync::Mutex` (not
// `tokio::sync::Mutex`) — these fixtures drive recovery from a single
// task per fixture; locks are short-held and never cross an `.await`
// while held.

struct RecoveryDpsStub {
    last_chk_queue: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    send_chk_queue: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
    last_chk_count: AtomicUsize,
    send_chk_count: AtomicUsize,
}

impl RecoveryDpsStub {
    /// Tick-1 / single-probe use: only `last_chk` is consulted.  The
    /// `send_chk` queue stays empty — any call would pop nothing and
    /// panic, which is the desired "no resend during probe" guarantee.
    fn for_last_chk(responses: Vec<Result<CheckAck, DpsError>>) -> Self {
        Self {
            last_chk_queue: Mutex::new(responses.into()),
            send_chk_queue: Mutex::new(VecDeque::new()),
            last_chk_count: AtomicUsize::new(0),
            send_chk_count: AtomicUsize::new(0),
        }
    }

    /// Tick-2 use (e.g. fixture #6 retry tick): only `send_chk` is
    /// exercised because the doc is in ERROR_RETRYABLE and recovery
    /// dispatches through `stage_send::run` (ADR-M3-A9 step 5-6, NOT
    /// via the SENT probe path).
    fn for_send_chk(responses: Vec<Result<CheckAck, DpsError>>) -> Self {
        Self {
            last_chk_queue: Mutex::new(VecDeque::new()),
            send_chk_queue: Mutex::new(responses.into()),
            last_chk_count: AtomicUsize::new(0),
            send_chk_count: AtomicUsize::new(0),
        }
    }

    fn last_chk_count(&self) -> usize {
        self.last_chk_count.load(Ordering::SeqCst)
    }

    fn send_chk_count(&self) -> usize {
        self.send_chk_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DpsChannel for RecoveryDpsStub {
    async fn send_chk(&self, _envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        self.send_chk_count.fetch_add(1, Ordering::SeqCst);
        self.send_chk_queue
            .lock()
            .unwrap()
            .pop_front()
            .expect("RecoveryDpsStub.send_chk queue empty (caller forgot to enqueue)")
    }

    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.last_chk_count.fetch_add(1, Ordering::SeqCst);
        self.last_chk_queue
            .lock()
            .unwrap()
            .pop_front()
            .expect("RecoveryDpsStub.last_chk queue empty (caller forgot to enqueue)")
    }

    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("RecoveryDpsStub: ping not exercised");
    }

    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("RecoveryDpsStub: status_rro not exercised");
    }

    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("RecoveryDpsStub: info_rro not exercised");
    }
}

// ─── Seed helper for SENT crash-recovery fixtures (#4 / #5 / #6) ───────
//
// A doc in SENT state structurally implies: the W7 worker reached
// 4-b on a prior run, persisted `server_fiscal_no` from the wire ack
// alongside CAS `Sending → Sent`, and crashed before the W8 KVT1
// handoff.  Recovery probes `last_chk` against the persisted
// `server_fiscal_no` (== `transport_request_id` per W7.4
// canonicalisation).  Fixtures must mirror this shape: state=SENT,
// SIGNED_XML persisted, `server_fiscal_no` set.
async fn seed_doc_sent_with_server_fiscal_no(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
    server_fiscal_no: &str,
) -> DocumentId {
    let doc = seed_doc_with_signed_xml(pool, fn_id, doc_byte, "SENT").await;
    sqlx::query("UPDATE fiscal_documents SET server_fiscal_no = ? WHERE document_id = ?")
        .bind(server_fiscal_no)
        .bind(doc)
        .execute(pool)
        .await
        .expect("seed server_fiscal_no on SENT doc");
    doc
}

/// Read the most-recent `transport_trace` row for a doc — used by
/// SENT recovery fixtures to assert the outcome shape committed by
/// the dispatch tx.  Returns `(outcome_kind, server_fiscal_no)`.
async fn read_latest_transport_trace(
    pool: &SqlitePool,
    doc: DocumentId,
) -> (Option<String>, Option<String>) {
    sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT outcome_kind, server_fiscal_no FROM transport_trace \
         WHERE document_id = ? ORDER BY attempt_no DESC LIMIT 1",
    )
    .bind(doc)
    .fetch_one(pool)
    .await
    .expect("read transport_trace row")
}

async fn count_transport_trace(pool: &SqlitePool, doc: DocumentId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM transport_trace WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .expect("count transport_trace rows")
}

/// Seed a completed `transport_trace` row with a specific durable
/// `retry_class`.  Used by ER recovery fixtures to drive the new
/// `dispatch_error_retryable_by_class` dispatcher.  Default fields
/// match a minimal "happy enough" attempt that already completed:
/// `attempt_no=1`, `outcome_kind` provided by caller, dummy
/// `request_envelope_sha256`.
async fn seed_completed_transport_trace(
    pool: &SqlitePool,
    doc: DocumentId,
    outcome_kind: &str,
    retry_class: Option<&str>,
) {
    seed_completed_transport_trace_at_attempt(pool, doc, 1, outcome_kind, retry_class).await;
}

/// Seed a completed `transport_trace` row at a specific
/// `attempt_no`.  H2 budget-cap fixtures seed multiple rows
/// (`attempt_no = 1..=N`) so `transport_trace::attempts_used`
/// returns the total count and the dispatcher trips the
/// `MAX_BOOT_ATTEMPTS` cap.
async fn seed_completed_transport_trace_at_attempt(
    pool: &SqlitePool,
    doc: DocumentId,
    attempt_no: i32,
    outcome_kind: &str,
    retry_class: Option<&str>,
) {
    let envelope_sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO transport_trace (document_id, attempt_no, backend_profile_id, \
            transport_profile_id, request_envelope_sha256, completed_at, \
            wire_call_started_at, wire_call_finished_at, outcome_kind, retry_class) \
         VALUES (?, ?, 'b1', 't1', ?, '2026-04-22T12:00:00Z', \
            '2026-04-22T12:00:00Z', '2026-04-22T12:00:01Z', ?, ?)",
    )
    .bind(doc)
    .bind(attempt_no)
    .bind(&envelope_sha)
    .bind(outcome_kind)
    .bind(retry_class)
    .execute(pool)
    .await
    .expect("seed transport_trace row");
}

async fn read_server_fiscal_no(pool: &SqlitePool, doc: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .expect("read server_fiscal_no")
}

// ─── Fixture #4 — §6.4-a SENT probe Match → KVT1 ───────────────────────
//
// W0-3 §6.4-a:744-762 mandates: a SENT doc whose `last_chk` returns
// an ack matching the persisted `transport_request_id` was
// successfully fiscalised pre-crash; recovery advances state
// `Sent → Kvt1` locally and persists the receipt bytes
// (`ack.data_sign`) as the KVT1_RAW artifact.
//
// **Critical assertions:**
//   (1) state SENT → KVT1.
//   (2) KVT1_RAW persisted byte-for-byte from `ack.data_sign`.
//   (3) `last_chk_count == 1`, `send_chk_count == 0` — recovery is
//       probe-only on the match branch.
//   (4) histogram counter `sent_match_to_kvt1 == 1`, peers zero.
//   (5) Zero `BOOT_DISPATCH_DEFERRED` audit — SENT dispatch arm
//       wired in PR-2b (this PR); deferred audit must not fire.
//   (6) `transport_trace` recovery row completed with `outcome_kind=OK`.

#[tokio::test]
async fn fixture_4_sent_last_chk_match_advances_to_kvt1() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let expected_id = "expected-id-44";
    let doc = seed_doc_sent_with_server_fiscal_no(app.db(), fn_id, 0x44, expected_id).await;

    let ack_data_sign = vec![0xDDu8; 32];
    let stub = RecoveryDpsStub::for_last_chk(vec![Ok(CheckAck {
        id: expected_id.into(),
        id_sign: vec![],
        data_sign: ack_data_sign.clone(),
    })]);
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) State SENT → KVT1.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "KVT1",
        "§6.4-a Match must advance Sent → Kvt1"
    );

    // (2) KVT1_RAW persisted byte-for-byte from ack.data_sign.
    assert_eq!(
        read_document_file_kind(app.db(), doc, "KVT1_RAW")
            .await
            .as_deref(),
        Some(ack_data_sign.as_slice()),
        "§6.4-a KVT1_RAW must carry ack.data_sign verbatim"
    );

    // (3) Exactly one last_chk call; zero send_chk (no re-send).
    assert_eq!(
        stub.last_chk_count(),
        1,
        "§6.4-a Match: exactly one last_chk probe issued"
    );
    assert_eq!(
        stub.send_chk_count(),
        0,
        "§6.4-a Match: send_chk must NOT be invoked during probe recovery"
    );

    // (4) Histogram — sent_match_to_kvt1 = 1, peers zero.
    assert_eq!(
        summary.docs_advanced.sent_match_to_kvt1, 1,
        "PR-2b wiring: probe Match increments sent_match_to_kvt1"
    );
    assert_eq!(
        summary.docs_advanced.sent_deferred, 0,
        "PR-2b wiring: SENT must NOT fall through to DEFERRED under Some(deps)"
    );
    assert_eq!(
        summary.docs_advanced.sent_mismatch_to_manual, 0,
        "peer counter must stay zero"
    );
    assert_eq!(
        summary.docs_advanced.sent_not_found_to_error_retryable, 0,
        "peer counter must stay zero"
    );
    assert_eq!(
        summary.docs_advanced.sent_probe_failure_deferred, 0,
        "peer counter must stay zero"
    );
    assert_eq!(
        summary.docs_advanced.total_visited(),
        1,
        "exactly one pending doc dispatched"
    );

    // (5) No deferred audit — PR-2b wires the SENT arm.
    assert_eq!(
        audit_count(app.db(), "BOOT_DISPATCH_DEFERRED").await,
        0,
        "SENT dispatch arm wired in PR-2b; deferred audit must not fire"
    );

    // (6) transport_trace recovery row committed with OK outcome.
    let (outcome_kind, _) = read_latest_transport_trace(app.db(), doc).await;
    assert_eq!(
        outcome_kind.as_deref(),
        Some("OK"),
        "§6.4-a Match: transport_trace recovery row completed with OK outcome"
    );

    // (7) Forensic audit fired — proves the match codepath ran end-
    //     to-end (not a CAS short-circuit).
    assert_eq!(
        audit_count(app.db(), "BOOT_LAST_CHK_MATCH_KVT1").await,
        1,
        "§6.4-a Match: BOOT_LAST_CHK_MATCH_KVT1 audit fires exactly once"
    );
}

// ─── Fixture #5 — §6.4-b SENT probe Mismatch → RequiresManualReconciliation ──
//
// W0-3 §6.4-b:766-784 mandates: a SENT doc whose `last_chk` returns
// an ack with a DIFFERENT id (DPS has a different last-submitted
// check than ours) cannot be locally proven fiscalised.  Either DPS
// never received our doc, or another doc was submitted between our
// crash and reboot; either way the operator must triage.  Recovery
// transitions `Sent → RequiresManualReconciliation` via the W11
// prep-PR whitelist edge (PR #35).
//
// **Critical assertions:**
//   (1) state SENT → REQUIRES_MANUAL_RECONCILIATION.
//   (2) `last_chk_count == 1`, `send_chk_count == 0`.
//   (3) histogram counter `sent_mismatch_to_manual == 1`, peers zero.
//   (4) audit `BOOT_SENT_LAST_CHK_MISMATCH_RM` fires (Severity::Error).
//   (5) `transport_trace` row with `outcome_kind=REJECTED` AND
//       `server_fiscal_no = Some(actual_id_from_dps)` for forensics.

#[tokio::test]
async fn fixture_5_sent_last_chk_mismatch_to_manual_reconciliation() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let expected_id = "expected-id-55";
    let actual_id = "different-id-55";
    let doc = seed_doc_sent_with_server_fiscal_no(app.db(), fn_id, 0x55, expected_id).await;

    let stub = RecoveryDpsStub::for_last_chk(vec![Ok(CheckAck {
        id: actual_id.into(),
        id_sign: vec![],
        data_sign: vec![],
    })]);
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) State SENT → REQUIRES_MANUAL_RECONCILIATION.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "§6.4-b Mismatch must transition Sent → RequiresManualReconciliation (whitelist edge from PR #35)"
    );

    // (2) Exactly one last_chk; zero send_chk.
    assert_eq!(stub.last_chk_count(), 1, "§6.4-b: exactly one last_chk");
    assert_eq!(
        stub.send_chk_count(),
        0,
        "§6.4-b: send_chk must NOT be invoked"
    );

    // (3) Histogram — sent_mismatch_to_manual = 1, peers zero.
    assert_eq!(
        summary.docs_advanced.sent_mismatch_to_manual, 1,
        "PR-2b wiring: probe Mismatch increments sent_mismatch_to_manual"
    );
    assert_eq!(
        summary.docs_advanced.sent_match_to_kvt1, 0,
        "peer counter must stay zero"
    );
    assert_eq!(
        summary.docs_advanced.sent_deferred, 0,
        "PR-2b wiring: SENT must NOT fall through to DEFERRED under Some(deps)"
    );
    assert_eq!(
        summary.docs_advanced.total_visited(),
        1,
        "exactly one pending doc dispatched"
    );

    // (4) Audit fired exactly once with Error severity (operator
    //     handoff signal).
    assert_eq!(
        audit_count(app.db(), "BOOT_SENT_LAST_CHK_MISMATCH_RM").await,
        1,
        "§6.4-b: BOOT_SENT_LAST_CHK_MISMATCH_RM audit fires exactly once"
    );
    assert_eq!(
        audit_count(app.db(), "BOOT_DISPATCH_DEFERRED").await,
        0,
        "SENT dispatch arm wired in PR-2b; deferred audit must not fire"
    );

    // (5) transport_trace row with REJECTED outcome + actual_id from DPS.
    let (outcome_kind, server_fiscal_no) = read_latest_transport_trace(app.db(), doc).await;
    assert_eq!(
        outcome_kind.as_deref(),
        Some("REJECTED"),
        "§6.4-b Mismatch: transport_trace recovery row completed with REJECTED outcome"
    );
    assert_eq!(
        server_fiscal_no.as_deref(),
        Some(actual_id),
        "§6.4-b Mismatch: transport_trace records the DPS-returned actual_id for forensics"
    );
}

// ─── Fixture #6 — §6.4-c SENT probe NotFound → two-tick retry path ─────
//
// W0-3 §6.4-c:788-820 mandates: a SENT doc whose `last_chk` returns
// `NotFound` (DPS has no record of any check for our FN_sign) was
// safely NOT received — Pattern B re-drive via
// `ErrorRetryable → Sending → wire` is the correct recovery.
// ADR-M3-A9 step 3 forbids the direct `Sent → Sending` edge: recovery
// MUST hop through `ErrorRetryable`.  This is the two-tick path,
// operator-decided per W11 design doc §9 Q1 (2026-05-12).
//
// **CRITICAL — the load-bearing fixture in PR-2b.**  Proves the
// two-tick driver is structural, not inline:
//   - Tick 1: probe → state `Sent → ErrorRetryable`, NO send_chk.
//   - Tick 2: NEW deps (separate stub with happy `send_chk` queue),
//     dispatch via `stage_send::run` drives `ER → Sending → Sent`.
//
// **Hard assertions:**
//   (1.a) Tick-1 state → ERROR_RETRYABLE.
//   (1.b) Tick-1 `last_chk_count == 1`, `send_chk_count == 0`.
//   (1.c) Tick-1 `sent_not_found_to_error_retryable == 1`.
//   (1.d) Tick-1 `BOOT_SENT_LAST_CHK_NOTFOUND` audit fires.
//   (1.e) Tick-1: NO `BOOT_RESUME_*` audit (NO direct Sent → Sending).
//   (2.a) Tick-2 state → SENT.
//   (2.b) Tick-2 NEW stub's `send_chk_count == 1`.
//   (2.c) Tick-2 `error_retryable_dispatched == 1`.
//   (2.d) Total `transport_trace` rows == 2 (one per tick).
//   (2.e) Doc's server_fiscal_no updated to tick-2's fresh id.

#[tokio::test]
async fn fixture_6_sent_last_chk_notfound_two_tick_retry_path() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let expected_id = "expected-id-66";
    let doc = seed_doc_sent_with_server_fiscal_no(app.db(), fn_id, 0x66, expected_id).await;

    // ── Tick 1 — last_chk NotFound → CAS Sent → ErrorRetryable ─────
    let tick1_stub = RecoveryDpsStub::for_last_chk(vec![Err(DpsError::NotFound)]);
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let tick1_deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &tick1_stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let tick1_summary = app
        .reconcile_pending_with(tick1_deps)
        .await
        .expect("tick 1 reconcile_pending_with green");

    // (1.a) State → ERROR_RETRYABLE.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "§6.4-c tick-1 NotFound must CAS Sent → ErrorRetryable (NOT direct Sent → Sending per ADR-M3-A9 step 3)"
    );

    // (1.b) One last_chk, zero send_chk on tick 1.
    assert_eq!(
        tick1_stub.last_chk_count(),
        1,
        "tick-1: exactly one last_chk"
    );
    assert_eq!(
        tick1_stub.send_chk_count(),
        0,
        "tick-1: send_chk MUST NOT fire — recovery is probe-only on the NotFound branch"
    );

    // (1.c) Histogram — sent_not_found_to_error_retryable == 1.
    assert_eq!(
        tick1_summary
            .docs_advanced
            .sent_not_found_to_error_retryable,
        1,
        "PR-2b wiring: NotFound increments sent_not_found_to_error_retryable"
    );
    assert_eq!(
        tick1_summary.docs_advanced.error_retryable_dispatched, 0,
        "tick-1: ER dispatch must NOT fire (doc was SENT at tick start)"
    );
    assert_eq!(
        tick1_summary.docs_advanced.total_visited(),
        1,
        "tick-1: exactly one pending doc dispatched"
    );

    // (1.d) Forensic audit fired.
    assert_eq!(
        audit_count(app.db(), "BOOT_SENT_LAST_CHK_NOTFOUND").await,
        1,
        "§6.4-c tick-1: BOOT_SENT_LAST_CHK_NOTFOUND audit fires exactly once"
    );

    // (1.e) **Load-bearing**: NO direct Sent → Sending audit.  ADR-M3-A9
    //       step 3 forbids the direct edge; the only audit on this
    //       tick must be BOOT_SENT_LAST_CHK_NOTFOUND.  W9's
    //       `BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE` could only fire
    //       if the doc were in SENDING (it is not — fixture seeds SENT).
    assert_eq!(
        audit_count(app.db(), "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        0,
        "two-tick contract: no SENDING-resume audit (doc was SENT, not SENDING)"
    );
    assert_eq!(
        audit_count(app.db(), "STAGE_SEND_RESULT").await,
        0,
        "two-tick contract: stage_send::run must NOT run on tick-1 (probe-only branch)"
    );
    assert_eq!(
        count_transport_trace(app.db(), doc).await,
        1,
        "tick-1: exactly one transport_trace row (the probe recovery row)"
    );

    // ── Tick 2 — NEW deps + happy send_chk → ER → Sending → Sent ───
    let fresh_id = "fresh-fiscal-66";
    let tick2_stub = RecoveryDpsStub::for_send_chk(vec![Ok(CheckAck {
        id: fresh_id.into(),
        id_sign: vec![],
        data_sign: vec![],
    })]);
    let tick2_deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &tick2_stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let tick2_summary = app
        .reconcile_pending_with(tick2_deps)
        .await
        .expect("tick 2 reconcile_pending_with green");

    // (2.a) State → SENT.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "SENT",
        "§6.4-c tick-2 + ADR-M3-A9 steps 5-6: stage_send::run drives ER → Sending → Sent"
    );

    // (2.b) Tick-2 NEW stub: send_chk count == 1.
    assert_eq!(
        tick2_stub.send_chk_count(),
        1,
        "tick-2: exactly one fresh send_chk on the retry"
    );
    assert_eq!(
        tick2_stub.last_chk_count(),
        0,
        "tick-2: last_chk must NOT fire — doc is in ER, not SENT, when tick-2 starts"
    );

    // (2.c) Tick-2 histogram — error_retryable_dispatched == 1
    //       (from PR-2a wiring; PR-2b reuses unchanged).
    assert_eq!(
        tick2_summary.docs_advanced.error_retryable_dispatched, 1,
        "tick-2: PR-2a ER dispatch wiring increments error_retryable_dispatched"
    );
    assert_eq!(
        tick2_summary
            .docs_advanced
            .sent_not_found_to_error_retryable,
        0,
        "tick-2: NotFound branch must NOT re-fire (doc was ER at tick start)"
    );
    assert_eq!(
        tick2_summary.docs_advanced.total_visited(),
        1,
        "tick-2: exactly one pending doc dispatched"
    );

    // (2.d) Total transport_trace rows across both ticks == 2.
    assert_eq!(
        count_transport_trace(app.db(), doc).await,
        2,
        "two-tick contract: tick-1 probe row + tick-2 send row"
    );

    // (2.e) Doc's server_fiscal_no updated by stage_send 4b.
    assert_eq!(
        read_server_fiscal_no(app.db(), doc).await.as_deref(),
        Some(fresh_id),
        "tick-2 stage_send 4b: server_fiscal_no overwritten with fresh wire id"
    );

    // (3) Sanity: no deferred audit at any tick.
    assert_eq!(
        audit_count(app.db(), "BOOT_DISPATCH_DEFERRED").await,
        0,
        "two-tick: PR-2a (ER) + PR-2b (SENT) arms wired; deferred audit must not fire"
    );
}

// ─── Seed helpers for fixture #1 (PREPARED end-to-end) ─────────────────
//
// PREPARED crash recovery dispatches via `stage_sign::run` → typed
// payload parse → canonical XML build.  The seed must therefore
// carry a real SELL payload (not the `'{}'` sentinel
// `seed_doc_in_state` uses) AND a matching ingress_inbox row in
// PROCESSING (the live worker flips inbox → PROCESSING in stage 1;
// stage_finalize step 5 will flip it → DONE later).  W6 stage_sign
// also needs an Opened shift in node_state (`check_shift_guard`
// passes only for SELL on `ShiftState::Opened`).

const FIXTURE_1_PAYLOAD_JSON: &str = r#"{"items":[{"code":"A1","name":"X","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"CASH","sum_kop":15000,"type_code":"0"}]}"#;

const FIXTURE_1_TOTAL_SUM_KOP: i64 = 15000;

/// Seed a PREPARED SELL doc end-to-end so `dispatch_prepared_via_chain`
/// can drive it through `stage_sign::run` + `stage_send::run`.
async fn seed_doc_prepared_full(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
) -> (DocumentId, [u8; 16]) {
    // W14a-2b Commit 5: SELL needs shift_id + signer attribution so
    // signer_guard at stage_send 4-pre returns Ok.
    let shift_byte = doc_byte ^ 0x80;
    let shift_bytes = vec![shift_byte; 16];
    sqlx::query(
        "INSERT OR IGNORE INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(&shift_bytes)
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();

    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            total_sum_kop, payload_json, payload_sha256_canonical, signed_by_cashier_id) \
         VALUES (?, ?, ?, ?, ?, 'SELL', 'PREPARED', 'b1', 't1', 'ONLINE', \
            '2026-04-22T12:00:00Z', ?, ?, ?, 'test-cashier')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(&shift_bytes)
    .bind(lnd)
    .bind(FIXTURE_1_TOTAL_SUM_KOP)
    .bind(FIXTURE_1_PAYLOAD_JSON)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
    let doc_id = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let req_arr: [u8; 16] = <[u8; 16]>::try_from(req_bytes.as_slice()).unwrap();
    (doc_id, req_arr)
}

/// Seed an `ingress_inbox` row in PROCESSING with the matching SELL
/// payload — required by `dispatch_prepared_via_chain` snapshot read
/// AND (downstream) `stage_finalize::mark_done_tx` if the chain ever
/// reaches Ack.  Seed payload matches the doc's so a future
/// payload-hash invariant guard wouldn't trip.
async fn seed_inbox_processing_for_sell(pool: &SqlitePool, fn_id: &str, req_id: &[u8; 16]) {
    let sha = vec![0u8; 32];
    let req_slice: &[u8] = req_id;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', ?, ?, ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(fn_id)
    .bind(format!("idem-{:02x}", req_id[0]))
    .bind(FIXTURE_1_PAYLOAD_JSON)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_shift_and_node(
    pool: &SqlitePool,
    fn_id: &str,
) -> prro::db::models::ids::ShiftId {
    use prro::db::models::ids::ShiftId;
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(shift_id)
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, current_shift_id, \
            next_lnd, backend_profile_id, transport_profile_id) \
         VALUES (?, 'ONLINE', 'OPENED', ?, 1, 'b1', 't1')",
    )
    .bind(fn_id)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
    shift_id
}

// ─── Fixture #1 — §6.1 PREPARED crash → stage_sign + stage_send chain ──
//
// W0-3 §6.1:670-689 mandates: a PREPARED doc crashed BEFORE the W6
// signing stage ran.  Recovery drives forward through the canonical
// `stage_sign::run` → `stage_send::run` chain.  This fixture is the
// end-to-end proof that PR-2b's `dispatch_prepared_via_chain` wiring
// reconstructs `WorkerContext` from DB rows AND lets the chain
// produce the same final-state shape as the live `process_request`
// path.
//
// **Assertions:**
//   (1) state PREPARED → SENT (drives sign + send chain to wire ack).
//   (2) SIGNED_XML + PAYLOAD_XML document_files rows created by
//       stage_sign 3-PERSIST.
//   (3) send_chk fired exactly once (no resend).
//   (4) histogram counter `prepared_dispatched == 1`, peers zero.
//   (5) Zero `BOOT_DISPATCH_DEFERRED` audit — PREPARED arm wired in
//       PR-2b; deferred audit must not fire under Some(deps).
//   (6) transport_trace OK row recorded (proves stage_send 4-b
//       committed cleanly).

#[tokio::test]
async fn fixture_1_prepared_crash_replays_to_sent_via_sign_send_chain() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    let _shift_id = seed_open_shift_and_node(app.db(), fn_id).await;
    let (doc, req_id) = seed_doc_prepared_full(app.db(), fn_id, 0x11).await;
    seed_inbox_processing_for_sell(app.db(), fn_id, &req_id).await;

    let stub = StubDpsChannel::new(Ok(ack("server-fiscal-prepared-11")));
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) State PREPARED → SENT (sign + send chain drove it forward).
    assert_eq!(
        doc_state(app.db(), doc).await,
        "SENT",
        "§6.1: dispatch_prepared_via_chain drives Prepared → Signed → Sending → Sent"
    );

    // (2) Both stage_sign artefacts persisted by 3-PERSIST.
    assert!(
        read_document_file_kind(app.db(), doc, "SIGNED_XML")
            .await
            .is_some(),
        "stage_sign 3-PERSIST must INSERT SIGNED_XML document_files row"
    );
    assert!(
        read_document_file_kind(app.db(), doc, "PAYLOAD_XML")
            .await
            .is_some(),
        "stage_sign 3-PERSIST must INSERT PAYLOAD_XML document_files row"
    );

    // (3) Exactly one send_chk fired.
    assert_eq!(
        stub.call_count(),
        1,
        "§6.1: exactly one wire send during PREPARED recovery"
    );

    // (4) Histogram — prepared_dispatched == 1, peers zero.
    assert_eq!(
        summary.docs_advanced.prepared_dispatched, 1,
        "PR-2b wiring: PREPARED Some(deps) path increments prepared_dispatched"
    );
    assert_eq!(
        summary.docs_advanced.prepared_deferred, 0,
        "PR-2b wiring: PREPARED must NOT fall through to DEFERRED arm under Some(deps)"
    );
    assert_eq!(
        summary.docs_advanced.signed_dispatched, 0,
        "peer counter must stay zero — recovery entered via PREPARED, not SIGNED"
    );
    assert_eq!(
        summary.docs_advanced.total_visited(),
        1,
        "exactly one pending doc dispatched"
    );

    // (5) No deferred audit — PREPARED arm wired in PR-2b.
    assert_eq!(
        audit_count(app.db(), "BOOT_DISPATCH_DEFERRED").await,
        0,
        "PREPARED dispatch arm wired in PR-2b; deferred audit must not fire"
    );

    // (6) transport_trace OK row recorded by stage_send 4-b.
    let (outcome_kind, server_fiscal_no) = read_latest_transport_trace(app.db(), doc).await;
    assert_eq!(
        outcome_kind.as_deref(),
        Some("OK"),
        "stage_send 4-b committed transport_trace with OK outcome"
    );
    assert_eq!(
        server_fiscal_no.as_deref(),
        Some("server-fiscal-prepared-11"),
        "transport_trace records the wire-returned server_fiscal_no"
    );
}

// ─── M3a hardening pass 1 — per-FN resolver multi-FN proof ─────────────
//
// Closes HIGH #2 (singleton deps risk).  Proves the resolver shape:
// two FNs with distinct fn_sign blobs, each in SENT crash-recovery,
// resolved to its own RuntimeView at dispatch time.  A per-FN stub
// records which fn_sign blob `last_chk` received per call; the
// fixture asserts FN-A's recovery used fn_sign-A and FN-B's used
// fn_sign-B.

/// Records the fn_sign blob bytes per `last_chk` call so the
/// fixture can prove the per-FN resolver delivered the right
/// identity to each recovery branch.  The scripted response queue
/// drives `last_chk` outcomes; `send_chk` is panic-armed because
/// SENT probe recovery must NEVER fire a resend.
struct PerFnRecordingDpsStub {
    recorded_blobs: Mutex<Vec<Vec<u8>>>,
    scripted: Mutex<VecDeque<Result<CheckAck, DpsError>>>,
}

impl PerFnRecordingDpsStub {
    fn new(responses: Vec<Result<CheckAck, DpsError>>) -> Self {
        Self {
            recorded_blobs: Mutex::new(Vec::new()),
            scripted: Mutex::new(responses.into()),
        }
    }

    fn recorded_fn_signs(&self) -> Vec<Vec<u8>> {
        self.recorded_blobs.lock().unwrap().clone()
    }
}

#[async_trait]
impl DpsChannel for PerFnRecordingDpsStub {
    async fn send_chk(&self, _envelope: CheckEnvelope) -> Result<CheckAck, DpsError> {
        panic!("PerFnRecordingDpsStub: send_chk must not fire during SENT probe recovery")
    }

    async fn last_chk(&self, blob: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.recorded_blobs.lock().unwrap().push(blob.0.clone());
        self.scripted
            .lock()
            .unwrap()
            .pop_front()
            .expect("PerFnRecordingDpsStub.last_chk queue empty")
    }

    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("PerFnRecordingDpsStub: ping not exercised");
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("PerFnRecordingDpsStub: status_rro not exercised");
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("PerFnRecordingDpsStub: info_rro not exercised");
    }
}

#[tokio::test]
async fn multi_fn_reconcile_pending_with_resolves_runtime_per_fn() {
    let (_dir, app) = fresh_app().await;
    let fn_a = "1111111110";
    let fn_b = "2222222220";

    // Seed both FNs as registered + both with SENT crash-resume
    // pending docs and the persisted server_fiscal_no required by
    // the SENT probe path.
    seed_fn_config(app.db(), fn_a).await;
    seed_fn_config(app.db(), fn_b).await;
    seed_node_state(app.db(), fn_a, "ONLINE", "CLOSED", 1).await;
    seed_node_state(app.db(), fn_b, "ONLINE", "CLOSED", 1).await;
    let doc_a = seed_doc_sent_with_server_fiscal_no(app.db(), fn_a, 0xAA, "fiscal-A").await;
    let doc_b = seed_doc_sent_with_server_fiscal_no(app.db(), fn_b, 0xBB, "fiscal-B").await;

    // Two distinct fn_sign blobs.  In production each blob is the
    // operator DPS identity for that FN; in this test we just need
    // them to be byte-distinguishable so the recording stub can
    // prove the resolver delivered the right one per FN.
    let fn_sign_a = CheckSignBlob(vec![0xA1u8; 8]);
    let fn_sign_b = CheckSignBlob(vec![0xB2u8; 8]);

    let stub_a = PerFnRecordingDpsStub::new(vec![Ok(CheckAck {
        id: "fiscal-A".into(),
        id_sign: vec![],
        data_sign: vec![0xAA; 32],
    })]);
    let stub_b = PerFnRecordingDpsStub::new(vec![Ok(CheckAck {
        id: "fiscal-B".into(),
        id_sign: vec![],
        data_sign: vec![0xBB; 32],
    })]);

    let signing_ctx = det_signing_ctx();

    // Per-FN resolver: FN-A → (stub_a, fn_sign_a); FN-B → (stub_b,
    // fn_sign_b); anything else → None (proves the "no foreign
    // identity" guarantee).
    let deps = ReconciliationRuntime::with_resolver(|fn_id: &str| -> Option<RuntimeView<'_>> {
        if fn_id == fn_a {
            Some(RuntimeView {
                dps: &stub_a,
                signing_ctx: &signing_ctx,
                fn_sign: &fn_sign_a,
            })
        } else if fn_id == fn_b {
            Some(RuntimeView {
                dps: &stub_b,
                signing_ctx: &signing_ctx,
                fn_sign: &fn_sign_b,
            })
        } else {
            None
        }
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("multi-FN reconcile_pending_with green");

    // (1) Both docs advanced via probe Match (last_chk returned
    // the expected ack.id).
    assert_eq!(
        doc_state(app.db(), doc_a).await,
        "KVT1",
        "FN-A SENT probe Match must drive Sent → KVT1"
    );
    assert_eq!(
        doc_state(app.db(), doc_b).await,
        "KVT1",
        "FN-B SENT probe Match must drive Sent → KVT1"
    );

    // (2) Histogram aggregates both: 2 sent_match_to_kvt1.
    assert_eq!(
        summary.docs_advanced.sent_match_to_kvt1, 2,
        "PR-2b multi-FN: both FNs produce sent_match_to_kvt1"
    );

    // (3) **Load-bearing**: each FN's stub received exactly one
    // last_chk call AND that call carried its OWN fn_sign blob.
    let calls_a = stub_a.recorded_fn_signs();
    let calls_b = stub_b.recorded_fn_signs();
    assert_eq!(
        calls_a.len(),
        1,
        "FN-A stub: exactly one last_chk call (its own SENT doc)"
    );
    assert_eq!(
        calls_b.len(),
        1,
        "FN-B stub: exactly one last_chk call (its own SENT doc)"
    );
    assert_eq!(
        calls_a[0], fn_sign_a.0,
        "HIGH #2 closure: FN-A's stub MUST receive fn_sign_a (NOT fn_sign_b — that would be foreign identity leak)"
    );
    assert_eq!(
        calls_b[0], fn_sign_b.0,
        "HIGH #2 closure: FN-B's stub MUST receive fn_sign_b (NOT fn_sign_a — that would be foreign identity leak)"
    );

    // (4) Cross-check: blobs are byte-distinguishable, so a false
    // positive (both ended up with the same blob) is impossible.
    assert_ne!(
        fn_sign_a.0, fn_sign_b.0,
        "fixture pre-condition: fn_sign blobs must differ to be distinguishable"
    );
}

// ─── M3a hardening pass 1 — ER retry_class dispatcher fixtures ─────────

// Fixture 9b — FnConfigError (-13/-14) escalation to manual.
#[tokio::test]
async fn fixture_9b_er_fn_config_error_escalates_to_manual_reconciliation() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0xCB, "ERROR_RETRYABLE").await;
    seed_completed_transport_trace(app.db(), doc, "REJECTED", Some("FnConfigError")).await;

    let stub = dps_panic_on_any_method(
        "FnConfigError → manual escalation must NOT touch DPS (Pattern B no-resend)",
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "FnConfigError must escalate ER → RequiresManualReconciliation"
    );
    assert_eq!(stub.call_count(), 0, "manual escalation: zero DPS calls");
    assert_eq!(
        summary.docs_advanced.error_retryable_escalated_to_manual, 1,
        "histogram: FnConfigError increments escalated counter"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_dispatched, 0,
        "TransientRetry dispatch path must NOT fire for FnConfigError"
    );
    assert_eq!(
        audit_count(app.db(), "BOOT_ER_ESCALATED_TO_MANUAL").await,
        1,
        "BOOT_ER_ESCALATED_TO_MANUAL audit emitted exactly once"
    );
}

// Fixture 9c — ProbeRequired retry_class held without state change.
#[tokio::test]
async fn fixture_9c_er_probe_required_defers_with_audit() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0xCC, "ERROR_RETRYABLE").await;
    seed_completed_transport_trace(app.db(), doc, "RETRYABLE_SERVER", Some("ProbeRequired")).await;

    let stub = dps_panic_on_any_method(
        "ProbeRequired hold must NOT touch DPS (submit-time probe deferred to M5)",
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "ProbeRequired hold: doc stays in ER, no state change"
    );
    assert_eq!(stub.call_count(), 0, "ProbeRequired hold: zero DPS calls");
    assert_eq!(
        summary.docs_advanced.error_retryable_probe_deferred, 1,
        "histogram: ProbeRequired increments probe-deferred counter"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_escalated_to_manual, 0,
        "ProbeRequired must NOT escalate to manual (different from FnConfigError)"
    );
    assert_eq!(
        audit_count(app.db(), "BOOT_ER_PROBE_DEFERRED").await,
        1,
        "BOOT_ER_PROBE_DEFERRED audit emitted exactly once"
    );
}

// Fixture 9d — Indeterminate (missing / NULL / unknown) retry_class held.
#[tokio::test]
async fn fixture_9d_er_indeterminate_retry_class_defers_with_audit() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0xCD, "ERROR_RETRYABLE").await;
    // NO transport_trace row seeded → `last_attempt_retry_class_for`
    // returns None → indeterminate hold.

    let stub =
        dps_panic_on_any_method("Indeterminate hold must NOT touch DPS (no durable evidence)");
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "Indeterminate hold: doc stays in ER, no state change"
    );
    assert_eq!(stub.call_count(), 0, "Indeterminate hold: zero DPS calls");
    assert_eq!(
        summary.docs_advanced.error_retryable_indeterminate_deferred, 1,
        "histogram: indeterminate increments indeterminate-deferred counter"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_dispatched, 0,
        "Indeterminate must NOT route to stage_send (would crash-loop)"
    );
    assert_eq!(
        audit_count(app.db(), "BOOT_ER_RETRY_CLASS_INDETERMINATE").await,
        1,
        "BOOT_ER_RETRY_CLASS_INDETERMINATE audit emitted exactly once"
    );
}

// Fixture 9e — TerminalReject in ER is structurally inconsistent;
// escalate with CRITICAL severity.
#[tokio::test]
async fn fixture_9e_er_terminal_reject_escalates_critical() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0xCE, "ERROR_RETRYABLE").await;
    // Synthetic inconsistency: TerminalReject SHOULD have routed the
    // doc to Rejected directly per error_routing::route_dps_error,
    // so observing it in ER is durable evidence of routing/CAS skew.
    seed_completed_transport_trace(app.db(), doc, "REJECTED", Some("TerminalReject")).await;

    let stub = dps_panic_on_any_method(
        "TerminalReject in ER is inconsistent durable state — no DPS contact",
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "TerminalReject in ER must escalate ER → RequiresManualReconciliation"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_escalated_to_manual, 1,
        "histogram: TerminalReject increments escalated counter"
    );
    // Verify the audit payload carries Critical severity (operator
    // alerting on structural-skew vs ordinary operator-triage class).
    let critical_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'BOOT_ER_ESCALATED_TO_MANUAL' AND severity = 'CRITICAL'",
    )
    .fetch_one(app.db())
    .await
    .expect("count critical escalation audits");
    assert_eq!(
        critical_count, 1,
        "TerminalReject escalation MUST emit Severity::Critical (structural inconsistency)"
    );
}

// Fixture 9f — resolver returns None for an FN with ctx-needy docs:
// recovery falls through to the deferred path, NO foreign identity
// substitution.  Closes the "what if resolver yields None" half of
// HIGH #2.
#[tokio::test]
async fn fixture_9f_resolver_none_defers_with_audit() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0xCF, "ERROR_RETRYABLE").await;
    seed_completed_transport_trace(app.db(), doc, "RETRYABLE_TRANSPORT", Some("TransientRetry"))
        .await;

    // Resolver intentionally returns None for every FN — simulates
    // the operator not having registered an identity binding for
    // this FN.  Hardening contract: recovery MUST NOT borrow a
    // singleton "default" identity; doc falls through to the
    // deferred path.
    let deps =
        ReconciliationRuntime::with_resolver(|_fn_id: &str| -> Option<RuntimeView<'_>> { None });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "resolver=None: doc stays in ER, no recovery dispatched"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_deferred, 1,
        "resolver=None must fall through to legacy deferred path"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_dispatched, 0,
        "no TransientRetry dispatch despite class match — deps unavailable"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_escalated_to_manual, 0,
        "no escalation either — resolver=None is operator config gap, not durable evidence"
    );
    // The deferred audit fires through the existing
    // emit_ctx_needy_deferred path (deps_available=false).
    assert_eq!(
        audit_count(app.db(), "BOOT_DISPATCH_DEFERRED").await,
        1,
        "BOOT_DISPATCH_DEFERRED audit fires for ctx-needy doc under resolver=None"
    );
}

// ─── M3a hardening pass 1 — PREPARED replay drift fixture ──────────────

#[tokio::test]
async fn fixture_1b_prepared_replay_drift_holds_with_critical_audit() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    let _shift = seed_open_shift_and_node(app.db(), fn_id).await;
    let (doc, req_id) = seed_doc_prepared_full(app.db(), fn_id, 0x1B).await;

    // Seed the inbox row with a DIFFERENT payload hash than the doc.
    // Live stage_acquire would have rejected this combination at
    // first ingress (command vs inbox mismatch); recovery encounters
    // it post-hoc only if the DB drifted between sessions.
    let mismatched_sha = vec![0xEE; 32];
    let req_slice: &[u8] = &req_id;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', ?, ?, ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(fn_id)
    .bind(format!("idem-{:02x}", req_id[0]))
    .bind(FIXTURE_1_PAYLOAD_JSON)
    .bind(&mismatched_sha)
    .execute(app.db())
    .await
    .unwrap();

    let stub = dps_panic_on_any_method("PREPARED drift hold must NOT touch DPS");
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "PREPARED",
        "drift detected: doc stays in PREPARED, no sign/send invoked"
    );
    assert_eq!(stub.call_count(), 0, "drift hold: zero DPS calls");
    assert_eq!(
        summary.docs_advanced.prepared_replay_drift_deferred, 1,
        "histogram: drift increments prepared_replay_drift_deferred"
    );
    assert_eq!(
        summary.docs_advanced.prepared_dispatched, 0,
        "drift must NOT proceed to stage_sign + stage_send"
    );
    // Verify CRITICAL severity (structural inconsistency, not
    // business reject).
    let critical_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'BOOT_PREPARED_REPLAY_DRIFT' AND severity = 'CRITICAL'",
    )
    .fetch_one(app.db())
    .await
    .expect("count critical drift audits");
    assert_eq!(
        critical_count, 1,
        "BOOT_PREPARED_REPLAY_DRIFT must emit Severity::Critical"
    );
    // Sanity: no SIGNED_XML / PAYLOAD_XML created (stage_sign was
    // never invoked).
    assert!(
        read_document_file_kind(app.db(), doc, "SIGNED_XML")
            .await
            .is_none(),
        "stage_sign must NOT run on drift detection"
    );
    assert!(
        read_document_file_kind(app.db(), doc, "PAYLOAD_XML")
            .await
            .is_none(),
        "stage_sign must NOT run on drift detection"
    );
}

// ─── M3a hardening pass 1 — H2 closure: TransientRetry budget cap ──────
//
// W9 freeze §4.0 declares `MAX_BOOT_ATTEMPTS = 5` (the cap
// `transport_trace::attempts_used(doc_id) >= MAX_BOOT_ATTEMPTS →
// escalate to RequiresManualReconciliation`).  Without enforcement
// at dispatch, an infinitely-failing TransientRetry doc would
// re-burn `send_chk` on every boot tick forever.  This fixture
// pins the cap.
//
// **Assertions:**
//   (1) state ER → REQUIRES_MANUAL_RECONCILIATION.
//   (2) `send_chk_count == 0` — zero DPS contact at budget exhaust.
//   (3) `BOOT_ER_BUDGET_EXHAUSTED` audit emitted exactly once with
//       Severity::Error and payload carrying `attempts_used`,
//       `max_boot_attempts`, `retry_class`.
//   (4) histogram counter `error_retryable_budget_exhausted == 1`.
//   (5) `error_retryable_dispatched == 0` — stage_send::run did
//       NOT fire (budget gate intercepted).
//   (6) No `BOOT_ER_ESCALATED_TO_MANUAL` audit — that is the
//       per-class escalation event; budget-exhausted has its own
//       distinct event type for operator-dashboard separation.

#[tokio::test]
async fn fixture_9g_er_transient_retry_budget_exhausted_escalates() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0x9C, "ERROR_RETRYABLE").await;

    // Seed 5 completed transport_trace rows (attempts 1..=5), each
    // tagged TransientRetry.  `attempts_used` returns COALESCE(MAX,
    // 0) = 5; the dispatcher's budget gate (`attempts >= 5`) trips.
    for attempt in 1..=5 {
        seed_completed_transport_trace_at_attempt(
            app.db(),
            doc,
            attempt,
            "RETRYABLE_TRANSPORT",
            Some("TransientRetry"),
        )
        .await;
    }

    let stub =
        dps_panic_on_any_method("H2 budget cap: zero DPS contact on TransientRetry budget exhaust");
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    // (1) State ER → REQUIRES_MANUAL_RECONCILIATION.
    assert_eq!(
        doc_state(app.db(), doc).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "budget exhausted: ER → RequiresManualReconciliation via CAS"
    );

    // (2) Zero DPS contact — the gate fires BEFORE stage_send::run.
    assert_eq!(
        stub.call_count(),
        0,
        "H2: budget-exhausted dispatch must not invoke send_chk"
    );

    // (3) Distinct budget-exhausted audit emitted once.
    assert_eq!(
        audit_count(app.db(), "BOOT_ER_BUDGET_EXHAUSTED").await,
        1,
        "BOOT_ER_BUDGET_EXHAUSTED audit fires exactly once"
    );
    // Severity::Error per design (operator triage needed; not a
    // structural breach — the doc IS retryable in principle, just
    // out of automatic budget).
    let error_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'BOOT_ER_BUDGET_EXHAUSTED' AND severity = 'ERROR'",
    )
    .fetch_one(app.db())
    .await
    .expect("count error budget audits");
    assert_eq!(
        error_count, 1,
        "BOOT_ER_BUDGET_EXHAUSTED must emit Severity::Error"
    );
    // Payload should record attempts_used + max + retry_class for forensics.
    let payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'BOOT_ER_BUDGET_EXHAUSTED'",
    )
    .fetch_one(app.db())
    .await
    .expect("read audit payload");
    assert!(
        payload.contains("\"attempts_used\":5"),
        "audit payload must record attempts_used=5: {payload}"
    );
    assert!(
        payload.contains("\"max_boot_attempts\":5"),
        "audit payload must record max_boot_attempts=5: {payload}"
    );
    assert!(
        payload.contains("\"retry_class\":\"TransientRetry\""),
        "audit payload must record retry_class=TransientRetry: {payload}"
    );

    // (4) Histogram counter.
    assert_eq!(
        summary.docs_advanced.error_retryable_budget_exhausted, 1,
        "histogram: budget-exhausted increments error_retryable_budget_exhausted"
    );

    // (5) stage_send::run path must NOT have fired.
    assert_eq!(
        summary.docs_advanced.error_retryable_dispatched, 0,
        "stage_send::run dispatch path must NOT increment under budget exhaust"
    );

    // (6) NO per-class escalation audit — budget-exhausted has its
    // own distinct event, the per-class arm did not fire.
    assert_eq!(
        audit_count(app.db(), "BOOT_ER_ESCALATED_TO_MANUAL").await,
        0,
        "budget-exhausted must NOT also emit BOOT_ER_ESCALATED_TO_MANUAL (distinct dashboards)"
    );
}

// ─── M3a hardening pass 2 — H1 closure: latest-attempt authoritative ───
//
// W11 fixture #3 proves Pattern B no-resend on first-boot SENDING
// crash.  Hardening pass 2 closes the across-boot duplicate-send
// hazard: when stage_send crashes mid-attempt (SENDING + unfinished
// trace), the next boot's SENDING→ER CAS leaves the unfinished
// trace as-is, and the subsequent ER recovery dispatch must NOT
// fall back to an older completed `TransientRetry` trace.
//
// Pre-hardening shape: `last_attempt_retry_class_for` filtered
// `WHERE completed_at IS NOT NULL`, hiding the unfinished trace
// from the dispatcher's eye.  Stale completed `TransientRetry`
// won; dispatcher routed to `stage_send::run` again → duplicate
// `send_chk` on the same envelope.
//
// Post-hardening: filter removed; latest attempt by `attempt_no`
// dominates.  Unfinished trace has `retry_class = NULL` →
// `from_wire_str` returns `None` → dispatcher routes to the
// indeterminate-hold branch.  Zero DPS contact; doc held in ER.

#[tokio::test]
async fn fixture_9h_er_latest_unfinished_trace_holds_no_send() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0x9A, "ERROR_RETRYABLE").await;

    // attempt 1 — completed, TransientRetry.  Pre-hardening, this
    // would have been read by `last_attempt_retry_class_for` and
    // mis-routed the doc to a duplicate send.
    seed_completed_transport_trace_at_attempt(
        app.db(),
        doc,
        1,
        "RETRYABLE_TRANSPORT",
        Some("TransientRetry"),
    )
    .await;
    // attempt 2 — UNFINISHED (completed_at IS NULL, retry_class IS
    // NULL).  Simulates the crash between stage_send 4-pre
    // (INSERT trace + CAS Signed/ER → Sending) and the wire
    // `send_chk` reply.  Boot's SENDING arm subsequently CAS'd
    // Sending → ER without touching this row.
    sqlx::query(
        "INSERT INTO transport_trace (document_id, attempt_no, backend_profile_id, \
            transport_profile_id, request_envelope_sha256) \
         VALUES (?, 2, 'b1', 't1', ?)",
    )
    .bind(doc)
    .bind(vec![0u8; 32])
    .execute(app.db())
    .await
    .expect("seed unfinished trace row");

    // Sanity — count: 2 rows, latest unfinished.
    assert_eq!(
        count_transport_trace(app.db(), doc).await,
        2,
        "pre-state: 2 trace rows seeded (1 completed TransientRetry + 1 unfinished)"
    );

    let stub = dps_panic_on_any_method(
        "H1/hardening-pass-2: latest unfinished trace must hold, NEVER re-send",
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    // (1) Doc held in ER (indeterminate hold branch).
    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "latest unfinished trace: indeterminate hold; no state change"
    );

    // (2) Zero DPS contact.  This is the load-bearing assertion —
    // the pre-hardening bug would have invoked send_chk via
    // stage_send::run.
    assert_eq!(
        stub.call_count(),
        0,
        "H1 closure: zero send_chk on latest-unfinished hold path"
    );

    // (3) Indeterminate-hold counter increments, NOT the dispatch
    // or escalation counters.
    assert_eq!(
        summary.docs_advanced.error_retryable_indeterminate_deferred, 1,
        "latest-unfinished → indeterminate-hold counter"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_dispatched, 0,
        "latest-unfinished must NOT route to stage_send::run (would be duplicate-send)"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_escalated_to_manual, 0,
        "latest-unfinished is NOT per-class escalation (different signal)"
    );
    assert_eq!(
        summary.docs_advanced.error_retryable_budget_exhausted, 0,
        "latest-unfinished is NOT budget exhaust (different signal)"
    );

    // (4) Forensic audit emitted at Severity::Error.
    assert_eq!(
        audit_count(app.db(), "BOOT_ER_RETRY_CLASS_INDETERMINATE").await,
        1,
        "BOOT_ER_RETRY_CLASS_INDETERMINATE audit fires once"
    );
}

// ─── Fixture 3b — two-boot SENDING-crash after TransientRetry ──────────
//
// End-to-end across two reconcile_pending_with calls.  Combined
// zero `send_chk_count` across BOTH ticks is the load-bearing
// safety contract.

#[tokio::test]
async fn fixture_3b_sending_crash_after_transient_retry_second_boot_no_resend() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let doc = seed_doc_with_signed_xml(app.db(), fn_id, 0x3B, "SENDING").await;

    // Prior completed TransientRetry attempt (attempt 1).
    seed_completed_transport_trace_at_attempt(
        app.db(),
        doc,
        1,
        "RETRYABLE_TRANSPORT",
        Some("TransientRetry"),
    )
    .await;
    // Current unfinished attempt (attempt 2) — this is the in-flight
    // SENDING row left behind by the crashed stage_send::run.
    sqlx::query(
        "INSERT INTO transport_trace (document_id, attempt_no, backend_profile_id, \
            transport_profile_id, request_envelope_sha256) \
         VALUES (?, 2, 'b1', 't1', ?)",
    )
    .bind(doc)
    .bind(vec![0u8; 32])
    .execute(app.db())
    .await
    .expect("seed unfinished trace row");

    let stub = dps_panic_on_any_method(
        "two-boot SENDING/ER: zero send_chk across BOTH ticks (Pattern B + H1 closure)",
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();

    // ── Boot 1: SENDING → ER ────────────────────────────────────────
    let boot1_deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });
    let boot1 = app
        .reconcile_pending_with(boot1_deps)
        .await
        .expect("boot 1 reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "boot 1: SENDING → ErrorRetryable via Pattern B crash-resume"
    );
    assert_eq!(
        boot1.docs_advanced.sending_resumed, 1,
        "boot 1: sending_resumed counter"
    );
    assert_eq!(
        audit_count(app.db(), "BOOT_RESUME_SENDING_TO_ERROR_RETRYABLE").await,
        1,
        "boot 1: SENDING-resume audit"
    );
    // Trace row 2 stays unfinished — boot 1 CAS only touched
    // fiscal_documents.state, not transport_trace.
    assert_eq!(
        count_transport_trace(app.db(), doc).await,
        2,
        "boot 1: trace row count unchanged (resume helper does not touch trace)"
    );

    // ── Boot 2: ER recovery sees unfinished latest trace ────────────
    let boot2_deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });
    let boot2 = app
        .reconcile_pending_with(boot2_deps)
        .await
        .expect("boot 2 reconcile green");

    // (1) Doc stays in ER (indeterminate hold).
    assert_eq!(
        doc_state(app.db(), doc).await,
        "ERROR_RETRYABLE",
        "boot 2: ER + unfinished latest trace → indeterminate hold; NO Pattern B re-drive"
    );

    // (2) Indeterminate-hold counter increments on boot 2.
    assert_eq!(
        boot2.docs_advanced.error_retryable_indeterminate_deferred, 1,
        "boot 2: latest-unfinished routes to indeterminate hold"
    );
    assert_eq!(
        boot2.docs_advanced.error_retryable_dispatched, 0,
        "boot 2: stage_send::run must NOT fire on latest-unfinished"
    );

    // (3) **LOAD-BEARING combined**: zero DPS contact across BOTH
    //     ticks.  Pre-hardening, boot 2 would have triggered a
    //     duplicate send_chk on the same envelope as the crashed
    //     attempt — the exact hazard H1 reports.
    assert_eq!(
        stub.call_count(),
        0,
        "H1 closure: zero send_chk across TWO consecutive boots (SENDING crash + ER recovery)"
    );
}

// ─── HP2-3 / ADR-M3-A10 structural enforcement ─────────────────────────
//
// Two `reconcile_pending_with` calls on the same `App` MUST
// serialise through the recon mutex.  A stub DPS channel records
// peak in-flight concurrency on `last_chk`; under the mutex
// max-in-flight stays at 1.

struct SequenceProbingDpsStub {
    started: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl SequenceProbingDpsStub {
    fn new() -> Self {
        Self {
            started: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
        }
    }
    fn max_concurrency_observed(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }
    fn calls_started(&self) -> usize {
        self.started.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DpsChannel for SequenceProbingDpsStub {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("SequenceProbingDpsStub: send_chk not exercised")
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(cur, Ordering::SeqCst);
        // Pause inside the critical section so an un-serialised
        // parallel caller would visibly increment in_flight > 1.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(CheckAck {
            id: "expected-id-CC".into(),
            id_sign: vec![],
            data_sign: vec![0xCC; 32],
        })
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

#[tokio::test]
async fn concurrent_reconcile_pending_with_same_app_serializes() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    seed_node_state(app.db(), fn_id, "ONLINE", "CLOSED", 1).await;
    let _doc = seed_doc_sent_with_server_fiscal_no(app.db(), fn_id, 0xCC, "expected-id-CC").await;

    let stub = SequenceProbingDpsStub::new();
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let app_b = app.clone();

    // tokio::join! multiplexes two futures on the same task — when
    // future A hits `tokio::time::sleep` inside last_chk, future B
    // is polled.  Absent the recon mutex, B would enter
    // reconcile_pending_inner and re-invoke last_chk on the same
    // SENT doc before A's critical section released.  With the
    // mutex, B awaits the lock and never enters the dispatcher.
    let deps_a = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });
    let deps_b = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let (result_a, result_b) = tokio::join!(
        app.reconcile_pending_with(deps_a),
        app_b.reconcile_pending_with(deps_b),
    );
    let result_a = result_a.expect("recon A green");
    let result_b = result_b.expect("recon B green");

    // Exactly one boot processed the SENT doc (the other reconcile
    // saw it already advanced past pending).
    let total_sent_match =
        result_a.docs_advanced.sent_match_to_kvt1 + result_b.docs_advanced.sent_match_to_kvt1;
    assert_eq!(
        total_sent_match, 1,
        "ADR-M3-A10 serialised: exactly one boot processed the SENT doc"
    );

    // **LOAD-BEARING:** max in-flight last_chk == 1.  Absent the
    // mutex, the 50ms sleep inside last_chk would let both calls
    // overlap and max_in_flight would reach 2.
    assert_eq!(
        stub.max_concurrency_observed(),
        1,
        "HP2-3 / ADR-M3-A10: last_chk MUST NOT overlap across concurrent reconcile_pending_with on the same App"
    );
    assert_eq!(
        stub.calls_started(),
        1,
        "serialised: second boot has nothing pending after first commits"
    );
}

// ─── Fixture 1c — PREPARED replay payload_json byte-equality drift ─────
//
// HP2-4 closure: extends fixture 1b's hash-only drift detection
// with explicit `payload_json` byte-equality.  Seeds fd.payload_json
// and inbox.payload_json with DIFFERENT JSON but the SAME
// `payload_sha256_canonical` and SAME `doc_type` — the hash-only
// check from pass 1 would have missed this drift.  Pass-2 check
// catches it via the byte-equality predicate.

#[tokio::test]
async fn fixture_1c_prepared_replay_payload_json_drift_holds() {
    let (_dir, app) = fresh_app().await;
    let fn_id = "1234567890";
    seed_fn_config(app.db(), fn_id).await;
    let _shift = seed_open_shift_and_node(app.db(), fn_id).await;
    let (doc, req_id) = seed_doc_prepared_full(app.db(), fn_id, 0x1C).await;

    // Seed inbox with DIFFERENT payload_json but the SAME hash and
    // SAME operation_type as the doc.  Pass-1 drift check would
    // pass; pass-2 byte-equality catches the mismatch.
    let inbox_sha = vec![0u8; 32];
    let req_slice: &[u8] = &req_id;
    let mutated_payload_json = r#"{"items":[{"code":"DRIFT","name":"Y","price_kop":1,"quantity_thousandths":1,"sum_kop":1}],"payments":[{"name":"CASH","sum_kop":1,"type_code":"0"}]}"#;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', ?, ?, ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(fn_id)
    .bind(format!("idem-{:02x}", req_id[0]))
    .bind(mutated_payload_json)
    .bind(&inbox_sha)
    .execute(app.db())
    .await
    .unwrap();

    let stub =
        dps_panic_on_any_method("HP2-4: payload_json drift must NOT touch DPS / sign / send");
    let signing_ctx = det_signing_ctx();
    let fn_sign = dummy_fn_sign();
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    let summary = app
        .reconcile_pending_with(deps)
        .await
        .expect("reconcile green");

    assert_eq!(
        doc_state(app.db(), doc).await,
        "PREPARED",
        "payload_json drift: doc stays in PREPARED, no sign/send"
    );
    assert_eq!(
        stub.call_count(),
        0,
        "payload_json drift hold: zero DPS calls"
    );
    assert_eq!(
        summary.docs_advanced.prepared_replay_drift_deferred, 1,
        "HP2-4: payload_json byte mismatch increments drift counter"
    );
    assert_eq!(
        summary.docs_advanced.prepared_dispatched, 0,
        "drift must NOT proceed to stage_sign + stage_send"
    );

    let critical_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'BOOT_PREPARED_REPLAY_DRIFT' AND severity = 'CRITICAL'",
    )
    .fetch_one(app.db())
    .await
    .expect("count critical drift audits");
    assert_eq!(critical_count, 1, "drift emits Severity::Critical");

    let audit_payload: String = sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE event_type = 'BOOT_PREPARED_REPLAY_DRIFT'",
    )
    .fetch_one(app.db())
    .await
    .expect("read drift audit payload");
    assert!(
        audit_payload.contains("\"payload_json_mismatch\":true"),
        "audit payload must surface payload_json_mismatch=true: {audit_payload}"
    );

    assert!(
        read_document_file_kind(app.db(), doc, "SIGNED_XML")
            .await
            .is_none(),
        "stage_sign must NOT run on payload_json drift"
    );
}
