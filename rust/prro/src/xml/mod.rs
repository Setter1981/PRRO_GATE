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
//! - `ShiftOpen` — `<C T="108">` + `DI=0` + `<O>` + `<E>`.
//! - `Sell` — `<C T="0">` containing `<P>` items + `<M>` payments +
//!   closing `<E FN N NO SM TS>`.
//! - `Return` — `<C T="1">` with the same body shape as `Sell`.
//! - `ZReport` — `<Z NO="...">` containing `<M>` per-payment-type
//!   totals + `<NC NI NO>` check counts.  Per WebCheck + DPS reality
//!   (`docs/webcheck_reverse/WEBCHECK_ANALYSIS.md:77`,
//!   `WebCheck/CreateDB.cs:624` doctype='80', DPS
//!   `Check.Type::ZREPORT=2` in `fiscal_server.proto:24`), `ZReport`
//!   IS the close-shift wire artifact.  There is no separate
//!   `ShiftClose` variant; do NOT add one.
//!
//! C1 (this commit) lands typed payload structs + the canonical-XML
//! builder + unit tests.  Unit-level acceptance only — attribute
//! alphabetical ordering, cp1251 byte mapping, escaping,
//! deterministic output.  Byte-equivalence against the
//! Python-captured goldens lands in C3.
//!
//! W4 first-round subset:  the typed payloads here mirror the Python
//! `_build_check` / `_build_z_report` shapes for the SUBSET we ship
//! goldens for in C2 — items/payments/closing-E for checks, and
//! payment-summaries+check-count for Z-reports.  The omitted optional
//! sections (per-item barcodes/excise/tax codes, per-item discounts,
//! check-level discounts, header/footer text lines, EPZ payment
//! attributes, TXS/IO/EPZ Z-report sections, tax_groups TX children
//! inside `<E>`) are intentional — C2 fixtures will be designed
//! against this subset, and tag/attr names ARE the Python names so a
//! future expansion is purely additive.

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
    /// `<O SM=...>`.  Opening cash sum (kopecks).
    pub opening_sum: i64,
}

/// SELL or RETURN check.  The CanonicalDoc variant picks
/// `<C T="0">` (Sell) vs `<C T="1">` (Return); body shape identical.
#[derive(Debug, Clone)]
pub struct CheckPayload {
    pub header: DocumentHeader,
    /// Per-FN local document number (`<DAT DI=...>`).
    pub local_number: u32,
    /// Line items emitted as `<P>` elements (W4 first-round subset:
    /// six required attrs only, no barcodes / excise / tax codes).
    pub items: Vec<CheckItem>,
    /// Payments emitted as `<M>` elements after items (W4 first-round
    /// subset: four required attrs only, no EPZ / change / rounding).
    pub payments: Vec<CheckPayment>,
    /// `<E SM=...>` total (kopecks).  The closing `<E>` element
    /// always emits `FN / N / NO / SM / TS` per ФСКО Table 23.
    pub total_sum: i64,
}

/// Single line item inside a SELL/RETURN check.  Mirrors Python
/// `_build_check`'s `<P>` element.
///
/// W4-Z1 (2026-05-27) extended the W4 minimal subset (C/N/NM/PRC/Q/SM)
/// with the optional attributes the Python `dps_xml.py` serializer
/// emits + the operator-confirmed pilot requirements per spec §2
/// (excise marks, UKTZED, barcode, TX/TX1 tax groups, per-item
/// discount/surcharge).  Empty `None` / `Vec::new()` → attribute /
/// child element NOT emitted (back-compat with existing minimal
/// goldens).
#[derive(Debug, Clone, Default)]
pub struct CheckItem {
    /// `<P C=...>`.  Item code (article SKU).
    pub code: String,
    /// `<P NM=...>`.  Item name.
    pub name: String,
    /// `<P PRC=...>`.  Per-unit price (kopecks).
    pub price: i64,
    /// `<P Q=...>`.  Quantity (thousandths, per Python).
    pub quantity: i64,
    /// `<P SM=...>`.  Line total (kopecks).
    pub sum: i64,

    // ─── W4-Z1 optional attributes ─────────────────────────────────

    /// `<P CD=...>`.  Barcode.  Per Python `dps_xml.py:197`:
    /// `p_attrs['CD'] = barcode`.  Omit if `None`.
    pub barcode: Option<String>,

