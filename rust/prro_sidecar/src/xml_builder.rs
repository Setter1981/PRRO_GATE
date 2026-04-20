//! Byte-compatible port of `dps_xml.py` — canonical JSON → cp1251 XML.
//!
//! Covers all DPS ФСКО operation types:
//!   SHIFT_OPEN, SELL, RETURN, SERVICE_IN, SERVICE_OUT, CASH_WITHDRAWAL, Z_REPORT
//!
//! Timestamps are converted from UTC to Kyiv time (fixed UTC+3, effective 2022-02-26).
//! Attributes are sorted alphabetically, matching Python `sorted(attrs.items())`.
//! Rounding uses banker's rounding (round-half-to-even) to match Python `round()`.

use std::collections::HashMap;

use time::{OffsetDateTime, UtcOffset, format_description, format_description::well_known::Rfc3339};

use crate::cp1251::encode_cp1251;
use crate::errors::SidecarError;
use crate::input::{CanonicalCommand, OperationType};

// ── Public types ──────────────────────────────────────────────────────────────

/// Tax group configuration — mirrors Python `TaxGroup` / `tax_groups` dict values.
#[derive(Debug, Clone)]
pub struct TaxGroup {
    pub tax_rate:        f64,
    pub additional_rate: f64,
    pub tax_algorithm:   i32,
    pub tax_type:        i32,
}

/// Per-request context passed alongside the canonical command.
pub struct BuildContext<'a> {
    pub local_number:   i64,
    pub z_number:       i64,
    pub previous_hash:  &'a str,
    pub tax_number:     &'a str,
    pub tax_groups:     Option<&'a HashMap<String, TaxGroup>>,
    pub device_name:    &'a str,
    pub device_version: &'a str,
}

