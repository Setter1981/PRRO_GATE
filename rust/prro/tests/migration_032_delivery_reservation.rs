//! Migration 032 (`delivery_reservation`) — the Spec #4 part A §6 matrix as
//! PERMANENT Rust regression teeth.
//!
//! CS-2 lands `delivery_reservation` as an **INACTIVE** durable table (no
//! production caller — Spec #4 §1/§2b).  This suite ports BOTH audit passes'
//! adversarial battery (external round 4 + the decorrelated internal Sonnet
//! real-SQLite pass) so the teeth live in-repo, not just in an ephemeral SQL
//! run.  Every attack asserts the DB **rejects** it (CHECK / trigger `ABORT` /
//! partial-unique fence / composite-FK), and every blessed/legal path asserts
//! it is **accepted**.
//!
//! Groups:
//! - structural: sqlite_master existence; PRAGMA column-set; INSERT round-trip + defaults; every CHECK reject.
//! - fence: `ux_reservation_active` partial-unique — a 2nd reservation is refused after every UNSAFE terminal, accepted after a fence-releasing one.
//! - transition: the `transition` trigger — only the legal edges succeed.
//! - immutability: the `immutable` trigger — identity/binding/marker frozen.
//! - append-only: the `append_only` trigger — no DELETE ever.
//! - collision: the `no_replace` collision-guard — `INSERT OR REPLACE` / `INSERT OR IGNORE` / UPSERT cannot evict or laund a row.
//! - composite-FK: `(document_id, fiscal_number)` FN-A/FN-B mismatch; parent delete blocked.
//! - blessed: `SUBMITTED_UNKNOWN + NO_RESPONSE + TransientRetry` (the canonical wire-timeout) is ACCEPTED.
//! - §6 merge pins: upgrade 031→032 on a NON-EMPTY representative DB; a sqlite_master byte-diff; the empty-after-fiscalisation production-flow pin; the static call-graph pin.
//!
//! Assertions on trigger `RAISE(ABORT, 'msg')` match a distinctive substring of
//! the message; CHECK / UNIQUE / FK violations match `check` / `constraint` /
//! `unique` / `foreign key` (mirrors `migration_011_outbox.rs`).

use prro::db::models::ids::DocumentId;
use prro::db::repositories::delivery_reservation::{self, NewReservation};
use prro::db::tx::with_immediate;
use sqlx::SqlitePool;
use std::collections::HashSet;

#[path = "support/inactive_lifecycle_scan.rs"]
mod inactive_lifecycle_scan;
use inactive_lifecycle_scan::{scan_inactive_lifecycle_refs, scan_src_tree};

// ─────────────────────────── fixtures ───────────────────────────

async fn fresh_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = prro::db::open_pool(&dir.path().join("m.db"))
        .await
        .expect("open_pool runs all migrations including 032");
    (dir, pool)
}

const FN_A: &str = "1234567890";
const FN_B: &str = "9876543210";

/// Seed an FN config row (idempotent).
async fn seed_fn(pool: &SqlitePool, fscl: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fscl)
    .execute(pool)
    .await
    .expect("seed fiscal_number_config");
}

/// Seed a parent `fiscal_documents` row under `fscl` and return its id.
/// `doc_byte` disambiguates the 16-byte document_id / request_id; `lnd`
/// dodges the `(fiscal_number, lnd)` partial UNIQUE.
async fn seed_doc(pool: &SqlitePool, fscl: &str, doc_byte: u8, lnd: i64) -> DocumentId {
    seed_fn(pool, fscl).await;
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, doc_type, \
            state, backend_profile_id, transport_profile_id, fs_mode, business_ts, payload_json, \
            payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SELL', 'SENDING', 'b1', 't1', 'ONLINE', \
            '2026-07-15T12:34:56Z', '{}', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fscl)
    .bind(lnd)
    .bind(&sha)
    .execute(pool)
    .await
    .expect("seed fiscal_documents");
    DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap())
}

/// The canonical `NewReservation` for `(res_byte, doc, fscl)`.
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

/// Insert a reservation via the repo (`RESERVED_NOT_STARTED`).  Returns the
/// assigned attempt_no.  Uses the tx-only repo `insert` under `with_immediate`.
async fn insert_res(pool: &SqlitePool, row: NewReservation) -> anyhow::Result<i64> {
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            delivery_reservation::insert(tx, row)
                .await
                .map_err(Into::into)
        })
    })
    .await
}

/// Raw INSERT of a fully-specified reservation as `RESERVED_NOT_STARTED`
/// bypassing the repo (so CHECK / trigger attacks can set arbitrary fields).
/// `attempt_no` is explicit here.
#[allow(clippy::too_many_arguments)]
async fn raw_insert_rns(
    pool: &SqlitePool,
    res_byte: u8,
    doc: DocumentId,
    fscl: &str,
    attempt_no: i64,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO delivery_reservation \
             (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
              protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, ?, 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[res_byte; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(fscl)
    .bind(attempt_no)
    .bind(&[0xABu8; 32][..])
    .execute(pool)
    .await
}

fn err_has(err: &sqlx::Error, needle: &str) -> bool {
    err.to_string().to_lowercase().contains(needle)
}

// ═══════════════════════════ structural ═══════════════════════════

#[tokio::test]
async fn s01_migration_creates_table_indexes_triggers() {
    let (_d, pool) = fresh_pool().await;

    let tables: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    assert!(tables.contains("delivery_reservation"), "table missing");

    let indexes: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='index'")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    for ix in [
        "ux_fd_docid_fn",
        "ux_reservation_active",
        "ix_reservation_call_started",
    ] {
        assert!(indexes.contains(ix), "index {ix} missing; have {indexes:?}");
    }

    let triggers: HashSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger'")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    for tg in [
        "delivery_reservation_insert_state",
        "delivery_reservation_no_replace",
        "delivery_reservation_transition",
        "delivery_reservation_immutable",
        "delivery_reservation_append_only",
        "delivery_reservation_updated_at",
    ] {
        assert!(
            triggers.contains(tg),
            "trigger {tg} missing; have {triggers:?}"
        );
    }
}

#[tokio::test]
async fn s02_column_set_matches_ddl() {
    let (_d, pool) = fresh_pool().await;
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(delivery_reservation)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: HashSet<String> = cols.iter().map(|c| c.1.clone()).collect();
    for col in [
        "reservation_id",
        "document_id",
        "fiscal_number",
        "attempt_no",
        "state",
        "call_started_at",
        "dps_protocol_id",
        "protocol_contract_version",
        "capability_profile_version",
        "endpoint_config_revision",
        "envelope_hash",
        "remote_correlation_id",
        "submission_certainty",
        "response_provenance",
        "routing_class",
        "created_at",
        "updated_at",
    ] {
        assert!(names.contains(col), "column {col} missing; have {names:?}");
    }
    // The 17 columns 032 introduced must all be present. The EXACT column count
    // is owned by the latest migration's test (migration_033 `rg01` pins 20 after
    // 033 adds authorized_generation / apply_state / node_effect); a later
    // migration extending the table must not force a churn edit here.
    assert!(
        names.len() >= 17,
        "the 032 column set must be present; have {names:?}"
    );
}

