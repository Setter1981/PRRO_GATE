"""
Sprint 2 / bounded step 5 — shift/document linkage persistence tests.

Verifies that shifts record their associated document IDs:
  L1 — SHIFT_OPEN persists open_document_id
  L2 — SHIFT_CLOSE (ACK) persists close_document_id
  L2b — SHIFT_CLOSE (async CLOSING) persists close_document_id for recovery
  L3 — Z_REPORT linkage: link_document sets z_report_document_id without
       touching other link fields (repository isolation test)
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import ShiftState
from prro_gateway.repositories.shifts import ShiftRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# HTTP mock
# ---------------------------------------------------------------------------

def _ack_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-linkage'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'linkage-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-LINK-001',
            'updated_at': '2026-04-12T12:00:00+00:00',
        })
    if path.endswith('/shifts/close'):
        return httpx.Response(200, json={
            'id': 'linkage-shift-001', 'status': 'CLOSED',
            'fiscal_code': 'SHIFT-LINK-001',
            'updated_at': '2026-04-12T12:05:00+00:00',
        })
    return httpx.Response(200, json={
        'id': 'linkage-receipt-001', 'status': 'DONE',
        'fiscal_code': 'RCPT-LINK-001',
        'updated_at': '2026-04-12T12:01:00+00:00',
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
            'channel_owner': 'linkage-test',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'LINK-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-04-12T12:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'linkage-test',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# L1 — SHIFT_OPEN persists open_document_id
# ---------------------------------------------------------------------------

def test_l1_shift_open_persists_open_document_id(tmp_path: Path) -> None:
    """After SHIFT_OPEN ACK, shift.open_document_id must equal the SHIFT_OPEN document_id."""
    cfg = _config(tmp_path, 'l1.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('link-open-l1'), 'business_ts': '2026-04-12T12:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-l1', 'cashier_id': 'cashier-link'},
        })

    assert r_open.status_code == 200
    body = r_open.json()
    assert body.get('document_state') == 'ACK', f'SHIFT_OPEN must ACK: {body}'
    open_doc_id = body['document_id']

    with c.connect() as conn:
        shift = ShiftRepository.get_active_shift(conn, FISCAL_NUMBER)
    assert shift is not None, 'Shift must exist after SHIFT_OPEN'
    assert shift.state == ShiftState.OPENED
    assert shift.open_document_id == open_doc_id, (
        f'open_document_id must be {open_doc_id}, got {shift.open_document_id}'
    )


# ---------------------------------------------------------------------------
# L2 — SHIFT_CLOSE persists close_document_id
# ---------------------------------------------------------------------------

def test_l2_shift_close_persists_close_document_id(tmp_path: Path) -> None:
    """After SHIFT_CLOSE ACK, shift.close_document_id must equal the SHIFT_CLOSE document_id."""
    cfg = _config(tmp_path, 'l2.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        # Open shift
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('link-open-l2'), 'business_ts': '2026-04-12T12:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-l2', 'cashier_id': 'cashier-link'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK'
        open_doc_id = r_open.json()['document_id']

        # Close shift
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('link-close-l2'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-l2', 'cashier_id': 'cashier-link'},
        })

    assert r_close.status_code == 200
    close_body = r_close.json()
    assert close_body.get('document_state') == 'ACK', f'SHIFT_CLOSE must ACK: {close_body}'
    close_doc_id = close_body['document_id']

    with c.connect() as conn:
        # Shift is CLOSED — not in active set, query directly
        row = conn.execute(
            "SELECT open_document_id, close_document_id, z_report_document_id, state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()
    assert row is not None, 'Shift row must exist'
    assert row[3] == 'CLOSED'
    assert row[0] == open_doc_id, f'open_document_id must be {open_doc_id}, got {row[0]}'
    assert row[1] == close_doc_id, f'close_document_id must be {close_doc_id}, got {row[1]}'
    assert row[2] is None, f'z_report_document_id must be None, got {row[2]}'


# ---------------------------------------------------------------------------
# L2b — async SHIFT_CLOSE (CLOSING) persists close_document_id
# ---------------------------------------------------------------------------

def test_l2b_async_shift_close_persists_close_document_id(tmp_path: Path) -> None:
    """When DPS returns SENT/PROCESSING for SHIFT_CLOSE, shift enters CLOSING
    and close_document_id must already be set for recovery safety."""

    def _async_close_mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-l2b'})
        if path.endswith('/shifts') and request.method == 'POST':
            return httpx.Response(200, json={
                'id': 'l2b-shift-001', 'status': 'OPENED',
                'fiscal_code': 'SHIFT-L2B-001',
                'updated_at': '2026-04-12T12:00:00+00:00',
            })
        if path.endswith('/shifts/close'):
            # DPS accepts async — returns PROCESSING, not DONE
            return httpx.Response(200, json={
                'id': 'l2b-shift-001', 'status': 'PROCESSING',
                'fiscal_code': None,
                'updated_at': '2026-04-12T12:05:00+00:00',
            })
        return httpx.Response(200, json={
            'id': 'l2b-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-L2B-001',
            'updated_at': '2026-04-12T12:01:00+00:00',
        })

    cfg = _config(tmp_path, 'l2b.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_async_close_mock)))

    with TestClient(create_app(c)) as client:
        # Open shift
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('link-open-l2b'), 'business_ts': '2026-04-12T12:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-l2b', 'cashier_id': 'cashier-link'},
        })
        assert r_open.status_code == 200 and r_open.json().get('document_state') == 'ACK'

        # Close shift — DPS returns async (PROCESSING → SENT)
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('link-close-l2b'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-l2b', 'cashier_id': 'cashier-link'},
        })

    assert r_close.status_code == 200
    close_body = r_close.json()
    close_doc_id = close_body.get('document_id')
    assert close_doc_id is not None, f'SHIFT_CLOSE must produce a document: {close_body}'
    # Document should be SENT (async), not ACK
    assert close_body.get('document_state') in ('SENT', 'KVT1', 'KVT2'), (
        f'Async SHIFT_CLOSE must be SENT/KVT, got {close_body.get("document_state")}'
    )

    with c.connect() as conn:
        row = conn.execute(
            "SELECT state, close_document_id FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()
    assert row is not None, 'Shift row must exist'
    assert row[0] == 'CLOSING', f'Shift must be CLOSING, got {row[0]}'
    assert row[1] == close_doc_id, (
        f'close_document_id must be set during CLOSING for recovery, got {row[1]}'
    )


# ---------------------------------------------------------------------------
# L3 — Z_REPORT linkage (repository-level)
# ---------------------------------------------------------------------------

def test_l3_z_report_linkage_repository_level(conn) -> None:
    """ShiftRepository.link_document sets z_report_document_id without touching other fields.

    End-to-end Z_REPORT is covered by test_sprint2_zreport_transport::test_z1.
    This test verifies repository-level isolation: link_document updates only
    the requested column and leaves other link fields untouched.
    """
    # Create a shift
    shift = ShiftRepository.create_shift(
        conn,
        shift_id='shift-l3-test',
        fiscal_number=FISCAL_NUMBER,
        state=ShiftState.OPENED,
        open_mode='ONLINE',
        backend_profile_id=BACKEND_PROFILE,
        transport_profile_id=TRANSPORT_PROFILE,
        protocol='CHECKBOX_REST',
        integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
        open_document_id='doc-open-l3',
    )
    assert shift.open_document_id == 'doc-open-l3'
    assert shift.z_report_document_id is None

    # Link Z_REPORT document
    ShiftRepository.link_document(conn, shift_id='shift-l3-test', z_report_document_id='doc-zreport-l3')
    conn.commit()

    updated = ShiftRepository.get_by_id(conn, 'shift-l3-test')
    assert updated is not None
    assert updated.z_report_document_id == 'doc-zreport-l3', (
        f'z_report_document_id must be doc-zreport-l3, got {updated.z_report_document_id}'
    )
    # Other links untouched
    assert updated.open_document_id == 'doc-open-l3'
    assert updated.close_document_id is None
