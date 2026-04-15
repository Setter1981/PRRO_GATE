"""
Gate 2n — ops summary outbox_pending_count.

GET /v1/ops/summary now includes:
  "outbox_pending_count": N

where N = COUNT(*) FROM sync_outbox WHERE status IN ('PENDING','SENDING').
DONE and DEAD records are excluded.

Three tests:
  A — zero: no outbox records → outbox_pending_count == 0
  B — nonzero: write-path ACK creates PENDING outbox record → count == 1
  C — terminal statuses (DONE, DEAD) not counted
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


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2n'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2n-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2N-001',
            'updated_at': '2026-03-30T23:50:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate2n-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2N-001',
            'updated_at': '2026-03-30T23:51:00+00:00',
        })
    raise AssertionError(f'gate2n phase1: unexpected {request.method} {request.url}')


def _strict_no_http_mock(request: httpx.Request) -> httpx.Response:
    raise AssertionError(f'gate2n: unexpected HTTP call {request.method} {request.url}')


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
            'channel_owner': 'gate2n',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2N-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T23:51:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2n',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — zero outbox records
# ---------------------------------------------------------------------------

def test_gate2n_ops_summary_zero_outbox_pending(tmp_path: Path) -> None:
    """ops summary returns outbox_pending_count=0 when sync_outbox is empty."""
    cfg = _config(tmp_path, 'gate2n_a.sqlite3')
    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert 'outbox_pending_count' in body, 'ops/summary must contain outbox_pending_count'
    assert body['outbox_pending_count'] == 0


# ---------------------------------------------------------------------------
# Test B — PENDING outbox record counted
# ---------------------------------------------------------------------------

def test_gate2n_ops_summary_counts_pending_outbox(tmp_path: Path) -> None:
    """
    After a write-path ACK, the reconcile-outbox record (PENDING) is counted.
    After injecting a PENDING record directly, count reflects it.
    Uses the reconciliation outbox path (crash-sim + reconcile) which creates
    a PENDING record that stays PENDING (no outbox worker running in tests).
    """
    db_name = 'gate2n_b.sqlite3'
    cfg = _config(tmp_path, db_name)

    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2n-open-b'), 'business_ts': '2026-03-30T23:50:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2n-b', 'cashier_id': 'cashier-gate2n'},
        })
        assert r_shift.status_code == 200

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2n-sell-b'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2n-b',
                'cashier_id': 'cashier-gate2n',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Write-path ACK creates an outbox record (status=PENDING, no worker dispatches it)
    with c1.connect() as conn:
        pending_count = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE status='PENDING'",
        ).fetchone()[0]
    assert pending_count >= 1, 'Write-path ACK must create at least one PENDING outbox record'

    # Check ops summary reflects the PENDING outbox record
    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['outbox_pending_count'] >= 1, (
        f'ops summary must count PENDING outbox records: got {body["outbox_pending_count"]}'
    )
    assert body['outbox_pending_count'] == pending_count, (
        f'outbox_pending_count must match actual PENDING count: '
        f'summary={body["outbox_pending_count"]}, actual={pending_count}'
    )


# ---------------------------------------------------------------------------
# Test C — DONE and DEAD not counted
# ---------------------------------------------------------------------------

def test_gate2n_ops_summary_terminal_outbox_not_counted(tmp_path: Path) -> None:
    """
    Records in DONE and DEAD status are not counted in outbox_pending_count.
    Only PENDING and SENDING are counted.
    """
    db_name = 'gate2n_c.sqlite3'
    cfg = _config(tmp_path, db_name)

    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c)) as client:
        # Inject DONE and DEAD records directly
        with c.connect() as conn:
            conn.execute(
                """INSERT INTO sync_outbox (
                    outbox_id, aggregate_type, aggregate_id, fiscal_number, artifact_class,
                    sequence_no, destination, payload_path, payload_sha256, status
                ) VALUES (?, 'DOCUMENT', ?, ?, 'DOCUMENT', 1, 'CLOUD_HUB', '/path/done', 'sha', 'DONE')""",
                ('gate2n-outbox-done', 'doc-done', FISCAL_NUMBER),
            )
            conn.execute(
                """INSERT INTO sync_outbox (
                    outbox_id, aggregate_type, aggregate_id, fiscal_number, artifact_class,
                    sequence_no, destination, payload_path, payload_sha256, status
                ) VALUES (?, 'DOCUMENT', ?, ?, 'DOCUMENT', 2, 'CLOUD_HUB', '/path/dead', 'sha', 'DEAD')""",
                ('gate2n-outbox-dead', 'doc-dead', FISCAL_NUMBER),
            )
            conn.commit()

        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['outbox_pending_count'] == 0, (
        f'DONE and DEAD outbox records must not be counted as pending: '
        f'got {body["outbox_pending_count"]}'
    )
