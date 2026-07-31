//! `PRRO_GATE-q5u` (P1) — a DETERMINISTIC tax-config defect must terminate the
//! document, not park it in `PREPARED` forever.
//!
//! The class is the one #192 and its boot-resume twin (#196) closed elsewhere:
//! **no doc rests in a non-terminal state at a quiescent boundary**.  Here the
//! trigger is mundane — one POS tax rate absent from the driver mapping.
//!
//! Pre-fix reality: `derive_check_tax_summaries` fails in stage 3-NO-TX with
//! `SignError::TaxSummary(CalcTaxError::DriverMappingMiss)`; the boot dispatcher
//! erases the type into `anyhow`, emits `BOOT_DISPATCH_ERROR` (Warning) and
//! returns `Ok(())` — so the doc stays `PREPARED` and the NEXT boot tick
//! re-dispatches it identically.  The same input fails on every tick: the loop
//! never terminates, and there is no operator-visible terminal state.
//!
//! The contract is already declared ON THE TYPE (`stage_sign.rs`:
//! `TaxSummary` — *"deterministic config / payload defect — NEVER retry"*), and
//! `(Prepared, Aborted)` is already a legal edge (`fiscal_documents.rs`, the
//! non-issued terminal for pre-issuance refusals).  What was missing is that
//! nothing HONOURED the declared contract.
//!
//! **Why the fix sits at the stage, not at the dispatcher.**  The `q5u` design
//! note claims the inline path shares the hole ("`inline.rs` terminalises only
//! the INBOX row").  That is STALE: `inline::terminalise_inbox` already aborts
//! any dangling {PREPARED,SIGNED} doc for the request — checked, not assumed,
//! and pinned below by the third test (which is GREEN on the state assertion
//! even before this fix).  BOOT is the uncovered caller.  The stage is still the
//! right site: the contract is declared on `SignError` there, one site serves
//! every caller, and the audit names the actual cause instead of the generic
//! `INLINE_REFUSED_DOC_ABORTED`.

use prro::db::invariant_scan;
use prro::db::models::enums::{DocState, DocType, NodeMode, Protocol, ShiftState};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::{
    fiscal_documents as fd, ingress_inbox as inbox, ingress_inbox::NewInboxEntry,
    signing_config_snapshots,
};
use prro::db::types::DbDocumentId;
use prro::services::write_path::stage_sign::{self, SignError};
use prro::services::write_path::tax_summary::{
    DriverNumberMapping, ResolvedTaxGroupBps, TaxResolutionSnapshot,
};
use prro::services::write_path::types::{CanonicalFiscalCommand, WorkerContext};
use prro::transports::dps::dto::CheckSignBlob;
use prro::xml::CalcTaxError;
use sqlx::SqlitePool;

mod common;
use common::{ack, det_signing_ctx, StubDpsChannel};

const FN: &str = "1234567890";
const BUSINESS_TS: &str = "2026-01-01T00:00:00Z";

/// The audit event the abort must leave behind.  Named so an operator can
/// answer "why did this receipt die?" without reading the code.
const ABORT_EVENT: &str = "SIGN_DETERMINISTIC_DEFECT_ABORTED";

/// A SELL item carrying `tax_group_1 = 99`.  The snapshot below maps ONLY
/// driver 5 → canonical 1, so 99 is a MISS → `CalcTaxError::DriverMappingMiss`.
/// That is the live config-drift shape: a POS starts sending a tax rate the
/// gateway's mapping table has never heard of.
const DEFECTIVE_SELL_PAYLOAD: &str = r#"{"items":[{"code":"A1","name":"X","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000,"tax_group_1":99}],"payments":[{"name":"CASH","sum_kop":15000,"type_code":"0"}]}"#;

/// Mapping that knows driver 5 only — every other driver number misses.
fn snapshot_without_driver_99() -> TaxResolutionSnapshot {
    TaxResolutionSnapshot::with_driver_mapping(
        vec![ResolvedTaxGroupBps {
            tx: 1,
            txpr_bps: 2000,
            dtpr_bps: 0,
            txal: 0,
            txty: 0,
        }],
        vec![DriverNumberMapping {
            driver_number: 5,
            canonical_tx_num: 1,
        }],
    )
}

// ─── shared seeds ───────────────────────────────────────────────────────

