//! RP-CS1-5 (representation-matrix conformance) — the **pure** half for the
//! ids (CS-1b′).
//!
//! Contract `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §2 (representation matrix) + §7 RP-CS1-5. This is the CS-1b′ closure for the
//! six UUID-BLOB newtype ids + `CashierId` (TEXT) + `DriverId` (TEXT) moved into
//! `prro-domain`.
//!
//! The relocation is a **storage non-event** — byte-identical. This test locks
//! the *pure* representation properties that survive the move (the sqlx
//! encode→decode round-trip through the store-side `Db*` wrappers lives in
//! `prro/tests/rp_cs1_5_db_id_roundtrip.rs`, which is where a live SQLite +
//! the wrappers reside):
//!
//!   * every BLOB id: `from_bytes`/`as_bytes` round-trip is byte-exact; `new()`
//!     is a UUIDv7 (version nibble = 7); `#[serde(transparent)]` output is the
//!     bare quoted UUID string (byte-identical to a raw `Uuid`);
//!   * `ShiftId::deterministic_for_shift_open` is a stable UUIDv5 over the
//!     opening `document_id` under the fixed PRRO namespace (same input ⇒ same
//!     output; a shift-id never equals its document-id);
//!   * `CashierId`: `new()` rejects Empty + `>MAX_LEN`; `from_persisted_unchecked`
//!     bypasses validation (hydrates legacy empty / oversize) SILENTLY (no warn
//!     in the pure crate — the oversize warn is store-side); `#[serde(transparent)]`
//!     output is the bare quoted string;
//!   * `DriverId`: `new()` trims + rejects empty / `>MAX_LEN`.

use prro_domain::{
    CashierId, CashierIdError, DocumentId, DriverId, DriverIdError, OfflineSessionId, OperatorId,
    PrinterId, RequestId, ShiftId,
};

// ── BLOB ids ─────────────────────────────────────────────────────────

/// `from_bytes` → `as_bytes` is byte-exact for every BLOB id, with NO
/// truncation / padding, for a value that exercises all 16 bytes.
macro_rules! blob_bytes_roundtrip {
    ($ty:ty) => {{
        let bytes: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let id = <$ty>::from_bytes(bytes);
        assert_eq!(
            id.as_bytes(),
            &bytes,
            "as_bytes round-trip drift for {}",
            stringify!($ty)
        );
    }};
}

#[test]
fn blob_ids_from_bytes_as_bytes_roundtrip_exact() {
    blob_bytes_roundtrip!(DocumentId);
    blob_bytes_roundtrip!(RequestId);
    blob_bytes_roundtrip!(ShiftId);
    blob_bytes_roundtrip!(OperatorId);
    blob_bytes_roundtrip!(PrinterId);
    blob_bytes_roundtrip!(OfflineSessionId);
}

#[test]
fn blob_ids_new_is_uuid_v7() {
    // `new()` must remain `Uuid::now_v7()` — version nibble is 7.
    assert_eq!(DocumentId::new().0.get_version_num(), 7);
    assert_eq!(RequestId::new().0.get_version_num(), 7);
    assert_eq!(ShiftId::new().0.get_version_num(), 7);
    assert_eq!(OperatorId::new().0.get_version_num(), 7);
    assert_eq!(PrinterId::new().0.get_version_num(), 7);
    assert_eq!(OfflineSessionId::new().0.get_version_num(), 7);
    // Default == new() (both fresh v7).
    assert_eq!(DocumentId::default().0.get_version_num(), 7);
}

#[test]
fn blob_id_serde_transparent_is_bare_uuid() {
    // `#[serde(transparent)]` — the id serialises EXACTLY as its inner Uuid
    // (byte-identical to a raw `uuid::Uuid`), not as a struct/tuple.
    let bytes: [u8; 16] = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd,
        0xef,
    ];
    let id = DocumentId::from_bytes(bytes);
    let raw = uuid::Uuid::from_bytes(bytes);
    let id_json = serde_json::to_string(&id).expect("serialize id");
    let raw_json = serde_json::to_string(&raw).expect("serialize raw uuid");
    assert_eq!(
        id_json, raw_json,
        "serde(transparent) must serialise as the bare Uuid"
    );
    // Round-trips back through serde to the same bytes.
    let back: DocumentId = serde_json::from_str(&id_json).expect("deserialize id");
    assert_eq!(back.as_bytes(), &bytes, "serde round-trip byte drift");
}

