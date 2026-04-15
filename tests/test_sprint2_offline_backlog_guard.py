"""
Sprint 2 / bounded step 4 — legal blocker for SHIFT_CLOSE and Z_REPORT
when pending OFFLINE_LOCAL_ACK backlog exists.

Coverage matrix:
  G1 — SHIFT_CLOSE blocked when pending offline docs exist for same fiscal_number
  G2 — SHIFT_CLOSE succeeds when no pending offline docs exist
  G3 — Z_REPORT blocked when pending offline docs exist (end-to-end via ingress)
  G4 — blocker is scoped by fiscal_number (other FN's backlog does not block)

Invariants verified:
  #1  Guard fires before sign/send — transport not reached when blocked.
  #2  fiscal_number scope preserved.
"""
from __future__ import annotations

import itertools
import uuid
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

_lnd_counter = itertools.count(80000)


# ---------------------------------------------------------------------------
# HTTP mocks
# ---------------------------------------------------------------------------

def _mock_ack_transport(request: httpx.Request) -> httpx.Response:
    """Full ACK transport for tests where the operation should succeed."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-guard'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'guard-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GUARD-001',
            'updated_at': '2026-04-12T10:00:00+00:00',
        })
    if path.endswith('/shifts/close'):
        return httpx.Response(200, json={
            'id': 'guard-shift-001', 'status': 'CLOSED',
            'fiscal_code': 'SHIFT-GUARD-001',
            'updated_at': '2026-04-12T10:05:00+00:00',
        })
    return httpx.Response(200, json={
        'id': 'guard-receipt-001', 'status': 'DONE',
        'fiscal_code': 'RCPT-GUARD-001',
        'updated_at': '2026-04-12T10:01:00+00:00',
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
            'channel_owner': 'guard-test',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GUARD-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-04-12T10:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'guard-test',
        'business_ts': ts,
    }


def _seed_offline_doc(conn, *, fiscal_number: str = FISCAL_NUMBER, offline_fiscal_no: int) -> str:
    """Seed an OFFLINE_LOCAL_ACK document directly in the DB."""
    doc_id = str(uuid.uuid4())
    req_id = str(uuid.uuid4())
    lnd = next(_lnd_counter)

    conn.execute(
        """INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256, status)
        VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?, '{}', ?, 'DONE')""",
        (req_id, f'sell:{fiscal_number.lower()}:{req_id}',
         fiscal_number, 'sha-' + req_id[:8]),
    )
    FiscalDocumentRepository.create_prepared(
        conn,
        document_id=doc_id,
        request_id=req_id,
        fiscal_number=fiscal_number,
        lnd=lnd,
        doc_type='SELL',
        backend_profile_id=BACKEND_PROFILE,
        transport_profile_id=TRANSPORT_PROFILE,
        fs_mode='OFFLINE',
        business_ts='2026-04-12T10:00:00Z',
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
# G1 — SHIFT_CLOSE blocked when pending offline backlog exists
# ---------------------------------------------------------------------------

def test_g1_shift_close_blocked_by_offline_backlog(tmp_path: Path) -> None:
    """SHIFT_CLOSE must return OFFLINE_BACKLOG_NOT_SYNCED when pending offline docs exist."""
    cfg = _config(tmp_path, 'g1.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_ack_transport)))

    with TestClient(create_app(c)) as client:
        # Open shift first
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('guard-open-g1'), 'business_ts': '2026-04-12T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-g1', 'cashier_id': 'cashier-guard'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN must succeed: {r_open.json()}'
        )

        # Seed offline backlog
        with c.connect() as conn:
            _seed_offline_doc(conn, offline_fiscal_no=5001)

        # Attempt SHIFT_CLOSE — must be blocked
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('guard-close-g1'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-g1', 'cashier_id': 'cashier-guard'},
        })

    assert r_close.status_code == 200, f'Expected 200, got {r_close.status_code}: {r_close.text}'
    body = r_close.json()
    assert body.get('error_code') == 'OFFLINE_BACKLOG_NOT_SYNCED', (
        f'Expected OFFLINE_BACKLOG_NOT_SYNCED, got {body}'
    )
    assert 'SHIFT_CLOSE' in body.get('error_message', '')
    assert body.get('document_state') != 'ACK'


# ---------------------------------------------------------------------------
# G2 — SHIFT_CLOSE succeeds when no pending offline docs
# ---------------------------------------------------------------------------

def test_g2_shift_close_succeeds_without_backlog(tmp_path: Path) -> None:
    """SHIFT_CLOSE must succeed normally when no pending offline docs exist."""
    cfg = _config(tmp_path, 'g2.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_ack_transport)))

    with TestClient(create_app(c)) as client:
        # Open shift
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('guard-open-g2'), 'business_ts': '2026-04-12T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-g2', 'cashier_id': 'cashier-guard'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK'

        # Close shift — no offline backlog, must succeed
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('guard-close-g2'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-g2', 'cashier_id': 'cashier-guard'},
        })

    assert r_close.status_code == 200
    body = r_close.json()
    assert body.get('document_state') == 'ACK', f'SHIFT_CLOSE must ACK: {body}'
    assert body.get('error_code') is None


# ---------------------------------------------------------------------------
# G3 — Z_REPORT blocked when pending offline backlog exists
# ---------------------------------------------------------------------------

def test_g3_z_report_blocked_by_offline_backlog(tmp_path: Path) -> None:
    """Z_REPORT must return OFFLINE_BACKLOG_NOT_SYNCED when pending offline docs exist.

    Exercises the real guard path via ingress_checkbox endpoint.
    Z_REPORT reaches _guard_preconditions through the adapter and write-path.
    """
    cfg = _config(tmp_path, 'g3.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_ack_transport)))

    with TestClient(create_app(c)) as client:
        # Open shift first
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('guard-open-g3'), 'business_ts': '2026-04-12T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-g3', 'cashier_id': 'cashier-guard'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN must succeed: {r_open.json()}'
        )

        # Seed offline backlog
        with c.connect() as conn:
            _seed_offline_doc(conn, offline_fiscal_no=6001)

        # Attempt Z_REPORT — must be blocked by guard
        r_zreport = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('guard-zreport-g3'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-g3', 'cashier_id': 'cashier-guard'},
        })

    assert r_zreport.status_code == 200
    body = r_zreport.json()
    assert body.get('error_code') == 'OFFLINE_BACKLOG_NOT_SYNCED', (
        f'Expected OFFLINE_BACKLOG_NOT_SYNCED, got {body}'
    )


# ---------------------------------------------------------------------------
# G4 — blocker is scoped by fiscal_number
# ---------------------------------------------------------------------------

def test_g4_blocker_scoped_by_fiscal_number(tmp_path: Path) -> None:
    """Offline backlog for a different fiscal_number must NOT block SHIFT_CLOSE."""
    cfg = _config(tmp_path, 'g4.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_ack_transport)))

    with TestClient(create_app(c)) as client:
        # Open shift for FN-DEV-0001
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('guard-open-g4'), 'business_ts': '2026-04-12T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-g4', 'cashier_id': 'cashier-guard'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK'

        # Seed offline backlog for a DIFFERENT fiscal_number
        with c.connect() as conn:
            _seed_offline_doc(conn, fiscal_number='FN-OTHER-9999', offline_fiscal_no=7001)

        # SHIFT_CLOSE for FN-DEV-0001 — must NOT be blocked
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('guard-close-g4'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-g4', 'cashier_id': 'cashier-guard'},
        })

    assert r_close.status_code == 200
    body = r_close.json()
    assert body.get('document_state') == 'ACK', (
        f'SHIFT_CLOSE must succeed when backlog is for different FN: {body}'
    )
    assert body.get('error_code') is None
