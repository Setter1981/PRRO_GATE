//! CS-3 Slice 5 — STOP operator completion of a PENDING reservation.
//!
//! RED-first teeth for `delivery_reservation::complete_operator_pending` (design §3.4). A
//! reservation held PENDING under STOP_MODE (a SubmittedUnknown / crashed send) is released only by
//! a typed operator resolution + the full authority CAS; the origin-split effect + APPLIED +
//! pointer-clear + verified mode target all commit together. INACTIVE — driven by the extended
//! reset_stop_mode admin path only at the cutover (Slice 7).
//!
//! - `oc01` accepted online: stamp F + seed advance + APPLIED + pointer clear + mode ONLINE;
//! - `oc02` accepted offline: stamp F + ZERO seed + APPLIED;
//! - `oc03` not-accepted online: doc Sending→RMR + APPLIED + seed unchanged;
//! - `oc04` MAC reseed: operator seed + doc→RMR + APPLIED;
//! - `oc05` mode target GOING_ONLINE when an active offline session must drain;
//! - `oc06` offline not-accepted → refused (cohort cleanup, Slice 5b) — nothing mutated;
//! - `oc07` shift-family doc → refused (Slice 5b) — nothing mutated;
//! - `oc08` stale generation → StaleAuthority — nothing mutated;
//! - `oc09` after completion the pointer is clear and the next document can authorize.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    self, authorize_submission, complete_operator_pending, record_outcome,
    resume_crashed_reservation, AttemptObservation, Authorization, CompletionError,
    CompletionResult, ModeTarget, NewReservation, OperatorResolution,
};
use prro::db::tx::with_immediate;
use prro_domain::delivery::evidence::EvidenceDiscriminant;
use prro_domain::delivery::{
    classify, AuthorizedGeneration, DecodedResponseDigest, DpsProtocolBinding, DpsProtocolId,
    EnvelopeHash, NonOkStatusCode, ObservedOutcomeV1, PositiveGeneration, ProtocolContractVersion,
    SendOutcome, SendResponse, SubmissionEvidence,
};
use prro_domain::enums::DocType;
use sqlx::SqlitePool;

const TS: &str = "2026-07-20T00:00:00Z";
const SEED: [u8; 32] = [0x77; 32];
const OPSEED: [u8; 32] = [0x99; 32];
/// The last-issued chain tip a MacReseed hardening fixture pins (distinct from `OPSEED`).
const PRED_TIP: [u8; 32] = [0x55; 32];

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

