//! bd `PRRO_GATE-6bj` step 1 — the held-transient evidence probe.
//!
//! The property under test is mostly about what does NOT happen. A held `-3` rests `SENDING` under a
//! `PENDING_APPLY` reservation with the node in `STOP_MODE`, and S7-1 R6 removed auto-redrive on
//! purpose: a document past `CALL_STARTED` must never be blindly re-sent. So this probe is allowed
//! to ask (`last_chk`) and to write an audit row, and nothing else — no document transition, no
//! `send_chk`, no mode change.
//!
//! Each test therefore asserts the verdict AND the non-effects, because a future edit that starts
//! resolving holds here would keep the verdict assertions green.

mod common;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use prro::config::AppConfig;
use prro::db::models::enums::{FiscalMode, NodeMode, ShiftState};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    authorize_submission, record_outcome, AttemptObservation, NewReservation,
};
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::repositories::node_state;
use prro::db::tx::with_immediate;
use prro::services::reconciliation::held_evidence_probe::{self, HeldVerdict, HELD_PROBE_EVENT};
use prro::services::reconciliation::runtime::RuntimeView;
use prro::transports::dps::dto::{CheckAck, CheckSignBlob};
use prro::transports::dps::error::DpsError;
use prro::App;
use prro_domain::delivery::evidence::EvidenceDiscriminant;
use prro_domain::delivery::{
    classify, AuthorizedGeneration, DecodedResponseDigest, DpsProtocolBinding, DpsProtocolId,
    EnvelopeHash, NonOkStatusCode, ObservedOutcomeV1, PositiveGeneration, ProtocolContractVersion,
    SendOutcome, SendResponse, SubmissionEvidence,
};
use prro_domain::enums::DocType;

use common::scripted_dps::ScriptedDps;

const FN: &str = "4000162280";
const TN: &str = "13667753";
const RESERVATION: [u8; 16] = [0x44; 16];
const DOC: [u8; 16] = [0x33; 16];

async fn boot_app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir
        .path()
        .join("6bj.db")
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

