"""
Pilot smoke test — end-to-end happy path for Checkbox REST ingress.

Covers the full pilot contour in a single connected scenario:
  1. Config/env overrides (no SQL edits required)
  2. RuntimeContainer initialization + health
  3. SHIFT_OPEN via REST → enriched ACK response
  4. SELL via REST → enriched ACK response with server_fiscal_no
  5. DB state consistency after sync processing

This is the acceptance criterion for pilot readiness:
"An operator can set credentials through config, start the runtime,
open a shift, send a sale, and get a usable ACK in one synchronous round-trip."
"""
from __future__ import annotations

import json
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'  # must match node_state row in seed data
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'

# ---------------------------------------------------------------------------
# Mock Checkbox API transport
# ---------------------------------------------------------------------------

def _checkbox_mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-pilot'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'pilot-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-PILOT-001',
            'updated_at': '2026-03-29T09:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'pilot-receipt-001',
            'status': 'DONE',
            'fiscal_code': 'RCPT-PILOT-001',
            'updated_at': '2026-03-29T09:01:00+00:00',
        })
    raise AssertionError(f'smoke: unexpected mock request: {request.method} {request.url}')


# ---------------------------------------------------------------------------
# Helper: build pilot config using AppConfig.checkbox (no SQL edit)
# ---------------------------------------------------------------------------

def _pilot_config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'pilot.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'pilot-smoke',
        },
        'runtime': {'process_immediately': True},
        # Credentials via config section — no SQL required
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'PILOT-LIC-001',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'pilot-smoke',
        'business_ts': '2026-03-29T09:00:00Z',
    }


# ---------------------------------------------------------------------------
# Smoke test
# ---------------------------------------------------------------------------

def test_pilot_rest_shift_open_then_sell_end_to_end(tmp_path: Path) -> None:
    cfg = _pilot_config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_checkbox_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:

        # 1. Runtime is live and ready after startup
        assert client.get('/health/live').status_code == 200
        assert client.get('/health/ready').status_code == 200

        # 2. Verify config override reached the in-memory transport profile
        profile = container.transport_router.profiles.get(TRANSPORT_PROFILE)
        assert profile is not None
        assert profile.endpoint == 'https://api.checkbox.mock/api/v1'
        profile_cfg = json.loads(profile.config_json)
        assert profile_cfg['auth']['license_key'] == 'PILOT-LIC-001'
        assert profile_cfg['auth']['cashier_pin'] == '0000'

        # 3. SHIFT_OPEN — sync enriched response
        open_resp = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('pilot-req-open-1'), 'business_ts': '2026-03-29T09:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'pilot-ext-open-1',
                'cashier_id': 'pilot-cashier-1',
            },
        })
        assert open_resp.status_code == 200
        open_body = open_resp.json()
        assert open_body['document_state'] == 'ACK', f"SHIFT_OPEN not ACK: {open_body}"
        assert open_body['document_id'] is not None
        assert open_body['server_fiscal_no'] == 'SHIFT-PILOT-001'
        assert open_body['error_message'] is None

        # 4. SELL — sync enriched response
        sell_resp = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('pilot-req-sell-1'), 'business_ts': '2026-03-29T09:01:00Z'},
            'operation': 'SELL',
            'request': {
                'external_request_id': 'pilot-ext-sell-1',
                'cashier_id': 'pilot-cashier-1',
                'goods': [{'name': 'Coffee', 'price': 2500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2500}],
            },
        })
        assert sell_resp.status_code == 200
        sell_body = sell_resp.json()
        assert sell_body['document_state'] == 'ACK', f"SELL not ACK: {sell_body}"
        assert sell_body['document_id'] is not None
        assert sell_body['server_fiscal_no'] == 'RCPT-PILOT-001'
        assert sell_body['fiscal_number'] == FISCAL_NUMBER
        assert sell_body['error_message'] is None

        # 5. GET /v1/documents lookup confirms same state persisted
        doc_resp = client.get(f'/v1/documents/{sell_body["request_id"]}')
        assert doc_resp.status_code == 200
        assert doc_resp.json()['state'] == 'ACK'
        assert doc_resp.json()['server_fiscal_no'] == 'RCPT-PILOT-001'

    # 6. DB consistency: correct document states and open shift after shutdown
    with container.connect() as conn:
        docs = conn.execute(
            'SELECT doc_type, state, server_fiscal_no FROM fiscal_documents ORDER BY created_at'
        ).fetchall()
        assert [(r[0], r[1], r[2]) for r in docs] == [
            ('SHIFT_OPEN', 'ACK', 'SHIFT-PILOT-001'),
            ('SELL',       'ACK', 'RCPT-PILOT-001'),
        ]
        shift = conn.execute(
            "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()
        assert shift is not None and shift[0] == 'OPENED'
