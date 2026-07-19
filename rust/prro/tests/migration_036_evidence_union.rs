//! Migration 036 — durable evidence union + fail-closed evidence matrix.
//!
//! RED-first teeth for CS-3 D/E Slice 2a (design §4.2 + §5). INACTIVE: the
//! migration only ADDs four nullable columns + three evidence triggers; no caller
//! is wired. The Rust EvidenceDiscriminant / record / hydration is Slice 2b.
//!
//! The matrix is VALIDATE-IF-PRESENT (see the migration header DEVIATION note):
//! all-NULL evidence at OUTCOME_OBSERVED is allowed; any present evidence must
//! match exactly one leaf. Slice 2b flips presence to mandatory.
//!
//! Groups:
//! - `ev_*`  — schema presence + valid-leaf acceptance + optional-absence.
//! - `tg_*`  — matrix tightness: each illegal/partial evidence row is REJECTED.
//! - `mx_*`  — the Rejected verdict→(routing,node_effect) map is complete + exact
//!   (live routing_for_reject, mod.rs:985-1002).
//! - `im_*`  — evidence immutability after OUTCOME_OBSERVED.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use sqlx::sqlite::SqliteQueryResult;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 036");
    (dir, pool)
}

const FN_A: &str = "1234567890";

async fn seed_doc(pool: &SqlitePool, doc_byte: u8, lnd: i64) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN_A)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(FN_A)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

fn new_res(res_byte: u8, doc: DocumentId) -> NewReservation {
    NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: FN_A.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    }
}

async fn insert_res(pool: &SqlitePool, row: NewReservation) {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::insert(tx, row)
                .await
                .map_err(Into::into)
        })
    })
    .await
    .expect("insert reservation");
}

async fn apply_release(pool: &SqlitePool, res_byte: u8) {
    // PENDING_APPLY → APPLIED releases the §3.1 fence so the next reservation on the
    // FN can be inserted (evidence unchanged, so the matrix + immutability pass).
    sqlx::query("UPDATE delivery_reservation SET apply_state='APPLIED' WHERE reservation_id=?")
        .bind(&[res_byte; 16][..])
        .execute(pool)
        .await
        .expect("PENDING_APPLY → APPLIED");
}

async fn mark_call_started(pool: &SqlitePool, res_byte: u8) {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='CALL_STARTED', call_started_at='2026-07-17T00:00:00Z', authorized_generation=1 \
         WHERE reservation_id=?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("RN→CS");
}

/// The OUTCOME_OBSERVED update payload. `None` binds SQL NULL.
#[derive(Clone)]
struct Oo {
    certainty: &'static str,
    provenance: &'static str,
    routing: Option<&'static str>,
    node_effect: &'static str,
    kind: Option<&'static str>,
    text: Option<&'static str>,
    code: Option<i64>,
    digest: Option<Vec<u8>>,
    rcid: Option<&'static str>,
}

/// Drive reservation `res_byte` to OUTCOME_OBSERVED with the given evidence.
/// For any non-`NOT_SUBMITTED` certainty a RN→CS step is performed first (the 033
/// transition trigger requires CALL_STARTED before a SUBMITTED* outcome).
async fn drive_oo(
    pool: &SqlitePool,
    res_byte: u8,
    oo: &Oo,
) -> Result<SqliteQueryResult, sqlx::Error> {
    if oo.certainty != "NOT_SUBMITTED" {
        mark_call_started(pool, res_byte).await;
    }
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state='OUTCOME_OBSERVED', submission_certainty=?, response_provenance=?, \
             routing_class=?, apply_state='PENDING_APPLY', node_effect=?, \
             evidence_kind=?, evidence_text=?, evidence_code=?, evidence_digest=?, \
             remote_correlation_id=? \
         WHERE reservation_id=?",
    )
    .bind(oo.certainty)
    .bind(oo.provenance)
    .bind(oo.routing)
    .bind(oo.node_effect)
    .bind(oo.kind)
    .bind(oo.text)
    .bind(oo.code)
    .bind(oo.digest.clone())
    .bind(oo.rcid)
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
}

