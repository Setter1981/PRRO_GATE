"""
Gate 3k — crypto breaker state in /v1/ops/summary.

ops/summary now includes:
  "crypto_breaker_open": bool
  "crypto_consecutive_failures": int
  "crypto_breaker_threshold": int

Read directly from WritePathWorker instance; safe fallback (all zeros/false)
when command_processor is not a WritePathWorker.

Four tests:
  A — healthy state: threshold=3, no failures → open=False, failures=0, threshold=3
  B — open breaker: after 3 failures → open=True, failures=3
  C — after admin reset: open=False, failures=0
  D — existing ops summary fields still present alongside crypto fields
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.ports import CryptoProviderUnavailableError
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


class CountingFailProvider:
    def __init__(self):
        self.call_count = 0

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.call_count += 1
        raise CryptoProviderUnavailableError('test: unavailable')


def _transport_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-gate3k'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3k-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3K-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    raise AssertionError(f'gate3k transport: unexpected {request.method} {request.url}')


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
            'channel_owner': 'gate3k',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3K-LIC',
            'cashier_pin': '0000',
        },
    }
    if extra:
        base.update(extra)
    return AppConfig.from_mapping(base)


def _ctx(req_id: str, ts: str = '2026-03-31T08:00:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate3k',
        'business_ts': ts,
    }


def _open_shift(client, suffix: str):
    return client.post('/v1/ingress/checkbox', json={
        'context': _ctx(f'gate3k-open-{suffix}'),
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-gate3k-{suffix}',
            'cashier_id': 'cashier-gate3k',
        },
    })


# ---------------------------------------------------------------------------
# Test A — healthy state: no failures → correct zero values
# ---------------------------------------------------------------------------

def test_gate3k_summary_healthy_crypto_state(tmp_path: Path) -> None:
    """ops/summary: no failures → open=False, failures=0, threshold from config."""
    threshold = 3
    cfg = _config(tmp_path, 'gate3k_a.sqlite3', extra={'crypto': {'breaker_threshold': threshold}})
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['crypto_breaker_open'] is False, (
        f'Gate 3k: healthy state must have open=False: {body["crypto_breaker_open"]!r}'
    )
    assert body['crypto_consecutive_failures'] == 0, (
        f'Gate 3k: healthy state must have failures=0: {body["crypto_consecutive_failures"]!r}'
    )
    assert body['crypto_breaker_threshold'] == threshold, (
        f'Gate 3k: threshold must match config: got {body["crypto_breaker_threshold"]!r}'
    )


# ---------------------------------------------------------------------------
# Test B — open breaker: after threshold failures → open=True
# ---------------------------------------------------------------------------

def test_gate3k_summary_shows_open_breaker(tmp_path: Path) -> None:
    """ops/summary reflects open breaker after consecutive failures reach threshold."""
    threshold = 2
    cfg = _config(tmp_path, 'gate3k_b.sqlite3', extra={'crypto': {'breaker_threshold': threshold}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=CountingFailProvider(),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        for i in range(threshold):
            _open_shift(client, f'b-{i}')

        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['crypto_breaker_open'] is True, (
        f'Gate 3k: after {threshold} failures, open must be True: {body["crypto_breaker_open"]!r}'
    )
    assert body['crypto_consecutive_failures'] == threshold, (
        f'Gate 3k: failures must equal threshold: got {body["crypto_consecutive_failures"]!r}'
    )


# ---------------------------------------------------------------------------
# Test C — after admin reset: open=False, failures=0 reflected in summary
# ---------------------------------------------------------------------------

def test_gate3k_summary_after_reset_shows_closed_breaker(tmp_path: Path) -> None:
    """ops/summary reflects reset breaker after POST /v1/admin/crypto/reset-breaker."""
    threshold = 2
    cfg = _config(tmp_path, 'gate3k_c.sqlite3', extra={'crypto': {'breaker_threshold': threshold}})
    c = RuntimeContainer(
        cfg,
        crypto_provider=CountingFailProvider(),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        for i in range(threshold):
            _open_shift(client, f'c-{i}')

        # Verify open before reset
        body_before = client.get('/v1/ops/summary').json()
        assert body_before['crypto_breaker_open'] is True

        # Reset
        r_reset = client.post('/v1/admin/crypto/reset-breaker')
        assert r_reset.status_code == 200

        # Verify closed after reset
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['crypto_breaker_open'] is False, (
        f'Gate 3k: after reset, open must be False: {body["crypto_breaker_open"]!r}'
    )
    assert body['crypto_consecutive_failures'] == 0, (
        f'Gate 3k: after reset, failures must be 0: {body["crypto_consecutive_failures"]!r}'
    )


# ---------------------------------------------------------------------------
# Test D — existing ops summary fields still present
# ---------------------------------------------------------------------------

def test_gate3k_existing_ops_summary_fields_intact(tmp_path: Path) -> None:
    """Adding crypto fields must not remove existing ops/summary fields."""
    cfg = _config(tmp_path, 'gate3k_d.sqlite3')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    for field in (
        'health', 'metrics', 'startup_report',
        'manual_reconciliation_count', 'outbox_pending_count', 'reconciliation_pending_count',
        'crypto_breaker_open', 'crypto_consecutive_failures', 'crypto_breaker_threshold',
    ):
        assert field in body, f'Gate 3k: ops/summary must contain {field!r}'
