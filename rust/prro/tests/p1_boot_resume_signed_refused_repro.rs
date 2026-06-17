//! P1 BOOT-RESUME — twin of fix #192.  Closes the boot-side of the
//! ledger-only-pin class bug that #192 closed on the LIVE write-path
//! (`inline::run` → `terminalise_inbox`): a post-sign refusal with the
//! TERMINAL `CodePoolExhausted` cause must abort a dangling
//! `{PREPARED,SIGNED}` doc to `Aborted`, not leave it resting non-terminal
//! to be wrongly resurrected (→ issue a refused check) later.
//!
//! The two `offline_exhausted_*` tests are KEPT RED→GREEN regression pins
//! (NOT `#[ignore]`d): pre-fix the boot arms ONLY bump
//! `histogram.write_path_dispatch_refused` and leave the doc in `SIGNED`
//! (RED — `after == "ABORTED"` fails); post-fix they Abort
//! (GREEN).  They boot the full `App`, seed a crash-recovery-shaped state,
//! run `App::reconcile_pending_with` (the production boot driver that calls
//! `run_boot_reconciliation`), and assert the doc is `ABORTED` + that
//! `invariant_scan::scan` flags NO `StuckNonTerminalDoc` for it.
//!
//! Scope (locked ШАГ 0, 2026-06-17): the fix touches EXACTLY the two
//! `OfflineAckOutcome::Refused` arms under `CodePoolExhausted`:
//!
//! - SIGNED-resume `dispatch_pending_doc` `DocState::Signed` (arc 3745).
//! - PREPARED-resume `dispatch_prepared_via_chain` (arc 3514) — a distinct
//!   code path: stage_sign drives PREPARED→SIGNED, then post-sign refusal.
//!
//! The `PostSignRoute::Refused` arcs (3522/3750) are NOT touched: node-wide
//! Blocked/StopMode/CryptoDegraded are short-circuited by `branch_f`, and
//! GoingOnline by `branch_d`, BEFORE the per-doc loop; the only mid-pass-
//! reachable cause there (NodeBlocked) is operator-recoverable → defer.
//!
//! Both pinned angles use OFFLINE mode + an EXHAUSTED offline code pool →
//! `OfflineAckOutcome::Refused(CodePoolExhausted)`, the proven-reachable
//! terminal subset (it will NOT self-resolve while the FN stays offline).
//!
//! The third test (`blocked_mode`) stays `#[ignore]`d as a CONTRAST: a node
//! in `BLOCKED` mode is short-circuited by the node-wide `branch_f` mode skip
//! (`branch_f_blocked == 1`, `write_path_dispatch_refused == 0`) and NEVER
//! reaches the `3745`/`3750` dispatcher arms.  Post-fix the BLOCKED doc
//! correctly STAYS `SIGNED` (transient-mode defer — abort would lose a
//! legitimate in-flight doc); `invariant_scan` over-flagging it is a SEPARATE
//! concern (O-findings).  Do not cite it as evidence of the dispatcher arms.

use prro::db::invariant_scan::{self, Violation};
use prro::db::models::enums::{DocType, Protocol};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::{ingress_inbox as inbox, ingress_inbox::NewInboxEntry};
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

async fn seed_node_state(pool: &SqlitePool, fn_id: &str, mode: &str, shift_state: &str, next_lnd: i64) {
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

/// Valid canonical SELL payload (lifted from `write_path_stage3_sign.rs`)
/// so `stage_sign::run` can build canonical XML + sign the PREPARED doc.
const SELL_PAYLOAD_JSON: &str = r#"{"items":[{"code":"A1","name":"X","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"CASH","sum_kop":15000,"type_code":"0"}]}"#;

/// Seed a PREPARED SELL doc + a drift-consistent `ingress_inbox` row so the
/// boot PREPARED-resume chain (`dispatch_prepared_via_chain`) passes the
/// fd↔inbox drift cross-check, signs it (PREPARED→SIGNED via `stage_sign` on a
/// valid canonical SELL payload), then reaches the post-sign offline-ack arm
/// (arc 3514).  `payload_sha` is identical on both rows — the drift check is a
/// cross-row equality, not a recompute (see boot_phase drift cross-check note).
/// Absent inbox row would fail the chain's `fetch_one` drift read instead.
async fn seed_prepared_sell(pool: &SqlitePool, fn_id: &str, doc_byte: u8) -> DocumentId {
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes: [u8; 16] = [doc_byte ^ 0xFF; 16];
    let payload_sha = [0xA7u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            total_sum_kop, payload_json, payload_sha256_canonical) \
         VALUES (?, ?, ?, 1, 'SELL', 'PREPARED', 'b1', 't1', 'ONLINE', \
            '2026-01-01T00:00:00Z', 15000, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes[..])
    .bind(fn_id)
    .bind(SELL_PAYLOAD_JSON)
    .bind(&payload_sha[..])
    .execute(pool)
    .await
    .unwrap();
    // Drift-consistent inbox row (same FN / payload_json / payload_sha /
    // operation_type) — the chain reads it via fetch_one + cross-checks.
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: req_bytes,
            fiscal_number: fn_id.into(),
            protocol: Protocol::Rest,
            operation_type: DocType::Sell.as_str().into(),
            idempotency_key: format!("idem-{doc_byte:02x}"),
            payload_json: SELL_PAYLOAD_JSON.into(),
            payload_sha256_canonical: payload_sha,
            correlation_id: None,
            signed_by_cashier_id: None,
            driver_id: Some("drv-test".into()),
            business_ts: None,
            total_sum_kop: None,
        },
    )
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

