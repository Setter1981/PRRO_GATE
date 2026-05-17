//! Smoke tests for UUIDv7 newtypes and sqlx::Type enum wrappers.
//!
//! Unit-level: byte-length, monotonicity, `as_str` round-trip.
//! Integration-level: BLOB and TEXT round-trips through real schema
//! tables (shifts and fiscal_number_config) so the Encode/Decode
//! glue is exercised end-to-end against bundled SQLite.

use prro::db::models::*;

#[test]
fn document_id_roundtrip_bytes() {
    let a = DocumentId::new();
    assert_eq!(a.as_bytes().len(), 16);
    let b = DocumentId::from_bytes(*a.as_bytes());
    assert_eq!(a, b);
}

#[test]
fn document_id_monotonic() {
    let a = DocumentId::new();
    let b = DocumentId::new();
    // UUIDv7 is monotonic by construction (time-ordered) — equal-or-greater.
    assert!(
        b.as_bytes() >= a.as_bytes(),
        "two new UUIDv7 should be monotonic: {:?} {:?}",
        a,
        b
    );
}

#[test]
fn enum_as_str_covers_known_variants() {
    assert_eq!(DocState::Prepared.as_str(), "PREPARED");
    assert_eq!(
        DocState::RequiresManualReconciliation.as_str(),
        "REQUIRES_MANUAL_RECONCILIATION"
    );
    assert_eq!(ShiftState::Opened.as_str(), "OPENED");
    assert_eq!(NodeMode::CryptoDegraded.as_str(), "CRYPTO_DEGRADED");
    assert_eq!(FiscalMode::Test.as_str(), "test");
    assert_eq!(FiscalMode::Prod.as_str(), "prod");
    assert_eq!(Protocol::Maria304.as_str(), "MARIA304");
    assert_eq!(Severity::Critical.as_str(), "CRITICAL");
    assert_eq!(InboxStatus::New.as_str(), "NEW");
    assert_eq!(DocType::ZReport.as_str(), "Z_REPORT");
}

async fn fresh_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = prro::db::open_pool(&dir.path().join("models.db"))
        .await
        .unwrap();
    (dir, pool)
}

#[tokio::test]
async fn fiscal_mode_roundtrips_through_fn_config_text_column() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, ?, ?)",
    )
    .bind("4444444444")
    .bind("12345678")
    .bind(FiscalMode::Test)
    .execute(&pool)
    .await
    .unwrap();

    let mode: FiscalMode = sqlx::query_scalar(
        "SELECT fiscal_mode FROM fiscal_number_config WHERE fiscal_number = '4444444444'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mode, FiscalMode::Test);
}

#[tokio::test]
async fn shift_id_roundtrips_through_shifts_blob_column() {
    let (_d, pool) = fresh_pool().await;
    sqlx::query(
        "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
         VALUES ('5555555555', '12345678', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, state, open_mode, opened_at, \
            cash_balance_kop, opened_by_cashier_id) VALUES (?, '5555555555', 'CREATED', 'ONLINE', \
            '2026-04-22T00:00:00Z', 0, 'test-cashier')",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    let got: ShiftId =
        sqlx::query_scalar("SELECT shift_id FROM shifts WHERE fiscal_number = '5555555555'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        id, got,
        "round-trip BLOB through real shifts column must be lossless"
    );
    assert_eq!(got.as_bytes().len(), 16);
}
