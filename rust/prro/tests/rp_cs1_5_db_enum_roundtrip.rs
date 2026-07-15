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
use prro::db::models::ids::CashierId;
use prro::db::types::{
    DbCashierId, DbDocState, DbDocType, DbFiscalMode, DbNodeMode, DbOfflineSessionState,
    DbProtocol, DbSeverity, DbShiftState,
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

// ─── CS-1R R3 (observable-equivalence pins) ───────────────────────────────
//
// spec `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §3.
//
// R3.1 (RP-R3-1) — the decode error string is FROZEN to the pre-move
//   `#[sqlx(type_name="TEXT")]` derive's inner `ColumnDecode.source` /
//   BoxDynError Display, captured EMPIRICALLY by a one-time throwaway probe at
//   commit f2c17b1 and committed to `tests/golden/cs1r_decode_errors.json`. The
//   golden — NOT the current production formatter — is the oracle (avoids
//   common-mode). We decode the canonical unknown literal `__CS1R_UNKNOWN__`
//   from a real `sqlite::memory:` TEXT column through each of the 8 `Db*`
//   wrappers and assert the inner-source Display == golden, per type.
//
// R3.2 (RP-R3-2) — the `DbCashierId::decode` oversize `tracing::warn!` carries
//   `target: "prro::db::models::ids"` plus its verbatim message + fields; a
//   `tracing` `Layer` subscriber captures target/message/fields structurally.
//
// R3.3 (RP-R3-3) — `Option<Enum> ↔ NULL` round-trips for ALL 8 wrappers
//   (None→NULL→None; Some(v) byte-identical + decodes back to Some(v)).

/// The canonical unknown TEXT literal frozen by R3.1 for every enum.
const CS1R_UNKNOWN: &str = "__CS1R_UNKNOWN__";

/// Load the committed golden decode-error map (wrapper name → inner
/// ColumnDecode.source Display string). Parsed from the JSON so the expected
/// value is NEVER produced by the same formatter/helper production uses.
fn golden_decode_errors() -> std::collections::BTreeMap<String, String> {
    // CARGO_MANIFEST_DIR points at rust/prro; the golden lives under tests/.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/cs1r_decode_errors.json"
    );
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read golden {path}: {e}"));
    let doc: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden {path}: {e}"));
    let errors = doc
        .get("errors")
        .and_then(|v| v.as_object())
        .expect("golden must have an `errors` object");
    errors
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_str()
                    .unwrap_or_else(|| panic!("golden error for {k} must be a string"))
                    .to_string(),
            )
        })
        .collect()
}

/// Decode `__CS1R_UNKNOWN__` as `$wrapper` from a real SQLite TEXT column and
/// return the **inner** `ColumnDecode.source` (BoxDynError) Display string —
/// NOT the full `sqlx::Error` (which carries the column index/name and would be
/// brittle). This is the exact error layer the golden froze.
macro_rules! decode_inner_source {
    ($pool:expr, $wrapper:ident) => {{
        let err = sqlx::query("SELECT ? AS v")
            .bind(CS1R_UNKNOWN)
            .try_map(|row: sqlx::sqlite::SqliteRow| row.try_get::<$wrapper, _>("v"))
            .fetch_one($pool)
            .await
            .expect_err("unknown literal must fail to decode (closed set)");
        match err {
            sqlx::Error::ColumnDecode { source, .. } => source.to_string(),
            other => panic!(
                "expected ColumnDecode for {}, got {other:?}",
                stringify!($wrapper)
            ),
        }
    }};
}

