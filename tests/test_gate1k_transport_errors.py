"""
Gate 1k — transport error handling smoke tests.

Two error vectors via httpx mock transports:

  Vector A — network error (retryable):
    Mock transport raises httpx.ConnectError during /receipts/sell.
    CheckboxRestTransport catches (httpx.TimeoutException, httpx.NetworkError) and
    re-raises TransportRetryableError.
    WritePathWorker catches TransportRetryableError →
    _mark_document_and_inbox_error(state=ERROR_RETRYABLE).

    Expected:
      - HTTP 200 (controlled error)
      - document_state = 'ERROR_RETRYABLE'
      - inbox status = 'ERROR' (not PROCESSING — lease released)
      - no crash

  Vector B — rejected (4xx non-retryable):
    Mock transport returns HTTP 422 for /receipts/sell.
    CheckboxRestTransport._raise_for_status: 4xx → TransportRejectedError.
    WritePathWorker catches TransportRejectedError →
    _mark_document_and_inbox_error(state=REJECTED).

    Expected:
      - HTTP 200 (controlled error)
      - document_state = 'REJECTED'
      - inbox status = 'ERROR'
      - no crash

Both vectors prove Invariant #8 (recovery must not silently violate state transitions)
and confirm no PROCESSING lease leaks.

Invariants preserved:
  #1 (no network in tx): error caught outside transaction, _mark_document_and_inbox_error
      opens its own BEGIN IMMEDIATE.
  #8 (no silent state violation): ERROR_RETRYABLE and REJECTED are valid terminal/
      recovery-eligible states per DocumentState enum.
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
            'channel_owner': 'gate1k',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1K-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1k',
        'business_ts': '2026-03-29T19:01:00Z',
    }


def _open_shift(client: TestClient, req_id: str) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx(req_id), 'business_ts': '2026-03-29T19:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-{req_id}',
            'cashier_id': 'cashier-gate1k',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _sell_payload(req_id: str, ext_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1k',
            'goods': [{'name': 'Widget', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    }


def _assert_no_processing_lease(container: RuntimeContainer, label: str) -> None:
    with container.connect() as conn:
        count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status = 'PROCESSING'"
        ).fetchone()[0]
    assert count == 0, (
        f'{label}: found {count} inbox record(s) stuck in PROCESSING — lease leaked'
    )


# ---------------------------------------------------------------------------
# Vector A — network error → ERROR_RETRYABLE
# ---------------------------------------------------------------------------

def test_gate1k_network_error_yields_retryable(tmp_path: Path) -> None:
    """
    httpx.ConnectError during receipt send must produce document_state=ERROR_RETRYABLE
    and no stuck PROCESSING inbox lease.
    """
    send_attempted = False

    def _mock(request: httpx.Request) -> httpx.Response:
        nonlocal send_attempted
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-gate1k'})
        if path.endswith('/shifts'):
            return httpx.Response(200, json={
                'id': 'gate1k-shift-001',
                'status': 'OPENED',
                'fiscal_code': 'SHIFT-GATE1K-001',
                'updated_at': '2026-03-29T19:00:00+00:00',
            })
        if path.endswith('/receipts/sell'):
            send_attempted = True
            # Simulate a network failure — Checkbox transport wraps this as
            # TransportRetryableError
            raise httpx.ConnectError('Connection refused by peer')
        raise AssertionError(f'gate1k: unexpected {request.method} {request.url}')

    cfg = _config(tmp_path, 'gate1k_vec_a.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1k-open-vec-a')
        resp = client.post('/v1/ingress/checkbox', json=_sell_payload(
            'gate1k-sell-vec-a', 'ext-vec-a'
        ))

    assert send_attempted, 'gate1k: transport send was not reached at all'
    assert resp.status_code == 200, (
        f'Vector A: expected HTTP 200 (controlled error), got {resp.status_code}: {resp.text}'
    )
    body = resp.json()
    assert body.get('document_state') == DocumentState.ERROR_RETRYABLE.value, (
        f'Vector A: expected document_state=ERROR_RETRYABLE, got: {body}'
    )
    assert body.get('document_state') != 'ACK', (
        f'Vector A: network error must not yield ACK: {body}'
    )

    # No stuck PROCESSING lease
    _assert_no_processing_lease(container, 'Vector A')

    # Document exists in ERROR_RETRYABLE state, has error_message
    doc_id = body.get('document_id')
    assert doc_id is not None, f'Vector A: document_id missing from response: {body}'
    with container.connect() as conn:
        doc = conn.execute(
            "SELECT state, canonical_error_code FROM fiscal_documents WHERE document_id = ?",
            (doc_id,),
        ).fetchone()
    assert doc is not None, f'Vector A: document {doc_id!r} not in DB'
    assert doc[0] == DocumentState.ERROR_RETRYABLE.value, (
        f'Vector A: DB document state={doc[0]!r}, expected ERROR_RETRYABLE'
    )
    assert doc[1] == 'TRANSPORT_RETRYABLE_ERROR', (
        f'Vector A: canonical_error_code={doc[1]!r}, expected TRANSPORT_RETRYABLE_ERROR'
    )


# ---------------------------------------------------------------------------
# Vector B — HTTP 4xx rejected → REJECTED
# ---------------------------------------------------------------------------

def test_gate1k_http_4xx_yields_rejected(tmp_path: Path) -> None:
    """
    HTTP 422 from Checkbox receipt endpoint must produce document_state=REJECTED
    (non-retryable) and no stuck PROCESSING inbox lease.
    """
    send_attempted = False

    def _mock(request: httpx.Request) -> httpx.Response:
        nonlocal send_attempted
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-gate1k-b'})
        if path.endswith('/shifts'):
            return httpx.Response(200, json={
                'id': 'gate1k-shift-002',
                'status': 'OPENED',
                'fiscal_code': 'SHIFT-GATE1K-002',
                'updated_at': '2026-03-29T19:00:00+00:00',
            })
        if path.endswith('/receipts/sell'):
            send_attempted = True
            # 4xx → TransportRejectedError → REJECTED
            return httpx.Response(422, json={'message': 'Receipt validation failed'})
        raise AssertionError(f'gate1k: unexpected {request.method} {request.url}')

    cfg = _config(tmp_path, 'gate1k_vec_b.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1k-open-vec-b')
        resp = client.post('/v1/ingress/checkbox', json=_sell_payload(
            'gate1k-sell-vec-b', 'ext-vec-b'
        ))

    assert send_attempted, 'gate1k: transport send was not reached at all'
    assert resp.status_code == 200, (
        f'Vector B: expected HTTP 200 (controlled error), got {resp.status_code}: {resp.text}'
    )
    body = resp.json()
    assert body.get('document_state') == DocumentState.REJECTED.value, (
        f'Vector B: expected document_state=REJECTED, got: {body}'
    )
    assert body.get('document_state') != 'ACK', (
        f'Vector B: 4xx must not yield ACK: {body}'
    )

    # No stuck PROCESSING lease
    _assert_no_processing_lease(container, 'Vector B')

    # Document is REJECTED, error captured
    doc_id = body.get('document_id')
    assert doc_id is not None, f'Vector B: document_id missing from response: {body}'
    with container.connect() as conn:
        doc = conn.execute(
            "SELECT state, canonical_error_code FROM fiscal_documents WHERE document_id = ?",
            (doc_id,),
        ).fetchone()
    assert doc is not None, f'Vector B: document {doc_id!r} not in DB'
    assert doc[0] == DocumentState.REJECTED.value, (
        f'Vector B: DB document state={doc[0]!r}, expected REJECTED'
    )
    assert doc[1] == 'BACKEND_TEMPORARY_UNAVAILABLE', (
        f'Vector B: canonical_error_code={doc[1]!r}, expected BACKEND_TEMPORARY_UNAVAILABLE'
    )