async fn seed_doc(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    doc_type: &str,
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
         VALUES (?, ?, ?, ?, ?, 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(doc_byte as i64)
    .bind(doc_type)
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

/// Authorize (RN→CALL_STARTED) then boot-resume the crash → OUTCOME_OBSERVED + PENDING_APPLY +
/// node STOP_MODE. This is the realistic held state the operator completes.
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
            resume_crashed_reservation(tx, [res_byte; 16], &fscl_owned)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("resume to PENDING+STOP");
}

async fn complete(
    pool: &SqlitePool,
    res_byte: u8,
    resolution: OperatorResolution,
) -> Result<CompletionResult, anyhow::Error> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            complete_operator_pending(tx, [res_byte; 16], resolution)
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
fn is_err(err: &anyhow::Error, pred: impl Fn(&CompletionError) -> bool) -> bool {
    err.downcast_ref::<CompletionError>().is_some_and(pred)
}

// ── MacReseed hardening fixtures (faithful `-12` hold + issued predecessor) ──────────
//
// A real `MacReseedPending` hold is a `-12` (BadHashPrev) DPS reject recorded through the
// production `record_outcome` boundary — NOT the crash path `held_pending` drives. These mirror
// `record_outcome.rs`'s domain builders so the hold row satisfies the evidence-union trigger
// (evidence_kind=Rejected, node_effect=MacReseedPending, routing_class=MacRecovery) BY
// CONSTRUCTION rather than by a raw UPDATE the evidence-immutability trigger would reject.

fn binding() -> DpsProtocolBinding {
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}
fn started(response: SendResponse) -> SubmissionEvidence {
    SubmissionEvidence::Started {
        response,
        binding: binding(),
        envelope_hash: EnvelopeHash([0u8; 32]),
    }
}
fn from_code(code: i32) -> SubmissionEvidence {
    started(SendResponse::parsed(SendOutcome::from_server_code(
        NonOkStatusCode::from_transport(code).unwrap(),
        DocType::Sell,
        DecodedResponseDigest::from_transport_digest([0xAB; 32]),
    )))
}
fn build(ev: &SubmissionEvidence, gen: i64) -> (EvidenceDiscriminant, ObservedOutcomeV1) {
    let classified = classify(ev);
    let disc = EvidenceDiscriminant::from_evidence(ev);
    let outcome = ObservedOutcomeV1::record(
        &classified,
        None,
        AuthorizedGeneration::Started(PositiveGeneration::new(gen).unwrap()),
    )
    .expect("observed-outcome mint");
    (disc, outcome)
}
async fn authorize(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) -> Authorization {
    let row = new_res(res_byte, doc, fscl);
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize")
}

/// Drive a fresh reservation to a REAL `-12` BadHashPrev / `MacReseedPending` hold:
/// `OUTCOME_OBSERVED` + `PENDING_APPLY` + `node_effect = MacReseedPending` + node `STOP_MODE`.
async fn held_macreseed_pending(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) {
    let auth = authorize(pool, res_byte, doc, fscl).await;
    let ev = from_code(-12); // BadHashPrev → MacReseedPending (held)
    let (disc, outcome) = build(&ev, 1);
    let obs = AttemptObservation::from_authorization(auth, ev);
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            record_outcome(tx, &obs, &outcome, &disc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("-12 records the MacReseedPending hold");
}

/// Insert an ISSUED online predecessor (`SENT` + non-empty `server_fiscal_no`) carrying
/// `unsigned_xml_sha256 = tip`, and pin `node_state.last_known_unsigned_xml_sha256 = tip` so the
/// FN chain is coherent. This is the value `invariant_scan` (and the guard) treats as the expected
/// chain tip. `lnd` must be below the held `-12` doc's lnd so it is the last-issued row.
async fn seed_issued_predecessor(
    pool: &SqlitePool,
    fscl: &str,
    doc_byte: u8,
    lnd: i64,
    tip: &[u8],
) {
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, server_fiscal_no) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENT', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:00:00Z', '{}', ?, ?, 'F-PRED')",
    )
    .bind(vec![doc_byte; 16])
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(fscl)
    .bind(lnd)
    .bind(vec![0u8; 32])
    .bind(tip)
    .execute(pool)
    .await
    .expect("seed issued predecessor");
    sqlx::query("UPDATE node_state SET last_known_unsigned_xml_sha256 = ? WHERE fiscal_number = ?")
        .bind(tip)
        .bind(fscl)
        .execute(pool)
        .await
        .expect("pin node_state chain tip");
}

// ───────────────────────────── oc01 accepted online ──────────────────────────

#[tokio::test]
async fn oc01_accepted_online_stamps_seed_applies_and_returns_online() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000001";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
    let r = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000111111".into(),
        },
    )
    .await
    .expect("accepted completes");
    assert!(r.applied && r.seed_advanced && r.mode_target == ModeTarget::Online);
    assert_eq!(read_sfn(&pool, 0x11).await.as_deref(), Some("4000111111"));
    assert_eq!(read_seed(&pool, fscl).await.as_deref(), Some(&SEED[..]));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
    assert!(read_pointer(&pool, fscl).await.is_none());
    assert_eq!(read_mode(&pool, fscl).await, "ONLINE");
}

// ───────────────────────────── oc02 accepted offline ─────────────────────────

#[tokio::test]
async fn oc02_accepted_offline_no_seed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000002";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", Some(500)).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let r = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000222222".into(),
        },
    )
    .await
    .expect("offline accepted completes");
    assert!(r.applied && !r.seed_advanced);
    assert_eq!(read_sfn(&pool, 0x11).await.as_deref(), Some("4000222222"));
    assert!(
        read_seed(&pool, fscl).await.is_none(),
        "offline never advances the seed"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
}

// ───────────────────────────── oc03 not-accepted online ──────────────────────

#[tokio::test]
async fn oc03_not_accepted_online_moves_doc_to_rmr() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000003";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let r = complete(&pool, 0x01, OperatorResolution::NotAccepted)
        .await
        .expect("not-accepted completes");
    assert!(r.applied && !r.seed_advanced && r.server_fiscal_no.is_none());
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    assert!(
        read_seed(&pool, fscl).await.is_none(),
        "seed unchanged on not-accepted"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
    assert_eq!(read_mode(&pool, fscl).await, "ONLINE");
}

// ───────────────────────────── oc04 MAC reseed ───────────────────────────────

