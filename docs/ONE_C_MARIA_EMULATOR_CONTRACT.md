# 1C Maria Emulator Contract

**Status:** draft extracted contract  
**Date:** 2026-04-21  
**Source:** provided 1C equipment module for `M301Manager.Application` and
local OLE docs from
`C:\Program Files (x86)\Resonance JSC\OLEManager\Resonance.OLEManager.xml`

This document captures the behavior that the Windows-side 1C emulator must
reproduce before real 1C is connected. The emulator does not need to implement
the full 1C runtime. It must reproduce the observable call sequence to the
Maria-compatible OLE/device layer.

## Device Object

The 1C module uses:

```text
Имя = "M301Manager.Application"
```

The installed OLE package also includes samples for `M304Manager.Application`.
For this pilot contract the default remains `M301Manager.Application`, because
that is what the provided 1C equipment module creates. The emulator should keep
the ProgID configurable so the same scenario pool can be replayed against
`M301Manager.Application` or `M304Manager.Application` on Windows.

Initialization in `Включить()`:

```text
СоздатьОбъект("M301Manager.Application")
InitEx(4, "Кассир", "1111111111", 0, "RBK", "localhost")
```

Shutdown in `Выключить()`:

```text
Done()
```

The emulator should make these calls explicit in the transcript even if they
are no-ops in a test double.

## OLE Compatibility Notes

The installed OLE XML documentation confirms the methods used by the 1C module.
Several methods have longer signatures in the OLE docs than in the 1C code; the
1C code relies on optional/default parameters exposed through COM.

| Method | OLE signature detail | 1C usage in provided module |
|---|---|---|
| `FiscalLineEx` | `FiscalLineEx(name, qty, price, dividual, tax1, tax2, article, discountType, discountName, discountSum, barcode)` | Omits `barcode` |
| `AddPayment` | `AddPayment(sum, paymentType, extended_payment_id)` | Omits `extended_payment_id` |
| `MoveCash` | `MoveCash(direction, amount, description)` | Omits `description` |
| `AddSlip` | `AddSlip(paymentFormIndex, merchantID, terminalID, operationType, PAN, approvalCode, paymentSystem, transactionCode, fee, printCashierSignaturePlaceholder, printCardholderSignaturePlaceholder)` | Omits `fee` and signature flags |
| `LockPrinter` / `UnlockPrinter` | documented as backward-compatible/no-op behavior | implement as success stubs and log to preserve 1C call sequence |
| `SetDoubledTaxCalcMode` | real RRO uses it to set tax ordering for the next line | implement as a logged success stub; tax semantics are verified on `FiscalLineEx` and generated PRRO XML |
| `NextZNumber` | physical RRO property for next Z number | emulator returns `-1`; do not use for acceptance assertions |
| `Settlement` / `SettlementAsync` / `SettlementState` | real RRO KSEF send controls | implement as compatibility stubs; gateway/DPS state is authoritative |

The OLE docs also expose:

- `SetReturnCheckNumber(number)`: sets the source receipt number for a return
  and must be called before `OpenReturnCheck`.
- `SetReturnCheckNumberStr(numbers)`: string form for one or more source
  receipt numbers.
- `SetDoubledTaxCalcMode(2, 1)`: documented as the required call before a
  fiscal line with 5% excise tax. The emulator records the call but does not
  perform separate tax-state mutation.
- `AddExciseStamp(mark)`: must be called before the fiscal line that consumes
  that excise mark.
- `GetPrinterConfigXML(1)`: returns XML with `common` attributes such as
  `FiscalNumber`, `date`, and `time`.
- `NullCheck()`: prints a zero receipt. The docs also have `OpenBusinessDay()`,
  but the provided 1C module uses `NullCheck()` in `Начать_смену`, so the
  emulator must reproduce the module behavior unless we intentionally test a
  corrected 1C integration later.

## High-Level Operations

| 1C function | Device calls |
|---|---|
| `Включить` | `CreateObject`, `InitEx(...)` |
| `Выключить` | `Done()` |
| `Начать_смену` | `LockPrinter(10000)`, `NullCheck()`, `UnlockPrinter()` |
| `Пробить_чек` | `ПечатьЧека(...)` sequence below |
| `Инкассировать` | `LockPrinter(10000)`, `MoveCash(...)`, result read, `UnlockPrinter()` |
| `Закрыть_смену` | `LockPrinter(10000)`, `ZReport()`, `UnlockPrinter()` |
| `Отчет` | `LockPrinter(10000)`, `XReport()`, `UnlockPrinter()` |
| `Открыть_ящик` | `LockPrinter(10000)`, `OpenCashBox()`, `UnlockPrinter()` |
| `Получить_статус` | currently mostly stubbed; returns success |
| `Получить_документ` | currently mostly stubbed; returns success |

