//! Targeted verification for `document_files::replace_tx` (W10.4
//! step 2).
//!
//! `replace_tx` is the canonical replace surface for MAC recovery —
//! `INSERT OR REPLACE` semantics: replaces an existing row in-place
//! OR INSERTs if none.  This file pins:
//!   1. Replace overwrites existing content + same PK survives.
//!   2. Replace on a missing row INSERTs (matches `INSERT OR REPLACE`
//!      contract; documented and intentional).
//!   3. Content round-trip: bytes go in, bytes come out unchanged
//!      (no encoding drift through the SQLite BLOB column).
//!
//! Anchored on freeze §4.4.2 + §4.4.4 step 2.

use prro::db::models::ids::DocumentId;
use prro::db::repositories::document_files::{self, DocumentFileKind};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs migrations");
    (dir, pool)
}

/// Seed FN config + a SIGNED fiscal_documents row so document_files'
/// FK to fiscal_documents is satisfied.
async fn seed_doc(pool: &SqlitePool, doc_byte: u8) -> DocumentId {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '12345678', 'test')",
    )
    .execute(pool)
    .await
    .unwrap();
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, '1234567890', ?, 'SELL', 'SIGNED', 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

async fn read_content(
    pool: &SqlitePool,
    doc: DocumentId,
    kind: DocumentFileKind,
) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT content FROM document_files WHERE document_id = ? AND kind = ?")
        .bind(doc)
        .bind(kind)
        .fetch_optional(pool)
        .await
        .expect("read document_files content")
}

#[tokio::test]
async fn replace_overwrites_existing_row_in_place() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x01).await;
    let original = b"original-signed-cms".to_vec();
    let recovered = b"recovered-signed-cms-with-new-mac".to_vec();

    // Initial INSERT via the existing helper.
    with_immediate(&pool, {
        let original = original.clone();
        move |tx| {
            Box::pin(async move {
                document_files::insert_tx(tx, doc, DocumentFileKind::SignedXml, &original).await?;
                Ok::<(), anyhow::Error>(())
            })
        }
    })
    .await
    .expect("seed initial SIGNED_XML");

    assert_eq!(
        read_content(&pool, doc, DocumentFileKind::SignedXml).await,
        Some(original)
    );

    // Replace.
    with_immediate(&pool, {
        let recovered = recovered.clone();
        move |tx| {
            Box::pin(async move {
                document_files::replace_tx(tx, doc, DocumentFileKind::SignedXml, &recovered)
                    .await?;
                Ok::<(), anyhow::Error>(())
            })
        }
    })
    .await
    .expect("replace_tx");

    assert_eq!(
        read_content(&pool, doc, DocumentFileKind::SignedXml).await,
        Some(recovered),
        "replace_tx must overwrite content with the new bytes"
    );

    // PK invariant: still exactly one row for (doc, SIGNED_XML).
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_files WHERE document_id = ? AND kind = 'SIGNED_XML'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        n, 1,
        "INSERT OR REPLACE must keep exactly one row for the PK"
    );
}

#[tokio::test]
async fn replace_inserts_when_no_existing_row() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x02).await;
    assert!(
        read_content(&pool, doc, DocumentFileKind::PayloadXml)
            .await
            .is_none(),
        "pre-condition: no PAYLOAD_XML row"
    );

    let content = b"first-payload-xml".to_vec();
    with_immediate(&pool, {
        let content = content.clone();
        move |tx| {
            Box::pin(async move {
                document_files::replace_tx(tx, doc, DocumentFileKind::PayloadXml, &content).await?;
                Ok::<(), anyhow::Error>(())
            })
        }
    })
    .await
    .expect("replace_tx as INSERT");

    assert_eq!(
        read_content(&pool, doc, DocumentFileKind::PayloadXml).await,
        Some(content),
        "replace_tx on a missing row must INSERT (INSERT OR REPLACE contract)"
    );
}

#[tokio::test]
async fn replace_fk_violation_on_missing_fiscal_document() {
    // R-W10.4-step2a-review LOW 2 close: replace_tx with a doc_id
    // that doesn't exist in fiscal_documents must surface the FK
    // violation via the wrapping tx (no silent ghost-row INSERT).
    // Pins the document_files FK contract against future schema
    // drift (e.g. accidental `ON DELETE SET NULL` change).
    let (_d, pool) = fresh_pool().await;
    let bogus = DocumentId::from_bytes([0xFFu8; 16]);

    let res = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            document_files::replace_tx(tx, bogus, DocumentFileKind::SignedXml, b"x").await?;
            Ok::<(), anyhow::Error>(())
        })
    })
    .await;
    let err = res.expect_err("FK violation must surface (no ghost INSERT)");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("foreign") || msg.contains("constraint"),
        "expected FK / constraint error, got: {msg}"
    );

    // Sanity: no row landed via the rolled-back attempt.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_files WHERE document_id = ?")
        .bind(bogus)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "rolled-back tx must not leave a partial row");
}

#[tokio::test]
async fn replace_round_trips_arbitrary_byte_content() {
    // Pin: SQLite BLOB does not transform bytes; replace_tx is bit-faithful.
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, 0x03).await;

    // Mix of 0x00, 0xFF, UTF-8 markers, embedded NUL, all 256 byte
    // values — the kind of payload a real CMS-signed XML carries.
    let content: Vec<u8> = (0..=255).chain([0xC3, 0xA0, 0x00, 0xFF, 0x7F]).collect();

    with_immediate(&pool, {
        let content = content.clone();
        move |tx| {
            Box::pin(async move {
                document_files::replace_tx(tx, doc, DocumentFileKind::SignedXml, &content).await?;
                Ok::<(), anyhow::Error>(())
            })
        }
    })
    .await
    .expect("replace_tx");

    let round_trip = read_content(&pool, doc, DocumentFileKind::SignedXml).await;
    assert_eq!(
        round_trip,
        Some(content),
        "BLOB column must round-trip arbitrary bytes (zero, high-byte, UTF-8 prefix)"
    );
}
