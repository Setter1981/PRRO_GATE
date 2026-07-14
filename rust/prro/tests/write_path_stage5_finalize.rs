//! W8.4 — stage 5 finalize integration fixtures.
//!
//! Anchored on the W8 design freeze §4.4 + §11 (W8.2 carry-forward
//! F5-bis: integrated rollback proof) + §12 (W8.3 carry-forward
//! F4-bis: `(Kvt2, Ack)` whitelist regression guard).  Plus W8.4
//! review F6-bis (rich audit payload assertion) + F7-bis (typed
//! error coverage for SeedUpdateMissing / InboxDoneMissing) + N2
//! (concurrent-finalize three-layer idempotency proof).
//!
//! Each fixture seeds a doc in some pre-finalize state + the
//! supporting rows (FN config, node_state, document_files SIGNED_XML,
//! ingress_inbox PROCESSING) and exercises a single (or, for
//! Fixture 12, concurrent) `stage_finalize::run` invocation.
//! Assertions cover state advance, inbox status, outbox row, audit row
//! count + payload contents, (negative paths) full rollback
//! completeness, and — post-A.3 (advance-at-SEND, design v3 §6) — the
//! invariant pins that finalize does NOT advance nor guard the cross-doc
//! MAC chain seed for EITHER lane (the online advance moved to
//! `stage_send`, the offline advance to `stage_offline_ack`).
//!
//! Fixture index:
//!   1.  `kvt2_to_ack_happy_path` — full happy + rich audit payload (F6-bis);
//!       doubles as A.3 pin (a) online / non-NULL-prior (seed non-advance).
//!   2.  `finalize_does_not_advance_seed_offline_origin_lane` — A.3 pin (a),
//!       offline lane (repurposed from `mac_chain_seed_advances_atomically_with_ack`).
//!   3.  `inbox_status_done_atomic_with_state_transition`.
//!   4.  `outbox_row_inserted_inside_lock_no_publish_yet`.
//!   5.  `rerun_on_ack_is_idempotent_no_op`.
//!   6.  `non_kvt2_state_short_circuits_no_seed_advance`.
//!   7.  `unsigned_xml_sha_missing_typed_error_full_rollback` (W8.2 F5-bis).
//!   8.  `document_missing_returns_outcome_not_error`.
//!   9.  `finalize_succeeds_with_node_state_absent_no_seed_dependency` — A.3
//!       pin (a), node_state independence (repurposed from `seed_update_missing…`).
//!   10. `inbox_done_missing_typed_error_full_rollback` (W8.4 F7-bis).
//!   11. `concurrent_finalize_yields_one_acked_and_one_already_acked` (W8.4 N2).
//!   12. `whitelist_kvt2_ack_regression_guard` (W8.3 F4-bis).
//!   13. A.3 seed-pins: `finalize_does_not_advance_seed_from_genesis_null` (pin a,
//!       genesis), `finalize_skips_when_seed_already_advanced_nongenesis` (pin b,
//!       Variant P skip), `finalize_skips_seed_guard_at_genesis_boundary` (pin b,
//!       genesis-boundary) — consolidated from the retired `chain_seed_*` guard tests.

use prro::db::models::enums::{DocState, NodeMode, ShiftState};
use prro::db::models::ids::DocumentId;
use prro::db::repositories::fiscal_documents::allowed_transition;
use prro::db::repositories::{node_state, outbox};
use prro::db::types::DbDocumentId;
use prro::services::write_path::stage_finalize::{self, StageFinalizeError, StageFinalizeOutcome};
use sqlx::SqlitePool;

// ─── Test seed helpers ───────────────────────────────────────────────

const FN_ID: &str = "1234567890";

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations");
    (dir, pool)
}

/// Seed FN config + node_state row + (optional) pre-existing seed.
async fn seed_fn(pool: &SqlitePool, prior_seed: Option<[u8; 32]>) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(FN_ID)
    .execute(pool)
    .await
    .unwrap();
    node_state::upsert_initial(pool, FN_ID, NodeMode::Online, ShiftState::Closed, 1)
        .await
        .expect("upsert_initial");
    if let Some(seed) = prior_seed {
        node_state::seed_prevhash(pool, FN_ID, &seed)
            .await
            .expect("seed_prevhash");
    }
}

