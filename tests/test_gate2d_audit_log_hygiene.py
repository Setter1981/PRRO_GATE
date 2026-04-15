"""
Gate 2d — audit log hygiene smoke tests for reconciliation outcomes.

Verifies that reconciliation correctly emits audit_log records for ACK and
REJECTED outcomes, with the expected fields for operational investigation.

Reconciliation audit events:
  ACK:      entity_type='DOCUMENT', event_type='DOCUMENT_RECONCILED_ACK',     severity='INFO'
  REJECTED: entity_type='DOCUMENT', event_type='DOCUMENT_RECONCILED_REJECTED', severity='WARNING'

The event_payload_json carries {'state': 'ACK'} or {'state': 'REJECTED'} respectively.

REJECTED reconciliation trigger:
  GET /receipts/{receipt_id} with response {'status': 'ERROR'} →
  _map_checkbox_state() → DocumentState.REJECTED → reconciliation REJECTED branch.

Invariant #8: reconciliation must not silently violate state transitions.
Audit log is the only persistence trace that reconciliation produces (no transport traces).
"""
from __future__ import annotations

import json
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
RECEIPT_ID = 'gate2d-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2d'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2d-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2D-001',
            'updated_at': '2026-03-30T15:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2D-001',
            'updated_at': '2026-03-30T15:01:00+00:00',
        })
    raise AssertionError(f'gate2d phase1: unexpected {request.method} {request.url}')


def _phase2_ack_mock(request: httpx.Request) -> httpx.Response:
    """Poll returns DONE (ACK outcome)."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2d-ack'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2D-001',
            'updated_at': '2026-03-30T15:01:00+00:00',
        })
    raise AssertionError(f'gate2d phase2 ack: unexpected {request.method} {request.url}')


def _phase2_rejected_mock(request: httpx.Request) -> httpx.Response:
    """Poll returns ERROR (REJECTED outcome via _map_checkbox_state)."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2d-rej'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        # status='ERROR' → _map_checkbox_state → DocumentState.REJECTED
        return httpx.Response(200, json={'id': RECEIPT_ID, 'status': 'ERROR'})
    raise AssertionError(f'gate2d phase2 rejected: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate2d.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate2d',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2D-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T15:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2d',
        'business_ts': ts,
    }


def _setup_sent_doc(tmp_path: Path, db_name: str, mock) -> tuple[RuntimeContainer, str]:
    """Common setup: SHIFT_OPEN + SELL → ACK, then crash-simulate SELL to SENT."""
    cfg = _config(tmp_path, db_name)
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2d-open-' + db_name), 'business_ts': '2026-03-30T15:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-' + db_name, 'cashier_id': 'cashier-gate2d'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2d-sell-' + db_name),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-' + db_name,
                'cashier_id': 'cashier-gate2d',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    with container1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    return container1, sell_doc_id


def _get_audit_events(conn, document_id: str, event_type: str) -> list[dict]:
    rows = conn.execute(
        "SELECT entity_type, entity_id, event_type, severity, event_payload_json "
        "FROM audit_log WHERE entity_id=? AND event_type=? ORDER BY audit_id",
        (document_id, event_type),
    ).fetchall()
    return [
        {'entity_type': r[0], 'entity_id': r[1], 'event_type': r[2],
         'severity': r[3], 'payload': json.loads(r[4]) if r[4] else None}
        for r in rows
    ]


# ---------------------------------------------------------------------------
# Test A — reconciliation ACK creates DOCUMENT_RECONCILED_ACK audit record
# ---------------------------------------------------------------------------

def test_gate2d_reconciliation_ack_creates_audit_record(tmp_path: Path) -> None:
    """
    After reconciliation transitions SENT → ACK:
    - audit_log must contain DOCUMENT_RECONCILED_ACK (severity=INFO)
    - entity_type='DOCUMENT', entity_id=document_id
    - event_payload_json={'state': 'ACK'}
    - Exactly one such record (not duplicated)
    """
    container1, sell_doc_id = _setup_sent_doc(tmp_path, 'gate2d_a.sqlite3', _phase1_mock)
    cfg = _config(tmp_path, 'gate2d_a.sqlite3')

    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_ack_mock))
    )
    with TestClient(create_app(container2)) as _:
        pass

    assert container2.last_startup_report.reconciliation_acked == 1

    with container2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.ACK

        events = _get_audit_events(conn, sell_doc_id, 'DOCUMENT_RECONCILED_ACK')

    assert len(events) == 1, (
        f'Expected exactly 1 DOCUMENT_RECONCILED_ACK audit record: got {len(events)}. '
        f'Records: {events}'
    )
    ev = events[0]
    assert ev['entity_type'] == 'DOCUMENT', f'entity_type: {ev["entity_type"]!r}'
    assert ev['entity_id'] == sell_doc_id, f'entity_id: {ev["entity_id"]!r}'
    assert ev['severity'] == 'INFO', (
        f'DOCUMENT_RECONCILED_ACK must be INFO severity: got {ev["severity"]!r}'
    )
    assert ev['payload'] == {'state': 'ACK'}, (
        f'event_payload must be {{state: ACK}}: got {ev["payload"]}'
    )


# ---------------------------------------------------------------------------
# Test B — reconciliation REJECTED creates DOCUMENT_RECONCILED_REJECTED audit record
# ---------------------------------------------------------------------------

def test_gate2d_reconciliation_rejected_creates_audit_record(tmp_path: Path) -> None:
    """
    After reconciliation transitions SENT → REJECTED (poll returns status='ERROR'):
    - audit_log must contain DOCUMENT_RECONCILED_REJECTED (severity=WARNING)
    - entity_type='DOCUMENT', entity_id=document_id
    - event_payload_json={'state': 'REJECTED'}
    - Exactly one such record

    Note: REJECTED is terminal — doc ends in REJECTED state, not ERROR_RETRYABLE.
    The audit record uses severity=WARNING (not ERROR) per current reconciliation model.
    """
    container1, sell_doc_id = _setup_sent_doc(tmp_path, 'gate2d_b.sqlite3', _phase1_mock)
    cfg = _config(tmp_path, 'gate2d_b.sqlite3')

    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_rejected_mock))
    )
    with TestClient(create_app(container2)) as _:
        pass

    assert container2.last_startup_report.reconciliation_rejected == 1

    with container2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.REJECTED, (
            f'Doc must be REJECTED after poll returns ERROR: got {doc.state}'
        )

        events = _get_audit_events(conn, sell_doc_id, 'DOCUMENT_RECONCILED_REJECTED')

    assert len(events) == 1, (
        f'Expected exactly 1 DOCUMENT_RECONCILED_REJECTED audit record: got {len(events)}. '
        f'Records: {events}'
    )
    ev = events[0]
    assert ev['entity_type'] == 'DOCUMENT'
    assert ev['entity_id'] == sell_doc_id
    assert ev['severity'] == 'WARNING', (
        f'DOCUMENT_RECONCILED_REJECTED must be WARNING severity: got {ev["severity"]!r}'
    )
    assert ev['payload'] == {'state': 'REJECTED'}, (
        f'event_payload must be {{state: REJECTED}}: got {ev["payload"]}'
    )
