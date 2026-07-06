//! A′.1 Phase C piece 2 — stage_send confirm edges 3/10 (SENT-commit).
//!
//! Edge 3 (Opening→Opened) fires on a fresh `WireDecision::Sent` Applied-CAS
//! of an online-origin `SHIFT_OPEN`; edge 10 (Closing→Closed) for online
//! `Z_REPORT`/`SHIFT_CLOSE`.  "DPS Ack" = SENT + server_fiscal_no, NOT
//! terminal Ack.  POST-WIRE: a non-Applied shift-CAS does NOT roll back the
//! Sent commit (wire-truth wins) — drift is audited CRITICAL and left to
//! recovery.  Online-origin ONLY (drain owns the offline edges).
//!
//! Drives `stage_send::run` (StubDpsChannel accept) directly, same idiom as
//! `write_path_stage4_send.rs`, plus one cross-stage E2E through
//! `stage_acquire::run` (the flagship admission test (c)).

use sqlx::SqlitePool;

use prro::db::models::ids::{CashierId, DocumentId, RequestId};
use prro::services::write_path::stage_send::{self, StageSendOutcome};
use prro::transports::dps::dto::CheckAck;

mod common;
use common::StubDpsChannel;

const FN: &str = "1234567890";

// ─── harness ──────────────────────────────────────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool");
    (dir, pool)
}

async fn fresh_secure_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_secure_pool(&dir.path().join("s.db"))
        .await
        .expect("open_secure_pool");
    (dir, pool)
}

fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![],
    }
}

async fn seed_fn(pool: &SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_shift(pool: &SqlitePool, id: &[u8], state: &str) {
    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, ?, 'ONLINE', 0, 'test-cashier')",
    )
    .bind(id)
    .bind(FN)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_node(pool: &SqlitePool, shift_state: &str, current: Option<&[u8]>) {
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, 'ONLINE', ?, ?, 5, 'b1', 't1')",
    )
    .bind(FN)
    .bind(shift_state)
    .bind(current)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a SIGNED `fiscal_documents` row (+ SIGNED_XML) bound to `shift_id`,
/// online-origin (`fs_mode='ONLINE'`, `offline_fiscal_no` from arg).
/// `cashier` set for non-bypass (SELL) so signer_guard passes.
#[allow(clippy::too_many_arguments)]
async fn seed_signed_doc(
    pool: &SqlitePool,
    doc_byte: u8,
    doc_type: &str,
    lnd: i64,
    state: &str,
    shift_id: Option<&[u8]>,
    offline_fiscal_no: Option<i64>,
    fs_mode: &str,
    cashier: Option<&str>,
) -> DocumentId {
    let doc = vec![doc_byte; 16];
    let req = vec![doc_byte ^ 0xFF; 16];
    // A.3 (advance-at-SEND): an online-origin doc (offline_fiscal_no NULL)
    // that reaches the Sending → Sent CAS advances the chain seed to this
    // doc's unsigned_xml_sha256, so stage_send reads it (non-NULL 32-byte).
    // The genesis node_state seeded by `seed_node` (last_known NULL) matches
    // this doc's NULL previous_hash so the pre-advance drift-assert passes.
    // (Harmless for the offline-origin negative fixture — its advance is
    // origin-gated off, so unsigned is simply never read.)
    let unsigned = vec![doc_byte; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no, \
            signed_by_cashier_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'b1', 't1', ?, '2026-05-09T12:34:56Z', '{}', ?, ?, ?, ?)",
    )
    .bind(&doc)
    .bind(&req)
    .bind(FN)
    .bind(shift_id)
    .bind(lnd)
    .bind(doc_type)
    .bind(state)
    .bind(fs_mode)
    .bind(vec![0u8; 32])
    .bind(&unsigned)
    .bind(offline_fiscal_no)
    .bind(cashier)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc)
    .bind(b"FAKE-CMS".to_vec())
    .execute(pool)
    .await
    .unwrap();
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc.as_slice()).unwrap())
}

// ─── readers ──────────────────────────────────────────────────────────

