"""
Gate 1x — startup reconciliation idempotency smoke test.

Verifies that running startup reconciliation on a database where all documents
are already in ACK state is a safe no-op:

  - get_pending_for_reconciliation() returns empty list (ACK not in candidate states)
  - Transport is never called (no docs to process)
  - reconciliation_checked == 0 (len of candidates fetched)
  - reconciliation_acked == 0
  - Document state is not mutated

Candidate states: SENT, KVT1, KVT2, ERROR_RETRYABLE, REQUIRES_MANUAL_RECONCILIATION.
ACK is terminal and excluded from selection.

Invariant #8: reconciliation must not silently violate state transitions.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.repositories import FiscalDocumentRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1x'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1x-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1X-001',
            'updated_at': '2026-03-30T09:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1x-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1X-001',
            'updated_at': '2026-03-30T09:01:00+00:00',
        })
    raise AssertionError(f'gate1x phase1: unexpected {request.method} {request.url}')


def _strict_no_call(request: httpx.Request) -> httpx.Response:
    """Phase 2 guard: transport must not be called when all docs are already ACK."""
    raise AssertionError(
        f'gate1x: transport must NOT be called during reconciliation of already-ACK docs: '
        f'{request.method} {request.url}'
    )


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1x.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1x',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1X-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T09:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1x',
        'business_ts': ts,
    }


def test_gate1x_reconciliation_is_noop_when_all_docs_are_ack(tmp_path: Path) -> None:
    """
    When all documents are already ACK, a second container's startup reconciliation
    must be a no-op: get_pending_for_reconciliation() returns empty list, transport
    is never called, counters remain zero, document state is not mutated.
    """
    cfg = _config(tmp_path)

    # Phase 1: SHIFT_OPEN + SELL — both reach ACK, no crash simulation
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1x-open'), 'business_ts': '2026-03-30T09:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1x', 'cashier_id': 'cashier-gate1x'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1x-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1x-001',
                'cashier_id': 'cashier-gate1x',
                'goods': [{'name': 'Tea', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Confirm sell doc is ACK — no manipulation
    with container1.connect() as conn:
        doc_before = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc_before.state == DocumentState.ACK

    # Phase 2: new container, same DB, strict transport (raises on any call)
    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_call))
    )
    with TestClient(create_app(container2)) as _:
        assert container2.health.ready, 'health.ready must be True during lifespan'
        assert container2.health.startup_complete, 'health.startup_complete must be True'

    report = container2.last_startup_report
    assert report is not None, 'last_startup_report must be populated'
    assert report.phase2_attempted, 'Phase 2 must be attempted (reconcile_on_startup=True)'
    assert report.reconciliation_checked == 0, (
        f'reconciliation_checked must be 0 (ACK docs not in candidate states): '
        f'got {report.reconciliation_checked}. Report: {report}'
    )
    assert report.reconciliation_acked == 0, (
        f'reconciliation_acked must be 0: got {report.reconciliation_acked}'
    )

    # Document state must remain ACK — idempotency proven
    with container2.connect() as conn:
        doc_after = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc_after.state == DocumentState.ACK, (
        f'Document must remain ACK after idempotent reconciliation: got {doc_after.state}'
    )
    assert doc_after.ack_at == doc_before.ack_at, (
        'ack_at must not be mutated by idempotent reconciliation'
    )
