//! Minimal canonical-XML builder for the 4 W4 doc types.
//!
//! Hand-written byte-stream builder targeting byte-identity against
//! the Python serializer at `src/prro_gateway/serializers/dps_xml.py`.
//! No XML crate dependency: a generic XML serializer would force us to
//! fight its attribute-ordering / namespace-emission / escaping
//! defaults to match the Python `_tag` helper exactly, and the
//! contract is per-tag byte-equivalence, not "pretty enough XML".
//!
//! W4 scope (revision 2026-05-06): four canonical-XML doc types per
//! ADR-M2-3 + W4 plan:
//!
//! - `ShiftOpen` — `<C T="108">` + DI=0
//! - `Sell`      — `<C T="0">`
//! - `Return`    — `<C T="1">`
//! - `ZReport`   — `<Z>` instead of `<C>`.  Per WebCheck +
//!   DPS reality (`docs/webcheck_reverse/WEBCHECK_ANALYSIS.md:77`,
//!   `WebCheck/CreateDB.cs:624` doctype='80', DPS
//!   `Check.Type::ZREPORT=2` in `fiscal_server.proto:24`),
//!   `ZReport` IS the close-shift wire artifact.  There is no
//!   separate `ShiftClose` variant; do NOT add one.
//!
//! C1 (this commit) lands typed payload structs + the canonical-XML
//! builder + unit tests.  Unit-level acceptance only — attribute
//! alphabetical ordering, cp1251 byte mapping, escaping,
//! deterministic output.  Byte-equivalence against the
//! Python-captured goldens lands in C3.

use std::fmt::Write as _;

pub mod cp1251;

// ─── Public typed payloads ────────────────────────────────────────────

/// Common header fields shared by every W4 doc type.  Carved out so
/// per-doc payloads don't repeat the device / FN / TN / TS shape.
#[derive(Debug, Clone)]
pub struct DocumentHeader {
    /// `<DAT FN=...>`.  Operator's fiscal number.
    pub fiscal_number: String,
    /// `<DAT TN=...>`.  Tax number (TIN/EDRPOU).
    pub tax_number: String,
    /// `<DAT ZN=...>`.  Z-report counter for the FN.
    pub z_number: u32,
    /// `<TS>` content.  Pre-formatted Kyiv-local `YYYYMMDDHHMMSS`
    /// string — the builder does NOT do timestamp formatting itself
    /// (that lives in the caller; see `dto::CheckEnvelope.date_time`
    /// commentary about Kyiv-local-as-epoch in `transports::dps`).
    pub ts_str: String,
    /// `<MAC>` content.  Hex-encoded previous-document hash.  Empty
    /// string for first-after-bootstrap.
    pub previous_hash: String,
    /// `<RQ NDv=...>`.  Default `"ПРО_каса"` (cp1251-encoded on
    /// output).  Held as `String` so callers can override per-pilot.
    pub device_name: String,
    /// `<RQ PrV=...>`.  Default `"1.1"`.
    pub device_version: String,
}

impl DocumentHeader {
    /// Default device-name + version mirror the Python serializer's
    /// kwargs: `device_name='ПРО_каса'`, `device_version='1.1'`.
    pub fn with_defaults(
        fiscal_number: impl Into<String>,
        tax_number: impl Into<String>,
        z_number: u32,
        ts_str: impl Into<String>,
        previous_hash: impl Into<String>,
    ) -> Self {
        Self {
            fiscal_number: fiscal_number.into(),
            tax_number: tax_number.into(),
            z_number,
            ts_str: ts_str.into(),
            previous_hash: previous_hash.into(),
            device_name: "ПРО_каса".into(),
            device_version: "1.1".into(),
        }
    }
}

/// SHIFT_OPEN — service receipt with `<C T="108">` and DI=0.
#[derive(Debug, Clone)]
pub struct ShiftOpenPayload {
    pub header: DocumentHeader,
    /// `<O SM=...>`.  Opening cash sum, string-formatted decimal
    /// (e.g. `"0"`, `"1234"`).
    pub opening_sum: String,
}