#[tokio::test]
async fn oc04_mac_reseed_installs_operator_seed_and_manual() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000004";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    // A real `-12` MacReseedPending hold, with an issued predecessor whose chain tip is the seed
    // the operator installs (OPSEED). MacReseed hardening: the seed must equal the last-issued tip
    // AND the hold must be MacReseedPending — both hold here, so the happy path still installs it.
    seed_issued_predecessor(&pool, fscl, 0x10, 0x10, &OPSEED).await;
    held_macreseed_pending(&pool, 0x01, doc, fscl).await;
    let r = complete(&pool, 0x01, OperatorResolution::MacReseed { seed: OPSEED })
        .await
        .expect("MAC reseed completes");
    assert!(r.applied && r.seed_advanced);
    assert_eq!(
        read_seed(&pool, fscl).await.as_deref(),
        Some(&OPSEED[..]),
        "operator seed installed"
    );
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
}

// ───────── oc23 MacReseed on a non-MacReseedPending hold refused (guard A) ─────────
//
// MacReseed hardening (prod finding 2026-07-24): the fuzzer drove `MacReseed` on a NoResponse /
// crash hold (node_effect = NoNodeEffect) — NOT a `-12` MacReseedPending hold — and prod installed
// the operator seed anyway, silently re-basing the chain (a later `invariant_scan`
// ChainSeedMismatch, no fail-closed). Now fail-closed BEFORE any mutation.
#[tokio::test]
async fn oc23_mac_reseed_on_non_macreseedpending_hold_refused_nothing_mutated() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000023";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await; // NoResponse / NoNodeEffect crash hold
    let err = complete(&pool, 0x01, OperatorResolution::MacReseed { seed: OPSEED })
        .await
        .expect_err("MacReseed on a non-MacReseedPending hold must be refused");
    assert!(
        is_err(&err, |e| matches!(
            e,
            CompletionError::MacReseedHoldMismatch
        )),
        "expected MacReseedHoldMismatch, got {err:?}"
    );
    // Nothing mutated — seed absent, doc still SENDING, reservation still PENDING under STOP.
    assert!(read_seed(&pool, fscl).await.is_none(), "seed unchanged");
    assert_eq!(read_doc_state(&pool, 0x11).await, "SENDING");
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_pointer(&pool, fscl).await.is_some(), "pointer intact");
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
}

// ───────── oc24 MacReseed seed not matching the chain tip refused (guard B) ─────────
//
// MacReseed hardening: even on a valid `-12` MacReseedPending hold, an operator seed that does not
// equal the last-issued chain tip re-bases `node_state` to an unrelated value (a durable
// ChainSeedMismatch). Now fail-closed BEFORE any mutation.
#[tokio::test]
async fn oc24_mac_reseed_wrong_seed_refused_nothing_mutated() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000024";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    seed_issued_predecessor(&pool, fscl, 0x10, 0x10, &PRED_TIP).await; // chain tip = PRED_TIP
    held_macreseed_pending(&pool, 0x01, doc, fscl).await;
    // OPSEED != PRED_TIP → the operator seed does not match the expected chain tip.
    let err = complete(&pool, 0x01, OperatorResolution::MacReseed { seed: OPSEED })
        .await
        .expect_err("a MacReseed seed != the chain tip must be refused");
    assert!(
        is_err(&err, |e| matches!(
            e,
            CompletionError::MacReseedSeedMismatch
        )),
        "expected MacReseedSeedMismatch, got {err:?}"
    );
    // Nothing mutated — seed still the pinned tip, doc still SENDING, reservation PENDING under STOP.
    assert_eq!(
        read_seed(&pool, fscl).await.as_deref(),
        Some(&PRED_TIP[..]),
        "seed unchanged (still the chain tip)"
    );
    assert_eq!(read_doc_state(&pool, 0x11).await, "SENDING");
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_pointer(&pool, fscl).await.is_some(), "pointer intact");
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
}

// ───────────────────── oc05 GOING_ONLINE with active drain ────────────────────

#[tokio::test]
async fn oc05_mode_going_online_when_offline_session_active() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000005";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    // An active OPEN offline session must finish draining → mode target GOING_ONLINE.
    sqlx::query(
        "INSERT INTO offline_sessions (offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'OPEN', '2026-07-19T00:00:00Z')",
    )
    .bind(vec![0xC0u8; 16])
    .bind(fscl)
    .execute(&pool)
    .await
    .expect("seed active offline session");
    let r = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000555555".into(),
        },
    )
    .await
    .expect("completes with active drain");
    assert_eq!(r.mode_target, ModeTarget::GoingOnline);
    assert_eq!(read_mode(&pool, fscl).await, "GOING_ONLINE");
}

// ───────────────── oc06 offline not-accepted refused (cohort) ─────────────────

