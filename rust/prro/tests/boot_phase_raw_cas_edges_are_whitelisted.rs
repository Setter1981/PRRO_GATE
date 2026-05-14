//! M3b W1 regression — pivoted from raw-CAS-scan to
//! `transition_with_audit` + direct `transition_state` call-site
//! enumeration in `services/reconciliation/boot_phase.rs`.
//!
//! ## What this test guards
//!
//! W1 promoted the 7 boot-phase raw-CAS sites to go through the
//! repository `fiscal_documents::transition_state` (whitelist single
//! source of truth) via two paths:
//!   - **Simple sites** call the service-layer helper
//!     `transition_with_audit(tx, doc_id, from, to, …)` (sites
//!     `resume_sending_to_error_retryable`,
//!     `cas_error_retryable_to_manual_reconciliation`,
//!     `cas_error_retryable_budget_exhausted`, and the inline
//!     Encrypted→ER reroute in branch (c)).
//!   - **Complex sites** (need supplementary writes ordered after
//!     CAS) call `fiscal_documents::transition_state(tx, doc_id,
//!     DocState::X, DocState::Y)` directly, then do supplementary
//!     writes + audit inline (sites `advance_sent_to_kvt1_from_probe`,
//!     `cas_sent_to_manual_reconciliation_from_probe`,
//!     `cas_sent_to_error_retryable_from_probe`).
//!
//! Both paths route through the whitelist gate that
//! `fiscal_documents::transition_state` enforces.  No raw
//! `UPDATE fiscal_documents SET state = '<X>' WHERE … AND state =
//! '<Y>'` SQL remains in `boot_phase.rs` — verified by the
//! `no_raw_update_fiscal_documents_set_state` test below.
//!
//! This pivot keeps the regression guarantee from PR #43 (HIGH-2)
//! while reflecting the W1 structural change.  The locked
//! `EXPECTED_HELPER_CALL_SITES = 7` count makes silent additions
//! (a new boot-phase CAS edge that bypasses the whitelist) fail
//! at `cargo test --features test-support` time.
//!
//! M3b carry-forward: when the boot-phase complex sites get further
//! refactored into a more focused helper (e.g. one that takes a
//! closure for supplementary writes), the scanner here can be
//! tightened further; for now it accepts both helper-call-site and
//! direct-`transition_state`-call-site shapes.

use prro::db::models::enums::DocState;
use prro::db::repositories::fiscal_documents::allowed_transition;

const BOOT_PHASE_SRC: &str = include_str!("../src/services/reconciliation/boot_phase.rs");

/// How many service-layer CAS call sites exist in `boot_phase.rs` that
/// drive a fiscal-document state transition.  Locked here so silent
/// additions surface as a test failure — the new site must be
/// inspected and the count updated together.
///
/// Sites today (post-W1, against `rust-gateway` `1651502`):
///   - `resume_sending_to_error_retryable`             (SENDING        → ERROR_RETRYABLE)              — helper
///   - `advance_sent_to_kvt1_from_probe`               (SENT           → KVT1)                         — direct `transition_state`
///   - `cas_sent_to_manual_reconciliation_from_probe`  (SENT           → REQUIRES_MANUAL_RECONCILIATION) — direct `transition_state`
///   - `cas_sent_to_error_retryable_from_probe`        (SENT           → ERROR_RETRYABLE)              — direct `transition_state`
///   - `cas_error_retryable_to_manual_reconciliation`  (ERROR_RETRYABLE → REQUIRES_MANUAL_RECONCILIATION) — helper
///   - `cas_error_retryable_budget_exhausted`          (ERROR_RETRYABLE → REQUIRES_MANUAL_RECONCILIATION) — helper
///   - branch (c) `Encrypted` reroute (inline)          (ENCRYPTED      → ERROR_RETRYABLE)              — helper
const EXPECTED_HELPER_CALL_SITES: usize = 7;

