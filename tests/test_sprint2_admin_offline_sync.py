"""
Sprint 2 / bounded step 3 — POST /v1/admin/offline-sync endpoint tests.

Verifies the manual runtime trigger for OfflineSyncService.sync_pending().

Coverage matrix:
  E1 — happy path: OFFLINE_LOCAL_ACK doc synced to ACK, response includes all counters
  E2 — fiscal_number required: 400 if missing or empty
  E3 — empty DB: 200 with checked=0
  E4 — service not initialized: 503
  E5 — scope isolation: only requested fiscal_number processed
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.repositories.fiscal_documents import FiscalDocumentRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# HTTP mocks
# ---------------------------------------------------------------------------

def _ack_mock(request: httpx.Request) -> httpx.Response:
    """Mock that returns ACK for any Checkbox REST call."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-offline-sync'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'os-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-OS-001',
            'updated_at': '2026-04-11T10:00:00+00:00',
        })
    # All receipts → ACK
    return httpx.Response(200, json={
        'id': 'os-receipt-001', 'status': 'DONE',
        'fiscal_code': 'RCPT-OS-001',
        'updated_at': '2026-04-11T10:01:00+00:00',
    })


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

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
            'channel_owner': 'offline-sync-test',
        },
        'runtime': {
            'process_immediately': True,
            'max_recovery_attempts': 5,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'OS-LIC',
            'cashier_pin': '0000',
        },
    })


import itertools
_lnd_counter = itertools.count(90001)


def _next_lnd() -> int:
    return next(_lnd_counter)


def _seed_offline_doc(
    conn,
    *,
    fiscal_number: str = FISCAL_NUMBER,
    offline_fiscal_no: int,
    doc_type: str = 'SELL',
) -> str:
    """Seed an OFFLINE_LOCAL_ACK document directly in the DB."""
    import uuid
    doc_id = str(uuid.uuid4())
    req_id = str(uuid.uuid4())
    lnd = _next_lnd()

    conn.execute(
        """INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256)
        VALUES (?, ?, 'CHECKBOX_REST', ?, ?, '{}', ?)""",
        (req_id, f'{doc_type.lower()}:{fiscal_number.lower()}:{req_id}',
         doc_type, fiscal_number, 'sha-' + req_id[:8]),
    )
    FiscalDocumentRepository.create_prepared(
        conn,
        document_id=doc_id,
        request_id=req_id,
        fiscal_number=fiscal_number,
        lnd=lnd,
        doc_type=doc_type,
        backend_profile_id=BACKEND_PROFILE,
        transport_profile_id=TRANSPORT_PROFILE,
        fs_mode='OFFLINE',
        business_ts='2026-04-11T10:00:00Z',
        payload_json='{}',
        payload_sha256='sha-' + doc_id[:8],
        offline_fiscal_no=offline_fiscal_no,
    )
    FiscalDocumentRepository.update_state(
        conn,
        document_id=doc_id,
        state=DocumentState.OFFLINE_LOCAL_ACK,
        submission_status='OFFLINE_LOCAL',
    )
    conn.commit()
    return doc_id


# ---------------------------------------------------------------------------
# E1 — happy path: sync one doc to ACK
# ---------------------------------------------------------------------------

