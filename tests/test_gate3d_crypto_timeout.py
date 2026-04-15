"""
Gate 3d — crypto timeout / degraded-mode scaffolding.

Adds config.crypto.timeout_seconds (float | None, default 5.0).
WritePathWorker receives crypto_timeout_seconds; when set, sign() is run
in a thread and CryptoProviderUnavailableError is raised on timeout.

Four tests:
  A — default config (timeout=5.0s): passthrough completes normally, doc reaches ACK
  B — slow provider (sleeps > timeout): timeout fires → ERROR_RETRYABLE,
      canonical_error_code=CRYPTO_PROVIDER_UNAVAILABLE, doc not stuck in PREPARED
  C — timeout=None, slow provider (short sleep): no timeout applied, completes normally → ACK
  D — spy provider + explicit timeout_seconds via config: spy called, doc reaches ACK
"""
from __future__ import annotations

import time
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.ports import CryptoProviderUnavailableError
from prro_gateway.repositories import FiscalDocumentRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# Test providers
# ---------------------------------------------------------------------------

class SlowCryptoProvider:
    """Sleeps for `delay_seconds` before returning. Simulates a hanging sidecar."""
    def __init__(self, delay_seconds: float):
        self.delay_seconds = delay_seconds

    def sign(self, *, document_id: str, payload_json: str) -> str:
        time.sleep(self.delay_seconds)
        return payload_json


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
        return httpx.Response(200, json={'access_token': 'mock-gate3d'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3d-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3D-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate3d-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE3D-001',
            'updated_at': '2026-03-31T08:01:00+00:00',
        })
    raise AssertionError(f'gate3d transport: unexpected {request.method} {request.url}')


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
            'channel_owner': 'gate3d',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3D-LIC',
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
        'channel_owner': 'gate3d',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — default config (timeout=5.0s): passthrough, doc reaches ACK
# ---------------------------------------------------------------------------

def test_gate3d_default_timeout_passthrough_reaches_ack(tmp_path: Path) -> None:
    """
    Default config includes crypto.timeout_seconds=5.0.
    PassthroughCryptoProvider completes instantly — well within timeout.
    Doc must reach ACK; no ERROR_RETRYABLE.
    """
    cfg = _config(tmp_path, 'gate3d_a.sqlite3')
    assert cfg.crypto.timeout_seconds == 5.0, (
        f'Default timeout_seconds must be 5.0, got {cfg.crypto.timeout_seconds!r}'
    )
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3d-open-a'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3d-a', 'cashier_id': 'cashier-gate3d'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN must reach ACK with default timeout passthrough: {r_shift.json()}'
        )

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate3d-sell-a'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate3d-a',
                'cashier_id': 'cashier-gate3d',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK', (
            f'SELL must reach ACK with default timeout passthrough: {r_sell.json()}'
        )


# ---------------------------------------------------------------------------
# Test B — slow provider exceeds timeout → ERROR_RETRYABLE
# ---------------------------------------------------------------------------

def test_gate3d_slow_provider_timeout_gives_error_retryable(tmp_path: Path) -> None:
    """
    SlowCryptoProvider sleeps 0.3s; crypto.timeout_seconds=0.05s.
    sign() times out → CryptoProviderUnavailableError raised internally →
    doc transitions to ERROR_RETRYABLE + canonical_error_code=CRYPTO_PROVIDER_UNAVAILABLE.
    HTTP response is 200 (not 400/500). Doc not stuck in PREPARED.
    """
    cfg = _config(tmp_path, 'gate3d_b.sqlite3', extra={'crypto': {'provider': 'passthrough', 'timeout_seconds': 0.05}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=SlowCryptoProvider(delay_seconds=0.3),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3d-open-b'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3d-b', 'cashier_id': 'cashier-gate3d'},
        })

    assert r_shift.status_code == 200, (
        f'Gate 3d: timeout must be handled gracefully (200), got {r_shift.status_code}. '
        f'Response: {r_shift.text}'
    )
    body = r_shift.json()
    assert body.get('document_state') == 'ERROR_RETRYABLE', (
        f'Gate 3d: doc must be ERROR_RETRYABLE after timeout: '
        f'got document_state={body.get("document_state")!r}'
    )
    assert body.get('canonical_error_code') == 'CRYPTO_PROVIDER_UNAVAILABLE', (
        f'Gate 3d: canonical_error_code must be CRYPTO_PROVIDER_UNAVAILABLE after timeout: '
        f'got {body.get("canonical_error_code")!r}'
    )

    doc_id = body.get('document_id')
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
        prepared_docs = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE state='PREPARED'"
        ).fetchall()

    assert doc.state == DocumentState.ERROR_RETRYABLE
    assert doc.canonical_error_code == 'CRYPTO_PROVIDER_UNAVAILABLE'
    assert len(prepared_docs) == 0, (
        f'Gate 3d: no doc must remain in PREPARED after timeout: {prepared_docs}'
    )


# ---------------------------------------------------------------------------
# Test C — timeout=None: slow provider completes without timeout enforcement
# ---------------------------------------------------------------------------

def test_gate3d_timeout_none_skips_thread_wrapper(tmp_path: Path) -> None:
    """
    crypto.timeout_seconds=null disables the thread wrapper entirely.
    SlowCryptoProvider(0.05s) completes within the test — doc reaches ACK.
    Verifies that timeout=None code path is correct (direct call, no ThreadPoolExecutor).
    """
    cfg = _config(tmp_path, 'gate3d_c.sqlite3', extra={'crypto': {'provider': 'passthrough', 'timeout_seconds': None}})
    assert cfg.crypto.timeout_seconds is None
    c = RuntimeContainer(
        cfg,
        crypto_provider=SlowCryptoProvider(delay_seconds=0.05),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3d-open-c'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3d-c', 'cashier_id': 'cashier-gate3d'},
        })

    assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK', (
        f'Gate 3d: with timeout=None, slow provider (0.05s) must complete and reach ACK: '
        f'{r_shift.json()}'
    )


# ---------------------------------------------------------------------------
# Test D — spy provider + explicit timeout_seconds via config → ACK
# ---------------------------------------------------------------------------

def test_gate3d_spy_with_timeout_reaches_ack(tmp_path: Path) -> None:
    """
    SpyCryptoProvider (instant) + crypto.timeout_seconds=2.0.
    sign() completes well within timeout; spy is called; doc reaches ACK.
    Verifies that normal fast providers are not affected by timeout scaffolding.
    """
    spy = SpyCryptoProvider()
    cfg = _config(tmp_path, 'gate3d_d.sqlite3', extra={'crypto': {'provider': 'passthrough', 'timeout_seconds': 2.0}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=spy,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )
    sell_doc_id: str | None = None

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3d-open-d'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3d-d', 'cashier_id': 'cashier-gate3d'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate3d-sell-d'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate3d-d',
                'cashier_id': 'cashier-gate3d',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    assert len(spy.calls) >= 1, (
        f'Gate 3d: spy must be called when used with timeout scaffolding. '
        f'Got {len(spy.calls)} calls.'
    )
    call_ids = [call['document_id'] for call in spy.calls]
    assert sell_doc_id in call_ids, (
        f'sign() must be called with SELL document_id. Actual: {call_ids}'
    )
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc.state == DocumentState.ACK
