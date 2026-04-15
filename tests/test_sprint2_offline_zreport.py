"""
Sprint 2 / bounded step 7 — offline Z_REPORT contour tests.

Coverage matrix:
  OZ1 — offline Z_REPORT with prior offline backlog: OFFLINE_LOCAL_ACK, no transport,
        z_report_document_id linked, submission_status='OFFLINE_LOCAL'
  OZ2 — online Z_REPORT still blocked by offline backlog (guard unchanged)
  OZ3 — SHIFT_CLOSE still blocked by offline backlog even in OFFLINE mode
  OZ4 — duplicate offline Z_REPORT blocked (INVALID_RECEIPT_SEQUENCE)
  OZ5 — SELL blocked after offline Z_REPORT (shift CLOSING)
  OZ6 — SHIFT_CLOSE blocked when linked Z_REPORT still OFFLINE_LOCAL_ACK
  OZ7 — SHIFT_CLOSE succeeds after Z_REPORT synced to ACK

Invariants verified:
  #1  transport NOT called in offline path
  linkage: z_report_document_id set on shift
  legal blocker: SHIFT_CLOSE always blocked with backlog; online Z_REPORT blocked
"""
from __future__ import annotations

from datetime import datetime, timedelta, timezone
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import NodeMode
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# HTTP mock
# ---------------------------------------------------------------------------

