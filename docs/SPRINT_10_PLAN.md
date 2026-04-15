# Sprint 10 — Cash Balance, Payments, X-Report

**Дата:** 2026-04-14
**Scope:** Online pilot only. Offline lifecycle = Sprint 11 (separate).

---

## Контекст

Після Sprint 9 (фіскальна коректність XML) та post-review fixes (lp3/4wi/r2c) залишився останній функціональний блок для online пілоту: облік готівки, решта, заокруглення, типи оплат, X-звіт.

**Pilot profile:** 50 точок / 70 кас, retail + HoReCa, WebCheck → PRRO_GATE, 1С інтеграція.

---

## Scope policy decisions

### 1. Online-first

Sprint 10 delivers online pilot accounting. Offline full lifecycle (GO_OFFLINE, ASK_CODES, offline sync, OFFLINE_LOCAL_ACK state fix) remains Sprint 11 scope. No offline delivery or acceptance in Sprint 10.

**Future compatibility note:** cash balance aggregation query should use `state IN ('ACK', 'OFFLINE_LOCAL_ACK')` from the start so Sprint 11 does not require rework. This is a defensive code choice, not an offline feature delivery.

### 2. Payment type policy

**Unknown/non-configured payment type for this PRRO = explicit reject before sign/send.**

- `CASH` is the only hardcoded canonical type (always `T="0"`, `NM="Готівка"`)
- All other payment types must be configured in `payment_type_definitions` for the fiscal number
- If POS sends a payment type not found in the table → `INVALID_RECEIPT_DATA` reject before signing
- No silent fallback to `T="2"` for unknown types
- This prevents incorrect fiscal classification of payment instruments

### 3. X-report: read model vs domain operation

Sprint 10 X-report is a **read-only operational endpoint** that aggregates current shift data and returns JSON. It is:
- NOT a fiscal document submitted to DPS
- NOT a shift-close or shift-state-change operation
- NOT a replacement for the existing `OperationType.X_REPORT` domain operation

If a full domain X_REPORT contour is needed (as a fiscal document type with DPS submission), that is a separate follow-up.

### 4. Cash balance source of truth

Two distinct concepts:
- **Current shift cash balance** = always derived on the fly from `fiscal_documents` for the active shift. Not a persisted running counter. This is the authoritative value for guards and X-report.
- **`node_state.last_cash_balance`** = carry-over snapshot, written ONLY at Z_REPORT boundary. Used ONLY to populate SHIFT_OPEN `<O SM="...">` when `cash_balance_mode = 'preserve'`. Not a replacement for aggregation. Not a universal running counter.

### 5. Rounding policy

`rounding_enabled` is a local per-FN gate that enables/disables the rounding mechanism in the gateway. When enabled:
- Gateway applies rounding per Постанова НБУ №115 від 08.09.2025 rules
- Gateway returns `rounded_sum` and `rounding` in REST response so POS can synchronize

Enabling the flag alone does not determine legal applicability. Actual rounding depends on payment context (cash only), receipt context, and current regulatory policy. The gateway implements the mechanical rule; the operator/accountant decides when to enable it.

---

## Step 1: Migrations

### 1.1 `sql/009_cash_balance.sql`

```sql
ALTER TABLE node_state ADD COLUMN last_cash_balance INTEGER NOT NULL DEFAULT 0;
ALTER TABLE node_state ADD COLUMN cash_balance_mode TEXT NOT NULL DEFAULT 'preserve';
ALTER TABLE node_state ADD COLUMN rounding_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE node_state ADD COLUMN print_zero_change INTEGER NOT NULL DEFAULT 0;
```

| Поле | Тип | Опис |
|------|-----|------|
| `last_cash_balance` | INTEGER | Carry-over snapshot: cash balance at last Z_REPORT (kopecks). Written at Z_REPORT only. |
| `cash_balance_mode` | TEXT | `preserve` = carry over last_cash_balance to next shift, `reset` = 0 after Z |
| `rounding_enabled` | INTEGER | 0/1 — local gate for cash rounding |
| `print_zero_change` | INTEGER | 0/1 — include "change: 0" in response when change is zero |

### 1.2 `sql/010_payment_type_definitions.sql`

```sql
CREATE TABLE payment_type_definitions (
    fiscal_number    TEXT NOT NULL,
    type_code        TEXT NOT NULL,
    type_group       INTEGER NOT NULL,
    name             TEXT NOT NULL,
    is_active        INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (fiscal_number, type_code)
);
```

`type_group` per ФСКО: 0 = готівка, 1 = інший засіб, 2 = безготівковий платіжний інструмент.

---

## Step 2: Cash Balance Calculation

### `_get_shift_cash_balance(conn, shift_id) -> int`

Derived from acknowledged documents in current shift:

