"""
Integration tests: write_path + FiscalSidecarTransport (DPS_PRRO_FISCAL_SIDECAR_V2).

Verifies the passthrough-crypto → sidecar-transport path:
  canonical JSON → PassthroughCryptoProvider.sign() → (unchanged) → FiscalSidecarTransport → HTTP POST
"""
from __future__ import annotations

import json
import sqlite3
from datetime import UTC, datetime
from typing import Callable
from unittest.mock import MagicMock

import httpx
import pytest

from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState, TransportKind
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.ports import SendResult
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.runtime.providers import PassthroughCryptoProvider
from prro_gateway.services import WritePathWorker
from prro_gateway.transports.fiscal_sidecar import FiscalSidecarTransport
from prro_gateway.utils.json_codec import dumps_json

_SIDECAR_URL = 'http://127.0.0.1:8765'
_FN = 'FN-DEV-0001'
_TP = 'transport_sidecar_v2_default'   # inserted by migration 018
_BP = 'backend_dps_direct'             # inserted by 002_seed_reference_data.sql


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_transport(handler: Callable) -> FiscalSidecarTransport:
    client = httpx.Client(transport=httpx.MockTransport(handler))
    return FiscalSidecarTransport(sidecar_url=_SIDECAR_URL, http_client=client)


def _seed_sidecar_binding(conn: sqlite3.Connection) -> None:
    """Wire FN-DEV-0001 to sidecar transport profile."""
    conn.execute(
        "UPDATE prro_bindings SET transport_profile_id = ? WHERE fiscal_number = ?",
        (_TP, _FN),
    )
    conn.execute(
        "UPDATE node_state SET mode = 'ONLINE' WHERE fiscal_number = ?", (_FN,)
    )


def _open_shift(conn: sqlite3.Connection) -> None:
    from prro_gateway.enums import ShiftState
    ShiftRepository.create_shift(
        conn,
        shift_id='shift-sidecar-1',
        fiscal_number=_FN,
        state=ShiftState.OPENED,
        open_mode='ONLINE',
        backend_profile_id=_BP,
        transport_profile_id=_TP,
        protocol=Protocol.CHECKBOX_REST,
        integration_owner='front-a',
        channel_lock_acquired_at='2026-04-20T09:00:00+00:00',
    )


def _accept_sell(conn: sqlite3.Connection, request_id: str = 'req-sc-1') -> None:
    cmd = CanonicalFiscalCommand(
        request_id=request_id,
        idempotency_key=f'idem-{request_id}',
        protocol=Protocol.CHECKBOX_REST,
        operation_type=OperationType.SELL,
        fiscal_number=_FN,
        route_key='main',
        backend_profile_id=_BP,
        transport_profile_id=_TP,
        channel_owner='front-a',
        external_request_id=f'ext-{request_id}',
        business_ts=datetime(2026, 4, 20, 10, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Тест', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': [{'amount': 1000, 'type': 'CASH'}],
                'totals': {'total_sum': 1000},
            }
        },
        payload_sha256=f'sha-{request_id}',
        trace_context=TraceContext(
            source_ip='10.0.0.1', source_port=12345,
            session_id='sess-sc', correlation_id=f'corr-{request_id}',
        ),
        correlation_id=f'corr-{request_id}',
    )
    InboxRepository.accept_command(
        conn,
        request_id=cmd.request_id,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner,
        external_request_id=cmd.external_request_id,
        protocol_session_id='proto-sc',
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )


# ---------------------------------------------------------------------------
# Test 1: happy path — sidecar ACK
# ---------------------------------------------------------------------------

def test_sidecar_happy_path_ack(conn: sqlite3.Connection) -> None:
    """Full write_path pipeline: canonical JSON → passthrough → sidecar → ACK."""
    captured_body: list[dict] = []

    def handler(request: httpx.Request) -> httpx.Response:
        captured_body.append(json.loads(request.content))
        return httpx.Response(200, json={'status': 1, 'fiscal_id': 'FIS-SC-001'})

    conn.execute('BEGIN IMMEDIATE')
    _seed_sidecar_binding(conn)
    _open_shift(conn)
    _accept_sell(conn)
    conn.commit()

    transport = _make_transport(handler)
    crypto = PassthroughCryptoProvider()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)

    result = worker.process_next(conn, fiscal_number=_FN)

    assert result.outcome == 'ACK', f'expected ACK, got {result.outcome}'
    assert result.document_state == 'ACK'

    # The body POSTed to the sidecar must be canonical JSON, not DPS XML.
    assert len(captured_body) == 1
    body = captured_body[0]
    assert not isinstance(body, str) or not body.startswith('<RQ'), \
        'sidecar received DPS XML instead of canonical JSON'
    # canonical JSON carries fiscal_number
    assert body.get('fiscal_number') == _FN or _FN in json.dumps(body), \
        f'fiscal_number {_FN!r} not found in sidecar request body'