/// SELL or RETURN check.  The `kind` discriminator picks
/// `<C T="0">` (Sell) vs `<C T="1">` (Return); the rest of the body
/// shape is identical.
#[derive(Debug, Clone)]
pub struct CheckPayload {
    pub header: DocumentHeader,
    /// Per-FN local document number (`<DAT DI=...>`).  Sell + Return
    /// always emit a non-zero DI; SHIFT_OPEN forces DI=0.
    pub local_number: u32,
    /// `<S SM=...>` — total sum across the receipt, string-formatted
    /// decimal.
    pub total_sum: String,
    pub items: Vec<CheckItem>,
}

/// Single line item inside a SELL/RETURN check.  Mirrors the Python
/// `<P>` element shape; full ФСКО semantics (discounts, taxes,
/// excise) are out of scope for C1 — the items list is just enough
/// to exercise the canonical-XML byte stream.
#[derive(Debug, Clone)]
pub struct CheckItem {
    /// `<P C=...>`.  Item code (article SKU).
    pub code: String,
    /// `<P NM=...>`.  Item name.
    pub name: String,
    /// `<P PR=...>`.  Per-unit price, string-formatted decimal.
    pub price: String,
    /// `<P AM=...>`.  Quantity, string-formatted decimal.
    pub quantity: String,
    /// `<P CS=...>`.  Line total, string-formatted decimal.
    pub line_total: String,
}

/// Z_REPORT — shift-close fiscal document.  Per WebCheck + DPS
/// reality this DOUBLES as the CloseShift wire artifact; do NOT add
/// a separate `ShiftClosePayload`.
#[derive(Debug, Clone)]
pub struct ZReportPayload {
    pub header: DocumentHeader,
    pub local_number: u32,
    /// Z-report total turnover, string-formatted decimal.
    pub total_sum: String,
    /// Document count for the shift.
    pub doc_count: u32,
}

/// Top-level discriminated wrapper consumed by `build_canonical_xml`.
#[derive(Debug, Clone)]
pub enum CanonicalDoc {
    ShiftOpen(ShiftOpenPayload),
    Sell(CheckPayload),
    Return(CheckPayload),
    /// CloseShift wire artifact (DPS Check.Type::ZREPORT=2; WebCheck
    /// CreateDB.cs:624 doctype='80').  No separate `ShiftClose`
    /// variant — that would obscure the fact that there is one
    /// fiscal doc on the wire, not two.
    ZReport(ZReportPayload),
}

// ─── Build entry point ────────────────────────────────────────────────

/// Errors a builder run can surface.  Encoding errors are the only
/// recoverable category: cp1251 cannot represent every Unicode char,
/// and a surprise non-cp1251 string in a payload (e.g. emoji in a
/// product name) would silently produce broken bytes if we let the
/// encoder fall back to `?`.  Fail closed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum XmlBuildError {
    #[error("cp1251: cannot encode character {0:?} (U+{1:04X}) in payload field")]
    Cp1251Unmappable(char, u32),
}

/// Build the canonical wire XML for a `CanonicalDoc`.  Output is a
/// cp1251-encoded byte stream ready for signing / wire submission.
pub fn build_canonical_xml(doc: &CanonicalDoc) -> Result<Vec<u8>, XmlBuildError> {
    let mut out = String::new();
    match doc {
        CanonicalDoc::ShiftOpen(p) => emit_shift_open(p, &mut out),
        CanonicalDoc::Sell(p) => emit_check(p, "0", &mut out),
        CanonicalDoc::Return(p) => emit_check(p, "1", &mut out),
        CanonicalDoc::ZReport(p) => emit_z_report(p, &mut out),
    }
    cp1251::encode(&out)
}

// ─── Per-doc emitters (mirror Python _build_* helpers) ────────────────

fn emit_shift_open(p: &ShiftOpenPayload, out: &mut String) {
    let h = &p.header;
    open_rq(out, h);
    open_dat(out, h, "0");
    tag_attrs(out, "C", &[("T", "108")]);
    tag_attrs(out, "O", &[("N", "1"), ("SM", &p.opening_sum), ("T", "0")]);
    close(out, "O");
    tag_attrs(out, "E", &[("N", "2")]);
    close(out, "E");
    close(out, "C");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    tag_text(out, "MAC", &h.previous_hash);
    close(out, "RQ");
}

