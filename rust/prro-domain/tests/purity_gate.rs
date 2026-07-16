//! CS-1R R2 — **gate soundness** for `prro-domain` purity.
//!
//! ## What this proves (R2.6 — honest scope)
//! Over the RESOLVED `cargo metadata --format-version 1 --all-features --locked`
//! graph, this gate proves THREE things:
//!   1. **R2.1** — `prro-domain`'s DIRECT non-dev dependency set is exactly
//!      `{uuid, serde, thiserror}`, matched on the canonical node+edge records
//!      (not name/source alone); its direct BUILD-dependency set is empty.
//!   2. **R2.2** — the full `--all-features` non-dev transitive closure is
//!      **set-equal** (BOTH the node table and the edge table) to the committed
//!      `purity-closure.lock`. A legitimate version bump turns this RED — the
//!      correct signal for so small a closure. `cargo xtask update-purity-closure`
//!      regenerates the lock; **CI never auto-updates it**.
//!   3. **R2.3** — the metadata arg vector is pinned permanently, so removing
//!      `--all-features` fails FOREVER (not only under a one-time canary).
//!
//! It does **NOT** prove the absence of `std::{fs,net,process}` syscalls in the
//! source (Cargo sees only crate edges, not raw syscalls). Syscall-level purity
//! is OUT OF SCOPE for CS-1R — this is dependency-graph purity + a pinned
//! closure. (R2.6 scope limit, documented here in the gate module.)
//!
//! ## Honesty (R2.5)
//! `prro-domain` has **no DB / network / async-runtime / RPC / HTTP adapter
//! dependencies**. The ONLY capability dependency is `getrandom` (OS entropy for
//! UUID v7 random bits); the v7 timestamp uses `std::time`, NOT a dependency.
//! The earlier "I/O-free / no clock" wording is struck as unprovable-by-Cargo.
//!
//! The single shared production walker lives in `support/metadata_graph.rs` and
//! is pulled into BOTH gate files (this one and `rp_cs1_4_contract_dag.rs`) and
//! into the fixtures below — one code path, no test-only copy.

#[path = "support/metadata_graph.rs"]
mod metadata_graph;

use metadata_graph as mg;
use std::collections::BTreeSet;

/// R2.1 — the EXACT direct non-dev dependency allowlist for `prro-domain`.
const DIRECT_ALLOWLIST: &[&str] = &["serde", "thiserror", "uuid"];

/// Crates that must NEVER appear in `prro-domain`'s non-dev closure — a fast
/// human-readable forbidden set layered ON TOP of the exhaustive closure
/// manifest (the manifest is the authoritative gate; this names the usual
/// suspects for a legible message).
const FORBIDDEN: &[&str] = &[
    "sqlx", "tonic", "tokio", "axum", "prost", "hyper", "reqwest",
];

// ===========================================================================
// R2.3 — PERMANENT arg-pin (RP-R2-b2).
// ===========================================================================

/// **RP-R2-b2 (permanent).** Pin the EXACT `cargo metadata` argument vector the
/// gate uses. `--all-features` and `--locked` are load-bearing: today's
/// `prro-domain` graph has no optional deps, so a bare `--all-features` removal
/// would leave the closure identical and stay green — this pin is what makes
/// dropping the flag fail FOREVER, not merely under a one-time canary.
///
/// Teeth: delete `"--all-features"` (or `"--locked"`) from `mg::METADATA_ARGS`
/// → this test goes RED.
#[test]
fn metadata_args_are_pinned() {
    assert_eq!(
        mg::metadata_args(),
        vec![
            "metadata".to_string(),
            "--format-version".to_string(),
            "1".to_string(),
            "--all-features".to_string(),
            "--locked".to_string(),
        ],
        "R2.3 arg-pin: the cargo-metadata arg vector changed. `--all-features` \
         and `--locked` are REQUIRED (all-features so an optional-dep evasion \
         cannot hide; --locked so a stale lock cannot silently reshape the \
         closure). Do not remove them.",
    );
}

// ===========================================================================
// R2.1 — direct-dependency allowlist + empty build-deps.
// ===========================================================================