impl Default for BuildContext<'_> {
    fn default() -> Self {
        BuildContext {
            local_number:   0,
            z_number:       0,
            previous_hash:  "",
            tax_number:     "",
            tax_groups:     None,
            device_name:    "ПРО_каса",
            device_version: "1.1",
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Build cp1251-encoded XML from a canonical command + build context.
pub fn build(cmd: &CanonicalCommand, ctx: &BuildContext) -> Result<Vec<u8>, SidecarError> {
    let xml = build_xml(cmd, ctx)?;
    encode_cp1251(&xml)
}

// ── Router ────────────────────────────────────────────────────────────────────

fn build_xml(cmd: &CanonicalCommand, ctx: &BuildContext) -> Result<String, SidecarError> {
    let ts_str = format_ts(&cmd.business_ts)?;
    let rq_attrs = vec![
        ("NDv", ctx.device_name.to_string()),
        ("PrV", ctx.device_version.to_string()),
        ("V",   "1".to_string()),
    ];

    match &cmd.operation_type {
        OperationType::ShiftOpen => Ok(build_shift_open(cmd, ctx, &ts_str, &rq_attrs)),
        OperationType::Sell => Ok(build_check(cmd, ctx, &ts_str, &rq_attrs, "0")),
        OperationType::Return => Ok(build_check(cmd, ctx, &ts_str, &rq_attrs, "1")),
        OperationType::ServiceIn => Ok(build_service(cmd, ctx, &ts_str, &rq_attrs, "IN")),
        OperationType::ServiceOut => Ok(build_service(cmd, ctx, &ts_str, &rq_attrs, "OUT")),
        OperationType::CashWithdrawal => Ok(build_cash_withdrawal(cmd, ctx, &ts_str, &rq_attrs)),
        OperationType::ZReport => Ok(build_z_report(cmd, ctx, &ts_str, &rq_attrs)),
        other => Err(SidecarError::BadRequest(format!(
            "DPS XML serializer does not support {:?}",
            other
        ))),
    }
}

// ── Timestamp ─────────────────────────────────────────────────────────────────

/// Parse ISO-8601 UTC timestamp and format as `YYYYMMDDHHMMSS` in Kyiv time (UTC+3).
///
/// Ukraine has been on permanent UTC+3 since 2022-02-26.
/// Returns `BadRequest` if the string cannot be parsed — the DPS MAC chain
/// depends on deterministic document content; a silent clock substitution
/// would break replay determinism and produce fiscally incorrect timestamps.
fn format_ts(business_ts: &str) -> Result<String, SidecarError> {
    let kyiv = UtcOffset::from_hms(3, 0, 0).expect("UTC+3 is always valid");

    let odt: OffsetDateTime = OffsetDateTime::parse(business_ts, &Rfc3339)
        .or_else(|_| {
            // Accept bare ISO-8601 without explicit timezone (append Z = UTC)
            OffsetDateTime::parse(&format!("{business_ts}Z"), &Rfc3339)
        })
        .map_err(|_| SidecarError::BadRequest(format!(
            "invalid business_ts {business_ts:?}; expected ISO-8601 UTC"
        )))?
        .to_offset(kyiv);

    let fmt = format_description::parse("[year][month][day][hour][minute][second]")
        .expect("static format string is valid");
    Ok(odt.format(&fmt).expect("YYYYMMDDHHMMSS format is infallible for a valid OffsetDateTime"))
}

// ── SHIFT_OPEN ────────────────────────────────────────────────────────────────

fn build_shift_open(
    cmd: &CanonicalCommand,
    ctx: &BuildContext,
    ts_str: &str,
    rq_attrs: &[(&str, String)],
) -> String {
    let opening_sum = cmd
        .payload
        .pointer("/receipt/totals/total_sum")
        .and_then(|v| v.as_i64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "0".to_string());

    let c_content = tag(
        "C",
        vec![("T", "108".to_string())],
        &(tag("O", vec![("N", "1".to_string()), ("SM", opening_sum), ("T", "0".to_string())], "")
            + &tag("E", vec![("N", "2".to_string())], "")),
    ) + &tag("TS", vec![], ts_str);

    let dat_content = tag(
        "DAT",
        vec![
            // DPS ФСКО v2.2.3 §3.1: SHIFT_OPEN (T=108) always carries DI=0.
            // The local document counter starts from 1 with the first SELL/RETURN
            // after shift open. ctx.local_number is intentionally not used here.
            ("DI", "0".to_string()),
            ("FN", cmd.fiscal_number.clone()),
            ("TN", ctx.tax_number.to_string()),
            ("V",  "1".to_string()),
            ("ZN", ctx.z_number.to_string()),
        ],
        &c_content,
    ) + &tag("MAC", vec![], ctx.previous_hash);

    tag("RQ", rq_attrs.to_vec(), &dat_content)
}

// ── SELL / RETURN ─────────────────────────────────────────────────────────────

fn build_check(
    cmd: &CanonicalCommand,
    ctx: &BuildContext,
    ts_str: &str,
    rq_attrs: &[(&str, String)],
    check_xml_type: &str,
) -> String {
    let receipt = cmd.payload.get("receipt");

    let mut header_xml = String::new();
    let mut items_xml = String::new();
    let mut discounts_xml = String::new();
    let mut payments_xml = String::new();
    let mut footer_xml = String::new();
    let mut item_no: i64 = 1;
    let mut total_sum: i64 = 0;
    let mut p_item_numbers: Vec<i64> = Vec::new();
    let mut group_sums: HashMap<String, i64> = HashMap::new();

    if let Some(receipt) = receipt.and_then(|v| v.as_object()) {
        // Header text lines
        if let Some(header) = receipt.get("header").and_then(|v| v.as_str()) {
            for line in header.split('\n') {
                let line = line.trim();
                if !line.is_empty() {
                    header_xml += &tag(
                        "L",
                        vec![("N", item_no.to_string()), ("NM", line.to_string())],
                        "",
                    );
                    item_no += 1;
                }
            }
        }

        let footer_lines: Vec<String> = receipt
            .get("footer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .split('\n')
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        // Goods
        if let Some(goods) = receipt.get("goods").and_then(|v| v.as_array()) {
            for g in goods {
                let Some(g) = g.as_object() else { continue };

                let code = g.get("code").and_then(|v| v.as_str()).unwrap_or("0");
                let name = g.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let price = g.get("price").and_then(|v| v.as_i64()).unwrap_or(0);
                let quantity = g.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0);
                let sum = g.get("sum").and_then(|v| v.as_i64()).unwrap_or(0);

                let mut p_attrs: Vec<(&str, String)> = vec![
                    ("C",   code.to_string()),
                    ("N",   item_no.to_string()),
                    ("NM",  name.to_string()),
                    ("PRC", price.to_string()),
                    ("Q",   quantity.to_string()),
                    ("SM",  sum.to_string()),
                ];
                if let Some(bc) = g.get("barcode").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    p_attrs.push(("CD", bc.to_string()));
                }
                if let Some(uktzed) = g.get("uktzed").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    p_attrs.push(("CZD", uktzed.to_string()));
                }
                if let Some(tax_id) = g.get("tax_id").and_then(|v| v.as_i64()) {
                    p_attrs.push(("TX", tax_id.to_string()));
                }
                if let Some(tax_id_2) = g.get("tax_id_2").and_then(|v| v.as_i64()) {
                    p_attrs.push(("TX1", tax_id_2.to_string()));
                }

                // Excise marks
                let ca_xml: String = g
                    .get("excise_barcodes")
                    .and_then(|v| v.as_array())
                    .map(|marks| {
                        marks
                            .iter()
                            .filter_map(|m| m.as_str())
                            .map(|m| tag("CA", vec![("CA", m.to_string())], ""))
                            .collect()
                    })
                    .unwrap_or_default();

                let p_item_no = item_no;
                p_item_numbers.push(item_no);
                items_xml += &tag("P", p_attrs, &ca_xml);
                item_no += 1;

                // Accumulate per-group sums
                if let Some(tax_id) = g.get("tax_id").and_then(|v| v.as_i64()) {
                    let entry = group_sums.entry(tax_id.to_string()).or_insert(0);
                    *entry += sum;
                }

                // Per-item discounts
                if let Some(disc_arr) = g.get("discounts").and_then(|v| v.as_array()) {
                    for d in disc_arr {
                        let Some(d) = d.as_object() else { continue };
                        let d_value = d.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
                        if d_value == 0 { continue; }
                        let d_type = d.get("type").and_then(|v| v.as_str()).unwrap_or("DISCOUNT");
                        let d_mode = d.get("mode").and_then(|v| v.as_str()).unwrap_or("VALUE");
                        let xml_tag = if d_type != "EXTRA_CHARGE" { "D" } else { "S" };
                        let mut d_attrs: Vec<(&str, String)> = vec![
                            ("N",  item_no.to_string()),
                            ("NI", p_item_no.to_string()),
                            ("TR", "0".to_string()),
                        ];
                        if d_mode == "PERCENT" {
                            d_attrs.push(("TY", "1".to_string()));
                            d_attrs.push(("PR", format!("{:.2}", d_value)));
                            d_attrs.push(("SM", py_round(sum as f64 * d_value as f64 / 100.0).to_string()));
                        } else {
                            d_attrs.push(("TY", "0".to_string()));
                            d_attrs.push(("SM", d_value.to_string()));
                        }
                        if let Some(nm) = d.get("name").and_then(|v| v.as_str()) {
                            d_attrs.push(("NM", nm.to_string()));
                        }
                        if let Some(priv_) = d.get("privilege").and_then(|v| v.as_str()) {
                            d_attrs.push(("DN", priv_.to_string()));
                        }
                        if let Some(tc) = d.get("tax_code").and_then(|v| v.as_str()) {
                            d_attrs.push(("TX", tc.to_string()));
                        }
                        discounts_xml += &tag(xml_tag, d_attrs, "");
                        item_no += 1;
                    }
                }
            }
        }

        // Check-level discounts
        let check_discounts = receipt.get("discounts").and_then(|v| v.as_array());
        if let Some(check_discounts) = check_discounts {
            if !check_discounts.is_empty() && !p_item_numbers.is_empty() {
                let goods_total: i64 = receipt
                    .get("goods")
                    .and_then(|v| v.as_array())
                    .map(|goods| {
                        goods
                            .iter()
                            .filter_map(|g| g.as_object())
                            .map(|g| g.get("sum").and_then(|v| v.as_i64()).unwrap_or(0))
                            .sum()
                    })
                    .unwrap_or(0);

                for d in check_discounts {
                    let Some(d) = d.as_object() else { continue };
                    let d_value = d.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
                    if d_value == 0 { continue; }
                    let d_type = d.get("type").and_then(|v| v.as_str()).unwrap_or("DISCOUNT");
                    let d_mode = d.get("mode").and_then(|v| v.as_str()).unwrap_or("VALUE");
                    let xml_tag = if d_type != "EXTRA_CHARGE" { "D" } else { "S" };
                    let ni_xml: String = p_item_numbers
                        .iter()
                        .map(|n| tag("NI", vec![("NI", n.to_string())], ""))
                        .collect();
                    let mut d_attrs: Vec<(&str, String)> = vec![
                        ("N",  item_no.to_string()),
                        ("TR", "1".to_string()),
                    ];
                    if d_mode == "PERCENT" {
                        d_attrs.push(("TY", "1".to_string()));
                        d_attrs.push(("PR", format!("{:.2}", d_value)));
                        d_attrs.push(("SM", py_round(goods_total as f64 * d_value as f64 / 100.0).to_string()));
                    } else {
                        d_attrs.push(("TY", "0".to_string()));
                        d_attrs.push(("SM", d_value.to_string()));
                    }
                    if let Some(nm) = d.get("name").and_then(|v| v.as_str()) {
                        d_attrs.push(("NM", nm.to_string()));
                    }
                    discounts_xml += &tag(xml_tag, d_attrs, &ni_xml);
                    item_no += 1;
                }
            }
        }

        // Totals
        total_sum = receipt
            .get("totals")
            .and_then(|v| v.as_object())
            .and_then(|t| t.get("total_sum"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // Payments
        let payments_arr = receipt.get("payments").and_then(|v| v.as_array());
        if let Some(payments) = payments_arr {
            let all_payments_sum: i64 = payments
                .iter()
                .filter_map(|p| p.as_object())
                .map(|p| p.get("amount").and_then(|v| v.as_i64()).unwrap_or(0))
                .sum();
            let change = if all_payments_sum > total_sum { all_payments_sum - total_sum } else { 0 };
            let rounding = receipt.get("rounding").and_then(|v| v.as_i64()).unwrap_or(0);
            let mut rm_assigned = false;
            let mut smp_assigned = false;

            for p in payments {
                let Some(p) = p.as_object() else { continue };
                let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("CASH");
                let amount = p.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);

                // `resolved_t` and `resolved_nm` are enriched by Python before posting
                let m_type = p
                    .get("resolved_t")
                    .and_then(|v| v.as_str())
                    .unwrap_or(if ptype == "CASH" { "0" } else { "2" });
                let m_nm = p
                    .get("resolved_nm")
                    .and_then(|v| v.as_str())
                    .unwrap_or(ptype);

                let mut m_attrs: Vec<(&str, String)> = vec![
                    ("N",  item_no.to_string()),
                    ("NM", m_nm.to_string()),
                    ("SM", amount.to_string()),
                    ("T",  m_type.to_string()),
                ];

                // Card/EPZ payment detail fields
                if m_type != "0" {
                    for (src, dst) in &[
                        ("rrn",            "RRN"),
                        ("payment_system", "PSNM"),
                        ("bank_name",      "PA"),
                        ("terminal",       "PB"),
                        ("label",          "PC"),
                        ("card_mask",      "PD"),
                        ("auth_code",      "PE"),
                    ] {
                        if let Some(val) = p.get(*src).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                            m_attrs.push((dst, val.to_string()));
                        }
                    }
                    if let Some(comm) = p.get("commission").and_then(|v| v.as_i64()).filter(|&c| c > 0) {
                        m_attrs.push(("PF", comm.to_string()));
                    }
                }

                // Cash change
                if m_type == "0" && change > 0 && !rm_assigned {
                    m_attrs.push(("RM", change.to_string()));
                    rm_assigned = true;
                }
                // Rounding
                if m_type == "0" && rounding != 0 && !smp_assigned {
                    m_attrs.push(("SMP", rounding.to_string()));
                    smp_assigned = true;
                }

                payments_xml += &tag("M", m_attrs, "");
                item_no += 1;
            }
        }

        // Footer lines
        for line in footer_lines {
            footer_xml += &tag("L", vec![("N", item_no.to_string()), ("NM", line)], "");
            item_no += 1;
        }
    }

    let total_xml = build_e_element(
        item_no,
        total_sum,
        &cmd.fiscal_number,
        ts_str,
        ctx.local_number,
        &group_sums,
        ctx.tax_groups,
    );

    let c_content = tag(
        "C",
        vec![("T", check_xml_type.to_string())],
        &(header_xml + &items_xml + &discounts_xml + &payments_xml + &footer_xml + &total_xml),
    ) + &tag("TS", vec![], ts_str);

    let dat_content = tag(
        "DAT",
        vec![
            ("DI", ctx.local_number.to_string()),
            ("FN", cmd.fiscal_number.clone()),
            ("TN", ctx.tax_number.to_string()),
            ("V",  "1".to_string()),
            ("ZN", ctx.z_number.to_string()),
        ],
        &c_content,
    ) + &tag("MAC", vec![], ctx.previous_hash);

    tag("RQ", rq_attrs.to_vec(), &dat_content)
}

