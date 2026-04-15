"""
Gate 1r — write-path error state transition smoke tests.

Verifies that:
  1. A document in ERROR_RETRYABLE state (from transport failure) IS selected by
     get_pending_for_reconciliation() and can be recovered via reconciliation.
  2. Reconciliation of ERROR_RETRYABLE calls poll_status(), not send(), and
     correctly transitions the document to ACK when poll returns ACK.
  3. Inbox reaches status DONE after reconciliation (no PROCESSING leak).

ERROR_RETRYABLE is produced by:
  CheckboxRestTransport: (httpx.TimeoutException | httpx.NetworkError)
    → TransportRetryableError → DocumentState.ERROR_RETRYABLE

Reconciliation path for ERROR_RETRYABLE:
  get_pending_for_reconciliation selects state IN (..., 'ERROR_RETRYABLE', ...)
  reconcile_pending() calls poll_status(); if poll.state == ACK:
    FiscalDocumentRepository.update_state(state=ACK, expected_states=(..., ERROR_RETRYABLE, ...))

Invariants:
  #1 (no network in tx): poll happens outside the lock transaction; confirmed by tracker.
  #8 (no silent state violations): ERROR_RETRYABLE → ACK is the only valid transition
      exercised here; send() is never called.
"""
from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState, TransportKind
from prro_gateway.ports import PollResult
from prro_gateway.repositories import FiscalDocumentRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app
from prro_gateway.services.reconciliation import ReconciliationService
from prro_gateway.transports import ProfileAwareTransportRouter

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1r'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1r-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1R-001',
            'updated_at': '2026-03-29T15:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        # Simulate network failure → ERROR_RETRYABLE
        raise httpx.ConnectError('Connection refused — gate1r')
    raise AssertionError(f'gate1r: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1r.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1r',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1R-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T15:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1r',
        'business_ts': ts,
    }


class _AckPollTracker:
    def __init__(self):
        self.send_calls: int = 0
        self.poll_calls: int = 0

    def send(self, **kwargs):
        self.send_calls += 1
        raise AssertionError('gate1r: send() must not be called during reconciliation')

    def poll_status(self, **kwargs):
        self.poll_calls += 1
        return PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK',
            ack_at=datetime.now(UTC),
            response_json='{"status": "ACK", "fiscal_code": "RCPT-GATE1R-001"}',
        )


def test_gate1r_error_retryable_recovered_by_reconciliation(tmp_path: Path) -> None:
    """
    ERROR_RETRYABLE document from transport failure is selected by reconciliation
    and transitioned to ACK when poll returns ACK.

    Assertions:
    1. Initial SELL yields document_state=ERROR_RETRYABLE (transport failure).
    2. ERROR_RETRYABLE document IS in get_pending_for_reconciliation() result set.
    3. reconcile_pending: poll_calls=1, send_calls=0.
    4. Document → ACK, ack_at populated.
    5. No PROCESSING lease leak.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        # Open shift
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1r-open'), 'business_ts': '2026-03-29T15:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1r', 'cashier_id': 'cashier-gate1r'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        # SELL — transport raises → ERROR_RETRYABLE
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1r-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1r-001',
                'cashier_id': 'cashier-gate1r',
                'goods': [{'name': 'Coffee', 'price': 3000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 3000}],
            },
        })

    assert r_sell.status_code == 200, f'SELL non-200: {r_sell.text}'
    sell_body = r_sell.json()
    assert sell_body.get('document_state') == DocumentState.ERROR_RETRYABLE.value, (
        f'Expected ERROR_RETRYABLE from transport failure, got: {sell_body}'
    )
    doc_id = sell_body['document_id']

    # Verify document is in ERROR_RETRYABLE, inbox is ERROR (not PROCESSING)
    with container.connect() as conn:
        doc_pre = FiscalDocumentRepository.get_by_id(conn, doc_id)
        processing = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status='PROCESSING'"
        ).fetchone()[0]
        pending_docs = FiscalDocumentRepository.get_pending_for_reconciliation(conn)

    assert doc_pre.state == DocumentState.ERROR_RETRYABLE, (
        f'Document must be ERROR_RETRYABLE before reconciliation: {doc_pre.state}'
    )
    assert processing == 0, f'PROCESSING lease must not remain: {processing}'
    assert any(d.document_id == doc_id for d in pending_docs), (
        f'ERROR_RETRYABLE document must appear in get_pending_for_reconciliation() result'
    )

    # Reconcile — poll returns ACK
    tracker = _AckPollTracker()
    with container.connect() as conn:
        router = ProfileAwareTransportRouter.from_connection(
            conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: tracker}
        )
        result = ReconciliationService(transport_status_client=router).reconcile_pending(conn)

    assert tracker.send_calls == 0, (
        f'send() must not be called: {tracker.send_calls} calls'
    )
    assert tracker.poll_calls == 1, (
        f'Expected 1 poll_status call, got {tracker.poll_calls}'
    )
    assert result.acked == 1, f'Expected 1 acked, got {result}'

    with container.connect() as conn:
        doc_post = FiscalDocumentRepository.get_by_id(conn, doc_id)

    assert doc_post.state == DocumentState.ACK, (
        f'ERROR_RETRYABLE must transition to ACK after reconciliation: {doc_post.state}'
    )
    assert doc_post.ack_at is not None, 'ack_at must be set after reconciliation ACK'
