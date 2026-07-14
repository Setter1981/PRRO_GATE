//! A′.3 PR-O3 STOP-O3-1 fix (b) — orphaned pending-drain shift recovery.
//!
//! The wedge (verified by the executed repro that drove the STOP): a
//! post-acquire refusal of an OFFLINE shift-lifecycle doc (CodePoolExhausted /
//! NoActiveSession / a GO_ONLINE race) aborts the DOC (`Aborted`, the #192
//! mechanics) but used to leave the SHIFT orphaned at
//! `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain` — no live anchor doc
//! to drain, so edge 5/13 can never fire, and the guard refuses everything
//! (OLPD: retry SHIFT_OPEN → ShiftAlreadyOpen; CLPD: the §0.5 24h-trap).
//!
//! Fix (b), per the STOP-O3-1 ruling: aborting a shift-class doc escalates the
//! orphaned pending-drain shift to `RequiresManualReconciliation` (the
//! idempotent `escalate_fn_to_manual_recon` seam, edges 6/14) — spec §16.7
//! family 1: the anchor doc is dead, rollback semantics do not apply, RMR is
//! the canonical operator surface.  Runtime half: `inline::terminalise_inbox`.
//! Boot half (#196 pattern): the same compensation re-runs at boot, covering
//! the crash gap between the abort and the escalation.
//!
//! TEETH: reverting the terminalise_inbox escalation (runtime half) REDs
//! cases 1-2; reverting the boot compensator REDs case 3.

mod common;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use prro::app::App;
use prro::config::AppConfig;
use prro::crypto::session::SigningSession;
use prro::db::models::enums::{FiscalMode, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::{DocumentId, OfflineSessionId, RequestId, ShiftId};
use prro::db::repositories::ingress_inbox::{
    self as inbox, InboxInsertOutcome, InboxRow, NewInboxEntry,
};
use prro::db::repositories::{fiscal_number_config as fn_cfg, operators as ops_repo};
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::ingress::inline_binding::production_write_path;
use prro::runtime::ingress::seam::{FiscalOutcome, WritePathEntry};
use prro::services::write_path::stage_sign::SigningContext;
use prro::transports::dps::channel::DpsChannel;
use sqlx::SqlitePool;

use common::{det_signing_ctx_for, StubDpsChannel};

const FN: &str = "4000000037";
const CASHIER: &str = "test-cashier";
const DRIVER: &str = "drv-test";
const SHIFT_OPEN_PAYLOAD: &str = r#"{"opening_sum_kop":0}"#;
const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

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
listen  = "127.0.0.1:8445"
"#
    )
}

async fn boot_app_at(db_path: &Path) -> App {
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    App::boot(cfg).await.unwrap()
}

async fn boot_app(db_name: &str) -> App {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join(db_name);
    std::mem::forget(dir);
    boot_app_at(&db_path).await
}

struct FixtureLoader;
#[async_trait]
impl OperatorKeyLoader for FixtureLoader {
    async fn load(
        &self,
        operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        let mut ctx = det_signing_ctx_for(operator_id);
        ctx.session = SigningSession::new_for_test(
            operator_id.to_string(),
            [7u8; 32],
            FIXTURE_CERT_DER.to_vec(),
        );
        Ok(ctx)
    }
}

fn fn_config() -> fn_cfg::NewFnConfig {
    fn_cfg::NewFnConfig {
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
    }
}

async fn build_registry(app: &App) -> BindingsRegistry {
    fn_cfg::insert(app.db(), &fn_config())
        .await
        .expect("seed FN config");
    ops_repo::insert(
        app.db_secure(),
        &ops_repo::NewOperator {
            operator_id: "OP-1".into(),
            fiscal_number: FN.into(),
            name: "Cashier".into(),
            key_path: "/tmp/k1.dat".into(),
            key_pass_enc: Coding::encode(b"secret1").expect("encode test password"),
        },
    )
    .await
    .expect("seed operator");
    // No DPS legs in these tests — empty queues panic loudly on any wire call.
    let dps: Arc<dyn DpsChannel> = Arc::new(StubDpsChannel::with_queue(vec![]));
    BindingsRegistry::build_from_db(app.db_secure(), app.db(), dps, &FixtureLoader)
        .await
        .expect("build_from_db")
}

