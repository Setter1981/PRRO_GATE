from __future__ import annotations

from pathlib import Path

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'runtime.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-RUNTIME',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'rest-tests',
        },
    })


def test_metrics_endpoint_and_ops_summary(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        metrics = client.get('/metrics')
        assert metrics.status_code == 200
        body = metrics.json()
        assert 'counters' in body
        assert 'gauges' in body
        summary = client.get('/v1/ops/summary')
        assert summary.status_code == 200
        data = summary.json()
        assert data['health']['ready'] is True
        assert data['startup_report']['phase1_ok'] is True


def test_adapter_mapping_error_creates_alert_audit_log(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        response = client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'req-bad-1',
                'fiscal_number': 'FN-RUNTIME',
                'backend_profile_id': 'backend_checkbox_default',
                'transport_profile_id': 'transport_checkbox_rest_default',
                'channel_owner': 'rest-tests',
                'business_ts': '2026-03-28T12:00:00Z',
            },
            'operation': 'NOPE',
            'request': {},
        })
        assert response.status_code == 400
        metrics = client.get('/metrics').json()
        assert metrics['counters']['ingress.checkbox.error'] == 1
    with container.connect() as conn:
        row = conn.execute("SELECT event_type, severity FROM audit_log ORDER BY audit_id DESC LIMIT 1").fetchone()
        assert row is not None
        assert row['event_type'] == 'ADAPTER_MAPPING_ERROR'
        assert row['severity'] == 'WARNING'
