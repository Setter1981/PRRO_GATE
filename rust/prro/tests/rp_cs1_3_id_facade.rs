//! RP-CS1-3 (facade completeness) — CS-1b′ id slice.
//!
//! Contract `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §5 / §7 RP-CS1-3: the legacy path `prro::db::models::ids::{…}` must keep
//! resolving **unchanged** after the six UUID-BLOB ids + `CashierId` / `DriverId`
//! (and their error types) moved to `prro-domain` behind the explicit
//! compatibility shim. This is a compile-only proof — if a moved id stopped
//! being re-exported, this target would fail to compile.
//!
//! It also pins that the shim re-exports the SAME `prro-domain` type (an
//! `AssertSame` type-equality check), not an accidental duplicate definition.

// Every moved id + error via the FULL legacy path (contract §5).
use prro::db::models::ids::{
    CashierId, CashierIdError, DocumentId, DriverId, DriverIdError, OfflineSessionId, OperatorId,
    PrinterId, RequestId, ShiftId,
};

/// Compile-time proof that two types are the SAME type. Instantiating
/// `AssertSame::<A, B>` only type-checks when `A == B`.
struct AssertSame<T>(std::marker::PhantomData<T>);
impl<T> AssertSame<T> {
    fn identical(_: std::marker::PhantomData<T>, _: std::marker::PhantomData<T>) {}
}

#[test]
fn moved_ids_resolve_via_legacy_path_and_are_the_domain_types() {
    // The legacy path resolves — construct/name a value of each.
    let _ = DocumentId::new();
    let _ = RequestId::new();
    let _ = ShiftId::new();
    let _ = OperatorId::new();
    let _ = PrinterId::new();
    let _ = OfflineSessionId::new();
    let _ = CashierId::new("cashier-vasya").unwrap();
    let _ = DriverId::new("vendor-x").unwrap();
    // Error types resolve via the legacy path too.
    let _ = CashierId::new("").unwrap_err();
    let _: fn(usize) -> CashierIdError = CashierIdError::TooLong;
    let _ = DriverId::new("").unwrap_err();
    let _: fn(usize) -> DriverIdError = DriverIdError::TooLong;

    // The shim re-exports the SAME `prro_domain` type (not a duplicate).
    AssertSame::<DocumentId>::identical(
        std::marker::PhantomData::<prro_domain::DocumentId>,
        std::marker::PhantomData::<DocumentId>,
    );
    AssertSame::<RequestId>::identical(
        std::marker::PhantomData::<prro_domain::RequestId>,
        std::marker::PhantomData::<RequestId>,
    );
    AssertSame::<ShiftId>::identical(
        std::marker::PhantomData::<prro_domain::ShiftId>,
        std::marker::PhantomData::<ShiftId>,
    );
    AssertSame::<OperatorId>::identical(
        std::marker::PhantomData::<prro_domain::OperatorId>,
        std::marker::PhantomData::<OperatorId>,
    );
    AssertSame::<PrinterId>::identical(
        std::marker::PhantomData::<prro_domain::PrinterId>,
        std::marker::PhantomData::<PrinterId>,
    );
    AssertSame::<OfflineSessionId>::identical(
        std::marker::PhantomData::<prro_domain::OfflineSessionId>,
        std::marker::PhantomData::<OfflineSessionId>,
    );
    AssertSame::<CashierId>::identical(
        std::marker::PhantomData::<prro_domain::CashierId>,
        std::marker::PhantomData::<CashierId>,
    );
    AssertSame::<DriverId>::identical(
        std::marker::PhantomData::<prro_domain::DriverId>,
        std::marker::PhantomData::<DriverId>,
    );
}