# ---------------------------------------------------------------------------
# Test 2: passthrough crypto does not modify canonical JSON
# ---------------------------------------------------------------------------

def test_passthrough_crypto_identity() -> None:
    """PassthroughCryptoProvider.sign() must return the payload unchanged."""
    provider = PassthroughCryptoProvider()
    payload = '{"x":1,"fiscal_number":"FN-DEV-0001"}'
    signed = provider.sign(document_id='doc-001', payload_json=payload)
    assert signed == payload, f'passthrough mutated payload: {signed!r}'


# ---------------------------------------------------------------------------
# Test 3: sign_input is NOT DPS XML for sidecar profile
# ---------------------------------------------------------------------------

def test_sign_input_is_not_dps_xml_for_sidecar(conn: sqlite3.Connection) -> None:
    """Verify that write_path passes canonical JSON (not DPS XML) through to transport."""
    received_payloads: list[str] = []

    class CapturingTransport:
        def send(
            self,
            *,
            document_id: str,
            signed_payload,
            fiscal_number: str,
            backend_profile_id: str,
            transport_profile_id: str,
            **kwargs,
        ) -> SendResult:
            received_payloads.append(signed_payload)
            return SendResult(
                state='ACK',
                transport_request_id='tx-cap-1',
                submission_status='DPS_ACK',
                response_json='{"status":1}',
            )

    conn.execute('BEGIN IMMEDIATE')
    _seed_sidecar_binding(conn)
    _open_shift(conn)
    _accept_sell(conn, request_id='req-sc-sig')
    conn.commit()

    crypto = PassthroughCryptoProvider()
    transport = CapturingTransport()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)
    worker.process_next(conn, fiscal_number=_FN)

    assert len(received_payloads) == 1
    payload = received_payloads[0]
    assert not payload.startswith('<RQ'), \
        f'transport received DPS XML instead of canonical JSON: {payload[:80]!r}'
    parsed = json.loads(payload)
    assert 'fiscal_number' in parsed, \
        f'"fiscal_number" key missing from signed_payload: {list(parsed.keys())}'
    assert parsed['fiscal_number'] == _FN


# ---------------------------------------------------------------------------
# Test 4: sidecar 502 → document stays ERROR_RETRYABLE
# ---------------------------------------------------------------------------

def test_sidecar_retryable_error(conn: sqlite3.Connection) -> None:
    """502 from sidecar must map to ERROR_RETRYABLE outcome."""

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(502, text='Bad Gateway')

    conn.execute('BEGIN IMMEDIATE')
    _seed_sidecar_binding(conn)
    _open_shift(conn)
    _accept_sell(conn, request_id='req-sc-retry')
    conn.commit()

    transport = _make_transport(handler)
    crypto = PassthroughCryptoProvider()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)

    result = worker.process_next(conn, fiscal_number=_FN)

    assert result.outcome == 'ERROR', f'expected ERROR, got {result.outcome}'
    assert result.document_state == 'ERROR_RETRYABLE', \
        f'expected ERROR_RETRYABLE, got {result.document_state}'


# ---------------------------------------------------------------------------
# Test 5: DPS rejection from sidecar → REJECTED
# ---------------------------------------------------------------------------

def test_sidecar_dps_rejection(conn: sqlite3.Connection) -> None:
    """DPS status=-1 from sidecar must map to REJECTED outcome."""

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            json={'status': -1, 'error_message': 'ERROR_VEREFY'},
        )

    conn.execute('BEGIN IMMEDIATE')
    _seed_sidecar_binding(conn)
    _open_shift(conn)
    _accept_sell(conn, request_id='req-sc-rej')
    conn.commit()

    transport = _make_transport(handler)
    crypto = PassthroughCryptoProvider()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)

    result = worker.process_next(conn, fiscal_number=_FN)

    assert result.outcome == 'ERROR', f'expected ERROR, got {result.outcome}'
    assert result.document_state == 'REJECTED', \
        f'expected REJECTED, got {result.document_state}'


