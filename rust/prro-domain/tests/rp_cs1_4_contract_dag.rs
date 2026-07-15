//! RP-CS1-4 (contract-crate DAG) — the CS-1d half.
//!
//! The contract (`docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §7 RP-CS1-4, §8 item 5) creates the three boundary crates
//! `prro-ingress-contract` / `prro-dps-contract` / `prro-fleet-contract` as
//! **empty dependency skeletons**. This test pins, over the RESOLVED
//! `cargo metadata` graph, the three invariants that keep those ports from
//! silently coupling before their ports even land:
//!
//!   1. all three `*-contract` crates ARE workspace members;
//!   2. NONE of them reaches a forbidden adapter/IO crate
//!      (`sqlx / tonic / tokio / axum / prost / hyper / reqwest`) in its
//!      normal+build+target dependency closure — the same walker/forbidden-set
//!      the RP-CS1-1 purity gate uses for `prro-domain`;
//!   3. NO contract crate depends on another contract crate
//!      (ingress ⊥ dps ⊥ fleet) — a hard DAG invariant so the ports never
//!      couple sideways.
//!
//! Walker semantics are IDENTICAL to `tests/purity_gate.rs`: shell out to
//! `cargo metadata --format-version 1`, follow ONLY non-dev edges (`kind: null`
//! = normal, or `kind: "build"`, including target-specific ones); dev edges are
//! excluded (they never ship in a downstream binary).
//!
//! RED→GREEN provenance (CS-1d): authored while the three crates were ABSENT
//! from `rust/Cargo.toml` → assertion (1) FAILED (`workspace_members` lacked
//! them) → the empty crates were added → GREEN. Canary for assertion (3):
//! temporarily add `prro-dps-contract` to `prro-ingress-contract`'s
//! `[dependencies]` → this test goes RED on the "contract ⊥ contract" edge →
//! revert.

use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;

/// The three boundary crates created in CS-1d (contract §8 item 5).
const CONTRACT_CRATES: &[&str] = &[
    "prro-ingress-contract",
    "prro-dps-contract",
    "prro-fleet-contract",
];

/// Crates that must NEVER appear in a contract crate's normal/build/target
/// dependency closure (contract §7 RP-CS1-4 — mirror of the RP-CS1-1 set).
const FORBIDDEN: &[&str] = &[
    "sqlx", "tonic", "tokio", "axum", "prost", "hyper", "reqwest",
];

/// Run `cargo metadata --format-version 1` from this crate's manifest dir.
fn cargo_metadata() -> serde_json::Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1"])
        .current_dir(manifest_dir)
        .env("SQLX_OFFLINE", "true")
        .output()
        .expect("failed to invoke `cargo metadata`");
    assert!(
        out.status.success(),
        "`cargo metadata` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("`cargo metadata` did not emit valid JSON")
}

/// Fallback crate-name extractor for a resolve-node package id (readable
/// messages only; the authoritative name comes from the id→name map).
fn pkg_name_of(id: &str) -> String {
    if let Some((_, tail)) = id.rsplit_once('#') {
        if let Some((name, _)) = tail.split_once('@') {
            return name.to_string();
        }
        return tail.to_string();
    }
    id.split_whitespace().next().unwrap_or(id).to_string()
}

/// id → crate-name map from the `packages` table (authoritative).
fn id_name_map(meta: &serde_json::Value) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for p in meta["packages"].as_array().expect("packages array") {
        let id = p["id"].as_str().expect("package id").to_string();
        let name = p["name"].as_str().expect("package name").to_string();
        m.insert(id, name);
    }
    m
}

