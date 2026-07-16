//! CS-1R R2 — the **single production gate walker**, shared (via `#[path]`)
//! by `purity_gate.rs` and `rp_cs1_4_contract_dag.rs` **and** by the RED-pin
//! fixtures, so a fixture exercises the REAL gate code, never a test-only copy.
//!
//! ## Placement (R2 §2) — DEV-ONLY, on purpose
//! This module is the production gate implementation, but it is **test-tier
//! code**. It MUST NOT move into `prro-domain`'s `lib`: that would make
//! `serde_json` a **normal** (non-dev) dependency and break the R2.1 direct
//! allowlist `{uuid, serde, thiserror}`. It lives under `tests/support/` and is
//! pulled into each integration-test binary with
//! `#[path = "support/metadata_graph.rs"] mod metadata_graph;`. `serde_json`
//! stays a `[dev-dependencies]` entry.
//!
//! ## R2.6 — honest scope limit
//! These gates prove **dependency-graph purity** over the RESOLVED
//! `--all-features` `cargo metadata` closure, plus a pinned transitive-closure
//! manifest. They do **NOT** prove the absence of `std::{fs,net,process}`
//! syscalls in the source — Cargo cannot see a raw syscall, only a crate edge.
//! Syscall-level purity is explicitly OUT OF SCOPE for CS-1R.
//!
//! ## R2.4 — PackageId traversal
//! The closure walk dedups `reached` on the **`PackageId`** (`resolve` node id /
//! `dep.pkg`), NEVER the crate name — two same-name versions are distinct nodes
//! and both must be visited. The name is used ONLY for the allow/deny/manifest
//! match. The root is resolved by **`workspace_members` id**, not by name, so a
//! foreign package sharing the root's name cannot masquerade as the root.

#![allow(dead_code)] // each test binary uses a different subset of this module.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::process::Command;

use serde_json::Value;

/// A resolve-node / dependency package id. Opaque string; the ONLY key we dedup
/// the traversal on (R2.4). Two same-name crates at different versions have
/// distinct `PackageId`s.
pub type PackageId = String;

// ---------------------------------------------------------------------------
// R2.3 — the exact, pinned `cargo metadata` argument vector.
// ---------------------------------------------------------------------------

/// The EXACT argument vector both gates pass to `cargo`. Pinned by the permanent
/// unit-pin (`metadata_args_are_pinned`, RP-R2-b2) so that deleting
/// `--all-features` fails FOREVER — not only under a one-time canary. Today's
/// `prro-domain` graph has no optional deps, so a bare `--all-features` removal
/// would otherwise stay green (the closure is identical); the arg-pin is what
/// keeps the flag load-bearing regardless.
pub const METADATA_ARGS: &[&str] = &[
    "metadata",
    "--format-version",
    "1",
    "--all-features",
    "--locked",
];

/// Return the pinned arg vector as owned strings (what the gate actually passes
/// to `Command::args`). Kept as a fn so the permanent arg-pin can assert on the
/// SAME value the live gate uses.
pub fn metadata_args() -> Vec<String> {
    METADATA_ARGS.iter().map(|s| s.to_string()).collect()
}

/// Run `cargo metadata` with the pinned arg vector from `prro-domain`'s manifest
/// dir and return the parsed JSON. `SQLX_OFFLINE=true` keeps resolution from
/// touching a live DB / network.
pub fn run_cargo_metadata() -> Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(cargo)
        .args(metadata_args())
        .current_dir(manifest_dir)
        .env("SQLX_OFFLINE", "true")
        .output()
        .expect("failed to invoke `cargo metadata`");
    assert!(
        out.status.success(),
        "`cargo metadata {}` failed: {}",
        metadata_args().join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("`cargo metadata` did not emit valid JSON")
}

// ---------------------------------------------------------------------------
// Canonical node + edge records (R2.2). kind/target belong to the EDGE.
// ---------------------------------------------------------------------------

/// One resolved package. `enabled_features` is sorted for a stable set compare.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeRecord {
    pub package_id: PackageId,
    pub name: String,
    pub version: String,
    /// `null` for a workspace path package; a registry/git URL otherwise.
    pub source: Option<String>,
    pub enabled_features: Vec<String>,
}

/// One `{kind, target}` pair on a dependency edge. `kind == None` = a normal
/// (runtime) dependency; `Some("build")` = a build-dependency; `Some("dev")` =
/// a dev-dependency (excluded from the non-dev closure).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DepKind {
    pub kind: Option<String>,
    pub target: Option<String>,
}

