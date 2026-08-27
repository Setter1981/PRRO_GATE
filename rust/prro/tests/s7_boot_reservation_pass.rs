//! CS-3 S7-1 · A3 — S7-P4-BOOT tooth for the boot-first reservation pass (design §7.1/§7.2/§8;
//! plan `docs/CS3_S7_A3_BOOT_PASS_PLAN.md`).
//!
//! Drives the PRODUCTION boot entrypoint (`App::reconcile_pending`, ctx-free) over a temp-file DB
//! seeded with three reservations a crash could leave mid-lifecycle, and asserts:
//!   - a crashed `CALL_STARTED` reservation becomes `NoResponse{Crashed}` + `PENDING_APPLY` + node
//!     STOP, and its `Sending` doc is LEFT `Sending` (branch-f skips the resume under STOP) —
//!     the **order-swap canary**: if the pass ran AFTER the per-FN loop, the loop would resume the
//!     `Sending` doc to `ErrorRetryable` BEFORE the STOP flip, and this assertion REDs;
//!   - a recorded online `Accepted` reservation is applied EXACTLY once (SFN stamped, seed advanced,
//!     doc `Sent`, reservation `APPLIED`);
//!   - a recorded `MacReseedPending` (-12) HOLD stays `PENDING_APPLY` and does NOT fail the boot
//!     (frozen invariant #9);
//!   - the pass performs ZERO wire I/O — structurally (the ctx-free entrypoint has no `DpsChannel`)
//!     and by a static pin that the pass module references no wire type.
//!
//! **INACTIVE** — this only fires post-cutover; today the queries are empty and the pass is a no-op.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

const TS: &str = "2026-07-21T00:00:00Z";
const SEED: [u8; 32] = [0x77; 32];
const DIG: [u8; 32] = [0x5A; 32];

const FN_CS: &str = "3200000001";
const FN_OK: &str = "3200000002";
const FN_HOLD: &str = "3200000003";

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

async fn seed_fn(pool: &SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CLOSED', 1)",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

/// A `Sending` online doc carrying `unsigned_xml_sha256 = SEED` (so an Accepted apply can advance the
/// seed) and NULL `offline_fiscal_no` (online origin).
async fn seed_sending_doc(pool: &SqlitePool, fn_id: &str, doc_byte: u8) -> DocumentId {
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-21T12:00:00Z', '{}', ?, ?, NULL)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fn_id)
    .bind(doc_byte as i64)
    .bind(vec![0u8; 32])
    .bind(&SEED[..])
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

fn new_res(res_byte: u8, doc: DocumentId, fn_id: &str) -> NewReservation {
    NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fn_id.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    }
}

/// Mint the reservation at `CALL_STARTED` (authorize sets the node active pointer + generation).
async fn authorize(pool: &SqlitePool, row: NewReservation) {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize");
}

/// Move a `CALL_STARTED` reservation to `OUTCOME_OBSERVED + PENDING_APPLY` with matrix-valid evidence
/// (simulates the record transaction the wiring lands at cutover).
#[allow(clippy::too_many_arguments)]
async fn record(
    pool: &SqlitePool,
    res_byte: u8,
    routing: Option<&str>,
    node_effect: &str,
    kind: &str,
    text: Option<&str>,
    digest: Option<&[u8]>,
    rcid: Option<&str>,
) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty='SUBMITTED', \
             response_provenance='PARSED_DPS_ENVELOPE', routing_class=?, apply_state='PENDING_APPLY', \
             node_effect=?, evidence_kind=?, evidence_text=?, evidence_digest=?, \
             remote_correlation_id=? \
         WHERE reservation_id=?",
    )
    .bind(routing)
    .bind(node_effect)
    .bind(kind)
    .bind(text)
    .bind(digest)
    .bind(rcid)
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("record PENDING_APPLY");
}

