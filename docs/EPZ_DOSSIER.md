# EPZ Increment — Design Dossier

**Op:** видача готівки за ЕПЗ (cash advance / cashback against a payment card — cashier
dispenses cash, customer pays by card; cash LEAVES the drawer).
**Position in roadmap:** next fiscal increment after L3 (service-io). Pilot-scope
(operator: «выдачу ДС тоже крутим, на пилоте есть»).
**Author:** architect. **Base branch for implementer: `origin/main` = `9062550`**
(local `main` is 8 commits stale; working copy is on `fuzzer-tier1-dossier` which
PRE-dates L0/L1/L3 — do NOT branch from the working tree).

---

## 0. TL;DR — what this increment does

Flip the already-scaffolded, fail-closed `CashWithdrawal` (EPZ) operation to a
**first-class, fully-wired receipt type**, bimodal (online + offline), with:

1. a distinct DPS wire receipt `operationtype='-8'` (NOT a Sell, NOT a service check);
2. the `− epz_out` term wired into the cash-on-hand ledger;
3. **guard-3c** (INV-21 готівка≥0): refuse fail-closed when the EPZ cash-out sum
   exceeds cash-on-hand (WebCheck `errCode 47`);
4. the Z-report `<EPZ EPC EPCS EPSM>` section populated (STOP-S2 coupling — same PR);
5. z-quiescence: EPZ docs block a Z-close while non-terminal;
6. fuzzer alphabet: `Op::OnlineEpz/OfflineEpz` + `apply_epz`.

**EPZ is NOT a copy of ServiceOut.** Different DPS operationtype, different receipt
body (card `<L>` + card `<payments>` legs), different Z section (`<EPZ>` not `<IO>`).
Copying the L3 service-check path would be a fiscal-correctness error.

---

## 1. Fiscal reality (what actually happens)

