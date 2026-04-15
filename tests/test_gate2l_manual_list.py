"""
Gate 2l — GET /v1/admin/documents/manual list endpoint.

Returns all documents currently in REQUIRES_MANUAL_RECONCILIATION state.

Response format:
  {
    "count": N,
    "documents": [
      {
        "document_id": str,
        "fiscal_number": str,
        "doc_type": str,
        "state": "REQUIRES_MANUAL_RECONCILIATION",
        "recovery_attempts": int,
        "submission_status": str | null,
        "canonical_error_code": str | null,
        "error_message": str | null,
        "sent_at": str | null,
        "updated_at": str | null,
        "created_at": str | null,
      }
    ]
  }

Three tests:
  A — empty list: 200 with count=0 and empty documents array
  B — manual docs visible: ceiling-hit doc appears in list with correct fields
  C — other states excluded: ACK and ERROR_RETRYABLE docs do not appear
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
RECEIPT_ID = 'gate2l-receipt-001'
MAX_RECOVERY = 2


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2l'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2l-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2L-001',
            'updated_at': '2026-03-30T23:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2L-001',
            'updated_at': '2026-03-30T23:01:00+00:00',
        })
    raise AssertionError(f'gate2l phase1: unexpected {request.method} {request.url}')


def _strict_no_http_mock(request: httpx.Request) -> httpx.Response:
    raise AssertionError(f'gate2l: unexpected HTTP call {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str, max_recovery_attempts: int = MAX_RECOVERY) -> AppConfig:
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
            'channel_owner': 'gate2l',
        },
        'runtime': {
            'process_immediately': True,
            'max_recovery_attempts': max_recovery_attempts,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2L-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T23:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2l',
        'business_ts': ts,
    }


def _reach_ceiling(client, db_name: str, cfg: AppConfig) -> str:
    """Send SELL, crash-sim to SENT/no transport_id, run ceiling-hit passes. Returns doc_id."""
    r_sell = client.post('/v1/ingress/checkbox', json={
        'context': _ctx('gate2l-sell-' + db_name),
        'operation': 'SELL',
        'request': {
            'external_request_id': 'ext-sell-' + db_name,
            'cashier_id': 'cashier-gate2l',
            'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 2000}],
        },
    })
    assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
    return r_sell.json()['document_id']


# ---------------------------------------------------------------------------
# Test A — empty list when no manual-state documents exist
# ---------------------------------------------------------------------------

def test_gate2l_empty_list(tmp_path: Path) -> None:
    """GET /v1/admin/documents/manual returns 200 with count=0 when no manual docs."""
    cfg = _config(tmp_path, 'gate2l_a.sqlite3')
    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/admin/documents/manual')

    assert resp.status_code == 200
    body = resp.json()
    assert body['count'] == 0
    assert body['documents'] == []


# ---------------------------------------------------------------------------
# Test B — manual docs appear with correct fields
# ---------------------------------------------------------------------------

def test_gate2l_manual_doc_appears_in_list(tmp_path: Path) -> None:
    """
    After a doc hits the recovery ceiling:
    - GET /v1/admin/documents/manual returns count=1
    - The document entry contains: document_id, fiscal_number, state, recovery_attempts,
      doc_type, and other available fields
    - state == 'REQUIRES_MANUAL_RECONCILIATION'
    - recovery_attempts == MAX_RECOVERY
    """
    db_name = 'gate2l_b.sqlite3'
    cfg = _config(tmp_path, db_name)

    # Phase 1: SHIFT_OPEN + SELL → ACK
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2l-open-b'), 'business_ts': '2026-03-30T23:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2l-b', 'cashier_id': 'cashier-gate2l'},
        })
        assert r_shift.status_code == 200
        sell_doc_id = _reach_ceiling(client, db_name, cfg)

    # Crash-sim → SENT + no transport_request_id
    with c1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents "
            "SET state='SENT', ack_at=NULL, response_json=NULL, transport_request_id=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    # Run MAX_RECOVERY passes to hit ceiling
    for _ in range(MAX_RECOVERY):
        c = RuntimeContainer(
            cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
        )
        with TestClient(create_app(c)) as _:
            pass

    # Verify list endpoint
    c_check = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c_check)) as client:
        resp = client.get('/v1/admin/documents/manual')

    assert resp.status_code == 200
    body = resp.json()
    assert body['count'] == 1, f'Expected 1 manual doc: got {body["count"]}'

    entry = body['documents'][0]
    assert entry['document_id'] == sell_doc_id
    assert entry['fiscal_number'] == FISCAL_NUMBER
    assert entry['state'] == 'REQUIRES_MANUAL_RECONCILIATION'
    assert entry['recovery_attempts'] == MAX_RECOVERY, (
        f'recovery_attempts must be {MAX_RECOVERY}: got {entry["recovery_attempts"]}'
    )
    assert entry['doc_type'] == 'SELL', f'doc_type must be SELL: got {entry["doc_type"]!r}'
    assert 'created_at' in entry
    assert 'updated_at' in entry
    assert 'submission_status' in entry


# ---------------------------------------------------------------------------
# Test C — other states not included in list
# ---------------------------------------------------------------------------

def test_gate2l_other_states_excluded(tmp_path: Path) -> None:
    """
    Documents in ACK, ERROR_RETRYABLE, SENT do not appear in /v1/admin/documents/manual.
    Only REQUIRES_MANUAL_RECONCILIATION docs appear.
    """
    db_name = 'gate2l_c.sqlite3'
    cfg = _config(tmp_path, db_name, max_recovery_attempts=10)  # high ceiling to keep doc in ERROR_RETRYABLE

    # Phase 1: SHIFT_OPEN + two SELLs
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2l-open-c'), 'business_ts': '2026-03-30T23:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2l-c', 'cashier_id': 'cashier-gate2l'},
        })
        assert r_shift.status_code == 200

        # SELL 1 → stays ACK (normal write-path)
        r1 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2l-sell-c1'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2l-c1',
                'cashier_id': 'cashier-gate2l',
                'goods': [{'name': 'Tea', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r1.status_code == 200 and r1.json().get('document_state') == 'ACK'
        ack_doc_id = r1.json()['document_id']

        # SELL 2 → crash-sim to SENT + no transport_id → will become ERROR_RETRYABLE
        r2 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2l-sell-c2'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2l-c2',
                'cashier_id': 'cashier-gate2l',
                'goods': [{'name': 'Juice', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })
        assert r2.status_code == 200 and r2.json().get('document_state') == 'ACK'
        retry_doc_id = r2.json()['document_id']

    # Crash-sim: second SELL → ERROR_RETRYABLE
    with c1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents "
            "SET state='SENT', ack_at=NULL, response_json=NULL, transport_request_id=NULL "
            "WHERE document_id=?",
            (retry_doc_id,),
        )
        conn.commit()

    # One reconciliation pass → retry_doc → ERROR_RETRYABLE (ceiling=10, won't hit)
    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass
    assert c2.last_startup_report.reconciliation_retryable == 1

    # Check manual list: must be empty (ack_doc=ACK, retry_doc=ERROR_RETRYABLE)
    c3 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c3)) as client:
        resp = client.get('/v1/admin/documents/manual')

    assert resp.status_code == 200
    body = resp.json()
    assert body['count'] == 0, (
        f'ACK and ERROR_RETRYABLE docs must not appear in manual list: '
        f'got count={body["count"]}, docs={[d["document_id"] for d in body["documents"]]}'
    )
    doc_ids_in_list = {d['document_id'] for d in body['documents']}
    assert ack_doc_id not in doc_ids_in_list, 'ACK doc must not appear in manual list'
    assert retry_doc_id not in doc_ids_in_list, 'ERROR_RETRYABLE doc must not appear in manual list'