/// Seed a doc + inbox row + SIGNED_XML.  `state` controls the
/// pre-finalize state; `unsigned_xml_sha256` is required for the
/// finalize seed advance — pass `Some(_)` for happy paths, `None`
/// for the `UnsignedXmlShaMissing` negative case.
async fn seed_doc(
    pool: &SqlitePool,
    doc_byte: u8,
    lnd: i64,
    state: &str,
    unsigned_xml_sha256: Option<[u8; 32]>,
    previous_hash: Option<[u8; 32]>,
) -> (DocumentId, [u8; 16]) {
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let payload_sha = vec![doc_byte; 32];

    // Inbox row in PROCESSING (would be set by W5 acquire_lease).
    let req_slice: &[u8] = &req_bytes;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'sell', ?, '{}', ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(FN_ID)
    .bind(format!("idem-{doc_byte:02x}"))
    .bind(&payload_sha)
    .execute(pool)
    .await
    .expect("seed inbox");

    // Doc row in target state.
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical, unsigned_xml_sha256, previous_hash) \
         VALUES (?, ?, ?, ?, 'SELL', ?, 'b1', 't1', 'ONLINE', \
            '2026-05-09T12:34:56Z', '{}', ?, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(req_slice)
    .bind(FN_ID)
    .bind(lnd)
    .bind(state)
    .bind(&payload_sha)
    .bind(unsigned_xml_sha256.as_ref().map(|h| &h[..]))
    .bind(previous_hash.as_ref().map(|h| &h[..]))
    .execute(pool)
    .await
    .expect("seed fiscal_documents");

    // SIGNED_XML — finalize doesn't read it but its presence reflects
    // the post-W6 state shape.
    sqlx::query(
        "INSERT INTO document_files(document_id, kind, content) \
         VALUES (?, 'SIGNED_XML', ?)",
    )
    .bind(&doc_bytes)
    .bind(b"FAKE-CMS".to_vec())
    .execute(pool)
    .await
    .expect("seed SIGNED_XML");

    // KVT raw bytes — for KVT1+ states finalize must NOT touch
    // document_files.  Seed KVT1_RAW + KVT2_RAW so W8.4 review F3
    // can assert post-finalize equality (R2 invariant: KVT raw
    // bytes are never lost during stage 5).
    if matches!(state, "KVT1" | "KVT2" | "ACK") {
        sqlx::query(
            "INSERT INTO document_files(document_id, kind, content) \
             VALUES (?, 'KVT1_RAW', ?)",
        )
        .bind(&doc_bytes)
        .bind(b"FAKE-KVT1-PROTOBUF".to_vec())
        .execute(pool)
        .await
        .expect("seed KVT1_RAW");
    }
    if matches!(state, "KVT2" | "ACK") {
        sqlx::query(
            "INSERT INTO document_files(document_id, kind, content) \
             VALUES (?, 'KVT2_RAW', ?)",
        )
        .bind(&doc_bytes)
        .bind(b"FAKE-KVT2-PROTOBUF".to_vec())
        .execute(pool)
        .await
        .expect("seed KVT2_RAW");
    }

    let doc = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let req: [u8; 16] = req_bytes.try_into().unwrap();
    (doc, req)
}

/// Read a `document_files` artifact by kind.  Used by the R2
/// invariant assertions (W8.4 review F3 close): KVT raw bytes
/// must survive finalize unchanged.
async fn read_doc_file(pool: &SqlitePool, doc: DocumentId, kind: &str) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT content FROM document_files WHERE document_id = ? AND kind = ?")
        .bind(DbDocumentId(doc))
        .bind(kind)
        .fetch_optional(pool)
        .await
        .expect("read document_files")
}

async fn read_state(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(DbDocumentId(doc))
        .fetch_one(pool)
        .await
        .expect("read state")
}

async fn read_seed(pool: &SqlitePool) -> Option<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT last_known_unsigned_xml_sha256 FROM node_state WHERE fiscal_number = ?",
    )
    .bind(FN_ID)
    .fetch_one(pool)
    .await
    .expect("read seed")
}

async fn read_inbox_status(pool: &SqlitePool, req: &[u8; 16]) -> String {
    let req_slice: &[u8] = req;
    sqlx::query_scalar("SELECT status FROM ingress_inbox WHERE request_id = ?")
        .bind(req_slice)
        .fetch_one(pool)
        .await
        .expect("read inbox status")
}

async fn read_outbox_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM outbox")
        .fetch_one(pool)
        .await
        .expect("count outbox")
}

async fn read_audit_event_types(pool: &SqlitePool, doc: DocumentId) -> Vec<String> {
    let entity_id = format!("{doc:?}");
    sqlx::query_scalar(
        "SELECT event_type FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? \
         ORDER BY audit_id",
    )
    .bind(&entity_id)
    .fetch_all(pool)
    .await
    .expect("read audit_log")
}

