//! P1-VERIFICATION REPRO — boot-resume twin of fix #192.  NOT a kept
//! regression test (every test here is `#[ignore]`d).  Its sole purpose is
//! to PROVE that the BOOT-RESUME `DocState::Signed` arm in
//! `services::reconciliation::boot_phase` (boot_phase.rs:3741-3751) does NOT
//! abort a dangling SIGNED doc when post-sign dispatch REFUSES — re-opening
//! the exact ledger-only-pin class bug that #192 closed on the LIVE
//! write-path (`inline::run` → `terminalise_inbox`).
//!
//! The fix #192 made the live path flow a post-sign-refused SIGNED doc to the
//! `Aborted` terminal.  The boot path was never given that fix: on
//! `Ok(PostSignRoute::Offline { outcome: Refused(_) })` and
//! `Ok(PostSignRoute::Refused(_))` it ONLY bumps a histogram counter and
//! leaves the doc resting in `SIGNED`.
//!
//! These tests boot the full `App`, seed a crash-recovery-shaped state where
//! a committed `SIGNED` doc coexists with a node that will REFUSE post-sign
//! dispatch, run `App::reconcile_pending_with` (the production boot driver
//! that calls `run_boot_reconciliation`), and assert the doc is STILL
//! `SIGNED` afterwards + that `invariant_scan::scan` flags a
//! `StuckNonTerminalDoc` for it.
//!
//! Two angles are reproduced:
//!   1. PRIMARY / TERMINAL subset — OFFLINE mode + offline code pool
//!      EXHAUSTED → `OfflineAckOutcome::Refused(CodePoolExhausted)`.  This is
//!      the severe case AND the load-bearing proof: the doc reaches the
//!      per-doc dispatcher and traverses the buggy `boot_phase.rs:3745` arm
//!      (verified at runtime: `write_path_dispatch_refused == 1`,
//!      `branch_d_offline_refusal == 0`).  It will NOT self-resolve while the
//!      FN stays offline, so the SIGNED row is durably stuck.
//!   2. FALLBACK / force-mode — node in `BLOCKED` mode.  This also leaves the
//!      doc resting in `SIGNED`, BUT runtime counters show it is short-
//!      circuited by the node-wide `branch_f` mode skip (`branch_f_blocked ==
//!      1`, `write_path_dispatch_refused == 0`) and NEVER reaches the
//!      `3745`/`3750` dispatcher arms.  It therefore demonstrates a SEPARATE
//!      concern — `invariant_scan` over-flagging a *transient-mode* resting
//!      doc — not the P1 abort-asymmetry.  Keep it only as a contrast case;
//!      do not cite it as evidence of the dispatcher-Refused arm.

use prro::db::invariant_scan::{self, Violation};
use prro::db::models::ids::DocumentId;
use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};
use prro::transports::dps::dto::CheckSignBlob;
use sqlx::SqlitePool;

mod common;
use common::{ack, det_signing_ctx, StubDpsChannel};

// ── shared fixture helpers (lifted from app_boot_reconciliation.rs) ──

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

/// Seed a SIGNED SELL doc (the dangling crash-survivor).  Sets
/// `unsigned_xml_sha256` so the boot MAC-chain drift assert doesn't trip
/// (mirrors the #17 happy-path fixture).  `previous_hash` stays NULL
/// (genesis), matching the genesis node seed.
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

/// Build the `deps=Some(..)` runtime view that makes boot reach the per-doc
/// post-sign dispatcher (deps=None DEFERS SIGNED docs and never reaches the
/// buggy arm).  The DPS stub panics on any wire call — proving neither
/// refusal path touches the wire.
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

fn stuck_signed_violation<'a>(
    violations: &'a [Violation],
    doc: DocumentId,
) -> Option<&'a Violation> {
    let want_hex: String = doc.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    violations.iter().find(|v| {
        matches!(
            v,
            Violation::StuckNonTerminalDoc { document_id_hex, state }
                if document_id_hex == &want_hex && state == "SIGNED"
        )
    })
}

// ── PRIMARY / TERMINAL subset: OFFLINE + code pool EXHAUSTED ──────────

#[tokio::test]
#[ignore = "P1-VERIFICATION repro of the #192 boot-resume twin — NOT a kept \
            regression test. Demonstrates the gap; flip to RED pin once the \
            boot arm is fixed."]