#[tokio::test]
async fn s03_insert_roundtrip_and_defaults() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    let attempt = insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    assert_eq!(attempt, 1, "first attempt_no is 1");

    let r = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap()
        .expect("active reservation must exist (RESERVED_NOT_STARTED is fenced)");
    assert_eq!(r.reservation_id, [0x01; 16]);
    assert_eq!(r.document_id, doc);
    assert_eq!(r.fiscal_number, FN_A);
    assert_eq!(r.attempt_no, 1);
    assert_eq!(r.state, "RESERVED_NOT_STARTED", "DEFAULT state");
    assert!(r.call_started_at.is_none());
    assert_eq!(r.dps_protocol_id, "FSCO_ZZD");
    assert_eq!(r.protocol_contract_version, 1);
    assert!(r.capability_profile_version.is_none());
    assert!(r.endpoint_config_revision.is_none());
    assert_eq!(r.envelope_hash, [0xAB; 32]);
    assert!(r.remote_correlation_id.is_none());
    assert!(r.submission_certainty.is_none());
    assert!(r.response_provenance.is_none());
    assert!(r.routing_class.is_none());
    assert!(
        !r.created_at.is_empty(),
        "created_at DEFAULT CURRENT_TIMESTAMP"
    );
    assert!(
        !r.updated_at.is_empty(),
        "updated_at DEFAULT CURRENT_TIMESTAMP"
    );
}

#[tokio::test]
async fn s04_check_bad_state_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, state, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'BOGUS_STATE', 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("bad state must be rejected");
    // state CHECK fires, or insert_state trigger — either way a reject.
    assert!(
        err_has(&err, "check")
            || err_has(&err, "constraint")
            || err_has(&err, "reserved_not_started"),
        "{err}"
    );
}

