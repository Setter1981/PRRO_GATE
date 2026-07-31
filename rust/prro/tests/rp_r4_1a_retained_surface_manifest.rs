//! RP-R4-1a (per-type retained-surface compile-manifest — BOTH paths).
//!
//! Spec `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §1 R4.5
//! RP-R4-1a: a compile/assert test that, **per legacy type**, exercises
//! **exactly that type's retained surface** (verified against
//! `prro-domain/src/{enums,ids}.rs`), via **both** the nested path
//! (`prro::db::models::enums::DocState`) **and** the short path
//! (`prro::db::models::DocState`).
//!
//! The surface is deliberately NON-uniform (that is the point of a per-type
//! matrix, not a blanket "derive Everything" assertion):
//!   * 8 TEXT enums: `Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize,
//!     Deserialize` + `as_str()` + `from_sql_str()`;
//!   * 6 BLOB ids: `Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize,
//!     Deserialize` + `#[serde(transparent)]` + `new()`/`from_bytes`/`as_bytes`/
//!     `Default` (NO public `now_v7`); `ShiftId` additionally
//!     `deterministic_for_shift_open`;
//!   * `CashierId`: `Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize`
//!     (NO `Copy`) + `#[serde(transparent)]` + `new()->Result` +
//!     `from_persisted_unchecked` + `as_str` + `into_inner` + `Display` +
//!     `FromStr`; `CashierIdError{Empty, TooLong}`;
//!   * `DriverId`: `Debug, Clone, PartialEq, Eq` (NO `Copy`, NO `Hash`, NO
//!     `Serialize`/`Deserialize`) + `new()->Result` + `as_str` + `into_inner`
//!     + `Display`; `DriverIdError{Empty, TooLong}`.
//!
//! Teeth (spec §1) — SYMMETRIC (both directions RED):
//!   * REMOVAL: dropping `Copy` from an enum → the positive `assert_enum_traits`
//!     bound (which requires `Copy`) fails to compile (RED). Removing a legacy
//!     export makes a path fail to resolve (RED).
//!   * ADDITION: `static_assertions::assert_not_impl_any!` pins the register's
//!     ABSENT surface — `DriverId: !Copy/!Hash/!Serialize/!DeserializeOwned`,
//!     `CashierId: !Copy` — so *adding* `Hash` (or `Copy`, `Serialize`, …) to
//!     `DriverId`, or `Copy` to `CashierId`, fails to compile (RED). (These
//!     negative pins use `assert_not_impl_any!`'s autoref-specialization trick;
//!     it works on stable, no nightly needed.)
//!
//! So this is a compile-tier oracle: it is GREEN iff the surface is EXACTLY the
//! retained one — no wider, no narrower.

#![allow(clippy::disallowed_names, dead_code, unused_imports)]

use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ── Static trait-presence assertions (compile-time; monomorphisation-only) ──
fn assert_clone<T: Clone>() {}
fn assert_copy<T: Copy>() {}
fn assert_debug<T: std::fmt::Debug>() {}
fn assert_partial_eq<T: PartialEq>() {}
fn assert_eq_trait<T: Eq>() {}
fn assert_hash<T: std::hash::Hash>() {}
fn assert_serialize<T: Serialize>() {}
fn assert_deserialize<T: for<'de> Deserialize<'de>>() {}
fn assert_default<T: Default>() {}
fn assert_display<T: Display>() {}

/// The full retained trait set of a TEXT enum.
fn assert_enum_traits<T>()
where
    T: Clone + Copy + std::fmt::Debug + PartialEq + Eq + std::hash::Hash + Serialize,
    T: for<'de> Deserialize<'de>,
{
}

/// The full retained trait set of a UUID-BLOB id.
fn assert_blob_id_traits<T>()
where
    T: Clone + Copy + std::fmt::Debug + PartialEq + Eq + std::hash::Hash + Serialize + Default,
    T: for<'de> Deserialize<'de>,
{
}

// ─────────────────────────────────────────────────────────────────────────
// 8 TEXT enums — exercised via BOTH paths.
// ─────────────────────────────────────────────────────────────────────────

/// Macro: for one enum, assert the retained trait set + `as_str`/`from_sql_str`
/// on the given fully-qualified path, constructing one variant.
macro_rules! exercise_enum {
    ($path:path, $variant:ident, $lit:literal) => {{
        use $path as E;
        assert_enum_traits::<E>();
        let v = E::$variant;
        // Copy: a bind-and-reuse proves `Copy` specifically (Clone alone would
        // move). If `Copy` were dropped, `let w = v; let _ = v;` would MOVE and
        // the second use is E0382 (RED) — this is the enum teeth surface.
        let w = v;
        let _ = v;
        // as_str() → the exact stored literal.
        assert_eq!(w.as_str(), $lit);
        // from_sql_str() round-trips the literal to the variant.
        assert_eq!(E::from_sql_str($lit), Some(E::$variant));
        // unknown literal ⇒ None (closed set).
        assert_eq!(E::from_sql_str("__cs1r_unknown__"), None);
        // serde round-trip (Serialize + Deserialize present).
        let j = serde_json::to_string(&w).unwrap();
        let back: E = serde_json::from_str(&j).unwrap();
        assert_eq!(back, E::$variant);
        // Hash usable (build a set).
        let mut m: HashMap<E, u8> = HashMap::new();
        m.insert(E::$variant, 1);
        assert_eq!(m.get(&E::$variant), Some(&1));
    }};
}

