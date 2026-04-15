"""
Gate 3a — crypto contour audit: seam verification tests.

Audit findings:
  - CryptoProvider.sign(*, document_id, payload_json) -> str  (ports.py:46-47)
  - Call site: write_path.py:239, OUTSIDE any open transaction (invariant #1 preserved)
  - PassthroughCryptoProvider: returns payload_json unchanged; default in all production paths
  - Provider injectable via RuntimeContainer(cfg, crypto_provider=...)
  - CryptoProviderUnavailableError (RuntimeError subclass) IS caught in write_path:
      → doc transitions to ERROR_RETRYABLE + canonical_error_code=CRYPTO_PROVIDER_UNAVAILABLE
      → API response is 200 (document_state=ERROR_RETRYABLE)
  - _requires_local_sign(): always returns True unless transport profile config_json
    has require_local_sign:false — so sign is always called on all current profiles
  - KNOWN GAP: bare exceptions from sign() that are NOT CryptoProviderUnavailableError
    propagate through process_next() into accept_checkbox() and get caught by the
    REST ingress ValueError/KeyError handler, returning 400. The doc is left in
    PREPARED state (already committed before sign was called).

Three tests:
  A — seam injectable: spy provider called with correct document_id, doc reaches ACK
  B — CryptoProviderUnavailableError integration: full HTTP path, 200 response,
      doc.state=ERROR_RETRYABLE, canonical_error_code=CRYPTO_PROVIDER_UNAVAILABLE
  C — bare exception gap (DOCUMENTED GAP): non-CryptoProviderUnavailableError from sign()
      → REST endpoint returns 400 (misleading: treated as bad input, not infrastructure error)
      → doc state is PREPARED (stuck, not ERROR_RETRYABLE)
      → requires lease expiry to recover
      This test should FAIL when the gap is fixed (broader except in _stage_sign).
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
# Inline test providers
# ---------------------------------------------------------------------------

class SpyCryptoProvider:
    """Records sign() calls and passes through payload unchanged."""
    def __init__(self):
        self.calls: list[dict] = []

    def sign(self, *, document_id: str, payload_json: str) -> str:
        self.calls.append({'document_id': document_id, 'payload_json': payload_json})
        return payload_json  # passthrough behaviour


class UnavailableCryptoProvider:
    """Always raises CryptoProviderUnavailableError — the handled failure path."""
    def sign(self, *, document_id: str, payload_json: str) -> str:
        raise CryptoProviderUnavailableError('test: crypto sidecar unavailable')


class BareExceptionCryptoProvider:
    """Raises a plain ValueError — NOT wrapped in CryptoProviderUnavailableError.
    Simulates a future sidecar client that raises an unexpected exception.
    Documents the current unhandled-exception gap in write_path._stage_sign().
    """
    def sign(self, *, document_id: str, payload_json: str) -> str:
        raise ValueError('test: bare exception from sign (not CryptoProviderUnavailableError)')


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------

def _transport_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-gate3a'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate3a-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE3A-001',
            'updated_at': '2026-03-31T08:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate3a-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE3A-001',
            'updated_at': '2026-03-31T08:01:00+00:00',
        })
    raise AssertionError(f'gate3a transport: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str) -> AppConfig:
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
            'channel_owner': 'gate3a',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE3A-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-31T08:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate3a',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — seam injectable: spy called, doc reaches ACK
# ---------------------------------------------------------------------------

def test_gate3a_sign_seam_injectable_and_called(tmp_path: Path) -> None:
    """
    CryptoProvider is injectable via RuntimeContainer(cfg, crypto_provider=...).
    After SHIFT_OPEN + SELL, the spy records sign() calls:
    - Called at least once for SELL document
    - Called with the document_id matching the created document
    - The write-path uses the spy's return value (passthrough): SELL reaches ACK
    """
    spy = SpyCryptoProvider()
    cfg = _config(tmp_path, 'gate3a_a.sqlite3')
    c = RuntimeContainer(
        cfg,
        crypto_provider=spy,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )
    sell_doc_id: str | None = None

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3a-open-a'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3a-a', 'cashier_id': 'cashier-gate3a'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate3a-sell-a'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate3a-a',
                'cashier_id': 'cashier-gate3a',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    assert len(spy.calls) >= 1, (
        'CryptoProvider.sign() must be called at least once during write-path processing. '
        f'Got {len(spy.calls)} calls. '
        'If 0: sign seam is broken or require_local_sign=false is silently active.'
    )

    sell_call_doc_ids = [call['document_id'] for call in spy.calls]
    assert sell_doc_id in sell_call_doc_ids, (
        f'sign() must be called with the SELL document_id ({sell_doc_id}). '
        f'Actual call document_ids: {sell_call_doc_ids}'
    )

    # Verify sign result is used: doc reaches ACK (passthrough returned valid payload)
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc.state == DocumentState.ACK, (
        f'SELL doc must reach ACK when spy passthrough is used: got {doc.state}'
    )


# ---------------------------------------------------------------------------
# Test B — CryptoProviderUnavailableError integration: full HTTP path
# ---------------------------------------------------------------------------

def test_gate3a_crypto_unavailable_error_integration_path(tmp_path: Path) -> None:
    """
    CryptoProviderUnavailableError raised from sign():
    - write_path._stage_sign() catches it and calls _mark_document_and_inbox_error()
    - doc transitions to ERROR_RETRYABLE + canonical_error_code=CRYPTO_PROVIDER_UNAVAILABLE
    - REST ingress returns 200 (not 500): the error is handled, not propagated
    - response includes document_state='ERROR_RETRYABLE'
    - canonical_error_code='CRYPTO_PROVIDER_UNAVAILABLE' visible in response

    This is the HANDLED failure path. No unhandled exceptions, no stuck documents.
    """
    cfg = _config(tmp_path, 'gate3a_b.sqlite3')
    c = RuntimeContainer(
        cfg,
        crypto_provider=UnavailableCryptoProvider(),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        # SHIFT_OPEN with failing crypto → ERROR_RETRYABLE (sign fails immediately)
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3a-open-b'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3a-b', 'cashier_id': 'cashier-gate3a'},
        })

    assert r_shift.status_code == 200, (
        f'CryptoProviderUnavailableError must be handled in write-path: '
        f'expect 200 response, got {r_shift.status_code}. '
        f'Response: {r_shift.text}'
    )
    body = r_shift.json()
    assert body.get('document_state') == 'ERROR_RETRYABLE', (
        f'doc must be ERROR_RETRYABLE after CryptoProviderUnavailableError: '
        f'got document_state={body.get("document_state")!r}'
    )
    assert body.get('canonical_error_code') == 'CRYPTO_PROVIDER_UNAVAILABLE', (
        f'canonical_error_code must be CRYPTO_PROVIDER_UNAVAILABLE: '
        f'got {body.get("canonical_error_code")!r}'
    )

    shift_doc_id = body.get('document_id')
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, shift_doc_id)
    assert doc.state == DocumentState.ERROR_RETRYABLE
    assert doc.canonical_error_code == 'CRYPTO_PROVIDER_UNAVAILABLE'


# ---------------------------------------------------------------------------
# Test C — DOCUMENTED GAP: bare exception from sign() leaves doc in PREPARED
# ---------------------------------------------------------------------------

def test_gate3a_bare_exception_from_sign_transitions_to_error_retryable(tmp_path: Path) -> None:
    """
    Gate 3b hardening: bare exceptions from sign() that are NOT CryptoProviderUnavailableError
    are now caught by the `except Exception` wrapper in write_path._stage_sign().

    Fixed behavior (Gate 3b):
    - ValueError is caught by the new `except Exception as exc` block
    - _mark_document_and_inbox_error() called with state=ERROR_RETRYABLE,
      canonical_error_code=CRYPTO_PROVIDER_UNAVAILABLE
    - REST ingress returns 200 (error is handled, not propagated)
    - response includes document_state='ERROR_RETRYABLE'
    - canonical_error_code='CRYPTO_PROVIDER_UNAVAILABLE' visible in response
    - Document is NOT stuck in PREPARED
    """
    cfg = _config(tmp_path, 'gate3a_c.sqlite3')
    c = RuntimeContainer(
        cfg,
        crypto_provider=BareExceptionCryptoProvider(),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_transport_mock)),
    )

    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate3a-open-c'), 'business_ts': '2026-03-31T08:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate3a-c', 'cashier_id': 'cashier-gate3a'},
        })

    assert r_shift.status_code == 200, (
        f'Gate 3b: bare exception from sign() must be handled in write-path: '
        f'expect 200 response, got {r_shift.status_code}. '
        f'Response: {r_shift.text}'
    )
    body = r_shift.json()
    assert body.get('document_state') == 'ERROR_RETRYABLE', (
        f'Gate 3b: doc must be ERROR_RETRYABLE after bare exception from sign(): '
        f'got document_state={body.get("document_state")!r}'
    )
    assert body.get('canonical_error_code') == 'CRYPTO_PROVIDER_UNAVAILABLE', (
        f'Gate 3b: canonical_error_code must be CRYPTO_PROVIDER_UNAVAILABLE: '
        f'got {body.get("canonical_error_code")!r}'
    )

    shift_doc_id = body.get('document_id')
    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, shift_doc_id)
    assert doc.state == DocumentState.ERROR_RETRYABLE, (
        f'Gate 3b: doc must be ERROR_RETRYABLE in DB: got {doc.state}'
    )
    assert doc.canonical_error_code == 'CRYPTO_PROVIDER_UNAVAILABLE', (
        f'Gate 3b: canonical_error_code in DB must be CRYPTO_PROVIDER_UNAVAILABLE: '
        f'got {doc.canonical_error_code!r}'
    )

    with c.connect() as conn:
        docs_prepared = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE state='PREPARED'"
        ).fetchall()
    assert len(docs_prepared) == 0, (
        f'Gate 3b: no document must remain in PREPARED after bare exception fix: '
        f'found PREPARED docs: {docs_prepared}'
    )
