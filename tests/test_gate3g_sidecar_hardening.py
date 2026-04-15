"""
Gate 3g — sidecar client hardening + crypto env overrides.

Changes:
  - CryptoConfig: sidecar_connect_timeout=5.0, sidecar_read_timeout=10.0
  - SidecarCryptoClient: connect_timeout/read_timeout params;
    default httpx.Client uses httpx.Timeout(timeout=read, connect=connect)
  - _apply_env_overrides: PRRO_CRYPTO_PROVIDER, PRRO_CRYPTO_SIDECAR_URL,
    PRRO_CRYPTO_SIDECAR_CONNECT_TIMEOUT, PRRO_CRYPTO_SIDECAR_READ_TIMEOUT
  - _resolve_crypto_provider: passes connect/read timeout to SidecarCryptoClient

Four tests:
  A — env overrides for provider + URL applied to AppConfig (unit test on _apply_env_overrides)
  B — timeout params reach SidecarCryptoClient's default httpx.Client
  C — end-to-end: env-configured sidecar (via _apply_env_overrides) + mock HTTP → doc ACK
  D — env provider=sidecar without sidecar_url → ValueError at initialize()
"""
from __future__ import annotations

import os
from pathlib import Path

import httpx
import pytest
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.repositories import FiscalDocumentRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.providers import SidecarCryptoClient, SidecarCryptoProvider
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'
SIDECAR_URL = 'https://crypto.sidecar.mock'


# ---------------------------------------------------------------------------
# Mocks
# ---------------------------------------------------------------------------

def _sidecar_ok_mock(request: httpx.Request) -> httpx.Response:
    if request.url.path == '/sign':
        return httpx.Response(200, json={'signed_payload': 'ENV-SIGNED'})
    raise AssertionError(f'gate3g sidecar: unexpected {request.method} {request.url}')


def _checkbox_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-gate3g'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3g-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3G-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    raise AssertionError(f'gate3g checkbox: unexpected {request.method} {request.url}')


# ---------------------------------------------------------------------------
# Config helpers
# ---------------------------------------------------------------------------

def _base_config(tmp_path: Path, db_name: str) -> AppConfig:
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
            'channel_owner': 'gate3g',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3G-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-31T08:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate3g',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — env overrides applied to AppConfig (unit test, no HTTP)
# ---------------------------------------------------------------------------

def test_gate3g_env_overrides_provider_and_url(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    PRRO_CRYPTO_PROVIDER and PRRO_CRYPTO_SIDECAR_URL env vars are applied via
    _apply_env_overrides(). Starting from default config (passthrough, no URL),
    env sets provider=sidecar and URL.
    """
    monkeypatch.setenv('PRRO_CRYPTO_PROVIDER', 'sidecar')
    monkeypatch.setenv('PRRO_CRYPTO_SIDECAR_URL', SIDECAR_URL)
    monkeypatch.setenv('PRRO_CRYPTO_SIDECAR_CONNECT_TIMEOUT', '3.0')
    monkeypatch.setenv('PRRO_CRYPTO_SIDECAR_READ_TIMEOUT', '7.5')

    base = AppConfig()
    assert base.crypto.provider == 'passthrough'
    assert base.crypto.sidecar_url is None

    cfg = AppConfig._apply_env_overrides(base)

    assert cfg.crypto.provider == 'sidecar', (
        f'Gate 3g: PRRO_CRYPTO_PROVIDER must override provider: got {cfg.crypto.provider!r}'
    )
    assert cfg.crypto.sidecar_url == SIDECAR_URL, (
        f'Gate 3g: PRRO_CRYPTO_SIDECAR_URL must override sidecar_url: got {cfg.crypto.sidecar_url!r}'
    )
    assert cfg.crypto.sidecar_connect_timeout == 3.0, (
        f'Gate 3g: PRRO_CRYPTO_SIDECAR_CONNECT_TIMEOUT must override: got {cfg.crypto.sidecar_connect_timeout!r}'
    )
    assert cfg.crypto.sidecar_read_timeout == 7.5, (
        f'Gate 3g: PRRO_CRYPTO_SIDECAR_READ_TIMEOUT must override: got {cfg.crypto.sidecar_read_timeout!r}'
    )


# ---------------------------------------------------------------------------
# Test B — timeout params reach SidecarCryptoClient's default httpx.Client
# ---------------------------------------------------------------------------

def test_gate3g_timeout_params_reach_default_http_client() -> None:
    """
    SidecarCryptoClient(connect_timeout=3.0, read_timeout=7.5) with no injected http_client
    creates httpx.Client with httpx.Timeout(timeout=7.5, connect=3.0).
    Verifies timeout config is wired into the underlying HTTP client.
    """
    client = SidecarCryptoClient(
        base_url=SIDECAR_URL,
        http_client=None,
        connect_timeout=3.0,
        read_timeout=7.5,
    )
    t = client._http.timeout
    assert t.connect == 3.0, (
        f'Gate 3g: connect_timeout must reach httpx.Client.timeout.connect: got {t.connect!r}'
    )
    assert t.read == 7.5, (
        f'Gate 3g: read_timeout must reach httpx.Client.timeout.read: got {t.read!r}'
    )


# ---------------------------------------------------------------------------
# Test C — end-to-end: env-configured sidecar → doc reaches ACK
# ---------------------------------------------------------------------------

def test_gate3g_env_configured_sidecar_reaches_ack(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """
    Full end-to-end: env vars set provider=sidecar + sidecar_url.
    AppConfig._apply_env_overrides() applied; RuntimeContainer resolves SidecarCryptoProvider.
    Mock sidecar returns signed payload; SHIFT_OPEN reaches ACK.
    """
    monkeypatch.setenv('PRRO_CRYPTO_PROVIDER', 'sidecar')
    monkeypatch.setenv('PRRO_CRYPTO_SIDECAR_URL', SIDECAR_URL)

    base = _base_config(tmp_path, 'gate3g_c.sqlite3')
    cfg = AppConfig._apply_env_overrides(base)
    assert cfg.crypto.provider == 'sidecar'
    assert cfg.crypto.sidecar_url == SIDECAR_URL

    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_checkbox_mock)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_ok_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3g-open-c'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3g-c', 'cashier_id': 'cashier-gate3g'},
        })

    assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK', (
        f'Gate 3g: env-configured sidecar must allow doc to reach ACK: {r_shift.json()}'
    )
    assert isinstance(c.command_processor.crypto_provider, SidecarCryptoProvider), (
        f'Gate 3g: resolved provider must be SidecarCryptoProvider: '
        f'got {type(c.command_processor.crypto_provider)}'
    )


# ---------------------------------------------------------------------------
# Test D — env provider=sidecar without sidecar_url → ValueError at initialize()
# ---------------------------------------------------------------------------

def test_gate3g_env_sidecar_without_url_raises(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """
    PRRO_CRYPTO_PROVIDER=sidecar without PRRO_CRYPTO_SIDECAR_URL (and no sidecar_url in config)
    → ValueError at container.initialize() → fail-fast, no silent misconfiguration.
    """
    monkeypatch.setenv('PRRO_CRYPTO_PROVIDER', 'sidecar')
    monkeypatch.delenv('PRRO_CRYPTO_SIDECAR_URL', raising=False)

    base = _base_config(tmp_path, 'gate3g_d.sqlite3')
    cfg = AppConfig._apply_env_overrides(base)
    assert cfg.crypto.provider == 'sidecar'
    assert cfg.crypto.sidecar_url is None

    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_checkbox_mock)),
    )

    with pytest.raises(ValueError, match="sidecar_url"):
        with TestClient(create_app(c)):
            pass
