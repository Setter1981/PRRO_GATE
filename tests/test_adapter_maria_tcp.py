# tests/test_adapter_maria_tcp.py
"""Unit tests for MariaTcpAdapter.map_command().

Covers all _COMMAND_TO_OPERATION entries + error path.
No production code is modified by this file.
"""
from __future__ import annotations

import pytest

from prro_gateway.adapters.maria_tcp import MariaTcpAdapter
from prro_gateway.adapters.base import AdapterMappingError
from prro_gateway.enums import OperationType, Protocol

ADAPTER = MariaTcpAdapter()

_CTX = {
    "fiscal_number": "FN-DEV-0001",
    "request_id": "req-maria-1",
    "backend_profile_id": "backend_checkbox_default",
    "transport_profile_id": "transport_checkbox_rest_default",
    "channel_owner": "pos-1",
    "business_ts": "2026-04-17T10:00:00Z",
}

# A minimal item whose `sum` matches price*quantity/1000
_ITEM = {"name": "Молоко", "price": 5000, "quantity": 1000, "sum": 5000, "code": "001"}
# Payment uses 'value'; _map_payment stores it as 'amount'
_PAYMENT = {"type": "CASH", "value": 5000}


def _req(command: str, fields: dict | None = None) -> dict:
    return {"context": _CTX, "command": command, "fields": fields or {}}


# ---------------------------------------------------------------------------
# 1. SALE → OperationType.SELL
# ---------------------------------------------------------------------------

def test_sale_maps_to_sell() -> None:
    cmd = ADAPTER.map_command(_req("SALE", {
        "ticket_no": "t-001",
        "items": [_ITEM],
        "payments": [_PAYMENT],
    }))
    assert cmd.operation_type == OperationType.SELL
    assert cmd.protocol == Protocol.MARIA_TCP
    receipt = cmd.payload["receipt"]
    assert len(receipt["goods"]) == 1
    assert len(receipt["payments"]) == 1
    assert receipt["goods"][0]["name"] == "Молоко"


# ---------------------------------------------------------------------------
# 2. RETURN → OperationType.RETURN
# ---------------------------------------------------------------------------

def test_return_maps_correctly() -> None:
    cmd = ADAPTER.map_command(_req("RETURN", {
        "ticket_no": "t-002",
        "items": [_ITEM],
        "payments": [_PAYMENT],
        "related_receipt_id": "orig-001",
    }))
    assert cmd.operation_type == OperationType.RETURN
    assert cmd.payload["receipt"]["related_receipt_id"] == "orig-001"


# ---------------------------------------------------------------------------
# 3. OPEN_SHIFT → OperationType.SHIFT_OPEN, empty goods/payments
# ---------------------------------------------------------------------------

def test_open_shift_has_empty_goods_and_payments() -> None:
    cmd = ADAPTER.map_command(_req("OPEN_SHIFT"))
    assert cmd.operation_type == OperationType.SHIFT_OPEN
    assert cmd.payload["receipt"]["goods"] == []
    assert cmd.payload["receipt"]["payments"] == []


# ---------------------------------------------------------------------------
# 4. CLOSE_SHIFT → OperationType.SHIFT_CLOSE
# ---------------------------------------------------------------------------

def test_close_shift_maps_correctly() -> None:
    cmd = ADAPTER.map_command(_req("CLOSE_SHIFT"))
    assert cmd.operation_type == OperationType.SHIFT_CLOSE
    # shift_close clears goods/payments like other shift ops
    assert cmd.payload["receipt"]["goods"] == []
    assert cmd.payload["receipt"]["payments"] == []


# ---------------------------------------------------------------------------
# 5. X_REPORT → OperationType.X_REPORT, empty goods
# ---------------------------------------------------------------------------

def test_x_report_maps_correctly() -> None:
    cmd = ADAPTER.map_command(_req("X_REPORT"))
    assert cmd.operation_type == OperationType.X_REPORT
    assert cmd.payload["receipt"]["goods"] == []
    assert cmd.payload["receipt"]["payments"] == []


# ---------------------------------------------------------------------------
# 6. Z_REPORT → OperationType.Z_REPORT
# ---------------------------------------------------------------------------

def test_z_report_maps_correctly() -> None:
    cmd = ADAPTER.map_command(_req("Z_REPORT"))
    assert cmd.operation_type == OperationType.Z_REPORT
    assert cmd.payload["receipt"]["goods"] == []
    assert cmd.payload["receipt"]["payments"] == []


# ---------------------------------------------------------------------------
# 7. STATUS → OperationType.GET_STATUS
# ---------------------------------------------------------------------------

def test_status_maps_correctly() -> None:
    cmd = ADAPTER.map_command(_req("STATUS"))
    assert cmd.operation_type == OperationType.GET_STATUS
    assert cmd.payload["receipt"]["goods"] == []
    assert cmd.payload["receipt"]["payments"] == []


# ---------------------------------------------------------------------------
# 8. SERVICE_IN → OperationType.SERVICE_IN, service_sum in payload
# ---------------------------------------------------------------------------

def test_service_in_builds_service_sum() -> None:
    cmd = ADAPTER.map_command(_req("SERVICE_IN", {"value": 20000}))
    assert cmd.operation_type == OperationType.SERVICE_IN
    assert cmd.payload["service_sum"] == 20000