```
balance = 0
+ SERVICE_IN: sum(service_sum)
- SERVICE_OUT: sum(service_sum)
+ SELL: sum(cash payment amounts)
- RETURN: sum(cash payment amounts)
- CASH_WITHDRAWAL: sum(cash_withdrawal_sum)
```

**Source:** `fiscal_documents WHERE shift_id = ? AND state IN ('ACK', 'OFFLINE_LOCAL_ACK')`

Note: `OFFLINE_LOCAL_ACK` included defensively for Sprint 11 compatibility. In Sprint 10 online scope, only `ACK` documents exist.

### Carry-over at shift boundaries

At Z_REPORT: write final balance to `node_state.last_cash_balance`.
At SHIFT_OPEN:
- `cash_balance_mode = 'preserve'` → `SM = node_state.last_cash_balance`
- `cash_balance_mode = 'reset'` → `SM = 0`

---

## Step 3: Guards

### SERVICE_OUT > cash balance → reject before sign

```
current_balance = _get_shift_cash_balance(conn, shift_id)
if service_sum > current_balance → reject
```

### CASH_WITHDRAWAL > cash balance → reject before sign

Same principle. Cash cannot go negative.

---

## Step 4: Решта (RM)

### Formula

```
change = sum(all payments) - total_sum
```

Rules:
- Sum of non-cash payments cannot exceed total_sum
- Change is always cash (overpayment is always from cash portion)
- `RM` attribute only on cash `<M>` (T="0")
- If only card payment → no RM
- REST response: include `change` if > 0 or if `print_zero_change = 1`

---

## Step 5: Заокруглення (SMP)

### Постанова НБУ №115 від 08.09.2025

From 01.10.2025 — round to nearest 50 kopecks:

| Last kopecks | Round |
|-------------|-------|
| 01-24 | ↓ .00 |
| 25-49 | ↑ .50 |
| 51-74 | ↓ .50 |
| 75-99 | ↑ .00 |

### Conditions

- Only cash payments (or cash portion of mixed)
- Only total receipt sum, not individual item prices
- Only when `rounding_enabled = 1` for this FN
- Purely cashless receipts → no rounding
- `SMP` on `<M T="0">` = difference (can be negative for rounding down)

### REST response

```json
{
  "total_sum": 37013,
  "rounded_sum": 37050,
  "rounding": 37
}
```

POS must use `rounded_sum` to prevent discrepancies.

---

## Step 6: Реквізити ЕПЗ на `<M>`

Non-cash `<M>` (T="1" or T="2") carries filled payment instrument attributes:

| Attr | Source | Required |
|------|--------|----------|
| RRN | payment.rrn | if present |
| PSNM | payment.payment_system | if present |
| PA | payment.bank_name | if present |
| PB | payment.terminal | if present |
| PC | payment.label | if present |
| PD | payment.card_mask | if present |
| PE | payment.auth_code | if present |
| PF | payment.commission | if present |

Rule: serialize only non-empty fields. Different payment systems provide different fields.

---

## Step 7: Payment Type Resolution

### Policy

- `CASH` → `T="0"`, `NM="Готівка"` — hardcoded canonical
- Everything else → lookup `payment_type_definitions` by `(fiscal_number, type_code)`
  - Found → use `type_group` for `T`, `name` for `NM`
  - **Not found → reject before sign** (INVALID_RECEIPT_DATA)

### Repository

```python
class PaymentTypeRepository:
    def get_for_fiscal_number(conn, fiscal_number) -> dict[str, PaymentTypeDef]
    def get_type(conn, fiscal_number, type_code) -> PaymentTypeDef | None
```

---

## Step 8: X-Report Endpoint

### `GET /v1/shifts/current/x-report?fiscal_number={fn}`

Read-only operational endpoint. Does NOT create a fiscal document. Does NOT change shift state.

### Response

```json
{
  "fiscal_number": "4000162280",
  "shift_id": "shift-123",
  "shift_opened_at": "2026-04-14T08:00:00Z",
  "report_ts": "2026-04-14T15:30:00Z",
  "tax_groups": { ... },
  "payments": { ... },
  "service": { ... },
  "check_count": {"sell": 42, "return": 3},
  "cash_balance": 55000,
  "cash_withdrawal": {"count": 1, "sum": 10000, "commission": 200}
}
```

Uses the same aggregation logic as Z-report but without closing the shift.

---

## Canonical Layer

All gateway-computed values must flow through WorkerProcessResult to REST response:

```python
class WorkerProcessResult:
    # existing fields...
    cash_balance: int | None = None
    change: int | None = None
    rounded_sum: int | None = None
    rounding: int | None = None
```

### Canonical flow

