//! RP-CS1-1 (structural purity — PRIMARY = cargo-metadata) & RP-CS1-4
//! (contract-crate DAG: `prro-testkit` graph-absence).
//!
//! The contract (`docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §7) requires the purity invariant to be enforced against the RESOLVED
//! dependency graph, not merely at `use`-site: a `trybuild` compile-fail canary
//! (`purity_gate_compile_fail.rs`) is a fast smoke, but only `cargo metadata`
//! catches alias / build-dependency / target-specific-dependency evasion.
//!
//! Both tests shell out to `cargo metadata --format-version 1` and walk the
//! `resolve` node graph, following ONLY non-dev edges (`kind: null` = normal,
//! or `kind: "build"`, including target-specific ones) — dev-dependencies do
//! not ship in a downstream binary, so they are excluded by design.

use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;

/// Crates that must NEVER appear in `prro-domain`'s normal/build/target
/// dependency closure (contract §9 — I/O, async runtime, DB, RPC, HTTP).
const FORBIDDEN: &[&str] = &[
    "sqlx", "tonic", "tokio", "axum", "prost", "hyper", "reqwest",
];

/// Run `cargo metadata --format-version 1` from this crate's manifest dir and
/// return the parsed JSON.
fn cargo_metadata() -> serde_json::Value {
    // `CARGO` is set by the test harness to the exact cargo that invoked the
    // build; fall back to the plain binary otherwise.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1"])
        .current_dir(manifest_dir)
        // Do not require a live DB / network for metadata resolution.
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

/// Extract the bare crate name from a resolve-node package id, tolerating both
/// the modern `…#name@version` / `…#name#version` forms and the older
/// `name version (source)` form.
fn pkg_name_of(id: &str) -> String {
    // Modern form: the part after the last '#', minus a trailing `@version` or
    // `#version`, OR `…/path#name@ver`. Simplest robust path: match the id back
    // to the `packages` table below. We keep this helper only for a readable
    // fallback in messages; the authoritative name comes from the id→name map.
    if let Some((_, tail)) = id.rsplit_once('#') {
        // tail may be `name@ver`, `ver` (when name was before the last '#'), etc.
        if let Some((name, _)) = tail.split_once('@') {
            return name.to_string();
        }
        return tail.to_string();
    }
    id.split_whitespace().next().unwrap_or(id).to_string()
}

/// Build an id → crate-name map from the `packages` table (authoritative).
fn id_name_map(meta: &serde_json::Value) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for p in meta["packages"].as_array().expect("packages array") {
        let id = p["id"].as_str().expect("package id").to_string();
        let name = p["name"].as_str().expect("package name").to_string();
        m.insert(id, name);
    }
    m
}

/// Compute the set of crate NAMES reachable from `root_name` following only
/// non-dev edges (normal + build, any target) in the resolve graph.
fn non_dev_closure(meta: &serde_json::Value, root_name: &str) -> HashSet<String> {
    let names = id_name_map(meta);
    let nodes = meta["resolve"]["nodes"]
        .as_array()
        .expect("resolve.nodes array");

    // Index nodes by id.
    let by_id: HashMap<&str, &serde_json::Value> = nodes
        .iter()
        .map(|n| (n["id"].as_str().expect("node id"), n))
        .collect();

    // Find the root node id.
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
            // Keep the edge only if it has at least one NON-dev kind
            // (kind == null → normal, or kind == "build"). A pure dev edge
            // (every dep_kind is "dev") is excluded — it never ships downstream.
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

/// RP-CS1-1: `prro-domain`'s normal+build+target dependency closure contains
/// none of the forbidden I/O / runtime / DB crates.
///
/// Canary demonstration (must be done by hand, restored clean): temporarily add
/// e.g. `sqlx = "0.8"` to `prro-domain/Cargo.toml` `[dependencies]` → this test
/// FAILS; remove it → it PASSES.
#[test]
fn prro_domain_is_sqlx_and_io_free() {
    let meta = cargo_metadata();
    let closure = non_dev_closure(&meta, "prro-domain");

    let violations: Vec<&str> = FORBIDDEN
        .iter()
        .copied()
        .filter(|f| closure.contains(*f))
        .collect();

    assert!(
        violations.is_empty(),
        "prro-domain (contract §9) must be sqlx-free / I/O-free / runtime-free, \
         but its normal+build+target dependency closure reached forbidden crate(s): {violations:?}.\n\
         Full non-dev closure: {closure:?}",
    );
}

/// RP-CS1-4: `prro-testkit` is absent from every PRODUCTION package's
/// normal/build dependency graph. It may be a workspace member that
/// `cargo build --workspace` compiles — the invariant is graph-absence from
/// prod deps, not membership.
///
/// "Production package" here = every workspace member EXCEPT `prro-testkit`
/// itself. (A test target pulling `prro-testkit` in as a `dev-dependency` is
/// allowed and, being a dev edge, is invisible to `non_dev_closure`.)
#[test]
fn prro_testkit_absent_from_production_graphs() {
    let meta = cargo_metadata();

    // Workspace member ids → names.
    let names = id_name_map(&meta);
    let members: Vec<String> = meta["workspace_members"]
        .as_array()
        .expect("workspace_members")
        .iter()
        .map(|id| {
            let id = id.as_str().expect("member id");
            names.get(id).cloned().unwrap_or_else(|| pkg_name_of(id))
        })
        .collect();

    assert!(
        members.iter().any(|m| m == "prro-testkit"),
        "prro-testkit should be a workspace member; members = {members:?}"
    );

    let mut offenders: Vec<String> = Vec::new();
    for member in &members {
        if member == "prro-testkit" {
            continue; // itself
        }
        let closure = non_dev_closure(&meta, member);
        if closure.contains("prro-testkit") {
            offenders.push(member.clone());
        }
    }

    assert!(
        offenders.is_empty(),
        "prro-testkit must stay ABSENT from every production package's \
         normal/build dependency graph (RP-CS1-4), but it is reachable from: {offenders:?}. \
         Depend on it only as a `[dev-dependencies]`.",
    );
}