def test_e1_happy_path_sync_to_ack(tmp_path: Path) -> None:
    """POST /v1/admin/offline-sync with valid fiscal_number syncs OFFLINE_LOCAL_ACK → ACK."""
    cfg = _config(tmp_path, 'e1.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        # Seed after initialize (lifespan runs initialize)
        with c.connect() as conn:
            doc_id = _seed_offline_doc(conn, offline_fiscal_no=1001)

        resp = client.post('/v1/admin/offline-sync', json={'fiscal_number': FISCAL_NUMBER})

    assert resp.status_code == 200, f'Expected 200, got {resp.status_code}: {resp.text}'
    body = resp.json()
    assert body['fiscal_number'] == FISCAL_NUMBER
    assert body['checked'] == 1
    assert body['synced'] == 1
    assert body['pending_async'] == 0
    assert body['rejected'] == 0
    assert body['retryable'] == 0
    assert body['manual'] == 0

    # Verify DB state
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.ACK


# ---------------------------------------------------------------------------
# E2 — fiscal_number required
# ---------------------------------------------------------------------------

def test_e2_fiscal_number_required(tmp_path: Path) -> None:
    """POST /v1/admin/offline-sync returns 400 if fiscal_number is missing or empty."""
    cfg = _config(tmp_path, 'e2.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        # No body
        resp_none = client.post('/v1/admin/offline-sync', json={})
        assert resp_none.status_code == 400
        assert 'fiscal_number' in resp_none.json()['detail']

        # Empty string
        resp_empty = client.post('/v1/admin/offline-sync', json={'fiscal_number': ''})
        assert resp_empty.status_code == 400

        # Wrong type
        resp_int = client.post('/v1/admin/offline-sync', json={'fiscal_number': 123})
        assert resp_int.status_code == 400

        # Non-object JSON body
        resp_list = client.post('/v1/admin/offline-sync', json=[1, 2, 3])
        assert resp_list.status_code == 400
        assert 'object' in resp_list.json()['detail']

        resp_str = client.post('/v1/admin/offline-sync', json='hello')
        assert resp_str.status_code == 400


# ---------------------------------------------------------------------------
# E3 — empty DB: 200 with checked=0
# ---------------------------------------------------------------------------

def test_e3_empty_db_returns_zero(tmp_path: Path) -> None:
    """POST /v1/admin/offline-sync on empty DB returns 200 with checked=0."""
    cfg = _config(tmp_path, 'e3.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        resp = client.post('/v1/admin/offline-sync', json={'fiscal_number': FISCAL_NUMBER})

    assert resp.status_code == 200
    body = resp.json()
    assert body['checked'] == 0
    assert body['synced'] == 0


# ---------------------------------------------------------------------------
# E4 — service not initialized: 503
# ---------------------------------------------------------------------------

def test_e4_service_not_initialized(tmp_path: Path) -> None:
    """POST /v1/admin/offline-sync returns 503 if offline_sync_service is None."""
    cfg = _config(tmp_path, 'e4.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))
    with TestClient(create_app(c)) as client:
        # Simulate uninitialized state after lifespan already ran
        saved = c.offline_sync_service
        c.offline_sync_service = None
        try:
            resp = client.post('/v1/admin/offline-sync', json={'fiscal_number': FISCAL_NUMBER})
        finally:
            c.offline_sync_service = saved
    assert resp.status_code == 503
    assert 'not initialized' in resp.json()['detail']


# ---------------------------------------------------------------------------
# E5 — scope isolation: only requested FN processed
# ---------------------------------------------------------------------------

def test_e5_scope_isolation(tmp_path: Path) -> None:
    """POST /v1/admin/offline-sync only processes docs for the requested fiscal_number."""
    cfg = _config(tmp_path, 'e5.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        with c.connect() as conn:
            doc_fn1 = _seed_offline_doc(conn, fiscal_number=FISCAL_NUMBER, offline_fiscal_no=2001)
            doc_fn2 = _seed_offline_doc(conn, fiscal_number='FN-OTHER-9999', offline_fiscal_no=2002)

        resp = client.post('/v1/admin/offline-sync', json={'fiscal_number': FISCAL_NUMBER})

    assert resp.status_code == 200
    body = resp.json()
    assert body['checked'] == 1, f'Only FN1 doc should be checked, got {body}'
    assert body['synced'] == 1

    # FN2 doc untouched
    with c.connect() as conn:
        doc2 = FiscalDocumentRepository.get_by_id(conn, doc_fn2)
    assert doc2.state == DocumentState.OFFLINE_LOCAL_ACK, (
        f'FN2 doc must remain OFFLINE_LOCAL_ACK, got {doc2.state}'
    )