#[derive(Debug, Clone, PartialEq)]
struct CasCallSite {
    /// 1-based line number of the opening token (helper name or
    /// `fiscal_documents::transition_state` qualified path).
    line_number: usize,
    /// Source-state literal (DocState variant name as it appears
    /// in the call, e.g. `Sending` or `Sent`).
    from_variant: String,
    /// Target-state literal (DocState variant name).
    to_variant: String,
    /// Whether this site goes through the service-layer helper
    /// (`transition_with_audit`) or directly through
    /// `fiscal_documents::transition_state`.
    kind: CasCallKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CasCallKind {
    /// Site invokes the service-layer composition helper.  Audit row
    /// + payload closure are passed to the helper.
    HelperServiceLayer,
    /// Site invokes the repository fn directly because it needs
    /// supplementary writes (e.g. `document_files::replace_tx`,
    /// `transport_trace::complete_via_recovery_tx`) between the CAS
    /// and the audit row.  Caller writes the audit inline.
    DirectRepositoryFn,
}

/// Parse a `DocState::<Variant>` token's variant name into a
/// `DocState` value.  Used to validate `(from, to)` pairs against
/// the whitelist.
fn parse_variant(variant: &str) -> DocState {
    match variant {
        "Prepared" => DocState::Prepared,
        "Signed" => DocState::Signed,
        "Encrypted" => DocState::Encrypted,
        "Sending" => DocState::Sending,
        "Sent" => DocState::Sent,
        "Kvt1" => DocState::Kvt1,
        "Kvt2" => DocState::Kvt2,
        "Ack" => DocState::Ack,
        "OfflineLocalAck" => DocState::OfflineLocalAck,
        "Rejected" => DocState::Rejected,
        "Cancelled" => DocState::Cancelled,
        "ErrorRetryable" => DocState::ErrorRetryable,
        "RequiresManualReconciliation" => DocState::RequiresManualReconciliation,
        other => panic!(
            "boot_phase.rs CAS call site references an unknown `DocState::{other}`. \
             Either add the variant to DocState (and to this parser) or fix the typo."
        ),
    }
}

/// Find the next `DocState::<Variant>` token in `text` starting from
/// `start_idx`; returns the variant name + the byte index immediately
/// past the variant identifier (so caller can resume scanning).
fn find_next_doc_state(text: &str, start_idx: usize) -> Option<(String, usize)> {
    let marker = "DocState::";
    let rel = text[start_idx..].find(marker)?;
    let after_marker_idx = start_idx + rel + marker.len();
    let tail = &text[after_marker_idx..];
    // Variant name is the longest leading run of ident chars.
    let end_offset = tail
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(tail.len());
    let variant = tail[..end_offset].to_string();
    if variant.is_empty() {
        return None;
    }
    Some((variant, after_marker_idx + end_offset))
}

/// Compute 1-based line number for a byte index in `src`.
fn line_number_for_byte(src: &str, byte_idx: usize) -> usize {
    src[..byte_idx.min(src.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

/// Scan `boot_phase.rs` for both call-site shapes.  Returns one
/// `CasCallSite` per invocation, regardless of shape.
///
/// **Helper invocation pattern** (multi-line via rustfmt):
///   `transition_with_audit(\n  tx,\n  doc_id,\n  DocState::Sending,\n  DocState::ErrorRetryable,\n  …)`
///
/// **Direct invocation pattern** (multi-line):
///   `fiscal_documents::transition_state(\n  tx,\n  doc_id,\n  DocState::Sent,\n  DocState::Kvt1,\n)`
///
/// In both cases the first two `DocState::X` tokens after the call
/// marker are `from` and `to` respectively.
fn scan_cas_call_sites(src: &str) -> Vec<CasCallSite> {
    let mut sites = Vec::new();

    for (marker, kind) in [
        ("transition_with_audit(", CasCallKind::HelperServiceLayer),
        (
            "fiscal_documents::transition_state(",
            CasCallKind::DirectRepositoryFn,
        ),
    ] {
        let mut cursor = 0usize;
        while let Some(rel) = src[cursor..].find(marker) {
            let call_byte = cursor + rel;
            cursor = call_byte + marker.len();

            // Find the next two DocState::<Variant> tokens — these
            // are `from` and `to` in helper-arg order.  We bound the
            // search to the next ~600 bytes (well past a typical
            // 5-line rustfmt'd call) so we don't accidentally pair
            // across function bodies.
            let scan_end = (cursor + 600).min(src.len());
            let region = &src[cursor..scan_end];

            let Some((from_variant, after_from)) = find_next_doc_state(region, 0) else {
                continue;
            };
            let Some((to_variant, _after_to)) = find_next_doc_state(region, after_from) else {
                continue;
            };

            sites.push(CasCallSite {
                line_number: line_number_for_byte(src, call_byte),
                from_variant,
                to_variant,
                kind: kind.clone(),
            });
        }
    }
    sites.sort_by_key(|s| s.line_number);
    sites
}

#[test]
fn boot_phase_cas_call_sites_are_whitelisted() {
    let sites = scan_cas_call_sites(BOOT_PHASE_SRC);

    assert_eq!(
        sites.len(),
        EXPECTED_HELPER_CALL_SITES,
        "Expected exactly {} CAS call sites in boot_phase.rs \
         (helper invocations + direct `fiscal_documents::transition_state` calls), \
         found {}.  If you added or removed one, update \
         EXPECTED_HELPER_CALL_SITES (and review the new site for whitelist \
         alignment).  Found sites: {:#?}",
        EXPECTED_HELPER_CALL_SITES,
        sites.len(),
        sites
    );

    let mut failures = Vec::new();
    for site in &sites {
        let from = parse_variant(&site.from_variant);
        let to = parse_variant(&site.to_variant);
        if !allowed_transition(from, to) {
            failures.push(format!(
                "line {}: ({}, {}) [{:?}] is NOT in `fiscal_documents::allowed_transition` \
                 — either restore the edge in the whitelist or remove this CAS site",
                site.line_number, site.from_variant, site.to_variant, site.kind
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "boot_phase.rs CAS call sites are out of sync with \
         `fiscal_documents::allowed_transition`:\n{}",
        failures.join("\n")
    );
}

/// Hard guard for the operator W1 review criterion: grep on raw
/// `UPDATE fiscal_documents SET state` in `boot_phase.rs` must
/// return 0.  Pre-W1 there were 7; post-W1 there must be 0.
///
/// If a future refactor reintroduces a raw UPDATE (e.g. an
/// optimisation that bypasses the helper), this test fails — and
/// the new site MUST land via `transition_with_audit` OR
/// `fiscal_documents::transition_state` so the whitelist gate runs.
#[test]
fn no_raw_update_fiscal_documents_set_state() {
    let count = BOOT_PHASE_SRC
        .matches("UPDATE fiscal_documents SET state")
        .count();
    assert_eq!(
        count, 0,
        "Found {count} raw `UPDATE fiscal_documents SET state` site(s) in \
         boot_phase.rs — W1 contract violated.  Each raw UPDATE bypasses \
         the `fiscal_documents::allowed_transition` whitelist and breaks \
         the W1 invariant.  Promote to `transition_with_audit` (simple \
         sites) or to `fiscal_documents::transition_state` (sites with \
         supplementary writes ordered after CAS)."
    );
}

/// Sanity check: scanner extracts a synthetic helper call correctly.
#[test]
fn scanner_extracts_helper_call_site() {
    let synthetic = r#"
        let outcome = transition_with_audit(
            tx,
            doc_id,
            DocState::Sending,
            DocState::ErrorRetryable,
            "BOOT_FOO",
            Severity::Error,
            || serde_json::json!({}),
        ).await?;
    "#;
    let sites = scan_cas_call_sites(synthetic);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].kind, CasCallKind::HelperServiceLayer);
    assert_eq!(sites[0].from_variant, "Sending");
    assert_eq!(sites[0].to_variant, "ErrorRetryable");
}

/// Sanity check: scanner extracts a synthetic direct call correctly.
#[test]
fn scanner_extracts_direct_transition_state_call_site() {
    let synthetic = r#"
        let cas = fiscal_documents::transition_state(
            tx,
            doc_id,
            DocState::Sent,
            DocState::Kvt1,
        ).await?;
    "#;
    let sites = scan_cas_call_sites(synthetic);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].kind, CasCallKind::DirectRepositoryFn);
    assert_eq!(sites[0].from_variant, "Sent");
    assert_eq!(sites[0].to_variant, "Kvt1");
}
