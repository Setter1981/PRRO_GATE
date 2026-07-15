//! RP-R4-1c (no-glob / no-widening — the CS-1R R4 TRUE RED-first anchor).
//!
//! Spec `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §1 R4.5
//! RP-R4-1c: an **AST parse** of `src/db/models/mod.rs` asserting **exact
//! set-equality** of the module's explicit `pub use` re-export set to a **pinned
//! legacy list**, and that **NO glob** (`use … ::*`) survives in that `pub use`.
//!
//! Rationale (spec §1): positive path-compilation (RP-R4-1a) proves the legacy
//! paths still *resolve*, but it does **NOT** detect *widening* — a `pub use *`
//! glob (or an extra explicit export) silently re-exports more than the pinned
//! legacy surface and stays green under a compile-only test. This AST
//! set-equality pin is the only guard that bites on widening.
//!
//! Teeth (spec §1, three mutations, each → this test RED):
//!   (2) restore a `pub use enums::*;` / `pub use ids::*;` glob → glob assertion RED;
//!   (3) add an extra explicit export (e.g. `pub use enums::InboxStatus as Foo;`)
//!       or drop a legacy export → set-equality RED.
//! (Mutation (1) — remove a legacy export — is also caught here via set-equality,
//!  and by RP-R4-1a via path-compilation.)

use std::collections::BTreeSet;
use syn::{Item, UseTree};

/// The path to the facade module under test, relative to the crate manifest dir.
const MODELS_MOD_PATH: &str = "src/db/models/mod.rs";

/// The PINNED legacy export set for `prro::db::models` (the short path).
///
/// This is exactly what the two pre-CS-1R globs (`pub use enums::*;` +
/// `pub use ids::*;`) re-exported, enumerated per-symbol:
///
///   * from `enums`: the 8 TEXT enums re-exported from `prro_domain`
///     (`DocState`/`DocType`/`FiscalMode`/`NodeMode`/`OfflineSessionState`/
///     `Protocol`/`Severity`/`ShiftState`) PLUS the locally-defined
///     sqlx-bearing `InboxStatus` (which stays in `prro`, contract §2/§10);
///   * from `ids`: every `pub` item — the 6 UUID-BLOB ids + `CashierId` /
///     `DriverId` + their error types `CashierIdError` / `DriverIdError`.
///
/// If the facade legitimately gains or loses a legacy symbol, THIS list is the
/// single reviewed place that must change — that is the point of the pin.
const PINNED_LEGACY_EXPORTS: &[&str] = &[
    // enums (8 domain enums)
    "DocState",
    "DocType",
    "FiscalMode",
    "NodeMode",
    "OfflineSessionState",
    "Protocol",
    "Severity",
    "ShiftState",
    // enums (local, stays in prro)
    "InboxStatus",
    // ids (6 UUID-BLOB newtypes)
    "DocumentId",
    "RequestId",
    "ShiftId",
    "OperatorId",
    "PrinterId",
    "OfflineSessionId",
    // ids (TEXT-shaped)
    "CashierId",
    "DriverId",
    // ids (error types)
    "CashierIdError",
    "DriverIdError",
];

/// Collected facts about the module's `pub use` re-exports.
#[derive(Default)]
struct FacadeUses {
    /// Terminal names introduced into the module's public namespace by a
    /// `pub use` (the last path segment, honouring `as` renames).
    names: BTreeSet<String>,
    /// True if ANY `pub use` ends in a glob (`::*`).
    has_glob: bool,
}

/// Walk a `UseTree`, recording introduced names and glob presence.
fn walk_use_tree(tree: &UseTree, facade: &mut FacadeUses) {
    match tree {
        UseTree::Path(p) => walk_use_tree(&p.tree, facade),
        UseTree::Group(g) => {
            for t in &g.items {
                walk_use_tree(t, facade);
            }
        }
        UseTree::Name(n) => {
            facade.names.insert(n.ident.to_string());
        }
        UseTree::Rename(r) => {
            // The introduced name is the `as` alias, not the source ident.
            facade.names.insert(r.rename.to_string());
        }
        UseTree::Glob(_) => {
            facade.has_glob = true;
        }
    }
}

fn parse_facade() -> FacadeUses {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest_dir).join(MODELS_MOD_PATH);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let file = syn::parse_file(&src)
        .unwrap_or_else(|e| panic!("cannot syn-parse {}: {e}", path.display()));

    let mut facade = FacadeUses::default();
    for item in &file.items {
        if let Item::Use(u) = item {
            // Only `pub`-visibility re-exports form the legacy surface. A
            // private `use` (visibility `Inherited`) is an internal import and
            // does NOT widen the facade.
            if matches!(u.vis, syn::Visibility::Public(_)) {
                walk_use_tree(&u.tree, &mut facade);
            }
        }
    }
    facade
}

#[test]
fn models_facade_has_no_glob_reexport() {
    let facade = parse_facade();
    assert!(
        !facade.has_glob,
        "RP-R4-1c: `{MODELS_MOD_PATH}` must NOT contain any `pub use … ::*` glob \
         re-export — globs silently widen the legacy surface and evade the \
         set-equality pin. Replace globs with explicit per-symbol `pub use`."
    );
}

#[test]
fn models_facade_reexports_exactly_the_pinned_legacy_set() {
    let facade = parse_facade();

    let pinned: BTreeSet<String> = PINNED_LEGACY_EXPORTS
        .iter()
        .map(|s| s.to_string())
        .collect();

    let missing: Vec<&String> = pinned.difference(&facade.names).collect();
    let extra: Vec<&String> = facade.names.difference(&pinned).collect();

    assert!(
        missing.is_empty() && extra.is_empty(),
        "RP-R4-1c: `{MODELS_MOD_PATH}` public `pub use` set does not equal the \
         pinned legacy export list.\n  missing (in pin, not re-exported): {missing:?}\n  \
         extra   (re-exported, not in pin — WIDENING): {extra:?}\n  \
         actual re-exported set: {:?}",
        facade.names
    );
}
