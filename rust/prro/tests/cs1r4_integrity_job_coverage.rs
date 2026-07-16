//! CS-1R4 item 2 — INTEGRITY-JOB COVERAGE assertion.
//!
//! The required `x86_64-unknown-linux-gnu` context in `.github/workflows/rust-prro.yml`
//! carries the CS-1 integrity signal by running `cargo build` + `cargo nextest run`
//! for the workspace members (with the required features). CS-1R4 removed the
//! path-detector dispatch, so this job runs UNCONDITIONALLY — but that only helps if
//! the job actually COVERS every member. This test makes a coverage SHRINK visible:
//! it fails if a workspace member loses its build leg or its test leg in the
//! workflow, or if the required `test-support` feature is dropped from the `prro`
//! test leg.
//!
//! This is a single non-matrix assertion (no aggregator job): it reads the live
//! `cargo metadata` member set and the workflow YAML `run:` lines, and cross-checks
//! them. It runs inside the very job it audits (the `prro` test leg), and — with the
//! workflow — is CODEOWNERS-gated (`.github/CODEOWNERS`), so weakening either the
//! workflow coverage OR this assertion is an owner-reviewed diff.
//!
//! ## Why parse the `run:` lines (not the comments)
//! We extract crate names ONLY from `cargo build …` / `cargo nextest run …` command
//! lines (the executable part), never from the human comments — a comment cannot
//! satisfy coverage, and a narrowed real line with a wide comment above it (the
//! round-3 comment-injection class) cannot fool this.
//!
//! ## Scope
//! `xtask` is the build-tooling crate (invoked as `cargo xtask …`, never
//! built/tested as a member on the required leg); it is excluded from the coverage
//! requirement by name, with an explicit assertion that it is the ONLY exclusion.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

/// Repo root = two levels above this crate's manifest dir (`<root>/rust/prro`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root two levels above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

/// The build-tooling crate excluded from the required build/test coverage (it is
/// run as `cargo xtask`, never built/tested as a member on the required leg).
const TOOLING_EXCLUSIONS: &[&str] = &["xtask"];

/// Live workspace member crate names from `cargo metadata --no-deps`.
fn workspace_members() -> BTreeSet<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(manifest_dir)
        .env("SQLX_OFFLINE", "true")
        .output()
        .expect("failed to invoke `cargo metadata`");
    assert!(
        out.status.success(),
        "`cargo metadata` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata not valid JSON");
    v["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .map(|p| p["name"].as_str().expect("package name").to_string())
        .collect()
}

/// The `run:` command text of the required workflow, one entry per non-comment,
/// non-blank line of a `cargo …` invocation (we keep only cargo lines).
fn workflow_cargo_lines() -> Vec<String> {
    let path = repo_root().join(".github/workflows/rust-prro.yml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read workflow {}: {e}", path.display()));
    text.lines()
        .map(|l| l.trim())
        // executable command lines only; a YAML comment starts with `#`.
        .filter(|l| !l.starts_with('#'))
        .filter(|l| l.contains("cargo build") || l.contains("cargo nextest run"))
        .map(str::to_string)
        .collect()
}

/// Extract the `-p <crate>` args from a single cargo command line.
fn crates_in_line(line: &str) -> Vec<String> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i] == "-p" && i + 1 < toks.len() {
            out.push(toks[i + 1].to_string());
        }
        i += 1;
    }
    out
}

/// The members named in a BUILD leg (`cargo build …`).
fn built_members(lines: &[String]) -> BTreeSet<String> {
    lines
        .iter()
        .filter(|l| l.contains("cargo build"))
        .flat_map(|l| crates_in_line(l))
        .collect()
}

/// The members named in a TEST leg (`cargo nextest run …`).
fn tested_members(lines: &[String]) -> BTreeSet<String> {
    lines
        .iter()
        .filter(|l| l.contains("cargo nextest run"))
        .flat_map(|l| crates_in_line(l))
        .collect()
}

/// The members that need a required build/test leg (all live members MINUS the
/// tooling exclusions). Asserts the exclusions are actually present as members so
/// a stale exclusion name is caught.
fn required_members() -> BTreeSet<String> {
    let members = workspace_members();
    for excl in TOOLING_EXCLUSIONS {
        assert!(
            members.contains(*excl),
            "TOOLING_EXCLUSIONS names `{excl}` which is not a live workspace member — \
             stale exclusion; update this test."
        );
    }
    members
        .into_iter()
        .filter(|m| !TOOLING_EXCLUSIONS.contains(&m.as_str()))
        .collect()
}

/// **CS-1R4 item 2.** Every non-tooling workspace member is COMPILED on the
/// required leg — either by an explicit `cargo build -p X` leg OR by a `cargo
/// nextest run -p X` leg (nextest compiles the crate + its test targets before
/// running). Removing BOTH → a crate-local COMPILE break would be SKIPPED → RED.
#[test]
fn every_member_is_compiled() {
    let lines = workflow_cargo_lines();
    let built = built_members(&lines);
    let tested = tested_members(&lines);
    let compiled: BTreeSet<String> = built.union(&tested).cloned().collect();
    let required = required_members();

    let missing: Vec<&String> = required.iter().filter(|m| !compiled.contains(*m)).collect();
    assert!(
        missing.is_empty(),
        "CS-1R4 integrity-job coverage: these workspace members have NEITHER a `cargo \
         build` NOR a `cargo nextest run` leg in .github/workflows/rust-prro.yml — a \
         crate-local COMPILE break in them would be SKIPPED on the required context: \
         {missing:?}\n  compiled(build∪test) = {compiled:?}\n  required = {required:?}",
    );
}

/// **CS-1R4 item 2.** Every non-tooling workspace member has a TEST leg in the
/// required workflow job. Removing a member's test coverage → RED.
#[test]
fn every_member_has_a_test_leg() {
    let lines = workflow_cargo_lines();
    let tested = tested_members(&lines);
    let required = required_members();

    let missing: Vec<&String> = required.iter().filter(|m| !tested.contains(*m)).collect();
    assert!(
        missing.is_empty(),
        "CS-1R4 integrity-job coverage: these workspace members have NO `cargo nextest \
         run` leg in .github/workflows/rust-prro.yml — a crate-local test regression \
         would be SKIPPED on the required context: {missing:?}\n  tested = {tested:?}\n  required = {required:?}",
    );
}

/// **CS-1R4 item 2.** The required `test-support` feature is present on the `prro`
/// TEST leg. Dropping the feature would silently disable the integration tests that
/// depend on the W2 ReconcileGuard test seam. Removing it → RED.
#[test]
fn prro_test_leg_carries_test_support_feature() {
    let lines = workflow_cargo_lines();
    // Find the `prro` test leg (a `cargo nextest run` line that names `-p prro`
    // exactly, not `-p prro-domain` etc.).
    let prro_test_leg = lines
        .iter()
        .find(|l| l.contains("cargo nextest run") && crates_in_line(l).iter().any(|c| c == "prro"));
    let leg = prro_test_leg.expect(
        "CS-1R4 integrity-job coverage: no `cargo nextest run -p prro …` TEST leg found \
         in .github/workflows/rust-prro.yml — the primary test signal is gone.",
    );
    assert!(
        leg.contains("--features test-support"),
        "CS-1R4 integrity-job coverage: the `prro` test leg lost `--features \
         test-support` — the integration tests that use the W2 ReconcileGuard test \
         seam would be silently disabled.\n  leg = {leg:?}",
    );
}