```
POS → request {goods, payments}
  ↓
Adapter → CanonicalFiscalCommand
  ↓
Write-path:
  1. Guards → cash balance check, payment type validation
  2. Rounding → rounded_sum, rounding delta
  3. Change → from cash overpayment
  4. Cash balance → derived after this operation
  5. Serializer → RM, SMP, EPZ attrs, payment T/NM from definitions
  6. WorkerProcessResult ← cash_balance, change, rounded_sum, rounding
  ↓
REST response ← cash_balance, change, rounded_sum, rounding
```

---

## Порядок реалізації

| # | Step | Оцінка |
|---|------|--------|
| 1 | Migrations (009, 010) | 15 хв |
| 2 | PaymentTypeRepository + guard | 1 год |
| 3 | Cash balance calculation | 1 год |
| 4 | Guards (SERVICE_OUT, CASH_WITHDRAWAL > balance) | 30 хв |
| 5 | Cash carry-over (SHIFT_OPEN / Z_REPORT) | 1 год |
| 6 | Решта (RM) in serializer | 30 хв |
| 7 | Заокруглення (SMP) + REST response | 1 год |
| 8 | Реквізити ЕПЗ on `<M>` | 1 год |
| 9 | X-report endpoint | 1 год |
| 10 | Canonical layer (WorkerProcessResult → REST) | 30 хв |
| 11 | Proof tests | 2 год |

**Загалом: ~9-10 годин.**

---

## Тести

**Кожен proof test доводить повний pipeline: ingress → write_path → serializer → XML + REST response.**

### Cash balance (CB1-CB6)
| Test | Proves |
|------|--------|
| CB1 | balance = SERVICE_IN - SERVICE_OUT + SELL(cash) - RETURN(cash) - CASH_WITHDRAWAL |
| CB2 | SERVICE_OUT > balance → reject, crypto NOT called |
| CB3 | CASH_WITHDRAWAL > balance → reject, crypto NOT called |
| CB4 | SHIFT_OPEN SM = last_cash_balance when preserve (in signed XML) |
| CB5 | SHIFT_OPEN SM = 0 when reset (in signed XML) |
| CB6 | Z_REPORT writes last_cash_balance, REST response has cash_balance |

### Решта (RM1-RM4)
| Test | Proves |
|------|--------|
| RM1 | Change in signed XML: `RM="13000"` on cash M |
| RM2 | Mixed payment: RM only from cash overpayment |
| RM3 | Card-only → no RM in XML |
| RM4 | REST response contains `change` |

### Заокруглення (RND1-RND5)
| Test | Proves |
|------|--------|
| RND1 | 37013→37050, SMP=37 in XML + rounded_sum in response |
| RND2 | 37075→37100, SMP=25 |
| RND3 | 37024→37000, SMP=-24 |
| RND4 | rounding_enabled=0 → no SMP, no rounding in response |
| RND5 | Cashless → no SMP even with rounding_enabled=1 |

### Реквізити ЕПЗ (EPZ1-EPZ2)
| Test | Proves |
|------|--------|
| EPZ1 | Non-cash M has RRN/PSNM/PD in signed XML (canonical attr order) |
| EPZ2 | Empty fields not serialized |

### Payment types (PT1-PT2)
| Test | Proves |
|------|--------|
| PT1 | Configured type → correct T and NM in signed XML |
| PT2 | Unknown type → reject before sign (not fallback T="2") |

### X-report (XR1-XR3)
| Test | Proves |
|------|--------|
| XR1 | Endpoint returns same aggregates as Z-report |
| XR2 | Shift stays OPENED after X-report |
| XR3 | Response includes cash_balance |

### Canonical layer (CL1-CL5)
| Test | Proves |
|------|--------|
| CL1 | POST SELL response has cash_balance |
| CL2 | POST SELL with change → response has change |
| CL3 | POST SELL with rounding → response has rounded_sum + rounding |
| CL4 | POST SERVICE_IN → response has updated cash_balance |
| CL5 | POST SERVICE_OUT → response has updated cash_balance |

---

## Виключення зі scope

### Sprint 10 Wave 2 (after pilot start)
- Знижки `<D>` серіалізація
- delLastChk / delLastChkId
- Нефіскальний текст `<L>`
- Код товару C (реальний замість "0")
- RT для RETURN
- EPZ в Z-звіті
- Domain X_REPORT operation (fiscal document to DPS)

### Sprint 11
- Offline full lifecycle (GO_OFFLINE, GO_ONLINE, ASK_CODES)
- OFFLINE_LOCAL_ACK state fix
- Offline sync live verification

---

## Ризики

| Ризик | Мітигація |
|-------|-----------|
| Cash balance розсинхрон при crash | Derived from documents, not separate counter |
| Заокруглення змінить суму після підпису | Rounding BEFORE sign, in serializer |
| POS не оновить суму після rounding | REST response explicitly returns rounded_sum |
| Unknown payment type silent misclassification | Reject before sign, no fallback |