// ── SERVICE_IN / SERVICE_OUT ──────────────────────────────────────────────────

fn build_service(
    cmd: &CanonicalCommand,
    ctx: &BuildContext,
    ts_str: &str,
    rq_attrs: &[(&str, String)],
    direction: &str,
) -> String {
    let service_sum = cmd
        .payload
        .get("service_sum")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .to_string();

    let mut header_xml = String::new();
    let mut item_no: i64 = 1;

    if let Some(receipt) = cmd.payload.get("receipt").and_then(|v| v.as_object()) {
        if let Some(header) = receipt.get("header").and_then(|v| v.as_str()) {
            for line in header.split('\n') {
                let line = line.trim();
                if !line.is_empty() {
                    header_xml += &tag(
                        "L",
                        vec![("N", item_no.to_string()), ("NM", line.to_string())],
                        "",
                    );
                    item_no += 1;
                }
            }
        }
    }

    let io_tag = if direction == "IN" { "I" } else { "O" };
    let io_n = item_no;
    let e_n = item_no + 1;

    let c_content = tag(
        "C",
        vec![("T", "2".to_string())],
        &(header_xml
            + &tag(
                io_tag,
                vec![
                    ("N",  io_n.to_string()),
                    ("NM", "ГОТІВКА".to_string()),
                    ("SM", service_sum),
                    ("T",  "0".to_string()),
                ],
                "",
            )
            + &tag("E", vec![("N", e_n.to_string())], "")),
    ) + &tag("TS", vec![], ts_str);

    let dat_content = tag(
        "DAT",
        vec![
            ("DI", ctx.local_number.to_string()),
            ("FN", cmd.fiscal_number.clone()),
            ("TN", ctx.tax_number.to_string()),
            ("V",  "1".to_string()),
            ("ZN", ctx.z_number.to_string()),
        ],
        &c_content,
    ) + &tag("MAC", vec![], ctx.previous_hash);

    tag("RQ", rq_attrs.to_vec(), &dat_content)
}