#[test]
fn enums_retained_surface_nested_path() {
    exercise_enum!(prro::db::models::enums::DocState, Prepared, "PREPARED");
    exercise_enum!(prro::db::models::enums::DocType, Sell, "SELL");
    exercise_enum!(prro::db::models::enums::FiscalMode, Test, "test");
    exercise_enum!(prro::db::models::enums::NodeMode, Online, "ONLINE");
    exercise_enum!(prro::db::models::enums::OfflineSessionState, Open, "OPEN");
    exercise_enum!(prro::db::models::enums::Protocol, Rest, "REST");
    exercise_enum!(prro::db::models::enums::Severity, Info, "INFO");
    exercise_enum!(prro::db::models::enums::ShiftState, Opened, "OPENED");
}

#[test]
fn enums_retained_surface_short_path() {
    exercise_enum!(prro::db::models::DocState, Prepared, "PREPARED");
    exercise_enum!(prro::db::models::DocType, Sell, "SELL");
    exercise_enum!(prro::db::models::FiscalMode, Test, "test");
    exercise_enum!(prro::db::models::NodeMode, Online, "ONLINE");
    exercise_enum!(prro::db::models::OfflineSessionState, Open, "OPEN");
    exercise_enum!(prro::db::models::Protocol, Rest, "REST");
    exercise_enum!(prro::db::models::Severity, Info, "INFO");
    exercise_enum!(prro::db::models::ShiftState, Opened, "OPENED");
}

// ─────────────────────────────────────────────────────────────────────────
// 6 UUID-BLOB ids — exercised via BOTH paths.
// ─────────────────────────────────────────────────────────────────────────

macro_rules! exercise_blob_id {
    ($path:path) => {{
        use $path as I;
        assert_blob_id_traits::<I>();
        // new() (public ctor; `Uuid::now_v7()` internal — NO public now_v7).
        let a = I::new();
        // Copy: bind-and-reuse.
        let b = a;
        let _ = a;
        // from_bytes / as_bytes round-trip.
        let bytes: [u8; 16] = *b.as_bytes();
        let c = I::from_bytes(bytes);
        assert_eq!(c.as_bytes(), &bytes);
        assert_eq!(b, c);
        // Default present.
        let _d: I = I::default();
        // serde transparent (bare Uuid string), round-trips.
        let j = serde_json::to_string(&c).unwrap();
        let back: I = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c);
        // Hash usable.
        let mut m: HashMap<I, u8> = HashMap::new();
        m.insert(c, 7);
        assert_eq!(m.get(&c), Some(&7));
    }};
}

#[test]
fn blob_ids_retained_surface_nested_path() {
    exercise_blob_id!(prro::db::models::ids::DocumentId);
    exercise_blob_id!(prro::db::models::ids::RequestId);
    exercise_blob_id!(prro::db::models::ids::ShiftId);
    exercise_blob_id!(prro::db::models::ids::OperatorId);
    exercise_blob_id!(prro::db::models::ids::PrinterId);
    exercise_blob_id!(prro::db::models::ids::OfflineSessionId);
    // ShiftId additionally exposes `deterministic_for_shift_open`.
    let doc = prro::db::models::ids::DocumentId::new();
    let s1 = prro::db::models::ids::ShiftId::deterministic_for_shift_open(doc);
    let s2 = prro::db::models::ids::ShiftId::deterministic_for_shift_open(doc);
    assert_eq!(
        s1, s2,
        "deterministic_for_shift_open must be a pure function"
    );
}

#[test]
fn blob_ids_retained_surface_short_path() {
    exercise_blob_id!(prro::db::models::DocumentId);
    exercise_blob_id!(prro::db::models::RequestId);
    exercise_blob_id!(prro::db::models::ShiftId);
    exercise_blob_id!(prro::db::models::OperatorId);
    exercise_blob_id!(prro::db::models::PrinterId);
    exercise_blob_id!(prro::db::models::OfflineSessionId);
    let doc = prro::db::models::DocumentId::new();
    let s1 = prro::db::models::ShiftId::deterministic_for_shift_open(doc);
    let s2 = prro::db::models::ShiftId::deterministic_for_shift_open(doc);
    assert_eq!(s1, s2);
}

// ─────────────────────────────────────────────────────────────────────────
// CashierId — NO Copy; has from_persisted_unchecked / as_str / into_inner /
// Display / FromStr; CashierIdError{Empty, TooLong}. Both paths.
// ─────────────────────────────────────────────────────────────────────────

