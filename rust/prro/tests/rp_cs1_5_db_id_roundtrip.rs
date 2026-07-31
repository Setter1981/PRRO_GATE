//! RP-CS1-5 (representation-matrix conformance) — the **sqlx** half for the ids
//! (CS-1b′).
//!
//! Contract `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §2 / §7 RP-CS1-5: every moved id encode→decode round-trips through its
//! store-side `prro::db::types::Db*` wrapper against a **real** in-memory SQLite,
//! byte-identical to the pre-move impls, so the relocation is a storage
//! non-event. Specifically:
//!
//!   * BLOB ids: `Type` affinity is **BLOB**; a 16-byte value round-trips
//!     exactly (hex/bytes identity); a stored blob whose **length ≠ 16 ⇒ decode
//!     error `"invalid UUID byte length"`** (no truncation / pad);
//!   * `CashierId`: `Type` affinity is **TEXT**; decode **hydrates a legacy
//!     empty string SILENTLY** and a `> MAX_LEN` string **WITH a tracing warn**
//!     (both ACCEPTED, not rejected — the strict `new()` still rejects them).
//!
//! The pure properties (from_bytes/as_bytes identity, serde transparency, v5
//! namespace, constructor validation) live in
//! `prro-domain/tests/rp_cs1_5_id_representation.rs`; this file proves the
//! wrapper `Type`/`Encode`/`Decode` behaviour the pure crate cannot host.
//! `DriverId` has **no** `Db*` wrapper (contract §2/§3) — its raw-`String` DB
//! boundary is exercised at the bottom.

use prro::db::models::ids::{
    CashierId, DocumentId, DriverId, OfflineSessionId, OperatorId, PrinterId, RequestId, ShiftId,
};
use prro::db::types::{
    DbCashierId, DbDocumentId, DbOfflineSessionId, DbOperatorId, DbPrinterId, DbRequestId,
    DbShiftId,
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

/// Round-trip one BLOB id through its wrapper: `Type` affinity is BLOB, the
/// stored raw blob equals the 16 `as_bytes()`, and decode round-trips exactly.
macro_rules! blob_roundtrip_case {
    ($pool:expr, $wrapper:ident, $id:expr) => {{
        let pool = $pool;
        let id = $id;
        let raw_bytes = id.as_bytes().to_vec();

        // (encode via wrapper) → the raw stored blob is byte-identical.
        let stored: Vec<u8> = sqlx::query_scalar("SELECT CAST(? AS BLOB)")
            .bind($wrapper(id))
            .fetch_one(pool)
            .await
            .expect("encode via wrapper");
        assert_eq!(stored, raw_bytes, "wrapper-encoded BLOB drift");
        assert_eq!(stored.len(), 16, "must persist exactly 16 bytes");

        // Type affinity is BLOB (typeof of the bound value is 'blob').
        let ty: String = sqlx::query_scalar("SELECT typeof(?)")
            .bind($wrapper(id))
            .fetch_one(pool)
            .await
            .expect("typeof");
        assert_eq!(ty, "blob", "BLOB affinity drift");

        // (decode via wrapper) — round-trips back to the same id bytes.
        let decoded: $wrapper = sqlx::query_scalar("SELECT ?")
            .bind($wrapper(id))
            .fetch_one(pool)
            .await
            .expect("decode via wrapper");
        assert_eq!(
            decoded.0.as_bytes(),
            id.as_bytes(),
            "wrapper round-trip drift"
        );

        // (encode via bare as_bytes slice, the alt repo-boundary form) → same blob.
        let stored2: Vec<u8> = sqlx::query_scalar("SELECT CAST(? AS BLOB)")
            .bind(&id.as_bytes()[..])
            .fetch_one(pool)
            .await
            .expect("encode via as_bytes");
        assert_eq!(stored2, raw_bytes, "as_bytes-encoded BLOB drift");
    }};
}

#[tokio::test]
async fn db_blob_id_wrappers_roundtrip_byte_identical() {
    let pool = mem_pool().await;
    let bytes: [u8; 16] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];
    blob_roundtrip_case!(&pool, DbDocumentId, DocumentId::from_bytes(bytes));
    blob_roundtrip_case!(&pool, DbRequestId, RequestId::from_bytes(bytes));
    blob_roundtrip_case!(&pool, DbShiftId, ShiftId::from_bytes(bytes));
    blob_roundtrip_case!(&pool, DbOperatorId, OperatorId::from_bytes(bytes));
    blob_roundtrip_case!(&pool, DbPrinterId, PrinterId::from_bytes(bytes));
    blob_roundtrip_case!(
        &pool,
        DbOfflineSessionId,
        OfflineSessionId::from_bytes(bytes)
    );
}

/// A stored blob whose length is NOT 16 is a **decode error** (`"invalid UUID
/// byte length"`), never a truncation / pad. Proven against real SQLite values
/// for one representative wrapper, for several wrong lengths incl. empty.
#[tokio::test]
async fn db_blob_id_wrong_length_is_decode_error() {
    let pool = mem_pool().await;
    for bad_len in [0usize, 1, 15, 17, 32] {
        let blob = vec![0xABu8; bad_len];
        let res = sqlx::query("SELECT ? AS id")
            .bind(blob)
            .try_map(|row: sqlx::sqlite::SqliteRow| {
                row.try_get::<DbDocumentId, _>("id").map(|w| w.0)
            })
            .fetch_one(&pool)
            .await;
        assert!(
            res.is_err(),
            "a {bad_len}-byte blob must fail to decode (no truncation/pad), got {res:?}"
        );
        let msg = format!("{:?}", res.unwrap_err());
        assert!(
            msg.contains("invalid UUID byte length"),
            "decode error message drift for len {bad_len}: {msg}"
        );
    }
}

