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
    canonical_fingerprint, canonical_fingerprints, extract_sqlx_sigs, git_hash_object, git_show,
    manual_residual_fingerprint, manual_residual_fingerprint_pin, manual_ruling_files,
    normalize_source, post_cs1_carveout, repo_root, BASE_SHA, CANONICAL_FINGERPRINTS_FILE,
    CANONICAL_PIN_DIR, HEAD_SHA, MANUAL_RESIDUAL_FINGERPRINT_FILE,
};

// CS-1R3 A2 — the pinned EXACT residual fingerprint of the sole manual-ruling file
// (`live_dps_extended_smoke.rs`) is LOADED from the code-owner-gated pin file
// `docs/cs1r/pins/manual_residual_fingerprint.txt` via `manual_residual_fingerprint_pin()`,
// NOT a constant next to this oracle (A2: a pin next to its checker is self-
// rewritable). Its post-canonicalization residual is the documented T7 use-site `.0`
// (base→head); ANY *other* residual (an assertion RHS flip, a control-flow edit, an
// added statement) perturbs the head token stream → a different fingerprint → RED.
// Re-mint (via the ignored helper `print_manual_residual_fingerprint` below) ONLY
// when the documented T7 residual itself legitimately changes — a CODEOWNER-gated
// data diff, distinct from this logic.

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

// CS-1R3 A2 (was A2b) — files whose WORKING-TREE content legitimately differs from
// `f2628ba` due to a specific, APPROVED post-CS-1 change (the fuzzer oracle fix in
// `32166cc`) are LOADED from the code-owner-gated pin file
// `docs/cs1r/pins/post_cs1_carveout.tsv` via `post_cs1_carveout()` — NOT a constant
// next to this oracle (A2: a pin next to its checker is self-rewritable). Each
// carries the EXACT approved blob SHA. The live-drift leg asserts the worktree blob
// == the approved SHA (any FURTHER drift → RED) and runs the AST/sqlx compare on the
// frozen CS-1 endpoint (base↔`f2628ba`). A future legitimate change re-pins the SHA
// in that DATA file with an artifact note — a CODEOWNER-gated diff distinct from
// this logic.

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

/// CS-1R4 (auditor finding-7) — PIN-LOADER HARDENING canary. A pin file placed
/// ANYWHERE outside the ONE canonical path must be IGNORED: the loader resolves the
/// pin ONLY from the compile-time canonical const (`docs/cs1r/pins/…`) joined to the
/// repo root — no directory scan, no glob, no env override, no `..` escape. This
/// canary drops a SPOOFED fingerprint file at a DIFFERENT path (both a sibling name
/// in the canonical dir and a wholly different dir) and proves the loader still
/// returns the CANONICAL value, never the spoof. If the loader ever honored an
/// off-canon pin, this would go RED (the returned value would be the spoof's).
#[test]
fn pin_loader_reads_only_canonical_path() {
    let root = repo_root();

    // The canonical fingerprint value the loader MUST return.
    let canonical = manual_residual_fingerprint_pin();
    assert_ne!(canonical, "", "canonical fingerprint must be non-empty");

    let spoof_value = "deadbeefdeadbeefdeadbeefdeadbeef"; // 32 hex, clearly not the pin
    assert_ne!(
        canonical, spoof_value,
        "test invariant: spoof value must differ from the canonical pin"
    );

    // (a) spoof placed in a DIFFERENT directory (docs/cs1r/pins_spoof/).
    let spoof_dir = root.join("docs/cs1r/pins_spoof");
    let spoof_a = spoof_dir.join("manual_residual_fingerprint.txt");
    // (b) spoof placed as a SIBLING file (different basename) in the canonical dir.
    let spoof_b = root
        .join(CANONICAL_PIN_DIR)
        .join("manual_residual_fingerprint.SPOOF.txt");

    let _guard = SpoofGuard {
        paths: vec![spoof_a.clone(), spoof_b.clone()],
        dirs: vec![spoof_dir.clone()],
    };
    std::fs::create_dir_all(&spoof_dir).expect("mk spoof dir");
    std::fs::write(&spoof_a, format!("{spoof_value}\n")).expect("write spoof a");
    std::fs::write(&spoof_b, format!("{spoof_value}\n")).expect("write spoof b");

    // The loader still returns the CANONICAL value — the spoofs are never read.
    let after = manual_residual_fingerprint_pin();
    assert_eq!(
        after, canonical,
        "PIN-LOADER HARDENING (finding-7): a spoofed pin outside the canonical path \
         ({}) must be ignored — the loader read a non-canonical value.",
        MANUAL_RESIDUAL_FINGERPRINT_FILE,
    );

    // Sanity: the loader physically refuses a non-canonical arg (the choke point).
    // We can only exercise the public loader, which is already pinned to the
    // canonical const; the assert_eq above proves off-canon files are inert.
}