fn emit_check(p: &CheckPayload, c_type: &str, out: &mut String) {
    let h = &p.header;
    open_rq(out, h);
    open_dat(out, h, &p.local_number.to_string());
    tag_attrs(out, "C", &[("T", c_type)]);
    for (idx, it) in p.items.iter().enumerate() {
        let n = (idx + 1).to_string();
        tag_attrs(
            out,
            "P",
            &[
                ("AM", &it.quantity),
                ("C", &it.code),
                ("CS", &it.line_total),
                ("N", &n),
                ("NM", &it.name),
                ("PR", &it.price),
            ],
        );
        close(out, "P");
    }
    tag_attrs(out, "S", &[("SM", &p.total_sum)]);
    close(out, "S");
    close(out, "C");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    tag_text(out, "MAC", &h.previous_hash);
    close(out, "RQ");
}

fn emit_z_report(p: &ZReportPayload, out: &mut String) {
    let h = &p.header;
    open_rq(out, h);
    open_dat(out, h, &p.local_number.to_string());
    let zn = h.z_number.to_string();
    let dc = p.doc_count.to_string();
    tag_attrs(out, "Z", &[("DC", &dc), ("NO", &zn), ("SM", &p.total_sum)]);
    close(out, "Z");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    tag_text(out, "MAC", &h.previous_hash);
    close(out, "RQ");
}

// ─── Tag-emission primitives (mirror Python _tag/_xml_escape) ─────────

/// Emit `<RQ NDv="..." PrV="..." V="1">`.  Attribute order is
/// alphabetical (per `_tag` in Python), which for `NDv / PrV / V` is
/// already lex-correct (`N < P < V` in ASCII).
fn open_rq(out: &mut String, h: &DocumentHeader) {
    tag_attrs(
        out,
        "RQ",
        &[
            ("NDv", &h.device_name),
            ("PrV", &h.device_version),
            ("V", "1"),
        ],
    );
}

/// Emit `<DAT DI="..." FN="..." TN="..." V="1" ZN="...">`.  DI is
/// caller-supplied: `"0"` for SHIFT_OPEN, the local_number for
/// SELL/RETURN/Z_REPORT.  Order is alphabetical.
fn open_dat(out: &mut String, h: &DocumentHeader, di: &str) {
    let zn = h.z_number.to_string();
    tag_attrs(
        out,
        "DAT",
        &[
            ("DI", di),
            ("FN", &h.fiscal_number),
            ("TN", &h.tax_number),
            ("V", "1"),
            ("ZN", &zn),
        ],
    );
}

/// Open a tag with attributes, sorted alphabetically by name.
/// Mirrors the Python `_tag` helper's `sorted(attrs.items())`.
/// Caller is responsible for the matching `close()` — Python never
/// emits self-closing tags here, and neither do we.
fn tag_attrs(out: &mut String, name: &str, attrs: &[(&str, &str)]) {
    let mut sorted: Vec<(&str, &str)> = attrs.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    out.push('<');
    out.push_str(name);
    for (k, v) in sorted {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        push_escaped(out, v);
        out.push('"');
    }
    out.push('>');
}

/// Emit `<name>content</name>` where content is XML-escaped text.
/// Used for `<TS>` and `<MAC>` and any other attr-less tag whose
/// content is pure text.
fn tag_text(out: &mut String, name: &str, content: &str) {
    out.push('<');
    out.push_str(name);
    out.push('>');
    push_escaped(out, content);
    // close() emitted by caller for symmetry with tag_attrs callers
    // — but tag_text is self-contained: emit the closer here.
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// Emit `</name>`.
fn close(out: &mut String, name: &str) {
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// Mirror of Python `_xml_escape`:
///   `&` → `&amp;`, `"` → `&quot;`, `<` → `&lt;`, `>` → `&gt;`.
/// Note: `'` (apostrophe) is NOT escaped — Python doesn't escape it
/// either; this is a documented difference from generic XML
/// canonicalisation and a place a generic XML crate would diverge.
fn push_escaped(out: &mut String, s: &str) {
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push_str(c.encode_utf8(&mut buf)),
        }
    }
}

// `Write as _` is imported but no `write!` calls are made — left
// intentional so `format!`-free helpers stay obviously pure pushers.
#[allow(dead_code)]
fn _silence_write_import_warning() {
    let _ = String::new().write_str("");
}