/// RP-R3-1 — the decode-error inner Display string of every one of the 8 TEXT
/// wrappers, decoded from a real SQLite TEXT column, equals the frozen golden
/// (captured at f2c17b1). Table-driven over all 8; the golden is the oracle.
#[tokio::test]
async fn rp_r3_1_decode_error_matches_golden_all_8() {
    let pool = mem_pool().await;
    let golden = golden_decode_errors();
    assert_eq!(
        golden.len(),
        8,
        "golden must freeze all 8 TEXT wrappers, got {}",
        golden.len()
    );

    // Table of (wrapper-name, captured-inner-source) via the real SQLite
    // decode path. One entry per wrapper.
    let captured: Vec<(&str, String)> = vec![
        ("DbDocState", decode_inner_source!(&pool, DbDocState)),
        ("DbDocType", decode_inner_source!(&pool, DbDocType)),
        ("DbFiscalMode", decode_inner_source!(&pool, DbFiscalMode)),
        ("DbNodeMode", decode_inner_source!(&pool, DbNodeMode)),
        (
            "DbOfflineSessionState",
            decode_inner_source!(&pool, DbOfflineSessionState),
        ),
        ("DbProtocol", decode_inner_source!(&pool, DbProtocol)),
        ("DbSeverity", decode_inner_source!(&pool, DbSeverity)),
        ("DbShiftState", decode_inner_source!(&pool, DbShiftState)),
    ];

    for (name, actual) in captured {
        let expected = golden
            .get(name)
            .unwrap_or_else(|| panic!("golden missing entry for {name}"));
        assert_eq!(
            &actual, expected,
            "RP-R3-1: decode-error inner source for {name} drifted from the \
             f2c17b1 golden.\n  golden:   {expected:?}\n  produced: {actual:?}"
        );
    }
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

// ─── RP-R3-2 — CashierId oversize warn: target + message + fields ─────────
//
// A structural `tracing` `Layer` captures each event's `target()` (from its
// static metadata — the exact thing `target: "…"` on the macro sets), plus its
// message + fields. Asserting the target this way is not brittle string-scrape:
// `metadata().target()` IS the restored `target: "prro::db::models::ids"`.

use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

/// One captured event, reduced to the observable surface R3.2 pins.
#[derive(Clone, Debug)]
struct CapturedEvent {
    target: String,
    level: String,
    message: String,
    fields: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct EventCapture(Arc<Mutex<Vec<CapturedEvent>>>);

struct FieldVisitor {
    message: String,
    fields: std::collections::BTreeMap<String, String>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for EventCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = FieldVisitor {
            message: String::new(),
            fields: std::collections::BTreeMap::new(),
        };
        event.record(&mut visitor);
        self.0.lock().unwrap().push(CapturedEvent {
            target: meta.target().to_string(),
            level: meta.level().to_string(),
            message: visitor.message,
            fields: visitor.fields,
        });
    }
}

/// RP-R3-2 — decoding an oversize (`> CashierId::MAX_LEN`) cashier_id emits a
/// single `WARN` with the RESTORED explicit `target: "prro::db::models::ids"`,
/// the verbatim message, and the `cashier_id_len` / `max_len` fields.
#[tokio::test(flavor = "current_thread")]
async fn rp_r3_2_cashier_oversize_warn_has_explicit_target_message_fields() {
    let capture = EventCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());

    let oversize_len = CashierId::MAX_LEN + 7;
    let oversize = "z".repeat(oversize_len);

    // `set_default` installs the capturing subscriber as the THIS-thread
    // default and returns a drop-guard. `flavor = "current_thread"` keeps the
    // decode future on this same thread, so the store-side `DbCashierId::decode`
    // warn is emitted into `capture`. Guard dropped at scope end.
    let _guard = tracing::subscriber::set_default(subscriber);

    let pool = mem_pool().await;
    // Decode an oversize TEXT literal through DbCashierId — legacy tolerant: no
    // decode error, but the oversize warn must fire.
    let decoded: DbCashierId = sqlx::query_scalar("SELECT ?")
        .bind(&oversize)
        .fetch_one(&pool)
        .await
        .expect("DbCashierId decode is legacy-tolerant, must not error");
    assert_eq!(
        decoded.0.as_str(),
        oversize.as_str(),
        "oversize value must decode byte-identical (no truncation)"
    );

    let events = capture.0.lock().unwrap().clone();
    let warns: Vec<&CapturedEvent> = events
        .iter()
        .filter(|e| {
            e.message
                .contains("CashierId decoded value exceeds MAX_LEN")
        })
        .collect();
    assert_eq!(
        warns.len(),
        1,
        "exactly one CashierId oversize warn expected, captured events: {events:?}"
    );
    let w = warns[0];

    // The RESTORED explicit target (R3.2).
    assert_eq!(
        w.target, "prro::db::models::ids",
        "RP-R3-2: CashierId oversize warn target drifted (must be the \
         explicitly-restored `prro::db::models::ids`)"
    );
    assert_eq!(w.level, "WARN", "oversize event must be WARN level");
    // Verbatim message.
    assert_eq!(
        w.message, "CashierId decoded value exceeds MAX_LEN — possible upstream schema drift",
        "RP-R3-2: warn message drifted"
    );
    // Verbatim fields: `cashier_id_len` = actual len, `max_len` = MAX_LEN.
    assert_eq!(
        w.fields.get("cashier_id_len").map(String::as_str),
        Some(oversize_len.to_string().as_str()),
        "RP-R3-2: cashier_id_len field drifted; fields={:?}",
        w.fields
    );
    assert_eq!(
        w.fields.get("max_len").map(String::as_str),
        Some(CashierId::MAX_LEN.to_string().as_str()),
        "RP-R3-2: max_len field drifted; fields={:?}",
        w.fields
    );
}

