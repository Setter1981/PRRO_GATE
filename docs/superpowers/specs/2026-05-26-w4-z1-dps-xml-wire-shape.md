# W4-Z1 — DPS XML Wire-Shape Specification

**Date:** 2026-05-26
**Status:** Ground-truth reference (locked)
**Source of truth:**
- `src/prro_gateway/serializers/dps_xml.py` — Sprint 7 production-proven against `cabinet.tax.gov.ua:9443` (full SHIFT_OPEN→SELL→Z_REPORT cycle succeeded; memory `project_sprint7_complete`).
- `docs/webcheck_reverse/WebCheckMain/WebCheck/StringXML.cs` — battle-tested across 50 retail points / 70 cash registers (operator pilot scope).
- ФСКО v2.2.3 protocol official spec (memory `project_fsko_protocol_gaps`).

This document is the **wire-shape contract** the Rust gateway's `xml/mod.rs` + `stage_sign.rs::parse_payload` + `canonical_doc_digest.rs` MUST implement.  Any divergence ⇒ DPS reject, incomplete fiscal audit trail, legal compliance risk.

---

## 0. Universal rules

### Quote style + attribute ordering
- **Quote style:** double-quote (`"`).  Python uses double; WebCheck uses single; both work with DPS.  Rust follows Python (matches existing `xml/mod.rs:cp1251` C2 design).
- **Attribute order:** alphabetical.  Per `dps_xml.py:579-589` `_tag` helper.  WebCheck uses a custom order; DPS accepts both.  Alphabetical is the simpler invariant for golden-byte-equivalence testing.
- **XML escape:** `&` → `&amp;`, `"` → `&quot;`, `<` → `&lt;`, `>` → `&gt;` (per `dps_xml.py:566-568`).
- **Self-closing:** never — always emit as `<tag>...</tag>` even when content is empty (per `dps_xml.py:589`).

### Encoding
- **CP-1251** for text content (UA Cyrillic text inside `NM=`, `<L>`, `<TS>`).  Already implemented in `rust/prro/src/xml/cp1251.rs`.

### Numeric formatting
- **Money** (`SM=`, `PRC=`, `TXSM=`, `DTSM=`): integer kopecks (no decimal point).  `All.Bablo(...)` in WebCheck = kopecks-as-integer formatter.  Python passes raw integers.
- **Quantity** (`Q=`): integer thousandths (1 kg = 1000).
- **Percentage** (`TXPR=`, `DTPR=`, `PR=`): `f'{value:.2f}'` — 2 decimal places (per `dps_xml.py:454-455`).

### Timestamp
- **`<TS>` content:** `YYYYMMDDHHMMSS` Kyiv-local time (NOT UTC; per `dps_xml.py:72-78`).
- **`<TXS TS=>` Z-report:** `YYYYMMDD` only (first 8 chars of TS string per `dps_xml.py:454`).

---

## 1. Envelope: `<RQ>` / `<DAT>` / `<TS>` / `<MAC>`

```xml
<RQ NDv="ПРО_каса" PrV="1.1" V="1">
  <DAT DI="{local_number}" FN="{fiscal_number}" TN="{tax_number}" V="1" ZN="{z_number}">
    <!-- {content: <C> or <Z>} -->
    <TS>{YYYYMMDDHHMMSS}</TS>
  </DAT>
  <MAC>{previous_hash_hex}</MAC>
</RQ>
```

| Attribute | Required? | Source | Format |
|-----------|-----------|--------|--------|
| `RQ NDv=` | yes | config | string, default `"ПРО_каса"` |
| `RQ PrV=` | yes | config | string, default `"1.1"` |
| `RQ V=` | yes | constant | always `"1"` |
| `DAT DI=` | yes | local_number | int.  `"0"` for SHIFT_OPEN, `lnd` for others |
| `DAT FN=` | yes | fiscal_number | string |
| `DAT TN=` | yes | tax_number (TIN/EDRPOU) | string |
| `DAT V=` | yes | constant | always `"1"` |
| `DAT ZN=` | yes | z_number | int.  `"0"` for SHIFT_OPEN |
| `<TS>` content | yes | business_ts Kyiv | `YYYYMMDDHHMMSS` |
| `<MAC>` content | yes | previous_hash | hex string, empty for first doc |

