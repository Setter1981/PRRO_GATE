//! RP-CS1-4 (contract-crate DAG) — the CS-1d half, hardened by CS-1R R2.
//!
//! The three boundary crates `prro-ingress-contract` / `prro-dps-contract` /
//! `prro-fleet-contract` are **empty dependency skeletons**. This test pins,
//! over the RESOLVED `--all-features` `cargo metadata` graph (via the SHARED
//! production walker in `support/metadata_graph.rs` — the SAME code the
//! `prro-domain` purity gate uses), the invariants that keep those ports from
//! silently coupling before their ports even land:
//!
//!   1. all three `*-contract` crates ARE workspace members;
//!   2. NONE reaches a forbidden adapter/IO crate in its non-dev closure;
//!   3. NO contract crate depends (non-dev) on another contract crate
//!      (ingress ⊥ dps ⊥ fleet — a hard DAG invariant);
//!   4. **R2.1 (PHASED)** — each contract crate's DIRECT non-dev dependency set
//!      is **∅**. This is PHASED: it changes ONLY via the corresponding port
//!      spec (specs #3–5 / CS-6). A drift here (a direct dep added ad hoc) is a
//!      contract violation, not a routine bump.
//!
//! Walker semantics: `cargo metadata --format-version 1 --all-features --locked`,
//! following ONLY non-dev edges, PackageId-dedup, root-by-workspace-id — all in
//! the shared module (R2.4). Same arg vector, same pin as the purity gate.
//!
//! RED→GREEN provenance (CS-1d): authored while the three crates were ABSENT
//! from `rust/Cargo.toml` → assertion (1) FAILED → the empty crates were added
//! → GREEN. Canary for (3): add `prro-dps-contract` to `prro-ingress-contract`'s
//! `[dependencies]` → RED on the "contract ⊥ contract" edge → revert. Canary for
//! (4) [RP-R2-f]: add ANY direct dep to a `*-contract` crate (+ lock refresh) →
//! the ∅-allowlist assertion fails → revert.

#[path = "support/metadata_graph.rs"]
mod metadata_graph;

use metadata_graph as mg;

/// The three boundary crates created in CS-1d (contract §8 item 5).
const CONTRACT_CRATES: &[&str] = &[
    "prro-ingress-contract",
    "prro-dps-contract",
    "prro-fleet-contract",
];

/// Crates that must NEVER appear in a contract crate's non-dev closure.
const FORBIDDEN: &[&str] = &[
    "sqlx", "tonic", "tokio", "axum", "prost", "hyper", "reqwest",
];

/// The set of workspace member crate names (via the shared graph).
fn workspace_member_names(graph: &mg::Graph, meta: &serde_json::Value) -> Vec<String> {
    meta["workspace_members"]
        .as_array()
        .expect("workspace_members")
        .iter()
        .map(|id| graph.name_of(id.as_str().expect("member id")).to_string())
        .collect()
}

/// RP-CS1-4 (1): the three `*-contract` crates are workspace members.
#[test]
fn contract_crates_are_workspace_members() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let members = workspace_member_names(&graph, &meta);

    let missing: Vec<&str> = CONTRACT_CRATES
        .iter()
        .copied()
        .filter(|c| !members.iter().any(|m| m == c))
        .collect();

    assert!(
        missing.is_empty(),
        "the CS-1d boundary crate(s) {missing:?} must be workspace members \
         (contract §8 item 5), but they are absent from workspace_members = {members:?}",
    );
}

/// RP-CS1-4 (2): no contract crate reaches a forbidden adapter/IO crate in its
/// non-dev closure. Uses the shared PackageId walker + root-by-workspace-id.
#[test]
fn contract_crates_are_io_free() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);

    let mut violations: Vec<String> = Vec::new();
    for crate_name in CONTRACT_CRATES {
        let root = graph.workspace_root_id(crate_name);
        let names = graph.non_dev_closure_names(&root);
        for f in FORBIDDEN {
            if names.contains(*f) {
                violations.push(format!("{crate_name} -> {f}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "the CS-1d boundary crates must have no DB/network/async-runtime/RPC/HTTP \
         adapter dependencies (contract §7 RP-CS1-4), but a forbidden crate is \
         reachable: {violations:?}",
    );
}

/// RP-CS1-4 (3): no contract crate depends (non-dev) on another contract crate
/// — ingress ⊥ dps ⊥ fleet.
#[test]
fn contract_crates_do_not_depend_on_each_other() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);

    let mut couplings: Vec<String> = Vec::new();
    for crate_name in CONTRACT_CRATES {
        let root = graph.workspace_root_id(crate_name);
        let names = graph.non_dev_closure_names(&root);
        for other in CONTRACT_CRATES {
            if other == crate_name {
                continue;
            }
            if names.contains(*other) {
                couplings.push(format!("{crate_name} -> {other}"));
            }
        }
    }

    assert!(
        couplings.is_empty(),
        "contract crates must stay orthogonal (ingress ⊥ dps ⊥ fleet, contract §7 \
         RP-CS1-4): no contract crate may depend on another, but found: {couplings:?}",
    );
}

/// **R2.1 PHASED (RP-R2-f).** Each `*-contract` crate's DIRECT non-dev dependency
/// set is exactly **∅**. PHASED: this set changes ONLY via the corresponding
/// port spec (specs #3–5 / CS-6). A direct dep added outside that process is a
/// contract violation.
///
/// RP-R2-f teeth: add any dep to `[dependencies]` of a `*-contract` crate
/// (+ refresh the lock) → this assertion fails naming the crate+dep; revert.
#[test]
fn contract_crates_have_empty_direct_deps_phased() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);

    let mut offenders: Vec<String> = Vec::new();
    for crate_name in CONTRACT_CRATES {
        let direct = graph.direct_deps(&meta, crate_name);
        for d in &direct.normal {
            offenders.push(format!("{crate_name} (normal) -> {d}"));
        }
        for d in &direct.build {
            offenders.push(format!("{crate_name} (build) -> {d}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "R2.1 PHASED ∅-allowlist: the `*-contract` crates must declare NO direct \
         non-dev/build dependencies until their port spec (specs #3–5 / CS-6) \
         lands one deliberately. Found: {offenders:?}. This changes ONLY via the \
         corresponding port spec — not an ad-hoc dep add.",
    );
}
