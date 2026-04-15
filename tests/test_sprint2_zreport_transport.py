"""
Sprint 2 / bounded step 6 — Checkbox ONLINE Z_REPORT transport tests.

Coverage matrix:
  Z1 — Z_REPORT happy path: SHIFT_OPEN → Z_REPORT → ACK, z_report_document_id persisted
  Z1b — Z_REPORT async: SENT → z_report_document_id set early for recovery
  Z1c — Z_REPORT REJECTED → retry ACK overwrites z_report_document_id
  Z2 — Z_REPORT still blocked by offline backlog (existing guard)
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

_lnd_counter = itertools.count(95000)


# ---------------------------------------------------------------------------
# HTTP mocks
# ---------------------------------------------------------------------------

def _zreport_ack_mock(request: httpx.Request) -> httpx.Response:
    """Mock Checkbox API that supports SHIFT_OPEN + Z_REPORT."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-zreport'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'zr-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-ZR-001',
            'updated_at': '2026-04-12T14:00:00+00:00',
        })
    if path.endswith('/reports') and request.method == 'POST':
        # Z_REPORT returns DONE immediately
        return httpx.Response(200, json={
            'id': 'zr-report-001', 'status': 'DONE',
            'fiscal_code': 'ZRPT-001',
            'updated_at': '2026-04-12T14:05:00+00:00',
        })
    return httpx.Response(200, json={
        'id': 'zr-receipt-001', 'status': 'DONE',
        'fiscal_code': 'RCPT-ZR-001',
        'updated_at': '2026-04-12T14:01:00+00:00',
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
            'channel_owner': 'zreport-test',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'ZR-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-04-12T14:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'zreport-test',
        'business_ts': ts,
    }


def _seed_offline_doc(conn, *, fiscal_number: str = FISCAL_NUMBER, offline_fiscal_no: int) -> str:
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
        conn, document_id=doc_id, request_id=req_id,
        fiscal_number=fiscal_number, lnd=lnd, doc_type='SELL',
        backend_profile_id=BACKEND_PROFILE, transport_profile_id=TRANSPORT_PROFILE,
        fs_mode='OFFLINE', business_ts='2026-04-12T14:00:00Z',
        payload_json='{}', payload_sha256='sha-' + doc_id[:8],
        offline_fiscal_no=offline_fiscal_no,
    )
    FiscalDocumentRepository.update_state(
        conn, document_id=doc_id,
        state=DocumentState.OFFLINE_LOCAL_ACK, submission_status='OFFLINE_LOCAL',
    )
    conn.commit()
    return doc_id


# ---------------------------------------------------------------------------
# Z1 — Z_REPORT happy path: ACK + z_report_document_id persisted
# ---------------------------------------------------------------------------