/// Read the `event_payload_json` for a specific event type.  Used by
/// F1 to verify the rich audit payload contents (W8.4 review F6-bis
/// close).
async fn read_audit_payload(pool: &SqlitePool, doc: DocumentId, event: &str) -> Option<String> {
    let entity_id = format!("{doc:?}");
    sqlx::query_scalar(
        "SELECT event_payload_json FROM audit_log \
         WHERE entity_type = 'fiscal_document' AND entity_id = ? AND event_type = ? \
         LIMIT 1",
    )
    .bind(&entity_id)
    .bind(event)
    .fetch_one(pool)
    .await
    .ok()
    .flatten()
}

// ─── Fixture 1 — happy: Kvt2 → Ack full round-trip ───────────────────

#[tokio::test]
async fn kvt2_to_ack_happy_path() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, Some([0xCD; 32])).await; // pre-existing seed (chain has prior docs)
    let (doc, req) = seed_doc(
        &pool,
        0x11,
        1,
        "KVT2",
        Some([0xAB; 32]), // doc's unsigned_xml_sha256 — becomes new seed
        Some([0xCD; 32]), // doc's previous_hash — should match prior seed
    )
    .await;

    let outcome = stage_finalize::run(&pool, doc)
        .await
        .expect("happy finalize");

    match outcome {
        StageFinalizeOutcome::Acked { fiscal_number, lnd } => {
            assert_eq!(fiscal_number, FN_ID);
            assert_eq!(lnd, 1);
        }
        other => panic!("expected Acked, got {other:?}"),
    }

    // Post-A.3 finalize writes: state→Ack, inbox DONE, outbox row,
    // audit row.  The cross-doc MAC chain seed is NO LONGER among them
    // — the online seed advance moved to `stage_send` (advance-at-SEND,
    // design v3 §6).  This test doubles as the **pin (a) online-lane /
    // non-NULL-prior** instance: the seed stays at its pre-existing
    // 0xCD value, NOT the doc's 0xAB hash.
    assert_eq!(read_state(&pool, doc).await, "ACK");
    assert_eq!(
        read_seed(&pool).await.unwrap(),
        vec![0xCD; 32],
        "A.3: finalize does NOT advance the seed — it stays at the \
         pre-existing 0xCD prior (would have become 0xAB pre-A.3)"
    );
    assert_eq!(read_inbox_status(&pool, &req).await, "DONE");
    assert_eq!(read_outbox_count(&pool).await, 1);

    let outbox_row = outbox::get_for_document(&pool, doc)
        .await
        .unwrap()
        .expect("outbox row");
    assert_eq!(outbox_row.status, "PENDING");
    assert!(outbox_row.published_at.is_none());
    assert_eq!(outbox_row.sequence_no, 1);
    assert_eq!(outbox_row.payload_sha256, [0x11; 32]);
    assert_eq!(outbox_row.fiscal_number, FN_ID);

    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_FINALIZE_ACK"]
    );

    // W8.4 review F6-bis close: rich audit payload contents must
    // match the design freeze §7 Q4 spec — request_id (hex),
    // document_id (hex), fiscal_number, lnd, unsigned_xml_sha256_hex,
    // previous_hash_hex.  hex strings are lowercase per W6 / W8.3
    // `hex_encode` convention.
    let payload = read_audit_payload(&pool, doc, "STAGE_FINALIZE_ACK")
        .await
        .expect("STAGE_FINALIZE_ACK must carry a non-NULL payload");
    let parsed: serde_json::Value =
        serde_json::from_str(&payload).expect("audit payload must be valid JSON");
    assert_eq!(parsed["fiscal_number"], FN_ID);
    assert_eq!(parsed["lnd"], 1);
    // request_id was 0xEE x 16 (= 0x11 ^ 0xFF).
    assert_eq!(
        parsed["request_id"].as_str().unwrap(),
        "ee".repeat(16),
        "request_id hex must be lowercase 32-char encoding of the [u8; 16]"
    );
    // doc's unsigned_xml_sha256 was 0xAB x 32.
    assert_eq!(
        parsed["unsigned_xml_sha256_hex"].as_str().unwrap(),
        "ab".repeat(32),
        "unsigned_xml_sha256_hex carries the doc's own hash for audit \
         cross-correlation — post-A.3 it no longer drives a seed advance"
    );
    // doc's previous_hash was 0xCD x 32.
    assert_eq!(
        parsed["previous_hash_hex"].as_str().unwrap(),
        "cd".repeat(32),
        "previous_hash_hex must be the doc's chain link"
    );
    // document_id is 0x11 x 16.
    assert_eq!(parsed["document_id"].as_str().unwrap(), "11".repeat(16));

    // W8 review F3 close: R2 "KVT raw bytes never lost" — finalize
    // does NOT touch document_files; KVT1_RAW and KVT2_RAW must
    // survive unchanged.
    assert_eq!(
        read_doc_file(&pool, doc, "KVT1_RAW").await.as_deref(),
        Some(&b"FAKE-KVT1-PROTOBUF"[..]),
        "KVT1_RAW raw bytes must be unchanged after Ack"
    );
    assert_eq!(
        read_doc_file(&pool, doc, "KVT2_RAW").await.as_deref(),
        Some(&b"FAKE-KVT2-PROTOBUF"[..]),
        "KVT2_RAW raw bytes must be unchanged after Ack"
    );
}