/// One dependency edge. `kind`/`target` live HERE (on the edge, per R2.2), never
/// on the node — the same package can be reached by a normal edge and a
/// target-gated build edge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeRecord {
    pub from_package_id: PackageId,
    pub to_package_id: PackageId,
    pub dependency_alias: String,
    pub dep_kinds: Vec<DepKind>,
}

// ---------------------------------------------------------------------------
// The graph.
// ---------------------------------------------------------------------------

/// The parsed `cargo metadata` graph: the `packages` table, the `resolve` node
/// table, and the `workspace_members` id list. Built by [`build_graph`] from
/// EITHER live metadata OR a JSON fixture — the same code path either way.
pub struct Graph {
    /// `package_id -> (name, version, source)` from the `packages` table.
    packages: HashMap<PackageId, (String, String, Option<String>)>,
    /// `package_id -> resolve node` (`features` + `deps`).
    nodes: HashMap<PackageId, Value>,
    /// Ordered `workspace_members` ids (root resolution is by id, R2.4).
    workspace_members: Vec<PackageId>,
}

/// An edge kept during traversal: its target `PackageId` plus the alias used for
/// the canonical [`EdgeRecord`].
struct KeptEdge {
    to: PackageId,
    alias: String,
    dep_kinds: Vec<DepKind>,
}

/// Parse a `cargo metadata` JSON value (live or fixture) into a [`Graph`].
pub fn build_graph(meta: &Value) -> Graph {
    let mut packages = HashMap::new();
    for p in meta["packages"].as_array().expect("packages array") {
        let id = p["id"].as_str().expect("package id").to_string();
        let name = p["name"].as_str().expect("package name").to_string();
        let version = p["version"].as_str().expect("package version").to_string();
        let source = p.get("source").and_then(|s| s.as_str()).map(str::to_string);
        packages.insert(id, (name, version, source));
    }

    let mut nodes = HashMap::new();
    for n in meta["resolve"]["nodes"]
        .as_array()
        .expect("resolve.nodes array")
    {
        let id = n["id"].as_str().expect("node id").to_string();
        nodes.insert(id, n.clone());
    }

    let workspace_members = meta["workspace_members"]
        .as_array()
        .expect("workspace_members array")
        .iter()
        .map(|m| m.as_str().expect("member id").to_string())
        .collect();

    Graph {
        packages,
        nodes,
        workspace_members,
    }
}

impl Graph {
    /// Resolve a workspace-member ROOT `PackageId` by NAME, but ONLY among
    /// `workspace_members` ids (R2.4) — a foreign, same-named package elsewhere
    /// in the graph can never be selected as the root.
    pub fn workspace_root_id(&self, root_name: &str) -> PackageId {
        let mut hit: Option<PackageId> = None;
        for id in &self.workspace_members {
            if let Some((name, _, _)) = self.packages.get(id) {
                if name == root_name {
                    assert!(
                        hit.is_none(),
                        "ambiguous workspace root: two members named `{root_name}`"
                    );
                    hit = Some(id.clone());
                }
            }
        }
        hit.unwrap_or_else(|| {
            panic!("workspace member `{root_name}` not found in workspace_members")
        })
    }

