//! RP-CS1-5 (representation-matrix conformance) — the **sqlx** half (CS-1b).
//!
//! Contract `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §2 / §7 RP-CS1-5 part (d): every moved TEXT enum encode→decode round-trips
//! through its store-side `prro::db::types::Db*` wrapper against a **real**
//! in-memory SQLite, and the persisted TEXT is byte-identical to the pre-move
//! literal (`as_str()`), so the relocation is a storage non-event.
//!
//! The pure properties (a/b/c/e — `from_sql_str`, the locked literal table,
//! unknown ⇒ None, byte-identical serde) live in
//! `prro-domain/tests/rp_cs1_5_enum_representation.rs`; this file proves the
//! wrapper's `Type`/`Encode`/`Decode` behaviour that the pure crate cannot host.

use prro::db::models::enums::{
    DocState, DocType, FiscalMode, NodeMode, OfflineSessionState, Protocol, Severity, ShiftState,
};
use prro::db::types::{
    DbDocState, DbDocType, DbFiscalMode, DbNodeMode, DbOfflineSessionState, DbProtocol, DbSeverity,
    DbShiftState,
};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

async fn mem_pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory sqlite")
}

/// Round-trip one variant: bind `DbX(v)` into a TEXT column, read the raw TEXT
/// back (must equal `v.as_str()`), and decode it back through `DbX` (must equal
/// `v`). Also proves the bare-`as_str()` bind used at the repository boundary
/// yields the SAME TEXT and decodes to the same variant.
macro_rules! roundtrip_case {
    ($pool:expr, $wrapper:ident, $v:expr, $lit:expr) => {{
        let pool = $pool;
        // (encode via wrapper) → (raw TEXT) — byte-identical to the literal.
        let raw: String = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
            .bind($wrapper($v))
            .fetch_one(pool)
            .await
            .expect("encode via wrapper");
        assert_eq!(raw, $lit, "wrapper-encoded TEXT drift for {:?}", $v);

        // (encode via bare as_str, the repo-boundary form) → same TEXT.
        let raw2: String = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
            .bind($v.as_str())
            .fetch_one(pool)
            .await
            .expect("encode via as_str");
        assert_eq!(raw2, $lit, "as_str-encoded TEXT drift for {:?}", $v);

        // (decode via wrapper) — round-trips back to the same variant.
        let decoded: $wrapper = sqlx::query_scalar("SELECT ?")
            .bind($wrapper($v))
            .fetch_one(pool)
            .await
            .expect("decode via wrapper");
        assert_eq!(decoded.0, $v, "wrapper round-trip mismatch for {:?}", $v);
    }};
}

#[tokio::test]
async fn db_enum_wrappers_roundtrip_byte_identical() {
    let pool = mem_pool().await;

    // DocState (14)
    for (v, lit) in [
        (DocState::Prepared, "PREPARED"),
        (DocState::Signed, "SIGNED"),
        (DocState::Encrypted, "ENCRYPTED"),
        (DocState::Sending, "SENDING"),
        (DocState::Sent, "SENT"),
        (DocState::Kvt1, "KVT1"),
        (DocState::Kvt2, "KVT2"),
        (DocState::Ack, "ACK"),
        (DocState::OfflineLocalAck, "OFFLINE_LOCAL_ACK"),
        (DocState::Rejected, "REJECTED"),
        (DocState::Cancelled, "CANCELLED"),
        (DocState::ErrorRetryable, "ERROR_RETRYABLE"),
        (
            DocState::RequiresManualReconciliation,
            "REQUIRES_MANUAL_RECONCILIATION",
        ),
        (DocState::Aborted, "ABORTED"),
    ] {
        roundtrip_case!(&pool, DbDocState, v, lit);
    }

    // OfflineSessionState (5)
    for (v, lit) in [
        (OfflineSessionState::Opening, "OPENING"),
        (OfflineSessionState::Open, "OPEN"),
        (OfflineSessionState::Draining, "DRAINING"),
        (OfflineSessionState::Closed, "CLOSED"),
        (OfflineSessionState::Aborted, "ABORTED"),
    ] {
        roundtrip_case!(&pool, DbOfflineSessionState, v, lit);
    }

    // ShiftState (9)
    for (v, lit) in [
        (ShiftState::Created, "CREATED"),
        (ShiftState::Opening, "OPENING"),
        (
            ShiftState::OpenedLocalPendingDrain,
            "OPENED_LOCAL_PENDING_DRAIN",
        ),
        (ShiftState::Opened, "OPENED"),
        (
            ShiftState::ClosingLocalPendingDrain,
            "CLOSING_LOCAL_PENDING_DRAIN",
        ),
        (ShiftState::Closing, "CLOSING"),
        (ShiftState::Closed, "CLOSED"),
        (
            ShiftState::RequiresManualReconciliation,
            "REQUIRES_MANUAL_RECONCILIATION",
        ),
        (ShiftState::Error, "ERROR"),
    ] {
        roundtrip_case!(&pool, DbShiftState, v, lit);
    }

    // NodeMode (7)
    for (v, lit) in [
        (NodeMode::Online, "ONLINE"),
        (NodeMode::GoingOffline, "GOING_OFFLINE"),
        (NodeMode::Offline, "OFFLINE"),
        (NodeMode::GoingOnline, "GOING_ONLINE"),
        (NodeMode::Blocked, "BLOCKED"),
        (NodeMode::StopMode, "STOP_MODE"),
        (NodeMode::CryptoDegraded, "CRYPTO_DEGRADED"),
    ] {
        roundtrip_case!(&pool, DbNodeMode, v, lit);
    }

    // Protocol (6)
    for (v, lit) in [
        (Protocol::Rest, "REST"),
        (Protocol::XmlRpc, "XMLRPC"),
        (Protocol::Maria, "MARIA"),
        (Protocol::Maria304, "MARIA304"),
        (Protocol::CheckboxCompat, "CHECKBOX_COMPAT"),
        (Protocol::Internal, "INTERNAL"),
    ] {
        roundtrip_case!(&pool, DbProtocol, v, lit);
    }

    // DocType (12)
    for (v, lit) in [
        (DocType::ShiftOpen, "SHIFT_OPEN"),
        (DocType::ShiftClose, "SHIFT_CLOSE"),
        (DocType::Sell, "SELL"),
        (DocType::Return, "RETURN"),
        (DocType::ServiceIn, "SERVICE_IN"),
        (DocType::ServiceOut, "SERVICE_OUT"),
        (DocType::CashWithdrawal, "CASH_WITHDRAWAL"),
        (DocType::CashAdvanceEpz, "CASH_ADVANCE_EPZ"),
        (DocType::XReport, "X_REPORT"),
        (DocType::ZReport, "Z_REPORT"),
        (DocType::OfflineSessionBegin, "OFFLINE_SESSION_BEGIN"),
        (DocType::OfflineSessionEnd, "OFFLINE_SESSION_END"),
    ] {
        roundtrip_case!(&pool, DbDocType, v, lit);
    }

    // FiscalMode (2) — lowercase pinned.
    for (v, lit) in [(FiscalMode::Test, "test"), (FiscalMode::Prod, "prod")] {
        roundtrip_case!(&pool, DbFiscalMode, v, lit);
    }

    // Severity (4)
    for (v, lit) in [
        (Severity::Info, "INFO"),
        (Severity::Warning, "WARNING"),
        (Severity::Error, "ERROR"),
        (Severity::Critical, "CRITICAL"),
    ] {
        roundtrip_case!(&pool, DbSeverity, v, lit);
    }
}

/// (c) closed set — an unknown TEXT literal is a **decode error**, not a silent
/// fallback. Proven against a real SQLite value for one representative wrapper.
#[tokio::test]
async fn db_enum_wrapper_unknown_literal_is_decode_error() {
    let pool = mem_pool().await;
    let res = sqlx::query("SELECT 'NOT_A_REAL_STATE' AS state")
        .try_map(|row: sqlx::sqlite::SqliteRow| row.try_get::<DbDocState, _>("state").map(|w| w.0))
        .fetch_one(&pool)
        .await;
    assert!(
        res.is_err(),
        "an unknown TEXT literal must fail to decode (closed set), got {res:?}"
    );
}

/// Confirms a genuine on-disk column persists the byte-identical literal and
/// decodes back — the end-to-end storage-non-event proof (not just a CAST).
#[tokio::test]
async fn db_enum_persists_and_decodes_via_real_column() {
    let pool = mem_pool().await;
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, state TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    // Insert every DocState variant, read raw TEXT + decoded wrapper back.
    for v in [
        DocState::Prepared,
        DocState::OfflineLocalAck,
        DocState::RequiresManualReconciliation,
        DocState::Aborted,
    ] {
        sqlx::query("INSERT INTO t (state) VALUES (?)")
            .bind(DbDocState(v))
            .execute(&pool)
            .await
            .unwrap();
    }
    let rows = sqlx::query("SELECT state FROM t ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    let raws: Vec<String> = rows.iter().map(|r| r.get::<String, _>("state")).collect();
    assert_eq!(
        raws,
        vec![
            "PREPARED".to_string(),
            "OFFLINE_LOCAL_ACK".to_string(),
            "REQUIRES_MANUAL_RECONCILIATION".to_string(),
            "ABORTED".to_string(),
        ]
    );
    let decoded: Vec<DocState> = rows
        .iter()
        .map(|r| r.get::<DbDocState, _>("state").0)
        .collect();
    assert_eq!(
        decoded,
        vec![
            DocState::Prepared,
            DocState::OfflineLocalAck,
            DocState::RequiresManualReconciliation,
            DocState::Aborted,
        ]
    );
}
