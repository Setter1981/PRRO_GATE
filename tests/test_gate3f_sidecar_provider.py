"""
Gate 3f — first real crypto sidecar adapter/client stub.

Adds SidecarCryptoClient and SidecarCryptoProvider to runtime/providers.py.
SidecarCryptoClient transport contract:
  POST {sidecar_url}/sign
  Request:  {"document_id": str, "payload": str}
  Response: {"signed_payload": str}

_resolve_crypto_provider() now supports provider='sidecar' (requires sidecar_url).
RuntimeContainer accepts crypto_http_client for sidecar HTTP injection.

Four tests:
  A — config-based sidecar selection: crypto.provider=sidecar + sidecar_url + mock sidecar HTTP
      → SidecarCryptoProvider used, signed payload returned, doc reaches ACK
  B — sidecar client success path: unit test on SidecarCryptoClient directly
      → correct POST body, returns signed_payload from response
  C — sidecar client failure paths → CryptoProviderUnavailableError
      → write-path: ERROR_RETRYABLE, not PREPARED, HTTP 200
  D — sidecar config without sidecar_url → ValueError at initialize()
"""
from __future__ import annotations

from pathlib import Path

import httpx
import pytest
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.ports import CryptoProviderUnavailableError
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
# Mock transports
# ---------------------------------------------------------------------------

def _sidecar_ok_mock(request: httpx.Request) -> httpx.Response:
    """Mock crypto sidecar: returns signed_payload = payload + ':signed'."""
    if request.url.path == '/sign':
        body = request.read()
        import json
        data = json.loads(body)
        return httpx.Response(200, json={'signed_payload': data['payload'] + ':signed'})
    raise AssertionError(f'gate3f sidecar mock: unexpected {request.method} {request.url}')


def _sidecar_error_mock(request: httpx.Request) -> httpx.Response:
    """Mock crypto sidecar: always returns 503."""
    if request.url.path == '/sign':
        return httpx.Response(503, json={'error': 'sidecar unavailable'})
    raise AssertionError(f'gate3f sidecar mock: unexpected {request.method} {request.url}')


def _checkbox_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-gate3f'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3f-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3F-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate3f-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE3F-001',
            'updated_at': '2026-03-31T08:01:00+00:00',
        })
    raise AssertionError(f'gate3f checkbox mock: unexpected {request.method} {request.url}')


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

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
            'channel_owner': 'gate3f',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3F-LIC',
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
        'channel_owner': 'gate3f',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — config-based sidecar selection: end-to-end doc reaches ACK
# ---------------------------------------------------------------------------

def test_gate3f_sidecar_config_selection_reaches_ack(tmp_path: Path) -> None:
    """
    crypto.provider='sidecar' + sidecar_url set → SidecarCryptoProvider selected.
    Mock sidecar returns payload+':signed'. Signed payload used for transport.
    SHIFT_OPEN doc reaches ACK end-to-end.
    """
    cfg = _config(tmp_path, 'gate3f_a.sqlite3', extra={
        'crypto': {
            'provider': 'sidecar',
            'sidecar_url': SIDECAR_URL,
            'timeout_seconds': 5.0,
        },
    })
    assert cfg.crypto.provider == 'sidecar'
    assert cfg.crypto.sidecar_url == SIDECAR_URL

    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_checkbox_mock)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_ok_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3f-open-a'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3f-a', 'cashier_id': 'cashier-gate3f'},
        })

    assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK', (
        f'Gate 3f: sidecar provider (config) must allow doc to reach ACK: {r_shift.json()}'
    )

    # Confirm resolved provider type at runtime
    assert isinstance(c.command_processor.crypto_provider, SidecarCryptoProvider), (
        f'Gate 3f: crypto_provider must be SidecarCryptoProvider when config=sidecar: '
        f'got {type(c.command_processor.crypto_provider)}'
    )


# ---------------------------------------------------------------------------
# Test B — SidecarCryptoClient unit: correct POST body, returns signed_payload
# ---------------------------------------------------------------------------