/// **R2.1.** `prro-domain`'s DIRECT non-dev deps == exactly `{uuid, serde,
/// thiserror}`, and its DIRECT build-deps are empty. Matched on the canonical
/// records, cross-checked against the pinned node manifest (so a same-named but
/// different-version/source direct dep is also caught).
///
/// RP-R2-a teeth: add `rusqlite` to `[dependencies]` (+ refresh the lock) →
/// this assertion fails naming `rusqlite`; remove → PASS.
#[test]
fn prro_domain_direct_deps_are_exactly_allowlisted() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let direct = graph.direct_deps(&meta, "prro-domain");

    let expected: BTreeSet<String> = DIRECT_ALLOWLIST.iter().map(|s| s.to_string()).collect();

    assert_eq!(
        direct.normal, expected,
        "R2.1 direct allowlist: `prro-domain`'s direct non-dev dependencies must \
         be EXACTLY {{serde, thiserror, uuid}} (the only pure capability deps: \
         serde for representation, thiserror for error types, uuid for the ids). \
         Found: {:?}. A new direct dep (e.g. a DB client like `rusqlite`) is a \
         purity violation — route representation/IO through `prro`'s store-side \
         wrappers, not the domain crate.",
        direct.normal,
    );

    assert!(
        direct.build.is_empty(),
        "R2.1 empty build-deps: `prro-domain` must declare NO `[build-dependencies]` \
         (a build script dep can smuggle codegen/IO tooling into the graph). \
         Found build-deps: {:?}.",
        direct.build,
    );
}

/// CS-1R2 A3a — the EXACT full-record pin of `prro-domain`'s NON-DEV direct deps.
/// `prro_domain_direct_deps_are_exactly_allowlisted` above matches NAMES only —
/// so a feature / version-req / source / default-features / rename change on an
/// allowlisted dep (the auditor's `rng`-on-`uuid` canary) stayed GREEN. This pins
/// every field cargo-metadata exposes on the direct-dep declaration, so any such
/// change is RED. Re-mint via the ignored `print_direct_dep_records` helper when a
/// direct dep legitimately changes.
const DIRECT_DEP_RECORDS: &[&str] = &[
    "name=serde rename=None req=Some(\"^1\") source=Some(\"registry+https://github.com/rust-lang/crates.io-index\") kind=None target=None optional=false default_features=true features=[\"derive\"]",
    "name=thiserror rename=None req=Some(\"^1\") source=Some(\"registry+https://github.com/rust-lang/crates.io-index\") kind=None target=None optional=false default_features=true features=[]",
    "name=uuid rename=None req=Some(\"^1\") source=Some(\"registry+https://github.com/rust-lang/crates.io-index\") kind=None target=None optional=false default_features=true features=[\"serde\", \"v5\", \"v7\"]",
];

#[test]
fn prro_domain_direct_dep_records_are_pinned() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let live: Vec<String> = graph
        .direct_dep_records(&meta, "prro-domain")
        .iter()
        .map(|r| r.canonical())
        .collect();
    let expected: Vec<String> = DIRECT_DEP_RECORDS.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        live, expected,
        "R2.1 direct-dep FULL-RECORD pin (A3a): a direct dependency's feature set / \
         version-req / source / default-features / rename / kind / target changed. \
         The name-only allowlist does NOT catch this (e.g. adding `rng` to `uuid`'s \
         features grows no closure node). If this change is intentional, re-mint via \
         `cargo test -p prro-domain --test purity_gate -- --ignored --nocapture \
         print_direct_dep_records` and review.\n  live    ={live:#?}\n  expected={expected:#?}",
    );
}

/// CS-1R2 A3a — MINT helper (ignored): prints the canonical direct-dep records so
/// `DIRECT_DEP_RECORDS` can be re-pinned when a direct dep legitimately changes.
#[test]
#[ignore = "mint helper — prints the pinned direct-dep records"]
fn print_direct_dep_records() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    for r in graph.direct_dep_records(&meta, "prro-domain") {
        println!("{}", r.canonical());
    }
}

// ===========================================================================
// R2.2 — pinned transitive-closure manifest (node + edge set-equality).
// ===========================================================================

/// **R2.2 (nodes).** The live `--all-features` non-dev node closure is set-equal
/// to the committed `purity-closure.lock` node table.
///
/// RP-R2-a/b teeth: adding ANY direct or optional dep (+ lock refresh) grows the
/// live closure → this set-equality fails. RP-R2-d teeth: perturb a node
/// `version`/`source`/`enabled_features` in the lock → RED.
#[test]
fn closure_node_manifest_is_set_equal() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let root = graph.workspace_root_id("prro-domain");

    let live = mg::normalize_node_records(&graph.closure_node_records(&root));
    let manifest = mg::load_closure_manifest();

    let missing_from_live: Vec<_> = manifest.nodes.difference(&live).collect();
    let extra_in_live: Vec<_> = live.difference(&manifest.nodes).collect();

    assert!(
        missing_from_live.is_empty() && extra_in_live.is_empty(),
        "R2.2 NODE set-equality vs purity-closure.lock FAILED.\n\
         In lock but NOT live (removed/changed): {missing_from_live:#?}\n\
         Live but NOT in lock (added/changed): {extra_in_live:#?}\n\
         A legitimate version/feature bump MUST turn this RED (correct signal for \
         so small a closure). Regenerate with `cargo xtask update-purity-closure` \
         — CI never auto-updates it.",
    );
}

/// **R2.2 (edges).** The live `--all-features` non-dev EDGE table is set-equal to
/// the committed lock. This is what catches an edge **rewiring** (e.g. a
/// wasm-only edge becoming Linux-active) that a node-only manifest would pass.
///
/// RP-R2-d teeth: perturb an edge `kind`/`target` in the lock → RED.
#[test]
fn closure_edge_manifest_is_set_equal() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let root = graph.workspace_root_id("prro-domain");

    let live = mg::normalize_edge_records(&graph.closure_edge_records(&root));
    let manifest = mg::load_closure_manifest();

    let missing_from_live: Vec<_> = manifest.edges.difference(&live).collect();
    let extra_in_live: Vec<_> = live.difference(&manifest.edges).collect();

    assert!(
        missing_from_live.is_empty() && extra_in_live.is_empty(),
        "R2.2 EDGE set-equality vs purity-closure.lock FAILED (kind/target live \
         on the EDGE, not the node).\n\
         In lock but NOT live: {missing_from_live:#?}\n\
         Live but NOT in lock: {extra_in_live:#?}\n\
         Regenerate with `cargo xtask update-purity-closure` — CI never \
         auto-updates it.",
    );
}

/// **R2.2 (acceptance annotations).** The two accepted capability nodes are
/// annotated with their justification. `getrandom` is REQUIRED (OS entropy for
/// UUID v7); `libc` is required IFF it is present in the live closure (its
/// getrandom syscall ABI). Dropping either annotation is RED (RP-R2-d).
#[test]
fn accepted_capability_nodes_are_annotated() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let root = graph.workspace_root_id("prro-domain");
    let live_names = graph.non_dev_closure_names(&root);
    let manifest = mg::load_closure_manifest();

    // getrandom: present in the closure AND must be annotated.
    assert!(
        live_names.contains("getrandom"),
        "R2.2 sanity: `getrandom` must be in prro-domain's live closure \
         (prro-domain -> uuid -> getrandom); it is the entropy source for UUID v7.",
    );
    assert_eq!(
        manifest.acceptance.get("getrandom").map(String::as_str),
        Some("OS entropy for UUID v7 random bits"),
        "R2.2 acceptance: `getrandom` MUST be annotated \"OS entropy for UUID v7 \
         random bits\" in purity-closure.lock. It is the one capability dependency \
         (the v7 timestamp uses std::time, not a dependency). Do not drop the \
         acceptance annotation.",
    );

    // libc: accepted only where present; if in the closure it MUST be annotated.
    if live_names.contains("libc") {
        assert_eq!(
            manifest.acceptance.get("libc").map(String::as_str),
            Some("getrandom syscall ABI"),
            "R2.2 acceptance: `libc` is in the closure (getrandom's syscall ABI) \
             and MUST be annotated \"getrandom syscall ABI\" in purity-closure.lock.",
        );
    }
}