---

## 2. SELL / RETURN: `<C T="0">` / `<C T="1">`

```xml
<C T="0|1">
  <!-- Header text lines (optional) -->
  <L N="1" NM="...header line 1..."/>

  <!-- Items -->
  <P C="..." CD="..." CZD="..." N="..." NM="..." PRC="..." Q="..." SM="..." TX="..." TX1="...">
    <CA CA="excise_stamp_1"></CA>
    <CA CA="excise_stamp_2"></CA>
  </P>

  <!-- Per-item discount/surcharge — sibling after corresponding <P> -->
  <D N="..." NI="prev_P_N" SM="..." TR="0" TY="0|1" PR="..." NM="..." DN="..." TX="..."/>
  <S N="..." NI="prev_P_N" SM="..." TR="0" TY="0|1" PR="..." NM="..." DN="..." TX="..."/>

  <!-- Check-level discount/surcharge — after all <P>, with <NI> children listing affected items -->
  <D N="..." TR="1" TY="0|1" SM="..." PR="..." NM="...">
    <NI NI="P_N_1"></NI>
    <NI NI="P_N_2"></NI>
  </D>

  <!-- Payments -->
  <M N="..." NM="..." SM="..." T="0|1|2|3" RM="..." SMP="..." PA="..." PB="..." PC="..." PD="..." PE="..." PF="..." PSNM="..." RRN="..."/>

  <!-- Footer text lines (optional) -->
  <L N="..." NM="...footer line..."/>

  <!-- Closing -->
  <E FN="..." N="..." NO="..." SM="..." TS="...">
    <TX DTPR="..." DTSM="..." TX="..." TXAL="..." TXPR="..." TXSM="..." TXTY="..."/>
  </E>
</C>
```

### `<P>` per-item attributes

| Attribute | Required? | Source (W3 DTO field) | Notes |
|-----------|-----------|----------------------|-------|
| `C=` | yes | `FiscalLine.article_code` | SKU, fallback `"0"` if missing |
| `N=` | yes | sequence counter | int, item_no |
| `NM=` | yes | `FiscalLine.name` | string |
| `PRC=` | yes | `FiscalLine.price_kopecks` | kopecks (integer) |
| `Q=` | yes | `FiscalLine.quantity_milli` | thousandths (1 milli = 1 thousandth, NO conversion needed) |
| `SM=` | yes | derived: `price * quantity / 1000` | kopecks (integer) |
| `CD=` | opt | `FiscalLine.barcode` | omit if empty/null |
| `CZD=` | opt | `FiscalLine.uktzed` | omit if empty/null — **WARNING: NOT `CD`, it's `CZD`** |
| `TX=` | opt | `FiscalLine.tax_group_1` | omit if `None`; primary tax group |
| `TX1=` | opt | `FiscalLine.tax_group_2` (when `dual_tax_mode = Some`) | omit if `None`; secondary tax group |

### `<CA>` excise stamp children of `<P>`

Per `FiscalLine.excise_stamps[]`:
```xml
<CA CA="{stamp_string}"></CA>
```

Multiple stamps = multiple `<CA>` children.  Empty `excise_stamps` ⇒ no `<CA>` emit ⇒ `<P ...></P>` self-closing form.

### `<D>` discount / `<S>` surcharge — per-item form

When `FiscalLine.discount = Some(Discount { direction, name, amount_kopecks })`:
- `direction = Discount` ⇒ tag = `<D>`
- `direction = Markup` ⇒ tag = `<S>`

Attributes:
| Attribute | Required? | Source | Notes |
|-----------|-----------|--------|-------|
| `N=` | yes | sequence counter | item_no, distinct from parent `<P>` N= |
| `NI=` | yes | parent `<P>` N= | "applies to item with this N" |
| `TR=` | yes | constant `"0"` | flag: per-item (vs check-level=`"1"`) |
| `TY=` | yes | `"0"` (VALUE mode) or `"1"` (PERCENT mode) | per `dps_xml.py:232-237` |
| `SM=` | yes | absolute amount in kopecks | for PERCENT: `round(item.sum * value / 100)` |
| `PR=` | conditional | percent value (if TY=1) | `f'{value:.2f}'` |
| `NM=` | opt | discount name | string |
| `DN=` | opt | privilege code | string |
| `TX=` | opt | tax_code | string |

