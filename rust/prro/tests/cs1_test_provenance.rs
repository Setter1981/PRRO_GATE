//! CS-1R R1.1 — per-hunk provenance audit (syn-based) — the RED-pin RP-R1-2.
//!
//! Spec `docs/superpowers/specs/2026-07-15-cs1r-remediation-spec.md` §4 R1.1.
//!
//! CS-1 (`f2c17b1..f2628ba`) applied a MECHANICAL `Db*`-wrapper refactor to 79
//! test files. This test PROVES the refactor was behaviour-neutral at the AST +
//! sqlx-query level: it parses both endpoints via `syn`, undoes ONLY the
//! whitelisted transforms, and asserts token-equality; and it extracts a
//! signature for every sqlx chain (SQL bytes, ORDERED bind vector, fetch mode)
//! and asserts they are unchanged.
//!
//! ## Two legs
//! 1. **Immutable provenance leg** — `f2c17b1` blob vs `f2628ba` blob. This is
//!    the frozen historical proof; it writes the classification artifact record.
//! 2. **Live-drift / teeth leg** — `f2c17b1` blob vs the WORKING-TREE file (which
//!    at rest == `f2628ba` for 78 files; `invariant_fuzzer/model.rs` carries a
//!    post-CS-1 oracle-fix and is compared against its `f2628ba` blob instead).
//!    A teeth mutation to a live file makes THIS leg RED (RP-R1-2).
//!
//! ## Teeth (spec §4 R1.1 — each → RED)
//! swap two same-type `.bind` · change a SQL literal · change a fixture value ·
//! change a control-flow condition · change an assertion RHS. The first is caught
//! by the sqlx bind-vector; the rest by the AST token-equality (they are neither
//! a `Db*` wrapper nor `.as_str()` nor an import, so the canonicalizer leaves
//! them and the compare diverges).

#[path = "support/cs1_provenance.rs"]
mod prov;

use std::collections::BTreeSet;

use prov::{
    extract_sqlx_sigs, git_hash_object, git_show, manual_residual_fingerprint, manual_ruling_files,
    normalize_source, repo_root, BASE_SHA, HEAD_SHA,
};

/// CS-1R2 A2a — the pinned EXACT residual fingerprint of the sole manual-ruling
/// file (`live_dps_extended_smoke.rs`). Its post-canonicalization residual is the
/// documented T7 use-site `.0` (base→head). We pin the fingerprint of the
/// (base-normalized, head-normalized) token pair so that ANY *other* residual —
/// an assertion RHS flip, a control-flow edit, an added statement — perturbs the
/// head token stream and turns the gate RED, instead of the old blanket
/// "any residual OK for a manual file" waiver. Re-mint (via the ignored helper
/// `print_manual_residual_fingerprint` below) ONLY when the documented T7
/// residual itself legitimately changes, with an artifact update.
const MANUAL_RESIDUAL_FINGERPRINT: &str = "4b6a3825f97d65880aaa8fd93fbd7d17";