async fn seed_fn(app: &App) {
    fn_repo::insert(
        app.db(),
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
    node_state::upsert_initial(app.db(), FN, NodeMode::Online, ShiftState::Opened, 1)
        .await
        .unwrap();
}

/// Seed the exact shape a real DPS non-OK reply leaves behind, THROUGH THE PRODUCTION BOUNDARY:
/// `authorize_submission` then `record_outcome`, so the reservation satisfies the evidence-union
/// trigger by construction. A raw `UPDATE` cannot do this — the evidence-immutability trigger
/// rejects it ("OUTCOME_OBSERVED requires an evidence leaf"), which is the schema refusing to let a
/// test invent a state production cannot reach.
async fn seed_held(pool: &sqlx::SqlitePool, code: i32) {
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 7, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-08-14T12:00:00Z', '{}', ?)",
    )
    .bind(&DOC[..])
    .bind(&[0xCCu8; 16][..])
    .bind(FN)
    .bind(&[0u8; 32][..])
    .execute(pool)
    .await
    .expect("seed SENDING doc");

    let auth = with_immediate(pool, move |tx| {
        Box::pin(async move {
            authorize_submission(
                tx,
                NewReservation {
                    reservation_id: RESERVATION,
                    document_id: DocumentId::from_bytes(DOC),
                    fiscal_number: FN.to_string(),
                    dps_protocol_id: "FSCO_ZZD".to_string(),
                    protocol_contract_version: 1,
                    capability_profile_version: None,
                    endpoint_config_revision: None,
                    envelope_hash: [0xAB; 32],
                },
                "2026-08-14T12:00:01Z",
            )
            .await
            .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("authorize");

    let evidence = SubmissionEvidence::Started {
        response: SendResponse::parsed(SendOutcome::from_server_code(
            NonOkStatusCode::from_transport(code).expect("a non-OK status code"),
            DocType::Sell,
            DecodedResponseDigest::from_transport_digest([0xAB; 32]),
        )),
        binding: DpsProtocolBinding {
            protocol_id: DpsProtocolId::FscoZzd,
            contract_version: ProtocolContractVersion(1),
            capability_profile_version: None,
            endpoint_config_revision: None,
        },
        envelope_hash: EnvelopeHash([0u8; 32]),
    };
    let classified = classify(&evidence);
    let disc = EvidenceDiscriminant::from_evidence(&evidence);
    let outcome = ObservedOutcomeV1::record(
        &classified,
        None,
        AuthorizedGeneration::Started(PositiveGeneration::new(1).unwrap()),
    )
    .expect("observed-outcome mint");
    let obs = AttemptObservation::from_authorization(auth, evidence);
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            record_outcome(tx, &obs, &outcome, &disc)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .expect("record the held outcome");
}

fn new_stub() -> Arc<ScriptedDps> {
    Arc::new(ScriptedDps::new(
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    ))
}

async fn run(app: &App, stub: &Arc<ScriptedDps>) -> Option<held_evidence_probe::HeldEvidence> {
    let fn_sign = CheckSignBlob(vec![0xAB, 0xCD]);
    let signing_ctx = common::det_signing_ctx();
    let view = RuntimeView {
        dps: stub.as_ref(),
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    };
    held_evidence_probe::run_tick_for_fn(app.db(), &view, FN)
        .await
        .expect("probe tick")
}

async fn audit_rows(pool: &sqlx::SqlitePool) -> Vec<(String, Option<String>, Option<String>)> {
    sqlx::query_as(
        "SELECT event_type, actor, event_payload_json FROM audit_log WHERE event_type = ?",
    )
    .bind(HELD_PROBE_EVENT)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn doc_state(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(&DOC[..])
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn node_mode(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar("SELECT mode FROM node_state WHERE fiscal_number = ?")
        .bind(FN)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Nothing held ⇒ ZERO wire calls. The SELECT-first shape is the difference between a probe that is
/// free when there is nothing to ask and one that hammers DPS on every tick of every FN.
#[tokio::test]
async fn no_held_reservation_makes_no_wire_call() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app).await;

    let stub = new_stub();
    let out = run(&app, &stub).await;

    assert!(out.is_none(), "nothing to probe, so no evidence: {out:?}");
    assert!(
        stub.calls().is_empty(),
        "a tick with no held reservation must not touch the wire at all, got {:?}",
        stub.calls()
    );
    assert!(
        audit_rows(app.db()).await.is_empty(),
        "and it must not write an audit row either — a probe that logs nothing-happened on every \
         tick buries the rows that matter"
    );
}

/// DPS reports no last check at all: strong evidence our document never landed. Recorded, and the
/// hold is left exactly as it was.
#[tokio::test]
async fn peer_with_no_checks_is_recorded_and_the_hold_is_untouched() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app).await;
    seed_held(app.db(), -3).await;

    let stub = new_stub();
    stub.push_last(Err(DpsError::NotFound));

    let evidence = run(&app, &stub).await.expect("a held transient was probed");
    assert_eq!(evidence.verdict, HeldVerdict::PeerHasNoChecks);
    assert_eq!(
        evidence.lnd, 7,
        "the audit row must be readable without a join"
    );

    let rows = audit_rows(app.db()).await;
    assert_eq!(rows.len(), 1, "one evidence row for the first verdict");
    let (_, actor, payload) = &rows[0];
    assert_eq!(
        actor.as_deref(),
        Some("system:held-evidence-probe"),
        "attribution must show the gateway asked, not a person — the same forensic reason the \
         force seams record an actor"
    );
    assert!(
        payload.as_deref().unwrap_or("").contains("PeerHasNoChecks"),
        "the verdict belongs in the payload, got {payload:?}"
    );

    // The non-effects, which are the actual contract.
    assert_eq!(
        doc_state(app.db()).await,
        "SENDING",
        "the probe must not transition the document — S7-1 R6 removed auto-resolution deliberately"
    );
    assert_eq!(
        node_mode(app.db()).await,
        "STOP_MODE",
        "and it must not un-stop the node: it replaces the operator's guess with a fact, it does \
         not take the decision"
    );
    assert!(
        stub.calls()
            .iter()
            .all(|c| !matches!(c, common::scripted_dps::DpsCall::SendChk(_))),
        "NO send_chk, ever: the document is past CALL_STARTED and a re-send is exactly what R6 \
         removed. got {:?}",
        stub.calls()
    );
}

/// DPS holds SOME check. That is not evidence about ours either way — the id is recorded so an
/// operator can look it up, and the verdict says plainly that it is inconclusive.
#[tokio::test]
async fn peer_holding_another_check_is_recorded_as_inconclusive() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app).await;
    seed_held(app.db(), -3).await;

    let stub = new_stub();
    stub.push_last(Ok(CheckAck {
        id: "DPS-FN-9999".to_string(),
        id_sign: vec![],
        data_sign: vec![0xDE; 64],
    }));

    let evidence = run(&app, &stub).await.expect("a held transient was probed");
    assert_eq!(
        evidence.verdict,
        HeldVerdict::PeerHoldsOther {
            actual_id: "DPS-FN-9999".to_string()
        },
        "with no expected id to compare against, ownership CANNOT be proven here — recording the \
         id an operator can check is the honest ceiling"
    );
    assert_eq!(doc_state(app.db()).await, "SENDING");
    assert_eq!(node_mode(app.db()).await, "STOP_MODE");
}

/// A failed probe learns nothing, says so, and changes nothing. It must not be mistaken for
/// "DPS does not have it".
#[tokio::test]
async fn a_failed_probe_is_indeterminate_not_evidence_of_absence() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app).await;
    seed_held(app.db(), -3).await;

    let stub = new_stub();
    stub.push_last(Err(DpsError::Transport("connection reset".into())));

    let evidence = run(&app, &stub).await.expect("a held transient was probed");
    assert!(
        matches!(evidence.verdict, HeldVerdict::Indeterminate { .. }),
        "a transport failure is not an answer, got {:?}",
        evidence.verdict
    );
    assert_eq!(doc_state(app.db()).await, "SENDING");
    assert_eq!(node_mode(app.db()).await, "STOP_MODE");
}