Per W3 DTO `Discount { direction, name: String, amount_kopecks: u64 }`: assume `TY=0` (VALUE mode) since DTO doesn't carry percent mode separately.  If percent-mode is needed later, extend DTO.

### `<D>` / `<S>` check-level form

Currently W3 DTO does **not** carry check-level discount (only per-line `FiscalLine.discount`).  Python serializer reads from `receipt.discounts[]`.  If pilot needs this, M5 plumb via `raw_frames` or DTO extension.

**For W4-Z1: SKIP check-level discount support; document as DTO-extension scope.**

### `<L>` header/footer text lines

W3 DTO does not carry header/footer text.  Python reads from `receipt.header` / `receipt.footer`.  M5 plumb via `raw_frames`.

**For W4-Z1: SKIP `<L>` emission; document.**

### `<M>` payment attributes

| Attribute | Required? | Source | Notes |
|-----------|-----------|--------|-------|
| `N=` | yes | sequence | int |
| `NM=` | yes | derived from `PaymentKind` | `"ГОТІВКА"` for CASH, etc.  Or use `acquirer_slip.payment_system` if cashless. |
| `SM=` | yes | `CanonicalPayment.amount_kopecks` | kopecks (int) |
| `T=` | yes | derived from `PaymentKind` | `"0"` for CASH, `"1|2|3"` per CASHLESS_1/2/3 (numeric per spec) |
| `RM=` | conditional | change | only on first CASH payment, only if change > 0 |
| `SMP=` | conditional | rounding | only on first CASH payment, only if `receipt.rounding != 0` |

EPZ attributes (only when `T != "0"` and `acquirer_slip.is_some()`):

| Attribute | Source (`AcquirerSlip` field) |
|-----------|-------------------------------|
| `PA=` | `acquirer_slip.payment_system` (or per `dps_xml.py:292-294`: bank_name) — **mapping ambiguity, defer** |
| `PB=` | `terminal_id` |
| `PC=` | `operation_type` (or "label") |
| `PD=` | `pan` (card mask) |
| `PE=` | `approval_code` |
| `PF=` | `fee_kopecks` (commission) |
| `PSNM=` | `payment_system` |
| `RRN=` | `transaction_code` |

**MAPPING NOTE** — `dps_xml.py:291-297` maps `bank_name→PA`, but W3 DTO's `AcquirerSlip.payment_system` could be the `PA` source.  This needs **operator clarification** before W4-Z1 implementation.  Field names in W3 DTO came from `maria304_driver` — driver-side semantics rule.

### `<E>` closing element

Always 5 required attrs:

| Attribute | Source |
|-----------|--------|
| `FN=` | fiscal_number |
| `N=` | sequence counter |
| `NO=` | local_number |
| `SM=` | total sum (kopecks) — `payload.totals.sale_kopecks` or `return_kopecks` |
| `TS=` | business_ts |

Children: `<TX>×N` per-tax-group summaries (see §3 below).  No tax_groups in receipt ⇒ no `<TX>` children, `<E>` is "empty" `<E ...></E>`.

EPZ attrs on `<E>` (`DopTegE()` in WebCheck) — these come from the last card-payment terminal data.  Match the EPZ attrs on `<M>`.  Python does NOT emit EPZ on `<E>` per `_build_e_element` — only on `<M>`.  **Python wins** — Rust skips EPZ on `<E>`.

---

## 3. `<TX>` tax-group summary inside `<E>` and `<TXS>` inside `<Z>`

Common attributes:

| Attribute | Source | Notes |
|-----------|--------|-------|
| `TX=` | tax_id (1..9) | per `TaxGroup.tax_id` config |
| `TXPR=` | `tax_rate` | `f'{rate:.2f}'` |
| `TXSM=` | computed | see `_calc_tax` below |
| `DTPR=` | `additional_rate` | `f'{rate:.2f}'` (excise rate within group) |
| `DTSM=` | computed | excise sum (within tax group) |
| `TXAL=` | `tax_algorithm` | 0/1/2/3 per ФСКО |
| `TXTY=` | `tax_type` | 0 (standard) |
| `TXI=` (Z-report only) | tax-on-sale | from `tax_sums[tax_id].smi` |
| `TXO=` (Z-report only) | tax-on-return | from `tax_sums[tax_id].smo` |
| `SMI=` (Z-report only) | sale subtotal | sum of `<P SM=>` for items in this group, SELL direction |
| `SMO=` (Z-report only) | return subtotal | RETURN direction |
| `TS=` (Z-report only) | date | `YYYYMMDD` (first 8 chars of TS) |