# ---------------------------------------------------------------------------
# 9. SERVICE_OUT → OperationType.SERVICE_OUT, service_sum in payload
# ---------------------------------------------------------------------------

def test_service_out_builds_service_sum() -> None:
    cmd = ADAPTER.map_command(_req("SERVICE_OUT", {"value": 10000}))
    assert cmd.operation_type == OperationType.SERVICE_OUT
    assert cmd.payload["service_sum"] == 10000


# ---------------------------------------------------------------------------
# 10. CASH_WITHDRAWAL → OperationType.CASH_WITHDRAWAL
#     Note: _map_payment stores 'value' as 'amount'; cash_withdrawal_sum sums
#     the mapped 'amount' fields from the payments list.
# ---------------------------------------------------------------------------

def test_cash_withdrawal_builds_sum() -> None:
    cmd = ADAPTER.map_command(_req("CASH_WITHDRAWAL", {
        "payments": [{"type": "CASH", "value": 30000}],
    }))
    assert cmd.operation_type == OperationType.CASH_WITHDRAWAL
    # _map_payment maps "value" → "amount"; cash_withdrawal_sum sums amount
    assert cmd.payload["cash_withdrawal_sum"] == 30000


# ---------------------------------------------------------------------------
# 11. Unknown command → AdapterMappingError
# ---------------------------------------------------------------------------

def test_unknown_command_raises_adapter_mapping_error() -> None:
    with pytest.raises(AdapterMappingError):
        ADAPTER.map_command(_req("UNKNOWN_OP"))


def test_unknown_command_error_has_code() -> None:
    with pytest.raises(AdapterMappingError) as exc_info:
        ADAPTER.map_command(_req("UNSUPPORTED_XYZ"))
    assert exc_info.value.code == "UNSUPPORTED_METHOD"


# ---------------------------------------------------------------------------
# 12. requires_shift: SALE → True, OPEN_SHIFT → False, STATUS → False
# ---------------------------------------------------------------------------

def test_sale_requires_shift_true() -> None:
    cmd = ADAPTER.map_command(_req("SALE", {"items": [_ITEM], "payments": [_PAYMENT]}))
    assert cmd.requires_shift is True


def test_open_shift_requires_shift_false() -> None:
    cmd = ADAPTER.map_command(_req("OPEN_SHIFT"))
    assert cmd.requires_shift is False


def test_status_requires_shift_false() -> None:
    cmd = ADAPTER.map_command(_req("STATUS"))
    assert cmd.requires_shift is False


def test_close_shift_requires_shift_true() -> None:
    # SHIFT_CLOSE is NOT in the exclusion set — it requires an open shift
    cmd = ADAPTER.map_command(_req("CLOSE_SHIFT"))
    assert cmd.requires_shift is True


# ---------------------------------------------------------------------------
# 13. Protocol is always MARIA_TCP
# ---------------------------------------------------------------------------

def test_all_commands_use_maria_tcp_protocol() -> None:
    commands = [
        "SALE",
        "RETURN",
        "OPEN_SHIFT",
        "CLOSE_SHIFT",
        "X_REPORT",
        "Z_REPORT",
        "STATUS",
        "SERVICE_IN",
        "SERVICE_OUT",
        "CASH_WITHDRAWAL",
    ]
    default_fields: dict[str, dict] = {
        "SALE": {"items": [_ITEM], "payments": [_PAYMENT]},
        "RETURN": {"items": [_ITEM], "payments": [_PAYMENT]},
        "SERVICE_IN": {"value": 100},
        "SERVICE_OUT": {"value": 100},
        "CASH_WITHDRAWAL": {"payments": [_PAYMENT]},
    }
    for cmd_name in commands:
        cmd = ADAPTER.map_command(_req(cmd_name, default_fields.get(cmd_name)))
        assert cmd.protocol == Protocol.MARIA_TCP, f"{cmd_name} should use MARIA_TCP protocol"


# ---------------------------------------------------------------------------
# 14. SALE items are mapped correctly (name, price, quantity preserved)
# ---------------------------------------------------------------------------

def test_sale_item_fields_mapped() -> None:
    item = {"name": "Хліб", "price": 3000, "quantity": 2000, "sum": 6000, "code": "002"}
    cmd = ADAPTER.map_command(_req("SALE", {"items": [item], "payments": [_PAYMENT]}))
    good = cmd.payload["receipt"]["goods"][0]
    assert good["name"] == "Хліб"
    assert good["price"] == 3000
    assert good["quantity"] == 2000
    assert good["sum"] == 6000
    assert good["code"] == "002"


# ---------------------------------------------------------------------------
# 15. SALE payments are mapped correctly (payment_type, amount preserved)
# ---------------------------------------------------------------------------

def test_sale_payment_fields_mapped() -> None:
    payment = {"type": "CASHLESS", "value": 8000, "card_mask": "****1234"}
    cmd = ADAPTER.map_command(_req("SALE", {"items": [_ITEM], "payments": [payment]}))
    pay = cmd.payload["receipt"]["payments"][0]
    assert pay["payment_type"] == "CASHLESS"
    assert pay["amount"] == 8000
    assert pay["card_mask"] == "****1234"