/// CS-1R2 A5 — the lock's `schema` and `root` are now CHECKED, not merely parsed.
/// Before, `schema` was written by xtask but read nowhere, and `root` was loaded
/// into the manifest struct but never verified against the live root — so a lock
/// minted for the WRONG root, or a shape drift, would go unnoticed.
#[test]
fn closure_manifest_root_and_schema_are_pinned() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let live_root = mg::normalize_package_id(&graph.workspace_root_id("prro-domain"));
    let manifest = mg::load_closure_manifest(); // panics on a schema mismatch (A5)

    assert_eq!(
        manifest.schema,
        mg::CLOSURE_MANIFEST_SCHEMA,
        "purity-closure.lock schema must equal the pinned CLOSURE_MANIFEST_SCHEMA",
    );
    assert_eq!(
        manifest.root, live_root,
        "R2.2 (A5): purity-closure.lock `root` ({}) must equal the live normalized \
         workspace root for `prro-domain` ({live_root}). A mismatch means the lock \
         was minted for the wrong crate/version — regenerate with \
         `cargo xtask update-purity-closure`.",
        manifest.root,
    );
}

// ===========================================================================
// Forbidden-crate smoke (legible message layered on the exhaustive manifest).
// ===========================================================================

/// A fast, legible check: none of the usual I/O / async-runtime / DB crates is
/// in `prro-domain`'s non-dev closure. The closure manifest above is the
/// authoritative gate; this just names the offender clearly.
#[test]
fn prro_domain_reaches_no_forbidden_crate() {
    let meta = mg::run_cargo_metadata();
    let graph = mg::build_graph(&meta);
    let root = graph.workspace_root_id("prro-domain");
    let names = graph.non_dev_closure_names(&root);

    let violations: Vec<&str> = FORBIDDEN
        .iter()
        .copied()
        .filter(|f| names.contains(*f))
        .collect();

    assert!(
        violations.is_empty(),
        "prro-domain must have NO DB/network/async-runtime/RPC/HTTP adapter \
         dependencies, but its non-dev closure reached: {violations:?}.",
    );
}

// ===========================================================================
// RP-R2-c — PackageId traversal via the SHARED production walker (fixtures).
// ===========================================================================

/// A synthetic `cargo metadata` fixture with TWO same-name `sqlx` versions, both
/// REACHABLE from the root but by different edges:
///   - `sqlx 0.1.0` (SAFE, no forbidden sub-deps) is reached FIRST — directly
///     from the root;
///   - `sqlx 9.9.9` (DANGEROUS — it pulls the forbidden `tokio`) is reached via
///     `root -> midcrate -> sqlx@9.9.9`, i.e. LATER in the traversal.
///
/// The traversal order guarantees the name `"sqlx"` is first *seen* on the SAFE
/// node. A NAME-dedup walker burns `"sqlx"` on the safe node and then SKIPS the
/// dangerous `sqlx@9.9.9` node entirely — so it never walks INTO `sqlx@9.9.9`'s
/// own deps and MISSES `tokio`. The production PackageId walker keys on the
/// distinct `PackageId`, so it visits BOTH sqlx nodes and reaches `tokio`.
///
/// This is the exact discriminator the spec demands: swapping the production
/// `visited` from PackageId to name makes `rp_r2_c_packageid_walker_...` RED.
fn two_sqlx_versions_fixture() -> serde_json::Value {
    serde_json::json!({
        "packages": [
            // SAFE version — reached first (direct root edge), no sub-deps.
            {"id":"registry+x#sqlx@0.1.0","name":"sqlx","version":"0.1.0","source":"registry+x","dependencies":[]},
            // DANGEROUS version — reached later, pulls the forbidden `tokio`.
            {"id":"registry+x#sqlx@9.9.9","name":"sqlx","version":"9.9.9","source":"registry+x","dependencies":[]},
            {"id":"registry+x#tokio@1.0.0","name":"tokio","version":"1.0.0","source":"registry+x","dependencies":[]},
            {"id":"registry+x#midcrate@1.0.0","name":"midcrate","version":"1.0.0","source":"registry+x","dependencies":[]},
            {"id":"path+file:///w/root#0.1.0","name":"root","version":"0.1.0","source":null,"dependencies":[]}
        ],
        "workspace_members": ["path+file:///w/root#0.1.0"],
        "resolve": {
            "nodes": [
                {"id":"registry+x#sqlx@0.1.0","features":[],"deps":[]},
                // The DANGEROUS version's OWN dep on the forbidden `tokio` — only
                // reachable if the walker actually enters this node.
                {"id":"registry+x#sqlx@9.9.9","features":[],"deps":[
                    {"pkg":"registry+x#tokio@1.0.0","name":"tokio","dep_kinds":[{"kind":null,"target":null}]}
                ]},
                {"id":"registry+x#tokio@1.0.0","features":[],"deps":[]},
                {"id":"registry+x#midcrate@1.0.0","features":[],"deps":[
                    {"pkg":"registry+x#sqlx@9.9.9","name":"sqlx","dep_kinds":[{"kind":null,"target":null}]}
                ]},
                // Root reaches the SAFE sqlx FIRST (so name is burned early),
                // and the DANGEROUS one only via midcrate.
                {"id":"path+file:///w/root#0.1.0","features":[],"deps":[
                    {"pkg":"registry+x#sqlx@0.1.0","name":"sqlx","dep_kinds":[{"kind":null,"target":null}]},
                    {"pkg":"registry+x#midcrate@1.0.0","name":"midcrate","dep_kinds":[{"kind":null,"target":null}]}
                ]}
            ]
        }
    })
}

