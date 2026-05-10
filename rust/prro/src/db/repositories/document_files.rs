//! Repository for `document_files`.
//!
//! Per-doc derivative artefacts (PAYLOAD_XML, SIGNED_XML, KVT1_RAW,
//! KVT2_RAW, PAYLOAD_JSON_CANONICAL, RECEIPT_PDF).  Schema:
//! `rust/prro/migrations/002_fiscal_documents.sql:65-73` — PRIMARY KEY
//! `(document_id, kind)`, FK to `fiscal_documents` with ON DELETE
//! CASCADE, kind constrained to a fixed CHECK list.
//!
//! Repo policy:
//! - INSERT and SELECT runtime-bound (`sqlx::query`) — content is BLOB
//!   and `kind` is encoded via the typed `DocumentFileKind` enum.
//! - **Strictly typed kinds**: `DocumentFileKind` enumerates every
//!   schema-allowed value; no string callers.  Adding a kind requires
//!   schema migration AND a variant here.
//! - **Errors bubble up unmodified.**  PRIMARY KEY clashes (caller
//!   tried to INSERT the same `(doc_id, kind)` twice) and FK
//!   violations (doc_id missing) surface as `sqlx::Error`; the W6
//!   stage 3-PERSIST persist-rollback fixture relies on this.

use crate::db::models::ids::DocumentId;
use crate::db::tx::WriteTxConn;

/// Schema-CHECK-aligned set of artefact kinds.  Adding a variant
/// requires a matching migration to extend the
/// `document_files.kind` CHECK list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum DocumentFileKind {
    #[sqlx(rename = "PAYLOAD_XML")]
    PayloadXml,
    #[sqlx(rename = "SIGNED_XML")]
    SignedXml,
    #[sqlx(rename = "KVT1_RAW")]
    Kvt1Raw,
    #[sqlx(rename = "KVT2_RAW")]
    Kvt2Raw,
    #[sqlx(rename = "PAYLOAD_JSON_CANONICAL")]
    PayloadJsonCanonical,
    #[sqlx(rename = "RECEIPT_PDF")]
    ReceiptPdf,
}

/// Plain `INSERT` — no upsert, no `ON CONFLICT` clause.  PRIMARY KEY
/// `(document_id, kind)` makes a duplicate row a hard error
/// (`sqlx::Error` with UNIQUE / PK constraint message).  FK violation
/// (doc_id missing) is the same — hard error.
///
/// W6 stage 3-PERSIST is the canonical caller for `PayloadXml` +
/// `SignedXml` inside the same `with_immediate` envelope as the
/// `Prepared→Signed` CAS.
pub async fn insert_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    kind: DocumentFileKind,
    content: &[u8],
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO document_files (document_id, kind, content) VALUES (?, ?, ?)")
        .bind(doc_id)
        .bind(kind)
        .bind(content)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Read a single artefact.  Returns `None` if the `(doc_id, kind)`
/// pair has no row.
pub async fn get_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    kind: DocumentFileKind,
) -> sqlx::Result<Option<Vec<u8>>> {
    let row: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT content FROM document_files WHERE document_id = ? AND kind = ?")
            .bind(doc_id)
            .bind(kind)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(row)
}

/// W10.4 step 2 — atomic in-place replacement of an artifact row.
///
/// Used by the MAC-recovery orchestrator (`mac_recovery.rs`) inside
/// the MR-PERSIST `with_immediate` envelope to swap PAYLOAD_XML and
/// SIGNED_XML after re-signing with a recovered `previous_hash`.
/// `INSERT OR REPLACE` is the SQLite idiom: the new row replaces any
/// existing `(document_id, kind)` row in a single statement, atomic
/// by definition.
///
/// **Why not DELETE+INSERT.**  Two statements would commit only if
/// both succeed (the wrapping `with_immediate` provides atomicity),
/// but `INSERT OR REPLACE` is a single statement, single B-tree
/// touch, and matches SQLite's documented "upsert by PK collision"
/// idiom.  No CHECK / UNIQUE / FK constraints on `document_files`
/// beyond the PK + FK to `fiscal_documents`, so PK collision is the
/// only concern — exactly what `OR REPLACE` handles.
///
/// **Forensic note.**  Replacing artifacts intentionally drops the
/// pre-recovery bytes.  The `MAC_RECOVERY_RESIGNED` audit row carries
/// `(old_previous_hash_hex, new_previous_hash_hex, new_unsigned_xml_sha256_hex)`
/// for chain-level forensics; raw pre-recovery XML is NOT preserved.
/// Operators wanting raw byte history of attempt #1 need a separate
/// archive layer (out of M3a scope).
pub async fn replace_tx(
    tx: &mut WriteTxConn<'_>,
    doc_id: DocumentId,
    kind: DocumentFileKind,
    content: &[u8],
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO document_files (document_id, kind, content) VALUES (?, ?, ?)",
    )
    .bind(doc_id)
    .bind(kind)
    .bind(content)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
