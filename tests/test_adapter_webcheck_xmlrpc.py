"""Unit tests for WebCheckXmlRpcAdapter.map_command()."""
from __future__ import annotations

import pytest

from prro_gateway.adapters.webcheck_xmlrpc import WebCheckXmlRpcAdapter
from prro_gateway.adapters.base import AdapterMappingError
from prro_gateway.enums import OperationType, Protocol

ADAPTER = WebCheckXmlRpcAdapter()

_CTX = {
    "fiscal_number": "FN-DEV-0001",
    "request_id": "req-wc-1",
    "backend_profile_id": "backend_checkbox_default",
    "transport_profile_id": "transport_checkbox_rest_default",
    "channel_owner": "pos-1",
    "business_ts": "2026-04-17T10:00:00Z",
}

_ROW = {"name": "Хліб", "price": 3000, "quantity": 1000, "sum": 3000}
_PAYMENT = {"type": "CASH", "value": 3000}


def _req(method: str, params: dict | None = None) -> dict:
    return {"context": _CTX, "method": method, "params": params or {}}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _map(method: str, params: dict | None = None):
    return ADAPTER.map_command(_req(method, params))


# ---------------------------------------------------------------------------
# Protocol tag — every result must carry WEBCHECK_XMLRPC
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("method, params", [
    ("SellCheck", {"rows": [_ROW], "payments": [_PAYMENT]}),
    ("ReturnCheck", {"rows": [_ROW], "payments": [_PAYMENT], "related_receipt_id": "orig-001"}),
    ("OpenShift", {}),
    ("CloseShift", {}),
    ("XReport", {}),
    ("ZReport", {}),
    ("GetStatus", {}),
    ("ServiceIn", {"value": 10000}),
    ("ServiceOut", {"value": 5000}),
    ("CashWithdrawal", {"value": 2000}),
])
def test_protocol_is_webcheck_xmlrpc(method, params):
    result = _map(method, params)
    assert result.protocol == Protocol.WEBCHECK_XMLRPC


# ---------------------------------------------------------------------------
# Operation type mapping
# ---------------------------------------------------------------------------

def test_sell_check_operation_type():
    result = _map("SellCheck", {"rows": [_ROW], "payments": [_PAYMENT]})
    assert result.operation_type == OperationType.SELL


def test_return_check_operation_type():
    result = _map("ReturnCheck", {"rows": [_ROW], "payments": [_PAYMENT], "related_receipt_id": "orig-001"})
    assert result.operation_type == OperationType.RETURN


def test_open_shift_operation_type():
    result = _map("OpenShift")
    assert result.operation_type == OperationType.SHIFT_OPEN


def test_close_shift_operation_type():
    result = _map("CloseShift")
    assert result.operation_type == OperationType.SHIFT_CLOSE


def test_x_report_operation_type():
    result = _map("XReport")
    assert result.operation_type == OperationType.X_REPORT


def test_z_report_operation_type():
    result = _map("ZReport")
    assert result.operation_type == OperationType.Z_REPORT


def test_get_status_operation_type():
    result = _map("GetStatus")
    assert result.operation_type == OperationType.GET_STATUS


def test_service_in_operation_type():
    result = _map("ServiceIn", {"value": 10000})
    assert result.operation_type == OperationType.SERVICE_IN


def test_service_out_operation_type():
    result = _map("ServiceOut", {"value": 5000})
    assert result.operation_type == OperationType.SERVICE_OUT


def test_cash_withdrawal_operation_type():
    result = _map("CashWithdrawal", {"value": 2000})
    assert result.operation_type == OperationType.CASH_WITHDRAWAL


# ---------------------------------------------------------------------------
# Unknown method → AdapterMappingError
# ---------------------------------------------------------------------------

def test_unknown_method_raises():
    with pytest.raises(AdapterMappingError) as exc_info:
        _map("NonExistentMethod")
    assert exc_info.value.code == "UNSUPPORTED_METHOD"
    assert "NonExistentMethod" in str(exc_info.value)


# ---------------------------------------------------------------------------
# SellCheck — goods and payments built from rows/payments
# ---------------------------------------------------------------------------

def test_sell_check_goods_from_rows():
    row = {"name": "Молоко", "price": 4500, "quantity": 2000, "sum": 9000}
    result = _map("SellCheck", {"rows": [row], "payments": [{"type": "CASH", "value": 9000}]})
    goods = result.payload["receipt"]["goods"]
    assert len(goods) == 1
    assert goods[0]["name"] == "Молоко"
    assert goods[0]["price"] == 4500
    assert goods[0]["quantity"] == 2000
    assert goods[0]["sum"] == 9000


def test_sell_check_payments_from_payments():
    result = _map(
        "SellCheck",
        {"rows": [_ROW], "payments": [{"type": "CASH", "value": 5000}]},
    )
    payments = result.payload["receipt"]["payments"]
    assert len(payments) == 1
    assert payments[0]["payment_type"] == "CASH"
    assert payments[0]["amount"] == 5000


