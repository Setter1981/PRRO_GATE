//! CS-3 Slice 2b — evidence matrix conformance.
//!
//! Proves the Rust half (Slice 2b: `EvidenceDiscriminant::from_evidence(...).to_columns()`
//! with `classify(...)` axes) and the SQL half (Slice 2a: migration 036's fail-closed
//! matrix) AGREE byte-for-byte: for every one of the 11 leaves, the Rust-serialized evidence
//! columns plus the classifier axes drive a real `delivery_reservation` row to
//! OUTCOME_OBSERVED and the migration matrix ACCEPTS it. A vocabulary drift between the two
//! would surface here as a rejected row.
//!
//! This is the seam test that keeps Slice 2b's `to_columns` and Slice 2a's trigger CASE from
//! diverging. It calls the REAL `classify` and `from_evidence` (no hand-copied tables).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use prro_domain::delivery::evidence::EvidenceDiscriminant;
use prro_domain::delivery::{
    classify, BoundedText, DecodedResponseDigest, DpsProtocolBinding, DpsProtocolId, EnvelopeHash,
    GrpcStatusDigest, NoResponseCause, NonEmptyFiscalNumber, NonOkStatusCode, PreflightRefusal,
    ProtocolContractVersion, RemoteStatusEvidence, SendOutcome, SendResponse, SubmissionEvidence,
};
use prro_domain::enums::DocType;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 036");
    (dir, pool)
}

async fn seed_doc(pool: &SqlitePool, fscl: &str, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
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
         VALUES (?, ?, ?, 1, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-17T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fscl)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn insert_res(pool: &SqlitePool, res_byte: u8, doc: DocumentId, fscl: &str) {
    let row = NewReservation {
        reservation_id: [res_byte; 16],
        document_id: doc,
        fiscal_number: fscl.to_string(),
        dps_protocol_id: "FSCO_ZZD".to_string(),
        protocol_contract_version: 1,
        capability_profile_version: None,
        endpoint_config_revision: None,
        envelope_hash: [0xAB; 32],
    };
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

// ── SubmissionEvidence construction (mirrors cs3_c_db + evidence.rs unit tests) ──

fn binding() -> DpsProtocolBinding {
    DpsProtocolBinding {
        protocol_id: DpsProtocolId::FscoZzd,
        contract_version: ProtocolContractVersion(1),
        capability_profile_version: None,
        endpoint_config_revision: None,
    }
}
fn hash() -> EnvelopeHash {
    EnvelopeHash([0u8; 32])
}
fn dig() -> DecodedResponseDigest {
    DecodedResponseDigest::from_transport_digest([0xAB; 32])
}
fn grpc() -> GrpcStatusDigest {
    GrpcStatusDigest::from_transport_digest([0xCD; 32])
}
fn started(response: SendResponse) -> SubmissionEvidence {
    SubmissionEvidence::Started {
        response,
        binding: binding(),
        envelope_hash: hash(),
    }
}
fn not_started(reason: PreflightRefusal) -> SubmissionEvidence {
    SubmissionEvidence::NotStarted {
        reason,
        binding: binding(),
        envelope_hash: hash(),
    }
}
fn from_code(code: i32, dt: DocType) -> SubmissionEvidence {
    started(SendResponse::parsed(SendOutcome::from_server_code(
        NonOkStatusCode::from_transport(code).unwrap(),
        dt,
        dig(),
    )))
}

fn all_leaves() -> Vec<(&'static str, SubmissionEvidence)> {
    vec![
        (
            "PreconditionFailed",
            not_started(PreflightRefusal::PreconditionFailed(
                BoundedText::from_truncating("g"),
            )),
        ),
        (
            "SigningFailed",
            not_started(PreflightRefusal::SigningFailed(
                BoundedText::from_truncating("s"),
            )),
        ),
        (
            "NoResponse",
            started(SendResponse::no_response(NoResponseCause::Timeout)),
        ),
        (
            "RemoteAuthStatus",
            started(SendResponse::remote_status(
                RemoteStatusEvidence::RemoteAuthStatus(grpc()),
            )),
        ),
        (
            "Accepted",
            started(SendResponse::parsed(SendOutcome::accepted(
                NonEmptyFiscalNumber::from_transport("4000123456".into()).unwrap(),
            ))),
        ),
        ("Rejected", from_code(-1, DocType::Sell)),
        ("UnknownStatus", from_code(-99, DocType::Sell)),
        ("SaveError", from_code(-3, DocType::Sell)),
        ("CloseAmbiguous", from_code(-2, DocType::ShiftClose)),
        (
            "MissingStatus",
            started(SendResponse::parsed(SendOutcome::missing_status(dig()))),
        ),
        (
            "OkButNoFiscalNumber",
            started(SendResponse::parsed(SendOutcome::ok_but_no_fiscal_number(
                dig(),
            ))),
        ),
    ]
}

/// For every leaf: the Rust `to_columns()` + `classify` axes are accepted by the migration
/// 036 fail-closed matrix, and the stored `evidence_kind` reads back as the leaf name.
#[tokio::test]
async fn every_leaf_conforms_to_migration_036_matrix() {
    let (_d, pool) = fresh_pool().await;
    for (i, (name, ev)) in all_leaves().into_iter().enumerate() {
        let fscl = format!("79000000{i:02}");
        let res_byte = 0x40 + i as u8;
        let doc = seed_doc(&pool, &fscl, res_byte).await;
        insert_res(&pool, res_byte, doc, &fscl).await;

        let classified = classify(&ev);
        let cols = EvidenceDiscriminant::from_evidence(&ev).to_columns();
        assert_eq!(cols.kind, name, "kind_str vs table name for {name}");

        // Accepted mirrors its fiscal number into remote_correlation_id (matrix requires it);
        // every other leaf keeps it NULL.
        let rcid: Option<String> = if name == "Accepted" {
            cols.text.clone()
        } else {
            None
        };

        if classified.certainty().as_str() != "NOT_SUBMITTED" {
            mark_call_started(&pool, res_byte).await;
        }

        let res = sqlx::query(
            "UPDATE delivery_reservation \
             SET state='OUTCOME_OBSERVED', submission_certainty=?, response_provenance=?, \
                 routing_class=?, apply_state='PENDING_APPLY', node_effect=?, \
                 evidence_kind=?, evidence_text=?, evidence_code=?, evidence_digest=?, \
                 remote_correlation_id=? \
             WHERE reservation_id=?",
        )
        .bind(classified.certainty().as_str())
        .bind(classified.provenance().as_str())
        .bind(classified.routing().map(|r| r.as_str()))
        .bind(classified.node_effect().as_str())
        .bind(cols.kind)
        .bind(cols.text.clone())
        .bind(cols.code)
        .bind(cols.digest.map(|d| d.to_vec()))
        .bind(rcid)
        .bind(&[res_byte; 16][..])
        .execute(&pool)
        .await;

        res.unwrap_or_else(|e| {
            panic!("migration 036 matrix REJECTED the Rust-serialized leaf {name}: {e}")
        });

        let stored: String = sqlx::query_scalar(
            "SELECT evidence_kind FROM delivery_reservation WHERE reservation_id=?",
        )
        .bind(&[res_byte; 16][..])
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored, name, "stored evidence_kind for {name}");
    }
}
