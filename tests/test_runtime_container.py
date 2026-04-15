from __future__ import annotations

import json
from pathlib import Path

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer

ROOT = Path(__file__).resolve().parents[1]


def _container(tmp_path: Path) -> RuntimeContainer:
    return RuntimeContainer(AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "container.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-CONTAINER",
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "container-tests",
        },
    }))


def test_runtime_container_applies_required_pragmas(tmp_path: Path) -> None:
    container = _container(tmp_path)
    container.initialize()
    with container.connect() as conn:
        journal_mode = conn.execute("PRAGMA journal_mode").fetchone()[0]
        synchronous = conn.execute("PRAGMA synchronous").fetchone()[0]
        foreign_keys = conn.execute("PRAGMA foreign_keys").fetchone()[0]
        busy_timeout = conn.execute("PRAGMA busy_timeout").fetchone()[0]

    assert str(journal_mode).lower() == "wal"
    assert int(synchronous) == 2  # FULL
    assert int(foreign_keys) == 1
    assert int(busy_timeout) == 5000


def test_runtime_container_health_assignment_is_validated(tmp_path: Path) -> None:
    container = _container(tmp_path)
    container.health.ready = True
    container.health.startup_complete = True
    assert container.health.ready is True
    assert container.health.startup_complete is True


def test_runtime_container_phase_reaches_running(tmp_path: Path) -> None:
    container = _container(tmp_path)
    container.initialize()
    assert container.health.phase == "RUNNING"
    assert container.last_startup_report is not None


def test_checkbox_config_overrides_patch_in_memory_profile(tmp_path: Path) -> None:
    """checkbox overrides in AppConfig are applied to in-memory transport profile at wire time."""
    container = RuntimeContainer(AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "checkbox-override.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "checkbox": {
            "endpoint": "https://override.checkbox.test/api/v1",
            "license_key": "LIC-OVERRIDE",
            "cashier_pin": "9999",
        },
    }))
    container.initialize()
    assert container.transport_router is not None
    profile = container.transport_router.profiles.get('transport_checkbox_rest_default')
    assert profile is not None
    assert profile.endpoint == "https://override.checkbox.test/api/v1"
    cfg = json.loads(profile.config_json)
    assert cfg['auth']['license_key'] == 'LIC-OVERRIDE'
    assert cfg['auth']['cashier_pin'] == '9999'
    container.shutdown()


def test_checkbox_config_overrides_absent_leaves_db_profile_unchanged(tmp_path: Path) -> None:
    """Without checkbox overrides, transport profile config from DB is preserved as-is."""
    container = _container(tmp_path)
    container.initialize()
    assert container.transport_router is not None
    profile = container.transport_router.profiles.get('transport_checkbox_rest_default')
    assert profile is not None
    # DB seed endpoint is the placeholder value
    assert profile.endpoint == 'https://example.checkbox.local/api/v1'
    cfg = json.loads(profile.config_json)
    assert 'auth' not in cfg  # seed has no auth block
    container.shutdown()


def test_checkbox_missing_auth_logs_warning(tmp_path: Path) -> None:
    """Container logs a warning at startup when active Checkbox profile has no auth credentials."""
    from unittest.mock import patch
    container = _container(tmp_path)
    with patch.object(container.logger, 'warning') as mock_warn:
        container.initialize()
    warned_msgs = [call.args[0] for call in mock_warn.call_args_list]
    assert 'checkbox_profile_missing_auth' in warned_msgs
    container.shutdown()


def test_checkbox_present_auth_no_warning(tmp_path: Path) -> None:
    """Container does not log auth warning when Checkbox profile has credentials."""
    from unittest.mock import patch
    container = RuntimeContainer(AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'auth-check.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'checkbox': {'license_key': 'LIC-TEST', 'cashier_pin': '0000'},
    }))
    with patch.object(container.logger, 'warning') as mock_warn:
        container.initialize()
    warned_msgs = [call.args[0] for call in mock_warn.call_args_list]
    assert 'checkbox_profile_missing_auth' not in warned_msgs
    container.shutdown()


def test_checkbox_partial_override_merges_into_existing_auth(tmp_path: Path) -> None:
    """Partial override (only cashier_pin) merges into existing auth without losing other keys."""
    container = RuntimeContainer(AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "checkbox-partial.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "checkbox": {"cashier_pin": "1111"},
    }))
    container.initialize()
    assert container.transport_router is not None
    profile = container.transport_router.profiles.get('transport_checkbox_rest_default')
    assert profile is not None
    # endpoint unchanged (DB value preserved)
    assert profile.endpoint == 'https://example.checkbox.local/api/v1'
    cfg = json.loads(profile.config_json)
    assert cfg['auth']['cashier_pin'] == '1111'
    assert 'license_key' not in cfg['auth']  # not provided, not injected
    container.shutdown()