#[tokio::test]
async fn oc06_offline_not_accepted_refused_nothing_mutated() {
    // NOTE: name retained (additions-only inventory gate forbids a rename). gap 4b: a PLAIN
    // NotAccepted (the ONLINE variant) on an OFFLINE document is an origin mismatch — the offline
    // path is NotAcceptedOffline (the DB origin is the sole authority). Fail-closed, nothing mutated.
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000006";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", Some(500)).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let err = complete(&pool, 0x01, OperatorResolution::NotAccepted)
        .await
        .expect_err("plain NotAccepted on an offline document is an origin mismatch");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::OriginMismatch
    )));
    // Nothing mutated — still PENDING under STOP, doc still SENDING.
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
    assert_eq!(read_doc_state(&pool, 0x11).await, "SENDING");
}

// ───────── b1 reset-stop-mode refuses while a PENDING_APPLY reservation is unresolved ─────────

/// B1 re-audit (CRITICAL): `reset_stop_mode` MUST refuse to flip `STOP_MODE → GOING_ONLINE` while an
/// `OUTCOME_OBSERVED + PENDING_APPLY` reservation is unresolved. A blind flip would make
/// `complete_operator_pending`'s `WHERE mode='STOP_MODE'` CAS match 0 rows forever → the FN is
/// PERMANENTLY BRICKED (fence blocks all sells, seed never advances vs an accepted DPS receipt).
/// The guard converts that silent brick into a loud, actionable refusal, preserving STOP_MODE so the
/// completion path stays reachable.
#[tokio::test]
async fn b1_reset_stop_mode_refuses_while_pending_apply_unresolved() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000091";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", Some(500)).await;
    held_pending(&pool, 0x01, doc, fscl).await; // OUTCOME_OBSERVED + PENDING_APPLY + node STOP_MODE
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");

    // The guard refuses — NOT a blind flip.
    let err = prro::admin::reset_stop_mode(&pool, fscl, "DPS restored, resuming")
        .await
        .expect_err(
            "B1: reset-stop-mode must refuse while a PENDING_APPLY reservation is unresolved",
        );
    assert!(
        matches!(
            err,
            prro::admin::AdminError::PendingResolutionRequired { .. }
        ),
        "B1: expected PendingResolutionRequired, got {err:?}"
    );

    // The node is STILL STOP_MODE — the flip was refused, so completion's precondition survives.
    assert_eq!(
        read_mode(&pool, fscl).await,
        "STOP_MODE",
        "B1: a refused reset must leave the node in STOP_MODE (completion precondition preserved)"
    );
}

// ───────────────────── oc07 shift-family refused ─────────────────────────────

#[tokio::test]
async fn oc07_shift_family_refused() {
    // NOTE: name retained (additions-only inventory gate forbids a rename). gap 4b: an OFFLINE
    // shift-family document is now HANDLED (positive path lives in the operator-completion service,
    // svc19-21). The surviving CORE-level refusal exercised here is MacReseed on an OFFLINE document
    // → MacReseedNotOfflineDefined (§3.4 has no offline MacReseed cell). Fail-closed, nothing mutated.
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000007";
    let doc = seed_doc(&pool, fscl, 0x11, "SHIFT_OPEN", Some(0x11)).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let err = complete(
        &pool,
        0x01,
        OperatorResolution::MacReseed { seed: [0x99; 32] },
    )
    .await
    .expect_err("MacReseed on an offline document is not defined");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::MacReseedNotOfflineDefined
    )));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_sfn(&pool, 0x11).await.is_none());
}

// ───────────────────── oc08 stale authority refused ──────────────────────────

#[tokio::test]
async fn oc08_stale_generation_refused_nothing_mutated() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000008";
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    // Simulate an intervening fence advance past this reservation's generation.
    sqlx::query(
        "UPDATE node_state SET delivery_generation = delivery_generation + 3 WHERE fiscal_number=?",
    )
    .bind(fscl)
    .execute(&pool)
    .await
    .unwrap();
    let err = complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000888888".into(),
        },
    )
    .await
    .expect_err("stale generation refuses");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::StaleAuthority
    )));
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert!(read_sfn(&pool, 0x11).await.is_none());
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
}

// ───────────────────── oc09 pointer lifecycle → next doc ──────────────────────

