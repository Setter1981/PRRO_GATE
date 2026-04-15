"""
Gate 1t — document-level REST error visibility smoke tests.

Verifies that the REST responses for document-level error states
(ERROR_RETRYABLE, REJECTED) expose sufficient information for clients
to act without an additional GET call.

POST /v1/ingress/checkbox response (when doc_id is present):
  Always returns: document_id, document_state, server_fiscal_no, error_message.
  After fix also returns: canonical_error_code (machine-readable error anchor).

GET /v1/documents/{request_id} response:
  Returns full document snapshot including canonical_error_code.

Gap identified and closed:
  Before: POST response for doc-level errors had error_message but not
          canonical_error_code, forcing a second GET call to get the code.
  Fix: one-line addition in rest_app.py if-doc_id branch.
  After: canonical_error_code is present in both POST and GET responses.

Tests:
  A — POST response for REJECTED includes canonical_error_code.
  B — POST response for ERROR_RETRYABLE includes canonical_error_code.
  C — GET /v1/documents/{request_id} returns canonical_error_code (unchanged).
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _mock_rejected(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1t'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1t-shift', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1T', 'updated_at': '2026-03-29T16:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(422, json={'message': 'Validation error — gate1t'})
    raise AssertionError(f'gate1t: unexpected {request.method} {request.url}')


def _mock_retryable(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1t'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1t-shift', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1T', 'updated_at': '2026-03-29T16:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        raise httpx.ConnectError('Connection refused — gate1t')
    raise AssertionError(f'gate1t: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate1t.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate1t',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1T-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T16:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1t',
        'business_ts': ts,
    }


def _open_shift(client: TestClient, req_id: str = 'gate1t-open') -> None:
    r = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx(req_id), 'business_ts': '2026-03-29T16:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {'external_request_id': f'ext-open-{req_id}', 'cashier_id': 'cashier-gate1t'},
    })
    assert r.status_code == 200
    assert r.json().get('document_state') == 'ACK'


def _sell_body(req_id: str, ext_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1t',
            'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 2000}],
        },
    }


# ---------------------------------------------------------------------------
# Test A — POST response for REJECTED includes canonical_error_code
# ---------------------------------------------------------------------------

def test_gate1t_rejected_post_response_includes_canonical_error_code(tmp_path: Path) -> None:
    """
    POST /v1/ingress/checkbox for a REJECTED SELL must include:
      - document_id
      - document_state = 'REJECTED'
      - error_message (set by write-path)
      - canonical_error_code (machine-readable anchor, e.g. BACKEND_TEMPORARY_UNAVAILABLE)
    """
    cfg = _config(tmp_path, 'gate1t_a.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_rejected))
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1t-open-a')
        r = client.post('/v1/ingress/checkbox',
                        json=_sell_body('gate1t-sell-rejected', 'ext-sell-gate1t-rejected'))

    assert r.status_code == 200
    body = r.json()
    assert body.get('document_state') == DocumentState.REJECTED.value, (
        f'Expected REJECTED, got: {body}'
    )
    assert 'document_id' in body, f'document_id missing from POST response: {body}'
    assert body.get('error_message') is not None, (
        f'error_message must be in POST response for REJECTED: {body}'
    )
    assert 'canonical_error_code' in body, (
        f'canonical_error_code missing from POST response for REJECTED: {body}\n'
        f'Client needs canonical_error_code to distinguish error types without a second GET call.'
    )
    assert body['canonical_error_code'] == 'BACKEND_TEMPORARY_UNAVAILABLE', (
        f'Expected BACKEND_TEMPORARY_UNAVAILABLE, got {body["canonical_error_code"]!r}'
    )


# ---------------------------------------------------------------------------
# Test B — POST response for ERROR_RETRYABLE includes canonical_error_code
# ---------------------------------------------------------------------------

def test_gate1t_retryable_post_response_includes_canonical_error_code(tmp_path: Path) -> None:
    """
    POST /v1/ingress/checkbox for an ERROR_RETRYABLE SELL must include canonical_error_code.
    """
    cfg = _config(tmp_path, 'gate1t_b.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_retryable))
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1t-open-b')
        r = client.post('/v1/ingress/checkbox',
                        json=_sell_body('gate1t-sell-retryable', 'ext-sell-gate1t-retryable'))

    assert r.status_code == 200
    body = r.json()
    assert body.get('document_state') == DocumentState.ERROR_RETRYABLE.value, (
        f'Expected ERROR_RETRYABLE, got: {body}'
    )
    assert 'canonical_error_code' in body, (
        f'canonical_error_code missing from POST response for ERROR_RETRYABLE: {body}'
    )
    assert body['canonical_error_code'] == 'TRANSPORT_RETRYABLE_ERROR', (
        f'Expected TRANSPORT_RETRYABLE_ERROR, got {body["canonical_error_code"]!r}'
    )


# ---------------------------------------------------------------------------
# Test C — GET /v1/documents/{request_id} returns canonical_error_code
# ---------------------------------------------------------------------------

def test_gate1t_get_document_endpoint_returns_canonical_error_code(tmp_path: Path) -> None:
    """
    GET /v1/documents/{request_id} must return canonical_error_code for error docs.
    This endpoint already exposed it before the POST fix.
    """
    cfg = _config(tmp_path, 'gate1t_c.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_rejected))
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1t-open-c')
        r_sell = client.post('/v1/ingress/checkbox',
                             json=_sell_body('gate1t-sell-get', 'ext-sell-gate1t-get'))
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == DocumentState.REJECTED.value

        r_get = client.get(f'/v1/documents/gate1t-sell-get')

    assert r_get.status_code == 200, f'GET non-200: {r_get.text}'
    get_body = r_get.json()
    assert get_body.get('state') == DocumentState.REJECTED.value
    assert get_body.get('canonical_error_code') == 'BACKEND_TEMPORARY_UNAVAILABLE', (
        f'GET endpoint must return canonical_error_code: {get_body}'
    )
    assert get_body.get('error_message') is not None
