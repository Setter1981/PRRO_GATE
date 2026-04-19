"""Sprint 12 — Admin API endpoint tests.

AA1: GET /v1/admin/node-state → 200 with mode and shift_state
AA2: GET /v1/admin/node-state → 404 for unknown fiscal_number
AA3: GET /v1/admin/offline-ranges → empty list when no ranges
AA4: GET /v1/admin/offline-ranges → populated after ASK_OFFLINE_CODES
AA5: GET /v1/admin/offline-sessions → active_session=null when ONLINE
AA6: GET /v1/admin/offline-sessions → active_session populated after GO_OFFLINE
AA7: GET /v1/admin/node-state → codes_remaining reflects active range
"""
from __future__ import annotations

import sqlite3
from pathlib import Path

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'admin_api.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {'fiscal_number': FN},
        'runtime': {
            'reconcile_on_startup': False,
            'ops_loop_enabled': False,
        },
    })


def _make_client(tmp_path: Path):
    container = RuntimeContainer(_config(tmp_path))
    app = create_app(container)
    client = TestClient(app, raise_server_exceptions=True)
    return client, container


# ---------------------------------------------------------------------------
# AA1 — node-state returns mode
# ---------------------------------------------------------------------------

def test_aa1_node_state_returns_mode(tmp_path: Path) -> None:
    client, container = _make_client(tmp_path)
    with client:
        resp = client.get('/v1/admin/node-state', params={'fiscal_number': FN})
    assert resp.status_code == 200
    data = resp.json()
    assert data['fiscal_number'] == FN
    assert data['mode'] == 'ONLINE'
    assert 'shift_state' in data
    assert 'crypto_breaker_open' in data
    assert 'enforce_blocked_mode' in data
    assert data['enforce_blocked_mode'] is False  # default


# ---------------------------------------------------------------------------
# AA2 — node-state 404 for unknown FN
# ---------------------------------------------------------------------------

def test_aa2_node_state_not_found(tmp_path: Path) -> None:
    client, container = _make_client(tmp_path)
    with client:
        resp = client.get('/v1/admin/node-state', params={'fiscal_number': 'UNKNOWN-FN'})
    assert resp.status_code == 404


# ---------------------------------------------------------------------------
# AA3 — offline-ranges empty when no ranges
# ---------------------------------------------------------------------------

def test_aa3_offline_ranges_empty(tmp_path: Path) -> None:
    client, container = _make_client(tmp_path)
    with client:
        resp = client.get('/v1/admin/offline-ranges', params={'fiscal_number': FN})
    assert resp.status_code == 200
    data = resp.json()
    assert data['fiscal_number'] == FN
    assert data['ranges'] == []
    assert data['total_remaining'] == 0


# ---------------------------------------------------------------------------
# AA4 — offline-ranges populated after seeding a range
# ---------------------------------------------------------------------------

def test_aa4_offline_ranges_populated(tmp_path: Path) -> None:
    client, container = _make_client(tmp_path)
    with client:
        with container.connect() as conn:
            from prro_gateway.repositories.offline import OfflineRepository
            conn.execute('BEGIN IMMEDIATE')
            OfflineRepository.create_range(
                conn, range_id='range-aa4', fiscal_number=FN,
                first_fiscal_no=5001, last_fiscal_no=5100,
                issued_at='2026-04-15T09:00:00+00:00',
            )
            conn.commit()
        resp = client.get('/v1/admin/offline-ranges', params={'fiscal_number': FN})
    assert resp.status_code == 200
    data = resp.json()
    assert len(data['ranges']) == 1
    r = data['ranges'][0]
    assert r['range_id'] == 'range-aa4'
    assert r['first_fiscal_no'] == 5001
    assert r['last_fiscal_no'] == 5100
    assert r['codes_remaining'] == 100
    assert r['status'] == 'ACTIVE'
    assert data['total_remaining'] == 100


# ---------------------------------------------------------------------------
# AA5 — offline-sessions: active_session=null when ONLINE
# ---------------------------------------------------------------------------

def test_aa5_offline_sessions_no_active(tmp_path: Path) -> None:
    client, container = _make_client(tmp_path)
    with client:
        resp = client.get('/v1/admin/offline-sessions', params={'fiscal_number': FN})
    assert resp.status_code == 200
    data = resp.json()
    assert data['fiscal_number'] == FN
    assert data['active_session'] is None
    assert 'month_limit_seconds' in data
    assert 'continuous_limit_seconds' in data
    assert data['month_limit_seconds'] == 168 * 3600
    assert data['continuous_limit_seconds'] == 36 * 3600


# ---------------------------------------------------------------------------
# AA6 — offline-sessions: active_session populated after GO_OFFLINE
# ---------------------------------------------------------------------------

def test_aa6_offline_sessions_active_after_offline(tmp_path: Path) -> None:
    client, container = _make_client(tmp_path)
    with client:
        with container.connect() as conn:
            from prro_gateway.repositories.offline import OfflineRepository
            conn.execute('BEGIN IMMEDIATE')
            OfflineRepository.create_open_session(
                conn, offline_session_id='sess-aa6', fiscal_number=FN,
                started_at='2026-04-15T10:00:00+00:00', status='OPEN',
            )
            from prro_gateway.repositories.node_state import NodeStateRepository
            from prro_gateway.enums import NodeMode
            NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=NodeMode.OFFLINE)
            conn.commit()
        resp = client.get('/v1/admin/offline-sessions', params={'fiscal_number': FN})
    assert resp.status_code == 200
    data = resp.json()
    session = data['active_session']
    assert session is not None
    assert session['offline_session_id'] == 'sess-aa6'
    assert session['status'] == 'OPEN'
    assert session['elapsed_seconds'] > 0


# ---------------------------------------------------------------------------
# AA7 — node-state codes_remaining reflects active range
# ---------------------------------------------------------------------------

def test_aa7_node_state_codes_remaining(tmp_path: Path) -> None:
    client, container = _make_client(tmp_path)
    with client:
        with container.connect() as conn:
            from prro_gateway.repositories.offline import OfflineRepository
            conn.execute('BEGIN IMMEDIATE')
            OfflineRepository.create_range(
                conn, range_id='range-aa7', fiscal_number=FN,
                first_fiscal_no=2001, last_fiscal_no=2050,
                issued_at='2026-04-15T09:00:00+00:00',
            )
            conn.commit()
        resp = client.get('/v1/admin/node-state', params={'fiscal_number': FN})
    assert resp.status_code == 200
    data = resp.json()
    assert data['codes_remaining'] == 50
    assert data['active_range_id'] == 'range-aa7'