async fn seed_fn_config(pool: &SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node_state(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'OPENED', 2)",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

/// PREPARED online SELL whose payload carries the unmapped driver number,
/// plus the drift-consistent inbox row the boot chain cross-checks.
/// `snapshot_id` is the persisted FK boot reloads (locked rule #9: recovery
/// signs against the snapshot the doc was pinned with, never current config —
/// which is precisely why the failure is deterministic across ticks).
async fn seed_prepared_defective_sell(
    pool: &SqlitePool,
    doc_byte: u8,
    snapshot_id: Option<i64>,
) -> (DocumentId, [u8; 16]) {
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes: [u8; 16] = [doc_byte ^ 0xFF; 16];
    let payload_sha = [0xA7u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            total_sum_kop, payload_json, payload_sha256_canonical, signing_config_snapshot_id) \
         VALUES (?, ?, ?, 1, 'SELL', 'PREPARED', 'b1', 't1', 'ONLINE', ?, \
            15000, ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes[..])
    .bind(FN)
    .bind(BUSINESS_TS)
    .bind(DEFECTIVE_SELL_PAYLOAD)
    .bind(&payload_sha[..])
    .bind(snapshot_id)
    .execute(pool)
    .await
    .unwrap();
    inbox::insert(
        pool,
        &NewInboxEntry {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: DocType::Sell.as_str().into(),
            idempotency_key: format!("idem-{doc_byte:02x}"),
            payload_json: DEFECTIVE_SELL_PAYLOAD.into(),
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
    (
        DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap()),
        req_bytes,
    )
}

async fn doc_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(DbDocumentId(doc))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn doc_hex(doc: DocumentId) -> String {
    doc.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

async fn audit_count(pool: &SqlitePool, doc: DocumentId, event: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE entity_type = 'fiscal_document' \
         AND entity_id = ? AND event_type = ?",
    )
    .bind(doc_hex(doc))
    .bind(event)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ─── 1. Stage-level pin: the abort happens where the defect is raised ───

#[tokio::test]
async fn deterministic_tax_defect_aborts_the_prepared_doc() {
    let dir = tempfile::tempdir().unwrap();
    let pool = prro::db::open_pool(&dir.path().join("q5u_stage.db"))
        .await
        .unwrap();

    seed_fn_config(&pool).await;
    seed_node_state(&pool).await;
    let snapshot = snapshot_without_driver_99();
    let snapshot_id = signing_config_snapshots::insert_or_get_id(&pool, FN, "drv-test", &snapshot)
        .await
        .unwrap();
    let (doc_id, req_bytes) = seed_prepared_defective_sell(&pool, 0x5A, Some(snapshot_id)).await;

    assert_eq!(
        doc_state(&pool, doc_id).await,
        "PREPARED",
        "precondition: the doc starts PREPARED"
    );

    let ctx = det_signing_ctx();
    let err = stage_sign::run(
        &pool,
        &ctx,
        worker_ctx(doc_id, req_bytes, Some(snapshot), Some(snapshot_id)),
    )
    .await
    .expect_err("an unmapped driver tax number MUST fail the sign");

    // The typed cause survives — the operator forensic surface is the whole
    // point of the fail-loud mapping guard.
    match &err {
        SignError::TaxSummary(CalcTaxError::DriverMappingMiss {
            driver_number,
            field,
        }) => {
            assert_eq!(*driver_number, 99);
            assert_eq!(*field, "tax_group_1");
        }
        other => panic!("expected TaxSummary(DriverMappingMiss), got: {other:?}"),
    }

    // THE PIN.  Pre-fix the doc rests PREPARED (RED); post-fix it is terminal.
    // `Aborted`, not `Rejected`: nothing reached the wire, so this is a
    // pre-issuance refusal — the #192 terminal.
    let after = doc_state(&pool, doc_id).await;
    assert_eq!(
        after, "ABORTED",
        "q5u: a deterministic tax-config defect MUST terminate the doc in \
         Aborted (the declared TaxSummary contract is 'NEVER retry'). Got \
         {after} — pre-fix the doc rests PREPARED and every tick re-signs it."
    );

    assert_eq!(
        audit_count(&pool, doc_id, ABORT_EVENT).await,
        1,
        "q5u: the abort MUST leave exactly one {ABORT_EVENT} audit event naming \
         the deterministic cause — a silent terminal is not operable"
    );

    // A re-drive can no longer re-enter the sign: the state gate refuses.
    let err2 = stage_sign::run(
        &pool,
        &ctx,
        worker_ctx(
            doc_id,
            req_bytes,
            Some(snapshot_without_driver_99()),
            Some(snapshot_id),
        ),
    )
    .await
    .expect_err("a terminal doc MUST NOT re-enter stage_sign");
    assert!(
        matches!(
            err2,
            SignError::StateConflict {
                observed: DocState::Aborted,
                ..
            }
        ),
        "q5u: the second entry MUST hit the PREPARED state gate with \
         observed=Aborted, got: {err2:?}"
    );

    let violations = invariant_scan::scan(&pool).await.unwrap();
    assert!(
        violations.is_empty(),
        "q5u: after the abort the FULL invariant scan MUST be clean. Got: {violations:#?}"
    );
}

// ─── 2. Boot-level pin: the loop is actually broken ─────────────────────

#[tokio::test]
async fn deterministic_tax_defect_does_not_re_dispatch_on_the_next_boot() {
    use prro::config::AppConfig;
    use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("q5u_boot.db");
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

    seed_fn_config(&pool).await;
    seed_node_state(&pool).await;
    let snapshot_id = signing_config_snapshots::insert_or_get_id(
        &pool,
        FN,
        "drv-test",
        &snapshot_without_driver_99(),
    )
    .await
    .unwrap();
    let (doc_id, _req) = seed_prepared_defective_sell(&pool, 0x6B, Some(snapshot_id)).await;

    // The wire must never be touched: the defect is caught before the send.
    let stub = StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-BE-CALLED")),
        Box::new(|| panic!("a deterministic sign defect MUST NOT reach the DPS wire")),
    );
    let signing_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let view = || {
        ReconciliationRuntime::single_fn(RuntimeView {
            dps: &stub,
            signing_ctx: &signing_ctx,
            fn_sign: &fn_sign,
        })
    };

    // ── tick 1: the boot dispatcher meets the defect ──
    app.reconcile_pending_with(view())
        .await
        .expect("boot must not Err on a typed sign defect");

    let after_first = doc_state(&pool, doc_id).await;
    assert_eq!(
        after_first, "ABORTED",
        "q5u: the FIRST boot tick MUST terminate the deterministically \
         defective doc. Got {after_first} — pre-fix it rests PREPARED."
    );

    let abort_events_after_first = audit_count(&pool, doc_id, ABORT_EVENT).await;
    assert_eq!(abort_events_after_first, 1, "one abort, one audit event");

    // ── tick 2: the restart that used to repeat the whole thing ──
    // This is the P1 itself: pre-fix the doc is still PREPARED, so the next
    // boot re-dispatches it, re-signs it, fails identically and audits a
    // SECOND BOOT_DISPATCH_ERROR — forever, at every restart.
    let dispatch_errors_before = audit_count(&pool, doc_id, "BOOT_DISPATCH_ERROR").await;
    // Non-vacuity guard: the count below is only meaningful if the first tick
    // ACTUALLY dispatched this doc into the sign.  If a fixture drift ever made
    // boot skip it, both sides of the comparison would be 0 and the pin would
    // pass while proving nothing.
    assert!(
        dispatch_errors_before >= 1,
        "q5u fixture: the first boot tick must really have dispatched the doc \
         into stage_sign (expected a BOOT_DISPATCH_ERROR from the typed sign \
         failure), else the no-re-dispatch assertion below is vacuous"
    );
    app.reconcile_pending_with(view())
        .await
        .expect("the second boot tick must not Err either");

    assert_eq!(
        doc_state(&pool, doc_id).await,
        "ABORTED",
        "q5u: the doc stays terminal across boots"
    );
    assert_eq!(
        audit_count(&pool, doc_id, ABORT_EVENT).await,
        abort_events_after_first,
        "q5u: the second boot MUST NOT re-abort — a terminal doc is not a \
         dispatch candidate"
    );
    assert_eq!(
        audit_count(&pool, doc_id, "BOOT_DISPATCH_ERROR").await,
        dispatch_errors_before,
        "q5u: the second boot MUST NOT re-dispatch the defective doc — a \
         growing BOOT_DISPATCH_ERROR count IS the never-terminating loop"
    );

    let violations = invariant_scan::scan(&pool).await.unwrap();
    assert!(
        violations.is_empty(),
        "q5u: no StuckNonTerminalDoc (or any other violation) may survive the \
         boot pass. Got: {violations:#?}"
    );
}

// ─── 3. Inline (live REST) pin: the "second site" the design named ─────
//
// The design note expected the live path to leave the document non-terminal.
// It does NOT: `terminalise_inbox` already aborts any dangling {PREPARED,SIGNED}
// doc for the request, so the state assertion below is GREEN pre-fix too — kept
// as a no-regression pin, not as evidence for the fix.  What DOES change here is
// WHICH abort runs: the stage terminalises first with a cause-naming audit, so
// the generic `INLINE_REFUSED_DOC_ABORTED` no longer fires for this class.  Both
// halves are asserted so the observable audit surface cannot drift silently.

#[tokio::test]
async fn deterministic_tax_defect_aborts_the_doc_on_the_inline_path() {
    use prro::runtime::ingress::seam::FiscalError;
    use prro::services::write_path::inline;
    use sha2::{Digest, Sha256};
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap();
    let pool = prro::db::open_pool(&dir.path().join("q5u_inline.db"))
        .await
        .unwrap();
    let pool_secure = prro::db::open_secure_pool(&dir.path().join("q5u_inline_secure.db"))
        .await
        .unwrap();

    seed_fn_config(&pool).await;
    let shift_id = prro::db::models::ids::ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(prro::db::types::DbShiftId(shift_id))
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
            backend_profile_id, transport_profile_id) \
         VALUES (?, 'ONLINE', 'OPENED', ?, 1, 'b', 't')",
    )
    .bind(FN)
    .bind(prro::db::types::DbShiftId(shift_id))
    .execute(&pool)
    .await
    .unwrap();

    // A NEW inbox SELL whose payload references a tax group the (empty) live
    // tax config cannot resolve — the everyday shape of the defect: a POS rate
    // the gateway's mapping has never been told about.
    let req_id = prro::db::models::ids::RequestId::new();
    let request_id: [u8; 16] = *req_id.as_bytes();
    let payload_sha256_canonical: [u8; 32] =
        Sha256::digest(DEFECTIVE_SELL_PAYLOAD.as_bytes()).into();
    inbox::insert(
        &pool,
        &NewInboxEntry {
            request_id,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: DocType::Sell.as_str().into(),
            idempotency_key: "idem-q5u-inline".into(),
            payload_json: DEFECTIVE_SELL_PAYLOAD.into(),
            payload_sha256_canonical,
            correlation_id: None,
            signed_by_cashier_id: Some("test-cashier".into()),
            driver_id: Some("drv-test".into()),
            business_ts: Some(BUSINESS_TS.into()),
            total_sum_kop: Some(15000),
        },
    )
    .await
    .unwrap();
    let row = inbox::InboxRow {
        request_id,
        fiscal_number: FN.into(),
        protocol: Protocol::Rest,
        operation_type: DocType::Sell.as_str().into(),
        idempotency_key: "idem-q5u-inline".into(),
        status: "NEW".into(),
        payload_json: DEFECTIVE_SELL_PAYLOAD.into(),
        payload_sha256_canonical,
        correlation_id: None,
        received_at: BUSINESS_TS.into(),
        signed_by_cashier_id: Some("test-cashier".into()),
        driver_id: Some("drv-test".into()),
        business_ts: Some(BUSINESS_TS.into()),
        total_sum_kop: Some(15000),
    };

    let stub = StubDpsChannel::with_spy(
        Ok(ack("MUST-NOT-BE-CALLED")),
        Box::new(|| panic!("a deterministic sign defect MUST NOT reach the DPS wire")),
    );
    let sign_ctx = det_signing_ctx();
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    let guard = gate.lock_owned().await;

    let err = inline::run(
        &pool,
        &pool_secure,
        &stub,
        &sign_ctx,
        &fn_sign,
        &guard,
        &row,
        prro::services::time_budget::system_gate(),
    )
    .await
    .expect_err("an unresolvable tax group MUST NOT fiscalise");

    // The client surface is unchanged (a gateway-side breach, 500).
    assert!(
        matches!(err, FiscalError::Internal { .. }),
        "q5u: the inline surface stays Internal/500, got {err:?}"
    );

    // No-regression: the DOCUMENT is terminal (true before this fix too, via
    // `terminalise_inbox`; the fix must not undo it).
    let doc_row: Option<(Vec<u8>, String)> =
        sqlx::query_as("SELECT document_id, state FROM fiscal_documents WHERE request_id = ?")
            .bind(&request_id[..])
            .fetch_optional(&pool)
            .await
            .unwrap();
    let (doc_bytes, state) = doc_row.expect(
        "acquire mints the doc before the sign — a missing row means the fixture \
         never reached stage_sign and the pin below would be vacuous",
    );
    assert_eq!(
        state, "ABORTED",
        "q5u: on the LIVE inline path the doc must be terminal — this half was \
         already true via terminalise_inbox and must stay true"
    );

    // THE PIN: WHICH abort ran.  The stage terminalises first and names the
    // deterministic cause; the generic inline arm then finds nothing dangling.
    let doc_id = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    assert_eq!(
        audit_count(&pool, doc_id, ABORT_EVENT).await,
        1,
        "q5u: the inline abort carries the cause-naming audit event too — \
         pre-fix this class died under the generic INLINE_REFUSED_DOC_ABORTED, \
         which tells an operator nothing about WHY"
    );
    assert_eq!(
        audit_count(&pool, doc_id, "INLINE_REFUSED_DOC_ABORTED").await,
        0,
        "q5u: the generic inline doc-abort must NOT also fire — the doc is \
         already terminal when terminalise_inbox runs, so its dangling SELECT \
         finds nothing.  Two abort audits for one death would be a drift."
    );

    let inbox_status: String =
        sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
            .bind(&request_id[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        inbox_status, "REJECTED",
        "q5u: the inbox row keeps its own terminal (unchanged behaviour) — the \
         fix ADDS the document terminal, it does not move the inbox one"
    );

    let violations = invariant_scan::scan(&pool).await.unwrap();
    assert!(
        violations.is_empty(),
        "q5u: the inline abort leaves the ledger scan clean. Got: {violations:#?}"
    );
}

// ─── WorkerContext for the direct stage-level drive ─────────────────────

fn worker_ctx(
    doc_id: DocumentId,
    req_bytes: [u8; 16],
    snapshot: Option<TaxResolutionSnapshot>,
    snapshot_id: Option<i64>,
) -> WorkerContext {
    WorkerContext {
        inbox: inbox::InboxRow {
            request_id: req_bytes,
            fiscal_number: FN.into(),
            protocol: Protocol::Rest,
            operation_type: DocType::Sell.as_str().into(),
            idempotency_key: format!("idem-{:02x}", req_bytes[0] ^ 0xFF),
            status: "PROCESSING".into(),
            payload_json: DEFECTIVE_SELL_PAYLOAD.into(),
            payload_sha256_canonical: [0xA7u8; 32],
            correlation_id: None,
            received_at: BUSINESS_TS.into(),
            signed_by_cashier_id: None,
            driver_id: Some("drv-test".into()),
            business_ts: None,
            total_sum_kop: None,
        },
        command: CanonicalFiscalCommand {
            doc_type: DocType::Sell,
            business_ts: BUSINESS_TS.into(),
            total_sum_kop: Some(15000),
            payload_json: DEFECTIVE_SELL_PAYLOAD.into(),
            payload_sha256_canonical: [0xA7u8; 32],
            source_sha256: [0xA7u8; 32],
            signed_by_cashier_id: None,
            driver_id: None,
        },
        node_state: prro::db::repositories::node_state::NodeStateRow {
            fiscal_number: FN.into(),
            mode: NodeMode::Online,
            shift_state: ShiftState::Opened,
            next_lnd: 2,
            last_known_unsigned_xml_sha256: None,
            current_shift_id: None,
            backend_profile_id: Some("b1".into()),
            transport_profile_id: Some("t1".into()),
            next_z_report_number: 1,
        },
        active_shift: None,
        document: fd::DocumentRow {
            document_id: doc_id,
            fiscal_number: FN.into(),
            lnd: 1,
            state: DocState::Prepared,
            doc_type: DocType::Sell,
            server_fiscal_no: None,
            submission_attempted_at: None,
            backend_profile_id: "b1".into(),
            transport_profile_id: "t1".into(),
            previous_hash: None,
            z_report_number: None,
            unsigned_xml_sha256: None,
            signing_inputs_pinned_at: None,
            signed_by_cashier_id: None,
            signing_config_snapshot_id: snapshot_id,
        },
        tax_resolution_snapshot: snapshot,
        tax_resolution_snapshot_id: snapshot_id,
    }
}