#[test]
fn shift_id_deterministic_for_shift_open_is_stable_v5() {
    let doc_bytes: [u8; 16] = [
        0xde, 0xad, 0xbe, 0xef, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
        0x0b,
    ];
    let doc = DocumentId::from_bytes(doc_bytes);
    let s1 = ShiftId::deterministic_for_shift_open(doc);
    let s2 = ShiftId::deterministic_for_shift_open(doc);
    // Deterministic: same document ⇒ same shift-id.
    assert_eq!(s1.as_bytes(), s2.as_bytes(), "v5 must be deterministic");
    // It is a UUIDv5.
    assert_eq!(s1.0.get_version_num(), 5, "must be UUIDv5");
    // A shift-id never equals its document-id (namespaced, not doc's own bytes).
    assert_ne!(
        s1.as_bytes(),
        doc.as_bytes(),
        "shift-id must not equal document-id"
    );
    // Pin the exact bytes so the fixed namespace can never drift (re-keying every
    // deterministic shift_id would defeat the collision backstop). Computed from
    // the frozen PRRO_SHIFT_OPEN_NS = bytes b"PRRO-SHFT-OPEN\x00\x01".
    let ns = uuid::Uuid::from_bytes([
        0x50, 0x52, 0x52, 0x4f, 0x2d, 0x53, 0x48, 0x46, 0x54, 0x2d, 0x4f, 0x50, 0x45, 0x4e, 0x00,
        0x01,
    ]);
    let expected = uuid::Uuid::new_v5(&ns, &doc_bytes);
    assert_eq!(
        s1.as_bytes(),
        expected.as_bytes(),
        "deterministic shift-id namespace drift"
    );
}

// ── CashierId (TEXT) ─────────────────────────────────────────────────

#[test]
fn cashier_id_new_rejects_empty_and_too_long() {
    assert!(matches!(CashierId::new(""), Err(CashierIdError::Empty)));
    let long = "x".repeat(CashierId::MAX_LEN + 1);
    assert!(matches!(
        CashierId::new(long),
        Err(CashierIdError::TooLong(_))
    ));
    // A typical handle is accepted and round-trips through as_str / Display.
    let c = CashierId::new("cashier-vasya").unwrap();
    assert_eq!(c.as_str(), "cashier-vasya");
    assert_eq!(format!("{c}"), "cashier-vasya");
}

#[test]
fn cashier_id_from_persisted_unchecked_bypasses_validation_silently() {
    // Legacy empty ⇒ accepted (SILENT — no warn in the pure crate).
    let empty = CashierId::from_persisted_unchecked(String::new());
    assert_eq!(empty.as_str(), "");
    // Oversize ⇒ accepted (the WARN lives store-side in DbCashierId, not here).
    let oversize = "y".repeat(CashierId::MAX_LEN + 5);
    let hydrated = CashierId::from_persisted_unchecked(oversize.clone());
    assert_eq!(hydrated.as_str(), oversize);
    assert_eq!(hydrated.clone().into_inner(), oversize);
}

#[test]
fn cashier_id_serde_transparent_is_bare_string() {
    let c = CashierId::new("operator-007").unwrap();
    let json = serde_json::to_string(&c).expect("serialize");
    assert_eq!(json, "\"operator-007\"", "serde(transparent) drift");
    let back: CashierId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.as_str(), "operator-007");
}

#[test]
fn cashier_id_from_str_roundtrip() {
    let c: CashierId = "operator-007".parse().unwrap();
    assert_eq!(c.as_str(), "operator-007");
    // FromStr enforces the strict constructor (empty rejected).
    assert!("".parse::<CashierId>().is_err());
}

// ── DriverId (TEXT, no sqlx) ─────────────────────────────────────────

#[test]
fn driver_id_new_trims_and_rejects() {
    // Trims surrounding whitespace.
    let d = DriverId::new("  vendor-x  ").unwrap();
    assert_eq!(d.as_str(), "vendor-x");
    assert_eq!(format!("{d}"), "vendor-x");
    assert_eq!(d.into_inner(), "vendor-x");
    // All-whitespace ⇒ Empty.
    assert!(matches!(DriverId::new("   "), Err(DriverIdError::Empty)));
    assert!(matches!(DriverId::new(""), Err(DriverIdError::Empty)));
    // Over MAX_LEN (after trim) ⇒ TooLong.
    let long = "z".repeat(DriverId::MAX_LEN + 1);
    assert!(matches!(
        DriverId::new(long),
        Err(DriverIdError::TooLong(_))
    ));
}
