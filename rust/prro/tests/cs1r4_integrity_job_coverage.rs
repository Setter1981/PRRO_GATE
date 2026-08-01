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

// ─── CS-1R4 addendum (2026-07-31): the docs-only fast path ──────────────────
//
// The `scope` step lets the required leg skip its expensive work when EVERY
// changed path is inert. That is safe only because the job still RUNS and still
// REPORTS the required context — the #305 bypass was a job that never reported,
// which branch protection counts as satisfied.
//
// These assertions pin the properties that keep the two apart. They are the
// teeth: turning the fast path into a bypass now fails a test instead of
// shipping. Like the rest of this file they are CODEOWNERS-gated.

/// Raw text of the required workflow.
fn workflow_text() -> String {
    let path = repo_root().join(".github/workflows/rust-prro.yml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read workflow {}: {e}", path.display()))
}

/// The `scope` step must default to HEAVY and clear it on exactly ONE path.
///
/// `heavy=true` is the initialiser; `heavy=false` may appear exactly once — the
/// single branch where every changed path matched the inert allowlist. A second
/// `heavy=false`, or a `heavy=true` initialiser that disappears, means some
/// path now skips the integrity legs by omission rather than by decision.
#[test]
fn scope_step_is_fail_closed() {
    let text = workflow_text();
    assert!(
        text.contains("id: scope"),
        "CS-1R4 fast path: the `scope` step is gone, but steps still reference \
         `steps.scope.outputs.heavy` — or the fast path was removed without \
         removing these teeth."
    );
    assert!(
        text.contains("heavy=true\n"),
        "CS-1R4 fast path: the `scope` step no longer INITIALISES `heavy=true`. \
         Fail-closed depends on that default: every error path (no base sha, base \
         absent, git diff failure, empty diff) leaves it untouched."
    );
    // Count in EXECUTABLE lines only — never in comments. Same discipline as
    // `workflow_cargo_lines` above: a comment cannot satisfy a property, and it
    // must not be able to break one either (the prose above this step discusses
    // `heavy=false` by name).
    let clears = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter(|l| l.contains("heavy=false"))
        .count();
    assert_eq!(
        clears, 1,
        "CS-1R4 fast path: expected EXACTLY ONE `heavy=false` (the sole inert-allowlist \
         branch), found {clears}. More than one means another path can skip the \
         integrity legs; zero means the fast path is dead code."
    );
    assert!(
        text.contains("--name-only --no-renames"),
        "CS-1R4 fast path: the change set must be computed with `--no-renames` so a \
         rename is split into BOTH endpoints. Following renames would let a file moved \
         OUT of `rust/` look like a docs-only change."
    );
}

/// The inert allowlist must never admit a path that can influence a build or a
/// gate. `docs/cs1r/**` is the subtle one: it *is* under `docs/`, but the CS-1
/// inventory manifests live there, so it must be classified NON-inert.
#[test]
fn scope_allowlist_excludes_everything_that_can_move_a_gate() {
    let text = workflow_text();
    let scope = text
        .split("id: scope")
        .nth(1)
        .expect("scope step present")
        .split("\n      - name:")
        .next()
        .expect("scope step body");

    assert!(
        scope.contains("docs/cs1r/*) inert=false"),
        "CS-1R4 fast path: `docs/cs1r/**` must be NON-inert — the CS-1 inventory \
         manifests live there, so a change under it can move the gate even though \
         the path looks like documentation."
    );
    // A `*)` catch-all setting inert=false is what makes the list an ALLOWLIST
    // rather than a denylist: anything unrecognised is heavy.
    assert!(
        scope.contains("*)           inert=false") || scope.contains("*) inert=false"),
        "CS-1R4 fast path: the `case` lost its catch-all `*) inert=false` arm. Without \
         it the classifier becomes a DENYLIST and any path nobody thought of would be \
         treated as inert — the #305 failure mode in a new costume."
    );
    for forbidden in ["rust/", ".github/", "scripts/", "Cargo", ".sqlx"] {
        assert!(
            !scope.contains(&format!("{forbidden}*) ;;"))
                && !scope.contains(&format!("{forbidden}*)      ;;")),
            "CS-1R4 fast path: `{forbidden}` was added to the inert allowlist. That path \
             influences the build or a gate; a change under it MUST run the integrity legs."
        );
    }
}

/// Every cargo leg must be gated on the scope decision — and the job itself must
/// stay unconditional, because a skipped JOB is the bypass CS-1R4 removed.
#[test]
fn cargo_legs_are_scope_gated_and_the_job_stays_unconditional() {
    let text = workflow_text();
    let job = text
        .split("\n  build:")
        .nth(1)
        .expect("the `build` job is present");
    let header = job.split("steps:").next().expect("job header");
    assert!(
        !header.contains("\n    if:") && !header.contains("\n    needs:"),
        "CS-1R4: the required job grew an `if:`/`needs:` gate. A SKIPPED required check \
         counts as SATISFIED by branch protection — that is the #305 bypass. The job must \
         always run; only its inner steps may be conditional.\n  header = {header:?}"
    );
    let gated = text.matches("steps.scope.outputs.heavy == 'true'").count();
    let cargo_legs = workflow_cargo_lines().len();
    assert!(
        gated >= cargo_legs,
        "CS-1R4 fast path: {cargo_legs} cargo legs but only {gated} scope gates. A cargo \
         leg that runs unconditionally is not a safety problem, but the mismatch means the \
         two lists drifted — re-check which legs are gated and why."
    );
}

