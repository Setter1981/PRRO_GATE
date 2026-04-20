"""
Contract tests for FiscalSidecarTransport (DPS_PRRO_FISCAL_SIDECAR_V2).

All tests use httpx.MockTransport — no real network or Rust binary required.
"""
from __future__ import annotations

import json

import httpx
import pytest

from prro_gateway.enums import DocumentState, TransportKind
from prro_gateway.ports import TransportRejectedError, TransportRetryableError
from prro_gateway.transports.fiscal_sidecar import FiscalSidecarTransport

# Minimal canonical payload that the sidecar would accept.
_CANONICAL = {
    'schema_version': '1.0',
    'request_id': 'req-001',
    'idempotency_key': 'idem-001',
    'operation_type': 'SELL',
    'fiscal_number': '3001234567',
    'business_ts': '2026-04-20T10:00:00Z',
    'payload': {'receipt': {'total': 10000, 'payments': [{'type': 'CASH', 'amount': 10000}]}},
    'payload_sha256': 'abc123',
}

_SIDECAR_URL = 'http://127.0.0.1:8765'


def _transport_with(handler) -> FiscalSidecarTransport:
    client = httpx.Client(transport=httpx.MockTransport(handler))
    return FiscalSidecarTransport(sidecar_url=_SIDECAR_URL, http_client=client)


def _send(transport: FiscalSidecarTransport, payload=None):
    return transport.send(
        document_id='doc-001',
        signed_payload=json.dumps(payload or _CANONICAL),
        fiscal_number='3001234567',
        backend_profile_id='bp-1',
        transport_profile_id='tp-1',
        operation_type='SELL',
    )


# ── 1. Successful DPS ACK ─────────────────────────────────────────────────────

def test_send_ok():
    def handler(req: httpx.Request) -> httpx.Response:
        assert req.url.path == '/fiscal/send'
        assert req.method == 'POST'
        body = json.loads(req.content)
        assert body['fiscal_number'] == '3001234567'
        return httpx.Response(200, json={'status': 1, 'fiscal_id': 'FIS-2026-001'})

    result = _send(_transport_with(handler))

    assert result.state == DocumentState.ACK
    assert result.submission_status == 'DPS_ACK'
    assert result.transport_request_id == 'FIS-2026-001'


# ── 2. DPS negative status (on-protocol rejection) ────────────────────────────

def test_send_dps_rejected():
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json={
            'status': -10,
            'fiscal_id': '',
            'error_message': 'ERROR_BAD_HASH_PREV',
        })

    with pytest.raises(TransportRejectedError, match='ERROR_BAD_HASH_PREV'):
        _send(_transport_with(handler))


# ── 3. License expired → 403 → non-retryable ─────────────────────────────────

def test_send_license_expired():
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(403, json={'error': 'license expired'})

    with pytest.raises(TransportRejectedError, match='403'):
        _send(_transport_with(handler))


# ── 4. gRPC unavailable → 502 → retryable ────────────────────────────────────

def test_send_grpc_unavailable_is_retryable():
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(502, json={'error': 'grpc: connection refused'})

    with pytest.raises(TransportRetryableError, match='502'):
        _send(_transport_with(handler))


# ── 5. Demo limit exceeded → 403 → non-retryable ─────────────────────────────

def test_send_demo_limit_exceeded():
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(403, json={'error': 'license: demo sell limit exceeded'})

    with pytest.raises(TransportRejectedError):
        _send(_transport_with(handler))


# ── 6. Request body is the canonical JSON passed through verbatim ─────────────

def test_request_body_shape_matches_spec():
    captured: list[dict] = []

    def handler(req: httpx.Request) -> httpx.Response:
        captured.append(json.loads(req.content))
        return httpx.Response(200, json={'status': 1, 'fiscal_id': 'FIS-X'})

    _send(_transport_with(handler))

    assert len(captured) == 1
    body = captured[0]
    assert body['schema_version'] == _CANONICAL['schema_version']
    assert body['operation_type'] == _CANONICAL['operation_type']
    assert body['fiscal_number'] == _CANONICAL['fiscal_number']
    assert body['payload'] == _CANONICAL['payload']


# ── Extra: network error → retryable ─────────────────────────────────────────

