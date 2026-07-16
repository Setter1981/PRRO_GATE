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
    extract_sqlx_sigs, git_show, manual_ruling_files, normalize_source, repo_root, BASE_SHA,
    HEAD_SHA,
};

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

/// Files whose WORKING-TREE content legitimately differs from `f2628ba` due to a
/// post-CS-1 change on this branch (the oracle fix in `32166cc`). For the
/// live-drift leg these are compared against their `f2628ba` blob instead of the
/// working tree, so the (approved, non-CS-1) delta does not false-RED the gate.
const POST_CS1_CARVEOUT: &[&str] = &["rust/prro/tests/invariant_fuzzer/model.rs"];

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
    let carveout: BTreeSet<&str> = POST_CS1_CARVEOUT.iter().copied().collect();
    let mut ast_ok = 0usize;
    let mut ast_manual = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in CS1_MODIFIED_FILES {
        let base_src = git_show(&root, BASE_SHA, path);
        let head_src = if carveout.contains(path) {
            // approved post-CS-1 delta: compare base against the frozen head blob.
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
    if base_sigs != head_sigs {
        // Produce a focused first-divergence diagnostic.
        let mut diag = format!(
            "{path}: sqlx signature drift ({} base chains vs {} head chains)",
            base_sigs.len(),
            head_sigs.len()
        );
        for (i, (b, h)) in base_sigs.iter().zip(head_sigs.iter()).enumerate() {
            if b != h {
                diag.push_str(&format!(
                    "\n  chain #{i} in fn `{}`:\n    base sql={:?} binds={:?} fetch={:?}\n    head sql={:?} binds={:?} fetch={:?}",
                    b.enclosing_fn, b.sql, b.binds, b.fetch_mode, h.sql, h.binds, h.fetch_mode
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
        // A manual-ruling file: its residual is the documented T7 use-site `.0`.
        // The sqlx signature check above still fully applies, so a bind/SQL/fetch
        // change is still RED even here.
        *ast_manual += 1;
    } else {
        // Flag the first token divergence for the operator.
        let diag = first_token_divergence(&base_norm, &head_norm);
        failures.push(format!(
            "{path}: AST token divergence OUTSIDE the whitelist (needs a manual \
             ruling or is genuine drift):\n{diag}"
        ));
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