#[tokio::test]
async fn s05_check_attempt_no_ge_1() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    let err = raw_insert_rns(&pool, 0x01, doc, FN_A, 0)
        .await
        .expect_err("attempt_no 0 must violate CHECK (>= 1)");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s06_check_reservation_id_length() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 8][..]) // 8 bytes, not 16
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("reservation_id length != 16 must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s07_check_envelope_hash_length() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 16][..]) // 16 bytes, not 32
    .execute(&pool)
    .await
    .expect_err("envelope_hash length != 32 must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s08_check_bad_protocol_and_version_floors() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;

    // bad dps_protocol_id
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'NOT_A_PROTOCOL', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("bad dps_protocol_id must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );

    // protocol_contract_version < 1
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 0, ?)",
    )
    .bind(&[0x02u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("protocol_contract_version 0 must violate CHECK (>= 1)");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s09_check_negative_optional_versions_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;

    // capability_profile_version = 0 (must be NULL or >= 1)
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, capability_profile_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 1, 0, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("capability_profile_version 0 must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );

    // endpoint_config_revision = -1
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, endpoint_config_revision, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 1, -1, ?)",
    )
    .bind(&[0x02u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("endpoint_config_revision -1 must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s10_check_remote_correlation_id_null_pre_oo() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    // remote_correlation_id set while state is RESERVED_NOT_STARTED → CHECK rejects.
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, remote_correlation_id, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 1, 'corr-123', ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("remote_correlation_id pre-OUTCOME_OBSERVED must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s11_check_insert_must_be_rns_trigger() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    // INSERT directly as CALL_STARTED → insert_state trigger ABORTs.
    // authorized_generation is set to keep the 034 RN→CS pairing trigger satisfied
    // (call_started_at set ⇒ authorized_generation set), so the rejection we observe
    // is the intended 032 insert_state trigger (state must be RESERVED_NOT_STARTED),
    // not the pairing guard.
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, state, call_started_at, \
             authorized_generation, dps_protocol_id, protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'CALL_STARTED', '2026-07-15T00:00:00Z', 1, 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("INSERT as CALL_STARTED must be blocked by insert_state trigger");
    assert!(
        err_has(&err, "reserved_not_started") || err_has(&err, "constraint"),
        "{err}"
    );
}

// ─── structural matrix: the cross-field / 3-field consistency CHECKs ───

/// Helper: attempt to advance an existing RNS row (attempt 1, res 0x01, doc)
/// to OUTCOME_OBSERVED with the given fields, expecting a CHECK/trigger reject.
/// Some attacks are pre-call (from RNS), some via CALL_STARTED first.
async fn advance_to_cs(pool: &SqlitePool, res_byte: u8) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'CALL_STARTED', call_started_at = '2026-07-15T00:00:00Z', \
             authorized_generation = 1 \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn s12_matrix_rns_must_have_null_outcome_fields() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    // RNS with a non-null submission_certainty → structural CHECK (line: state<>'RNS' OR ...NULL).
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, state, submission_certainty, \
             dps_protocol_id, protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'RESERVED_NOT_STARTED', 'SUBMITTED', 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("RNS with outcome field set must violate structural CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s13_matrix_not_submitted_requires_no_call_and_no_response() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // RN → CS sets call_started_at.
    advance_to_cs(&pool, 0x01).await.unwrap();
    // CS → OO(NOT_SUBMITTED) is illegal both structurally (call_started_at NOT NULL)
    // AND by the transition trigger (CS→OO requires SUBMITTED*).
    // apply_state + node_effect satisfy 034 H3 (OO completeness) / H4 (clean-accept)
    // so the row reaches the intended 032 structural CHECK (NOT_SUBMITTED requires
    // call_started_at NULL / routing NULL is SUBMITTED-only), not the H3 guard.
    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'NOT_SUBMITTED', \
             response_provenance = 'NO_RESPONSE', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("CS→OO(NOT_SUBMITTED) must be rejected (call_started set + illegal edge)");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint") || err_has(&err, "illegal"),
        "{err}"
    );
}

#[tokio::test]
async fn s14_matrix_submitted_requires_parsed_envelope() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // SUBMITTED with response_provenance != PARSED_DPS_ENVELOPE → CHECK reject.
    // apply_state + node_effect satisfy 034 H3/H4 so the row reaches the intended
    // 032 CHECK (SUBMITTED requires PARSED_DPS_ENVELOPE), not the H3 guard.
    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED', \
             response_provenance = 'AUTHENTICATED_PEER', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("SUBMITTED requires PARSED_DPS_ENVELOPE");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s15_matrix_submitted_unknown_requires_call_started() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    // Try to INSERT-then-jump RN→OO(SUBMITTED_UNKNOWN) — but there is no call_started_at
    // because RN→OO(SUBMITTED_UNKNOWN) is not a legal transition AND SUBMITTED_UNKNOWN
    // requires call_started_at NOT NULL.  We assert the structural CHECK via a raw row.
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, state, submission_certainty, \
             response_provenance, dps_protocol_id, protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'RESERVED_NOT_STARTED', 'SUBMITTED_UNKNOWN', 'NO_RESPONSE', \
                 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("SUBMITTED_UNKNOWN without call_started_at must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s16_matrix_response_derived_class_needs_parsed_envelope() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // OO(SUBMITTED_UNKNOWN + NO_RESPONSE) with routing_class = TerminalReject → reject:
    // response-derived classes require SUBMITTED + PARSED_DPS_ENVELOPE.
    // apply_state + node_effect satisfy 034 H3 (routing NOT NULL ⇒ H4 n/a) so the row
    // reaches the intended 032 CHECK (response-derived class requires SUBMITTED+PARSED).
    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("TerminalReject without parsed DPS envelope must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s17_matrix_drainchain_requires_submitted_parsed() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // DrainChainSettleRetry with SUBMITTED_UNKNOWN + NO_RESPONSE → reject
    // (must be a parsed DPS artifact, never pre-call/no-response — rev 5 fix).
    // apply_state + node_effect satisfy 034 H3 (routing NOT NULL ⇒ H4 n/a) so the row
    // reaches the intended 032 CHECK (DrainChain class requires SUBMITTED+PARSED).
    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', routing_class = 'DrainChainSettleRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("DrainChainSettleRetry + SUBMITTED_UNKNOWN/NO_RESPONSE must violate CHECK");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn s18_matrix_oo_requires_certainty_and_provenance() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    // Raw insert cannot be OO (insert_state trigger), so drive via transition:
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // CS→OO leaving submission_certainty NULL → transition trigger requires SUBMITTED*
    // (so this is illegal both at the trigger and the OO structural CHECK).
    // apply_state + node_effect satisfy 034 H3/H4 (routing NULL + NoNodeEffect) so the
    // row reaches the intended 032 CHECK (OO requires certainty + provenance), not H3.
    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', apply_state = 'PENDING_APPLY', \
             node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("OO without certainty/provenance must be rejected");
    assert!(
        err_has(&err, "check") || err_has(&err, "constraint") || err_has(&err, "illegal"),
        "{err}"
    );
}

// ═══════════════════════════ transitions (legal) ═══════════════════════════

/// Drive a reservation RN→CS→OO(SUBMITTED, PARSED, routing NULL) = clean accept.
async fn drive_clean_accept(pool: &SqlitePool, res_byte: u8) {
    advance_to_cs(pool, res_byte).await.expect("RN→CS legal");
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted', evidence_text = '4000000001', \
             remote_correlation_id = '4000000001' \
         WHERE reservation_id = ?",
    )
    .bind(&[res_byte; 16][..])
    .execute(pool)
    .await
    .expect("CS→OO(SUBMITTED) legal");
}

#[tokio::test]
async fn t01_rn_cs_oo_happy_path_ok() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // RN→CS→OO clean accept. §3.1: holds at PENDING_APPLY, releases at APPLIED.
    drive_clean_accept(&pool, 0x01).await;
    sqlx::query("UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap();
    assert!(
        active.is_none(),
        "clean accept (SUBMITTED + routing NULL) at APPLIED releases the fence"
    );
}

#[tokio::test]
async fn t02_rn_oo_not_submitted_clears_marker_ok() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // RN → OO(NOT_SUBMITTED): safe pre-call cancel. call_started_at stays NULL.
    // An OUTCOME_OBSERVED row with routing_class NULL is only legal for SUBMITTED
    // (CHECK line 110), so a NOT_SUBMITTED cancel carries a non-response-derived
    // routing_class (TransientRetry — the only classes allowed with NOT_SUBMITTED
    // are TransientRetry / WrapperBug).
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'NOT_SUBMITTED', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'PreconditionFailed' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("RN→OO(NOT_SUBMITTED) is a legal edge");
    // §3.1: releases only at APPLIED (PENDING_APPLY holds).
    sqlx::query("UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap();
    assert!(
        active.is_none(),
        "NOT_SUBMITTED cancel at APPLIED releases the fence"
    );
}

#[tokio::test]
async fn t03_rn_to_oo_submitted_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // RN → OO(SUBMITTED) skips CALL_STARTED → illegal edge (transition trigger).
    // authorized_generation (034 H2 pairing) + apply_state + node_effect (H3/H4) are all
    // set so every 034 guard passes and the rejection is the intended 032 transition
    // trigger (RN→OO skips CALL_STARTED = illegal edge).
    let err = sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             call_started_at = '2026-07-15T00:00:00Z', authorized_generation = 1, \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("RN→OO(SUBMITTED) is not a legal edge");
    assert!(
        err_has(&err, "illegal") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn t04_cs_to_rn_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // CS → RN (regress) → illegal edge.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET state = 'RESERVED_NOT_STARTED' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("CS→RN regress is not a legal edge");
    // Illegal transition OR immutable (call_started_at was set) — either is a reject.
    assert!(
        err_has(&err, "illegal") || err_has(&err, "immutable") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn t05_rn_cs_oo_submitted_unknown_ok_and_fences() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // CS→OO(SUBMITTED_UNKNOWN, NO_RESPONSE, TransientRetry) — legal edge,
    // canonical timeout.  routing_class is required (CHECK line 110: only
    // SUBMITTED may have a NULL routing_class at OUTCOME_OBSERVED).
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'NoResponse', evidence_text = 'Timeout' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("CS→OO(SUBMITTED_UNKNOWN) legal");
    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap();
    assert!(
        active.is_some(),
        "SUBMITTED_UNKNOWN keeps the fence held (in-doubt)"
    );
}

// ═══════════════════════════ fence (partial-unique) ═══════════════════════════

#[tokio::test]
async fn f01_second_reservation_blocked_while_rns() {
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();
    // Second reservation on the SAME FN while the first is RESERVED_NOT_STARTED
    // (fenced) → blocked (collision-guard trigger fires first; the partial-unique
    // is the backstop).
    let err = insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect_err("2nd active reservation on a fenced FN must be blocked");
    let m = err.to_string().to_lowercase();
    assert!(
        m.contains("collision") || m.contains("unique") || m.contains("constraint"),
        "{m}"
    );
}

#[tokio::test]
async fn f02_second_reservation_blocked_after_submitted_unknown() {
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'NoResponse', evidence_text = 'Timeout' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    // FN is still fenced (SUBMITTED_UNKNOWN) → 2nd reservation blocked.
    let err = insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect_err("2nd reservation after SUBMITTED_UNKNOWN must be blocked");
    let m = err.to_string().to_lowercase();
    assert!(
        m.contains("collision") || m.contains("unique") || m.contains("constraint"),
        "{m}"
    );
}

#[tokio::test]
async fn f03_second_reservation_blocked_after_submitted_with_routing() {
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // OO(SUBMITTED, PARSED, routing=TerminalReject) → observed reject, fence HELD.
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', routing_class = 'TerminalReject', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'Rejected', evidence_text = 'Verify', \
             evidence_digest = X'0000000000000000000000000000000000000000000000000000000000000000' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    let err = insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect_err("2nd reservation after SUBMITTED+routing!=NULL must be blocked");
    let m = err.to_string().to_lowercase();
    assert!(
        m.contains("collision") || m.contains("unique") || m.contains("constraint"),
        "{m}"
    );
}

#[tokio::test]
async fn f04_second_reservation_accepted_after_clean_accept() {
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();
    drive_clean_accept(&pool, 0x01).await; // SUBMITTED + routing NULL, PENDING_APPLY → STILL fenced (§3.1)
                                           // §3.1: a clean accept RELEASES the fence only once APPLIED (PENDING_APPLY holds until then).
    sqlx::query("UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    // Fence released at APPLIED → a NEW reservation on the FN is ACCEPTED.
    insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect("2nd reservation after clean accept APPLIED must be accepted (fence released)");
    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap()
        .expect("the new reservation is the active one");
    assert_eq!(active.reservation_id, [0x02; 16]);
}

#[tokio::test]
async fn f05_second_reservation_accepted_after_not_submitted() {
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'NOT_SUBMITTED', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'PreconditionFailed' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    // §3.1: PENDING_APPLY still holds the fence; NOT_SUBMITTED releases only at APPLIED.
    sqlx::query("UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    insert_res(&pool, new_res(0x02, doc2, FN_A))
        .await
        .expect("2nd reservation after NOT_SUBMITTED cancel (APPLIED) must be accepted");
}

// ═══════════════════════════ immutability ═══════════════════════════

#[tokio::test]
async fn i01_settled_routing_null_to_terminalreject_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    drive_clean_accept(&pool, 0x01).await; // OO SUBMITTED, routing NULL (settled)
                                           // Attempt to mutate a settled OO row's routing_class NULL→TerminalReject.
    let err = sqlx::query(
        "UPDATE delivery_reservation SET routing_class = 'TerminalReject' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("settled OO routing NULL→value must be blocked (immutable)");
    // Blocked by the 032 immutability trigger AND the 036 matrix (Accepted requires routing NULL).
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint") || err_has(&err, "evidence"),
        "{err}"
    );
}

#[tokio::test]
async fn i02_binding_mutation_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // Every immutable protocol-binding column: mutating it must be blocked.
    for (col, val) in [
        ("dps_protocol_id", "'EVPZ_DPS'"),
        ("protocol_contract_version", "2"),
        ("capability_profile_version", "3"),
        ("endpoint_config_revision", "4"),
    ] {
        let sql = format!("UPDATE delivery_reservation SET {col} = {val} WHERE reservation_id = ?");
        let err = sqlx::query(&sql)
            .bind(&[0x01u8; 16][..])
            .execute(&pool)
            .await
            .unwrap_err();
        assert!(
            err_has(&err, "immutable") || err_has(&err, "constraint"),
            "mutating {col} must be blocked (immutable), got: {err}"
        );
    }
}

#[tokio::test]
async fn i03_identity_mutation_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // Mutate fiscal_number → immutable trigger.
    let err =
        sqlx::query("UPDATE delivery_reservation SET fiscal_number = ? WHERE reservation_id = ?")
            .bind(FN_B)
            .bind(&[0x01u8; 16][..])
            .execute(&pool)
            .await
            .expect_err("fiscal_number mutation must be blocked (immutable)");
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn i04_remote_correlation_id_mutation_after_oo_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // Reach OO WITH a remote_correlation_id set at the OO step. Under the 036 evidence matrix
    // the ONLY leaf that carries a correlation is Accepted (correlation = evidence_text = F),
    // so the correlation-bearing OO row is a clean accept (routing NULL, F='4000000001').
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED', \
             response_provenance = 'PARSED_DPS_ENVELOPE', \
             remote_correlation_id = '4000000001', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'Accepted', evidence_text = '4000000001' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("setting remote_correlation_id (= F) at the OO step is legal for a clean accept");
    // Now mutating it once OO is blocked — by the 032 immutability trigger AND the 036 matrix
    // (Accepted requires correlation = evidence_text, so 'corr-2' ≠ '4000000001' also aborts).
    let err = sqlx::query(
        "UPDATE delivery_reservation SET remote_correlation_id = 'corr-2' WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect_err("remote_correlation_id mutation after OO must be blocked (immutable)");
    assert!(
        err_has(&err, "immutable") || err_has(&err, "constraint") || err_has(&err, "evidence"),
        "{err}"
    );
}

// ═══════════════════════════ append-only + collision-guard ═══════════════════════════

#[tokio::test]
async fn a01_delete_rejected_append_only() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // Even a settled (resolved) row cannot be deleted.
    drive_clean_accept(&pool, 0x01).await;
    let err = sqlx::query("DELETE FROM delivery_reservation WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .expect_err("DELETE must be blocked (append-only) — no attempt_no reuse");
    assert!(
        err_has(&err, "append-only") || err_has(&err, "constraint"),
        "{err}"
    );
}

/// Byte-snapshot of a row's every column (for the "byte-identical after a
/// rejected collision" assertion).  Each column is read via `quote()` +
/// `hex()`-agnostic `CAST(... AS BLOB)`? — simplest robust form: read the
/// column as `Option<Vec<u8>>` so BLOB / TEXT / INT all decode losslessly
/// (SQLite returns the raw stored bytes for any affinity).
async fn row_snapshot(pool: &SqlitePool, res_byte: u8) -> Vec<(String, Option<Vec<u8>>)> {
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(delivery_reservation)")
            .fetch_all(pool)
            .await
            .unwrap();
    let mut out = Vec::new();
    for c in cols {
        let name = c.1;
        // quote() renders any value (BLOB/TEXT/INT/NULL) to a canonical SQL
        // literal string, so it round-trips losslessly and decodes as UTF-8.
        let sql =
            format!("SELECT quote({name}) FROM delivery_reservation WHERE reservation_id = ?");
        let v: Option<String> = sqlx::query_scalar(&sql)
            .bind(&[res_byte; 16][..])
            .fetch_one(pool)
            .await
            .unwrap();
        out.push((name, v.map(|s| s.into_bytes())));
    }
    out
}

#[tokio::test]
async fn a02_insert_or_replace_by_reservation_id_rejected_no_eviction() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    let before = row_snapshot(&pool, 0x01).await;

    // INSERT OR REPLACE with the SAME reservation_id → collision-guard ABORTs
    // (no DELETE+INSERT eviction, original row survives byte-identical).
    let err = sqlx::query(
        "INSERT OR REPLACE INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 9, 'EVPZ_DPS', 5, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xCDu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("INSERT OR REPLACE on existing reservation_id must be blocked");
    assert!(
        err_has(&err, "collision") || err_has(&err, "constraint"),
        "{err}"
    );

    let after = row_snapshot(&pool, 0x01).await;
    assert_eq!(
        before, after,
        "original row must be byte-identical after rejected REPLACE"
    );
}

#[tokio::test]
async fn a03_insert_or_replace_by_doc_attempt_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap(); // (doc, attempt 1)
                                                                // Different reservation_id but SAME (document_id, attempt_no=1) → collision-guard.
    let err = sqlx::query(
        "INSERT OR REPLACE INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x99u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("INSERT OR REPLACE on existing (document_id,attempt_no) must be blocked");
    assert!(
        err_has(&err, "collision") || err_has(&err, "constraint"),
        "{err}"
    );
}

#[tokio::test]
async fn a04_insert_or_replace_evicting_active_fn_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc1 = seed_doc(&pool, FN_A, 0x11, 1).await;
    let doc2 = seed_doc(&pool, FN_A, 0x22, 2).await;
    insert_res(&pool, new_res(0x01, doc1, FN_A)).await.unwrap();
    let before = row_snapshot(&pool, 0x01).await;
    // New reservation_id, new (doc2, attempt 1) — no PK/UNIQUE collision — BUT the
    // FN is fenced by the first row. The active-FN clause of the collision-guard ABORTs.
    let err = sqlx::query(
        "INSERT OR REPLACE INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 1, 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x02u8; 16][..])
    .bind(doc2.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("INSERT OR REPLACE that would evict an active-FN reservation must be blocked");
    assert!(
        err_has(&err, "collision") || err_has(&err, "constraint"),
        "{err}"
    );
    let after = row_snapshot(&pool, 0x01).await;
    assert_eq!(
        before, after,
        "active reservation must survive byte-identical"
    );
}

#[tokio::test]
async fn a05_insert_or_ignore_and_upsert_rejected() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    let before = row_snapshot(&pool, 0x01).await;

    // INSERT OR IGNORE on same reservation_id — the collision-guard is a BEFORE
    // INSERT trigger that RAISE(ABORT)s; ABORT is NOT suppressed by OR IGNORE
    // (OR IGNORE only suppresses constraint violations, not explicit RAISE(ABORT)).
    let err = sqlx::query(
        "INSERT OR IGNORE INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 2, 'FSCO_ZZD', 1, ?)",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("INSERT OR IGNORE on colliding reservation_id must be blocked");
    assert!(
        err_has(&err, "collision") || err_has(&err, "constraint"),
        "{err}"
    );

    // UPSERT ON CONFLICT DO UPDATE on the reservation_id PK — collision-guard ABORTs
    // (BEFORE INSERT fires before conflict resolution).
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 3, 'FSCO_ZZD', 1, ?) \
         ON CONFLICT(reservation_id) DO UPDATE SET fiscal_number = excluded.fiscal_number",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("UPSERT DO UPDATE on colliding reservation_id must be blocked");
    assert!(
        err_has(&err, "collision") || err_has(&err, "constraint"),
        "{err}"
    );

    // UPSERT ON CONFLICT DO NOTHING — same, ABORT is not suppressed.
    let err = sqlx::query(
        "INSERT INTO delivery_reservation \
            (reservation_id, document_id, fiscal_number, attempt_no, dps_protocol_id, \
             protocol_contract_version, envelope_hash) \
         VALUES (?, ?, ?, 4, 'FSCO_ZZD', 1, ?) \
         ON CONFLICT(reservation_id) DO NOTHING",
    )
    .bind(&[0x01u8; 16][..])
    .bind(doc.as_bytes().to_vec())
    .bind(FN_A)
    .bind(&[0xABu8; 32][..])
    .execute(&pool)
    .await
    .expect_err("UPSERT DO NOTHING on colliding reservation_id must be blocked");
    assert!(
        err_has(&err, "collision") || err_has(&err, "constraint"),
        "{err}"
    );

    let after = row_snapshot(&pool, 0x01).await;
    assert_eq!(
        before, after,
        "original row byte-identical after all rejected variants"
    );
}

// ═══════════════════════════ composite-FK ═══════════════════════════

#[tokio::test]
async fn c01_fk_mismatch_doc_fn_a_under_fence_fn_b_rejected() {
    let (_d, pool) = fresh_pool().await;
    // doc lives under FN_A; try to reserve it declaring FN_B → composite FK fails
    // (no fiscal_documents row with (doc, FN_B)).
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    seed_fn(&pool, FN_B).await;
    let err = insert_res(&pool, new_res(0x01, doc, FN_B))
        .await
        .expect_err("doc FN-A under reservation FN-B must violate composite FK");
    let m = err.to_string().to_lowercase();
    assert!(m.contains("foreign key") || m.contains("constraint"), "{m}");
}

#[tokio::test]
async fn c02_parent_delete_blocked_while_referenced() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    // ON DELETE RESTRICT blocks deleting the parent fiscal_documents row.
    let err = sqlx::query("DELETE FROM fiscal_documents WHERE document_id = ?")
        .bind(doc.as_bytes().to_vec())
        .execute(&pool)
        .await
        .expect_err("ON DELETE RESTRICT must block parent delete while referenced");
    let m = err.to_string().to_lowercase();
    assert!(m.contains("foreign key") || m.contains("constraint"), "{m}");
}

// ═══════════════════════════ blessed ═══════════════════════════

#[tokio::test]
async fn b01_submitted_unknown_no_response_transient_retry_accepted() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    advance_to_cs(&pool, 0x01).await.unwrap();
    // The CANONICAL wire-timeout: bytes may have left, no ack came back.
    // SUBMITTED_UNKNOWN + NO_RESPONSE + TransientRetry is VALID (blessed §3).
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'SUBMITTED_UNKNOWN', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'NoResponse', evidence_text = 'Timeout' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .expect("SUBMITTED_UNKNOWN + NO_RESPONSE + TransientRetry is the blessed canonical timeout");
    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap();
    assert!(active.is_some(), "canonical timeout keeps the fence held");
}

// ═══════════════════════════ concurrent attempt_no ═══════════════════════════

#[tokio::test]
async fn n01_sequential_attempts_increment_after_cancel() {
    let (_d, pool) = fresh_pool().await;
    let doc = seed_doc(&pool, FN_A, 0x11, 1).await;
    // attempt 1
    let a1 = insert_res(&pool, new_res(0x01, doc, FN_A)).await.unwrap();
    assert_eq!(a1, 1);
    // Release the fence (NOT_SUBMITTED) so a 2nd reservation on the same doc is allowed.
    sqlx::query(
        "UPDATE delivery_reservation \
         SET state = 'OUTCOME_OBSERVED', submission_certainty = 'NOT_SUBMITTED', \
             response_provenance = 'NO_RESPONSE', routing_class = 'TransientRetry', \
             apply_state = 'PENDING_APPLY', node_effect = 'NoNodeEffect', \
             evidence_kind = 'PreconditionFailed' \
         WHERE reservation_id = ?",
    )
    .bind(&[0x01u8; 16][..])
    .execute(&pool)
    .await
    .unwrap();
    // §3.1: release the fence at APPLIED (attempt 1 never started, so call-once allows a 2nd).
    sqlx::query("UPDATE delivery_reservation SET apply_state = 'APPLIED' WHERE reservation_id = ?")
        .bind(&[0x01u8; 16][..])
        .execute(&pool)
        .await
        .unwrap();
    // attempt 2 for the SAME document — repo computes MAX(attempt_no)+1 = 2.
    let a2 = insert_res(&pool, new_res(0x02, doc, FN_A)).await.unwrap();
    assert_eq!(
        a2, 2,
        "attempt_no must increment to 2 for the same document"
    );
}

// ═══════════════════════════ §6 merge pins ═══════════════════════════

/// A representative NON-EMPTY DB at schema 031: seed several fiscal_documents,
/// a shift, offline session, etc. through the normal migrations, then snapshot
/// sqlite_master, upgrade to 032, and prove pre-existing objects are byte-
/// identical + only the expected new objects appear.
#[tokio::test]
async fn p01_upgrade_on_nonempty_db_sqlite_master_diff() {
    // We cannot easily open at 031-only (open_pool always runs all migrations),
    // so we simulate: open a fresh pool at 032, seed representative rows, then
    // assert the pre-existing (< 032) objects are exactly those of a fresh
    // 031-state schema, and the 032 objects are exactly the expected new set.
    //
    // To get a genuine "before" we compute the 031 sqlite_master by reading the
    // canonical object set from a DB where we then DROP the 032 objects and diff.
    let (_d, pool) = fresh_pool().await;

    // Seed a representative non-empty DB (parent rows the FK will reference).
    let _doc1 = seed_doc(&pool, FN_A, 0xA1, 1).await;
    let _doc2 = seed_doc(&pool, FN_A, 0xA2, 2).await;
    let _doc3 = seed_doc(&pool, FN_B, 0xB1, 1).await;

    // Full sqlite_master (name, sql) for every non-032, non-sqlx object.
    let all: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT type, name, sql FROM sqlite_master \
         WHERE name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx%' ORDER BY type, name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let new_objects: HashSet<&str> = [
        "delivery_reservation",
        "ux_fd_docid_fn",
        "ux_reservation_active",
        // 035 adds the per-document lifetime call-once index (P2); like the other
        // reservation objects, the control DB drops the table (auto-dropping this
        // partial index), so it belongs in the filtered-out reservation-object set.
        "ux_delivery_document_ever_started",
        "ix_reservation_call_started",
        "delivery_reservation_insert_state",
        "delivery_reservation_no_replace",
        "delivery_reservation_transition",
        "delivery_reservation_immutable",
        "delivery_reservation_append_only",
        "delivery_reservation_updated_at",
        // 033 adds one more trigger ON delivery_reservation; the control DB drops the
        // table (auto-dropping this trigger), so it belongs in the reservation-object
        // set filtered out of the pre-existing byte-identity comparison.
        "delivery_reservation_apply_state_transition",
        // 034 adds four more triggers ON delivery_reservation (H2/H3/H4 authority
        // integrity). Like the 033 trigger, the control DB drops the table and
        // auto-drops these, so they belong in the filtered-out reservation-object set.
        // (034's H1 trigger `node_state_delivery_generation_monotone` is ON node_state,
        // NOT delivery_reservation — it survives in BOTH DBs and stays in the
        // byte-identity comparison, so it is deliberately NOT listed here.)
        "delivery_reservation_cs_pairing_insert",
        "delivery_reservation_cs_pairing_update",
        "delivery_reservation_oo_completeness",
        "delivery_reservation_clean_accept_node_effect",
        // 036 adds three more triggers ON delivery_reservation (Slice 2a evidence
        // union: pre-OO/insert null-guard, fail-closed matrix, evidence immutability).
        // The control DB's DROP TABLE auto-drops them, so they belong in the
        // filtered-out reservation-object set. (036's four evidence COLUMNS are part
        // of the delivery_reservation table DDL, which is already filtered out via the
        // table itself — only the triggers are separate sqlite_master rows.)
        "delivery_reservation_evidence_insert",
        "delivery_reservation_evidence_matrix_update",
        "delivery_reservation_evidence_immutable",
        // 037 adds the mandatory-at-OO trigger ON delivery_reservation (evidence_kind NOT
        // NULL at OUTCOME_OBSERVED). Like the other reservation triggers, the control DB's
        // DROP TABLE auto-drops it, so it belongs in the filtered-out reservation-object set.
        "delivery_reservation_evidence_mandatory_update",
    ]
    .into_iter()
    .collect();

    // Every 032 object present.
    let present: HashSet<String> = all.iter().map(|r| r.1.clone()).collect();
    for o in &new_objects {
        assert!(
            present.contains(*o),
            "032 object {o} missing from sqlite_master"
        );
    }

    // ux_fd_docid_fn is specifically the additive index ON fiscal_documents.
    let ux = all
        .iter()
        .find(|r| r.1 == "ux_fd_docid_fn")
        .expect("ux_fd_docid_fn present");
    let ux_sql = ux.2.clone().unwrap_or_default();
    assert!(
        ux_sql.contains("fiscal_documents") && ux_sql.contains("document_id"),
        "ux_fd_docid_fn must be the unique index on fiscal_documents(document_id, fiscal_number): {ux_sql}"
    );

    // The pre-existing (< 032) objects: prove their DDL text matches a control
    // DB that carries NO 032 objects (we get the control by dropping the 032
    // objects from a second fresh pool).  This is the byte-identity pin.
    let (_d2, control) = fresh_pool().await;
    for tg in [
        "delivery_reservation_updated_at",
        "delivery_reservation_append_only",
        "delivery_reservation_immutable",
        "delivery_reservation_transition",
        "delivery_reservation_no_replace",
        "delivery_reservation_insert_state",
    ] {
        sqlx::query(&format!("DROP TRIGGER {tg}"))
            .execute(&control)
            .await
            .unwrap();
    }
    for ix in ["ix_reservation_call_started", "ux_reservation_active"] {
        sqlx::query(&format!("DROP INDEX {ix}"))
            .execute(&control)
            .await
            .unwrap();
    }
    sqlx::query("DROP TABLE delivery_reservation")
        .execute(&control)
        .await
        .unwrap();
    sqlx::query("DROP INDEX ux_fd_docid_fn")
        .execute(&control)
        .await
        .unwrap();

    let control_objs: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT type, name, sql FROM sqlite_master \
         WHERE name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx%' ORDER BY type, name",
    )
    .fetch_all(&control)
    .await
    .unwrap();

    // The pre-existing objects in the 032 DB must be byte-identical to the
    // control (031-equivalent) DB — and the ONLY difference is the new set.
    let pre_existing: Vec<_> = all
        .iter()
        .filter(|r| !new_objects.contains(r.1.as_str()))
        .cloned()
        .collect();
    assert_eq!(
        pre_existing, control_objs,
        "pre-032 sqlite_master objects must be byte-identical to the 031-equivalent schema"
    );
}

/// Production-flow pin: a normal fiscalisation touches fiscal_documents but
/// NEVER writes delivery_reservation in CS-2 (INACTIVE — no caller).  We
/// simulate a normal fiscalisation by seeding docs the way the write-path
/// would, and assert the reservation table stays empty.
#[tokio::test]
async fn p02_normal_fiscalisation_leaves_reservation_empty() {
    let (_d, pool) = fresh_pool().await;
    // "Normal fiscalisation" surrogate: several fiscal_documents rows created
    // and advanced (as the write-path does) — none of which the INACTIVE repo
    // is wired into.
    let _ = seed_doc(&pool, FN_A, 0x11, 1).await;
    let _ = seed_doc(&pool, FN_A, 0x22, 2).await;
    let _ = seed_doc(&pool, FN_B, 0x33, 1).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM delivery_reservation")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "delivery_reservation stays EMPTY after fiscalisation (INACTIVE)"
    );

    let active = delivery_reservation::get_active_for_fn(&pool, FN_A)
        .await
        .unwrap();
    assert!(
        active.is_none(),
        "no active reservation for any FN post-fiscalisation"
    );
}