/// **RP-R2-c (i).** The production PackageId walker, driven over the fixture via
/// the SHARED module, visits BOTH sqlx nodes and therefore reaches the forbidden
/// `tokio` that hangs off the DANGEROUS `sqlx@9.9.9`. A name-dedup walker would
/// skip `sqlx@9.9.9` (its name already burned by the safe node) and MISS
/// `tokio`, wrongly passing purity.
///
/// Teeth (spec): swap the production `visited` in `metadata_graph::non_dev_closure`
/// from `PackageId` to crate name → `tokio` is no longer reached → this test RED.
#[test]
fn rp_r2_c_packageid_walker_reaches_second_version_sqlx() {
    let meta = two_sqlx_versions_fixture();
    let graph = mg::build_graph(&meta);
    let root = graph.workspace_root_id("root");

    let reached_ids = graph.non_dev_closure(&root);
    assert!(
        reached_ids.contains("registry+x#sqlx@9.9.9"),
        "RP-R2-c: the PackageId walker MUST visit the DANGEROUS second sqlx \
         version (sqlx@9.9.9) even though the SAFE sqlx@0.1.0 was reached first; \
         reached ids = {reached_ids:?}",
    );

    // The load-bearing assertion: a forbidden crate hangs off ONLY the dangerous
    // second version. If the walker keyed on NAME it would skip that node and
    // MISS `tokio`. The PackageId walker reaches it → the fixture is UNSAFE.
    let names = graph.non_dev_closure_names(&root);
    assert!(
        names.contains("tokio"),
        "RP-R2-c: keying on PackageId must reach the forbidden `tokio` that hangs \
         off the DANGEROUS sqlx@9.9.9 (a name-dedup walker would skip it and \
         wrongly pass); names = {names:?}",
    );
}

