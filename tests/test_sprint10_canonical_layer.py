"""Sprint 10 Step 10: canonical layer fields in REST response.

Tests:
  CL1: POST SELL → response has cash_balance
  CL2: POST SELL with overpayment → response has change
  CL3: POST SELL with rounding → response has rounded_sum + rounding
  CL4: POST SERVICE_IN → response has updated cash_balance
  CL5: SERVICE_IN then SERVICE_OUT → cash_balance reflects both
  CL6: no rounding_enabled → no rounded_sum/rounding in response
  CL7: error response doesn't carry canonical fields
"""
from __future__ import annotations

import uuid
from datetime import datetime, UTC

from prro_gateway.config import AppConfig
from prro_gateway.ports import SendResult
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app
from prro_gateway.services.write_path import WritePathWorker
from starlette.testclient import TestClient


class _Crypto:
    def sign(self, **kw): return 'signed'
    def sign_raw(self, *, data): return b'\x30\x82SIGNED'


class _OkTransport:
    def send(self, **kw):
        return SendResult(state='ACK', transport_request_id='tr',
                          submission_status='DPS_ACK', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


def _make_app(tmp_path):
    cfg = AppConfig(database={"db_path": str(tmp_path / "cl.db"), "sql_dir": "sql"})
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    container = RuntimeContainer(cfg, command_processor=worker)
    return container, create_app(container)


def _req(fn, operation, request_body):
    return {
        "context": {
            "request_id": str(uuid.uuid4()), "fiscal_number": fn,
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "test", "business_ts": datetime.now(UTC).isoformat(),
        },
        "operation": operation,
        "request": request_body,
    }


def _shift_open(client, fn):
    return client.post("/v1/ingress/checkbox", json=_req(fn, "SHIFT_OPEN", {}))


def _sell(client, fn, total_sum, payment_amount=None, payment_type="CASH"):
    if payment_amount is None:
        payment_amount = total_sum
    return client.post("/v1/ingress/checkbox", json=_req(fn, "SELL", {
        "goods": [{"good": {"name": "Item"}, "price": total_sum, "quantity": 1000}],
        "payments": [{"type": payment_type, "value": payment_amount}],
        "totals": {"total_sum": total_sum},
    }))


def _service_in(client, fn, amount):
    return client.post("/v1/ingress/checkbox", json=_req(fn, "SERVICE_IN", {"value": amount}))


def _service_out(client, fn, amount):
    return client.post("/v1/ingress/checkbox", json=_req(fn, "SERVICE_OUT", {"value": amount}))


# ---------------------------------------------------------------------------
# CL1 — POST SELL → response has cash_balance
# ---------------------------------------------------------------------------

def test_cl1_sell_has_cash_balance(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _shift_open(client, fn)
        resp = _sell(client, fn, total_sum=10000)

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK"
    assert data.get("cash_balance") == 10000


# ---------------------------------------------------------------------------
# CL2 — POST SELL with overpayment → response has change
# ---------------------------------------------------------------------------

def test_cl2_sell_overpayment_has_change(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _shift_open(client, fn)
        # customer pays 8000 for a 5000 item → change = 3000
        resp = _sell(client, fn, total_sum=5000, payment_amount=8000)

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK", f"got: {data}"
    assert data.get("change") == 3000


# ---------------------------------------------------------------------------
# CL3 — POST SELL with rounding → response has rounded_sum + rounding
# ---------------------------------------------------------------------------

def test_cl3_sell_with_rounding(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        # Set rounding_enabled after migrations have run (inside lifespan)
        with container.connect() as conn:
            conn.execute("UPDATE node_state SET rounding_enabled = 1 WHERE fiscal_number = ?", (fn,))
            conn.commit()

        _shift_open(client, fn)
        # 125.13 UAH → round down to 125.00, rounding = -13
        resp = _sell(client, fn, total_sum=12513)

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK"
    assert data.get("rounding") == -13
    assert data.get("rounded_sum") == 12500


# ---------------------------------------------------------------------------
# CL4 — POST SERVICE_IN → response has updated cash_balance
# ---------------------------------------------------------------------------

def test_cl4_service_in_has_cash_balance(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _shift_open(client, fn)
        resp = _service_in(client, fn, 5000)

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK"
    assert data.get("cash_balance") == 5000


# ---------------------------------------------------------------------------
# CL5 — SERVICE_IN then SERVICE_OUT → cash_balance reflects both
# ---------------------------------------------------------------------------

def test_cl5_service_in_then_out_cash_balance(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _shift_open(client, fn)
        _service_in(client, fn, 10000)
        resp = _service_out(client, fn, 3000)

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK"
    assert data.get("cash_balance") == 7000


# ---------------------------------------------------------------------------
# CL6 — no rounding_enabled → no rounded_sum/rounding in response
# ---------------------------------------------------------------------------

def test_cl6_no_rounding_when_disabled(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    # rounding_enabled defaults to 0

    with TestClient(app) as client:
        _shift_open(client, fn)
        resp = _sell(client, fn, total_sum=12513)

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK"
    assert "rounded_sum" not in data
    assert "rounding" not in data


# ---------------------------------------------------------------------------
# CL7 — error response doesn't carry canonical fields
# ---------------------------------------------------------------------------

def test_cl7_error_response_no_canonical_fields(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        # SELL without an open shift → rejected by guard
        resp = _sell(client, fn, total_sum=5000)

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") != "ACK"
    assert "cash_balance" not in data
    assert "change" not in data
    assert "rounded_sum" not in data
    assert "rounding" not in data


# ---------------------------------------------------------------------------
# F1 — print_zero_change flag controls change=0 visibility
# ---------------------------------------------------------------------------

def test_f1_zero_change_suppressed_by_default(tmp_path) -> None:
    """Exact payment: change=0 must NOT appear when print_zero_change=0 (default)."""
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _shift_open(client, fn)
        resp = _sell(client, fn, total_sum=5000)  # exact payment

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK"
    assert "change" not in data


def test_f1_zero_change_included_when_flag_set(tmp_path) -> None:
    """Exact payment: change=0 MUST appear when print_zero_change=1."""
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        with container.connect() as conn:
            conn.execute("UPDATE node_state SET print_zero_change = 1 WHERE fiscal_number = ?", (fn,))
            conn.commit()
        _shift_open(client, fn)
        resp = _sell(client, fn, total_sum=5000)  # exact payment

    assert resp.status_code == 200
    data = resp.json()
    assert data.get("document_state") == "ACK"
    assert data.get("change") == 0


# ---------------------------------------------------------------------------
# F4 — replay response has canonical fields
# ---------------------------------------------------------------------------

def test_f4_replay_has_canonical_fields(tmp_path) -> None:
    """Second request with same request_id (replay) must carry canonical fields."""
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    # Build a fixed request_id so the second call is an idempotency replay
    fixed_request_id = "replay-test-fixed-id-001"

    def _sell_fixed(client):
        return client.post("/v1/ingress/checkbox", json={
            "context": {
                "request_id": fixed_request_id, "fiscal_number": fn,
                "backend_profile_id": "backend_checkbox_default",
                "transport_profile_id": "transport_checkbox_rest_default",
                "channel_owner": "test", "business_ts": datetime.now(UTC).isoformat(),
            },
            "operation": "SELL",
            "request": {
                "goods": [{"good": {"name": "Item"}, "price": 8000, "quantity": 1000}],
                "payments": [{"type": "CASH", "value": 8000}],
                "totals": {"total_sum": 8000},
            },
        })

    with TestClient(app) as client:
        _shift_open(client, fn)

        # First call — live path
        resp1 = _sell_fixed(client)
        assert resp1.status_code == 200
        data1 = resp1.json()
        assert data1.get("document_state") == "ACK"
        assert data1.get("cash_balance") == 8000
        assert data1.get("existing") is False

        # Second call — replay path (same request_id)
        resp2 = _sell_fixed(client)
        assert resp2.status_code == 200
        data2 = resp2.json()
        assert data2.get("existing") is True
        assert data2.get("document_state") == "ACK"
        assert data2.get("cash_balance") == 8000
