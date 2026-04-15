"""
Gate 3j — crypto breaker reset admin path.

POST /v1/admin/crypto/reset-breaker:
  - Resets WritePathWorker._crypto_consecutive_failures to 0
  - Returns previous value, new value (0), breaker_was_open flag, threshold
  - Logs crypto_breaker_reset
  - Returns 409 if command_processor is not WritePathWorker

WritePathWorker gains:
  - crypto_breaker_open: bool property
  - reset_crypto_breaker() -> int (returns previous value)

Four tests:
  A — breaker open → reset → returns correct previous/new values, breaker_was_open=True
  B — after reset, next sign attempt reaches provider (not short-circuited)
  C — reset when breaker not open: no-op, breaker_was_open=False, returns 0→0
  D — endpoint unavailable when command_processor is not WritePathWorker → 409
"""
from __future__ import annotations

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

class CountingFailProvider:
    def __init__(self):
        self.call_count = 0

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.call_count += 1
        raise CryptoProviderUnavailableError('test: always unavailable')


class CountingPassProvider:
    """Counts calls and always succeeds (passthrough)."""
    def __init__(self):
        self.call_count = 0

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.call_count += 1
        return payload_json


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _transport_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-gate3j'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3j-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3J-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate3j-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE3J-001',
            'updated_at': '2026-03-31T08:01:00+00:00',
        })
    raise AssertionError(f'gate3j transport: unexpected {request.method} {request.url}')


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
            'channel_owner': 'gate3j',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3J-LIC',
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
        'channel_owner': 'gate3j',
        'business_ts': ts,
    }


def _open_shift(client, suffix: str) -> None:
    r = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx(f'gate3j-open-{suffix}'), 'business_ts': '2026-03-31T08:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-gate3j-{suffix}',
            'cashier_id': 'cashier-gate3j',
        },
    })
    return r


# ---------------------------------------------------------------------------
# Test A — breaker open → reset → correct response values
# ---------------------------------------------------------------------------

def test_gate3j_reset_open_breaker_returns_correct_values(tmp_path: Path) -> None:
    """
    After threshold=2 failures, breaker is open (consecutive_failures=2).
    POST /v1/admin/crypto/reset-breaker returns:
      - previous_consecutive_failures=2
      - consecutive_failures=0
      - breaker_was_open=True
      - threshold=2
    Worker state is reset.
    """
    threshold = 2
    cfg = _config(tmp_path, 'gate3j_a.sqlite3', extra={'crypto': {'breaker_threshold': threshold}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=CountingFailProvider(),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        # Open the breaker
        for i in range(threshold):
            _open_shift(client, f'a-fail-{i}')

        worker = c.command_processor
        assert worker.crypto_breaker_open, 'Breaker must be open before reset'

        r = client.post('/v1/admin/crypto/reset-breaker')

    assert r.status_code == 200, f'Gate 3j: reset must return 200: {r.text}'
    body = r.json()
    assert body['previous_consecutive_failures'] == threshold, (
        f'Gate 3j: previous must equal threshold ({threshold}): got {body["previous_consecutive_failures"]}'
    )
    assert body['consecutive_failures'] == 0, (
        f'Gate 3j: consecutive_failures after reset must be 0: got {body["consecutive_failures"]}'
    )
    assert body['breaker_was_open'] is True, (
        f'Gate 3j: breaker_was_open must be True: got {body["breaker_was_open"]}'
    )
    assert body['threshold'] == threshold, (
        f'Gate 3j: threshold in response must match config: got {body["threshold"]}'
    )
    assert worker._crypto_consecutive_failures == 0, (
        f'Gate 3j: worker counter must be 0 after reset: got {worker._crypto_consecutive_failures}'
    )
    assert not worker.crypto_breaker_open, 'Gate 3j: breaker must be closed after reset'


# ---------------------------------------------------------------------------
# Test B — after reset, next sign attempt reaches provider
# ---------------------------------------------------------------------------

def test_gate3j_after_reset_provider_called_again(tmp_path: Path) -> None:
    """
    After breaker reset, the next request actually calls provider.sign() again
    (not short-circuited by the breaker). Verifies recovery is functional.
    Uses CountingPassProvider: after reset it switches to success path.
    """
    threshold = 2
    fail_provider = CountingFailProvider()

    cfg = _config(tmp_path, 'gate3j_b.sqlite3', extra={'crypto': {'breaker_threshold': threshold}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=fail_provider,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        # Open breaker
        for i in range(threshold):
            _open_shift(client, f'b-fail-{i}')
        assert c.command_processor.crypto_breaker_open

        # Reset
        r_reset = client.post('/v1/admin/crypto/reset-breaker')
        assert r_reset.status_code == 200

        # After reset: next attempt must call provider again (not be blocked by breaker)
        calls_before = fail_provider.call_count
        _open_shift(client, 'b-after-reset')
        calls_after = fail_provider.call_count

    assert calls_after == calls_before + 1, (
        f'Gate 3j: after reset, provider.sign() must be called on next attempt. '
        f'calls_before={calls_before}, calls_after={calls_after}'
    )


# ---------------------------------------------------------------------------
# Test C — reset when breaker not open: no-op, breaker_was_open=False
# ---------------------------------------------------------------------------

def test_gate3j_reset_when_not_open_is_noop(tmp_path: Path) -> None:
    """
    POST /v1/admin/crypto/reset-breaker when no failures occurred:
      - HTTP 200
      - previous_consecutive_failures=0
      - breaker_was_open=False
    Worker state unchanged (still 0).
    """
    cfg = _config(tmp_path, 'gate3j_c.sqlite3', extra={'crypto': {'breaker_threshold': 3}})
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r = client.post('/v1/admin/crypto/reset-breaker')

    assert r.status_code == 200
    body = r.json()
    assert body['previous_consecutive_failures'] == 0
    assert body['consecutive_failures'] == 0
    assert body['breaker_was_open'] is False, (
        f'Gate 3j: breaker_was_open must be False when no failures: got {body["breaker_was_open"]}'
    )


# ---------------------------------------------------------------------------
# Test D — 409 when command_processor is not WritePathWorker
# ---------------------------------------------------------------------------

def test_gate3j_reset_unavailable_for_non_worker_processor(tmp_path: Path) -> None:
    """
    When RuntimeContainer is constructed with a custom command_processor (not WritePathWorker),
    POST /v1/admin/crypto/reset-breaker returns 409.
    """
    class CustomProcessor:
        pass

    cfg = _config(tmp_path, 'gate3j_d.sqlite3')
    c = RuntimeContainer(
        cfg,
        command_processor=CustomProcessor(),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r = client.post('/v1/admin/crypto/reset-breaker')

    assert r.status_code == 409, (
        f'Gate 3j: reset must return 409 for non-WritePathWorker processor: got {r.status_code}'
    )
    assert 'not a WritePathWorker' in r.json().get('detail', ''), (
        f'Gate 3j: 409 detail must mention WritePathWorker: got {r.json()}'
    )