// ─── p03: static pin — no production caller of the still-INACTIVE S7-1 lifecycle ──────────
//
// The structural scanner lives in `support/inactive_lifecycle_scan.rs` (shared with
// `migration_033::rg08`). Retargeted after S7-2 (`3f69af5`): `fn_fence_active_tx` is the
// intentionally-ACTIVE fence read-helper, so the pin targets the INACTIVE S7-1 *lifecycle* symbols
// (authorize/record/apply/resume/list/insert) STRUCTURALLY (syn AST, not grep), excluding
// `#[cfg(test)]` scaffolding but NOT `#[cfg(not(test))]`. Teeth: the `p03_bite_*` synthetic controls.

/// p03 (merge pin §6.4, retargeted): the INACTIVE S7-1 delivery lifecycle has NO production caller.
#[test]
fn p03_no_production_caller_static_pin() {
    let hits = scan_src_tree(env!("CARGO_MANIFEST_DIR"));
    assert!(
        hits.is_empty(),
        "the INACTIVE S7-1 delivery lifecycle (authorize/record/apply/resume/list/insert) must have \
         NO production caller — `fn_fence_active_tx` (S7-2 fence) and `complete_operator_pending` \
         (gap 4a) are the sanctioned ACTIVE exceptions; found: {hits:#?}"
    );
}