fn d32() -> Vec<u8> {
    vec![0x5A; 32]
}

fn accepted(f: &'static str) -> Oo {
    Oo {
        certainty: "SUBMITTED",
        provenance: "PARSED_DPS_ENVELOPE",
        routing: None,
        node_effect: "NoNodeEffect",
        kind: Some("Accepted"),
        text: Some(f),
        code: None,
        digest: None,
        rcid: Some(f),
    }
}

fn rejected(verdict: &'static str, routing: &'static str, node_effect: &'static str) -> Oo {
    Oo {
        certainty: "SUBMITTED",
        provenance: "PARSED_DPS_ENVELOPE",
        routing: Some(routing),
        node_effect,
        kind: Some("Rejected"),
        text: Some(verdict),
        code: None,
        digest: Some(d32()),
        rcid: None,
    }
}

// ───────────────────────────── ev_* — schema + acceptance ────────────────────

#[tokio::test]
async fn ev01_evidence_columns_exist() {
    let (_d, pool) = fresh_pool().await;
    let cols: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('delivery_reservation')")
            .fetch_all(&pool)
            .await
            .unwrap();
    for c in [
        "evidence_kind",
        "evidence_text",
        "evidence_code",
        "evidence_digest",
    ] {
        assert!(cols.iter().any(|x| x == c), "missing column {c}: {cols:?}");
    }
}

#[tokio::test]
async fn ev02_evidence_triggers_exist() {
    let (_d, pool) = fresh_pool().await;
    let trg: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger'")
            .fetch_all(&pool)
            .await
            .unwrap();
    for t in [
        "delivery_reservation_evidence_insert",
        "delivery_reservation_evidence_matrix_update",
        "delivery_reservation_evidence_immutable",
    ] {
        assert!(trg.iter().any(|x| x == t), "missing trigger {t}");
    }
}

#[tokio::test]
async fn ev03_valid_accepted_ok_and_stored() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    drive_oo(&pool, 0x01, &accepted("4000999999"))
        .await
        .expect("valid Accepted stores");
    let (kind, text): (String, String) = sqlx::query_as(
        "SELECT evidence_kind, evidence_text FROM delivery_reservation WHERE reservation_id=?",
    )
    .bind(&[0x01u8; 16][..])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind, "Accepted");
    assert_eq!(
        text, "4000999999",
        "the exact accepted fiscal number is durable"
    );
}

#[tokio::test]
async fn ev04_valid_rejected_verify_ok() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    drive_oo(
        &pool,
        0x01,
        &rejected("Verify", "TerminalReject", "NoNodeEffect"),
    )
    .await
    .expect("valid Rejected/Verify stores");
}

#[tokio::test]
async fn ev05_valid_noresponse_ok() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let oo = Oo {
        certainty: "SUBMITTED_UNKNOWN",
        provenance: "NO_RESPONSE",
        routing: Some("TransientRetry"),
        node_effect: "NoNodeEffect",
        kind: Some("NoResponse"),
        text: Some("Timeout"),
        code: None,
        digest: None,
        rcid: None,
    };
    drive_oo(&pool, 0x01, &oo)
        .await
        .expect("valid NoResponse/Timeout stores");
}

#[tokio::test]
async fn ev06_optional_absent_evidence_ok() {
    // Validate-if-present: a clean-accept OO row with NO evidence at all is allowed
    // in Slice 2a (Slice 2b flips presence to mandatory).
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let oo = Oo {
        certainty: "SUBMITTED",
        provenance: "PARSED_DPS_ENVELOPE",
        routing: None,
        node_effect: "NoNodeEffect",
        kind: None,
        text: None,
        code: None,
        digest: None,
        rcid: None,
    };
    drive_oo(&pool, 0x01, &oo)
        .await
        .expect("all-NULL evidence at OO is allowed (validate-if-present)");
}

// ───────────────────────────── tg_* — matrix tightness ───────────────────────

