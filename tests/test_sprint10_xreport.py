"""Sprint 10 step 9: X-Report endpoint.

Tests:
  XR1: endpoint returns aggregated shift data
  XR2: shift stays OPENED after X-report
  XR3: response includes cash_balance
  XR4: no active shift → 404
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
    def sign_raw(self, *, data, document_id=None): return b'\x30\x82SIGNED'


class _OkTransport:
    def send(self, **kw):
        return SendResult(state='ACK', transport_request_id='tr',
                          submission_status='DPS_ACK', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


def _make_app(tmp_path):
    cfg = AppConfig(database={"db_path": str(tmp_path / "xr.db"), "sql_dir": "sql"})
    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_OkTransport(), tax_number='TN')
    container = RuntimeContainer(cfg, command_processor=worker)
    return container, create_app(container)


def _post_sell(client, fn, amount=5000, payment_type='CASH'):
    return client.post("/v1/ingress/checkbox", json={
        "context": {
            "request_id": str(uuid.uuid4()), "fiscal_number": fn,
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "test", "business_ts": datetime.now(UTC).isoformat(),
        },
        "operation": "SELL",
        "request": {
            "goods": [{"good": {"name": "Item"}, "price": amount, "quantity": 1000}],
            "payments": [{"type": payment_type, "value": amount}],
        },
    })


def _post_shift_open(client, fn):
    return client.post("/v1/ingress/checkbox", json={
        "context": {
            "request_id": str(uuid.uuid4()), "fiscal_number": fn,
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "test", "business_ts": datetime.now(UTC).isoformat(),
        },
        "operation": "SHIFT_OPEN",
        "request": {},
    })


def _post_service_in(client, fn, amount):
    return client.post("/v1/ingress/checkbox", json={
        "context": {
            "request_id": str(uuid.uuid4()), "fiscal_number": fn,
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "test", "business_ts": datetime.now(UTC).isoformat(),
        },
        "operation": "SERVICE_IN",
        "request": {"value": amount},
    })


# ---------------------------------------------------------------------------
# XR1 — endpoint returns aggregated data
# ---------------------------------------------------------------------------

def test_xr1_returns_aggregated_data(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _post_shift_open(client, fn)
        _post_service_in(client, fn, 20000)
        _post_sell(client, fn, 5000, 'CASH')
        _post_sell(client, fn, 3000, 'CASH')

        resp = client.get(f"/v1/shifts/current/x-report?fiscal_number={fn}")

    assert resp.status_code == 200
    data = resp.json()
    assert data["fiscal_number"] == fn
    assert data["check_count"]["sell"] == 2
    assert data["check_count"]["return"] == 0
    assert "CASH" in data["payments"]
    assert data["payments"]["CASH"]["smi"] == 8000  # 5000+3000


# ---------------------------------------------------------------------------
# XR2 — shift stays OPENED after X-report
# ---------------------------------------------------------------------------

def test_xr2_shift_stays_opened(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _post_shift_open(client, fn)
        _post_sell(client, fn, 1000, 'CASH')

        # X-report
        resp1 = client.get(f"/v1/shifts/current/x-report?fiscal_number={fn}")
        assert resp1.status_code == 200

        # SELL after X-report should still work (shift is open)
        resp2 = _post_sell(client, fn, 2000, 'CASH')
        assert resp2.status_code == 200
        assert resp2.json().get("document_state") == "ACK"

        # Second X-report should show updated data
        resp3 = client.get(f"/v1/shifts/current/x-report?fiscal_number={fn}")
        assert resp3.status_code == 200
        assert resp3.json()["check_count"]["sell"] == 2  # 1+1


# ---------------------------------------------------------------------------
# XR3 — response includes cash_balance
# ---------------------------------------------------------------------------

def test_xr3_includes_cash_balance(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    fn = "FN-DEV-0001"
    with TestClient(app) as client:
        _post_shift_open(client, fn)
        _post_service_in(client, fn, 50000)
        _post_sell(client, fn, 10000, 'CASH')

        resp = client.get(f"/v1/shifts/current/x-report?fiscal_number={fn}")

    assert resp.status_code == 200
    data = resp.json()
    assert "cash_balance" in data
    # SERVICE_IN 50000 + SELL CASH 10000 = 60000
    assert data["cash_balance"] == 60000


# ---------------------------------------------------------------------------
# XR4 — no active shift → 404
# ---------------------------------------------------------------------------

def test_xr4_no_shift_404(tmp_path) -> None:
    container, app = _make_app(tmp_path)
    with TestClient(app) as client:
        resp = client.get("/v1/shifts/current/x-report?fiscal_number=FN-NONE")
    assert resp.status_code == 404
    assert resp.json()["error"] == "NO_ACTIVE_SHIFT"
