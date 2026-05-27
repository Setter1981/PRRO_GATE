//! W4-C3 byte-equivalence harness.
//!
//! Reads each frozen `.bin` under `tests/goldens/`, runs the Rust
//! `prro::xml::build_canonical_xml` (or the matching artefact-builder)
//! against the same hard-coded payload `regenerate.py` ships, and
//! asserts byte equality.  The Python serializer at
//! `src/prro_gateway/serializers/dps_xml.py` is the byte-oracle (per
//! ADR-M2-3 + W4-C0); Rust is the candidate.
//!
//! No skip-if-missing semantics: a missing `.bin` is a hard failure
//! with the canonical "run regenerate.py" remediation message.  CI
//! must catch a mistakenly-deleted golden as a real test failure,
//! NOT pass silently.
//!
//! On byte mismatch the harness emits a hex dump of the first up-to
//! 64 differing bytes (per ADR-M2-3 §"hex diff readable in CI logs").
//! That gives a reviewer a diagnostic before they even open the
//! `.bin` file.

use std::path::{Path, PathBuf};

use prro::xml::{
    build_canonical_xml, CanonicalDoc, CheckItem, CheckPayload, CheckPayment, DocumentHeader,
    ShiftOpenPayload, ZReportCheckCount, ZReportPayload, ZReportPaymentSum,
};

// ─── Fixture-payload mirrors of regenerate.py ────────────────────────

/// Canonical fixture header.  MUST match `regenerate.py`'s `FN`,
/// `TN`, `Z_NUMBER`, `BUSINESS_TS`-derived ts_str, `PREVIOUS_HASH`.
fn fixture_header() -> DocumentHeader {
    DocumentHeader::with_defaults(
        "1234567890",
        "12345678",
        7,
        // Kyiv-local rendering of `datetime(2026, 5, 6, 9, 0, 0,
        // tzinfo=UTC)` — UTC+3 EEST (summer half) → 12:00 → wire TS
        // "20260506120000".  See goldens/README.md "Determinism" +
        // docs/M2-goldens-capture.md.
        "20260506120000",
        "deadbeef",
    )
}

/// SHIFT_OPEN — opening_sum = 0 (regenerate.py).
fn shift_open_doc() -> CanonicalDoc {
    CanonicalDoc::ShiftOpen(ShiftOpenPayload {
        header: fixture_header(),
        opening_sum: 0,
    })
}

/// One Apple item, one cash payment (used by both sell + return).
fn check_payload(local_number: u32) -> CheckPayload {
    CheckPayload {
        header: fixture_header(),
        local_number,
        items: vec![CheckItem {
            code: "ART-1".into(),
            name: "Apple".into(),
            price: 1500,
            quantity: 1000,
            sum: 1500,
            ..Default::default()
        }],
        payments: vec![CheckPayment {
            name: "CASH".into(),
            sum: 1500,
            type_code: "0".into(),
            ..Default::default()
        }],
        total_sum: 1500,
        check_level_adjustments: Vec::new(),
        header_lines: Vec::new(),
        footer_lines: Vec::new(),
        tax_summaries: Vec::new(),
    }
}

fn sell_doc() -> CanonicalDoc {
    CanonicalDoc::Sell(check_payload(42))
}

fn return_doc() -> CanonicalDoc {
    CanonicalDoc::Return(check_payload(13))
}

fn z_report_doc() -> CanonicalDoc {
    CanonicalDoc::ZReport(ZReportPayload {
        header: fixture_header(),
        local_number: 100,
        payments: vec![ZReportPaymentSum {
            name: "CASH".into(),
            sum_in: 5000,
            sum_out: 0,
            type_code: "0".into(),
        }],
        check_count: ZReportCheckCount {
            sell_count: 17,
            return_count: 2,
        },
    })
}

// ─── Harness primitives ──────────────────────────────────────────────

/// Resolve a fixture path under `tests/goldens/` from the repo's
/// `tests/` cwd.  `CARGO_MANIFEST_DIR` points at `rust/prro/`, so
/// the goldens live one level deeper.
fn golden_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
        .join(rel)
}

/// Read the frozen golden bytes.  Missing file → hard panic with
/// the canonical "run regenerate.py" remediation message.  CI must
/// surface this as a real failure, not skip silently.
fn read_golden(rel: &str) -> Vec<u8> {
    let path = golden_path(rel);
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden fixture {rel}: {e}\n\n\
             To regenerate fixtures from the Python oracle, run:\n  \
             python3 rust/prro/tests/goldens/regenerate.py\n\n\
             Re-capture is a deliberate-spec-change action, NOT a \
             regression-fix action.  See docs/M2-goldens-capture.md \
             for the operator procedure + reviewer checklist."
        );
    })
}