// ─── Pin (a) [offline lane] — finalize does NOT advance the seed for an
// OFFLINE-ORIGIN doc ─────────────────────────────────────────────────
//
// A.3 (design v3 §6): the online seed advance moved to `stage_send`; the
// offline-origin seed advance happens at `stage_offline_ack` (M2-01).
// By the time an offline-origin doc (`offline_fiscal_no` set) reaches
// finalize its seed is ALREADY advanced upstream, so finalize must be
// advance-free for this lane too.  (Repurposed from the old
// `mac_chain_seed_advances_atomically_with_ack`, whose "seed advances at
// ACK" premise is now false — the SEND-time advance is covered elsewhere.)

#[tokio::test]
async fn finalize_does_not_advance_seed_offline_origin_lane() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, Some([0x55; 32])).await; // non-NULL prior (advanced upstream)
    let (doc, _req) = seed_doc(&pool, 0x22, 1, "KVT2", Some([0x77; 32]), Some([0x55; 32])).await;

    // Stamp `offline_fiscal_no` so the row is an OFFLINE-ORIGIN doc —
    // `seed_doc` seeds online-origin (NULL) by default.
    sqlx::query("UPDATE fiscal_documents SET offline_fiscal_no = ? WHERE document_id = ?")
        .bind(4242_i64)
        .bind(DbDocumentId(doc))
        .execute(&pool)
        .await
        .expect("stamp offline_fiscal_no (offline-origin)");

    let pre_seed = read_seed(&pool).await.unwrap();
    assert_eq!(pre_seed, vec![0x55; 32]);

    let outcome = stage_finalize::run(&pool, doc).await.expect("finalize");
    assert!(matches!(outcome, StageFinalizeOutcome::Acked { .. }));

    assert_eq!(read_state(&pool, doc).await, "ACK");
    assert_eq!(
        read_seed(&pool).await.unwrap(),
        vec![0x55; 32],
        "A.3: offline-origin finalize does NOT re-advance the seed \
         (advanced at offline-ack, M2-01) — stays 0x55, not 0x77"
    );
}

// ─── Fixture 3 — inbox.status = DONE atomic with state transition ────

#[tokio::test]
async fn inbox_status_done_atomic_with_state_transition() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, None).await;
    let (doc, req) = seed_doc(&pool, 0x33, 1, "KVT2", Some([0x99; 32]), None).await;

    assert_eq!(read_inbox_status(&pool, &req).await, "PROCESSING");

    stage_finalize::run(&pool, doc).await.expect("finalize");

    assert_eq!(read_inbox_status(&pool, &req).await, "DONE");
    assert_eq!(read_state(&pool, doc).await, "ACK");
}

// ─── Fixture 4 — outbox row inserted inside lock; no publish yet ────

#[tokio::test]
async fn outbox_row_inserted_inside_lock_no_publish_yet() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, None).await;
    let (doc, _req) = seed_doc(&pool, 0x44, 5, "KVT2", Some([0xAA; 32]), None).await;

    assert_eq!(read_outbox_count(&pool).await, 0);

    stage_finalize::run(&pool, doc).await.expect("finalize");

    let row = outbox::get_for_document(&pool, doc)
        .await
        .unwrap()
        .expect("outbox row exists");
    assert_eq!(
        row.status, "PENDING",
        "stage 5 enqueues only — publish is post-M3a"
    );
    assert!(row.published_at.is_none());
    assert_eq!(
        row.sequence_no, 5,
        "sequence_no mirrors lnd at finalize time"
    );
    assert_eq!(row.payload_sha256, [0x44; 32]);
}

// ─── Fixture 5 — rerun on Ack is idempotent no-op (R3 / W8.3 contract)