def test_z1_zreport_happy_path_ack(tmp_path: Path) -> None:
    """ONLINE Z_REPORT via Checkbox REST: SHIFT_OPEN → Z_REPORT → ACK.
    Shift must have z_report_document_id set."""
    cfg = _config(tmp_path, 'z1.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_zreport_ack_mock)))

    with TestClient(create_app(c)) as client:
        # Open shift
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('zr-open-z1'), 'business_ts': '2026-04-12T14:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-z1', 'cashier_id': 'cashier-zr'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN must ACK: {r_open.json()}'
        )

        # Z_REPORT
        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('zr-report-z1'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-z1', 'cashier_id': 'cashier-zr'},
        })

    assert r_zr.status_code == 200, f'Expected 200, got {r_zr.status_code}: {r_zr.text}'
    body = r_zr.json()
    assert body.get('document_state') == 'ACK', f'Z_REPORT must ACK: {body}'
    zr_doc_id = body['document_id']

    # Verify z_report_document_id on shift
    with c.connect() as conn:
        row = conn.execute(
            "SELECT z_report_document_id, state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()
    assert row is not None
    assert row[0] == zr_doc_id, f'z_report_document_id must be {zr_doc_id}, got {row[0]}'


# ---------------------------------------------------------------------------
# Z1b — Z_REPORT async: SENT, z_report_document_id set early
# ---------------------------------------------------------------------------

def test_z1b_zreport_async_sent_linkage(tmp_path: Path) -> None:
    """When DPS returns PROCESSING for Z_REPORT, document enters SENT and
    z_report_document_id must already be set on the shift for recovery safety."""

    def _async_zreport_mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-z1b'})
        if path.endswith('/shifts') and request.method == 'POST':
            return httpx.Response(200, json={
                'id': 'z1b-shift-001', 'status': 'OPENED',
                'fiscal_code': 'SHIFT-Z1B-001',
                'updated_at': '2026-04-12T14:00:00+00:00',
            })
        if path.endswith('/reports') and request.method == 'POST':
            # Z_REPORT accepted async
            return httpx.Response(200, json={
                'id': 'z1b-report-001', 'status': 'PROCESSING',
                'fiscal_code': None,
                'updated_at': '2026-04-12T14:05:00+00:00',
            })
        return httpx.Response(200, json={
            'id': 'z1b-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-Z1B-001',
            'updated_at': '2026-04-12T14:01:00+00:00',
        })

    cfg = _config(tmp_path, 'z1b.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_async_zreport_mock)))

    with TestClient(create_app(c)) as client:
        # Open shift
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('zr-open-z1b'), 'business_ts': '2026-04-12T14:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-z1b', 'cashier_id': 'cashier-zr'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK'

        # Z_REPORT — DPS returns PROCESSING → SENT
        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('zr-report-z1b'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-z1b', 'cashier_id': 'cashier-zr'},
        })

    assert r_zr.status_code == 200
    body = r_zr.json()
    zr_doc_id = body.get('document_id')
    assert zr_doc_id is not None
    assert body.get('document_state') in ('SENT', 'KVT1', 'KVT2'), (
        f'Async Z_REPORT must be SENT/KVT, got {body.get("document_state")}'
    )

    # z_report_document_id must be set even though doc is still SENT
    with c.connect() as conn:
        row = conn.execute(
            "SELECT z_report_document_id, state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()
    assert row is not None
    assert row[0] == zr_doc_id, (
        f'z_report_document_id must be set during SENT for recovery, got {row[0]}'
    )


# ---------------------------------------------------------------------------
# Z1c — REJECTED Z_REPORT does not block retry ACK linkage
# ---------------------------------------------------------------------------

def test_z1c_zreport_rejected_then_retry_ack_overwrites_linkage(tmp_path: Path) -> None:
    """A REJECTED Z_REPORT must NOT permanently occupy z_report_document_id.
    A subsequent successful Z_REPORT must overwrite the linkage."""
    call_count = 0

    def _reject_then_ack_mock(request: httpx.Request) -> httpx.Response:
        nonlocal call_count
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-z1c'})
        if path.endswith('/shifts') and request.method == 'POST':
            return httpx.Response(200, json={
                'id': 'z1c-shift-001', 'status': 'OPENED',
                'fiscal_code': 'SHIFT-Z1C-001',
                'updated_at': '2026-04-12T14:00:00+00:00',
            })
        if path.endswith('/reports') and request.method == 'POST':
            call_count += 1
            if call_count == 1:
                # First Z_REPORT → 422 rejected
                return httpx.Response(422, json={'message': 'fiscal data invalid'})
            # Second Z_REPORT → ACK
            return httpx.Response(200, json={
                'id': 'z1c-report-002', 'status': 'DONE',
                'fiscal_code': 'ZRPT-Z1C-002',
                'updated_at': '2026-04-12T14:10:00+00:00',
            })
        return httpx.Response(200, json={
            'id': 'z1c-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-Z1C-001',
            'updated_at': '2026-04-12T14:01:00+00:00',
        })

    cfg = _config(tmp_path, 'z1c.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_reject_then_ack_mock)))

    with TestClient(create_app(c)) as client:
        # Open shift
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('zr-open-z1c'), 'business_ts': '2026-04-12T14:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-z1c', 'cashier_id': 'cashier-zr'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK'

        # First Z_REPORT → REJECTED by transport (422)
        r_zr1 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('zr-report-z1c-1'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-z1c-1', 'cashier_id': 'cashier-zr'},
        })
        assert r_zr1.status_code == 200
        zr1_body = r_zr1.json()
        zr1_doc_id = zr1_body.get('document_id')
        assert zr1_body.get('document_state') == 'REJECTED', (
            f'First Z_REPORT must be REJECTED, got {zr1_body}'
        )

        # Verify z_report_document_id is NOT set for REJECTED
        with c.connect() as conn:
            row1 = conn.execute(
                "SELECT z_report_document_id FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
                (FISCAL_NUMBER,),
            ).fetchone()
        assert row1[0] is None, (
            f'REJECTED Z_REPORT must NOT set z_report_document_id, got {row1[0]}'
        )

        # Second Z_REPORT → ACK
        r_zr2 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('zr-report-z1c-2'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-z1c-2', 'cashier_id': 'cashier-zr'},
        })
        assert r_zr2.status_code == 200
        zr2_body = r_zr2.json()
        zr2_doc_id = zr2_body.get('document_id')
        assert zr2_body.get('document_state') == 'ACK', (
            f'Second Z_REPORT must ACK, got {zr2_body}'
        )

    # z_report_document_id must be the ACK doc, not the REJECTED one
    with c.connect() as conn:
        row2 = conn.execute(
            "SELECT z_report_document_id FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()
    assert row2[0] == zr2_doc_id, (
        f'z_report_document_id must be retry ACK doc {zr2_doc_id}, got {row2[0]}'
    )
    assert row2[0] != zr1_doc_id, 'REJECTED doc must not persist in linkage'


# ---------------------------------------------------------------------------
# Z2 — Z_REPORT blocked by offline backlog
# ---------------------------------------------------------------------------

def test_z2_zreport_blocked_by_offline_backlog(tmp_path: Path) -> None:
    """Z_REPORT must return OFFLINE_BACKLOG_NOT_SYNCED when pending offline docs exist."""
    cfg = _config(tmp_path, 'z2.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_zreport_ack_mock)))

    with TestClient(create_app(c)) as client:
        # Open shift
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('zr-open-z2'), 'business_ts': '2026-04-12T14:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-z2', 'cashier_id': 'cashier-zr'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK'

        # Seed offline backlog
        with c.connect() as conn:
            _seed_offline_doc(conn, offline_fiscal_no=11001)

        # Z_REPORT — must be blocked
        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('zr-report-z2'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-z2', 'cashier_id': 'cashier-zr'},
        })

    assert r_zr.status_code == 200
    body = r_zr.json()
    assert body.get('error_code') == 'OFFLINE_BACKLOG_NOT_SYNCED', (
        f'Expected OFFLINE_BACKLOG_NOT_SYNCED, got {body}'
    )
