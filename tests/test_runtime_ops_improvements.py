from __future__ import annotations

from pathlib import Path

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.maria_shell import MariaIngressShell
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'ops.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-OPS',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'ops-tests',
        },
    })


def test_rest_lifespan_shutdown_marks_health_false(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        assert client.get('/health/live').status_code == 200
        assert container.health.live is True
    assert container.health.live is False
    assert container.health.ready is False
    assert container.health.phase == 'STOPPING'


def test_shell_close_shuts_down_container(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    shell = MariaIngressShell(container)
    shell.close()
    assert container.health.live is False
    assert container.health.ready is False


def test_config_from_env_overrides(monkeypatch, tmp_path: Path) -> None:
    cfg_path = tmp_path / 'config.yaml'
    cfg_path.write_text(
        'database:\n'
        '  db_path: ./base.sqlite3\n'
        '  sql_dir: ./sql\n'
        'defaults:\n'
        '  fiscal_number: FN-BASE\n'
        '  backend_profile_id: backend_checkbox_default\n'
        '  transport_profile_id: transport_checkbox_rest_default\n'
        '  channel_owner: tests\n',
        encoding='utf-8',
    )
    monkeypatch.setenv('PRRO_GATEWAY_CONFIG', str(cfg_path))
    monkeypatch.setenv('PRRO_DB_PATH', '/tmp/override.sqlite3')
    monkeypatch.setenv('PRRO_FISCAL_NUMBER', 'FN-OVERRIDE')
    monkeypatch.setenv('PRRO_REST_PORT', '18080')
    monkeypatch.setenv('PRRO_LOG_LEVEL', 'DEBUG')
    cfg = AppConfig.from_env()
    assert cfg.database.db_path == '/tmp/override.sqlite3'
    assert cfg.defaults.fiscal_number == 'FN-OVERRIDE'
    assert cfg.ingress.rest.port == 18080
    assert cfg.logging.level == 'DEBUG'