#[tokio::test]
async fn oc09_completion_clears_pointer_and_next_doc_authorizes() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000009";
    let doc_a = seed_doc(&pool, fscl, 0x11, "SELL", None).await;
    let doc_b = seed_doc(&pool, fscl, 0x22, "SELL", None).await;
    held_pending(&pool, 0x01, doc_a, fscl).await;
    complete(
        &pool,
        0x01,
        OperatorResolution::Accepted {
            fiscal_number: "4000999999".into(),
        },
    )
    .await
    .expect("A completes");
    assert!(read_pointer(&pool, fscl).await.is_none());
    // The next legitimate document authorizes (fence released, pointer clear).
    let row = new_res(0x02, doc_b, fscl);
    with_immediate(&pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::authorize_submission(tx, row, TS)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("next document authorizes after operator completion");
}

// ═══════════ B1 part 2 (§4.1) — the ADMIN resolution surface ═══════════
//
// The B1 guard makes `reset_stop_mode` REFUSE while a reservation is PENDING_APPLY (commit
// `98a356f`), pointing the operator at `resolve-operator-pending`. Until now that command did not
// exist, so an operator had NO path to clear a PENDING_APPLY brick. These teeth drive the admin
// surface `prro::admin::resolve_operator_pending` end-to-end.

#[tokio::test]
async fn oc10_admin_resolve_operator_pending_accepted_releases_node() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000010";
    let doc = seed_doc(&pool, fscl, 0x1A, "SELL", None).await;
    held_pending(&pool, 0x0A, doc, fscl).await;
    assert_eq!(read_mode(&pool, fscl).await, "STOP_MODE");
    assert_eq!(
        read_apply_state(&pool, 0x0A).await.as_deref(),
        Some("PENDING_APPLY"),
        "precondition: reservation held PENDING_APPLY"
    );

    // Drive the ADMIN surface end-to-end (this is what §4.1 adds).
    let outcome = prro::admin::resolve_operator_pending(
        &pool,
        fscl,
        [0x0A; 16],
        OperatorResolution::Accepted {
            fiscal_number: "4000222222".into(),
        },
    )
    .await
    .expect("admin resolve completes");

    // The doc is issued, the reservation applied, the pointer cleared, the node released — the
    // exact brick the B1 guard now blocks until this command runs.
    assert!(outcome.applied, "resolution applied");
    assert_eq!(
        outcome.mode_target, "ONLINE",
        "no active offline session → ONLINE"
    );
    assert_eq!(
        read_doc_state(&pool, 0x1A).await,
        "SENT",
        "Accepted issues the doc"
    );
    assert_eq!(read_sfn(&pool, 0x1A).await.as_deref(), Some("4000222222"));
    assert_eq!(
        read_apply_state(&pool, 0x0A).await.as_deref(),
        Some("APPLIED"),
        "reservation APPLIED"
    );
    assert!(
        read_pointer(&pool, fscl).await.is_none(),
        "active pointer cleared"
    );
    assert_eq!(
        read_mode(&pool, fscl).await,
        "ONLINE",
        "node released out of STOP_MODE"
    );
}