fn stuck_signed_violation(violations: &[Violation], doc: DocumentId) -> Option<&Violation> {
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
async fn p1_boot_resume_offline_exhausted_signed_aborts() {
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
    assert_eq!(doc_state(&pool, doc).await, "SIGNED", "precondition: doc starts SIGNED");
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
    eprintln!("[P1-REPRO offline-exhausted] doc state after boot = {after}");
    eprintln!("[P1-REPRO offline-exhausted] invariant_scan violations = {violations:#?}");

    // PIN (mirror of #192 on the boot path): a post-sign refusal with the
    // terminal CodePoolExhausted cause MUST Abort the dangling SIGNED doc.
    // Pre-fix the boot arm (arc 3745) only bumps a histogram → doc stays
    // SIGNED (RED).  Post-fix → ABORTED + no StuckNonTerminalDoc.
    assert_eq!(
        after, "ABORTED",
        "P1: boot-resume MUST Abort the post-sign-refused SIGNED doc \
         (CodePoolExhausted is a terminal refusal — mirrors #192 on the live \
         path). Got {after} (pre-fix the arc leaves it SIGNED)."
    );
    // FULL scan clean (not just "no StuckNonTerminalDoc"): bakes the MAC-walk
    // chain-fence into the pin — if the SIGNED→Aborted abort accidentally
    // produced a ChainBreak/ChainSeedMismatch (refused doc still in the
    // chain-walk), this catches it.  Aborted carries offline_fiscal_no=NULL so
    // it is excluded from the issued-chain + cohort; the empty OPEN session is
    // legal.
    assert!(
        violations.is_empty(),
        "P1: after Abort the FULL invariant_scan MUST be clean (zero \
         violations). Got: {violations:#?}"
    );
}

// ── PREPARED-resume (arc 3514): PREPARED→SIGNED via stage_sign, THEN
//    offline-refused on an EXHAUSTED pool.  Distinct code path from the
//    SIGNED-resume arm (3745) — pinned independently. ───────────────────

#[tokio::test]
async fn p1_boot_resume_offline_exhausted_prepared_aborts() {
    let (_dir, app, pool) = boot_app_with_db("p1_offline_exhausted_prepared.db").await;
    let fn_id = "1234567890";

    // Same crash-recovery shape as the SIGNED case, but the survivor is a
    // PREPARED doc: boot's `dispatch_prepared_via_chain` re-signs it
    // (PREPARED→SIGNED via stage_sign on a valid canonical SELL payload),
    // then post-sign dispatch routes OFFLINE → stage_offline_ack → the
    // EXHAUSTED pool yields Refused(CodePoolExhausted) at arc 3514.
    seed_fn_config(&pool, fn_id).await;
    seed_node_state(&pool, fn_id, "OFFLINE", "OPENED", 1).await;

    let session_id_bytes: [u8; 16] = [0xBB; 16];
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-05-16T00:00:00Z')",
    )
    .bind(&session_id_bytes[..])
    .bind(fn_id)
    .execute(&pool)
    .await
    .unwrap();

    let doc = seed_prepared_sell(&pool, fn_id, 0x33).await;

    assert_eq!(
        doc_state(&pool, doc).await,
        "PREPARED",
        "precondition: doc starts PREPARED"
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
    eprintln!("[P1 prepared-exhausted] reconcile_pending_with -> {summary:?}");

    let after = doc_state(&pool, doc).await;
    let violations = invariant_scan::scan(&pool).await.unwrap();
    eprintln!("[P1 prepared-exhausted] doc state after boot = {after}");
    eprintln!("[P1 prepared-exhausted] invariant_scan violations = {violations:#?}");

    // PIN: the PREPARED-resume chain signs the doc, then hits the terminal
    // offline refusal (arc 3514) — it MUST Abort, not leave a SIGNED orphan.
    // Pre-fix → SIGNED (RED); post-fix → ABORTED.  (A `PREPARED` result here
    // would mean the chain never reached the arc — fix the fixture, not the pin.)
    assert_eq!(
        after, "ABORTED",
        "P1: boot PREPARED-resume MUST Abort the re-signed-then-refused doc \
         at arc 3514. Got {after} (pre-fix the arc leaves it SIGNED; \
         PREPARED would mean drift/sign failed before the arc)."
    );
    // FULL scan clean — same chain-fence rationale as the SIGNED pin, plus it
    // confirms the seeded `ingress_inbox` row + aborted doc raise no
    // RejectedInboxWithAcceptedDoc / chain violation.
    assert!(
        violations.is_empty(),
        "P1: after Abort the FULL invariant_scan MUST be clean (zero \
         violations). Got: {violations:#?}"
    );
}

// ── CONTRAST (kept `#[ignore]`d): BLOCKED → branch_f short-circuit ─────

#[tokio::test]
#[ignore = "CONTRAST, NOT a P1 pin: BLOCKED is short-circuited by branch_f \
            BEFORE the per-doc dispatcher (never reaches arc 3745/3750). The \
            doc correctly STAYS SIGNED (transient-mode defer); invariant_scan \
            over-flagging it is a separate O-finding concern."]
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

    assert_eq!(doc_state(&pool, doc).await, "SIGNED", "precondition: doc starts SIGNED");

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
