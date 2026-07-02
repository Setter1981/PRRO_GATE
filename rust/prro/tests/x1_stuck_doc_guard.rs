//! X1 (Phase-3 U1) — production runtime guard for the ledger-only pin.
//!
//! The #192/P1 invariant — "no doc rests in a non-terminal state
//! (`PREPARED`/`SIGNED`/`ENCRYPTED`) at a quiescent boundary" — was until now
//! enforced only by the test-gated `invariant_scan` (zero prod callers).  This
//! pins the PRODUCTION detector: after the runtime reconciliation pass
//! (`reconcile_pending_with`, `deps == Some`), any doc still resting in a
//! non-terminal pipeline state on a SETTLED (Online/Offline) FN is counted in
//! `ReconciliationSummary::stuck_non_terminal_docs` and surfaced as a
//! `BOOT_STUCK_NON_TERMINAL_DOC` CRITICAL audit row.
//!
//! The guard is a DETECTOR, not a repairer: it must not abort/advance the doc
//! (that is #192/P1 fix territory) and must not fail the boot (Frozen #9).
//!
//! Fixture shape mirrors `p1_boot_resume_signed_refused_repro.rs`.  The
//! positive case uses the `NoActiveSession` deferral (an OFFLINE FN with an
//! OPENED shift but NO offline session): the post-sign offline-ack refusal is
//! NON-terminal (only `CodePoolExhausted` aborts, per the P1 contract), so the
//! SIGNED doc legitimately survives reconciliation — resting, on a settled FN.
//! Exactly the class the runtime detector exists to surface.

use prro::db::models::ids::DocumentId;
use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};
use prro::transports::dps::dto::CheckSignBlob;
use sqlx::SqlitePool;

mod common;
use common::{ack, det_signing_ctx, StubDpsChannel};

// ── fixture helpers (lifted from p1_boot_resume_signed_refused_repro.rs) ──

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
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) VALUES (?, ?, ?, ?)",
    )
    .bind(fn_id)
    .bind(mode)
    .bind(shift_state)
    .bind(next_lnd)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_open_offline_session(pool: &SqlitePool, fn_id: &str) {
    let session_id_bytes: [u8; 16] = [0xAA; 16];
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-05-16T00:00:00Z')",
    )
    .bind(&session_id_bytes[..])
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_available_offline_code(pool: &SqlitePool, fn_id: &str, code_lnd: i64) {
    sqlx::query("INSERT INTO offline_codes(fiscal_number, code_lnd) VALUES (?, ?)")
        .bind(fn_id)
        .bind(code_lnd)
        .execute(pool)
        .await
        .unwrap();
}

/// Seed a SIGNED SELL doc (the dangling crash-survivor) — same shape as the
/// P1 repro fixture (`unsigned_xml_sha256` set so the boot MAC-chain drift
/// assert doesn't trip; `previous_hash` NULL = genesis).
async fn seed_signed_sell(pool: &SqlitePool, fn_id: &str, doc_byte: u8) -> DocumentId {
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, 1, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', '{}', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(&sha)
    .bind(vec![0xA7u8; 32])
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

fn deps_view<'a>(
    stub: &'a StubDpsChannel,
    signing_ctx: &'a prro::services::write_path::stage_sign::SigningContext,
    fn_sign: &'a CheckSignBlob,
) -> ReconciliationRuntime<'a> {
    ReconciliationRuntime::single_fn(RuntimeView {
        dps: stub,
        signing_ctx,
        fn_sign,
    })
}

fn doc_hex(doc: DocumentId) -> String {
    doc.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// CRITICAL `BOOT_STUCK_NON_TERMINAL_DOC` audit rows for the given doc.
async fn stuck_audit_rows(pool: &SqlitePool, doc: DocumentId) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE event_type = 'BOOT_STUCK_NON_TERMINAL_DOC' \
           AND severity = 'CRITICAL' AND entity_id = ?",
    )
    .bind(doc_hex(doc))
    .fetch_one(pool)
    .await
    .unwrap()
}

fn wire_panic_stub() -> StubDpsChannel {
    StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-BE-CALLED")),
        Box::new(|| panic!("X1 guard path MUST NOT reach the DPS wire")),
    )
}

// ── POSITIVE: a deferred-refusal SIGNED doc rests on a settled OFFLINE FN —
//    the runtime detector MUST count it + emit the CRITICAL audit. ─────────

