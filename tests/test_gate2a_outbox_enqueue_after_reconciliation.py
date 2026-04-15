"""
Gate 2a — outbox enqueue after reconciliation ACK smoke test.

Verifies that ReconciliationService correctly enqueues a sync_outbox record
when it transitions a SENT document to ACK via poll_status().

Outbox record created by reconciliation:
  outbox_id      = '{document_id}-reconcile-outbox'
  aggregate_id   = document_id
  fiscal_number  = FISCAL_NUMBER
  status         = 'PENDING'
  destination    = 'CLOUD_HUB'
  aggregate_type = 'DOCUMENT'
  artifact_class = 'DOCUMENT'
  payload_path   = '/archive/{fiscal_number}/{document_id}/response.json'
  payload_sha256 = 'reconcile'

This is distinct from the write-path outbox record ('{document_id}-outbox'),
which is created when the transport returns ACK during the initial send.
Both records can coexist because they have different outbox_ids.

Setup uses crash-simulation (ACK → SQL reset to SENT → new container reconciles),
so the write-path outbox record also exists. The test checks the reconciliation
record specifically.
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
RECEIPT_ID = 'gate2a-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2a'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2a-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2A-001',
            'updated_at': '2026-03-30T12:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2A-001',
            'updated_at': '2026-03-30T12:01:00+00:00',
        })
    raise AssertionError(f'gate2a phase1: unexpected {request.method} {request.url}')


def _phase2_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2a-phase2'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2A-001',
            'updated_at': '2026-03-30T12:01:00+00:00',
        })
    raise AssertionError(f'gate2a phase2: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate2a.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate2a',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2A-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T12:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2a',
        'business_ts': ts,
    }


def test_gate2a_reconciliation_ack_creates_outbox_record(tmp_path: Path) -> None:
    """
    After reconciliation transitions a SENT document to ACK:
    - sync_outbox must contain a record with outbox_id='{document_id}-reconcile-outbox'
    - Record must be in PENDING status (ready for delivery)
    - aggregate_id, fiscal_number, destination must match the document
    - payload_path and payload_sha256 must match the reconciliation convention
    """
    cfg = _config(tmp_path)

    # Phase 1: SHIFT_OPEN + SELL → both ACK
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2a-open'), 'business_ts': '2026-03-30T12:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2a', 'cashier_id': 'cashier-gate2a'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2a-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2a-001',
                'cashier_id': 'cashier-gate2a',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Crash simulation: reset SELL to SENT
    with container1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        # Also remove the write-path outbox record so reconciliation is the sole enqueue
        conn.execute(
            "DELETE FROM sync_outbox WHERE outbox_id=?",
            (f'{sell_doc_id}-outbox',),
        )
        conn.commit()

    # Phase 2: new container — reconciliation runs and recovers SENT → ACK
    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_mock))
    )
    with TestClient(create_app(container2)) as _:
        assert container2.health.ready
        assert container2.health.startup_complete

    # Verify document reached ACK
    with container2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.ACK

        # Check the reconciliation outbox record
        row = conn.execute(
            """
            SELECT outbox_id, aggregate_type, aggregate_id, fiscal_number,
                   artifact_class, destination, payload_path, payload_sha256,
                   status, sequence_no
            FROM sync_outbox WHERE outbox_id = ?
            """,
            (f'{sell_doc_id}-reconcile-outbox',),
        ).fetchone()

    assert row is not None, (
        f'sync_outbox must contain a record with outbox_id={sell_doc_id!r}-reconcile-outbox '
        f'after reconciliation ACK. Record not found — reconciliation dropped the outbox enqueue.'
    )

    (outbox_id, agg_type, agg_id, fiscal_no,
     artifact_cls, destination, payload_path, payload_sha256,
     status, sequence_no) = row

    assert outbox_id == f'{sell_doc_id}-reconcile-outbox'
    assert agg_type == 'DOCUMENT', f'aggregate_type must be DOCUMENT: got {agg_type!r}'
    assert agg_id == sell_doc_id, f'aggregate_id must be document_id: got {agg_id!r}'
    assert fiscal_no == FISCAL_NUMBER, f'fiscal_number mismatch: {fiscal_no!r}'
    assert artifact_cls == 'DOCUMENT', f'artifact_class must be DOCUMENT: got {artifact_cls!r}'
    assert destination == 'CLOUD_HUB', f'destination must be CLOUD_HUB: got {destination!r}'
    assert payload_path == f'/archive/{FISCAL_NUMBER}/{sell_doc_id}/response.json', (
        f'payload_path mismatch: {payload_path!r}'
    )
    assert payload_sha256 == 'reconcile', (
        f'payload_sha256 for reconciliation outbox must be "reconcile": got {payload_sha256!r}'
    )
    assert status == 'PENDING', (
        f'outbox record must be PENDING (ready for delivery): got {status!r}'
    )
    assert sequence_no == doc.lnd, (
        f'sequence_no must match document lnd={doc.lnd}: got {sequence_no}'
    )

    # Verify the write-path outbox record does NOT exist (we deleted it above)
    with container2.connect() as conn:
        wp_row = conn.execute(
            "SELECT outbox_id FROM sync_outbox WHERE outbox_id=?",
            (f'{sell_doc_id}-outbox',),
        ).fetchone()
    assert wp_row is None, 'Write-path outbox record must not exist (deleted in crash setup)'