/// The 79 CS-1 modified test files (repo-relative). Frozen list: the provenance
/// set is exactly the files `git diff --name-status f2c17b1 f2628ba` reports as
/// `M` under `rust/prro/tests`. Pinned here so ADDING a 80th modified file to a
/// future "behaviour-neutral" PR without a provenance entry is impossible to do
/// silently (the count assertion below bites).
const CS1_MODIFIED_FILES: &[&str] = &[
    "rust/prro/tests/a3_final_binding_flip.rs",
    "rust/prro/tests/app_boot_reconciliation.rs",
    "rust/prro/tests/app_drain_offline_backlog.rs",
    "rust/prro/tests/aprime1_piece2_send_confirm_edges.rs",
    "rust/prro/tests/aprime1_piece3_acquire_shift_edges.rs",
    "rust/prro/tests/b10_offline_session_handshake.rs",
    "rust/prro/tests/b8_acquire_real_first.rs",
    "rust/prro/tests/b8_stamp_offline_dps_code.rs",
    "rust/prro/tests/b9_stamp_at_sign.rs",
    "rust/prro/tests/backlog_drain_finalize.rs",
    "rust/prro/tests/backlog_drain_per_doc_loop.rs",
    "rust/prro/tests/backlog_drain_prerequisites.rs",
    "rust/prro/tests/backlog_drain_state_dispatch.rs",
    "rust/prro/tests/backup_restore.rs",
    "rust/prro/tests/boot_phase_w9_helpers.rs",
    "rust/prro/tests/common/mod.rs",
    "rust/prro/tests/document_files_replace.rs",
    "rust/prro/tests/epz.rs",
    "rust/prro/tests/fiscal_documents_send_helpers.rs",
    "rust/prro/tests/inbox_reaper.rs",
    "rust/prro/tests/invariant_fuzzer/interp.rs",
    "rust/prro/tests/invariant_fuzzer/model.rs",
    "rust/prro/tests/invariant_scan.rs",
    "rust/prro/tests/kill_point_matrix.rs",
    "rust/prro/tests/l0_l1_cash_ledger.rs",
    "rust/prro/tests/l3_service_io.rs",
    "rust/prro/tests/l5_input_guards.rs",
    "rust/prro/tests/l6_xreport.rs",
    "rust/prro/tests/live_dps_extended_smoke.rs",
    "rust/prro/tests/mac_recovery_orchestrator.rs",
    "rust/prro/tests/migration_010_transport_trace.rs",
    "rust/prro/tests/migration_011_outbox.rs",
    "rust/prro/tests/migration_013_mac_recovery.rs",
    "rust/prro/tests/migrations_007_008.rs",
    "rust/prro/tests/models_smoke.rs",
    "rust/prro/tests/node_state_mode_setters.rs",
    "rust/prro/tests/offline_codes_dps_code.rs",
    "rust/prro/tests/offline_door_coupling.rs",
    "rust/prro/tests/offline_lifecycle_orphan_recovery.rs",
    "rust/prro/tests/offline_session_code_pool.rs",
    "rust/prro/tests/offline_session_state_machine.rs",
    "rust/prro/tests/online_convergence_tick.rs",
    "rust/prro/tests/p1_boot_resume_signed_refused_repro.rs",
    "rust/prro/tests/pilot_offline_full_drill_e2e.rs",
    "rust/prro/tests/pilot_offline_half_e2e.rs",
    "rust/prro/tests/pilot_offline_shift_lifecycle_e2e.rs",
    "rust/prro/tests/pilot_online_half_e2e.rs",
    "rust/prro/tests/pin_signing_inputs_coalesce.rs",
    "rust/prro/tests/repo_fiscal_documents_state_cas.rs",
    "rust/prro/tests/repo_shifts.rs",
    "rust/prro/tests/return_online_probe.rs",
    "rust/prro/tests/rs2_convert_payments.rs",
    "rust/prro/tests/rs2_replay_matrix.rs",
    "rust/prro/tests/shift_create_primitive.rs",
    "rust/prro/tests/shift_life_matrix.rs",
    "rust/prro/tests/shift_state_whitelist_matrix.rs",
    "rust/prro/tests/shift_transition_service.rs",
    "rust/prro/tests/shifts_force_seam_source_guard.rs",
    "rust/prro/tests/shifts_no_silent_error_paths.rs",
    "rust/prro/tests/shifts_senior_close.rs",
    "rust/prro/tests/stage_finalize_idempotency.rs",
    "rust/prro/tests/stage_offline_ack.rs",
    "rust/prro/tests/stage_send_offline_doc_routed_online.rs",
    "rust/prro/tests/stage_send_signer_refused.rs",
    "rust/prro/tests/t2_offline_close_reserve.rs",
    "rust/prro/tests/t3_auto_z_ticker.rs",
    "rust/prro/tests/t3_time_budgets.rs",
    "rust/prro/tests/transition_state_atomicity.rs",
    "rust/prro/tests/webcheck_replay.rs",
    "rust/prro/tests/write_path_deterministic_replay.rs",
    "rust/prro/tests/write_path_dispatcher_post_sign.rs",
    "rust/prro/tests/write_path_dps_error_routing.rs",
    "rust/prro/tests/write_path_inline.rs",
    "rust/prro/tests/write_path_stage1_acquire.rs",
    "rust/prro/tests/write_path_stage3_sign.rs",
    "rust/prro/tests/write_path_stage4_send.rs",
    "rust/prro/tests/write_path_stage5_finalize.rs",
    "rust/prro/tests/x1_stuck_doc_guard.rs",
    "rust/prro/tests/z_quiescence.rs",
];

