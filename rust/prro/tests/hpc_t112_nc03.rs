//! bd PRRO_GATE-hpc — durable T=112 seed witness (NC-03 recovery + ordering).
//!
//! A standalone T=112 replenish advances the online MAC-chain seed to a
//! NON-DOCUMENT value `Hs = sha256(request_xml)`.  Before this slice, the shared
//! ledger-walk projection (`active_chain_tip_unsigned_xml_sha256`) could not recover
//! `Hs` after an NC-03 node_state loss — it recovered the pre-replenish issued-doc
//! hash `Hp` (or genesis `None`).  The durable `chain_seed_transitions` witness
//! (migration 040), folded into that ONE projection, closes the gap for all three
//! seed consumers (NC-03 boot, MacReseed guard-B, invariant_scan).
//!
//! These tests assert against the PUBLIC `active_chain_tip_unsigned_xml_sha256`
//! projection — that IS the seed source NC-03 `reconstruct_lost_node_state` and the
//! oracle both consume (per the design's §9 note and the mcc adjudication pattern).
//!
//! RED-first canaries (verified during implementation, restored after):
//!   - Test 1 (`nc03_pre_sell_recovers_hs`): neutralise the witness fold → returns
//!     `Hp` → RED.
//!   - Test 2 (`after_sell_recovers_hsell_not_stale_hs`): flip §4.2 `>` → `>=` →
//!     returns the stale `Hs` → RED.  This is the ordinal tie-break proof.

mod common;

use std::sync::Arc;

use prro::config::AppConfig;
use prro::db::invariant_scan::{scan, Violation};
use prro::db::models::enums::FiscalMode;
use prro::db::models::enums::{NodeMode, ShiftState};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::chain_seed_transitions;
use prro::db::repositories::delivery_reservation::{
    authorize_submission, complete_operator_pending, record_outcome, AttemptObservation,
    Authorization, CompletionError, CompletionResult, NewReservation, OperatorResolution,
};
use prro::db::repositories::fiscal_documents::active_chain_tip_unsigned_xml_sha256 as active_tip;
use prro::db::repositories::fiscal_number_config as fn_repo;
use prro::db::repositories::fiscal_number_config::NewFnConfig;
use prro::db::repositories::node_state;
use prro::db::tx::with_immediate;
use prro::db::types::DbDocumentId;
use prro::services::offline_sync::offline_code_replenish::OfflineCodeReplenishService;
use prro::transports::dps::dto::OfflineCodesResponse;
use prro::App;
use prro_domain::delivery::evidence::EvidenceDiscriminant;
use prro_domain::delivery::{
    classify, AuthorizedGeneration, DecodedResponseDigest, DpsProtocolBinding, DpsProtocolId,
    EnvelopeHash, NonOkStatusCode, ObservedOutcomeV1, PositiveGeneration, ProtocolContractVersion,
    SendOutcome, SendResponse, SubmissionEvidence,
};
use prro_domain::enums::DocType;
use sha2::{Digest, Sha256};

const TS: &str = "2026-07-24T00:00:00Z";

use common::det_signing_ctx;
use common::scripted_dps::ScriptedDps;

const FN: &str = "4000162280";
const TN: &str = "13667753";

// ─── Boot + seed helpers (modelled on offline_code_replenish.rs) ────────────────

async fn boot_app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir
        .path()
        .join("hpc.db")
        .display()
        .to_string()
        .replace('\\', "/");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#
    );
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.unwrap();
    (dir, app)
}

/// Seed `fiscal_number_config` + `node_state` (offline_enabled) with a chosen
/// `next_lnd` and optional prior seed.
async fn seed_fn(app: &App, next_lnd: i64, prior_seed: Option<[u8; 32]>) {
    let pool = app.db();
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
            tax_number: TN.into(),
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
        },
    )
    .await
    .unwrap();
    node_state::upsert_initial(pool, FN, NodeMode::Online, ShiftState::Opened, next_lnd)
        .await
        .unwrap();
    if let Some(seed) = prior_seed {
        node_state::seed_prevhash(pool, FN, &seed).await.unwrap();
    }
}