#[tokio::test]
async fn oc11_admin_resolve_rejects_reservation_of_a_different_fn_nothing_mutated() {
    // Safety cross-check: the operator's --fn must own the reservation. A typo that names the
    // WRONG FN (but a real reservation_id belonging to ANOTHER FN) must be refused — else the
    // admin surface would silently resolve a reservation the operator did not intend.
    let (_d, pool) = fresh_pool().await;
    let fscl_a = "5000000011";
    let fscl_b = "5000000012";
    let doc = seed_doc(&pool, fscl_a, 0x1B, "SELL", None).await;
    seed_doc(&pool, fscl_b, 0x2B, "SELL", None).await; // FN B exists but owns no held reservation
    held_pending(&pool, 0x0B, doc, fscl_a).await;

    // Operator names FN B but the reservation_id belongs to FN A → refuse.
    let err = prro::admin::resolve_operator_pending(
        &pool,
        fscl_b,
        [0x0B; 16],
        OperatorResolution::Accepted {
            fiscal_number: "4000333333".into(),
        },
    )
    .await
    .expect_err("mismatched FN must be refused");
    assert_eq!(
        err.exit_code(),
        64,
        "FN mismatch is operator command misuse (EX_USAGE)"
    );

    // Nothing mutated on FN A: still PENDING_APPLY, still STOP_MODE, pointer intact, doc SENDING.
    assert_eq!(
        read_apply_state(&pool, 0x0B).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert_eq!(read_mode(&pool, fscl_a).await, "STOP_MODE");
    assert!(read_pointer(&pool, fscl_a).await.is_some());
    assert_eq!(read_doc_state(&pool, 0x1B).await, "SENDING");
}

// ═══════════ B5 (§4.2) — completion is reachable on a BLOCKED node, but stays BLOCKED ═══════════
//
// An OFFLINE-origin `-11` (ERROR_OFFLINE_168 — over the 168h offline ceiling) rests SENDING +
// PENDING_APPLY with the node BLOCKED: `record_outcome`'s early halt sets BLOCKED (not STOP_MODE),
// and `apply_outcome` returns HeldNotAutoRelease for an offline-origin reject. Two guarantees:
//   (1) `resolve_operator_pending` must be REACHABLE — the completion mode-CAS was
//       `WHERE mode='STOP_MODE'`, so a BLOCKED node returned NodeNotStopMode and the held doc could
//       NEVER be cleared (the B5 brick);
//   (2) completing it must NOT re-enable the over-ceiling node — it STAYS BLOCKED. Blindly flipping
//       to ONLINE/GOING_ONLINE would let an over-ceiling FN issue offline again (INV-05 breach).

#[tokio::test]
async fn b5_resolve_on_blocked_node_completes_and_stays_blocked() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000024";
    let doc = seed_doc(&pool, fscl, 0x24, "SELL", Some(700)).await; // offline-origin (offline_fiscal_no set)
    held_pending(&pool, 0xA4, doc, fscl).await;
    // Simulate the -11 early halt: the node is BLOCKED, NOT STOP_MODE.
    sqlx::query("UPDATE node_state SET mode = 'BLOCKED' WHERE fiscal_number = ?")
        .bind(fscl)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(read_mode(&pool, fscl).await, "BLOCKED");

    let outcome = prro::admin::resolve_operator_pending(
        &pool,
        fscl,
        [0xA4; 16],
        OperatorResolution::Accepted {
            fiscal_number: "4000444444".into(),
        },
    )
    .await
    .expect("resolve MUST be reachable on a BLOCKED node (B5)");

    // The held doc is resolved and the fence pointer cleared...
    assert!(outcome.applied, "reservation applied");
    assert_eq!(
        read_doc_state(&pool, 0x24).await,
        "SENT",
        "offline Accepted issues the doc"
    );
    assert_eq!(
        read_apply_state(&pool, 0xA4).await.as_deref(),
        Some("APPLIED")
    );
    assert!(
        read_pointer(&pool, fscl).await.is_none(),
        "fence pointer cleared"
    );
    // ...but the over-ceiling node STAYS BLOCKED (offline is still disabled — not re-enabled).
    assert_eq!(
        outcome.mode_target, "BLOCKED",
        "B5: an over-ceiling node is NOT re-enabled by completion"
    );
    assert_eq!(
        read_mode(&pool, fscl).await,
        "BLOCKED",
        "B5: node remains BLOCKED after completing the held doc"
    );
}

// ═══════════ gap 4b — offline-cohort operator completion (P3 fork-safety) ═══════════

const SESSION: [u8; 16] = [0x5E; 16];
const TIP: [u8; 32] = [0xEE; 32]; // a wrong global chain tip the rewind must overwrite
const PREV: [u8; 32] = [0xBB; 32]; // the predecessor doc's OWN pinned previous_hash

async fn seed_offline_doc(
    pool: &SqlitePool,
    fscl: &str,
    b: u8,
    lnd: i64,
    state: &str,
    prev: Option<[u8; 32]>,
) {
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, offline_fiscal_no, offline_session_id, \
            previous_hash) \
         VALUES (?, ?, ?, ?, 'SELL', ?, 'b1', 't1', 'OFFLINE', '2026-07-17T12:34:56Z', '{}', \
            ?, ?, ?, ?, ?)",
    )
    .bind(vec![b; 16])
    .bind(vec![b ^ 0xFF; 16])
    .bind(fscl)
    .bind(lnd)
    .bind(state)
    .bind(vec![0u8; 32])
    .bind(&[0x77u8; 32][..])
    .bind(lnd)
    .bind(&SESSION[..])
    .bind(prev.map(|h| h.to_vec()))
    .execute(pool)
    .await
    .expect("seed offline doc");
}

/// Seed FN + node_state(last_known=TIP) + a DRAINING offline session + a predecessor doc (SENDING,
/// offline, `prev`) at `pred_lnd`, plus later successors `(byte, lnd, state)`. Returns the pred id.
async fn seed_cohort(
    pool: &SqlitePool,
    fscl: &str,
    pred_byte: u8,
    pred_lnd: i64,
    prev: Option<[u8; 32]>,
    succ: &[(u8, i64, &str)],
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
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd, \
            last_known_unsigned_xml_sha256) VALUES (?, 'ONLINE', 'CREATED', 1, ?)",
    )
    .bind(fscl)
    .bind(&TIP[..])
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO offline_sessions(offline_session_id, fiscal_number, state, opened_at) \
         VALUES (?, ?, 'DRAINING', '2026-07-20T00:00:00Z')",
    )
    .bind(&SESSION[..])
    .bind(fscl)
    .execute(pool)
    .await
    .unwrap();
    seed_offline_doc(pool, fscl, pred_byte, pred_lnd, "SENDING", prev).await;
    for (b, lnd, st) in succ {
        seed_offline_doc(pool, fscl, *b, *lnd, st, None).await;
    }
    DocumentId::from_bytes([pred_byte; 16])
}

/// Assert an offline not-accepted completion is REFUSED with `pred`, and NOTHING mutated: the
/// predecessor stays SENDING, the reservation PENDING_APPLY, node STOP_MODE + seed still TIP, and
/// each named successor keeps its state.
async fn assert_cohort_refused(
    pool: &SqlitePool,
    fscl: &str,
    pred: u8,
    succ: &[(u8, &str)],
    pred_err: impl Fn(&CompletionError) -> bool,
) {
    let err = complete(pool, 0x01, OperatorResolution::NotAcceptedOffline)
        .await
        .expect_err("offline not-accepted is refused");
    assert!(is_err(&err, pred_err), "unexpected error: {err:?}");
    assert_eq!(
        read_doc_state(pool, pred).await,
        "SENDING",
        "pred unchanged"
    );
    assert_eq!(
        read_apply_state(pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
    assert_eq!(read_mode(pool, fscl).await, "STOP_MODE");
    assert_eq!(
        read_seed(pool, fscl).await.as_deref(),
        Some(&TIP[..]),
        "seed unchanged"
    );
    for (b, st) in succ {
        assert_eq!(read_doc_state(pool, *b).await, *st, "successor unchanged");
    }
}

// oc10 — happy: later OLA successors cancelled, seed rewound to the predecessor's previous_hash.
#[tokio::test]
async fn oc10_offline_not_accepted_cancels_cohort_and_rewinds_seed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000010";
    let pred = seed_cohort(
        &pool,
        fscl,
        0x10,
        10,
        Some(PREV),
        &[
            (0x11, 11, "OFFLINE_LOCAL_ACK"),
            (0x12, 12, "OFFLINE_LOCAL_ACK"),
        ],
    )
    .await;
    held_pending(&pool, 0x01, pred, fscl).await;
    let r = complete(&pool, 0x01, OperatorResolution::NotAcceptedOffline)
        .await
        .expect("offline not-accepted completes");
    assert_eq!(read_doc_state(&pool, 0x11).await, "CANCELLED");
    assert_eq!(read_doc_state(&pool, 0x12).await, "CANCELLED");
    // rewound to the predecessor's OWN previous_hash (NOT the wrong global TIP) — derive-not-trust.
    assert_eq!(read_seed(&pool, fscl).await.as_deref(), Some(&PREV[..]));
    assert_eq!(
        read_doc_state(&pool, 0x10).await,
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );
    assert_eq!(r.cancelled_cohort.len(), 2);
    assert_eq!(r.rewound_predecessor_seed, Some(Some(PREV)));
}

// oc11 — a later REJECTED successor is ISSUED (the exact rev1 break) → fail-closed, nothing mutated.
#[tokio::test]
async fn oc11_later_rejected_successor_is_fork_guarded() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000011";
    let pred = seed_cohort(&pool, fscl, 0x10, 10, Some(PREV), &[(0x11, 11, "REJECTED")]).await;
    held_pending(&pool, 0x01, pred, fscl).await;
    assert_cohort_refused(&pool, fscl, 0x10, &[(0x11, "REJECTED")], |e| {
        matches!(e, CompletionError::LaterSuccessorIssued(..))
    })
    .await;
}

// oc12 — a later SENDING successor is in-flight → fail-closed, nothing mutated.
#[tokio::test]
async fn oc12_later_sending_successor_is_in_flight_guarded() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000012";
    let pred = seed_cohort(&pool, fscl, 0x10, 10, Some(PREV), &[(0x11, 11, "SENDING")]).await;
    held_pending(&pool, 0x01, pred, fscl).await;
    assert_cohort_refused(&pool, fscl, 0x10, &[(0x11, "SENDING")], |e| {
        matches!(e, CompletionError::LaterSuccessorInFlight(_))
    })
    .await;
}