async fn p1_boot_resume_offline_exhausted_leaves_stuck_signed() {
    let (_dir, app, pool) = boot_app_with_db("p1_offline_exhausted.db").await;
    let fn_id = "1234567890";

    // Crash-recovery shape: FN OFFLINE + OPENED shift + an OPEN offline
    // session + a committed SIGNED SELL doc — but the offline code pool is
    // EXHAUSTED.  We seed NO `offline_codes` rows at all: `acquire_code_tx`'s
    // `consumed_at IS NULL` selector finds nothing → `Ok(None)` →
    // `CodePoolExhausted` (verified at offline_sessions.rs:408).  An empty
    // pool is exhaustion by construction and avoids the W4 FK on
    // `consumed_by_document_id` that a synthetic "already-consumed" row trips.
    // This is the state a node lands in if it crashed right after committing
    // the SIGNED doc with its last offline code already spent.
    seed_fn_config(&pool, fn_id).await;
    seed_node_state(&pool, fn_id, "OFFLINE", "OPENED", 1).await;

    let session_id_bytes: [u8; 16] = [0xAA; 16];
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-05-16T00:00:00Z')",
    )
    .bind(&session_id_bytes[..])
    .bind(fn_id)
    .execute(&pool)
    .await
    .unwrap();

    let doc = seed_signed_sell(&pool, fn_id, 0x77).await;

    // Sanity: before boot the doc is SIGNED and there are zero AVAILABLE codes.
    assert_eq!(
        doc_state(&pool, doc).await,
        "SIGNED",
        "precondition: doc starts SIGNED"
    );
    let available: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND consumed_at IS NULL",
    )
    .bind(fn_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(available, 0, "precondition: offline code pool is EXHAUSTED");

    let stub = StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-BE-CALLED")),
        Box::new(|| panic!("offline-refused boot path MUST NOT reach the DPS wire")),
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xDE, 0xAD, 0xBE, 0xEF]);

    let summary = app
        .reconcile_pending_with(deps_view(&stub, &signing_ctx, &fn_sign))
        .await
        .expect("reconcile_pending_with should not Err on a typed refusal");
    eprintln!("[P1-REPRO offline-exhausted] reconcile_pending_with -> {summary:?}");

    // ── THE BUG ────────────────────────────────────────────────────────
    let after = doc_state(&pool, doc).await;
    let violations = invariant_scan::scan(&pool).await.unwrap();
    let stuck = stuck_signed_violation(&violations, doc);

    eprintln!("[P1-REPRO offline-exhausted] doc state after boot = {after}");
    eprintln!("[P1-REPRO offline-exhausted] invariant_scan violations = {violations:#?}");

    // The #192 fix on the LIVE path would have ABORTED this doc. The boot
    // path leaves it resting in SIGNED — a ledger-only-pin violation.
    assert_eq!(
        after, "SIGNED",
        "REPRO: boot-resume leaves the post-sign-refused doc stuck in SIGNED \
         (the live path would Abort it). If this is now ABORTED, the boot arm \
         has been fixed and this repro should become a RED pin / be deleted."
    );
    assert!(
        stuck.is_some(),
        "REPRO: invariant_scan MUST flag StuckNonTerminalDoc(SIGNED) for the \
         stranded doc. Got violations: {violations:#?}"
    );
}

// ── FALLBACK / force-mode: BLOCKED → PostSignRoute::Refused ───────────

#[tokio::test]
#[ignore = "P1-VERIFICATION repro (fallback force-mode) of the #192 boot-resume \
            twin — NOT a kept regression test."]
async fn p1_boot_resume_blocked_mode_leaves_stuck_signed() {
    let (_dir, app, pool) = boot_app_with_db("p1_blocked.db").await;
    let fn_id = "1234567890";

    // Force-mode angle: node in BLOCKED.  VERIFIED runtime behaviour: boot's
    // node-wide `branch_f` mode skip catches the BLOCKED FN FIRST
    // (`branch_f_blocked == 1`) and the SIGNED doc is NEVER routed to the
    // per-doc `dispatch_post_sign` (`write_path_dispatch_refused == 0`).  The
    // doc still rests in SIGNED, but for a DIFFERENT reason than the P1
    // dispatcher-Refused arms — this is a transient-mode deferral that
    // `invariant_scan` nonetheless over-flags.  Contrast case only.
    seed_fn_config(&pool, fn_id).await;
    seed_node_state(&pool, fn_id, "BLOCKED", "OPENED", 1).await;
    let doc = seed_signed_sell(&pool, fn_id, 0x55).await;

    assert_eq!(
        doc_state(&pool, doc).await,
        "SIGNED",
        "precondition: doc starts SIGNED"
    );

    let stub = StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-BE-CALLED")),
        Box::new(|| panic!("blocked-refused boot path MUST NOT reach the DPS wire")),
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xCA, 0xFE, 0xBA, 0xBE]);

    let outcome = app
        .reconcile_pending_with(deps_view(&stub, &signing_ctx, &fn_sign))
        .await;

    let after = doc_state(&pool, doc).await;
    let violations = invariant_scan::scan(&pool).await.unwrap();
    let stuck = stuck_signed_violation(&violations, doc);

    eprintln!("[P1-REPRO blocked] reconcile_pending_with -> {outcome:?}");
    eprintln!("[P1-REPRO blocked] doc state after boot = {after}");
    eprintln!("[P1-REPRO blocked] invariant_scan violations = {violations:#?}");

    assert_eq!(
        after, "SIGNED",
        "REPRO (fallback): BLOCKED-refused boot-resume leaves the doc stuck in SIGNED"
    );
    assert!(
        stuck.is_some(),
        "REPRO (fallback): invariant_scan MUST flag StuckNonTerminalDoc(SIGNED). \
         Got violations: {violations:#?}"
    );
}
