"""
Gate 3i — minimal crypto circuit breaker / temporary sign suppression.

WritePathWorker gains:
  - crypto_breaker_threshold: int = 0  (0 = disabled)
  - _crypto_consecutive_failures: int = 0

_stage_sign() logic:
  1. If breaker_threshold > 0 and consecutive_failures >= threshold:
     → immediate ERROR_RETRYABLE without calling provider (breaker open)
  2. On CryptoProviderUnavailableError or bare Exception: increment counter
  3. On success: reset counter to 0

CryptoConfig.breaker_threshold: int = 5 (default)
Container wires crypto_breaker_threshold=config.crypto.breaker_threshold.

Four tests:
  A — happy path: breaker_threshold=3, passthrough → ACK, counter stays 0
  B — breaker opens: threshold=2, counting-fail provider called exactly 2×,
      3rd attempt skips provider (breaker open), still ERROR_RETRYABLE
  C — doc state when breaker open: 200 response, ERROR_RETRYABLE, CRYPTO_PROVIDER_UNAVAILABLE,
      error_message contains 'circuit breaker'
  D — threshold=0 disables breaker: repeated failures keep calling provider every time
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
    """Always fails; counts how many times sign() was called."""
    def __init__(self):
        self.call_count = 0

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.call_count += 1
        raise CryptoProviderUnavailableError('test: always unavailable')


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _transport_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-gate3i'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3i-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3I-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate3i-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE3I-001',
            'updated_at': '2026-03-31T08:01:00+00:00',
        })
    raise AssertionError(f'gate3i transport: unexpected {request.method} {request.url}')


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
            'channel_owner': 'gate3i',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3I-LIC',
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
        'channel_owner': 'gate3i',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — happy path: breaker_threshold=3, passthrough → counter stays 0
# ---------------------------------------------------------------------------

def test_gate3i_happy_path_counter_stays_zero(tmp_path: Path) -> None:
    """
    With breaker_threshold=3 and passthrough provider, SHIFT_OPEN + SELL reach ACK.
    Consecutive failure counter stays at 0 (no failures).
    """
    cfg = _config(tmp_path, 'gate3i_a.sqlite3', extra={'crypto': {'breaker_threshold': 3}})
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3i-open-a'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3i-a', 'cashier_id': 'cashier-gate3i'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK', (
            f'Gate 3i: happy path must reach ACK: {r_shift.json()}'
        )
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate3i-sell-a'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate3i-a',
                'cashier_id': 'cashier-gate3i',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK', (
            f'Gate 3i: SELL happy path must reach ACK: {r_sell.json()}'
        )

    worker = c.command_processor
    assert worker._crypto_consecutive_failures == 0, (
        f'Gate 3i: consecutive_failures must stay 0 on success: got {worker._crypto_consecutive_failures}'
    )


# ---------------------------------------------------------------------------
# Test B — breaker opens: provider called exactly threshold times, not more
# ---------------------------------------------------------------------------

def test_gate3i_breaker_opens_after_threshold(tmp_path: Path) -> None:
    """
    With threshold=2 and CountingFailProvider:
    - Requests 1 and 2: provider.sign() called → consecutive_failures reaches 2 → breaker opens
    - Request 3: breaker is open → provider.sign() NOT called (total calls stays at 2)
    """
    threshold = 2
    provider = CountingFailProvider()
    cfg = _config(tmp_path, 'gate3i_b.sqlite3', extra={'crypto': {'breaker_threshold': threshold}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=provider,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        for i in range(threshold + 1):
            client.post('/v1/ingress/checkbox', json={
                'context': {**_ctx(f'gate3i-open-b-{i}'), 'business_ts': '2026-03-31T08:00:00Z'},
                'operation': 'SHIFT_OPEN',
                'request': {
                    'external_request_id': f'ext-open-gate3i-b-{i}',
                    'cashier_id': 'cashier-gate3i',
                },
            })

    assert provider.call_count == threshold, (
        f'Gate 3i: provider.sign() must be called exactly {threshold} times (threshold), '
        f'not on breaker-open attempt. Got call_count={provider.call_count}.'
    )
    worker = c.command_processor
    assert worker._crypto_consecutive_failures == threshold, (
        f'Gate 3i: consecutive_failures must equal threshold after breaker opens: '
        f'got {worker._crypto_consecutive_failures}'
    )


# ---------------------------------------------------------------------------
# Test C — doc state when breaker is open: 200, ERROR_RETRYABLE, 'circuit breaker' message
# ---------------------------------------------------------------------------

def test_gate3i_breaker_open_gives_error_retryable_with_message(tmp_path: Path) -> None:
    """
    After breaker opens (threshold=1: first failure opens it),
    the second request returns:
      - HTTP 200
      - document_state=ERROR_RETRYABLE
      - canonical_error_code=CRYPTO_PROVIDER_UNAVAILABLE
      - error_message contains 'circuit breaker'
    Doc is not stuck in PREPARED.
    """
    provider = CountingFailProvider()
    cfg = _config(tmp_path, 'gate3i_c.sqlite3', extra={'crypto': {'breaker_threshold': 1}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=provider,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        # First request: failure → counter=1 → breaker opens (threshold=1)
        client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3i-open-c-1'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3i-c-1', 'cashier_id': 'cashier-gate3i'},
        })

        # Second request: breaker open → provider NOT called
        r2 = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3i-open-c-2'), 'business_ts': '2026-03-31T08:00:01Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3i-c-2', 'cashier_id': 'cashier-gate3i'},
        })

    assert r2.status_code == 200, (
        f'Gate 3i: breaker-open response must be 200: got {r2.status_code}. {r2.text}'
    )
    body = r2.json()
    # C1 fast-path guard: CRYPTO_DEGRADED rejects before document creation,
    # so document_state is None (no LND consumed — correct behavior).
    assert body.get('document_state') is None, (
        f'Gate 3i: CRYPTO_DEGRADED fast-path must not create a document: got {body.get("document_state")!r}'
    )
    # C1 fast-path: no document created → response uses 'error_code' (not 'canonical_error_code')
    assert body.get('error_code') == 'CRYPTO_PROVIDER_UNAVAILABLE', (
        f'Gate 3i: error_code must be CRYPTO_PROVIDER_UNAVAILABLE: '
        f'got {body.get("error_code")!r}'
    )
    assert 'circuit breaker' in (body.get('error_message') or ''), (
        f'Gate 3i: error_message must mention circuit breaker: got {body.get("error_message")!r}'
    )

    # provider called exactly once (first request); second skipped by breaker
    assert provider.call_count == 1, (
        f'Gate 3i: provider.sign() must be called exactly once (breaker skips 2nd): '
        f'got {provider.call_count}'
    )

    # C1 fast-path: second request has no document (no LND consumed).
    assert body.get('document_id') is None, (
        f'Gate 3i: CRYPTO_DEGRADED fast-path must not set document_id: got {body.get("document_id")!r}'
    )
    with c.connect() as conn:
        prepared = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE state='PREPARED'"
        ).fetchall()
    assert len(prepared) == 0, f'Gate 3i: no doc must remain in PREPARED: {prepared}'


# ---------------------------------------------------------------------------
# Test D — threshold=0 disables breaker: provider called on every attempt
# ---------------------------------------------------------------------------

def test_gate3i_threshold_zero_disables_breaker(tmp_path: Path) -> None:
    """
    crypto.breaker_threshold=0 disables the circuit breaker entirely.
    Even after many consecutive failures, provider.sign() is called on every attempt.
    """
    call_limit = 4
    provider = CountingFailProvider()
    cfg = _config(tmp_path, 'gate3i_d.sqlite3', extra={'crypto': {'breaker_threshold': 0}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=provider,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        for i in range(call_limit):
            client.post('/v1/ingress/checkbox', json={
                'context': {**_ctx(f'gate3i-open-d-{i}'), 'business_ts': '2026-03-31T08:00:00Z'},
                'operation': 'SHIFT_OPEN',
                'request': {
                    'external_request_id': f'ext-open-gate3i-d-{i}',
                    'cashier_id': 'cashier-gate3i',
                },
            })

    assert provider.call_count == call_limit, (
        f'Gate 3i: with threshold=0, provider must be called on every attempt ({call_limit}×): '
        f'got call_count={provider.call_count}'
    )