#[tokio::test]
async fn rerun_on_ack_is_idempotent_no_op() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, None).await;
    let (doc, _req) = seed_doc(&pool, 0x55, 1, "KVT2", Some([0xEE; 32]), None).await;

    // First run — happy.
    let first = stage_finalize::run(&pool, doc)
        .await
        .expect("first finalize");
    assert!(matches!(first, StageFinalizeOutcome::Acked { .. }));
    assert_eq!(read_state(&pool, doc).await, "ACK");
    let seed_after_first = read_seed(&pool).await;
    let outbox_after_first = read_outbox_count(&pool).await;
    let audit_after_first = read_audit_event_types(&pool, doc).await;

    // Second run — must be no-op, no side effects.
    let second = stage_finalize::run(&pool, doc)
        .await
        .expect("rerun on Ack is success-shape");
    assert_eq!(
        second,
        StageFinalizeOutcome::AlreadyAcked,
        "rerun on Ack must short-circuit to AlreadyAcked"
    );

    // No side effects on second run.
    assert_eq!(read_state(&pool, doc).await, "ACK");
    assert_eq!(
        read_seed(&pool).await,
        seed_after_first,
        "seed must NOT advance on rerun"
    );
    assert_eq!(
        read_outbox_count(&pool).await,
        outbox_after_first,
        "outbox must NOT add a duplicate row"
    );
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        audit_after_first,
        "no second STAGE_FINALIZE_ACK audit row"
    );
}

// ─── Fixture 6 — non-Kvt2 state short-circuits, no seed advance (R1) ─

#[tokio::test]
async fn non_kvt2_state_short_circuits_no_seed_advance() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, Some([0x33; 32])).await;
    let (doc, req) = seed_doc(&pool, 0x66, 1, "KVT1", Some([0x99; 32]), None).await;

    let pre_seed = read_seed(&pool).await.unwrap();

    let outcome = stage_finalize::run(&pool, doc)
        .await
        .expect("non-Kvt2 is success-shape, not error");

    match outcome {
        StageFinalizeOutcome::StateConflict { observed } => {
            assert_eq!(observed, DocState::Kvt1);
        }
        other => panic!("expected StateConflict {{ Kvt1 }}, got {other:?}"),
    }

    // Atomic rollback: doc state preserved, seed unchanged, inbox
    // unchanged, outbox empty, audit empty.
    assert_eq!(read_state(&pool, doc).await, "KVT1");
    assert_eq!(read_seed(&pool).await.unwrap(), pre_seed);
    assert_eq!(read_inbox_status(&pool, &req).await, "PROCESSING");
    assert_eq!(read_outbox_count(&pool).await, 0);
    assert!(read_audit_event_types(&pool, doc).await.is_empty());
}

// ─── Fixture 7 — UnsignedXmlShaMissing → typed error + full rollback ─
// (W8.2 F5-bis carry-forward: integrated rollback proof.)

#[tokio::test]
async fn unsigned_xml_sha_missing_typed_error_full_rollback() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, Some([0x11; 32])).await;
    // unsigned_xml_sha256 = None — state-invariant breach (W6 must
    // populate it before reaching Kvt2; this is the simulated bug).
    let (doc, req) = seed_doc(&pool, 0x77, 1, "KVT2", None, None).await;

    let pre_seed = read_seed(&pool).await.unwrap();

    let err = stage_finalize::run(&pool, doc)
        .await
        .expect_err("missing unsigned_xml_sha256 must surface as typed error");

    match err {
        StageFinalizeError::UnsignedXmlShaMissing { document_id } => {
            assert_eq!(document_id, doc);
        }
        other => panic!("expected UnsignedXmlShaMissing, got {other:?}"),
    }

    // **Integrated rollback proof:** the CAS Kvt2 → Ack INSIDE the
    // closure committed in memory, but the closure's `?`-propagation
    // of the typed error rolled the entire `with_immediate` envelope
    // back.  All five would-be writes are undone:
    assert_eq!(
        read_state(&pool, doc).await,
        "KVT2",
        "doc state must rollback from in-progress Ack to original Kvt2"
    );
    assert_eq!(
        read_seed(&pool).await.unwrap(),
        pre_seed,
        "node_state seed must be unchanged"
    );
    assert_eq!(
        read_inbox_status(&pool, &req).await,
        "PROCESSING",
        "inbox must remain PROCESSING"
    );
    assert_eq!(
        read_outbox_count(&pool).await,
        0,
        "outbox must be empty (insert rolled back)"
    );
    assert!(
        read_audit_event_types(&pool, doc).await.is_empty(),
        "STAGE_FINALIZE_ACK must NOT have been written"
    );
}