def _shift_only_mock(request: httpx.Request) -> httpx.Response:
    """Allows SHIFT_OPEN only. Any other transport call = test failure."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-oz'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'oz-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-OZ-001',
            'updated_at': '2026-04-12T16:00:00+00:00',
        })
    raise AssertionError(
        f'offline_zreport: transport called in OFFLINE mode — {request.method} {request.url}'
    )


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
            'channel_owner': 'oz-test',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'OZ-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-04-12T16:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'oz-test',
        'business_ts': ts,
    }


def _open_shift(client: TestClient) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx('oz-open'), 'business_ts': '2026-04-12T16:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {'external_request_id': 'ext-open-oz', 'cashier_id': 'cashier-oz'},
    })
    assert resp.status_code == 200 and resp.json().get('document_state') == 'ACK', (
        f'SHIFT_OPEN must ACK: {resp.json()}'
    )


def _seed_offline_state(container: RuntimeContainer) -> None:
    """Switch node to OFFLINE and seed session + range."""
    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')
        NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.OFFLINE)
        _now = datetime.now(timezone.utc)
        _start = (_now - timedelta(minutes=5)).strftime('%Y-%m-%dT%H:%M:%S')
        OfflineRepository.create_open_session(
            conn,
            offline_session_id='oz-session-001',
            fiscal_number=FISCAL_NUMBER,
            started_at=_start,
            status='OPEN',
            accumulated_month_seconds=0,
        )
        conn.execute(
            """INSERT INTO offline_ranges
                (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
                 next_fiscal_no, issued_at, status)
            VALUES (?, ?, ?, ?, ?, ?, ?)""",
            ('oz-range-001', FISCAL_NUMBER, 5001, 5010, 5001, _start, 'ACTIVE'),
        )
        conn.commit()


# ---------------------------------------------------------------------------
# OZ1 — offline Z_REPORT: OFFLINE_LOCAL_ACK, no transport, linkage
# ---------------------------------------------------------------------------

def test_oz1_offline_zreport_with_backlog(tmp_path: Path) -> None:
    """Offline Z_REPORT succeeds even with prior offline docs (SELL).
    Must be OFFLINE_LOCAL_ACK, transport not called, z_report_document_id linked."""
    cfg = _config(tmp_path, 'oz1.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_shift_only_mock)))

    with TestClient(create_app(c)) as client:
        # Phase 1: open shift ONLINE
        _open_shift(client)

        # Phase 2: switch to OFFLINE
        _seed_offline_state(c)

        # Phase 3: offline SELL (creates backlog)
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-oz-001',
                'cashier_id': 'cashier-oz',
                'goods': [{'name': 'Widget', 'price': 5000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 5000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'OFFLINE_LOCAL_ACK', (
            f'Offline SELL must be OFFLINE_LOCAL_ACK: {r_sell.json()}'
        )

        # Phase 4: offline Z_REPORT — must succeed despite backlog
        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz-zreport-001'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-oz-001', 'cashier_id': 'cashier-oz'},
        })

    assert r_zr.status_code == 200, f'Expected 200, got {r_zr.status_code}: {r_zr.text}'
    body = r_zr.json()
    assert body.get('document_state') == 'OFFLINE_LOCAL_ACK', (
        f'Offline Z_REPORT must be OFFLINE_LOCAL_ACK: {body}'
    )
    zr_doc_id = body['document_id']

    # Verify shift linkage + CLOSING state
    with c.connect() as conn:
        row = conn.execute(
            "SELECT z_report_document_id, state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()
    assert row is not None
    assert row[0] == zr_doc_id, f'z_report_document_id must be {zr_doc_id}, got {row[0]}'
    assert row[1] == 'CLOSING', f'Shift must be CLOSING after offline Z_REPORT, got {row[1]}'


# ---------------------------------------------------------------------------
# OZ2 — online Z_REPORT still blocked by backlog
# ---------------------------------------------------------------------------

def test_oz2_online_zreport_still_blocked_by_backlog(tmp_path: Path) -> None:
    """Online Z_REPORT must still be blocked when offline backlog exists."""
    cfg = _config(tmp_path, 'oz2.sqlite3')

    def _ack_mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-oz2'})
        if path.endswith('/shifts') and request.method == 'POST':
            return httpx.Response(200, json={
                'id': 'oz2-shift', 'status': 'OPENED',
                'fiscal_code': 'SHIFT-OZ2', 'updated_at': '2026-04-12T16:00:00+00:00',
            })
        return httpx.Response(200, json={
            'id': 'oz2-r', 'status': 'DONE',
            'fiscal_code': 'R-OZ2', 'updated_at': '2026-04-12T16:01:00+00:00',
        })

    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        _open_shift(client)

        # Create offline backlog, then return to ONLINE
        _seed_offline_state(c)
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz2-sell'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-oz2',
                'cashier_id': 'cashier-oz',
                'goods': [{'name': 'W', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r_sell.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        # Return to ONLINE
        with c.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.ONLINE)
            conn.commit()

        # Online Z_REPORT — must be blocked
        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz2-zreport'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-oz2', 'cashier_id': 'cashier-oz'},
        })

    assert r_zr.status_code == 200
    assert r_zr.json().get('error_code') == 'OFFLINE_BACKLOG_NOT_SYNCED', (
        f'Online Z_REPORT must be blocked by backlog: {r_zr.json()}'
    )


# ---------------------------------------------------------------------------
# OZ3 — SHIFT_CLOSE still blocked even in OFFLINE mode
# ---------------------------------------------------------------------------

def test_oz3_shift_close_blocked_in_offline_mode(tmp_path: Path) -> None:
    """SHIFT_CLOSE must remain blocked by backlog even when node is OFFLINE."""
    cfg = _config(tmp_path, 'oz3.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_shift_only_mock)))

    with TestClient(create_app(c)) as client:
        _open_shift(client)
        _seed_offline_state(c)

        # Create offline backlog
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz3-sell'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-oz3',
                'cashier_id': 'cashier-oz',
                'goods': [{'name': 'W', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r_sell.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        # SHIFT_CLOSE in OFFLINE mode — must still be blocked
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz3-close'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-oz3', 'cashier_id': 'cashier-oz'},
        })

    assert r_close.status_code == 200
    assert r_close.json().get('error_code') == 'OFFLINE_BACKLOG_NOT_SYNCED', (
        f'SHIFT_CLOSE must remain blocked in OFFLINE mode: {r_close.json()}'
    )


# ---------------------------------------------------------------------------
# OZ4 — duplicate offline Z_REPORT blocked
# ---------------------------------------------------------------------------

def test_oz4_duplicate_offline_zreport_blocked(tmp_path: Path) -> None:
    """Second offline Z_REPORT for the same shift must be blocked."""
    cfg = _config(tmp_path, 'oz4.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_shift_only_mock)))

    with TestClient(create_app(c)) as client:
        _open_shift(client)
        _seed_offline_state(c)

        # First offline Z_REPORT — succeeds
        r_zr1 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz4-zreport-1'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-oz4-1', 'cashier_id': 'cashier-oz'},
        })
        assert r_zr1.status_code == 200
        assert r_zr1.json().get('document_state') == 'OFFLINE_LOCAL_ACK', (
            f'First offline Z_REPORT must succeed: {r_zr1.json()}'
        )

        # Second offline Z_REPORT — must be blocked
        r_zr2 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz4-zreport-2'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-oz4-2', 'cashier_id': 'cashier-oz'},
        })

    assert r_zr2.status_code == 200
    assert r_zr2.json().get('error_code') == 'INVALID_RECEIPT_SEQUENCE', (
        f'Duplicate Z_REPORT must be INVALID_RECEIPT_SEQUENCE: {r_zr2.json()}'
    )


# ---------------------------------------------------------------------------
# OZ5 — SELL blocked after offline Z_REPORT (shift CLOSING)
# ---------------------------------------------------------------------------

def test_oz5_sell_blocked_after_offline_zreport(tmp_path: Path) -> None:
    """After offline Z_REPORT, shift is CLOSING — further SELL must be blocked."""
    cfg = _config(tmp_path, 'oz5.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_shift_only_mock)))

    with TestClient(create_app(c)) as client:
        _open_shift(client)
        _seed_offline_state(c)

        # Offline SELL
        r_sell1 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz5-sell-1'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-oz5-1',
                'cashier_id': 'cashier-oz',
                'goods': [{'name': 'W', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r_sell1.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        # Offline Z_REPORT — sets shift to CLOSING
        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz5-zreport'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-oz5', 'cashier_id': 'cashier-oz'},
        })
        assert r_zr.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        # Subsequent SELL — must be blocked
        r_sell2 = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz5-sell-2'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-oz5-2',
                'cashier_id': 'cashier-oz',
                'goods': [{'name': 'X', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })

    assert r_sell2.status_code == 200
    assert r_sell2.json().get('error_code') == 'INVALID_RECEIPT_SEQUENCE', (
        f'SELL after offline Z_REPORT must be blocked: {r_sell2.json()}'
    )


# ---------------------------------------------------------------------------
# OZ6 — SHIFT_CLOSE blocked when linked Z_REPORT still OFFLINE_LOCAL_ACK
# ---------------------------------------------------------------------------

def test_oz6_shift_close_blocked_until_zreport_ack(tmp_path: Path) -> None:
    """SHIFT_CLOSE must be blocked when shift is CLOSING and linked Z_REPORT
    is still OFFLINE_LOCAL_ACK (not yet DPS-confirmed)."""

    def _ack_mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-oz6'})
        if path.endswith('/shifts') and request.method == 'POST':
            return httpx.Response(200, json={
                'id': 'oz6-shift', 'status': 'OPENED',
                'fiscal_code': 'SHIFT-OZ6', 'updated_at': '2026-04-12T16:00:00+00:00',
            })
        if path.endswith('/shifts/close'):
            return httpx.Response(200, json={
                'id': 'oz6-shift', 'status': 'CLOSED',
                'fiscal_code': 'SHIFT-OZ6', 'updated_at': '2026-04-12T16:10:00+00:00',
            })
        return httpx.Response(200, json={
            'id': 'oz6-r', 'status': 'DONE',
            'fiscal_code': 'R-OZ6', 'updated_at': '2026-04-12T16:01:00+00:00',
        })

    cfg = _config(tmp_path, 'oz6.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_ack_mock)))

    with TestClient(create_app(c)) as client:
        _open_shift(client)
        _seed_offline_state(c)

        # Offline SELL + Z_REPORT → shift CLOSING, Z_REPORT is OFFLINE_LOCAL_ACK
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz6-sell'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-oz6',
                'cashier_id': 'cashier-oz',
                'goods': [{'name': 'W', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r_sell.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz6-zreport'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-oz6', 'cashier_id': 'cashier-oz'},
        })
        assert r_zr.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        # Return to ONLINE — backlog still exists
        with c.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.ONLINE)
            conn.commit()

        # SHIFT_CLOSE — blocked by backlog (SELL + Z_REPORT still OFFLINE_LOCAL_ACK)
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz6-close'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-oz6', 'cashier_id': 'cashier-oz'},
        })

    assert r_close.status_code == 200
    assert r_close.json().get('error_code') == 'OFFLINE_BACKLOG_NOT_SYNCED', (
        f'SHIFT_CLOSE must be blocked while backlog exists: {r_close.json()}'
    )


# ---------------------------------------------------------------------------
# OZ7 — SHIFT_CLOSE succeeds after full sync (Z_REPORT ACK)
# ---------------------------------------------------------------------------

def test_oz7_shift_close_succeeds_after_sync(tmp_path: Path) -> None:
    """After offline sync drains backlog (all docs ACK), SHIFT_CLOSE succeeds."""

    def _full_ack_mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-oz7'})
        if path.endswith('/shifts') and request.method == 'POST':
            return httpx.Response(200, json={
                'id': 'oz7-shift', 'status': 'OPENED',
                'fiscal_code': 'SHIFT-OZ7', 'updated_at': '2026-04-12T16:00:00+00:00',
            })
        if path.endswith('/shifts/close'):
            return httpx.Response(200, json={
                'id': 'oz7-shift', 'status': 'CLOSED',
                'fiscal_code': 'SHIFT-OZ7', 'updated_at': '2026-04-12T16:10:00+00:00',
            })
        if path.endswith('/reports') and request.method == 'POST':
            return httpx.Response(200, json={
                'id': 'oz7-report', 'status': 'DONE',
                'fiscal_code': 'ZR-OZ7', 'updated_at': '2026-04-12T16:05:00+00:00',
            })
        return httpx.Response(200, json={
            'id': 'oz7-r', 'status': 'DONE',
            'fiscal_code': 'R-OZ7', 'updated_at': '2026-04-12T16:01:00+00:00',
        })

    cfg = _config(tmp_path, 'oz7.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_full_ack_mock)))

    with TestClient(create_app(c)) as client:
        _open_shift(client)
        _seed_offline_state(c)

        # Offline SELL + Z_REPORT
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz7-sell'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-oz7',
                'cashier_id': 'cashier-oz',
                'goods': [{'name': 'W', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r_sell.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        r_zr = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz7-zreport'),
            'operation': 'Z_REPORT',
            'request': {'external_request_id': 'ext-zreport-oz7', 'cashier_id': 'cashier-oz'},
        })
        assert r_zr.json().get('document_state') == 'OFFLINE_LOCAL_ACK'

        # Return to ONLINE
        with c.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.ONLINE)
            conn.commit()

        # Sync offline backlog via admin endpoint
        r_sync = client.post('/v1/admin/offline-sync', json={'fiscal_number': FISCAL_NUMBER})
        assert r_sync.status_code == 200
        sync_body = r_sync.json()
        assert sync_body['synced'] == 2, f'Both docs must sync: {sync_body}'

        # Now SHIFT_CLOSE — must succeed (backlog drained, Z_REPORT ACK)
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('oz7-close'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-oz7', 'cashier_id': 'cashier-oz'},
        })

    assert r_close.status_code == 200
    body = r_close.json()
    assert body.get('document_state') == 'ACK', f'SHIFT_CLOSE must ACK after sync: {body}'
