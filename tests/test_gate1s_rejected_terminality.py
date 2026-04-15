"""
Gate 1s — REJECTED state terminality smoke test.

Verifies that a document in state REJECTED is NOT selected by
get_pending_for_reconciliation() and therefore cannot be re-processed
by reconciliation without explicit manual intervention.

Terminality mechanism:
  FiscalDocumentRepository.get_pending_for_reconciliation() filters by:
    state IN ('SENT','KVT1','KVT2','ERROR_RETRYABLE','REQUIRES_MANUAL_RECONCILIATION')
  REJECTED is not in this set → document is never picked up.

REJECTED is produced by:
  CheckboxRestTransport: HTTP 4xx response → TransportRejectedError
    → DocumentState.REJECTED, canonical_error_code='BACKEND_TEMPORARY_UNAVAILABLE'

Two sub-assertions:
  A — REJECTED document does NOT appear in get_pending_for_reconciliation().
  B — reconcile_pending() with a strict tracker calls no transport methods;
      document remains REJECTED with all fields intact.

Invariants:
  #8 (recovery must not silently violate state transitions): REJECTED must not
      be moved to any other state without explicit manual action.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState, TransportKind
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
        return httpx.Response(200, json={'access_token': 'mock-token-gate1s'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1s-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1S-001',
            'updated_at': '2026-03-29T15:30:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        # HTTP 4xx → TransportRejectedError → REJECTED
        return httpx.Response(422, json={'message': 'Validation failed — gate1s'})
    raise AssertionError(f'gate1s: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1s.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1s',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1S-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1s',
        'business_ts': '2026-03-29T15:31:00Z',
    }


class _StrictNoCallTracker:
    def __init__(self):
        self.send_calls: int = 0
        self.poll_calls: int = 0

    def send(self, **kwargs):
        self.send_calls += 1
        raise AssertionError('gate1s: send() called — REJECTED document must not be re-sent')

    def poll_status(self, **kwargs):
        self.poll_calls += 1
        raise AssertionError(
            'gate1s: poll_status() called — REJECTED document must not enter reconciliation'
        )


def test_gate1s_rejected_document_is_terminal(tmp_path: Path) -> None:
    """
    A REJECTED document from HTTP 4xx:
      A: does NOT appear in get_pending_for_reconciliation().
      B: reconcile_pending() calls no transport methods.
      C: document remains REJECTED with canonical_error_code and error_message intact.
      D: No PROCESSING lease leak.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1s-open'), 'business_ts': '2026-03-29T15:30:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1s', 'cashier_id': 'cashier-gate1s'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1s-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1s-001',
                'cashier_id': 'cashier-gate1s',
                'goods': [{'name': 'Widget', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })

    assert r_sell.status_code == 200
    sell_body = r_sell.json()
    assert sell_body.get('document_state') == DocumentState.REJECTED.value, (
        f'Expected REJECTED from HTTP 422, got: {sell_body}'
    )
    doc_id = sell_body['document_id']

    with container.connect() as conn:
        doc_pre = FiscalDocumentRepository.get_by_id(conn, doc_id)
        processing = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status='PROCESSING'"
        ).fetchone()[0]
        # A — not in reconciliation candidate set
        pending_docs = FiscalDocumentRepository.get_pending_for_reconciliation(conn)

    assert doc_pre.state == DocumentState.REJECTED, (
        f'Document must be REJECTED: {doc_pre.state}'
    )
    assert doc_pre.canonical_error_code is not None, (
        'canonical_error_code must be set for REJECTED documents'
    )
    assert doc_pre.error_message is not None, (
        'error_message must be set for REJECTED documents'
    )
    assert processing == 0, f'PROCESSING lease must not remain: {processing}'

    # A — REJECTED must not be in reconciliation queue
    rejected_in_pending = [d for d in pending_docs if d.document_id == doc_id]
    assert rejected_in_pending == [], (
        f'REJECTED document must NOT appear in get_pending_for_reconciliation(): {rejected_in_pending}'
    )

    # B — reconcile_pending must not touch it
    tracker = _StrictNoCallTracker()
    with container.connect() as conn:
        router = ProfileAwareTransportRouter.from_connection(
            conn, handlers={TransportKind.CHECKBOX_REST_TRANSPORT: tracker}
        )
        result = ReconciliationService(transport_status_client=router).reconcile_pending(conn)

    assert tracker.send_calls == 0, (
        f'send() must not be called on REJECTED document: {tracker.send_calls}'
    )
    assert tracker.poll_calls == 0, (
        f'poll_status() must not be called on REJECTED document: {tracker.poll_calls}'
    )
    assert result.acked == 0, f'No documents should be ACKed: {result}'

    # C — document must remain exactly REJECTED
    with container.connect() as conn:
        doc_post = FiscalDocumentRepository.get_by_id(conn, doc_id)

    assert doc_post.state == DocumentState.REJECTED, (
        f'REJECTED must remain terminal after reconciliation: {doc_post.state}'
    )
    assert doc_post.canonical_error_code == doc_pre.canonical_error_code, (
        f'canonical_error_code must not change: {doc_post.canonical_error_code!r}'
    )
    assert doc_post.error_message == doc_pre.error_message, (
        'error_message must not change after reconciliation'
    )