// ─── RP-R3-3 — Option<Enum> ↔ NULL round-trip for ALL 8 wrappers ──────────
//
// `None` binds as SQL NULL and decodes back to `None`; `Some(v)` binds as the
// byte-identical literal and decodes back to `Some(v)`. Proven per-wrapper
// against a real in-memory SQLite via the store-side `Db*` Encode/Decode path
// (`Option<T>` is bound/decoded through sqlx's blanket `Option` impls, which
// route the non-null case through `T`'s wrapper impl).

/// None → NULL → None, and Some(v) → literal → Some(v), for one wrapper.
macro_rules! option_null_case {
    ($pool:expr, $wrapper:ident, $v:expr, $lit:expr) => {{
        let pool = $pool;

        // None → NULL: raw column is SQL NULL, decodes back to None.
        let raw_null: Option<String> = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
            .bind(Option::<$wrapper>::None)
            .fetch_one(pool)
            .await
            .expect("encode None via wrapper");
        assert_eq!(
            raw_null,
            None,
            "None must bind as SQL NULL for {}",
            stringify!($wrapper)
        );
        let decoded_none: Option<$wrapper> = sqlx::query_scalar("SELECT ?")
            .bind(Option::<$wrapper>::None)
            .fetch_one(pool)
            .await
            .expect("decode None via wrapper");
        assert!(
            decoded_none.is_none(),
            "None must round-trip to None for {}",
            stringify!($wrapper)
        );

        // Some(v) → literal: raw TEXT is byte-identical, decodes back to Some(v).
        let raw_some: Option<String> = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
            .bind(Some($wrapper($v)))
            .fetch_one(pool)
            .await
            .expect("encode Some via wrapper");
        assert_eq!(
            raw_some.as_deref(),
            Some($lit),
            "Some({:?}) must bind byte-identical for {}",
            $v,
            stringify!($wrapper)
        );
        let decoded_some: Option<$wrapper> = sqlx::query_scalar("SELECT ?")
            .bind(Some($wrapper($v)))
            .fetch_one(pool)
            .await
            .expect("decode Some via wrapper");
        assert_eq!(
            decoded_some.map(|w| w.0),
            Some($v),
            "Some round-trip mismatch for {}",
            stringify!($wrapper)
        );
    }};
}

#[tokio::test]
async fn rp_r3_3_option_null_roundtrip_all_8_wrappers() {
    let pool = mem_pool().await;

    // One representative variant per wrapper is sufficient for the NULL pin
    // (the literal round-trip is exhaustively covered above); the pin here is
    // the None↔NULL mapping, exercised for ALL 8 wrappers.
    option_null_case!(&pool, DbDocState, DocState::Prepared, "PREPARED");
    option_null_case!(
        &pool,
        DbOfflineSessionState,
        OfflineSessionState::Draining,
        "DRAINING"
    );
    option_null_case!(&pool, DbShiftState, ShiftState::Opened, "OPENED");
    option_null_case!(&pool, DbNodeMode, NodeMode::Online, "ONLINE");
    option_null_case!(&pool, DbProtocol, Protocol::Rest, "REST");
    option_null_case!(&pool, DbDocType, DocType::Sell, "SELL");
    option_null_case!(&pool, DbFiscalMode, FiscalMode::Test, "test");
    option_null_case!(&pool, DbSeverity, Severity::Info, "INFO");
}