### Tax algorithms (`TXAL=`)

Per `dps_xml.py:_calc_tax:536-563`:

| TXAL | Formula |
|------|---------|
| `0` | TXSM = SM × TXPR / (100 + TXPR).  Standard VAT. |
| `1` | DTSM = SM × DTPR / 100; TXSM = (SM + DTSM) × TXPR / (100 + TXPR).  Excise added pre-VAT. |
| `2` | DTSM = SM × DTPR / (100 + DTPR); TXSM = (SM − DTSM) × TXPR / (100 + TXPR).  Excise post-VAT.  **This is the "акциз через DTPR в групі" pattern per memory `feedback_tax_groups_real`.** |
| `3` | Absolute (per-unit) excise.  **NOT SUPPORTED** in Python serializer; defer M5+. |

Important per `feedback_tax_groups_real`: акциз через DTPR within tax group (TX="4"/ГА), не через separate TX1.  This means **dual-tax_mode is rarely needed** for excise — only for cases where item has TWO independent taxes (PDV + city tax, etc).  Most retail = TXAL=2 single-tax per item.

### Special "0" / "−1" PDV groups (memory `feedback_pdv_zero`)

- `TX="0"` — звільнено (PDV-exempt)
- `TX="-1"` — не об'єкт (not subject to PDV)

Both valid.  TXPR=0 in both cases; TXSM=0.  Rust must emit these correctly when DTO has `tax_group_1 = 0` or `tax_group_1 = -1`.  Currently `FiscalLine.tax_group_1: u8` — u8 cannot represent `-1`.  **Schema gap**: W3 DTO may need to change `tax_group_1` to `i8` or `Option<i8>` to support PDV-not-object.  Operator confirmation needed.

---

## 4. SHIFT_OPEN: `<C T="108">`

```xml
<C T="108">
  <O N="1" SM="{opening_sum_kop}" T="0"></O>
  <E N="2"></E>
</C>
```

- `T="108"` is local; recoded to `SERVICECHK=3` for sendChkV2 gRPC.
- `<O>` = opening operation with cash amount.
- `<E>` self-closing (no items, no payments).
- W3 DTO does NOT carry opening_sum_kop ⇒ placeholder `"0"` + audit_log warn (per W4 §3 W3 addendum).

---

## 5. SERVICE_IN / SERVICE_OUT: `<C T="2">`

```xml
<C T="2">
  <I N="1" NM="ГОТІВКА" SM="{service_sum}" T="0"></I>  <!-- or <O> for OUT -->
  <E N="2"></E>
</C>
```

- `T="2"` = service receipt.
- `<I>` for IN, `<O>` for OUT.  `T="0"` = cash.  `NM="ГОТІВКА"` is constant per `dps_xml.py:359`.
- No `<P>`, no `<M>`.

Source field: W3 DTO does not currently carry service_sum.  Maria304 driver opcodes CAIOI / CAIOO — amount lives in `raw_frames`.  M5 plumb required.

**For W4-Z1: not implementing SERVICE_*; this is PR-C scope.**

---

## 6. CASH_WITHDRAWAL: `<C T="8">`

```xml
<C T="8">
  <P C="0" N="1" NM="Гривня" SM="{cash_sum}"></P>
  <M N="2" NM="..." SM="{cash_sum}" T="0" PA="..." PB="..." PC="..." PD="..." PE="..." PSNM="..." RRN="..." PF="..."/>
  <E FN="..." N="3" NO="..." SM="{cash_sum}" TS="..."></E>
</C>
```

- `T="8"` = cash withdrawal via ЕПЗ.
- Single `<P>` describing withdrawal as "Гривня" item.
- `<M>` payment with EPZ slip attrs.
- Default `PC="ВИДАЧА КОШТІВ"` if not provided (per `dps_xml.py:393-394`).