/// Record CHANGES of knowledge, not heartbeats. A hold rests for hours and the tick runs every five
/// minutes; a row per tick saying the same thing would bury the rows that matter. The contract:
/// same (reservation, verdict) as the LAST row ⇒ probe still runs, nothing is written; a verdict
/// TRANSITION is always written — including back to a verdict seen earlier.
#[tokio::test]
async fn repeat_verdicts_are_not_re_recorded_but_transitions_are() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app).await;
    seed_held(app.db(), -3).await;

    // Twice the same answer: one row.
    let stub = new_stub();
    stub.push_last(Err(DpsError::NotFound));
    let _ = run(&app, &stub).await.expect("probed");
    let stub2 = new_stub();
    stub2.push_last(Err(DpsError::NotFound));
    let e2 = run(&app, &stub2).await.expect("probed again");
    assert_eq!(
        e2.verdict,
        HeldVerdict::PeerHasNoChecks,
        "the probe itself still runs on every tick — only the RECORDING is deduplicated"
    );
    assert_eq!(
        audit_rows(app.db()).await.len(),
        1,
        "an unchanged verdict must not be re-recorded"
    );

    // The answer changes: a second row.
    let stub3 = new_stub();
    stub3.push_last(Err(DpsError::Transport("outage".into())));
    let _ = run(&app, &stub3).await.expect("probed");
    assert_eq!(
        audit_rows(app.db()).await.len(),
        2,
        "a verdict TRANSITION is knowledge and must be recorded"
    );

    // And back again: a third row — dedup compares against the LAST row only, so the history of
    // transitions survives.
    let stub4 = new_stub();
    stub4.push_last(Err(DpsError::NotFound));
    let _ = run(&app, &stub4).await.expect("probed");
    assert_eq!(
        audit_rows(app.db()).await.len(),
        3,
        "returning to an earlier verdict is a transition too"
    );
}

/// Scope: only the TRANSIENT class. A `-12` (`MacReseedPending`) hold has its own resolution story —
/// the operator supplies a corrected seed — and this probe has nothing to say about it.
#[tokio::test]
async fn a_non_transient_hold_is_not_probed() {
    let (_dir, app) = boot_app().await;
    seed_fn(&app).await;
    seed_held(app.db(), -12).await;

    let stub = new_stub();
    stub.push_last(Err(DpsError::NotFound));

    let out = run(&app, &stub).await;
    assert!(
        out.is_none(),
        "a MacRecovery hold is out of scope for bd 6bj, got {out:?}"
    );
    assert!(
        stub.calls().is_empty(),
        "and it must not be probed at all — the narrowing lives in the SELECT, before the wire"
    );
}
