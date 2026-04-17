# tests/test_invariant1_no_crypto_in_tx.py
"""Frozen Invariant 1: crypto.sign() and transport.send() must be called outside SQLite tx."""
from __future__ import annotations
import sqlite3
from datetime import UTC, datetime
from prro_gateway.enums import OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json

FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'
_seq = 0

def _nid(p):
    global _seq; _seq += 1; return f'{p}-inv1-{_seq}'


def test_invariant1_crypto_and_transport_outside_transaction(conn: sqlite3.Connection) -> None:
    """crypto.sign() and transport.send() must be called when conn.in_transaction is False."""
    crypto_in_tx = []
    transport_in_tx = []

    class _TracingCrypto:
        def sign(self, *, document_id, payload_json):
            crypto_in_tx.append(conn.in_transaction)
            return f'sig::{document_id}'
        def sign_raw(self, *, data, document_id=None):
            crypto_in_tx.append(conn.in_transaction)
            return b'signed'

    class _TracingTransport:
        def send(self, **kw):
            transport_in_tx.append(conn.in_transaction)
            from prro_gateway.ports import SendResult
            now = datetime.now(UTC)
            return SendResult(transport_request_id='tx', submission_status='ACK',
                server_fiscal_no='SFN', server_fiscal_date=now.isoformat(),
                response_json='{}', sent_at=now, ack_at=now)

    worker = WritePathWorker(
        crypto_provider=_TracingCrypto(),
        transport_client=_TracingTransport(),
        crypto_breaker_threshold=0,
    )
    # Setup: open shift
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(conn, shift_id=_nid('shift'), fiscal_number=FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST, integration_owner='front-a',
        channel_lock_acquired_at='2026-04-17T10:00:00+00:00')
    conn.commit()
    # Enqueue SELL
    rid = _nid('req')
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=_nid('idem'),
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number=FN, route_key='pos-1',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        channel_owner='front-a', external_request_id=_nid('ext'),
        business_ts=datetime(2026, 4, 17, 10, 0, 0, tzinfo=UTC),
        payload={'receipt': {'payments': [{'type': 'CASH', 'value': 100}],
                             'goods': [{'name': 'Item', 'price': 100, 'quantity': 1000}]}},
        payload_sha256=_nid('sha'),
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(conn, request_id=rid,
        idempotency_key=cmd.idempotency_key, protocol=cmd.protocol,
        operation_type=cmd.operation_type, fiscal_number=FN,
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        channel_owner='front-a', external_request_id=cmd.external_request_id,
        protocol_session_id=None,
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256)
    conn.commit()
    result = worker.process_next(conn, fiscal_number=FN)
    assert result.outcome == 'ACK', f"Sell must succeed: {result.canonical_error}"
    assert crypto_in_tx, "crypto.sign() was never called"
    assert all(not in_tx for in_tx in crypto_in_tx), (
        f"crypto.sign() called INSIDE transaction: {crypto_in_tx}"
    )
    assert transport_in_tx, "transport.send() was never called"
    assert all(not in_tx for in_tx in transport_in_tx), (
        f"transport.send() called INSIDE transaction: {transport_in_tx}"
    )