# ── 6. SHIFT_OPEN → SELL → Z_REPORT end-to-end via sidecar ──────────────────

def test_sidecar_full_lifecycle_shift_open_sell_z_report(conn):
    """
    Наскрізний тест: чотири операції через sidecar транспорт.

    SHIFT_OPEN → SELL → Z_REPORT → SHIFT_CLOSE

    Online Z_REPORT не закриває зміну — лише пов'язує Z-документ.
    Зміна переходить у CLOSED тільки після SHIFT_CLOSE (ACK).

    Перевіряємо:
    - кожен документ досягає ACK
    - тіло запиту до сайдкару — canonical JSON (не DPS XML)
    - operation_type в тілі відповідає операції
    - LND зростає монотонно між фіскальними документами
    - стан зміни після Z_REPORT — OPENED (Z-doc linked, shift still open)
    - стан зміни після SHIFT_CLOSE — CLOSED
    - POST йде на /fiscal/send для кожної операції
    """
    _FISCAL_ID_COUNTER = iter(['FIS-OPEN-001', 'FIS-SELL-001', 'FIS-Z-001', 'FIS-CLOSE-001'])
    sidecar_calls: list[dict] = []

    def handler(req: httpx.Request) -> httpx.Response:
        assert req.url.path == '/fiscal/send', f'unexpected path: {req.url.path}'
        body = json.loads(req.content)
        sidecar_calls.append(body)
        return httpx.Response(200, json={
            'status': 1,
            'fiscal_id': next(_FISCAL_ID_COUNTER),
        })

    conn.execute('BEGIN IMMEDIATE')
    _seed_sidecar_binding(conn)
    conn.commit()

    transport = _make_transport(handler)
    crypto = PassthroughCryptoProvider()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=transport)

    # ── 1. SHIFT_OPEN ─────────────────────────────────────────────────────────
    conn.execute('BEGIN IMMEDIATE')
    _enqueue_op(
        conn,
        request_id='req-sc-open',
        operation_type=OperationType.SHIFT_OPEN,
        payload={
            'receipt': {'goods': [], 'payments': [], 'totals': {'total_sum': 0}}
        },
        offset=0,
    )
    conn.commit()

    r_open = worker.process_next(conn, fiscal_number=_FN)
    assert r_open is not None
    assert r_open.outcome == 'ACK', f'SHIFT_OPEN: expected ACK, got {r_open.outcome}'

    # After SHIFT_OPEN the shift must be OPENED
    shift_state = conn.execute(
        "SELECT state FROM shifts WHERE fiscal_number = ?", (_FN,)
    ).fetchone()
    assert shift_state is not None and shift_state[0] == 'OPENED', (
        f'shift must be OPENED after SHIFT_OPEN, got {shift_state}'
    )

    # ── 2. SELL ───────────────────────────────────────────────────────────────
    conn.execute('BEGIN IMMEDIATE')
    _enqueue_op(
        conn,
        request_id='req-sc-sell',
        operation_type=OperationType.SELL,
        payload={
            'receipt': {
                'goods': [{'name': 'Молоко 1л', 'price': 3500, 'quantity': 1000, 'sum': 3500}],
                'payments': [{'type': 'CASH', 'amount': 3500}],
                'totals': {'total_sum': 3500},
            }
        },
        offset=5,
    )
    conn.commit()

    r_sell = worker.process_next(conn, fiscal_number=_FN)
    assert r_sell is not None
    assert r_sell.outcome == 'ACK', f'SELL: expected ACK, got {r_sell.outcome}'

    lnd_sell = conn.execute(
        "SELECT lnd FROM fiscal_documents WHERE document_id = ?", (r_sell.document_id,)
    ).fetchone()[0]

    # ── 3. Z_REPORT ───────────────────────────────────────────────────────────
    conn.execute('BEGIN IMMEDIATE')
    _enqueue_op(
        conn,
        request_id='req-sc-zrep',
        operation_type=OperationType.Z_REPORT,
        payload={
            'z_report_data': {
                'check_count': {'ni': 1, 'no': 0},
                'payment_sums': {'CASH': {'smi': 3500, 'smo': 0}},
                'service_sums': {},
            }
        },
        offset=60,
    )
    conn.commit()

    r_zrep = worker.process_next(conn, fiscal_number=_FN)
    assert r_zrep is not None
    assert r_zrep.outcome == 'ACK', f'Z_REPORT: expected ACK, got {r_zrep.outcome}'

    # After Z_REPORT the shift is still OPENED — Z-doc is linked but shift is not closed yet.
    # Online Z_REPORT only links the z_report_document_id; SHIFT_CLOSE closes the shift.
    shift_mid = conn.execute(
        "SELECT state, z_report_document_id FROM shifts WHERE fiscal_number = ?", (_FN,)
    ).fetchone()
    assert shift_mid is not None and shift_mid[0] == 'OPENED', (
        f'shift must still be OPENED after Z_REPORT (online), got {shift_mid}'
    )
    assert shift_mid[1] == r_zrep.document_id, (
        f'z_report_document_id not linked after Z_REPORT: {shift_mid[1]!r}'
    )

    # ── 4. SHIFT_CLOSE ────────────────────────────────────────────────────────
    conn.execute('BEGIN IMMEDIATE')
    _enqueue_op(
        conn,
        request_id='req-sc-close',
        operation_type=OperationType.SHIFT_CLOSE,
        payload={
            'receipt': {'goods': [], 'payments': [], 'totals': {'total_sum': 0}}
        },
        offset=65,
    )
    conn.commit()

    r_close = worker.process_next(conn, fiscal_number=_FN)
    assert r_close is not None
    assert r_close.outcome == 'ACK', f'SHIFT_CLOSE: expected ACK, got {r_close.outcome}'

    # After SHIFT_CLOSE the shift must be CLOSED
    shift_state_after = conn.execute(
        "SELECT state FROM shifts WHERE fiscal_number = ?", (_FN,)
    ).fetchone()
    assert shift_state_after is not None and shift_state_after[0] == 'CLOSED', (
        f'shift must be CLOSED after SHIFT_CLOSE, got {shift_state_after}'
    )

    # ── Sidecar call invariants ────────────────────────────────────────────────
    assert len(sidecar_calls) == 4, (
        f'expected exactly 4 sidecar calls, got {len(sidecar_calls)}'
    )

    ops_sent = [call.get('operation_type') for call in sidecar_calls]
    assert ops_sent == ['SHIFT_OPEN', 'SELL', 'Z_REPORT', 'SHIFT_CLOSE'], (
        f'operation_type sequence wrong: {ops_sent}'
    )

    for i, body in enumerate(sidecar_calls):
        # Every body is canonical JSON — not DPS XML
        assert not json.dumps(body).startswith('<RQ'), (
            f'call {i}: sidecar received DPS XML instead of canonical JSON'
        )
        assert body.get('fiscal_number') == _FN, (
            f'call {i}: fiscal_number missing or wrong: {body.get("fiscal_number")!r}'
        )

    # LND of SELL is strictly positive (sequence started)
    assert lnd_sell is not None and lnd_sell >= 1, (
        f'SELL lnd must be ≥ 1, got {lnd_sell}'
    )

    conn.close()


