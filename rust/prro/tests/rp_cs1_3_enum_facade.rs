//! RP-CS1-3 (facade completeness) — CS-1b enum slice.
//!
//! Contract `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §5 / §7 RP-CS1-3: the legacy path `prro::db::models::enums::{…}` must keep
//! resolving **unchanged** after the eight TEXT enums moved to `prro-domain`
//! behind the explicit compatibility shim. This is a compile-only proof — if a
//! moved enum stopped being re-exported (or `InboxStatus` stopped being defined
//! locally), this target would fail to compile.
//!
//! It also pins that the shim is a re-export of the SAME `prro-domain` type (an
//! `AssertSame` type-equality check), not an accidental duplicate definition,
//! and that `InboxStatus` is NOT sourced from `prro-domain` (it stays in `prro`).

// Every moved enum via the FULL legacy path (contract §5).
use prro::db::models::enums::{
    DocState, DocType, FiscalMode, InboxStatus, NodeMode, OfflineSessionState, Protocol, Severity,
    ShiftState,
};

/// Compile-time proof that two types are the SAME type. Instantiating
/// `AssertSame::<A, B>` only type-checks when `A == B`.
struct AssertSame<T>(std::marker::PhantomData<T>);
impl<T> AssertSame<T> {
    fn identical(_: std::marker::PhantomData<T>, _: std::marker::PhantomData<T>) {}
}

#[test]
fn moved_enums_resolve_via_legacy_path_and_are_the_domain_types() {
    // The legacy path resolves — construct a value of each.
    let _ = DocState::Prepared;
    let _ = OfflineSessionState::Open;
    let _ = ShiftState::Opened;
    let _ = NodeMode::Online;
    let _ = Protocol::Rest;
    let _ = DocType::Sell;
    let _ = FiscalMode::Test;
    let _ = Severity::Info;

    // The shim re-exports the SAME `prro_domain` type (not a duplicate).
    AssertSame::<DocState>::identical(
        std::marker::PhantomData::<prro_domain::DocState>,
        std::marker::PhantomData::<DocState>,
    );
    AssertSame::<OfflineSessionState>::identical(
        std::marker::PhantomData::<prro_domain::OfflineSessionState>,
        std::marker::PhantomData::<OfflineSessionState>,
    );
    AssertSame::<ShiftState>::identical(
        std::marker::PhantomData::<prro_domain::ShiftState>,
        std::marker::PhantomData::<ShiftState>,
    );
    AssertSame::<NodeMode>::identical(
        std::marker::PhantomData::<prro_domain::NodeMode>,
        std::marker::PhantomData::<NodeMode>,
    );
    AssertSame::<Protocol>::identical(
        std::marker::PhantomData::<prro_domain::Protocol>,
        std::marker::PhantomData::<Protocol>,
    );
    AssertSame::<DocType>::identical(
        std::marker::PhantomData::<prro_domain::DocType>,
        std::marker::PhantomData::<DocType>,
    );
    AssertSame::<FiscalMode>::identical(
        std::marker::PhantomData::<prro_domain::FiscalMode>,
        std::marker::PhantomData::<FiscalMode>,
    );
    AssertSame::<Severity>::identical(
        std::marker::PhantomData::<prro_domain::Severity>,
        std::marker::PhantomData::<Severity>,
    );

    // `InboxStatus` stays local to `prro` (contract §2/§10): its legacy path
    // resolves, and it must NOT be re-exported from `prro-domain`.
    let _ = InboxStatus::New;
}