#[tokio::test]
async fn x1_guard_flags_stuck_signed_on_settled_offline_fn() {
    let (_dir, app, pool) = boot_app_with_db("x1_stuck_flagged.db").await;
    let fn_id = "1234567890";

    // OFFLINE + OPENED shift + NO offline session: the SIGNED doc's post-sign
    // offline-ack refuses with `NoActiveSession` — a NON-terminal refusal that
    // DEFERS (P1 contract: only CodePoolExhausted aborts) → the doc survives
    // reconciliation still SIGNED, on a SETTLED (Offline) FN.
    seed_fn_config(&pool, fn_id).await;
    seed_node_state(&pool, fn_id, "OFFLINE", "OPENED", 1).await;
    let doc = seed_signed_sell(&pool, fn_id, 0x11).await;

    let stub = wire_panic_stub();
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xDE, 0xAD, 0xBE, 0xEF]);

    let summary = app
        .reconcile_pending_with(deps_view(&stub, &signing_ctx, &fn_sign))
        .await
        .expect("reconcile_pending_with must not Err on a deferred refusal");
    eprintln!("[X1 positive] summary = {summary:?}");

    // Precondition of the pin: the doc really RESTS (deferred, not advanced) —
    // this also empirically pins the adjacent-class rest (NoActiveSession).
    assert_eq!(
        doc_state(&pool, doc).await,
        "SIGNED",
        "precondition: NoActiveSession refusal defers — doc rests SIGNED"
    );

    // THE PIN — the runtime detector surfaces the resting non-terminal doc.
    assert_eq!(
        summary.stuck_non_terminal_docs, 1,
        "X1: a SIGNED doc resting on a settled Offline FN after the runtime \
         reconciliation pass MUST be counted by the prod ledger-pin guard"
    );
    assert_eq!(
        stuck_audit_rows(&pool, doc).await,
        1,
        "X1: the stuck doc MUST leave a CRITICAL BOOT_STUCK_NON_TERMINAL_DOC \
         audit row (the durable operator surface)"
    );
}

// ── NEGATIVE (healthy): a doc reconciliation ADVANCES is not stuck. ───────

#[tokio::test]
async fn x1_guard_ignores_doc_recovery_advanced() {
    let (_dir, app, pool) = boot_app_with_db("x1_healthy.db").await;
    let fn_id = "1234567890";

    // Same shape but WITH an OPEN session + one available code: the offline
    // ack SUCCEEDS → SIGNED → OFFLINE_LOCAL_ACK (issued; legitimately rests).
    seed_fn_config(&pool, fn_id).await;
    seed_node_state(&pool, fn_id, "OFFLINE", "OPENED", 1).await;
    seed_open_offline_session(&pool, fn_id).await;
    seed_available_offline_code(&pool, fn_id, 1).await;
    let doc = seed_signed_sell(&pool, fn_id, 0x22).await;

    let stub = wire_panic_stub();
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xDE, 0xAD, 0xBE, 0xEF]);

    let summary = app
        .reconcile_pending_with(deps_view(&stub, &signing_ctx, &fn_sign))
        .await
        .expect("reconcile_pending_with must succeed on the healthy path");
    eprintln!("[X1 healthy] summary = {summary:?}");

    assert_eq!(
        doc_state(&pool, doc).await,
        "OFFLINE_LOCAL_ACK",
        "precondition: recovery advances the doc to the issued offline state"
    );
    assert_eq!(
        summary.stuck_non_terminal_docs, 0,
        "X1 negative: an advanced (issued) doc must NOT be flagged"
    );
    assert_eq!(
        stuck_audit_rows(&pool, doc).await,
        0,
        "X1 negative: no CRITICAL audit for a healthy advanced doc"
    );
}

// ── NEGATIVE (mode-gate): a BLOCKED FN's resting doc is a TRANSIENT-mode
//    rest (operator-recoverable, short-circuited before the per-doc loop) —
//    the guard must NOT flag it (false-positive = alarm fatigue). ──────────

#[tokio::test]
async fn x1_guard_mode_gates_blocked_fn() {
    let (_dir, app, pool) = boot_app_with_db("x1_blocked.db").await;
    let fn_id = "1234567890";

    seed_fn_config(&pool, fn_id).await;
    seed_node_state(&pool, fn_id, "BLOCKED", "OPENED", 1).await;
    let doc = seed_signed_sell(&pool, fn_id, 0x33).await;

    let stub = wire_panic_stub();
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xDE, 0xAD, 0xBE, 0xEF]);

    let summary = app
        .reconcile_pending_with(deps_view(&stub, &signing_ctx, &fn_sign))
        .await
        .expect("reconcile_pending_with must not Err on a BLOCKED FN");
    eprintln!("[X1 blocked] summary = {summary:?}");

    assert_eq!(
        doc_state(&pool, doc).await,
        "SIGNED",
        "precondition: branch_f short-circuits a BLOCKED FN — doc rests"
    );
    assert_eq!(
        summary.stuck_non_terminal_docs, 0,
        "X1 mode-gate: a BLOCKED (transient, operator-recoverable) FN's \
         resting doc must NOT be flagged by the settled-boundary guard"
    );
    assert_eq!(
        stuck_audit_rows(&pool, doc).await,
        0,
        "X1 mode-gate: no CRITICAL audit for a transient-mode rest"
    );
}