/// CS-1R2 A2b — files whose WORKING-TREE content legitimately differs from
/// `f2628ba` due to a specific, APPROVED post-CS-1 change (the fuzzer oracle fix
/// in `32166cc`). Each carries the EXACT approved blob SHA. The old gate blanket-
/// excluded such a file from the live-drift leg (compared base↔`f2628ba` and
/// ignored the worktree entirely), so ANY further edit to `model.rs` was
/// invisible to this teeth leg. Now the live-drift leg first asserts the worktree
/// blob == the approved SHA (any FURTHER drift → RED) and only then compares
/// base↔approved-blob for the AST/sqlx check. A future legitimate change to this
/// file must re-pin the SHA here with an artifact note.
///
/// `(repo-relative path, approved git blob SHA)`. The approved blob is
/// `git rev-parse 32166cc:<path>` (the oracle-fix commit).
const POST_CS1_CARVEOUT: &[(&str, &str)] = &[(
    "rust/prro/tests/invariant_fuzzer/model.rs",
    "c19654a4f1115cd500cef6bf67372a48ef7d197f",
)];

#[test]
fn cs1_provenance_set_is_exactly_79() {
    assert_eq!(
        CS1_MODIFIED_FILES.len(),
        79,
        "the CS-1 provenance set is the 79 modified test files under \
         f2c17b1..f2628ba; if this count changed, re-mint the set + artifact"
    );
    // No dup.
    let set: BTreeSet<&&str> = CS1_MODIFIED_FILES.iter().collect();
    assert_eq!(set.len(), 79, "duplicate entry in CS1_MODIFIED_FILES");
}

/// LEG 1 — immutable provenance: base blob vs head blob. AST-equality (outside
/// whitelist) + sqlx signature equality for every one of the 79 files.
#[test]
fn cs1_immutable_provenance_base_vs_head() {
    let root = repo_root();
    let manual = manual_ruling_files();
    let mut ast_ok = 0usize;
    let mut ast_manual = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in CS1_MODIFIED_FILES {
        let base_src = git_show(&root, BASE_SHA, path);
        let head_src = git_show(&root, HEAD_SHA, path);
        check_one(
            path,
            &base_src,
            &head_src,
            &manual,
            &mut ast_ok,
            &mut ast_manual,
            &mut failures,
        );
    }

    assert!(
        failures.is_empty(),
        "immutable provenance (base→head) failures:\n{}",
        failures.join("\n\n")
    );
    // 78 pure-whitelist + 1 manual-ruling (live_dps_extended_smoke.rs).
    assert_eq!(ast_ok, 78, "expected 78 pure-whitelist files");
    assert_eq!(ast_manual, 1, "expected exactly 1 manual-ruling file");
}

/// LEG 2 — live-drift / teeth: base blob vs WORKING-TREE file. A mutation to a
/// live file (any of the 5 R1.1 teeth) makes this RED.
#[test]
fn cs1_live_drift_base_vs_worktree() {
    let root = repo_root();
    let manual = manual_ruling_files();
    let approved: std::collections::BTreeMap<&str, &str> =
        POST_CS1_CARVEOUT.iter().copied().collect();
    let mut ast_ok = 0usize;
    let mut ast_manual = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in CS1_MODIFIED_FILES {
        let base_src = git_show(&root, BASE_SHA, path);
        let head_src = if let Some(&approved_sha) = approved.get(path) {
            // CS-1R2 A2b — a carved-out file carries an APPROVED post-CS-1 delta
            // (the oracle fix 32166cc) that is NOT a behaviour-neutral CS-1
            // transform, so the CS-1 provenance AST/sqlx compare must stay on the
            // frozen CS-1 endpoint (base ↔ f2628ba), NOT the oracle-fix blob. The
            // hardening this replaces the OLD blanket "skip the worktree entirely"
            // with: the worktree MUST equal the pinned approved blob SHA — any
            // FURTHER drift (a mutation smuggled into this file) is RED. A future
            // legitimate change re-pins POST_CS1_CARVEOUT with an artifact note.
            let abs = root.join(path);
            let live_sha = git_hash_object(&root, &abs);
            if live_sha != approved_sha {
                failures.push(format!(
                    "{path}: worktree drifted from the APPROVED post-CS-1 blob \
                     (oracle fix 32166cc). Any change to this carved-out file must \
                     re-pin POST_CS1_CARVEOUT with an artifact note.\n  \
                     approved sha={approved_sha}\n  worktree sha={live_sha}"
                ));
            }
            // AST/sqlx provenance compare stays on the frozen CS-1 endpoint.
            git_show(&root, HEAD_SHA, path)
        } else {
            let abs = root.join(path);
            std::fs::read_to_string(&abs)
                .unwrap_or_else(|e| panic!("read worktree file {}: {e}", abs.display()))
        };
        check_one(
            path,
            &base_src,
            &head_src,
            &manual,
            &mut ast_ok,
            &mut ast_manual,
            &mut failures,
        );

        // CS-1R2 A2c — decode-type teeth: pin the live (worktree, or frozen CS-1
        // blob for a carveout file) EXPLICIT decode types against the FROZEN head
        // blob. Both are post-CS-1 and explicit, so a mutation of a decode type to
        // a DIFFERENT type (`::<_, DbShiftState>` → `::<_, DbDocState>`) diverges
        // — the old gate blanket-stripped the whole turbofish and pinned it
        // NOWHERE.
        let head_blob_src = git_show(&root, HEAD_SHA, path);
        pin_worktree_decode_types(path, &head_blob_src, &head_src, &mut failures);
    }

    assert!(
        failures.is_empty(),
        "live-drift (base→worktree) failures — a CS-1 test file diverged from its \
         provenance-equivalent base beyond the whitelisted transforms:\n{}",
        failures.join("\n\n")
    );
    assert_eq!(ast_ok, 78);
    assert_eq!(ast_manual, 1);
}