    /// `<P CZD=...>`.  УКТЗЕД (HS) code.  Per Python `:200`:
    /// `p_attrs['CZD'] = uktzed`.  Omit if `None`.  NB this is NOT
    /// the same as `CD` (which is barcode).
    pub uktzed: Option<String>,

    /// `<P TX=...>`.  Primary tax group code.  `Some(0)` = звільнено
    /// (exempt), `Some(-1)` = не об'єкт ПДВ (not-VAT-object per
    /// `feedback_pdv_zero` memory), `Some(1..)` = regular tax group.
    /// `None` → attribute omitted (W4 refund variant per WebCheck
    /// `StringXML.cs:957` opertyp=-8 branch).  `i64` so the field can
    /// carry -1 even though canonical W3 DTO uses `u8` (W4-Z1 piece
    /// 7 conversion layer maps DTO→CheckItem with sentinel handling).
    pub tax_group_1: Option<i64>,

    /// `<P TX1=...>`.  Secondary tax group (dual-tax mode per
    /// Python `:204`).  Rare; only emit when operator explicitly
    /// configured the line as dual-tax.
    pub tax_group_2: Option<i64>,

    /// Excise marks (DSTU 9095-04 acc-codes).  Per WebCheck
    /// `StringXML.cs:1547 AAAAA()`: each stamp becomes a
    /// `<CA CA='{stamp}'></CA>` child of `<P>`.  Empty → `<P .../>`
    /// self-closing form (no children).  Per Python `:205-206`:
    /// `ca_xml = ''.join(_tag('CA', {'CA': m}) for m in excise_marks)`.
    pub excise_stamps: Vec<String>,

    /// Per-item discount.  When `Some`, a sibling `<D>` element is
    /// emitted IMMEDIATELY after this `<P>`, with `NI=` referencing
    /// the parent item's `N`.  Per spec §2 `<D>` per-item form + W4
    /// PR-A conversion-layer pinning.
    pub discount: Option<LineAdjustment>,

    /// Per-item surcharge.  Like `discount` but emits `<S>` (per
    /// Python `dps_xml.py:225` `xml_tag = 'S' if d_type ==
    /// 'EXTRA_CHARGE'`).
    pub surcharge: Option<LineAdjustment>,
}