## Sale And Return Receipt Sequence

`Пробить_чек()` calls `ПечатьЧека(Устройство)`.

For each receipt copy:

```text
LockPrinter(10000)
ПечатьШапкиЧека(...)
ПечатьСтрокиЧека(...) for each table row
ПечатьПодвалаЧека(...)
```

### Header

For fiscal receipts:

```text
if sale:
    OpenCheck("1")
else return:
    SetReturnCheckNumber(original_receipt_number)
    OpenReturnCheck("1")
```

Non-fiscal/precheck paths are mostly commented out and are not pilot-critical.

The literal `SetReturnCheckNumber(100)` in the provided 1C module should be
treated as a placeholder/smoke value. In acceptance replay, return receipts must
carry the original receipt number from WebCheck or from the prepared scenario.
If WebCheck provides multiple source receipt numbers, use
`SetReturnCheckNumberStr(...)`. A return without source linkage belongs in the
negative test pool, not in the positive acceptance pool.

### Line Registration

For each row:

```text
Наименование = left(trim(Товар.Наименование), 35)
ИмяФиск = trim(Товар.УКТЗЕД) + "#" + left(Наименование, 41)
          if УКТЗЕД is present
          else left(Наименование, 41)

Арт = Константа.ПоследнийАртикул
if Арт == 0: Арт = 1
Константа.ПоследнийАртикул += 1

Делимый = 1
кол = Количество * 1000
Цена = Цена * 100
```

Discount/surcharge conversion:

```text
line_adjustment = СуммаСкидкиНаТовар
                + СуммаСкидкиНаКоличество
                + СуммаСкидкиНаЧек

if line_adjustment > 0:
    Скидка = 0
    СКИМЯ = "Знижка"
    Суммаскидкикоп = line_adjustment * 100
elif line_adjustment < 0:
    Скидка = 1
    СКИМЯ = "Надбавка"
    Суммаскидкикоп = abs(line_adjustment) * 100
else:
    Скидка = -1
    СКИМЯ = ""
    Суммаскидкикоп = 0

if ((кол / 1000 * Цена) - Суммаскидкикоп) == 0:
    Суммаскидкикоп -= 1
```

Item classification is not inferable in a legally safe way from OLE alone.
The 1C algorithm has separate branches for `Товар.Алкоголь` and
`Товар.Цигарки`, and we should not guess them from the fiscal number or generic
receipt metadata. The normalized scenario input must therefore contain explicit
per-line flags:

```json
{
  "alcohol": true,
  "cigarettes": false,
  "excise_mark": "AA123456",
  "uktzed": "2203000100"
}
```

Samples missing this classification should either be manually mapped before
positive replay or skipped into a "needs classification" bucket. UKTZED and
excise marks are useful hints, but they do not replace the explicit
`alcohol`/`cigarettes` decision.

Fiscal line calls:

```text
if Товар.Цигарки == 1:
    FiscalLineEX(ИмяФиск, кол, Цена, Делимый, 4, 0, Арт,
                 Скидка, СКИМЯ, Суммаскидкикоп)

elif Товар.Алкоголь == 1:
    SetDoubledTaxCalcMode(2, 1)
    if АкцизнаяМарка != "":
        AddExciseStamp(АкцизнаяМарка)
    FiscalLineEX(ИмяФиск, кол, Цена, Делимый, 1, 2, Арт,
                 Скидка, СКИМЯ, Суммаскидкикоп)

else:
    FiscalLineEX(ИмяФиск, кол, Цена, Делимый, 3, 0, Арт,
                 Скидка, СКИМЯ, Суммаскидкикоп)
```

Important pilot implication:

- UKTZED is encoded in the fiscal item name as `UKTZED#Name`.
- Alcohol path calls `SetDoubledTaxCalcMode(2, 1)` before `FiscalLineEX`.
- Alcohol excise marks are sent with `AddExciseStamp(mark)` before the fiscal
  line.
- Cigarettes use `FiscalLineEX(..., 4, 0, ...)`.
- Ordinary goods use `FiscalLineEX(..., 3, 0, ...)`.

## Payment And Receipt Close

`ПечатьПодвалаЧека(...)` performs payments and closes the fiscal receipt:

```text
if ПолученоБезНал != 0:
    AddPayment(ПолученоБезНал * 100, 2)

if ПолученоНал != 0:
    AddPayment(ПолученоНал * 100, 0)
```

If there is a card transaction payload and non-cash amount is non-zero:

