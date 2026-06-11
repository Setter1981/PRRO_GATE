# OLE Methods Used By 1C Maria Integrations

**Status:** working reference  
**Date:** 2026-04-21  
**Sources:**

- legacy 1C module for `M301Manager.Application`
- 1C trade equipment API v2.9 module for `M304Manager.Application`
- local OLE documentation:
  `C:\Program Files (x86)\Resonance JSC\OLEManager\Resonance.OLEManager.xml`

This document lists OLE methods and properties that appear in the two provided
1C integrations. It is written as an implementation checklist for the Windows
1C behavior emulator and the Maria-compatible test double.

## Return Convention

The OLE XML documentation often leaves `<returns>` empty for commands that
print or configure the RRO. Both 1C integrations treat the returned value as:

- `0`: error; read `LastErrorMessage`
- non-zero / `1`: success
- string-returning methods use an empty string as error in the 1C code

The emulator should reproduce this practical convention even where the XML docs
do not spell it out. Several methods are intentionally implemented as stubs for
the pilot because the PRRO contour, not a physical RRO/KSEF queue, owns the real
state:

- `LockPrinter()` / `UnlockPrinter()`: no-op, return success, log the call.
- `SetDoubledTaxCalcMode(...)`: no-op, return success, log the requested tax
  ordering; actual tax semantics are carried by the following `FiscalLineEx`.
- `NextZNumber`: return `-1`, because the emulator cannot know the next real
  Z-report number in advance.
- `Settlement()` / `SettlementAsync()` / `AbortSettlementAsync(...)` /
  `SettlementState`: no-op/compatibility stubs for API v2.9 UI flows.

## Object And Connection

| Method / property | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `M301Manager.Application` | legacy module | COM ProgID | COM object or creation error | Legacy module creates this object through `СоздатьОбъект("M301Manager.Application")`. Default ProgID for the legacy contract. |
| `M304Manager.Application` | API v2.9 module | COM ProgID | COM object or creation error | API v2.9 module creates this object through `Новый COMОбъект("M304Manager.Application")`. Emulator should make ProgID configurable. |
| `InitEx(...)` | legacy module | In the sample: `4`, cashier name, password/fiscal value, `0`, client id `"RBK"`, host `"localhost"` | 1C expects success/error return | XML comment is malformed in the installed docs, but the method exists and is used for legacy startup. Log all arguments exactly. |
| `Init(portName, cashierName, rroPassword, showStatusDialogs)` | API v2.9 module | COM port, IP, host, or `tcp://host[:port]`; cashier; password; status dialog flag | API v2.9 code expects `1` on success, `0` on failure | Main connection method in the newer integration. After success the code calls `GetDocumentsInfoXML()` and `GetPrinterTime()` as connection validation. |
| `Done()` | both | none | success/error return | Closes the RRO connection. API v2.9 calls it after most operations; legacy calls it on shutdown and after reports. |
| `LockPrinter(timeoutMs)` | legacy module | timeout in milliseconds, legacy passes `10000` | success | Stub/no-op. OLE docs describe it as backward-compatible no-op. Keep it in logs to preserve call order. |
| `UnlockPrinter()` | legacy module | none | success | Stub/no-op. Keep it in logs to preserve call order. |
| `LastErrorMessage` | both | property read | string, then value is cleared by read | Error text after a failed method. Emulator should preserve meaningful errors because 1C displays them. |
| `ShowErrorMessages` | API v2.9, commented | boolean/int property | none | Commented in code. OLE docs say it enables UI error messages. Not required for automated replay. |
| `VerboseErrorMessages` | API v2.9, commented | boolean/int property | none | Commented in code. Not required for automated replay. |

## Receipt Lifecycle

| Method / property | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `OpenCheck(department)` | both | department name/id string. Legacy passes `"1"`; API v2.9 passes `ИдОтдела` | success/error return | Opens a fiscal sale receipt. |
| `OpenReturnCheck(department)` | both | department name/id string | success/error return | Opens a fiscal return receipt. For acceptance this should be preceded by return-source linkage. |
| `SetReturnCheckNumber(checkNumber)` | legacy module | integer original receipt number | success/error return | OLE docs require calling this before `OpenReturnCheck`. The legacy code uses hardcoded `100`; acceptance replay must use the real original WebCheck/source receipt number. |
| `SetReturnCheckNumberStr(checkNumbers)` | not called directly, required fallback | string with one or more original receipt numbers | success/error return | OLE string variant for return linkage. Use when source reference is non-numeric or contains several receipt numbers. |
| `CloseCheck()` | legacy module | none; payments added earlier by `AddPayment` | returns paid total in kopiykas or `0` on error per OLE docs | Legacy flow: add payments with `AddPayment`, optionally `AddSlip`, then close. OLE docs warn that `0` may mean printing failed or an error happened after printing; comparing `LastCheckNumber` before/after is safer. |
| `CloseCheckEx(cash, noncash1, noncash2, noncash3, rrn?)` | API v2.9 module | cash and cashless amounts in kopiykas. Code calls `CloseCheckEx(СуммаНал*100, СуммаБезнал*100, 0, 0)` | success/error return in 1C usage | API v2.9 flow closes and pays the receipt in one command. The installed XML marks this overload as ignored, but it exists and is used. |
| `AbortCheck()` | API v2.9 module | none | success/error return | Cancels an open receipt after line/close errors. Emulator should support this and mark the active receipt aborted. |
| `LastCheckNumber` | API v2.9 module; recommended by OLE docs | property read | last printed fiscal receipt number | API v2.9 reads it after `OpenCheck/OpenReturnCheck`; OLE docs recommend comparing it around close to prove printing succeeded. |
| `NextZNumber` | API v2.9 module | property read | emulator returns `-1` | The physical RRO can expose next Z-report number, but our emulator cannot know the real future Z number in advance. Treat as placeholder only; do not use it for acceptance assertions. |