async fn doc_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn sfn(pool: &SqlitePool, doc: DocumentId) -> Option<String> {
    sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn shift_state_of(pool: &SqlitePool, id: &[u8]) -> String {
    sqlx::query_scalar("SELECT state FROM shifts WHERE shift_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn node_shift_state(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT shift_state FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn node_current_shift_id(pool: &SqlitePool) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT current_shift_id FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
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

const S: [u8; 16] = [0x5a; 16];

// ─── (a) edge 3 — online SHIFT_OPEN Sent-confirm → Opening→Opened ───────
#[tokio::test]
async fn a_shift_open_sent_confirm_drives_opened() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool).await;
    seed_shift(&pool, &S, "OPENING").await;
    seed_node(&pool, "OPENING", Some(&S)).await;
    let doc = seed_signed_doc(
        &pool,
        0x11,
        "SHIFT_OPEN",
        5,
        "SIGNED",
        Some(&S),
        None,
        "ONLINE",
        None,
    )
    .await;

    let out = stage_send::run(&pool, &StubDpsChannel::new(Ok(ack("DPS-1"))), doc, None)
        .await
        .unwrap();
    assert!(matches!(out, StageSendOutcome::Sent { .. }), "got {out:?}");

    // doc committed Sent + sfn (in the SAME 4-b tx as the shift edge)
    assert_eq!(doc_state(&pool, doc).await, "SENT");
    assert_eq!(sfn(&pool, doc).await.as_deref(), Some("DPS-1"));
    // edge 3: shift + projection to Opened, pointer intact
    assert_eq!(shift_state_of(&pool, &S).await, "OPENED");
    assert_eq!(node_shift_state(&pool).await, "OPENED");
    assert_eq!(
        node_current_shift_id(&pool).await.as_deref(),
        Some(S.as_slice())
    );
}

// ─── (b) edge 10 — online Z Sent-confirm → Closing→Closed (pointer kept) ─
#[tokio::test]
async fn b_z_report_sent_confirm_drives_closed_pointer_kept() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool).await;
    seed_shift(&pool, &S, "CLOSING").await;
    seed_node(&pool, "CLOSING", Some(&S)).await;
    let doc = seed_signed_doc(
        &pool,
        0x22,
        "Z_REPORT",
        5,
        "SIGNED",
        Some(&S),
        None,
        "ONLINE",
        None,
    )
    .await;

    let out = stage_send::run(&pool, &StubDpsChannel::new(Ok(ack("DPS-2"))), doc, None)
        .await
        .unwrap();
    assert!(matches!(out, StageSendOutcome::Sent { .. }), "got {out:?}");
    assert_eq!(doc_state(&pool, doc).await, "SENT");
    assert_eq!(shift_state_of(&pool, &S).await, "CLOSED");
    assert_eq!(node_shift_state(&pool).await, "CLOSED");
    // close-pin: current_shift_id NOT cleared (next open overwrites)
    assert_eq!(
        node_current_shift_id(&pool).await.as_deref(),
        Some(S.as_slice()),
        "terminal_close_pins_current_shift_id_behavior"
    );
}

