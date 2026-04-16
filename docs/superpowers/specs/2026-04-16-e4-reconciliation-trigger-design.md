# E4 — POST /v1/admin/reconciliation/trigger

**Date:** 2026-04-16  
**Status:** Approved  
**Scope:** Single new endpoint in `rest_app.py`, no schema changes

---

## Problem

`ReconciliationService.reconcile_pending()` runs automatically on startup (via `StartupSupervisor`) and periodically in the ops-loop. There is no way for an operator to trigger it manually — for example after a network outage when DPS was unreachable and documents are stuck in PENDING.

---

## API Contract

```
POST /v1/admin/reconciliation/trigger
Content-Type: application/json
```

### Request body

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `fiscal_number` | string | No | Target fiscal number. Omit (or pass `null`/`""`) to reconcile all. |

**Examples:**

```json
{"fiscal_number": "FN-0001"}
```
```json
{}
```

### Response 200

```json
{
  "fiscal_number": "FN-0001",
  "checked": 12,
  "acked": 8,
  "rejected": 1,
  "retryable": 2,
  "still_pending": 1,
  "manual": 0
}
```

`fiscal_number` is `null` when the request was for all fiscal numbers.

### Response 503

```json
{"detail": "reconciliation_service not available"}
```

Returned when `container.reconciliation_service is None` (e.g. container started without transport wiring).

---

## Implementation

**File:** `src/prro_gateway/runtime/rest_app.py`

One new async function following the pattern of `/v1/admin/offline-sync`:

```python
@app.post("/v1/admin/reconciliation/trigger")
async def admin_reconciliation_trigger(request: Request) -> JSONResponse:
    if container.reconciliation_service is None:
        return JSONResponse(status_code=503,
            content={"detail": "reconciliation_service not available"})
    body = await request.json()
    fiscal_number = body.get("fiscal_number") or None   # "" → None
    with container.connect() as conn:
        result = container.reconciliation_service.reconcile_pending(
            conn, fiscal_number=fiscal_number
        )
    logger.info("admin_reconciliation_triggered", extra={"extra_fields": {
        "fiscal_number": fiscal_number,
        "checked": result.checked,
        "acked": result.acked,
        "manual": result.manual,
    }})
    return JSONResponse(status_code=200, content={
        "fiscal_number": fiscal_number,
        "checked": result.checked,
        "acked": result.acked,
        "rejected": result.rejected,
        "retryable": result.retryable,
        "still_pending": result.still_pending,
        "manual": result.manual,
    })
```

No changes to `ReconciliationService`, `container.py`, or DB schema.

---

## Tests

**File:** `tests/test_e4_reconciliation_trigger.py`

| Test | Scenario | Assert |
|------|----------|--------|
| `test_e4_trigger_specific_fn` | POST `{"fiscal_number": "FN-..."}` with no pending docs | `status=200`, `fiscal_number` echoed, all counts are 0 |
| `test_e4_trigger_all_fns` | POST `{}` | `status=200`, `fiscal_number=null` |
| `test_e4_503_when_service_unavailable` | Container without reconciliation_service | `status=503` |

---

## Invariants preserved

- No network or crypto calls inside SQLite transactions (reconcile_pending is called outside any open transaction).
- Single-writer per fiscal_number: reconciliation only reads poll_status (transport), never writes concurrently with write-path.
- Idempotent: calling the endpoint twice produces the same result (second call finds nothing pending).