// ── CASH_WITHDRAWAL ───────────────────────────────────────────────────────────

fn build_cash_withdrawal(
    cmd: &CanonicalCommand,
    ctx: &BuildContext,
    ts_str: &str,
    rq_attrs: &[(&str, String)],
) -> String {
    let cash_sum = cmd
        .payload
        .get("cash_withdrawal_sum")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .to_string();

    let mut payment_xml = String::new();
    let mut item_no: i64 = 1;

    if let Some(receipt) = cmd.payload.get("receipt").and_then(|v| v.as_object()) {
        if let Some(payments) = receipt.get("payments").and_then(|v| v.as_array()) {
            for p in payments {
                let Some(p) = p.as_object() else { continue };
                item_no += 1;
                let amount = p
                    .get("amount")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| cash_sum.clone());
                let mut m_attrs: Vec<(&str, String)> = vec![
                    ("N",  item_no.to_string()),
                    ("SM", amount),
                    ("T",  "0".to_string()),
                ];
                for (src, dst) in &[
                    ("bank_name",      "PA"),
                    ("terminal",       "PB"),
                    ("label",          "PC"),
                    ("card_mask",      "PD"),
                    ("auth_code",      "PE"),
                    ("payment_system", "PSNM"),
                    ("rrn",            "RRN"),
                ] {
                    if let Some(val) = p.get(*src).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                        m_attrs.push((dst, val.to_string()));
                    }
                }
                // Default label for cash withdrawal
                let has_pc = m_attrs.iter().any(|(k, _)| *k == "PC");
                if !has_pc {
                    m_attrs.push(("PC", "ВИДАЧА КОШТІВ".to_string()));
                }
                if let Some(comm) = p.get("commission").and_then(|v| v.as_i64()).filter(|&c| c > 0) {
                    m_attrs.push(("PF", comm.to_string()));
                }
                payment_xml += &tag("M", m_attrs, "");
            }
        }
    }

    if payment_xml.is_empty() {
        item_no += 1;
        payment_xml = tag(
            "M",
            vec![
                ("N",  item_no.to_string()),
                ("NM", "ГОТІВКА".to_string()),
                ("SM", cash_sum.clone()),
                ("T",  "0".to_string()),
            ],
            "",
        );
    }

    let e_no = item_no + 1;

    let c_content = tag(
        "C",
        vec![("T", "8".to_string())],
        &(tag(
            "P",
            vec![
                ("C",  "0".to_string()),
                ("N",  "1".to_string()),
                ("NM", "Гривня".to_string()),
                ("SM", cash_sum.clone()),
            ],
            "",
        ) + &payment_xml
            + &tag(
                "E",
                vec![
                    ("FN", cmd.fiscal_number.clone()),
                    ("N",  e_no.to_string()),
                    ("NO", ctx.local_number.to_string()),
                    ("SM", cash_sum),
                    ("TS", ts_str.to_string()),
                ],
                "",
            )),
    ) + &tag("TS", vec![], ts_str);

    let dat_content = tag(
        "DAT",
        vec![
            ("DI", ctx.local_number.to_string()),
            ("FN", cmd.fiscal_number.clone()),
            ("TN", ctx.tax_number.to_string()),
            ("V",  "1".to_string()),
            ("ZN", ctx.z_number.to_string()),
        ],
        &c_content,
    ) + &tag("MAC", vec![], ctx.previous_hash);

    tag("RQ", rq_attrs.to_vec(), &dat_content)
}

// ── Z_REPORT ──────────────────────────────────────────────────────────────────

