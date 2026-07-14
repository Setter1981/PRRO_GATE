//! W4-Z2a piece 6b mid-fix — `pin_signing_inputs_tx` COALESCE
//! semantics for `signing_config_snapshot_id`.
//!
//! Locked-design rule: INSERT-set FK is authoritative.  Pin must
//! NEVER drift it via overwrite from a downstream caller.  Truth
//! table:
//!   existing Some(X) + caller None     → keeps X  (no wipe)
//!   existing Some(X) + caller Some(Y)  → keeps X  (no drift)
//!   existing NULL    + caller Some(Y)  → Y        (legacy backfill)
//!   existing NULL    + caller None     → NULL     (unchanged)

use prro::db::models::enums::DocType;
use prro::db::models::ids::{DocumentId, RequestId};
use prro::db::open_pool;
use prro::db::repositories::fiscal_documents::{self as fd, NewDocument};
use prro::db::repositories::fiscal_number_config::{self as fn_repo, NewFnConfig};
use prro::db::tx::with_immediate;
use prro::db::types::DbDocumentId;

const FN: &str = "4000000099";

async fn fresh_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pin-coalesce.db");
    std::mem::forget(dir);
    open_pool(&path).await.unwrap()
}

async fn seed_fn(pool: &sqlx::SqlitePool) {
    fn_repo::insert(
        pool,
        &NewFnConfig {
            fiscal_number: FN.into(),
            tax_number: "12345678".into(),
            vat_payer_inn: None,
            fiscal_mode: prro::db::models::enums::FiscalMode::Test,
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
}

async fn seed_doc_with_snapshot(pool: &sqlx::SqlitePool, snapshot_id: Option<i64>) -> DocumentId {
    seed_fn(pool).await;
    // First need to ensure a signing_config_snapshots row exists for
    // the FK constraint.  Insert a stub row id=1 if test requests
    // Some(_) — sqlite AUTOINCREMENT yields id=1 on first insert.
    if let Some(id) = snapshot_id {
        sqlx::query(
            "INSERT INTO signing_config_snapshots \
             (id, fn, driver_id, kind, payload_json, payload_sha256, created_at) \
             VALUES (?, ?, 'test-driver', 'check_tax_mapping_v1', '{}', \
                     X'0000000000000000000000000000000000000000000000000000000000000000', \
                     '2026-05-28T00:00:00Z')",
        )
        .bind(id)
        .bind(FN)
        .execute(pool)
        .await
        .unwrap();
    }
    let doc_id = DocumentId::new();
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            fd::insert_prepared_tx(
                tx,
                &NewDocument {
                    document_id: doc_id,
                    request_id: RequestId::new(),
                    fiscal_number: FN.into(),
                    shift_id: None,
                    offline_session_id: None,
                    lnd: 1,
                    doc_type: DocType::Sell,
                    backend_profile_id: "b".into(),
                    transport_profile_id: "t".into(),
                    fs_mode: "ONLINE",
                    business_ts: "2026-05-28T12:00:00Z".into(),
                    total_sum_kop: Some(1000),
                    payload_json: r#"{"goods":[]}"#.into(),
                    payload_sha256_canonical: [0u8; 32],
                    source_sha256: [0u8; 32],
                    unsigned_xml_sha256: None,
                    previous_hash: None,
                    signed_by_cashier_id: None,
                    signing_config_snapshot_id: snapshot_id,
                },
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .unwrap();
    doc_id
}

async fn fetch_snapshot_id(pool: &sqlx::SqlitePool, doc_id: DocumentId) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT signing_config_snapshot_id FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc_id))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn pin_with(pool: &sqlx::SqlitePool, doc_id: DocumentId, caller_snapshot: Option<i64>) {
    // Need snapshot row for FK if backfill case (caller_snapshot Some(Y), Y not yet existing).
    if let Some(y) = caller_snapshot {
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM signing_config_snapshots WHERE id = ?")
                .bind(y)
                .fetch_one(pool)
                .await
                .unwrap();
        if exists == 0 {
            sqlx::query(
                "INSERT INTO signing_config_snapshots \
                 (id, fn, driver_id, kind, payload_json, payload_sha256, created_at) \
                 VALUES (?, ?, 'test-driver', 'check_tax_mapping_v1', '{}', \
                         X'1111111111111111111111111111111111111111111111111111111111111111', \
                         '2026-05-28T00:00:00Z')",
            )
            .bind(y)
            .bind(FN)
            .execute(pool)
            .await
            .unwrap();
        }
    }
    with_immediate(pool, move |tx| {
        Box::pin(async move {
            let _ = fd::pin_signing_inputs_tx(tx, doc_id, None, None, caller_snapshot).await?;
            Ok::<_, anyhow::Error>(())
        })
    })
    .await
    .unwrap();
}

// ─── Truth-table cases ──────────────────────────────────────────────

#[tokio::test]
async fn pin_existing_some_caller_none_preserves_existing() {
    let pool = fresh_pool().await;
    let doc_id = seed_doc_with_snapshot(&pool, Some(1)).await;
    pin_with(&pool, doc_id, None).await;
    assert_eq!(
        fetch_snapshot_id(&pool, doc_id).await,
        Some(1),
        "pin caller None MUST NOT wipe INSERT-set FK"
    );
}

#[tokio::test]
async fn pin_existing_some_caller_some_other_preserves_existing_no_drift() {
    // THE drift-prevention test: prevents future refactor reverting
    // to reviewer's `COALESCE(?, existing)` form which would overwrite.
    let pool = fresh_pool().await;
    let doc_id = seed_doc_with_snapshot(&pool, Some(1)).await;
    // Seed a second snapshot row (id=2) so FK is valid if drift bug exists.
    sqlx::query(
        "INSERT INTO signing_config_snapshots \
         (id, fn, driver_id, kind, payload_json, payload_sha256, created_at) \
         VALUES (2, ?, 'test-driver', 'check_tax_mapping_v1', '{}', \
                 X'2222222222222222222222222222222222222222222222222222222222222222', \
                 '2026-05-28T00:00:00Z')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();
    pin_with(&pool, doc_id, Some(2)).await;
    assert_eq!(
        fetch_snapshot_id(&pool, doc_id).await,
        Some(1),
        "pin caller Some(other) MUST NOT drift INSERT-set FK — locked-design rule"
    );
}

#[tokio::test]
async fn pin_existing_null_caller_some_backfills() {
    let pool = fresh_pool().await;
    let doc_id = seed_doc_with_snapshot(&pool, None).await;
    pin_with(&pool, doc_id, Some(7)).await;
    assert_eq!(
        fetch_snapshot_id(&pool, doc_id).await,
        Some(7),
        "NULL FK with pin caller Some(Y) MUST backfill to Y (legacy / pre-W4-Z2a doc path)"
    );
}

#[tokio::test]
async fn pin_existing_null_caller_none_stays_null() {
    let pool = fresh_pool().await;
    let doc_id = seed_doc_with_snapshot(&pool, None).await;
    pin_with(&pool, doc_id, None).await;
    assert_eq!(
        fetch_snapshot_id(&pool, doc_id).await,
        None,
        "NULL + None remains NULL — piece 8/9 classifies via NULL-semantics rule"
    );
}

// ─── Piece 8a — WHERE-guard drift rejection (CRIT-1 close) ──────────

/// Helper: fetch full pin state for invariant-preservation assertions.
async fn fetch_pin_state(
    pool: &sqlx::SqlitePool,
    doc_id: DocumentId,
) -> (Option<i64>, Option<String>) {
    let row = sqlx::query(
        "SELECT signing_config_snapshot_id, signing_inputs_pinned_at \
         FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(DbDocumentId(doc_id))
    .fetch_one(pool)
    .await
    .unwrap();
    use sqlx::Row;
    (
        row.try_get::<Option<i64>, _>(0).unwrap(),
        row.try_get::<Option<String>, _>(1).unwrap(),
    )
}

/// W4-Z2a piece 8a (CRIT-1 close per `PRRO_GATE-vax` + spec rule #14)
/// — fail-loud drift rejection at the repository level.  When a caller
/// passes `Some(Y)` AND the row already has `Some(X)` with `X != Y`,
/// the WHERE-guard clause in `pin_signing_inputs_tx` MUST return
/// `rows_affected = 0` and leave the row unchanged.  Direct-repo
/// test — typed `SignError::SnapshotIdMismatch` mapping is exercised
/// by stage_sign integration tests in piece 8b/8c.
///
/// This is the "5th case" that extends the COALESCE truth-table from
/// 4 to 5 cases, per the acceptance gate mandate in
/// `project_m4_w4_z2a_locked_design.md` rule #14.  Without this
/// regression guard, a future refactor reverting the WHERE-guard
/// would silently re-enable the drift bug.
#[tokio::test]
async fn pin_existing_some_caller_some_other_where_guard_rejects_rows_zero() {
    let pool = fresh_pool().await;
    let doc_id = seed_doc_with_snapshot(&pool, Some(1)).await;
    // Seed a second snapshot row (id=2) so FK constraint allows
    // a hypothetical drift attempt.
    sqlx::query(
        "INSERT INTO signing_config_snapshots \
         (id, fn, driver_id, kind, payload_json, payload_sha256, created_at) \
         VALUES (2, ?, 'test-driver', 'check_tax_mapping_v1', '{}', \
                 X'3333333333333333333333333333333333333333333333333333333333333333', \
                 '2026-05-28T00:00:00Z')",
    )
    .bind(FN)
    .execute(&pool)
    .await
    .unwrap();

    let (pre_id, pre_pinned_at) = fetch_pin_state(&pool, doc_id).await;
    assert_eq!(pre_id, Some(1));
    assert!(pre_pinned_at.is_none(), "row should be unpinned pre-test");

    let rows_affected = with_immediate(&pool, move |tx| {
        Box::pin(async move {
            let res = fd::pin_signing_inputs_tx(tx, doc_id, None, None, Some(2)).await?;
            Ok::<u64, anyhow::Error>(res)
        })
    })
    .await
    .unwrap();

    assert_eq!(
        rows_affected, 0,
        "WHERE-guard MUST reject caller-Some(Y) vs existing-Some(X != Y) — rows_affected stays 0"
    );

    let (post_id, post_pinned_at) = fetch_pin_state(&pool, doc_id).await;
    assert_eq!(
        post_id,
        Some(1),
        "FK MUST NOT drift to caller's Y — row unchanged"
    );
    assert!(
        post_pinned_at.is_none(),
        "signing_inputs_pinned_at MUST stay NULL — rejected pin must not set timestamp"
    );
}