fn codes_resp(codes: &[&str]) -> OfflineCodesResponse {
    OfflineCodesResponse {
        codes: codes.iter().map(|s| s.to_string()).collect(),
    }
}

fn new_scripted() -> Arc<ScriptedDps> {
    use std::sync::atomic::AtomicUsize;
    Arc::new(ScriptedDps::new(
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    ))
}

/// Run the REAL replenish (advances seed → `Hs`, appends the witness at
/// `lnd_at_write = current next_lnd`).  Returns `Hs` (the 32-byte new seed).
async fn run_replenish(app: &App, di: u32) -> [u8; 32] {
    let stub = new_scripted();
    stub.push_ask_codes(Ok(codes_resp(&["code-X"])));
    let svc = OfflineCodeReplenishService::new(app.clone(), stub, Arc::new(det_signing_ctx()));
    let summary = svc
        .replenish(FN, TN, di, 1)
        .await
        .expect("replenish must succeed");
    let hs: [u8; 32] = Sha256::digest(summary.request_xml.as_bytes()).into();
    hs
}

/// Insert an ISSUED (offline OLA) fiscal_documents row for FN at `lnd`, chaining
/// `previous_hash` and carrying `unsigned_xml_sha256`.  `offline_fiscal_no` set →
/// `is_issued` is true (the doc contributes to the active chain tip).  Mirrors the
/// column list in `invariant_scan_chain_superseded.rs::seed_off`.
async fn seed_issued_offline_doc(
    pool: &sqlx::SqlitePool,
    lnd: i64,
    previous_hash: Option<[u8; 32]>,
    unsigned: [u8; 32],
) -> DocumentId {
    let doc_id = DocumentId::new();
    let request_id: [u8; 16] = *DocumentId::new().as_bytes();
    sqlx::query(
        "INSERT INTO fiscal_documents( \
            document_id, request_id, fiscal_number, lnd, doc_type, state, \
            backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, previous_hash, unsigned_xml_sha256, offline_fiscal_no) \
         VALUES (?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', 'b', 't', 'OFFLINE', \
            '2026-07-24T00:00:00Z', '{}', ?, ?, ?, ?)",
    )
    .bind(DbDocumentId(doc_id))
    .bind(&request_id[..])
    .bind(FN)
    .bind(lnd)
    .bind(&[0u8; 32][..]) // payload_sha256_canonical
    .bind(previous_hash.map(|h| h.to_vec()))
    .bind(&unsigned[..])
    .bind(lnd) // offline_fiscal_no → is_issued
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

// ────────────────────────────────────────────────────────────────────────────────
// Test 1 — NC-03 pre-SELL recovers Hs (the replenish→pre-SELL window).
// ────────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn nc03_pre_sell_recovers_hs() {
    // Pre-replenish issued doc Hp at lnd = k-1 = 4; next_lnd = k = 5.
    let (_dir, app) = boot_app().await;
    let hp = [0x11u8; 32];
    seed_fn(&app, 5, Some(hp)).await;
    seed_issued_offline_doc(app.db(), 4, None, hp).await;

    // Sanity: before replenish the tip is the doc Hp.
    assert_eq!(
        active_tip(app.db(), FN).await.unwrap().as_deref(),
        Some(&hp[..]),
        "pre-replenish tip must be the issued doc Hp"
    );

    // Real replenish → seed advances to Hs, witness appended at lnd_at_write = 5.
    let hs = run_replenish(&app, 1).await;
    assert_ne!(hs, hp, "Hs (non-doc seed) must differ from Hp");

    // NC-03: the projection (what reconstruct_lost_node_state consumes) recovers Hs.
    assert_eq!(
        active_tip(app.db(), FN).await.unwrap().as_deref(),
        Some(&hs[..]),
        "NC-03 pre-SELL must recover the durable T=112 witness Hs, NOT the stale doc Hp"
    );
}

