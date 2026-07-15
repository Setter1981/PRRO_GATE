//! RP-CS1-3 (facade completeness) — CS-1c command slice.
//!
//! Contract `docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §3 (manifest row for `CanonicalFiscalCommand`) / §5 / §7 RP-CS1-3: the
//! legacy path `prro::services::write_path::types::CanonicalFiscalCommand` must
//! keep resolving **unchanged** after the struct moved to `prro-domain` under
//! the SAME name, behind the explicit compatibility shim. This is a
//! compile-only proof — if the moved command stopped being re-exported (or was
//! renamed / aliased), this target would fail to compile.
//!
//! It also pins that the shim re-exports the SAME `prro-domain` type (an
//! `AssertSame` type-equality check), not an accidental duplicate definition,
//! and that a struct-literal still builds through the legacy path with every
//! field in place (the fields are the load-bearing surface all 11 src + 9 test
//! consumers construct against).

// The moved command via the FULL legacy path (contract §5 / §3).
use prro::db::models::enums::DocType;
use prro::db::models::ids::{CashierId, DriverId};
use prro::services::write_path::types::CanonicalFiscalCommand;

/// Compile-time proof that two types are the SAME type. Instantiating
/// `AssertSame::<T>::identical(..)` only type-checks when both `PhantomData`
/// arguments name the same `T`.
struct AssertSame<T>(std::marker::PhantomData<T>);
impl<T> AssertSame<T> {
    fn identical(_: std::marker::PhantomData<T>, _: std::marker::PhantomData<T>) {}
}

#[test]
fn moved_command_resolves_via_legacy_path_and_is_the_domain_type() {
    // The shim re-exports the SAME `prro_domain` type (not a duplicate).
    AssertSame::<CanonicalFiscalCommand>::identical(
        std::marker::PhantomData::<prro_domain::CanonicalFiscalCommand>,
        std::marker::PhantomData::<CanonicalFiscalCommand>,
    );

    // A struct-literal still builds through the legacy path — every field
    // present, in place, with the same types as the baseline definition.
    let cmd = CanonicalFiscalCommand {
        doc_type: DocType::Sell,
        business_ts: "2026-07-15T00:00:00Z".to_string(),
        total_sum_kop: Some(1_234),
        payload_json: "{}".to_string(),
        payload_sha256_canonical: [0u8; 32],
        source_sha256: [0u8; 32],
        signed_by_cashier_id: Some(CashierId::new("cashier-vasya").unwrap()),
        driver_id: Some(DriverId::new("vendor-x").unwrap()),
    };

    // `#[derive(Debug, Clone)]` still holds through the legacy path.
    let cloned = cmd.clone();
    assert_eq!(cloned.doc_type, DocType::Sell);
    assert_eq!(cloned.total_sum_kop, Some(1_234));
    assert_eq!(cloned.business_ts, "2026-07-15T00:00:00Z");
    let _ = format!("{cloned:?}");
}