/// Per-line adjustment (discount or surcharge) — shared shape for
/// `<D>` and `<S>` sibling elements after `<P>`.  Per Python
/// `dps_xml.py:226-244`.
#[derive(Debug, Clone)]
pub struct LineAdjustment {
    /// Sum in kopecks.  For percent-mode this is the resolved
    /// `round(item.sum * pr / 100)`; caller resolves percent at
    /// construction time (xml/ stays format-only).
    pub sum: i64,
    /// `TY=` mode flag.  `0` = VALUE (default), `1` = PERCENT.
    pub mode: AdjustmentMode,
    /// `PR=` percent value, formatted as `"{:.2}"`.  Only emitted
    /// when `mode == AdjustmentMode::Percent`.
    pub percent: Option<String>,
    /// `NM=` operator-readable name (e.g. "Знижка постійному
    /// клієнту").  Omit if empty.
    pub name: Option<String>,
    /// `DN=` privilege code.  Omit if empty.
    pub privilege: Option<String>,
    /// `TX=` tax code.  Omit if empty.  Drives ПДВ-зачот calculations.
    pub tax_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustmentMode {
    /// `TY="0"` — fixed-sum mode.  `sum` carries the value.
    Value,
    /// `TY="1"` — percent mode.  `percent` is the rate, `sum` is the
    /// resolved kopeck amount (caller pre-computes from item sum ×
    /// rate / 100).
    Percent,
}

/// Single payment inside a SELL/RETURN check.  Mirrors Python's
/// `<M>` shape with the W4 first-round attribute subset: `N / NM /
/// SM / T`.
#[derive(Debug, Clone)]
pub struct CheckPayment {
    /// `<M NM=...>`.  Payment-type display name (e.g. `"CASH"`).
    pub name: String,
    /// `<M SM=...>`.  Amount (kopecks).
    pub sum: i64,
    /// `<M T=...>`.  Type code: `"0"` cash, `"2"` non-cash.
    pub type_code: String,
}

/// Z_REPORT — shift-close fiscal document.  Per WebCheck + DPS
/// reality this DOUBLES as the CloseShift wire artifact; do NOT add
/// a separate `ShiftClosePayload`.
///
/// W4 first-round subset: `<Z NO=...>` containing `<M>` per-payment-
/// type sums and a single `<NC NI NO>` check-count footer.  Optional
/// `<TXS>` / `<IO>` / `<EPZ>` sections from the Python serializer
/// are deliberately omitted; C2 fixtures will be designed against
/// this subset.
#[derive(Debug, Clone)]
pub struct ZReportPayload {
    pub header: DocumentHeader,
    pub local_number: u32,
    /// Per-payment-type aggregate sums.  Each entry emits one
    /// `<M NM SMI SMO T>` element.
    pub payments: Vec<ZReportPaymentSum>,
    /// `<NC NI=... NO=...>` check counts.
    pub check_count: ZReportCheckCount,
}

#[derive(Debug, Clone)]
pub struct ZReportPaymentSum {
    /// `<M NM=...>`.  Payment-type name (`"CASH"`, `"CARD"`, ...).
    pub name: String,
    /// `<M SMI=...>`.  Inflow sum (kopecks).
    pub sum_in: i64,
    /// `<M SMO=...>`.  Outflow sum (kopecks).
    pub sum_out: i64,
    /// `<M T=...>`.  `"0"` cash, `"2"` non-cash.
    pub type_code: String,
}

#[derive(Debug, Clone)]
pub struct ZReportCheckCount {
    /// `<NC NI=...>`.  Sell-receipt count.
    pub sell_count: u32,
    /// `<NC NO=...>`.  Return-receipt count.
    pub return_count: u32,
}

/// Top-level discriminated wrapper consumed by `build_canonical_xml`.
#[derive(Debug, Clone)]
pub enum CanonicalDoc {
    ShiftOpen(ShiftOpenPayload),
    Sell(CheckPayload),
    Return(CheckPayload),
    /// CloseShift wire artifact (DPS Check.Type::ZREPORT=2; WebCheck
    /// CreateDB.cs:624 doctype='80').  No separate `ShiftClose`
    /// variant.
    ZReport(ZReportPayload),
}

// ─── Build entry point ────────────────────────────────────────────────

/// Errors a builder run can surface.
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

// ─── Per-doc emitters (literal port of Python `_build_*` helpers) ─────

fn emit_shift_open(p: &ShiftOpenPayload, out: &mut String) {
    let h = &p.header;
    open_rq(out, h);
    open_dat(out, h, "0");
    tag_attrs(out, "C", &[("T", "108")]);
    let opening = p.opening_sum.to_string();
    tag_attrs(out, "O", &[("N", "1"), ("SM", &opening), ("T", "0")]);
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
    let di = p.local_number.to_string();
    open_dat(out, h, &di);
    tag_attrs(out, "C", &[("T", c_type)]);

    // Item numbering N is shared across <P> + <M>; Python increments
    // a single `item_no` counter.  Mirror that.
    let mut item_no: u32 = 1;

    for it in &p.items {
        let p_item_n = item_no;
        let n = p_item_n.to_string();
        let prc = it.price.to_string();
        let q = it.quantity.to_string();
        let sm = it.sum.to_string();

        // Build `<P>` attribute list dynamically — Python sorts
        // alphabetically; we follow the same ordering convention
        // mirrored by `tag_attrs_sorted` (lazily here: hand-built
        // sorted form).  Required attrs: C, N, NM, PRC, Q, SM.
        // Optional: CD (barcode), CZD (uktzed), TX, TX1.
        //
        // Construction note: we collect (&str, &str) pairs.  String
        // arithmetic for optional Number fields (TX/TX1) needs owned
        // String buffers; bind them outside the tuple so they outlive
        // the slice.
        let tx_str = it.tax_group_1.map(|v| v.to_string());
        let tx1_str = it.tax_group_2.map(|v| v.to_string());

        let mut p_attrs: Vec<(&str, &str)> = Vec::with_capacity(10);
        p_attrs.push(("C", &it.code));
        if let Some(barcode) = &it.barcode {
            p_attrs.push(("CD", barcode));
        }
        if let Some(uktzed) = &it.uktzed {
            p_attrs.push(("CZD", uktzed));
        }
        p_attrs.push(("N", &n));
        p_attrs.push(("NM", &it.name));
        p_attrs.push(("PRC", &prc));
        p_attrs.push(("Q", &q));
        p_attrs.push(("SM", &sm));
        if let Some(tx) = tx_str.as_deref() {
            p_attrs.push(("TX", tx));
        }
        if let Some(tx1) = tx1_str.as_deref() {
            p_attrs.push(("TX1", tx1));
        }
        // Note: caller responsibility to keep attrs in alphabetical
        // order — we build in alpha order above (C, CD, CZD, N, NM,
        // PRC, Q, SM, TX, TX1).  Python sorts dict keys; we lock
        // ordering by construction.
        tag_attrs(out, "P", &p_attrs);

        // Emit `<CA>` excise stamp children (no children → `<P/>`
        // form via the existing `close(out, "P")` call below).
        // Python `:206` ca_xml = join over excise_marks.
        for stamp in &it.excise_stamps {
            tag_attrs(out, "CA", &[("CA", stamp)]);
            close(out, "CA");
        }
        close(out, "P");
        item_no += 1;

        // Sibling `<D>` / `<S>` elements immediately after the
        // parent `<P>`.  NI references parent_n.  Per Python
        // `:217-244` per-item-discount branch.
        if let Some(adj) = &it.discount {
            emit_line_adjustment(out, "D", adj, item_no, p_item_n);
            item_no += 1;
        }
        if let Some(adj) = &it.surcharge {
            emit_line_adjustment(out, "S", adj, item_no, p_item_n);
            item_no += 1;
        }
    }

    for pay in &p.payments {
        let n = item_no.to_string();
        let sm = pay.sum.to_string();
        // Python `_build_check` m_attrs (subset): N, NM, SM, T.
        tag_attrs(
            out,
            "M",
            &[
                ("N", &n),
                ("NM", &pay.name),
                ("SM", &sm),
                ("T", &pay.type_code),
            ],
        );
        close(out, "M");
        item_no += 1;
    }

    // Closing <E FN N NO SM TS> per ФСКО Table 23 / Python
    // `_build_e_element` no-tax-groups branch.
    let e_n = item_no.to_string();
    let e_no = p.local_number.to_string();
    let e_sm = p.total_sum.to_string();
    tag_attrs(
        out,
        "E",
        &[
            ("FN", &h.fiscal_number),
            ("N", &e_n),
            ("NO", &e_no),
            ("SM", &e_sm),
            ("TS", &h.ts_str),
        ],
    );
    close(out, "E");
    close(out, "C");
    tag_text(out, "TS", &h.ts_str);
    close(out, "DAT");
    tag_text(out, "MAC", &h.previous_hash);
    close(out, "RQ");
}

fn emit_z_report(p: &ZReportPayload, out: &mut String) {
    let h = &p.header;
    open_rq(out, h);
    let di = p.local_number.to_string();
    open_dat(out, h, &di);
    let zn = h.z_number.to_string();
    tag_attrs(out, "Z", &[("NO", &zn)]);

    // Z body — per-payment-type <M NM SMI SMO T>.  Python iterates
    // `sorted(payment_sums.keys())`; we mirror that by sorting the
    // caller-supplied vec by `name` so the wire output is
    // deterministic regardless of caller insertion order.
    let mut sorted_payments: Vec<&ZReportPaymentSum> = p.payments.iter().collect();
    sorted_payments.sort_by(|a, b| a.name.cmp(&b.name));
    for pay in sorted_payments {
        let smi = pay.sum_in.to_string();
        let smo = pay.sum_out.to_string();
        tag_attrs(
            out,
            "M",
            &[
                ("NM", &pay.name),
                ("SMI", &smi),
                ("SMO", &smo),
                ("T", &pay.type_code),
            ],
        );
        close(out, "M");
    }

    // Z body — <NC NI NO>.  Always emitted in W4 first-round shape
    // (Python emits if `check_count` is a dict, which it always is
    // in our typed payload).
    let ni = p.check_count.sell_count.to_string();
    let no = p.check_count.return_count.to_string();
    tag_attrs(out, "NC", &[("NI", &ni), ("NO", &no)]);
    close(out, "NC");

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

/// W4-Z1: emit a `<D>` (discount) or `<S>` (surcharge) sibling
/// element after a `<P>` line.  Per Python `dps_xml.py:217-244`
/// per-item-adjustment branch + spec §2 `<D>`/`<S>` shape.
///
/// `parent_n` — the sibling `<P>`'s `N` (sequence number) referenced
/// via `NI`.  `self_n` — this adjustment's own `N`.
fn emit_line_adjustment(
    out: &mut String,
    tag_name: &str, // "D" or "S"
    adj: &LineAdjustment,
    self_n: u32,
    parent_n: u32,
) {
    let self_n_str = self_n.to_string();
    let parent_n_str = parent_n.to_string();
    let sm_str = adj.sum.to_string();
    let (ty_str, percent_str) = match adj.mode {
        AdjustmentMode::Value => ("0", None),
        AdjustmentMode::Percent => ("1", adj.percent.as_deref()),
    };

    let mut attrs: Vec<(&str, &str)> = Vec::with_capacity(8);
    attrs.push(("N", &self_n_str));
    attrs.push(("NI", &parent_n_str));
    attrs.push(("SM", &sm_str));
    attrs.push(("TR", "0")); // per-item flag (TR=1 is check-level form)
    attrs.push(("TY", ty_str));
    if let Some(pr) = percent_str {
        attrs.push(("PR", pr));
    }
    if let Some(name) = adj.name.as_deref() {
        attrs.push(("NM", name));
    }
    if let Some(dn) = adj.privilege.as_deref() {
        attrs.push(("DN", dn));
    }
    if let Some(tx) = adj.tax_code.as_deref() {
        attrs.push(("TX", tx));
    }
    tag_attrs(out, tag_name, &attrs);
    close(out, tag_name);
}

/// Emit `<name>content</name>`.  Used for `<TS>` and `<MAC>`.
///
/// **Content is NOT XML-escaped** — mirrors Python `_tag` exactly:
/// the Python helper f-strings `{open_tag}{content}` directly, so
/// `&` / `<` / `>` in a hex-MAC or a YYYYMMDDHHMMSS timestamp are
/// passed through verbatim.  In practice TS is always digits and
/// MAC is always hex, so the question is academic — but we match
/// the oracle's behaviour for byte-equivalence.  If a future text
/// content needs escaping, callers should pre-escape and emit via
/// `tag_attrs` instead.
fn tag_text(out: &mut String, name: &str, content: &str) {
    out.push('<');
    out.push_str(name);
    out.push('>');
    out.push_str(content);
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

// ─── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> DocumentHeader {
        DocumentHeader::with_defaults("1234567890", "12345678", 7, "20260506120000", "deadbeef")
    }

    fn ascii_header() -> DocumentHeader {
        let mut h = header();
        // Override the cyrillic device name so the XML is pure-ASCII
        // for tests that assert string-shape via `from_utf8`.
        h.device_name = "ASCII_RRO".into();
        h
    }

    fn render_ascii(doc: &CanonicalDoc) -> String {
        let bytes = build_canonical_xml(doc).expect("build ok");
        String::from_utf8(bytes).expect("ASCII fixture must round-trip via UTF-8")
    }

    fn one_check_item() -> CheckItem {
        CheckItem {
            code: "ART-1".into(),
            name: "Apple".into(),
            price: 1500,
            quantity: 1000,
            sum: 1500,
            ..Default::default()
        }
    }

    fn one_cash_payment() -> CheckPayment {
        CheckPayment {
            name: "CASH".into(),
            sum: 1500,
            type_code: "0".into(),
        }
    }

    // ─── Attribute alphabetical ordering ──────────────────────────

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
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains(r#"<DAT DI="0" FN="1234567890" TN="12345678" V="1" ZN="7">"#),
            "DAT attrs out of alphabetical order: {s}"
        );
    }