// ─── p03 synthetic bite controls (parsed inline; production scan stays GREEN) ──────────────

#[test]
fn p03_bite_production_record_outcome_call_is_flagged() {
    let snippet = r#"
        async fn caller(tx: &mut Tx) {
            let _ = crate::db::repositories::delivery_reservation::record_outcome(tx, o, e).await;
        }
    "#;
    let hits = scan_inactive_lifecycle_refs(snippet, "<prod_record>");
    assert!(
        hits.iter().any(|h| h.detail.contains("record_outcome")),
        "a production record_outcome call must be flagged: {hits:#?}"
    );
}

#[test]
fn p03_bite_production_qualified_insert_is_flagged() {
    let snippet = r#"
        async fn caller(tx: &mut Tx) {
            let _ = delivery_reservation::insert(tx, row).await;
        }
    "#;
    let hits = scan_inactive_lifecycle_refs(snippet, "<prod_insert>");
    assert!(
        hits.iter()
            .any(|h| h.detail.contains("delivery_reservation::insert")),
        "a production delivery_reservation::insert call must be flagged: {hits:#?}"
    );
}

#[test]
fn p03_bite_cfg_test_insert_is_not_flagged() {
    // The SAME forbidden call inside a #[cfg(test)] module is test scaffolding → GREEN.
    let snippet = r#"
        #[cfg(test)]
        mod tests {
            async fn seed(tx: &mut Tx) {
                let _ = super::delivery_reservation::insert(tx, row).await;
            }
        }
    "#;
    let hits = scan_inactive_lifecycle_refs(snippet, "<cfg_test_insert>");
    assert!(
        hits.is_empty(),
        "a #[cfg(test)] lifecycle call must NOT be flagged: {hits:#?}"
    );
}

