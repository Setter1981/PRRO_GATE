"""
Sprint 6 / step 1 — production crypto startup gate tests.

Coverage matrix:
  CG1 — dev mode + passthrough: initializes normally
  CG2 — production + config passthrough: startup fails with RuntimeError
  CG3 — production + injected PassthroughCryptoProvider: startup fails
  CG4 — production + sidecar config: initializes successfully
  CG5 — env-driven production mode applied correctly
  CG6 — test mode + passthrough: initializes normally (same as dev)
  CG7 — invalid environment value rejected by config validation
  CG8 — production + injected WritePathWorker(PassthroughCryptoProvider): startup fails
"""
from __future__ import annotations

import os
from pathlib import Path

import httpx
import pytest

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.providers import PassthroughCryptoProvider

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'


def _config(tmp_path: Path, db_name: str, *, environment: str = 'development', crypto_provider: str = 'passthrough', sidecar_url: str | None = None) -> AppConfig:
    crypto = {'provider': crypto_provider}
    if sidecar_url:
        crypto['sidecar_url'] = sidecar_url
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'crypto-gate-test',
        },
        'runtime': {
            'process_immediately': True,
            'environment': environment,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'CG-LIC',
            'cashier_pin': '0000',
        },
        'crypto': crypto,
    })


def _mock_transport(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-cg'})
    return httpx.Response(200, json={'id': 'cg-001', 'status': 'DONE'})


def _sidecar_mock(request: httpx.Request) -> httpx.Response:
    """Mock for both Checkbox transport AND sidecar crypto."""
    path = str(request.url)
    if 'checkbox' in path:
        return _mock_transport(request)
    # Sidecar sign endpoint
    return httpx.Response(200, json={'signed_payload': 'signed-by-sidecar', 'signature': 'sig-cg'})


# ---------------------------------------------------------------------------
# CG1 — dev mode + passthrough: initializes normally
# ---------------------------------------------------------------------------

def test_cg1_dev_passthrough_initializes(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'cg1.sqlite3', environment='development', crypto_provider='passthrough')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)))
    c.initialize()
    assert c.health.live is True
    assert c.health.phase == 'RUNNING'


# ---------------------------------------------------------------------------
# CG2 — production + config passthrough: startup fails
# ---------------------------------------------------------------------------

def test_cg2_production_passthrough_config_fails(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'cg2.sqlite3', environment='production', crypto_provider='passthrough')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)))
    with pytest.raises(RuntimeError, match='PassthroughCryptoProvider is not allowed'):
        c.initialize()
    assert c.health.live is False
    assert c.health.phase == 'STARTUP_ERROR'


# ---------------------------------------------------------------------------
# CG3 — production + injected PassthroughCryptoProvider: startup fails
# ---------------------------------------------------------------------------

def test_cg3_production_injected_passthrough_fails(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'cg3.sqlite3', environment='production', crypto_provider='sidecar', sidecar_url='http://sidecar:8080')
    c = RuntimeContainer(
        cfg,
        crypto_provider=PassthroughCryptoProvider(),  # injection overrides config
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
    )
    with pytest.raises(RuntimeError, match='PassthroughCryptoProvider is not allowed'):
        c.initialize()
    assert c.health.live is False


# ---------------------------------------------------------------------------
# CG4 — production + sidecar: initializes successfully
# ---------------------------------------------------------------------------

def test_cg4_production_sidecar_initializes(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'cg4.sqlite3', environment='production', crypto_provider='sidecar', sidecar_url='http://sidecar:8080')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    # Deactivate DPS stub profiles so transport gate passes
    from prro_gateway.migrations.runner import apply_migrations_to_connection
    with c.connect() as conn:
        apply_migrations_to_connection(conn, Path(ROOT / 'sql'))
        conn.execute("UPDATE transport_profiles SET is_active = 0 WHERE kind IN ('DPS_PRRO_GRPC_ECABINET', 'DPS_PRRO_XML_UNIFIED_WINDOW')")
        conn.commit()
    c.initialize()
    assert c.health.live is True
    assert c.health.phase == 'RUNNING'


# ---------------------------------------------------------------------------
# CG5 — env-driven production mode
# ---------------------------------------------------------------------------

def test_cg5_env_driven_production_mode(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv('PRRO_RUNTIME_ENVIRONMENT', 'production')
    monkeypatch.setenv('PRRO_GATEWAY_CONFIG', str(tmp_path / 'cg5.yaml'))

    # Write minimal config file (env override will set environment=production)
    (tmp_path / 'cg5.yaml').write_text(
        f"""
database:
  db_path: {tmp_path / 'cg5.sqlite3'}
  sql_dir: {ROOT / 'sql'}
  auto_migrate: true
defaults:
  fiscal_number: {FISCAL_NUMBER}
  backend_profile_id: backend_checkbox_default
  transport_profile_id: transport_checkbox_rest_default
  channel_owner: cg5
runtime:
  process_immediately: true
checkbox:
  endpoint: https://api.checkbox.mock/api/v1
  license_key: CG5-LIC
  cashier_pin: '0000'
crypto:
  provider: passthrough
"""
    )

    cfg = AppConfig.from_env()
    assert cfg.runtime.environment == 'production', f'Env override must set production, got {cfg.runtime.environment}'

    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)))
    with pytest.raises(RuntimeError, match='PassthroughCryptoProvider is not allowed'):
        c.initialize()


# ---------------------------------------------------------------------------
# CG6 — test mode + passthrough: initializes normally
# ---------------------------------------------------------------------------

def test_cg6_test_mode_passthrough_initializes(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'cg6.sqlite3', environment='test', crypto_provider='passthrough')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)))
    c.initialize()
    assert c.health.live is True


# ---------------------------------------------------------------------------
# CG7 — invalid environment value rejected
# ---------------------------------------------------------------------------

def test_cg7_invalid_environment_rejected() -> None:
    from pydantic import ValidationError
    with pytest.raises(ValidationError):
        AppConfig.from_mapping({
            'database': {'db_path': '/tmp/cg7.sqlite3', 'sql_dir': '/tmp'},
            'runtime': {'environment': 'staging'},
        })


# ---------------------------------------------------------------------------
# CG8 — production + injected WritePathWorker(PassthroughCryptoProvider): fails
# ---------------------------------------------------------------------------

def test_cg8_production_injected_worker_passthrough_fails(tmp_path: Path) -> None:
    """Injecting a prebuilt WritePathWorker carrying PassthroughCryptoProvider
    must still be caught by the production crypto gate."""
    from prro_gateway.services.write_path import WritePathWorker

    cfg = _config(tmp_path, 'cg8.sqlite3', environment='production', crypto_provider='sidecar', sidecar_url='http://sidecar:8080')

    class _StubTransport:
        def send(self, **kw): pass

    worker = WritePathWorker(
        crypto_provider=PassthroughCryptoProvider(),
        transport_client=_StubTransport(),
    )
    c = RuntimeContainer(
        cfg,
        command_processor=worker,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    with pytest.raises(RuntimeError, match='PassthroughCryptoProvider is not allowed'):
        c.initialize()
    assert c.health.live is False
    assert c.health.phase == 'STARTUP_ERROR'