/// **RP-R2-c (ii) — teeth demonstrated IN-TEST.** A deliberately BUGGY
/// name-dedup walker (models the mutation "swap production `visited` from
/// PackageId to name"), run over the SAME fixture. Because the SAFE `sqlx@0.1.0`
/// is reached FIRST, a name-keyed `visited` burns `"sqlx"` there and never
/// enters `sqlx@9.9.9` — so the forbidden `tokio` hanging off it is MISSED and
/// purity would WRONGLY pass. This proves the PackageId walker above is
/// load-bearing (the production non_dev_closure keys on PackageId, so its `tokio`
/// finding is not reproducible by name-keying).
#[test]
fn rp_r2_c_name_dedup_walker_is_unsound_teeth() {
    let meta = two_sqlx_versions_fixture();

    let pkgs: std::collections::HashMap<String, String> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_string(),
                p["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let nodes: std::collections::HashMap<String, &serde_json::Value> = meta["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap().to_string(), n))
        .collect();
    let root = "path+file:///w/root#0.1.0".to_string();

    // Buggy walker: dedup traversal on crate NAME (not PackageId). The safe
    // sqlx@0.1.0 is reached first via the direct root edge, burning "sqlx"; the
    // BFS then never enters sqlx@9.9.9, so it never sees tokio.
    let mut reached_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut visited_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root.clone());
    while let Some(id) = queue.pop_front() {
        if let Some(n) = nodes.get(&id) {
            for dep in n["deps"].as_array().unwrap() {
                let to = dep["pkg"].as_str().unwrap().to_string();
                let name = pkgs.get(&to).cloned().unwrap_or_default();
                // Dedup on NAME — the unsound mutation.
                if visited_names.insert(name.clone()) {
                    reached_names.insert(name);
                    queue.push_back(to);
                }
            }
        }
    }

    assert!(
        reached_names.contains("sqlx") && !reached_names.contains("tokio"),
        "RP-R2-c teeth: a NAME-dedup walker must reach `sqlx` (the safe first \
         version) but MISS `tokio` (hanging off the skipped dangerous second \
         version) — proving name-keying is unsound. reached = {reached_names:?}",
    );
}

/// A SEPARATE fixture (RP-R2-c, root-by-workspace-id half): a FOREIGN registry
/// package shares the root's name `prro-domain`. Root resolution by
/// `workspace_members` id must pick the workspace path package, not the foreign
/// one — else a foreign same-named package could masquerade as the root.
fn foreign_same_name_root_fixture() -> serde_json::Value {
    serde_json::json!({
        "packages": [
            // FOREIGN same-name package from a registry, with a FORBIDDEN dep.
            {"id":"registry+x#prro-domain@0.0.1","name":"prro-domain","version":"0.0.1","source":"registry+x","dependencies":[]},
            {"id":"registry+x#sqlx@1.0.0","name":"sqlx","version":"1.0.0","source":"registry+x","dependencies":[]},
            // The REAL workspace root — pure.
            {"id":"path+file:///w/prro-domain#0.1.0","name":"prro-domain","version":"0.1.0","source":null,"dependencies":[]}
        ],
        "workspace_members": ["path+file:///w/prro-domain#0.1.0"],
        "resolve": {
            "nodes": [
                {"id":"registry+x#prro-domain@0.0.1","features":[],"deps":[
                    {"pkg":"registry+x#sqlx@1.0.0","name":"sqlx","dep_kinds":[{"kind":null,"target":null}]}
                ]},
                {"id":"registry+x#sqlx@1.0.0","features":[],"deps":[]},
                {"id":"path+file:///w/prro-domain#0.1.0","features":[],"deps":[]}
            ]
        }
    })
}

/// **RP-R2-c (root-by-workspace-id).** With a foreign same-named package present,
/// root resolution by `workspace_members` id selects the PURE workspace root, so
/// the foreign package's `sqlx` edge is NOT in the closure. (RED if root is
/// chosen by name and happens to pick the foreign one.)
#[test]
fn rp_r2_c_root_resolved_by_workspace_id_not_name() {
    let meta = foreign_same_name_root_fixture();
    let graph = mg::build_graph(&meta);

    let root = graph.workspace_root_id("prro-domain");
    assert_eq!(
        root, "path+file:///w/prro-domain#0.1.0",
        "RP-R2-c: root MUST be resolved by workspace_members id (the path \
         package), never by name (which could pick the foreign registry package)",
    );

    let names = graph.non_dev_closure_names(&root);
    assert!(
        !names.contains("sqlx"),
        "RP-R2-c: the REAL workspace root is pure — the foreign same-named \
         package's sqlx edge must NOT leak into the closure; names = {names:?}",
    );
}