/// Best-effort cleanup of the spoof files/dirs the canary creates.
struct SpoofGuard {
    paths: Vec<std::path::PathBuf>,
    dirs: Vec<std::path::PathBuf>,
}
impl Drop for SpoofGuard {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
        for d in &self.dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

/// LEG 1 — immutable provenance: base blob vs head blob. AST-equality (outside
/// whitelist) + sqlx signature equality for every one of the 79 files.
#[test]
fn cs1_immutable_provenance_base_vs_head() {
    let root = repo_root();
    let manual = manual_ruling_files();
    let manual_fp = manual_residual_fingerprint_pin();
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
            &manual_fp,
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

/// LEG 2 — live-drift / teeth: the WORKING-TREE file vs its CANONICAL FINGERPRINT.
///
/// CS-1R5: this leg no longer references git history. It used to shell
/// `git show <LIVE_DRIFT_BASE_SHA>:<path>` for all 79 files, where the anchor was
/// a commit INSIDE a feature branch — unreachable from `main` after a squash
/// merge, so the leg silently depended on that branch never being deleted, and
/// `git_show` hard-asserts, so losing the object would have failed all 79 files
/// at once with an opaque message. The fingerprints in
/// `docs/cs1r/pins/cs1_canonical_fingerprints.tsv` depend on CONTENT, not on
/// reachability. (The IMMUTABLE leg still uses `git show`, correctly: `f2c17b1`
/// and `f2628ba` are ancestors of `main`.)
///
/// The accept-set is unchanged: a whitelisted-neutral edit leaves the canonical
/// AST identical, so its digest is identical; anything else diverges and is RED.
/// One contract change is adjudicated in `canonical_fingerprint`'s doc: the decode
/// type is now hashed verbatim instead of going through the non-transitive
/// empty-is-wildcard rule — strictly stricter, never weaker.
#[test]
fn cs1_live_drift_base_vs_worktree() {
    let root = repo_root();
    // CS-1R3 A2 — carve-outs loaded from the code-owner-gated pin DATA file.
    // The carve-out pin is a WORKTREE BLOB hash (`git hash-object`), which is
    // already content-addressed and never depended on reachability.
    let carveout = post_cs1_carveout();
    let approved: std::collections::BTreeMap<&str, &str> = carveout
        .iter()
        .map(|(p, sha)| (p.as_str(), sha.as_str()))
        .collect();
    let pinned = canonical_fingerprints();
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for path in CS1_MODIFIED_FILES {
        let abs = root.join(path);
        let src = std::fs::read_to_string(&abs)
            .unwrap_or_else(|e| panic!("read worktree file {}: {e}", abs.display()));

        if let Some(&approved_sha) = approved.get(path) {
            // A carved-out file carries an APPROVED post-CS-1 delta. The worktree
            // MUST equal the pinned blob exactly — any FURTHER drift (a mutation
            // smuggled in) is RED. A future legitimate change re-pins the
            // carve-out SHA with an artifact note.
            let live_sha = git_hash_object(&root, &abs);
            if live_sha != approved_sha {
                failures.push(format!(
                    "{path}: worktree drifted from the APPROVED post-CS-1 blob. Any change to \
                     this carved-out file must re-pin the carve-out SHA in \
                     docs/cs1r/pins/post_cs1_carveout.tsv with an artifact note.\n  \
                     approved sha={approved_sha}\n  worktree sha={live_sha}"
                ));
            }
        }

        let Some((want_ast, want_sqlx)) = pinned.get(*path) else {
            failures.push(format!(
                "{path}: NO row in {CANONICAL_FINGERPRINTS_FILE}. Every frozen file must be \
                 pinned — re-mint with `mint_canonical_fingerprints -- --ignored`."
            ));
            continue;
        };
        let (got_ast, got_sqlx) = match canonical_fingerprint(&src) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{path}: canonicalisation failed: {e}"));
                continue;
            }
        };
        checked += 1;

        if &got_ast != want_ast {
            failures.push(format!(
                "{path}: CANONICAL AST fingerprint diverged — the file changed beyond the \
                 whitelisted CS-1 transforms.\n  pinned   {want_ast}\n  worktree {got_ast}\n  \
                 If the change is an adjudicated one, re-mint the manifest and SAY WHY in the PR."
            ));
        }
        if &got_sqlx != want_sqlx {
            failures.push(format!(
                "{path}: SQLX-signature fingerprint diverged — an SQL edit, a bind swap/drop, a \
                 fetch-mode change, or a DECODE-TYPE change.\n  pinned   {want_sqlx}\n  \
                 worktree {got_sqlx}"
            ));
        }
    }

