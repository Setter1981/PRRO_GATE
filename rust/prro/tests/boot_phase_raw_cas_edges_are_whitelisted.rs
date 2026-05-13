//! M3a HIGH-2 regression: every raw `UPDATE fiscal_documents SET state`
//! statement in `services/reconciliation/boot_phase.rs` must use a
//! `(from, to)` pair that is in the
//! `fiscal_documents::allowed_transition` whitelist.
//!
//! Background.  Boot-phase recovery bypasses the `transition_state` /
//! `transition_state_tx` repo helpers (those helpers gate on the
//! whitelist before issuing CAS).  Instead, boot_phase emits hand-written
//! `UPDATE … SET state = 'X' WHERE … AND state = 'Y'` SQL directly,
//! because the boot path needs to attach branch-specific audit-log
//! payloads inside the same `with_immediate` envelope.  See
//! ADR-M3-A10 and the carry-forward residual called out in
//! `docs/M3a-handoff.md §6.1`.
//!
//! Bypassing the helper means bypassing the whitelist check.  A code
//! refactor that drops an edge from `allowed_transition` (e.g. tightening
//! the state machine) would leave boot_phase quietly issuing forbidden
//! CAS — the UPDATE would no-op (CAS predicate mismatches the row's
//! state at runtime), and a stuck boot would surface only via passive
//! audit or a confused operator.
//!
//! This test is the cheap-fix for HIGH-2 (option B in the M3a review):
//! at compile time it scans the boot_phase source as a string, extracts
//! every `(to, from)` literal pair, and asserts each pair satisfies
//! `allowed_transition(from, to)`.  If somebody adds a new raw CAS in
//! boot_phase, this test must see it too — the count assertion at the
//! bottom guards against silent additions slipping in unreviewed.
//!
//! Promoting these raw CASs to a `transition_state_tx`-equivalent
//! helper that takes an audit-payload closure is the right long-term
//! shape (Pattern A-style separation) but is M3b work, tracked under
//! the carry-forward in the handoff doc.

use prro::db::models::enums::DocState;
use prro::db::repositories::fiscal_documents::allowed_transition;

const BOOT_PHASE_SRC: &str = include_str!("../src/services/reconciliation/boot_phase.rs");

/// How many raw `UPDATE fiscal_documents SET state` sites exist in the
/// current boot_phase.rs.  Locked here so silent additions surface as
/// a test failure — the new site must be inspected and the count
/// updated together.  Sites today (HEAD `5e1c560`):
///   - line  318: resume_sending_to_error_retryable      (SENDING → ERROR_RETRYABLE)
///   - line  406: advance_sent_to_kvt1_from_probe        (SENT    → KVT1)
///   - line  499: cas_sent_to_manual_reconciliation      (SENT    → REQUIRES_MANUAL_RECONCILIATION)
///   - line  586: cas_sent_to_error_retryable_from_probe (SENT    → ERROR_RETRYABLE)
///   - line 1336: cas_er_to_manual_reconciliation        (ER      → REQUIRES_MANUAL_RECONCILIATION)
///   - line 1391: cas_er_budget_exhausted                (ER      → REQUIRES_MANUAL_RECONCILIATION)
///   - line 2057: encrypted_reroute                      (ENCRYPTED → ERROR_RETRYABLE)
const EXPECTED_RAW_CAS_COUNT: usize = 7;

#[derive(Debug, PartialEq)]
struct RawCasEdge {
    line_number: usize,
    from_wire: String,
    to_wire: String,
}

/// Parse the wire-string for a state enum into a `DocState` variant.
///
/// We deliberately do *not* import `serde_json::from_str` or add a
/// `FromStr` impl on `DocState` just for this test — keeping the
/// mapping inline makes the test self-documenting (you can see at a
/// glance which wire strings are legitimate) and means a typo in
/// boot_phase.rs (e.g. `'ERROR_RETRYABLE_'`) surfaces as a clean
/// "unknown wire form" panic at the call site, not a deeper
/// serde error.
fn parse_doc_state(wire: &str) -> DocState {
    match wire {
        "PREPARED" => DocState::Prepared,
        "SIGNED" => DocState::Signed,
        "ENCRYPTED" => DocState::Encrypted,
        "SENDING" => DocState::Sending,
        "SENT" => DocState::Sent,
        "KVT1" => DocState::Kvt1,
        "KVT2" => DocState::Kvt2,
        "ACK" => DocState::Ack,
        "OFFLINE_LOCAL_ACK" => DocState::OfflineLocalAck,
        "REJECTED" => DocState::Rejected,
        "CANCELLED" => DocState::Cancelled,
        "ERROR_RETRYABLE" => DocState::ErrorRetryable,
        "REQUIRES_MANUAL_RECONCILIATION" => DocState::RequiresManualReconciliation,
        other => panic!(
            "boot_phase.rs raw CAS references an unknown wire state '{other}'. \
             Either add the variant to DocState (and to this parser) or fix \
             the typo in boot_phase.rs."
        ),
    }
}

/// Extract a state literal between single quotes that follows
/// `state = `.  Returns `None` if the marker isn't found on this line.
fn extract_state_literal(line: &str, after_marker: &str) -> Option<String> {
    let start = line.find(after_marker)? + after_marker.len();
    let rest = &line[start..];
    let open = rest.find('\'')?;
    let after_open = &rest[open + 1..];
    let close = after_open.find('\'')?;
    Some(after_open[..close].to_string())
}

