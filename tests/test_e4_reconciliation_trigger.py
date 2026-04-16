"""E4 — POST /v1/admin/reconciliation/trigger

E4_1: POST {"fiscal_number": "FN-..."} with no pending docs → 200, fiscal_number echoed, all counts 0
E4_2: POST {} (no fiscal_number) → 200, fiscal_number=null, all counts 0
E4_3: POST when reconciliation_service is None → 503
"""
from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
FN = 'FN-DEV-0001'


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'e4.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {'fiscal_number': FN},
        'runtime': {
            'reconcile_on_startup': False,
            'ops_loop_enabled': False,
        },
    })


def test_e4_trigger_specific_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    try:
        with TestClient(create_app(container)) as client:
            resp = client.post('/v1/admin/reconciliation/trigger',
                               json={'fiscal_number': FN})
        assert resp.status_code == 200
        data = resp.json()
        assert data['fiscal_number'] == FN
        assert data['checked'] == 0
        assert data['acked'] == 0
        assert data['rejected'] == 0
        assert data['retryable'] == 0
        assert data['still_pending'] == 0
        assert data['manual'] == 0
    finally:
        container.shutdown()


def test_e4_trigger_all_fns(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    try:
        with TestClient(create_app(container)) as client:
            resp = client.post('/v1/admin/reconciliation/trigger', json={})
        assert resp.status_code == 200
        data = resp.json()
        assert data['fiscal_number'] is None
        assert data['checked'] == 0
    finally:
        container.shutdown()


def test_e4_503_when_service_unavailable(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    try:
        with TestClient(create_app(container)) as client:
            # Null out after lifespan/initialize so the endpoint sees None
            container.reconciliation_service = None
            resp = client.post('/v1/admin/reconciliation/trigger', json={})
        assert resp.status_code == 503
        assert 'reconciliation_service not available' in resp.json()['detail']
    finally:
        container.shutdown()
