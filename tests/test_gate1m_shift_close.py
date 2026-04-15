"""
Gate 1m — shift close / Z-report state machine smoke test.

Full lifecycle: SHIFT_OPEN → SELL → SHIFT_CLOSE → post-close SELL rejected.

State transitions verified:
  SHIFT_OPEN   → document ACK, shift state = OPENED
  SELL         → document ACK, shift remains OPENED
  SHIFT_CLOSE  → document ACK, shift state = CLOSED
                 (_sync_shift_close_locked: target_state==ACK →
                  ShiftRepository.update_state(..., CLOSED))
  SELL (post)  → guard rejects: active_shift is None (CLOSED excluded from
                  get_active_shift query) → SHIFT_NOT_OPEN

Transport mapping (checkbox_rest.py):
  SHIFT_CLOSE sends POST /shifts/close.
  status='CLOSED' in response → _map_checkbox_state returns DocumentState.ACK.

Invariants preserved:
  #3 (channel switch forbidden): not touched — both OPEN and CLOSE use same channel.
  #8 (no silent state violations): shift CLOSED is terminal; guard enforces it.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import ShiftState
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1m'})
    if path.endswith('/shifts') and request.method == 'POST':
        # SHIFT_OPEN
        return httpx.Response(200, json={
            'id': 'gate1m-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1M-001',
            'updated_at': '2026-03-29T21:00:00+00:00',
        })
    if path.endswith('/shifts/close'):
        # SHIFT_CLOSE
        return httpx.Response(200, json={
            'id': 'gate1m-shift-001',
            'status': 'CLOSED',
            'fiscal_code': 'SHIFT-GATE1M-001',
            'updated_at': '2026-03-29T21:30:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1m-receipt',
            'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1M',
            'updated_at': '2026-03-29T21:05:00+00:00',
        })
    raise AssertionError(f'gate1m: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1m.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1m',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1M-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T21:05:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1m',
        'business_ts': ts,
    }


def test_gate1m_shift_close_lifecycle(tmp_path: Path) -> None:
    """
    Full lifecycle: OPEN → SELL → CLOSE → post-close SELL rejected.

    Assertions:
    1. SHIFT_OPEN: document ACK, shift OPENED.
    2. SELL: document ACK, shift still OPENED.
    3. SHIFT_CLOSE: document ACK, shift CLOSED.
    4. post-close SELL: controlled error, no ACK, no new document.
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:

        # --- Step 1: SHIFT_OPEN ---
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1m-open'), 'business_ts': '2026-03-29T21:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-open-gate1m',
                'cashier_id': 'cashier-gate1m',
            },
        })
        assert r_open.status_code == 200, f'SHIFT_OPEN failed: {r_open.text}'
        assert r_open.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {r_open.json()}'

        with container.connect() as conn:
            shift_after_open = conn.execute(
                "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
                (FISCAL_NUMBER,),
            ).fetchone()
        assert shift_after_open is not None and shift_after_open[0] == ShiftState.OPENED.value, (
            f'Shift must be OPENED after SHIFT_OPEN, got {shift_after_open}'
        )

        # --- Step 2: SELL ---
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1m-sell'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1m',
                'cashier_id': 'cashier-gate1m',
                'goods': [{'name': 'Coffee', 'price': 3000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 3000}],
            },
        })
        assert r_sell.status_code == 200, f'SELL failed: {r_sell.text}'
        assert r_sell.json().get('document_state') == 'ACK', f'SELL not ACK: {r_sell.json()}'

        with container.connect() as conn:
            shift_after_sell = conn.execute(
                "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
                (FISCAL_NUMBER,),
            ).fetchone()
        assert shift_after_sell is not None and shift_after_sell[0] == ShiftState.OPENED.value, (
            f'Shift must remain OPENED after SELL, got {shift_after_sell}'
        )

        # --- Step 3: SHIFT_CLOSE ---
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1m-close'), 'business_ts': '2026-03-29T21:30:00Z'},
            'operation': 'SHIFT_CLOSE',
            'request': {
                'external_request_id': 'ext-close-gate1m',
                'cashier_id': 'cashier-gate1m',
            },
        })
        assert r_close.status_code == 200, f'SHIFT_CLOSE failed: {r_close.text}'
        close_body = r_close.json()
        assert close_body.get('document_state') == 'ACK', (
            f'SHIFT_CLOSE document not ACK: {close_body}'
        )

        with container.connect() as conn:
            shift_after_close = conn.execute(
                "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
                (FISCAL_NUMBER,),
            ).fetchone()
            close_doc_id = close_body.get('document_id')
            close_doc = conn.execute(
                "SELECT state, doc_type FROM fiscal_documents WHERE document_id = ?",
                (close_doc_id,),
            ).fetchone() if close_doc_id else None

        assert shift_after_close is not None and shift_after_close[0] == ShiftState.CLOSED.value, (
            f'Shift must be CLOSED after SHIFT_CLOSE, got {shift_after_close}'
        )
        assert close_doc is not None and close_doc[0] == 'ACK', (
            f'SHIFT_CLOSE document must be ACK in DB, got {close_doc}'
        )
        assert close_doc[1] == 'SHIFT_CLOSE', (
            f'Close document doc_type must be SHIFT_CLOSE, got {close_doc[1]!r}'
        )

        # --- Step 4: SELL after close — must be rejected ---
        r_post_close_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1m-sell-post-close'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-post-close',
                'cashier_id': 'cashier-gate1m',
                'goods': [{'name': 'Tea', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })

    assert r_post_close_sell.status_code == 200, (
        f'Post-close SELL must return 200 (controlled error), '
        f'got {r_post_close_sell.status_code}: {r_post_close_sell.text}'
    )
    post_close_body = r_post_close_sell.json()
    assert post_close_body.get('error_code') == 'SHIFT_NOT_OPEN', (
        f'Post-close SELL must return SHIFT_NOT_OPEN, got: {post_close_body}'
    )
    assert post_close_body.get('document_state') != 'ACK', (
        f'Post-close SELL must not ACK: {post_close_body}'
    )

    # No SELL document created after close
    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT COUNT(*) FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchone()[0]
    assert sell_docs == 1, (
        f'Exactly 1 SELL document must exist (the one before close), found {sell_docs}'
    )
