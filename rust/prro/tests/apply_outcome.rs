//! CS-3 Slice 4 — origin-sensitive repeatable apply.
//!
//! RED-first teeth for `delivery_reservation::apply_outcome` (design §3.2 + §4.3). A recorded
//! PENDING_APPLY outcome is auto-released only for the online/offline Accepted, online reject,
//! and safe-NotSubmitted rows; everything else HOLDs. Apply is repeatable + idempotent and the
//! generation CAS drops stale rows without mutating seed / SFN / fence. INACTIVE — shadows the
//! live stage_send 4-b (wired at Slice 7).
//!
//! - `ap01` online Accepted: SFN stamped + seed advanced once + APPLIED + pointer cleared;
//! - `ap02` offline Accepted: SFN stamped + ZERO seed write + APPLIED;
//! - `ap03` online reject (-11): no SFN / no seed, node BLOCKED, APPLIED;
//! - `ap04` offline reject → HOLD (`HeldNotAutoRelease`), nothing mutated;
//! - `ap05` SubmittedUnknown (NoResponse) → HOLD;
//! - `ap06` replay is idempotent (2nd apply is a no-op);
//! - `ap07` a stale generation drops without mutating seed / SFN / pointer.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    self, apply_outcome, list_call_started_without_outcome, resume_crashed_reservation, ApplyError,
    ApplyResult, Authorization, NewReservation,
};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

const TS: &str = "2026-07-20T00:00:00Z";
const SEED: [u8; 32] = [0x77; 32];
const DIG: [u8; 32] = [0x5A; 32];

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// Seed FN config + node_state + a fiscal document with the given origin + seed hash.
async fn seed_doc(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    offline_no: Option<i64>,
) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CREATED', 1)",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(doc_byte as i64) // unique lnd per doc so multiple docs share an FN
    .bind(vec![0u8; 32])
    .bind(&SEED[..])
    .bind(offline_no)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
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

async fn authorize(pool: &SqlitePool, row: NewReservation) -> Authorization {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize")
}

/// Record an OUTCOME_OBSERVED + PENDING_APPLY row with matrix-valid evidence (simulates the
/// record transaction — the production record_outcome path lands with the wiring).
#[allow(clippy::too_many_arguments)]
async fn record(
    pool: &SqlitePool,
    res_byte: u8,
    cert: &str,
    prov: &str,
    routing: Option<&str>,
    node_effect: &str,
    kind: &str,
    text: Option<&str>,
    digest: Option<&[u8]>,
    rcid: Option<&str>,
) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty=?, response_provenance=?, \
             routing_class=?, apply_state='PENDING_APPLY', node_effect=?, \
             evidence_kind=?, evidence_text=?, evidence_digest=?, remote_correlation_id=? \
         WHERE reservation_id=?",
    )
    .bind(cert)
    .bind(prov)
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

async fn record_accepted(pool: &SqlitePool, res_byte: u8, f: &str) {
    record(
        pool,
        res_byte,
        "SUBMITTED",
        "PARSED_DPS_ENVELOPE",
        None,
        "NoNodeEffect",
        "Accepted",
        Some(f),
        None,
        Some(f),
    )
    .await;
}

async fn apply(pool: &SqlitePool, res_byte: u8) -> Result<ApplyResult, anyhow::Error> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            apply_outcome(tx, [res_byte; 16])
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
}