// ─── (c) 🏁 E2E ADMISSION: open shift → SENT-confirm → SELL admitted → SENT
#[tokio::test]
async fn c_e2e_open_then_sell_admitted_and_sent() {
    let (_d, pool) = fresh_pool().await;
    let (_s, secure) = fresh_secure_pool().await;
    seed_fn(&pool).await;
    seed_shift(&pool, &S, "OPENING").await;
    seed_node(&pool, "OPENING", Some(&S)).await;
    // SHIFT_OPEN at lnd=4 (node.next_lnd=5) so the later acquired SELL gets
    // lnd=5 without colliding on ux_fd_fn_lnd.
    let open_doc = seed_signed_doc(
        &pool,
        0x31,
        "SHIFT_OPEN",
        4,
        "SIGNED",
        Some(&S),
        None,
        "ONLINE",
        None,
    )
    .await;

    // 1. SHIFT_OPEN confirms → edge 3 → shift OPENED.
    stage_send::run(
        &pool,
        &StubDpsChannel::new(Ok(ack("DPS-OPEN"))),
        open_doc,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        node_shift_state(&pool).await,
        "OPENED",
        "shift opened by edge 3"
    );

    // 2. SELL is now ADMITTED by stage_acquire (guard (Sell, Opened) → allow).
    let req = *RequestId::new().as_bytes();
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, signed_by_cashier_id, driver_id) \
         VALUES (?, ?, 'REST', 'SELL', ?, '{\"goods\":[]}', ?, 'test-cashier', 'drv')",
    )
    .bind(req.as_slice())
    .bind(FN)
    .bind(format!("idem-{}", req[0]))
    .bind(vec![0u8; 32])
    .execute(&pool)
    .await
    .unwrap();
    let cmd = prro::services::write_path::types::CanonicalFiscalCommand {
        doc_type: prro::db::models::enums::DocType::Sell,
        business_ts: "2026-05-09T12:34:56Z".into(),
        total_sum_kop: Some(15000),
        payload_json: r#"{"goods":[]}"#.into(),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        signed_by_cashier_id: Some(CashierId::new("test-cashier").unwrap()),
        driver_id: None,
    };
    let acq = prro::services::write_path::stage_acquire::run(&pool, &secure, "drv", req, cmd)
        .await
        .unwrap();
    let sell_doc = match acq {
        prro::services::write_path::types::WorkerProcessResult::Proceed(ctx) => {
            ctx.document.document_id
        }
        other => panic!("SELL must be ADMITTED after shift opened, got {other:?}"),
    };

    // 3. The admitted SELL reaches SENT (promote PREPARED→SIGNED + XML, then send).
    // A.3 chain-continuity: the SHIFT_OPEN send above advanced the seed to
    // open_doc.unsigned_xml_sha256 = [0x31; 32].  This SELL is the NEXT link,
    // so its previous_hash must equal that advanced seed (else the pre-advance
    // drift-assert fails), and it needs its own non-NULL unsigned_xml_sha256
    // as the next advance target.  stage_acquire minted it with both NULL
    // (previous_hash/unsigned are populated by later sign-stage wiring), so the
    // manual sign-promotion sets them here.
    sqlx::query(
        "UPDATE fiscal_documents \
         SET state = 'SIGNED', previous_hash = ?, unsigned_xml_sha256 = ? \
         WHERE document_id = ?",
    )
    .bind(vec![0x31u8; 32])
    .bind(vec![0x3Fu8; 32])
    .bind(sell_doc)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(sell_doc)
    .bind(b"FAKE-CMS".to_vec())
    .execute(&pool)
    .await
    .unwrap();
    let out = stage_send::run(
        &pool,
        &StubDpsChannel::new(Ok(ack("DPS-SELL"))),
        sell_doc,
        None,
    )
    .await
    .unwrap();
    assert!(
        matches!(out, StageSendOutcome::Sent { .. }),
        "SELL must reach SENT, got {out:?}"
    );
    assert_eq!(doc_state(&pool, sell_doc).await, "SENT");
    // SELL is shift-neutral: shift stays OPENED.
    assert_eq!(node_shift_state(&pool).await, "OPENED");
}

// ─── (d) SELL through 4-b → zero shift-writes (neutrality) ──────────────
#[tokio::test]
async fn d_sell_send_is_shift_neutral() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool).await;
    seed_shift(&pool, &S, "OPENED").await;
    seed_node(&pool, "OPENED", Some(&S)).await;
    let doc = seed_signed_doc(
        &pool,
        0x44,
        "SELL",
        5,
        "SIGNED",
        Some(&S),
        None,
        "ONLINE",
        Some("test-cashier"),
    )
    .await;

    stage_send::run(&pool, &StubDpsChannel::new(Ok(ack("DPS-4"))), doc, None)
        .await
        .unwrap();
    assert_eq!(doc_state(&pool, doc).await, "SENT");
    assert_eq!(
        shift_state_of(&pool, &S).await,
        "OPENED",
        "SELL does not move the shift"
    );
    assert_eq!(node_shift_state(&pool).await, "OPENED");
}

// ─── (e) reject/Routed → edges do NOT fire, shift stays Opening ─────────
#[tokio::test]
async fn e_reject_does_not_fire_edge() {
    use prro::transports::dps::error::DpsError;
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool).await;
    seed_shift(&pool, &S, "OPENING").await;
    seed_node(&pool, "OPENING", Some(&S)).await;
    let doc = seed_signed_doc(
        &pool,
        0x55,
        "SHIFT_OPEN",
        5,
        "SIGNED",
        Some(&S),
        None,
        "ONLINE",
        None,
    )
    .await;

    // DPS terminal reject → Routed(Rejected), NOT WireDecision::Sent.
    let stub = StubDpsChannel::new(Err(DpsError::Authorization {
        code: -1,
        kind: prro::transports::dps::error::AuthorizationKind::DocumentReject,
        message: "rejected".into(),
    }));
    stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert_ne!(doc_state(&pool, doc).await, "SENT");
    assert_eq!(
        shift_state_of(&pool, &S).await,
        "OPENING",
        "no edge on non-Sent"
    );
    assert_eq!(node_shift_state(&pool).await, "OPENING");
}