/// Execute the classifier THE WORKFLOW ACTUALLY RUNS against a table of paths.
///
/// The `case` block is extracted verbatim from the YAML and executed by `bash`,
/// so this tests the shipped text rather than a Rust re-implementation of it —
/// a re-implementation could agree with itself while the workflow drifted.
///
/// The table is the point. `docs/cs1r/**` and a `.md` OUTSIDE the repo root are
/// the two cases a reader is most likely to get wrong, and both must be HEAVY.
#[test]
fn scope_classifier_decides_correctly_on_a_table_of_paths() {
    let text = workflow_text();
    let case_block = {
        let start = text
            .find("            inert=true")
            .expect("classifier `inert=true` preamble present");
        let end = text[start..]
            .find("            done <<< \"$files\"")
            .expect("classifier loop terminator present");
        &text[start..start + end]
    };

    // `docs/a.md` etc. are newline-joined into `$files`, exactly as in the job.
    let cases: &[(&str, bool)] = &[
        ("docs/a.md\ndocs/b/c.md\nREADME.md", true),
        (".beads/issues.json", true),
        ("docs/cs1r/inventory/manifest.test-support.tsv", false),
        ("docs/a.md\nrust/prro/src/lib.rs", false),
        (".github/workflows/rust-prro.yml", false),
        ("scripts/cs1r/mint_manifests.sh", false),
        ("rust/Cargo.toml", false),
        ("rust/prro/.sqlx/query-abc.json", false),
        ("rust/notes.md", false),
        ("docs/a.md\n.github/CODEOWNERS", false),
    ];

    for (files, expect_inert) in cases {
        let script = format!(
            "set -u\nfiles='{files}'\n{case_block}\n            done <<< \"$files\"\n\
             if $inert; then echo INERT; else echo HEAVY; fi\n"
        );
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(&script)
            .output()
            .expect("run the extracted classifier under bash");
        let got = String::from_utf8_lossy(&out.stdout);
        let got = got.trim();
        let want = if *expect_inert { "INERT" } else { "HEAVY" };
        assert_eq!(
            got,
            want,
            "CS-1R4 fast path: the workflow's own classifier misclassified.\n  \
             files:\n{files}\n  expected {want}, got {got:?}\n  stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// ═══════════ bd PRRO_GATE-r4c — the SECOND scope detector ═══════════
//
// Everything above pins `rust-prro.yml`. `mutation-diff.yml` has an equivalent
// detector deciding whether to run the required `mutation diff-gate` leg, and it
// had NO pin — so the two silently drifted into opposite postures. `rust-prro.yml`
// refuses to treat an empty diff as inert; `mutation-diff.yml` simply ran
// `git fetch` + `git diff` and took ANY non-matching result as out-of-path, so a
// missing base sha, a failed fetch or an empty diff SKIPPED the heavy leg — and a
// skipped required check reads as satisfied.
//
// That asymmetry is exactly how the mutation gate came to be trusted while doing
// nothing (the gate script's own three defects are pinned by
// `scripts/mutation/run_teeth.sh`). One pinned detector and one unpinned one is
// not a policy; these teeth make the second one a decision too.

fn mutation_workflow_text() -> String {
    let path = repo_root().join(".github/workflows/mutation-diff.yml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read workflow {}: {e}", path.display()))
}

/// The mutation-gate scope detector must DEFAULT to running the heavy leg and
/// lower that on exactly ONE path — the same property `scope_step_is_fail_closed`
/// pins for `rust-prro.yml`.
#[test]
fn mutation_diff_scope_detector_is_fail_closed() {
    let text = mutation_workflow_text();
    assert!(
        text.contains("inpath=true\n"),
        "r4c: the mutation-gate detector no longer INITIALISES `inpath=true`. \
         Fail-closed depends on that default: every path that cannot establish \
         scope (no base sha, fetch failure, git diff failure, empty diff) must \
         leave the heavy leg ON."
    );
    // Executable lines only — the prose above the step names `inpath=false`.
    let clears = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter(|l| l.contains("inpath=false"))
        .count();
    assert_eq!(
        clears, 1,
        "r4c: expected EXACTLY ONE `inpath=false` (the sole 'non-empty diff matched \
         nothing' branch), found {clears}. More than one means another path can skip \
         the required mutation leg; zero means the fast path is dead code."
    );
    assert!(
        text.contains("empty diff — refusing to treat as out-of-path"),
        "r4c: the empty-diff branch lost its refusal. An empty diff means scope could \
         not be established, NOT that the PR is out of path — treating the two as the \
         same is what let the heavy leg skip silently."
    );
    assert!(
        text.contains("--name-only --no-renames"),
        "r4c: the change set must be computed with `--no-renames`, matching \
         rust-prro.yml, so a rename is split into BOTH endpoints."
    );
}

/// The gate's own teeth must run UNCONDITIONALLY.
///
/// `scripts/mutation/run.sh` was vacuous and fail-open for three weeks with no
/// check noticing. The teeth are seconds and need no cargo, so there is no reason
/// to gate them — and gating them on `changes` would silence them on exactly the
/// PRs where nobody is looking at the mutation gate.
#[test]
fn mutation_gate_teeth_job_runs_unconditionally() {
    let text = mutation_workflow_text();
    let job = text
        .split("\n  gate-teeth:")
        .nth(1)
        .expect("the `gate-teeth` job is present in mutation-diff.yml");
    let header = job.split("steps:").next().expect("job header");
    assert!(
        !header.contains("\n    if:") && !header.contains("\n    needs:"),
        "r4c: the gate-teeth job grew an `if:`/`needs:` gate. It must always run — \
         proving the gate can still FAIL is the cheap half; discovering that it \
         cannot was the expensive half.\n  header = {header:?}"
    );
    assert!(
        job.contains("scripts/mutation/run_teeth.sh"),
        "r4c: the gate-teeth job no longer invokes scripts/mutation/run_teeth.sh."
    );
}