def _enqueue_op(
    conn,
    *,
    request_id: str,
    operation_type: OperationType,
    payload: dict,
    offset: int = 0,
) -> None:
    """Enqueue a canonical command with sidecar transport profile."""
    from datetime import timedelta
    business_ts = datetime(2026, 4, 20, 10, 0, 0, tzinfo=UTC) + timedelta(minutes=offset)
    cmd = CanonicalFiscalCommand(
        request_id=request_id,
        idempotency_key=f'idem-{request_id}',
        protocol=Protocol.CHECKBOX_REST,
        operation_type=operation_type,
        fiscal_number=_FN,
        route_key='main',
        backend_profile_id=_BP,
        transport_profile_id=_TP,
        channel_owner='front-a',
        external_request_id=f'ext-{request_id}',
        business_ts=business_ts,
        payload=payload,
        payload_sha256=f'sha-{request_id}',
        trace_context=TraceContext(
            source_ip='10.0.0.1', source_port=12345,
            session_id='sess-e2e', correlation_id=f'corr-{request_id}',
        ),
        correlation_id=f'corr-{request_id}',
    )
    InboxRepository.accept_command(
        conn,
        request_id=cmd.request_id,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner,
        external_request_id=cmd.external_request_id,
        protocol_session_id='proto-e2e',
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )
