//! bd PRRO_GATE-mcc adjudication — is mcc SUPERSEDED by #338 + bd 2nk?
//!
//! mcc (pre-#338): "a MacReseed completion re-bases node_state.last_known_unsigned_xml_sha256 to the
//! operator's corrected seed, which is NOT ledger-derivable → an NC-03 boot (node_state lost, ledger
//! survives) cannot reconstruct it."
//!
//! Two post-mcc changes may supersede it:
//!   - #338 + bd 2nk MacReseed guard-B (delivery_reservation.rs): a valid MacReseed now REQUIRES
//!     `operator seed == active_chain_tip_unsigned_xml_sha256` (else `MacReseedSeedMismatch`). So a
//!     valid reseed value EQUALS the ledger-derivable active chain tip.
//!   - bd 2nk NC-03 boot (`reconstruct_lost_node_state`) projects the recovered seed via
//!     `fiscal_documents::active_chain_tip_unsigned_xml_sha256` — the exact fn this test calls.
//!
//! This is a NON-FROZEN, uncommitted adjudication file. It replicates the PUBLIC-API helpers from
//! `tests/operator_completion.rs` (copied verbatim, not shared) and adds ONE directed test.
//! `reconstruct_lost_node_state` is `pub(crate)` and NOT callable here; we assert on the public
//! `active_chain_tip_unsigned_xml_sha256`, which IS the exact projection that fn uses for the seed.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    authorize_submission, complete_operator_pending, record_outcome, AttemptObservation,
    Authorization, CompletionResult, NewReservation, OperatorResolution,
};
use prro::db::repositories::fiscal_documents::active_chain_tip_unsigned_xml_sha256;
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
/// The active chain tip pinned on the issued predecessor. A VALID MacReseed's operator seed MUST
/// equal this (guard-B), so the installed seed == this ledger-derivable tip. Distinct from any
/// default (not 0x00, not 0x77, not 0xEE) so equality assertions are non-vacuous.
const TIP: [u8; 32] = [0x55; 32];

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

// ── MacReseed hold fixtures (faithful `-12` hold + issued predecessor) ──────────

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
/// `unsigned_xml_sha256 = tip`, and pin `node_state.last_known_unsigned_xml_sha256 = tip`. This is
/// the active chain tip guard-B checks the operator seed against. `lnd` must be BELOW the held `-12`
/// doc's lnd so it is the last-issued row.
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

// ─────────────────────────── the mcc adjudication test ───────────────────────────

/// PROOF: a VALID MacReseed's installed seed IS the ledger-derivable active chain tip, and it
/// survives an NC-03 node_state loss. If GREEN, mcc is SUPERSEDED by #338 guard-B + bd 2nk.
#[tokio::test]
async fn mcc_valid_macreseed_seed_is_ledger_derivable_and_survives_nc03() {
    let (_d, pool) = fresh_pool().await;
    let fscl = "5000000199";

    // 1. Seed an FN with an ISSUED predecessor at a LOW lnd carrying unsigned_xml_sha256 = TIP.
    //    This is the active chain tip; node_state.last_known is pinned to TIP.
    let doc = seed_doc(&pool, fscl, 0x11, "SELL", None).await; // held -12 doc, lnd = 0x11 (17)
    seed_issued_predecessor(&pool, fscl, 0x10, 0x10, &TIP).await; // pred lnd = 16 < 17

    // 2. Seed a real -12 MacReseedPending hold; node → STOP_MODE.
    held_macreseed_pending(&pool, 0x01, doc, fscl).await;
    assert_eq!(
        read_mode(&pool, fscl).await,
        "STOP_MODE",
        "precondition: -12 MacReseedPending hold halts the node"
    );
    // Sanity: TIP really is the active chain tip BEFORE the reseed (issued predecessor).
    assert_eq!(
        active_chain_tip_unsigned_xml_sha256(&pool, fscl)
            .await
            .expect("tip query"),
        Some(TIP.to_vec()),
        "the issued predecessor's hash IS the ledger-derivable active chain tip"
    );

    // 3. Complete a VALID MacReseed with seed == TIP (== the active chain tip, so guard-B accepts).
    let r = complete(&pool, 0x01, OperatorResolution::MacReseed { seed: TIP })
        .await
        .expect("a valid MacReseed (seed == active chain tip) must be ACCEPTED");
    assert!(r.applied, "reseed applied");
    assert!(r.seed_advanced, "reseed advances the seed");
    assert_eq!(
        read_doc_state(&pool, 0x11).await,
        "REQUIRES_MANUAL_RECONCILIATION",
        "the held -12 doc escalates to RMR"
    );
    assert_eq!(
        read_apply_state(&pool, 0x01).await.as_deref(),
        Some("APPLIED")
    );

    // 4. The reseed installed the operator seed (== TIP) into node_state.
    assert_eq!(
        read_seed(&pool, fscl).await.as_deref(),
        Some(&TIP[..]),
        "reseed installed the operator seed"
    );

    // 5. The reseed value IS the ledger-derivable active chain tip (guard-B forced them equal).
    assert_eq!(
        active_chain_tip_unsigned_xml_sha256(&pool, fscl)
            .await
            .expect("tip query"),
        Some(TIP.to_vec()),
        "the installed reseed value EQUALS the ledger-derivable active chain tip"
    );

    // 6. NC-03 simulation: node_state is LOST, the ledger (fiscal_documents) SURVIVES.
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = ?")
        .bind(fscl)
        .execute(&pool)
        .await
        .expect("simulate NC-03 node_state loss");
    // node_state is gone...
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM node_state WHERE fiscal_number = ?")
            .bind(fscl)
            .fetch_one(&pool)
            .await
            .unwrap()
            == 0,
        "NC-03: node_state row deleted"
    );
    // ...but the exact seed value `reconstruct_lost_node_state` would re-install is STILL the
    // ledger-derived active chain tip, and it EQUALS the reseed value. THIS is the mcc-recovery
    // proof: the reseed survives node_state loss because it is ledger-derivable.
    assert_eq!(
        active_chain_tip_unsigned_xml_sha256(&pool, fscl)
            .await
            .expect("tip query after NC-03"),
        Some(TIP.to_vec()),
        "NC-03: the ledger-derived seed STILL equals the reseed — mcc SUPERSEDED"
    );

    // 7. Non-vacuity: TIP is a specific 32-byte value distinct from every default in the fixtures.
    assert_ne!(TIP, [0u8; 32], "TIP != all-zero default");
    assert_ne!(TIP, SEED, "TIP != the seed_doc default (0x77)");
}