async fn reservation_apply_state(pool: &SqlitePool, res_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT apply_state FROM delivery_reservation WHERE reservation_id = ?")
        .bind(&[res_byte; 16][..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn reservation_evidence(pool: &SqlitePool, res_byte: u8) -> (Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT evidence_kind, evidence_text FROM delivery_reservation WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn node_mode(pool: &SqlitePool, fn_id: &str) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(fn_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn node_seed(pool: &SqlitePool, fn_id: &str) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(fn_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn doc_state(pool: &SqlitePool, doc_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(&vec![doc_byte; 16][..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn doc_sfn(pool: &SqlitePool, doc_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(&vec![doc_byte; 16][..])
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn s7_p4_boot_pass_resolves_crashed_pending_reservations_wire_free() {
    let (_d, app, pool) = boot_app_with_db("s7boot.db").await;

    // FN_CS — a crash left a CALL_STARTED reservation + its in-flight Sending doc.
    seed_fn(&pool, FN_CS).await;
    let doc_cs = seed_sending_doc(&pool, FN_CS, 0xA1).await;
    authorize(&pool, new_res(0xA0, doc_cs, FN_CS)).await;

    // FN_OK — a recorded online Accepted, not yet applied (auto-release).
    seed_fn(&pool, FN_OK).await;
    let doc_ok = seed_sending_doc(&pool, FN_OK, 0xB1).await;
    authorize(&pool, new_res(0xB0, doc_ok, FN_OK)).await;
    record(
        &pool,
        0xB0,
        None,
        "NoNodeEffect",
        "Accepted",
        Some("5100000001"),
        None,
        Some("5100000001"),
    )
    .await;

    // FN_HOLD — a recorded online -12 (MacReseedPending) HOLD.
    seed_fn(&pool, FN_HOLD).await;
    let doc_hold = seed_sending_doc(&pool, FN_HOLD, 0xC1).await;
    authorize(&pool, new_res(0xC0, doc_hold, FN_HOLD)).await;
    record(
        &pool,
        0xC0,
        Some("MacRecovery"),
        "MacReseedPending",
        "Rejected",
        Some("BadHashPrev"),
        Some(&DIG[..]),
        None,
    )
    .await;

    // The production boot entrypoint (ctx-free → no DpsChannel in scope → wire=0 by construction).
    app.reconcile_pending()
        .await
        .expect("boot must not fail on a held reservation (invariant #9)");

    // FN_CS — crash evidence + STOP.
    assert_eq!(
        reservation_apply_state(&pool, 0xA0).await.as_deref(),
        Some("PENDING_APPLY")
    );
    let (kind, text) = reservation_evidence(&pool, 0xA0).await;
    assert_eq!(kind.as_deref(), Some("NoResponse"));
    assert_eq!(text.as_deref(), Some("CrashedBeforeObservation"));
    assert_eq!(node_mode(&pool, FN_CS).await, "STOP_MODE");
    // ORDER-SWAP CANARY (load-bearing): the pass ran BEFORE the per-FN loop, so the loop early-returns
    // on the STOP node and LEAVES the Sending doc awaiting operator completion. If the pass ran AFTER
    // the loop, the loop would resume Sending→ErrorRetryable first, flipping this assertion.
    assert_eq!(
        doc_state(&pool, 0xA1).await,
        "SENDING",
        "order-swap canary: the crashed doc must rest SENDING (pass-before-loop)"
    );

    // FN_OK — applied exactly once.
    assert_eq!(
        reservation_apply_state(&pool, 0xB0).await.as_deref(),
        Some("APPLIED")
    );
    assert_eq!(doc_state(&pool, 0xB1).await, "SENT");
    assert_eq!(doc_sfn(&pool, 0xB1).await.as_deref(), Some("5100000001"));
    assert_eq!(
        node_seed(&pool, FN_OK).await.as_deref(),
        Some(&SEED[..]),
        "online Accepted advances the seed exactly once"
    );

    // FN_HOLD — held, still PENDING, boot did NOT error.
    assert_eq!(
        reservation_apply_state(&pool, 0xC0).await.as_deref(),
        Some("PENDING_APPLY"),
        "a -12 hold stays PENDING and does not fail boot (inv #9)"
    );
}

async fn audit_count(pool: &SqlitePool, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE event_type = ?")
        .bind(event_type)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Cutover fix A (NC-03 authority restore): a crash loses `node_state` while the ledger + a live
/// reservation survive. Before the fix, reconstruction left `generation=default` + pointer NULL, so
/// BOTH `apply_outcome` and `complete_operator_pending` failed the authority CAS forever (stuck). The
/// fix restores the reservation's authority so the operator path works.
#[tokio::test]
async fn s7_nc03_authority_restore_enables_operator_completion() {
    let (_d, app, pool) = boot_app_with_db("s7nc03.db").await;
    let fscl = "3200000009";
    seed_fn(&pool, fscl).await;
    let doc = seed_sending_doc(&pool, fscl, 0xD1).await;
    authorize(&pool, new_res(0xD0, doc, fscl)).await; // CALL_STARTED; node gen/pointer set

    // NC-03: node_state row is LOST while the ledger + reservation survive.
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = ?")
        .bind(fscl)
        .execute(&pool)
        .await
        .unwrap();

    app.reconcile_pending()
        .await
        .expect("boot must not fail on NC-03 + live reservation");

    // Reconstructed: node STOP (not BLOCKED), reservation normalized to OO+PENDING, restore audited.
    assert_eq!(node_mode(&pool, fscl).await, "STOP_MODE");
    assert_eq!(
        reservation_apply_state(&pool, 0xD0).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(
        audit_count(&pool, "BOOT_LEDGER_WITHOUT_NODE_STATE_RESERVATION_RESTORED").await >= 1,
        "the NC-03 reservation-restore must be audited"
    );

    // The operator can NOW complete it — the authority CAS passes (was StaleAuthority before fix A).
    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::complete_operator_pending(
                tx,
                [0xD0; 16],
                delivery_reservation::OperatorResolution::Accepted {
                    fiscal_number: "5200000001".to_string(),
                },
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("operator completion must SUCCEED after NC-03 authority restore");
    assert!(res.applied, "the operator resolution applied");
    assert_eq!(
        reservation_apply_state(&pool, 0xD0).await.as_deref(),
        Some("APPLIED")
    );
    assert_eq!(doc_state(&pool, 0xD1).await, "SENT");
    assert_eq!(doc_sfn(&pool, 0xD1).await.as_deref(), Some("5200000001"));
}

/// Cutover fix A — the OO+PENDING-deferred NC-03 sub-case (re-audit coverage gap): a reservation
/// already at `OUTCOME_OBSERVED + PENDING_APPLY` when `node_state` is lost is deferred by step 2
/// (`NodeStateMissing`), NOT step 1. Step 3 reconstructs + restores authority; step 4 is a no-op for
/// it (not `CALL_STARTED`), so on the FIRST boot it is NOT auto-applied — it rests PENDING with
/// restored authority for the operator. (A second boot, node_state now present, WOULD auto-apply an
/// auto-release outcome via step 2 — documented in CS3_S7_A3_CUTOVER_FIXES_DESIGN.md; fiscally correct,
/// no fork. This test pins the intended first-boot operator-completion path.)
#[tokio::test]
async fn s7_nc03_authority_restore_oo_pending_deferred_path() {
    let (_d, app, pool) = boot_app_with_db("s7nc03oo.db").await;
    let fscl = "3200000011";
    seed_fn(&pool, fscl).await;
    let doc = seed_sending_doc(&pool, fscl, 0xF1).await;
    authorize(&pool, new_res(0xF0, doc, fscl)).await;
    // Record an online Accepted → the reservation is now OUTCOME_OBSERVED + PENDING_APPLY.
    record(
        &pool,
        0xF0,
        None,
        "NoNodeEffect",
        "Accepted",
        Some("5300000001"),
        None,
        Some("5300000001"),
    )
    .await;
    // NC-03: node_state lost AFTER record, BEFORE apply.
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = ?")
        .bind(fscl)
        .execute(&pool)
        .await
        .unwrap();

    app.reconcile_pending()
        .await
        .expect("boot must not fail on OO+PENDING NC-03");

    // Deferred by step 2, reconstructed + authority-restored in step 3, NOT auto-applied on boot 1.
    assert_eq!(node_mode(&pool, fscl).await, "STOP_MODE");
    assert_eq!(
        reservation_apply_state(&pool, 0xF0).await.as_deref(),
        Some("PENDING_APPLY"),
        "first boot must NOT auto-apply an NC-03-deferred reservation"
    );
    assert!(audit_count(&pool, "BOOT_LEDGER_WITHOUT_NODE_STATE_RESERVATION_RESTORED").await >= 1);
    // The operator resolves it (authority restored ⇒ the CAS passes).
    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::complete_operator_pending(
                tx,
                [0xF0; 16],
                delivery_reservation::OperatorResolution::Accepted {
                    fiscal_number: "5300000001".to_string(),
                },
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("operator completion must succeed on the OO+PENDING NC-03 path");
    assert!(res.applied);
    assert_eq!(doc_state(&pool, 0xF1).await, "SENT");
}

/// Cutover fix B (§7.1): normalize must atomically complete the crashed reservation's in-flight
/// `transport_trace` as SYSTEM_CRASH + a recovery audit — the < 60 s orphan scanner skips fresh
/// traces, so a fast restart would otherwise leave it open.
#[tokio::test]
async fn s7_normalize_completes_crashed_transport_trace() {
    let (_d, app, pool) = boot_app_with_db("s7trace.db").await;
    let fscl = "3200000010";
    seed_fn(&pool, fscl).await;
    let doc = seed_sending_doc(&pool, fscl, 0xE1).await;
    authorize(&pool, new_res(0xE0, doc, fscl)).await; // CALL_STARTED
                                                      // An OPEN trace (outcome_kind NULL) started NOW (< 60 s → the orphan scanner would skip it).
    sqlx::query(
        "INSERT INTO transport_trace(document_id, attempt_no, backend_profile_id, \
            transport_profile_id, request_envelope_sha256, is_probe) \
         VALUES (?, 1, 'b1', 't1', ?, 0)",
    )
    .bind(&vec![0xE1u8; 16][..])
    .bind(&vec![0u8; 32][..])
    .execute(&pool)
    .await
    .expect("seed open trace");

    app.reconcile_pending().await.expect("boot");

    let (outcome, err_kind): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT outcome_kind, error_kind FROM transport_trace WHERE document_id = ? AND attempt_no = 1",
    )
    .bind(&vec![0xE1u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        outcome.as_deref(),
        Some("SYSTEM_CRASH"),
        "the crashed reservation's trace must be completed (not left open)"
    );
    assert_eq!(err_kind.as_deref(), Some("CRASHED_RESERVATION_AT_BOOT"));
    assert!(
        audit_count(&pool, "TRANSPORT_TRACE_CRASHED_RESERVATION_CLOSED").await >= 1,
        "the trace-completion must be audited"
    );
}

/// Static wire-free pin: the boot-first reservation pass has NO access to the wire — it references no
/// `DpsChannel` / `send_chk` type. Together with the ctx-free entrypoint (no channel) this proves the
/// pass cannot send. (The sole-wire seam is separately pinned by S7-P2-2.)
#[test]
fn boot_pass_module_has_no_wire_access() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/services/reconciliation/reservation_boot_pass.rs"
    );
    let text = std::fs::read_to_string(path).expect("read reservation_boot_pass.rs");
    assert!(
        !text.contains("DpsChannel") && !text.contains("send_chk"),
        "the boot-first reservation pass must have no wire access"
    );
}

/// bd `PRRO_GATE-ixf` (retro-1rw survivor, re-mapped on the current tree): the boot pass's
/// BENIGN STALE-DROP lane — a recorded PENDING_APPLY reservation whose generation the node has
/// moved past — had ZERO coverage: the full suite stayed green under the `dropped -= 1` mutant
/// (a u64 underflow panic on the first stale drop).  Production reaches this state via the §7
/// apply-replay frame (a later re-authorize on the same node supersedes the recorded one); the
/// generation bump below is that supersession's minimal seed.
///
/// Teeth: under the mutant this test REDs (the underflow panics inside `reconcile_pending`);
/// the behavioral half pins the drop's contract — NOTHING is mutated: the reservation rests
/// exactly as recorded, the doc keeps SENDING, no sfn stamp, no seed advance.  A skipped
/// generation-CAS would instead APPLY the stale outcome (doc → SENT + sfn + seed advance),
/// reddening all four asserts.
#[tokio::test]
async fn s7_boot_pass_drops_a_stale_generation_reservation_without_mutation() {
    let (_d, app, pool) = boot_app_with_db("s7stale.db").await;
    seed_fn(&pool, FN_CS).await;
    let doc = seed_sending_doc(&pool, FN_CS, 0xD1).await;
    authorize(&pool, new_res(0xD0, doc, FN_CS)).await;
    record(
        &pool,
        0xD0,
        None,
        "NoNodeEffect",
        "Accepted",
        Some("5100000009"),
        None,
        Some("5100000009"),
    )
    .await;

    // Supersede: the node's delivery generation moves past the recorded reservation.
    sqlx::query(
        "UPDATE node_state SET delivery_generation = delivery_generation + 1 \
         WHERE fiscal_number = ?",
    )
    .bind(FN_CS)
    .execute(&pool)
    .await
    .expect("bump generation");
    let seed_before = node_seed(&pool, FN_CS).await;

    app.reconcile_pending()
        .await
        .expect("a stale reservation must DROP benignly, never fail boot (invariant #9)");

    assert_eq!(
        reservation_apply_state(&pool, 0xD0).await.as_deref(),
        Some("PENDING_APPLY"),
        "a stale drop mutates NOTHING — the reservation rests exactly as recorded"
    );
    // The doc is NOT applied from the stale outcome (that would be SENT + sfn + seed): the
    // per-FN boot loop AFTER the pass resumes the crashed SENDING doc down the ORDINARY
    // recovery lane (Sending → ErrorRetryable, zero re-sends) — the stale reservation had no
    // say in it.
    assert_eq!(
        doc_state(&pool, 0xD1).await,
        "ERROR_RETRYABLE",
        "the doc takes the ordinary boot-resume lane; SENT here would mean the stale \
         outcome was applied past the generation CAS"
    );
    assert_eq!(
        doc_sfn(&pool, 0xD1).await,
        None,
        "…no server_fiscal_no stamp from a stale outcome"
    );
    assert_eq!(
        node_seed(&pool, FN_CS).await,
        seed_before,
        "…and the chain seed does not advance on a dropped stale apply"
    );
}
