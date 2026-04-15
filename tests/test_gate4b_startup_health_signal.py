"""
Gate 4b — startup failure health signaling.

initialize() now wraps _ensure_persistent_pragmas() and _migrate() in try/except.
On exception:
  health.live = False
  health.ready = False
  health.startup_complete = False
  health.phase = 'STARTUP_ERROR'
  health.last_error = str(exc)
  logger.error("startup_storage_failure", ...)
  raise  (exception still propagates — startup is visibly stopped)

Four tests:
  A — corrupt DB: RuntimeError propagates AND health state shows STARTUP_ERROR,
      live=False, ready=False, startup_complete=False, last_error set
  B — normal startup: no exception, health state is healthy,
      last_error=None, phase='RUNNING'
  C — /health/live on healthy startup returns {"status": "ok"}
  D — health state after startup error: startup_complete=False (not partial-success)
"""
from __future__ import annotations

from pathlib import Path

import httpx
import pytest
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]


def _strict_no_http(request: httpx.Request) -> httpx.Response:
    raise AssertionError(f'gate4b: unexpected HTTP {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str, auto_migrate: bool = True) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': auto_migrate,
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'gate4b',
        },
        'runtime': {'process_immediately': False, 'reconcile_on_startup': False},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE4B-LIC',
            'cashier_pin': '0000',
        },
    })


# ---------------------------------------------------------------------------
# Test A — corrupt DB: exception propagates AND health state updated
# ---------------------------------------------------------------------------

def test_gate4b_corrupt_db_sets_startup_error_health(tmp_path: Path) -> None:
    """
    Corrupt DB file → RuntimeError propagates from initialize().
    Before Gate 4b: health.phase stayed 'CREATED', health.live stayed True
    (misleading: orchestrator saw live=True even though startup failed).
    After Gate 4b: health.phase='STARTUP_ERROR', health.live=False, last_error set.
    """
    db_path = tmp_path / 'gate4b_a.sqlite3'
    db_path.write_bytes(b'not a valid sqlite database\x00\xff\xfe' * 100)

    cfg = _config(tmp_path, 'gate4b_a.sqlite3', auto_migrate=False)
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )

    with pytest.raises(RuntimeError):
        with TestClient(create_app(c)):
            pass

    assert c.health.phase == 'STARTUP_ERROR', (
        f'Gate 4b: health.phase must be STARTUP_ERROR after corrupt DB: '
        f'got {c.health.phase!r}'
    )
    assert c.health.live is False, (
        f'Gate 4b: health.live must be False after startup failure: got {c.health.live!r}'
    )
    assert c.health.ready is False, (
        f'Gate 4b: health.ready must be False after startup failure: got {c.health.ready!r}'
    )
    assert c.health.startup_complete is False, (
        f'Gate 4b: health.startup_complete must be False after startup failure'
    )
    assert c.health.last_error is not None and len(c.health.last_error) > 0, (
        f'Gate 4b: health.last_error must contain error description: '
        f'got {c.health.last_error!r}'
    )


# ---------------------------------------------------------------------------
# Test B — normal startup: healthy state, last_error=None
# ---------------------------------------------------------------------------

def test_gate4b_normal_startup_health_is_healthy(tmp_path: Path) -> None:
    """
    Normal startup path not broken: health state is correct after successful initialize().
    last_error=None, phase='RUNNING', startup_complete=True.
    """
    cfg = _config(tmp_path, 'gate4b_b.sqlite3')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )

    phase_during_run = None
    live_during_run = None
    startup_complete_during_run = None
    last_error_during_run = 'sentinel'

    with TestClient(create_app(c)) as client:
        # Read state while app is running (before shutdown() sets phase='STOPPING')
        phase_during_run = c.health.phase
        live_during_run = c.health.live
        startup_complete_during_run = c.health.startup_complete
        last_error_during_run = c.health.last_error

    assert phase_during_run == 'RUNNING', (
        f'Gate 4b: normal startup must reach RUNNING: got {phase_during_run!r}'
    )
    assert live_during_run is True
    assert startup_complete_during_run is True
    assert last_error_during_run is None, (
        f'Gate 4b: last_error must be None after clean startup: got {last_error_during_run!r}'
    )


# ---------------------------------------------------------------------------
# Test C — /health/live returns "ok" on healthy startup
# ---------------------------------------------------------------------------

def test_gate4b_health_live_returns_ok_on_healthy_startup(tmp_path: Path) -> None:
    """
    /health/live endpoint returns {"status": "ok"} when startup completed normally.
    Confirms health signaling path is end-to-end wired.
    """
    cfg = _config(tmp_path, 'gate4b_c.sqlite3')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )

    with TestClient(create_app(c)) as client:
        resp = client.get('/health/live')

    assert resp.status_code == 200
    body = resp.json()
    assert body.get('status') == 'ok', (
        f'Gate 4b: /health/live must return ok after clean startup: {body}'
    )
    assert body.get('phase') == 'RUNNING', (
        f'Gate 4b: /health/live phase must be RUNNING: {body}'
    )


# ---------------------------------------------------------------------------
# Test D — startup_error: startup_complete=False preserved after exception
# ---------------------------------------------------------------------------

def test_gate4b_startup_error_complete_is_false(tmp_path: Path) -> None:
    """
    After startup failure, startup_complete must be False — not accidentally True.
    Verifies that the health state update in the except block sets all three
    flags consistently (live=False, ready=False, startup_complete=False).
    An orchestrator relying on startup_complete to gate traffic must see False.
    """
    db_path = tmp_path / 'gate4b_d.sqlite3'
    db_path.write_bytes(b'garbage bytes' * 200)

    cfg = _config(tmp_path, 'gate4b_d.sqlite3', auto_migrate=False)
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http)),
    )

    with pytest.raises(RuntimeError):
        with TestClient(create_app(c)):
            pass

    # All three flags must be consistently False
    assert c.health.live is False
    assert c.health.ready is False
    assert c.health.startup_complete is False
    assert c.health.phase == 'STARTUP_ERROR'
    # last_error carries the diagnostic
    assert 'Database' in (c.health.last_error or ''), (
        f'Gate 4b: last_error must mention Database: got {c.health.last_error!r}'
    )