#[test]
fn p03_bite_cfg_not_test_call_is_flagged() {
    // #[cfg(not(test))] is PRODUCTION — a lifecycle call there IS flagged (exclusion is cfg(test)
    // only, never cfg(not(test))).
    let snippet = r#"
        #[cfg(not(test))]
        mod prod {
            async fn caller(tx: &mut Tx) {
                let _ = super::delivery_reservation::record_outcome(tx).await;
            }
        }
    "#;
    let hits = scan_inactive_lifecycle_refs(snippet, "<cfg_not_test>");
    assert!(
        hits.iter().any(|h| h.detail.contains("record_outcome")),
        "#[cfg(not(test))] is production — must be flagged: {hits:#?}"
    );
}

#[test]
fn p03_bite_production_fn_fence_active_tx_is_not_flagged() {
    // The S7-2 fence read-helper is intentionally active — must NOT be flagged.
    let snippet = r#"
        async fn writer(tx: &mut Tx) -> anyhow::Result<()> {
            if delivery_reservation::fn_fence_active_tx(tx, fn_id).await? {
                return Ok(());
            }
            Ok(())
        }
    "#;
    let hits = scan_inactive_lifecycle_refs(snippet, "<prod_fence>");
    assert!(
        hits.is_empty(),
        "fn_fence_active_tx (S7-2 active fence) must NOT be flagged: {hits:#?}"
    );
}