// ─── Fixture 8 — DocumentMissing for bogus doc id (race-with-delete) ─

#[tokio::test]
async fn document_missing_returns_outcome_not_error() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, None).await;
    let bogus = DocumentId::from_bytes([0xAA; 16]);

    let outcome = stage_finalize::run(&pool, bogus)
        .await
        .expect("missing doc is success-shape, not error");
    assert_eq!(outcome, StageFinalizeOutcome::DocumentMissing);

    // No state mutation:
    assert!(
        read_seed(&pool).await.is_none(),
        "seed must remain unchanged (NULL after upsert_initial)"
    );
    assert_eq!(read_outbox_count(&pool).await, 0);
}

// ─── Pin (a) [independence] — finalize succeeds with node_state ABSENT
// ──────────────────────────────────────────────────────────────────────
//
// Pre-A.3 finalize read + UPDATE'd `node_state` to advance the seed, so a
// missing `node_state` row surfaced `SeedUpdateMissing` + full rollback.
// Post-A.3 finalize NO LONGER touches `node_state` at all — deleting the
// row has NO effect: finalize still reaches ACK.  This is the negative
// pin against re-introducing a node_state read/advance in finalize
// (which would resurrect the SeedUpdateMissing failure).  (Repurposed
// from the old `seed_update_missing_typed_error_full_rollback`.)

#[tokio::test]
async fn finalize_succeeds_with_node_state_absent_no_seed_dependency() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, None).await;
    let (doc, req) = seed_doc(&pool, 0x88, 1, "KVT2", Some([0xCC; 32]), None).await;

    // Delete the node_state row.  Pre-A.3 this forced SeedUpdateMissing;
    // post-A.3 finalize never reads it, so the delete is inert.
    sqlx::query("DELETE FROM node_state WHERE fiscal_number = ?")
        .bind(FN_ID)
        .execute(&pool)
        .await
        .expect("delete node_state");

    let outcome = stage_finalize::run(&pool, doc)
        .await
        .expect("finalize must SUCCEED — it no longer depends on node_state");
    assert!(matches!(outcome, StageFinalizeOutcome::Acked { .. }));

    // Doc reaches terminal Ack; downstream writes (inbox, outbox, audit)
    // land normally — none of them touch node_state.
    assert_eq!(read_state(&pool, doc).await, "ACK");
    assert_eq!(read_inbox_status(&pool, &req).await, "DONE");
    assert_eq!(read_outbox_count(&pool).await, 1);
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_FINALIZE_ACK"]
    );

    // node_state stays absent — finalize never re-creates or advances it.
    let ns_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM node_state WHERE fiscal_number = ?")
            .bind(FN_ID)
            .fetch_one(&pool)
            .await
            .expect("count node_state");
    assert_eq!(
        ns_count, 0,
        "node_state row must stay absent — finalize does not touch it"
    );
}

// ─── Fixture 10 — InboxDoneMissing typed error + full rollback ──────
// (W8.4 review F7-bis companion.)

#[tokio::test]
async fn inbox_done_missing_typed_error_full_rollback() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, Some([0x44; 32])).await;
    // doc.previous_hash MUST match FN seed (0x44) — otherwise the
    // chain-continuity guard would reject FIRST and we'd never reach
    // the InboxDoneMissing path.
    let (doc, req) = seed_doc(&pool, 0x99, 1, "KVT2", Some([0x66; 32]), Some([0x44; 32])).await;

    let pre_seed = read_seed(&pool).await.unwrap();

    // Simulate inbox row vanished between W5 acquire_lease and
    // stage_finalize::run.  CAS Kvt2 → Ack applies, seed advance
    // applies, then mark_done_tx returns false → InboxDoneMissing
    // typed error → full envelope rollback.
    let req_slice: &[u8] = &req;
    sqlx::query("DELETE FROM ingress_inbox WHERE request_id = ?")
        .bind(req_slice)
        .execute(&pool)
        .await
        .expect("delete inbox row");

    let err = stage_finalize::run(&pool, doc)
        .await
        .expect_err("missing inbox row must surface as typed error");

    match err {
        StageFinalizeError::InboxDoneMissing { request_id } => {
            assert_eq!(request_id, req);
        }
        other => panic!("expected InboxDoneMissing, got {other:?}"),
    }

    // Integrated rollback proof: CAS + seed advance both rolled back.
    assert_eq!(read_state(&pool, doc).await, "KVT2");
    assert_eq!(
        read_seed(&pool).await.unwrap(),
        pre_seed,
        "seed advance must rollback (chain pointer protected)"
    );
    assert_eq!(read_outbox_count(&pool).await, 0);
    assert!(read_audit_event_types(&pool, doc).await.is_empty());
}

