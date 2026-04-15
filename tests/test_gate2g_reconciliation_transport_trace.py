"""
Gate 2g — reconciliation transport traceability smoke tests.

Verifies that ReconciliationService.reconcile_pending() now writes a
transport_trace_log record for each poll_status() call that produces a
terminal or retryable outcome (ACK, REJECTED, RETRYABLE).

Trace record format:
  trace_id          = '{document_id}-reconcile-poll'          (ACK / REJECTED)
  trace_id          = '{document_id}-reconcile-poll-{n}'      (RETRYABLE, n = new recovery_attempts)
  direction         = 'EVENT'                                  (not OUT or IN — poll is not a send)
  technical_status  = 'RECONCILE_POLL'                        (distinguishes from write-path send)
  business_status   = poll.submission_status                  (e.g. 'DONE', 'ERROR')
  response_envelope = poll.response_json                      (raw response captured)

Implementation properties:
  - Written inside the outcome transaction (after update_state, before commit)
  - If update_state returns None (state machine mismatch), transaction rolls back
    and the trace is not committed (no orphaned trace records)
  - 'EVENT' direction is used (existing DDL value) — no schema migration needed
  - Send-path traces (OUT/IN) are unaffected

Three tests:
  A — ACK outcome: '{doc_id}-reconcile-poll' record created, direction=EVENT
  B — REJECTED outcome: '{doc_id}-reconcile-poll' record created, severity semantics
  C — Send-path trace contract unchanged: write-path OUT/IN records still present
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
RECEIPT_ID = 'gate2g-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2g'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2g-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2G-001',
            'updated_at': '2026-03-30T18:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2G-001',
            'updated_at': '2026-03-30T18:01:00+00:00',
        })
    raise AssertionError(f'gate2g phase1: unexpected {request.method} {request.url}')


def _phase2_ack_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2g-ack'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2G-001',
            'updated_at': '2026-03-30T18:01:00+00:00',
        })
    raise AssertionError(f'gate2g phase2 ack: unexpected {request.method} {request.url}')


def _phase2_rejected_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2g-rej'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={'id': RECEIPT_ID, 'status': 'ERROR'})
    raise AssertionError(f'gate2g phase2 rejected: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate2g.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate2g',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2G-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T18:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2g',
        'business_ts': ts,
    }


def _setup_sent(tmp_path: Path, db_name: str, phase1_mock) -> tuple[RuntimeContainer, str]:
    cfg = _config(tmp_path, db_name)
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2g-open-' + db_name), 'business_ts': '2026-03-30T18:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-' + db_name, 'cashier_id': 'cashier-gate2g'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2g-sell-' + db_name),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-' + db_name,
                'cashier_id': 'cashier-gate2g',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    with c1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()
    return c1, sell_doc_id


# ---------------------------------------------------------------------------
# Test A — ACK outcome: reconcile-poll EVENT trace created
# ---------------------------------------------------------------------------

def test_gate2g_reconciliation_ack_creates_event_trace(tmp_path: Path) -> None:
    """
    When reconciliation transitions SENT → ACK:
    - transport_trace_log gains '{doc_id}-reconcile-poll' with direction='EVENT'
    - technical_status='RECONCILE_POLL' distinguishes it from send traces
    - response_envelope contains the poll response JSON
    - The original send OUT/IN traces are not modified
    """
    c1, sell_doc_id = _setup_sent(tmp_path, 'gate2g_a.sqlite3', _phase1_mock)
    cfg = _config(tmp_path, 'gate2g_a.sqlite3')

    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_ack_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass

    assert c2.last_startup_report.reconciliation_acked == 1

    with c2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.ACK

        poll_row = conn.execute(
            "SELECT direction, technical_status, business_status, response_envelope "
            "FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll',),
        ).fetchone()

    assert poll_row is not None, (
        f'transport_trace_log must contain {{doc_id}}-reconcile-poll after reconciliation ACK. '
        f'trace_id={sell_doc_id}-reconcile-poll not found.'
    )
    direction, technical_status, business_status, response_envelope = poll_row

    assert direction == 'EVENT', (
        f'Reconcile poll trace must use direction=EVENT (distinguishes from send OUT/IN): '
        f'got {direction!r}'
    )
    assert technical_status == 'RECONCILE_POLL', (
        f'technical_status must be RECONCILE_POLL: got {technical_status!r}'
    )
    assert business_status == 'DONE', (
        f'business_status must reflect poll submission_status (DONE): got {business_status!r}'
    )
    assert response_envelope is not None, (
        'response_envelope must contain the poll response JSON for investigation'
    )

    # Send-path traces must still exist and be unchanged
    with c2.connect() as conn:
        out_trace = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-out',),
        ).fetchone()
        in_trace = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-in',),
        ).fetchone()

    assert out_trace is not None and out_trace[0] == 'OUT', (
        'Write-path transport-out trace must be unchanged'
    )
    assert in_trace is not None and in_trace[0] == 'IN', (
        'Write-path transport-in trace must be unchanged'
    )


# ---------------------------------------------------------------------------
# Test B — REJECTED outcome: reconcile-poll EVENT trace created
# ---------------------------------------------------------------------------

def test_gate2g_reconciliation_rejected_creates_event_trace(tmp_path: Path) -> None:
    """
    When reconciliation transitions SENT → REJECTED (poll returns status='ERROR'):
    - transport_trace_log gains '{doc_id}-reconcile-poll' with direction='EVENT'
    - business_status reflects the ERROR submission status
    - Doc ends in REJECTED state
    """
    c1, sell_doc_id = _setup_sent(tmp_path, 'gate2g_b.sqlite3', _phase1_mock)
    cfg = _config(tmp_path, 'gate2g_b.sqlite3')

    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_rejected_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass

    assert c2.last_startup_report.reconciliation_rejected == 1

    with c2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.REJECTED

        poll_row = conn.execute(
            "SELECT direction, technical_status, business_status "
            "FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll',),
        ).fetchone()

    assert poll_row is not None, (
        f'transport_trace_log must contain reconcile-poll trace for REJECTED outcome'
    )
    direction, technical_status, business_status = poll_row
    assert direction == 'EVENT'
    assert technical_status == 'RECONCILE_POLL'
    assert business_status == 'ERROR', (
        f'business_status for REJECTED poll must be ERROR: got {business_status!r}'
    )


# ---------------------------------------------------------------------------
# Test C — send-path trace contract: write-path OUT/IN unaffected by fix
# ---------------------------------------------------------------------------

def test_gate2g_write_path_send_traces_unaffected(tmp_path: Path) -> None:
    """
    The Gate 2g fix must not change the write-path send trace contract.
    After a direct write-path ACK (no reconciliation):
    - '{doc_id}-transport-out' direction=OUT must exist
    - '{doc_id}-transport-in'  direction=IN must exist
    - NO '{doc_id}-reconcile-poll' trace exists (reconciliation never ran)
    """
    cfg = _config(tmp_path, 'gate2g_c.sqlite3')
    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2g-open-c'), 'business_ts': '2026-03-30T18:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2g-c', 'cashier_id': 'cashier-gate2g'},
        })
        assert r_shift.status_code == 200

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2g-sell-c'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2g-c',
                'cashier_id': 'cashier-gate2g',
                'goods': [{'name': 'Tea', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    with c.connect() as conn:
        out_trace = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-out',),
        ).fetchone()
        in_trace = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-transport-in',),
        ).fetchone()
        poll_trace = conn.execute(
            "SELECT trace_id FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll',),
        ).fetchone()

    assert out_trace is not None and out_trace[0] == 'OUT', (
        'Write-path must create transport-out (direction=OUT) trace for SELL'
    )
    assert in_trace is not None and in_trace[0] == 'IN', (
        'Write-path must create transport-in (direction=IN) trace for SELL'
    )
    assert poll_trace is None, (
        'No reconcile-poll trace must exist when reconciliation never ran for this doc'
    )