// ─── (f) offline-origin doc → edges do NOT fire (origin-gate negative) ──
#[tokio::test]
async fn f_offline_origin_does_not_fire_edge() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool).await;
    seed_shift(&pool, &S, "OPENING").await;
    seed_node(&pool, "OPENING", Some(&S)).await;
    // offline-origin SHIFT_OPEN in the drain source state, offline_fiscal_no set.
    let doc = seed_signed_doc(
        &pool,
        0x66,
        "SHIFT_OPEN",
        5,
        "OFFLINE_LOCAL_ACK",
        Some(&S),
        Some(7),
        "OFFLINE",
        None,
    )
    .await;

    stage_send::run(&pool, &StubDpsChannel::new(Ok(ack("DPS-6"))), doc, None)
        .await
        .unwrap();
    assert_eq!(doc_state(&pool, doc).await, "SENT");
    // origin gate: drain owns the offline shift edges — piece-2 edges must skip.
    assert_eq!(
        shift_state_of(&pool, &S).await,
        "OPENING",
        "offline-origin: no online edge"
    );
    assert_eq!(node_shift_state(&pool).await, "OPENING");
}

// ─── (g) non-Applied (shift in Error) → Sent STANDS + CRITICAL, no rollback
#[tokio::test]
async fn g_non_applied_keeps_sent_and_audits_critical() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool).await;
    // Boot-orphan family: shift quarantined to ERROR before the doc re-sends.
    seed_shift(&pool, &S, "ERROR").await;
    seed_node(&pool, "OPENING", Some(&S)).await;
    let doc = seed_signed_doc(
        &pool,
        0x77,
        "SHIFT_OPEN",
        5,
        "SIGNED",
        Some(&S),
        None,
        "ONLINE",
        None,
    )
    .await;

    let out = stage_send::run(&pool, &StubDpsChannel::new(Ok(ack("DPS-7"))), doc, None)
        .await
        .unwrap();
    // POST-WIRE: DPS accepted → Sent commit STANDS despite the shift drift.
    assert!(matches!(out, StageSendOutcome::Sent { .. }), "got {out:?}");
    assert_eq!(
        doc_state(&pool, doc).await,
        "SENT",
        "no rollback of the Sent commit"
    );
    assert_eq!(
        shift_state_of(&pool, &S).await,
        "ERROR",
        "shift untouched (edge did not force)"
    );
    // real drift → CRITICAL audit; recovery owns the repair.
    assert_eq!(audit_count(&pool, "SHIFT_CONFIRM_EDGE_DRIFT").await, 1);
}

// ─── (h) idempotency: re-drive of a Sent doc never reaches 4-b ──────────
#[tokio::test]
async fn h_redrive_sent_doc_state_conflict_no_edge() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool).await;
    seed_shift(&pool, &S, "OPENED").await;
    seed_node(&pool, "OPENED", Some(&S)).await;
    // Doc already Sent (a prior confirm) — 4-pre allowlist is {Signed,
    // ErrorRetryable, OfflineLocalAck}; Sent is excluded → StateConflict.
    let doc = seed_signed_doc(
        &pool,
        0x88,
        "SHIFT_OPEN",
        5,
        "SENT",
        Some(&S),
        None,
        "ONLINE",
        None,
    )
    .await;

    let stub = StubDpsChannel::new(Ok(ack("DPS-8")));
    let out = stage_send::run(&pool, &stub, doc, None).await.unwrap();
    assert!(
        matches!(
            out,
            StageSendOutcome::StateConflict {
                observed: prro::db::models::enums::DocState::Sent
            }
        ),
        "re-drive of a Sent doc must short-circuit at 4-pre, got {out:?}"
    );
    assert_eq!(stub.call_count(), 0, "no wire call on StateConflict");
    // edge never runs; shift untouched.
    assert_eq!(shift_state_of(&pool, &S).await, "OPENED");
}