## Fiscal Lines, Taxes, Excise

| Method | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `FiscalLineEx(name, qty, price, dividual, tax1, tax2, article, discountType, discountName, discountSum, barcode?)` | both | `name`; quantity; price in kopiykas; divisibility flag; tax group 1; tax group 2; article code or omitted; discount type `-1/0/1`; discount name; discount sum in kopiykas; optional barcode | line amount without imposed taxes or `0` on error | Core item registration method. OLE docs allow UKTZED as `UKTZED#Name` prefix. Both 1C fragments omit barcode. |
| `SetDoubledTaxCalcMode(tax1, tax2)` | both | two tax group numbers | emulator returns success | Stub/no-op in the emulator. Log the requested order; actual fiscal comparison should use the tax fields on the following `FiscalLineEx` and generated PRRO XML. |
| `AddExciseStamp(number)` | legacy module | excise stamp string | success/error return | Adds excise stamp to the next fiscal line. Must be called before `FiscalLineEx`. Missing from API v2.9 snippet; required for positive excise acceptance. |
| `AddExciseStamps(numbers[])` | not called directly, OLE-supported fallback | array of stamp strings | success/error return | OLE supports multiple marks for the next line. Useful if a normalized WebCheck sample has several marks per item. |

## Payments And Card Slip

| Method | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `AddPayment(sum, paymentType, extendedPaymentId?)` | legacy module | amount in kopiykas; payment type `0` cash, `1..19` cashless depending on RRO model; optional extended payment id | success/error return | Legacy module calls `AddPayment(card*100, 2)` and `AddPayment(cash*100, 0)`. API v2.9 does not use it; it pays through `CloseCheckEx`. |
| `AddSlip(paymentFormIndex, merchantID, terminalID, operationType, PAN, approvalCode, paymentSystem, transactionCode, fee?, cashierSign?, cardholderSign?)` | legacy module | card/acquirer fields; OLE docs also define fee and signature flags | success/error return | Adds card-payment requisites into the fiscal receipt. Legacy code passes 8 args: payment form `2`, bank/acquirer, terminal, operation, PAN, approval, payment system, RRN. |

## Reports, Service Operations, Cash Drawer

| Method | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `NullCheck()` | both | none | success/error return | Prints a zero receipt. Legacy uses it as `Начать_смену`; API v2.9 uses it for device test and explicit zero-receipt button. |
| `XReport()` | both | none | success/error return | Prints X-report without closing shift. |
| `ZReport()` | both | none | success/error return | Closes fiscal shift and prints Z-report. |
| `PeriodicalFiscalReportEx(firstZ, lastZ, printFullReport)` | API v2.9 module | first Z number; last Z number; `false/true` short/full report | success/error return | Prints fiscal period report by Z-report numbers. |
| `PeriodicalFiscalReportDateEx(dateFrom, dateTo, printFullReport)` | API v2.9 module | start date; end date; `false/true` short/full report | success/error return | Prints fiscal period report by dates. |
| `MoveCash(direction, amount, description?)` | both | direction `0` withdrawal / `1` deposit; amount in kopiykas; optional description | success/error return | Service cash operation. Both fragments use two arguments and multiply hryvnia by 100. |
| `OpenCashBox()` | legacy module | none | success/error return | Opens the cash drawer. Not present in the API v2.9 snippet. |
| `CheckCopy()` | API v2.9 module | none | success/error return | Prints one copy of the last receipt. OLE docs note Maria 304 allows no more than one copy. |
| `Feed(linesToFeed)` | API v2.9 module | number of paper feed motor steps, 0.125 mm each | success/error return | Paper feed. |

## Text Lines And Non-Fiscal Text

| Method | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `FreeTextLine(placeBeforeFiscalPart, printOnJournal, doubleStrike, text)` | API v2.9 module | place `1` before fiscal part / `0` after; print-on-journal flag; text style `0..3`; text | success/error return | Adds a text line to the fiscal receipt or service document. API v2.9 uses it in `НапечататьСтроки`. |
| `ClearFreeTextLines()` | API v2.9 module | none | success/error return | Cancels text lines added by `FreeTextLine`. API v2.9 calls it after text-line errors. |

## Information Queries