/// CS-1R2 A2a — MINT helper: prints the current residual fingerprint of every
/// manual-ruling file so `MANUAL_RESIDUAL_FINGERPRINT` can be (re)pinned by a
/// human when the documented T7 residual legitimately changes. Ignored (never
/// runs in CI); run with `cargo test -p prro --features test-support \
/// --test cs1_test_provenance -- --ignored --nocapture print_manual_residual`.
#[test]
#[ignore = "mint helper — prints the pinned residual fingerprint"]
fn print_manual_residual_fingerprint() {
    let root = repo_root();
    for path in &manual_ruling_files() {
        let base_src = git_show(&root, BASE_SHA, path);
        let head_src = git_show(&root, HEAD_SHA, path);
        let base_norm = normalize_source(&base_src).expect("base norm").to_string();
        let head_norm = normalize_source(&head_src).expect("head norm").to_string();
        let fp = manual_residual_fingerprint(&base_norm, &head_norm);
        println!("MANUAL_RESIDUAL_FINGERPRINT for {path} = {fp}");
        assert_ne!(
            base_norm, head_norm,
            "{path}: a manual-ruling file must have a residual (else it is not manual)"
        );
    }
}

/// CS-1R2 A2c — PERMANENT TEETH: prove the sqlx signature now PINS the decode
/// type (the old gate blanket-stripped the turbofish, pinning it NOWHERE — a
/// decode-type swap on a query was invisible to the tool). A compile-broken swap
/// in a real file is caught by rustc; this exercises the tool-level pin directly
/// on syn snippets so the teeth is empirical and independent of compilation.
#[test]
fn a2c_decode_type_is_pinned_in_sig() {
    let head_shiftstate: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar::<_, prro::db::types::DbShiftState>(\
         \"SELECT state FROM shifts WHERE shift_id = ?\").bind(id).fetch_one(p); }",
    )
    .unwrap();
    let head_docstate: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar::<_, prro::db::types::DbDocState>(\
         \"SELECT state FROM shifts WHERE shift_id = ?\").bind(id).fetch_one(p); }",
    )
    .unwrap();
    // bare, module-path-free base form (what CS-1 base looked like where explicit)
    let base_shiftstate: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar::<_, ShiftState>(\
         \"SELECT state FROM shifts WHERE shift_id = ?\").bind(id).fetch_one(p); }",
    )
    .unwrap();

    let s_shift = &extract_sqlx_sigs(&head_shiftstate)[0];
    let s_doc = &extract_sqlx_sigs(&head_docstate)[0];
    let s_base = &extract_sqlx_sigs(&base_shiftstate)[0];

    // SQL + binds + fetch identical, ONLY the decode type differs → the sigs must
    // now DIFFER (the pin bites). Before A2c, decode_type did not exist → equal.
    assert_ne!(
        s_shift.decode_type, s_doc.decode_type,
        "decode type MUST be pinned: DbShiftState vs DbDocState are distinct"
    );
    assert!(
        !s_shift.equiv_across_cs1(s_doc),
        "a decode-type swap must make the signatures diverge"
    );
    // module-path / Db-wrapper transparency: qualified `DbShiftState` and bare
    // `ShiftState` are the SAME decode type (the whitelisted W3 transform).
    assert_eq!(
        s_shift.decode_type, s_base.decode_type,
        "qualified DbShiftState and bare ShiftState must normalize equal (W3)"
    );
    assert!(
        s_shift.equiv_across_cs1(s_base),
        "the whitelisted qualified+Db* transform must stay equivalent"
    );
}