/// End-to-end on-disk column: every BLOB id persists 16 raw bytes and decodes
/// back — the storage-non-event proof (not just a CAST).
#[tokio::test]
async fn db_blob_id_persists_and_decodes_via_real_column() {
    let pool = mem_pool().await;
    sqlx::query("CREATE TABLE t (k INTEGER PRIMARY KEY, id BLOB NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let a: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let b: [u8; 16] = [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
    for bytes in [a, b] {
        sqlx::query("INSERT INTO t (id) VALUES (?)")
            .bind(DbShiftId(ShiftId::from_bytes(bytes)))
            .execute(&pool)
            .await
            .unwrap();
    }
    let rows = sqlx::query("SELECT id FROM t ORDER BY k")
        .fetch_all(&pool)
        .await
        .unwrap();
    let raws: Vec<Vec<u8>> = rows.iter().map(|r| r.get::<Vec<u8>, _>("id")).collect();
    assert_eq!(raws, vec![a.to_vec(), b.to_vec()], "on-disk blob drift");
    let decoded: Vec<[u8; 16]> = rows
        .iter()
        .map(|r| *r.get::<DbShiftId, _>("id").0.as_bytes())
        .collect();
    assert_eq!(decoded, vec![a, b], "decoded id drift");
}

/// `CashierId` wrapper: `Type` affinity is TEXT, and the stored TEXT is the bare
/// string (byte-identical to `as_str()`), round-tripping through the wrapper.
#[tokio::test]
async fn db_cashier_id_wrapper_roundtrip_text() {
    let pool = mem_pool().await;
    let cid = CashierId::new("cashier-vasya").unwrap();

    let raw: String = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
        .bind(DbCashierId(cid.clone()))
        .fetch_one(&pool)
        .await
        .expect("encode via wrapper");
    assert_eq!(raw, "cashier-vasya", "wrapper-encoded TEXT drift");

    let ty: String = sqlx::query_scalar("SELECT typeof(?)")
        .bind(DbCashierId(cid.clone()))
        .fetch_one(&pool)
        .await
        .expect("typeof");
    assert_eq!(ty, "text", "TEXT affinity drift");

    let decoded: DbCashierId = sqlx::query_scalar("SELECT ?")
        .bind(DbCashierId(cid.clone()))
        .fetch_one(&pool)
        .await
        .expect("decode via wrapper");
    assert_eq!(decoded.0.as_str(), "cashier-vasya", "round-trip drift");

    // Bare `.as_str()` bind (the repo-boundary form) yields the same TEXT.
    let raw2: String = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
        .bind(cid.as_str())
        .fetch_one(&pool)
        .await
        .expect("encode via as_str");
    assert_eq!(raw2, "cashier-vasya", "as_str-encoded TEXT drift");
}

/// The CENTRAL CS-1b′ legacy-tolerance pin: `CashierId` decode is
/// legacy-tolerant even though the strict `new()` is not.
///
///   * an EMPTY stored string ⇒ **accepted SILENTLY** (hydrated, not rejected);
///   * a `> MAX_LEN` stored string ⇒ **accepted WITH a tracing warn** (hydrated,
///     not rejected — the warn is store-side observability only).
///
/// Both are the exact opposite of `CashierId::new()`, which rejects Empty /
/// TooLong. This is what lets historical rows (the `__pre_w14a1__` sentinel /
/// upstream drift) decode instead of failing the read.
#[tokio::test]
async fn db_cashier_id_decode_is_legacy_tolerant() {
    let pool = mem_pool().await;

    // Strict constructor STILL rejects both (unchanged).
    assert!(CashierId::new("").is_err(), "new() must reject empty");
    assert!(
        CashierId::new("x".repeat(CashierId::MAX_LEN + 1)).is_err(),
        "new() must reject too-long"
    );

    // Empty ⇒ decode ACCEPTS (silently).
    let empty: DbCashierId = sqlx::query_scalar("SELECT CAST('' AS TEXT)")
        .fetch_one(&pool)
        .await
        .expect("empty cashier_id must decode, not error");
    assert_eq!(empty.0.as_str(), "", "empty must hydrate to empty string");

    // Oversize ⇒ decode ACCEPTS (with a warn — value preserved verbatim).
    let over = "y".repeat(CashierId::MAX_LEN + 7);
    let decoded: DbCashierId = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
        .bind(&over)
        .fetch_one(&pool)
        .await
        .expect("oversize cashier_id must decode (with warn), not error");
    assert_eq!(
        decoded.0.as_str(),
        over,
        "oversize must hydrate to the exact stored value"
    );
    assert!(
        decoded.0.as_str().len() > CashierId::MAX_LEN,
        "oversize value must survive decode unchanged"
    );
}

/// `DriverId` has NO `Db*` wrapper (contract §2/§3): it is bound as a raw
/// `String` and decoded as a `String` then re-validated via `DriverId::new()`.
/// This exercises that raw-String DB boundary round-trip.
#[tokio::test]
async fn driver_id_raw_string_boundary_roundtrips() {
    let pool = mem_pool().await;
    let d = DriverId::new("vendor-x").unwrap();

    // Bind as raw &str (the repo-boundary form — no wrapper exists).
    let raw: String = sqlx::query_scalar("SELECT CAST(? AS TEXT)")
        .bind(d.as_str())
        .fetch_one(&pool)
        .await
        .expect("encode via as_str");
    assert_eq!(raw, "vendor-x");

    // Decode as String, then re-validate via the strict constructor.
    let back = DriverId::new(raw).expect("re-validate");
    assert_eq!(
        back.as_str(),
        "vendor-x",
        "raw-String boundary round-trip drift"
    );
}