**For W4-Z1: not implementing; PR-C scope.**

---

## 7. Z_REPORT: `<Z NO="...">`

```xml
<Z NO="{z_number}">
  <!-- Per-tax-group summaries -->
  <TXS DTPR="..." SMI="..." SMO="..." TS="YYYYMMDD" TX="..." TXAL="..." TXI="..." TXO="..." TXPR="..." TXTY="..."/>

  <!-- Per-payment-type summaries (CASH/CASHLESS aggregates) -->
  <M NM="..." SMI="..." SMO="..." T="0|2"></M>

  <!-- Per-service-type summaries (SERVICE_IN/OUT aggregates) -->
  <IO NM="..." SMI="..." SMO="..." T="0"></IO>

  <!-- Check counts -->
  <NC NI="{sell_count}" NO="{return_count}"></NC>

  <!-- Optional: EPZ cash-withdrawal summary -->
  <EPZ EPC="..." EPCS="..." EPSM="..."></EPZ>
</Z>
```

### `<NC>` — required, primary Z-report data

| Attribute | Source |
|-----------|--------|
| `NI=` | count of SELL docs in shift with state IN (ACK, OFFLINE_LOCAL_ACK) |
| `NO=` | count of RETURN docs (same filter) |

Source: NEW repo method `count_by_direction_for_shift(pool, shift_id) -> (sell, return)` mirroring Python `shift_aggregation.aggregate_shift_data`.  SQL spec in `2026-05-25-m4-ingress-plan.md` §3 W4 Algorithm step 0.

### `<TXS>`, `<M>`, `<IO>`, `<EPZ>` aggregations

All require per-group sums computed from `fiscal_documents` rows in the shift.  Python computes in `shift_aggregation.aggregate_shift_data`.

**For W4-Z1: minimum viable Z-report has only `<NC>` populated.**  Pilot acceptance: empty `<TXS>` / `<M>` / `<IO>` / `<EPZ>` is legal-valid Z-report (DPS accepts; same as Python with empty `z_report_data`).  Full aggregation = optional later work (M5 or W4-Z2 if pilot needs detailed Z-report breakdown).

---

## 8. Open questions before W4-Z1 implementation

These need **operator decision** before TDD start:

### Q1. AcquirerSlip → EPZ attribute mapping

W3 DTO `AcquirerSlip`:
```rust
struct AcquirerSlip {
    payment_form_index: u8,
    merchant_id: String,
    terminal_id: String,
    operation_type: String,
    pan: String,
    approval_code: String,
    payment_system: String,
    transaction_code: String,
    fee_kopecks: u64,
    cashier_signature_placeholder: bool,
    cardholder_signature_placeholder: bool,
}
```

DPS XML EPZ attrs: `PA / PB / PC / PD / PE / PF / PSNM / RRN`.

Python `dps_xml.py:291-297` mapping:
```
bank_name      → PA
terminal       → PB  (= terminal_id?)
label          → PC  (= operation_type?)
card_mask      → PD  (= pan?)
auth_code      → PE  (= approval_code?)
payment_system → PSNM
rrn            → RRN  (= transaction_code?)
commission     → PF  (= fee_kopecks?)
```

W3 DTO does NOT have `bank_name` or `card_mask` (it has `pan`).  Mapping is ambiguous between Python's POS-receipt-shape and W3's driver-DTO-shape.

**Operator decision needed:** which W3 fields populate which EPZ attrs?  Suggested mapping:
- `merchant_id` → `PA` (instead of bank_name)
- `terminal_id` → `PB` ✓
- `operation_type` → `PC` ✓
- `pan` → `PD` ✓
- `approval_code` → `PE` ✓
- `payment_system` → `PSNM` ✓
- `transaction_code` → `RRN` ✓
- `fee_kopecks` → `PF` ✓

### Q2. `tax_group_1: u8` cannot represent PDV-not-object (`TX="-1"`)