async fn expect_reject(pool: &SqlitePool, res_byte: u8, oo: &Oo, why: &str) {
    let err = drive_oo(pool, res_byte, oo).await.expect_err(why);
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("evidence") || msg.contains("abort") || msg.contains("constraint"),
        "expected an evidence-matrix rejection ({why}); got: {err}"
    );
}

#[tokio::test]
async fn tg01_accepted_missing_fiscal_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let mut oo = accepted("4000999999");
    oo.text = None; // Accepted with no fiscal number
    oo.rcid = None;
    expect_reject(
        &pool,
        0x01,
        &oo,
        "Accepted requires a non-empty fiscal number",
    )
    .await;
}

#[tokio::test]
async fn tg02_accepted_correlation_mismatch_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let mut oo = accepted("4000999999");
    oo.rcid = Some("4000000001"); // correlation != evidence_text
    expect_reject(
        &pool,
        0x01,
        &oo,
        "Accepted requires remote_correlation_id == evidence_text",
    )
    .await;
}

#[tokio::test]
async fn tg03_rejected_wrong_verdict_routing_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    // BadHashPrev must map to MacRecovery/MacReseedPending; give it TerminalReject.
    let oo = rejected("BadHashPrev", "TerminalReject", "NoNodeEffect");
    expect_reject(
        &pool,
        0x01,
        &oo,
        "Rejected verdict routing must match routing_for_reject",
    )
    .await;
}

#[tokio::test]
async fn tg04_rejected_unknown_verdict_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let oo = rejected("NotAVerdict", "TerminalReject", "NoNodeEffect");
    expect_reject(
        &pool,
        0x01,
        &oo,
        "Rejected verdict must be a known DpsReject name",
    )
    .await;
}

#[tokio::test]
async fn tg05_noresponse_bad_cause_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let oo = Oo {
        certainty: "SUBMITTED_UNKNOWN",
        provenance: "NO_RESPONSE",
        routing: Some("TransientRetry"),
        node_effect: "NoNodeEffect",
        kind: Some("NoResponse"),
        text: Some("Nonsense"),
        code: None,
        digest: None,
        rcid: None,
    };
    expect_reject(
        &pool,
        0x01,
        &oo,
        "NoResponse cause must be a known NoResponseCause",
    )
    .await;
}

#[tokio::test]
async fn tg06_digest_wrong_length_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let oo = Oo {
        certainty: "SUBMITTED_UNKNOWN",
        provenance: "AUTHENTICATED_PEER",
        routing: Some("ProbeRequired"),
        node_effect: "ProbeRequired",
        kind: Some("RemoteAuthStatus"),
        text: None,
        code: None,
        digest: Some(vec![0x5A; 16]), // 16 bytes, not 32
        rcid: None,
    };
    expect_reject(&pool, 0x01, &oo, "digest must be exactly 32 bytes").await;
}

#[tokio::test]
async fn tg07_unknown_kind_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let mut oo = accepted("4000999999");
    oo.kind = Some("Bogus");
    expect_reject(
        &pool,
        0x01,
        &oo,
        "an unknown evidence_kind at OO is rejected",
    )
    .await;
}

#[tokio::test]
async fn tg08_partial_evidence_no_kind_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    // Clean accept row, no kind, but a stray evidence_text — rule (b).
    let oo = Oo {
        certainty: "SUBMITTED",
        provenance: "PARSED_DPS_ENVELOPE",
        routing: None,
        node_effect: "NoNodeEffect",
        kind: None,
        text: Some("4000999999"),
        code: None,
        digest: None,
        rcid: None,
    };
    expect_reject(
        &pool,
        0x01,
        &oo,
        "partial evidence without a kind is rejected",
    )
    .await;
}