    // Totality: every pinned row must correspond to a frozen file, so a stale row
    // (a file dropped from the set) cannot sit in the manifest unnoticed.
    let frozen: std::collections::BTreeSet<&str> = CS1_MODIFIED_FILES.iter().copied().collect();
    for path in pinned.keys() {
        if !frozen.contains(path.as_str()) {
            failures.push(format!(
                "{CANONICAL_FINGERPRINTS_FILE} pins {path:?}, which is NOT in the frozen set — \
                 a stale row. Re-mint."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "live-drift (canonical fingerprints) failures — a CS-1 test file diverged from its \
         pinned canonical form beyond the whitelisted transforms:\n{}",
        failures.join("\n\n")
    );
    assert_eq!(
        checked, 79,
        "every frozen file must have been fingerprinted"
    );
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

/// CS-1R4 — PERMANENT TEETH: the sqlx signature treats a `col as "col: Type"`
/// DECODE-ANNOTATION removal as a **fiscal-neutral transform class** (like W1-W4/T3)
/// — it normalizes to equal — while ANY OTHER SQL edit still diverges → RED. There
/// is NO SQL-byte-identity assertion and NO catalogued diff-set (round-4 auditor:
/// "don't add SQL machinery"). The tool asserts binds/fetch/decode-type, and the
/// SQL surface compared is the annotation-stripped `sql`. This exercises the tool on
/// syn snippets so the teeth is empirical and independent of compilation.
#[test]
fn a4_runtime_sql_alias_removal_is_catalogued_not_hidden() {
    // (1) a pure decode-annotation removal → normalizes equal (fiscal-neutral class).
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
    let b = &extract_sqlx_sigs(&with_alias)[0];
    let h = &extract_sqlx_sigs(&no_alias)[0];
    assert!(
        b.equiv_across_cs1(h),
        "a pure `as \"col: Type\"` decode-annotation removal must be accepted as the \
         fiscal-neutral transform class (annotation-stripped SQL is equal)"
    );

    // (2) a REAL SQL edit (different table + WHERE) must still diverge — the
    // annotation-strip does NOT hide a genuine statement change.
    let real_base: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar(\
         r#\"SELECT x as \"x: DocState\" FROM other WHERE id = ?\"#)\
         .bind(id).fetch_one(p); }",
    )
    .unwrap();
    let real_head: syn::File = syn::parse_str(
        "fn q() { sqlx::query_scalar::<_, DbDocState>(\
         \"SELECT x FROM DIFFERENT_TABLE WHERE id = ?\")\
         .bind(id).fetch_one(p); }",
    )
    .unwrap();
    let rb = &extract_sqlx_sigs(&real_base)[0];
    let rh = &extract_sqlx_sigs(&real_head)[0];
    assert!(
        !rb.equiv_across_cs1(rh),
        "a genuine SQL edit (table/WHERE) beyond the decode-annotation must be RED"
    );

    // (3) a changed LITERAL VALUE inside a SQL string must still diverge (the
    // annotation-strip only removes `as \"…: …\"`, never a VALUES literal).
    let val_base: syn::File =
        syn::parse_str("fn q() { sqlx::query(\"INSERT INTO t VALUES (0)\").execute(p); }").unwrap();
    let val_head: syn::File =
        syn::parse_str("fn q() { sqlx::query(\"INSERT INTO t VALUES (1)\").execute(p); }").unwrap();
    let vb = &extract_sqlx_sigs(&val_base)[0];
    let vh = &extract_sqlx_sigs(&val_head)[0];
    assert!(
        !vb.equiv_across_cs1(vh),
        "a changed SQL literal value must be RED (annotation-strip does not hide it)"
    );
}

#[allow(clippy::too_many_arguments)]
fn check_one(
    path: &str,
    base_src: &str,
    head_src: &str,
    manual: &BTreeSet<String>,
    // CS-1R3 A2 — the manual-residual fingerprint pin, loaded ONCE by the caller
    // from the code-owner-gated `docs/cs1r/pins/manual_residual_fingerprint.txt`.
    manual_fp: &str,
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
                    "\n  chain #{i} in fn `{}`:\n    base sql={:?} decode={:?} binds={:?} fetch={:?}\n    head sql={:?} decode={:?} binds={:?} fetch={:?}\n  (only the fiscal-neutral `as \"col: Type\"` decode-annotation removal is normalized — CS-1R4 T8 class; any other SQL edit is RED)",
                    b.enclosing_fn, b.sql, b.decode_type, b.binds, b.fetch_mode, h.sql, h.decode_type, h.binds, h.fetch_mode
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
        if fp == manual_fp {
            *ast_manual += 1;
        } else {
            let diag = first_token_divergence(&base_norm, &head_norm);
            failures.push(format!(
                "{path}: manual-ruling file residual does NOT match the pinned T7 \
                 fingerprint — the ONLY tolerated residual is the documented use-site \
                 `.0` (see docs/cs1r/CS1_TEST_PROVENANCE.md). This looks like a real \
                 change smuggled onto a manual-ruling file.\n  expected fp={manual_fp}\n  actual   fp={fp}\n{diag}"
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

/// CS-1R5 MINT (human-run, `#[ignore]`d) — regenerate the canonical-fingerprint
/// manifest at the CURRENT worktree.
///
/// Same discipline as the inventory manifests: CI never auto-mints, because a
/// drift is MEANT to turn the gate RED so a human adjudicates and re-mints on
/// purpose. Run with:
///
///   cargo test -p prro --features test-support --test cs1_test_provenance \
///       mint_canonical_fingerprints -- --ignored --nocapture
#[test]
#[ignore = "human-run mint: regenerates docs/cs1r/pins/cs1_canonical_fingerprints.tsv"]
fn mint_canonical_fingerprints() {
    let root = repo_root();
    let mut rows: Vec<String> = Vec::new();
    for path in CS1_MODIFIED_FILES {
        let src =
            std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let (ast, sqlx) =
            canonical_fingerprint(&src).unwrap_or_else(|e| panic!("canonicalise {path}: {e}"));
        rows.push(format!("{ast}\t{sqlx}\t{path}"));
    }
    rows.sort();
    let header = "\
# CS-1R5 — CANONICAL FINGERPRINTS of the 79 frozen CS-1 test files (DATA, code-owner-gated).
#
# Replaces the former `LIVE_DRIFT_BASE_SHA` git anchor. That anchor pointed at a commit INSIDE a
# feature branch; after a squash-merge it is NOT reachable from `main`, so the live-drift leg only
# worked while the branch survived on the remote — and `git show` hard-asserts, so losing it would
# have failed all 79 files at once with an opaque message. A fingerprint depends on CONTENT, not on
# history reachability.
#
# Columns (TAB-separated):
#   <ast_sha256> <TAB> <sqlx_sha256> <TAB> <repo-relative path>
#
#   ast_sha256  — sha256 of `normalize_source(src).to_string()`: the file's AST after the
#                 whitelisted CS-1 transforms (W1-W4 Db* unwrap, T3 enum `.as_str()`, T6 import
#                 drop, T8 decode-annotation strip). A whitelisted-neutral edit does NOT change it;
#                 anything else does.
#   sqlx_sha256 — sha256 of the explicit, hand-written encoding of every sqlx signature in the file
#                 (enclosing fn, occurrence, decode-annotation-stripped SQL, query kind, DECODE
#                 TYPE, ordered bind vector, fetch mode). Carried separately because the
#                 canonicaliser deletes the query-head turbofish, so an AST-only digest would stop
#                 pinning the decode type.
#
# RE-MINT is legitimate ONLY for an adjudicated change to a frozen file, and the PR must say what
# changed and why — exactly the discipline the old one-line re-anchor carried. A bulk re-mint with
# no written rationale is the failure mode to watch for: prefer one re-mint per adjudicated change.
";
    let body = rows.join("\n");
    let out = format!("{header}{body}\n");
    let dest = root.join(CANONICAL_FINGERPRINTS_FILE);
    std::fs::write(&dest, out).unwrap_or_else(|e| panic!("write {}: {e}", dest.display()));
    eprintln!(
        "minted {} canonical fingerprints -> {}",
        CS1_MODIFIED_FILES.len(),
        dest.display()
    );
}