#[test]
fn p03_bite_production_complete_operator_pending_is_not_flagged() {
    // The sanctioned gap-4a operator entrypoint — active, must NOT be flagged.
    let snippet = r#"
        async fn orchestrate(tx: &mut Tx) {
            let _ = delivery_reservation::complete_operator_pending(tx, id, res).await;
        }
    "#;
    let hits = scan_inactive_lifecycle_refs(snippet, "<prod_complete>");
    assert!(
        hits.is_empty(),
        "complete_operator_pending (gap 4a) must NOT be flagged: {hits:#?}"
    );
}

#[test]
fn p03_bite_use_import_of_lifecycle_symbol_is_flagged() {
    // A bare `use` import (even renamed) of a lifecycle symbol is a production reference → RED.
    let snippet = r#"
        use crate::db::repositories::delivery_reservation::apply_outcome as _aliased;
    "#;
    let hits = scan_inactive_lifecycle_refs(snippet, "<use_import>");
    assert!(
        hits.iter().any(|h| h.detail.contains("apply_outcome")),
        "a use-import (even aliased) of a lifecycle symbol must be flagged: {hits:#?}"
    );
}

/// A3 (S7-1 §7.2): the boot-first reservation pass is the SANCTIONED production caller of the
/// still-INACTIVE lifecycle helpers — but ONLY of the read/resume subset. This positive
/// allowlist pin guards the file-wide exclusion in `scan_src_tree` (so the exclusion is not a
/// denylist hole): the pass must reference EXACTLY the read/resume subset and NEVER a
/// mint/authority symbol (`authorize_submission` / `record_outcome` / `delivery_reservation::insert`
/// / `get_active_for_fn`), which stay cutover-only.
///
/// S7-1 B3 (re-audit fix): the boot APPLY now routes through
/// `apply_orchestration::apply_recorded_outcome` (which fires the online shift edges 3/10), NOT the
/// bare repo `apply_outcome` — so the pass no longer references `apply_outcome` directly, and this
/// pin asserts the corrected wiring positively.
#[test]
fn boot_pass_references_only_the_sanctioned_read_apply_subset() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/services/reconciliation/reservation_boot_pass.rs"
    );
    let text = std::fs::read_to_string(path).expect("read reservation_boot_pass.rs");
    let details: Vec<String> = scan_inactive_lifecycle_refs(&text, "reservation_boot_pass.rs")
        .into_iter()
        .map(|h| h.detail)
        .collect();
    // Match the EXACT backticked symbol the scanner records — never a substring.
    let refs = |sym: &str| details.iter().any(|d| d.contains(&format!("`{sym}`")));

    // Load-bearing: the pass DOES reference the read/resume subset — proves the file-wide
    // scan exclusion is real, not vacuous.
    for sym in [
        "resume_crashed_reservation",
        "list_call_started_without_outcome",
        "list_outcome_observed_pending_apply",
    ] {
        assert!(
            refs(sym),
            "boot pass must reference `{sym}` (§7.2): {details:#?}"
        );
    }
    // S7-1 B3: the APPLY step is routed through the shared orchestration (which fires shift edges
    // 3/10), NOT the bare repo `apply_outcome`. Assert the corrected wiring positively.
    assert!(
        text.contains("apply_orchestration::apply_recorded_outcome"),
        "boot pass must route apply through `apply_orchestration::apply_recorded_outcome` (S7-1 B3 — \
         the bare `apply_outcome` skips the online shift edges and strands SHIFT_OPEN/Z in Opening/Closing)"
    );
    // Forbidden: the pass must NEVER touch a mint / authority symbol — those activate at cutover.
    for sym in [
        "authorize_submission",
        "record_outcome",
        "delivery_reservation::insert",
        "get_active_for_fn",
    ] {
        assert!(
            !refs(sym),
            "boot pass must NOT reference the mint/authority symbol `{sym}` (cutover-only): {details:#?}"
        );
    }
}