/// Assert two byte slices are identical.  On mismatch emit an
/// ADR-M2-3-shaped hex dump diagnostic: scan the FULL overlap for
/// the first divergence (not just the first 64 bytes — a regression
/// at byte 100 with equal length must be reported precisely, not
/// misclassified as "length only"), then show a 64-byte window
/// CENTRED on the first-diff offset (±32 bytes) so the dump always
/// covers the divergence point.
fn assert_byte_equal(rel: &str, actual: &[u8], expected: &[u8]) {
    if actual == expected {
        return;
    }
    // Scan the full overlap (not min(64)) — drift at any offset
    // must surface as "first diff at byte N", not as a false
    // "length only" verdict.
    let overlap = actual.len().min(expected.len());
    let first_diff = (0..overlap).find(|&i| actual[i] != expected[i]);

    let mut msg = String::new();
    msg.push_str(&format!(
        "byte-equivalence FAILED for {rel}\n  rust_len={} python_len={}\n",
        actual.len(),
        expected.len()
    ));

    // Window centred on the first diff (±32 bytes, total 64).  When
    // the divergence is purely in length (one slice is a prefix of
    // the other), `first_diff` is None and we anchor the window at
    // the truncation boundary.
    let window_size = 64usize;
    let half = window_size / 2;
    let anchor = first_diff.unwrap_or(overlap);
    let start = anchor.saturating_sub(half);

    match first_diff {
        Some(idx) => {
            msg.push_str(&format!(
                "  first diff at byte {idx}\n  window: bytes {start}..\n"
            ));
        }
        None => {
            msg.push_str(&format!(
                "  first {overlap} bytes match — divergence is in length only \
                 (truncation or appended bytes)\n  window: bytes {start}..\n"
            ));
        }
    }
    let rust_window: &[u8] = &actual[start.min(actual.len())..];
    let py_window: &[u8] = &expected[start.min(expected.len())..];
    msg.push_str("  rust    (window, up to 64 bytes):");
    for &b in rust_window.iter().take(window_size) {
        msg.push_str(&format!(" {b:02x}"));
    }
    msg.push('\n');
    msg.push_str("  python  (window, up to 64 bytes):");
    for &b in py_window.iter().take(window_size) {
        msg.push_str(&format!(" {b:02x}"));
    }
    msg.push('\n');

    panic!("{msg}");
}

/// Build the Rust canonical-XML output and diff it against the
/// frozen Python-captured golden at `rel`.
fn assert_xml_golden(rel: &str, doc: &CanonicalDoc) {
    let rust_bytes = build_canonical_xml(doc).expect("build_canonical_xml must succeed");
    let py_bytes = read_golden(rel);
    assert_byte_equal(rel, &rust_bytes, &py_bytes);
}

// ─── Per-doc byte-equivalence tests ──────────────────────────────────

#[test]
fn xml_shift_open_byte_equivalent() {
    assert_xml_golden("xml/shift_open.bin", &shift_open_doc());
}

#[test]
fn xml_sell_byte_equivalent() {
    assert_xml_golden("xml/sell.bin", &sell_doc());
}

#[test]
fn xml_return_byte_equivalent() {
    assert_xml_golden("xml/return.bin", &return_doc());
}

/// Z_REPORT IS the close-shift wire artifact (DPS Check.Type::ZREPORT=2;
/// WebCheck CreateDB.cs:624 doctype='80'; W4-C0 contract).  This test
/// pins the bytes; no separate xml/shift_close.bin should exist.
#[test]
fn xml_z_report_byte_equivalent_doubles_as_close_shift() {
    assert_xml_golden("xml/z_report.bin", &z_report_doc());
}

// ─── CMS deterministic prefix ────────────────────────────────────────

/// Per ADR-M2-3, CMS goldens split into:
///   (a) deterministic prefix — XML-to-be-signed, pinned byte-equal,
///   (b) signature shape — parsed + verified, NOT byte-compared.
/// W4 first round pins (a) only.  The simplest viable
/// "XML-to-be-signed" is the SELL fixture, so this golden's bytes
/// equal `xml/sell.bin` byte-for-byte; this test surfaces drift
/// between the two would-be-equal fixtures (e.g. someone edits one
/// without re-capturing the other).
#[test]
fn cms_deterministic_prefix_equals_sell_xml() {
    let prefix = read_golden("cms/deterministic_prefix.bin");
    let sell = read_golden("xml/sell.bin");
    assert_byte_equal(
        "cms/deterministic_prefix.bin vs xml/sell.bin",
        &prefix,
        &sell,
    );
}

// ─── prevhash seed ───────────────────────────────────────────────────

/// First-after-bootstrap previous_hash seed: 32 zero bytes.  Pins
/// the size + content so a future contributor doesn't accidentally
/// fill in `Utc::now()` or some other drift source as a "seed".
#[test]
fn prevhash_seed_is_32_zero_bytes() {
    let seed = read_golden("prevhash/seed.bin");
    assert_eq!(seed.len(), 32, "prevhash seed must be exactly 32 bytes");
    assert!(
        seed.iter().all(|&b| b == 0),
        "prevhash seed must be all-zero placeholder; got {seed:?}"
    );
}

// ─── Negative test: harness fails loudly on missing fixture ─────────

/// Direct test of `read_golden`'s error message — proves the
/// no-skip-if-missing contract.  We can't easily delete a real
/// fixture inside a test, so we probe a deliberately-absent path.
#[test]
#[should_panic(expected = "missing golden fixture")]
fn missing_golden_panics_with_remediation_message() {
    let _ = read_golden("xml/this_does_not_exist.bin");
}