async fn read_sfn(pool: &SqlitePool, doc_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id=?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_seed(pool: &SqlitePool, fscl: &str) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number=?",
    )
    .bind(fscl)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn read_apply_state(pool: &SqlitePool, res_byte: u8) -> Option<String> {
    sqlx::query_scalar("SELECT apply_state FROM delivery_reservation WHERE reservation_id=?")
        .bind(&[res_byte; 16][..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_pointer(pool: &SqlitePool, fscl: &str) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT active_delivery_reservation_id FROM node_state WHERE fiscal_number=?",
    )
    .bind(fscl)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn read_mode(pool: &SqlitePool, fscl: &str) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number=?")
        .bind(fscl)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn read_doc_state(pool: &SqlitePool, doc_byte: u8) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id=?")
        .bind(vec![doc_byte; 16])
        .fetch_one(pool)
        .await
        .unwrap()
}

fn is_apply_err(err: &anyhow::Error, pred: impl Fn(&ApplyError) -> bool) -> bool {
    err.downcast_ref::<ApplyError>().is_some_and(pred)
}

// ───────────────────────────── ap01 — online Accepted ────────────────────────

#[tokio::test]
async fn ap01_online_accepted_stamps_sfn_advances_seed_and_releases() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000001";
    let doc = seed_doc(&pool, fscl, 0x11, None).await; // online origin
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record_accepted(&pool, 0x01, "4000111111").await;

    let r = apply(&pool, 0x01).await.expect("online Accepted applies");
    assert!(r.applied && r.seed_advanced);
    assert_eq!(r.server_fiscal_no.as_deref(), Some("4000111111"));
    assert_eq!(read_sfn(&pool, 0x11).await.as_deref(), Some("4000111111"));
    assert_eq!(
        read_seed(&pool, fscl).await.as_deref(),
        Some(&SEED[..]),
        "online seed advanced"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
    assert!(
        read_pointer(&pool, fscl).await.is_none(),
        "active pointer cleared on release"
    );
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "SENT",
        "the issued document left Sending for Sent"
    );
}

// ───────────────────────────── ap02 — offline Accepted ───────────────────────

#[tokio::test]
async fn ap02_offline_accepted_stamps_sfn_but_no_seed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000002";
    let doc = seed_doc(&pool, fscl, 0x11, Some(500)).await; // offline origin
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record_accepted(&pool, 0x01, "4000222222").await;

    let r = apply(&pool, 0x01).await.expect("offline Accepted applies");
    assert!(
        r.applied && !r.seed_advanced,
        "offline origin performs ZERO seed writes"
    );
    assert_eq!(read_sfn(&pool, 0x11).await.as_deref(), Some("4000222222"));
    assert!(
        read_seed(&pool, fscl).await.is_none(),
        "offline seed must NOT advance"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
}

// ───────────────────────────── ap03 — online reject (-11) ────────────────────

#[tokio::test]
async fn ap03_online_reject_offline168_blocks_node_and_releases() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000003";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    // Rejected / Offline168 (online -11): routing TerminalReject, node_effect NodeBlocked.
    record(
        &pool,
        0x01,
        "SUBMITTED",
        "PARSED_DPS_ENVELOPE",
        Some("TerminalReject"),
        "NodeBlocked",
        "Rejected",
        Some("Offline168"),
        Some(&DIG[..]),
        None,
    )
    .await;

    let r = apply(&pool, 0x01).await.expect("online reject applies");
    assert!(r.applied && !r.seed_advanced && r.server_fiscal_no.is_none());
    assert!(
        read_sfn(&pool, 0x11).await.is_none(),
        "reject stamps no SFN"
    );
    assert!(
        read_seed(&pool, fscl).await.is_none(),
        "reject advances no seed"
    );
    assert_eq!(
        read_mode(&pool, fscl).await,
        "BLOCKED",
        "online -11 flips node BLOCKED"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "REJECTED",
        "a definitive pre-SENT reject leaves Sending for Rejected"
    );
}

// ────────────────── ap08 / ap09 — online -12 / -6 HOLD (S7-0 gap 5) ───────────

