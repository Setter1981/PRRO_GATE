"""
Gate 1n — payload hash integrity smoke tests.

payload_sha256 semantics:
  CanonicalEnvelopeBuilder.build():
    inner_json  = dumps_json(payload)          # JSON of inner fiscal payload dict
    sha256_hex  = sha256(inner_json.encode())  # hash of inner payload only

  InboxRepository.accept_command():
    inbox.payload_json   = command.model_dump_json()  # FULL CanonicalFiscalCommand
    inbox.payload_sha256 = command.payload_sha256     # sha256 of inner payload

  FiscalDocumentRepository.create_prepared():
    document.payload_json   = inbox.payload_json      # same full command JSON
    document.payload_sha256 = inbox.payload_sha256    # same inner-payload sha256

Invariant: payload_sha256 is the audit anchor for the fiscal content (goods,
payments, amounts) — independent of command metadata (request_id, ts, etc.).
A mismatch means silent payload corruption.

Tests:
  A — unit: adapter produces correct sha256 (matches manual re-derivation).
  B — unit: sha256 is stable for identical payloads (deterministic serialisation).
  C — unit: sha256 differs for different payloads.
  D — integration: inbox.payload_sha256 == document.payload_sha256 after full flow.
  E — integration: hash can be re-derived from payload recovered from inbox.payload_json.
  F — unit: RETURN payload has distinct sha256 from SELL with same goods/payments.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.adapters import CheckboxRestAdapter
from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app
from prro_gateway.utils.json_codec import dumps_json

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'

_ADAPTER = CheckboxRestAdapter()


# ---------------------------------------------------------------------------
# Shared payloads for unit tests
# ---------------------------------------------------------------------------

_BASE_CTX = {
    'request_id': 'req-gate1n',
    'fiscal_number': FISCAL_NUMBER,
    'business_ts': '2026-03-29T22:00:00Z',
}


def _raw_sell(ext_id: str = 'ext-gate1n-sell') -> dict:
    return {
        'context': _BASE_CTX,
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1n',
            'goods': [{'name': 'Coffee', 'price': 5000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 5000}],
        },
    }


def _raw_return() -> dict:
    return {
        'context': _BASE_CTX,
        'operation': 'RETURN',
        'request': {
            'external_request_id': 'ext-gate1n-return',
            'cashier_id': 'cashier-gate1n',
            'related_receipt_id': 'rcpt-orig-001',
            'goods': [{'name': 'Coffee', 'price': 5000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 5000}],
        },
    }


def _recompute_sha256(payload: dict) -> str:
    """Re-derive sha256 the same way CanonicalEnvelopeBuilder does."""
    inner_json = dumps_json(payload) or '{}'
    return hashlib.sha256(inner_json.encode('utf-8')).hexdigest()


# ---------------------------------------------------------------------------
# A — adapter produces correct sha256
# ---------------------------------------------------------------------------

def test_gate1n_sell_sha256_matches_payload(tmp_path: Path) -> None:
    """payload_sha256 on the command must equal sha256(dumps_json(payload))."""
    cmd = _ADAPTER.map_command(_raw_sell())
    expected = _recompute_sha256(cmd.payload)
    assert cmd.payload_sha256 == expected, (
        f'SELL: payload_sha256 mismatch.\n'
        f'  stored   = {cmd.payload_sha256!r}\n'
        f'  expected = {expected!r}'
    )


def test_gate1n_return_sha256_matches_payload() -> None:
    cmd = _ADAPTER.map_command(_raw_return())
    expected = _recompute_sha256(cmd.payload)
    assert cmd.payload_sha256 == expected, (
        f'RETURN: payload_sha256 mismatch.\n'
        f'  stored   = {cmd.payload_sha256!r}\n'
        f'  expected = {expected!r}'
    )


# ---------------------------------------------------------------------------
# B — sha256 is deterministic (same payload → same hash)
# ---------------------------------------------------------------------------

def test_gate1n_sha256_is_deterministic() -> None:
    """Two adapter calls with identical input must produce identical sha256."""
    cmd1 = _ADAPTER.map_command(_raw_sell('ext-a'))
    cmd2 = _ADAPTER.map_command(_raw_sell('ext-a'))
    assert cmd1.payload_sha256 == cmd2.payload_sha256, (
        f'Same payload → different sha256:\n'
        f'  first  = {cmd1.payload_sha256!r}\n'
        f'  second = {cmd2.payload_sha256!r}'
    )


# ---------------------------------------------------------------------------
# C — sha256 differs for different fiscal content
# ---------------------------------------------------------------------------

def test_gate1n_different_goods_produce_different_sha256() -> None:
    """Changing goods must change sha256 — hash is sensitive to fiscal content."""
    raw_a = _raw_sell()
    raw_b = {**_raw_sell(), 'request': {**_raw_sell()['request'],
             'goods': [{'name': 'Tea', 'price': 2000, 'quantity': 1000}]}}
    cmd_a = _ADAPTER.map_command(raw_a)
    cmd_b = _ADAPTER.map_command(raw_b)
    assert cmd_a.payload_sha256 != cmd_b.payload_sha256, (
        'Different goods must produce different payload_sha256 '
        '(hash not sensitive to fiscal content changes)'
    )


# ---------------------------------------------------------------------------
# F — SELL and RETURN with same goods produce different sha256
# ---------------------------------------------------------------------------

def test_gate1n_sell_and_return_sha256_are_distinct() -> None:
    """
    SELL and RETURN payloads differ in receipt.type — sha256 must differ.
    If they matched, RETURN could be idempotency-confused with SELL.
    """
    cmd_sell = _ADAPTER.map_command(_raw_sell())
    cmd_return = _ADAPTER.map_command(_raw_return())
    assert cmd_sell.payload_sha256 != cmd_return.payload_sha256, (
        'SELL and RETURN payload sha256 must differ (receipt.type differs)'
    )


# ---------------------------------------------------------------------------
# D + E — integration: inbox and document sha256 consistent after full flow
# ---------------------------------------------------------------------------

def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1n'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1n-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1N-001',
            'updated_at': '2026-03-29T22:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1n-receipt', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1N',
            'updated_at': '2026-03-29T22:01:00+00:00',
        })
    raise AssertionError(f'gate1n: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1n.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1n',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1N-LIC',
            'cashier_pin': '0000',
        },
    })


def test_gate1n_inbox_and_document_sha256_consistent(tmp_path: Path) -> None:
    """
    After a full REST flow:
    D: inbox.payload_sha256 == document.payload_sha256 (consistent propagation).
    E: sha256 can be re-derived from the inner payload extracted from stored JSON.
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        # Open shift
        client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'gate1n-open',
                'fiscal_number': FISCAL_NUMBER,
                'backend_profile_id': BACKEND_PROFILE,
                'transport_profile_id': TRANSPORT_PROFILE,
                'channel_owner': 'gate1n',
                'business_ts': '2026-03-29T22:00:00Z',
            },
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1n', 'cashier_id': 'cashier-gate1n'},
        })

        # SELL
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'gate1n-sell-001',
                'fiscal_number': FISCAL_NUMBER,
                'backend_profile_id': BACKEND_PROFILE,
                'transport_profile_id': TRANSPORT_PROFILE,
                'channel_owner': 'gate1n',
                'business_ts': '2026-03-29T22:01:00Z',
            },
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1n-001',
                'cashier_id': 'cashier-gate1n',
                'goods': [{'name': 'Coffee', 'price': 5000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 5000}],
            },
        })
        assert r_sell.status_code == 200, f'SELL failed: {r_sell.text}'
        assert r_sell.json().get('document_state') == 'ACK', f'SELL not ACK: {r_sell.json()}'
        doc_id = r_sell.json()['document_id']

    with container.connect() as conn:
        inbox_row = conn.execute(
            "SELECT payload_json, payload_sha256 FROM ingress_inbox "
            "WHERE request_id = 'gate1n-sell-001'",
        ).fetchone()
        doc_row = conn.execute(
            "SELECT payload_json, payload_sha256 FROM fiscal_documents WHERE document_id = ?",
            (doc_id,),
        ).fetchone()

    assert inbox_row is not None, 'Inbox record not found'
    assert doc_row is not None, 'Document record not found'

    inbox_payload_json, inbox_sha256 = inbox_row
    doc_payload_json, doc_sha256 = doc_row

    # D: consistent propagation
    assert inbox_sha256 == doc_sha256, (
        f'inbox.payload_sha256 != document.payload_sha256\n'
        f'  inbox = {inbox_sha256!r}\n'
        f'  doc   = {doc_sha256!r}'
    )
    assert inbox_payload_json == doc_payload_json, (
        'inbox.payload_json != document.payload_json — payload diverged during write-path'
    )

    # E: re-derive sha256 from stored payload_json
    # inbox.payload_json = full CanonicalFiscalCommand JSON
    # The inner payload is at command["payload"]
    stored_command = json.loads(inbox_payload_json)
    inner_payload = stored_command['payload']
    recomputed = _recompute_sha256(inner_payload)

    assert recomputed == inbox_sha256, (
        f'Re-derived sha256 from stored payload_json does not match stored hash.\n'
        f'  stored     = {inbox_sha256!r}\n'
        f'  recomputed = {recomputed!r}\n'
        f'This means the hash anchor is broken — payload or serialization drifted.'
    )