/// CS-1R2 A4 — PERMANENT TEETH: prove the sqlx signature now compares the RAW
/// runtime SQL (the bytes SQLite executes) and accepts a change ONLY when it is in
/// the `RUNTIME_SQL_DELTAS` catalog. The old tool stripped the `as "col: Type"`
/// alias from BOTH endpoints, so an alias removal (a real runtime-SQL byte change)
/// was HIDDEN. This exercises the tool directly on syn snippets.
#[test]
fn a4_runtime_sql_alias_removal_is_catalogued_not_hidden() {
    let with_alias: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar(\
         r#\"SELECT state as \"state: ShiftState\" FROM shifts WHERE shift_id = ?\"#)\
         .bind(id).fetch_one(p); }",
    )
    .unwrap();
    let no_alias: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar::<_, DbShiftState>(\
         \"SELECT state FROM shifts WHERE shift_id = ?\")\
         .bind(id).fetch_one(p); }",
    )
    .unwrap();
    // an UNCATALOGUED alias removal (different table) — must diverge.
    let uncatalogued_base: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar(\
         r#\"SELECT x as \"x: DocState\" FROM other WHERE id = ?\"#)\
         .bind(id).fetch_one(p); }",
    )
    .unwrap();
    let uncatalogued_head: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar::<_, DbDocState>(\
         \"SELECT x FROM other WHERE id = ?\")\
         .bind(id).fetch_one(p); }",
    )
    .unwrap();

    let b = &extract_sqlx_sigs(&with_alias)[0];
    let h = &extract_sqlx_sigs(&no_alias)[0];
    // the RAW SQL genuinely differs (the alias IS part of the executed statement)
    assert_ne!(
        b.sql_raw, h.sql_raw,
        "the raw runtime SQL must differ — the alias is executed bytes, not hidden"
    );
    // but this specific delta is CATALOGUED → equiv.
    assert!(
        b.equiv_across_cs1(h),
        "the catalogued read_shift_state alias-removal must be accepted"
    );

    let ub = &extract_sqlx_sigs(&uncatalogued_base)[0];
    let uh = &extract_sqlx_sigs(&uncatalogued_head)[0];
    assert!(
        !ub.equiv_across_cs1(uh),
        "an UNCATALOGUED runtime-SQL change must be RED (not silently hidden)"
    );
}

#[allow(clippy::too_many_arguments)]
fn check_one(
    path: &str,
    base_src: &str,
    head_src: &str,
    manual: &BTreeSet<String>,
    ast_ok: &mut usize,
    ast_manual: &mut usize,
    failures: &mut Vec<String>,
) {
    // ── sqlx signature equality (the bind-ORDER / SQL / fetch-mode pin) ──
    let base_file = match syn::parse_file(base_src) {
        Ok(f) => f,
        Err(e) => {
            failures.push(format!("{path}: base parse error: {e}"));
            return;
        }
    };
    let head_file = match syn::parse_file(head_src) {
        Ok(f) => f,
        Err(e) => {
            failures.push(format!("{path}: head parse error: {e}"));
            return;
        }
    };
    let base_sigs = extract_sqlx_sigs(&base_file);
    let head_sigs = extract_sqlx_sigs(&head_file);
    let sigs_diverge = base_sigs.len() != head_sigs.len()
        || base_sigs
            .iter()
            .zip(head_sigs.iter())
            .any(|(b, h)| !b.equiv_across_cs1(h));
    if sigs_diverge {
        // Produce a focused first-divergence diagnostic.
        let mut diag = format!(
            "{path}: sqlx signature drift ({} base chains vs {} head chains)",
            base_sigs.len(),
            head_sigs.len()
        );
        for (i, (b, h)) in base_sigs.iter().zip(head_sigs.iter()).enumerate() {
            if !b.equiv_across_cs1(h) {
                diag.push_str(&format!(
                    "\n  chain #{i} in fn `{}`:\n    base sql_raw={:?} decode={:?} binds={:?} fetch={:?}\n    head sql_raw={:?} decode={:?} binds={:?} fetch={:?}\n  (a runtime-SQL edit must be in RUNTIME_SQL_DELTAS to be accepted — A4)",
                    b.enclosing_fn, b.sql_raw, b.decode_type, b.binds, b.fetch_mode, h.sql_raw, h.decode_type, h.binds, h.fetch_mode
                ));
                break;
            }
        }
        failures.push(diag);
    }

    // ── AST token-equality outside the whitelist ──
    let base_norm = match normalize_source(base_src) {
        Ok(t) => t.to_string(),
        Err(e) => {
            failures.push(format!("{path}: base normalize error: {e}"));
            return;
        }
    };
    let head_norm = match normalize_source(head_src) {
        Ok(t) => t.to_string(),
        Err(e) => {
            failures.push(format!("{path}: head normalize error: {e}"));
            return;
        }
    };
    if base_norm == head_norm {
        *ast_ok += 1;
    } else if manual.contains(path) {
        // A manual-ruling file: its residual MUST be EXACTLY the documented T7
        // use-site `.0` — pinned by fingerprint (A2a). Any OTHER residual (an
        // assertion RHS flip, a control-flow edit, an added statement) perturbs
        // the head token stream → the fingerprint changes → FLAGGED. The old
        // gate accepted ANY residual here, which let a real change ride in on a
        // manual-ruling file. The sqlx signature check above also still applies.
        let fp = manual_residual_fingerprint(&base_norm, &head_norm);
        if fp == MANUAL_RESIDUAL_FINGERPRINT {
            *ast_manual += 1;
        } else {
            let diag = first_token_divergence(&base_norm, &head_norm);
            failures.push(format!(
                "{path}: manual-ruling file residual does NOT match the pinned T7 \
                 fingerprint — the ONLY tolerated residual is the documented use-site \
                 `.0` (see docs/cs1r/CS1_TEST_PROVENANCE.md). This looks like a real \
                 change smuggled onto a manual-ruling file.\n  expected fp={MANUAL_RESIDUAL_FINGERPRINT}\n  actual   fp={fp}\n{diag}"
            ));
        }
    } else {
        // Flag the first token divergence for the operator.
        let diag = first_token_divergence(&base_norm, &head_norm);
        failures.push(format!(
            "{path}: AST token divergence OUTSIDE the whitelist (needs a manual \
             ruling or is genuine drift):\n{diag}"
        ));
    }
}