// oc13 — a later ERROR_RETRYABLE successor is ISSUED → fail-closed, no auto-redrive.
#[tokio::test]
async fn oc13_later_error_retryable_successor_is_issued_guarded() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000013";
    let pred = seed_cohort(
        &pool,
        fscl,
        0x10,
        10,
        Some(PREV),
        &[(0x11, 11, "ERROR_RETRYABLE")],
    )
    .await;
    held_pending(&pool, 0x01, pred, fscl).await;
    assert_cohort_refused(&pool, fscl, 0x10, &[(0x11, "ERROR_RETRYABLE")], |e| {
        matches!(e, CompletionError::LaterSuccessorIssued(..))
    })
    .await;
}

// oc14 — a later pre-OLA SIGNED successor is a structural breach → fail-closed.
#[tokio::test]
async fn oc14_later_signed_successor_is_invalid_state() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000014";
    let pred = seed_cohort(&pool, fscl, 0x10, 10, Some(PREV), &[(0x11, 11, "SIGNED")]).await;
    held_pending(&pool, 0x01, pred, fscl).await;
    assert_cohort_refused(&pool, fscl, 0x10, &[(0x11, "SIGNED")], |e| {
        matches!(e, CompletionError::LaterSuccessorInvalidState(..))
    })
    .await;
}

// oc15 — genesis: the predecessor chained off genesis (previous_hash NULL) → seed rewound to NULL.
#[tokio::test]
async fn oc15_genesis_predecessor_rewinds_seed_to_null() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000015";
    let pred = seed_cohort(
        &pool,
        fscl,
        0x10,
        10,
        None,
        &[(0x11, 11, "OFFLINE_LOCAL_ACK")],
    )
    .await;
    held_pending(&pool, 0x01, pred, fscl).await;
    let r = complete(&pool, 0x01, OperatorResolution::NotAcceptedOffline)
        .await
        .expect("genesis offline not-accepted completes");
    assert_eq!(read_doc_state(&pool, 0x11).await, "CANCELLED");
    assert!(
        read_seed(&pool, fscl).await.is_none(),
        "seed rewound to genesis NULL"
    );
    assert_eq!(r.rewound_predecessor_seed, Some(None));
}

// oc16 — NotAcceptedOffline on an ONLINE document → OriginMismatch, no seed overwrite.
#[tokio::test]
async fn oc16_not_accepted_offline_on_online_doc_is_origin_mismatch() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000016";
    // an ONLINE doc (offline_fiscal_no None) via the ordinary seed_doc.
    let doc = seed_doc(&pool, fscl, 0x10, "SELL", None).await;
    held_pending(&pool, 0x01, doc, fscl).await;
    let err = complete(&pool, 0x01, OperatorResolution::NotAcceptedOffline)
        .await
        .expect_err("NotAcceptedOffline on an online doc is an origin mismatch");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::OriginMismatch
    )));
    assert_eq!(read_doc_state(&pool, 0x10).await, "SENDING");
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("PENDING_APPLY")
    );
}

// oc22 — atomicity: a 0-row mode CAS AFTER the cancels rolls the WHOLE tx back (single envelope).
#[tokio::test]
async fn oc22_mode_cas_failure_rolls_back_cohort_and_seed() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5100000022";
    let pred = seed_cohort(
        &pool,
        fscl,
        0x10,
        10,
        Some(PREV),
        &[(0x11, 11, "OFFLINE_LOCAL_ACK")],
    )
    .await;
    held_pending(&pool, 0x01, pred, fscl).await;
    // Force the completion's mode check to fail by moving the node to a NON-halt mode (a racing
    // reset flipped it out of the held-halt). NB: BLOCKED is now a VALID completion mode (B5, §4.2),
    // so this uses ONLINE — neither STOP_MODE nor BLOCKED → the `_` arm → NodeNotStopMode.
    sqlx::query("UPDATE node_state SET mode = 'ONLINE' WHERE fiscal_number = ?")
        .bind(fscl)
        .execute(&pool)
        .await
        .unwrap();
    let err = complete(&pool, 0x01, OperatorResolution::NotAcceptedOffline)
        .await
        .expect_err("a non-halt mode fails the completion");
    assert!(is_err(&err, |e| matches!(
        e,
        CompletionError::NodeNotStopMode
    )));
    // The whole tx rolled back: the successor is NOT cancelled and the seed is NOT rewound.
    assert_eq!(read_doc_state(&pool, 0x11).await, "OFFLINE_LOCAL_ACK");
    assert_eq!(read_seed(&pool, fscl).await.as_deref(), Some(&TIP[..]));
    assert_eq!(read_doc_state(&pool, 0x10).await, "SENDING");
}