/// Online `-12` (BadHashPrev / MacReseedPending) is a MAC-recovery hold — it must NOT auto-release,
/// even though it is an online-origin `Rejected`.
#[tokio::test]
async fn ap08_online_mac_reseed_pending_holds() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000008";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record(
        &pool,
        0x01,
        "SUBMITTED",
        "PARSED_DPS_ENVELOPE",
        Some("MacRecovery"),
        "MacReseedPending",
        "Rejected",
        Some("BadHashPrev"),
        Some(&DIG[..]),
        None,
    )
    .await;
    let err = apply(&pool, 0x01)
        .await
        .expect_err("online -12 must HOLD for MAC recovery");
    assert!(is_apply_err(&err, |e| matches!(
        e,
        ApplyError::HeldNotAutoRelease
    )));
    // Nothing released — still PENDING, pointer held, doc still Sending.
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_pointer(&pool, fscl).await.is_some());
    assert_eq!(read_doc_state(&pool, 0x11).await, "SENDING");
}

/// Online `-6` (NotPrevZReport / OperatorEscalation) is an operator hold — it must NOT auto-release.
#[tokio::test]
async fn ap09_online_operator_escalation_holds() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000009";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record(
        &pool,
        0x01,
        "SUBMITTED",
        "PARSED_DPS_ENVELOPE",
        Some("OperatorEscalation"),
        "OperatorEscalation",
        "Rejected",
        Some("NotPrevZReport"),
        Some(&DIG[..]),
        None,
    )
    .await;
    let err = apply(&pool, 0x01)
        .await
        .expect_err("online -6 must HOLD for the operator");
    assert!(is_apply_err(&err, |e| matches!(
        e,
        ApplyError::HeldNotAutoRelease
    )));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_pointer(&pool, fscl).await.is_some());
}

// ───────────────────────── ap04 — offline reject HOLDs ────────────────────────

#[tokio::test]
async fn ap04_offline_reject_holds_and_mutates_nothing() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000004";
    let doc = seed_doc(&pool, fscl, 0x11, Some(500)).await; // offline origin
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record(
        &pool,
        0x01,
        "SUBMITTED",
        "PARSED_DPS_ENVELOPE",
        Some("TerminalReject"),
        "NoNodeEffect",
        "Rejected",
        Some("Verify"),
        Some(&DIG[..]),
        None,
    )
    .await;

    let err = apply(&pool, 0x01)
        .await
        .expect_err("offline reject is a HOLD");
    assert!(is_apply_err(&err, |e| matches!(
        e,
        ApplyError::HeldNotAutoRelease
    )));
    // Nothing mutated: still PENDING, pointer still set, no SFN.
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_pointer(&pool, fscl).await.is_some());
    assert!(read_sfn(&pool, 0x11).await.is_none());
}

// ───────────────────── ap05 — SubmittedUnknown HOLDs ──────────────────────────

#[tokio::test]
async fn ap05_submitted_unknown_holds() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000005";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record(
        &pool,
        0x01,
        "SUBMITTED_UNKNOWN",
        "NO_RESPONSE",
        Some("TransientRetry"),
        "NoNodeEffect",
        "NoResponse",
        Some("Timeout"),
        None,
        None,
    )
    .await;
    let err = apply(&pool, 0x01)
        .await
        .expect_err("SubmittedUnknown is a HOLD");
    assert!(is_apply_err(&err, |e| matches!(
        e,
        ApplyError::HeldNotAutoRelease
    )));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
}

// ───────────────────────── ap06 — idempotent replay ──────────────────────────

#[tokio::test]
async fn ap06_replay_is_idempotent() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000006";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record_accepted(&pool, 0x01, "4000666666").await;
    let first = apply(&pool, 0x01).await.unwrap();
    assert!(first.applied);
    let second = apply(&pool, 0x01).await.expect("replay is a benign no-op");
    assert!(!second.applied, "second apply mutates nothing");
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
}

// ─────────────────────── ap07 — stale generation drops ───────────────────────