// ─── Fixture 11 — concurrent finalize: one Acked, one AlreadyAcked ──
// (W8.4 review N2: stress the three-layer idempotency under genuine
// concurrent invocations.  WAL writer-lock + CAS short-circuit +
// outbox PK should yield (Acked, AlreadyAcked) regardless of which
// task wins the writer-lock race.)

#[tokio::test]
async fn concurrent_finalize_yields_one_acked_and_one_already_acked() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, None).await;
    let (doc, _req) = seed_doc(&pool, 0xAA, 1, "KVT2", Some([0xBB; 32]), None).await;

    let pool_a = pool.clone();
    let pool_b = pool.clone();

    let handle_a = tokio::spawn(async move { stage_finalize::run(&pool_a, doc).await });
    let handle_b = tokio::spawn(async move { stage_finalize::run(&pool_b, doc).await });

    let outcome_a = handle_a.await.expect("spawn join a").expect("finalize a");
    let outcome_b = handle_b.await.expect("spawn join b").expect("finalize b");

    // Exactly one Acked + one AlreadyAcked, in either order.
    let mut shapes: Vec<&str> = vec![&outcome_a, &outcome_b]
        .into_iter()
        .map(|o| match o {
            StageFinalizeOutcome::Acked { .. } => "Acked",
            StageFinalizeOutcome::AlreadyAcked => "AlreadyAcked",
            other => panic!("unexpected outcome: {other:?}"),
        })
        .collect();
    shapes.sort();
    assert_eq!(
        shapes,
        vec!["Acked", "AlreadyAcked"],
        "concurrent finalize MUST yield exactly one Acked and one AlreadyAcked"
    );

    // Exactly ONE outbox row (PK on document_id + CAS short-circuit).
    assert_eq!(
        read_outbox_count(&pool).await,
        1,
        "outbox PK + CAS short-circuit ensure exactly one row even under concurrent finalize"
    );

    // Exactly ONE STAGE_FINALIZE_ACK audit row (audit-on-Applied-only
    // discipline).
    assert_eq!(
        read_audit_event_types(&pool, doc).await,
        vec!["STAGE_FINALIZE_ACK"],
        "exactly one audit row — only the Acked path appends"
    );

    assert_eq!(read_state(&pool, doc).await, "ACK");
}

// ─── Pin (a) [genesis] — finalize does NOT advance the seed from genesis
// NULL ────────────────────────────────────────────────────────────────
//
// Genesis: FN seed NULL, doc previous_hash NULL.  Pre-A.3 finalize
// advanced the seed to the doc's unsigned_xml_sha256 (0xAA here); the old
// `chain_seed_genesis_none_to_none_succeeds` asserted exactly that.
// Post-A.3 finalize is advance-free — the seed stays NULL.  (The SEND
// stage owns the genesis advance now.)

#[tokio::test]
async fn finalize_does_not_advance_seed_from_genesis_null() {
    let (_d, pool) = fresh_pool().await;
    seed_fn(&pool, None).await;
    let (doc, _req) = seed_doc(&pool, 0xC4, 1, "KVT2", Some([0xAA; 32]), None).await;

    let outcome = stage_finalize::run(&pool, doc)
        .await
        .expect("genesis finalize must succeed");
    assert!(matches!(outcome, StageFinalizeOutcome::Acked { .. }));
    assert_eq!(read_state(&pool, doc).await, "ACK");
    assert!(
        read_seed(&pool).await.is_none(),
        "A.3: finalize does NOT advance from genesis — seed stays NULL \
         (would have become 0xAA pre-A.3)"
    );
}

// ─── Pin (b) [non-genesis] — seed already advanced past the doc →
// finalize passes with NO guard-fire and NO re-advance ─────────────────
//
// Variant P / advance-at-SEND skip: by the time finalize runs, the seed
// has ALREADY moved past this doc at SEND (e.g. a successor was issued),
// so `node_state.last_known != doc.previous_hash`.  Pre-A.3 the §3
// chain-continuity guard would have FALSE-FIRED `ChainSeedMismatch` here
// (Some(0x99) expected vs Some(0x11) actual).  Post-A.3 there is no
// guard: finalize succeeds to ACK and the already-advanced seed is left
// untouched (no re-advance to the doc's own hash).  (Consolidates the
// old `chain_seed_mismatch_some_neq_some_typed_error_full_rollback`.)

