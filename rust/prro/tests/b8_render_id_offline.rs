//! B8-3 RED pins: stage_send must render `CheckEnvelope.id_offline` from
//! `fiscal_documents.offline_dps_code` (not the integer ordinal string), and
//! a doc with NULL `offline_dps_code` must be refused fail-closed.
//!
//! Pin (i): `fetch_send_inputs_tx` must surface `offline_dps_code` field.
//!          Before B8-3: field absent on SendInputs → compile error → RED.
//!          After: field present = "OPAQUE-XYZ" → GREEN.
//!
//! Pin (ii): offline-origin doc with `offline_dps_code IS NULL` →
//!           stage_send refuses fail-closed with `OfflineDpsCodeMissing`.
//!           Before B8-3: no such error variant, no such check → RED.
//!           After B8-3: typed error `OfflineDpsCodeMissing` → GREEN.

use sqlx::SqlitePool;

use prro::db::models::ids::DocumentId;
use prro::services::write_path::stage_send::{self, StageSendError};
use prro::transports::dps::dto::CheckAck;

mod common;
use common::StubDpsChannel;

const FN: &str = "1234567890";

async fn fresh_pool(label: &str) -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join(format!("{label}.db")))
        .await
        .expect("open_pool");
    (dir, pool)
}

fn ok_ack(id: &str) -> CheckAck {
    CheckAck {
        id: id.into(),
        id_sign: vec![],
        data_sign: vec![0xAB],
    }
}

/// Seed an OFFLINE_LOCAL_ACK doc ready for drain.
async fn seed_ola_doc(
    pool: &SqlitePool,
    offline_fiscal_no: i64,
    offline_dps_code: Option<&str>,
) -> DocumentId {
    let doc_b = 0xA1u8;
    let shift_b = 0xB1u8;
    let shift_bytes = vec![shift_b; 16];
    let doc_bytes = vec![doc_b; 16];
    let req_bytes = vec![doc_b ^ 0xFF; 16];
    let unsigned = vec![doc_b; 32];

    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'OFFLINE', 0, 'csh')",
    )
    .bind(&shift_bytes)
    .bind(FN)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            payload_json, payload_sha256_canonical, unsigned_xml_sha256, signed_by_cashier_id, \
            offline_fiscal_no, offline_dps_code) \
         VALUES (?, ?, ?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', 'b1', 't1', 'OFFLINE', \
            '2026-07-08T10:00:00Z', '{}', ?, ?, 'csh', ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(FN)
    .bind(&shift_bytes)
    .bind(offline_fiscal_no)
    .bind([0u8; 32].as_slice())
    .bind(&unsigned)
    .bind(offline_fiscal_no)
    .bind(offline_dps_code)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO node_state(fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
            backend_profile_id, transport_profile_id, last_known_unsigned_xml_sha256) \
         VALUES (?, 'ONLINE', 'OPENED', ?, 1, 'b1', 't1', NULL)",
    )
    .bind(FN)
    .bind(&shift_bytes)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"FAKE-CMS-OLA".to_vec())
    .execute(pool)
    .await
    .unwrap();

    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

/// B8-3 pin (i): `fetch_send_inputs_tx` must surface `offline_dps_code` field
/// so the envelope builder can render it as `id_offline`.
///
/// Before B8-3: `SendInputs` has no `offline_dps_code` field → compile RED.
/// After: field present and equals "OPAQUE-XYZ" → GREEN.
#[tokio::test]
async fn send_inputs_surfaces_offline_dps_code() {
    let (_d, pool) = fresh_pool("b8_3_i").await;
    let doc = seed_ola_doc(&pool, 1, Some("OPAQUE-XYZ")).await;

    let inputs = prro::db::tx::with_immediate(&pool, |tx| {
        let d = doc;
        Box::pin(async move {
            prro::db::repositories::fiscal_documents::fetch_send_inputs_tx(tx, d)
                .await
                .map_err(anyhow::Error::from)
        })
    })
    .await
    .unwrap()
    .expect("inputs must be present");

    assert_eq!(
        inputs.offline_dps_code.as_deref(),
        Some("OPAQUE-XYZ"),
        "fetch_send_inputs_tx must surface offline_dps_code='OPAQUE-XYZ', got {:?}",
        inputs.offline_dps_code
    );
    // Regression: offline_fiscal_no must still work.
    assert_eq!(inputs.offline_fiscal_no, Some(1));
}

/// B8-3 pin (ii): offline-origin doc with `offline_dps_code IS NULL` →
/// stage_send must refuse fail-closed with `OfflineDpsCodeMissing`.
///
/// Before B8-3: no such error variant → compile RED.
/// After B8-3: `OfflineDpsCodeMissing` → GREEN.
#[tokio::test]
async fn offline_drain_null_dps_code_refuses_fail_closed() {
    let (_d, pool) = fresh_pool("b8_3_ii").await;
    let doc = seed_ola_doc(&pool, 1, None).await; // offline_dps_code IS NULL

    let stub = StubDpsChannel::new(Ok(ok_ack("MUST-NOT-SEND")));
    let err = stage_send::run(&pool, &stub, doc, None)
        .await
        .expect_err("must refuse fail-closed when offline_dps_code IS NULL");

    assert!(
        matches!(err, StageSendError::OfflineDpsCodeMissing { document_id } if document_id == doc),
        "expected OfflineDpsCodeMissing, got: {err:?}"
    );
}