// ────────────────────────────────────────────────────────────────────────────────
// Test 2 — after-SELL recovers Hsell, not the stale Hs (ORDINAL TIE-BREAK PROOF).
// ────────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn after_sell_recovers_hsell_not_stale_hs() {
    let (_dir, app) = boot_app().await;
    let hp = [0x11u8; 32];
    seed_fn(&app, 5, Some(hp)).await;
    seed_issued_offline_doc(app.db(), 4, None, hp).await;

    // Replenish: seed → Hs, witness lnd_at_write = 5.
    let hs = run_replenish(&app, 1).await;

    // An offline SELL then CONSUMES lnd 5, chains previous_hash = Hs, unsigned = Hsell.
    let hsell = [0x22u8; 32];
    seed_issued_offline_doc(app.db(), 5, Some(hs), hsell).await;

    // §4.2 strict `>`: doc.ord (5) == witness.lnd_at_write (5) → DOC wins → Hsell.
    assert_eq!(
        active_tip(app.db(), FN).await.unwrap().as_deref(),
        Some(&hsell[..]),
        "after a SELL chained onto Hs, the tip is the SELL's Hsell — NOT the stale witness Hs \
         (the doc consumed the ordinal the witness merely reserved; strict `>` tie-break)"
    );
}

// ────────────────────────────────────────────────────────────────────────────────
// Test 3 — crash-atomicity: success leaves ALL THREE {codes, seed=Hs, witness}.
// ────────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn crash_atomicity_all_three_coexist_on_success() {
    let (_dir, app) = boot_app().await;
    let hp = [0x11u8; 32];
    seed_fn(&app, 5, Some(hp)).await;

    let hs = run_replenish(&app, 1).await;

    // (a) codes present.
    let n_codes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM offline_codes WHERE fiscal_number = ? AND dps_code IS NOT NULL",
    )
    .bind(FN)
    .fetch_one(app.db())
    .await
    .unwrap();
    assert_eq!(n_codes, 1, "codes must be present on success");

    // (b) node_state seed == Hs.
    let ns = node_state::get(app.db(), FN).await.unwrap().unwrap();
    assert_eq!(
        ns.last_known_unsigned_xml_sha256,
        Some(hs),
        "seed must be advanced to Hs on success"
    );

    // (c) witness row present with matching Hs and lnd_at_write = 5.
    let w = chain_seed_transitions::latest_seed_transition(app.db(), FN)
        .await
        .unwrap()
        .expect("witness row must exist on success");
    assert_eq!(w.0.as_slice(), &hs[..], "witness new_seed must equal Hs");
    assert_eq!(w.1, 5, "witness lnd_at_write must equal next_lnd at write");

    // All three share the ONE `with_immediate` envelope in offline_code_replenish.rs
    // (codes + seed advance + witness insert). There is no window where the seed
    // advanced but the witness did not (or vice versa) — atomicity by construction.
}