def test_network_error_is_retryable():
    def handler(req: httpx.Request) -> httpx.Response:
        raise httpx.ConnectError('connection refused')

    with pytest.raises(TransportRetryableError, match='unreachable'):
        _send(_transport_with(handler))


# ── Extra: TransportKind enum value exists ────────────────────────────────────

def test_transport_kind_value():
    assert TransportKind.DPS_PRRO_FISCAL_SIDECAR_V2 == 'DPS_PRRO_FISCAL_SIDECAR_V2'


# ── Extra: bytes signed_payload is parsed correctly ──────────────────────────

def test_bytes_payload_parsed():
    def handler(req: httpx.Request) -> httpx.Response:
        body = json.loads(req.content)
        assert body['fiscal_number'] == '3001234567'
        return httpx.Response(200, json={'status': 1, 'fiscal_id': 'FIS-B'})

    client = httpx.Client(transport=httpx.MockTransport(handler))
    transport = FiscalSidecarTransport(sidecar_url=_SIDECAR_URL, http_client=client)
    result = transport.send(
        document_id='doc-bytes',
        signed_payload=json.dumps(_CANONICAL).encode(),
        fiscal_number='3001234567',
        backend_profile_id='bp-1',
        transport_profile_id='tp-1',
    )
    assert result.state == DocumentState.ACK


# ── H-3: 429 rate-limit → retryable (boundary: in _RETRYABLE_HTTP set) ───────

def test_send_rate_limited_is_retryable():
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(429, json={'error': 'too many requests'})

    with pytest.raises(TransportRetryableError, match='429'):
        _send(_transport_with(handler))


# ── Retryable boundary: 500 and 503 also retryable ───────────────────────────

@pytest.mark.parametrize('status_code', [500, 503, 504])
def test_5xx_codes_are_retryable(status_code: int):
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(status_code, json={'error': 'server error'})

    with pytest.raises(TransportRetryableError, match=str(status_code)):
        _send(_transport_with(handler))


# ── Non-retryable boundary: 400 and 404 are rejected, not retried ─────────────

@pytest.mark.parametrize('status_code', [400, 404])
def test_4xx_non_retryable_codes_are_rejected(status_code: int):
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(status_code, json={'error': 'bad'})

    with pytest.raises(TransportRejectedError, match=str(status_code)):
        _send(_transport_with(handler))


# ── C-1: crypto_provider guard — ValueError if not passthrough ────────────────

def test_non_passthrough_crypto_provider_raises():
    with pytest.raises(ValueError, match='passthrough'):
        FiscalSidecarTransport(
            sidecar_url=_SIDECAR_URL,
            crypto_provider='sidecar',
        )


def test_passthrough_crypto_provider_is_accepted():
    # Must not raise — passthrough is the only valid value.
    transport = FiscalSidecarTransport(
        sidecar_url=_SIDECAR_URL,
        crypto_provider='passthrough',
    )
    assert transport is not None


# ── L-1: status=0 is rejected, not silently ignored (boundary case) ───────────

def test_dps_status_zero_is_rejected():
    # status=0 is not a DPS-defined success; _map_dps_response must raise.
    def handler(req: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json={'status': 0, 'fiscal_id': ''})

    with pytest.raises(TransportRejectedError, match='status=0'):
        _send(_transport_with(handler))


# ── transport_profile.endpoint overrides constructor URL ─────────────────────

def test_transport_profile_endpoint_override():
    captured_urls: list[str] = []

    def handler(req: httpx.Request) -> httpx.Response:
        captured_urls.append(str(req.url))
        return httpx.Response(200, json={'status': 1, 'fiscal_id': 'FIS-OVR'})

    client = httpx.Client(transport=httpx.MockTransport(handler))
    transport = FiscalSidecarTransport(
        sidecar_url='http://default-host:8765',
        http_client=client,
    )

    class FakeProfile:
        endpoint = 'http://override-host:9999'

    transport.send(
        document_id='doc-ovr',
        signed_payload=json.dumps(_CANONICAL),
        fiscal_number='3001234567',
        backend_profile_id='bp-1',
        transport_profile_id='tp-1',
        transport_profile=FakeProfile(),
    )
    assert len(captured_urls) == 1
    assert 'override-host:9999' in captured_urls[0]
    assert 'default-host' not in captured_urls[0]
