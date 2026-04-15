"""
Gate 3c — crypto provider config seam.

Adds config.crypto.provider to AppConfig (CryptoConfig model).
RuntimeContainer._resolve_crypto_provider() selects the provider at startup:
  - constructor injection (crypto_provider=...) takes priority over config
  - config.crypto.provider='passthrough' → PassthroughCryptoProvider (default)
  - config.crypto.provider=<unknown>    → ValueError at _wire_runtime_services() time

Four tests:
  A — default config (no crypto section): passthrough used, doc reaches ACK
  B — explicit crypto.provider='passthrough': same behavior as A
  C — unknown provider name ('sidecar'): ValueError raised at initialize()
  D — constructor injection spy overrides config: spy used regardless of config
"""
from __future__ import annotations

from pathlib import Path

import httpx
import pytest
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.repositories import FiscalDocumentRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# Inline spy
# ---------------------------------------------------------------------------

class SpyCryptoProvider:
    def __init__(self):
        self.calls: list[dict] = []

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.calls.append({'document_id': document_id, 'payload_json': payload_json})
        return payload_json


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _transport_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-gate3c'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3c-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3C-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate3c-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE3C-001',
            'updated_at': '2026-03-31T08:01:00+00:00',
        })
    raise AssertionError(f'gate3c transport: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str, extra: dict | None = None) -> AppConfig:
    base: dict = {
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate3c',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3C-LIC',
            'cashier_pin': '0000',
        },
    }
    if extra:
        base.update(extra)
    return AppConfig.from_mapping(base)


def _ctx(req_id: str, ts: str = '2026-03-31T08:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate3c',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — default config (no crypto section): passthrough, doc reaches ACK
# ---------------------------------------------------------------------------

def test_gate3c_default_config_uses_passthrough(tmp_path: Path) -> None:
    """
    When no crypto section is present in config, provider defaults to 'passthrough'.
    PassthroughCryptoProvider returns payload unchanged.
    SHIFT_OPEN + SELL both reach ACK.
    """
    cfg = _config(tmp_path, 'gate3c_a.sqlite3')
    assert cfg.crypto.provider == 'passthrough', (
        f'Default crypto.provider must be passthrough, got {cfg.crypto.provider!r}'
    )
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3c-open-a'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3c-a', 'cashier_id': 'cashier-gate3c'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN must reach ACK with default passthrough: {r_shift.json()}'
        )

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate3c-sell-a'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate3c-a',
                'cashier_id': 'cashier-gate3c',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK', (
            f'SELL must reach ACK with default passthrough: {r_sell.json()}'
        )


# ---------------------------------------------------------------------------
# Test B — explicit crypto.provider='passthrough': same as default
# ---------------------------------------------------------------------------

def test_gate3c_explicit_passthrough_config(tmp_path: Path) -> None:
    """
    config.crypto.provider='passthrough' explicitly behaves identically to default.
    """
    cfg = _config(tmp_path, 'gate3c_b.sqlite3', extra={'crypto': {'provider': 'passthrough'}})
    assert cfg.crypto.provider == 'passthrough'
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3c-open-b'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3c-b', 'cashier_id': 'cashier-gate3c'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN must reach ACK with explicit passthrough: {r_shift.json()}'
        )


# ---------------------------------------------------------------------------
# Test C — unknown provider name raises ValueError at initialize()
# ---------------------------------------------------------------------------

def test_gate3c_unknown_provider_raises_at_initialize(tmp_path: Path) -> None:
    """
    config.crypto.provider=<unknown name> raises ValueError during container.initialize()
    → _wire_runtime_services() → _resolve_crypto_provider().
    This is a fail-fast behavior: misconfigured startup is immediately visible.
    Note: 'sidecar' is now a supported provider (Gate 3f); this test uses a truly unknown name.
    """
    cfg = _config(tmp_path, 'gate3c_c.sqlite3', extra={'crypto': {'provider': 'unknown_provider_xyz'}})
    assert cfg.crypto.provider == 'unknown_provider_xyz'
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with pytest.raises(ValueError, match="Unsupported crypto provider"):
        with TestClient(create_app(c)):
            pass


# ---------------------------------------------------------------------------
# Test D — constructor injection spy overrides config
# ---------------------------------------------------------------------------

def test_gate3c_constructor_injection_overrides_config(tmp_path: Path) -> None:
    """
    When crypto_provider= is explicitly passed to RuntimeContainer,
    it takes priority over config.crypto.provider.
    Even with config.crypto.provider='passthrough', the injected spy is used.
    This preserves existing test-override behavior.
    """
    spy = SpyCryptoProvider()
    cfg = _config(tmp_path, 'gate3c_d.sqlite3', extra={'crypto': {'provider': 'passthrough'}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=spy,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )
    sell_doc_id: str | None = None

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3c-open-d'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3c-d', 'cashier_id': 'cashier-gate3c'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate3c-sell-d'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate3c-d',
                'cashier_id': 'cashier-gate3c',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    assert len(spy.calls) >= 1, (
        f'Injected spy must be used when constructor injection is provided. '
        f'Got {len(spy.calls)} calls.'
    )
    call_ids = [c['document_id'] for c in spy.calls]
    assert sell_doc_id in call_ids, (
        f'sign() must be called with SELL document_id ({sell_doc_id}). '
        f'Actual: {call_ids}'
    )
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc.state == DocumentState.ACK