    #[test]
    fn rq_attrs_alphabetical_ndv_prv_v() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        assert!(
            s.starts_with(r#"<RQ NDv="ASCII_RRO" PrV="1.1" V="1">"#),
            "RQ attrs out of order: {}",
            &s[..40.min(s.len())]
        );
    }

    // ─── Escaping ─────────────────────────────────────────────────

    #[test]
    fn attribute_values_escape_amp_quote_lt_gt_but_not_apostrophe() {
        let mut h = ascii_header();
        h.device_name = r#"<a&b>"c'd"#.into();
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        assert!(
            s.contains(r#"NDv="&lt;a&amp;b&gt;&quot;c'd""#),
            "attribute escape mismatch: {s}"
        );
        assert!(s.contains("c'd"), "apostrophe must NOT be escaped");
    }

    #[test]
    fn text_content_is_not_escaped_in_ts_or_mac() {
        // Python `_tag` interpolates `content` raw — any `&` or `<`
        // appearing in a TS / MAC string lands verbatim on the wire.
        // Practically TS is digits and MAC is hex, but the contract
        // matters because byte-equivalence depends on it.
        let mut h = ascii_header();
        h.previous_hash = "<dead&beef>".into();
        h.ts_str = "AB&CD".into(); // synthetic — TS is normally numeric
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: 0,
        });
        let s = render_ascii(&doc);
        // Both raw — no escape.
        assert!(
            s.contains("<MAC><dead&beef></MAC>"),
            "MAC content must pass through raw: {s}"
        );
        assert!(
            s.contains("<TS>AB&CD</TS>"),
            "TS content must pass through raw: {s}"
        );
    }

    // ─── Per-doc shape invariants ─────────────────────────────────

    #[test]
    fn shift_open_uses_c_t_108_and_di_zero() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: ascii_header(),
            opening_sum: 1234,
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
    fn sell_uses_c_t_0_with_p_m_e_body_in_python_shape() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 42,
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="0">"#), "SELL must be C T=0");
        assert!(s.contains(r#"DI="42""#), "SELL DI must equal local_number");
        // Per-item attrs alphabetical: C, N, NM, PRC, Q, SM
        assert!(
            s.contains(r#"<P C="ART-1" N="1" NM="Apple" PRC="1500" Q="1000" SM="1500">"#),
            "SELL <P> shape mismatch: {s}"
        );
        // Per-payment attrs alphabetical: N, NM, SM, T (item_no=2 because item_no=1 was the P)
        assert!(
            s.contains(r#"<M N="2" NM="CASH" SM="1500" T="0">"#),
            "SELL <M> shape mismatch: {s}"
        );
        // Closing <E FN N NO SM TS> with N=3 (after one P + one M)
        assert!(
            s.contains(r#"<E FN="1234567890" N="3" NO="42" SM="1500" TS="20260506120000">"#),
            "SELL <E> shape mismatch: {s}"
        );
    }

    #[test]
    fn return_uses_c_t_1_with_same_body_shape_as_sell() {
        let doc = CanonicalDoc::Return(CheckPayload {
            header: ascii_header(),
            local_number: 13,
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
        });
        let s = render_ascii(&doc);
        assert!(s.contains(r#"<C T="1">"#), "RETURN must be C T=1");
        assert!(s.contains(r#"DI="13""#));
        // Body shape identical to SELL.
        assert!(s.contains(r#"<P C="ART-1" N="1" NM="Apple" PRC="1500" Q="1000" SM="1500">"#));
        assert!(s.contains(r#"<M N="2" NM="CASH" SM="1500" T="0">"#));
    }

    #[test]
    fn z_report_uses_z_no_with_m_and_nc_body_and_doubles_as_close_shift() {
        // This test pins the contract that `ZReport` IS the
        // close-shift wire artifact (DPS Check.Type::ZREPORT=2;
        // WebCheck CreateDB.cs:624 doctype='80').  A future
        // contributor adding a separate ShiftClose variant will
        // need to update this test, which is the gate.
        let doc = CanonicalDoc::ZReport(ZReportPayload {
            header: ascii_header(),
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
        });
        let s = render_ascii(&doc);
        // <Z NO="..."> not <Z DC NO SM>.
        assert!(s.contains(r#"<Z NO="7">"#), "Z open shape mismatch: {s}");
        // <M NM SMI SMO T> alphabetical.
        assert!(
            s.contains(r#"<M NM="CASH" SMI="5000" SMO="0" T="0">"#),
            "Z<M> shape mismatch: {s}"
        );
        // <NC NI NO>.
        assert!(
            s.contains(r#"<NC NI="17" NO="2">"#),
            "Z<NC> shape mismatch: {s}"
        );
        // Z_REPORT must NOT contain a <C T=...> wrapper.
        assert!(!s.contains("<C "), "Z_REPORT must not emit a <C> tag: {s}");
    }

    #[test]
    fn z_report_payments_emit_in_sorted_name_order() {
        // Python iterates `sorted(payment_sums.keys())`; Rust must
        // match regardless of caller insertion order.  Pass in
        // CARD, then CASH; expect CASH < CARD lex order ⇒ CARD
        // emits first (because `'CARD' < 'CASH'`).
        let doc = CanonicalDoc::ZReport(ZReportPayload {
            header: ascii_header(),
            local_number: 1,
            payments: vec![
                ZReportPaymentSum {
                    name: "CASH".into(),
                    sum_in: 2,
                    sum_out: 0,
                    type_code: "0".into(),
                },
                ZReportPaymentSum {
                    name: "CARD".into(),
                    sum_in: 1,
                    sum_out: 0,
                    type_code: "2".into(),
                },
            ],
            check_count: ZReportCheckCount {
                sell_count: 0,
                return_count: 0,
            },
        });
        let s = render_ascii(&doc);
        let card_idx = s.find(r#"NM="CARD""#).expect("CARD present");
        let cash_idx = s.find(r#"NM="CASH""#).expect("CASH present");
        assert!(card_idx < cash_idx, "CARD < CASH lex order: {s}");
    }

    #[test]
    fn check_items_emit_in_input_order_with_alphabetised_attrs() {
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 1,
            items: vec![
                CheckItem {
                    code: "CODE-1".into(),
                    name: "Apple".into(),
                    price: 100,
                    quantity: 1000,
                    sum: 100,
                    ..Default::default()
                },
                CheckItem {
                    code: "CODE-2".into(),
                    name: "Banana".into(),
                    price: 100,
                    quantity: 1000,
                    sum: 100,
                    ..Default::default()
                },
            ],
            payments: vec![one_cash_payment()],
            total_sum: 200,
        });
        let s = render_ascii(&doc);
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
            items: vec![one_check_item()],
            payments: vec![one_cash_payment()],
            total_sum: 1500,
        });
        let a = build_canonical_xml(&doc).unwrap();
        let b = build_canonical_xml(&doc).unwrap();
        assert_eq!(a, b);
    }

    // ─── cp1251 byte mapping at the builder boundary ──────────────

    #[test]
    fn default_device_name_encodes_to_known_cp1251_bytes() {
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: header(),
            opening_sum: 0,
        });
        let bytes = build_canonical_xml(&doc).expect("default cyrillic must encode");
        let expected: &[u8] = &[0xCF, 0xD0, 0xCE, 0x5F, 0xEA, 0xE0, 0xF1, 0xE0];
        let _ = bytes
            .windows(expected.len())
            .position(|w| w == expected)
            .expect("ПРО_каса bytes must appear in cp1251 output");
    }

    #[test]
    fn ukrainian_name_encodes_via_cp1251() {
        // Item name with Ukrainian glyphs must round-trip; this is
        // the guard that catches cp1251 coverage drift.
        let doc = CanonicalDoc::Sell(CheckPayload {
            header: ascii_header(),
            local_number: 1,
            items: vec![CheckItem {
                code: "ART".into(),
                name: "Їжа".into(), // Ї (0xAF) + ж (0xE6) + а (0xE0)
                price: 100,
                quantity: 1000,
                sum: 100,
                ..Default::default()
            }],
            payments: vec![one_cash_payment()],
            total_sum: 100,
        });
        let bytes = build_canonical_xml(&doc).expect("ukrainian must encode");
        // Find the cp1251 bytes for "Їжа" inside the wire output.
        let needle: &[u8] = &[0xAF, 0xE6, 0xE0];
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "Ukrainian name must round-trip through cp1251"
        );
    }

    #[test]
    fn unmappable_char_in_device_name_returns_typed_error() {
        let mut h = ascii_header();
        h.device_name = "RRO😀".into();
        let doc = CanonicalDoc::ShiftOpen(ShiftOpenPayload {
            header: h,
            opening_sum: 0,
        });
        let err = build_canonical_xml(&doc).expect_err("emoji must be unmappable");
        assert!(
            matches!(err, XmlBuildError::Cp1251Unmappable(c, _) if c == '😀'),
            "expected typed Cp1251Unmappable for emoji, got {err:?}"
        );
    }
}