async fn seed_boot_baseline(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, NULL, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(NodeMode::Online.as_str())
    .bind(ShiftState::Closed.as_str())
    .execute(pool)
    .await
    .unwrap();
}

fn entry(op: &str, payload: &str, idem: &str, total: Option<i64>) -> NewInboxEntry {
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(payload.as_bytes()).into();
    NewInboxEntry {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: op.into(),
        idempotency_key: idem.into(),
        payload_json: payload.into(),
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: Some(CASHIER.into()),
        driver_id: Some(DRIVER.into()),
        business_ts: Some("2026-07-07T12:00:00Z".into()),
        total_sum_kop: total,
    }
}

async fn drive(
    wp: &dyn WritePathEntry,
    pool: &SqlitePool,
    entry: NewInboxEntry,
) -> Result<FiscalOutcome, prro::runtime::ingress::seam::FiscalError> {
    let row: InboxRow = match inbox::insert(pool, &entry).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await
}

async fn node_row(pool: &SqlitePool) -> (String, String) {
    sqlx::query_as("SELECT mode, shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(FN)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn doc_states(pool: &SqlitePool, doc_type: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT state FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = ? ORDER BY lnd",
    )
    .bind(FN)
    .bind(doc_type)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn audit_count(pool: &SqlitePool, event: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ════════════════════════════════════════════════════════════════════════
// Fix (a) composition (e2e) — the mundane "morning without seeded codes":
// an offline SHIFT_OPEN on an EMPTY pool is refused PRE-MINT through the live
// binding, so the orphan the runtime-half of fix (b) would otherwise have to
// escalate is never manufactured (no doc, no shift transition, no RMR event).
// The runtime half of fix (b) — the terminalise_inbox escalation — is proven
// directly in `inline.rs` unit tests (its only remaining trigger is the
// GO_ONLINE mode-race, which fix (a) cannot pre-empt); the boot half is Case 3.
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn fix_a_empty_pool_offline_shift_open_refused_premint_no_orphan() {
    let app = boot_app("fix_a_empty_pool.db").await;
    let registry = build_registry(&app).await;
    seed_boot_baseline(app.db()).await;
    let write_path = production_write_path(app.clone(), Arc::new(registry));

    // GO_OFFLINE opens the session, but NO codes are seeded.
    prro::admin::go_offline(app.db(), FN, "morning, forgot to seed")
        .await
        .expect("live door: GO_OFFLINE");

    let open = drive(
        &*write_path,
        app.db(),
        entry("SHIFT_OPEN", SHIFT_OPEN_PAYLOAD, "idem-fixa-OPEN", None),
    )
    .await;
    assert!(
        open.is_err(),
        "offline SHIFT_OPEN on an empty pool must be refused pre-mint, got {open:?}"
    );
    // The DOUBLE ABSENCE (fix (a)): no doc was minted, no shift transition
    // committed — so there is no orphan to escalate (fix (b)'s trigger is
    // never manufactured for this mundane case).
    assert_eq!(
        doc_states(app.db(), "SHIFT_OPEN").await,
        Vec::<String>::new(),
        "pre-mint refusal minted no doc"
    );
    let shift_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shifts WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(app.db())
        .await
        .unwrap();
    assert_eq!(shift_rows, 0, "no edge-2 shift row");
    assert_eq!(
        node_row(app.db()).await.1,
        "CLOSED",
        "the shift stays Closed — no orphan, no RMR (retry after seed-codes)"
    );
    // B10 (Finding B): an offline SHIFT_OPEN is now bracketed by the lazy
    // DocType=9 BEGIN, which is ensured BEFORE the SHIFT_OPEN mints.  On an EMPTY
    // pool the `ensure_offline_session_begin` pre-mint guard fail-closes the whole
    // op RETRYABLE (503 `OFFLINE_SESSION_BEGIN_PENDING`) WITHOUT minting either a
    // BEGIN or the SHIFT_OPEN — so the DOUBLE ABSENCE (no doc, no shift) is
    // preserved, but the refusal no longer flows through stage_acquire's
    // per-op-type `OFFLINE_LIFECYCLE_CODE_POOL_EMPTY_REFUSED` audit (that audit is
    // for the SHIFT_OPEN's own acquire, which is never reached).  There is no
    // aborted-BEGIN churn (the guard refuses PRE-mint).
    assert_eq!(
        doc_states(app.db(), "OFFLINE_SESSION_BEGIN").await,
        Vec::<String>::new(),
        "B10: no BEGIN doc minted either — the empty-pool guard refuses pre-mint"
    );
    // No fix-(b) escalation happened — there was nothing to escalate.
    assert_eq!(
        audit_count(app.db(), "INLINE_OFFLINE_LIFECYCLE_ABORT_ESCALATE_MANUAL").await,
        0,
        "no orphan → no escalation"
    );
    prro::db::invariant_scan::assert_clean(app.db()).await;
}

// ════════════════════════════════════════════════════════════════════════
// Case 3 (boot half, #196 pattern) — the crash gap: a shift-lifecycle doc was
// aborted but the process died BEFORE the escalation.  Boot re-runs the
// compensation (the remaining reachable orphan is the GO_ONLINE race, whose
// abort commits before its escalation — this closes that gap).
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn boot_escalates_pre_existing_olpd_orphan_with_terminal_lifecycle_doc() {
    // Seed the post-crash shape directly: OLPD shift + its SHIFT_OPEN doc
    // already ABORTED + node_state pointing at the shift — then boot.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("orphan_boot.db");
    std::mem::forget(dir);
    {
        let pool = prro::db::open_pool(&db_path).await.unwrap();
        sqlx::query(
            "INSERT INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
             VALUES (?, '12345678', 'test')",
        )
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
        let shift_id = ShiftId::new();
        sqlx::query(
            "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
                cash_balance_kop, opened_by_cashier_id) \
             VALUES (?, ?, 1, 'OPENED_LOCAL_PENDING_DRAIN', 'OFFLINE', 0, 'csh')",
        )
        .bind(shift_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO node_state \
             (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
              backend_profile_id, transport_profile_id) \
             VALUES (?, 'OFFLINE', 'OPENED_LOCAL_PENDING_DRAIN', ?, 2, 'b', 't')",
        )
        .bind(FN)
        .bind(shift_id)
        .execute(&pool)
        .await
        .unwrap();
        let session_id = OfflineSessionId::new();
        sqlx::query(
            "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
             VALUES (?, ?, 'OPEN', '2026-07-07T00:00:00Z')",
        )
        .bind(session_id)
        .bind(FN)
        .execute(&pool)
        .await
        .unwrap();
        // The dead anchor: the SHIFT_OPEN doc, ABORTED before the escalation.
        let doc_id = DocumentId::new();
        let req_id = *RequestId::new().as_bytes();
        sqlx::query(
            "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, \
                lnd, doc_type, state, backend_profile_id, transport_profile_id, fs_mode, \
                business_ts, payload_json, payload_sha256_canonical) \
             VALUES (?, ?, ?, ?, 1, 'SHIFT_OPEN', 'ABORTED', 'b', 't', 'OFFLINE', \
                '2026-07-07T08:00:00Z', '{}', ?)",
        )
        .bind(doc_id)
        .bind(&req_id[..])
        .bind(FN)
        .bind(shift_id)
        .bind(&[0u8; 32][..])
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    // Boot over the crashed shape + run boot reconciliation (the sweep-SW-2
    // home; App::boot itself does not reconcile): the compensation must
    // escalate the orphan.
    let app = boot_app_at(&db_path).await;
    app.reconcile_pending()
        .await
        .expect("boot reconciliation over the orphan shape");
    assert_eq!(
        audit_count(
            app.db(),
            "BOOT_ORPHANED_PENDING_DRAIN_SHIFT_ESCALATE_MANUAL"
        )
        .await,
        1,
        "sweep SW-2 emitted its forensic escalation audit"
    );
    assert_eq!(
        shift_state(app.db()).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "boot half of fix (b): the pre-existing OLPD orphan escalates to RMR"
    );
    assert_eq!(
        node_row(app.db()).await.1,
        "REQUIRES_MANUAL_RECONCILIATION",
        "node_state mirrors the boot escalation"
    );
}
