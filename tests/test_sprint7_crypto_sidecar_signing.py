"""
Sprint 7 / step 6 — real crypto-sidecar signing contract tests.

Coverage:
  CS1 — SidecarCryptoClient.sign_raw sends base64 payload, returns decoded bytes
  CS2 — SidecarCryptoProvider.sign_raw delegates to client
  CS3 — PassthroughCryptoProvider.sign_raw returns raw bytes (dev compat)
  CS4 — DPS transport base64-decodes signed_payload for check_sign
  CS5 — existing DPS send/recovery/probe tests remain green
"""
from __future__ import annotations

import base64

import httpx
import pytest

from prro_gateway.runtime.providers import (
    PassthroughCryptoProvider,
    SidecarCryptoClient,
    SidecarCryptoProvider,
)


# ---------------------------------------------------------------------------
# CS1 — SidecarCryptoClient.sign_raw request/response shape
# ---------------------------------------------------------------------------

def test_cs1_sidecar_client_sign_raw() -> None:
    """sign_raw sends base64 payload to /sign_raw, decodes base64 response."""
    captured = {}
    fake_cms = b'\x30\x82\x00\x01FAKE_CMS_DER'

    def _mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/sign_raw'):
            body = request.read()
            import json
            captured['request'] = json.loads(body)
            return httpx.Response(200, json={
                'signed_base64': base64.b64encode(fake_cms).decode('ascii'),
            })
        return httpx.Response(404)

    client = SidecarCryptoClient(
        base_url='http://sidecar:8080',
        http_client=httpx.Client(transport=httpx.MockTransport(_mock)),
    )

    result = client.sign_raw(data=b'test-payload')

    assert result == fake_cms
    assert captured['request']['payload_base64'] == base64.b64encode(b'test-payload').decode('ascii')


# ---------------------------------------------------------------------------
# CS2 — SidecarCryptoProvider.sign_raw delegates to client
# ---------------------------------------------------------------------------

def test_cs2_provider_sign_raw_delegates() -> None:
    fake_cms = b'\x30\x82PROVIDER_CMS'

    def _mock(request: httpx.Request) -> httpx.Response:
        if request.url.path.endswith('/sign_raw'):
            return httpx.Response(200, json={
                'signed_base64': base64.b64encode(fake_cms).decode('ascii'),
            })
        return httpx.Response(404)

    client = SidecarCryptoClient(
        base_url='http://sidecar:8080',
        http_client=httpx.Client(transport=httpx.MockTransport(_mock)),
    )
    provider = SidecarCryptoProvider(client=client)

    result = provider.sign_raw(data=b'FN-001')
    assert result == fake_cms


# ---------------------------------------------------------------------------
# CS3 — PassthroughCryptoProvider.sign_raw returns raw bytes
# ---------------------------------------------------------------------------

def test_cs3_passthrough_sign_raw() -> None:
    provider = PassthroughCryptoProvider()
    result = provider.sign_raw(data=b'test-data')
    assert result == b'test-data'


# ---------------------------------------------------------------------------
# CS4 — DPS transport base64-decodes signed_payload for check_sign
# ---------------------------------------------------------------------------

def test_cs4_transport_passes_bytes_directly() -> None:
    """When signed_payload is bytes (from sign_raw), transport uses them directly — no decoding."""
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    fake_cms = b'\x30\x82REAL_CMS_DER_BYTES'

    class _MockResponse:
        id = 'DPS-CS4'
        status = 1
        error_message = ''

    class _CapturingStub:
        def __init__(self):
            self.captured_check_sign = None

        def sendChkV2(self, request, *, timeout=None):
            self.captured_check_sign = request.check_sign
            return _MockResponse()

    stub = _CapturingStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    transport.send(
        document_id='doc-cs4',
        signed_payload=fake_cms,  # bytes from sign_raw (CMS DER)
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='SELL',
    )

    assert stub.captured_check_sign == fake_cms, (
        f'check_sign must be raw CMS bytes, got {stub.captured_check_sign!r}'
    )


def test_cs4b_transport_does_not_mangle_string_signed_payload() -> None:
    """When signed_payload is a plain string (non-base64), transport must NOT
    silently decode it to garbage. It should encode to utf-8 as-is."""
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    plain_xml = '<RQ V="1"><DAT>test</DAT></RQ>'

    class _MockResponse:
        id = 'DPS-CS4B'
        status = 1
        error_message = ''

    class _CapturingStub:
        def __init__(self):
            self.captured_check_sign = None

        def sendChkV2(self, request, *, timeout=None):
            self.captured_check_sign = request.check_sign
            return _MockResponse()

    stub = _CapturingStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    transport.send(
        document_id='doc-cs4b',
        signed_payload=plain_xml,  # plain string, NOT base64
        fiscal_number='FN-001',
        backend_profile_id='bp',
        transport_profile_id='tp',
        operation_type='SELL',
    )

    # Must be clean utf-8 encoding, NOT garbage from base64 decode attempt
    assert stub.captured_check_sign == plain_xml.encode('utf-8'), (
        f'String signed_payload must be utf-8 encoded, got {stub.captured_check_sign!r}'
    )
