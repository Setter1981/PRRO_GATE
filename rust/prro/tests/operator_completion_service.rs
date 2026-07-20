//! CS-3 gap 4a — the operator-completion SERVICE orchestrator (design §3.4).
//!
//! RED-first teeth for `services::reconciliation::operator_completion::complete_operator_resolution`.
//! It applies the shift-state projection for an ONLINE shift-family document (via the sole
//! `node_state.shift_state` writer `apply_shift_transition`) AND the repository core
//! `complete_operator_pending`, in ONE tx. Covers the four §3.4 online cells + a regular
//! pass-through + the MacReseed-on-shift refusal.
//!
//! - `svc01` SHIFT_OPEN accepted  → shift Opening→Opened, doc SENT, SFN, mode ONLINE;
//! - `svc02` SHIFT_OPEN rejected  → shift Opening→Closed, doc RMR, seed unchanged;
//! - `svc03` Z_REPORT accepted    → shift Closing→Closed, doc SENT;
//! - `svc04` SHIFT_CLOSE rejected → shift Closing→Opened, doc RMR;
//! - `svc05` regular SELL accepted → no shift projection (pass-through), doc SENT;
//! - `svc06` MacReseed on SHIFT_OPEN → refused (`MacReseedOnShiftFamily`), nothing mutated.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation, OperatorResolution};
use prro::db::tx::with_immediate;
use prro::services::reconciliation::operator_completion::{
    complete_operator_resolution, OperatorCompletionError,
};
use sqlx::SqlitePool;

const TS: &str = "2026-07-20T00:00:00Z";
const SHIFT_ID: [u8; 16] = [0x5A; 16];

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool");
    (dir, pool)
}

/// Seed FN + node_state (STOP-eligible) + a shift in `shift_state` + a shift-family/regular doc in
/// SENDING linked to that shift (online origin; `unsigned_xml_sha256` set for the seed advance).
async fn seed(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    doc_type: &str,
    shift_state: &str,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    // node_state mirrors the shift and starts ONLINE (held_pending drives it to STOP_MODE).
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd, \
            current_shift_id) VALUES (?, 'ONLINE', ?, 1, ?)",
    )
    .bind(fscl)
    .bind(shift_state)
    .bind(&SHIFT_ID[..])
    .execute(pool)
    .await
    .unwrap();
    // The shift row (authoritative), matching the node_state mirror.
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, cash_balance_kop, \
            opened_by_cashier_id) VALUES (?, ?, ?, 'ONLINE', 0, 'cashier-1')",
    )
    .bind(&SHIFT_ID[..])
    .bind(fscl)
    .bind(shift_state)
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, ?, ?, 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(&SHIFT_ID[..])
    .bind(doc_byte as i64)
    .bind(doc_type)
    .bind(vec![0u8; 32])
    .bind(&[0x77u8; 32][..])
    .execute(pool)
    .await
    .expect("seed doc");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

fn new_res(res_byte: u8, doc: DocumentId, fscl: &str) -> NewReservation {
    NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fscl.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    }
}

/// Authorize → boot-resume the crash → OUTCOME_OBSERVED + PENDING_APPLY + node STOP_MODE.
async fn held_pending(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) {
    let row = new_res(res_byte, doc, fscl);
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize");
    let fscl_owned = fscl.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::resume_crashed_reservation(tx, [res_byte; 16], &fscl_owned)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("resume");
}

async fn complete(
    pool: &SqlitePool,
    res_byte: u8,
    resolution: OperatorResolution,
) -> Result<(), anyhow::Error> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            complete_operator_resolution(tx, [res_byte; 16], resolution)
                .await
                .map(|_| ())
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

async fn shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(&SHIFT_ID[..])
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn node_shift_state(pool: &SqlitePool, fscl: &str) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(fscl)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn doc_state(pool: &SqlitePool, doc_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn node_mode(pool: &SqlitePool, fscl: &str) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(fscl)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn apply_state(pool: &SqlitePool, res_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT apply_state FROM delivery_reservation WHERE reservation_id = ?")
        .bind(&[res_byte; 16][..])
        .fetch_one(pool)
        .await
        .unwrap()
}

// ─────────────────────── svc01 — SHIFT_OPEN accepted ───────────────────────

#[tokio::test]
async fn svc01_shift_open_accepted_opens_shift_and_issues() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "8000000001";
    let doc = seed(&pool, fscl, 0x11, "SHIFT_OPEN", "OPENING").await;
    held_pending(&pool, 0x01, doc, fscl).await;

    complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000111111".into(),
        },
    )
    .await
    .expect("online SHIFT_OPEN accepted completes");

    assert_eq!(shift_state(&pool).await, "OPENED", "shift Opening→Opened");
    assert_eq!(node_shift_state(&pool, fscl).await, "OPENED", "mirror");
    assert_eq!(doc_state(&pool, 0x11).await, "SENT", "issued doc → Sent");
    assert_eq!(apply_state(&pool, 0x01).await.as_deref(), Some("APPLIED"));
    assert_eq!(node_mode(&pool, fscl).await, "ONLINE");
}

// ─────────────────────── svc02 — SHIFT_OPEN rejected ───────────────────────

