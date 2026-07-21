//! S7-1 — `submit_authorized`: the SOLE wire function (design §0/§2 Layer 3, spec4b §6).
//!
//! Consumes an `Authorization` by value, verifies the envelope-hash rebind guard + the full
//! 5-binding echo (AO-2) BEFORE any wire I/O, fires EXACTLY ONE `send_chk_observed`, and returns a
//! post-wire `AttemptObservation`. INACTIVE — no production caller wires it until the S7-1 cutover.
//!
//! Teeth (S7-P2-2, behavioural):
//! - `sa01` — a valid submit fires EXACTLY one wire and returns an `AttemptObservation` carrying the
//!   token's reservation identity + a `Started` evidence over the wire response;
//! - `sa02` — an envelope whose bytes do not hash to the token's `envelope_hash` is refused with
//!   ZERO wire calls (rebind guard);
//! - `sa03` — a port binding that differs from the token's binding is refused with ZERO wire calls
//!   (AO-2 echo).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{
    authorize_submission, Authorization, NewReservation,
};
use prro::db::tx::with_immediate;
use prro::services::write_path::submit::{submit_authorized, SubmitRefused};
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, DpsCheckType};
use prro_domain::delivery::{
    DpsProtocolBinding, DpsProtocolId, ProtocolContractVersion, SubmissionEvidence,
};
use prro_domain::enums::DocType;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

mod common;
use common::scripted_dps::ScriptedDps;

const FN: &str = "4000000001";
const TS: &str = "2026-07-20T00:00:00Z";
const CHECK_SIGN: &[u8] = b"cms-signed-envelope-blob";

fn envelope_hash() -> [u8; 32] {
    <Sha256 as Digest>::digest(CHECK_SIGN).into()
}

fn wire_envelope() -> CheckEnvelope {
    CheckEnvelope {
        rro_fn: FN.into(),
        date_time: 0,
        check_sign: CHECK_SIGN.to_vec(),
        local_number: 1,
        check_type: DpsCheckType::Chk,
        id_offline: String::new(),
        id_cancel: String::new(),
    }
}

fn ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![],
    }
}

fn port_binding() -> DpsProtocolBinding {
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// Seed fn_config + node_state + a SENDING doc, then authorize → CALL_STARTED with a token whose
/// `envelope_hash` matches [`CHECK_SIGN`] and whose binding matches [`port_binding`].
async fn seed_and_authorize(pool: &sqlx::SqlitePool) -> Authorization {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
    sqlx::query(
        "INSERT OR IGNORE INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES (?, 'ONLINE', 'CREATED', 1)",
    )
    .bind(FN)
    .execute(pool)
    .await
    .expect("seed node_state");
    let doc_bytes = vec![0x11u8; 16];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, 1, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(vec![0xEEu8; 16])
    .bind(FN)
    .bind(vec![0u8; 32])
    .execute(pool)
    .await
    .expect("seed fiscal_documents");

    let doc = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let row = NewReservation {
        reservation_id: [0x01; 16],
        document_id: doc,
        fiscal_number: FN.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: envelope_hash(),
    };
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

fn counting_channel() -> (ScriptedDps, Arc<AtomicUsize>) {
    let send_calls = Arc::new(AtomicUsize::new(0));
    let last_calls = Arc::new(AtomicUsize::new(0));
    let stub = ScriptedDps::new(send_calls.clone(), last_calls);
    (stub, send_calls)
}

#[tokio::test]
async fn sa01_valid_submit_fires_exactly_one_wire_and_returns_started_observation() {
    let (_d, pool) = fresh_pool().await;
    let auth = seed_and_authorize(&pool).await;
    let reservation_id = auth.reservation_id();
    let generation = auth.authorized_generation();

    let (channel, send_calls) = counting_channel();
    channel.push_send(Ok(ack("4000123456")));

    let obs = submit_authorized(
        &channel,
        &port_binding(),
        auth,
        wire_envelope(),
        DocType::Sell,
    )
    .await
    .expect("a valid authorization submits");

    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        1,
        "EXACTLY one send_chk wire call per submit"
    );
    // The observation carries the token's immutable identity + a Started evidence over the wire.
    assert_eq!(
        obs.reservation_id(),
        reservation_id,
        "obs keeps the reservation id"
    );
    assert_eq!(
        obs.authorized_generation(),
        generation,
        "obs keeps the generation"
    );
    assert!(
        matches!(obs.evidence(), SubmissionEvidence::Started { .. }),
        "post-`CALL_STARTED` evidence is always Started; got {:?}",
        obs.evidence()
    );
}

#[tokio::test]
async fn sa02_envelope_rebind_is_refused_with_zero_wire() {
    let (_d, pool) = fresh_pool().await;
    let auth = seed_and_authorize(&pool).await;

    let (channel, send_calls) = counting_channel();
    channel.push_send(Ok(ack("4000123456")));

    // A DIFFERENT check_sign → SHA256 mismatch vs the token's envelope_hash → refuse before wire.
    let mut tampered = wire_envelope();
    tampered.check_sign = b"a-different-cms-blob".to_vec();

    let err = submit_authorized(&channel, &port_binding(), auth, tampered, DocType::Sell)
        .await
        .expect_err("a rebound envelope must be refused");
    assert!(
        matches!(err, SubmitRefused::EnvelopeRebind),
        "expected EnvelopeRebind; got {err:?}"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "a rebind refusal performs ZERO wire I/O"
    );
}

#[tokio::test]
async fn sa03_binding_mismatch_is_refused_with_zero_wire() {
    let (_d, pool) = fresh_pool().await;
    let auth = seed_and_authorize(&pool).await;

    let (channel, send_calls) = counting_channel();
    channel.push_send(Ok(ack("4000123456")));

    // A port binding whose contract_version differs from the token's (1) → AO-2 echo fails.
    let wrong_binding = DpsProtocolBinding {
        contract_version: ProtocolContractVersion(2),
        ..port_binding()
    };

    let err = submit_authorized(
        &channel,
        &wrong_binding,
        auth,
        wire_envelope(),
        DocType::Sell,
    )
    .await
    .expect_err("a mismatched binding must be refused");
    assert!(
        matches!(err, SubmitRefused::BindingMismatch),
        "expected BindingMismatch; got {err:?}"
    );
    assert_eq!(
        send_calls.load(Ordering::SeqCst),
        0,
        "a binding refusal performs ZERO wire I/O"
    );
}