def test_gate3f_sidecar_client_success_path() -> None:
    """
    Unit test: SidecarCryptoClient.sign() sends correct JSON body and returns signed_payload.
    Verifies transport contract: POST /sign with document_id + payload.
    """
    captured_requests: list[httpx.Request] = []

    def _capturing_mock(request: httpx.Request) -> httpx.Response:
        captured_requests.append(request)
        return httpx.Response(200, json={'signed_payload': 'SIGNED-RESULT'})

    client = SidecarCryptoClient(
        base_url=SIDECAR_URL,
        http_client=httpx.Client(transport=httpx.MockTransport(_capturing_mock)),
    )
    result = client.sign(document_id='doc-001', payload_json='{"fiscal": "data"}')

    assert result == 'SIGNED-RESULT', (
        f'Gate 3f: SidecarCryptoClient must return signed_payload from response: {result!r}'
    )
    assert len(captured_requests) == 1
    req = captured_requests[0]
    assert req.method == 'POST'
    assert req.url.path == '/sign'

    import json
    body = json.loads(req.read())
    assert body['document_id'] == 'doc-001', (
        f'Gate 3f: POST body must include document_id: {body}'
    )
    assert body['payload'] == '{"fiscal": "data"}', (
        f'Gate 3f: POST body must include payload: {body}'
    )


# ---------------------------------------------------------------------------
# Test C — sidecar failure → ERROR_RETRYABLE (full write-path integration)
# ---------------------------------------------------------------------------

def test_gate3f_sidecar_failure_gives_error_retryable(tmp_path: Path) -> None:
    """
    Sidecar returns 503 → SidecarCryptoClient raises CryptoProviderUnavailableError →
    _stage_sign() catches it → doc transitions to ERROR_RETRYABLE.
    HTTP response is 200. Doc not stuck in PREPARED.
    """
    cfg = _config(tmp_path, 'gate3f_c.sqlite3', extra={
        'crypto': {'provider': 'sidecar', 'sidecar_url': SIDECAR_URL, 'timeout_seconds': 5.0},
    })
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_checkbox_mock)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_error_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3f-open-c'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3f-c', 'cashier_id': 'cashier-gate3f'},
        })

    assert r_shift.status_code == 200, (
        f'Gate 3f: sidecar 503 must be handled gracefully (200), got {r_shift.status_code}. '
        f'Response: {r_shift.text}'
    )
    body = r_shift.json()
    assert body.get('document_state') == 'ERROR_RETRYABLE', (
        f'Gate 3f: doc must be ERROR_RETRYABLE after sidecar 503: '
        f'got {body.get("document_state")!r}'
    )
    assert body.get('canonical_error_code') == 'CRYPTO_PROVIDER_UNAVAILABLE', (
        f'Gate 3f: canonical_error_code must be CRYPTO_PROVIDER_UNAVAILABLE: '
        f'got {body.get("canonical_error_code")!r}'
    )

    doc_id = body.get('document_id')
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
        prepared = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE state='PREPARED'"
        ).fetchall()
    assert doc.state == DocumentState.ERROR_RETRYABLE
    assert doc.canonical_error_code == 'CRYPTO_PROVIDER_UNAVAILABLE'
    assert len(prepared) == 0, f'Gate 3f: no doc must remain in PREPARED: {prepared}'


# ---------------------------------------------------------------------------
# Test D — sidecar without sidecar_url → ValueError at initialize()
# ---------------------------------------------------------------------------

def test_gate3f_sidecar_without_url_raises(tmp_path: Path) -> None:
    """
    crypto.provider='sidecar' with no sidecar_url raises ValueError at initialize().
    Fail-fast: misconfigured sidecar is immediately visible at startup.
    """
    cfg = _config(tmp_path, 'gate3f_d.sqlite3', extra={
        'crypto': {'provider': 'sidecar'},
    })
    assert cfg.crypto.sidecar_url is None

    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_checkbox_mock)),
    )

    with pytest.raises(ValueError, match="sidecar_url"):
        with TestClient(create_app(c)):
            pass