#[tokio::test]
async fn svc02_shift_open_rejected_rolls_shift_to_closed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "8000000002";
    let doc = seed(&pool, fscl, 0x22, "SHIFT_OPEN", "OPENING").await;
    held_pending(&pool, 0x02, doc, fscl).await;

    complete(&pool, 0x02, OperatorResolution::NotAccepted)
        .await
        .expect("online SHIFT_OPEN not-accepted completes");

    assert_eq!(
        shift_state(&pool).await,
        "CLOSED",
        "shift Opening→Closed rollback"
    );
    assert_eq!(node_shift_state(&pool, fscl).await, "CLOSED");
    assert_eq!(
        doc_state(&pool, 0x22).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "not-accepted doc → RMR"
    );
    assert_eq!(apply_state(&pool, 0x02).await.as_deref(), Some("APPLIED"));
}

// ─────────────────────── svc03 — Z_REPORT accepted ─────────────────────────

#[tokio::test]
async fn svc03_zreport_accepted_closes_shift_and_issues() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "8000000003";
    let doc = seed(&pool, fscl, 0x33, "Z_REPORT", "CLOSING").await;
    held_pending(&pool, 0x03, doc, fscl).await;

    complete(
        &pool,
        0x03,
        OperatorResolution::Accepted {
            fiscal_number: "4000333333".into(),
        },
    )
    .await
    .expect("online Z_REPORT accepted completes");

    assert_eq!(shift_state(&pool).await, "CLOSED", "shift Closing→Closed");
    assert_eq!(doc_state(&pool, 0x33).await, "SENT");
    assert_eq!(apply_state(&pool, 0x03).await.as_deref(), Some("APPLIED"));
}

// ─────────────────────── svc04 — SHIFT_CLOSE rejected ──────────────────────

#[tokio::test]
async fn svc04_shift_close_rejected_reopens_shift() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "8000000004";
    let doc = seed(&pool, fscl, 0x44, "SHIFT_CLOSE", "CLOSING").await;
    held_pending(&pool, 0x04, doc, fscl).await;

    complete(&pool, 0x04, OperatorResolution::NotAccepted)
        .await
        .expect("online SHIFT_CLOSE not-accepted completes");

    assert_eq!(
        shift_state(&pool).await,
        "OPENED",
        "shift Closing→Opened rollback"
    );
    assert_eq!(
        doc_state(&pool, 0x44).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
}

// ─────────────────────── svc05 — regular pass-through ──────────────────────

#[tokio::test]
async fn svc05_regular_sell_accepted_no_shift_projection() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "8000000005";
    // A regular SELL doc: the orchestrator applies NO shift projection (the shift stays OPENED).
    let doc = seed(&pool, fscl, 0x55, "SELL", "OPENED").await;
    held_pending(&pool, 0x05, doc, fscl).await;

    complete(
        &pool,
        0x05,
        OperatorResolution::Accepted {
            fiscal_number: "4000555555".into(),
        },
    )
    .await
    .expect("regular accepted completes");

    assert_eq!(
        shift_state(&pool).await,
        "OPENED",
        "regular doc → shift untouched"
    );
    assert_eq!(doc_state(&pool, 0x55).await, "SENT");
    assert_eq!(apply_state(&pool, 0x05).await.as_deref(), Some("APPLIED"));
}

// ─────────────────────── svc06 — MacReseed on shift refused ────────────────

#[tokio::test]
async fn svc06_macreseed_on_shift_family_refused() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "8000000006";
    let doc = seed(&pool, fscl, 0x66, "SHIFT_OPEN", "OPENING").await;
    held_pending(&pool, 0x06, doc, fscl).await;

    let err = complete(
        &pool,
        0x06,
        OperatorResolution::MacReseed { seed: [0x99; 32] },
    )
    .await
    .expect_err("MacReseed on a shift-family document has no defined cell");
    assert!(
        err.downcast_ref::<OperatorCompletionError>()
            .is_some_and(|e| matches!(e, OperatorCompletionError::MacReseedOnShiftFamily)),
        "expected MacReseedOnShiftFamily; got {err:?}"
    );
    // Nothing mutated — the shift stays Opening, the reservation stays PENDING.
    assert_eq!(shift_state(&pool).await, "OPENING");
    assert_eq!(
        apply_state(&pool, 0x06).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert_eq!(doc_state(&pool, 0x66).await, "SENDING");
}

// ─────────────── svc07 — sole-caller pin for the operator-only edge 16 ──────

/// Edge 16 (`Opening -> Closed`) is operator-completion-only (shifts §4.1). Its ONLY `src`
/// occurrences are the `allowed_transition` whitelist definition (`shifts.rs`) and the
/// orchestrator's `shift_projection` (`operator_completion.rs`). No write-path stage
/// (`stage_send` / `stage_acquire` / drain) may produce it — a grep-based static pin, mirroring
/// the `delivery_reservation` no-production-caller pin.
#[test]
fn svc07_opening_to_closed_edge_sole_caller_pin() {
    use std::process::Command;
    let src = format!("{}/src", env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("grep")
        .args(["-rn", "(Opening, Closed)", &src])
        .output()
        .expect("grep runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let offenders: Vec<&str> = text
        .lines()
        .filter(|l| {
            let p = l.split(':').next().unwrap_or("");
            !p.ends_with("repositories/shifts.rs")
                && !p.ends_with("reconciliation/operator_completion.rs")
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "edge 16 (Opening -> Closed) produced outside the sanctioned sites \
         (shifts.rs whitelist + operator_completion.rs): {offenders:#?}"
    );
}