/// Crate NAMES reachable from `root_name` following only non-dev edges
/// (normal + build, any target) in the resolve graph. Identical to the
/// purity-gate walker.
fn non_dev_closure(meta: &serde_json::Value, root_name: &str) -> HashSet<String> {
    let names = id_name_map(meta);
    let nodes = meta["resolve"]["nodes"]
        .as_array()
        .expect("resolve.nodes array");

    let by_id: HashMap<&str, &serde_json::Value> = nodes
        .iter()
        .map(|n| (n["id"].as_str().expect("node id"), n))
        .collect();

    let root_id = names
        .iter()
        .find(|(_, n)| n.as_str() == root_name)
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| panic!("crate `{root_name}` not found in metadata packages"));

    let mut reached: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(root_id);

    while let Some(id) = queue.pop_front() {
        let node = match by_id.get(id.as_str()) {
            Some(n) => n,
            None => continue,
        };
        for dep in node["deps"].as_array().unwrap_or(&Vec::new()) {
            let dep_id = dep["pkg"].as_str().expect("dep pkg id");
            let is_non_dev = dep["dep_kinds"]
                .as_array()
                .map(|ks| {
                    ks.iter().any(|k| {
                        let kind = k.get("kind").and_then(|v| v.as_str());
                        kind.is_none() || kind == Some("build")
                    })
                })
                .unwrap_or(false);
            if !is_non_dev {
                continue;
            }
            let dep_name = names
                .get(dep_id)
                .cloned()
                .unwrap_or_else(|| pkg_name_of(dep_id));
            if reached.insert(dep_name) {
                queue.push_back(dep_id.to_string());
            }
        }
    }
    reached
}

/// The set of workspace member crate names.
fn workspace_member_names(meta: &serde_json::Value) -> Vec<String> {
    let names = id_name_map(meta);
    meta["workspace_members"]
        .as_array()
        .expect("workspace_members")
        .iter()
        .map(|id| {
            let id = id.as_str().expect("member id");
            names.get(id).cloned().unwrap_or_else(|| pkg_name_of(id))
        })
        .collect()
}

/// RP-CS1-4 (1): the three `*-contract` crates are workspace members.
///
/// This is the assertion that goes RED when the crates are absent from
/// `rust/Cargo.toml` (the CS-1d RED→GREEN pin).
#[test]
fn contract_crates_are_workspace_members() {
    let meta = cargo_metadata();
    let members = workspace_member_names(&meta);

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
/// normal+build+target dependency closure.
#[test]
fn contract_crates_are_io_free() {
    let meta = cargo_metadata();

    let mut violations: Vec<String> = Vec::new();
    for crate_name in CONTRACT_CRATES {
        let closure = non_dev_closure(&meta, crate_name);
        for f in FORBIDDEN {
            if closure.contains(*f) {
                violations.push(format!("{crate_name} -> {f}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "the CS-1d boundary crates must be I/O-free / runtime-free (contract §7 RP-CS1-4), \
         but a forbidden crate is reachable: {violations:?}",
    );
}

/// RP-CS1-4 (3): no contract crate depends (non-dev) on another contract crate
/// — ingress ⊥ dps ⊥ fleet. A hard DAG invariant so the ports never couple.
///
/// Canary: temporarily add e.g. `prro-dps-contract` to
/// `prro-ingress-contract/Cargo.toml` `[dependencies]` → this test FAILS on the
/// `prro-ingress-contract -> prro-dps-contract` edge; revert → PASSES.
#[test]
fn contract_crates_do_not_depend_on_each_other() {
    let meta = cargo_metadata();

    let mut couplings: Vec<String> = Vec::new();
    for crate_name in CONTRACT_CRATES {
        let closure = non_dev_closure(&meta, crate_name);
        for other in CONTRACT_CRATES {
            if other == crate_name {
                continue;
            }
            if closure.contains(*other) {
                couplings.push(format!("{crate_name} -> {other}"));
            }
        }
    }

    assert!(
        couplings.is_empty(),
        "contract crates must stay orthogonal (ingress ⊥ dps ⊥ fleet, contract §7 RP-CS1-4): \
         no contract crate may depend on another, but found: {couplings:?}",
    );
}