/// CS-1R2 A2c — pin the worktree's EXPLICIT sqlx decode types against the frozen
/// head blob. Both endpoints are post-CS-1 (both explicit where CS-1 pinned a
/// type), so a live change of a decode type to a DIFFERENT explicit type is RED.
/// Order-aligned by the `(enclosing_fn, occurrence)` identity carried in SqlxSig.
fn pin_worktree_decode_types(
    path: &str,
    head_blob_src: &str,
    worktree_src: &str,
    failures: &mut Vec<String>,
) {
    let head_file = match syn::parse_file(head_blob_src) {
        Ok(f) => f,
        Err(_) => return, // parse errors already surfaced by check_one
    };
    let wt_file = match syn::parse_file(worktree_src) {
        Ok(f) => f,
        Err(_) => return,
    };
    let head_sigs = extract_sqlx_sigs(&head_file);
    let wt_sigs = extract_sqlx_sigs(&wt_file);
    if head_sigs.len() != wt_sigs.len() {
        return; // a chain add/remove is caught by the base↔worktree sig compare
    }
    for (h, w) in head_sigs.iter().zip(wt_sigs.iter()) {
        // both explicit (or both empty) → require exact equality; only wildcard
        // when one is genuinely inferred (should not happen post-CS-1, but stays
        // safe).
        if !h.decode_type.is_empty() && !w.decode_type.is_empty() && h.decode_type != w.decode_type
        {
            failures.push(format!(
                "{path}: sqlx DECODE-TYPE drift in fn `{}` (occ {}): head blob pins \
                 `{}`, worktree has `{}` — a decode type was changed to a different \
                 type (not a whitelisted inferred→explicit `Db*` transform).",
                w.enclosing_fn, w.occurrence, h.decode_type, w.decode_type
            ));
        }
    }
}

/// Show the first differing region of two normalized token strings.
fn first_token_divergence(a: &str, b: &str) -> String {
    let at: Vec<&str> = a.split_whitespace().collect();
    let bt: Vec<&str> = b.split_whitespace().collect();
    let mut i = 0;
    while i < at.len() && i < bt.len() && at[i] == bt[i] {
        i += 1;
    }
    let ctx = |v: &[&str], i: usize| {
        let lo = i.saturating_sub(6);
        let hi = (i + 8).min(v.len());
        v[lo..hi].join(" ")
    };
    format!(
        "  first divergence at token #{i}:\n    base: …{}…\n    head: …{}…",
        ctx(&at, i),
        ctx(&bt, i)
    )
}
