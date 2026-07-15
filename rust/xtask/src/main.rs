//! `cargo xtask update-purity-closure` — regenerate
//! `rust/prro-domain/purity-closure.lock` from live `cargo metadata`.
//!
//! CS-1R R2.2: the closure manifest is regenerated ONLY by this explicit,
//! human-run command; **CI never auto-updates it** (a legitimate version bump is
//! meant to turn the gate RED so a human reviews and re-mints the lock here).
//!
//! The walker semantics MUST match the gate's shared walker
//! (`prro-domain/tests/support/metadata_graph.rs`): the pinned arg vector,
//! non-dev edges only, PackageId-dedup, root-by-workspace-id, and the same
//! `path+file://` id normalization. The gate's set-equality check is the
//! cross-verification that this generator agrees with the gate.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::process::Command;

use serde_json::{json, Value};

/// The pinned arg vector — MUST match `metadata_graph::METADATA_ARGS`.
const METADATA_ARGS: &[&str] = &[
    "metadata",
    "--format-version",
    "1",
    "--all-features",
    "--locked",
];

/// Accepted capability nodes → justification annotation (R2.2). Emitted only for
/// nodes actually present in the closure.
fn acceptance_for(name: &str) -> Option<&'static str> {
    match name {
        "getrandom" => Some("OS entropy for UUID v7 random bits"),
        "libc" => Some("getrandom syscall ABI"),
        _ => None,
    }
}

fn normalize_package_id(id: &str) -> String {
    if let Some(rest) = id.strip_prefix("path+file://") {
        if let Some((path, ver)) = rest.rsplit_once('#') {
            let dir = path.rsplit('/').next().unwrap_or(path);
            return format!("path+WORKSPACE:{dir}#{ver}");
        }
    }
    id.to_string()
}