fn build_z_report(
    cmd: &CanonicalCommand,
    ctx: &BuildContext,
    ts_str: &str,
    rq_attrs: &[(&str, String)],
) -> String {
    let mut z_content = String::new();

    let z_data = cmd
        .payload
        .get("z_report_data")
        .and_then(|v| v.as_object());

    if let Some(z_data) = z_data {
        // Tax sums
        if let Some(tax_sums) = z_data.get("tax_sums").and_then(|v| v.as_object()) {
            let mut tax_ids: Vec<&String> = tax_sums.keys().collect();
            tax_ids.sort();
            for tax_id in tax_ids {
                let ts_data = match tax_sums.get(tax_id).and_then(|v| v.as_object()) {
                    Some(d) => d,
                    None => continue,
                };
                let smi = ts_data.get("smi").and_then(|v| v.as_i64()).unwrap_or(0);
                let smo = ts_data.get("smo").and_then(|v| v.as_i64()).unwrap_or(0);

                if let Some(tax_groups) = ctx.tax_groups {
                    if let Some(group) = tax_groups.get(tax_id.as_str()) {
                        let (txi, _) = if smi != 0 {
                            calc_tax(smi, group.tax_rate, group.additional_rate, group.tax_algorithm)
                        } else {
                            (0, 0)
                        };
                        let (txo, _) = if smo != 0 {
                            calc_tax(smo, group.tax_rate, group.additional_rate, group.tax_algorithm)
                        } else {
                            (0, 0)
                        };
                        z_content += &tag(
                            "TXS",
                            vec![
                                ("DTPR", format!("{:.2}", group.additional_rate)),
                                ("SMI",  smi.to_string()),
                                ("SMO",  smo.to_string()),
                                ("TS",   ts_str[..8].to_string()),
                                ("TX",   tax_id.clone()),
                                ("TXAL", group.tax_algorithm.to_string()),
                                ("TXI",  txi.to_string()),
                                ("TXO",  txo.to_string()),
                                ("TXPR", format!("{:.2}", group.tax_rate)),
                                ("TXTY", group.tax_type.to_string()),
                            ],
                            "",
                        );
                        continue;
                    }
                }
                // No group info — minimal TXS
                z_content += &tag(
                    "TXS",
                    vec![
                        ("SMI", smi.to_string()),
                        ("SMO", smo.to_string()),
                        ("TX",  tax_id.clone()),
                    ],
                    "",
                );
            }
        }

        // Payment sums
        if let Some(payment_sums) = z_data.get("payment_sums").and_then(|v| v.as_object()) {
            let mut ptypes: Vec<&String> = payment_sums.keys().collect();
            ptypes.sort();
            for ptype in ptypes {
                let ps = match payment_sums.get(ptype).and_then(|v| v.as_object()) {
                    Some(d) => d,
                    None => continue,
                };
                let m_type = if ptype == "CASH" { "0" } else { "2" };
                z_content += &tag(
                    "M",
                    vec![
                        ("NM",  ptype.clone()),
                        ("SMI", ps.get("smi").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                        ("SMO", ps.get("smo").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                        ("T",   m_type.to_string()),
                    ],
                    "",
                );
            }
        }

        // Service sums
        if let Some(service_sums) = z_data.get("service_sums").and_then(|v| v.as_object()) {
            let mut stypes: Vec<&String> = service_sums.keys().collect();
            stypes.sort();
            for stype in stypes {
                let ss = match service_sums.get(stype).and_then(|v| v.as_object()) {
                    Some(d) => d,
                    None => continue,
                };
                z_content += &tag(
                    "IO",
                    vec![
                        ("NM",  stype.clone()),
                        ("SMI", ss.get("smi").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                        ("SMO", ss.get("smo").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                        ("T",   "0".to_string()),
                    ],
                    "",
                );
            }
        }

        // Check count
        if let Some(check_count) = z_data.get("check_count").and_then(|v| v.as_object()) {
            z_content += &tag(
                "NC",
                vec![
                    ("NI", check_count.get("ni").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                    ("NO", check_count.get("no").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                ],
                "",
            );
        }

        // EPZ sums (cash withdrawal summary)
        if let Some(epz_sums) = z_data.get("epz_sums").and_then(|v| v.as_object()) {
            if !epz_sums.is_empty() {
                z_content += &tag(
                    "EPZ",
                    vec![
                        ("EPC",  epz_sums.get("epc").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                        ("EPCS", epz_sums.get("epcs").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                        ("EPSM", epz_sums.get("epsm").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
                    ],
                    "",
                );
            }
        }
    }

    let z_tag = tag(
        "Z",
        vec![("NO", ctx.z_number.to_string())],
        &z_content,
    );
    let dat_content = tag(
        "DAT",
        vec![
            ("DI", ctx.local_number.to_string()),
            ("FN", cmd.fiscal_number.clone()),
            ("TN", ctx.tax_number.to_string()),
            ("V",  "1".to_string()),
            ("ZN", ctx.z_number.to_string()),
        ],
        &(z_tag + &tag("TS", vec![], ts_str)),
    ) + &tag("MAC", vec![], ctx.previous_hash);

    tag("RQ", rq_attrs.to_vec(), &dat_content)
}

// ── E element (totals + tax breakdown) ───────────────────────────────────────

fn build_e_element(
    item_no: i64,
    total_sum: i64,
    fiscal_number: &str,
    ts_str: &str,
    local_number: i64,
    group_sums: &HashMap<String, i64>,
    tax_groups: Option<&HashMap<String, TaxGroup>>,
) -> String {
    let base_attrs = vec![
        ("FN", fiscal_number.to_string()),
        ("N",  item_no.to_string()),
        ("NO", local_number.to_string()),
        ("SM", total_sum.to_string()),
        ("TS", ts_str.to_string()),
    ];

    if tax_groups.is_none() || group_sums.is_empty() {
        return tag("E", base_attrs, "");
    }
    let tax_groups = tax_groups.unwrap();

    let mut tx_xml = String::new();
    let mut sorted_ids: Vec<&String> = group_sums.keys().collect();
    sorted_ids.sort();

    for tax_id in sorted_ids {
        let group_sum = group_sums[tax_id];
        let group = match tax_groups.get(tax_id.as_str()) {
            Some(g) => g,
            None => continue,
        };
        let (txsm, dtsm) = calc_tax(group_sum, group.tax_rate, group.additional_rate, group.tax_algorithm);
        tx_xml += &tag(
            "TX",
            vec![
                ("DTPR", format!("{:.2}", group.additional_rate)),
                ("DTSM", dtsm.to_string()),
                ("TX",   tax_id.clone()),
                ("TXAL", group.tax_algorithm.to_string()),
                ("TXPR", format!("{:.2}", group.tax_rate)),
                ("TXSM", txsm.to_string()),
                ("TXTY", group.tax_type.to_string()),
            ],
            "",
        );
    }

    tag("E", base_attrs, &tx_xml)
}

// ── Tax calculation ───────────────────────────────────────────────────────────

/// Mirrors Python `_calc_tax`. Returns `(txsm, dtsm)`.
fn calc_tax(group_sum: i64, tax_rate: f64, additional_rate: f64, tax_algorithm: i32) -> (i64, i64) {
    let mut dtsm: i64 = 0;
    let mut txsm: i64 = 0;

    match tax_algorithm {
        0 => {
            if tax_rate > 0.0 {
                txsm = py_round(group_sum as f64 * tax_rate / (100.0 + tax_rate));
            }
        }
        1 => {
            if additional_rate > 0.0 {
                dtsm = py_round(group_sum as f64 * additional_rate / 100.0);
            }
            if tax_rate > 0.0 {
                txsm = py_round((group_sum + dtsm) as f64 * tax_rate / (100.0 + tax_rate));
            }
        }
        2 => {
            if additional_rate > 0.0 {
                dtsm = py_round(group_sum as f64 * additional_rate / (100.0 + additional_rate));
            }
            if tax_rate > 0.0 {
                txsm = py_round((group_sum - dtsm) as f64 * tax_rate / (100.0 + tax_rate));
            }
        }
        _ => {}
    }

    (txsm, dtsm)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// XML attribute + content escaping — matches Python `_xml_escape`.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Build an XML element.
///
/// `attrs` is a `Vec<(&str, String)>` — sorted alphabetically by key before
/// output to match Python `sorted(attrs.items())`.
/// Empty `attrs` → no attribute string.
fn tag(name: &str, mut attrs: Vec<(&str, String)>, content: &str) -> String {
    attrs.sort_by(|a, b| a.0.cmp(b.0));
    if attrs.is_empty() {
        format!("<{name}>{content}</{name}>")
    } else {
        let attr_str: String = attrs
            .into_iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, xml_escape(&v)))
            .collect::<Vec<_>>()
            .join(" ");
        format!("<{name} {attr_str}>{content}</{name}>")
    }
}

/// Banker's rounding (round-half-to-even) to match Python `round()`.
///
/// Rust's `f64::round()` uses round-half-away-from-zero, which diverges for
/// exact midpoints (e.g. 0.5 → 1 in Rust, 0 in Python).
fn py_round(x: f64) -> i64 {
    let floor = x.floor() as i64;
    let frac = x - x.floor();
    if (frac - 0.5).abs() < f64::EPSILON {
        // Exact midpoint — round to even
        if floor % 2 == 0 { floor } else { floor + 1 }
    } else {
        x.round() as i64
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Helper ────────────────────────────────────────────────────────────────

    fn make_cmd(op: &str, payload: serde_json::Value) -> CanonicalCommand {
        serde_json::from_value(json!({
            "schema_version": "1.0",
            "request_id":     "req-1",
            "idempotency_key":"idem-1",
            "operation_type": op,
            "fiscal_number":  "3001234567",
            "business_ts":    "2026-04-19T09:00:00Z",
            "payload_sha256": "abc",
            "payload": payload
        }))
        .unwrap()
    }

    fn default_ctx<'a>() -> BuildContext<'a> {
        BuildContext {
            local_number:   42,
            z_number:       7,
            previous_hash:  "AABBCC",
            tax_number:     "12345678",
            tax_groups:     None,
            device_name:    "ПРО_каса",
            device_version: "1.1",
        }
    }

    fn xml_from(cmd: &CanonicalCommand, ctx: &BuildContext) -> String {
        let bytes = build(cmd, ctx).expect("build must succeed");
        // Decode cp1251 → utf-8 for inspection
        let (decoded, _, _) = encoding_rs::WINDOWS_1251.decode(&bytes);
        decoded.into_owned()
    }

    // ── Unit tests ────────────────────────────────────────────────────────────

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("&"), "&amp;");
        assert_eq!(xml_escape("\""), "&quot;");
        assert_eq!(xml_escape("<"), "&lt;");
        assert_eq!(xml_escape(">"), "&gt;");
        assert_eq!(xml_escape("a&b<c>d\"e"), "a&amp;b&lt;c&gt;d&quot;e");
    }

    #[test]
    fn test_tag_basic() {
        assert_eq!(tag("E", vec![], "content"), "<E>content</E>");
    }

    #[test]
    fn test_tag_attrs_sorted() {
        // Insert in reverse order; output must be alphabetical
        let out = tag("X", vec![("ZZZ", "3".to_string()), ("AAA", "1".to_string()), ("MMM", "2".to_string())], "");
        assert_eq!(out, r#"<X AAA="1" MMM="2" ZZZ="3"></X>"#);
    }

    #[test]
    fn test_py_round_banker() {
        // Round half to even
        assert_eq!(py_round(0.5), 0);   // floor=0 (even) → 0
        assert_eq!(py_round(1.5), 2);   // floor=1 (odd)  → 2
        assert_eq!(py_round(2.5), 2);   // floor=2 (even) → 2
        assert_eq!(py_round(3.5), 4);   // floor=3 (odd)  → 4
        assert_eq!(py_round(-0.5), 0);  // Python round(-0.5) == 0
        // Normal rounding
        assert_eq!(py_round(1.4), 1);
        assert_eq!(py_round(1.6), 2);
    }

    #[test]
    fn test_calc_tax_al0() {
        // TXAL=0, 20% ПДВ included: 12000 kopecks, taxsm = round(12000*20/120) = 2000
        let (txsm, dtsm) = calc_tax(12000, 20.0, 0.0, 0);
        assert_eq!(txsm, 2000);
        assert_eq!(dtsm, 0);
    }

    #[test]
    fn test_calc_tax_al1() {
        // TXAL=1: dtsm = round(10000 * 5/100) = 500
        // txsm = round((10000+500) * 20/120) = round(10500*20/120) = round(1750) = 1750
        let (txsm, dtsm) = calc_tax(10000, 20.0, 5.0, 1);
        assert_eq!(dtsm, 500);
        assert_eq!(txsm, 1750);
    }

    #[test]
    fn test_format_ts_utc_to_kyiv() {
        // UTC 09:00 → Kyiv 12:00 (UTC+3)
        assert_eq!(format_ts("2026-04-19T09:00:00Z").unwrap(), "20260419120000");
    }

    #[test]
    fn test_format_ts_invalid_returns_bad_request() {
        let err = format_ts("not-a-date").unwrap_err();
        assert!(matches!(err, SidecarError::BadRequest(_)), "expected BadRequest, got {err:?}");
    }

    #[test]
    fn test_shift_open_structure() {
        let cmd = make_cmd("SHIFT_OPEN", json!({}));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains("<RQ "), "missing <RQ");
        assert!(xml.contains("<DAT "), "missing <DAT");
        assert!(xml.contains(r#"<C T="108">"#), "missing <C T=\"108\">");
        assert!(xml.contains("<MAC>"), "missing <MAC>");
    }

    #[test]
    fn test_sell_structure() {
        let cmd = make_cmd("SELL", json!({
            "receipt": {
                "goods": [{"name":"Кава","price":5000,"quantity":1000,"sum":5000}],
                "payments": [{"type":"CASH","amount":5000}],
                "totals": {"total_sum": 5000}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains(r#"<C T="0">"#), "missing <C T=\"0\">");
        assert!(xml.contains("<P "), "missing <P");
        assert!(xml.contains("<M "), "missing <M");
        assert!(xml.contains("<E "), "missing <E");
    }

    #[test]
    fn test_service_in_structure() {
        let cmd = make_cmd("SERVICE_IN", json!({"service_sum": 10000}));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains(r#"<C T="2">"#), "missing <C T=\"2\">");
        assert!(xml.contains("<I "), "missing <I");
    }

    #[test]
    fn test_service_out_structure() {
        let cmd = make_cmd("SERVICE_OUT", json!({"service_sum": 5000}));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains("<O "), "missing <O");
    }

    #[test]
    fn test_cash_withdrawal_structure() {
        let cmd = make_cmd("CASH_WITHDRAWAL", json!({"cash_withdrawal_sum": 20000}));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains(r#"<C T="8">"#), "missing <C T=\"8\">");
        assert!(xml.contains("<P "), "missing <P");
        assert!(xml.contains("<M "), "missing <M");
        assert!(xml.contains("<E "), "missing <E");
    }

    #[test]
    fn test_z_report_structure() {
        let cmd = make_cmd("Z_REPORT", json!({
            "z_report_data": {
                "payment_sums": {"CASH": {"smi": 50000, "smo": 0}},
                "check_count": {"ni": 5, "no": 1}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains("<Z "), "missing <Z");
        assert!(xml.contains("<M "), "missing <M");
        assert!(xml.contains("<NC "), "missing <NC");
    }

    #[test]
    fn test_return_uses_type_1() {
        let cmd = make_cmd("RETURN", json!({
            "receipt": {
                "goods": [{"name":"Товар","price":1000,"quantity":1000,"sum":1000}],
                "payments": [{"type":"CASH","amount":1000}],
                "totals": {"total_sum": 1000}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains(r#"<C T="1">"#), "RETURN must use T=\"1\"");
    }

    #[test]
    fn test_sell_with_discount_item_level() {
        let cmd = make_cmd("SELL", json!({
            "receipt": {
                "goods": [{
                    "name": "Товар",
                    "price": 10000,
                    "quantity": 1000,
                    "sum": 10000,
                    "discounts": [{"value": 500, "type": "DISCOUNT", "mode": "VALUE"}]
                }],
                "payments": [{"type":"CASH","amount":9500}],
                "totals": {"total_sum": 9500}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains("<D "), "missing per-item discount <D");
    }

    #[test]
    fn test_sell_with_discount_check_level() {
        let cmd = make_cmd("SELL", json!({
            "receipt": {
                "goods": [{"name":"T","price":10000,"quantity":1000,"sum":10000}],
                "payments": [{"type":"CASH","amount":9000}],
                "totals": {"total_sum": 9000},
                "discounts": [{"value": 1000, "type": "DISCOUNT", "mode": "VALUE"}]
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains(r#"TR="1""#), "check-level discount must have TR=\"1\"");
    }

    #[test]
    fn test_header_footer_as_l_tags() {
        let cmd = make_cmd("SELL", json!({
            "receipt": {
                "header": "Магазин\nАдреса",
                "footer": "Дякуємо",
                "goods": [{"name":"X","price":100,"quantity":1000,"sum":100}],
                "payments": [{"type":"CASH","amount":100}],
                "totals": {"total_sum": 100}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        // Header lines should appear as <L N="..." NM="...">
        assert!(xml.contains("<L "), "missing <L tag for header");
        assert!(xml.contains("Магазин"), "missing header text");
        assert!(xml.contains("Дякуємо"), "missing footer text");
    }

    #[test]
    fn test_card_payment_epz_attrs() {
        let cmd = make_cmd("SELL", json!({
            "receipt": {
                "goods": [{"name":"T","price":5000,"quantity":1000,"sum":5000}],
                "payments": [{
                    "type":"CARD","amount":5000,
                    "rrn":"000123456789",
                    "resolved_t":"2",
                    "resolved_nm":"Visa"
                }],
                "totals": {"total_sum": 5000}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(xml.contains("000123456789"), "RRN must appear in XML");
        assert!(xml.contains("RRN="), "RRN attribute missing");
    }

    #[test]
    fn test_z_report_with_payment_sums() {
        let cmd = make_cmd("Z_REPORT", json!({
            "z_report_data": {
                "payment_sums": {
                    "CASH": {"smi": 100000, "smo": 5000},
                    "CARD": {"smi": 50000, "smo": 0}
                },
                "check_count": {"ni": 10, "no": 2}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        // Both CASH and CARD payment entries
        assert_eq!(xml.matches("<M ").count(), 2, "expect 2 payment <M> entries");
    }

    #[test]
    fn test_unsupported_op_returns_err() {
        let cmd = make_cmd("X_REPORT", json!({}));
        let ctx = default_ctx();
        let result = build(&cmd, &ctx);
        assert!(result.is_err(), "X_REPORT must return SidecarError");
        match result.unwrap_err() {
            SidecarError::BadRequest(_) => {}
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }

    // ── New evidential tests ──────────────────────────────────────────────────

    /// Tax algorithm 2: additional rate extracted from gross, then primary on remainder.
    ///
    /// Formula (from calc_tax branch 2):
    ///   dtsm = py_round(10500 * 5.0 / (100.0 + 5.0)) = py_round(500.0) = 500
    ///   txsm = py_round((10500 - 500) * 20.0 / (100.0 + 20.0)) = py_round(10000 * 20/120)
    ///        = py_round(1666.666...) = 1667
    #[test]
    fn calc_tax_al2_additional_rate() {
        let (txsm, dtsm) = calc_tax(10500, 20.0, 5.0, 2);
        assert_eq!(dtsm, 500, "algo2: additional portion dtsm must be 500");
        assert_eq!(txsm, 1667, "algo2: primary tax txsm on remainder must be 1667");
    }

    /// algo2 with zero primary rate: only additional rate applies.
    ///   dtsm = py_round(12000 * 5.0 / 105.0) = py_round(571.428...) = 571
    ///   txsm = 0 (prc == 0.0)
    #[test]
    fn calc_tax_al2_zero_primary_rate() {
        let (txsm, dtsm) = calc_tax(12000, 0.0, 5.0, 2);
        assert_eq!(dtsm, 571, "algo2: additional only dtsm must be 571");
        assert_eq!(txsm, 0, "algo2: zero primary rate → txsm must be 0");
    }

    /// algo2 with both rates zero: no tax.
    #[test]
    fn calc_tax_al2_both_rates_zero() {
        let (txsm, dtsm) = calc_tax(10000, 0.0, 0.0, 2);
        assert_eq!(dtsm, 0, "algo2 zero rates: dtsm=0");
        assert_eq!(txsm, 0, "algo2 zero rates: txsm=0");
    }

    /// Timestamp without Z suffix must be treated as UTC and converted to Kyiv (UTC+3).
    ///   "2026-04-19T09:00:00" → append Z → UTC 09:00 → Kyiv 12:00.
    #[test]
    fn format_ts_without_z_suffix_parsed_via_fallback() {
        let result = format_ts("2026-04-19T09:00:00");
        assert_eq!(
            result.unwrap(),
            "20260419120000",
            "bare ISO-8601 without Z must be treated as UTC and converted to Kyiv (UTC+3)"
        );
    }

    /// SELL with empty goods list must produce no `<P ` tags.
    #[test]
    fn sell_with_empty_goods_list_produces_no_p_tags() {
        let cmd = make_cmd("SELL", json!({
            "receipt": {
                "goods": [],
                "payments": [{"type": "CASH", "amount": 0}],
                "totals": {"total_sum": 0}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(
            !xml.contains("<P "),
            "SELL with empty goods must produce no <P tags; got: {xml}"
        );
        assert!(xml.contains("<M "), "payment M tag must still be present");
        assert!(xml.contains("<E "), "totals E tag must still be present");
    }

    /// SERVICE_IN: I tag amount must match payload["service_sum"].
    #[test]
    fn service_in_sum_matches_payload_field() {
        let cmd = make_cmd("SERVICE_IN", json!({"service_sum": 87654}));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(
            xml.contains(r#"SM="87654""#),
            "SERVICE_IN: SM attribute must equal service_sum 87654; got: {xml}"
        );
        assert!(xml.contains("<I "), "SERVICE_IN must produce an I tag");
    }

    /// SERVICE_OUT: O tag amount must match payload["service_sum"].
    #[test]
    fn service_out_sum_matches_payload_field() {
        let cmd = make_cmd("SERVICE_OUT", json!({"service_sum": 33333}));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(
            xml.contains(r#"SM="33333""#),
            "SERVICE_OUT: SM attribute must equal service_sum 33333; got: {xml}"
        );
        assert!(xml.contains("<O "), "SERVICE_OUT must produce an O tag");
    }

    /// Large value overflow guard: calc_tax on a fiscal-maximum sum must not panic.
    ///   sum=9_999_999_999, algo=0, 20% ПДВ:
    ///   txsm = py_round(9_999_999_999 * 20.0 / 120.0)
    ///        = py_round(1_666_666_666.5) → floor=1666666666 (even) → 1666666666
    ///   dtsm = 0 (algo 0 does not set additional)
    #[test]
    fn py_round_large_values_no_overflow() {
        let (txsm, dtsm) = calc_tax(9_999_999_999i64, 20.0, 0.0, 0);
        assert_eq!(txsm, 1_666_666_666, "large-sum tax must equal 1_666_666_666 (banker round of .5 to even)");
        assert_eq!(dtsm, 0, "algo=0 must produce dtsm=0");
        assert!(txsm > 0, "tax on large sum must be positive");
        assert!(
            dtsm + txsm <= 9_999_999_999i64 + 1,
            "components must not exceed original plus rounding tolerance"
        );
    }

    /// Zero sum produces zero tax regardless of rate.
    #[test]
    fn py_round_zero_sum_produces_zero_tax() {
        let (txsm, dtsm) = calc_tax(0, 20.0, 0.0, 0);
        assert_eq!(dtsm, 0, "tax on zero sum must be zero");
        assert_eq!(txsm, 0, "net on zero sum must be zero");
    }

    /// Z_REPORT with no payment_sums key must produce no M tags.
    #[test]
    fn z_report_with_no_payments_has_no_m_tags() {
        let cmd = make_cmd("Z_REPORT", json!({
            "z_report_data": {
                "check_count": {"ni": 3, "no": 0}
                // payment_sums intentionally absent
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(
            !xml.contains("<M "),
            "Z_REPORT with no payment_sums must produce no M tags; got: {xml}"
        );
        assert!(xml.contains("<NC "), "check count NC tag must be present");
    }

    /// Z_REPORT with empty payment_sums object must produce no M tags.
    #[test]
    fn z_report_with_empty_payment_sums_has_no_m_tags() {
        let cmd = make_cmd("Z_REPORT", json!({
            "z_report_data": {
                "payment_sums": {},
                "check_count": {"ni": 0, "no": 0}
            }
        }));
        let ctx = default_ctx();
        let xml = xml_from(&cmd, &ctx);
        assert!(
            !xml.contains("<M "),
            "Z_REPORT with empty payment_sums must produce no M tags; got: {xml}"
        );
    }

    /// Unknown tax algorithm (e.g. 99) must silently produce (0, 0) — no crash.
    #[test]
    fn calc_tax_unknown_algo_returns_zero() {
        let (txsm, dtsm) = calc_tax(10000, 20.0, 5.0, 99);
        assert_eq!(txsm, 0, "unknown tax algo must return txsm=0");
        assert_eq!(dtsm, 0, "unknown tax algo must return dtsm=0");
    }
}