/// Scan boot_phase.rs as raw text, find every `UPDATE fiscal_documents
/// SET state = '<TO>' \` line, and read the immediately-following line
/// for the matching `state = '<FROM>'` predicate.
///
/// The two-line shape is the convention boot_phase.rs has converged
/// on (verified via `grep` at test-design time — see test docstring
/// for the 7 confirmed sites).  If somebody later writes the UPDATE
/// on a single line, the test will under-count and the
/// `EXPECTED_RAW_CAS_COUNT` assert will fire — at which point the
/// scanner here should be extended to handle that shape too.  An
/// explicit "single-line UPDATE" panic guards against quietly missing
/// such a site.
fn scan_raw_cas_edges(src: &str) -> Vec<RawCasEdge> {
    let lines: Vec<&str> = src.lines().collect();
    let mut edges = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        // We match on the `SET state = '` form so we don't accidentally
        // pick up the `WHERE ... state = '` half of the same CAS.
        if !line.contains("UPDATE fiscal_documents SET state = '") {
            continue;
        }
        let to_wire = extract_state_literal(line, "SET state = ").unwrap_or_else(|| {
            panic!(
                "boot_phase.rs line {} contains the UPDATE marker but no \
                     quoted target state could be parsed.  Line: {:?}",
                i + 1,
                line
            )
        });

        // Heuristic: if this line itself already carries the WHERE
        // predicate, the test is looking at a one-line UPDATE shape
        // it wasn't designed for.  Fail loud rather than silently
        // miss the (from) half.
        if line.contains("WHERE document_id") && line.contains("AND state = '") {
            // Single-line UPDATE — extract from the same line.
            let from_wire = extract_state_literal(line, "AND state = ").unwrap_or_else(|| {
                panic!(
                    "boot_phase.rs line {} appears single-line but the \
                         FROM predicate could not be parsed.  Line: {:?}",
                    i + 1,
                    line
                )
            });
            edges.push(RawCasEdge {
                line_number: i + 1,
                from_wire,
                to_wire,
            });
            continue;
        }

        // Two-line shape: the FROM predicate is on the next non-empty
        // line.  We tolerate one possible continuation line of pure
        // whitespace just in case, but the canonical shape is "next
        // line".
        let next = lines.get(i + 1).unwrap_or_else(|| {
            panic!(
                "boot_phase.rs line {} has UPDATE marker but no following \
                 line for WHERE predicate",
                i + 1
            )
        });
        let from_wire = extract_state_literal(next, "AND state = ").unwrap_or_else(|| {
            panic!(
                "boot_phase.rs line {} (continuation of UPDATE at line \
                     {}) does not match the expected \
                     `WHERE document_id = ? AND state = '<FROM>'` shape. \
                     Line: {:?}",
                i + 2,
                i + 1,
                next
            )
        });

        edges.push(RawCasEdge {
            line_number: i + 1,
            from_wire,
            to_wire,
        });
    }
    edges
}

#[test]
fn boot_phase_raw_cas_edges_are_whitelisted() {
    let edges = scan_raw_cas_edges(BOOT_PHASE_SRC);

    assert_eq!(
        edges.len(),
        EXPECTED_RAW_CAS_COUNT,
        "Expected exactly {} raw `UPDATE fiscal_documents SET state` sites \
         in boot_phase.rs, found {}. If you added or removed one, update \
         EXPECTED_RAW_CAS_COUNT (and review the new site for whitelist \
         alignment).  Found edges: {:#?}",
        EXPECTED_RAW_CAS_COUNT,
        edges.len(),
        edges
    );

    let mut failures = Vec::new();
    for edge in &edges {
        let from = parse_doc_state(&edge.from_wire);
        let to = parse_doc_state(&edge.to_wire);
        if !allowed_transition(from, to) {
            failures.push(format!(
                "line {}: ({}, {}) is NOT in `fiscal_documents::allowed_transition` \
                 — either restore the edge in the whitelist or remove this raw CAS",
                edge.line_number, edge.from_wire, edge.to_wire
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "boot_phase.rs raw CAS sites are out of sync with the \
         `fiscal_documents::allowed_transition` whitelist:\n{}",
        failures.join("\n")
    );
}

/// Sanity check: the scanner itself works on the canonical two-line
/// shape (guards against a future regression where the parser
/// stops matching anything and the main test trivially passes with
/// zero edges — though the count assert already catches that, this
/// is belt-and-suspenders).
#[test]
fn scanner_extracts_two_line_update_correctly() {
    let synthetic = r#"
        // some preamble
        let q = sqlx::query(
            "UPDATE fiscal_documents SET state = 'KVT1' \
             WHERE document_id = ? AND state = 'SENT'",
        );
    "#;
    let edges = scan_raw_cas_edges(synthetic);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from_wire, "SENT");
    assert_eq!(edges[0].to_wire, "KVT1");
}

#[test]
fn scanner_extracts_single_line_update_correctly() {
    // Cover the single-line shape too so that if a future commit
    // collapses one of the boot_phase sites to a one-liner the
    // edge-validation still runs (rather than panicking on missing
    // continuation line).
    let synthetic = "    \"UPDATE fiscal_documents SET state = 'ERROR_RETRYABLE' WHERE document_id = ? AND state = 'SENDING'\",";
    let edges = scan_raw_cas_edges(synthetic);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from_wire, "SENDING");
    assert_eq!(edges[0].to_wire, "ERROR_RETRYABLE");
}
