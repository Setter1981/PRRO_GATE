# WebCheck COM-shim — pilot ingress spec (1С verbs → canonical)

**Date:** 2026-05-30 · **Status:** design spec · no code yet
**Decision:** [[project-o1-ingress-decision]] — pilot site is on WebCheck; ingress = a thin Windows **.NET COM shim**
presenting `WebCheck.ClassFiscal` / `AddIn.vk_WebCheck`, forwarding to the cross-platform gateway. 3rd front-end
**alongside `maria304_driver`**, producing the **same wire `CanonicalCommand`**. Does NOT change the spine.
**Sources:** decompile `…/webcheck_reverse/WebCheckMain/WebCheck/{ClassFiscal,StringXML}.cs`; gateway
`rust/prro/src/runtime/ingress/dto.rs` + `services/write_path/{types,stage_sign}.rs`; template
`rust/maria304_driver/src/{session/dispatcher,bridge/dto}.rs`; `docs/webcheck/WEBCHECK_1C_INTEGRATION.md`.

---

## §1 — Architecture: the shim is a keyless, stateful→stateless bridge

```
1С (Windows)  ── COM "WebCheck.ClassFiscal" ──►  WebCheck-shim (.NET COM, thin)
   verb(xml) ; then reads StatusBarXML()             │  parse verb XML → CanonicalCommand
                                                      │  POST /v1/ingress  +  GET /v1/status/{fn}
                                                      ▼
                                          gateway (Rust, cross-platform) ── DPS
```

Three properties define the shim:
1. **Keyless.** WebCheck's `SetSignSettings(keypath,keypass)` configures *local* signing — but in the gateway
   architecture the **gateway holds the keys and signs** (RS-Q2 `OperatorKeyLoader`/`SigningContext`). So the shim
   **never signs and never holds the key password**. `keypass` is a secret that must not leave the PC / not be
   logged. → security win: no fiscal key material in the Windows shim.
2. **Stateful→stateless.** WebCheck verbs are a per-FN *session* (`Initialization` binds an FN, verbs operate on it,
   `Finalization` unbinds; `GetCurrentStatus` reads). The gateway ingress is **stateless commands**. The shim holds
   the per-FN session locally (bound FN + the operator from `OpenShift`) and synthesizes status from gateway reads.
3. **Wire-shape producer.** The shim emits the **wire `CanonicalCommand`** (dto.rs:67) — identical target to
   `maria304_driver`. The gateway-side **wire→stage_sign payload conversion is shared** (RS-2's conversion layer,
   dto.rs:37-43); the shim does NOT build `CheckJson`/`ZReportJson` itself.

**COM contract (`ClassFiscal.cs`):** 1С calls a verb (`bool`), then reads `StatusBarXML()` (ClassFiscal.cs:1408).
Input = `<InputParameters><Parameters attr='..'/></InputParameters>` (case-insensitive; FiscalReceipt uses root
`<check>`). Output = `<OutputParameters><Parameters Err="0" .../></OutputParameters>` (all string attrs) — EXCEPT
`ReportZ`/X which return the **raw report XML** verbatim (ClassFiscal.cs:1411). On error, output appends
`ErrHelp="<msg>" version="<dll>"`.

---

## §2 — Verb → gateway action map (the 7 + 1)

| Verb (file:line) | Input attrs | Gateway action | `command_type` | Output (StatusBar attrs) |
|---|---|---|---|---|
| `SetSignSettings` (3501) | keypath, keypass *(secret, 5 case-spellings)* | **none** (keyless; record FN→key-ref locally or no-op) | — | (prior status) |
| `Initialization` (96) | fn *(numeric, len 10 → errCode 3)*, mode | **bind** session + `GET /v1/status/{fn}` | — | Err, TIN, FN, version, license, OfflineCount, Offline, OfflinePool, ApiVer |
| `GetCurrentStatus` (801) | fn | `GET /v1/status/{fn}` | — | Err, FN, ShiftNumber, LocalCheckNumber, Cashier, OfflineCount, Offline, OfflinePool, LastOfflineStart, BalanceOffline, ShiftStart, CashBalance, FiscalMode, LastFiscalNumber, CertExpireDate |
| `OpenShift` (289) | fn, OperatorID | `POST /v1/ingress` | **SHIFT_OPEN** | Err, FN, ShiftNumber, OperatorName |
| `FiscalReceipt` (904) | `<check …>` (see §3) | `POST /v1/ingress` | **SELL / RETURN / SERVICE_*** | Err, FN, **CheckID** (= DPS `fiscal_id`) |
| `ReportZ` (508) | fn, OperatorID, shiftCashInOut | `POST /v1/ingress` (+ CASH_WITHDRAWAL if `shiftCashInOut=1`) | **Z_REPORT** | **raw Z-report XML** (then next StatusBar = OutputParameters) |
| `Finalization` (227) | fn | **unbind** session (release binding) | — | Err, FN |
| `OnlineToOffline` (1752) | fn | `POST /v1/ingress` control (force offline) | *(force-offline)* | Err, FN, … |