    /// The crate NAME for a `PackageId` (allow/deny/manifest match only).
    /// Falls back to the id string itself if the id is not in the packages
    /// table (should not happen for a well-formed graph).
    pub fn name_of(&self, id: &str) -> String {
        self.packages
            .get(id)
            .map(|(n, _, _)| n.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// Non-dev edges out of `id`: kept iff any `dep_kind` is normal (`kind ==
    /// null`) or `build`. A pure-dev edge (every `dep_kind` is `"dev"`) never
    /// ships downstream and is excluded.
    fn kept_edges(&self, id: &str) -> Vec<KeptEdge> {
        let mut out = Vec::new();
        let node = match self.nodes.get(id) {
            Some(n) => n,
            None => return out,
        };
        for dep in node["deps"].as_array().unwrap_or(&Vec::new()) {
            let to = dep["pkg"].as_str().expect("dep pkg id").to_string();
            let dep_kinds = parse_dep_kinds(dep);
            let is_non_dev = dep_kinds
                .iter()
                .any(|k| k.kind.is_none() || k.kind.as_deref() == Some("build"));
            if !is_non_dev {
                continue;
            }
            let alias = dep
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.push(KeptEdge {
                to,
                alias,
                dep_kinds,
            });
        }
        out
    }

    /// **R2.4 core**: the set of `PackageId`s reachable from `root_id` following
    /// only non-dev edges, dedup'd on **`PackageId`** (never name). The root id
    /// itself is NOT included in the returned set.
    pub fn non_dev_closure(&self, root_id: &PackageId) -> HashSet<PackageId> {
        let mut reached: HashSet<PackageId> = HashSet::new();
        let mut queue: VecDeque<PackageId> = VecDeque::new();
        queue.push_back(root_id.clone());
        // Visited includes the root so we do not re-enqueue it, but we do not
        // put the root in `reached` (callers want the dependency set only).
        let mut visited: HashSet<PackageId> = HashSet::new();
        visited.insert(root_id.clone());

        while let Some(id) = queue.pop_front() {
            for edge in self.kept_edges(&id) {
                // Dedup on PackageId — the load-bearing R2.4 line.
                if visited.insert(edge.to.clone()) {
                    reached.insert(edge.to.clone());
                    queue.push_back(edge.to);
                }
            }
        }
        reached
    }

    /// The set of crate NAMES reachable non-dev from `root_id` (convenience for
    /// the forbidden-crate / contract-DAG assertions). Built ON TOP of the
    /// PackageId closure, so it inherits the R2.4 dedup.
    pub fn non_dev_closure_names(&self, root_id: &PackageId) -> HashSet<String> {
        self.non_dev_closure(root_id)
            .iter()
            .map(|id| self.name_of(id).to_string())
            .collect()
    }

    /// The canonical **node** records for the non-dev closure of `root_id`
    /// (INCLUDING the root node itself, so a change to the root's resolved
    /// features is also pinned).
    pub fn closure_node_records(&self, root_id: &PackageId) -> BTreeSet<NodeRecord> {
        let mut ids: HashSet<PackageId> = self.non_dev_closure(root_id);
        ids.insert(root_id.clone());
        let mut out = BTreeSet::new();
        for id in &ids {
            let (name, version, source) = self
                .packages
                .get(id)
                .expect("closure id must be a known package")
                .clone();
            let mut feats: Vec<String> = self
                .nodes
                .get(id)
                .and_then(|n| n["features"].as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|f| f.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            feats.sort();
            out.insert(NodeRecord {
                package_id: id.clone(),
                name,
                version,
                source,
                enabled_features: feats,
            });
        }
        out
    }

    /// The canonical **edge** records within the non-dev closure of `root_id`
    /// (edges FROM the root and from every reached node, to a reached node).
    /// kind/target live on the edge (R2.2).
    pub fn closure_edge_records(&self, root_id: &PackageId) -> BTreeSet<EdgeRecord> {
        let mut ids: HashSet<PackageId> = self.non_dev_closure(root_id);
        ids.insert(root_id.clone());
        let mut out = BTreeSet::new();
        for from in &ids {
            for edge in self.kept_edges(from) {
                // Only edges that stay inside the non-dev closure.
                if !ids.contains(&edge.to) {
                    continue;
                }
                out.insert(EdgeRecord {
                    from_package_id: from.clone(),
                    to_package_id: edge.to,
                    dependency_alias: edge.alias,
                    dep_kinds: edge.dep_kinds,
                });
            }
        }
        out
    }

    /// The **direct** dependency edges declared on a package's `packages`-table
    /// entry, split by kind. Returns `(normal_or_target_names, build_dep_names)`
    /// where the first excludes `dev`/`build` and the second is `build` only.
    /// Used by R2.1: direct non-dev allowlist + empty-build-dep assertion.
    pub fn direct_deps(&self, meta: &Value, root_name: &str) -> DirectDeps {
        let pkg = meta["packages"]
            .as_array()
            .expect("packages array")
            .iter()
            .find(|p| {
                p["name"].as_str() == Some(root_name)
                    && self.workspace_members.iter().any(|m| {
                        self.packages
                            .get(m)
                            .map(|(n, _, _)| n == root_name)
                            .unwrap_or(false)
                            && p["id"].as_str() == Some(m.as_str())
                    })
            })
            .unwrap_or_else(|| panic!("workspace package `{root_name}` not found"));

        let mut normal = BTreeSet::new();
        let mut build = BTreeSet::new();
        for d in pkg["dependencies"].as_array().expect("dependencies array") {
            let name = d["name"].as_str().expect("dep name").to_string();
            match d.get("kind").and_then(|v| v.as_str()) {
                None => {
                    normal.insert(name);
                } // normal runtime dep (kind field absent/null)
                Some("build") => {
                    build.insert(name);
                }
                Some("dev") => {} // dev deps do not ship downstream
                Some(other) => panic!("unknown dependency kind `{other}` on `{name}`"),
            }
        }
        DirectDeps { normal, build }
    }

    /// CS-1R2 A3a — the FULL canonical record of every NON-DEV direct dependency
    /// declaration on a workspace package. `direct_deps` above returns NAMES only,
    /// so a feature / version-req / source / default-features / rename change on an
    /// allowlisted direct dep (e.g. adding `rng` to `uuid`) was INVISIBLE to the
    /// gate. This returns the pinnable record so any such change is RED.
    pub fn direct_dep_records(&self, meta: &Value, root_name: &str) -> Vec<DirectDepRecord> {
        let pkg = meta["packages"]
            .as_array()
            .expect("packages array")
            .iter()
            .find(|p| {
                p["name"].as_str() == Some(root_name)
                    && self.workspace_members.iter().any(|m| {
                        self.packages
                            .get(m)
                            .map(|(n, _, _)| n == root_name)
                            .unwrap_or(false)
                            && p["id"].as_str() == Some(m.as_str())
                    })
            })
            .unwrap_or_else(|| panic!("workspace package `{root_name}` not found"));

        let mut out = Vec::new();
        for d in pkg["dependencies"].as_array().expect("dependencies array") {
            let kind = d.get("kind").and_then(|v| v.as_str()).map(str::to_string);
            if kind.as_deref() == Some("dev") {
                continue; // dev deps do not ship downstream
            }
            let mut features: Vec<String> = d
                .get("features")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            features.sort();
            out.push(DirectDepRecord {
                name: d["name"].as_str().expect("dep name").to_string(),
                rename: d.get("rename").and_then(|v| v.as_str()).map(str::to_string),
                req: d.get("req").and_then(|v| v.as_str()).map(str::to_string),
                source: d.get("source").and_then(|v| v.as_str()).map(str::to_string),
                kind,
                target: d.get("target").and_then(|v| v.as_str()).map(str::to_string),
                optional: d.get("optional").and_then(|v| v.as_bool()).unwrap_or(false),
                uses_default_features: d
                    .get("uses_default_features")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                features,
            });
        }
        // deterministic order for diffable pin: (name, rename, kind, target)
        out.sort_by(|a, b| {
            (&a.name, &a.rename, &a.kind, &a.target).cmp(&(&b.name, &b.rename, &b.kind, &b.target))
        });
        out
    }
}

/// Direct declared deps of a package, split into non-dev (normal/target) and
/// build. Both exclude dev.
pub struct DirectDeps {
    pub normal: BTreeSet<String>,
    pub build: BTreeSet<String>,
}

/// CS-1R2 A3a — the full pinnable record of one NON-DEV direct dependency
/// declaration (the fields cargo-metadata exposes on `packages[].dependencies[]`).
/// A feature / version-req / source / default-features / kind / target / rename
/// change on an allowlisted dep changes this record → the pin RED's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectDepRecord {
    pub name: String,
    pub rename: Option<String>,
    pub req: Option<String>,
    pub source: Option<String>,
    pub kind: Option<String>,
    pub target: Option<String>,
    pub optional: bool,
    pub uses_default_features: bool,
    pub features: Vec<String>,
}

impl DirectDepRecord {
    /// A stable one-line canonical rendering for a diffable, human-legible pin.
    pub fn canonical(&self) -> String {
        format!(
            "name={} rename={:?} req={:?} source={:?} kind={:?} target={:?} optional={} default_features={} features={:?}",
            self.name,
            self.rename,
            self.req,
            self.source,
            self.kind,
            self.target,
            self.optional,
            self.uses_default_features,
            self.features,
        )
    }
}

// ---------------------------------------------------------------------------
// R2.2 — pinned transitive-closure manifest (`purity-closure.lock`).
// ---------------------------------------------------------------------------

/// The committed closure manifest, deserialized from `purity-closure.lock`.
/// Two set-equality tables (`nodes` + `edges`) plus the accepted-node
/// annotations (`getrandom` / `libc`).
pub struct ClosureManifest {
    pub root: PackageId,
    /// Accepted capability nodes → their justification annotation.
    /// MUST contain `getrandom` (OS entropy for UUID v7); `libc` if present.
    pub acceptance: std::collections::BTreeMap<String, String>,
    pub nodes: BTreeSet<NodeRecord>,
    pub edges: BTreeSet<EdgeRecord>,
}

/// A `path+file://…` root id embeds the absolute worktree path, which differs
/// per checkout. Normalize it to a path-independent token so the committed lock
/// matches regardless of where the repo lives. Registry/git ids are already
/// stable and pass through unchanged.
pub fn normalize_package_id(id: &str) -> String {
    if let Some(rest) = id.strip_prefix("path+file://") {
        // rest = `/abs/path/to/<crate-dir>#<version>`
        if let Some((path, ver)) = rest.rsplit_once('#') {
            let dir = path.rsplit('/').next().unwrap_or(path);
            return format!("path+WORKSPACE:{dir}#{ver}");
        }
    }
    id.to_string()
}

/// Load and parse the committed `purity-closure.lock` from `prro-domain`'s
/// manifest dir.
pub fn load_closure_manifest() -> ClosureManifest {
    let path = format!("{}/purity-closure.lock", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read committed closure manifest `{path}`: {e}"));
    let v: Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("`{path}` is not valid JSON: {e}"));

    let root = v["root"].as_str().expect("manifest.root").to_string();

    let mut acceptance = std::collections::BTreeMap::new();
    for (k, val) in v["acceptance"].as_object().expect("manifest.acceptance") {
        acceptance.insert(
            k.clone(),
            val.as_str().expect("acceptance value").to_string(),
        );
    }

    let mut nodes = BTreeSet::new();
    for n in v["nodes"].as_array().expect("manifest.nodes") {
        let mut feats: Vec<String> = n["enabled_features"]
            .as_array()
            .expect("node.enabled_features")
            .iter()
            .map(|f| f.as_str().expect("feature").to_string())
            .collect();
        feats.sort();
        nodes.insert(NodeRecord {
            package_id: n["package_id"]
                .as_str()
                .expect("node.package_id")
                .to_string(),
            name: n["name"].as_str().expect("node.name").to_string(),
            version: n["version"].as_str().expect("node.version").to_string(),
            source: n.get("source").and_then(|s| s.as_str()).map(str::to_string),
            enabled_features: feats,
        });
    }

    let mut edges = BTreeSet::new();
    for e in v["edges"].as_array().expect("manifest.edges") {
        let mut dep_kinds: Vec<DepKind> = e["dep_kinds"]
            .as_array()
            .expect("edge.dep_kinds")
            .iter()
            .map(|k| DepKind {
                kind: k.get("kind").and_then(|x| x.as_str()).map(str::to_string),
                target: k.get("target").and_then(|x| x.as_str()).map(str::to_string),
            })
            .collect();
        dep_kinds.sort();
        edges.insert(EdgeRecord {
            from_package_id: e["from_package_id"]
                .as_str()
                .expect("edge.from")
                .to_string(),
            to_package_id: e["to_package_id"].as_str().expect("edge.to").to_string(),
            dependency_alias: e["dependency_alias"]
                .as_str()
                .expect("edge.alias")
                .to_string(),
            dep_kinds,
        });
    }

    ClosureManifest {
        root,
        acceptance,
        nodes,
        edges,
    }
}

/// Apply [`normalize_package_id`] to a whole node-record set (for live-vs-lock
/// comparison, so absolute worktree paths never leak into the diff).
pub fn normalize_node_records(recs: &BTreeSet<NodeRecord>) -> BTreeSet<NodeRecord> {
    recs.iter()
        .map(|r| NodeRecord {
            package_id: normalize_package_id(&r.package_id),
            ..r.clone()
        })
        .collect()
}

/// Apply [`normalize_package_id`] to a whole edge-record set.
pub fn normalize_edge_records(recs: &BTreeSet<EdgeRecord>) -> BTreeSet<EdgeRecord> {
    recs.iter()
        .map(|r| EdgeRecord {
            from_package_id: normalize_package_id(&r.from_package_id),
            to_package_id: normalize_package_id(&r.to_package_id),
            ..r.clone()
        })
        .collect()
}

/// Parse the `dep_kinds` array of a resolve-node dependency into canonical
/// [`DepKind`] records (sorted for a stable edge compare).
fn parse_dep_kinds(dep: &Value) -> Vec<DepKind> {
    let mut v: Vec<DepKind> = dep["dep_kinds"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|k| DepKind {
                    kind: k.get("kind").and_then(|x| x.as_str()).map(str::to_string),
                    target: k.get("target").and_then(|x| x.as_str()).map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}