W3 DTO `FiscalLine.tax_group_1: u8` — only 0..255.  Memory `feedback_pdv_zero` says `TX="-1"` (не об'єкт) is valid.

**Operator decision needed:** change `tax_group_1` type to `i8` or `Option<i8>` in W3 DTO?  OR sentinel value (`255` = -1)?  OR pilot doesn't have PDV-not-object items?

### Q3. Check-level discount + header/footer text lines

Python supports both; W3 DTO does not carry them.  Pilot need?  Defer to M5 plumb via raw_frames?

### Q4. TXAL=3 (absolute excise per quantity)

Python doesn't support it; ФСКО v2.2.3 defines it.  Pilot need?  Probably not (memory `feedback_tax_groups_real`: TXAL=2 within-group covers UA alcohol).

### Q5. Z-report aggregation depth for pilot

Minimum = `<NC>` only.  Full = `<TXS>` per-group + `<M>` per-payment + `<IO>` + `<EPZ>`.  Pilot need full or minimum?  Memory `feedback_cash_balance_zreport`: cash balance NOT reset on Z (operator preference).  This is independent of report content depth.

---

## 9. Implementation surface (for W4-Z1 TDD plan)

Once Q1-Q5 resolved, the W4-Z1 worklet touches:

### A. `rust/prro/src/xml/mod.rs`
- Extend `CheckItem`: add `barcode: Option<String>`, `uktzed: Option<String>`, `tax_group_1: Option<i8>`, `tax_group_2: Option<i8>`, `excise_stamps: Vec<String>`, `discount: Option<Discount>`.
- Extend `CheckPayment`: add EPZ slip fields (8 fields per Q1).
- New struct `TaxGroupSummary { tx, txpr, txsm, dtpr, dtsm, txal, txty, ... }` for `<E>` children + `<TXS>` Z-report children.
- Extend `ZReportPayload`: add `tax_summaries: Vec<TaxGroupSummary>` (optional, default empty).

### B. `rust/prro/src/xml/builders/` (or expand existing builders in mod.rs)
- `build_p_element(item)` — emit `<P>` with optional CD/CZD/TX/TX1 + `<CA>` children.
- `build_d_or_s_element(discount, parent_n)` — emit per-item `<D>` or `<S>`.
- `build_m_element(payment, change, rounding, is_first_cash)` — emit `<M>` with optional EPZ.
- `build_e_element_with_tx(header, tax_summaries)` — emit `<E>` with `<TX>` children.
- `build_tax_calc(group_sum, rate, additional_rate, algo) -> (txsm, dtsm)` — mirror `_calc_tax`.

### C. `rust/prro/src/xml/cp1251.rs`
- Already implemented for existing minimal subset.  Verify new fields (NM with Cyrillic) still work.

### D. Golden tests
- For each doc type × each optional feature combination, capture Python output as golden + assert Rust byte-equivalence.
- Fixtures live in `rust/prro/tests/fixtures/dps_xml_goldens/`.
- Naming: `sell_with_excise.xml`, `sell_with_discount_per_item.xml`, `sell_with_dual_tax.xml`, etc.

### E. Tests
- Unit tests on conversion helpers.
- Golden-byte-equivalence tests against Python output.
- Roundtrip tests: build → parse → re-build = identical bytes.
- Edge cases: empty excise_stamps, dual_tax_mode None vs Some, discount=None.

---

## 10. Out of scope for W4-Z1 (defer markers)

- **SERVICE_IN/OUT/CASH_WITHDRAWAL** — PR-C scope, not Z1.
- **X_REPORT local short-path** — PR-B scope, not Z1.
- **Check-level discount** (Python `receipt.discounts`) — needs DTO extension.
- **Header/footer text lines** (`<L>`) — needs DTO extension.
- **TXAL=3 (absolute excise)** — needs spec clarification.
- **DPS live smoke** — W4-Z3 scope.
- **stage_sign canonical_doc_digest updates** — W4-Z2 scope (must happen after Z1 lands xml builder).
- **W3 DTO `tax_group_1` type fix** — depends on Q2 answer.

---

## Sign-off

This spec is the SOLE wire-shape contract for W4-Z1.  Any change to it requires explicit operator approval + plan-revision commit.  Golden tests pin the bytes.

Author: Claude (autonomous session 2026-05-26)
Verified against: dps_xml.py (Sprint 7 ground truth) + StringXML.cs (WebCheck production ground truth) + ФСКО v2.2.3 (memory `project_fsko_protocol_gaps`)