`Initialization`/`Finalization`/`SetSignSettings`/`GetCurrentStatus` are **session/state**, not fiscal commands —
they never produce a `fiscal_documents` row. Only `OpenShift`/`FiscalReceipt`/`ReportZ`/`OnlineToOffline` hit
`/v1/ingress`.

---

## §3 — `FiscalReceipt` field map (`<check>` → `CanonicalCommand`)

Root `<check>` (names lowercased; values preserved — `RegUpLow:true`, ClassFiscal.cs:913). Parsed in `StringXML.cs`.

**Envelope:**
| WebCheck | → Canonical (dto.rs) | Notes |
|---|---|---|
| `/check/@fn` (StringXML.cs:627) | `fiscal_number` | 10-digit (code enforces len==10; instruction says 12 — **code wins**, flag) |
| `/check/@uuid` (:334) | `idempotency_key` | if absent → mint `webcheck:{fn}:{sha256(payload)}` (mirror maria304 dispatcher.rs:661). `checkUID` on → empty/dup uuid = errCode 88 |
| `/check/@operationtype` (:355) | `command_type` + `direction` | `0`→SELL/SALE, `1`(+`@idcancel`)→RETURN/RETURN, `±8`→SERVICE/open (sign-dependent, **ambiguous — flag**) |
| `/check/@idcancel` (op=1) | `return_check_number` | cancelled-receipt ref |
| *(no cashier on `<check>`)* | `cashier_id` | **source from the bound session's `OpenShift @OperatorID`** (not on the receipt) |
| *(no department)* | `department` | absent in WebCheck basic envelope → null/default |

**Goods — `<good>` (Dereban() StringXML.cs:1595, index map :1731):**
| WebCheck `@` | → `FiscalLine` (dto.rs:101) | Conversion |
|---|---|---|
| `@quantity` | `quantity_milli` | ×1000 (WebCheck qty = 3-decimal; must ≥0 → errCode 1017) |
| `@price` | `price_kopecks` | ×100 (hryvnia→kop; must >0 → errCode 12) |
| `@name` (+uktzed/excise appended) | `name` | |
| `@code` | `article_code` | |
| `@taxrate` (ABC/num) | `tax_group_1` (+`tax_group_2`/`dual_tax_mode` for ГА/ГБ/ДА) | via gateway W4-Z2a `driver_tax_mapping`/`translate_tax_group` (stage_sign.rs:1145). А=1(20%), Б=2(0%), В=3(7%), ГА=4(20%+excise5%), ГБ=5, ДА=6(fuel7.5%), **ДБ=7 rejected errCode 76**, Е=8(untaxed) |
| `@uktzed` | `uktzed` | |
| `@excisestamp` | `excise_stamps[]` | |
| `@barcode` | `barcode` | |
| *(derived: `@sum` ≠ qty×price)* | `discount{direction,name,amount_kopecks}` | WebCheck never sends an explicit discount — `Discount()` StringXML.cs:1578 computes it from `qty×price − sum` (positive→DISCOUNT, negative→MARKUP) |

**Payments — `<payments><payment>` (StringXML.cs:720):**
| WebCheck `@` | → `CanonicalPayment` (dto.rs:137) | Conversion |
|---|---|---|
| `@id` (required numeric) | `type` (CASH/CASHLESS_1..3) + payform | via FN pay-form directory (`get_PayName`/`get_PayISCASH` StringXML.cs:781/1007; `id=1`→CASH). **Needs the per-FN pay-form table** |
| `@sum` | `amount_kopecks` | ×100 |
| `@smb` | (change/given) | |
| `<l>` `@psnm`,`@rrn` (acquirer) | `acquirer_slip{payment_system, …}` | carried via DopTegE |

**Text lines — `<l>`:** `@up1..@upN` (max 50, before goods) → `header_lines[]`; `@dn1..@dnN` (after goods) →
`footer_lines[]` (stage_sign.rs:812). **Totals:** `totals.sale_kopecks`/`return_kopecks` = Σ good `@sum` (kop);
gateway cross-validates goods vs totals (dto.rs:184).

---

## §4 — `/v1/ingress` contract (what the shim POSTs) — same as maria304

