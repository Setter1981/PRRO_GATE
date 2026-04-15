"""
Gate 2m — ops summary manual_reconciliation_count.

GET /v1/ops/summary now includes:
  "manual_reconciliation_count": N

where N = count of fiscal_documents WHERE state = 'REQUIRES_MANUAL_RECONCILIATION'.

Three tests:
  A — zero: no manual docs → manual_reconciliation_count == 0
  B — nonzero: ceiling-hit doc → manual_reconciliation_count == 1
  C — isolation: ACK and ERROR_RETRYABLE docs do not affect the counter
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'
MAX_RECOVERY = 2


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2m'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2m-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2M-001',
            'updated_at': '2026-03-30T23:30:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate2m-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2M-001',
            'updated_at': '2026-03-30T23:31:00+00:00',
        })
    raise AssertionError(f'gate2m phase1: unexpected {request.method} {request.url}')


def _strict_no_http_mock(request: httpx.Request) -> httpx.Response:
    raise AssertionError(f'gate2m: unexpected HTTP call {request.method} {request.url}')


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
            'channel_owner': 'gate2m',
        },
        'runtime': {
            'process_immediately': True,
            'max_recovery_attempts': max_recovery_attempts,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2M-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T23:31:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2m',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — zero manual docs
# ---------------------------------------------------------------------------

def test_gate2m_ops_summary_zero_manual_count(tmp_path: Path) -> None:
    """ops summary returns manual_reconciliation_count=0 when no manual docs exist."""
    cfg = _config(tmp_path, 'gate2m_a.sqlite3')
    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert 'manual_reconciliation_count' in body, (
        'ops/summary must contain manual_reconciliation_count field'
    )
    assert body['manual_reconciliation_count'] == 0, (
        f'Expected 0 manual docs: got {body["manual_reconciliation_count"]}'
    )


# ---------------------------------------------------------------------------
# Test B — nonzero: ceiling-hit doc counted
# ---------------------------------------------------------------------------

def test_gate2m_ops_summary_counts_manual_docs(tmp_path: Path) -> None:
    """
    After a doc hits the recovery ceiling → REQUIRES_MANUAL_RECONCILIATION,
    ops summary manual_reconciliation_count == 1.
    """
    db_name = 'gate2m_b.sqlite3'
    cfg = _config(tmp_path, db_name)

    # Phase 1: SHIFT_OPEN + SELL → ACK
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2m-open-b'), 'business_ts': '2026-03-30T23:30:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2m-b', 'cashier_id': 'cashier-gate2m'},
        })
        assert r_shift.status_code == 200

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2m-sell-b'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2m-b',
                'cashier_id': 'cashier-gate2m',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Crash-sim: SENT + no transport_request_id
    with c1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents "
            "SET state='SENT', ack_at=NULL, response_json=NULL, transport_request_id=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    # Hit ceiling (MAX_RECOVERY passes)
    for _ in range(MAX_RECOVERY):
        c = RuntimeContainer(
            cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
        )
        with TestClient(create_app(c)) as _:
            pass
    assert c.last_startup_report.reconciliation_manual == 1

    # Check ops summary
    c_check = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c_check)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['manual_reconciliation_count'] == 1, (
        f'Expected 1 manual doc in ops summary: got {body["manual_reconciliation_count"]}'
    )


# ---------------------------------------------------------------------------
# Test C — ACK and ERROR_RETRYABLE do not affect counter
# ---------------------------------------------------------------------------

def test_gate2m_ops_summary_other_states_not_counted(tmp_path: Path) -> None:
    """
    Documents in ACK and ERROR_RETRYABLE states are not counted in manual_reconciliation_count.
    """
    db_name = 'gate2m_c.sqlite3'
    cfg = _config(tmp_path, db_name, max_recovery_attempts=10)

    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2m-open-c'), 'business_ts': '2026-03-30T23:30:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2m-c', 'cashier_id': 'cashier-gate2m'},
        })
        assert r_shift.status_code == 200

        # SELL → stays ACK
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2m-sell-c'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2m-c',
                'cashier_id': 'cashier-gate2m',
                'goods': [{'name': 'Tea', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Crash-sim → ERROR_RETRYABLE (ceiling=10, won't hit)
    with c1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents "
            "SET state='SENT', ack_at=NULL, response_json=NULL, transport_request_id=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass
    assert c2.last_startup_report.reconciliation_retryable == 1

    c3 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c3)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['manual_reconciliation_count'] == 0, (
        f'ACK and ERROR_RETRYABLE docs must not be counted as manual: '
        f'got {body["manual_reconciliation_count"]}'
    )