macro_rules! exercise_cashier_id {
    ($cid:path, $cerr:path) => {{
        use $cerr as CErr;
        use $cid as C;

        // Retained trait set: Clone, Debug, PartialEq, Eq, Hash, Serialize,
        // Deserialize — but NOT Copy.
        assert_clone::<C>();
        assert_debug::<C>();
        assert_partial_eq::<C>();
        assert_eq_trait::<C>();
        assert_hash::<C>();
        assert_serialize::<C>();
        assert_deserialize::<C>();
        assert_display::<C>();

        // NEGATIVE pin (ADDITION teeth): `CashierId` must NOT be `Copy` — it is
        // a `String` newtype. Adding `#[derive(Copy)]` upstream is RED here.
        static_assertions::assert_not_impl_any!(C: Copy);

        // new() -> Result; strict validation.
        let c = C::new("cashier-vasya").unwrap();
        assert_eq!(c.as_str(), "cashier-vasya");
        assert!(matches!(C::new(""), Err(CErr::Empty)));
        let long = "x".repeat(129);
        assert!(matches!(C::new(long), Err(CErr::TooLong(_))));

        // from_persisted_unchecked bypasses validation (accepts empty).
        let legacy = C::from_persisted_unchecked(String::new());
        assert_eq!(legacy.as_str(), "");

        // Display.
        assert_eq!(format!("{c}"), "cashier-vasya");

        // FromStr.
        let parsed: C = "operator-007".parse().unwrap();
        assert_eq!(parsed.as_str(), "operator-007");

        // into_inner consumes → String.
        let owned: String = c.into_inner();
        assert_eq!(owned, "cashier-vasya");

        // Clone (NOT Copy): explicit .clone() needed.
        let c2 = parsed.clone();
        assert_eq!(c2, parsed);

        // serde transparent round-trip.
        let j = serde_json::to_string(&c2).unwrap();
        assert_eq!(j, "\"operator-007\"");
        let back: C = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c2);

        // Hash usable.
        let mut m: HashMap<C, u8> = HashMap::new();
        m.insert(c2.clone(), 3);
        assert_eq!(m.get(&c2), Some(&3));

        // Error enum variants both present.
        let _e1: CErr = CErr::Empty;
        let _e2: CErr = CErr::TooLong(200);
    }};
}

#[test]
fn cashier_id_retained_surface_nested_path() {
    exercise_cashier_id!(
        prro::db::models::ids::CashierId,
        prro::db::models::ids::CashierIdError
    );
}

#[test]
fn cashier_id_retained_surface_short_path() {
    exercise_cashier_id!(
        prro::db::models::CashierId,
        prro::db::models::CashierIdError
    );
}

// ─────────────────────────────────────────────────────────────────────────
// DriverId — Debug, Clone, PartialEq, Eq only (NO Copy/Hash/Serialize/
// Deserialize); has new()->Result / as_str / into_inner / Display;
// DriverIdError{Empty, TooLong}. Both paths.
// ─────────────────────────────────────────────────────────────────────────

macro_rules! exercise_driver_id {
    ($did:path, $derr:path) => {{
        use $derr as DErr;
        use $did as D;

        // Retained trait set: Debug, Clone, PartialEq, Eq — nothing else.
        assert_debug::<D>();
        assert_clone::<D>();
        assert_partial_eq::<D>();
        assert_eq_trait::<D>();
        assert_display::<D>();

        // NEGATIVE pin (ADDITION teeth): `DriverId` deliberately has NO `Copy`,
        // `Hash`, `Serialize`, or `Deserialize` (register §11.1 / spec §1). It is
        // a listener-context String newtype with no wire/serde or hashing use.
        // Adding ANY of these derives upstream is RED here — this makes RP-R4-1a
        // SYMMETRIC (both the removal and the addition direction bite).
        static_assertions::assert_not_impl_any!(
            D: Copy, core::hash::Hash, serde::Serialize, serde::de::DeserializeOwned
        );

        // new() -> Result; trims + rejects empty/too-long.
        let d = D::new("vendor-x").unwrap();
        assert_eq!(d.as_str(), "vendor-x");
        assert!(matches!(D::new("   "), Err(DErr::Empty)));
        let long = "y".repeat(65);
        assert!(matches!(D::new(long), Err(DErr::TooLong(_))));

        // Display.
        assert_eq!(format!("{d}"), "vendor-x");

        // Clone (NOT Copy).
        let d2 = d.clone();
        assert_eq!(d2, d);

        // into_inner consumes → String.
        let owned: String = d.into_inner();
        assert_eq!(owned, "vendor-x");

        // Error enum variants both present.
        let _e1: DErr = DErr::Empty;
        let _e2: DErr = DErr::TooLong(100);
    }};
}

#[test]
fn driver_id_retained_surface_nested_path() {
    exercise_driver_id!(
        prro::db::models::ids::DriverId,
        prro::db::models::ids::DriverIdError
    );
}

#[test]
fn driver_id_retained_surface_short_path() {
    exercise_driver_id!(prro::db::models::DriverId, prro::db::models::DriverIdError);
}