// ────────────────────────────────────────────────────────────────────────────────
// Test 4 — invariant_scan clean after a standalone replenish (no crash, no SELL).
// Locks the §1.1 live-window false-positive finding.
// ────────────────────────────────────────────────────────────────────────────────
#[tokio::test]
async fn invariant_scan_clean_after_standalone_replenish() {
    let (_dir, app) = boot_app().await;
    let hp = [0x11u8; 32];
    seed_fn(&app, 5, Some(hp)).await;
    seed_issued_offline_doc(app.db(), 4, None, hp).await;

    // Standalone replenish: node_seed → Hs; witness lnd_at_write = 5.
    let _hs = run_replenish(&app, 1).await;

    // The oracle must NOT flag a ChainSeedMismatch: node_seed (Hs) == active_tip (Hs,
    // via the witness fold). Before the fold this false-positived (walk → Hp).
    let violations = scan(app.db()).await.unwrap();
    assert!(
        !violations
            .iter()
            .any(|v| matches!(v, Violation::ChainSeedMismatch { .. })),
        "no ChainSeedMismatch after a standalone replenish (witness fold makes node_seed == \
         active_tip); got: {violations:#?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────────
// Test 5 — guard-B accepts a reseed to Hs / rejects Hp (defensive).
// A MacReseedPending hold on a replenished FN; the witness-fed active tip is Hs.
// ────────────────────────────────────────────────────────────────────────────────

/// Seed a SENDING doc a reservation can attach to (mirrors operator_completion.rs::seed_doc
/// column shape, but at a caller-chosen lnd and doc_byte).
async fn seed_sending_doc(pool: &sqlx::SqlitePool, doc_byte: u8, lnd: i64) -> DocumentId {
    let doc_bytes = vec![doc_byte; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-24T12:34:56Z', '{}', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![doc_byte ^ 0xFF; 16])
    .bind(FN)
    .bind(lnd)
    .bind(vec![0u8; 32])
    .bind(&[0x88u8; 32][..]) // its own unsigned hash — irrelevant to guard-B (still SENDING)
    .execute(pool)
    .await
    .expect("seed SENDING doc");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

// ── operator_completion.rs domain builders (verbatim shape) ──────────────────────
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
async fn authorize(
    pool: &sqlx::SqlitePool,
    res_byte: u8,
    doc: DocumentId,
    fscl: &str,
) -> Authorization {
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
/// Drive a fresh reservation to a REAL -12 BadHashPrev / MacReseedPending hold.
async fn held_macreseed_pending(
    pool: &sqlx::SqlitePool,
    res_byte: u8,
    doc: DocumentId,
    fscl: &str,
) {
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
async fn complete(
    pool: &sqlx::SqlitePool,
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
fn is_completion_err(err: &anyhow::Error, pred: impl Fn(&CompletionError) -> bool) -> bool {
    err.downcast_ref::<CompletionError>().is_some_and(pred)
}

#[tokio::test]
async fn guard_b_accepts_reseed_to_hs_rejects_hp() {
    let (_dir, app) = boot_app().await;
    let pool = app.db();
    let hp = [0x11u8; 32];
    // FN at next_lnd = 5, prior issued doc Hp at lnd 4.
    seed_fn(&app, 5, Some(hp)).await;
    seed_issued_offline_doc(pool, 4, None, hp).await;
    // Replenish → seed=Hs, witness lnd_at_write=5. Active tip is now Hs (witness-fed).
    let hs = run_replenish(&app, 1).await;

    // A held SENDING doc (lnd 6, above the replenish ordinal) + a real -12 MacReseedPending hold.
    let held_doc = seed_sending_doc(pool, 0xEE, 6).await;
    held_macreseed_pending(pool, 0x01, held_doc, FN).await;

    // Reject: seed = Hp (the stale resurrected doc hash) — must fail closed.
    let rej = complete(pool, 0x01, OperatorResolution::MacReseed { seed: hp }).await;
    assert!(
        rej.as_ref()
            .is_err_and(|e| is_completion_err(e, |c| matches!(c, CompletionError::MacReseedSeedMismatch))),
        "guard-B must reject a reseed to the stale Hp (active tip is the witness-fed Hs); got {rej:?}"
    );

    // Accept: seed = Hs (the witness-fed active tip) — must be ACCEPTED.
    let acc = complete(pool, 0x01, OperatorResolution::MacReseed { seed: hs })
        .await
        .expect("guard-B must accept a reseed to the witness-fed active tip Hs");
    assert!(
        acc.applied,
        "MacReseed to the correct witness-fed tip Hs must apply"
    );
}

/// PROBE (determinism, FORCED TIE): two witness rows sharing BOTH `lnd_at_write` AND `created_at`
/// (reachable: replenish allocates no lnd, and `datetime('now')` is second-granular, so two
/// replenishes inside one second tie on both ordering keys). The projection must recover the LATEST
/// witness — the seed `node_state` actually holds — not an arbitrary tied row.
#[tokio::test]
async fn tied_witnesses_same_second_recover_the_latest_seed() {
    let (_dir, app) = boot_app().await;
    let hp = [0x11u8; 32];
    seed_fn(&app, 5, Some(hp)).await;
    seed_issued_offline_doc(app.db(), 4, None, hp).await;

    let a = [0xAAu8; 32];
    let b = [0xBBu8; 32];
    for seed in [&a, &b] {
        sqlx::query(
            "INSERT INTO chain_seed_transitions(fiscal_number, lnd_at_write, new_seed, source, \
                created_at) VALUES (?, 5, ?, 'T112', '2026-07-25T10:00:00')",
        )
        .bind(FN)
        .bind(&seed[..])
        .execute(app.db())
        .await
        .unwrap();
    }
    // node_state holds the LATER of the two (B) — the live truth the projection must reproduce.
    node_state::seed_prevhash(app.db(), FN, &b).await.unwrap();

    assert_eq!(
        active_tip(app.db(), FN).await.unwrap().as_deref(),
        Some(&b[..]),
        "tied witnesses (same lnd_at_write AND created_at): the projection must recover the LATEST \
         appended witness == node_state, not an arbitrary tied row"
    );
}

/// Test 3b — ROLLBACK proof (design §5 crash-before-commit). The seed advance and the witness insert
/// share ONE `with_immediate` envelope; an error anywhere inside it must roll BOTH back, leaving no
/// window where `node_state.seed = Hs` but no witness row exists (or vice versa).
#[tokio::test]
async fn witness_and_seed_roll_back_together_on_tx_error() {
    let (_dir, app) = boot_app().await;
    let hp = [0x11u8; 32];
    seed_fn(&app, 5, Some(hp)).await;
    let hs = [0x99u8; 32];

    let res: Result<(), anyhow::Error> = with_immediate(app.db(), move |tx| {
        Box::pin(async move {
            node_state::update_last_known_xml_sha_tx(tx, FN, &hs)
                .await
                .map_err(anyhow::Error::from)?;
            chain_seed_transitions::insert_seed_transition_tx(tx, FN, 5, &hs, "T112")
                .await
                .map_err(anyhow::Error::from)?;
            Err(anyhow::anyhow!("injected failure before commit"))
        })
    })
    .await;
    assert!(res.is_err(), "the injected failure must abort the envelope");

    let ns = node_state::get(app.db(), FN).await.unwrap().unwrap();
    assert_eq!(
        ns.last_known_unsigned_xml_sha256,
        Some(hp),
        "seed must roll back to Hp — no half-applied advance"
    );
    assert!(
        chain_seed_transitions::latest_seed_transition(app.db(), FN)
            .await
            .unwrap()
            .is_none(),
        "witness must roll back with the seed — no orphan witness row"
    );
}

/// PROBE — scan walk vs a doc that legitimately chains onto the T=112 witness seed. After a
/// replenish the NEXT document's `previous_hash` is `Hs` (a non-doc seed). The MAC-walk's running
/// `expected` was left at the prior doc's `Hp`, so it may emit a FALSE `ChainBreak` at that doc —
/// the same class as the bd-2nk marker re-anchor.
#[tokio::test]
async fn scan_clean_when_a_doc_chains_onto_the_t112_witness() {
    let (_dir, app) = boot_app().await;
    let hp = [0x11u8; 32];
    seed_fn(&app, 5, Some(hp)).await;
    seed_issued_offline_doc(app.db(), 4, None, hp).await;

    let hs = run_replenish(&app, 1).await;
    // The next SELL consumes lnd 5 and chains onto the replenish seed Hs.
    let hsell = [0x22u8; 32];
    seed_issued_offline_doc(app.db(), 5, Some(hs), hsell).await;
    node_state::seed_prevhash(app.db(), FN, &hsell)
        .await
        .unwrap();

    let v = scan(app.db()).await.unwrap();
    assert!(
        !v.iter().any(|x| matches!(x, Violation::ChainBreak { .. })),
        "a doc chaining onto the T=112 witness seed is a LEGITIMATE link — no ChainBreak; got {v:#?}"
    );
}
