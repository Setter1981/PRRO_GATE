"""
Gate 2e — protocol/transport trace hygiene smoke tests for reconciliation path.

Documents and verifies the transport_trace_log contract for reconciliation vs write-path.

Write-path trace records created per document:
  '{doc_id}-transport-out' direction=OUT  (outbound send request)
  '{doc_id}-transport-in'  direction=IN   (inbound send response)

Reconciliation trace record (Gate 2g fix):
  '{doc_id}-reconcile-poll' direction=EVENT, technical_status='RECONCILE_POLL'
  Written inside the outcome transaction (ACK/REJECTED/RETRYABLE branch).
  Semantically distinct from send traces: EVENT direction vs OUT/IN.

Key hygiene properties:
  - Reconciliation poll is traceable (EVENT record created)
  - Poll trace does NOT duplicate or overwrite the send OUT/IN traces
  - direction='EVENT' clearly distinguishes poll from send activity
  - protocol_trace_log is still not written by reconciliation (write-path only)

Invariant #8: no silent state violations — traceability must be explicit.
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
RECEIPT_ID = 'gate2e-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2e'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2e-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2E-001',
            'updated_at': '2026-03-30T16:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2E-001',
            'updated_at': '2026-03-30T16:01:00+00:00',
        })
    raise AssertionError(f'gate2e phase1: unexpected {request.method} {request.url}')


def _phase2_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2e-p2'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2E-001',
            'updated_at': '2026-03-30T16:01:00+00:00',
        })
    raise AssertionError(f'gate2e phase2: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate2e.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate2e',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2E-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T16:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2e',
        'business_ts': ts,
    }


def test_gate2e_reconciliation_adds_event_trace_distinct_from_send_traces(tmp_path: Path) -> None:
    """
    After write-path sends a document, transport_trace_log has two records per doc:
      - '{doc_id}-transport-out' direction=OUT  (outbound send)
      - '{doc_id}-transport-in'  direction=IN   (inbound send response)

    After reconciliation transitions SENT → ACK via poll_status():
      - transport_trace_log gains exactly ONE new record: '{doc_id}-reconcile-poll'
      - direction='EVENT', technical_status='RECONCILE_POLL'
      - This is semantically distinct from the send OUT/IN pair
      - The original send traces are NOT modified or duplicated

    Hygiene: reconciliation poll is traceable but clearly distinct from a normal send.
    """
    cfg = _config(tmp_path)

    # Phase 1: write-path creates transport traces
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2e-open'), 'business_ts': '2026-03-30T16:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2e', 'cashier_id': 'cashier-gate2e'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2e-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2e-001',
                'cashier_id': 'cashier-gate2e',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Capture transport trace count after write-path
    with container1.connect() as conn:
        trace_count_after_phase1 = conn.execute(
            "SELECT COUNT(*) FROM transport_trace_log WHERE fiscal_number=?",
            (FISCAL_NUMBER,),
        ).fetchone()[0]

        # Verify write-path created the expected OUT/IN trace pair for SELL
        sell_out = conn.execute(
            "SELECT trace_id, direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-out',),
        ).fetchone()
        sell_in = conn.execute(
            "SELECT trace_id, direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-in',),
        ).fetchone()

    assert sell_out is not None, (
        f'Write-path must create transport-out trace for SELL: not found. '
        f'(trace_id={sell_doc_id}-transport-out)'
    )
    assert sell_out[1] == 'OUT', f'transport-out must have direction=OUT: got {sell_out[1]!r}'
    assert sell_in is not None, (
        f'Write-path must create transport-in trace for SELL: not found. '
        f'(trace_id={sell_doc_id}-transport-in)'
    )
    assert sell_in[1] == 'IN', f'transport-in must have direction=IN: got {sell_in[1]!r}'

    # Crash simulation: reset SELL to SENT
    with container1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    # Phase 2: reconciliation runs
    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_mock))
    )
    with TestClient(create_app(container2)) as _:
        pass

    assert container2.last_startup_report.reconciliation_acked == 1

    with container2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.ACK

        trace_count_after_phase2 = conn.execute(
            "SELECT COUNT(*) FROM transport_trace_log WHERE fiscal_number=?",
            (FISCAL_NUMBER,),
        ).fetchone()[0]

        # Verify no new transport trace records were added by reconciliation
        recon_traces = conn.execute(
            "SELECT trace_id FROM transport_trace_log "
            "WHERE trace_id LIKE ?",
            (f'{sell_doc_id}-reconcile-%',),
        ).fetchall()

    # Reconciliation adds exactly ONE new transport trace record
    assert trace_count_after_phase2 == trace_count_after_phase1 + 1, (
        f'Reconciliation must add exactly 1 transport_trace_log EVENT record: '
        f'before={trace_count_after_phase1}, after={trace_count_after_phase2}'
    )

    # The new record is the reconcile-poll EVENT trace
    assert len(recon_traces) == 1, (
        f'Exactly 1 reconcile-poll trace must exist: found {recon_traces}'
    )
    assert recon_traces[0][0] == f'{sell_doc_id}-reconcile-poll'

    with container2.connect() as conn:
        poll_trace = conn.execute(
            "SELECT direction, technical_status, business_status "
            "FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll',),
        ).fetchone()

    assert poll_trace is not None
    direction, technical_status, business_status = poll_trace
    assert direction == 'EVENT', (
        f'Reconcile poll trace must use direction=EVENT (not OUT/IN): got {direction!r}'
    )
    assert technical_status == 'RECONCILE_POLL', (
        f'technical_status must be RECONCILE_POLL to distinguish from send: got {technical_status!r}'
    )

    # Original send traces must be unchanged
    with container2.connect() as conn:
        out_row = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-out',),
        ).fetchone()
        in_row = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-in',),
        ).fetchone()
    assert out_row is not None and out_row[0] == 'OUT', 'Send OUT trace must be unchanged'
    assert in_row is not None and in_row[0] == 'IN', 'Send IN trace must be unchanged'
