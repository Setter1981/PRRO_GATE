"""
Gate 1e — shift state boundary smoke tests.

Two guard vectors:

  Vector A — SELL without an open shift:
    Send SELL directly with no prior SHIFT_OPEN.
    _guard_preconditions: active_shift is None → SHIFT_NOT_OPEN.
    Expected: HTTP 200, error_code='SHIFT_NOT_OPEN', no document_state=ACK,
              no document row, transport NOT called at all.

  Vector B — SHIFT_OPEN while a shift is already open:
    Open a shift normally (transport allowed for that call), then try to open
    a second shift.
    _guard_preconditions: active_shift is not None → SHIFT_ALREADY_OPEN.
    Expected: HTTP 200, error_code='SHIFT_ALREADY_OPEN', no second document,
              transport NOT called for the second SHIFT_OPEN.

Both vectors prove guards fire before the send stage (Invariant #1) and
that the responses are usable error signals (not crash, not false ACK).

Invariants preserved:
  #1 (no network in tx): guard fires before sign/send → transport not reached.
  #3 (no channel switch with open shift): not tested here (separate boundary).
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
# Mock transports
# ---------------------------------------------------------------------------

def _mock_no_transport(request: httpx.Request) -> httpx.Response:
    """Used for Vector A: no transport call must happen at all."""
    raise AssertionError(
        f'gate1e: transport reached when guard should have blocked — '
        f'{request.method} {request.url}'
    )


_shift_open_call_count = 0


def _mock_first_shift_only(request: httpx.Request) -> httpx.Response:
    """
    Used for Vector B: allows one shift-open sequence, then fails on any
    subsequent receipt/shift attempt so we can detect if the second
    SHIFT_OPEN reaches the transport.
    """
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1e'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1e-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1E-001',
            'updated_at': '2026-03-29T14:00:00+00:00',
        })
    raise AssertionError(
        f'gate1e: transport reached after guard should have blocked — '
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
            'channel_owner': 'gate1e',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1E-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T14:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1e',
        'business_ts': ts,
    }


def _shift_open_payload(req_id: str) -> dict:
    return {
        'context': {**_ctx(req_id), 'business_ts': '2026-03-29T14:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-{req_id}',
            'cashier_id': 'cashier-gate1e',
        },
    }


def _sell_payload(req_id: str, ext_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1e',
            'goods': [{'name': 'Widget', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    }


# ---------------------------------------------------------------------------
# Vector A — SELL without open shift
# ---------------------------------------------------------------------------

def test_gate1e_sell_without_shift_returns_shift_not_open(tmp_path: Path) -> None:
    """
    SELL with no open shift must return SHIFT_NOT_OPEN.
    Transport must NOT be called (guard fires before send stage).
    No document row must be created.
    """
    cfg = _config(tmp_path, 'gate1e_vec_a.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_no_transport))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        resp = client.post('/v1/ingress/checkbox', json=_sell_payload(
            'gate1e-sell-no-shift', 'ext-sell-no-shift'
        ))

    assert resp.status_code == 200, f'Expected 200 (controlled error), got {resp.status_code}: {resp.text}'
    body = resp.json()

    assert body.get('error_code') == 'SHIFT_NOT_OPEN', (
        f'Vector A: expected error_code=SHIFT_NOT_OPEN, got: {body}'
    )
    assert body.get('document_state') != 'ACK', (
        f'Vector A: SELL must not ACK without open shift: {body}'
    )

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()
    assert len(sell_docs) == 0, (
        f'Vector A: no SELL document must be created when guard blocks: {sell_docs}'
    )


# ---------------------------------------------------------------------------
# Vector B — SHIFT_OPEN while shift already open
# ---------------------------------------------------------------------------

def test_gate1e_shift_open_while_already_open_returns_shift_already_open(tmp_path: Path) -> None:
    """
    Second SHIFT_OPEN while a shift is active must return SHIFT_ALREADY_OPEN.
    Transport must NOT be called for the second attempt.
    Only 1 SHIFT_OPEN document must exist in DB.
    """
    cfg = _config(tmp_path, 'gate1e_vec_b.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_first_shift_only))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        # First SHIFT_OPEN — must succeed
        r1 = client.post('/v1/ingress/checkbox', json=_shift_open_payload('gate1e-open-first'))
        assert r1.status_code == 200, f'First SHIFT_OPEN failed: {r1.text}'
        assert r1.json().get('document_state') == 'ACK', (
            f'First SHIFT_OPEN not ACK: {r1.json()}'
        )

        # Second SHIFT_OPEN while shift is OPENED — guard must block it
        r2 = client.post('/v1/ingress/checkbox', json=_shift_open_payload('gate1e-open-second'))

    assert r2.status_code == 200, (
        f'Vector B: expected 200 (controlled error), got {r2.status_code}: {r2.text}'
    )
    body2 = r2.json()

    assert body2.get('error_code') == 'SHIFT_ALREADY_OPEN', (
        f'Vector B: expected error_code=SHIFT_ALREADY_OPEN, got: {body2}'
    )
    assert body2.get('document_state') != 'ACK', (
        f'Vector B: second SHIFT_OPEN must not ACK: {body2}'
    )

    with container.connect() as conn:
        shift_docs = conn.execute(
            "SELECT document_id, state FROM fiscal_documents WHERE doc_type = 'SHIFT_OPEN'"
        ).fetchall()
        shift_rows = conn.execute(
            "SELECT shift_id, state FROM shifts WHERE fiscal_number = ?",
            (FISCAL_NUMBER,),
        ).fetchall()

    # Exactly 1 SHIFT_OPEN document — no duplicate
    assert len(shift_docs) == 1, (
        f'Vector B: expected 1 SHIFT_OPEN document, got {shift_docs}'
    )
    assert shift_docs[0][1] == 'ACK', (
        f'Vector B: SHIFT_OPEN document not ACK: {shift_docs[0]}'
    )

    # Exactly 1 shift row
    assert len(shift_rows) == 1, (
        f'Vector B: expected 1 shift row, got {shift_rows}'
    )
    assert shift_rows[0][1] == 'OPENED', (
        f'Vector B: shift not OPENED after guard: {shift_rows[0]}'
    )
