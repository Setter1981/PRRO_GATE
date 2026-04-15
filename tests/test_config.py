from __future__ import annotations

import os
from pathlib import Path

from prro_gateway.config import AppConfig


def test_config_loads_from_yaml(tmp_path: Path) -> None:
    cfg = tmp_path / 'config.yaml'
    cfg.write_text(
        'app_name: test-gw\n'
        'database:\n'
        '  db_path: ./tmp/test.sqlite3\n'
        '  sql_dir: ./sql\n'
        'defaults:\n'
        '  fiscal_number: FN-TEST\n'
        '  backend_profile_id: backend_checkbox_default\n'
        '  transport_profile_id: transport_checkbox_rest_default\n'
        '  channel_owner: tests\n',
        encoding='utf-8',
    )
    loaded = AppConfig.from_file(cfg)
    assert loaded.app_name == 'test-gw'
    assert loaded.defaults.fiscal_number == 'FN-TEST'


def test_checkbox_env_overrides_applied_to_config(monkeypatch) -> None:
    monkeypatch.setenv('PRRO_CHECKBOX_ENDPOINT', 'https://custom.checkbox.test/api/v1')
    monkeypatch.setenv('PRRO_CHECKBOX_LICENSE_KEY', 'LIC-ENV-1')
    monkeypatch.setenv('PRRO_CHECKBOX_CASHIER_PIN', '4321')
    cfg = AppConfig.from_env()
    assert cfg.checkbox.endpoint == 'https://custom.checkbox.test/api/v1'
    assert cfg.checkbox.license_key == 'LIC-ENV-1'
    assert cfg.checkbox.cashier_pin == '4321'


def test_checkbox_env_overrides_absent_leaves_defaults(monkeypatch) -> None:
    for var in ('PRRO_CHECKBOX_ENDPOINT', 'PRRO_CHECKBOX_LICENSE_KEY', 'PRRO_CHECKBOX_CASHIER_PIN'):
        monkeypatch.delenv(var, raising=False)
    cfg = AppConfig.from_env()
    assert cfg.checkbox.endpoint is None
    assert cfg.checkbox.license_key is None
    assert cfg.checkbox.cashier_pin is None


def test_checkbox_partial_override_via_mapping() -> None:
    cfg = AppConfig.from_mapping({'checkbox': {'license_key': 'ONLY-KEY'}})
    assert cfg.checkbox.license_key == 'ONLY-KEY'
    assert cfg.checkbox.endpoint is None
    assert cfg.checkbox.cashier_pin is None