/// CS-3 S7-1 Slice-4 CUTOVER — positive pin closing the `stage_send.rs` file-wide scan exclusion.
/// The live cutover activates `authorize_submission` (4-pre mint) + `record_outcome` (record tx) in
/// `stage_send::run_one_attempt`, and NOTHING ELSE from the delivery lifecycle — the apply projection
/// goes through `apply_orchestration::apply_recorded_outcome` (not a scanned symbol), never the repo
/// `apply_outcome`; boot-resume / list / active-read are the reconciliation pass's territory.
#[test]
fn stage_send_references_only_the_authorize_record_subset() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/services/write_path/stage_send.rs"
    );
    let text = std::fs::read_to_string(path).expect("read stage_send.rs");
    let details: Vec<String> = scan_inactive_lifecycle_refs(&text, "stage_send.rs")
        .into_iter()
        .map(|h| h.detail)
        .collect();
    let refs = |sym: &str| details.iter().any(|d| d.contains(&format!("`{sym}`")));

    // Load-bearing: the cutover DOES reference the mint + record symbols — proves the exclusion is
    // real, not vacuous.
    for sym in ["authorize_submission", "record_outcome"] {
        assert!(
            refs(sym),
            "stage_send must reference the sanctioned cutover symbol `{sym}`: {details:#?}"
        );
    }
    // Forbidden: stage_send must NEVER call the repo apply / boot-resume / list / active-read / insert
    // symbols directly.
    for sym in [
        "apply_outcome",
        "resume_crashed_reservation",
        "list_call_started_without_outcome",
        "list_outcome_observed_pending_apply",
        "get_active_for_fn",
        "delivery_reservation::insert",
    ] {
        assert!(
            !refs(sym),
            "stage_send must NOT reference `{sym}` directly (apply goes via apply_orchestration): \
             {details:#?}"
        );
    }
}

/// CS-3 S7-1 Q3 — positive pin closing the `apply_orchestration.rs` file-wide scan exclusion. The
/// shared orchestration wraps EXACTLY `apply_outcome` (with the online shift edges) and touches no
/// mint / record / boot-resume / list symbol.
#[test]
fn apply_orchestration_references_only_the_apply_subset() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/services/write_path/apply_orchestration.rs"
    );
    let text = std::fs::read_to_string(path).expect("read apply_orchestration.rs");
    let details: Vec<String> = scan_inactive_lifecycle_refs(&text, "apply_orchestration.rs")
        .into_iter()
        .map(|h| h.detail)
        .collect();
    let refs = |sym: &str| details.iter().any(|d| d.contains(&format!("`{sym}`")));

    // Load-bearing: the orchestration DOES wrap `apply_outcome`.
    assert!(
        refs("apply_outcome"),
        "apply_orchestration must reference `apply_outcome` (it wraps it with the shift edges): \
         {details:#?}"
    );
    // Forbidden: it must NEVER mint / record / boot-resume / list.
    for sym in [
        "authorize_submission",
        "record_outcome",
        "resume_crashed_reservation",
        "list_call_started_without_outcome",
        "list_outcome_observed_pending_apply",
        "get_active_for_fn",
        "delivery_reservation::insert",
    ] {
        assert!(
            !refs(sym),
            "apply_orchestration must NOT reference the mint/record/boot symbol `{sym}`: {details:#?}"
        );
    }
}