#[tokio::test]
async fn ap07_stale_generation_drops_without_mutation() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "2000000007";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    authorize(&pool, new_res(0x01, doc, fscl)).await;
    record_accepted(&pool, 0x01, "4000777777").await;
    // Simulate an intervening fence advance: bump the node generation past the reservation's.
    sqlx::query(
        "UPDATE node_state SET delivery_generation = delivery_generation + 5 WHERE fiscal_number=?",
    )
    .bind(fscl)
    .execute(&pool)
    .await
    .unwrap();
    let r = apply(&pool, 0x01)
        .await
        .expect("stale generation is a benign no-op");
    assert!(!r.applied, "stale generation drops");
    // NOTHING mutated: still PENDING, no SFN, no seed.
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_sfn(&pool, 0x11).await.is_none());
    assert!(read_seed(&pool, fscl).await.is_none());
}

// ═══════════════════════ boot PENDING resume (design §4.3 step 5) ══════════════

async fn resume(pool: &SqlitePool, res_byte: u8, fscl: &str) -> bool {
    let fscl = fscl.to_string();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            resume_crashed_reservation(tx, [res_byte; 16], &fscl)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("resume")
}

async fn read_evidence(pool: &SqlitePool, res_byte: u8) -> (Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT evidence_kind, evidence_text FROM delivery_reservation WHERE reservation_id=?",
    )
    .bind(&[res_byte; 16][..])
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_authorize_only(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) {
    authorize(pool, new_res(res_byte, doc, fscl)).await;
}

#[tokio::test]
async fn br01_crashed_call_started_resumes_to_pending_stop() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "3000000001";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    insert_authorize_only(&pool, 0x01, doc, fscl).await; // CALL_STARTED, no outcome (crashed)

    // Boot scan finds the crashed reservation.
    let crashed = list_call_started_without_outcome(&pool).await.unwrap();
    assert_eq!(crashed.len(), 1);
    assert_eq!(crashed[0].0, [0x01; 16]);
    assert_eq!(crashed[0].1, fscl);

    assert!(resume(&pool, 0x01, fscl).await, "converts the crashed row");

    // Converted to NoResponse{CrashedBeforeObservation} + PENDING_APPLY, node STOP_MODE.
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    let (kind, text) = read_evidence(&pool, 0x01).await;
    assert_eq!(kind.as_deref(), Some("NoResponse"));
    assert_eq!(text.as_deref(), Some("CrashedBeforeObservation"));
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");

    // The resumed row is a SubmittedUnknown HOLD — apply refuses (operator territory, Slice 5).
    let err = apply(&pool, 0x01)
        .await
        .expect_err("crashed resume is a HOLD");
    assert!(is_apply_err(&err, |e| matches!(
        e,
        ApplyError::HeldNotAutoRelease
    )));
}

#[tokio::test]
async fn br02_resume_is_idempotent() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "3000000002";
    let doc = seed_doc(&pool, fscl, 0x11, None).await;
    insert_authorize_only(&pool, 0x01, doc, fscl).await;
    assert!(resume(&pool, 0x01, fscl).await, "first converts");
    assert!(
        !resume(&pool, 0x01, fscl).await,
        "second is a no-op (already OUTCOME_OBSERVED)"
    );
}

#[tokio::test]
async fn br03_resumed_reservation_holds_the_fn_fence() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "3000000003";
    let doc_a = seed_doc(&pool, fscl, 0x11, None).await;
    let doc_b = seed_doc(&pool, fscl, 0x22, None).await;
    insert_authorize_only(&pool, 0x01, doc_a, fscl).await;
    resume(&pool, 0x01, fscl).await;
    // The resumed row rests at OUTCOME_OBSERVED + PENDING_APPLY — it HOLDS the §3.1 fence, so a
    // new document on the FN cannot authorize until operator completion resolves it.
    let err = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, new_res(0x02, doc_b, "3000000003"), TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect_err("the PENDING resumed reservation holds the fence");
    assert!(err
        .downcast_ref::<delivery_reservation::AuthorizeError>()
        .is_some());
}