```text
ПараметрТранз = ЗначениеИзСтроки(СтрокаТранзакцииПлатежнойСистемы)

paymentsystem = ПараметрТранз.Получить("paymentsystem")
bankacquirer  = ПараметрТранз.Получить("bankacquirer")
approvalcode  = ПараметрТранз.Получить("approvalcode")
invoicenumber = ПараметрТранз.Получить("invoicenumber")
merchant      = ПараметрТранз.Получить("merchant")
pan           = ПараметрТранз.Получить("pan")
terminalid    = ПараметрТранз.Получить("terminalid")
Toper         = ПараметрТранз.Получить("Toper")
rrn           = ПараметрТранз.Получить("rrn")

AddSlip(2, bankacquirer, terminalid, Toper, pan,
        approvalcode, paymentsystem, rrn)
```

Then:

```text
CloseCheck()
GetCheckResult()
GetPrinterConfigXML(1)
UnlockPrinter()
```

The module parses:

- `GetCheckResult()` as semicolon-separated text, taking the part before `;` as
  receipt number.
- `GetPrinterConfigXML(1)` for `date=` and `time=` values.

The emulator should preserve both raw return strings in the transcript.

## Service In / Service Out

`Инкассировать()` maps service cash movement to:

```text
LockPrinter(10000)

if ТипИнкассации == Изъятие:
    MoveCash(0, Сумма * 100)
elif ТипИнкассации == Внесение:
    MoveCash(1, Сумма * 100)

GetCheckResult()
GetPrinterConfigXML(1)
UnlockPrinter()
```

Pilot mapping:

- `MoveCash(1, amount)` means service-in.
- `MoveCash(0, amount)` means service-out.

## Reports

Start shift check:

```text
LockPrinter(10000)
NullCheck()
UnlockPrinter()
```

Close shift:

```text
LockPrinter(10000)
ZReport()
UnlockPrinter()
Константа.ПоследнийАртикул = 500
```

X-report:

```text
LockPrinter(10000)
XReport()
UnlockPrinter()
```

## Required Emulator Input Model

The emulator should read selected receipt JSON samples and map them into this
1C-like input model:

```json
{
  "scenario": "sell_with_excise",
  "receipt_id": "webcheck__ksef_123",
  "fiscal_number": "4000162280",
  "operation_type": "SELL",
  "fiscal": true,
  "copy_count": 1,
  "cash_amount": "100.00",
  "card_amount": "215.00",
  "transaction": {
    "paymentsystem": "VISA",
    "bankacquirer": "BANK",
    "approvalcode": "123456",
    "invoicenumber": "INV-1",
    "merchant": "M-1",
    "pan": "************1234",
    "terminalid": "T-1",
    "Toper": "SALE",
    "rrn": "RRN-1"
  },
  "lines": [
    {
      "code": "SKU-1",
      "name": "Пиво",
      "uktzed": "2203000100",
      "quantity": "1.000",
      "price": "120.00",
      "sum": "120.00",
      "alcohol": true,
      "cigarettes": false,
      "excise_mark": "AA123456",
      "line_discount": "5.00",
      "quantity_discount": "0.00",
      "receipt_discount": "0.00"
    }
  ]
}
```

## Required Emulator Output

The emulator must be verbose enough for an operator to understand the run
without opening JSON.

Console output for each receipt:

```text
[001/042] sell_with_excise webcheck__ksef_123 FN=4000162280 total=315.00
  -> LockPrinter(10000)
  -> OpenCheck("1")
  -> SetDoubledTaxCalcMode(2, 1)
  -> AddExciseStamp("AA123456")
  -> FiscalLineEX(name="2203000100#Пиво", qty=1000, price=12000, tax=1, exciseTax=2, article=500)
  -> AddPayment(21500, 2)
  -> AddPayment(10000, 0)
  -> CloseCheck()
  <- OK 184 ms check_result="12345;..." state=ACK document_id=doc_...
```

Required `transcript.jsonl` event fields:

- `ts`
- `run_id`
- `scenario`
- `receipt_id`
- `fiscal_number`
- `operation_type`
- `call.method`
- `call.args_summary`
- `result.ok`
- `result.elapsed_ms`
- `result.return_value`
- `result.error`
- `artifacts.raw_frames`
- `artifacts.gateway_document_id`

Required `summary.json` fields:

- total/passed/failed/skipped
- counts by operation type
- counts by category
- counts by fiscal number
- list of failed receipt ids
- path to `transcript.jsonl`
- path to artifact directory

## Open Questions For Implementation

- Exact `GetCheckResult()` string shape returned by the Maria layer. The 1C
  code expects a semicolon-separated string and takes the first segment as the
  receipt number.
- Whether the pilot should keep the current `NullCheck()` behavior exactly or
  separately test `OpenBusinessDay()`. For the 1C behavior emulator, keep
  `NullCheck()` because it matches the supplied module.
- Whether the real return source number should be numeric-only
  `SetReturnCheckNumber(...)` or string-based `SetReturnCheckNumberStr(...)`
  for cases where WebCheck stores multiple or non-numeric references.