// ─── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> DocumentHeader {
        DocumentHeader::with_defaults("1234567890", "12345678", 7, "20260506120000", "deadbeef")
    }

    fn ascii_header() -> DocumentHeader {
        let mut h = header();
        // Override the cyrillic device name so the XML is
        // pure-ASCII for tests that assert string-shape via
        // `from_utf8` (cp1251 cyrillic bytes are NOT valid UTF-8
        // and would fail that path).
        h.device_name = "ASCII_RRO".into();
        h
    }

    fn render_ascii(doc: &CanonicalDoc) -> String {
        let bytes = build_canonical_xml(doc).expect("build ok");
        String::from_utf8(bytes).expect("ASCII fixture must round-trip via UTF-8")
    }

    // ─── tag_attrs alphabetical ordering ──────────────────────────

    #[test]
    fn attributes_emitted_alphabetically_by_name() {
        let mut out = String::new();
        tag_attrs(&mut out, "T", &[("Z", "z"), ("A", "a"), ("M", "m")]);
        assert_eq!(out, r#"<T A="a" M="m" Z="z">"#);
    }

    #[test]
    fn dat_attrs_alphabetical_di_fn_tn_v_zn() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: "0".into(),
        });
        let s = render_ascii(&doc);
        let dat_idx = s.find("<DAT").expect("DAT tag");
        let dat_close = s[dat_idx..].find('>').unwrap() + dat_idx;
        let dat_open = &s[dat_idx..=dat_close];
        // Expect the exact attr order DI, FN, TN, V, ZN.
        assert!(
            dat_open.contains(r#" DI="0" FN="1234567890" TN="12345678" V="1" ZN="7""#),
            "DAT attrs out of alphabetical order: {dat_open}"
        );
    }

    #[test]
    fn rq_attrs_alphabetical_ndv_prv_v() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: "0".into(),
        });
        let s = render_ascii(&doc);
        // The full RQ open is the first chunk of the doc.
        assert!(
            s.starts_with(r#"<RQ NDv="ASCII_RRO" PrV="1.1" V="1">"#),
            "RQ attrs out of order: {}",
            &s[..40.min(s.len())]
        );
    }

    // ─── Escaping ─────────────────────────────────────────────────

    #[test]
    fn escapes_amp_quote_lt_gt_in_attrs_and_text() {
        let mut h = ascii_header();
        // Inject every escape-target into the device name so it
        // lands as an attribute value.
        h.device_name = r#"<a&b>"c'd"#.into();
        // Apostrophe MUST NOT be escaped (mirror Python).
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: "0".into(),
        });
        let s = render_ascii(&doc);
        // Every metacharacter except `'` must be escaped.
        assert!(
            s.contains(r#"NDv="&lt;a&amp;b&gt;&quot;c'd""#),
            "escape mismatch: {s}"
        );
        // Single quote stays raw.
        assert!(s.contains("c'd"), "apostrophe must NOT be escaped");
    }

    #[test]
    fn escapes_inside_macro_text_content() {
        // <MAC> is a text-content tag (tag_text path); a previous-
        // hash that happens to contain '<' / '&' must be escaped.
        let mut h = ascii_header();
        h.previous_hash = "<dead&beef>".into();
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: "0".into(),
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains("<MAC>&lt;dead&amp;beef&gt;</MAC>"),
            "MAC escape mismatch: {s}"
        );
    }

    // ─── Per-doc invariants ───────────────────────────────────────

    #[test]
    fn shift_open_uses_c_t_108_and_di_zero() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: "1234".into(),
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="108">"#), "SHIFT_OPEN must be C T=108");
        assert!(s.contains(r#"DI="0""#), "SHIFT_OPEN must force DI=0");
        assert!(
            s.contains(r#"<O N="1" SM="1234" T="0">"#),
            "SHIFT_OPEN <O> shape mismatch: {s}"
        );
        assert!(s.contains(r#"<E N="2">"#), "SHIFT_OPEN <E> missing");
    }

    #[test]
    fn sell_uses_c_t_0_and_local_number_in_di() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 42,
            total_sum: "9999".into(),
            items: vec![],
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="0">"#), "SELL must be C T=0");
        assert!(s.contains(r#"DI="42""#), "SELL DI must equal local_number");
        assert!(
            s.contains(r#"<S SM="9999">"#),
            "SELL <S> shape mismatch: {s}"
        );
    }

    #[test]
    fn return_uses_c_t_1_and_local_number_in_di() {
        let doc = CanonicalDoc::Return(CheckPayload {
            header: ascii_header(),
            local_number: 13,
            total_sum: "100".into(),
            items: vec![],
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="1">"#), "RETURN must be C T=1");
        assert!(s.contains(r#"DI="13""#));
    }

    #[test]
    fn z_report_uses_z_tag_and_doubles_as_close_shift_artifact() {
        // This test pins the contract that `ZReport` IS the
        // close-shift wire artifact (DPS Check.Type::ZREPORT=2;
        // WebCheck CreateDB.cs:624 doctype='80').  A future
        // contributor adding a separate ShiftClose variant will
        // need to update this test, which is the gate.
        let doc = CanonicalDoc::ZReport(ZReportPayload {
            header: ascii_header(),
            local_number: 100,
            total_sum: "5000".into(),
            doc_count: 17,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains(r#"<Z DC="17" NO="7" SM="5000">"#),
            "Z shape mismatch: {s}"
        );
        // Z_REPORT must NOT contain a <C T=...> wrapper — that's
        // the SELL/RETURN/SHIFT_OPEN shape.
        assert!(!s.contains("<C "), "Z_REPORT must not emit a <C> tag: {s}");
    }

    #[test]
    fn check_items_emit_in_input_order_with_alphabetised_attrs() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 1,
            total_sum: "200".into(),
            items: vec![
                CheckItem {
                    code: "CODE-1".into(),
                    name: "Apple".into(),
                    price: "100".into(),
                    quantity: "1".into(),
                    line_total: "100".into(),
                },
                CheckItem {
                    code: "CODE-2".into(),
                    name: "Banana".into(),
                    price: "100".into(),
                    quantity: "1".into(),
                    line_total: "100".into(),
                },
            ],
        });
        let s = render_ascii(&doc);
        // Per-item attr order: AM, C, CS, N, NM, PR (alphabetical).
        assert!(
            s.contains(r#"<P AM="1" C="CODE-1" CS="100" N="1" NM="Apple" PR="100">"#),
            "first item shape mismatch: {s}"
        );
        // Item order preserved (N=1 before N=2).
        let idx1 = s.find("CODE-1").expect("first item present");
        let idx2 = s.find("CODE-2").expect("second item present");
        assert!(idx1 < idx2, "items must emit in input order");
    }

    // ─── Determinism ──────────────────────────────────────────────

    #[test]
    fn build_is_deterministic_for_same_input() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 1,
            total_sum: "1".into(),
            items: vec![],
        });
        let a = build_canonical_xml(&doc).unwrap();
        let b = build_canonical_xml(&doc).unwrap();
        assert_eq!(a, b);
    }

    // ─── cp1251 byte mapping at the builder boundary ──────────────

    #[test]
    fn default_device_name_encodes_to_known_cp1251_bytes() {
        // Default device_name = "ПРО_каса".  cp1251 mapping:
        //   П=0xCF, Р=0xD0, О=0xCE, _=0x5F, к=0xEA, а=0xE0,
        //   с=0xF1, а=0xE0  → 8 bytes total.
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: header(),
            opening_sum: "0".into(),
        });
        let bytes = build_canonical_xml(&doc).expect("default cyrillic must encode");
        let expected: &[u8] = &[0xCF, 0xD0, 0xCE, 0x5F, 0xEA, 0xE0, 0xF1, 0xE0];
        let needle_idx = bytes
            .windows(expected.len())
            .position(|w| w == expected)
            .expect("ПРО_каса bytes must appear in cp1251 output");
        let _ = needle_idx;
    }

    #[test]
    fn unmappable_char_in_device_name_returns_typed_error() {
        let mut h = ascii_header();
        h.device_name = "RRO😀".into(); // emoji not in cp1251
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: "0".into(),
        });
        let err = build_canonical_xml(&doc).expect_err("emoji must be unmappable");
        assert!(
            matches!(err, XmlBuildError::Cp1251Unmappable(c, _) if c == '😀'),
            "expected typed Cp1251Unmappable for emoji, got {err:?}"
        );
    }
}