#[tokio::test]
async fn finalize_skips_when_seed_already_advanced_nongenesis() {
    let (_d, pool) = fresh_pool().await;
    // Seed already advanced to 0x11 (a successor's hash), NOT the doc's
    // older previous_hash 0x99.
    seed_fn(&pool, Some([0x11; 32])).await;
    let (doc, req) = seed_doc(&pool, 0xC1, 1, "KVT2", Some([0xCC; 32]), Some([0x99; 32])).await;

    let outcome = stage_finalize::run(&pool, doc)
        .await
        .expect("A.3: no chain guard — finalize must SUCCEED, not ChainSeedMismatch");
    assert!(matches!(outcome, StageFinalizeOutcome::Acked { .. }));

    // Terminal Ack + downstream writes land; seed is NOT re-advanced.
    assert_eq!(read_state(&pool, doc).await, "ACK");
    assert_eq!(
        read_seed(&pool).await.unwrap(),
        vec![0x11; 32],
        "seed stays at the already-advanced 0x11 — finalize neither \
         guards nor re-advances (0xCC would be a double-move)"
    );
    assert_eq!(read_inbox_status(&pool, &req).await, "DONE");
    assert_eq!(read_outbox_count(&pool).await, 1);
    // KVT2_RAW untouched (finalize is a state close, not a wire write).
    assert_eq!(
        read_doc_file(&pool, doc, "KVT2_RAW").await.as_deref(),
        Some(&b"FAKE-KVT2-PROTOBUF"[..]),
        "KVT2_RAW must remain intact through finalize"
    );
}

// ─── Pin (b) [genesis boundary] — the two genesis-boundary mismatch
// shapes ALSO pass post-A.3 (no guard) ─────────────────────────────────
//
// Pre-A.3 both shapes tripped the §3 chain-continuity guard:
//   * None vs Some — seed NULL (genesis) but doc claims a previous_hash.
//   * Some vs None — seed advanced but doc claims to be genesis.
// Post-A.3 neither guard exists: finalize succeeds to ACK and leaves the
// seed exactly as-is (no advance in either direction).  (Consolidates the
// old `chain_seed_mismatch_none_vs_some` + `..._some_vs_none`.)

#[tokio::test]
async fn finalize_skips_seed_guard_at_genesis_boundary() {
    // Shape 1 — None vs Some: genesis seed, doc previous_hash = Some.
    {
        let (_d, pool) = fresh_pool().await;
        seed_fn(&pool, None).await;
        let (doc, _req) =
            seed_doc(&pool, 0xC2, 1, "KVT2", Some([0xDD; 32]), Some([0x77; 32])).await;

        let outcome = stage_finalize::run(&pool, doc)
            .await
            .expect("A.3: None-vs-Some no longer a guard breach — must SUCCEED");
        assert!(matches!(outcome, StageFinalizeOutcome::Acked { .. }));
        assert_eq!(read_state(&pool, doc).await, "ACK");
        assert!(
            read_seed(&pool).await.is_none(),
            "seed stays NULL — finalize does not advance to 0xDD"
        );
    }

    // Shape 2 — Some vs None: advanced seed, doc previous_hash = None.
    {
        let (_d, pool) = fresh_pool().await;
        seed_fn(&pool, Some([0x44; 32])).await;
        let (doc, _req) = seed_doc(&pool, 0xC3, 1, "KVT2", Some([0xEE; 32]), None).await;

        let outcome = stage_finalize::run(&pool, doc)
            .await
            .expect("A.3: Some-vs-None no longer a guard breach — must SUCCEED");
        assert!(matches!(outcome, StageFinalizeOutcome::Acked { .. }));
        assert_eq!(read_state(&pool, doc).await, "ACK");
        assert_eq!(
            read_seed(&pool).await.unwrap(),
            vec![0x44; 32],
            "seed stays at 0x44 — finalize does not advance to 0xEE"
        );
    }
}

// ─── Fixture 16 — `(Kvt2, Ack)` whitelist regression guard (F4-bis) ──

#[test]
fn whitelist_kvt2_ack_regression_guard() {
    // W8.3 closure depends on `(Kvt2, Ack)` being whitelisted; the
    // Forbidden arm carries `unreachable!()`.  A future migration that
    // accidentally drops this entry would make the unreachable! fire
    // in production; this sub-millisecond test catches the regression
    // at CI.  Mirrors W7.5 `whitelist_signed_sending_regression_guard`.
    assert!(
        allowed_transition(DocState::Kvt2, DocState::Ack),
        "(Kvt2, Ack) MUST stay in the allowed_transition whitelist"
    );
}