fn parse_dep_kinds(dep: &Value) -> Vec<(Option<String>, Option<String>)> {
    let mut v: Vec<(Option<String>, Option<String>)> = dep["dep_kinds"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|k| {
                    (
                        k.get("kind").and_then(|x| x.as_str()).map(str::to_string),
                        k.get("target").and_then(|x| x.as_str()).map(str::to_string),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn is_non_dev(kinds: &[(Option<String>, Option<String>)]) -> bool {
    kinds
        .iter()
        .any(|(k, _)| k.is_none() || k.as_deref() == Some("build"))
}

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("update-purity-closure") => update_purity_closure(),
        other => {
            eprintln!(
                "unknown xtask `{}`. available: update-purity-closure",
                other.unwrap_or("")
            );
            std::process::exit(2);
        }
    }
}

fn update_purity_closure() {
    // prro-domain manifest dir, relative to the workspace root xtask runs from.
    let manifest_root = env!("CARGO_MANIFEST_DIR"); // .../rust/xtask
    let ws_rust = std::path::Path::new(manifest_root)
        .parent()
        .expect("xtask parent");
    let domain_dir = ws_rust.join("prro-domain");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = Command::new(&cargo)
        .args(METADATA_ARGS)
        .current_dir(&domain_dir)
        .env("SQLX_OFFLINE", "true")
        .output()
        .expect("failed to invoke cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: Value = serde_json::from_slice(&out.stdout).expect("metadata json");

    let packages: HashMap<String, (String, String, Option<String>)> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_string(),
                (
                    p["name"].as_str().unwrap().to_string(),
                    p["version"].as_str().unwrap().to_string(),
                    p.get("source").and_then(|s| s.as_str()).map(str::to_string),
                ),
            )
        })
        .collect();
    let nodes: HashMap<String, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap().to_string(), n))
        .collect();

    // root = workspace member named prro-domain.
    let root = meta["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .find(|id| {
            packages
                .get(id)
                .map(|(n, _, _)| n == "prro-domain")
                .unwrap_or(false)
        })
        .expect("prro-domain workspace member");

    // PackageId non-dev closure.
    let mut reached: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(root.clone());
    let mut q: VecDeque<String> = VecDeque::new();
    q.push_back(root.clone());
    while let Some(id) = q.pop_front() {
        if let Some(n) = nodes.get(&id) {
            for dep in n["deps"].as_array().unwrap_or(&Vec::new()) {
                let to = dep["pkg"].as_str().unwrap().to_string();
                if is_non_dev(&parse_dep_kinds(dep)) && visited.insert(to.clone()) {
                    reached.insert(to.clone());
                    q.push_back(to);
                }
            }
        }
    }
    let mut ids: HashSet<String> = reached.clone();
    ids.insert(root.clone());

    // nodes table, sorted by (name, version, id).
    let mut node_rows: Vec<Value> = Vec::new();
    let mut sorted_ids: Vec<&String> = ids.iter().collect();
    sorted_ids.sort_by(|a, b| {
        let pa = &packages[*a];
        let pb = &packages[*b];
        (pa.0.as_str(), pa.1.as_str(), a.as_str()).cmp(&(pb.0.as_str(), pb.1.as_str(), b.as_str()))
    });
    let mut acceptance: BTreeMap<String, String> = BTreeMap::new();
    for id in &sorted_ids {
        let (name, version, source) = &packages[*id];
        let mut feats: Vec<String> = nodes
            .get(*id)
            .and_then(|n| n["features"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        feats.sort();
        if let Some(ann) = acceptance_for(name) {
            acceptance.insert(name.clone(), ann.to_string());
        }
        node_rows.push(json!({
            "package_id": normalize_package_id(id),
            "name": name,
            "version": version,
            "source": source,
            "enabled_features": feats,
        }));
    }

    // edges table (edges whose BOTH ends are inside the closure).
    // (from_id, to_id, alias, sorted dep_kinds) — a canonical edge tuple.
    type EdgeTuple = (
        String,
        String,
        String,
        Vec<(Option<String>, Option<String>)>,
    );
    let mut edge_set: BTreeSet<EdgeTuple> = BTreeSet::new();
    for from in &ids {
        if let Some(n) = nodes.get(from) {
            for dep in n["deps"].as_array().unwrap_or(&Vec::new()) {
                let to = dep["pkg"].as_str().unwrap().to_string();
                let kinds = parse_dep_kinds(dep);
                if is_non_dev(&kinds) && ids.contains(&to) {
                    let alias = dep
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    edge_set.insert((from.clone(), to, alias, kinds));
                }
            }
        }
    }
    // sort edges by (from-name, to-name, alias) for a stable file.
    let mut edge_vec: Vec<_> = edge_set.into_iter().collect();
    edge_vec.sort_by(|a, b| {
        let (fa, fb) = (&packages[&a.0].0, &packages[&b.0].0);
        let (ta, tb) = (&packages[&a.1].0, &packages[&b.1].0);
        (fa.as_str(), ta.as_str(), a.2.as_str()).cmp(&(fb.as_str(), tb.as_str(), b.2.as_str()))
    });
    let edge_rows: Vec<Value> = edge_vec
        .iter()
        .map(|(f, t, alias, kinds)| {
            json!({
                "from_package_id": normalize_package_id(f),
                "to_package_id": normalize_package_id(t),
                "dependency_alias": alias,
                "dep_kinds": kinds.iter().map(|(k, tg)| json!({"kind": k, "target": tg})).collect::<Vec<_>>(),
            })
        })
        .collect();

    let doc = json!({
        "schema": 1,
        "root": normalize_package_id(&root),
        "acceptance": acceptance,
        "nodes": node_rows,
        "edges": edge_rows,
    });

    let lock_path = domain_dir.join("purity-closure.lock");
    let pretty = serde_json::to_string_pretty(&doc).expect("serialize");
    std::fs::write(&lock_path, format!("{pretty}\n")).expect("write lock");
    eprintln!(
        "regenerated {} ({} nodes, {} edges)",
        lock_path.display(),
        node_rows.len(),
        edge_rows.len()
    );
}