| Method / property | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `GetCheckResult()` | legacy module | none | semicolon-separated string; legacy takes text before first `;` as receipt number | Used after `CloseCheck()` and `MoveCash()`. This method is used by the legacy code even though the inspected XML docs do not expose its detailed contract. Emulator must return a stable raw string and log it. |
| `GetPrinterConfigXML(mode)` | legacy module | integer mode, legacy passes `1` | XML string | OLE docs show `<m301_printer_config><common SerialNumber=... FiscalNumber=... date=... time=... /></m301_printer_config>`. Legacy parses `date=` and `time=` by string search. |
| `GetDocumentsInfoXML()` | API v2.9 module | none | XML string or empty string on error | Returns document/check/report numbers. API v2.9 parses `last_check_num`, `fiscal_report_num`, and `fiscal_report_made`. OLE example also includes `last_doc_num`, `last_serv_doc_num`, and `article_mode`. |
| `GetCashInfoXML()` | API v2.9 module | none | XML string or empty string on error | API v2.9 parses cash movement fields: `rest`, `income`, `outcome`, `sales`, `return`, `total`, `check_income`, `check_outcome`. |
| `GetPrinterTime()` | API v2.9 module | none | string `yyyymmddhhmmss` or empty string on error | Used during connection validation. API v2.9 rejects connection when RRO time differs from 1C time by more than 5 minutes. |
| `SettlementState` | API v2.9 module | property read | emulator may return `0` or terminal stub state | Stub for API v2.9 UI compatibility. It does not represent PRRO transport state; acceptance must use gateway state instead. |

## KSEF / DPA Data Sending

| Method | Used by | Input | Returns / output | Description and emulator note |
|---|---|---|---|---|
| `Settlement()` | API v2.9 module | none | emulator returns `2` by default, or `1` if configured | Stub/no-op. In real RRO this sends KSEF documents to server; in PRRO pilot the gateway transport owns synchronization. |
| `SettlementAsync()` | API v2.9 module | none | emulator returns `1` | Stub/no-op that only satisfies the UI flow. Do not use it as proof of DPS synchronization. |
| `AbortSettlementAsync(waitIdle)` | API v2.9 module | boolean: `false` cancel only, `true` cancel and wait until idle | emulator returns a terminal stub state | Stub/no-op. 1C passes `Ложь`. |

## Methods Present In One Integration But Missing In The Other

| Capability | Legacy `M301` module | API v2.9 `M304` module | Pilot implication |
|---|---|---|---|
| Excise marks | `AddExciseStamp` before line | missing | API v2.9 path must be extended or emulator must mark excise samples unsupported. |
| Return source number | `SetReturnCheckNumber(100)` placeholder | missing | Positive return replay requires real source number before `OpenReturnCheck`. |
| UKTZED handling | builds `UKTZED#Name` itself | not visible | API v2.9 path must receive `UKTZED#Name` in `Наименование` or be extended. |
| Payment style | `AddPayment` then `CloseCheck` | `CloseCheckEx` | Emulator must support both close modes. |
| Card slip details | `AddSlip` | missing | API v2.9 path loses terminal requisites unless extended. |
| Connection style | `InitEx` | `Init` | Emulator should support both. |
| Reports by period | missing | `PeriodicalFiscalReportEx/DateEx` | Not needed for receipt acceptance, but needed for management form parity. |
| Settlement controls | missing | `Settlement`, `SettlementAsync`, state, abort | Stub for UI compatibility only; gateway/DPS state is authoritative in PRRO pilot. |
| Status/cash info | minimal / `GetPrinterConfigXML` | `GetDocumentsInfoXML`, `GetCashInfoXML`, `GetPrinterTime` | API v2.9 validates connection and displays cash movement; emulator needs these XML responses. |

## Minimum Emulator Surface For Pilot

For the first acceptance emulator, implement these as mandatory:

- object creation for configurable ProgID
- `Init` and `InitEx`
- `Done`
- `LockPrinter` and `UnlockPrinter` as no-op stubs
- `LastErrorMessage`
- `OpenCheck`
- `SetReturnCheckNumber` and `SetReturnCheckNumberStr`
- `OpenReturnCheck`
- `FiscalLineEx`
- `SetDoubledTaxCalcMode` as a logged no-op stub
- `AddExciseStamp`
- `AddPayment`
- `AddSlip`
- `CloseCheck`
- `CloseCheckEx`
- `AbortCheck`
- `MoveCash`
- `NullCheck`
- `XReport`
- `ZReport`
- `GetCheckResult`
- `GetPrinterConfigXML`
- `GetDocumentsInfoXML`
- `GetCashInfoXML`
- `GetPrinterTime`
- `LastCheckNumber`
- `NextZNumber` as placeholder `-1`

Implement the rest as second-priority parity methods:

- `CheckCopy`
- `Feed`
- `FreeTextLine`
- `ClearFreeTextLines`
- `PeriodicalFiscalReportEx`
- `PeriodicalFiscalReportDateEx`
- `Settlement` as no-op stub
- `SettlementAsync` as no-op stub
- `SettlementState` as no-op stub state
- `AbortSettlementAsync` as no-op stub
- `OpenCashBox`