def test_sell_check_goods_count():
    rows = [_ROW, {"name": "Масло", "price": 6000, "quantity": 1000, "sum": 6000}]
    result = _map("SellCheck", {"rows": rows, "payments": [_PAYMENT]})
    assert result.payload["goods_count"] == 2


# ---------------------------------------------------------------------------
# ReturnCheck — carries related_receipt_id
# ---------------------------------------------------------------------------

def test_return_check_related_receipt_id():
    result = _map(
        "ReturnCheck",
        {"rows": [_ROW], "payments": [_PAYMENT], "related_receipt_id": "orig-receipt-999"},
    )
    assert result.payload["receipt"]["related_receipt_id"] == "orig-receipt-999"


# ---------------------------------------------------------------------------
# OpenShift — requires_shift=False, empty goods/payments
# ---------------------------------------------------------------------------

def test_open_shift_requires_shift_false():
    result = _map("OpenShift")
    assert result.requires_shift is False


def test_open_shift_empty_goods():
    result = _map("OpenShift")
    assert result.payload["receipt"]["goods"] == []
    assert result.payload["receipt"]["payments"] == []


def test_open_shift_goods_count_zero():
    result = _map("OpenShift")
    assert result.payload["goods_count"] == 0
    assert result.payload["payments_count"] == 0


# ---------------------------------------------------------------------------
# ShiftClose / XReport / ZReport / GetStatus — empty goods/payments
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("method", ["CloseShift", "XReport", "ZReport", "GetStatus"])
def test_control_methods_empty_goods_payments(method):
    result = _map(method)
    assert result.payload["receipt"]["goods"] == []
    assert result.payload["receipt"]["payments"] == []


# ---------------------------------------------------------------------------
# GetStatus — requires_shift=False
# ---------------------------------------------------------------------------

def test_get_status_requires_shift_false():
    result = _map("GetStatus")
    assert result.requires_shift is False


# ---------------------------------------------------------------------------
# ServiceIn — service_sum in payload
# ---------------------------------------------------------------------------

def test_service_in_service_sum():
    result = _map("ServiceIn", {"value": 15000})
    assert result.payload["service_sum"] == 15000


def test_service_in_service_sum_zero_default():
    result = _map("ServiceIn", {})
    assert result.payload["service_sum"] == 0


# ---------------------------------------------------------------------------
# ServiceOut — service_sum in payload
# ---------------------------------------------------------------------------

def test_service_out_service_sum():
    result = _map("ServiceOut", {"value": 8000})
    assert result.payload["service_sum"] == 8000


# ---------------------------------------------------------------------------
# CashWithdrawal — no service_sum in payload
# ---------------------------------------------------------------------------

def test_cash_withdrawal_no_service_sum():
    result = _map("CashWithdrawal", {"value": 2000})
    assert "service_sum" not in result.payload


# ---------------------------------------------------------------------------
# requires_shift for sell/return/service/cash-withdrawal
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("method, params", [
    ("SellCheck", {"rows": [_ROW], "payments": [_PAYMENT]}),
    ("ReturnCheck", {"rows": [_ROW], "payments": [_PAYMENT], "related_receipt_id": "x"}),
    ("ServiceIn", {"value": 100}),
    ("ServiceOut", {"value": 100}),
    ("CashWithdrawal", {"value": 100}),
    ("CloseShift", {}),
    ("XReport", {}),
    ("ZReport", {}),
])
def test_requires_shift_true(method, params):
    result = _map(method, params)
    assert result.requires_shift is True


# ---------------------------------------------------------------------------
# Idempotency key format
# ---------------------------------------------------------------------------

def test_idempotency_key_contains_operation_and_fiscal():
    result = _map("SellCheck", {"rows": [_ROW], "payments": [_PAYMENT], "doc_no": "DOC-42"})
    key = result.idempotency_key
    assert "sell" in key
    assert "fn-dev-0001" in key


def test_idempotency_key_uses_doc_no_hint():
    result = _map("SellCheck", {"rows": [_ROW], "payments": [_PAYMENT], "doc_no": "DOC-42"})
    assert "DOC-42" in result.idempotency_key or "doc-42" in result.idempotency_key.lower()


def test_idempotency_key_uses_external_request_id_over_hint():
    result = _map(
        "SellCheck",
        {"rows": [_ROW], "payments": [_PAYMENT], "external_request_id": "ext-999", "doc_no": "DOC-1"},
    )
    assert "ext-999" in result.idempotency_key


# ---------------------------------------------------------------------------
# schema_version is set
# ---------------------------------------------------------------------------

def test_schema_version_present():
    result = _map("SellCheck", {"rows": [_ROW], "payments": [_PAYMENT]})
    assert result.schema_version is not None
    assert result.schema_version != ""