A customer at the POS asks for cash. The cashier charges the customer's **card** for
sum `X` (card leg, money into the merchant's acquiring account) and hands the customer
`X` in **cash** from the drawer. Net effect on the physical cash drawer: **−X**.

This is a real, regulated PRRO operation with its own DPS document type. It is fiscally
distinct from `SERVICE_OUT` (службова видача — operational removal of cash, e.g. to a
safe/collector, with no counterpart card transaction).

Legal / limits (WebCheck-grounded):
- **50 000 UAH cap** per operation on `DopNal/AllowableCash` (`All.cs:877-886`) —
  hard `if(>50000)=50000`. (Reconcile with our pre-pilot 50k hardcode, gap #5.)
- The drawer can never go negative (guard-3c, below).

---

## 2. Authoritative DPS wire — CORRECTED to our compact dialect

> **⚠ CORRECTION (implementer-caught, architect-verified):** `ClassFiscal.EPZtoCash`
> (`ClassFiscal.cs:1394-1399`) builds the VERBOSE `<check operationtype='-8'>` form — but
> that is WebCheck's **COM/input layer, NOT the DPS wire.** WebCheck `StringXML`
> transforms it: `num17 = Math.Abs(operationtype) = abs(-8) = 8` (`StringXML.cs:917-931`)
> and emits **`<C T='8'>`** in the same **compact `<RQ><DAT><C T=..>` dialect** our system
> already uses (verified `xml/mod.rs`: Sell=`<C T="0">`, Return=`T="1">`, service-io=`T='2'`,
> ShiftOpen=`T="108">`, B10=`T="109/110">`; ZERO `operationtype`). No `<C T='8'>` collision
> (8 is free; ShiftOpen is 108). **EPZ = `<C T='8'>` — NOT verbose `operationtype='-8'`.**

Authoritative EPZ DPS wire (compact; new `emit_epz_check` alongside `emit_service_check`):

```xml
<RQ V='1'><DAT FN='{fn}' TN='{tn}' DI='{lnd}' ZN='0' V='1'>
  <C T='8'>
    <P N='1' C='{code}'
       NM='ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ ДЕРЖАТЕЛЮ ЕЛЕКТРОННОГО ПЛАТІЖНОГО ЗАСОБУ'
       SM='{sum}' .../>                       <!-- good line — NO TX= (tax skipped, StringXML.cs:1424-1427) -->
    <M N='2' T='0' NM='{card payname}' SM='{sum}'
       PA=.. PB=.. PC=.. PD=.. PE=.. PF=.. PSNM=.. RRN=../>   <!-- CARD leg, T='0'; slip attrs already on CheckPayment (mod.rs:642-663) -->
    <E .../>
  </C>
  <TS>{ts}</TS>
</DAT><MAC .../></RQ>
```

**`<C T='8'>` is THE EPZ identity on the wire.** No `<TX>` element (EPZ is not a VAT good —
`StringXML.cs:1424-1427` `break` before the tax loop). The card `<M>` leg is `T='0'` and
carries the acquirer-slip attrs (PA/PB/PC/PD/PE/PF/PSNM/RRN — **already modeled on
`CheckPayment`**, but currently DEFERRED/fail-closed at ingress, see §8.4). The cash-out is
a LEDGER effect only — there is NO cash `<M>` line. The verbose `<L>`/`paymentid≥2` fields
below map onto the compact `<M>`/`<P>` (paymentid≥2 = card form for the `<M>`).

**Element-by-element:**

| Part | Meaning | Notes |
|---|---|---|
| `operationtype='-8'` | THE EPZ operation type | distinguishes EPZ from Sell/Return/service; DPS code `-8` = «видача епз» (`All.cs:1660`) |
| `<checkhead><ver>1</ver>` | minimal head | |
| `<L .../>` | EPZ/card data element | carries the card-transaction fields |
| `paymentid` | payment-form id | **MUST be ≥ 2** (`ClassFiscal.cs:1377`: «Тип paymentid не может быть меньше 2» → errCode 94). Non-cash form (card/EPI). This is the card payment form. |
| `pf` | (optional) extra field | absent-tolerant (`cs:1338`) |
| `sum` | operation amount (kop/UAH) | same value used in `<L>`, `<payments>`, `<goods>` |
| `pa pb pc pd pe` | card/acquirer data | `pd` = masked PAN (printed as «ЕПЗ: {PD}», `PrintExportCheck.cs:1873`); `pa/pb/pc/pe` = acquirer/payment-system/terminal/auth fields |
| `psnm` | payment system name | |
| `rrn` | retrieval reference number | the card transaction RRN |
| `<payments><payment id=paymentid sum/>` | **CARD leg** | payment recorded under the **card** form (`id=paymentid≥2`), value `sum` — the customer's card is charged |
| `<goods><good ... taxrate='2'/>` | the cash-advance line item | fixed name string above; `taxrate='2'` (⚠ confirm tax-group code vs our tax table — cash advance is not a VAT-able good) |

**Card-leg resolution (the open question from fact-finding):** the receipt's payment
leg is a **card** payment (`paymentid ≥ 2`), NOT a cash entry. The cash-out (drawer −X)
is implicit in `operationtype='-8'`; it is a **ledger effect**, not a `<payments>`
cash line. Consequence: **EPZ does NOT add to the cash payform total in the Z `<M>`
breakdown** — it is a card payform for turnover, while separately decrementing the
cash drawer via the `− epz_out` ledger term.

---

## 3. Ledger effect (cash-on-hand)

WebCheck `Nal()` (`All.cs:431-462`) is the authoritative cash-on-hand formula:

```
cash = sell_cash − return_cash + (SMI − SMO) − epz_out
```

`epz_out` = Σ of EPZ receipt sums in the shift. `NalOld()` (`All.cs:399`) is the
pre-EPZ version (same minus the `− num5` EPZ term). This confirms our reserved
`derive_cash_on_hand` slot (on `origin/main`, `cash_ledger.rs:64` comment already says
«EPZ … its term (−epz_out) is 0 for the current contour»).

**Change:** `derive_cash_on_hand` gains a 6th parameter `epz_out_kop` and the term
`− epz_out_kop`; an `aggregate_shift_epz` helper sums EPZ docs
(`state IN ('ACK','OFFLINE_LOCAL_ACK')`, `doc_type='CASH_ADVANCE_EPZ'`), mirroring
`aggregate_shift_service_io`. Callers updated (3 sites on `origin/main`:
`cash_ledger.rs:282`, `:389`, `stage_acquire.rs:802`).

---

## 4. Guard-3c (INV-21 готівка≥0) — confirmed verbatim

WebCheck fail-closes EPZ when the cash-out exceeds cash-on-hand
(`ClassFiscal.cs:1385-1391`):

```
if (StrToDouble(sum) > All.Nal()) {
    ErrHelp = "Помилка! У касі немає необхідної суми.";
    ErrCode = 47;              // ← the готівка≥0 fiscal refusal
    return false;
}
```

This is exactly the INV-21 «готівка не в минус» guard, `errCode 47`. Mirror the L3
guard-3b (ServiceOut) shape:
- **guard-3c pre-inbox** in `convert.rs`: row-less refusal (`CashInsufficient`) when
  `epz_sum > cash_on_hand`;
- **guard-3c in-lease** in `stage_acquire.rs:802` region: re-check under lease
  (TOCTOU-safe), online-only, pre-mint (mirrors guard-3b for `DocType::ServiceOut` →
  add `DocType::CashAdvanceEpz`).

`epz` joins the cash-out set `{Return, ServiceOut, Epz}` at both guard sites.

---

## 5. Z-report `<EPZ>` section (STOP-S2 coupling)

The Z struct + emitter **already exist** on `origin/main`:
- `ZReportEpzTotals { epc: i64, epcs: i64, epsm: i64 }` (`xml/mod.rs:750-757`)
- emitter `<EPZ EPC=… EPCS=… EPSM=…>` (`xml/mod.rs:1288-1300`)
- `ZReportPayload.epz: Option<ZReportEpzTotals>` (`xml/mod.rs:690-693`) — **always
  `None` today** (the population path is the gap).

WebCheck emits (`FormDate.cs:436`, `StringXML.cs:2398`):
```
<EPZ EPC='{count}' EPCS='0' EPSM='{total_sum}'></EPZ>
```
- `EPC` = count of EPZ operations in the shift
- `EPCS` = **hardcoded `0`** in WebCheck (⚠ decision: match verbatim `0`, or compute
  successful count — recommend match `0` for byte-parity until proven otherwise)
- `EPSM` = total EPZ sum (kop)

**STOP-S2 coupling pin (`z_builder.rs:33-55`, `FULL_Z_SURFACE_READY`):** the pin names
BOTH the IO (service-io) and EPZ (card/acquirer_slip) Z-halves. Relaxing the EPZ
ingress guard WITHOUT populating `<EPZ>` in the SAME PR re-opens the under-reporting
hazard. → **populating `ZReportPayload.epz` from shift EPZ docs is mandatory in this
increment's PR.**

---

## 6. Seam map (touch-points on `origin/main` 9062550)

EPZ is scaffolded fail-closed; the increment relaxes the seam exactly as L3 did for
service-io. Every touch-point (implementer resolves exact line numbers against
`origin/main`):

| # | File | Current | Change |
|---|---|---|---|
| a | `db/models/enums.rs` (`str_enum! DocType`) | `CashWithdrawal => "CASH_WITHDRAWAL"` exists | add `CashAdvanceEpz => "CASH_ADVANCE_EPZ"` (or reuse `CashWithdrawal` — see §8 decision) |
| b | `runtime/ingress/dto.rs` (`CommandType`) | `CashWithdrawal` variant | add EPZ `CommandType` + `CommandType→DocType` map + `handler.rs` string name |
| c | `runtime/ingress/policy.rs` `classify_command` | `CashWithdrawal → Unsupported` | EPZ → `Signable` (exhaustive match — compile-forces the decision) |
| d | `xml/mod.rs` | EPZ check body builder absent | build `<check operationtype='-8'>` + `<L …/>` + card `<payments>` + fixed `<goods>` (§2). New `EpzCheckPayload` (card fields pa..pf, psnm, rrn, paymentid). |
| e | `services/cash_ledger.rs:66` `derive_cash_on_hand` | 5-param, EPZ=0 | +`epz_out_kop`, `− epz_out`; +`aggregate_shift_epz`; fix 3 callers |
| f | `services/write_path/stage_acquire.rs:~802` | guard-3a/3b in-lease | +guard-3c (Epz in cash-out set, online-only, pre-mint) + EPZ shift-state arms |
| g | `runtime/ingress/convert.rs` | guard-3a/3b pre-inbox; `aggregate_zreport` Sell/Return only | +guard-3c pre-inbox; +EPZ canonical build; **populate `ZReportPayload.epz`** from EPZ docs |
| h | `services/write_path/stage_sign.rs` | EPZ in `UnsupportedDocType` arm | give EPZ a supported `WireArtifactKind` (signable check) |
| i | `services/write_path/stage_offline_ack.rs:~294` (9-variant tripwire @ :315) | EPZ matched | wire EPZ offline-ack; update the «all N variants» tripwire comment |
| j | `db/repositories/fiscal_documents.rs` z-quiescence (`:823` on 9062550) | `IN ('SELL','RETURN','SERVICE_IN','SERVICE_OUT')` | add `'CASH_ADVANCE_EPZ'` |
| k | `tests/invariant_fuzzer/op.rs` + `model.rs` | `apply_service_io`, EPZ=0 | `Op::OnlineEpz/OfflineEpz` + `apply_epz` + `− epz_out` in model + z-quiescence-blocker includes EPZ |

---

## 7. Bimodality (online + offline)

Operator requirement: внесення/видача/Z/X/**EPZ** all bimodal. EPZ reuses the generic
offline lane exactly as L3:
- **online:** `run_staged` → sign `<check operationtype='-8'>` → send → ACK.
- **offline:** `stage_offline_ack` durable `OFFLINE_LOCAL_ACK` → drain (W9b) → DPS.
- guard-3c is **online-only in-lease** (mirrors guard-3b); the offline lane relies on
  the pre-inbox guard + the local cash ledger (offline EPZ still checks
  `epz_sum ≤ cash_on_hand` at ingress, using the durable local ledger). Offline cash-out
  can only proceed if prior offline cash-in / opening anchor covers it (same reasoning
  as offline ServiceOut, `model.rs:493` note).

---

## 8. Open decisions (operator / architect)

1. **DocType identity:** new `CashAdvanceEpz => "CASH_ADVANCE_EPZ"` vs reuse the existing
   `CashWithdrawal` variant. **Recommend NEW variant** — `CashWithdrawal` is the current
   fail-closed placeholder; a clean EPZ doc-type keeps the ledger/z-quiescence/aggregation
   filters unambiguous and avoids overloading a name. (Decision needed before contract V0.)
2. **`EPCS` in `<EPZ>`:** match WebCheck's hardcoded `0`, or compute successful count.
   **Recommend `0`** (byte-parity; revisit if DPS rejects).
3. **`taxrate='2'`** on the `<goods>` line — confirm against our tax-code table (cash
   advance is not a VAT good). Pin the exact value the DPS expects; do NOT guess.
4. **Card fields source — RESOLVED (already plumbed):** the EPZ card requisites are
   ALREADY modeled on `origin/main`: `dto.rs:301 acquirer_slip: Option<AcquirerSlip>`
   (with `pan`, `payment_system`, … `dto.rs:322-324`) and `xml/mod.rs:632 CheckPayment`
   carries EPZ slip attrs PA/PB/PC/PD/PE/PF/PSNM/RRN (`mod.rs:642-663`, «W4-Z1»). Today
   an `acquirer_slip`-carrying cashless payment is **fail-closed** at
   `convert.rs:775 → ACQUIRER_SLIP_DEFERRED` (422, `handler.rs:1486`). INV-6 concern is
   therefore SATISFIED — the increment reuses these field structures for the EPZ `<L>`
   element; no new adapter plumbing for card fields. ⚠ Note the two EPZ surfaces are
   distinct: (i) `acquirer_slip` on a card payment of a normal sale (national-cashback
   style, separate backlog) vs (ii) the standalone cash-advance `operationtype='-8'`
   receipt (THIS increment). Both feed the shared Z `<EPZ>` section.
5. **`paymentid ≥ 2` enforcement:** fail-closed input guard (errCode 94 analog) — belongs
   with the L5 input-guard family or inline here. Recommend inline (EPZ-specific).

---

## 9. Fuzzer-impact (mandatory per rule «каждая новая фича → проверка надо ли допиливать фаззер»)

EPZ adds a new cash-out op with a new Z section and a new guard → the fuzzer MUST track it:
- **Alphabet:** add `Op::OnlineEpz(DpsScript)` + `Op::OfflineEpz` (mirror Sell/Return
  online-carries-script / offline-none).
- **Model:** `apply_epz` mirroring `apply_service_io(is_out=true)` but: (a) subtracts
  `epz_out` from the model's cash-on-hand; (b) guard-3c refusal (`NoMutation`) when
  `epz_sum > cash_on_hand`; (c) EPZ is **card** payform for turnover, cash-drawer −X.
- **z-quiescence:** `has_z_quiescence_blocker` must count non-terminal EPZ docs (else a
  Z could close over an in-flight EPZ — the exact class the L3 fuzzer caught for
  service-io, PR #258 `9f9151e`).
- **Teeth:** revert guard-3c → a seeded harness that drives EPZ-over-drawer must go RED;
  revert the z-quiescence EPZ inclusion → a seeded `[OfflineEpz, ZReport]` must go RED.
- **Known-red until wired:** until `apply_epz` lands, mark the EPZ fuzzer arm known-red.

Map of remaining uncovered surface: `docs/FUZZER_TIER2_RAGE_DOSSIER.md`.

---

## 10. Increment decomposition (slices → contract)

- **V0 (RED-first pins, TEST-ONLY):** doc-type + wire pin (1-byte `operationtype='-8'`),
  guard-3c pin (epz_sum>cash → 422 CashInsufficient pre-inbox, errCode-47 analog),
  ledger pin (`− epz_out`), Z `<EPZ>` populate pin, z-quiescence pin.
- **V1 core (online):** enums (a,b), policy flip (c), wire `<check -8>` (d), ledger (e),
  guard-3c in-lease (f), convert guard + EPZ canonical + Z `<EPZ>` populate (g), sign (h),
  z-quiescence (j).
- **V2 offline:** stage_offline_ack (i) + drain + offline guard-3c-at-ingress + bimodal e2e.
- **V3 fuzzer:** op.rs + apply_epz + z-quiescence-blocker + teeth (k) — TEST-ONLY.

STOP-S2 forces (g)'s Z `<EPZ>` populate INTO the same PR as the (c) policy flip.

---

## 11. Invariant check

- **INV-6** (full canonical payload): EPZ `<L>` carries all card fields — adapter must
  build them, not summarize (open decision §8.4).
- **INV-21** (готівка≥0): guard-3c, dual-site, errCode-47 — verbatim WebCheck.
- **No-network-in-txn / short-txn:** guard-3c reads the cash ledger inside the lease
  (same as guard-3b) — no network.
- **z-quiescence** (#192/P1 pin): EPZ added to the non-terminal blocker set.
- **STOP-S2:** Z `<EPZ>` populated same PR as ingress relaxation.
- **advance-at-SEND / D2:** EPZ online issuance advances the chain at Sending→Sent like
  any signable receipt (no special-casing).

## 12. Known risks / not done

- Card-field ingress plumbing (§8.4) — needs the Maria/REST adapter to carry pa..rrn.
- `taxrate='2'` tax-group confirmation (§8.3).
- Offline EPZ cash-sufficiency uses the durable local ledger — same trust boundary as
  offline ServiceOut (documented, not new risk).
- EPZ live-DPS wire not yet exercised (needs key + test-FN — same gate as the rest).
