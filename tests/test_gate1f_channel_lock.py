"""
Gate 1f — channel lock boundary smoke tests.

When a shift is open the channel is locked to the (backend_profile_id,
transport_profile_id, protocol, channel_owner) tuple captured at SHIFT_OPEN.
Any SELL arriving with a different tuple must be rejected with
SHIFT_CHANNEL_SWITCH_FORBIDDEN before reaching the transport.

Two mismatch vectors:

  Vector A — channel_owner mismatch:
    Shift opened by 'owner-a'.  SELL arrives from 'owner-b'.
    _channel_matches: channel_owner 'owner-b' != lock.integration_owner 'owner-a' → False.
    Expected: HTTP 200, error_code='SHIFT_CHANNEL_SWITCH_FORBIDDEN', no ACK, no document,
              transport NOT called for the SELL.

  Vector B — transport_profile_id mismatch:
    Shift opened with transport_checkbox_rest_default.
    SELL arrives with transport_webcheck_stub (unknown but structurally valid string).
    _channel_matches: transport_profile_id differs → False.
    Expected: same controlled-error shape as Vector A.

Both vectors confirm Invariant #3 (channel switch forbidden with open shift).

Invariants preserved:
  #1 (no network in tx): guard fires before send → transport not reached.
  #3 (channel switch forbidden): directly verified.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# Mock transport
# ---------------------------------------------------------------------------

def _mock_shift_open_only(request: httpx.Request) -> httpx.Response:
    """Allows the initial shift-open sequence; raises if a receipt call is attempted."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1f'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1f-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1F-001',
            'updated_at': '2026-03-29T15:00:00+00:00',
        })
    raise AssertionError(
        f'gate1f: transport reached when channel-lock guard should have blocked — '
        f'{request.method} {request.url}'
    )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _config(tmp_path: Path, db_name: str) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1f-default',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1F-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, *, channel_owner: str, transport_profile_id: str = TRANSPORT_PROFILE,
         ts: str = '2026-03-29T15:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': transport_profile_id,
        'channel_owner': channel_owner,
        'business_ts': ts,
    }


def _open_shift(client: TestClient, req_id: str, channel_owner: str) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx(req_id, channel_owner=channel_owner), 'business_ts': '2026-03-29T15:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-{req_id}',
            'cashier_id': 'cashier-gate1f',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _sell(client: TestClient, req_id: str, ext_id: str, *,
          channel_owner: str, transport_profile_id: str = TRANSPORT_PROFILE):
    return client.post('/v1/ingress/checkbox', json={
        'context': _ctx(req_id, channel_owner=channel_owner,
                        transport_profile_id=transport_profile_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1f',
            'goods': [{'name': 'Widget', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    })


def _assert_channel_switch_forbidden(resp, label: str) -> None:
    assert resp.status_code == 200, (
        f'{label}: expected 200 (controlled error), got {resp.status_code}: {resp.text}'
    )
    body = resp.json()
    assert body.get('error_code') == 'SHIFT_CHANNEL_SWITCH_FORBIDDEN', (
        f'{label}: expected SHIFT_CHANNEL_SWITCH_FORBIDDEN, got: {body}'
    )
    assert body.get('document_state') != 'ACK', (
        f'{label}: SELL must not ACK on channel lock violation: {body}'
    )


# ---------------------------------------------------------------------------
# Vector A — channel_owner mismatch
# ---------------------------------------------------------------------------

def test_gate1f_channel_owner_mismatch_blocks_sell(tmp_path: Path) -> None:
    """
    Shift opened by 'owner-a'. SELL from 'owner-b' must be rejected.

    lock.integration_owner = 'owner-a'
    command.channel_owner   = 'owner-b'
    → _channel_matches returns False → SHIFT_CHANNEL_SWITCH_FORBIDDEN.
    """
    cfg = _config(tmp_path, 'gate1f_vec_a.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_shift_open_only))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    OWNER_A = 'gate1f-owner-a'
    OWNER_B = 'gate1f-owner-b'

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1f-open-vec-a', channel_owner=OWNER_A)
        resp = _sell(client, 'gate1f-sell-vec-a', 'ext-vec-a', channel_owner=OWNER_B)

    _assert_channel_switch_forbidden(resp, 'Vector A (owner mismatch)')

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()
        shift = conn.execute(
            "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()

    assert len(sell_docs) == 0, (
        f'Vector A: no SELL document must be created on channel lock violation: {sell_docs}'
    )
    assert shift is not None and shift[0] == 'OPENED', (
        f'Vector A: shift must remain OPENED after rejected SELL: {shift}'
    )


# ---------------------------------------------------------------------------
# Vector B — transport_profile_id mismatch
# ---------------------------------------------------------------------------

def test_gate1f_transport_profile_mismatch_blocks_sell(tmp_path: Path) -> None:
    """
    Shift opened with transport_checkbox_rest_default.
    SELL sent with a different transport_profile_id.

    lock.transport_profile_id = 'transport_checkbox_rest_default'
    command.transport_profile_id = 'transport_webcheck_stub'
    → _channel_matches returns False → SHIFT_CHANNEL_SWITCH_FORBIDDEN.
    """
    cfg = _config(tmp_path, 'gate1f_vec_b.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_shift_open_only))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    OWNER = 'gate1f-owner-vec-b'
    ALT_TRANSPORT = 'transport_webcheck_stub'

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1f-open-vec-b', channel_owner=OWNER)
        resp = _sell(
            client, 'gate1f-sell-vec-b', 'ext-vec-b',
            channel_owner=OWNER,
            transport_profile_id=ALT_TRANSPORT,
        )

    _assert_channel_switch_forbidden(resp, 'Vector B (transport_profile mismatch)')

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()
        shift = conn.execute(
            "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()

    assert len(sell_docs) == 0, (
        f'Vector B: no SELL document must be created on channel lock violation: {sell_docs}'
    )
    assert shift is not None and shift[0] == 'OPENED', (
        f'Vector B: shift must remain OPENED after rejected SELL: {shift}'
    )