#[tokio::test]
async fn tg09_evidence_before_oo_rejected() {
    // Rule (a): a CALL_STARTED row (state != OO) may carry no evidence.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    mark_call_started(&pool, 0x01).await;
    let err = sqlx::query(
        "UPDATE delivery_reservation SET evidence_kind='Accepted' WHERE reservation_id=?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("evidence before OUTCOME_OBSERVED is rejected");
    assert!(
        err.to_string().to_lowercase().contains("evidence"),
        "got: {err}"
    );
}

#[tokio::test]
async fn tg10_remote_auth_missing_digest_rejected() {
    // Matrix-only NULL-bypass: 033/034 do not check the digest; the matrix does.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    let oo = Oo {
        certainty: "SUBMITTED_UNKNOWN",
        provenance: "AUTHENTICATED_PEER",
        routing: Some("ProbeRequired"),
        node_effect: "ProbeRequired",
        kind: Some("RemoteAuthStatus"),
        text: None,
        code: None,
        digest: None, // required non-NULL len 32
        rcid: None,
    };
    expect_reject(
        &pool,
        0x01,
        &oo,
        "RemoteAuthStatus requires a 32-byte digest",
    )
    .await;
}

// ───────────────────────────── mx_* — verdict map faithfulness ───────────────

#[tokio::test]
async fn mx01_all_thirteen_verdicts_map_exactly() {
    // Every DpsReject verdict with its LIVE routing_for_reject (routing, node_effect)
    // stores; the map is complete + exact (mod.rs:985-1002).
    let (_d, pool) = fresh_pool().await;
    let cases: [(&str, &str, &str); 13] = [
        ("Verify", "TerminalReject", "NoNodeEffect"),
        ("Type", "TerminalReject", "NoNodeEffect"),
        ("Xml", "TerminalReject", "NoNodeEffect"),
        ("XmlDate", "TerminalReject", "NoNodeEffect"),
        ("XmlChk", "TerminalReject", "NoNodeEffect"),
        ("XmlZReport", "TerminalReject", "NoNodeEffect"),
        ("OfflineId", "TerminalReject", "NoNodeEffect"),
        ("Close", "TerminalReject", "NoNodeEffect"),
        ("NotPrevZReport", "OperatorEscalation", "OperatorEscalation"),
        ("Offline168", "TerminalReject", "NodeBlocked"),
        ("BadHashPrev", "MacRecovery", "MacReseedPending"),
        ("NotRegisteredRro", "FnConfigError", "FnConfigError"),
        ("NotRegisteredSigner", "FnConfigError", "FnConfigError"),
    ];
    for (i, (verdict, routing, node_effect)) in cases.iter().enumerate() {
        let res_byte = 0x20 + i as u8;
        let doc = seed_doc(&pool, res_byte, 100 + i as i64).await;
        insert_res(&pool, new_res(res_byte, doc)).await;
        drive_oo(&pool, res_byte, &rejected(verdict, routing, node_effect))
            .await
            .unwrap_or_else(|e| panic!("verdict {verdict} with its live routing must store: {e}"));
        apply_release(&pool, res_byte).await; // release the fence before the next FN_A reservation
    }
}

// ───────────────────────────── im_* — evidence immutability ──────────────────

#[tokio::test]
async fn im01_evidence_frozen_after_oo() {
    // Record Rejected/Verify (digest d32), then mutate ONLY evidence_digest to another
    // 32-byte value: the matrix still matches (len 32, Verify→TerminalReject) and
    // remote_correlation_id is unchanged (NULL), so ONLY the evidence-immutability
    // trigger can catch it — isolating the immutability guard.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc)).await;
    drive_oo(
        &pool,
        0x01,
        &rejected("Verify", "TerminalReject", "NoNodeEffect"),
    )
    .await
    .expect("record Rejected/Verify");
    let other = vec![0x77u8; 32];
    let err =
        sqlx::query("UPDATE delivery_reservation SET evidence_digest=? WHERE reservation_id=?")
            .bind(&other)
            .bind(&[0x01u8; 16][..])
            .execute(&pool)
            .await
            .expect_err("evidence is immutable after OUTCOME_OBSERVED");
    assert!(
        err.to_string().to_lowercase().contains("immutable"),
        "got: {err}"
    );
}