`POST /v1/ingress/<source>` · body = `CanonicalCommand` (dto.rs:67-77):
```json
{ "schema_version": "1.0",            // exact; mismatch → MappingError (dto.rs:65,244)
  "fiscal_number": "4000000000",
  "command_type": "SELL",             // SCREAMING_SNAKE (dto.rs:92): SELL|RETURN|SHIFT_OPEN|SHIFT_CLOSE|
                                       //   X_REPORT|Z_REPORT|SERVICE_IN|SERVICE_OUT|CASH_WITHDRAWAL
  "idempotency_key": "webcheck:4000000000:<sha>",
  "cashier_id": "<operator from OpenShift>",
  "department": null,
  "return_check_number": null,
  "payload": { /* wire ReceiptPayload (dto.rs:115) — goods[], payments[], totals, dual_tax_mode */ } }
```
**Response** = `CanonicalResponse` (dto.rs:79): `{ ok, document_id, fiscal_id, fiscal_ts, document_state,
sale_total_kopecks, return_total_kopecks }`.

**New endpoint the shim ALSO needs:** `GET /v1/status/{fn}` — for `Initialization`/`GetCurrentStatus`, exposing
shift/node state (ShiftNumber, LocalCheckNumber, Cashier, Offline counts, ShiftStart, CashBalance, FiscalMode,
LastFiscalNumber, CertExpireDate). This is a **read API the gateway does not have yet** (RS-2 adds the POST
ingress; this status GET is an additional surface).

---

## §5 — Response mapping (`CanonicalResponse` → `StatusBarXML`)

| Gateway | → WebCheck StatusBar |
|---|---|
| `ok=true` & `fiscal_id` present | `Err=0_FN=<fn>_CheckID=<fiscal_id>` (matches ClassFiscal.cs:1084, CheckID = DPS receipt #) |
| `ok=false` | `Err=<mapped_code>_ErrHelp=<document_state/msg>_version=<shim_ver>` |
| Z_REPORT | the **Z-report document body XML** verbatim (gateway must return it — see §6) |
| `fiscal_ts` | WebCheck fiscal date/time |

**Error code mapping** (gateway refusal/state → WebCheck code; the shim translates): no open shift → **8**; FN not
registered → **31/1001**; offline limit → **42**; access denied → **1**; different FN → **2**; bad FN format →
**3**; no connection → **4**; already-open-shift → **1006**; cert near-expiry → **66**; time skew ±60min → **77**;
first-shift-must-be-online → **89**; bad operationtype → **19**; ДБ tax → **76**. `Err=0` on success.

---

## §6 — Open items / gaps (resolve before coding)

1. **`GET /v1/status/{fn}` read endpoint** — does not exist; the shim needs it for Initialization/GetCurrentStatus.
   Scope it with the spine (RS).
2. **Z-report body in the response** — `ReportZ` must return the raw Z-XML via StatusBarXML; `CanonicalResponse`
   currently carries `document_id`/`fiscal_id`, **not** the Z document body. The gateway Z_REPORT response must
   include the rendered Z-report XML (or the shim fetches it via the status/document API).
3. **`cashier_id` source** — not on `<check>`; comes from the session's `OpenShift @OperatorID`. The shim must carry
   it in session state and stamp every receipt.
4. **`opening_sum_kop`** — `OpenShift` has no opening-cash attr → `ShiftOpenJson.opening_sum_kop` defaults 0 unless
   the shim reads a cash-balance attr.
5. **Z `sell_count`/`return_count`** — absent from any WebCheck verb; WebCheck computes locally. **Gateway derives
   from the ledger** (dto.rs:32-35) — so the shim need not supply them, but the gateway Z path must (stage_sign.rs:1014
   currently hardcodes empty — a gateway gap).
6. **Per-FN pay-form directory** — `<payment @id>` → CASH/CASHLESS needs the FN's pay-form table (WebCheck loads it
   locally). Decide: shim ships a static map, or the gateway resolves `@id`→type.
7. **`operationtype ±8`** — service/open branch sign-dependent (StringXML.cs:368); pin the exact SERVICE_IN/OUT/open
   mapping.
8. **FN length** — code enforces 10 digits; instruction says 12. Code wins; confirm.
9. **Wire→stage_sign conversion** is RS-2's shared layer (dto.rs:37-43) — the shim produces wire shape; do NOT
   duplicate the conversion in the shim.

---

## §7 — What this does NOT change

The gateway **core, `/v1/ingress` canonical contract, RS spine, HB5, refined-A′** are all unchanged. The shim is a
new **leaf adapter** (separate Windows .NET project), the second front-end against the same canonical core. The only
gateway-side additions this ingress *implies* are the **`GET /v1/status/{fn}`** read endpoint (§4) and the **Z-report
body in the response** (§6) — both small, and useful for any ingress, not just WebCheck.

*Grounded against the decompile + gateway DTOs at file:line. The shim's job is parse-XML → wire-CanonicalCommand →
POST, and CanonicalResponse → StatusBarXML — no fiscal logic, no keys.*
